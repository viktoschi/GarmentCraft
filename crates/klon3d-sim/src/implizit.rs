//! Impliziter Schritt mit konjugierten Gradienten — das Verfahren von CLO, zum
//! Vergleich mit der XPBD-Referenz (Befund 35).
//!
//! CLO löst je Zeitschritt ein dünn besetztes lineares System mit Kraft und
//! Jacobi-Matrix je Constraint und konjugierten Gradienten (Befund 09:
//! `Compute…ForceAndJacobian`, `ComputeDeltaX`; Befund 43: Block-CG mit
//! symmetrischen 3×3-Blöcken). Voreinstellung (Befund 49): Zeitschritt 1/30 s,
//! 50 CG-Iterationen, Nahtfedern `fSeamSpringK` 500 000 (= 500 N/m in der
//! Umrechnung von `MATERIAL.md`). Gebaut ist hier das Lehrbuchverfahren dazu,
//! nicht CLOs Code: **Rückwärts-Euler mit einem Newton-Schritt** (Baraff &
//! Witkin, „Large Steps in Cloth Simulation“, 1998):
//!
//! ```text
//! (M·(1 + h·c) + h²·H) · Δv = h · (f(x, v) − h·H·v),     x ← x + h·(v + Δv)
//! ```
//!
//! mit H = −∂f/∂x (positiv semidefinit gemacht) und Luftdämpfung c.
//!
//! * **Dehnung:** dieselben Kantenfedern wie die Referenz, k = 1/α. Für H gilt
//!   je Kante k·[n·nᵀ + max(0, 1 − L₀/l)·(I − n·nᵀ)] — die Querrichtung nur
//!   unter Zug, sonst wäre H nicht positiv (Choi & Ko 2002).
//! * **Naht:** Feder mit `naht_k` (Vorgabe CLO: 500 N/m) und der Ruhelänge der
//!   Szene, sonst 0 (dann H = k·I); H wie bei den Kanten. Mit einer Ruhelänge,
//!   die je Bild kürzer wird ([`crate::Szene::naht_kuerzen`]), schließt sich
//!   eine Naht mit begrenzter Geschwindigkeit statt in einem Bild (Befund 96).
//! * **Biegung:** Kraft −k·θ·∇θ aus dem Diederwinkel, in H die Gauß-Newton-
//!   Näherung k·∇θ·∇θᵀ (positiv).
//! * **Kontakt:** Punkte an oder in einem Körper werden im CG in Normalrichtung
//!   festgelegt (Filter, „modified PCG“ nach Baraff & Witkin): die neue
//!   Normalgeschwindigkeit bringt sie genau auf die Oberfläche. Die
//!   **Normalkraft fällt aus dem System heraus** (Rest A·Δv − b in
//!   Normalrichtung); zieht sie am Körper, wird der Punkt freigegeben und noch
//!   einmal gelöst.
//! * **Durchstoß:** Nach dem Schritt wird der Weg jedes Punkts gegen ein
//!   Körpernetz geprüft; hat er die Haut von außen durchstoßen, setzt er am
//!   Eintritt auf ([`crate::netzkoerper`]). Ohne das ginge ein Punkt, der
//!   schneller als Suchtiefe/h läuft (10 mm bei 1/60 s: 0,6 m/s), verloren.
//! * **Körperpunkt gegen Stoffdreieck** (wahlweise, Vorgabe aus): danach
//!   heben bis zu `koerperpunkt_durchgaenge` Durchgänge jedes Stoffdreieck,
//!   durch das ein Körperpunkt sticht
//!   ([`crate::Koerpernetz::punkte_gegen_dreiecke`], Befund 96). Beim Anziehen
//!   läuft das während des Nähens ([`crate::platzierung::anziehen`]); in Ruhe
//!   zieht der Schritt gespannte Dreiecke jedes Bild wieder ein wenig durch,
//!   und die Korrektur hielte die Szene in Bewegung.
//! * **Reibung:** Coulomb auf die Geschwindigkeit relativ zum Körper: steht
//!   der Punkt, hält ihn μ·F_N·h/m, sonst bremst sie um so viel.
//! * **CG:** matrixfrei, Vorkonditionierer mit den 3×3-Diagonalblöcken von A,
//!   Abbruch bei ‖r‖² ≤ tol²·‖b‖² oder nach `cg_max` Iterationen.
//!
//! Ein Schritt verschiebt um g·h² ≈ 10⁻² m — weit über der f32-Auflösung; die
//! Verschiebungsfelder der Referenz braucht dieser Weg nicht.

use crate::cpu::{dieder, Rechenweg};
use crate::koerper::{add, dot, mul, norm, sub};
use crate::selbstkollision::{self, Feder, Stoffnetz, Stoffwerte};
use crate::szene::Szene;

/// Die Paare (Punkt der ersten Seite, Punkt der zweiten Seite, Gewicht
/// |gₖ|·|gₗ|), über die eine Feder in die Systemmatrix geht — bei CLO je
/// Punkt–Dreieck 3, je Kante–Kante 4, je Durchdringung 6 Nebendiagonal-Blöcke,
/// die `UpdateCOOValue` in Laplace-Form bringt (−B daneben, +B auf beide
/// Diagonalen; Befund 116).
fn feder_paare(fe: &Feder) -> impl Iterator<Item = (usize, usize, f64)> + '_ {
    (0..5).flat_map(move |q| {
        (0..5).filter_map(move |r| {
            let (gq, gr) = (fe.gewichte[q], fe.gewichte[r]);
            (gq > 0.0 && gr < 0.0).then(|| (fe.punkte[q] as usize, fe.punkte[r] as usize, -gq * gr))
        })
    })
}

/// 3×3-Matrix, zeilenweise.
pub type M3 = [[f64; 3]; 3];

/// Eine Abstands-Constraint, wie das CG sie sieht: Punktpaar, Steifigkeit,
/// **aktuelle** Richtung und Querfaktor `max(0, 1 − L₀/l)`. Kanten und Nähte
/// haben dieselbe Form; der Unterschied ist nur, woher `k` kommt.
#[derive(Debug, Clone, Copy)]
pub struct Kante {
    pub i: u32,
    pub j: u32,
    pub k: f64,
    pub richtung: [f64; 3],
    pub quer: f64,
    /// Rest length: `ruhe = l₀` for a pattern edge, and for a seam the rest
    /// length it was sewn to (infinite while open, then the current length is
    /// used). The GPU build needs it, so it travels with the constraint.
    pub ruhe: f64,
}

