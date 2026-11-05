//! The chain pattern → mesh → body → placement → scene → frame as a library.
//!
//! No argument parsing, no console output: every step returns a `Result` with
//! [`Fehler`], and the caller decides what to print. `klon3d anziehen` and the
//! application window both build their scenes through these functions, so the
//! window shows what the command line computes.
//!
//! Typical use:
//!
//! ```no_run
//! # use std::path::Path;
//! # use klon3d_pipeline::*;
//! let muster = Muster::laden(Path::new("bluse.sm2d"), None)?;
//! let naehte = nahtliste(Path::new("bluse.naehte"))?;
//! let netz = muster.netz(&naehte, &klon3d_mesh::Parameter::default())?;
//! let lage = lageliste(Path::new("bluse.lage"))?;
//! let koerper = koerper(Koerperquelle::Massdatei(muster.massdatei()))?;
//! let mut s = szene(&netz, &lage, &koerper, klon3d_sim::RADICAL_RUBBER_04, 0.0, standard_simparameter())?;
//! let mut rw = klon3d_sim::ImplizitCg::neu(klon3d_sim::ImplizitParameter::default());
//! let a = klon3d_sim::platzierung::Anziehen::default();
//! let mut vorgang = ablauf::Anziehvorgang::beginnen(&mut s, &rw, &a);
//! while vorgang.schritt(&mut s, &mut rw).is_some() {
//!     // draw, check for cancel, ...
//! }
//! # Ok::<(), Fehler>(())
//! ```
//!
//! The placement phase runs frame by frame through [`ablauf::Anziehvorgang`],
//! so a caller can pause or stop between two frames.

pub mod ablauf;
pub mod bewegen;
pub mod dxf;
pub mod stoff;

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use klon3d_koerper::anordnung_clo::{aus_clo_koerper, CloNummern};
use klon3d_pattern::geom::{lies_messungen, Auswertung};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};
use klon3d_pattern::Pattern;
use klon3d_sim::netzkoerper::{clo_nach_szene, SUCHTIEFE};
use klon3d_sim::platzierung::{lies_lage, platziere_mit, Huellen, Koerperteile};
use klon3d_sim::{Koerper, Material, Rechenweg, Teilvorgabe};

pub use klon3d_koerper::koerper::Anordnung;
pub use klon3d_mesh::Musternetz;
pub use klon3d_pattern::naehte::{Nahtbericht, Nahtdef};
pub use klon3d_sim::platzierung::{Lageliste, Platzierung};
pub use klon3d_sim::{Koerpernetz, Simparameter, Szene};

/// What went wrong, by step. The message is complete and printable as is;
/// where the library does not know the file a value came from (seam check,
/// placement), the caller may put the file name in front.
#[derive(Debug, Clone, PartialEq)]
pub enum Fehler {
    /// Pattern or its measurement file not readable.
    Muster(String),
    /// A seam list or placement file not readable or not parseable.
    Datei(String),
    /// The seam list does not fit the pattern.
    Naehte(String),
    /// Meshing failed.
    Netz(String),
    /// The body could not be loaded, shaped or arranged.
    Koerper(String),
    /// The placement file does not fit the mesh or the body.
    Lage(String),
}

impl Fehler {
    /// The message without the kind.
    pub fn meldung(&self) -> &str {
        match self {
            Fehler::Muster(m)
            | Fehler::Datei(m)
            | Fehler::Naehte(m)
            | Fehler::Netz(m)
            | Fehler::Koerper(m)
            | Fehler::Lage(m) => m,
        }
    }
}

impl fmt::Display for Fehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.meldung())
    }
}

impl std::error::Error for Fehler {}

/// A loaded pattern with the measurements it is evaluated with. Owns
/// everything; [`Muster::auswertung`] builds the borrowing evaluation on
/// demand.
pub struct Muster {
    pfad: PathBuf,
    muster: Pattern,
    massdatei: PathBuf,
    mess: HashMap<String, f64>,
}

