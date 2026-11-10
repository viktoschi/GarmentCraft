//! Acceptance for glue zones: a bonded layer carries the other one with it.
//!
//! Two identical layers deform identically under the same load, so comparing
//! their deflection cannot show what glue does - both cases look the same. What
//! glue does is **transfer load**: displace one layer and the other follows,
//! because the bond holds the offset instead of letting the layers slide.
//!
//! The case: two 0.4 mm layers of latex, 20 × 4 cm, no gravity. The lower layer
//! is clamped along one short edge. The upper layer's free end is pushed 5 mm
//! sideways and held there. Then the lower layer is bent along - with a glue
//! zone the lower layer's free end is pulled along, without one it stays.
//!
//! The second test records why the deflection comparison from the task
//! description does **not** work here, with numbers instead of an assertion of
//! faith. At these dimensions the sag of a strip held at both ends is carried
//! by membrane tension, not by bending: two loose layers in parallel carry
//! 2 × 348 N/m and the laminate carries 696 N/m, so all three cases sag alike.
//! The transverse (= sliding) stiffness is what separates them.
//!
//! The third test is the one the sag test cannot be. Its first version tried a
//! cantilever and failed for a physical reason worth keeping: at those mesh
//! sizes the cantilever deforms into a **straight line** - it shears, it does
//! not bend - and 10× stiffer bending constraints move the tip by 0.03 %. What
//! the zone must deliver is plate stiffness, so the test measures it as bending
//! energy at a **prescribed curvature**: a strip bent onto a circular arc
//! carries no membrane strain, and `½·Σθ²/α` is then the plate stiffness
//! itself. There the bonded pair matches the `Latex_Laminat_2x0.4mm` row of
//! MATERIAL.md and is 4× two loose layers.
//!
//! The fourth test guards the border: a zone over half a piece must not
//! stiffen the rest, and a scene without a zone must compute exactly as before.
//!
//! Only with `cargo test --release`.

use std::collections::HashMap;

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_str;
use klon3d_sim::klebung::{Stoffpaar, Zone};
use klon3d_sim::roehre::rechteck_muster;
use klon3d_sim::{ImplizitCg, ImplizitParameter, Rechenweg, Simparameter, Szene, Teilvorgabe, Material, RADICAL_RUBBER_04};

/// The laminate fabric of two 0.4 mm layers, from MATERIAL.md: 0.8 mm,
/// 698 g/m², stretch 696,548, shear 348,318, bending 19,858 (in N·m²:
/// 19,858·10⁻⁹).
const LAMINAT_08: Material = Material {
    name: "Latex_Laminat_2x0.4mm",
    dehnung_n_m: 696.548,
    kette_n_m: 696.548,
    schub_n_m: 348.318,
    biegung_nm: 2482.2424e-9 * 8.0,
    // The same number in all three directions: this synthetic laminate is
    // defined as the weft value times eight, so the measured energy of the test
    // stays the one derived in the module doc ("4 × 2,482"). The real `.zfab`
    // files differ by 0.17 % between the directions; a test material built by
    // hand has no reason to copy that.
    biegung_kette_nm: 2482.2424e-9 * 8.0,
    biegung_schraege_nm: 2482.2424e-9 * 8.0,
    flaechengewicht_kg_m2: 0.698,
    dicke_m: 0.8e-3,
    reibung: 0.03,
};

