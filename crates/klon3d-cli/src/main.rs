//! Kommandozeile für `klon3d`.
//!
//! ```text
//! klon3d info   <muster.sm2d>
//! klon3d punkte <muster.sm2d> [--json] [--vergleich <dump.txt>]
//! klon3d stoelle <vergleich.txt> <muster.sm2d>
//! klon3d umriss <muster.sm2d> [--svg <datei.svg>]
//! klon3d dxf    <muster.sm2d> <aus.dxf> [--teile <muster,...>]
//! klon3d naehte <muster.sm2d> [--liste <datei.naehte>] [--kanten] [--zeichnung] [--toleranz <cm>]
//! klon3d netz   <muster.sm2d> [--liste <datei.naehte>] [--ohne-naehte] [--kante <mm>] [--winkel <grad>]
//!               [--fang <mm>] [--zeichnung] [--svg <datei>] [--nebeneinander] [--obj <datei>] [--paare <datei>]
//! klon3d roehre [--material radical|retifist] [--reduktion <%>] [--kante <mm>] [--spalt] [--rutsch <s>] [--obj <datei>]
//! klon3d koerper <massdatei.smis> [--out <koerper.obj>] [--ordner <Resizing>] [--wie-clo]
//! klon3d lage   <muster.sm2d> [--naehte <datei>] [--schreiben <datei>]
//! klon3d zprj   <projekt.zprj> [--teile] [--naehte] [--stoffe] [--netz]
//!                [--obj <datei.obj>] [--koerper-obj <datei.obj>]
//! klon3d anziehen <muster.sm2d> [--masse <datei.smis>] [--naehte <datei>] [--lage <datei>] [--naehen <m/s>]
//!                [--naehbilder <n>] [--einschwingen-max <n>] [--ohne-koerperpunkte]
//!                [--material <profil|datei.zfab|stamm>]
//!                [--obj <ordner>] [--ordner <Resizing>]
//! ```
//!
//! `info` liest nur. `punkte` rechnet die Konstruktion aus und meldet, was
//! nicht auflösbar ist. `--vergleich` stellt die Ergebnisse neben einen Dump
//! von `schnitt.py` — zwei Rechnungen desselben Musters nebeneinander.
//! `dxf` schreibt die Nahtlinien als AAMA-DXF für CLO (siehe [`dxf`]).
//! `naehte` teilt die Nahtlinien in Randkanten und prüft die Nahtpaare einer
//! Nahtliste auf gleiche Länge (siehe [`klon3d_pattern::naehte`]).
//! `netz` vernetzt jedes Teil und teilt die Nähte in Punktpaare auf (siehe
//! [`klon3d_mesh`]).
//! `koerper` formt den Körper aus der Maßdatei (siehe
//! [`klon3d_koerper::koerper`]) und schreibt die Gruppe `body` in mm.

mod anziehen;
mod stoffwahl;
mod umriss;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use klon3d_pattern::geom::{lies_messungen, Auswertung, Pt};
use klon3d_pattern::{read_path, Pattern};
use klon3d_pipeline::dxf;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        hilfe();
        return ExitCode::SUCCESS;
    }
    let rc = match args[0].as_str() {
        "info" => befehl_info(&args[1..]),
        "punkte" => befehl_punkte(&args[1..]),
        "umriss" => befehl_umriss(&args[1..]),
        "stoelle" => befehl_stoelle(&args[1..]),
        "dxf" => befehl_dxf(&args[1..]),
        "naehte" => befehl_naehte(&args[1..]),
        "netz" => befehl_netz(&args[1..]),
        "roehre" => befehl_roehre(&args[1..]),
        "koerper" => befehl_koerper(&args[1..]),
        "skelett" => befehl_skelett(&args[1..]),
        "anziehen" => anziehen::befehl_anziehen(&args[1..]),
        "lage" => befehl_lage(&args[1..]),
        "zprj" => befehl_zprj(&args[1..]),
        "vergleich" => befehl_vergleich(&args[1..]),
        andere => {
            eprintln!("unbekannter Befehl {:?}\n", andere);
            hilfe();
            ExitCode::FAILURE
        }
    };
    rc
}

fn hilfe() {
    eprintln!(
        "klon3d — Seamly2D-Muster lesen und auswerten

  klon3d info   <muster.sm2d>
      liest das Muster und meldet seinen Aufbau

  klon3d punkte <muster.sm2d> [--json] [--vergleich <dump.txt>]
      rechnet alle Konstruktionspunkte aus; --vergleich stellt sie neben
      einen Dump von schnitt.py

  klon3d stoelle <vergleich.txt> <muster.sm2d>
      misst die Abweichung zu einem Dump, nennt das schlechteste Ergebnis

  klon3d umriss <muster.sm2d> [--svg <datei.svg>]
      Nahtlinie je Teil: Laenge und Huelle, auf Wunsch als SVG

  klon3d dxf    <muster.sm2d> <aus.dxf> [--teile <muster,...>]
      AAMA-DXF fuer CLO (Import ohne \"Swap Cutting Line and Sewing Line\"):
      Kante = Nahtlinie, Passzeichen 4 mm nach innen, keine Innenlinien;
      --teile nimmt nur Teile, deren Name so beginnt (2,3 oder BL1,ST_VR)

  klon3d naehte <muster.sm2d> [--liste <datei.naehte>] [--kanten] [--zeichnung]
                [--toleranz <cm>]
      Randkanten je Teil (Nahtlinie zwischen Punktknoten) und Nahtpaare der
      Nahtliste (sonst <muster>.naehte daneben): Laengen beider Seiten,
      Passzeichen, Abdeckung; Rueckgabe 1, wenn eine Naht mehr als die
      Toleranz (0.08 cm) abweicht, eine Seite die Laenge 0 hat, eine Kante
      in zwei Naehten steht oder die Liste leer ist; --zeichnung misst auf
      der gezeichneten Polylinie statt auf Seamlys Kurvenzerlegung

  klon3d netz   <muster.sm2d> [--liste <datei.naehte>] [--ohne-naehte]
                [--kante <mm>] [--winkel <grad>] [--fang <mm>] [--zeichnung]
                [--svg <datei>] [--nebeneinander] [--obj <datei>] [--paare <datei>]
      Dreiecksnetz je Teil (Kante 10 mm, Mindestwinkel 25 Grad, Rand auf
      0,1 mm an der Randkante) und Punktpaare je Naht der Nahtliste (sonst
      <muster>.naehte daneben, ohne Liste nur die Netze); Rueckgabe 1, wenn
      die Abnahme nicht besteht; --svg zeichnet Netze und Nahtpunkte
      (--nebeneinander: Teile in Reihen statt in Musterlage), --obj schreibt
      die Netze, --paare die Punktpaare; --fang legt Punktknoten beider
      Nahtseiten bis zu diesem Abstand zusammen (Vorgabe 2 mm, 0 = CLOs
      reine Anteilsregel)

  klon3d roehre [--material radical|retifist] [--reduktion <%>] [--kante <mm>]
                [--spalt] [--rutsch <s>] [--obj <datei.obj>]
      Pruefroehre des Solvers (Stufe 4a): Rechteck zur Roehre vernaeht, um
      einen Zylinder gelegt, bis zur Ruhe gerechnet; misst Umfangsdehnung
      gegen die Rechnung, Nahtspalt, Abheben; --rutsch rechnet danach mit
      Schwerkraft und vergleicht den Weg mit Coulomb; --obj schreibt den
      Endzustand (mm) und daneben die Dehnung je Dreieck (.csv)

  klon3d koerper <massdatei.smis> [--out <koerper.obj>] [--ordner <Resizing>]
                [--wie-clo]
      formt den Koerper mit der Formungskette nach CLO (Fallback: alter Weg
      ueber anpassen, wenn Resizing-Daten fehlen) und formt danach zwischen
      Taille und Huefte nach, was ueber CLO hinausgeht; --wie-clo laesst das
      weg und gibt den Koerper genau wie die Kette; --out schreibt die
      Groesse als OBJ

  klon3d skelett [--out <koerper.obj>] [--drehung <grad>] [--ordner <makehuman>]
                 [--hoehe <cm>]
      Stufe 7: Skelett, Hautgewichte und Verformung auf dem freien MakeHuman-
      Koerper (CC0). Liest Gelenke aus denselben joint-*-Gruppen, rechnet die
      Gewichte aus dem Abstand zu den Knochenstrecken (vier Einfluesse, je
      Knochen eigener Radius) und meldet die Abnahme bei 90 Grad Armheben:
      ferne Kanten (ueber 120 mm von der Schulter) duerfen sich hoechstens
      verdoppeln und um hoechstens 10 mm wachsen, die Binde-Pose muss den
      Koerper unveraendert lassen; --drehung hebt um einen anderen Winkel,
      --out schreibt den verformten Koerper als OBJ (mm)

  klon3d anziehen <muster.sm2d> [--masse <datei.smis>] [--naehte <datei>]
                  [--lage <datei>] [--naehen <m/s>] [--naehbilder <n>]
                  [--einschwingen-max <n>] [--ohne-koerperpunkte]
                  [--material <profil|datei.zfab|stamm>] [--obj <ordner>] [--ordner <Resizing>]
      legt die Teile nach der Lagedatei (<muster>.lage) um den Koerper der
      Massdatei (Vorgabe: die des Musters) und zieht sie an (Stufe 4c): die
      Naehte schliessen sich mit begrenzter Geschwindigkeit (Vorgabe 0,1 m/s
      ueber 200 Bilder), dabei sticht kein Koerperpunkt durch den Stoff; dann
      250 Bilder Ruhe, mit Schwerkraft; meldet die Nahtabstaende je Naht;
      --material nimmt ein eingebautes Profil (radical, retifist), einen Pfad
      auf eine .zfab, oder den Stamm eines Stoffs aus assets/stoffe (Vorgabe:
      0,4 mm Radical); --obj schreibt koerper.obj, anfang.obj und ende.obj (mm)

  klon3d vergleich <a.obj> <b.obj> [--zelle <mm>] [--csv <datei>] [--farben <ordner>]
      stellt zwei Netze gegeneinander (Stufe 5): Abstand jedes Knotens zur
      FLAECHE des anderen Netzes, in beide Richtungen - nicht ueber gleiche
      Punktnummern, weil die Vernetzung nichts ueber die Simulation sagt.
      Meldet je Richtung Anzahl, Mittel, Median, 90 %, 99 % und den groessten
      Einzelwert (mm), dazu den Median je Gruppe, wenn beide Netze
      Gruppennamen tragen (o/g im OBJ); --csv schreibt die Abstaende je
      Knoten, --zelle ist die Kantenlaenge des Suchgitters (Vorgabe 20 mm),
      --farben schreibt beide Netze als OBJ mit Farbe je Eckpunkt (mm; blau
      bis 2 mm, im Rauschen des Vergleichs, rot ab 30 mm)
"
    );
}

// ───────────────────────────────────────────────────────────── info ──

fn befehl_info(args: &[String]) -> ExitCode {
    let Some(pfad) = pfad(args) else {
        eprintln!("Aufruf: klon3d info <muster.sm2d>");
        return ExitCode::FAILURE;
    };
    let muster = match read_path(&pfad) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", pfad.display());
            return ExitCode::FAILURE;
        }
    };

    println!("{}", pfad.display());
    println!("  Version      {}", muster.version);
    println!("  Einheit      {}", muster.unit);
    if let Some(m) = &muster.measurements {
        println!("  Massdatei    {m}");
    }
    println!("  Bloecke      {}", muster.draft_blocks.len());
    for b in &muster.draft_blocks {
        let n_p = b.calculation.as_ref().map_or(0, |c| c.points.len());
        let n_s = b.calculation.as_ref().map_or(0, |c| c.splines.len());
        let n_l = b.calculation.as_ref().map_or(0, |c| c.lines.len());
        let n_a = b.calculation.as_ref().map_or(0, |c| c.arcs.len());
        let n_m = b.modeling.as_ref().map_or(0, |m| m.points.len() + m.splines.len());
        let n_st = b.pieces.as_ref().map_or(0, |p| p.items.len());
        println!(
            "    {:<28} Punkte {n_p:>4}  Splines {n_s:>3}  Linien {n_l:>3}  Boegen {n_a:>3}  Modeling {n_m:>4}  Stuecke {n_st:>3}",
            b.name.clone().unwrap_or_else(|| "<ohne Namen>".into()),
        );
    }
    println!("  Punkte       {}", muster.point_count());
    println!("  Splines      {}", muster.spline_count());
    println!("  Stuecke      {}", muster.pieces().len());
    if let Some(inc) = &muster.increments {
        println!("  Variablen    {}", inc.items.len());
    }
    println!("  Punkttypen:");
    for (t, n) in muster.point_kinds() {
        println!("    {t:<24} {n:>5}");
    }
    ExitCode::SUCCESS
}

