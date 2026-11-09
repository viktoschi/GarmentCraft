//! Step 4b: the implicit step's system build and CG solve on the GPU (`wgpu`,
//! `f32`).
//!
//! **The system is built on the device.** The connectivity - which points an
//! edge joins, which four points a bending angle uses, the neighbour lists - is
//! the same in every substep; only the positions change. The caller therefore
//! hands over topology, stiffnesses and rest lengths
//! ([`System::ohne_geometrie`]), and three passes compute what the CPU build
//! used to upload: direction, cross factor and force per distance constraint
//! (`aufbau_k`), the dihedral gradients per bending constraint (`aufbau_b`), and
//! per point the force, the physical load, `H·v`, the right-hand side and the
//! inverted 3×3 diagonal block (`sammeln`). Per constraint one writer, per
//! point one gather over the neighbour list - no assembly, no atomics. Topology
//! and stiffnesses are uploaded only when they change (seam rest lengths do,
//! while a garment is sewn); per substep only positions, velocities, the warm
//! start and the contact set go up.
//!
//! **Positions go up in two parts**, `x = hi + lo` with `hi = f32(x)` and
//! `lo = f32(x − hi)`. A constraint needs `x_j − x_i`, and on a body 1.5 m tall a
//! single `f32` position is only good to about 0.1 µm; the force of a 10 mm edge
//! at 1 % strain depends on `l − l₀ = 0.1 mm` and would carry a relative error of
//! 1e-3. Taking the differences of both parts separately gives the difference
//! itself to `f32` precision (`tests/gpu.rs`, `aufbau_wie_cpu`).
//!
//! **The operator runs over the whole card, the rest on one workgroup.** `A·p`
//! has no dependency between points, so it runs over the whole grid - one
//! workgroup per 256 points - and is about 15 times faster that way on the
//! blouse (0.036 against 0.57 ms per pass, measured by
//! `crates/klon3d-sim/examples/gpu_takt.rs`). What needs the reductions - the
//! warm start, α and β with their dot products, the preconditioner, the vector
//! updates - stays on one workgroup of 1024 lanes.
//!
//! **One submission per solve.** The build passes and all passes of a solve are
//! recorded into one command buffer and submitted once; the pass boundary is
//! the global barrier between them and costs about 6 µs, no round trip.
//!
//! **The CG state lives in the data buffer**, not in workgroup memory: it has
//! to survive the pass boundaries. Eight scalar slots next to the vectors hold
//! δ, ‖b‖², the iteration count, the target, the "converged" flag and α, β.
//! Once the flag is set, the remaining operator passes return immediately.
//!
//! **Recorded iterations follow the previous solve.** A pass that returns at
//! once still costs its dispatch, so recording `cg_max` passes would waste
//! ~30 ms per solve on a converged system; the solver records about what the
//! last solve needed and submits another batch if that was not enough.
//!
//! What stays on the CPU: contact distance queries, the penetration push,
//! friction, the position update and self-collision.
//!
//! **No springs.** A step whose system carries springs (glue zones or
//! self-collision) is built in full on the CPU and solved by the CPU backend,
//! and counted ([`GpuLoeser::ausweichungen`]) instead of silently dropping them.
//!
//! **f32 on purpose** (ADR-018). How far `f32` is from the `f64` reference is
//! measured, not assumed: `crates/klon3d-sim/tests/gpu.rs` compares every
//! record of the build, `A·p` and the state after the same number of frames.
//!
//! **Iteration budget.** The CPU stops at `‖r‖ ≤ tol·‖b‖`; the GPU stops at the
//! same criterion, checked in the reduction pass between iterations, so the
//! count is the same as the CPU's.

use crate::implizit::{CgLauf, CpuLoeser, Fest, Loeser, System, M3};

/// Offsets of the vector segments inside the data buffer, in `vec4` units.
#[derive(Debug, Clone, Copy)]
struct Lage {
    dv: u32,
    r: u32,
    z: u32,
    dir: u32,
    ap: u32,
    p: u32,
    b: u32,
    v: u32,
    /// Positions, high part `f32(x)`.
    xh: u32,
    /// Positions, low part `f32(x − hi)`.
    xl: u32,
    /// Physical load (tension only, plus weight), for friction.
    last: u32,
    /// Force per point.
    f: u32,
    /// `H·v`.
    hv: u32,
    fb: u32,
    zfb: u32,
    sk: u32,
    ende: u32,
}

impl Lage {
    fn neu(n: usize) -> Lage {
        let n32 = n as u32;
        let mut o = 0;
        let mut naechste = |len: u32| {
            let hier = o;
            o += len;
            hier
        };
        Lage {
            dv: naechste(n32),
            r: naechste(n32),
            z: naechste(n32),
            dir: naechste(n32),
            ap: naechste(n32),
            p: naechste(n32),
            b: naechste(n32),
            v: naechste(n32),
            xh: naechste(n32),
            xl: naechste(n32),
            last: naechste(n32),
            f: naechste(n32),
            hv: naechste(n32),
            fb: naechste(n32),
            zfb: naechste(n32),
            sk: naechste(8),
            ende: o,
        }
    }
}

/// The uniform block; mirrors `Params` in the shader.
///
/// Scalars only, no `vec3`: a `vec3<f32>` in a uniform has align 16 while
/// `[f32; 3]` in Rust has align 4, and the two structures would drift apart
/// silently. 22 `u32`, 8 `f32`, 2 of padding: 128 bytes.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    /// Distance constraints of pattern edges; seams follow at this index.
    kanten: u32,
    naehte: u32,
    biege: u32,
    modus: u32,
    cg_max: u32,
    l_dv: u32,
    l_r: u32,
    l_z: u32,
    l_dir: u32,
    l_ap: u32,
    l_b: u32,
    l_v: u32,
    l_xh: u32,
    l_xl: u32,
    l_last: u32,
    l_f: u32,
    l_hv: u32,
    l_fb: u32,
    l_zfb: u32,
    l_sk: u32,
    l_p: u32,
    /// Substep length, air damping and `h²`.
    h: f32,
    c: f32,
    h2: f32,
    tol: f32,
    boden: f32,
    /// Gravity (m/s²).
    gx: f32,
    gy: f32,
    gz: f32,
    _pad: [f32; 2],
}

