//! The membrane: one elastic element per triangle, with warp, weft and shear
//! as three independent stiffnesses.
//!
//! A net of distance springs along the mesh edges cannot separate stretch from
//! shear. For every set of edge springs `C1212 ≡ C1122` (both are
//! `Σ w·n₁²n₂²`), so such a net always has `ν = G/E`; measured on all 22
//! pieces of the test blouse the net gives `ν = 0.333`, `G = 0.375·E`. Latex
//! measures `G/E = 174.159 / 348.274 = 0.5001`, i.e. `ν ≈ 0`. Recalibrating the
//! weights only slides along the line `ν = G/E`; `ν = 0` together with
//! `G/E = 0.5` is unreachable.
//!
//! This module therefore carries the membrane **per triangle** and takes the
//! three in-plane stiffnesses from the fabric profile:
//!
//! | input | CLO field | [`crate::Material`] |
//! |---|---|---|
//! | warp (chain) stretch | `fSvK` | `kette_n_m` |
//! | weft (shute) stretch | `fSuK` | `dehnung_n_m` |
//! | shear | `fHK` | `schub_n_m` |
//!
//! The `.zfab` states no Poisson ratio, and none is needed: with three
//! independent stiffnesses the strain energy is
//!
//! ```text
//! U/A₀ = ½·E_k·ε_k² + ½·E_s·ε_s² + 2·G·ε_ks²
//! ```
//!
//! with the Green strain `ε = ½(FᵀF − I)` in the piece's own orthonormal
//! material basis (warp, weft). Poisson's ratio then follows from `E` and `G`
//! (`ν = E/(2G) − 1`) instead of being assumed; for the eight built-in
//! profiles that is `−0.0001`, i.e. the measured `≈ 0`.
//!
//! **Voigt convention.** `g = (g₁₁, g₂₂, 2g₁₂)` with `g = FᵀF`, so that the
//! strain components are single dot products: `ε_w = ½(a(w)·g − 1)` with
//! `a(w) = (w₁², w₂², w₁w₂)` and `ε_wv = ½ c(w, v)·g` with
//! `c(w, v) = (w₁v₁, w₂v₂, ½(w₁v₂ + w₂v₁))`.
//!
//! **Basis.** The rest triangle and the material directions live in the
//! piece's pattern coordinates (mm → m). The warp direction is the piece's
//! grainline, the weft its perpendicular; the pattern states nothing else. A
//! pure rotation of the triangle leaves `FᵀF` unchanged, so the energy does not
//! depend on where the piece sits.
//!
//! **Rest shape.** [`crate::Szene::ruhe_inv`] is the pattern rest triangle. The
//! ease ([`crate::Szene::abstand`] rest lengths) can shorten a boundary edge, so
//! the membrane builds its own rest matrix from the three (possibly shortened)
//! rest lengths of its edges, keeping the pattern direction of edge `0 → 1`.
//! Without an ease that reproduces the pattern triangle exactly.
//!
//! **Ease and the material basis.** The rebuilt rest triangle is the reference
//! configuration, so the material basis is taken there: both pattern
//! directions are carried into it and then orthonormalised (the weft against
//! the warp). Without an ease the two are perpendicular up to the pattern's own
//! geometry and nothing is lost; with an ease the deviation is second order,
//! because the ease changes edge lengths by a few percent and never the
//! direction of edge `0 → 1`.
//!
//! [`Membran::krafte`], [`Membran::mal_h`] and [`Membran::diag_bloecke`] are
//! what a solver needs. The tangent is the **full** Hessian, material part plus
//! the geometric one: the geometric term is what gives a flat triangle its
//! out-of-plane stiffness, and leaving it out makes the operator singular in
//! that direction (a distance constraint carries the same term as its `quer`
//! factor). The stress in the geometric term is taken tensile only, like that
//! factor, so compression cannot make the operator indefinite.

use crate::koerper::{add, dot, mul, norm, sub};

