//! Where the time goes in the GPU solve (task #29).
//!
//! One fixed system, both backends, one number each - the acceptance the task
//! asks for: at least 100 solves after a warm-up, same `cg_max`, same
//! tolerance. On top of that the GPU solve is split into upload and solve, and
//! the cost of a single operator pass is measured, so an optimisation has a
//! target instead of a guess. Since task #36 it also reports the system build:
//! its passes on the device and what the CPU still does per substep (the full
//! build for the CPU backend, only the topology for the GPU backend).
//!
//! Without an adapter it prints `übersprungen: kein GPU-Adapter` and measures
//! the CPU only.
//!
//! ```text
//! cargo run --release -p klon3d-sim --example gpu_takt
//! ```

use std::time::Instant;

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::implizit::{Fest, System};
use klon3d_sim::roehre::{param_ruhe, Roehre};
use klon3d_sim::{
    CgLauf, GpuLoeser, ImplizitCg, ImplizitParameter, Loeser, Rechenweg, Simparameter, Szene,
    Teilvorgabe, RADICAL_RUBBER_04,
};

/// Solves per backend after the warm-up.
const LOESUNGEN: usize = 120;
/// Whole frames for the end-to-end number.
const BILDER: usize = 20;
/// System builds for the CPU share per substep.
const AUFBAUTEN: usize = 50;

/// The same start vector every time: the solve has to do the same work.
fn start(n: usize) -> Vec<[f64; 3]> {
    (0..n)
        .map(|i| {
            let w = i as f64 * 0.6180339887498949;
            [
                w.fract() - 0.5,
                (w * 2.0).fract() - 0.5,
                (w * 3.0).fract() - 0.5,
            ]
        })
        .collect()
}

struct Ergebnis {
    ms: f64,
    iterationen: Vec<usize>,
}

/// Runs `LOESUNGEN` solves and returns the mean milliseconds and the
/// iteration counts. The start vector is reset before each solve so that every
/// run does the same work.
fn messen(l: &mut dyn Loeser, sys: &System<'_>, fest: &[Option<Fest>]) -> Ergebnis {
    let n = sys.masse.len();
    let v = start(n);
    let mut dv = v.clone();
    let mut lauf = |dv: &mut Vec<[f64; 3]>| -> CgLauf {
        dv.copy_from_slice(&v);
        l.loese(sys, fest, dv)
    };
    // Warm up: shader compilation, buffer growth, clock ramp.
    for _ in 0..5 {
        lauf(&mut dv);
    }
    let mut its = Vec::with_capacity(LOESUNGEN);
    let t0 = Instant::now();
    for _ in 0..LOESUNGEN {
        its.push(lauf(&mut dv).iterationen);
    }
    let ms = t0.elapsed().as_secs_f64() * 1e3 / LOESUNGEN as f64;
    Ergebnis {
        ms,
        iterationen: its,
    }
}

