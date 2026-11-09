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
//! ## Welded to the piece's boundary points
//!
//! [`verschweisst`] makes every boundary point of the mesh a vertex of the band.
//! The boundary points already lie on the band's seam-line ring (measured:
//! 0.000000 mm on every piece of the three test patterns) but only a minority is
//! one of its vertices - 23.1 % / 21.5 % / 7.8 % of them, per piece between
//! 6.7 % and 32.7 %. The rest sit mid-segment, because the mesh boundary is much
//! coarser than the contour. So the ring is split there: the
//! boundary point becomes an inner vertex at its own coordinates, and its outer
//! partner is read off the *outer* segment at the same fraction, which puts it
//! on the cutting line by construction. Splitting a segment adds no area, so the
//! band keeps its width; `tests/zugabe.rs` measures that per piece.
//!
//! The split has a price, and it is not hidden: the band's free inner edges are
//! still the seam line, not the mesh boundary. Between two boundary points the
//! contour keeps the vertices it drew - they are what makes the band wide where
//! the boundary chord is not - so those free edges run through contour points
//! and are *not* edges of the piece. Making them boundary edges would mean
//! replacing the inner ring by the boundary polygon, which trades the seam line
//! for its chord and moves the band's area by up to 21.8 mm² on the test
//! patterns. That is the opposite of what the acceptance wants, so the spare
//! vertices stay and `tests/zugabe.rs` counts both kinds of free inner edge.
//!
//! Boundary points on a stretch without fabric have no triangle to be a vertex
//! of. They are inserted all the same, for one rule instead of two, and the test
//! names and counts the ones no triangle can use.
//!
//! ## Nothing the solver reads changes
//!
//! The band is new data: no point, no triangle and no seam pair of
//! [`crate::Teilnetz::punkte`], [`crate::Teilnetz::dreiecke`] or
//! [`crate::Musternetz::naehte`] is added, moved or renumbered, and welding only
//! touches the band's own rings. Folding it inward and giving the band a seam
//! lives in `klon3d_sim::saumfalte`, after the placement - it changes what the
//! solver sees, so it is a change of its own (#123) and not part of the weld.
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
//! band with it instead of leaving it behind. After [`verschweisst`] every
//! boundary point is a ring point with itself as anchor, so the shared ones are
//! the outer partners: one anchor per boundary point, several outer points and
//! several cutting-line points on one boundary stretch.
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
//! No finding describes this band. Four decisions are therefore this module's
//! own, and each is stated where it is made: the outer edge is the contour of
//! `Ecken::Eigen` (the callers' rule), a stretch whose allowance is zero at
//! both ends is left out instead of kept as a degenerate triangle, the anchor is
//! the boundary point of the seam-line stretch rather than the nearest boundary
//! point by distance, and a miter keeps its wedge triangle under a turned
//! winding instead of losing it as negatively wound.

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

/// Two seam-line points below this distance are the same point (mm).
///
/// A miter puts two of them on one place. Not [`NULL_BREITE_MM`]: that one asks
/// whether a stretch carries fabric, this one whether two points coincide - a
/// split corner can carry width while its two inner points are one place.
const MIN_ABSTAND_MM: f64 = 1e-9;

/// An arc fraction below this is the ring point itself, not a point between two
/// ring points (dimensionless).
///
/// Used where a boundary point lands on a ring vertex: it must reuse that
/// vertex instead of inserting a second point at the same place. Not
/// [`NULL_BREITE_MM`], which is a length in mm on a different question.
const BRUCH_EPS: f64 = 1e-9;