/// One triangle's membrane. Built once per scene, never changed afterwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Membran {
    /// Rest area `A₀` (m²) of the eased reference triangle.
    pub flaeche: f64,
    /// Inverse of the reference rest matrix `[p₁ − p₀, p₂ − p₀]`, row-wise
    /// `[[r0, r1], [r2, r3]]` — the same layout as
    /// [`crate::Szene::ruhe_inv`], but built from the eased edge lengths.
    pub ruhe_inv: [f64; 4],
    /// Warp direction, unit vector in reference coordinates.
    pub kette: [f64; 2],
    /// Weft direction, unit and orthogonal to [`Self::kette`] in reference
    /// coordinates.
    pub schuss: [f64; 2],
    /// Warp stretch stiffness `E_k` (N/m).
    pub kette_n_m: f64,
    /// Weft stretch stiffness `E_s` (N/m).
    pub dehnung_n_m: f64,
    /// Shear stiffness `G` (N/m).
    pub schub_n_m: f64,
}

/// A membrane triangle as the solver sees it: the three point indices and the
/// element. Built once per scene ([`crate::Szene::membran_elemente`]) and only
/// borrowed afterwards — it carries nothing that follows from the positions, so
/// rebuilding it per substep would copy the whole list five times a frame for
/// nothing.
#[derive(Debug, Clone, Copy)]
pub struct MembranElement {
    pub punkte: [u32; 3],
    pub element: Membran,
}

/// The part of one triangle's Hessian that depends on the positions of the
/// **build** state only.
///
/// The implicit solvers evaluate the tangent at the positions the system was
/// built from and never at the CG iterate (the membrane Jacobian of
/// [`Membran::mal_h`]), so this is computed once per substep and reused by every
/// iteration of the CG. It is what makes the membrane affordable in the CPU
/// solver: without it each iteration repeats the strain, the stress and the PSD
/// projection of every triangle, which measured 0.79 of the 0.89 ms an operator
/// pass costs on the test blouse.
///
/// Deliberately small: `F`'s two columns and the projected stress block, 72
/// bytes. Everything else the tangent needs — the rest coefficients, the
/// material basis, the stiffness — is a load from [`Membran`] and is cheaper to
/// derive than to carry along on every pass.
#[derive(Debug, Clone, Copy)]
pub struct Hessen {
    /// `F`'s first column at the build positions.
    pub f1: [f64; 3],
    /// `F`'s second column at the build positions.
    pub f2: [f64; 3],
    /// The geometric Hessian factor `(w₁₁, w₂₂, w₁₂)`, already projected onto
    /// the PSD cone (see [`Membran::geo_w_aus`]). Every 3×3 block of the
    /// geometric term is `w₁₁·c₁c₁ᵀ + w₂₂·c₂c₂ᵀ + w₁₂·(c₁c₂ᵀ + c₂c₁ᵀ)` times
    /// `2`, so three scalars carry it — not the nine of the assembled matrix.
    pub w: [f64; 3],
}

/// One scalar constraint of a membrane triangle, for XPBD: its (already
/// computed) strain value, the gradient at each of the three vertices and the
/// compliance `1/(K·A₀)`.
#[derive(Debug, Clone, Copy)]
pub struct Zug {
    /// The constraint value (`ε_warp`, `ε_weft` or `γ`).
    pub c: f64,
    /// ∂C/∂xₖ per vertex, in order `(0, 1, 2)`.
    pub grad: [[f64; 3]; 3],
    /// Compliance (m/N).
    pub alpha: f64,
}

/// `a(w)`: `wᵀg w` reads as `a(w)·g` for `g = (g₁₁, g₂₂, 2g₁₂)`.
fn a3(w: [f64; 2]) -> [f64; 3] {
    [w[0] * w[0], w[1] * w[1], w[0] * w[1]]
}

/// `c(w, v)`: `wᵀg v` reads as `c(w, v)·g`.
fn c3(w: [f64; 2], v: [f64; 2]) -> [f64; 3] {
    [w[0] * v[0], w[1] * v[1], 0.5 * (w[0] * v[1] + w[1] * v[0])]
}

/// The Frobenius-nearest positive semi-definite symmetric matrix to
/// `[[a, c], [c, b]]`, as `(a, b, c)`.
///
/// Both eigenvalues are kept when they are positive (the result is the input,
/// unchanged). Otherwise only the positive one survives; with none, the result
/// is zero. Used on the shear block in [`Membran::geo_w_aus`].
fn psd2(a: f64, b: f64, c: f64) -> (f64, f64, f64) {
    let m = 0.5 * (a + b);
    let d = 0.5 * (a - b);
    let r = (d * d + c * c).sqrt();
    if m - r >= 0.0 {
        return (a, b, c);
    }
    let l_max = m + r;
    if !(l_max > 0.0) {
        return (0.0, 0.0, 0.0);
    }
    // Eigenvector of `l_max`: `(l_max - b, c)` (row 1 of `M - l_max·I`).
    let u = [l_max - b, c];
    let n2 = u[0] * u[0] + u[1] * u[1];
    if !(n2 > 0.0) {
        return (0.0, 0.0, 0.0);
    }
    let f = l_max / n2;
    (f * u[0] * u[0], f * u[1] * u[1], f * u[0] * u[1])
}

