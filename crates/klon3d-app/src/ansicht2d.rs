//! The 2D view: all pieces of a pattern, filled, with labels, seams and a scale bar.
//!
//! ## Coordinates
//!
//! Pattern coordinates ("world") are in the pattern's unit with y pointing down, like the pattern
//! file and like the screen – otherwise the pattern would stand on its head. The two meet only in
//! [`Sicht`]: screen = (world − `ursprung`) · `zoom`, relative to the top-left corner of the view.
//! Everything that maps, fits, zooms or hits is a plain function on [`Sicht`] or a free function
//! here, unit-tested without egui; [`Ansicht2d::ui`] only paints and reads the pointer.
//!
//! ## Operation
//!
//! Drag to pan, scroll wheel (or pinch, Ctrl+wheel) to zoom about the cursor, click to select a
//! piece (point in polygon on the drawn position, topmost piece wins), click on empty space to
//! clear the selection. Fitting and the layout switch come from the window (menu, toolbar, keys).
//!
//! ## Seams
//!
//! With "Nähte" on, boundary edges used by the seam list are drawn thick: in the seam colour, in
//! the warning colour when the seam is conspicuous (|Δ| > `TOLERANZ_CM` or a side without length),
//! in the error colour when two seams use the same edge. A seam selected in the seam list is always
//! highlighted.
//!
//! ## Display toggles
//!
//! [`Anzeige2d`] switches the seams, the piece names, the scale bar and the edge lengths. With
//! "Teilnamen" on, a piece shows its name where the name fits into it; the selected piece always
//! shows its name (above the piece on a backdrop when it does not fit), and the piece under the
//! pointer names itself in a hover text – so the toggle has a visible effect also when the 2D
//! window is narrow. With "Längen" on, every boundary edge of the selected piece – of all pieces
//! when none is selected – carries its length in cm at its middle ([`halbweg`]), on a backdrop; an
//! edge shorter on screen than its text gets none, nor does one whose label would cover a name or
//! another length, so the labels never pile up – zooming in shows the rest.

use std::sync::Arc;

use eframe::egui;
use egui::{Align2, Color32, FontId, Galley, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use klon3d_pattern::geom::Pt;

use crate::anordnung::Anzeige2d;
use crate::muster::{Huelle, Kantenart, Muster, Teil};

/// Free margin when fitting, in screen points.
pub const EINPASS_RAND: f64 = 24.0;

/// How far one can zoom out and in, relative to the zoom that fits the whole pattern.
pub const ZOOM_SPANNE: (f64, f64) = (1.0 / 20.0, 1000.0);

/// Width of the band around the selected piece, in screen points; wider than the seam strokes.
pub const AUSWAHL_BREITE: f32 = 6.0;

/// Longest scale bar in screen points.
pub const MASSSTAB_MAX_PX: f64 = 160.0;

/// Gap in screen points between the top of the selected piece and its name placed above it:
/// clear of the selection band and of a length label on a top edge.
const NAME_ABSTAND: f32 = 10.0;

/// A piece name placed in the view: where, the text, its colour and whether it has a backdrop.
struct Name {
    feld: Rect,
    galley: Arc<Galley>,
    farbe: Color32,
    hinterlegt: bool,
}

// ───────────────────────────────────────────────────────── Sicht

/// Mapping between pattern coordinates and screen points (relative to the view's top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sicht {
    /// Screen points per pattern unit.
    pub zoom: f64,
    /// Pattern coordinate at the top-left corner of the view.
    pub ursprung: Pt,
}

impl Sicht {
    /// World → screen (relative to the view's top-left).
    pub fn zu_bild(&self, p: Pt) -> (f64, f64) {
        (
            (p.x - self.ursprung.x) * self.zoom,
            (p.y - self.ursprung.y) * self.zoom,
        )
    }

    /// Screen (relative to the view's top-left) → world.
    pub fn zu_welt(&self, x: f64, y: f64) -> Pt {
        Pt::new(
            x / self.zoom + self.ursprung.x,
            y / self.zoom + self.ursprung.y,
        )
    }

    /// The view that shows `h` centred in a `breite` × `hoehe` area with `rand` free on every side.
    pub fn eingepasst(h: Huelle, breite: f64, hoehe: f64, rand: f64) -> Sicht {
        let bw = h.breite().max(1e-9);
        let bh = h.hoehe().max(1e-9);
        let frei_b = (breite - 2.0 * rand).max(1.0);
        let frei_h = (hoehe - 2.0 * rand).max(1.0);
        let zoom = (frei_b / bw).min(frei_h / bh);
        Sicht {
            zoom,
            ursprung: Pt::new(
                (h.x0 + h.x1) / 2.0 - breite / 2.0 / zoom,
                (h.y0 + h.y1) / 2.0 - hoehe / 2.0 / zoom,
            ),
        }
    }

