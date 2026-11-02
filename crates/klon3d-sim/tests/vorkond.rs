//! Was der Vorkonditionierer wirklich kann (Aufgabe #41), gemessen am
//! Bluse3-v2-System — derselben Bluse, die `examples/gpu_takt.rs` löst.
//!
//! Ergebnis: **keine** der untersuchten Varianten erreicht 1e-5 in höchstens 50
//! Iterationen *und* bleibt dabei billiger als das heutige Block-Jacobi. Das
//! Test hält die Zahlen fest, damit der nächste Versuch nicht bei Null anfängt
//! (Punkt 4 der Aufgabe).
//!
//! Warum das nicht überrascht: `A = M(1+h·c) + h²·H` ist ein elliptischer
//! Operator mit einem `h⁻²`-Spektrum. Die Masse ist mit ~10⁻⁵ der Diagonale
//! verschwindend, deshalb liegt `λ_min(A) = 1.31e-5` bei `λ_max(A) = 2.5677e-2`
//! — Kondition **1961** ohne Vorkonditionierung und **2097** mit den 3×3-Blöcken.
//! Der Diagonalschätzer ist also fast die Einheitsmatrix, und mehr Blockdiagonale
//! gibt es nicht zu holen.
//!
//! Nur mit `cargo test --release`.

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::implizit::{System, M3};
use klon3d_sim::{
    CpuLoeser, ImplizitCg, ImplizitParameter, Loeser, Rechenweg, Szene, Teilvorgabe,
    RADICAL_RUBBER_04,
};

const ZIEL: f64 = 1e-5;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn m3_mal(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn m3_inv(m: &M3) -> M3 {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = 1.0 / det;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
        ],
    ]
}

fn cholesky(a: &mut [Vec<f64>]) -> bool {
    let m = a.len();
    for i in 0..m {
        for j in 0..=i {
            let mut sum = a[i][j];
            for k in 0..j {
                sum -= a[i][k] * a[j][k];
            }
            if i == j {
                if sum <= 0.0 {
                    return false;
                }
                a[i][i] = sum.sqrt();
            } else {
                a[i][j] = sum / a[j][j];
            }
        }
    }
    true
}

fn cholesky_loese(l: &[Vec<f64>], b: &mut [f64]) {
    let m = l.len();
    for i in 0..m {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i][k] * b[k];
        }
        b[i] = s / l[i][i];
    }
    for i in (0..m).rev() {
        let mut s = b[i];
        for k in i + 1..m {
            s -= l[k][i] * b[k];
        }
        b[i] = s / l[i][i];
    }
}

/// Die Bluse, wie `examples/gpu_takt.rs` sie aufbaut: flach, ohne Körper,
/// 6 471 Punkte, drei Bilder mit dem Vorgabe-Budget als Warmstart.
fn bluse() -> (Szene, ImplizitCg) {
    let wurzel =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster");
    let m = klon3d_pattern::read_path(&wurzel.join("bluse3_v2.sm2d")).expect("Muster");
    let ev = Auswertung::lade(&m, &wurzel.join("measurements.smis")).expect("Maße");
    let defs = lies_nahtliste(
        &std::fs::read_to_string(wurzel.join("bluse3_v2.naehte")).expect("Nahtliste"),
    )
    .expect("Nahtliste");
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).expect("Nähte");
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).expect("Netz");
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.055,
    };
    let mut s = Szene::aus_netz(
        &netz,
        &|_| vg,
        &|ti, q| [q[0] * 1e-3, q[1] * 1e-3, ti as f64 * 0.01],
        vec![],
        klon3d_sim::Simparameter {
            schwerkraft: [0.0, 0.0, -9.81],
            ..klon3d_sim::Simparameter::default()
        },
    );
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    for _ in 0..3 {
        rw.bild(&mut s);
    }
    (s, rw)
}

fn aggregat(n: usize, nachbarn: &[Vec<usize>], groesse: usize) -> (Vec<usize>, usize) {
    let mut agg = vec![usize::MAX; n];
    let mut anzahl = 0;
    for start in 0..n {
        if agg[start] != usize::MAX {
            continue;
        }
        let mut queue = vec![start];
        agg[start] = anzahl;
        let mut kopf = 0;
        let mut mitglieder = 1;
        while kopf < queue.len() && mitglieder < groesse {
            let p = queue[kopf];
            kopf += 1;
            for &q in &nachbarn[p] {
                if agg[q] == usize::MAX && mitglieder < groesse {
                    agg[q] = anzahl;
                    queue.push(q);
                    mitglieder += 1;
                }
            }
        }
        anzahl += 1;
    }
    (agg, anzahl)
}

