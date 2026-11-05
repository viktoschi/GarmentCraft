//! The 3D view: body and garment, drawn with a wgpu paint callback on the window's own device.
//!
//! ## Why a paint callback
//!
//! egui has no 3D pipeline, and a second `wgpu` instance (as `klon3d-sim`'s GPU solver builds)
//! would be a second adapter and a second `Device`. [`egui_wgpu::Callback`] draws into the render
//! pass egui already uses: same device, same queue, same target texture. The GPU objects live in
//! [`egui_wgpu::CallbackResources`] (a `TypeMap` keyed by type), so they survive frames; the
//! callback value itself is rebuilt every frame and only carries the frame's data.
//!
//! ## What is drawn
//!
//! Flat-shaded triangles: one normal per face, no vertex normals, no depth buffer. Instead the
//! triangles are sorted back to front about the camera and drawn in that order (painter's
//! algorithm) – for a garment over a body that is enough, and it needs no depth texture that
//! egui would have to allocate. Each face normal is turned towards the camera, so open cloth
//! edges show their inside instead of a hole. The body is neutral grey, the cloth a wine red;
//! both stay readable on the dark and the light theme (the pane's background is egui's
//! [`egui::Visuals::extreme_bg_color`], painted by egui before the callback runs).
//!
//! The wireframe overlay ([`Ansicht3d::netz`]) is the cloth edges as line lists, drawn after the
//! triangles. Its colour follows the theme, because the background does.
//!
//! ## Camera
//!
//! Metres, **z up**, **front is −y** – the scene's coordinates (`klon3d-pipeline` turns CLO's
//! y-up into this). An orbit camera around [`Ansicht3d::ziel`]: drag with the left button
//! rotates, the middle button pans, the wheel zooms, a double click frames the scene. The
//! direction *from the target towards the eye* is `[cos(pitch)·sin(yaw), −cos(pitch)·cos(yaw),
//! sin(pitch)]`, so yaw 0 looks from the front. [`Blickwinkel`] sets the five standard views;
//! the pitch stops at [`PITCH_MAX`], because at exactly 90° the up vector would disappear.
//!
//! ## Multisampling
//!
//! [`wgpu::MultisampleState::count`] must equal the sample count of the pass egui renders into.
//! `egui-wgpu` does not expose that count, so this view fixes it at 1 ([`MSAA`]) – which is what
//! both paths use today: eframe passes `NativeOptions::multisampling` (default 0) through as
//! `msaa_samples` and the renderer takes `max(1)`; `egui_kittest` uses
//! `RendererOptions::PREDICTABLE` with `msaa_samples: 1`. An application that turns multisampling
//! on would have to pass the count in here as well.
//!
//! ## One view per window
//!
//! The GPU objects are one set per application (they live in the callback resources by type), so
//! they hold the data of the **last** callback prepared in a frame. With one 3D view per window
//! that is exactly its data; a second view would have to key its resources.

use std::collections::HashSet;
use std::sync::Arc;

use eframe::{egui, egui_wgpu, wgpu};

use crate::karten::{Karte, Kartenwerte, Skalen, KOMFORT_ABSTAND_MM};
use crate::simulation::{Bau, Schnappschuss};

/// Sample count of the pass (see the module documentation).
const MSAA: u32 = 1;

/// Vertical field of view, in radians.
const BLICKWINKEL: f64 = 40.0 * std::f64::consts::PI / 180.0;

/// Nearest and farthest plane, in metres.
const NAH: f64 = 0.02;
const FERN: f64 = 60.0;

/// Largest camera distance, in metres.
const ABSTAND_MAX: f64 = 30.0;

/// Upper limit of the pitch, in radians: at 90° the up vector would equal the view direction.
const PITCH_MAX: f64 = 85.0 * std::f64::consts::PI / 180.0;

/// Rotating speed, radians per point dragged.
const DREHEN: f64 = 0.01;
/// Panning speed, shares of the distance per point dragged.
const SCHIEBEN: f64 = 0.0015;
/// Zooming per wheel point, as a factor on the distance.
const ZOOM: f64 = 0.0015;

/// Body colour: neutral grey, readable on both themes.
const KOERPER_FARBE: [f32; 3] = [0.62, 0.63, 0.66];
/// Cloth colour. [`klon3d_sim::Material`] carries no colour, so the view picks one: a dark wine
/// red that stands out against the grey body on dark and on light.
const STOFF_FARBE: [f32; 3] = [0.66, 0.13, 0.20];

/// How much nearer than its true centre distance a cloth triangle counts for the sort (m).
///
/// Cloth rests about half a fabric thickness (0.2 mm) on the skin, which is far below the size of a
/// body triangle: without a bias a body triangle whose centre lies a millimetre in front of the
/// cloth's centre wins the painter's order and the grey speckles through the garment. Short enough
/// to stay below any real distance between the cloth and the body part it covers.
const STOFF_VORZUG: f64 = 0.003;

/// Floats per vertex: position, normal, colour.
const KNOTEN_FLOATS: usize = 9;

/// Floats of the uniform block: the 4×4 view-projection, then the light direction (padded).
const KOPF_FLOATS: usize = 20;

/// The five views of Anzeige › Blickwinkel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blickwinkel {
    Vorne,
    Hinten,
    Links,
    Rechts,
    Oben,
}

impl Blickwinkel {
    /// Name as the menu shows it.
    pub fn name(self) -> &'static str {
        match self {
            Blickwinkel::Vorne => "Vorne",
            Blickwinkel::Hinten => "Hinten",
            Blickwinkel::Links => "Links",
            Blickwinkel::Rechts => "Rechts",
            Blickwinkel::Oben => "Oben",
        }
    }

    /// (yaw, pitch) in radians – the direction **towards** the camera.
    pub fn winkel(self) -> (f64, f64) {
        use std::f64::consts::PI;
        match self {
            Blickwinkel::Vorne => (0.0, 0.0),
            Blickwinkel::Hinten => (PI, 0.0),
            Blickwinkel::Links => (-PI / 2.0, 0.0),
            Blickwinkel::Rechts => (PI / 2.0, 0.0),
            Blickwinkel::Oben => (0.0, PITCH_MAX),
        }
    }

    /// All five, in menu order.
    pub const ALLE: [Blickwinkel; 5] = [
        Blickwinkel::Vorne,
        Blickwinkel::Hinten,
        Blickwinkel::Links,
        Blickwinkel::Rechts,
        Blickwinkel::Oben,
    ];
}

