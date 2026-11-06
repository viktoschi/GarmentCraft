//! Moving a dressed garment: both arms go up, the body deforms with them, the
//! cloth follows frame by frame.
//!
//! The garment is handed in already dressed ([`crate::szene`]); the body net of
//! the bind pose is taken out of that scene. The skeleton then drives the skin:
//! [`Skelett::hebe_arme`] sets the pose, [`Skelett::verforme`] moves the skin
//! points, [`Koerpernetz::nachfuehren`] gives the body its collision data for
//! the new pose, [`Koerpernetz::setze_punktgeschwindigkeit`] says how fast the
//! skin moves there, and one solver frame runs against that body.
//!
//! The skin the garment is checked against is the **same** mesh the skeleton
//! deforms; a scene whose body comes from a different mesh is an error here,
//! not a silently wrong index.
//!
//! Frame 0 is the state as handed in (angle 0, nothing solved). Every frame
//! after that is one solver step; the angle is simply `grad * bild / bilder`,
//! there is no easing between the skin poses.

use std::path::Path;

use klon3d_koerper::skelett::Skelett;
use klon3d_sim::netzkoerper::{clo_nach_szene, Koerpernetz, SUCHTIEFE};
use klon3d_sim::{abstand, ausgabe, dehnung, messung, Koerper, Rechenweg, Szene};

use crate::{Fehler, GeladenerKoerper, Musternetz};

/// A seam pair counts as open above this distance (m) — the same limit the
/// command line uses in its table.
pub const OFFEN: f64 = 0.01;

/// What one frame looked like, for the caller's table and file.
#[derive(Debug, Clone, PartialEq)]
pub struct Bildbericht {
    /// Frame number, from 0 (the state as handed in).
    pub bild: usize,
    /// Arm angle of this frame, degrees.
    pub grad: f64,
    /// Deepest cloth point against the skin **of this frame** (m, negative
    /// inside).
    pub tiefster: f64,
    /// Seam pairs further apart than [`OFFEN`].
    pub offene_nahtpaare: usize,
}

/// Raises both arms from 0 to `grad` degrees in `bilder` equal steps and steps
/// the solver once per frame.
///
/// `s` is the dressed scene and is left in the last pose; `rw` is the solver
/// that steps it (implicit, GPU or XPBD — the caller's choice). `sk` must carry
/// the weights of the very mesh the scene's body was built from; `netz` is that
/// mesh's pattern net, needed for the seam pairs.
///
/// `nach_bild` sees every frame including frame 0, after that frame has been
/// solved and with the body of that frame already in the scene.
pub fn bewegen(
    s: &mut Szene,
    rw: &mut dyn Rechenweg,
    sk: &mut Skelett,
    netz: &Musternetz,
    bilder: usize,
    grad: f64,
    nach_bild: &mut dyn FnMut(&Bildbericht, &Szene),
) -> Result<(), Fehler> {
    let k0 = binde_netz(s)?;
    let binde_mm: Vec<[f64; 3]> = k0.punkte.iter().map(|&q| szene_nach_clo(q)).collect();
    // The skeleton deforms its own point list; index i of that list must be
    // index i of the body net, otherwise the skin is deformed in one order and
    // the collision runs against another.
    if binde_mm.len() != sk.gewichte.len() {
        return Err(Fehler::Koerper(format!(
            "das Skelett verformt {} Punkte, der Körper der Szene hat {} — Skelett und Körper gehören nicht zum selben Netz",
            sk.gewichte.len(),
            binde_mm.len()
        )));
    }
    // Velocity is a difference of scene-frame positions, so the previous state
    // is kept in that frame.
    let mut vorher: Vec<[f64; 3]> = k0.punkte.clone();
    let dt = s.param.bild_dt;

    nach_bild(&bericht(s, netz, 0, 0.0)?, s);
    if bilder == 0 {
        return Ok(());
    }

    for bild in 1..=bilder {
        let winkel = grad * bild as f64 / bilder as f64;
        sk.hebe_arme(winkel);
        let posed_mm = sk.verforme(&binde_mm).ok_or_else(|| {
            Fehler::Koerper(
                "das Skelett kann dieses Netz nicht verformen (eine Binde-Matrix ist nicht starr)".into(),
            )
        })?;
        let posed: Vec<[f64; 3]> = posed_mm.iter().map(|&q| clo_nach_szene(q)).collect();
        // Speed of the skin per point (m/s). Without it the solver would treat
        // the arm as standing still and drag the sleeve with friction over a
        // surface that is in fact sliding away.
        let v: Vec<[f64; 3]> = posed
            .iter()
            .zip(&vorher)
            .map(|(a, b)| [(a[0] - b[0]) / dt, (a[1] - b[1]) / dt, (a[2] - b[2]) / dt])
            .collect();
        // One clone per frame: `Koerper::netz` takes the net by value, and
        // `nachfuehren` keeps the grid box of the bind pose. That cost belongs
        // to the body share and is reported there, not hidden.
        let mut neu = k0.clone();
        neu.nachfuehren(&posed)
            .map_err(|e| Fehler::Koerper(format!("Körper nachführen: {e}")))?;
        neu.setze_punktgeschwindigkeit(v)
            .map_err(|e| Fehler::Koerper(format!("Körpergeschwindigkeit: {e}")))?;
        s.koerper = vec![Koerper::netz(neu)];
        vorher = posed;

        rw.bild(s);
        nach_bild(&bericht(s, netz, bild, winkel)?, s);
    }
    Ok(())
}

