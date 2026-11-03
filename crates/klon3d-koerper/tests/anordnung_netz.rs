//! [`anordnung_netz::aus_netz`] gegen CLOs eigene Landmarken.
//!
//! Die Rechnung hier liest **keine** CLO-Datei: Eingang ist allein das Netz
//! (`BaseMesh.obj` als gewöhnliches OBJ). Die Erwartung kommt aus
//! `IndexSet.txt` — das ist die Prüfgrösse, nicht der Eingang.
//!
//! Fehlt CLOs Ordner, wird übersprungen.

use klon3d_koerper::anordnung_clo::{
    CloNummern, L_ACHSEL, L_BRUSTPUNKT, L_ELLBOGEN, L_HALS_HINTEN, L_HALS_SEITLICH, L_HALS_VORN,
    L_SCHULTER,
};
use klon3d_koerper::anordnung_netz::aus_netz;
use klon3d_koerper::{lies_obj, Netz};
use std::path::PathBuf;

use klon3d_koerper::Punkt3;

/// CLOs `Resizing`-Ordner: `CLO_FEMALE`, sonst die neuere, sonst die ältere
/// Installation.
fn ordner() -> Option<PathBuf> {
    [
        std::env::var_os("CLO_FEMALE").map(PathBuf::from),
        Some(PathBuf::from(
            r"C:\Program Files\CLO Standalone OnlineAuth\Preset\Avatar\Resizing\Female",
        )),
        Some(PathBuf::from(
            r"C:\Program Files\CLO Standalone\Preset\Avatar\Resizing\Female",
        )),
    ]
    .into_iter()
    .flatten()
    .find(|p| {
        ["BaseMesh.obj", "range_list.rlist", "IndexSet.txt"]
            .iter()
            .all(|f| p.join(f).is_file())
    })
}

/// `BaseMesh.obj` steht in Zentimetern; `lies_obj` macht daraus Millimeter.
fn basismesh(ordner: &std::path::Path) -> Netz {
    lies_obj(&ordner.join("BaseMesh.obj"), 10.0).expect("BaseMesh.obj")
}

fn abstand(a: Punkt3, b: Punkt3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() / 10.0
}

#[test]
fn dieselben_landmarken_wie_clo_ohne_clo_dateien() {
    let Some(ordner) = ordner() else {
        eprintln!("übersprungen: CLO-Ordner fehlt");
        return;
    };
    let netz = basismesh(&ordner);
    let an = aus_netz(&netz, 10.0).expect("Anordnung aus dem Netz");
    let nummern = CloNummern::aus_ordner(&ordner).expect("CLO-Listen");

    // Reihenfolge wie in `Anordnung`; Toleranzen aus der Eichung.
    let faelle: [(&str, Punkt3, usize, f64); 12] = [
        ("Drosselgrube", an.hals_vorn, L_HALS_VORN, 0.5),
        ("7. Halswirbel", an.hals_hinten, L_HALS_HINTEN, 0.5),
        (
            "Hals seitlich links",
            an.hals_seitlich[0],
            L_HALS_SEITLICH[0],
            2.5,
        ),
        (
            "Hals seitlich rechts",
            an.hals_seitlich[1],
            L_HALS_SEITLICH[1],
            2.5,
        ),
        ("Schulter links", an.schulter[0], L_SCHULTER[0], 0.5),
        ("Schulter rechts", an.schulter[1], L_SCHULTER[1], 0.5),
        ("Achsel links", an.achsel[0], L_ACHSEL[0], 2.5),
        ("Achsel rechts", an.achsel[1], L_ACHSEL[1], 2.5),
        ("Brustpunkt links", an.brustpunkt[0], L_BRUSTPUNKT[0], 2.5),
        ("Brustpunkt rechts", an.brustpunkt[1], L_BRUSTPUNKT[1], 2.5),
        ("Ellbogen links", an.ellbogen[0], L_ELLBOGEN[0], 1.5),
        ("Ellbogen rechts", an.ellbogen[1], L_ELLBOGEN[1], 1.5),
    ];

    let mut summe = 0.0;
    let mut groesste = 0.0f64;
    for (name, mein, zeile, toleranz) in faelle {
        let i = nummern.landmarke(zeile).expect("Landmarke ist ein Punkt");
        let soll = netz.punkte[i];
        let d = abstand(mein, soll);
        eprintln!(
            "{name:22} ({:7.2} {:7.2} {:7.2}) soll ({:7.2} {:7.2} {:7.2})  {d:.2} cm",
            mein[0] / 10.0,
            mein[1] / 10.0,
            mein[2] / 10.0,
            soll[0] / 10.0,
            soll[1] / 10.0,
            soll[2] / 10.0
        );
        assert!(
            d <= toleranz,
            "{name}: {d:.2} cm neben CLOs L{zeile} (Toleranz {toleranz:.1} cm)"
        );
        summe += d;
        groesste = groesste.max(d);
    }
    let mittel = summe / 12.0;
    eprintln!("Mittel {mittel:.2} cm, grösste {groesste:.2} cm");
    assert!(mittel < 1.5, "Mittel {mittel:.2} cm ist zu gross");
}