/// The orbit camera and what is shown.
#[derive(Clone, Debug)]
pub struct Ansicht3d {
    /// Point the camera looks at (m).
    pub ziel: [f64; 3],
    /// Distance from the target (m).
    pub abstand: f64,
    /// Rotation about z (radians); 0 looks from the front (−y).
    pub yaw: f64,
    /// Elevation (radians), clamped to ±[`PITCH_MAX`].
    pub pitch: f64,
    /// Draw the cloth edges as a wireframe.
    pub netz: bool,
    /// Where the upper end of each map's colour scale sits.
    ///
    /// It belongs here rather than to the menu: the vertex data is rebuilt when this changes, and
    /// the cloth colours, the legend and the value box all read the same number from it.
    pub skalen: Skalen,
    /// The scene was framed; a later frame with content leaves the user's camera alone.
    eingepasst: bool,
    /// What the built vertex data belongs to.
    stand: Option<Stand>,
    /// The vertex data of `stand`, shared with the callback.
    daten: Option<Arc<Nutzlast>>,
}

/// The state the built vertex data belongs to; anything else means rebuilding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stand {
    bild: usize,
    yaw_milli: i64,
    pitch_milli: i64,
    abstand_milli: i64,
    ziel_milli: [i64; 3],
    netz: bool,
    thema_hell: bool,
    /// The map painted on the cloth; part of the state because it decides the vertex colours.
    ///
    /// Taken from the snapshot, not from the selection: the selection is a request, and the values
    /// for it arrive with the next snapshot. Colouring with the selection would paint the old
    /// numbers on the new ramp for one frame.
    karte: Option<Karte>,
    /// Upper end of that map's scale, in thousandths of its unit.
    ///
    /// Part of the state because a moved end changes the vertex colours: the numbers stay the same,
    /// but the data has to be rebuilt with the new ramp. Thousandths keep the comparison exact
    /// without carrying a float through an `Eq`, and are far finer than a legible step on the
    /// 64-step bar.
    bis_milli: i64,
}

impl Default for Ansicht3d {
    fn default() -> Self {
        Ansicht3d {
            ziel: [0.0, 0.0, 1.0],
            abstand: 3.0,
            yaw: 0.0,
            pitch: 0.08,
            netz: false,
            skalen: Skalen::default(),
            eingepasst: false,
            stand: None,
            daten: None,
        }
    }
}

impl Ansicht3d {
    /// Set one of the five standard views.
    pub fn blickwinkel(&mut self, b: Blickwinkel) {
        let (yaw, pitch) = b.winkel();
        self.yaw = yaw;
        self.pitch = pitch;
    }

    /// Is the camera at this standard view?
    pub fn ist_blickwinkel(&self, b: Blickwinkel) -> bool {
        let (yaw, pitch) = b.winkel();
        (yaw - self.yaw).abs() < 1e-6 && (pitch - self.pitch).abs() < 1e-6
    }

    /// Frame the whole scene on the next frame; the scene is only known inside [`Ansicht3d::ui`].
    pub fn einpassen_anfordern(&mut self) {
        self.eingepasst = false;
    }

    /// Back to the default camera; the next frame frames the scene again.
    ///
    /// The map scales survive: they are a reading setting with their own reset per map, and a
    /// camera reset that silently undid them would be a surprise.
    pub fn zuruecksetzen(&mut self) {
        let skalen = self.skalen;
        *self = Ansicht3d::default();
        self.skalen = skalen;
    }

    /// Target, distance and angles that show the whole scene: the middle of the box around body
    /// and cloth, far enough away for the box's bounding sphere.
    pub fn einpassen(&mut self, bau: &Bau, x: &[[f64; 3]]) {
        let (mitte, radius) = box_mitte_radius(bau, x);
        self.ziel = mitte;
        let radius = if radius.is_finite() && radius > 1e-6 {
            radius
        } else {
            0.5
        };
        self.abstand = (radius / (BLICKWINKEL / 2.0).tan() * 1.6).clamp(0.2, ABSTAND_MAX);
        self.eingepasst = true;
    }

    /// The camera position in scene coordinates.
    pub fn auge(&self) -> [f64; 3] {
        let r = self.richtung();
        [
            self.ziel[0] + r[0] * self.abstand,
            self.ziel[1] + r[1] * self.abstand,
            self.ziel[2] + r[2] * self.abstand,
        ]
    }

    /// Unit vector from the target towards the camera.
    pub fn richtung(&self) -> [f64; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [cp * sy, -cp * cy, sp]
    }

    /// Triangles of the last built data – 0 before the first frame.
    pub fn dreiecke(&self) -> usize {
        self.daten.as_ref().map_or(0, |d| d.dreiecke)
    }

    /// Whether the last build had a wireframe.
    pub fn segmente(&self) -> usize {
        self.daten.as_ref().map_or(0, |d| d.segmente)
    }

    /// The camera in short form for the status bar: `Vorne 2,4 m` or `frei 2,4 m`.
    pub fn kamera_text(&self) -> String {
        let nah = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let name = Blickwinkel::ALLE.iter().copied().find(|b| {
            let (yaw, pitch) = b.winkel();
            nah(yaw, self.yaw) && nah(pitch, self.pitch)
        });
        let art = name.map_or("frei", Blickwinkel::name);
        format!("{art} {:.1} m", self.abstand)
    }

    /// What the built data must match; a change means rebuilding it.
    fn stand(&self, bild: usize, thema_hell: bool, karte: Option<Karte>) -> Stand {
        let milli = |v: f64| (v * 1000.0).round() as i64;
        Stand {
            bild,
            yaw_milli: milli(self.yaw),
            pitch_milli: milli(self.pitch),
            abstand_milli: milli(self.abstand),
            ziel_milli: self.ziel.map(milli),
            netz: self.netz,
            thema_hell,
            karte,
            bis_milli: karte.map_or(0, |k| milli(self.skalen.bis(k))),
        }
    }

