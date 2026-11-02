//! Sub-curves of `curveIntersectAxis`: Seamly2D splits the curve at the axis
//! point and registers both parts under point id + 1 and + 2, named after
//! their ends (`vtoolcurveintersectaxis.cpp: InitSegments`). Expected values
//! computed by hand.

use std::collections::HashMap;

use klon3d_pattern::geom::{Auswertung, Pt};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_str;
use klon3d_pattern::schneiden::laenge_bis_punkt;

fn muster(rechnung: &str, modell: &str, teile: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
  <version>0.7.4</version>
  <unit>cm</unit>
  <draftBlock name="A">
    <calculation>
{rechnung}
    </calculation>
    <modeling>
{modell}
    </modeling>
    <pieces>
{teile}
    </pieces>
  </draftBlock>
</pattern>"#
    )
}

fn nah(p: Pt, x: f64, y: f64, tol: f64) {
    assert!((p.x - x).abs() < tol && (p.y - y).abs() < tol, "({:.6}, {:.6}) instead of ({x}, {y})", p.x, p.y);
}

/// A straight spline A(0, 0)–B(10, 0) and a straight path A–B–C(20, 0); a
/// vertical axis from above meets the spline at x = 4 and the path at x = 13.
///
/// The spline parts end at the **axis point**, so they are exactly 4 and 6
/// long — unlike a `cutSpline`, whose cut sits within 0.1 % of the length.
/// The path parts meet at the cut point of `CutSplinePath`, which lies on the
/// same line within that 0.1 %. Ids 11/12 and 21/22 are the sub-curves.
#[test]
fn spline_and_path() {
    let x = muster(
        r#"<point id="1" name="A" type="single" x="0" y="0" mx="0" my="0"/>
<point id="2" name="B" type="single" x="10" y="0" mx="0" my="0"/>
<point id="3" name="C" type="single" x="20" y="0" mx="0" my="0"/>
<point id="4" name="P" type="single" x="4" y="-5" mx="0" my="0"/>
<point id="5" name="Q" type="single" x="13" y="-5" mx="0" my="0"/>
<spline id="6" type="simpleInteractive" point1="1" point4="2" angle1="0" angle2="180" length1="0" length2="0"/>
<spline id="7" type="pathInteractive">
  <pathPoint pSpline="1" angle1="180" angle2="0" length1="0" length2="0"/>
  <pathPoint pSpline="2" angle1="180" angle2="0" length1="0" length2="0"/>
  <pathPoint pSpline="3" angle1="180" angle2="0" length1="0" length2="0"/>
</spline>
<point id="10" name="X" type="curveIntersectAxis" curve="6" basePoint="4" angle="270" mx="0" my="0"/>
<point id="20" name="M" type="curveIntersectAxis" curve="7" basePoint="5" angle="270" mx="0" my="0"/>
<point id="30" name="L1" type="endLine" basePoint="1" angle="0" length="Spl_A_X" mx="0" my="0"/>
<point id="31" name="L2" type="endLine" basePoint="1" angle="0" length="Spl_X_B" mx="0" my="0"/>"#,
        "",
        "",
    );
    let m = read_str(&x).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    nah(ev.punkt("10").unwrap(), 4.0, 0.0, 1e-9);
    nah(ev.punkt("20").unwrap(), 13.0, 0.0, 1e-9);

    assert!((ev.geo("Spl_A_X").unwrap() - 4.0).abs() < 1e-9);
    assert!((ev.geo("Spl_X_B").unwrap() - 6.0).abs() < 1e-9);
    nah(ev.punkt("30").unwrap(), 4.0, 0.0, 1e-9);
    nah(ev.punkt("31").unwrap(), 6.0, 0.0, 1e-9);
    let k = ev.kontrollen("11").unwrap();
    assert_eq!(k.len(), 1);
    nah(k[0][0], 0.0, 0.0, 1e-12);
    nah(k[0][3], 4.0, 0.0, 1e-12);
    nah(ev.kontrollen("12").unwrap()[0][0], 4.0, 0.0, 1e-12);
    assert!(!ev.ist_pfad("11"));

    let a = ev.geo("SplPath_A_M").unwrap();
    let b = ev.geo("SplPath_M_C").unwrap();
    assert!((a - 13.0).abs() < 0.013 && (a + b - 20.0).abs() < 1e-9, "{a} + {b}");
    assert!(ev.ist_pfad("21") && ev.ist_pfad("22"));
    // two segments in the first part (A–B and B–cut), one in the second
    assert_eq!(ev.kontrollen("21").unwrap().len(), 2);
    assert_eq!(ev.kontrollen("22").unwrap().len(), 1);
    // `Spl_` does not name a path part
    assert!(ev.geo("Spl_A_M").is_err());
}

