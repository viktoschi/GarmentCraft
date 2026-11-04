//! Abnahme von Issue #65: Körper aus MakeHuman, geformt auf ihre Formziele.
//!
//! Der Test braucht **keine** CLO-Daten: der Grundkörper ist
//! `assets/3dobjs/makehuman/base.obj` mitgeliefert, die Formziele stehen in
//! `klon3d_pattern/tests/muster/hexen_2026_10_27/measurements.smis`.
//!
//! Geprüft wird auf der **Hülle** (dem Maßband), nicht auf der Hautkontur, und
//! je Linie gegen sechs Werte mit den Toleranzen aus dem Issue:
//!
//! | Größe | Toleranz |
//! |---|---|
//! | Umfang | 0,2 cm |
//! | Brustpunktabstand | 0,5 cm |
//! | Bogen vorn / hinten | 1,0 cm |
//! | Breite / Tiefe | 0,5 cm |
//!
//! Die gemessenen Höhen sind die der Kette (Brust 113, Taille 98, Hochhüfte 93,
//! Hüfte 77), nicht `height_bustpoint`.

use klon3d_koerper::anpassen::{
    formen, formlinien_aus_smis, formziel_ebenen, formziele_aus_smis, FormzielEbene, Messort,
};
use klon3d_koerper::bogenmasse::Querschnitt;
use klon3d_koerper::koerper_quelle::{makehuman, ordner_ab};
use klon3d_koerper::massband::{huelle, WAAGERECHT};
use klon3d_koerper::{rumpf, Netz, Punkt3};
use std::path::{Path, PathBuf};

fn smis() -> Option<PathBuf> {
    let p = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../klon3d-pattern/tests/muster/hexen_2026_10_27/measurements.smis"
    ));
    p.is_file().then_some(p)
}

fn basis() -> Option<PathBuf> {
    let start = Path::new(env!("CARGO_MANIFEST_DIR"));
    ordner_ab(start)
}

/// Umfang einer Linie, gemessen auf der Hülle, cm.
fn umfang_bei(netz: &Netz, hoehe: f64) -> Option<f64> {
    let q = rumpf(netz, hoehe * 10.0)?;
    let ring: Vec<Punkt3> = q
        .ring
        .iter()
        .map(|p| [p.0 / 10.0, hoehe, p.1 / 10.0])
        .collect();
    let (_, u) = huelle(&ring, WAAGERECHT);
    (u > 0.0).then_some(u)
}

/// Zylinder um die Achse `x = z = 0`, als geschlossenes Netz.
///
/// `Netz::zylinder` ist `#[cfg(test)]` und damit von einem Integrationstest
/// aus nicht erreichbar; hier steht derselbe Aufbau noch einmal.
fn zylinder(radius: f64, hoehe: f64, segmente: usize, ringe: usize) -> Netz {
    let mut n = Netz::default();
    for r in 0..=ringe {
        let y = hoehe * r as f64 / ringe as f64;
        for s in 0..segmente {
            let w = 2.0 * std::f64::consts::PI * s as f64 / segmente as f64;
            n.punkte.push([radius * w.cos(), y, radius * w.sin()]);
        }
    }
    for r in 0..ringe {
        for s in 0..segmente {
            let s1 = (s + 1) % segmente;
            let a = r * segmente + s;
            let b = r * segmente + s1;
            let c = (r + 1) * segmente + s;
            let d = (r + 1) * segmente + s1;
            n.dreiecke.push([a, c, b]);
            n.dreiecke.push([b, c, d]);
        }
    }
    n
}

