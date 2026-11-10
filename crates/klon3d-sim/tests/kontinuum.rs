//! Acceptance for the membrane (stage 3, task #127).
//!
//! The membrane replaces the per-edge stretch springs with one element per
//! triangle and takes warp stretch, weft stretch and shear as **three
//! independent** stiffnesses. What that has to deliver is checked here, in this
//! order:
//!
//! 1. **tension along the grainline** — a piece of `RADICAL_RUBBER_04` deformed
//!    affinely in warp direction has `E_eff = σ/ε = kette_n_m` (`fSvK`) within
//!    the stated tolerance;
//! 2. **pure shear** — the same piece sheared affinely has `G_eff = τ/γ =
//!    schub_n_m` (`fHK`);
//! 3. **ν follows, it is not set** — from the two measured stiffnesses alone,
//!    `ν = E/(2G) − 1` is ≈ 0 for latex; no code carries a Poisson ratio;
//! 4. **anisotropy** — an invented profile with warp ≠ weft shows both
//!    directions separately, and a grainline turned by 90° swaps which stiffness
//!    an x-stretch reads;
//! 5. **no false energy** — a rigid rotation of the whole triangle changes
//!    neither the strain nor the energy (the energy is built on `FᵀF`);
//! 6. **the force is the gradient** — the nodal forces equal the negative
//!    central difference of the energy, so force and tangent belong to the same
//!    energy;
//! 7. **the tangent is the derivative of the force** — `H·Δ` reproduces the
//!    central difference of the force, including the out-of-plane stiffness a
//!    flat triangle needs;
//! 8. **tension through the solver** — the same 20 × 4 cm strip of
//!    `RADICAL_RUBBER_04`, clamped at both short edges and stretched 2 % in the
//!    warp, reaches the imposed strain and reads `E` back from the membrane at
//!    the **solved** positions. It also keeps its width: `ν ≈ 0` in the middle,
//!    which is the part a distance-spring net provably cannot do — that net is
//!    forced to `ν = G/E = 0.33` and would contract by 0.33 · 2 % of the width;
//! 9. **shear through the solver** — the same strip, every boundary point walked
//!    to the affine image of a simple shear of 4 %, reads `G`;
//! 10. **the basis in a non-unit triangle** — a stretch along the grainline is
//!    pure warp strain even when the reference triangle is not the unit one.
//!    That is the case that hid the first basis bug: with `D⁻¹ ≠ I` a wrong
//!    basis still passed every unit-triangle test.
//!
//! All of it runs without CLO data. Items 1–7 and 10 need no solver; 8 and 9 go
//! through [`ImplizitCg`] and the real scene builder.
//!
//! **What `E_eff` means here.** The material reports stress per unit **rest**
//! width on the Green strain `ε = ½(λ² − 1)`. A tensile test reports the
//! *engineering* strain `e = λ − 1`, so the secant modulus is
//! `σ/e = E·(λ + 1)/2` — 2.5 % above `E` at 5 % stretch, exactly second order.
//! The tolerance covers that and still catches the factor-2 and factor-3/8
//! mistakes this task is about.

use std::collections::HashMap;

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::read_str;
use klon3d_sim::kontinuum::Membran;
use klon3d_sim::roehre::{bis_zur_ruhe, rechteck_muster};
use klon3d_sim::{
    ImplizitCg, ImplizitParameter, Rechenweg, Simparameter, Szene, Teilvorgabe,
    RADICAL_RUBBER_04,
};

/// Tolerance for a stiffness read back from an affine deformation: 5 % covers
/// `(λ+1)/2 = 1.025` at `λ = 1.05` and every second-order term, and still
/// catches a wrong factor.
const TOLERANZ: f64 = 0.05;

/// A right triangle in pattern coordinates (m) with legs `breite` (x) and
/// `hoehe` (y), and its rest edge lengths `(l₀₁, l₁₂, l₂₀)`.
fn dreieck(breite: f64, hoehe: f64) -> ([[f64; 2]; 3], [f64; 3]) {
    let ruhe = [[0.0, 0.0], [breite, 0.0], [0.0, hoehe]];
    let l12 = (breite * breite + hoehe * hoehe).sqrt();
    (ruhe, [breite, l12, hoehe])
}

