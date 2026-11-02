//! The layout of the window, arranged the way CLO users know it.
//!
//! From the outside in: the menu bar and the file toolbar at the top, the status bar at the
//! bottom, the library ([`crate::bibliothek`]) at the left, the object browser above the
//! properties ([`crate::panele`]) at the right, and in the middle two windows side by side – the
//! 3D window at the left, the 2D window at the right. Both splits have a divider that can be
//! dragged (double click: back to the default); their shares are stored with the arrangement, so
//! they adapt to any window size. Every window has its own toolbar at its top and a thin
//! frame; the frame of the active window is highlighted, and a click into a window activates it.
//! Each part can be hidden through Anzeige › Fenster and Anzeige › Werkzeugleisten; a hidden
//! window leaves the full width to the other one.
//!
//! The side panels are capped at a share of the window ([`SEITE_ANTEIL`]) and scroll inside, so
//! the windows in the middle keep their room also at the minimum window size. While the 3D window
//! is a placeholder the 2D window has the larger minimum width ([`MIN_2D`]); see
//! [`teilung_breiten`].
//!
//! Every area is also named for assistive technology (role "pane"): "Bibliothek",
//! "Objekt-Browser", "Eigenschaften", "3D-Fenster", "2D-Fenster". The name sits on a widget that
//! is laid out before the area's content, so it never catches a click meant for the content.

use eframe::egui;
use egui::{
    pos2, vec2, Align, Layout, Rect, Role, Sense, Stroke, StrokeKind, UiBuilder, WidgetInfo,
};

use crate::anordnung::{Fenster, TEILUNG_RECHTS_VORGABE, TEILUNG_VORGABE};
use crate::ansicht2d::Klick;
use crate::app::{Aktion, App};

/// Widest share of the window the library (left) and the object browser with the properties
/// (right) may take. At the minimum window size (800) this leaves the middle 800 · 0.56 = 448 –
/// room for the 2D window's minimum and a narrow 3D window. Wider content scrolls in the panel.
pub const SEITE_ANTEIL: (f32, f32) = (0.17, 0.27);
/// Default width of the library.
pub const BIBLIOTHEK_BREITE: f32 = 240.0;
/// Default width of the object browser and the properties.
pub const RECHTS_BREITE: f32 = 380.0;
/// Narrowest the 3D window gets while both windows are shown and there is room for both minima.
pub const MIN_3D: f32 = 120.0;
/// Width of a window's frame; the content lies inside it.
pub const RAHMEN: f32 = 2.0;
/// Narrowest the 2D window gets while both windows are shown and there is room for both minima:
/// 300 for the view inside the frame. Larger than [`MIN_3D`]: the 2D window holds the only
/// working view so far.
pub const MIN_2D: f32 = 300.0 + 2.0 * RAHMEN;
/// Width of the divider between the 3D and the 2D window, and between the object browser and
/// the properties.
pub const TRENNER: f32 = 6.0;
/// Lowest the object browser and the properties get while both are shown (as far as there is
/// room for both).
pub const MIN_RECHTS: (f32, f32) = (140.0, 90.0);

/// The widths of the 3D and the 2D window in a middle `gesamt` wide, with the 3D window taking
/// the share `teilung` of the room beside the divider. With room for both minima each window
/// keeps its minimum (the 2D window first); with less room the share is kept.
pub fn teilung_breiten(gesamt: f32, teilung: f32) -> (f32, f32) {
    let frei = (gesamt - TRENNER).max(0.0);
    let teilung = if teilung.is_finite() {
        teilung.clamp(0.0, 1.0)
    } else {
        TEILUNG_VORGABE
    };
    let b3 = if frei >= MIN_3D + MIN_2D {
        (frei * teilung).clamp(MIN_3D, frei - MIN_2D)
    } else {
        frei * teilung
    };
    (b3, frei - b3)
}