    /// The pane inside `ui`. `format` is the colour format of the window's target texture, `None`
    /// when the window renders without wgpu (the headless tests with the lazy renderer): then the
    /// pane keeps its role and label and shows why nothing is drawn.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        bau: Option<&Bau>,
        bild: Option<&Schnappschuss>,
        format: Option<wgpu::TextureFormat>,
    ) {
        let (rect, resp) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "3D-Ansicht"));
        let maler = ui.painter_at(rect);
        maler.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

        let hint = |text: &str, maler: &egui::Painter, farbe: egui::Color32| {
            let galerie = maler.layout(
                text.into(),
                egui::FontId::proportional(14.0),
                farbe,
                (rect.width() - 24.0).max(40.0),
            );
            maler.galley(rect.center() - galerie.size() / 2.0, galerie, farbe);
        };

        let Some((bau, bild)) = bau.zip(bild) else {
            hint(
                "3D-Ansicht – Muster öffnen, Körper und Stoff wählen, dann 3D-Kleidung › Simulation",
                &maler,
                ui.visuals().weak_text_color(),
            );
            return;
        };

        if !self.eingepasst {
            self.einpassen(bau, &bild.x);
        }
        self.steuern(ui, &resp);

        let Some(format) = format else {
            hint(
                "3D-Ansicht ohne Grafikkarte (wgpu nicht eingerichtet)",
                &maler,
                ui.visuals().weak_text_color(),
            );
            return;
        };

        // Only when the frame or the camera changed: the vertex data follows the newest snapshot,
        // not the solver's frames.
        let thema_hell = !ui.visuals().dark_mode;
        // The map the snapshot carries – not the one selected: the selection is a request, the
        // values arrive with the next snapshot.
        let karte = bild.karte.as_ref().map(|k| k.karte);
        let stand = self.stand(bild.bild, thema_hell, karte);
        if self.stand != Some(stand) {
            let netzfarbe = if thema_hell {
                [0.16, 0.16, 0.18]
            } else {
                [0.92, 0.90, 0.94]
            };
            self.daten = Some(Arc::new(bauen(
                bau,
                &bild.x,
                self,
                self.netz.then_some(netzfarbe),
                bild.karte.as_ref(),
            )));
            self.stand = Some(stand);
        }
        let Some(daten) = self.daten.clone() else {
            return;
        };
        if daten.dreiecke == 0 {
            return;
        }
        // `new_paint_callback` already builds the `PaintCallback` (rect + `Arc<dyn Any>`); wrapping
        // it in another `Arc` puts a `PaintCallback` where egui-wgpu expects its own `Callback`,
        // the downcast then fails and nothing is drawn.
        maler.add(egui_wgpu::Callback::new_paint_callback(
            rect,
            AnsichtCallback { daten, format },
        ));
        // After the callback: the shapes painted after it lie on top of the 3D image.
        if let Some((karte, werte)) = karten_werte(bild) {
            let bis = self.skalen.bis(karte);
            legende(ui, &maler, rect, karte, bis, werte, !bau.koerper.is_empty());
            if let Some(zeiger) = resp.hover_pos() {
                if let Some(wert) = wert_unter(rect, self, &bild.x, werte, zeiger) {
                    wert_hinweis(ui, &maler, rect, zeiger, karte, bis, wert);
                }
            }
        }
    }

    /// Drag rotates, middle drag pans, wheel zooms, double click frames.
    fn steuern(&mut self, ui: &egui::Ui, resp: &egui::Response) {
        if resp.double_clicked() {
            self.eingepasst = false;
            self.stand = None;
            return;
        }
        // `Response::drag_delta` reads the context's input itself; asking for it inside
        // `ui.input` would write the same lock again and block forever (a re-entrant RwLock).
        let delta = resp.drag_delta();
        let (scroll, ctrl) = ui.input(|i| (i.smooth_scroll_delta.y as f64, i.modifiers.ctrl));
        if resp.dragged_by(egui::PointerButton::Primary) && delta != egui::Vec2::ZERO {
            self.yaw -= delta.x as f64 * DREHEN;
            self.pitch = (self.pitch + delta.y as f64 * DREHEN).clamp(-PITCH_MAX, PITCH_MAX);
            let pi = std::f64::consts::PI;
            self.yaw = self.yaw.rem_euclid(2.0 * pi);
            if self.yaw > pi {
                self.yaw -= 2.0 * pi;
            }
        } else if resp.dragged_by(egui::PointerButton::Middle) && delta != egui::Vec2::ZERO {
            let r = self.richtung();
            // To the right in the image and up in the image plane.
            let laenge = (r[0] * r[0] + r[1] * r[1]).sqrt();
            if laenge > 1e-9 {
                let rechts = [r[1] / laenge, -r[0] / laenge, 0.0];
                let oben = [
                    r[1] * r[2] - r[2] * r[1],
                    r[2] * r[0] - r[0] * r[2],
                    r[0] * r[1] - r[1] * r[0],
                ];
                let s = self.abstand * SCHIEBEN;
                for i in 0..3 {
                    self.ziel[i] += (-delta.x as f64 * rechts[i] + delta.y as f64 * oben[i]) * s;
                }
            }
        }
        if resp.hovered() && scroll != 0.0 && !ctrl {
            let faktor = (1.0 + scroll * ZOOM).clamp(0.2, 5.0);
            self.abstand = (self.abstand * faktor).clamp(0.05, ABSTAND_MAX);
        }
    }
}

