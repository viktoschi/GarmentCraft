//! Helpers shared by the UI tests: the test patterns, windows of the right size, an independent
//! evaluation of the Bluse3 v2, grid rows, clicks, menus and an in-memory storage.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use egui::{Modifiers, Role};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;
use klon3d_app::app::App;
use klon3d_pattern::geom::{lies_messungen, Auswertung};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::read_path;

pub fn muster_ordner() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

pub fn bluse() -> PathBuf {
    muster_ordner().join("bluse3_v2.sm2d")
}

/// A window large enough that the piece list and the seam list are laid out completely.
pub fn fenster() -> Harness<'static, App> {
    Harness::builder()
        .with_size([1600.0, 1400.0])
        .build_eframe(|cc| App::neu(cc))
}

pub fn mit_bluse() -> Harness<'static, App> {
    let mut h = fenster();
    assert!(h.state_mut().muster_laden(bluse()));
    h.run();
    h
}

/// Δ as `klon3d naehte` prints it (same expression as in `klon3d-cli`).
pub fn delta_wie_cli(d: f64) -> String {
    format!("{:+.4}", if d.abs() < 5e-5 { 0.0 } else { d })
}

/// The name after the last " - " (all Bluse3 pieces are called "Bluse 3 - …").
pub fn kurzname(name: &str) -> &str {
    name.rsplit_once(" - ").map_or(name, |(_, k)| k)
}

/// Independent evaluation of the Bluse3 v2.
pub struct Soll {
    /// Pieces with an outline: full name, circumference in cm, boundary edges as
    /// ("A* › B", length in cm).
    pub teile: Vec<(String, f64, Vec<(String, f64)>)>,
    /// Seams: Kennung, L_A, L_B, Δ.
    pub naehte: Vec<(String, f64, f64, f64)>,
}

pub fn soll_bluse() -> Soll {
    let muster = read_path(&bluse()).unwrap();
    let mess = lies_messungen(&muster_ordner().join("measurements.smis"), &muster.unit).unwrap();
    let ev = Auswertung::new(&muster, mess);
    let teile = muster
        .pieces()
        .into_iter()
        .filter(|p| ev.stueck_umriss(p).map(|u| u.len() >= 3).unwrap_or(false))
        .map(|p| {
            let r = ev.teilrand(p, Kurvenquelle::Seamly).unwrap();
            let kanten = r
                .kanten
                .iter()
                .map(|k| {
                    let (a, b) = (&r.punktknoten[k.von], &r.punktknoten[k.bis]);
                    let stern = |x: bool| if x { "*" } else { "" };
                    (
                        format!(
                            "{}{} › {}{}",
                            a.name,
                            stern(a.kerbe),
                            b.name,
                            stern(b.kerbe)
                        ),
                        r.in_cm(k.laenge),
                    )
                })
                .collect();
            (p.name.clone().unwrap(), r.in_cm(r.umfang()), kanten)
        })
        .collect();
    let text = std::fs::read_to_string(bluse().with_extension("naehte")).unwrap();
    let defs = lies_nahtliste(&text).unwrap();
    let bericht = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let naehte = bericht
        .naehte
        .iter()
        .map(|n| (n.kennung.clone(), n.a.laenge_cm, n.b.laenge_cm, n.diff_cm))
        .collect();
    Soll { teile, naehte }
}

/// The texts of the `n` cells right of the button `knopf` in its grid row.
pub fn zeile(h: &Harness<'_, App>, knopf: &str, n: usize) -> Vec<String> {
    zeile_nach(h, Role::Button, knopf, n)
}