/// The heights of the object browser and the properties in a right panel `gesamt` high, with
/// the browser taking the share `teilung` of the room beside the divider; each keeps its minimum
/// ([`MIN_RECHTS`]) as far as there is room for both.
pub fn teilung_rechts(gesamt: f32, teilung: f32) -> (f32, f32) {
    let frei = (gesamt - TRENNER).max(0.0);
    let teilung = if teilung.is_finite() {
        teilung.clamp(0.0, 1.0)
    } else {
        TEILUNG_RECHTS_VORGABE
    };
    let oben = if frei >= MIN_RECHTS.0 + MIN_RECHTS.1 {
        (frei * teilung).clamp(MIN_RECHTS.0, frei - MIN_RECHTS.1)
    } else {
        frei * teilung
    };
    (oben, frei - oben)
}

/// Name the area `rect` for assistive technology: a hover-only widget laid out before the content.
fn flaeche(ui: &mut egui::Ui, rect: Rect, name: &str) {
    let r = ui.interact(rect, ui.scope_id().with(("flaeche", name)), Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(Role::Pane, true, name));
}

/// Lay out the whole window.
pub fn zeichnen(app: &mut App, ui: &mut egui::Ui) {
    egui::Panel::top("menueleiste").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| crate::menue::menues(app, ui));
    });
    if app.anordnung.leiste_datei {
        egui::Panel::top("werkzeugleiste").show(ui, |ui| crate::menue::werkzeugleiste(app, ui));
    }
    egui::Panel::bottom("statusleiste").show(ui, |ui| crate::panele::statusleiste(app, ui));
    let breite = ui.available_width();
    let (links_max, rechts_max) = (breite * SEITE_ANTEIL.0, breite * SEITE_ANTEIL.1);
    if app.anordnung.bibliothek {
        egui::Panel::left("bibliothek")
            .default_size(BIBLIOTHEK_BREITE)
            .size_range(160.0f32.min(links_max)..=links_max)
            .show(ui, |ui| {
                flaeche(ui, ui.max_rect(), "Bibliothek");
                crate::bibliothek::panel(app, ui);
            });
    }
    let (browser, eigenschaften) = (app.anordnung.browser, app.anordnung.eigenschaften);
    if browser || eigenschaften {
        egui::Panel::right("rechts")
            .default_size(RECHTS_BREITE)
            .size_range(200.0f32.min(rechts_max)..=rechts_max)
            .show(ui, |ui| rechts(app, ui, browser, eigenschaften));
    }
    // Filled like the panels: the toolbar rows and the divider lie on it (unfilled, the light
    // theme showed black there).
    let grund = egui::Frame::NONE.fill(ui.visuals().panel_fill);
    egui::CentralPanel::no_frame()
        .frame(grund)
        .show(ui, |ui| mitte(app, ui));
}

/// The right panel: object browser above, properties below, or one of them alone.
fn rechts(app: &mut App, ui: &mut egui::Ui, browser: bool, eigenschaften: bool) {
    let rect = ui.available_rect_before_wrap();
    let (oben, unten) = match (browser, eigenschaften) {
        (true, true) => {
            let (h1, _) = teilung_rechts(rect.height(), app.anordnung.teilung_rechts);
            let oben = Rect::from_min_size(rect.min, vec2(rect.width(), h1));
            let trenn = Rect::from_min_size(
                pos2(rect.left(), oben.bottom()),
                vec2(rect.width(), TRENNER),
            );
            trenner(app, ui, trenn, rect, Teilung::Rechts);
            (
                Some(oben),
                Some(Rect::from_min_max(
                    pos2(rect.left(), trenn.bottom()),
                    rect.max,
                )),
            )
        }
        (true, false) => (Some(rect), None),
        (false, true) => (None, Some(rect)),
        (false, false) => (None, None),
    };
    for (r, name) in [(oben, "Objekt-Browser"), (unten, "Eigenschaften")] {
        let Some(r) = r else {
            continue;
        };
        flaeche(ui, r, name);
        let mut kind = ui.new_child(
            UiBuilder::new()
                .id_salt(name)
                .max_rect(r)
                .layout(Layout::top_down(Align::Min)),
        );
        kind.shrink_clip_rect(r);
        if name == "Eigenschaften" {
            crate::panele::eigenschaften(app, &mut kind);
        } else {
            crate::panele::browser(app, &mut kind);
        }
    }
    ui.advance_cursor_after_rect(rect);
}

