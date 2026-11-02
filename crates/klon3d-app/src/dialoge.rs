//! The windows that float above the layout: the file dialog, the error window, the help windows
//! (shortcuts, about), "Informationen" about the loaded files, and the hint while files are
//! dragged over the window.

use eframe::egui;

use crate::app::{Aktion, App, Hilfe};
use crate::dateiwahl::Antwort;
use crate::muster::{dateiname, Nahtstand};

impl App {
    /// Show the open dialogs; runs every frame after the layout.
    pub(crate) fn dialoge(&mut self, ctx: &egui::Context) {
        self.dateidialog(ctx);
        self.fehlerdialog(ctx);
        self.hilfedialog(ctx);
        self.informationsdialog(ctx);
    }

    fn dateidialog(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.dateiwahl else {
            return;
        };
        let zweck = d.zweck;
        match d.ui(ctx) {
            Some(Antwort::Gewaehlt(p)) => {
                if let Some(o) = p.parent().filter(|o| o.is_dir()) {
                    self.letzter_ordner = o.to_path_buf();
                }
                self.dateiwahl = None;
                self.datei_annehmen(zweck, p);
            }
            Some(Antwort::Abbruch) => self.dateiwahl = None,
            None => {}
        }
    }

    fn fehlerdialog(&mut self, ctx: &egui::Context) {
        let Some(text) = self.fehlerfenster.clone() else {
            return;
        };
        let (mut zu, mut waehlen) = (false, false);
        // The pattern that waits for a measurement file, if the error was about it.
        let angebot = self
            .wartend
            .as_ref()
            .filter(|_| self.massdatei_anbieten)
            .map(|a| dateiname(&a.muster));
        egui::Window::new("Fehler")
            .id(egui::Id::unique("fehlerfenster"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.colored_label(ui.visuals().error_fg_color, &text);
                if let Some(name) = &angebot {
                    ui.add_space(4.0);
                    ui.label(format!(
                        "{name} lädt, sobald eine passende Maßdatei gewählt ist."
                    ));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if angebot.is_some() {
                        waehlen = ui.button("Maßdatei wählen…").clicked();
                    }
                    zu = ui.button("Fehler schließen").clicked();
                });
            });
        if waehlen {
            self.fehlerfenster = None;
            self.aktion(Aktion::MassdateiWaehlen);
        }
        if zu {
            self.fehlerfenster = None;
        }
    }

    fn hilfedialog(&mut self, ctx: &egui::Context) {
        let Some(h) = self.hilfe else {
            return;
        };
        let mut zu = false;
        let titel = match h {
            Hilfe::Tastenkuerzel => "Tastenkürzel",
            Hilfe::Ueber => "Über klon3d",
        };
        // Centred like the other dialogs: without a position egui puts a window at the top left,
        // over the menu bar and the file toolbar.
        egui::Window::new(titel)
            .id(egui::Id::unique("hilfefenster"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                match h {
                    Hilfe::Tastenkuerzel => tastenkuerzel(ui),
                    Hilfe::Ueber => ueber(ui),
                }
                ui.add_space(6.0);
                zu = ui.button("Hilfe schließen").clicked();
            });
        if zu {
            self.hilfe = None;
        }
    }

    /// Datei › Informationen…: which files the pattern shown was loaded with.
    fn informationsdialog(&mut self, ctx: &egui::Context) {
        if !self.informationen {
            return;
        }
        let Some(m) = &self.muster else {
            self.informationen = false;
            return;
        };
        let mut zu = false;
        egui::Window::new("Informationen")
            .id(egui::Id::unique("informationen"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(640.0);
                egui::Grid::new("informationen_raster")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        let zeile = |ui: &mut egui::Ui, name: &str, wert: String| {
                            ui.strong(name);
                            ui.label(wert);
                            ui.end_row();
                        };
                        zeile(ui, "Muster", m.pfad.display().to_string());
                        zeile(ui, "Einheit", m.einheit.clone());
                        zeile(ui, "Teile", m.teile.len().to_string());
                        let herkunft = if m.massdatei_von_hand {
                            "von Hand gewählt"
                        } else {
                            "vom Muster genannt"
                        };
                        zeile(
                            ui,
                            "Maßdatei",
                            format!("{} ({herkunft})", m.massdatei.display()),
                        );
                        let (liste, naehte) = match &m.naehte {
                            Nahtstand::Geprueft { pfad, bericht } => {
                                (pfad.display().to_string(), bericht.naehte.len().to_string())
                            }
                            Nahtstand::Keine { gesucht } => {
                                (format!("keine – {} fehlt", gesucht.display()), "–".into())
                            }
                            Nahtstand::Fehler { pfad, text } => (
                                format!("{} – nicht prüfbar: {text}", pfad.display()),
                                "–".into(),
                            ),
                        };
                        zeile(ui, "Nahtliste", liste);
                        zeile(ui, "Nähte", naehte);
                    });
                ui.add_space(6.0);
                zu = ui.button("Informationen schließen").clicked();
            });
        if zu {
            self.informationen = false;
        }
    }

    /// Paint a hint while files are dragged over the window.
    pub(crate) fn ablage_hinweis(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let namen: Vec<String> = ctx.input(|i| {
            i.raw
                .hovered_files
                .iter()
                .filter_map(|f| f.path.as_ref())
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect()
        });
        let maler = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("ablage"),
        ));
        let r = ctx.content_rect();
        maler.rect_filled(r, 0.0, egui::Color32::from_black_alpha(160));
        maler.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            format!("Loslassen zum Öffnen\n{}", namen.join("\n")),
            egui::FontId::proportional(20.0),
            egui::Color32::WHITE,
        );
    }
}