/// The rest triangle of a membrane: the inverse of the rebuilt matrix
/// `D = [p₁ − p₀, p₂ − p₀]` in the layout of [`crate::Szene::ruhe_inv`]
/// (`[d00, d01, d10, d11]`) and the area.
#[derive(Debug, Clone, Copy)]
struct Rest {
    inv: [f64; 4],
    flaeche: f64,
}

/// Rebuilds a rest triangle from three side lengths, keeping edge `0 → 1`
/// along `+x` with the third vertex at positive `y`.
///
/// `kanten` is `(l₀₁, l₁₂, l₂₀)`. Returns `None` when the three lengths close
/// no triangle with area — a degenerate triangle carries no membrane.
fn rest_aus_kanten(kanten: [f64; 3]) -> Option<Rest> {
    let [l01, l12, l20] = kanten;
    if !(l01 > 0.0 && l12 > 0.0 && l20 > 0.0) {
        return None;
    }
    // p₀ = (0, 0), p₁ = (l01, 0); p₂ from the law of cosines, on the positive
    // side.
    let x = (l01 * l01 + l20 * l20 - l12 * l12) / (2.0 * l01);
    let y2 = l20 * l20 - x * x;
    if !(y2 > 0.0) {
        return None;
    }
    let y = y2.sqrt();
    let det = l01 * y;
    if !(det > 0.0) {
        return None;
    }
    // `D = [[l01, x], [0, y]]` row-wise as `[l01, x, 0, y]`; the inverse of a
    // lower triangular matrix in the same layout.
    Some(Rest {
        inv: [1.0 / l01, -x / det, 0.0, 1.0 / y],
        flaeche: 0.5 * det,
    })
}

impl Membran {
    /// Builds the membrane of one triangle.
    ///
    /// `ruhe` are the three pattern rest vertices (m, reduced) — only the
    /// direction of edge `0 → 1` is taken from them, because the rebuilt rest
    /// triangle above is aligned to `+x`. `kanten` are the three eased rest
    /// edges `(l₀₁, l₁₂, l₂₀)` in m, `dir2` the piece's grainline in pattern
    /// coordinates (any length) and `material` the three stiffnesses
    /// `(E_k, E_s, G)` in N/m.
    ///
    /// Returns `None` for a triangle without rest area; the caller counts it.
    pub fn bauen(
        ruhe: [[f64; 2]; 3],
        kanten: [f64; 3],
        dir2: [f64; 2],
        material: (f64, f64, f64),
    ) -> Option<Membran> {
        let (ruhe_inv, flaeche) = {
            let r = rest_aus_kanten(kanten)?;
            (r.inv, r.flaeche)
        };
        let d01 = sub([ruhe[1][0], ruhe[1][1], 0.0], [ruhe[0][0], ruhe[0][1], 0.0]);
        let l01 = norm(d01);
        if !(l01 > 0.0) {
            return None;
        }
        // Turn the pattern directions into the frame of the rebuilt rest
        // triangle, where edge 0 → 1 runs along +x.
        //
        // Only the **rotation** belongs here. `F = [x₁−x₀, x₂−x₀]·D⁻¹` already
        // maps the orthonormal reference triangle onto the piece, so a material
        // direction is a **unit** vector of that reference triangle: the pattern
        // angle to edge `0 → 1`, carried along. Applying `D⁻¹` a second time
        // would read the direction as edge coefficients and give a basis that
        // only coincides with the right one when `D = I` — true for the unit
        // test triangle and false for every real one.
        let (c, s) = (d01[0] / l01, d01[1] / l01);
        let dreh = |v: [f64; 2]| [c * v[0] + s * v[1], -s * v[0] + c * v[1]];
        let nach_ruhe = |v: [f64; 2]| {
            let l = norm([v[0], v[1], 0.0]);
            if l > 0.0 {
                let w = dreh([v[0] / l, v[1] / l]);
                Some(w)
            } else {
                None
            }
        };
        let kette = nach_ruhe(dir2)?;
        // The weft is the pattern perpendicular; orthonormalised against the
        // warp so the Green strain components are taken in an orthonormal
        // basis.
        let q = nach_ruhe([-dir2[1], dir2[0]])?;
        let along = q[0] * kette[0] + q[1] * kette[1];
        let w = [q[0] - along * kette[0], q[1] - along * kette[1]];
        let lw = norm([w[0], w[1], 0.0]);
        if !(lw > 1e-12) {
            return None;
        }
        let schuss = [w[0] / lw, w[1] / lw];
        Some(Membran {
            flaeche,
            ruhe_inv,
            kette,
            schuss,
            kette_n_m: material.0,
            dehnung_n_m: material.1,
            schub_n_m: material.2,
        })
    }