/// Middle and radius of the bounding sphere around body and cloth at this frame (m).
fn box_mitte_radius(bau: &Bau, x: &[[f64; 3]]) -> ([f64; 3], f64) {
    let (mut min, mut max) = bau.huelle();
    for p in x {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if !min[0].is_finite() || !max[0].is_finite() {
        return ([0.0, 0.0, 1.0], 0.5);
    }
    let mitte = [
        0.5 * (min[0] + max[0]),
        0.5 * (min[1] + max[1]),
        0.5 * (min[2] + max[2]),
    ];
    let mut r2 = 0.0;
    for i in 0..3 {
        r2 += (0.5 * (max[i] - min[i])).powi(2);
    }
    (mitte, r2.sqrt())
}

// ---------------------------------------------------------------------------------------------
// Vertex data

/// One frame of vertex data, shared between the window and the render callback.
///
/// Plain `f32` vectors, not a packed struct: this crate has no `bytemuck`, so the buffer contents
/// are assembled as floats and handed over as bytes ([`als_bytes`]).
#[derive(Debug)]
pub struct Nutzlast {
    /// Uniform block: the 4×4 view-projection (column major) and the light direction.
    kopf: Vec<f32>,
    /// Vertices, [`KNOTEN_FLOATS`] floats each: position, normal, colour.
    knoten: Vec<f32>,
    /// Triangle indices into `knoten`.
    flaechen: Vec<u32>,
    /// Line indices into `knoten` (empty without the wireframe).
    kanten: Vec<u32>,
    /// Triangles in `flaechen`.
    dreiecke: usize,
    /// Line segments in `kanten`.
    segmente: usize,
}

impl Nutzlast {
    /// Vertices in `knoten`.
    fn knoten_zahl(&self) -> usize {
        self.knoten.len() / KNOTEN_FLOATS
    }
}

/// A float slice as bytes.
///
/// # Safety
///
/// `f32` is plain data: no padding, every bit pattern valid, alignment 4 ≥ the byte alignment.
fn als_bytes(f: &[f32]) -> &[u8] {
    // SAFETY: see above – the slice is read-only and its length in bytes is exactly
    // `size_of_val`, which keeps the pointer and length consistent.
    unsafe { std::slice::from_raw_parts(f.as_ptr().cast::<u8>(), std::mem::size_of_val(f)) }
}

/// A `u32` slice as bytes.
///
/// # Safety
///
/// As [`als_bytes`]: `u32` is plain data with a stricter alignment than `u8`.
fn als_bytes_u32(d: &[u32]) -> &[u8] {
    // SAFETY: see above.
    unsafe { std::slice::from_raw_parts(d.as_ptr().cast::<u8>(), std::mem::size_of_val(d)) }
}

/// The mean of the three corner colours a triangle names.
///
/// `d` holds indices **into `f`**, so every triangle gets its own corners' colours – not the first
/// three entries of the list, which would paint the whole garment in one colour.
fn mittel_farbe(f: &[[f32; 3]], d: [u32; 3]) -> [f32; 3] {
    let (f0, f1, f2) = (f[d[0] as usize], f[d[1] as usize], f[d[2] as usize]);
    [
        (f0[0] + f1[0] + f2[0]) / 3.0,
        (f0[1] + f1[1] + f2[1]) / 3.0,
        (f0[2] + f1[2] + f2[2]) / 3.0,
    ]
}

/// Assemble one frame: body and cloth triangles sorted back to front about the camera, each face
/// flat-shaded with its normal turned towards the camera, then the cloth edges as lines.
///
/// `karte` carries one value per cloth point; a triangle then takes the mean colour of its three
/// corners, so the ramp stays continuous across neighbouring triangles. Without a map the cloth
/// keeps its single colour ([`STOFF_FARBE`]).
fn bauen(
    bau: &Bau,
    x: &[[f64; 3]],
    kamera: &Ansicht3d,
    netzfarbe: Option<[f32; 3]>,
    karte: Option<&Kartenwerte>,
) -> Nutzlast {
    let auge = kamera.auge();
    let punkte = |i: u32| -> [f64; 3] {
        let i = i as usize;
        if i < bau.koerper.len() {
            bau.koerper[i]
        } else {
            x.get(i - bau.koerper.len()).copied().unwrap_or([0.0; 3])
        }
    };
    // Colour per cloth point. Only when the values cover every point the triangles use: a short
    // list would leave the rest with a colour that means nothing.
    let stoff_start = bau.koerper.len() as u32;
    let gebraucht = bau
        .stoff_dreiecke
        .iter()
        .flatten()
        .map(|i| *i as usize + 1)
        .max()
        .unwrap_or(0);
    let farben: Option<Vec<[f32; 3]>> = karte
        .map(|k| k.farben_bis(kamera.skalen.bis(k.karte)))
        .filter(|f| f.len() >= gebraucht);

    // (corners, colour, centre distance from the eye)
    let mut dreiecke: Vec<([u32; 3], [f32; 3], f64)> =
        Vec::with_capacity(bau.koerper_dreiecke.len() + bau.stoff_dreiecke.len());
    let mut sammeln = |liste: &[[u32; 3]],
                       versatz: u32,
                       farbe: Option<&[[f32; 3]]>,
                       einfarbig: [f32; 3],
                       vorzug: f64| {
        for d in liste {
            let e = [d[0] + versatz, d[1] + versatz, d[2] + versatz];
            let (a, b, c) = (punkte(e[0]), punkte(e[1]), punkte(e[2]));
            let mitte = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let ab = [mitte[0] - auge[0], mitte[1] - auge[1], mitte[2] - auge[2]];
            let entfernung = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).sqrt();
            // The corners' own colours, averaged: `d` holds the indices *within* the list.
            let mittel = match farbe {
                Some(f) => mittel_farbe(f, *d),
                None => einfarbig,
            };
            dreiecke.push((e, mittel, entfernung - vorzug));
        }
    };
    sammeln(&bau.koerper_dreiecke, 0, None, KOERPER_FARBE, 0.0);
    // The cloth lies a fraction of a millimetre on the skin, so comparing raw distances lets body
    // triangles whose centre happens to be nearer win that race and punch grey speckles through the
    // garment. A bias shorter than the thinnest cloth-to-skin gap keeps the cloth in front of the
    // body it covers, and far shorter than a limb's distance so it cannot reach through anything.
    sammeln(
        &bau.stoff_dreiecke,
        stoff_start,
        farben.as_deref(),
        STOFF_FARBE,
        STOFF_VORZUG,
    );
    // Far to near. Stable, so equal distances keep their order and the picture does not flicker.
    dreiecke.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut knoten: Vec<f32> = Vec::with_capacity(dreiecke.len() * 3 * KNOTEN_FLOATS);
    let mut flaechen: Vec<u32> = Vec::with_capacity(dreiecke.len() * 3);
    for (e, farbe, _) in &dreiecke {
        let p = [punkte(e[0]), punkte(e[1]), punkte(e[2])];
        let mut n = normale(p);
        let mitte = [
            (p[0][0] + p[1][0] + p[2][0]) / 3.0,
            (p[0][1] + p[1][1] + p[2][1]) / 3.0,
            (p[0][2] + p[1][2] + p[2][2]) / 3.0,
        ];
        let blick = [auge[0] - mitte[0], auge[1] - mitte[1], auge[2] - mitte[2]];
        if n[0] * blick[0] + n[1] * blick[1] + n[2] * blick[2] < 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        let basis = (knoten.len() / KNOTEN_FLOATS) as u32;
        for q in p {
            knoten.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32]);
            knoten.extend_from_slice(&[n[0] as f32, n[1] as f32, n[2] as f32]);
            knoten.extend_from_slice(farbe);
        }
        flaechen.extend_from_slice(&[basis, basis + 1, basis + 2]);
    }

    // The wireframe: every cloth edge once.
    let mut kanten: Vec<u32> = Vec::new();
    let mut segmente = 0usize;
    if let Some(farbe) = netzfarbe {
        let mut gesehen: HashSet<(u32, u32)> = HashSet::new();
        for d in &bau.stoff_dreiecke {
            for (a, b) in [(d[0], d[1]), (d[1], d[2]), (d[2], d[0])] {
                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                if !gesehen.insert((a, b)) {
                    continue;
                }
                let basis = (knoten.len() / KNOTEN_FLOATS) as u32;
                for i in [a, b] {
                    let q = punkte(stoff_start + i);
                    knoten.extend_from_slice(&[
                        q[0] as f32,
                        q[1] as f32,
                        q[2] as f32,
                        0.0,
                        0.0,
                        1.0,
                    ]);
                    knoten.extend_from_slice(&farbe);
                }
                kanten.extend_from_slice(&[basis, basis + 1]);
                segmente += 1;
            }
        }
    }

    // Light from above and a little in front of the camera, so faces keep some relief.
    let licht = [0.35, -0.45, 0.82];
    let mut kopf = blick_matrix(&auge, &kamera.ziel);
    kopf.extend_from_slice(&[licht[0], licht[1], licht[2], 0.0]);
    debug_assert_eq!(kopf.len(), KOPF_FLOATS);

    Nutzlast {
        kopf,
        knoten,
        flaechen,
        kanten,
        dreiecke: dreiecke.len(),
        segmente,
    }
}

