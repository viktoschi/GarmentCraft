//! Acceptance for the inspection maps of the 3D view (stage 6, task #58).
//!
//! The maps the window paints are not measured by the window: `klon3d-sim` measures them, and the
//! app only picks the measure, its scale and its colour. This test pins that down – the values the
//! app hands to the vertex buffer are bit-for-bit the ones `klon3d-sim` reports for the same scene,
//! so a colour on screen and a row in `klon3d anziehen --farben` can never disagree.
//!
//! Checked in this order:
//!
//! 1. **the same values** — strain, distance and fit per cloth point, against the reference
//!    measures of `klon3d_sim::dehnung`, `klon3d_sim::abstand` and the fit definition written
//!    down in `src/karten.rs`;
//! 2. **the fit map's definition** — the worse of the two halves, 100 % at the comfort limit, and
//!    the strain half alone when there is no body;
//! 3. **the vertex colours** — one colour per point, from the fixed ramp, converted to linear light
//!    exactly once.
//!
//! The scene is a tube on a cylinder: a real garment with a real body, no CLO data needed, and no
//! solver run – the maps are measured on the frame the scene is built with, which is what the
//! window shows while the simulation is held.

use klon3d_app::karten::{Karte, Kartenwerte, KOMFORT_ABSTAND_MM};
use klon3d_sim::abstand::Karte as Abstandskarte;
use klon3d_sim::dehnung::{Karte as Dehnungskarte, UEBER_DEHNUNG};
use klon3d_sim::farbkarte;
use klon3d_sim::roehre::Roehre;
use klon3d_sim::{Simparameter, Szene};

/// The tube on the 90 mm cylinder: 5 mm of air plus half the fabric thickness, the same scene
/// `klon3d-sim/tests/abstand.rs` measures.
fn roehre() -> Szene {
    Roehre {
        reduktion: 0.055,
        ..Roehre::default()
    }
    .szene(Simparameter::default())
    .expect("Röhre baubar")
}

/// Values the app hands to the shader for `karte`, as `(karte, je_punkt)`.
fn werte(s: &Szene, karte: Karte) -> Kartenwerte {
    karte.messen(s).expect("Karte messbar")
}

/// 1a. Strain: the app's map is the reference map, point by point.
#[test]
fn dehnung_je_punkt_ist_die_karte_von_klon3d_sim() {
    let s = roehre();
    let soll = Dehnungskarte::messe(&s).je_punkt(&s);
    let haben = werte(&s, Karte::Dehnung);
    assert_eq!(haben.karte, Karte::Dehnung);
    assert_eq!(haben.je_punkt.len(), s.x.len());
    assert_eq!(haben.je_punkt.len(), soll.len());
    for (i, (&h, &w)) in haben.je_punkt.iter().zip(&soll).enumerate() {
        assert_eq!(h.to_bits(), w.to_bits(), "Punkt {i}: {h} gegen {w}");
    }
    // The tube is stretched a little by the reduction: the map is not all zeros.
    assert!(haben.je_punkt.iter().any(|v| *v != 0.0));
}

/// 1b. Distance: the reference map in metres, the app's in millimetres.
#[test]
fn abstand_je_punkt_ist_die_karte_von_klon3d_sim() {
    let s = roehre();
    let soll = Abstandskarte::messe(&s).expect("Körper vorhanden");
    let haben = werte(&s, Karte::Abstand);
    assert_eq!(haben.karte, Karte::Abstand);
    assert_eq!(haben.je_punkt.len(), soll.je_punkt.len());
    for (i, (&h, p)) in haben.je_punkt.iter().zip(&soll.je_punkt).enumerate() {
        assert_eq!(h.to_bits(), (p.abstand * 1e3).to_bits(), "Punkt {i}");
    }
    // Every point sits 5 mm of air plus half the thickness off the skin.
    for (i, v) in haben.je_punkt.iter().enumerate() {
        assert!(
            (v - 5.2).abs() < 1e-9,
            "Punkt {i}: {v:.6} mm statt 5,2 mm"
        );
    }
}

/// 2. The fit map: the worse of the two halves, each as a share of its tolerance.
#[test]
fn passform_ist_der_schlechtere_der_beiden_anteile() {
    let s = roehre();
    let dehnung = Dehnungskarte::messe(&s).je_punkt(&s);
    let abstand = Abstandskarte::messe(&s).expect("Körper vorhanden").je_punkt;
    let haben = werte(&s, Karte::Passform);
    assert_eq!(haben.karte, Karte::Passform);
    assert_eq!(haben.je_punkt.len(), dehnung.len());

    for i in 0..dehnung.len() {
        let aus_dehnung = dehnung[i] / (UEBER_DEHNUNG * 100.0) * 100.0;
        let aus_abstand = abstand[i].abstand.abs() * 1e3 / KOMFORT_ABSTAND_MM * 100.0;
        let soll = aus_dehnung.max(aus_abstand);
        assert_eq!(haben.je_punkt[i].to_bits(), soll.to_bits(), "Punkt {i}");
        // 5,2 mm of air is more than the 5 mm tolerance (104 %), and the tube is stretched past
        // the 10 % comfort strain as well – here the strain half is the one that decides.
        assert!(
            aus_dehnung > aus_abstand,
            "Punkt {i}: Dehnung {aus_dehnung:.3} % gegen Abstand {aus_abstand:.3} %"
        );
        assert!(haben.je_punkt[i] > 100.0, "Punkt {i}: {}", haben.je_punkt[i]);
    }
}

/// 2b. Without a body the fit map falls back to its strain half – and says so in the scale, which
/// is the reason [`Karte::braucht_koerper`] is false for it.
#[test]
fn passform_ohne_koerper_ist_die_dehnungshaelfte() {
    let mut s = roehre();
    s.koerper.clear();
    assert!(Karte::Abstand.messen(&s).is_none());

    let dehnung = Dehnungskarte::messe(&s).je_punkt(&s);
    let haben = werte(&s, Karte::Passform);
    for (i, (&h, &d)) in haben.je_punkt.iter().zip(&dehnung).enumerate() {
        let soll = d / (UEBER_DEHNUNG * 100.0) * 100.0;
        assert_eq!(h.to_bits(), soll.to_bits(), "Punkt {i}");
    }
}

/// 3. One colour per point, from the fixed ramp, converted to linear light exactly once: the bytes
/// are the ramp's, and the triple is those bytes through the sRGB curve.
#[test]
fn farben_sind_die_fixierte_skala_in_linearem_licht() {
    let s = roehre();
    for karte in Karte::ALLE {
        let w = werte(&s, karte);
        let farben = w.farben();
        assert_eq!(farben.len(), w.je_punkt.len());

        // The bytes, as the legend and `anziehen --farben` use them.
        for (i, v) in w.je_punkt.iter().enumerate() {
            let bytes = karte.farbe(*v);
            assert_eq!(
                karte.farbe(*v),
                match karte {
                    Karte::Dehnung => farbkarte::dehnung(*v),
                    Karte::Passform => farbkarte::farbe(*v, karte.von(), karte.bis()),
                    Karte::Abstand => farbkarte::abstand(*v),
                },
                "{karte:?} Punkt {i}"
            );
            // And the shader triple is those bytes, sRGB → linear.
            let linear: [f32; 3] = bytes.map(|k| {
                let x = k as f32 / 255.0;
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            });
            for c in 0..3 {
                assert!(
                    (farben[i][c] - linear[c]).abs() < 1e-7,
                    "{karte:?} Punkt {i} Kanal {c}: {} gegen {}",
                    farben[i][c],
                    linear[c]
                );
            }
        }
    }
}
