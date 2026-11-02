//! UI tests of stage 6c: grainlines and notches in the 2D view, and "Nähte vorschlagen…".
//!
//! Headless with egui_kittest like `oberflaeche.rs`. Grainlines and the seam suggestion are
//! compared with an independent evaluation through `klon3d-pattern` (the calls `klon3d naehte
//! --vorschlag` makes), not with the app's own data. None of the test patterns in this repository
//! has a visible grainline, so one test copies the Bluse3 v2 with every grainline switched on.

mod gemeinsam;

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Role, Shape};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use gemeinsam::*;
use klon3d_app::anordnung::{Anzeige2d, SCHLUESSEL_ANZEIGE2D};
use klon3d_app::ansicht2d::passzeichen;
use klon3d_app::app::App;
use klon3d_app::dateiwahl::Zweck;
use klon3d_app::menue::ERST_MUSTER;
use klon3d_app::muster::Nahtstand;
use klon3d_pattern::geom::{lies_messungen, Auswertung};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::lies_nahtliste;
use klon3d_pattern::read_path;

fn hinweis_da(h: &Harness<'_, App>, text: &str) -> bool {
    h.query_all_by_role(Role::Tooltip)
        .any(|t| t.query_by_label_contains(text).is_some())
}

/// The toolbar button `name` (not a menu entry).
fn knopf<'t>(h: &'t Harness<'_, App>, name: &'t str) -> egui_kittest::Node<'t> {
    h.query_all_by_role_and_label(Role::Button, name)
        .find(|n| !in_einem_menue(n))
        .unwrap_or_else(|| panic!("{name} in keiner Werkzeugleiste"))
}

/// Line segments painted in the frame - grainline axes and notch ticks are painted as such.
fn strecken(h: &Harness<'_, App>) -> usize {
    fn zaehlen(s: &Shape) -> usize {
        match s {
            Shape::LineSegment { .. } => 1,
            Shape::Vec(v) => v.iter().map(zaehlen).sum(),
            _ => 0,
        }
    }
    h.output().shapes.iter().map(|c| zaehlen(&c.shape)).sum()
}

