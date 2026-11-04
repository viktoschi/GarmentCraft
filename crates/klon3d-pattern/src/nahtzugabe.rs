//! Seam allowance as a run along the edge, and the contour as a displacement
//! per point.
//!
//! Two findings of 2026-11-03 (132 and 133 of the author's notes), rebuilt as
//! arithmetic. The reference program computes the outer seam allowance in two
//! steps, and neither is the one klon3d had:
//!
//! **1. The allowance is a run, not a width.** [`verlauf`] spreads an amount
//! linearly over the **accumulated arc length** of a polyline: the first point
//! gets the start value, the last the end value, in between
//! `a + (b − a) · s/S`. It returns nothing below [`MINDESTKANTEN`] segments -
//! the reference skips the run there. What it does *not* do is move a point.
//!
//! **2. At a point node the values are averaged crossed with 0.5.** The start
//! value is the own allowance blended towards the **neighbour's sub value**
//! ([`startwert`]), the end value the own sub value blended towards the
//! **neighbour's allowance** ([`endwert`]). Without the smoothing markers each
//! side simply takes its own field.
//!
//! **3. The contour is a displacement per point.** [`kontur`] moves both ends
//! of every edge along that edge's **stored direction** by the allowance of that
//! end. No offset polygon, no self-intersection removal, no corner types - the
//! reference builds none of that. One quirk is reproduced on purpose: at the
//! two ends of the run the point is pushed one more allowance along the
//! **neighbouring** edge's stored direction, but only if it lies on that edge's
//! line.
//!
//! Which direction that is, is **not** settled here. The reference reads it from
//! the line data, and the tests pass the outward normal because that is what a
//! contour needs - but whether the stored vector really is the normal, or
//! depends on the piece's winding, was not answered by the findings. This module
//! only requires that the direction points to the outside; it does not derive it.
//!
//! Coordinates and allowances stay in pattern units; nothing is converted
//! here. `kanten.rs` keeps the width per edge; this module is the arithmetic
//! behind a contour and is not called by the pipeline yet.

use crate::geom::Pt;

/// Fewest edges a run needs. Below this the reference computes no run at all
/// and leaves the allowance at its scalar value.
pub const MINDESTKANTEN: usize = 4;

/// Distance under which a vector counts as zero - the reference's tolerance,
/// used both for the normalisation and for the collinearity test.
pub const TOLERANZ: f64 = 1e-5;

/// Allowance per point of a polyline: start value first, end value last.
///
/// `S` is the length of the polyline itself, without any offset;
/// `o_k = a + (b − a) · s_k/S` with `s_k` the length up to point `k`. The first
/// value is exactly `a`. The last is `a + (b − a) · 1`, which equals `b`
/// whenever that sum is exact in `f64` (for `1 → 2` and `−1 → −2` it is) and is
/// otherwise one rounding step off - the same single expression the reference
/// evaluates, so the rounding is its rounding too.
///
/// `None` when the line has fewer than [`MINDESTKANTEN`] segments or no length
/// at all; both are the cases the reference skips.
pub fn verlauf(punkte: &[Pt], start: f64, ende: f64) -> Option<Vec<f64>> {
    if punkte.len() < MINDESTKANTEN + 1 {
        return None;
    }
    let mut s = Vec::with_capacity(punkte.len());
    let mut gesamt = 0.0;
    s.push(0.0);
    for i in 1..punkte.len() {
        gesamt += punkte[i].dist(punkte[i - 1]);
        s.push(gesamt);
    }
    if gesamt <= 0.0 {
        return None;
    }
    Some(
        s.iter()
            .map(|&sk| start + (ende - start) * (sk / gesamt))
            .collect(),
    )
}

/// Half-way blend of two allowances: `a + (b − a) · 0.5`.
///
/// Equal values give the value itself, as in the reference, where that case is
/// written out without arithmetic.
pub fn mitte(a: f64, b: f64) -> f64 {
    if a == b {
        a
    } else {
        a + (b - a) * 0.5
    }
}

/// Start value at a point node: the own allowance, blended towards the
/// neighbour's **sub** value when both smoothing markers are set and a
/// neighbour object exists.
///
/// `nachbar_unter` is `None` when either marker is off or there is no
/// neighbour object; then the result is `eigen` unchanged.
pub fn startwert(eigen: f64, nachbar_unter: Option<f64>) -> f64 {
    nachbar_unter.map_or(eigen, |s| mitte(eigen, s))
}

/// End value at a point node: the own **sub** value, blended towards the
/// neighbour's allowance. The counterpart of [`startwert`], mirrored.
pub fn endwert(eigen_unter: f64, nachbar: Option<f64>) -> f64 {
    nachbar.map_or(eigen_unter, |g| mitte(eigen_unter, g))
}

