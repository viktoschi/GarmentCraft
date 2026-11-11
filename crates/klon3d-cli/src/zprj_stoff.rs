//! Which fabric of a `.zprj` project a run uses.
//!
//! `anziehen --zprj` takes the **body** from a CLO project. Until #131 that was
//! all it took: the fabric still had to come from `--material`, so a project
//! that states its own stretch, shear and bending could not hand any of it to
//! the solver. This module is the missing link - it picks one fabric out of
//! `Projekt::stoffe` and maps it with
//! [`klon3d_pipeline::stoff::aus_zprj_stoff`].
//!
//! **One fabric, not one per piece.** A project *does* carry a fabric index per
//! piece (`Teil.stoff` into `Projekt::stoffe`, checked: 6/18/2/5 over 31
//! pieces), but the pieces are named by CLO (`Pattern_999671`) while a pattern
//! and its placement file use the names from the `.sm2d` (`Bluse 2 -
//! Rueckenteil Mitte Links`). There is no correspondence between the two
//! namings, so a per-piece fabric cannot be assigned from a project without
//! inventing a mapping. This module therefore states the limitation instead of
//! hiding it: the run takes one fabric for the whole garment, and the report
//! says which one.
//!
//! **Choosing.** The project often holds several fabrics (the reference project
//! has four: `black latex 0.4mm`, `white latex 0.4mm`, `black latex 0.8mm`,
//! `Ribbon_14`). A silent "first one" would be a guess, so `--zprj-stoff <name>`
//! names a fabric by its `qsName` or profile, and a project with several
//! fabrics but no name is an error that lists them - the caller can then choose
//! knowingly. A name that matches nothing names the alternatives as well.
//!
//! [`stoff_aus_projekt`] can also decide without being told (`wahl = None`),
//! but only for a project with exactly one fabric. **The command does not use
//! that**: `anziehen --zprj` without `--zprj-stoff` keeps the material it had
//! (the built-in profile or `--material`), because switching those runs to the
//! project's fabric would change what they compute without being asked. A
//! single-fabric project therefore still wants the name; the branch exists for
//! a caller that has already established the count.
//!
//! A **profile** name is not unique in practice: in the reference project
//! `white latex 0.4mm` and `black latex 0.4mm` are both
//! `Latex_0.4mm_Radical_Rubber`. A profile name that matches two fabrics is
//! therefore refused as well, with both named - picking the first would be the
//! same guess as picking the first fabric.
//!
//! Latex is not special-cased here; the fabric with the most pieces would be a
//! plausible default, but "plausible" is not "stated".

use std::path::Path;

use klon3d_pattern::zprj::Projekt;
use klon3d_sim::Material;

/// The fabric a run takes from `projekt`, or a message naming what to choose
/// from.
///
/// `wahl` is the `--zprj-stoff` value: a fabric name (`qsName`) or a profile
/// name. `None` means "the project states exactly one", which is the only case
/// this can decide without being told - `anziehen` always passes `Some`, so
/// that a run without `--zprj-stoff` keeps its material instead of quietly
/// taking the project's only fabric.
pub fn stoff_aus_projekt(
    projekt: &Projekt,
    wahl: Option<&str>,
    pfad: &Path,
) -> Result<Material, String> {
    let stoffe = &projekt.stoffe;
    if stoffe.is_empty() {
        return Err(format!(
            "--zprj {}: das Projekt traegt keinen Stoff (listFabric ist leer); \
             nimm --material fuer den Stoff",
            pfad.display()
        ));
    }

    let treffer: Vec<&klon3d_pattern::zprj::Stoff> = match wahl {
        Some(n) => {
            let gesucht = |s: &klon3d_pattern::zprj::Stoff| {
                s.name.eq_ignore_ascii_case(n)
                    || s.profil
                        .as_deref()
                        .is_some_and(|p| p.eq_ignore_ascii_case(n))
            };
            let t: Vec<_> = stoffe.iter().filter(|s| gesucht(s)).collect();
            if t.is_empty() {
                return Err(format!(
                    "--zprj-stoff {n:?}: kein Stoff dieses Namens in {}; dort stehen: {}",
                    pfad.display(),
                    liste(projekt)
                ));
            }
            t
        }
        None if stoffe.len() == 1 => vec![&stoffe[0]],
        None => {
            return Err(format!(
                "--zprj {}: das Projekt traegt {} Stoffe - welcher gilt, muss --zprj-stoff sagen. \
                 Dort stehen: {}",
                pfad.display(),
                stoffe.len(),
                liste(projekt)
            ))
        }
    };

    if treffer.len() > 1 {
        return Err(format!(
            "--zprj-stoff {:?}: {} Stoffe passen in {}; bitte den vollen Namen nennen: {}",
            wahl.unwrap_or(""),
            treffer.len(),
            pfad.display(),
            liste(projekt)
        ));
    }

    klon3d_pipeline::stoff::aus_zprj_stoff(treffer[0], pfad).map_err(|e| e.to_string())
}

/// The fabrics of a project as one line, for every message above: name and
/// profile, so a caller can copy the token it wants.
fn liste(projekt: &Projekt) -> String {
    let mut teile: Vec<String> = projekt
        .stoffe
        .iter()
        .map(|s| match &s.profil {
            Some(p) => format!("{:?} ({p})", s.name),
            None => format!("{:?}", s.name),
        })
        .collect();
    teile.sort();
    teile.join(", ")
}

/// How many pieces of the project use each fabric, in `listFabric` order.
///
/// Not used to choose (see the module doc), but printed: it is the number that
/// shows why "one fabric for the garment" is a statement about this project and
/// not a preference.
pub fn stuecke_je_stoff(projekt: &Projekt) -> Vec<usize> {
    let mut zaehler = vec![0usize; projekt.stoffe.len()];
    for t in &projekt.teile {
        if let Some(i) = t.stoff {
            if let Some(c) = zaehler.get_mut(i) {
                *c += 1;
            }
        }
    }
    zaehler
}