/// The middle: the 3D and the 2D window side by side, or one of them, or a hint.
fn mitte(app: &mut App, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    match (app.anordnung.fenster3d, app.anordnung.fenster2d) {
        (true, true) => {
            let (b3, _) = teilung_breiten(rect.width(), app.anordnung.teilung);
            let r3 = Rect::from_min_size(rect.min, vec2(b3, rect.height()));
            let trenn =
                Rect::from_min_size(pos2(r3.right(), rect.top()), vec2(TRENNER, rect.height()));
            let r2 = Rect::from_min_max(pos2(trenn.right(), rect.top()), rect.max);
            fenster(app, ui, r3, Fenster::Drei);
            trenner(app, ui, trenn, rect, Teilung::Mitte);
            fenster(app, ui, r2, Fenster::Zwei);
        }
        (true, false) => fenster(app, ui, rect, Fenster::Drei),
        (false, true) => fenster(app, ui, rect, Fenster::Zwei),
        (false, false) => {
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
                |ui| {
                    ui.weak(
                        "3D- und 2D-Fenster sind ausgeblendet – Anzeige › Fenster blendet sie \
                         wieder ein, Anzeige › Anordnung zurücksetzen zeigt alles.",
                    );
                },
            );
        }
    }
    ui.advance_cursor_after_rect(rect);
}

/// Which divider.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Teilung {
    /// Between the 3D and the 2D window (vertical line).
    Mitte,
    /// Between the object browser and the properties (horizontal line).
    Rechts,
}

/// A divider `r` inside `bereich`: drag to move it, double click for the default. The share moves
/// with the pointer directly (like a panel edge), not through an action.
fn trenner(app: &mut App, ui: &mut egui::Ui, r: Rect, bereich: Rect, art: Teilung) {
    let (id, name) = match art {
        Teilung::Mitte => ("trenner_3d_2d", "Teilung 3D | 2D"),
        Teilung::Rechts => ("trenner_rechts", "Teilung Objekt-Browser | Eigenschaften"),
    };
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click_and_drag());
    resp.widget_info(|| WidgetInfo::labeled(Role::Splitter, true, name));
    let aktiv = resp.hovered() || resp.dragged();
    if aktiv {
        ui.ctx().set_cursor_icon(match art {
            Teilung::Mitte => egui::CursorIcon::ResizeHorizontal,
            Teilung::Rechts => egui::CursorIcon::ResizeVertical,
        });
    }
    let (anfang, laenge) = match art {
        Teilung::Mitte => (bereich.left(), bereich.width()),
        Teilung::Rechts => (bereich.top(), bereich.height()),
    };
    let frei = laenge - TRENNER;
    let anteil = match art {
        Teilung::Mitte => &mut app.anordnung.teilung,
        Teilung::Rechts => &mut app.anordnung.teilung_rechts,
    };
    if resp.dragged() && frei > 0.0 {
        if let Some(p) = resp.interact_pointer_pos() {
            let zeiger = match art {
                Teilung::Mitte => p.x,
                Teilung::Rechts => p.y,
            };
            *anteil = ((zeiger - anfang - TRENNER / 2.0) / frei).clamp(0.0, 1.0);
        }
    }
    if resp.double_clicked() {
        *anteil = match art {
            Teilung::Mitte => TEILUNG_VORGABE,
            Teilung::Rechts => TEILUNG_RECHTS_VORGABE,
        };
    }
    let strich = if aktiv {
        ui.visuals().widgets.hovered.fg_stroke
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    };
    match art {
        Teilung::Mitte => ui.painter().vline(r.center().x, r.y_range(), strich),
        Teilung::Rechts => ui.painter().hline(r.x_range(), r.center().y, strich),
    };
}