/// Unit vector of `v`; `(1.0, 1.0)` when the length is under [`TOLERANZ`].
///
/// The odd fallback is the reference's: it does not return the zero vector but
/// writes two ones. It only reaches that branch for a vector of length zero.
pub fn normiere(v: Pt) -> Pt {
    let l = (v.x * v.x + v.y * v.y).sqrt();
    if l < TOLERANZ {
        Pt::new(1.0, 1.0)
    } else {
        Pt::new(v.x / l, v.y / l)
    }
}

/// One edge of a seam line as the contour needs it: both ends plus the
/// direction the allowance is displaced in.
///
/// The direction is stored per edge, not derived from `von → bis` - which side
/// is outside depends on the piece's winding, and the reference takes it from
/// the line data as well. It need not be a unit vector; [`kontur`] normalises
/// it. Which end is which does not matter, but the values in [`Abstaende`] must
/// follow the list: entry `k` belongs to `kanten[k].von`, entry `k + 1` to
/// `kanten[k].bis`, so neighbouring edges share their corner value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Saumkante {
    pub von: Pt,
    pub bis: Pt,
    pub richtung: Pt,
}

impl Saumkante {
    pub fn new(von: Pt, bis: Pt, richtung: Pt) -> Self {
        Self { von, bis, richtung }
    }
}

/// What [`kontur`] displaces by.
#[derive(Debug, Clone, Copy)]
pub enum Abstaende<'a> {
    /// One value for every point - the case without a computed run.
    Fest(f64),
    /// The run's value per point, one more entry than there are edges.
    Verlauf(&'a [f64]),
}

impl Abstaende<'_> {
    fn bei(&self, i: usize) -> f64 {
        match self {
            Abstaende::Fest(w) => *w,
            Abstaende::Verlauf(werte) => werte[i],
        }
    }
}

/// The contour: per edge the displaced start and the displaced end.
///
/// Each point moves along its edge's **stored direction** by the allowance of
/// that end. That the direction is the outward normal is the caller's
/// assumption, not this module's: see [`Saumkante`] and the module note.
/// At the two ends of the run - first and last edge of the list - one extra
/// rule applies: if the displaced corner point lies on the line of the
/// neighbouring edge (cross product within [`TOLERANZ`]), it is pushed one more
/// allowance along that neighbour's direction. On a square the two
/// displacements add up to the diagonal corner; at an oblique corner the point
/// stays where the plain displacement put it.
///
/// Two deliberate details of the reference: the extra step uses the
/// neighbouring edge's **stored** vector, not the normalised one, so it is
/// exactly one allowance only when that vector is a unit vector; and the rule
/// runs only at the ends of the run, not at every corner. A corner between two
/// middle edges therefore stays split in two points.
///
/// # Panics
///
/// `Abstaende::Verlauf` with a length other than `kanten.len() + 1`.
pub fn kontur(kanten: &[Saumkante], abstaende: Abstaende<'_>) -> Vec<(Pt, Pt)> {
    if let Abstaende::Verlauf(werte) = abstaende {
        assert_eq!(
            werte.len(),
            kanten.len() + 1,
            "Verlauf braucht einen Wert je Kantenende: {} für {} Kanten",
            kanten.len() + 1,
            kanten.len()
        );
    }
    let n = kanten.len();
    let mut out = Vec::with_capacity(n);
    for (i, k) in kanten.iter().enumerate() {
        let e = normiere(k.richtung);
        let (d_von, d_bis) = (abstaende.bei(i), abstaende.bei(i + 1));
        let mut von = versetzt(k.von, e, d_von);
        let mut bis = versetzt(k.bis, e, d_bis);
        if i == 0 {
            let nachbar = &kanten[n - 1];
            if kollinear(nachbar.bis, nachbar.von, von) {
                von = versetzt(von, nachbar.richtung, d_von);
            }
        } else if i == n - 1 {
            let nachbar = &kanten[0];
            if kollinear(nachbar.von, nachbar.bis, bis) {
                bis = versetzt(bis, nachbar.richtung, d_bis);
            }
        }
        out.push((von, bis));
    }
    out
}

/// `p` moved by `richtung · d`.
fn versetzt(p: Pt, richtung: Pt, d: f64) -> Pt {
    Pt::new(p.x + richtung.x * d, p.y + richtung.y * d)
}

/// Does `p` lie on the line through `a` and `b`, within [`TOLERANZ`]?
///
/// The reference passes the cross product `(a − b) × (a − p)` to a range test
/// "value in `[−tol, +tol]`", which is the same as `abs(cross) <= tol`.
fn kollinear(a: Pt, b: Pt, p: Pt) -> bool {
    let u = Pt::new(a.x - b.x, a.y - b.y);
    let v = Pt::new(a.x - p.x, a.y - p.y);
    (u.x * v.y - u.y * v.x).abs() <= TOLERANZ
}