/// The membrane of the **unit** right triangle (`legs 1, 1`), grainline in x,
/// with stiffnesses `(E_k, E_s, G)`.
///
/// The unit triangle matters: [`Membran`] rebuilds its reference from the edge
/// lengths, and only on the unit triangle does the deformation below read as
/// the deformation gradient itself.
fn einheit(e: (f64, f64, f64), dir2: [f64; 2]) -> Membran {
    let (ruhe, kanten) = dreieck(1.0, 1.0);
    Membran::bauen(ruhe, kanten, dir2, e).expect("Einheitsdreieck hat eine Membran")
}

/// `E_eff` of a uniaxial stretch along `x`: `F = diag(λ, 1)` (no transverse
/// contraction, which is what `ν ≈ 0` means), read as `σ/e` with the
/// engineering strain `e = λ − 1`.
fn zug_e_eff(m: &Membran, lambda: f64) -> f64 {
    let p = [[0.0, 0.0, 0.0], [lambda, 0.0, 0.0], [0.0, 1.0, 0.0]];
    m.spannung(p)[0] / (lambda - 1.0)
}

/// `G_eff` of a pure shear `F = [[1, γ], [0, 1]]`, read as `τ/γ`.
fn schub_g_eff(m: &Membran, gamma: f64) -> f64 {
    let p = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [gamma, 1.0, 0.0]];
    m.spannung(p)[2] / gamma
}

/// 1. Uniaxial stretch along the grainline reads the fabric's warp stiffness.
#[test]
fn zug_liest_die_kettensteifigkeit() {
    let m = einheit(
        (
            RADICAL_RUBBER_04.kette_n_m,
            RADICAL_RUBBER_04.dehnung_n_m,
            RADICAL_RUBBER_04.schub_n_m,
        ),
        [1.0, 0.0],
    );
    let soll = RADICAL_RUBBER_04.kette_n_m;
    for lambda in [1.02, 1.05] {
        let e = zug_e_eff(&m, lambda);
        assert!(
            (e / soll - 1.0).abs() <= TOLERANZ,
            "λ = {lambda}: E_eff {e:.3} gegen Kette {soll:.3} N/m, Abweichung {:.2} %",
            (e / soll - 1.0) * 100.0
        );
    }
}

/// 2. Pure shear reads the fabric's shear stiffness.
#[test]
fn schub_liest_die_schubsteifigkeit() {
    let m = einheit(
        (
            RADICAL_RUBBER_04.kette_n_m,
            RADICAL_RUBBER_04.dehnung_n_m,
            RADICAL_RUBBER_04.schub_n_m,
        ),
        [1.0, 0.0],
    );
    let soll = RADICAL_RUBBER_04.schub_n_m;
    for gamma in [0.02, 0.05] {
        let g = schub_g_eff(&m, gamma);
        assert!(
            (g / soll - 1.0).abs() <= TOLERANZ,
            "γ = {gamma}: G_eff {g:.3} gegen Schub {soll:.3} N/m, Abweichung {:.2} %",
            (g / soll - 1.0) * 100.0
        );
    }
}