/// The texts of the `n` cells right of the widget with `rolle` and `text` in its grid row.
pub fn zeile_nach(h: &Harness<'_, App>, rolle: Role, text: &str, n: usize) -> Vec<String> {
    let k = h.get_by_role_and_label(rolle, text);
    let a = k.accesskit_node();
    let eltern = a.parent().expect("Knopf ohne Eltern");
    let kinder: Vec<_> = eltern.children().collect();
    let i = kinder.iter().position(|c| c.id() == a.id()).unwrap();
    kinder[i + 1..i + 1 + n]
        .iter()
        .map(|c| c.value().or_else(|| c.label()).unwrap_or_default())
        .collect()
}

/// A primary click at a screen position.
pub fn klick_bei(h: &mut Harness<'_, App>, pos: egui::Pos2) {
    h.hover_at(pos);
    for gedrueckt in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: gedrueckt,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
}

/// Does `n` lie inside a menu (a `Ui` of kind menu among its ancestors)?
pub fn in_einem_menue(n: &egui_kittest::Node<'_>) -> bool {
    let mut a = n.accesskit_node().parent();
    while let Some(p) = a {
        if p.role() == Role::Menu {
            return true;
        }
        a = p.parent();
    }
    false
}

/// The button `name` of the menu bar – not a toolbar button of the same name ("Avatar") and not
/// a submenu of the same name in an open menu (those lie below the bar).
pub fn in_der_leiste<'t>(h: &'t Harness<'_, App>, name: &'t str) -> egui_kittest::Node<'t> {
    h.query_all_by_role_and_label(Role::Button, name)
        .filter(in_einem_menue)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap_or_else(|| panic!("„{name}“ nicht in der Menüleiste"))
}

/// The entry `name` of an open menu or submenu – not a button of the menu bar (some submenus
/// carry the name of a menu) and not a toolbar button of the same name. Where a submenu and an
/// entry in it share the name (Anzeige › Avatar › Avatar), the entry of the innermost menu – the
/// rightmost column – is meant; two of the same name in one menu are an error.
pub fn im_menue<'t>(h: &'t Harness<'_, App>, name: &'t str) -> egui_kittest::Node<'t> {
    let leiste = in_der_leiste(h, "Datei").rect().bottom();
    let mut treffer: Vec<_> = h
        .query_all_by_label(name)
        .filter(|n| in_einem_menue(n) && n.rect().top() >= leiste - 0.5)
        .collect();
    assert!(!treffer.is_empty(), "„{name}“ in keinem offenen Menü");
    treffer.sort_by(|a, b| a.rect().left().total_cmp(&b.rect().left()));
    let k = treffer.pop().unwrap();
    assert!(
        treffer
            .iter()
            .all(|n| n.rect().left() < k.rect().left() - 10.0),
        "„{name}“ mehrfach in einem offenen Menü"
    );
    k
}

/// Open menu `pfad[0]` of the menu bar and in it the submenus `pfad[1..]`.
pub fn menue_oeffnen(h: &mut Harness<'_, App>, pfad: &[&str]) {
    in_der_leiste(h, pfad[0]).click();
    h.run();
    for name in &pfad[1..] {
        im_menue(h, name).click();
        h.run();
    }
}

/// Close the open menu (with its submenus) by clicking its button in the menu bar again.
pub fn menue_schliessen(h: &mut Harness<'_, App>, menue: &str) {
    in_der_leiste(h, menue).click();
    h.run();
}

/// Open the menu at `pfad` and click its entry `eintrag`.
pub fn menue_waehlen(h: &mut Harness<'_, App>, pfad: &[&str], eintrag: &str) {
    menue_oeffnen(h, pfad);
    im_menue(h, eintrag).click();
    h.run();
}

/// A tiny in-memory storage for the persistence test.
#[derive(Default)]
pub struct Speicher(pub HashMap<String, String>);

impl eframe::Storage for Speicher {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

/// A fresh temporary folder for one test.
pub fn frischer_ordner(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("klon3d-app-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// File name of the pattern shown.
pub fn gezeigt(h: &Harness<'_, App>) -> Option<String> {
    h.state().muster.as_ref().map(|m| m.dateiname())
}