/// Two layers 20 × 4 cm on top of each other, 0.4 mm apart, no gravity.
/// The lower one is held along the short edge x = 0.
fn zwei_lagen(geklebt: bool) -> Szene {
    let m = read_str(&rechteck_muster(0.2, 0.04)).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let p = Parameter { kante_mm: 10.0, ..Parameter::default() };
    let mut netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &p).unwrap();
    assert!(netz.fehler.is_empty(), "{:?}", netz.fehler);
    let oben = netz.teile[0].clone();
    netz.teile.push(oben);
    let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
    // Die obere Lage liegt als Folie direkt auf: ihr Ruheversatz ist die Dicke.
    let lage = |ti: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, ti as f64 * 0.4e-3];
    let param = Simparameter {
        bild_dt: 1.0 / 30.0,
        daempfung: 1.0,
        schwerkraft: [0.0; 3],
        ..Simparameter::default()
    };
    let mut s = Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), param);
    // Untere Lage am kurzen Rand halten.
    for i in s.teile[0].punkte.clone() {
        if s.x[i][0].abs() < 1e-6 {
            s.w[i] = 0.0;
        }
    }
    let oben_r = s.teile[1].punkte.clone();
    let unten_r = s.teile[0].punkte.clone();
    if geklebt {
        let paar = Stoffpaar {
            dehnung_oben: RADICAL_RUBBER_04.dehnung_n_m,
            dicke_oben: RADICAL_RUBBER_04.dicke_m,
            dicke_unten: RADICAL_RUBBER_04.dicke_m,
            biegung_oben: RADICAL_RUBBER_04.biegung_nm,
            biegung_unten: RADICAL_RUBBER_04.biegung_nm,
        };
        let (zone, ohne) = Zone::flaeche("laminat", &s.x, &s.dreiecke, oben_r, unten_r, 0.01, paar);
        assert_eq!(ohne, 0, "{ohne} Punkte ohne Gegenstueck");
        assert!(!zone.is_empty(), "keine Bindungen");
        s.klebungen_setzen(vec![zone]);
    }
    s
}

/// Verschiebt das freie Ende (`frei`: Punktauswahl) um `weg` in y und hält es
/// dort. Gibt die Zahl der festgelegten Punkte zurück.
fn ende_verschieben(s: &mut Szene, teil: usize, weg: f64) -> usize {
    let mut n = 0;
    for i in s.teile[teil].punkte.clone() {
        if s.x[i][0] > 0.2 - 1e-6 {
            s.x[i][1] += weg;
            s.v[i] = [0.0; 3];
            s.w[i] = 0.0;
            n += 1;
        }
    }
    n
}

/// Größte Verschiebung in y am freien Ende eines Teils gegenüber dem Start, in
/// mm.
fn auslenkung(s: &Szene, teil: usize, x0: &[[f64; 3]]) -> f64 {
    s.teile[teil]
        .punkte
        .clone()
        .filter(|&i| s.x[i][0] > 0.2 - 1e-6)
        .map(|i| (s.x[i][1] - x0[i][1]).abs())
        .fold(0.0, f64::max)
        * 1e3
}

#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn geklebte_lage_traegt_die_andere_mit() {
    let mut werte = Vec::new();
    for geklebt in [false, true] {
        let mut s = zwei_lagen(geklebt);
        let mut rw = ImplizitCg::neu(ImplizitParameter::default());
        // Erst zur Ruhe, dann das obere Ende 5 mm zur Seite und halten.
        for _ in 0..20 {
            rw.bild(&mut s);
        }
        let x0 = s.x.clone();
        let fest = ende_verschieben(&mut s, 1, 5e-3);
        assert!(fest > 0, "kein Punkt am freien Ende");
        for _ in 0..60 {
            rw.bild(&mut s);
        }
        werte.push((geklebt, auslenkung(&s, 0, &x0)));
        println!("  geklebt={geklebt}: untere Lage am freien Ende {:.3} mm", werte.last().unwrap().1);
    }
    let (lose, geklebt) = (werte[0].1, werte[1].1);
    println!("Auslenkung der unteren Lage: lose {lose:.3} mm, geklebt {geklebt:.3} mm");
    assert!(lose < 1.0, "ohne Klebung darf die untere Lage nicht folgen, sie folgt {lose:.3} mm");
    assert!(
        geklebt > 2.0,
        "mit Klebung muss die untere Lage folgen, sie folgt nur {geklebt:.3} mm"
    );
}

