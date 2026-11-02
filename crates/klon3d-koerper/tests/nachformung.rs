//! Post-shaping between waist and hip on a body of revolution with a known
//! profile, without the reference data.

use klon3d_koerper::massband::{rumpf_waagerecht, segment, Messdaten};
use klon3d_koerper::nachformung::{nachformen, Pchip, Ziele};
use klon3d_koerper::Netz;

/// A body of revolution (mm, y up, axis x = z = 0) whose tape measure at
/// height `y` cm is `umfang(y)` cm; rings every 2 mm, 96 segments.
fn rotationskoerper(umfang: &dyn Fn(f64) -> f64, bis_cm: f64) -> Netz {
    let seg = 96;
    let ringe = (bis_cm * 5.0) as usize;
    // a 96-gon's perimeter is 2 r · 96 · sin(π/96)
    let k = 2.0 * seg as f64 * (std::f64::consts::PI / seg as f64).sin();
    let mut n = Netz::default();
    for r in 0..=ringe {
        let y = bis_cm * r as f64 / ringe as f64;
        let radius = umfang(y) / k * 10.0;
        for s in 0..seg {
            let w = 2.0 * std::f64::consts::PI * s as f64 / seg as f64;
            n.punkte
                .push([radius * w.cos(), y * 10.0, radius * w.sin()]);
        }
    }
    for r in 0..ringe {
        for s in 0..seg {
            let s1 = (s + 1) % seg;
            let (a, b, c, d) = (
                r * seg + s,
                r * seg + s1,
                (r + 1) * seg + s,
                (r + 1) * seg + s1,
            );
            n.dreiecke.push([a, c, b]);
            n.dreiecke.push([b, c, d]);
        }
    }
    n
}

fn messen(n: &Netz, d: &Messdaten, y: f64) -> f64 {
    let v: Vec<[f64; 3]> = n
        .punkte
        .iter()
        .map(|p| [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0])
        .collect();
    rumpf_waagerecht(&v, d, y)
}

/// Waist 70 at 100 cm, hip 90 at 76 cm (the chain's places); the targets
/// move the waist to 98 cm, the hip to 77 cm and ask for 83 cm at 93 cm.
#[test]
fn waist_high_hip_and_hip_at_their_heights() {
    let profil = Pchip::neu(&[
        (40.0, 86.0),
        (76.0, 90.0),
        (100.0, 70.0),
        (115.0, 85.0),
        (140.0, 88.0),
    ]);
    let mut netz = rotationskoerper(&|y| profil.wert(y), 150.0);
    let d = Messdaten::aus_netz(&netz, segment::RUMPF);
    let vorher = netz.punkte.clone();
    let z = Ziele {
        taille: (98.0, 70.0),
        huefte_oben: (93.0, 83.0),
        huefte: (77.0, 90.0),
    };
    let b = nachformen(&mut netz.punkte, &d, (100.0, 76.0, 115.0), &z).expect("post-shaping");
    for (h, soll) in [(98.0, 70.0), (93.0, 83.0), (77.0, 90.0)] {
        let ist = messen(&netz, &d, h);
        assert!((ist - soll).abs() < 0.1, "{h} cm: {ist:.3}, target {soll}");
    }
    // the waist is the narrowest place, the hip is not overtaken on the way up
    assert!(
        (b.schmalste - 98.0).abs() <= 0.5,
        "narrowest at {}",
        b.schmalste
    );
    let mut y = 77.0;
    let mut vorig = messen(&netz, &d, y);
    while y < 93.0 {
        y += 1.0;
        let u = messen(&netz, &d, y);
        assert!(u <= 90.1, "{y} cm: {u:.2} above the hip");
        assert!(
            u <= vorig + 0.05,
            "{y} cm: {u:.2} after {vorig:.2}, not falling towards the waist"
        );
        vorig = u;
    }
    // outside the bands nothing moves, bit for bit
    let (unten, oben) = b.band_hoehen;
    for (p, q) in netz.punkte.iter().zip(&vorher) {
        let y = q[1] / 10.0;
        if y <= unten || y >= oben {
            assert_eq!(p, q);
        }
    }
}

/// Targets out of order (high hip above the waist) are refused, not bent.
#[test]
fn targets_out_of_order() {
    let wert = |n: &str| match n {
        "height_waist_side" => Some(98.0),
        "waist_circ" => Some(70.0),
        "height_highhip" => Some(99.0),
        "highhip_circ" => Some(83.0),
        "height_hip" => Some(77.0),
        "hip_circ" => Some(90.0),
        _ => None,
    };
    assert_eq!(Ziele::aus_smis(&wert), None);
    let wert2 = |n: &str| {
        if n == "height_highhip" {
            None
        } else if n == "waist_to_highhip_side" {
            Some(5.0)
        } else {
            wert(n)
        }
    };
    assert_eq!(Ziele::aus_smis(&wert2).unwrap().huefte_oben, (93.0, 83.0));
}

/// The monotone cubic stays between its knots and is exact at them.
#[test]
fn pchip_monotone() {
    let p = Pchip::neu(&[(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 5.0)]);
    for (x, y) in [(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 5.0)] {
        assert!((p.wert(x) - y).abs() < 1e-12);
    }
    let mut vorig = p.wert(0.0);
    for i in 1..=300 {
        let w = p.wert(i as f64 / 100.0);
        assert!(w >= vorig - 1e-12, "not monotone at {}", i as f64 / 100.0);
        assert!(
            i < 100 || i > 200 || (w - 1.0).abs() < 1e-12,
            "overshoot on the flat part"
        );
        vorig = w;
    }
}