/// Eine Biege-Constraint: die vier Punkte, der Gradient des Diederwinkels und
/// die Steifigkeit (Gauß-Newton-Glied `k·∇θ·∇θᵀ`).
#[derive(Debug, Clone, Copy)]
pub struct Biegung {
    pub punkte: [u32; 4],
    pub grad: [[f64; 3]; 4],
    pub k: f64,
    /// Rest angle of the dihedral constraint.
    pub ruhe: f64,
}

/// Das lineare System **eines Unterschritts**, in Feldern je Punkt bzw. je
/// Constraint — die Form, die auf der GPU ohne Zusammenbau und ohne Atomics
/// gelesen wird.
///
/// Kräfte, Kontakte, Reibung und die Körperabfragen bleiben beim Aufrufer; hier
/// steht nur, was das CG braucht. `kanten`, `naehte` und `biegungen` liegen in
/// der Reihenfolge der Szene (nach Farbe geordnet), damit die Reihenfolge der
/// Summation und damit das Ergebnis der CPU-Fassung erhalten bleibt.
///
/// Federn der Selbstkollision und der Klebezonen (Punkt–Dreieck,
/// Kante–Kante) stehen als `federn` daneben: sie sind keine Kanten, ihre
/// 3×3-Blöcke hängen an fünf Punkten. Die CPU rechnet sie mit; ein Löser, der
/// sie nicht kann, lehnt den Schritt ab, statt sie stillschweigend
/// wegzulassen ([`Loeser::loese`]).
#[derive(Debug)]
pub struct System<'a> {
    pub h: f64,
    /// Luftdämpfung (1/s).
    pub c: f64,
    pub cg_max: usize,
    pub cg_toleranz: f64,
    pub masse: &'a [f64],
    pub w: &'a [f64],
    pub v: &'a [[f64; 3]],
    /// Positions at build time. Only a backend that builds the geometry itself
    /// needs them ([`System::ohne_geometrie`]).
    pub x: &'a [[f64; 3]],
    /// Gravity (m/s²), for the same reason: the force per point.
    pub g: [f64; 3],
    pub kanten: Vec<Kante>,
    pub naehte: Vec<Kante>,
    pub biegungen: Vec<Biegung>,
    pub federn: Vec<Feder>,
    /// Rechte Seite `b = h·(f − h·H·v)`.
    pub rhs: Vec<[f64; 3]>,
    /// Force per point `f` (gravity, damping, constraints, springs). Empty
    /// when the geometry is built on the device.
    pub f: Vec<[f64; 3]>,
    /// Physical load per point (tension only, plus weight), for friction.
    pub last: Vec<[f64; 3]>,
    /// Inverse der 3×3-Diagonalblöcke von `A` (Block-Jacobi).
    pub pinv: Vec<M3>,
    /// `true`: this system's geometry is built on the device. Only the topology,
    /// the stiffnesses and the rest lengths are filled here; `richtung`, `quer`,
    /// `grad`, `rhs` and `pinv` stay empty and the backend computes them from
    /// the positions.
    ///
    /// Set by the backend, never by a caller - a solver that cannot build on
    /// its own must not see a half-built system.
    pub ohne_geometrie: bool,
}

impl System<'_> {
    /// `H·p` — Steifigkeit ohne Masse und ohne Faktor `h²`.
    ///
    /// Je Kante und Naht `k·[n·nᵀ + quer·(I − n·nᵀ)]·p`, je Biegung
    /// `k·∇θ·(Σ ∇θ·p)`, je Feder die Blöcke der Selbstkollision.
    pub fn mal_h(&self, p: &[[f64; 3]], out: &mut [[f64; 3]]) {
        for o in out.iter_mut() {
            *o = [0.0; 3];
        }
        for k in self.kanten.iter().chain(&self.naehte) {
            let (i, j) = (k.i as usize, k.j as usize);
            let d = sub(p[i], p[j]);
            let bd = mul(add(mul(k.richtung, dot(k.richtung, d) * (1.0 - k.quer)), mul(d, k.quer)), k.k);
            out[i] = add(out[i], bd);
            out[j] = sub(out[j], bd);
        }
        for b in &self.biegungen {
            let idx = b.punkte.map(|q| q as usize);
            let gp: f64 = (0..4).map(|m| dot(b.grad[m], p[idx[m]])).sum();
            for m in 0..4 {
                out[idx[m]] = add(out[idx[m]], mul(b.grad[m], b.k * gp));
            }
        }
        for fe in &self.federn {
            for (q, r, om) in feder_paare(fe) {
                let t = mul(m3_mal(&fe.block, sub(p[q], p[r])), om);
                out[q] = add(out[q], t);
                out[r] = sub(out[r], t);
            }
        }
    }

    /// `A·p = M(1 + h·c)·p + h²·H·p`.
    pub fn mal_a(&self, p: &[[f64; 3]], out: &mut [[f64; 3]]) {
        self.mal_h(p, out);
        for i in 0..p.len() {
            out[i] = add(mul(out[i], self.h * self.h), mul(p[i], self.masse[i] * (1.0 + self.h * self.c)));
        }
    }

    /// Die 3×3-Diagonalblöcke von `A` (Vorkonditionierer), noch nicht invertiert.
    pub fn diag(&self) -> Vec<M3> {
        let h2 = self.h * self.h;
        let mut diag: Vec<M3> = (0..self.masse.len())
            .map(|i| {
                let mut m = [[0.0; 3]; 3];
                m3_add_skal(&mut m, self.masse[i] * (1.0 + self.h * self.c));
                m
            })
            .collect();
        for k in self.kanten.iter().chain(&self.naehte) {
            for q in [k.i as usize, k.j as usize] {
                m3_add_skal(&mut diag[q], k.k * h2 * k.quer);
                m3_add_aus(&mut diag[q], k.richtung, k.richtung, k.k * h2 * (1.0 - k.quer));
            }
        }
        for b in &self.biegungen {
            for m in 0..4 {
                let q = b.punkte[m] as usize;
                m3_add_aus(&mut diag[q], b.grad[m], b.grad[m], b.k * h2);
            }
        }
        for fe in &self.federn {
            for (q, r, om) in feder_paare(fe) {
                for p in [q, r] {
                    for (a, z) in diag[p].iter_mut().enumerate() {
                        for (b, d) in z.iter_mut().enumerate() {
                            *d += h2 * om * fe.block[a][b];
                        }
                    }
                }
            }
        }
        diag
    }
}

/// Nimmt an einem festgelegten Punkt die Normalkomponente heraus.
pub(crate) fn filtern(fest: &Option<Fest>, v: [f64; 3]) -> [f64; 3] {
    match fest {
        Some(fe) => sub(v, mul(fe.normale, dot(v, fe.normale))),
        None => v,
    }
}

