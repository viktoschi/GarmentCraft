//! Bending follows the grainline (stage 4, #129): the same strip, bent over the
//! same cylinder, bends differently depending on where its warp runs.
//!
//! The unit tests in [`klon3d_sim::biegung`] pin the formula. What is checked
//! here is the path from the pattern into the solver: the grainline reaches the
//! bending constraint of a real scene, and an anisotropic fabric really does
//! bend softer across the grain than along it.
//!
//! All of it runs without CLO data.

use klon3d_mesh::{Fadenlauf, Musternetz, Teilnetz};
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{Material, Simparameter, Szene};

/// A strip 3 x 1 squares, pattern x across it and y along it: 4 columns at
/// 0/10/20/30 mm, 2 rows at 0/10 mm, six triangles.
///
/// The strip is bent around the **cylinder axis along pattern y**, so the
/// curvature runs along pattern x.
fn streifen() -> (Vec<[f64; 2]>, Vec<[u32; 3]>) {
    let mut p = Vec::new();
    for j in 0..2 {
        for i in 0..4 {
            p.push([10.0 * i as f64, 10.0 * j as f64]);
        }
    }
    // Positive orientation in pattern coordinates (y down).
    let d = vec![
        [0u32, 4, 1],
        [1, 4, 5],
        [1, 5, 2],
        [2, 5, 6],
        [2, 6, 3],
        [3, 6, 7],
    ];
    (p, d)
}

fn netz(punkte: Vec<[f64; 2]>, dreiecke: Vec<[u32; 3]>, achse: Fadenlauf) -> Musternetz {
    Musternetz {
        parameter: Default::default(),
        teile: vec![Teilnetz {
            teil: "Streifen".into(),
            punkte,
            dreiecke,
            nrand: 0,
            randort: Vec::new(),
            punktknoten: Vec::new(),
            kanten: Vec::new(),
            kontur: Vec::new(),
            schnitt: Vec::new(),
            zugabe: Default::default(),
            fadenlauf: Some(achse),
            aufgegeben: 0,
        }],
        naehte: Vec::new(),
        fehler: Vec::new(),
        runden: 0,
        offene_teilungen: 0,
    }
}

/// Wraps pattern x around a cylinder of radius `r`; pattern y runs along the
/// axis.
fn zylinder(r: f64) -> impl Fn(usize, [f64; 2]) -> [f64; 3] {
    move |_: usize, q: [f64; 2]| {
        let w = q[0] * 1e-3 / r;
        [r * w.cos(), r * w.sin(), -q[1] * 1e-3]
    }
}

/// A fabric that is four times stiffer along the warp than across it - far
/// apart on purpose, so the scene shows the direction instead of hiding it in
/// the 0.17 % the eight `assets/stoffe` files differ by.
fn gewebe() -> Material {
    Material {
        name: "Test_4zu1",
        biegung_nm: 1.0e-6,
        biegung_kette_nm: 4.0e-6,
        biegung_schraege_nm: 2.0e-6,
        ..klon3d_sim::RADICAL_RUBBER_04
    }
}

fn szene(n: &Musternetz, lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3]) -> Szene {
    let vg = Teilvorgabe {
        material: gewebe(),
        reduktion: 0.0,
    };
    Szene::aus_netz(n, &|_| vg, lage, Vec::new(), Simparameter::default())
}

/// Grainline **along** the fold axis (pattern y): the cloth is bent across its
/// grainline, so the soft weft carries the curvature.
fn laengs() -> Fadenlauf {
    Fadenlauf {
        p1: [0.0, 0.0],
        p2: [0.0, 10.0],
        pfeile: 0,
    }
}

/// Grainline **across** the fold axis (pattern x): the stiff warp runs into the
/// curvature.
fn quer() -> Fadenlauf {
    Fadenlauf {
        p1: [0.0, 0.0],
        p2: [30.0, 0.0],
        pfeile: 0,
    }
}

/// One number per run: the bending energy of the bent strip.
fn energie(achse: Fadenlauf, r: f64) -> f64 {
    let (p, d) = streifen();
    let s = szene(&netz(p, d, achse), &zylinder(r));
    s.biegeenergie()
}

/// The same geometry, only the grainline turned: a fabric bends with **less**
/// energy across its grainline than along it, because the bending energy of a
/// plate is `½·D·κ²·A` - proportional to the stiffness, not inversely.
///
/// So the ratio of the two energies is `D_weft / D_warp`, and it is the ratio of
/// the two stated stiffnesses rather than a number the mesh invents. The
/// cylinder is developable, so the bending is uniaxial: the only edges that
/// carry a dihedral angle are the ones whose bending normal points along the
/// circumference, everything else is flat - which is why the ratio comes out as
/// cleanly as it does.
#[test]
fn quer_zur_faser_biegt_weicher() {
    let r = 0.04;
    let weich = energie(laengs(), r); // curved across the grain: D_weft
    let steif = energie(quer(), r); // curved along the grain: D_warp
    let d = gewebe();
    let erwartet = d.biegung_nm / d.biegung_kette_nm; // 1/4
    let gemessen = weich / steif;
    // The number is printed so a change in the mesh shows up as a change, and
    // the tolerance is the measured one (see the assertion below).
    eprintln!(
        "quer/laengs: Energie {weich:e} / {steif:e} = {gemessen:.6} (erwartet {erwartet:.6})"
    );
    assert!(
        (gemessen - erwartet).abs() <= 0.06 * erwartet,
        "Verhaeltnis {gemessen:.4} gegen {erwartet:.4} (weich {weich:e}, steif {steif:e})"
    );
    assert!(
        weich < steif,
        "quer zur Faser muss weniger Energie kosten: {weich:e} gegen {steif:e}"
    );
}

/// A fabric whose three bending values are equal must come out **bit-identical**
/// whichever way its grainline points: the whole promise of the change is that a
/// material with three equal values behaves exactly as it did before.
///
/// `RADICAL_RUBBER_04` is deliberately *not* used here - it carries weft
/// 2482.2424 against warp = bias 2486.4924 and is therefore anisotropic by
/// 0.17 %, which is exactly the case the next test is about.
#[test]
fn isotrop_ist_der_fadenlauf_gleichgueltig() {
    let (p, d) = streifen();
    let vg = Teilvorgabe {
        material: Material {
            biegung_nm: 2.482_242_4e-6,
            biegung_kette_nm: 2.482_242_4e-6,
            biegung_schraege_nm: 2.482_242_4e-6,
            ..klon3d_sim::RADICAL_RUBBER_04
        },
        reduktion: 0.0,
    };
    let bauen = |a: Fadenlauf| {
        Szene::aus_netz(
            &netz(p.clone(), d.clone(), a),
            &|_| vg,
            &zylinder(0.04),
            Vec::new(),
            Simparameter::default(),
        )
        .biegeenergie()
    };
    let a = bauen(laengs());
    let b = bauen(quer());
    assert_eq!(a.to_bits(), b.to_bits(), "{a:e} gegen {b:e}");
    assert!(a > 0.0, "der Streifen muss sich ueberhaupt biegen");

    // And the grainline really reaches the constraint: with a stiff warp the
    // same strip costs more, so the equal case above is not equal by accident.
    let steif = Teilvorgabe {
        material: gewebe(),
        reduktion: 0.0,
    };
    let c = Szene::aus_netz(
        &netz(p, d, quer()),
        &|_| steif,
        &zylinder(0.04),
        Vec::new(),
        Simparameter::default(),
    )
    .biegeenergie();
    assert!(c > a, "die steife Richtung muss teurer sein: {c:e} gegen {a:e}");
}
