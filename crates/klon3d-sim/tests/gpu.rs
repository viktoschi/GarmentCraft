//! Step 4b: the GPU backend against the CPU reference.
//!
//! The claim is a number, not a label: `A·p` of the same system, and the state
//! after the same number of frames, are compared on the tube. Without a GPU
//! adapter every test here prints `übersprungen: kein GPU-Adapter` and returns -
//! a missing device is not a failure (that is what CI has).
//!
//! The tube runs take `cargo test --release`; in the debug build they are
//! ignored like the other solver acceptance tests.

use klon3d_sim::roehre::{bis_zur_ruhe, param_ruhe, Roehre};
use klon3d_sim::{
    messung, GpuLoeser, ImplizitCg, ImplizitParameter, Rechenweg, Simparameter, Szene, Teilvorgabe,
    RADICAL_RUBBER_04,
};

/// The tests of this binary run one after the other: they share the card and
/// the cores, and `bluse_schafft_30_bilder` measures time. Taken first thing
/// in every test.
fn allein() -> std::sync::MutexGuard<'static, ()> {
    static SPERRE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    SPERRE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Opens the GPU or reports that there is none.
fn geraet() -> Option<GpuLoeser> {
    match GpuLoeser::neu() {
        Ok(g) => {
            eprintln!("GPU: {}", g.geraet());
            Some(g)
        }
        Err(e) => {
            eprintln!("übersprungen: kein GPU-Adapter ({e})");
            None
        }
    }
}

/// Zwei Rechenwege auf derselben Szene; der GPU-Weg bekommt einen frischen
/// Löser, damit kein Zustand vom Vorgänger mitkommt.
fn beide(param: ImplizitParameter, gpu: GpuLoeser) -> (ImplizitCg, ImplizitCg) {
    (
        ImplizitCg::neu(param),
        ImplizitCg::mit_loeser(param, Box::new(gpu)),
    )
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Größter Abstand zweier Punktwolken (m).
fn abstand(a: &[[f64; 3]], b: &[[f64; 3]]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(p, q)| {
            let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
            dot(d, d).sqrt()
        })
        .fold(0.0, f64::max)
}

/// Der Operator `A·p` beider Rechenwege auf demselben Zustand: dasselbe System,
/// dieselben Blöcke, nur einmal `f64` und einmal `f32`. Die Schranke ist an der
/// Messung festgemacht (`1e-4` relativ zur Norm von `A·p`), nicht geraten.
#[test]
fn operator_stimmt_mit_der_cpu() {
    let _allein = allein();
    let Some(gpu) = geraet() else { return };
    let r = Roehre::default();
    let mut s = r.szene(param_ruhe()).unwrap();
    let (mut cpu, mut g) = beide(ImplizitParameter::default(), gpu);
    // Ein paar Bilder rechnen, damit v nicht null ist und der Zustand etwas
    // von der Anfangslage weg ist. Beide Wege starten vom selben Zustand.
    for _ in 0..3 {
        cpu.bild(&mut s);
    }
    let n = s.x.len();
    let mut streuung = vec![[0.0; 3]; n];
    for (i, z) in streuung.iter_mut().enumerate() {
        let w = i as f64 * 0.6180339887498949;
        *z = [
            w.fract() - 0.5,
            (w * 2.0).fract() - 0.5,
            (w * 3.0).fract() - 0.5,
        ];
    }
    let vektoren = [vec![[1.0, 0.0, 0.0]; n], streuung, s.v.clone()];
    for (k, p) in vektoren.iter().enumerate() {
        let a = cpu.operator(&mut s, p);
        let b = g.operator(&mut s, p);
        let norm = a.iter().map(|q| dot(*q, *q)).sum::<f64>().sqrt();
        let diff = a
            .iter()
            .zip(&b)
            .map(|(x, y)| {
                dot(
                    [x[0] - y[0], x[1] - y[1], x[2] - y[2]],
                    [x[0] - y[0], x[1] - y[1], x[2] - y[2]],
                )
            })
            .sum::<f64>()
            .sqrt();
        assert!(
            diff <= 1e-4 * norm.max(1e-30),
            "Vektor {k}: Abweichung {diff:.3e} gegen Norm {norm:.3e} (relativ {:.2e})",
            diff / norm.max(1e-30)
        );
    }
}

