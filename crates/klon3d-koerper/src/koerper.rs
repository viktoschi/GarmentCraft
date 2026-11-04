//! The user's body from her measurement file: entry point of `klon3d.exe` and
//! later of the simulation.
//!
//! Four paths, tried in this order ([`Weg`], `koerper_weg.rs`):
//!
//! 1. **Reference shaping chain** ([`crate::kette_referenz`], findings/64) — the
//!    basis: BASE + DETAIL Human over the `.bd` fields, calibration, symmetry,
//!    centring, `.avs`, hand and head post-processing. It also provides the 67
//!    measurements ([`crate::messung_referenz`]); on the `.smis` path HighHip is
//!    derived from the chain's own BASE mesh because the fields do not carry it.
//! 2. **Older shaping chain** ([`crate::kette`], findings/31) — only for old
//!    resizing folders when the reference data is missing.
//! 3. **Newton fit** ([`crate::person`] + [`crate::anpassen`], findings/23, 24)
//!    — needs only `BaseMesh.obj` and the `DETAIL_Advanced` fields.
//! 4. **MakeHuman body** ([`crate::koerper_quelle`] + `anpassen`, issue #65) —
//!    the shipped CC0 base mesh with MakeHuman's own female and breast targets,
//!    pulled onto the shape targets of the `.smis` file. Needs **no CLO file at
//!    all**; [`makehuman_weg`] is public so it can be forced (`--makehuman`).
//!
//! Paths 1–3 give the `body` group (points 0 … 31 421) in mm with its triangles,
//! path 4 MakeHuman's `body` group (13 380 points, plus the rings and corners
//! [`crate::anpassen::formen`] inserts).
//!
//! On the reference path follows klon3d's own **post-shaping between waist and
//! hip** ([`crate::nachformung`], findings/124), which goes beyond the reference
//! program: the chain's statistical shape cannot follow a steep rise right
//! below the waist. [`aus_smis_mit`] with `nachformen = false` (command line:
//! `klon3d koerper --wie-clo`) leaves the body exactly as the chain shapes it.

use std::path::{Path, PathBuf};

use crate::anpassen::{anpassen, formlinien_aus_smis, formziel_ebenen, formziele_aus_smis, formen};
use crate::formung::{namen_vektor, referenz_vektor, Formung};
use crate::kette::KOERPER;
use crate::kette_referenz::{Ende, Ganze, KetteReferenz, Konfiguration};
use crate::messung_referenz::{platz, ANZAHL};
use crate::person::zielperson;
use crate::{lies_obj, Netz, Punkt3};

/// CLO's resizing folder of the default installation.
pub const STANDARD_ORDNER: &str =
    r"C:\Program Files\CLO Standalone OnlineAuth\Preset\Avatar\Resizing\Female";

/// Resizing-Ordner: `CLO_FEMALE`, sonst [`STANDARD_ORDNER`].
pub fn ordner() -> PathBuf {
    std::env::var_os("CLO_FEMALE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(STANDARD_ORDNER))
}

/// Which path shaped the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weg {
    /// Reference shaping chain (`kette_referenz.rs`).
    Kette,
    /// Older shaping chain (`kette.rs`), only for old folders.
    KetteAlt,
    /// Old path, Newton fit onto the outline (`anpassen.rs`).
    Anpassen,
    /// MakeHuman base mesh pulled onto the shape targets (`koerper_quelle.rs`
    /// + `anpassen.rs`). Needs no CLO file.
    Makehuman,
}

impl Weg {
    pub fn name(self) -> &'static str {
        match self {
            Weg::Kette => "Formungskette (Referenz)",
            Weg::KetteAlt => "Formungskette (alt)",
            Weg::Anpassen => "anpassen (Fallback)",
            Weg::Makehuman => "MakeHuman + Formziele",
        }
    }
}

