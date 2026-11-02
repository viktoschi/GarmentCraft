//! What the window remembers between runs, as plain data with unit tests: which panels, windows
//! and toolbars are shown, where the divider between the 3D and the 2D window sits, the display
//! toggles of the 2D view and the recently opened patterns (whether those still exist is checked
//! in the background, [`ZuletztPruefung`]). Also the search for the fabrics folder.
//!
//! Everything is stored through eframe's storage as short texts (`name=wert;…` and one path per
//! line). Reading is forgiving: a missing key, an unknown name or a value that does not parse
//! falls back to the default, so a stored state of an older version – or a damaged one – never
//! stops the start.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::dateiwahl::bereinigt;

/// Storage key of the window arrangement ([`Anordnung::als_text`]).
pub const SCHLUESSEL_ANORDNUNG: &str = "klon3d.anordnung";
/// Storage key of the 2D display toggles ([`Anzeige2d::als_text`]).
pub const SCHLUESSEL_ANZEIGE2D: &str = "klon3d.anzeige2d";
/// Storage key of the recently opened patterns, one absolute path per line, newest first.
pub const SCHLUESSEL_ZULETZT: &str = "klon3d.zuletzt";

/// How many recently opened patterns are kept.
pub const ZULETZT_MAX: usize = 10;

/// Default share of the centre width taken by the 3D window.
pub const TEILUNG_VORGABE: f32 = 0.5;
/// Default share of the right panel's height taken by the object browser (the properties get
/// the rest).
pub const TEILUNG_RECHTS_VORGABE: f32 = 0.6;

/// A part of the window that can be shown or hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bereich {
    Bibliothek,
    Browser,
    Eigenschaften,
    Fenster3d,
    Fenster2d,
    LeisteDatei,
    Leiste3d,
    Leiste2d,
}

impl Bereich {
    /// All parts in menu order, with their name in the stored text and their German label.
    pub const ALLE: [(Bereich, &'static str, &'static str); 8] = [
        (Bereich::Bibliothek, "bibliothek", "Bibliothek"),
        (Bereich::Browser, "browser", "Objekt-Browser"),
        (Bereich::Eigenschaften, "eigenschaften", "Eigenschaften"),
        (Bereich::Fenster3d, "fenster3d", "3D-Fenster"),
        (Bereich::Fenster2d, "fenster2d", "2D-Fenster"),
        (Bereich::LeisteDatei, "leiste_datei", "Datei-Werkzeugleiste"),
        (Bereich::Leiste3d, "leiste_3d", "3D-Werkzeugleiste"),
        (Bereich::Leiste2d, "leiste_2d", "2D-Werkzeugleiste"),
    ];

    /// The label in menus.
    pub fn titel(self) -> &'static str {
        Bereich::ALLE
            .iter()
            .find(|(b, _, _)| *b == self)
            .map_or("", |(_, _, t)| t)
    }
}

/// The window that has the focus of the commands for the 3D or the 2D view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fenster {
    Drei,
    Zwei,
}

/// Which panels, windows and toolbars are shown, and the divider between 3D and 2D.
#[derive(Clone, Debug, PartialEq)]
pub struct Anordnung {
    pub bibliothek: bool,
    pub browser: bool,
    pub eigenschaften: bool,
    pub fenster3d: bool,
    pub fenster2d: bool,
    pub leiste_datei: bool,
    pub leiste_3d: bool,
    pub leiste_2d: bool,
    /// Share of the centre width taken by the 3D window, in `0.0..=1.0`.
    pub teilung: f32,
    /// Share of the right panel's height taken by the object browser, in `0.0..=1.0`.
    pub teilung_rechts: f32,
}

impl Default for Anordnung {
    fn default() -> Self {
        Anordnung {
            bibliothek: true,
            browser: true,
            eigenschaften: true,
            fenster3d: true,
            fenster2d: true,
            leiste_datei: true,
            leiste_3d: true,
            leiste_2d: true,
            teilung: TEILUNG_VORGABE,
            teilung_rechts: TEILUNG_RECHTS_VORGABE,
        }
    }
}

