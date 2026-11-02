//! Cleaning a piece's seam line the way Seamly2D does before it uses it.
//!
//! Seamly2D does not take the chained node points as the seam line as they
//! are: `VPiece::mainPathPoints` runs them through
//! `CheckLoops(CorrectEquidistantPoints(points))` (`vpiece.cpp`,
//! `vabstractpiece.cpp/.h`). Rebuilt here from reading those sources (GPL,
//! not copied):
//!
//! * [`korrigieren`] (`CorrectEquidistantPoints`): drop the last point if it
//!   repeats the first, drop a point that lies within the accuracy of its
//!   predecessor in both coordinates, then drop **spikes** — a point on the
//!   line through its neighbours whose two neighbours are the same point. A
//!   point that merely lies on a straight run stays.
//! * [`schleifen_entfernen`] (`CheckLoops`): for each segment, search from the
//!   end for a later, non-adjacent segment that crosses it (bounded
//!   intersection) or lies on the same line and overlaps it (parallel case);
//!   [`schnitt_pruefen`] (`CheckIntersection`) decides with the signed areas
//!   of the two sub-paths and whether they overlap. The loop between is cut
//!   out: kept are the start of the segment and the crossing point.
//!
//! Example (L036, "back", finding 123): the centre back runs hem → GF → SKI
//! and back down to the zip end, which lies below GF. The segment from the zip
//! end sideways touches the first segment, the run between is cut out, and
//! the seam line is 2294.16 mm instead of 2727.4.
//!
//! Two deliberate differences, both without effect on the shape: the points
//! of point nodes are not merged away as near duplicates (the later of two
//! near points is kept if only it is a node point, two node points both
//! stay), so an edge between two nodes keeps its exact ends; and repeated
//! points left by a crossing, or a collapsed piece ending where it began, are
//! dropped at the end ([`naht_bereinigen_fest`]). On L036 the seam lines of
//! all five pieces agree with Seamly2D's export to 0.001 mm in both
//! directions.
//!
//! Everything runs in Seamly2D's pixels (96 dpi), like its accuracy of
//! 0.1555 mm (`accuracyPointOnLine`). Point comparisons are Qt's fuzzy
//! `QPointF ==`.
//!
//! Each function also returns, for every input point, the output index that
//! stands for it: its own if it survives, otherwise the point that replaces it
//! (the crossing point for a loop, the next kept point for a spike, the kept
//! one for a duplicate). Callers move node marks with it.

/// `accuracyPointOnLine`: 0.1555 mm in pixels.
pub const GENAUIGKEIT_PX: f64 = 0.1555 / 25.4 * 96.0;

type P = [f64; 2];

/// `qFuzzyIsNull` / `qFuzzyCompare` for one coordinate, as `QPointF ==`.
fn gleich_wert(a: f64, b: f64) -> bool {
    if a == 0.0 || b == 0.0 {
        (a - b).abs() <= 1e-12
    } else {
        (a - b).abs() * 1e12 <= a.abs().min(b.abs())
    }
}

/// `QPointF ==`.
pub fn gleich(a: P, b: P) -> bool {
    gleich_wert(a[0], b[0]) && gleich_wert(a[1], b[1])
}

/// `VGObject::PerpDotProduct`.
fn pdp(p1: P, p2: P, t: P) -> f64 {
    (p1[0] - t[0]) * (p2[1] - t[1]) - (p1[1] - t[1]) * (p2[0] - t[0])
}

