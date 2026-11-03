//! The simulation runner on its own, without a window: what is missing before a run
//! ([`hindernis`]), and the command loop of the worker thread – start, hold, single step, reset,
//! run to the end – against the newest [`Schnappschuss`].
//!
//! The solver owns its scene in the worker thread, so the test only sends [`Befehl`]s and calls
//! [`Simulation::abholen`] until the wanted snapshot is there. One CPU frame of `bluse3_v2` takes
//! about 260–350 ms in a release build (and about 15 s in a debug build), so the solving tests are
//! `ignore`d there like the acceptance tests of `klon3d-sim`.
//!
//! The pattern and its files are in the repository; the body needs the resizing folder
//! ([`klon3d_koerper::koerper::ordner`]). Without it the run is skipped with
//! `übersprungen: … fehlt`.

mod gemeinsam;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use klon3d_app::simulation::{hindernis, Anziehen, Befehl, Simulation, Vorgaben};
use klon3d_pipeline::ablauf::{Anziehvorgang, Phase};
use klon3d_pipeline::{Koerperquelle, Muster, Szene};
use klon3d_sim::{ImplizitCg, ImplizitParameter, RADICAL_RUBBER_04};

/// A short run through all three phases: 2 sewing frames, at most 2 settling frames, 1 hold frame.
fn kurz() -> Anziehen {
    Anziehen {
        naehbilder: 2,
        einschwingen_max: 2,
        haltebilder: 1,
        ..Anziehen::default()
    }
}

fn vorgaben(muster: PathBuf, massdatei: PathBuf) -> Vorgaben {
    Vorgaben::aus(muster, massdatei, RADICAL_RUBBER_04, None)
}

fn testmuster() -> Vorgaben {
    let o = gemeinsam::muster_ordner();
    vorgaben(o.join("bluse3_v2.sm2d"), o.join("measurements.smis"))
}

fn koerper_da() -> bool {
    let o = klon3d_koerper::koerper::ordner();
    if o.is_dir() {
        true
    } else {
        eprintln!("übersprungen: {} fehlt", o.display());
        false
    }
}

/// The same scene the runner builds, but straight from `klon3d_pipeline` – the reference for
/// "the window shows what the library computes".
fn direkte_szene(v: &Vorgaben) -> Szene {
    assert!(
        matches!(
            v.koerper,
            klon3d_app::simulation::Koerperdatei::Massdatei(_)
        ),
        "hier wird mit der Maßdatei des Musters gebaut"
    );
    let m = Muster::laden(&v.muster, Some(&v.massdatei)).expect("Muster");
    let defs = klon3d_pipeline::nahtliste(&v.nahtliste).expect("Nahtliste");
    let netz = m
        .netz(&defs, &klon3d_mesh::Parameter::default())
        .expect("Netz");
    let lage = klon3d_pipeline::lageliste(&v.lage).expect("Lage");
    let ordner = klon3d_koerper::koerper::ordner();
    let k = klon3d_pipeline::koerper(Koerperquelle::MassdateiMitOrdner(&v.massdatei, ordner))
        .expect("Körper");
    klon3d_pipeline::szene(
        &netz,
        &lage,
        &k,
        v.material,
        0.0,
        klon3d_pipeline::standard_simparameter(),
    )
    .expect("Szene")
}

/// The same number of frames on a scene of [`direkte_szene`], frame by frame.
fn direkter_lauf(v: &Vorgaben, a: &Anziehen, bilder: usize) -> Szene {
    let mut s = direkte_szene(v);
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let mut vorgang = Anziehvorgang::beginnen(&mut s, &rw, a);
    for i in 1..=bilder {
        assert!(
            vorgang.schritt(&mut s, &mut rw).is_some(),
            "der direkte Lauf endet vor Bild {i}"
        );
    }
    s
}

