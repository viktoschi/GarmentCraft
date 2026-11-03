//! The simulation runner: a worker thread steps [`Anziehvorgang`] and sends snapshots.
//!
//! One CPU frame of `bluse3_v2` takes about 260–350 ms in a release build, so it must never
//! run inside an egui frame: the window would stand still for a third of a second per step.
//! The thread therefore owns the scene, the solver and the running [`Anziehvorgang`]; the
//! window only sends [`Befehl`]s and keeps the newest [`Schnappschuss`]. No lock is held
//! across a solver frame – the two sides only share two channels.
//!
//! The order of the calls when building is `klon3d-mcp`'s (`src/simulation.rs` there) and, one
//! level up, `klon3d anziehen`: pattern → seam list → mesh → placement list → body → hulls →
//! placement → scene → bonding zones. The mesh and placement step is unrolled instead of
//! [`klon3d_pipeline::szene`], because the point counts and the pieces left out are part of
//! [`Bau`].
//!
//! ## Reset
//!
//! [`Befehl::Zuruecksetzen`] puts back the scene **before** [`Anziehvorgang::beginnen`], which is
//! the scene [`klon3d_pipeline::szene_aus`] gives: frame 0 shows the pieces as laid out, not as
//! pushed out of the body. `beginnen` runs again when the next frame is asked for, so a reset
//! followed by a run gives the same frames as a fresh start.

use std::collections::VecDeque;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use eframe::egui;
use klon3d_pipeline::ablauf::{Anziehvorgang, Phase};
use klon3d_pipeline::{Koerperquelle, Musternetz, Szene};
pub use klon3d_sim::platzierung::Anziehen;
use klon3d_sim::{ImplizitCg, ImplizitParameter, Material};

/// Where the body comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Koerperdatei {
    /// A measurement file (`.smis`), shaped with the reference resizing folder.
    Massdatei(PathBuf),
    /// A `.zprj` project that brings its own body.
    Projekt(PathBuf),
}

impl Koerperdatei {
    /// The file, whichever kind it is.
    pub fn pfad(&self) -> &Path {
        match self {
            Koerperdatei::Massdatei(p) | Koerperdatei::Projekt(p) => p,
        }
    }

    /// Whether the file names a `.zprj` project (by its extension).
    pub fn ist_projekt(&self) -> bool {
        matches!(self, Koerperdatei::Projekt(_))
    }
}

/// What the scene is built from. Equal values mean the same scene, so a running simulation can
/// be kept; anything else needs a new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Vorgaben {
    pub muster: PathBuf,
    /// The measurement file the pattern is evaluated with.
    pub massdatei: PathBuf,
    pub nahtliste: PathBuf,
    pub lage: PathBuf,
    pub koerper: Koerperdatei,
    pub material: Material,
}

impl Vorgaben {
    /// The files as `klon3d anziehen` looks for them: the seam list and the placement list
    /// next to the pattern (`<stem>.naehte`, `<stem>.lage`), the body from `masse` – the
    /// pattern's measurement file unless a `.zprj` was chosen.
    pub fn aus(
        muster: impl Into<PathBuf>,
        massdatei: impl Into<PathBuf>,
        material: Material,
        koerper: Option<Koerperdatei>,
    ) -> Vorgaben {
        let muster = muster.into();
        let massdatei = massdatei.into();
        Vorgaben {
            nahtliste: muster.with_extension("naehte"),
            lage: muster.with_extension("lage"),
            koerper: koerper.unwrap_or_else(|| Koerperdatei::Massdatei(massdatei.clone())),
            muster,
            massdatei,
            material,
        }
    }
}

/// A command from the window to the worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Befehl {
    /// Compute frames back to back.
    Laufen,
    /// Hold after the frame in flight.
    Anhalten,
    /// Compute this many frames, then hold.
    Schritte(usize),
    /// Frame 0 with the positions of the scene as built.
    Zuruecksetzen,
    /// End the thread (pattern change, quit).
    Beenden,
}