/// Was ein Löser zurückschreibt: wie viele CG-Iterationen er gebraucht hat und
/// wie weit er gekommen ist. Ein Löser, der sein Budget durchrechnet (die GPU),
/// meldet seine Höchstzahl.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CgLauf {
    pub iterationen: usize,
    pub nicht_konvergiert: bool,
    /// `sqrt(‖r‖²/‖b‖²)` am Ende.
    pub rest: f64,
}

/// Ein Löser für `A·Δv = b`. Die CPU-Fassung ist die Referenz; die GPU-Fassung
/// (`f32`, `wgpu`) muss dasselbe Ergebnis liefern, und der Vergleich ist eine
/// Zahl, keine Behauptung ([`crate::gpu`]).
pub trait Loeser: std::fmt::Debug {
    /// Name für den Bericht, z. B. `"CPU (f64)"`.
    fn name(&self) -> &'static str;
    /// Löst das System und schreibt `Δv` (Startwert: Warmstart des Aufrufers).
    fn loese(&mut self, sys: &System<'_>, fest: &[Option<Fest>], dv: &mut [[f64; 3]]) -> CgLauf;
    /// `A·p` ohne Filter, für den Freigabe-Durchgang (Normalkraft aus dem Rest).
    fn mal_a(&mut self, sys: &System<'_>, p: &[[f64; 3]], out: &mut [[f64; 3]]);

    /// `A·Δv` für den Freigabe-Durchgang, wenn `Δv` genau das ist, was der
    /// letzte [`Loeser::loese`] zurückgegeben hat, und `sys` dasselbe System.
    ///
    /// Die Vorgabe rechnet wie [`Loeser::mal_a`]. Ein Backend, das `Δv` noch
    /// auf dem Gerät hat, kann sich den Transfer sparen: der Freigabe-Durchgang
    /// braucht nur die ungefilterte Anwendung, und die hängt nicht davon ab,
    /// welche Punkte gehalten werden - nur die *zweite* Lösung tut das.
    fn mal_a_geloest(&mut self, sys: &System<'_>, dv: &[[f64; 3]], out: &mut [[f64; 3]]) {
        self.mal_a(sys, dv, out);
    }

    /// `true`: this backend builds the constraint geometry, the forces, the
    /// right-hand side and the preconditioner from the positions itself. The
    /// caller then only hands over the topology, the stiffnesses and the rest
    /// lengths ([`System::ohne_geometrie`]) and saves that work.
    fn baut_selbst(&self) -> bool {
        false
    }

    /// The residual `A·Δv − b` for the release pass, under the same condition
    /// as [`Loeser::mal_a_geloest`]. The release pass reads a held point's
    /// normal force from it.
    ///
    /// The default takes `b` from the system. A backend that builds `b` itself
    /// ([`Loeser::baut_selbst`]) has it on the device and must override this.
    fn rest_geloest(&mut self, sys: &System<'_>, dv: &[[f64; 3]], out: &mut [[f64; 3]]) {
        self.mal_a_geloest(sys, dv, out);
        for (o, b) in out.iter_mut().zip(&sys.rhs) {
            *o = sub(*o, *b);
        }
    }

    /// The physical load per point (tension only, plus weight) of the system
    /// this backend built last, for friction with
    /// [`ImplizitParameter::normalkraft_aus_last`]. Only a backend that builds
    /// the system itself has it; the default is empty.
    fn last_lesen(&mut self) -> Vec<[f64; 3]> {
        Vec::new()
    }
}

/// Der CPU-Löser: matrixfreies PCG in `f64`, genau das Verfahren, das die
/// Referenz bisher im Schritt selbst hatte.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuLoeser;

impl Loeser for CpuLoeser {
    fn name(&self) -> &'static str {
        "CPU (f64)"
    }

    fn mal_a(&mut self, sys: &System<'_>, p: &[[f64; 3]], out: &mut [[f64; 3]]) {
        sys.mal_a(p, out);
    }

    /// Modifiziertes PCG (Baraff & Witkin 1998): festgelegte Punkte bekommen
    /// in Normalrichtung die verlangte Geschwindigkeitsänderung, der Filter
    /// hält sie dort.
    fn loese(&mut self, sys: &System<'_>, fest: &[Option<Fest>], dv: &mut [[f64; 3]]) -> CgLauf {
        let (n, w) = (sys.masse.len(), sys.w);
        let b = &sys.rhs;
        let pinv = &sys.pinv;
        // Start: frei wie zuvor, festgelegt auf den Sollwert in Normalrichtung.
        for i in 0..n {
            if w[i] <= 0.0 {
                dv[i] = [0.0; 3];
                continue;
            }
            if let Some(fe) = fest[i] {
                let soll = fe.vn_soll - dot(sys.v[i], fe.normale);
                dv[i] = add(filtern(&fest[i], dv[i]), mul(fe.normale, soll));
            }
        }
        let mut ap = vec![[0.0; 3]; n];
        sys.mal_a(dv, &mut ap);
        let mut r: Vec<[f64; 3]> = (0..n).map(|i| if w[i] > 0.0 { filtern(&fest[i], sub(b[i], ap[i])) } else { [0.0; 3] }).collect();
        let bb: f64 = (0..n).map(|i| if w[i] > 0.0 { let fb = filtern(&fest[i], b[i]); dot(fb, m3_mal(&pinv[i], fb)) } else { 0.0 }).sum();
        let mut z: Vec<[f64; 3]> = (0..n).map(|i| m3_mal(&pinv[i], r[i])).collect();
        let mut cdir: Vec<[f64; 3]> = (0..n).map(|i| filtern(&fest[i], z[i])).collect();
        let mut delta: f64 = (0..n).map(|i| dot(r[i], cdir[i])).sum();
        // Ziel relativ zu ‖b‖ und zum Anfangsrest, aber nie unter einer
        // Geschwindigkeitsgenauigkeit von 10⁻⁹ m/s (r·P⁻¹·r ≈ Σ m·Δv²): in
        // Ruhe ist b ≈ 0, und ein rein relatives Ziel ließe das CG im
        // Rundungsrauschen weiterlaufen, bis ein winziger Nenner die
        // Schrittweite sprengt (am 20.10.2026 gesehen: Rest 10¹⁰⁴).
        let boden: f64 = sys.masse.iter().sum::<f64>() * 1e-18;
        let ziel = (sys.cg_toleranz * sys.cg_toleranz * bb.max(delta)).max(boden);
        let mut it = 0;
        while delta > ziel && it < sys.cg_max {
            sys.mal_a(&cdir, &mut ap);
            for i in 0..n {
                ap[i] = if w[i] > 0.0 { filtern(&fest[i], ap[i]) } else { [0.0; 3] };
            }
            let ca: f64 = (0..n).map(|i| dot(cdir[i], ap[i])).sum();
            if ca <= f64::MIN_POSITIVE || !ca.is_finite() {
                break;
            }
            let alpha = delta / ca;
            for i in 0..n {
                dv[i] = add(dv[i], mul(cdir[i], alpha));
                r[i] = sub(r[i], mul(ap[i], alpha));
            }
            for i in 0..n {
                z[i] = m3_mal(&pinv[i], r[i]);
            }
            let neu: f64 = (0..n).map(|i| dot(r[i], z[i])).sum();
            let beta = neu / delta;
            delta = neu;
            for i in 0..n {
                cdir[i] = if w[i] > 0.0 { filtern(&fest[i], add(z[i], mul(cdir[i], beta))) } else { [0.0; 3] };
            }
            it += 1;
        }
        CgLauf { iterationen: it, nicht_konvergiert: delta > ziel, rest: (delta / bb.max(boden)).sqrt() }
    }
}

fn m3_add_skal(m: &mut M3, s: f64) {
    for (k, z) in m.iter_mut().enumerate() {
        z[k] += s;
    }
}
fn m3_add_aus(m: &mut M3, a: [f64; 3], b: [f64; 3], s: f64) {
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] += s * a[i] * b[j];
        }
    }
}
fn m3_mal(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
fn m3_inv(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    let s = 1.0 / det;
    [
        [(e * i - f * h) * s, (c * h - b * i) * s, (b * f - c * e) * s],
        [(f * g - d * i) * s, (a * i - c * g) * s, (c * d - a * f) * s],
        [(d * h - e * g) * s, (b * g - a * h) * s, (a * e - b * d) * s],
    ]
}

/// Einstellungen des impliziten Wegs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImplizitParameter {
    /// Substeps per frame (CLO: frame 1/30 s; preset `Nonlinear` five
    /// substeps, `Normal` one).
    pub schritte_je_bild: usize,
    /// Maximum number of CG iterations per solve (CLO: 50).
    pub cg_max: usize,
    /// Stop at ‖r‖ ≤ tol·‖b‖ (CLO "residual"; `Nonlinear` 1e-5, `Normal`
    /// 1e-4).
    pub cg_toleranz: f64,
    /// Nahtfeder (N/m), CLO `fSeamSpringK` 500 000 g/s².
    pub naht_k: f64,
    /// Kontakt schon ab diesem Abstand zur Oberfläche (m).
    pub kontakt_rand: f64,
    /// Normalkraft der Reibung aus der physikalischen Last (Kantenzug +
    /// Gewicht gegen die Normale, wie die XPBD-Referenz) statt aus dem Rest
    /// des Gleichungssystems — robust, wenn das CG nicht konvergiert.
    pub normalkraft_aus_last: bool,
    /// Den Weg je Schritt gegen Körpernetze prüfen und am Eintritt aufsetzen
    /// ([`crate::Koerper::durchstoss`]). Nur zum Vergleich abschaltbar.
    pub durchstoss_pruefen: bool,
    /// Durchgänge Körperpunkt gegen Stoffdreieck je Schritt
    /// ([`crate::Koerpernetz::punkte_gegen_dreiecke`]); 0 schaltet ab.
    pub koerperpunkt_durchgaenge: usize,
    /// So tief (m) unter einem Körperpunkt wird ein Stoffdreieck noch gehoben.
    pub koerperpunkt_tiefe: f64,
    /// Selbstkollision Stoff gegen Stoff wie CLO ([`crate::selbstkollision`],
    /// Befund 116): Nähe-Federn im System, Stoßprüfung danach.
    pub selbstkollision: bool,
    /// Zusatzdicke „Kollision“ je Teil (m), CLO-Vorgabe 2,5 mm: Radius je
    /// Punkt = (Stoffdicke + Zusatzdicke)/2.
    pub selbst_zusatzdicke: f64,
    /// Steifigkeitsfaktor der Nähe (CLO `AvoidanceStiffness`, 0,001).
    pub selbst_steifigkeit: f64,
    /// Durchgänge der Stoßantwort (CLO `SelfCollisionIterationCount`, 1).
    pub selbst_stoss_iterationen: usize,
    /// Durchdringungen auch zwischen Nahtpartnern suchen wie CLO (Vorgabe aus,
    /// [`selbstkollision::finde_schnitte`]).
    pub selbst_schnitte_an_naehten: bool,
    /// Faktor auf die Steifigkeit jeder Klebezone (Vorgabe 1). Die Steifigkeit
    /// selbst steht in der Zone und kommt aus der **Dehnsteifigkeit** der oberen
    /// Lage, nicht aus ihrer Biegesteifigkeit ([`crate::klebung::Stoffpaar`]);
    /// der Faktor ist zum Prüfen da. Die Biegung des geklebten Paares steht
    /// getrennt in der Zone (`Zone::biegefaktor`) und wirkt auf die
    /// Biege-Constraints, nicht auf die Federn. CLO rechnet die Klebe als Feder
    /// mit Jacobian, nicht als feste Kopplung (Befund 59).
    pub klebe_faktor: f64,
}

