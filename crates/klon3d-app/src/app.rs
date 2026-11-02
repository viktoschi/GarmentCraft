//! The application: state, actions, keyboard shortcuts and persistence.
//!
//! [`App`] implements [`eframe::App`]. Every frame [`App::ui`] reads dropped files and keys, lays
//! out the window ([`crate::fenster`]) and the floating dialogs ([`crate::dialoge`]) and then runs
//! the queued actions.
//!
//! Menu entries, toolbar buttons, keys and clicks do not change the state directly: they queue an
//! [`Aktion`], and [`App::ausfuehren`] runs the queue at the end of the frame. One place therefore
//! decides what a command does, whichever way it was given.
//!
//! The window is built without a native window in the tests: [`App::neu`] only needs a
//! [`eframe::CreationContext`] (kittest makes one), and [`App::muster_laden`] loads a pattern
//! without the file dialog.
//!
//! ## Loading
//!
//! Every load goes through [`App::laden`], which makes the paths absolute (and without `..`)
//! first. When a pattern fails because of its measurement file (not named, missing, unreadable),
//! the request is kept as [`App::wartend`]: "Maßdatei wählen…" (menu, error window, a dropped
//! `.smis`) then applies to that pattern, not to the one still shown. Files dropped together are
//! one request ([`App::dateien_ablegen`]): a pattern is opened with the measurement file and seam
//! list dropped with it. Every pattern loaded goes to the front of the recently opened ones.
//!
//! ## Shortcuts
//!
//! Strg+O, Strg+Q and Esc always act. Letters (F, L, Umschalt+S, Umschalt+Z) and Tab
//! (Tab, Umschalt+Tab, Strg+Tab) only act while no text field has the keyboard and the file dialog
//! is closed – otherwise they are text or move the focus in the dialog.
//!
//! ## Persistence
//!
//! Stored through eframe's storage ([`App::save`]) and read back in [`App::neu`]: the theme (dark
//! by default, [`SCHLUESSEL_THEMA`]), the last folder of the file dialog ([`SCHLUESSEL_ORDNER`]),
//! the window arrangement, the 2D display toggles and the recently opened patterns (keys in
//! [`crate::anordnung`]). A missing key means its default, so the state of an older version
//! starts as before. The theme is applied every frame, not only at start: whoever builds the
//! context may set its own theme after the app was created (kittest does).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Key, KeyboardShortcut, Modifiers, Theme};

use crate::anordnung::{
    self, Anordnung, Anzeige2d, Bereich, Fenster, ZuletztPruefung, SCHLUESSEL_ANORDNUNG,
    SCHLUESSEL_ANZEIGE2D, SCHLUESSEL_ZULETZT,
};
use crate::ansicht2d::Ansicht2d;
use crate::bibliothek::Bibliothek;
use crate::dateiwahl::{auflisten, passt, Dateiwahl, Zweck};
use crate::muster::{
    dateiname, delta4, naehte_vorschlagen, nahtliste_neben, netz_pruefen, Ladeauftrag, Ladefehler,
    Muster, Nahtstand, Nahtvorschlag, Netzauftrag, Netzpruefung,
};
use crate::panele::Reiter;

/// Storage key of the theme (`"dunkel"` or `"hell"`).
pub const SCHLUESSEL_THEMA: &str = "klon3d.thema";
/// Storage key of the last folder of the file dialog.
pub const SCHLUESSEL_ORDNER: &str = "klon3d.ordner";

/// Hover text of every entry that a later task brings.
pub const SPAETER: &str = "folgt in einer späteren Aufgabe";

/// Ctrl+O (Cmd+O on macOS).
pub const KUERZEL_OEFFNEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
/// Ctrl+Q (Cmd+Q on macOS).
pub const KUERZEL_BEENDEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);

/// How many frame intervals the frame rate is averaged over.
const BILDZEITEN: usize = 60;

/// A command, from a menu, a button, a key or a click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Aktion {
    Oeffnen,
    /// Open this pattern (recently opened).
    MusterOeffnen(PathBuf),
    ZuletztLeeren,
    MassdateiWaehlen,
    NahtlisteWaehlen,
    /// Use a file by its kind: a pattern is opened, a measurement file or seam list applies to
    /// [`App::ladeziel`] – as if it had been dropped onto the window.
    DateiVerwenden(PathBuf),
    InformationenZeigen,
    /// Mesh the pattern shown, check the mesh and show the report.
    NetzPruefen,
    Beenden,
    Einpassen,
    LageUmschalten,
    /// `true` = laid out, `false` = construction coordinates.
    LageSetzen(bool),
    NaehteUmschalten,
    LaengenUmschalten,
    TeilnamenUmschalten,
    MassstabUmschalten,
    FadenlaufUmschalten,
    PasszeichenUmschalten,
    /// Suggest the seams of the pattern shown and ask where to save the list.
    NaehteVorschlagen,
    ThemaUmschalten,
    ThemaSetzen(Theme),
    NaechstesTeil,
    VorigesTeil,
    AuswahlAufheben,
    TeilWaehlen(usize),
    NahtWaehlen(usize),
    NahtlaengenZeigen,
    /// Show or hide a panel, window or toolbar.
    BereichUmschalten(Bereich),
    /// Make the other of the 3D and 2D window the active one (both must be shown).
    FensterWechseln,
    FensterAktivieren(Fenster),
    /// All panels, windows and toolbars shown, default sizes and divider.
    AnordnungZuruecksetzen,
    /// Show the library at the fabrics folder.
    StoffbibliothekZeigen,
    /// Show a tab of the object browser.
    ReiterZeigen(Reiter),
    /// Show the last error message in the error window again.
    FehlerZeigen,
    TastenkuerzelZeigen,
    UeberZeigen,
}

