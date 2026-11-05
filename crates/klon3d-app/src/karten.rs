//! The inspection maps of the 3D view: one value and one colour per cloth point.
//!
//! Stage 6 asks for strain, fit and distance as colours on the garment, and stage 5's stress map
//! belongs beside them. The measurements themselves are `klon3d-sim`'s
//! ([`klon3d_sim::dehnung`], [`klon3d_sim::abstand`], [`klon3d_sim::spannung`]), and they are the
//! same ones `klon3d anziehen --farben` writes into a coloured OBJ. This module only decides which
//! of them the view shows, where its scale starts and ends, and how a value becomes a colour.
//!
//! It holds no egui type on purpose: the worker thread measures the map ([`crate::simulation`])
//! and the values travel with the snapshot, so the colours always belong to the frame on screen –
//! and measuring the same scene twice gives the same colours as the command line tool.
//!
//! ## Why the scales are fixed
//!
//! [`klon3d_sim::farbkarte`] keeps one scale per measure and says why: a scale normalised to the
//! worst value of each run makes every run look equally bad, and then a colour means nothing.
//! The maps therefore print their bounds next to themselves and keep the same ramp everywhere.
//!
//! ## Why the upper end can still be moved
//!
//! A fixed scale fails on the garment it was not made for: a piece whose values sit above the
//! default end is one flat red field with no structure, and until now the only way out was editing
//! a constant and rebuilding. [`Skalen`] lets the upper end of each map be raised or lowered while
//! the window runs, and the cloth and the legend colour from the same number, so they cannot drift
//! apart.
//!
//! Moving the end costs no solver frame: the snapshot carries raw numbers (%, mm, N/m) and the
//! scale only decides how a number becomes a colour. The same numbers read differently, which is
//! correct — nothing was re-measured. The default stays a named constant
//! ([`Karte::bis`] → [`klon3d_sim::farbkarte`], [`PASSFORM_BIS`]), not a literal in a menu, so
//! "reset" means the number the map was designed with.
//!
//! ## The fit map
//!
//! "Passformkarte" is not a CLO feature to copy. CLO 2026.1 has a *Fit Map* menu of its own, but
//! its exports carry no definition of the measure, and "fit" is not "strain under another name":
//! a garment can be badly fitted because it is stretched too far **or** because it does not lie on
//! the body at all. The definition used here is our own and stated in full:
//!
//! A latex garment fits when it lies on the skin and is stretched only as far as the material can
//! take. Both limits are faults when exceeded and they are not interchangeable, so the map does
//! not average them: it shows **how much of the tolerance the worse of the two uses**, in percent.
//!
//! * strain against the comfort limit of the fabric, 10 % – the documented end of latex's linear
//!   range ([`klon3d_sim::dehnung::UEBER_DEHNUNG`], `MATERIAL.md`);
//! * standoff from the body against 5 mm, taken as a magnitude: cloth floating 5 mm off the skin
//!   and cloth pushed 5 mm into it are both a bad fit.
//!
//! 100 % is the limit: below it the garment fits, above it does not. Without a body only the
//! strain half can be measured; the map then says so and its scale is the strain scale in
//! disguise, which is honest but worth knowing.
//!
//! ## The stress map
//!
//! "Spannungskarte" answers *how hard* the cloth pulls, in N/m of membrane force per width, where
//! the strain map answers *how far* it is stretched. [`klon3d_sim::spannung`] states the honest
//! limit and it is repeated here rather than hidden: the solver's membrane is linear elastic, so
//! for **one** material the stress is the strain times a constant and the map is the strain map
//! with different axis labels. It earns its place through two things only:
//!
//! * **Absolute units** — 35 N/m is the 10 % line of `Latex_0.4mm_Radical_Rubber`, so a colour at
//!   the warm end says "this piece is at the limit its material profile documents", which "10 %"
//!   alone cannot say for a different fabric;
//! * **Mixed stiffness in one scene** — two pieces of different thickness under the same stretch
//!   carry different force, and the scene carries the material per piece
//!   (`Teilvorgabe.material`, set from the piece's thickness in `klon3d-pipeline`).
//!
//! A piece whose material has no usable `dehnung_n_m` reads 0 N/m at every point
//! ([`klon3d_sim::spannung::Karte::messe`] says why: that is a scene-building bug, not a
//! measurement). The scale cannot show the difference between "slack" and "not measurable", so the
//! legend says it in words ([`crate::ansicht3d`]).