impl Default for ImplizitParameter {
    fn default() -> Self {
        ImplizitParameter {
            // CLO preset `Nonlinear.smp`: five substeps, residual 1e-5.
            // With one substep a 5.5 % Radical band on the reference body does not
            // settle and sits 2…4 % off the tape-measure circumference; the
            // error falls monotonically with the number of substeps
            // (0.06 % at five).
            schritte_je_bild: 5,
            cg_max: 50,
            cg_toleranz: 1e-5,
            naht_k: 500.0,
            kontakt_rand: 1e-4,
            normalkraft_aus_last: false,
            durchstoss_pruefen: true,
            koerperpunkt_durchgaenge: 0,
            koerperpunkt_tiefe: 0.01,
            selbstkollision: false,
            selbst_zusatzdicke: 2.5e-3,
            selbst_steifigkeit: 1e-3,
            selbst_stoss_iterationen: 1,
            selbst_schnitte_an_naehten: false,
            // 1 = die Steifigkeit der Zone selbst (aus ihrem Stoffpaar).
            klebe_faktor: 1.0,
        }
    }
}

/// Zahlen des letzten Bildes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImplizitStatistik {
    /// CG-Iterationen insgesamt und Lösungen (mit Freigabe-Durchgängen).
    pub cg_iterationen: usize,
    pub loesungen: usize,
    /// Lösungen, die `cg_max` erreichten, ohne die Toleranz zu treffen.
    pub nicht_konvergiert: usize,
    /// Kontakte (Punkt × Schritt), davon haftend.
    pub kontakte: usize,
    pub haftend: usize,
    /// Punkte, die wegen ziehender Normalkraft freigegeben wurden.
    pub freigegeben: usize,
    /// Wege, die die Haut eines Körpernetzes durchstießen (am Eintritt
    /// aufgesetzt).
    pub durchstoesse: usize,
    /// Stoffdreiecke, durch die ein Körperpunkt stach (gehoben).
    pub koerperpunkte: usize,
    /// Größter relativer Rest ‖r‖/‖b‖ einer Lösung.
    pub rest_max: f64,
    /// Nähe-Paare Punkt–Dreieck und Kante–Kante (Summe über die Schritte).
    pub naehe_punkt_dreieck: usize,
    pub naehe_kante_kante: usize,
    /// Stöße Stoff gegen Stoff, die die Stoßantwort zurücksetzte.
    pub selbst_stoesse: usize,
    /// Schnitte Kante durch Dreieck und ihre Gebiete (Summe über die Schritte).
    pub durchdringungen: usize,
    pub durchdringungsgebiete: usize,
}

