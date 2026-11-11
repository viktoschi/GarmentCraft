//! The seam allowance as fabric: the allowance band folded 180° inwards.
//!
//! [`klon3d_mesh::Teilnetz::zugabe`] carries the allowance as a strip of
//! triangles between the seam line and the cutting line, welded to the piece's
//! boundary points and hanging outwards from the seam line. In cloth it does not
//! hang outwards: it is folded onto the piece's reverse and sewn down along the
//! seam. This module turns the strip into scene pieces that have a mass, a
//! stretch stiffness and a bending stiffness like the piece they belong to, and
//! it places them folded:
//!
//! * **The fold is a turn.** Every cutting-line point is turned about the fold
//!   line through its seam-line point by `π − FALT_START_ABWEICHUNG`. The fold
//!   line is the seam-line edge leaving that point, lying in the piece - not the
//!   point's own offset, and not a chord across it - so the whole band comes out
//!   at the same angle whatever its width there, and the offset's component along
//!   the seam survives the turn instead of being flipped. Nothing is re-meshed:
//!   the band's points, triangles and stretch springs are the ones the mesh built.
//! * **The seam line is held, not pinned.** Each band seam-line point lies on
//!   the piece's boundary, at most 0.1 mm off the boundary polyline (measured on
//!   all three test patterns). A rest-length-0 distance constraint onto that
//!   boundary point - or, where the band point sits inside a boundary edge, two
//!   onto the edge's endpoints - keeps the fold where it was built. These are
//!   springs with the piece's own `dehnung_n_m`; no point loses its degree of
//!   freedom, so the band still holds through tension and friction alone.
//! * **The fold line is a bend.** One bending constraint per fold edge, rest
//!   angle 180°, so the band resists being peeled off the piece. A miter - two
//!   seam-line points welded into one, see [`klon3d_mesh::zugabe::Streifen`] -
//!   has no fold edge there; such edges are counted, never silently dropped.
//! * **Needle edges are capped.** The strip carries sliver triangles that span
//!   the whole band width at a length of a thousandth of a millimetre. Their
//!   stretch stiffness `dehnung_n_m · A / l²` would exceed the piece's by six
//!   orders of magnitude and the solver does not resolve that range; it is capped
//!   at the shape measure the pattern mesh itself reaches at the top of its own
//!   range ([`HOECHSTE_FORM`]). Lengths, points and triangles are untouched.
//!
//! The band is appended to the scene *after* the pattern pieces and gets its own
//! [`Teilinfo`] entries, named `<piece> (Zugabe)`. It never takes part in
//! placement: [`crate::platzierung::Platzierung`] indexes the pattern pieces by
//! position, and every lookup of a seam, a measurement or a glue zone goes
//! through piece indices or names of the pattern, both of which the band's
//! entries leave untouched. The seam point pairs, the seam order and the seam
//! arc fractions are unchanged - the seam line is the mesh boundary, exactly
//! where it was.
//!
//! What the band costs is reported in [`Faltbericht`]: fabric in mm², points,
//! triangles, holds, and how many fold edges the strip's miters and the stretches
//! that carry no fabric prevent. Nothing about it is silent.

use std::collections::BTreeMap;

use klon3d_mesh::{Musternetz, Teilnetz};

use crate::koerper::{add, cross, dot, mul, sub};
use crate::szene::Teilinfo;

/// How far the band starts short of a flat 180° fold (rad).
///
/// The dihedral angle is read through `atan2`, whose branch cut lies exactly at
/// 180°: a band folded to the last bit of a degree would fall on either side of
/// it from one frame to the next and the bending constraint would see a 360°
/// error instead of a hundredth. Starting 0.1° short tilts the band towards the
/// piece's reverse by the same small angle everywhere, because it is a rotation
/// and not a lift: the step across the band is `w · cos(0.1°)`, a difference of
/// 0.0000005 mm on a 3 mm band, and to the piece's reverse it is `w · sin(0.1°)`
/// = 0.005 mm - below the 0.1 mm by which the strip's seam line already misses
/// the boundary polyline anyway.
const FALT_START_ABWEICHUNG: f64 = 0.1 * std::f64::consts::PI / 180.0;

