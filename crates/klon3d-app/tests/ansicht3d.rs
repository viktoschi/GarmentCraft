//! The 3D view without a graphics card: the five standard views, the camera text of the status
//! bar, framing the scene, and the pane itself with `format: None` – its role and label, its hint
//! text, and that nothing is drawn (no render callback).
//!
//! A `Bau` and a `Schnappschuss` are data (public fields), so the test builds a tiny scene by hand
//! instead of running the solver: a body box from 0.80 m to 1.60 m and one cloth point in front of
//! it. The pane is run in a kittest harness with the lazy renderer, which has no wgpu – exactly the
//! case `Ansicht3d::ui` handles with its hint (see the module documentation of `ansicht3d.rs`).
//!
//! The hint text is painted with `Painter::galley`, not shown as a label: it lives in the frame's
//! shapes, so the test walks `Harness::output().shapes` for text shapes.

use eframe::egui;
use egui::Role;
use egui_kittest::kittest::Queryable as _;
use egui_kittest::Harness;
use klon3d_app::ansicht3d::{Ansicht3d, Blickwinkel};
use klon3d_app::simulation::{Bau, Schnappschuss, Teilbereich};
use klon3d_pipeline::ablauf::Phase;

/// A body box and one cloth point: 30 cm wide, 20 cm deep, 80 cm to 160 cm high (z up, front −y).
fn bau() -> Bau {
    Bau {
        koerper: vec![
            [-0.15, -0.10, 0.80],
            [0.15, -0.10, 0.80],
            [0.15, 0.10, 0.80],
            [-0.15, 0.10, 0.80],
            [-0.15, -0.10, 1.60],
            [0.15, -0.10, 1.60],
            [0.15, 0.10, 1.60],
            [-0.15, 0.10, 1.60],
        ],
        koerper_dreiecke: vec![[0, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6]],
        stoff_dreiecke: vec![[0, 1, 2]],
        teile: vec![Teilbereich {
            name: "Bluse 3 - Vorderteil".to_string(),
            punkte: 0..3,
            dreiecke: 0..1,
        }],
        netz_punkte: 8,
        platziert_punkte: 3,
        weggelassen: Vec::new(),
        herkunft: "tests/measurements.smis (Frau 38)".to_string(),
        material: "Radical Rubber 0.4".to_string(),
        aufbau_s: 0.5,
    }
}

/// Frame 0 of that scene: three cloth points in front of the chest, in front of the body.
fn schnappschuss() -> Schnappschuss {
    Schnappschuss {
        x: vec![
            [-0.12, -0.20, 1.00],
            [0.00, -0.22, 1.10],
            [0.12, -0.20, 1.00],
        ],
        bild: 0,
        phase: Phase::Naehen,
        zeit_s: 0.0,
        rechenzeit_s: 0.0,
        dauer_s: 0.0,
        fertig: false,
        karte: None,
    }
}

/// The distance in a camera text like `Vorne 2.4 m` – the decimal separator is not part of the
/// contract (the source's comment writes a comma, the format a point).
fn abstand_aus(text: &str) -> f64 {
    let zahl = text
        .rsplit(' ')
        .nth(1)
        .unwrap_or_else(|| panic!("kein Abstand in „{text}“"));
    zahl.replace(',', ".")
        .parse()
        .unwrap_or_else(|_| panic!("kein Abstand in „{text}“"))
}

#[test]
fn fuenf_blickwinkel_sind_verschieden_und_werden_gesetzt() {
    let alle = Blickwinkel::ALLE;
    assert_eq!(alle.len(), 5);
    // Menu order and names of Anzeige › Blickwinkel.
    let namen: Vec<&str> = alle.iter().map(|b| b.name()).collect();
    assert_eq!(namen, ["Vorne", "Hinten", "Links", "Rechts", "Oben"]);

    // Every view has its own (yaw, pitch); none repeats another.
    let winkel: Vec<(f64, f64)> = alle.iter().map(|b| b.winkel()).collect();
    for (i, a) in winkel.iter().enumerate() {
        for (k, b) in winkel.iter().enumerate().skip(i + 1) {
            assert!(
                (a.0 - b.0).abs() > 1e-9 || (a.1 - b.1).abs() > 1e-9,
                "{} und {} teilen sich {:?}",
                namen[i],
                namen[k],
                a
            );
        }
    }
    // Front looks along -y, up is the cap of the pitch.
    assert_eq!(winkel[0], (0.0, 0.0));
    assert!(winkel[4].1 > 1.0, "oben: {:?}", winkel[4]);

    // Setting a view sets exactly its angles and is recognised – and no other view fits as well.
    for b in alle {
        let mut a = Ansicht3d::default();
        a.blickwinkel(b);
        let (yaw, pitch) = b.winkel();
        assert_eq!((a.yaw, a.pitch), (yaw, pitch), "{}", b.name());
        assert!(a.ist_blickwinkel(b), "{}", b.name());
        for c in alle {
            if c != b {
                assert!(
                    !a.ist_blickwinkel(c),
                    "{} gilt auch für {}",
                    b.name(),
                    c.name()
                );
            }
        }
    }

    // The default camera is slightly raised, so it is no standard view.
    let a = Ansicht3d::default();
    assert!(
        !Blickwinkel::ALLE.iter().any(|b| a.ist_blickwinkel(*b)),
        "Standardkamera beim Start"
    );
}

