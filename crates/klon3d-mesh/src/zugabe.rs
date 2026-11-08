//! The seam allowance as a strip of triangles.
//!
//! [`Teilnetz`] carries the seam line, and that is what the solver sews
//! ([`crate::Teilnetz::dreiecke`]) and what the seam pairs index. The allowance
//! was, until #117, a ring of numbers next to the mesh
//! ([`crate::Teilnetz::schnitt`]) and nothing else: a piece cut *with* an
//! allowance behaved exactly like a piece cut without one. This module turns
//! that ring into a band - the fabric between the seam line and the cutting
//! line - as a strip of triangles.
//!
//! ## Built from the drawn contour, not from a second approximation
//!
//! The outer edge of the band is the ring of
//! [`klon3d_pattern::saumkontur::Saumkontur`], reached from the piece's
//! [`Teilrand`] with the same [`Ecken::Eigen`] the DXF export (#78), the 2D
//! view (#80), `klon3d umriss` (#82) and the ring of #117 pass. So the band's
//! outer edge *is* the cutting line those four print, to the last digit: the
//! contour supplies both ends of every segment, so the pair `(inner, outer)` is
//! read off one walk and never re-derived from an arc length, and the
//! deduplication is [`crate::schnitt`]'s own
//! [`crate::schnitt::ringindizes`] - the same indices that build the ring, so
//! the two cannot drift apart. `tests/zugabe.rs` checks the band's length
//! against `klon3d umriss` - the independent measurement, since `umriss`
//! deduplicates through its own copy of the rule - and the result against
//! `Teilnetz::schnitt` for consistency.
//!
//! ## Nothing the solver reads changes
//!
//! The band is new data: no point, no triangle and no seam pair of
//! [`crate::Teilnetz::punkte`], [`crate::Teilnetz::dreiecke`] or
//! [`crate::Musternetz::naehte`] is added, moved or renumbered. Welding the band
//! to the piece's boundary points and folding it inward are the next step; both
//! need this strip first, and both change what the solver sees, so they do not
//! belong in the same change as the strip.
//!
//! ## Winding: the band is the same sheet, and the mesh wants it positive
//!
//! The piece's own triangles are positive, whichever way the pattern wound its
//! ring: [`crate::dreieck`] reverses the ring when its signed area is negative.
//! The band therefore does not inherit the pattern's winding, it takes the
//! mesh's: the triangles are emitted positive, the outer edge forward and the
//! inner edge back when the cutting line runs counter-clockwise, the other way
//! round when it runs clockwise.
//!
//! ## Anchors: the band moves with the piece
//!
//! The contour is much denser than the mesh boundary (mesh edges are about
//! 10 mm, the pattern's polyline points can be 4 mm apart), so a one-to-one
//! correspondence between outer points and boundary points cannot exist. Each
//! outer point therefore carries an anchor: the boundary point whose stretch of
//! seam line it lies on. Several outer points may share an anchor; the anchors
//! run non-decreasing along the ring, so a deformation of the piece carries the
//! band with it instead of leaving it behind.
//!
//! The lookup assumes that [`Streifen::bogen`] (arc length along the seam line)
//! and [`crate::Teilnetz::randort`] (arc length along the mesh boundary) are one
//! scale. They are two different polylines, so this is an assumption, not a
//! tautology - it holds because both walk the same pattern points in the same
//! direction, and `anker_und_rand_liegen_auf_derselben_bogenskala` in
//! `tests/zugabe.rs` measures it per piece.
//!
//! ## Units
//!
//! [`Teilrand`] and [`Saumkontur`] work in pattern units, [`crate::Teilnetz`]
//! in millimetres. The conversion happens once, at the end, with
//! `Teilrand::mm_pro_einheit`; every distance in this module is then in mm.
//!
//! ## Decided here, not found
//!
//! No finding describes this band. Three decisions are therefore this module's
//! own, and each is stated where it is made: the outer edge is the contour of
//! `Ecken::Eigen` (the callers' rule), a stretch whose allowance is zero at
//! both ends is left out instead of kept as a degenerate triangle, and the
//! anchor is the boundary point of the seam-line stretch, not the nearest
//! boundary point by distance.

use klon3d_pattern::geom::Pt;
use klon3d_pattern::kanten::Teilrand;
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};

use crate::rand::Kantenzug;

/// A width below this is no width (mm): a nanometre of allowance is none.
///
/// Not the ring's `EPS`: that one works in pattern units and only decides
/// whether two points are the same point. This one works in millimetres and
/// decides whether a stretch carries any fabric - a different question with a
/// different unit, so it gets its own constant instead of borrowing a tolerance
/// that means something else.
const NULL_BREITE_MM: f64 = 1e-6;

/// Twice the area below which a triangle counts as degenerate (mm²).
const MIN_FLAECHE2: f64 = 1e-9;

