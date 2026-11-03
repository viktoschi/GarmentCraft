//! Ausgabe: Endzustand als OBJ (mm), als OBJ mit farbigen Eckpunkten und
//! Dehnung je Dreieck als CSV.

use std::fmt::Write;
use std::ops::Range;

use crate::messung::dehnung_je_dreieck;
use crate::szene::Szene;

/// Wavefront-OBJ in mm, je Schnittteil eine Gruppe.
pub fn obj(s: &Szene) -> String {
    obj_aus(
        &s.x,
        &s.dreiecke,
        s.teile
            .iter()
            .map(|t| (t.name.as_str(), t.dreiecke.clone())),
    )
}

/// The same file from the pieces alone: points in scene coordinates (m), the
/// triangles indexing them, and per piece its name and the range of its
/// triangles.
///
/// [`obj`] is this function with a [`Szene`] broken up, so the format has one
/// place. The application holds a simulation without a scene (its snapshot
/// plus the scene as built) and writes it with the same text.
pub fn obj_aus<'a>(
    x: &[[f64; 3]],
    dreiecke: &[[u32; 3]],
    teile: impl IntoIterator<Item = (&'a str, Range<usize>)>,
) -> String {
    let mut out = String::from("# klon3d-sim, Einheit mm\n");
    for p in x {
        let _ = writeln!(out, "v {:.4} {:.4} {:.4}", p[0] * 1e3, p[1] * 1e3, p[2] * 1e3);
    }
    for (name, bereich) in teile {
        let _ = writeln!(out, "g {}", name.replace(' ', "_"));
        for d in &dreiecke[bereich] {
            let _ = writeln!(out, "f {} {} {}", d[0] + 1, d[1] + 1, d[2] + 1);
        }
    }
    out
}

/// OBJ in mm with a colour attribute on every vertex: `v x y z r g b`.
///
/// `farbe(i)` gives the RGB for scene point `i`. Coordinates, groups and faces
/// are exactly those of [`obj`], so a coloured file and the plain one describe
/// the same geometry — the colours are an extra pair of columns and nothing
/// else. Red, green and blue are 0…255, which every viewer that reads vertex
/// colours understands (and which is what `v x y z r g b` means in the
/// extension, as opposed to the 0…1 of `vertex colors` in some writers).
pub fn obj_farbig(s: &Szene, farbe: impl Fn(usize) -> [u8; 3]) -> String {
    let mut out = String::from("# klon3d-sim, Einheit mm, Farbe je Eckpunkt 0..255\n");
    for (i, p) in s.x.iter().enumerate() {
        let f = farbe(i);
        let _ = writeln!(
            out,
            "v {:.4} {:.4} {:.4} {} {} {}",
            p[0] * 1e3,
            p[1] * 1e3,
            p[2] * 1e3,
            f[0],
            f[1],
            f[2]
        );
    }
    for t in &s.teile {
        let _ = writeln!(out, "g {}", t.name.replace(' ', "_"));
        for d in &s.dreiecke[t.dreiecke.clone()] {
            let _ = writeln!(out, "f {} {} {}", d[0] + 1, d[1] + 1, d[2] + 1);
        }
    }
    out
}

/// Dehnung je Dreieck: `dreieck;teil;groesste;kleinste` (Anteile, nicht %).
pub fn dehnung_csv(s: &Szene) -> String {
    let d = dehnung_je_dreieck(s);
    let mut out = String::from("dreieck;teil;groesste;kleinste\n");
    for t in &s.teile {
        for k in t.dreiecke.clone() {
            let _ = writeln!(out, "{};{};{:.6};{:.6}", k, t.name, d[k].0, d[k].1);
        }
    }
    out
}