/// What the chain knows about the mesh: final values and target values, in cm.
#[derive(Clone, Debug)]
pub struct Kettenwerte {
    /// The 60 measured final values, 67 slots (`messung_referenz::NAMEN`).
    pub werte: [f64; ANZAHL],
    /// The input of the DETAIL stage, 67 slots (slot 7: derived HighHip).
    pub eingabe: [f64; ANZAHL],
    /// Form steps BASE / DETAIL.
    pub formungen: (usize, usize),
    /// End reason BASE / DETAIL.
    pub grund: (Ende, Ende),
}

impl Kettenwerte {
    fn aus_referenz(g: &Ganze) -> Kettenwerte {
        Kettenwerte {
            werte: g.detail.werte,
            eingabe: g.eingabe,
            formungen: (g.base.e_folge.len(), g.detail.e_folge.len()),
            grund: (g.base.grund, g.detail.grund),
        }
    }
}

/// The end reason as text.
fn text(e: Ende) -> &'static str {
    match e {
        Ende::Konvergenz => "E < K·0,01",
        Ende::Rueckfall => "Rückfall",
        Ende::ZuViele => "zu viele Formungen",
        Ende::Negativ => "X < 0",
        Ende::Fehler => "Fehler",
    }
}

/// Der geformte Körper.
pub struct Koerper {
    /// Gruppe `body` in mm; alle Dreiecksindizes < [`KOERPER`].
    pub netz: Netz,
    pub weg: Weg,
    /// Was beim Formen auffiel, Zeile für Zeile (Grund des Fallbacks,
    /// Formungen, Ende-Grund).
    pub meldungen: Vec<String>,
    /// The chain's values, when it shaped the body. They describe the
    /// chain's body before [`Self::nachformung`].
    pub kette: Option<Kettenwerte>,
    /// klon3d's own post-shaping between waist and hip, beyond the
    /// reference program ([`crate::nachformung`]).
    pub nachformung: Option<crate::nachformung::Bericht>,
}

/// Shapes the body from SeamlyMe measurements; `wert` asks for a measurement by
/// its name, in cm.
///
/// The first path is the reference chain ([`KetteReferenz`]); without its files
/// the older chain, and [`anpassen`] last. If the reference chain fails on the
/// measurements, that is an error.
pub fn aus_smis(ordner: &Path, wert: &dyn Fn(&str) -> Option<f64>) -> Result<Koerper, String> {
    aus_smis_mit(ordner, wert, true)
}

