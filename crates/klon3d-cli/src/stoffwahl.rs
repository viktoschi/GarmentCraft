//! The fabric for `--material`: one resolution for every command that takes one.
//!
//! `anziehen` grew the option in #85, `roehre` takes it since #87. Both must
//! resolve the same three kinds of value in the same order and print the same
//! message on a name that is none of them:
//!
//! 1. a built-in profile (`radical`, `retifist`), so a run from before the
//!    option grew keeps its material and needs no folder;
//! 2. a path to a `.zfab`, read wherever it lies;
//! 3. the stem of a file in `assets/stoffe` - the eight latex fabrics that
//!    [`klon3d_pipeline::stoff`] reads.
//!
//! The folder is searched upwards from the program's own folder and then from
//! the current folder, the same search `klon3d-app` makes. The command line
//! does not depend on `klon3d-app`, so the few lines stand here.

use std::path::{Path, PathBuf};

use klon3d_pipeline as pipeline;
use klon3d_sim::Material;

/// The folder `assets/stoffe` in `start` or in one of the folders above it.
pub fn stoffordner_ab(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|o| o.join("assets").join("stoffe"))
        .find(|p| p.is_dir())
}

/// The fabrics folder: `assets/stoffe` searched upwards from the folder of the
/// program, then from the current folder. The first hit wins.
pub fn stoffordner() -> Option<PathBuf> {
    let programm = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let aktuell = std::env::current_dir().ok();
    [programm, aktuell]
        .into_iter()
        .flatten()
        .find_map(|s| stoffordner_ab(&s))
}

/// The fabric for `--material`: a built-in profile, a `.zfab` path, or the stem
/// of a file in the fabrics folder.
///
/// The order is the one the option promises: the two built-in profiles keep
/// their names (a run from before this option grew keeps its material), then a
/// path, then a stem. A name that is none of the three is an error that says
/// where it looked and what it found there - not a silent run at 0.4 mm.
pub fn stoff_waehlen(angabe: &str) -> Result<Material, String> {
    stoff_aufloesen(angabe, stoffordner().as_deref())
}