/// 3. Poisson's ratio follows from E and G; latex measures G/E = 0.5001, so
/// ν ≈ 0. The check derives ν, it does not read it anywhere — and the model has
/// no place to store it.
#[test]
fn ni_folgt_aus_ketten_und_schub() {
    let e = RADICAL_RUBBER_04.kette_n_m;
    let g = RADICAL_RUBBER_04.schub_n_m;
    let ni = e / (2.0 * g) - 1.0;
    assert!(ni.abs() < 0.01, "ν = {ni:.5} aus E = {e:.3} und G = {g:.3}");

    // Und der Beleg am Element: ohne aufgeprägte Querkontraktion bleibt die
    // Querspannung null und die Querstreckung eins.
    let m = einheit((e, RADICAL_RUBBER_04.dehnung_n_m, g), [1.0, 0.0]);
    let p = [[0.0, 0.0, 0.0], [1.05, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let s = m.spannung(p);
    assert!(s[1].abs() < 1e-9, "Querspannung {} bei ν = 0", s[1]);
}

/// 4. An invented profile with warp ≠ weft shows the anisotropy in both
/// directions, and a grainline turned by 90° swaps which stiffness an x-stretch
/// reads.
#[test]
fn anisotropes_profil_zeigt_beide_richtungen() {
    let steif = (500.0, 200.0, 150.0);
    let m = einheit(steif, [1.0, 0.0]);
    let lam = 1.05;

    // Zug in x liest die Kette.
    let p_x = [[0.0, 0.0, 0.0], [lam, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let s_x = m.spannung(p_x);
    assert!(
        (s_x[0] / (lam - 1.0) / steif.0 - 1.0).abs() <= TOLERANZ,
        "Kette: {:.3} gegen {}",
        s_x[0] / (lam - 1.0),
        steif.0
    );
    // Zug in y liest den Schuss.
    let p_y = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, lam, 0.0]];
    let s_y = m.spannung(p_y);
    assert!(
        (s_y[1] / (lam - 1.0) / steif.1 - 1.0).abs() <= TOLERANZ,
        "Schuss: {:.3} gegen {}",
        s_y[1] / (lam - 1.0),
        steif.1
    );
    // Die Antworten verhalten sich wie die Steifigkeiten.
    let verhaeltnis = (s_x[0] / s_y[1]) / (steif.0 / steif.1);
    assert!(
        (verhaeltnis - 1.0).abs() <= TOLERANZ,
        "Verhältnis {verhaeltnis:.3} statt {}",
        steif.0 / steif.1
    );

    // Faser um 90° gedreht: derselbe Zug in x ist jetzt Zug im Schuss und
    // liest die Schusssteifigkeit.
    let gedreht = einheit(steif, [0.0, 1.0]);
    let s = gedreht.spannung(p_x);
    assert!(
        (s[1] / (lam - 1.0) / steif.1 - 1.0).abs() <= TOLERANZ,
        "gedreht: {:.3} gegen Schuss {}",
        s[1] / (lam - 1.0),
        steif.1
    );
    // Kette und Schuss sind vertauscht, nicht verwechselt: der Zug in y liest
    // jetzt die Kette.
    let s = gedreht.spannung(p_y);
    assert!(
        (s[0] / (lam - 1.0) / steif.0 - 1.0).abs() <= TOLERANZ,
        "gedreht y: {:.3} gegen Kette {}",
        s[0] / (lam - 1.0),
        steif.0
    );
}

/// 5. A rigid rotation changes neither strain nor energy: the energy is built
/// on `FᵀF`, which is rotation invariant.
#[test]
fn starre_drehung_aendert_die_energie_nicht() {
    let m = einheit((348.0, 348.0, 174.0), [1.0, 0.0]);
    let gestreckt = [[0.0, 0.0, 0.0], [1.04, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let w = std::f64::consts::PI / 5.0; // 36°
    let (c, s) = (w.cos(), w.sin());
    let dreh = |p: [f64; 3]| [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]];
    let gedreht = gestreckt.map(dreh);
    let (e0, e1) = (m.energie(gestreckt), m.energie(gedreht));
    assert!(
        (e1 - e0).abs() <= 1e-12 * e0.abs().max(1.0),
        "Energie {e1:.6e} gegen {e0:.6e}"
    );
    let (d0, d1) = (m.dehnung(gestreckt), m.dehnung(gedreht));
    for i in 0..3 {
        assert!(
            (d1[i] - d0[i]).abs() < 1e-12,
            "Dehnung {i}: {:.3e}",
            d1[i] - d0[i]
        );
    }
}

/// 7. The tangent is the derivative of the force: `H·Δ = −FD(f)`. This is the
/// check that catches a material-only operator — without the geometric term a
/// flat triangle has no out-of-plane stiffness at all, and a solve then
/// overshoots instead of converging.
#[test]
fn tangente_ist_die_ableitung_der_kraft() {
    let m = einheit((348.0, 300.0, 174.0), [1.0, 0.0]);
    // Both normal strains tensile (2 % and 1 %), so the geometric term is not
    // clamped: only there is `H` exactly the derivative of the force. Off plane
    // in z, so the geometric term is exercised in all three coordinates.
    let p = [[0.0, 0.0, 0.0], [1.04, 0.002, 0.01], [0.003, 1.02, -0.008]];
    let d = [[-0.7, 0.4, 0.9], [0.3, 0.1, 0.4], [1.3, -0.2, -0.1]];
    let hd = m.mal_h(p, d);
    let eps = 1e-7;
    for k in 0..3 {
        for c in 0..3 {
            let mut plus = p;
            let mut minus = p;
            for b in 0..3 {
                for cc in 0..3 {
                    plus[b][cc] += eps * d[b][cc];
                    minus[b][cc] -= eps * d[b][cc];
                }
            }
            let fp = m.krafte(plus);
            let fm = m.krafte(minus);
            let fd = -(fp[k][c] - fm[k][c]) / (2.0 * eps);
            let skala = hd[k][c].abs().max(fd.abs()).max(1.0);
            assert!(
                (hd[k][c] - fd).abs() <= 2e-5 * skala,
                "H·Δ[{k}][{c}] {:.6e} gegen −FD {fd:.6e}",
                hd[k][c]
            );
        }
    }
    // A flat triangle must still have out-of-plane stiffness: without the
    // geometric term this row is zero and the CG solve explodes.
    let flach = [[0.0, 0.0, 0.0], [1.04, 0.0, 0.0], [0.0, 1.02, 0.0]];
    let quer = m.mal_h(flach, [[0.0, 0.0, 1.0], [0.0; 3], [0.0; 3]]);
    assert!(
        quer[0][2].abs() > 1e-3,
        "flaches Dreieck: z-Steifigkeit {:.3e}, ohne geometrisches Glied null",
        quer[0][2]
    );
    // And the Jacobi diagonal is the diagonal of H — otherwise the
    // preconditioner is singular in z.
    let dg = m.diag_bloecke(p);
    for k in 0..3 {
        for c in 0..3 {
            let mut e = [[0.0f64; 3]; 3];
            e[k][c] = 1.0;
            let spalte = m.mal_h(p, e);
            assert!(
                (dg[k][c][c] - spalte[k][c]).abs() <= 1e-9 * spalte[k][c].abs().max(1.0),
                "Diagonale[{k}][{c}] {:.6e} gegen {:.6e}",
                dg[k][c][c],
                spalte[k][c]
            );
        }
    }
}

/// 6. The nodal forces are the negative gradient of the energy and sum to zero.
#[test]
fn kraft_ist_der_gradient_der_energie() {
    let m = einheit((348.0, 300.0, 174.0), [1.0, 0.0]);
    let p = [[0.0, 0.0, 0.0], [1.04, 0.002, 0.0], [0.003, 0.99, 0.001]];
    let f = m.krafte(p);
    let eps = 1e-7;
    for k in 0..3 {
        for c in 0..3 {
            let mut plus = p;
            let mut minus = p;
            plus[k][c] += eps;
            minus[k][c] -= eps;
            let fd = -(m.energie(plus) - m.energie(minus)) / (2.0 * eps);
            let skala = f[k][c].abs().max(1.0);
            assert!(
                (fd - f[k][c]).abs() <= 1e-5 * skala,
                "Kraft[{k}][{c}] {:.6e} gegen Finite Differenz {fd:.6e}",
                f[k][c]
            );
        }
    }
    let summe: [f64; 3] = (0..3).fold([0.0; 3], |a, k| {
        [a[0] + f[k][0], a[1] + f[k][1], a[2] + f[k][2]]
    });
    for c in 0..3 {
        assert!(summe[c].abs() < 1e-12, "Summe {c}: {:.3e}", summe[c]);
    }
}

// ---------------------------------------------------------------------------
// Through the solver
// ---------------------------------------------------------------------------

/// The test strip: 20 × 4 cm of `RADICAL_RUBBER_04`, flat in the `z = 0` plane,
/// piece takes the substitute axis (`+x`, [`klon3d_sim::faser::ERSATZ`]): the
/// warp runs along the long side, which is what a stretch along x reads.
fn streifen() -> Szene {
    let m = read_str(&rechteck_muster(0.2, 0.04)).unwrap();
    let ev = Auswertung::new(&m, HashMap::new());
    let p = Parameter { kante_mm: 10.0, ..Parameter::default() };
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &[], &p).unwrap();
    assert!(netz.fehler.is_empty(), "{:?}", netz.fehler);
    let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
    let lage = |_ti: usize, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0];
    let param = Simparameter {
        bild_dt: 1.0 / 30.0,
        schwerkraft: [0.0; 3],
        daempfung: 5.0,
        ..Simparameter::default()
    };
    let s = Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), param);
    // Every triangle gets a membrane, and every distance spring is carried by
    // one: the test then measures the continuum and nothing of the old net.
    assert_eq!(s.dreiecke.len(), s.membran_elemente.len(), "Dreiecke ohne Membran");
    assert_eq!(s.membran_entartet, 0);
    assert!(
        s.membran_kante.iter().all(|&b| b),
        "{} von {} Kanten tragen keine Membran",
        s.membran_kante.iter().filter(|b| !**b).count(),
        s.membran_kante.len()
    );
    assert_eq!(s.faser_ersatz, 1, "der Prüfstreifen hat keinen Fadenlauf");
    s
}