    /// `(a3(kette), a3(schuss), c3(kette, schuss))` — the three Voigt rows that
    /// read the strain out of `g`. Derived, not stored: the element is streamed
    /// per CG iteration, so every field counts against the cache.
    #[inline]
    fn basis(&self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        (
            a3(self.kette),
            a3(self.schuss),
            c3(self.kette, self.schuss),
        )
    }

    /// `[−(r₀+r₂), r₀, r₂]`: how `∂f₁/∂xₖ` scales the rest matrix.
    #[inline]
    fn c1(&self) -> [f64; 3] {
        let r = self.ruhe_inv;
        [-(r[0] + r[2]), r[0], r[2]]
    }

    /// `[−(r₁+r₃), r₁, r₃]`: how `∂f₂/∂xₖ` scales the rest matrix.
    #[inline]
    fn c2(&self) -> [f64; 3] {
        let r = self.ruhe_inv;
        [-(r[1] + r[3]), r[1], r[3]]
    }

    /// `F = [x₁ − x₀, x₂ − x₀]·D⁻¹` as its two columns.
    fn f(&self, p: [[f64; 3]; 3]) -> ([f64; 3], [f64; 3]) {
        let (e1, e2) = (sub(p[1], p[0]), sub(p[2], p[0]));
        let r = self.ruhe_inv;
        (
            add(mul(e1, r[0]), mul(e2, r[2])),
            add(mul(e1, r[1]), mul(e2, r[3])),
        )
    }

    /// `g = (g₁₁, g₂₂, 2g₁₂)` with `g = FᵀF`, from the already built columns.
    #[inline]
    fn g_aus(&self, f1: [f64; 3], f2: [f64; 3]) -> [f64; 3] {
        [dot(f1, f1), dot(f2, f2), 2.0 * dot(f1, f2)]
    }

    /// The Green strain in the material basis: `(ε_warp, ε_weft, ε_shear)`.
    pub fn dehnung(&self, p: [[f64; 3]; 3]) -> [f64; 3] {
        let (f1, f2) = self.f(p);
        self.dehnung_aus(f1, f2)
    }

    /// [`Self::dehnung`] for a caller that already has the columns.
    #[inline]
    fn dehnung_aus(&self, f1: [f64; 3], f2: [f64; 3]) -> [f64; 3] {
        let (a, b, c) = self.basis();
        let g = self.g_aus(f1, f2);
        [
            0.5 * (dot(a, g) - 1.0),
            0.5 * (dot(b, g) - 1.0),
            0.5 * dot(c, g),
        ]
    }

    /// The stress that belongs to [`Self::dehnung`], per unit rest width (N/m):
    /// `(σ_warp, σ_weft, τ)`. `τ = G·γ` with the engineering shear
    /// `γ = 2·ε_shear`.
    pub fn spannung(&self, p: [[f64; 3]; 3]) -> [f64; 3] {
        let e = self.dehnung(p);
        [
            self.kette_n_m * e[0],
            self.dehnung_n_m * e[1],
            2.0 * self.schub_n_m * e[2],
        ]
    }

    /// The strain energy of this triangle (J).
    pub fn energie(&self, p: [[f64; 3]; 3]) -> f64 {
        let e = self.dehnung(p);
        self.flaeche
            * (0.5 * self.kette_n_m * e[0] * e[0]
                + 0.5 * self.dehnung_n_m * e[1] * e[1]
                + 2.0 * self.schub_n_m * e[2] * e[2])
    }

