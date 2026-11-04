//! Abnahme von Stufe 2 mit dem Maßband.
//!
//! Formt den Körper der Anwenderin aus ihrer `.smis` wie das Beispiel
//! `forme` und misst ihn danach so, wie `spez/massdefinitionen.md`
//! die Abnahme definiert (PLAN, Stufe 2: ±1 cm):
//!
//! * Brust: waagerechter Schnitt an CLOs Brustpunkten, konvexe Hülle;
//! * Taille: kleinster Hüllenumfang zwischen Hüft- und Unterbrustpunkt;
//! * Hüfte: grösster Hüllenumfang zwischen Schritthöhe + 0,5 cm und
//!   Taillenpunkt (Schritt 0,25 cm, Ebene durch `(0, y, 0)`).
//!
//! Zum Vergleich stehen CLOs eigene Messwerte an den gewählten Kandidaten
//! daneben. Vier Läufe:
//!
//! 1. angepasst auf die Schnittkontur (bisheriger Weg);
//! 2. angepasst auf das Maßband an denselben Linien — **das wird
//!    abgenommen**;
//! 3. dasselbe ohne HighHip-Linie (zeigt, dass das Feld gebraucht wird);
//! 4. HighHip nach `massdefinitionen.md` an CLOs Lage (86,23 cm, 12,39 cm
//!    unter der Taille) statt 83 cm am Beckenkamm.
//!
//! Stand 19.10.2026 (Befund 30): Lauf 2 verfehlt Taille und Hüfte; Lauf 4
//! hält Brust und Taille, die Hüfte nur nach CLOs Messung (+0,90), nicht nach
//! der Abtastung (+1,12 knapp über dem Schritt). Welche Zahl entscheidet,
//! ist offene Frage 2/3 in `spez/fragen_offen.md`.
//!
//! Läuft nur auf Wunsch, weil er CLOs Dateien und die Maßdatei braucht:
//!
//! ```text
//! cargo test --release -p klon3d-koerper --test abnahme_massband -- --ignored --nocapture
//! ```
//!
//! `CLO_FEMALE` (Ordner der Resizing-Daten) und `KLON3D_SMIS` (Maßdatei)
//! setzen andere Pfade.

use klon3d_koerper::anpassen::{anpassen, Ergebnis};
use klon3d_koerper::formung::{namen_vektor, referenz_vektor, Formung};
use klon3d_koerper::massband::{abnahme, rumpfmasse, zentriert_cm, Messdaten, SCHRITT};
use klon3d_koerper::person::zielperson;
use klon3d_koerper::{lies_obj, Netz};
use klon3d_pattern::lies_smis;
use std::path::PathBuf;
use std::sync::Arc;

fn pfad(var: &str, standard: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(standard))
}

/// Abnahmegrenze, cm.
const GRENZE: f64 = 1.0;

struct Bericht {
    brust: f64,
    taille: f64,
    huefte: f64,
}

