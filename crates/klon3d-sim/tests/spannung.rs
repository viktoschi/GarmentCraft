//! Acceptance for the stress map (stage 5, task #23).
//!
//! `PLAN.md` asks for "where the material pulls". The strain map says how far;
//! this one says how hard, in N/m. The tests are built around the one thing
//! that separates the two maps, because everything else would also pass for
//! "strain times a constant":
//!
//! 1. the number itself — a flat piece at 12 % with the documented latex
//!    stiffness reads `0.12 × 348.274 = 41.79 N/m`, hand-computed;
//! 2. the second principal force of a pure in-plane stretch is 0, and
//!    compression is negative;
//! 3. **two thicknesses, one strain → different force.** This is the test that
//!    fails if the stress map degenerates into "strain with another unit";
//! 4. **two pieces in one scene** keep their own stiffness — the case
//!    `anziehen` actually builds, with the material chosen per piece;
//! 5. the per-piece threshold is the piece's own 10 % line, not a global one;
//! 6. the colour scale agrees with the material, not with the largest value.
//!
//! All six run without CLO data.

use klon3d_sim::dehnung::Karte as Dehnungskarte;
use klon3d_sim::farbkarte::{spannung, SPANNUNG_BIS, SPANNUNG_VON};
use klon3d_sim::roehre::rohr_netz;
use klon3d_sim::spannung::{Karte, UEBER_SPANNUNG};
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{Simparameter, Szene, RADICAL_RUBBER_04};

/// The documented membrane stiffness of the reference latex (N/m).
const E_LATEX: f64 = 348.274_156_2;

/// A flat rectangle stretched in x by `faktor`, with the given stiffness.
///
/// The pattern is 560 × 280 mm; the scene places it flat in the z = 0 plane
/// with x scaled, which is a pure in-plane stretch: the principal strains are
/// `faktor − 1` and 0, no shear.
fn flach(faktor: f64, steifigkeit: f64) -> Szene {
    let netz = rohr_netz(0.56, 0.28, 10.0).expect("rectangle meshable");
    let material = klon3d_sim::Material {
        dehnung_n_m: steifigkeit,
        kette_n_m: steifigkeit,
        ..RADICAL_RUBBER_04
    };
    let vg = Teilvorgabe {
        material,
        reduktion: 0.0,
    };
    let lage = move |_: usize, q: [f64; 2]| [q[0] * 1e-3 * faktor, q[1] * 1e-3, 0.0];
    Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), Simparameter::default())
}

/// 1. The number is the number: 12 % strain on the documented latex gives
/// 41.79 N/m, computed by hand from `MATERIAL.md`, not read back.
#[test]
fn zwoelf_prozent_lesen_sich_als_die_kraft() {
    let s = flach(1.12, E_LATEX);
    let k = Karte::messe(&s);
    let g = k.gesamt();
    let soll = 0.12 * E_LATEX;
    assert!((soll - 41.792_898_744).abs() < 1e-9, "hand value: {soll}");
    assert!(
        (g.median - soll).abs() < 1e-6,
        "median {:.6} against {soll:.6} N/m",
        g.median
    );
    assert!((g.max - soll).abs() < 1e-6, "max {:.6}", g.max);
    // Every face is stretched the same in a pure in-plane stretch.
    assert!(
        k.je_flaeche
            .iter()
            .all(|f| (f.groesste - soll).abs() < 1e-6),
        "a flat stretch is the same everywhere"
    );
}

/// 2. The smaller principal force of a pure in-plane stretch is zero up to
/// floating point, and compression comes out clearly negative rather than
/// clamped away.
///
/// The zero is not exact: the principal values come from a closed-form
/// eigenvalue of `FᵀF`, and a value that is analytically 0 lands within about
/// 1e-13 N/m of it. That is stated here instead of being tightened into a
/// meaningless assertion — the scale of the number is 1e-13 against forces of
/// tens of N/m.
#[test]
fn zweite_richtung_ist_null_und_druck_ist_negativ() {
    let s = flach(1.12, E_LATEX);
    let k = Karte::messe(&s);
    for f in &k.je_flaeche {
        assert!(
            f.kleinste.abs() < 1e-9,
            "smaller force {:e} must be 0 up to rounding",
            f.kleinste
        );
        // And it is not a compression: nothing pulls inward in a flat stretch.
        assert!(
            f.kleinste > -1e-9,
            "must not be compressive: {:e}",
            f.kleinste
        );
    }

    // Compression: shrink the piece; the larger principal force stays at 0
    // (compression lives in the smaller one), which is the sign convention.
    let gestaucht = flach(0.88, E_LATEX);
    let k = Karte::messe(&gestaucht);
    let f = k.je_flaeche[0];
    assert!(
        f.kleinste < 0.0,
        "compression must be negative: {}",
        f.kleinste
    );
    assert!(
        (f.kleinste + 0.12 * E_LATEX).abs() < 1e-6,
        "-41.79 expected: {}",
        f.kleinste
    );
    assert!(
        f.groesste.abs() < 1e-9,
        "larger force of a compression is 0: {}",
        f.groesste
    );
}

