//! The seam allowance contour of a piece: `kanten.rs` and `nahtzugabe.rs`
//! joined.
//!
//! [`Teilrand`] knows the seam line as edges between point nodes and the
//! allowance of every node. [`crate::nahtzugabe`] knows the arithmetic: the
//! allowance is a run along the line, the contour is a displacement per point,
//! and at the two ends of the ring one extra step applies. This module is the
//! joint - it takes a `Teilrand` and returns the displaced segments.
//!
//! ## The winding gives the outward normal
//!
//! The arithmetic moves a point along a stored direction, and nothing in the
//! findings says that this direction is the outward normal: the reference reads
//! it from the line data. The direction is therefore decided here, per segment,
//! as the outward normal of its chord - `(dy, −dx)` on the outside. Which side
//! that is follows from the **winding** of the seam line (the sign of its
//! signed area), so a piece wound either way gets its contour on the outside.
//! A curved edge is thus approximated by the normal of each of its segments and
//! not by one normal for the whole edge.
//!
//! ## The run
//!
//! An edge of at least [`crate::nahtzugabe::MINDESTKANTEN`] segments gets a run
//! along its own point list, from the value of its start node to the value of
//! its end node; every interior point keeps the value the run gives it. Below
//! that the reference skips the run, so the interior points keep the start
//! value. The far node itself still carries its own value, which is why a node
//! with two different values shows a step between two edges. That step is not
//! modelled - it is the case this module deliberately leaves to whoever can
//! decide it.
//!
//! ## One value per node
//!
//! A point node has two allowance fields (`before`, `after`) because the
//! allowance may step at it. The contour carries one value per node, so
//! [`Ecken`] states how the two become one.
//!
//! ## What this does not do
//!
//! No corner types, no self-intersection removal, no default for the smoothing
//! markers of the reference, and no caller: the pipeline, the mesh, the DXF
//! export and the app each still have to decide whether they draw the seam line
//! or the contour.

use crate::geom::Pt;
use crate::kanten::Teilrand;
use crate::nahtzugabe::{kontur, mitte, normiere, verlauf, Abstaende, Saumkante};

/// How the two allowance fields of a point node become the one value the
/// contour carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ecken {
    /// The node's own `after` value, the `before` value is dropped.
    ///
    /// `after` and not `before`, because a free edge has `after = 0` at its
    /// start and must stay free there instead of gaining an allowance.
    #[default]
    Eigen,
    /// The half-way blend `before + (after − before) · 0.5`.
    ///
    /// This is the 0.5 of finding 132 section 4.2, reduced to one node: the
    /// reference blends across two neighbouring corners (own allowance with
    /// the neighbour's sub value), which this model does not have. With one
    /// value per node that crossing collapses to the blend of the node's own
    /// two fields.
    Gekreuzt,
}

/// A piece's seam allowance contour: the displaced seam line, segment by
/// segment.
#[derive(Debug, Clone, Default)]
pub struct Saumkontur {
    segmente: Vec<(Pt, Pt)>,
    punkte: Vec<Pt>,
}

