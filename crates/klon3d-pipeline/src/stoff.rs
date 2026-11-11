//! Fabrics from CLO `.zfab` files for the solver.
//!
//! The file itself is read by [`klon3d_pattern::zfab`] - container, head and
//! property stream live there, together with the test table of all eight
//! files. This module is the **second step only**: it maps a parsed
//! [`Zfab`] onto the SI [`Material`] the solver needs, and it is the place
//! that *demands* a value. A field the file does not state is an error here,
//! because the solver cannot run on "not stated" - the parser keeps it as
//! `None` instead of inventing a zero.
//!
//! There are two entry points and one table, because a `.zfab` and a `.zprj`
//! state a fabric in the same units: [`aus_zfab`] for a file that *is* one
//! fabric, [`aus_zprj_stoff`] for one fabric out of a project that holds many.
//! The project side was added in #131 - before that a `.zprj` could hand over
//! the body but not its fabric.
//!
//! The numbers live in `mapPhysical`; the field names and units are the ones
//! CLO's Property Editor shows ([`MATERIAL.md`], which also lists the values
//! of all eight files):
//!
//! | field | unit in the file | [`Material`] | factor |
//! |---|---|---|---|
//! | `fSuK` (Stretch weft) | g/s² = mN/m | `dehnung_n_m` (N/m) | 1e-3 |
//! | `fSvK` (Stretch warp) | g/s² | `kette_n_m` (N/m) | 1e-3 |
//! | `fHK` (Shear) | g/s² | `schub_n_m` (N/m) | 1e-3 |
//! | `fBuK` (Bending weft) | g·mm²/s²/rad | `biegung_nm` (N·m) | 1e-9 |
//! | `fBvK` (Bending warp) | g·mm²/s²/rad | `biegung_kette_nm` (N·m) | 1e-9 |
//! | `fBhK` (Bending bias) | g·mm²/s²/rad | `biegung_schraege_nm` (N·m) | 1e-9 |
//! | `fDensity` | g/mm² | `flaechengewicht_kg_m2` | 1e+3 |
//! | `fThickness` | mm | `dicke_m` (m) | 1e-3 |
//! | `fFriction` | – | `reibung` | 1 |
//!
//! **Weft on purpose.** Latex is direction independent, but CLO stored the
//! measured curve slightly differently per direction (`fSvK` 348 282,
//! `fBuK` 2 482.24 against `fBvK`/`fBhK` 2 486.49 at 0.4 mm). The built-in
//! profile `RADICAL_RUBBER_04` carries the weft values, so the weft is what
//! keeps the round trip exact. Nothing is averaged.
//!
//! Since #129 the two other bending directions reach the solver as well
//! ([`Material::biegung_kette_nm`], [`Material::biegung_schraege_nm`]) and
//! [`klon3d_sim::biegung`] interpolates between the three. `biegung_nm` stays
//! the **weft** value, so every caller without a grainline keeps the number it
//! had.
//!
//! [`MATERIAL.md`]: ../../../MATERIAL.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use klon3d_pattern::zfab::Zfab;
use klon3d_sim::Material;

use crate::Fehler;

/// The error of this module carries the file it is about.
fn fehler(pfad: &Path, was: &str) -> Fehler {
    Fehler::Datei(format!("{}: {was}", pfad.display()))
}

/// Reads one `.zfab` into a [`Material`] the solver can use.
///
/// The name is the file stem (`Latex_0.4mm_Radical_Rubber`), not the
/// `qsName` inside the file: they agree in all eight files, and the stem is
/// what the caller selected.
pub fn stoff_laden(pfad: &Path) -> Result<Material, Fehler> {
    let daten = std::fs::read(pfad).map_err(|e| fehler(pfad, &e.to_string()))?;
    let stamm = pfad
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| pfad.display().to_string());
    aus_bytes(&daten, &stamm, pfad)
}

/// Every `.zfab` in `ordner`, sorted by path, one result per file.
///
/// A missing or unreadable folder gives an empty list: a fabric folder is not
/// an error in the sense of this chain, the caller shows what it has. A file
/// that does not load appears in the list with its error instead of being
/// dropped silently.
pub fn stoffe_im_ordner(ordner: &Path) -> Vec<(PathBuf, Result<Material, Fehler>)> {
    let Ok(eintraege) = std::fs::read_dir(ordner) else {
        return Vec::new();
    };
    let mut pfade: Vec<PathBuf> = eintraege
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("zfab"))
        })
        .collect();
    pfade.sort();
    pfade
        .into_iter()
        .map(|p| {
            let m = stoff_laden(&p);
            (p, m)
        })
        .collect()
}

/// The mapping itself, from the bytes of a `.zfab`. `stamm` is the name the
/// [`Material`] gets, `pfad` only labels the error message.
fn aus_bytes(daten: &[u8], stamm: &str, pfad: &Path) -> Result<Material, Fehler> {
    // The parser names its own failure (head, ZIP, `.fab`, completeness); the
    // caller only adds the file it was reading.
    let z = Zfab::lies_bytes(daten, &pfad.display().to_string())
        .map_err(|e| fehler(pfad, &e.to_string()))?;
    aus_zfab(&z, stamm, pfad)
}

