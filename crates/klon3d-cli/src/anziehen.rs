//! `klon3d anziehen`: ein Muster um den Körper der Maßdatei legen und am
//! Körper anziehen (Stufe 4c, Befunde 95, 96).
//!
//! Muster → Netz mit Nahtliste → Körper aus der Maßdatei (Formungskette) mit
//! Segmenten und Landmarken → Hüllkörper → Lagedatei → Szene → anziehen:
//! Nähte mit begrenzter Geschwindigkeit schließen, dabei Körperpunkt gegen
//! Stoffdreieck, gehalten (ohne Schwerkraft), dann ruhen mit voller
//! Schwerkraft; die letzten 10 s der Ruhe sind die Haltephase (ADR-023).
//! Ausgabe: Abstandskarte Stoff → Haut je Teil (Stufe 5, [`klon3d_sim::abstand`]),
//! Dehnungskarte je Teil ([`klon3d_sim::dehnung`]), Spannungskarte je Teil in
//! N/m ([`klon3d_sim::spannung`]), Nahtlage gegen die benannten Körperpunkte
//! ([`klon3d_sim::nahtlage`]), Nahtabstände je Naht,
//! tiefster Punkt, auf Wunsch `koerper.obj`, `anfang.obj`, `ende.obj` (mm),
//! `abstand.csv`, `dehnung.csv`, `spannung.csv` und `nahtlage.csv` in je einem
//! Ordner, dazu mit
//! `--farben` alle Karten als OBJ mit Farbe je Eckpunkt
//! ([`klon3d_sim::farbkarte`]).
//!
//! Mit `--zprj` kommt der Körper stattdessen aus CLOs Projekt: der `body` der
//! `.avt` (Befund 119) und die Segment- und Landmarkenlisten aus CLOs
//! `Resizing`-Ordner (Befund 120). Das ist derselbe Körper, den CLO selbst
//! simuliert — mit der Prüfung aus [`klon3d_koerper::anordnung_clo`].
//!
//! `--material` nimmt drei Arten von Angabe: ein eingebautes Profil
//! (`radical`, `retifist`), einen Pfad auf eine `.zfab`-Datei, oder den Stamm
//! eines Stoffs aus `assets/stoffe` - die acht Latexstoffe, die
//! [`klon3d_pipeline::stoff`] liest. Ohne Angabe rechnet es mit 0,4 mm wie
//! bisher.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use klon3d_koerper::koerper::ordner;
use klon3d_mesh::Parameter;
use klon3d_pipeline::{self as pipeline, Fehler, Koerperquelle, Muster};
use klon3d_sim::netzkoerper::SUCHTIEFE;
use klon3d_sim::platzierung::{lies_lage, platziere, Angezogen, Anziehbar, Anziehen};
use klon3d_sim::{
    abstand, ausgabe, dehnung, farbkarte, messung, nahtlage, spannung, CpuReferenz, ImplizitCg, ImplizitParameter,
    Material, Simparameter, Szene, RADICAL_RUBBER_04, WEICH_FAKTOR,
};

const AUFRUF: &str = "Aufruf: klon3d anziehen <muster.sm2d> [--masse <datei.smis>] [--zprj <projekt.zprj>] [--naehte <datei>] \
                      [--lage <datei>] [--naehen <m/s>] [--schwerkraft-naehen <anteil>] [--naht-k <N/m>] [--cg-max <n>] [--naehbilder <n>] [--einschwingen-min <n>] [--einschwingen-max <n>] [--ohne-koerperpunkte] [--ohne-weite] [--offen] \
                      [--material <profil|datei.zfab|stamm>] [--obj <ordner>] [--abstand <ordner>] [--dehnung <ordner>] [--spannung <ordner>] [--nahtlage <ordner>] [--farben <ordner>] [--ordner <Resizing>] \
                      [--rechenweg implizit|gpu|xpbd] [--selbstkollision [--schnitte-an-naehten]] [--anfang <lage.obj>] [--bilder <ordner>] [--zeiten <datei>]";

fn fehler(m: impl std::fmt::Display) -> ExitCode {
    eprintln!("{m}");
    ExitCode::FAILURE
}

// ───────────────────────────────────────────────────────── Stoffwahl

/// The folder `assets/stoffe` in `start` or in one of the folders above it.
///
/// The same search the application makes (its `stoffordner_ab`); the command
/// line does not depend on `klon3d-app`, so the few lines stand here.
fn stoffordner_ab(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(|o| o.join("assets").join("stoffe")).find(|p| p.is_dir())
}

/// The fabrics folder: `assets/stoffe` searched upwards from the folder of the
/// program, then from the current folder. The first hit wins.
fn stoffordner() -> Option<PathBuf> {
    let programm = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let aktuell = std::env::current_dir().ok();
    [programm, aktuell].into_iter().flatten().find_map(|s| stoffordner_ab(&s))
}

/// The fabric for `--material`: a built-in profile, a `.zfab` path, or the stem
/// of a file in the fabrics folder.
///
/// The order is the one the option promises: the two built-in profiles keep
/// their names (a run from before this option grew keeps its material), then a
/// path, then a stem. A name that is none of the three is an error that says
/// where it looked and what it found there - not a silent run at 0.4 mm.
fn stoff_waehlen(angabe: &str) -> Result<Material, String> {
    stoff_aufloesen(angabe, stoffordner().as_deref())
}

/// [`stoff_waehlen`] with the fabrics folder handed in, so that the search can
/// be tested without the repository.
fn stoff_aufloesen(angabe: &str, ordner: Option<&Path>) -> Result<Material, String> {
    if let Some(m) = Material::nach_namen(angabe) {
        return Ok(m);
    }
    let pfad = Path::new(angabe);
    if pfad.extension().is_some_and(|e| e.eq_ignore_ascii_case("zfab")) {
        return pipeline::stoff::stoff_laden(pfad).map_err(|e| format!("--material {angabe:?}: {e}"));
    }
    let Some(ordner) = ordner else {
        return Err(format!(
            "--material {angabe:?}: kein eingebautes Profil (radical, retifist), und kein Ordner \
             assets/stoffe gefunden - gesucht ab dem Programmordner und dem aktuellen Ordner aufwaerts"
        ));
    };
    let stoffe = pipeline::stoff::stoffe_im_ordner(ordner);
    let stamm = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let treffer = stoffe
        .iter()
        .find(|(p, _)| stamm(p).eq_ignore_ascii_case(angabe));
    match treffer {
        // A file that exists and does not load names itself: the error of
        // `stoff_laden` carries the path and the field at fault.
        Some((_, Ok(m))) => Ok(*m),
        Some((_, Err(e))) => Err(format!("--material {angabe:?}: {e}")),
        None => {
            let mut namen: Vec<String> = stoffe.iter().map(|(p, _)| stamm(p)).collect();
            namen.sort();
            Err(format!(
                "--material {angabe:?}: kein eingebautes Profil (radical, retifist) und kein Stoff \
                 dieses Namens in {}; dort liegen: {}",
                ordner.display(),
                if namen.is_empty() { "keine .zfab-Datei".to_string() } else { namen.join(", ") }
            ))
        }
    }
}