/// An arc around C(0, 0) with radius 10 from 0° to 90° and an axis from C
/// at 30°: the point lies on the arc's polyline (a chord, just inside the
/// circle), and the arc is cut at the polyline length up to it — 10·π/6 to
/// the precision of the polyline. Both sub-arcs (ids 11, 12) add up to the
/// whole arc, 5π, exactly.
#[test]
fn arc() {
    let x = muster(
        r#"<point id="1" name="C" type="single" x="0" y="0" mx="0" my="0"/>
<arc id="2" type="simple" center="1" radius="10" angle1="0" angle2="90"/>
<point id="10" name="X" type="curveIntersectAxis" curve="2" basePoint="1" angle="30" mx="0" my="0"/>"#,
        "",
        "",
    );
    let m = read_str(&x).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let p = ev.punkt("10").unwrap();
    let w = 30f64.to_radians();
    assert!((p.dist(Pt::new(0.0, 0.0)) - 10.0).abs() < 1e-3, "{p:?}");
    nah(p, 10.0 * w.cos(), -10.0 * w.sin(), 1e-3);
    let a = ev.geo("Arc_C_11").unwrap();
    let b = ev.geo("Arc_C_12").unwrap();
    assert!((a - 10.0 * std::f64::consts::PI / 6.0).abs() < 1e-3, "{a}");
    assert!((a + b - 5.0 * std::f64::consts::PI).abs() < 1e-9, "{a} + {b}");
}

/// The sub-curve as a piece node: triangle A → (spline part A–X) → X → P.
/// Perimeter 4 + 5 + √41.
#[test]
fn sub_curve_as_piece_node() {
    let x = muster(
        r#"<point id="1" name="A" type="single" x="0" y="0" mx="0" my="0"/>
<point id="2" name="B" type="single" x="10" y="0" mx="0" my="0"/>
<point id="4" name="P" type="single" x="4" y="-5" mx="0" my="0"/>
<spline id="6" type="simpleInteractive" point1="1" point4="2" angle1="0" angle2="180" length1="0" length2="0"/>
<point id="10" name="X" type="curveIntersectAxis" curve="6" basePoint="4" angle="270" mx="0" my="0"/>"#,
        r#"<point id="40" idObject="1" type="modeling" inUse="true" mx="0" my="0"/>
<spline id="41" idObject="11" type="modelingSpline" inUse="true"/>
<point id="42" idObject="10" type="modeling" inUse="true" mx="0" my="0"/>
<point id="43" idObject="4" type="modeling" inUse="true" mx="0" my="0"/>"#,
        r#"<piece id="100" name="T" seamAllowance="0" version="2" width="0" closed="1">
  <nodes>
    <node idObject="40" type="NodePoint"/>
    <node idObject="41" type="NodeSpline" reverse="0"/>
    <node idObject="42" type="NodePoint"/>
    <node idObject="43" type="NodePoint"/>
  </nodes>
</piece>"#,
    );
    let m = read_str(&x).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let rand = ev.teilrand(&m.pieces()[0], Kurvenquelle::Seamly).unwrap();
    let soll = 4.0 + 5.0 + 41f64.sqrt();
    assert!((rand.in_cm(rand.umfang()) - soll).abs() < 1e-9, "{}", rand.umfang());
}

/// `GetLengthByPoint`: up to the last segment that holds the point; 0 when
/// the start rounds to the same whole pixel, the whole length when the end
/// does; `None` beside the polyline.
#[test]
fn length_by_point() {
    let zug = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    assert_eq!(laenge_bis_punkt(&zug, [4.0, 0.0]), Some(4.0));
    assert_eq!(laenge_bis_punkt(&zug, [10.0, 3.0]), Some(13.0));
    assert_eq!(laenge_bis_punkt(&zug, [0.3, 0.2]), Some(0.0));
    assert_eq!(laenge_bis_punkt(&zug, [0.4, 9.7]), Some(30.0));
    assert_eq!(laenge_bis_punkt(&zug, [5.0, 5.0]), None);
    // A point on two segments counts on the later one (searched from the end).
    let hin_und_her = [[0.0, 0.0], [10.0, 0.0], [2.0, 0.0]];
    assert_eq!(laenge_bis_punkt(&hin_und_her, [5.0, 0.0]), Some(15.0));
}
