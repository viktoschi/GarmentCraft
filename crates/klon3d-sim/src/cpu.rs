//! Der Rechenweg auf der CPU — die Referenz für die GPU-Fassung (ADR-018).
//!
//! XPBD mit kleinen Unterschritten (Macklin u. a., „Small Steps in Physics
//! Simulation“, 2019): je Unterschritt h
//!
//! 1. Vorhersage: v ← (v + g·h)·(1 − d·h), x ← x + v·h;
//! 2. eine Runde Constraints, Farbe für Farbe (Dehnung, Naht, Biegung). Mit
//!    einer Iteration je Unterschritt beginnt der Lagrange-Multiplikator bei 0:
//!    Δλ = −C / (Σ wₖ |∇ₖC|² + α/h²), Δxₖ = wₖ ∇ₖC · Δλ
//!    (Macklin u. a., „XPBD“, 2016);
//! 3. Kollision mit den Körpern samt Coulomb-Reibung als Lagekorrektur:
//!    die tangentiale Bewegung im Unterschritt, relativ zum Körper, wird ganz
//!    aufgehoben, solange sie unter μ·N bleibt, sonst um μ·N verkürzt
//!    (Macklin u. a., „Unified Particle Physics“, 2014). N ist der größere von
//!    zwei Normalwegen: die Eindringtiefe (Stoß) und F_N·h²/m aus der
//!    **physikalischen** Last (siehe unten);
//! 4. v ← (x − x_alt)/h.
//!
//! ## Warum die Normalkraft aus der Dehnung kommt
//!
//! Hält ein Körper eine Dehnung dauerhaft fest (Röhre auf dem Zylinder),
//! überträgt ein steifer Constraint mit einer Iteration je Unterschritt nur
//! den Bruchteil α̃/(Σw + α̃) seiner Kraft k·C auf den Kontakt (α̃ = α/h²).
//! Die Lage stimmt trotzdem — falsch wäre nur die Reibung: Die Röhre aus
//! Radical-Latex rutschte bei 5,5 % Reduktion vom Zylinder, obwohl Coulomb
//! sie mit fast doppelter Reserve hält (am 20.10.2026 gemessen: Eindringtiefe
//! 0,053 mm je Unterschritt statt 0,42 mm). Deshalb schätzt der Rechenweg die
//! Last je Punkt aus dem Zustand am Anfang des Unterschritts: Zug der Kanten
//! k·max(0, L − L₀) (eine Membran trägt keinen Druck, sie wirft Falten) plus
//! Gewicht, und nimmt davon den Anteil gegen die Kontaktnormale. Die Summe
//! läuft Farbe für Farbe und braucht so auch auf der GPU keine Atomics.
//!
//! Innerhalb einer Farbe teilen Constraints keinen Punkt; die Reihenfolge darin
//! ändert nichts am Ergebnis. Ein Compute-Shader rechnet eine Farbe je Aufruf.
//!
//! ## Lage, Verschiebung, Korrektur — damit f32 reicht
//!
//! Ein Unterschritt verschiebt um g·h² ≈ 10⁻¹⁰…10⁻⁸ m — kleiner als die
//! f32-Auflösung einer Lage auf Körperhöhe (≈ 10⁻⁷ m bei 1,3 m). Mit
//! x ← x + v·h und v = (x − x_alt)/h in absoluten Lagen rundet eine
//! f32-Fassung die Schwerkraft weg (Befund 34, Prüfung). Deshalb drei Felder:
//!
//! * `x` — Lage am Bildanfang, bleibt im Bild fest (x ← x + δ am Ende);
//! * δ — Verschiebung seit Bildanfang, wächst um v·h je Unterschritt;
//! * κ — Korrektur dieses Unterschritts durch Constraints und Kontakt.
//!
//! Gerechnet wird mit p = x + δ + κ, Abstände als Summe der Differenzen.
//! Am Ende des Unterschritts δ ← δ + κ und **v ← v + κ/h** — die
//! Geschwindigkeit entsteht nie aus der Differenz zweier großer Zahlen; in
//! exakter Rechnung ist das dasselbe wie v = (x − x_alt)/h. Auch die Körper
//! werden je Unterschritt aus ihrer Lage am Bildanfang gesetzt.

use crate::koerper::{add, cross, dot, mul, norm, sub};
use crate::szene::Szene;

/// Ein Rechenweg rechnet die Szene um ein Bild weiter. Die GPU-Fassung
/// (Schritt 4b) bekommt dieselbe Schnittstelle.
pub trait Rechenweg {
    fn bild(&mut self, szene: &mut Szene);
}

