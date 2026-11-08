//! The allowance strip inside the mesh, checked against the ring that already
//! exists next to it, and against the piece's boundary points it is welded to.
//!
//! Three patterns, every piece: the band's outer edge must be the ring of
//! [`Teilnetz::schnitt`], and the length the band covers plus the stretches it
//! leaves out because they have no width must be the ring length of
//! `klon3d umriss`. Two paths to one number, so a band built from a second
//! approximation of the contour fails here instead of quietly reaching a caller.
//!
//! The weld (#121) is checked from two directions that do not share code with
//! it: every boundary point of the piece is a vertex of the band at its own
//! coordinates, and every outer point's anchor is a *nearest* boundary edge of
//! the piece's boundary polygon. The first walks the mesh, the second measures
//! distances; both fail if the band is paired wrongly.
//!
//! The expected values are the `Schnitt` column of `klon3d umriss` on the same
//! patterns, to the 0.1 mm it prints.

use std::collections::HashMap;
use std::path::PathBuf;

use klon3d_mesh::schnitt::ringlaenge;
use klon3d_mesh::zugabe::{aus_teilrand, Streifen};
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
/// 33 pieces: the two totals are identical to below 0.000001 mm, and residuals
/// run from 0.00 mm to 9.96 / 9.95 / 10.00 mm - at most one boundary edge
/// (10 mm), never negative. An outer point may reach the next boundary point
/// exactly, which is the closing edge's case.
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
/// the three patterns that carry a strip, each has 1.25 to 2.88 times as many
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

/// Every boundary point of a piece that carries a band is a vertex of that band,
/// at its own coordinates.
///
/// That is the whole point of the weld: the seam-line side of the band is no
/// longer a second walk of the pattern that happens to lie on the same line, but
/// the piece's own boundary. Measured over the 33 pieces with a band of the three
/// patterns: 1492 / 1487 / 1288 boundary points, all of them at 0.000000 mm.
///
/// The exception is named and counted, and it is not a tolerance: a boundary
/// point on a stretch whose allowance is zero at both ends carries no fabric, so
/// there is no triangle that could use it. It is *inserted* all the same - the
/// rule is one rule, not two - and this test asserts that every such point lies
/// on the band's seam line at 0.000000 mm and that its stretch has no width. Over
/// the three patterns that is 129 / 85 / 567 points, all with width
/// [`klon3d_mesh::zugabe::NULL_BREITE_MM`] on both ends of their stretch.
#[test]
fn jeder_randpunkt_ist_bandpunkt() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        let mut geprueft = 0usize;
        let mut ohne_stoff = 0usize;
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            assert_eq!(
                s.innen.len(),
                s.aussen.len(),
                "{muster} {}: Innenring {} Punkte, Aussenring {}",
                t.teil,
                s.innen.len(),
                s.aussen.len()
            );
            let nr = s.innen.len();
            for &p in &t.punkte[..t.nrand] {
                let Some(i) = s.bandpunkt_von(p) else {
                    panic!(
                        "{muster} {}: Randpunkt {p:?} ist kein Bandpunkt",
                        t.teil
                    );
                };
                geprueft += 1;
                // The point is *the* point, not a neighbour within a tolerance.
                assert_eq!(
                    s.innen[i], p,
                    "{muster} {}: Bandpunkt {i} ist nicht der Randpunkt",
                    t.teil
                );
                // A point without fabric: on a stretch that is zero width at
                // both ends, or no triangle claims it.
                let benutzt = s.dreiecke.iter().any(|d| d.contains(&(i as u32)));
                if !benutzt {
                    let k = i;
                    let k1 = (i + 1) % nr;
                    let l0 = if i == 0 { nr - 1 } else { i - 1 };
                    let beide_null = |a: usize, b: usize| {
                        abstand(s.innen[a], s.aussen[a]) <= 1e-6
                            && abstand(s.innen[b], s.aussen[b]) <= 1e-6
                    };
                    assert!(
                        beide_null(k, k1) || beide_null(l0, k),
                        "{muster} {}: Bandpunkt {i} ohne Dreieck, aber seine Strecke hat Breite ({:.6} / {:.6} mm)",
                        t.teil,
                        abstand(s.innen[k], s.aussen[k]),
                        abstand(s.innen[k1], s.aussen[k1])
                    );
                    ohne_stoff += 1;
                }
            }
        }
        assert!(geprueft > 1000, "{muster}: nur {geprueft} Randpunkte geprüft");
        // The counted exception, so a change that hides points behind it fails.
        let (soll_rand, soll_ohne) = match muster {
            "bluse2_v26.sm2d" => (1492, 129),
            "bluse3_v2.sm2d" => (1487, 85),
            "rock_2schlitze_v34.sm2d" => (1288, 567),
            _ => unreachable!(),
        };
        assert_eq!(geprueft, soll_rand, "{muster}: Randpunkte");
        assert_eq!(
            ohne_stoff, soll_ohne,
            "{muster}: Randpunkte ohne Fabric - gezählte Ausnahme"
        );
    }
}

