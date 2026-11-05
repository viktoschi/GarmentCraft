//! Stufe 7, Bewegung: der Körper wird **nachgeführt**, nicht neu gebaut
//! (Befund 96, Issue #96).
//!
//! `Koerpernetz::aus_clo_mm` kostet je Bild 9–10 ms, und der größte Posten
//! darin ist die Kantentabelle — reine Topologie, die eine Verformung nicht
//! anfasst. `Koerpernetz::nachfuehren` rechnet deshalb nur, was von den Punkten
//! abhängt. Drei Sätze prüfen das, jeder gegen den Neuaufbau als Wahrheit:
//!
//! 1. **Gleiche Punkte, gleiches Ergebnis:** auf denselben Punkten liefert
//!    `neu` + `nachfuehren` Feld für Feld dieselben Bits wie `neu` allein,
//!    **einschließlich** der Zellbelegung beider Gitter. Schrumpft die
//!    Verformung (Zylinder mit kleinerem Radius), bleibt der Kasten des alten
//!    Gitters gültig und das Raster damit identisch.
//! 2. **Echte Pose, gleiche Werte:** der MakeHuman-Körper aus der Binde-Pose in
//!    die Pose von #94 (`hebe_arme(90°)`) nachgeführt, gegen `aus_clo_mm` auf
//!    genau diesen Punkten: Flächennormalen, Kantennormalen, Punktnormalen,
//!    Hüllkugeln, Volumen — und die Ergebnisse der Abstandsfragen
//!    (`naechster`, `naechster_alle`, `abstand_ohne_grenze`, `durchstoss`) an
//!    Proben auf und in der Haut, Bit für Bit.
//! 3. **Der Gewinn ist mindestens 3×**, gemessen im selben Lauf (je die beste
//!    von drei Runden), nicht behauptet.
//!
//! Satz 2 und 3 brauchen die MakeHuman-Daten; fehlen sie, sagen sie das und
//! hören auf. Nur mit `cargo test --release`.

use std::path::Path;
use std::time::Instant;

use klon3d_koerper::koerper_quelle::ordner_ab;
use klon3d_koerper::skelett;
use klon3d_sim::netzkoerper::{clo_nach_szene, zylinder, Gitter, Koerpernetz, Naechster, SUCHTIEFE};

/// Bitvergleich zweier Punktfelder; `Some(text)` beim ersten Unterschied.
fn feld_weg(a: &[[f64; 3]], b: &[[f64; 3]]) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("Länge {} gegen {}", a.len(), b.len()));
    }
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if (0..3).any(|k| x[k].to_bits() != y[k].to_bits()) {
            return Some(format!("[{i}] {x:?} gegen {y:?}"));
        }
    }
    None
}

/// Bitvergleich zweier Hüllkugelfelder.
fn kugel_weg(a: &[([f64; 3], f64)], b: &[([f64; 3], f64)]) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("Länge {} gegen {}", a.len(), b.len()));
    }
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x.1.to_bits() != y.1.to_bits() || (0..3).any(|k| x.0[k].to_bits() != y.0[k].to_bits()) {
            return Some(format!("[{i}] {x:?} gegen {y:?}"));
        }
    }
    None
}

/// Bitvergleich zweier Gitter (Raster und Zellbelegung).
fn gitter_weg(a: &Gitter, b: &Gitter) -> Option<String> {
    if (0..3).any(|k| a.ursprung[k].to_bits() != b.ursprung[k].to_bits()) {
        return Some(format!("Ursprung {:?} gegen {:?}", a.ursprung, b.ursprung));
    }
    if a.zelle.to_bits() != b.zelle.to_bits() || a.zahl != b.zahl {
        return Some(format!("Zelle {} × {:?} gegen {} × {:?}", a.zelle, a.zahl, b.zelle, b.zahl));
    }
    if a.anfang != b.anfang {
        let i = a.anfang.iter().zip(&b.anfang).position(|(x, y)| x != y).unwrap_or(0);
        return Some(format!("anfang[{i}] {} gegen {}", a.anfang[i], b.anfang[i]));
    }
    if a.eintraege != b.eintraege {
        let i = a.eintraege.iter().zip(&b.eintraege).position(|(x, y)| x != y).unwrap_or(0);
        return Some(format!("eintraege[{i}] {} gegen {}", a.eintraege[i], b.eintraege[i]));
    }
    None
}