/// Face normal of a triangle, unit length (zero for a degenerate one).
fn normale(p: [[f64; 3]; 3]) -> [f64; 3] {
    let a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let mut n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 1e-12 {
        for v in n.iter_mut() {
            *v /= l;
        }
    }
    n
}

/// View-projection matrix, column major, for the wgpu depth range `0..1`.
fn blick_matrix(auge: &[f64; 3], ziel: &[f64; 3]) -> Vec<f32> {
    let f = norm([ziel[0] - auge[0], ziel[1] - auge[1], ziel[2] - auge[2]]);
    let s = norm(kreuz(f, [0.0, 0.0, 1.0]));
    let u = kreuz(s, f);
    let sicht = [
        s[0],
        u[0],
        -f[0],
        0.0, //
        s[1],
        u[1],
        -f[1],
        0.0, //
        s[2],
        u[2],
        -f[2],
        0.0, //
        -(s[0] * auge[0] + s[1] * auge[1] + s[2] * auge[2]),
        -(u[0] * auge[0] + u[1] * auge[1] + u[2] * auge[2]),
        f[0] * auge[0] + f[1] * auge[1] + f[2] * auge[2],
        1.0,
    ];
    let f_ = 1.0 / (BLICKWINKEL / 2.0).tan();
    let (n, fa) = (NAH, FERN);
    let seite = 1.0; // The viewport is square enough; the shader gets the aspect as well.
    let _ = seite;
    let proj = [
        f_,
        0.0,
        0.0,
        0.0, //
        0.0,
        f_,
        0.0,
        0.0, //
        0.0,
        0.0,
        fa / (n - fa),
        -1.0, //
        0.0,
        0.0,
        n * fa / (n - fa),
        0.0,
    ];
    let mut m = [0.0f64; 16];
    for i in 0..4 {
        for j in 0..4 {
            let mut summe = 0.0;
            for k in 0..4 {
                summe += proj[k * 4 + j] * sicht[i * 4 + k];
            }
            m[i * 4 + j] = summe;
        }
    }
    m.iter().map(|v| *v as f32).collect()
}