fn messe(name: &str, netz: &Netz, d: &Messdaten, ziel: (f64, f64, f64)) -> Bericht {
    let v = zentriert_cm(netz, 10.0);
    let r = rumpfmasse(&v, d).expect("Rumpfmaße");
    let a = abnahme(&v, d, &r).expect("Abnahme");
    let taille = a.taille.expect("Taille abgetastet");
    let huefte = a.huefte.expect("Hüfte abgetastet");
    let k = &r.kandidaten;
    println!("\n{name}");
    println!("  Maß         Ziel   Maßband-Abnahme             CLO am Kandidaten");
    println!(
        "  Brust     {:6.2}   {:6.2} ({:+.2})              {:6.2} ({:+.2})  Paar {}/{} y {:.2}",
        ziel.0,
        a.brust,
        a.brust - ziel.0,
        r.brust.laenge,
        r.brust.laenge - ziel.0,
        k.brustpaar.0,
        k.brustpaar.1,
        r.brust.p0[1]
    );
    println!(
        "  Taille    {:6.2}   {:6.2} ({:+.2}) bei y {:6.2}  {:6.2} ({:+.2})  Punkt {} y {:.2}",
        ziel.1,
        taille.umfang,
        taille.umfang - ziel.1,
        taille.y,
        r.taille.laenge,
        r.taille.laenge - ziel.1,
        k.taille,
        r.taille.p0[1]
    );
    println!(
        "  Hüfte     {:6.2}   {:6.2} ({:+.2}) bei y {:6.2}  {:6.2} ({:+.2})  Punkt {} y {:.2}",
        ziel.2,
        huefte.umfang,
        huefte.umfang - ziel.2,
        huefte.y,
        r.huefte.laenge,
        r.huefte.laenge - ziel.2,
        k.huefte_unten,
        r.huefte.p0[1]
    );
    println!(
        "  Unterbrust (CLO) {:6.2} an Punkt {}, HighHip (CLO) {:6.2} an Punkt {}",
        r.unterbrust.laenge, k.unterbrust, r.highhip.laenge, k.huefte_oben
    );
    let schritt = d.landmarke(SCHRITT).expect("Schrittpunkt");
    println!(
        "  Bereiche: Taille {:.2}–{:.2}, Hüfte {:.2}–{:.2} (Schritt {:.2})",
        v[k.huefte_unten][1],
        v[k.unterbrust][1],
        v[schritt][1] + 0.5,
        v[k.taille][1],
        v[schritt][1]
    );
    Bericht {
        brust: a.brust,
        taille: taille.umfang,
        huefte: huefte.umfang,
    }
}

