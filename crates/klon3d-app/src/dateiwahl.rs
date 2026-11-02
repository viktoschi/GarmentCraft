//! Folder listings: the small file dialog inside the window, and the core it shares with the
//! library panel ([`crate::bibliothek`]).
//!
//! No `rfd`: on Linux it pulls in GTK or a D-Bus portal stack, and a native dialog cannot be driven
//! by the headless UI tests. [`Verzeichnis`] is one folder with its subfolders and the files that
//! match a list of extensions, its drive buttons and its rows (single click: open a folder or mark
//! a file, double click on a file: act – only on the file the first click marked).
//! [`Dateiwahl`] puts it into a window with a filter switch and a path field that takes any path
//! – absolute or relative to the current folder, on Windows also a bare drive letter (`D:`); the
//! library shows the same core docked at the left. The listing and the path handling are plain
//! functions with unit tests; the `ui` functions only show them.
//!
//! Nothing here touches the disk per frame: the folder is read when it changes, the drive list
//! once per listing (on Windows from `GetLogicalDrives`, without opening a drive – a disconnected
//! network drive would block), and only the visible rows of the listing are laid out.

use std::path::{Path, PathBuf};

use eframe::egui;

/// What the dialog chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zweck {
    Muster,
    Massdatei,
    Nahtliste,
}

impl Zweck {
    /// File extensions shown with the filter on (lower case, without the dot).
    pub fn endungen(self) -> &'static [&'static str] {
        match self {
            Zweck::Muster => &["sm2d"],
            Zweck::Massdatei => &["smis"],
            Zweck::Nahtliste => &["naehte"],
        }
    }

    /// Window title.
    pub fn titel(self) -> &'static str {
        match self {
            Zweck::Muster => "Muster öffnen",
            Zweck::Massdatei => "Maßdatei wählen",
            Zweck::Nahtliste => "Nahtliste wählen",
        }
    }
}

/// One line of the listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub name: String,
    pub pfad: PathBuf,
    pub ordner: bool,
}

/// How the dialog ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Antwort {
    Gewaehlt(PathBuf),
    Abbruch,
}

/// Does the file name end in one of `endungen` (case-insensitive)?
pub fn passt(pfad: &Path, endungen: &[&str]) -> bool {
    pfad.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| endungen.iter().any(|w| e.eq_ignore_ascii_case(w)))
}

/// Subfolders first, then the files that match `endungen` (all files with `alle`), each sorted by
/// name without regard to case. Hidden entries ([`versteckt`]) are left out.
pub fn auflisten(ordner: &Path, endungen: &[&str], alle: bool) -> Result<Vec<Eintrag>, String> {
    let lesen = std::fs::read_dir(ordner)
        .map_err(|e| format!("Ordner nicht lesbar: {}: {e}", ordner.display()))?;
    let mut ordner_liste = Vec::new();
    let mut dateien = Vec::new();
    for e in lesen.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if versteckt(&e, &name) {
            continue;
        }
        let pfad = e.path();
        // `is_dir` follows links, so a link to a folder is a folder.
        if pfad.is_dir() {
            ordner_liste.push(Eintrag {
                name,
                pfad,
                ordner: true,
            });
        } else if alle || passt(&pfad, endungen) {
            dateien.push(Eintrag {
                name,
                pfad,
                ordner: false,
            });
        }
    }
    let nach_name = |a: &Eintrag, b: &Eintrag| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    };
    ordner_liste.sort_by(nach_name);
    dateien.sort_by(nach_name);
    ordner_liste.extend(dateien);
    Ok(ordner_liste)
}

/// Is a folder entry hidden? Its name starts with a dot, or on Windows it carries the hidden or
/// system attribute (AppData, `$Recycle.Bin`, "System Volume Information", the old junctions such
/// as "Application Data" that cannot be listed). The attributes come with the directory listing on
/// Windows, so this opens nothing.
fn versteckt(eintrag: &std::fs::DirEntry, name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const VERSTECKT: u32 = 0x2; // FILE_ATTRIBUTE_HIDDEN
        const SYSTEM: u32 = 0x4; // FILE_ATTRIBUTE_SYSTEM
        if let Ok(m) = eintrag.metadata() {
            return m.file_attributes() & (VERSTECKT | SYSTEM) != 0;
        }
    }
    #[cfg(not(windows))]
    let _ = eintrag;
    false
}