#[test]
fn kamera_text_nennt_blickwinkel_oder_frei_und_zuruecksetzen_stellt_wieder_her() {
    let mut a = Ansicht3d::default();
    assert!(a.kamera_text().starts_with("frei "), "{}", a.kamera_text());
    assert_eq!(abstand_aus(&a.kamera_text()), 3.0);

    for b in Blickwinkel::ALLE {
        a.blickwinkel(b);
        assert!(a.kamera_text().starts_with(b.name()), "{}", a.kamera_text());
    }

    // The distance follows the camera, rounded to a tenth of a metre.
    a.abstand = 2.44;
    assert_eq!(abstand_aus(&a.kamera_text()), 2.4);
    a.abstand = 2.46;
    assert_eq!(abstand_aus(&a.kamera_text()), 2.5);
    // Any other angle is "frei", even a hair beside a standard view.
    a.blickwinkel(Blickwinkel::Vorne);
    a.yaw = 0.02;
    assert!(a.kamera_text().starts_with("frei "), "{}", a.kamera_text());

    // Reset: the default camera and the default scene again – wireframe off, target in the middle.
    a.netz = true;
    a.ziel = [1.0, 2.0, 3.0];
    a.abstand = 0.7;
    a.yaw = 1.0;
    a.pitch = 0.5;
    a.zuruecksetzen();
    let d = Ansicht3d::default();
    assert_eq!(
        (a.ziel, a.abstand, a.yaw, a.pitch, a.netz),
        (d.ziel, d.abstand, d.yaw, d.pitch, d.netz)
    );
    assert_eq!(a.dreiecke(), 0, "nach dem Zurücksetzen keine Vertexdaten");
}

#[test]
fn einpassen_legt_die_kamera_um_koerper_und_stoff() {
    let bau = bau();
    let x = schnappschuss().x;
    let (min, max) = bau.huelle();
    // The cloth reaches further forward than the body, so the box is the union of both.
    let mut min = min;
    let mut max = max;
    for p in &x {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    let mut a = Ansicht3d::default();
    a.einpassen(&bau, &x);
    for i in 0..3 {
        let soll = 0.5 * (min[i] + max[i]);
        assert!(
            (a.ziel[i] - soll).abs() < 1e-9,
            "Ziel Achse {i}: {} statt {soll}",
            a.ziel[i]
        );
    }
    // Far enough away for the bounding sphere, and within the limits.
    let mut r2 = 0.0;
    for i in 0..3 {
        r2 += (0.5 * (max[i] - min[i])).powi(2);
    }
    let r = r2.sqrt();
    assert!((0.2..=30.0).contains(&a.abstand), "Abstand {}", a.abstand);
    let halb = a.abstand * (40.0f64.to_radians() / 2.0).tan();
    assert!(halb >= r, "halbe Höhe {halb} < Radius {r}");

    // A scene without a single point falls back to the middle of a metre-high box.
    let leer = Bau {
        koerper: Vec::new(),
        koerper_dreiecke: Vec::new(),
        stoff_dreiecke: Vec::new(),
        teile: Vec::new(),
        netz_punkte: 0,
        platziert_punkte: 0,
        weggelassen: Vec::new(),
        herkunft: String::new(),
        material: String::new(),
        aufbau_s: 0.0,
    };
    let mut b = Ansicht3d::default();
    b.einpassen(&leer, &[]);
    assert_eq!(b.ziel, [0.0, 0.0, 1.0]);
    assert!(b.abstand.is_finite() && b.abstand > 0.0, "{}", b.abstand);
}

/// What one frame of the pane holds: the camera, and the scene to show.
struct Buehne {
    ansicht: Ansicht3d,
    bau: Option<Bau>,
    bild: Option<Schnappschuss>,
}

impl Buehne {
    fn neu(bau: Option<Bau>, bild: Option<Schnappschuss>) -> Self {
        Buehne {
            ansicht: Ansicht3d::default(),
            bau,
            bild,
        }
    }
}

fn buehne(bau: Option<Bau>, bild: Option<Schnappschuss>) -> Harness<'static, Buehne> {
    let mut h = Harness::builder()
        .with_size([420.0, 320.0])
        // 1/30 s per step: kittest feeds one event per step, and with its default step (0.25 s) the
        // two clicks of a double click lie 0.5 s apart - too slow for egui (0.3 s).
        .with_step_dt(1.0 / 30.0)
        .build_ui_state(
            |ui, s: &mut Buehne| s.ansicht.ui(ui, s.bau.as_ref(), s.bild.as_ref(), None),
            Buehne::neu(bau, bild),
        );
    h.run();
    h
}

