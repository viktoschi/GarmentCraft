//! Dressing frame by frame.
//!
//! [`klon3d_sim::platzierung::anziehen`] runs the whole sequence in one call:
//! push points out of the body, sew, settle, hold. [`Anziehvorgang`] is the
//! same sequence as a state machine built only on the public items of
//! `klon3d_sim::platzierung` ([`heraus_schieben`], [`Anziehbar::phase`],
//! [`Szene::naht_kuerzen`], [`Rechenweg::bild`]): each [`Anziehvorgang::schritt`]
//! computes exactly one frame, so a window can draw, pause or stop in between.
//! Frame for frame it does the same arithmetic in the same order, so both give
//! identical positions (`tests/kette.rs`).

use klon3d_sim::messung::sinken_je_teil;
use klon3d_sim::platzierung::{heraus_schieben, Angezogen, Anziehbar, Anziehen};
use klon3d_sim::Szene;

/// Phase of the frame computed last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Seams close at limited speed, gravity reduced, body point against
    /// cloth triangle.
    Naehen,
    /// Full gravity until no piece moves any more.
    Einschwingen,
    /// The hold phase (ADR-023).
    Halt,
    /// Done; the solver's settings and gravity are restored.
    Fertig,
}

/// A dressing run in progress. Created with [`Anziehvorgang::beginnen`],
/// advanced with [`Anziehvorgang::schritt`] until it returns `None`.
pub struct Anziehvorgang<R: Anziehbar> {
    a: Anziehen,
    phase: Phase,
    bild: usize,
    vorher: Option<R::Vorgaben>,
    schwerkraft: [f64; 3],
    heraus: usize,
    heraus_tiefe: f64,
    genaeht: usize,
    je_sekunde: usize,
    einschwingbilder: usize,
    ruhig: usize,
    eingeschwungen: bool,
    probe: Vec<[f64; 3]>,
    halt_ab: usize,
    x0: Vec<[f64; 3]>,
    gehalten: usize,
    ergebnis: Option<Angezogen>,
}

impl<R: Anziehbar> Anziehvorgang<R> {
    /// Pushes cloth points out of the body, remembers the solver's settings
    /// and the scene's gravity, opens all seams. No frame is computed yet.
    pub fn beginnen(s: &mut Szene, rw: &R, a: &Anziehen) -> Self {
        let (heraus, heraus_tiefe) = heraus_schieben(s);
        let vorher = Some(rw.vorgaben());
        let schwerkraft = s.param.schwerkraft;
        s.naht_offen();
        Anziehvorgang {
            a: *a,
            phase: Phase::Naehen,
            bild: 0,
            vorher,
            schwerkraft,
            heraus,
            heraus_tiefe,
            genaeht: 0,
            je_sekunde: 1,
            einschwingbilder: 0,
            ruhig: 0,
            eingeschwungen: false,
            probe: Vec::new(),
            halt_ab: 0,
            x0: Vec::new(),
            gehalten: 0,
            ergebnis: None,
        }
    }

    /// Frames computed so far (the number of the last one, from 1).
    pub fn bild(&self) -> usize {
        self.bild
    }

    /// The phase the next frame belongs to as far as known, `Fertig` at the end.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn ablauf(&self) -> &Anziehen {
        &self.a
    }

    /// The result, once [`Anziehvorgang::schritt`] has returned `None`.
    pub fn ergebnis(&self) -> Option<&Angezogen> {
        self.ergebnis.as_ref()
    }

    fn rechne(&mut self, s: &mut Szene, rw: &mut R, naehen: bool) {
        self.bild += 1;
        rw.phase(&self.a, naehen);
        let g = self.schwerkraft;
        let k = self.a.schwerkraft_naehen;
        s.param.schwerkraft = if naehen {
            [g[0] * k, g[1] * k, g[2] * k]
        } else {
            g
        };
        s.naht_kuerzen(self.a.naht_geschwindigkeit * s.param.bild_dt);
        rw.bild(s);
    }

    /// Computes the next frame and returns its number (from 1), or `None`
    /// when the run is over. The call that returns `None` computes no frame;
    /// it restores the solver's settings and gravity and stores the result.
    pub fn schritt(&mut self, s: &mut Szene, rw: &mut R) -> Option<usize> {
        loop {
            match self.phase {
                Phase::Naehen => {
                    if self.genaeht < self.a.naehbilder {
                        self.rechne(s, rw, true);
                        self.genaeht += 1;
                        return Some(self.bild);
                    }
                    self.je_sekunde = (1.0 / s.param.bild_dt).round().max(1.0) as usize;
                    self.probe = s.x.clone();
                    self.phase = Phase::Einschwingen;
                }
                Phase::Einschwingen => {
                    if !self.eingeschwungen && self.einschwingbilder < self.a.einschwingen_max {
                        self.rechne(s, rw, false);
                        self.einschwingbilder += 1;
                        if self.einschwingbilder % self.je_sekunde == 0 {
                            // Height change per piece in the last second, in
                            // absolute value; two calm seconds in a row.
                            let bewegt = sinken_je_teil(s, &self.probe)
                                .iter()
                                .map(|x| x.1.abs())
                                .fold(0.0, f64::max);
                            self.probe.clone_from(&s.x);
                            self.ruhig = if bewegt
                                <= self.a.ruhe_sinken * self.je_sekunde as f64 * s.param.bild_dt
                            {
                                self.ruhig + 1
                            } else {
                                0
                            };
                            if self.einschwingbilder >= self.a.einschwingen_min && self.ruhig >= 2 {
                                self.eingeschwungen = true;
                            }
                        }
                        return Some(self.bild);
                    }
                    self.halt_ab = self.a.naehbilder + self.einschwingbilder;
                    self.x0 = s.x.clone();
                    self.probe = Vec::new();
                    self.phase = Phase::Halt;
                }
                Phase::Halt => {
                    if self.gehalten < self.a.haltebilder {
                        self.rechne(s, rw, false);
                        self.gehalten += 1;
                        return Some(self.bild);
                    }
                    if let Some(v) = self.vorher.take() {
                        rw.setze_vorgaben(v);
                    }
                    s.param.schwerkraft = self.schwerkraft;
                    self.ergebnis = Some(Angezogen {
                        heraus: self.heraus,
                        heraus_tiefe: self.heraus_tiefe,
                        einschwingbilder: self.einschwingbilder,
                        eingeschwungen: self.eingeschwungen,
                        halt_ab: self.halt_ab,
                        sinken: sinken_je_teil(s, &self.x0),
                    });
                    self.x0 = Vec::new();
                    self.phase = Phase::Fertig;
                    return None;
                }
                Phase::Fertig => return None,
            }
        }
    }
}

/// The whole run through [`Anziehvorgang`]: same behaviour and signature as
/// [`klon3d_sim::platzierung::anziehen`]. `nach_bild` sees every frame
/// (number from 1, scene, solver).
pub fn anziehen<R: Anziehbar>(
    s: &mut Szene,
    rw: &mut R,
    a: &Anziehen,
    nach_bild: &mut dyn FnMut(usize, &Szene, &R),
) -> Angezogen {
    let mut v = Anziehvorgang::beginnen(s, rw, a);
    while let Some(bild) = v.schritt(s, rw) {
        nach_bild(bild, s, rw);
    }
    v.ergebnis
        .take()
        .expect("the run sets its result when it ends")
}