/// The path meant by the text in the path field, seen from folder `basis`.
///
/// Quotes and surrounding blanks (as pasted from a file manager) are removed, `~` at the start
/// means the home folder, a relative path is taken relative to `basis`. On Windows a bare drive
/// letter `D:` means the root of that drive (`D:\`), not its current folder.
pub fn eingabe_aufloesen(text: &str, basis: &Path) -> PathBuf {
    let t = text.trim().trim_matches('"').trim();
    if t.is_empty() {
        return basis.to_path_buf();
    }
    if cfg!(windows) && ist_laufwerk(t) {
        return PathBuf::from(format!("{}\\", &t[..2]));
    }
    if let Some(rest) = t.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(heim) = heimordner() {
                return heim.join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    let p = Path::new(t);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        basis.join(p)
    }
}

/// `X:` or `X:\` / `X:/` – a drive letter on its own.
fn ist_laufwerk(t: &str) -> bool {
    let b = t.as_bytes();
    (b.len() == 2 || (b.len() == 3 && (b[2] == b'\\' || b[2] == b'/')))
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
}

fn heimordner() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// The roots one can jump to: on Windows the drives the system reports, elsewhere `/`.
///
/// On Windows the drives are not touched: probing `X:\` of a disconnected network drive blocks
/// until a network timeout. Whether a drive is readable shows when the user opens it.
pub fn laufwerke() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        // SAFETY: GetLogicalDrives takes no arguments and only returns a bit mask.
        laufwerke_aus_maske(unsafe { GetLogicalDrives() })
    }
    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
    }
}

/// The drive roots of a bit mask as `GetLogicalDrives` returns it: bit 0 is `A:\`, bit 25 `Z:\`.
pub fn laufwerke_aus_maske(maske: u32) -> Vec<PathBuf> {
    (0..26u8)
        .filter(|i| maske & (1 << i) != 0)
        .map(|i| PathBuf::from(format!("{}:\\", (b'A' + i) as char)))
        .collect()
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLogicalDrives() -> u32;
}

/// Remove `.` and `..` components where that is possible without the file system, so that the
/// path field shows `/a/b` and not `/a/b/c/..`.
pub fn bereinigt(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                let kann_hoch = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if kann_hoch {
                    out.pop();
                } else if !out.has_root() {
                    out.push("..");
                }
            }
            andere => out.push(andere.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// What a click on a row of a [`Verzeichnis`] means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Zeilenklick {
    /// A folder was clicked: open it.
    Ordner(PathBuf),
    /// A file was clicked once: it is marked now.
    Markiert(PathBuf),
    /// A file was double-clicked: act on it.
    Doppelt(PathBuf),
}

/// One folder as a listing – the core of the file dialog and of the library.
#[derive(Debug, Clone)]
pub struct Verzeichnis {
    /// The folder shown.
    pub ordner: PathBuf,
    /// Extensions of the files listed (lower case, without the dot).
    pub endungen: &'static [&'static str],
    /// List all files, not only the matching ones.
    pub alle: bool,
    pub eintraege: Vec<Eintrag>,
    /// Why the last change of folder did not work.
    pub fehler: Option<String>,
    /// The drive buttons, read once when the listing is made ([`laufwerke`]).
    pub laufwerke: Vec<PathBuf>,
    /// The file marked by a single click.
    pub markiert: Option<PathBuf>,
}

