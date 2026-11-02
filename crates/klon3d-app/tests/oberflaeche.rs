//! UI tests of the window, headless with egui_kittest: no GPU and no real window needed.
//!
//! The window is driven by its labels (AccessKit), as a user would: menus are clicked, keys are
//! pressed, files are dropped. The numbers it shows are compared with an independent evaluation
//! through `klon3d-pattern` – the calls `klon3d naehte` makes – not with the app's own data.
//! The patterns are the test patterns of `klon3d-pattern` in this repository.

mod gemeinsam;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;
use egui::{Key, Modifiers, Role, Theme};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use gemeinsam::*;
use klon3d_app::app::{App, SCHLUESSEL_ORDNER, SCHLUESSEL_THEMA, SPAETER};
use klon3d_app::dateiwahl::Zweck;
use klon3d_app::muster::Ladeauftrag;
use klon3d_pattern::geom::{lies_messungen, Auswertung};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::read_path;

/// The nine menus of the menu bar, in their order.
const MENUES: [&str; 9] = [
    "Datei",
    "Bearbeiten",
    "3D-Kleidung",
    "2D-Schnitt",
    "Materialien",
    "Avatar",
    "Anzeige",
    "Einstellungen",
    "Hilfe",
];

#[test]
fn teileliste_zeigt_jedes_teil_mit_umriss() {
    let h = mit_bluse();
    let soll = soll_bluse();
    assert_eq!(soll.teile.len(), 28, "Bluse3 v2 hat 28 Teile mit Umriss");
    assert!(h
        .query_by_label(&format!("Teile ({})", soll.teile.len()))
        .is_some());
    for (name, umfang, _) in &soll.teile {
        let kurz = kurzname(name);
        let z = zeile(&h, kurz, 2);
        assert_eq!(z[0], format!("{umfang:.4}"), "Umfang von {name}");
    }
    assert_eq!(
        h.state().muster.as_ref().unwrap().teile.len(),
        soll.teile.len()
    );
}

#[test]
fn nahtliste_zeigt_jede_naht_wie_klon3d_naehte() {
    let mut h = mit_bluse();
    let soll = soll_bluse();
    // The seams are in their own tab of the object browser.
    assert!(h.query_by_label("keine Kante in zwei Nähten").is_none());
    h.get_by_role_and_label(Role::Tab, "Nähte").click();
    h.run();
    assert_eq!(soll.naehte.len(), 24);
    for (kennung, la, lb, d) in &soll.naehte {
        // Kennung | Δ | L_A | L_B | Bezeichnung
        let z = zeile(&h, kennung, 4);
        assert_eq!(z[0], delta_wie_cli(*d), "Δ von {kennung}");
        assert_eq!(z[1], format!("{la:.4}"), "L_A von {kennung}");
        assert_eq!(z[2], format!("{lb:.4}"), "L_B von {kennung}");
    }
    // The list uses no edge twice; the status bar names the largest deviation like the CLI.
    assert!(h.query_by_label("keine Kante in zwei Nähten").is_some());
    let (k, d) = soll
        .naehte
        .iter()
        .max_by(|x, y| x.3.abs().total_cmp(&y.3.abs()))
        .map(|n| (n.0.clone(), n.3))
        .unwrap();
    let text = format!("größte Abweichung {k} Δ {} cm", delta_wie_cli(d));
    assert!(h.query_by_label(&text).is_some(), "{text}");
}

#[test]
fn klick_in_der_liste_fuellt_die_eigenschaften() {
    let mut h = mit_bluse();
    let soll = soll_bluse();
    let (i, (name, umfang, kanten)) = soll
        .teile
        .iter()
        .enumerate()
        .find(|(_, t)| t.0.ends_with(" - Band Mitte"))
        .unwrap();
    // Nothing is selected after loading: the panel asks for a choice and shows no edges.
    assert_eq!(h.state().gewaehlt, None);
    assert!(h.query_by_label_contains("Kein Teil gewählt").is_some());
    assert!(h
        .query_by_label(&format!("Randkanten ({})", kanten.len()))
        .is_none());
    h.get_by_role_and_label(Role::Button, "Band Mitte").click();
    h.run();
    assert_eq!(h.state().gewaehlt, Some(i));
    assert!(h.query_by_label(name).is_some(), "voller Name {name}");
    assert!(h.query_by_label(&format!("{umfang:.4} cm")).is_some());
    assert!(h
        .query_by_label(&format!("Randkanten ({})", kanten.len()))
        .is_some());
    for (kante, laenge) in kanten {
        let z = zeile_nach(&h, Role::Label, kante, 1);
        assert_eq!(z[0], format!("{laenge:.4}"), "Länge von {kante}");
    }
}