fn kreuz(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> [f64; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

// ---------------------------------------------------------------------------------------------
// Inspection maps: legend and the value under the cursor

/// Width of the legend box, in points (narrower in a narrow pane).
const LEGENDE_BREITE: f32 = 176.0;
/// Padding inside the legend box and the value box, in points.
const LEGENDE_RAND: f32 = 8.0;
/// Height of the colour bar, in points.
const LEGENDE_BALKEN: f32 = 12.0;
/// How near the cursor has to be to a cloth point, in points.
const ZEIGER_RADIUS: f32 = 14.0;

/// The map and its values, when the snapshot carries them.
fn karten_werte(bild: &Schnappschuss) -> Option<(Karte, &Kartenwerte)> {
    bild.karte.as_ref().map(|w| (w.karte, w))
}

/// A number the way the rest of the window writes it: German comma, one decimal at most, and
/// `-` for a value that is not finite. Public so the toolbar's scale control writes the same.
pub fn zahl_text(v: f64) -> String {
    if !v.is_finite() {
        return "–".to_string();
    }
    let gerundet = (v * 10.0).round() / 10.0;
    if (gerundet - gerundet.round()).abs() < 1e-9 {
        format!("{}", gerundet.round() as i64)
    } else {
        format!("{gerundet:.1}").replace('.', ",")
    }
}

/// A ramp colour as egui sees it: the same sRGB bytes the command line writes into an OBJ.
fn egui_farbe(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// Background of an overlaid box, dark or light with the theme, so the text stays readable on both.
fn feld_grund(vis: &egui::Visuals) -> egui::Color32 {
    if vis.dark_mode {
        egui::Color32::from_black_alpha(200)
    } else {
        egui::Color32::from_white_alpha(220)
    }
}

/// What the map does not say by its title and scale alone.
fn notizen(karte: Karte, hat_koerper: bool, werte: &Kartenwerte, bis: f64) -> Vec<String> {
    let mut zeilen = match karte {
        Karte::Dehnung => vec!["größte Hauptdehnung".to_string()],
        Karte::Passform => vec![
            "100 % = Grenze der Toleranz".to_string(),
            format!(
                "Dehnung {} % · Abstand {} mm",
                zahl_text(klon3d_sim::dehnung::UEBER_DEHNUNG * 100.0),
                zahl_text(KOMFORT_ABSTAND_MM)
            ),
        ],
        Karte::Abstand => vec!["0 mm = auf der Haut · darunter blau".to_string()],
        Karte::Spannung => {
            let mut z = vec!["größte Hauptmembrankraft".to_string()];
            // The default end is the 10 % line of the reference material; a moved end is the
            // reader's own choice and must not claim that.
            if bis == karte.bis() {
                z.push(format!(
                    "{} {} = Grenze der linearen Dehnung",
                    zahl_text(bis),
                    karte.einheit()
                ));
            }
            z
        }
    };
    if !hat_koerper && karte == Karte::Passform {
        // Without a body only the strain half can be measured; saying so is better than letting
        // the scale look like the whole picture.
        zeilen.push("ohne Körper: nur Dehnung".to_string());
    }
    if karte == Karte::Spannung {
        if werte.ohne_steifigkeit > 0 {
            // 0 N/m from missing stiffness is not the same statement as 0 N/m from slack cloth,
            // and the colour cannot tell them apart.
            zeilen.push(format!(
                "{} ohne Steifigkeit: 0 N/m",
                werte.ohne_steifigkeit
            ));
        }
        // With one material this is the strain map with other axis labels; saying so keeps the
        // colour from promising more than it delivers.
        zeilen.push("ein Stoff: Dehnung × Steifigkeit".to_string());
    }
    zeilen
}

/// The legend of the map on screen, bottom left: title, scale with its unit, the ramp and the
/// notes that keep the colours readable.
///
/// The bar is drawn from [`Karte::farbe_bis`], the same bytes the garment is coloured with (there
/// converted to linear light for the shader), so bar and cloth cannot drift apart. `bis` is the
/// upper end in effect — the map's default or the reader's own.
fn legende(
    ui: &egui::Ui,
    maler: &egui::Painter,
    rect: egui::Rect,
    karte: Karte,
    bis: f64,
    werte: &Kartenwerte,
    hat_koerper: bool,
) {
    let vis = ui.visuals();
    let breite = LEGENDE_BREITE.min((rect.width() - 20.0).max(LEGENDE_BREITE / 2.0));
    let innen = breite - 2.0 * LEGENDE_RAND;

    let mut zeilen: Vec<(std::sync::Arc<egui::Galley>, egui::Color32)> = Vec::new();
    let mut anfuegen = |text: &str, font: egui::FontId, farbe: egui::Color32| {
        zeilen.push((maler.layout(text.into(), font, farbe, innen), farbe));
    };
    anfuegen(
        &karte.titel(),
        egui::FontId::proportional(13.0),
        vis.strong_text_color(),
    );
    anfuegen(
        &format!(
            "{} … {} {}",
            zahl_text(karte.von()),
            zahl_text(bis),
            karte.einheit()
        ),
        egui::FontId::monospace(11.0),
        vis.text_color(),
    );
    for text in notizen(karte, hat_koerper, werte, bis) {
        anfuegen(&text, egui::FontId::proportional(11.0), vis.text_color());
    }
    // The grey of the ramp is not on the ramp: it marks points the measure could not reach.
    let grau = werte.je_punkt.iter().any(|v| !v.is_finite());
    if grau {
        anfuegen(
            "grau: kein Wert",
            egui::FontId::proportional(11.0),
            egui_farbe(klon3d_sim::farbkarte::UNGUELTIG),
        );
    }

    let mut hoehe = LEGENDE_RAND;
    for (galley, _) in &zeilen {
        hoehe += galley.size().y + 2.0;
    }
    hoehe += 4.0 + LEGENDE_BALKEN + LEGENDE_RAND;

    let feld = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0 - hoehe),
        egui::vec2(breite, hoehe),
    );
    maler.rect_filled(feld, 4.0, feld_grund(vis));
    maler.rect_stroke(
        feld,
        4.0,
        vis.window_stroke(),
        egui::StrokeKind::Inside,
    );

    let mut y = feld.top() + LEGENDE_RAND;
    for (galley, farbe) in zeilen {
        let h = galley.size().y;
        maler.galley(egui::pos2(feld.left() + LEGENDE_RAND, y), galley, farbe);
        y += h + 2.0;
    }

    // The ramp, sampled finely enough that the steps are not visible as bands.
    let balken = egui::Rect::from_min_size(
        egui::pos2(feld.left() + LEGENDE_RAND, y + 2.0),
        egui::vec2(innen, LEGENDE_BALKEN),
    );
    let schritte = 64;
    for i in 0..schritte {
        let t0 = i as f32 / schritte as f32;
        let t1 = (i + 1) as f32 / schritte as f32;
        let mitte = (t0 + t1) / 2.0;
        let wert = karte.von() + (bis - karte.von()) * mitte as f64;
        maler.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(balken.left() + balken.width() * t0, balken.top()),
                egui::pos2(balken.left() + balken.width() * t1, balken.bottom()),
            ),
            0.0,
            egui_farbe(karte.farbe_bis(wert, bis)),
        );
    }
    maler.rect_stroke(
        balken,
        0.0,
        egui::Stroke::new(1.0, vis.text_color()),
        egui::StrokeKind::Inside,
    );
}

/// A point of the scene in the picture, with its depth: `(x, y, w)` in points and the clip `w`,
/// which grows with the distance from the eye.
fn projizieren(m: &[f32], p: &[f64; 3], rect: egui::Rect) -> Option<(f32, f32, f64)> {
    let v = [p[0] as f32, p[1] as f32, p[2] as f32];
    let mut clip = [0.0f32; 4];
    for (i, c) in clip.iter_mut().enumerate() {
        // Column major, as in the shader: element (row i, column j) is `m[j * 4 + i]`.
        *c = m[i] * v[0] + m[4 + i] * v[1] + m[8 + i] * v[2] + m[12 + i];
    }
    if clip[3] <= 1e-6 {
        return None;
    }
    let x = rect.left() + (clip[0] / clip[3] * 0.5 + 0.5) * rect.width();
    // NDC y points up, the window's down.
    let y = rect.top() + (0.5 - clip[1] / clip[3] * 0.5) * rect.height();
    Some((x, y, clip[3] as f64))
}