impl Verzeichnis {
    /// The listing of `ordner` (the current folder if `ordner` is not readable).
    pub fn neu(endungen: &'static [&'static str], ordner: &Path) -> Verzeichnis {
        let mut v = Verzeichnis {
            ordner: PathBuf::new(),
            endungen,
            alle: false,
            eintraege: Vec::new(),
            fehler: None,
            laufwerke: laufwerke(),
            markiert: None,
        };
        let start = if ordner.is_dir() {
            ordner.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        v.wechseln(&start);
        v
    }

    /// Show folder `ordner`; on an error the old folder stays, the error is kept and `false`
    /// returned.
    pub fn wechseln(&mut self, ordner: &Path) -> bool {
        let ziel = bereinigt(&std::path::absolute(ordner).unwrap_or_else(|_| ordner.to_path_buf()));
        match auflisten(&ziel, self.endungen, self.alle) {
            Ok(e) => {
                self.eintraege = e;
                self.ordner = ziel;
                self.fehler = None;
                self.markiert = None;
                true
            }
            Err(e) => {
                self.fehler = Some(e);
                false
            }
        }
    }

    /// The folder above, if there is one.
    pub fn oben(&self) -> Option<PathBuf> {
        self.ordner.parent().map(Path::to_path_buf)
    }

    /// Re-read the folder (e.g. after switching the filter).
    pub fn neu_lesen(&mut self) -> bool {
        let o = self.ordner.clone();
        self.wechseln(&o)
    }

    /// One button per drive; returns the drive clicked.
    pub fn laufwerksknoepfe(&self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut ziel = None;
        for lw in &self.laufwerke {
            let name = lw.display().to_string();
            if ui
                .button(&name)
                .on_hover_text(format!("zu {name} wechseln"))
                .clicked()
            {
                ziel = Some(lw.clone());
            }
        }
        ziel
    }

    /// The rows of the listing in a scroll area at most `max_hoehe` high. A single click marks a
    /// file ([`Verzeichnis::markiert`]) or asks to open a folder, a double click on a file asks to
    /// act on it; the caller decides. Files for which `gesperrt` says so are shown disabled with
    /// the hover text `warum`. Every row has the same height, so only the visible ones are laid out
    /// – a folder with tens of thousands of files stays quick.
    ///
    /// A double click acts only on the file its first click marked. egui counts a double click
    /// for whatever widget gets the second click: after the first click of a double click on a
    /// folder has opened it, the second one lands on the row that now lies under the pointer –
    /// it must neither act on that row nor open a further folder.
    pub fn zeilen(
        &mut self,
        ui: &mut egui::Ui,
        max_hoehe: f32,
        gesperrt: impl Fn(&Eintrag) -> bool,
        warum: &str,
    ) -> Option<Zeilenklick> {
        let zeile = zeilenhoehe(ui);
        let mut klick = None;
        // Marked before this frame: by the first click of a double click on the same file.
        let vorher = self.markiert.clone();
        egui::ScrollArea::vertical()
            .max_height(max_hoehe)
            .auto_shrink([false, false])
            .show_rows(ui, zeile, self.eintraege.len(), |ui, sichtbar| {
                for e in &self.eintraege[sichtbar] {
                    let text = if e.ordner {
                        format!("{}{}", e.name, std::path::MAIN_SEPARATOR)
                    } else {
                        e.name.clone()
                    };
                    let frei = e.ordner || !gesperrt(e);
                    let gewaehlt = self.markiert.as_ref() == Some(&e.pfad);
                    let r = ui
                        .add_enabled(
                            frei,
                            egui::Button::selectable(gewaehlt, text)
                                .min_size(egui::vec2(0.0, zeile)),
                        )
                        .on_disabled_hover_text(warum);
                    // Only the first click of a run opens a folder; the further ones of a
                    // double or triple click belong to the row the first one hit.
                    let weiterer = r.double_clicked() || r.triple_clicked();
                    if e.ordner {
                        if r.clicked() && !weiterer {
                            klick = Some(Zeilenklick::Ordner(e.pfad.clone()));
                        }
                    } else if weiterer {
                        if r.double_clicked() && vorher.as_ref() == Some(&e.pfad) {
                            klick = Some(Zeilenklick::Doppelt(e.pfad.clone()));
                        }
                    } else if r.clicked() {
                        klick = Some(Zeilenklick::Markiert(e.pfad.clone()));
                    }
                }
            });
        if let Some(Zeilenklick::Markiert(p) | Zeilenklick::Doppelt(p)) = &klick {
            self.markiert = Some(p.clone());
        }
        klick
    }
}

/// The dialog's state while it is open.
#[derive(Debug, Clone)]
pub struct Dateiwahl {
    pub zweck: Zweck,
    /// The folder shown.
    pub liste: Verzeichnis,
    /// Text of the path field.
    pub pfad_text: String,
}