/// Kontakte des letzten Bildes — zum Nachsehen, ob die Reibung haftet.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Kontaktstatistik {
    /// Kontakte (Punkt × Unterschritt).
    pub kontakte: usize,
    /// davon haftend (tangentiale Bewegung ganz aufgehoben).
    pub haftend: usize,
    /// Summe der Eindringtiefen und der tangentialen Wege (m).
    pub tiefe: f64,
    pub tangential: f64,
}

/// CPU-Referenz in `f64`.
#[derive(Debug, Clone, Default)]
pub struct CpuReferenz {
    /// Verschiebung seit Bildanfang (δ) und Korrektur im Unterschritt (κ).
    delta: Vec<[f64; 3]>,
    korr: Vec<[f64; 3]>,
    /// Last je Punkt (N) aus Kantenzug und Gewicht, für die Reibung.
    last: Vec<[f64; 3]>,
    /// Kontakte des letzten Bildes.
    pub statistik: Kontaktstatistik,
}

impl CpuReferenz {
    pub fn neu() -> Self {
        CpuReferenz::default()
    }

    /// Ein Unterschritt der Länge `h`; `s.x` ist die Lage am Bildanfang.
    fn unterschritt(&mut self, s: &mut Szene, h: f64) {
        let n = s.x.len();
        let g = s.param.schwerkraft;
        let daempf = (1.0 - s.param.daempfung * h).max(0.0);
        // Last je Punkt am Anfang des Unterschritts (p = x + δ): Kantenzug und
        // Gewicht.
        for i in 0..n {
            self.last[i] = mul(g, s.masse[i]);
        }
        for f in s.abstand.farben.clone() {
            for c in f {
                // Carried by a membrane: its stiffness lives in the triangle,
                // counted below ([`Szene::membran_kante`]).
                if s.membran_kante.get(c).copied().unwrap_or(false) {
                    continue;
                }
                let [i, j] = s.abstand.punkte[c].map(|p| p as usize);
                let d = add(sub(s.x[j], s.x[i]), sub(self.delta[j], self.delta[i]));
                let l = norm(d);
                let zug = (l - s.abstand.ruhe[c]).max(0.0) / s.abstand.nachgiebigkeit[c];
                if l > 1e-12 && zug > 0.0 {
                    let f_ij = mul(d, zug / l);
                    self.last[i] = add(self.last[i], f_ij);
                    self.last[j] = sub(self.last[j], f_ij);
                }
            }
        }
        // Membranes: what the fabric pulls with, tension only.
        for m in &s.membran_elemente {
            let idx = m.punkte.map(|p| p as usize);
            let p = [
                add(add(s.x[idx[0]], self.delta[idx[0]]), self.korr[idx[0]]),
                add(add(s.x[idx[1]], self.delta[idx[1]]), self.korr[idx[1]]),
                add(add(s.x[idx[2]], self.delta[idx[2]]), self.korr[idx[2]]),
            ];
            let fz = m.element.krafte_zug(p);
            for k in 0..3 {
                self.last[idx[k]] = add(self.last[idx[k]], fz[k]);
            }
        }
        // Vorhersage.
        for i in 0..n {
            self.korr[i] = [0.0; 3];
            if s.w[i] > 0.0 {
                s.v[i] = mul(add(s.v[i], mul(g, h)), daempf);
                self.delta[i] = add(self.delta[i], mul(s.v[i], h));
            }
        }
        let h2 = h * h;
        let (x, d) = (&s.x, &self.delta);
        for f in s.abstand.farben.clone() {
            for c in f {
                if s.membran_kante.get(c).copied().unwrap_or(false) {
                    continue;
                }
                abstand(x, d, &mut self.korr, &s.w, s.abstand.punkte[c], s.abstand.ruhe[c], s.abstand.nachgiebigkeit[c] / h2);
            }
        }
        // Membranes: three scalar constraints per triangle (warp strain, weft
        // strain, shear). They share no point with a distance constraint, so
        // the pass is safe next to the coloured groups; one after the other is
        // the Gauss–Seidel order.
        for m in &s.membran_elemente {
            let idx = m.punkte.map(|p| p as usize);
            let p = [
                add(add(x[idx[0]], d[idx[0]]), self.korr[idx[0]]),
                add(add(x[idx[1]], d[idx[1]]), self.korr[idx[1]]),
                add(add(x[idx[2]], d[idx[2]]), self.korr[idx[2]]),
            ];
            for zug in m.element.zuege(p) {
                if !zug.alpha.is_finite() {
                    continue;
                }
                let mut wsum = zug.alpha / h2;
                for k in 0..3 {
                    wsum += s.w[idx[k]] * dot(zug.grad[k], zug.grad[k]);
                }
                if !(wsum > 0.0) {
                    continue;
                }
                let dl = -zug.c / wsum;
                for k in 0..3 {
                    self.korr[idx[k]] = add(self.korr[idx[k]], mul(zug.grad[k], s.w[idx[k]] * dl));
                }
            }
        }
        for f in s.naht.farben.clone() {
            for c in f {
                abstand(x, d, &mut self.korr, &s.w, s.naht.punkte[c], s.naht.ruhe[c], s.naht.nachgiebigkeit[c] / h2);
            }
        }
        for f in s.biegung.farben.clone() {
            for c in f {
                biegung(x, d, &mut self.korr, &s.w, s.biegung.punkte[c], s.biegung.ruhe[c], s.biegung.nachgiebigkeit[c] / h2);
            }
        }
        for i in 0..n {
            if s.w[i] <= 0.0 {
                continue;
            }
            for k in &s.koerper {
                let p = add(add(s.x[i], self.delta[i]), self.korr[i]);
                let Some(kt) = k.kontakt(p, s.kollisionsabstand[i]) else { continue };
                self.korr[i] = add(self.korr[i], mul(kt.normale, kt.tiefe));
                // Reibung: Bewegung im Unterschritt relativ zum Körper,
                // v·h + κ − v_Körper·h (alles kleine Größen).
                let dx = sub(add(mul(s.v[i], h), self.korr[i]), mul(kt.geschwindigkeit, h));
                let t = sub(dx, mul(kt.normale, dot(dx, kt.normale)));
                let lt = norm(t);
                let normallast = (-dot(self.last[i], kt.normale)).max(0.0);
                let grenze = s.reibung[i] * kt.tiefe.max(normallast * h2 * s.w[i]);
                self.statistik.kontakte += 1;
                self.statistik.tiefe += kt.tiefe;
                self.statistik.tangential += lt;
                if lt <= grenze {
                    self.statistik.haftend += 1;
                    self.korr[i] = sub(self.korr[i], t);
                } else if lt > 0.0 {
                    self.korr[i] = sub(self.korr[i], mul(t, grenze / lt));
                }
            }
        }
        for i in 0..n {
            if s.w[i] > 0.0 {
                self.delta[i] = add(self.delta[i], self.korr[i]);
                s.v[i] = add(s.v[i], mul(self.korr[i], 1.0 / h));
            }
        }
    }
}

