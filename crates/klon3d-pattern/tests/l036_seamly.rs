//! L036 (square neck dress, a third-party pattern in mm) against Seamly2D
//! itself. It is the only pattern at hand with `trueDarts`, `cubicBezier`,
//! `cubicBezierPath` and formulas on the sub-curves of a `curveIntersectAxis`
//! (`SplPath_SKO_M - Spl_SKN_M`).
//!
//! The pattern is not in the repository; the test looks it up through
//! `KLON3D_L036` and skips without it. The measurements are a synthetic body
//! (`tests/muster/l036_synthetic.smis`), not anyone's.
//!
//! Expected values from Seamly2D 2026.9.7 (`seamly2d.exe -f 14
//! --exportOnlyDetails`). Points: a probe copy of the pattern with one piece
//! per point, polygon point–A–B–C with known A, B, C, which fixes the layout
//! transform; all 204 named points agreed to 7e-5 mm. Seam lines: the closed
//! polylines of the export of the pattern itself. Seamly2D 2026.9 crashes on
//! a piece without a `color` attribute, so the exported copy carries one.

use std::path::PathBuf;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;

fn muster() -> Option<PathBuf> {
    let p = std::env::var_os("KLON3D_L036").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"D:\patterns\Latex\seamly2d\mrdooslatexprimer.blog\L036 Square neck dress (pub) rev1.sm2d")
    });
    if p.is_file() {
        Some(p)
    } else {
        eprintln!("übersprungen: L036 fehlt ({}), KLON3D_L036 setzen", p.display());
        None
    }
}

fn masse() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/muster/l036_synthetic.smis"))
}

/// Points that go through the reader paths fixed for this pattern, where
/// Seamly2D puts them (mm, stored orientation, y down).
#[test]
fn points_as_in_seamly2d() {
    let Some(pfad) = muster() else { return };
    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &masse()).unwrap();
    let soll = [
        // cut on a `cubicBezier` (inner control points were read as the origin: 109 mm off)
        ("SKN", -76.6759, 441.6075),
        ("SKR", -413.4521, 443.8222),
        // cut on a `cubicBezierPath` (was read as a curve through every point: 11 mm off)
        ("OF", -173.0007, 194.7684),
        // `trueDarts`
        ("BQ1", -344.3699, 26.1527),
        ("BP3", -338.0782, 27.3813),
        // `curveIntersectAxis`
        ("M", -90.0000, 272.4233),
        ("BBF", -83.6366, 353.6421),
        ("R11", -170.8772, 189.1995),
        ("A29", -485.0000, 29.9614),
        // `pointOfIntersectionCurves` on a `cubicBezierPath`
        ("A28", -193.2900, 220.3801),
        ("R12", -193.6362, 220.2409),
        // length `SplPath_SKO_M - Spl_SKN_M`: sub-curves of the axis point M
        ("diff", 0.0000, 647.3727),
    ];
    for (name, x, y) in soll {
        let id = ev.id_von_name(name).unwrap_or_else(|| panic!("{name} unknown"));
        let p = ev.punkt(id).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!((p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3, "{name}: ({:.4}, {:.4}), Seamly2D ({x}, {y})", p.x, p.y);
    }
}

/// Seam line per piece as in Seamly2D's export. "L036 - back" is left out:
/// its centre back runs down to the hem, up past the zip end and back down,
/// and Seamly2D removes that overlap from the seam line (`CheckLoops`;
/// 2294.16 mm against 2727.4 here). klon3d does not remove loops yet.
#[test]
fn seam_lines_as_in_seamly2d() {
    let Some(pfad) = muster() else { return };
    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &masse()).unwrap();
    let soll = [
        ("6 panel + fillet - fillet", 283.43),
        ("L036 - back side", 2192.33),
        ("L036 - front", 2340.23),
        ("L036 - front side real", 2019.00),
    ];
    let mut geprueft = 0;
    for t in m.pieces() {
        let rand = ev.teilrand(t, Kurvenquelle::Seamly).unwrap();
        let Some(&(_, mm)) = soll.iter().find(|(n, _)| *n == rand.teil) else { continue };
        let ist = rand.in_cm(rand.umfang()) * 10.0;
        assert!((ist - mm).abs() < 0.05, "{}: {ist:.2} mm, Seamly2D {mm:.2} mm", rand.teil);
        geprueft += 1;
    }
    assert_eq!(geprueft, soll.len());
}
