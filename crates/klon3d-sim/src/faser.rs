//! The material direction per triangle: the piece's grainline carried through
//! the placement into the triangle's own plane.
//!
//! The pattern gives one axis per piece (`klon3d_mesh::Fadenlauf`, in pattern
//! coordinates) and the placement bends that piece over the body - a hull, a
//! prism, an arm cylinder - so the axis is *not* the same 3D direction at every
//! triangle. The direction used here is the image of the 2D axis under the
//! linearisation the solver already builds for each triangle: the reduced rest
//! triangle is mapped onto the current one, and a 2D vector's coordinates in
//! the rest triangle's basis give the same combination of the current edges.
//! That reproduces the nonlinear placement exactly as far as the mesh resolves
//! it, without sampling the placement again.
//!
//! What this module does **not** do is make the material anisotropic in the
//! sense of a pose-dependent direction: [`crate::Szene::faser`] is carried data,
//! and no solver term reads it. The bending stiffness since #129 takes its
//! direction from the piece's **rest** triangle ([`crate::biegung`]), not from
//! this vector - a bending stiffness is a material number and must not follow
//! the pose.

use klon3d_mesh::Fadenlauf;

/// The substitute axis for a piece whose pattern has no readable grainline:
/// pattern x, i.e. the direction the pattern's own coordinates run in. It is a
/// placeholder, not a measurement - which is why every piece that gets it is
/// counted in [`crate::Szene::faser_ersatz`] and named in the report. What a
/// piece axis should be instead is the open part of stage 1 (PLAN.md).
pub const ERSATZ: [f64; 2] = [1.0, 0.0];

/// The piece's direction in pattern coordinates and whether it is the
/// substitute (no visible, evaluable grainline, or a grainline without length).
pub fn richtung(fadenlauf: Option<Fadenlauf>) -> ([f64; 2], bool) {
    match fadenlauf.and_then(|f| f.richtung()) {
        Some(d) => (d, false),
        None => (ERSATZ, true),
    }
}

/// The 3D direction of the 2D unit vector `dir2` inside one triangle.
///
/// `ruhe_inv` is the inverse of the triangle's reduced rest matrix
/// [`crate::Szene::ruhe_inv`]: it turns `dir2` into the coefficients of
/// `dir2` in the basis `[p1 − p0, p2 − p0]`. The same coefficients applied to
/// the current edges `[x1 − x0, x2 − x0]` give the 3D vector; the component
/// along the triangle's normal is removed and the result normalised, so the
/// direction lies in the triangle's plane and has length one.
///
/// `None` when the triangle has no area - either in the pattern or after the
/// placement - so there is no plane to put a direction in.
pub fn im_dreieck(dir2: [f64; 2], ruhe_inv: [f64; 4], p: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let a = ruhe_inv[0] * dir2[0] + ruhe_inv[1] * dir2[1];
    let b = ruhe_inv[2] * dir2[0] + ruhe_inv[3] * dir2[1];
    let e1 = sub(p[1], p[0]);
    let e2 = sub(p[2], p[0]);
    let d = add(mul(e1, a), mul(e2, b));
    let n = cross(e1, e2);
    let nn = dot(n, n);
    if !(nn > 0.0) || !d.iter().all(|v| v.is_finite()) {
        return None;
    }
    let t = sub(d, mul(n, dot(d, n) / nn));
    let l = dot(t, t).sqrt();
    (l > 1e-12).then(|| mul(t, 1.0 / l))
}

/// Direction of the longest edge of the triangle, as a fallback for a triangle
/// the placement collapsed. `None` if all three points coincide.
pub fn kante(p: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let mut best = None;
    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
        let e = sub(p[j], p[i]);
        let l = dot(e, e);
        if l > 1e-24 && best.map_or(true, |(b, _)| l > b) {
            best = Some((l, mul(e, 1.0 / l.sqrt())));
        }
    }
    best.map(|(_, e)| e)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