impl Rechenweg for CpuReferenz {
    fn bild(&mut self, s: &mut Szene) {
        let n = s.param.unterschritte.max(1);
        let h = s.param.bild_dt / n as f64;
        let np = s.x.len();
        self.statistik = Kontaktstatistik::default();
        for feld in [&mut self.delta, &mut self.korr, &mut self.last] {
            feld.clear();
            feld.resize(np, [0.0; 3]);
        }
        let koerper_anfang = s.koerper.clone();
        for k in 0..n {
            let weg = (k + 1) as f64 * h;
            for (kb, ka) in s.koerper.iter_mut().zip(&koerper_anfang) {
                *kb = ka.verschoben(weg);
            }
            self.unterschritt(s, h);
        }
        for i in 0..np {
            s.x[i] = add(s.x[i], self.delta[i]);
        }
        s.zeit += s.param.bild_dt;
    }
}

/// Abstand-Constraint C = |pᵢ − pⱼ| − ruhe mit p = x + δ + κ; korrigiert κ.
pub(crate) fn abstand(
    x: &[[f64; 3]],
    delta: &[[f64; 3]],
    dl_: &mut [[f64; 3]],
    w: &[f64],
    p: [u32; 2],
    ruhe: f64,
    alpha_h2: f64,
) {
    let (i, j) = (p[0] as usize, p[1] as usize);
    let d = add(add(sub(x[i], x[j]), sub(delta[i], delta[j])), sub(dl_[i], dl_[j]));
    let l = norm(d);
    let wsum = w[i] + w[j] + alpha_h2;
    if l < 1e-12 || wsum <= 0.0 {
        return;
    }
    let dl = -(l - ruhe) / wsum;
    let nrm = mul(d, 1.0 / l);
    dl_[i] = add(dl_[i], mul(nrm, w[i] * dl));
    dl_[j] = sub(dl_[j], mul(nrm, w[j] * dl));
}

