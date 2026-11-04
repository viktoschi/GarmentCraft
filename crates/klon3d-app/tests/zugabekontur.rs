//! UI tests of the seam allowance contour in the 2D view.
//!
//! Headless with egui_kittest like `oberflaeche.rs`. The contour the app keeps per piece is
//! compared with an independent evaluation through `klon3d-pattern` (`Saumkontur::aus_teilrand`,
//! the call the DXF export makes), not with the app's own data. The drawing itself is counted as
//! closed painted paths - one per piece with a contour.

mod gemeinsam;

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Key, Modifiers, Role, Shape};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use gemeinsam::*;
use klon3d_app::anordnung::{Anzeige2d, SCHLUESSEL_ANZEIGE2D};
use klon3d_app::app::App;
use klon3d_app::menue::ERST_MUSTER;
use klon3d_pattern::geom::{lies_messungen, Auswertung, Pt};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_path;
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};

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

/// Closed painted paths - one per piece outline, selection band and allowance contour.
fn geschlossene_wege(h: &Harness<'_, App>) -> usize {
    fn zaehlen(s: &Shape) -> usize {
        match s {
            Shape::Path(p) if p.closed => 1,
            Shape::Vec(v) => v.iter().map(zaehlen).sum(),
            _ => 0,
        }
    }
    h.output().shapes.iter().map(|c| zaehlen(&c.shape)).sum()
}

fn weg() -> PathBuf {
    muster_ordner().join("rock_2schlitze_v34.sm2d")
}

/// The contour `klon3d-pattern` computes for every piece of `muster`, by piece name.
///
/// A local deduplication, not `muster::bereinigt`: `punkte()` repeats both ends of every segment,
/// and the ring must not.
fn soll_konturen(muster: &Path) -> Vec<(String, Option<Vec<Pt>>)> {
    let m = read_path(muster).unwrap();
    let mess = lies_messungen(&muster.with_file_name("measurements.smis"), &m.unit).unwrap();
    let ev = Auswertung::new(&m, mess);
    ev.muster()
        .pieces()
        .iter()
        .map(|p| {
            let name = p.name.clone().unwrap_or_default();
            let kontur = ev.teilrand(p, Kurvenquelle::Seamly).ok().and_then(|r| {
                let has = r
                    .punktknoten
                    .iter()
                    .any(|k| k.zugabe_nach.unwrap_or(0.0).abs() > 1e-9);
                if !has {
                    return None;
                }
                let mut pts: Vec<Pt> = Vec::new();
                for &q in Saumkontur::aus_teilrand(&r, Ecken::Eigen).punkte() {
                    if pts.last().is_none_or(|s: &Pt| s.dist(q) > 1e-9) {
                        pts.push(q);
                    }
                }
                while pts.len() > 1 && pts[0].dist(*pts.last().unwrap()) <= 1e-9 {
                    pts.pop();
                }
                (pts.len() >= 3).then_some(pts)
            });
            (name, kontur)
        })
        .collect()
}

fn gleiche_punkte(a: &[Pt], b: &[Pt]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p.dist(*q) <= 1e-9 * (1.0 + p.x.abs().max(q.x.abs())))
}

#[test]
fn jedes_teil_traegt_seine_zugabekontur() {
    let h = mit_bluse();
    let m = h.state().muster.as_ref().unwrap();
    let soll = soll_konturen(&bluse());
    assert_eq!(soll.len(), m.teile.len(), "jedes Teil der Bluse");
    for (name, kontur) in &soll {
        let t = m.teile.iter().find(|t| &t.name == name).unwrap();
        match (kontur, &t.kontur) {
            (Some(s), Some(i)) => assert!(
                gleiche_punkte(s, i),
                "{name}: Kontur weicht ab ({} statt {} Punkte)",
                i.len(),
                s.len()
            ),
            (None, None) => {}
            _ => panic!(
                "{name}: Kontur {:?} statt {:?}",
                t.kontur.is_some(),
                kontur.is_some()
            ),
        }
    }
}

#[test]
fn teile_ohne_zugabe_haben_keine_kontur() {
    // The rock has ten pieces with `seamAllowance="false"` and thirteen with an allowance.
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(weg()));
    h.run();
    let m = h.state().muster.as_ref().unwrap();
    let ohne = m.teile.iter().filter(|t| t.kontur.is_none()).count();
    assert!(ohne > 0, "kein Teil ohne Zugabe - falsches Muster");
    assert!(
        m.teile.iter().filter(|t| t.kontur.is_some()).count() > 0,
        "kein Teil mit Zugabe"
    );
    for (name, kontur) in soll_konturen(&weg()) {
        let Some(t) = m.teile.iter().find(|t| t.name == *name) else {
            continue;
        };
        assert_eq!(
            t.kontur.is_some(),
            kontur.is_some(),
            "{name}: Kontur vorhanden = {}",
            t.kontur.is_some()
        );
    }
}