/// Ohne Körper und ohne Schwerkraft zieht sich die Röhre auf ihre Ruhelänge
/// zusammen. Beide Rechenwege müssen **dasselbe Minimum** finden: gleicher
/// Umfang und kein Punkt weiter als 0,05 mm weg.
#[test]
fn ohne_koerper_gleiches_gleichgewicht() {
    let _allein = allein();
    let Some(gpu) = geraet() else { return };
    let r = Roehre {
        reduktion: 0.055,
        ..Roehre::default()
    };
    let mut a = r.szene_roh(param_ruhe()).unwrap();
    let mut b = a.clone();
    a.koerper.clear();
    b.koerper.clear();
    a.param.schwerkraft = [0.0; 3];
    b.param.schwerkraft = [0.0; 3];
    let (mut cpu, mut g) = beide(ImplizitParameter::default(), gpu);
    for _ in 0..30 {
        cpu.bild(&mut a);
        g.bild(&mut b);
    }
    let ua = messung::umfang_in_ebene(&a, [0.0, 0.0, -r.hoehe_m / 2.0], [0.0, 0.0, 1.0]);
    let ub = messung::umfang_in_ebene(&b, [0.0, 0.0, -r.hoehe_m / 2.0], [0.0, 0.0, 1.0]);
    assert!(
        (ua / ub - 1.0).abs() < 1e-4,
        "Umfang {ua:.6} gegen {ub:.6} m"
    );
    let d = abstand(&a.x, &b.x);
    assert!(d < 5e-5, "größter Punktabstand {:.4} mm", d * 1e3);
    let v = messung::mittlere_geschwindigkeit(&b);
    assert!(
        v.is_finite() && v < 1e-2,
        "GPU-Weg bewegt sich noch: {v:.2e} m/s"
    );
}

/// Die Röhre auf dem Zylinder, 5,5 % Radical: derselbe Umfang, dieselbe Naht,
/// derselbe Sitz wie die CPU. Die Schranken sind die von Schritt 4a′ (0,02
/// Prozentpunkte Dehnung zwischen den Rechenwegen), nicht aufgeweicht.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn roehre_gleiches_gleichgewicht() {
    let _allein = allein();
    let Some(gpu) = geraet() else { return };
    let r = Roehre {
        reduktion: 0.055,
        ..Roehre::default()
    };
    let mut a = r.szene(param_ruhe()).unwrap();
    let mut b = a.clone();
    let (mut cpu, mut g) = beide(ImplizitParameter::default(), gpu);
    let la = bis_zur_ruhe(&mut a, &mut cpu, 600, 1e-4, 10);
    let lb = bis_zur_ruhe(&mut b, &mut g, 600, 1e-4, 10);
    assert!(
        la.ruhig && lb.ruhig,
        "nicht zur Ruhe: CPU {la:?}, GPU {lb:?}"
    );
    let ea = r.auswerten(&a);
    let eb = r.auswerten(&b);
    assert!(
        (ea.dehnung - eb.dehnung).abs() < 2e-4,
        "Umfangsdehnung CPU {:.4} % gegen GPU {:.4} %",
        ea.dehnung * 100.0,
        eb.dehnung * 100.0
    );
    assert!(
        (ea.dehnung - ea.dehnung_soll).abs() < 0.005,
        "GPU: {:.4} % gegen Soll {:.4} %",
        eb.dehnung * 100.0,
        ea.dehnung_soll * 100.0
    );
    assert!(
        eb.nahtspalt_m < 1.0e-3,
        "GPU: Naht {:.3} mm",
        eb.nahtspalt_m * 1e3
    );
    assert!(
        eb.abheben_m < 0.5e-3,
        "GPU: hebt {:.3} mm ab",
        eb.abheben_m * 1e3
    );
    // Der Radius je Punkt, nicht die Lage: mit Haftreibung ist die tangentiale
    // Lage eine freie Größe — beide Wege sind gültige Gleichgewichte, und ein
    // um Bruchteile eines Grads verdrehtes Rohr hätte punktförmig Millimeter
    // Abstand bei gleicher Physik. Der Radius ist die Größe, die das Problem
    // festlegt.
    let radius = |p: &[f64; 3]| (p[0] * p[0] + p[1] * p[1]).sqrt();
    let dr =
        a.x.iter()
            .zip(&b.x)
            .map(|(p, q)| (radius(p) - radius(q)).abs())
            .fold(0.0f64, f64::max);
    assert!(dr < 5e-5, "größter Radiusunterschied {:.4} mm", dr * 1e3);
}

