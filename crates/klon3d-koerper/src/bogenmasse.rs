//! Shape targets for a horizontal section: apex distance, partial arcs,
//! width and depth.
//!
//! A circumference alone does not describe a cross-section. The same 92.67 cm
//! can be a circle, a flattened ellipse, or a section with a bust in front and
//! a flat back - and a garment fits one of them and not the others. Finding
//! 123 (issue #65) therefore adds five numbers per line: the apex distance
//! `D` between the two bust points, the front and back arcs `F` and `B`, the
//! width `W` and the depth `T`.
//!
//! Nothing is pinned: `F + B = P` holds for a closed band, so of the six
//! numbers only five carry information. [`Querschnitt`] shows which five those
//! are and that they are constructible: from `W`, `T`, `D` and `F` the outline
//! is fixed except for one free exponent on the back curve, and that exponent
//! is set so `B` matches.
//!
//! Everything here is in centimeters in the horizontal plane: `x` to the
//! right, `z` to the front, `y` constant.

use crate::massband::{abstand, huelle};
use crate::Punkt3;

/// Band below the front top in which the apex points are looked for, cm.
///
/// The apex is the point furthest **forward** on its own side of the body
/// axis, `|x| > 1` cm. That is the measurement the pattern side uses
/// (`bogen_seitenstueck.py`, `brustprofil_x.py`) and the one the acceptance
/// table of issue #65 is built from.
///
/// Not the widest point of the front: on a narrow, high bust - MakeHuman with
/// the maxcup target - the widest front points sit near the flank and give
/// `D = 23.45` where the reference is 16.27. The band is only needed to keep
/// float noise from electing a point on the bridge; on a real bust the
/// forward maximum is sharp anyway.
const APEX_BAND: f64 = 0.5;

/// Points closer to the body axis than this are not apex candidates, cm.
const APEX_MIN_X: f64 = 1.0;

/// How far from the widest `x` a hull point may lie and still count as part of
/// the side, cm.
///
/// The side is the widest point, and the section is split there. The only case
/// that needs averaging is a *vertical* piece of outline: where two neighbouring
/// hull points have the same `x` and differ only in `z`, the polygon's widest
/// edge is one straight wall and the split belongs in its middle, so the run's
/// length is shared between front and back.
///
/// The band therefore has to be at the level of float noise and nothing more.
/// It was once `0.05` cm, to "catch the same chord ends on two rings that
/// describe the same curve with slightly different vertices" - that is wrong,
/// and measurably so: the flank of a superellipse with `n = 2.7` decreases in
/// `x` by only `0.046` cm over `1.35` cm of `z`, so the band swallowed most of
/// the flank. The target section built to `F = 55.84` measured `57.19`, and
/// every front arc in the acceptance was 1.3 cm too long on both sides. A
/// gentle flank is not an ambiguous side: it has one widest point, and both
/// the built outline and the measured ring find it.
const SEITEN_BAND: f64 = 1.0e-9;

/// Sag of the bridge between the two apex points, cm.
///
/// Zero would put a kink at both apex points and make the apex band
/// ambiguous; a few millimeters are enough to keep it well defined.
pub const BRIDGE_SAG: f64 = 0.4;

/// Segments of the back curve. Used both for solving the exponent and for
/// building the outline, so the measured length matches the solved one.
const BACK_SEGMENTS: usize = 400;

/// Lowest exponent of the common-exponent superellipse, see
/// [`Querschnitt::umriss_glatt`]. `n = 2` is the ellipse.
const N_MIN: f64 = 2.0;

/// Highest exponent searched. Above the peak of `B(n)` (near `n ≈ 20`) the
/// curve falls again and would describe a section pointier than any body.
const N_MAX: f64 = 60.0;

/// The measurements of one convex horizontal section.
#[derive(Clone, Debug)]
pub struct Boegen {
    /// Closed hull length, cm (`P`).
    pub umfang: f64,
    /// Largest minus smallest `x`, cm (`W`).
    pub breite: f64,
    /// Largest minus smallest `z`, cm (`T`).
    pub tiefe: f64,
    /// Arc from one side point to the other across the front, cm (`F`).
    pub vorn: f64,
    /// The other arc, across the back, cm (`B`).
    pub hinten: f64,
    /// Straight-line distance between the two apex points, cm (`D`).
    ///
    /// The straight line, not the path over the bridge. On a real bust both
    /// agree to the millimeter; on a flat forehead the path would measure the
    /// bridge instead of the distance.
    pub brustpunkt: f64,
    /// Half the front arc minus the apex distance, cm (`S`).
    pub seite: f64,
}

/// Measures a convex hull. `hull` is closed, without a repeated last point.
///
/// Front and back are split at the points with the largest and the smallest
/// `x`; the front is the half with the larger mean `z`. That is the only
/// robust rule - a body can have its bust on either side of the file's `z`
/// axis (the MakeHuman base mesh has front and back reversed against CLO's
/// BaseMesh).
///
/// `None` when the hull is degenerate: fewer than four points, only one side
/// point, or a section that is not closed.
///
/// A section without a pair of breast points - a cylinder, the waist, the hip -
/// has `brustpunkt = 0`. That is not an error: there is no second lobe whose
/// distance could be measured, and `tiefe` alone describes the front.
pub fn boegen(hull: &[Punkt3]) -> Option<Boegen> {
    let m = hull.len();
    if m < 4 {
        return None;
    }

    let mut umfang = 0.0;
    for i in 0..m {
        umfang += abstand(hull[i], hull[(i + 1) % m]);
    }

    let (mut rechts, mut links) = (0usize, 0usize);
    for i in 0..m {
        if hull[i][0] > hull[rechts][0] {
            rechts = i;
        }
        if hull[i][0] < hull[links][0] {
            links = i;
        }
    }
    if rechts == links {
        return None;
    }

    // The two ways around the section, from one side to the other. The side is
    // the **middle of its flat run** (see [`SEITEN_BAND`]): on a vertical side
    // the hull reports two neighbouring points with nearly the same `x`, and
    // splitting at either of them moves a centimetre of arc between front and
    // back without the section having changed. Each half therefore gets half of
    // that run, and both halves together still carry the whole circumference.
    let (kum, _) = bogen_tafel(hull);
    let sr = seiten_mitte_arc(hull, &kum, rechts, SEITEN_BAND, 1.0);
    let sl = seiten_mitte_arc(hull, &kum, links, SEITEN_BAND, -1.0);
    let (weg1, weg2) = (
        bogen_stueck(hull, &kum, sr, sl),
        bogen_stueck(hull, &kum, sl, sr),
    );
    let mitte_z =
        |w: &[Punkt3]| w.iter().map(|p| p[2]).sum::<f64>() / w.len().max(1) as f64;
    let (vorn, hinten_i) = if mitte_z(&weg1) > mitte_z(&weg2) {
        (weg1, weg2)
    } else {
        (weg2, weg1)
    };
    let bogen =
        |w: &[Punkt3]| -> f64 { w.windows(2).map(|p| abstand(p[0], p[1])).sum() };

    // Apex: the point furthest forward on each side of the body axis, per the
    // measurement rule of issue #65 (`bogen_seitenstueck.py`,
    // `brustprofil_x.py`): among the front hull points with `|x| > APEX_MIN_X`
    // take the one with the largest `z` on the right and on the left.
    //
    // Not the widest point of the front: on MakeHuman with the maxcup target
    // the widest front points sit near the flank and give `D = 23.45` where
    // the reference section has 16.27. The band keeps the choice from landing
    // on the bridge, where every point is equally far forward.
    let z_top = vorn
        .iter()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    // A pair exists only where the section has two lobes: then the foremost
    // point of the whole front lies on a breast, off the midplane. A round or
    // elliptical section - cylinder, waist, hip - comes forward on the middle,
    // and there the "apex" would be an arbitrary pair of points on a smooth
    // arc: a cylinder of radius 12 reports 2.19 cm of bust point distance.
    // Its front arc is then one piece, and `brustpunkt` is zero.
    let mitte_vorn = vorn.iter().find(|p| p[2] >= z_top - 1.0e-9);
    let hat_paar = mitte_vorn.is_some_and(|p| p[0].abs() > APEX_MIN_X);
    let (mut ar, mut al) = (None, None);
    for p in vorn.iter().filter(|_| hat_paar) {
        if p[2] <= z_top - APEX_BAND || p[0].abs() <= APEX_MIN_X {
            continue;
        }
        if p[0] > 0.0 {
            if ar.map_or(true, |b: Punkt3| p[2] > b[2]) {
                ar = Some(*p);
            }
        } else if al.map_or(true, |b: Punkt3| p[2] > b[2]) {
            al = Some(*p);
        }
    }
    // Both must lie on their own side of the middle, otherwise the section has
    // no pair of apex points but a single nearest point.
    let brustpunkt = match (ar, al) {
        (Some(pr), Some(pl)) if pr[0] > 0.0 && pl[0] < 0.0 => abstand(pr, pl),
        // No pair: a round section. Zero is the honest value - there is no
        // second lobe to measure a distance between.
        _ => 0.0,
    };

    let vorn_laenge = bogen(&vorn);
    let hinten_laenge = bogen(&hinten_i);
    let (mut x_lo, mut x_hi) = (hull[0][0], hull[0][0]);
    let (mut z_lo, mut z_hi) = (hull[0][2], hull[0][2]);
    for p in hull {
        x_lo = x_lo.min(p[0]);
        x_hi = x_hi.max(p[0]);
        z_lo = z_lo.min(p[2]);
        z_hi = z_hi.max(p[2]);
    }

    Some(Boegen {
        umfang,
        breite: x_hi - x_lo,
        tiefe: z_hi - z_lo,
        vorn: vorn_laenge,
        hinten: hinten_laenge,
        brustpunkt,
        seite: 0.5 * (vorn_laenge - brustpunkt),
    })
}