use klon3d_sim::abstand::Karte as Abstandskarte;
use klon3d_sim::dehnung::Karte as Dehnungskarte;
use klon3d_sim::farbkarte::{self, Rgb};
use klon3d_sim::spannung::Karte as Spannungskarte;
use klon3d_sim::Szene;

/// Strain at which the comfort range ends, in percent: [`klon3d_sim::dehnung::UEBER_DEHNUNG`] as
/// a percentage.
const KOMFORT_DEHNUNG_PROZENT: f64 = klon3d_sim::dehnung::UEBER_DEHNUNG * 100.0;

/// Standoff beyond which cloth no longer counts as lying on the body, in mm.
///
/// An assumption, not a measurement: 5 mm is about twelve fabric thicknesses of 0.4 mm latex, far
/// more than a wrinkle and far less than a sleeve cap standing off. It decides the left half of
/// the fit scale and is named in the legend's hover text.
pub const KOMFORT_ABSTAND_MM: f64 = 5.0;

/// Fit scale: from this value (blue) to that value (red), in percent of the comfort range.
pub const PASSFORM_VON: f64 = 0.0;
pub const PASSFORM_BIS: f64 = 200.0;

/// One of the maps the 3D view can paint on the garment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Karte {
    /// Larger principal strain per point, in percent.
    Dehnung,
    /// How much of the material's comfort range the point uses, in percent (100 % = at the limit).
    Passform,
    /// Signed distance from the cloth to the body, in mm.
    Abstand,
    /// Larger principal membrane force per point, in N/m.
    Spannung,
}

impl Karte {
    /// All maps, in the order the menu lists them.
    pub const ALLE: [Karte; 4] = [
        Karte::Dehnung,
        Karte::Passform,
        Karte::Abstand,
        Karte::Spannung,
    ];

