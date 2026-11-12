//! Nahtvorschlag an den fünf Prüfmustern mit Handliste (Befund 101):
//! Kante für Kante gegen die Handliste, Spiegelung aus dem Nahtgraphen und
//! keine verdrehte Naht. Die Lage dazu prüft `klon3d-sim/tests/platzierung.rs`.

use std::path::PathBuf;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, Abschnittsbezug, Nahtdef};
use klon3d_pattern::vorschlag::{spiegelung, vergleiche, verdrehte_naehte};
use klon3d_pattern::Pattern;

const Q: Kurvenquelle = Kurvenquelle::Seamly;

/// Muster, Maße daneben und Handliste eines Prüfmusters.
fn pruefmuster(name: &str) -> (Pattern, PathBuf, Vec<Nahtdef>) {
    let w = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/muster"));
    let pfad = w.join(format!("{name}.sm2d"));
    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let defs = lies_nahtliste(&std::fs::read_to_string(w.join(format!("{name}.naehte"))).unwrap()).unwrap();
    (m, pfad.with_file_name("measurements.smis"), defs)
}

fn sortiert(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// Bluse3, Bluse1, Bustier und Rock: jede Kante der Handliste an dieselben
/// Gegenkanten, nichts verdreht, nichts zu viel — auch die Schlitze des Rocks
/// nicht, die ihre Punktnamen verraten. Der Bolero näht seine Falten noch
/// nicht: dort nur nichts Falsches.
#[test]
fn nahtvorschlag_trifft_handlisten() {
    for (name, zuviel) in [("bluse3_v2", 0), ("bluse1_v25", 0), ("bustier_v1", 0), ("hexen_2026_10_27/rock_v36", 0), ("hexen_2026_10_27/bolero_v37", 0)] {
        let (m, mess, soll) = pruefmuster(name);
        let ev = Auswertung::lade(&m, &mess).unwrap();
        let v = ev.schlage_naehte_vor(Q).unwrap();
        let c = vergleiche(&ev, Q, &soll, &v.naehte).unwrap();
        eprintln!("{name}: {c:?}");
        assert_eq!((c.falsch, c.verdreht, c.zuviel), (0, 0, zuviel), "{name}");
        if !name.ends_with("bolero_v37") {
            assert_eq!((c.richtig, c.fehlend), (c.soll, 0), "{name}");
        }
        assert_eq!(v.widersprueche, 0, "{name}");
        assert!(verdrehte_naehte(&ev, Q, &v.naehte, &v.gespiegelt).unwrap().is_empty(), "{name}");
    }
}

/// Die Spiegelung, die der Vorschlag aus seinen Nähten ableitet, ist die der
/// Handliste — und die der Handlagen (Bluse3, Bluse1: Rückenteile und Ärmel
/// von innen gezeichnet; Bustier: der Vorderblock; Rock: die vier Bahnen).
#[test]
fn spiegelung_aus_den_naehten() {
    let rueck_aermel = |p: &str| {
        ["Rueckenteil Mitte Links", "Rueckenteil Mitte Rechts", "Rueckenteil Seite Links", "Rueckenteil Seite Rechts", "Aermel Kurz Links", "Aermel Kurz Rechts"]
            .iter()
            .map(|t| format!("{p} - {t}"))
            .collect::<Vec<_>>()
    };
    let erwartet: [(&str, Vec<String>); 4] = [
        ("bluse3_v2", rueck_aermel("Bluse 3")),
        ("bluse1_v25", rueck_aermel("Bluse 1")),
        (
            "bustier_v1",
            [
                "Vorderteil mit Bruecke",
                "Seitenvorderteil rechts",
                "Seitenvorderteil links",
                "Untercup innen rechts",
                "Untercup aussen rechts",
                "Untercup innen links",
                "Untercup aussen links",
                "Obercup rechts",
                "Obercup links",
            ]
            .iter()
            .map(|t| format!("Bustier - {t}"))
            .collect(),
        ),
        (
            "hexen_2026_10_27/rock_v36",
            ["2 VORNE RECHTS - obere Lage an CF", "3 VORNE LINKS - untere Lage an CF", "4 HINTEN RECHTS - obere Lage an CB", "5 HINTEN LINKS - untere Lage an CB"]
                .iter()
                .map(|t| t.to_string())
                .collect(),
        ),
    ];
    for (name, soll) in erwartet {
        let (m, mess, defs) = pruefmuster(name);
        let ev = Auswertung::lade(&m, &mess).unwrap();
        let v = ev.schlage_naehte_vor(Q).unwrap();
        let (aus_liste, widersprueche) = spiegelung(&ev, Q, &defs).unwrap();
        assert_eq!(widersprueche, 0, "{name}");
        let ende = |v: Vec<String>| sortiert(v.into_iter().map(|s| s.rsplit_once(" - ").map_or(s.clone(), |(_, b)| b.to_string())).collect());
        let soll_kurz = ende(soll.clone());
        assert_eq!(ende(v.gespiegelt.clone()), soll_kurz, "{name}: Vorschlag");
        assert_eq!(ende(aus_liste), soll_kurz, "{name}: Handliste");
    }
    // Bolero: aus der Handliste Rückenteil, Ärmel und Manschetten.
    let (m, mess, defs) = pruefmuster("hexen_2026_10_27/bolero_v37");
    let ev = Auswertung::lade(&m, &mess).unwrap();
    let (g, w) = spiegelung(&ev, Q, &defs).unwrap();
    assert_eq!(w, 0);
    assert_eq!(
        sortiert(g),
        sortiert(
            ["Rueckenteil 1x", "Bischofaermel links 1x", "Bischofaermel rechts 1x", "Manschette links 1x", "Manschette rechts 1x"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        )
    );
}

/// Die Schlitze des Rocks bleiben offen: bis zum Kniepunkt (`KN…`) wird
/// genäht, darunter nicht. Die Bundmarken `B_Schlitz_…` heißen genauso, liegen
/// aber mitten in einer Naht und teilen sie nicht — sonst risse der Bund auf.
#[test]
fn schlitze_bleiben_offen() {
    let (m, mess, _) = pruefmuster("hexen_2026_10_27/rock_v36");
    let ev = Auswertung::lade(&m, &mess).unwrap();
    let v = ev.schlage_naehte_vor(Q).unwrap();
    // Die Schlitzkante läuft vom Kniepunkt zum Saum; ihre inneren Punkte
    // heißen PZ_CF_u1 … PZ_CF_u4.
    let zum_saum = |z: &Abschnittsbezug| z.von.contains("KN") && (z.bis.contains("PZ_CF_u") || z.bis.starts_with("H0_CF"));
    for d in &v.naehte {
        for z in d.a.iter().chain(&d.b) {
            assert!(!zum_saum(z), "{}: näht den Schlitz zu: {} -> {}", d.kennung, z.von, z.bis);
        }
    }
    // Über dem Knie wird genäht: die zwei Mittelnähte enden dort, je beidseits.
    let am_knie = v
        .naehte
        .iter()
        .flat_map(|d| d.a.iter().chain(&d.b))
        .filter(|z| z.von.contains("KN") || z.bis.contains("KN"))
        .count();
    assert_eq!(am_knie, 4, "vordere und hintere Mitte enden am Knie, je beidseits");
    // Der Bund läuft über seine Marken hinweg, statt dort zu enden.
    let bund = |von: &str, bis: &str| {
        v.naehte.iter().any(|d| d.a.iter().chain(&d.b).any(|z| z.von.starts_with(von) && z.bis.starts_with(bis)))
    };
    assert!(bund("B_CB_links", "B_CB_rechts"), "Bund nicht am Stück genäht");
}

/// Die Falten des Boleros bleiben unerkannt, und zwar nicht aus Vorsicht,
/// sondern weil sie geometrisch keine Ecke sind: am Ärmel liegen beide
/// Hälften einer Falte gleich lang, die drei Punkte fast auf einer Geraden
/// und der Knick unter 5° statt der 25° einer Ecke. Eine Kollinearitätsgrenze,
/// die alle Falten trifft, nimmt die Manschettenmarke `MM` mit. Der Test hält
/// fest, dass die Schwelle nicht still gesenkt wird: der Ärmel bleibt eine
/// Kette von 61,80 cm.
#[test]
fn falten_sind_keine_ecken() {
    use klon3d_pattern::geom::Pt;
    use klon3d_pattern::kanten::Teilrand;
    use klon3d_pattern::vorschlag::ECKE_GRAD;

    /// Richtung über mindestens 2 mm, wie der Vorschlag sie nimmt.
    fn richtung(p: &[Pt], am_anfang: bool) -> Option<(f64, f64)> {
        let n = p.len();
        if n < 2 {
            return None;
        }
        let (a, mut i) = if am_anfang { (p[0], 1) } else { (p[n - 1], n - 2) };
        loop {
            let b = p[i];
            if a.dist(b) >= 0.2 || (am_anfang && i == n - 1) || (!am_anfang && i == 0) {
                let (dx, dy) = if am_anfang { (b.x - a.x, b.y - a.y) } else { (a.x - b.x, a.y - b.y) };
                let l = (dx * dx + dy * dy).sqrt();
                return (l > 0.0).then(|| (dx / l, dy / l));
            }
            i = if am_anfang { i + 1 } else { i - 1 };
        }
    }
    /// Knick am Punktknoten `i` in Grad.
    fn knick(r: &Teilrand, i: usize) -> f64 {
        let n = r.kanten.len();
        let a = richtung(&r.kanten[(i + n - 1) % n].punkte, false).unwrap();
        let b = richtung(&r.kanten[i].punkte, true).unwrap();
        (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0).acos().to_degrees()
    }
    /// Abstand des Mittelpunkts von der Geraden durch die beiden Nachbarn, mm.
    fn gerade(r: &Teilrand, i: usize) -> f64 {
        let n = r.kanten.len();
        let (vor, nach) = (&r.kanten[(i + n - 1) % n], &r.kanten[i]);
        let (a, b, m) = (vor.punkte[0], nach.punkte[nach.punkte.len() - 1], nach.punkte[0]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = (dx * dx + dy * dy).sqrt();
        assert!(l > 0.0, "Nachbarn fallen zusammen");
        ((m.x - a.x) * dy - (m.y - a.y) * dx).abs() / l * r.mm_pro_einheit
    }
    fn teil<'a>(ev: &'a Auswertung<'a>, name: &str) -> Teilrand {
        let stueck = ev.muster().pieces().into_iter().find(|p| p.name.as_deref() == Some(name)).expect(name);
        ev.teilrand(&stueck, Q).unwrap()
    }

    let (m, mess, _) = pruefmuster("hexen_2026_10_27/bolero_v37");
    let ev = Auswertung::lade(&m, &mess).unwrap();

    // Je Ärmel sieben Kugel- und sechs Saumfalten, laut Handliste.
    let falten: Vec<String> = (["KF1M", "KF2M", "KF3M", "SPT", "KF5M", "KF6M", "KF7M", "KS1M", "KS2M", "KS3M", "KS4M", "KS5M", "KS6M"] as [&str; 13])
        .iter()
        .flat_map(|n| [n.to_string(), format!("{n}r")])
        .collect();
    let mut gesehen = 0usize;
    for name in ["Bischofaermel links 1x", "Bischofaermel rechts 1x"] {
        let r = teil(&ev, name);
        let n = r.kanten.len();
        for i in 0..n {
            let punkt = r.punktknoten[i].name.as_str();
            if !falten.iter().any(|f| f == punkt) {
                continue;
            }
            gesehen += 1;
            let k = knick(&r, i);
            // Keine Ecke: weit unter der Schwelle, die eine Kette bricht.
            assert!(k < 10.0 && k < ECKE_GRAD, "{name} {punkt}: Knick {k:.2}° ist eine Ecke");
            // Beide Hälften kurz und gleich lang — die Falte ist ein Paar.
            let (la, lb) = (r.kanten[(i + n - 1) % n].laenge * r.mm_pro_einheit, r.kanten[i].laenge * r.mm_pro_einheit);
            assert!(
                la > 14.0 && la < 17.0 && (la - lb).abs() < 1.0,
                "{name} {punkt}: Hälften {la:.2}/{lb:.2} mm"
            );
        }
    }
    assert_eq!(gesehen, 26, "13 Falten je Ärmel");

    // Eine Geradengrenze, die alle Falten trifft, trifft auch die Marken: die
    // weiteste Falte liegt bei 2,46 mm, die Manschettenmarke MM bei 0,00 mm
    // und die Armlochkerbe FSM bei 0,26 mm. Bei 3 mm sind alle 26 Falten
    // gefunden — und sechs Fremde dazu, die keine sind.
    let aermel = teil(&ev, "Bischofaermel links 1x");
    let weiteste = (0..aermel.kanten.len())
        .filter(|&i| falten.iter().any(|f| *f == aermel.punktknoten[i].name))
        .map(|i| gerade(&aermel, i))
        .fold(0.0f64, f64::max);
    assert!(weiteste > 2.0 && weiteste < 3.0, "weiteste Falte {weiteste:.3} mm");
    for (name, punkt) in [("Manschette links 1x", "MM"), ("Bischofaermel links 1x", "FSM")] {
        let r = teil(&ev, name);
        let i = (0..r.kanten.len()).find(|&i| r.punktknoten[i].name == punkt).expect(punkt);
        let g = gerade(&r, i);
        assert!(g <= 3.0, "{punkt} liegt {g:.3} mm auf der Geraden — unter derselben Grenze wie die Falten");
    }

    // Und so bleibt der Ärmel eine Kette: der Vorschlag findet am Bolero
    // fünf Nähte, die Ärmel und Manschetten stehen allein.
    let v = ev.schlage_naehte_vor(Q).unwrap();
    assert_eq!(v.naehte.len(), 5, "Bolero: fünf Nähte aus dem Muster");
    for name in ["Bischofaermel links 1x", "Bischofaermel rechts 1x"] {
        let kette = v.allein.iter().find(|(t, _, _, l)| t == name && (l - 61.801).abs() < 0.01).expect(name);
        // Die Falten sitzen in dieser einen Kette: sie beginnt und endet an
        // den beiden Saumpunkten SB, statt zwischen ihnen zu brechen.
        assert!(kette.1.starts_with("SB") && kette.2.starts_with("SB"), "{name}: {kette:?}");
    }
}

/// Aufgelegte Teile nähen nicht: der Keyhole-Trim der Bluse1 (teilt mit dem
/// Vorderteil nur eine Randkante, auf derselben Seite) und die äußeren
/// Kragenlagen.
#[test]
fn auflagen_bleiben_ungenaeht() {
    for (name, teile) in [("bluse1_v25", ["Trim Keyhole-Kante Rechts", "Stehkragen Aussen"]), ("bluse3_v2", ["Trim Oeffnung Links", "Stehkragen Aussen"])] {
        let (m, mess, _) = pruefmuster(name);
        let ev = Auswertung::lade(&m, &mess).unwrap();
        let v = ev.schlage_naehte_vor(Q).unwrap();
        for d in &v.naehte {
            for z in d.a.iter().chain(&d.b) {
                assert!(!teile.iter().any(|t| z.teil.contains(t)), "{name}: {} näht {}", d.kennung, z.teil);
            }
        }
    }
}