/// Der implizite Rechenweg.
///
/// Der Schritt (Kräfte, Kontakte, Reibung, Körperabfragen) steht hier einmal;
/// nur das Lösen von `A·Δv = b` geht an einen [`Loeser`]. Vorgabe ist
/// [`CpuLoeser`] (`f64`); [`Self::mit_loeser`] nimmt die GPU-Fassung
/// ([`crate::gpu`]).
#[derive(Debug)]
pub struct ImplizitCg {
    pub param: ImplizitParameter,
    pub statistik: ImplizitStatistik,
    loeser: Box<dyn Loeser>,
    /// Δv des vorigen Schritts — Startwert des CG (Warmstart). Ohne ihn findet
    /// ein CG mit lokalem Vorkonditionierer die langsame Gesamtbewegung (die
    /// ganze Röhre fällt) erst nach Hunderten Iterationen: mit 50 fiel sie
    /// 170 mm statt 1.255 mm (20.10.2026).
    dv_vorher: Vec<[f64; 3]>,
    /// Stoffnetz und Stoffwerte der Selbstkollision (beim ersten Schritt gebaut).
    selbst: Option<(Stoffnetz, Stoffwerte)>,
    /// Nähe- und Schubfedern des laufenden Schritts.
    federn: Vec<Feder>,
    /// Schnitte Kante durch Dreieck des laufenden Schritts.
    schnitte: Vec<selbstkollision::Schnitt>,
    /// je Dreieck die Schritte in Folge mit Durchdringung
    folge: Vec<u32>,
}

/// Contact of one point in the current substep: the normal the point is held
/// along, the normal velocity it must have afterwards, and the body's velocity
/// there. The filter of the CG is built from the first two; friction uses all
/// three.
#[derive(Debug, Clone, Copy)]
pub struct Fest {
    pub normale: [f64; 3],
    /// Normal velocity the point must have after the step (m/s).
    pub vn_soll: f64,
    pub v_koerper: [f64; 3],
}

impl Default for ImplizitCg {
    fn default() -> Self {
        ImplizitCg::neu(ImplizitParameter::default())
    }
}

impl ImplizitCg {
    pub fn neu(param: ImplizitParameter) -> Self {
        ImplizitCg::mit_loeser(param, Box::new(CpuLoeser))
    }

    /// Derselbe Schritt mit einem anderen Löser für `A·Δv = b`. Der Löser
    /// bekommt je Unterschritt das System und die Kontakte; alles andere
    /// (Kräfte, Freigabe-Durchgang, Reibung, Lage) bleibt hier.
    pub fn mit_loeser(param: ImplizitParameter, loeser: Box<dyn Loeser>) -> Self {
        ImplizitCg {
            param,
            statistik: ImplizitStatistik::default(),
            loeser,
            dv_vorher: Vec::new(),
            selbst: None,
            federn: Vec::new(),
            schnitte: Vec::new(),
            folge: Vec::new(),
        }
    }