/// [`stoff_waehlen`] with the fabrics folder handed in, so that the search can
/// be tested without the repository.
pub fn stoff_aufloesen(angabe: &str, ordner: Option<&Path>) -> Result<Material, String> {
    if let Some(m) = Material::nach_namen(angabe) {
        return Ok(m);
    }
    let pfad = Path::new(angabe);
    if pfad
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zfab"))
    {
        return pipeline::stoff::stoff_laden(pfad)
            .map_err(|e| format!("--material {angabe:?}: {e}"));
    }
    let Some(ordner) = ordner else {
        return Err(format!(
            "--material {angabe:?}: kein eingebautes Profil (radical, retifist), und kein Ordner \
             assets/stoffe gefunden - gesucht ab dem Programmordner und dem aktuellen Ordner aufwaerts"
        ));
    };
    let stoffe = pipeline::stoff::stoffe_im_ordner(ordner);
    let stamm = |p: &Path| {
        p.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let treffer = stoffe
        .iter()
        .find(|(p, _)| stamm(p).eq_ignore_ascii_case(angabe));
    match treffer {
        // A file that exists and does not load names itself: the error of
        // `stoff_laden` carries the path and the field at fault.
        Some((_, Ok(m))) => Ok(*m),
        Some((_, Err(e))) => Err(format!("--material {angabe:?}: {e}")),
        None => {
            let mut namen: Vec<String> = stoffe.iter().map(|(p, _)| stamm(p)).collect();
            namen.sort();
            Err(format!(
                "--material {angabe:?}: kein eingebautes Profil (radical, retifist) und kein Stoff \
                 dieses Namens in {}; dort liegen: {}",
                ordner.display(),
                if namen.is_empty() {
                    "keine .zfab-Datei".to_string()
                } else {
                    namen.join(", ")
                }
            ))
        }
    }
}

/// A thickness in metres as millimetres, with as few decimals as it needs but
/// at least one.
///
/// One decimal is not enough for the fabrics this option selects: 0.25 mm
/// printed with one decimal reads "0.2 mm" and cannot be told apart from a
/// 0.2 mm sheet. Three decimals reach the micrometre, which is finer than any
/// fabric file here, and the trailing zeros go away so 1.0 mm stays "1.0 mm"
/// and not "1.000 mm".
pub fn dicke_gedruckt(dicke_m: f64) -> String {
    let mut s = format!("{:.3}", dicke_m * 1e3);
    while s.ends_with('0') && s.split('.').nth(1).is_some_and(|n| n.len() > 1) {
        s.pop();
    }
    if s.ends_with('.') {
        s.push('0');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight files as `klon3d-pipeline`'s own test names them.
    fn stoffe() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/stoffe")
    }

    /// The two built-in profiles keep their names: a run from before the option
    /// grew must not change its material, and neither name may need a folder.
    #[test]
    fn eingebaute_profile_bleiben() {
        for (name, soll) in [
            ("radical", klon3d_sim::RADICAL_RUBBER_04),
            ("retifist", Material::nach_namen("retifist").unwrap()),
        ] {
            let m = stoff_aufloesen(name, None).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(m.name, soll.name);
            assert_eq!(m.dicke_m, soll.dicke_m);
        }
        // The long names of the profiles still work, as before.
        assert_eq!(
            stoff_aufloesen("Latex_0.4mm_Radical_Rubber", None)
                .unwrap()
                .name,
            klon3d_sim::RADICAL_RUBBER_04.name
        );
    }

    /// A stem from the fabrics folder is read: this is the path the solver
    /// could not reach before.
    #[test]
    fn stamm_aus_dem_stoffordner() {
        let o = stoffe();
        let m = stoff_aufloesen("Latex_1.0mm_abgeleitet", Some(&o)).unwrap();
        assert_eq!(m.name, "Latex_1.0mm_abgeleitet");
        assert!(
            (m.dicke_m * 1e3 - 1.0).abs() < 0.01,
            "Dicke {} mm",
            m.dicke_m * 1e3
        );
        // Every one of the eight is selectable by its stem.
        for (pfad, _) in pipeline::stoff::stoffe_im_ordner(&o) {
            let stamm = pfad.file_stem().unwrap().to_string_lossy().into_owned();
            let m = stoff_aufloesen(&stamm, Some(&o)).unwrap_or_else(|e| panic!("{stamm}: {e}"));
            assert_eq!(m.name, stamm);
        }
    }

    /// Case does not matter for a stem; the shell and the file system disagree
    /// often enough about it.
    #[test]
    fn stamm_gross_klein_egal() {
        let o = stoffe();
        let m = stoff_aufloesen("latex_laminat_2x0.4mm_abgeleitet", Some(&o)).unwrap();
        assert_eq!(m.name, "Latex_Laminat_2x0.4mm_abgeleitet");
        assert!(
            (m.dicke_m * 1e3 - 0.8).abs() < 0.01,
            "Dicke {} mm",
            m.dicke_m * 1e3
        );
    }

    /// A path to a `.zfab` is read directly, wherever it lies.
    #[test]
    fn pfad_auf_eine_datei() {
        let p = stoffe().join("Latex_Laminat_3x0.4mm_abgeleitet.zfab");
        let m = stoff_aufloesen(&p.display().to_string(), None).unwrap();
        assert_eq!(m.name, "Latex_Laminat_3x0.4mm_abgeleitet");
        // The extension is compared case-insensitively.
        let gross = p.display().to_string().replace(".zfab", ".ZFAB");
        let m = stoff_aufloesen(&gross, None);
        assert!(
            m.is_ok() || m.unwrap_err().contains(".ZFAB"),
            "eine andere Endung wurde nicht als .zfab erkannt"
        );
    }

    /// A path that is no fabric says which file and which field - not a run at
    /// 0.4 mm.
    #[test]
    fn kaputte_datei_nennt_sich_selbst() {
        let d = std::env::temp_dir().join(format!("klon3d-cli-stoff-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("halbe.zfab");
        std::fs::write(&p, b"kein Kopf").unwrap();
        let e = stoff_aufloesen(&p.display().to_string(), None).unwrap_err();
        assert!(e.contains("halbe.zfab"), "{e}");
        let _ = std::fs::remove_file(&p);
    }

    /// An unknown name is an error that names the folder and lists what is in
    /// it, so the caller does not have to guess the spelling.
    #[test]
    fn unbekannter_name_nennt_ordner_und_stoffe() {
        let o = stoffe();
        let e = stoff_aufloesen("Latex_0.5mm", Some(&o)).unwrap_err();
        assert!(
            e.contains(&o.display().to_string()),
            "Ordner fehlt in der Meldung: {e}"
        );
        assert!(
            e.contains("Latex_0.25mm_abgeleitet"),
            "die vorhandenen Stoffe fehlen: {e}"
        );
        assert!(e.contains("radical"), "die eingebauten Profile fehlen: {e}");
    }

    /// Without a fabrics folder the message says so instead of listing nothing.
    #[test]
    fn ohne_ordner_sagt_dass_er_fehlt() {
        let e = stoff_aufloesen("Latex_1.0mm_abgeleitet", None).unwrap_err();
        assert!(e.contains("assets/stoffe"), "{e}");
        assert!(e.contains("kein Ordner"), "{e}");
    }

    /// The printed thickness tells the fabrics apart: one decimal would print
    /// 0.25 mm as "0.2 mm", the same as a 0.2 mm sheet, and the files a run may
    /// select start at 0.25 mm.
    #[test]
    fn dicke_wird_unterscheidbar_gedruckt() {
        assert_eq!(dicke_gedruckt(0.25e-3), "0.25");
        assert_eq!(dicke_gedruckt(0.4e-3), "0.4");
        assert_eq!(dicke_gedruckt(1.0e-3), "1.0");
        assert_eq!(dicke_gedruckt(0.65e-3), "0.65");
        assert_eq!(dicke_gedruckt(2.0e-3), "2.0");
        // Every fabric in the folder prints a thickness of its own.
        let mut gedruckt: Vec<String> = pipeline::stoff::stoffe_im_ordner(&stoffe())
            .iter()
            .filter_map(|(_, m)| m.as_ref().ok().map(|m| dicke_gedruckt(m.dicke_m)))
            .collect();
        let anzahl = gedruckt.len();
        gedruckt.sort();
        gedruckt.dedup();
        assert_eq!(
            gedruckt.len(),
            anzahl,
            "zwei Stoffe drucken dieselbe Dicke: {gedruckt:?}"
        );
    }

    /// The eight files are one compound at eight thicknesses, and the solver
    /// scales whatever was selected to the thickness the placement file gives
    /// the piece (`Material::mit_dicke`). Where every piece carries a `dicke`,
    /// choosing between the eight therefore changes nothing in the solver - the
    /// values arrive normalised to the piece, and only the name on the material
    /// line tells the run apart. A different compound (`retifist`) does change
    /// them, which is what this pins.
    #[test]
    fn die_acht_stoffe_sind_ein_stoff_in_acht_dicken() {
        let o = stoffe();
        let dicke = 2.0e-3;
        let a = stoff_aufloesen("Latex_0.25mm_abgeleitet", Some(&o))
            .unwrap()
            .mit_dicke(dicke);
        let b = stoff_aufloesen("Latex_Laminat_2x1.0mm_abgeleitet", Some(&o))
            .unwrap()
            .mit_dicke(dicke);
        for (x, y) in [
            (a.dehnung_n_m, b.dehnung_n_m),
            (a.kette_n_m, b.kette_n_m),
            (a.schub_n_m, b.schub_n_m),
            (a.biegung_nm, b.biegung_nm),
            (a.flaechengewicht_kg_m2, b.flaechengewicht_kg_m2),
        ] {
            assert!((x - y).abs() <= 1e-9 * x.abs().max(1.0), "{x} gegen {y}");
        }
        // The names still say which file the caller picked, so a run stays
        // traceable even where the numbers agree.
        assert_eq!(a.name, "Latex_0.25mm_abgeleitet");
        assert_eq!(b.name, "Latex_Laminat_2x1.0mm_abgeleitet");
        // A different compound keeps its own stiffness at the same thickness.
        let r = Material::nach_namen("retifist").unwrap().mit_dicke(dicke);
        assert!(
            r.dehnung_n_m < 0.2 * a.dehnung_n_m,
            "Retifist {} gegen {}",
            r.dehnung_n_m,
            a.dehnung_n_m
        );
        assert_eq!(r.dicke_m, dicke);
    }

    /// The folder search walks upwards from a subfolder, like the app's.
    #[test]
    fn stoffordner_wird_aufwaerts_gefunden() {
        let o = stoffe();
        let tief = o.join("gibt/es/nicht");
        assert_eq!(stoffordner_ab(&tief), Some(o.clone()));
        // An unrelated folder has none.
        let leer = std::env::temp_dir().join(format!("klon3d-cli-ohne-{}", std::process::id()));
        std::fs::create_dir_all(&leer).unwrap();
        assert_eq!(stoffordner_ab(&leer), None);
    }
}
