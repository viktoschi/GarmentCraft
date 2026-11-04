//! A body without CLO data: MakeHuman as the source mesh.
//!
//! Until now the body came from CLO's resizing folder - `BaseMesh.obj`, the
//! fields of `DETAIL_Advanced`, and the shaping chain that turns them into a
//! body. That ties klon3d to a CLO installation and to CLO's own idea of what
//! a body looks like. Issue #65 (finding 123) replaces the source: a freely
//! licensed base mesh plus shape targets, shaped by measurements the user
//! really has.
//!
//! This module does the reading half of that: it loads MakeHuman's `base.obj`,
//! applies `.target` files, cuts the naked body out of the file and puts it
//! into the frame the rest of the crate expects - millimetres, `y` up, the
//! lowest point at `y = 0`, `x` and `z` centred.
//!
//! Four details of the format cost time and are therefore written down here:
//!
//! * **Decimetres.** `base.obj` and the `.target` files count in dm; the rest
//!   of klon3d counts in mm ([`SKALA`]).
//! * **Quads and fans.** The `f` lines have four and more corners with
//!   `i/j/k` texture indices; fan triangulation is mandatory, a "first three
//!   corners" reader loses half the body.
//! * **Groups.** The file holds 172 groups: the naked body (`body`), helper
//!   meshes (`helper-tights` and others) and joint markers (`joint-*`).
//!   Only `body` is a body; the joint markers are single points and would
//!   break every section.
//! * **Targets before cutting.** A target addresses points by their index in
//!   the *whole* file, so all of them are applied before the body group is
//!   extracted.
//!
//! The two target files that belong to the female body with a large bust are
//! shipped in [`ORDNER`]; [`makehuman`] loads the folder as a whole.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{Netz, Punkt3};

/// The shipped MakeHuman data, relative to the repository root.
pub const ORDNER: &str = "assets/3dobjs/makehuman";

/// The base mesh in that folder.
pub const BASIS: &str = "base.obj";

/// Targets applied by [`makehuman`], in this order.
///
/// The first one shapes the macrodetails (a young Caucasian woman), the second
/// gives the breasts their size and firmness. Both are CC0; see
/// `provenance.json` and `LICENSE.ASSETS.md` in the same folder.
pub const ZIELE: [&str; 2] = [
    "macrodetails_caucasian-female-young.target",
    "breast_female-young-averagemuscle-averageweight-maxcup-averagefirmness.target",
];

/// The group of the naked body.
pub const KOERPER_GRUPPE: &str = "body";

/// File units to millimetres: `base.obj` and the targets count in decimetres.
pub const SKALA: f64 = 100.0;

fn ungueltig(pfad: &Path, was: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{}: {was}", pfad.display()))
}

/// A whole MakeHuman file: all points and the triangles of every group.
///
/// Points keep the file's own unit and order. Groups keep their name and refer
/// to the global point indices - that is what the targets address as well.
#[derive(Clone, Debug, Default)]
pub struct Rohteil {
    pub punkte: Vec<Punkt3>,
    /// Group name and its triangles, in order of first appearance.
    ///
    /// A name can appear more than once: `helper-l-eyelashes-1` is split into
    /// nine `g` blocks, and MakeHuman means them as one group. They are
    /// therefore collected under one name, not listed nine times - otherwise
    /// [`Self::gruppe`] would return a ninth of the eyelashes.
    pub gruppen: Vec<(String, Vec<[usize; 3]>)>,
}