/// The allowance strip of one piece, in millimetres in the piece's own frame.
///
/// Two rings of the same length: `innen[i]` is a point of the seam line,
/// `aussen[i]` the cutting line point it was displaced to. Both are closed
/// rings without the repeated first point, in the run direction of the piece.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Streifen {
    /// The seam line, point by point (mm).
    pub innen: Vec<[f64; 2]>,
    /// The cutting line, point for point to [`Streifen::innen`] (mm).
    ///
    /// The same closed ring as [`crate::Teilnetz::schnitt`], by construction and
    /// by the test in `tests/zugabe.rs`.
    pub aussen: Vec<[f64; 2]>,
    /// Arc length of every ring point along the seam line (mm), starting at the
    /// ring's first point. Increasing; it stops short of the closed ring length
    /// by the closing stretch.
    pub bogen: Vec<f64>,
    /// Per outer point: the boundary point of the piece (index in
    /// [`crate::Teilnetz::punkte`], below [`crate::Teilnetz::nrand`]) whose
    /// stretch of seam line it lies on.
    ///
    /// Empty until [`mit_ankern`] has seen the mesh's boundary points.
    pub anker: Vec<u32>,
    /// The band's triangles. Indices below `innen.len()` are seam line points,
    /// above it cutting line points, so point `n + i` is `aussen[i]`.
    ///
    /// Positive, like the piece's own triangles. A triangle without area is not
    /// emitted, so a corner the arithmetic left split yields one triangle
    /// instead of two.
    pub dreiecke: Vec<[u32; 3]>,
    /// Stretches left out because the allowance is zero at both of their ends.
    pub ohne_breite: usize,
    /// Their length on the cutting line (mm), so the ring closes exactly: the
    /// ring length of [`Streifen::aussen`] is [`Streifen::aussenlaenge`] plus
    /// this number, to the last digit.
    pub ohne_breite_mm: f64,
}

impl Streifen {
    /// Whether the piece has no band at all - not even a degenerate one.
    pub fn ist_leer(&self) -> bool {
        self.dreiecke.is_empty()
    }

    /// The point of a triangle index: below `innen.len()` the seam line, above
    /// it the cutting line.
    pub fn punkt(&self, i: u32) -> [f64; 2] {
        let n = self.innen.len();
        if (i as usize) < n {
            self.innen[i as usize]
        } else {
            self.aussen[i as usize - n]
        }
    }

    /// Whether the outer ring is the one [`crate::Teilnetz::schnitt`] holds.
    ///
    /// `false` for a piece without an allowance: both are empty then, and an
    /// empty band is not a cutting line.
    pub fn deckt_die_schnittlinie(&self, schnitt: &[[f64; 2]]) -> bool {
        !self.aussen.is_empty()
            && self.aussen.len() == schnitt.len()
            && self.aussen.iter().zip(schnitt).all(|(a, b)| {
                (a[0] - b[0]).abs() <= NULL_BREITE_MM && (a[1] - b[1]).abs() <= NULL_BREITE_MM
            })
    }

    /// The outer edge of every stretch that carries fabric, summed: the length
    /// the band covers along the cutting line (mm).
    ///
    /// A stretch carries fabric unless its allowance is zero at both ends - the
    /// same rule [`aus_teilrand`] uses to leave it out. Plus
    /// [`Streifen::ohne_breite_mm`] this is the closed ring length of
    /// [`Streifen::aussen`]; `tests/zugabe.rs` asserts that sum against the
    /// ring length of `klon3d umriss`, which is a different path to the same
    /// number.
    pub fn aussenlaenge(&self) -> f64 {
        let n = self.innen.len();
        if n < 2 || self.aussen.len() != n {
            return 0.0;
        }
        let breite = |i: usize| abstand(self.innen[i], self.aussen[i]);
        (0..n)
            .filter(|&i| breite(i) > NULL_BREITE_MM || breite((i + 1) % n) > NULL_BREITE_MM)
            .map(|i| abstand(self.aussen[i], self.aussen[(i + 1) % n]))
            .sum()
    }
}

