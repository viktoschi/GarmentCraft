//! Der Körper als Netz im Solver (Schritte 4c/4d).
//!
//! Befund 94.
//!
//! 1. **Röhre auf dem Zylindernetz:** derselbe Prüffall wie Befund 34/35, der
//!    Zylinder aber als Dreiecksnetz (360 Segmente, 79.920 Dreiecke) statt
//!    analytisch. Er muss dieselbe Abnahme bestehen und dasselbe Gleichgewicht
//!    finden wie der analytische Zylinder.
//! 2. **Band um die Taille:** ein 2 cm hohes Band aus dem Rechteckmuster der
//!    Röhre, an der schmalsten Stelle des Rumpfs um den Körper aus
//!    `klon3d koerper` gelegt (5 mm Abstand), Ruhelänge = Umfang × (1 − r).
//!    Es zieht sich zusammen und muss wie ein Maßband liegen: jede Kante so
//!    lang wie die konvexe Hülle des Rumpfschnitts in ihrer Ebene + π·t (die
//!    Mittelfläche liegt t/2 über der Haut), kein Punkt in der Haut.
//! 3. **Punkteregen:** lose Punkte fallen mit bis zu 6 m/s auf den Körper; die
//!    Durchstoßprüfung des impliziten Schritts hält sie draußen.
//!
//! Die Körpertests brauchen CLOs Resizing-Daten; fehlen sie, sagen sie das und
//! hören auf. Nur mit `cargo test --release`.

use std::time::Instant;

use klon3d_sim::roehre::{bis_zur_ruhe, param_ruhe, Ergebnis, Roehre};
use klon3d_sim::{CpuReferenz, ImplizitCg, ImplizitParameter, Rechenweg, RADICAL_RUBBER_04, RETIFIST_NR_040};

mod band;

fn weg_diskret(a: f64, h: f64, t: f64, teilschritte: usize) -> f64 {
    let h = h / teilschritte.max(1) as f64;
    let n = (t / h).round();
    a * h * h * n * (n + 1.0) / 2.0
}

fn gleichgewicht(r: &Roehre, rw: &mut dyn Rechenweg) -> Ergebnis {
    let mut s = r.szene(param_ruhe()).unwrap();
    let l = bis_zur_ruhe(&mut s, rw, 1200, 1e-4, 10);
    assert!(l.ruhig, "{} {:.1} %, Netz {}: {l:?}", r.material.name, r.reduktion * 100.0, r.zylinder_als_netz);
    r.auswerten(&s)
}

/// Impliziter Schritt mit CLOs Einstellungen: die Röhre auf dem Netz besteht
/// die Abnahme von Befund 35 und hat dieselbe Umfangsdehnung wie auf dem
/// analytischen Zylinder (auf 0,05 %-Punkte). Die Verteilung über die
/// Dreiecke vergleicht der reibungsfreie Test unten: mit Reibung hängt sie
/// vom Weg ab (Befund 35).
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn roehre_auf_netz_wie_auf_zylinder() {
    for m in [RADICAL_RUBBER_04, RETIFIST_NR_040] {
        for red in [0.055, 0.10] {
            let zyl = Roehre { material: m, reduktion: red, ..Roehre::default() };
            let netz = Roehre { zylinder_als_netz: true, ..zyl };
            let t0 = Instant::now();
            let a = gleichgewicht(&zyl, &mut ImplizitCg::neu(ImplizitParameter::default()));
            let t_zyl = t0.elapsed().as_secs_f64();
            let t0 = Instant::now();
            let b = gleichgewicht(&netz, &mut ImplizitCg::neu(ImplizitParameter::default()));
            let t_netz = t0.elapsed().as_secs_f64();
            let was = format!("{} {:.1} %", m.name, red * 100.0);
            eprintln!(
                "{was}: ε Zylinder {:.4} %, Netz {:.4} % (Soll {:.4} %), Naht {:.3} / {:.3} mm, abheben {:.3} / {:.3} mm, {:.2} s / {:.2} s",
                a.dehnung * 100.0, b.dehnung * 100.0, b.dehnung_soll * 100.0,
                a.nahtspalt_m * 1e3, b.nahtspalt_m * 1e3, a.abheben_m * 1e3, b.abheben_m * 1e3, t_zyl, t_netz
            );
            assert!((b.dehnung - b.dehnung_soll).abs() <= 0.005, "{was}: ε {:.4}", b.dehnung);
            assert!(b.abheben_m < 0.5e-3, "{was}: abheben {:.3} mm", b.abheben_m * 1e3);
            assert!(b.nahtspalt_m < 1.0e-3, "{was}: Naht {:.3} mm", b.nahtspalt_m * 1e3);
            assert!((a.dehnung - b.dehnung).abs() < 5e-4, "{was}: ε {:.5} gegen {:.5}", a.dehnung, b.dehnung);
        }
    }
}