/// Alles, was von den Punkten abhängt, Bit für Bit — optional mit Gittern.
fn werte_weg(a: &Koerpernetz, b: &Koerpernetz, mit_gitter: bool) -> Option<String> {
    if let Some(w) = feld_weg(&a.flaechennormale, &b.flaechennormale) {
        return Some(format!("Flächennormale {w}"));
    }
    if let Some(w) = feld_weg(&a.punktnormale, &b.punktnormale) {
        return Some(format!("Punktnormale {w}"));
    }
    if let Some(w) = kugel_weg(&a.kugel, &b.kugel) {
        return Some(format!("Hüllkugel {w}"));
    }
    // Die Kantennormalen sind keine Vorberechnung mehr, sondern eine Abfrage;
    // sie muss über *alle* Dreieckskanten dieselbe sein.
    for t in 0..a.dreiecke.len() {
        for k in 0..3 {
            let (x, y) = (a.kantennormale(t, k), b.kantennormale(t, k));
            if (0..3).any(|i| x[i].to_bits() != y[i].to_bits()) {
                return Some(format!("Kantennormale t{t} k{k}: {x:?} gegen {y:?}"));
            }
        }
    }
    if a.volumen.to_bits() != b.volumen.to_bits() {
        return Some(format!("Volumen {} gegen {}", a.volumen, b.volumen));
    }
    if a.randkanten != b.randkanten || a.nachbar != b.nachbar || a.dreiecke != b.dreiecke {
        return Some("Topologie (Randkanten, Nachbarschaft, Dreiecke) weicht ab".into());
    }
    if let Some(w) = feld_weg(&a.punkte, &b.punkte) {
        return Some(format!("Punkte {w}"));
    }
    if mit_gitter {
        if let Some(w) = gitter_weg(&a.gitter, &b.gitter) {
            return Some(format!("Dreiecksgitter {w}"));
        }
        if let Some(w) = gitter_weg(&a.punktgitter, &b.punktgitter) {
            return Some(format!("Punktgitter {w}"));
        }
    }
    None
}

fn naechster_gleich(p: &Naechster, q: &Naechster) -> bool {
    p.abstand.to_bits() == q.abstand.to_bits()
        && p.dreieck == q.dreieck
        && p.region == q.region
        && (0..3).all(|k| p.punkt[k].to_bits() == q.punkt[k].to_bits())
        && (0..3).all(|k| p.normale[k].to_bits() == q.normale[k].to_bits())
}

/// Wie [`naechster_gleich`], aber zwei **gleich nahe** Dreiecke dürfen
/// verschiedenen Wege melden. Das passiert bei einer schrumpfenden Verformung:
/// das Raster bleibt der weitere Kasten der Ausgangslage (so verlangt), also
/// läuft die Zellsuche in anderer Reihenfolge. Liegt der Probepunkt genau
/// zwischen zwei Sehnen eines Ringes, sind beide Abstände bis aufs Bit gleich,
/// und dann ist **jede** der Antworten richtig — ein Gleichstand, den die
/// Reihenfolge entscheidet. Verlangt wird deshalb: derselbe Abstand bis aufs
/// Bit, und jeder gemeldete Punkt ist der wahre nächste Punkt auf seinem
/// eigenen Dreieck. Beim **selben** Trefferdreieck gilt weiter Bitgleichheit.
fn naechster_gleich_bis_gleichstand(
    x: [f64; 3],
    p: &Naechster,
    q: &Naechster,
    a: &Koerpernetz,
    b: &Koerpernetz,
) -> bool {
    if p.dreieck == q.dreieck {
        return naechster_gleich(p, q);
    }
    if p.abstand.to_bits() != q.abstand.to_bits() {
        return false;
    }
    let stimmt = |n: &Naechster, netz: &Koerpernetz| {
        let [u, v, w] = netz.dreiecke[n.dreieck as usize].map(|i| netz.punkte[i as usize]);
        let (soll, _) = klon3d_sim::netzkoerper::naechster_auf_dreieck(x, u, v, w);
        (0..3).all(|k| (soll[k] - n.punkt[k]).abs() <= 1e-12)
    };
    stimmt(p, a) && stimmt(q, b)
}

