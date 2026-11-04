//! The inspection maps of the 3D view: one value and one colour per cloth point.
//!
//! Stage 6 asks for strain, fit and distance as colours on the garment. The measurements
//! themselves are `klon3d-sim`'s ([`klon3d_sim::dehnung`], [`klon3d_sim::abstand`]), and they are
//! the same ones `klon3d anziehen --farben` writes into a coloured OBJ. This module only decides
//! which of them the view shows, where its scale starts and ends, and how a value becomes a
//! colour.
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

use klon3d_sim::abstand::Karte as Abstandskarte;
use klon3d_sim::dehnung::Karte as Dehnungskarte;
use klon3d_sim::farbkarte::{self, Rgb};
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
}

impl Karte {
    /// All maps, in the order the menu lists them.
    pub const ALLE: [Karte; 3] = [Karte::Dehnung, Karte::Passform, Karte::Abstand];

    /// The measure's name, as the menu and the toolbar write it.
    pub fn name(self) -> &'static str {
        match self {
            Karte::Dehnung => "Dehnung",
            Karte::Passform => "Passform",
            Karte::Abstand => "Abstand",
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
        }
    }

    /// The unit the values are in.
    pub fn einheit(self) -> &'static str {
        match self {
            Karte::Dehnung | Karte::Passform => "%",
            Karte::Abstand => "mm",
        }
    }

    /// Lower end of the colour scale (blue).
    pub fn von(self) -> f64 {
        match self {
            Karte::Dehnung => farbkarte::DEHNUNG_VON,
            Karte::Passform => PASSFORM_VON,
            Karte::Abstand => farbkarte::ABSTAND_VON,
        }
    }

    /// Upper end of the colour scale (red).
    pub fn bis(self) -> f64 {
        match self {
            Karte::Dehnung => farbkarte::DEHNUNG_BIS,
            Karte::Passform => PASSFORM_BIS,
            Karte::Abstand => farbkarte::ABSTAND_BIS,
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
        };
        Some(Kartenwerte { karte: self, je_punkt })
    }

    /// Colour of one value on this map's ramp, as the bytes the ramp was defined with.
    pub fn farbe(self, wert: f64) -> Rgb {
        match self {
            Karte::Dehnung => farbkarte::dehnung(wert),
            Karte::Passform => farbkarte::farbe(wert, PASSFORM_VON, PASSFORM_BIS),
            Karte::Abstand => farbkarte::abstand(wert),
        }
    }

    /// The same colour as a linear RGB triple, for the shader.
    ///
    /// The ramp is defined in sRGB bytes (it is also written into an OBJ and drawn in the legend),
    /// the render target is sRGB and the shader works in linear light, so the bytes have to be
    /// converted once. Without this the garment would be visibly darker than the legend beside it.
    pub fn farbe_linear(self, wert: f64) -> [f32; 3] {
        let c = self.farbe(wert);
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
}

impl Kartenwerte {
    /// Colour per cloth point, linear RGB for the vertex buffer.
    pub fn farben(&self) -> Vec<[f32; 3]> {
        self.je_punkt
            .iter()
            .map(|&w| self.karte.farbe_linear(w))
            .collect()
    }
}

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