/// Sag of a strip held at both short ends, under its own weight, in mm: the
/// three cases of the task description, measured.
fn streifen_durchbiegung(lagen: usize, material: Material, kleben: bool) -> f64 {
    let m = read_str(&rechteck_muster(0.2, 0.04)).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let p = Parameter { kante_mm: 10.0, ..Parameter::default() };
    let mut netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &p).unwrap();
    let g = netz.teile[0].clone();
    for _ in 1..lagen {
        netz.teile.push(g.clone());
    }
    let vg = Teilvorgabe { material, reduktion: 0.0 };
    let dicke = material.dicke_m;
    let lage = |ti: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, ti as f64 * dicke];
    let param = Simparameter { bild_dt: 1.0 / 30.0, daempfung: 1.0, ..Simparameter::default() };
    let mut s = Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), param);
    // Beide kurzen Ränder halten: der Streifen hängt in der Mitte durch.
    for ti in 0..s.teile.len() {
        for i in s.teile[ti].punkte.clone() {
            if s.x[i][0].abs() < 1e-6 || s.x[i][0] > 0.2 - 1e-6 {
                s.w[i] = 0.0;
            }
        }
    }
    if kleben {
        let oben = s.teile[1].punkte.clone();
        let unten = s.teile[0].punkte.clone();
        let paar = Stoffpaar {
            dehnung_oben: RADICAL_RUBBER_04.dehnung_n_m,
            dicke_oben: RADICAL_RUBBER_04.dicke_m,
            dicke_unten: RADICAL_RUBBER_04.dicke_m,
            biegung_oben: RADICAL_RUBBER_04.biegung_nm,
            biegung_unten: RADICAL_RUBBER_04.biegung_nm,
        };
        let (zone, ohne) = Zone::flaeche("laminat", &s.x, &s.dreiecke, oben, unten, 0.01, paar);
        assert_eq!(ohne, 0);
        s.klebungen_setzen(vec![zone]);
    }
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    for _ in 0..120 {
        rw.bild(&mut s);
    }
    let ti = s.teile.len() - 1;
    let z0 = s.teile[ti].punkte.clone().map(|i| s.x[i][2]).fold(f64::NEG_INFINITY, f64::max);
    s.teile[ti]
        .punkte
        .clone()
        .filter(|&i| (s.x[i][0] - 0.1).abs() < 0.02)
        .map(|i| (z0 - s.x[i][2]) * 1e3)
        .fold(0.0, f64::max)
}

/// The deflection comparison of the task description, measured - and it does
/// not separate the cases. This is a property of the test piece, not a defect
/// of the glue, and the numbers say why:
///
/// * the sag needs the strip to lengthen; the tension that does it is
///   `stretch · strain · width`,
/// * two loose layers stretch in parallel, so the pair carries 2 × 348 N/m,
/// * the 0.8 mm laminate carries 696 N/m - the same,
/// * bending is irrelevant at 200 × 40 mm (its share of the sag is below the
///   print precision).
///
/// So all three sag the same and the load-transfer test above is the
/// discriminator. If this test ever fails, the deflection has started to
/// depend on bending, and the laminate comparison becomes meaningful again.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn durchbiegung_trennt_die_faelle_nicht_und_das_ist_erklaert() {
    let lose = streifen_durchbiegung(2, RADICAL_RUBBER_04, false);
    let geklebt = streifen_durchbiegung(2, RADICAL_RUBBER_04, true);
    let laminat = streifen_durchbiegung(1, LAMINAT_08, false);
    println!("Durchbiegung (mm): lose {lose:.3}  geklebt {geklebt:.3}  laminat {laminat:.3}");
    let max = lose.max(geklebt).max(laminat);
    for (name, w) in [("lose", lose), ("geklebt", geklebt), ("laminat", laminat)] {
        assert!(
            (w - max).abs() / max < 0.05,
            "{name} weicht von der Membranwirkung ab ({w:.3} gegen {max:.3} mm) - die Erklaerung oben gilt nicht mehr"
        );
    }
    // Und die Membranrechnung, die das erklaert: Zug aus der Dehnung.
    let zug_lose = 2.0 * RADICAL_RUBBER_04.dehnung_n_m;
    let zug_laminat = LAMINAT_08.dehnung_n_m;
    println!("Membranzug des Streifens (N/m): lose {zug_lose:.0}  laminat {zug_laminat:.0}");
    assert!((zug_lose - zug_laminat).abs() / zug_laminat < 0.01, "parallele Lagen tragen nicht dasselbe wie das Laminat");
}