/// Every shortcut with what it does, in the order of the help window.
pub const TASTENKUERZEL: [(&str, &str); 14] = [
    ("Strg+O", "Muster öffnen"),
    ("Strg+Q", "Beenden"),
    ("Strg+Tab", "zwischen 3D- und 2D-Fenster wechseln"),
    ("F", "alles einpassen"),
    ("L", "Auslage / Konstruktionslage"),
    ("Tab", "nächstes Teil wählen"),
    ("Umschalt+Tab", "vorheriges Teil wählen"),
    ("Umschalt+S", "Nähte anzeigen ein/aus"),
    ("Umschalt+Z", "Längen anzeigen ein/aus"),
    ("Esc", "Auswahl aufheben, Dialog schließen"),
    ("Mausrad", "Zoom um den Mauszeiger"),
    ("Ziehen", "Ansicht verschieben"),
    ("Klick", "Teil wählen"),
    ("Klick ins Fenster", "3D- oder 2D-Fenster aktivieren"),
];

/// The table of the help window "Tastenkürzel".
fn tastenkuerzel(ui: &mut egui::Ui) {
    egui::Grid::new("tastenkuerzel")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (taste, wirkung) in TASTENKUERZEL {
                ui.strong(taste);
                ui.label(wirkung);
                ui.end_row();
            }
        });
    ui.weak(
        "Buchstaben und Tab wirken nicht, solange ein Textfeld den Fokus hat oder der \
         Dateidialog offen ist.",
    );
}

/// The text of the help window "Über klon3d".
fn ueber(ui: &mut egui::Ui) {
    ui.heading(format!("klon3d {}", env!("CARGO_PKG_VERSION")));
    ui.label("3D-Simulator für Latex- und Textilkleidung.");
    ui.label(
        "Dieses Fenster (Stufe 6) zeigt Seamly2D-Muster und prüft Nahtlängen mit denselben Zahlen \
         wie „klon3d naehte“. 3D-Ansicht, Simulation, Material und Export folgen.",
    );
    ui.label("Lizenz: MIT OR Apache-2.0");
}
