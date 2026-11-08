//! Evidence for fault 2 of the test blouse v26 ("sleeve cap stands off ~3 cm"):
//! where the stand-off would have to come from, and why this pattern has
//! nothing to give.
//!
//! Four facts, from the pattern and from the placement:
//!
//! 1. **The cap carries no ease over the armhole.** `#ArmholeSolved` in
//!    `bluse2_v26.sm2d` drops the armpit until the armhole is exactly as long
//!    as the cap, so the two differ by rounding only (`-0.00 mm` on both cap
//!    seams).
//! 2. **The cap is only half a centimetre taller than measured.** In the
//!    pattern it measures 14.50 cm against the body's 14.0 cm. That surplus did
//!    not end up as extra seam length (fact 1).
//! 3. **The placement does not supply the missing 3 cm either.** After placing,
//!    before any solver step, the fabric under the cap lies 10.7 mm / 11.0 mm
//!    from the skin in the median. Band by band along the cap it is 13.4 mm at
//!    the top against 6.8 mm six to nine centimetres further down - an arch of
//!    6.6 mm, part of which is simply the gap between the arm prism and the
//!    skin. Three centimetres would be more than four times that.
//! 4. **Sewing removes what little is there.** `bluse2_stehend` measures the
//!    same cap at 0.5 mm in this run (0.7 mm on 2026-11-01, the value in the
//!    fault table) after the seam is closed, against 10.7 mm placed. A seam
//!    that pairs two equally long edges pulls the cap edge onto the armhole,
//!    and with no surplus length along the cap there is nothing left to bulge.
//!
//! Conclusion for the fault list: the stand-off of a real garment comes from a
//! cap that is **longer** than its armhole, so the surplus has to be eased in
//! somewhere and lifts the fabric off the shoulder. This pattern is cut without
//! that surplus by construction, so the simulation correctly produces a seated
//! cap. Fault 2 is therefore not something the solver can show with v26 - it is
//! a property of the pattern.
//!
//! Task #111 ran that claim down the other way: it built a **test copy** of the
//! pattern whose cap seams are 15.0 mm longer than their armholes
//! (`bluse2_v26_einhalte.sm2d`), dressed it with
//! [`Simparameter::einhalteweite`](klon3d_sim::Simparameter::einhalteweite) on
//! and measured the air under the cap again (`bluse2_stehend_einhalte.rs`). The
//! cap still lies on the shoulder: 0.9 mm in the median against 10 mm, and the
//! switch only moves the widest single gap (16.2 mm to 20.7 mm) without lifting
//! the median. The reason is in the numbers of that run: sewn, the armhole side
//! is **76.5 mm** longer than its pattern while the cap side takes only 35.1 mm,
//! so the surplus leaves through the armhole instead of lifting the cap. The
//! armhole of the copy is cut 41.5 cm long around a 41 cm body and has slack to
//! give before fabric has to gather; latex, which this blouse is made of,
//! stretches rather than gathering (#109). Fault 2 is therefore not a knob of
//! the solver, and the fault list says "not shown" with this reason instead of
//! reading the 0.7 mm as a solver failure. A cap that stands off needs both:
//! ease along the cap **and** an armhole that does not stretch away under it.
//!
//! Task #115 took the two remaining ways out and closed them. **A shorter
//! armhole does not work:** the cap reads the armhole length through
//! `cutSplinePath`, so cutting it 10.0 mm shorter takes 10.0 mm off the cap as
//! well and leaves the ease at +1.500 cm. **A larger dose does not work either:**
//! `bluse2_v26_kappe40.sm2d` carries 40.0 mm of ease per cap seam, four times
//! #111's, and the cap still lies on the shoulder - 1.3 mm of air in the median
//! against the 10 mm floor, with the armhole side's stretch rising from 18.41 %
//! to 24.90 % while the cap side's falls (8.16 % to 7.19 %). That is
//! `bluse2_stehend_dosis.rs`; the reason is the armhole's 5.3 mm of slack over a
//! 41 cm body, not the solver.
//!
//! Needs the reference body; without it the test prints `übersprungen` and
//! returns. Release only, like the other placement tests.