/// Largest `A/l²` a band edge may take its stiffness from.
///
/// A pattern piece's stretch spring is `dehnung_n_m · A / l²`, `A` the summed
/// area of the triangles on the edge and `l` its length. On the three meshed test
/// patterns that quotient spans 0.25 … 1.65 (`bluse2_v26`, 17665 edges), 0.21 …
/// 1.74 (`bluse3_v2`, 17177) and 0.29 … 1.50 (`rock_2schlitze_v34`, 55010):
/// `vernetze` does not emit the slivers that would push it up. The strip carries
/// such triangles - welding it to the boundary and cutting it at the miters
/// leaves needles 0.0001 mm long that still span the full band width - so `A/l²`
/// reaches 2.2 · 10⁹ and `k` up to 7.8 · 10⁵ N/m against the piece's 574 N/m, on
/// points of 1.6 · 10⁻⁹ kg.
///
/// The implicit solver cannot resolve a stiffness range of six orders of
/// magnitude in 50 CG iterations per substep: the band alone drives the flat
/// scene from 4 · 10⁻⁵ J to 10⁷³ J in 180 frames, and every substep reports
/// `nicht konvergiert`. Capping `A/l²` at the top of the pattern mesh's range -
/// this number, `bluse2_v26`'s own maximum - settles it to 3.0 · 10⁻⁶ J /
/// 0.0062 m/s, against 4.1 · 10⁻⁵ J of the same sheet without a band, and holds
/// every band edge at or below the stiffness of the piece's own stiffest edge.
/// The cap only shortens the needles' stiffness; it touches no position, no
/// triangle and no seam line.
pub const HOECHSTE_FORM: f64 = 1.65;

/// A band seam-line point this close to a boundary vertex is that vertex (mm).
///
/// The weld that built the strip inserted the boundary points themselves, so the
/// distance is 0.000000 mm wherever it applied. Above this the point sits inside
/// a boundary edge and needs two holds, not one.
const AUF_PUNKT_MM: f64 = 1e-6;

/// What folding the allowance cost and bought, per call to [`falte_band`].
///
/// Every count is either a piece of the result or a refusal that the caller has
/// to be able to report: a fold edge that the strip's miters block is a fact
/// about the pattern, not an error to swallow.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Faltbericht {
    /// Pieces whose band became cloth.
    pub teile: usize,
    /// Pieces without an allowance, or whose band has no triangle left.
    pub ohne_band: usize,
    /// Band points and triangles added to the scene.
    pub punkte: usize,
    pub dreiecke: usize,
    /// Band triangles dropped because the weld made two of their corners one.
    pub dreiecke_entartet: usize,
    /// Band fabric in mm², pattern area before reduction.
    pub flaeche_mm2: f64,
    /// Holds that keep a band seam-line point on the piece's boundary.
    pub halte: usize,
    /// Holds with a rest length (a band point inside a boundary edge).
    pub halte_kante: usize,
    /// Holds without a rest length (a band point that is a boundary vertex).
    pub halte_punkt: usize,
    /// Band seam-line points that sat inside a boundary edge, not on a vertex.
    pub punkte_auf_kante: usize,
    /// Largest distance of a band seam-line point from the boundary polyline
    /// (mm), and the point that was worst.
    pub halte_abstand_mm: f64,
    /// Fold edges that got a 180° bending constraint.
    pub falzkanten: usize,
    /// Fold edges blocked by a miter (two seam-line points became one vertex).
    pub falzkanten_gehrung: usize,
    /// Fold edges without a band triangle - a stretch that carries no fabric.
    pub falzkanten_ohne_band: usize,
    /// Pieces whose band has no triangle left after the weld.
    pub ohne_normale: usize,
    /// Fold edges whose seam-line points are no boundary edge of the piece.
    pub falzkanten_keine_randkante: usize,
    /// Fold edges that did not end up on the piece's reverse: the dihedral as
    /// built was under 90°, so the band points away from the piece instead of
    /// lying on it. Should be 0; a number here is a placement the fold did not
    /// survive, not a rounding.
    pub falz_falsche_seite: usize,
    /// Fold edges whose dihedral could not be resolved (degenerate triangles).
    pub falz_dreieck_fehlt: usize,
    /// Cutting-line points with no fold line to turn about: both seam
    /// neighbours coincided in the placed frame, so the point keeps its
    /// unfolded offset. Should be 0.
    pub falz_ohne_richtung: usize,
    /// Ring edges of the piece's boundary that carry no band triangle.
    pub halte_ohne_kante: usize,
    /// Band stretch edges that had their stiffness capped at
    /// [`HOECHSTE_FORM`] because their triangle is a needle.
    pub kanten_form_gedeckelt: usize,
    /// Band triangles without a usable plane in 3D; they take their longest
    /// edge as the material direction, like a pattern piece does.
    pub faser_entartet: usize,
    /// Smallest and largest dihedral angle of a fold edge as built (rad). Both
    /// sit near ±180°, their sign is the side the band was folded to.
    pub falz_winkel_min_rad: f64,
    pub falz_winkel_max_rad: f64,
}

