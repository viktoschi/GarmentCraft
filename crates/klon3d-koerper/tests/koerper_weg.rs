//! The entry point of `klon3d.exe`: body from `.smis`, chain or fallback.
//!
//! Mesh and data: CLO's resizing folder (`CLO_FEMALE` selects another one; the
//! default is the reference folder). Without it the CLO paths are skipped.
//!
//! Four paths are checked: the reference chain (default folder), the older chain
//! (only when the older folder exists – backup), the old `anpassen` path from a
//! folder holding only the older `Advanced` files, and the MakeHuman path, which
//! needs no CLO file at all and therefore always runs.

use std::path::PathBuf;

use klon3d_koerper::kette::KOERPER;
use klon3d_koerper::koerper::{aus_smis, makehuman_weg, ordner, Weg};

/// CLO's older resizing folder of the default installation.
const ORDNER_ALT: &str = r"C:\Program Files\CLO Standalone\Preset\Avatar\Resizing\Female";

fn daten() -> Option<(PathBuf, klon3d_pattern::Smis)> {
    let o = ordner();
    if !o.join("BaseMesh.obj").is_file() {
        eprintln!("übersprungen: CLO-Resizing-Daten fehlen in {}", o.display());
        return None;
    }
    let pfad = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../klon3d-pattern/tests/muster/measurements.smis"
    ));
    let Ok(smis) = klon3d_pattern::lies_smis(&pfad) else {
        eprintln!("übersprungen: {} fehlt", pfad.display());
        return None;
    };
    Some((o, smis))
}

fn pruefe_netz(k: &klon3d_koerper::koerper::Koerper) {
    assert_eq!(k.netz.punkte.len(), KOERPER);
    assert!(!k.netz.dreiecke.is_empty());
    assert!(k.netz.dreiecke.iter().all(|d| d.iter().all(|&i| i < KOERPER)));
    // mm: rund 160 cm hoch.
    let (lo, hi) = k.netz.hoehe().unwrap();
    assert!((1550.0..1650.0).contains(&(hi - lo)), "Höhe {} mm", hi - lo);
}

#[test]
fn kette_formt_den_koerper() {
    let Some((o, smis)) = daten() else { return };
    let k = aus_smis(&o, &|n| smis.wert(n)).expect("Körper");
    assert_eq!(k.weg, Weg::Kette, "{:?}", k.meldungen);
    pruefe_netz(&k);
    let e = k.kette.as_ref().expect("Werte der Kette");
    for (j, name) in [(2, "bust_circ"), (6, "waist_circ"), (8, "hip_circ")] {
        assert!(
            (e.werte[j] - smis.wert(name).unwrap()).abs() < 1.0,
            "{name}: {}",
            e.werte[j]
        );
    }
}

/// No reference data, but older data: the older chain shapes (backup).
#[test]
fn sicherung_alt_formt_den_koerper() {
    let Some((_, smis)) = daten() else { return };
    let alt = PathBuf::from(ORDNER_ALT);
    if !alt.join("ConvertingMat_DETAIL_Human_Bust_TotalHeight.bd").is_file() {
        eprintln!("übersprungen: ältere Daten fehlen in {}", alt.display());
        return;
    }
    let k = aus_smis(&alt, &|n| smis.wert(n)).expect("Körper");
    assert_eq!(k.weg, Weg::KetteAlt, "{:?}", k.meldungen);
    assert!(
        k.meldungen[0].starts_with("Referenzkette nicht ladbar"),
        "{:?}",
        k.meldungen
    );
    pruefe_netz(&k);
    assert!(k.kette.is_some());
}

/// Only the files of the old path: neither chain is loadable, `anpassen` shapes.
#[test]
fn fallback_ohne_kettendaten() {
    let Some((_, smis)) = daten() else { return };
    let alt_ordner = PathBuf::from(ORDNER_ALT);
    let alt = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("koerper_nur_alt");
    std::fs::create_dir_all(&alt).expect("Ordner");
    for f in [
        "BaseMesh.obj",
        "ConvertingMat_DETAIL_Advanced_TotalHeight.bd",
        "_DetailFeature_Advanced_Bust_TotalHeight.txt",
    ] {
        std::fs::copy(alt_ordner.join(f), alt.join(f)).expect(f);
    }
    let k = aus_smis(&alt, &|n| smis.wert(n)).expect("Körper");
    assert_eq!(k.weg, Weg::Anpassen);
    assert!(
        k.meldungen[0].starts_with("Referenzkette nicht ladbar"),
        "{:?}",
        k.meldungen
    );
    assert!(k.kette.is_none());
    pruefe_netz(&k);
}

/// The MakeHuman path needs no CLO file: it runs everywhere, on the shipped
/// CC0 mesh. It must hold the four shape lines of the `.smis` file.
#[test]
fn makehuman_weg_ohne_clo_daten() {
    let pfad = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../klon3d-pattern/tests/muster/hexen_2026_10_27/measurements.smis"
    ));
    let Ok(smis) = klon3d_pattern::lies_smis(&pfad) else {
        eprintln!("übersprungen: {} fehlt", pfad.display());
        return;
    };
    let Some(o) = klon3d_koerper::koerper_quelle::ordner() else {
        eprintln!("übersprungen: MakeHuman-Assets fehlen");
        return;
    };

    let k = makehuman_weg(&o, &|n| smis.wert(n)).expect("MakeHuman-Körper");
    assert_eq!(k.weg, Weg::Makehuman, "{:?}", k.meldungen);
    assert!(k.kette.is_none());

    // 13 380 Punkte der Gruppe `body` plus die Ringe und Ecken, die `formen`
    // einfügt - nach oben offen, aber nicht beliebig gross.
    let n = k.netz.punkte.len();
    assert!((13_000..40_000).contains(&n), "{n} Punkte");
    assert!(!k.netz.dreiecke.is_empty());
    assert!(k.netz.dreiecke.iter().all(|d| d.iter().all(|&i| i < n)));
    let (lo, hi) = k.netz.hoehe().unwrap();
    assert!((1550.0..1650.0).contains(&(hi - lo)), "Höhe {} mm", hi - lo);

    // Die Formlinien selbst prüft `abnahme_formziele.rs` gegen alle sechs
    // Werte; hier zählt, dass der Weg sie überhaupt geformt hat und dass die
    // Meldungen jede Linie nennen.
    for name in ["Brust", "Taille", "HighHip", "Hüfte"] {
        assert!(
            k.meldungen.iter().any(|m| m.starts_with(name)),
            "{name} fehlt in {:?}",
            k.meldungen
        );
    }
}
