//! A renderer for the headless window that creates its wgpu device on the first picture.
//!
//! `WgpuTestRenderer::new` panics when there is no adapter. Created lazily and inside
//! `catch_unwind`, a missing adapter becomes an error of `render_preview` and the server goes
//! on. Texture updates of the frames before are kept and handed over once the device exists.

use std::panic::{catch_unwind, AssertUnwindSafe};

use eframe::egui;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::TestRenderer;

/// Start of every error text when no device could be created.
pub const KEIN_ADAPTER: &str = "kein wgpu-Adapter";

#[derive(Default)]
pub struct Vorschau {
    renderer: Option<WgpuTestRenderer>,
    /// Texture updates not yet handed to a renderer.
    offen: egui::TexturesDelta,
    /// Why the device could not be created; not tried again.
    fehler: Option<String>,
}

impl Vorschau {
    fn bereit(&mut self) -> Result<&mut WgpuTestRenderer, String> {
        if let Some(f) = &self.fehler {
            return Err(f.clone());
        }
        if self.renderer.is_none() {
            match catch_unwind(WgpuTestRenderer::new) {
                Ok(mut r) => {
                    if !self.offen.is_empty() {
                        r.handle_delta(&mut self.offen);
                    }
                    self.offen = egui::TexturesDelta::default();
                    self.renderer = Some(r);
                }
                Err(nutzlast) => {
                    let f = format!("{KEIN_ADAPTER}: {}", panik_text(&*nutzlast));
                    self.fehler = Some(f.clone());
                    return Err(f);
                }
            }
        }
        Ok(self.renderer.as_mut().expect("set above"))
    }
}

impl Drop for Vorschau {
    fn drop(&mut self) {
        // Unapplied deltas must not reach egui's drop check.
        self.offen.clear();
    }
}

impl TestRenderer for Vorschau {
    fn handle_delta(&mut self, delta: &mut egui::TexturesDelta) {
        match &mut self.renderer {
            Some(r) => r.handle_delta(delta),
            None => self.offen.append(std::mem::take(delta)),
        }
    }

    fn render(
        &mut self,
        ctx: &egui::Context,
        output: &egui::FullOutput,
    ) -> Result<image::RgbaImage, String> {
        let r = self.bereit()?;
        catch_unwind(AssertUnwindSafe(|| r.render(ctx, output)))
            .unwrap_or_else(|p| Err(format!("Rendern abgebrochen: {}", panik_text(&*p))))
    }
}

/// The message of a caught panic.
pub fn panik_text(nutzlast: &(dyn std::any::Any + Send)) -> String {
    nutzlast
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| nutzlast.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unbekannter Fehler".into())
}