impl Dateiwahl {
    /// Open the dialog in `ordner` (falls back to the current folder if it is not readable).
    pub fn neu(zweck: Zweck, ordner: &Path) -> Dateiwahl {
        let liste = Verzeichnis::neu(zweck.endungen(), ordner);
        let pfad_text = liste.ordner.display().to_string();
        Dateiwahl {
            zweck,
            liste,
            pfad_text,
        }
    }

    /// Show folder `ordner`; on an error the old folder stays and the error is shown.
    pub fn wechseln(&mut self, ordner: &Path) {
        if self.liste.wechseln(ordner) {
            self.pfad_text = self.liste.ordner.display().to_string();
        }
    }

    /// One folder up.
    pub fn hoch(&mut self) {
        if let Some(p) = self.liste.oben() {
            self.wechseln(&p);
        }
    }

    /// Re-read the folder (e.g. after switching the filter).
    pub fn neu_lesen(&mut self) {
        let o = self.liste.ordner.clone();
        self.wechseln(&o);
    }

    /// What the path field means: a folder is opened, a matching file is the answer.
    pub fn bestaetigen(&mut self) -> Option<Antwort> {
        let ziel = eingabe_aufloesen(&self.pfad_text, &self.liste.ordner);
        if ziel.is_dir() {
            self.wechseln(&ziel);
            return None;
        }
        if !ziel.is_file() {
            self.liste.fehler = Some(format!("Datei nicht gefunden: {}", ziel.display()));
            return None;
        }
        if !self.liste.alle && !passt(&ziel, self.zweck.endungen()) {
            self.liste.fehler = Some(format!(
                "{} ist keine {}-Datei (Filter ausschalten mit „alle Dateien“)",
                ziel.display(),
                self.filter_text()
            ));
            return None;
        }
        Some(Antwort::Gewaehlt(bereinigt(&ziel)))
    }

    fn filter_text(&self) -> String {
        self.zweck
            .endungen()
            .iter()
            .map(|e| format!(".{e}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The filter as the footer shows it.
    pub fn filter_zeile(&self) -> String {
        if self.liste.alle {
            "Filter: aus (alle Dateien)".into()
        } else {
            format!("Filter: {}", self.filter_text())
        }
    }

    /// What an empty listing says.
    pub fn leer_text(&self) -> String {
        if self.liste.alle {
            "keine Einträge".into()
        } else {
            format!("keine Ordner und keine {}-Dateien", self.filter_text())
        }
    }

    /// Show the dialog as a window; returns the answer once the user chose or cancelled.
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<Antwort> {
        let mut antwort = None;
        egui::Window::new(self.zweck.titel())
            .id(egui::Id::unique("dateiwahl"))
            .collapsible(false)
            .resizable(true)
            .default_size([620.0, 440.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("Hoch")
                        .on_hover_text("übergeordneter Ordner")
                        .clicked()
                    {
                        self.hoch();
                    }
                    if let Some(lw) = self.liste.laufwerksknoepfe(ui) {
                        self.wechseln(&lw);
                    }
                    if ui
                        .checkbox(&mut self.liste.alle, "alle Dateien")
                        .on_hover_text(format!("aus: nur {}", self.filter_text()))
                        .changed()
                    {
                        self.neu_lesen();
                    }
                });
                ui.horizontal(|ui| {
                    let l = ui.label("Pfad:");
                    let feld = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.pfad_text)
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(l.id);
                    if feld.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        antwort = self.bestaetigen();
                    }
                });
                if let Some(f) = &self.liste.fehler {
                    ui.colored_label(ui.visuals().error_fg_color, f);
                }
                ui.separator();
                let unten = 36.0;
                if self.liste.eintraege.is_empty() {
                    ui.weak(self.leer_text());
                }
                let hoehe = (ui.available_height() - unten).max(80.0);
                match self.liste.zeilen(ui, hoehe, |_| false, "") {
                    Some(Zeilenklick::Ordner(p)) => self.wechseln(&p),
                    Some(Zeilenklick::Doppelt(p)) => antwort = Some(Antwort::Gewaehlt(p)),
                    Some(Zeilenklick::Markiert(p)) => self.pfad_text = p.display().to_string(),
                    None => {}
                }
                ui.separator();
                ui.horizontal(|ui| {
                    // Not just "Öffnen": the toolbar has a button of that name.
                    let titel = match self.zweck {
                        Zweck::Muster => "Datei öffnen",
                        _ => "Datei wählen",
                    };
                    if ui.button(titel).clicked() {
                        antwort = self.bestaetigen();
                    }
                    if ui.button("Abbrechen").clicked() {
                        antwort = Some(Antwort::Abbruch);
                    }
                    ui.weak(self.filter_zeile());
                });
            });
        antwort
    }
}