/// Die Abstandsfragen an Proben auf und in der Haut: dieselben Antworten.
///
/// Die Proben liegen auf denselben Punkten wie bei beiden Netzen — 5 mm über
/// und 3 mm unter der Haut, also innerhalb der Suchtiefe. `gleichstand`
/// erlaubt zwei gleich nah gelegenen Dreiecken verschiedene Wege (siehe
/// [`naechster_gleich_bis_gleichstand`]) und zählt, wie oft das vorkommt.
fn abfragen_weg(a: &Koerpernetz, b: &Koerpernetz, gleichstand: bool) -> Option<String> {
    // Rund 200 Proben, unabhängig von der Netzgröße.
    let schritt = (a.punkte.len() / 200).max(1);
    let mut proben = 0;
    let mut gleichstand_faelle = 0;
    for i in (0..a.punkte.len()).step_by(schritt) {
        let n = a.punktnormale[i];
        for (s, wo) in [(0.005, "außen"), (-0.003, "innen")] {
            let x: [f64; 3] = [0, 1, 2].map(|k| a.punkte[i][k] + s * n[k]);
            let gleich = |p: &Naechster, q: &Naechster| {
                naechster_gleich(p, q) || (gleichstand && naechster_gleich_bis_gleichstand(x, p, q, a, b))
            };
            let (pa, pb) = (a.naechster(x, SUCHTIEFE), b.naechster(x, SUCHTIEFE));
            match (pa, pb) {
                (Some(p), Some(q)) if gleich(&p, &q) => {
                    if p.dreieck != q.dreieck {
                        gleichstand_faelle += 1;
                    }
                }
                (None, None) => {}
                (p, q) => {
                    return Some(format!("naechster an Punkt {i} ({wo}) {x:?}: {p:?} gegen {q:?}"));
                }
            }
            let (pa, pb) = (a.naechster_alle(x), b.naechster_alle(x));
            if !gleich(&pa, &pb) {
                return Some(format!("naechster_alle an Punkt {i} ({wo}) {x:?}: {pa:?} gegen {pb:?}"));
            }
            let (pa, pb) = (a.abstand_ohne_grenze(x), b.abstand_ohne_grenze(x));
            if !gleich(&pa, &pb) {
                return Some(format!("abstand_ohne_grenze an Punkt {i} ({wo}) {x:?}: {pa:?} gegen {pb:?}"));
            }
            proben += 1;
        }
    }
    if proben < 20 {
        return Some(format!("nur {proben} Proben"));
    }
    if gleichstand_faelle > 0 {
        eprintln!("  {gleichstand_faelle} von {proben} Proben: gleich nahe Nachbardreiecke, Abstand Bit für Bit gleich");
    }
    // Durchstöße: von 30 mm außen nach 10 mm innen, quer durch die Haut.
    for i in (0..a.punkte.len()).step_by((a.punkte.len() / 20).max(1)) {
        let n = a.punktnormale[i];
        let von: [f64; 3] = [0, 1, 2].map(|k| a.punkte[i][k] + 0.03 * n[k]);
        let nach: [f64; 3] = [0, 1, 2].map(|k| a.punkte[i][k] - 0.01 * n[k]);
        match (a.durchstoss(von, nach, [0.0; 3]), b.durchstoss(von, nach, [0.0; 3])) {
            (Some((pa, na, va)), Some((pb, nb, vb))) => {
                if !(0..3).all(|k| pa[k].to_bits() == pb[k].to_bits() && na[k].to_bits() == nb[k].to_bits()) {
                    return Some(format!("durchstoss an Punkt {i}: {pa:?} / {na:?} gegen {pb:?} / {nb:?}"));
                }
                if !(0..3).all(|k| va[k].to_bits() == vb[k].to_bits()) {
                    return Some(format!("durchstoss-Geschwindigkeit an Punkt {i}: {va:?} gegen {vb:?}"));
                }
            }
            (None, None) => {}
            (p, q) => return Some(format!("durchstoss an Punkt {i} {von:?} → {nach:?}: {p:?} gegen {q:?}")),
        }
    }
    None
}