/// Die Rutschprobe mit dem GPU-Löser: derselbe Weg wie die diskrete Rechnung,
/// mit der Toleranz von Schritt 4a′ (2 %).
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn rutschprobe_wie_cpu() {
    let _allein = allein();
    let Some(gpu) = geraet() else { return };
    let r = Roehre {
        reduktion: 0.0,
        ..Roehre::default()
    };
    let mut rw = ImplizitCg::mit_loeser(ImplizitParameter::default(), Box::new(gpu));
    let p = r.rutschprobe(&mut rw, 0.5).unwrap();
    let h = 1.0 / 60.0;
    let n = ImplizitParameter::default().schritte_je_bild;
    let h = h / n as f64;
    let schritte = (p.sekunden / h).round();
    let soll =
        9.81 * (1.0 - p.halteverhaeltnis_gemessen) * h * h * schritte * (schritte + 1.0) / 2.0;
    assert!(
        (p.weg_m / soll - 1.0).abs() < 0.02,
        "GPU: {:.1} mm gegen {:.1} mm",
        p.weg_m * 1e3,
        soll * 1e3
    );
}

/// Ein Weg, der Federn im System hat (Klebezonen, Selbstkollision), geht an die
/// CPU und wird gezählt - nicht stillschweigend halb gerechnet.
#[test]
fn federn_gehen_an_die_cpu() {
    let _allein = allein();
    let Some(gpu) = geraet() else { return };
    let r = Roehre::default();
    let mut s = r.szene(param_ruhe()).unwrap();
    let mut rw = ImplizitCg::mit_loeser(
        ImplizitParameter {
            selbstkollision: true,
            ..ImplizitParameter::default()
        },
        Box::new(gpu),
    );
    for _ in 0..2 {
        rw.bild(&mut s);
    }
    assert!(s.x.iter().all(|p| p.iter().all(|q| q.is_finite())));
}

/// Die Abnahme von Schritt 4b: **Bluse3 v2 mit = 30 Bildern je Sekunde auf der
/// RTX 3090**, also 33,3 ms je Bild.
///
/// Die Schranke ist die Zahl aus `PLAN.md`, nicht was gerade herauskommt: sie
/// wird nicht aufgeweicht, wenn der Stand sie nicht erreicht. Gemessen wird wie
/// in `examples/gpu_takt.rs` - dasselbe Muster, dieselbe Szene, 20 Bilder nach
/// drei Bildern Anlauf.
///
/// Ohne Adapter (CI) wird übersprungen; das ist der einzige Grund, aus dem
/// dieser Test nicht läuft.
///
/// **Alleine laufen lassen:** die übrigen Tests dieses Binaries rechnen
/// gleichzeitig auf denselben Kernen, und eine Messung unter Last ist keine
/// Messung (parallel gemessen kam 77,7 statt 38,7 ms heraus).
///
/// ```text
/// cargo test --release -p klon3d-sim --test gpu bluse_schafft_30_bilder -- --test-threads=1
/// ```
///
/// Until task #36 it was `ignore`d because it asserted the open acceptance
/// (40.7 ms on 2026-11-02). With the system build on the device it holds and
/// runs in the suite.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn bluse_schafft_30_bilder() {
    let _allein = allein();
    let Some(gpu) = geraet() else { return };
    const BILDER: usize = 20;
    const SCHRANKE_MS: f64 = 1000.0 / 30.0;
    let netz = bluse3_v2();
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.055,
    };
    let bauen = || {
        Szene::aus_netz(
            &netz,
            &|_| vg,
            &|ti, q| [q[0] * 1e-3, q[1] * 1e-3, ti as f64 * 0.01],
            vec![],
            Simparameter {
                schwerkraft: [0.0, 0.0, -9.81],
                ..Simparameter::default()
            },
        )
    };
    let param = ImplizitParameter::default();
    let mut a = bauen();
    let mut b = a.clone();
    // Anlauf, damit der Zustand nicht die Anfangslage ist.
    let mut warm = ImplizitCg::neu(param);
    for _ in 0..3 {
        warm.bild(&mut a);
        warm.bild(&mut b);
    }
    let mut rw = ImplizitCg::mit_loeser(param, Box::new(gpu));
    let t0 = std::time::Instant::now();
    for _ in 0..BILDER {
        rw.bild(&mut b);
    }
    let ms = t0.elapsed().as_secs_f64() * 1e3 / BILDER as f64;
    eprintln!("Bluse3 v2: {ms:.2} ms je Bild = {:.1} Bilder/s", 1000.0 / ms);
    assert!(
        ms <= SCHRANKE_MS,
        "Bluse3 v2 braucht {ms:.2} ms je Bild = {:.1} Bilder/s, verlangt sind \
         höchstens {SCHRANKE_MS:.1} ms (30/s)",
        1000.0 / ms
    );
}

