//! The cutting line of a piece for `klon3d umriss`: the seam line displaced by
//! the allowance of its nodes.
//!
//! The same construction the DXF export uses (#78) and the 2D view draws (#80):
//! [`Saumkontur::aus_teilrand`] with [`Ecken::Eigen`], so a free edge
//! (`after = 0`) stays free instead of gaining an allowance. The command line
//! must not depend on `klon3d-app`, so the deduplication is repeated here -
//! same rule, no shared code.

use klon3d_pattern::geom::Pt;
use klon3d_pattern::kanten::Teilrand;
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};

/// Below this two coordinates count as the same point.
const EPS: f64 = 1e-9;

/// One ring out of [`Saumkontur::punkte`]: drops repeated points and the
/// closing point.
///
/// `punkte()` lists both ends of every segment, so a corner that the arithmetic
/// leaves split appears twice; a closed ring must not repeat its first point at
/// the end. Same rule as `klon3d_app::muster::bereinigt`.
pub fn bereinigt(punkte: &[Pt]) -> Vec<Pt> {
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

/// The cutting line of a piece, or `None` when it has none to show.
///
/// `None` for a piece whose seam line had to fall back to
/// [`klon3d_pattern::Auswertung::umriss_aus_knoten`] - there is no
/// [`Teilrand`], so there are no allowances to displace by, and none is guessed.
/// (None of the test patterns takes that fallback; the bishop sleeve of Bolero
/// v37 used to and resolves to Seamly2D's line today.) `None` also when the
/// nodes carry no allowance at all: the contour would only be a copy of the
/// seam line, and the console must print no column for it.
/// A negative allowance is a reduction, not "no allowance", so the magnitude
/// decides.
pub fn zugabekontur(rand: Option<&Teilrand>) -> Option<Vec<Pt>> {
    let rand = rand?;
    if rand.kanten.is_empty() || rand.punktknoten.is_empty() {
        return None;
    }
    if !rand
        .punktknoten
        .iter()
        .any(|k| k.zugabe_nach.unwrap_or(0.0).abs() > EPS)
    {
        return None;
    }
    let kontur = bereinigt(Saumkontur::aus_teilrand(rand, Ecken::Eigen).punkte());
    (kontur.len() >= 3).then_some(kontur)
}

/// The length of a closed ring in pattern units: every edge plus the closing
/// one.
pub fn ringlaenge(ring: &[Pt]) -> f64 {
    if ring.len() < 2 {
        return 0.0;
    }
    let mut summe = 0.0;
    for i in 0..ring.len() {
        summe += ring[i].dist(ring[(i + 1) % ring.len()]);
    }
    summe
}

/// The `d` attribute of a closed path in millimetres, `y` down like the `.sm2d`
/// and the seam lines around it.
pub fn pfad(punkte: &[Pt], x0: f64, y0: f64, f: f64) -> String {
    let mut d = String::new();
    for (i, p) in punkte.iter().enumerate() {
        d.push_str(&format!(
            "{}{:.3} {:.3}",
            if i == 0 { "M" } else { " L" },
            (p.x - x0) * f,
            (p.y - y0) * f
        ));
    }
    d.push_str(" Z");
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bereinigt_entfernt_doppelte_und_schluss() {
        let p = [
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 1.0, y: 0.0 },
            Pt { x: 1.0, y: 1.0 },
            Pt { x: 0.0, y: 0.0 },
        ];
        assert_eq!(bereinigt(&p), vec![p[0], p[2], p[3]]);
    }

    #[test]
    fn ringlaenge_zaehlt_die_schlusskante() {
        let q = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 3.0, y: 0.0 },
            Pt { x: 3.0, y: 4.0 },
        ];
        assert!((ringlaenge(&q) - 12.0).abs() < 1e-12);
        assert_eq!(ringlaenge(&q[..1]), 0.0);
    }

    #[test]
    fn ohne_rand_keine_kontur() {
        assert_eq!(zugabekontur(None), None);
    }
}
