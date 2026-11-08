//! The grainline as a direction **per triangle** (stage 3, `PLAN.md`).
//!
//! The pattern gives one axis per piece. The placement bends the piece over the
//! body, so the same axis is a different 3D direction at every triangle.
//! [`klon3d_sim::Szene::faser`] carries that direction; nothing reads it yet -
//! the material is isotropic - so what is checked here is the geometry only:
//!
//! 1. the axis as a 2D unit vector, and the substitute when there is none;
//! 2. a flat placement: the direction is exactly the axis, length one, in the
//!    triangle's plane;
//! 3. a turned grainline turns the direction with it;
//! 4. a nonlinear placement (wrapping around a cylinder): the direction follows
//!    the map, not the flat axis - across the piece, not at one point;
//! 5. pieces without a grainline get the substitute axis and are counted, and
//!    a triangle the placement collapses is counted too.
//!
//! All of it runs without CLO data.

use klon3d_mesh::{Fadenlauf, Musternetz, Teilnetz};
use klon3d_sim::faser::{im_dreieck, richtung, ERSATZ};
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{Simparameter, Szene, RADICAL_RUBBER_04};

/// One piece from points and triangles in pattern millimetres.
fn netz(punkte: Vec<[f64; 2]>, dreiecke: Vec<[u32; 3]>, fadenlauf: Option<Fadenlauf>) -> Musternetz {
    Musternetz {
        parameter: Default::default(),
        teile: vec![Teilnetz {
            teil: "Prüf".into(),
            punkte,
            dreiecke,
            nrand: 0,
            randort: Vec::new(),
            punktknoten: Vec::new(),
            kanten: Vec::new(),
            kontur: Vec::new(),
            schnitt: Vec::new(),
            zugabe: Default::default(),
            fadenlauf,
            aufgegeben: 0,
        }],
        naehte: Vec::new(),
        fehler: Vec::new(),
        runden: 0,
        offene_teilungen: 0,
    }
}

/// A 3 × 2 grid of points (x 0/10/20 mm, y 0/10 mm), four triangles.
fn gitter() -> (Vec<[f64; 2]>, Vec<[u32; 3]>) {
    let mut p = Vec::new();
    for j in 0..2 {
        for i in 0..3 {
            p.push([10.0 * i as f64, 10.0 * j as f64]);
        }
    }
    // Positive orientation in pattern coordinates (y down).
    let d = vec![[0u32, 3, 1], [1, 3, 4], [1, 4, 2], [2, 4, 5]];
    (p, d)
}

/// Pattern x to 3D x, pattern y to 3D y, flat in z = 0.
fn flach(_: usize, q: [f64; 2]) -> [f64; 3] {
    [q[0] * 1e-3, q[1] * 1e-3, 0.0]
}

fn szene(n: &Musternetz, lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3]) -> Szene {
    let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
    Szene::aus_netz(n, &|_| vg, lage, Vec::new(), Simparameter::default())
}

fn fadenlauf(p1: [f64; 2], p2: [f64; 2]) -> Option<Fadenlauf> {
    Some(Fadenlauf { p1, p2, pfeile: 0 })
}

fn nahe(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-9)
}

