//! Post-shaping on the body from the user's measurement file, with the
//! reference resizing folder. Skips without it (`CLO_FEMALE`).

mod common;

use std::path::PathBuf;

use common::json_mini::ordner;
use klon3d_koerper::koerper::aus_smis_mit;
use klon3d_koerper::massband::{rumpf_waagerecht, Messdaten};
use klon3d_koerper::silhouette::{neue_kontakte, pruefe, Art, Parameter};

fn daten() -> Option<(PathBuf, klon3d_pattern::Smis)> {
    let Some(o) = ordner() else {
        eprintln!("übersprungen: Referenzordner fehlt (CLO_FEMALE)");
        return None;
    };
    let pfad = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../klon3d-pattern/tests/muster/hexen_2026_10_27/measurements.smis"
    ));
    match klon3d_pattern::lies_smis(&pfad) {
        Ok(s) => Some((o, s)),
        Err(_) => {
            eprintln!("übersprungen: {} fehlt", pfad.display());
            None
        }
    }
}

/// Waist 70 at 98 cm, high hip 83 at 93 cm, hip 90 at 77 cm (the file of
/// 27.10.2026): within 0.3 cm after the step, waist and hip at the file's
/// heights, the waist the narrowest section, the hip not overtaken on the way
/// up; bust and underbust and everything above the band unchanged; no new
/// crossing in the silhouette.
#[test]
fn body_between_waist_and_hip() {
    let Some((o, smis)) = daten() else { return };
    let wert = |n: &str| smis.wert(n);
    let ohne = aus_smis_mit(&o, &wert, false).expect("chain");
    let mit = aus_smis_mit(&o, &wert, true).expect("chain with post-shaping");
    let b = mit.nachformung.as_ref().expect("post-shaping ran");
    for z in b.zeilen() {
        println!("{z}");
    }

    let d = Messdaten::laden(&o).unwrap();
    let boden = mit
        .netz
        .punkte
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min);
    let v: Vec<[f64; 3]> = mit
        .netz
        .punkte
        .iter()
        .map(|p| [p[0] / 10.0, (p[1] - boden) / 10.0, p[2] / 10.0])
        .collect();
    for (h, name) in [
        ("height_waist_side", "waist_circ"),
        ("height_highhip", "highhip_circ"),
        ("height_hip", "hip_circ"),
    ] {
        let (h, soll) = (wert(h).unwrap(), wert(name).unwrap());
        let ist = rumpf_waagerecht(&v, &d, h);
        assert!(
            (ist - soll).abs() < 0.3,
            "{name} at {h} cm: {ist:.3}, target {soll}"
        );
    }
    assert!(
        (b.taille_hoehe.1 - wert("height_waist_side").unwrap()).abs() <= 0.3,
        "waist height {}",
        b.taille_hoehe.1
    );
    assert!(
        (b.huefte_hoehe.1 - wert("height_hip").unwrap()).abs() <= 0.3,
        "hip height {}",
        b.huefte_hoehe.1
    );
    assert!(
        (b.schmalste - wert("height_waist_side").unwrap()).abs() <= 0.3,
        "narrowest at {}",
        b.schmalste
    );
    // the hip is not overtaken on the way up to the waist (flat at the hip)
    let hh = wert("height_hip").unwrap();
    let mut h = hh;
    while h <= wert("height_highhip").unwrap() + 1e-9 {
        let u = rumpf_waagerecht(&v, &d, h);
        assert!(
            u <= wert("hip_circ").unwrap() + 0.3,
            "at {h} cm {u:.3} above the hip {}",
            wert("hip_circ").unwrap()
        );
        h += 1.0;
    }

    // above the band (bust, underbust, arms, head) nothing moved
    let oben = b.band_hoehen.1;
    let mut gleich = 0;
    for (p, q) in mit.netz.punkte.iter().zip(&ohne.netz.punkte) {
        if (q[1] - boden) / 10.0 >= oben {
            assert_eq!(p, q);
            gleich += 1;
        }
    }
    assert!(gleich > 10_000, "{gleich} points above the band");

    // silhouette: no new crossing
    let par = Parameter::default();
    let (b0, b1) = (pruefe(&ohne.netz, &par), pruefe(&mit.netz, &par));
    println!("ohne: {}\nmit:  {}", b0.als_text(), b1.als_text());
    let (neu, _) = neue_kontakte(&b0, &b1, 0.0);
    let kreuzungen: Vec<_> = neu.iter().filter(|k| k.art == Art::Kreuzung).collect();
    assert!(
        kreuzungen.is_empty(),
        "{} new crossings: {:?}",
        kreuzungen.len(),
        &kreuzungen[..kreuzungen.len().min(5)]
    );
}
