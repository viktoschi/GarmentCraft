//! Maßband: Umfänge wie ein straff gespanntes Band.
//!
//! [`crate::rumpf`] misst die Länge der Schnittkontur. Ein Maßband liegt
//! anders: Es überbrückt Dekolleté, Gesäßfalte und Leisten. So misst auch
//! CLO, und so misst die Anwenderin. Am BaseMesh macht das 1,3 cm an der Brust
//! und 1,7 cm an der Hüfte aus.
//!
//! Gebaut nach der Spezifikation `spez/messung.md`
//! (Abschnitte 1.1–1.5, 1.9 und 4) und geprüft gegen
//! `spez/pruefwerte.json`:
//!
//! 1. **Kantenschnitt**: Die Ebene schneidet die Netzkanten in der
//!    Reihenfolge von `BaseMesh.be`; nur Kanten im gewählten Körpersegment.
//! 2. **0,5-cm-Filter**: Ein Punkt fällt weg, wenn ein schon behaltener näher
//!    als 0,5 cm liegt.
//! 3. **Kette**: Start beim Punkt nächst `P0`, dann immer der nächste Nachbar,
//!    solange er näher als 20 cm liegt. Offene Nähte stören nicht, das
//!    Verfahren braucht die Topologie nicht.
//! 4. **Hülle**: Dellen und Rücksprünge werden gestrichen; die Länge der
//!    geschlossenen Restlinie ist der Umfang.
//!
//! Alle Rechnungen hier laufen in Zentimetern; [`punkte_cm`] und
//! [`zentriert_cm`] holen die Punkte aus einem [`Netz`] in Millimetern.

use crate::{Netz, Punkt3};
use std::fs;
use std::io;
use std::path::Path;

/// Wo ein Netzpunkt liegt (`range_list.rlist`, `formung.md` 2.3).
pub mod segment {
    pub const ARM_RECHTS: i32 = 0;
    pub const ARM_LINKS: i32 = 1;
    pub const BEIN_RECHTS: i32 = 2;
    pub const BEIN_LINKS: i32 = 3;
    pub const RUMPF: i32 = 4;
    pub const AUSSERHALB: i32 = 5;
    pub const KOPF: i32 = 6;
    /// Alle Kanten, ohne Segmentprüfung.
    pub const ALLE: i32 = -1;
}

/// Startschranke für den Punkt nächst `P0`, cm.
const START_SCHRANKE: f64 = 100_000.0;
/// Mindestabstand der behaltenen Schnittpunkte, cm.
const FILTER: f64 = 0.5;
/// Grösster Schritt in der Kette, cm.
const KETTENGLIED: f64 = 20.0;
/// So viele Plätze wird die Kette vor der Hülle gedreht.
const DREHUNG: usize = 20;

/// Waagerechte Normale.
pub const WAAGERECHT: Punkt3 = [0.0, 1.0, 0.0];

/// Was die Messung über das Netz wissen muss, ausser den Punkten.
///
/// Die Formung verschiebt nur Punkte; Kanten, Segmente und Landmarken gelten
/// für jedes geformte Netz derselben Topologie.
#[derive(Clone, Debug, Default)]
pub struct Messdaten {
    /// Kanten in Dateireihenfolge; die Reihenfolge ist Teil des Verfahrens.
    pub kanten: Vec<[u32; 2]>,
    /// Körpersegment je Punkt.
    pub segment: Vec<i32>,
    /// Zeilen von `IndexSet.txt`: Punktnummer, −1 (unbenutzt) oder
    /// Platzhalter −2…−9 für berechnete Punkte.
    pub landmarken: Vec<i64>,
}

fn ungueltig(pfad: &Path, was: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{}: {was}", pfad.display()))
}

impl Messdaten {
    /// Liest `BaseMesh.be`, `range_list.rlist` und `IndexSet.txt` aus CLOs
    /// Resizing-Ordner (`formung.md` 2.2–2.4).
    pub fn laden(ordner: &Path) -> io::Result<Messdaten> {
        Ok(Messdaten {
            kanten: lies_kanten(&ordner.join("BaseMesh.be"))?,
            segment: lies_zahlen(&ordner.join("range_list.rlist"))?
                .into_iter()
                .map(|z| z as i32)
                .collect(),
            landmarken: lies_zahlen(&ordner.join("IndexSet.txt"))?,
        })
    }