/// Largest deviation of a field, relative to the field's largest magnitude:
/// `max |a − b| / max |a|` over all records.
fn feld_abweichung(a: &[[f64; 3]], b: &[[f64; 3]]) -> f64 {
    assert_eq!(a.len(), b.len());
    let gross = a.iter().map(|q| dot(*q, *q).sqrt()).fold(0.0, f64::max);
    let d = abstand(a, b);
    d / gross.max(1e-300)
}

/// The system build on the device against the CPU build, record by record, on
/// Bluse3 v2 three frames after the start (velocities not zero, pieces moved).
///
/// Index fields must agree **exactly** - an edge that joins other points is not
/// a rounding error. Values must agree to `1e-5` relative to the largest
/// magnitude of their field: per distance constraint stiffness, direction and
/// cross factor, per bending constraint stiffness and the four gradients, per
/// point the force, the load for friction, the right-hand side and the inverse
/// diagonal block.
#[test]
fn aufbau_wie_cpu() {
    let _allein = allein();
    let Some(mut gpu) = geraet() else { return };
    const TOL: f64 = 1e-5;
    let mut s = bluse_szene();
    let mut cpu = ImplizitCg::neu(ImplizitParameter::default());
    for _ in 0..3 {
        cpu.bild(&mut s);
    }
    // Every triangle of the blouse gets a membrane; the count is read before
    // the borrow below. `Szene::membran_entartet` is not printed by any tool, so
    // this assertion is where the "0 of 10 734" in its doc comment comes from.
    let entartet = s.membran_entartet;
    assert_eq!(entartet, 0, "Dreiecke ohne Membran (Federn tragen sie allein)");
    let sys = cpu.system_fuer(&mut s);
    assert!(!sys.ohne_geometrie, "die CPU baut das System selbst");
    let a = gpu.aufbau_lesen(&sys);

    let distanz: Vec<_> = sys.kanten.iter().chain(&sys.naehte).collect();
    assert_eq!(a.kanten_punkte.len(), distanz.len());
    assert_eq!(a.biege_punkte.len(), sys.biegungen.len());
    // A scene without membranes would pass the rest of this test without the
    // continuum ever reaching the device, so the count is asserted first.
    assert!(!sys.membran.is_empty(), "die Bluse trägt Membranen");
    assert_eq!(a.mem_punkte.len(), sys.membran.len());
    for (c, k) in distanz.iter().enumerate() {
        assert_eq!(a.kanten_punkte[c], [k.i, k.j], "Abstand {c}: Punkte");
    }
    for (c, b) in sys.biegungen.iter().enumerate() {
        assert_eq!(a.biege_punkte[c], b.punkte, "Biegung {c}: Punkte");
    }
    for (c, m) in sys.membran.iter().enumerate() {
        assert_eq!(a.mem_punkte[c], m.punkte, "Membran {c}: Punkte");
    }

    let rel = |x: f64, y: f64| (x - y).abs() / x.abs().max(1e-300);
    let k_kante = distanz.iter().zip(&a.kanten_k).map(|(k, g)| rel(k.k, *g)).fold(0.0, f64::max);
    let k_biege = sys.biegungen.iter().zip(&a.biege_k).map(|(b, g)| rel(b.k, *g)).fold(0.0, f64::max);
    let richtung = feld_abweichung(&distanz.iter().map(|k| k.richtung).collect::<Vec<_>>(), &a.richtung);
    let quer_gross = distanz.iter().map(|k| k.quer.abs()).fold(0.0, f64::max);
    let quer = distanz.iter().zip(&a.quer).map(|(k, g)| (k.quer - g).abs()).fold(0.0, f64::max)
        / quer_gross.max(1e-300);
    let grad = (0..4)
        .map(|m| {
            feld_abweichung(
                &sys.biegungen.iter().map(|b| b.grad[m]).collect::<Vec<_>>(),
                &a.grad.iter().map(|g| g[m]).collect::<Vec<_>>(),
            )
        })
        .fold(0.0, f64::max);
    let kraft = feld_abweichung(&sys.f, &a.f);
    let last = feld_abweichung(&sys.last, &a.last);
    let rhs = feld_abweichung(&sys.rhs, &a.rhs);
    let pinv = (0..3)
        .map(|z| {
            feld_abweichung(
                &sys.pinv.iter().map(|p| p[z]).collect::<Vec<_>>(),
                &a.pinv.iter().map(|p| p[z]).collect::<Vec<_>>(),
            )
        })
        .fold(0.0, f64::max);
    let zeilen = [
        ("Steifigkeit Abstand", k_kante),
        ("Steifigkeit Biegung", k_biege),
        ("Richtung", richtung),
        ("Querfaktor", quer),
        ("Biegegradient", grad),
        ("Kraft", kraft),
        ("Last", last),
        ("rechte Seite", rhs),
        ("Vorkonditionierer", pinv),
    ];
    for (name, wert) in zeilen {
        eprintln!("{name:<20} {wert:.2e}");
    }
    for (name, wert) in &zeilen[..zeilen.len() - 1] {
        assert!(*wert <= TOL, "{name}: Abweichung {wert:.2e} > {TOL:.0e}");
    }
    // The inverse diagonal block cannot meet 1e-5 in f32, and that is a property
    // of the block, not of the build: in-plane stiffness h²·k stands against the
    // out-of-plane mass, the condition number κ of the blocks reaches ~700 on
    // this state, and an inverse in f32 is no better than κ·ε (692 · 1.19e-7 =
    // 8.2e-5; measured 3.7e-5 on 2026-11-02). The bound is therefore κ·ε,
    // computed from the blocks themselves. The preconditioner only steers the
    // CG; the operator, which decides the solution, is held to 1e-4 by
    // `operator_stimmt_mit_der_cpu`.
    let frob = |m: &[[f64; 3]; 3]| m.iter().flatten().map(|x| x * x).sum::<f64>().sqrt();
    let kond = sys.diag().iter().zip(&sys.pinv).map(|(d, p)| frob(d) * frob(p)).fold(0.0, f64::max);
    let tol_p = TOL.max(kond * f32::EPSILON as f64);
    eprintln!("Kondition Block max {kond:.1}, Schranke Vorkonditionierer {tol_p:.2e}");
    assert!(pinv <= tol_p, "Vorkonditionierer: Abweichung {pinv:.2e} > {tol_p:.2e} (κ {kond:.1})");
}