/// The value of the cloth point nearest the cursor, if one is close enough.
///
/// Projected with the same matrix the shader uses, so the value belongs to what is drawn. Of the
/// points within [`ZEIGER_RADIUS`] the one nearest the camera wins: the garment covers itself,
/// and the cursor is meant to read the front.
fn wert_unter(
    rect: egui::Rect,
    kamera: &Ansicht3d,
    x: &[[f64; 3]],
    werte: &Kartenwerte,
    zeiger: egui::Pos2,
) -> Option<f64> {
    let m = blick_matrix(&kamera.auge(), &kamera.ziel);
    // (distance to the cursor, depth, value)
    let mut bestes: Option<(f32, f64, f64)> = None;
    for (i, p) in x.iter().enumerate() {
        let Some(&wert) = werte.je_punkt.get(i) else {
            continue;
        };
        if !wert.is_finite() {
            continue;
        }
        let Some((sx, sy, tiefe)) = projizieren(&m, p, rect) else {
            continue;
        };
        let d = egui::pos2(sx, sy).distance(zeiger);
        if d > ZEIGER_RADIUS {
            continue;
        }
        let besser = match bestes {
            None => true,
            Some((bisher, bisher_tiefe, _)) => {
                d < bisher - 0.5 || (d <= bisher + 0.5 && tiefe < bisher_tiefe)
            }
        };
        if besser {
            bestes = Some((d, tiefe, wert));
        }
    }
    bestes.map(|(_, _, wert)| wert)
}

/// The value under the cursor, as a small box beside it: the number with its unit and a swatch of
/// the colour it has on the garment.
fn wert_hinweis(
    ui: &egui::Ui,
    maler: &egui::Painter,
    rect: egui::Rect,
    zeiger: egui::Pos2,
    karte: Karte,
    bis: f64,
    wert: f64,
) {
    let vis = ui.visuals();
    let text = format!("{} {}", zahl_text(wert), karte.einheit());
    let galley = maler.layout(
        text.as_str().into(),
        egui::FontId::monospace(12.0),
        vis.strong_text_color(),
        200.0,
    );
    let kasten = egui::vec2(
        galley.size().x + 2.0 * LEGENDE_RAND + 14.0,
        galley.size().y + 2.0 * LEGENDE_RAND,
    );
    let mut pos = zeiger + egui::vec2(14.0, 14.0);
    if pos.x + kasten.x > rect.right() - 4.0 {
        pos.x = zeiger.x - 14.0 - kasten.x;
    }
    if pos.y + kasten.y > rect.bottom() - 4.0 {
        pos.y = zeiger.y - 14.0 - kasten.y;
    }
    let feld = egui::Rect::from_min_size(pos, kasten);
    maler.rect_filled(feld, 4.0, feld_grund(vis));
    maler.rect_stroke(
        feld,
        4.0,
        vis.window_stroke(),
        egui::StrokeKind::Inside,
    );
    let kante = egui::Rect::from_min_size(
        egui::pos2(
            feld.left() + LEGENDE_RAND,
            feld.center().y - 5.0,
        ),
        egui::vec2(10.0, 10.0),
    );
    maler.rect_filled(kante, 2.0, egui_farbe(karte.farbe_bis(wert, bis)));
    maler.galley(
        egui::pos2(feld.left() + LEGENDE_RAND + 14.0, feld.top() + LEGENDE_RAND),
        galley,
        vis.strong_text_color(),
    );
}

// ---------------------------------------------------------------------------------------------
// Rendering

/// The shader: two entry points, one pipeline for triangles and one for lines.
const WGSL: &str = r#"
struct Kopf {
    vp: mat4x4<f32>,
    licht: vec4<f32>,
};
@group(0) @binding(0) var<uniform> kopf: Kopf;

struct Knoten {
    @location(0) ort: vec3<f32>,
    @location(1) normale: vec3<f32>,
    @location(2) farbe: vec3<f32>,
};

struct Aus {
    @builtin(position) position: vec4<f32>,
    @location(0) farbe: vec3<f32>,
};

@vertex
fn flaeche_vert(k: Knoten) -> Aus {
    var a: Aus;
    a.position = kopf.vp * vec4<f32>(k.ort, 1.0);
    // Two lights: a fixed one from above and one from the camera side, so nothing stays black.
    let l1 = normalize(vec3<f32>(0.35, -0.45, 0.82));
    let l2 = kopf.licht.xyz;
    let n = normalize(k.normale);
    let hell = 0.30 + 0.55 * max(abs(dot(n, l1)), 0.0) + 0.35 * max(abs(dot(n, l2)), 0.0);
    a.farbe = k.farbe * min(hell, 1.25);
    return a;
}

@fragment
fn flaeche_frag(a: Aus) -> @location(0) vec4<f32> {
    return vec4<f32>(a.farbe, 1.0);
}

@vertex
fn kante_vert(k: Knoten) -> Aus {
    var a: Aus;
    a.position = kopf.vp * vec4<f32>(k.ort, 1.0);
    a.farbe = k.farbe;
    return a;
}

@fragment
fn kante_frag(a: Aus) -> @location(0) vec4<f32> {
    return vec4<f32>(a.farbe, 1.0);
}
"#;

/// The GPU objects of the view, kept in [`egui_wgpu::CallbackResources`].
struct Vorrat {
    kopf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    knoten: wgpu::Buffer,
    knoten_kapazitaet: u64,
    flaechen: wgpu::Buffer,
    flaechen_kapazitaet: u64,
    kanten: wgpu::Buffer,
    kanten_kapazitaet: u64,
    flaechen_pipeline: wgpu::RenderPipeline,
    kanten_pipeline: wgpu::RenderPipeline,
    /// The target format the pipelines were built for.
    format_wert: wgpu::TextureFormat,
    /// What is in the buffers, so an unchanged frame uploads nothing.
    geladen: Option<usize>,
}