    /// Kanten aus den Dreiecken eines Netzes, alle Punkte im selben Segment.
    ///
    /// Für Netze ohne CLO-Dateien, etwa Zylinder in den Prüfungen. Jede Kante
    /// einmal, in der Reihenfolge ihres ersten Auftretens.
    pub fn aus_netz(netz: &Netz, seg: i32) -> Messdaten {
        let mut gesehen = std::collections::HashSet::new();
        let mut kanten = Vec::new();
        for d in &netz.dreiecke {
            for e in 0..3 {
                let (a, b) = (d[e] as u32, d[(e + 1) % 3] as u32);
                let k = [a.min(b), a.max(b)];
                if gesehen.insert(k) {
                    kanten.push(k);
                }
            }
        }
        Messdaten {
            kanten,
            segment: vec![seg; netz.punkte.len()],
            landmarken: Vec::new(),
        }
    }

    /// Punktnummer der Landmarke `L[zeile]`, wenn sie ein Netzpunkt ist.
    pub fn landmarke(&self, zeile: usize) -> Option<usize> {
        match self.landmarken.get(zeile) {
            Some(&v) if v >= 0 => Some(v as usize),
            _ => None,
        }
    }

    pub(crate) fn pruefe(&self, punkte: usize) -> Result<(), String> {
        if self.segment.len() != punkte {
            return Err(format!(
                "{} Segmente für {punkte} Punkte",
                self.segment.len()
            ));
        }
        if let Some(k) = self.kanten.iter().find(|k| k[1] as usize >= punkte) {
            return Err(format!("Kante {k:?} ausserhalb von {punkte} Punkten"));
        }
        Ok(())
    }
}

/// `BaseMesh.be`: `int32 n`, dann `n` int32, je zwei eine Kante.
fn lies_kanten(pfad: &Path) -> io::Result<Vec<[u32; 2]>> {
    let b = fs::read(pfad)?;
    if b.len() < 4 {
        return Err(ungueltig(pfad, "zu kurz".into()));
    }
    let n = i32::from_le_bytes(b[0..4].try_into().unwrap());
    if n < 0 || n % 2 != 0 || b.len() != 4 + 4 * n as usize {
        return Err(ungueltig(pfad, format!("{n} Zahlen bei {} Byte", b.len())));
    }
    let zahl = |i: usize| i32::from_le_bytes(b[4 + 4 * i..8 + 4 * i].try_into().unwrap());
    let mut kanten = Vec::with_capacity(n as usize / 2);
    for k in 0..n as usize / 2 {
        let (a, c) = (zahl(2 * k), zahl(2 * k + 1));
        if a < 0 || c < 0 {
            return Err(ungueltig(pfad, format!("Kante {k}: ({a}, {c})")));
        }
        kanten.push([a as u32, c as u32]);
    }
    Ok(kanten)
}

/// Ganzzahlen, durch Leerraum getrennt (`.rlist`, `IndexSet.txt`).
fn lies_zahlen(pfad: &Path) -> io::Result<Vec<i64>> {
    fs::read_to_string(pfad)?
        .split_whitespace()
        .map(|t| {
            t.parse::<i64>()
                .map_err(|e| ungueltig(pfad, format!("{t:?}: {e}")))
        })
        .collect()
}

/// Punkte eines Netzes in Zentimetern; `pro_cm` = Netzeinheiten je cm.
pub fn punkte_cm(netz: &Netz, pro_cm: f64) -> Vec<Punkt3> {
    netz.punkte
        .iter()
        .map(|p| [p[0] / pro_cm, p[1] / pro_cm, p[2] / pro_cm])
        .collect()
}

/// Punkte in Zentimetern, zentriert wie CLO (`formung.md` 8): x und z auf
/// Mittelwert 0 über alle Punkte, der tiefste Punkt auf y = 0.
///
/// Nicht symmetrisiert; das gehört zur Formung, nicht zur Messung.
pub fn zentriert_cm(netz: &Netz, pro_cm: f64) -> Vec<Punkt3> {
    let mut v = punkte_cm(netz, pro_cm);
    if v.is_empty() {
        return v;
    }
    let n = v.len() as f64;
    let mx = v.iter().map(|p| p[0]).sum::<f64>() / n;
    let mz = v.iter().map(|p| p[2]).sum::<f64>() / n;
    let my = v.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    for p in &mut v {
        p[0] -= mx;
        p[1] -= my;
        p[2] -= mz;
    }
    v
}

pub(crate) fn sub(a: Punkt3, b: Punkt3) -> Punkt3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: Punkt3, b: Punkt3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn kreuz(a: Punkt3, b: Punkt3) -> Punkt3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn betrag(a: Punkt3) -> f64 {
    dot(a, a).sqrt()
}

pub(crate) fn abstand(a: Punkt3, b: Punkt3) -> f64 {
    betrag(sub(a, b))
}