fn pfad(args: &[String]) -> Option<PathBuf> {
    args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from)
}

// ─────────────────────────────────────────────────────────── naehte ──

/// Text auf `n` Zeichen gekürzt, sonst rechts aufgefüllt.
fn spalte(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    } else {
        format!("{s:<n$}")
    }
}

/// `klon3d naehte --vorschlag`: Nahtliste aus dem Muster allein
/// ([`klon3d_pattern::vorschlag`]), auf Wunsch geschrieben und mit einer
/// Soll-Liste verglichen.
fn naehte_vorschlagen(
    ev: &Auswertung<'_>,
    muster_pfad: &Path,
    quelle: klon3d_pattern::kanten::Kurvenquelle,
    schreiben: Option<&Path>,
    gegen: Option<&Path>,
) -> ExitCode {
    use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte, TOLERANZ_CM};
    use klon3d_pattern::vorschlag::{nahtliste_text, vergleiche, verdrehte_naehte};
    let v = match ev.schlage_naehte_vor(quelle) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}: {e}", muster_pfad.display());
            return ExitCode::FAILURE;
        }
    };
    let kopf = format!(
        "Nahtvorschlag zu {} (klon3d naehte --vorschlag), {} Naehte.\nAus gleichen Laengen, Kerben, gemeinsamen Punkten und der Lage in der Zeichnung; bitte durchsehen.",
        muster_pfad.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        v.naehte.len()
    );
    let text = nahtliste_text(&v.naehte, &kopf);
    match schreiben {
        Some(p) => {
            if let Err(e) = std::fs::write(p, &text) {
                eprintln!("{}: {e}", p.display());
                return ExitCode::FAILURE;
            }
            println!("{}: {} Nähte vorgeschlagen, geschrieben nach {}", muster_pfad.display(), v.naehte.len(), p.display());
        }
        None => print!("{text}"),
    }
    // Selbstprüfung: alle vorgeschlagenen Nähte lösbar und gleich lang.
    match pruefe_naehte(ev, &v.naehte, quelle) {
        Ok(b) => {
            let auff = b.naehte.iter().filter(|n| n.auffaellig(TOLERANZ_CM)).count();
            println!(
                "# Prüfung: {} Nähte, {auff} über {TOLERANZ_CM} cm, {} Kanten doppelt; ohne Partner {} Ketten",
                b.naehte.len(),
                b.doppelt.len(),
                v.allein.len()
            );
        }
        Err(e) => println!("# Prüfung: {e}"),
    }
    for (teil, von, bis, cm) in &v.allein {
        println!("# ohne Partner: {teil}: {von} -> {bis} ({cm:.2} cm)");
    }
    println!("# von innen gezeichnet (aus den Nähten): {}", if v.gespiegelt.is_empty() { "keins".to_string() } else { v.gespiegelt.join(", ") });
    if v.widersprueche > 0 {
        println!("# {} Nähte ohne passende Spiegelung (verdreht)", v.widersprueche);
    }
    if let Some(g) = gegen {
        let soll = match std::fs::read_to_string(g).map_err(|e| e.to_string()).and_then(|t| lies_nahtliste(&t).map_err(|e| e.to_string())) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", g.display());
                return ExitCode::FAILURE;
            }
        };
        // Spiegelung der Teile aus der Lagedatei neben der Soll-Liste.
        let gespiegelt: Vec<String> = std::fs::read_to_string(g.with_extension("lage"))
            .ok()
            .and_then(|t| klon3d_sim::platzierung::lies_lage(&t).ok())
            .map(|l| l.angaben.into_iter().filter(|a| a.gespiegelt).map(|a| a.teil).collect())
            .unwrap_or_default();
        for (wer, liste, sp) in [("Soll", &soll, &gespiegelt), ("Vorschlag", &v.naehte, &v.gespiegelt)] {
            if let Ok(f) = verdrehte_naehte(ev, quelle, liste, sp) {
                for (k, grund) in f {
                    println!("# verdreht ({wer}): {k}: {grund}");
                }
            }
        }
        // Spiegelung gegen die Lagedatei (nur wenn es eine gibt).
        if g.with_extension("lage").exists() {
            // Lagenamen wie in der Nahtliste: voll oder der Teil nach dem letzten „ - “.
            let gleich = |voll: &str, kurz: &str| voll == kurz || voll.ends_with(&format!(" - {kurz}"));
            let mut anders: Vec<String> =
                v.gespiegelt.iter().filter(|s| !gespiegelt.iter().any(|g| gleich(s, g))).map(|s| format!("{s} (nur Vorschlag)")).collect();
            anders.extend(gespiegelt.iter().filter(|g| !v.gespiegelt.iter().any(|s| gleich(s, g))).map(|s| format!("{s} (nur Lage)")));
            println!("# Spiegelung gegen {}: {}", g.with_extension("lage").display(), if anders.is_empty() { "gleich".to_string() } else { anders.join(", ") });
        }
        match vergleiche(ev, quelle, &soll, &v.naehte) {
            Ok(c) => {
                println!(
                    "# gegen {}: {} genähte Kanten, richtig {} ({:.0} %, davon verdreht {}), falsch {}, fehlend {}, zu viel {}",
                    g.display(),
                    c.soll,
                    c.richtig,
                    100.0 * c.richtig as f64 / c.soll.max(1) as f64,
                    c.verdreht,
                    c.falsch,
                    c.fehlend,
                    c.zuviel
                );
                for b in &c.beispiele {
                    println!("#   {b}");
                }
            }
            Err(e) => println!("# Vergleich: {e}"),
        }
    }
    ExitCode::SUCCESS
}

fn befehl_naehte(args: &[String]) -> ExitCode {
    use klon3d_pattern::kanten::Kurvenquelle;
    use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte, MIN_SEITE_CM, TOLERANZ_CM};

    const AUFRUF: &str = "Aufruf: klon3d naehte <muster.sm2d> [--liste <datei.naehte>] [--kanten] \
                          [--zeichnung] [--toleranz <cm>] [--vorschlag [--schreiben <datei>] [--gegen <datei.naehte>]]";
    let mut muster_pfad: Option<PathBuf> = None;
    let mut liste: Option<PathBuf> = None;
    let (mut vorschlag, mut schreiben, mut gegen): (bool, Option<PathBuf>, Option<PathBuf>) = (false, None, None);
    let mut mit_kanten = false;
    let mut quelle = Kurvenquelle::Seamly;
    let mut toleranz = TOLERANZ_CM;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--liste" | "--toleranz" | "--schreiben" | "--gegen" if i + 1 >= args.len() => {
                eprintln!("{} braucht einen Wert\n{AUFRUF}", args[i]);
                return ExitCode::FAILURE;
            }
            "--liste" => {
                i += 1;
                liste = Some(PathBuf::from(&args[i]));
            }
            "--toleranz" => {
                i += 1;
                match args[i].replace(',', ".").parse::<f64>() {
                    Ok(t) if t >= 0.0 => toleranz = t,
                    _ => {
                        eprintln!("Toleranz {:?} ist keine Zahl ≥ 0\n{AUFRUF}", args[i]);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--kanten" => mit_kanten = true,
            "--vorschlag" => vorschlag = true,
            "--schreiben" => {
                i += 1;
                schreiben = Some(PathBuf::from(&args[i]));
            }
            "--gegen" => {
                i += 1;
                gegen = Some(PathBuf::from(&args[i]));
            }
            "--zeichnung" => quelle = Kurvenquelle::Zeichnung,
            a if a.starts_with("--") => {
                eprintln!("unbekannte Option {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
            a if muster_pfad.is_none() => muster_pfad = Some(PathBuf::from(a)),
            a => {
                eprintln!("überzähliges Argument {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }
    let Some(muster_pfad) = muster_pfad else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };
    let Some((muster, mess)) = laden(&muster_pfad) else {
        return ExitCode::FAILURE;
    };
    let ev = Auswertung::new(&muster, mess);
    if vorschlag {
        return naehte_vorschlagen(&ev, &muster_pfad, quelle, schreiben.as_deref(), gegen.as_deref());
    }
    let quelle_text = match quelle {
        Kurvenquelle::Seamly => "Seamly-Zerlegung",
        Kurvenquelle::Zeichnung => "gezeichnete Polylinie",
    };
    println!("{}", muster_pfad.display());
    println!("  Kurven: {quelle_text}, Längen in cm");

    // Ohne ausdrückliche Liste: `<muster>.naehte` daneben.
    let liste_pfad = liste.clone().unwrap_or_else(|| muster_pfad.with_extension("naehte"));
    let liste_da = liste_pfad.is_file();
    if liste.is_some() && !liste_da {
        eprintln!("Nahtliste fehlt: {}", liste_pfad.display());
        return ExitCode::FAILURE;
    }

    if mit_kanten || !liste_da {
        for t in muster.pieces() {
            println!();
            let rand = match ev.teilrand(t, quelle) {
                Ok(r) => r,
                Err(e) => {
                    println!("  {}: {e}", anzeige(t));
                    continue;
                }
            };
            println!(
                "  {} — {} Kanten, Umfang {:.4}",
                rand.teil,
                rand.kanten.len(),
                rand.in_cm(rand.umfang())
            );
            let zugabe = |z: Option<f64>| z.map_or("-".to_string(), |w| format!("{:.2}", rand.in_cm(w)));
            for (k, kante) in rand.kanten.iter().enumerate() {
                let (a, b) = (&rand.punktknoten[kante.von], &rand.punktknoten[kante.bis]);
                let (za, zb) = rand.zugabe(k);
                println!(
                    "    {:<12} → {:<12} {:>9.4}  Zugabe {} / {}{}",
                    format!("{}{}", a.name, if a.kerbe { "*" } else { "" }),
                    format!("{}{}", b.name, if b.kerbe { "*" } else { "" }),
                    rand.in_cm(kante.laenge),
                    zugabe(za),
                    zugabe(zb),
                    if rand.ist_frei(k) { "  frei" } else { "" }
                );
            }
        }
        println!();
        println!("  * = Passzeichen; Zugabe nach dem Anfang / vor dem Ende der Kante");
    }
    if !liste_da {
        println!();
        println!("  keine Nahtliste ({}) — nur die Randkanten", liste_pfad.display());
        return if mit_kanten { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    let text = match std::fs::read_to_string(&liste_pfad) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", liste_pfad.display());
            return ExitCode::FAILURE;
        }
    };
    let defs = match lies_nahtliste(&text) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}: {e}", liste_pfad.display());
            return ExitCode::FAILURE;
        }
    };
    let bericht = match pruefe_naehte(&ev, &defs, quelle) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}: {e}", liste_pfad.display());
            return ExitCode::FAILURE;
        }
    };

    println!();
    println!("  Nahtliste {} ({} Nähte)", liste_pfad.display(), bericht.naehte.len());
    println!();
    println!("  {:<6} {:<50} {:>9} {:>9} {:>8}", "Naht", "Bezeichnung", "L_A", "L_B", "Δ");
    for n in &bericht.naehte {
        println!(
            "  {:<6} {} {:>9.4} {:>9.4} {:>+8.4}{}",
            n.kennung,
            spalte(&n.name, 50),
            n.a.laenge_cm,
            n.b.laenge_cm,
            // −0,0000 nicht anzeigen
            if n.diff_cm.abs() < 5e-5 { 0.0 } else { n.diff_cm },
            if n.a.laenge_cm < MIN_SEITE_CM || n.b.laenge_cm < MIN_SEITE_CM {
                "  SEITE OHNE LÄNGE"
            } else if n.auffaellig(toleranz) {
                "  ÜBER"
            } else {
                ""
            }
        );
    }

    println!();
    println!("  Passzeichen (gegenseitig nächste bis 1 cm, Abstand vom Nahtanfang):");
    let kerben: Vec<_> = bericht
        .naehte
        .iter()
        .filter_map(|n| n.groesste_kerbabweichung().map(|p| (n, p)))
        .collect();
    match kerben.iter().max_by(|x, y| x.1.diff_cm.abs().total_cmp(&y.1.diff_cm.abs())) {
        Some((n, p)) => println!(
            "    {} Paare, größte Abweichung {} {}/{} {:+.4}",
            bericht.naehte.iter().map(|n| n.kerbpaare.len()).sum::<usize>(),
            n.kennung,
            p.a.name,
            p.b.name,
            p.diff_cm
        ),
        None => println!("    keine Paare"),
    }
    for n in &bericht.naehte {
        for (seite, m) in &n.kerben_allein {
            println!("    {} Seite {seite}: {} bei {:.4} ohne Gegenstück", n.kennung, m.name, m.abstand_cm);
        }
    }

    println!();
    println!("  Abdeckung:");
    if bericht.doppelt.is_empty() {
        println!("    keine Kante in zwei Nähten");
    }
    for h in &bericht.doppelt {
        println!("    DOPPELT {}: {} → {} in {}", h.teil, h.von, h.bis, h.naehte.join(", "));
    }
    for h in &bericht.zugabe_ohne_naht {
        println!("    Zugabe ohne Naht: {}: {} → {} ({:.4})", h.teil, h.von, h.bis, h.laenge_cm);
    }
    for h in &bericht.genaeht_ohne_zugabe {
        println!(
            "    genäht ohne Zugabe: {}: {} → {} in {}",
            h.teil,
            h.von,
            h.bis,
            h.naehte.join(", ")
        );
    }
    for f in &bericht.teile_fehler {
        println!("    nicht auflösbar: {f}");
    }

    println!();
    let ueber = bericht.ueber(toleranz);
    if let Some(n) = bericht.groesste_abweichung() {
        println!(
            "  größte Abweichung: {} Δ {:+.4} ({:.4} / {:.4})",
            n.kennung, n.diff_cm, n.a.laenge_cm, n.b.laenge_cm
        );
    }
    let ok = bericht.bestanden(toleranz);
    println!(
        "  Abnahme |Δ| ≤ {toleranz} cm: {} ({} von {} Nähten in der Toleranz{})",
        if ok { "bestanden" } else { "NICHT bestanden" },
        bericht.naehte.len() - ueber.len(),
        bericht.naehte.len(),
        if bericht.naehte.is_empty() {
            ", Liste leer"
        } else if bericht.doppelt.is_empty() {
            ""
        } else {
            ", Kanten doppelt belegt"
        }
    );
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ───────────────────────────────────────────────────────────── netz ──