/// The help window shown, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hilfe {
    Tastenkuerzel,
    Ueber,
}

/// The application.
pub struct App {
    /// Dark or light; dark unless the storage says otherwise.
    pub thema: Theme,
    /// What was loaded last (successfully).
    pub auftrag: Option<Ladeauftrag>,
    /// A request that failed because of its measurement file. A measurement file or seam list
    /// chosen afterwards applies to it ([`App::ladeziel`]) until a load succeeds or fails for
    /// another reason.
    pub wartend: Option<Ladeauftrag>,
    /// The error window offers to choose a measurement file for [`App::wartend`].
    pub massdatei_anbieten: bool,
    pub muster: Option<Muster>,
    /// Selected piece (index into `muster.teile`).
    pub gewaehlt: Option<usize>,
    /// Seam selected in the seam list (index into the seam report).
    pub naht_gewaehlt: Option<usize>,
    /// The properties show the selected seam rather than the selected piece: the seam was
    /// chosen last.
    pub naht_in_eigenschaften: bool,
    pub ansicht: Ansicht2d,
    /// Search text of the object browser; filters pieces and seams.
    pub suche: String,
    /// The tab of the object browser shown.
    pub reiter: Reiter,
    /// The library panel.
    pub bibliothek: Bibliothek,
    /// The last message for the status bar, and whether it is an error.
    pub meldung: String,
    pub meldung_fehler: bool,
    /// Text of the error window while it is open.
    pub fehlerfenster: Option<String>,
    pub dateiwahl: Option<Dateiwahl>,
    /// The seam suggestion waiting for the save dialog ("Nähte vorschlagen…").
    pub nahtvorschlag: Option<Nahtvorschlag>,
    pub hilfe: Option<Hilfe>,
    /// The window "Informationen" about the loaded files is open.
    pub informationen: bool,
    /// The mesh check window is open with this result, for the files it was computed from. It
    /// closes when the pattern shown changes.
    pub netzpruefung: Option<(Netzauftrag, Result<Netzpruefung, String>)>,
    /// Folder the file dialog starts in.
    pub letzter_ordner: PathBuf,
    /// Recently opened patterns, newest first, at most [`anordnung::ZULETZT_MAX`].
    pub zuletzt: Vec<PathBuf>,
    /// Which of them still exist, checked in the background when the Datei menu opens.
    pub zuletzt_pruefung: ZuletztPruefung,
    /// The Datei menu was open in the last frame.
    pub(crate) datei_menue_offen: bool,
    /// Which panels, windows and toolbars are shown.
    pub anordnung: Anordnung,
    /// The 3D or the 2D window.
    pub aktiv: Fenster,
    /// The fabrics folder (`assets/stoffe`), if one was found at start.
    pub stoffordner: Option<PathBuf>,
    /// File names of the fabrics (`.zfab`) in the fabrics folder, read at start.
    pub stoffe: Vec<String>,
    /// Set by "Beenden"; the window is asked to close.
    pub beenden_angefordert: bool,
    /// Scroll the seam list into view in the next frame.
    pub(crate) nahtliste_zeigen: bool,
    /// Drop the panel sizes egui keeps, so that the panels start at their default sizes again.
    pub(crate) panelgroessen_vergessen: bool,
    aktionen: Vec<Aktion>,
    bildzeiten: VecDeque<f64>,
    letzte_zeit: Option<f64>,
    gpu: Option<String>,
    titel: Option<String>,
    /// The start size was compared with the monitor ([`App::groesse_pruefen`]).
    groesse_geprueft: bool,
}

impl App {
    /// Build the app; reads the stored settings if there is a storage.
    pub fn neu(cc: &eframe::CreationContext<'_>) -> App {
        let app = App::mit_speicher(cc.storage);
        cc.egui_ctx.set_theme(app.thema);
        app
    }