/// A bonded pair must carry the plate bending stiffness of the laminate of the
/// same total thickness, not just the sum of the two layers.
///
/// Theory from MATERIAL.md: one 0.4 mm layer is 2,482 (in N·m², 2,482·10⁻⁹);
/// two loose layers in parallel are `2 × 2,482 = 4,964`; the bonded pair about a
/// common neutral axis is `8 × 2,482 = 19,858`, exactly the
/// `Latex_Laminat_2x0.4mm` row. So the bonded pair must be **4×** the loose
/// pair and must match a single 0.8 mm layer within 10 %.
///
/// **Why bending energy and not a tip deflection.** A cantilever was tried
/// first (it is the obvious piece for bending) and does **not** work in this
/// model at the mesh sizes that solve: under scaled gravity the strip deforms
/// into a straight line - it shears, it does not bend - and making the bending
/// constraints 10× stiffer changes the tip by 0.03 % (measured: 3.6068 against
/// 3.6075 mm, profile 0/5/10/…/40 mm at 0.451 mm per step, exactly linear). A
/// deflection measurement therefore cannot see the bending stiffness here at
/// all; it is not the glue that is missing but the piece.
///
/// Bending energy at a **prescribed** curvature has no such problem. A strip
/// bent into a circular arc carries no membrane strain, so the energy is
/// `½·D·κ²·A` and nothing else. This test bends the mesh onto an exact arc and
/// sums the constraint energy ([`Szene::biegeenergie`]); it measures the plate
/// stiffness itself, independent of how stiff the mesh happens to be against
/// shear.
fn biegeenergie_im_bogen(lagen: usize, material: Material, kleben: bool) -> f64 {
    let m = read_str(&rechteck_muster(0.1, 0.02)).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let p = Parameter { kante_mm: 5.0, ..Parameter::default() };
    let mut netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &p).unwrap();
    assert!(netz.fehler.is_empty(), "{:?}", netz.fehler);
    let g = netz.teile[0].clone();
    for _ in 1..lagen {
        netz.teile.push(g.clone());
    }
    let vg = Teilvorgabe { material, reduktion: 0.0 };
    let dicke = material.dicke_m;
    // Bogen mit Kruemmung KRUMMUNG um die y-Achse: reine Biegung, die
    // Bogenlaenge bleibt erhalten, keine Dehnung.
    let r = 1.0 / KRUMMUNG;
    let lage = |ti: usize, q: [f64; 2]| {
        let s_ = q[0] * 1e-3;
        [r * (s_ / r).sin(), q[1] * 1e-3, ti as f64 * dicke + r * (1.0 - (s_ / r).cos())]
    };
    let param = Simparameter {
        bild_dt: 1.0 / 30.0,
        schwerkraft: [0.0; 3],
        ..Simparameter::default()
    };
    let mut s = Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), param);
    if kleben {
        let oben = s.teile[1].punkte.clone();
        let unten = s.teile[0].punkte.clone();
        let paar = Stoffpaar {
            dehnung_oben: RADICAL_RUBBER_04.dehnung_n_m,
            dicke_oben: RADICAL_RUBBER_04.dicke_m,
            dicke_unten: RADICAL_RUBBER_04.dicke_m,
            biegung_oben: RADICAL_RUBBER_04.biegung_nm,
            biegung_unten: RADICAL_RUBBER_04.biegung_nm,
        };
        let (zone, ohne) = Zone::flaeche("laminat", &s.x, &s.dreiecke, oben, unten, 0.01, paar);
        assert_eq!(ohne, 0, "{ohne} Punkte ohne Gegenstueck");
        s.klebungen_setzen(vec![zone]);
    }
    s.biegeenergie()
}

/// Curvature of the test arc in 1/m: a 10 cm strip bent to a 1 m radius, i.e.
/// 10 cm of arc over a 10° turn. Gentle enough that the arc chord differs from
/// the arc length by well under a mesh edge, so the pre-bent mesh carries
/// essentially no membrane strain.
const KRUMMUNG: f64 = 1.0;
const KRUMMUNG_L: f64 = 0.1;
const KRUMMUNG_B: f64 = 0.02;