/// Clamps both short edges and walks the far one by `weg` in `schritte` steps of
/// `frames_je_schritt` frames. Walking it instead of setting it once means the
/// solve has to carry the strip there; setting it once would start at the
/// answer.
fn ziehen(s: &mut Szene, weg: [f64; 3], schritte: usize, frames_je_schritt: usize) -> usize {
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let nah: Vec<usize> = s.teile[0].punkte.clone().filter(|&i| s.x[i][0].abs() < 1e-6).collect();
    let fern: Vec<usize> = s.teile[0].punkte.clone().filter(|&i| s.x[i][0] > 0.2 - 1e-6).collect();
    assert!(nah.len() >= 2 && fern.len() >= 2, "Rand nicht gefunden: {} nah, {} fern", nah.len(), fern.len());
    for i in nah.into_iter().chain(fern.iter().copied()) {
        s.w[i] = 0.0;
        s.v[i] = [0.0; 3];
    }
    for k in 1..=schritte {
        for &i in &fern {
            s.x[i] = [
                s.x[i][0] + weg[0] / schritte as f64,
                s.x[i][1] + weg[1] / schritte as f64,
                s.x[i][2],
            ];
            s.v[i] = [0.0; 3];
        }
        let _ = k;
        for _ in 0..frames_je_schritt {
            rw.bild(s);
        }
    }
    fern.len()
}

