//! Skinning with dual quaternions, as the alternative to linear blending.
//!
//! [`Skelett::verforme`](crate::skelett::Skelett::verforme) averages the
//! *positions* that two bones produce for the same point. Two rigid transforms
//! do not average to a rigid transform, so wherever bones pull in different
//! directions the body shrinks: at the shoulder during an arm raise, at the
//! elbow in a sharp bend, at the hip in a wide step. The effect is known as
//! the candy-wrapper artefact.
//!
//! This module averages the *transforms* instead. Each rigid transform becomes
//! one unit dual quaternion, the weights blend in that space, and the sum is
//! normalized back onto the unit dual quaternions before it is applied.
//!
//! ## What the algebra is
//!
//! A rotation is the quaternion `q_r = (w, x, y, z)`. A translation `t` is a
//! pure quaternion `q_t = (0, t)`. The pair
//!
//! ```text
//! q̂ = q_r + ε · q_d      with   q_d = ½ · q_t · q_r
//! ```
//!
//! with `ε² = 0` is a dual quaternion, and `q̂` is a **unit** dual quaternion
//! when `|q_r| = 1` and `q_r · q_d = 0`. The point `p` travels with
//!
//! ```text
//! p' = rotate(q_r, p) + t,      t = 2 · vec(q_d · conj(q_r))
//! ```
//!
//! Nothing of that is a new model: it is the same rigid transform the matrix
//! path applies, only written so it can be averaged. Method after Kavan et al.,
//! *Skinning with Dual Quaternions* (2007); the formulas are adopted, the code
//! is written here.
//!
//! ## Two details that are easy to get wrong
//!
//! * **Hemisphere.** `q` and `−q` are the same rotation, but their sum is
//!   zero. Before adding, every part is flipped when its dot product with a
//!   reference part is negative ([`mische`] takes the heaviest part as the
//!   reference).
//! * **De-doubling.** A weighted sum of unit dual quaternions is not unit any
//!   more: the dual part picks up a component along the real part that no
//!   rigid transform has. [`DualQuat::normiert`] divides by `|q_r|` and then
//!   removes that component, which is what makes the result rigid again
//!   instead of merely short.
//!
//! ## The convention
//!
//! Matrices arrive as [`Mat4`], which is a row-vector layout: `p · M`,
//! translation in `[12..14]`. As a column-vector matrix that is `A = Mᵀ`, so
//! [`aus_starr`] reads `A[i][j] = m[j·4 + i]`. Points stay plain column
//! vectors throughout this module — the layout never leaks past the reader.

use crate::skelett::Mat4;
use crate::Punkt3;

/// A quaternion `(w, x, y, z)`; a rotation when normalized.
pub type Quat = [f64; 4];

/// A unit dual quaternion: rotation part and dual part.
///
/// Both are four numbers. `dual` is `½ · q_t · real`, so the pair carries
/// rotation **and** translation — there is no separate vector for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DualQuat {
    /// The rotation.
    pub real: Quat,
    /// The dual part, `½ · (0, t) · real`.
    pub dual: Quat,
}

/// Below this length a real part carries no rotation and the blend has no
/// answer. Two bones rotating by exactly opposite angles is the case.
pub const ENTARTET: f64 = 1e-9;

// ── Quaternionen ───────────────────────────────────────────────────────

/// The length of a quaternion.
pub fn norm(q: Quat) -> f64 {
    (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt()
}

/// The dot product of two quaternions.
pub fn dot(a: Quat, b: Quat) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

/// The conjugate; for a unit quaternion the inverse rotation.
pub fn conj(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], -q[3]]
}