/// Satz 1a: unveränderte Punkte — **alles** ist Bit für Bit dasselbe,
/// einschließlich Raster und Zellbelegung beider Gitter. Der kleinste Fall,
/// der die Zellzuordnung (`klammer`) gegen den Neuaufbau prüft.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn unveraenderte_punkte_geben_dasselbe_gitter() {
    let (p, d) = zylinder(0.15, -0.4, 0.4, 48, 24);
    let mut nach = Koerpernetz::neu(p.clone(), d.clone(), SUCHTIEFE).expect("Zylindernetz");
    nach.nachfuehren(&p).expect("nachfuehren");
    let neu = Koerpernetz::neu(p, d, SUCHTIEFE).expect("Zylindernetz");

    if let Some(w) = werte_weg(&nach, &neu, true) {
        panic!("nachfuehren auf unveränderten Punkten weicht ab: {w}");
    }
    if let Some(w) = abfragen_weg(&nach, &neu, false) {
        panic!("nachfuehren auf unveränderten Punkten: {w}");
    }
}

/// Satz 1b: schrumpfende Verformung — die Werte und Antworten sind Bit für
/// Bit dieselben, das Raster ist aber **nicht** kleiner geworden (so verlangt:
/// sonst hinge die Zellenzahl an der Pose). Der nachgeführte Kasten bleibt der
/// der Binde-Pose, der Neuaufbau nimmt den engeren.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn geschrumpfter_koerper_antwortet_wie_neuaufbau() {
    let (p, d) = zylinder(0.15, -0.4, 0.4, 48, 24);
    let frisch = Koerpernetz::neu(p.clone(), d.clone(), SUCHTIEFE).expect("Zylindernetz");

    // Radius um 2 % kleiner, Höhe gleich: der Hüllquader schrumpft.
    let enger: Vec<[f64; 3]> = p.iter().map(|q| [q[0] * 0.98, q[1] * 0.98, q[2]]).collect();
    let mut nach = Koerpernetz::neu(p, d, SUCHTIEFE).expect("Zylindernetz");
    nach.nachfuehren(&enger).expect("nachfuehren");
    let neu = Koerpernetz::neu(enger, nach.dreiecke.clone(), SUCHTIEFE).expect("Zylindernetz");

    if let Some(w) = werte_weg(&nach, &neu, false) {
        panic!("nachfuehren auf geschrumpftem Zylinder weicht ab: {w}");
    }
    if let Some(w) = abfragen_weg(&nach, &neu, true) {
        panic!("nachfuehren auf geschrumpftem Zylinder: {w}");
    }
    for a in 0..3 {
        assert!(
            nach.gitter.zahl[a] >= neu.gitter.zahl[a] && nach.punktgitter.zahl[a] >= neu.punktgitter.zahl[a],
            "das nachgeführte Raster ist in Achse {a} kleiner geworden: {:?} gegen {:?}",
            nach.gitter.zahl,
            neu.gitter.zahl
        );
    }
    assert!(
        nach.gitter.zahl != neu.gitter.zahl || nach.gitter.ursprung != neu.gitter.ursprung,
        "der geschrumpfte Körper hat den alten Kasten nicht behalten"
    );
    // Und gegen das unverformte Netz ist wirklich etwas passiert.
    assert!(feld_weg(&frisch.punkte, &nach.punkte).is_some(), "der Zylinder ist gar nicht geschrumpft");
}

/// MakeHuman-Körper und Skelett; `None`, wenn die Assets fehlen.
fn makehuman() -> Option<(klon3d_koerper::Netz, skelett::Skelett)> {
    let ordner = ordner_ab(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    match skelett::baue(&ordner, 1600.0) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("übersprungen: {e}");
            None
        }
    }
}

