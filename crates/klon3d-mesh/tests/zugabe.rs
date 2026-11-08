//! The allowance strip inside the mesh, checked against the ring that already
//! exists next to it.
//!
//! Three patterns, every piece: the band's outer edge must be the ring of
//! [`Teilnetz::schnitt`], and the length the band covers plus the stretches it
//! leaves out because they have no width must be the ring length of
//! `klon3d umriss`. Two paths to one number, so a band built from a second
//! approximation of the contour fails here instead of quietly reaching a caller.
//!
//! The expected values are the `Schnitt` column of `klon3d umriss` on the same
//! patterns, to the 0.1 mm it prints.

use std::path::PathBuf;

use klon3d_mesh::schnitt::ringlaenge;
use klon3d_mesh::zugabe::Streifen;
use klon3d_mesh::{vernetze, Musternetz, Parameter, Teilnetz};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_path;

/// The three patterns and the pieces whose `Schnitt` value `klon3d umriss`
/// prints (mm). The same numbers `tests/schnitt.rs` asserts.
const MUSTER: [(&str, &[(&str, f64)]); 3] = [
    (
        "bluse2_v26.sm2d",
        &[
            ("Rueckenteil Mitte Links", 1097.0),
            ("Vorderteil Seite Links", 1091.6),
            ("Aermel Kurz Links", 872.3),
        ],
    ),
    (
        "bluse3_v2.sm2d",
        &[
            ("Rueckenteil Mitte Links", 1092.3),
            ("Untercup Links", 569.4),
            ("Band Mitte", 734.0),
            ("Aermel Kurz Links", 861.2),
        ],
    ),
    (
        "rock_2schlitze_v34.sm2d",
        &[
            ("1 BUND - Umschlag, ganzer Umfang, Naht hinten", 1526.4),
            ("2 VORNE RECHTS - obere Lage an CF", 2557.0),
            ("4 HINTEN RECHTS - obere Lage an CB", 2847.4),
        ],
    ),
];

fn wurzel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

/// The mesh of a pattern without a seam list: this test is about the allowance
/// of a piece, not about how pieces are sewn.
fn netz(muster: &str) -> Musternetz {
    let m = read_path(&wurzel().join(muster)).unwrap();
    let ev = Auswertung::lade(&m, &wurzel().join("measurements.smis")).unwrap();
    vernetze(&ev, Kurvenquelle::Seamly, &[], &Parameter::default()).unwrap()
}

fn teil<'a>(netz: &'a Musternetz, name: &str) -> &'a Teilnetz {
    netz.teile
        .iter()
        .find(|t| t.teil.ends_with(name))
        .unwrap_or_else(|| panic!("kein Teil {name}"))
}

/// Twice the signed area of the triangle `a b c`, positive counter-clockwise.
fn flaeche2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn abstand(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The band covers the ring: its own outer edges plus the stretches without
/// width are the ring length `klon3d umriss` prints, to 0.1 mm.
///
/// That comparison is the independent one: the expected values come from
/// `klon3d umriss`, which deduplicates through its own copy of the rule in
/// `klon3d_app::muster`. The two checks against `Teilnetz::schnitt` below are
/// consistency checks, not a second measurement - since `ringindizes` is shared,
/// band and ring start from the same indices, so they can only disagree if
/// [`crate::zugabe`] reorders, converts or filters wrongly on the way to
/// millimetres. That is worth catching, but it is not evidence about the
/// geometry.
#[test]
fn band_deckt_die_schnittlinie() {
    for (muster, faelle) in MUSTER {
        let n = netz(muster);
        for (name, soll) in faelle {
            let t = teil(&n, name);
            let s = &t.zugabe;
            assert!(!s.ist_leer(), "{muster} {name}: kein Band, obwohl das Teil Zugabe hat");
            assert!(
                s.deckt_die_schnittlinie(&t.schnitt),
                "{muster} {name}: Außenrand {} Punkte, Schnittlinie {}", 
                s.aussen.len(),
                t.schnitt.len()
            );
            let ist = s.aussenlaenge() + s.ohne_breite_mm;
            assert!(
                (ist - soll).abs() <= 0.1,
                "{muster} {name}: Band {ist:.2} mm, erwartet {soll:.1} mm"
            );
            // Consistency only, see the test note: same indices, same ring.
            let ring = ringlaenge(&t.schnitt);
            assert!(
                (ist - ring).abs() <= 0.1,
                "{muster} {name}: Band {ist:.2} mm, Ring {ring:.2} mm"
            );
        }
    }
}

/// Every triangle of the band is wound positive, like the piece's own
/// triangles: no triangle is the reverse side of the fabric.
#[test]
fn band_dreiecke_laufen_positiv() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        let mut geprueft = 0usize;
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            for d in &s.dreiecke {
                let [a, b, c] = d.map(|k| s.punkt(k));
                let f = flaeche2(a, b, c);
                assert!(
                    f > 0.0,
                    "{muster} {}: Dreieck {d:?} hat Fläche {f:.6} mm²",
                    t.teil
                );
                geprueft += 1;
            }
            // Both rings have one point per band point and the same length.
            assert_eq!(s.innen.len(), s.aussen.len(), "{muster} {}", t.teil);
            assert_eq!(s.bogen.len(), s.innen.len(), "{muster} {}", t.teil);
        }
        assert!(geprueft > 0, "{muster}: kein Banddreieck geprüft");
    }
}