#[test]
fn umschalten_zeichnet_und_entfernt_die_konturen() {
    let mut h = mit_bluse();
    assert!(h.state().ansicht.anzeige.zugabe, "Vorgabe: an");
    let mit = geschlossene_wege(&h);
    let anzahl = h
        .state()
        .muster
        .as_ref()
        .unwrap()
        .teile
        .iter()
        .filter(|t| t.kontur.is_some())
        .count();
    assert!(anzahl > 0, "kein Teil mit Zugabe");

    knopf(&h, "Zugabe").click();
    h.run();
    assert!(!h.state().ansicht.anzeige.zugabe);
    assert_eq!(mit - geschlossene_wege(&h), anzahl, "ein Weg je Kontur");

    knopf(&h, "Zugabe").click();
    h.run();
    assert!(h.state().ansicht.anzeige.zugabe);
    assert_eq!(geschlossene_wege(&h), mit);
}

#[test]
fn kuerzel_menue_und_knopf_schalten_dieselbe_zugabe() {
    let mut h = mit_bluse();
    assert!(h.state().ansicht.anzeige.zugabe);

    h.key_press_modifiers(Modifiers::SHIFT, Key::U);
    h.run();
    assert!(!h.state().ansicht.anzeige.zugabe, "Umschalt+U schaltet aus");

    menue_waehlen(&mut h, &["Anzeige", "2D-Schnitt"], "Zugabe Umschalt+U");
    menue_schliessen(&mut h, "Anzeige");
    assert!(h.state().ansicht.anzeige.zugabe, "Menüeintrag schaltet ein");

    knopf(&h, "Zugabe").click();
    h.run();
    assert!(!h.state().ansicht.anzeige.zugabe, "Knopf schaltet aus");

    // In the search field the key is text, not the shortcut.
    h.get_by_label("Suchen:").focus();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::U);
    h.run();
    assert!(!h.state().ansicht.anzeige.zugabe, "Umschalt+U im Textfeld");
    h.key_press(Key::Escape);
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::U);
    h.run();
    assert!(h.state().ansicht.anzeige.zugabe);
}

#[test]
fn zugabe_bleibt_gespeichert() {
    let mut h = mit_bluse();
    assert!(h.state().ansicht.anzeige.zugabe);
    knopf(&h, "Zugabe").click();
    h.run();
    let a = h.state().ansicht.anzeige;
    assert!(!a.zugabe);

    let mut speicher = Speicher::default();
    eframe::App::save(h.state_mut(), &mut speicher);
    let text = speicher.0[SCHLUESSEL_ANZEIGE2D].clone();
    assert!(text.contains("zugabe=0"), "{text}");
    drop(h);
    let mut h = Harness::builder()
        .with_size([1600.0, 1400.0])
        .build_eframe(|cc| {
            cc.storage = Some(&speicher);
            App::neu(cc)
        });
    h.run();
    assert_eq!(h.state().ansicht.anzeige, a);

    // A text stored before the toggle existed switches it on.
    let mut alt = Speicher::default();
    alt.0.insert(
        SCHLUESSEL_ANZEIGE2D.into(),
        "naehte=1;laengen=1;teilnamen=0;massstab=1;fadenlauf=0;passzeichen=0".into(),
    );
    let h = Harness::builder()
        .with_size([1600.0, 1400.0])
        .build_eframe(|cc| {
            cc.storage = Some(&alt);
            App::neu(cc)
        });
    let a = h.state().ansicht.anzeige;
    assert!(a.zugabe, "alter Text: Zugabe an");
    assert_eq!(Anzeige2d::aus_text(&a.als_text()), a);
}

#[test]
fn ohne_muster_gesperrt_mit_hinweis() {
    let mut h = fenster();
    h.ctx
        .global_style_mut(|s| s.interaction.tooltip_delay = 0.0);
    h.run();
    let k = knopf(&h, "Zugabe");
    assert!(k.accesskit_node().is_disabled(), "Zugabe nicht gesperrt");
    k.hover();
    h.run();
    assert!(hinweis_da(&h, ERST_MUSTER), "kein Hinweis");

    menue_oeffnen(&mut h, &["Anzeige", "2D-Schnitt"]);
    let k = im_menue(&h, "Zugabe Umschalt+U");
    assert!(
        k.accesskit_node().is_disabled(),
        "Menüeintrag nicht gesperrt"
    );
    k.hover();
    h.run();
    assert!(hinweis_da(&h, ERST_MUSTER), "Menü: kein Hinweis");
    menue_schliessen(&mut h, "Anzeige");
}
