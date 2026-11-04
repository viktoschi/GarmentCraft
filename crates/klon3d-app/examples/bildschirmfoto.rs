//! `bildschirmfoto [--muster <datei.sm2d>] [--aus <bild.png>] [--breite <punkte>] [--hoehe <punkte>]`
//! `[--simulieren <bilder>]` – renders the application window to a PNG without opening one.
//!
//! Why an example and not a test: it needs a wgpu adapter. Where there is none (a headless CI
//! runner without a software Vulkan) it reports that and exits with code 3 instead of failing.
//!
//! This is also the piece an MCP server would sit on: the window is built by the library, driven
//! by actions instead of clicks, and every step can be photographed. Nothing here needs a desktop,
//! a window handle or the focus.
//!
//! ## Simulation
//!
//! `--simulieren <bilder>` starts the simulation of the pattern shown (the same call the menu
//! makes, [`App::simulation_starten`]), lets the worker build the scene, computes `<bilder>`
//! solver frames and photographs the 3D view with them. The frames are drawn by the 3D view's
//! wgpu paint callback, so this needs the real renderer ([`WgpuTestRenderer`] hands the render
//! state to the window, exactly as eframe does). One solver frame of `bluse3_v2` takes about
//! 260–350 ms in a release build; the scene itself takes longer than the frames.
//!
//! Run with `cargo run -p klon3d-app --example bildschirmfoto -- --muster … --simulieren 2`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::Harness;
use klon3d_app::karten::Karte;
use klon3d_app::simulation::Befehl;
use klon3d_app::App;

