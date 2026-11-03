//! UI tests of stage 6f: "Datei › Export › OBJ…" and "DXF…".
//!
//! Headless with egui_kittest like `oberflaeche.rs`. The files are compared with what the pipeline
//! produces directly – `Muster::dxf_teile` + `dxf::schreibe`, which is exactly what `klon3d dxf`
//! calls – not with the app's own data. The test patterns are copied to a fresh folder first: the
//! dialog writes next to the pattern, and the repository's test data must stay untouched (Seamly
//! writes its `.sm2d` back when it is loaded).
//!
//! `env!("CARGO_BIN_EXE_klon3d")` only exists in the test targets of `klon3d-cli`, so the command
//! line itself is checked in `klon3d-cli/tests/dxf.rs`; here the reference is the same pipeline
//! call, byte for byte.

mod gemeinsam;

use std::path::{Path, PathBuf};

use eframe::egui::Role;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use gemeinsam::*;
use klon3d_app::dateiwahl::Zweck;
use klon3d_app::simulation::{Anziehen, Befehl};
use klon3d_pipeline::{dxf, Muster};
use klon3d_mesh::Parameter;

/// The Bluse3 v2 with the files `klon3d anziehen` and `klon3d dxf` need, copied to `ordner`.
fn kopie(ordner: &Path) -> PathBuf {
    let muster = ordner.join("bluse3_v2.sm2d");
    for name in ["bluse3_v2.sm2d", "measurements.smis"] {
        std::fs::copy(muster_ordner().join(name), ordner.join(name)).unwrap();
    }
    for endung in ["naehte", "lage"] {
        let ziel = muster.with_extension(endung);
        std::fs::copy(bluse().with_extension(endung), &ziel).unwrap();
    }
    muster
}

/// What `klon3d dxf` writes for `muster`: every piece with an outline, in DXF bytes.
fn dxf_wie_klon3d(muster: &Path, massdatei: &Path) -> Vec<u8> {
    let m = Muster::laden(muster, Some(massdatei)).unwrap();
    let teile: Vec<dxf::Teil> = m
        .dxf_teile(|_| true)
        .unwrap()
        .into_iter()
        .filter(|d| !d.uebergangen)
        .map(|d| d.teil)
        .collect();
    assert!(!teile.is_empty());
    dxf::schreibe(&teile)
}

/// The number of `v` lines – one per point of the Wavefront OBJ.
fn punkte(text: &str) -> usize {
    text.lines().filter(|z| z.starts_with("v ")).count()
}

fn text(pfad: &Path) -> String {
    std::fs::read_to_string(pfad).unwrap_or_else(|e| panic!("{}: {e}", pfad.display()))
}

#[test]
fn ohne_muster_ist_der_export_gesperrt() {
    let mut h = fenster();
    h.run();
    menue_oeffnen(&mut h, &["Datei", "Export"]);
    for eintrag in ["OBJ…", "DXF…"] {
        let k = im_menue(&h, eintrag);
        assert!(
            k.accesskit_node().is_disabled(),
            "Datei › Export › {eintrag} frei"
        );
        k.hover();
        h.run();
        let da = h
            .query_all_by_role(Role::Tooltip)
            .any(|t| t.query_by_label("erst ein Muster öffnen").is_some());
        assert!(da, "{eintrag}: kein Hinweis „erst ein Muster öffnen“");
    }
    menue_schliessen(&mut h, "Datei");
}

