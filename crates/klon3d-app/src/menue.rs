//! Menu bar and the file toolbar.
//!
//! The nine menus follow the order CLO users know – Datei, Bearbeiten, 3D-Kleidung, 2D-Schnitt,
//! Materialien, Avatar, Anzeige, Einstellungen, Hilfe – with klon3d's own German labels. Entries
//! only queue an [`Aktion`]; the app runs it at the end of the frame. Entries that a later task
//! brings (saving, simulation, body, export, maps, …) are shown disabled with the hover text
//! [`SPAETER`], so the menus already have their final shape. Toggles are checkboxes, choices radio
//! buttons; a shortcut is shown on the right of its entry.

use eframe::egui;
use egui::{Atom, Button, RichText, Theme};

use crate::anordnung::{self, Bereich};
use crate::app::{Aktion, App, SPAETER};
use crate::muster::dateiname;

/// Hover text of an entry that needs a pattern.
pub const ERST_MUSTER: &str = "erst ein Muster öffnen";

/// Hover text of the fabrics entries when no fabrics folder was found.
pub const KEIN_STOFFORDNER: &str =
    "kein Ordner assets/stoffe gefunden – gesucht ab dem Programmordner und dem aktuellen Ordner aufwärts";

/// A button with the shortcut on the right (none for `""`).
fn knopf<'a>(text: &'a str, kuerzel: &'a str) -> Button<'a> {
    let b = Button::new(text);
    if kuerzel.is_empty() {
        b
    } else {
        b.shortcut_text(kuerzel)
    }
}

/// A menu entry that queues `aktion` when clicked.
fn eintrag(ui: &mut egui::Ui, app: &mut App, text: &str, kuerzel: &str, aktion: Aktion) {
    if ui.add(knopf(text, kuerzel)).clicked() {
        app.aktion(aktion);
    }
}

/// A menu entry that is enabled only with a loaded pattern.
fn eintrag_mit_muster(ui: &mut egui::Ui, app: &mut App, text: &str, kuerzel: &str, aktion: Aktion) {
    let da = app.muster.is_some();
    if ui
        .add_enabled(da, knopf(text, kuerzel))
        .on_disabled_hover_text(ERST_MUSTER)
        .clicked()
    {
        app.aktion(aktion);
    }
}

/// A menu entry that chooses a file for [`App::ladeziel`] – the pattern shown, or one that failed
/// for want of a measurement file; the hover text names it.
fn eintrag_fuer_ziel(ui: &mut egui::Ui, app: &mut App, text: &str, aktion: Aktion) {
    let ziel = app.ladeziel().map(|a| dateiname(&a.muster));
    let r = ui
        .add_enabled(ziel.is_some(), Button::new(text))
        .on_disabled_hover_text(ERST_MUSTER);
    let r = match &ziel {
        Some(z) => r.on_hover_text(format!("für {z}")),
        None => r,
    };
    if r.clicked() {
        app.aktion(aktion);
    }
}

/// A disabled entry of a later task.
pub fn spaeter(ui: &mut egui::Ui, text: &str, kuerzel: &str) {
    ui.add_enabled(false, knopf(text, kuerzel))
        .on_disabled_hover_text(SPAETER);
}

/// A checkbox entry for a toggle, the shortcut on the right; a click queues `aktion`.
fn haken(ui: &mut egui::Ui, app: &mut App, text: &str, kuerzel: &str, an: bool, aktion: Aktion) {
    let mut zustand = an;
    let r = if kuerzel.is_empty() {
        ui.checkbox(&mut zustand, text)
    } else {
        ui.checkbox(
            &mut zustand,
            (text, Atom::grow(), RichText::new(kuerzel).weak()),
        )
    };
    if r.clicked() {
        app.aktion(aktion);
    }
}

/// A checkbox entry that shows or hides part `b` of the window.
fn bereich(ui: &mut egui::Ui, app: &mut App, b: Bereich) {
    let an = app.anordnung.sichtbar(b);
    haken(ui, app, b.titel(), "", an, Aktion::BereichUmschalten(b));
}