/// Signed dihedral angle difference taken the short way round the circle.
///
/// A dihedral angle only exists modulo 2π, and `atan2` reports it in (−π, π].
/// A rest angle of ±π (a fold turned over completely) therefore sits exactly on
/// the branch cut: a fold built at +179.99° reports −179.99° after a perturbation
/// and `θ − ruhe` jumps by −2π although the fabric moved by 0.02°. Wrapping the
/// difference to (−π, π] leaves every ordinary bending constraint (rest ≈ 0)
/// untouched and makes the fold continuous.
pub(crate) fn winkel_fehler(theta: f64, ruhe: f64) -> f64 {
    let d = theta - ruhe;
    d - std::f64::consts::TAU * (d / std::f64::consts::TAU).round()
}

/// Vorzeichenbehafteter Diederwinkel an der Kante p0–p1; p2 liegt im Dreieck
/// (p0, p1, p2), p3 im Dreieck (p1, p0, p3). Flach = 0. Liefert den Winkel und
/// die Gradienten nach p0…p3 (Bridson u. a. 2003, „Simulation of Clothing
/// with Folds and Wrinkles“).
pub(crate) fn dieder(q: [[f64; 3]; 4]) -> Option<(f64, [[f64; 3]; 4])> {
    let [x0, x1, x2, x3] = q;
    let e = sub(x1, x0);
    let le = norm(e);
    let n1 = cross(sub(x2, x0), sub(x2, x1));
    let n2 = cross(sub(x3, x1), sub(x3, x0));
    let (q1, q2) = (dot(n1, n1), dot(n2, n2));
    if le < 1e-12 || q1 < 1e-30 || q2 < 1e-30 {
        return None;
    }
    let (m1, m2) = (mul(n1, 1.0 / q1.sqrt()), mul(n2, 1.0 / q2.sqrt()));
    let theta = dot(cross(m1, m2), mul(e, 1.0 / le)).atan2(dot(m1, m2));
    // Bridsons Biegemoden u₁…u₄ sind −∇θ für diese Vorzeichenwahl von θ
    // (Test `dieder_gradient_wie_differenzen`).
    let g2 = mul(n1, -le / q1);
    let g3 = mul(n2, -le / q2);
    let g0 = mul(add(mul(n1, dot(sub(x2, x1), e) / le / q1), mul(n2, dot(sub(x3, x1), e) / le / q2)), -1.0);
    let g1 = add(mul(n1, dot(sub(x2, x0), e) / le / q1), mul(n2, dot(sub(x3, x0), e) / le / q2));
    Some((theta, [g0, g1, g2, g3]))
}