    /// `∂U/∂g` in the Voigt convention of [`Self::g`].
    pub(crate) fn dU_dg(&self, p: [[f64; 3]; 3]) -> [f64; 3] {
        self.dU_dg_aus(self.dehnung(p))
    }

    /// `∂U/∂g` for a given strain — the caller may pass a partly clamped one
    /// (see [`Self::geo_w_aus`]).
    fn dU_dg_aus(&self, e: [f64; 3]) -> [f64; 3] {
        let (a, b, c) = self.basis();
        let mut out = [0.0; 3];
        for i in 0..3 {
            out[i] = self.flaeche
                * (0.5 * self.kette_n_m * e[0] * a[i]
                    + 0.5 * self.dehnung_n_m * e[1] * b[i]
                    + 2.0 * self.schub_n_m * e[2] * c[i]);
        }
        out
    }

    /// `∂²U/∂g²` in the Voigt convention of [`Self::g`]; symmetric, positive
    /// semi-definite (definite on a non-degenerate triangle). It does not depend
    /// on the positions — the energy is quadratic in the strain.
    fn kg_aus_basis(&self) -> [[f64; 3]; 3] {
        let (a, b, c) = self.basis();
        let mut m = [[0.0f64; 3]; 3];
        for (w, v) in [
            (0.25 * self.kette_n_m, a),
            (0.25 * self.dehnung_n_m, b),
            (self.schub_n_m, c),
        ] {
            for i in 0..3 {
                for k in 0..3 {
                    m[i][k] += self.flaeche * w * v[i] * v[k];
                }
            }
        }
        m
    }

    /// [`Self::kg_aus_basis`] as the solvers read it.
    #[inline]
    pub(crate) fn d2U_dg2(&self) -> [[f64; 3]; 3] {
        self.kg_aus_basis()
    }

    /// `∂²U/∂g²·q`, applied rank-one instead of assembled.
    ///
    /// [`Self::kg_aus_basis`] is a sum of three rank-one terms
    /// `flaeche·w·v vᵀ`, so the product is three dot products and three
    /// axpys — the 3×3 matrix (72 bytes) is never formed. It is used on every
    /// CG iteration, on every triangle.
    #[inline]
    fn kg_mal(&self, q: [f64; 3]) -> [f64; 3] {
        let (a, b, c) = self.basis();
        let mut w = [0.0f64; 3];
        for (s, v) in [
            (0.25 * self.kette_n_m, a),
            (0.25 * self.dehnung_n_m, b),
            (self.schub_n_m, c),
        ] {
            let t = self.flaeche * s * dot(v, q);
            for i in 0..3 {
                w[i] += t * v[i];
            }
        }
        w
    }

    /// `Jₖ = ∂g/∂xₖ`: 3 rows (`g₁₁`, `g₂₂`, `2g₁₂`), 3 columns (x, y, z).
    /// `J₀ + J₁ + J₂ = 0`, because `g` is translation invariant.
    pub(crate) fn jacobi(&self, p: [[f64; 3]; 3]) -> [[[f64; 3]; 3]; 3] {
        let (f1, f2) = self.f(p);
        self.jacobi_aus(f1, f2)
    }

    /// [`Self::jacobi`] from the columns of `F`.
    #[inline]
    fn jacobi_aus(&self, f1: [f64; 3], f2: [f64; 3]) -> [[[f64; 3]; 3]; 3] {
        let r = self.ruhe_inv;
        let zeile = |a: f64, b: f64| -> [f64; 3] {
            [
                a * f1[0] + b * f2[0],
                a * f1[1] + b * f2[1],
                a * f1[2] + b * f2[2],
            ]
        };
        let j1 = [
            zeile(2.0 * r[0], 0.0),
            zeile(0.0, 2.0 * r[1]),
            zeile(2.0 * r[1], 2.0 * r[0]),
        ];
        let j2 = [
            zeile(2.0 * r[2], 0.0),
            zeile(0.0, 2.0 * r[3]),
            zeile(2.0 * r[3], 2.0 * r[2]),
        ];
        let mut j0 = [[0.0f64; 3]; 3];
        for i in 0..3 {
            for k in 0..3 {
                j0[i][k] = -(j1[i][k] + j2[i][k]);
            }
        }
        [j0, j1, j2]
    }