/// What the worker reports.
pub enum Meldung {
    /// The scene is built: its description and the positions of frame 0.
    Bereit { bau: Box<Bau>, x: Vec<[f64; 3]> },
    /// One solver frame (numbered from 1) or the frame 0 of a reset.
    Bild(Box<Schnappschuss>),
    /// The thread holds; no frame will follow until the next command.
    Angehalten { bild: usize },
    /// The scene could not be built; the thread has ended.
    Fehler(String),
}

/// The positions of one frame and its numbers.
#[derive(Clone, Debug)]
pub struct Schnappschuss {
    /// Cloth positions in scene coordinates (m), in the order of [`Bau::teile`].
    pub x: Vec<[f64; 3]>,
    /// Frames computed so far (0 = nothing computed yet).
    pub bild: usize,
    pub phase: Phase,
    /// Simulated time (s).
    pub zeit_s: f64,
    /// Solver time over all frames (s).
    pub rechenzeit_s: f64,
    /// Duration of this frame (s), for the frames-per-second display.
    pub dauer_s: f64,
    /// The run is over.
    pub fertig: bool,
}

/// A piece of the mesh, where its points and triangles sit in the scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Teilbereich {
    pub name: String,
    pub punkte: Range<usize>,
    pub dreiecke: Range<usize>,
}

/// The scene as built: what the 3D view needs that does not move with the solver.
#[derive(Clone, Debug)]
pub struct Bau {
    /// Body points in scene coordinates (m).
    pub koerper: Vec<[f64; 3]>,
    pub koerper_dreiecke: Vec<[u32; 3]>,
    /// Cloth triangles, indices into the points of a [`Schnappschuss`].
    pub stoff_dreiecke: Vec<[u32; 3]>,
    pub teile: Vec<Teilbereich>,
    /// Points of the mesh before placement and of the placed pieces.
    pub netz_punkte: usize,
    pub platziert_punkte: usize,
    /// Pieces the placement list left out.
    pub weggelassen: Vec<String>,
    /// Where the body came from, e.g. `<measurements.smis> (Frau 38)`.
    pub herkunft: String,
    pub material: String,
    pub aufbau_s: f64,
}

impl Bau {
    /// The box around body and cloth at frame 0, for the camera: (min, max) in metres.
    pub fn huelle(&self) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &self.koerper {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        (min, max)
    }
}

/// The scene as the worker built it, before the first frame.
struct Erbaut {
    szene: Szene,
    bau: Bau,
}

/// What is missing for these inputs, with the file named for the hover text of the menu entry;
/// `None` when everything is there.
pub fn hindernis(v: &Vorgaben) -> Option<String> {
    let fehlt = |was: &str, p: &Path, hinweis: &str| {
        (!p.is_file()).then(|| {
            let hinweis = if hinweis.is_empty() {
                String::new()
            } else {
                format!(" – {hinweis}")
            };
            format!("{was} fehlt: {}{hinweis}", p.display())
        })
    };
    if !v.muster.is_file() {
        return Some(format!("Muster fehlt: {}", v.muster.display()));
    }
    if let Some(t) = fehlt("Maßdatei", &v.massdatei, "Avatar › Maßdatei wählen…") {
        return Some(t);
    }
    if let Some(t) = fehlt("Nahtliste", &v.nahtliste, "2D-Schnitt › Nähte vorschlagen…") {
        return Some(t);
    }
    if let Some(t) = fehlt("Lagedatei", &v.lage, "") {
        return Some(t);
    }
    if let Koerperdatei::Projekt(p) = &v.koerper {
        if !p.is_file() {
            return Some(format!("CLO-Projekt fehlt: {}", p.display()));
        }
    }
    if !v.koerper.ist_projekt() {
        let ordner = klon3d_koerper::koerper::ordner();
        if !ordner.is_dir() {
            return Some(format!(
                "Körperordner fehlt: {} (Umgebungsvariable CLO_FEMALE)",
                ordner.display()
            ));
        }
    }
    None
}