/// Every outer point hangs on a boundary point of its own piece and the anchors
/// run along the ring without going back.
///
/// The issue also asks that no two outer points share a boundary point. That is
/// not asserted here, because it is false for every piece: the counts are
/// printed instead, and `anker_sind_ein_zusammenfassung_schluessel` checks what
/// the anchors are actually for.
#[test]
fn anker_liegen_im_rand_und_laufen_vorwaerts() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            assert_eq!(s.anker.len(), s.aussen.len(), "{muster} {}: Ankerzahl", t.teil);
            for (i, &a) in s.anker.iter().enumerate() {
                assert!(
                    (a as usize) < t.nrand,
                    "{muster} {}: Anker {a} von Punkt {i} liegt nicht im Rand ({} Punkte)",
                    t.teil,
                    t.nrand
                );
            }
            for w in s.anker.windows(2) {
                assert!(
                    w[1] >= w[0],
                    "{muster} {}: Anker springt von {} auf {}",
                    t.teil,
                    w[0],
                    w[1]
                );
            }
            let mut belegt = s.anker.clone();
            belegt.sort_unstable();
            belegt.dedup();
            eprintln!(
                "{} {}: {} Außenpunkte auf {} Randpunkten, {} Strecken ohne Breite ({:.3} mm), {} Dreiecke",
                muster,
                t.teil,
                s.anker.len(),
                belegt.len(),
                s.ohne_breite,
                s.ohne_breite_mm,
                s.dreiecke.len()
            );
        }
    }
}

/// The band's arc length and the mesh boundary's arc length are one scale: from
/// the same start, spanning the same curve, so that an anchor can be looked up by
/// arc length at all.
///
/// This is the assumption [`Streifen::anker`] rests on and nothing else states:
/// `bogen` measures along the seam line, `randort` along the mesh boundary, and
/// the two are different polylines - the boundary carries the group divisions,
/// the seam line the contour points. Both run over the same pattern points in
/// the same direction, so their totals agree, and every outer point's arc length
/// falls between its anchor's and the next boundary point's. Measured over all
/// 33 pieces: totals equal to 0.01 mm, residuals from 0.00 mm to 10.0 mm, i.e.
/// at most one boundary edge (10 mm) and never negative.
#[test]
fn anker_und_rand_liegen_auf_derselben_bogenskala() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            // The boundary's arc length per boundary point: edge prefixes plus
            // the place within the edge, the same mapping `anker` uses.
            let mut praefix = Vec::with_capacity(t.kanten.len());
            let mut summe = 0.0;
            for k in &t.kanten {
                praefix.push(summe);
                summe += k.laenge;
            }
            let rand: Vec<f64> = t
                .randort
                .iter()
                .map(|&(k, v)| praefix.get(k).expect("Randort auf fremder Kante") + v)
                .collect();
            let rand_gesamt = summe;
            let naht_gesamt = s.bogen[s.bogen.len() - 1] + abstand(s.innen[s.innen.len() - 1], s.innen[0]);
            assert!(
                (naht_gesamt - rand_gesamt).abs() <= 0.001 * rand_gesamt,
                "{muster} {}: Naht {naht_gesamt:.2} mm, Rand {rand_gesamt:.2} mm",
                t.teil
            );
            assert_eq!(rand.len(), t.nrand, "{muster} {}: Randorte", t.teil);
            for (i, &a) in s.anker.iter().enumerate() {
                let a = a as usize;
                // Residual on the ring: the outer point sits after its anchor,
                // and before the next boundary point (over the closing edge too).
                let mut res = s.bogen[i] - rand[a];
                if res < 0.0 {
                    res += rand_gesamt;
                }
                assert!(
                    res >= -1e-9,
                    "{muster} {}: Punkt {i} liegt {res:.3} mm vor seinem Anker {a}",
                    t.teil
                );
                let luecke = if a + 1 < rand.len() {
                    rand[a + 1] - rand[a]
                } else {
                    rand_gesamt - rand[a]
                };
                assert!(
                    res <= luecke + 1e-9,
                    "{muster} {}: Punkt {i} liegt {res:.3} mm hinter Anker {a}, die nächste Randstrecke ist nur {luecke:.3} mm",
                    t.teil
                );
            }
        }
    }
}