pub(crate) fn normiert(a: Punkt3) -> Option<Punkt3> {
    let l = betrag(a);
    (l > 0.0).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// Orientierungstest O_n(a, b, c) (1.1): n · ((c − b) × (a − b)) ≥ 0.
///
/// Entartet (zwei Punkte gleich, alle drei auf einer Geraden) ist er falsch.
pub(crate) fn orientierung(n: Punkt3, a: Punkt3, b: Punkt3, c: Punkt3) -> bool {
    let (Some(u), Some(w)) = (normiert(sub(c, b)), normiert(sub(a, b))) else {
        return false;
    };
    match normiert(kreuz(u, w)) {
        Some(k) => dot(n, k) >= 0.0,
        None => false,
    }
}

/// Edge cut K(P0, n, seg) (1.2) in `f64`, points in edge order.
///
/// Rule per `messung.md` 1.2: an edge counts when `d_a · d_b ≤ 0` (it touches
/// or crosses the plane).
///
/// This is the path of the older measurement. The reference measurement uses
/// [`kantenschnitt_f32`], because it cuts the plane at nodes that define it
/// themselves — see there.
///
/// Kanten, die ganz in der Ebene liegen, fallen weg; ihr Punkt kommt über
/// die Nachbarkanten in die Liste.
pub(crate) fn kantenschnitt(v: &[Punkt3], d: &Messdaten, p0: Punkt3, n: Punkt3, seg: i32) -> Vec<Punkt3> {
    let mut q: Vec<Punkt3> = Vec::new();
    for &[a, b] in &d.kanten {
        let (a, b) = (a as usize, b as usize);
        if seg != segment::ALLE && (d.segment[a] != seg || d.segment[b] != seg) {
            continue;
        }
        let (va, vb) = (v[a], v[b]);
        let da = dot(n, sub(va, p0));
        let db = dot(n, sub(vb, p0));
        if da * db > 0.0 {
            continue;
        }
        let Some(u) = normiert(sub(vb, va)) else {
            continue;
        };
        let nu = dot(n, u);
        if nu == 0.0 {
            continue;
        }
        let t = -da / nu;
        q.push([va[0] + u[0] * t, va[1] + u[1] * t, va[2] + u[2] * t]);
    }
    q
}

/// Edge cut K(P0, n, seg) (1.2) **in `f32`, the way CLO computes it**.
///
/// The mesh is stored as `f32`, and CLO computes it that way too: plane
/// distance, side decision, edge direction, parameter `t` and intersection
/// point all arise in single precision.
///
/// Why this is needed: for several measurements the plane passes through the
/// mesh points it is meant to measure (neck circumference through L1, L2, L3;
/// upper arm, elbow and wrist through their three points). There the distance
/// is mathematically 0 — in `f64` that is rounding noise of about 10⁻¹⁷ cm
/// whose **sign** depends on the summation order (scalar or vector, with or
/// without FMA). A node with `d = 0` then lies on both sides; its edges yield
/// intersections at `t = 0`, i.e. at the node itself, and these displace a
/// **real** neighbouring point in the greedy 0.5 cm filter depending on the
/// order (measured 0.08 mm at the neck circumference and at the upper arm).
/// In `f32` there is no tie: the defining points lie about 2.4·10⁻⁷ cm beside
/// the plane, always on the same side. That makes the result independent of
/// the order and matches CLO on all three reference meshes to 0.036 / 0.0003 /
/// 0.0001 mm.
///
/// A tolerance on the `f64` distance does not replace this: it flips nodes
/// just beside the plane onto the wrong side (likewise 0.08 mm).
///
/// The **plane** must be built from `f32` points as well, otherwise a residual
/// of 0.04 mm remains ([`crate::messung_referenz::normale_f32`]).
pub(crate) fn kantenschnitt_f32(
    v: &[Punkt3],
    d: &Messdaten,
    p0: Punkt3,
    n: Punkt3,
    seg: i32,
) -> Vec<Punkt3> {
    let p0 = [p0[0] as f32, p0[1] as f32, p0[2] as f32];
    let n = [n[0] as f32, n[1] as f32, n[2] as f32];
    let mut q: Vec<Punkt3> = Vec::new();
    for &[a, b] in &d.kanten {
        let (a, b) = (a as usize, b as usize);
        if seg != segment::ALLE && (d.segment[a] != seg || d.segment[b] != seg) {
            continue;
        }
        let va = [v[a][0] as f32, v[a][1] as f32, v[a][2] as f32];
        let vb = [v[b][0] as f32, v[b][1] as f32, v[b][2] as f32];
        let da = abstand_f32(va, p0, n);
        let db = abstand_f32(vb, p0, n);
        if da * db > 0.0 {
            continue;
        }
        let u = [vb[0] - va[0], vb[1] - va[1], vb[2] - va[2]];
        let lu = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        if lu == 0.0 {
            continue;
        }
        let u = [u[0] / lu, u[1] / lu, u[2] / lu];
        let nu = u[0] * n[0] + u[1] * n[1] + u[2] * n[2];
        if nu == 0.0 {
            continue;
        }
        let t = -da / nu;
        q.push([
            (va[0] + u[0] * t) as f64,
            (va[1] + u[1] * t) as f64,
            (va[2] + u[2] * t) as f64,
        ]);
    }
    q
}

/// Ebenendistanz `n · (V − P0)` in `f32`, wie CLO sie rechnet.
fn abstand_f32(punkt: [f32; 3], p0: [f32; 3], n: [f32; 3]) -> f32 {
    let a = [punkt[0] - p0[0], punkt[1] - p0[1], punkt[2] - p0[2]];
    a[0] * n[0] + a[1] * n[1] + a[2] * n[2]
}

/// Ergebnis des Umfangsschnitts S (1.3).
#[derive(Clone, Debug)]
pub struct Schnitt {
    /// Schnittpunkte vor dem Filter.
    pub roh: usize,
    /// Punkte nach dem 0,5-cm-Filter.
    pub gefiltert: usize,
    /// Die Rohkette mit Umlaufsinn.
    pub kette: Vec<Punkt3>,
}

/// Umfangsschnitt S(P0, n, seg) (1.3): Ring mit Umlaufsinn.
pub fn umfangsschnitt(v: &[Punkt3], d: &Messdaten, p0: Punkt3, n: Punkt3, seg: i32) -> Schnitt {
    umfangsschnitt_mit(v, d, p0, n, seg, kantenschnitt)
}

/// Circumference cut S(P0, n, seg) (1.3) with the edge cut of the reference
/// measurement (`f32`, like CLO) - see [`kantenschnitt_f32`].
pub fn umfangsschnitt_f32(v: &[Punkt3], d: &Messdaten, p0: Punkt3, n: Punkt3, seg: i32) -> Schnitt {
    umfangsschnitt_mit(v, d, p0, n, seg, kantenschnitt_f32)
}

fn umfangsschnitt_mit(
    v: &[Punkt3],
    d: &Messdaten,
    p0: Punkt3,
    n: Punkt3,
    seg: i32,
    schnitt: fn(&[Punkt3], &Messdaten, Punkt3, Punkt3, i32) -> Vec<Punkt3>,
) -> Schnitt {
    let q = schnitt(v, d, p0, n, seg);

    let mut behalten: Vec<Punkt3> = Vec::new();
    for p in &q {
        if behalten.iter().all(|b| abstand(*b, *p) >= FILTER) {
            behalten.push(*p);
        }
    }

    let mut start = None;
    let mut best = START_SCHRANKE;
    for (i, p) in behalten.iter().enumerate() {
        let d = abstand(*p, p0);
        if d < best {
            best = d;
            start = Some(i);
        }
    }

    let mut kette = Vec::new();
    if let Some(s) = start {
        let mut vorrat = behalten.clone();
        kette.push(vorrat.remove(s));
        loop {
            let letzt = *kette.last().unwrap();
            let mut wahl = None;
            let mut schranke = KETTENGLIED;
            for (i, p) in vorrat.iter().enumerate() {
                let d = abstand(*p, letzt);
                if d < schranke {
                    schranke = d;
                    wahl = Some(i);
                }
            }
            match wahl {
                Some(i) => kette.push(vorrat.remove(i)),
                None => break,
            }
        }
        let m = kette.len();
        if !orientierung(n, kette[0], kette[m / 3], kette[2 * m / 3]) {
            kette.reverse();
        }
    }

    Schnitt {
        roh: q.len(),
        gefiltert: behalten.len(),
        kette,
    }
}

/// Konvexe Hülle H(k, n) (1.4) und ihre geschlossene Länge.
///
/// Gibt die Hüllpunkte (ohne den angehängten Schlusspunkt) und die Länge
/// zurück. `q[0]` wird nie gestrichen, und es gibt keine zweite Runde über
/// die Nahtstelle; so verhält sich das Verfahren der Spezifikation.
///
/// **Die Richtung der Kette zählt.** Das Verfahren läuft die Kette einmal
/// durch und streicht dabei Ecken; es ist kein echter Hüllenalgorithmus, weil
/// es `q[0]` schont und die Nahtstelle nicht nachfasst. Läuft es gegen den
/// Uhrzeigersinn einer nach aussen gedrehten Fläche, streicht es die falschen
/// Punkte — an einem Rumpfring des MakeHuman-Körpers wurde aus 97,19 cm so
/// 5,29 cm mit drei Hüllpunkten. Die Schnittkette kommt aus einem fremden Netz
/// und hat keine zugesicherte Richtung, deshalb wird hier **beide** Richtungen
/// gerechnet und die längere genommen. Bei CLOs Rumpf ist die alte Richtung
/// ohnehin die längere; dort ändert sich nichts.
pub fn huelle(kette: &[Punkt3], n: Punkt3) -> (Vec<Punkt3>, f64) {
    let (h, l) = huelle_richtung(kette, n);
    let rueck: Vec<Punkt3> = kette.iter().rev().copied().collect();
    let (h2, l2) = huelle_richtung(&rueck, n);
    if l2 > l { (h2, l2) } else { (h, l) }
}

/// Eine Richtung von [`huelle`], ohne Windungsschutz.
fn huelle_richtung(kette: &[Punkt3], n: Punkt3) -> (Vec<Punkt3>, f64) {
    let m = kette.len();
    if m == 0 {
        return (Vec::new(), 0.0);
    }
    let mut q: Vec<Punkt3> = (0..m).map(|i| kette[(i + DREHUNG) % m]).collect();
    let mut i = 2;
    while i < q.len() {
        let (a, b, c) = (q[i - 2], q[i - 1], q[i]);
        if !orientierung(n, a, b, c) || abstand(a, c) < abstand(a, b) {
            q.remove(i - 1);
            i = (i - 2).max(2) + 1;
        } else {
            i += 1;
        }
    }
    let mut laenge = 0.0;
    for t in 0..q.len() {
        laenge += abstand(q[t], q[(t + 1) % q.len()]);
    }
    (q, laenge)
}

/// Ein gemessener Umfang.
#[derive(Clone, Debug)]
pub struct Umfang {
    /// Länge der Hülle, cm.
    pub laenge: f64,
    pub p0: Punkt3,
    pub roh: usize,
    pub gefiltert: usize,
    pub kette: Vec<Punkt3>,
    pub huellpunkte: usize,
}

/// Umfang (Typ 0) durch `p0` mit Normale `n` im Segment `seg` (1.9).
pub fn umfang(v: &[Punkt3], d: &Messdaten, p0: Punkt3, n: Punkt3, seg: i32) -> Umfang {
    let s = umfangsschnitt(v, d, p0, n, seg);
    let (h, laenge) = huelle(&s.kette, n);
    Umfang {
        laenge,
        p0,
        roh: s.roh,
        gefiltert: s.gefiltert,
        kette: s.kette,
        huellpunkte: h.len(),
    }
}

/// Geneigte Ebene durch drei Punkte (1.9, Slot mit drei Indizes):
/// `P0 = V[a]`, `n = normiert((V[b] − V[a]) × (V[c] − V[a]))`.
pub fn ebene_drei(v: &[Punkt3], a: usize, b: usize, c: usize) -> Option<(Punkt3, Punkt3)> {
    let n = normiert(kreuz(sub(v[b], v[a]), sub(v[c], v[a])))?;
    Some((v[a], n))
}

/// Waagerechter Hüllenumfang U(P, seg) (1.5).
pub fn waagerecht(v: &[Punkt3], d: &Messdaten, p: Punkt3, seg: i32) -> f64 {
    let s = umfangsschnitt(v, d, p, WAAGERECHT, seg);
    huelle(&s.kette, WAAGERECHT).1
}

/// Waagerechte konvexe Hülle des Rumpfs auf Höhe `y`, Ebene durch
/// `(0, y, 0)`, mit ihrer Länge.
///
/// Die Hülle selbst wird gebraucht, sobald nicht nur der Umfang zählt:
/// Brustpunktabstand, Teilbögen, Breite und Tiefe (`crate::bogenmasse`).
pub fn rumpf_huelle(v: &[Punkt3], d: &Messdaten, y: f64) -> (Vec<Punkt3>, f64) {
    let s = umfangsschnitt(v, d, [0.0, y, 0.0], WAAGERECHT, segment::RUMPF);
    huelle(&s.kette, WAAGERECHT)
}

/// Waagerechter Hüllenumfang des Rumpfs auf Höhe `y`, Ebene durch
/// `(0, y, 0)` wie beim Abtasten der Abnahme.
pub fn rumpf_waagerecht(v: &[Punkt3], d: &Messdaten, y: f64) -> f64 {
    rumpf_huelle(v, d, y).1
}

/// Die gewählten Kandidaten der fünf Rumpfumfänge (Abschnitt 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kandidaten {
    pub brustpaar: (usize, usize),
    pub unterbrust: usize,
    pub taille: usize,
    pub huefte_oben: usize,
    pub huefte_unten: usize,
}