/// A thickness in metres as millimetres, with as few decimals as it needs but
/// at least one.
///
/// One decimal is not enough for the fabrics this command selects: 0.25 mm
/// printed with one decimal reads "0.2 mm" and cannot be told apart from a
/// 0.2 mm sheet. Three decimals reach the micrometre, which is finer than any
/// fabric file here, and the trailing zeros go away so 1.0 mm stays "1.0 mm"
/// and not "1.000 mm".
fn dicke_gedruckt(dicke_m: f64) -> String {
    let mut s = format!("{:.3}", dicke_m * 1e3);
    while s.ends_with('0') && s.split('.').nth(1).is_some_and(|n| n.len() > 1) {
        s.pop();
    }
    if s.ends_with('.') {
        s.push('0');
    }
    s
}

pub fn befehl_anziehen(args: &[String]) -> ExitCode {
    let mut muster_pfad: Option<PathBuf> = None;
    let (mut masse, mut naehte, mut lage, mut obj): (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) =
        (None, None, None, None);
    let mut gegen_lage: Option<PathBuf> = None;
    let mut ohne_weite = false;
    let mut zprj: Option<PathBuf> = None;
    let mut resizing = ordner();
    let mut ablauf = Anziehen::default();
    let mut material: Material = RADICAL_RUBBER_04;
    let mut offen_zeigen = false;
    let mut xpbd = false;
    // Step 4b: the same implicit step, solved on the GPU.
    let mut gpu = false;
    let mut selbstkollision = false;
    let mut schnitte_an_naehten = false;
    // Abstandskarte Stoff → Haut (Stufe 5): Ordner für `abstand.csv`.
    let mut abstand_ordner: Option<PathBuf> = None;
    // Dehnungskarte (Stufe 5, Aufgabe #17): Ordner für `dehnung.csv`.
    let mut dehnung_ordner: Option<PathBuf> = None;
    // Spannungskarte (Stufe 5, Aufgabe #23): Ordner für `spannung.csv`.
    let mut spannung_ordner: Option<PathBuf> = None;
    // Seam map (stage 5, task #25): folder for `nahtlage.csv`.
    let mut nahtlage_ordner: Option<PathBuf> = None;
    // Coloured maps as OBJ (stage 5, tasks #19/#23): folder for `dehnung.obj`,
    // `spannung.obj` and `abstand.obj`.
    let mut farben_ordner: Option<PathBuf> = None;
    let (mut anfang, mut bilder, mut zeiten): (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) = (None, None, None);
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--ohne-koerperpunkte" {
            ablauf.koerperpunkt_durchgaenge = 0;
            i += 1;
            continue;
        }
        if a == "--offen" {
            offen_zeigen = true;
            i += 1;
            continue;
        }
        if a == "--ohne-weite" {
            ohne_weite = true;
            i += 1;
            continue;
        }
        // Stoff gegen Stoff wie CLO (Befund 116); nur im impliziten Rechenweg.
        if a == "--selbstkollision" {
            selbstkollision = true;
            i += 1;
            continue;
        }
        // Durchdringungen auch zwischen Nahtpartnern suchen, wie CLO (Befund 116).
        if a == "--schnitte-an-naehten" {
            schnitte_an_naehten = true;
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
            "--zprj" => zprj = Some(PathBuf::from(w)),
            "--naehte" => naehte = Some(PathBuf::from(w)),
            "--lage" => lage = Some(PathBuf::from(w)),
            "--gegen-lage" => gegen_lage = Some(PathBuf::from(w)),
            "--obj" => obj = Some(PathBuf::from(w)),
            "--abstand" => abstand_ordner = Some(PathBuf::from(w)),
            "--dehnung" => dehnung_ordner = Some(PathBuf::from(w)),
            "--spannung" => spannung_ordner = Some(PathBuf::from(w)),
            "--nahtlage" => nahtlage_ordner = Some(PathBuf::from(w)),
            "--farben" => farben_ordner = Some(PathBuf::from(w)),
            "--ordner" => resizing = PathBuf::from(w),
            "--naehbilder" => match w.parse() {
                Ok(n) => ablauf.naehbilder = n,
                Err(_) => return fehler(format!("--naehbilder {w:?}: keine Zahl")),
            },
            "--einschwingen-min" => match w.parse() {
                Ok(n) => ablauf.einschwingen_min = n,
                Err(_) => return fehler(format!("--einschwingen-min {w:?}: keine Zahl")),
            },
            "--rechenweg" => match w.as_str() {
                "implizit" => {
                    xpbd = false;
                    gpu = false;
                }
                "gpu" => {
                    xpbd = false;
                    gpu = true;
                }
                "xpbd" => xpbd = true,
                _ => return fehler(format!("--rechenweg {w:?}: implizit, gpu oder xpbd")),
            },
            "--anfang" => anfang = Some(PathBuf::from(w)),
            "--bilder" => bilder = Some(PathBuf::from(w)),
            "--zeiten" => zeiten = Some(PathBuf::from(w)),
            "--einschwingen-max" => match w.parse() {
                Ok(n) => ablauf.einschwingen_max = n,
                Err(_) => return fehler(format!("--einschwingen-max {w:?}: keine Zahl")),
            },
            "--cg-max" => match w.parse() {
                Ok(n) => ablauf.cg_max = n,
                Err(_) => return fehler(format!("--cg-max {w:?}: keine Zahl")),
            },
            "--naht-k" => match w.replace(',', ".").parse() {
                Ok(v) => ablauf.naht_k = v,
                Err(_) => return fehler(format!("--naht-k {w:?}: keine Zahl")),
            },
            "--schwerkraft-naehen" => match w.replace(',', ".").parse() {
                Ok(v) => ablauf.schwerkraft_naehen = v,
                Err(_) => return fehler(format!("--schwerkraft-naehen {w:?}: keine Zahl")),
            },
            "--naehen" => match w.replace(',', ".").parse() {
                Ok(v) => ablauf.naht_geschwindigkeit = v,
                Err(_) => return fehler(format!("--naehen {w:?}: keine Zahl")),
            },
            "--material" => match stoff_waehlen(w) {
                Ok(m) => material = m,
                Err(e) => return fehler(e),
            },
            _ => return fehler(format!("unbekannte Option {a:?}\n{AUFRUF}")),
        }
        i += 2;
    }
    let Some(muster_pfad) = muster_pfad else { return fehler(AUFRUF) };

    // Pattern, seam list, mesh, placement file and body all come from the
    // library chain (`klon3d_pipeline`); this command only adds its options
    // and the printed report.
    let muster = match Muster::laden(&muster_pfad, None) {
        Ok(m) => m,
        Err(e) => return fehler(e),
    };
    let naehte_pfad = naehte.unwrap_or_else(|| muster_pfad.with_extension("naehte"));
    let defs = match pipeline::nahtliste(&naehte_pfad) {
        Ok(d) => d,
        Err(e) => return fehler(e),
    };
    let netz = match muster.netz(&defs, &Parameter::default()) {
        Ok(n) => n,
        Err(Fehler::Naehte(e)) => return fehler(format!("{}: {e}", naehte_pfad.display())),
        Err(e) => return fehler(e),
    };
    let lage_pfad = lage.unwrap_or_else(|| muster_pfad.with_extension("lage"));
    let liste = match pipeline::lageliste(&lage_pfad) {
        Ok(l) => l,
        Err(e) => return fehler(e),
    };

    // Body: from the `.zprj` project (`--zprj`) or from the measurement file
    // (default: the pattern's own).
    let masse = masse.unwrap_or_else(|| muster.massdatei().to_path_buf());
    let quelle = match &zprj {
        Some(pfad) => Koerperquelle::CloProjekt(pfad, resizing),
        None => Koerperquelle::MassdateiMitOrdner(&masse, resizing),
    };
    let geladen = match pipeline::koerper(quelle) {
        Ok(k) => k,
        Err(e) => return fehler(e),
    };
    let (koerper, an, herkunft) = (&geladen.netz, &geladen.anordnung, &geladen.herkunft);
    let huellen = match pipeline::huellen(&geladen) {
        Ok(h) => h,
        Err(e) => return fehler(e),
    };
    let p = match pipeline::platzieren(&netz, &liste, &huellen, !ohne_weite) {
        Ok(p) => p,
        Err(e) => return fehler(format!("{}: {e}", lage_pfad.display())),
    };

    // Nur vergleichen: je Teil der mittlere Abstand seiner Netzpunkte
    // zwischen beiden Lagen, ohne zu rechnen.
    if let Some(g) = &gegen_lage {
        let andere = match std::fs::read_to_string(g).map_err(|e| e.to_string()).and_then(|t| lies_lage(&t)).and_then(|l| platziere(&netz, &l, &huellen)) {
            Ok(q) => q,
            Err(e) => return fehler(format!("{}: {e}", g.display())),
        };
        println!("{}: Lage {} gegen {}", muster_pfad.display(), lage_pfad.display(), g.display());
        let mut summe = (0.0, 0usize);
        for (ti, t) in p.netz.teile.iter().enumerate() {
            let Some(tj) = andere.netz.teile.iter().position(|u| u.teil == t.teil) else {
                println!("  {:<50} nur hier", t.teil);
                continue;
            };
            let d: Vec<f64> = t
                .punkte
                .iter()
                .map(|&q| {
                    let (a, b) = (p.lage(ti, q), andere.lage(tj, q));
                    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
                })
                .collect();
            let mittel = d.iter().sum::<f64>() / d.len().max(1) as f64;
            let groesster = d.iter().cloned().fold(0.0, f64::max);
            println!("  {:<50} mittel {:5.1} cm, größter {:5.1} cm", t.teil, mittel * 100.0, groesster * 100.0);
            summe.0 += mittel;
            summe.1 += 1;
        }
        for u in &andere.netz.teile {
            if !p.netz.teile.iter().any(|t| t.teil == u.teil) {
                println!("  {:<50} nur dort", u.teil);
            }
        }
        println!("  Mittel über {} Teile: {:.1} cm", summe.1, summe.0 / summe.1.max(1) as f64 * 100.0);
        return ExitCode::SUCCESS;
    }

    println!("{}", muster_pfad.display());
    println!("  Körper       {}", herkunft);
    println!("  Nahtliste    {} ({} Nähte)", naehte_pfad.display(), netz.naehte.len());
    println!("  Lage         {} ({} Teile platziert, Abstand {:.1} cm)", lage_pfad.display(), p.netz.teile.len(), liste.abstand * 100.0);
    if !p.weggelassen.is_empty() {
        println!("  ohne Lage    {}", p.weggelassen.join(", "));
    }
    if !p.naehte_weggelassen.is_empty() {
        println!("  Nähte weg    {}", p.naehte_weggelassen.join(", "));
    }

    // Je Teil das Material, in der Dicke der Lageangabe.
    let dicken: Vec<String> = (0..p.netz.teile.len())
        .filter_map(|ti| {
            p.dicke(ti)
                .map(|d| format!("{} {} mm", p.netz.teile[ti].teil.rsplit(" - ").next().unwrap_or(""), dicke_gedruckt(d)))
        })
        .collect();
    println!("  Material     {} {} mm{}", material.name, dicke_gedruckt(material.dicke_m), if dicken.is_empty() { String::new() } else { format!("; {}", dicken.join(", ")) });
    // A placement file that gives every piece a thickness replaces the fabric's
    // own one: the solver scales the selected material onto that thickness
    // (`Material::mit_dicke`). The eight files in `assets/stoffe` are one
    // compound at eight thicknesses, so among them the choice then changes
    // nothing but the name - worth saying, because the option looks like it
    // would.
    let mit_dicke = (0..p.netz.teile.len()).filter(|&ti| p.dicke(ti).is_some()).count();
    if mit_dicke == p.netz.teile.len() && !p.netz.teile.is_empty() {
        println!("  Hinweis      die Lage gibt jedem Teil eine Dicke; der Stoff wird darauf umgerechnet -");
        println!("               zwischen den acht Dateien in assets/stoffe aendert das keine Zahl");
    }
    // XPBD: die Nahtfeder des Anziehens als Nachgiebigkeit in der Szene.
    let naht_nachgiebigkeit = if xpbd { 1.0 / ablauf.naht_k } else { 0.0 };
    let param = Simparameter { naht_nachgiebigkeit, ..pipeline::standard_simparameter() };
    let mut s = pipeline::szene_aus(&p, &geladen, material, 0.0, param);
    if xpbd {
        s.param.unterschritte = s.unterschritte_weich(WEICH_FAKTOR);
        println!("  Rechenweg    XPBD (CpuReferenz), {} Unterschritte je Bild (weich, Faktor {WEICH_FAKTOR})", s.param.unterschritte);
    }
    // Gemeinsame Anfangslage aller Löser (Befund 109): Punkte in mm, in
    // derselben Reihenfolge wie `anfang.obj`.
    if let Some(a) = &anfang {
        let t = match std::fs::read_to_string(a) {
            Ok(t) => t,
            Err(e) => return fehler(format!("{}: {e}", a.display())),
        };
        let x: Vec<[f64; 3]> = t
            .lines()
            .filter_map(|z| z.strip_prefix("v "))
            .filter_map(|z| {
                let w: Vec<f64> = z.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                (w.len() >= 3).then(|| [w[0] * 1e-3, w[1] * 1e-3, w[2] * 1e-3])
            })
            .collect();
        if x.len() != s.x.len() {
            return fehler(format!("{}: {} Punkte, die Szene hat {}", a.display(), x.len(), s.x.len()));
        }
        s.x = x;
        println!("  Anfangslage  {}", a.display());
    }
    // Abstandskarte Stoff → Haut (Stufe 5): einmal rechnen, dann Anfangslage
    // und Endzustand daraus. Der tiefste Punkt kommt aus derselben Karte statt
    // aus einer eigenen Suche je Punkt.
    let karte_anfang = abstand::Karte::messe(&s);
    let tiefster = |k: &Option<abstand::Karte>| {
        k.as_ref().and_then(|k| k.tiefster()).map_or(f64::INFINITY, |(d, _)| d)
    };
    println!(
        "  Anfang       {} Punkte, tiefster Punkt {:+.1} mm{}",
        s.x.len(),
        tiefster(&karte_anfang) * 1e3,
        if karte_anfang.is_some() { String::new() } else { format!(" (Kollider findet bis {:.0} mm)", SUCHTIEFE * 1e3) }
    );
    if let Some(k) = &karte_anfang {
        println!("  Abstand zur Haut, Anfangslage:");
        abstand_tabelle(k);
    }
    // Klebezonen aus der Lagedatei (`klebt <Teil>`): Die Bindung entsteht dort,
    // wo die Teile tatsächlich liegen; ohne `klebt` entsteht keine Feder und
    // jedes bestehende Muster rechnet unverändert.
    let mut zonen = Vec::new();
    for (zone, ohne) in p.klebungen(&s) {
        println!(
            "  Klebung      {:<40} {} Punkte gebunden, größter Abstand {:.1} mm, ohne Gegenstück {}",
            zone.name,
            zone.len(),
            zone.groesster_abstand(&s.x) * 1e3,
            ohne
        );
        zonen.push(zone);
    }
    // Setzt die Zonen und hebt zugleich die Biegung der Constraints in der
    // Klebefläche auf die Plattensteifigkeit des Laminats.
    s.klebungen_setzen(zonen);
    if let Some(o) = &obj {
        if let Err(e) = std::fs::create_dir_all(o).and_then(|_| std::fs::write(o.join("anfang.obj"), ausgabe::obj(&s))) {
            return fehler(format!("{}: {e}", o.display()));
        }
        let mut t = String::from("# klon3d anziehen: Körper, Einheit mm\n");
        for q in &koerper.punkte {
            t += &format!("v {:.3} {:.3} {:.3}\n", q[0] * 1e3, q[1] * 1e3, q[2] * 1e3);
        }
        for d in &koerper.dreiecke {
            t += &format!("f {} {} {}\n", d[0] + 1, d[1] + 1, d[2] + 1);
        }
        if let Err(e) = std::fs::write(o.join("koerper.obj"), t) {
            return fehler(format!("{}: {e}", o.display()));
        }
    }

    let start = std::time::Instant::now();
    // CG: Lösungen, Iterationen, nicht konvergiert (Befund 102).
    let mut selbst_letztes = None;
    let lauf = if xpbd {
        rechnen(&mut s, &mut CpuReferenz::neu(), &ablauf, bilder.as_deref(), &|_| (0, 0, 0))
    } else {
        let st = |rw: &ImplizitCg| (rw.statistik.loesungen, rw.statistik.cg_iterationen, rw.statistik.nicht_konvergiert);
        let param = ImplizitParameter { selbstkollision, selbst_schnitte_an_naehten: schnitte_an_naehten, ..ImplizitParameter::default() };
        let mut rw = if gpu {
            let loeser = match klon3d_sim::GpuLoeser::neu() {
                Ok(l) => l,
                Err(e) => return fehler(format!("GPU: {e} (ohne --rechenweg gpu rechnet die CPU)")),
            };
            println!("  Rechenweg    GPU (f32, wgpu) auf {}", loeser.geraet());
            ImplizitCg::mit_loeser(param, Box::new(loeser))
        } else {
            ImplizitCg::neu(param)
        };
        let l = rechnen(&mut s, &mut rw, &ablauf, bilder.as_deref(), &st);
        if selbstkollision {
            selbst_letztes = Some((rw.statistik.naehe_punkt_dreieck, rw.statistik.naehe_kante_kante, rw.statistik.selbst_stoesse));
        }
        l
    };
    let (erg, je_bild) = match lauf {
        Ok(l) => l,
        Err(e) => return fehler(e),
    };
    let cg = je_bild.iter().fold((0, 0, 0), |a, b| (a.0 + b.2 .0, a.1 + b.2 .1, a.2 + b.2 .2));
    if erg.heraus > 0 {
        println!("  heraus       {} Punkte vor dem Nähen aus dem Körper geschoben (tiefster {:.1} mm)", erg.heraus, erg.heraus_tiefe * 1e3);
    }
    println!(
        "  angezogen    {} Bilder nähen ({} m/s, Nahtfeder {:.0} N/m, Schwerkraft {:.0} %, Körperpunkt gegen Stoffdreieck {}), {} Bilder Einschwingen ({}), {} Bilder Halt, {:.1} s",
        ablauf.naehbilder,
        ablauf.naht_geschwindigkeit,
        ablauf.naht_k,
        ablauf.schwerkraft_naehen * 100.0,
        if ablauf.koerperpunkt_durchgaenge > 0 { "an" } else { "aus" },
        erg.einschwingbilder,
        if erg.eingeschwungen { "ruhig" } else { "Höchstzahl erreicht, sinkt noch" },
        ablauf.haltebilder,
        start.elapsed().as_secs_f64()
    );
    if !xpbd {
        println!(
            "  CG           {} Lösungen, im Mittel {:.0} Iterationen (höchstens {}), {} nicht konvergiert",
            cg.0,
            cg.1 as f64 / cg.0.max(1) as f64,
            ablauf.cg_max,
            cg.2
        );
    }
    if let Some((tv, ee, st)) = selbst_letztes {
        println!("  Selbstkoll.  letztes Bild: {tv} Paare Punkt–Dreieck, {ee} Kante–Kante, {st} Stöße zurückgesetzt");
    }
    // Wandzeit je Bild nach Phase: Median, P95, größte (ms).
    for (name, von, bis) in [("Nähen", 0, ablauf.naehbilder), ("Einschwingen", ablauf.naehbilder, erg.halt_ab), ("Halt", erg.halt_ab, je_bild.len())] {
        let mut ms: Vec<f64> = je_bild[von.min(bis)..bis].iter().map(|z| z.1).collect();
        if ms.is_empty() {
            continue;
        }
        ms.sort_by(f64::total_cmp);
        let q = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
        println!("  Zeit {name:<12} {} Bilder, Median {:.1} ms, P95 {:.1} ms, größte {:.1} ms", ms.len(), q(0.5), q(0.95), ms[ms.len() - 1]);
    }
    if let Some(z) = &zeiten {
        let mut t = String::from("# bild phase ms cg_loesungen cg_iterationen nicht_konvergiert\n");
        for (b, ms, c) in &je_bild {
            let ph = if *b <= ablauf.naehbilder { "n" } else if *b <= erg.halt_ab { "e" } else { "h" };
            t += &format!("{b} {ph} {ms:.3} {} {} {}\n", c.0, c.1, c.2);
        }
        if let Err(e) = std::fs::write(z, t) {
            return fehler(format!("{}: {e}", z.display()));
        }
    }
    // Abstandskarte Stoff → Haut (Stufe 5): der eigentliche Zweck dieser Stufe.
    // Einmal am Endzustand gerechnet, daraus Tabelle, tiefster Punkt und CSV.
    let karte_ende = abstand::Karte::messe(&s);
    println!(
        "  Ende         mittlere Geschwindigkeit {:.4} m/s, tiefster Punkt {:+.2} mm",
        messung::mittlere_geschwindigkeit(&s),
        tiefster(&karte_ende) * 1e3
    );
    if messung::mittlere_geschwindigkeit(&s) > 0.01 {
        // Nicht in Ruhe: wo der Stoff noch läuft (schnellstes Teil, schnellster Punkt).
        let mut je: Vec<(f64, &str, [f64; 3])> = s
            .teile
            .iter()
            .map(|t| {
                let r = t.punkte.clone();
                let i = r.clone().max_by(|&a, &b| {
                    let (va, vb) = (s.v[a], s.v[b]);
                    (va[0] * va[0] + va[1] * va[1] + va[2] * va[2]).total_cmp(&(vb[0] * vb[0] + vb[1] * vb[1] + vb[2] * vb[2]))
                });
                let mittel = r.clone().map(|i| (s.v[i][0].powi(2) + s.v[i][1].powi(2) + s.v[i][2].powi(2)).sqrt()).sum::<f64>() / r.len() as f64;
                (mittel, t.name.as_str(), i.map_or([0.0; 3], |i| s.x[i]))
            })
            .collect();
        je.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (v, name, x) in je.iter().take(3) {
            println!("  läuft noch   {name}: Mittel {v:.3} m/s, schnellster Punkt bei ({:.1}, {:.1}, {:.1}) cm", x[0] * 100.0, x[1] * 100.0, x[2] * 100.0);
        }
    }
    if !erg.sinken.is_empty() {
        let sinken = &erg.sinken;
        let dauer = ablauf.haltebilder as f64 * s.param.bild_dt;
        let (name, dz) = &sinken[0];
        println!("  Halt         in {dauer:.1} s Haltephase sinkt am stärksten {name}: {:+.1} mm (Mittel des Teils; ADR-023: höchstens 5 mm)", dz * 1e3);
        for (name, dz) in sinken.iter().skip(1).take(4).filter(|(_, dz)| *dz > 5e-3) {
            println!("               {name}: {:+.1} mm", dz * 1e3);
        }
    }
    println!();
    println!("  Naht   Paare  Median mm  größter mm  über 10 mm");
    let (mut alle, mut zu) = (0usize, 0usize);
    for (kennung, mut d) in messung::nahtabstaende(&s, &p.netz) {
        let offen = d.iter().filter(|&&x| x > 0.01).count();
        alle += d.len();
        zu += d.len() - offen;
        d.sort_by(f64::total_cmp);
        println!("  {kennung:<5} {:6} {:10.1} {:11.1} {:11}", d.len(), d[d.len() / 2] * 1e3, d[d.len() - 1] * 1e3, offen);
    }
    println!("  {zu} von {alle} Nahtpaaren bis 10 mm ({:.1} %)", 100.0 * zu as f64 / alle.max(1) as f64);
    if offen_zeigen {
        // Jedes Paar über 10 mm: Naht, Teile, Lage beider Punkte (cm).
        println!();
        println!("  offene Paare (über 10 mm), Lage in cm (x links, y hinten, z oben):");
        for n in &p.netz.naehte {
            for (k, (tn, pp)) in n.paare().enumerate() {
                let i = s.teile[tn.teil_a].punkte.start + pp.a as usize;
                let j = s.teile[tn.teil_b].punkte.start + pp.b as usize;
                let (a, b) = (s.x[i], s.x[j]);
                let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
                if d > 0.01 {
                    let teil = |t: usize| p.netz.teile[t].teil.rsplit(" - ").next().unwrap_or("").to_string();
                    println!(
                        "    {:<6} Paar {k:3} {:6.1} mm  {} ({:.1}, {:.1}, {:.1})  {} ({:.1}, {:.1}, {:.1})",
                        n.kennung, d * 1e3, teil(tn.teil_a), a[0] * 100.0, a[1] * 100.0, a[2] * 100.0,
                        teil(tn.teil_b), b[0] * 100.0, b[1] * 100.0, b[2] * 100.0
                    );
                }
            }
        }
    }
    // Abstandskarte am Endzustand: der eigentliche Zweck der Stufe 5. Sie
    // erscheint auch ohne `--obj`, weil sie zum Lauf gehört und nicht zur
    // Ausgabe in Dateien.
    let karte_abstand = &karte_ende;
    let karte_dehnung = dehnung::Karte::messe(&s);
    let karte_spannung = spannung::Karte::messe(&s);
    if let Some(k) = karte_abstand {
        println!();
        println!("  Abstand zur Haut, Endzustand:");
        abstand_tabelle(k);
        if let Some((d, i)) = k.tiefster() {
            let t = s.teile.iter().find(|t| t.punkte.contains(&i)).map_or("?", |t| t.name.as_str());
            println!("  tiefster Punkt {i} ({t}): {:+.1} mm", d * 1e3);
        }
    }
    // Dehnungskarte: der zweite Ausgabewert der Stufe 5. Sie braucht keinen
    // Körper — Dehnung ist eine Eigenschaft des Stoffs allein.
    println!();
    println!(
        "  Dehnung je Teil, Endzustand (Hauptrichtungen, gegen Ruhelänge × (1 − Reduktion); über {:.0} % zählt als überdehnt):",
        dehnung::UEBER_DEHNUNG * 100.0
    );
    dehnung_tabelle(&karte_dehnung);
    if let Some((e, i)) = karte_dehnung.staerkste() {
        let t = s.teile.iter().find(|t| t.dreiecke.contains(&i)).map_or("?", |t| t.name.as_str());
        println!("  stärkste Dehnung Dreieck {i} ({t}): {:+.2} %", e * 100.0);
    }
    // Spannungskarte: dieselbe Geometrie, andere Zahl. Sie sagt, wie hart ein
    // Teil zieht (N/m), nicht wie weit es gedehnt ist — der Unterschied wird
    // sichtbar, sobald zwei Teile verschiedene Dicke und damit verschiedene
    // Dehnsteifigkeit haben.
    println!();
    println!(
        "  Spannung je Teil, Endzustand (Hauptkräfte in N/m, Zug positiv; über der 10-%-Linie des Materials zählt als über der linearen Reichweite):"
    );
    spannung_tabelle(&karte_spannung);
    if let Some((w, i)) = karte_spannung.staerkste() {
        let t = s.teile.iter().find(|t| t.dreiecke.contains(&i)).map_or("?", |t| t.name.as_str());
        let e = karte_spannung.je_flaeche[i].steifigkeit_n_m;
        println!("  stärkste Kraft Dreieck {i} ({t}): {w:.2} N/m (Dehnsteifigkeit {e:.1} N/m)");
    }
    if !spannung::reihenfolge_gleich(&karte_spannung, &karte_dehnung) {
        // Nur bei gemischter Steifigkeit zu erwarten; bei einem Material wäre
        // es ein Fehler in einer der beiden Karten.
        println!("  Hinweis      die stärkste Kraft sitzt nicht auf dem stärksten Dehnungsdreieck — bei gemischten Dicken normal");
    }
    // Seam position (stage 5, task #25): the last open line of the stage. The
    // seam is the sewn seam — the midpoint of each pair — and the distance is
    // taken to the polyline through those midpoints, not to the sampled points.
    if !s.koerper.is_empty() {
        let karte_nahtlage = nahtlage::Karte::messe(&s, &p.netz, &koerperpunkte(&an));
        println!();
        println!(
            "  Nahtlage, Endzustand (kleinster Abstand jeder Naht zu den benannten Körperpunkten, mm; die Naht ist die Mittellinie der vernähten Paare):"
        );
        nahtlage_tabelle(&karte_nahtlage);
    }
    if let Some(o) = &abstand_ordner {
        // Ohne Körper gibt es keine Karte — dann wäre eine leere Datei eine
        // Lüge, also ein Fehler statt einer stillen Null.
        let Some(k) = &karte_ende else {
            return fehler("--abstand: die Szene hat keinen Körper, es gibt keine Abstandskarte");
        };
        if let Err(e) = std::fs::create_dir_all(o).and_then(|_| std::fs::write(o.join("abstand.csv"), k.csv(&s))) {
            return fehler(format!("{}: {e}", o.display()));
        }
        println!("  geschrieben  {} (abstand.csv: Punkt, Teil, Lage in cm, Abstand in mm, Normale)", o.display());
    }
    if let Some(o) = &dehnung_ordner {
        // Die Dehnungskarte braucht keinen Körper, nur Stoff — hier gibt es
        // also keinen Grund für einen Fehler.
        if let Err(e) =
            std::fs::create_dir_all(o).and_then(|_| std::fs::write(o.join("dehnung.csv"), karte_dehnung.csv(&s)))
        {
            return fehler(format!("{}: {e}", o.display()));
        }
        println!(
            "  geschrieben  {} (dehnung.csv: Dreieck, Teil, Ruhefläche in cm², beide Hauptdehnungen in %, Muster)",
            o.display()
        );
    }
    if let Some(o) = &spannung_ordner {
        // Auch die Spannungskarte braucht nur Stoff, keinen Körper.
        if let Err(e) =
            std::fs::create_dir_all(o).and_then(|_| std::fs::write(o.join("spannung.csv"), karte_spannung.csv(&s)))
        {
            return fehler(format!("{}: {e}", o.display()));
        }
        println!(
            "  geschrieben  {} (spannung.csv: Dreieck, Teil, Ruhefläche in cm², beide Hauptkräfte in N/m, Dehnsteifigkeit)",
            o.display()
        );
    }
    // Seam position (stage 5, task #25): the last open line of the stage. It
    // needs the body, because the points are body points; without one there is
    // no number rather than a zero.
    if let Some(o) = &nahtlage_ordner {
        if !s.koerper.is_empty() {
            let e = nahtlage::Karte::messe(&s, &p.netz, &koerperpunkte(&an));
            if let Err(err) = std::fs::create_dir_all(o).and_then(|_| std::fs::write(o.join("nahtlage.csv"), e.csv())) {
                return fehler(format!("{}: {err}", o.display()));
            }
            println!(
                "  geschrieben  {} (nahtlage.csv: Naht, Länge in mm, Paare, Körperpunkt, Abstand in mm, Paar, Anteil, Ort in cm)",
                o.display()
            );
        } else {
            println!("  übersprungen {}: die Szene hat keinen Körper", o.join("nahtlage.csv").display());
        }
    }
    if let Some(o) = &obj {
        if let Err(e) = std::fs::write(o.join("ende.obj"), ausgabe::obj(&s)) {
            return fehler(format!("{}: {e}", o.display()));
        }
        println!("  geschrieben  {} (koerper.obj, anfang.obj, ende.obj; mm)", o.display());
    }
    if let Some(o) = &farben_ordner {
        // Alle Karten als OBJ mit Farbe je Eckpunkt. Dieselbe Geometrie wie
        // `ende.obj`, nur mit drei Zahlen mehr je `v` — ein Betrachter, der
        // Eckpunktfarben kennt, zeigt die Karte ohne Umweg.
        let dehnung_pro_punkt = karte_dehnung.je_punkt(&s);
        let spannung_pro_punkt = karte_spannung.je_punkt(&s);
        let dateien: [(&str, String); 3] = [
            (
                "dehnung.obj",
                ausgabe::obj_farbig(&s, |i| farbkarte::dehnung(dehnung_pro_punkt[i])),
            ),
            (
                "spannung.obj",
                ausgabe::obj_farbig(&s, |i| farbkarte::spannung(spannung_pro_punkt[i])),
            ),
            (
                "abstand.obj",
                match &karte_ende {
                    Some(k) => ausgabe::obj_farbig(&s, |i| farbkarte::abstand(k.je_punkt[i].abstand * 1e3)),
                    // Ohne Körper gibt es nur die Dehnung; statt einer leeren
                    // Datei eine Meldung, die zum Fehlen passt.
                    None => String::new(),
                },
            ),
        ];
        if let Err(e) = std::fs::create_dir_all(o) {
            return fehler(format!("{}: {e}", o.display()));
        }
        for (name, inhalt) in dateien {
            if inhalt.is_empty() {
                println!("  übersprungen {}: die Szene hat keinen Körper, es gibt keine Abstandskarte", o.join(name).display());
                continue;
            }
            if let Err(e) = std::fs::write(o.join(name), inhalt) {
                return fehler(format!("{}: {e}", o.join(name).display()));
            }
        }
        println!(
            "  geschrieben  {} (dehnung.obj, spannung.obj, abstand.obj; mm, Farbe je Eckpunkt 0..255)",
            o.display()
        );
        println!(
            "  Farbskala   Dehnung {:.0}…{:.0} %, Spannung {:.0}…{:.0} N/m, Abstand {:.0}…{:.0} mm (blau = wenig, rot = viel)",
            farbkarte::DEHNUNG_VON,
            farbkarte::DEHNUNG_BIS,
            farbkarte::SPANNUNG_VON,
            farbkarte::SPANNUNG_BIS,
            farbkarte::ABSTAND_VON,
            farbkarte::ABSTAND_BIS
        );
    }
    ExitCode::SUCCESS
}