/// The band lies outside the seam line: the cutting line encloses a larger area
/// than the seam line. A band mirrored onto the seam line fails here.
#[test]
fn band_liegt_aussen() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            let aussen = ringflaeche(&s.aussen).abs();
            let innen = ringflaeche(&s.innen).abs();
            assert!(
                aussen > innen,
                "{muster} {}: Außenring {aussen:.2} mm², Nahtring {innen:.2} mm²",
                t.teil
            );
        }
    }
}

/// A piece whose nodes carry no allowance keeps an empty band, not a degenerate
/// one: "no allowance" and "allowance of zero" are different statements.
#[test]
fn ohne_zugabe_leeres_band() {
    let n = netz("bluse2_v26.sm2d");
    // The facings declare `width="0"` and behave like `seamAllowance="false"`;
    // `klon3d umriss` prints no `Schnitt` column for them.
    for name in ["Scheibe Saum Links", "Halbe Scheibe Kragenende Links"] {
        let t = teil(&n, name);
        assert!(t.schnitt.is_empty(), "{name}: Schnittlinie, obwohl ohne Zugabe");
        assert_eq!(t.zugabe, Streifen::default(), "{name}: Band, obwohl ohne Zugabe");
        assert!(t.zugabe.ist_leer(), "{name}: Band, obwohl ohne Zugabe");
    }
}

/// A piece with an allowance has a band whose own ring is not the seam line: at
/// least somewhere the two differ, or the band would carry no fabric at all.
#[test]
fn band_hat_breite() {
    let n = netz("bluse2_v26.sm2d");
    let t = teil(&n, "Rueckenteil Mitte Links");
    let s = &t.zugabe;
    assert!(!s.ist_leer(), "Rueckenteil Mitte Links: kein Band");
    let breit = s
        .innen
        .iter()
        .zip(&s.aussen)
        .filter(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) > 0.1)
        .count();
    assert!(breit > s.innen.len() / 2, "nur {breit} von {} Punkten mit Breite", s.innen.len());
    assert!(s.ohne_breite < s.innen.len(), "{} von {} Strecken ohne Breite", s.ohne_breite, s.innen.len());
}

/// Every stretch of the ring is accounted for exactly once: covered by its two
/// triangles, or left out as a stretch without width. That is an identity, not
/// a tolerance - the 0.1 mm of the acceptance is the other direction (against
/// the printed ring length), this one has to be exact.
#[test]
fn jede_strecke_ist_gedeckt_oder_ohne_breite() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            let summe = s.aussenlaenge() + s.ohne_breite_mm;
            let ring = ringlaenge(&s.aussen);
            assert!(
                (summe - ring).abs() <= 1e-6,
                "{muster} {}: Strecken {summe:.6} mm, Außenring {ring:.6} mm",
                t.teil
            );
            assert!(
                s.ohne_breite <= s.innen.len(),
                "{muster} {}: {} Strecken ohne Breite bei {} Ringpunkten",
                t.teil,
                s.ohne_breite,
                s.innen.len()
            );
        }
    }
}

/// The anchors are a coarsening, not a one-to-one map, and that is geometry,
/// not a defect: the mesh boundary has edges of about 10 mm, the contour points
/// can be 4 mm apart, so several outer points necessarily hang on one boundary
/// point. The acceptance asks for unique anchors; measured over all 33 pieces of
/// the three patterns that carry a strip, each has 1.34 to 3.19 times as many
/// outer points as boundary points - not one is 1:1, and no band that follows
/// the drawn cutting line can be. What is checked here is the property that
/// makes the anchors useful: they never run backwards and every boundary point
/// that is used is used by an uninterrupted run of outer points.
#[test]
fn anker_sind_ein_zusammenfassung_schluessel() {
    let n = netz("bluse2_v26.sm2d");
    let t = teil(&n, "Rueckenteil Mitte Links");
    let s = &t.zugabe;
    assert!(!s.anker.is_empty(), "keine Anker");
    let mut lagen: Vec<u32> = s.anker.clone();
    lagen.dedup();
    assert!(lagen.len() < s.anker.len(), "Anker sind 1:1 - dann prüft dieser Test nichts");
    // Jeder Anker kommt in genau einem Stück vor: die Folge zerfällt in Blöcke.
    for (i, &a) in s.anker.iter().enumerate() {
        let erste = s.anker.iter().position(|&b| b == a).unwrap();
        let letzte = s.anker.iter().rposition(|&b| b == a).unwrap();
        assert!(
            erste <= i && i <= letzte,
            "Anker {a} kommt an Stelle {i} ausserhalb seines Blocks {erste}..={letzte} vor"
        );
    }
}

/// Twice the signed area of the closed ring.
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
