//! Innenpfade (`<path>` und `<iPaths>`): was das Muster als Linie auf einem
//! Teil zeichnet, ohne dass es zur Nahtlinie gehoert.
//!
//! Geprüft wird an synthetischen Mustern mit bekanntem Maß (ein Kreis aus zwei
//! Halbboegen, ein Viertelkreis) und an vier Mustern des Bestands: Zahl
//! der Pfade je Teil, die Zuordnung ueber `<iPaths>`, offen/geschlossen und die
//! Laenge der gezeichneten Polylinie.

use std::collections::HashMap;
use std::path::PathBuf;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::innenpfad::Innenpfad;
use klon3d_pattern::read_str;

// ───────────────────────────────────────────── synthetische Muster ──

/// Ein Muster mit bekanntem Maß: ein geschlossener, auszuschneidender Kreis
/// aus zwei Bögen und zwei Punkten (fuenf Knoten, Anfang und Ende an
/// **verschiedenen** Modellierzielen, wie das Kopfloch des Hexenhuts), ein
/// Viertelkreis als offener Pfad und ein Pfad mit unaufloesbarem Knoten.
fn synthetisch() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
  <version>0.7.4</version>
  <unit>cm</unit>
  <draftBlock name="A">
    <calculation>
      <point id="1" name="M" type="single" x="0" y="0" mx="0" my="0"/>
      <point id="2" name="P" type="single" x="10" y="0" mx="0" my="0"/>
      <point id="3" name="Q" type="single" x="0" y="10" mx="0" my="0"/>
      <point id="4" name="L" type="single" x="-5" y="0" mx="0" my="0"/>
      <point id="5" name="R" type="single" x="5" y="0" mx="0" my="0"/>
      <arc id="6" type="simple" center="1" angle1="0" angle2="90" radius="10"/>
      <arc id="7" type="simple" center="1" angle1="0" angle2="180" radius="5"/>
      <arc id="8" type="simple" center="1" angle1="180" angle2="0" radius="5"/>
    </calculation>
    <modeling>
      <point id="11" idObject="2" type="modeling"/>
      <arc id="12" idObject="6" type="modeling"/>
      <point id="21" idObject="5" type="modeling"/>
      <arc id="22" idObject="7" type="modeling"/>
      <point id="23" idObject="4" type="modeling"/>
      <arc id="24" idObject="8" type="modeling"/>
      <point id="25" idObject="5" type="modeling"/>
      <path cut="true" id="30" name="Loch" type="2">
        <nodes>
          <node idObject="21" type="NodePoint"/>
          <node idObject="22" type="NodeArc"/>
          <node idObject="23" type="NodePoint"/>
          <node idObject="24" type="NodeArc"/>
          <node idObject="25" type="NodePoint"/>
        </nodes>
      </path>
      <path cut="false" id="31" name="Viertelkreis" type="2">
        <nodes>
          <node idObject="11" type="NodePoint"/>
          <node idObject="12" type="NodeArc"/>
        </nodes>
      </path>
      <path id="32" name="Kaputt" type="2">
        <nodes>
          <node idObject="21" type="NodePoint"/>
          <node idObject="999" type="NodePoint"/>
        </nodes>
      </path>
      <path id="33" name="Rueckwaerts" type="2">
        <nodes>
          <node idObject="12" type="NodeArc" reverse="1"/>
          <node idObject="11" type="NodePoint"/>
        </nodes>
      </path>
    </modeling>
    <pieces>
      <piece id="40" name="Quadrat">
        <nodes>
          <node idObject="21" type="NodePoint"/>
          <node idObject="23" type="NodePoint"/>
        </nodes>
        <iPaths>
          <record path="30"/>
          <record path="31"/>
          <record path="32"/>
          <record path="33"/>
        </iPaths>
      </piece>
      <piece id="41" name="Ohne">
        <nodes>
          <node idObject="21" type="NodePoint"/>
        </nodes>
      </piece>
    </pieces>
  </draftBlock>
</pattern>"#
        .to_string()
}

fn pfad<'a>(liste: &'a [Innenpfad], id: &str) -> &'a Innenpfad {
    liste
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("Pfad {id} fehlt: {:?}", liste.iter().map(|p| &p.id).collect::<Vec<_>>()))
}