/// How far a boundary point may lie from the seam line and still be welded (mm).
///
/// A thousandth of a millimetre - far below the roughly 10 mm of a mesh edge, so
/// a boundary point that is genuinely elsewhere is refused, and far above the
/// rounding of two walks over the same pattern points. Measured on the three
/// test patterns: 0.000000 mm everywhere.
const RAND_AUF_NAHT_MM: f64 = 1e-3;

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
    /// Empty until [`verschweisst`] has seen the mesh's boundary points.
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

    /// The vertex every seam-line point shares, index for index.
    ///
    /// Two inner points at the same place are one vertex in the band: the entry
    /// is the lowest index carrying that place, so `vertreter()[i] == i` for a
    /// point that no other point shares. A stretch `i` therefore has no inner
    /// length exactly when `vertreter()[i] == vertreter()[i + 1]`, and its inner
    /// edge - if it has one - is `(vertreter()[i], vertreter()[i + 1])`, not
    /// `(i, i + 1)`.
    ///
    /// Only the inner ring is welded. Every cutting-line point keeps its own
    /// index, since sharing one would move the drawn contour.
    pub fn vertreter(&self) -> Vec<u32> {
        let n = self.innen.len();
        let mut vertreter: Vec<u32> = (0..n as u32).collect();
        for i in 1..n {
            for h in 0..i {
                if abstand(self.innen[i], self.innen[h]) <= MIN_ABSTAND_MM {
                    vertreter[i] = vertreter[h];
                    break;
                }
            }
        }
        vertreter
    }

    /// Whether the outer ring is the one [`crate::Teilnetz::schnitt`] holds.
    ///
    /// Before [`verschweisst`] this was point for point. Welding splits the
    /// outer ring where a boundary point arrives, so the two lists are no longer
    /// identical - but they are still the same closed curve, and that is what
    /// this checks now: same ring length, every outer point on the cutting line,
    /// every cutting-line corner among the outer points. Splitting a segment
    /// moves none of the three, so a band that drifts off the drawn line fails
    /// here as it did before.
    ///
    /// `false` for a piece without an allowance: both are empty then, and an
    /// empty band is not a cutting line.
    pub fn deckt_die_schnittlinie(&self, schnitt: &[[f64; 2]]) -> bool {
        if self.aussen.len() < 3 || schnitt.len() < 3 {
            return false;
        }
        let laenge = |ring: &[[f64; 2]]| {
            let n = ring.len();
            (0..n).map(|i| abstand(ring[i], ring[(i + 1) % n])).sum::<f64>()
        };
        if (laenge(&self.aussen) - laenge(schnitt)).abs() > NULL_BREITE_MM {
            return false;
        }
        // Every outer point on the cutting line ...
        let drauf = |p: [f64; 2]| {
            (0..schnitt.len())
                .map(|k| {
                    let (a, b) = (schnitt[k], schnitt[(k + 1) % schnitt.len()]);
                    let d = [b[0] - a[0], b[1] - a[1]];
                    let l2 = d[0] * d[0] + d[1] * d[1];
                    if l2 <= 0.0 {
                        return abstand(p, a);
                    }
                    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
                    abstand(p, [a[0] + t * d[0], a[1] + t * d[1]])
                })
                .fold(f64::INFINITY, f64::min)
        };
        // ... and every corner of the cutting line kept.
        self.aussen.iter().all(|&p| drauf(p) <= NULL_BREITE_MM)
            && schnitt.iter().all(|&q| {
                self.aussen.iter().any(|&p| abstand(p, q) <= NULL_BREITE_MM)
            })
    }

    /// The band point of a boundary point, if it is one: the inner ring index
    /// whose place is that boundary point's, or `None` when the band has none.
    ///
    /// An exact match wins over a neighbour within [`NULL_BREITE_MM`]: after the
    /// weld the ring holds the boundary point itself, and a second vertex a
    /// micrometre away must not answer for it.
    ///
    /// `None` is the counted exception of the weld: a boundary point on a
    /// stretch that carries no fabric has no triangle to be a vertex of.
    pub fn bandpunkt_von(&self, p: [f64; 2]) -> Option<usize> {
        self.innen
            .iter()
            .position(|&q| q == p)
            .or_else(|| {
                self.innen
                    .iter()
                    .position(|&q| abstand(p, q) <= NULL_BREITE_MM)
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
    bogenskala(&mut s);
    baue(&mut s);
    Ok(s)
}

/// `s` welded to the mesh boundary: every point of `rand` becomes a vertex of
/// the band's seam-line ring at its own coordinates.
///
/// `rand` are the piece's boundary points in ring order
/// ([`crate::Teilnetz::punkte`] below [`crate::Teilnetz::nrand`]), `kanten` and
/// `orte` their places as in [`anker`]. The result keeps the seam line's
/// vertices and splits a segment wherever a boundary point falls between two of
/// them; each inserted point's partner on the cutting line is read off the
/// *outer* segment at the same fraction, so it lies on the cutting line by
/// construction and no area is added.
///
/// The weld rests on one assumption, the same one the anchors rest on: the arc
/// length of [`Streifen::bogen`] and the arc length of `orte` are one scale -
/// same origin, same direction, same total. Both walk the piece's pattern points
/// in the piece's own direction, measured at 0.000000 mm on every piece of the
/// three test patterns. When a boundary point is not on the seam line, the weld
/// is not attempted at all: an unchanged strip is wrong in a way the tests
/// report, a silently mispaired one would not be.
///
/// Boundary points on a stretch that carries no fabric are inserted all the
/// same - one rule instead of two - and have no triangle to be a vertex of.
pub fn verschweisst(
    s: &Streifen,
    rand: &[[f64; 2]],
    kanten: &[Kantenzug],
    orte: &[(usize, f64)],
) -> Streifen {
    let mut neu = geschweisst(s, rand, kanten, orte).unwrap_or_else(|| s.clone());
    neu.anker = anker(&neu, kanten, orte);
    neu
}

/// The welded strip, or `None` when the two arc scales do not fit together.
fn geschweisst(
    s: &Streifen,
    rand: &[[f64; 2]],
    kanten: &[Kantenzug],
    orte: &[(usize, f64)],
) -> Option<Streifen> {
    let n = s.innen.len();
    if n < 3 || s.aussen.len() != n || s.bogen.len() != n {
        return None;
    }
    let u = s.bogen[n - 1] + abstand(s.innen[n - 1], s.innen[0]);
    if u <= 0.0 {
        return None;
    }
    let ort = randorte(kanten, orte, rand.len())?;

    // Every boundary point on the ring's own scale: the ring vertex it is, or
    // the segment and fraction where it falls between two of them.
    let mut einsetzen: Vec<Vec<(f64, usize)>> = vec![Vec::new(); n];
    let mut am_punkt: Vec<Option<usize>> = vec![None; n];
    let mut auf_ring = 0usize;
    for (i, &t) in ort.iter().enumerate() {
        let t = t.rem_euclid(u);
        let (k, f) = einordnen(&s.bogen, u, t);
        // The point has to lie on the seam line, or the pairing below is
        // meaningless. Checked against the segment the place points at, not
        // against the whole ring: that is the partner the band would take.
        let k1 = (k + 1) % n;
        let q = [
            s.innen[k][0] + f * (s.innen[k1][0] - s.innen[k][0]),
            s.innen[k][1] + f * (s.innen[k1][1] - s.innen[k][1]),
        ];
        if abstand(rand[i], q) > RAND_AUF_NAHT_MM {
            return None;
        }
        if f <= BRUCH_EPS {
            if am_punkt[k].is_none() {
                am_punkt[k] = Some(i);
                auf_ring += 1;
            }
        } else if f >= 1.0 - BRUCH_EPS {
            if am_punkt[k1].is_none() {
                am_punkt[k1] = Some(i);
                auf_ring += 1;
            }
        } else {
            einsetzen[k].push((f, i));
            auf_ring += 1;
        }
    }
    // Not one boundary point in the band's own ring: nothing to weld.
    if auf_ring == 0 {
        return None;
    }

    let mut neu = Streifen::default();
    for k in 0..n {
        // A boundary point that falls on a ring vertex takes that vertex's
        // place: it is the same point, but only this exact pair of coordinates
        // makes the mesh share it. Keeping the ring's own copy would leave two
        // vertices a hair apart, which is not a weld.
        neu.innen.push(match am_punkt[k] {
            Some(i) => rand[i],
            None => s.innen[k],
        });
        neu.aussen.push(s.aussen[k]);
        let k1 = (k + 1) % n;
        let mut liste = std::mem::take(&mut einsetzen[k]);
        liste.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut letzte = 0.0f64;
        for (f, i) in liste {
            // Two boundary points at the same place would be one vertex, not
            // two: the first one holds it.
            if f - letzte <= BRUCH_EPS {
                continue;
            }
            letzte = f;
            neu.innen.push(rand[i]);
            neu.aussen.push([
                s.aussen[k][0] + f * (s.aussen[k1][0] - s.aussen[k][0]),
                s.aussen[k][1] + f * (s.aussen[k1][1] - s.aussen[k][1]),
            ]);
        }
    }
    bogenskala(&mut neu);
    baue(&mut neu);
    Some(neu)
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

/// The arc scale of a ring: length along the seam line from its first point.
///
/// A dropped duplicate takes no length with it, so summing the kept points is
/// the whole polyline. The closing stretch is not in the list; the ring length
/// is the last entry plus [`abstand`] to the first point.
fn bogenskala(s: &mut Streifen) {
    s.bogen.clear();
    let mut bogen = 0.0;
    for i in 0..s.innen.len() {
        if i > 0 {
            bogen += abstand(s.innen[i - 1], s.innen[i]);
        }
        s.bogen.push(bogen);
    }
}

/// The band's triangles over the two rings as they stand.
///
/// Called after every change to the rings, so the count of stretches without
/// fabric and the triangle list always describe the same rings.
///
/// ## A miter is a stretch of no length, and it must not cut the band
///
/// Where the contour steps around a corner, the seam line can put two points on
/// the same place while the cutting line keeps them apart - the miter the module
/// note names. The stretch between them has inner length 0 and a width, so its
/// second triangle `[o_i, i_j, i_i]` has two equal corners and no area, and its
/// first one is the wedge `(o_i, o_j, i)`, the triangle that makes the outer
/// edge one line again. Emitted under the ordinary winding rule that wedge comes
/// out negatively wound - the ring turns back on itself there - and the area test
/// would drop it, which severs the connection between the stretches before and
/// after the corner. Both halves of the repair are needed:
///
/// 1. Two inner points at the same place become one vertex, so the wedge's two
///    inner edges are the neighbours' edges.
/// 2. The wedge is kept even though it is negative, with its winding turned so
///    it is positive like every other triangle of the mesh.
///
/// The wedge is the overlap of the band with itself at the corner; keeping it
/// adds that overlap's area to the band and removes no triangle. Measured on the
/// three test patterns: 578 / 650 / 104 wedges, band area +93.3 / +93.4 /
/// +8.7 mm², and the number of detached triangle groups falls from 1122 / 1108 /
/// 242 to 14 / 17 / 5 across 12 / 16 / 5 pieces - one group per run of fabric,
/// except three places where the band pinches to no width at a point.
fn baue(s: &mut Streifen) {
    // Positive as the mesh wants them, see the module note on winding.
    let positiv = ringflaeche(&s.aussen) >= 0.0;
    let n = s.innen.len();
    // Inner points that share a place share a vertex: `vertreter[i]` is the
    // lowest index with the same inner point. Only the inner ring is welded -
    // the cutting line keeps every point it has, since moving an outer point
    // would move the drawn contour.
    let vertreter = s.vertreter();
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
        let mut t = [
            [n + i, n + j, vertreter[j] as usize],
            [n + i, vertreter[j] as usize, vertreter[i] as usize],
        ];
        if !positiv {
            t.iter_mut().for_each(|d| d.swap(1, 2));
        }
        // A miter: the two inner points are one vertex while the stretch still
        // carries width, so only the wedge can hold the band together.
        let gespalten = vertreter[i] == vertreter[j]
            && abstand(s.innen[i], s.aussen[i]) > NULL_BREITE_MM;
        for (nr, d) in t.iter().enumerate() {
            let [a, b, c] = d.map(|k| s.punkt(k as u32));
            let flaeche = flaeche2(a, b, c);
            if flaeche > MIN_FLAECHE2 {
                s.dreiecke.push(d.map(|k| k as u32));
                continue;
            }
            // The second triangle is degenerate by construction at a miter, so
            // only the wedge (the first) is kept.
            if !gespalten || nr != 0 || flaeche == 0.0 {
                continue;
            }
            let mut w = *d;
            if positiv == (flaeche < 0.0) {
                w.swap(1, 2);
            }
            s.dreiecke.push(w.map(|k| k as u32));
        }
    }
}

/// Arc length of every boundary point along the piece's boundary, from the
/// first one: edge prefixes plus the place within the edge.
///
/// `None` when `orte` names an edge `kanten` does not have, has the wrong
/// number of entries or does not run forward - a wrong order is worse than no
/// weld, since every insert would go to the wrong place.
fn randorte(kanten: &[Kantenzug], orte: &[(usize, f64)], nrand: usize) -> Option<Vec<f64>> {
    if kanten.is_empty() || orte.len() != nrand || orte.len() < 2 {
        return None;
    }
    let mut praefix = Vec::with_capacity(kanten.len());
    let mut summe = 0.0;
    for k in kanten {
        praefix.push(summe);
        summe += k.laenge;
    }
    let mut ort = Vec::with_capacity(orte.len());
    for &(k, v) in orte {
        ort.push(praefix.get(k)? + v);
    }
    if ort.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    Some(ort)
}

/// Where an arc length falls on the closed ring: segment `k` and the fraction
/// `f` from `bogen[k]` to the next ring point.
///
/// `f == 0` means the length is the ring point `k` itself; the caller decides
/// what to do with it. The last segment runs from the last point back to the
/// first, so a length in that stretch is reported as it is, not as a wrap.
fn einordnen(bogen: &[f64], u: f64, t: f64) -> (usize, f64) {
    let n = bogen.len();
    let t = t.rem_euclid(u);
    for k in 0..n {
        let l0 = bogen[k];
        let l1 = if k + 1 < n { bogen[k + 1] } else { u };
        if t >= l0 - BRUCH_EPS && t <= l1 + BRUCH_EPS {
            let f = if l1 - l0 <= 0.0 { 0.0 } else { (t - l0) / (l1 - l0) };
            return (k, f.clamp(0.0, 1.0));
        }
    }
    (n - 1, 1.0)
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
