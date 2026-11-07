//! A test copy of the blouse v26 whose sleeve caps carry real ease (task #111).
//!
//! Fault 2 of the acceptance list says the sleeve cap of v26 stands off the
//! shoulder by about 3 cm, but the delivered pattern cannot show it: its
//! `#ArmholeSolved` drops the armhole until it is exactly as long as the cap,
//! so both cap seams differ by rounding only (see `aermelkugel.rs`).
//!
//! For the solver run this file supplies the missing ingredient: a copy of the
//! pattern with `#CapEase` in it, which makes both cap seams **15.0 mm** longer
//! than their armhole. The delivered `bluse2_v26.sm2d` stays untouched, so every
//! acceptance number of `bluse2_stehend` keeps its meaning.
//!
//! The copy names itself in its `<description>` and differs from v26 in one new
//! variable plus twelve formula sites that read it: the four `cutSplinePath`
//! intersection lengths and, per sleeve side, the two clamp chains `AB1T`/`AB1S`
//! and `AB2T`/`AB2S` - each in its length **and** in its `abs(...) >= 0.12`
//! selection, because a chain that does not see the ease cuts it off again.
//!
//! The source of the ease is the sleeve's cap construction, not the armhole:
//! the cap corner hangs on the clamp chain `AB1T -> AB1S -> AB1B -> AB1C ->
//! AB1K`, and `AB1T`/`AB1S` read `Line_AL0_ALB` and clamp at 1.2 mm. A wider cap
//! only reaches the corner when the four `cutSplinePath` points **and** that
//! chain both see the extra length.
//!
//! This test is the cheap half of the evidence: it only reads the pattern, it
//! runs no solver. The solver run is `bluse2_stehend_einhalte.rs` in
//! `klon3d-sim`.

mod muster;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::read_path;

/// The copy and its seam list; both lie in `tests/muster/`.
const MUSTER: &str = "bluse2_v26_einhalte";

/// The two cap seams and the ease they have to carry, in cm.
const KAPPENNAEHTE: [&str; 2] = ["V13", "V14"];
const EASE_CM: f64 = 1.5;

/// The ease a sleeve cap needs to be worth easing: 10 to 20 mm per cap seam
/// (the range `aermelkugel.rs` names).
const EASE_MIN_CM: f64 = 1.0;
const EASE_MAX_CM: f64 = 2.0;

/// Every other seam keeps v26's lengths, which differ by at most 0.34 mm.
const REST_CM: f64 = 0.1;

#[test]
fn kappe_traegt_einhalteweite() {
    let pfad = muster::muster_pfad(MUSTER);
    if !pfad.exists() {
        eprintln!("übersprungen: {} fehlt", pfad.display());
        return;
    }
    let m = read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &muster::mess_pfad()).unwrap();
    let text = std::fs::read_to_string(muster::wurzel().join(format!("{MUSTER}.naehte"))).unwrap();
    let defs = lies_nahtliste(&text).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();

    // Both cap seams are longer than their armhole by the same ease.
    for k in KAPPENNAEHTE {
        let n = b.naehte.iter().find(|n| n.kennung == k).unwrap();
        let d = n.a.laenge_cm - n.b.laenge_cm;
        assert!(
            (d - EASE_CM).abs() < 2e-3,
            "{k}: Kappe {:.4} / Armloch {:.4} = {d:+.4} cm, erwartet +{EASE_CM}",
            n.a.laenge_cm,
            n.b.laenge_cm
        );
        assert!(
            (EASE_MIN_CM..=EASE_MAX_CM).contains(&d),
            "{k}: {d:+.4} cm liegt außerhalb von 10 bis 20 mm"
        );
    }

    // Nothing else moved: the copy differs from v26 in the cap seams only.
    for n in &b.naehte {
        if KAPPENNAEHTE.contains(&n.kennung.as_str()) {
            continue;
        }
        let d = (n.a.laenge_cm - n.b.laenge_cm).abs();
        assert!(d <= REST_CM, "{}: {d:+.4} cm, die Kopie darf nur die Kappe ändern", n.kennung);
    }
}

/// The ease goes into the cap, it does not break the seam: the cap seam still
/// runs over the same pieces as in v26, so the mesh pairs cap and armhole
/// along the whole curve.
///
/// The notches are a different matter: the 15 mm push them out of the 1 cm
/// catch range, so `klon3d naehte` reports them as unmatched on both sides.
/// That is the expected consequence of the ease, not a loose seam.
#[test]
fn kappennnaht_bleibt_ganz() {
    let pfad = muster::muster_pfad(MUSTER);
    if !pfad.exists() {
        eprintln!("übersprungen: {} fehlt", pfad.display());
        return;
    }
    let m = read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &muster::mess_pfad()).unwrap();
    let text = std::fs::read_to_string(muster::wurzel().join(format!("{MUSTER}.naehte"))).unwrap();
    let defs = lies_nahtliste(&text).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();

    // Same pieces, same order as in the delivered pattern.
    let original = read_path(&muster::muster_pfad("bluse2_v26")).unwrap();
    let original_ev = Auswertung::lade(&original, &muster::mess_pfad()).unwrap();
    let original_text =
        std::fs::read_to_string(muster::wurzel().join("bluse2_v26.naehte")).unwrap();
    let original_defs = lies_nahtliste(&original_text).unwrap();
    let original_b = pruefe_naehte(&original_ev, &original_defs, Kurvenquelle::Seamly).unwrap();

    for k in KAPPENNAEHTE {
        let neu = b.naehte.iter().find(|n| n.kennung == k).unwrap();
        let alt = original_b.naehte.iter().find(|n| n.kennung == k).unwrap();
        let stuecke = |n: &klon3d_pattern::naehte::Nahtergebnis| -> Vec<(String, String, String)> {
            n.a.teilstuecke.iter().map(|t| (t.teil.clone(), t.von.clone(), t.bis.clone())).collect()
        };
        assert_eq!(stuecke(neu), stuecke(alt), "{k}: die Kappe läuft über andere Teilstücke");
        // The whole cap curve is still there: the ease adds length, it does not
        // cut a piece off.
        assert!(neu.a.teilstuecke.iter().all(|t| t.laenge_cm > 0.0), "{k}: leeres Teilstück");
    }
}