/// Like [`aus_smis`]; `nachformen` = false leaves the reference chain's body
/// as it is (no [`crate::nachformung`]), exactly as the reference program
/// shapes it.
pub fn aus_smis_mit(
    ordner: &Path,
    wert: &dyn Fn(&str) -> Option<f64>,
    nachformen: bool,
) -> Result<Koerper, String> {
    let konf = Konfiguration::klon3d();
    match KetteReferenz::laden(ordner) {
        Ok(k) => {
            let e = k.aus_smis(&konf, wert)?;
            let mut meldungen = vec![
                format!(
                    "BASE: {} Formungen, {}",
                    e.base.e_folge.len(),
                    text(e.base.grund)
                ),
                format!(
                    "DETAIL: {} Formungen, {}",
                    e.detail.e_folge.len(),
                    text(e.detail.grund)
                ),
            ];
            let mut netz = Netz {
                punkte: crate::kette_referenz::koerper_mm(&e.detail.punkte),
                dreiecke: k.basis.dreiecke.clone(),
            };
            let kette = Some(Kettenwerte::aus_referenz(&e));
            let mut nachformung = None;
            let ziele = nachformen
                .then(|| crate::nachformung::Ziele::aus_smis(wert))
                .flatten();
            if let Some(z) = ziele {
                let w = &e.detail.werte;
                let hoehe = |n: &str| w[platz(n).expect("name of the measurement set")];
                let hoehen = (
                    hoehe("HEIGHT_Waist"),
                    hoehe("HEIGHT_LowHip"),
                    hoehe("HEIGHT_UnderBust"),
                );
                match crate::nachformung::nachformen(&mut netz.punkte, &k.messdaten, hoehen, &z) {
                    Ok(b) => {
                        meldungen.extend(b.zeilen());
                        nachformung = Some(b);
                    }
                    Err(g) => meldungen.push(format!("Nachformung nicht möglich: {g}")),
                }
            }
            Ok(Koerper {
                netz,
                weg: Weg::Kette,
                meldungen,
                kette,
                nachformung,
            })
        }
        Err(grund) => {
            // No reference data. The older chain when its folder is there,
            // `anpassen` when CLO's older `Advanced` files are (that is the
            // case `fallback_ohne_kettendaten` builds), and MakeHuman last -
            // the only path that needs no CLO file at all (issue #65).
            let mut k = match weg_alt(ordner, wert) {
                Ok(k) => k,
                Err(_) => match alter_weg(ordner, wert) {
                    Ok(k) => k,
                    Err(alt) => match crate::koerper_quelle::ordner() {
                        Some(o) => makehuman_weg(&o, wert).map_err(|mh| {
                            format!("CLO-Pfad: {alt}; MakeHuman: {mh}")
                        })?,
                        None => {
                            return Err(format!(
                                "CLO-Pfad: {alt}; {} fehlt",
                                crate::koerper_quelle::ORDNER
                            ))
                        }
                    },
                },
            };
            k.meldungen
                .insert(0, format!("Referenzkette nicht ladbar aus {}: {grund}", ordner.display()));
            Ok(k)
        }
    }
}

/// The older chain, for folders without reference data (backup).
fn weg_alt(ordner: &Path, wert: &dyn Fn(&str) -> Option<f64>) -> Result<Koerper, String> {
    let k = crate::kette::Kette::laden(ordner).map_err(|e| e.to_string())?;
    let e = k.aus_smis(wert)?;
    let mut werte = [0.0; ANZAHL];
    werte[..crate::messung::ANZAHL].copy_from_slice(&e.detail.messung.werte);
    let mut eingabe = [0.0; ANZAHL];
    eingabe[..crate::messung::ANZAHL].copy_from_slice(&e.eingabe);
    let kette = Kettenwerte {
        werte,
        eingabe,
        formungen: (e.base.formungen, e.detail.formungen),
        grund: (grund_alt(e.base.ende), grund_alt(e.detail.ende)),
    };
    let meldungen = vec![
        format!(
            "BASE (alt): {} Formungen, {}",
            e.base.formungen,
            e.base.ende.als_text()
        ),
        format!(
            "DETAIL (alt): {} Formungen, {}",
            e.detail.formungen,
            e.detail.ende.als_text()
        ),
        format!("Handanteil t = {:.4}", e.hand_t),
    ];
    Ok(Koerper {
        netz: Netz {
            punkte: crate::kette::Kette::koerper_mm(&e.detail.punkte),
            dreiecke: k.dreiecke.clone(),
        },
        weg: Weg::KetteAlt,
        meldungen,
        kette: Some(kette),
        nachformung: None,
    })
}

/// Maps the older chain's end reason onto the shared one.
fn grund_alt(e: crate::kette::Ende) -> Ende {
    use crate::kette::Ende as EAlt;
    match e {
        EAlt::Getroffen => Ende::Konvergenz,
        EAlt::Rueckfall => Ende::Rueckfall,
        EAlt::Hoechstzahl => Ende::ZuViele,
        EAlt::Negativ => Ende::Negativ,
    }
}

