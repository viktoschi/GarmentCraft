//! The seam allowance contour reached from a piece, not from a hand-built edge
//! list.
//!
//! `tests/nahtzugabe.rs` pins the arithmetic: the run is linear in the
//! accumulated arc length, the contour is a displacement per point, and at the
//! two ends of the ring one extra step applies at a right angle. This file
//! checks the joint: a `Teilrand` with its node allowances goes in, the
//! displaced segments come out.
//!
//! The pieces are written here as values, not loaded from a `.sm2d`: the point
//! is the arithmetic, and a pattern file would only add parsing to that.

use klon3d_pattern::geom::Pt;
use klon3d_pattern::kanten::{Punktknoten, Randkante, Teilrand};
use klon3d_pattern::saumkontur::{Ecken, Saumkontur};

/// The run values are compared with this tolerance: a run of a third produces
/// values that are not exact in `f64`, everything else is.
const TOL: f64 = 1e-12;

fn pt(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

fn assert_near(p: Pt, x: f64, y: f64) {
    assert!(
        (p.x - x).abs() <= TOL && (p.y - y).abs() <= TOL,
        "expected ({x}, {y}), got ({}, {})",
        p.x,
        p.y
    );
}

fn knoten(vor: Option<f64>, nach: Option<f64>, stelle: usize) -> Punktknoten {
    Punktknoten {
        name: format!("P{stelle}"),
        id: format!("p{stelle}"),
        knoten_nr: stelle,
        kerbe: false,
        zugabe_vor: vor,
        zugabe_nach: nach,
        stelle,
    }
}

fn kante(von: usize, bis: usize, punkte: Vec<Pt>) -> Randkante {
    let laenge = punkte.windows(2).map(|w| w[0].dist(w[1])).sum();
    Randkante {
        von,
        bis,
        punkte,
        laenge,
    }
}

fn teilrand(umriss: Vec<Pt>, punktknoten: Vec<Punktknoten>, kanten: Vec<Randkante>) -> Teilrand {
    Teilrand {
        teil: "Test".to_string(),
        umriss,
        punktknoten,
        kanten,
        mm_pro_einheit: 10.0,
    }
}

/// Square 10 × 10 at the origin, one point node per corner, allowance 1 at
/// every node. The ring runs (0,0) → (10,0) → (10,10) → (0,10), so the normal
/// turned right of the direction of travel is the outward normal of every
/// edge - the bottom edge gets `(0, −1)`.
fn quadrat() -> Teilrand {
    let ecken = [pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)];
    teilrand(
        ecken.to_vec(),
        (0..4).map(|i| knoten(Some(1.0), Some(1.0), i)).collect(),
        (0..4)
            .map(|i| kante(i, (i + 1) % 4, vec![ecken[i], ecken[(i + 1) % 4]]))
            .collect(),
    )
}

#[test]
fn a_square_of_allowance_one_moves_out_by_one() {
    let k = Saumkontur::aus_teilrand(&quadrat(), Ecken::Eigen);
    let s = k.segmente();

    // The same numbers as `a_right_angle_at_the_end_of_the_run_gets_the_diagonal`
    // in `tests/nahtzugabe.rs`: the closing corner gets the diagonal, the three
    // middle corners stay split.
    assert_eq!(s.len(), 4);
    assert_eq!(s[0], (pt(-1.0, -1.0), pt(10.0, -1.0)));
    assert_eq!(s[1], (pt(11.0, 0.0), pt(11.0, 10.0)));
    assert_eq!(s[2], (pt(10.0, 11.0), pt(0.0, 11.0)));
    assert_eq!(s[3], (pt(-1.0, 10.0), pt(-1.0, -1.0)));

    // Both ends of the ring are the same point here, because the closing corner
    // is a right angle and the extra step closes it.
    let p = k.punkte();
    assert_eq!(p.len(), 8);
    assert_eq!(p[0], pt(-1.0, -1.0));
    assert_eq!(p[0], p[7]);
}