    /// The measure's name, as the menu and the toolbar write it.
    pub fn name(self) -> &'static str {
        match self {
            Karte::Dehnung => "Dehnung",
            Karte::Passform => "Passform",
            Karte::Abstand => "Abstand",
            Karte::Spannung => "Spannung",
        }
    }

    /// The title of the legend and the buttons: the name with "karte".
    ///
    /// Written out instead of glued together: German puts a linking "s" into "Dehnungskarte", and
    /// a `format!` would write "Dehnungkarte".
    pub fn titel(self) -> &'static str {
        match self {
            Karte::Dehnung => "Dehnungskarte",
            Karte::Passform => "Passformkarte",
            Karte::Abstand => "Abstandskarte",
            Karte::Spannung => "Spannungskarte",
        }
    }

    /// The unit the values are in.
    pub fn einheit(self) -> &'static str {
        match self {
            Karte::Dehnung | Karte::Passform => "%",
            Karte::Abstand => "mm",
            Karte::Spannung => "N/m",
        }
    }

    /// Lower end of the colour scale (blue).
    pub fn von(self) -> f64 {
        match self {
            Karte::Dehnung => farbkarte::DEHNUNG_VON,
            Karte::Passform => PASSFORM_VON,
            Karte::Abstand => farbkarte::ABSTAND_VON,
            Karte::Spannung => farbkarte::SPANNUNG_VON,
        }
    }

    /// Upper end of the colour scale (red).
    pub fn bis(self) -> f64 {
        match self {
            Karte::Dehnung => farbkarte::DEHNUNG_BIS,
            Karte::Passform => PASSFORM_BIS,
            Karte::Abstand => farbkarte::ABSTAND_BIS,
            Karte::Spannung => farbkarte::SPANNUNG_BIS,
        }
    }

    /// What the map shows, for the menu entry's hover text.
    pub fn erklaerung(self) -> &'static str {
        match self {
            Karte::Dehnung => "wie weit der Stoff gedehnt ist (größte Hauptdehnung je Punkt)",
            Karte::Passform => {
                "wie viel der Stoff von seiner Toleranz braucht: Dehnung gegen 10 %, \
                 Abstand zur Haut gegen 5 mm, der schlechtere Wert zählt"
            }
            Karte::Abstand => "wie weit der Stoff von der Haut steht (vorzeichenbehaftet, je Punkt)",
            Karte::Spannung => {
                "wie hart der Stoff zieht (größte Hauptmembrankraft je Punkt in N/m, \
                 abhängig von der Steifigkeit des Teils)"
            }
        }
    }

    /// Whether the map needs a body to be measured.
    pub fn braucht_koerper(self) -> bool {
        matches!(self, Karte::Abstand)
    }

    /// The values of this map for `s`, one per cloth point, in scene order.
    ///
    /// `None` when the map cannot be measured for this scene — a distance map without a body.
    pub fn messen(self, s: &Szene) -> Option<Kartenwerte> {
        let je_punkt = match self {
            Karte::Dehnung => Dehnungskarte::messe(s).je_punkt(s),
            Karte::Passform => passform_je_punkt(s),
            Karte::Abstand => Abstandskarte::messe(s)?
                .je_punkt
                .iter()
                .map(|p| p.abstand * 1e3)
                .collect(),
            Karte::Spannung => Spannungskarte::messe(s).je_punkt(s),
        };
        // Only the stress map reads a material property; for the others the count stays 0 and
        // nothing on screen changes.
        let ohne_steifigkeit = if self == Karte::Spannung {
            teile_ohne_steifigkeit(s)
        } else {
            0
        };
        Some(Kartenwerte {
            karte: self,
            je_punkt,
            ohne_steifigkeit,
        })
    }

    /// Stored name of the map, independent of the German label: it is written into the
    /// settings file, so it must not follow a wording change in a menu.
    pub fn schluessel(self) -> &'static str {
        match self {
            Karte::Dehnung => "dehnung",
            Karte::Passform => "passform",
            Karte::Abstand => "abstand",
            Karte::Spannung => "spannung",
        }
    }

    /// Index in [`Karte::ALLE`], for per-map state that is kept in a fixed-size array.
    fn index(self) -> usize {
        match self {
            Karte::Dehnung => 0,
            Karte::Passform => 1,
            Karte::Abstand => 2,
            Karte::Spannung => 3,
        }
    }

    /// Colour of one value on this map's ramp, as the bytes the ramp was defined with.
    pub fn farbe(self, wert: f64) -> Rgb {
        self.farbe_bis(wert, self.bis())
    }

    /// The same, on a ramp that ends at `bis` instead of the default.
    ///
    /// The value itself is never touched: the ramp only decides how a number becomes a colour, so
    /// a moved end re-reads the numbers already measured instead of needing a new solver frame.
    pub fn farbe_bis(self, wert: f64, bis: f64) -> Rgb {
        farbkarte::farbe(wert, self.von(), bis)
    }

    /// The same colour as a linear RGB triple, for the shader.
    ///
    /// The ramp is defined in sRGB bytes (it is also written into an OBJ and drawn in the legend),
    /// the render target is sRGB and the shader works in linear light, so the bytes have to be
    /// converted once. Without this the garment would be visibly darker than the legend beside it.
    pub fn farbe_linear(self, wert: f64) -> [f32; 3] {
        self.farbe_linear_bis(wert, self.bis())
    }

    /// [`Karte::farbe_linear`] on a ramp that ends at `bis`.
    pub fn farbe_linear_bis(self, wert: f64, bis: f64) -> [f32; 3] {
        let c = self.farbe_bis(wert, bis);
        [linear(c[0]), linear(c[1]), linear(c[2])]
    }
}