/// Die fünf Rumpfumfänge wie CLO sie misst (Abschnitt 4, Maße 2, 5, 6, 8, 7).
#[derive(Clone, Debug)]
pub struct Rumpfmasse {
    pub brust: Umfang,
    pub unterbrust: Umfang,
    pub taille: Umfang,
    pub highhip: Umfang,
    pub huefte: Umfang,
    pub kandidaten: Kandidaten,
}

/// Landmarkenzeilen der Kandidatenlisten (Abschnitt 4).
const BRUST: [usize; 10] = [18, 19, 66, 67, 68, 69, 70, 71, 72, 73];
const UNTERBRUST: [usize; 5] = [22, 100, 101, 102, 103];
const TAILLE: [usize; 10] = [24, 74, 75, 76, 77, 78, 79, 80, 81, 82];
const HUEFTE_UNTEN: [usize; 7] = [29, 83, 84, 85, 86, 87, 88];
const HUEFTE_OBEN: [usize; 8] = [26, 89, 90, 91, 92, 93, 94, 95];
/// Zeile des Schrittpunkts (Maß 26, HEIGHT_Crotch).
pub const SCHRITT: usize = 30;

/// Hysterese der Brustpaarwahl, cm (Körpertyp Female).
const BRUST_HYSTERESE: f64 = 0.3;
/// Höchste Steigung Δy/Δz der Unterbrustkette: tan(1,13446).
const UNTERBRUST_STEIGUNG: f64 = 2.144484;