impl Anordnung {
    fn feld(&mut self, b: Bereich) -> &mut bool {
        match b {
            Bereich::Bibliothek => &mut self.bibliothek,
            Bereich::Browser => &mut self.browser,
            Bereich::Eigenschaften => &mut self.eigenschaften,
            Bereich::Fenster3d => &mut self.fenster3d,
            Bereich::Fenster2d => &mut self.fenster2d,
            Bereich::LeisteDatei => &mut self.leiste_datei,
            Bereich::Leiste3d => &mut self.leiste_3d,
            Bereich::Leiste2d => &mut self.leiste_2d,
        }
    }

    /// Is part `b` shown?
    pub fn sichtbar(&self, b: Bereich) -> bool {
        match b {
            Bereich::Bibliothek => self.bibliothek,
            Bereich::Browser => self.browser,
            Bereich::Eigenschaften => self.eigenschaften,
            Bereich::Fenster3d => self.fenster3d,
            Bereich::Fenster2d => self.fenster2d,
            Bereich::LeisteDatei => self.leiste_datei,
            Bereich::Leiste3d => self.leiste_3d,
            Bereich::Leiste2d => self.leiste_2d,
        }
    }

    /// Show or hide part `b`.
    pub fn setzen(&mut self, b: Bereich, an: bool) {
        *self.feld(b) = an;
    }

    /// Show part `b` if hidden, hide it if shown.
    pub fn umschalten(&mut self, b: Bereich) {
        let f = self.feld(b);
        *f = !*f;
    }

    /// `bibliothek=1;browser=1;…;teilung=0.500;teilung_rechts=0.600`
    pub fn als_text(&self) -> String {
        let mut teile: Vec<String> = Bereich::ALLE
            .iter()
            .map(|(b, name, _)| format!("{name}={}", u8::from(self.sichtbar(*b))))
            .collect();
        teile.push(format!("teilung={:.3}", self.teilung));
        teile.push(format!("teilung_rechts={:.3}", self.teilung_rechts));
        teile.join(";")
    }

    /// The arrangement from [`Anordnung::als_text`]; everything missing or unreadable keeps its
    /// default.
    pub fn aus_text(text: &str) -> Anordnung {
        let mut a = Anordnung::default();
        for (name, wert) in paare(text) {
            if let Some((b, _, _)) = Bereich::ALLE.iter().find(|(_, n, _)| *n == name) {
                if let Some(an) = schalter(wert) {
                    a.setzen(*b, an);
                }
            } else if let Some(anteil) = anteil(wert) {
                match name {
                    "teilung" => a.teilung = anteil,
                    "teilung_rechts" => a.teilung_rechts = anteil,
                    _ => {}
                }
            }
        }
        a
    }
}

/// The display toggles of the 2D view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anzeige2d {
    pub naehte: bool,
    pub laengen: bool,
    pub teilnamen: bool,
    pub massstab: bool,
}

impl Default for Anzeige2d {
    fn default() -> Self {
        Anzeige2d {
            naehte: true,
            laengen: false,
            teilnamen: true,
            massstab: true,
        }
    }
}

impl Anzeige2d {
    /// `naehte=1;laengen=0;teilnamen=1;massstab=1`
    pub fn als_text(&self) -> String {
        format!(
            "naehte={};laengen={};teilnamen={};massstab={}",
            u8::from(self.naehte),
            u8::from(self.laengen),
            u8::from(self.teilnamen),
            u8::from(self.massstab)
        )
    }

    /// The toggles from [`Anzeige2d::als_text`]; everything missing or unreadable keeps its
    /// default.
    pub fn aus_text(text: &str) -> Anzeige2d {
        let mut a = Anzeige2d::default();
        for (name, wert) in paare(text) {
            let Some(an) = schalter(wert) else {
                continue;
            };
            match name {
                "naehte" => a.naehte = an,
                "laengen" => a.laengen = an,
                "teilnamen" => a.teilnamen = an,
                "massstab" => a.massstab = an,
                _ => {}
            }
        }
        a
    }
}

