//! Der Prüfkörper und das Prüfband um die Taille: Körper aus
//! `klon3d koerper` mit seinen Hüllkörpern, Rumpfschnitt wie ein Maßband,
//! Anfangslage, Auswertung. Geteilt von den Tests und Beispielen in
//! `klon3d-sim`.

#![allow(dead_code)]

use std::f64::consts::PI;
use std::path::PathBuf;

use klon3d_sim::netzkoerper::{clo_nach_szene, SUCHTIEFE};
use klon3d_sim::platzierung::{Huellen, Koerperteile};
pub use klon3d_sim::platzierung::{auf_polygon, huelle, umfang};
use klon3d_sim::roehre::{param_ruhe, rohr_netz};
use klon3d_sim::{messung, Koerper, Koerpernetz, Material, Simparameter, Szene, Teilvorgabe};

/// Der Körper der Prüfmuster-Maßdatei, im Szenenrahmen.
pub fn koerper() -> Option<Koerpernetz> {
    let smis_pfad = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster/measurements.smis"));
    let smis = klon3d_pattern::lies_smis(&smis_pfad).ok()?;
    let ordner = klon3d_koerper::koerper::ordner();
    match klon3d_koerper::koerper::aus_smis(&ordner, &|n| smis.wert(n)) {
        Ok(k) => Some(Koerpernetz::aus_clo_mm(&k.netz.punkte, &k.netz.dreiecke, SUCHTIEFE).expect("Körpernetz")),
        Err(e) => {
            eprintln!("übersprungen: Körper nicht formbar aus {}: {e}", ordner.display());
            None
        }
    }
}

/// Der Körper der Prüfmuster-Maßdatei mit seinen Hüllkörpern.
pub fn pruefkoerper() -> Option<(Koerpernetz, Huellen)> {
    pruefkoerper_aus(&PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster/measurements.smis")))
}

/// Wie [`pruefkoerper`], aus einer anderen Maßdatei.
pub fn pruefkoerper_aus(smis_pfad: &std::path::Path) -> Option<(Koerpernetz, Huellen)> {
    let smis = klon3d_pattern::lies_smis(smis_pfad).ok()?;
    let ordner = klon3d_koerper::koerper::ordner();
    let k = match klon3d_koerper::koerper::aus_smis(&ordner, &|n| smis.wert(n)) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("übersprungen: Körper nicht formbar aus {}: {e}", ordner.display());
            return None;
        }
    };
    let a = match klon3d_koerper::koerper::anordnung(&ordner, &k.netz) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("übersprungen: {e}");
            return None;
        }
    };
    let netz = Koerpernetz::aus_clo_mm(&k.netz.punkte, &k.netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
    let c = clo_nach_szene;
    let h = Huellen::neu(&Koerperteile {
        punkte: &netz.punkte,
        dreiecke: &netz.dreiecke,
        rumpf: &a.rumpf,
        arme: [&a.arme[0], &a.arme[1]],
        hals_vorn: c(a.hals_vorn),
        hals_hinten: c(a.hals_hinten),
        hals_seitlich: a.hals_seitlich.map(c),
        schulter: a.schulter.map(c),
        ellbogen: a.ellbogen.map(c),
    })
    .expect("Hüllkörper");
    Some((netz, h))
}

/// Schnitt des Netzes mit der Ebene z = h: die Kantenpunkte (x, y).
pub fn schnitt(k: &Koerpernetz, h: f64) -> Vec<[f64; 2]> {
    let mut v = Vec::new();
    for d in &k.dreiecke {
        for e in 0..3 {
            let (a, b) = (k.punkte[d[e] as usize], k.punkte[d[(e + 1) % 3] as usize]);
            if (a[2] > h) != (b[2] > h) {
                let u = (h - a[2]) / (b[2] - a[2]);
                v.push([a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1])]);
            }
        }
    }
    v
}

/// Nur der Rumpf: Arme liegen jenseits der größten Lücke in |x|.
pub fn rumpf(v: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut ax: Vec<f64> = v.iter().map(|p| p[0].abs()).collect();
    ax.sort_by(f64::total_cmp);
    let (mut luecke, mut grenze) = (0.0, f64::INFINITY);
    for w in ax.windows(2) {
        if w[1] - w[0] > luecke {
            luecke = w[1] - w[0];
            grenze = 0.5 * (w[0] + w[1]);
        }
    }
    if luecke < 0.02 {
        return v;
    }
    v.into_iter().filter(|p| p[0].abs() < grenze).collect()
}

/// Umfang des Rumpfs wie ein Maßband: konvexe Hülle des Schnitts.
pub fn rumpf_umfang(k: &Koerpernetz, h: f64) -> f64 {
    umfang(&huelle(rumpf(schnitt(k, h))))
}


