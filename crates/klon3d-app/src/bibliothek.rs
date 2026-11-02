//! The library: a file browser docked at the left of the window, not modal.
//!
//! It is the listing of the file dialog ([`Verzeichnis`]) with places to jump to – "Muster" (the
//! folder of the pattern shown, or of the one waiting for a measurement file), "Stoffe" (the
//! fabrics folder, if one was found), "Zuletzt" (the last folder of a load), on Windows the drives
//! – the current path, "Hoch", and the folders and files of the kinds klon3d knows. A single click
//! marks a file, a double click or the button below uses it: a pattern (`.sm2d`) is opened, a
//! measurement file (`.smis`) or seam list (`.naehte`) applies to the pattern shown or to the one
//! that failed for want of a measurement file – exactly as if the file had been dropped onto the
//! window. Fabrics (`.zfab`) and CLO projects (`.zprj`) are listed but greyed: nothing reads them
//! yet.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::Button;

use crate::app::{Aktion, App, SPAETER};
use crate::dateiwahl::{passt, Eintrag, Verzeichnis, Zeilenklick};
use crate::menue::{ERST_MUSTER, KEIN_STOFFORDNER};
use crate::muster::dateiname;

/// The kinds of files the library lists.
pub const ENDUNGEN: &[&str] = &["sm2d", "smis", "naehte", "zfab", "zprj"];

/// Kinds listed without an action yet.
const OHNE_AKTION: &[&str] = &["zfab", "zprj"];

/// What the library does with a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Muster,
    Massdatei,
    Nahtliste,
    /// Listed, but nothing reads it yet.
    Spaeter,
}

impl Art {
    /// The kind of `pfad` by its extension; `None` for files the library does not list.
    pub fn von(pfad: &Path) -> Option<Art> {
        if passt(pfad, &["sm2d"]) {
            Some(Art::Muster)
        } else if passt(pfad, &["smis"]) {
            Some(Art::Massdatei)
        } else if passt(pfad, &["naehte"]) {
            Some(Art::Nahtliste)
        } else if passt(pfad, OHNE_AKTION) {
            Some(Art::Spaeter)
        } else {
            None
        }
    }

    /// The label of the button that uses a file of this kind.
    pub fn knopf(self) -> &'static str {
        match self {
            Art::Muster => "Muster laden",
            Art::Massdatei => "Als Maßdatei nehmen",
            Art::Nahtliste => "Als Nahtliste nehmen",
            Art::Spaeter => "Datei verwenden",
        }
    }
}

/// The state of the library panel.
#[derive(Debug, Clone)]
pub struct Bibliothek {
    pub liste: Verzeichnis,
}

impl Bibliothek {
    /// The library in `ordner` (the current folder if `ordner` is not readable).
    pub fn neu(ordner: &Path) -> Bibliothek {
        Bibliothek {
            liste: Verzeichnis::neu(ENDUNGEN, ordner),
        }
    }

    /// Show folder `ordner`.
    pub fn wechseln(&mut self, ordner: &Path) -> bool {
        self.liste.wechseln(ordner)
    }
}

/// The places row: pattern folder, fabrics folder, last folder, drives (Windows).
fn orte(app: &mut App, ui: &mut egui::Ui) -> Option<PathBuf> {
    let mut ziel = None;
    let muster_ordner = app
        .ladeziel()
        .and_then(|a| a.muster.parent().map(Path::to_path_buf));
    let r = ui
        .add_enabled(muster_ordner.is_some(), Button::new("Muster"))
        .on_disabled_hover_text(ERST_MUSTER);
    if let Some(o) = &muster_ordner {
        if r.on_hover_text(format!("Ordner des Musters: {}", o.display()))
            .clicked()
        {
            ziel = Some(o.clone());
        }
    }
    let r = ui
        .add_enabled(app.stoffordner.is_some(), Button::new("Stoffe"))
        .on_disabled_hover_text(KEIN_STOFFORDNER);
    if let Some(o) = &app.stoffordner {
        if r.on_hover_text(format!("Stoffordner: {}", o.display()))
            .clicked()
        {
            ziel = Some(o.clone());
        }
    }
    if ui
        .button("Zuletzt")
        .on_hover_text(format!(
            "zuletzt benutzter Ordner: {}",
            app.letzter_ordner.display()
        ))
        .clicked()
    {
        ziel = Some(app.letzter_ordner.clone());
    }
    if cfg!(windows) {
        if let Some(lw) = app.bibliothek.liste.laufwerksknoepfe(ui) {
            ziel = Some(lw);
        }
    }
    ziel
}