#[test]
fn klick_in_die_ansicht_waehlt_ein_teil() {
    let mut h = mit_bluse();
    let flaeche = h.get_by_label("2D-Ansicht").rect();
    let (i, punkt) = {
        let app = h.state();
        let m = app.muster.as_ref().unwrap();
        let i = m.teile.iter().position(|t| t.kurz == "Band Mitte").unwrap();
        let t = &m.teile[i];
        let v = t.versatz(app.ansicht.ausgelegt);
        let sicht = app.ansicht.sicht.unwrap();
        let (x, y) = sicht.zu_bild(klon3d_pattern::Pt::new(
            t.etikett.x + v.x,
            t.etikett.y + v.y,
        ));
        (i, flaeche.min + egui::vec2(x as f32, y as f32))
    };
    assert!(
        flaeche.contains(punkt),
        "Etikett von Band Mitte liegt in der Ansicht"
    );
    klick_bei(&mut h, punkt);
    assert_eq!(h.state().gewaehlt, Some(i));
    // A click on empty space clears the selection.
    klick_bei(&mut h, flaeche.min + egui::vec2(3.0, 3.0));
    assert_eq!(h.state().gewaehlt, None);
}

#[test]
fn design_umschalten_und_behalten() {
    let mut h = fenster();
    h.run();
    assert_eq!(h.state().thema, Theme::Dark, "dunkel ist die Vorgabe");
    assert!(h.ctx.global_style().visuals.dark_mode);

    // The hover text names the design the button switches to.
    h.ctx
        .global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
    let hinweis = |h: &mut Harness<'_, App>, text: &str| {
        h.get_by_label("Hell/Dunkel").hover();
        h.run();
        assert!(
            h.query_all_by_role(Role::Tooltip)
                .any(|t| t.query_by_label(text).is_some()),
            "Hinweis „{text}“ fehlt"
        );
    };
    hinweis(&mut h, "zum hellen Design wechseln");
    h.get_by_label("Hell/Dunkel").click();
    h.run();
    assert_eq!(h.state().thema, Theme::Light);
    hinweis(&mut h, "zum dunklen Design wechseln");
    assert_eq!(h.ctx.theme(), Theme::Light);
    assert!(!h.ctx.global_style().visuals.dark_mode);

    // Saved and read back by a new window from the same storage.
    let mut speicher = Speicher::default();
    eframe::App::save(h.state_mut(), &mut speicher);
    assert_eq!(
        speicher.0.get(SCHLUESSEL_THEMA).map(String::as_str),
        Some("hell")
    );
    drop(h);
    let mut h2 = Harness::builder()
        .with_size([1600.0, 1400.0])
        .build_eframe(|cc| {
            cc.storage = Some(&speicher);
            App::neu(cc)
        });
    h2.run();
    assert_eq!(h2.state().thema, Theme::Light);
    assert!(!h2.ctx.global_style().visuals.dark_mode);

    // Back to dark through Einstellungen › Design.
    menue_waehlen(&mut h2, &["Einstellungen", "Design"], "Dunkel");
    assert_eq!(h2.state().thema, Theme::Dark);
    assert!(h2.ctx.global_style().visuals.dark_mode);
}

