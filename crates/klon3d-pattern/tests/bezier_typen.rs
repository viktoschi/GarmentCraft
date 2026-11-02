//! `cubicBezier` and `cubicBezierPath`: curves given by their control points
//! (`vpattern.cpp: ParseToolCubicBezier, ParseToolCubicBezierPath`).

use std::collections::HashMap;

use klon3d_pattern::geom::{Auswertung, Pt};
use klon3d_pattern::read_str;

fn muster(rechnung: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
  <version>0.7.4</version>
  <unit>cm</unit>
  <draftBlock name="A">
    <calculation>
{rechnung}
    </calculation>
    <modeling/>
    <pieces/>
  </draftBlock>
</pattern>"#
    )
}

fn gleich(p: Pt, q: Pt) {
    assert!(p.dist(q) < 1e-12, "{p:?} instead of {q:?}");
}

const PUNKTE: &str = r#"<point id="1" name="P0" type="single" x="0" y="0" mx="0" my="0"/>
<point id="2" name="P1" type="single" x="2" y="-3" mx="0" my="0"/>
<point id="3" name="P2" type="single" x="8" y="-3" mx="0" my="0"/>
<point id="4" name="P3" type="single" x="10" y="0" mx="0" my="0"/>
<point id="5" name="P4" type="single" x="12" y="3" mx="0" my="0"/>
<point id="6" name="P5" type="single" x="18" y="3" mx="0" my="0"/>
<point id="7" name="P6" type="single" x="20" y="0" mx="0" my="0"/>"#;

/// The four points are the control points as they stand; `Spl_P0_P3` is the
/// length of that Bézier curve.
#[test]
fn cubic_bezier() {
    let x = muster(&format!(
        r#"{PUNKTE}
<spline id="10" type="cubicBezier" point1="1" point2="2" point3="3" point4="4"/>"#
    ));
    let m = read_str(&x).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let k = ev.kontrollen("10").unwrap();
    assert_eq!(k.len(), 1);
    for (a, b) in k[0].iter().zip(["1", "2", "3", "4"]) {
        gleich(*a, ev.punkt(b).unwrap());
    }
    // symmetric arch: longer than the chord 10, shorter than the control polygon 3.606·2 + 6
    let l = ev.geo("Spl_P0_P3").unwrap();
    assert!(l > 10.0 && l < 2.0 * 13f64.sqrt() + 6.0, "{l}");
}

/// Seven points make two segments, P0–P3 and P3–P6, with P3 shared; the
/// curve passes through P0, P3 and P6 only. Mirror symmetric, so both
/// segments are equally long.
#[test]
fn cubic_bezier_path() {
    let x = muster(&format!(
        r#"{PUNKTE}
<spline id="10" type="cubicBezierPath">
  <pathPoint pSpline="1"/><pathPoint pSpline="2"/><pathPoint pSpline="3"/><pathPoint pSpline="4"/>
  <pathPoint pSpline="5"/><pathPoint pSpline="6"/><pathPoint pSpline="7"/>
</spline>
<spline id="11" type="cubicBezier" point1="1" point2="2" point3="3" point4="4"/>"#
    ));
    let m = read_str(&x).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let k = ev.kontrollen("10").unwrap();
    assert_eq!(k.len(), 2);
    for (s, ids) in k.iter().zip([["1", "2", "3", "4"], ["4", "5", "6", "7"]]) {
        for (a, b) in s.iter().zip(ids) {
            gleich(*a, ev.punkt(b).unwrap());
        }
    }
    assert!(ev.ist_pfad("10"));
    let pfad = ev.geo("SplPath_P0_P6").unwrap();
    let halb = ev.geo("Spl_P0_P3").unwrap();
    assert!((pfad - 2.0 * halb).abs() < 1e-9, "{pfad} vs 2 × {halb}");
}