/// The library panel.
pub fn panel(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.heading("Bibliothek");
    let mut ziel = None;
    ui.horizontal_wrapped(|ui| ziel = orte(app, ui));
    ui.horizontal(|ui| {
        let oben = app.bibliothek.liste.oben();
        if ui
            .add_enabled(oben.is_some(), Button::new("Hoch"))
            .on_hover_text("übergeordneter Ordner")
            .clicked()
        {
            ziel = oben;
        }
        let pfad = app.bibliothek.liste.ordner.display().to_string();
        ui.add(egui::Label::new(egui::RichText::new(&pfad).weak()).truncate())
            .on_hover_text(pfad);
    });
    if let Some(o) = ziel {
        app.bibliothek.wechseln(&o);
    }
    if let Some(f) = &app.bibliothek.liste.fehler {
        ui.colored_label(ui.visuals().error_fg_color, f);
    }
    ui.separator();
    if app.bibliothek.liste.eintraege.is_empty() {
        ui.weak("keine Ordner und keine Muster-, Maß-, Naht- oder Stoffdateien");
    }
    let unten = 2.0 * ui.spacing().interact_size.y + 12.0;
    let hoehe = (ui.available_height() - unten).max(60.0);
    let gesperrt = |e: &Eintrag| Art::von(&e.pfad) == Some(Art::Spaeter);
    match app.bibliothek.liste.zeilen(ui, hoehe, gesperrt, SPAETER) {
        Some(Zeilenklick::Ordner(p)) => {
            app.bibliothek.wechseln(&p);
        }
        Some(Zeilenklick::Doppelt(p)) => verwenden(app, p),
        Some(Zeilenklick::Markiert(_)) | None => {}
    }
    ui.separator();
    knopfzeile(app, ui);
}

/// The button below the listing: uses the marked file by its kind.
fn knopfzeile(app: &mut App, ui: &mut egui::Ui) {
    let markiert = app.bibliothek.liste.markiert.clone();
    let art = markiert.as_deref().and_then(Art::von);
    let ziel = app.ladeziel().map(|a| dateiname(&a.muster));
    let (frei, warum) = match art {
        None => (false, "erst eine Datei anklicken".to_string()),
        Some(Art::Spaeter) => (false, SPAETER.to_string()),
        Some(Art::Muster) => (true, String::new()),
        Some(Art::Massdatei | Art::Nahtliste) if ziel.is_none() => (false, ERST_MUSTER.to_string()),
        Some(_) => (true, String::new()),
    };
    ui.horizontal(|ui| {
        let text = art.map_or("Datei verwenden", Art::knopf);
        let r = ui
            .add_enabled(frei, Button::new(text))
            .on_disabled_hover_text(warum);
        let r = match (&ziel, art) {
            (Some(z), Some(Art::Massdatei | Art::Nahtliste)) => r.on_hover_text(format!("für {z}")),
            _ => r,
        };
        if r.clicked() {
            if let Some(p) = markiert {
                verwenden(app, p);
            }
        }
        if let Some(p) = &app.bibliothek.liste.markiert {
            ui.add(egui::Label::new(egui::RichText::new(dateiname(p)).weak()).truncate());
        }
    });
}

/// Use a file of the library by its kind; greyed kinds do nothing.
fn verwenden(app: &mut App, p: PathBuf) {
    match Art::von(&p) {
        Some(Art::Muster | Art::Massdatei | Art::Nahtliste) => {
            app.aktion(Aktion::DateiVerwenden(p))
        }
        Some(Art::Spaeter) | None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arten_nach_endung() {
        assert_eq!(Art::von(Path::new("a/B.SM2D")), Some(Art::Muster));
        assert_eq!(Art::von(Path::new("m.smis")), Some(Art::Massdatei));
        assert_eq!(Art::von(Path::new("n.naehte")), Some(Art::Nahtliste));
        assert_eq!(Art::von(Path::new("s.zfab")), Some(Art::Spaeter));
        assert_eq!(Art::von(Path::new("p.zprj")), Some(Art::Spaeter));
        assert_eq!(Art::von(Path::new("notiz.txt")), None);
        // Every kind listed has a kind, every kind has a listed extension.
        for e in ENDUNGEN {
            assert!(Art::von(Path::new(&format!("x.{e}"))).is_some(), "{e}");
        }
    }
}
