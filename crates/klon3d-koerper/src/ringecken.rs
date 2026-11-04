//! Ein Netzring bekommt einen Punkt in einer vorgegebenen Richtung.
//!
//! Der Grund steht in [`crate::bogenmasse`]: ein Querschnitt lässt sich nur so
//! genau messen, wie sein Polygon Ecken darstellen kann. Zwei Stellen der
//! Zielform sind solche Ecken - die Brustspitzen (1,2 rad Knick) und die beiden
//! Seitenpunkte (flache Flanke, dort entscheidet die Teilung zwischen vorn und
//! hinten). Ein MakeHuman-Rumpfring hat an keiner von beiden zufällig einen
//! Punkt: über der Brustspitze liegt eine Lücke von 6°, und auf der Flanke ist
//! der breiteste Punkt des Polygons schlicht der, der zuletzt kommt.
//!
//! [`crate::bogenmasse::auf_umriss`] setzt die Ecken in den *gemessenen*
//! Querschnitt ein, und das genügt für die Messung des Umrisses - aber nicht für
//! das Netz. Das Netz behält seine Punkte, und eine radiale Verschiebung, die
//! an 143 Punkten abgetastet wird, kann keine Ecke erzeugen, die zwischen zwei
//! davon liegt. Gemessen an der MakeHuman-Brust kostet das 0,5 cm des
//! Zielabstands, und es verschwindet nicht mit der Dichte: die Sehne rückt bei
//! jeder Verfeinerung nur asymptotisch an die Ecke heran.
//!
//! Deshalb wird die Ecke vorher **in das Netz** gesetzt, als gewöhnliche
//! Kantenteilung: der neue Punkt liegt auf einer Kante des Rings, das Polygon
//! ändert dabei seine Form nicht (der Punkt liegt auf der Kante), und die
//! angrenzenden Dreiecke werden geteilt. Es entsteht keine Naht, kein Punkt
//! wandert, und die Zahl der Dreiecke steigt je Ecke um höchstens zwei.

use crate::{rumpf, Netz, Punkt3};
use std::collections::HashMap;

/// Raster, in dem eine Ringposition einem Netupunkt zugeordnet wird, mm.
///
/// Der Ring entsteht als Schnitt durch die Dreiecke; seine Punkte sind
/// Netzpunkte auf den Milliardstel Millimeter genau. `1e-3` mm ist der
/// Rasterwert, den auch die Verkettung in [`crate::rumpf`] benutzt.
const RASTER: f64 = 1.0e-3;

/// Ab welchem Winkelabstand ein vorhandener Punkt als "in dieser Richtung"
/// gilt, rad.
///
/// Float-Rauschen, keine Breite: ein Punkt, den diese Datei einsetzt und
/// hinterher wiederfindet, weicht um `1e-16` rad ab. Ein Punkt ein Grad daneben
/// ist ein anderer Punkt.
const GLEICH: f64 = 1.0e-4;

fn schluessel(p: (f64, f64)) -> (i64, i64) {
    (
        (p.0 / RASTER).round() as i64,
        (p.1 / RASTER).round() as i64,
    )
}

/// Signed angle from `a` to `b` in `(-π, π]`.
fn winkel_differenz(b: f64, a: f64) -> f64 {
    let mut d = b - a;
    while d <= -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    while d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    }
    d
}

/// Flächenschwerpunkt eines geschlossenen Rings, mm.
fn mitte(ring: &[(f64, f64)]) -> (f64, f64) {
    let m = ring.len();
    let n = m.max(1) as f64;
    let eckig = (
        ring.iter().map(|p| p.0).sum::<f64>() / n,
        ring.iter().map(|p| p.1).sum::<f64>() / n,
    );
    if m < 3 {
        return eckig;
    }
    let (mut a, mut cx, mut cz) = (0.0, 0.0, 0.0);
    for i in 0..m {
        let p = ring[i];
        let q = ring[(i + 1) % m];
        let kreuz = p.0 * q.1 - q.0 * p.1;
        a += kreuz;
        cx += (p.0 + q.0) * kreuz;
        cz += (p.1 + q.1) * kreuz;
    }
    if a.abs() < 1.0e-9 {
        return eckig;
    }
    (cx / (3.0 * a), cz / (3.0 * a))
}

/// Setzt in jede der `richtungen` einen Netupunkt auf den Ring der Höhe `y`.
///
/// `richtungen` sind Winkel im Bogenmass um den Flächenschwerpunkt des Rings -
/// dieselben, die [`crate::bogenmasse::ecken_richtungen`] aus der Zielform
/// liefert und die [`crate::bogenmasse::auf_umriss`] benutzt. Gibt zurück, wie
/// viele Punkte wirklich eingesetzt wurden.
pub fn ecken_einpassen(netz: &mut Netz, y: f64, richtungen: &[f64]) -> usize {
    let mut anzahl = 0;
    for &w in richtungen {
        if ecke_einpassen(netz, y, w) {
            anzahl += 1;
        }
    }
    anzahl
}