/// The band of every piece of a pattern, ready to be appended to a scene.
///
/// Everything in here is in the scene's units and frames: `punkte` in metres,
/// indices global, offsets already added. [`falte_band`] builds it against the
/// scene's current point count, so a caller only has to append.
#[derive(Debug, Clone, Default)]
pub struct Faltung {
    pub punkte: Vec<[f64; 3]>,
    pub masse: Vec<f64>,
    pub kollisionsabstand: Vec<f64>,
    pub reibung: Vec<f64>,
    pub dreiecke: Vec<[u32; 3]>,
    pub ruhe_inv: Vec<[f64; 4]>,
    pub faser: Vec<[f64; 3]>,
    /// Distance constraints: points, rest length (m), compliance (m/N).
    pub abstand: Vec<([u32; 2], f64, f64)>,
    /// Bending constraints: the four points, rest angle (rad), compliance (1/Nm).
    pub biegung: Vec<([u32; 4], f64, f64)>,
    pub teile: Vec<Teilinfo>,
    pub bericht: Faltbericht,
}

fn abstand2(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Area of a 2D triangle, positive for a positive winding.
fn flaeche2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
}

/// The point of `ring` closest to `p`, as (distance, edge index, fraction).
///
/// `ring` is closed: edge `k` runs from `ring[k]` to `ring[(k + 1) % len]`.
fn projiziere(p: [f64; 2], ring: &[[f64; 2]]) -> (f64, usize, f64) {
    let mut best = (f64::INFINITY, 0usize, 0.0f64);
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let q = d[0] * d[0] + d[1] * d[1];
        let f = if q <= 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / q).clamp(0.0, 1.0)
        };
        let dist = abstand2(p, [a[0] + f * d[0], a[1] + f * d[1]]);
        if dist < best.0 {
            best = (dist, k, f);
        }
    }
    best
}

/// Edges of a triangle list in 2D: sorted point pair → summed area and the
/// opposite points, exactly as [`crate::szene::Szene::aus_netz`] builds them for
/// a pattern piece.
fn kanten(punkte: &[[f64; 2]], dreiecke: &[[u32; 3]]) -> BTreeMap<(u32, u32), (f64, Vec<u32>)> {
    let mut m: BTreeMap<(u32, u32), (f64, Vec<u32>)> = BTreeMap::new();
    for d in dreiecke {
        let (a, b, c) = (
            punkte[d[0] as usize],
            punkte[d[1] as usize],
            punkte[d[2] as usize],
        );
        let flaeche = flaeche2(a, b, c).abs();
        for k in 0..3 {
            let (i, j, o) = (d[k], d[(k + 1) % 3], d[(k + 2) % 3]);
            let e = m.entry((i.min(j), i.max(j))).or_insert((0.0, Vec::new()));
            e.0 += flaeche;
            e.1.push(o);
        }
    }
    m
}

/// The seam line's direction at ring point `j`, unit length and in the piece's
/// plane, or `None` where the seam line has none there.
///
/// It is the seam-line edge leaving `j` towards its next point, projected into
/// the piece: the fold line about which the band at `j` is turned. The outgoing
/// edge, not a chord through both neighbours and not the point's own offset -
/// see the note at the turn in [`band_teil`] for why each of those is wrong
/// where the seam line turns. Where the outgoing edge has no length (the weld
/// put two seam-line points on one place) the incoming one stands in.
fn nahtrichtung(innen3: &[[f64; 3]], nrm: [f64; 3], j: usize) -> Option<[f64; 3]> {
    let n = innen3.len();
    if n < 3 {
        return None;
    }
    let kante = |a: usize, b: usize| {
        let mut t = sub(innen3[b % n], innen3[a % n]);
        t = sub(t, mul(nrm, dot(t, nrm)));
        let l = dot(t, t).sqrt();
        if l > 1e-9 {
            Some(mul(t, 1.0 / l))
        } else {
            None
        }
    };
    kante(j, j + 1).or_else(|| kante(j + n - 1, j))
}