    /// Build the app from an optional storage (no egui context needed). Keys that are missing –
    /// a first start, or the state of an older version – mean their defaults.
    pub fn mit_speicher(speicher: Option<&dyn eframe::Storage>) -> App {
        let lies = |schluessel: &str| speicher.and_then(|s| s.get_string(schluessel));
        let thema = match lies(SCHLUESSEL_THEMA).as_deref() {
            Some("hell") => Theme::Light,
            _ => Theme::Dark,
        };
        let letzter_ordner = lies(SCHLUESSEL_ORDNER)
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut ansicht = Ansicht2d::default();
        ansicht.anzeige = Anzeige2d::aus_text(&lies(SCHLUESSEL_ANZEIGE2D).unwrap_or_default());
        let stoffordner = anordnung::stoffordner();
        let stoffe = stoffordner
            .as_deref()
            .and_then(|o| auflisten(o, &["zfab"], false).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|e| !e.ordner)
            .map(|e| e.name)
            .collect();
        let mut app = App {
            thema,
            auftrag: None,
            wartend: None,
            massdatei_anbieten: false,
            muster: None,
            gewaehlt: None,
            naht_gewaehlt: None,
            naht_in_eigenschaften: false,
            ansicht,
            suche: String::new(),
            reiter: Reiter::Teile,
            bibliothek: Bibliothek::neu(&letzter_ordner),
            meldung: "Bereit – Datei › Öffnen… (Strg+O)".into(),
            meldung_fehler: false,
            fehlerfenster: None,
            dateiwahl: None,
            nahtvorschlag: None,
            hilfe: None,
            informationen: false,
            netzpruefung: None,
            letzter_ordner,
            zuletzt: anordnung::zuletzt_aus_text(&lies(SCHLUESSEL_ZULETZT).unwrap_or_default()),
            zuletzt_pruefung: ZuletztPruefung::default(),
            datei_menue_offen: false,
            anordnung: Anordnung::aus_text(&lies(SCHLUESSEL_ANORDNUNG).unwrap_or_default()),
            aktiv: Fenster::Zwei,
            stoffordner,
            stoffe,
            beenden_angefordert: false,
            nahtliste_zeigen: false,
            panelgroessen_vergessen: false,
            aktionen: Vec::new(),
            bildzeiten: VecDeque::with_capacity(BILDZEITEN),
            letzte_zeit: None,
            gpu: None,
            titel: None,
            groesse_geprueft: false,
        };
        // The stored arrangement may hide the 2D window: then the 3D window is the active one.
        app.aktiv_pruefen();
        app
    }

    /// Load a pattern with the measurement file it names and the seam list next to it.
    /// Returns whether it worked; on failure the message is in the status bar and the error
    /// window, and the previous pattern stays.
    pub fn muster_laden(&mut self, pfad: impl AsRef<Path>) -> bool {
        self.laden(Ladeauftrag::neu(pfad.as_ref()))
    }

    /// Load with explicitly chosen files (see [`Ladeauftrag`]). The paths are made absolute and
    /// freed of `.` and `..` first ([`Ladeauftrag::absolut`]), so that the folder remembered from
    /// them stays valid in another working folder and one file is one entry of the recently
    /// opened ones, however it was reached.
    pub fn laden(&mut self, auftrag: Ladeauftrag) -> bool {
        let auftrag = auftrag.absolut();
        match Muster::laden(&auftrag) {
            Ok(m) => {
                let naehte = match &m.naehte {
                    Nahtstand::Geprueft { bericht, .. } => {
                        format!("{} Nähte", bericht.naehte.len())
                    }
                    Nahtstand::Keine { .. } => "keine Nahtliste".into(),
                    Nahtstand::Fehler { .. } => "Nahtliste fehlerhaft".into(),
                };
                self.melden(
                    format!(
                        "{} geladen mit {}: {} Teile, {naehte}",
                        m.dateiname(),
                        m.massdatei_name(),
                        m.teile.len()
                    ),
                    false,
                );
                if let Some(o) = auftrag.muster.parent().filter(|o| o.is_dir()) {
                    self.letzter_ordner = o.to_path_buf();
                }
                anordnung::zuletzt_vorne(&mut self.zuletzt, &auftrag.muster);
                self.zuletzt_pruefung.setzen(&auftrag.muster, true);
                self.titel = Some(format!("klon3d – {}", m.dateiname()));
                self.muster = Some(m);
                self.auftrag = Some(auftrag);
                self.wartend = None;
                self.fehlerfenster = None;
                self.massdatei_anbieten = false;
                self.gewaehlt = None;
                self.naht_gewaehlt = None;
                self.naht_in_eigenschaften = false;
                self.nahtliste_zeigen = false;
                self.ansicht.einpassen();
                true
            }
            Err(e) => {
                self.fehler(e.text());
                if matches!(e, Ladefehler::Massdatei(_)) {
                    self.wartend = Some(auftrag);
                    self.massdatei_anbieten = true;
                } else {
                    self.wartend = None;
                }
                false
            }
        }
    }

    /// The request a chosen measurement file or seam list applies to: the one waiting for a
    /// measurement file, otherwise the one loaded.
    pub fn ladeziel(&self) -> Option<&Ladeauftrag> {
        self.wartend.as_ref().or(self.auftrag.as_ref())
    }