fn befehl_netz(args: &[String]) -> ExitCode {
    use klon3d_mesh::{ausgabe, pruefung, vernetze, Parameter, Stellenart};
    use klon3d_pattern::kanten::Kurvenquelle;
    use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};

    const AUFRUF: &str = "Aufruf: klon3d netz <muster.sm2d> [--liste <datei.naehte>] [--ohne-naehte] \
                          [--kante <mm>] [--winkel <grad>] [--fang <mm>] [--zeichnung] [--svg <datei>] \
                          [--nebeneinander] [--obj <datei>] [--paare <datei>]";
    let mut p = Parameter::default();
    let mut muster_pfad: Option<PathBuf> = None;
    let (mut liste, mut svg, mut obj, mut paare): (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) =
        (None, None, None, None);
    let (mut ohne_naehte, mut nebeneinander) = (false, false);
    let mut quelle = Kurvenquelle::Seamly;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--liste" | "--svg" | "--obj" | "--paare" | "--kante" | "--winkel" | "--fang" => {
                let Some(w) = args.get(i + 1) else {
                    eprintln!("{a} braucht einen Wert\n{AUFRUF}");
                    return ExitCode::FAILURE;
                };
                // Fang 0 heißt CLOs reine Anteilsregel; alles andere muss > 0 sein.
                let zahl = w
                    .replace(',', ".")
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite() && (*x > 0.0 || (a == "--fang" && *x == 0.0)));
                match (a, zahl) {
                    ("--liste", _) => liste = Some(PathBuf::from(w)),
                    ("--svg", _) => svg = Some(PathBuf::from(w)),
                    ("--obj", _) => obj = Some(PathBuf::from(w)),
                    ("--paare", _) => paare = Some(PathBuf::from(w)),
                    (_, None) => {
                        eprintln!("{a} {w:?} ist keine Zahl > 0\n{AUFRUF}");
                        return ExitCode::FAILURE;
                    }
                    ("--kante", Some(x)) => p.kante_mm = x,
                    // Ruppert endet sicher bis 20,7°, in der Praxis bis gut 30°.
                    ("--winkel", Some(x)) if x > 34.0 => {
                        eprintln!("--winkel {x}: über 34° endet die Verfeinerung nicht sicher");
                        return ExitCode::FAILURE;
                    }
                    ("--winkel", Some(x)) => p.winkel_grad = x,
                    (_, Some(x)) => p.fang_mm = x,
                }
                i += 1;
            }
            "--ohne-naehte" => ohne_naehte = true,
            "--nebeneinander" => nebeneinander = true,
            "--zeichnung" => quelle = Kurvenquelle::Zeichnung,
            a if a.starts_with("--") => {
                eprintln!("unbekannte Option {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
            a if muster_pfad.is_none() => muster_pfad = Some(PathBuf::from(a)),
            a => {
                eprintln!("überzähliges Argument {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }
    let Some(muster_pfad) = muster_pfad else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };
    let Some((muster, mess)) = laden(&muster_pfad) else {
        return ExitCode::FAILURE;
    };
    let ev = Auswertung::new(&muster, mess);
    println!("{}", muster_pfad.display());
    println!(
        "  Kurven: {}; Kante {} mm, Mindestwinkel {}°, Rand auf {} mm, Fang {} mm",
        match quelle {
            Kurvenquelle::Seamly => "Seamly-Zerlegung",
            Kurvenquelle::Zeichnung => "gezeichnete Polylinie",
        },
        p.kante_mm,
        p.winkel_grad,
        p.sehne_mm,
        p.fang_mm
    );

    // Ohne ausdrückliche Liste: `<muster>.naehte` daneben, sonst nur die Netze.
    let liste_pfad = liste.clone().unwrap_or_else(|| muster_pfad.with_extension("naehte"));
    let naehte = if ohne_naehte || (liste.is_none() && !liste_pfad.is_file()) {
        println!("  ohne Nahtliste — nur die Netze");
        Vec::new()
    } else {
        let defs = std::fs::read_to_string(&liste_pfad)
            .map_err(|e| e.to_string())
            .and_then(|t| lies_nahtliste(&t).map_err(|e| e.to_string()));
        let bericht = defs.and_then(|d| pruefe_naehte(&ev, &d, quelle).map_err(|e| e.to_string()));
        match bericht {
            Ok(b) => {
                println!("  Nahtliste {} ({} Nähte)", liste_pfad.display(), b.naehte.len());
                b.naehte
            }
            Err(e) => {
                eprintln!("{}: {e}", liste_pfad.display());
                return ExitCode::FAILURE;
            }
        }
    };

    let start = std::time::Instant::now();
    let netz = match vernetze(&ev, quelle, &naehte, &p) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let dauer = start.elapsed();
    let bericht = pruefung::pruefe(&netz);

    println!();
    println!(
        "  {:<34} {:>6} {:>5} {:>6} {:>6} {:>11} {:>7} {:>7} {:>8}",
        "Teil", "Punkte", "Rand", "Dreie.", "min∠", "Kante Med/Max", "Sehne", "Länge", "Fläche"
    );
    for t in &bericht.teile {
        let kurz = t.teil.rsplit(" - ").next().unwrap_or(&t.teil);
        println!(
            "  {} {:>6} {:>5} {:>6} {:>5.1}° {:>5.1}/{:>5.1} {:>7.4} {:>7.4} {:>+7.3}%",
            spalte(kurz, 34),
            t.punkte,
            t.randpunkte,
            t.dreiecke,
            t.min_winkel,
            t.kante_median,
            t.kante_max,
            t.sehne_max,
            t.laenge_max,
            100.0 * (t.flaeche_netz - t.flaeche_kontur) / t.flaeche_kontur.max(1e-9)
        );
    }
    println!("  Kante in mm; Sehne = größter Abstand Randkante–Netzrand, Länge = größte Längenabweichung");
    println!("  einer Randkante (mm); Fläche = Netz gegen Nahtlinie");
    for f in &netz.fehler {
        println!("  nicht vernetzt: {f}");
    }

    if !netz.naehte.is_empty() {
        println!();
        println!("  {:<6} {:<24} {:>5}  {:<28} {}", "Naht", "Punkte je Teilnaht", "Paare", "größter Versatz (mm)", "nur A/B");
        for (n, np) in netz.naehte.iter().zip(&bericht.naehte) {
            let je: Vec<String> = np.punkte.iter().map(|(a, _)| a.to_string()).collect();
            let nur = |art| n.stellen.iter().filter(|s| s.art == art).count();
            println!(
                "  {:<6} {} {:>5}  {} {}/{}",
                n.kennung,
                spalte(&je.join("+"), 24),
                np.paare,
                spalte(
                    &if np.versatz_max > 0.0 { format!("{:.3} {}", np.versatz_max, np.versatz_stelle) } else { "-".into() },
                    28
                ),
                nur(Stellenart::NurA),
                nur(Stellenart::NurB),
            );
        }
        println!("  Teilnähte teilen ihre Endpunkte; Versatz = Abweichung von CLOs reiner Anteilsregel an einer");
        println!("  zusammengelegten Stelle; nur A/B = Punktknoten ohne Gegenstück (bekommen einen eigenen Randpunkt)");
    }

    let summe = |f: fn(&pruefung::Teilpruefung) -> usize| bericht.teile.iter().map(f).sum::<usize>();
    println!();
    println!(
        "  {} Teile, {} Punkte, {} Dreiecke; {} Runden Teilen–Neuvernetzen, {:.0} ms",
        bericht.teile.len(),
        summe(|t| t.punkte),
        summe(|t| t.dreiecke),
        netz.runden,
        dauer.as_secs_f64() * 1000.0
    );

    let schreib = |ziel: &Option<PathBuf>, text: String, was: &str| -> bool {
        let Some(z) = ziel else { return true };
        match std::fs::write(z, text) {
            Ok(()) => {
                println!("  {was}: {}", z.display());
                true
            }
            Err(e) => {
                eprintln!("{}: {e}", z.display());
                false
            }
        }
    };
    let geschrieben = schreib(&svg, ausgabe::svg(&netz, nebeneinander), "SVG")
        & schreib(&obj, ausgabe::obj(&netz), "OBJ")
        & schreib(&paare, ausgabe::paarliste(&netz), "Paare");

    if netz.offene_teilungen > 0 {
        println!("  {} Randteilungen nach {} Runden offen", netz.offene_teilungen, netz.runden);
    }
    let verstoesse = bericht.verstoesse(&p);
    for v in &verstoesse {
        println!("  VERSTOSS {v}");
    }
    let ok = verstoesse.is_empty() && netz.offene_teilungen == 0 && geschrieben;
    println!("  Abnahme: {}", if ok { "bestanden" } else { "NICHT bestanden" });
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ─────────────────────────────────────────────────────────── roehre ──

const ROEHRE_AUFRUF: &str = "Aufruf: klon3d roehre [--material <profil|datei.zfab|stamm>] [--reduktion <%>] \
                             [--kante <mm>] [--spalt] [--rutsch <s>] [--obj <datei.obj>] [--gpu]";

/// Die ausgewerteten Optionen von `roehre`, ohne dass etwas gerechnet wurde.
#[derive(Debug)]
struct RoehreAufruf {
    r: klon3d_sim::roehre::Roehre,
    /// Step 4b: the implicit step on the GPU instead of the XPBD reference.
    gpu: bool,
    rutsch: Option<f64>,
    obj: Option<PathBuf>,
}

/// Reads `roehre`'s options. Separate from the run so that `--material` can be
/// checked without twelve minutes of solver: `anziehen` and the tube resolve a
/// fabric through the same [`crate::stoffwahl::stoff_waehlen`], and this is the
/// place where the tube could silently fall back to 0.4 mm again.
fn roehre_optionen(args: &[String]) -> Result<RoehreAufruf, String> {
    let mut r = klon3d_sim::roehre::Roehre::default();
    let mut gpu = false;
    let mut rutsch: Option<f64> = None;
    let mut obj: Option<PathBuf> = None;
    let zahl = |a: &str| a.replace(',', ".").parse::<f64>().ok();
    let mut i = 0;
    while i < args.len() {
        let wert = args.get(i + 1).map(String::as_str);
        // Schalter ohne Wert rücken um 1 vor, Optionen mit Wert um 2.
        let mut schritt = 2;
        let ok = match args[i].as_str() {
            // Ein unbekannter Name ist ein Fehler, keine stille 0,4-mm-Röhre.
            "--material" => {
                let Some(w) = wert else { return Err(format!("--material ohne Wert\n{ROEHRE_AUFRUF}")) };
                r.material = crate::stoffwahl::stoff_waehlen(w)?;
                true
            }
            "--reduktion" => wert.and_then(zahl).filter(|v| (0.0..90.0).contains(v)).map(|v| r.reduktion = v / 100.0).is_some(),
            "--kante" => wert.and_then(zahl).filter(|v| *v >= 2.0).map(|v| r.kante_mm = v).is_some(),
            "--rutsch" => wert.and_then(zahl).filter(|v| *v > 0.0).map(|v| rutsch = Some(v)).is_some(),
            "--obj" => wert.map(|w| obj = Some(PathBuf::from(w))).is_some(),
            "--spalt" => {
                r.anfang_mit_spalt = true;
                schritt = 1;
                true
            }
            "--gpu" => {
                gpu = true;
                schritt = 1;
                true
            }
            _ => false,
        };
        if !ok {
            return Err(format!("unbekannte oder unvollständige Option {:?}\n{ROEHRE_AUFRUF}", args[i]));
        }
        i += schritt;
    }
    Ok(RoehreAufruf { r, gpu, rutsch, obj })
}

fn befehl_roehre(args: &[String]) -> ExitCode {
    use klon3d_sim::roehre::{bis_zur_ruhe, param_ruhe};
    use klon3d_sim::{ausgabe, CpuReferenz};

    let RoehreAufruf { r, gpu, rutsch, obj } = match roehre_optionen(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let mut s = match r.szene(param_ruhe()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Röhre: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Röhre {} × {} mm um Zylinder R {} mm, {} ({} mm), Reduktion {:.1} %{}",
        (r.breite_m() * 1e3).round(),
        (r.hoehe_m * 1e3).round(),
        r.zylinder_radius_m * 1e3,
        r.material.name,
        crate::stoffwahl::dicke_gedruckt(r.material.dicke_m),
        r.reduktion * 100.0,
        if r.anfang_mit_spalt { ", Beginn mit offenem Nahtspalt" } else { "" }
    );
    println!(
        "  Netz {} Punkte, {} Dreiecke, {} Kanten, {} Nahtpaare, {} Biegekanten; {} Unterschritte je Bild (Weichheitsbedingung)",
        s.x.len(),
        s.dreiecke.len(),
        s.abstand.len(),
        s.naht.len(),
        s.biegung.len(),
        s.param.unterschritte
    );
    let mut rw: Box<dyn klon3d_sim::Rechenweg> = if gpu {
        let loeser = match klon3d_sim::GpuLoeser::neu() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("GPU: {e} (ohne --gpu rechnet die XPBD-Referenz)");
                return ExitCode::FAILURE;
            }
        };
        println!("  Rechenweg    GPU (f32, wgpu) auf {}, impliziter Schritt", loeser.geraet());
        Box::new(klon3d_sim::ImplizitCg::mit_loeser(klon3d_sim::ImplizitParameter::default(), Box::new(loeser)))
    } else {
        Box::new(CpuReferenz::neu())
    };
    let t0 = std::time::Instant::now();
    let lauf = bis_zur_ruhe(&mut s, rw.as_mut(), 1200, 1e-4, 10);
    let dauer = t0.elapsed().as_secs_f64();
    let e = r.auswerten(&s);
    println!(
        "  {} nach {} Bildern ({:.2} s simuliert, {:.1} s gerechnet, {:.0} µs je Unterschritt)",
        if lauf.ruhig { "in Ruhe" } else { "NICHT in Ruhe" },
        lauf.bilder,
        s.zeit,
        dauer,
        dauer / (lauf.bilder * s.param.unterschritte) as f64 * 1e6
    );
    println!(
        "  Umfang {:.2} mm, Dehnung {:+.3} % (Rechnung {:+.3} %, Δ {:+.3} %-Punkte)",
        e.umfang_m * 1e3,
        e.dehnung * 100.0,
        e.dehnung_soll * 100.0,
        (e.dehnung - e.dehnung_soll) * 100.0
    );
    println!(
        "  Dreiecke: Median {:+.3} %, 95 % {:+.3} %, größte {:+.3} %; Nahtspalt {:.4} mm; Abheben {:+.3} mm",
        e.dehnung_median * 100.0,
        e.dehnung_p95 * 100.0,
        e.dehnung_max * 100.0,
        e.nahtspalt_m * 1e3,
        e.abheben_m * 1e3
    );
    let abnahme = lauf.ruhig
        && (e.dehnung - e.dehnung_soll).abs() <= 0.005
        && e.nahtspalt_m < 0.5e-3
        && (r.reduktion == 0.0 || e.abheben_m < 0.5e-3);
    println!("  Abnahme 4a: {}", if abnahme { "bestanden" } else { "NICHT bestanden" });

    if let Some(pfad) = &obj {
        let csv = pfad.with_extension("dehnung.csv");
        if let Err(e) = std::fs::write(pfad, ausgabe::obj(&s)).and_then(|_| std::fs::write(&csv, ausgabe::dehnung_csv(&s))) {
            eprintln!("{}: {e}", pfad.display());
            return ExitCode::FAILURE;
        }
        println!("  geschrieben: {} und {}", pfad.display(), csv.display());
    }

    if let Some(sek) = rutsch {
        match r.rutschprobe(&mut CpuReferenz::neu(), sek) {
            Ok(p) => println!(
                "  Rutschprobe: Halteverhältnis (Coulomb) {:.3}, mit anliegender Last {:.3} → {}; Weg in {:.2} s {:.2} mm (Coulomb {:.2} mm)",
                p.halteverhaeltnis,
                p.halteverhaeltnis_gemessen,
                if p.halteverhaeltnis_gemessen >= 1.0 { "hält" } else { "rutscht" },
                p.sekunden,
                p.weg_m * 1e3,
                p.weg_soll_m * 1e3
            ),
            Err(e) => {
                eprintln!("Rutschprobe: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    if abnahme {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ────────────────────────────────────────────────────────── koerper ──

fn befehl_koerper(args: &[String]) -> ExitCode {
    use klon3d_koerper::koerper::{aus_smis_mit, makehuman_weg, ordner};

    const AUFRUF: &str = "Aufruf: klon3d koerper <massdatei.smis> [--out <koerper.obj>] [--ordner <Resizing>] [--wie-clo] [--makehuman]";
    let mut smis_pfad = None;
    let mut aus = None;
    let mut resizing = ordner();
    let mut nachformen = true;
    let mut nur_makehuman = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            // the body exactly as the reference chain shapes it, without
            // klon3d's post-shaping between waist and hip
            "--wie-clo" => {
                nachformen = false;
                i += 1;
            }
            // the MakeHuman path even where CLO's resizing data exists
            "--makehuman" => {
                nur_makehuman = true;
                i += 1;
            }
            o @ ("--out" | "--ordner") => {
                let Some(w) = args.get(i + 1) else {
                    eprintln!("{o} ohne Pfad
{AUFRUF}");
                    return ExitCode::FAILURE;
                };
                if o == "--out" {
                    aus = Some(PathBuf::from(w));
                } else {
                    resizing = PathBuf::from(w);
                }
                i += 2;
            }
            a if a.starts_with("--") => {
                eprintln!("unbekannte Option {a:?}
{AUFRUF}");
                return ExitCode::FAILURE;
            }
            a if smis_pfad.is_none() => {
                smis_pfad = Some(PathBuf::from(a));
                i += 1;
            }
            a => {
                eprintln!("unerwartetes Argument {a:?}
{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(smis_pfad) = smis_pfad else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };
    let smis = match klon3d_pattern::lies_smis(&smis_pfad) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", smis_pfad.display());
            return ExitCode::FAILURE;
        }
    };

    let t0 = std::time::Instant::now();
    let gebaut = if nur_makehuman {
        match klon3d_koerper::koerper_quelle::ordner() {
            Some(o) => makehuman_weg(&o, &|n| smis.wert(n)),
            None => Err(format!(
                "{} nicht gefunden (MakeHuman-Assets fehlen)",
                klon3d_koerper::koerper_quelle::ORDNER
            )),
        }
    } else {
        aus_smis_mit(&resizing, &|n| smis.wert(n), nachformen)
    };
    let k = match gebaut {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Körper nicht formbar: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", smis_pfad.display());
    println!("  Resizing     {}", resizing.display());
    println!("  Weg          {} ({:.2} s)", k.weg.name(), t0.elapsed().as_secs_f64());
    for m in &k.meldungen {
        println!("    {m}");
    }
    let (lo, hi) = k.netz.hoehe().unwrap_or((0.0, 0.0));
    println!(
        "  body         {} Punkte, {} Dreiecke, {:.2} cm hoch",
        k.netz.punkte.len(),
        k.netz.dreiecke.len(),
        (hi - lo) / 10.0
    );
    if let Some(e) = &k.kette {
        let m = &e.werte;
        let soll = |n: &str| smis.wert(n).map_or("–".to_string(), |v| format!("{v:.2}"));
        if k.nachformung.is_some() {
            println!("  Maße der Formungskette, vor der Nachformung (cm, Soll aus der Maßdatei):");
        } else {
            println!("  Maße am Körper (cm, Soll aus der Maßdatei):");
        }
        for (was, j, name) in [
            ("Höhe", 0, "height"),
            ("Brust", 2, "bust_circ"),
            ("Unterbrust", 5, "lowbust_circ"),
            ("Taille", 6, "waist_circ"),
            ("Hüfte", 8, "hip_circ"),
        ] {
            println!("    {was:<12} {:8.3}  Soll {}", m[j], soll(name));
        }
        println!(
            "    {:<12} {:8.3}  Soll {:.3} (an CLOs Lage abgeleitet)",
            "HighHip", m[7], e.eingabe[7]
        );
    }
    if let Some(aus) = aus {
        if let Err(e) = klon3d_koerper::schreibe_obj(&aus, &k.netz, 1.0) {
            eprintln!("{}: {e}", aus.display());
            return ExitCode::FAILURE;
        }
        println!("  geschrieben  {} (mm)", aus.display());
    }
    ExitCode::SUCCESS
}

// ─────────────────────────────────────────────────────────── skelett ──

fn befehl_skelett(args: &[String]) -> ExitCode {
    use klon3d_koerper::skelett;

    const AUFRUF: &str = "Aufruf: klon3d skelett [--out <koerper.obj>] [--drehung <grad>] [--ordner <makehuman>] [--hoehe <cm>]";
    let mut aus = None;
    let mut ordner = klon3d_koerper::koerper_quelle::ordner();
    let mut drehung = 90.0f64;
    let mut hoehe_cm = 160.0f64;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            o @ ("--out" | "--ordner" | "--drehung" | "--hoehe") => {
                let Some(w) = args.get(i + 1) else {
                    eprintln!("{o} ohne Wert\n{AUFRUF}");
                    return ExitCode::FAILURE;
                };
                match o {
                    "--out" => aus = Some(PathBuf::from(w)),
                    "--ordner" => ordner = Some(PathBuf::from(w)),
                    "--drehung" => match w.parse::<f64>() {
                        Ok(v) => drehung = v,
                        Err(_) => {
                            eprintln!("{w:?} ist kein Winkel\n{AUFRUF}");
                            return ExitCode::FAILURE;
                        }
                    },
                    _ => match w.parse::<f64>() {
                        Ok(v) if v > 0.0 => hoehe_cm = v,
                        _ => {
                            eprintln!("{w:?} ist keine Hoehe\n{AUFRUF}");
                            return ExitCode::FAILURE;
                        }
                    },
                }
                i += 2;
            }
            a => {
                eprintln!("unerwartetes Argument {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(ordner) = ordner else {
        eprintln!(
            "{} nicht gefunden (MakeHuman-Assets fehlen)",
            klon3d_koerper::koerper_quelle::ORDNER
        );
        return ExitCode::FAILURE;
    };

    let t0 = std::time::Instant::now();
    let (netz, skelett) = match skelett::baue(&ordner, hoehe_cm * 10.0) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Skelett nicht baubar: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("MakeHuman-Koerper {}", ordner.display());
    println!(
        "  {} Punkte, {} Dreiecke, {:.1} cm hoch ({:.1} s)",
        netz.punkte.len(),
        netz.dreiecke.len(),
        netz.hoehe().map_or(0.0, |(a, b)| (b - a) / 10.0),
        t0.elapsed().as_secs_f64()
    );
    println!(
        "  Skelett      {} Gelenke, {} Knochen tragen Gewichte",
        skelett.gelenke.len(),
        skelett.knochen.len()
    );
    println!(
        "  Gewichte     hoechstens {} je Punkt, Summenabweichung {:.1e}, {} Punkte ohne Gewicht",
        skelett.einfluesse_max(),
        skelett.gewichtssumme_abweichung(),
        skelett.gewichte.iter().filter(|g| g.is_empty()).count()
    );
    println!("  Radien der grossen Knochen (mm, Untergrenze {}):", skelett::RADIUS_MIN as i64);
    for n in [
        "spine-1", "spine-2", "l-clavicle", "l-shoulder", "l-elbow", "l-hand", "neck", "head-2",
    ] {
        if let Some(r) = skelett.radius(n) {
            println!("    {n:<12} {r:8.3}");
        }
    }

    let mut s = skelett.clone();
    s.hebe_arme(drehung);
    let Some(neu) = s.verforme(&netz.punkte) else {
        eprintln!("Verformung nicht moeglich (Binde-Matrix nicht starr)");
        return ExitCode::FAILURE;
    };
    let Some(ab) = skelett.abnahme(&netz.punkte, &netz.dreiecke, drehung) else {
        eprintln!("Abnahme nicht messbar");
        return ExitCode::FAILURE;
    };
    println!("  Armheben {drehung:.0} Grad:");
    println!(
        "    ferne Kanten {}: ueber 2x {}, ueber 10 mm {}, groesstes Verhaeltnis {:.3}",
        ab.fern_kanten, ab.fern_ueber_doppelt, ab.fern_ueber_10mm, ab.fern_max
    );
    println!(
        "    nahe Kanten  {}: Verhaeltnis {:.2} bis {:.2}, davon unter 0.2 {}",
        ab.nah_kanten, ab.nah_min, ab.nah_max, ab.nah_kollaps
    );
    println!(
        "    Schulter     Abstand zum Gelenk Median {:.3}, Minimum {:.3}",
        ab.schulter_median, ab.schulter_min
    );
    println!("    Binde-Pose   groesster Weg {:.1e} mm", ab.binde_abweichung);

    let bestanden = ab.bestanden();
    if let Some(aus) = aus {
        let mut n = netz.clone();
        n.punkte = neu;
        if let Err(e) = klon3d_koerper::schreibe_obj(&aus, &n, 1.0) {
            eprintln!("{}: {e}", aus.display());
            return ExitCode::FAILURE;
        }
        println!("  geschrieben  {} (mm)", aus.display());
    }
    if bestanden {
        println!("  Abnahme      bestanden");
        ExitCode::SUCCESS
    } else {
        println!("  Abnahme      NICHT bestanden");
        ExitCode::FAILURE
    }
}

// ─────────────────────────────────────────────────────────── punkte ──

fn befehl_punkte(args: &[String]) -> ExitCode {
    let Some(pfad) = pfad(args) else {
        eprintln!("Aufruf: klon3d punkte <muster.sm2d> [--json] [--vergleich <dump.txt>]");
        return ExitCode::FAILURE;
    };
    let als_json = args.iter().any(|a| a == "--json");
    let vergleich = args
        .windows(2)
        .find(|w| w[0] == "--vergleich")
        .map(|w| PathBuf::from(&w[1]));

    let muster = match read_path(&pfad) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", pfad.display());
            return ExitCode::FAILURE;
        }
    };

    // Messdatei: der Pfad im Muster ist relativ zur Musterdatei.
    let messpfad = muster
        .measurements
        .as_ref()
        .map(|m| {
            let p = Path::new(m);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                pfad.parent().unwrap_or(Path::new(".")).join(p)
            }
        });

    let mess = match &messpfad {
        Some(p) if p.is_file() => match lies_messungen(p, &muster.unit) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        Some(p) => {
            eprintln!("Massdatei fehlt: {}", p.display());
            return ExitCode::FAILURE;
        }
        None => {
            eprintln!("Muster nennt keine Massdatei");
            return ExitCode::FAILURE;
        }
    };

    let ev = Auswertung::new(&muster, mess);

    // Alle benannten Punkte rechnen.
    let mut namen: BTreeMap<String, String> = BTreeMap::new();
    for b in &muster.draft_blocks {
        if let Some(c) = &b.calculation {
            for p in &c.points {
                if let Some(n) = &p.name {
                    namen.insert(n.clone(), p.id.clone());
                }
            }
        }
    }

    // Dazu die benannten Punkte ohne eigenen `<point>`: Kopien aus
    // Operationen und die Punkte eines `trueDarts`.
    for (n, id) in ev.abgeleitete_punkte() {
        namen.entry(n).or_insert(id);
    }

    let mut geordnet: Vec<(&String, &String)> = namen.iter().collect();
    geordnet.sort();

    let mut ergebnis: Vec<(String, String, Result<(f64, f64), String>)> = Vec::new();
    for (name, id) in &geordnet {
        let r = ev
            .punkt(id)
            .map(|p| (p.x, p.y))
            .map_err(|e| e.to_string());
        ergebnis.push(((*name).clone(), (*id).clone(), r));
    }
    let ok = ergebnis.iter().filter(|(_, _, r)| r.is_ok()).count();
    let schlecht = ergebnis.len() - ok;

    if als_json {
        println!("{{");
        println!("  \"muster\": {},", json_str(&pfad.display().to_string()));
        println!("  \"einheit\": {},", json_str(&muster.unit));
        println!("  \"punkte\": [");
        for (i, (name, id, r)) in ergebnis.iter().enumerate() {
            let k = if i + 1 == ergebnis.len() { "" } else { "," };
            match r {
                Ok((x, y)) => println!(
                    "    {{\"name\": {}, \"id\": {}, \"x\": {x:.4}, \"y\": {y:.4}}}{k}",
                    json_str(name),
                    json_str(id)
                ),
                Err(e) => println!(
                    "    {{\"name\": {}, \"id\": {}, \"fehler\": {}}}{k}",
                    json_str(name),
                    json_str(id),
                    json_str(e)
                ),
            }
        }
        println!("  ]");
        println!("}}");
    } else {
        println!("{}", pfad.display());
        println!("  {} von {} benannten Punkten berechnet", ok, ergebnis.len());
        let mut nach_typ: BTreeMap<String, usize> = BTreeMap::new();
        for (_, id, _) in &ergebnis {
            let t = muster
                .draft_blocks
                .iter()
                .filter_map(|b| b.calculation.as_ref())
                .flat_map(|c| c.points.iter())
                .find(|p| &p.id == id)
                .map(|p| p.kind.clone())
                .unwrap_or_default();
            *nach_typ.entry(t).or_default() += 1;
        }
        for (t, n) in &nach_typ {
            println!("    {t:<24} {n:>5}");
        }
        if schlecht > 0 {
            println!();
            println!("  nicht aufloesbar ({schlecht}):");
            for (name, _, r) in ergebnis.iter().filter(|(_, _, r)| r.is_err()).take(40) {
                let grund = match r {
                    Err(e) => e,
                    Ok(_) => "",
                };
                println!("    {name:<24} {grund}");
            }
            if schlecht > 40 {
                println!("    … und {} weitere", schlecht - 40);
            }
        }
    }

    if let Some(v) = vergleich {
        return vergleiche(&ergebnis, &v, als_json);
    }
    if schlecht == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Vergleicht mit einem Dump von `schnitt.py`.
///
/// Erwartetes Format (siehe `v26_dump.txt`, `b3_punkte.txt`):
/// ```text
///   Punktname          x          y      typ
/// ```
fn vergleiche(
    ergebnis: &[(String, String, Result<(f64, f64), String>)],
    referenz: &Path,
    als_json: bool,
) -> ExitCode {
    let Ok(text) = std::fs::read_to_string(referenz) else {
        eprintln!("Referenz nicht lesbar: {}", referenz.display());
        return ExitCode::FAILURE;
    };
    let mut soll: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for zeile in text.lines() {
        let s = zeile.trim_start();
        // `  B1_RM0    10.000   -10.000   single`
        let teile: Vec<&str> = s.split_whitespace().collect();
        if teile.len() < 3 {
            continue;
        }
        // Name darf keinen Leerraum enthalten; x und y sind Zahlen.
        if let (Ok(x), Ok(y)) = (teile[teile.len() - 3].parse::<f64>(), teile[teile.len() - 2].parse::<f64>())
        {
            soll.insert(teile[0].trim_start_matches('-').to_string(), (x, y));
        } else if let (Ok(x), Ok(y)) = (teile[1].parse::<f64>(), teile[2].parse::<f64>()) {
            soll.insert(teile[0].to_string(), (x, y));
        }
    }

    let einheit = 1.0f64; // Dump und Muster in derselben Einheit
    let mut max_abw = 0.0f64;
    let mut max_weg = (String::new(), f64::MAX);
    let mut verglichen = 0usize;
    let mut fehlend = Vec::new();

    for (name, _, r) in ergebnis {
        let Ok((x, y)) = r.as_ref() else { continue };
        let Some(&(sx, sy)) = soll.get(name) else {
            fehlend.push(name.clone());
            continue;
        };
        let d = ((x - sx) * einheit).hypot(y - sy);
        verglichen += 1;
        if d > max_abw {
            max_abw = d;
            max_weg = (name.clone(), d);
        }
        if d > max_weg.1 {
            max_weg = (name.clone(), d);
        }
    }

    if als_json {
        println!("{{");
        println!("  \"referenz\": {},", json_str(&referenz.display().to_string()));
        println!("  \"verglichen\": {verglichen},");
        println!("  \"max_abweichung\": {max_abw:.6},");
        println!("  \"am_schlechtesten\": {}", json_str(&max_weg.0));
        println!("}}");
    } else {
        println!();
        println!("Vergleich mit {}", referenz.display());
        println!("  verglichen     {verglichen} Punkte");
        println!("  groesste Abw.  {:.6} Einheiten bei {}", max_abw, max_weg.0);
        if !fehlend.is_empty() {
            println!("  nur im Muster  {}", fehlend.len());
        }
        let nur_soll = soll.len().saturating_sub(verglichen);
        if nur_soll > 0 {
            println!("  nur im Dump    {nur_soll}");
        }
    }
    ExitCode::SUCCESS
}

fn befehl_umriss(args: &[String]) -> ExitCode {
    let Some(pfad) = pfad(args) else {
        eprintln!("Aufruf: klon3d umriss <muster.sm2d> [--svg <datei.svg>]");
        return ExitCode::FAILURE;
    };
    let svg_aus = args
        .windows(2)
        .find(|w| w[0] == "--svg")
        .map(|w| PathBuf::from(&w[1]));

    let Some((muster, mess)) = laden(&pfad) else {
        return ExitCode::FAILURE;
    };
    let ev = Auswertung::new(&muster, mess);

    println!("{}", pfad.display());
    let f = muster.unit_to_mm().unwrap_or(1.0);
    println!("  Einheit {} (1 Einheit = {f} mm)", muster.unit);
    println!();

    let teile = muster.pieces().to_vec();
    let mut gesamt_ungeloest = 0usize;
    let mut schranke: Option<[f64; 4]> = None;
    let mut umrisse = Vec::with_capacity(teile.len());
    // Die Zugabekontur je Teil, `None` ohne Zugabe. Getrennt von `umrisse`
    // gehalten, damit die SVG-Ausgabe erst alle Schnittlinien und dann die
    // Nahtlinien schreiben kann (die Naht soll obenauf liegen).
    let mut konturen: Vec<Option<Vec<Pt>>> = Vec::with_capacity(teile.len());

    for t in &teile {
        let (knoten, meldung) = ev.stueck_knoten(t);
        // The seam line as Seamly2D builds it on its own curve polylines, the
        // same one seams and mesh use. The drawn curves only where the piece
        // cannot be resolved: there, notches on a curve named several times
        // miss the drawing and the edge runs in a zigzag (bishop sleeve of
        // Bolero v37: 2142.7 instead of 1418.6 mm).
        let rand = ev.teilrand(t, klon3d_pattern::kanten::Kurvenquelle::Seamly).ok();
        let umriss = match &rand {
            Some(r) => r.umriss.clone(),
            None => ev.umriss_aus_knoten(&knoten),
        };
        gesamt_ungeloest += meldung.len();
        umrisse.push(umriss.clone());
        // Die Schnittlinie: die Nahtlinie, um die Zugabe ihrer Knoten
        // verschoben - dieselbe `Ecken::Eigen` wie im DXF-Export und in der
        // 2D-Ansicht. Ohne `Teilrand` (Rueckfall oben) gibt es keine Zugabe,
        // und es wird keine geraten.
        let kontur = umriss::zugabekontur(rand.as_ref());
        let schnitt = kontur.as_ref().map(|k| umriss::ringlaenge(k) * f);
        konturen.push(kontur);

        if umriss.len() < 3 {
            println!(
                "  {:<44} {:>4} Punkte  —  {} Knoten unauflösbar",
                anzeige(t),
                umriss.len(),
                meldung.len()
            );
            continue;
        }
        let xs: Vec<f64> = umriss.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = umriss.iter().map(|p| p.y).collect();
        let (x0, x1, y0, y1) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
        schranke = Some(match schranke {
            None => [x0, x1, y0, y1],
            Some(a) => [a[0].min(x0), a[1].max(x1), a[2].min(y0), a[3].max(y1)],
        });
        // Der Ring ist geschlossen: was zwischen dem letzten und dem ersten
        // Knoten liegt, ist eine echte Nahtlinie, keine Luecke. Deshalb
        // gehoert sie zur Laenge und wird nicht als Fehler gemeldet.
        let mut ring = umriss.clone();
        ring.push(*umriss.first().unwrap());
        let laenge = ev.umriss_laenge(&ring) * f;
        let schlusskante = umriss[0].dist(*umriss.last().unwrap()) * f;
        let knotenzahl = knoten.len();
        // Die Schnittlinie steht nur da, wo es eine gibt: ein Teil ohne Zugabe
        // bekommt keine Spalte (kein `0.0`).
        let schnittspalte = match schnitt {
            Some(s) => format!("  {s:>8.1} mm Schnitt"),
            None => String::new(),
        };
        println!(
            "  {:<40} {:>3} Knoten {:>5} Pkt {:>8.1} mm Naht {:>7.1} x {:>7.1} mm Schluss {:>5.1} mm{}{}",
            anzeige(t),
            knotenzahl,
            umriss.len(),
            laenge,
            (x1 - x0) * f,
            (y1 - y0) * f,
            schlusskante,
            schnittspalte,
            if meldung.is_empty() {
                String::new()
            } else {
                format!("  ({} Knoten offen)", meldung.len())
            }
        );
    }

    if gesamt_ungeloest > 0 {
        println!();
        println!("  {gesamt_ungeloest} Knoten insgesamt nicht aufloesbar");
    }

    if let (Some(ziel), Some([x0, x1, y0, y1])) = (svg_aus, schranke) {
        // SVG in Millimetern. SVG zählt y nach unten wie die Datei und die
        // Auswertung, also ohne Spiegelung — so steht das Teil wie in
        // Seamly2D. (Bis zum 19.10.2026 stand hier `d - y`: jedes Teil auf
        // dem Kopf, die Ärmelkugel unten.)
        let (mut a, mut b) = (x0.min(x1), x0.max(x1));
        let (mut c, mut d) = (y0.min(y1), y0.max(y1));
        let rand = 5.0;
        a -= rand;
        c -= rand;
        b += rand;
        d += rand;
        let breite = (b - a) * f;
        let hoehe = (d - c) * f;
        let mut s = String::new();
        s.push_str(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" \
             width=\"{breite:.3}mm\" height=\"{hoehe:.3}mm\" \
             viewBox=\"0 0 {breite:.3} {hoehe:.3}\">\n"
        ));
        // Erst die Schnittlinien, dann die Nahtlinien: die Naht soll obenauf
        // liegen. Ein Teil ohne Zugabe bekommt keine zweite Linie - seine
        // Kontur waere nur eine Kopie der Naht.
        let mut grau = String::new();
        for kontur in konturen.iter().flatten() {
            if kontur.len() < 3 {
                continue;
            }
            grau.push_str(&format!(
                "    <path d=\"{}\"/>\n",
                umriss::pfad(kontur, a, c, f)
            ));
        }
        if !grau.is_empty() {
            s.push_str(&format!(
                "  <!-- Schnittlinien, Einheit mm, vor der Naht -->\n  <g fill=\"none\" \
                 stroke=\"#808080\" stroke-width=\"0.2\">\n{grau}  </g>\n"
            ));
        }
        s.push_str(&format!(
            "  <!-- Nahtlinien, Einheit mm, y nach unten wie in der .sm2d -->\n  <g fill=\"none\" \
             stroke=\"black\" stroke-width=\"0.3\">\n"
        ));
        for umriss in &umrisse {
            if umriss.len() < 3 {
                continue;
            }
            let d2 = umriss::pfad(umriss, a, c, f);
            s.push_str(&format!("    <path d=\"{d2}\"/>\n"));
        }
        s.push_str("  </g>\n</svg>\n");
        match std::fs::write(&ziel, s) {
            Ok(()) => {
                println!();
                println!("SVG geschrieben: {}", ziel.display());
            }
            Err(e) => {
                eprintln!("SVG nicht schreibbar ({}): {e}", ziel.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

// ────────────────────────────────────────────────────────────── dxf ──

fn befehl_dxf(args: &[String]) -> ExitCode {
    const AUFRUF: &str = "Aufruf: klon3d dxf <muster.sm2d> <aus.dxf> [--teile <muster,...>]";
    let mut pfade = Vec::new();
    let mut auswahl: Option<Vec<String>> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--teile" => {
                let Some(w) = args.get(i + 1) else {
                    eprintln!("--teile ohne Muster\n{AUFRUF}");
                    return ExitCode::FAILURE;
                };
                auswahl = Some(w.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect());
                i += 2;
            }
            a if a.starts_with("--") => {
                eprintln!("unbekannte Option {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
            a => {
                pfade.push(PathBuf::from(a));
                i += 1;
            }
        }
    }
    let [ein, aus] = pfade.as_slice() else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };

    let m = match klon3d_pipeline::Muster::laden(ein, None) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // `--teile`: der Name beginnt mit einem der Muster; Groß/Klein egal,
    // Leerzeichen und `_` gleich — so passen auch Seamlys Blocknamen.
    let gewaehlt = |t: &klon3d_pattern::Piece| {
        let name = t.name.clone().unwrap_or_default();
        auswahl.as_ref().map_or(true, |a| {
            let n = dxf::blockname(&name);
            a.iter().any(|m| n.starts_with(&dxf::blockname(m)))
        })
    };

    let vorbereitet = match m.dxf_teile(gewaehlt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    println!("{}", ein.display());
    let mut warnungen = 0usize;
    let mut teile = Vec::new();
    for d in vorbereitet {
        warnungen += d.warnungen();
        if d.uebergangen {
            println!(
                "  {:<44} uebergangen: {} Punkte, {} Knoten offen",
                anzeige_name(&d.teil.name),
                d.teil.kante.len(),
                d.offene.len()
            );
            continue;
        }
        let h = dxf::huelle(&d.teil.kante);
        let schnitt = d
            .teil
            .kontur
            .as_ref()
            .map(|k| format!(" Schnitt {:>8.1} mm", dxf::ring_laenge(k)))
            .unwrap_or_default();
        println!(
            "  {:<44} {:>4} Pkt {:>8.1} mm Naht {:>7.1} x {:>7.1} mm {:>3} Passzeichen{}{}",
            anzeige_name(&d.teil.name),
            d.teil.kante.len(),
            dxf::ring_laenge(&d.teil.kante),
            h[2] - h[0],
            h[3] - h[1],
            d.teil.kerben.len(),
            if d.offene.is_empty() {
                String::new()
            } else {
                format!("  ({} Knoten offen)", d.offene.len())
            },
            schnitt
        );
        for text in &d.daneben {
            println!("      Passzeichen fehlt: {text}");
        }
        teile.push(d.teil);
    }

    if teile.is_empty() {
        eprintln!("kein Teil zu schreiben");
        return ExitCode::FAILURE;
    }

    if let Err(e) = std::fs::write(aus, dxf::schreibe(&teile)) {
        eprintln!("DXF nicht schreibbar ({}): {e}", aus.display());
        return ExitCode::FAILURE;
    }
    println!();
    println!("DXF geschrieben: {} ({} Teile)", aus.display(), teile.len());
    if warnungen > 0 {
        println!("  {warnungen} Warnungen, siehe oben");
    }
    ExitCode::SUCCESS
}

fn anzeige(t: &klon3d_pattern::Piece) -> String {
    anzeige_name(&t.name.clone().unwrap_or_else(|| "<ohne Namen>".into()))
}

/// How `klon3d dxf` prints a piece name: at most 43 characters, shortened from the front.
fn anzeige_name(name: &str) -> String {
    let n = if name.is_empty() {
        "<ohne Namen>".to_string()
    } else {
        name.to_string()
    };
    if n.chars().count() > 43 {
        format!("…{}", n.chars().skip(n.chars().count() - 42).collect::<String>())
    } else {
        n
    }
}

/// Muster und Messwerte laden.
///
/// Gibt nur das Muster und die Messwerte zurueck, nicht die Auswertung --
/// die leiht das Muster und koennte so nicht aus der Funktion kommen.
/// `klon3d lage`: Lagedatei vorschlagen (siehe `klon3d_pattern::lagevorschlag`).
fn befehl_lage(args: &[String]) -> ExitCode {
    use klon3d_pattern::kanten::Kurvenquelle;
    use klon3d_pattern::naehte::lies_nahtliste;
    const AUFRUF: &str = "Aufruf: klon3d lage <muster.sm2d> [--naehte <datei.naehte>] [--schreiben <datei.lage>]";
    let (mut muster_pfad, mut naehte, mut schreiben): (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) = (None, None, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--naehte" | "--schreiben" if i + 1 >= args.len() => {
                eprintln!("{} braucht einen Wert\n{AUFRUF}", args[i]);
                return ExitCode::FAILURE;
            }
            "--naehte" => {
                i += 1;
                naehte = Some(PathBuf::from(&args[i]));
            }
            "--schreiben" => {
                i += 1;
                schreiben = Some(PathBuf::from(&args[i]));
            }
            a if a.starts_with("--") => {
                eprintln!("unbekannte Option {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
            a if muster_pfad.is_none() => muster_pfad = Some(PathBuf::from(a)),
            a => {
                eprintln!("überzähliges Argument {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }
    let Some(muster_pfad) = muster_pfad else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };
    let Some((muster, mess)) = laden(&muster_pfad) else {
        return ExitCode::FAILURE;
    };
    let ev = Auswertung::new(&muster, mess);
    let quelle = Kurvenquelle::Seamly;
    // Spiegelung aus dem Nahtvorschlag; die Nähte aus der Liste, sonst auch aus ihm.
    let v = match ev.schlage_naehte_vor(quelle) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}: {e}", muster_pfad.display());
            return ExitCode::FAILURE;
        }
    };
    let defs = match &naehte {
        Some(p) => match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|t| lies_nahtliste(&t).map_err(|e| e.to_string())) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        None => v.naehte.clone(),
    };
    let kopf = format!(
        "Lagevorschlag zu {} (klon3d lage), Naehte aus {}.\nAus Teilnamen, Rahmen, Naehten und den Massen des Musters; bitte durchsehen.",
        muster_pfad.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        naehte.as_ref().map_or("dem Nahtvorschlag".to_string(), |p| p.display().to_string())
    );
    // Mit eigener Nahtliste: Spiegelung aus ihr.
    let gespiegelt = match &naehte {
        Some(p) => match klon3d_pattern::vorschlag::spiegelung(&ev, quelle, &defs) {
            Ok((g, 0)) => g,
            Ok((g, w)) => {
                eprintln!("{}: {w} Nähte genügen keiner Spiegelung (verdreht)", p.display());
                g
            }
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        None => v.gespiegelt.clone(),
    };
    let l = match klon3d_pattern::lagevorschlag::schlage_lage_vor(&ev, quelle, &defs, &gespiegelt, &kopf) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{}: {e}", muster_pfad.display());
            return ExitCode::FAILURE;
        }
    };
    // Selbstprüfung: die Lagedatei muss lesbar sein.
    if let Err(e) = klon3d_sim::platzierung::lies_lage(&l.text) {
        eprintln!("Lagevorschlag nicht lesbar: {e}\n{}", l.text);
        return ExitCode::FAILURE;
    }
    match schreiben {
        Some(p) => {
            if let Err(e) = std::fs::write(&p, &l.text) {
                eprintln!("{}: {e}", p.display());
                return ExitCode::FAILURE;
            }
            println!("{}: Lage geschrieben nach {}", muster_pfad.display(), p.display());
            for h in &l.hinweise {
                println!("# Hinweis: {h}");
            }
        }
        None => print!("{}", l.text),
    }
    ExitCode::SUCCESS
}

fn laden(pfad: &Path) -> Option<(Pattern, std::collections::HashMap<String, f64>)> {
    let muster = match read_path(pfad) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", pfad.display());
            return None;
        }
    };
    let messpfad = muster.measurements.as_ref().map(|m| {
        let p = Path::new(m);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            pfad.parent().unwrap_or(Path::new(".")).join(p)
        }
    });
    let mess = match &messpfad {
        Some(p) if p.is_file() => match lies_messungen(p, &muster.unit) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                return None;
            }
        },
        Some(p) => {
            eprintln!("Massdatei fehlt: {}", p.display());
            return None;
        }
        None => {
            eprintln!("Muster nennt keine Massdatei");
            return None;
        }
    };
    Some((muster, mess))
}

fn befehl_stoelle(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("Aufruf: klon3d stoelle <dump.txt> <muster.sm2d>");
        return ExitCode::FAILURE;
    }
    let mut v: Vec<String> = args.to_vec();
    v.push("--vergleich".into());
    let erste = args[0].clone();
    v.push(erste);
    befehl_punkte(&v[1..])
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Nur zum Laufzeitprüfen: hält `Pattern` sichtbar.
#[allow(dead_code)]
fn _typ(_: &Pattern) {}

/// Schreibt CLOs gespeichertes 3D-Netz als OBJ.
///
/// Ein OBJ-Objekt je Schnittteil, Knoten fortlaufend durchnummeriert (OBJ
/// zählt ab 1). Die Koordinaten sind CLOs Avatarraum in Millimetern - **nicht**
/// gedreht und **nicht** skaliert, damit ein Vergleich gegen `klon3d anziehen`
/// ohne Umrechnung auskommt.
///
/// Objekte ohne Netz (nur `Arrangement Point`) werden übersprungen.
fn schreibe_netz_obj(
    projekt: &klon3d_pattern::zprj::Projekt,
    ziel: &Path,
) -> Result<(usize, usize, usize), String> {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "# CLO 3D-Netz aus {}", projekt.name);
    let _ = writeln!(s, "# Knoten in Millimetern, Avatarraum (x, y = Höhe, z)");
    let _ = writeln!(s, "# je Schnittteil ein Objekt; Bauteile ohne Netz übersprungen");
    let (mut teile, mut knoten, mut dreiecke) = (0, 0usize, 0usize);
    for (i, t) in projekt.teile.iter().enumerate() {
        if t.position3d.is_empty() || t.dreiecke.is_empty() {
            continue;
        }
        teile += 1;
        let stoff = t
            .stoff
            .and_then(|k| projekt.stoffe.get(k))
            .map(|x| x.name.as_str())
            .unwrap_or("-");
        let _ = writeln!(s, "o {}_{}", i, t.name.replace(' ', "_"));
        let _ = writeln!(s, "# Teil {i} {}, Stoff {stoff}", t.name);
        for p in &t.position3d {
            let _ = writeln!(s, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z);
        }
        let basis = knoten as u32;
        for d in &t.dreiecke {
            let _ = writeln!(
                s,
                "f {} {} {}",
                d[0] + basis + 1,
                d[1] + basis + 1,
                d[2] + basis + 1
            );
        }
        knoten += t.position3d.len();
        dreiecke += t.dreiecke.len();
    }
    std::fs::write(ziel, s).map_err(|e| e.to_string())?;
    Ok((teile, knoten, dreiecke))
}

/// Schreibt den Körper aus der `.avt` als OBJ (Millimeter, Avatarraum).
fn schreibe_koerper_obj(
    koerper: &klon3d_pattern::zprj::Koerper,
    ziel: &Path,
) -> Result<(usize, usize), String> {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "# CLO-Körper '{}' aus der .avt", koerper.name);
    let _ = writeln!(s, "# Knoten in Millimetern, Avatarraum (x, y = Höhe, z), A-Pose");
    let _ = writeln!(s, "o {}", koerper.name.replace(' ', "_"));
    for p in &koerper.punkte {
        let _ = writeln!(s, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z);
    }
    for d in &koerper.dreiecke {
        let _ = writeln!(s, "f {} {} {}", d[0] + 1, d[1] + 1, d[2] + 1);
    }
    std::fs::write(ziel, s).map_err(|e| e.to_string())?;
    Ok((koerper.punkte.len(), koerper.dreiecke.len()))
}

/// `klon3d zprj` - liest ein CLO-Projekt (`.zprj`).
///
/// Gibt ohne Optionen eine Übersicht; `--teile`, `--naehte` und `--stoffe`
/// listen die Einzelheiten, `--netz` die 3D-Lage je Teil, und `--obj <datei>`
/// schreibt CLOs simuliertes Netz als OBJ. Rückgabe 1, wenn eine Nahtseite
/// nicht auf eine Linie ihres Teils zeigt - dann stimmt die Deutung des
/// Formats nicht.
fn befehl_zprj(args: &[String]) -> ExitCode {
    use klon3d_pattern::zprj::Projekt;
    const AUFRUF: &str = "Aufruf: klon3d zprj <projekt.zprj> [--teile] [--naehte] [--stoffe] [--netz] [--obj <datei.obj>] [--koerper-obj <datei.obj>]";
    let (mut pfad, mut zeige_teile, mut zeige_naehte, mut zeige_stoffe) =
        (None::<PathBuf>, false, false, false);
    let (mut zeige_netz, mut obj) = (false, None::<PathBuf>);
    let mut koerper_obj: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--teile" => zeige_teile = true,
            "--naehte" => zeige_naehte = true,
            "--stoffe" => zeige_stoffe = true,
            "--netz" => zeige_netz = true,
            "--obj" => match it.next() {
                Some(w) => obj = Some(PathBuf::from(w)),
                None => {
                    eprintln!("--obj braucht einen Dateinamen\n{AUFRUF}");
                    return ExitCode::FAILURE;
                }
            },
            "--koerper-obj" => match it.next() {
                Some(w) => koerper_obj = Some(PathBuf::from(w)),
                None => {
                    eprintln!("--koerper-obj braucht einen Dateinamen\n{AUFRUF}");
                    return ExitCode::FAILURE;
                }
            },
            s if s.starts_with("--") => {
                eprintln!("unbekannte Option {s:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
            s if pfad.is_none() => pfad = Some(PathBuf::from(s)),
            s => {
                eprintln!("überzähliges Argument {s:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(pfad) = pfad else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };

    let projekt = match Projekt::lies(&pfad) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: {e}", pfad.display());
            return ExitCode::FAILURE;
        }
    };

    let knoten: usize = projekt.teile.iter().map(|t| t.netzknoten).sum();
    let dreiecke: usize = projekt.teile.iter().map(|t| t.netzdreiecke).sum();
    let linien: usize = projekt
        .teile
        .iter()
        .map(|t| t.linien.len() + t.innenformen.iter().map(|f| f.linien.len()).sum::<usize>())
        .sum();
    let innen: usize = projekt.teile.iter().map(|t| t.innenformen.len()).sum();

    println!("Projekt  {}", projekt.name);
    println!("Avatar   {}", projekt.avatar.as_deref().unwrap_or("-"));
    println!(
        "Teile    {}   {} Punkte, {} Linien, {} Innenformen",
        projekt.teile.len(),
        projekt.teile.iter().map(|t| t.punkte.len()).sum::<usize>(),
        linien,
        innen
    );
    println!("Netz     {knoten} Knoten, {dreiecke} Dreiecke (2D, Summe der Teile)");
    let knoten3d: usize = projekt.teile.iter().map(|t| t.position3d.len()).sum();
    let dreiecke3d: usize = projekt.teile.iter().map(|t| t.dreiecke.len()).sum();
    if knoten3d > 0 {
        let flach = projekt
            .teile
            .iter()
            .filter(|t| {
                !t.position3d.is_empty()
                    && t.position3d.iter().map(|p| p.z).fold(f64::MIN, f64::max)
                        - t.position3d.iter().map(|p| p.z).fold(f64::MAX, f64::min)
                        < 0.01
            })
            .count();
        println!(
            "3D       {knoten3d} Knoten, {dreiecke3d} Dreiecke aus baPosition3D ({} Teile flach)",
            flach
        );
    }
    println!("Nähte    {} Paare", projekt.naehte.len());
    println!("Stoffe   {}", projekt.stoffe.len());
    if let Some(k) = &projekt.koerper {
        println!(
            "Körper   {} ({} Knoten, {} Dreiecke aus der .avt; {} Netze in der .dan)",
            k.name,
            k.punkte.len(),
            k.dreiecke.len(),
            k.teile.len()
        );
    }

    if zeige_teile {
        println!();
        println!("{:>3}  {:<24} {:>6} {:>6} {:>7} {:>7}  {}", "Nr", "Name", "Punkte", "Linien", "Innen", "Netz", "Stoff");
        for (i, t) in projekt.teile.iter().enumerate() {
            let innenl: usize = t.innenformen.iter().map(|f| f.linien.len()).sum();
            println!(
                "{:>3}  {:<24} {:>6} {:>6} {:>7} {:>7}  {}",
                i,
                t.name,
                t.punkte.len(),
                t.linien.len() + innenl,
                t.innenformen.len(),
                t.netzknoten,
                t.stoff
                    .map(|s| {
                        projekt
                            .stoffe
                            .get(s)
                            .map(|x| x.name.clone())
                            .unwrap_or_else(|| format!("#{s}"))
                    })
                    .unwrap_or_else(|| "-".to_string())
            );
        }
    }

    if zeige_stoffe {
        println!();
        for (i, s) in projekt.stoffe.iter().enumerate() {
            println!(
                "[{i}] {:<24} {:<36} {} mm  {} g/m²",
                s.name,
                s.profil.as_deref().unwrap_or("-"),
                s.dicke_mm.map(|d| format!("{d:.2}")).unwrap_or_else(|| "?".into()),
                s.gewicht_g_m2
                    .map(|g| format!("{g:.0}"))
                    .unwrap_or_else(|| "?".into())
            );
        }
    }

    if zeige_netz {
        println!();
        println!(
            "{:>3}  {:<24} {:<26} {:<8} {:>7} {:>7} {:>7}  {}",
            "Nr", "Name", "Lage", "Zone", "Höhe", "Bogen", "Winkel", "3D"
        );
        for (i, t) in projekt.teile.iter().enumerate() {
            println!(
                "{:>3}  {:<24} {:<26} {:<8} {:>7.3} {:>7.3} {:>7.1}  {}",
                i,
                t.name,
                if t.lage_name.is_empty() { "-" } else { &t.lage_name },
                if t.zone.is_empty() { "-" } else { &t.zone },
                t.lage_hoehe,
                t.lage_bogen,
                t.lage_winkel,
                if t.position3d.is_empty() {
                    "-".to_string()
                } else {
                    format!("{} Knoten, {} Dreiecke", t.position3d.len(), t.dreiecke.len())
                }
            );
        }
    }

    if let Some(ziel) = &obj {
        let knoten: usize = projekt.teile.iter().map(|t| t.position3d.len()).sum();
        if knoten == 0 {
            eprintln!("{}: Projekt trägt kein 3D-Netz (baPosition3D)", pfad.display());
            return ExitCode::FAILURE;
        }
        match schreibe_netz_obj(&projekt, ziel) {
            Ok((t, k, d)) => println!("\nOBJ      {} → {} Teile, {} Knoten, {} Dreiecke", ziel.display(), t, k, d),
            Err(e) => {
                eprintln!("{}: {e}", ziel.display());
                return ExitCode::FAILURE;
            }
        }
    }

    if let Some(ziel) = &koerper_obj {
        let Some(k) = &projekt.koerper else {
            eprintln!("{}: Projekt trägt keinen Körper (.avt)", pfad.display());
            return ExitCode::FAILURE;
        };
        match schreibe_koerper_obj(k, ziel) {
            Ok((n, d)) => println!("\nKörper   {} → {} Knoten, {} Dreiecke", ziel.display(), n, d),
            Err(e) => {
                eprintln!("{}: {e}", ziel.display());
                return ExitCode::FAILURE;
            }
        }
    }

    // Jede Nahtseite muss auf eine Linie ihres Teils zeigen. Wenn nicht,
    // stimmt die Deutung von `uiDepth`/`uiStartIndex` nicht, und das soll
    // auffallen statt still falsche Nähte zu liefern.
    let mut kaputt = 0;
    for n in &projekt.naehte {
        for seite in [&n.seite_a, &n.seite_b] {
            let Some(teil) = projekt.teile.get(seite.teil) else {
                kaputt += 1;
                continue;
            };
            if teil.linie_fuer(seite).is_none() {
                kaputt += 1;
            }
        }
    }

    if zeige_naehte {
        println!();
        println!("{:>3}  {:<22} {:<28} {:<28}", "Nr", "Name", "Seite A", "Seite B");
        for (i, n) in projekt.naehte.iter().enumerate() {
            let seite = |s: &klon3d_pattern::Nahtseite| {
                format!(
                    "Teil {} {}{} L{}..{} n={}",
                    s.teil,
                    if s.tiefe == 1 { "Rand" } else { "Innen" },
                    s.innenform.map(|f| format!("#{f}")).unwrap_or_default(),
                    s.linie_von,
                    s.linie_bis,
                    s.netzpunkte.len()
                )
            };
            println!("{:>3}  {:<22} {:<28} {:<28}", i, n.name, seite(&n.seite_a), seite(&n.seite_b));
        }
        // Gezählt werden Naht**seiten**, nicht Paare: eine Naht zwischen Rand
        // und Innenlinie hat je eine Seite von beiden Sorten.
        let seiten: Vec<&klon3d_pattern::Nahtseite> = projekt
            .naehte
            .iter()
            .flat_map(|n| [&n.seite_a, &n.seite_b])
            .collect();
        let rand = seiten.iter().filter(|s| s.tiefe == 1).count();
        println!();
        println!(
            "Nahtseiten: {} auf dem Teilrand, {} auf Innenlinien ({} Paare)",
            rand,
            seiten.len() - rand,
            projekt.naehte.len()
        );
    }

    if kaputt > 0 {
        eprintln!("{kaputt} Nahtseiten zeigen auf keine Linie ihres Teils");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

// ─────────────────────────────────────────────────────────── vergleich ──

/// `klon3d vergleich` - stellt zwei 3D-Netze gegeneinander (Stufe 5).
///
/// Misst den Abstand jedes Knotens zur **Fläche** des anderen Netzes, in
/// beide Richtungen (siehe [`klon3d_sim::vergleich`]). Das ist der
/// Vergleichsmaßstab aus `PLAN.md`: nicht über gleiche Punktnummern, und
/// wegen der Ausreißer der Median neben dem Mittel.
///
/// Mit `--farben` kommt die Anzeige dazu: beide Netze als OBJ mit einer Farbe
/// je Eckpunkt ([`klon3d_sim::vergleichskarte`]), Skala aus
/// [`klon3d_sim::farbkarte`].
///
/// Rückgabe 1, wenn ein Netz nicht lesbar ist oder keine Dreiecke hat.
fn befehl_vergleich(args: &[String]) -> ExitCode {
    use klon3d_sim::vergleich::{Netz, Vergleich};
    const AUFRUF: &str =
        "Aufruf: klon3d vergleich <a.obj> <b.obj> [--zelle <mm>] [--csv <datei>] [--farben <ordner>]";
    let (mut a, mut b) = (None::<PathBuf>, None::<PathBuf>);
    let mut zelle = 20.0f64;
    let mut csv: Option<PathBuf> = None;
    let mut farben: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--zelle" => match it.next().and_then(|w| w.parse::<f64>().ok()) {
                Some(z) if z > 0.0 && z.is_finite() => zelle = z,
                _ => {
                    eprintln!("--zelle braucht eine Zahl groesser 0 in mm\n{AUFRUF}");
                    return ExitCode::FAILURE;
                }
            },
            "--csv" => match it.next() {
                Some(w) => csv = Some(PathBuf::from(w)),
                None => {
                    eprintln!("--csv braucht einen Dateinamen\n{AUFRUF}");
                    return ExitCode::FAILURE;
                }
            },
            "--farben" => match it.next() {
                Some(w) => farben = Some(PathBuf::from(w)),
                None => {
                    eprintln!("--farben braucht einen Ordner\n{AUFRUF}");
                    return ExitCode::FAILURE;
                }
            },
            s if s.starts_with("--") => {
                eprintln!("unbekannte Option {s:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
            s if a.is_none() => a = Some(PathBuf::from(s)),
            s if b.is_none() => b = Some(PathBuf::from(s)),
            s => {
                eprintln!("ueberzaehliges Argument {s:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
    }
    let (Some(pa), Some(pb)) = (a, b) else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };

    let lies = |p: &Path| -> Option<Netz> {
        match std::fs::read_to_string(p) {
            Ok(t) => match Netz::lies_obj(&t) {
                Ok(n) => Some(n),
                Err(e) => {
                    eprintln!("{}: {e}", p.display());
                    None
                }
            },
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                None
            }
        }
    };
    let (Some(na), Some(nb)) = (lies(&pa), lies(&pb)) else {
        return ExitCode::FAILURE;
    };

    let t0 = std::time::Instant::now();
    let v = match Vergleich::rechne(&na, &nb, zelle) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Vergleich nicht moeglich: {e}");
            return ExitCode::FAILURE;
        }
    };
    let dauer = t0.elapsed().as_secs_f64();

    let zeige = |was: &str, p: &Path, n: &Netz, k: &klon3d_sim::vergleich::Kennzahlen| {
        println!(
            "  {was:<10} {}  {} Punkte, {} Dreiecke",
            p.display(),
            n.punkte.len(),
            n.dreiecke.len()
        );
        println!(
            "      Median {:8.3}  Mittel {:8.3}  90 % {:8.3}  99 % {:8.3}  max {:8.3} mm",
            k.median, k.mittel, k.p90, k.p99, k.max
        );
    };
    println!("Vergleich in {dauer:.2} s, Suchgitter {zelle} mm");
    zeige("a (Hinweg)", &pa, &na, &v.hin());
    zeige("b (Rueckweg)", &pb, &nb, &v.zurueck());

    let gruppen = v.je_gruppe(&na, &nb);
    if !gruppen.is_empty() {
        println!();
        println!("Je Gruppe (a gegen die Flaeche von b, mm):");
        println!("  {:<34} {:>7} {:>9} {:>9} {:>9}", "Gruppe", "Knoten", "Median", "Mittel", "max");
        for (name, anzahl, k) in &gruppen {
            println!(
                "  {:<34} {:>7} {:>9.3} {:>9.3} {:>9.3}",
                name, anzahl, k.median, k.mittel, k.max
            );
        }
    }

    if let Some(ziel) = csv {
        let mut s = String::from("richtung,knoten,abstand_mm\n");
        for (i, d) in v.a_zu_b.iter().enumerate() {
            s.push_str(&format!("a_zu_b,{i},{d:.6}\n"));
        }
        for (i, d) in v.b_zu_a.iter().enumerate() {
            s.push_str(&format!("b_zu_a,{i},{d:.6}\n"));
        }
        if let Err(e) = std::fs::write(&ziel, s) {
            eprintln!("{}: {e}", ziel.display());
            return ExitCode::FAILURE;
        }
        println!("\nAbstaende je Knoten: {}", ziel.display());
    }

    if let Some(o) = farben {
        // Beide Richtungen als OBJ mit Farbe je Eckpunkt: dieselbe Geometrie
        // wie die Eingaben, nur drei Zahlen mehr je `v`. A gegen b zeigt, wo b
        // von a abweicht, b gegen a die Gegenrichtung - ein Netz kann feiner
        // sein als das andere, eine Richtung allein verschweigt das.
        use klon3d_sim::vergleichskarte::obj_farbig;
        if let Err(e) = std::fs::create_dir_all(&o) {
            eprintln!("{}: {e}", o.display());
            return ExitCode::FAILURE;
        }
        let dateien = [
            ("a_zu_b.obj", obj_farbig(&na, &v.a_zu_b)),
            ("b_zu_a.obj", obj_farbig(&nb, &v.b_zu_a)),
        ];
        for (name, text) in dateien {
            let ziel = o.join(name);
            if let Err(e) = std::fs::write(&ziel, text) {
                eprintln!("{}: {e}", ziel.display());
                return ExitCode::FAILURE;
            }
        }
        println!("\nVergleichskarte: {}", o.display());
        println!("  a_zu_b.obj  Knoten von a, Farbe = Abstand zur Flaeche von b");
        println!("  b_zu_a.obj  Knoten von b, Farbe = Abstand zur Flaeche von a");
        println!(
            "  Farbskala   {:.1}…{:.0} mm (blau = im Rauschen des Vergleichs, rot = Abweichung)",
            klon3d_sim::farbkarte::VERGLEICH_VON,
            klon3d_sim::farbkarte::VERGLEICH_BIS
        );
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    //! Nur die Optionsauswertung: die Roehre selbst rechnet zwoelf Minuten, und
    //! was hier geprueft wird, ist, dass `--material` ueberhaupt bis zum Stoff
    //! kommt - der Rueckfall auf 0,4 mm war der Fehler aus #87.

    use super::*;

    fn args(liste: &[&str]) -> Vec<String> {
        liste.iter().map(|s| (*s).to_string()).collect()
    }

    /// The eight files as `klon3d-pipeline`'s own test names them.
    fn stoffordner() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/stoffe")
    }

    /// A path to a fabric arrives at the tube as that fabric - thickness
    /// included, which is the whole point of comparing a material against the
    /// arithmetic it came from.
    #[test]
    fn roehre_nimmt_eine_stoffdatei() {
        let p = stoffordner().join("Latex_Laminat_2x1.0mm_abgeleitet.zfab");
        let a = roehre_optionen(&args(&["--material", &p.display().to_string()])).unwrap();
        assert_eq!(a.r.material.name, "Latex_Laminat_2x1.0mm_abgeleitet");
        assert!((a.r.material.dicke_m * 1e3 - 2.0).abs() < 0.01, "Dicke {} mm", a.r.material.dicke_m * 1e3);
    }

    /// The same by stem, through the folder search - the case that needs
    /// `assets/stoffe` and therefore failed silently before #87.
    #[test]
    fn roehre_nimmt_einen_stamm() {
        let a = roehre_optionen(&args(&["--material", "Latex_1.0mm_abgeleitet"])).unwrap();
        assert_eq!(a.r.material.name, "Latex_1.0mm_abgeleitet");
        assert!((a.r.material.dicke_m * 1e3 - 1.0).abs() < 0.01, "Dicke {} mm", a.r.material.dicke_m * 1e3);
    }

    /// The built-in profiles stay, and so does the default without `--material`.
    #[test]
    fn roehre_ohne_material_bleibt_bei_radical() {
        let a = roehre_optionen(&[]).unwrap();
        assert_eq!(a.r.material.name, klon3d_sim::RADICAL_RUBBER_04.name);
        let a = roehre_optionen(&args(&["--material", "retifist"])).unwrap();
        assert_eq!(a.r.material.name, klon3d_sim::Material::nach_namen("retifist").unwrap().name);
    }

    /// An unknown name is an error that names the fabrics folder or the
    /// profiles, not a run at 0.4 mm. Before #87 the tube answered `false` here
    /// and stopped with the generic "unknown option".
    #[test]
    fn roehre_lehnt_unbekannten_stoff_ab() {
        let e = roehre_optionen(&args(&["--material", "Latex_0.5mm"])).unwrap_err();
        assert!(e.contains("Latex_0.5mm"), "{e}");
        assert!(e.contains("assets/stoffe") || e.contains("radical"), "die Meldung nennt keine Quelle: {e}");
    }

    /// A missing value is not silently the default either.
    #[test]
    fn roehre_lehnt_material_ohne_wert_ab() {
        let e = roehre_optionen(&args(&["--material"])).unwrap_err();
        assert!(e.contains("--material"), "{e}");
    }

    /// The other options still parse, so splitting the reader did not break the
    /// command.
    #[test]
    fn roehre_optionen_bleiben() {
        let a = roehre_optionen(&args(&["--reduktion", "5,5", "--kante", "20", "--spalt", "--rutsch", "0,5"])).unwrap();
        assert!((a.r.reduktion - 0.055).abs() < 1e-12);
        assert!((a.r.kante_mm - 20.0).abs() < 1e-12);
        assert!(a.r.anfang_mit_spalt);
        assert_eq!(a.rutsch, Some(0.5));
        // An out-of-range value is refused, as before.
        assert!(roehre_optionen(&args(&["--reduktion", "95"])).is_err());
        assert!(roehre_optionen(&args(&["--kante", "1"])).is_err());
    }
}