/// Satz 2: echte Pose — nachführen und neu bauen antworten gleich.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn nachfuehren_auf_gehobenem_arm_wie_neuaufbau() {
    let Some((netz, sk)) = makehuman() else {
        return;
    };
    let mut gedreht = sk.clone();
    gedreht.hebe_arme(90.0);
    let posed = gedreht.verforme(&netz.punkte).expect("Verformung möglich");
    let pose_szene: Vec<[f64; 3]> = posed.iter().map(|&q| clo_nach_szene(q)).collect();

    // Wahrheit: aus der Pose heraus neu gebaut.
    let frisch = Koerpernetz::aus_clo_mm(&posed, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
    // Nachgeführt: aus der Binde-Pose gebaut, dann in die Pose geführt.
    let mut nach = Koerpernetz::aus_clo_mm(&netz.punkte, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
    nach.nachfuehren(&pose_szene).expect("nachfuehren");

    // Die Gitter dürfen sich unterscheiden (der nachgeführte Körper behält den
    // weiteren Kasten der Binde-Pose), die Antworten nicht.
    assert!(
        nach.gitter.zahl[0] >= frisch.gitter.zahl[0]
            && nach.gitter.zahl[1] >= frisch.gitter.zahl[1]
            && nach.gitter.zahl[2] >= frisch.gitter.zahl[2],
        "das nachgeführte Raster ist kleiner geworden: {:?} gegen {:?}",
        nach.gitter.zahl,
        frisch.gitter.zahl
    );
    if let Some(w) = werte_weg(&nach, &frisch, false) {
        panic!("nachfuehren mit gehobenem Arm weicht ab: {w}");
    }
    if let Some(w) = abfragen_weg(&nach, &frisch, true) {
        panic!("nachfuehren mit gehobenem Arm: {w}");
    }
    eprintln!(
        "Pose nachgeführt: {} Punkte, {} Dreiecke, Gitter {:?} / {:?}",
        nach.punkte.len(),
        nach.dreiecke.len(),
        nach.gitter.zahl,
        frisch.gitter.zahl
    );
}

/// Satz 3: der Gewinn, gemessen statt behauptet — mindestens 3×.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn nachfuehren_ist_mindestens_dreimal_billiger() {
    let Some((netz, sk)) = makehuman() else {
        return;
    };
    let mut gedreht = sk.clone();
    gedreht.hebe_arme(90.0);
    let posed = gedreht.verforme(&netz.punkte).expect("Verformung möglich");
    let pose_szene: Vec<[f64; 3]> = posed.iter().map(|&q| clo_nach_szene(q)).collect();
    let mut nach = Koerpernetz::aus_clo_mm(&netz.punkte, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");

    const RUNDEN: usize = 5;
    let (mut best_neu, mut best_nach) = (f64::INFINITY, f64::INFINITY);
    for _ in 0..3 {
        let t0 = Instant::now();
        for _ in 0..RUNDEN {
            let r = Koerpernetz::aus_clo_mm(&posed, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
            std::hint::black_box(&r);
        }
        best_neu = best_neu.min(t0.elapsed().as_secs_f64() / RUNDEN as f64);
        let t0 = Instant::now();
        for _ in 0..RUNDEN {
            nach.nachfuehren(&pose_szene).expect("nachfuehren");
            std::hint::black_box(&nach);
        }
        best_nach = best_nach.min(t0.elapsed().as_secs_f64() / RUNDEN as f64);
    }
    let verhaeltnis = best_neu / best_nach;
    eprintln!(
        "Neuaufbau {:.2} ms, nachfuehren {:.2} ms = {:.2}×",
        best_neu * 1e3,
        best_nach * 1e3,
        verhaeltnis
    );
    assert!(
        verhaeltnis >= 3.0,
        "nachfuehren ist nur {verhaeltnis:.2}× billiger ({:.2} ms gegen {:.2} ms), verlangt sind 3×",
        best_nach * 1e3,
        best_neu * 1e3
    );
}