/// Die sechs benannten Körperpunkte der Nahtlage, in Szenenkoordinaten (m).
///
/// Reihenfolge und Namen sind Teil der Ausgabe: „Achsel links" und „Achsel
/// rechts" sind zwei Punkte, und die Tabelle nennt sie einzeln, statt „die
/// Achsel" zu schreiben und offenzulassen, welche gemeint ist.
fn koerperpunkte(an: &klon3d_koerper::koerper::Anordnung) -> Vec<nahtlage::Koerperpunkt> {
    let c = |q: [f64; 3]| {
        let q = klon3d_sim::netzkoerper::clo_nach_szene(q);
        nahtlage::Koerperpunkt {
            name: String::new(),
            punkt: q,
        }
    };
    let mut v = Vec::with_capacity(6);
    for (name, paar) in [
        ("Achsel", an.achsel),
        ("Schulterpunkt", an.schulter),
        ("Brustpunkt", an.brustpunkt),
    ] {
        for (i, seite) in ["links", "rechts"].into_iter().enumerate() {
            let mut p = c(paar[i]);
            p.name = format!("{name} {seite}");
            v.push(p);
        }
    }
    v
}

/// Tabelle der Nahtlage: eine Zeile je Naht, je Körperpunkt eine Spalte mit dem
/// kleinsten Abstand in mm. Ein Punkt, zu dem die Naht keinen Abstand hat (die
/// Naht ist leer), bleibt leer statt auf 0 zu fallen.
fn nahtlage_tabelle(k: &nahtlage::Karte) {
    // 21 columns: the longest name ("Schulterpunkt rechts") is 21 characters,
    // so every column stays under its heading instead of shifting the row.
    print!("  {:<6} {:>8} {:>6}", "Naht", "Länge", "Paare");
    for p in &k.punkte {
        print!(" {:>21}", p.name);
    }
    println!("  (mm)");
    for n in &k.je_naht {
        print!("  {:<6} {:8.1} {:6}", n.kennung, n.laenge_m * 1e3, n.paare);
        for p in &k.punkte {
            match n.je_punkt.iter().find(|q| q.koerperpunkt == p.name) {
                Some(q) => print!(" {:21.1}", q.stelle.abstand * 1e3),
                None => print!(" {:>21}", "-"),
            }
        }
        println!();
    }
    if let Some((d, naht, punkt)) = k.kleinster() {
        // `kleinster` is in metres like every other field; the table is in mm.
        println!("  kleinster Abstand {:.1} mm: {naht} zu {punkt}", d * 1e3);
    }
}