    /// Load `muster` – without one the pattern of [`App::ladeziel`] again – with the measurement
    /// file and seam list given; one not given stays as the request has it.
    fn mit_dateien(
        &mut self,
        muster: Option<PathBuf>,
        massdatei: Option<PathBuf>,
        nahtliste: Option<PathBuf>,
    ) -> bool {
        let mut auftrag = match muster {
            Some(m) => Ladeauftrag::neu(m),
            None => match self.ladeziel() {
                Some(a) => a.clone(),
                None => {
                    self.melden("Erst ein Muster öffnen", true);
                    return false;
                }
            },
        };
        if massdatei.is_some() {
            auftrag.massdatei = massdatei;
        }
        if nahtliste.is_some() {
            auftrag.nahtliste = nahtliste;
        }
        self.laden(auftrag)
    }

    /// Files dropped onto the window together, as one request: a pattern is opened with the
    /// measurement file and seam list dropped with it; without a pattern they apply to
    /// [`App::ladeziel`]. Other files are refused with a message.
    pub fn dateien_ablegen(&mut self, pfade: Vec<PathBuf>) {
        let (mut muster, mut mass, mut naht, mut fremd) = (vec![], vec![], vec![], vec![]);
        for p in pfade {
            if passt(&p, Zweck::Muster.endungen()) {
                muster.push(p);
            } else if passt(&p, Zweck::Massdatei.endungen()) {
                mass.push(p);
            } else if passt(&p, Zweck::Nahtliste.endungen()) {
                naht.push(p);
            } else {
                fremd.push(p);
            }
        }
        let mehrfach: Vec<&str> = [
            (muster.len(), "Muster"),
            (mass.len(), "Maßdateien"),
            (naht.len(), "Nahtlisten"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 1)
        .map(|(_, art)| art)
        .collect();
        if !mehrfach.is_empty() {
            self.fehler(format!(
                "Mehrere {} auf einmal abgelegt – bitte höchstens ein Muster, eine Maßdatei und \
                 eine Nahtliste",
                mehrfach.join(" und ")
            ));
            return;
        }
        let geladen = (!muster.is_empty() || !mass.is_empty() || !naht.is_empty())
            .then(|| self.mit_dateien(muster.pop(), mass.pop(), naht.pop()));
        if !fremd.is_empty() {
            let namen: Vec<String> = fremd.iter().map(|p| p.display().to_string()).collect();
            let hinweis = format!(
                "{}: kein Muster (.sm2d), keine Maßdatei (.smis) und keine Nahtliste (.naehte)",
                namen.join(", ")
            );
            // A failed load keeps its own message (and its offer) in front.
            match (geladen, self.fehlerfenster.as_mut()) {
                (Some(false), Some(f)) => {
                    f.push('\n');
                    f.push_str(&hinweis);
                }
                _ => self.fehler(hinweis),
            }
        }
    }

    /// Show `text` in the status bar.
    pub fn melden(&mut self, text: impl Into<String>, fehler: bool) {
        self.meldung = text.into();
        self.meldung_fehler = fehler;
    }

    /// Show an error in the status bar and in the error window.
    pub fn fehler(&mut self, text: impl Into<String>) {
        let t = text.into();
        self.fehlerfenster = Some(t.clone());
        self.massdatei_anbieten = false;
        self.melden(t, true);
    }

    /// Queue a command for the end of the frame.
    pub fn aktion(&mut self, a: Aktion) {
        self.aktionen.push(a);
    }

    /// Frames per second, averaged over the last frame intervals; `None` before two frames.
    pub fn bilder_pro_sekunde(&self) -> Option<f64> {
        let summe: f64 = self.bildzeiten.iter().sum();
        (self.bildzeiten.len() >= 2 && summe > 0.0).then(|| self.bildzeiten.len() as f64 / summe)
    }

    /// "GPU: <adapter> (<backend>)" or "GPU: –" without a wgpu renderer.
    pub fn gpu_text(&self) -> &str {
        self.gpu.as_deref().unwrap_or("GPU: –")
    }

    /// Is the window `f` shown?
    pub fn fenster_sichtbar(&self, f: Fenster) -> bool {
        match f {
            Fenster::Drei => self.anordnung.fenster3d,
            Fenster::Zwei => self.anordnung.fenster2d,
        }
    }

    /// Keep the active window a shown one: when it was hidden, the other one takes over.
    fn aktiv_pruefen(&mut self) {
        let anderes = match self.aktiv {
            Fenster::Drei => Fenster::Zwei,
            Fenster::Zwei => Fenster::Drei,
        };
        if !self.fenster_sichtbar(self.aktiv) && self.fenster_sichtbar(anderes) {
            self.aktiv = anderes;
        }
    }

    /// Run one command.
    pub fn ausfuehren(&mut self, a: Aktion, ctx: &egui::Context) {
        let teile = self.muster.as_ref().map_or(0, |m| m.teile.len());
        match a {
            Aktion::Oeffnen => {
                self.dateiwahl = Some(Dateiwahl::neu(Zweck::Muster, &self.letzter_ordner))
            }
            Aktion::MusterOeffnen(p) => {
                // A recent file that failed may be gone: the menu learns it from a new check.
                if !self.mit_dateien(Some(p), None, None) {
                    self.zuletzt_pruefung.starten(&self.zuletzt);
                }
            }
            Aktion::ZuletztLeeren => self.zuletzt.clear(),
            Aktion::MassdateiWaehlen | Aktion::NahtlisteWaehlen => {
                let Some(auftrag) = self.ladeziel() else {
                    self.melden("Erst ein Muster öffnen", true);
                    return;
                };
                let zweck = if a == Aktion::MassdateiWaehlen {
                    Zweck::Massdatei
                } else {
                    Zweck::Nahtliste
                };
                let ordner = auftrag
                    .muster
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.letzter_ordner.clone());
                self.dateiwahl = Some(Dateiwahl::neu(zweck, &ordner));
            }
            Aktion::DateiVerwenden(p) => self.dateien_ablegen(vec![p]),
            Aktion::InformationenZeigen => self.informationen = self.muster.is_some(),
            Aktion::NetzPruefen => {
                let Some(m) = &self.muster else {
                    self.melden("Erst ein Muster öffnen", true);
                    return;
                };
                // Synchronous: about 55 ms on Bluse3 v2 in a release build.
                let auftrag = Netzauftrag::zu(m);
                let ergebnis = netz_pruefen(&auftrag);
                match &ergebnis {
                    Ok(n) => self.melden(
                        &format!(
                            "Netzprüfung {}: {} ({:.0} ms)",
                            dateiname(&auftrag.muster),
                            if n.bestanden() {
                                "bestanden"
                            } else {
                                "NICHT bestanden"
                            },
                            n.dauer.as_secs_f64() * 1000.0
                        ),
                        !n.bestanden(),
                    ),
                    Err(e) => self.melden(&format!("Netzprüfung: {e}"), true),
                }
                self.netzpruefung = Some((auftrag, ergebnis));
            }
            Aktion::Beenden => {
                self.beenden_angefordert = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Aktion::Einpassen => self.ansicht.einpassen(),
            Aktion::LageUmschalten => self.ansicht.lage_umschalten(),
            Aktion::LageSetzen(aus) => {
                if self.ansicht.ausgelegt != aus {
                    self.ansicht.lage_umschalten();
                }
            }
            Aktion::NaehteUmschalten => self.ansicht.anzeige.naehte = !self.ansicht.anzeige.naehte,
            Aktion::LaengenUmschalten => {
                self.ansicht.anzeige.laengen = !self.ansicht.anzeige.laengen
            }
            Aktion::TeilnamenUmschalten => {
                self.ansicht.anzeige.teilnamen = !self.ansicht.anzeige.teilnamen
            }
            Aktion::MassstabUmschalten => {
                self.ansicht.anzeige.massstab = !self.ansicht.anzeige.massstab
            }
            Aktion::FadenlaufUmschalten => {
                self.ansicht.anzeige.fadenlauf = !self.ansicht.anzeige.fadenlauf
            }
            Aktion::PasszeichenUmschalten => {
                self.ansicht.anzeige.passzeichen = !self.ansicht.anzeige.passzeichen
            }
            Aktion::NaehteVorschlagen => self.naehte_vorschlagen(),
            Aktion::ThemaUmschalten => {
                self.thema = match self.thema {
                    Theme::Dark => Theme::Light,
                    Theme::Light => Theme::Dark,
                };
            }
            Aktion::ThemaSetzen(t) => self.thema = t,
            Aktion::NaechstesTeil if teile > 0 => {
                self.gewaehlt = Some(self.gewaehlt.map_or(0, |g| (g + 1) % teile));
                self.naht_in_eigenschaften = false;
            }
            Aktion::VorigesTeil if teile > 0 => {
                self.gewaehlt = Some(self.gewaehlt.map_or(teile - 1, |g| (g + teile - 1) % teile));
                self.naht_in_eigenschaften = false;
            }
            Aktion::NaechstesTeil | Aktion::VorigesTeil => {}
            Aktion::AuswahlAufheben => {
                self.gewaehlt = None;
                self.naht_gewaehlt = None;
                self.naht_in_eigenschaften = false;
            }
            Aktion::TeilWaehlen(i) if i < teile => {
                self.gewaehlt = Some(i);
                self.naht_in_eigenschaften = false;
            }
            Aktion::TeilWaehlen(_) => {}
            Aktion::NahtWaehlen(n) => {
                self.naht_gewaehlt = if self.naht_gewaehlt == Some(n) {
                    None
                } else {
                    Some(n)
                };
                self.naht_in_eigenschaften = self.naht_gewaehlt.is_some();
            }
            Aktion::NahtlaengenZeigen => {
                self.anordnung.browser = true;
                self.reiter = Reiter::Naehte;
                self.nahtliste_zeigen = true;
            }
            Aktion::ReiterZeigen(r) => self.reiter = r,
            Aktion::BereichUmschalten(b) => {
                self.anordnung.umschalten(b);
                self.aktiv_pruefen();
            }
            Aktion::FensterWechseln => {
                if self.anordnung.fenster3d && self.anordnung.fenster2d {
                    self.aktiv = match self.aktiv {
                        Fenster::Drei => Fenster::Zwei,
                        Fenster::Zwei => Fenster::Drei,
                    };
                } else {
                    self.aktiv_pruefen();
                }
            }
            Aktion::FensterAktivieren(f) => {
                if self.fenster_sichtbar(f) {
                    self.aktiv = f;
                }
            }
            Aktion::AnordnungZuruecksetzen => {
                self.anordnung = Anordnung::default();
                self.panelgroessen_vergessen = true;
            }
            Aktion::StoffbibliothekZeigen => {
                if let Some(o) = self.stoffordner.clone() {
                    self.anordnung.bibliothek = true;
                    self.bibliothek.wechseln(&o);
                }
            }
            Aktion::FehlerZeigen if self.meldung_fehler => {
                self.fehlerfenster = Some(self.meldung.clone());
            }
            Aktion::FehlerZeigen => {}
            Aktion::TastenkuerzelZeigen => self.hilfe = Some(Hilfe::Tastenkuerzel),
            Aktion::UeberZeigen => self.hilfe = Some(Hilfe::Ueber),
        }
        ctx.request_repaint();
    }

    /// A file chosen in the dialog. Its purpose comes from the dialog, not from the file name:
    /// with "alle Dateien" any file can be chosen.
    pub(crate) fn datei_annehmen(&mut self, zweck: Zweck, pfad: PathBuf) {
        match zweck {
            Zweck::Muster => self.mit_dateien(Some(pfad), None, None),
            Zweck::Massdatei => self.mit_dateien(None, Some(pfad), None),
            Zweck::Nahtliste => self.mit_dateien(None, None, Some(pfad)),
            Zweck::NahtlisteSpeichern => self.vorschlag_speichern(&pfad),
        };
    }

    /// "Nähte vorschlagen…": suggest the seams of the pattern shown and open the save dialog at
    /// `<pattern>.naehte`; the suggestion waits in [`App::nahtvorschlag`].
    fn naehte_vorschlagen(&mut self) {
        let Some(m) = &self.muster else {
            self.melden("Erst ein Muster öffnen", true);
            return;
        };
        match naehte_vorschlagen(m) {
            Ok(v) => {
                let ziel = nahtliste_neben(&m.pfad);
                let ordner = ziel
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.letzter_ordner.clone());
                let text = format!("{} Nähte vorgeschlagen – wohin speichern?", v.naehte);
                self.dateiwahl = Some(Dateiwahl::zum_speichern(
                    Zweck::NahtlisteSpeichern,
                    &ordner,
                    &dateiname(&ziel),
                ));
                self.nahtvorschlag = Some(v);
                self.melden(text, false);
            }
            Err(e) => self.fehler(e),
        }
    }