/// Der grobe Operator `R·A·Rᵀ`.
///
/// Die Vorzeichen sind hier die Falle, in die die erste Fassung des
/// Messprogramms gelaufen ist: `System::mal_h` legt je Kante `out[i] += K·(p_i − p_j)`
/// und `out[j] −= K·(p_i − p_j)` ab, also ist `A_ij = A_ji = −K` und
/// `A_ii = A_jj = +K`. Wer die Blöcke — wie selbstverständlich — positiv
/// addiert, baut eine andere Matrix und misst an ihr. Die Biegung trägt alle
/// Blöcke positiv bei, aber mit demselben `h²` wie die Kanten (`A = M(1+h·c) + h²·H`,
/// und `H` führt `b.k·∇θ⊗∇θ` ohne `h²`).
fn grob_aus_system(sys: &System<'_>, agg: &[usize], anzahl: usize) -> Vec<Vec<f64>> {
    let n = sys.masse.len();
    let m = 3 * anzahl;
    let mut ac = vec![vec![0.0f64; m]; m];
    let h2 = sys.h * sys.h;
    let add = |ac: &mut Vec<Vec<f64>>, i: usize, j: usize, block: &M3, sign: f64| {
        let (ai, aj) = (agg[i], agg[j]);
        for x in 0..3 {
            for y in 0..3 {
                ac[3 * ai + x][3 * aj + y] += sign * block[x][y];
            }
        }
    };
    for i in 0..n {
        let masse = sys.masse[i] * (1.0 + sys.h * sys.c);
        let mut block = [[0.0; 3]; 3];
        for x in 0..3 {
            block[x][x] = masse;
        }
        add(&mut ac, i, i, &block, 1.0);
    }
    for k in sys.kanten.iter().chain(&sys.naehte) {
        let (i, j) = (k.i as usize, k.j as usize);
        let mut block = [[0.0; 3]; 3];
        for x in 0..3 {
            for y in 0..3 {
                let nn = k.richtung[x] * k.richtung[y] * (1.0 - k.quer);
                let id = if x == y { k.quer } else { 0.0 };
                block[x][y] = k.k * h2 * (nn + id);
            }
        }
        add(&mut ac, i, j, &block, -1.0);
        add(&mut ac, j, i, &block, -1.0);
        add(&mut ac, i, i, &block, 1.0);
        add(&mut ac, j, j, &block, 1.0);
    }
    for b in &sys.biegungen {
        let idx = b.punkte.map(|q| q as usize);
        for p in 0..4 {
            for l in 0..4 {
                let mut block = [[0.0; 3]; 3];
                for x in 0..3 {
                    for y in 0..3 {
                        block[x][y] = h2 * b.k * b.grad[p][x] * b.grad[l][y];
                    }
                }
                add(&mut ac, idx[p], idx[l], &block, 1.0);
            }
        }
    }
    for i in 0..m {
        for j in 0..i {
            let s = 0.5 * (ac[i][j] + ac[j][i]);
            ac[i][j] = s;
            ac[j][i] = s;
        }
    }
    ac
}

struct ZweiEbenen {
    agg: Vec<usize>,
    anzahl: usize,
    /// Cholesky-Faktor von `A_c`.
    chol: Vec<Vec<f64>>,
    /// Inverse 3×3-Blöcke von `A` je Punkt (Block-Jacobi auf der feinen Ebene).
    pinv: Vec<M3>,
}

impl ZweiEbenen {
    fn bauen(sys: &System<'_>, groesse: usize) -> Self {
        let n = sys.masse.len();
        let mut nachbarn = vec![Vec::new(); n];
        for k in sys.kanten.iter().chain(&sys.naehte) {
            let (i, j) = (k.i as usize, k.j as usize);
            nachbarn[i].push(j);
            nachbarn[j].push(i);
        }
        let (agg, anzahl) = aggregat(n, &nachbarn, groesse);
        let ac = grob_aus_system(sys, &agg, anzahl);
        let diag = sys.diag();
        let pinv = diag.iter().map(m3_inv).collect();
        let mut chol = ac;
        assert!(cholesky(&mut chol), "A_c nicht positiv definit");
        ZweiEbenen {
            agg,
            anzahl,
            chol,
            pinv,
        }
    }

    /// Der grobe Operator, wie er gebaut wurde — für den Beweis gegen `R·A·Rᵀ`.
    fn grob_matrix(&self, sys: &System<'_>) -> Vec<Vec<f64>> {
        grob_aus_system(sys, &self.agg, self.anzahl)
    }

