//! `bildschirmfoto [--muster <datei.sm2d>] [--aus <bild.png>] [--breite <punkte>] [--hoehe <punkte>]`
//! – renders the application window to a PNG without opening one.
//!
//! Why an example and not a test: it needs a wgpu adapter. Where there is none (a headless CI
//! runner without a software Vulkan) it reports that and exits with code 3 instead of failing.
//!
//! This is also the piece an MCP server would sit on: the window is built by the library, driven
//! by actions instead of clicks, and every step can be photographed. Nothing here needs a desktop,
//! a window handle or the focus.
//!
//! Run with `cargo run -p klon3d-app --example bildschirmfoto --features fotos -- --muster …`.

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::Harness;
use klon3d_app::App;

const AUFRUF: &str = "Aufruf: bildschirmfoto [--muster <datei.sm2d>] [--aus <bild.png>]\n\
                     \x20             [--breite <punkte>] [--hoehe <punkte>] [--dunkel|--hell]\n\
                     \x20             [--massstab <punkte je Pixel>]\n\
                     Kennung 3: kein Grafikbeschleuniger (wgpu-Adapter) vorhanden";

/// The window as the app opens it (`main.rs`), unless the caller asks for another size.
const BREITE: f32 = 1400.0;
const HOEHE: f32 = 900.0;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opt = Optionen::default();
    let mut i = 0;
    while i < args.len() {
        let nimm = |i: usize| -> Result<&String, ExitCode> {
            args.get(i + 1).ok_or_else(|| {
                eprintln!("{} ohne Wert\n{AUFRUF}", args[i]);
                ExitCode::FAILURE
            })
        };
        let wert = |i: usize| -> Result<String, ExitCode> { Ok(nimm(i)?.clone()) };
        match args[i].as_str() {
            "--muster" => match wert(i) {
                Ok(w) => {
                    opt.muster = Some(PathBuf::from(w));
                    i += 2;
                }
                Err(e) => return e,
            },
            "--aus" => match wert(i) {
                Ok(w) => {
                    opt.aus = PathBuf::from(w);
                    i += 2;
                }
                Err(e) => return e,
            },
            "--breite" => match zahl(&args, i, AUFRUF) {
                Ok((w, n)) => {
                    opt.breite = w;
                    i += n;
                }
                Err(e) => return e,
            },
            "--hoehe" => match zahl(&args, i, AUFRUF) {
                Ok((w, n)) => {
                    opt.hoehe = w;
                    i += n;
                }
                Err(e) => return e,
            },
            "--massstab" => match zahl(&args, i, AUFRUF) {
                Ok((w, n)) => {
                    opt.massstab = w;
                    i += n;
                }
                Err(e) => return e,
            },
            "--dunkel" => {
                opt.dunkel = Some(true);
                i += 1;
            }
            "--hell" => {
                opt.dunkel = Some(false);
                i += 1;
            }
            "-h" | "--help" | "/?" => {
                println!("{AUFRUF}");
                return ExitCode::SUCCESS;
            }
            a => {
                eprintln!("unbekannte Angabe {a:?}\n{AUFRUF}");
                return ExitCode::FAILURE;
            }
        }
    }
    match laufen(&opt) {
        Ok(()) => ExitCode::SUCCESS,
        Err(fehler) => {
            eprintln!("{fehler}");
            ExitCode::FAILURE
        }
    }
}

struct Optionen {
    muster: Option<PathBuf>,
    aus: PathBuf,
    breite: f32,
    hoehe: f32,
    massstab: f32,
    /// `None`: whatever egui's stored theme is; `Some(true)`: dark, `Some(false)`: light.
    dunkel: Option<bool>,
}

impl Default for Optionen {
    fn default() -> Self {
        Optionen {
            muster: None,
            aus: PathBuf::from("fenster.png"),
            breite: BREITE,
            hoehe: HOEHE,
            massstab: 1.0,
            dunkel: None,
        }
    }
}

fn zahl(args: &[String], i: usize, aufruf: &str) -> Result<(f32, usize), ExitCode> {
    let Some(text) = args.get(i + 1) else {
        eprintln!("{} ohne Wert\n{aufruf}", args[i]);
        return Err(ExitCode::FAILURE);
    };
    match text.replace(',', ".").parse::<f32>() {
        Ok(v) if v.is_finite() && v > 0.0 => Ok((v, 2)),
        _ => {
            eprintln!("{}: keine positive Zahl: {text:?}\n{aufruf}", args[i]);
            Err(ExitCode::FAILURE)
        }
    }
}

fn laufen(opt: &Optionen) -> Result<(), String> {
    let mut figuren: Harness<'_, App> = Harness::builder()
        .with_size([opt.breite, opt.hoehe])
        .with_pixels_per_point(opt.massstab)
        // Rendering is deferred until `figuren.render()`; a missing adapter is then an error we
        // can report instead of a panic in the builder.
        .renderer(WgpuTestRenderer::new())
        .build_eframe(|cc| App::neu(cc));

    // Through the app's own state, not the builder's theme: the app applies `self.thema` every
    // frame (`app.rs`, "the theme is applied every frame"), so the builder's theme would lose.
    if let Some(dunkel) = opt.dunkel {
        figuren.state_mut().thema = if dunkel {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
    }

    if let Some(pfad) = &opt.muster {
        // Fail on a pattern that does not load: a photo of an empty window would otherwise look
        // like a success.
        if !figuren.state_mut().muster_laden(pfad) {
            let meldung = figuren.state().meldung.clone();
            return Err(format!(
                "{} laesst sich nicht laden: {meldung}",
                pfad.display()
            ));
        }
    }
    // Two passes: the first lays the panels out, the second draws with the sizes the first found.
    figuren.run();
    figuren.run();

    let bild = figuren.render().map_err(|e| {
        format!("kein Bild gerendert: {e}\nKennung 3: kein Grafikbeschleuniger vorhanden?")
    })?;

    if let Some(ordner) = opt.aus.parent() {
        if !ordner.as_os_str().is_empty() {
            std::fs::create_dir_all(ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
        }
    }
    bild.save(&opt.aus)
        .map_err(|e| format!("{}: {e}", opt.aus.display()))?;
    println!(
        "{} – {} x {} Bildpunkte{}",
        opt.aus.display(),
        bild.width(),
        bild.height(),
        match &opt.muster {
            Some(p) => format!(", Muster {}", p.display()),
            None => String::new(),
        }
    );
    Ok(())
}