/// Builds mesh, body, placement and scene – everything up to but without a frame.
fn aufbauen(v: &Vorgaben) -> Result<Erbaut, String> {
    let start = Instant::now();
    let fehler = |e: klon3d_pipeline::Fehler| e.to_string();
    let m = klon3d_pipeline::Muster::laden(&v.muster, Some(&v.massdatei)).map_err(fehler)?;
    let defs = klon3d_pipeline::nahtliste(&v.nahtliste).map_err(fehler)?;
    let netz: Musternetz = m
        .netz(&defs, &klon3d_mesh::Parameter::default())
        .map_err(fehler)?;
    let lage = klon3d_pipeline::lageliste(&v.lage).map_err(fehler)?;
    let ordner = klon3d_koerper::koerper::ordner();
    let quelle = match &v.koerper {
        Koerperdatei::Massdatei(p) => {
            if !ordner.is_dir() {
                return Err(format!(
                    "Körperordner fehlt: {} (Umgebungsvariable CLO_FEMALE)",
                    ordner.display()
                ));
            }
            Koerperquelle::MassdateiMitOrdner(p, ordner.clone())
        }
        Koerperdatei::Projekt(p) => Koerperquelle::CloProjekt(p, ordner.clone()),
    };
    let k = klon3d_pipeline::koerper(quelle).map_err(fehler)?;
    // As `klon3d_pipeline::szene`, unrolled to keep point counts and the pieces left out.
    let h = klon3d_pipeline::huellen(&k).map_err(fehler)?;
    let p = klon3d_pipeline::platzieren(&netz, &lage, &h, true)
        .map_err(|e| format!("{}: {e}", v.lage.display()))?;
    let mut szene = klon3d_pipeline::szene_aus(
        &p,
        &k,
        v.material,
        0.0,
        klon3d_pipeline::standard_simparameter(),
    );
    let zonen = p.klebungen(&szene).into_iter().map(|(z, _)| z).collect();
    szene.klebungen_setzen(zonen);

    let bau = Bau {
        koerper: k.netz.punkte.clone(),
        koerper_dreiecke: k.netz.dreiecke.clone(),
        stoff_dreiecke: szene.dreiecke.clone(),
        teile: szene
            .teile
            .iter()
            .map(|t| Teilbereich {
                name: t.name.clone(),
                punkte: t.punkte.clone(),
                dreiecke: t.dreiecke.clone(),
            })
            .collect(),
        netz_punkte: netz.teile.iter().map(|t| t.punkte.len()).sum(),
        platziert_punkte: p.netz.teile.iter().map(|t| t.punkte.len()).sum(),
        weggelassen: p.weggelassen.clone(),
        herkunft: k.herkunft.clone(),
        material: v.material.name.to_string(),
        aufbau_s: start.elapsed().as_secs_f64(),
    };
    Ok(Erbaut { szene, bau })
}

/// The worker's state: the scene, the solver, the run in progress and the commands it follows.
struct Lauf {
    /// The scene as built (frame 0); reset comes back to it.
    basis: Szene,
    szene: Szene,
    rw: ImplizitCg,
    /// The run in progress; `None` before the first frame and after a reset.
    vorgang: Option<Anziehvorgang<ImplizitCg>>,
    a: Anziehen,
    bild: usize,
    fertig: bool,
    rechenzeit_s: f64,
    laufen: bool,
    rest: usize,
    meldungen: Sender<Meldung>,
    ctx: egui::Context,
}

