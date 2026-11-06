//! `klon3d bewegen`: dress a garment, then raise both arms frame by frame.
//!
//! The motion itself is [`klon3d_pipeline::bewegen`]; this command only dresses
//! (the way `anziehen` does), writes every frame and prints the table.
//!
//! **The body.** A skeleton deforms *its* point list, and the garment has to be
//! checked against exactly that skin. So the body comes from
//! [`klon3d_pipeline::bewegen::koerper_mit_skelett`] - the MakeHuman mesh
//! together with its joints - and not from the shaping chain of a measurement
//! file, whose mesh has a different point list. The measurement file therefore
//! only gives the height here; her shaping targets are not applied on this path.
//! That is a limit of the motion path, not an oversight, and the report says so.
//!
//! `--bilder` is the **number of motion frames** here, unlike `anziehen`, where
//! the same option names an output folder.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use klon3d_mesh::Parameter;
use klon3d_pipeline::{self as pipeline, Fehler, Muster};
use klon3d_sim::platzierung::Anziehen;
use klon3d_sim::{
    abstand, ImplizitCg, ImplizitParameter, Material, RADICAL_RUBBER_04,
};

use crate::stoffwahl::{dicke_gedruckt, stoff_waehlen};

const AUFRUF: &str = "Aufruf: klon3d bewegen <muster.sm2d> [--masse <datei.smis>] [--bilder <n>] [--grad <grad>] \
                      [--out <ordner>] [--rechenweg implizit|gpu] [--material <profil|datei.zfab|stamm>] \
                      [--ordner <makehuman>] [--hoehe <cm>] [--kurz]";

/// Height the body is built at when neither `--hoehe` nor the measurement file
/// gives one (mm) - the same default as `klon3d skelett`.
const STANDARDHOEHE_MM: f64 = 1600.0;

fn fehler(m: impl std::fmt::Display) -> ExitCode {
    eprintln!("{m}");
    ExitCode::FAILURE
}