/// Two quick primary clicks at `pos`. Some time passes first, so that a click shortly before does
/// not make it a triple click.
fn doppelklick_bei(h: &mut Harness<'_, Buehne>, pos: egui::Pos2) {
    h.run_steps(20);
    h.hover_at(pos);
    for _ in 0..2 {
        for gedrueckt in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: gedrueckt,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    h.run();
}

/// All texts painted in the last frame (a hint of the pane is a `Shape::Text`, not a label).
fn gemalte_texte(h: &Harness<'_, Buehne>) -> Vec<String> {
    fn sammeln(s: &egui::Shape, aus: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => aus.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| sammeln(s, aus)),
            _ => {}
        }
    }
    let mut aus = Vec::new();
    for cs in &h.output().shapes {
        sammeln(&cs.shape, &mut aus);
    }
    aus
}

/// Render callbacks in the last frame – none without a target format.
fn rueckrufe(h: &Harness<'_, Buehne>) -> usize {
    fn zaehlen(s: &egui::Shape) -> usize {
        match s {
            egui::Shape::Callback(_) => 1,
            egui::Shape::Vec(v) => v.iter().map(zaehlen).sum(),
            _ => 0,
        }
    }
    h.output().shapes.iter().map(|c| zaehlen(&c.shape)).sum()
}

#[test]
fn ohne_gpu_bleibt_die_rolle_und_der_hinweis_steht_da() {
    // No scene yet: the pane keeps its role and label and says what to do.
    let h = buehne(None, None);
    h.get_by_role_and_label(Role::Canvas, "3D-Ansicht");
    let texte = gemalte_texte(&h);
    assert!(
        texte.iter().any(|t| t.contains("Muster öffnen")),
        "Hinweis ohne Szene fehlt: {texte:?}"
    );
    assert_eq!(rueckrufe(&h), 0, "ohne Szene kein Zeichnen");
    // The camera was left alone.
    assert_eq!(h.state().ansicht.abstand, 3.0);
}

#[test]
fn mit_szene_ohne_gpu_wird_eingepasst_und_der_grund_genannt() {
    let h = buehne(Some(bau()), Some(schnappschuss()));
    h.get_by_role_and_label(Role::Canvas, "3D-Ansicht");
    let texte = gemalte_texte(&h);
    assert!(
        texte.iter().any(|t| t.contains("ohne Grafikkarte")),
        "Hinweis ohne wgpu fehlt: {texte:?}"
    );
    assert!(
        !texte.iter().any(|t| t.contains("Muster öffnen")),
        "falscher Hinweis: {texte:?}"
    );
    // Framed on the first frame: the target lies inside the body, the distance is no longer 3 m.
    let a = &h.state().ansicht;
    assert!((a.ziel[2] - 1.2).abs() < 1e-6, "Zielhöhe {}", a.ziel[2]);
    assert!(
        (a.abstand - 3.0).abs() > 1e-6,
        "nicht eingepasst: {}",
        a.abstand
    );
    // Without a target format nothing is drawn and no vertex data is built.
    assert_eq!(rueckrufe(&h), 0, "Rückruf ohne Format");
    assert_eq!(a.dreiecke(), 0);
    assert_eq!(a.segmente(), 0);
}

#[test]
fn ziehen_dreht_die_kamera_und_doppelklick_passt_wieder_ein() {
    let mut h = buehne(Some(bau()), Some(schnappschuss()));
    let mitte = h
        .get_by_role_and_label(Role::Canvas, "3D-Ansicht")
        .rect()
        .center();
    let vorher = h.state().ansicht.yaw;

    // Drag to the right with the primary button: yaw follows the drag, the pane stays "frei".
    h.hover_at(mitte);
    h.event(egui::Event::PointerButton {
        pos: mitte,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.event(egui::Event::PointerMoved(mitte + egui::vec2(40.0, 0.0)));
    h.run();
    h.event(egui::Event::PointerButton {
        pos: mitte + egui::vec2(40.0, 0.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();

    let gedreht = h.state().ansicht.yaw;
    assert!(
        (gedreht - vorher).abs() > 0.1,
        "kein Drehen: {vorher} → {gedreht}"
    );
    assert!(
        !Blickwinkel::ALLE
            .iter()
            .any(|b| h.state().ansicht.ist_blickwinkel(*b)),
        "gedrehte Kamera gilt als Standardblick"
    );

    // A standard view, then a double click frames the scene again.
    h.state_mut().ansicht.blickwinkel(Blickwinkel::Vorne);
    h.state_mut().ansicht.abstand = 0.5;
    h.run();
    assert!(h.state().ansicht.kamera_text().starts_with("Vorne "));

    doppelklick_bei(&mut h, mitte);

    assert!(
        (h.state().ansicht.abstand - 0.5).abs() > 1e-6,
        "nicht eingepasst"
    );
    assert!(
        h.state().ansicht.abstand > 0.5 && h.state().ansicht.abstand <= 30.0,
        "Abstand nach dem Doppelklick: {}",
        h.state().ansicht.abstand
    );
}
