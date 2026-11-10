//! Ease along an unequal seam (task #109).
//!
//! A seam whose two sides differ in length: one side is 100 mm, the other
//! 112 mm — the 12 mm a sleeve cap carries over an armhole. Both sides are laid
//! flat, no gravity, no body, no reduction: the pattern is the cloth.
//!
//! The mesh already pairs the points at the same arc fraction on both sides, so
//! the surplus is spread evenly along the seam. What is missing is the rest
//! length: with both sides unshortened the seam springs (rest 0) pull the pairs
//! together and the solver splits the surplus **half and half** — the shorter
//! side stretches, the longer one compresses.
//!
//! [`Simparameter::einhalteweite`] shortens the boundary springs of the longer
//! side by `l_short/l_long`, so that side is at rest once the seam has closed
//! and the shorter side keeps its pattern length.
//!
//! ## The fabric decides whether the ease holds
//!
//! The ease moves the surplus onto the longer side. Whether it *stays* there is
//! the bending stiffness: the longer side has to fold, and a stiff material
//! pushes back instead. Measured on the same cap, both sides in % of their
//! pattern length (after the continuum of #127):
//!
//! | material | ease | shorter side | longer side |
//! |---|---|---|---|
//! | Radical 0.4, latex (2.48e-6 N m) | off / on | +8.03 / +5.31 | −3.54 / −6.01 |
//! | Retifist 0.4, woven (6.04e-8 N m) | off / on | +6.35 / **+0.10** | −5.04 / −10.63 |
//!
//! (The numbers when #109 was written were +7.99 / +5.08 and +6.40 / +0.53; the
//! continuum of #127 moved them, the woven case to its benefit. #129 moves the
//! latex row alone, because it makes warp and bias 0.17 % stiffer: on `main`
//! before it the same latex run read +7.82 / +5.18 and −3.70 / −6.14. The woven
//! row is unchanged to the printed digit - `RETIFIST_NR_040` states one bending
//! value for all three directions, so [`klon3d_sim::biegung::steifigkeit`]
//! returns it bit for bit.)
//!
//! Latex never reaches the 1 % because it stretches rather than gathers; the
//! switch still works there, it just cannot put the surplus anywhere. Woven
//! fabric is the case the issue is about, so [`kappe_mitt_mehrweite_haelt_die_kuerzere_seite`]
//! runs on it and [`material_und_falten`] keeps both numbers.
//!
//! ## Why the cap and not two rectangles
//!
//! The surplus of a real cap comes from the **curvature** of its seam line: the
//! arc is 112 mm, the chord 100 mm, so the two ends of the seam already sit
//! where the armhole's ends are. Only the arc has to give.
//!
//! Two flat rectangles of 100 and 112 mm are a different case: there the ends
//! are 12 mm apart too, so closing the seam shears the whole 112 mm wide piece
//! and drags the 100 mm side along. Measured with the ease on: the shorter side
//! still reads 101.97 mm (**+1.97 %**) instead of 106.85 mm (**+6.85 %**). The
//! ease moves most of the surplus onto the longer side, but the shorter one
//! cannot hold its length because nothing in a rectangle resists the shear. The
//! rectangle case is kept as [`rechteck_zahlen`], a measurement of that
//! difference, not as an acceptance.

use std::collections::HashMap;