/// The acceptance of task #13: the bonded pair carries the laminate's plate
/// stiffness.
///
/// **What is asserted and what is not.** The *ratios* are what this piece
/// measures cleanly, because all three cases share one mesh: bent onto the same
/// arc, every case has the same dihedral angles, so `½·Σθ²/α` differs only by
/// `1/α`. The **absolute** value is off by about 20 % against `½·D·κ²·A` - a
/// discretisation effect of the 5 mm arc mesh, identical in all three cases and
/// therefore not something this test can use. It asserts the ratios and prints
/// the absolute numbers; it does not assert the discretised absolute value.
#[test]
fn geklebtes_paar_traegt_die_plattensteifigkeit_des_laminats() {
    let lose = biegeenergie_im_bogen(2, RADICAL_RUBBER_04, false);
    let geklebt = biegeenergie_im_bogen(2, RADICAL_RUBBER_04, true);
    let laminat = biegeenergie_im_bogen(1, LAMINAT_08, false);
    let a = KRUMMUNG_L * KRUMMUNG_B;
    let soll = |d: f64| 0.5 * d * KRUMMUNG * KRUMMUNG * a;
    println!(
        "Biegeenergie im Bogen (µJ): lose {:.4}  geklebt {:.4}  laminat {:.4}",
        lose * 1e6,
        geklebt * 1e6,
        laminat * 1e6
    );
    println!(
        "  Soll ½·D·κ²·A (µJ): 2x lose {:.4}  Laminat {:.4}  (D = 2,482 bzw. 19,858 ·10⁻⁹ N·m²)",
        soll(2.0 * RADICAL_RUBBER_04.biegung_nm) * 1e6,
        soll(LAMINAT_08.biegung_nm) * 1e6
    );

    // (1) Das geklebte Paar trifft das Laminat auf 10 %.
    let abw = (geklebt - laminat).abs() / laminat;
    assert!(
        abw < 0.10,
        "geklebtes Paar weicht {:.1} % vom Laminat ab ({:.4} gegen {:.4} µJ)",
        abw * 100.0,
        geklebt * 1e6,
        laminat * 1e6
    );
    // (2) Und es traegt das Vierfache zweier loser Lagen - der Sprung, den die
    // Klebung liefern muss (2 x 2,482 gegen 8 x 2,482).
    let verhaeltnis = geklebt / lose;
    assert!(
        (verhaeltnis - 4.0).abs() / 4.0 < 0.15,
        "geklebtes Paar traegt das {verhaeltnis:.3}-fache der losen Lagen, erwartet 4"
    );
    // (3) Und jeder Fall trifft seine eigene Sollkurve: die lose Doppellage
    // ½·(2D)·κ²·A, das geklebte Paar ½·(8D)·κ²·A. Damit ist ausgeschlossen,
    // dass beide nur zufaellig gleich weit danebenliegen.
    let soll_geklebt = soll(LAMINAT_08.biegung_nm);
    let soll_lose = soll(2.0 * RADICAL_RUBBER_04.biegung_nm);
    for (name, ist, soll_wert) in [("lose", lose, soll_lose), ("geklebt", geklebt, soll_geklebt)] {
        let r = ist / soll_wert;
        assert!(
            (r - 1.0).abs() < 0.15,
            "{name}: {ist:.5} gegen Soll {soll_wert:.5} µJ ({:.1} % daneben)",
            (r - 1.0) * 100.0
        );
    }
    println!(
        "  geklebt/lose = {verhaeltnis:.4} (Soll 4), geklebt/Laminat = {:.4}, \
         gegen die Sollkurven lose {:.3}x / geklebt {:.3}x",
        geklebt / laminat,
        lose / soll_lose,
        geklebt / soll_geklebt
    );
}