/// Maps a parsed fabric onto the solver's SI material.
///
/// Every value is demanded: a file that does not state its thickness is an
/// error naming the field, not a material with a zero in it.
fn aus_zfab(z: &Zfab, stamm: &str, pfad: &Path) -> Result<Material, Fehler> {
    // The weft bending is the one value the material cannot do without, so it
    // is read once and the other two directions fall back to it.
    let biegung_weft = pflicht(z.bending_weft, "fBuK", pfad)?;
    Ok(Material {
        name: name_static(stamm),
        dehnung_n_m: pflicht(z.stretch_weft_g_s2, "fSuK", pfad)? * 1.0e-3,
        kette_n_m: pflicht(z.stretch_warp_g_s2, "fSvK", pfad)? * 1.0e-3,
        schub_n_m: pflicht(z.shear_g_s2, "fHK", pfad)? * 1.0e-3,
        biegung_nm: biegung_weft * 1.0e-9,
        // Stated by every one of the eight files, but not demanded: a file that
        // gives only the weft bending is an isotropic fabric, and inventing an
        // error for it would refuse a usable material. `fBuK` above stays the
        // field that must be there.
        biegung_kette_nm: z.bending_warp.unwrap_or(biegung_weft) * 1.0e-9,
        biegung_schraege_nm: z.bending_bias.unwrap_or(biegung_weft) * 1.0e-9,
        flaechengewicht_kg_m2: pflicht(z.dichte_g_mm3, "fDensity", pfad)? * 1.0e3,
        dicke_m: pflicht(z.dicke_mm, "fThickness", pfad)? * 1.0e-3,
        reibung: pflicht(z.reibung, "fFriction", pfad)?,
    })
}

/// Maps a fabric **read from a `.zprj` project** onto the solver's material.
///
/// The same `mapPhysical` fields and the same factors as [`aus_zfab`] - a
/// project and a `.zfab` state a fabric in the same units. The difference is
/// where the name comes from: a `.zfab` is one fabric and its stem names it,
/// while a project holds many, so the name is the fabric's own `qsName` and the
/// caller picks *which* fabric of the project it means.
///
/// What is demanded: thickness, areal weight, stretch (weft), shear and bending
/// (weft). Those are what the solver cannot run without. The two remaining
/// directions fall back to the weft value, the same rule `aus_zfab` uses for
/// bending, and a file that does not state its friction gets CLO's default 0.03
/// - every one of the 207 fabrics in the 47 foreign projects states it, so the
/// fallback is a decision for future files, not a silent repair of the current
/// ones.
pub fn aus_zprj_stoff(
    s: &klon3d_pattern::zprj::Stoff,
    pfad: &Path,
) -> Result<Material, Fehler> {
    let schuss = pflicht(s.dehnung_schuss, "fSuK", pfad)?;
    let schub = pflicht(s.schub, "fHK", pfad)?;
    // Bending weft is demanded once; the two other directions fall back to it
    // so that no branch can quietly produce a zero stiffness.
    let biegung = pflicht(s.biegung_schuss, "fBuK", pfad)?;
    Ok(Material {
        name: name_static(&s.name),
        dehnung_n_m: schuss * 1.0e-3,
        kette_n_m: s.dehnung_kette.unwrap_or(schuss) * 1.0e-3,
        schub_n_m: schub * 1.0e-3,
        biegung_nm: biegung * 1.0e-9,
        biegung_kette_nm: s.biegung_kette.unwrap_or(biegung) * 1.0e-9,
        biegung_schraege_nm: s.biegung_schraege.unwrap_or(biegung) * 1.0e-9,
        flaechengewicht_kg_m2: pflicht(s.gewicht_g_m2, "fDensity", pfad)? * 1.0e-3,
        dicke_m: pflicht(s.dicke_mm, "fThickness", pfad)? * 1.0e-3,
        reibung: s.reibung.unwrap_or(0.03),
    })
}

/// One field the material cannot do without; a missing one is named rather
/// than replaced by a default.
fn pflicht(wert: Option<f64>, feld: &str, pfad: &Path) -> Result<f64, Fehler> {
    wert.ok_or_else(|| fehler(pfad, &format!("mapPhysical.{feld} fehlt oder ist keine f32")))
}

/// [`Material::name`] is `&'static str` and `Material` is `Copy`; the name
/// therefore cannot live inside it. Each distinct file stem is handed out
/// once and kept for the lifetime of the process — a few dozen bytes for the
/// eight files here, nothing that grows while the same fabric is loaded
/// again.
fn name_static(name: &str) -> &'static str {
    static NAMEN: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut karte = NAMEN
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(&n) = karte.get(name) {
        return n;
    }
    let n: &'static str = Box::leak(name.to_owned().into_boxed_str());
    karte.insert(name.to_owned(), n);
    n
}