/// Prescribes the affine map `F` (2×2, row-major `[f00, f01, f10, f11]`) on
/// **every** boundary point of the strip and walks there in `schritte` steps.
///
/// All four sides are held on purpose. Prescribing only the short edges works
/// for tension, because a lateral contraction is then free to happen and `ν`
/// shows up by itself. For shear it does not: simple shear carries
/// `τ = G·γ` on the long edges, so a strip with free lateral faces relieves it
/// by **rotating about the clamp** — that is what the first version of this
/// test measured, γ = 0.0034 instead of 0.04. Holding all four sides leaves the
/// homogeneous simple-shear state as the one the interior can relax into, and
/// the reactions on the long edges are exactly the missing traction.
fn affin(s: &mut Szene, f: [f64; 4], schritte: usize, frames_je_schritt: usize) -> usize {
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let rand: Vec<usize> = s.teile[0]
        .punkte
        .clone()
        .filter(|&i| {
            let [x, y, _] = s.x[i];
            x < 1e-6 || x > 0.2 - 1e-6 || y < 1e-6 || y > 0.04 - 1e-6
        })
        .collect();
    assert!(rand.len() > 20, "Rand zu klein: {}", rand.len());
    let x0: Vec<[f64; 3]> = rand.iter().map(|&i| s.x[i]).collect();
    for &i in &rand {
        s.w[i] = 0.0;
        s.v[i] = [0.0; 3];
    }
    for k in 1..=schritte {
        let t = k as f64 / schritte as f64;
        for (&i, p) in rand.iter().zip(&x0) {
            s.x[i] = [
                p[0] + t * (f[0] * p[0] + f[1] * p[1] - p[0]),
                p[1] + t * (f[2] * p[0] + f[3] * p[1] - p[1]),
                p[2],
            ];
            s.v[i] = [0.0; 3];
        }
        for _ in 0..frames_je_schritt {
            rw.bild(s);
        }
    }
    rand.len()
}