const AUFRUF: &str = "Aufruf: bildschirmfoto [--muster <datei.sm2d>] [--aus <bild.png>]\n\
                     \x20             [--breite <punkte>] [--hoehe <punkte>] [--dunkel|--hell]\n\
                     \x20             [--massstab <punkte je Pixel>] [--simulieren <bilder>]\n\
                     \x20             [--karte dehnung|passform|abstand]\n\
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
            "--simulieren" => match anzahl(&args, i, AUFRUF) {
                Ok((w, n)) => {
                    opt.simulieren = w;
                    i += n;
                }
                Err(e) => return e,
            },
            "--karte" => match wert(i) {
                Ok(w) => {
                    opt.karte = match w.as_str() {
                        "dehnung" => Some(Karte::Dehnung),
                        "passform" => Some(Karte::Passform),
                        "abstand" => Some(Karte::Abstand),
                        "keine" | "aus" => None,
                        a => {
                            eprintln!(
                                "--karte: unbekannte Karte {a:?} (dehnung, passform, abstand)\n{AUFRUF}"
                            );
                            return ExitCode::FAILURE;
                        }
                    };
                    i += 2;
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
    /// Solver frames to compute before the photo; 0 = no simulation.
    simulieren: usize,
    /// The inspection map to show (needs `--simulieren`).
    karte: Option<Karte>,
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
            simulieren: 0,
            karte: None,
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

/// A whole number of frames; 0 is allowed (no frame), negative and fractional values are not.
fn anzahl(args: &[String], i: usize, aufruf: &str) -> Result<(usize, usize), ExitCode> {
    let Some(text) = args.get(i + 1) else {
        eprintln!("{} ohne Wert\n{aufruf}", args[i]);
        return Err(ExitCode::FAILURE);
    };
    match text.parse::<usize>() {
        Ok(v) => Ok((v, 2)),
        _ => {
            eprintln!("{}: keine ganze Zahl: {text:?}\n{aufruf}", args[i]);
            Err(ExitCode::FAILURE)
        }
    }
}

/// `WgpuTestRenderer::new` sets up the device at once and panics without an adapter; caught here so
/// that a machine without one gets exit code 3 and a sentence instead of a backtrace.
fn renderer() -> Option<WgpuTestRenderer> {
    let vorher = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(WgpuTestRenderer::new).ok();
    std::panic::set_hook(vorher);
    r
}

/// Give up after this long: the scene build (seconds) and the frames (≈ 0.3 s each) must fit.
/// Bounded so that a stuck worker ends in an error instead of a hanging run.
const SIMULATIONSFRIST: Duration = Duration::from_secs(600);

/// How long to wait for a map: the distance to the body is measured in the worker, point by point,
/// and takes a few seconds on a full garment. Longer than any plausible measurement, short enough
/// that a map which never comes ends in an error instead of a hanging run.
const KARTENFRIST: Duration = Duration::from_secs(300);

/// Between two window passes: the solver works on the other cores, so a pause costs nothing but
/// keeps this thread from fighting it for a core.
const SCHRITT_PAUSE: Duration = Duration::from_millis(20);

/// Run the simulation of the pattern shown for `bilder` frames and hold it there; the window then
/// shows that frame in the 3D view.
///
/// The scene is built by the worker thread ([`klon3d_app::simulation`]) and arrives as messages,
/// which the window takes in during its own pass – hence the loop of [`Harness::step`] until the
/// frame number is reached. [`Harness::run`] cannot be used: the worker asks for repaints while it
/// computes, so `run` would never see a settled window.
fn simulieren(figuren: &mut Harness<'_, App>, bilder: usize) -> Result<(), String> {
    let ctx = figuren.ctx.clone();
    if figuren.state().muster.is_none() {
        return Err("--simulieren braucht ein geladenes Muster (--muster <datei.sm2d>)".into());
    }
    let start = Instant::now();
    // The same call the menu makes, so the example exercises the window's own path.
    if !figuren.state_mut().simulation_starten(&ctx) {
        let meldung = figuren.state().meldung.clone();
        return Err(format!("Simulation startet nicht: {meldung}"));
    }
    // `starten` lets the worker run freely (`Laufen`). The command is in the channel long before
    // the worker has built the scene, so it holds after exactly these frames.
    figuren
        .state_mut()
        .simulation
        .befehl(Befehl::Schritte(bilder));

    let mut szenenzeit = None;
    loop {
        figuren.step();
        {
            let s = &figuren.state().simulation;
            if let Some(f) = s.fehler() {
                return Err(format!("Simulation: {f}"));
            }
            if szenenzeit.is_none() && s.bereit() {
                let dauer = start.elapsed().as_secs_f64();
                println!("Simulation: Szene steht nach {dauer:.1} s");
                szenenzeit = Some(dauer);
            }
            if s.bereit() && s.bildnummer() >= bilder {
                break;
            }
        }
        if start.elapsed() > SIMULATIONSFRIST {
            let s = &figuren.state().simulation;
            return Err(format!(
                "Simulation nach {} s abgebrochen: bereit {}, Bild {} von {bilder}",
                SIMULATIONSFRIST.as_secs(),
                s.bereit(),
                s.bildnummer()
            ));
        }
        std::thread::sleep(SCHRITT_PAUSE);
    }

    // Hold the run: no frame may move the cloth while the picture is taken.
    figuren.state_mut().simulation.befehl(Befehl::Anhalten);
    let ende = Instant::now() + Duration::from_secs(30);
    while figuren.state().simulation.laeuft() && Instant::now() < ende {
        figuren.step();
        std::thread::sleep(SCHRITT_PAUSE);
    }

    let s = &figuren.state().simulation;
    let (bau, x_punkte) = s
        .bau()
        .map(|b| (b.herkunft.clone(), b.koerper.len()))
        .unwrap_or((String::new(), 0));
    println!(
        "Simulation: Bild {} ({}), Körper {x_punkte} Punkte, {} in {} s",
        s.bildnummer(),
        s.phase_name(),
        bau,
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Step the window until the snapshot that is shown carries `karte`.
///
/// [`klon3d_app::simulation::Simulation::karte_setzen`] only asks the worker; the values arrive as
/// a snapshot of their own ([`klon3d_app::simulation::Meldung::Bild`]) once the worker has measured
/// them. Waiting for the frame number would not do: the run is held, so no frame advances.
fn warten_auf_karte(figuren: &mut Harness<'_, App>, karte: Karte) -> Result<(), String> {
    let start = Instant::now();
    loop {
        figuren.step();
        {
            let s = &figuren.state().simulation;
            if let Some(f) = s.fehler() {
                return Err(format!("Simulation: {f}"));
            }
            if s.bild().and_then(|b| b.karte.as_ref()).is_some_and(|w| w.karte == karte) {
                let dauer = start.elapsed().as_secs_f64();
                println!("Karte {}: gemessen nach {dauer:.1} s", karte.titel());
                return Ok(());
            }
        }
        if start.elapsed() > KARTENFRIST {
            return Err(format!(
                "Karte {} nach {} s nicht gemessen",
                karte.titel(),
                KARTENFRIST.as_secs()
            ));
        }
        std::thread::sleep(SCHRITT_PAUSE);
    }
}

fn laufen(opt: &Optionen) -> Result<(), String> {
    let Some(renderer) = renderer() else {
        eprintln!("kein Grafikbeschleuniger (wgpu-Adapter) vorhanden");
        std::process::exit(3);
    };
    let mut figuren: Harness<'_, App> = Harness::builder()
        .with_size([opt.breite, opt.hoehe])
        .with_pixels_per_point(opt.massstab)
        .renderer(renderer)
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

    // After those passes: the 3D window needs its width from the layout, and the simulation
    // frames must not be computed while the panels still settle.
    if opt.karte.is_some() && opt.simulieren == 0 {
        return Err("--karte braucht --simulieren <bilder>: die Karte wird auf einem Bild gemessen".into());
    }
    if opt.simulieren > 0 {
        simulieren(&mut figuren, opt.simulieren)?;
        // The last frames built the 3D vertex data for the frame reached; two passes let the
        // camera frame the scene and the 3D view draw it.
        figuren.run();
        figuren.run();
    }
    if let Some(karte) = opt.karte {
        // The run is held; the worker measures the map on the frame that is standing and sends it
        // once. The distance to the body is measured point by point and takes seconds, so the
        // window is stepped until the snapshot carries the map – a fixed number of passes would
        // photograph the garment in its plain colour and the legend would be missing.
        figuren.state_mut().simulation.karte_setzen(Some(karte));
        warten_auf_karte(&mut figuren, karte)?;
        figuren.run();
        figuren.run();
    }

    let bild = figuren
        .render()
        .map_err(|e| format!("kein Bild gerendert: {e}"))?;

    if let Some(ordner) = opt.aus.parent() {
        if !ordner.as_os_str().is_empty() {
            std::fs::create_dir_all(ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
        }
    }
    bild.save(&opt.aus)
        .map_err(|e| format!("{}: {e}", opt.aus.display()))?;
    println!(
        "{} – {} x {} Bildpunkte{}{}",
        opt.aus.display(),
        bild.width(),
        bild.height(),
        match &opt.muster {
            Some(p) => format!(", Muster {}", p.display()),
            None => String::new(),
        },
        match opt.simulieren {
            0 => String::new(),
            n => format!(", Simulierung nach {n} Bildern"),
        }
    );
    if let Some(karte) = opt.karte {
        println!("Karte: {}", karte.titel());
    }
    Ok(())
}