/// Eine Bandkante gegen den Körper.
#[derive(Debug, Clone, Copy)]
pub struct Kante {
    /// Länge der Kante als geschlossene Schleife (m).
    pub laenge: f64,
    /// Rumpfumfang wie ein Maßband in der Ebene der Kante + π·t (m).
    pub soll: f64,
    /// Neigung der Ebene gegen die Waagerechte (Grad).
    pub neigung_grad: f64,
    /// Größte Abweichung der Kante von ihrer Ebene (m).
    pub uneben: f64,
}

/// Ergebnis des Bands.
#[derive(Debug)]
pub struct Band {
    pub taille_z: f64,
    pub mitte_z: f64,
    /// obere und untere Kante
    pub kanten: [Kante; 2],
    /// kleinster und mittlerer Abstand Stoffmitte − t/2 zur Haut (m)
    pub tiefster: f64,
    pub median_luecke: f64,
    /// Anteil der Punkte näher als 0,5 mm an der Haut
    pub anteil_anliegend: f64,
    pub nahtspalt: f64,
}

/// Konvexes Polygon um `abstand` nach außen versetzt, an jeder Ecke entlang
/// der gemittelten Kantennormalen (so ist das Band in Befund 94 gelegt; die
/// Bibliothek versetzt genau, auf Gehrung — das Radical-Band mit 10 % ändert
/// dann seine Kantenlänge um 0,5 %).
pub fn versetzt_mittel(h: &[[f64; 2]], abstand: f64) -> Vec<[f64; 2]> {
    let n = h.len();
    let normale = |a: [f64; 2], b: [f64; 2]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        [dy / l, -dx / l]
    };
    (0..n)
        .map(|i| {
            let (v, p, w) = (h[(i + n - 1) % n], h[i], h[(i + 1) % n]);
            let (n1, n2) = (normale(v, p), normale(p, w));
            let m = [n1[0] + n2[0], n1[1] + n2[1]];
            let l = (m[0] * m[0] + m[1] * m[1]).sqrt();
            [p[0] + abstand * m[0] / l, p[1] + abstand * m[1] / l]
        })
        .collect()
}

/// Bandhöhe (m), Netzkante (mm), Anfangsabstand über der Haut (m). Bei 5 mm
/// faltet sich das Radical-Band quer (siehe `band_faltet_und_bleibt_draussen`).
pub const HOEHE: f64 = 0.02;
pub const KANTE_MM: f64 = 10.0;
pub const ANFANG: f64 = 0.005;

/// Taille: schmalste Stelle des Rumpfs zwischen 0,90 und 1,10 m.
pub fn taille(k: &Koerpernetz) -> f64 {
    (0..=80)
        .map(|i| 0.90 + 0.0025 * i as f64)
        .min_by(|&a, &b| rumpf_umfang(k, a).total_cmp(&rumpf_umfang(k, b)))
        .unwrap()
}

/// Das Band in der Anfangslage um die Taille `taille_z`: Breite = Rumpfumfang
/// dort + π·t, gelegt auf die Hülle aller Schnitte über die Bandhöhe, um
/// t/2 + 5 mm versetzt; Ruhebedingungen ohne Schwerkraft. Dazu die
/// Musterkoordinaten der Punkte (mm) in der Reihenfolge der Szene.
pub fn band_szene(k: &Koerpernetz, taille_z: f64, material: Material, reduktion: f64, kante_mm: f64) -> (Szene, Vec<[f64; 2]>) {
    let t = material.dicke_m;
    let breite = rumpf_umfang(k, taille_z) + PI * t;
    let mut alle = Vec::new();
    for i in 0..=6 {
        alle.extend(rumpf(schnitt(k, taille_z - HOEHE / 2.0 - 0.002 + (HOEHE + 0.004) * i as f64 / 6.0)));
    }
    let start = versetzt_mittel(&huelle(alle), t / 2.0 + ANFANG);
    let ganz = umfang(&start);
    let netz = rohr_netz(breite, HOEHE, kante_mm).unwrap();
    let vg = Teilvorgabe { material, reduktion };
    let lage = |_: usize, q: [f64; 2]| {
        let p = auf_polygon(&start, q[0] * 1e-3 / breite * ganz);
        [p[0], p[1], taille_z + HOEHE / 2.0 - q[1] * 1e-3]
    };
    let s = Szene::aus_netz(&netz, &|_| vg, &lage, vec![Koerper::netz(k.clone())], param_ruhe());
    (s, netz.teile[0].punkte.clone())
}

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn kreuz3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn skalar3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normiert3(a: [f64; 3]) -> [f64; 3] {
    let l = skalar3(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Schnitt mit der Ebene durch `p0` mit Normale `n`: Kantenpunkte in den
/// Ebenenkoordinaten (e₁ ≈ x, e₂ = n × e₁).
pub fn ebenen_schnitt(k: &Koerpernetz, p0: [f64; 3], n: [f64; 3]) -> Vec<[f64; 2]> {
    let e1 = normiert3(sub3([1.0, 0.0, 0.0], [n[0] * n[0], n[0] * n[1], n[0] * n[2]]));
    let e2 = kreuz3(n, e1);
    let h = |p: [f64; 3]| skalar3(sub3(p, p0), n);
    let mut v = Vec::new();
    for d in &k.dreiecke {
        for e in 0..3 {
            let (a, b) = (k.punkte[d[e] as usize], k.punkte[d[(e + 1) % 3] as usize]);
            let (ha, hb) = (h(a), h(b));
            if (ha > 0.0) != (hb > 0.0) {
                let u = ha / (ha - hb);
                let q = sub3([a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1]), a[2] + u * (b[2] - a[2])], p0);
                v.push([skalar3(q, e1), skalar3(q, e2)]);
            }
        }
    }
    v
}

