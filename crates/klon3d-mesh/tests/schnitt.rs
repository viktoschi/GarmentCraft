//! The cutting line inside the mesh, checked against the path that already
//! produced it.
//!
//! Three patterns, every piece: the ring the mesh now carries must be the same
//! ring the DXF export writes and `klon3d umriss` prints. Two paths through the
//! code, one number - so a wrong unit (`Teilrand::zugabe` is in pattern units,
//! the mesh in millimetres) or a ring built from the wrong `Teilrand` fails
//! here instead of quietly reaching a caller.
//!
//! The expected values are the `Schnitt` column of `klon3d umriss` on the same
//! patterns, to the 0.1 mm it prints.

use std::path::PathBuf;

use klon3d_mesh::schnitt::{hat_zugabe, ringlaenge};
use klon3d_mesh::{vernetze, Musternetz, Parameter};
use klon3d_pattern::geom::{Auswertung, Pt};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_path;

fn wurzel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

/// The mesh of a pattern without a seam list: this test is about the cutting
/// line of a piece, not about how pieces are sewn.
fn netz(muster: &str) -> Musternetz {
    let m = read_path(&wurzel().join(muster)).unwrap();
    let ev = Auswertung::lade(&m, &wurzel().join("measurements.smis")).unwrap();
    vernetze(&ev, Kurvenquelle::Seamly, &[], &Parameter::default()).unwrap()
}

/// The ring of `teil`, or a panic naming the piece.
fn ring<'a>(netz: &'a Musternetz, teil: &str) -> &'a [[f64; 2]] {
    let t = netz
        .teile
        .iter()
        .find(|t| t.teil.ends_with(teil))
        .unwrap_or_else(|| panic!("kein Teil {teil}"));
    &t.schnitt
}

