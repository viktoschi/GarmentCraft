//! A test copy of the blouse v26 whose sleeve caps carry 40.0 mm of ease (#115).
//!
//! Task #111 built `bluse2_v26_einhalte.sm2d`, a copy whose cap seams are
//! 15.0 mm longer than their armholes, and dressed it: the cap still lay on the
//! shoulder (`0.9 mm` of air against a 10 mm floor). The recorded reason is the
//! armhole, cut 41.5 cm long around a 41 cm body, which gives the surplus
//! somewhere to leave.
//!
//! Cutting the armhole shorter does not take that slack away: the cap reads the
//! armhole length through `cutSplinePath` over `Line_AL0_ALB` / `Line_AL0_ALF`,
//! so both sides lose the same millimetres and the ease stays exactly where it
//! was (measured: cap and armhole both -10.0 mm, difference unchanged at
//! +1.500 cm). What is open is the **dose** - 15 mm of ease gave 0.9 mm of air,
//! so how much ease does a 10 mm stand-off need?
//!
//! This file supplies the second point of that curve: `bluse2_v26_kappe40.sm2d`
//! is a copy of the #111 pattern in which `#CapEase` is `2` instead of `0.75`,
//! so both cap seams are **40.0 mm** longer than their armholes. One formula
//! differs; no node id moves, so the seam list and the placement of the #111
//! copy apply unchanged. The delivered `bluse2_v26.sm2d` and the #111 copy are
//! only read, so every number they carry keeps its meaning.
//!
//! The file reads the copy without a solver: that the dose really is 40 mm and
//! that the other seams did not move. The dressing run is
//! `bluse2_stehend_dosis.rs` in `klon3d-sim`, and its answer is that 40 mm still
//! do not lift the cap (1.3 mm of air against the 10 mm floor).

use std::path::PathBuf;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte, Nahtergebnis};

/// The dose this copy carries per cap seam, in mm (2 x `#CapEase`).
const DOSIS_MM: f64 = 40.0;
/// The cap seams; side A is the sleeve, side B the armhole.
const KAPPENNAEHTE: [&str; 2] = ["V13", "V14"];
/// Seams that must keep v26's difference of their two sides (cm).
const UNVERAENDERT: [(&str, f64); 4] = [
    ("V05", -0.0342),
    ("V06", -0.0342),
    ("V07", 0.0005),
    ("V15", 0.0),
];

fn muster(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/muster")).join(format!("{name}.sm2d"))
}

/// Checked seams of a pattern, or `None` when a file is missing.
fn lies(name: &str) -> Option<Vec<Nahtergebnis>> {
    let pfad = muster(name);
    let liste = pfad.with_extension("naehte");
    for p in [&pfad, &liste] {
        if !p.exists() {
            eprintln!("übersprungen: {} fehlt", p.display());
            return None;
        }
    }
    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &pfad.with_file_name("measurements.smis")).unwrap();
    let defs = lies_nahtliste(&std::fs::read_to_string(&liste).unwrap()).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    Some(b.naehte)
}

/// Difference of the two sides of one seam in cm, or `None` if it is absent.
fn unterschied(naehte: &[Nahtergebnis], kennung: &str) -> Option<f64> {
    naehte.iter().find(|n| n.kennung == kennung).map(|n| n.diff_cm)
}

#[test]
fn kopie_traegt_vierzig_millimeter_mehrweite() {
    let Some(naehte) = lies("bluse2_v26_kappe40") else { return };

    for kennung in KAPPENNAEHTE {
        let d = unterschied(&naehte, kennung)
            .unwrap_or_else(|| panic!("{kennung} fehlt in der Kopie"));
        eprintln!("{kennung}: Kappe {:.1} mm länger als das Armloch", d * 10.0);
        assert!(
            (d * 10.0 - DOSIS_MM).abs() < 0.5,
            "{kennung}: {:.1} mm Mehrweite, erwartet {DOSIS_MM:.1} mm",
            d * 10.0
        );
    }

    // The dose is the only thing #115 changes: the other seams have to show
    // v26's differences, not shifted ones.
    for (kennung, soll) in UNVERAENDERT {
        let d = unterschied(&naehte, kennung)
            .unwrap_or_else(|| panic!("{kennung} fehlt in der Kopie"));
        eprintln!("{kennung}: {d:+.4} cm (v26: {soll:+.4} cm)");
        assert!(
            (d - soll).abs() < 0.01,
            "{kennung} hat sich verschoben: {d:+.4} cm gegen {soll:+.4} cm"
        );
    }
}

#[test]
fn kopie_hat_dieselben_teile_wie_die_vorlage() {
    // The copy must stay the same garment, otherwise v26's placement no longer
    // applies and the dressing run measures a different blouse.
    let Some(eigene) = lies("bluse2_v26_kappe40") else { return };
    let Some(vorlage) = lies("bluse2_v26_einhalte") else { return };

    let kennungen = |v: &[Nahtergebnis]| -> Vec<String> {
        let mut k: Vec<String> = v.iter().map(|n| n.kennung.clone()).collect();
        k.sort();
        k
    };
    eprintln!(
        "Nähte: Kopie {} / #111 {}",
        eigene.len(),
        vorlage.len()
    );
    assert_eq!(
        kennungen(&eigene),
        kennungen(&vorlage),
        "die Kopie hat andere Nähte als die #111-Vorlage"
    );
}