/// Hulls the cut chain first, then measures it - the usual path.
pub fn aus_schnitt(kette: &[Punkt3], n: Punkt3) -> Option<Boegen> {
    let (h, _) = huelle(kette, n);
    boegen(&h)
}

/// Arc table of the closed hull: `kum[i]` is the distance from `hull[0]` to
/// `hull[i]` along the polygon; the last entry is the total length.
fn bogen_tafel(hull: &[Punkt3]) -> (Vec<f64>, f64) {
    let m = hull.len();
    let mut kum = Vec::with_capacity(m + 1);
    kum.push(0.0);
    for i in 0..m {
        kum.push(kum[i] + abstand(hull[i], hull[(i + 1) % m]));
    }
    let gesamt = *kum.last().unwrap_or(&0.0);
    (kum, gesamt)
}

/// The point at arc position `s` on the closed hull, cm from `hull[0]`.
fn punkt_bei(hull: &[Punkt3], kum: &[f64], s: f64) -> Punkt3 {
    let m = hull.len();
    let gesamt = kum[m];
    let s = s.rem_euclid(gesamt);
    let mut i = 0;
    while i + 1 < m && kum[i + 1] <= s {
        i += 1;
    }
    let d = kum[i + 1] - kum[i];
    let t = if d > 0.0 { (s - kum[i]) / d } else { 0.0 };
    let (p, q) = (hull[i], hull[(i + 1) % m]);
    [
        p[0] + t * (q[0] - p[0]),
        p[1] + t * (q[1] - p[1]),
        p[2] + t * (q[2] - p[2]),
    ]
}

/// Arc position of the middle of the flat run around `i`, cm from `hull[0]`.
///
/// See [`SEITEN_BAND`]: on a vertical side the run holds two neighbouring
/// points with nearly the same `x`; the section's division into a front and a
/// back half runs through the middle of that run, not through one end.
///
/// `nach_aussen` says which way the run continues from the extreme: at the
/// right side (largest `x`) the neighbours are at most `band` smaller, at the
/// left side (smallest `x`) at most `band` larger. Testing the wrong way
/// would take most of the circumference for one "side".
fn seiten_mitte_arc(hull: &[Punkt3], kum: &[f64], i: usize, band: f64, nach_aussen: f64) -> f64 {
    let m = hull.len();
    let weiter = |k: usize| nach_aussen * (hull[k][0] - hull[i][0]) >= -band;
    let mut a = i;
    let mut n = 0;
    while n < m - 1 && weiter((a + m - 1) % m) {
        a = (a + m - 1) % m;
        n += 1;
    }
    let mut b = i;
    n = 0;
    while n < m - 1 && weiter((b + 1) % m) {
        b = (b + 1) % m;
        n += 1;
    }
    let mut lauf = kum[b] - kum[a];
    if lauf < 0.0 {
        lauf += kum[m];
    }
    (kum[a] + lauf / 2.0).rem_euclid(kum[m])
}

/// The hull points from arc position `von` to `bis`, walking forwards, both
/// ends included and interpolated where a position falls inside an edge.
fn bogen_stueck(hull: &[Punkt3], kum: &[f64], von: f64, bis: f64) -> Vec<Punkt3> {
    let m = hull.len();
    let gesamt = kum[m];
    let von = von.rem_euclid(gesamt);
    let mut ende = bis.rem_euclid(gesamt);
    while ende <= von {
        ende += gesamt;
    }
    let mut w = vec![punkt_bei(hull, kum, von)];
    // Erste Ecke nach `von`, dann vorwärts bis `bis`.
    let mut k = 0;
    while k < m && kum[k] <= von + 1.0e-12 {
        k += 1;
    }
    let mut schleife = 0;
    while schleife < m {
        let mut s = kum[k % m];
        if s <= von {
            s += gesamt;
        }
        if s >= ende - 1.0e-12 {
            break;
        }
        w.push(hull[k % m]);
        k += 1;
        schleife += 1;
    }
    w.push(punkt_bei(hull, kum, bis));
    w
}
/// The five numbers that do not contradict each other, plus the sixth.
///
/// `vorn` and `hinten` are the two arcs, `brustpunkt` the apex distance. A
/// section that contains all of them is built by [`Querschnitt::umriss`].
#[derive(Clone, Copy, Debug)]
pub struct Querschnitt {
    pub breite: f64,
    pub tiefe: f64,
    pub brustpunkt: f64,
    pub vorn: f64,
    pub hinten: f64,
}