/// Ein Punkt in Richtung `w` auf dem Ring der Höhe `y`.
///
/// Der Ring wird nach jedem Einsetzen neu gelesen: zwei Richtungen können in
/// dieselbe Kante fallen, und dann muss die zweite die schon geteilte Kante
/// treffen und nicht die alte.
fn ecke_einpassen(netz: &mut Netz, y: f64, w: f64) -> bool {
    let Some(q) = rumpf(netz, y) else {
        return false;
    };
    if q.ring.len() < 4 {
        return false;
    }
    let (cx, cz) = mitte(&q.ring);
    let wink: Vec<f64> = q
        .ring
        .iter()
        .map(|p| (p.1 - cz).atan2(p.0 - cx))
        .collect();
    if wink.iter().any(|a| winkel_differenz(*a, w).abs() < GLEICH) {
        return false;
    }

    // Die Kante suchen, die der Strahl schneidet. Getestet wird in
    // Laufrichtung des Rings, damit ein im oder gegen den Uhrzeigersinn
    // gegebener Ring gleich behandelt wird.
    let m = q.ring.len();
    let mut treffer = None;
    for i in 0..m {
        let d = winkel_differenz(wink[(i + 1) % m], wink[i]);
        let t = winkel_differenz(w, wink[i]);
        if (d > 0.0 && t >= 0.0 && t < d) || (d < 0.0 && t <= 0.0 && t > d) {
            treffer = Some(i);
            break;
        }
    }
    let Some(i) = treffer else {
        return false;
    };

    // Netzpunkte des Rings: alle Punkte auf dieser Höhe, im Raster der
    // Verkettung zugeordnet.
    let mut karte: HashMap<(i64, i64), usize> = HashMap::new();
    for (k, p) in netz.punkte.iter().enumerate() {
        if (p[1] - y).abs() <= RASTER {
            karte.entry(schluessel((p[0], p[2]))).or_insert(k);
        }
    }
    let (pa, pb) = (q.ring[i], q.ring[(i + 1) % m]);
    let (Some(&ia), Some(&ib)) = (karte.get(&schluessel(pa)), karte.get(&schluessel(pb))) else {
        return false;
    };
    if ia == ib {
        return false;
    }

    // Schnittpunkt des Strahls mit der Kante, nicht ihr Mittelpunkt: nur so
    // liegt der neue Punkt in *genau* der Richtung, für die die Zielform ihren
    // Radius vorgibt.
    let (dx, dz) = (w.cos(), w.sin());
    let (ex, ez) = (pb.0 - pa.0, pb.1 - pa.1);
    let nenner = dx * ez - dz * ex;
    if nenner.abs() < 1.0e-12 {
        return false;
    }
    let (ax, az) = (pa.0 - cx, pa.1 - cz);
    let s = (ax * dz - az * dx) / nenner;
    if !(0.0..=1.0).contains(&s) {
        return false;
    }
    let neu = [pa.0 + s * ex, y, pa.1 + s * ez];
    kante_teilen(netz, ia, ib, neu)
}

/// Teilt die Netzkante `i-j` in `p` und gibt zurück, ob sie eine war.
///
/// Jedes Dreieck, das die Kante enthält, wird zu zwei Dreiecken; der Umlaufsinn
/// bleibt, weil `p` auf der Kante liegt und die Reihenfolge `i → p → j` dieselbe
/// ist. Eine Kante, die zu keinem Dreieck gehört, wird nicht geteilt.
fn kante_teilen(netz: &mut Netz, i: usize, j: usize, p: Punkt3) -> bool {
    let neu = netz.punkte.len();
    let mut dreiecke: Vec<[usize; 3]> = Vec::with_capacity(netz.dreiecke.len() + 2);
    let mut geteilt = 0;
    for &[a, b, c] in &netz.dreiecke {
        let e = [a, b, c];
        let mut ersetzt = false;
        for k in 0..3 {
            let (u, v, w) = (e[k], e[(k + 1) % 3], e[(k + 2) % 3]);
            if (u == i && v == j) || (u == j && v == i) {
                dreiecke.push([u, neu, w]);
                dreiecke.push([neu, v, w]);
                ersetzt = true;
                geteilt += 1;
                break;
            }
        }
        if !ersetzt {
            dreiecke.push(e);
        }
    }
    if geteilt == 0 {
        return false;
    }
    netz.punkte.push(p);
    netz.dreiecke = dreiecke;
    true
}