    /// Name des Lösers, für den Bericht.
    pub fn loeser_name(&self) -> &'static str {
        self.loeser.name()
    }

    /// `A·p` of the system the next substep would solve, computed by this
    /// backend at the scene's current state. It builds the same system as a
    /// step and discards everything else - the probe with which step 4b
    /// compares CPU and GPU operator.
    pub fn operator(&mut self, s: &mut Szene, p: &[[f64; 3]]) -> Vec<[f64; 3]> {
        let h = s.param.bild_dt / self.param.schritte_je_bild.max(1) as f64;
        let (sys, _) = self.system_bauen(s, h);
        let mut out = vec![[0.0; 3]; p.len()];
        self.loeser.mal_a(&sys, p, &mut out);
        out
    }

    /// The system the next substep would solve, from the scene at its current
    /// state. For measuring a backend on a fixed system instead of on a whole
    /// frame (`crates/klon3d-sim/examples/gpu_takt.rs`).
    ///
    /// Building it has side effects on this step (the springs of the substep
    /// are taken into the system), so it is not a pure getter.
    pub fn system_fuer<'a>(&mut self, s: &'a mut Szene) -> System<'a> {
        let h = s.param.bild_dt / self.param.schritte_je_bild.max(1) as f64;
        self.system_bauen(s, h).0
    }

    /// The linear system of one substep: forces, geometry terms, right-hand
    /// side, preconditioner and the contact set - everything a solver needs,
    /// from the scene at its current state.
    ///
    /// Kept apart from the rest of the step so the operator can be applied
    /// without running one ([`Self::operator`]); that is how the two backends
    /// are compared, on the same construction.
    fn system_bauen<'a>(&mut self, s: &'a mut Szene, h: f64) -> (System<'a>, Vec<Option<Fest>>) {
        let n = s.x.len();
        let g = s.param.schwerkraft;
        let c = s.param.daempfung;

        // Selbstkollision: Nähe-Paare suchen, Federn bilden; die Dämpfung der
        // Annäherung wirkt auf v vor allen anderen Kräften (wie bei CLO).
        self.federn.clear();
        // Klebezonen stehen fest und wirken in jedem Schritt, unabhängig von
        // der Selbstkollision: Eine Zone ist keine Nähe, sondern eine Bindung.
        for z in &s.klebungen {
            if z.is_empty() {
                continue;
            }
            self.federn.extend(crate::klebung::federn(z, &s.x, self.param.klebe_faktor));
        }
        if self.param.selbstkollision {
            if self.selbst.as_ref().is_none_or(|(netz, _)| netz.r_punkt.len() != n) {
                self.selbst = Some(selbstkollision::aus_szene(s, self.param.selbst_zusatzdicke));
            }
            let (netz, werte) = self.selbst.as_ref().expect("eben gebaut");
            let naehe = selbstkollision::finde(netz, &s.x, &s.w, true, true);
            for p in &naehe {
                match p.art {
                    selbstkollision::Art::PunktDreieck => self.statistik.naehe_punkt_dreieck += 1,
                    selbstkollision::Art::KanteKante => self.statistik.naehe_kante_kante += 1,
                }
            }
            self.federn = selbstkollision::federn(netz, werte, &naehe, self.param.selbst_steifigkeit, &s.masse, &s.w, &s.naht.ruhe, &mut s.v);
            // Durchdringung: Schnitte, Gebiete, Schubfedern. Eingeklemmt ist
            // ein Schnitt, wenn der Körper Kanten- und Dreiecksseite aus
            // entgegengesetzten Richtungen hält (Achsel, Schritt).
            let mut schnitte = selbstkollision::finde_schnitte(netz, &s.x, self.param.selbst_schnitte_an_naehten);
            let wurzeln = selbstkollision::gebiete(netz, &mut schnitte);
            self.statistik.durchdringungen += schnitte.len();
            self.statistik.durchdringungsgebiete += wurzeln.iter().enumerate().filter(|&(i, &r)| i == r).count();
            let rand = self.param.kontakt_rand;
            let normale = |i: usize| s.koerper.iter().find_map(|k| k.kontakt(s.x[i], s.kollisionsabstand[i] + rand).map(|kt| kt.normale));
            let eingeklemmt = |kp: [usize; 2], dp: [usize; 3]| {
                let summe = dp.iter().filter_map(|&i| normale(i)).fold([0.0; 3], add);
                norm(summe) > 0.0 && kp.iter().filter_map(|&i| normale(i)).any(|nk| dot(nk, summe) < 0.0)
            };
            let schub = selbstkollision::schubfedern(netz, werte, &schnitte, &wurzeln, self.param.selbst_steifigkeit, &mut self.folge, eingeklemmt);
            self.federn.extend(schub);
            self.schnitte = schnitte;
        }

        // Kanten: Steifigkeit, Richtung, Querfaktor; Kräfte. Daneben die
        // physikalische Last (nur Zug, plus Gewicht) für die Reibung.
        //
        // `nur_topologie`: the device computes direction, cross factor, forces,
        // `diag` and the right-hand side itself, so only what does not follow
        // from the positions is filled in here. Springs (glue zones,
        // self-collision) are not built on the device; a step that has them is
        // built here in full and solved by the CPU backend.
        let nur_topologie = self.loeser.baut_selbst() && self.federn.is_empty();
        let mut f: Vec<[f64; 3]> = Vec::new();
        let mut last: Vec<[f64; 3]> = Vec::new();
        if !nur_topologie {
            f = (0..n).map(|i| sub(mul(g, s.masse[i]), mul(s.v[i], c * s.masse[i]))).collect();
            last = (0..n).map(|i| mul(g, s.masse[i])).collect();
        }
        let mut kanten = Vec::with_capacity(s.abstand.len());
        for (e, &[i, j]) in s.abstand.punkte.iter().enumerate() {
            let (i, j) = (i as usize, j as usize);
            let k = 1.0 / s.abstand.nachgiebigkeit[e];
            let l0 = s.abstand.ruhe[e];
            if nur_topologie {
                kanten.push(Kante { i: i as u32, j: j as u32, k, richtung: [0.0; 3], quer: 0.0, ruhe: l0 });
                continue;
            }
            let d = sub(s.x[j], s.x[i]);
            let l = norm(d).max(1e-12);
            let nr = mul(d, 1.0 / l);
            let fi = mul(nr, k * (l - l0));
            f[i] = add(f[i], fi);
            f[j] = sub(f[j], fi);
            if l > l0 {
                last[i] = add(last[i], fi);
                last[j] = sub(last[j], fi);
            }
            kanten.push(Kante { i: i as u32, j: j as u32, k, richtung: nr, quer: (1.0 - l0 / l).max(0.0), ruhe: l0 });
        }
        let mut naehte = Vec::with_capacity(s.naht.len());
        for (e, &[i, j]) in s.naht.punkte.iter().enumerate() {
            let (i, j) = (i as usize, j as usize);
            // Offen (unendlich, vor dem ersten Kürzen): Ruhelänge = Abstand.
            let l0 = s.naht.ruhe[e];
            if nur_topologie {
                naehte.push(Kante { i: i as u32, j: j as u32, k: self.param.naht_k, richtung: [0.0; 3], quer: 0.0, ruhe: l0 });
                continue;
            }
            let d = sub(s.x[j], s.x[i]);
            let l = norm(d);
            let l0 = if l0.is_finite() { l0 } else { l };
            let fi = if l0 > 0.0 && l > 1e-12 { mul(d, self.param.naht_k * (l - l0) / l) } else { mul(d, self.param.naht_k) };
            f[i] = add(f[i], fi);
            f[j] = sub(f[j], fi);
            // Richtung und Querfaktor wie bei den Kanten; Länge 0: k·I.
            let (richtung, quer) = if l0 > 0.0 && l > 1e-12 { (mul(d, 1.0 / l), (1.0 - l0 / l).max(0.0)) } else { ([0.0; 3], 1.0) };
            naehte.push(Kante { i: i as u32, j: j as u32, k: self.param.naht_k, richtung, quer, ruhe: l0 });
        }
        // Biegung: Kraft und Gauß-Newton-Glied (Gradienten merken).
        let mut biegungen = Vec::with_capacity(s.biegung.len());
        for (b, p) in s.biegung.punkte.iter().enumerate() {
            let idx = p.map(|i| i as usize);
            let k = 1.0 / s.biegung.nachgiebigkeit[b];
            let ruhe = s.biegung.ruhe[b];
            let theta = if nur_topologie { None } else { dieder(idx.map(|i| s.x[i])) };
            let (theta, gr) = match theta {
                Some((t, gr)) => (t, gr),
                // Degenerate four points (zero area or zero edge): the angle
                // does not exist, so the constraint has no stiffness. It stays
                // in the list with k = 0 instead of being dropped - a backend
                // that builds on the device has to keep the same list, or every
                // index behind it shifts.
                None => (0.0, [[0.0; 3]; 4]),
            };
            if !nur_topologie {
                // The angle is only defined modulo 2π; a fold's rest angle sits
                // on the branch cut (see `cpu::winkel_fehler`).
                let fehler = crate::cpu::winkel_fehler(theta, ruhe);
                for m in 0..4 {
                    f[idx[m]] = sub(f[idx[m]], mul(gr[m], k * fehler));
                }
            }
            biegungen.push(Biegung { punkte: *p, grad: gr, k, ruhe });
        }
        if !nur_topologie {
            for fe in &self.federn {
                for q in 0..5 {
                    let i = fe.punkte[q] as usize;
                    f[i] = add(f[i], mul(fe.kraft, fe.gewichte[q]));
                }
            }
        }
        // Das System dieses Unterschritts. Federn der Selbstkollision und der
        // Klebezonen stehen darin; Kontakte folgen unten.
        let mut sys = System {
            h,
            c,
            cg_max: self.param.cg_max,
            cg_toleranz: self.param.cg_toleranz,
            masse: &s.masse,
            w: &s.w,
            v: &s.v,
            x: &s.x,
            g,
            kanten,
            naehte,
            biegungen,
            federn: std::mem::take(&mut self.federn),
            rhs: Vec::new(),
            f: Vec::new(),
            last: Vec::new(),
            pinv: Vec::new(),
            ohne_geometrie: nur_topologie,
        };
        // Kontakte: festlegen, lösen, ziehende freigeben, noch einmal lösen.
        // A contact is a query against the body's velocity, so it is asked here
        // even for a backend that builds everything else itself.
        let mut fest: Vec<Option<Fest>> = vec![None; n];
        for i in 0..n {
            if s.w[i] <= 0.0 {
                continue;
            }
            for k in &s.koerper {
                if let Some(kt) = k.kontakt(s.x[i], s.kollisionsabstand[i] + self.param.kontakt_rand) {
                    let tiefe = kt.tiefe - self.param.kontakt_rand;
                    fest[i] = Some(Fest { normale: kt.normale, vn_soll: tiefe / h + dot(kt.geschwindigkeit, kt.normale), v_koerper: kt.geschwindigkeit });
                }
            }
        }
        if nur_topologie {
            return (sys, fest);
        }
        // b = h·(f − h·H·v)
        let mut hv = vec![[0.0; 3]; n];
        sys.mal_h(&s.v, &mut hv);
        sys.rhs = (0..n).map(|i| mul(sub(f[i], mul(hv[i], h)), h)).collect();
        sys.f = f;
        sys.last = last;
        // Diagonalblöcke von A für den Vorkonditionierer.
        sys.pinv = sys.diag().iter().map(m3_inv).collect();
        (sys, fest)
    }

    /// Ein Zeitschritt der Länge `h`.
    fn schritt(&mut self, s: &mut Szene, h: f64) {
        let n = s.x.len();
        let g = s.param.schwerkraft;
        // Der Weg vor dem Schritt, für die Stoßprüfung der Selbstkollision.
        let x_anfang = if self.param.selbstkollision { s.x.clone() } else { Vec::new() };
        let (mut sys, mut fest) = self.system_bauen(s, h);
        // Warmstart: Δv des vorigen Schritts, beim ersten Schritt h·g.
        let mut dv = if self.dv_vorher.len() == n { self.dv_vorher.clone() } else { vec![mul(g, h); n] };
        let mut fn_kraft = vec![0.0; n];
        for _durchgang in 0..2 {
            let lauf = self.loeser.loese(&sys, &fest, &mut dv);
            self.statistik.loesungen += 1;
            self.statistik.cg_iterationen += lauf.iterationen;
            self.statistik.rest_max = self.statistik.rest_max.max(lauf.rest);
            if lauf.nicht_konvergiert {
                self.statistik.nicht_konvergiert += 1;
            }
            // No point is held: nothing can be released, and the normal force
            // is only needed for held points. The operator below would write
            // nothing, so the whole pass is skipped - it costs a separate
            // upload and readback otherwise (11 % of the frame on the blouse,
            // which has no body).
            if fest.iter().all(|f| f.is_none()) {
                break;
            }
            // Normalkraft aus dem Rest r = A·Δv − b (in Normalrichtung) / h.
            let mut rest = vec![[0.0; 3]; n];
            self.loeser.rest_geloest(&sys, &dv, &mut rest);
            let mut frei = false;
            for i in 0..n {
                if let Some(fe) = fest[i] {
                    let fn_i = dot(rest[i], fe.normale) / h;
                    if fn_i < 0.0 {
                        fest[i] = None;
                        frei = true;
                        self.statistik.freigegeben += 1;
                    } else {
                        fn_kraft[i] = fn_i;
                    }
                }
            }
            if !frei {
                break;
            }
        }

        self.dv_vorher.clone_from(&dv);
        // The load for friction: from the system, or from the device that
        // built it. Read back only when it is used.
        let last = if !sys.ohne_geometrie {
            std::mem::take(&mut sys.last)
        } else if self.param.normalkraft_aus_last {
            self.loeser.last_lesen()
        } else {
            Vec::new()
        };
        drop(sys);
        // Neue Geschwindigkeit, Reibung, Lage.
        for i in 0..n {
            if s.w[i] <= 0.0 {
                continue;
            }
            let mut v = add(s.v[i], dv[i]);
            if let Some(fe) = fest[i] {
                self.statistik.kontakte += 1;
                let rel = sub(v, fe.v_koerper);
                let t = sub(rel, mul(fe.normale, dot(rel, fe.normale)));
                let lt = norm(t);
                let normal = if self.param.normalkraft_aus_last {
                    (-dot(last[i], fe.normale)).max(0.0)
                } else {
                    fn_kraft[i]
                };
                let grenze = s.reibung[i] * normal * h * s.w[i];
                if lt <= grenze {
                    self.statistik.haftend += 1;
                    v = sub(v, t);
                } else if lt > 0.0 {
                    v = sub(v, mul(t, grenze / lt));
                }
            }
            s.v[i] = v;
            let alt = s.x[i];
            s.x[i] = add(alt, mul(v, h));
            // Hat der Weg die Haut durchstoßen, setzt der Punkt am Eintritt auf
            // (sonst ginge er tiefer als die Suchtiefe verloren oder käme an
            // einem dünnen Teil auf der Gegenseite heraus). Der Stoß nimmt die
            // Normalgeschwindigkeit relativ zum Körper weg; Coulomb auf den
            // Stoß nimmt von der tangentialen bis zu μ·|vₙ|.
            for k in s.koerper.iter().filter(|_| self.param.durchstoss_pruefen) {
                if let Some((p, nk, vk)) = k.durchstoss(alt, s.x[i]) {
                    self.statistik.durchstoesse += 1;
                    s.x[i] = add(p, mul(nk, s.kollisionsabstand[i]));
                    let rel = sub(s.v[i], vk);
                    let vn = dot(rel, nk);
                    if vn < 0.0 {
                        let t = sub(rel, mul(nk, vn));
                        let lt = norm(t);
                        let grenze = s.reibung[i] * -vn;
                        let t = if lt <= grenze { [0.0; 3] } else { mul(t, 1.0 - grenze / lt) };
                        s.v[i] = add(vk, t);
                    }
                }
            }
            // Rest der Eindringung (Linearisierung) wegschieben und die
            // Geschwindigkeit in den Körper hinein entfernen.
            for k in &s.koerper {
                if let Some(kt) = k.kontakt(s.x[i], s.kollisionsabstand[i]) {
                    s.x[i] = add(s.x[i], mul(kt.normale, kt.tiefe));
                    let vn = dot(sub(s.v[i], kt.geschwindigkeit), kt.normale);
                    if vn < 0.0 {
                        s.v[i] = sub(s.v[i], mul(kt.normale, vn));
                    }
                }
            }
        }
        // Körperpunkte gegen Stoffdreiecke: kein Körperpunkt sticht durch den
        // Stoff (Kanten zwischen zwei Stoffpunkten durch Schulter oder Arm).
        for _ in 0..self.param.koerperpunkt_durchgaenge {
            let mut n = 0;
            for k in &s.koerper {
                n += k.punkte_gegen_dreiecke(self.param.koerperpunkt_tiefe, &s.dreiecke, &s.w, &mut s.x, &mut s.v);
            }
            self.statistik.koerperpunkte += n;
            if n == 0 {
                break;
            }
        }
        // Stoff gegen Stoff: den Weg prüfen und Durchgänge zurücksetzen (reine
        // Lagekorrektur, v bleibt wie bei CLO). Punkte am Körper zählen wie
        // gebundene Punkte bei CLO als schwer (× 10⁹).
        if let Some((netz, _)) = self.selbst.as_ref().filter(|_| self.param.selbstkollision) {
            let stoesse = selbstkollision::finde_stoesse(netz, &x_anfang, &s.x, &s.w, true, true);
            let stoesse = selbstkollision::ohne_durchdrungene(netz, stoesse, &self.schnitte);
            if !stoesse.is_empty() {
                let m_eff: Vec<f64> = (0..n).map(|i| if fest[i].is_some() { s.masse[i] * 1e9 } else { s.masse[i] }).collect();
                self.statistik.selbst_stoesse += selbstkollision::stossantwort(&stoesse, &mut s.x, &m_eff, &s.w, self.param.selbst_stoss_iterationen);
            }
        }
    }

}

