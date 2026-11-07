//! The cutting line of a piece, in mesh coordinates.
//!
//! [`Teilnetz`] carries the seam line, and that was the last thing the pipeline
//! saw before the solver sewed. The cutting line - the seam line displaced by
//! the allowance of its nodes - existed only as a contour for the DXF export,
//! the 2D view and `klon3d umriss`, never inside the mesh. This module puts it
//! there: [`klon3d_pattern::saumkontur::Saumkontur`] reached from a
//! [`Teilrand`], converted to the millimetres and pattern axes of
//! [`Teilnetz::punkte`], deduplicated to a closed ring.
//!
//! ## No triangles, on purpose
//!
//! The ring is data, not geometry: no point, no triangle and no seam pair is
//! added or moved by it. The allowance strip as triangles, and the seam that
//! folds it inward, are the next step and need this ring first.
//!
//! ## The rules come from the callers, not from here
//!
//! Two decisions this module does not make for itself:
//!
//! - **`Ecken::Eigen`**, not `Gekreuzt`. The DXF export (#78), the 2D view (#80)
//!   and `klon3d umriss` (#82) all pass `Eigen`, so a free edge (`after = 0`)
//!   stays free instead of gaining an allowance. A fourth rule here would
//!   silently disagree with the exported contour.
//! - **The deduplication is repeated, not shared.** `Saumkontur::punkte` lists
//!   both ends of every segment, so a corner that the arithmetic leaves split
//!   appears twice, and a closed ring must not repeat its first point at the
//!   end. `klon3d umriss` repeats the same rule from `klon3d_app::muster`, for
//!   the same reason: the mesh must not depend on the app, and the pattern crate
//!   must not depend on the mesh.
//!
//! ## Units
//!
//! `Saumkontur` works in pattern units and [`Teilnetz`] works in millimetres, so
//! the conversion happens once, at the end, with `Teilrand::mm_pro_einheit`. The
//! allowance itself is in pattern units as well - `Teilrand::zugabe` returns
//! them unconverted - which is why nothing in this module touches it directly.

use klon3d_pattern::geom::Pt;
use klon3d_pattern::kanten::Teilrand;
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};

/// Below this two coordinates count as the same point. The same tolerance as
/// `klon3d umriss` and `klon3d_app::muster`, because it is the same rule.
const EPS: f64 = 1e-9;

/// The cutting line of `rand` in millimetres, as a closed ring without a
/// repeated closing point; empty when the piece has no cutting line to show.
///
/// Empty for a piece whose nodes carry no allowance at all: the contour would
/// only be a copy of the seam line, and a copy is worse than nothing - a caller
/// could not tell "no allowance" from "allowance of zero". `None` and `0` count
/// alike, a negative allowance is a reduction and does count, so the magnitude
/// decides.
pub fn aus_teilrand(rand: &Teilrand) -> Vec<[f64; 2]> {
    if !hat_zugabe(rand) {
        return Vec::new();
    }
    let kontur = bereinigt(Saumkontur::aus_teilrand(rand, Ecken::Eigen).punkte());
    if kontur.len() < 3 {
        return Vec::new();
    }
    let mm = rand.mm_pro_einheit;
    kontur.iter().map(|p| [p.x * mm, p.y * mm]).collect()
}

/// Whether the piece has a cutting line at all: it needs edges, point nodes and
/// at least one node whose `after` allowance is not zero.
///
/// `after` and not `before`, because that is what `Ecken::Eigen` reads.
pub fn hat_zugabe(rand: &Teilrand) -> bool {
    if rand.kanten.is_empty() || rand.punktknoten.is_empty() {
        return false;
    }
    rand.punktknoten
        .iter()
        .any(|k| k.zugabe_nach.unwrap_or(0.0).abs() > EPS)
}

/// One ring out of [`Saumkontur::punkte`]: drops repeated points and the
/// closing point. Same rule as `klon3d umriss`, which repeats it from the app.
fn bereinigt(punkte: &[Pt]) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(punkte.len());
    for &p in punkte {
        if out.last().is_none_or(|q| q.dist(p) > EPS) {
            out.push(p);
        }
    }
    while out.len() > 1 && out[0].dist(*out.last().unwrap()) <= EPS {
        out.pop();
    }
    out
}

/// The length of a closed ring in millimetres: every edge plus the closing one.
pub fn ringlaenge(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 2 {
        return 0.0;
    }
    let mut summe = 0.0;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        summe += (a[0] - b[0]).hypot(a[1] - b[1]);
    }
    summe
}
