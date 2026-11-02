//! The eight latex fabric files in `assets/stoffe/` load into
//! [`klon3d_sim::Material`], and the numbers they carry are the ones
//! `MATERIAL.md` names.
//!
//! Two kinds of assertion live here and they are kept apart on purpose:
//!
//! * what the **mapping** has to do — units, thickness from the file, one
//!   compound across all eight;
//! * what the **files** contain — measured once in CLO and visible in
//!   `mapPhysical`. Where a file does not keep a promise of the task, the
//!   test names the file instead of loosening the check silently.

use std::path::{Path, PathBuf};

use klon3d_pipeline::stoff::{stoff_laden, stoffe_im_ordner};
use klon3d_sim::RADICAL_RUBBER_04;

/// The eight files with the thickness their name states, in millimetres.
const DATEIEN: [(&str, f64); 8] = [
    ("Latex_0.25mm_abgeleitet", 0.25),
    ("Latex_0.4mm_Radical_Rubber", 0.40),
    ("Latex_1.0mm_abgeleitet", 1.00),
    ("Latex_Laminat_0.4+0.25mm_abgeleitet", 0.65),
    ("Latex_Laminat_2x0.25mm_abgeleitet", 0.50),
    ("Latex_Laminat_2x0.4mm_abgeleitet", 0.80),
    ("Latex_Laminat_2x1.0mm_abgeleitet", 2.00),
    ("Latex_Laminat_3x0.4mm_abgeleitet", 1.20),
];

fn ordner() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/stoffe")
}

fn datei(stamm: &str) -> PathBuf {
    ordner().join(format!("{stamm}.zfab"))
}