/// 3. The test that separates this map from the strain map: **one strain, two
/// thicknesses.** The strain values are identical, the forces differ by exactly
/// the thickness ratio. A stress map that is "strain × one constant" fails here.
#[test]
fn gleiche_dehnung_verschiedene_dicke_andere_kraft() {
    let s = flach(1.12, E_LATEX);
    let s_doppelt = flach(1.12, 2.0 * E_LATEX);

    let d = Dehnungskarte::messe(&s);
    let d_doppelt = Dehnungskarte::messe(&s_doppelt);
    let k = Karte::messe(&s);
    let k_doppelt = Karte::messe(&s_doppelt);

    // The strain maps agree: this is not a different deformation.
    let a = d.gesamt().median;
    let b = d_doppelt.gesamt().median;
    assert!(
        (a - b).abs() < 1e-9,
        "strain must not move: {a:.9} / {b:.9}"
    );

    // The stress maps do not: exactly twice.
    let ka = k.gesamt().median;
    let kb = k_doppelt.gesamt().median;
    assert!(
        (kb / ka - 2.0).abs() < 1e-9,
        "twice the thickness must double the force: {ka:.6} / {kb:.6}"
    );
    // And the shape of the map is unchanged — same faces, same order.
    assert_eq!(k.je_flaeche.len(), k_doppelt.je_flaeche.len());
    assert_eq!(k.je_teil[0].name, k_doppelt.je_teil[0].name);
}

/// 4. Two pieces **in one scene** under the same stretch: the thin piece pulls
/// with its stiffness, the thick one with double — and neither inherits the
/// other's value. This is the real case `anziehen` builds (material per piece
/// by thickness), and the one a single-material test cannot see.
///
/// The scene is the same rectangle twice, in two meshes-side-by-side pieces, so
/// both experience identical strain while carrying different materials.
#[test]
fn zwei_teile_in_einer_szene_halten_ihre_steifigkeit() {
    let netz = rohr_netz(0.56, 0.28, 10.0).expect("rectangle meshable");
    // Two pieces: duplicate the one the tube mesh gives us, offset in x so the
    // two do not overlap. Nothing simulates here, only the map is measured.
    let mut doppelt = netz.clone();
    let mut b = netz.teile[0].clone();
    b.teil = "dick".to_string();
    for p in &mut b.punkte {
        p[0] += 800.0;
    }
    doppelt.teile.push(b);
    let dünn_ende = netz.teile[0].punkte.len();

    let dick = klon3d_sim::Material {
        dehnung_n_m: 2.0 * E_LATEX,
        kette_n_m: 2.0 * E_LATEX,
        ..RADICAL_RUBBER_04
    };
    let vg = |name: &str| Teilvorgabe {
        material: if name == "dick" {
            dick
        } else {
            RADICAL_RUBBER_04
        },
        reduktion: 0.0,
    };
    let lage = move |_: usize, q: [f64; 2]| [q[0] * 1e-3 * 1.12, q[1] * 1e-3, 0.0];
    let s = Szene::aus_netz(&doppelt, &vg, &lage, Vec::new(), Simparameter::default());
    assert!(
        s.teile.len() >= 2,
        "two pieces expected, got {}",
        s.teile.len()
    );
    assert!(dünn_ende > 0);

    let k = Karte::messe(&s);
    let je_teil: Vec<&klon3d_sim::spannung::Teil> = k.je_teil.iter().collect();
    let duenn = je_teil
        .iter()
        .find(|t| t.name != "dick")
        .expect("thin piece");
    let thick = je_teil
        .iter()
        .find(|t| t.name == "dick")
        .expect("thick piece");

    // Stiffness is the piece's own, not the neighbour's.
    assert!(
        (duenn.steifigkeit_n_m - E_LATEX).abs() < 1e-9,
        "thin: {}",
        duenn.steifigkeit_n_m
    );
    assert!(
        (thick.steifigkeit_n_m - 2.0 * E_LATEX).abs() < 1e-9,
        "thick: {}",
        thick.steifigkeit_n_m
    );
    // Same strain, so the force ratio is the stiffness ratio.
    assert!(
        (thick.verteilung.median / duenn.verteilung.median - 2.0).abs() < 1e-9,
        "same strain, double stiffness → double force: {:.6} / {:.6}",
        duenn.verteilung.median,
        thick.verteilung.median
    );
    // And the per-face stiffness matches the piece that owns the face.
    for (i, f) in k.je_flaeche.iter().enumerate() {
        let owner = s
            .teile
            .iter()
            .find(|t| t.dreiecke.contains(&i))
            .expect("face has an owner");
        let erwartet = if owner.name == "dick" {
            2.0 * E_LATEX
        } else {
            E_LATEX
        };
        assert!(
            (f.steifigkeit_n_m - erwartet).abs() < 1e-9,
            "face {i} in {}: {} instead of {erwartet}",
            owner.name,
            f.steifigkeit_n_m
        );
    }
}

