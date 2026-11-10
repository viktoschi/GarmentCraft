//! Bending stiffness as a function of direction: the three values a `.zfab`
//! states instead of one.
//!
//! A fabric bends differently along its grainline than across it. CLO states
//! three stiffnesses (`fBuK` weft, `fBvK` warp, `fBhK` bias, each in
//! g·mm²/s²/rad) and all three are parsed by `klon3d-pattern`; until now the
//! solver took one number for every direction.
//!
//! ## What the angle means here
//!
//! A bending constraint folds the two triangles that share an edge around that
//! edge, so the **curvature runs perpendicular to the edge**. The direction of
//! bending is therefore the edge's **normal** in the piece's rest plane, not the
//! edge itself. `theta` is the angle from the **warp** direction to that normal,
//! measured in the rest triangle - so
//!
//! * `theta = 0` bends the fabric about an axis running along the weft: the
//!   curvature is along the warp, and the stiffness is `D_warp`;
//! * `theta = pi/2` bends it about the warp axis: `D_weft`.
//!
//! Getting this the other way round swaps weft and warp. It is invisible on the
//! eight fabrics in `assets/stoffe` (they differ by 0.17 %), which is why the
//! unit tests pin both ends rather than relying on a scene.
//!
//! Task #129 writes the same two ends the other way round (`D(0) = weft`,
//! `D(pi/2) = warp`) because it lets `theta` be the angle of the **fold axis**
//! to the warp. A fold axis along the warp makes the curvature run along the
//! weft, so that description and this one are the same function with the angle
//! shifted by `pi/2`: `D_axis(t) = D_curv(t + pi/2)`. This module keeps the
//! curvature direction, because that is what an edge of a mesh gives you
//! directly and what the textbook rotation is written in.
//!
//! ## The interpolation
//!
//! For a thin orthotropic plate the bending stiffness about an axis at angle
//! `theta` to the warp is the tensor form
//!
//! ```text
//! D(theta) = D_w·cos⁴(theta) + D_s·sin⁴(theta) + (4·D_b − D_w − D_s)·cos²(theta)·sin²(theta)
//! ```
//!
//! with `w` warp, `s` weft and `b` bias (45°). It reproduces the three stated
//! directions (`theta = 0` gives `D_w`, `pi/2` gives `D_s`, `pi/4` gives `D_b`).
//!
//! This is the textbook form: an orthotropic plate has four independent bending
//! constants and `D(theta)` is the tensor rotation of them, because
//! `2·(D₁₂ + 2·D₆₆) = 4·D_b − D_w − D_s`.
//!
//! **It does not stay inside the interval of the three values**, and the earlier
//! version of this doc claimed it did. With a *soft* bias the curve dips below
//! all three: `D = (2, 3, 0.8)` reaches `519/680 = 0.7632` at 49.22826°, which is
//! 4.6 % under the 0.8 the file states for 45°. A plate with a soft diagonal
//! genuinely bends most easily somewhat off the diagonal. A form that is convex
//! in `cos²` (a straight line between the three points) would stay inside the
//! interval, but that is an interpolation invention and not plate theory. The
//! eight files in `assets/stoffe` have `warp == bias`, so there the minimum is
//! exactly the weft value and the curve only ever rises - which is why the
//! scenes cannot get softer (see the note on the bound below).
//!
//! The reciprocal form `1/D` (Föppl) gives a number within 7e-7 relative of this
//! one on the eight repo fabrics, so the choice between them does not matter for
//! what is measured here - documented rather than hidden.
//!
//! **What this costs.** Measured over all angles, the eight repo fabrics span
//! `0.0000 %` to `+0.1926 %` against the weft value the solver used before; the
//! maximum sits at 30°, where the curve bulges above both ends (`+0.1926 %`
//! against `+0.1712 %` at 90°). So an anisotropic run can be at most 0.193 %
//! stiffer and never softer - the fault floors (10 mm, 0.02 pp, 0.02) have far
//! more room than that.
//!
//! ## What is *not* done
//!
//! The bending of the fold band ([`crate::saumfalte`]) and of a bonded pair
//! ([`crate::klebung::Stoffpaar`]) keep one value: their edges carry no
//! grainline of their own, and a bonded pair is a spring pair rather than a
//! plate. [`crate::Material::biegung_nm`] stays the weft value, so those callers
//! keep today's number without being touched.