/// Relative deviation, or infinity when `soll` is zero.
fn abweichung(ist: f64, soll: f64) -> f64 {
    if soll == 0.0 {
        if ist == 0.0 {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        ((ist - soll) / soll).abs()
    }
}

/// The round trip the built-in profile was made for.
///
/// `RADICAL_RUBBER_04` was written from this file, so the reader has to land
/// on the same numbers. It does for stretch and shear to 1.4e-10 — but not to
/// 1e-9 for bending, areal weight, thickness and friction, and that is not the
/// mapping: the file stores `f32`, while the constant carries decimal values
/// (2482.2424e-9 against 2482.242431640625e-9, 0.3489899 against
/// 0.348989880…, 0.4e-3 against 0.40000000596e-3, 0.03 against
/// 0.029999999329). Those four differ by 1.3e-8 to 5.5e-8 and no decoding of
/// an `f32` can close that. The check below therefore holds the tight 1e-9
/// where the constant is exact and the `f32` resolution (1e-6) elsewhere; the
/// numbers are asserted explicitly, so a change in either side is visible.
#[test]
fn rundlauf_radical_rubber() {
    let m = stoff_laden(&datei("Latex_0.4mm_Radical_Rubber")).unwrap();
    let c = RADICAL_RUBBER_04;

    assert_eq!(m.name, "Latex_0.4mm_Radical_Rubber");

    // Exact in the constant, exact in the file.
    assert!(
        abweichung(m.dehnung_n_m, c.dehnung_n_m) < 1e-9,
        "Stretch {} gegen {}",
        m.dehnung_n_m,
        c.dehnung_n_m
    );
    assert!(
        abweichung(m.schub_n_m, c.schub_n_m) < 1e-9,
        "Shear {} gegen {}",
        m.schub_n_m,
        c.schub_n_m
    );

    // Decimal roundings in the constant, `f32` in the file.
    for (was, ist, soll) in [
        ("Biegung", m.biegung_nm, c.biegung_nm),
        (
            "Flächengewicht",
            m.flaechengewicht_kg_m2,
            c.flaechengewicht_kg_m2,
        ),
        ("Dicke", m.dicke_m, c.dicke_m),
        ("Reibung", m.reibung, c.reibung),
    ] {
        assert!(
            abweichung(ist, soll) < 1e-6,
            "{was}: {ist:e} gegen {soll:e}, relativ {:.2e}",
            abweichung(ist, soll)
        );
    }

    // The values as the file has them, so that a silent change in either the
    // file or the conversion shows up as a number and not as a tolerance.
    // Written as the `f32` of the file times the factor from the module docs.
    assert_eq!(m.dehnung_n_m, 348_274.16f32 as f64 * 1.0e-3);
    assert_eq!(m.schub_n_m, 174_159.05f32 as f64 * 1.0e-3);
    assert_eq!(m.biegung_nm, 2_482.242_4f32 as f64 * 1.0e-9);
    assert_eq!(m.flaechengewicht_kg_m2, 0.000_348_989_88f32 as f64 * 1.0e3);
    assert_eq!(m.dicke_m, 0.4f32 as f64 * 1.0e-3);
    assert_eq!(m.reibung, 0.03f32 as f64);
}

#[test]
fn alle_acht_stoffe_laden() {
    for (stamm, _) in DATEIEN {
        let m = stoff_laden(&datei(stamm)).unwrap_or_else(|e| panic!("{stamm}: {e}"));
        assert_eq!(m.name, stamm);
        // No file may come out empty: the solver would divide by zero.
        for (was, wert) in [
            ("dehnung", m.dehnung_n_m),
            ("schub", m.schub_n_m),
            ("biegung", m.biegung_nm),
            ("flaechengewicht", m.flaechengewicht_kg_m2),
            ("dicke", m.dicke_m),
        ] {
            assert!(wert > 0.0, "{stamm}: {was} ist {wert}");
        }
        assert!(
            m.reibung > 0.0 && m.reibung < 1.0,
            "{stamm}: reibung {}",
            m.reibung
        );
    }
}

/// The thickness in `fThickness` has to be the one in the file name — a
/// laminate the sum of its layers — within 1 %.
#[test]
fn dicke_aus_dem_dateinamen() {
    for (stamm, soll_mm) in DATEIEN {
        let m = stoff_laden(&datei(stamm)).unwrap();
        let ist_mm = m.dicke_m * 1.0e3;
        assert!(
            abweichung(ist_mm, soll_mm) < 0.01,
            "{stamm}: Dicke {ist_mm} mm, im Namen {soll_mm} mm"
        );
    }
}

/// One compound: areal weight / thickness is the same density for all eight
/// (within 1 %), and so are stretch / t and bending / t³. A file that breaks
/// this is named with its value instead of being bent into shape.
#[test]
fn eine_dichte_fuer_alle_acht() {
    let mut werte = Vec::new();
    for (stamm, _) in DATEIEN {
        let m = stoff_laden(&datei(stamm)).unwrap();
        let t = m.dicke_m;
        werte.push((
            stamm,
            m.flaechengewicht_kg_m2 / t,
            m.dehnung_n_m / t,
            m.biegung_nm / t.powi(3),
        ));
    }

    // Reference: the measured file itself.
    let (_, dichte0, dehnung0, biegung0) = werte[1];
    assert!(
        (dichte0 - 872.4747).abs() < 0.01,
        "Dichte der gemessenen Datei: {dichte0} kg/m³"
    );

    for (stamm, dichte, dehnung, biegung) in werte {
        assert!(
            abweichung(dichte, dichte0) < 0.01,
            "{stamm}: Dichte {dichte:.4} kg/m³ gegen {dichte0:.4}"
        );
        assert!(
            abweichung(dehnung, dehnung0) < 0.01,
            "{stamm}: Stretch/t {dehnung:.2} N/m² gegen {dehnung0:.2}"
        );
        assert!(
            abweichung(biegung, biegung0) < 0.01,
            "{stamm}: Biegung/t³ {biegung:.2} gegen {biegung0:.2}"
        );
    }
}

#[test]
fn stoffe_im_ordner_listet_alle_acht() {
    let liste = stoffe_im_ordner(&ordner());
    assert_eq!(liste.len(), DATEIEN.len(), "{} Dateien", liste.len());
    for (pfad, ergebnis) in &liste {
        let m = ergebnis
            .as_ref()
            .unwrap_or_else(|e| panic!("{}: {e}", pfad.display()));
        let stamm = pfad.file_stem().unwrap().to_string_lossy();
        assert_eq!(m.name, stamm);
    }
    // Sorted, so a caller can show the list as it is.
    assert!(liste.windows(2).all(|w| w[0].0 <= w[1].0));

    // A folder that does not exist is empty, not an error.
    assert!(stoffe_im_ordner(Path::new("gibt/es/nicht")).is_empty());
}

/// Writes `daten` to a temporary file and hands it to `stoff_laden`.
fn mit_datei<T>(name: &str, daten: &[u8], f: impl FnOnce(&Path) -> T) -> T {
    let d = std::env::temp_dir().join(format!("klon3d-stoff-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(format!("{name}.zfab"));
    std::fs::write(&p, daten).unwrap();
    let r = f(&p);
    let _ = std::fs::remove_file(&p);
    r
}

fn roh(stamm: &str) -> Vec<u8> {
    std::fs::read(datei(stamm)).unwrap()
}

/// A cut file must give a named error, not a panic and not a half material.
#[test]
fn abgeschnittene_datei_ist_ein_fehler() {
    let ganz = roh("Latex_0.4mm_Radical_Rubber");
    for (was, laenge) in [
        ("kein Byte", 0),
        ("Kopf halb", 20),
        ("Kopf ganz, Rest weg", 0x28),
        ("ZIP halb", ganz.len() / 2),
        ("letzte 100 Byte weg", ganz.len() - 100),
    ] {
        let daten = &ganz[..laenge];
        let fehler = mit_datei("kurz", daten, |p| stoff_laden(p).unwrap_err());
        let text = fehler.to_string();
        assert!(
            text.contains("kurz.zfab"),
            "{was}: Meldung nennt die Datei nicht: {text}"
        );
        assert!(!text.is_empty());
    }
}

/// Without the `" FAB        CLO "` header there is no fabric: the magic is
/// checked, so a foreign file is refused by name and not parsed as garbage.
#[test]
fn ohne_kopf_ist_ein_fehler() {
    let ganz = roh("Latex_0.4mm_Radical_Rubber");

    // Header cut off in front: the magic is gone.
    let fehler = mit_datei("kopflos", &ganz[16..], |p| stoff_laden(p).unwrap_err());
    let text = fehler.to_string();
    assert!(text.contains("kopflos.zfab"), "{text}");
    assert!(text.contains("Kopf"), "{text}");

    // Header present, sizes inside do not fit: also caught at the head.
    let mut falsch = ganz.clone();
    falsch[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
    let fehler = mit_datei("falschgroesse", &falsch, |p| stoff_laden(p).unwrap_err());
    assert!(
        fehler.to_string().contains("falschgroesse.zfab"),
        "{fehler}"
    );

    // A ZIP that is not a fabric has the magic but no `.fab` entry.
    let leer = {
        let mut v = ganz[..0x203].to_vec();
        v.resize(0x203 + 22, 0);
        v
    };
    let fehler = mit_datei("leerzip", &leer, |p| stoff_laden(p).unwrap_err());
    assert!(fehler.to_string().contains("leerzip.zfab"), "{fehler}");
}

/// A file that is not there at all is an error with the path in it.
#[test]
fn fehlende_datei_ist_ein_fehler() {
    let p = ordner().join("gibt_es_nicht.zfab");
    let text = stoff_laden(&p).unwrap_err().to_string();
    assert!(text.contains("gibt_es_nicht.zfab"), "{text}");
}