fn vermessen(name: &str, mut s: Szene, gpu: Option<GpuLoeser>) {
    let n = s.x.len();
    let param = ImplizitParameter::default();
    let mut cpu_rw = ImplizitCg::neu(param);
    // A few frames, so the scene is not in its starting layout any more.
    for _ in 0..3 {
        cpu_rw.bild(&mut s);
    }
    println!("\n{name}: {n} Punkte, cg_max {}", param.cg_max);

    let sys = cpu_rw.system_fuer(&mut s);
    println!(
        "  Constraints        {} Abstand, {} Naht, {} Biegung",
        sys.kanten.len(),
        sys.naehte.len(),
        sys.biegungen.len()
    );
    let fest = vec![None; n];

    let mut cpu = klon3d_sim::CpuLoeser;
    let e = messen(&mut cpu, &sys, &fest);
    let cpu_ms = e.ms;
    println!(
        "  {LOESUNGEN} Lösen     CPU  {cpu_ms:.3} ms je Lösen  ({} Iterationen, {:.3} ms je Iteration)",
        e.iterationen.iter().sum::<usize>() / LOESUNGEN,
        cpu_ms / (e.iterationen.iter().sum::<usize>() as f64 / LOESUNGEN as f64).max(1.0)
    );

    if let Some(mut g) = gpu {
        let e = messen(&mut g, &sys, &fest);
        let (hoch, loesen) = (g.zeiten()[0], g.zeiten()[1]);
        println!(
            "  {LOESUNGEN} Lösen     GPU  {:.3} ms je Lösen  ({} Iterationen, {:.3} ms je Iteration)",
            e.ms,
            e.iterationen.iter().sum::<usize>() / LOESUNGEN,
            e.ms / (e.iterationen.iter().sum::<usize>() as f64 / LOESUNGEN as f64).max(1.0)
        );
        println!("  letztes Lösen      hoch {hoch:.3} ms, lösen {loesen:.3} ms");
        println!("  Verhältnis         CPU/GPU = {:.2}", cpu_ms / e.ms);

        // Can a solve spread over the whole grid? That needs many passes with
        // a global barrier between them, so a pass has to be cheap.
        let p = start(n);
        for (art, name) in [(0u32, "leer"), (1, "A·p 1 WG"), (2, "A·p breit"), (3, "Aufbau")] {
            let d = g.takt_paesse(&sys, &p, 64, art);
            println!("  64 Pässe           {name:<9} {d:.4} ms je Pass");
        }
    }
    drop(sys);

    // What the CPU still does per substep to hand a system over: the full build
    // for the CPU backend, the topology only for a backend that builds on the
    // device (task #36).
    let aufbau = |rw: &mut ImplizitCg, s: &mut Szene| {
        for _ in 0..3 {
            rw.system_fuer(s);
        }
        let t0 = Instant::now();
        for _ in 0..AUFBAUTEN {
            rw.system_fuer(s);
        }
        t0.elapsed().as_secs_f64() * 1e3 / AUFBAUTEN as f64
    };
    let voll = aufbau(&mut cpu_rw, &mut s.clone());
    println!("  Systemaufbau CPU   voll      {voll:.3} ms je Unterschritt");
    if let Ok(g) = GpuLoeser::neu() {
        let mut rw = ImplizitCg::mit_loeser(param, Box::new(g));
        let topo = aufbau(&mut rw, &mut s.clone());
        println!("  Systemaufbau CPU   Topologie {topo:.3} ms je Unterschritt");
    }

    // Whole frames, from the same state, so the end-to-end effect is visible
    // and not only the solve in isolation.
    let mut a = s.clone();
    let mut b = s.clone();
    let mut rw = ImplizitCg::neu(param);
    let t0 = Instant::now();
    for _ in 0..BILDER {
        rw.bild(&mut a);
    }
    let cpu_bild = t0.elapsed().as_secs_f64() * 1e3 / BILDER as f64;
    let mut gpu_bild = f64::NAN;
    if let Ok(g) = GpuLoeser::neu() {
        let mut rw = ImplizitCg::mit_loeser(param, Box::new(g));
        let t0 = Instant::now();
        for _ in 0..BILDER {
            rw.bild(&mut b);
        }
        gpu_bild = t0.elapsed().as_secs_f64() * 1e3 / BILDER as f64;
    }
    println!("  {BILDER} Bilder     CPU  {cpu_bild:.2} ms je Bild");
    if gpu_bild.is_finite() {
        println!(
            "  {BILDER} Bilder     GPU  {gpu_bild:.2} ms je Bild  (CPU/GPU = {:.2})",
            cpu_bild / gpu_bild
        );
    }
}

fn main() {
    let r = Roehre::default();
    let roehre = r.szene(param_ruhe()).expect("Röhre");

    let wurzel =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster");
    let m = klon3d_pattern::read_path(&wurzel.join("bluse3_v2.sm2d")).expect("Muster");
    let ev = Auswertung::lade(&m, &wurzel.join("measurements.smis")).expect("Maße");
    let defs = lies_nahtliste(
        &std::fs::read_to_string(wurzel.join("bluse3_v2.naehte")).expect("Nahtliste"),
    )
    .expect("Nahtliste");
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).expect("Nähte");
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).expect("Netz");
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.055,
    };
    let bluse = Szene::aus_netz(
        &netz,
        &|_| vg,
        &|ti, q| [q[0] * 1e-3, q[1] * 1e-3, ti as f64 * 0.01],
        vec![],
        Simparameter {
            schwerkraft: [0.0, 0.0, -9.81],
            ..Simparameter::default()
        },
    );

    let gpu = match GpuLoeser::neu() {
        Ok(g) => {
            println!("GPU: {}", g.geraet());
            Some(g)
        }
        Err(e) => {
            println!("übersprungen: kein GPU-Adapter ({e}) – nur die CPU wird gemessen");
            None
        }
    };

    vermessen("Röhre 5,5 %", roehre, gpu);
    // A second adapter for the blouse: two scenes in one run, no state carried
    // over from the first.
    vermessen(
        "Bluse3 v2, flach, ohne Körper",
        bluse,
        GpuLoeser::neu().ok(),
    );
}