/// The `name=wert` pairs of a stored text, blanks around both trimmed.
fn paare(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split(';')
        .filter_map(|p| p.split_once('='))
        .map(|(n, w)| (n.trim(), w.trim()))
}

/// A share in `0.0..=1.0`; values outside are clamped, anything else is `None`.
fn anteil(wert: &str) -> Option<f32> {
    wert.parse::<f32>()
        .ok()
        .filter(|t| t.is_finite())
        .map(|t| t.clamp(0.0, 1.0))
}

fn schalter(wert: &str) -> Option<bool> {
    match wert {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────── recently opened

/// Put `pfad` at the front of the recently opened patterns; an equal entry further back is
/// removed, and the list keeps at most [`ZULETZT_MAX`] entries. Paths are compared without `.`
/// and `..` ([`bereinigt`]).
pub fn zuletzt_vorne(liste: &mut Vec<PathBuf>, pfad: &Path) {
    let pfad = bereinigt(pfad);
    liste.retain(|p| *p != pfad);
    liste.insert(0, pfad);
    liste.truncate(ZULETZT_MAX);
}

/// One path per line, newest first.
pub fn zuletzt_als_text(liste: &[PathBuf]) -> String {
    liste
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The list from [`zuletzt_als_text`]: empty lines and repeats dropped (also the same file
/// reached through `..`, as an earlier version stored it), at most [`ZULETZT_MAX`]. Files that no
/// longer exist stay – the menu shows them disabled.
pub fn zuletzt_aus_text(text: &str) -> Vec<PathBuf> {
    let mut liste: Vec<PathBuf> = Vec::new();
    for zeile in text.lines().map(str::trim).filter(|z| !z.is_empty()) {
        let p = bereinigt(Path::new(zeile));
        if !liste.contains(&p) {
            liste.push(p);
        }
    }
    liste.truncate(ZULETZT_MAX);
    liste
}

/// The menu labels of the recently opened patterns: the file name, and where two entries share
/// it, the name of the folder as well (`name · ordner`).
pub fn zuletzt_beschriftungen(liste: &[PathBuf]) -> Vec<String> {
    let name = |p: &Path| crate::muster::dateiname(p);
    liste
        .iter()
        .map(|p| {
            let n = name(p);
            let doppelt = liste.iter().filter(|q| name(q) == n).count() > 1;
            match p.parent().and_then(Path::file_name) {
                Some(o) if doppelt => format!("{n} · {}", o.to_string_lossy()),
                _ => n,
            }
        })
        .collect()
}

/// Which of the recently opened patterns still exist, checked on a background thread.
///
/// Looking at a file can block: a path on a disconnected network drive waits for the network
/// timeout. The menu therefore never touches the disk itself; it shows what the last check found,
/// and a file not checked yet counts as there (opening it reports a missing file). A check is
/// started when the Datei menu opens, at most one runs at a time, and it reports every file as
/// soon as it has looked at it.
#[derive(Debug, Default)]
pub struct ZuletztPruefung {
    bekannt: HashMap<PathBuf, bool>,
    laeuft: Option<mpsc::Receiver<(PathBuf, bool)>>,
    /// Asked for another check while one was running.
    nochmal: bool,
    /// How many checks were started so far.
    pub gestartet: usize,
}

impl ZuletztPruefung {
    /// Check the files of `liste` in the background. While a check runs, another one follows it
    /// (see [`ZuletztPruefung::abholen`]).
    pub fn starten(&mut self, liste: &[PathBuf]) {
        if self.laeuft.is_some() {
            self.nochmal = true;
            return;
        }
        // Only what the list holds is kept: the map stays as small as the list.
        self.bekannt.retain(|p, _| liste.contains(p));
        if liste.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let liste = liste.to_vec();
        let faden = std::thread::Builder::new()
            .name("klon3d-zuletzt".into())
            .spawn(move || {
                for p in liste {
                    let da = p.is_file();
                    if tx.send((p, da)).is_err() {
                        break;
                    }
                }
            });
        // Without a thread nothing is checked; the entries stay as they were known.
        if faden.is_ok() {
            self.laeuft = Some(rx);
            self.gestartet += 1;
        }
    }

    /// Take over what the running check has found so far, without waiting; returns whether a
    /// check is still running. If another check was asked for meanwhile, it starts on `liste`
    /// once this one is done.
    pub fn abholen(&mut self, liste: &[PathBuf]) -> bool {
        let Some(rx) = &self.laeuft else {
            return false;
        };
        loop {
            match rx.try_recv() {
                Ok((p, da)) => {
                    self.bekannt.insert(p, da);
                }
                Err(mpsc::TryRecvError::Empty) => return true,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        self.laeuft = None;
        if std::mem::take(&mut self.nochmal) {
            self.starten(liste);
        }
        self.laeuft.is_some()
    }

    /// Is a check running?
    pub fn laeuft(&self) -> bool {
        self.laeuft.is_some()
    }

    /// Does `pfad` exist, as far as known? `None`: not checked yet.
    pub fn da(&self, pfad: &Path) -> Option<bool> {
        self.bekannt.get(pfad).copied()
    }

    /// Note what is known without looking (a file just loaded is there).
    pub fn setzen(&mut self, pfad: &Path, da: bool) {
        self.bekannt.insert(pfad.to_path_buf(), da);
    }
}

// ───────────────────────────────────────────────────────── fabrics folder

/// The folder `assets/stoffe` in `start` or in one of the folders above it, if any.
pub fn stoffordner_ab(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|o| o.join("assets").join("stoffe"))
        .find(|p| p.is_dir())
}

/// The fabrics folder: `assets/stoffe` searched upwards from the folder of the program, then
/// from the current folder; the first hit wins.
pub fn stoffordner() -> Option<PathBuf> {
    let programm = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let aktuell = std::env::current_dir().ok();
    [programm, aktuell]
        .into_iter()
        .flatten()
        .find_map(|s| stoffordner_ab(&s))
        .map(|p| std::path::absolute(&p).unwrap_or(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anordnung_hin_und_zurueck() {
        let mut a = Anordnung::default();
        a.umschalten(Bereich::Bibliothek);
        a.setzen(Bereich::Leiste3d, false);
        a.teilung = 0.3;
        a.teilung_rechts = 0.25;
        let b = Anordnung::aus_text(&a.als_text());
        assert_eq!(b, a);
        assert!(!b.sichtbar(Bereich::Bibliothek) && b.sichtbar(Bereich::Browser));
        assert_eq!(Bereich::Fenster2d.titel(), "2D-Fenster");
    }

    #[test]
    fn alter_oder_kaputter_stand_gibt_vorgaben() {
        assert_eq!(Anordnung::aus_text(""), Anordnung::default());
        // Unknown names, unreadable values and a ratio out of range.
        let a = Anordnung::aus_text("gibt_es_nicht=0;browser=ja;teilung=NaN;fenster3d=0");
        assert!(a.browser && !a.fenster3d);
        assert_eq!(a.teilung, TEILUNG_VORGABE);
        assert_eq!(Anordnung::aus_text("teilung=7").teilung, 1.0);
        assert_eq!(Anordnung::aus_text("teilung_rechts=-1").teilung_rechts, 0.0);
        assert_eq!(
            Anordnung::aus_text("teilung_rechts=x").teilung_rechts,
            TEILUNG_RECHTS_VORGABE
        );
        assert_eq!(Anzeige2d::aus_text("unsinn"), Anzeige2d::default());
        let z = Anzeige2d::aus_text(" laengen = 1 ;massstab=0");
        assert!(z.laengen && !z.massstab && z.naehte && z.teilnamen);
        assert_eq!(Anzeige2d::aus_text(&z.als_text()), z);
    }

    #[test]
    fn zuletzt_neueste_vorne_ohne_doppel() {
        let mut l = Vec::new();
        for i in 0..12 {
            zuletzt_vorne(&mut l, Path::new(&format!("/m/{i}.sm2d")));
        }
        assert_eq!(l.len(), ZULETZT_MAX);
        assert_eq!(l[0], PathBuf::from("/m/11.sm2d"));
        zuletzt_vorne(&mut l, Path::new("/m/5.sm2d"));
        assert_eq!(l[0], PathBuf::from("/m/5.sm2d"));
        assert_eq!(l.iter().filter(|p| p.ends_with("5.sm2d")).count(), 1);
        let text = zuletzt_als_text(&l);
        assert_eq!(zuletzt_aus_text(&text), l);
        assert_eq!(
            zuletzt_aus_text("\n/a.sm2d\n\n/a.sm2d\n/b.sm2d"),
            [PathBuf::from("/a.sm2d"), PathBuf::from("/b.sm2d")]
        );
        // One file reached through `..` is one entry.
        assert_eq!(
            zuletzt_aus_text("/x/y/../a.sm2d\n/x/a.sm2d\n/x/./b.sm2d"),
            [PathBuf::from("/x/a.sm2d"), PathBuf::from("/x/b.sm2d")]
        );
        let mut l = vec![PathBuf::from("/x/a.sm2d")];
        zuletzt_vorne(&mut l, Path::new("/x/y/../a.sm2d"));
        assert_eq!(l, [PathBuf::from("/x/a.sm2d")]);
    }

    #[test]
    fn zuletzt_im_hintergrund_pruefen() {
        let d = std::env::temp_dir().join(format!("klon3d-app-pruefen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (da, weg) = (d.join("da.sm2d"), d.join("weg.sm2d"));
        std::fs::write(&da, "x").unwrap();
        let liste = vec![da.clone(), weg.clone()];
        let abwarten = |p: &mut ZuletztPruefung| {
            for _ in 0..500 {
                if !p.abholen(&liste) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("Prüfung endet nicht");
        };
        let mut p = ZuletztPruefung::default();
        assert_eq!(p.da(&da), None, "vor der Prüfung unbekannt");
        p.starten(&liste);
        assert!(p.laeuft());
        // Asked again while it runs: no second thread now, one more check after it.
        p.starten(&liste);
        assert_eq!(p.gestartet, 1);
        abwarten(&mut p);
        assert_eq!(p.gestartet, 2);
        assert_eq!((p.da(&da), p.da(&weg)), (Some(true), Some(false)));
        // A file deleted meanwhile is found by the next check.
        std::fs::remove_file(&da).unwrap();
        p.starten(&liste);
        abwarten(&mut p);
        assert_eq!(p.da(&da), Some(false));
        // An empty list starts nothing and forgets what it knew.
        p.starten(&[]);
        assert!(!p.laeuft() && p.gestartet == 3 && p.da(&weg).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn gleiche_namen_mit_ordner() {
        let l = [
            PathBuf::from("/x/eins/bluse.sm2d"),
            PathBuf::from("/x/zwei/bluse.sm2d"),
            PathBuf::from("/x/zwei/rock.sm2d"),
        ];
        assert_eq!(
            zuletzt_beschriftungen(&l),
            ["bluse.sm2d · eins", "bluse.sm2d · zwei", "rock.sm2d"]
        );
    }

    #[test]
    fn stoffordner_ueber_dem_start() {
        let d = std::env::temp_dir().join(format!("klon3d-app-stoffe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("assets/stoffe")).unwrap();
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        assert_eq!(
            stoffordner_ab(&d.join("a/b")),
            Some(d.join("assets/stoffe"))
        );
        let _ = std::fs::remove_dir_all(&d);
        // The tests run in the crate folder; the repository's fabrics are above it.
        let o = stoffordner().expect("assets/stoffe des Repositorys");
        assert!(o.ends_with("assets/stoffe") && o.is_absolute());
    }
}