pub fn befehl_bewegen(args: &[String]) -> ExitCode {
    let mut muster_pfad: Option<PathBuf> = None;
    let mut masse: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut bilder: usize = 60;
    let mut grad: f64 = 90.0;
    let mut gpu = false;
    let mut material: Material = RADICAL_RUBBER_04;
    let mut mh_ordner: Option<PathBuf> = None;
    let mut hoehe_cm: Option<f64> = None;
    // The short dress schedule of `tests/kette.rs`: for looking at the motion
    // without waiting for a full 200 + 600 frame dress.
    let mut kurz = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--kurz" {
            kurz = true;
            i += 1;
            continue;
        }
        if !a.starts_with("--") {
            if muster_pfad.is_some() {
                return fehler(format!("unerwartetes Argument {a:?}\n{AUFRUF}"));
            }
            muster_pfad = Some(PathBuf::from(a));
            i += 1;
            continue;
        }
        let Some(w) = args.get(i + 1) else { return fehler(format!("{a} ohne Wert\n{AUFRUF}")) };
        match a {
            "--masse" => masse = Some(PathBuf::from(w)),
            "--out" => out = Some(PathBuf::from(w)),
            "--bilder" => match w.parse::<usize>() {
                // Deliberately spelled out: the same option means a folder in
                // `anziehen`, and a silent folder name would be read as "0".
                Ok(n) => bilder = n,
                Err(_) => {
                    return fehler(format!(
                        "--bilder {w:?}: keine Zahl (hier die Anzahl der Bewegungsbilder, kein Ordner)"
                    ))
                }
            },
            "--grad" => match w.replace(',', ".").parse::<f64>() {
                Ok(v) => grad = v,
                Err(_) => return fehler(format!("--grad {w:?}: keine Zahl")),
            },
            "--rechenweg" => match w.as_str() {
                "implizit" => gpu = false,
                "gpu" => gpu = true,
                _ => return fehler(format!("--rechenweg {w:?}: implizit oder gpu")),
            },
            "--material" => match stoff_waehlen(w) {
                Ok(m) => material = m,
                Err(e) => return fehler(e),
            },
            "--ordner" => mh_ordner = Some(PathBuf::from(w)),
            "--hoehe" => match w.replace(',', ".").parse::<f64>() {
                Ok(v) => hoehe_cm = Some(v),
                Err(_) => return fehler(format!("--hoehe {w:?}: keine Zahl")),
            },
            _ => return fehler(format!("unbekannte Option {a:?}\n{AUFRUF}")),
        }
        i += 2;
    }
    let Some(muster_pfad) = muster_pfad else { return fehler(AUFRUF) };

    // Pattern, seam list, mesh, placement file: the same chain as `anziehen`.
    let muster = match Muster::laden(&muster_pfad, None) {
        Ok(m) => m,
        Err(e) => return fehler(e),
    };
    let naehte_pfad = muster_pfad.with_extension("naehte");
    let defs = match pipeline::nahtliste(&naehte_pfad) {
        Ok(d) => d,
        Err(e) => return fehler(e),
    };
    let netz = match muster.netz(&defs, &Parameter::default()) {
        Ok(n) => n,
        Err(Fehler::Naehte(e)) => return fehler(format!("{}: {e}", naehte_pfad.display())),
        Err(e) => return fehler(e),
    };
    let lage_pfad = muster_pfad.with_extension("lage");
    let liste = match pipeline::lageliste(&lage_pfad) {
        Ok(l) => l,
        Err(e) => return fehler(e),
    };

    // Height: the option, else the measurement file, else the default.
    let masse = masse.unwrap_or_else(|| muster.massdatei().to_path_buf());
    let mut meldungen = Vec::new();
    let hoehe_mm = match hoehe_cm {
        Some(h) => h * 10.0,
        None => {
            match klon3d_pattern::lies_smis(&masse).map(|s| s.wert("height")) {
                Ok(Some(h)) if h > 0.0 => h * 10.0,
                Ok(_) => {
                    meldungen.push(format!("{}: kein Maß height", masse.display()));
                    STANDARDHOEHE_MM
                }
                Err(e) => {
                    meldungen.push(format!("{}: {e}", masse.display()));
                    STANDARDHOEHE_MM
                }
            }
        }
    };
    let Some(ordner) = mh_ordner.or_else(klon3d_koerper::koerper_quelle::ordner) else {
        return fehler(
            "MakeHuman-Daten fehlen (assets/3dobjs/makehuman); sie kommen mit dem Programm \
             - oder --ordner <makehuman> nennen",
        );
    };
    let (geladen, mut sk) = match pipeline::bewegen::koerper_mit_skelett(&ordner, hoehe_mm) {
        Ok(x) => x,
        Err(e) => return fehler(e),
    };
    let herkunft = &geladen.herkunft;
    let huellen = match pipeline::huellen(&geladen) {
        Ok(h) => h,
        Err(e) => return fehler(e),
    };
    let p = match pipeline::platzieren(&netz, &liste, &huellen, true) {
        Ok(p) => p,
        Err(e) => return fehler(format!("{}: {e}", lage_pfad.display())),
    };

    println!("{}", muster_pfad.display());
    println!("  Körper       {herkunft}");
    println!("  Nahtliste    {} ({} Nähte)", naehte_pfad.display(), netz.naehte.len());
    println!(
        "  Lage         {} ({} Teile platziert, Abstand {:.1} cm)",
        lage_pfad.display(),
        p.netz.teile.len(),
        liste.abstand * 100.0
    );
    for m in &meldungen {
        println!("  Hinweis      {m}");
    }
    let dicken: Vec<String> = (0..p.netz.teile.len())
        .filter_map(|ti| {
            p.dicke(ti).map(|d| {
                format!(
                    "{} {} mm",
                    p.netz.teile[ti].teil.rsplit(" - ").next().unwrap_or(""),
                    dicke_gedruckt(d)
                )
            })
        })
        .collect();
    println!(
        "  Material     {} {} mm{}",
        material.name,
        dicke_gedruckt(material.dicke_m),
        if dicken.is_empty() { String::new() } else { format!("; {}", dicken.join(", ")) }
    );
    println!(
        "  Bewegung     beide Arme 0° → {grad:.0}° in {bilder} Bildern{}",
        if bilder == 0 { " (nur Bild 0)" } else { "" }
    );
    if !kurz {
        println!(
            "  Hinweis      der Körper ist MakeHuman mit Gelenken, nicht ihre Messform; die Maßdatei gibt hier nur die Höhe"
        );
    }

    let mut a = Anziehen::default();
    if kurz {
        a = Anziehen { naehbilder: 2, einschwingen_max: 2, haltebilder: 1, ..Anziehen::default() };
        println!("  Anziehen     kurz: {} Bilder nähen, höchstens {} einschwingen, {} halten", a.naehbilder, a.einschwingen_max, a.haltebilder);
    }
    let param = pipeline::standard_simparameter();
    let mut s = pipeline::szene_aus(&p, &geladen, material, 0.0, param);
    // Bonding zones from the placement file (`klebt <Teil>`), exactly as
    // `anziehen` sets them: the bond arises where the pieces really lie, and
    // without `klebt` no spring appears.
    let zonen = p.klebungen(&s).into_iter().map(|(z, _)| z).collect();
    s.klebungen_setzen(zonen);

    let mut rw = if gpu {
        let loeser = match klon3d_sim::GpuLoeser::neu() {
            Ok(l) => l,
            Err(e) => return fehler(format!("GPU: {e} (ohne --rechenweg gpu rechnet die CPU)")),
        };
        println!("  Rechenweg    GPU (f32, wgpu) auf {}", loeser.geraet());
        ImplizitCg::mit_loeser(ImplizitParameter::default(), Box::new(loeser))
    } else {
        println!("  Rechenweg    implizit (CPU, f64)");
        ImplizitCg::neu(ImplizitParameter::default())
    };

    let start = Instant::now();
    let erg = pipeline::ablauf::anziehen(&mut s, &mut rw, &a, &mut |_, _, _| {});
    let dress_s = start.elapsed().as_secs_f64();
    let karte_anfang = abstand::Karte::messe(&s);
    println!(
        "  angezogen    {} Bilder nähen, {} Bilder einschwingen ({}), {} Bilder Halt, tiefster Punkt {:+.2} mm, {:.1} s",
        a.naehbilder,
        erg.einschwingbilder,
        if erg.eingeschwungen { "ruhig" } else { "Höchstzahl erreicht, sinkt noch" },
        a.haltebilder,
        karte_anfang.as_ref().and_then(|k| k.tiefster()).map_or(f64::INFINITY, |(d, _)| d) * 1e3,
        dress_s
    );

    // Every frame is written: `bild_NNNN.obj`, `dehnung_NNNN.csv`,
    // `abstand_NNNN.csv`. The clock runs from callback to callback, so the
    // times below contain measuring and writing - that is the honest cost of
    // this command, not the pure solve.
    let mut schreiber = out.as_ref().map(|o| pipeline::bewegen::Schreiber::neu(o));
    let mut letzt = Instant::now();
    let mut zeilen: Vec<(usize, f64, f64, f64, usize)> = Vec::new();
    let mut schreibfehler: Option<String> = None;
    let mut cb = |b: &pipeline::bewegen::Bildbericht, s: &klon3d_sim::Szene| {
        let ms = letzt.elapsed().as_secs_f64() * 1e3;
        letzt = Instant::now();
        zeilen.push((b.bild, b.grad, ms, b.tiefster, b.offene_nahtpaare));
        if let Some(w) = schreiber.as_mut() {
            if let Err(e) = w.bild(b, s) {
                schreibfehler.get_or_insert(e.to_string());
            }
        }
    };
    let lauf = pipeline::bewegen::bewegen(&mut s, &mut rw, &mut sk, &netz, bilder, grad, &mut cb);
    let bewegen_s = start.elapsed().as_secs_f64() - dress_s;
    if let Some(e) = schreibfehler {
        return fehler(e);
    }
    if let Err(e) = lauf {
        return fehler(e);
    }
    // The glTF is complete only now: one morph target per frame, so the frames
    // cannot be written as they come.
    if let Some(w) = schreiber.as_mut() {
        if let Err(e) = w.abschluss() {
            return fehler(e.to_string());
        }
    }

    println!("  Bild  Grad      Zeit      tiefster  offene Nahtpaare");
    for (bild, g, ms, tief, offen) in &zeilen {
        println!("  {bild:4}  {g:5.1}°  {ms:7.1} ms  {:+8.2} mm  {offen:5}", tief * 1e3);
    }
    if let Some(o) = &out {
        let mut t = String::from("bild;grad;zeit_ms;tiefster_mm;offene_nahtpaare\n");
        for (bild, g, ms, tief, offen) in &zeilen {
            t += &format!("{bild};{g:.4};{ms:.3};{:.4};{offen}\n", tief * 1e3);
        }
        if let Err(e) = std::fs::write(o.join("lauf.csv"), t) {
            return fehler(format!("{}: {e}", o.display()));
        }
        let je_bild = zeilen.iter().filter(|z| z.0 > 0).count();
        println!(
            "  geschrieben  {} (bild_NNNN.obj, dehnung_NNNN.csv, abstand_NNNN.csv für {} Bilder, bewegung.glb, lauf.csv)",
            o.display(),
            zeilen.len()
        );
        if let Some(w) = &schreiber {
            println!(
                "  glTF         {} ({} Bilder als Morph-Targets, STEP, ein Mesh je Teil und eines für den Körper)",
                w.bewegung_pfad().display(),
                w.bilder()
            );
        }
        if je_bild > 0 {
            let schnitt = zeilen.iter().filter(|z| z.0 > 0).map(|z| z.2).sum::<f64>() / je_bild as f64;
            println!("  Zeit Bewegung {je_bild} Bilder, {schnitt:.1} ms im Mittel (einschließlich Messen und Schreiben)");
        }
    }
    if zeilen.iter().any(|z| z.0 > 0) {
        println!("  Zeit gesamt  {:.1} s (davon {:.1} s Anziehen, {:.1} s Bewegung)", start.elapsed().as_secs_f64(), dress_s, bewegen_s);
    }
    ExitCode::SUCCESS
}