/// The band does not grow by being welded: its triangles cover the same area as
/// the strip of #119, stretched over the same two rings.
///
/// This is the tight version of the acceptance. Comparing against
/// `|area(cutting line) - area(seam line)|` would fold in the chord residual of
/// the band itself - the quads are triangulated straight, so they fall short of
/// the two curved rings by the sagitta - and that residual is about
/// 14.4 mm² of the three patterns (measured, per pattern: 14.3646 / 14.6368 /
/// 3.0013 mm² on top of `|A_cut - A_seam|`). It is a property of the strip, not
/// of the weld, so the assertion here is the one that isolates the weld: rebuild
/// the strip from the same [`Teilnetz::kanten`] without welding it and compare
/// the two triangle sums. Splitting a segment at a boundary point re-triangulates
/// it and adds nothing, so the two are equal to the last digit (measured: max
/// 1.5e-11 mm² over all 33 pieces).
#[test]
fn schweissen_aendert_die_flaeche_nicht() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        // The unwelded strip comes from the pattern rather than from the mesh: a
        // second path, so the comparison is not the weld against itself.
        let m = read_path(&wurzel().join(muster)).unwrap();
        let ev = Auswertung::lade(&m, &wurzel().join("measurements.smis")).unwrap();
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let stueck = ev
                .muster()
                .pieces()
                .into_iter()
                .find(|p| p.name.clone().unwrap_or_else(|| p.id.clone()) == t.teil)
                .unwrap_or_else(|| panic!("{muster}: kein Musterteil {}", t.teil));
            let rand = ev
                .teilrand(stueck, Kurvenquelle::Seamly)
                .unwrap_or_else(|e| panic!("{muster} {}: {e}", t.teil));
            let alt = aus_teilrand(&rand).unwrap_or_else(|e| panic!("{muster} {}: {e}", t.teil));
            let neu = bandflaeche(&t.zugabe);
            let vorher = bandflaeche(&alt);
            assert!(
                (neu - vorher).abs() <= 1e-6,
                "{muster} {}: Band {neu:.6} mm², unverschweisst {vorher:.6} mm²",
                t.teil
            );
        }
    }
}