/// The signed area of a closed ring; the sign is the winding.
fn flaeche(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

/// Length of an open polyline that is closed by `umriss` itself.
fn nahtlinie(ring: &[Pt], mm: f64) -> f64 {
    let mut s = 0.0;
    for i in 0..ring.len() {
        s += ring[i].dist(ring[(i + 1) % ring.len()]);
    }
    s * mm
}

/// Bluse 2: 12 of 22 pieces carry an allowance, the rest do not. The three
/// numbers are the `Schnitt` column of `klon3d umriss bluse2_v26.sm2d`.
#[test]
fn bluse2_ring_trifft_die_schnittspalte() {
    let n = netz("bluse2_v26.sm2d");
    let faelle = [
        ("Rueckenteil Mitte Links", 1097.0),
        ("Vorderteil Seite Links", 1091.6),
        ("Aermel Kurz Links", 872.3),
    ];
    for (teil, soll) in faelle {
        let r = ring(&n, teil);
        assert!(!r.is_empty(), "{teil}: kein Ring, obwohl das Teil Zugabe hat");
        let ist = ringlaenge(r);
        assert!(
            (ist - soll).abs() <= 0.1,
            "{teil}: Schnittlinie {ist:.2} mm, erwartet {soll:.1} mm"
        );
    }
}

/// Bluse 3: cups, bands and the collar, which are the pieces whose allowance
/// runs around a curve.
#[test]
fn bluse3_ring_trifft_die_schnittspalte() {
    let n = netz("bluse3_v2.sm2d");
    let faelle = [
        ("Rueckenteil Mitte Links", 1092.3),
        ("Untercup Links", 569.4),
        ("Band Mitte", 734.0),
        ("Aermel Kurz Links", 861.2),
    ];
    for (teil, soll) in faelle {
        let r = ring(&n, teil);
        assert!(!r.is_empty(), "{teil}: kein Ring, obwohl das Teil Zugabe hat");
        let ist = ringlaenge(r);
        assert!(
            (ist - soll).abs() <= 0.1,
            "{teil}: Schnittlinie {ist:.2} mm, erwartet {soll:.1} mm"
        );
    }
}

/// Rock v34: a long piece and the folded waistband, which is the one whose
/// allowance sits on a 180° turn.
#[test]
fn rock_ring_trifft_die_schnittspalte() {
    let n = netz("rock_2schlitze_v34.sm2d");
    let faelle = [
        ("1 BUND - Umschlag, ganzer Umfang, Naht hinten", 1526.4),
        ("2 VORNE RECHTS - obere Lage an CF", 2557.0),
        ("4 HINTEN RECHTS - obere Lage an CB", 2847.4),
    ];
    for (teil, soll) in faelle {
        let r = ring(&n, teil);
        assert!(!r.is_empty(), "{teil}: kein Ring, obwohl das Teil Zugabe hat");
        let ist = ringlaenge(r);
        assert!(
            (ist - soll).abs() <= 0.1,
            "{teil}: Schnittlinie {ist:.2} mm, erwartet {soll:.1} mm"
        );
    }
}

/// A piece without an allowance gets an empty ring, never a copy of its seam
/// line: a copy would let a caller believe there is an allowance of zero, which
/// is a different statement.
#[test]
fn ohne_zugabe_leerer_ring() {
    let n = netz("bluse2_v26.sm2d");
    // The facings declare `width="0"` and behave like `seamAllowance="false"`;
    // `klon3d umriss` prints no `Schnitt` column for them.
    for teil in ["Scheibe Saum Links", "Halbe Scheibe Kragenende Links"] {
        let r = ring(&n, teil);
        assert!(r.is_empty(), "{teil}: Ring mit {} Punkten, obwohl ohne Zugabe", r.len());
    }
}

/// The ring lies outside the seam line: longer, and enclosing a larger area.
/// A ring mirrored onto the seam line, emptied or collapsed fails here.
#[test]
fn ring_liegt_aussen_um_die_nahtlinie() {
    for muster in ["bluse2_v26.sm2d", "bluse3_v2.sm2d", "rock_2schlitze_v34.sm2d"] {
        let m = read_path(&wurzel().join(muster)).unwrap();
        let ev = Auswertung::lade(&m, &wurzel().join("measurements.smis")).unwrap();
        let n = netz(muster);
        let mut geprueft = 0usize;
        for t in n.teile.iter() {
            if t.schnitt.is_empty() {
                continue;
            }
            // The seam line read again from the pattern, in mm, independent of
            // how the mesh ordered its own points. Looked up by name, not by
            // index: a piece the mesher skipped would otherwise shift the
            // comparison onto its neighbour.
            let teile = m.pieces().to_vec();
            let teil = teile
                .iter()
                .find(|p| p.name.as_deref().map_or_else(|| p.id == t.teil, |n| n == t.teil))
                .unwrap_or_else(|| panic!("{muster}: kein Musterteil zu {}", t.teil));
            let rand = ev.teilrand(teil, Kurvenquelle::Seamly).unwrap();
            assert!(hat_zugabe(&rand), "{muster} {}: Ring ohne Zugabe im Teil", t.teil);
            let mm = rand.mm_pro_einheit;
            let naht = nahtlinie(&rand.umriss, mm);

            let laenge_schnitt = ringlaenge(&t.schnitt);
            assert!(
                laenge_schnitt > naht + 1e-6,
                "{} {}: Schnittlinie {laenge_schnitt:.3} mm nicht länger als die Nahtlinie {naht:.3} mm",
                muster,
                t.teil
            );
            let flaeche_schnitt = flaeche(&t.schnitt);
            let flaeche_naht = flaeche_umriss(&rand.umriss);
            // Same winding, so the contour follows the piece instead of being
            // mirrored onto it; and strictly more area inside.
            assert!(
                (flaeche_schnitt > 0.0) == (flaeche_naht > 0.0),
                "{muster} {}: Schnittlinie läuft anders herum als die Nahtlinie ({flaeche_schnitt:.3} gegen {flaeche_naht:.3} mm²)",
                t.teil
            );
            assert!(
                flaeche_schnitt.abs() > flaeche_naht.abs(),
                "{muster} {}: Schnittlinie umschließt {:.3} mm², die Nahtlinie {:.3} mm²",
                t.teil,
                flaeche_schnitt.abs(),
                flaeche_naht.abs()
            );
            geprueft += 1;
        }
        assert!(geprueft > 0, "{muster}: kein Teil mit Schnittlinie geprüft");
    }
}

/// The area of the seam line in mm², for the comparison above.
fn flaeche_umriss(umriss: &[Pt]) -> f64 {
    if umriss.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..umriss.len() {
        let a = umriss[i];
        let b = umriss[(i + 1) % umriss.len()];
        s += a.x * b.y - b.x * a.y;
    }
    0.5 * s
}