/// Was die Platzierung (Stufe 4c) über den geformten Körper wissen muss: die
/// Punkte von Rumpf und Armen nach CLOs Segmenten (`range_list.rlist`) und
/// die Landmarken aus `IndexSet.txt` (mm, Rahmen von CLO). Die Zeilen sind
/// die der älteren Fassung (`spez/messung.md`), passend zur
/// Formungskette.
#[derive(Clone, Debug)]
pub struct Anordnung {
    pub rumpf: Vec<usize>,
    /// links, rechts
    pub arme: [Vec<usize>; 2],
    /// L4 vordere Halsmitte (Drosselgrube), L5 hintere Halsmitte (7. Halswirbel)
    pub hals_vorn: Punkt3,
    pub hals_hinten: Punkt3,
    /// L6, L7 seitlicher Halspunkt links, rechts
    pub hals_seitlich: [Punkt3; 2],
    /// L8, L9 Schulterpunkt links, rechts
    pub schulter: [Punkt3; 2],
    /// L16, L17 Achsel (tiefster Punkt des Armlochs) links, rechts
    pub achsel: [Punkt3; 2],
    /// L18, L19 Brustpunkt links, rechts
    pub brustpunkt: [Punkt3; 2],
    /// L49, L52 Ellbogen hinten links, rechts
    pub ellbogen: [Punkt3; 2],
}

/// Segmente und Landmarken zum geformten `netz` aus dem Resizing-Ordner.
///
/// CLOs Listen haben Vorrang: sie sind die Referenz, gegen die die Rechnung
/// geprüft ist. Fehlt der Ordner oder eine Liste (Resizing-Ordner eines fremden
/// Rechners), kommen die Landmarken aus dem Netz selbst — siehe
/// [`crate::anordnung_netz::aus_netz`].
pub fn anordnung(ordner: &Path, netz: &Netz) -> Result<Anordnung, String> {
    match anordnung_aus_clo(ordner, netz) {
        Ok(an) => Ok(an),
        Err(grund) => crate::anordnung_netz::aus_netz(netz, 10.0)
            .map_err(|e| format!("CLO-Listen: {grund}; aus dem Netz: {e}")),
    }
}

fn anordnung_aus_clo(ordner: &Path, netz: &Netz) -> Result<Anordnung, String> {
    use crate::massband::{segment, Messdaten};
    let d = Messdaten::laden(ordner).map_err(|e| format!("Segmente und Landmarken: {e}"))?;
    let n = netz.punkte.len();
    if d.segment.len() < n {
        return Err(format!("{} Segmente für {n} Punkte", d.segment.len()));
    }
    let teil = |s: i32| (0..n).filter(|&i| d.segment[i] == s).collect::<Vec<_>>();
    let lm = |zeile: usize| {
        d.landmarke(zeile)
            .filter(|&i| i < n)
            .map(|i| netz.punkte[i])
            .ok_or_else(|| format!("Landmarke L{zeile} fehlt"))
    };
    Ok(Anordnung {
        rumpf: teil(segment::RUMPF),
        arme: [teil(segment::ARM_LINKS), teil(segment::ARM_RECHTS)],
        hals_vorn: lm(4)?,
        hals_hinten: lm(5)?,
        hals_seitlich: [lm(6)?, lm(7)?],
        schulter: [lm(8)?, lm(9)?],
        achsel: [lm(16)?, lm(17)?],
        brustpunkt: [lm(18)?, lm(19)?],
        ellbogen: [lm(49)?, lm(52)?],
    })
}