const WGSL: &str = r#"
struct Params {
    n: u32,
    kanten: u32,
    naehte: u32,
    biege: u32,
    modus: u32,
    cg_max: u32,
    l_dv: u32,
    l_r: u32,
    l_z: u32,
    l_dir: u32,
    l_ap: u32,
    l_b: u32,
    l_v: u32,
    l_xh: u32,
    l_xl: u32,
    l_last: u32,
    l_f: u32,
    l_hv: u32,
    l_fb: u32,
    l_zfb: u32,
    l_sk: u32,
    l_p: u32,
    h: f32,
    c: f32,
    h2: f32,
    tol: f32,
    boden: f32,
    gx: f32,
    gy: f32,
    gz: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> par: Params;
// mass·(1 + h·c), inverse mass, held flag, required normal velocity
@group(0) @binding(1) var<storage, read_write> kopf: array<vec4<f32>>;
// contact normal, and the mass in w (the gravity and damping force needs it)
@group(0) @binding(2) var<storage, read_write> normale: array<vec4<f32>>;
// inverse of the 3x3 diagonal block, three rows per point
@group(0) @binding(3) var<storage, read_write> pinv: array<vec4<f32>>;
// dv, r, z, dir, ap, p, b, v, xh, xl, last, f, hv, fb, zfb, then eight scalars
@group(0) @binding(4) var<storage, read_write> daten: array<vec4<f32>>;
// n+1 neighbour-list starts, then four u32 per entry: kind, index, slot, 0
@group(0) @binding(5) var<storage, read_write> liste: array<u32>;
// one vec4 per distance constraint, edges first, then seams: (i, j, k, rest);
// rest < 0 marks a seam that is not sewn yet
@group(0) @binding(6) var<storage, read_write> kante: array<vec4<f32>>;
// two vec4 per bending constraint: (i0, i1, i2, i3), (k, rest angle, 0, 0)
@group(0) @binding(7) var<storage, read_write> biege: array<vec4<f32>>;
// written by the build: two vec4 per distance constraint, (direction i->j,
// cross factor) and (force on i, 1 if it counts as load), then five per bending
// constraint, the four gradients and (angle - rest, 0, 0, 0)
@group(0) @binding(8) var<storage, read_write> geo: array<vec4<f32>>;

var<workgroup> red_a: array<f32, 1024>;
var<workgroup> red_b: array<f32, 1024>;

fn lies(off: u32, i: u32) -> vec3<f32> {
    return daten[off + i].xyz;
}

fn schreib(off: u32, i: u32, v: vec3<f32>) {
    daten[off + i] = vec4<f32>(v, 0.0);
}

fn anf(i: u32) -> u32 {
    return liste[i];
}

fn eintrag(i: u32, feld: u32) -> u32 {
    return liste[par.n + 1u + 4u * i + feld];
}

fn geo_biegung(c: u32) -> u32 {
    return 2u * (par.kanten + par.naehte) + 5u * c;
}

// x_j - x_i from both parts of the positions: the parts are subtracted
// separately, so the difference keeps f32 precision of its own size.
fn abst(i: u32, j: u32) -> vec3<f32> {
    return (lies(par.l_xh, j) - lies(par.l_xh, i)) + (lies(par.l_xl, j) - lies(par.l_xl, i));
}

fn filtere(i: u32, v: vec3<f32>) -> vec3<f32> {
    let k = kopf[i];
    if (k.y <= 0.0) {
        return vec3<f32>(0.0);
    }
    if (k.z > 0.5) {
        let nl = normale[i].xyz;
        return v - nl * dot(v, nl);
    }
    return v;
}

// ap = M(1 + h c)·p + h^2·H·p gathered over the point's entries.
// `gefiltert`: the contact filter is applied (what the CG iterates on).
fn punkt(i: u32, off: u32, gefiltert: bool) {
    let pi = lies(off, i);
    var sum = vec3<f32>(0.0);
    let von = anf(i);
    let bis = anf(i + 1u);
    for (var e = von; e < bis; e = e + 1u) {
        let art = eintrag(e, 0u);
        let c = eintrag(e, 1u);
        if (art == 0u) {
            let a = kante[c];
            let g = geo[2u * c];
            let ist_i = u32(a.x) == i;
            let j = select(u32(a.x), u32(a.y), ist_i);
            // d ist schon p_selbst - p_partner; der Block ist auf beiden
            // Seiten derselbe, ein Vorzeichenwechsel wäre hier falsch.
            let d = pi - lies(off, j);
            let nr = g.xyz;
            sum = sum + a.z * (nr * dot(nr, d) * (1.0 - g.w) + d * g.w);
        } else {
            let basis = geo_biegung(c);
            let q = biege[2u * c];
            let idx = vec4<u32>(u32(q.x), u32(q.y), u32(q.z), u32(q.w));
            var gp = dot(geo[basis].xyz, lies(off, idx.x));
            gp = gp + dot(geo[basis + 1u].xyz, lies(off, idx.y));
            gp = gp + dot(geo[basis + 2u].xyz, lies(off, idx.z));
            gp = gp + dot(geo[basis + 3u].xyz, lies(off, idx.w));
            let m = eintrag(e, 2u);
            sum = sum + biege[2u * c + 1u].x * geo[basis + m].xyz * gp;
        }
    }
    var aus = sum * par.h2 + kopf[i].x * pi;
    if (gefiltert) {
        aus = filtere(i, aus);
    }
    schreib(par.l_ap, i, aus);
}

// --- the system build ---------------------------------------------------------
//
// The same arithmetic as the CPU build (`implizit::system_bauen`), so the two
// can be compared record by record.

/// Direction, cross factor and force of every distance constraint.
@compute @workgroup_size(256)
fn aufbau_k(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = nwg.x * 256u;
    let gesamt = par.kanten + par.naehte;
    for (var c = gid.x; c < gesamt; c = c + stride) {
        let a = kante[c];
        let d = abst(u32(a.x), u32(a.y));
        let k = a.z;
        var dir = vec3<f32>(0.0);
        var quer = 1.0;
        var fi = vec3<f32>(0.0);
        var zug = 0.0;
        if (c < par.kanten) {
            // Pattern edge: always a direction, the load counts under tension.
            let l = max(length(d), 1e-12);
            dir = d / l;
            fi = dir * (k * (l - a.w));
            quer = max(1.0 - a.w / l, 0.0);
            zug = select(0.0, 1.0, l > a.w);
        } else {
            // Seam: open (rest < 0) means the current length is the rest
            // length; length 0 gives k·I.
            let l = length(d);
            let l0 = select(a.w, l, a.w < 0.0);
            if (l0 > 0.0 && l > 1e-12) {
                dir = d / l;
                fi = d * (k * (l - l0) / l);
                quer = max(1.0 - l0 / l, 0.0);
            } else {
                fi = d * k;
            }
        }
        geo[2u * c] = vec4<f32>(dir, quer);
        geo[2u * c + 1u] = vec4<f32>(fi, zug);
    }
}

/// Dihedral angle and its four gradients, per bending constraint (Bridson et
/// al. 2003, as `cpu::dieder`), from positions relative to the first point.
@compute @workgroup_size(256)
fn aufbau_b(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = nwg.x * 256u;
    for (var c = gid.x; c < par.biege; c = c + stride) {
        let q = biege[2u * c];
        let i0 = u32(q.x);
        let x1 = abst(i0, u32(q.y));
        let x2 = abst(i0, u32(q.z));
        let x3 = abst(i0, u32(q.w));
        let basis = geo_biegung(c);
        let e = x1;
        let le = length(e);
        let n1 = cross(x2, x2 - x1);
        let n2 = cross(x3 - x1, x3);
        let q1 = dot(n1, n1);
        let q2 = dot(n2, n2);
        if (le < 1e-12 || q1 < 1e-30 || q2 < 1e-30) {
            // The angle does not exist: no force, no stiffness. The constraint
            // keeps its slot.
            for (var m = 0u; m < 5u; m = m + 1u) {
                geo[basis + m] = vec4<f32>(0.0);
            }
            continue;
        }
        let m1 = n1 / sqrt(q1);
        let m2 = n2 / sqrt(q2);
        let theta = atan2(dot(cross(m1, m2), e / le), dot(m1, m2));
        let g2 = n1 * (-le / q1);
        let g3 = n2 * (-le / q2);
        let g0 = -(n1 * (dot(x2 - x1, e) / le / q1) + n2 * (dot(x3 - x1, e) / le / q2));
        let g1 = n1 * (dot(x2, e) / le / q1) + n2 * (dot(x3, e) / le / q2);
        geo[basis] = vec4<f32>(g0, 0.0);
        geo[basis + 1u] = vec4<f32>(g1, 0.0);
        geo[basis + 2u] = vec4<f32>(g2, 0.0);
        geo[basis + 3u] = vec4<f32>(g3, 0.0);
        geo[basis + 4u] = vec4<f32>(winkel_fehler(theta, biege[2u * c + 1u].y), 0.0, 0.0, 0.0);
    }
}

/// The angle difference the short way round the circle: a dihedral angle only
/// exists modulo 2π, and a fold's rest angle of ±π sits exactly on the branch cut
/// of `atan2` (see `cpu::winkel_fehler`).
fn winkel_fehler(theta: f32, ruhe: f32) -> f32 {
    let d = theta - ruhe;
    return d - 6.283185307179586 * round(d / 6.283185307179586);
}

fn aussen(a: vec3<f32>, b: vec3<f32>) -> mat3x3<f32> {
    return mat3x3<f32>(a * b.x, a * b.y, a * b.z);
}

fn skalar(s: f32) -> mat3x3<f32> {
    return mat3x3<f32>(vec3<f32>(s, 0.0, 0.0), vec3<f32>(0.0, s, 0.0), vec3<f32>(0.0, 0.0, s));
}

/// Inverts the 3x3 block and writes it as three rows, like `implizit::m3_inv`.
/// A singular block becomes zero.
fn schreib_inverse(i: u32, m: mat3x3<f32>) {
    // m[col][row]; named row by row.
    let a = m[0][0];
    let b = m[1][0];
    let c = m[2][0];
    let d = m[0][1];
    let e = m[1][1];
    let f = m[2][1];
    let g = m[0][2];
    let h = m[1][2];
    let k = m[2][2];
    let det = a * (e * k - f * h) - b * (d * k - f * g) + c * (d * h - e * g);
    var s = 0.0;
    if (abs(det) > 0.0) {
        s = 1.0 / det;
    }
    pinv[3u * i] = vec4<f32>((e * k - f * h) * s, (c * h - b * k) * s, (b * f - c * e) * s, 0.0);
    pinv[3u * i + 1u] = vec4<f32>((f * g - d * k) * s, (a * k - c * g) * s, (c * d - a * f) * s, 0.0);
    pinv[3u * i + 2u] = vec4<f32>((d * h - e * g) * s, (b * g - a * h) * s, (a * e - b * d) * s, 0.0);
}

/// Force, physical load, `H·v`, right-hand side and the inverted diagonal
/// block, per point, gathered over the neighbour list in the scene's order.
@compute @workgroup_size(256)
fn sammeln(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = nwg.x * 256u;
    let g = vec3<f32>(par.gx, par.gy, par.gz);
    for (var i = gid.x; i < par.n; i = i + stride) {
        let masse = normale[i].w;
        let vi = lies(par.l_v, i);
        var f = g * masse - vi * (par.c * masse);
        var last = g * masse;
        var hv = vec3<f32>(0.0);
        var dg = skalar(kopf[i].x);
        let von = anf(i);
        let bis = anf(i + 1u);
        for (var e = von; e < bis; e = e + 1u) {
            let art = eintrag(e, 0u);
            let c = eintrag(e, 1u);
            if (art == 0u) {
                let a = kante[c];
                let gk = geo[2u * c];
                let gf = geo[2u * c + 1u];
                let ist_i = u32(a.x) == i;
                let j = select(u32(a.x), u32(a.y), ist_i);
                let vz = select(-1.0, 1.0, ist_i);
                f = f + gf.xyz * vz;
                if (gf.w > 0.5) {
                    last = last + gf.xyz * vz;
                }
                let dv = vi - lies(par.l_v, j);
                let nr = gk.xyz;
                hv = hv + a.z * (nr * dot(nr, dv) * (1.0 - gk.w) + dv * gk.w);
                let s = a.z * par.h2;
                dg = dg + skalar(s * gk.w) + aussen(nr, nr) * (s * (1.0 - gk.w));
            } else {
                let basis = geo_biegung(c);
                let q = biege[2u * c];
                let k = biege[2u * c + 1u].x;
                let gr = geo[basis + eintrag(e, 2u)].xyz;
                f = f - gr * (k * geo[basis + 4u].x);
                var gp = dot(geo[basis].xyz, lies(par.l_v, u32(q.x)));
                gp = gp + dot(geo[basis + 1u].xyz, lies(par.l_v, u32(q.y)));
                gp = gp + dot(geo[basis + 2u].xyz, lies(par.l_v, u32(q.z)));
                gp = gp + dot(geo[basis + 3u].xyz, lies(par.l_v, u32(q.w)));
                hv = hv + gr * (k * gp);
                dg = dg + aussen(gr, gr) * (k * par.h2);
            }
        }
        schreib_inverse(i, dg);
        schreib(par.l_f, i, f);
        schreib(par.l_last, i, last);
        schreib(par.l_hv, i, hv);
        // b = h·(f − h·H·v)
        schreib(par.l_b, i, (f - hv * par.h) * par.h);
    }
}

// z = P^-1·v, written to `off`.
fn vorkond(tid: u32, von: u32, nach: u32) {
    for (var i = tid; i < par.n; i = i + 1024u) {
        let v = lies(von, i);
        let p0 = pinv[3u * i];
        let p1 = pinv[3u * i + 1u];
        let p2 = pinv[3u * i + 2u];
        schreib(nach, i, vec3<f32>(dot(p0.xyz, v), dot(p1.xyz, v), dot(p2.xyz, v)));
    }
}

fn summe(v: f32, tid: u32, zweite: bool) -> f32 {
    if (zweite) {
        red_b[tid] = v;
    } else {
        red_a[tid] = v;
    }
    workgroupBarrier();
    var s = 512u;
    loop {
        if (s == 0u) {
            break;
        }
        if (tid < s) {
            if (zweite) {
                red_b[tid] = red_b[tid] + red_b[tid + s];
            } else {
                red_a[tid] = red_a[tid] + red_a[tid + s];
            }
        }
        workgroupBarrier();
        s = s / 2u;
    }
    if (zweite) {
        return red_b[0];
    }
    return red_a[0];
}

// --- the solve, one pass per phase ------------------------------------------
//
// The operator runs over the whole grid (0.042 ms on the blouse instead of
// 0.372 ms on one workgroup); everything else runs on one workgroup, because
// it needs the reductions. The CG state lives in the eight scalar slots of the
// data buffer, so it survives the pass boundaries; those are recorded into one
// submission and cost 6 µs each, no round trip.

// Scalar slot `i` of the solve state.
fn sk(i: u32) -> f32 {
    return daten[par.l_sk + i].x;
}

fn setz_sk(i: u32, v: f32) {
    daten[par.l_sk + i] = vec4<f32>(v, 0.0, 0.0, 0.0);
}

/// ap = A·dv for the warm start: unfiltered, before the first direction
/// exists.
@compute @workgroup_size(256)
fn op_warm(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = nwg.x * 256u;
    for (var i = gid.x; i < par.n; i = i + stride) {
        punkt(i, par.l_dv, false);
    }
}

/// ap = A·dir for the current iteration, with the contact filter. Returns at
/// once once the solve has converged, so the remaining passes are free.
@compute @workgroup_size(256)
fn op_dir(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    if (sk(4u) > 0.5) {
        return;
    }
    let stride = nwg.x * 256u;
    for (var i = gid.x; i < par.n; i = i + stride) {
        punkt(i, par.l_dir, true);
    }
}

/// A·p for the acceptance test and the benchmark: the input vector is in `p`.
@compute @workgroup_size(256)
fn op_roh(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    let stride = nwg.x * 256u;
    for (var i = gid.x; i < par.n; i = i + stride) {
        punkt(i, par.l_p, false);
    }
}

/// Warm start, filtered right-hand side, preconditioned `fb`.
@compute @workgroup_size(1024)
fn start(@builtin(local_invocation_index) tid: u32) {
    let n = par.n;
    for (var i = tid; i < n; i = i + 1024u) {
        schreib(par.l_fb, i, filtere(i, lies(par.l_b, i)));
        let k = kopf[i];
        if (k.y <= 0.0) {
            schreib(par.l_dv, i, vec3<f32>(0.0));
            continue;
        }
        var dv = filtere(i, lies(par.l_dv, i));
        if (k.z > 0.5) {
            let nl = normale[i].xyz;
            dv = dv + nl * (k.w - dot(lies(par.l_v, i), nl));
        }
        schreib(par.l_dv, i, dv);
    }
    workgroupBarrier();
    vorkond(tid, par.l_fb, par.l_zfb);
}

/// r = filter(b - A·dv), z = P^-1 r, dir = filter(z), and the two starting
/// products. Runs after `operator` on the warm start.
@compute @workgroup_size(1024)
fn richtungen(@builtin(local_invocation_index) tid: u32) {
    let n = par.n;
    for (var i = tid; i < n; i = i + 1024u) {
        let r = filtere(i, lies(par.l_b, i) - lies(par.l_ap, i));
        schreib(par.l_r, i, select(vec3<f32>(0.0), r, kopf[i].y > 0.0));
    }
    workgroupBarrier();
    vorkond(tid, par.l_r, par.l_z);
    workgroupBarrier();
    for (var i = tid; i < n; i = i + 1024u) {
        schreib(par.l_dir, i, filtere(i, lies(par.l_z, i)));
    }
    workgroupBarrier();
    var a = 0.0;
    var bsum = 0.0;
    for (var i = tid; i < n; i = i + 1024u) {
        if (kopf[i].y > 0.0) {
            a = a + dot(lies(par.l_r, i), lies(par.l_dir, i));
            bsum = bsum + dot(lies(par.l_fb, i), lies(par.l_zfb, i));
        }
    }
    let delta0 = summe(a, tid, false);
    let bb = summe(bsum, tid, true);
    if (tid == 0u) {
        setz_sk(0u, delta0);
        setz_sk(1u, bb);
        setz_sk(2u, 0.0);
        setz_sk(7u, 0.0);
        let ziel = max(par.tol * par.tol * max(bb, delta0), par.boden);
        setz_sk(3u, ziel);
        // Converged before the first iteration: the following passes return.
        setz_sk(4u, select(0.0, 1.0, delta0 <= ziel));
        setz_sk(5u, 1.0);
    }
}

/// One CG iteration, after `operator` wrote A·p for `dir`: α, the updates of
/// `dv` and `r`, the preconditioner, β and the new direction.
@compute @workgroup_size(1024)
fn schritt(@builtin(local_invocation_index) tid: u32) {
    if (sk(4u) > 0.5) {
        return;
    }
    let n = par.n;
    var ca = 0.0;
    for (var i = tid; i < n; i = i + 1024u) {
        if (kopf[i].y > 0.0) {
            ca = ca + dot(lies(par.l_dir, i), lies(par.l_ap, i));
        }
    }
    let ca_summe = summe(ca, tid, false);
    if (tid == 0u) {
        if (ca_summe > 1e-30) {
            setz_sk(6u, sk(0u) / ca_summe);
        } else {
            setz_sk(6u, 0.0);
        }
    }
    workgroupBarrier();
    let alpha = sk(6u);
    for (var i = tid; i < n; i = i + 1024u) {
        schreib(par.l_dv, i, lies(par.l_dv, i) + alpha * lies(par.l_dir, i));
        schreib(par.l_r, i, lies(par.l_r, i) - alpha * lies(par.l_ap, i));
    }
    workgroupBarrier();
    vorkond(tid, par.l_r, par.l_z);
    workgroupBarrier();
    var neu = 0.0;
    for (var i = tid; i < n; i = i + 1024u) {
        if (kopf[i].y > 0.0) {
            neu = neu + dot(lies(par.l_r, i), lies(par.l_z, i));
        }
    }
    let neu_summe = summe(neu, tid, false);
    if (tid == 0u) {
        let alt = sk(0u);
        var beta = 0.0;
        if (alt > 1e-30) {
            beta = neu_summe / alt;
        }
        setz_sk(0u, neu_summe);
        setz_sk(5u, beta);
        let it = sk(7u) + 1.0;
        setz_sk(7u, it);
        setz_sk(2u, it);
        let fertig = neu_summe <= sk(3u) || it >= f32(par.cg_max);
        setz_sk(4u, select(0.0, 1.0, fertig));
    }
    workgroupBarrier();
    let beta = sk(5u);
    for (var i = tid; i < n; i = i + 1024u) {
        schreib(par.l_dir, i, filtere(i, lies(par.l_z, i) + beta * lies(par.l_dir, i)));
    }
}
"#;

/// The buffers of one scene size.
#[derive(Clone)]
struct Speicher {
    n: usize,
    kanten: usize,
    naehte: usize,
    biege: usize,
    kopf: wgpu::Buffer,
    normale: wgpu::Buffer,
    pinv: wgpu::Buffer,
    daten: wgpu::Buffer,
    liste: wgpu::Buffer,
    kante: wgpu::Buffer,
    biege_b: wgpu::Buffer,
    geo: wgpu::Buffer,
    params: wgpu::Buffer,
    bind: wgpu::BindGroup,
    lesen: wgpu::Buffer,
}

/// What the device build wrote for one system, read back for the comparison
/// with the CPU build (`tests/gpu.rs`, `aufbau_wie_cpu`). Distance constraints
/// are edges first, then seams, as in [`System`].
#[derive(Debug, Clone, Default)]
pub struct GpuAufbau {
    /// Per distance constraint: the two points as stored on the device.
    pub kanten_punkte: Vec<[u32; 2]>,
    /// Per distance constraint: stiffness as stored on the device.
    pub kanten_k: Vec<f64>,
    /// Per distance constraint: unit direction `i → j`.
    pub richtung: Vec<[f64; 3]>,
    /// Per distance constraint: cross factor `max(0, 1 − l₀/l)`.
    pub quer: Vec<f64>,
    /// Per bending constraint: the four points as stored on the device.
    pub biege_punkte: Vec<[u32; 4]>,
    /// Per bending constraint: stiffness as stored on the device.
    pub biege_k: Vec<f64>,
    /// Per bending constraint: the gradient of the dihedral angle at its four
    /// points (zero for a degenerate constraint).
    pub grad: Vec<[[f64; 3]; 4]>,
    /// Per point: force, physical load, `H·v`, right-hand side.
    pub f: Vec<[f64; 3]>,
    pub last: Vec<[f64; 3]>,
    pub hv: Vec<[f64; 3]>,
    pub rhs: Vec<[f64; 3]>,
    /// Per point: inverse of the 3×3 diagonal block, by rows.
    pub pinv: Vec<M3>,
}

/// Step 4b: the `wgpu` backend of [`Loeser`].
///
/// [`GpuLoeser::neu`] returns `Err` on a machine without an adapter; the caller
/// then keeps the CPU backend, and the tests skip instead of failing.
pub struct GpuLoeser {
    geraet: String,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Warm start: dv, filtered rhs, preconditioned fb. One workgroup.
    pipeline: wgpu::ComputePipeline,
    /// `A·p` on `dv`, unfiltered, over the whole grid.
    op_warm: wgpu::ComputePipeline,
    /// `A·p` for the search direction, over the whole grid.
    op_dir: wgpu::ComputePipeline,
    /// `A·p` for the acceptance test, over the whole grid.
    op_roh: wgpu::ComputePipeline,
    /// First r, z, dir and both starting products. One workgroup.
    richtungen: wgpu::ComputePipeline,
    /// One CG iteration's reductions and vector updates. One workgroup.
    schritt: wgpu::ComputePipeline,
    /// The system build: distance constraints, bending, per point.
    aufbau_k: wgpu::ComputePipeline,
    aufbau_b: wgpu::ComputePipeline,
    sammeln: wgpu::ComputePipeline,
    lage: wgpu::BindGroupLayout,
    speicher: Option<Speicher>,
    /// What `kante` and `biege` on the device hold, to upload only changes.
    hoch_k: Vec<f32>,
    hoch_b: Vec<f32>,
    cpu: CpuLoeser,
    ausweich: usize,
    /// Milliseconds of the last solve: upload into the buffers, then everything
    /// the solve itself does (build included) with its readback.
    zeiten: [f64; 2],
    /// Iterations the previous solve needed, to record passes for about that
    /// many. The recorded passes past convergence are empty, but not free.
    letzte_it: usize,
    /// `true` while the buffers still hold the system and result of the last
    /// solve, so the release pass can reuse them instead of uploading again.
    bereit: bool,
}

impl std::fmt::Debug for GpuLoeser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GpuLoeser({})", self.geraet)
    }
}

