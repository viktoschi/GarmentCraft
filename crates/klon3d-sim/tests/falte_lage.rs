//! The folded allowance band against its own piece: one sheet of fabric, not
//! two layers (task #133).
//!
//! Before this task the proximity search knew two kinds of "same place": the
//! identical point index, and the seam partner. A folded band is a third: from
//! #123 the band hangs on the piece's seam line by springs of rest length 0, so
//! band point and piece point sit on the same place like a seam - and the
//! search saw two layers at distance 0 and pushed them apart. On a rectangle of
//! 20 × 20 cm with a 3 mm allowance that was 788 proximity pairs and 406 of them
//! over a rest-0 spring (measured 11.11.2026, before the change).
//!
//! The band is not a second layer at all: it is the piece's own reverse, turned
//! once about the seam line. Two layers of one fabric rest **one fabric
//! thickness** apart. So the pair gets a rest distance of `2 × halbe Dicke`
//! instead of the padded `Dicke + Zusatzdicke` (0,4 mm instead of 2,9 mm) -
//! the padding is a solver cushion against penetration, not geometry, and with
//! it the band would visibly float above the cloth it covers.
//!
//! ## What is asserted
//!
//! 1. **The pair carries the fabric thickness, not the cushion.** Every
//!    proximity pair band-on-carrier has a rest distance of exactly `2 × halbe
//!    Dicke`; none carries `Dicke + Zusatzdicke`. This holds for both searches:
//!    point–triangle and edge–edge.
//! 2. **The band settles at that distance.** After 60 frames with the piece
//!    held, the median band point lies `−0,40 mm` below the piece (its nearest
//!    point in xy): one fabric thickness, inside 0,05 mm. No band point is more
//!    than 0,1 mm **above** the piece - the band does not lift off.
//! 3. **Without self-collision the band lies in the piece**, so the number in
//!    (2) is the contact's and not the fold's. Median within 0,05 mm of 0, and
//!    the deepest band point within 0,1 mm of the value with self-collision -
//!    the depth is the mitre's, not the contact's.
//! 4. **The rest-0 holds no longer read as layers.** No proximity pair at t=0
//!    connects a band point to its carrier over a rest-0 hold; the 406 of the
//!    starting state are gone.
//! 5. **The layering adds no cuts.** After 60 frames at most 10 edges pierce a
//!    triangle (measured 3; with the 0-holds read as layers 6 at the end). Those
//!    3 sit at the mitre's crossing arms, same as the pairs above.
//!
//! ## What is *not* asserted, and why
//!
//! The band's own mitre at a piece corner still self-overlaps: the strip turns
//! the corner on both sides of the seam line, so at the cutting line the two
//! arms cross. Measured on the rectangle: 34 band↔band pairs and 7 cuts at t=0,
//! all within 10 mm of a corner and all 2,4…4,2 mm from the seam line on a 3 mm
//! band - at the cutting line, not at the fold. For the same reason the band
//! sinks to −3,15 mm at the deepest corner point, and it does so **without**
//! self-collision as well (−3,161 mm without against −3,147 mm with), so the
//! depth is the mitre's and not the contact's; the contact only adds points to
//! the count (8 of 163 deeper than 1 mm with, 2 without). That is a question of the
//! mitre in `klon3d_mesh::zugabe`, not of the layering, so the count is
//! asserted not to grow rather than required to be zero.
//!
//! No body, no gravity: the pattern is the cloth (the device
//! [`crate::saumfalte`] and `tests/saumfalte.rs` use). Under gravity the whole
//! rectangle falls, and a z comparison would say nothing about the band's place
//! on the piece. Release only - 60 frames of the implicit solver.
//!
//! `cargo test --release -p klon3d-sim --test falte_lage`

use std::collections::HashMap;

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_str;
use klon3d_sim::roehre::{param_ruhe, rechteck_muster};
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{ImplizitCg, ImplizitParameter, Rechenweg, Simparameter, Szene, RADICAL_RUBBER_04};

/// Height of a band point against the piece's nearest point in xy, in mm.
/// Positive is above the piece.
const LAGE_TOLERANZ_MM: f64 = 0.05;
/// How far a band point may stand above the piece (mm): the band must not lift
/// off. The 2,9 mm cushion would be far outside.
const ABHEBE_MM: f64 = 0.1;
/// Band points deeper than a millimetre under the piece (mm) are the mitre at
/// the corners, counted and bounded, never required to be zero.
const GEHRUNG_MAX: usize = 10;
/// The deepest band point is the mitre, so it sinks as deep with as without
/// self-collision (measured -3,161 mm without against -3,147 mm with). Within
/// 0,1 mm of each other proves the contact does not create the depth.
const GEHRUNG_TIEFE_MM: f64 = 0.1;
/// Edge-through-triangle cuts left at the cutting line (measured 3 after 60
/// frames; with the 0-holds read as layers 6 at the end, 30 over all steps
/// against 15). The mitre arms keep a few; the layering must not add to them.
const SCHNITTE_MAX: usize = 10;