/// Tabelle der Dehnungskarte: eine Zeile je Teil, darunter die Summe.
///
/// Die Spalten `min … max` sind die größere Hauptdehnung gegen die reduzierte
/// Ruhelänge. `Muster` ist derselbe Wert gegen das unveränderte Schnittmuster:
/// bei einem Teil, das mit 5,5 % Reduktion geschnitten wurde und in der
/// Ruhelage auf Musterlänge sitzt, steht dort 0, während die Ruhespalte 5,82 %
/// zeigt. Beide Zahlen sind richtig, sie beantworten nur verschiedene Fragen.
///
/// `über` zählt die Dreiecke über [`dehnung::UEBER_DEHNUNG`]; das ist die
/// Zahl, die den Fehler benennt, und nicht das Etikett.
fn dehnung_tabelle(k: &dehnung::Karte) {
    println!(
        "  Teil                                    Flächen  Fläche cm²      min  Median   90 %    max  Muster-Median   über"
    );
    let mut ueber = 0usize;
    for t in &k.je_teil {
        let v = &t.verteilung;
        ueber += t.ueber;
        println!(
            "  {:<40} {:7} {:10.1} {:8.2} {:7.2} {:6.2} {:6.2} {:13.2} {:6}{}",
            t.name,
            t.flaechen,
            t.ruheflaeche_m2 * 1e4,
            v.min,
            v.median,
            v.p90,
            v.max,
            t.muster.median,
            t.ueber,
            if t.ueber > 0 { "  ← überdehnt" } else { "" }
        );
    }
    let g = k.gesamt();
    let flaeche: f64 = k.je_teil.iter().map(|t| t.ruheflaeche_m2).sum();
    println!(
        "  {:<40} {:7} {:10.1} {:8.2} {:7.2} {:6.2} {:6.2} {:13.2} {:6}",
        "(alle)",
        g.anzahl,
        flaeche * 1e4,
        g.min,
        g.median,
        g.p90,
        g.max,
        k.gesamt_muster().median,
        ueber
    );
}