/// Misst die fünf Rumpfumfänge auf zentrierten Punkten (cm).
///
/// Kandidatenwahl wie in Abschnitt 4: Brust am vordersten Punktpaar,
/// Unterbrust dort, wo die Kette steil in die Brust übergeht, Taille am
/// kleinsten, Hüfte am grössten Kandidaten, High Hip nächst der halben Höhe
/// zwischen Taille und Hüfte. Gemessen in der Reihenfolge 2, 5, 6, 8, 7.
pub fn rumpfmasse(v: &[Punkt3], d: &Messdaten) -> Result<Rumpfmasse, String> {
    d.pruefe(v.len())?;
    let l = |zeile: usize| d.landmarke(zeile);
    let liste = |zeilen: &[usize]| -> Vec<Option<usize>> { zeilen.iter().map(|&z| l(z)).collect() };
    let seg = segment::RUMPF;

    // 2 Brust: Paarwahl über fünf Paare.
    let brust_liste = liste(&BRUST);
    let (mut best, mut z_stern) = (0usize, 0.0f64);
    for p in 0..5 {
        let (Some(a), Some(b)) = (brust_liste[2 * p], brust_liste[2 * p + 1]) else {
            continue;
        };
        let w = (v[a][2] + v[b][2]) / 2.0 - BRUST_HYSTERESE;
        if w > z_stern {
            best = p;
            z_stern = w + BRUST_HYSTERESE;
        }
    }
    let (a, b) = match (brust_liste[2 * best], brust_liste[2 * best + 1]) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err("kein Brustpunktpaar in IndexSet".into()),
    };
    let p0 = [
        (v[a][0] + v[b][0]) / 2.0,
        (v[a][1] + v[b][1]) / 2.0,
        (v[a][2] + v[b][2]) / 2.0,
    ];
    let brust = umfang(v, d, p0, WAAGERECHT, seg);

    // 5 Unterbrust: erstes Kettenglied mit flacher Steigung nach vorn oben.
    let ub = liste(&UNTERBRUST);
    let mut q_wahl = 0;
    for q in 0..4 {
        let (Some(e0), Some(e1)) = (ub[q], ub[q + 1]) else {
            continue;
        };
        let delta = sub(v[e1], v[e0]);
        if delta[2] == 0.0 {
            continue;
        }
        let s = delta[1] / delta[2];
        if (0.0..UNTERBRUST_STEIGUNG).contains(&s) {
            q_wahl = q;
            break;
        }
    }
    let u = ub[q_wahl].ok_or("Unterbrustkandidat fehlt in IndexSet")?;
    let unterbrust = umfang(v, d, v[u], WAAGERECHT, seg);

    // 6 Taille: kleinster waagerechter Hüllenumfang.
    let mut w_wahl = None;
    let mut schranke = 1000.0;
    for e in liste(&TAILLE).into_iter().flatten() {
        let u = waagerecht(v, d, v[e], seg);
        if u < schranke {
            schranke = u;
            w_wahl = Some(e);
        }
    }
    let w = w_wahl.ok_or("kein Taillenkandidat")?;
    let taille = umfang(v, d, v[w], WAAGERECHT, seg);

    // 8 Hüfte unten: grösster waagerechter Hüllenumfang (vor 7 gemessen).
    let mut h_wahl = None;
    let mut schranke = 0.0;
    for e in liste(&HUEFTE_UNTEN).into_iter().flatten() {
        let u = waagerecht(v, d, v[e], seg);
        if u > schranke {
            schranke = u;
            h_wahl = Some(e);
        }
    }
    let h = h_wahl.ok_or("kein Hüftkandidat")?;
    let huefte = umfang(v, d, v[h], WAAGERECHT, seg);

    // 7 Hüfte oben: nächst der halben Höhe zwischen Hüfte und Taille.
    let y_stern = (v[h][1] + v[w][1]) / 2.0;
    let mut hh_wahl = None;
    let mut schranke = 1000.0;
    for e in liste(&HUEFTE_OBEN).into_iter().flatten() {
        let a = (y_stern - v[e][1]).abs();
        if a < schranke {
            schranke = a;
            hh_wahl = Some(e);
        }
    }
    let hh = hh_wahl.ok_or("kein Kandidat für die obere Hüfte")?;
    let highhip = umfang(v, d, v[hh], WAAGERECHT, seg);

    Ok(Rumpfmasse {
        brust,
        unterbrust,
        taille,
        highhip,
        huefte,
        kandidaten: Kandidaten {
            brustpaar: (a, b),
            unterbrust: u,
            taille: w,
            huefte_oben: hh,
            huefte_unten: h,
        },
    })
}

