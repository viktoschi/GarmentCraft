//! `klon3d-app [muster.sm2d]` – opens the application window, optionally with a pattern.
//!
//! A release build on Windows is a GUI program: started from Explorer, a shortcut or a file
//! association it opens no console window (closing that would end the app without saving). Its
//! messages – usage, a window that cannot open – then also appear in a message box. Debug builds
//! (`cargo run`) keep the console.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use klon3d_app::App;

const AUFRUF: &str = "Aufruf: klon3d-app [muster.sm2d]";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let datei = args.next().map(PathBuf::from);
    if args.next().is_some() {
        melden(&format!("zu viele Argumente\n{AUFRUF}"), true);
        return ExitCode::FAILURE;
    }
    if let Some(d) = &datei {
        if matches!(d.to_str(), Some("-h" | "--help" | "/?")) {
            melden(AUFRUF, false);
            return ExitCode::SUCCESS;
        }
    }

    let optionen = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("klon3d")
            .with_app_id("klon3d")
            .with_inner_size(klon3d_app::app::FENSTER_VORGABE)
            .with_min_inner_size(klon3d_app::app::FENSTER_MIN)
            .with_drag_and_drop(true),
        ..Default::default()
    };
    let ergebnis = eframe::run_native(
        "klon3d",
        optionen,
        Box::new(move |cc| {
            let mut app = App::neu(cc);
            if let Some(d) = datei {
                // Errors end up in the status bar and the error window.
                app.muster_laden(d);
            }
            Ok(Box::new(app))
        }),
    );
    match ergebnis {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            melden(
                &format!("klon3d-app: Fenster lässt sich nicht öffnen: {e}"),
                true,
            );
            ExitCode::FAILURE
        }
    }
}

/// A message on the console (stderr for errors); in a Windows release build, which has no
/// console, also in a message box.
fn melden(text: &str, fehler: bool) {
    if fehler {
        eprintln!("{text}");
    } else {
        println!("{text}");
    }
    #[cfg(windows)]
    if !cfg!(debug_assertions) {
        meldungsfenster(text, fehler);
    }
}

#[cfg(windows)]
fn meldungsfenster(text: &str, fehler: bool) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            fenster: *mut std::ffi::c_void,
            text: *const u16,
            titel: *const u16,
            art: u32,
        ) -> i32;
    }
    const MB_ICONERROR: u32 = 0x10;
    const MB_ICONINFORMATION: u32 = 0x40;
    let breit = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (text, titel) = (breit(text), breit("klon3d"));
    let art = if fehler {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    // SAFETY: both strings are NUL-terminated UTF-16 and outlive the call; no owner window.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), titel.as_ptr(), art);
    }
}