/// The body net of the bind pose out of the scene: exactly one body, and it has
/// to be a net — a cylinder has no points to deform.
fn binde_netz(s: &Szene) -> Result<Koerpernetz, Fehler> {
    match s.koerper.as_slice() {
        [Koerper::Netz { netz, .. }] => Ok((**netz).clone()),
        [] => Err(Fehler::Koerper(
            "die Szene hat keinen Körper; ohne Haut lässt sich nichts bewegen".into(),
        )),
        [Koerper::Zylinder { .. }] => Err(Fehler::Koerper(
            "der Körper ist ein Zylinder, kein Netz; ein Zylinder lässt sich nicht verformen".into(),
        )),
        k => Err(Fehler::Koerper(format!(
            "die Szene hat {} Körper; erwartet wird genau einer",
            k.len()
        ))),
    }
}

/// Inverse of [`clo_nach_szene`]: scene frame (m, z up) back to the body frame
/// (mm, y up, front +z).
///
/// `clo_nach_szene` maps `(x, y, z) ↦ (x, −z, y)·10⁻³`, so this is
/// `(x, y, z) ↦ (x, z, −y)·10³`.
fn szene_nach_clo(q: [f64; 3]) -> [f64; 3] {
    [q[0] * 1e3, q[2] * 1e3, -q[1] * 1e3]
}

/// The deepest point against this frame's skin and the seam pairs still apart.
fn bericht(s: &Szene, netz: &Musternetz, bild: usize, grad: f64) -> Result<Bildbericht, Fehler> {
    let karte = abstand::Karte::messe(s).ok_or_else(|| {
        Fehler::Koerper("die Szene hat keinen Körper; es gibt keinen Abstand zur Haut".into())
    })?;
    let tiefster = karte.tiefster().map_or(f64::INFINITY, |(d, _)| d);
    let offene_nahtpaare = messung::nahtabstaende(s, netz)
        .iter()
        .map(|(_, d)| d.iter().filter(|&&x| x > OFFEN).count())
        .sum();
    Ok(Bildbericht {
        bild,
        grad,
        tiefster,
        offene_nahtpaare,
    })
}

/// The body of the motion path: the MakeHuman mesh at `hoehe_mm` with its
/// joints, the arrangement taken from the net itself, and that very net as the
/// collision body.
///
/// The three steps belong together — [`Skelett::verforme`] moves exactly the
/// points [`Skelett::baue`] returned, and the scene has to collide against
/// those same points, index for index. A body shaped from a measurement file
/// has a different point list and is therefore not usable here; that is why
/// [`bewegen`] refuses a scene with a foreign body instead of guessing.
pub fn koerper_mit_skelett(mh: &Path, hoehe_mm: f64) -> Result<(GeladenerKoerper, Skelett), Fehler> {
    let k = |m: String| Fehler::Koerper(m);
    let (netz, sk) = klon3d_koerper::skelett::baue(mh, hoehe_mm)
        .map_err(|e| k(format!("Skelett aus {}: {e}", mh.display())))?;
    let an = klon3d_koerper::anordnung_netz::aus_netz(&netz, 10.0)
        .map_err(|e| k(format!("Anordnung aus dem MakeHuman-Netz: {e}")))?;
    let kn = Koerpernetz::aus_clo_mm(&netz.punkte, &netz.dreiecke, SUCHTIEFE)
        .map_err(|e| k(format!("Körpernetz: {e}")))?;
    let herkunft = format!(
        "MakeHuman {} cm ({} Punkte, {} Gelenke) - dasselbe Netz, das das Skelett verformt",
        hoehe_mm / 10.0,
        netz.punkte.len(),
        sk.gelenke.len()
    );
    Ok((
        GeladenerKoerper {
            netz: kn,
            anordnung: an,
            herkunft,
        },
        sk,
    ))
}