/// Ohne Reibung, CG auskonvergiert: auf dem Netz dasselbe Gleichgewicht wie
/// auf dem analytischen Zylinder, mit den Toleranzen des Vergleichs XPBD
/// gegen implizit (Befund 35): Median 0,02, 95-%-Wert 0,1 %-Punkte.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn ohne_reibung_dasselbe_gleichgewicht_wie_zylinder() {
    let mut erg = Vec::new();
    for netz in [false, true] {
        let r = Roehre { material: RETIFIST_NR_040, zylinder_als_netz: netz, ..Roehre::default() };
        let mut s = r.szene(param_ruhe()).unwrap();
        for x in s.reibung.iter_mut() {
            *x = 0.0;
        }
        let mut rw = ImplizitCg::neu(ImplizitParameter { cg_max: 3000, cg_toleranz: 1e-8, naht_k: 3e4, ..Default::default() });
        let l = bis_zur_ruhe(&mut s, &mut rw, 1200, 1e-5, 10);
        assert!(l.ruhig, "Netz {netz}: {l:?}");
        erg.push(r.auswerten(&s));
    }
    let (z, n) = (erg[0], erg[1]);
    eprintln!("Median {:.5} / {:.5}, P95 {:.5} / {:.5}", z.dehnung_median, n.dehnung_median, z.dehnung_p95, n.dehnung_p95);
    assert!((z.dehnung_median - n.dehnung_median).abs() < 2e-4, "Median {} gegen {}", z.dehnung_median, n.dehnung_median);
    assert!((z.dehnung_p95 - n.dehnung_p95).abs() < 1e-3, "P95 {} gegen {}", z.dehnung_p95, n.dehnung_p95);
}

/// Rutschprobe auf dem Netz wie Befund 35: frei fallend, haltend (Radical
/// 5,5 %), rutschend nach Coulomb (Retifist).
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn rutschprobe_auf_netz() {
    let h = 1.0 / 60.0;
    let n = ImplizitParameter::default().schritte_je_bild;
    let frei = Roehre { reduktion: 0.0, zylinder_als_netz: true, ..Roehre::default() };
    let p = frei.rutschprobe(&mut ImplizitCg::neu(ImplizitParameter::default()), 0.5).unwrap();
    let soll = weg_diskret(9.81 * (1.0 - p.halteverhaeltnis_gemessen), h, p.sekunden, n);
    eprintln!("frei: {:.1} mm gegen {:.1} mm", p.weg_m * 1e3, soll * 1e3);
    assert!((p.weg_m / soll - 1.0).abs() < 0.005, "freier Fall {:.1} mm gegen {:.1} mm", p.weg_m * 1e3, soll * 1e3);

    let halten = Roehre { zylinder_als_netz: true, ..Roehre::default() };
    let p = halten.rutschprobe(&mut ImplizitCg::neu(ImplizitParameter::default()), 0.5).unwrap();
    eprintln!("halten: q {:.2}, {:.3} mm", p.halteverhaeltnis_gemessen, p.weg_m * 1e3);
    assert!(p.halteverhaeltnis_gemessen > 1.5);
    assert!(p.weg_m < 1e-3, "Radical 5,5 % rutscht {:.2} mm", p.weg_m * 1e3);

    let rutschen = Roehre { material: RETIFIST_NR_040, zylinder_als_netz: true, ..Roehre::default() };
    let p = rutschen.rutschprobe(&mut ImplizitCg::neu(ImplizitParameter::default()), 0.5).unwrap();
    let soll = weg_diskret(9.81 * (1.0 - p.halteverhaeltnis_gemessen), h, p.sekunden, n);
    eprintln!("rutschen: q {:.3}, {:.1} mm gegen {:.1} mm", p.halteverhaeltnis_gemessen, p.weg_m * 1e3, soll * 1e3);
    assert!((p.weg_m / soll - 1.0).abs() < 0.02, "Retifist {:.1} mm gegen {:.1} mm", p.weg_m * 1e3, soll * 1e3);
}

/// Der XPBD-Prüfweg auf dem Netz: dieselbe Abnahme wie Befund 34 an einem
/// Fall (Retifist 5,5 %, die wenigsten Unterschritte).
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn xpbd_auf_netz() {
    let r = Roehre { material: RETIFIST_NR_040, zylinder_als_netz: true, ..Roehre::default() };
    let t0 = Instant::now();
    let e = gleichgewicht(&r, &mut CpuReferenz::neu());
    eprintln!("XPBD Netz: ε {:.4} % (Soll {:.4} %), Naht {:.4} mm, abheben {:.3} mm, {:.1} s", e.dehnung * 100.0, e.dehnung_soll * 100.0, e.nahtspalt_m * 1e3, e.abheben_m * 1e3, t0.elapsed().as_secs_f64());
    assert!((e.dehnung - e.dehnung_soll).abs() <= 0.005);
    assert!(e.nahtspalt_m < 0.5e-3);
    assert!(e.abheben_m < 0.5e-3);
}