    /// Write the waiting seam suggestion to `pfad` and load the pattern shown with it, so the
    /// seam tab shows the checked result. Returns whether the list was written and loaded.
    fn vorschlag_speichern(&mut self, pfad: &Path) -> bool {
        let Some(v) = self.nahtvorschlag.take() else {
            self.melden("Kein Nahtvorschlag – 2D-Schnitt › Nähte vorschlagen…", true);
            return false;
        };
        if let Err(e) = std::fs::write(pfad, &v.text) {
            self.fehler(format!("{}: {e}", pfad.display()));
            return false;
        }
        let Some(mut auftrag) = self.auftrag.clone() else {
            return false;
        };
        auftrag.nahtliste = Some(pfad.to_path_buf());
        let geladen = self.laden(auftrag);
        if geladen {
            self.melden(
                format!(
                    "{} Nähte vorgeschlagen und gespeichert: {}",
                    v.naehte,
                    pfad.display()
                ),
                false,
            );
            self.anordnung.browser = true;
            self.reiter = Reiter::Naehte;
            self.nahtliste_zeigen = true;
        }
        geladen
    }

    /// Keyboard shortcuts. Letters and Tab only act while no text field has the keyboard and
    /// the file dialog is closed.
    fn tasten(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&KUERZEL_OEFFNEN)) {
            self.aktion(Aktion::Oeffnen);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KUERZEL_BEENDEN)) {
            self.aktion(Aktion::Beenden);
        }
        // Escape is only read, not consumed: egui closes menus and popups with it as well.
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.dateiwahl.is_some() {
                self.dateiwahl = None;
            } else if self.fehlerfenster.is_some() {
                self.fehlerfenster = None;
            } else if self.hilfe.is_some() {
                self.hilfe = None;
            } else if self.informationen {
                self.informationen = false;
            } else if self.netzpruefung.is_some() {
                self.netzpruefung = None;
            } else {
                self.aktion(Aktion::AuswahlAufheben);
            }
        }
        if self.dateiwahl.is_some() || ctx.text_edit_focused() {
            return;
        }
        let (strg_tab, f, l, zurueck, vor, naehte, laengen) = ctx.input_mut(|i| {
            (
                // Ctrl must match exactly, so plain Tab below does not see Ctrl+Tab.
                i.consume_key(Modifiers::COMMAND, Key::Tab),
                i.consume_key(Modifiers::NONE, Key::F),
                i.consume_key(Modifiers::NONE, Key::L),
                // Shift+Tab first: `consume_key` ignores extra Shift.
                i.consume_key(Modifiers::SHIFT, Key::Tab),
                i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::SHIFT, Key::S),
                i.consume_key(Modifiers::SHIFT, Key::Z),
            )
        });
        if f {
            self.aktion(Aktion::Einpassen);
        }
        if l {
            self.aktion(Aktion::LageUmschalten);
        }
        if naehte {
            self.aktion(Aktion::NaehteUmschalten);
        }
        if laengen {
            self.aktion(Aktion::LaengenUmschalten);
        }
        if strg_tab || zurueck || vor {
            // Tab selects pieces and windows here; egui must not also move the keyboard focus.
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        if strg_tab {
            self.aktion(Aktion::FensterWechseln);
        }
        if zurueck || vor {
            self.aktion(if zurueck {
                Aktion::VorigesTeil
            } else {
                Aktion::NaechstesTeil
            });
        }
    }

    /// Files dropped onto the window in this frame; one drop is one request
    /// ([`App::dateien_ablegen`]).
    fn abgelegte_dateien(&mut self, ctx: &egui::Context) {
        let pfade: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !pfade.is_empty() {
            self.dateien_ablegen(pfade);
        }
    }

    /// Frame intervals for the frame rate.
    fn zeit_messen(&mut self, ctx: &egui::Context) {
        let t = ctx.input(|i| i.time);
        if let Some(v) = self.letzte_zeit {
            let dt = t - v;
            if dt > 0.0 {
                if self.bildzeiten.len() == BILDZEITEN {
                    self.bildzeiten.pop_front();
                }
                self.bildzeiten.push_back(dt);
            }
        }
        self.letzte_zeit = Some(t);
    }

    /// Corrects the window size once at start ([`startkorrektur`]). Sizes the user chooses later
    /// are left alone.
    fn groesse_pruefen(&mut self, ctx: &egui::Context) {
        if self.groesse_geprueft {
            return;
        }
        let (monitor, innen, aussen) = ctx.input(|i| {
            let v = i.viewport();
            (v.monitor_size, v.inner_rect, v.outer_rect)
        });
        let (Some(monitor), Some(innen), Some(aussen)) = (monitor, innen, aussen) else {
            // Not known yet (first frames) or never (test harness): try again next frame.
            return;
        };
        self.groesse_geprueft = true;
        let rand = aussen.size() - innen.size();
        match startkorrektur(innen.size(), rand, monitor) {
            Startkorrektur::Keine => {}
            Startkorrektur::Groesse(g) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(g));
                // The stored position of a minimized window is off screen (-32000).
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    ((monitor - g - rand) / 2.0).max(egui::Vec2::ZERO).to_pos2(),
                ));
            }
            Startkorrektur::Maximieren => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
        }
    }

    /// Adapter name and backend of the wgpu renderer, once.
    fn gpu_erfassen(&mut self, frame: &eframe::Frame) {
        if self.gpu.is_some() {
            return;
        }
        if let Some(rs) = frame.wgpu_render_state() {
            let info = rs.adapter.get_info();
            let backend = match info.backend.to_str() {
                "vulkan" => "Vulkan",
                "metal" => "Metal",
                "dx12" => "DirectX 12",
                "gl" => "OpenGL",
                "webgpu" => "WebGPU",
                andere => andere,
            };
            self.gpu = Some(format!("GPU: {} ({backend})", info.name));
        }
    }

    /// The pattern with the largest seam deviation for the status bar.
    pub fn abweichung_text(&self) -> String {
        let Some(m) = &self.muster else {
            return "Nähte: –".into();
        };
        match (&m.naehte, m.groesste_abweichung()) {
            (_, Some(n)) => format!("größte Abweichung {} Δ {} cm", n.kennung, delta4(n.diff_cm)),
            (Nahtstand::Keine { .. }, None) => "keine Nahtliste".into(),
            (Nahtstand::Fehler { .. }, None) => "Nahtliste fehlerhaft".into(),
            (Nahtstand::Geprueft { .. }, None) => "Nahtliste leer".into(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.zeit_messen(&ctx);
        self.gpu_erfassen(frame);
        self.groesse_pruefen(&ctx);
        if ctx.theme() != self.thema {
            ctx.set_theme(self.thema);
        }
        if let Some(t) = self.titel.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t));
        }
        if std::mem::take(&mut self.panelgroessen_vergessen) {
            ctx.data_mut(|d| d.remove_by_type::<egui::containers::panel::PanelState>());
        }
        self.abgelegte_dateien(&ctx);
        self.tasten(&ctx);
        if self.zuletzt_pruefung.abholen(&self.zuletzt) {
            // Show the result once it is there; the check itself never makes the frame wait.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        crate::fenster::zeichnen(self, ui);
        self.dialoge(&ctx);
        self.ablage_hinweis(&ctx);

        for a in std::mem::take(&mut self.aktionen) {
            self.ausfuehren(a, &ctx);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let thema = match self.thema {
            Theme::Dark => "dunkel",
            Theme::Light => "hell",
        };
        storage.set_string(SCHLUESSEL_THEMA, thema.into());
        storage.set_string(SCHLUESSEL_ORDNER, self.letzter_ordner.display().to_string());
        storage.set_string(SCHLUESSEL_ANORDNUNG, self.anordnung.als_text());
        storage.set_string(SCHLUESSEL_ANZEIGE2D, self.ansicht.anzeige.als_text());
        storage.set_string(
            SCHLUESSEL_ZULETZT,
            anordnung::zuletzt_als_text(&self.zuletzt),
        );
    }
}