fn kreuz(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn skalar(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn laenge(a: [f64; 3]) -> f64 {
    skalar(a, a).sqrt()
}

/// 1. The axis as a unit vector; a zero-length axis has no direction.
#[test]
fn richtung_ist_die_normierte_achse() {
    let f = Fadenlauf { p1: [0.0, 10.0], p2: [0.0, 0.0], pfeile: 0 };
    let d = f.richtung().expect("Achse hat eine Richtung");
    // Pattern y points down, so p1 → p2 is -y.
    assert!(nahe([d[0], d[1], 0.0], [0.0, -1.0, 0.0]), "{d:?}");
    let schief = Fadenlauf { p1: [0.0, 0.0], p2: [30.0, 40.0], pfeile: 0 };
    let d = schief.richtung().expect("Achse hat eine Richtung");
    assert!(nahe([d[0], d[1], 0.0], [0.6, 0.8, 0.0]), "{d:?}");
    // Both ends on the same point: Seamly2D can write that, it is no direction.
    let punkt = Fadenlauf { p1: [5.0, 5.0], p2: [5.0, 5.0], pfeile: 0 };
    assert_eq!(punkt.richtung(), None);
    assert_eq!(richtung(Some(punkt)), (ERSATZ, true));
    assert_eq!(richtung(None), (ERSATZ, true));
    assert_eq!(richtung(Some(schief)), ([0.6, 0.8], false));
}

/// 2. Flat: the direction is the axis itself, one long, in the triangle plane.
#[test]
fn flach_ist_die_faser_die_achse() {
    let (p, d) = gitter();
    let n = netz(p, d, fadenlauf([0.0, 0.0], [20.0, 0.0]));
    let s = szene(&n, &flach);
    assert_eq!(s.faser.len(), s.dreiecke.len(), "eine Richtung je Dreieck");
    assert_eq!(s.faser_ersatz, 0, "Fadenlauf war da");
    assert_eq!(s.faser_entartet, 0);
    for (k, &f) in s.faser.iter().enumerate() {
        assert!((laenge(f) - 1.0).abs() < 1e-12, "Dreieck {k}: Länge {}", laenge(f));
        assert!(nahe(f, [1.0, 0.0, 0.0]), "Dreieck {k}: {f:?}");
        let t = s.dreiecke[k];
        let (a, b, c) = (s.x[t[0] as usize], s.x[t[1] as usize], s.x[t[2] as usize]);
        let nrm = kreuz([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        assert!(skalar(f, nrm).abs() < 1e-12, "Dreieck {k} liegt nicht in der Ebene");
    }
}

/// 3. A turned grainline turns the direction with it.
#[test]
fn gedrehter_fadenlauf_dreht_mit() {
    let (p, d) = gitter();
    // Along pattern y (down): 3D y. Flat, so the image is +y.
    let s = szene(&netz(p.clone(), d.clone(), fadenlauf([0.0, 0.0], [0.0, 10.0])), &flach);
    for &f in &s.faser {
        assert!(nahe(f, [0.0, 1.0, 0.0]), "{f:?}");
    }
    // Diagonal: 45° in the pattern is 45° in the plane.
    let s = szene(&netz(p, d, fadenlauf([0.0, 0.0], [10.0, 10.0])), &flach);
    let w = 0.5_f64.sqrt();
    for &f in &s.faser {
        assert!(nahe(f, [w, w, 0.0]), "{f:?}");
    }
}

/// 4. Nonlinear placement: wrapping around a cylinder. The direction follows the
/// map - pattern y along the axis, pattern x around the circumference - and it
/// changes from one triangle to the next, which a single axis per piece cannot
/// express.
#[test]
fn auf_dem_zylinder_folgt_die_richtung_der_abbildung() {
    let (p, d) = gitter();
    let r = 0.05;
    let je_m = 1.0 / r;
    let zylinder = move |_: usize, q: [f64; 2]| {
        let w = q[0] * 1e-3 * je_m;
        [r * w.cos(), r * w.sin(), -q[1] * 1e-3]
    };

    // Grainline along pattern y: around the body it stays the axis direction.
    let s = szene(&netz(p.clone(), d.clone(), fadenlauf([0.0, 0.0], [0.0, 10.0])), &zylinder);
    for (k, &f) in s.faser.iter().enumerate() {
        assert!(nahe(f, [0.0, 0.0, -1.0]), "Dreieck {k}: {f:?}");
    }

    // Grainline along pattern x: tangential, and turning with the surface. The
    // placement maps the pattern x extent [0, 10, 20] mm onto the arc
    // w = [0, 0.2, 0.4] rad of a 50 mm radius. A triangle between two of those
    // columns becomes a chord, and the image of pattern x is the chord's own
    // direction - the tangent at the middle of its arc, `[-sin(w̄), cos(w̄)]`.
    // That is the whole point of the test: two neighbouring columns get two
    // different directions, which one axis per piece cannot express.
    let s = szene(&netz(p, d, fadenlauf([0.0, 0.0], [20.0, 0.0])), &zylinder);
    // Column of each point in the grid above: 0, 1, 2, 0, 1, 2.
    let spalte = [0usize, 1, 2, 0, 1, 2];
    let mut gesehen = Vec::new();
    for (k, &f) in s.faser.iter().enumerate() {
        let t = s.dreiecke[k];
        let lo = t.iter().map(|&i| spalte[i as usize]).min().expect("drei Punkte");
        let hi = t.iter().map(|&i| spalte[i as usize]).max().expect("drei Punkte");
        let mitte = 0.1 * (lo + hi) as f64;
        let soll = [-mitte.sin(), mitte.cos(), 0.0];
        assert!(nahe(f, soll), "Dreieck {k} (Spalten {lo}–{hi}): {f:?} statt {soll:?}");
        assert!((laenge(f) - 1.0).abs() < 1e-12, "Dreieck {k}: Länge {}", laenge(f));
        gesehen.push(soll);
    }
    // Both columns occur, and they really differ: about 0.2 rad.
    assert_eq!(gesehen.len(), 4);
    let winkel = |f: [f64; 3]| f[1].atan2(f[0]);
    let spanne = gesehen.iter().map(|&f| winkel(f)).fold((f64::MAX, f64::MIN), |(lo, hi), w| (lo.min(w), hi.max(w)));
    assert!((spanne.1 - spanne.0 - 0.2).abs() < 1e-9, "Spanne {spanne:?} rad");
}

/// 5. No grainline: the substitute axis, and the piece is counted as such.
#[test]
fn ohne_fadenlauf_ersatzachse_mit_zaehler() {
    let (p, d) = gitter();
    let s = szene(&netz(p, d, None), &flach);
    assert_eq!(s.faser_ersatz, 1, "das Teil hat keinen Fadenlauf");
    assert_eq!(s.faser_entartet, 0);
    for &f in &s.faser {
        assert!(nahe(f, [1.0, 0.0, 0.0]), "{f:?}");
    }
    // A grainline that collapses to a point is no better than none.
    let s = szene(&netz(gitter().0, gitter().1, fadenlauf([7.0, 7.0], [7.0, 7.0])), &flach);
    assert_eq!(s.faser_ersatz, 1);
}

/// 6. A placement that collapses the triangle has no plane: the substitute is
/// counted, never silent.
#[test]
fn entartetes_dreieck_wird_gezaehlt() {
    let (p, d) = gitter();
    // Everything onto one line: no triangle has an area.
    let linie = |_: usize, q: [f64; 2]| [q[0] * 1e-3, 0.0, 0.0];
    let s = szene(&netz(p, d, fadenlauf([0.0, 0.0], [20.0, 0.0])), &linie);
    assert_eq!(s.faser.len(), s.dreiecke.len());
    assert_eq!(s.faser_entartet, s.faser.len(), "kein Dreieck hat eine Fläche");
    for &f in &s.faser {
        // Longest edge along ±x; the sign is the edge's own direction.
        assert!(nahe([f[0].abs(), f[1], f[2]], [1.0, 0.0, 0.0]), "längste Kante entlang x: {f:?}");
    }
}

/// 7. The direction of one triangle, directly: a triangle standing in an
/// arbitrary plane, with an axis across its rest shape.
#[test]
fn im_dreieck_bleibt_in_der_ebene() {
    // Rest triangle: right angle at the origin, 10 mm legs (in metres here).
    let ruhe_inv = [100.0, 0.0, 0.0, 100.0];
    let p = [[0.0, 0.0, 0.0], [0.01, 0.0, 0.0], [0.0, 0.01, 0.0]];
    let f = im_dreieck([1.0, 0.0], ruhe_inv, p).expect("Dreieck hat eine Ebene");
    assert!(nahe(f, [1.0, 0.0, 0.0]), "{f:?}");
    // The same triangle tilted out of the drawing plane keeps the direction in
    // its own plane: the plane normal is (1,1,0)/√2 after this rotation.
    let s = 0.5_f64.sqrt();
    let q = [[0.0, 0.0, 0.0], [s * 0.01, s * 0.01, 0.0], [0.0, 0.0, 0.01]];
    let f = im_dreieck([1.0, 0.0], ruhe_inv, q).expect("Dreieck hat eine Ebene");
    let n = kreuz([q[1][0], q[1][1], q[1][2]], [q[2][0], q[2][1], q[2][2]]);
    assert!(skalar(f, n).abs() < 1e-12, "{f:?} liegt nicht in der Ebene");
    assert!((laenge(f) - 1.0).abs() < 1e-12);
    // An axis across the rest shape (pattern y) now points along the third edge.
    let f = im_dreieck([0.0, 1.0], ruhe_inv, p).expect("Dreieck hat eine Ebene");
    assert!(nahe(f, [0.0, 1.0, 0.0]), "{f:?}");
    // No area, no plane.
    assert_eq!(im_dreieck([1.0, 0.0], ruhe_inv, [[0.0; 3]; 3]), None);
}