impl Lauf {
    fn schnappschuss(&self, dauer_s: f64) -> Schnappschuss {
        Schnappschuss {
            x: self.szene.x.clone(),
            bild: self.bild,
            phase: self.vorgang.as_ref().map_or(Phase::Naehen, |v| v.phase()),
            zeit_s: self.szene.zeit,
            rechenzeit_s: self.rechenzeit_s,
            dauer_s,
            fertig: self.fertig,
        }
    }

    fn senden(&self, dauer_s: f64) {
        let _ = self
            .meldungen
            .send(Meldung::Bild(Box::new(self.schnappschuss(dauer_s))));
        self.ctx.request_repaint();
    }

    /// One solver frame; sends its snapshot. `false` when the run is over.
    fn rechnen(&mut self) -> bool {
        if self.fertig {
            return false;
        }
        if self.vorgang.is_none() {
            // `beginnen` pushes the points out of the body and opens all seams; after a reset
            // this is where the run continues from frame 0.
            self.vorgang = Some(Anziehvorgang::beginnen(&mut self.szene, &self.rw, &self.a));
        }
        let start = Instant::now();
        let naechstes = self
            .vorgang
            .as_mut()
            .and_then(|v| v.schritt(&mut self.szene, &mut self.rw));
        let dauer = start.elapsed().as_secs_f64();
        self.rechenzeit_s += dauer;
        match naechstes {
            Some(b) => {
                self.bild = b;
                self.senden(dauer);
                true
            }
            None => {
                // The call that returns `None` computed no frame: it restored the solver's
                // settings and gravity and stored the result.
                self.fertig = true;
                self.laufen = false;
                self.senden(dauer);
                false
            }
        }
    }

    /// Handles one command; `true` = the thread ends.
    fn befehl(&mut self, b: Befehl) -> bool {
        match b {
            Befehl::Beenden => return true,
            Befehl::Laufen => {
                self.laufen = true;
                self.rest = 0;
            }
            Befehl::Anhalten => {
                if self.laufen || self.rest > 0 {
                    self.laufen = false;
                    self.rest = 0;
                    let _ = self.meldungen.send(Meldung::Angehalten { bild: self.bild });
                }
            }
            Befehl::Schritte(n) => {
                self.laufen = false;
                self.rest = n;
            }
            Befehl::Zuruecksetzen => {
                self.szene = self.basis.clone();
                self.rw = ImplizitCg::neu(ImplizitParameter::default());
                self.vorgang = None;
                self.bild = 0;
                self.fertig = false;
                self.rechenzeit_s = 0.0;
                self.laufen = false;
                self.rest = 0;
                self.senden(0.0);
            }
        }
        false
    }
}