/// Workgroups for `anzahl` items at 256 per group; the kernels loop with the
/// grid stride, so the count may be capped.
fn gruppen(anzahl: usize) -> u32 {
    (anzahl as u32).div_ceil(256).clamp(1, 65_535)
}

fn als_f64(f: &[f32], i: usize) -> [f64; 3] {
    [f[i * 4] as f64, f[i * 4 + 1] as f64, f[i * 4 + 2] as f64]
}

/// Indices of `a` and `b` agree: `schritt` floats per record, the first
/// `zahl` of them are point indices.
fn indizes_gleich(a: &[f32], b: &[f32], schritt: usize, zahl: usize) -> bool {
    a.len() == b.len()
        && a.chunks_exact(schritt)
            .zip(b.chunks_exact(schritt))
            .all(|(x, y)| x[..zahl] == y[..zahl])
}

impl GpuLoeser {
    /// Opens the GPU. `Err` carries the reason (no adapter, no device).
    pub fn neu() -> Result<Self, String> {
        let inst = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|_| "kein GPU-Adapter".to_string())?;
        let info = adapter.get_info();
        let geraet = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("klon3d CG"),
            // Der Löser ist ein einziger Workgroup-Kernel; die Vorgabe von
            // WebGPU sind 256 Aufrufe je Workgroup, hier sind es 1024.
            required_limits: wgpu::Limits {
                max_compute_invocations_per_workgroup: 1024,
                max_compute_workgroup_size_x: 1024,
                ..Default::default()
            },
            ..Default::default()
        }))
        .map_err(|e| format!("GPU-Gerät: {e}"))?;

        let modul = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("klon3d CG"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let eintrag = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        // Eight storage buffers: the WebGPU default limit per stage.
        let mut eintraege = vec![eintrag(0, wgpu::BufferBindingType::Uniform)];
        for b in 1..=8 {
            eintraege.push(eintrag(b, wgpu::BufferBindingType::Storage { read_only: false }));
        }
        let lage = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("klon3d CG"),
            entries: &eintraege,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("klon3d CG"),
            bind_group_layouts: &[Some(&lage)],
            immediate_size: 0,
        });
        let pipe = |name: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: Some(&pl),
                module: &modul,
                entry_point: Some(name),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Ok(GpuLoeser {
            pipeline: pipe("start"),
            op_warm: pipe("op_warm"),
            op_dir: pipe("op_dir"),
            op_roh: pipe("op_roh"),
            richtungen: pipe("richtungen"),
            schritt: pipe("schritt"),
            aufbau_k: pipe("aufbau_k"),
            aufbau_b: pipe("aufbau_b"),
            sammeln: pipe("sammeln"),
            geraet,
            device,
            queue,
            lage,
            speicher: None,
            hoch_k: Vec::new(),
            hoch_b: Vec::new(),
            cpu: CpuLoeser,
            ausweich: 0,
            zeiten: [0.0; 2],
            letzte_it: 0,
            bereit: false,
        })
    }

    /// Milliseconds of the last solve: upload into the buffers, then everything
    /// the solve itself does including the build and its readback. The
    /// benchmark (`examples/gpu_takt.rs`) reads it to see the share of the
    /// transfer.
    pub fn zeiten(&self) -> [f64; 2] {
        self.zeiten
    }

    /// Backend, device name and device type, for the report.
    pub fn geraet(&self) -> &str {
        &self.geraet
    }

    /// Steps that went to the CPU because the system carried springs.
    pub fn ausweichungen(&self) -> usize {
        self.ausweich
    }

    fn puffer(&self, bytes: u64, lesen: bool) -> wgpu::Buffer {
        let usage = if lesen {
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST
        } else {
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
        };
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes.max(16),
            usage,
            mapped_at_creation: false,
        })
    }

    /// The gathering structure: per point its distance constraints and bending
    /// slots, with the slot the point occupies in the bending constraint.
    /// Depends on the topology only.
    fn liste_bauen(sys: &System<'_>) -> Vec<u32> {
        let n = sys.masse.len();
        let mut zahl = vec![0u32; n];
        for k in sys.kanten.iter().chain(&sys.naehte) {
            zahl[k.i as usize] += 1;
            zahl[k.j as usize] += 1;
        }
        for b in &sys.biegungen {
            for q in b.punkte {
                zahl[q as usize] += 1;
            }
        }
        let mut aus = vec![0u32; n + 1];
        for i in 0..n {
            aus[i + 1] = aus[i] + zahl[i];
        }
        let mut fuell = aus.clone();
        let mut eintraege = vec![0u32; aus[n] as usize * 4];
        let mut eintrag = |i: usize, art: u32, c: u32, m: u32| {
            let e = fuell[i] as usize;
            fuell[i] += 1;
            eintraege[4 * e..4 * e + 4].copy_from_slice(&[art, c, m, 0]);
        };
        // Kanten und Nähte teilen sich einen Indexraum: erst Kanten, dann Nähte.
        for (c, k) in sys.kanten.iter().chain(&sys.naehte).enumerate() {
            eintrag(k.i as usize, 0, c as u32, 0);
            eintrag(k.j as usize, 0, c as u32, 0);
        }
        for (c, b) in sys.biegungen.iter().enumerate() {
            for m in 0..4 {
                eintrag(b.punkte[m] as usize, 1, c as u32, m as u32);
            }
        }
        aus.extend_from_slice(&eintraege);
        aus
    }

    /// Puts the system into the buffers: topology and stiffnesses when they
    /// changed, then positions, velocities, the warm start `dv` and the contact
    /// set. The geometry itself is computed by [`Self::bauen`].
    fn hochladen(&mut self, sys: &System<'_>, fest: &[Option<Fest>], dv: &[[f64; 3]]) -> Speicher {
        // Whatever was in the buffers is gone after this: `dv` is overwritten.
        self.bereit = false;
        let n = sys.masse.len();
        let (nk, nn, nb) = (sys.kanten.len(), sys.naehte.len(), sys.biegungen.len());
        let neu = match &self.speicher {
            Some(s) => s.n != n || s.kanten != nk || s.naehte != nn || s.biege != nb,
            None => true,
        };
        if neu {
            let lage = Lage::neu(n);
            let liste_laenge = n + 1 + 4 * (2 * (nk + nn) + 4 * nb);
            let kopf = self.puffer(n as u64 * 16, false);
            let normale = self.puffer(n as u64 * 16, false);
            let pinv = self.puffer(n as u64 * 48, false);
            let daten = self.puffer(lage.ende as u64 * 16, false);
            let liste = self.puffer(liste_laenge as u64 * 4, false);
            let kante = self.puffer((nk + nn) as u64 * 16, false);
            let biege_b = self.puffer(nb as u64 * 32, false);
            let geo = self.puffer((2 * (nk + nn) + 5 * nb) as u64 * 16, false);
            let params = self.puffer(std::mem::size_of::<Params>() as u64, false);
            let lesen = self.puffer((2 * n + 8) as u64 * 16, true);
            let puffer = [&params, &kopf, &normale, &pinv, &daten, &liste, &kante, &biege_b, &geo];
            let eintraege: Vec<wgpu::BindGroupEntry> = puffer
                .iter()
                .enumerate()
                .map(|(b, p)| wgpu::BindGroupEntry {
                    binding: b as u32,
                    resource: p.as_entire_binding(),
                })
                .collect();
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.lage,
                entries: &eintraege,
            });
            self.speicher = Some(Speicher {
                n,
                kanten: nk,
                naehte: nn,
                biege: nb,
                kopf,
                normale,
                pinv,
                daten,
                liste,
                kante,
                biege_b,
                geo,
                params,
                bind,
                lesen,
            });
            self.hoch_k.clear();
            self.hoch_b.clear();
        }
        let s = self.speicher.clone().expect("eben gebaut");
        let lage = Lage::neu(n);

        // Topology and stiffnesses. Seam rest lengths change while a garment is
        // sewn, the rest stays; the neighbour list only follows the indices.
        let mut kanten_f = Vec::with_capacity((nk + nn) * 4);
        for k in &sys.kanten {
            kanten_f.extend_from_slice(&[k.i as f32, k.j as f32, k.k as f32, k.ruhe as f32]);
        }
        for k in &sys.naehte {
            let ruhe = if k.ruhe.is_finite() { k.ruhe as f32 } else { -1.0 };
            kanten_f.extend_from_slice(&[k.i as f32, k.j as f32, k.k as f32, ruhe]);
        }
        let mut biege_f = Vec::with_capacity(nb * 8);
        for b in &sys.biegungen {
            biege_f.extend_from_slice(&b.punkte.map(|q| q as f32));
            biege_f.extend_from_slice(&[b.k as f32, b.ruhe as f32, 0.0, 0.0]);
        }
        let liste_neu = !indizes_gleich(&kanten_f, &self.hoch_k, 4, 2)
            || !indizes_gleich(&biege_f, &self.hoch_b, 8, 4);
        if liste_neu {
            let liste = Self::liste_bauen(sys);
            self.queue.write_buffer(&s.liste, 0, bytemuck::cast_slice(&liste));
        }
        if kanten_f != self.hoch_k {
            self.queue.write_buffer(&s.kante, 0, bytemuck::cast_slice(&kanten_f));
            self.hoch_k = kanten_f;
        }
        if biege_f != self.hoch_b {
            self.queue.write_buffer(&s.biege_b, 0, bytemuck::cast_slice(&biege_f));
            self.hoch_b = biege_f;
        }

        // Per point.
        let mut kopf = Vec::with_capacity(n * 4);
        let mut normale = Vec::with_capacity(n * 4);
        let mut dv_f = Vec::with_capacity(n * 4);
        // v, xh, xl lie one after the other in the data buffer: one write.
        let mut vx = vec![0.0f32; 3 * n * 4];
        for i in 0..n {
            kopf.extend_from_slice(&[
                (sys.masse[i] * (1.0 + sys.h * sys.c)) as f32,
                sys.w[i] as f32,
                if fest[i].is_some() { 1.0 } else { 0.0 },
                fest[i].map(|f| f.vn_soll).unwrap_or(0.0) as f32,
            ]);
            let nrm = fest[i].map(|f| f.normale).unwrap_or([0.0; 3]);
            normale.extend_from_slice(&[nrm[0] as f32, nrm[1] as f32, nrm[2] as f32, sys.masse[i] as f32]);
            dv_f.extend_from_slice(&[dv[i][0] as f32, dv[i][1] as f32, dv[i][2] as f32, 0.0]);
            for a in 0..3 {
                let x = sys.x[i][a];
                let hi = x as f32;
                vx[i * 4 + a] = sys.v[i][a] as f32;
                vx[(n + i) * 4 + a] = hi;
                vx[(2 * n + i) * 4 + a] = (x - hi as f64) as f32;
            }
        }
        debug_assert!(lage.xh == lage.v + n as u32 && lage.xl == lage.xh + n as u32);
        self.queue.write_buffer(&s.kopf, 0, bytemuck::cast_slice(&kopf));
        self.queue.write_buffer(&s.normale, 0, bytemuck::cast_slice(&normale));
        self.queue.write_buffer(&s.daten, lage.dv as u64 * 16, bytemuck::cast_slice(&dv_f));
        self.queue.write_buffer(&s.daten, lage.v as u64 * 16, bytemuck::cast_slice(&vx));
        s
    }

    fn params(&self, s: &Speicher, sys: &System<'_>, modus: u32) -> Params {
        let lage = Lage::neu(s.n);
        Params {
            n: s.n as u32,
            kanten: s.kanten as u32,
            naehte: s.naehte as u32,
            biege: s.biege as u32,
            modus,
            cg_max: sys.cg_max as u32,
            l_dv: lage.dv,
            l_r: lage.r,
            l_z: lage.z,
            l_dir: lage.dir,
            l_ap: lage.ap,
            l_b: lage.b,
            l_v: lage.v,
            l_xh: lage.xh,
            l_xl: lage.xl,
            l_last: lage.last,
            l_f: lage.f,
            l_hv: lage.hv,
            l_fb: lage.fb,
            l_zfb: lage.zfb,
            l_sk: lage.sk,
            l_p: lage.p,
            h: sys.h as f32,
            c: sys.c as f32,
            h2: (sys.h * sys.h) as f32,
            tol: sys.cg_toleranz as f32,
            boden: (sys.masse.iter().sum::<f64>() * 1e-18) as f32,
            gx: sys.g[0] as f32,
            gy: sys.g[1] as f32,
            gz: sys.g[2] as f32,
            _pad: [0.0; 2],
        }
    }

    fn pass(&self, s: &Speicher, enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, gruppen: u32) {
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        cp.set_pipeline(pipe);
        cp.set_bind_group(0, &s.bind, &[]);
        cp.dispatch_workgroups(gruppen, 1, 1);
    }

    /// Records the system build: geometry per constraint, then per point.
    fn bauen(&self, s: &Speicher, enc: &mut wgpu::CommandEncoder) {
        self.pass(s, enc, &self.aufbau_k, gruppen(s.kanten + s.naehte));
        self.pass(s, enc, &self.aufbau_b, gruppen(s.biege));
        self.pass(s, enc, &self.sammeln, gruppen(s.n));
    }

    fn warten(&self) {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok();
    }

    /// Copies the given `(offset, vec4 count)` segments of the data buffer into
    /// the readback buffer, one after the other, and maps once.
    fn lesen(&self, s: &Speicher, teile: &[(u32, usize)]) -> Vec<f32> {
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let mut ziel = 0u64;
        for &(lage, zahl) in teile {
            enc.copy_buffer_to_buffer(&s.daten, lage as u64 * 16, &s.lesen, ziel, zahl as u64 * 16);
            ziel += zahl as u64 * 16;
        }
        self.queue.submit([enc.finish()]);
        let slice = s.lesen.slice(..ziel.max(16));
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.warten();
        let daten = slice.get_mapped_range().unwrap();
        let f: Vec<f32> = bytemuck::cast_slice(&daten).to_vec();
        drop(daten);
        s.lesen.unmap();
        f
    }

    /// Reads a whole buffer through a temporary one; for the acceptance test,
    /// not for the step.
    fn puffer_lesen(&self, quelle: &wgpu::Buffer) -> Vec<f32> {
        let bytes = quelle.size();
        let ziel = self.puffer(bytes, true);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(quelle, 0, &ziel, 0, bytes);
        self.queue.submit([enc.finish()]);
        let slice = ziel.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.warten();
        let daten = slice.get_mapped_range().unwrap();
        let f: Vec<f32> = bytemuck::cast_slice(&daten).to_vec();
        drop(daten);
        ziel.unmap();
        f
    }

    fn loese_gpu(&mut self, sys: &System<'_>, fest: &[Option<Fest>], dv: &mut [[f64; 3]]) -> CgLauf {
        let n = sys.masse.len();
        let t0 = std::time::Instant::now();
        let s = self.hochladen(sys, fest, dv);
        let t1 = std::time::Instant::now();
        let par = self.params(&s, sys, 0);
        self.queue.write_buffer(&s.params, 0, bytemuck::bytes_of(&par));
        // The build and all passes of the solve go into one submission: the
        // operator over the whole grid, the reductions and updates on one
        // workgroup. The pass boundary is the global barrier between them and
        // costs about 6 µs.
        //
        // How many iterations to record is a guess: the passes past convergence
        // return at once, but each still costs its dispatch. A solve that
        // covers the budget without converging gets another submission, so the
        // guess costs at most one extra wait.
        let breit = gruppen(n);
        let lage = Lage::neu(n);
        let mut erste = true;
        let mut it = 0usize;
        let (dv_f, sk) = loop {
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            if erste {
                self.bauen(&s, &mut enc);
                self.pass(&s, &mut enc, &self.pipeline, 1);
                self.pass(&s, &mut enc, &self.op_warm, breit);
                self.pass(&s, &mut enc, &self.richtungen, 1);
                erste = false;
            }
            let rest = sys.cg_max.saturating_sub(it);
            let budget = if it == 0 {
                // First solve: no count to go by yet. Take a guess that
                // usually covers a whole solve in one submission.
                let schaetzung = if self.letzte_it > 0 {
                    self.letzte_it + self.letzte_it / 4 + 8
                } else {
                    128
                };
                schaetzung.min(rest)
            } else {
                rest
            };
            for _ in 0..budget {
                self.pass(&s, &mut enc, &self.op_dir, breit);
                self.pass(&s, &mut enc, &self.schritt, 1);
            }
            self.queue.submit([enc.finish()]);
            let f = self.lesen(&s, &[(lage.dv, n), (lage.sk, 8)]);
            let mut sk = [0.0f32; 8];
            for (i, a) in sk.iter_mut().enumerate() {
                *a = f[(n + i) * 4];
            }
            it = sk[2] as usize;
            // Converged, out of budget, or nothing recorded: done.
            if sk[4] > 0.5 || it >= sys.cg_max || budget == 0 {
                break (f, sk);
            }
        };
        self.letzte_it = it;
        // The buffers still hold this system and `Δv`, so the release pass can
        // apply the operator on the device instead of uploading again.
        self.bereit = true;
        let t2 = std::time::Instant::now();
        self.zeiten = [(t1 - t0).as_secs_f64() * 1e3, (t2 - t1).as_secs_f64() * 1e3];
        for (i, d) in dv.iter_mut().enumerate() {
            *d = als_f64(&dv_f, i);
        }
        // Je Skalar ein vec4: erstes Feld = Wert.
        let (delta, bb, it, ziel) = (sk[0] as f64, sk[1] as f64, sk[2] as f64, sk[3] as f64);
        let boden = sys.masse.iter().sum::<f64>() * 1e-18;
        CgLauf {
            iterationen: it as usize,
            nicht_konvergiert: delta > ziel,
            rest: (delta / bb.max(boden)).sqrt(),
        }
    }

    /// Cost of a chain of `pässe` compute passes recorded into one submission
    /// with one wait, in milliseconds per pass. `art` selects what runs: 0 an
    /// empty pass, 1 `A·p` on one workgroup, 2 `A·p` over the whole grid, 3 the
    /// whole system build (three passes, counted as one).
    pub fn takt_paesse(&mut self, sys: &System<'_>, p: &[[f64; 3]], pässe: usize, art: u32) -> f64 {
        let n = sys.masse.len();
        let s = self.hochladen(sys, &vec![None; n], &vec![[0.0; 3]; n]);
        let lage = Lage::neu(n);
        let mut p_f = Vec::with_capacity(n * 4);
        for q in p {
            p_f.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32, 0.0]);
        }
        self.queue.write_buffer(&s.daten, lage.p as u64 * 16, bytemuck::cast_slice(&p_f));
        let par = self.params(&s, sys, 1);
        self.queue.write_buffer(&s.params, 0, bytemuck::bytes_of(&par));
        // The operator needs the geometry: build once before timing.
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.bauen(&s, &mut enc);
        self.queue.submit([enc.finish()]);
        let lauf = || {
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            for _ in 0..pässe {
                match art {
                    0 => {
                        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
                        cp.set_bind_group(0, &s.bind, &[]);
                        cp.set_pipeline(&self.richtungen);
                    }
                    1 => self.pass(&s, &mut enc, &self.op_roh, 1),
                    2 => self.pass(&s, &mut enc, &self.op_roh, gruppen(n)),
                    _ => self.bauen(&s, &mut enc),
                }
            }
            self.queue.submit([enc.finish()]);
        };
        lauf();
        self.warten();
        let t0 = std::time::Instant::now();
        lauf();
        self.warten();
        t0.elapsed().as_secs_f64() * 1e3 / pässe as f64
    }

    /// `A·p` on the device, the system built there from the positions; for the
    /// acceptance test.
    pub fn mal_a_roh(&mut self, sys: &System<'_>, p: &[[f64; 3]]) -> Vec<[f64; 3]> {
        let n = sys.masse.len();
        let s = self.hochladen(sys, &vec![None; n], &vec![[0.0; 3]; n]);
        let lage = Lage::neu(n);
        let mut p_f = Vec::with_capacity(n * 4);
        for q in p {
            p_f.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32, 0.0]);
        }
        self.queue.write_buffer(&s.daten, lage.p as u64 * 16, bytemuck::cast_slice(&p_f));
        let par = self.params(&s, sys, 1);
        self.queue.write_buffer(&s.params, 0, bytemuck::bytes_of(&par));
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.bauen(&s, &mut enc);
        self.pass(&s, &mut enc, &self.op_roh, gruppen(n));
        self.queue.submit([enc.finish()]);
        let f = self.lesen(&s, &[(lage.ap, n)]);
        (0..n).map(|i| als_f64(&f, i)).collect()
    }

    /// Builds the system of `sys` on the device and reads every record back:
    /// what the acceptance test compares against the CPU build. The contact
    /// set does not enter the build, so none is needed.
    pub fn aufbau_lesen(&mut self, sys: &System<'_>) -> GpuAufbau {
        let n = sys.masse.len();
        let s = self.hochladen(sys, &vec![None; n], &vec![[0.0; 3]; n]);
        let par = self.params(&s, sys, 1);
        self.queue.write_buffer(&s.params, 0, bytemuck::bytes_of(&par));
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.bauen(&s, &mut enc);
        self.queue.submit([enc.finish()]);
        let lage = Lage::neu(n);
        let kante = self.puffer_lesen(&s.kante);
        let biege = self.puffer_lesen(&s.biege_b);
        let geo = self.puffer_lesen(&s.geo);
        let pinv = self.puffer_lesen(&s.pinv);
        let punkte = self.lesen(&s, &[(lage.f, n), (lage.last, n)]);
        let punkte2 = self.lesen(&s, &[(lage.hv, n), (lage.b, n)]);
        let nd = s.kanten + s.naehte;
        let mut a = GpuAufbau::default();
        for c in 0..nd {
            let k = &kante[c * 4..c * 4 + 4];
            a.kanten_punkte.push([k[0] as u32, k[1] as u32]);
            a.kanten_k.push(k[2] as f64);
            a.richtung.push(als_f64(&geo, 2 * c));
            a.quer.push(geo[2 * c * 4 + 3] as f64);
        }
        for c in 0..s.biege {
            let b = &biege[c * 8..c * 8 + 8];
            a.biege_punkte.push([b[0] as u32, b[1] as u32, b[2] as u32, b[3] as u32]);
            a.biege_k.push(b[4] as f64);
            let basis = 2 * nd + 5 * c;
            a.grad.push([0, 1, 2, 3].map(|m| als_f64(&geo, basis + m)));
        }
        for i in 0..n {
            a.f.push(als_f64(&punkte, i));
            a.last.push(als_f64(&punkte, n + i));
            a.hv.push(als_f64(&punkte2, i));
            a.rhs.push(als_f64(&punkte2, n + i));
            a.pinv.push([0, 1, 2].map(|z| als_f64(&pinv, 3 * i + z)));
        }
        self.bereit = false;
        a
    }
}