/// Ein Wert der Abtastung: Höhe und Hüllenumfang, cm.
#[derive(Clone, Copy, Debug)]
pub struct Abgetastet {
    pub y: f64,
    pub umfang: f64,
}

/// Schrittweite der Abtastung, cm.
pub const ABTAST_SCHRITT: f64 = 0.25;

/// Kleinster bzw. grösster waagerechter Hüllenumfang des Rumpfs zwischen
/// `von` und `bis` (cm), abgetastet von unten in Schritten von 0,25 cm,
/// `bis` eingeschlossen. Ebene durch `(0, y, 0)`.
pub fn abtasten(v: &[Punkt3], d: &Messdaten, von: f64, bis: f64, groesster: bool) -> Option<Abgetastet> {
    let mut best: Option<Abgetastet> = None;
    let mut k = 0usize;
    loop {
        let y = von + k as f64 * ABTAST_SCHRITT;
        if y > bis + 1.0e-9 {
            break;
        }
        let u = rumpf_waagerecht(v, d, y);
        if u > 0.0 {
            let besser = match best {
                None => true,
                Some(b) => {
                    if groesster {
                        u > b.umfang
                    } else {
                        u < b.umfang
                    }
                }
            };
            if besser {
                best = Some(Abgetastet { y, umfang: u });
            }
        }
        k += 1;
    }
    best
}