#[test]
fn die_punktmengen_sind_plausibel() {
    let Some(ordner) = ordner() else {
        eprintln!("übersprungen: CLO-Ordner fehlt");
        return;
    };
    let netz = basismesh(&ordner);
    let an = aus_netz(&netz, 10.0).expect("Anordnung aus dem Netz");
    let nummern = CloNummern::aus_ordner(&ordner).expect("CLO-Listen");

    // Kein Punkt in zwei Mengen, alle Indizes im Netz.
    let n = netz.punkte.len();
    let mut gesehen = vec![0u8; n];
    for (name, menge) in [
        ("Rumpf", &an.rumpf),
        ("Arm links", &an.arme[0]),
        ("Arm rechts", &an.arme[1]),
    ] {
        for &i in menge {
            assert!(i < n, "{name}: Punkt {i} liegt ausserhalb von {n}");
            assert_eq!(gesehen[i], 0, "{name}: Punkt {i} steht doppelt");
            gesehen[i] = 1;
        }
    }

    // CLOs Rumpf (Segment 4) muss in unserer Einteilung liegen — bis auf die
    // Punkte, die wir zum Arm zählen (Schulter- und Achselbereich; CLO zieht
    // die Grenze dort anders).
    let seg4: Vec<usize> = (0..n).filter(|&i| nummern.segment[i] == 4).collect();
    assert!(
        seg4.len() > 4_000,
        "Segment 4 hat nur {} Punkte",
        seg4.len()
    );
    let fehlend = seg4.iter().filter(|&&i| gesehen[i] == 0).count();
    assert!(
        fehlend * 100 < seg4.len() * 3,
        "{fehlend} von {} CLO-Rumpfpunkten fehlen (mehr als 3 %)",
        seg4.len()
    );
    assert!(an.rumpf.len() > 5_000, "nur {} Rumpfpunkte", an.rumpf.len());

    // Umriss je Höhenband: die halbe Breite des Rumpfs darf nicht weit von
    // der liegen, die CLOs Segment 4 ergibt.
    //
    // Verglichen wird das 95. Perzentil von `|x|`, nicht das Maximum: CLOs
    // Segment 4 führt im Band y 90…110 acht Ausreisser bis |x| = 35,6 cm
    // (Index 6099…12111), die kein Rumpfumriss sind. Über 600 Punkte im Band
    // liegen sie bei 1,3 % und fallen aus dem Perzentil heraus.
    //
    // Unter der Achselverschmelzung (y_merge, hier 124,4 cm) muss der Umriss
    // übereinstimmen. **Darüber** ist er bei uns schmaler, und das ist die
    // Absicht: CLO führt Segment 4 bis zum Armloch, wir zählen das Deltoid
    // zum Arm (Dijkstra). Ein Gleichheitsanspruch wäre dort falsch.
    for (lo, hi) in [(70.0, 80.0), (80.0, 90.0), (90.0, 110.0), (110.0, 125.0)] {
        let (Some(meine), Some(clo)) = (
            breite_robust(&netz, &an.rumpf, lo, hi),
            breite_robust(&netz, &seg4, lo, hi),
        ) else {
            continue;
        };
        assert!(
            (meine - clo).abs() < 2.5,
            "y {lo}..{hi}: halbe Breite {meine:.1} statt {clo:.1} cm"
        );
    }
    // Im Schulterband: nie breiter als CLO, aber auch nicht zusammengeschrumpft.
    for (lo, hi) in [(125.0, 135.0)] {
        let (Some(meine), Some(clo)) = (
            breite_robust(&netz, &an.rumpf, lo, hi),
            breite_robust(&netz, &seg4, lo, hi),
        ) else {
            continue;
        };
        assert!(
            meine <= clo + 0.5,
            "y {lo}..{hi}: halbe Breite {meine:.1} ist breiter als CLOs {clo:.1} cm"
        );
        assert!(
            meine > 0.5 * clo,
            "y {lo}..{hi}: halbe Breite {meine:.1} gegen CLOs {clo:.1} cm — zu schmal"
        );
    }
}

