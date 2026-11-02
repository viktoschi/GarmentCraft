//! Bedienung der App ohne Fenster: echte Tastenereignisse (Strg+O, Esc, Tab, F, L, Strg+Q) gehen
//! als `egui::Event` in den Harness – keine Maus, keine Tastatur, kein Fokus, kein Prozess.
//!
//! Die Ereignisse werden bewusst roh gebaut (`egui::Event::Key`), nicht über `Harness::key_press`:
//! so steht im Test, welche Modifikatoren ankommen. Auf Windows schickt winit für Strg sowohl
//! `ctrl` als auch `command`; egui vergleicht Kürzel aber über `command` (`Modifiers::COMMAND`).

mod gemeinsam;

use eframe::egui;
use egui::{Key, Modifiers, Role};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use gemeinsam::*;
use klon3d_app::app::App;
use klon3d_app::dateiwahl::Zweck;

/// Ein echter Tastendruck: Drücken und Loslassen als zwei `egui::Event::Key`, dann laufen lassen.
fn taste(h: &mut Harness<'_, App>, key: Key, modifiers: Modifiers) {
    for gedrueckt in [true, false] {
        h.event(egui::Event::Key {
            key,
            physical_key: None,
            pressed: gedrueckt,
            repeat: false,
            modifiers,
        });
    }
    h.run();
}

/// Nur das Drücken, genau ein Frame – damit die Viewport-Befehle dieses Frames sichtbar bleiben
/// (`Harness::run` läuft weiter, und `Close` wird nur im Frame des Tastendrucks geschickt).
fn druck_ein_frame(h: &mut Harness<'_, App>, key: Key, modifiers: Modifiers) -> bool {
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    h.step();
    let close = h.output().viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))
    });
    h.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    });
    h.step();
    close
}

/// Ist das Fenster `titel` (egui::Window) im AccessKit-Baum sichtbar?
fn fenster_da(h: &Harness<'_, App>, titel: &str) -> bool {
    h.query_all_by_role_and_label(Role::Window, titel)
        .any(|n| !n.accesskit_node().is_hidden())
}

// ───────────────────────────────────────────────────────────────── 1. Strg+O

#[test]
fn strg_o_oeffnet_den_dateidialog() {
    let mut h = mit_bluse();
    assert!(h.state().dateiwahl.is_none());
    assert!(!fenster_da(&h, "Muster öffnen"));

    taste(&mut h, Key::O, Modifiers::COMMAND);

    // Die App hat den Dialog geöffnet …
    let w = h.state().dateiwahl.as_ref().expect("Dialog offen");
    assert_eq!(w.zweck, Zweck::Muster);
    // … und er ist als Fenster da, mit seinen Knöpfen.
    assert!(
        fenster_da(&h, "Muster öffnen"),
        "Fenster „Muster öffnen“ nicht im AccessKit-Baum"
    );
    assert!(h.query_by_label("Datei öffnen").is_some());
    assert!(h.query_by_label("Abbrechen").is_some());
    // Ein Tastendruck allein lädt nichts und wählt nichts.
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse3_v2.sm2d"));
    assert_eq!(h.state().gewaehlt, None);

    // Ein zweiter Druck: der Dialog bleibt offen und brauchbar, das Muster bleibt geladen.
    h.state_mut().dateiwahl.as_mut().unwrap().pfad_text = "getippt.sm2d".into();
    taste(&mut h, Key::O, Modifiers::COMMAND);
    let w = h
        .state()
        .dateiwahl
        .as_ref()
        .expect("Dialog nach dem 2. Druck");
    assert_eq!(w.zweck, Zweck::Muster);
    assert!(fenster_da(&h, "Muster öffnen"));
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse3_v2.sm2d"));
    assert_eq!(h.state().muster.as_ref().unwrap().teile.len(), 28);
    // Befund: der zweite Druck legt den Dialog neu an – der eingetippte Pfad ist dann weg.
    assert_eq!(
        w.pfad_text,
        w.liste.ordner.display().to_string(),
        "Pfadfeld nach dem 2. Strg+O (neu angelegter Dialog behält Eingaben?)"
    );

    // Der Dialog lässt sich danach noch bedienen: Abbrechen schliesst ihn.
    h.get_by_label("Abbrechen").click();
    h.run();
    assert!(h.state().dateiwahl.is_none());
    assert!(!fenster_da(&h, "Muster öffnen"));
}