#[test]
fn geschlossener_ring_aus_zwei_boegen_und_zwei_punkten() {
    let m = read_str(&synthetisch()).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let teile = m.pieces().to_vec();
    let (liste, meldung) = ev.innenpfade(teile[0]);

    let loch = pfad(&liste, "30");
    assert!(loch.cut, "cut=\"true\" muss ankommen");
    assert_eq!(loch.name, "Loch");
    assert!(loch.geschlossen, "die aufgeloesten Enden fallen zusammen");
    assert_eq!(loch.knoten, 5);
    // Kreis mit r = 5 cm, aus zwei Halbboegen zu je 64 Schritten: 2 pi r =
    // 314,16 mm; die gezeichnete Polylinie ist 0,03 mm kuerzer.
    assert!((loch.laenge_mm - 314.16).abs() < 0.1, "{} mm", loch.laenge_mm);
    // Der Punkt am Ende darf nicht doppelt in der Kette stehen; jeder Bogen
    // bringt 64 Schritte, ein Punktknoten auf dem Anschlusspunkt keinen.
    assert_eq!(loch.punkte.len(), 128, "Schlusspunkt gehoert nicht in die Kette");
    let flaeche = loch.flaeche_mm2.expect("geschlossen hat eine Flaeche");
    // pi r² = 7853,98 mm²; das 128-Eck ist 3,15 mm² (0,04 %) kleiner.
    assert!((flaeche - 7853.98).abs() < 4.0, "{flaeche} mm²");

    // Drei Pfade loesen auf, einer (Knoten 999) wird gemeldet und nicht
    // ausgegeben (eine Kette mit Loch haette eine falsche Laenge und eine
    // falsche Flaeche).
    assert_eq!(liste.len(), 3, "{:?}", liste.iter().map(|p| &p.id).collect::<Vec<_>>());
    assert_eq!(meldung.len(), 1, "{meldung:?}");
    assert!(meldung[0].contains("999"), "{meldung:?}");

    // Ein Teil ohne <iPaths> hat keine Innenpfade und nichts zu melden.
    let (leer, nichts) = ev.innenpfade(teile[1]);
    assert!(leer.is_empty() && nichts.is_empty());
}

#[test]
fn offener_bogen_ist_laenger_als_seine_sehne() {
    let m = read_str(&synthetisch()).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let teile = m.pieces().to_vec();
    let (liste, _) = ev.innenpfade(teile[0]);

    let bogen = pfad(&liste, "31");
    assert!(!bogen.cut, "fehlendes cut zaehlt als false");
    assert!(!bogen.geschlossen);
    assert_eq!(bogen.knoten, 2);
    assert_eq!(bogen.flaeche_mm2, None);
    // Viertelkreis mit r = 10 cm: 2 pi r / 4 = 15,708 cm = 157,08 mm. Die
    // gezeichnete Polylinie (64 Schritte) ist 0,004 mm kuerzer.
    assert!((bogen.laenge_mm - 157.08).abs() < 0.05, "{} mm", bogen.laenge_mm);
    let sehne = 100.0 * 2f64.sqrt();
    assert!(bogen.laenge_mm > sehne + 10.0, "{} mm", bogen.laenge_mm);
}

#[test]
fn rueckwaerts_bogen_laeuft_in_seiner_richtung() {
    let m = read_str(&synthetisch()).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let teile = m.pieces().to_vec();
    let (liste, _) = ev.innenpfade(teile[0]);

    let r = pfad(&liste, "33");
    assert!(!r.geschlossen);
    // Der Bogen laeuft von 0 bis 90 Grad, also von (10, 0) nach (0, -10) in
    // Mustereinheiten; rueckwaerts beginnt die Kette in (0, -10). Der
    // Punktknoten danach liegt auf (10, 0) - dort begann der Bogen.
    let start = r.punkte.first().expect("Kette");
    assert!(start.dist(klon3d_pattern::geom::Pt::new(0.0, -10.0)) < 1e-9, "{start:?}");
    let ende = r.punkte.last().expect("Kette");
    assert!(ende.dist(klon3d_pattern::geom::Pt::new(10.0, 0.0)) < 1e-9, "{ende:?}");
    // Die Laenge bleibt die des Bogens: 157,08 mm.
    assert!((r.laenge_mm - 157.08).abs() < 0.05, "{} mm", r.laenge_mm);
}

// ────────────────────────────────────────────────── Muster des Bestands ──

fn wochenordner() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/muster"))
}

/// Laedt ein Muster samt der Massdatei, die daneben liegt.
fn laden(name: &str, ordner: &str) -> (klon3d_pattern::Pattern, HashMap<String, f64>) {
    let dir = if ordner.is_empty() {
        wochenordner()
    } else {
        wochenordner().join(ordner)
    };
    let m = klon3d_pattern::read_path(&dir.join(format!("{name}.sm2d")))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let mess = klon3d_pattern::geom::lies_messungen(&dir.join("measurements.smis"), &m.unit)
        .unwrap_or_else(|e| panic!("{name}: Massdatei {e}"));
    (m, mess)
}

/// Zahl der Pfade je Muster und Teil.
fn zaehlung(name: &str, ordner: &str, teile_mit: usize) -> Vec<Vec<String>> {
    let (m, mess) = laden(name, ordner);
    let ev = Auswertung::new(&m, mess);
    let mut je_teil = Vec::new();
    let mut mit = 0usize;
    for t in m.pieces() {
        let (liste, meldung) = ev.innenpfade(t);
        assert!(meldung.is_empty(), "{name}: {meldung:?}");
        if !liste.is_empty() {
            mit += 1;
        }
        je_teil.push(liste.iter().map(|p| p.id.clone()).collect());
    }
    assert_eq!(mit, teile_mit, "{name}: Teile mit Innenpfaden");
    je_teil
}