/// The allowance strip of `rand` in millimetres; empty when the piece has no
/// allowance, so that a caller cannot mistake "no allowance" for "an allowance
/// of zero".
///
/// Returns `Err` when the contour and the seam line disagree on their point
/// count. That cannot happen through [`Saumkontur::aus_teilrand`] - it is the
/// same walk - and a silent empty strip would hide the next change that breaks
/// the pairing.
pub fn aus_teilrand(rand: &Teilrand) -> Result<Streifen, String> {
    let mut s = Streifen::default();
    if !crate::schnitt::hat_zugabe(rand) {
        return Ok(s);
    }
    let kontur = Saumkontur::aus_teilrand(rand, Ecken::Eigen);
    // The seam line point by point, in the order the contour walked it: both
    // ends of every segment, so shared corners appear twice, exactly as in
    // `Saumkontur::punkte`. Counting the same segments in the same order is
    // what makes the two lists parallel.
    let mut naht: Vec<Pt> = Vec::new();
    for k in &rand.kanten {
        if k.punkte.len() < 2 {
            continue;
        }
        for w in k.punkte.windows(2) {
            naht.push(w[0]);
            naht.push(w[1]);
        }
    }
    let aussen = kontur.punkte();
    if naht.len() != aussen.len() {
        return Err(format!(
            "Zugabestreifen: {} Nahtpunkte zu {} Konturpunkten",
            naht.len(),
            aussen.len()
        ));
    }
    // The indices the drawn ring keeps. Reusing them keeps the band's outer
    // edge identical to `Teilnetz::schnitt` instead of almost identical.
    let idx = crate::schnitt::ringindizes(aussen);
    if idx.len() < 3 {
        return Ok(s);
    }

    let mm = rand.mm_pro_einheit;
    s.innen = idx.iter().map(|&i| [naht[i].x * mm, naht[i].y * mm]).collect();
    s.aussen = idx.iter().map(|&i| [aussen[i].x * mm, aussen[i].y * mm]).collect();

    // Arc length along the seam line, from the ring's first point. A dropped
    // duplicate takes no length with it, so summing the kept points is the
    // whole polyline.
    let mut bogen = 0.0;
    for i in 0..s.innen.len() {
        if i > 0 {
            bogen += abstand(s.innen[i - 1], s.innen[i]);
        }
        s.bogen.push(bogen);
    }

    // Positive as the mesh wants them, see the module note on winding.
    let positiv = ringflaeche(&s.aussen) >= 0.0;
    let n = s.innen.len();
    for i in 0..n {
        let j = (i + 1) % n;
        // No width at either end: there is no fabric here, and a quad of that
        // shape would be a line with three corners - a degenerate triangle
        // instead of a statement that the piece has no allowance.
        if abstand(s.innen[i], s.aussen[i]) <= NULL_BREITE_MM
            && abstand(s.innen[j], s.aussen[j]) <= NULL_BREITE_MM
        {
            s.ohne_breite += 1;
            s.ohne_breite_mm += abstand(s.aussen[i], s.aussen[j]);
            continue;
        }
        // Two triangles per stretch: the outer edge and one diagonal.
        let mut t = [[n + i, n + j, j], [n + i, j, i]];
        if !positiv {
            t.iter_mut().for_each(|d| d.swap(1, 2));
        }
        for d in t {
            let [a, b, c] = d.map(|k| s.punkt(k as u32));
            if flaeche2(a, b, c) > MIN_FLAECHE2 {
                s.dreiecke.push(d.map(|k| k as u32));
            }
        }
    }
    Ok(s)
}

/// `s` with [`Streifen::anker`] filled in from the mesh's boundary points.
///
/// `kanten` are the piece's boundary edges in mm ([`crate::Teilnetz::kanten`])
/// and `orte` the place of every boundary point on them
/// ([`crate::Teilnetz::randort`]: edge and arc length from its start). Both are
/// needed to place the boundary points on the same arc scale as
/// [`Streifen::bogen`].
pub fn mit_ankern(s: &Streifen, kanten: &[Kantenzug], orte: &[(usize, f64)]) -> Streifen {
    let mut s = s.clone();
    s.anker = anker(&s, kanten, orte);
    s
}

/// Anchor of every outer point: the boundary point of its stretch of seam line.
///
/// `bogen` and `randort` are arc lengths along *different* polylines - the seam
/// line carries the contour points, the mesh boundary the group divisions - so
/// the lookup only means anything if the two share one scale: same start, same
/// direction, same total. They do, because both walk the piece's pattern points
/// in the piece's own direction; `anker_und_rand_liegen_auf_derselben_bogenskala`
/// in `tests/zugabe.rs` holds that to 0.1 % of the length and every outer point
/// to the interval between its anchor and the next boundary point.
///
/// Empty when there is nothing to compare - a strip without triangles, a piece
/// without boundary points - or when `orte` is not the ring order the contour
/// walked, since a wrong anchor is worse than none.
fn anker(s: &Streifen, kanten: &[Kantenzug], orte: &[(usize, f64)]) -> Vec<u32> {
    if s.dreiecke.is_empty() || orte.len() < 2 || kanten.is_empty() || s.bogen.len() != s.aussen.len() {
        return Vec::new();
    }
    let mut praefix = Vec::with_capacity(kanten.len());
    let mut summe = 0.0;
    for k in kanten {
        praefix.push(summe);
        summe += k.laenge;
    }
    let mut ort = Vec::with_capacity(orte.len());
    for &(k, v) in orte {
        match praefix.get(k) {
            Some(&p) => ort.push(p + v),
            None => return Vec::new(),
        }
    }
    if ort.windows(2).any(|w| w[1] < w[0]) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(s.bogen.len());
    let mut j = 0usize;
    for &b in &s.bogen {
        // A point before the first boundary point belongs to the stretch that
        // closes the ring.
        if b < ort[0] {
            out.push((ort.len() - 1) as u32);
            continue;
        }
        while j + 1 < ort.len() && ort[j + 1] <= b {
            j += 1;
        }
        out.push(j as u32);
    }
    out
}

fn abstand(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Twice the signed area of the triangle `a b c`: positive when it is wound
/// counter-clockwise.
fn flaeche2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Twice the signed area of the closed ring: positive counter-clockwise.
fn ringflaeche(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}