/// Wait until `bedingung` holds, taking in what the worker sent meanwhile.
fn abwarten(
    sim: &mut Simulation,
    sekunden: f64,
    was: &str,
    bedingung: impl Fn(&Simulation) -> bool,
) {
    let start = Instant::now();
    while !bedingung(sim) {
        if start.elapsed().as_secs_f64() > sekunden {
            panic!(
                "{was}: nach {sekunden} s nicht erreicht (Bild {}, läuft {}, bereit {}, Fehler {:?})",
                sim.bildnummer(),
                sim.laeuft(),
                sim.bereit(),
                sim.fehler()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
        sim.abholen();
    }
}

/// Wait until the frame number has stayed the same for a moment: the worker holds.
fn angehalten(sim: &mut Simulation, sekunden: f64) -> usize {
    let start = Instant::now();
    let mut letzte = sim.bildnummer();
    while start.elapsed().as_secs_f64() < sekunden {
        std::thread::sleep(Duration::from_millis(200));
        sim.abholen();
        if sim.bildnummer() == letzte && !sim.laeuft() {
            return letzte;
        }
        letzte = sim.bildnummer();
    }
    panic!("hält nicht: Bild {}", sim.bildnummer());
}

#[test]
fn hindernis_nennt_die_fehlende_datei() {
    let o = gemeinsam::muster_ordner();

    // Nothing at all: the pattern itself.
    let leer = gemeinsam::frischer_ordner("simulation-leer");
    let h = hindernis(&vorgaben(
        leer.join("fehlt.sm2d"),
        o.join("measurements.smis"),
    ))
    .unwrap();
    assert_eq!(
        h,
        format!("Muster fehlt: {}", leer.join("fehlt.sm2d").display())
    );

    // The pattern, but no measurement file: the hover text names the menu path.
    let h = hindernis(&vorgaben(o.join("bluse3_v2.sm2d"), leer.join("fehlt.smis"))).unwrap();
    assert!(h.starts_with("Maßdatei fehlt: "), "{h}");
    assert!(h.ends_with(" – Avatar › Maßdatei wählen…"), "{h}");
    assert!(
        h.contains(&leer.join("fehlt.smis").display().to_string()),
        "{h}"
    );

    // A copy of the pattern without seam list and placement list: the seam list is named first,
    // then the placement list, both with the menu path that makes them.
    let kopie = gemeinsam::frischer_ordner("simulation-ohne-listen");
    std::fs::copy(o.join("bluse3_v2.sm2d"), kopie.join("bluse3_v2.sm2d")).unwrap();
    let h = hindernis(&vorgaben(
        kopie.join("bluse3_v2.sm2d"),
        o.join("measurements.smis"),
    ))
    .unwrap();
    assert!(h.starts_with("Nahtliste fehlt: "), "{h}");
    assert!(h.ends_with(" – 2D-Schnitt › Nähte vorschlagen…"), "{h}");
    std::fs::copy(o.join("bluse3_v2.naehte"), kopie.join("bluse3_v2.naehte")).unwrap();
    let h = hindernis(&vorgaben(
        kopie.join("bluse3_v2.sm2d"),
        o.join("measurements.smis"),
    ))
    .unwrap();
    assert_eq!(
        h,
        format!(
            "Lagedatei fehlt: {}",
            kopie.join("bluse3_v2.lage").display()
        )
    );

    // All files there: nothing missing (the body folder is checked too).
    if !koerper_da() {
        return;
    }
    assert_eq!(hindernis(&testmuster()), None);

    // A CLO project that is not there is named at its own entry.
    let projekt = Vorgaben::aus(
        o.join("bluse3_v2.sm2d"),
        o.join("measurements.smis"),
        RADICAL_RUBBER_04,
        Some(klon3d_app::simulation::Koerperdatei::Projekt(
            leer.join("fehlt.zprj"),
        )),
    );
    let h = hindernis(&projekt).unwrap();
    assert_eq!(
        h,
        format!("CLO-Projekt fehlt: {}", leer.join("fehlt.zprj").display())
    );
}

#[test]
fn ohne_simulation_laufen_keine_befehle_ins_leere() {
    let mut sim = Simulation::neu();
    assert!(!sim.eingeschaltet() && !sim.bereit() && !sim.laeuft());
    assert_eq!(sim.bildnummer(), 0);
    assert!(sim.bau().is_none() && sim.bild().is_none());
    // Every command is dropped without a worker, and taking in messages does nothing.
    sim.befehl(Befehl::Laufen);
    sim.befehl(Befehl::Anhalten);
    sim.befehl(Befehl::Schritte(1));
    sim.befehl(Befehl::Zuruecksetzen);
    sim.abholen();
    assert!(!sim.laeuft() && !sim.bereit() && !sim.eingeschaltet());
    assert!(sim.fehler().is_none() && sim.fehler_nehmen().is_none());
    assert!(sim.bilder_pro_sekunde().is_none());
    assert_eq!(sim.phase(), Phase::Naehen);
    assert_eq!(sim.phase_name(), "Nähen");
    sim.beenden();
}

#[test]
fn fehlender_aufbau_wird_gemeldet() {
    let mut sim = Simulation::neu();
    let ctx = egui::Context::default();
    let leer = gemeinsam::frischer_ordner("simulation-aufbau-fehler");
    let v = vorgaben(
        leer.join("fehlt.sm2d"),
        gemeinsam::muster_ordner().join("measurements.smis"),
    );
    sim.starten(v, kurz(), &ctx);
    assert!(sim.eingeschaltet() && sim.laeuft());
    abwarten(&mut sim, 60.0, "Fehlermeldung", |s| s.fehler().is_some());
    let text = sim.fehler().unwrap().to_string();
    // `starten` does not ask `hindernis` first (the menu does), so the message comes from the
    // pattern reader - it names the file either way, and the simulation does not come up.
    assert!(text.contains("fehlt.sm2d"), "{text}");
    // The thread is gone: no worker, no frame, and the error is taken once.
    assert!(!sim.eingeschaltet() && !sim.laeuft() && !sim.bereit());
    assert!(sim.bau().is_none() && sim.bild().is_none());
    assert_eq!(sim.fehler_nehmen().as_deref(), Some(text.as_str()));
    assert!(sim.fehler().is_none());
    sim.beenden();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "nur mit --release: ein Löserbild dauert ~0,3 s"
)]
fn simulation_naeht_haelt_an_schreitet_und_setzt_zurueck() {
    if !koerper_da() {
        return;
    }
    let ctx = egui::Context::default();
    let mut sim = Simulation::neu();
    sim.starten(testmuster(), kurz(), &ctx);
    // Hold right away: the command waits in the channel while the worker builds, so the scene
    // stays at frame 0 and its positions can be compared with the reset later.
    sim.befehl(Befehl::Anhalten);
    assert!(sim.eingeschaltet(), "kein Faden gestartet");

    abwarten(&mut sim, 300.0, "Aufbau", |s| {
        s.bereit() || s.fehler().is_some()
    });
    assert!(sim.fehler().is_none(), "Simulation: {:?}", sim.fehler());
    let n = angehalten(&mut sim, 30.0);
    assert_eq!(n, 0, "vor dem ersten Bild schon {} Bilder", n);
    assert!(sim.frisch(), "frisch nach dem Aufbau");
    assert!(!sim.laeuft());

    // What the build reports: body, cloth pieces and where they came from.
    let bau = sim.bau().unwrap();
    assert!(!bau.koerper.is_empty(), "Körper ohne Punkte");
    assert!(!bau.koerper_dreiecke.is_empty(), "Körper ohne Dreiecke");
    assert!(!bau.stoff_dreiecke.is_empty(), "Stoff ohne Dreiecke");
    assert!(!bau.teile.is_empty(), "keine Teile");
    assert!(bau.netz_punkte > 0 && bau.platziert_punkte > 0);
    assert!(!bau.herkunft.is_empty(), "Herkunft fehlt");
    assert_eq!(bau.material, RADICAL_RUBBER_04.name);
    assert!(bau.aufbau_s > 0.0, "Aufbauzeit {}", bau.aufbau_s);
    let (min, max) = bau.huelle();
    assert!(
        (0..3).all(|i| min[i].is_finite() && max[i] > min[i]),
        "{min:?} {max:?}"
    );
    let x0 = sim.bild().unwrap().x.clone();
    assert_eq!(x0.len(), bau.platziert_punkte, "Punktezahl von Bild 0");

    // Running: frames come in, numbered from 1, each with its own duration.
    sim.befehl(Befehl::Laufen);
    assert!(!sim.frisch(), "läuft, also nicht mehr frisch");
    abwarten(&mut sim, 120.0, "zwei Bilder", |s| s.bildnummer() >= 2);
    let bild = sim.bild().unwrap();
    assert!(bild.dauer_s > 0.0, "Bilddauer {}", bild.dauer_s);
    assert!(bild.zeit_s > 0.0, "Zeit {}", bild.zeit_s);
    assert!(bild.rechenzeit_s > 0.0, "Rechenzeit {}", bild.rechenzeit_s);
    assert!(
        matches!(sim.phase(), Phase::Naehen | Phase::Einschwingen),
        "Phase {:?}",
        sim.phase()
    );
    assert_eq!(
        sim.phase_name(),
        match sim.phase() {
            Phase::Naehen => "Nähen",
            Phase::Einschwingen => "Einschwingen",
            Phase::Halt => "Halten",
            Phase::Fertig => "fertig",
        }
    );
    let rate = sim.bilder_pro_sekunde().expect("Rate nach zwei Bildern");
    assert!(rate > 0.0, "Bilder/s {rate}");

    // Holding: the frame number stays where it was, no further frame is computed.
    sim.befehl(Befehl::Anhalten);
    let gehalten = angehalten(&mut sim, 30.0);
    assert!(gehalten >= 2, "nach {gehalten} Bildern angehalten");
    assert!(!sim.laeuft());

    // A single step computes exactly one more frame and holds again.
    sim.befehl(Befehl::Schritte(1));
    abwarten(&mut sim, 120.0, "einen Schritt", |s| {
        s.bildnummer() == gehalten + 1
    });
    assert_eq!(angehalten(&mut sim, 10.0), gehalten + 1);
    assert!(!sim.laeuft());

    // Reset: frame 0 again, bit for bit the positions the scene was built with.
    sim.befehl(Befehl::Zuruecksetzen);
    abwarten(&mut sim, 60.0, "Zurücksetzen", |s| s.bildnummer() == 0);
    assert_eq!(angehalten(&mut sim, 10.0), 0);
    let zurueck = &sim.bild().unwrap().x;
    assert_eq!(zurueck.len(), x0.len());
    for (i, (a, b)) in zurueck.iter().zip(&x0).enumerate() {
        for k in 0..3 {
            assert!(
                (a[k] - b[k]).abs() < 1e-9,
                "Punkt {i} Achse {k}: {} statt {}",
                a[k],
                b[k]
            );
        }
    }
    assert!(!sim.frisch(), "nach dem Zurücksetzen nicht frisch");
    assert!(!sim.fertig());

    // To the end: 2 sewing frames, one or two settling frames, 1 hold frame, then done.
    sim.befehl(Befehl::Laufen);
    abwarten(&mut sim, 300.0, "Ende", |s| s.fertig());
    assert_eq!(sim.phase(), Phase::Fertig);
    assert!(!sim.laeuft(), "fertig, also angehalten");
    let ende = sim.bildnummer();
    assert!((4..=5).contains(&ende), "Ende nach {ende} Bildern");
    let bild = sim.bild().unwrap();
    assert!(bild.fertig && bild.zeit_s > 0.0 && bild.rechenzeit_s > 0.0);

    // Stopping ends the thread; commands afterwards do nothing.
    sim.beenden();
    assert!(!sim.eingeschaltet() && !sim.bereit() && !sim.laeuft());
    sim.befehl(Befehl::Laufen);
    assert!(!sim.laeuft());
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "nur mit --release: ein Löserbild dauert ~0,3 s"
)]
fn die_bilder_sind_das_was_der_ablauf_direkt_rechnet() {
    if !koerper_da() {
        return;
    }
    let a = kurz();
    let v = testmuster();
    let ctx = egui::Context::default();
    let mut sim = Simulation::neu();
    sim.starten(v.clone(), a.clone(), &ctx);
    // Hold while the worker builds, so no frame is computed behind our back.
    sim.befehl(Befehl::Anhalten);
    abwarten(&mut sim, 300.0, "Aufbau", |s| {
        s.bereit() || s.fehler().is_some()
    });
    assert!(sim.fehler().is_none(), "Simulation: {:?}", sim.fehler());
    // Back to the scene as built, so both runs start from the same frame 0 whatever the worker
    // computed while it was building.
    sim.befehl(Befehl::Anhalten);
    sim.befehl(Befehl::Zuruecksetzen);
    abwarten(&mut sim, 60.0, "Zurücksetzen", |s| s.bildnummer() == 0);
    assert_eq!(angehalten(&mut sim, 30.0), 0);

    // Two frames, then the same two frames on a scene built straight from `klon3d_pipeline`.
    sim.befehl(Befehl::Schritte(2));
    abwarten(&mut sim, 300.0, "zwei Schritte", |s| s.bildnummer() == 2);
    assert_eq!(angehalten(&mut sim, 10.0), 2);
    let aus_der_simulation = sim.bild().unwrap().x.clone();
    let direkt = direkter_lauf(&v, &a, 2);
    assert_eq!(
        aus_der_simulation.len(),
        direkt.x.len(),
        "andere Punktzahl als der direkte Lauf"
    );
    let mut groesste: f64 = 0.0;
    for (i, (a, b)) in aus_der_simulation.iter().zip(&direkt.x).enumerate() {
        for k in 0..3 {
            let d = (a[k] - b[k]).abs();
            groesste = groesste.max(d);
            assert!(
                d < 1e-9,
                "Punkt {i} Achse {k}: {} statt {} (Abweichung {d:e})",
                a[k],
                b[k]
            );
        }
    }
    eprintln!(
        "größte Abweichung zum direkten Lauf: {groesste:e} m über {} Punkte",
        direkt.x.len()
    );
    sim.beenden();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "nur mit --release: ein Löserbild dauert ~0,3 s"
)]
fn die_oberflaeche_bleibt_waehrend_der_simulation_schnell() {
    if !koerper_da() {
        return;
    }
    // A window with the Bluse3 v2 that is actually built (the scene needs the body folder).
    let mut h = gemeinsam::mit_bluse();
    let ctx = h.ctx.clone();
    let gestartet = h.state_mut().simulation_starten(&ctx);
    assert!(
        gestartet,
        "Simulation nicht gestartet: {:?}",
        h.state().simulation.fehler()
    );
    // Wait (outside the measured frames) until the scene stands and a frame has been computed.
    // Afterwards the worker keeps solving on its own: `laeuft` says what the window last asked
    // for, not what the thread does, so the new frames are the witness here.
    let frist = Instant::now();
    while h.state().simulation.bildnummer() < 1 {
        assert!(
            frist.elapsed().as_secs_f64() < 300.0,
            "keine Bilder: {:?}",
            h.state().simulation.fehler()
        );
        assert!(
            h.state().simulation.fehler().is_none(),
            "Simulation: {:?}",
            h.state().simulation.fehler()
        );
        std::thread::sleep(Duration::from_millis(50));
        h.state_mut().simulation.abholen();
    }
    let vorher = h.state().simulation.bildnummer();

    // Draw while the solver works in the background, until it has delivered another frame; the
    // slowest frame of the window is what matters, and a solver frame is ~300 ms away.
    let mut zeiten = Vec::new();
    let weiter = Instant::now();
    while h.state().simulation.bildnummer() == vorher {
        assert!(
            weiter.elapsed().as_secs_f64() < 300.0,
            "kein neues Bild nach 300 s (Bild {}, Fehler {:?})",
            h.state().simulation.bildnummer(),
            h.state().simulation.fehler()
        );
        let start = Instant::now();
        h.step();
        zeiten.push(start.elapsed().as_secs_f64());
    }
    zeiten.extend((0..20).map(|_| {
        let start = Instant::now();
        h.step();
        start.elapsed().as_secs_f64()
    }));
    assert!(
        h.state().simulation.bildnummer() > vorher,
        "der Löser hat während der {} gezeichneten Bilder nicht gerechnet",
        zeiten.len()
    );
    let langsamste = zeiten.iter().copied().fold(0.0_f64, f64::max);
    let mittel = zeiten.iter().sum::<f64>() / zeiten.len() as f64;
    eprintln!(
        "Bildzeit der Oberfläche: {:.1} ms im Mittel, {:.1} ms am langsamsten ({} Bilder, Bild {} des Lösers)",
        mittel * 1000.0,
        langsamste * 1000.0,
        zeiten.len(),
        h.state().simulation.bildnummer()
    );
    assert!(
        langsamste < 0.050,
        "langsamstes Bild {:.1} ms ≥ 50 ms (Mittel {:.1} ms)",
        langsamste * 1000.0,
        mittel * 1000.0
    );
    h.state_mut().simulation.beenden();
}