/// sRGB byte → linear share of full brightness.
fn linear(k: u8) -> f32 {
    let s = k as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// The measured values of one map for one frame.
#[derive(Clone, Debug)]
pub struct Kartenwerte {
    /// Which map these numbers belong to. The view compares it with the map selected: a snapshot
    /// measured for another map is not coloured with it.
    pub karte: Karte,
    /// One value per cloth point, in scene order.
    pub je_punkt: Vec<f64>,
    /// Pieces whose material had no usable stiffness, so every one of their points reads 0 N/m.
    ///
    /// Only the stress map fills this in. It is a scene-building fault, not a measurement: 0 N/m
    /// from missing stiffness and 0 N/m from slack cloth are the same blue, and the legend has to
    /// say which one the reader is looking at.
    pub ohne_steifigkeit: usize,
}

impl Kartenwerte {
    /// Colour per cloth point, linear RGB for the vertex buffer, on each map's default scale.
    pub fn farben(&self) -> Vec<[f32; 3]> {
        self.farben_bis(self.karte.bis())
    }

    /// The same colours on a ramp whose upper end is `bis`.
    ///
    /// The values are untouched; only the mapping to colour changes, which is why a moved end shows
    /// on the frame already on screen and needs neither a new solver frame nor a new measurement.
    pub fn farben_bis(&self, bis: f64) -> Vec<[f32; 3]> {
        self.je_punkt
            .iter()
            .map(|&w| self.karte.farbe_linear_bis(w, bis))
            .collect()
    }
}

/// Where the upper end of each map's colour scale sits while the window runs.
///
/// One value per map, in [`Karte::ALLE`] order. The lower end is not adjustable on purpose: it is
/// part of what the map *means* (0 % strain, 0 mm on the skin, 0 N/m of slack cloth) and moving it
/// would move the meaning, not just the picture. Only the upper end decides how much of the garment
/// reaches red, and that is the number a reader changes when a real garment's values sit outside
/// the default range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skalen {
    /// Upper end per map, in [`Karte::ALLE`] order.
    bis: [f64; Karte::ALLE.len()],
}

impl Default for Skalen {
    fn default() -> Self {
        let mut bis = [0.0; Karte::ALLE.len()];
        for k in Karte::ALLE {
            bis[k.index()] = k.bis();
        }
        Skalen { bis }
    }
}

impl Skalen {
    /// The upper end in effect for `karte`.
    pub fn bis(&self, karte: Karte) -> f64 {
        self.bis[karte.index()]
    }

    /// Whether `karte` still runs on the default end it was designed with.
    pub fn ist_vorgabe(&self, karte: Karte) -> bool {
        self.bis(karte) == karte.bis()
    }

    /// Set the upper end of `karte`.
    ///
    /// Clamped to stay above the lower end: a scale that ends where it starts has no ramp
    /// ([`klon3d_sim::farbkarte::farbe`] answers such a scale with the "no value" grey), and a
    /// value of 0 or below would make a whole garment one colour. A NaN is refused the same way.
    pub fn setzen(&mut self, karte: Karte, bis: f64) {
        if !bis.is_finite() || bis <= karte.von() {
            return;
        }
        self.bis[karte.index()] = bis;
    }

    /// Put `karte` back on its default end ([`Karte::bis`], i.e. the constant the map was designed
    /// with — not a literal of the menu).
    pub fn zuruecksetzen(&mut self, karte: Karte) {
        self.bis[karte.index()] = karte.bis();
    }

    /// `spannung=35;dehnung=20;…`, one pair per map that differs from its default.
    ///
    /// Only the moved ends are written: a settings file from before this feature, or from a start
    /// where nothing was touched, stays empty and every map takes its constant.
    pub fn als_text(&self) -> String {
        Karte::ALLE
            .iter()
            .filter(|k| !self.ist_vorgabe(**k))
            .map(|k| format!("{}={:.4}", k.schluessel(), self.bis(*k)))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The scales from [`Skalen::als_text`]; anything unknown, unreadable or below its lower end
    /// keeps the default.
    pub fn aus_text(text: &str) -> Skalen {
        let mut s = Skalen::default();
        for paar in text.split(';') {
            let Some((name, wert)) = paar.split_once('=') else {
                continue;
            };
            let (name, wert) = (name.trim(), wert.trim());
            let Some(karte) = Karte::ALLE.iter().find(|k| k.schluessel() == name) else {
                continue;
            };
            if let Ok(v) = wert.parse::<f64>() {
                s.setzen(*karte, v);
            }
        }
        s
    }
}

/// Storage key of the map scales ([`Skalen::als_text`]).
pub const SCHLUESSEL_SKALEN: &str = "klon3d.karten.skalen";

/// The fit map: the worse of the two halves, each as a share of its tolerance.
///
/// Without a body the standoff half is missing and the strain half decides alone — the only
/// honest answer, and the legend says so ([`Karte::braucht_koerper`]).
fn passform_je_punkt(s: &Szene) -> Vec<f64> {
    let dehnung = Dehnungskarte::messe(s).je_punkt(s);
    let abstand = Abstandskarte::messe(s).map(|k| k.je_punkt);
    dehnung
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            let aus_dehnung = if KOMFORT_DEHNUNG_PROZENT > 0.0 {
                d / KOMFORT_DEHNUNG_PROZENT * 100.0
            } else {
                0.0
            };
            let aus_abstand = abstand.as_ref().map_or(0.0, |a| {
                a.get(i).map_or(0.0, |p| {
                    p.abstand.abs() * 1e3 / KOMFORT_ABSTAND_MM * 100.0
                })
            });
            aus_dehnung.max(aus_abstand)
        })
        .collect()
}