    /// The nodal forces `−∂U/∂xₖ`, one per vertex, in order `(0, 1, 2)`.
    pub fn krafte(&self, p: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        let d = self.dU_dg(p);
        self.kraefte_aus(p, d)
    }

    /// The nodal forces with only the **tensile** part of the stress: what the
    /// fabric pulls with, for the friction estimate (the same "tension only"
    /// rule the distance constraints use with `l > l₀`). A compressed triangle
    /// pushes, and a push is not a load that holds a point on the body.
    pub fn krafte_zug(&self, p: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        let e = self.dehnung(p);
        let (a, b, c) = self.basis();
        let mut d = [0.0f64; 3];
        for i in 0..3 {
            d[i] = self.flaeche
                * (0.5 * self.kette_n_m * e[0].max(0.0) * a[i]
                    + 0.5 * self.dehnung_n_m * e[1].max(0.0) * b[i]
                    + 2.0 * self.schub_n_m * e[2] * c[i]);
        }
        self.kraefte_aus(p, d)
    }

    /// `−Jₖᵀ·d` for a virtual work vector `d = ∂U/∂g`, without building `J`.
    ///
    /// Every row `Jᵢₖ` of the Jacobian is `cᵢₖ·f₁ + c'ᵢₖ·f₂` with the
    /// coefficients of [`Self::c1`]/[`Self::c2`] (row `g₁₁` only `f₁`, row
    /// `g₂₂` only `f₂`, row `2g₁₂` both), so
    /// `Jₖᵀ·d = (Σᵢ dᵢ·cᵢₖ)·f₁ + (Σᵢ dᵢ·c'ᵢₖ)·f₂` — two scalar combinations of
    /// `f₁` and `f₂` per vertex instead of a 3×3×3 array.
    fn kraefte_aus(&self, p: [[f64; 3]; 3], d: [f64; 3]) -> [[f64; 3]; 3] {
        let (f1, f2) = self.f(p);
        let (c1, c2) = (self.c1(), self.c2());
        // Coefficients per vertex. Row `2g₁₂` mixes both columns, and every row
        // carries the factor 2 of `∂g/∂x` (see [`Self::jacobi`]).
        let mut out = [[0.0f64; 3]; 3];
        for k in 0..3 {
            let s1 = 2.0 * (d[0] * c1[k] + d[2] * c2[k]);
            let s2 = 2.0 * (d[1] * c2[k] + d[2] * c1[k]);
            out[k] = [
                -(s1 * f1[0] + s2 * f2[0]),
                -(s1 * f1[1] + s2 * f2[1]),
                -(s1 * f1[2] + s2 * f2[2]),
            ];
        }
        out
    }

    /// The same element as three scalar constraints for XPBD
    /// ([`crate::cpu`]): warp strain, weft strain and the engineering shear
    /// angle `γ = 2·ε_ks`, each with its gradient per vertex and its compliance
    /// `α = 1/(K·A₀)` (energy `½·K·A₀·C²`).
    pub fn zuege(&self, p: [[f64; 3]; 3]) -> [Zug; 3] {
        let e = self.dehnung(p);
        let j = self.jacobi(p);
        let (a, b, c) = self.basis();
        let grad = |v: [f64; 3], halb: bool| -> [[f64; 3]; 3] {
            let mut g = [[0.0f64; 3]; 3];
            for k in 0..3 {
                for col in 0..3 {
                    let mut s = 0.0;
                    for i in 0..3 {
                        s += j[k][i][col] * v[i];
                    }
                    g[k][col] = if halb { 0.5 * s } else { s };
                }
            }
            g
        };
        let fl = self.flaeche;
        let alpha = |k: f64| if k > 0.0 && fl > 0.0 { 1.0 / (k * fl) } else { f64::INFINITY };
        [
            Zug { c: e[0], grad: grad(a, true), alpha: alpha(self.kette_n_m) },
            Zug { c: e[1], grad: grad(b, true), alpha: alpha(self.dehnung_n_m) },
            // Shear: `½·G·A₀·γ²` is the `2·G·ε_ks²·A₀` of [`Self::energie`].
            Zug { c: 2.0 * e[2], grad: grad(c, false), alpha: alpha(self.schub_n_m) },
        ]
    }