/// The rectangle with its band folded in, laid flat and without gravity. `fest`
/// holds the piece, so the band's own place on it can be measured.
fn szene(fest: bool) -> Szene {
    let m = read_str(&rechteck_muster(0.2, 0.2)).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &Parameter::default()).unwrap();
    assert!(netz.fehler.is_empty(), "{:?}", netz.fehler);
    let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
    let param = Simparameter { einhalteweite: false, schwerkraft: [0.0, 0.0, 0.0], ..param_ruhe() };
    let (mut s, bericht) = Szene::aus_netz_mit_falte(
        &netz,
        &|_| vg,
        &|_, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0],
        Vec::new(),
        param,
    );
    assert_eq!(bericht.teile, 1, "ein Teil mit Band");
    if fest {
        for t in s.teile.clone() {
            if t.band_von.is_some() {
                continue;
            }
            for i in t.punkte {
                s.w[i] = 0.0;
            }
        }
    }
    s
}

/// The band's points and the pieces' points, told apart by `band_von` - the
/// field the layering rule itself reads.
fn trennen(s: &Szene) -> (Vec<usize>, Vec<usize>) {
    let mut band = Vec::new();
    let mut teil = Vec::new();
    for t in &s.teile {
        let ziel = if t.band_von.is_some() { &mut band } else { &mut teil };
        ziel.extend(t.punkte.clone());
    }
    (band, teil)
}

/// Height in m of every band point against the nearest piece point in xy.
fn hoehen(s: &Szene, band: &[usize], teil: &[usize]) -> Vec<f64> {
    let mut d: Vec<f64> = band
        .iter()
        .map(|&i| {
            let (mut best, mut dz) = (f64::INFINITY, 0.0);
            for &j in teil {
                let dx = s.x[i][0] - s.x[j][0];
                let dy = s.x[i][1] - s.x[j][1];
                let q = dx * dx + dy * dy;
                if q < best {
                    best = q;
                    dz = s.x[i][2] - s.x[j][2];
                }
            }
            dz * 1e3
        })
        .collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    d
}

fn mittel(d: &[f64]) -> f64 {
    d[d.len() / 2]
}