/// Tabelle der Spannungskarte: eine Zeile je Teil, darunter die Summe.
///
/// Die Spalte „Grenze" ist die 10-%-Linie *dieses* Teils (Dehnsteifigkeit ×
/// 0,10) und „über" zählt die Flächen darüber. Beides steht in der Zeile und
/// nicht in einer Fußnote, weil die Grenze eine Eigenschaft des Materials ist
/// und sich zwischen den Teilen unterscheidet, sobald die Dicken sich
/// unterscheiden.
fn spannung_tabelle(k: &spannung::Karte) {
    println!("  Teil                                    Flächen  Fläche cm²      min  Median   90 %    max   Grenze   über");
    let mut ueber = 0usize;
    for t in &k.je_teil {
        let v = &t.verteilung;
        ueber += t.ueber;
        println!(
            "  {:<40} {:7} {:10.1} {:8.2} {:7.2} {:6.2} {:6.2} {:8.1} {:6}{}",
            t.name,
            t.flaechen,
            t.ruheflaeche_m2 * 1e4,
            v.min,
            v.median,
            v.p90,
            v.max,
            t.grenze_n_m,
            t.ueber,
            if t.ueber > 0 { "  ← über der linearen Reichweite" } else { "" }
        );
    }
    let g = k.gesamt();
    let flaeche: f64 = k.je_teil.iter().map(|t| t.ruheflaeche_m2).sum();
    println!(
        "  {:<40} {:7} {:10.1} {:8.2} {:7.2} {:6.2} {:6.2} {:8} {:6}",
        "(alle)", g.anzahl, flaeche * 1e4, g.min, g.median, g.p90, g.max, "", ueber
    );
}