    /// Zoom by `faktor` about the screen point (`x`, `y`), which stays where it is; the zoom is
    /// clamped to `grenzen`.
    pub fn zoom_um(&mut self, faktor: f64, x: f64, y: f64, grenzen: (f64, f64)) {
        if !faktor.is_finite() || faktor <= 0.0 {
            return;
        }
        let vorher = self.zu_welt(x, y);
        self.zoom = (self.zoom * faktor).clamp(grenzen.0, grenzen.1);
        let nachher = self.zu_welt(x, y);
        self.ursprung.x += vorher.x - nachher.x;
        self.ursprung.y += vorher.y - nachher.y;
    }

    /// Move the content by (`dx`, `dy`) screen points.
    pub fn schieben(&mut self, dx: f64, dy: f64) {
        self.ursprung.x -= dx / self.zoom;
        self.ursprung.y -= dy / self.zoom;
    }
}

/// Zoom limits for a view whose fitting zoom is `einpass_zoom`.
pub fn zoom_grenzen(einpass_zoom: f64) -> (f64, f64) {
    (einpass_zoom * ZOOM_SPANNE.0, einpass_zoom * ZOOM_SPANNE.1)
}

/// Is `p` inside the ring `punkte`? (Ray to the right, even-odd rule.)
pub fn innen(punkte: &[Pt], p: Pt) -> bool {
    let n = punkte.len();
    let mut drin = false;
    for i in 0..n {
        let (a, b) = (punkte[i], punkte[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let schnitt = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < schnitt {
                drin = !drin;
            }
        }
    }
    drin
}

/// The piece under the world point `p` as drawn; with overlaps the topmost (last drawn) one.
pub fn treffer(teile: &[Teil], p: Pt, ausgelegt: bool) -> Option<usize> {
    (0..teile.len()).rev().find(|&i| {
        let t = &teile[i];
        let v = t.versatz(ausgelegt);
        let q = Pt::new(p.x - v.x, p.y - v.y);
        let h = t.huelle;
        q.x >= h.x0 && q.x <= h.x1 && q.y >= h.y0 && q.y <= h.y1 && innen(&t.umriss, q)
    })
}

/// Length of the scale bar in cm: the longest of 1, 2, 5 · 10ⁿ cm that is at most `max_px` long.
pub fn massstab_cm(px_pro_cm: f64, max_px: f64) -> f64 {
    if !(px_pro_cm > 0.0) || !px_pro_cm.is_finite() {
        return 0.0;
    }
    let roh = max_px / px_pro_cm;
    let zehner = 10f64.powf(roh.log10().floor());
    [5.0, 2.0, 1.0]
        .into_iter()
        .map(|f| f * zehner)
        .find(|&l| l <= roh * (1.0 + 1e-12))
        .unwrap_or(zehner)
}

/// The point halfway along the polyline `punkte` and the unit direction of the segment it lies
/// on; `None` without length.
pub fn halbweg(punkte: &[Pt]) -> Option<(Pt, Pt)> {
    let gesamt: f64 = punkte.windows(2).map(|s| s[0].dist(s[1])).sum();
    if !(gesamt > 0.0) {
        return None;
    }
    let mut rest = gesamt / 2.0;
    for s in punkte.windows(2) {
        let l = s[0].dist(s[1]);
        if l > 0.0 && rest <= l {
            let (dx, dy) = ((s[1].x - s[0].x) / l, (s[1].y - s[0].y) / l);
            return Some((
                Pt::new(s[0].x + dx * rest, s[0].y + dy * rest),
                Pt::new(dx, dy),
            ));
        }
        rest -= l;
    }
    None
}

/// Length of the polyline `punkte`.
pub fn linienlaenge(punkte: &[Pt]) -> f64 {
    punkte.windows(2).map(|s| s[0].dist(s[1])).sum()
}

/// An edge length in the view: cm with one decimal and a German decimal comma ("12,3").
pub fn laenge_text(cm: f64) -> String {
    format!("{cm:.1}").replace('.', ",")
}

/// Text for a scale bar length: "10 cm", "0,5 cm", "2 m".
pub fn massstab_text(cm: f64) -> String {
    if cm >= 100.0 {
        format!("{} m", zahl_kurz(cm / 100.0))
    } else if cm < 0.1 {
        format!("{} mm", zahl_kurz(cm * 10.0))
    } else {
        format!("{} cm", zahl_kurz(cm))
    }
}

/// A short number with a German decimal comma, without trailing zeros.
fn zahl_kurz(w: f64) -> String {
    let s = format!("{w:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

/// `feld` moved into `flaeche` as far as it fits (a field wider than `flaeche` keeps its left
/// edge at the left of `flaeche`).
pub fn in_die_flaeche(feld: Rect, flaeche: Rect) -> Rect {
    let dx = (flaeche.right() - feld.right()).min(0.0);
    let dy = (flaeche.bottom() - feld.bottom()).min(0.0);
    let f = feld.translate(Vec2::new(dx, dy));
    let dx = (flaeche.left() - f.left()).max(0.0);
    let dy = (flaeche.top() - f.top()).max(0.0);
    f.translate(Vec2::new(dx, dy))
}

/// The backdrop of labels in the view: the view's background, slightly transparent.
fn hinterlegt(v: &egui::Visuals) -> Color32 {
    let g = v.extreme_bg_color;
    Color32::from_rgba_unmultiplied(g.r(), g.g(), g.b(), 210)
}

// ───────────────────────────────────────────────────────── Ansicht

/// What a click in the view did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Klick {
    /// A piece was clicked.
    Teil(usize),
    /// Empty space was clicked.
    Leer,
}

/// State of the 2D view.
#[derive(Debug, Clone)]
pub struct Ansicht2d {
    /// `None` until the first frame with a pattern fits it.
    pub sicht: Option<Sicht>,
    /// Zoom that fits the whole pattern; the zoom limits refer to it.
    pub einpass_zoom: f64,
    /// Pieces laid out side by side (`true`) or in construction coordinates.
    pub ausgelegt: bool,
    /// Seams, edge lengths, piece names, scale bar.
    pub anzeige: Anzeige2d,
    einpassen_bitte: bool,
    /// Fit again when the view changes its size – until the user pans or zooms. The first
    /// frames do not have the final size yet (side panels settle), and a window resize should
    /// keep a fitted pattern fitted.
    folgt_groesse: bool,
    /// Size of the view in the last frame.
    pub groesse: Vec2,
}

impl Default for Ansicht2d {
    fn default() -> Self {
        Ansicht2d {
            sicht: None,
            einpass_zoom: 1.0,
            ausgelegt: true,
            anzeige: Anzeige2d::default(),
            einpassen_bitte: true,
            folgt_groesse: true,
            groesse: Vec2::ZERO,
        }
    }
}

/// Colours of the view, derived from the current theme.
struct Farben {
    flaeche: Color32,
    flaeche_gewaehlt: Color32,
    linie: Color32,
    linie_gewaehlt: Color32,
    naht: Color32,
    auffaellig: Color32,
    doppelt: Color32,
    naht_gewaehlt: Color32,
    text: Color32,
}

impl Farben {
    fn aus(v: &egui::Visuals) -> Farben {
        if v.dark_mode {
            Farben {
                flaeche: Color32::from_rgba_unmultiplied(80, 120, 170, 70),
                // Clearly lighter than `flaeche` on the dark background (the selection colour
                // at low alpha mixes to almost the same dark tone).
                flaeche_gewaehlt: Color32::from_rgba_unmultiplied(90, 160, 220, 150),
                linie: Color32::from_rgb(150, 180, 215),
                linie_gewaehlt: Color32::from_rgb(235, 240, 255),
                naht: Color32::from_rgb(90, 200, 130),
                auffaellig: v.warn_fg_color,
                doppelt: v.error_fg_color,
                naht_gewaehlt: Color32::from_rgb(255, 225, 90),
                text: v.strong_text_color(),
            }
        } else {
            let sel = v.selection.bg_fill;
            Farben {
                flaeche: Color32::from_rgba_unmultiplied(70, 110, 160, 45),
                flaeche_gewaehlt: Color32::from_rgba_unmultiplied(sel.r(), sel.g(), sel.b(), 140),
                linie: Color32::from_rgb(40, 70, 110),
                linie_gewaehlt: Color32::from_rgb(10, 20, 40),
                naht: Color32::from_rgb(20, 130, 60),
                auffaellig: v.warn_fg_color,
                doppelt: v.error_fg_color,
                naht_gewaehlt: Color32::from_rgb(175, 110, 0),
                text: v.strong_text_color(),
            }
        }
    }
}

impl Ansicht2d {
    /// Fit the whole pattern in the next frame.
    pub fn einpassen(&mut self) {
        self.einpassen_bitte = true;
    }

    /// Switch between layout and construction coordinates; fits again.
    pub fn lage_umschalten(&mut self) {
        self.ausgelegt = !self.ausgelegt;
        self.einpassen();
    }

    /// Paint the view into all of `ui`'s remaining space and handle the pointer.
    ///
    /// `gewaehlt` is the selected piece, `naht` the seam selected in the seam list (index into the
    /// seam report). Returns what a click did, if anything was clicked.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        muster: Option<&Muster>,
        gewaehlt: Option<usize>,
        naht: Option<usize>,
    ) -> Option<Klick> {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "2D-Ansicht"));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let groesse_neu = (self.groesse - rect.size()).length() > 0.5;
        self.groesse = rect.size();

        let Some(muster) = muster else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Kein Muster geladen – Datei › Öffnen… (Strg+O)\noder eine .sm2d-Datei ins Fenster ziehen",
                FontId::proportional(15.0),
                ui.visuals().weak_text_color(),
            );
            return None;
        };
        let Some(huelle) = muster.huelle(self.ausgelegt) else {
            return None;
        };
        let (breite, hoehe) = (rect.width() as f64, rect.height() as f64);
        if self.einpassen_bitte || self.sicht.is_none() || (groesse_neu && self.folgt_groesse) {
            let s = Sicht::eingepasst(huelle, breite, hoehe, EINPASS_RAND);
            self.einpass_zoom = s.zoom;
            self.sicht = Some(s);
            self.einpassen_bitte = false;
            self.folgt_groesse = true;
        }
        let grenzen = zoom_grenzen(self.einpass_zoom);
        let mut sicht = self.sicht.unwrap();

        // Pointer: pan, zoom, click.
        if resp.dragged() {
            let d = resp.drag_delta();
            if d != Vec2::ZERO {
                sicht.schieben(d.x as f64, d.y as f64);
                self.folgt_groesse = false;
            }
        }
        let mut klick = None;
        if let Some(maus) = resp.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let faktor = (scroll as f64 / 200.0).exp() * pinch as f64;
            if (faktor - 1.0).abs() > 1e-6 {
                let m = maus - rect.min;
                sicht.zoom_um(faktor, m.x as f64, m.y as f64, grenzen);
                self.folgt_groesse = false;
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let m = pos - rect.min;
                let welt = sicht.zu_welt(m.x as f64, m.y as f64);
                klick = Some(match treffer(&muster.teile, welt, self.ausgelegt) {
                    Some(i) => Klick::Teil(i),
                    None => Klick::Leer,
                });
            }
        }
        self.sicht = Some(sicht);
        // The piece under the pointer names itself – also where its name does not fit into it.
        if self.anzeige.teilnamen && !resp.dragged() {
            if let Some(maus) = resp.hover_pos() {
                let m = maus - rect.min;
                let welt = sicht.zu_welt(m.x as f64, m.y as f64);
                if let Some(t) =
                    treffer(&muster.teile, welt, self.ausgelegt).map(|i| &muster.teile[i])
                {
                    resp.clone().on_hover_text_at_pointer(&t.kurz);
                }
            }
        }

        let farben = Farben::aus(ui.visuals());
        let bild = |p: Pt, v: Pt| -> Pos2 {
            let (x, y) = sicht.zu_bild(Pt::new(p.x + v.x, p.y + v.y));
            rect.min + Vec2::new(x as f32, y as f32)
        };
        let auf_schirm = |t: &Teil, v: Pt| -> bool {
            let h = t.huelle.verschoben(v);
            let a = bild(Pt::new(h.x0, h.y0), Pt::new(0.0, 0.0));
            let b = bild(Pt::new(h.x1, h.y1), Pt::new(0.0, 0.0));
            Rect::from_two_pos(a, b).intersects(rect)
        };

        // Fill and outline.
        for (i, t) in muster.teile.iter().enumerate() {
            let v = t.versatz(self.ausgelegt);
            if !auf_schirm(t, v) {
                continue;
            }
            let ist_gewaehlt = gewaehlt == Some(i);
            let fuell = if ist_gewaehlt {
                farben.flaeche_gewaehlt
            } else {
                farben.flaeche
            };
            let mut mesh = Mesh::default();
            for &p in &t.fuellung.punkte {
                mesh.colored_vertex(bild(p, v), fuell);
            }
            for d in &t.fuellung.dreiecke {
                mesh.add_triangle(d[0], d[1], d[2]);
            }
            painter.add(Shape::mesh(mesh));
            let punkte: Vec<Pos2> = t.umriss.iter().map(|&p| bild(p, v)).collect();
            painter.add(Shape::closed_line(punkte, Stroke::new(1.0, farben.linie)));
        }

        // The selected piece's outline as a wide band over all fills (pieces overlap in
        // construction coordinates) but under the seam strokes, so it shows on both sides of
        // every seam and the seam colours stay readable.
        if let Some(t) = gewaehlt.and_then(|i| muster.teile.get(i)) {
            let v = t.versatz(self.ausgelegt);
            let punkte: Vec<Pos2> = t.umriss.iter().map(|&p| bild(p, v)).collect();
            painter.add(Shape::closed_line(
                punkte,
                Stroke::new(AUSWAHL_BREITE, farben.linie_gewaehlt),
            ));
        }

        // Seam edges.
        let gewaehlte_naht_kanten = naht.map(|n| muster.kanten_der_naht(n)).unwrap_or_default();
        for (i, t) in muster.teile.iter().enumerate() {
            let Ok(rand) = &t.rand else {
                continue;
            };
            let v = t.versatz(self.ausgelegt);
            if !auf_schirm(t, v) {
                continue;
            }
            for (k, kante) in rand.kanten.iter().enumerate() {
                let hervor = gewaehlte_naht_kanten.contains(&(i, k));
                let strich = if hervor {
                    Stroke::new(4.0, farben.naht_gewaehlt)
                } else if self.anzeige.naehte {
                    match muster.kantenart(i, k) {
                        Kantenart::Ohne => continue,
                        Kantenart::Genaeht => Stroke::new(2.5, farben.naht),
                        Kantenart::Auffaellig => Stroke::new(3.5, farben.auffaellig),
                        Kantenart::Doppelt => Stroke::new(3.5, farben.doppelt),
                    }
                } else {
                    continue;
                };
                let punkte: Vec<Pos2> = kante.punkte.iter().map(|&p| bild(p, v)).collect();
                painter.add(Shape::line(punkte, strich));
            }
        }

        // Names before the lengths: a length label that would cover a name is left out.
        let namen = if self.anzeige.teilnamen {
            self.namen(
                &painter,
                muster,
                gewaehlt,
                &bild,
                &auf_schirm,
                sicht,
                rect,
                &farben,
            )
        } else {
            Vec::new()
        };
        let grund = hinterlegt(ui.visuals());
        if self.anzeige.laengen {
            let belegt: Vec<Rect> = namen.iter().map(|n| n.feld).collect();
            self.laengen(
                &painter,
                muster,
                gewaehlt,
                &bild,
                &auf_schirm,
                sicht,
                &farben,
                grund,
                belegt,
            );
        }
        // Names on top of everything.
        for n in namen {
            let pos = if n.hinterlegt {
                painter.rect_filled(n.feld, 3.0, grund);
                n.feld.center() - n.galley.size() / 2.0
            } else {
                n.feld.min
            };
            painter.galley(pos, n.galley, n.farbe);
        }

        if self.anzeige.massstab {
            self.massstab(&painter, rect, muster, sicht, ui.visuals());
        }
        klick
    }

    /// Where the piece names go. A name is shown where it fits into its piece (narrower than 90 %
    /// of the piece on screen), at the piece's label point. The selected piece's name is shown in
    /// any case – where it does not fit, above the piece on a backdrop, kept inside the view – and
    /// placed first; a name that would cover one already placed is left out.
    #[allow(clippy::too_many_arguments)]
    fn namen(
        &self,
        painter: &egui::Painter,
        muster: &Muster,
        gewaehlt: Option<usize>,
        bild: &dyn Fn(Pt, Pt) -> Pos2,
        auf_schirm: &dyn Fn(&Teil, Pt) -> bool,
        sicht: Sicht,
        rect: Rect,
        farben: &Farben,
    ) -> Vec<Name> {
        let schrift = FontId::proportional(13.0);
        let mut namen: Vec<Name> = Vec::new();
        let andere = (0..muster.teile.len()).filter(|&i| Some(i) != gewaehlt);
        for i in gewaehlt.into_iter().chain(andere) {
            let Some(t) = muster.teile.get(i) else {
                continue;
            };
            let v = t.versatz(self.ausgelegt);
            if !auf_schirm(t, v) {
                continue;
            }
            let ist_gewaehlt = gewaehlt == Some(i);
            let farbe = if ist_gewaehlt {
                farben.linie_gewaehlt
            } else {
                farben.text
            };
            let galley = painter.layout_no_wrap(t.kurz.clone(), schrift.clone(), farbe);
            let breite_px = t.huelle.breite() * sicht.zoom;
            let (feld, hinterlegt) = if (galley.size().x as f64) < breite_px * 0.9 {
                (
                    Rect::from_center_size(bild(t.etikett, v), galley.size()),
                    false,
                )
            } else if ist_gewaehlt {
                let h = t.huelle;
                let oben = bild(Pt::new((h.x0 + h.x1) / 2.0, h.y0), v);
                let feld = Rect::from_center_size(oben, galley.size()).expand2(Vec2::new(3.0, 1.0));
                let feld = feld.translate(Vec2::new(0.0, -(NAME_ABSTAND + feld.height() / 2.0)));
                (in_die_flaeche(feld, rect.shrink(2.0)), true)
            } else {
                continue;
            };
            if namen.iter().any(|n| n.feld.intersects(feld)) {
                continue;
            }
            namen.push(Name {
                feld,
                galley,
                farbe,
                hinterlegt,
            });
        }
        namen
    }

    /// The length of every boundary edge of the selected piece (of all pieces without a
    /// selection) at the edge's middle, on the backdrop `grund`; edges shorter on screen than
    /// their text, and labels that would cover one of `belegt` (the piece names) or a label
    /// already drawn, are left out.
    #[allow(clippy::too_many_arguments)]
    fn laengen(
        &self,
        painter: &egui::Painter,
        muster: &Muster,
        gewaehlt: Option<usize>,
        bild: &dyn Fn(Pt, Pt) -> Pos2,
        auf_schirm: &dyn Fn(&Teil, Pt) -> bool,
        sicht: Sicht,
        farben: &Farben,
        grund: Color32,
        mut belegt: Vec<Rect>,
    ) {
        let schrift = FontId::proportional(11.0);
        for (i, t) in muster.teile.iter().enumerate() {
            if gewaehlt.is_some_and(|g| g != i) {
                continue;
            }
            let Ok(rand) = &t.rand else {
                continue;
            };
            let v = t.versatz(self.ausgelegt);
            if !auf_schirm(t, v) {
                continue;
            }
            for kante in &rand.kanten {
                let Some((mitte, _)) = halbweg(&kante.punkte) else {
                    continue;
                };
                let galley = painter.layout_no_wrap(
                    laenge_text(rand.in_cm(kante.laenge)),
                    schrift.clone(),
                    farben.text,
                );
                let auf_dem_schirm = linienlaenge(&kante.punkte) * sicht.zoom;
                if auf_dem_schirm < galley.size().x as f64 + 6.0 {
                    continue;
                }
                let feld = Rect::from_center_size(bild(mitte, v), galley.size())
                    .expand2(Vec2::new(3.0, 1.0));
                if belegt.iter().any(|b| b.intersects(feld)) {
                    continue;
                }
                belegt.push(feld);
                painter.rect_filled(feld, 3.0, grund);
                painter.galley(feld.min + Vec2::new(3.0, 1.0), galley, farben.text);
            }
        }
    }

    /// Scale bar in the bottom-left corner.
    fn massstab(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        muster: &Muster,
        sicht: Sicht,
        v: &egui::Visuals,
    ) {
        let px_pro_cm = sicht.zoom * 10.0 / muster.mm_pro_einheit;
        let cm = massstab_cm(px_pro_cm, MASSSTAB_MAX_PX);
        if cm <= 0.0 {
            return;
        }
        let laenge = (cm * px_pro_cm) as f32;
        let farbe = v.text_color();
        let links = rect.left_bottom() + Vec2::new(16.0, -16.0);
        let rechts = links + Vec2::new(laenge, 0.0);
        let strich = Stroke::new(1.5, farbe);
        painter.line_segment([links, rechts], strich);
        painter.line_segment([links, links - Vec2::new(0.0, 6.0)], strich);
        painter.line_segment([rechts, rechts - Vec2::new(0.0, 6.0)], strich);
        painter.text(
            links + Vec2::new(laenge / 2.0, -8.0),
            Align2::CENTER_BOTTOM,
            massstab_text(cm),
            FontId::proportional(12.0),
            farbe,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::muster::{etikett, fuellung};

    fn pt(x: f64, y: f64) -> Pt {
        Pt::new(x, y)
    }

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * (1.0 + a.abs().max(b.abs()))
    }

    fn quadrat(x: f64, y: f64, s: f64, auslage: Pt) -> Teil {
        let umriss = vec![pt(x, y), pt(x + s, y), pt(x + s, y + s), pt(x, y + s)];
        Teil {
            name: format!("Q {x} {y}"),
            kurz: String::new(),
            schluessel: String::new(),
            fuellung: fuellung(&umriss),
            huelle: Huelle::um(&umriss),
            auslage,
            etikett: etikett(&umriss),
            knoten: 4,
            umfang_cm: None,
            rand: Err("ohne".into()),
            kanten_naehte: Vec::new(),
            umriss,
        }
    }

    #[test]
    fn hin_und_zurueck() {
        let s = Sicht {
            zoom: 2.5,
            ursprung: pt(-10.0, 4.0),
        };
        let p = pt(3.25, -7.5);
        let (x, y) = s.zu_bild(p);
        let q = s.zu_welt(x, y);
        assert!(nah(p.x, q.x) && nah(p.y, q.y));
        assert_eq!(s.zu_bild(pt(-10.0, 4.0)), (0.0, 0.0));
    }

    #[test]
    fn einpassen_zentriert_mit_rand() {
        let h = Huelle {
            x0: 10.0,
            x1: 110.0,
            y0: -20.0,
            y1: 30.0,
        };
        // 100 × 50 into 424 × 424 with margin 12: the width limits, zoom 4.
        let s = Sicht::eingepasst(h, 424.0, 424.0, 12.0);
        assert!(nah(s.zoom, 4.0));
        let (x0, y0) = s.zu_bild(pt(h.x0, h.y0));
        let (x1, y1) = s.zu_bild(pt(h.x1, h.y1));
        assert!(nah(x0, 12.0) && nah(x1, 412.0));
        // Centred vertically: 200 high in 424.
        assert!(nah(y0, 112.0) && nah(y1, 312.0));
        // Degenerate box and tiny area do not divide by zero.
        let p = Huelle {
            x0: 1.0,
            x1: 1.0,
            y0: 1.0,
            y1: 1.0,
        };
        let s = Sicht::eingepasst(p, 0.0, 0.0, 24.0);
        assert!(s.zoom.is_finite() && s.ursprung.x.is_finite());
    }

    #[test]
    fn zoom_um_den_zeiger() {
        let mut s = Sicht {
            zoom: 2.0,
            ursprung: pt(5.0, 5.0),
        };
        let unter = s.zu_welt(100.0, 60.0);
        s.zoom_um(1.5, 100.0, 60.0, (0.1, 100.0));
        assert!(nah(s.zoom, 3.0));
        let danach = s.zu_welt(100.0, 60.0);
        assert!(nah(unter.x, danach.x) && nah(unter.y, danach.y));
        // Clamped at the limit, the point still stays.
        s.zoom_um(1000.0, 100.0, 60.0, (0.1, 10.0));
        assert!(nah(s.zoom, 10.0));
        let danach = s.zu_welt(100.0, 60.0);
        assert!(nah(unter.x, danach.x) && nah(unter.y, danach.y));
        // Nonsense factors change nothing.
        let vorher = s;
        s.zoom_um(f64::NAN, 1.0, 1.0, (0.1, 10.0));
        s.zoom_um(0.0, 1.0, 1.0, (0.1, 10.0));
        assert_eq!(s, vorher);
    }

    #[test]
    fn schieben_folgt_der_maus() {
        let mut s = Sicht {
            zoom: 4.0,
            ursprung: pt(0.0, 0.0),
        };
        let p = pt(10.0, 10.0);
        let (x, y) = s.zu_bild(p);
        s.schieben(8.0, -4.0);
        let (x2, y2) = s.zu_bild(p);
        assert!(nah(x2 - x, 8.0) && nah(y2 - y, -4.0));
    }

    #[test]
    fn treffer_mit_und_ohne_auslage() {
        let teile = vec![
            quadrat(0.0, 0.0, 10.0, pt(0.0, 0.0)),
            // Overlaps the first one in construction coordinates, moved away in the layout.
            quadrat(5.0, 5.0, 10.0, pt(20.0, 0.0)),
        ];
        assert_eq!(treffer(&teile, pt(2.0, 2.0), false), Some(0));
        // In the overlap the last drawn piece wins.
        assert_eq!(treffer(&teile, pt(7.0, 7.0), false), Some(1));
        assert_eq!(treffer(&teile, pt(7.0, 7.0), true), Some(0));
        assert_eq!(treffer(&teile, pt(27.0, 7.0), true), Some(1));
        assert_eq!(treffer(&teile, pt(50.0, 50.0), true), None);
    }

    #[test]
    fn innen_bei_konkaven_teilen() {
        let u = [
            pt(0.0, 0.0),
            pt(3.0, 0.0),
            pt(3.0, 3.0),
            pt(2.0, 3.0),
            pt(2.0, 1.0),
            pt(1.0, 1.0),
            pt(1.0, 3.0),
            pt(0.0, 3.0),
        ];
        assert!(innen(&u, pt(0.5, 2.5)));
        assert!(innen(&u, pt(1.5, 0.5)));
        assert!(!innen(&u, pt(1.5, 2.0)));
        assert!(!innen(&u, pt(-1.0, 1.0)));
    }

    /// Relative luminance of an sRGB colour (WCAG 2).
    fn leuchtdichte(c: Color32) -> f64 {
        let lin = |k: u8| {
            let k = k as f64 / 255.0;
            if k <= 0.04045 {
                k / 12.92
            } else {
                ((k + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    #[test]
    fn gewaehlte_flaeche_hebt_sich_im_dunkeln_ab() {
        // The fills are drawn over the view background; the selected one must stand out clearly
        // (it used to mix to almost the same dark tone: contrast 1.07).
        let v = egui::Visuals::dark();
        let f = Farben::aus(&v);
        let grund = v.extreme_bg_color;
        let auf_grund = |c: Color32| {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            let a = a as f64 / 255.0;
            let mix = |x: u8, y: u8| (x as f64 * a + y as f64 * (1.0 - a)).round() as u8;
            Color32::from_rgb(mix(r, grund.r()), mix(g, grund.g()), mix(b, grund.b()))
        };
        let (n, s) = (
            leuchtdichte(auf_grund(f.flaeche)),
            leuchtdichte(auf_grund(f.flaeche_gewaehlt)),
        );
        let kontrast = (s.max(n) + 0.05) / (s.min(n) + 0.05);
        assert!(kontrast >= 1.8, "Kontrast {kontrast:.2}");
    }

    #[test]
    fn halbweg_entlang_der_linie() {
        // An L: 10 along x, then 6 along y; halfway (8) lies on the first leg.
        let l = [pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 6.0)];
        let (p, r) = halbweg(&l).unwrap();
        assert!(nah(p.x, 8.0) && nah(p.y, 0.0));
        assert!(nah(r.x, 1.0) && nah(r.y, 0.0));
        assert!(nah(linienlaenge(&l), 16.0));
        // Repeated points do not count; on the second leg the direction turns.
        let l = [pt(0.0, 0.0), pt(0.0, 0.0), pt(2.0, 0.0), pt(2.0, 8.0)];
        let (p, r) = halbweg(&l).unwrap();
        assert!(nah(p.x, 2.0) && nah(p.y, 3.0) && nah(r.y, 1.0));
        assert!(halbweg(&[pt(1.0, 1.0), pt(1.0, 1.0)]).is_none());
        assert!(halbweg(&[]).is_none());
        assert_eq!(laenge_text(12.345), "12,3");
    }

    #[test]
    fn feld_in_die_flaeche_schieben() {
        let f = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 50.0));
        let r = |x0, y0, x1, y1| Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1));
        // Inside: unchanged; over an edge: moved back in, size kept.
        assert_eq!(
            in_die_flaeche(r(10.0, 10.0, 30.0, 20.0), f),
            r(10.0, 10.0, 30.0, 20.0)
        );
        assert_eq!(
            in_die_flaeche(r(-5.0, -8.0, 15.0, 2.0), f),
            r(0.0, 0.0, 20.0, 10.0)
        );
        assert_eq!(
            in_die_flaeche(r(90.0, 45.0, 110.0, 55.0), f),
            r(80.0, 40.0, 100.0, 50.0)
        );
        // Wider than the area: its left edge at the left.
        assert_eq!(in_die_flaeche(r(50.0, 0.0, 250.0, 10.0), f).left(), 0.0);
    }

    #[test]
    fn massstab_waehlt_1_2_5() {
        // 4 px per cm, at most 160 px: 40 cm would fit, 50 does not → 20 cm.
        assert!(nah(massstab_cm(4.0, 160.0), 20.0));
        assert!(nah(massstab_cm(16.0, 160.0), 10.0));
        assert!(nah(massstab_cm(30.0, 160.0), 5.0));
        assert!(nah(massstab_cm(1000.0, 160.0), 0.1));
        assert_eq!(massstab_cm(0.0, 160.0), 0.0);
        assert_eq!(massstab_cm(f64::NAN, 160.0), 0.0);
        assert_eq!(massstab_text(20.0), "20 cm");
        assert_eq!(massstab_text(0.5), "0,5 cm");
        assert_eq!(massstab_text(200.0), "2 m");
        assert_eq!(massstab_text(0.05), "0,5 mm");
    }
}