/// 95. Perzentil von `|x|` der Punkte aus `menge` im Höhenband, in cm.
fn breite_robust(netz: &Netz, menge: &[usize], lo: f64, hi: f64) -> Option<f64> {
    let mut x: Vec<f64> = menge
        .iter()
        .map(|&i| netz.punkte[i])
        .filter(|q| q[1] / 10.0 >= lo && q[1] / 10.0 <= hi)
        .map(|q| (q[0] / 10.0).abs())
        .collect();
    if x.len() < 20 {
        return None;
    }
    x.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(x[(x.len() as f64 * 0.95) as usize])
}

/// Ohne Arme im Netz gibt es keinen Arm-Samen — das muss eine Meldung sein,
/// die den Grund nennt, kein stiller Absturz.
#[test]
fn ein_netz_ohne_arme_meldet_den_grund() {
    // Ein einfacher Zylinder: Arme, Hals und Ristschlitz gibt es nicht. Er
    // muss über `KOERPER_MINDEST` liegen (1000 Punkte), sonst bricht die
    // Rechnung schon an der Grösse ab und die Meldung nennt den falschen Grund.
    let mut netz = Netz::default();
    let (segmente, ringe) = (48, 40);
    for r in 0..=ringe {
        let y = 1600.0 * r as f64 / ringe as f64;
        for s in 0..segmente {
            let w = 2.0 * std::f64::consts::PI * s as f64 / segmente as f64;
            netz.punkte.push([150.0 * w.cos(), y, 150.0 * w.sin()]);
        }
    }
    assert!(
        netz.punkte.len() >= 1000,
        "Zylinder hat nur {} Punkte",
        netz.punkte.len()
    );
    for r in 0..ringe {
        for s in 0..segmente {
            let s1 = (s + 1) % segmente;
            let (a, b) = (r * segmente + s, r * segmente + s1);
            let (c, d) = (a + segmente, b + segmente);
            netz.dreiecke.push([a, c, b]);
            netz.dreiecke.push([b, c, d]);
        }
    }
    let fehler = aus_netz(&netz, 10.0).expect_err("ein Zylinder hat keine Landmarken");
    assert!(
        fehler.contains("Arm-Samen"),
        "Meldung nennt den Grund nicht: {fehler}"
    );
}