/// One of the two windows in `rect`: frame, toolbar, content; a press inside activates it.
fn fenster(app: &mut App, ui: &mut egui::Ui, rect: Rect, f: Fenster) {
    let (name, kurz, leiste) = match f {
        Fenster::Drei => ("3D-Fenster", "3D", app.anordnung.leiste_3d),
        Fenster::Zwei => ("2D-Fenster", "2D", app.anordnung.leiste_2d),
    };
    // A press inside the window – on its content or its toolbar – activates it, unless a menu or
    // dialog lies over that point.
    let druck = ui.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.press_origin())
            .flatten()
    });
    if let Some(p) = druck {
        if rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id()) {
            app.aktion(Aktion::FensterAktivieren(f));
        }
    }
    flaeche(ui, rect, name);
    let innen = rect.shrink(RAHMEN);
    let mut kind = ui.new_child(
        UiBuilder::new()
            .id_salt(name)
            .max_rect(innen)
            .layout(Layout::top_down(Align::Min)),
    );
    kind.shrink_clip_rect(innen);
    if leiste {
        // Wrapped, not cut: in a narrow window every button stays reachable.
        kind.horizontal_wrapped(|ui| {
            ui.strong(kurz);
            ui.separator();
            match f {
                Fenster::Drei => crate::menue::werkzeugleiste_3d(ui),
                Fenster::Zwei => crate::menue::werkzeugleiste_2d(app, ui),
            }
        });
        kind.separator();
    }
    match f {
        Fenster::Drei => ansicht_3d(&mut kind),
        Fenster::Zwei => {
            let klick = app.ansicht.ui(
                &mut kind,
                app.muster.as_ref(),
                app.gewaehlt,
                app.naht_gewaehlt,
            );
            match klick {
                Some(Klick::Teil(i)) => app.aktion(Aktion::TeilWaehlen(i)),
                Some(Klick::Leer) => app.aktion(Aktion::AuswahlAufheben),
                None => {}
            }
        }
    }
    let v = ui.visuals();
    let rahmen = if app.aktiv == f {
        Stroke::new(2.0, v.selection.bg_fill)
    } else {
        v.widgets.noninteractive.bg_stroke
    };
    ui.painter()
        .rect_stroke(rect, 0.0, rahmen, StrokeKind::Inside);
}

/// The 3D window's content until the 3D view exists: a calm placeholder.
fn ansicht_3d(ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(Role::Canvas, true, "3D-Ansicht"));
    let maler = ui.painter_at(rect);
    maler.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let farbe = ui.visuals().weak_text_color();
    let text = maler.layout(
        "3D-Ansicht folgt – Körper und Simulation kommen in einer späteren Aufgabe".into(),
        egui::FontId::proportional(14.0),
        farbe,
        (rect.width() - 24.0).max(40.0),
    );
    maler.galley(rect.center() - text.size() / 2.0, text, farbe);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teilung_mit_mindestbreiten() {
        // Plenty of room: the share as set.
        let (a, b) = teilung_breiten(1006.0, 0.5);
        assert_eq!((a, b), (500.0, 500.0));
        // The 2D window keeps its minimum, then the 3D window.
        let (a, b) = teilung_breiten(TRENNER + 600.0, 0.9);
        assert_eq!((a, b), (600.0 - MIN_2D, MIN_2D));
        let (a, b) = teilung_breiten(TRENNER + 600.0, 0.0);
        assert_eq!((a, b), (MIN_3D, 600.0 - MIN_3D));
        // The middle at the minimum window size with all panels at their caps.
        let mitte = 800.0 * (1.0 - SEITE_ANTEIL.0 - SEITE_ANTEIL.1);
        let (a, b) = teilung_breiten(mitte, TEILUNG_VORGABE);
        assert!(b >= MIN_2D && a >= MIN_3D, "{a} | {b} in {mitte}");
        // Too little room for both minima: the share is kept; nonsense means the default.
        assert_eq!(teilung_breiten(206.0, 0.25), (50.0, 150.0));
        assert_eq!(teilung_breiten(206.0, f32::NAN), (100.0, 100.0));
        assert_eq!(teilung_breiten(0.0, 0.5), (0.0, 0.0));
    }

    #[test]
    fn teilung_rechts_mit_mindesthoehen() {
        let (a, b) = teilung_rechts(TRENNER + 1000.0, 0.6);
        assert!((a - 600.0).abs() < 1e-3 && (b - 400.0).abs() < 1e-3);
        // The properties keep their minimum, then the browser.
        assert_eq!(teilung_rechts(TRENNER + 400.0, 1.0), (310.0, MIN_RECHTS.1));
        assert_eq!(teilung_rechts(TRENNER + 400.0, 0.0), (MIN_RECHTS.0, 260.0));
        assert_eq!(teilung_rechts(TRENNER + 100.0, 0.5), (50.0, 50.0));
    }
}
