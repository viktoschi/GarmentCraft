//! Seam lines cleaned like Seamly2D's `mainPathPoints`: loops cut out
//! (`CheckLoops`), spikes and near duplicates dropped
//! (`CorrectEquidistantPoints`). Expected values by hand.

use std::collections::HashMap;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_str;
use klon3d_pattern::schleifen::{naht_bereinigen, naht_bereinigen_fest, schleifen_entfernen};

/// A run that goes back over itself, as the centre back of L036: up from A
/// past D to C and back down to D, then on. The segment from D sideways
/// touches the first one at D; the run between is cut out.
#[test]
fn backtracking_run() {
    let p = [[0.0, 100.0], [0.0, 40.0], [0.0, 30.0], [0.0, 80.0], [50.0, 80.0], [50.0, 100.0]];
    let (q, karte) = naht_bereinigen(&p);
    assert_eq!(q, vec![[0.0, 100.0], [0.0, 80.0], [50.0, 80.0], [50.0, 100.0]]);
    // B, C and D all stand for the kept D
    assert_eq!(karte, vec![0, 1, 1, 1, 2, 3]);
}

/// A figure eight A–B–C–D: segment A–B crosses C–D at (50, 50); the loop
/// through B and C is cut out and the crossing point takes their place.
#[test]
fn figure_eight() {
    let p = [[0.0, 0.0], [100.0, 100.0], [100.0, 0.0], [0.0, 100.0]];
    let (q, karte) = schleifen_entfernen(&p);
    assert_eq!(q, vec![[0.0, 0.0], [50.0, 50.0], [0.0, 100.0]]);
    assert_eq!(karte, vec![0, 1, 1, 2]);
}

/// An outline without loops, spikes or near duplicates stays as it is,
/// points on a straight run included.
#[test]
fn clean_outline_unchanged() {
    let p = [[0.0, 0.0], [50.0, 0.0], [100.0, 0.0], [100.0, 60.0], [100.0, 100.0], [0.0, 100.0]];
    let (q, karte) = naht_bereinigen(&p);
    assert_eq!(q, p.to_vec());
    assert_eq!(karte, (0..p.len()).collect::<Vec<_>>());
}

/// Points closer than 0.1555 mm (0.588 px) in both coordinates fall
/// together; the later one stands for the earlier.
#[test]
fn near_duplicates() {
    let p = [[0.0, 0.0], [100.0, 0.0], [100.3, 0.2], [100.0, 100.0], [0.0, 100.0]];
    let (q, karte) = naht_bereinigen(&p);
    assert_eq!(q, vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]);
    assert_eq!(karte, vec![0, 1, 1, 2, 3]);
}

/// The point of a point node keeps its place: if only the later of two near
/// points is fixed, it is kept instead of the earlier; two fixed points both
/// stay.
#[test]
fn fixed_points_keep_their_place() {
    let p = [[0.0, 0.0], [100.0, 0.0], [100.3, 0.2], [100.0, 100.0], [0.0, 100.0]];
    let (q, _) = naht_bereinigen_fest(&p, &[false, false, true, false, false]);
    assert_eq!(q, vec![[0.0, 0.0], [100.3, 0.2], [100.0, 100.0], [0.0, 100.0]]);
    let (q, _) = naht_bereinigen_fest(&p, &[false, true, true, false, false]);
    assert_eq!(q, p.to_vec());
}

/// The same backtracking run as a piece: the nodes B and C inside the loop
/// bound no edge any more and drop out; the edge A → D is 2 cm, the
/// perimeter 2 + 5 + 2 + 5 cm.
#[test]
fn loop_nodes_drop_out_of_the_edges() {
    let x = r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
  <version>0.7.4</version>
  <unit>cm</unit>
  <draftBlock name="A">
    <calculation>
<point id="1" name="A" type="single" x="0" y="10" mx="0" my="0"/>
<point id="2" name="B" type="single" x="0" y="4" mx="0" my="0"/>
<point id="3" name="C" type="single" x="0" y="3" mx="0" my="0"/>
<point id="4" name="D" type="single" x="0" y="8" mx="0" my="0"/>
<point id="5" name="E" type="single" x="5" y="8" mx="0" my="0"/>
<point id="6" name="F" type="single" x="5" y="10" mx="0" my="0"/>
    </calculation>
    <modeling>
<point id="11" idObject="1" type="modeling" inUse="true" mx="0" my="0"/>
<point id="12" idObject="2" type="modeling" inUse="true" mx="0" my="0"/>
<point id="13" idObject="3" type="modeling" inUse="true" mx="0" my="0"/>
<point id="14" idObject="4" type="modeling" inUse="true" mx="0" my="0"/>
<point id="15" idObject="5" type="modeling" inUse="true" mx="0" my="0"/>
<point id="16" idObject="6" type="modeling" inUse="true" mx="0" my="0"/>
    </modeling>
    <pieces>
<piece id="100" name="T" seamAllowance="0" version="2" width="0" closed="1">
  <nodes>
    <node idObject="11" type="NodePoint"/><node idObject="12" type="NodePoint"/>
    <node idObject="13" type="NodePoint"/><node idObject="14" type="NodePoint"/>
    <node idObject="15" type="NodePoint"/><node idObject="16" type="NodePoint"/>
  </nodes>
</piece>
    </pieces>
  </draftBlock>
</pattern>"#;
    let m = read_str(x).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let rand = ev.teilrand(&m.pieces()[0], Kurvenquelle::Seamly).unwrap();
    let namen: Vec<&str> = rand.punktknoten.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(namen, ["A", "D", "E", "F"]);
    assert!((rand.kanten[0].laenge - 2.0).abs() < 1e-12, "{}", rand.kanten[0].laenge);
    assert!((rand.umfang() - 14.0).abs() < 1e-12, "{}", rand.umfang());
}