#[test]
fn menues_und_gesperrte_eintraege() {
    let mut h = mit_bluse();
    // Tooltips at once, so hovering an entry shows its hover text in the next frame.
    h.ctx
        .global_style_mut(|s| s.interaction.tooltip_delay = 0.0);

    // The nine menus, left to right in one row.
    let knoepfe: Vec<egui::Rect> = MENUES.iter().map(|m| in_der_leiste(&h, m).rect()).collect();
    for (paar, namen) in knoepfe.windows(2).zip(MENUES.windows(2)) {
        assert!(
            paar[0].right() <= paar[1].left() && (paar[0].top() - paar[1].top()).abs() < 1.0,
            "{} vor {}",
            namen[0],
            namen[1]
        );
    }

    // Every menu and submenu with its enabled and its disabled entries (a pattern is loaded).
    type Menue<'a> = (&'a [&'a str], &'a [&'a str], &'a [&'a str]);
    let menues: [Menue; 22] = [
        (
            &["Datei"],
            &[
                "Öffnen… Strg+O",
                "Zuletzt geöffnet",
                "Hinzufügen",
                "Import",
                "Export",
                "Informationen…",
                "Beenden Strg+Q",
            ],
            &[
                "Neu Strg+N",
                "Speichern Strg+S",
                "Speichern unter… Strg+Umschalt+S",
            ],
        ),
        (&["Datei", "Hinzufügen"], &["Maßdatei…", "Nahtliste…"], &[]),
        (
            &["Datei", "Import"],
            &["Seamly2D-Muster (.sm2d)…"],
            &["CLO-Projekt (.zprj)…"],
        ),
        (&["Datei", "Export"], &[], &["OBJ…", "DXF…"]),
        (
            &["Bearbeiten"],
            &["Auswahl aufheben Esc"],
            &[
                "Rückgängig Strg+Z",
                "Wiederholen Strg+Y",
                "Alles auswählen Strg+A",
            ],
        ),
        (
            &["3D-Kleidung"],
            &[],
            &[
                "Simulation starten/anhalten",
                "Einzelschritt",
                "Zurücksetzen",
                "Anordnen",
                "Netzauflösung…",
                "Netzprüfung",
            ],
        ),
        (
            &["2D-Schnitt"],
            &[
                "Auslage / Konstruktionslage L",
                "Einpassen F",
                "Nächstes Teil Tab",
                "Vorheriges Teil Umschalt+Tab",
                "Nähen",
            ],
            &[],
        ),
        (
            &["2D-Schnitt", "Nähen"],
            &[
                "Nahtliste wählen…",
                "Nahtlängen prüfen",
                "Nähte anzeigen Umschalt+S",
            ],
            &[],
        ),
        (&["Materialien"], &["Stoffbibliothek"], &["Stoff zuweisen…"]),
        (
            &["Avatar"],
            &["Maßdatei wählen…"],
            &[
                "Körper laden… Strg+Umschalt+A",
                "Avatar anzeigen Umschalt+A",
                "Maße am Körper",
            ],
        ),
        (
            &["Anzeige"],
            &[
                "Blickwinkel",
                "3D-Kleidung",
                "2D-Schnitt",
                "Avatar",
                "Fenster",
                "Werkzeugleisten",
                "Anordnung zurücksetzen",
            ],
            &[],
        ),
        (
            &["Anzeige", "Blickwinkel"],
            &[],
            &["Vorne", "Hinten", "Links", "Rechts", "Oben"],
        ),
        (
            &["Anzeige", "3D-Kleidung"],
            &[],
            &[
                "Netz",
                "Dehnungskarte Alt+7",
                "Passformkarte Alt+8",
                "Abstandskarte",
            ],
        ),
        (
            &["Anzeige", "2D-Schnitt"],
            &[
                "Nähte Umschalt+S",
                "Längen Umschalt+Z",
                "Teilnamen",
                "Maßstab",
            ],
            &[],
        ),
        (&["Anzeige", "Avatar"], &[], &["Avatar"]),
        (
            &["Anzeige", "Fenster"],
            &[
                "Bibliothek",
                "Objekt-Browser",
                "Eigenschaften",
                "3D-Fenster",
                "2D-Fenster",
                "Fenster wechseln Strg+Tab",
            ],
            &[],
        ),
        (
            &["Anzeige", "Werkzeugleisten"],
            &[
                "Datei-Werkzeugleiste",
                "3D-Werkzeugleiste",
                "2D-Werkzeugleiste",
            ],
            &[],
        ),
        (&["Einstellungen"], &["Design", "Sprache"], &[]),
        (&["Einstellungen", "Design"], &["Dunkel", "Hell"], &[]),
        (&["Einstellungen", "Sprache"], &["Deutsch"], &["English"]),
        (&["Hilfe"], &["Tastenkürzel", "Über klon3d"], &[]),
        (
            &["Datei", "Zuletzt geöffnet"],
            &["bluse3_v2.sm2d", "Liste leeren"],
            &[],
        ),
    ];
    for (pfad, frei, gesperrt) in menues {
        let wo = pfad.join(" › ");
        menue_oeffnen(&mut h, pfad);
        for e in frei {
            let k = im_menue(&h, e);
            assert!(!k.accesskit_node().is_disabled(), "{wo} › {e} gesperrt");
        }
        for e in gesperrt {
            let k = im_menue(&h, e);
            assert!(
                k.accesskit_node().is_disabled(),
                "{wo} › {e} nicht gesperrt"
            );
            // Hovering a disabled entry says why.
            k.hover();
            h.run();
            let hinweis = h
                .query_all_by_role(Role::Tooltip)
                .any(|t| t.query_by_label(SPAETER).is_some());
            assert!(hinweis, "{wo} › {e}: kein Hinweis „{SPAETER}“");
        }
        // Close the menu again: none of its entries is left.
        menue_schliessen(&mut h, pfad[0]);
        for e in gesperrt.iter().chain(frei.iter()) {
            let noch_da = h
                .query_all_by_label(e)
                .any(|n| in_einem_menue(&n) && n.rect().top() > knoepfe[0].bottom());
            assert!(!noch_da, "{wo} › {e} nach dem Schliessen");
        }
    }

    // An enabled entry acts: Hilfe › Tastenkürzel opens the help window.
    menue_waehlen(&mut h, &["Hilfe"], "Tastenkürzel");
    assert!(h.state().hilfe.is_some());
    h.get_by_label("Hilfe schließen").click();
    h.run();
    assert!(h.state().hilfe.is_none());
}

#[test]
fn tastenkuerzel_und_textfeld() {
    let mut h = mit_bluse();
    let n = h.state().muster.as_ref().unwrap().teile.len();
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(h.state().gewaehlt, Some(0));
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(h.state().gewaehlt, Some(1));
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run();
    assert_eq!(h.state().gewaehlt, Some(n - 1));
    h.key_press(Key::Escape);
    h.run();
    assert_eq!(h.state().gewaehlt, None);

    assert!(h.state().ansicht.ausgelegt);
    h.key_press(Key::L);
    h.run();
    assert!(
        !h.state().ansicht.ausgelegt,
        "L schaltet zur Konstruktionslage"
    );

    // F fits again after zooming in.
    let eingepasst = h.state().ansicht.sicht.unwrap();
    h.state_mut().ansicht.sicht.as_mut().unwrap().zoom *= 3.0;
    h.run();
    h.key_press(Key::F);
    h.run();
    let s = h.state().ansicht.sicht.unwrap();
    assert!((s.zoom - eingepasst.zoom).abs() < 1e-9 * eingepasst.zoom);

    // With the search field focused, letters are text, not shortcuts.
    h.get_by_label("Suchen:").focus();
    h.run();
    h.key_press(Key::L);
    h.key_press(Key::F);
    h.get_by_label("Suchen:").type_text("Band");
    h.run();
    assert!(
        !h.state().ansicht.ausgelegt,
        "L darf im Textfeld nicht wirken"
    );
    assert_eq!(h.state().suche, "Band");
    assert!(
        h.query_by_label("Rueckenteil Mitte Links").is_none(),
        "Suche filtert"
    );
    assert!(h
        .query_by_role_and_label(Role::Button, "Band Mitte")
        .is_some());

    // Ctrl+O opens the file dialog, Esc closes it.
    h.key_press(Key::Escape);
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    h.run();
    assert_eq!(
        h.state().dateiwahl.as_ref().map(|d| d.zweck),
        Some(Zweck::Muster)
    );
    h.key_press(Key::Escape);
    h.run();
    assert!(h.state().dateiwahl.is_none());

    // Ctrl+Q asks the window to close.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
    h.run();
    assert!(h.state().beenden_angefordert);
}

#[test]
fn oeffnen_ueber_den_dialog() {
    let mut h = fenster();
    h.run();
    h.get_by_label("Öffnen").click();
    h.run();
    assert!(h.state().dateiwahl.is_some());
    // A path that does not exist gives a message in the dialog, nothing is loaded.
    h.state_mut().dateiwahl.as_mut().unwrap().pfad_text =
        muster_ordner().join("fehlt.sm2d").display().to_string();
    h.get_by_label("Datei öffnen").click();
    h.run();
    assert!(h.state().muster.is_none());
    assert!(h.query_by_label_contains("Datei nicht gefunden").is_some());
    // The real pattern loads.
    h.state_mut().dateiwahl.as_mut().unwrap().pfad_text = bluse().display().to_string();
    h.get_by_label("Datei öffnen").click();
    h.run();
    assert!(h.state().dateiwahl.is_none());
    assert_eq!(h.state().muster.as_ref().map(|m| m.teile.len()), Some(28));
}

#[test]
fn nicht_vorhandene_datei_gibt_meldung_statt_panik() {
    let mut h = fenster();
    h.run();
    let weg = muster_ordner().join("gibt_es_nicht.sm2d");
    assert!(!h.state_mut().muster_laden(&weg));
    h.run();
    assert!(h.state().muster.is_none());
    assert!(h.state().meldung_fehler);
    let f = h.state().fehlerfenster.clone().expect("Fehlerfenster");
    assert!(f.contains("gibt_es_nicht.sm2d"), "{f}");
    // Status bar and error window both show it.
    assert!(h.query_all_by_label_contains("gibt_es_nicht.sm2d").count() >= 2);
    // A missing pattern is not cured by a measurement file: nothing is offered.
    assert!(h.state().wartend.is_none());
    assert!(h
        .query_by_role_and_label(Role::Button, "Maßdatei wählen…")
        .is_none());
    h.get_by_label("Fehler schließen").click();
    h.run();
    assert!(h.state().fehlerfenster.is_none());
    assert!(h.state().meldung.contains("gibt_es_nicht.sm2d"));

    // Choosing a missing measurement file keeps the loaded pattern.
    assert!(h.state_mut().muster_laden(bluse()));
    let mut auftrag = h.state().auftrag.clone().unwrap();
    auftrag.massdatei = Some(muster_ordner().join("fehlt.smis"));
    assert!(!h.state_mut().laden(auftrag));
    h.run();
    assert!(h.state().muster.is_some());
    assert!(h
        .state()
        .fehlerfenster
        .as_deref()
        .unwrap()
        .starts_with("Maßdatei fehlt"));
}

/// A file dropped onto the window.
#[derive(Debug)]
struct Abgelegt(PathBuf);

impl egui::DroppedFile for Abgelegt {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn muster_ablegen_oeffnet_es() {
    let mut h = fenster();
    h.run();
    h.input_mut()
        .dropped_files
        .push(Arc::new(Abgelegt(bluse())));
    h.run();
    assert_eq!(h.state().muster.as_ref().map(|m| m.teile.len()), Some(28));
    // A file of another kind is refused for its kind – not handed to a loader that fails on
    // it – and the pattern shown stays.
    let vorher = h
        .state()
        .muster
        .as_ref()
        .map(|m| (m.pfad.clone(), m.teile.len()));
    h.input_mut()
        .dropped_files
        .push(Arc::new(Abgelegt(muster_ordner().join("mod.rs"))));
    h.run();
    let f = h.state().fehlerfenster.clone().expect("Fehlerfenster");
    assert!(
        f.contains("mod.rs: kein Muster (.sm2d), keine Maßdatei (.smis)"),
        "{f}"
    );
    let nachher = h
        .state()
        .muster
        .as_ref()
        .map(|m| (m.pfad.clone(), m.teile.len()));
    assert_eq!(nachher, vorher);
}

/// Drop `dateien` onto the window in one frame, as a file manager does with a selection.
fn ablegen(h: &mut Harness<'_, App>, dateien: &[PathBuf]) {
    for d in dateien {
        h.input_mut()
            .dropped_files
            .push(Arc::new(Abgelegt(d.clone())));
    }
    h.run();
}

/// A copy of bluse1_v25.sm2d alone in `ordner`, without the measurement file it names.
fn bluse1_ohne_massdatei(ordner: &Path) -> PathBuf {
    let kopie = ordner.join("bluse1_v25.sm2d");
    std::fs::copy(muster_ordner().join("bluse1_v25.sm2d"), &kopie).unwrap();
    kopie
}

/// Choose `datei` in the open file dialog through its path field.
fn im_dialog_waehlen(h: &mut Harness<'_, App>, datei: &Path, knopf: &str) {
    h.state_mut()
        .dateiwahl
        .as_mut()
        .expect("Dialog offen")
        .pfad_text = datei.display().to_string();
    h.get_by_label(knopf).click();
    h.run();
}

#[test]
fn muster_ohne_massdatei_laedt_mit_gewaehlter() {
    let ordner = frischer_ordner("ohne-mass");
    let kopie = bluse1_ohne_massdatei(&ordner);
    let mass = muster_ordner().join("measurements.smis");

    // Nothing open: the load fails, and Avatar › Maßdatei wählen… applies to the failed pattern.
    let mut h = fenster();
    h.run();
    assert!(!h.state_mut().muster_laden(&kopie));
    h.run();
    let f = h.state().fehlerfenster.clone().expect("Fehlerfenster");
    assert!(f.starts_with("Maßdatei fehlt"), "{f}");
    assert!(f.contains("bluse1_v25.sm2d"), "{f}");
    h.get_by_label("Fehler schließen").click();
    h.run();
    menue_oeffnen(&mut h, &["Avatar"]);
    let eintrag = im_menue(&h, "Maßdatei wählen…");
    assert!(!eintrag.accesskit_node().is_disabled(), "Eintrag gesperrt");
    eintrag.click();
    h.run();
    let w = h.state().dateiwahl.as_ref().expect("Dialog");
    assert_eq!(w.zweck, Zweck::Massdatei);
    assert!(
        w.liste.ordner.ends_with(ordner.file_name().unwrap()),
        "Dialog im Ordner des Musters: {}",
        w.liste.ordner.display()
    );
    im_dialog_waehlen(&mut h, &mass, "Datei wählen");
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse1_v25.sm2d"));
    let app = h.state();
    let m = app.muster.as_ref().unwrap();
    assert_eq!(m.pfad, std::path::absolute(&kopie).unwrap());
    assert!(m.massdatei_von_hand);
    assert_eq!(m.teile.len(), 23);
    assert!(app.wartend.is_none() && app.fehlerfenster.is_none());
    assert!(
        app.meldung
            .starts_with("bluse1_v25.sm2d geladen mit measurements.smis: 23 Teile"),
        "{}",
        app.meldung
    );
    assert!(h
        .query_by_label("Maße: measurements.smis · von Hand gewählt")
        .is_some());

    // Another pattern open: the error window offers the choice for the failed pattern, and the
    // pattern shown meanwhile stays.
    let mut h = mit_bluse();
    assert!(h
        .query_by_label("Maße: measurements.smis · vom Muster genannt")
        .is_some());
    assert!(!h.state_mut().muster_laden(&kopie));
    h.run();
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse3_v2.sm2d"));
    assert!(h
        .query_by_label_contains("bluse1_v25.sm2d lädt, sobald")
        .is_some());
    h.get_by_role_and_label(Role::Button, "Maßdatei wählen…")
        .click();
    h.run();
    assert!(h.state().fehlerfenster.is_none());
    let w = h.state().dateiwahl.as_ref().expect("Dialog");
    assert_eq!(w.zweck, Zweck::Massdatei);
    assert!(w.liste.ordner.ends_with(ordner.file_name().unwrap()));
    im_dialog_waehlen(&mut h, &mass, "Datei wählen");
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse1_v25.sm2d"));
    assert_eq!(h.state().muster.as_ref().unwrap().teile.len(), 23);

    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn muster_und_massdatei_zusammen_ablegen() {
    let ordner = frischer_ordner("ablegen");
    let kopie = bluse1_ohne_massdatei(&ordner);
    let andere = ordner.join("andere.smis");
    std::fs::copy(muster_ordner().join("measurements.smis"), &andere).unwrap();

    // One drop is one request, whatever order the file manager gives – also for a pattern
    // whose own measurement file is there.
    for muster in [kopie.clone(), bluse()] {
        for umgekehrt in [false, true] {
            let mut h = mit_bluse1_oder_leer(muster == bluse());
            let mut dateien = vec![muster.clone(), andere.clone()];
            if umgekehrt {
                dateien.reverse();
            }
            ablegen(&mut h, &dateien);
            let app = h.state();
            let m = app.muster.as_ref().expect("geladen");
            assert_eq!(m.pfad.file_name(), muster.file_name(), "{dateien:?}");
            assert_eq!(m.massdatei_name(), "andere.smis", "{dateien:?}");
            assert!(app.fehlerfenster.is_none(), "{:?}", app.fehlerfenster);
            assert!(
                app.meldung.contains(" geladen mit andere.smis: "),
                "{}",
                app.meldung
            );
        }
    }

    // A measurement file dropped alone goes to the pattern that waits for one …
    let mut h = mit_bluse();
    assert!(!h.state_mut().muster_laden(&kopie));
    h.run();
    ablegen(&mut h, std::slice::from_ref(&andere));
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse1_v25.sm2d"));
    assert!(h.state().fehlerfenster.is_none());
    // … and otherwise to the pattern shown.
    ablegen(&mut h, &[muster_ordner().join("measurements.smis")]);
    let m = h.state().muster.as_ref().unwrap();
    assert_eq!(m.dateiname(), "bluse1_v25.sm2d");
    assert_eq!(m.massdatei_name(), "measurements.smis");
    assert!(m.massdatei_von_hand);

    // Two patterns at once are refused; the pattern shown stays.
    ablegen(&mut h, &[bluse(), kopie.clone()]);
    let f = h.state().fehlerfenster.clone().unwrap_or_default();
    assert!(f.starts_with("Mehrere Muster"), "{f}");
    assert_eq!(gezeigt(&h).as_deref(), Some("bluse1_v25.sm2d"));

    let _ = std::fs::remove_dir_all(&ordner);
}

/// A window with Bluse1 v25 loaded, or an empty one.
fn mit_bluse1_oder_leer(bluse1: bool) -> Harness<'static, App> {
    let mut h = fenster();
    if bluse1 {
        assert!(h
            .state_mut()
            .muster_laden(muster_ordner().join("bluse1_v25.sm2d")));
    }
    h.run();
    h
}

#[test]
fn relativer_pfad_wird_absolut_gespeichert() {
    // cargo runs the tests in the crate folder; a relative path as from the command line.
    let relativ = Path::new("../klon3d-pattern/tests/muster/bluse3_v2.sm2d");
    assert!(relativ.is_file(), "Test läuft nicht im Crate-Ordner");
    let mut h = fenster();
    h.run();
    assert!(h.state_mut().muster_laden(relativ));
    h.run();
    assert!(h.state().auftrag.as_ref().unwrap().muster.is_absolute());
    assert!(h.state().letzter_ordner.is_absolute());
    let mut speicher = Speicher::default();
    eframe::App::save(h.state_mut(), &mut speicher);
    let ordner = PathBuf::from(
        speicher
            .0
            .get(SCHLUESSEL_ORDNER)
            .expect("Ordner gespeichert"),
    );
    assert!(ordner.is_absolute(), "{}", ordner.display());
    assert!(ordner.join("bluse3_v2.sm2d").is_file());
}

#[test]
fn ohne_muster_sind_musterbefehle_gesperrt() {
    let mut h = fenster();
    h.run();
    let menues: [(&[&str], &[&str]); 5] = [
        (&["Datei"], &["Informationen…"]),
        (&["Datei", "Hinzufügen"], &["Maßdatei…", "Nahtliste…"]),
        (&["Avatar"], &["Maßdatei wählen…"]),
        (
            &["2D-Schnitt"],
            &[
                "Einpassen F",
                "Nächstes Teil Tab",
                "Vorheriges Teil Umschalt+Tab",
            ],
        ),
        (
            &["2D-Schnitt", "Nähen"],
            &["Nahtliste wählen…", "Nahtlängen prüfen"],
        ),
    ];
    for (pfad, eintraege) in menues {
        menue_oeffnen(&mut h, pfad);
        for e in eintraege {
            let k = im_menue(&h, e);
            assert!(
                k.accesskit_node().is_disabled(),
                "{} › {e} frei",
                pfad.join(" › ")
            );
        }
        menue_schliessen(&mut h, pfad[0]);
    }
    assert!(h
        .get_by_role_and_label(Role::Button, "Einpassen")
        .accesskit_node()
        .is_disabled());
}

#[test]
fn unaufloesbarer_rand_zeigt_keinen_umfang() {
    // A measurement file with every second value missing: some boundaries cannot be resolved.
    let ordner = frischer_ordner("halbe-masse");
    let text = std::fs::read_to_string(muster_ordner().join("measurements.smis")).unwrap();
    let mut n = 0;
    let halb: String = text
        .lines()
        .filter(|z| {
            let wert = z.trim_start().starts_with("<m ");
            n += wert as usize;
            !wert || n % 2 == 1
        })
        .map(|z| format!("{z}\n"))
        .collect();
    let mass = ordner.join("halb.smis");
    std::fs::write(&mass, halb).unwrap();

    // Independently: which pieces with an outline have a boundary that cannot be resolved.
    let muster = read_path(&bluse()).unwrap();
    let ev = Auswertung::new(&muster, lies_messungen(&mass, &muster.unit).unwrap());
    let (mut ohne, mut mit) = (Vec::new(), Vec::new());
    for p in muster.pieces() {
        if !ev.stueck_umriss(p).map(|u| u.len() >= 3).unwrap_or(false) {
            continue;
        }
        let name = p.name.clone().unwrap();
        match ev.teilrand(p, Kurvenquelle::Seamly) {
            Ok(r) => mit.push((name, r.in_cm(r.umfang()))),
            Err(_) => ohne.push(name),
        }
    }
    assert!(!ohne.is_empty(), "kein Teil mit unauflösbarem Rand");

    let mut h = fenster();
    h.run();
    let mut auftrag = Ladeauftrag::neu(bluse());
    auftrag.massdatei = Some(mass.clone());
    assert!(h.state_mut().laden(auftrag));
    h.run();
    // No number where `klon3d naehte` prints none; the others as before.
    for name in &ohne {
        assert_eq!(zeile(&h, kurzname(name), 2)[0], "–", "Umfang von {name}");
    }
    for (name, umfang) in &mit {
        let z = zeile(&h, kurzname(name), 2);
        assert_eq!(z[0], format!("{umfang:.4}"), "Umfang von {name}");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// A window of the minimum size `main.rs` allows.
fn kleinstes_fenster() -> Harness<'static, App> {
    Harness::builder()
        .with_size([800.0, 500.0])
        .build_eframe(|cc| App::neu(cc))
}

#[test]
fn kleinstes_fenster_laesst_der_ansicht_platz() {
    let mut h = kleinstes_fenster();
    assert!(h.state_mut().muster_laden(bluse()));
    h.run();
    let ansicht = h.get_by_label("2D-Ansicht").rect();
    assert!(ansicht.width() >= 300.0, "2D-Ansicht nur {ansicht:?}");
    // The seam list is still there, inside its panel: its summary lies in the object browser,
    // and the browser inside the window.
    h.get_by_role_and_label(Role::Tab, "Nähte").click();
    h.run();
    h.get_by_label_contains("Nähten in der Toleranz")
        .scroll_to_me();
    h.run();
    let browser = h.get_by_role_and_label(Role::Pane, "Objekt-Browser").rect();
    let summe = h.get_by_label_contains("Nähten in der Toleranz").rect();
    // Scrolled into view it shows in the browser (clipped at its edges: the line is wider than
    // the narrow panel, which scrolls sideways).
    assert!(
        browser.intersects(summe)
            && browser.y_range().contains(summe.top())
            && browser.right() <= 800.0 + 0.5,
        "Nahtliste {summe:?} im Objekt-Browser {browser:?}"
    );
    assert!(!browser.intersects(ansicht), "{browser:?} über {ansicht:?}");
}

#[test]
fn lange_meldung_gekuerzt_und_wieder_zu_zeigen() {
    let mut h = kleinstes_fenster();
    h.run();
    let weg = muster_ordner()
        .join("ein_recht_langer_ordnername_der_nicht_in_die_statusleiste_passt")
        .join("gibt_es_nicht.sm2d");
    assert!(!h.state_mut().muster_laden(&weg));
    h.run();
    let meldung = h.state().meldung.clone();
    h.get_by_label("Fehler schließen").click();
    h.run();
    // Cut at the window edge; the label still carries the whole text.
    let status = h.get_by_label(&meldung);
    assert!(
        status.rect().right() <= 800.0,
        "Meldung reicht bis {}",
        status.rect().right()
    );
    // A click shows it in the error window again.
    status.click();
    h.run();
    assert_eq!(h.state().fehlerfenster.as_deref(), Some(meldung.as_str()));
}

/// All `.sm2d` below `ordner`.
fn alle_muster(ordner: &Path, out: &mut Vec<PathBuf>) {
    let mut e: Vec<PathBuf> = std::fs::read_dir(ordner)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    e.sort();
    for p in e {
        if p.is_dir() {
            alle_muster(&p, out);
        } else if p.extension().is_some_and(|x| x == "sm2d") {
            out.push(p);
        }
    }
}

#[test]
fn jedes_pruefmuster_laedt_wie_klon3d_naehte() {
    let mut muster = Vec::new();
    alle_muster(&muster_ordner(), &mut muster);
    assert!(muster.len() >= 10, "{} Muster gefunden", muster.len());
    let (mut mit_liste, mut ohne_liste) = (0, 0);
    for p in muster {
        let mut h = fenster();
        let ok = h.state_mut().muster_laden(&p);
        h.run();
        assert!(ok, "{}: {:?}", p.display(), h.state().fehlerfenster);
        let app = h.state();
        let m = app.muster.as_ref().unwrap();
        assert!(!m.teile.is_empty(), "{}: keine Teile", p.display());

        // Independently, with the calls `klon3d naehte` makes: the measurement file the pattern
        // names (relative to the pattern) and the seam list next to it.
        let muster = read_path(&p).unwrap();
        let mass = p.parent().unwrap().join(
            muster
                .measurements
                .as_deref()
                .expect("Muster nennt Maßdatei"),
        );
        let ev = Auswertung::new(&muster, lies_messungen(&mass, &muster.unit).unwrap());
        let liste = p.with_extension("naehte");
        let soll = if liste.is_file() {
            mit_liste += 1;
            let defs = lies_nahtliste(&std::fs::read_to_string(&liste).unwrap()).unwrap();
            let bericht = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
            let n = bericht.groesste_abweichung().expect("Nahtliste ohne Nähte");
            format!(
                "größte Abweichung {} Δ {} cm",
                n.kennung,
                delta_wie_cli(n.diff_cm)
            )
        } else {
            ohne_liste += 1;
            "keine Nahtliste".to_string()
        };
        assert_eq!(app.abweichung_text(), soll, "{}", p.display());
        assert!(h.query_by_label(&soll).is_some(), "Statusleiste: {soll}");
    }
    // Both kinds are among the test patterns, so both branches above ran.
    assert!(
        mit_liste > 0 && ohne_liste > 0,
        "{mit_liste} / {ohne_liste}"
    );
}