/// The bending stiffness of a plate whose **curvature** runs at `theta` radians
/// from the warp direction, in N·m.
///
/// So `theta = 0` is a fold axis along the weft and returns `warp`, `theta =
/// pi/2` is a fold axis along the warp and returns `weft`; the fold axis itself
/// is `theta + pi/2`. (The task text #129 counts `theta` from the fold axis and
/// therefore states the two ends the other way round - see the module doc.)
///
/// `weft`, `warp` and `bias` are the three stiffnesses the file states
/// (`fBuK`, `fBvK`, `fBhK`). When all three are equal the result is that value
/// **bit for bit** - no sum and no rounding, so an isotropic fabric cannot move
/// by having been made anisotropic. That equality is what keeps every acceptance
/// number of the existing scenes reproducible.
pub fn steifigkeit(weft: f64, warp: f64, bias: f64, theta: f64) -> f64 {
    if weft == warp && warp == bias {
        return weft;
    }
    let (s, c) = theta.sin_cos();
    let (s2, c2) = (s * s, c * c);
    positiv(
        warp * (c2 * c2) + weft * (s2 * s2) + (4.0 * bias - warp - weft) * (c2 * s2),
        weft,
        warp,
        bias,
    )
}

/// [`steifigkeit`] with the angle given by `cos²` and `sin²` of the angle, for a
/// caller that has the normal already and would only square it again.
///
/// `c2 + s2` is expected to be one; the caller takes it from a unit normal.
#[inline]
pub fn steifigkeit_aus(c2: f64, s2: f64, weft: f64, warp: f64, bias: f64) -> f64 {
    if weft == warp && warp == bias {
        return weft;
    }
    positiv(
        warp * (c2 * c2) + weft * (s2 * s2) + (4.0 * bias - warp - weft) * (c2 * s2),
        weft,
        warp,
        bias,
    )
}

/// Keeps a bending stiffness positive.
///
/// The tensor form can turn **negative** for a triple that is not a valid plate:
/// `D = (1, 10, 0.01)` reaches -0.912 at 57°, because a nearly free diagonal is
/// not compatible with a stiff warp over the whole circle of directions. The
/// reciprocal form does the same (-14.1 for the same input). The solver builds
/// its spring from `1/k`, so a negative value would be an anti-spring that
/// pushes the fabric apart - it must never get there.
///
/// No file in `assets/stoffe` can trigger this (`warp == bias` there, so the
/// minimum is the weft value and it is positive), and a valid triple does not
/// either. The floor is therefore a guard, not a material value: 0.1 % of the
/// softest stated direction, which behaves like a nearly free hinge.
fn positiv(d: f64, weft: f64, warp: f64, bias: f64) -> f64 {
    if d > 0.0 {
        return d;
    }
    let kleinste = weft.min(warp).min(bias).abs();
    1.0e-3 * kleinste
}