/// The menu bar.
pub fn menues(app: &mut App, ui: &mut egui::Ui) {
    let offen = ui.menu_button("Datei", |ui| datei(app, ui)).inner.is_some();
    // Whether the recent files exist is checked once per opening, in the background.
    if offen && !app.datei_menue_offen {
        app.zuletzt_pruefung.starten(&app.zuletzt);
    }
    app.datei_menue_offen = offen;
    ui.menu_button("Bearbeiten", |ui| {
        spaeter(ui, "Rückgängig", "Strg+Z");
        spaeter(ui, "Wiederholen", "Strg+Y");
        ui.separator();
        spaeter(ui, "Alles auswählen", "Strg+A");
        eintrag(ui, app, "Auswahl aufheben", "Esc", Aktion::AuswahlAufheben);
    });
    ui.menu_button("3D-Kleidung", |ui| {
        spaeter(ui, "Simulation starten/anhalten", "");
        spaeter(ui, "Einzelschritt", "");
        spaeter(ui, "Zurücksetzen", "");
        ui.separator();
        spaeter(ui, "Anordnen", "");
        spaeter(ui, "Netzauflösung…", "");
        spaeter(ui, "Netzprüfung", "");
    });
    ui.menu_button("2D-Schnitt", |ui| schnitt(app, ui));
    ui.menu_button("Materialien", |ui| {
        spaeter(ui, "Stoff zuweisen…", "");
        let r = ui.add_enabled(app.stoffordner.is_some(), Button::new("Stoffbibliothek"));
        let r = match &app.stoffordner {
            Some(o) => r.on_hover_text(format!("Bibliothek bei {}", o.display())),
            None => r.on_disabled_hover_text(KEIN_STOFFORDNER),
        };
        if r.clicked() {
            app.aktion(Aktion::StoffbibliothekZeigen);
        }
    });
    ui.menu_button("Avatar", |ui| {
        spaeter(ui, "Körper laden…", "Strg+Umschalt+A");
        eintrag_fuer_ziel(ui, app, "Maßdatei wählen…", Aktion::MassdateiWaehlen);
        ui.separator();
        spaeter(ui, "Avatar anzeigen", "Umschalt+A");
        spaeter(ui, "Maße am Körper", "");
    });
    ui.menu_button("Anzeige", |ui| anzeige(app, ui));
    ui.menu_button("Einstellungen", |ui| {
        ui.menu_button("Design", |ui| {
            let dunkel = app.thema == Theme::Dark;
            if ui.radio(dunkel, "Dunkel").clicked() {
                app.aktion(Aktion::ThemaSetzen(Theme::Dark));
            }
            if ui.radio(!dunkel, "Hell").clicked() {
                app.aktion(Aktion::ThemaSetzen(Theme::Light));
            }
        });
        ui.menu_button("Sprache", |ui| {
            // German is the only language so far; choosing it again changes nothing.
            let _ = ui.radio(true, "Deutsch");
            ui.add_enabled(false, egui::RadioButton::new(false, "English"))
                .on_disabled_hover_text(SPAETER);
        });
    });
    ui.menu_button("Hilfe", |ui| {
        eintrag(ui, app, "Tastenkürzel", "", Aktion::TastenkuerzelZeigen);
        eintrag(ui, app, "Über klon3d", "", Aktion::UeberZeigen);
    });
}

fn datei(app: &mut App, ui: &mut egui::Ui) {
    spaeter(ui, "Neu", "Strg+N");
    eintrag(ui, app, "Öffnen…", "Strg+O", Aktion::Oeffnen);
    ui.menu_button("Zuletzt geöffnet", |ui| zuletzt(app, ui));
    ui.menu_button("Hinzufügen", |ui| {
        eintrag_fuer_ziel(ui, app, "Maßdatei…", Aktion::MassdateiWaehlen);
        eintrag_fuer_ziel(ui, app, "Nahtliste…", Aktion::NahtlisteWaehlen);
    });
    ui.separator();
    spaeter(ui, "Speichern", "Strg+S");
    spaeter(ui, "Speichern unter…", "Strg+Umschalt+S");
    ui.separator();
    ui.menu_button("Import", |ui| {
        eintrag(ui, app, "Seamly2D-Muster (.sm2d)…", "", Aktion::Oeffnen);
        spaeter(ui, "CLO-Projekt (.zprj)…", "");
    });
    ui.menu_button("Export", |ui| {
        spaeter(ui, "OBJ…", "");
        spaeter(ui, "DXF…", "");
    });
    ui.separator();
    eintrag_mit_muster(ui, app, "Informationen…", "", Aktion::InformationenZeigen);
    ui.separator();
    eintrag(ui, app, "Beenden", "Strg+Q", Aktion::Beenden);
}

