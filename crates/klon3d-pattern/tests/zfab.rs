#![allow(non_snake_case)] // Tests sind deutsch benannt

//! Prüft den `.zfab`-Leser gegen die acht Stoffdateien in `assets/stoffe`.
//!
//! **Warum diese Tests zählen:** Die Sollwerte sind in CLO (Fassung vor 5.2) im
//! Property Editor abgelesen und in `../latex-stoffe/werte/latex_profile.csv`
//! festgehalten - außerhalb dieses Repos. Stimmen die gelesenen Zahlen mit
//! dieser Tabelle überein, ist die Feldbedeutung belegt und nicht geraten.
//!
//! Toleranz: `f32` in der Datei gegen vier Nachkommastellen im CSV, also
//! `1e-4` relativ. Das ist die Genauigkeit der Tabelle, nicht des Lesers.

use klon3d_pattern::zfab::{Zfab, ZfabFehler};
use std::path::PathBuf;

fn stoffe() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/stoffe")
}

/// Ein Sollwert-Satz aus `latex_profile.csv`: Spalten dicke_mm, g_m2,
/// stretch_weft_g_s2, stretch_warp_g_s2, shear_g_s2, bending_weft, bending_warp,
/// bending_bias, reibung, daempfung - auf vier Nachkommastellen gerundet.
struct Soll {
    datei: &'static str,
    dicke_mm: f64,
    gewicht_g_m2: f64,
    stretch_weft: f64,
    stretch_warp: f64,
    shear: f64,
    bending_weft: f64,
    bending_warp: f64,
    bending_bias: f64,
    reibung: f64,
    daempfung: f64,
}

const TABELLE: [Soll; 8] = [
    Soll {
        datei: "Latex_0.4mm_Radical_Rubber.zfab",
        dicke_mm: 0.4,
        gewicht_g_m2: 348.9899,
        stretch_weft: 348274.1562,
        stretch_warp: 348281.7188,
        shear: 174159.0469,
        bending_weft: 2482.2424,
        bending_warp: 2486.4924,
        bending_bias: 2486.4924,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_0.25mm_abgeleitet.zfab",
        dicke_mm: 0.25,
        gewicht_g_m2: 218.1187,
        stretch_weft: 217671.3438,
        stretch_warp: 217676.0781,
        shear: 108849.4062,
        bending_weft: 606.0162,
        bending_warp: 607.0538,
        bending_bias: 607.0538,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_1.0mm_abgeleitet.zfab",
        dicke_mm: 1.0,
        gewicht_g_m2: 872.4747,
        stretch_weft: 870685.3750,
        stretch_warp: 870704.3125,
        shear: 435397.6250,
        bending_weft: 38785.0352,
        bending_warp: 38851.4414,
        bending_bias: 38851.4414,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_Laminat_2x0.25mm_abgeleitet.zfab",
        dicke_mm: 0.5,
        gewicht_g_m2: 436.2373,
        stretch_weft: 435342.6875,
        stretch_warp: 435352.1562,
        shear: 217698.8125,
        bending_weft: 4848.1294,
        bending_warp: 4856.4302,
        bending_bias: 4856.4302,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_Laminat_0.4+0.25mm_abgeleitet.zfab",
        dicke_mm: 0.65,
        gewicht_g_m2: 567.1086,
        stretch_weft: 565945.5000,
        stretch_warp: 565957.8125,
        shear: 283008.4375,
        bending_weft: 10651.3408,
        bending_warp: 10669.5771,
        bending_bias: 10669.5771,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_Laminat_2x0.4mm_abgeleitet.zfab",
        dicke_mm: 0.8,
        gewicht_g_m2: 697.9798,
        stretch_weft: 696548.3125,
        stretch_warp: 696563.4375,
        shear: 348318.0938,
        bending_weft: 19857.9395,
        bending_warp: 19891.9395,
        bending_bias: 19891.9395,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_Laminat_3x0.4mm_abgeleitet.zfab",
        dicke_mm: 1.2,
        gewicht_g_m2: 1046.9697,
        stretch_weft: 1044822.4375,
        stretch_warp: 1044845.1250,
        shear: 522477.1250,
        bending_weft: 67020.5391,
        bending_warp: 67135.2891,
        bending_bias: 67135.2891,
        reibung: 0.03,
        daempfung: 0.0001,
    },
    Soll {
        datei: "Latex_Laminat_2x1.0mm_abgeleitet.zfab",
        dicke_mm: 2.0,
        gewicht_g_m2: 1744.9494,
        stretch_weft: 1741370.7500,
        stretch_warp: 1741408.6250,
        shear: 870795.2500,
        bending_weft: 310280.2812,
        bending_warp: 310811.5312,
        bending_bias: 310811.5312,
        reibung: 0.03,
        daempfung: 0.0001,
    },
];

