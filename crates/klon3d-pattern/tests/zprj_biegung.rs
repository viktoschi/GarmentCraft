#![allow(non_snake_case)] // Tests sind deutsch benannt

//! Die Biegewerte eines `.zprj` (#131): bis dahin las der Projektleser nur
//! Dicke, Gewicht und Profilname, obwohl der Solver seit #129 richtungsabhängig
//! rechnen kann. Ein CLO-Projekt konnte seine eigene Anisotropie also gar nicht
//! einbringen.
//!
//! Dieser Test hält beide Enden fest: an der Referenzdatei die **bekannten**
//! Werte des Latex-Profils (0,17 % Spanne - das ist der Stoff, mit dem klon3d
//! ohnehin rechnet), und über die fremden Projekte die **Verteilung**, damit
//! eine spätere Änderung am Leser nicht still die halbe Menge verliert.
//!
//! Was er nicht prüft: ob die Werte im Solver ankommen. Das tut
//! `klon3d-sim/tests/biegung.rs` mit einem synthetischen Stoff - ein `.zprj`
//! kann heute noch nicht angezogen werden, weil ihm die Lagedatei fehlt.

use klon3d_pattern::zprj::Projekt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PRUEFPROJEKT: &str =
    r"D:\ai\rea\work\messung\japanese_school_uniform\japanese_school_uniform.zprj";
const FREMD: &str = r"D:\ai\rea\downloads\clo3d-patterns";

fn fremd_ordner() -> PathBuf {
    std::env::var_os("KLON3D_FREMD").map_or_else(|| PathBuf::from(FREMD), PathBuf::from)
}

fn fremde() -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "zprj") {
                out.push(p);
            }
        }
    }
    walk(&fremd_ordner(), &mut out);
    out.sort();
    out
}

/// Spanne zwischen dem weichsten und dem steifsten der drei Werte, in Prozent
/// des weichsten. `None`, wenn ein Wert fehlt oder nicht positiv ist.
fn spanne(s: &klon3d_pattern::zprj::Stoff) -> Option<f64> {
    let [a, b, c] = [s.biegung_schuss?, s.biegung_kette?, s.biegung_schraege?];
    let klein = a.min(b).min(c);
    let gross = a.max(b).max(c);
    (klein > 0.0).then(|| (gross / klein - 1.0) * 100.0)
}

/// Das Referenzprojekt trägt vier Stoffe, und der Latex-Stoff darin hat genau
/// die drei Werte, die auch `RADICAL_RUBBER_04` im Repo trägt. Weicht das ab,
/// ist entweder der Feldname falsch oder die Umrechnung - beides würde die
/// Biegung still auf den Schusswert zurückfallen lassen.
#[test]
fn das_Pruefprojekt_traegt_seine_Biegewerte() {
    let p = PathBuf::from(PRUEFPROJEKT);
    if !p.is_file() {
        eprintln!("übersprungen: {PRUEFPROJEKT} fehlt");
        return;
    }
    let projekt = Projekt::lies(&p).expect("Projekt lesbar");
    assert_eq!(projekt.stoffe.len(), 4, "Stoffzahl im Prüfprojekt");

    let latex: Vec<_> = projekt
        .stoffe
        .iter()
        .filter(|s| s.profil.as_deref() == Some("Latex_0.4mm_Radical_Rubber"))
        .collect();
    assert_eq!(latex.len(), 2, "zwei Latexstoffe (schwarz, weiß)");
    for s in latex {
        // Genau die Zahlen aus dem `assets/stoffe`-Profil, in CLOs Einheit.
        let nah = |a: Option<f64>, soll: f64| {
            let a = a.expect("Biegewert fehlt");
            assert!((a - soll).abs() < 0.01, "{a} gegen {soll}");
        };
        nah(s.biegung_schuss, 2482.2424);
        nah(s.biegung_kette, 2486.4924);
        nah(s.biegung_schraege, 2486.4924);
        let sp = spanne(s).expect("alle drei Werte");
        assert!(
            (sp - 0.1712).abs() < 0.0001,
            "Spanne {sp:.4} % - das Repo-Profil liegt bei 0,1712 %"
        );
    }

    // Und der Gegenpol im selben Projekt: ein exakt isotroper Stoff. Er belegt,
    // dass ein fehlender Unterschied als 0 ankommt und nicht als fehlender Wert.
    let iso = projekt
        .stoffe
        .iter()
        .find(|s| s.profil.as_deref() == Some("Trim_Hardware"))
        .expect("Trim_Hardware im Prüfprojekt");
    assert_eq!(
        spanne(iso).expect("alle drei Werte"),
        0.0,
        "Trim_Hardware gibt dreimal denselben Wert"
    );
}