use klon3d_mesh::{vernetze, Musternetz, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::read_str;
use klon3d_sim::roehre::{bis_zur_ruhe, param_ruhe};
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{CpuReferenz, Simparameter, Szene, RADICAL_RUBBER_04};

/// Length of the shorter seam side, in mm.
const KURZ_MM: f64 = 100.0;
/// Length of the longer seam side, in mm.
const LANG_MM: f64 = 112.0;
/// Mesh width of the test pieces, in mm.
const KANTE_MM: f64 = 5.0;
/// Height of the flat piece, in mm.
const HOEHE_MM: f64 = 50.0;
/// The fabric the acceptance is written on: a woven one. Easing is a property of
/// woven fabric — the longer side folds. Latex stretches instead of folding and
/// is measured as the counter-case in [`material_und_falten`].
const WEBSTOFF: klon3d_sim::Material = klon3d_sim::RETIFIST_NR_040;

fn netz_aus(xml: &str, naht: &str) -> Musternetz {
    let m = read_str(xml).expect("pattern readable");
    let ev = Auswertung::new(&m, HashMap::new());
    let defs = lies_nahtliste(naht).expect("seam list readable");
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).expect("seams check out");
    let p = Parameter { kante_mm: KANTE_MM, ..Parameter::default() };
    let n = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &p).expect("meshable");
    assert!(n.fehler.is_empty(), "{:?}", n.fehler);
    n
}

fn szene_aus(netz: Musternetz, einhalteweite: bool) -> Szene {
    szene_mit_stoff(netz, einhalteweite, RADICAL_RUBBER_04)
}

fn szene_mit_stoff(netz: Musternetz, einhalteweite: bool, material: klon3d_sim::Material) -> Szene {
    let vg = Teilvorgabe {
        material,
        reduktion: 0.0,
    };
    let param = Simparameter {
        einhalteweite,
        ..param_ruhe()
    };
    Szene::aus_netz(&netz, &|_| vg, &|_, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0], Vec::new(), param)
}