impl Muster {
    /// Reads the pattern and its measurements: `masse`, or else the file the
    /// pattern names (relative paths next to the pattern).
    pub fn laden(pfad: &Path, masse: Option<&Path>) -> Result<Muster, Fehler> {
        let muster = klon3d_pattern::read_path(pfad)
            .map_err(|e| Fehler::Muster(format!("{}: {e}", pfad.display())))?;
        let massdatei = match masse {
            Some(m) => m.to_path_buf(),
            None => {
                let Some(m) = muster.measurements.as_deref() else {
                    return Err(Fehler::Muster("Muster nennt keine Maßdatei".into()));
                };
                let m = Path::new(m);
                if m.is_absolute() {
                    m.to_path_buf()
                } else {
                    pfad.parent().unwrap_or(Path::new(".")).join(m)
                }
            }
        };
        let mess = lies_messungen(&massdatei, &muster.unit)
            .map_err(|e| Fehler::Muster(format!("{}: {e}", massdatei.display())))?;
        Ok(Muster {
            pfad: pfad.to_path_buf(),
            muster,
            massdatei,
            mess,
        })
    }

    pub fn pfad(&self) -> &Path {
        &self.pfad
    }

    /// The measurement file the pattern is evaluated with.
    pub fn massdatei(&self) -> &Path {
        &self.massdatei
    }

    pub fn muster(&self) -> &Pattern {
        &self.muster
    }

