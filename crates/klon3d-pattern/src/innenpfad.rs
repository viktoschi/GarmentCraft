//! Inner paths (`<path>`): lines drawn on a piece that are not its seam line.
//!
//! A piece names them through `<iPaths><record path="…"/></iPaths>`; the paths
//! themselves hang in the block's `<modeling>` part as a chain of modelling
//! objects, exactly like the nodes of a piece. In the measured stock (1 464
//! `.sm2d`, 16 844 paths) every path sits under `pattern/draftBlock/modeling`,
//! carries `type="2"`, and has a `<nodes>` element; `cut="true"` appears 234
//! times, `cut` is missing on 6 120 (counted as `false`), and 4 879 of 27 076
//! pieces reference at least one path (1 to 28 each, no reference without a
//! target).
//!
//! ## What this module answers
//!
//! Name, `cut`, open or closed, node count, length and - for a closed path -
//! the area, all in millimetres. The data a glue zone, a decoration or an
//! opening to cut out needs; no mesh is built and no piece is changed.
//!
//! ## Closed means: the resolved ends meet
//!
//! Not "the first and last node name the same object". A real cut-out closes on
//! a *second* node that lands on the same arithmetic point (a head opening built
//! from two reversed arcs and three points ends on a different node id than it
//! starts), so the resolved points decide, within Seamly2D's on-curve tolerance
//! ([`crate::kontur::toleranz`], 0.1555 mm). The repeated closing point is
//! dropped from [`Innenpfad::punkte`], but the segment from the last point back
//! to the first is a drawn piece of the path and counts towards the length.
//!
//! ## Length is the drawn polyline
//!
//! Each node contributes the points [`Auswertung::kurve`] draws (lines exactly,
//! a spline in 48 steps, an arc in 64, a path in 12 per segment); a curve node
//! with `reverse="1"` runs backwards. Consecutive nodes that meet are joined
//! without repeating the joint point. Seamly2D shows and prints these paths as
//! this polyline, so this is the length to compare printed values against; it
//! is not the seam-line length [`Auswertung::kurven_laenge`] measures on the
//! curve decomposition, and for an inner path no compiled formula depends on
//! it.
//!
//! A node whose target cannot be resolved is reported, never dropped silently;
//! the path is then not reported at all, because a chain with a hole in it
//! would have a wrong length and a wrong area.

use crate::geom::{Auswertung, GeomError, Pt};
use crate::model::{InnerPath, Piece};

const KIND: &str = "Innenpfad";

/// An inner path, resolved.
#[derive(Debug, Clone)]
pub struct Innenpfad {
    /// `id` of the `<path>` in the block.
    pub id: String,
    /// `name` of the `<path>`, empty when it has none.
    pub name: String,
    /// `cut="true"`: cut the path out, making a hole.
    pub cut: bool,
    /// Both ends of the resolved chain meet (see the module doc).
    pub geschlossen: bool,
    /// Nodes the path names, including ones that contributed no point.
    pub knoten: usize,
    /// The chain in stored pattern units, y down, without a repeated closing
    /// point. Multiply by [`Auswertung::mm_pro_einheit`] for millimetres.
    pub punkte: Vec<Pt>,
    /// Drawn length in millimetres.
    pub laenge_mm: f64,
    /// Enclosed area in mm²; `Some` only for a closed path.
    pub flaeche_mm2: Option<f64>,
}