impl Vorrat {
    fn neu(device: &wgpu::Device, format: wgpu::TextureFormat) -> Vorrat {
        let modul = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("klon3d Ansicht3d"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let kopf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("klon3d Ansicht3d – Kopf"),
            size: (KOPF_FLOATS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("klon3d Ansicht3d – Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("klon3d Ansicht3d – Bind"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: kopf.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("klon3d Ansicht3d – Pipeline-Layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let knoten_layout = wgpu::VertexBufferLayout {
            array_stride: (KNOTEN_FLOATS * 4) as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 3 * 4,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 6 * 4,
                    shader_location: 2,
                },
            ],
        };
        let pipeline = |label: &str, vert: &str, frag: &str, topologie: wgpu::PrimitiveTopology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &modul,
                    entry_point: Some(vert),
                    compilation_options: Default::default(),
                    buffers: &[Some(knoten_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    topology: topologie,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                // No depth buffer: egui's pass has none. The triangles are sorted instead.
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: MSAA,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &modul,
                    entry_point: Some(frag),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let leer = |nutzung: wgpu::BufferUsages, name: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(name),
                size: 256,
                usage: nutzung | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Vorrat {
            kopf,
            bind,
            knoten: leer(wgpu::BufferUsages::VERTEX, "klon3d Ansicht3d – Knoten"),
            knoten_kapazitaet: 256,
            flaechen: leer(wgpu::BufferUsages::INDEX, "klon3d Ansicht3d – Flächen"),
            flaechen_kapazitaet: 256,
            kanten: leer(wgpu::BufferUsages::INDEX, "klon3d Ansicht3d – Kanten"),
            kanten_kapazitaet: 256,
            flaechen_pipeline: pipeline(
                "klon3d Ansicht3d – Flächen",
                "flaeche_vert",
                "flaeche_frag",
                wgpu::PrimitiveTopology::TriangleList,
            ),
            kanten_pipeline: pipeline(
                "klon3d Ansicht3d – Kanten",
                "kante_vert",
                "kante_frag",
                wgpu::PrimitiveTopology::LineList,
            ),
            format_wert: format,
            geladen: None,
        }
    }

    /// Write a frame into the buffers, growing them when needed.
    fn hochladen(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, n: &Nutzlast) {
        queue.write_buffer(&self.kopf, 0, als_bytes(&n.kopf));
        schreiben(
            device,
            queue,
            &mut self.knoten,
            &mut self.knoten_kapazitaet,
            als_bytes(&n.knoten),
            wgpu::BufferUsages::VERTEX,
        );
        schreiben(
            device,
            queue,
            &mut self.flaechen,
            &mut self.flaechen_kapazitaet,
            als_bytes_u32(&n.flaechen),
            wgpu::BufferUsages::INDEX,
        );
        if !n.kanten.is_empty() {
            schreiben(
                device,
                queue,
                &mut self.kanten,
                &mut self.kanten_kapazitaet,
                als_bytes_u32(&n.kanten),
                wgpu::BufferUsages::INDEX,
            );
        }
        self.geladen = Some(n.knoten_zahl());
    }
}

/// Write `bytes` into `buffer`, replacing the buffer when it is too small.
fn schreiben(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    kapazitaet: &mut u64,
    bytes: &[u8],
    nutzung: wgpu::BufferUsages,
) {
    if bytes.is_empty() {
        return;
    }
    if bytes.len() as u64 > *kapazitaet {
        let neu = (bytes.len() as u64).next_power_of_two().max(256);
        *buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("klon3d Ansicht3d – Puffer"),
            size: neu,
            usage: nutzung | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *kapazitaet = neu;
    }
    queue.write_buffer(buffer, 0, bytes);
}

/// The paint callback of one frame: it uploads on `prepare` and draws in the egui pass.
struct AnsichtCallback {
    daten: Arc<Nutzlast>,
    format: wgpu::TextureFormat,
}

impl egui_wgpu::CallbackTrait for AnsichtCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        ressourcen: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let format = self.format;
        let vorrat = ressourcen
            .entry::<Vorrat>()
            .or_insert_with(|| Vorrat::neu(device, format));
        // The target format is fixed for the life of a window; a changed one means new pipelines.
        if vorrat.format() != format {
            *vorrat = Vorrat::neu(device, format);
        }
        vorrat.hochladen(device, queue, &self.daten);
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        ressourcen: &egui_wgpu::CallbackResources,
    ) {
        let Some(vorrat) = ressourcen.get::<Vorrat>() else {
            return;
        };
        // egui resets the viewport and its own pipeline around a callback; the clip rectangle is
        // still eguis, and this pane lies inside it. Its own scissor keeps the drawing inside.
        let clip = info.clip_rect_in_pixels();
        pass.set_scissor_rect(
            clip.left_px.max(0) as u32,
            clip.top_px.max(0) as u32,
            clip.width_px.max(0) as u32,
            clip.height_px.max(0) as u32,
        );
        pass.set_bind_group(0, &vorrat.bind, &[]);
        pass.set_vertex_buffer(0, vorrat.knoten.slice(..));
        let n = &self.daten;

        if n.dreiecke > 0 {
            pass.set_pipeline(&vorrat.flaechen_pipeline);
            pass.set_index_buffer(vorrat.flaechen.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..(n.dreiecke * 3) as u32, 0, 0..1);
        }
        if n.segmente > 0 {
            pass.set_pipeline(&vorrat.kanten_pipeline);
            pass.set_index_buffer(vorrat.kanten.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..(n.segmente * 2) as u32, 0, 0..1);
        }
    }
}

impl Vorrat {
    /// Format the pipelines were built for – taken from the colour target of the triangle
    /// pipeline, which wgpu does not return, so it is stored next to it.
    fn format(&self) -> wgpu::TextureFormat {
        self.format_wert
    }
}

#[cfg(test)]
mod tests {
    use super::mittel_farbe;

    /// A camera reset puts back the camera, not the reading: the map scales stay where the reader
    /// put them, because each has its own reset.
    #[test]
    fn kamera_zuruecksetzen_laesst_die_skalen_stehen() {
        let mut a = super::Ansicht3d::default();
        a.yaw = 0.7;
        a.skalen.setzen(super::Karte::Spannung, 80.0);
        a.zuruecksetzen();
        assert_eq!(a.yaw, 0.0, "die Kamera steht wieder auf der Vorgabe");
        assert_eq!(a.skalen.bis(super::Karte::Spannung), 80.0);
        assert!(!a.skalen.ist_vorgabe(super::Karte::Spannung));
    }

    /// Two triangles of a four-point cloth must not both take the colours of the first three
    /// points: the mean has to use the corners the triangle itself names.
    #[test]
    fn jedes_dreieck_nimmt_die_farben_seiner_eigenen_ecken() {
        let f = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
        ];
        // A quad as 0-1-2 and 0-2-3: the second triangle's corners are 0, 2 and 3.
        let a = mittel_farbe(&f, [0, 1, 2]);
        let b = mittel_farbe(&f, [0, 2, 3]);
        for (i, wert) in a.iter().enumerate() {
            assert!((wert - 1.0 / 3.0).abs() < 1e-6, "Dreieck 1, Kanal {i}: {wert}");
        }
        assert!((b[2] - 1.0 / 3.0).abs() < 1e-6, "Dreieck 2: {b:?}");
        assert_ne!(a, b, "beide Dreiecke tragen dieselbe Farbe");
    }
}