/// How many pieces of `s` carry no usable stiffness, i.e. would read 0 N/m at every point.
///
/// The same test [`klon3d_sim::spannung::Karte::messe`] applies per piece: finite and above zero.
/// Repeated here instead of reaching into the map's internals so the count means what the values
/// mean.
fn teile_ohne_steifigkeit(s: &Szene) -> usize {
    s.teile
        .iter()
        .filter(|t| {
            let e = t.vorgabe.material.dehnung_n_m;
            !(e.is_finite() && e > 0.0)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jede_karte_hat_titel_einheit_und_skala() {
        for k in Karte::ALLE {
            assert!(k.titel().ends_with("karte"), "{:?}", k);
            assert!(!k.einheit().is_empty() && !k.erklaerung().is_empty());
            assert!(k.bis() > k.von(), "{k:?}: {} … {}", k.von(), k.bis());
        }
        // Only the distance map needs a body.
        assert!(Karte::Abstand.braucht_koerper());
        assert!(!Karte::Dehnung.braucht_koerper());
        assert!(!Karte::Passform.braucht_koerper());
        // Stress is a property of the cloth and its material alone: no body needed.
        assert!(!Karte::Spannung.braucht_koerper());
    }

    /// The ramp's ends and its "no value" grey go through the conversion unchanged in meaning: the
    /// darkest byte stays dark, the brightest stays bright, and a NaN is the neutral grey.
    #[test]
    fn farben_sind_die_der_farbkarte() {
        for k in Karte::ALLE {
            assert_eq!(k.farbe(k.von()), farbkarte::farbe(k.von(), k.von(), k.bis()));
            assert_eq!(k.farbe(f64::NAN), farbkarte::UNGUELTIG);
            // Linear conversion: black stays 0, full brightness stays 1.
            let unten = k.farbe_linear(k.von());
            assert!(unten.iter().all(|c| (0.0..=1.0).contains(c)), "{unten:?}");
        }
        // A value far above the scale is the top colour, not an error.
        assert_eq!(Karte::Dehnung.farbe(1000.0), Karte::Dehnung.farbe(Karte::Dehnung.bis()));
        // Mid grey in, lighter grey out: the ramp's middle is not the ramp's end.
        let mitte = Karte::Passform.farbe(100.0);
        assert_ne!(mitte, Karte::Passform.farbe(PASSFORM_VON));
        assert_ne!(mitte, Karte::Passform.farbe(PASSFORM_BIS));
    }

    #[test]
    fn ohne_koerper_gibt_es_keine_abstandskarte() {
        let s = Szene::aus_netz(
            &klon3d_sim::roehre::rohr_netz(0.56, 0.28, 10.0).expect("Rechteck"),
            &|_| klon3d_sim::Teilvorgabe {
                material: klon3d_sim::RADICAL_RUBBER_04,
                reduktion: 0.0,
            },
            &|_, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0],
            Vec::new(),
            klon3d_sim::Simparameter::default(),
        );
        assert!(Karte::Abstand.messen(&s).is_none());
        // Strain needs no body, and so neither does the strain half of the fit map.
        assert!(Karte::Dehnung.messen(&s).is_some());
        assert!(Karte::Passform.messen(&s).is_some());
    }
}