#[test]
fn strg_ohne_command_ist_kein_kuerzel() {
    // Dokumentiert egui, nicht die App: `consume_shortcut` vergleicht über `command`.
    // Reines `Modifiers::CTRL` (ctrl: true, command: false) trifft Strg+O deshalb nicht –
    // winit setzt auf Windows beides, ein Test muss also COMMAND schicken.
    assert!(
        !Modifiers::CTRL.matches_logically(Modifiers::COMMAND),
        "egui-Verhalten geändert: CTRL trifft jetzt COMMAND"
    );
    assert!(Modifiers::CTRL
        .plus(Modifiers::COMMAND)
        .matches_logically(Modifiers::COMMAND));

    let mut h = mit_bluse();
    taste(&mut h, Key::O, Modifiers::CTRL);
    assert!(
        h.state().dateiwahl.is_none(),
        "reines CTRL hat den Dialog geöffnet (dann vergleicht egui anders als angenommen)"
    );
    // Mit beiden Bits, wie winit es schickt, wirkt es.
    taste(&mut h, Key::O, Modifiers::CTRL.plus(Modifiers::COMMAND));
    assert_eq!(
        h.state().dateiwahl.as_ref().map(|d| d.zweck),
        Some(Zweck::Muster)
    );
}

// ───────────────────────────────────────────────────────────────── 2. Escape

#[test]
fn escape_schliesst_den_dialog_und_laesst_das_muster() {
    let mut h = mit_bluse();
    taste(&mut h, Key::O, Modifiers::COMMAND);
    assert!(fenster_da(&h, "Muster öffnen"));

    taste(&mut h, Key::Escape, Modifiers::NONE);

    assert!(h.state().dateiwahl.is_none(), "Dialog noch offen");
    assert!(!fenster_da(&h, "Muster öffnen"));
    assert!(h.query_by_label("Datei öffnen").is_none());
    // Das Muster bleibt geladen, samt Auswahl- und Eigenschaften-Panel.
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse3_v2.sm2d"));
    assert_eq!(h.state().muster.as_ref().unwrap().teile.len(), 28);
    assert!(h.query_by_label("Teile (28)").is_some());
    assert!(h.query_by_label("2D-Ansicht").is_some());

    // Weitere Escape-Tasten stören nicht: das Muster bleibt, nur die Auswahl wird leer.
    taste(&mut h, Key::Escape, Modifiers::NONE);
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse3_v2.sm2d"));
    assert_eq!(h.state().gewaehlt, None);
}

// ───────────────────────────────────────────────────────────────── 3. Tab

#[test]
fn tab_waehlt_jedes_teil_und_fuellt_die_eigenschaften() {
    let mut h = mit_bluse();
    let teile: Vec<String> = h
        .state()
        .muster
        .as_ref()
        .unwrap()
        .teile
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(teile.len(), 28, "Bluse3 v2 hat 28 Teile");
    assert_eq!(h.state().gewaehlt, None);
    assert!(h.query_by_label_contains("Kein Teil gewählt").is_some());

    // So oft Tab, wie es Teile gibt: 0, 1, … 27, und dann wieder von vorn.
    for runde in 0..2 {
        for i in 0..teile.len() {
            taste(&mut h, Key::Tab, Modifiers::NONE);
            assert_eq!(
                h.state().gewaehlt,
                Some(i),
                "Tab in Runde {runde} blieb bei {:?} statt {i}",
                h.state().gewaehlt
            );
            // Das Eigenschaften-Panel zieht mit: Zeile „Name“ | voller Name.
            let z = zeile_nach(&h, Role::Label, "Name", 1);
            assert_eq!(z[0], teile[i], "Eigenschaften nach Tab auf Teil {i}");
            assert!(h.query_by_label("Kein Teil gewählt").is_none());
        }
    }

    // Nach der zweiten Runde steht Tab auf dem letzten Teil; Umschalt+Tab geht rückwärts …
    assert_eq!(h.state().gewaehlt, Some(teile.len() - 1));
    taste(&mut h, Key::Tab, Modifiers::SHIFT);
    assert_eq!(h.state().gewaehlt, Some(teile.len() - 2));
    assert_eq!(
        zeile_nach(&h, Role::Label, "Name", 1)[0],
        teile[teile.len() - 2]
    );
    // … und über den Anfang hinaus wieder auf das letzte.
    for _ in 0..teile.len() {
        taste(&mut h, Key::Tab, Modifiers::SHIFT);
    }
    assert_eq!(h.state().gewaehlt, Some(teile.len() - 2));
}