/// Biege-Constraint C = θ − ruhe mit p = x + δ + κ; korrigiert κ.
pub(crate) fn biegung(
    x: &[[f64; 3]],
    delta: &[[f64; 3]],
    dl_: &mut [[f64; 3]],
    w: &[f64],
    p: [u32; 4],
    ruhe: f64,
    alpha_h2: f64,
) {
    let idx = p.map(|i| i as usize);
    // Winkel aus Lagen relativ zum ersten Punkt (in f32 fein aufgelöst).
    let p0 = idx[0];
    let rel = |i: usize| add(add(sub(x[i], x[p0]), sub(delta[i], delta[p0])), sub(dl_[i], dl_[p0]));
    let Some((theta, g)) = dieder(idx.map(rel)) else { return };
    let mut nenner = alpha_h2;
    for k in 0..4 {
        nenner += w[idx[k]] * dot(g[k], g[k]);
    }
    if nenner <= 0.0 {
        return;
    }
    let dl = -winkel_fehler(theta, ruhe) / nenner;
    for k in 0..4 {
        dl_[idx[k]] = add(dl_[idx[k]], mul(g[k], w[idx[k]] * dl));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Gradienten des Diederwinkels stimmen mit Differenzenquotienten.
    #[test]
    fn dieder_gradient_wie_differenzen() {
        let q = [[0.0, 0.0, 0.0], [1.0, 0.1, 0.0], [0.4, 0.9, 0.3], [0.6, -0.8, 0.5]];
        let (t, g) = dieder(q).unwrap();
        let eps = 1e-7;
        for k in 0..4 {
            for a in 0..3 {
                let mut qp = q;
                qp[k][a] += eps;
                let mut qm = q;
                qm[k][a] -= eps;
                let d = (dieder(qp).unwrap().0 - dieder(qm).unwrap().0) / (2.0 * eps);
                assert!((d - g[k][a]).abs() < 1e-5, "Punkt {k}, Achse {a}: {d} gegen {}", g[k][a]);
            }
        }
        // flach = 0
        let f = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 1.0, 0.0], [0.5, -1.0, 0.0]];
        assert!(dieder(f).unwrap().0.abs() < 1e-12);
        assert!(t.abs() > 0.1);
    }

    /// Ein Abstand-Constraint mit α = 0 stellt die Ruhelänge in einem Schritt her.
    #[test]
    fn abstand_starr() {
        let x = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let mut d = [[0.0; 3]; 2];
        abstand(&x, &[[0.0; 3]; 2], &mut d, &[1.0, 1.0], [0, 1], 1.0, 0.0);
        let p = [add(x[0], d[0]), add(x[1], d[1])];
        assert!((norm(sub(p[1], p[0])) - 1.0).abs() < 1e-12);
        assert!((p[0][0] - 0.5).abs() < 1e-12);
    }

    /// Biegeenergie: die Röhre genau auf R gerollt (ohne Streckung, Naht
    /// geschlossen) trägt Σ ½·k·θ² = D/2·κ²·Fläche (Vorfaktor 1 in
    /// `szene.rs`; mit ¾ waren es 75 %).
    #[test]
    fn biegeenergie_auf_dem_zylinder() {
        use crate::roehre::{param_ruhe, Roehre};
        let r = Roehre { anfangsabstand_m: 0.0, reduktion: 0.0, ..Roehre::default() };
        let s = r.szene_roh(param_ruhe()).unwrap();
        let mut e = 0.0;
        for c in 0..s.biegung.len() {
            let q = s.biegung.punkte[c].map(|i| s.x[i as usize]);
            let (theta, _) = dieder(q).unwrap();
            e += 0.5 * theta * theta / s.biegung.nachgiebigkeit[c];
        }
        let kappa = 1.0 / r.mittelradius_m();
        let soll = 0.5 * r.material.biegung_nm * kappa * kappa * r.breite_m() * r.hoehe_m;
        assert!((e / soll - 1.0).abs() < 0.06, "Biegeenergie {e:.4e} J gegen {soll:.4e} J ({:.3})", e / soll);
        assert_eq!(s.naht_biegung_ohne, 0);
    }

    /// Warum Lage, Verschiebung und Korrektur getrennt: in f32 geht die
    /// Schwerkraft eines Unterschritts auf Körperhöhe verloren, wenn die
    /// absolute Lage fortgeschrieben und v aus ihrer Differenz gebildet wird —
    /// und ebenso, wenn v aus der Differenz der Verschiebung seit Bildanfang
    /// entsteht. Mit v ← v + κ/h bleibt sie erhalten (freier Fall 0,5 s,
    /// 2.986 Unterschritte je Bild wie die Bluse aus Radical).
    #[test]
    fn f32_schwerkraft_nur_mit_getrennter_korrektur() {
        let (n, bilder, g) = (2986usize, 30usize, 9.81f32);
        let h = (1.0f32 / 60.0) / n as f32;
        let soll = 0.5 * 9.81 * 0.5f64 * 0.5;
        let fall = |art: u8| -> f64 {
            let (mut z, mut v) = (1.3f32, 0.0f32);
            for _ in 0..bilder {
                let mut d = 0.0f32;
                for _ in 0..n {
                    v -= g * h;
                    match art {
                        // absolut: x += v·h, v = (x − x_alt)/h
                        0 => {
                            let alt = z;
                            z += v * h;
                            v = (z - alt) / h;
                        }
                        // Verschiebung, v aus ihrer Differenz
                        1 => {
                            let alt = d;
                            d += v * h;
                            v = (d - alt) / h;
                        }
                        // Verschiebung, v ← v + κ/h (κ = 0 im freien Fall)
                        _ => d += v * h,
                    }
                }
                z += d;
            }
            (1.3 - z) as f64
        };
        assert!(fall(0) < 0.01 * soll, "absolut: {} m", fall(0));
        assert!((fall(1) / soll - 1.0).abs() > 0.1, "δ-Differenz: {} m", fall(1));
        let weg = fall(2);
        assert!((weg / soll - 1.0).abs() < 0.005, "getrennt: {weg} m gegen {soll} m");
    }
}
