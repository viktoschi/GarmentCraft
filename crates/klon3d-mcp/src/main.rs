//! `klon3d-mcp [--lesen <folder>]... [--schreiben <folder>]...` – an MCP server on stdin/stdout
//! that drives the klon3d application headless: no window, no window handle, no focus.
//!
//! | Module | Content |
//! |---|---|
//! | [`rpc`] | JSON-RPC 2.0 lines, `initialize`, `ping`, `tools/list`, `tools/call` |
//! | [`werkzeuge`] | tool list with input schemas, commands of `command_run` |
//! | [`sitzung`] | the application in an `egui_kittest` harness; the tools |
//! | [`simulation`] | the stepped scene of `simulate` |
//! | [`vorschau`] | wgpu renderer created on the first picture |
//! | [`wurzeln`] | read and write roots |
//! | [`base64`] | encoding of the PNG |
//!
//! Only protocol lines go to stdout; messages go to stderr.

mod base64;
mod rpc;
mod simulation;
mod sitzung;
mod vorschau;
mod werkzeuge;
mod wurzeln;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const AUFRUF: &str = "Aufruf: klon3d-mcp [--lesen <ordner>]... [--schreiben <ordner>]...\n\
                      MCP-Server über stdin/stdout (JSON-RPC, eine Nachricht je Zeile).\n\
                      Jeder Pfad eines Werkzeugs muss unter einer Lese- bzw. Schreib-Wurzel liegen;\n\
                      Schreib-Wurzeln sind auch lesbar.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut lesen, mut schreiben) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            a @ ("--lesen" | "--schreiben") => {
                let Some(w) = args.get(i + 1) else {
                    eprintln!("{a} ohne Ordner\n{AUFRUF}");
                    return ExitCode::FAILURE;
                };
                if a == "--lesen" {
                    lesen.push(PathBuf::from(w));
                } else {
                    schreiben.push(PathBuf::from(w));
                }
                i += 2;
            }
            "-h" | "--help" => {
                println!("{AUFRUF}");
                return ExitCode::SUCCESS;
            }
            a => {
                eprintln!("unbekannte Angabe {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
    }
    let wurzeln = match wurzeln::Wurzeln::neu(&lesen, &schreiben) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if lesen.is_empty() && schreiben.is_empty() {
        eprintln!(
            "klon3d-mcp: keine Wurzel angegeben – kein Werkzeug darf Dateien lesen oder schreiben"
        );
    }
    let mut sitzung = sitzung::Sitzung::neu(wurzeln);
    eprintln!("klon3d-mcp {} bereit", env!("CARGO_PKG_VERSION"));

    let mut eingabe = std::io::stdin().lock();
    let mut ausgabe = std::io::stdout().lock();
    let mut puffer = Vec::new();
    loop {
        puffer.clear();
        match eingabe.read_until(b'\n', &mut puffer) {
            Ok(0) => return ExitCode::SUCCESS,
            Ok(_) => {}
            Err(e) => {
                eprintln!("stdin: {e}");
                return ExitCode::FAILURE;
            }
        }
        let text = String::from_utf8_lossy(&puffer);
        if let Some(antwort) = rpc::zeile(&text, &mut sitzung) {
            let ok = writeln!(ausgabe, "{antwort}").and_then(|_| ausgabe.flush());
            if ok.is_err() {
                // The client has gone.
                return ExitCode::SUCCESS;
            }
        }
    }
}