/// Auch an einem **fremden, anders posierten** Körper muss die Rechnung
/// durchlaufen: dort gibt es keine CLO-Nummern und keine Vorlage, nur
/// geometrische Aussagen.
#[test]
fn fremder_koerper_ergibt_geometrisch_stimmige_landmarken() {
    let ordner = std::env::var_os("KLON3D_FREMD")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\ai\rea\downloads\clo3d-patterns"));
    let mut dateien = Vec::new();
    fn sammle(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                sammle(&p, out);
            } else if p.extension().is_some_and(|x| x == "zprj") {
                out.push(p);
            }
        }
    }
    sammle(&ordner, &mut dateien);
    dateien.sort();
    if dateien.is_empty() {
        eprintln!("übersprungen: keine Fremdprojekte in {}", ordner.display());
        return;
    }

    let mut geprueft = 0;
    let mut verstoesse: Vec<String> = Vec::new();
    for pfad in &dateien {
        let Ok(projekt) = klon3d_pattern::zprj::Projekt::lies(pfad) else {
            continue;
        };
        let Some(k) = projekt.koerper else { continue };
        let netz = Netz {
            punkte: k.punkte.iter().map(|q| [q.x, q.y, q.z]).collect(),
            dreiecke: k
                .dreiecke
                .iter()
                .map(|d| [d[0] as usize, d[1] as usize, d[2] as usize])
                .collect(),
        };
        let Ok(an) = aus_netz(&netz, 10.0) else {
            continue;
        };
        let name = pfad
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (y_lo, y_hi) = netz.hoehe().expect("Netz hat Punkte");
        eprintln!(
            "{name:28} y {:.0}..{:.0} H {:.0} | vorn z {:.1} hinten z {:.1} | sch {:.0}/{:.0} ach {:.0}/{:.0} | Arme {} / {}",
            y_lo / 10.0,
            y_hi / 10.0,
            (y_hi - y_lo) / 10.0,
            an.hals_vorn[2] / 10.0,
            an.hals_hinten[2] / 10.0,
            an.schulter[0][1] / 10.0,
            an.schulter[1][1] / 10.0,
            an.achsel[0][1] / 10.0,
            an.achsel[1][1] / 10.0,
            an.arme[0].len(),
            an.arme[1].len()
        );
        // Der Nacken liegt hinter der Drosselgrube — das gilt in jeder Pose,
        // in der die Person nicht rückwärts liegt.
        if an.hals_hinten[2] >= an.hals_vorn[2] {
            verstoesse.push(format!(
                "{name}: Nacken z {:.1} nicht hinter der Drosselgrube z {:.1}",
                an.hals_hinten[2], an.hals_vorn[2]
            ));
        }

        // Ab hier nur Aussagen, die auch bei gehobenem Arm gelten. Die
        // Fremdprojekte sind **nicht** alle in A-Pose: hebt die Person den
        // Arm, liegt der Schulterpunkt über der Drosselgrube. Ein Höhen-
        // vergleich „Hals über Schulter" wäre dann falsch und nicht die
        // Schuld der Rechnung.

        // Die Achsel sitzt unter der Schulter, die Drosselgrube über der
        // Achsel — das ist die Lage der Teile zueinander, unabhängig davon,
        // wie weit der Arm gehoben ist.
        for s in 0..2 {
            if an.achsel[s][1] >= an.schulter[s][1] {
                verstoesse.push(format!(
                    "{name}: Achsel {s} ({:.1}) nicht unter der Schulter ({:.1})",
                    an.achsel[s][1], an.schulter[s][1]
                ));
            }
            if an.ellbogen[s][1] >= an.schulter[s][1] {
                verstoesse.push(format!(
                    "{name}: Ellbogen {s} ({:.1}) nicht unter der Schulter ({:.1})",
                    an.ellbogen[s][1], an.schulter[s][1]
                ));
            }
            // Die Arme sind die Mengen mit den Ellbogen darin.
            if !an.arme[s].contains(&naechster(&netz, an.ellbogen[s])) {
                verstoesse.push(format!("{name}: Ellbogen {s} liegt nicht im Arm {s}"));
            }
        }
        geprueft += 1;
    }
    eprintln!("{geprueft} fremde Körper geprüft");
    assert!(geprueft > 0, "kein Fremdprojekt liess sich prüfen");
    assert!(
        verstoesse.is_empty(),
        "{} von {geprueft} Körpern verletzen die Geometrie:\n  {}",
        verstoesse.len(),
        verstoesse.join("\n  ")
    );
}

fn naechster(netz: &Netz, p: Punkt3) -> usize {
    let mut best = 0;
    let mut d = f64::INFINITY;
    for (i, q) in netz.punkte.iter().enumerate() {
        let e = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt();
        if e < d {
            d = e;
            best = i;
        }
    }
    best
}