#[test]
fn the_two_fields_of_a_node_blend_crossed_or_not_at_all() {
    // The first node gets `before = 1, after = 2`, the three others keep 1.
    let mut rand = quadrat();
    rand.punktknoten[0].zugabe_vor = Some(1.0);
    rand.punktknoten[0].zugabe_nach = Some(2.0);

    // `Gekreuzt`: 1 + (2 − 1)·0.5 = 1.5 at the node, on both sides of it.
    let k = Saumkontur::aus_teilrand(&rand, Ecken::Gekreuzt);
    let s = k.segmente();
    assert_eq!(s[0].0, pt(-1.5, -1.5), "the closing corner takes the blend");
    assert_eq!(s[3].1, pt(-1.5, -1.5), "from the other side as well");
    assert_eq!(s[0].1, pt(10.0, -1.0), "the other corners stay at 1");
    assert_eq!(s[1].0, pt(11.0, 0.0));
    assert_eq!(s[2].0, pt(10.0, 11.0));

    // `Eigen`: the node's own `after` = 2, the `before` = 1 is dropped. That is
    // the step at a node which this module deliberately does not model.
    let k = Saumkontur::aus_teilrand(&rand, Ecken::Eigen);
    let s = k.segmente();
    assert_eq!(s[0].0, pt(-2.0, -2.0));
    assert_eq!(s[3].1, pt(-2.0, -2.0));
    assert_eq!(s[0].1, pt(10.0, -1.0));
}

/// Rectangle 10 × 10 whose bottom edge is a polyline with points at
/// `x = 0, 1, 3, 6, 10`, so four segments of length 1, 2, 3 and 4. Node 0
/// carries 1, node 1 carries 2.
fn unterkante_vier_segmente() -> Teilrand {
    let unten = [
        pt(0.0, 0.0),
        pt(1.0, 0.0),
        pt(3.0, 0.0),
        pt(6.0, 0.0),
        pt(10.0, 0.0),
    ];
    let (rechts_oben, links_oben) = (pt(10.0, 10.0), pt(0.0, 10.0));
    let mut umriss = unten.to_vec();
    umriss.push(rechts_oben);
    umriss.push(links_oben);
    teilrand(
        umriss,
        vec![
            knoten(Some(1.0), Some(1.0), 0),
            knoten(Some(2.0), Some(2.0), 4),
            knoten(Some(1.0), Some(1.0), 5),
            knoten(Some(1.0), Some(1.0), 6),
        ],
        vec![
            kante(0, 1, unten.to_vec()),
            kante(1, 2, vec![unten[4], rechts_oben]),
            kante(2, 3, vec![rechts_oben, links_oben]),
            kante(3, 0, vec![links_oben, unten[0]]),
        ],
    )
}

#[test]
fn the_run_of_an_edge_follows_its_arc_length() {
    let k = Saumkontur::aus_teilrand(&unterkante_vier_segmente(), Ecken::Eigen);
    let s = k.segmente();
    assert_eq!(s.len(), 7, "four segments below, one for each other edge");

    // 1 → 2 over four segments of length 1, 2, 3 and 4: by arc length the
    // values are 1, 1.1, 1.3, 1.6, 2 - the numbers of
    // `run_counts_length_not_points`, now reached from a piece.
    let x = [0.0, 1.0, 3.0, 6.0, 10.0];
    let y = [-1.0, -1.1, -1.3, -1.6, -2.0];

    // The first bottom point is also the start of the ring, so the extra step
    // applies: from (0, −1) along the neighbouring edge to (−1, −1).
    assert_near(s[0].0, -1.0, -1.0);
    assert_near(s[0].1, x[1], y[1]);
    for i in 1..4 {
        assert_near(s[i].0, x[i], y[i]);
        assert_near(s[i].1, x[i + 1], y[i + 1]);
    }
}