impl Rechenweg for ImplizitCg {
    fn bild(&mut self, s: &mut Szene) {
        let n = self.param.schritte_je_bild.max(1);
        let h = s.param.bild_dt / n as f64;
        self.statistik = ImplizitStatistik::default();
        let koerper_anfang = s.koerper.clone();
        for k in 0..n {
            let weg = (k + 1) as f64 * h;
            for (kb, ka) in s.koerper.iter_mut().zip(&koerper_anfang) {
                *kb = ka.verschoben(weg);
            }
            self.schritt(s, h);
        }
        s.zeit += s.param.bild_dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zwei freie Punkte, 10 cm auseinander, an einer Nahtfeder: Mit
    /// Ruhelänge 4 cm stellen sie sich auf 4 cm ein, mit dem Kürzen je Bild
    /// ([`Szene::naht_kuerzen`]) laufen sie mit begrenzter Geschwindigkeit
    /// zusammen bis auf 0.
    #[test]
    fn naht_mit_ruhelaenge_und_kuerzen() {
        use crate::szene::{Gruppe, Simparameter};
        let szene = |ruhe: f64| Szene {
            x: vec![[0.0; 3], [0.1, 0.0, 0.0]],
            v: vec![[0.0; 3]; 2],
            masse: vec![1e-5; 2],
            w: vec![1e5; 2],
            kollisionsabstand: vec![2e-4; 2],
            reibung: vec![0.0; 2],
            abstand: Gruppe::default(),
            naht: Gruppe { punkte: vec![[0, 1]], ruhe: vec![ruhe], nachgiebigkeit: vec![0.0], farben: vec![0..1] },
            biegung: Gruppe::default(),
            biegung_grund: Vec::new(),
            dreiecke: Vec::new(),
            ruhe_inv: Vec::new(),
            faser: Vec::new(),
            faser_ersatz: 0,
            faser_entartet: 0,
            teile: Vec::new(),
            naht_biegung_ohne: 0,
            einhalte_kanten: 0,
            klebungen: Vec::new(),
            koerper: Vec::new(),
            param: Simparameter { schwerkraft: [0.0; 3], daempfung: 5.0, ..Simparameter::default() },
            zeit: 0.0,
        };
        let abstand = |s: &Szene| norm(sub(s.x[1], s.x[0]));
        let mut s = szene(0.04);
        let mut rw = ImplizitCg::neu(ImplizitParameter::default());
        for _ in 0..60 {
            rw.bild(&mut s);
        }
        assert!((abstand(&s) - 0.04).abs() < 1e-6, "{}", abstand(&s));
        // Kürzen um 1 mm je Bild: nach 30 Bildern 3 cm, nach 120 Bildern zu.
        let mut s = szene(0.0);
        s.naht_offen();
        let mut rw = ImplizitCg::neu(ImplizitParameter::default());
        let mut groesster_schritt = 0.0f64;
        for bild in 1..=120 {
            let vorher = abstand(&s);
            s.naht_kuerzen(1e-3);
            rw.bild(&mut s);
            groesster_schritt = groesster_schritt.max(vorher - abstand(&s));
            if bild == 30 {
                assert!((abstand(&s) - 0.07).abs() < 1e-4, "nach 30 Bildern {}", abstand(&s));
            }
        }
        assert!(abstand(&s) < 1e-6, "{}", abstand(&s));
        assert!(groesster_schritt < 1.1e-3, "Schritt {groesster_schritt}");
    }

    #[test]
    fn inverse_3x3() {
        let m = [[4.0, 1.0, 0.5], [1.0, 3.0, 0.2], [0.5, 0.2, 2.0]];
        let i = m3_inv(&m);
        for r in 0..3 {
            for c in 0..3 {
                let v: f64 = (0..3).map(|k| m[r][k] * i[k][c]).sum();
                assert!((v - if r == c { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
    }
}
