//! Issue #113: dual quaternion skinning against linear blend, measured on the
//! free MakeHuman body.
//!
//! No CLO data is needed: `assets/3dobjs/makehuman/base.obj` brings the body and
//! its 125 `joint-*` groups. Both methods use the same transforms and the same
//! weights (#94); they differ only in what is averaged, so every number here is
//! the difference between two runs on one net.
//!
//! The acceptance of issue #94 stays the acceptance for **both** methods:
//! far edges not torn, bind pose unchanged, weight sums 1, at most four
//! influences. What is new is the comparison: where linear blending shortens the
//! shoulder, dual quaternions should not.

use std::path::{Path, PathBuf};

use klon3d_koerper::koerper_quelle::ordner_ab;
use klon3d_koerper::skelett::{self, Skelett, Verformung};
use klon3d_koerper::{Netz, Punkt3};

/// The arm raise the acceptance is defined at.
const GRAD: f64 = 90.0;

fn basis() -> Option<PathBuf> {
    ordner_ab(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Body and skeleton as the CLI builds them; `None` when the assets are absent.
fn gebaut() -> Option<(Netz, Skelett)> {
    let ordner = basis()?;
    match skelett::baue(&ordner, 1600.0) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("übersprungen: {e}");
            None
        }
    }
}

fn abstand(a: Punkt3, b: Punkt3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The raised pose per method.
fn beide(s: &Skelett, punkte: &[Punkt3]) -> (Vec<Punkt3>, Vec<Punkt3>) {
    let mut hoch = s.clone();
    hoch.hebe_arme(GRAD);
    let linear = hoch
        .verforme_mit(Verformung::Linear, punkte)
        .expect("lineare Verformung möglich");
    let dual = hoch
        .verforme_mit(Verformung::Dual, punkte)
        .expect("duale Verformung möglich");
    (linear, dual)
}

#[test]
fn beide_verfahren_bestehen_die_abnahme_von_94() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    for (name, v) in [
        ("linear", Verformung::Linear),
        ("dual", Verformung::Dual),
    ] {
        let ab = s
            .abnahme_mit(v, &netz.punkte, &netz.dreiecke, GRAD)
            .unwrap_or_else(|| panic!("Abnahme mit {name} nicht möglich"));

        println!(
            "{name:<6} fern {} Kanten, >2: {}, >10mm: {}, max {:.6}; \
             nah {} Kanten, min {:.6}, max {:.6}, Kollaps {}; \
             Schulter Median {:.6}, min {:.6}; Binde {:.3e} mm; \
             Gewichtssumme {:.3e}; Einflüsse {}",
            ab.fern_kanten,
            ab.fern_ueber_doppelt,
            ab.fern_ueber_10mm,
            ab.fern_max,
            ab.nah_kanten,
            ab.nah_min,
            ab.nah_max,
            ab.nah_kollaps,
            ab.schulter_median,
            ab.schulter_min,
            ab.binde_abweichung,
            ab.gewichtssumme,
            ab.einfluesse
        );

        assert_eq!(ab.fern_ueber_doppelt, 0, "{name}: ferne Kante über doppelter Länge");
        assert_eq!(ab.fern_ueber_10mm, 0, "{name}: ferne Kante über 10 mm gewachsen");
        assert!(
            ab.binde_abweichung <= 1e-9,
            "{name}: Binde-Pose weicht {:.3e} mm ab",
            ab.binde_abweichung
        );
        assert!(ab.gewichtssumme <= 1e-15, "{name}: Gewichtssumme");
        assert!(ab.einfluesse <= 4, "{name}: mehr als vier Einflüsse");
        assert!(ab.nah_min > 0.2, "{name}: nahe Kante kollabiert");
    }
}

#[test]
fn der_duale_weg_haelt_die_schulter() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    let p = &netz.punkte;
    let (linear, dual) = beide(&s, p);

    // Wie weit die beiden Ergebnisse am selben Punkt auseinanderliegen.
    let mut max = 0.0f64;
    let mut max_i = 0;
    for i in 0..p.len() {
        let d = abstand(linear[i], dual[i]);
        if d > max {
            max = d;
            max_i = i;
        }
    }

    let ab_lin = s
        .abnahme_mit(Verformung::Linear, p, &netz.dreiecke, GRAD)
        .expect("linear");
    let ab_dual = s
        .abnahme_mit(Verformung::Dual, p, &netz.dreiecke, GRAD)
        .expect("dual");

    println!(
        "Schulter Median linear {:.6} / dual {:.6}; Minimum {:.6} / {:.6}",
        ab_lin.schulter_median, ab_dual.schulter_median, ab_lin.schulter_min, ab_dual.schulter_min
    );
    println!(
        "nahe Kanten min linear {:.6} / dual {:.6}; Kollaps {} / {}",
        ab_lin.nah_min, ab_dual.nah_min, ab_lin.nah_kollaps, ab_dual.nah_kollaps
    );
    println!(
        "nahe Kanten max linear {:.6} / dual {:.6}",
        ab_lin.nah_max, ab_dual.nah_max
    );
    println!("groesster Punktabstand der beiden Ergebnisse {max:.4} mm (Punkt {max_i})");

    // The dual blend never shortens a point more than the linear one does:
    // this is the whole point of blending transforms instead of positions.
    assert!(
        ab_dual.schulter_median >= ab_lin.schulter_median,
        "duale Schulter Median {:.6} unter linear {:.6}",
        ab_dual.schulter_median,
        ab_lin.schulter_median
    );
    assert!(
        ab_dual.nah_min >= ab_lin.nah_min,
        "duale nahe Kante {:.6} unter linear {:.6}",
        ab_dual.nah_min,
        ab_lin.nah_min
    );
    assert!(
        max > 0.1,
        "die beiden Verfahren liegen nur {max:.4} mm auseinander - dann misst der Test nichts"
    );
}