/// Inner size of a new window, in points (`main.rs`).
pub const FENSTER_VORGABE: [f32; 2] = [1400.0, 900.0];
/// Smallest inner size of the window, in points (`main.rs`).
pub const FENSTER_MIN: [f32; 2] = [800.0, 500.0];

/// What [`App`] does to the window size in its first frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Startkorrektur {
    Keine,
    /// Set this inner size (points) and center the window.
    Groesse(egui::Vec2),
    Maximieren,
}

/// The correction for a window of inner size `innen` with frame `rand` (outer minus inner) on a
/// monitor of size `monitor`, all in points.
///
/// Two cases needed one:
/// - The default size (1400 x 900) is larger than many laptop screens: on 1536 x 864 points with
///   a taskbar the status bar ended up below the screen edge. Maximizing respects the taskbar on
///   every platform.
/// - eframe stores the geometry of a minimized window (Windows: position -32000, size 0 x 0) and
///   restores it clamped to 64 x 64, ignoring the minimum size. Such a window gets the default
///   size back, or is maximized when that does not fit.
pub fn startkorrektur(innen: egui::Vec2, rand: egui::Vec2, monitor: egui::Vec2) -> Startkorrektur {
    let passt = |g: egui::Vec2| g.x + rand.x <= monitor.x && g.y + rand.y <= monitor.y;
    let zu_klein = innen.x < FENSTER_MIN[0] || innen.y < FENSTER_MIN[1];
    if zu_klein {
        let vorgabe = egui::Vec2::from(FENSTER_VORGABE);
        if passt(vorgabe) {
            Startkorrektur::Groesse(vorgabe)
        } else {
            Startkorrektur::Maximieren
        }
    } else if passt(innen) {
        Startkorrektur::Keine
    } else {
        Startkorrektur::Maximieren
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAND: egui::Vec2 = egui::vec2(16.0, 39.0);

    #[test]
    fn vorgabe_auf_kleinem_bildschirm_wird_maximiert() {
        // 1400 x 900 plus title bar on a 1536 x 864 laptop: too high.
        let innen = egui::Vec2::from(FENSTER_VORGABE);
        assert_eq!(
            startkorrektur(innen, RAND, egui::vec2(1536.0, 864.0)),
            Startkorrektur::Maximieren
        );
        // Too wide only.
        assert_eq!(
            startkorrektur(egui::vec2(1400.0, 600.0), RAND, egui::vec2(1366.0, 768.0)),
            Startkorrektur::Maximieren
        );
    }

    #[test]
    fn passendes_fenster_bleibt() {
        let innen = egui::Vec2::from(FENSTER_VORGABE);
        assert_eq!(
            startkorrektur(innen, RAND, egui::vec2(1920.0, 1080.0)),
            Startkorrektur::Keine
        );
    }

    #[test]
    fn minimiert_gespeichertes_fenster_kommt_zurueck() {
        // eframe restored the stored 0 x 0 as 64 x 64.
        let winzig = egui::vec2(64.0, 64.0);
        assert_eq!(
            startkorrektur(winzig, RAND, egui::vec2(1920.0, 1080.0)),
            Startkorrektur::Groesse(egui::Vec2::from(FENSTER_VORGABE))
        );
        assert_eq!(
            startkorrektur(winzig, RAND, egui::vec2(1536.0, 864.0)),
            Startkorrektur::Maximieren
        );
    }
}