impl Loeser for GpuLoeser {
    fn name(&self) -> &'static str {
        "GPU (f32, wgpu)"
    }

    fn baut_selbst(&self) -> bool {
        true
    }

    fn loese(&mut self, sys: &System<'_>, fest: &[Option<Fest>], dv: &mut [[f64; 3]]) -> CgLauf {
        if !sys.federn.is_empty() {
            self.ausweich += 1;
            // The CPU solve does not put anything into the device buffers.
            self.bereit = false;
            return self.cpu.loese(sys, fest, dv);
        }
        self.loese_gpu(sys, fest, dv)
    }

    fn mal_a(&mut self, sys: &System<'_>, p: &[[f64; 3]], out: &mut [[f64; 3]]) {
        if !sys.federn.is_empty() {
            // Springs are not on the device; such a system is built in full.
            return self.cpu.mal_a(sys, p, out);
        }
        let gpu = self.mal_a_roh(sys, p);
        out.copy_from_slice(&gpu);
    }

    /// The release pass after a solve: system and `Δv` are still in the device
    /// buffers, so the unfiltered `A·Δv` runs there without uploading again.
    /// `op_warm` is exactly that operator - unfiltered `A·p` over the whole
    /// grid, reading `l_dv`.
    fn mal_a_geloest(&mut self, sys: &System<'_>, dv: &[[f64; 3]], out: &mut [[f64; 3]]) {
        let mut rest = vec![[0.0; 3]; dv.len()];
        self.a_und_b(sys, dv, out, &mut rest);
    }

    /// `A·Δv − b` with `b` as the device built it.
    fn rest_geloest(&mut self, sys: &System<'_>, dv: &[[f64; 3]], out: &mut [[f64; 3]]) {
        if !sys.federn.is_empty() {
            return self.cpu.rest_geloest(sys, dv, out);
        }
        let mut b = vec![[0.0; 3]; dv.len()];
        self.a_und_b(sys, dv, out, &mut b);
        for (o, b) in out.iter_mut().zip(&b) {
            *o = [o[0] - b[0], o[1] - b[1], o[2] - b[2]];
        }
    }

    fn last_lesen(&mut self) -> Vec<[f64; 3]> {
        let Some(s) = self.speicher.clone() else { return Vec::new() };
        let f = self.lesen(&s, &[(Lage::neu(s.n).last, s.n)]);
        (0..s.n).map(|i| als_f64(&f, i)).collect()
    }
}