/// Datei › Zuletzt geöffnet: the last patterns, newest first; a file that is gone is shown
/// disabled. The menu does not look at the files: a check in the background, started when the
/// Datei menu opens, tells which are gone ([`crate::anordnung::ZuletztPruefung`]) – a file on a
/// disconnected network drive would otherwise block every frame.
fn zuletzt(app: &mut App, ui: &mut egui::Ui) {
    if app.zuletzt.is_empty() {
        ui.weak("noch keine Muster geöffnet");
    }
    let namen = anordnung::zuletzt_beschriftungen(&app.zuletzt);
    let mut gewaehlt = None;
    for (p, name) in app.zuletzt.iter().zip(namen) {
        let da = app.zuletzt_pruefung.da(p) != Some(false);
        let r = ui
            .add_enabled(da, Button::new(name))
            .on_hover_text(p.display().to_string())
            .on_disabled_hover_text(format!("Datei fehlt: {}", p.display()));
        if r.clicked() {
            gewaehlt = Some(p.clone());
        }
    }
    if let Some(p) = gewaehlt {
        app.aktion(Aktion::MusterOeffnen(p));
    }
    ui.separator();
    if ui
        .add_enabled(!app.zuletzt.is_empty(), Button::new("Liste leeren"))
        .clicked()
    {
        app.aktion(Aktion::ZuletztLeeren);
    }
}

fn schnitt(app: &mut App, ui: &mut egui::Ui) {
    let lage = if app.ansicht.ausgelegt {
        "jetzt: Auslage – Teile nebeneinander"
    } else {
        "jetzt: Konstruktionslage"
    };
    if ui
        .add(knopf("Auslage / Konstruktionslage", "L"))
        .on_hover_text(lage)
        .clicked()
    {
        app.aktion(Aktion::LageUmschalten);
    }
    eintrag_mit_muster(ui, app, "Einpassen", "F", Aktion::Einpassen);
    ui.separator();
    eintrag_mit_muster(ui, app, "Nächstes Teil", "Tab", Aktion::NaechstesTeil);
    eintrag_mit_muster(
        ui,
        app,
        "Vorheriges Teil",
        "Umschalt+Tab",
        Aktion::VorigesTeil,
    );
    ui.separator();
    ui.menu_button("Nähen", |ui| {
        eintrag_fuer_ziel(ui, app, "Nahtliste wählen…", Aktion::NahtlisteWaehlen);
        eintrag_mit_muster(ui, app, "Nahtlängen prüfen", "", Aktion::NahtlaengenZeigen);
        let an = app.ansicht.anzeige.naehte;
        haken(
            ui,
            app,
            "Nähte anzeigen",
            "Umschalt+S",
            an,
            Aktion::NaehteUmschalten,
        );
    });
}

fn anzeige(app: &mut App, ui: &mut egui::Ui) {
    ui.menu_button("Blickwinkel", |ui| {
        for text in ["Vorne", "Hinten", "Links", "Rechts", "Oben"] {
            spaeter(ui, text, "");
        }
    });
    ui.menu_button("3D-Kleidung", |ui| {
        spaeter(ui, "Netz", "");
        spaeter(ui, "Dehnungskarte", "Alt+7");
        spaeter(ui, "Passformkarte", "Alt+8");
        spaeter(ui, "Abstandskarte", "");
    });
    ui.menu_button("2D-Schnitt", |ui| {
        let a = app.ansicht.anzeige;
        haken(
            ui,
            app,
            "Nähte",
            "Umschalt+S",
            a.naehte,
            Aktion::NaehteUmschalten,
        );
        haken(
            ui,
            app,
            "Längen",
            "Umschalt+Z",
            a.laengen,
            Aktion::LaengenUmschalten,
        );
        haken(
            ui,
            app,
            "Teilnamen",
            "",
            a.teilnamen,
            Aktion::TeilnamenUmschalten,
        );
        haken(
            ui,
            app,
            "Maßstab",
            "",
            a.massstab,
            Aktion::MassstabUmschalten,
        );
    });
    ui.menu_button("Avatar", |ui| spaeter(ui, "Avatar", ""));
    ui.separator();
    ui.menu_button("Fenster", |ui| {
        for b in [
            Bereich::Bibliothek,
            Bereich::Browser,
            Bereich::Eigenschaften,
            Bereich::Fenster3d,
            Bereich::Fenster2d,
        ] {
            bereich(ui, app, b);
        }
        ui.separator();
        let beide = app.anordnung.fenster3d && app.anordnung.fenster2d;
        if ui
            .add_enabled(beide, knopf("Fenster wechseln", "Strg+Tab"))
            .on_hover_text("zwischen 3D- und 2D-Fenster")
            .on_disabled_hover_text("nur mit 3D- und 2D-Fenster")
            .clicked()
        {
            app.aktion(Aktion::FensterWechseln);
        }
    });
    ui.menu_button("Werkzeugleisten", |ui| {
        for b in [Bereich::LeisteDatei, Bereich::Leiste3d, Bereich::Leiste2d] {
            bereich(ui, app, b);
        }
    });
    ui.separator();
    if ui
        .button("Anordnung zurücksetzen")
        .on_hover_text("alle Fenster und Werkzeugleisten zeigen, Größen und Teilung wie am Anfang")
        .clicked()
    {
        app.aktion(Aktion::AnordnungZuruecksetzen);
    }
}

