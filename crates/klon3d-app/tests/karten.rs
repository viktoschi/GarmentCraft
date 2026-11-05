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
//!    exactly once;
//! 4. **the stress map** — the same values `klon3d_sim::spannung` measures, the hand-computed
//!    41.79 N/m of 12 % strain on the documented latex, and two pieces of different stiffness in
//!    one scene keeping their own force (the case that separates stress from "strain in N/m");
//! 5. **the movable scale** — a moved upper end changes the colour of a fixed value and nothing
//!    else, and the reset puts back the exact bytes; the stored text round-trips and refuses an end
//!    at or below the lower one.
//!
//! The scene is a tube on a cylinder: a real garment with a real body, no CLO data needed, and no
//! solver run – the maps are measured on the frame the scene is built with, which is what the
//! window shows while the simulation is held.

use klon3d_app::karten::{Karte, Kartenwerte, Skalen, KOMFORT_ABSTAND_MM};
use klon3d_sim::abstand::Karte as Abstandskarte;
use klon3d_sim::dehnung::{Karte as Dehnungskarte, UEBER_DEHNUNG};
use klon3d_sim::farbkarte;
use klon3d_sim::roehre::Roehre;
use klon3d_sim::spannung::Karte as Spannungskarte;
use klon3d_sim::{Simparameter, Szene};

/// The documented membrane stiffness of the reference latex (N/m), as `MATERIAL.md` states it.
const E_LATEX: f64 = 348.274_156_2;

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

/// A flat rectangle stretched in x by `faktor`, with the given stiffness: a pure in-plane stretch,
/// so the principal strains are `faktor − 1` and 0 and every face pulls equally.
///
/// The scene is the one `klon3d-sim/tests/spannung.rs` measures; it needs no body and no solving,
/// only the rest state and the placed state.
fn flach(faktor: f64, steifigkeit: f64) -> Szene {
    let netz = klon3d_sim::roehre::rohr_netz(0.56, 0.28, 10.0).expect("Rechteck vernetzbar");
    let material = klon3d_sim::Material {
        dehnung_n_m: steifigkeit,
        ..klon3d_sim::RADICAL_RUBBER_04
    };
    let vg = klon3d_sim::szene::Teilvorgabe {
        material,
        reduktion: 0.0,
    };
    let lage = move |_: usize, q: [f64; 2]| [q[0] * 1e-3 * faktor, q[1] * 1e-3, 0.0];
    Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), Simparameter::default())
}

/// 1d. Stress: the app's map is the reference map, point by point – the same bit-for-bit promise as
/// strain and distance, for the map that reads a material property.
#[test]
fn spannung_je_punkt_ist_die_karte_von_klon3d_sim() {
    let s = roehre();
    let soll = Spannungskarte::messe(&s).je_punkt(&s);
    let haben = werte(&s, Karte::Spannung);
    assert_eq!(haben.karte, Karte::Spannung);
    assert_eq!(haben.je_punkt.len(), s.x.len());
    assert_eq!(haben.je_punkt.len(), soll.len());
    for (i, (&h, &w)) in haben.je_punkt.iter().zip(&soll).enumerate() {
        assert_eq!(h.to_bits(), w.to_bits(), "Punkt {i}: {h} gegen {w}");
    }
    // The one material of this scene has a usable stiffness, so nothing is counted as unmeasurable.
    assert_eq!(haben.ohne_steifigkeit, 0);
    // The tube is stretched by the reduction: the map is not all zeros, and no point is negative
    // (compression lives in the smaller principal force, which the map does not show).
    assert!(haben.je_punkt.iter().any(|v| *v > 0.0));
    assert!(haben.je_punkt.iter().all(|v| *v >= 0.0));
}

/// 2c. The number is the number, through the app's own [`Karte::messen`]: 12 % strain on the
/// documented latex reads 0.12 × 348.274 = 41.79 N/m, computed by hand from `MATERIAL.md`.
///
/// That the value is above the 35 N/m default end is the reason the map is red without structure
/// here – and the reason the scale can be moved (test 5).
#[test]
fn zwoelf_prozent_lesen_sich_durch_die_app_als_41_79_n_m() {
    let s = flach(1.12, E_LATEX);
    let soll = 0.12 * E_LATEX;
    assert!((soll - 41.792_898_744).abs() < 1e-9, "Handwert: {soll}");

    let haben = werte(&s, Karte::Spannung);
    for (i, v) in haben.je_punkt.iter().enumerate() {
        assert!(
            (v - soll).abs() < 1e-6,
            "Punkt {i}: {v:.6} N/m statt {soll:.6} N/m"
        );
    }
    // A flat stretch pulls the same everywhere, so the map has no structure here: every point is
    // beyond the default warm end and reads as the top colour.
    assert!(
        haben.je_punkt.iter().all(|&v| v > Karte::Spannung.bis()),
        "41,79 N/m liegt über dem Vorgabe-Ende {} N/m",
        Karte::Spannung.bis()
    );
    for (i, v) in haben.je_punkt.iter().enumerate() {
        assert_eq!(
            Karte::Spannung.farbe(*v),
            Karte::Spannung.farbe(Karte::Spannung.bis()),
            "Punkt {i}"
        );
    }
}