impl Querschnitt {
    /// Builds the outline from the five values.
    ///
    /// The construction is forced, not fitted: the side point sits at
    /// `±W/2`, the apex points at `±D/2`, and the front side pieces are
    /// `S = (F - L_b)/2` long, where `L_b` is the length of the bridge
    /// polyline - the bridge sags by [`BRIDGE_SAG`], so it is a few hundredths
    /// of a millimeter longer than the straight line between the apex points.
    /// That leaves exactly one free height for the bridge, and it follows from
    /// the right triangle `h = sqrt(S² - (W/2 - D/2)²)`: too high and the side
    /// pieces cannot reach, too low and `F` breaks. The remaining depth
    /// `T - h` goes to the back, whose curve is a superellipse
    /// `|x/A|^n + |z/b|^n = 1`; the exponent `n` is solved so that the back
    /// arc hits `B`.
    ///
    /// Returns the outline (closed, `y = 0`) and the exponent. `None` when
    /// the values cannot be built: `S` shorter than the horizontal offset
    /// between side point and apex (a body cannot have that), a non-positive
    /// back depth, or a back arc shorter than an ellipse gives.
    pub fn umriss(&self, schritte: usize) -> Option<(Vec<Punkt3>, f64)> {
        let a = self.breite / 2.0;
        let bp = self.brustpunkt / 2.0;
        if a <= 0.0 || bp <= 0.0 || self.vorn <= 0.0 {
            return None;
        }
        let m = schritte.max(4);
        let bruecke = brueckenpunkte(bp, 0.0, m);
        let s = 0.5 * (self.vorn - laenge(&bruecke));
        if s <= 0.0 {
            return None;
        }
        let ueber = a - bp;
        if s < ueber.abs() {
            return None;
        }
        let h = (s * s - ueber * ueber).sqrt();
        let b = self.tiefe - h;
        if b <= 0.0 {
            return None;
        }
        let n = self.ordnung(a, b)?;

        // Same sampling as the length above, now at the solved height, so the
        // measured front arc is the one that was built.
        let mut p = Vec::with_capacity(m + BACK_SEGMENTS + 3);
        p.push([a, 0.0, 0.0]);
        p.extend(brueckenpunkte(bp, h, m));
        p.push([-a, 0.0, 0.0]);
        // The back curve runs from (A, 0) to (-A, 0); reversed, the interior
        // points continue the outline from (-A, 0) back to (A, 0).
        let hinten = back_points(a, b, n, BACK_SEGMENTS);
        for k in (1..hinten.len() - 1).rev() {
            p.push(hinten[k]);
        }
        Some((p, n))
    }

    /// Builds a section that has **no** apex pair: waist, high hip, hip.
    ///
    /// The bridge construction above needs a bust point distance, and only the
    /// bust line has one. Everywhere else the section is one closed convex
    /// curve, and the cleanest family that still hits `W`, `T`, `F` and `B`
    /// exactly is two superellipse halves with a **common** exponent:
    /// `|x/A|^n + |z/h|^n = 1` in front, `|x/A|^n + |z/b|^n = 1` behind,
    /// `h + b = T`. Two unknowns (`n`, `h`), two arcs (`F`, `B`) - forced.
    ///
    /// `F` alone fixes `h` for a given `n` (the half arc grows monotonically
    /// with the depth), so what remains is one scalar equation `B(n) = hinten`.
    /// That equation is **not** monotone over the whole range: `B(n)` falls
    /// from `n = 1` to the ellipse at `n = 2`, then rises to a peak near
    /// `n ≈ 20` and falls again. The rising branch from the ellipse upwards is
    /// the one that describes a body - flesh does not get pointier than an
    /// ellipse - and it spans a wide range of back arcs. `None` below the
    /// ellipse and above the peak; both are real limitations, not bugs.
    pub fn umriss_glatt(&self, schritte: usize) -> Option<(Vec<Punkt3>, f64)> {
        let a = self.breite / 2.0;
        if a <= 0.0 || self.tiefe <= 0.0 || self.vorn <= 0.0 || self.hinten <= 0.0 {
            return None;
        }
        let m = schritte.max(4);
        let hinten_bei = |n: f64| -> Option<(f64, f64)> {
            let h = self.tiefe_front(a, n, m)?;
            Some((laenge(&back_points(a, self.tiefe - h, n, m)), h))
        };

        let (b_ellipse, _) = hinten_bei(N_MIN)?;
        if self.hinten < b_ellipse - 1.0e-9 {
            return None;
        }
        // Peak of B(n): log grid, then bisect on the rising branch.
        let (mut n_spitze, mut b_spitze) = (N_MIN, b_ellipse);
        for j in 0..=60 {
            let n = N_MIN * (N_MAX / N_MIN).powf(j as f64 / 60.0);
            match hinten_bei(n) {
                Some((b, _)) if b > b_spitze => {
                    b_spitze = b;
                    n_spitze = n;
                }
                _ => {}
            }
        }
        if self.hinten > b_spitze + 1.0e-9 {
            return None;
        }
        let (mut lo, mut hi) = (N_MIN, n_spitze);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            match hinten_bei(mid) {
                Some((b, _)) if b < self.hinten => lo = mid,
                _ => hi = mid,
            }
        }
        let n = 0.5 * (lo + hi);
        let (_, h) = hinten_bei(n)?;

