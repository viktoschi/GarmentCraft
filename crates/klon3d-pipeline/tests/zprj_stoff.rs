#![allow(non_snake_case)] // Tests sind deutsch benannt

//! A fabric read out of a `.zprj` project becomes the solver's material.
//!
//! Until #131 the project reader carried thickness, weight and the profile name
//! - and nothing the solver could bend or stretch with, so `anziehen --zprj`
//! took the body from the project and the fabric from `--material`. This module
//! is the second step: the same `mapPhysical` fields mapped with the same
//! factors as a `.zfab`, and the round trip against the built-in profile proves
//! the two paths agree.
//!
//! What this does **not** prove: that a whole project can be draped. A `.zprj`
//! still has no layer file, so a scene is built from a pattern, not from the
//! project. The test is about the material hand-off, not about a garment.

use std::path::{Path, PathBuf};

use klon3d_pattern::zprj::Projekt;
use klon3d_pipeline::stoff::{aus_zprj_stoff, stoff_laden};
use klon3d_sim::RADICAL_RUBBER_04;

const PRUEFPROJEKT: &str =
    r"D:\ai\rea\work\messung\japanese_school_uniform\japanese_school_uniform.zprj";

fn pruefprojekt() -> Option<PathBuf> {
    let p = PathBuf::from(PRUEFPROJEKT);
    p.is_file().then_some(p)
}

fn zfab() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/stoffe/Latex_0.4mm_Radical_Rubber.zfab")
}

/// The latex fabric in the reference project has to land on the same numbers as
/// the built-in profile - the project and the `.zfab` describe one fabric, so
/// the two readers must not disagree.
///
/// The `f32` resolution is the limit here, exactly as in `stoff.rs`: the file
/// stores `f32`, the constant carries decimals.
#[test]
fn der_Projektstoff_ist_dasselbe_Material_wie_die_zfab() {
    let Some(p) = pruefprojekt() else {
        eprintln!("übersprungen: {PRUEFPROJEKT} fehlt");
        return;
    };
    let projekt = Projekt::lies(&p).expect("Projekt lesbar");
    // By the fabric's **own name**, not by profile: `white latex 0.4mm` and
    // `black latex 0.4mm` both carry `Latex_0.4mm_Radical_Rubber`, so a profile
    // filter would depend on list order. That the name is needed is itself part
    // of #131 (`zprj_stoff` refuses an ambiguous profile name).
    let s = projekt
        .stoffe
        .iter()
        .find(|s| s.name == "white latex 0.4mm")
        .expect("white latex 0.4mm im Prüfprojekt");

    let aus_projekt = aus_zprj_stoff(s, &p).expect("Projektstoff wird ein Material");
    let aus_datei = stoff_laden(&zfab()).expect("die .zfab lädt");

    // Every number the solver reads, against the other path.
    for (was, a, b) in [
        ("Dehnung", aus_projekt.dehnung_n_m, aus_datei.dehnung_n_m),
        ("Kette", aus_projekt.kette_n_m, aus_datei.kette_n_m),
        ("Schub", aus_projekt.schub_n_m, aus_datei.schub_n_m),
        ("Biegung", aus_projekt.biegung_nm, aus_datei.biegung_nm),
        (
            "Biegung Kette",
            aus_projekt.biegung_kette_nm,
            aus_datei.biegung_kette_nm,
        ),
        (
            "Biegung schräg",
            aus_projekt.biegung_schraege_nm,
            aus_datei.biegung_schraege_nm,
        ),
        (
            "Flächengewicht",
            aus_projekt.flaechengewicht_kg_m2,
            aus_datei.flaechengewicht_kg_m2,
        ),
        ("Dicke", aus_projekt.dicke_m, aus_datei.dicke_m),
        ("Reibung", aus_projekt.reibung, aus_datei.reibung),
    ] {
        let rel = ((a - b) / b).abs();
        assert!(
            rel < 1e-6,
            "{was}: Projekt {a:e} gegen Datei {b:e}, relativ {rel:.2e}"
        );
    }

    // The third path: the constant the built-in profile was written from. It
    // carries decimals, the project stores `f32`, so the same 1e-6 separates
    // them as above - but the check is here, not left to the `.zfab`.
    for (was, a, b) in [
        ("Dehnung", aus_projekt.dehnung_n_m, RADICAL_RUBBER_04.dehnung_n_m),
        ("Kette", aus_projekt.kette_n_m, RADICAL_RUBBER_04.kette_n_m),
        ("Schub", aus_projekt.schub_n_m, RADICAL_RUBBER_04.schub_n_m),
        ("Biegung", aus_projekt.biegung_nm, RADICAL_RUBBER_04.biegung_nm),
        (
            "Biegung Kette",
            aus_projekt.biegung_kette_nm,
            RADICAL_RUBBER_04.biegung_kette_nm,
        ),
        (
            "Biegung schräg",
            aus_projekt.biegung_schraege_nm,
            RADICAL_RUBBER_04.biegung_schraege_nm,
        ),
        (
            "Flächengewicht",
            aus_projekt.flaechengewicht_kg_m2,
            RADICAL_RUBBER_04.flaechengewicht_kg_m2,
        ),
        ("Dicke", aus_projekt.dicke_m, RADICAL_RUBBER_04.dicke_m),
        ("Reibung", aus_projekt.reibung, RADICAL_RUBBER_04.reibung),
    ] {
        let rel = ((a - b) / b).abs();
        assert!(
            rel < 1e-6,
            "{was}: Projekt {a:e} gegen Profil {b:e}, relativ {rel:.2e}"
        );
    }

    // The name comes from the project's `qsName`, not from a file stem.
    assert_eq!(aus_projekt.name, s.name.as_str());
}

/// A fabric that does not state what the solver cannot do without is an error
/// that names the field - not a material with a zero in it.
#[test]
fn ein_unvollstaendiger_Stoff_nennt_das_Feld() {
    let Some(p) = pruefprojekt() else {
        eprintln!("übersprungen: {PRUEFPROJEKT} fehlt");
        return;
    };
    let projekt = Projekt::lies(&p).expect("Projekt lesbar");
    let mut s = projekt
        .stoffe
        .iter()
        .find(|s| s.name == "white latex 0.4mm")
        .expect("white latex 0.4mm")
        .clone();

    s.dehnung_schuss = None;
    let e = aus_zprj_stoff(&s, Path::new("test.zprj")).expect_err("Dehnung fehlt");
    let text = e.to_string();
    assert!(
        text.contains("fSuK"),
        "die Meldung muss das Feld nennen, war: {text}"
    );

    s.dehnung_schuss = Some(348_274.16);
    s.schub = None;
    let e = aus_zprj_stoff(&s, Path::new("test.zprj")).expect_err("Schub fehlt");
    assert!(
        e.to_string().contains("fHK"),
        "die Meldung muss das Feld nennen, war: {e}"
    );

    // A missing friction is not an error: CLO's default is a decision, and the
    // material still has one.
    s.schub = Some(174_159.05);
    s.reibung = None;
    let m = aus_zprj_stoff(&s, Path::new("test.zprj")).expect("Reibung fehlt, Vorgabe greift");
    assert_eq!(m.reibung, 0.03);
}