/// Height of one row of the listing without spacing: at least a button's natural height.
fn zeilenhoehe(ui: &egui::Ui) -> f32 {
    let natuerlich =
        ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
    natuerlich.max(ui.spacing().interact_size.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder with a few files and subfolders.
    fn ordner(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "klon3d-app-dateiwahl-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Zweig")).unwrap();
        std::fs::create_dir_all(d.join("ast")).unwrap();
        std::fs::create_dir_all(d.join(".versteckt")).unwrap();
        for f in [
            "b.sm2d",
            "A.SM2D",
            "c.smis",
            "d.naehte",
            "notiz.txt",
            ".geheim.sm2d",
        ] {
            std::fs::write(d.join(f), "x").unwrap();
        }
        d
    }

    fn namen(e: &[Eintrag]) -> Vec<&str> {
        e.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn ordner_zuerst_dann_passende_dateien() {
        let d = ordner("liste");
        let e = auflisten(&d, Zweck::Muster.endungen(), false).unwrap();
        assert_eq!(namen(&e), ["ast", "Zweig", "A.SM2D", "b.sm2d"]);
        assert!(e[0].ordner && !e[2].ordner);
        let e = auflisten(&d, Zweck::Nahtliste.endungen(), false).unwrap();
        assert_eq!(namen(&e), ["ast", "Zweig", "d.naehte"]);
        let e = auflisten(&d, &[], true).unwrap();
        assert_eq!(
            namen(&e),
            [
                "ast",
                "Zweig",
                "A.SM2D",
                "b.sm2d",
                "c.smis",
                "d.naehte",
                "notiz.txt"
            ]
        );
        assert!(auflisten(&d.join("gibt_es_nicht"), &["sm2d"], false).is_err());
        if cfg!(windows) {
            // A file with the hidden attribute is left out like a dot file.
            let ok = std::process::Command::new("attrib")
                .arg("+h")
                .arg(d.join("b.sm2d"))
                .status()
                .unwrap()
                .success();
            assert!(ok, "attrib +h");
            let e = auflisten(&d, Zweck::Muster.endungen(), false).unwrap();
            assert_eq!(namen(&e), ["ast", "Zweig", "A.SM2D"]);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn eingabe_relativ_absolut_und_heim() {
        let basis = std::env::temp_dir();
        assert_eq!(eingabe_aufloesen("", &basis), basis);
        assert_eq!(
            eingabe_aufloesen("  \"x.sm2d\" ", &basis),
            basis.join("x.sm2d")
        );
        let abs = std::env::temp_dir().join("y.sm2d");
        assert_eq!(
            eingabe_aufloesen(&abs.display().to_string(), Path::new("woanders")),
            abs
        );
        if let Some(h) = heimordner() {
            assert_eq!(eingabe_aufloesen("~", &basis), h);
            assert_eq!(eingabe_aufloesen("~/muster", &basis), h.join("muster"));
        }
    }

    #[test]
    fn laufwerksbuchstaben() {
        assert!(ist_laufwerk("D:") && ist_laufwerk("d:\\") && ist_laufwerk("C:/"));
        assert!(!ist_laufwerk("D:x") && !ist_laufwerk("1:") && !ist_laufwerk("D"));
        let basis = Path::new("basis");
        assert_eq!(
            laufwerke_aus_maske(0b101 | 1 << 25),
            ["A:\\", "C:\\", "Z:\\"].map(PathBuf::from)
        );
        assert!(laufwerke_aus_maske(0).is_empty());
        if cfg!(windows) {
            assert_eq!(eingabe_aufloesen("D:", basis), PathBuf::from("D:\\"));
            assert_eq!(eingabe_aufloesen("d:/", basis), PathBuf::from("d:\\"));
            // The system drive is among the drives the system reports.
            let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
            assert!(
                laufwerke().contains(&PathBuf::from(format!("{system}\\"))),
                "{:?}",
                laufwerke()
            );
        } else {
            // Elsewhere `D:` is an ordinary (relative) name.
            assert_eq!(eingabe_aufloesen("D:", basis), basis.join("D:"));
            assert_eq!(laufwerke(), vec![PathBuf::from("/")]);
        }
    }

    #[test]
    fn punkte_im_pfad_bereinigen() {
        assert_eq!(
            bereinigt(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(bereinigt(Path::new("/..")), PathBuf::from("/"));
        assert_eq!(bereinigt(Path::new("../x")), PathBuf::from("../x"));
        assert_eq!(bereinigt(Path::new("a/..")), PathBuf::from("."));
    }

    #[test]
    fn dialog_wechselt_ordner_und_waehlt_datei() {
        let d = ordner("dialog");
        let mut w = Dateiwahl::neu(Zweck::Muster, &d);
        assert_eq!(w.liste.ordner, bereinigt(&std::path::absolute(&d).unwrap()));
        assert_eq!(w.liste.eintraege.len(), 4);

        // A folder in the path field is opened.
        w.pfad_text = "Zweig".into();
        assert_eq!(w.bestaetigen(), None);
        assert!(w.liste.ordner.ends_with("Zweig"));
        assert!(w.liste.eintraege.is_empty());
        w.hoch();
        assert_eq!(w.liste.ordner, bereinigt(&std::path::absolute(&d).unwrap()));

        // A matching file is the answer, with any case of the extension.
        w.pfad_text = "A.SM2D".into();
        assert_eq!(
            w.bestaetigen(),
            Some(Antwort::Gewaehlt(w.liste.ordner.join("A.SM2D")))
        );

        // A file that does not match the filter or does not exist gives a message.
        w.pfad_text = "notiz.txt".into();
        assert_eq!(w.bestaetigen(), None);
        assert!(w
            .liste
            .fehler
            .as_deref()
            .unwrap()
            .contains("keine .sm2d-Datei"));
        w.liste.alle = true;
        assert!(w.bestaetigen().is_some());
        w.pfad_text = "fehlt.sm2d".into();
        assert_eq!(w.bestaetigen(), None);
        assert!(w
            .liste
            .fehler
            .as_deref()
            .unwrap()
            .starts_with("Datei nicht gefunden"));

        // An unreadable folder keeps the old one.
        let vorher = w.liste.ordner.clone();
        w.wechseln(&d.join("gibt_es_nicht"));
        assert_eq!(w.liste.ordner, vorher);
        assert!(w.liste.fehler.is_some());

        // The footer and an empty listing say whether the filter is on.
        w.liste.alle = false;
        assert_eq!(w.filter_zeile(), "Filter: .sm2d");
        w.wechseln(&d.join("Zweig"));
        assert_eq!(w.leer_text(), "keine Ordner und keine .sm2d-Dateien");
        w.liste.alle = true;
        assert_eq!(w.filter_zeile(), "Filter: aus (alle Dateien)");
        assert_eq!(w.leer_text(), "keine Einträge");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn lange_ordner_nur_sichtbare_zeilen() {
        // 3000 files: only the rows in view are laid out. Counted through AccessKit, which gets a
        // node for every widget laid out, painted or not.
        let d = std::env::temp_dir().join(format!("klon3d-app-viele-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..3000 {
            std::fs::write(d.join(format!("teil_{i:04}.sm2d")), "").unwrap();
        }
        let mut w = Dateiwahl::neu(Zweck::Muster, &d);
        assert_eq!(w.liste.eintraege.len(), 3000);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut zeilen = 0;
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                w.ui(ui.ctx());
            });
            // Unapplied texture deltas panic when dropped in debug builds.
            out.textures_delta.clear();
            let baum = out.platform_output.accesskit_update.expect("AccessKit");
            zeilen = baum
                .nodes
                .iter()
                .filter(|(_, n)| n.label().is_some_and(|l| l.starts_with("teil_")))
                .count();
        }
        assert!(zeilen > 0, "keine Zeile");
        assert!(zeilen < 100, "{zeilen} von 3000 Zeilen angelegt");
        let _ = std::fs::remove_dir_all(&d);
    }
}
