//! `klon3d innenpfade`: die Innenpfade (`<path>`) je Teil, ausgewertet.
//!
//! Name, `cut`, offen/geschlossen, Knotenzahl, Laenge in mm und - bei einem
//! geschlossenen Pfad - die Flaeche in mm². Das sind die Daten, die eine
//! Klebezone, eine Verzierung oder ein auszuschneidendes Loch beschreiben;
//! es wird nichts genaeht und kein Teil veraendert ([`klon3d_pattern::innenpfad`]).
//!
//! Rueckgabe 1, wenn ein Pfad nicht aufgeloest werden kann oder ein Teil einen
//! Pfad nennt, den es im Muster nicht gibt; beides wird mit Namen gemeldet.
//! `--alle` zeigt zusaetzlich die Pfade, die kein Teil traegt: gezeichnet
//! werden sie nirgends, aber in der Datei stehen sie.

use std::path::PathBuf;
use std::process::ExitCode;

use klon3d_pattern::geom::Auswertung;

const AUFRUF: &str = "Aufruf: klon3d innenpfade <muster.sm2d> [--alle]";

pub fn befehl_innenpfade(args: &[String]) -> ExitCode {
    let Some(pfad) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) else {
        eprintln!("{AUFRUF}");
        return ExitCode::FAILURE;
    };
    let alle = args.iter().any(|a| a == "--alle");

    let Some((muster, mess)) = crate::laden(&pfad) else {
        return ExitCode::FAILURE;
    };
    let ev = Auswertung::new(&muster, mess);

    let f = muster.unit_to_mm().unwrap_or(1.0);
    let mut mit_pfaden = 0usize;
    let mut pfade = 0usize;
    let mut meldungen = 0usize;

    for t in muster.pieces() {
        let (liste, meldung) = ev.innenpfade(t);
        if liste.is_empty() && meldung.is_empty() {
            continue;
        }
        mit_pfaden += 1;
        pfade += liste.len();
        println!(
            "  {:<40} {:>2} {}",
            crate::anzeige_name(t.name.as_deref().unwrap_or("")),
            liste.len(),
            if liste.len() == 1 { "Innenpfad" } else { "Innenpfade" }
        );
        for p in &liste {
            let art = if p.cut { "schnitt" } else { "zeichnen" };
            let form = if p.geschlossen { "geschlossen" } else { "offen" };
            let flaeche = match p.flaeche_mm2 {
                Some(a) => format!("  {a:>10.1} mm²"),
                None => String::new(),
            };
            println!(
                "      {:<34} {:<9} {:<11} {:>2} Knoten {:>9.1} mm{flaeche}",
                crate::anzeige_name(&p.name),
                art,
                form,
                p.knoten,
                p.laenge_mm
            );
        }
        for m in &meldung {
            println!("      ! {m}");
        }
        meldungen += meldung.len();
    }

    println!();
    println!(
        "  {pfade} Innenpfade an {mit_pfaden} von {} Teilen (1 Einheit = {f} mm)",
        muster.pieces().len()
    );

    if alle {
        let unbenutzt: Vec<&klon3d_pattern::InnerPath> = ev
            .alle_innenpfade()
            .into_iter()
            .filter(|p| {
                !muster.pieces().iter().any(|t| {
                    t.i_paths
                        .iter()
                        .flat_map(|b| b.paths.iter())
                        .any(|r| r.path == p.id)
                })
            })
            .collect();
        println!("  {} Pfade ohne Teil", unbenutzt.len());
        for p in unbenutzt {
            println!(
                "      {:<10} {:<34} {}",
                p.id,
                crate::anzeige_name(p.name.as_deref().unwrap_or("")),
                if matches!(p.cut.as_deref(), Some("true") | Some("1")) {
                    "schnitt"
                } else {
                    "zeichnen"
                }
            );
        }
    }

    if meldungen > 0 {
        eprintln!("{meldungen} Pfade nicht aufloesbar");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