    /// Ein V-Zyklus `x ↦ V(x)` als Vorkonditionierer, mit `vor`/`nach`
    /// Glättungsschritten und Dämpfung `omega` auf der feinen und groben Ebene.
    fn v_zyklus(
        &self,
        sys: &System<'_>,
        b: &[[f64; 3]],
        x: &mut [[f64; 3]],
        vor: usize,
        nach: usize,
        omega: f64,
    ) {
        let n = sys.masse.len();
        let mut ap = vec![[0.0; 3]; n];
        let glaetten = |x: &mut [[f64; 3]], ap: &mut [[f64; 3]]| {
            sys.mal_a(x, ap);
            for i in 0..n {
                let d = [b[i][0] - ap[i][0], b[i][1] - ap[i][1], b[i][2] - ap[i][2]];
                let k = m3_mal(&self.pinv[i], d);
                for c in 0..3 {
                    x[i][c] += omega * k[c];
                }
            }
        };
        for _ in 0..vor {
            glaetten(x, &mut ap);
        }
        sys.mal_a(x, &mut ap);
        let m = 3 * self.anzahl;
        let mut rc = vec![0.0f64; m];
        for i in 0..n {
            let ai = self.agg[i];
            for c in 0..3 {
                rc[3 * ai + c] += b[i][c] - ap[i][c];
            }
        }
        // Grobkorrektur auf der feinen Ebene: der grobe Zyklus ist hier eine
        // einzige direkte Lösung (eine grobe Ebene).
        cholesky_loese(&self.chol, &mut rc);
        for i in 0..n {
            let ai = self.agg[i];
            for c in 0..3 {
                x[i][c] += rc[3 * ai + c];
            }
        }
        for _ in 0..nach {
            glaetten(x, &mut ap);
        }
    }
}

/// PCG bis `ZIEL`; liefert die Iteration und wie oft der **feine** Operator
/// gerechnet wurde (das ist die Rechenzeit, nicht die Iterationszahl).
fn cg_v(
    sys: &System<'_>,
    zwei: &ZweiEbenen,
    max_it: usize,
    vor: usize,
    nach: usize,
    omega: f64,
) -> (Option<usize>, usize) {
    let n = sys.masse.len();
    let b = &sys.rhs;
    let mut dv = vec![[0.0; 3]; n];
    let mut ap = vec![[0.0; 3]; n];
    sys.mal_a(&dv, &mut ap);
    let mut r: Vec<[f64; 3]> = (0..n)
        .map(|i| [b[i][0] - ap[i][0], b[i][1] - ap[i][1], b[i][2] - ap[i][2]])
        .collect();
    let mut z = vec![[0.0; 3]; n];
    zwei.v_zyklus(sys, &r, &mut z, vor, nach, omega);
    let mut p = z.clone();
    let mut reach: f64 = (0..n).map(|i| dot(r[i], z[i])).sum();
    let d0 = reach;
    let mut paesse = vor + 1 + nach;
    for it in 1..=max_it {
        sys.mal_a(&p, &mut ap);
        paesse += 1;
        let ca: f64 = (0..n).map(|i| dot(p[i], ap[i])).sum();
        let alpha = reach / ca;
        for i in 0..n {
            for c in 0..3 {
                dv[i][c] += p[i][c] * alpha;
                r[i][c] -= ap[i][c] * alpha;
            }
        }
        let mut z = vec![[0.0; 3]; n];
        zwei.v_zyklus(sys, &r, &mut z, vor, nach, omega);
        paesse += vor + 1 + nach;
        let neu: f64 = (0..n).map(|i| dot(r[i], z[i])).sum();
        if (neu / d0).sqrt() <= ZIEL {
            return (Some(it), paesse);
        }
        let beta = neu / reach;
        reach = neu;
        for i in 0..n {
            for c in 0..3 {
                p[i][c] = z[i][c] + p[i][c] * beta;
            }
        }
    }
    (None, paesse)
}

