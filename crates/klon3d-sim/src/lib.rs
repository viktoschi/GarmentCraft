//! Der Solver von klon3d (Stufe 4): der implizite Schritt mit CG wie CLO als
//! Grundlage und CPU-Referenz, XPBD als Prüfweg (ADR-020); so angelegt, dass
//! eine GPU-Fassung über `wgpu` dieselben Daten rechnet (ADR-018).
//!
//! * [`szene`] — Punkte, Constraints und Körper als Felder, gebaut aus dem
//!   Schnittnetz von `klon3d-mesh` (Befund 33);
//! * [`faerbung`] — Constraints in Farben ohne gemeinsame Punkte;
//! * [`cpu`] — XPBD auf der CPU, `f64`, kleine Unterschritte (Prüfweg);
//! * [`implizit`] — Rückwärts-Euler mit CG wie CLO (Befund 35, Grundlage);
//! * [`klebung`] — Klebezonen: zwei verbundene Lagen haften aneinander
//!   (Versatz gehalten, weiche Tack-Feder, Befund 59);
//! * [`koerper`] — Kollisionskörper: Zylinder und Körpernetz;
//! * [`netzkoerper`] — das Körpernetz: vorzeichenbehafteter Abstand über
//!   Pseudonormalen, Suche im Gitter;
//! * [`platzierung`] — Schnittteile vor dem Nähen um den Körper legen
//!   (Hüllkörper, Lagedatei, Abwicklung);
//! * [`faser`] — die Fadenlaufrichtung je Dreieck im 3D-Netz (Stufe 3);
//! * [`material`] — Stoffwerte aus CLO-Profilen in SI-Einheiten;
//! * [`messung`] — Umfang, Dehnung, Nahtspalt, Energie;
//! * [`abstand`] — Abstandskarte Stoff → Haut: vorzeichenbehaftet je Punkt,
//!   mit Kennzahlen je Teil (Stufe 5);
//! * [`dehnung`] — Dehnungskarte: Hauptdehnungen je Dreieck, mit Kennzahlen je
//!   Teil (Stufe 5);
//! * [`spannung`] - Spannungskarte: Hauptkräfte je Dreieck in N/m, gegen die
//!   10-%-Linie des Materials (Stufe 5);
//! * [`nahtlage`] - Nahtlage: Abstand jeder Naht zu den benannten Körperpunkten
//!   (Achsel, Schulterpunkt, Brustpunkt; Stufe 5);
//! * [`verteilung`] — die Kennzahlen, die beide Karten teilen;
//! * [`farbkarte`] — Wert → Farbe für die Karten, feste Skala je Maß;
//! * [`roehre`] — der Prüffall von Schritt 4a;
//! * [`selbstkollision`] - Stoff gegen Stoff wie CLO (Befund 116):
//!   Nähe-Federn, Durchdringungs-Schub und Stoßprüfung;
//! * [`vergleich`] — zwei Netze gegeneinander messen: Abstand jedes Knotens
//!   zur Fläche des anderen, in beide Richtungen (Stufe 5);
//! * [`vergleichskarte`] — beide Netze mit Farbe je Eckpunkt als OBJ, die
//!   Anzeige der Vergleichskarte (Stufe 5);
//! * [`ausgabe`] — OBJ und Dehnung je Dreieck.

#![allow(non_snake_case)] // Tests sind deutsch benannt

pub mod abstand;
pub mod ausgabe;
pub mod cpu;
pub mod dehnung;
pub mod faerbung;
pub mod faser;
pub mod farbkarte;
pub mod gpu;
pub mod implizit;
pub mod klebung;
pub mod koerper;
pub mod material;
pub mod messung;
pub mod nahtlage;
pub mod netzkoerper;
pub mod platzierung;
pub mod roehre;
pub mod selbstkollision;
pub mod spannung;
pub mod szene;
pub mod vergleich;
pub mod vergleichskarte;
pub mod verteilung;

pub use cpu::{CpuReferenz, Rechenweg};
pub use gpu::GpuLoeser;
pub use implizit::{CgLauf, CpuLoeser, ImplizitCg, ImplizitParameter, Loeser};
pub use koerper::Koerper;
pub use netzkoerper::Koerpernetz;
pub use material::{Material, RADICAL_RUBBER_04, RETIFIST_NR_040};
pub use szene::{Simparameter, Szene, Teilvorgabe, WEICH_FAKTOR};
