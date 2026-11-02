//! UI tests of 3D-Kleidung > Netzprüfung: disabled without a pattern, and with the Bluse3 v2 a
//! report window with the numbers of an independent `vernetze` + `pruefe` call (the calls of
//! `klon3d netz`, without the pipeline).

mod gemeinsam;

use egui::Role;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use gemeinsam::*;
use klon3d_app::app::App;
use klon3d_app::menue::ERST_MUSTER;
use klon3d_mesh::pruefung;
use klon3d_pattern::geom::{lies_messungen, Auswertung};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::read_path;

use eframe::egui;

fn hinweis_da(h: &Harness<'_, App>, text: &str) -> bool {
    h.query_all_by_role(Role::Tooltip)
        .any(|t| t.query_by_label_contains(text).is_some())
}

/// The value right of the label `name` in its grid row.
fn wert(h: &Harness<'_, App>, name: &str) -> String {
    zeile_nach(h, Role::Label, name, 1)[0].clone()
}

#[test]
fn ohne_muster_gesperrt_mit_hinweis() {
    let mut h = fenster();
    h.ctx
        .global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
    h.run();
    menue_oeffnen(&mut h, &["3D-Kleidung"]);
    let k = im_menue(&h, "Netzprüfung");
    assert!(
        k.accesskit_node().is_disabled(),
        "Netzprüfung nicht gesperrt"
    );
    k.hover();
    h.run();
    assert!(hinweis_da(&h, ERST_MUSTER), "kein Hinweis");
    assert!(h.state().netzpruefung.is_none());
}

#[test]
fn bericht_wie_vernetze_und_pruefe() {
    let mut h = mit_bluse();
    menue_waehlen(&mut h, &["3D-Kleidung"], "Netzprüfung");
    let n = match &h.state().netzpruefung {
        Some((_, Ok(n))) => n.clone(),
        andere => panic!("kein Bericht: {andere:?}"),
    };
    assert!(h.query_by_label("Netzprüfung schließen").is_some());

    // Independently, as `klon3d netz` computes it.
    let muster = read_path(&bluse()).unwrap();
    let mess = lies_messungen(&muster_ordner().join("measurements.smis"), &muster.unit).unwrap();
    let ev = Auswertung::new(&muster, mess);
    let text = std::fs::read_to_string(bluse().with_extension("naehte")).unwrap();
    let defs = lies_nahtliste(&text).unwrap();
    let naehte = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let p = klon3d_mesh::Parameter::default();
    let netz = klon3d_mesh::vernetze(&ev, Kurvenquelle::Seamly, &naehte.naehte, &p).unwrap();
    let soll = pruefung::pruefe(&netz);
    let punkte: usize = netz.teile.iter().map(|t| t.punkte.len()).sum();
    let dreiecke: usize = netz.teile.iter().map(|t| t.dreiecke.len()).sum();
    let versatz = soll
        .naehte
        .iter()
        .map(|n| n.versatz_max)
        .fold(0.0, f64::max);
    let winkel = soll
        .teile
        .iter()
        .map(|t| t.min_winkel)
        .fold(f64::INFINITY, f64::min);

    assert_eq!(wert(&h, "Teile im Muster"), "28");
    assert_eq!(wert(&h, "Teile vernetzt"), soll.teile.len().to_string());
    assert_eq!(wert(&h, "Punkte gesamt"), punkte.to_string());
    assert_eq!(wert(&h, "Dreiecke gesamt"), dreiecke.to_string());
    assert_eq!(wert(&h, "Nähte"), soll.naehte.len().to_string());
    assert_eq!(soll.naehte.len(), 24);
    assert_eq!(wert(&h, "größter Nahtversatz"), format!("{versatz:.3} mm"));
    assert_eq!(wert(&h, "kleinster Winkel"), format!("{winkel:.1}°"));
    assert_eq!(
        wert(&h, "Runden Teilen–Neuvernetzen"),
        netz.runden.to_string()
    );
    let bestanden = soll.verstoesse(&p).is_empty() && netz.offene_teilungen == 0;
    assert_eq!(
        wert(&h, "Abnahme"),
        if bestanden {
            "bestanden"
        } else {
            "NICHT bestanden"
        }
    );
    assert!(wert(&h, "Nahtliste").ends_with("bluse3_v2.naehte"));
    assert!(wert(&h, "Dauer").ends_with(" ms"));
    assert_eq!(
        wert(&h, "Dauer"),
        format!("{:.0} ms", n.dauer.as_secs_f64() * 1000.0)
    );

    // One row per piece with the numbers of `klon3d netz`. The object browser lists the same
    // short names, so take the label whose row carries the expected cells.
    for t in &soll.teile {
        let kurz = kurzname(&t.teil);
        let erwartet = vec![
            t.punkte.to_string(),
            t.randpunkte.to_string(),
            t.dreiecke.to_string(),
            format!("{:.1}°", t.min_winkel),
            format!("{:.1}/{:.1}", t.kante_median, t.kante_max),
        ];
        let gefunden = h.query_all_by_role_and_label(Role::Label, kurz).any(|k| {
            let a = k.accesskit_node();
            let Some(eltern) = a.parent() else {
                return false;
            };
            let kinder: Vec<_> = eltern.children().collect();
            let i = kinder.iter().position(|c| c.id() == a.id()).unwrap();
            kinder.len() >= i + 6
                && kinder[i + 1..i + 6]
                    .iter()
                    .map(|c| c.value().or_else(|| c.label()).unwrap_or_default())
                    .collect::<Vec<_>>()
                    == erwartet
        });
        assert!(gefunden, "{kurz}: keine Zeile {erwartet:?}");
    }

    // The window keeps its place once shown (a scroll area that grew by a few pixels per frame
    // moved the close button away from under the first click).
    let knopf = h.get_by_label("Netzprüfung schließen").rect();
    for _ in 0..3 {
        h.step();
        assert_eq!(h.get_by_label("Netzprüfung schließen").rect(), knopf);
    }
    h.get_by_label("Netzprüfung schließen").click();
    h.run();
    assert!(h.state().netzpruefung.is_none());
}

#[test]
fn bericht_schliesst_mit_esc_und_beim_laden() {
    let mut h = mit_bluse();
    menue_waehlen(&mut h, &["3D-Kleidung"], "Netzprüfung");
    assert!(h.state().netzpruefung.is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().netzpruefung.is_none());

    // A report of the files shown closes when another pattern is loaded.
    menue_waehlen(&mut h, &["3D-Kleidung"], "Netzprüfung");
    assert!(h.state().netzpruefung.is_some());
    let anderes = muster_ordner().join("bluse3_v1.sm2d");
    assert!(h.state_mut().muster_laden(anderes));
    h.run();
    assert!(h.state().netzpruefung.is_none());
    assert!(h.query_by_label("Netzprüfung schließen").is_none());
}