#[test]
fn dxf_export_schreibt_was_klon3d_dxf_schreibt() {
    let ordner = frischer_ordner("dxf");
    let muster = kopie(&ordner);
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&muster));
    h.run();

    menue_waehlen(&mut h, &["Datei", "Export"], "DXF…");
    let d = h.state().dateiwahl.as_ref().expect("Dialog offen");
    assert_eq!(d.zweck, Zweck::DxfSpeichern);
    assert_eq!(d.name, "bluse3_v2.dxf");
    assert!(d.zweck.speichern());
    assert!(h.query_by_label("DXF speichern").is_some());
    h.get_by_label("Datei speichern").click();
    h.run();
    assert!(h.state().dateiwahl.is_none(), "Dialog zu");

    let ziel = ordner.join("bluse3_v2.dxf");
    let soll = dxf_wie_klon3d(&muster, &ordner.join("measurements.smis"));
    let ist = std::fs::read(&ziel).unwrap();
    assert_eq!(
        ist, soll,
        "{} Bytes statt {} – nicht das, was klon3d dxf schreibt",
        ist.len(),
        soll.len()
    );
    // The status bar says what was written: the number of pieces is the reference's.
    let m = h.state().meldung.clone();
    assert!(m.starts_with("DXF gespeichert: "), "Meldung: {m}");
    assert!(m.contains(&format!(" ({} Teile", 28)), "Meldung: {m}");
    assert!(!h.state().meldung_fehler);
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn obj_export_ohne_simulation_schreibt_das_musternetz() {
    let ordner = frischer_ordner("obj-netz");
    let muster = kopie(&ordner);
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&muster));
    h.run();
    assert!(
        !h.state().simulation.bereit(),
        "hier wird ohne Simulation exportiert"
    );

    menue_waehlen(&mut h, &["Datei", "Export"], "OBJ…");
    let d = h.state().dateiwahl.as_ref().expect("Dialog offen");
    assert_eq!(d.zweck, Zweck::ObjSpeichern);
    assert_eq!(d.name, "bluse3_v2.obj");
    h.get_by_label("Datei speichern").click();
    h.run();
    assert!(h.state().dateiwahl.is_none(), "Dialog zu");

    // As many points as the pattern's mesh has, computed through the pipeline.
    let m = Muster::laden(&muster, Some(&ordner.join("measurements.smis"))).unwrap();
    let defs = klon3d_pipeline::nahtliste(&muster.with_extension("naehte")).unwrap();
    let netz = m.netz(&defs, &Parameter::default()).unwrap();
    let soll: usize = netz.teile.iter().map(|t| t.punkte.len()).sum();

    let text = text(&ordner.join("bluse3_v2.obj"));
    assert_eq!(punkte(&text), soll, "Punkte im OBJ");
    // The file says itself that it is not a simulated scene.
    let kopf = text.lines().take(2).collect::<Vec<_>>().join("\n");
    assert!(kopf.contains("ohne Simulation"), "Dateikopf:\n{kopf}");
    let meldung = h.state().meldung.clone();
    assert!(meldung.contains("OBJ des Musternetzes gespeichert"), "{meldung}");
    assert!(meldung.contains("ohne Simulation"), "{meldung}");
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "nur mit --release: ein Löserbild dauert ~0,3 s"
)]
fn obj_export_nach_kurzer_simulation_hat_so_viele_punkte_wie_die_szene() {
    if !koerperordner_da() {
        return;
    }
    let ordner = frischer_ordner("obj-szene");
    let muster = kopie(&ordner);
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&muster));
    h.run();

    // Two sewing frames, at most two settling frames, one hold frame.
    let kurz = Anziehen {
        naehbilder: 2,
        einschwingen_max: 2,
        haltebilder: 1,
        ..Anziehen::default()
    };
    let v = h.state().vorgaben().expect("Vorgaben");
    let ctx = h.ctx.clone();
    h.state_mut().simulation.starten(v, kurz, &ctx);
    assert!(
        h.state().simulation.eingeschaltet(),
        "Simulation nicht gestartet: {:?}",
        h.state().simulation.fehler()
    );
    let start = std::time::Instant::now();
    while h.state().simulation.bildnummer() < 1 {
        assert!(
            start.elapsed().as_secs_f64() < 600.0,
            "kein Bild: {:?}",
            h.state().simulation.fehler()
        );
        assert!(
            h.state().simulation.fehler().is_none(),
            "Simulation: {:?}",
            h.state().simulation.fehler()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.state_mut().simulation.abholen();
    }
    // Hold the run: a `Harness::run` while the worker keeps repainting never settles. The export
    // is the same with a held simulation (`bereit`, `bau` and `bild` are what it reads).
    h.state_mut().simulation.befehl(Befehl::Anhalten);
    h.run();
    assert!(h.state().simulation.bereit(), "Szene nicht bereit");

    menue_waehlen(&mut h, &["Datei", "Export"], "OBJ…");
    h.get_by_label("Datei speichern").click();
    h.run();

    let szene = h.state().simulation.bild().expect("Bild").x.len();
    let bau = h.state().simulation.bau().expect("Bau");
    assert_eq!(
        szene, bau.platziert_punkte,
        "die Szene hat so viele Punkte wie das platzierte Netz"
    );
    let text = text(&ordner.join("bluse3_v2.obj"));
    assert_eq!(punkte(&text), szene, "Punkte im OBJ");
    // Every piece is a group, as in `klon3d anziehen`'s ende.obj.
    let gruppen = text.lines().filter(|z| z.starts_with("g ")).count();
    assert_eq!(gruppen, bau.teile.len(), "Gruppen im OBJ");
    let meldung = h.state().meldung.clone();
    assert!(meldung.contains("OBJ der Szene gespeichert"), "{meldung}");

    h.state_mut().simulation.beenden();
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn export_fragt_vor_dem_ueberschreiben() {
    let ordner = frischer_ordner("ueberschreiben");
    let muster = kopie(&ordner);
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&muster));
    h.run();

    for (eintrag, name, alt) in [
        ("DXF…", "bluse3_v2.dxf", "# alt dxf\n"),
        ("OBJ…", "bluse3_v2.obj", "# alt obj\n"),
    ] {
        let ziel = ordner.join(name);
        std::fs::write(&ziel, alt).unwrap();
        menue_waehlen(&mut h, &["Datei", "Export"], eintrag);
        h.get_by_label("Datei speichern").click();
        h.run();
        assert!(
            h.query_by_label_contains("gibt es schon – überschreiben?")
                .is_some(),
            "{eintrag}: keine Rückfrage"
        );
        assert_eq!(text(&ziel), alt, "{eintrag}: schon überschrieben");
        h.get_by_label("Nein").click();
        h.run();
        assert!(h.state().dateiwahl.is_some(), "{eintrag}: Dialog zu");
        assert_eq!(text(&ziel), alt, "{eintrag}: nach „Nein“ geschrieben");

        h.get_by_label("Datei speichern").click();
        h.run();
        h.get_by_label("Überschreiben").click();
        h.run();
        assert!(h.state().dateiwahl.is_none(), "{eintrag}: Dialog offen");
        let neu = std::fs::read(&ziel).unwrap();
        assert!(neu.len() > alt.len(), "{eintrag}: nicht geschrieben");
        assert!(!h.state().meldung_fehler, "{eintrag}: {}", h.state().meldung);
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn export_schreibt_in_den_ordner_des_musters() {
    // The dialog starts at the pattern's folder, not at the last one used: the export belongs
    // next to the pattern it was made from.
    let ordner = frischer_ordner("ordner");
    let muster = kopie(&ordner);
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&muster));
    h.run();
    menue_waehlen(&mut h, &["Datei", "Export"], "DXF…");
    let d = h.state().dateiwahl.as_ref().expect("Dialog offen");
    assert_eq!(d.liste.ordner, ordner);
    assert_eq!(d.ziel(), Some(ordner.join("bluse3_v2.dxf")));
    h.get_by_label("Abbrechen").click();
    h.run();
    assert!(h.state().dateiwahl.is_none());
    let _ = std::fs::remove_dir_all(&ordner);
}