/// Band um die Taille, impliziter Schritt mit CLOs Einstellungen, 10-mm-Netz:
/// kommt zur Ruhe, kein Punkt mehr als 0,1 mm in der Haut, bleibt an der
/// Taille, und jede Kante ist so lang wie der Rumpf in ihrer Ebene wie ein
/// Maßband (+ π·t). Toleranz Retifist 1 %; Radical 3 %, weil sich seine
/// Kanten bis 11 mm aus der Ebene wellen und dadurch länger sind.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn band_um_die_taille() {
    let Some(k) = band::koerper() else { return };
    eprintln!("Körper: {} Punkte, {} Dreiecke, {} Randkanten, {:.1} l", k.punkte.len(), k.dreiecke.len(), k.randkanten, k.volumen * 1e3);
    let taille_z = band::taille(&k);
    for (m, red, tol) in [
        (RETIFIST_NR_040, 0.055, 0.01),
        (RETIFIST_NR_040, 0.10, 0.01),
        (RADICAL_RUBBER_04, 0.055, 0.03),
        (RADICAL_RUBBER_04, 0.10, 0.03),
    ] {
        let (mut s, muster) = band::band_szene(&k, taille_z, m, red, band::KANTE_MM);
        let t0 = Instant::now();
        let l = bis_zur_ruhe(&mut s, &mut ImplizitCg::neu(ImplizitParameter::default()), 2000, 1e-4, 10);
        let sekunden = t0.elapsed().as_secs_f64();
        let b = band::band_auswerten(&k, &s, &muster, taille_z);
        let was = format!("{} {:.1} %", m.name, red * 100.0);
        eprintln!("{was}: {l:?}, {sekunden:.2} s
  {b:?}");
        assert!(l.ruhig, "{was}: {l:?}");
        assert!(b.tiefster > -0.1e-3, "{was}: Punkt {:.3} mm in der Haut", b.tiefster * 1e3);
        assert!((b.mitte_z - b.taille_z).abs() < 0.01, "{was}: Band auf {:.1} cm statt {:.1} cm", b.mitte_z * 100.0, b.taille_z * 100.0);
        for e in &b.kanten {
            assert!((e.laenge / e.soll - 1.0).abs() < tol, "{was}: Kante {:.2} cm gegen Maßband {:.2} cm", e.laenge * 100.0, e.soll * 100.0);
        }
    }
}

/// Im 5-mm-Netz faltet sich das Radical-Band quer und kommt ohne
/// Selbstkollision nicht zur Ruhe (Befund 94, offen, ob das physikalisch
/// ist). Der Körper hält trotzdem: auch im Flattern kein Punkt in der Haut.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn band_faltet_und_bleibt_draussen() {
    let Some(k) = band::koerper() else { return };
    let taille_z = band::taille(&k);
    let (mut s, muster) = band::band_szene(&k, taille_z, RADICAL_RUBBER_04, 0.055, 5.0);
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let mut tiefster = f64::INFINITY;
    for bild in 1..=600 {
        rw.bild(&mut s);
        if bild % 50 == 0 {
            tiefster = tiefster.min(band::band_auswerten(&k, &s, &muster, taille_z).tiefster);
        }
    }
    let b = band::band_auswerten(&k, &s, &muster, taille_z);
    eprintln!("Radical 5,5 %, 5 mm: v {:.3} m/s, {b:?}", klon3d_sim::messung::mittlere_geschwindigkeit(&s));
    assert!(tiefster > -0.1e-3, "Punkt {:.3} mm in der Haut", tiefster * 1e3);
}

