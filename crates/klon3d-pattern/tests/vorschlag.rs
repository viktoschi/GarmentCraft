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