/// 2d. The case that separates the stress map from "strain with other axis labels": two pieces of
/// **different stiffness in one scene** under the same stretch keep their own force, and neither
/// inherits the neighbour's value.
///
/// The scene is the same rectangle twice, offset in x so the pieces do not overlap. Nothing is
/// solved; only the map is measured, which is exactly what the app colours.
#[test]
fn zwei_teile_mit_verschiedener_steifigkeit_halten_ihre_kraft() {
    let netz = klon3d_sim::roehre::rohr_netz(0.56, 0.28, 10.0).expect("Rechteck vernetzbar");
    let mut doppelt = netz.clone();
    let mut dick = netz.teile[0].clone();
    dick.teil = "dick".to_string();
    for p in &mut dick.punkte {
        p[0] += 800.0;
    }
    doppelt.teile.push(dick);

    let steif = klon3d_sim::Material {
        dehnung_n_m: 2.0 * E_LATEX,
        ..klon3d_sim::RADICAL_RUBBER_04
    };
    let vg = |name: &str| klon3d_sim::szene::Teilvorgabe {
        material: if name == "dick" {
            steif
        } else {
            klon3d_sim::RADICAL_RUBBER_04
        },
        reduktion: 0.0,
    };
    let lage = move |_: usize, q: [f64; 2]| [q[0] * 1e-3 * 1.12, q[1] * 1e-3, 0.0];
    let s = Szene::aus_netz(&doppelt, &vg, &lage, Vec::new(), Simparameter::default());
    assert_eq!(s.teile.len(), 2, "zwei Teile erwartet");

    // The strain map cannot tell the two apart: this is not a different deformation.
    let dehnung = werte(&s, Karte::Dehnung);
    let haben = werte(&s, Karte::Spannung);
    assert_eq!(haben.ohne_steifigkeit, 0);

    let duenn_spanne = s.teile[0].punkte.clone();
    let dick_spanne = s.teile[1].punkte.clone();
    let mittel = |spanne: &std::ops::Range<usize>| {
        let we = &haben.je_punkt[spanne.clone()];
        we.iter().sum::<f64>() / we.len() as f64
    };
    let (duenn, dick) = (mittel(&duenn_spanne), mittel(&dick_spanne));
    let soll = 0.12 * E_LATEX;
    assert!((duenn - soll).abs() < 1e-6, "dünn: {duenn:.6}");
    assert!((dick - 2.0 * soll).abs() < 1e-6, "dick: {dick:.6}");
    // Same strain, double stiffness → double force. A stress map that were strain in other units
    // would fail right here with two equal numbers.
    assert!(
        (dick / duenn - 2.0).abs() < 1e-9,
        "gleiche Dehnung, doppelte Steifigkeit → doppelte Kraft: {duenn:.6} / {dick:.6}"
    );
    for (i, &d) in dehnung.je_punkt.iter().enumerate() {
        assert!((d - 12.0).abs() < 1e-6, "Dehnung muss 12 % bleiben (Punkt {i}): {d}");
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
                    Karte::Spannung => farbkarte::spannung(*v),
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

/// 4. The scales start on the constants the maps were designed with – one number each, taken from
/// [`Karte::bis`] and not repeated in a menu – and moving one leaves every other map alone.
#[test]
fn jede_karte_startet_auf_ihrer_vorgabe() {
    let s = Skalen::default();
    for karte in Karte::ALLE {
        assert_eq!(s.bis(karte), karte.bis(), "{karte:?}");
        assert!(s.ist_vorgabe(karte), "{karte:?}");
        assert_eq!(s.als_text(), "", "nichts verschoben, nichts gespeichert");
    }
    assert_eq!(Karte::ALLE.len(), 4, "vier Karten");
    for karte in Karte::ALLE {
        assert!(!karte.name().is_empty() && !karte.einheit().is_empty(), "{karte:?}");
        assert!(karte.bis() > karte.von(), "{karte:?}");
    }

    let mut s = Skalen::default();
    s.setzen(Karte::Spannung, 80.0);
    assert_eq!(s.bis(Karte::Spannung), 80.0);
    assert!(!s.ist_vorgabe(Karte::Spannung));
    for andere in Karte::ALLE.into_iter().filter(|k| *k != Karte::Spannung) {
        assert!(s.ist_vorgabe(andere), "{andere:?} darf unberührt bleiben");
    }
}

/// 5. Moving the upper end re-reads the same numbers: the value is untouched, only its colour
/// moves. The reset puts back the exact bytes of the default ramp.
///
/// This is the whole point of the feature – a garment whose values sit above the default end is a
/// flat red field, and a higher end shows its structure without a new solver frame.
#[test]
fn verschobenes_ende_aendert_die_farbe_nicht_den_wert() {
    let s = roehre();
    for karte in Karte::ALLE {
        let w = werte(&s, karte);
        let stand = w.farben();
        // A value in the middle of the default ramp is the one whose colour is easy to move: the
        // extremes clamp at both ends whatever the scale does.
        let mitte = (karte.von() + karte.bis()) / 2.0;

        let mut skalen = Skalen::default();
        skalen.setzen(karte, karte.bis() * 2.0);
        let weiter = w.farben_bis(skalen.bis(karte));
        // Same numbers, one colour per point – nothing was re-measured.
        assert_eq!(weiter.len(), stand.len());
        assert_eq!(w.je_punkt, werte(&s, karte).je_punkt);

        // Half way up a ramp that now ends twice as high is a cooler colour than before.
        assert_eq!(
            karte.farbe_bis(mitte, skalen.bis(karte)),
            farbkarte::farbe(mitte, karte.von(), karte.bis() * 2.0),
            "{karte:?}"
        );
        assert_ne!(
            karte.farbe_bis(mitte, skalen.bis(karte)),
            karte.farbe(mitte),
            "{karte:?}: das verschobene Ende muss die Farbe ändern"
        );

        // Back on the default: byte for byte the ramp the map was designed with.
        skalen.zuruecksetzen(karte);
        assert!(skalen.ist_vorgabe(karte), "{karte:?}");
        assert_eq!(skalen.bis(karte), karte.bis(), "{karte:?}");
        assert_eq!(w.farben_bis(skalen.bis(karte)), stand, "{karte:?}");
        assert_eq!(
            w.farben_bis(skalen.bis(karte)),
            w.farben(),
            "{karte:?}: farben_bis(Vorgabe) und farben() sind dasselbe"
        );
    }
}

/// 5b. The stored text: only moved ends are written, and reading it back gives the same numbers.
/// A settings file from before the feature is empty and every map keeps its constant.
#[test]
fn skalen_gehen_als_text_hin_und_zurueck() {
    let mut s = Skalen::default();
    s.setzen(Karte::Spannung, 80.0);
    s.setzen(Karte::Abstand, 12.5);
    let text = s.als_text();
    assert!(text.contains("spannung="), "{text}");
    assert!(text.contains("abstand="), "{text}");
    assert!(
        !text.contains("dehnung=") && !text.contains("passform="),
        "nur verschobene Enden werden geschrieben: {text}"
    );

    let zurueck = Skalen::aus_text(&text);
    for karte in Karte::ALLE {
        assert_eq!(zurueck.bis(karte), s.bis(karte), "{karte:?}");
    }
    assert_eq!(zurueck, s);

    // Empty, unknown and unreadable entries keep the default.
    for text in ["", "  ", "unfug", "spannung=", "spannung=abc", "gibtsnicht=5"] {
        let s = Skalen::aus_text(text);
        for karte in Karte::ALLE {
            assert!(s.ist_vorgabe(karte), "{text:?}: {karte:?}");
        }
    }
    // A stored end at or below the lower end is refused, not taken as a ramp of no width.
    for text in ["spannung=0", "spannung=-5", "spannung=NaN", "spannung=inf"] {
        let s = Skalen::aus_text(text);
        assert!(s.ist_vorgabe(Karte::Spannung), "{text:?}");
    }
}

/// 5c. [`Skalen::setzen`] refuses what would leave no ramp: a value at or below the lower end, and
/// a value that is not finite. The previous end stays.
#[test]
fn skalen_lehnen_ein_ende_ohne_rampe_ab() {
    let mut s = Skalen::default();
    s.setzen(Karte::Dehnung, 30.0);
    for schlecht in [
        Karte::Dehnung.von(),
        Karte::Dehnung.von() - 1.0,
        -100.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        s.setzen(Karte::Dehnung, schlecht);
        assert_eq!(s.bis(Karte::Dehnung), 30.0, "{schlecht} wurde angenommen");
    }
    // And a good one is still taken afterwards.
    s.setzen(Karte::Dehnung, 25.0);
    assert_eq!(s.bis(Karte::Dehnung), 25.0);
    s.zuruecksetzen(Karte::Dehnung);
    assert_eq!(s.bis(Karte::Dehnung), Karte::Dehnung.bis());
}

/// 5d. A piece without a usable stiffness reads 0 N/m everywhere and is counted, because 0 N/m from
/// a missing material and 0 N/m from slack cloth are the same blue – the legend has to name it.
#[test]
fn teil_ohne_steifigkeit_wird_gezaehlt() {
    let s = flach(1.12, E_LATEX);
    let ohne = flach(1.12, 0.0);
    let w = werte(&ohne, Karte::Spannung);
    assert_eq!(w.ohne_steifigkeit, 1, "ein Teil ohne Steifigkeit");
    assert!(w.je_punkt.iter().all(|v| *v == 0.0));
    // The same scene with a material reads a real force and counts nothing.
    let mit = werte(&s, Karte::Spannung);
    assert_eq!(mit.ohne_steifigkeit, 0);
    assert!(mit.je_punkt.iter().all(|v| *v > 0.0));
    // Only the stress map counts stiffness; the others stay at 0 even without a material.
    assert_eq!(werte(&ohne, Karte::Dehnung).ohne_steifigkeit, 0);
}