/// The Hamilton product `a · b`.
///
/// Order matters and is the usual one: `a · b` applies `b` first.
pub fn mul(a: Quat, b: Quat) -> Quat {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

/// Turns a vector with a **unit** quaternion: `q · v · conj(q)`.
///
/// Written out as `v + 2·qv × (qv × v + w·v)` — two cross products instead of
/// two quaternion products, because the point loop calls it for every bone of
/// every point.
pub fn rotate(q: Quat, v: Punkt3) -> Punkt3 {
    let (w, qv) = (q[0], [q[1], q[2], q[3]]);
    let t = [
        2.0 * (qv[1] * v[2] - qv[2] * v[1]),
        2.0 * (qv[2] * v[0] - qv[0] * v[2]),
        2.0 * (qv[0] * v[1] - qv[1] * v[0]),
    ];
    [
        v[0] + w * t[0] + qv[1] * t[2] - qv[2] * t[1],
        v[1] + w * t[1] + qv[2] * t[0] - qv[0] * t[2],
        v[2] + w * t[2] + qv[0] * t[1] - qv[1] * t[0],
    ]
}

/// The rotation of a rigid [`Mat4`] as a unit quaternion.
///
/// `None` when the upper 3×3 is not a rotation: a scaled or sheared matrix has
/// no quaternion, and it must not be approximated silently.
pub fn quat_aus_starr(m: Mat4) -> Option<Quat> {
    // Column-vector view of the row-vector layout.
    let a = |z: usize, s: usize| m[s * 4 + z];

    // Is the upper 3×3 orthogonal? `starr_invers` asks the same question; a
    // scaled joint matrix is not a rotation and must not be read as one.
    for z in 0..3 {
        for s in 0..3 {
            let mut v = 0.0;
            for k in 0..3 {
                v += a(z, k) * a(s, k);
            }
            let soll = if z == s { 1.0 } else { 0.0 };
            if (v - soll).abs() > 1e-9 {
                return None;
            }
        }
    }

    let spur = a(0, 0) + a(1, 1) + a(2, 2);
    let q = if spur > 0.0 {
        let s = (spur + 1.0).sqrt() * 2.0;
        [
            0.25 * s,
            (a(2, 1) - a(1, 2)) / s,
            (a(0, 2) - a(2, 0)) / s,
            (a(1, 0) - a(0, 1)) / s,
        ]
    } else if a(0, 0) > a(1, 1) && a(0, 0) > a(2, 2) {
        let s = (1.0 + a(0, 0) - a(1, 1) - a(2, 2)).sqrt() * 2.0;
        [
            (a(2, 1) - a(1, 2)) / s,
            0.25 * s,
            (a(0, 1) + a(1, 0)) / s,
            (a(0, 2) + a(2, 0)) / s,
        ]
    } else if a(1, 1) > a(2, 2) {
        let s = (1.0 + a(1, 1) - a(0, 0) - a(2, 2)).sqrt() * 2.0;
        [
            (a(0, 2) - a(2, 0)) / s,
            (a(0, 1) + a(1, 0)) / s,
            0.25 * s,
            (a(1, 2) + a(2, 1)) / s,
        ]
    } else {
        let s = (1.0 + a(2, 2) - a(0, 0) - a(1, 1)).sqrt() * 2.0;
        [
            (a(1, 0) - a(0, 1)) / s,
            (a(0, 2) + a(2, 0)) / s,
            (a(1, 2) + a(2, 1)) / s,
            0.25 * s,
        ]
    };
    let n = norm(q);
    if n < ENTARTET {
        return None;
    }
    Some([q[0] / n, q[1] / n, q[2] / n, q[3] / n])
}

// ── Duale Quaternionen ─────────────────────────────────────────────────

impl DualQuat {
    /// The identity: no rotation, no translation.
    pub fn einheit() -> DualQuat {
        DualQuat {
            real: [1.0, 0.0, 0.0, 0.0],
            dual: [0.0; 4],
        }
    }

    /// Reads a rigid matrix as a unit dual quaternion.
    ///
    /// `None` when the upper 3×3 is not a rotation — see [`quat_aus_starr`].
    pub fn aus_starr(m: Mat4) -> Option<DualQuat> {
        let real = quat_aus_starr(m)?;
        let t = [m[12], m[13], m[14]];
        // q_d = ½ · (0, t) · q_r
        let qt = mul([0.0, t[0], t[1], t[2]], real);
        Some(DualQuat {
            real,
            dual: [
                0.5 * qt[0],
                0.5 * qt[1],
                0.5 * qt[2],
                0.5 * qt[3],
            ],
        })
    }

    /// Moves a point: `rotate(q_r, p) + t`.
    pub fn auf(&self, p: Punkt3) -> Punkt3 {
        let q = conj(self.real);
        // t = 2 · vec(q_d · conj(q_r))
        let d = mul(self.dual, q);
        [
            rotate(self.real, p)[0] + 2.0 * d[1],
            rotate(self.real, p)[1] + 2.0 * d[2],
            rotate(self.real, p)[2] + 2.0 * d[3],
        ]
    }

    /// The translation this transform carries, in the units of the matrix.
    pub fn translation(&self) -> Punkt3 {
        let d = mul(self.dual, conj(self.real));
        [2.0 * d[1], 2.0 * d[2], 2.0 * d[3]]
    }

    /// Puts `self` back onto the unit dual quaternions.
    ///
    /// Two steps, both needed: dividing by `|q_r|` undoes the shortening of a
    /// weighted sum, and removing the part of `q_d` along `q_r` undoes the
    /// doubling. Without the second step the result is not rigid — it would be
    /// a rotation with a scale, which is exactly what this module avoids.
    ///
    /// `None` when no rotation is left, e.g. two bones turning by opposite
    /// angles with equal weight.
    pub fn normiert(&self) -> Option<DualQuat> {
        let n = norm(self.real);
        if !(n > ENTARTET) {
            return None;
        }
        let mut real = [0.0f64; 4];
        let mut dual = [0.0f64; 4];
        for i in 0..4 {
            real[i] = self.real[i] / n;
            dual[i] = self.dual[i] / n;
        }
        let e = dot(real, dual);
        for i in 0..4 {
            dual[i] -= e * real[i];
        }
        let n2 = norm(real);
        if !(n2 > ENTARTET) {
            return None;
        }
        for i in 0..4 {
            real[i] /= n2;
            dual[i] /= n2;
        }
        Some(DualQuat { real, dual })
    }
}

/// Blends transforms with weights and normalizes the sum.
///
/// The heaviest part is the reference for the hemisphere: every other part is
/// flipped when its dot product with the reference is negative, because `q`
/// and `−q` are the same rotation but cancel in a sum. Weights are **not**
/// re-normalized here — the caller passes normalized weights, and the
/// division by `|q_r|` in [`DualQuat::normiert`] handles the rest.
///
/// `None` when the parts cancel (no rotation left) or nothing was passed.
pub fn mische(teile: &[(DualQuat, f64)]) -> Option<DualQuat> {
    let (bezug, _) = teile
        .iter()
        .copied()
        .reduce(|a, b| if b.1 > a.1 { b } else { a })?;
    let mut real = [0.0f64; 4];
    let mut dual = [0.0f64; 4];
    for (q, w) in teile {
        let s = if dot(q.real, bezug.real) < 0.0 { -w } else { *w };
        for i in 0..4 {
            real[i] += s * q.real[i];
            dual[i] += s * q.dual[i];
        }
    }
    DualQuat { real, dual }.normiert()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skelett::{drehmatrix, mul as mat_mul, punkt_mal, verschiebung, welt_drehung};

    fn nah(a: Punkt3, b: Punkt3) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    /// A rigid matrix and its dual quaternion move a point the same way.
    #[test]
    fn matrix_und_dualquaternion_stimmen_ueberein() {
        let m = mat_mul(
            drehmatrix([0.3, -0.5, 0.81], 37.5),
            verschiebung([12.0, -4.5, 130.0]),
        );
        let q = DualQuat::aus_starr(m).expect("starr");
        for p in [
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [-23.0, 456.0, 78.0],
            [1.0, 1.0, 1.0],
        ] {
            assert!(
                nah(punkt_mal(m, p), q.auf(p)) < 1e-9,
                "{} gegen {}",
                punkt_mal(m, p)[0],
                q.auf(p)[0]
            );
        }
    }

    /// The rotation of a matrix read as a quaternion, applied, is the matrix.
    #[test]
    fn quat_liest_die_drehung_richtig() {
        for (achse, grad) in [
            ([1.0, 0.0, 0.0], 90.0),
            ([0.0, 0.0, 1.0], 90.0),
            ([0.0, 1.0, 0.0], 45.0),
            ([0.3, -0.5, 0.81], 200.0),
        ] {
            // Only the rotation: build it around the origin, no translation.
            let m = welt_drehung(achse, [0.0, 0.0, 0.0], grad);
            let q = quat_aus_starr(m).expect("Drehung");
            for p in [[7.0, 0.0, 0.0], [0.0, 5.0, 0.0], [1.0, 2.0, 3.0]] {
                let a = punkt_mal(m, p);
                let b = rotate(q, p);
                assert!(nah(a, b) < 1e-9, "Achse {achse:?} {grad}°: {a:?} gegen {b:?}");
            }
        }
    }

    /// The translation of the dual quaternion is the one of the matrix.
    #[test]
    fn verschiebung_wird_gelesen() {
        let m = verschiebung([3.5, -11.25, 240.0]);
        let q = DualQuat::aus_starr(m).expect("starr");
        let t = q.translation();
        assert!(nah(t, [3.5, -11.25, 240.0]) < 1e-9, "{t:?}");
    }

    /// A weighted sum of one part is that part.
    #[test]
    fn eine_einzige_stimme_bleibt() {
        let m = welt_drehung([0.0, 0.0, 1.0], [10.0, 0.0, 0.0], 60.0);
        let q = DualQuat::aus_starr(m).expect("starr");
        let g = mische(&[(q, 1.0)]).expect("gemischt");
        for p in [[0.0, 0.0, 0.0], [50.0, 12.0, -3.0]] {
            assert!(nah(g.auf(p), punkt_mal(m, p)) < 1e-9);
        }
    }

    /// The candy-wrapper case, with the numbers that separate the two methods.
    ///
    /// One bone does nothing, the other turns 90° about `z`; a point 100 mm
    /// from the axis is weighted half and half. Blending positions gives
    /// `100 · cos(45°) = 70.7107 mm`, blending transforms keeps `100 mm`.
    #[test]
    fn der_duale_weg_haelt_den_abstand() {
        let drehung = welt_drehung([0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 90.0);
        let p = [100.0, 0.0, 0.0];

        let linear = {
            let a = punkt_mal(crate::skelett::EINHEIT, p);
            let b = punkt_mal(drehung, p);
            [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1]), 0.5 * (a[2] + b[2])]
        };
        let dual = mische(&[
            (DualQuat::einheit(), 0.5),
            (DualQuat::aus_starr(drehung).expect("starr"), 0.5),
        ])
        .expect("gemischt")
        .auf(p);

        let l = (linear[0] * linear[0] + linear[1] * linear[1]).sqrt();
        let d = (dual[0] * dual[0] + dual[1] * dual[1]).sqrt();
        // Hand-computed: 100·cos(45°) = 70.710678…, dual stays at 100.
        assert!((l - 100.0 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9, "{l}");
        assert!((d - 100.0).abs() < 1e-9, "{d}");
        // And it is a rotation, not a shortening: the point is on the arc.
        assert!((dual[0].hypot(dual[1]) - 100.0).abs() < 1e-9);
        assert!((dual[2]).abs() < 1e-9);
        // The half angle: 45°.
        assert!((dual[1].atan2(dual[0]).to_degrees() - 45.0).abs() < 1e-9);
    }

    /// Two identical transforms blend to themselves in both methods.
    #[test]
    fn gleiche_teile_bleiben_gleich() {
        let m = mat_mul(
            welt_drehung([0.0, 1.0, 0.0], [0.0, 0.0, 0.0], 30.0),
            verschiebung([5.0, 6.0, 7.0]),
        );
        let q = DualQuat::aus_starr(m).expect("starr");
        let g = mische(&[(q, 0.25), (q, 0.75)]).expect("gemischt");
        for p in [[0.0, 0.0, 0.0], [40.0, -20.0, 9.0]] {
            assert!(nah(g.auf(p), punkt_mal(m, p)) < 1e-9);
        }
    }

    /// Half a turn apart cancels: no rotation is left, and that is said.
    #[test]
    fn gegenlaeufige_drehungen_ergeben_keine_antwort() {
        let a = DualQuat::aus_starr(welt_drehung([0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 90.0))
            .expect("starr");
        let b = DualQuat::aus_starr(welt_drehung([0.0, 0.0, 1.0], [0.0, 0.0, 0.0], -90.0))
            .expect("starr");
        // The hemisphere correction puts both on the same side; the sum of two
        // opposite 90° turns is the identity rotation, so the blend exists.
        let g = mische(&[(a, 0.5), (b, 0.5)]).expect("gemischt");
        let p = [100.0, 0.0, 0.0];
        assert!(nah(g.auf(p), p) < 1e-9, "{:?}", g.auf(p));

        // Exactly opposite (180° apart) has no hemisphere on which both fit.
        let c = DualQuat::aus_starr(welt_drehung([0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 180.0))
            .expect("starr");
        let e = DualQuat::einheit();
        assert!(mische(&[(c, 0.5), (e, 0.5)]).is_some(), "180° ist noch eine Drehung");
        assert!(mische(&[]).is_none());
    }

    /// A scaled matrix is refused, not silently rotated.
    #[test]
    fn skalierte_matrizen_werden_abgelehnt() {
        let mut m = crate::skelett::EINHEIT;
        m[0] = 2.0;
        assert!(quat_aus_starr(m).is_none());
        assert!(DualQuat::aus_starr(m).is_none());
    }
}