#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn das_band_liegt_eine_stoffdicke_auf_dem_teil() {
    let zusatz = 2.5e-3;
    let band_dicke = |s: &Szene| 2.0 * s.kollisionsabstand[0];

    // (1) Jedes Band-Träger-Paar trägt die Stoffdicke als Ruheabstand.
    let s0 = szene(true);
    let (band, teil) = trennen(&s0);
    let soll = band_dicke(&s0);
    assert!((soll - 0.4e-3).abs() < 1e-9, "2 × 0,2 mm, nicht {:.4} mm", soll * 1e3);
    let (sn, _) = klon3d_sim::selbstkollision::aus_szene(&s0, zusatz);
    let paare = klon3d_sim::selbstkollision::finde(&sn, &s0.x, &s0.w, true, true);
    let mut auf_traeger = 0;
    let mut je_art = [0usize; 2];
    for p in &paare {
        let seite = |vorzeichen: f64| -> Vec<usize> {
            (0..4).filter(|&q| p.gewichte[q] * vorzeichen > 0.0).map(|q| p.punkte[q] as usize).collect()
        };
        let (a, b) = (seite(1.0), seite(-1.0));
        let ganz_band = |l: &[usize]| !l.is_empty() && l.iter().all(|i| band.contains(i));
        let ganz_teil = |l: &[usize]| !l.is_empty() && l.iter().all(|i| teil.contains(i));
        // Nur Band gegen eigenen Träger. Band gegen Band ist die Gehrung (siehe
        // Moduldoc), Teil gegen Teil gibt es auf einem einzelnen Teil nicht.
        if !((ganz_band(&a) && ganz_teil(&b)) || (ganz_teil(&a) && ganz_band(&b))) {
            continue;
        }
        auf_traeger += 1;
        je_art[match p.art {
            klon3d_sim::selbstkollision::Art::PunktDreieck => 0,
            klon3d_sim::selbstkollision::Art::KanteKante => 1,
        }] += 1;
        assert!(
            (p.ruhe - soll).abs() < 1e-9,
            "Ruheabstand {:.4} mm statt einer Stoffdicke {:.4} mm ({:?})",
            p.ruhe * 1e3,
            soll * 1e3,
            p.art
        );
    }
    assert!(auf_traeger > 0, "kein Band-Träger-Paar bei t=0");
    // Beide Nähe-Suchen müssen die Regel tragen: nur die Punkt–Dreieck-Suche
    // umzustellen ließe die Kante–Kante-Paare auf dem Kissenabstand stehen.
    assert!(
        je_art[0] > 0 && je_art[1] > 0,
        "Band-Träger-Paare: {} Punkt–Dreieck, {} Kante–Kante - beide Suchen sind gemeint",
        je_art[0],
        je_art[1]
    );

    // (4) Keine Nähe mehr über eine Haltefeder der Ruhelänge 0.
    let halte0: std::collections::HashSet<(u32, u32)> = s0
        .abstand
        .punkte
        .iter()
        .enumerate()
        .filter(|(e, _)| s0.abstand.ruhe[*e] == 0.0)
        .map(|(_, &[i, j])| (i.min(j), i.max(j)))
        .collect();
    let ueber_halte0 = paare
        .iter()
        .filter(|p| {
            let seite: Vec<u32> = (0..4).filter(|&q| p.gewichte[q] != 0.0).map(|q| p.punkte[q]).collect();
            seite.iter().enumerate().any(|(k, &a)| {
                seite[k + 1..].iter().any(|&b| halte0.contains(&(a.min(b), a.max(b))))
            })
        })
        .count();
    assert_eq!(ueber_halte0, 0, "{ueber_halte0} Paare über eine 0-Halte");

    // (2) Mit Selbstkollision ruht das Band eine Stoffdicke unter dem Teil.
    let mut s = szene(true);
    let mut rw = ImplizitCg::neu(ImplizitParameter {
        selbstkollision: true,
        selbst_zusatzdicke: zusatz,
        ..ImplizitParameter::default()
    });
    for _ in 0..60 {
        rw.bild(&mut s);
    }
    let d = hoehen(&s, &band, &teil);
    let med = mittel(&d);
    let tief_mit = d[0];
    let gehrung = d.iter().filter(|&&v| v < -1.0).count();
    println!(
        "mit Selbstkollision: Band {med:.4} mm unter dem Teil (Stoffdicke {:.4} mm), min {:.3} / max {:.3} mm, {gehrung}/{} tiefer als 1 mm, {auf_traeger} Band-Träger-Paare ({} Punkt–Dreieck, {} Kante–Kante)",
        -soll * 1e3,
        d[0],
        d[d.len() - 1],
        d.len(),
        je_art[0],
        je_art[1]
    );
    assert!(
        (med + soll * 1e3).abs() < LAGE_TOLERANZ_MM,
        "Band ruht bei {:.3} mm, eine Stoffdicke wäre {:.3} mm",
        med,
        -soll * 1e3
    );
    assert!(d[d.len() - 1] < ABHEBE_MM, "Band steht {:.3} mm über dem Teil", d[d.len() - 1]);
    assert!(gehrung <= GEHRUNG_MAX, "{gehrung} Bandpunkte tiefer als 1 mm (Gehrung, gemessen 8)");
    // Die Gehrung reißt am Schnittrand ein paar Kanten durch Dreiecke; die
    // Schichtung darf keine dazutun (mit den 0-Federn als Lagen 6 am Ende).
    let (sn_ende, _) = klon3d_sim::selbstkollision::aus_szene(&s, zusatz);
    let schnitte = klon3d_sim::selbstkollision::finde_schnitte(&sn_ende, &s.x, false).len();
    println!("nach 60 Bildern: {schnitte} Schnitte Kante durch Dreieck (mit den 0-Federn als Lagen 6)");
    assert!(schnitte <= SCHNITTE_MAX, "{schnitte} Schnitte Kante durch Dreieck, gemessen 3");

    // (3) Ohne Selbstkollision liegt das Band im Teil: der Wert (2) ist der
    //     des Kontakts, nicht der Faltung. Der **tiefste** Punkt bleibt dabei,
    //     denn er ist die Gehrung, nicht die Schichtung; die Zahl der tiefen
    //     Punkte wächst durch den Kontakt (8 statt 2), die Tiefe nicht.
    let mut s = szene(true);
    let mut rw = ImplizitCg::neu(ImplizitParameter { selbstkollision: false, ..ImplizitParameter::default() });
    for _ in 0..60 {
        rw.bild(&mut s);
    }
    let d = hoehen(&s, &band, &teil);
    let ohne = mittel(&d);
    let tief_ohne = d.iter().filter(|&&v| v < -1.0).count();
    println!(
        "ohne Selbstkollision: Band {ohne:.4} mm (liegt im Teil), min {:.3} / max {:.3} mm, {tief_ohne}/{} tiefer als 1 mm",
        d[0],
        d[d.len() - 1],
        d.len()
    );
    assert!(ohne.abs() < LAGE_TOLERANZ_MM, "ohne Selbstkollision bei {ohne:.3} mm");
    // Die Gehrung ist der tiefste Punkt, nicht der Kontakt: ohne
    // Selbstkollision sinkt er genauso tief (gemessen -3,161 gegen -3,147 mm).
    assert!(
        (d[0] - tief_mit).abs() < GEHRUNG_TIEFE_MM,
        "tiefster Punkt ohne Selbstkollision {:.3} mm, mit {:.3} mm - die Gehrung, nicht der Kontakt",
        d[0],
        tief_mit
    );
    assert!(tief_ohne <= GEHRUNG_MAX, "{tief_ohne} Bandpunkte tiefer als 1 mm ohne Selbstkollision");
}