/// Rectangle 3 × 10 whose bottom edge has three segments (`x = 0, 1, 2, 3`),
/// node 0 at 1 and node 1 at 3.
fn unterkante_drei_segmente() -> Teilrand {
    let unten = [pt(0.0, 0.0), pt(1.0, 0.0), pt(2.0, 0.0), pt(3.0, 0.0)];
    let (rechts_oben, links_oben) = (pt(3.0, 10.0), pt(0.0, 10.0));
    let mut umriss = unten.to_vec();
    umriss.push(rechts_oben);
    umriss.push(links_oben);
    teilrand(
        umriss,
        vec![
            knoten(Some(1.0), Some(1.0), 0),
            knoten(Some(3.0), Some(3.0), 3),
            knoten(Some(1.0), Some(1.0), 4),
            knoten(Some(1.0), Some(1.0), 5),
        ],
        vec![
            kante(0, 1, unten.to_vec()),
            kante(1, 2, vec![unten[3], rechts_oben]),
            kante(2, 3, vec![rechts_oben, links_oben]),
            kante(3, 0, vec![links_oben, unten[0]]),
        ],
    )
}

#[test]
fn below_four_segments_the_run_is_skipped() {
    let k = Saumkontur::aus_teilrand(&unterkante_drei_segmente(), Ecken::Eigen);
    let s = k.segmente();
    assert_eq!(s.len(), 6, "three segments below, one for each other edge");

    // Three segments are too few, so the run is skipped and the interior
    // points keep the start value 1.
    assert_near(s[0].0, -1.0, -1.0);
    assert_near(s[0].1, 1.0, -1.0);
    assert_near(s[1].0, 1.0, -1.0);
    assert_near(s[1].1, 2.0, -1.0);

    // The far node still carries its own value 3 - that step is not modelled,
    // it is exactly what a caller has to decide about.
    assert_near(s[2].0, 2.0, -1.0);
    assert_near(s[2].1, 3.0, -3.0);
    assert_near(s[3].0, 6.0, 0.0);
}

#[test]
fn a_piece_without_allowance_stays_at_its_seam_line() {
    let mut rand = quadrat();
    for k in rand.punktknoten.iter_mut() {
        k.zugabe_vor = None;
        k.zugabe_nach = None;
    }
    let k = Saumkontur::aus_teilrand(&rand, Ecken::Eigen);
    let s = k.segmente();
    assert_eq!(s.len(), 4);
    for (i, (von, bis)) in s.iter().enumerate() {
        let e = &rand.kanten[i];
        assert_eq!(*von, e.punkte[0], "segment {i} start does not move");
        assert_eq!(
            *bis,
            *e.punkte.last().unwrap(),
            "segment {i} end does not move"
        );
    }
}

#[test]
fn the_winding_decides_which_side_is_outside() {
    // The same square traversed the other way round, allowance 1: every
    // returned point has to lie outside the square.
    let ecken = [pt(0.0, 0.0), pt(0.0, 10.0), pt(10.0, 10.0), pt(10.0, 0.0)];
    let rand = teilrand(
        ecken.to_vec(),
        (0..4).map(|i| knoten(Some(1.0), Some(1.0), i)).collect(),
        (0..4)
            .map(|i| kante(i, (i + 1) % 4, vec![ecken[i], ecken[(i + 1) % 4]]))
            .collect(),
    );
    let k = Saumkontur::aus_teilrand(&rand, Ecken::Eigen);
    for p in k.punkte() {
        let aussen = p.x < -1e-9 || p.x > 10.0 + 1e-9 || p.y < -1e-9 || p.y > 10.0 + 1e-9;
        assert!(aussen, "({}, {}) lies inside the square", p.x, p.y);
    }
    // The closing corner closes on the outside too, not on the inside where a
    // wrong sign would put it.
    assert_eq!(k.punkte()[0], pt(-1.0, -1.0));
}

#[test]
fn a_piece_without_edges_gives_an_empty_contour() {
    let rand = teilrand(Vec::new(), Vec::new(), Vec::new());
    let k = Saumkontur::aus_teilrand(&rand, Ecken::Eigen);
    assert!(k.segmente().is_empty());
    assert!(k.punkte().is_empty());
}