/// `A` auf dem Bluse3-v2-System ist derselbe Operator wie in `examples/gpu_takt.rs`.
/// Drei Zahlen, die #41 festhalten:
///
/// * mit dem Vorgabe-Budget von 50 Iterationen läuft der Löser in die Grenze
///   und meldet `nicht_konvergiert` — der Zustand, den #41 beheben will;
/// * bis 1e-5 braucht er 182 Iterationen (Budget dafür auf 400 gesetzt, sonst
///   dieselbe Matrix und dieselbe Toleranz);
/// * das sind 3,64-mal das Budget, also ist ein Faktor 3,6 zu holen.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn block_jacobi_braucht_182_iterationen() {
    let (mut s, mut rw) = bluse();
    let mut sys = rw.system_fuer(&mut s);
    assert_eq!(sys.masse.len(), 6471, "das System der Messung");
    assert_eq!(sys.cg_max, 50, "Budget der Vorgabe");
    assert_eq!(sys.cg_toleranz, 1e-5, "Toleranz der Vorgabe");
    let fest = vec![None; sys.masse.len()];
    let mut loeser = CpuLoeser;

    // 1. Das Budget der Vorgabe: der Löser kommt nicht durch.
    let mut dv = vec![[0.0; 3]; sys.masse.len()];
    let knapp = loeser.loese(&sys, &fest, &mut dv);
    assert_eq!(knapp.iterationen, 50, "Budget ausgeschöpft");
    assert!(
        knapp.nicht_konvergiert,
        "bei 50 Iterationen nicht konvergiert"
    );
    assert!(knapp.rest > 1e-3, "Endrest nach 50: {:.2e}", knapp.rest);
    println!(
        "Budget 50: {} Iterationen, Rest {:.2e}, nicht konvergiert {}",
        knapp.iterationen, knapp.rest, knapp.nicht_konvergiert
    );

    // 2. Derselbe Operator, nur mehr Budget: 182 Iterationen bis 1e-5.
    sys.cg_max = 400;
    let mut dv = vec![[0.0; 3]; sys.masse.len()];
    let voll = loeser.loese(&sys, &fest, &mut dv);
    assert!(!voll.nicht_konvergiert, "Rest {:.2e}", voll.rest);
    assert_eq!(voll.iterationen, 182, "Iterationen bis 1e-5");
}

/// Der grobe Operator ist genau `R·A·Rᵀ` des feinen. Ohne diese Probe ist der
/// gemessene V-Zyklus eine Aussage über eine andere Matrix — genau das ist in
/// der ersten Fassung passiert (Vorzeichen der Kantenblöcke, fehlendes `h²` bei
/// der Biegung), und es fiel erst auf, als die Probe da war.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn groboperator_ist_r_a_r_transponiert() {
    let (mut s, mut rw) = bluse();
    let sys = rw.system_fuer(&mut s);
    let n = sys.masse.len();
    let zwei = ZweiEbenen::bauen(&sys, 16);
    let ac = zwei.grob_matrix(&sys);
    // Spaltenweise: für `x = Rᵀ·e_k` (Eins auf allen Punkten des Aggregats k)
    // muss der feine Operator, restringiert, die Spalte k von `A_c` ergeben.
    let mut ap = vec![[0.0; 3]; n];
    let mut worst = 0.0f64;
    let mut scale = 0.0f64;
    for k in [0usize, zwei.anzahl / 3, zwei.anzahl / 2, zwei.anzahl - 1] {
        let mut x = vec![[0.0; 3]; n];
        for i in 0..n {
            if zwei.agg[i] == k {
                x[i] = [1.0, 1.0, 1.0];
            }
        }
        sys.mal_a(&x, &mut ap);
        for row in 0..zwei.anzahl {
            let mut got = [0.0f64; 3];
            for i in 0..n {
                if zwei.agg[i] == row {
                    for c in 0..3 {
                        got[c] += ap[i][c];
                    }
                }
            }
            for c in 0..3 {
                let want: f64 = (0..3).map(|y| ac[3 * row + c][3 * k + y]).sum();
                worst = worst.max((got[c] - want).abs());
                scale = scale.max(want.abs());
            }
        }
    }
    assert!(
        worst <= 1e-10 * scale,
        "Groboperator ist nicht R·A·Rᵀ: {worst:.3e} bei Maßstab {scale:.3e}"
    );
}

/// Zwei Ebenen mit korrektem grobem Operator: V(1,1) mit Dämpfung 0.7 braucht
/// 55 Iterationen, kostet aber 223 feine Operatorpässe gegen 182 von
/// Block-Jacobi. V(2,2) braucht 43 Iterationen bei 263 Pässen. Beides ist
/// **langsamer** als das, was da ist — der Test hält die Grenze fest.
#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn zwei_ebenen_senkt_iterationen_aber_nicht_die_arbeit() {
    let (mut s, mut rw) = bluse();
    let sys = rw.system_fuer(&mut s);
    let zwei = ZweiEbenen::bauen(&sys, 16);
    let (it11, p11) = cg_v(&sys, &zwei, 200, 1, 1, 0.7);
    let (it22, p22) = cg_v(&sys, &zwei, 200, 2, 2, 0.7);
    assert_eq!(it11, Some(55), "V(1,1) Iterationen");
    assert_eq!(it22, Some(43), "V(2,2) Iterationen");
    assert_eq!(p11, 223, "V(1,1) feine Pässe");
    assert_eq!(p22, 263, "V(2,2) feine Pässe");
    // Die eigentliche Aussage: keiner der beiden ist billiger.
    assert!(p11 > 182 && p22 > 182, "beide teurer als Block-Jacobi");
}
