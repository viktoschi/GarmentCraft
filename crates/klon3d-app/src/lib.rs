//! klon3d-app – the application window of stage 6 (see `README.md` in this crate).
//!
//! | Module | Content |
//! |---|---|
//! | [`app`] | `App` (`eframe::App`): state, actions, shortcuts, persistence |
//! | [`anordnung`] | what the window remembers: arrangement, 2D toggles, recent files; fabrics folder |
//! | [`dialoge`] | floating windows: file dialog, error, help, information |
//! | [`fenster`] | the layout: panels, 3D and 2D window with divider, window frames and toolbars |
//! | [`menue`] | menu bar, file toolbar, toolbars of the 3D and 2D window |
//! | [`bibliothek`] | the library panel (file browser at the left) |
//! | [`panele`] | object browser (pieces, seams, fabrics), properties, status bar |
//! | [`ansicht2d`] | the 2D view and its view math |
//! | [`dateiwahl`] | folder listing shared by the file dialog and the library; the file dialog |
//! | [`muster`] | a loaded pattern as plain data, no egui types |
//!
//! The modules are public so that integration tests can build the window without a real one.

pub mod anordnung;
pub mod ansicht2d;
pub mod app;
pub mod bibliothek;
pub mod dateiwahl;
pub mod dialoge;
pub mod fenster;
pub mod menue;
pub mod muster;
pub mod panele;

pub use app::App;