/// The boundary vertex of `ring` that `p` sits on, or `None` when `p` lies
/// inside a boundary edge.
///
/// A fold edge must be a boundary edge of the piece, and that needs both of its
/// ends to be boundary vertices. A seam-line point inside an edge is on the
/// boundary but not a vertex, so the piece has no edge to fold against there -
/// the caller counts it instead of inventing one.
fn randpunkt_von(p: [f64; 2], ring: &[[f64; 2]]) -> Option<u32> {
    let mut best: Option<(f64, u32)> = None;
    for (k, &q) in ring.iter().enumerate() {
        let d = abstand2(p, q);
        if d <= AUF_PUNKT_MM && best.map_or(true, |(b, _)| d < b) {
            best = Some((d, k as u32));
        }
    }
    best.map(|(_, k)| k)
}

/// The boundary edges of a piece: sorted point pair → the opposite point of its
/// only triangle.
fn randkanten(t: &Teilnetz) -> BTreeMap<(u32, u32), u32> {
    kanten(&t.punkte, &t.dreiecke)
        .into_iter()
        .filter_map(|(k, (_, gegen))| match gegen[..] {
            [o] => Some((k, o)),
            _ => None,
        })
        .collect()
}

/// The band of every piece with an allowance, folded and ready to be appended
/// to a scene that already holds `x` (and whose pieces are `teile`).
///
/// `teile` are the **pattern** pieces, one entry each, in mesh order: their
/// point ranges give the global offset of a piece, their `vorgabe` the material
/// the band is cut from. The returned indices count from `x.len()`, so a caller
/// appends its points and triangles in order and adds nothing.
///
/// `lage` is the same placement the pieces went through; the fold is built in
/// pattern coordinates and placed, so the band lands on the piece's reverse the
/// way the piece itself was placed.
pub fn falte_band(
    netz: &Musternetz,
    x: &[[f64; 3]],
    teile: &[Teilinfo],
    lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3],
) -> Faltung {
    let mut f = Faltung::default();
    let mut winkel: Vec<f64> = Vec::new();
    for (ti, t) in netz.teile.iter().enumerate() {
        if t.zugabe.ist_leer() || ti >= teile.len() {
            f.bericht.ohne_band += 1;
            continue;
        }
        let vorher = f.bericht.punkte;
        band_teil(&mut f, ti, t, x, teile, lage, &mut winkel);
        if f.bericht.punkte > vorher {
            f.bericht.teile += 1;
        } else {
            f.bericht.ohne_band += 1;
        }
    }
    if let (Some(lo), Some(hi)) = (
        winkel.iter().cloned().reduce(f64::min),
        winkel.iter().cloned().reduce(f64::max),
    ) {
        f.bericht.falz_winkel_min_rad = lo;
        f.bericht.falz_winkel_max_rad = hi;
    }
    f
}