    /// A fresh evaluation of the pattern (empty caches).
    pub fn auswertung(&self) -> Auswertung<'_> {
        Auswertung::new(&self.muster, self.mess.clone())
    }

    /// Checks the seam list against the pattern (Seamly curves).
    pub fn nahtbericht(&self, nahtliste: &[Nahtdef]) -> Result<Nahtbericht, Fehler> {
        pruefe_naehte(&self.auswertung(), nahtliste, Kurvenquelle::Seamly)
            .map_err(|e| Fehler::Naehte(e.to_string()))
    }

    /// The whole chain: evaluate → check seams → mesh.
    pub fn netz(
        &self,
        nahtliste: &[Nahtdef],
        p: &klon3d_mesh::Parameter,
    ) -> Result<Musternetz, Fehler> {
        let ev = self.auswertung();
        let bericht = pruefe_naehte(&ev, nahtliste, Kurvenquelle::Seamly)
            .map_err(|e| Fehler::Naehte(e.to_string()))?;
        klon3d_mesh::vernetze(&ev, Kurvenquelle::Seamly, &bericht.naehte, p)
            .map_err(|e| Fehler::Netz(e.to_string()))
    }

    /// The pieces as the DXF export writes them: outline in DXF coordinates (mm, y upwards),
    /// notch marks, the whole arrangement moved to the origin. The single place where
    /// `klon3d dxf` and the application window prepare the file, so both write the same bytes.
    ///
    /// `waehlen` picks the pieces (`--teile`); it runs before the arrangement is moved, so the
    /// result is the same as selecting the pieces first.
    pub fn dxf_teile(
        &self,
        waehlen: impl Fn(&klon3d_pattern::Piece) -> bool,
    ) -> Result<Vec<Dxfteil>, Fehler> {
        let f = self
            .muster
            .unit_to_mm()
            .map_err(|e| Fehler::Muster(format!("{}: {e}", self.pfad.display())))?;
        let ev = self.auswertung();
        let index = ev.modeling_index();

        let mut out = Vec::new();
        for t in self.muster.pieces() {
            let name = t.name.clone().unwrap_or_default();
            if !waehlen(t) {
                continue;
            }
            // As in Seamly's piece mode (and in its DXF): construction plus the piece's offset.
            // DXF counts y upwards, the .sm2d downwards.
            let mx = t
                .mx
                .as_deref()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
            let my = t
                .my
                .as_deref()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
            let nach_dxf = |p: klon3d_pattern::Pt| [(p.x + mx) * f, -(p.y + my) * f];

            let (knoten, offene) = ev.stueck_knoten(t);
            let kante = dxf::bereinige(ev.umriss_aus_knoten(&knoten).into_iter().map(nach_dxf));
            // The cutting line: the seam line displaced by the allowance of its nodes, which is
            // what Seamly writes on its own second layer. A piece without allowance keeps one
            // line; its contour would only be a copy of the seam line.
            let kontur = ev
                .teilrand(t, Kurvenquelle::Seamly)
                .ok()
                .filter(|r| {
                    r.punktknoten
                        .iter()
                        .any(|k| k.zugabe_nach.unwrap_or(0.0) > 0.0)
                })
                .map(|r| {
                    // `punkte()` repeats both ends of every segment; the ring must not.
                    let mut pts: Vec<klon3d_pattern::Pt> = Vec::new();
                    for p in Saumkontur::aus_teilrand(&r, Ecken::Eigen).punkte() {
                        if pts.last().map_or(true, |q| q.dist(*p) > 1e-9) {
                            pts.push(*p);
                        }
                    }
                    dxf::bereinige(pts.into_iter().map(nach_dxf))
                })
                .filter(|k| k.len() >= 3);
            if kante.len() < 3 {
                out.push(Dxfteil {
                    teil: dxf::Teil {
                        name,
                        kante,
                        kontur: None,
                        kerben: Vec::new(),
                    },
                    uebergangen: true,
                    offene,
                    daneben: Vec::new(),
                });
                continue;
            }

            // Notch marks: point nodes with notch="true". They are corners of the outline and
            // so lie on the edge.
            let mut kerben = Vec::new();
            let mut daneben = Vec::new();
            for k in t.nodes.iter().flat_map(|n| &n.items) {
                let ja = |w: &Option<String>| matches!(w.as_deref(), Some("true") | Some("1"));
                if !ja(&k.notch) || ja(&k.exclude) || k.kind != "NodePoint" {
                    continue;
                }
                let p = match index.get(&k.id_object) {
                    Some((true, ziel)) => ev.punkt(ziel).map_err(|e| e.to_string()),
                    _ => Err("ohne Modellierziel".into()),
                };
                match p.map(nach_dxf) {
                    Ok(p) => match dxf::kerbe(&kante, p, 0.01) {
                        Some(l) => kerben.push(l),
                        None => daneben.push(format!(
                            "Knoten {} liegt nicht auf der Kante",
                            k.id_object
                        )),
                    },
                    Err(e) => daneben.push(format!("Knoten {}: {e}", k.id_object)),
                }
            }
            out.push(Dxfteil {
                teil: dxf::Teil { name, kante, kontur, kerben },
                uebergangen: false,
                offene,
                daneben,
            });
        }

        // The whole arrangement to the origin.
        let alle: Vec<dxf::P> = out
            .iter()
            .filter(|t| !t.uebergangen)
            .flat_map(|t| {
                t.teil
                    .kante
                    .iter()
                    .copied()
                    .chain(t.teil.kontur.iter().flatten().copied())
            })
            .collect();
        let h = dxf::huelle(&alle);
        for t in out.iter_mut().filter(|t| !t.uebergangen) {
            for p in t
                .teil
                .kante
                .iter_mut()
                .chain(t.teil.kontur.iter_mut().flatten())
                .chain(t.teil.kerben.iter_mut().flatten())
            {
                *p = [p[0] - h[0], p[1] - h[1]];
            }
        }
        Ok(out)
    }
}

/// One piece as the DXF export prepared it, with the notes the command line prints.
#[derive(Debug, Clone)]
pub struct Dxfteil {
    pub teil: dxf::Teil,
    /// Left out: fewer than three points in the outline.
    pub uebergangen: bool,
    /// Nodes whose target did not resolve.
    pub offene: Vec<String>,
    /// Notch marks that did not lie on the outline.
    pub daneben: Vec<String>,
}

impl Dxfteil {
    /// Warnings of this piece, as `klon3d dxf` counts them.
    pub fn warnungen(&self) -> usize {
        if self.uebergangen {
            1
        } else {
            self.daneben.len() + self.offene.len()
        }
    }
}