#[test]
fn rock_v36_hat_zwoelf_pfade_an_vier_teilen() {
    let je_teil = zaehlung("rock_v36", "hexen_2026_10_27", 4);
    let gesamt: usize = je_teil.iter().map(|v| v.len()).sum();
    assert_eq!(gesamt, 12);
    // Jedes der vier Teile traegt seine drei Klebezonen.
    for v in je_teil.iter().filter(|v| !v.is_empty()) {
        assert_eq!(v.len(), 3, "{v:?}");
    }
}

#[test]
fn rock_2schlitze_v34_hat_zwoelf_pfade() {
    let je_teil = zaehlung("rock_2schlitze_v34", "", 4);
    assert_eq!(je_teil.iter().map(|v| v.len()).sum::<usize>(), 12);
}

#[test]
fn strumpf_v26_hat_achtzehn_pfade() {
    let je_teil = zaehlung("strumpf_v26", "hexen_2026_10_27", 10);
    assert_eq!(je_teil.iter().map(|v| v.len()).sum::<usize>(), 18);
}

#[test]
fn bustier_v1_hat_32_pfade_und_15_am_rueckteil() {
    let je_teil = zaehlung("bustier_v1", "", 4);
    assert_eq!(je_teil.iter().map(|v| v.len()).sum::<usize>(), 32);
    let (m, mess) = laden("bustier_v1", "");
    let ev = Auswertung::new(&m, mess);
    let rueck = m
        .pieces()
        .into_iter()
        .find(|t| t.name.as_deref() == Some("Bustier - Rueckteil rechts"))
        .expect("Rueckteil rechts");
    let (liste, meldung) = ev.innenpfade(rueck);
    assert!(meldung.is_empty(), "{meldung:?}");
    assert_eq!(liste.len(), 15);
    // 14 Oesen (je 3 mm + 3 mm) und der Stab; alle offen und gezeichnet.
    for p in liste.iter().filter(|p| p.name.starts_with("Oese")) {
        assert!(!p.cut && !p.geschlossen, "{}", p.name);
        assert!((p.laenge_mm - 6.0).abs() < 0.05, "{}: {} mm", p.name, p.laenge_mm);
    }
}

/// Die Laenge ist die gezeichnete Polylinie: sie stimmt mit der Summe der
/// Strecken ueberein, die aus denselben aufgeloesten Punkten folgt. Gilt fuer
/// alle vier Muster des Bestands - dort bestehen die Pfade aus Punktknoten,
/// also ist die Summe unabhaengig von der Zerlegung der Kurven.
#[test]
fn laenge_ist_die_summe_der_gezeichneten_strecken() {
    for (name, ordner, soll) in [
        ("rock_v36", "hexen_2026_10_27", 12usize),
        ("rock_2schlitze_v34", "", 12),
        ("strumpf_v26", "hexen_2026_10_27", 18),
        ("bustier_v1", "", 32),
    ] {
        let (m, mess) = laden(name, ordner);
        let ev = Auswertung::new(&m, mess);
        let f = m.unit_to_mm().unwrap_or(1.0);
        let mut geprueft = 0usize;
        for t in m.pieces() {
            for p in ev.innenpfade(t).0 {
                let mut summe: f64 = (0..p.punkte.len().saturating_sub(1))
                    .map(|i| p.punkte[i].dist(p.punkte[i + 1]))
                    .sum::<f64>();
                if p.geschlossen {
                    summe += p.punkte[0].dist(*p.punkte.last().unwrap());
                }
                let summe = summe * f;
                assert!(
                    (p.laenge_mm - summe).abs() < 1e-9,
                    "{name} {}: {} gegen {}",
                    p.name,
                    p.laenge_mm,
                    summe
                );
                geprueft += 1;
            }
        }
        assert_eq!(geprueft, soll, "{name}: die Zahl der geprueften Pfade");
    }
}

/// Die Klebezonen des Rocks sind 11 cm lang, die kleine 5 cm; eine
/// geschlossene Flaeche hat keiner dieser Pfade.
#[test]
fn klebezonen_rock_haben_ihre_laengen() {
    let (m, mess) = laden("rock_v36", "hexen_2026_10_27");
    let ev = Auswertung::new(&m, mess);
    let mut laengen: Vec<(String, f64)> = Vec::new();
    for t in m.pieces() {
        for p in ev.innenpfade(t).0 {
            assert!(!p.geschlossen && !p.cut, "{}", p.name);
            laengen.push((p.name, p.laenge_mm));
        }
    }
    for (name, soll) in [
        ("Klebezone Blende innen 1 cm", 899.9),
        ("Klebezone VS1 2 x 11 cm", 130.0),
        ("Klebezone VS2 3 x 2 cm", 50.0),
    ] {
        let treffer: Vec<f64> = laengen
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, l)| *l)
            .collect();
        assert_eq!(treffer.len(), 4, "{name} steht an vier Teilen");
        for l in treffer {
            assert!((l - soll).abs() < 0.1, "{name}: {l} statt {soll} mm");
        }
    }
}