    /// The geometric part of the Hessian, `(∂U/∂g_i)·∂²g_i/∂x_a∂x_b`, reduced
    /// to the three scalars it is made of.
    ///
    /// Written out, the term is `2·W` with `W = [[d₁, d₃], [d₃, d₂]]` in the
    /// warp/weft pair `(c₁, c₂)`; every 3×3 block of the assembled matrix is
    /// `2·(w₁₁·c1ₐc1_b + w₂₂·c2ₐc2_b + w₁₂·(c1ₐc2_b + c2ₐc1_b))·I`, because `g`
    /// is a sum of squared edge vectors. So `(w₁₁, w₂₂, w₁₂)` carries the whole
    /// thing, and [`crate::kontinuum::Hessen`] stores exactly these three
    /// numbers instead of nine.
    ///
    /// This is the term that gives a **flat** triangle its out-of-plane
    /// stiffness: an in-plane strain does not change to first order when the
    /// triangle is pushed sideways, so the material part alone reports zero and
    /// the CG sees a singular operator there. A distance constraint carries the
    /// same term as its `quer` factor, `k·(1 − l₀/l)`.
    ///
    /// **Tensile only, and projected onto the PSD cone.** `d₁` and `d₂` are the
    /// normal stresses and are clamped at zero, like the `quer` factor of a
    /// distance constraint, so compression cannot pull. That leaves the shear
    /// stress `d₃`, which is signed, and a pure shear state (`d₁ = d₂ = 0`,
    /// `d₃ ≠ 0`) makes `W` indefinite — the exact energy is quartic in the
    /// positions, so its Hessian genuinely is not positive. A CG cannot solve
    /// with an indefinite operator; it is the one thing that made the membrane
    /// diverge (frame 0, residual growing past `‖b‖`) while the spring net
    /// stayed stable.
    ///
    /// So `W` is replaced by the nearest positive semi-definite matrix
    /// (Frobenius norm): the negative eigenvalue is dropped and the positive
    /// one kept. Where the exact Hessian is already PSD — every tensile state —
    /// the projection is the identity, so the tangent there is still the exact
    /// derivative of the force.
    fn geo_w_aus(&self, f1: [f64; 3], f2: [f64; 3]) -> (f64, f64, f64) {
        let e = self.dehnung_aus(f1, f2);
        // `dU/dg` with the compressive part of the two normal strains dropped;
        // the shear keeps its sign.
        let d = self.dU_dg_aus([e[0].max(0.0), e[1].max(0.0), e[2]]);
        // The 2×2 block in the (c₁, c₂) basis, projected onto the PSD cone.
        psd2(d[0], d[1], d[2])
    }

    /// `H·Δ` for [`Membran::krafte`]'s Hessian: the material part
    /// `(Jₖᵀ·∂²U/∂g²·J_l)·Δ` plus the geometric part from [`Self::geo_w_aus`].
    ///
    /// Without the second term the operator is singular on a flat piece — the
    /// force does respond to a sideways push (that is `∂²g/∂x²`, not
    /// `∂g/∂x`), and a solve that ignores it overshoots instead of converging.
    pub fn mal_h(&self, p: [[f64; 3]; 3], delta: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        self.mal_h_aus(self.hessen(p), delta)
    }

    /// What [`Self::mal_h`] needs from the build positions, computed once.
    ///
    /// The implicit solvers hold the tangent fixed over the iterations of one
    /// substep ([`crate::implizit::System::mal_h`]), so the caller builds this
    /// once and hands it to every [`Self::mal_h_aus`] call.
    pub fn hessen(&self, p: [[f64; 3]; 3]) -> Hessen {
        let (f1, f2) = self.f(p);
        let w = self.geo_w_aus(f1, f2);
        Hessen { f1, f2, w: [w.0, w.1, w.2] }
    }