/// The pattern and its measurement file evaluated independently.
fn auswerten<R>(muster: &Path, f: impl FnOnce(&Auswertung<'_>) -> R) -> R {
    let m = read_path(muster).unwrap();
    let mess = lies_messungen(&muster.with_file_name("measurements.smis"), &m.unit).unwrap();
    let ev = Auswertung::new(&m, mess);
    f(&ev)
}

/// The Bluse3 v2 with its measurement file copied to `ordner`, every piece given a visible
/// grainline with an explicit axis (the file has none); the arrows run through 0, 1, 2. Returns
/// the copy.
fn bluse_mit_fadenlauf(ordner: &Path) -> PathBuf {
    let text = std::fs::read_to_string(bluse()).unwrap();
    assert!(
        !text.contains("<grainline"),
        "Bluse3 v2 hat jetzt Fadenläufe"
    );
    let teile: Vec<&str> = text.split("<piece ").collect();
    assert_eq!(teile.len(), 29, "28 Teile");
    let mut neu = String::from(teile[0]);
    for (i, rest) in teile[1..].iter().enumerate() {
        let ende = rest.find('>').unwrap() + 1;
        neu += "<piece ";
        neu += &rest[..ende];
        neu += &format!(
            r#"<grainline arrows="{}" length="150" rotation="{}" visible="true"/>"#,
            i % 3,
            if i % 2 == 0 { 90 } else { 45 }
        );
        neu += &rest[ende..];
    }
    let kopie = ordner.join("bluse3_fadenlauf.sm2d");
    std::fs::write(&kopie, neu).unwrap();
    std::fs::copy(
        muster_ordner().join("measurements.smis"),
        ordner.join("measurements.smis"),
    )
    .unwrap();
    kopie
}

#[test]
fn jedes_teil_mit_sichtbarem_fadenlauf_hat_ihn() {
    // The Bluse3 v2 as it is: every piece Seamly2D draws a grainline for has one in the app.
    let h = mit_bluse();
    let m = h.state().muster.as_ref().unwrap();
    let sichtbar = auswerten(&bluse(), |ev| {
        ev.muster()
            .pieces()
            .iter()
            .filter(|p| matches!(ev.fadenlauf(p), Ok(Some(_))))
            .map(|p| p.name.clone().unwrap_or_default())
            .collect::<Vec<_>>()
    });
    for name in &sichtbar {
        let t = m.teile.iter().find(|t| &t.name == name).unwrap();
        assert!(t.fadenlauf.is_some(), "{name}: kein Fadenlauf");
    }
    assert_eq!(
        m.teile.iter().filter(|t| t.fadenlauf.is_some()).count(),
        sichtbar.len()
    );
    assert!(m.teile.iter().all(|t| t.fadenlauf_fehler.is_none()));

    // The copy with every grainline visible: all pieces have it, exactly as evaluated.
    let ordner = frischer_ordner("fadenlauf");
    let kopie = bluse_mit_fadenlauf(&ordner);
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&kopie));
    h.run();
    let m = h.state().muster.as_ref().unwrap();
    let soll = auswerten(&kopie, |ev| {
        ev.muster()
            .pieces()
            .iter()
            .filter_map(|p| {
                let f = ev.fadenlauf(p).unwrap()?;
                Some((p.name.clone().unwrap_or_default(), f.p1, f.p2, f.pfeile))
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(soll.len(), m.teile.len(), "jedes Teil mit Fadenlauf");
    for (name, p1, p2, pfeile) in &soll {
        let t = m.teile.iter().find(|t| &t.name == name).unwrap();
        assert_eq!(t.fadenlauf, Some((*p1, *p2, *pfeile)), "{name}");
    }
    assert!((0..3).all(|k| soll.iter().any(|s| s.3 == k)));

    // Drawn: switching the grainline off removes one axis per piece.
    let mit = strecken(&h);
    knopf(&h, "Fadenlauf").click();
    h.run();
    assert!(!h.state().ansicht.anzeige.fadenlauf);
    assert_eq!(mit - strecken(&h), m_teile(&h));
    let _ = std::fs::remove_dir_all(&ordner);
}

fn m_teile(h: &Harness<'_, App>) -> usize {
    h.state().muster.as_ref().unwrap().teile.len()
}

#[test]
fn passzeichen_gezeichnet_und_schaltbar() {
    let mut h = mit_bluse();
    let m = h.state().muster.as_ref().unwrap();
    let kerben: usize = m
        .teile
        .iter()
        .filter_map(|t| t.rand.as_ref().ok())
        .map(|r| passzeichen(r).len())
        .sum();
    // Independently: the notch nodes of all pieces with a resolvable boundary.
    let soll: usize = auswerten(&bluse(), |ev| {
        ev.muster()
            .pieces()
            .iter()
            .filter_map(|p| ev.teilrand(p, Kurvenquelle::Seamly).ok())
            .map(|r| r.punktknoten.iter().filter(|k| k.kerbe).count())
            .sum()
    });
    assert!(soll > 0);
    assert_eq!(kerben, soll);
    // Off through the menu, on and off again with the toolbar button. Counted only after the
    // toolbar clicks: the closing menu still paints a few line segments of its own.
    menue_waehlen(&mut h, &["Anzeige", "2D-Schnitt"], "Passzeichen");
    menue_schliessen(&mut h, "Anzeige");
    assert!(!h.state().ansicht.anzeige.passzeichen);
    knopf(&h, "Passzeichen").click();
    h.run();
    assert!(h.state().ansicht.anzeige.passzeichen);
    let mit = strecken(&h);
    knopf(&h, "Passzeichen").click();
    h.run();
    assert!(!h.state().ansicht.anzeige.passzeichen);
    assert_eq!(mit - strecken(&h), soll, "ein Strich je Passzeichen");
}

#[test]
fn fadenlauf_und_passzeichen_bleiben_gespeichert() {
    let mut h = mit_bluse();
    let a = h.state().ansicht.anzeige;
    assert!(a.fadenlauf && a.passzeichen, "Vorgabe: beide an");
    knopf(&h, "Passzeichen").click();
    h.run();
    menue_waehlen(&mut h, &["Anzeige", "2D-Schnitt"], "Fadenlauf");
    menue_schliessen(&mut h, "Anzeige");
    let a = h.state().ansicht.anzeige;
    assert!(!a.fadenlauf && !a.passzeichen);

    let mut speicher = Speicher::default();
    eframe::App::save(h.state_mut(), &mut speicher);
    let text = speicher.0[SCHLUESSEL_ANZEIGE2D].clone();
    assert!(
        text.contains("fadenlauf=0") && text.contains("passzeichen=0"),
        "{text}"
    );
    drop(h);
    let mut h = Harness::builder()
        .with_size([1600.0, 1400.0])
        .build_eframe(|cc| {
            cc.storage = Some(&speicher);
            App::neu(cc)
        });
    h.run();
    assert_eq!(h.state().ansicht.anzeige, a);

    // A text stored before both toggles existed switches both on.
    let mut alt = Speicher::default();
    alt.0.insert(
        SCHLUESSEL_ANZEIGE2D.into(),
        "naehte=1;laengen=1;teilnamen=0;massstab=1".into(),
    );
    let h = Harness::builder()
        .with_size([1600.0, 1400.0])
        .build_eframe(|cc| {
            cc.storage = Some(&alt);
            App::neu(cc)
        });
    let a = h.state().ansicht.anzeige;
    assert!(a.fadenlauf && a.passzeichen && a.laengen && !a.teilnamen);
    assert_eq!(Anzeige2d::aus_text(&a.als_text()), a);
}

#[test]
fn ohne_muster_gesperrt_mit_hinweis() {
    let mut h = fenster();
    h.ctx
        .global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
    h.run();
    for name in ["Fadenlauf", "Passzeichen"] {
        let k = knopf(&h, name);
        assert!(k.accesskit_node().is_disabled(), "{name} nicht gesperrt");
        k.hover();
        h.run();
        assert!(hinweis_da(&h, ERST_MUSTER), "{name}: kein Hinweis");
    }
    for (pfad, eintrag) in [
        (&["2D-Schnitt"][..], "Nähte vorschlagen…"),
        (&["Anzeige", "2D-Schnitt"][..], "Fadenlauf"),
        (&["Anzeige", "2D-Schnitt"][..], "Passzeichen"),
    ] {
        menue_oeffnen(&mut h, pfad);
        let k = im_menue(&h, eintrag);
        assert!(k.accesskit_node().is_disabled(), "{eintrag} nicht gesperrt");
        k.hover();
        h.run();
        assert!(hinweis_da(&h, ERST_MUSTER), "{eintrag}: kein Hinweis");
        menue_schliessen(&mut h, pfad[0]);
    }
}

#[test]
fn naehte_vorschlagen_schreibt_und_laedt_die_liste() {
    let ordner = frischer_ordner("vorschlag");
    let kopie = ordner.join("bluse3_v2.sm2d");
    std::fs::copy(bluse(), &kopie).unwrap();
    std::fs::copy(
        muster_ordner().join("measurements.smis"),
        ordner.join("measurements.smis"),
    )
    .unwrap();
    let ziel = ordner.join("bluse3_v2.naehte");
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&kopie));
    h.run();
    assert!(matches!(
        h.state().muster.as_ref().unwrap().naehte,
        Nahtstand::Keine { .. }
    ));

    // The save dialog opens at <pattern>.naehte.
    menue_waehlen(&mut h, &["2D-Schnitt"], "Nähte vorschlagen…");
    let d = h.state().dateiwahl.as_ref().expect("Dialog offen");
    assert_eq!(d.zweck, Zweck::NahtlisteSpeichern);
    assert_eq!(d.name, "bluse3_v2.naehte");
    assert!(h.query_by_label("Nahtliste speichern").is_some());
    h.get_by_label("Datei speichern").click();
    h.run();
    assert!(h.state().dateiwahl.is_none(), "Dialog zu");

    // The file reads back with as many seams as `klon3d naehte --vorschlag` suggests.
    let soll = auswerten(&kopie, |ev| {
        ev.schlage_naehte_vor(Kurvenquelle::Seamly)
            .unwrap()
            .naehte
            .len()
    });
    assert!(soll > 0);
    let text = std::fs::read_to_string(&ziel).unwrap();
    assert_eq!(lies_nahtliste(&text).unwrap().len(), soll);
    // The pattern is shown with the list just written, checked.
    let m = h.state().muster.as_ref().unwrap();
    match &m.naehte {
        Nahtstand::Geprueft { pfad, bericht } => {
            assert_eq!(pfad, &ziel);
            assert_eq!(bericht.naehte.len(), soll);
        }
        anders => panic!("Nahtliste nicht geprüft: {anders:?}"),
    }
    assert_eq!(
        h.state().auftrag.as_ref().unwrap().nahtliste.as_ref(),
        Some(&ziel)
    );

    // Again: the existing file is not overwritten without asking.
    std::fs::write(&ziel, "# alt\n").unwrap();
    menue_waehlen(&mut h, &["2D-Schnitt"], "Nähte vorschlagen…");
    h.get_by_label("Datei speichern").click();
    h.run();
    assert!(h
        .query_by_label_contains("gibt es schon – überschreiben?")
        .is_some());
    assert_eq!(std::fs::read_to_string(&ziel).unwrap(), "# alt\n");
    h.get_by_label("Nein").click();
    h.run();
    assert!(h.state().dateiwahl.is_some(), "Dialog bleibt offen");
    assert_eq!(std::fs::read_to_string(&ziel).unwrap(), "# alt\n");
    h.get_by_label("Datei speichern").click();
    h.run();
    h.get_by_label("Überschreiben").click();
    h.run();
    assert!(h.state().dateiwahl.is_none());
    let text = std::fs::read_to_string(&ziel).unwrap();
    assert_eq!(lies_nahtliste(&text).unwrap().len(), soll);
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn eigenschaften_nennen_fadenlauf_und_seinen_fehler() {
    // The original: no piece has a grainline.
    let mut h = mit_bluse();
    h.state_mut().gewaehlt = Some(0);
    h.run();
    assert!(
        h.query_by_label("keiner").is_some(),
        "kein Fadenlauf: keiner"
    );

    // The copy with grainlines, the first one broken (Seamly2D knows only arrows 0, 1, 2).
    let ordner = frischer_ordner("fadenlauf_fehler");
    let kopie = bluse_mit_fadenlauf(&ordner);
    let text = std::fs::read_to_string(&kopie).unwrap();
    std::fs::write(&kopie, text.replacen(r#"arrows="0""#, r#"arrows="7""#, 1)).unwrap();
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(&kopie));
    h.run();
    let m = h.state().muster.as_ref().unwrap();
    let kaputt: Vec<usize> = (0..m.teile.len())
        .filter(|&i| m.teile[i].fadenlauf_fehler.is_some())
        .collect();
    assert_eq!(kaputt.len(), 1, "genau ein Fadenlauf fehlerhaft");
    let heil = (0..m.teile.len()).find(|i| !kaputt.contains(i)).unwrap();

    h.state_mut().gewaehlt = Some(kaputt[0]);
    h.run();
    assert!(
        h.query_by_label_contains("Fehler: ").is_some(),
        "Fehler des Fadenlaufs in den Eigenschaften"
    );
    assert!(
        h.query_by_label_contains(r#"arrows="7""#).is_some(),
        "der Fehler nennt das Attribut"
    );

    h.state_mut().gewaehlt = Some(heil);
    h.run();
    assert!(h.query_by_label("ja").is_some(), "gültiger Fadenlauf: ja");
    assert!(h.query_by_label_contains("Fehler: ").is_none());
    let _ = std::fs::remove_dir_all(&ordner);
}