/// `VGObject::IsPointOnLineviaPDP`: `t` at most the accuracy away from the
/// **line** through `p1` and `p2` (not the segment).
pub fn auf_linie(t: P, p1: P, p2: P) -> bool {
    let p = pdp(p1, p2, t).abs();
    // `GetEpsilon`: the same product for a point at the accuracy distance
    // perpendicular to the line, i.e. its length times the accuracy.
    let e = ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt() * GENAUIGKEIT_PX;
    p < e || {
        // `VFuzzyComparePossibleNulls`
        if p.abs() <= 1e-12 {
            e.abs() <= 1e-12
        } else if e.abs() <= 1e-12 {
            false
        } else {
            (p - e).abs() * 1e12 <= p.abs().min(e.abs())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Schnitt {
    /// parallel (or degenerate)
    Keiner,
    Unbegrenzt,
    Begrenzt,
}

/// `QLineF::intersects` (Qt 6): the point is computed on the first line.
fn linien_schnitt(p1: P, p2: P, q1: P, q2: P) -> (Schnitt, P) {
    let a = [p2[0] - p1[0], p2[1] - p1[1]];
    let b = [q1[0] - q2[0], q1[1] - q2[1]];
    let c = [p1[0] - q1[0], p1[1] - q1[1]];
    let nenner = a[1] * b[0] - a[0] * b[1];
    if nenner == 0.0 || !nenner.is_finite() {
        return (Schnitt::Keiner, [0.0, 0.0]);
    }
    let k = 1.0 / nenner;
    let na = (b[1] * c[0] - b[0] * c[1]) * k;
    let punkt = [p1[0] + a[0] * na, p1[1] + a[1] * na];
    if !(0.0..=1.0).contains(&na) {
        return (Schnitt::Unbegrenzt, punkt);
    }
    let nb = (a[0] * c[1] - a[1] * c[0]) * k;
    if !(0.0..=1.0).contains(&nb) {
        return (Schnitt::Unbegrenzt, punkt);
    }
    (Schnitt::Begrenzt, punkt)
}

/// `QLineF(p1, p2)` turned by +90° about `p1`, same length (`setAngle`; Qt's
/// angles count counter-clockwise with y down).
fn gedreht90(p1: P, p2: P) -> (P, P) {
    let (dx, dy) = (p2[0] - p1[0], p2[1] - p1[1]);
    let l = (dx * dx + dy * dy).sqrt();
    let w = (-dy).atan2(dx).to_degrees() + 90.0;
    let r = w.to_radians();
    (p1, [p1[0] + r.cos() * l, p1[1] - r.sin() * l])
}

/// `VAbstractPiece::sumTrapezoids`: twice the signed area.
fn trapeze(p: &[P]) -> f64 {
    let n = p.len();
    if n <= 2 {
        return 0.0;
    }
    (0..n)
        .map(|i| {
            let vor = p[(i + n - 1) % n];
            let nach = p[(i + 1) % n];
            p[i][0] * (vor[1] - nach[1])
        })
        .sum()
}

/// `VAbstractPiece::SubPath`: from `start` to `ende` inclusive, wrapping.
fn teilpfad(p: &[P], start: usize, ende: usize) -> Vec<P> {
    if p.is_empty() || start >= p.len() || ende >= p.len() || start == ende {
        return p.to_vec();
    }
    let mut out = Vec::new();
    let mut i = start;
    loop {
        out.push(p[i]);
        if i == ende {
            break;
        }
        i = (i + 1) % p.len();
    }
    out
}

/// `QRectF::intersects` of the bounding boxes: a box without width or height
/// intersects nothing.
fn rahmen_schneiden(a: &[P], b: &[P]) -> bool {
    let rahmen = |p: &[P]| {
        let mut r = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for q in p {
            r[0] = r[0].min(q[0]);
            r[1] = r[1].min(q[1]);
            r[2] = r[2].max(q[0]);
            r[3] = r[3].max(q[1]);
        }
        r
    };
    let (ra, rb) = (rahmen(a), rahmen(b));
    if ra[0] == ra[2] || rb[0] == rb[2] || ra[1] == ra[3] || rb[1] == rb[3] {
        return false;
    }
    !(ra[0] >= rb[2] || rb[0] >= ra[2] || ra[1] >= rb[3] || rb[1] >= ra[3])
}

/// Winding number of `t` around the closed polygon `p` (non-zero = inside).
fn innen(p: &[P], t: P) -> bool {
    let mut w = 0i32;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let seite = (b[0] - a[0]) * (t[1] - a[1]) - (t[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= t[1] {
            if b[1] > t[1] && seite > 0.0 {
                w += 1;
            }
        } else if b[1] <= t[1] && seite < 0.0 {
            w -= 1;
        }
    }
    w != 0
}

/// Do two segments meet (touching counts)?
fn strecken_treffen(a1: P, a2: P, b1: P, b2: P) -> bool {
    let o = |p: P, q: P, r: P| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let auf = |p: P, q: P, r: P| {
        r[0] >= p[0].min(q[0]) && r[0] <= p[0].max(q[0]) && r[1] >= p[1].min(q[1]) && r[1] <= p[1].max(q[1])
    };
    let (d1, d2, d3, d4) = (o(b1, b2, a1), o(b1, b2, a2), o(a1, a2, b1), o(a1, a2, b2));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    (d1 == 0.0 && auf(b1, b2, a1))
        || (d2 == 0.0 && auf(b1, b2, a2))
        || (d3 == 0.0 && auf(a1, a2, b1))
        || (d4 == 0.0 && auf(a1, a2, b2))
}

/// `VAbstractPiece::Crossing`: do the two closed polygons (winding fill)
/// overlap? Box test as `QRectF::intersects`, then as `QPathClipper::
/// intersect`: an edge of one meets an edge of the other, or the start of
/// one lies inside the other.
fn kreuzen(a: &[P], b: &[P]) -> bool {
    if a.is_empty() || b.is_empty() || !rahmen_schneiden(a, b) {
        return false;
    }
    for i in 0..a.len() {
        let (a1, a2) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            if strecken_treffen(a1, a2, b[j], b[(j + 1) % b.len()]) {
                return true;
            }
        }
    }
    innen(a, b[0]) || innen(b, a[0])
}

/// `VAbstractPiece::CheckIntersection`: is the loop between segment `i` and
/// segment `j` one to cut out?
fn schnitt_pruefen(p: &[P], i: usize, i_nach: usize, j: usize, j_nach: usize, kreuz: P) -> bool {
    let mut teil1 = teilpfad(p, i_nach, j);
    teil1.push(kreuz);
    let teil1 = schleifen_entfernen(&korrigieren(&teil1, false).0).0;
    let s1 = trapeze(&teil1);
    let mut teil2 = teilpfad(p, j_nach, i);
    teil2.push(kreuz);
    let teil2 = schleifen_entfernen(&korrigieren(&teil2, false).0).0;
    let s2 = trapeze(&teil2);
    if s1 < 0.0 && s2 < 0.0 {
        kreuzen(&teil1, &teil2)
    } else {
        !kreuzen(&teil1, &teil2)
    }
}

/// `VAbstractPiece::ParallelCrossPoint`.
fn parallel_kreuz(l1: (P, P), l2: (P, P)) -> Option<P> {
    if gleich(l1.1, l2.1) || gleich(l1.1, l2.0) {
        Some(l1.1)
    } else if gleich(l1.0, l2.0) || gleich(l1.0, l2.1) {
        Some(l1.0)
    } else {
        None
    }
}

/// `VAbstractPiece::RemoveDublicates`, with the index map.
///
/// `fest` marks points that must keep their place (the points of point
/// nodes): of two points within the accuracy Seamly2D keeps the first; if
/// only the second is fixed, it is kept instead, and two fixed points both
/// stay unless they are the same point. Seamly2D's seam line can therefore differ here by at most the
/// accuracy next to a node; edges between nodes keep their exact ends and
/// never shrink to nothing.
fn doppelte_entfernen(p: &[P], erste_letzte: bool, fest: &[bool]) -> (Vec<P>, Vec<usize>) {
    let nah = |a: P, b: P| (a[0] - b[0]).abs() < GENAUIGKEIT_PX && (a[1] - b[1]).abs() < GENAUIGKEIT_PX;
    let ist_fest = |k: usize| fest.get(k).copied().unwrap_or(false);
    let mut n = p.len();
    let mut letzte_weg = false;
    if erste_letzte && n > 1 && nah(p[0], p[n - 1]) && (!ist_fest(n - 1) || gleich(p[0], p[n - 1])) {
        n -= 1;
        letzte_weg = true;
    }
    let mut out: Vec<P> = Vec::with_capacity(n);
    let mut out_fest: Vec<bool> = Vec::with_capacity(n);
    let mut karte = vec![0usize; p.len()];
    for k in 0..n {
        if let (Some(&l), Some(&lf)) = (out.last(), out_fest.last()) {
            if nah(l, p[k]) && !(lf && ist_fest(k) && !gleich(l, p[k])) {
                if ist_fest(k) && !lf {
                    *out.last_mut().unwrap() = p[k];
                    *out_fest.last_mut().unwrap() = true;
                }
                karte[k] = out.len() - 1;
                continue;
            }
        }
        out.push(p[k]);
        out_fest.push(ist_fest(k));
        karte[k] = out.len() - 1;
    }
    if letzte_weg {
        karte[p.len() - 1] = 0;
    }
    (out, karte)
}

/// `VAbstractPiece::CorrectEquidistantPoints`: duplicates and spikes out.
/// Returns the points and, per input point, the output index standing for it.
pub fn korrigieren(p: &[P], erste_letzte: bool) -> (Vec<P>, Vec<usize>) {
    korrigieren_fest(p, erste_letzte, &[])
}

/// [`korrigieren`] with fixed points ([`doppelte_entfernen`]).
pub fn korrigieren_fest(p: &[P], erste_letzte: bool, fest: &[bool]) -> (Vec<P>, Vec<usize>) {
    let identisch = || (p.to_vec(), (0..p.len()).collect());
    if p.len() < 4 {
        return identisch();
    }
    let (b1, karte1) = doppelte_entfernen(p, erste_letzte, fest);
    if b1.len() < 3 {
        return (b1, karte1);
    }
    let n = b1.len();
    let mut fest1 = vec![false; n];
    for (k, &o) in karte1.iter().enumerate() {
        if fest.get(k).copied().unwrap_or(false) {
            fest1[o] = true;
        }
    }
    let mut b2: Vec<P> = Vec::with_capacity(n);
    let mut fest2: Vec<bool> = Vec::with_capacity(n);
    // Index in b2 of every b1 point; a dropped spike gets the next kept one.
    let mut karte2: Vec<Option<usize>> = vec![None; n];
    let mut vor: Option<usize> = None;
    for i in 0..n {
        let v = vor.unwrap_or(if i == 0 { n - 1 } else { i - 1 });
        vor = Some(v);
        let nach = if i == n - 1 { 0 } else { i + 1 };
        let (ip, vp, np) = (b1[i], b1[v], b1[nach]);
        let spitze = auf_linie(ip, vp, np) && gleich(vp, np);
        let rand = (i == 0 || i == n - 1) && (gleich(ip, vp) || gleich(ip, np));
        if !spitze || rand {
            b2.push(ip);
            fest2.push(fest1[i]);
            karte2[i] = Some(b2.len() - 1);
            vor = None;
        }
    }
    let m = b2.len();
    let mut ersatz = vec![0usize; n];
    let mut naechste = karte2.iter().flatten().next().copied().unwrap_or(0);
    for i in (0..n).rev() {
        if let Some(k) = karte2[i] {
            naechste = k;
        }
        ersatz[i] = naechste.min(m.saturating_sub(1));
    }
    let (b3, karte3) = doppelte_entfernen(&b2, false, &fest2);
    let karte = karte1.iter().map(|&k| karte3.get(ersatz[k]).copied().unwrap_or(0)).collect();
    (b3, karte)
}

/// `VAbstractPiece::CheckLoops`: loops cut out. Returns the points and, per
/// input point, the output index standing for it.
pub fn schleifen_entfernen(p: &[P]) -> (Vec<P>, Vec<usize>) {
    let n = p.len();
    if n < 4 {
        return (p.to_vec(), (0..n).collect());
    }
    let geschlossen = gleich(p[0], p[n - 1]);
    let mut out: Vec<P> = Vec::with_capacity(n);
    let mut karte = vec![0usize; n];
    let mut i = 0usize;
    while i < n {
        if i + 3 > n {
            karte[i] = out.len();
            out.push(p[i]);
            i += 1;
            continue;
        }
        let l1 = (p[i], p[i + 1]);
        let mut gefunden: Option<(usize, usize, Option<P>)> = None;
        let mut j = n - 1;
        while j >= i + 2 {
            let j_nach = if j == n - 1 { 0 } else { j + 1 };
            let l2 = (p[j], p[j_nach]);
            let l2_laenge = ((l2.1[0] - l2.0[0]).powi(2) + (l2.1[1] - l2.0[1]).powi(2)).sqrt();
            if l2_laenge.abs() > 1e-12 {
                let mut ecken = vec![i, i + 1];
                for e in [j, if geschlossen && j_nach == n - 1 { 0 } else { j_nach }] {
                    if !ecken.contains(&e) {
                        ecken.push(e);
                    }
                }
                let (art, kreuz) = linien_schnitt(l1.0, l1.1, l2.0, l2.1);
                match art {
                    Schnitt::Keiner => {
                        if auf_linie(p[j], p[i], p[i + 1]) && ecken.len() == 4 {
                            let t1 = gedreht90(l1.0, l1.1);
                            let s1 = linien_schnitt(t1.0, t1.1, l2.0, l2.1).0;
                            let t2 = gedreht90(l2.0, l2.1);
                            let s2 = linien_schnitt(l1.0, l1.1, t2.0, t2.1).0;
                            if s1 == Schnitt::Begrenzt || s2 == Schnitt::Begrenzt {
                                let c = parallel_kreuz(l1, l2);
                                let ok = match c {
                                    None => true,
                                    Some(c) => schnitt_pruefen(p, i, i + 1, j, j_nach, c),
                                };
                                if ok {
                                    gefunden = Some((j, j_nach, None));
                                    break;
                                }
                            }
                        }
                    }
                    Schnitt::Begrenzt => {
                        if ecken.len() == 4
                            && ((!gleich(l1.0, kreuz) && !gleich(l1.1, kreuz) && !gleich(l2.0, kreuz) && !gleich(l2.1, kreuz))
                                || schnitt_pruefen(p, i, i + 1, j, j_nach, kreuz))
                        {
                            gefunden = Some((j, j_nach, Some(kreuz)));
                            break;
                        }
                    }
                    Schnitt::Unbegrenzt => {}
                }
            }
            if j == 0 {
                break;
            }
            j -= 1;
        }
        match gefunden {
            // parallel: keep i and the end of the overlapping segment
            Some((j, j_nach, None)) => {
                karte[i] = out.len();
                out.push(p[i]);
                let weiter = if j_nach > j { j_nach } else { j };
                let ziel = out.len();
                out.push(p[j_nach]);
                for k in i + 1..=weiter.min(n - 1) {
                    karte[k] = ziel;
                }
                karte[j_nach] = ziel;
                i = weiter + 1;
            }
            // crossing: keep i and the crossing point
            Some((j, _, Some(kreuz))) => {
                karte[i] = out.len();
                out.push(p[i]);
                let ziel = out.len();
                out.push(kreuz);
                for k in i + 1..=j {
                    karte[k] = ziel;
                }
                i = j + 1;
            }
            None => {
                karte[i] = out.len();
                out.push(p[i]);
                i += 1;
            }
        }
    }
    (out, karte)
}

/// Both steps of `mainPathPoints`, with the combined index map.
pub fn naht_bereinigen(p: &[P]) -> (Vec<P>, Vec<usize>) {
    naht_bereinigen_fest(p, &[])
}

/// [`naht_bereinigen`] with fixed points ([`doppelte_entfernen`]).
pub fn naht_bereinigen_fest(p: &[P], fest: &[bool]) -> (Vec<P>, Vec<usize>) {
    let (a, k1) = korrigieren_fest(p, true, fest);
    let (b, k2) = schleifen_entfernen(&a);
    let (c, k3) = gleiche_entfernen(&b);
    let karte = k1.iter().map(|&k| k3[k2.get(k).copied().unwrap_or(0)]).collect();
    (c, karte)
}

/// A crossing point can repeat its neighbour, and a collapsed piece can end
/// where it began; Seamly2D leaves such repeats in its list. They carry no
/// geometry, so they go here, so that a seam line that collapsed to a stroke
/// also counts as fewer than three points.
fn gleiche_entfernen(p: &[P]) -> (Vec<P>, Vec<usize>) {
    let mut out: Vec<P> = Vec::with_capacity(p.len());
    let mut karte = vec![0usize; p.len()];
    for (k, &q) in p.iter().enumerate() {
        if out.last().is_some_and(|&l| gleich(l, q)) {
            karte[k] = out.len() - 1;
            continue;
        }
        out.push(q);
        karte[k] = out.len() - 1;
    }
    while out.len() > 1 && gleich(out[0], *out.last().unwrap()) {
        let letzte = out.len() - 1;
        out.pop();
        for k in karte.iter_mut().filter(|k| **k == letzte) {
            *k = 0;
        }
    }
    (out, karte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_run_is_kept_spike_is_dropped() {
        // a point on a straight run stays
        let p = [[0.0, 0.0], [50.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        assert_eq!(korrigieren(&p, true).0.len(), 5);
        // a spike out and back to the same point goes
        let p = [[0.0, 0.0], [100.0, 0.0], [150.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let (q, k) = korrigieren(&p, true);
        assert_eq!(q, vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]);
        assert_eq!(k[2], 1, "the spike tip stands for the next kept point");
    }
}