/// Mean strain, mean stress and triangle count of the membranes whose centroid
/// sits in the middle band `x ∈ [0,09; 0,11] m` — two widths away from both
/// clamps, so the clamped edges are not measured.
fn mitte(s: &Szene) -> ([f64; 3], [f64; 3], usize) {
    let mut e = [0.0; 3];
    let mut sig = [0.0; 3];
    let mut n = 0usize;
    for me in &s.membran_elemente {
        let p = me.punkte.map(|i| s.x[i as usize]);
        let cx = (p[0][0] + p[1][0] + p[2][0]) / 3.0;
        if !(0.09..=0.11).contains(&cx) {
            continue;
        }
        let d = me.element.dehnung(p);
        let sp = me.element.spannung(p);
        for k in 0..3 {
            e[k] += d[k];
            sig[k] += sp[k];
        }
        n += 1;
    }
    assert!(n > 0, "kein Dreieck in der Messzone");
    let f = n as f64;
    ([e[0] / f, e[1] / f, e[2] / f], [sig[0] / f, sig[1] / f, sig[2] / f], n)
}

/// Settles the scene and reports how it went.
fn zur_ruhe(s: &mut Szene) -> klon3d_sim::roehre::Lauf {
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let lauf = bis_zur_ruhe(s, &mut rw, 600, 1e-4, 10);
    println!("  Ruhelauf: {} Bilder, ruhig = {}, v = {:.3e} m/s", lauf.bilder, lauf.ruhig, lauf.v_mittel);
    lauf
}

/// 8. Tension through the solver, along the grainline: the strip is walked 2 %
/// longer and held. The middle has to carry the imposed strain and read `E`
/// back, and it has to keep its width — `ν ≈ 0`, which is the property a net of
/// distance springs cannot have (there `ν = G/E = 0,33`, so the strip would
/// narrow by 0.66 %).
#[test]
fn zug_durch_den_loeser_liest_die_kettensteifigkeit() {
    let mut s = streifen();
    let l = 0.2;
    let weg = 0.02 * l;
    assert_eq!(ziehen(&mut s, [weg, 0.0, 0.0], 10, 3), 5, "fern: 5 Punkte");
    let lauf = zur_ruhe(&mut s);
    assert!(lauf.ruhig, "nicht zur Ruhe gekommen: {lauf:?}");

    let (e, sig, n) = mitte(&s);
    // Green strain → engineering strain: ε = ½(λ² − 1) ⇒ e = λ − 1 = √(1+2ε) − 1.
    let eps_eng = (1.0 + 2.0 * e[0]).sqrt() - 1.0;
    let eps_soll = weg / l;
    println!(
        "  Zug: e = {:.5} (soll {:.5}), E_eff = {:.3} N/m, Schuss ε = {:+.2e} ({n} Dreiecke)",
        eps_eng,
        eps_soll,
        sig[0] / eps_eng,
        e[1]
    );
    assert!(
        (eps_eng / eps_soll - 1.0).abs() <= TOLERANZ,
        "Dehnung {eps_eng:.5} gegen aufgeprägt {eps_soll:.5}, {:.2} % daneben",
        (eps_eng / eps_soll - 1.0) * 100.0
    );
    let e_eff = sig[0] / eps_eng;
    assert!(
        (e_eff / RADICAL_RUBBER_04.kette_n_m - 1.0).abs() <= TOLERANZ,
        "E_eff {e_eff:.3} gegen Kette {:.3} N/m, {:.2} % daneben",
        RADICAL_RUBBER_04.kette_n_m,
        (e_eff / RADICAL_RUBBER_04.kette_n_m - 1.0) * 100.0
    );
    // ν = ε_quer/ε_längs im gelösten Zustand. Ein Federnetz gäbe hier 0,33.
    let ni = e[1] / e[0];
    assert!(
        ni.abs() <= 0.05,
        "ν = {ni:.3} im gelösten Zustand, erwartet ≈ 0 (Federnetz: 0,33)"
    );
    // Und die Schubspannung bleibt bei reinem Zug null.
    assert!(sig[2].abs() <= 1e-6 * sig[0].abs().max(1.0), "Schub {:.3e} bei reinem Zug", sig[2]);
}