fn zeige(name: &str, e: &Ergebnis, namen: &[String]) {
    println!(
        "{name}: {} Runden{}, Restfehler {}",
        e.runden,
        if e.divergiert { " (divergiert)" } else { "" },
        namen
            .iter()
            .zip(&e.fehler)
            .map(|(n, f)| format!("{n} {f:+.3}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[test]
#[ignore = "braucht CLOs Resizing-Daten und die Maßdatei"]
fn abnahme_mit_massband() {
    let ordner = pfad(
        "CLO_FEMALE",
        r"C:\Program Files\CLO Standalone\Preset\Avatar\Resizing\Female",
    );
    let smis_pfad = pfad(
        "KLON3D_SMIS",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../klon3d-pattern/tests/muster/measurements.smis"
        ),
    );
    let grund = lies_obj(&ordner.join("BaseMesh.obj"), 10.0).expect("BaseMesh.obj");
    let formung = Formung::laden(
        &ordner.join("ConvertingMat_DETAIL_Advanced_TotalHeight.bd"),
        grund.punkte.len(),
    )
    .expect("Formungsdatei");
    let txt = ordner.join("_DetailFeature_Advanced_Bust_TotalHeight.txt");
    let c_ref = referenz_vektor(&txt).expect("Referenzvektor");
    let namen = namen_vektor(&txt).expect("Maßnamen");
    let d = Arc::new(Messdaten::laden(&ordner).expect("Messdaten"));
    let smis = lies_smis(&smis_pfad).expect("Maßdatei");
    let p = zielperson(&grund, &namen, &c_ref, &|n| smis.wert(n)).expect("Zielperson");

    let ziel = (
        smis.wert("bust_circ").unwrap(),
        smis.wert("waist_circ").unwrap(),
        smis.wert("hip_circ").unwrap(),
    );
    let linien_namen = p.linien.namen();
    println!(
        "Ziel: Höhe {:.2}, {}",
        p.ziel.hoehe,
        linien_namen
            .iter()
            .zip(&p.ziel.masse)
            .zip(p.linien.hoehen())
            .map(|((n, m), h)| format!("{n} {m:.2} bei {h:.2}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Bisher: angepasst auf die Schnittkontur.
    let kontur = anpassen(&grund, &formung, &p.c_start, &p.linien, &p.ziel, &p.aktiv)
        .expect("Anpassung Kontur");
    zeige("Kontur", &kontur, &linien_namen);
    let netz = formung.anwenden(&grund, &kontur.c);
    let (lo, _) = netz.hoehe().unwrap();
    println!("tiefster Punkt des geformten Netzes: y = {:.3} cm", lo / 10.0);
    messe("Angepasst auf die Schnittkontur", &netz, &d, ziel);

    // Neu: angepasst auf das Maßband an denselben Linien.
    let linien = p.linien.clone().mit_massband(d.clone());
    let band = anpassen(&grund, &formung, &p.c_start, &linien, &p.ziel, &p.aktiv)
        .expect("Anpassung Maßband");
    zeige("Maßband", &band, &linien_namen);
    let netz = formung.anwenden(&grund, &band.c);
    let b = messe("Angepasst auf das Maßband", &netz, &d, ziel);

    // Zum Vergleich: ohne die HighHip-Linie. CLOs Feld CIRCUMFERENCE_HighHip
    // wirkt an CLOs Lage (etwa 12 cm unter der Taille), die Linie der
    // Maßdatei liegt 5 cm darunter (Beckenkamm, `massdefinitionen.md`).
    let ohne_hh = zielperson(&grund, &namen, &c_ref, &|n| {
        if n == "highhip_circ" {
            None
        } else {
            smis.wert(n)
        }
    })
    .expect("Zielperson ohne HighHip");
    let linien = ohne_hh.linien.clone().mit_massband(d.clone());
    let vier = anpassen(&grund, &formung, &ohne_hh.c_start, &linien, &ohne_hh.ziel, &ohne_hh.aktiv)
        .expect("Anpassung ohne HighHip");
    zeige("Maßband ohne HighHip", &vier, &ohne_hh.linien.namen());
    messe(
        "Angepasst auf das Maßband, ohne HighHip-Linie",
        &formung.anwenden(&grund, &vier.c),
        &d,
        ziel,
    );

    // HighHip nach `massdefinitionen.md` (Platz 7): an CLOs Lage,
    // d = 12,39376 cm unter der Taille, mit ihrem Umfang dort, linear
    // zwischen 83 (5 cm) und 90 (21 cm) unter der Taille: 86,23477 cm.
    const D_WAIST_HIGHHIP: f64 = 12.39376;
    const HIGHHIP_CLO: f64 = 86.23477;
    let taillenhoehe = smis.wert("height_waist_side").expect("height_waist_side");
    let clo_hh = zielperson(&grund, &namen, &c_ref, &|n| match n {
        "highhip_circ" => Some(HIGHHIP_CLO),
        "height_highhip" => Some(taillenhoehe - D_WAIST_HIGHHIP),
        _ => smis.wert(n),
    })
    .expect("Zielperson mit HighHip an CLOs Lage");
    let linien = clo_hh.linien.clone().mit_massband(d.clone());
    let fuenf = anpassen(&grund, &formung, &clo_hh.c_start, &linien, &clo_hh.ziel, &clo_hh.aktiv)
        .expect("Anpassung HighHip an CLOs Lage");
    zeige("Maßband, HighHip an CLOs Lage", &fuenf, &clo_hh.linien.namen());
    messe(
        "Angepasst auf das Maßband, HighHip 86,23 an CLOs Lage",
        &formung.anwenden(&grund, &fuenf.c),
        &d,
        ziel,
    );

    let mut fehler = Vec::new();
    for (name, ist, soll) in [
        ("Brust", b.brust, ziel.0),
        ("Taille", b.taille, ziel.1),
        ("Hüfte", b.huefte, ziel.2),
    ] {
        if (ist - soll).abs() > GRENZE {
            fehler.push(format!("{name}: {ist:.2} statt {soll:.2} ± {GRENZE}"));
        }
    }
    assert!(fehler.is_empty(), "Abnahme verfehlt:\n{}", fehler.join("\n"));
}