/// Über die 47 fremden Projekte: **jeder** benutzte Stoff trägt alle drei
/// Werte, fast jeder ist richtungsabhängig, und die Extremfälle liegen weit
/// auseinander. Das ist der Nenner, den `klon3d zprj --stoffe` zeigt -
/// `listFabric` je Projekt, nicht jeder Physik-Eintrag im Property-Stream
/// (der enthält zusätzlich Blockbibliotheken und Archive).
#[test]
fn die_fremden_Projekte_sind_richtungsabhaengig() {
    let fremd = fremde();
    if fremd.is_empty() {
        eprintln!("übersprungen: {} fehlt", fremd_ordner().display());
        return;
    }
    let mut stoffe = 0usize;
    let mut ohne_werte = 0usize;
    let mut isotrop = 0usize;
    let mut latex_mit_default_werten = 0usize;
    let mut je_profil: BTreeMap<String, (usize, f64, f64)> = BTreeMap::new();

    for p in &fremd {
        let Ok(projekt) = Projekt::lies(p) else { continue };
        for s in &projekt.stoffe {
            stoffe += 1;
            let Some(sp) = spanne(s) else {
                ohne_werte += 1;
                continue;
            };
            if sp == 0.0 {
                isotrop += 1;
                continue;
            }
            // Der eine Widerspruch, den die Dateien selbst tragen: ein Stoff
            // heißt `Latex_0.4mm_Radical_Rubber` und trägt trotzdem CLOs
            // Vorgabewerte. Gezählt, nicht übersehen.
            if s.profil.as_deref() == Some("Latex_0.4mm_Radical_Rubber")
                && (sp - 500.0).abs() < 1.0
            {
                latex_mit_default_werten += 1;
            }
            let profil = s.profil.clone().unwrap_or_else(|| "-".into());
            let e = je_profil.entry(profil).or_insert((0, f64::INFINITY, 0.0));
            e.0 += 1;
            e.1 = e.1.min(sp);
            e.2 = e.2.max(sp);
        }
    }

    // Die Tabelle ist der Beleg: sie steht im Testausgang, damit eine spätere
    // Änderung am Leser als Zahl auffällt und nicht als geänderte Grenze.
    eprintln!("\nProfil | Stoffe | Spanne min .. max");
    for (profil, (n, min, max)) in &je_profil {
        eprintln!("  {profil:<32} {n:4}  {min:9.4} .. {max:9.4} %");
    }

    assert_eq!(stoffe, 207, "Stoffe in allen 47 Projekten zusammen");
    assert_eq!(ohne_werte, 0, "Stoffe ohne vollständige Biegewerte");
    assert_eq!(
        isotrop, 1,
        "genau ein exakt isotroper Stoff (Trim_Hardware); mehr hieße, der Leser \
         verliert Unterschiede"
    );

    // CLOs Vorgabestoff spreizt 1 : 6 zwischen Schuss und Kette - mit 132
    // Nennungen der häufigste benannte Stoff der fremden Projekte. Die Zahl
    // ist gemessen, nicht gewählt: sie steht hier, damit ein Leser, der
    // Einträge verliert, als Zahl auffällt.
    let (n_default, _, max_default) = je_profil.get("Default").expect("Default kommt vor");
    assert_eq!(*n_default, 132, "Default-Einträge über alle Projekte");
    assert!(
        (max_default - 500.0).abs() < 0.01,
        "Default spannt {max_default:.2} %, erwartet 500"
    );

    // Baumwoll-Canvas: 1 : 2,67, ein echter Gewebestoff mit klarer Richtung.
    let (_, min_canvas, max_canvas) = je_profil
        .get("Cotton_Canvas")
        .expect("Cotton_Canvas kommt vor");
    assert!(
        (min_canvas - 166.69).abs() < 0.05 && (max_canvas - 166.69).abs() < 0.05,
        "Cotton_Canvas spannt {min_canvas:.2} .. {max_canvas:.2} %, erwartet 166,69"
    );

    // **Der Name ist keine Zusage.** In `miko.zprj` und `miko_alt.zprj` heißt
    // ein Stoff `Latex_0.4mm_Radical_Rubber` und trägt trotzdem die
    // `Default`-Werte (100/600/150). Der Leser gibt wieder, was in der Datei
    // steht - und die Datei widerspricht sich hier selbst. Deshalb steht die
    // Latex-Erwartung oben am Referenzprojekt und nicht an dieser Verteilung.
    let (n_latex, min_latex, max_latex) = je_profil
        .get("Latex_0.4mm_Radical_Rubber")
        .expect("Latex kommt vor");
    assert_eq!(*n_latex, 53, "Latex-Einträge über alle Projekte");
    assert!(
        (min_latex - 0.1712).abs() < 0.0001,
        "Latex spannt minimal {min_latex:.4} %, erwartet 0,1712"
    );
    assert!(
        (max_latex - 500.32).abs() < 0.5,
        "Latex spannt maximal {max_latex:.2} % - die zwei falsch benannten \
         Stoffe aus miko/miko_alt tragen Default-Werte"
    );
    assert_eq!(
        latex_mit_default_werten, 2,
        "genau zwei Stoffe heißen Latex und tragen Default-Werte (miko, miko_alt)"
    );
}