#[test]
fn makehuman_koerper_traegt_ihre_formziele() {
    let (Some(ordner), Some(pfad)) = (basis(), smis()) else {
        eprintln!("übersprungen: MakeHuman-Assets oder Maßdatei fehlt");
        return;
    };
    let Ok(smis) = klon3d_pattern::lies_smis(&pfad) else {
        eprintln!("übersprungen: Maßdatei nicht lesbar");
        return;
    };
    let wert = |n: &str| smis.wert(n);

    // 1. Grundkörper, auf ihre Körpergröße skaliert.
    let hoehe_cm = wert("height").unwrap_or(160.0);
    let mut netz = makehuman(&ordner, hoehe_cm * 10.0).expect("MakeHuman-Körper");
    assert!(
        netz.punkte.len() > 10_000,
        "nur {} Punkte - Gruppe body fehlt?",
        netz.punkte.len()
    );

    // 2. Formziele aus ihrer Maßdatei, Lücke halbiert wie im Bericht.
    let formziele = formziele_aus_smis(&wert);
    println!("Formziele: {} Ziele", formziele.ziele.len());
    for m in &formziele.meldungen {
        println!("  {m}");
    }

    // 3. Die Zielquerschnitte je Linie bauen - derselbe Bau wie im Weg
    //    `Weg::Makehuman`, damit Abnahme und Weg nicht auseinanderlaufen.
    let hoehen = formlinien_aus_smis(&wert);
    let (ebenen, meldungen) = formziel_ebenen(&formziele, &hoehen, &wert);
    for m in &meldungen {
        println!("Formlinie: {m}");
    }
    assert!(
        ebenen.len() >= 3,
        "nur {} Formlinien - Maßdatei unerwartet",
        ebenen.len()
    );

    let vorher: Vec<f64> = ebenen
        .iter()
        .map(|e| umfang_bei(&netz, e.hoehe).unwrap_or(f64::NAN))
        .collect();

    // 4. Formen.
    let (linien, meldungen) = formen(&mut netz, &ebenen);
    for m in &meldungen {
        println!("Meldung: {m}");
    }

    let mut verletzt = Vec::new();
    for (i, l) in linien.iter().enumerate() {
        println!(
            "\n{} bei {:.1} cm (vorher Umfang {:.2}):",
            l.ort.name(),
            l.hoehe,
            vorher[i]
        );
        for (art, f) in l.fehler() {
            let ist = l
                .ist()
                .iter()
                .find(|(a, _)| *a == art)
                .and_then(|(_, w)| *w);
            let soll = l
                .ziel
                .soll()
                .iter()
                .find(|(a, _)| *a == art)
                .map(|(_, w)| *w);
            match (ist, soll, f) {
                (Some(ist), Some(soll), Some(f)) => {
                    let grenze = art.toleranz();
                    let ok = f.abs() <= grenze + 1.0e-9;
                    println!(
                        "  {:<18} soll {soll:8.2}  ist {ist:8.2}  Δ {f:+6.2}  Grenze {grenze:.1}  {}",
                        art.name(),
                        if ok { "ok" } else { "ZU GROSS" }
                    );
                    if !ok {
                        verletzt.push(format!(
                            "{} {}: Δ {f:+.2} cm gegen Grenze {grenze:.1}",
                            l.ort.name(),
                            art.name()
                        ));
                    }
                }
                _ => {
                    println!("  {:<18} nicht messbar", art.name());
                    verletzt.push(format!("{} {}: nicht messbar", l.ort.name(), art.name()));
                }
            }
        }
    }

    assert!(
        verletzt.is_empty(),
        "Abnahme verletzt an {} Stellen:\n{}",
        verletzt.len(),
        verletzt.join("\n")
    );
}

/// Die Formung muss auch dort greifen, wo kein MakeHuman-Körper im Weg ist:
/// an einem Zylinder, dessen Querschnitt bekannt ist.
#[test]
fn die_form_trifft_den_zielumriss_am_zylinder() {
    let mut zylinder = zylinder(120.0, 4000.0, 360, 40);
    let vorher = umfang_bei(&zylinder, 200.0).expect("Umfang vorher");
    // 2 π · 12 = 75,40 cm; Ziel 69,00.
    assert!((vorher - 75.40).abs() < 0.05, "Zylinder vorher {vorher:.3}");

    let querschnitt = Querschnitt {
        breite: 24.0,
        tiefe: 18.0,
        brustpunkt: 0.0,
        vorn: 36.0,
        hinten: 33.0,
    };
    let ebene = FormzielEbene {
        ort: Messort::Taille,
        hoehe: 200.0,
        umfang: 69.0,
        querschnitt,
    };
    let (linien, meldungen) = formen(&mut zylinder, &[ebene]);
    assert_eq!(linien.len(), 1, "keine Linie: {meldungen:?}");

    let l = &linien[0];
    println!("Zylinder vorher {vorher:.3}, nachher {:?}", l.ist());
    for (art, f) in l.fehler() {
        let Some(f) = f else {
            panic!("{} nicht messbar", art.name());
        };
        assert!(
            f.abs() <= art.toleranz() + 1.0e-9,
            "{}: Δ {f:+.3} cm gegen Grenze {:.1}",
            art.name(),
            art.toleranz()
        );
    }
}