/// Die Abnahmewerte nach `massdefinitionen.md`: Brust an CLOs Brustpunkten,
/// Taille als kleinster Hüllenumfang zwischen Hüft- und Unterbrustpunkt,
/// Hüfte als grösster zwischen Schritthöhe + 0,5 cm und Taillenpunkt.
#[derive(Clone, Debug)]
pub struct Abnahme {
    pub brust: f64,
    pub taille: Option<Abgetastet>,
    pub huefte: Option<Abgetastet>,
}

pub fn abnahme(v: &[Punkt3], d: &Messdaten, r: &Rumpfmasse) -> Result<Abnahme, String> {
    let k = &r.kandidaten;
    let schritt = d.landmarke(SCHRITT).ok_or("Schrittpunkt fehlt in IndexSet")?;
    let taille = abtasten(v, d, v[k.huefte_unten][1], v[k.unterbrust][1], false);
    let huefte = abtasten(v, d, v[schritt][1] + 0.5, v[k.taille][1], true);
    Ok(Abnahme {
        brust: r.brust.laenge,
        taille,
        huefte,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zylinder in Zentimetern mit Kanten aus seinen Dreiecken.
    fn zylinder(r: f64, n: usize) -> (Vec<Punkt3>, Messdaten) {
        let netz = Netz::zylinder(r, 100.0, n, 4);
        let d = Messdaten::aus_netz(&netz, segment::RUMPF);
        (netz.punkte, d)
    }

    fn vieleck(r: f64, n: usize) -> f64 {
        n as f64 * 2.0 * r * (std::f64::consts::PI / n as f64).sin()
    }

    /// The synthetic meshes do not hit the ideal down to the last bit: the
    /// edge cut computes in `f32` (like CLO, see [`kantenschnitt`]), which
    /// costs about 1·10⁻⁶ cm.
    const SYNTH: f64 = 1e-5;

    /// Ein konvexer Querschnitt bleibt ganz: Hülle = Vieleck.
    #[test]
    fn konvexer_zylinder_misst_sein_vieleck() {
        // 36 Ecken: Seiten 5,2 cm, also über dem 0,5-cm-Filter.
        let (v, d) = zylinder(30.0, 36);
        let u = umfang(&v, &d, [0.0, 50.0, 0.0], WAAGERECHT, segment::RUMPF);
        assert_eq!(u.gefiltert, 36);
        assert!((u.laenge - vieleck(30.0, 36)).abs() < SYNTH, "{}", u.laenge);
    }

    /// Eine Delle wird überbrückt, wie vom Maßband.
    #[test]
    fn delle_wird_ueberbrueckt() {
        let n = 36;
        let mut netz = Netz::zylinder(30.0, 100.0, n, 4);
        // Ein Ringpunkt jeder Ebene nach innen gedrückt.
        for (i, p) in netz.punkte.iter_mut().enumerate() {
            if i % n == 0 {
                p[0] *= 0.7;
                p[2] *= 0.7;
            }
        }
        let d = Messdaten::aus_netz(&netz, segment::RUMPF);
        let u = umfang(&netz.punkte, &d, [0.0, 50.0, 0.0], WAAGERECHT, segment::RUMPF);
        let soll = vieleck(30.0, n) - 2.0 * 2.0 * 30.0 * (std::f64::consts::PI / n as f64).sin()
            + 2.0 * 30.0 * (2.0 * std::f64::consts::PI / n as f64).sin();
        assert!((u.laenge - soll).abs() < SYNTH, "{} statt {soll}", u.laenge);
        assert_eq!(u.huellpunkte, n - 1);
    }

    /// Nur das gewählte Segment zählt; ein Arm daneben fällt weg.
    #[test]
    fn fremdes_segment_faellt_weg() {
        let rumpf = Netz::zylinder(30.0, 100.0, 36, 4);
        let arm = Netz::zylinder(5.0, 100.0, 12, 4);
        let mut netz = rumpf.clone();
        let basis = netz.punkte.len();
        for p in &arm.punkte {
            netz.punkte.push([p[0] + 45.0, p[1], p[2]]);
        }
        for t in &arm.dreiecke {
            netz.dreiecke.push([t[0] + basis, t[1] + basis, t[2] + basis]);
        }
        let mut d = Messdaten::aus_netz(&netz, segment::RUMPF);
        for s in d.segment.iter_mut().skip(basis) {
            *s = segment::ARM_LINKS;
        }
        let u = umfang(&netz.punkte, &d, [0.0, 50.0, 0.0], WAAGERECHT, segment::RUMPF);
        assert!((u.laenge - vieleck(30.0, 36)).abs() < SYNTH, "{}", u.laenge);
    }

    /// Eine offene Naht ändert die Hülle nicht: Die Kette folgt dem nächsten
    /// Nachbarn, nicht der Topologie. Hier ist der Zylinder entlang einer
    /// Spalte aufgetrennt (eigene Punkte für die letzte Spalte, wie an den
    /// Nähten des BaseMesh, Befund 25), geschnitten zwischen zwei Ringen.
    #[test]
    fn offene_naht_stoert_nicht() {
        let n = 36;
        let voll = Netz::zylinder(30.0, 100.0, n, 4);
        let mut offen = voll.clone();
        let mut doppel = std::collections::HashMap::new();
        for t in offen.dreiecke.iter_mut() {
            // Dreiecke der letzten Spalte greifen auf Spalte 0 zu.
            let letzte = t.iter().any(|&i| i % n == n - 1);
            if !letzte {
                continue;
            }
            for i in t.iter_mut() {
                if *i % n == 0 {
                    let p = voll.punkte[*i];
                    *i = *doppel.entry(*i).or_insert_with(|| {
                        offen.punkte.push(p);
                        offen.punkte.len() - 1
                    });
                }
            }
        }
        assert!(!doppel.is_empty());
        let d = Messdaten::aus_netz(&offen, segment::RUMPF);
        let u = umfang(&offen.punkte, &d, [0.0, 40.0, 0.0], WAAGERECHT, segment::RUMPF);
        assert!((u.laenge - vieleck(30.0, n)).abs() < SYNTH, "{}", u.laenge);
    }

    #[test]
    fn entarteter_orientierungstest_ist_falsch() {
        let a = [0.0, 0.0, 0.0];
        assert!(!orientierung(WAAGERECHT, a, a, [1.0, 0.0, 0.0]));
        assert!(!orientierung(WAAGERECHT, a, [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]));
    }
}