    /// [`Self::mal_h`] from a Hessian that was built once for the substep.
    #[inline]
    pub fn mal_h_aus(&self, h: Hessen, delta: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
        let (f1, f2) = (h.f1, h.f2);
        // Every row of the Jacobian is `2·c1ₖ·f₁` (row `g₁₁`), `2·c2ₖ·f₂`
        // (row `g₂₂`) or `2·c2ₖ·f₁ + 2·c1ₖ·f₂` (row `2g₁₂`), so both `Jₖᵀ·δₖ`
        // and `Jₖᵀ·w` collapse to one scalar pair per vertex. Neither `J`
        // (27 entries) nor the 27-term product is formed.
        let (c1, c2) = (self.c1(), self.c2());
        let mut q = [0.0f64; 3];
        for l in 0..3 {
            let (d1, d2) = (dot(f1, delta[l]), dot(f2, delta[l]));
            q[0] += 2.0 * c1[l] * d1;
            q[1] += 2.0 * c2[l] * d2;
            q[2] += 2.0 * (c2[l] * d1 + c1[l] * d2);
        }
        let w = self.kg_mal(q);
        let mut out = [[0.0f64; 3]; 3];
        for k in 0..3 {
            let s1 = 2.0 * (c1[k] * w[0] + c2[k] * w[2]);
            let s2 = 2.0 * (c2[k] * w[1] + c1[k] * w[2]);
            out[k] = [
                s1 * f1[0] + s2 * f2[0],
                s1 * f1[1] + s2 * f2[1],
                s1 * f1[2] + s2 * f2[2],
            ];
        }
        // Geometric part, expanded from the three projected scalars:
        // `s_ab = p_a·c1_b + q_a·c2_b` with `p_a = 2·(w₁₁·c1_a + w₁₂·c2_a)` and
        // `q_a = 2·(w₁₂·c1_a + w₂₂·c2_a)`. Then
        // `out_ac += Σ_b s_ab·Δ_bc = p_a·T1_c + q_a·T2_c` with
        // `T1_c = Σ_b c1_b·Δ_bc`, `T2_c = Σ_b c2_b·Δ_bc` — two dot products
        // instead of nine per (a, c).
        let (w11, w22, w12) = (h.w[0], h.w[1], h.w[2]);
        let mut t1 = [0.0f64; 3];
        let mut t2 = [0.0f64; 3];
        for b in 0..3 {
            for c in 0..3 {
                t1[c] += c1[b] * delta[b][c];
                t2[c] += c2[b] * delta[b][c];
            }
        }
        for a in 0..3 {
            let pa = 2.0 * (w11 * c1[a] + w12 * c2[a]);
            let qa = 2.0 * (w12 * c1[a] + w22 * c2[a]);
            for c in 0..3 {
                out[a][c] += pa * t1[c] + qa * t2[c];
            }
        }
        out
    }

    /// The three 3×3 diagonal blocks of `H`, one per vertex — what the
    /// Jacobi preconditioner needs. Material and geometric part, so the z row
    /// of a flat triangle is not zero.
    pub fn diag_bloecke(&self, p: [[f64; 3]; 3]) -> [[[f64; 3]; 3]; 3] {
        self.diag_bloecke_aus(self.hessen(p))
    }

    /// [`Self::diag_bloecke`] from a Hessian that was built once for the
    /// substep (see [`Self::hessen`]).
    pub fn diag_bloecke_aus(&self, h: Hessen) -> [[[f64; 3]; 3]; 3] {
        let j = self.jacobi_aus(h.f1, h.f2);
        let kg = self.d2U_dg2();
        let mut out = [[[0.0f64; 3]; 3]; 3];
        for k in 0..3 {
            // tmp = kg · Jₖ, then Jₖᵀ · tmp.
            let mut tmp = [[0.0f64; 3]; 3];
            for i in 0..3 {
                for c in 0..3 {
                    for m in 0..3 {
                        tmp[i][c] += kg[i][m] * j[k][m][c];
                    }
                }
            }
            for a in 0..3 {
                for c in 0..3 {
                    for i in 0..3 {
                        out[k][a][c] += j[k][i][a] * tmp[i][c];
                    }
                }
            }
        }
        // Geometric part: `(sₖₖ)·I`, the diagonal block of the geometric term,
        // with `sₖₖ = 2·(w₁₁·c1ₖ² + w₂₂·c2ₖ² + 2·w₁₂·c1ₖ·c2ₖ)`.
        let (w11, w22, w12) = (h.w[0], h.w[1], h.w[2]);
        let (c1, c2) = (self.c1(), self.c2());
        for k in 0..3 {
            let s = 2.0 * (w11 * c1[k] * c1[k] + w22 * c2[k] * c2[k] + 2.0 * w12 * c1[k] * c2[k]);
            for c in 0..3 {
                out[k][c][c] += s;
            }
        }
        out
    }
}