/// The file toolbar below the menu bar; hover texts name the shortcut.
pub fn werkzeugleiste(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_enabled(false, Button::new("Neu"))
            .on_disabled_hover_text(format!("Neu (Strg+N) – {SPAETER}"));
        if ui
            .button("Öffnen")
            .on_hover_text("Muster öffnen (Strg+O)")
            .clicked()
        {
            app.aktion(Aktion::Oeffnen);
        }
        ui.add_enabled(false, Button::new("Speichern"))
            .on_disabled_hover_text(format!("Speichern (Strg+S) – {SPAETER}"));
        ui.separator();
        ui.add_enabled(false, Button::new("Rückgängig"))
            .on_disabled_hover_text(format!("Rückgängig (Strg+Z) – {SPAETER}"));
        ui.add_enabled(false, Button::new("Wiederholen"))
            .on_disabled_hover_text(format!("Wiederholen (Strg+Y) – {SPAETER}"));
        ui.separator();
        let ziel = match app.thema {
            Theme::Dark => "hellen",
            Theme::Light => "dunklen",
        };
        if ui
            .button("Hell/Dunkel")
            .on_hover_text(format!("zum {ziel} Design wechseln"))
            .clicked()
        {
            app.aktion(Aktion::ThemaUmschalten);
        }
    });
}

/// A disabled toolbar button of a later task; the hover text names it.
fn spaeter_knopf(ui: &mut egui::Ui, text: &str, wofuer: &str) {
    ui.add_enabled(false, Button::new(text))
        .on_disabled_hover_text(format!("{wofuer} – {SPAETER}"));
}

/// The toolbar of the 3D window: simulation and maps, all for later tasks.
pub fn werkzeugleiste_3d(ui: &mut egui::Ui) {
    spaeter_knopf(ui, "Simulation", "Simulation starten/anhalten");
    spaeter_knopf(ui, "Einzelschritt", "einen Simulationsschritt");
    spaeter_knopf(ui, "Zurücksetzen", "Simulation zurücksetzen");
    ui.separator();
    spaeter_knopf(ui, "Avatar", "Avatar anzeigen (Umschalt+A)");
    spaeter_knopf(ui, "Dehnungskarte", "Dehnungskarte (Alt+7)");
    spaeter_knopf(ui, "Passformkarte", "Passformkarte (Alt+8)");
}

/// The toolbar of the 2D window: fit, layout and the display toggles; hover texts name the
/// shortcuts.
pub fn werkzeugleiste_2d(app: &mut App, ui: &mut egui::Ui) {
    if ui
        .add_enabled(app.muster.is_some(), Button::new("Einpassen"))
        .on_hover_text("alles einpassen (F)")
        .on_disabled_hover_text(format!("alles einpassen (F) – {ERST_MUSTER}"))
        .clicked()
    {
        app.aktion(Aktion::Einpassen);
    }
    let aus = app.ansicht.ausgelegt;
    if ui
        .add(Button::selectable(aus, "Auslage"))
        .on_hover_text("Teile nebeneinander ausgelegt (L)")
        .clicked()
    {
        app.aktion(Aktion::LageSetzen(true));
    }
    if ui
        .add(Button::selectable(!aus, "Konstruktion"))
        .on_hover_text("Teile in der Konstruktionslage (L)")
        .clicked()
    {
        app.aktion(Aktion::LageSetzen(false));
    }
    ui.separator();
    let a = app.ansicht.anzeige;
    for (an, text, hinweis, aktion) in [
        (
            a.naehte,
            "Nähte",
            "genähte Kanten farbig (Umschalt+S)",
            Aktion::NaehteUmschalten,
        ),
        (
            a.laengen,
            "Längen",
            "Länge jeder Randkante in cm – des gewählten Teils, ohne Wahl aller Teile (Umschalt+Z)",
            Aktion::LaengenUmschalten,
        ),
        (
            a.teilnamen,
            "Teilnamen",
            "Namen der Teile, wo sie hineinpassen; das gewählte Teil und das Teil unter dem \
             Mauszeiger nennen ihren Namen immer",
            Aktion::TeilnamenUmschalten,
        ),
    ] {
        if ui
            .add(Button::selectable(an, text))
            .on_hover_text(hinweis)
            .clicked()
        {
            app.aktion(aktion);
        }
    }
}