// ───────────────────────────────────────────────────────────────── 4. F und L

#[test]
fn f_und_l_schalten_die_ansicht_um() {
    let mut h = mit_bluse();
    assert!(h.state().ansicht.ausgelegt, "Auslage ist die Vorgabe");
    let eingepasst = h.state().ansicht.sicht.expect("Ansicht eingepasst").zoom;

    // L: Auslage ↔ Konstruktionslage, der Zustand selbst ändert sich.
    taste(&mut h, Key::L, Modifiers::NONE);
    assert!(
        !h.state().ansicht.ausgelegt,
        "L hat nicht auf Konstruktionslage geschaltet"
    );
    taste(&mut h, Key::L, Modifiers::NONE);
    assert!(
        h.state().ansicht.ausgelegt,
        "L hat nicht zurück auf Auslage geschaltet"
    );
    // Die Ansicht wird dabei neu eingepasst (Zoom der eingepassten Lage).
    let s = h.state().ansicht.sicht.unwrap();
    assert!(
        (s.zoom - h.state().ansicht.einpass_zoom).abs() < 1e-9 * s.zoom,
        "nach dem Lagwechsel nicht eingepasst"
    );

    // F: nach dem Zoomen wieder einpassen.
    h.state_mut().ansicht.sicht.as_mut().unwrap().zoom *= 3.0;
    h.run();
    let gezoomt = h.state().ansicht.sicht.unwrap().zoom;
    assert!(gezoomt > eingepasst * 2.0, "Zoom wurde nicht gesetzt");
    taste(&mut h, Key::F, Modifiers::NONE);
    let s = h.state().ansicht.sicht.unwrap();
    assert!(
        (s.zoom - h.state().ansicht.einpass_zoom).abs() < 1e-9 * s.zoom,
        "F hat nicht eingepasst: zoom {} statt einpass_zoom {}",
        s.zoom,
        h.state().ansicht.einpass_zoom
    );
    assert!(
        (s.zoom - eingepasst).abs() < 1e-6 * eingepasst,
        "F ergab einen anderen Zoom als beim Laden"
    );
}

// ───────────────────────────────────────────────────────────────── 5. Strg+Q

#[test]
fn strg_q_fordert_das_schliessen_an() {
    let mut h = mit_bluse();
    assert!(!h.state().beenden_angefordert);

    let close = druck_ein_frame(&mut h, Key::Q, Modifiers::COMMAND);
    h.run();

    // Die App setzt ihre eigene Marke (das ist die App, nicht egui) …
    assert!(
        h.state().beenden_angefordert,
        "App hat Strg+Q nicht verarbeitet"
    );
    // … und der Frame enthielt den Schliessen-Befehl an das Fenster.
    assert!(close, "kein ViewportCommand::Close im Frame des Strg+Q");
    // Nur eine Anfrage: die App läuft weiter, das Muster bleibt geladen.
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse3_v2.sm2d"));
    assert_eq!(h.state().muster.as_ref().unwrap().teile.len(), 28);
    assert!(h.query_by_label("Teile (28)").is_some());
}

#[test]
fn ohne_strg_q_kein_schliessen() {
    // Gegenprobe: ein gewöhnliches Q schickt keinen Schließen-Befehl.
    let mut h = mit_bluse();
    let close = druck_ein_frame(&mut h, Key::Q, Modifiers::NONE);
    h.run();
    assert!(!close, "Q allein schickt ViewportCommand::Close");
    assert!(!h.state().beenden_angefordert);
}
