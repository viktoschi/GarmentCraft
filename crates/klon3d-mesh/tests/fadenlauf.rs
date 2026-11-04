#![allow(non_snake_case)] // Tests are named in German

//! The grainline reaches the mesh: `Teilnetz::fadenlauf` in mm, in the same
//! frame as the mesh points, and drawn in the SVG (#88).
//!
//! The inline pattern is the one from `klon3d-pattern/tests/grainline.rs`: a
//! rectangle with five points, one piece. Its point coordinates are in mm, so a
//! grainline anchored between two points must come back at exactly those
//! coordinates - measured against `Teilnetz::punkte`, not against the axis
//! reader.

use std::collections::HashMap;

use klon3d_mesh::{ausgabe, vernetze, Musternetz, Parameter};
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::Auswertung;

/// Minimal pattern: an enclosing rectangle plus a centre point; 5 construction
/// points, 5 modelled points, one piece. `grainline` is appended verbatim.
fn muster_mit(grainline: &str) -> klon3d_pattern::Pattern {
    let xml = format!(
        r#"<pattern version="0.6.8">
  <unit>mm</unit>
  <measurements/>
  <draftBlock name="A">
    <calculation>
      <point id="1" type="single" x="-60" y="-60"/>
      <point id="2" type="single" x="200" y="-60"/>
      <point id="3" type="single" x="200" y="160"/>
      <point id="4" type="single" x="-60" y="160"/>
      <point id="5" type="single" x="70" y="50"/>
    </calculation>
    <modeling>
      <point id="11" idObject="1" type="modeling"/>
      <point id="12" idObject="2" type="modeling"/>
      <point id="13" idObject="3" type="modeling"/>
      <point id="14" idObject="4" type="modeling"/>
      <point id="15" idObject="5" type="modeling"/>
    </modeling>
  </draftBlock>
  <pieces>
    <piece id="100" name="T" mx="0" my="0">
      {grainline}
      <nodes>
        <node idObject="11" type="NodePoint"/>
        <node idObject="12" type="NodePoint"/>
        <node idObject="13" type="NodePoint"/>
        <node idObject="14" type="NodePoint"/>
      </nodes>
    </piece>
  </pieces>
</pattern>"#
    );
    klon3d_pattern::read_str(&xml).unwrap_or_else(|e| panic!("Muster liest nicht: {e}"))
}

fn netz_mit(grainline: &str) -> Musternetz {
    let muster = muster_mit(grainline);
    let ev = Auswertung::new(&muster, HashMap::new());
    let p = Parameter { kante_mm: 20.0, ..Parameter::default() };
    vernetze(&ev, Kurvenquelle::Seamly, &[], &p).unwrap_or_else(|e| panic!("Netz: {e}"))
}

/// The grainline runs from point 1 (-60,-60) to point 3 (200,160): two corners
/// of the rectangle. The axis in the mesh must sit on those corners.
#[test]
fn Fadenlauf_liegt_auf_den_Musterpunkten() {
    let g = r#"<grainline arrows="0" bottomAnchor="1" topAnchor="3" visible="true"/>"#;
    let n = netz_mit(g);
    let t = n.teil("T").expect("Teil T");
    let f = t.fadenlauf.expect("sichtbarer Fadenlauf fehlt");
    // The piece has four point nodes; two of them are the anchors.
    let knoten: Vec<[f64; 2]> = t.punktknoten.iter().map(|(_, v)| t.punkte[*v as usize]).collect();
    let nahe = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    assert!(knoten.iter().any(|&p| nahe(p, f.p1)), "p1 {:?} ist kein Punktknoten {knoten:?}", f.p1);
    assert!(knoten.iter().any(|&p| nahe(p, f.p2)), "p2 {:?} ist kein Punktknoten {knoten:?}", f.p2);
    // And in millimetres, not in pattern units.
    assert!(nahe(f.p1, [-60.0, -60.0]), "p1 {:?}", f.p1);
    assert!(nahe(f.p2, [200.0, 160.0]), "p2 {:?}", f.p2);
    assert_eq!(f.pfeile, 0);
}

/// An invisible grainline leaves the field empty - the piece is meshed anyway.
#[test]
fn unsichtbarer_Fadenlauf_bleibt_leer() {
    for g in [
        r#"<grainline arrows="0" length="" mx="0" my="0" rotation="" visible="false"/>"#,
        r#"<grainline arrows="0" visible="0"/>"#,
    ] {
        let n = netz_mit(g);
        let t = n.teil("T").expect("Teil T");
        assert_eq!(t.fadenlauf, None, "{g}");
    }
}

/// A visible grainline is drawn: a line inside the piece's group, with the
/// arrowheads Seamly2D asks for.
#[test]
fn svg_zeichnet_den_Fadenlauf() {
    let g = r#"<grainline arrows="1" bottomAnchor="1" topAnchor="3" visible="true"/>"#;
    let n = netz_mit(g);
    let svg = ausgabe::svg(&n, false);
    let zeile = svg
        .lines()
        .find(|l| l.contains("class=\"fadenlauf\""))
        .unwrap_or_else(|| panic!("keine Fadenlauf-Linie im SVG:\n{svg}"));
    // `pfeile` 1: only `p2` carries an arrowhead.
    assert!(zeile.contains("marker-start=\"none\""), "{zeile}");
    assert!(zeile.contains("marker-end=\"url(#pfeil)\""), "{zeile}");
    assert!(svg.contains(r#"<marker id="pfeil""#), "der Marker fehlt");
    // The ends are the two corners, in SVG coordinates (y down as in Seamly2D;
    // here the mesh y equals the pattern y, so the pair appears as it is).
    assert!(zeile.contains("x1=\"-60.000\"") && zeile.contains("y1=\"-60.000\""), "{zeile}");
    assert!(zeile.contains("x2=\"200.000\"") && zeile.contains("y2=\"160.000\""), "{zeile}");
}

/// No grainline, no line - and no stray marker reference in the piece group.
#[test]
fn svg_ohne_Fadenlauf_hat_keine_Linie() {
    let n = netz_mit("");
    let svg = ausgabe::svg(&n, false);
    assert!(!svg.contains("class=\"fadenlauf\""), "{svg}");
}

/// Every grainline in the repository pattern is invisible, so this pattern must
/// keep meshing without an axis on any piece.
#[test]
fn rock_muster_ohne_Fadenlauf() {
    let wurzel = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster");
    let m = klon3d_pattern::read_path(&wurzel.join("rock_2schlitze_v34.sm2d")).unwrap();
    let ev = Auswertung::lade(&m, &wurzel.join("measurements.smis")).unwrap();
    // The pattern has no seam list in the repository; the grainlines are what
    // this test is about, and they are read without one.
    let n = vernetze(&ev, Kurvenquelle::Seamly, &[], &Parameter::default()).unwrap();
    assert!(n.teile.len() >= 2, "Teile: {}", n.teile.len());
    for t in &n.teile {
        assert_eq!(t.fadenlauf, None, "{} hat einen Fadenlauf", t.teil);
    }
}