/// `cos²(theta)` and `sin²(theta)` of the angle between the rest triangle's
/// bending normal and the warp direction.
///
/// `e` is the edge in the piece's rest coordinates (m), `warp` the grainline in
/// the same frame (any length, not normalised). The bending normal is `e`
/// turned by 90°, so its cosine against the warp is `-sin` of the edge's own
/// angle to the warp - the two squared values are simply swapped relative to the
/// edge. Only squares enter [`steifigkeit_aus`], so no `atan2` and no quadrant
/// are needed.
///
/// `None` when either vector has no length: a degenerate edge or a piece without
/// a readable grainline.
pub fn cos2_sin2(e: [f64; 2], warp: [f64; 2]) -> Option<(f64, f64)> {
    let (le, lw) = (
        (e[0] * e[0] + e[1] * e[1]).sqrt(),
        (warp[0] * warp[0] + warp[1] * warp[1]).sqrt(),
    );
    if !(le > 1e-12) || !(lw > 1e-12) {
        return None;
    }
    let (ex, ey) = (e[0] / le, e[1] / le);
    let (wx, wy) = (warp[0] / lw, warp[1] / lw);
    // cos(angle of edge to warp) and sin of it.
    let c_edge = ex * wx + ey * wy;
    let s_edge = ex * wy - ey * wx;
    // The normal is the edge turned by 90°: (c, s) -> (-s, c).
    let c = -s_edge;
    let s = c_edge;
    Some((c * c, s * s))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEFT: f64 = 2482.2424e-9;
    const WARP: f64 = 2486.4924e-9;
    const BIAS: f64 = 2486.4924e-9;

    /// An isotropic triple is returned untouched - bit for bit, not within a
    /// tolerance. This is the promise that keeps every existing acceptance
    /// number reproducible.
    #[test]
    fn isotrop_bleibt_bitgleich() {
        for d in [WEFT, 1.0e-12, 0.0, 6.0361e-8, 1.0e9] {
            for i in 0..=72 {
                let theta = i as f64 * std::f64::consts::FRAC_PI_2 / 72.0;
                let r = steifigkeit(d, d, d, theta);
                assert_eq!(
                    r.to_bits(),
                    d.to_bits(),
                    "d={d:e} theta={theta}: {r:e} gegen {d:e}"
                );
            }
        }
    }

    /// The three stated directions are reproduced exactly at their own angle.
    #[test]
    fn die_drei_richtungen_treffen() {
        let halbe = std::f64::consts::FRAC_PI_2;
        // 0 and 90 degrees are exact: at those angles only one term is left.
        assert_eq!(steifigkeit(WEFT, WARP, BIAS, 0.0).to_bits(), WARP.to_bits());
        assert_eq!(steifigkeit(WEFT, WARP, BIAS, halbe).to_bits(), WEFT.to_bits());
        // 45 degrees is the bias value: with c² = s² = 1/2 and their sum one,
        // (D_w + D_s)/4 + (4·D_b − D_w − D_s)/4 leaves D_b. It is not bit-exact
        // because cos(pi/4) is not exactly 1/sqrt(2): measured 1.5 ulp, so the
        // tolerance is 1e-15 relative and not a number picked from the air.
        let viertel = std::f64::consts::FRAC_PI_4;
        let gemessen = steifigkeit(WEFT, WARP, BIAS, viertel);
        assert!(
            (gemessen - BIAS).abs() <= 1e-15 * BIAS,
            "{gemessen:e} gegen {BIAS:e}"
        );
    }

    /// **The curve leaves the interval of its three values** when the diagonal is
    /// soft, and the module doc says why: the tensor rotation of an orthotropic
    /// plate is not a convex combination. Pinned with a hand-computed number so
    /// nobody "fixes" the curve back into a straight line by accident.
    ///
    /// `D = (2, 3, 0.8)`: with `u = cos²theta` the curve is the parabola
    /// `f(u) = w + B·u + A·u²`, `A = 2(w + v − 2b) = 6.8`, `B = 4b − v − 3w =
    /// −5.8`; its vertex is `u* = −B/(2A)` and `f(u*) = 519/680`.
    #[test]
    fn mit_weicher_schraege_faellt_die_kurve_unter_die_drei_werte() {
        let (weft, warp, bias): (f64, f64, f64) = (2.0, 3.0, 0.8);
        let a = 2.0 * (weft + warp - 2.0 * bias);
        let b = 4.0 * bias - warp - 3.0 * weft;
        let u_stern = -b / (2.0 * a);
        // Pinned by hand, not read back from the code.
        assert!((a - 6.8).abs() < 1e-15 && (b + 5.8).abs() < 1e-15);
        assert!((u_stern - 0.426_470_588_235_294).abs() < 1e-15);

        let theta_stern = u_stern.sqrt().acos();
        assert!(
            (theta_stern.to_degrees() - 49.228_260).abs() < 1e-5,
            "Scheitel bei {:.6} Grad",
            theta_stern.to_degrees()
        );
        let minimum = steifigkeit(weft, warp, bias, theta_stern);
        let erwartet = 519.0 / 680.0;
        assert!(
            (minimum - erwartet).abs() <= 1e-13 * erwartet,
            "Minimum {minimum:?} gegen {erwartet:?}"
        );
        assert!(minimum < bias, "das Minimum liegt unter der Schraegen");
        // The same number from the other end, so a typo in the vertex cannot
        // hide: w − B²/(4A) = 2 − 33.64/27.2.
        assert!((weft - b * b / (4.0 * a) - erwartet).abs() < 1e-13);

        // No angle of the circle falls below it - checked on a grid, whose
        // resolution (2.4e-8 relative) is well inside the 1e-7 asked for here.
        for i in 0..=3600 {
            let theta = i as f64 * std::f64::consts::FRAC_PI_2 / 3600.0;
            let d = steifigkeit(weft, warp, bias, theta);
            assert!(d >= minimum - 2e-7 * erwartet, "theta {theta}: {d:e}");
        }
    }

    /// On the eight fabrics in `assets/stoffe` the warp and the bias are equal,
    /// so the minimum is exactly the weft value and the curve only rises. That
    /// is the bound the issue names, and it is the reason the scenes can get at
    /// most 0.193 % stiffer, never softer.
    #[test]
    fn die_acht_stoffe_werden_nur_steifer() {
        let (weft, warp, bias) = (WEFT, WARP, BIAS);
        let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..=1800 {
            let theta = i as f64 * std::f64::consts::FRAC_PI_2 / 1800.0;
            let d = steifigkeit(weft, warp, bias, theta);
            min = min.min(d);
            max = max.max(d);
        }
        assert!((min - weft).abs() < 1e-18, "min {min:e} gegen weft {weft:e}");
        // The maximum is at 30 degrees where the curve bulges above both ends.
        let rel = (max - weft) / weft;
        assert!(
            (rel - 0.001_926).abs() < 2e-6,
            "Maximum {max:e}, das sind {:.6} % gegen den Schuss",
            rel * 100.0
        );
        assert!(max > warp, "der Bauch liegt ueber der Kette");
    }

    /// A triple that is not a valid plate can make the tensor form negative -
    /// a nearly free diagonal next to a stiff warp. The solver builds its spring
    /// from `1/k`, so the guard must never hand it zero or a negative number.
    #[test]
    fn eine_negative_steifigkeit_wird_abgefangen() {
        let (weft, warp, bias) = (1.0e-6, 10.0e-6, 1.0e-8);
        let mut negativ = 0;
        for i in 0..=3600 {
            let theta = i as f64 * std::f64::consts::FRAC_PI_2 / 3600.0;
            let d = steifigkeit(weft, warp, bias, theta);
            assert!(d > 0.0, "theta {theta}: {d:e}");
            // The floor is 0.1 % of the softest stated direction.
            assert!(d >= 1.0e-3 * bias * (1.0 - 1e-12), "theta {theta}: {d:e}");
            if d <= 1.0e-3 * bias * 1.000001 {
                negativ += 1;
            }
        }
        assert!(negativ > 0, "der Fall muss die Grenze ueberhaupt treffen");
        // The same at the given angle, without the round trip through radians.
        assert!(steifigkeit(weft, warp, bias, 1.0) > 0.0);
    }
}