impl GpuLoeser {
    /// `A·Δv` and `b` of `sys` in one readback. Reuses the buffers when they
    /// still hold the last solve; otherwise uploads and builds first.
    fn a_und_b(&mut self, sys: &System<'_>, dv: &[[f64; 3]], adv: &mut [[f64; 3]], b: &mut [[f64; 3]]) {
        if !sys.federn.is_empty() {
            self.cpu.mal_a(sys, dv, adv);
            b.copy_from_slice(&sys.rhs);
            return;
        }
        let n = sys.masse.len();
        let frisch = !self.bereit || self.speicher.as_ref().is_none_or(|s| s.n != n);
        let s = if frisch {
            self.hochladen(sys, &vec![None; n], dv)
        } else {
            self.speicher.clone().expect("geprüft")
        };
        let par = self.params(&s, sys, 0);
        self.queue.write_buffer(&s.params, 0, bytemuck::bytes_of(&par));
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        if frisch {
            self.bauen(&s, &mut enc);
        }
        self.pass(&s, &mut enc, &self.op_warm, gruppen(n));
        self.queue.submit([enc.finish()]);
        let lage = Lage::neu(n);
        let f = self.lesen(&s, &[(lage.ap, n), (lage.b, n)]);
        for i in 0..n {
            adv[i] = als_f64(&f, i);
            b[i] = als_f64(&f, n + i);
        }
        // `op_warm` wrote over `l_ap`, not `l_dv`: the buffers still hold the
        // solve, and a second call can reuse them.
        if frisch {
            self.bereit = true;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn einstiegspunkte_im_shader() {
        for name in [
            "fn start(",
            "fn op_warm(",
            "fn op_dir(",
            "fn schritt(",
            "fn aufbau_k(",
            "fn aufbau_b(",
            "fn sammeln(",
        ] {
            assert!(super::WGSL.contains(name), "{name} fehlt im Shader");
        }
    }

    #[test]
    fn indizes_gleich_vergleicht_nur_indizes() {
        let a = [1.0, 2.0, 0.5, 0.1, 3.0, 4.0, 0.5, 0.2];
        let mut b = a;
        b[3] = 9.0;
        assert!(super::indizes_gleich(&a, &b, 4, 2));
        b[4] = 5.0;
        assert!(!super::indizes_gleich(&a, &b, 4, 2));
        assert!(!super::indizes_gleich(&a, &a[..4], 4, 2));
    }
}