/// 9. Shear through the solver: every boundary point is walked to the affine
/// image of a simple shear `F = [[1,0],[γ,1]]`, which `γ = 2·ε₁₂` reads exactly.
/// The middle has to carry that `γ` and read `G` back.
///
/// All four sides are held, unlike the tension test: a simple shear carries
/// `τ = G·γ` on the long edges, so with free lateral faces the strip relieves it
/// by rotating about the clamp instead of shearing. What the long edges then
/// hold is exactly that missing traction, and the interior has one state left.
#[test]
fn schub_durch_den_loeser_liest_die_schubsteifigkeit() {
    let mut s = streifen();
    let gamma = 0.04;
    // F = [[1, 0], [γ, 1]]: y' = y + γ·x, x' = x.
    let rand = affin(&mut s, [1.0, 0.0, gamma, 1.0], 10, 3);
    assert!(rand > 20, "Rand zu klein: {rand}");
    let lauf = zur_ruhe(&mut s);
    assert!(lauf.ruhig, "nicht zur Ruhe gekommen: {lauf:?}");

    let (e, sig, n) = mitte(&s);
    let g_ist = 2.0 * e[2];
    println!(
        "  Schub: γ = {:.5} (soll {:.5}), G_eff = {:.3} N/m ({n} Dreiecke)",
        g_ist,
        gamma,
        sig[2] / g_ist
    );
    assert!(
        (g_ist / gamma - 1.0).abs() <= TOLERANZ,
        "γ {g_ist:.5} gegen aufgeprägt {gamma:.5}, {:.2} % daneben",
        (g_ist / gamma - 1.0) * 100.0
    );
    let g_eff = sig[2] / g_ist;
    assert!(
        (g_eff / RADICAL_RUBBER_04.schub_n_m - 1.0).abs() <= TOLERANZ,
        "G_eff {g_eff:.3} gegen Schub {:.3} N/m, {:.2} % daneben",
        RADICAL_RUBBER_04.schub_n_m,
        (g_eff / RADICAL_RUBBER_04.schub_n_m - 1.0) * 100.0
    );
    // Reiner Schub ist keine Normalspannung: die Kettenspannung bleibt klein
    // gegen die Schubspannung.
    assert!(
        sig[0].abs() <= 0.05 * sig[2].abs(),
        "Kettenspannung {:.3} bei reinem Schub {:.3}",
        sig[0],
        sig[2]
    );
}