fn nah(ist: Option<f64>, soll: f64, was: &str) {
    let ist = ist.unwrap_or_else(|| panic!("{was} fehlt"));
    let abweichung = (ist - soll).abs() / soll.abs().max(1e-9);
    assert!(
        abweichung <= 1e-4,
        "{was}: gelesen {ist}, Tabelle {soll} (relativ {abweichung:e})"
    );
}

fn lies(soll: &Soll) -> Zfab {
    Zfab::lies(stoffe().join(soll.datei)).unwrap_or_else(|e| panic!("{}: {e}", soll.datei))
}

#[test]
fn liest_alle_acht_stoffe_gegen_die_tabelle() {
    for soll in &TABELLE {
        let datei = soll.datei;
        let z = lies(soll);

        // CLO schreibt `qsName` und das Physikprofil gleich dem Dateistamm.
        let stamm = datei.trim_end_matches(".zfab");
        assert_eq!(z.name.as_deref(), Some(stamm), "{datei}: qsName");
        assert_eq!(z.profil.as_deref(), Some(stamm), "{datei}: Physikprofil");

        nah(z.dicke_mm, soll.dicke_mm, &format!("{datei}: fThickness"));
        nah(
            z.gewicht_g_m2(),
            soll.gewicht_g_m2,
            &format!("{datei}: fDensity"),
        );
        nah(
            z.stretch_weft_g_s2,
            soll.stretch_weft,
            &format!("{datei}: fSuK"),
        );
        nah(
            z.stretch_warp_g_s2,
            soll.stretch_warp,
            &format!("{datei}: fSvK"),
        );
        nah(z.shear_g_s2, soll.shear, &format!("{datei}: fHK"));
        nah(z.bending_weft, soll.bending_weft, &format!("{datei}: fBuK"));
        nah(z.bending_warp, soll.bending_warp, &format!("{datei}: fBvK"));
        nah(z.bending_bias, soll.bending_bias, &format!("{datei}: fBhK"));
        nah(z.reibung, soll.reibung, &format!("{datei}: fFriction"));
        nah(z.daempfung, soll.daempfung, &format!("{datei}: fIDS"));

        // Latex hat keine Fäden: Knickverhältnis 0, Knicksteife 0
        // (MATERIAL.md, Tabelle der CLO-Felder).
        for (wert, was) in [
            (z.buckling_ratio, "fBuLR"),
            (z.buckling_stiffness_weft, "fBucklingStiffnessU"),
            (z.buckling_stiffness_warp, "fBucklingStiffnessV"),
            (z.buckling_stiffness_bias, "fBucklingStiffnessH"),
        ] {
            assert_eq!(wert, Some(0.0), "{datei}: {was}");
        }
    }
}