impl Saumkontur {
    /// The contour of `rand`, displaced by the allowance of its nodes.
    ///
    /// One returned segment per segment of the seam line, in the run direction
    /// of the piece. A `Teilrand` without edges or without point nodes yields an
    /// empty contour; a piece whose nodes carry no allowance at all (`None`)
    /// yields its seam line unchanged, because a missing allowance counts as
    /// zero.
    pub fn aus_teilrand(rand: &Teilrand, ecken: Ecken) -> Self {
        if rand.kanten.is_empty() || rand.punktknoten.is_empty() {
            return Self::default();
        }
        let umlauf = umlaufrichtung(rand);

        // One edge entry per segment of the seam line, plus one value per ring
        // point: that is what `kontur` expects, and `kanten.len()` grows to the
        // number of segments accordingly.
        let mut kanten = Vec::new();
        let mut werte: Vec<f64> = Vec::new();
        for k in &rand.kanten {
            let m = k.punkte.len();
            if m < 2 {
                continue;
            }
            let start = wert(rand, k.von, ecken);
            let ende = wert(rand, k.bis, ecken);
            match verlauf(&k.punkte, start, ende) {
                // The last value belongs to the next edge's first point.
                Some(werte_k) => werte.extend_from_slice(&werte_k[..m - 1]),
                None => werte.resize(werte.len() + m - 1, start),
            }
            for paar in k.punkte.windows(2) {
                kanten.push(Saumkante::new(
                    paar[0],
                    paar[1],
                    aussennormale(paar[0], paar[1], umlauf),
                ));
            }
        }
        if kanten.is_empty() || werte.is_empty() {
            return Self::default();
        }
        // The ring closes: the last point is the first node again.
        werte.push(werte[0]);

        let segmente = kontur(&kanten, Abstaende::Verlauf(&werte));
        let mut punkte = Vec::with_capacity(segmente.len() * 2);
        for (von, bis) in &segmente {
            punkte.push(*von);
            punkte.push(*bis);
        }
        Self { segmente, punkte }
    }

    /// The contour per segment, in the run direction of the piece.
    pub fn segmente(&self) -> &[(Pt, Pt)] {
        &self.segmente
    }

    /// The contour as one polyline: every segment contributes both of its
    /// points, so a corner that the arithmetic leaves split appears twice.
    ///
    /// First equals last when the closing corner got the extra step, which
    /// needs a right angle; otherwise the two stay apart, because the reference
    /// removes no self-intersections and closes no gap.
    pub fn punkte(&self) -> &[Pt] {
        &self.punkte
    }
}

/// The allowance value of point node `knoten`.
///
/// A missing allowance counts as zero, which keeps a piece without seam
/// allowance at its seam line instead of making the contour undefined.
fn wert(rand: &Teilrand, knoten: usize, ecken: Ecken) -> f64 {
    let k = &rand.punktknoten[knoten];
    let nach = k.zugabe_nach.unwrap_or(0.0);
    match ecken {
        Ecken::Eigen => nach,
        Ecken::Gekreuzt => mitte(k.zugabe_vor.unwrap_or(0.0), nach),
    }
}

/// `+1` for a seam line wound counter-clockwise in the stored coordinates, `−1`
/// for clockwise, `0` when there is no area to decide it.
///
/// The seam line is preferred over the edges, because it is the line the piece
/// is drawn from; when it is missing (a `Teilrand` built by hand) the edges
/// decide.
fn umlaufrichtung(rand: &Teilrand) -> f64 {
    let a = if rand.umriss.len() >= 3 {
        flaeche(&rand.umriss)
    } else {
        let ring: Vec<Pt> = rand
            .kanten
            .iter()
            .flat_map(|k| k.punkte.iter().copied())
            .collect();
        flaeche(&ring)
    };
    a.signum()
}

/// Twice the signed area of the closed ring, shoelace.
fn flaeche(p: &[Pt]) -> f64 {
    let n = p.len();
    if n < 3 {
        return 0.0;
    }
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// Unit outward normal of the segment `a → b`.
///
/// `(dy, −dx)` is the right-hand side of the direction of travel; for a ring
/// with positive signed area the piece lies on the left, so the right-hand side
/// is outside. A negative area flips both. Zero area is treated as positive -
/// there is no outside to tell then.
///
/// [`normiere`] returns `(1, 1)` for a degenerate segment, the reference's own
/// fallback; a zero-length segment therefore gets no meaningful direction.
fn aussennormale(a: Pt, b: Pt, umlauf: f64) -> Pt {
    let d = Pt::new(b.x - a.x, b.y - a.y);
    if umlauf < 0.0 {
        normiere(Pt::new(-d.y, d.x))
    } else {
        normiere(Pt::new(d.y, -d.x))
    }
}