/// Der Körper aus MakeHuman, auf ihre Formziele gezogen — **ohne CLO-Daten**.
///
/// `ordner_makehuman` ist der Ordner mit `base.obj` und den Zielen
/// ([`crate::koerper_quelle::ORDNER`]); `wert` liefert die Maße in cm.
///
/// Der Ablauf ist der aus Issue #65: Grundkörper in ihrer Körpergröße
/// ([`crate::koerper_quelle::makehuman`]), dann die Formziele aus der Maßdatei
/// ([`formziele_aus_smis`]), deren Messhöhen ([`formlinien_aus_smis`]) als
/// Querschnitte ([`formziel_ebenen`]) und schliesslich [`formen`].
///
/// Die Meldungen sagen, was am Ziel nicht messbar oder nicht konstruierbar war
/// und wie weit die Linien danach noch daneben liegen; still wird nichts
/// übersprungen.
pub fn makehuman_weg(
    ordner_makehuman: &Path,
    wert: &dyn Fn(&str) -> Option<f64>,
) -> Result<Koerper, String> {
    let hoehe_cm = wert("height").filter(|h| *h > 0.0).unwrap_or(160.0);
    let mut netz = crate::koerper_quelle::makehuman(ordner_makehuman, hoehe_cm * 10.0)?;
    let punkte_vorher = netz.punkte.len();

    let formziele = formziele_aus_smis(wert);
    let hoehen = formlinien_aus_smis(wert);
    let (ebenen, mut meldungen) = formziel_ebenen(&formziele, &hoehen, wert);
    meldungen.extend(formziele.meldungen.iter().cloned());
    meldungen.insert(
        0,
        format!(
            "MakeHuman: {punkte_vorher} Punkte, Zielgrösse {hoehe_cm:.1} cm, {} Formlinien",
            ebenen.len()
        ),
    );

    let (linien, form_meldungen) = formen(&mut netz, &ebenen);
    meldungen.extend(form_meldungen);
    for l in &linien {
        let zeile = l
            .fehler()
            .iter()
            .map(|(art, f)| match f {
                Some(f) => format!("{} {f:+.2}", art.name()),
                None => format!("{} —", art.name()),
            })
            .collect::<Vec<_>>()
            .join(", ");
        meldungen.push(format!("{} bei {:.1} cm: {zeile}", l.ort.name(), l.hoehe));
    }
    if linien.len() < ebenen.len() {
        meldungen.push(format!(
            "{} von {} Formlinien geformt",
            linien.len(),
            ebenen.len()
        ));
    }

    Ok(Koerper {
        netz,
        weg: Weg::Makehuman,
        meldungen,
        kette: None,
        nachformung: None,
    })
}

/// Der alte Weg wie `examples/forme.rs`, ohne Maßband (das braucht die
/// Messdaten, die der Kette dann meist auch fehlen).
fn alter_weg(ordner: &Path, wert: &dyn Fn(&str) -> Option<f64>) -> Result<Koerper, String> {
    let txt = ordner.join("_DetailFeature_Advanced_Bust_TotalHeight.txt");
    let bd = ordner.join("ConvertingMat_DETAIL_Advanced_TotalHeight.bd");
    let grund = lies_obj(&ordner.join("BaseMesh.obj"), 10.0)
        .map_err(|e| format!("BaseMesh.obj: {e}"))?;
    let formung =
        Formung::laden(&bd, grund.punkte.len()).map_err(|e| format!("Formungsdatei: {e}"))?;
    let c_ref = referenz_vektor(&txt).map_err(|e| format!("Referenzvektor: {e}"))?;
    let namen = namen_vektor(&txt).map_err(|e| format!("Maßnamen: {e}"))?;
    let p = zielperson(&grund, &namen, &c_ref, wert)?;
    let e = anpassen(&grund, &formung, &p.c_start, &p.linien, &p.ziel, &p.aktiv)?;
    let mut geformt = formung.anwenden(&grund, &e.c);
    geformt.punkte.truncate(KOERPER.min(geformt.punkte.len()));
    let n = geformt.punkte.len();
    geformt.dreiecke.retain(|d| d.iter().all(|&i| i < n));
    let mut meldungen = vec![format!(
        "{} Runden{}",
        e.runden,
        if e.divergiert { ", divergiert" } else { "" }
    )];
    for (name, f) in p.linien.namen().iter().zip(&e.fehler) {
        meldungen.push(format!("{name}: Fehler {f:+.3} cm"));
    }
    Ok(Koerper {
        netz: geformt,
        weg: Weg::Anpassen,
        meldungen,
        kette: None,
        nachformung: None,
    })
}