/// Appends one piece's folded band to `f`; `winkel` collects the dihedral
/// angles of the fold edges as built.
#[allow(clippy::too_many_arguments)]
fn band_teil(
    f: &mut Faltung,
    ti: usize,
    t: &Teilnetz,
    x: &[[f64; 3]],
    teile: &[Teilinfo],
    lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3],
    winkel: &mut Vec<f64>,
) {
    let s = &t.zugabe;
    let n = s.innen.len();
    if n < 3 || s.aussen.len() != n || s.dreiecke.is_empty() {
        return;
    }
    let vertreter = s.vertreter();
    let vg = teile[ti].vorgabe;
    let skala = 1.0 - vg.reduktion;
    let dir2 = crate::faser::richtung(t.fadenlauf).0;
    let g = teile[ti].punkte.start; // global index of the piece's local point 0
    let d0 = f.dreiecke.len();
    let p0 = f.punkte.len();
    let basis = x.len() + p0; // global index of the band's local point 0

    // The band's vertices: an inner point is its weld representative, every
    // cutting-line point keeps its own index. Only the vertices a triangle
    // really names are kept, and they are renumbered densely from 0.
    let mut id: BTreeMap<u32, u32> = BTreeMap::new();
    let mut quelle: Vec<u32> = Vec::new();
    let mut lokal: Vec<[u32; 3]> = Vec::new();
    for d in &s.dreiecke {
        let r = d.map(|k| {
            if (k as usize) < n {
                vertreter[k as usize]
            } else {
                k
            }
        });
        let mut m = [0u32; 3];
        for (c, &k) in r.iter().enumerate() {
            m[c] = match id.get(&k) {
                Some(&l) => l,
                None => {
                    quelle.push(k);
                    let l = (quelle.len() - 1) as u32;
                    id.insert(k, l);
                    l
                }
            };
        }
        if m[0] == m[1] || m[1] == m[2] || m[0] == m[2] {
            f.bericht.dreiecke_entartet += 1;
            continue;
        }
        lokal.push(m);
    }
    if lokal.is_empty() {
        return;
    }
    let punkte2: Vec<[f64; 2]> = quelle.iter().map(|&k| s.punkt(k)).collect();

    // The seam line, placed: it is the fold line, the piece's boundary, and the
    // line the band is held to.
    let innen3: Vec<[f64; 3]> = (0..n).map(|j| lage(ti, s.innen[j])).collect();

    // The piece's placed reverse: its area-weighted normal, negated. The band is
    // folded onto that side, so the tenth of a degree below is taken towards it.
    // No lift off the fold line: a folded allowance lies *on* the reverse, and the
    // half fabric thickness between the two layers is the collision distance's
    // job (`kollisionsabstand` below), not a translation of the mesh.
    let mut nrm = [0.0; 3];
    for d in &t.dreiecke {
        let (a, b, c) = (
            x[g + d[0] as usize],
            x[g + d[1] as usize],
            x[g + d[2] as usize],
        );
        nrm = add(nrm, cross(sub(b, a), sub(c, a)));
    }
    let ln = dot(nrm, nrm).sqrt();
    if !(ln > 0.0) {
        f.bericht.ohne_normale += 1;
        return;
    }
    let nrm = mul(nrm, -1.0 / ln);

    // Folded, not mirrored and then placed: the cutting line is the turn of the
    // unfolded strip about the fold line, built in the placed frame.
    //
    // `lage` is not affine - it wraps a piece over a hull, a prism or an arm - so
    // placing a mirrored pattern point would land the band *near* the piece's
    // reverse and at an angle nobody measured. The turn is exact: a seam-line
    // point stays where the mesh put it, and a cutting-line point turns about the
    // fold axis through its seam-line point `q` by `π − FALT_START_ABWEICHUNG`.
    //
    // The fold line at this point is the seam-line edge leaving it, taken from
    // the seam line - not `normal × offset`, and not a chord through the point's
    // two neighbours. The offset only names the fold line where it is
    // perpendicular to it. Where the offset also runs *along* the seam - a miter
    // tip, and every outer point the weld inserted by arc fraction -
    // `normal × offset` is tilted, the turn flips the offset's component along the
    // seam instead of leaving it, and the band is sheared: its rest lengths no
    // longer match the folded place. Measured on a 20 × 20 cm square with a 3 mm
    // allowance: the diagonal band spring `a0–i1` rests at 13.342 mm, folds to
    // 7.616 mm (-42.9 %), 60 of 326 band springs were compressed and the band sank
    // 3.16 mm through the piece. The chord halves that (3 springs, still
    // -3.1611 mm) because at the ring start it is the corner's bisector; the
    // outgoing edge leaves one spring (-28.1 %, the single-point corner) and
    // 0.3737 mm. The detailed note at the turn below has the numbers.
    //
    // A rotation, not a reflection: it keeps the component along the fold line,
    // tips the outward direction to inward and the surface normal to against the
    // piece, and leaves the band's winding intact - which the rest matrix and the
    // material direction both rely on.
    //
    // The whole band is turned by the same angle, so every fold edge comes out at
    // `π − FALT_START_ABWEICHUNG` whatever the band's width there. Lifting only
    // the cutting line off a flat fold instead would tilt each triangle by
    // `atan(lift / width)`: 4° across a 3 mm band but 69° across a 0.08 mm
    // sliver, which the strip has where a corner narrows.
    let mut pos: Vec<[f64; 3]> = Vec::with_capacity(punkte2.len());
    for &k in &quelle {
        if (k as usize) < n {
            // A welded seam-line vertex: its representative's place.
            pos.push(innen3[k as usize]);
            continue;
        }
        let j = k as usize - n;
        let q = innen3[j];
        let d = sub(lage(ti, s.aussen[j]), q);
        // The fold axis at `j` is the seam-line edge leaving `j`; the note above
        // the turn has the measured reason and the numbers.
        let achse = nahtrichtung(&innen3, nrm, j);
        let (tang, quer) = match achse {
            Some(t) => {
                let a = dot(d, t);
                (mul(t, a), sub(d, mul(t, a)))
            }
            None => ([0.0; 3], d),
        };
        let lq = dot(quer, quer).sqrt();
        if !(lq > 0.0) {
            // A seam-line point with no band width across the fold: it is already
            // on the fold line and folding it is the identity. Not a refusal, so
            // not counted - unless the offset really points out of the piece.
            let dn = dot(d, nrm).abs();
            if dn > 0.0 && dn >= dot(d, d).sqrt() * (1.0 - 1e-12) {
                // An offset that lies along the piece's normal has no direction
                // in the plane. Counted, and its fold edges report
                // `falz_falsche_seite`.
                f.bericht.falz_ohne_richtung += 1;
            }
            pos.push(add(q, d));
            continue;
        }
        let mhat = mul(quer, 1.0 / lq);
        // The tilt axis: the seam-line edge leaving the point. Where the seam line
        // has no direction there (this point and its successor are one place) the
        // incoming edge stands in, and failing that the bisector of the offset is
        // all there is, and it is what an axis had to come from.
        let u = match achse {
            Some(t) => t,
            None => {
                let u = cross(nrm, mhat);
                let l = dot(u, u).sqrt();
                if l > 0.0 { mul(u, 1.0 / l) } else { mhat }
            }
        };
        // The step short of half a turn, taken towards the piece's reverse so the
        // band lies *behind* the piece it is folded onto.
        let spitz = mul(cross(u, mhat), lq * FALT_START_ABWEICHUNG.sin());
        let spitz = if dot(spitz, nrm) < 0.0 {
            mul(spitz, -1.0)
        } else {
            spitz
        };
        pos.push(add(q, add(tang, add(mul(quer, -FALT_START_ABWEICHUNG.cos()), spitz))));
    }

    // Mass, rest matrix and material direction, by the same rules as a pattern
    // piece: the band is the same sheet of the same fabric.
    let rho = vg.material.flaechengewicht_kg_m2;
    let mut masse = vec![0.0f64; pos.len()];
    let mut entartet = 0usize;
    for d in &lokal {
        let (a, b, c) = (
            punkte2[d[0] as usize],
            punkte2[d[1] as usize],
            punkte2[d[2] as usize],
        );
        let fl = flaeche2(a, b, c).abs() * 1e-6; // mm² → m²
        for &i in d {
            masse[i as usize] += fl * skala * skala * rho / 3.0;
        }
        let q = |p: [f64; 2]| [p[0] * 1e-3 * skala, p[1] * 1e-3 * skala];
        let (qa, qb, qc) = (q(a), q(b), q(c));
        let (m00, m10, m01, m11) = (qb[0] - qa[0], qb[1] - qa[1], qc[0] - qa[0], qc[1] - qa[1]);
        let det = m00 * m11 - m01 * m10;
        let inv = if det != 0.0 {
            [m11 / det, -m01 / det, -m10 / det, m00 / det]
        } else {
            // No rest matrix to invert: the triangle keeps its slot with a zero
            // matrix and is counted, the way a pattern piece does not (it never
            // produces one - `vernetze` drops such triangles).
            entartet += 1;
            [0.0; 4]
        };
        f.dreiecke.push(d.map(|i| basis as u32 + i));
        f.ruhe_inv.push(inv);
        let p3 = [pos[d[0] as usize], pos[d[1] as usize], pos[d[2] as usize]];
        match crate::faser::im_dreieck(dir2, inv, p3) {
            Some(v) => f.faser.push(v),
            None => {
                f.bericht.faser_entartet += 1;
                f.faser.push(crate::faser::kante(p3).unwrap_or([0.0; 3]));
            }
        }
        f.bericht.flaeche_mm2 += fl * 1e6;
    }
    f.bericht.dreiecke_entartet += entartet;

    // Stretch springs over the band's own edges, with the fabric's stiffness.
    //
    // `asum` is the summed triangle area on the edge, so `asum / l²` is the
    // edge's shape measure; it is capped at [`HOECHSTE_FORM`] here. The strip's
    // sliver triangles reach 2.2 · 10⁹ there, and the stiffness that follows from
    // them is far above what the solver resolves. The cap uses the area such a
    // shape would give at this length and leaves every other edge as it is; it
    // changes no length, no point and no triangle.
    let bk = kanten(&punkte2, &lokal);
    let mut gedeckelt = 0usize;
    for (&(i, j), (asum, _)) in &bk {
        let l_mm = abstand2(punkte2[i as usize], punkte2[j as usize]);
        let l = l_mm * 1e-3;
        if l <= 0.0 {
            continue;
        }
        let form = asum / (l_mm * l_mm);
        let a = if form > HOECHSTE_FORM {
            gedeckelt += 1;
            HOECHSTE_FORM * l_mm * l_mm
        } else {
            *asum
        };
        let k = vg.material.dehnung_n_m * a * 1e-6 / (l * l);
        f.abstand
            .push(([basis as u32 + i, basis as u32 + j], l * skala, 1.0 / k));
    }
    f.bericht.kanten_form_gedeckelt += gedeckelt;

    // Holds: every band seam-line point stays on the piece's boundary, without
    // taking a degree of freedom away. A point that is a boundary vertex gets
    // one constraint of rest length 0; the solver turns that into a radial
    // `k·I` spring (`implizit::system_bauen`: `quer = 1.0` when `l0 = 0`). A
    // point inside a boundary edge gets two, one per edge end, each with the
    // distance it was built with as rest length.
    //
    // The stiffness is the boundary spring the seam line has there:
    // `dehnung_n_m · A / l²`, `A` the area of the piece's triangle on that edge
    // and `l` the edge's length. So the hold stays within the fabric it takes the
    // place of - the piece's edge spring sums the triangle areas of *both* sides
    // of the edge, so the hold is that spring's half.
    let randring: Vec<[f64; 2]> = t.punkte[..t.nrand.min(t.punkte.len())].to_vec();
    let rand: BTreeMap<(u32, u32), u32> = randkanten(t);
    // Stiffness per ring edge, indexed by its first vertex; 0 where the edge
    // carries no band triangle. The opposite point of a boundary edge is not
    // necessarily on the ring - it is the piece's own point, read from `t`.
    let mut randsteif: Vec<f64> = Vec::with_capacity(randring.len());
    for k in 0..randring.len() {
        let (a, b) = (k, (k + 1) % randring.len());
        let l = abstand2(randring[a], randring[b]);
        let fl = rand
            .get(&((a as u32).min(b as u32), (a as u32).max(b as u32)))
            .map(|&o| flaeche2(randring[a], randring[b], t.punkte[o as usize]).abs());
        match fl {
            Some(fl) if l > 0.0 && fl > 0.0 => {
                randsteif.push(vg.material.dehnung_n_m * fl * 1e-6 / (l * 1e-3).powi(2));
            }
            _ => {
                // No band triangle on this edge: the seam line has no fabric to
                // be held to there. Counted, and the point falls back to the
                // piece's boundary spring on its neighbouring edge.
                f.bericht.halte_ohne_kante += 1;
                randsteif.push(0.0);
            }
        }
    }
    let mut gesetzt = BTreeMap::new();
    for i in 0..n {
        let vi = vertreter[i];
        if gesetzt.insert(vi, ()).is_some() {
            continue;
        }
        let Some(&li) = id.get(&vi) else { continue };
        let p = s.innen[i];
        let (d, k, _) = projiziere(p, &randring);
        f.bericht.halte_abstand_mm = f.bericht.halte_abstand_mm.max(d);
        let (a, b) = (k, (k + 1) % randring.len());
        // On a vertex means on a vertex, not merely on the polyline: a point a
        // hundredth of a millimetre inside a long edge is held by that edge's two
        // ends, not welded to whichever end happens to be nearest.
        let auf_vertex = randpunkt_von(p, &randring);
        // A ring edge without a band triangle takes its neighbour's stiffness,
        // so the hold is never silently missing.
        let kk = match randsteif.get(k) {
            Some(&v) if v > 0.0 => v,
            _ => randsteif.iter().cloned().fold(0.0f64, f64::max).max(1e-9),
        };
        let eigen = basis as u32 + li;
        if let Some(r) = auf_vertex {
            // On a vertex: the outgoing ring edge carries the stiffness.
            f.abstand.push(([eigen, (g as u32) + r], 0.0, 1.0 / kk));
            f.bericht.halte += 1;
            f.bericht.halte_punkt += 1;
        } else {
            // Inside an edge: two holds, each with its built distance.
            for r in [a, b] {
                let d3 = sub(pos[li as usize], x[g + r]);
                f.abstand
                    .push(([eigen, (g + r) as u32], dot(d3, d3).sqrt(), 1.0 / kk));
            }
            f.bericht.halte += 2;
            f.bericht.halte_kante += 2;
            f.bericht.punkte_auf_kante += 1;
        }
    }

    // The fold line: one bending constraint per band seam-line edge that is a
    // boundary edge of the piece, rest angle 180°.
    //
    // Triangle 1 is the piece's triangle on the boundary edge, triangle 2 the
    // band's triangle on the other side, both in the order `cpu::dieder` wants:
    // (p0, p1, p2) and (p1, p0, p3). The band point of a fold edge is one
    // position with two vertices - one of the piece, one of the band - so the
    // constraint names the piece's pair and only p3 belongs to the band. The
    // holds above keep the two vertices together.
    let bandkanten_top: BTreeMap<(u32, u32), usize> = {
        let mut m = BTreeMap::new();
        for d in &lokal {
            for q in 0..3 {
                let (a, b) = (d[q], d[(q + 1) % 3]);
                *m.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        m
    };
    let dritte: BTreeMap<(u32, u32), u32> = {
        let mut m = BTreeMap::new();
        for d in &lokal {
            for q in 0..3 {
                let (a, b, o) = (d[q], d[(q + 1) % 3], d[(q + 2) % 3]);
                m.entry((a.min(b), a.max(b))).or_insert(o);
            }
        }
        m
    };
    for i in 0..n {
        let j = (i + 1) % n;
        let (vi, vj) = (vertreter[i], vertreter[j]);
        if vi == vj {
            // A miter: the seam line has no length here, so no fold edge.
            if abstand2(s.innen[i], s.aussen[i]) > 1e-6 {
                f.bericht.falzkanten_gehrung += 1;
            }
            continue;
        }
        let (Some(pa), Some(pb)) = (
            randpunkt_von(s.innen[i], &randring),
            randpunkt_von(s.innen[j], &randring),
        ) else {
            f.bericht.falzkanten_keine_randkante += 1;
            continue;
        };
        let key = (pa.min(pb), pa.max(pb));
        let Some(&o_a) = rand.get(&key) else {
            f.bericht.falzkanten_keine_randkante += 1;
            continue;
        };
        let (Some(&li), Some(&lj)) = (id.get(&vi), id.get(&vj)) else {
            f.bericht.falzkanten_ohne_band += 1;
            continue;
        };
        let bkey = (li.min(lj), li.max(lj));
        let Some(&zahl) = bandkanten_top.get(&bkey) else {
            f.bericht.falzkanten_ohne_band += 1;
            continue;
        };
        if zahl != 1 {
            f.bericht.falzkanten_ohne_band += 1;
            continue;
        }
        let Some(&o_b) = dritte.get(&bkey) else {
            f.bericht.falzkanten_ohne_band += 1;
            continue;
        };
        // Order the edge so that triangle 1 (the piece's) is positive-wound,
        // exactly like the piece's own bending constraints.
        let (p0, p1) = if flaeche2(
            t.punkte[pa as usize],
            t.punkte[pb as usize],
            t.punkte[o_a as usize],
        ) > 0.0
        {
            (pa, pb)
        } else {
            (pb, pa)
        };
        let q = [
            x[g + p0 as usize],
            x[g + p1 as usize],
            x[g + o_a as usize],
            pos[o_b as usize],
        ];
        let Some((theta, _)) = crate::cpu::dieder(q) else {
            f.bericht.falz_dreieck_fehlt += 1;
            continue;
        };
        winkel.push(theta);
        if theta.abs() < 0.5 * std::f64::consts::PI {
            f.bericht.falz_falsche_seite += 1;
            continue;
        }
        let l = abstand2(t.punkte[p0 as usize], t.punkte[p1 as usize]);
        let a12 = flaeche2(
            t.punkte[p0 as usize],
            t.punkte[p1 as usize],
            t.punkte[o_a as usize],
        )
        .abs()
            + flaeche2(
                punkte2[li as usize],
                punkte2[lj as usize],
                punkte2[o_b as usize],
            )
            .abs();
        if !(l > 0.0) || !(a12 > 0.0) {
            f.bericht.falz_dreieck_fehlt += 1;
            continue;
        }
        let kb = vg.material.biegung_nm * l * l / a12;
        let ruhe = if theta > 0.0 {
            std::f64::consts::PI
        } else {
            -std::f64::consts::PI
        };
        f.biegung.push((
            [
                (g + p0 as usize) as u32,
                (g + p1 as usize) as u32,
                (g + o_a as usize) as u32,
                basis as u32 + o_b,
            ],
            ruhe,
            1.0 / kb,
        ));
        f.bericht.falzkanten += 1;
    }
    let neu = pos.len();
    for _ in 0..neu {
        f.kollisionsabstand.push(vg.material.dicke_m / 2.0);
        f.reibung.push(vg.material.reibung);
    }
    f.punkte.extend(pos);
    f.masse.extend(masse);
    f.bericht.punkte += neu;
    f.bericht.dreiecke = f.dreiecke.len();
    f.teile.push(Teilinfo {
        name: format!("{} (Zugabe)", t.teil),
        punkte: basis..(basis + neu),
        dreiecke: d0..f.dreiecke.len(),
        vorgabe: vg,
        band_von: Some(ti),
    });
}