/// A zone that covers **part** of a piece must stiffen only that part, a scene
/// without a zone must keep every bending constraint exactly as built, and
/// setting the zones twice must not compound the factor.
///
/// **The border band.** The bonded area is defined through the bonds: the cloth
/// points they hang on and the corners of the triangles they project into. A
/// triangle on the border of the zone has some corners inside and some outside,
/// so it stays out - but a triangle one step outside can still have all three
/// corners inside that set, because those corners are corners of bonded
/// triangles. The stiffened region therefore reaches **one mesh element** past
/// the bonded points. That is measured here, not hidden: the test asserts that
/// nothing beyond `rand` is touched, and prints the band it actually found.
/// Physically it is the half-covered boundary element, which does carry part of
/// the glued load; what must not happen - and does not - is the whole panel
/// going rigid.
#[test]
fn biegeterm_bleibt_in_der_klebeflaeche() {
    // Ein Teil, 100 x 20 mm, allein: die Zone bindet es an sich selbst und deckt
    // nur die linke Haelfte (x < 50 mm).
    let m = read_str(&rechteck_muster(0.1, 0.02)).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let p = Parameter { kante_mm: 5.0, ..Parameter::default() };
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &p).unwrap();
    let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
    let lage = |_ti: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0];
    let mut s = Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), Simparameter::default());
    let grund = s.biegung.nachgiebigkeit.clone();
    assert_eq!(grund, s.biegung_grund, "Grundnachgiebigkeit nicht mitgeschrieben");

    // Ohne Zone: nichts angefasst.
    s.klebungen_setzen(Vec::new());
    assert_eq!(s.biegung.nachgiebigkeit, grund, "ohne Zone muss alles unveraendert bleiben");

    const GRENZE: f64 = 0.05;
    let mut bindungen = Vec::new();
    for (i, q) in s.x.clone().iter().enumerate() {
        if q[0] > GRENZE + 1e-9 {
            continue;
        }
        for d in &s.dreiecke {
            if d.contains(&(i as u32)) {
                let bary = if d[0] == i as u32 {
                    [1.0, 0.0, 0.0]
                } else if d[1] == i as u32 {
                    [0.0, 1.0, 0.0]
                } else {
                    [0.0, 0.0, 1.0]
                };
                bindungen.push(klon3d_sim::klebung::Bindung { punkt: i as u32, dreieck: *d, bary, versatz: [0.0; 3] });
                break;
            }
        }
    }
    let zone = Zone { name: "haelfte".into(), bindungen, steifigkeit: 1.0, biegefaktor: 4.0 };
    let idx = zone.biege_constraints(&s);
    assert!(!idx.is_empty(), "keine Biege-Constraint in der halben Flaeche");
    s.klebungen_setzen(vec![zone]);

    // Wie weit reicht der gesteifte Bereich ueber die Klebegrenze hinaus?
    let kante = 5e-3;
    let weiteste = idx
        .iter()
        .flat_map(|&c| s.biegung.punkte[c].iter().map(|&i| s.x[i as usize][0]))
        .fold(f64::NEG_INFINITY, f64::max);
    let rand = weiteste - GRENZE;
    println!(
        "  gesteift: {} von {} Biege-Constraints, weitester Punkt {:.1} mm, Saum {:.1} mm (Kante {:.0} mm)",
        idx.len(),
        s.biegung.len(),
        weiteste * 1e3,
        rand * 1e3,
        kante * 1e3
    );
    assert!(
        rand <= 1.5 * kante,
        "der Biegeterm reicht {:.1} mm ueber die Klebeflaeche hinaus, mehr als eineinhalb Kanten",
        rand * 1e3
    );
    // Ein Teil der Constraints ist gesteift, ein Teil nicht - sonst prueft der
    // Test nichts.
    assert!(idx.len() < s.biegung.len(), "alle Constraints gesteift - die Begrenzung greift nicht");
    assert!(idx.len() > s.biegung.len() / 4, "kaum etwas gesteift - die Zone wirkt nicht");

    // Rechts der Klebeflaeche plus Saum ist keine einzige Constraint angefasst.
    for (c, p) in s.biegung.punkte.iter().enumerate() {
        if p.iter().all(|&i| s.x[i as usize][0] > GRENZE + 1.5 * kante) {
            assert_eq!(
                s.biegung.nachgiebigkeit[c], grund[c],
                "Constraint {c} weit rechts der Klebeflaeche wurde gesteift"
            );
        }
    }

    // Wiederholtes Setzen kumuliert nicht.
    let nach_erstem = s.biegung.nachgiebigkeit.clone();
    let bindungen2 = {
        let mut v = Vec::new();
        for (i, q) in s.x.clone().iter().enumerate() {
            if q[0] > GRENZE + 1e-9 {
                continue;
            }
            for d in &s.dreiecke {
                if d.contains(&(i as u32)) {
                    let bary = if d[0] == i as u32 {
                        [1.0, 0.0, 0.0]
                    } else if d[1] == i as u32 {
                        [0.0, 1.0, 0.0]
                    } else {
                        [0.0, 0.0, 1.0]
                    };
                    v.push(klon3d_sim::klebung::Bindung { punkt: i as u32, dreieck: *d, bary, versatz: [0.0; 3] });
                    break;
                }
            }
        }
        v
    };
    s.klebungen_setzen(vec![Zone { name: "haelfte".into(), bindungen: bindungen2, steifigkeit: 1.0, biegefaktor: 4.0 }]);
    assert_eq!(s.biegung.nachgiebigkeit, nach_erstem, "zweites Setzen hat den Faktor erneut angewandt");

    // Und ohne Zone wieder exakt der Ausgangszustand.
    s.klebungen_setzen(Vec::new());
    assert_eq!(s.biegung.nachgiebigkeit, grund, "Zuruecknehmen stellt die Nachgiebigkeit nicht wieder her");
}