/// Reads a seam list (`.naehte`).
pub fn nahtliste(pfad: &Path) -> Result<Vec<Nahtdef>, Fehler> {
    std::fs::read_to_string(pfad)
        .map_err(|e| e.to_string())
        .and_then(|t| lies_nahtliste(&t).map_err(|e| e.to_string()))
        .map_err(|e| Fehler::Datei(format!("{}: {e}", pfad.display())))
}

/// Reads a placement file (`.lage`).
pub fn lageliste(pfad: &Path) -> Result<Lageliste, Fehler> {
    std::fs::read_to_string(pfad)
        .map_err(|e| e.to_string())
        .and_then(|t| lies_lage(&t))
        .map_err(|e| Fehler::Datei(format!("{}: {e}", pfad.display())))
}

/// Where the body comes from.
#[derive(Debug, Clone)]
pub enum Koerperquelle<'a> {
    /// Shaped from a measurement file with the resizing folder of
    /// [`klon3d_koerper::koerper::ordner`] (the default path).
    Massdatei(&'a Path),
    /// As [`Koerperquelle::Massdatei`], with an explicit resizing folder.
    MassdateiMitOrdner(&'a Path, PathBuf),
    /// The `.avt` body of a `.zprj` project plus the resizing folder that
    /// names its segments and landmarks. This body is already in simulation
    /// pose.
    CloProjekt(&'a Path, PathBuf),
}

/// A body ready for placement: collision mesh in scene coordinates (m), its
/// arrangement (trunk, arms, landmarks in mm) and a line saying where it came
/// from.
#[derive(Clone)]
pub struct GeladenerKoerper {
    pub netz: Koerpernetz,
    pub anordnung: Anordnung,
    pub herkunft: String,
}

/// Loads or shapes the body. Both sources give points and triangles in mm
/// in the avatar frame plus their arrangement.
pub fn koerper(q: Koerperquelle<'_>) -> Result<GeladenerKoerper, Fehler> {
    let k = |m: String| Fehler::Koerper(m);
    let (punkte, dreiecke, anordnung, herkunft) = match q {
        Koerperquelle::CloProjekt(pfad, resizing) => {
            let projekt = klon3d_pattern::zprj::Projekt::lies(pfad)
                .map_err(|e| k(format!("{}: {e}", pfad.display())))?;
            let Some(kp) = projekt.koerper else {
                return Err(k(format!(
                    "{}: keine .avt mit einem Körper",
                    pfad.display()
                )));
            };
            let punkte: Vec<[f64; 3]> = kp.punkte.iter().map(|q| [q.x, q.y, q.z]).collect();
            let dreiecke: Vec<[usize; 3]> = kp
                .dreiecke
                .iter()
                .map(|d| [d[0] as usize, d[1] as usize, d[2] as usize])
                .collect();
            let nummern = CloNummern::aus_ordner(&resizing)
                .map_err(|e| k(format!("{}: {e}", resizing.display())))?;
            let netz = klon3d_koerper::Netz {
                punkte: punkte.clone(),
                dreiecke: dreiecke.clone(),
            };
            let an = aus_clo_koerper(&netz, &nummern)
                .map_err(|e| k(format!("{}: {e}", pfad.display())))?;
            let herkunft = format!(
                "{} (CLO .avt), {} Punkte, {} Rumpf, {} + {} Arm",
                pfad.display(),
                punkte.len(),
                an.rumpf.len(),
                an.arme[0].len(),
                an.arme[1].len()
            );
            (punkte, dreiecke, an, herkunft)
        }
        Koerperquelle::Massdatei(masse) => {
            aus_massdatei(masse, &klon3d_koerper::koerper::ordner())?
        }
        Koerperquelle::MassdateiMitOrdner(masse, resizing) => aus_massdatei(masse, &resizing)?,
    };
    let netz = Koerpernetz::aus_clo_mm(&punkte, &dreiecke, SUCHTIEFE)
        .map_err(|e| k(format!("Körpernetz: {e}")))?;
    Ok(GeladenerKoerper {
        netz,
        anordnung,
        herkunft,
    })
}

type Rohkoerper = (Vec<[f64; 3]>, Vec<[usize; 3]>, Anordnung, String);

fn aus_massdatei(masse: &Path, resizing: &Path) -> Result<Rohkoerper, Fehler> {
    let k = |m: String| Fehler::Koerper(m);
    let smis =
        klon3d_pattern::lies_smis(masse).map_err(|e| k(format!("{}: {e}", masse.display())))?;
    let kp = klon3d_koerper::koerper::aus_smis(resizing, &|n| smis.wert(n))
        .map_err(|e| k(format!("Körper nicht formbar: {e}")))?;
    let an = klon3d_koerper::koerper::anordnung(resizing, &kp.netz).map_err(k)?;
    let herkunft = format!("{} ({})", masse.display(), kp.weg.name());
    Ok((kp.netz.punkte, kp.netz.dreiecke, an, herkunft))
}

/// The hull bodies the placement lays the pieces around.
pub fn huellen(k: &GeladenerKoerper) -> Result<Huellen, Fehler> {
    let (c, an) = (clo_nach_szene, &k.anordnung);
    Huellen::neu(&Koerperteile {
        punkte: &k.netz.punkte,
        dreiecke: &k.netz.dreiecke,
        rumpf: &an.rumpf,
        arme: [&an.arme[0], &an.arme[1]],
        hals_vorn: c(an.hals_vorn),
        hals_hinten: c(an.hals_hinten),
        hals_seitlich: an.hals_seitlich.map(c),
        schulter: an.schulter.map(c),
        ellbogen: an.ellbogen.map(c),
    })
    .map_err(|e| Fehler::Koerper(format!("Hüllkörper: {e}")))
}

/// Places the pieces of `netz` around the hulls; `weiten` fits each piece's
/// circumference to the body (the default).
pub fn platzieren(
    netz: &Musternetz,
    lage: &Lageliste,
    huellen: &Huellen,
    weiten: bool,
) -> Result<Platzierung, Fehler> {
    platziere_mit(netz, lage, huellen, weiten).map_err(Fehler::Lage)
}

/// Gravity and damping of a dressing run.
pub fn standard_simparameter() -> Simparameter {
    Simparameter {
        schwerkraft: [0.0, 0.0, -9.81],
        daempfung: 1.0,
        ..Simparameter::default()
    }
}

/// The scene of a placement, without bonding zones: every piece in
/// `material`, at the thickness its placement line gives.
pub fn szene_aus(
    p: &Platzierung,
    k: &GeladenerKoerper,
    material: Material,
    reduktion: f64,
    sim: Simparameter,
) -> Szene {
    let vg = |name: &str| {
        let dicke = p
            .netz
            .teile
            .iter()
            .position(|t| t.teil == name)
            .and_then(|ti| p.dicke(ti));
        Teilvorgabe {
            material: dicke.map_or(material, |d| material.mit_dicke(d)),
            reduktion,
        }
    };
    Szene::aus_netz(
        &p.netz,
        &vg,
        &|ti, q| p.lage(ti, q),
        vec![Koerper::netz(k.netz.clone())],
        sim,
    )
}

/// Placement + scene, without stepping: hulls, placement with fitted
/// circumference, scene, bonding zones where the placement file asks for them.
pub fn szene(
    netz: &Musternetz,
    lage: &Lageliste,
    koerper: &GeladenerKoerper,
    material: Material,
    reduktion: f64,
    sim: Simparameter,
) -> Result<Szene, Fehler> {
    let h = huellen(koerper)?;
    let p = platzieren(netz, lage, &h, true)?;
    let mut s = szene_aus(&p, koerper, material, reduktion, sim);
    let zonen = p.klebungen(&s).into_iter().map(|(z, _)| z).collect();
    s.klebungen_setzen(zonen);
    Ok(s)
}

/// One frame; the caller keeps the solver and the scene, so pausing works.
pub fn bild(s: &mut Szene, rw: &mut dyn Rechenweg) {
    rw.bild(s);
}