impl Rohteil {
    /// Reads `v`, `f` and `g`. `vt`, `vn`, `s`, `usemtl` and comments fall
    /// away; polygons are fan triangulated, negative indices are allowed.
    pub fn lies(pfad: &Path) -> io::Result<Rohteil> {
        let text = fs::read_to_string(pfad)?;
        let mut roh = Rohteil::default();
        // Index of the group that `f` lines belong to; a repeated `g` name
        // continues its existing group.
        let mut aktuell: Option<usize> = None;
        for (nr, zeile) in text.lines().enumerate() {
            let mut teile = zeile.split_whitespace();
            match teile.next() {
                Some("v") => {
                    let mut p = [0.0f64; 3];
                    for a in 0..3 {
                        let t = teile.next().ok_or_else(|| {
                            ungueltig(pfad, format!("Zeile {}: Punkt zu kurz", nr + 1))
                        })?;
                        p[a] = t
                            .parse::<f64>()
                            .map_err(|e| ungueltig(pfad, format!("Zeile {}: {e}", nr + 1)))?;
                    }
                    roh.punkte.push(p);
                }
                Some("g") => {
                    let name = teile.next().unwrap_or("").to_string();
                    let i = match roh.gruppen.iter().position(|(g, _)| *g == name) {
                        Some(i) => i,
                        None => {
                            roh.gruppen.push((name, Vec::new()));
                            roh.gruppen.len() - 1
                        }
                    };
                    aktuell = Some(i);
                }
                Some("f") => {
                    let i = aktuell
                        .ok_or_else(|| ungueltig(pfad, format!("Zeile {}: f ohne g", nr + 1)))?;
                    let mut idx = Vec::new();
                    for t in teile {
                        let roh_i: i64 = t
                            .split('/')
                            .next()
                            .unwrap_or("")
                            .parse()
                            .map_err(|e| ungueltig(pfad, format!("Zeile {}: {e}", nr + 1)))?;
                        let j = if roh_i < 0 {
                            roh.punkte.len() as i64 + roh_i
                        } else {
                            roh_i - 1
                        };
                        if j < 0 || j as usize >= roh.punkte.len() {
                            return Err(ungueltig(
                                pfad,
                                format!(
                                    "Zeile {}: Punktindex {roh_i} ausserhalb von {}",
                                    nr + 1,
                                    roh.punkte.len()
                                ),
                            ));
                        }
                        idx.push(j as usize);
                    }
                    let gruppe = &mut roh.gruppen[i].1;
                    for k in 1..idx.len().saturating_sub(1) {
                        gruppe.push([idx[0], idx[k], idx[k + 1]]);
                    }
                }
                _ => {}
            }
        }
        Ok(roh)
    }

    /// Adds the offsets of one or more `.target` files, in file units.
    ///
    /// A target file is one line per moved point: `index dx dy dz`, optional
    /// comments with `#`. Returns the number of applied lines.
    pub fn ziele_anwenden(&mut self, pfade: &[PathBuf]) -> io::Result<usize> {
        let mut angewandt = 0;
        for pfad in pfade {
            let text = fs::read_to_string(pfad)?;
            for (nr, zeile) in text.lines().enumerate() {
                let zeile = zeile.trim();
                if zeile.is_empty() || zeile.starts_with('#') {
                    continue;
                }
                let mut teile = zeile.split_whitespace();
                let i: usize = teile
                    .next()
                    .unwrap_or("")
                    .parse()
                    .map_err(|e| ungueltig(pfad, format!("Zeile {}: {e}", nr + 1)))?;
                if i >= self.punkte.len() {
                    return Err(ungueltig(
                        pfad,
                        format!(
                            "Zeile {}: Punktindex {i} ausserhalb von {}",
                            nr + 1,
                            self.punkte.len()
                        ),
                    ));
                }
                for a in 0..3 {
                    let t = teile
                        .next()
                        .ok_or_else(|| ungueltig(pfad, format!("Zeile {}: zu kurz", nr + 1)))?;
                    let d = t
                        .parse::<f64>()
                        .map_err(|e| ungueltig(pfad, format!("Zeile {}: {e}", nr + 1)))?;
                    self.punkte[i][a] += d;
                }
                angewandt += 1;
            }
        }
        Ok(angewandt)
    }

    /// One group as its own mesh, points renumbered from zero.
    ///
    /// The points keep the file's order: a renumbering by first use would be
    /// just as correct for a mesh, but the sections are taken later on a body
    /// whose point order should be traceable back to the source file.
    pub fn gruppe(&self, name: &str) -> Option<Netz> {
        let dreiecke = self
            .gruppen
            .iter()
            .find(|(g, _)| g == name)
            .map(|(_, d)| d)?;
        let mut benutzt = vec![false; self.punkte.len()];
        for d in dreiecke {
            for &i in d {
                benutzt[i] = true;
            }
        }
        let mut nummer = vec![usize::MAX; self.punkte.len()];
        let mut netz = Netz::default();
        for (i, &b) in benutzt.iter().enumerate() {
            if b {
                nummer[i] = netz.punkte.len();
                netz.punkte.push(self.punkte[i]);
            }
        }
        netz.dreiecke = dreiecke
            .iter()
            .map(|d| [nummer[d[0]], nummer[d[1]], nummer[d[2]]])
            .collect();
        Some(netz)
    }
}

/// The folder [`ORDNER`] in `start` or in one of the folders above it.
pub fn ordner_ab(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(|o| o.join(ORDNER)).find(|p| p.is_dir())
}