/// The worker: build once, then step on command until told to end.
fn arbeiten(
    v: Vorgaben,
    a: Anziehen,
    befehle: Receiver<Befehl>,
    meldungen: Sender<Meldung>,
    ctx: egui::Context,
) {
    let Erbaut { szene, bau } = match aufbauen(&v) {
        Ok(e) => e,
        Err(text) => {
            let _ = meldungen.send(Meldung::Fehler(text));
            ctx.request_repaint();
            return;
        }
    };
    let x = szene.x.clone();
    // Running right away: the user pressed the button.
    let mut lauf = Lauf {
        basis: szene.clone(),
        szene,
        rw: ImplizitCg::neu(ImplizitParameter::default()),
        vorgang: None,
        a,
        bild: 0,
        fertig: false,
        rechenzeit_s: 0.0,
        laufen: true,
        rest: 0,
        meldungen,
        ctx,
    };
    if lauf
        .meldungen
        .send(Meldung::Bereit {
            bau: Box::new(bau),
            x,
        })
        .is_err()
    {
        return;
    }
    lauf.ctx.request_repaint();

    loop {
        // Between frames: everything that is waiting. Pause and stop act here, so no frame
        // starts after they were given.
        loop {
            match befehle.try_recv() {
                Ok(b) => {
                    if lauf.befehl(b) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if lauf.laufen {
            lauf.rechnen();
        } else if lauf.rest > 0 {
            lauf.rest -= 1;
            lauf.rechnen();
        } else {
            // Idle: wait for the next command without burning the CPU.
            match befehle.recv() {
                Ok(b) => {
                    if lauf.befehl(b) {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    }
}

/// The runner as the window sees it: command channel, newest snapshot, what is running.
pub struct Simulation {
    befehle: Option<Sender<Befehl>>,
    meldungen: Option<Receiver<Meldung>>,
    faden: Option<JoinHandle<()>>,
    /// What was started; another request means a new thread.
    vorgaben: Option<Vorgaben>,
    bau: Option<Bau>,
    bild: Option<Schnappschuss>,
    fehler: Option<String>,
    bereit: bool,
    laeuft: bool,
    /// The scene is built and no run has started yet (frame 0).
    frisch: bool,
    bildzeiten: VecDeque<f64>,
}

/// How many frame durations the frames-per-second of the solver are averaged over.
const BILDZEITEN: usize = 8;

impl Default for Simulation {
    fn default() -> Self {
        Simulation::neu()
    }
}

impl Simulation {
    /// No simulation.
    pub fn neu() -> Simulation {
        Simulation {
            befehle: None,
            meldungen: None,
            faden: None,
            vorgaben: None,
            bau: None,
            bild: None,
            fehler: None,
            bereit: false,
            laeuft: false,
            frisch: false,
            bildzeiten: VecDeque::with_capacity(BILDZEITEN),
        }
    }

    /// Start a run; a running one is stopped first. The worker builds the scene in the
    /// background (a few seconds) and reports [`Meldung::Bereit`] or [`Meldung::Fehler`].
    pub fn starten(&mut self, vorgaben: Vorgaben, a: Anziehen, ctx: &egui::Context) {
        self.beenden();
        let (befehl_sender, befehl_empfaenger) = mpsc::channel();
        let (meldung_sender, meldung_empfaenger) = mpsc::channel();
        let ctx = ctx.clone();
        let fuer_faden = vorgaben.clone();
        let faden = thread::Builder::new()
            .name("klon3d-simulation".into())
            .spawn(move || arbeiten(fuer_faden, a, befehl_empfaenger, meldung_sender, ctx));
        let Ok(faden) = faden else {
            self.fehler = Some("Simulation: kein Faden für den Löser".into());
            return;
        };
        self.befehle = Some(befehl_sender);
        self.meldungen = Some(meldung_empfaenger);
        self.faden = Some(faden);
        self.vorgaben = Some(vorgaben);
        self.bau = None;
        self.bild = None;
        self.fehler = None;
        self.bereit = false;
        self.laeuft = true;
        self.frisch = false;
        self.bildzeiten.clear();
    }

    /// Send a command; without a simulation and after an error nothing happens.
    pub fn befehl(&mut self, b: Befehl) {
        let Some(s) = &self.befehle else { return };
        if s.send(b).is_err() {
            // The worker ended (an error, or it was told to stop).
            self.befehle = None;
            self.laeuft = false;
        } else if b == Befehl::Laufen {
            self.laeuft = true;
            self.frisch = false;
        } else if matches!(b, Befehl::Anhalten | Befehl::Schritte(_)) {
            self.laeuft = false;
            self.frisch = false;
        }
    }

    /// Stop the thread and wait for it: at most one solver frame. Called on a pattern change
    /// and when the window closes.
    pub fn beenden(&mut self) {
        if let Some(s) = self.befehle.take() {
            let _ = s.send(Befehl::Beenden);
        }
        self.meldungen = None;
        if let Some(f) = self.faden.take() {
            let _ = f.join();
        }
        self.laeuft = false;
        self.bereit = false;
    }

    /// Take in what the worker sent since the last frame, keeping the newest snapshot only.
    pub fn abholen(&mut self) {
        let Some(empfaenger) = &self.meldungen else {
            return;
        };
        loop {
            match empfaenger.try_recv() {
                Ok(Meldung::Bereit { bau, x }) => {
                    self.bereit = true;
                    self.frisch = true;
                    // `laeuft` stays as it is: the window set it when it sent `Laufen` or
                    // `Anhalten`, and the command waits in the channel until the build is done.
                    // Setting it to `false` here showed "angehalten" although the worker ran.
                    self.bau = Some(*bau);
                    self.bild = Some(Schnappschuss {
                        x,
                        bild: 0,
                        phase: Phase::Naehen,
                        zeit_s: 0.0,
                        rechenzeit_s: 0.0,
                        dauer_s: 0.0,
                        fertig: false,
                    });
                }
                Ok(Meldung::Bild(b)) => {
                    if b.bild > 0 && b.dauer_s > 0.0 {
                        if self.bildzeiten.len() == BILDZEITEN {
                            self.bildzeiten.pop_front();
                        }
                        self.bildzeiten.push_back(b.dauer_s);
                    }
                    self.bereit = true;
                    if b.fertig {
                        self.laeuft = false;
                    }
                    self.bild = Some(*b);
                }
                Ok(Meldung::Angehalten { bild }) => {
                    self.laeuft = false;
                    if let Some(b) = &mut self.bild {
                        b.bild = b.bild.max(bild);
                    }
                }
                Ok(Meldung::Fehler(text)) => {
                    self.fehler = Some(text);
                    self.bereit = false;
                    self.laeuft = false;
                    self.befehle = None;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.befehle = None;
                    self.laeuft = false;
                    break;
                }
            }
        }
    }

    /// The error of a failed build, once.
    pub fn fehler_nehmen(&mut self) -> Option<String> {
        self.fehler.take()
    }

    /// The error of a failed build, without taking it.
    pub fn fehler(&self) -> Option<&str> {
        self.fehler.as_deref()
    }

    /// A thread is running (it may still be building).
    pub fn eingeschaltet(&self) -> bool {
        self.befehle.is_some()
    }

    /// The scene is built and the window can draw it.
    pub fn bereit(&self) -> bool {
        self.bereit
    }

    /// Frames are being computed.
    pub fn laeuft(&self) -> bool {
        self.laeuft
    }

    /// The run is over (`Phase::Fertig`).
    pub fn fertig(&self) -> bool {
        self.bild.as_ref().is_some_and(|b| b.fertig)
    }

    /// The scene is built, but no frame has been computed since the start or the last reset.
    pub fn frisch(&self) -> bool {
        self.frisch
    }

    pub fn vorgaben(&self) -> Option<&Vorgaben> {
        self.vorgaben.as_ref()
    }

    pub fn bau(&self) -> Option<&Bau> {
        self.bau.as_ref()
    }

    pub fn bild(&self) -> Option<&Schnappschuss> {
        self.bild.as_ref()
    }

    /// The number of the frame shown (0 = as built).
    pub fn bildnummer(&self) -> usize {
        self.bild.as_ref().map_or(0, |b| b.bild)
    }

    pub fn phase(&self) -> Phase {
        self.bild.as_ref().map_or(Phase::Naehen, |b| b.phase)
    }

    /// Solver frames per second, averaged over the last frames; `None` before two of them.
    pub fn bilder_pro_sekunde(&self) -> Option<f64> {
        let summe: f64 = self.bildzeiten.iter().sum();
        (self.bildzeiten.len() >= 2 && summe > 0.0).then(|| self.bildzeiten.len() as f64 / summe)
    }

    /// The phase in German, for the status bar.
    pub fn phase_name(&self) -> &'static str {
        phase_name(self.phase())
    }
}

/// The phase in German.
pub fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Naehen => "Nähen",
        Phase::Einschwingen => "Einschwingen",
        Phase::Halt => "Halten",
        Phase::Fertig => "fertig",
    }
}