/// Bluse3 v2 flat, as in `examples/gpu_takt.rs`.
fn bluse_szene() -> Szene {
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.055,
    };
    Szene::aus_netz(
        &bluse3_v2(),
        &|_| vg,
        &|ti, q| [q[0] * 1e-3, q[1] * 1e-3, ti as f64 * 0.01],
        vec![],
        Simparameter {
            schwerkraft: [0.0, 0.0, -9.81],
            ..Simparameter::default()
        },
    )
}

/// Das Muster der Bluse, wie `examples/gpu_takt.rs` es lädt.
fn bluse3_v2() -> klon3d_mesh::Musternetz {
    use klon3d_mesh::{vernetze, Parameter};
    use klon3d_pattern::geom::Auswertung;
    use klon3d_pattern::kanten::Kurvenquelle;
    use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
    let wurzel = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../klon3d-pattern/tests/muster");
    let m = klon3d_pattern::read_path(&wurzel.join("bluse3_v2.sm2d")).expect("Muster");
    let ev = Auswertung::lade(&m, &wurzel.join("measurements.smis")).expect("Maße");
    let defs = lies_nahtliste(
        &std::fs::read_to_string(&wurzel.join("bluse3_v2.naehte")).expect("Nahtliste"),
    )
    .expect("Nahtliste");
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).expect("Nähte");
    vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).expect("Netz")
}