/// 10. The material basis in a triangle that is **not** the unit one.
///
/// This is the case that hid the first basis bug: [`Membran::bauen`] rebuilt the
/// reference triangle from the eased edge lengths (aligned to `+x`) and then
/// applied the reference matrix **inverse** to the pattern's grainline, which
/// turns a direction into edge coefficients. On the unit right triangle that
/// map is the identity, so all the tests above passed; on a real triangle the
/// warp axis came out rotated — on the 20 × 4 cm strip by about 12°.
///
/// The triangle here is right-angled only in the pattern's own frame: edge
/// `0 → 1` runs along `+x`, but the third vertex sits at `x = 0,05 m`, so the
/// rebuilt rest matrix is not diagonal and `D⁻¹` is not a scaling. The
/// grainline is at 45° to edge `0 → 1`.
#[test]
fn basis_im_nicht_einheitsdreieck() {
    let ruhe = [[0.0, 0.0], [0.2, 0.0], [0.05, 0.04]];
    let l = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let kanten = [l(ruhe[0], ruhe[1]), l(ruhe[1], ruhe[2]), l(ruhe[2], ruhe[0])];
    let steif = (348.0, 348.0, 174.0);
    let m = Membran::bauen(ruhe, kanten, [1.0, 1.0], steif).expect("Dreieck hat eine Membran");

    // The grainline at 45° in pattern coordinates: edge 0 → 1 already points
    // along +x, so the reference frame is the pattern frame and the warp axis is
    // the normalised grainline itself. Nothing may be applied to it but its
    // length.
    let w = 0.5f64.sqrt();
    for (i, soll) in [w, w].iter().enumerate() {
        assert!(
            (m.kette[i] - soll).abs() < 1e-12,
            "Kette {i} = {:.6}, soll {:.6} — die Richtung wurde als Kantenkoeffizient gelesen",
            m.kette[i],
            soll
        );
    }
    // The weft is the perpendicular grainline, unit and orthogonal.
    let along = m.schuss[0] * m.kette[0] + m.schuss[1] * m.kette[1];
    assert!(along.abs() < 1e-12, "Schuss nicht senkrecht: {along:.3e}");
    let ls = (m.schuss[0] * m.schuss[0] + m.schuss[1] * m.schuss[1]).sqrt();
    assert!((ls - 1.0).abs() < 1e-12, "Schuss nicht Einheitsvektor: {ls:.6}");

    // And the energy agrees with the basis: stretching along the warp axis by
    // 5 % (F = I + (λ−1)·k kᵀ in reference coordinates) is pure warp strain.
    let lambda = 1.05;
    let k = m.kette;
    let f = [
        [1.0 + (lambda - 1.0) * k[0] * k[0], (lambda - 1.0) * k[0] * k[1]],
        [(lambda - 1.0) * k[1] * k[0], 1.0 + (lambda - 1.0) * k[1] * k[1]],
    ];
    // E = F·D with D = [p₁−p₀, p₂−p₀]; the current triangle is E's columns.
    let d1 = [ruhe[1][0] - ruhe[0][0], ruhe[1][1] - ruhe[0][1]];
    let d2 = [ruhe[2][0] - ruhe[0][0], ruhe[2][1] - ruhe[0][1]];
    let e = |v: [f64; 2]| [f[0][0] * v[0] + f[0][1] * v[1], f[1][0] * v[0] + f[1][1] * v[1]];
    let p = [[0.0, 0.0, 0.0], [e(d1)[0], e(d1)[1], 0.0], [e(d2)[0], e(d2)[1], 0.0]];
    let dehn = m.dehnung(p);
    let soll = 0.5 * (lambda * lambda - 1.0);
    assert!(
        (dehn[0] - soll).abs() < 1e-12,
        "Kettendehnung {:.6} gegen {soll:.6}",
        dehn[0]
    );
    assert!(dehn[1].abs() < 1e-12, "Schussdehnung {:.3e} bei reinem Kettzug", dehn[1]);
    assert!(dehn[2].abs() < 1e-12, "Schub {:.3e} bei reinem Kettzug", dehn[2]);
    let s = m.spannung(p);
    let e_eff = s[0] / (lambda - 1.0);
    assert!(
        (e_eff / steif.0 - 1.0).abs() <= TOLERANZ,
        "E_eff {e_eff:.3} gegen {:.3} N/m",
        steif.0
    );
    assert!(s[1].abs() < 1e-9, "Querspannung {:.3e} bei ν = 0", s[1]);
    assert!(s[2].abs() < 1e-9, "Schubspannung {:.3e} bei reinem Kettzug", s[2]);
}