/// Radius and half-angle of a circular arc whose **polyline** of `n` chords has
/// length `s` over a chord of `c`.
///
/// Solved on `n·sin(θ/n)/sin(θ) = s/c`, not on the continuous arc: the pattern
/// stores the sampled vertices, so the polyline is what the mesh measures. With
/// the arc as target a 24-gon reads 111.978 mm instead of 112 — a 22 µm hole in
/// the arithmetic that the ease factor would inherit.
fn bogen(c: f64, s: f64, n: usize) -> (f64, f64) {
    let ziel = s / c;
    let (mut lo, mut hi) = (1e-9, std::f64::consts::PI - 1e-9);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let g = (n as f64) * (mid / n as f64).sin() / mid.sin();
        if g < ziel {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let theta = 0.5 * (lo + hi);
    (c / (2.0 * theta.sin()), theta)
}

/// Cap above a seam line of `lang` mm of arc over a chord of `kurz` mm, plus a
/// flat piece below whose seam edge is exactly the chord.
///
/// The cap's arc bulges **up** out of the chord, so the two seam ends coincide
/// from the start. Piece A sits below the line y = 0, piece B above it.
fn muster_kappe() -> String {
    let n = 24;
    let (r, theta) = bogen(KURZ_MM, LANG_MM, n);
    // Arc points from the left end (0,0) to the right end (100,0).
    let mut bogen_punkte = Vec::new();
    for i in 0..=n {
        let phi = -theta + 2.0 * theta * i as f64 / n as f64;
        let x = 0.5 * KURZ_MM + r * phi.sin();
        let y = -r * theta.cos() + r * phi.cos();
        bogen_punkte.push((x, y));
    }
    // Round the ends exactly onto the chord.
    bogen_punkte[0] = (0.0, 0.0);
    bogen_punkte[n] = (KURZ_MM, 0.0);

    let mut punkte: Vec<(u32, String, f64, f64)> = Vec::new();
    let mut id = 1u32;
    // A: rectangle below the seam, its top edge is the chord.
    let a1 = id;
    punkte.push((id, "A1".into(), 0.0, -HOEHE_MM));
    id += 1;
    let a2 = id;
    punkte.push((id, "A2".into(), KURZ_MM, -HOEHE_MM));
    id += 1;
    let a3 = id;
    punkte.push((id, "A3".into(), KURZ_MM, 0.0));
    id += 1;
    let a4 = id;
    punkte.push((id, "A4".into(), 0.0, 0.0));
    id += 1;
    // B: the arc, then the cap body above it.
    let mut b_ids = Vec::new();
    for (i, (x, y)) in bogen_punkte.iter().enumerate() {
        let name = if i == 0 {
            "B1".to_string()
        } else if i == n {
            "B2".to_string()
        } else {
            format!("Bm{i}")
        };
        punkte.push((id, name, *x, *y));
        b_ids.push(id);
        id += 1;
    }
    let b3 = id;
    punkte.push((id, "B3".into(), KURZ_MM, HOEHE_MM));
    id += 1;
    let b4 = id;
    punkte.push((id, "B4".into(), 0.0, HOEHE_MM));

    let calc: String = punkte
        .iter()
        .map(|(i, n, x, y)| {
            format!(r#"<point id="{i}" name="{n}" type="single" x="{x:.6}" y="{y:.6}" mx="0" my="0"/>"#)
        })
        .collect();
    let model: String = punkte
        .iter()
        .map(|(i, ..)| format!(r#"<point id="{}" idObject="{i}" type="modeling"/>"#, i + 200))
        .collect();
    let knoten = |ids: &[u32]| -> String {
        ids.iter()
            .map(|i| format!(r#"<node idObject="{}" type="NodePoint"/>"#, i + 200))
            .collect()
    };
    let mut b_knoten = b_ids.clone();
    b_knoten.push(b3);
    b_knoten.push(b4);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern><version>0.7.4</version><unit>mm</unit>
  <draftBlock name="Kappe">
    <calculation>{calc}</calculation>
    <modeling>{model}</modeling>
    <pieces>
      <piece id="50" name="Teil A" seamAllowance="false" width="0"><nodes>{}</nodes></piece>
      <piece id="51" name="Teil B" seamAllowance="false" width="0"><nodes>{}</nodes></piece>
    </pieces>
  </draftBlock>
</pattern>"#,
        knoten(&[a1, a2, a3, a4]),
        knoten(&b_knoten)
    )
}

/// The issue's own geometry: two flat rectangles, sewn along their edges of
/// 100 mm and 112 mm. Kept because it is the case that shows what a rectangle
/// cannot do — see the module doc.
fn muster_rechteck() -> String {
    let punkte = [
        (1u32, "A1", 0.0, 0.0),
        (2, "A2", KURZ_MM, 0.0),
        (3, "A3", KURZ_MM, HOEHE_MM),
        (4, "A4", 0.0, HOEHE_MM),
        (5, "B1", 0.0, 10.0 * HOEHE_MM),
        (6, "B2", LANG_MM, 10.0 * HOEHE_MM),
        (7, "B3", LANG_MM, 11.0 * HOEHE_MM),
        (8, "B4", 0.0, 11.0 * HOEHE_MM),
    ];
    let calc: String = punkte
        .iter()
        .map(|(i, n, x, y)| format!(r#"<point id="{i}" name="{n}" type="single" x="{x}" y="{y}" mx="0" my="0"/>"#))
        .collect();
    let model: String = punkte
        .iter()
        .map(|(i, ..)| format!(r#"<point id="{}" idObject="{i}" type="modeling"/>"#, i + 200))
        .collect();
    let knoten = |ids: &[u32]| -> String {
        ids.iter()
            .map(|i| format!(r#"<node idObject="{}" type="NodePoint"/>"#, i + 200))
            .collect()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern><version>0.7.4</version><unit>mm</unit>
  <draftBlock name="Rechteck">
    <calculation>{calc}</calculation>
    <modeling>{model}</modeling>
    <pieces>
      <piece id="50" name="Teil A" seamAllowance="false" width="0"><nodes>{}</nodes></piece>
      <piece id="51" name="Teil B" seamAllowance="false" width="0"><nodes>{}</nodes></piece>
    </pieces>
  </draftBlock>
</pattern>"#,
        knoten(&[1, 2, 3, 4]),
        knoten(&[5, 6, 7, 8])
    )
}

/// `A4 A3 r`: the cap's bottom edge is the piece's *top* edge, and the outline
/// runs A1 → A2 → A3 → A4, so the seam has to be walked backwards. Written `v`
/// the walk goes the long way round the piece and reads 200 mm instead of 100.
const NAHT_KAPPE: &str = "N1 | Kappe 12 mm | Teil A: A4 A3 r | Teil B: B1 B2 v";
const NAHT_RECHTECK: &str = "N1 | Rechteck 12 mm | Teil A: A1 A2 v | Teil B: B1 B2 v";

fn netz_kappe() -> Musternetz {
    netz_aus(&muster_kappe(), NAHT_KAPPE)
}

fn netz_rechteck() -> Musternetz {
    netz_aus(&muster_rechteck(), NAHT_RECHTECK)
}

/// The two sewn edges as point chains, in seam order, in the scene's
/// coordinates (m): `(kürzere Seite, längere Seite)`.
fn nahtkanten(s: &Szene, netz: &Musternetz) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let tn = &netz.naehte[0].teilnaehte[0];
    let (oa, ob) = (s.teile[tn.teil_a].punkte.start, s.teile[tn.teil_b].punkte.start);
    let a: Vec<[f64; 3]> = tn.paare.iter().map(|p| s.x[oa + p.a as usize]).collect();
    let b: Vec<[f64; 3]> = tn.paare.iter().map(|p| s.x[ob + p.b as usize]).collect();
    // `teil_a` is the shorter side in both patterns above.
    (a, b)
}

fn laenge(p: &[[f64; 3]]) -> f64 {
    p.windows(2)
        .map(|w| {
            ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2) + (w[1][2] - w[0][2]).powi(2))
                .sqrt()
        })
        .sum()
}

/// Largest distance of the edge's points from the chord between its ends, in m.
fn woelbung(p: &[[f64; 3]]) -> f64 {
    let (a, b) = (p[0], p[p.len() - 1]);
    let d: [f64; 3] = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    p.iter()
        .map(|q| {
            let w: [f64; 3] = [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
            let t = (w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / l2;
            let r: [f64; 3] = [w[0] - t * d[0], w[1] - t * d[1], w[2] - t * d[2]];
            (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
        })
        .fold(0.0, f64::max)
}

/// Median and maximum distance of the seam's point pairs, in m.
fn paarabstaende(s: &Szene, netz: &Musternetz) -> (f64, f64) {
    let mut d = Vec::new();
    for n in &netz.naehte {
        for tn in &n.teilnaehte {
            let (oa, ob) = (s.teile[tn.teil_a].punkte.start, s.teile[tn.teil_b].punkte.start);
            for pp in &tn.paare {
                let (a, b) = (s.x[oa + pp.a as usize], s.x[ob + pp.b as usize]);
                d.push(((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt());
            }
        }
    }
    d.sort_by(f64::total_cmp);
    (d[d.len() / 2], *d.last().expect("pairs"))
}

/// How far the two sewn edges have come from their pattern lengths, in per
/// cent of the pattern: `(kürzere, längere)`.
fn dehnung(s: &Szene, netz: &Musternetz) -> (f64, f64) {
    let (a, b) = nahtkanten(s, netz);
    let n = &netz.naehte[0];
    (
        (laenge(&a) * 1e3 / n.laenge_a_mm - 1.0) * 100.0,
        (laenge(&b) * 1e3 / n.laenge_b_mm - 1.0) * 100.0,
    )
}

/// 1. The pattern really has the promised lengths, and the mesh pairs the arcs
/// point for point. Without this every number below would be meaningless.
#[test]
fn das_muster_hat_die_mehrweite() {
    let n = netz_kappe();
    let s = &n.naehte[0];
    assert!(
        (s.laenge_a_mm - KURZ_MM).abs() < 1e-6,
        "shorter side {:.6} mm",
        s.laenge_a_mm
    );
    assert!(
        (s.laenge_b_mm - LANG_MM).abs() < 1e-4,
        "longer side {:.6} mm, expected {LANG_MM} (24 chords, so a hair under)",
        s.laenge_b_mm
    );
    // Both sides carry the same number of points, or the pairing would be off.
    let paare = &s.teilnaehte[0].paare;
    assert!(paare.len() > 20, "only {} pairs", paare.len());
    // The two ends coincide already: the cap's arc spans the same chord.
    let (a, b) = nahtkanten(&scene_zurueck(&n), &n);
    let (a0, b0) = (a[0], b[0]);
    let (a1, b1) = (a[a.len() - 1], b[b.len() - 1]);
    assert!((a0[0] - b0[0]).abs() < 1e-9 && (a0[1] - b0[1]).abs() < 1e-9, "start {a0:?} vs {b0:?}");
    assert!((a1[0] - b1[0]).abs() < 1e-9 && (a1[1] - b1[1]).abs() < 1e-9, "end {a1:?} vs {b1:?}");
}

fn scene_zurueck(n: &Musternetz) -> Szene {
    szene_aus(n.clone(), false)
}

/// 2. The rest lengths carry the surplus: with the switch on, the summed rest
/// lengths of the scene are exactly `112 − 100 = 12 mm` smaller. No simulation,
/// so a wrong factor cannot hide behind a converged run.
#[test]
fn der_schalter_kuerzt_genau_die_mehrweite() {
    for netz in [netz_kappe(), netz_rechteck()] {
        let ohne = szene_aus(netz.clone(), false);
        let mit = szene_aus(netz, true);
        let summe = |s: &Szene| s.abstand.ruhe.iter().sum::<f64>();
        let weg = summe(&ohne) - summe(&mit);
        assert!(
            (weg - 0.012).abs() < 1e-9,
            "rest lengths differ by {:.9} m, expected 0.012",
            weg
        );
        assert_eq!(ohne.einhalte_kanten, 0, "the ease must stay off when not asked for");
        assert!(mit.einhalte_kanten > 1, "only {} edges eased", mit.einhalte_kanten);
    }
}

/// 3. The shorter side keeps its pattern length, the longer side takes the
/// surplus, and the seam stays closed.
///
/// Woven fabric, not latex: easing means the longer side has to **fold**, and
/// whether it folds or just pushes back is the bending stiffness. Measured, in
/// % of each side's pattern length:
///
/// | material | ease | shorter | longer |
/// |---|---|---|---|
/// | Radical 0.4 (latex, 2.48e-6 N m) | off / on | +8.03 / **+5.31** | −3.54 / −6.01 |
/// | Retifist 0.4 (woven, 6.04e-8 N m) | off / on | +6.35 / **+0.10** | −5.04 / −10.63 |
///
/// Latex cannot hold the 1 % because it stretches instead of gathering — with
/// it the ease still works, it just moves the surplus somewhere else. Woven
/// fabric is the case the issue is about, so it is the acceptance.
#[test]
#[cfg_attr(debug_assertions, ignore = "solver run, only with --release")]
fn kappe_mitt_mehrweite_haelt_die_kuerzere_seite() {
    let netz = netz_kappe();
    let mut s = szene_mit_stoff(netz.clone(), true, WEBSTOFF);
    bis_zur_ruhe(&mut s, &mut CpuReferenz::neu(), 2000, 1e-4, 10);
    let (dk, dl) = dehnung(&s, &netz);
    let (med, max) = paarabstaende(&s, &netz);
    println!(
        "Kappe mit Mehrweite (Webstoff): kurz {dk:+.3} %, lang {dl:+.3} %, Paare {:.4}/{:.4} mm",
        med * 1e3,
        max * 1e3
    );

    assert!(dk.abs() <= 1.0, "shorter side {dk:+.3} %, allowed 1 %");
    // At least 80 % of the 12 mm surplus must be gone from the longer side:
    // 80 % of 12 mm on 112 mm is 8.57 %.
    assert!(dl <= -8.57, "longer side {dl:+.3} %, expected at most −8.57 %");
    assert!(med <= 0.001, "seam pairs median {:.6} m", med);
}

/// 4. Without the ease the same scene splits the surplus evenly: both sides land
/// on the same length, 106.4 mm — 100 mm stretched by 6.40 %, 112 mm shortened
/// by 5.01 %. That is what makes the switch the only difference between 3 and 4.
#[test]
#[cfg_attr(debug_assertions, ignore = "solver run, only with --release")]
fn ohne_mehrweite_teilt_der_loeser_haelfte_haelfte() {
    let netz = netz_kappe();
    let mut s = szene_mit_stoff(netz.clone(), false, WEBSTOFF);
    bis_zur_ruhe(&mut s, &mut CpuReferenz::neu(), 2000, 1e-4, 10);
    let (dk, dl) = dehnung(&s, &netz);
    let (kurz_mm, lang_mm) = (KURZ_MM * (1.0 + dk / 100.0), LANG_MM * (1.0 + dl / 100.0));
    println!(
        "Kappe ohne Mehrweite: kurz {dk:+.3} % = {kurz_mm:.3} mm, lang {dl:+.3} % = {lang_mm:.3} mm"
    );
    // Both sides meet in the middle instead of one keeping its length.
    assert!(dk > 3.0, "shorter side only {dk:+.3} % stretched — not the split");
    assert!(dl < -3.0, "longer side only {dl:+.3} % shortened — not the split");
    assert!(
        (kurz_mm - lang_mm).abs() < 0.5,
        "sides did not meet in the middle: {kurz_mm:.3} vs {lang_mm:.3} mm"
    );
}

/// Measurement, not acceptance: the issue's two flat rectangles. The ease does
/// move most of the surplus onto the longer side, but the shorter side cannot
/// hold its length there — a rectangle shears instead of gathering. The numbers
/// are printed so the difference to the cap stays documented.
#[test]
#[cfg_attr(debug_assertions, ignore = "solver run, only with --release")]
fn rechteck_zahlen() {
    for ease in [false, true] {
        let netz = netz_rechteck();
        let mut s = szene_aus(netz.clone(), true == ease);
        let l = bis_zur_ruhe(&mut s, &mut CpuReferenz::neu(), 2000, 1e-4, 10);
        let (dk, dl) = dehnung(&s, &netz);
        let (med, max) = paarabstaende(&s, &netz);
        println!(
            "Rechteck ease {ease}: ruhig {} nach {} Bildern, v {:.2e}; kurz {dk:+.3} %, \
             lang {dl:+.3} %, Paare {:.4}/{:.4} mm",
            l.ruhig,
            l.bilder,
            l.v_mittel,
            med * 1e3,
            max * 1e3
        );
    }
}

/// Diagnostic: which material gathers best. Easing means the longer side has to
/// fold, and the bending stiffness decides whether it folds or just pushes back.
/// Measured, so the choice of the acceptance material is not a guess.
#[test]
#[cfg_attr(debug_assertions, ignore = "diagnosis only, slow in debug")]
fn material_und_falten() {
    for (name, m) in [
        ("Radical 0.4", RADICAL_RUBBER_04),
        ("Retifist 0.4", klon3d_sim::RETIFIST_NR_040),
    ] {
        for ease in [false, true] {
            let netz = netz_kappe();
            let mut s = szene_mit_stoff(netz.clone(), ease, m);
            let l = bis_zur_ruhe(&mut s, &mut CpuReferenz::neu(), 2000, 1e-4, 10);
            let (dk, dl) = dehnung(&s, &netz);
            let (a, b) = nahtkanten(&s, &netz);
            let (med, max) = paarabstaende(&s, &netz);
            println!(
                "{name:14} ease {ease:5}: kurz {dk:+7.3} %, lang {dl:+7.3} %, Woelbung {:.2}/{:.2} mm, \
                 Paare {:.4}/{:.4} mm, Biegung {:.3e}",
                woelbung(&a) * 1e3,
                woelbung(&b) * 1e3,
                med * 1e3,
                max * 1e3,
                m.biegung_nm
            );
            let _ = l;
        }
    }
}