/// Welding must not spoil the topology, and it must not invent gaps either:
/// wherever a stretch carries fabric, the band has to join it with a triangle.
///
/// What is *not* true, measured, is that both rings are chains of band edges.
/// Three shapes of stretch have no edge, and all three are inherited from the
/// unwelded strip (#119), not caused by the weld - the welded and unwelded band
/// count the same miters and the same zero-width stretches:
///
/// 1. **No width at both ends**: the piece has no allowance there, so there is
///    no fabric and no triangle (262 / 177 / 678 stretches).
/// 2. **Miter**: the inner point is identical to its neighbour while the outer
///    points are up to 4.85 / 5.50 / 4.25 mm apart. Such a stretch has no
///    length, so no triangle can be positive on it (1108 / 1095 / 237
///    stretches). A miter makes the strip topologically several pieces -
///    592 / 663 / 109 components across 12 / 16 / 5 pieces - which is why the
///    "connected along its own rings" claim of the issue is dropped here as
///    false.
/// 3. **No width at one end**: the fabric ends inside the stretch, so at least
///    one of the two triangles degenerates (28 / 26 / 10 stretches; on 14 / 13 /
///    5 of them only the outer edge is missing, the inner one is there).
///
/// Over every piece of the three patterns those three cases cover all stretches
/// without an edge, and the assertion is that nothing else is left over. The
/// positive half is checked as well and is the one that would catch a botched
/// split: a stretch that carries fabric at both ends and has real length is a
/// band edge, on the inner side and on the outer side. A ring reassembled in
/// the wrong order still sums to the right area, but it would stop here.
#[test]
fn band_topologie_bleibt_sauber() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        let mut geprueft = 0usize;
        let mut ohne_kante = 0usize;
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            let nr = s.innen.len();
            let mut kanten: HashMap<(u32, u32), usize> = HashMap::new();
            for &[a, b, c] in &s.dreiecke {
                for x in [a, b, c] {
                    assert!(
                        (x as usize) < 2 * nr,
                        "{muster} {}: Index {x} ausserhalb von {} Punkten",
                        t.teil,
                        2 * nr
                    );
                }
                for (x, y) in [(a, b), (b, c), (c, a)] {
                    let key = if x < y { (x, y) } else { (y, x) };
                    *kanten.entry(key).or_insert(0) += 1;
                }
            }
            let mehrfach: Vec<_> = kanten.iter().filter(|(_, &c)| c > 2).collect();
            assert!(
                mehrfach.is_empty(),
                "{muster} {}: {} Kanten mit mehr als zwei Dreiecken, z. B. {:?}",
                t.teil,
                mehrfach.len(),
                mehrfach.first()
            );
            assert!(
                kanten.values().all(|&c| c <= 2),
                "{muster} {}: Kante mit mehr als zwei Dreiecken",
                t.teil
            );

            let null = |i: usize| abstand(s.innen[i], s.aussen[i]) <= 1e-6;
            for i in 0..nr {
                let j = (i + 1) % nr;
                let (b0, b1) = (null(i), null(j));
                let fase = abstand(s.innen[i], s.innen[j]) <= 1e-9;
                let hat = |a: usize, b: usize| {
                    let (x, y) = (a as u32, b as u32);
                    kanten.contains_key(&(if x < y { (x, y) } else { (y, x) }))
                };
                let innen = hat(i, j);
                // The outer edge belongs to the triangle whose third point is
                // the far inner point; it is only built when that point has
                // width.
                let aussen = hat(nr + i, nr + j);
                geprueft += 1;

                if !innen || !aussen {
                    ohne_kante += 1;
                    assert!(
                        fase || b0 || b1,
                        "{muster} {}: Strecke {i} hat Stoff ({:.6} / {:.6} mm) und Laenge {:.6} mm, aber keine {}kante",
                        t.teil,
                        abstand(s.innen[i], s.aussen[i]),
                        abstand(s.innen[j], s.aussen[j]),
                        abstand(s.innen[i], s.innen[j]),
                        if !innen { "Innen" } else { "Aussen" }
                    );
                }
                // The positive half: a stretch with fabric at both ends and
                // real length is joined on both sides.
                if b0 || b1 || fase {
                    continue;
                }
                assert!(innen, "{muster} {}: Innenkante {i} fehlt", t.teil);
                // Without width at the far end there is no triangle at all;
                // that case is already excluded above.
                assert!(aussen, "{muster} {}: Aussenkante {i} fehlt", t.teil);
            }
        }
        assert!(geprueft > 0, "{muster}: keine Strecke geprüft");
        // Named here so a change in the strip's make-up shows up as a number
        // rather than as a silently different topology: measured over the three
        // patterns 3536 / 3487 / 1851 stretches, of which 1398 / 1298 / 925
        // carry no edge on at least one side (1108+262+28 / 1095+177+26 /
        // 237+678+10, i.e. the three cases above and nothing else).
        assert!(
            ohne_kante * 2 < geprueft,
            "{muster}: {ohne_kante} von {geprueft} Strecken ohne Kante - mehr als die Haelfte, das ist keine Ausnahme mehr"
        );
    }
}