/// Point rain: 3 026 loose points fall from 1.75 m onto the body (up to
/// 5.9 m/s, i.e. up to 98 mm per step, ten times the search depth), friction
/// 0.6. Whose fall line first meets a surface flatter than 25° must stay
/// there: the impact takes the whole tangential velocity (tan α ≤ μ), and
/// static friction holds up to 31°. The hit point comes from a perpendicular
/// ray, not from the solver. With the swept contact test almost all stay at
/// their hit point (a few slide over a slope and fall), and after 1.5 s no
/// point is inside the skin; without it most fall through the body.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn punkteregen_bleibt_draussen() {
    let Some(k) = band::koerper() else { return };
    let start = band::punkteregen(&k).x;
    // Erwartete Lage: erster Treffer des senkrechten Strahls, Fläche flacher als 25°.
    let soll: Vec<Option<[f64; 3]>> = start
        .iter()
        .map(|&p| {
            let (q, n, _) = k.durchstoss(p, [p[0], p[1], -0.5], [0.0; 3])?;
            (n[2] >= 25f64.to_radians().cos()).then_some(q)
        })
        .collect();
    let flach = soll.iter().flatten().count();
    let mut liegen = Vec::new();
    for pruefen in [true, false] {
        let mut s = band::punkteregen(&k);
        let mut rw = ImplizitCg::neu(ImplizitParameter { durchstoss_pruefen: pruefen, ..Default::default() });
        let mut stoesse = 0;
        for _ in 0..90 {
            rw.bild(&mut s);
            stoesse += rw.statistik.durchstoesse;
        }
        let drin = s.x.iter().filter(|&&p| k.naechster_alle(p).abstand < -0.1e-3).count();
        let da = soll
            .iter()
            .zip(&s.x)
            .filter(|(q, p)| q.is_some_and(|q| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() < 0.005))
            .count();
        eprintln!("Durchstoßprüfung {pruefen}: {drin} von {} in der Haut, {da} von {flach} flach getroffenen liegen an der Treffstelle, {stoesse} Durchstöße", s.x.len());
        if pruefen {
            assert_eq!(drin, 0, "mit Durchstoßprüfung");
        }
        liegen.push(da);
    }
    assert!(flach > 60, "nur {flach} flache Treffer");
    // At 5 substeps two points slide off over the shoulder slope; the check
    // keeps the rest at their hit point.
    assert!(liegen[0] >= flach - 4, "mit Durchstoßprüfung liegen zu wenige an der Treffstelle: {liegen:?}");
    assert!(liegen[1] < flach / 2, "ohne Durchstoßprüfung sollten die meisten durchfallen: {liegen:?}");
}

/// Tuch auf einer Spitze (Befund 96): ein 10 × 10 cm großes Tuch (10-mm-Netz)
/// fällt aus 5 mm auf die Spitze eines Tetraeders. Die Spitze liegt zwischen
/// den Stoffpunkten; ohne Körperpunkt gegen Stoffdreieck sticht sie durch das
/// Tuch, mit ihm nicht.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn tuch_ueber_spitze() {
    use klon3d_mesh::{vernetze, Parameter};
    use klon3d_pattern::geom::Auswertung;
    use klon3d_pattern::kanten::Kurvenquelle;
    use klon3d_sim::netzkoerper::SUCHTIEFE;
    use klon3d_sim::roehre::rechteck_muster;
    use klon3d_sim::{Koerper, Koerpernetz, Simparameter, Szene, Teilvorgabe};

    let p = vec![[-0.02, -0.02, -0.03], [0.02, -0.02, -0.03], [0.0, 0.03, -0.03], [0.0, 0.0, 0.01]];
    let d = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
    let spitze = Koerpernetz::neu(p, d, SUCHTIEFE).unwrap();
    let m = klon3d_pattern::read_str(&rechteck_muster(0.1, 0.1)).unwrap();
    let ev = Auswertung::new(&m, std::collections::HashMap::new());
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &Parameter::default()).unwrap();
    let vg = Teilvorgabe { material: RETIFIST_NR_040, reduktion: 0.0 };
    let param = Simparameter { schwerkraft: [0.0, 0.0, -9.81], daempfung: 1.0, ..Simparameter::default() };
    let mut tiefste = Vec::new();
    for durchgaenge in [2, 0] {
        let mut s = Szene::aus_netz(
            &netz,
            &|_| vg,
            &|_, q| [q[0] * 1e-3 - 0.0467, q[1] * 1e-3 - 0.0533, 0.015],
            vec![Koerper::netz(spitze.clone())],
            param,
        );
        let mut rw = ImplizitCg::neu(ImplizitParameter { koerperpunkt_durchgaenge: durchgaenge, ..Default::default() });
        for _ in 0..30 {
            rw.bild(&mut s);
        }
        // Wie tief sticht die Spitze durch die Mittelfläche des Tuchs?
        let mut t_min = 0.0f64;
        spitze.stiche(SUCHTIEFE, 0.0, [0.0; 3], &s.dreiecke, &s.x, |_, _, _, _, t| t_min = t_min.min(t));
        let z_min = s.x.iter().map(|q| q[2]).fold(f64::INFINITY, f64::min);
        eprintln!("Durchgänge {durchgaenge}: Spitze {:.2} mm durch den Stoff, tiefster Stoffpunkt z {:.1} mm", -t_min * 1e3, z_min * 1e3);
        tiefste.push(t_min);
    }
    assert!(tiefste[0] > -1e-6, "mit Körperpunkt gegen Stoffdreieck sticht die Spitze {:.3} mm durch", -tiefste[0] * 1e3);
    assert!(tiefste[1] < -1e-3, "ohne sollte die Spitze durchstechen ({:.3} mm)", -tiefste[1] * 1e3);
}