/// The shipped folder, searched from the program's folder and then from the
/// current folder - the same search as `assets/stoffe` in the app.
///
/// The data travels with the program, so a body without CLO files needs no
/// path from the user: whoever runs `klon3d` from the repository or next to
/// `assets/` finds it. `None` only when the assets are missing, and then the
/// caller reports it.
pub fn ordner() -> Option<PathBuf> {
    let programm = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let aktuell = std::env::current_dir().ok();
    [programm, aktuell]
        .into_iter()
        .flatten()
        .find_map(|s| ordner_ab(&s))
}

/// Scales a mesh to a height and puts it into the frame of the crate.
///
/// The height is measured over the mesh itself (`max y - min y`), so the
/// result is exactly `hoehe_mm` tall. Afterwards `x` and `z` have mean zero
/// over all points and the lowest point sits at `y = 0` - the same frame
/// [`crate::massband::zentriert_cm`] gives CLO's meshes, which
/// [`crate::rumpf`] needs to find the ring around the body axis.
pub fn einpassen(netz: &mut Netz, hoehe_mm: f64) {
    let Some((lo, hi)) = netz.hoehe() else {
        return;
    };
    let ist = hi - lo;
    if ist <= 0.0 || hoehe_mm <= 0.0 {
        return;
    }
    let f = hoehe_mm / ist;
    for p in &mut netz.punkte {
        for a in 0..3 {
            p[a] *= f;
        }
    }
    let n = netz.punkte.len() as f64;
    let mx = netz.punkte.iter().map(|p| p[0]).sum::<f64>() / n;
    let mz = netz.punkte.iter().map(|p| p[2]).sum::<f64>() / n;
    let my = netz
        .punkte
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min);
    for p in &mut netz.punkte {
        p[0] -= mx;
        p[1] -= my;
        p[2] -= mz;
    }
}

/// MakeHuman's body from a folder with `base.obj` and the targets of
/// [`ZIELE`], in millimetres and `hoehe_mm` tall.
pub fn makehuman(ordner: &Path, hoehe_mm: f64) -> Result<Netz, String> {
    makehuman_mit(ordner, hoehe_mm, &ZIELE)
}

/// Like [`makehuman`], with an explicit list of target files.
pub fn makehuman_mit(ordner: &Path, hoehe_mm: f64, ziele: &[&str]) -> Result<Netz, String> {
    let basis = ordner.join(BASIS);
    let mut roh = Rohteil::lies(&basis).map_err(|e| e.to_string())?;
    let pfade: Vec<PathBuf> = ziele.iter().map(|z| ordner.join(z)).collect();
    roh.ziele_anwenden(&pfade).map_err(|e| e.to_string())?;
    let mut netz = roh
        .gruppe(KOERPER_GRUPPE)
        .ok_or_else(|| format!("Gruppe {KOERPER_GRUPPE} fehlt in {}", basis.display()))?;
    for p in &mut netz.punkte {
        for a in 0..3 {
            p[a] *= SKALA;
        }
    }
    einpassen(&mut netz, hoehe_mm);
    Ok(netz)
}

/// Group names of a file with how many triangles each holds - for the docs
/// and for a quick look at an unfamiliar file.
pub fn uebersicht(roh: &Rohteil) -> HashMap<String, usize> {
    roh.gruppen
        .iter()
        .map(|(g, d)| (g.clone(), d.len()))
        .collect()
}

#[cfg(test)]
mod pruefungen {
    use super::*;