/// Eine Bandkante (Musterkoordinate y = `y_mm`) als Schleife, gemessen gegen
/// den Rumpfumfang in ihrer Ebene (Newell-Normale der Schleife).
pub fn kante(k: &Koerpernetz, s: &Szene, muster: &[[f64; 2]], y_mm: f64) -> Kante {
    let t = s.kollisionsabstand[0] * 2.0;
    let mut idx: Vec<usize> = (0..muster.len()).filter(|&i| (muster[i][1] - y_mm).abs() < 1e-6).collect();
    idx.sort_by(|&a, &b| muster[a][0].total_cmp(&muster[b][0]));
    let p: Vec<[f64; 3]> = idx.iter().map(|&i| s.x[i]).collect();
    let n = p.len();
    let laenge: f64 = (0..n).map(|i| skalar3(sub3(p[(i + 1) % n], p[i]), sub3(p[(i + 1) % n], p[i])).sqrt()).sum();
    let mut c = [0.0; 3];
    for q in &p {
        for a in 0..3 {
            c[a] += q[a] / n as f64;
        }
    }
    let mut nv = [0.0; 3];
    for i in 0..n {
        let w = kreuz3(sub3(p[i], c), sub3(p[(i + 1) % n], c));
        for a in 0..3 {
            nv[a] += w[a];
        }
    }
    let nv = normiert3(nv);
    let uneben = p.iter().map(|&q| skalar3(sub3(q, c), nv).abs()).fold(0.0, f64::max);
    let soll = umfang(&huelle(rumpf(ebenen_schnitt(k, c, nv)))) + PI * t;
    Kante { laenge, soll, neigung_grad: nv[2].abs().min(1.0).acos().to_degrees(), uneben }
}

/// Messwerte des Bands; der Abstand zur Haut über alle Dreiecke gemessen.
pub fn band_auswerten(k: &Koerpernetz, s: &Szene, muster: &[[f64; 2]], taille_z: f64) -> Band {
    let t = s.kollisionsabstand[0] * 2.0;
    let mitte_z = s.x.iter().map(|p| p[2]).sum::<f64>() / s.x.len() as f64;
    let mut luecke: Vec<f64> = s.x.iter().map(|&p| k.naechster_alle(p).abstand - t / 2.0).collect();
    luecke.sort_by(f64::total_cmp);
    let anliegend = luecke.iter().filter(|&&d| d < 0.5e-3).count() as f64 / luecke.len() as f64;
    Band {
        taille_z,
        mitte_z,
        kanten: [kante(k, s, muster, 0.0), kante(k, s, muster, HOEHE * 1e3)],
        tiefster: luecke[0],
        median_luecke: luecke[luecke.len() / 2],
        anteil_anliegend: anliegend,
        nahtspalt: messung::nahtspalt(s),
    }
}

/// Lose Punkte (ohne Constraints) auf einem 1-cm-Raster über dem Körper,
/// 1,75 m hoch, mit Schwerkraft und Reibung 0,6.
pub fn punkteregen(k: &Koerpernetz) -> Szene {
    let mut x = Vec::new();
    for i in 0..89 {
        for j in 0..34 {
            x.push([-0.44 + 0.01 * i as f64, -0.18 + 0.01 * j as f64, 1.75]);
        }
    }
    let n = x.len();
    let masse = 1e-5;
    Szene {
        v: vec![[0.0; 3]; n],
        masse: vec![masse; n],
        w: vec![1.0 / masse; n],
        kollisionsabstand: vec![0.2e-3; n],
        reibung: vec![0.6; n],
        abstand: Default::default(),
        naht: Default::default(),
        biegung: Default::default(),
        biegung_grund: Vec::new(),
        dreiecke: Vec::new(),
        ruhe_inv: Vec::new(),
        faser: Vec::new(),
        faser_ersatz: 0,
        faser_entartet: 0,
        teile: Vec::new(),
        klebungen: Vec::new(),
        naht_biegung_ohne: 0,
        einhalte_kanten: 0,
        koerper: vec![Koerper::netz(k.clone())],
        param: Simparameter { schwerkraft: [0.0, 0.0, -9.81], daempfung: 0.0, ..Simparameter::default() },
        zeit: 0.0,
        x,
    }
}