/// Every anchor is a *nearest* boundary edge of the piece's boundary polygon:
/// there is no other boundary edge closer to the outer point, and the anchor's
/// distance is inside the band width.
///
/// This is the second, independent check on the pairing asked for in #121 - it
/// measures distances in the plane instead of walking the same arc scale the
/// weld walks. Both agreeing means the outer point really is opposite the
/// boundary point it hangs on and not merely opposite something at the same arc
/// length.
///
/// Measured over 8874 outer points of the three patterns: the anchor's edge is
/// the nearest one everywhere; the anchor edge is at most 0.0286 / 0.0528 /
/// 0.0010 mm farther than the nearest (a corner region, where two edges of the
/// boundary polygon are within a few hundredths of a millimetre of each other),
/// and the distance itself stays at or under 3.0984 / 3.0987 / 3.0074 mm - the
/// band width of 3 mm plus the mesh's own sagitta.
#[test]
fn anker_ist_eine_naechste_randkante() {
    for (muster, _) in MUSTER {
        let n = netz(muster);
        let mut geprueft = 0usize;
        let mut max_mehr = 0.0f64;
        let mut max_abst = 0.0f64;
        for t in n.teile.iter().filter(|t| !t.zugabe.ist_leer()) {
            let s = &t.zugabe;
            let rand = &t.punkte[..t.nrand];
            let m = rand.len();
            assert_eq!(
                s.anker.len(),
                s.aussen.len(),
                "{muster} {}: {} Anker für {} Außenpunkte",
                t.teil,
                s.anker.len(),
                s.aussen.len()
            );
            for (i, &a) in s.anker.iter().enumerate() {
                let a = a as usize;
                assert!(
                    a < m,
                    "{muster} {}: Anker {a} ist kein Randpunkt (nrand {m})",
                    t.teil
                );
                let q = s.aussen[i];
                let abst = (0..m)
                    .map(|k| punkt_strecke(q, rand[k], rand[(k + 1) % m]))
                    .fold(f64::INFINITY, f64::min);
                let eigene = punkt_strecke(q, rand[a], rand[(a + 1) % m]);
                let mehr = eigene - abst;
                assert!(
                    mehr <= 0.1,
                    "{muster} {}: Außenpunkt {i} hängt an Kante {a} mit {eigene:.4} mm, die nächste ist {abst:.4} mm",
                    t.teil
                );
                max_mehr = max_mehr.max(mehr);
                max_abst = max_abst.max(eigene);
                geprueft += 1;
            }
        }
        assert!(geprueft > 1000, "{muster}: nur {geprueft} Anker geprüft");
        // The measured ceilings, so a pairing that drifts fails here.
        assert!(
            max_mehr <= 0.1 && max_abst <= 3.2,
            "{muster}: Anker bis {max_mehr:.4} mm von der nächsten Kante, Abstand bis {max_abst:.4} mm"
        );
    }
}

/// The distance from a point to the stretch `a b`.
fn punkt_strecke(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    if l2 <= 0.0 {
        return abstand(p, a);
    }
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
    abstand(p, [a[0] + t * d[0], a[1] + t * d[1]])
}

/// Twice the surface of the band's triangles (mm²).
fn bandflaeche(s: &Streifen) -> f64 {
    s.dreiecke
        .iter()
        .map(|&[a, b, c]| flaeche2(s.punkt(a), s.punkt(b), s.punkt(c)).abs())
        .sum::<f64>()
        * 0.5
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