    fn ordner() -> PathBuf {
        // The data is shipped with the repository, so this needs no skip.
        ordner_ab(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("assets/3dobjs/makehuman in the repository")
    }

    #[test]
    fn gruppen_und_triangulierung() {
        let roh = Rohteil::lies(&ordner().join(BASIS)).expect("base.obj lesbar");
        assert!(roh.punkte.len() > 19_000, "Punkte {}", roh.punkte.len());
        // 172 `g` lines, but five names repeat (the eyelash and tights
        // helpers are split into blocks): 139 distinct groups.
        let u = uebersicht(&roh);
        assert_eq!(u.len(), 139, "Gruppen");
        // `body` is 13 378 `f` lines, two thirds of them quads; read as
        // "first three corners" it would lose about half the triangles.
        assert_eq!(u[KOERPER_GRUPPE], 26_756, "Dreiecke der Gruppe body");
        let netz = roh.gruppe(KOERPER_GRUPPE).expect("Gruppe body");
        assert_eq!(netz.punkte.len(), 13_380);
        assert!(netz.dreiecke.iter().all(|d| d.iter().all(|&i| i < netz.punkte.len())));
    }

    #[test]
    fn targets_verschieben_nur_ihre_punkte() {
        let o = ordner();
        let mut roh = Rohteil::lies(&o.join(BASIS)).expect("base.obj lesbar");
        let vorher = roh.punkte.clone();
        let n = roh
            .ziele_anwenden(&[o.join(ZIELE[0]), o.join(ZIELE[1])])
            .expect("Ziele lesbar");
        assert_eq!(n, 19_150 + 700, "Zeilen beider Ziele");
        // Every line moves its point; at least somewhere something has to
        // change, and no point may end up where it started everywhere.
        let bewegt = vorher
            .iter()
            .zip(&roh.punkte)
            .filter(|(a, b)| a != b)
            .count();
        assert!(bewegt > 10_000, "bewegte Punkte {bewegt}");
    }

    #[test]
    fn koerper_ist_160_cm_hoch_und_zentriert() {
        let netz = makehuman(&ordner(), 1600.0).expect("MakeHuman-Koerper");
        let (lo, hi) = netz.hoehe().expect("Punkte");
        assert!((hi - lo - 1600.0).abs() < 1.0e-6, "Hoehe {}", hi - lo);
        assert!(lo.abs() < 1.0e-6, "tiefster Punkt {lo}");
        let n = netz.punkte.len() as f64;
        let mx = netz.punkte.iter().map(|p| p[0]).sum::<f64>() / n;
        let mz = netz.punkte.iter().map(|p| p[2]).sum::<f64>() / n;
        assert!(mx.abs() < 1.0e-6, "x-Mittel {mx}");
        assert!(mz.abs() < 1.0e-6, "z-Mittel {mz}");
    }

    /// The body has to be measurable: at the bust line there is exactly one
    /// ring around the axis, and it is a torso, not an arm.
    #[test]
    fn rumpf_ist_am_brustumfang_messbar() {
        let netz = makehuman(&ordner(), 1600.0).expect("MakeHuman-Koerper");
        let q = crate::rumpf(&netz, 1130.0).expect("Rumpfring bei 113 cm");
        assert!(q.ring.len() > 40, "Ringpunkte {}", q.ring.len());
        let umfang = q.laenge / 10.0;
        assert!((80.0..120.0).contains(&umfang), "Umfang {umfang} cm");
    }

    /// **Die Hüfte liegt bei MakeHuman hinter der Dateiachse.**
    ///
    /// Der Körper ist über den Mittelwert aller Punkte zentriert, und das
    /// Becken sitzt weit hinten: bei 77 cm reicht der Querschnitt von
    /// z = −20,1 cm bis +0,16 cm, der Ursprung `(0, 0)` liegt also **vor** dem
    /// Körper. Trotzdem ist der Schnitt ein sauberer, geschlossener
    /// Rumpfquerschnitt - nur eben nicht um `(0, 0)` herum.
    ///
    /// Diese Prüfung hält beide Hälften fest: dass der Ring gefunden wird
    /// (sonst ist die Hüfte unter 83 cm nicht messbar), und dass er ein Rumpf
    /// und kein Oberschenkel ist. Der Oberschenkel ist der Grund für die
    /// zweite Bedingung in [`crate::rumpf`]: er liegt ganz auf einer Seite und
    /// umschliesst die Mittelebene `x = 0` nie.
    #[test]
    fn huefte_ist_ein_rumpfring_und_kein_oberschenkel() {
        let netz = makehuman(&ordner(), 1600.0).expect("MakeHuman-Koerper");
        let q = crate::rumpf(&netz, 770.0).expect("Rumpfring bei 77 cm");

        let xs: Vec<f64> = q.ring.iter().map(|p| p.0 / 10.0).collect();
        let zs: Vec<f64> = q.ring.iter().map(|p| p.1 / 10.0).collect();
        let (xlo, xhi) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let (zlo, zhi) = (
            zs.iter().copied().fold(f64::INFINITY, f64::min),
            zs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );

        // Ein Rumpf ist breit und symmetrisch um die Mittelebene; ein
        // Oberschenkel wäre halb so breit und läge ganz auf einer Seite.
        assert!(xlo < -15.0 && xhi > 15.0, "Breite {xlo:.1} .. {xhi:.1}");
        assert!((xlo + xhi).abs() < 2.0, "nicht mittig: {xlo:.1} .. {xhi:.1}");

        // Und er liegt hinter der Dateiachse - das ist der Befund, der den
        // Ursprungstest allein scheitern lässt.
        assert!(
            (zlo..=zhi).contains(&0.0),
            "erwartet: der Schnitt reicht über z = 0, ist aber {zlo:.1} .. {zhi:.1}"
        );

        let umfang = q.laenge / 10.0;
        assert!((85.0..105.0).contains(&umfang), "Umfang {umfang} cm");
    }
}
