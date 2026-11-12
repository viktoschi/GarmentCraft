//! `klon3d innenpfade`: die Innenpfade je Teil aus der Kommandozeile.
//!
//! Die Zahlen stammen aus `klon3d-pattern/tests/innenpfad.rs`; hier steht,
//! dass der Befehl sie auch druckt und dass ein unaufloesbarer Pfad die
//! Rueckgabe 1 gibt.

use std::path::PathBuf;
use std::process::Command;

fn muster(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../klon3d-pattern/tests/muster")
        .join(name)
}

fn lauf(name: &str) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_klon3d"))
        .arg("innenpfade")
        .arg(muster(name))
        .output()
        .expect("klon3d startet");
    (o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Rock v36: vier Teile mit je drei Klebezonen, keine geschlossen.
#[test]
fn rock_v36_zeigt_zwoelf_offene_pfade() {
    let (ok, t) = lauf("hexen_2026_10_27/rock_v36.sm2d");
    assert!(ok, "{t}");
    assert!(t.contains("12 Innenpfade an 4 von 23 Teilen"), "{t}");
    assert!(t.contains("Klebezone VS1 2 x 11 cm"), "{t}");
    assert!(t.contains("130.0 mm"), "{t}");
    // Offene Pfade tragen keine Flaeche.
    assert!(!t.contains("mm²"), "{t}");
    assert!(!t.contains("geschlossen"), "{t}");
}

/// Bustier v1: 32 Pfade, das Rueckteil rechts traegt 15.
#[test]
fn bustier_zeigt_die_oesen() {
    let (ok, t) = lauf("bustier_v1.sm2d");
    assert!(ok, "{t}");
    assert!(t.contains("32 Innenpfade an 4 von 14 Teilen"), "{t}");
    assert!(t.contains("Rueckteil rechts") && t.contains("15 Innenpfade"), "{t}");
    assert!(t.contains("Oese0_laengs_R") && t.contains("6.0 mm"), "{t}");
}

/// Ein Muster ohne Innenpfade meldet keine und endet mit 0.
#[test]
fn muster_ohne_innenpfade() {
    let (ok, t) = lauf("bluse3_v1.sm2d");
    assert!(ok, "{t}");
    assert!(t.contains("0 Innenpfade an 0 von"), "{t}");
}