/// Writes the per-frame files of [`bewegen`] into a folder:
/// `bild_NNNN.obj` (mm), `dehnung_NNNN.csv` and `abstand_NNNN.csv`, and — once
/// the motion is complete — the whole thing as `bewegung.glb`.
///
/// It lives here, not in the command line, so that a test can check the very
/// same bytes the command writes — the acceptance asks for that and for a
/// second run being identical. The folder is created on the first file; a
/// missing folder is not an error, an unwritable one is.
pub struct Schreiber {
    ordner: std::path::PathBuf,
    angelegt: bool,
    /// The motion so far, for [`Self::abschluss`]. A glTF carries a morph
    /// target per frame, so the frames cannot be written one by one.
    bewegung: Option<crate::gltf::Bewegung>,
}

impl Schreiber {
    /// Writes into `ordner`, which is created when the first file is written.
    pub fn neu(ordner: impl Into<std::path::PathBuf>) -> Schreiber {
        Schreiber {
            ordner: ordner.into(),
            angelegt: false,
            bewegung: None,
        }
    }

    /// The three paths one frame goes to, in the order they are written.
    pub fn pfade(ordner: &Path, bild: usize) -> [std::path::PathBuf; 3] {
        [
            ordner.join(format!("bild_{bild:04}.obj")),
            ordner.join(format!("dehnung_{bild:04}.csv")),
            ordner.join(format!("abstand_{bild:04}.csv")),
        ]
    }

    /// The one path the whole motion goes to, written by [`Self::abschluss`].
    pub fn glb_pfad(ordner: &Path) -> std::path::PathBuf {
        ordner.join("bewegung.glb")
    }

    /// The folder these files go to.
    pub fn ordner(&self) -> &Path {
        &self.ordner
    }

    /// Where [`Self::abschluss`] writes the motion.
    pub fn bewegung_pfad(&self) -> std::path::PathBuf {
        Self::glb_pfad(&self.ordner)
    }

    /// Motion frames collected so far; frame 0 is the state as handed in.
    pub fn bilder(&self) -> usize {
        self.bewegung.as_ref().map_or(0, |m| m.bilder())
    }

    fn anlegen(&mut self) -> Result<(), Fehler> {
        if !self.angelegt {
            std::fs::create_dir_all(&self.ordner).map_err(|e| {
                Fehler::Datei(format!("{}: {e}", self.ordner.display()))
            })?;
            self.angelegt = true;
        }
        Ok(())
    }

    /// Writes one frame, and keeps it for [`Self::abschluss`]. Measuring happens
    /// here, against the body in `s`, which is the body of that frame.
    pub fn bild(&mut self, b: &Bildbericht, s: &Szene) -> Result<(), Fehler> {
        let datei = |e: std::io::Error, p: &Path| Fehler::Datei(format!("{}: {e}", p.display()));
        self.anlegen()?;
        let [obj, dehn, abst] = Self::pfade(&self.ordner, b.bild);
        std::fs::write(&obj, ausgabe::obj(s)).map_err(|e| datei(e, &obj))?;
        std::fs::write(&dehn, dehnung::Karte::messe(s).csv(s)).map_err(|e| datei(e, &dehn))?;
        let karte = abstand::Karte::messe(s).ok_or_else(|| {
            Fehler::Koerper("die Szene hat keinen Körper; es gibt keinen Abstand zur Haut".into())
        })?;
        std::fs::write(&abst, karte.csv(s)).map_err(|e| datei(e, &abst))?;

        match self.bewegung.as_mut() {
            None => self.bewegung = Some(crate::gltf::Bewegung::neu(s).map_err(glb)?),
            Some(m) => m.bild(s).map_err(glb)?,
        }
        Ok(())
    }

    /// Writes `bewegung.glb` out of the frames collected so far. Called once,
    /// after the last frame; calling it with no frame at all writes nothing.
    pub fn abschluss(&mut self) -> Result<(), Fehler> {
        if self.bewegung.is_none() {
            return Ok(());
        }
        self.anlegen()?;
        let p = Self::glb_pfad(&self.ordner);
        let daten = self.bewegung.as_ref().expect("oben geprüft").glb();
        std::fs::write(&p, daten)
            .map_err(|e| Fehler::Datei(format!("{}: {e}", p.display())))
    }
}

/// A glTF bookkeeping error, named after the file it would have gone to.
fn glb(m: String) -> Fehler {
    Fehler::Datei(format!("bewegung.glb: {m}"))
}