        // Front half from (A, 0) over (0, +h) to (-A, 0); the back curve is
        // built towards -z and its interior points continue from (-A, 0).
        let mut p: Vec<Punkt3> = back_points(a, h, n, m)
            .into_iter()
            .map(|q| [q[0], 0.0, -q[2]])
            .collect();
        let hinten = back_points(a, self.tiefe - h, n, m);
        for k in (1..hinten.len() - 1).rev() {
            p.push(hinten[k]);
        }
        Some((p, n))
    }

    /// Front depth `h` at which the front half arc is `vorn`, for exponent `n`.
    fn tiefe_front(&self, a: f64, n: f64, m: usize) -> Option<f64> {
        let f = |h: f64| laenge(&back_points(a, h, n, m));
        if f(self.tiefe) < self.vorn - 1.0e-9 || f(1.0e-6) > self.vorn + 1.0e-9 {
            return None;
        }
        let (mut lo, mut hi) = (1.0e-6, self.tiefe);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if f(mid) < self.vorn {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(0.5 * (lo + hi))
    }

    /// Builds the outline: with an apex pair if there is one, else
    /// [`Self::umriss_glatt`].
    pub fn form(&self, schritte: usize) -> Option<(Vec<Punkt3>, f64)> {
        if self.brustpunkt > 0.0 {
            self.umriss(schritte)
        } else {
            self.umriss_glatt(schritte)
        }
    }

    /// Exponent of the back curve that makes the back arc equal `hinten`.
    fn ordnung(&self, a: f64, b: f64) -> Option<f64> {
        let ziel = self.hinten;
        if ziel <= 0.0 {
            return None;
        }
        if laenge(&back_points(a, b, 2.0, BACK_SEGMENTS)) > ziel {
            return None;
        }
        let (mut lo, mut hi) = (2.0, 2.0);
        loop {
            if laenge(&back_points(a, b, hi, BACK_SEGMENTS)) >= ziel {
                break;
            }
            if hi >= 500.0 {
                return None;
            }
            hi *= 1.5;
        }
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if laenge(&back_points(a, b, mid, BACK_SEGMENTS)) < ziel {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(0.5 * (lo + hi))
    }
}

/// Bridge of the front outline, from `(bp, h)` to `(-bp, h)`, `m + 1`
/// points.
///
/// Parabola sagging by [`BRIDGE_SAG`] in the middle. Zero sag would put a kink
/// at both apex points; the length does not depend on `h`, so it can be
/// measured before the height is known.
fn brueckenpunkte(bp: f64, h: f64, m: usize) -> Vec<Punkt3> {
    (0..=m)
        .map(|k| {
            let u = k as f64 / m as f64;
            [
                bp * (1.0 - 2.0 * u),
                0.0,
                (h - BRIDGE_SAG) + BRIDGE_SAG * (2.0 * u - 1.0).powi(2),
            ]
        })
        .collect()
}

/// Back curve `|x/A|^n + |z/b|^n = 1`, from `(A, 0)` through `(0, -b)` to
/// `(-A, 0)`, `m + 1` points.
fn back_points(a: f64, b: f64, n: f64, m: usize) -> Vec<Punkt3> {
    let mut p = Vec::with_capacity(m + 1);
    for k in 0..=m {
        let t = k as f64 / m as f64;
        let (c, s) = ((std::f64::consts::PI * t).cos(), (std::f64::consts::PI * t).sin());
        p.push([
            a * c.signum() * c.abs().powf(2.0 / n),
            0.0,
            -b * s.abs().powf(2.0 / n),
        ]);
    }
    p
}

/// Length of an open polyline, cm.
fn laenge(p: &[Punkt3]) -> f64 {
    p.windows(2).map(|w| abstand(w[0], w[1])).sum()
}

/// Where a ray from the section's centre leaves the outline, cm.
///
/// `richtung` is a unit vector in the plane `(x, z)`, the outline is closed and
/// convex. `None` when the ray misses - a section that does not surround its
/// own centre has no radial description.
pub fn strahlaustritt(umriss: &[Punkt3], richtung: (f64, f64)) -> Option<f64> {
    let (dx, dz) = richtung;
    let m = umriss.len();
    if m < 3 {
        return None;
    }
    let mut weit = f64::NEG_INFINITY;
    for i in 0..m {
        let a = umriss[i];
        let b = umriss[(i + 1) % m];
        let (ex, ez) = (b[0] - a[0], b[2] - a[2]);
        // Ray `t · d` against the edge `a + s · e`, `s` in [0, 1].
        let nenner = dx * ez - dz * ex;
        if nenner.abs() < 1.0e-12 {
            continue;
        }
        let t = (a[0] * ez - a[2] * ex) / nenner;
        let s = (a[0] * dz - a[2] * dx) / nenner;
        if (0.0..=1.0).contains(&s) && t > 0.0 {
            weit = weit.max(t);
        }
    }
    (weit > 0.0).then_some(weit)
}

/// Area centroid of a closed section in the plane, cm.
///
/// Not the vertex centroid: that one depends on how finely an arc happens to be
/// sampled, so subdividing a section ([`verfeinern`]) moves it and the whole
/// radial map with it - measured on the acceptance case that alone costs 0.3 cm
/// of bust point distance. The area centroid of a polygon is invariant under
/// subdivision, which is what a centre of a *shape* has to be.
///
/// Falls back to the vertex centroid when the section encloses no area.
fn mitte(p: &[Punkt3]) -> (f64, f64) {
    let n = p.len().max(1) as f64;
    let eckig = (
        p.iter().map(|q| q[0]).sum::<f64>() / n,
        p.iter().map(|q| q[2]).sum::<f64>() / n,
    );
    let m = p.len();
    if m < 3 {
        return eckig;
    }
    let (mut a, mut cx, mut cz) = (0.0, 0.0, 0.0);
    for i in 0..m {
        let q = p[i];
        let r = p[(i + 1) % m];
        let kreuz = q[0] * r[2] - r[0] * q[2];
        a += kreuz;
        cx += (q[0] + r[0]) * kreuz;
        cz += (q[2] + r[2]) * kreuz;
    }
    // Doppelte Fläche; unter der Rastergrösse ist der Querschnitt entartet.
    if a.abs() < 1.0e-9 {
        return eckig;
    }
    (cx / (3.0 * a), cz / (3.0 * a))
}

/// Directions of the target outline of [`Querschnitt::form`]. One per 1.5°,
/// which resolves the steep flank between side point and apex (56° wide) with
/// 37 points.
pub const STUETZEN: usize = 240;

/// Length of a closed polyline in vertex order, cm.
///
/// [`huelle`] would be wrong here: it removes points depending on winding, and a
/// ring from a mesh may come in either direction. The section is measured in the
/// order it is given.
fn geschlossene_laenge(p: &[Punkt3]) -> f64 {
    let m = p.len();
    if m < 2 {
        return 0.0;
    }
    (0..m).map(|i| abstand(p[i], p[(i + 1) % m])).sum()
}

/// Rounds of [`auf_umriss`].
///
/// The two steps fight each other: the radial map sets the radii and leaves the
/// circumference a few tenths short, the scaling closes the circumference and
/// moves the radii. The pair converges - the scaling factor falls below 1e-5
/// after eight rounds - so eight is enough and cheap (the ray cast is over the
/// target only, the ring is one pass per point).
pub const UMRISS_RUNDEN: usize = 8;

/// Longest edge a section may carry into [`auf_umriss`], cm.
///
/// A polygon cannot take on a shape with sharper corners than its own vertex
/// spacing resolves. The target bust has one: the apex corner sits in a 6° gap
/// of the lip width, and a MakeHuman torso ring has no vertex there (between
/// 47.4° and 53.7° it has none at all). The radial map then bridges the corner
/// and the section comes out 3 cm deep instead of 22 - the corners are not
/// cut by the support angles, they are cut by the ring.
///
/// Subdividing the edges to 0.5 cm before the rounds is the first half of the
/// fix; the corners themselves are inserted separately ([`ecken`]), because no
/// reasonable spacing hits a corner angle by accident.
pub const VERFEINERUNG: f64 = 0.5;

/// Turning angle from which a vertex of the target counts as a corner, rad.
///
/// The target bust has 1.2 rad at the apex (the flank meets the bridge). A
/// smooth superellipse flank stays below 0.2 rad per vertex at the sampling
/// used here, so 0.35 separates the two without a magic number per section.
const ECKE: f64 = 0.35;

/// Subdivides every edge of a closed section to at most `schritt` cm.
fn verfeinern(p: &[Punkt3], schritt: f64) -> Vec<Punkt3> {
    let m = p.len();
    if m < 2 || !(schritt > 0.0) {
        return p.to_vec();
    }
    let mut out = Vec::with_capacity(m * 2);
    for i in 0..m {
        let a = p[i];
        let b = p[(i + 1) % m];
        let d = ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        let k = ((d / schritt).ceil() as usize).max(1);
        for j in 0..k {
            let t = j as f64 / k as f64;
            out.push([a[0] + t * (b[0] - a[0]), a[1], a[2] + t * (b[2] - a[2])]);
        }
    }
    out
}

/// Corners of the target: direction from its own centre (rad) and the point.
///
/// Two kinds of point have to be a vertex of the mapped section, and they are
/// found by two different rules.
///
/// 1. **Turning corners.** The radius function of a section jumps at a corner:
///    just left of the target apex the outline is 1.0 cm nearer the centre than
///    just right of it. A polygon can only represent that jump with a vertex
///    exactly there, and the measured ring never has one by chance.
/// 2. **The two side points**, at the extreme `x`. They are corners of the
///    construction as well - the straight front side piece meets the back curve
///    there at about 10° - but that is far below [`ECKE`], so the turn test does
///    not see them. They must be a vertex anyway, because [`boegen`] splits
///    front and back at the **widest point**, and on a nearly flat flank the
///    widest point of a polygon is simply whichever vertex ends last.
///
/// The second rule is not a refinement, it is the difference between a correct
/// and a wrong measurement. The target bust holds `x = 14.080` from its own
/// side point up to 0.8 cm into the back curve; without a vertex there the
/// 145-point ring put its widest point 0.86 cm further back and moved those
/// 0.8 cm of arc from `hinten` to `vorn`. It cannot be fixed by ring density:
/// subdividing the polygon keeps its shape and only moves the vertices, and the
/// error stayed at exactly 0.8 cm from 145 up to 2 320 points.
///
/// These are what [`auf_umriss`] inserts into the ring before mapping it, and
/// where it pins them afterwards.
fn ecken(ziel: &[Punkt3]) -> Vec<(f64, Punkt3)> {
    let m = ziel.len();
    if m < 4 {
        return Vec::new();
    }
    let (cx, cz) = mitte(ziel);
    let mut w = Vec::new();
    for i in 0..m {
        let p = ziel[i];
        let (a, b) = (ziel[(i + m - 1) % m], ziel[(i + 1) % m]);
        let (u, v) = ([p[0] - a[0], p[2] - a[2]], [b[0] - p[0], b[2] - p[2]]);
        let (lu, lv) = (
            (u[0] * u[0] + u[1] * u[1]).sqrt(),
            (v[0] * v[0] + v[1] * v[1]).sqrt(),
        );
        if lu < 1.0e-12 || lv < 1.0e-12 {
            continue;
        }
        let kreuz = u[0] * v[1] - u[1] * v[0];
        let skalar = u[0] * v[0] + u[1] * v[1];
        if kreuz.atan2(skalar).abs() >= ECKE {
            w.push(((p[2] - cz).atan2(p[0] - cx), p));
        }
    }

    // Die beiden Seitenpunkte. Bei Gleichstand gewinnt der erste Punkt, und ein
    // schon gefundener Eckenwinkel wird nicht doppelt eingesetzt: `ecken` darf
    // je Richtung nur einmal in `ecken_einfuegen` landen, sonst sitzen zwei
    // Punkte auf derselben Kante.
    let (mut rechts, mut links) = (0usize, 0usize);
    for i in 0..m {
        if ziel[i][0] > ziel[rechts][0] {
            rechts = i;
        }
        if ziel[i][0] < ziel[links][0] {
            links = i;
        }
    }
    for i in [rechts, links] {
        let p = ziel[i];
        let winkel = (p[2] - cz).atan2(p[0] - cx);
        if w.iter().any(|(b, _)| (b - winkel).abs() < 1.0e-6) {
            continue;
        }
        w.push((winkel, p));
    }

    w.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    w
}

/// The corner directions of a target outline, rad - what [`ecken`] found.
///
/// The caller needs them to put a vertex at the same directions into a *mesh*
/// ring ([`crate::anpassen::formen`]): [`ecken_einfuegen`] only shapes a section
/// in memory, and the mesh keeps its own vertices. Directions are measured from
/// the section's own area centroid and are therefore comparable between the
/// target and the measured ring - [`auf_umriss`] places a point of direction `w`
/// at the target's radius in exactly that direction.
pub fn ecken_richtungen(ziel: &[Punkt3]) -> Vec<f64> {
    ecken(ziel).into_iter().map(|(w, _)| w).collect()
}

/// The area centroid of a section, cm - the centre [`Versatz`] measures from.
pub fn flaechen_mitte(p: &[Punkt3]) -> (f64, f64) {
    mitte(p)
}

/// Inserts a vertex at each of `ecken` into the closed section `p`.
///
/// The new point sits on the boundary edge that the ray from the centroid in
/// the corner's direction crosses **nearest** - the same ray test as
/// [`strahlaustritt`], so the section may run clockwise or counter-clockwise
/// and the result is the same. (Searching for "the edge whose two vertex angles
/// bracket this direction" is not winding-independent, and on a clockwise ring
/// it silently drops one of the two apex corners.)
///
/// A vertex at exactly the corner direction is all that is needed: the radial
/// map then gives it exactly the corner radius.
///
/// A direction that already carries a vertex is left alone. Without that test
/// the caller that puts the same directions into the *mesh* ring first
/// ([`crate::anpassen::formen`]) would get two points on one spot: this map
/// would split the edge right next to the existing corner and leave a
/// zero-length edge, which the ray cast of [`Versatz`] cannot resolve.
///
/// The tolerance is a float-noise band, not a width: a vertex inserted into the
/// mesh at `w` and a vertex measured at `w` from the same centroid differ by
/// `1e-16` rad. A vertex one degree away is a different point.
const ECKE_GLEICH: f64 = 1.0e-6;

fn ecken_einfuegen(p: &[Punkt3], ecken: &[(f64, Punkt3)]) -> Vec<Punkt3> {
    let mut out: Vec<Punkt3> = p.to_vec();
    if ecken.is_empty() || p.len() < 3 {
        return out;
    }
    let (cx, cz) = mitte(p);
    for &(w, _) in ecken {
        let vorhanden = out.iter().any(|q| {
            let d = ((q[2] - cz).atan2(q[0] - cx) - w).rem_euclid(std::f64::consts::TAU);
            d.min(std::f64::consts::TAU - d) < ECKE_GLEICH
        });
        if vorhanden {
            continue;
        }
        let m = out.len();
        let (dx, dz) = (w.cos(), w.sin());
        // Nächster Schnittpunkt des Strahls mit einer Kante.
        let mut best: Option<(f64, usize, f64)> = None;
        for i in 0..m {
            let a = out[i];
            let b = out[(i + 1) % m];
            let (ex, ez) = (b[0] - a[0], b[2] - a[2]);
            let nenner = dx * ez - dz * ex;
            if nenner.abs() < 1.0e-12 {
                continue;
            }
            let (ax, az) = (a[0] - cx, a[2] - cz);
            let t = (ax * ez - az * ex) / nenner;
            let s = (ax * dz - az * dx) / nenner;
            if !(0.0..=1.0).contains(&s) || !(t > 1.0e-9) {
                continue;
            }
            if best.map_or(true, |(bt, _, _)| t < bt) {
                best = Some((t, i, s));
            }
        }
        let Some((_, i, s)) = best else { continue };
        let (a, b) = (out[i], out[(i + 1) % m]);
        out.insert(i + 1, [a[0] + s * (b[0] - a[0]), a[1], a[2] + s * (b[2] - a[2])]);
    }
    out
}

/// Carries a measured section onto a target outline, in centimetres.
///
/// Two steps per round, and both are needed:
///
/// 1. **Radii**: every point keeps its direction from the centroid and takes
///    the target's radius in that direction. Width, depth and the apex pair
///    follow. Done per point, not over a fixed set of support angles: a target
///    corner (the bust apex, a 1.2 rad jump in the radius function) falls
///    between any two supports and an interpolated field bridges it - measured
///    on the target bust that loses 0.6 cm of apex distance out of 22.
/// 2. A uniform scaling about the area centroid closes the circumference. One
///    pass is not enough (the radii leave `P` a few tenths short), and the
///    scaling moves the radii again, so the two repeat until the factor is 1.
///
/// The input is first subdivided to [`VERFEINERUNG`] and gets a vertex at every
/// target corner ([`ecken`], [`ecken_einfuegen`]): a section without a vertex
/// on the apex corner cannot take the corner on, and the MakeHuman torso ring
/// has a 6° gap exactly there.
///
/// The result stays in the frame of `ist`: only the centroid is used, and it is
/// the section's own. A body section is not centred on the file axis, and the
/// offset is not a shape error.
///
/// `None` when the section has no radial description (it does not surround its
/// own centroid) or when it collapses.
pub fn auf_umriss(ist: &[Punkt3], ziel: &[Punkt3], umfang: f64, runden: usize) -> Option<Vec<Punkt3>> {
    if ist.len() < 3 || ziel.len() < 3 || !(umfang > 0.0) || !umfang.is_finite() {
        return None;
    }
    let ecken = ecken(ziel);
    let mut p: Vec<Punkt3> = ecken_einfuegen(&verfeinern(ist, VERFEINERUNG), &ecken);
    // Das Ziel in sein eigenes Zentrum legen: die Radien kommen aus dem Umriss,
    // die Lage aus `ist`.
    let (zx, zz) = mitte(ziel);
    let ziel_m: Vec<Punkt3> = ziel
        .iter()
        .map(|q| [q[0] - zx, 0.0, q[2] - zz])
        .collect();

    for _ in 0..runden.max(1) {
        let (cx, cz) = mitte(&p);
        let mut neu = Vec::with_capacity(p.len());
        for q in &p {
            let (dx, dz) = (q[0] - cx, q[2] - cz);
            let r = (dx * dx + dz * dz).sqrt();
            if r < 1.0e-12 {
                neu.push(*q);
                continue;
            }
            let Some(soll) = strahlaustritt(&ziel_m, (dx / r, dz / r)) else {
                return None;
            };
            neu.push([cx + dx / r * soll, q[1], cz + dz / r * soll]);
        }
        p = neu;
        let laenge = geschlossene_laenge(&p);
        if !(laenge > 1.0e-9) {
            return None;
        }
        let k = umfang / laenge;
        if !k.is_finite() {
            return None;
        }
        if (k - 1.0).abs() < 1.0e-9 {
            break;
        }
        let (cx, cz) = mitte(&p);
        for q in &mut p {
            q[0] = cx + (q[0] - cx) * k;
            q[2] = cz + (q[2] - cz) * k;
        }
    }

    Some(p)
}

/// Per-angle displacement that carries `ist` onto `neu`, cm.
///
/// Both are closed sections of the same kind (`y` ignored); `neu` is what
/// [`auf_umriss`] returned for `ist`. Measured along the rays from **one**
/// centre - the area centroid of `ist` - not from each section's own: this
/// field is applied to a whole mesh, and a mesh has one centre per section.
///
/// Evaluated exactly, by ray casting into both sections. Sampling the
/// displacement at a fixed set of support angles and interpolating between them
/// is cheaper and wrong: the target's apex corner is a jump in the radius
/// function, and any interpolated field bridges it - measured on the real
/// MakeHuman torso that costs 2.0 cm of the 22 cm bust point distance.
#[derive(Clone, Debug)]
pub struct Versatz {
    /// The centre the field belongs to, cm.
    pub mitte: (f64, f64),
    /// The measured section around `mitte`, cm.
    ist: Vec<Punkt3>,
    /// The shaped section around `mitte`, cm.
    neu: Vec<Punkt3>,
    /// How much of the displacement applies, 1 is all of it.
    skala: f64,
}

impl Versatz {
    /// The displacement at an arbitrary angle, cm.
    pub fn bei(&self, winkel: f64) -> f64 {
        if self.skala == 0.0 {
            return 0.0;
        }
        let richtung = (winkel.cos(), winkel.sin());
        match (
            strahlaustritt(&self.neu, richtung),
            strahlaustritt(&self.ist, richtung),
        ) {
            (Some(neu), Some(ist)) => self.skala * (neu - ist),
            _ => 0.0,
        }
    }

    /// The displacement at a point of the plane, cm.
    pub fn am(&self, p: Punkt3) -> f64 {
        self.bei((p[2] - self.mitte.1).atan2(p[0] - self.mitte.0))
    }

    /// Multiplies the whole field - how it fades out above and below the
    /// shaped band.
    pub fn mal(&self, faktor: f64) -> Versatz {
        Versatz {
            mitte: self.mitte,
            ist: self.ist.clone(),
            neu: self.neu.clone(),
            skala: self.skala * faktor,
        }
    }
}

/// The field of [`Versatz`] between two sections, cm.
pub fn versatz_feld(ist: &[Punkt3], neu: &[Punkt3]) -> Option<Versatz> {
    if ist.len() < 3 || neu.len() < 3 {
        return None;
    }
    let mitte = mitte(ist);
    let (cx, cz) = mitte;
    let a: Vec<Punkt3> = ist
        .iter()
        .map(|q| [q[0] - cx, 0.0, q[2] - cz])
        .collect();
    let b: Vec<Punkt3> = neu
        .iter()
        .map(|q| [q[0] - cx, 0.0, q[2] - cz])
        .collect();
    // Both sections have to surround the shared centre, otherwise the field has
    // no value anywhere and the caller must not apply it.
    strahlaustritt(&a, (1.0, 0.0))?;
    strahlaustritt(&b, (1.0, 0.0))?;
    Some(Versatz {
        mitte,
        ist: a,
        neu: b,
        skala: 1.0,
    })
}

#[cfg(test)]
mod pruefungen {
    use super::*;

    /// The acceptance values of issue #65 at the bust line, cm.
    fn soll() -> Querschnitt {
        Querschnitt {
            breite: 28.16,
            tiefe: 24.47,
            brustpunkt: 22.0,
            vorn: 56.0,
            hinten: 36.67,
        }
    }

    #[test]
    fn konstruierter_querschnitt_trifft_alle_sechs() {
        let q = soll();
        let (umriss, n) = q.umriss(200).expect("Querschnitt ist baubar");
        let b = boegen(&umriss).expect("Umriss hat Brustpunkte");
        let tol = |ist: f64, ziel: f64| (ist - ziel).abs();
        assert!(tol(b.breite, q.breite) < 1.0e-6, "W {}", b.breite);
        assert!(tol(b.tiefe, q.tiefe) < 1.0e-6, "T {}", b.tiefe);
        assert!(tol(b.brustpunkt, q.brustpunkt) < 1.0e-6, "D {}", b.brustpunkt);
        assert!(tol(b.vorn, q.vorn) < 0.01, "F {}", b.vorn);
        assert!(tol(b.hinten, q.hinten) < 0.01, "B {}", b.hinten);
        // F + B = P is a property of a closed band, not a coincidence.
        assert!(tol(b.umfang, q.vorn + q.hinten) < 1.0e-9, "P {}", b.umfang);
        assert!((2.0..10.0).contains(&n), "Exponent {n}");
        assert!(tol(b.seite, 0.5 * (q.vorn - q.brustpunkt)) < 0.01);
    }

    #[test]
    fn unbaubarer_querschnitt_wird_abgelehnt() {
        // 56 cm front arc with only 28.16 cm width: the side piece would have
        // to be shorter than the horizontal offset (14.08 - 11.00 = 3.08).
        let mut q = soll();
        q.vorn = 27.0;
        assert!(q.umriss(200).is_none());

        // Back arc shorter than an ellipse through the same axes.
        let mut q = soll();
        q.hinten = 20.0;
        assert!(q.umriss(200).is_none());

        // No depth left for the back.
        let mut q = soll();
        q.tiefe = 10.0;
        assert!(q.umriss(200).is_none());
    }

    /// The apex is the widest point in the band, not the one furthest
    /// forward. On the flat bridge of the target section `argmax z` returns
    /// the middle of the bridge and `D` comes out 10.22 instead of 22.00.
    #[test]
    fn flache_stirn_liefert_den_brustpunktabstand() {
        let q = soll();
        let (umriss, _) = q.umriss(200).expect("Querschnitt ist baubar");
        // The bridge really is 22 cm long - the two droop points are 11 cm
        // apart in x each.
        let b = boegen(&umriss).expect("Umriss hat Brustpunkte");
        assert!((b.brustpunkt - 22.0).abs() < 1.0e-6);
        // A wrong rule (furthest forward) would give the sag only.
        assert!(b.brustpunkt > 20.0);
    }

    /// Front and back follow the mean `z` of the two halves, not the order of
    /// the points: the same section measured the other way round must give the
    /// same arcs.
    #[test]
    fn umlaufsinn_aendert_nichts() {
        let q = soll();
        let (umriss, _) = q.umriss(120).expect("Querschnitt ist baubar");
        let vorwaerts = boegen(&umriss).expect("messbar");
        let rueckwaerts: Vec<Punkt3> = umriss.iter().rev().copied().collect();
        let anders = boegen(&rueckwaerts).expect("messbar");
        assert!((vorwaerts.vorn - anders.vorn).abs() < 1.0e-9);
        assert!((vorwaerts.hinten - anders.hinten).abs() < 1.0e-9);
        assert!((vorwaerts.brustpunkt - anders.brustpunkt).abs() < 1.0e-9);
    }

    /// A section without a bust pair - a circle - is measured as one front
    /// arc and reports **no** bust point distance at all.
    ///
    /// The old rule took the widest points per side and reported 2.19 cm of
    /// "bust point distance" on a radius-12 cylinder; that is not a measurement
    /// but an artefact of looking for a pair where there is none. `D = 0` says
    /// the same thing the section says: one lobe, no gap between two.
    #[test]
    fn runde_seite_hat_keinen_brustpunktabstand() {
        let mut p = Vec::new();
        for k in 0..360 {
            let w = 2.0 * std::f64::consts::PI * k as f64 / 360.0;
            p.push([10.0 * w.cos(), 0.0, 10.0 * w.sin()]);
        }
        let b = boegen(&p).expect("Kreis ist messbar");
        assert!((b.umfang - 2.0 * std::f64::consts::PI * 10.0).abs() < 0.01);
        assert!((b.vorn - b.hinten).abs() < 0.01);
        assert_eq!(b.brustpunkt, 0.0);
        assert!((b.seite - 0.5 * b.vorn).abs() < 1.0e-9);
        assert!((b.breite - 20.0).abs() < 0.01);
    }

    /// The working path of issue #65: a section that is far off (an ellipse of
    /// MakeHuman's bust size, no apex pair) is carried onto the target outline,
    /// and then measures all six acceptance values.
    ///
    /// This is the check that "radial does not work" from the old scratchpad
    /// runs is wrong. It used to assert that a single radial map over fixed
    /// support angles stays short of `P` - that map is gone, because the
    /// support angles are exactly what cut the apex corner. What stays is the
    /// part that matters: the ring is carried onto the target and measures the
    /// acceptance values.
    #[test]
    fn radiale_verschiebung_trifft_die_sechs_ziele() {
        let q = soll();
        let (ziel, _) = q.umriss(200).expect("Querschnitt ist baubar");

        // Start: an ellipse without an apex pair, off-centre.
        let ist = |n: usize| -> Vec<Punkt3> {
            (0..n)
                .map(|k| {
                    let w = std::f64::consts::TAU * k as f64 / n as f64;
                    [1.3 + 15.725 * w.cos(), 0.0, -0.8 + 11.81 * w.sin()]
                })
                .collect()
        };

        // All six inside the acceptance band. The real MakeHuman torso ring has
        // ~90 points, so 90 is the case that matters.
        for n in [90usize, 120, 180] {
            let geformt = auf_umriss(&ist(n), &ziel, q.vorn + q.hinten, UMRISS_RUNDEN)
                .expect("auf den Umriss");
            let b = boegen(&geformt).expect("geformte Naht ist messbar");
            let nah = |ist: f64, ziel: f64, tol: f64, was: &str| {
                assert!(
                    (ist - ziel).abs() <= tol,
                    "{was} bei {n} Punkten: {ist:.3} statt {ziel:.3}"
                );
            };
            nah(b.umfang, q.vorn + q.hinten, 0.2, "P");
            nah(b.breite, q.breite, 0.5, "W");
            nah(b.tiefe, q.tiefe, 0.5, "T");
            nah(b.vorn, q.vorn, 1.0, "F");
            nah(b.hinten, q.hinten, 1.0, "B");
            nah(b.brustpunkt, q.brustpunkt, 0.5, "D");
        }
    }

    /// A very coarse ring still reaches the apex - because the section is
    /// subdivided and gets a vertex at the target's corners before it is
    /// mapped, not because the ring happens to have a point there.
    ///
    /// This was the opposite before: with a fixed set of support angles the
    /// radial map interpolated across the apex corner, and a 24-point ring came
    /// out 3 cm deep instead of 22. The test stays as the boundary marker: a
    /// section coarser than the shape it must take on is where the method
    /// breaks, and 24 points is well inside what a mesh ring carries
    /// (MakeHuman's torso section has about 90).
    #[test]
    fn auch_ein_grober_ring_trifft_den_brustpunktabstand() {
        let q = soll();
        let (ziel, _) = q.umriss(200).expect("Querschnitt ist baubar");
        let ist: Vec<Punkt3> = (0..24)
            .map(|k| {
                let w = std::f64::consts::TAU * k as f64 / 24.0;
                [1.3 + 15.725 * w.cos(), 0.0, -0.8 + 11.81 * w.sin()]
            })
            .collect();
        let geformt = auf_umriss(&ist, &ziel, q.vorn + q.hinten, UMRISS_RUNDEN)
            .expect("auf den Umriss");
        let b = boegen(&geformt).expect("messbar");
        assert!(
            (b.brustpunkt - q.brustpunkt).abs() <= 0.5,
            "D bei 24 Punkten: {:.3} statt {:.3}",
            b.brustpunkt,
            q.brustpunkt
        );
        assert!(
            (b.umfang - (q.vorn + q.hinten)).abs() <= 0.2,
            "P bei 24 Punkten: {:.3}",
            b.umfang
        );
    }

    /// The field has one value per direction and it wraps around the circle:
    /// the same angle read as `-π` and as `+π` gives the same displacement.
    ///
    /// Also the exactness the interpolated predecessors lacked: at a direction
    /// where the target's radius jumps (a target corner), the field reports the
    /// jump, not an average across it.
    #[test]
    fn versatz_feld_ist_umlaufend_und_trifft_die_ecke() {
        let q = soll();
        let (ziel, _) = q.umriss(200).expect("baubar");
        let ist: Vec<Punkt3> = (0..180)
            .map(|k| {
                let w = std::f64::consts::TAU * k as f64 / 180.0;
                [1.3 + 15.725 * w.cos(), 0.0, -0.8 + 11.81 * w.sin()]
            })
            .collect();
        let neu = auf_umriss(&ist, &ziel, q.vorn + q.hinten, UMRISS_RUNDEN).expect("geformt");
        let feld = versatz_feld(&ist, &neu).expect("Feld");

        // Wraps: an angle and the same angle plus a full turn agree.
        let tau = std::f64::consts::TAU;
        for w in [0.0, 0.7, 2.9, -1.4] {
            assert!((feld.bei(w) - feld.bei(w + tau)).abs() < 1.0e-12, "Umlauf bei {w}");
            assert!((feld.bei(w) - feld.bei(w - tau)).abs() < 1.0e-12, "Umlauf bei {w}");
        }

        // No movement where the two sections agree: the field of a section with
        // itself is zero everywhere.
        let eigen = versatz_feld(&ist, &ist).expect("Feld");
        for w in [0.0, 1.0, 2.5, -2.0] {
            assert!(eigen.bei(w).abs() < 1.0e-9, "Eigenfeld bei {w}: {}", eigen.bei(w));
        }

        // Fading: factor 0 removes the displacement, 1 keeps it.
        for w in [0.3, 1.7, -0.9] {
            assert!(feld.mal(0.0).bei(w).abs() < 1.0e-9, "Faktor 0 bei {w}");
            assert!((feld.mal(1.0).bei(w) - feld.bei(w)).abs() < 1.0e-9, "Faktor 1 bei {w}");
        }
    }

    /// Sections without an apex pair: the four target lines of the
    /// acceptance, with the arcs as they come out after the gap between
    /// `F + B` and `P` is split in half (see
    /// `anpassen::formziele_aus_smis`). All four are built exactly.
    #[test]
    fn glatter_querschnitt_trifft_vier_ziele() {
        let faelle = [
            ("Taille", 24.95, 18.43, 36.40, 33.60, 2.241),
            ("Taille ungeteilt", 24.95, 18.43, 36.90, 34.10, 2.431),
            ("Hochhüfte", 30.16, 21.45, 43.00, 40.00, 2.184),
            ("Hochhüfte ungeteilt", 30.16, 21.45, 44.00, 41.00, 2.507),
            ("Hüfte", 31.83, 20.81, 44.00, 46.00, 3.151),
            ("Unterbrust", 26.0, 18.0, 38.0, 38.0, 3.434),
        ];
        for (name, breite, tiefe, vorn, hinten, n_soll) in faelle {
            let q = Querschnitt {
                breite,
                tiefe,
                brustpunkt: 0.0,
                vorn,
                hinten,
            };
            let (umriss, n) = q.form(BACK_SEGMENTS).unwrap_or_else(|| {
                panic!("{name}: nicht baubar");
            });
            assert!(
                (n - n_soll).abs() < 0.01,
                "{name}: Exponent {n:.3} statt {n_soll:.3}"
            );
            let b = boegen(&umriss).unwrap_or_else(|| panic!("{name}: nicht messbar"));
            let abw = |ist: f64, ziel: f64| (ist - ziel).abs();
            assert!(abw(b.breite, breite) < 1.0e-6, "{name}: W {:.4}", b.breite);
            assert!(abw(b.tiefe, tiefe) < 1.0e-6, "{name}: T {:.4}", b.tiefe);
            assert!(abw(b.vorn, vorn) < 1.0e-6, "{name}: F {:.4}", b.vorn);
            assert!(abw(b.hinten, hinten) < 1.0e-6, "{name}: B {:.4}", b.hinten);
            assert!(
                abw(b.umfang, vorn + hinten) < 1.0e-6,
                "{name}: P {:.4} statt {:.4}",
                b.umfang,
                vorn + hinten
            );
        }
    }

    /// The two families share one entry point, and the choice is made by the
    /// apex distance alone: the bust line has one, the waist does not.
    #[test]
    fn form_waehlt_die_familie_nach_dem_brustpunkt() {
        let brust = Querschnitt {
            breite: 28.16,
            tiefe: 24.47,
            brustpunkt: 22.0,
            vorn: 56.0,
            hinten: 36.67,
        };
        let (u, n) = brust.form(BACK_SEGMENTS).expect("Brust ist baubar");
        // The bridge has an exponent below 10; the smooth family at these
        // values is around 3.9 but gives D = 15.8, so the apex pair is the
        // tell-tale.
        let b = boegen(&u).expect("messbar");
        assert!((b.brustpunkt - 22.0).abs() < 1.0e-6, "D {:.3}", b.brustpunkt);
        assert!((2.0..10.0).contains(&n), "Exponent {n}");

        let taille = Querschnitt {
            breite: 24.95,
            tiefe: 18.43,
            brustpunkt: 0.0,
            vorn: 36.40,
            hinten: 33.60,
        };
        let (u, _) = taille.form(BACK_SEGMENTS).expect("Taille ist baubar");
        let b = boegen(&u).expect("messbar");
        assert!(b.brustpunkt < 12.0, "D {:.3}", b.brustpunkt);
        assert!((b.vorn - 36.40).abs() < 1.0e-6);
    }

    /// Real limits, not bugs: below the ellipse value and above the peak there
    /// is no section of this family.
    #[test]
    fn glatter_querschnitt_lehnt_unmoegliche_boegen_ab() {
        let basis = Querschnitt {
            breite: 24.95,
            tiefe: 18.43,
            brustpunkt: 0.0,
            vorn: 36.40,
            hinten: 33.60,
        };
        // Ellipse would give 32.23 - shorter is not reachable on this branch.
        let mut q = basis;
        q.hinten = 30.0;
        assert!(q.umriss_glatt(BACK_SEGMENTS).is_none());
        // Peak is 47.64.
        q.hinten = 50.0;
        assert!(q.umriss_glatt(BACK_SEGMENTS).is_none());
        // No depth at all.
        q = basis;
        q.tiefe = 0.0;
        assert!(q.umriss_glatt(BACK_SEGMENTS).is_none());
    }

    /// The displacement field is what carries a whole mesh: it is measured
    /// from the centre of the measured section, and applied to a section of
    /// the same shape it reproduces the shaped one.
    ///
    /// Compared **by direction**, not by index: [`auf_umriss`] subdivides and
    /// inserts the target's corners, so the shaped section has more points than
    /// the measured one and the two do not pair up one to one.
    #[test]
    fn versatz_feld_beschreibt_die_bewegung() {
        let q = Querschnitt {
            breite: 28.16,
            tiefe: 24.47,
            brustpunkt: 22.0,
            vorn: 56.0,
            hinten: 36.67,
        };
        let (ziel, _) = q.umriss(200).expect("baubar");
        let ist: Vec<Punkt3> = (0..120)
            .map(|k| {
                let w = std::f64::consts::TAU * k as f64 / 120.0;
                [1.3 + 15.725 * w.cos(), 0.0, -0.8 + 11.81 * w.sin()]
            })
            .collect();
        let neu = auf_umriss(&ist, &ziel, q.vorn + q.hinten, UMRISS_RUNDEN).expect("auf den Umriss");
        let feld = versatz_feld(&ist, &neu).expect("Feld");

        // The centre is the measured section's own, not the shaped one's.
        let (cx, cz) = mitte(&ist);
        assert!((feld.mitte.0 - cx).abs() < 1.0e-12);
        assert!((feld.mitte.1 - cz).abs() < 1.0e-12);

        // Applied to the original points, the field reproduces the shaped
        // section's radius in every direction of the original ring.
        let neu_l: Vec<Punkt3> = neu
            .iter()
            .map(|p| [p[0] - cx, 0.0, p[2] - cz])
            .collect();
        let mut max = 0.0f64;
        for p in &ist {
            let (dx, dz) = (p[0] - cx, p[2] - cz);
            let r = (dx * dx + dz * dz).sqrt();
            let richtung = (dx / r, dz / r);
            let s = feld.am(*p);
            let soll = strahlaustritt(&neu_l, richtung).expect("Umriss umschliesst die Mitte");
            let abw = ((r + s) - soll).abs();
            max = max.max(abw);
        }
        assert!(max < 0.5, "grösste Abweichung {max:.3} cm");

        // Fading to zero gives no movement, doubling doubles it.
        assert_eq!(feld.mal(0.0).am(ist[0]), 0.0);
        assert!((feld.mal(2.0).am(ist[0]) - 2.0 * feld.am(ist[0])).abs() < 1.0e-12);    }
}