use klon3d_mesh::{vernetze, Musternetz, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::platzierung::{lies_lage, platziere, Platzierung};
use klon3d_sim::Koerpernetz;

mod band;

const MUSTER: &str = "bluse2_v26";

/// Cap and armhole may differ by rounding only (mm). Both are cut to the same
/// length on purpose; a cap that has to be eased in shows 10-20 mm here.
const MEHRLAENGE_GRENZE_MM: f64 = 1.0;

/// The cap may be at most this much taller than the measured cap height (cm).
/// Anything above would be fabric that has to go over the shoulder.
const HOEHE_MEHR_GRENZE_CM: f64 = 1.0;

/// The stand-off the real garment shows over the shoulder (m), from the fault
/// list: about 3 cm of air under the cap.
const ECHTE_LUFT: f64 = 0.030;

/// After placing, the air under the cap stays below half of [`ECHTE_LUFT`]: the
/// untensioned placement does not supply the stand-off the real garment has. A
/// cap zone that arches the sleeve over the shoulder would cross this and
/// should be looked at, not silently accepted.
const LAGE_LUFT_GRENZE: f64 = 0.5 * ECHTE_LUFT;

/// How much more air the top band of the cap may show than the band six to nine
/// centimetres further down the arm (m). A cap arched over the shoulder puts its
/// fabric up there; 10 mm still leaves room for the shape of the arm, and the
/// 3 cm of the real garment is far above it.
const WOELBUNG_GRENZE: f64 = 0.010;

fn median(v: &[f64]) -> f64 {
    let mut w = v.to_vec();
    w.sort_by(f64::total_cmp);
    w[w.len() / 2]
}

fn groesster(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

/// Index a piece by its name or the name behind the last ` - `.
fn teil_index(netz: &Musternetz, name: &str) -> Option<usize> {
    let treffer: Vec<usize> = (0..netz.teile.len())
        .filter(|&i| netz.teile[i].teil == name || netz.teile[i].teil.rsplit(" - ").next() == Some(name))
        .collect();
    match treffer[..] {
        [i] => Some(i),
        _ => None,
    }
}

/// Local indices of side A of seam `kennung` on piece `ti`.
fn naht_lokal(netz: &Musternetz, kennung: &str, ti: usize) -> Vec<u32> {
    let mut v: Vec<u32> = Vec::new();
    for n in netz.naehte.iter().filter(|n| n.kennung == kennung) {
        for tn in n.teilnaehte.iter().filter(|t| t.teil_a == ti) {
            v.extend(tn.paare.iter().map(|pp| pp.a));
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// Air between the placed cap fabric and the skin (m) and the number of points
/// looked at. Selection as in fault 2 of `bluse2_stehend`: within 5 cm of the
/// cap seam in the pattern, above the highest seam point minus 6 cm.
fn kappenluft(p: &Platzierung, k: &Koerpernetz, ti: usize, kennung: &str) -> (Vec<f64>, usize) {
    let naht = naht_lokal(&p.netz, kennung, ti);
    let naehe: Vec<[f64; 2]> = naht.iter().map(|&i| p.netz.teile[ti].punkte[i as usize]).collect();
    let oben = naht
        .iter()
        .map(|&i| p.lage(ti, p.netz.teile[ti].punkte[i as usize])[2])
        .fold(f64::NEG_INFINITY, f64::max)
        - 0.06;
    let mut anzahl = 0usize;
    let mut luft: Vec<f64> = Vec::new();
    for q in p.netz.teile[ti].punkte.iter() {
        let dicht = naehe.iter().any(|n| ((n[0] - q[0]).powi(2) + (n[1] - q[1]).powi(2)).sqrt() < 50.0);
        if !dicht {
            continue;
        }
        let pos = p.lage(ti, *q);
        if pos[2] > oben {
            anzahl += 1;
            luft.push(k.naechster_alle(pos).abstand);
        }
    }
    (luft, anzahl)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn kugel_hat_keine_einhalteweite() {
    let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster"));
    let pfad = dir.join(format!("{MUSTER}.sm2d"));
    let naehte = dir.join(format!("{MUSTER}.naehte"));
    let lage = dir.join(format!("{MUSTER}.lage"));
    let smis_pfad = dir.join("measurements.smis");
    for p in [&pfad, &naehte, &lage, &smis_pfad] {
        if !p.exists() {
            eprintln!("übersprungen: {} fehlt", p.display());
            return;
        }
    }
    let Some((k, h)) = band::pruefkoerper() else { return };

    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &smis_pfad).unwrap();
    let defs = lies_nahtliste(&std::fs::read_to_string(&naehte).unwrap()).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).unwrap();
    let liste = lies_lage(&std::fs::read_to_string(&lage).unwrap()).unwrap();
    let p = platziere(&netz, &liste, &h).unwrap();

    // --- 1. Cap length against armhole length -------------------------------
    // V13 / V14 are the two cap seams: side A is the sleeve, side B the armhole.
    for kennung in ["V13", "V14"] {
        let n = p
            .netz
            .naehte
            .iter()
            .find(|n| n.kennung == kennung)
            .unwrap_or_else(|| panic!("Naht {kennung} fehlt"));
        let mehr = n.laenge_a_mm - n.laenge_b_mm;
        eprintln!(
            "  [1] {kennung}: Kugel {:.2} cm, Armloch {:.2} cm, Unterschied {mehr:+.2} mm",
            n.laenge_a_mm / 10.0,
            n.laenge_b_mm / 10.0
        );
        assert!(
            mehr.abs() < MEHRLAENGE_GRENZE_MM,
            "{kennung}: Kugel ist {mehr:+.2} mm länger als das Armloch, erwartet höchstens \
             {MEHRLAENGE_GRENZE_MM} mm - diese Bluse ist ohne Einhalteweite geschnitten"
        );
    }

    // --- 2. Cap height in the pattern against the measured one --------------
    let gemessen_cm = klon3d_pattern::lies_smis(&smis_pfad)
        .unwrap()
        .wert("arm_shoulder_tip_to_armfold_line")
        .expect("arm_shoulder_tip_to_armfold_line fehlt");
    for teil in ["Aermel Kurz Links", "Aermel Kurz Rechts"] {
        let ti = teil_index(&p.netz, teil).unwrap();
        // Höchster Punkt des Ärmelteils gegen den tiefsten Punkt seiner
        // Kugelnaht (Seite A von V13/V14) - das ist die Kugelhöhe im Zuschnitt.
        let oben = p.netz.teile[ti].punkte.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min);
        let achsel = ["V13", "V14"]
            .iter()
            .flat_map(|&n| naht_lokal(&p.netz, n, ti))
            .map(|i| p.netz.teile[ti].punkte[i as usize][1])
            .fold(f64::NEG_INFINITY, f64::max);
        let hoehe_cm = (achsel - oben) / 10.0;
        eprintln!(
            "  [2] {teil}: Kugelhöhe im Zuschnitt {hoehe_cm:.2} cm, gemessen {gemessen_cm:.1} cm, \
             mehr {:+.2} cm",
            hoehe_cm - gemessen_cm
        );
        assert!(
            hoehe_cm - gemessen_cm < HOEHE_MEHR_GRENZE_CM,
            "{teil}: Kugelhöhe {hoehe_cm:.2} cm gegen gemessen {gemessen_cm:.1} cm - mehr als \
             {HOEHE_MEHR_GRENZE_CM} cm über dem Maß wäre Stoff, der über die Schulter muss"
        );
    }

    // --- 3. Air under the cap after placing, before any solver step ---------
    // The placement is what the solver starts from. If that already supplies
    // 3 cm, the fault would be a solver problem; it does not.
    for (kennung, teil) in [("V13", "Aermel Kurz Links"), ("V14", "Aermel Kurz Rechts")] {
        let ti = teil_index(&p.netz, teil).unwrap();
        let (luft, anzahl) = kappenluft(&p, &k, ti, kennung);
        assert!(anzahl > 40, "{teil}: nur {anzahl} Punkte über der Schulter");
        let med = median(&luft);
        eprintln!(
            "  [3] {teil}: {anzahl} Punkte, Luft zur Haut nach dem Platzieren im Median \
             {:.1} mm, höchstens {:.1} mm",
            med * 1e3,
            groesster(&luft) * 1e3
        );
        assert!(
            med < LAGE_LUFT_GRENZE,
            "{teil}: Kugel liegt nach dem Platzieren {:.1} mm über der Haut (Grenze {:.0} mm) - \
             die Platzierung liefert die Wölbung, nicht das Muster; bitte PLAN.md Fehler 2 prüfen",
            med * 1e3,
            LAGE_LUFT_GRENZE * 1e3
        );
    }

    // --- 4. Is the air a dome or just the gap around the arm? ---------------
    // Pattern y runs along the arm axis, so bands of pattern y are rings around
    // the arm. A cap that arches over the shoulder would put more air at the
    // top band (near the shoulder) than on the upper arm below it. Equal air in
    // both means the placement lays the sleeve onto the arm prism and the
    // stand-off is the prism gap, not fullness.
    let ti = teil_index(&p.netz, "Aermel Kurz Links").unwrap();
    let gipfel = p.netz.teile[ti].punkte.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min);
    let mut baender: Vec<(String, Vec<f64>)> = (0..4)
        .map(|b| (format!("{:2}-{:2} cm unter dem Gipfel", b * 3, b * 3 + 3), Vec::new()))
        .collect();
    for q in p.netz.teile[ti].punkte.iter() {
        let tiefe_cm = (q[1] - gipfel) / 10.0;
        let b = (tiefe_cm / 3.0).floor();
        if !(0.0..4.0).contains(&b) {
            continue;
        }
        let pos = p.lage(ti, *q);
        baender[b as usize].1.push(k.naechster_alle(pos).abstand);
    }
    let mut band_med: Vec<f64> = Vec::new();
    for (name, v) in &baender {
        if v.is_empty() {
            continue;
        }
        let m = median(v);
        eprintln!("  [4] {name}: {} Punkte, Luft im Median {:.1} mm", v.len(), m * 1e3);
        band_med.push(m);
    }
    assert!(band_med.len() >= 3, "Kugelbänder: nur {} Bänder belegt", band_med.len());
    let oben = band_med[0];
    let unten = band_med[2];
    eprintln!(
        "  [4] Gipfelband {:.1} mm gegen das Band 6-9 cm tiefer {:.1} mm ({:+.1} mm)",
        oben * 1e3,
        unten * 1e3,
        (oben - unten) * 1e3
    );
    assert!(
        oben - unten < WOELBUNG_GRENZE,
        "über der Schulter liegen {:.1} mm mehr Luft als am Oberarm darunter (Grenze {:.0} mm) - \
         die Platzierung wölbt die Kugel, das gehört dann in PLAN.md Fehler 2 nachgezogen",
        (oben - unten) * 1e3,
        WOELBUNG_GRENZE * 1e3
    );
}
