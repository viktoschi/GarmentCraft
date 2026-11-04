//! Seam allowance as a run and as a contour, checked number by number.
//!
//! The values come from the two findings of 2026-11-03, not from a guess: the
//! run is linear in the accumulated arc length, the first point gets the start
//! value, the last the end value. The contour is one displacement per point,
//! with the crossed 0.5 averaging at a point node and the extra step at the two
//! ends of the run.
//!
//! The signs matter: a rule built from the magnitudes of the two allowances
//! would also hit the right numbers for positive values and would miss the end
//! point for negative ones. Therefore the run is checked with a sign change too.

use klon3d_pattern::geom::Pt;
use klon3d_pattern::nahtzugabe::{
    endwert, kontur, mitte, normiere, startwert, verlauf, Abstaende, Saumkante, MINDESTKANTEN,
    TOLERANZ,
};

/// Points of a run are compared exactly where the arithmetic is exact in
/// `f64`; the 45° direction needs this tolerance.
const TOL: f64 = 1e-12;

fn assert_near(p: Pt, x: f64, y: f64) {
    assert!(
        (p.x - x).abs() <= TOL && (p.y - y).abs() <= TOL,
        "expected ({x}, {y}), got ({}, {})",
        p.x,
        p.y
    );
}

fn pt(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

/// Square with side `s`, lower left corner `unten_links`, run
/// counter-clockwise. The outward normal of an edge is its direction turned
/// `(dy, −dx)`, so the piece lies to the left of every edge.
fn square(unten_links: Pt, s: f64) -> Vec<Saumkante> {
    let (x, y) = (unten_links.x, unten_links.y);
    vec![
        Saumkante::new(pt(x, y), pt(x + s, y), pt(0.0, -1.0)),
        Saumkante::new(pt(x + s, y), pt(x + s, y + s), pt(1.0, 0.0)),
        Saumkante::new(pt(x + s, y + s), pt(x, y + s), pt(0.0, 1.0)),
        Saumkante::new(pt(x, y + s), pt(x, y), pt(-1.0, 0.0)),
    ]
}

#[test]
fn run_is_linear_in_the_arc_length() {
    // Four equal segments: 1.0 → 2.0 in quarters.
    let p: Vec<Pt> = (0..=4).map(|i| pt(i as f64, 0.0)).collect();
    let o = verlauf(&p, 1.0, 2.0).unwrap();
    assert_eq!(o, vec![1.0, 1.25, 1.5, 1.75, 2.0]);
    assert_eq!(o[0], 1.0, "the first point gets the start value exactly");
    assert_eq!(o[4], 2.0, "the last point hits the end value");
}

#[test]
fn run_counts_length_not_points() {
    // Points at 0, 1, 3, 6, 10 - total length 10. By index the values would be
    // quarters, by arc length they are not.
    let p = vec![
        pt(0.0, 0.0),
        pt(1.0, 0.0),
        pt(3.0, 0.0),
        pt(6.0, 0.0),
        pt(10.0, 0.0),
    ];
    let o = verlauf(&p, 1.0, 2.0).unwrap();
    let soll = [1.0, 1.1, 1.3, 1.6, 2.0];
    for (i, wert) in soll.iter().enumerate() {
        assert!(
            (o[i] - wert).abs() <= TOL,
            "point {i}: expected {wert}, got {}",
            o[i]
        );
    }
}

#[test]
fn run_hits_the_end_value_across_a_sign_change() {
    // Computed from the magnitudes this would give 0.0 for −1 → −2: the end
    // point is the probe the earlier reading failed.
    let p: Vec<Pt> = (0..=4).map(|i| pt(i as f64, 0.0)).collect();
    let o = verlauf(&p, -1.0, -2.0).unwrap();
    assert_eq!(o[0], -1.0);
    assert_eq!(o[4], -2.0, "−1 + (−2 − (−1))·1 = −2, not 0");

    let o = verlauf(&p, 0.0, -1.0).unwrap();
    assert_eq!(o[0], 0.0);
    assert_eq!(o[4], -1.0);
}

#[test]
fn fewer_than_four_segments_give_no_run() {
    assert_eq!(MINDESTKANTEN, 4);
    let drei: Vec<Pt> = (0..=3).map(|i| pt(i as f64, 0.0)).collect();
    assert_eq!(verlauf(&drei, 1.0, 2.0), None, "three segments are too few");
    let vier: Vec<Pt> = (0..=4).map(|i| pt(i as f64, 0.0)).collect();
    assert!(verlauf(&vier, 1.0, 2.0).is_some(), "four segments suffice");
}

#[test]
fn no_length_gives_no_run() {
    let p = vec![pt(0.0, 0.0); 5];
    assert_eq!(verlauf(&p, 1.0, 2.0), None);
}

#[test]
fn mitte_is_the_half_blend() {
    assert_eq!(mitte(1.0, 2.0), 1.5);
    assert_eq!(mitte(2.0, 1.0), 1.5);
    assert_eq!(mitte(1.0, 1.0), 1.0);
}

#[test]
fn corner_values_are_averaged_crossed() {
    // Start: own allowance + neighbour's sub value.
    assert_eq!(startwert(1.0, Some(2.0)), 1.5);
    // End: own sub value + neighbour's allowance.
    assert_eq!(endwert(1.0, Some(2.0)), 1.5);
    // Without the smoothing markers or without a neighbour object: the own
    // value, unchanged.
    assert_eq!(startwert(1.0, None), 1.0);
    assert_eq!(endwert(2.0, None), 2.0);
}

#[test]
fn normalisation_falls_back_to_one_one() {
    assert_near(normiere(pt(0.0, 3.0)), 0.0, 1.0);
    // Below the tolerance the reference writes (1, 1), not (0, 0).
    assert_eq!(normiere(pt(1e-9, 0.0)), pt(1.0, 1.0));
    assert!(1e-9 < TOLERANZ);
}

#[test]
fn a_right_angle_at_the_end_of_the_run_gets_the_diagonal() {
    let o = kontur(&square(pt(0.0, 0.0), 10.0), Abstaende::Fest(1.0));
    // Shifted along its own normal the corner point would be (0, −1); that
    // point lies on the line of the neighbouring edge x = 0, so the extra step
    // applies and gives (−1, −1).
    assert_eq!(o[0].0, pt(-1.0, -1.0));
    assert_eq!(o[0].1, pt(10.0, -1.0));
    assert_eq!(o[1].0, pt(11.0, 0.0));
    assert_eq!(o[1].1, pt(11.0, 10.0));
    assert_eq!(o[2].0, pt(10.0, 11.0));
    assert_eq!(o[2].1, pt(0.0, 11.0));
    assert_eq!(o[3].0, pt(-1.0, 10.0));
    assert_eq!(o[3].1, pt(-1.0, -1.0));
}

#[test]
fn a_corner_between_two_middle_edges_stays_split() {
    // Only the two ends of the run get the extra step. At a corner in the
    // middle the two displaced points stand next to each other, with a gap of
    // d·√2 - the reference leaves that gap, it removes no self-intersections.
    let o = kontur(&square(pt(0.0, 0.0), 10.0), Abstaende::Fest(1.0));
    assert_eq!(o[0].1, pt(10.0, -1.0));
    assert_eq!(o[1].0, pt(11.0, 0.0));
    assert_ne!(o[0].1, o[1].0);
    assert!((o[0].1.dist(o[1].0) - 2.0_f64.sqrt()).abs() <= TOL);
}

#[test]
fn an_oblique_corner_at_the_end_of_the_run_stays_where_it_is() {
    // Pentagon, run starts at the oblique corner (6, 2). Displaced along the
    // own normal it is (7, 2); that point is not on the line of the
    // neighbouring edge through (4, 0) and (6, 2), so no extra step.
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let k = vec![
        Saumkante::new(pt(6.0, 2.0), pt(6.0, 6.0), pt(1.0, 0.0)),
        Saumkante::new(pt(6.0, 6.0), pt(0.0, 6.0), pt(0.0, 1.0)),
        Saumkante::new(pt(0.0, 6.0), pt(0.0, 0.0), pt(-1.0, 0.0)),
        Saumkante::new(pt(0.0, 0.0), pt(4.0, 0.0), pt(0.0, -1.0)),
        Saumkante::new(pt(4.0, 0.0), pt(6.0, 2.0), pt(s, -s)),
    ];
    let o = kontur(&k, Abstaende::Fest(1.0));
    assert_near(o[0].0, 7.0, 2.0);
    // The other end of the run: (6, 2) displaced by (0.7071, −0.7071).
    assert_near(o[4].1, 6.0 + s, 2.0 - s);
}

#[test]
fn run_and_contour_together() {
    // Square of 10, run 0.5 → 1.5: the values sit at the four corners.
    let o = kontur(
        &square(pt(0.0, 0.0), 10.0),
        Abstaende::Verlauf(&[0.5, 0.75, 1.0, 1.25, 1.5]),
    );
    assert_eq!(o[0].0, pt(-0.5, -0.5));
    assert_eq!(o[0].1, pt(10.0, -0.75));
    assert_eq!(o[1].0, pt(10.75, 0.0));
    assert_eq!(o[1].1, pt(11.0, 10.0));
    assert_eq!(o[2].0, pt(10.0, 11.0));
    assert_eq!(o[2].1, pt(0.0, 11.25));
    assert_eq!(o[3].0, pt(-1.25, 10.0));
    assert_eq!(o[3].1, pt(-1.5, -1.5));
}

#[test]
fn the_extra_step_uses_the_stored_vector() {
    // A detail of the reference: the extra step uses the stored vector of the
    // neighbouring edge, not the normalised one. A vector twice as long
    // therefore moves by two allowances.
    let mut k = square(pt(0.0, 0.0), 10.0);
    k[3].richtung = pt(-2.0, 0.0);
    let o = kontur(&k, Abstaende::Fest(1.0));
    assert_eq!(o[3].0, pt(-1.0, 10.0), "the plain step is normalised");
    assert_eq!(o[3].1, pt(-1.0, -1.0), "the extra step belongs to k[0]");
    assert_eq!(o[0].0, pt(-2.0, -1.0), "and it reaches the start corner");
}

#[test]
fn the_extra_step_needs_a_right_angle() {
    // The rule is not "every corner": only where the displaced point lies on
    // the neighbouring line, which needs a right angle. The pentagon above is
    // the counter-case; here the square with nothing to move, and with twice
    // the allowance.
    let o = kontur(&square(pt(0.0, 0.0), 10.0), Abstaende::Fest(0.0));
    assert_eq!(o[0].0, pt(0.0, 0.0), "with allowance 0 nothing moves");
    let o = kontur(&square(pt(0.0, 0.0), 10.0), Abstaende::Fest(2.0));
    assert_eq!(o[0].0, pt(-2.0, -2.0), "twice, along both normals");
}

#[test]
#[should_panic(expected = "Verlauf braucht einen Wert je Kantenende")]
fn a_run_of_the_wrong_length_panics() {
    let k = square(pt(0.0, 0.0), 10.0);
    kontur(&k, Abstaende::Verlauf(&[1.0, 1.0, 1.0]));
}