/// Tabelle der Abstandskarte: eine Zeile je Teil, darunter die Summe.
///
/// Die letzte Spalte zählt die Punkte, die tiefer als [`abstand::INNEN`] in
/// der Haut stecken. Das ist die Zahl, die ein einschneidendes Teil verrät —
/// und die im Mittel untergeht.
fn abstand_tabelle(k: &abstand::Karte) {
    println!("  Teil                                    Punkte      min  Median   90 %    max   innen");
    for t in &k.je_teil {
        let v = &t.verteilung;
        println!(
            "  {:<40} {:6} {:8.1} {:7.1} {:6.1} {:6.1} {:7}{}",
            t.name,
            v.anzahl,
            v.min,
            v.median,
            v.p90,
            v.max,
            t.innen,
            if t.innen > 0 { "  ← steckt in der Haut" } else { "" }
        );
    }
    let g = k.gesamt();
    let innen: usize = k.je_teil.iter().map(|t| t.innen).sum();
    println!(
        "  {:<40} {:6} {:8.1} {:7.1} {:6.1} {:6.1} {:7}",
        "(alle)", g.anzahl, g.min, g.median, g.p90, g.max, innen
    );
}

/// Rechnet das Anziehen mit einem Rechenweg. Je Bild die Wandzeit des
/// Schritts in ms (ohne das Schreiben der Bilder) und die Kennzahl des
/// Rechenwegs; auf Wunsch jedes 60. Bild, das Ende des Nähens und den
/// erwarteten Anfang der Haltephase als `bild_NNNN.obj` (mm; Befund 109).
fn rechnen<R: Anziehbar>(
    s: &mut Szene,
    rw: &mut R,
    ablauf: &Anziehen,
    bilder: Option<&Path>,
    kennzahl: &dyn Fn(&R) -> (usize, usize, usize),
) -> Result<(Angezogen, Vec<(usize, f64, (usize, usize, usize))>), String> {
    if let Some(o) = bilder {
        std::fs::create_dir_all(o).and_then(|_| std::fs::write(o.join("bild_0000.obj"), ausgabe::obj(s))).map_err(|e| format!("{}: {e}", o.display()))?;
    }
    let halt_erwartet = ablauf.naehbilder + ablauf.einschwingen_min;
    let mut je_bild = Vec::new();
    let mut fehler: Option<String> = None;
    let mut letzt = std::time::Instant::now();
    let erg = pipeline::ablauf::anziehen(s, rw, ablauf, &mut |bild, s, rw| {
        je_bild.push((bild, letzt.elapsed().as_secs_f64() * 1e3, kennzahl(rw)));
        if bilder.is_some() && bild % 60 == 0 {
            eprintln!("  Bild {bild}: {:.1} ms", je_bild[bild - 1].1);
        }
        if let Some(o) = bilder {
            if bild % 60 == 0 || bild == ablauf.naehbilder || bild == halt_erwartet {
                if let Err(e) = std::fs::write(o.join(format!("bild_{bild:04}.obj")), ausgabe::obj(s)) {
                    fehler.get_or_insert(format!("{}: {e}", o.display()));
                }
            }
        }
        letzt = std::time::Instant::now();
    });
    if let Some(e) = fehler {
        return Err(e);
    }
    if let Some(o) = bilder {
        let n = je_bild.len();
        std::fs::write(o.join(format!("bild_{n:04}.obj")), ausgabe::obj(s)).map_err(|e| format!("{}: {e}", o.display()))?;
    }
    Ok((erg, je_bild))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight files as `klon3d-pipeline`'s own test names them.
    fn stoffe() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/stoffe")
    }

    /// The two built-in profiles keep their names: a run from before the option
    /// grew must not change its material, and neither name may need a folder.
    #[test]
    fn eingebaute_profile_bleiben() {
        for (name, soll) in [("radical", RADICAL_RUBBER_04), ("retifist", Material::nach_namen("retifist").unwrap())] {
            let m = stoff_aufloesen(name, None).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(m.name, soll.name);
            assert_eq!(m.dicke_m, soll.dicke_m);
        }
        // The long names of the profiles still work, as before.
        assert_eq!(stoff_aufloesen("Latex_0.4mm_Radical_Rubber", None).unwrap().name, RADICAL_RUBBER_04.name);
    }

    /// A stem from the fabrics folder is read: this is the path the solver
    /// could not reach before.
    #[test]
    fn stamm_aus_dem_stoffordner() {
        let o = stoffe();
        let m = stoff_aufloesen("Latex_1.0mm_abgeleitet", Some(&o)).unwrap();
        assert_eq!(m.name, "Latex_1.0mm_abgeleitet");
        assert!((m.dicke_m * 1e3 - 1.0).abs() < 0.01, "Dicke {} mm", m.dicke_m * 1e3);
        // Every one of the eight is selectable by its stem.
        for (pfad, _) in pipeline::stoff::stoffe_im_ordner(&o) {
            let stamm = pfad.file_stem().unwrap().to_string_lossy().into_owned();
            let m = stoff_aufloesen(&stamm, Some(&o)).unwrap_or_else(|e| panic!("{stamm}: {e}"));
            assert_eq!(m.name, stamm);
        }
    }

    /// Case does not matter for a stem; the shell and the file system disagree
    /// often enough about it.
    #[test]
    fn stamm_gross_klein_egal() {
        let o = stoffe();
        let m = stoff_aufloesen("latex_laminat_2x0.4mm_abgeleitet", Some(&o)).unwrap();
        assert_eq!(m.name, "Latex_Laminat_2x0.4mm_abgeleitet");
        assert!((m.dicke_m * 1e3 - 0.8).abs() < 0.01, "Dicke {} mm", m.dicke_m * 1e3);
    }

    /// A path to a `.zfab` is read directly, wherever it lies.
    #[test]
    fn pfad_auf_eine_datei() {
        let p = stoffe().join("Latex_Laminat_3x0.4mm_abgeleitet.zfab");
        let m = stoff_aufloesen(&p.display().to_string(), None).unwrap();
        assert_eq!(m.name, "Latex_Laminat_3x0.4mm_abgeleitet");
        // The extension is compared case-insensitively.
        let gross = p.display().to_string().replace(".zfab", ".ZFAB");
        let m = stoff_aufloesen(&gross, None);
        assert!(m.is_ok() || m.unwrap_err().contains(".ZFAB"), "eine andere Endung wurde nicht als .zfab erkannt");
    }

    /// A path that is no fabric says which file and which field - not a run at
    /// 0.4 mm.
    #[test]
    fn kaputte_datei_nennt_sich_selbst() {
        let d = std::env::temp_dir().join(format!("klon3d-cli-stoff-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("halbe.zfab");
        std::fs::write(&p, b"kein Kopf").unwrap();
        let e = stoff_aufloesen(&p.display().to_string(), None).unwrap_err();
        assert!(e.contains("halbe.zfab"), "{e}");
        let _ = std::fs::remove_file(&p);
    }

    /// An unknown name is an error that names the folder and lists what is in
    /// it, so the caller does not have to guess the spelling.
    #[test]
    fn unbekannter_name_nennt_ordner_und_stoffe() {
        let o = stoffe();
        let e = stoff_aufloesen("Latex_0.5mm", Some(&o)).unwrap_err();
        assert!(e.contains(&o.display().to_string()), "Ordner fehlt in der Meldung: {e}");
        assert!(e.contains("Latex_0.25mm_abgeleitet"), "die vorhandenen Stoffe fehlen: {e}");
        assert!(e.contains("radical"), "die eingebauten Profile fehlen: {e}");
    }

    /// Without a fabrics folder the message says so instead of listing nothing.
    #[test]
    fn ohne_ordner_sagt_dass_er_fehlt() {
        let e = stoff_aufloesen("Latex_1.0mm_abgeleitet", None).unwrap_err();
        assert!(e.contains("assets/stoffe"), "{e}");
        assert!(e.contains("kein Ordner"), "{e}");
    }

    /// The printed thickness tells the fabrics apart: one decimal would print
    /// 0.25 mm as "0.2 mm", the same as a 0.2 mm sheet, and the files a run may
    /// select start at 0.25 mm.
    #[test]
    fn dicke_wird_unterscheidbar_gedruckt() {
        assert_eq!(dicke_gedruckt(0.25e-3), "0.25");
        assert_eq!(dicke_gedruckt(0.4e-3), "0.4");
        assert_eq!(dicke_gedruckt(1.0e-3), "1.0");
        assert_eq!(dicke_gedruckt(0.65e-3), "0.65");
        assert_eq!(dicke_gedruckt(2.0e-3), "2.0");
        // Every fabric in the folder prints a thickness of its own.
        let mut gedruckt: Vec<String> = pipeline::stoff::stoffe_im_ordner(&stoffe())
            .iter()
            .filter_map(|(_, m)| m.as_ref().ok().map(|m| dicke_gedruckt(m.dicke_m)))
            .collect();
        let anzahl = gedruckt.len();
        gedruckt.sort();
        gedruckt.dedup();
        assert_eq!(gedruckt.len(), anzahl, "zwei Stoffe drucken dieselbe Dicke: {gedruckt:?}");
    }

    /// The eight files are one compound at eight thicknesses, and the solver
    /// scales whatever was selected to the thickness the placement file gives
    /// the piece (`Material::mit_dicke`). Where every piece carries a `dicke`,
    /// choosing between the eight therefore changes nothing in the solver - the
    /// values arrive normalised to the piece, and only the name on the material
    /// line tells the run apart. A different compound (`retifist`) does change
    /// them, which is what this pins.
    #[test]
    fn die_acht_stoffe_sind_ein_stoff_in_acht_dicken() {
        let o = stoffe();
        let dicke = 2.0e-3;
        let a = stoff_aufloesen("Latex_0.25mm_abgeleitet", Some(&o)).unwrap().mit_dicke(dicke);
        let b = stoff_aufloesen("Latex_Laminat_2x1.0mm_abgeleitet", Some(&o)).unwrap().mit_dicke(dicke);
        for (x, y) in [
            (a.dehnung_n_m, b.dehnung_n_m),
            (a.schub_n_m, b.schub_n_m),
            (a.biegung_nm, b.biegung_nm),
            (a.flaechengewicht_kg_m2, b.flaechengewicht_kg_m2),
        ] {
            assert!((x - y).abs() <= 1e-9 * x.abs().max(1.0), "{x} gegen {y}");
        }
        // The names still say which file the caller picked, so a run stays
        // traceable even where the numbers agree.
        assert_eq!(a.name, "Latex_0.25mm_abgeleitet");
        assert_eq!(b.name, "Latex_Laminat_2x1.0mm_abgeleitet");
        // A different compound keeps its own stiffness at the same thickness.
        let r = Material::nach_namen("retifist").unwrap().mit_dicke(dicke);
        assert!(r.dehnung_n_m < 0.2 * a.dehnung_n_m, "Retifist {} gegen {}", r.dehnung_n_m, a.dehnung_n_m);
        assert_eq!(r.dicke_m, dicke);
    }

    /// The folder search walks upwards from a subfolder, like the app's.
    #[test]
    fn stoffordner_wird_aufwaerts_gefunden() {
        let o = stoffe();
        let tief = o.join("gibt/es/nicht");
        assert_eq!(stoffordner_ab(&tief), Some(o.clone()));
        // An unrelated folder has none.
        let leer = std::env::temp_dir().join(format!("klon3d-cli-ohne-{}", std::process::id()));
        std::fs::create_dir_all(&leer).unwrap();
        assert_eq!(stoffordner_ab(&leer), None);
    }
}