#[test]
fn latex_dehnung_ist_richtungsunabhaengig_und_schub_halbe_dehnung() {
    // MATERIAL.md: "direction-independent" (Weft = Warp) und "Shear = Stretch/2".
    // Zwei Beziehungen, die kein einzelner Tabellenwert zeigt - ein falsch
    // zugeordnetes Feld würde hier auffallen.
    //
    // Gemessen weichen beide um mehr als Rundung ab: Weft/Warp um 2,2e-5,
    // Schub gegen Weft/2 um 1,3e-4 (0,4 mm: 348 274,16 / 2 = 174 137,08 gegen
    // die geschriebenen 174 159,05). Beides ist über alle acht Dateien
    // gleich, also eine Eigenschaft der CLO-Werte und kein Lesefehler -
    // deshalb 1e-3, nicht 1e-5.
    for soll in &TABELLE {
        let z = lies(soll);
        let weft = z.stretch_weft_g_s2.unwrap();
        let warp = z.stretch_warp_g_s2.unwrap();
        let shear = z.shear_g_s2.unwrap();
        assert!(
            (weft - warp).abs() / weft < 1e-3,
            "{}: Weft {weft} != Warp {warp}",
            soll.datei
        );
        assert!(
            (shear - weft / 2.0).abs() / shear < 1e-3,
            "{}: Shear {shear} != Weft/2 {}",
            soll.datei,
            weft / 2.0
        );
    }
}

#[test]
fn alle_acht_tragen_die_rohdaten_des_fabric_kits() {
    // Die Spalte `fabric_kit_rohdaten` der Tabelle sagt für alle acht `ja`.
    for soll in &TABELLE {
        let z = lies(soll);
        assert!(z.hat_rohdaten, "{}: mapRawData fehlt", soll.datei);
    }
}

#[test]
fn dicke_reicht_von_025_bis_2_mm() {
    // Die acht Dateien decken nach MATERIAL.md 0,25 bis 2,0 mm ab; ein
    // vertauschtes Feld würde die Reihe sprengen.
    let mut dicken: Vec<f64> = TABELLE.iter().map(|s| lies(s).dicke_mm.unwrap()).collect();
    dicken.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (ist, soll) in dicken.iter().zip([0.25, 0.4, 0.5, 0.65, 0.8, 1.0, 1.2, 2.0]) {
        assert!((ist - soll).abs() < 1e-5, "Dicke {ist}, erwartet {soll}");
    }
}

#[test]
fn datei_ohne_kopf_gibt_keinen_kopf_statt_panik() {
    let daten = std::fs::read(stoffe().join("Latex_0.4mm_Radical_Rubber.zfab")).unwrap();
    // Ein abgeschnittener Kopf: die Kopfregel `u32[0x20] + u32[0x24] == Länge`
    // geht nicht mehr auf.
    let fall = match Zfab::lies_bytes(&daten[..daten.len() - 100], "abgeschnitten") {
        Err(e) => e,
        Ok(_) => panic!("abgeschnittene Datei wurde gelesen"),
    };
    assert!(matches!(fall, ZfabFehler::KeinKopf(_)), "{fall}");

    // Eine leere Datei ebenso - kein Indexpanzer.
    assert!(matches!(
        Zfab::lies_bytes(&[], "leer"),
        Err(ZfabFehler::KeinKopf(_))
    ));

    // Ein fremdes ZIP mit gültigem Inneren, aber ohne die Typkennung
    // `" FAB        CLO "` in den ersten 16 Byte: die Datei wird benannt
    // abgelehnt, nicht als Stoff gedeutet.
    let mut fremd = daten.clone();
    fremd[..16].copy_from_slice(b" PK         ZIP ");
    assert!(matches!(
        Zfab::lies_bytes(&fremd, "fremd"),
        Err(ZfabFehler::KeinKopf(_))
    ));
}

#[test]
fn beschaedigtes_zip_gibt_einen_zipfehler() {
    let mut daten = std::fs::read(stoffe().join("Latex_0.4mm_Radical_Rubber.zfab")).unwrap();
    // Kopf bleibt gültig; das zentrale Verzeichnis am ZIP-Ende wird zerstört.
    let n = daten.len();
    for b in &mut daten[n - 22..] {
        *b = 0;
    }
    match Zfab::lies_bytes(&daten, "kaputt") {
        Err(ZfabFehler::Zip(_)) => {}
        Err(e) => panic!("falscher Fehler: {e}"),
        Ok(_) => panic!("kaputtes ZIP wurde gelesen"),
    }
}