impl Auswertung<'_> {
    /// The inner paths a piece names, in the order of its `<iPaths>`, and the
    /// nodes or references that could not be resolved.
    ///
    /// An empty list for a piece without `<iPaths>` (825 of the measured files
    /// have none at all, and pieces without one are the majority: 4 879 of
    /// 27 076 carry paths).
    pub fn innenpfade(&self, stueck: &Piece) -> (Vec<Innenpfad>, Vec<String>) {
        let index = self.modeling_index();
        let tol = crate::kontur::toleranz(self);
        let mut out = Vec::new();
        let mut meldung = Vec::new();
        for block in &stueck.i_paths {
            for r in &block.paths {
                match self.innenpfad(&r.path, &index, tol) {
                    Ok(p) => out.push(p),
                    Err(e) => meldung.push(format!("Innenpfad {}: {e}", r.path)),
                }
            }
        }
        (out, meldung)
    }

    /// Every inner path of the pattern in file order - also those no piece
    /// names. A path that no piece references is not drawn anywhere, but it is
    /// data in the file that a reader should not lose silently.
    pub fn alle_innenpfade(&self) -> Vec<&InnerPath> {
        self.muster()
            .draft_blocks
            .iter()
            .filter_map(|b| b.modeling.as_ref())
            .flat_map(|m| m.paths.iter())
            .collect()
    }

    /// One path by its `id`, resolved. `index` is
    /// [`Auswertung::modeling_index`] - the caller resolves it once for several
    /// paths.
    fn innenpfad(
        &self,
        id: &str,
        index: &std::collections::HashMap<String, (bool, String)>,
        tol: f64,
    ) -> Result<Innenpfad, GeomError> {
        // Path ids are unique per file (measured: no duplicate in 1 464 files),
        // so the lookup does not need to know which block the piece belongs to.
        let pfad = self
            .alle_innenpfade()
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| GeomError::Geometrie {
                kind: KIND,
                grund: format!("kein <path> mit id {id}"),
            })?;

        let mut punkte: Vec<Pt> = Vec::new();
        let mut knoten = 0usize;
        for k in pfad.nodes.as_ref().map(|n| n.items.as_slice()).unwrap_or(&[]) {
            knoten += 1;
            let Some((ist_punkt, ref_id)) = index.get(&k.id_object) else {
                return Err(GeomError::Geometrie {
                    kind: KIND,
                    grund: format!("Knoten {} ohne Modellierziel", k.id_object),
                });
            };
            let mut pts = if *ist_punkt {
                vec![self.punkt(ref_id)?]
            } else {
                self.kurve(ref_id)?
            };
            if matches!(k.reverse.as_deref(), Some("1") | Some("true")) {
                pts.reverse();
            }
            // The next node starts where this one ended; that joint point is
            // the same point twice and would only add a zero-length segment.
            if let (Some(&letzt), Some(&erst)) = (punkte.last(), pts.first()) {
                if letzt.dist(erst) <= tol {
                    pts.remove(0);
                }
            }
            punkte.extend(pts);
        }

        // A ring needs three distinct points plus the closing one.
        let mut geschlossen = false;
        if punkte.len() >= 4 {
            if let (Some(&erst), Some(&letzt)) = (punkte.first(), punkte.last()) {
                geschlossen = erst.dist(letzt) <= tol;
            }
        }
        if geschlossen {
            punkte.pop();
        }

        let f = self.mm_pro_einheit();
        // A ring has one segment more than it has points: the one from the last
        // point back to the first. It is a real drawn piece of the path, so it
        // counts towards the length even though its point is not repeated in
        // `punkte`.
        let mut laenge = Self::polyline_laenge(&punkte);
        if let (true, Some(&erst), Some(&letzt)) = (geschlossen, punkte.first(), punkte.last()) {
            laenge += erst.dist(letzt);
        }
        let laenge_mm = laenge * f;
        let flaeche_mm2 = geschlossen.then(|| flaeche(&punkte) * f * f);
        Ok(Innenpfad {
            id: pfad.id.clone(),
            name: pfad.name.clone().unwrap_or_default(),
            cut: matches!(pfad.cut.as_deref(), Some("true") | Some("1")),
            geschlossen,
            knoten,
            punkte,
            laenge_mm,
            flaeche_mm2,
        })
    }
}

/// Absolute area of a ring in the units of its points (shoelace, closes the
/// ring itself). Used for inner paths only; the seam line's orientation
/// matters elsewhere ([`crate::saumkontur`]) and is not touched here.
fn flaeche(p: &[Pt]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let doppelt: f64 = (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    (doppelt / 2.0).abs()
}