/// 5. Each piece carries its own 10 % line, and the count above it uses that
/// line. A thicker piece has a higher line, so the same strain does not count
/// as "over" for it.
#[test]
fn die_grenze_gehoert_zum_teil() {
    let duenn = flach(1.12, E_LATEX);
    let dick = flach(1.12, 2.0 * E_LATEX);

    let kd = Karte::messe(&duenn);
    let kk = Karte::messe(&dick);

    assert!((kd.je_teil[0].grenze_n_m - 0.10 * E_LATEX).abs() < 1e-9);
    assert!((kk.je_teil[0].grenze_n_m - 2.0 * 0.10 * E_LATEX).abs() < 1e-9);
    assert!(
        (kk.je_teil[0].grenze_n_m / kd.je_teil[0].grenze_n_m - 2.0).abs() < 1e-12,
        "the line scales with the stiffness"
    );
    // 12 % is above the 10 % line for both, and the count says so in both.
    assert!(kd.je_teil[0].ueber > 0, "thin piece: 12 % is over 10 %");
    assert!(kk.je_teil[0].ueber > 0, "thick piece: 12 % is over 10 %");
    // At 5 % neither is over.
    for s in [flach(1.05, E_LATEX), flach(1.05, 2.0 * E_LATEX)] {
        let k = Karte::messe(&s);
        assert_eq!(k.je_teil[0].ueber, 0, "5 % is below the line");
    }
}

/// 5. The colour scale is fixed and starts at the 10 % line of the reference
/// latex, not at the largest value of the run.
#[test]
fn die_farbskala_haengt_am_material_nicht_am_lauf() {
    // The warm end is the documented 10 % line of the reference latex.
    assert!(
        (SPANNUNG_BIS - E_LATEX * UEBER_SPANNUNG).abs() < 0.2,
        "{SPANNUNG_BIS} against {}",
        E_LATEX * UEBER_SPANNUNG
    );
    assert_eq!(SPANNUNG_VON, 0.0);

    // 12 % strain of that latex (41.79 N/m) is past the warm end, so it clamps
    // there: the map does not rescale to make the worst face look mid-ramp.
    let kraft = 0.12 * E_LATEX;
    assert!(kraft > SPANNUNG_BIS, "{kraft} must be above the scale");
    assert_eq!(spannung(kraft), spannung(SPANNUNG_BIS));
    assert_eq!(spannung(0.0), spannung(SPANNUNG_VON));

    // Two different runs at the same force read the same colour — the scale
    // does not depend on the scene.
    let a = Karte::messe(&flach(1.12, E_LATEX)).gesamt().median;
    let b = Karte::messe(&flach(1.06, 2.0 * E_LATEX)).gesamt().median;
    assert!(
        (a - b).abs() < 1e-6,
        "same force by construction: {a} / {b}"
    );
    assert_eq!(spannung(a), spannung(b));

    // Blue falls from cold to warm without rising.
    let mut vorher = spannung(SPANNUNG_VON);
    for i in 0..=200 {
        let v = SPANNUNG_VON + (SPANNUNG_BIS - SPANNUNG_VON) * i as f64 / 200.0;
        let c = spannung(v);
        assert!(c[2] <= vorher[2], "blue rises at {v:.2}");
        vorher = c;
    }
}
