//! Selbstkollision Stoff gegen Stoff nach dem Verfahren von CLO
//! (Befund 116, am laufenden Programm gemessen).
//!
//! CLO hält den Stoff in drei Stufen von sich selbst fern:
//!
//! 1. **Nähe** (vor dem Löser): Paare Punkt–Dreieck und Kante–Kante, die
//!    näher als die Summe ihrer Kollisionsdicken liegen, bekommen eine
//!    Abstoßung samt Jacobi-Blöcken ins implizite System. Gemessen am
//!    Schuluniform-Projekt: je Schritt rund 27.500 Punkt–Dreieck- und
//!    80.000 Kante–Kante-Paare.
//! 2. **Durchdringung** (vor dem Löser): Kanten, die schon durch ein
//!    Dreieck stechen, werden zu zusammenhängenden Gebieten verbunden und je
//!    Gebiet in eine gemeinsame Richtung zurückgeschoben.
//! 3. **Stoß** (nach dem Löser): was trotzdem durchkommt, wird auf dem Weg
//!    vom alten zum neuen Ort gefunden und zurückgesetzt. Am Projekt je
//!    Schritt rund 150 Punkt–Dreieck- und 400 Kante–Kante-Stöße, also an etwa
//!    0,1 % der Dreiecke.
//!
//! **Dicke.** Je Punkt gilt der Radius r = (Stoffdicke + Zusatzdicke
//! Kollision)/2 — gemessen 1,45/1,50/1,65 mm bei Stoffen von 0,4/0,5/0,8 mm
//! und CLOs Vorgabe 2,5 mm Zusatzdicke an allen Teilen. Zwei Lagen ruhen also
//! um r₁ + r₂ (= Stoffdicke + Zusatzdicke) auseinander, dazu ein Zuschlag von
//! 0,04 mm.
//!
//! **Ausnahme: ein Blatt mit sich selbst.** Das gefaltete Zugabeband ist nicht
//! die zweite Lage, sondern die Rückseite desselben Teils ([`crate::saumfalte`]).
//! Für ein Paar Band auf eigenem Träger gilt als Ruheabstand die Summe der
//! **Stoffdicken** (2 × 0,2 mm), nicht der Kissenabstand mit der Zusatzdicke
//! (2,9 mm) — die Zusatzdicke ist ein Rechenpolster gegen Durchdringen, keine
//! Geometrie, und würde das aufliegende Band sichtbar abheben. Das gilt für die
//! Punkt–Dreieck- wie für die Kante–Kante-Suche. Ebenso zählt eine Feder der
//! Ruhelänge 0 als Schweißung wie eine Naht: zwei Punkte am selben Ort sind eine
//! Stelle, keine zwei Lagen.
//!
//! Dieses Modul ist ein eigener Nachbau in Rust auf der CPU (`f64`, SI), kein
//! übertragener Code.

use crate::koerper::{add, cross, dot, mul, norm, sub};

/// Grundzuschlag der Hüllquader (CLO: 0,02 mm).
pub const ZUSCHLAG_QUADER: f64 = 2e-5;
/// Zuschlag auf die Summe der Dicken (CLO: 0,04 mm).
pub const ZUSCHLAG_PAAR: f64 = 4e-5;
/// Kürzere Kanten nimmt die Kante–Kante-Suche nicht (CLO: 0,1 mm).
pub const KANTE_MIN: f64 = 1e-4;

/// Art eines Nähe-Paars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Punkt `punkte[0]` gegen Dreieck `punkte[1..4]`.
    PunktDreieck,
    /// Kante `punkte[0..2]` gegen Kante `punkte[2..4]`.
    KanteKante,
}

/// Ein Paar in Nähe: vier Punkte mit Gewichten, so dass
/// `Σ gewichte[k]·x[punkte[k]]` der Abstandsvektor von der zweiten zur ersten
/// Seite ist (Punkt minus Dreieckspunkt bzw. Punkt auf Kante 1 minus Punkt auf
/// Kante 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Naehe {
    pub art: Art,
    pub punkte: [u32; 4],
    pub gewichte: [f64; 4],
    /// Einheitsrichtung von der zweiten zur ersten Seite.
    pub normale: [f64; 3],
    pub abstand: f64,
    /// Ruheabstand: Summe der beiden Radien. Gefunden wird bis
    /// `ruhe + ZUSCHLAG_PAAR`.
    pub ruhe: f64,
}

/// Stoffnetz für die Suche: Dreiecke, Kanten mit Nachbardreiecken und die
/// Radien je Punkt, Kante und Dreieck. Wird einmal aus der Szene gebaut.
#[derive(Debug, Clone, Default)]
pub struct Stoffnetz {
    pub dreiecke: Vec<[u32; 3]>,
    pub kanten: Vec<[u32; 2]>,
    /// je Kante bis zu zwei Dreiecke (`u32::MAX` = keins)
    pub kanten_dreiecke: Vec<[u32; 2]>,
    /// je Dreieck seine drei Kanten (gegenüber Punkt 2, 0, 1: Kante k läuft
    /// von Punkt k zu Punkt k+1)
    pub dreieck_kanten: Vec<[u32; 3]>,
    pub r_punkt: Vec<f64>,
    pub r_kante: Vec<f64>,
    pub r_dreieck: Vec<f64>,
    /// (kleinerer, größerer Punkt) → Kantennummer
    pub kanten_index: std::collections::HashMap<(u32, u32), u32>,
    /// je Punkt seine Nahtpartner **und seine Weld-Partner**. In klon3d sind die
    /// beiden Seiten einer Naht zwei Punkte am selben Ort; die Stoßsuche (und,
    /// wenn nicht anders verlangt, die Schnittsuche) behandelt sie wie eine
    /// gemeinsame Ecke — bei CLO der einzige Ausschluss. Ohne das gälte jedes
    /// Zittern an der Naht als Durchgang durch das Nachbardreieck (Bluse3: 300
    /// Stöße je Bild in Ruhe statt 1).
    ///
    /// Dasselbe gilt für zwei Punkte, die eine Feder der Ruhelänge 0
    /// zusammenhält: das gefaltete Zugabeband ist über solche Federn an die
    /// Nahtlinie des Teils geschweißt ([`crate::saumfalte`]), liegt also mit dem
    /// Teilpunkt auf demselben Ort wie eine Naht. Ohne den Ausschluss sah die
    /// Nähe-Suche dort zwei Lagen im Abstand 0 und schob das Band vom Teil weg
    /// (Rechteck 20 × 20 cm mit 3 mm Zugabe: 788 Paare bei t = 0, davon 406 über
    /// eine 0-Feder; nach 60 Bildern lag das Band bei −2,52 mm statt bei −0,40 mm
    /// und die Durchdringungen am Ende bei 6 statt 3). Mit dem
    /// Ausschluss bleiben 0 Paare über eine 0-Feder und 382 Paare übrig: 348 Band
    /// auf eigenem Träger (83 Punkt–Dreieck, 265 Kante–Kante) und 34 Band↔Band
    /// an der Gehrung. Für die erste Gruppe gilt nicht der Ausschluss, sondern
    /// der Ruheabstand [`Stoffnetz::band_abstand`] — in **beiden** Nähe-Suchen,
    /// Punkt–Dreieck wie Kante–Kante.
    pub naht_partner: Vec<Vec<u32>>,
    /// Je Punkt das Schnittteil, zu dem er gehört (Index in `Szene::teile`).
    pub teil_von: Vec<u32>,
    /// Je Punkt der Träger seines gefalteten Zugabebands (`u32::MAX` = kein
    /// Band). Ein Band und sein Träger sind **eine Lage Stoff**: das Band ist
    /// die Rückseite desselben Teils, nur um die Nahtlinie geklappt. Für ein
    /// solches Paar gilt als Ruheabstand **die Summe der beiden Stoffdicken**
    /// (2 × 0,2 mm), nicht der Kissenabstand `Stoffdicke + Zusatzdicke`
    /// (2,9 mm): die Zusatzdicke ist ein Rechenpolster, keine Geometrie, und
    /// würde das aufliegende Band sichtbar abheben.
    pub band_von: Vec<u32>,
    /// Je Punkt die halbe **Stoffdicke** (m) ohne die Zusatzdicke — der
    /// Abstand, den zwei Lagen desselben Stoffes wirklich trennt.
    pub dicke: Vec<f64>,
}

impl Stoffnetz {
    /// Gehören `a` und `b` zu demselben Blatt, weil eines davon das gefaltete
    /// Zugabeband des anderen ist? Dann ist es kein Kontakt zweier Lagen,
    /// sondern die Rückseite einer Lage.
    pub fn band_und_traeger(&self, a: u32, b: u32) -> bool {
        let traeger = |p: u32| -> Option<u32> {
            let v = *self.band_von.get(p as usize)?;
            (v != u32::MAX).then_some(v)
        };
        let teil = |p: u32| -> Option<u32> { self.teil_von.get(p as usize).copied() };
        match (traeger(a), traeger(b)) {
            (Some(t), _) if teil(b) == Some(t) => true,
            (_, Some(t)) if teil(a) == Some(t) => true,
            _ => false,
        }
    }

    /// Ruheabstand eines Bandes auf seinem eigenen Träger: die Summe der beiden
    /// **Stoffdicken** (2 × halbe Dicke), nicht `Stoffdicke + Zusatzdicke`. Die
    /// Zusatzdicke ist CLOs Rechenpolster gegen Durchdringen, keine Geometrie;
    /// mit ihr stünde das aufliegende Band 2,9 mm über dem Stoff, den es
    /// bedeckt. `None`, wenn es kein Band-Träger-Paar ist.
    pub fn band_abstand(&self, a: u32, andere: &[u32]) -> Option<f64> {
        andere
            .iter()
            .any(|&b| self.band_und_traeger(a, b))
            .then(|| {
                let dick = |p: u32| self.dicke.get(p as usize).copied().unwrap_or(0.0);
                dick(a) + andere.iter().map(|&b| dick(b)).fold(0.0, f64::max)
            })
    }

    /// Aus Dreiecken und dem Radius je Punkt. Kante und Dreieck bekommen den
    /// größten Radius ihrer Punkte (bei CLO ist er je Schnittteil gleich).
    pub fn neu(dreiecke: &[[u32; 3]], r_punkt: Vec<f64>) -> Stoffnetz {
        let mut karte: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
        let mut kanten = Vec::new();
        let mut kanten_dreiecke: Vec<[u32; 2]> = Vec::new();
        let mut dreieck_kanten = Vec::with_capacity(dreiecke.len());
        for (t, d) in dreiecke.iter().enumerate() {
            let mut dk = [0u32; 3];
            for k in 0..3 {
                let (a, b) = (d[k], d[(k + 1) % 3]);
                let schluessel = (a.min(b), a.max(b));
                let e = *karte.entry(schluessel).or_insert_with(|| {
                    kanten.push([schluessel.0, schluessel.1]);
                    kanten_dreiecke.push([u32::MAX; 2]);
                    (kanten.len() - 1) as u32
                });
                let kd = &mut kanten_dreiecke[e as usize];
                if kd[0] == u32::MAX {
                    kd[0] = t as u32;
                } else if kd[1] == u32::MAX {
                    kd[1] = t as u32;
                }
                dk[k] = e;
            }
            dreieck_kanten.push(dk);
        }
        let r_kante = kanten.iter().map(|k| r_punkt[k[0] as usize].max(r_punkt[k[1] as usize])).collect();
        let r_dreieck = dreiecke.iter().map(|d| d.iter().map(|&i| r_punkt[i as usize]).fold(0.0, f64::max)).collect();
        Stoffnetz { dreiecke: dreiecke.to_vec(), kanten, kanten_dreiecke, dreieck_kanten, r_punkt, r_kante, r_dreieck, kanten_index: karte, naht_partner: Vec::new(), teil_von: Vec::new(), band_von: Vec::new(), dicke: Vec::new() }
    }

    /// Ist `b` für `a` derselbe Punkt oder sein Naht- oder Weldpartner?
    pub fn gleich_oder_partner(&self, a: u32, b: u32) -> bool {
        a == b || self.naht_partner.get(a as usize).is_some_and(|l| l.contains(&b))
    }
}

/// Sicher normiert; zu kurz (< 1e-12) gibt (1, 0, 0) wie CLO.
fn einheit(a: [f64; 3]) -> [f64; 3] {
    let l = norm(a);
    if l > 1e-12 {
        mul(a, 1.0 / l)
    } else {
        [1.0, 0.0, 0.0]
    }
}

/// Normalen je Dreieck, je Punkt (Summe der Dreiecksnormalen, normiert) und
/// je Kante die Ebene durch die Kante, die die gemittelte Normale enthält
/// (deren Normale steht quer zur Kante in der Stoffebene).
#[derive(Debug, Clone, Default)]
pub struct Normalen {
    pub dreieck: Vec<[f64; 3]>,
    pub punkt: Vec<[f64; 3]>,
    pub kantenebene: Vec<[f64; 3]>,
}

impl Normalen {
    pub fn neu(netz: &Stoffnetz, x: &[[f64; 3]]) -> Normalen {
        let dreieck: Vec<[f64; 3]> = netz
            .dreiecke
            .iter()
            .map(|d| {
                let [a, b, c] = d.map(|i| x[i as usize]);
                einheit(cross(sub(b, a), sub(c, a)))
            })
            .collect();
        let mut punkt = vec![[0.0; 3]; x.len()];
        for (t, d) in netz.dreiecke.iter().enumerate() {
            for &i in d {
                punkt[i as usize] = add(punkt[i as usize], dreieck[t]);
            }
        }
        for p in &mut punkt {
            *p = einheit(*p);
        }
        let kantenebene = netz
            .kanten
            .iter()
            .zip(&netz.kanten_dreiecke)
            .map(|(k, kd)| {
                let m = match (kd[0], kd[1]) {
                    (u32::MAX, _) => [1.0, 0.0, 0.0],
                    (t, u32::MAX) => dreieck[t as usize],
                    (t, u) => {
                        let (a, b) = (dreieck[t as usize], dreieck[u as usize]);
                        if dot(a, b) < 0.0 { sub(a, b) } else { add(a, b) }
                    }
                };
                einheit(cross(m, sub(x[k[0] as usize], x[k[1] as usize])))
            })
            .collect();
        Normalen { dreieck, punkt, kantenebene }
    }
}

/// Räumliches Hashing wie CLO: Zellen der Kante `zelle`, Zelle (i, j, k) im
/// Eimer `(i·73856093 ⊕ …)`-artig gestreut; je Eintrag sein Hüllquader.
struct Hashgitter {
    zelle: f64,
    eimer: usize,
    anfang: Vec<u32>,
    eintraege: Vec<u32>,
}

fn zellindex(x: f64, zelle: f64) -> i64 {
    (x / zelle).floor() as i64
}

/// Ein Quader mit nicht endlichen Ecken oder über mehr als 64 Zellen je Achse
/// kommt nur aus einer zerfallenen Lage; er bleibt draußen, statt die Suche
/// lahmzulegen.
fn zellbereich_ok(q: &([f64; 3], [f64; 3]), zelle: f64) -> bool {
    (0..3).all(|a| q.0[a].is_finite() && q.1[a].is_finite() && (q.1[a] - q.0[a]) <= 64.0 * zelle)
}

fn streue(i: i64, j: i64, k: i64, n: usize) -> usize {
    // Die Primzahlen aus Ericson (2005), 7.1.6, wie bei CLO; Rest vorzeichenlos.
    let h = (i as u32).wrapping_mul(0x8DA6_B343) ^ (j as u32).wrapping_mul(0xD816_3841) ^ (k as u32).wrapping_mul(0xCB1A_B31F);
    h as usize % n
}

impl Hashgitter {
    fn neu(quader: &[([f64; 3], [f64; 3])], zelle: f64) -> Hashgitter {
        let eimer = (2 * quader.len()).max(64);
        let mut zahl = vec![0u32; eimer + 1];
        // Je Eintrag jeden Eimer nur einmal, auch wenn zwei seiner Zellen in
        // denselben Eimer fallen (sonst käme ein Paar doppelt).
        let je_zelle = |q: &([f64; 3], [f64; 3]), f: &mut dyn FnMut(usize)| {
            if !zellbereich_ok(q, zelle) {
                return;
            }
            let lo = q.0.map(|v| zellindex(v, zelle));
            let hi = q.1.map(|v| zellindex(v, zelle));
            let mut hs = Vec::with_capacity(8);
            for i in lo[0]..=hi[0] {
                for j in lo[1]..=hi[1] {
                    for k in lo[2]..=hi[2] {
                        hs.push(streue(i, j, k, eimer));
                    }
                }
            }
            hs.sort_unstable();
            hs.dedup();
            for h in hs {
                f(h);
            }
        };
        for q in quader {
            je_zelle(q, &mut |h| zahl[h + 1] += 1);
        }
        for h in 0..eimer {
            zahl[h + 1] += zahl[h];
        }
        let anfang = zahl.clone();
        let mut fuell = zahl;
        let mut eintraege = vec![0u32; anfang[eimer] as usize];
        for (e, q) in quader.iter().enumerate() {
            je_zelle(q, &mut |h| {
                eintraege[fuell[h] as usize] = e as u32;
                fuell[h] += 1;
            });
        }
        Hashgitter { zelle, eimer, anfang, eintraege }
    }

    /// Ruft `f(eintrag, zelle)` für jeden Eintrag in den Zellen des Quaders.
    /// Ein Eintrag kann aus einem Eimer kommen, in den auch eine fremde Zelle
    /// fällt; [`erste_zelle`] mit der abgefragten Zelle hält jedes Paar
    /// trotzdem bei einem Aufruf.
    fn je_eintrag(&self, q: &([f64; 3], [f64; 3]), mut f: impl FnMut(u32, [i64; 3])) {
        if !zellbereich_ok(q, self.zelle) {
            return;
        }
        let lo = q.0.map(|v| zellindex(v, self.zelle));
        let hi = q.1.map(|v| zellindex(v, self.zelle));
        for i in lo[0]..=hi[0] {
            for j in lo[1]..=hi[1] {
                for k in lo[2]..=hi[2] {
                    let h = streue(i, j, k, self.eimer);
                    for &e in &self.eintraege[self.anfang[h] as usize..self.anfang[h + 1] as usize] {
                        f(e, [i, j, k]);
                    }
                }
            }
        }
    }
}

fn quader_punkte(p: &[[f64; 3]], m: f64) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in p {
        for a in 0..3 {
            lo[a] = lo[a].min(q[a] - m);
            hi[a] = hi[a].max(q[a] + m);
        }
    }
    (lo, hi)
}

fn ueberlappt(a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])) -> bool {
    (0..3).all(|k| a.0[k] <= b.1[k] && b.0[k] <= a.1[k])
}

/// Jedes Paar nur in einer Zelle zählen: in der, die je Achse beim größeren
/// unteren Rand der beiden Quader liegt.
fn erste_zelle(a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3]), zelle: f64, c: [i64; 3]) -> bool {
    (0..3).all(|k| zellindex(a.0[k].max(b.0[k]), zelle) == c[k])
}

/// Nächster Punkt auf dem Dreieck (p0, p1, p2) zu x als Gewichte (w0, w1, w2).
fn naechster_dreieck(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3], x: [f64; 3]) -> [f64; 3] {
    let (a, _) = crate::netzkoerper::naechster_auf_dreieck(x, p0, p1, p2);
    // Gewichte aus dem Punkt zurückrechnen (Ericson liefert den Punkt).
    let (e1, e2, r) = (sub(p1, p0), sub(p2, p0), sub(a, p0));
    let (d11, d12, d22) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let det = d11 * d22 - d12 * d12;
    if det <= 1e-30 {
        return [1.0, 0.0, 0.0];
    }
    let (r1, r2) = (dot(r, e1), dot(r, e2));
    let w1 = (d22 * r1 - d12 * r2) / det;
    let w2 = (d11 * r2 - d12 * r1) / det;
    [1.0 - w1 - w2, w1, w2]
}

/// Sucht alle Nähe-Paare im Zustand `x`. `w`: inverse Massen — Paare, deren
/// Punkte alle festgehalten sind, zählen nicht.
pub fn finde(netz: &Stoffnetz, x: &[[f64; 3]], w: &[f64], punkt_dreieck: bool, kante_kante: bool) -> Vec<Naehe> {
    let nrm = Normalen::neu(netz, x);
    let mut aus = Vec::new();
    let r_max = netz.r_punkt.iter().copied().fold(0.0, f64::max);
    if punkt_dreieck {
        finde_punkt_dreieck(netz, x, w, &nrm, r_max, &mut aus);
    }
    if kante_kante {
        finde_kante_kante(netz, x, w, &nrm, r_max, &mut aus);
    }
    aus
}

fn finde_punkt_dreieck(netz: &Stoffnetz, x: &[[f64; 3]], w: &[f64], nrm: &Normalen, r_max: f64, aus: &mut Vec<Naehe>) {
    let qp: Vec<_> = (0..x.len()).map(|v| quader_punkte(&[x[v]], netz.r_punkt[v] + ZUSCHLAG_QUADER)).collect();
    let zelle = (2.0 * r_max + ZUSCHLAG_PAAR).max(1e-4) * 2.0;
    let gitter = Hashgitter::neu(&qp, zelle);
    for (t, d) in netz.dreiecke.iter().enumerate() {
        let [i0, i1, i2] = *d;
        if i0 == i1 || i1 == i2 || i0 == i2 {
            continue;
        }
        let p = [x[i0 as usize], x[i1 as usize], x[i2 as usize]];
        let rt = netz.r_dreieck[t];
        let qt = quader_punkte(&p, rt + ZUSCHLAG_QUADER);
        let nt = nrm.dreieck[t];
        let en = netz.dreieck_kanten[t].map(|e| nrm.kantenebene[e as usize]);
        // Vorzeichen der gegenüberliegenden Ecke je Kantenebene: innen.
        let s = [dot(sub(p[2], p[0]), en[0]), dot(sub(p[0], p[1]), en[1]), dot(sub(p[1], p[2]), en[2])];
        gitter.je_eintrag(&qt, |v, c| {
            // Same point, seam partner or weld partner: one place, not two
            // layers. The proximity search used to skip only the identical
            // indices, so a welded band point counted as a layer lying on the
            // piece's own triangle at distance 0 and was pushed off it.
            if d.iter().any(|&i| netz.gleich_oder_partner(v, i)) {
                return;
            }
            let vu = v as usize;
            if w[vu] <= 0.0 && d.iter().all(|&i| w[i as usize] <= 0.0) {
                return;
            }
            if !ueberlappt(&qp[vu], &qt) || !erste_zelle(&qt, &qp[vu], gitter.zelle, c) {
                return;
            }
            let xv = x[vu];
            let mut ruhe = rt + netz.r_punkt[vu];
            // A folded allowance band on its own piece: the two layers of the
            // same fabric rest one fabric thickness apart, not the padded
            // `dicke + zusatzdicke`.
            if let Some(eine_dicke) = netz.band_abstand(v, d) {
                ruhe = eine_dicke;
            }
            let soll = ruhe + ZUSCHLAG_PAAR;
            let sd = dot(nt, sub(xv, p[0]));
            if sd.abs() > soll {
                return;
            }
            // Prisma über dem Dreieck, an den Kanten zum Nachbarn halbiert.
            for k in 0..3 {
                if dot(sub(xv, p[k]), en[k]) * s[k] < 0.0 {
                    return;
                }
            }
            let g = naechster_dreieck(p[0], p[1], p[2], xv);
            let c_punkt = add(add(mul(p[0], g[0]), mul(p[1], g[1])), mul(p[2], g[2]));
            let r = sub(xv, c_punkt);
            let abstand = norm(r);
            if abstand > soll {
                return;
            }
            aus.push(Naehe {
                art: Art::PunktDreieck,
                punkte: [v, i0, i1, i2],
                gewichte: [1.0, -g[0], -g[1], -g[2]],
                normale: if abstand > 1e-12 { mul(r, 1.0 / abstand) } else { nt },
                abstand,
                ruhe,
            });
        });
    }
}

fn kanten_quader(a: [f64; 3], b: [f64; 3], m: f64) -> ([f64; 3], [f64; 3]) {
    // quer zur Kante um m aufgebläht, nicht in Kantenrichtung (wie CLO)
    let t = einheit(sub(b, a));
    let h = if (t[1] - 1.0).abs() < 1e-5 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = einheit(cross(t, h));
    let v = einheit(cross(t, u));
    let mut ecken = Vec::with_capacity(8);
    for p in [a, b] {
        for su in [-m, m] {
            for sv in [-m, m] {
                ecken.push(add(add(p, mul(u, su)), mul(v, sv)));
            }
        }
    }
    quader_punkte(&ecken, 0.0)
}

/// Die Kante–Kante-Nähe ist „nicht die kürzeste“, wenn ihre Richtung flach
/// (mehr als 60° von dessen Normale) in ein Nachbardreieck der Kante zeigt —
/// dann übernimmt die Punkt–Dreieck-Nähe.
fn ist_kuerzeste(netz: &Stoffnetz, nrm: &Normalen, x: &[[f64; 3]], e: usize, u: [f64; 3], m: [f64; 3]) -> bool {
    let [k0, k1] = netz.kanten[e];
    for &t in &netz.kanten_dreiecke[e] {
        if t == u32::MAX {
            continue;
        }
        let d = netz.dreiecke[t as usize];
        let Some(&o) = d.iter().find(|&&i| i != k0 && i != k1) else { continue };
        let r = sub(x[o as usize], x[k0 as usize]);
        let wr = einheit(sub(r, mul(u, dot(r, u))));
        if dot(m, wr) > 0.0 && dot(m, nrm.dreieck[t as usize]).abs() < 0.5 {
            return false;
        }
    }
    true
}

fn finde_kante_kante(netz: &Stoffnetz, x: &[[f64; 3]], w: &[f64], nrm: &Normalen, r_max: f64, aus: &mut Vec<Naehe>) {
    let qk: Vec<_> = netz
        .kanten
        .iter()
        .enumerate()
        .map(|(e, k)| kanten_quader(x[k[0] as usize], x[k[1] as usize], netz.r_kante[e] + ZUSCHLAG_QUADER))
        .collect();
    let zelle = (2.0 * r_max + ZUSCHLAG_PAAR).max(1e-4) * 2.0;
    let gitter = Hashgitter::neu(&qk, zelle);
    for (e, &[a0, a1]) in netz.kanten.iter().enumerate() {
        if a0 == a1 {
            continue;
        }
        let (pa0, pa1) = (x[a0 as usize], x[a1 as usize]);
        let ne = einheit(add(nrm.punkt[a0 as usize], nrm.punkt[a1 as usize]));
        gitter.je_eintrag(&qk[e], |f, c| {
            let f = f as usize;
            if f <= e || !ueberlappt(&qk[e], &qk[f]) || !erste_zelle(&qk[e], &qk[f], gitter.zelle, c) {
                return;
            }
            let [b0, b1] = netz.kanten[f];
            // Shared point, seam partner or weld partner: one place, not two
            // layers (same rule as in `finde_punkt_dreieck`).
            if [b0, b1].iter().any(|&b| netz.gleich_oder_partner(a0, b) || netz.gleich_oder_partner(a1, b)) {
                return;
            }
            // Band against its own piece: the band is the piece's own reverse,
            // so this pair gets the same rest distance as in the point-triangle
            // search (one fabric thickness, `Stoffnetz::band_abstand`). Either
            // edge may be the band's, so both sides are asked.
            let band_dick = netz
                .band_abstand(a0, &[b0, b1])
                .or_else(|| netz.band_abstand(a1, &[b0, b1]))
                .or_else(|| netz.band_abstand(b0, &[a0, a1]))
                .or_else(|| netz.band_abstand(b1, &[a0, a1]));
            if [a0, a1, b0, b1].iter().all(|&i| w[i as usize] <= 0.0) {
                return;
            }
            let (pb0, pb1) = (x[b0 as usize], x[b1 as usize]);
            let (da, db) = (sub(pa1, pa0), sub(pb1, pb0));
            let (la, lb) = (norm(da), norm(db));
            if la < KANTE_MIN || lb < KANTE_MIN {
                return;
            }
            let (ua, ub) = (mul(da, 1.0 / la), mul(db, 1.0 / lb));
            let cw = dot(ua, ub);
            if cw.abs() > 0.99999 {
                return;
            }
            let r = sub(pb0, pa0);
            let (p, q) = (dot(r, ua), -dot(r, ub));
            let nen = 1.0 - cw * cw;
            let sa = (p + cw * q) / nen;
            let sb = (q + cw * p) / nen;
            // keine Klemmung an die Enden: liegt der nächste Punkt außerhalb,
            // ist es ein Fall für Punkt–Dreieck
            if !(0.0..=la).contains(&sa) || !(0.0..=lb).contains(&sb) {
                return;
            }
            let pa = add(pa0, mul(ua, sa));
            let pb = add(pb0, mul(ub, sb));
            let dv = sub(pa, pb);
            let abstand = norm(dv);
            let ruhe = match band_dick {
                Some(eine_dicke) => eine_dicke,
                None => netz.r_kante[e] + netz.r_kante[f],
            };
            if abstand > ruhe + ZUSCHLAG_PAAR {
                return;
            }
            let n = einheit(dv);
            let _ = ne;
            if !ist_kuerzeste(netz, nrm, x, f, ub, n) || !ist_kuerzeste(netz, nrm, x, e, ua, mul(n, -1.0)) {
                return;
            }
            let (s, t) = (sa / la, sb / lb);
            aus.push(Naehe {
                art: Art::KanteKante,
                punkte: [a0, a1, b0, b1],
                gewichte: [1.0 - s, s, -(1.0 - t), -t],
                normale: n,
                abstand,
                ruhe,
            });
        });
    }
}

// ---------------------------------------------------------------------------
// Nähe-Kraft: Feder um den Ruheabstand, ins implizite System.

/// Begrenzung der Federauslenkung (CLO: 2,5 mm).
pub const AUSLENKUNG_MAX: f64 = 2.5e-3;
/// Je Schritt nimmt die Dämpfung 1/32 der Annäherungsgeschwindigkeit (CLO).
pub const ANNAEHERUNG_ANTEIL: f64 = 1.0 / 32.0;

/// Was die Nähe-Kraft je Element braucht: Dehnsteifigkeit E (N/m), Fläche A
/// (m²) und Reibung μ — bei CLO stehen genau diese drei in `CudaVertex`,
/// `CudaEdge` und `CudaTriangle` (+0, +4, +8; am laufenden Programm gelesen:
/// 348.274 g/s², Fläche in mm², 0,03). Die Fläche ist beim Dreieck seine
/// eigene, beim Punkt ein Drittel der anliegenden, bei der Kante das Mittel
/// der anliegenden.
#[derive(Debug, Clone, Default)]
pub struct Stoffwerte {
    pub e_punkt: Vec<f64>,
    pub a_punkt: Vec<f64>,
    pub e_kante: Vec<f64>,
    pub a_kante: Vec<f64>,
    pub e_dreieck: Vec<f64>,
    pub a_dreieck: Vec<f64>,
    pub mu_punkt: Vec<f64>,
    /// je Punkt die Nahtfedern: (Partner, Index in `Szene::naht`)
    pub naht_partner: Vec<Vec<(u32, u32)>>,
}

/// Netz, Radien und Stoffwerte aus der Szene. Der Radius je Punkt ist
/// (Stoffdicke + `zusatzdicke`)/2 wie bei CLO.
pub fn aus_szene(s: &crate::Szene, zusatzdicke: f64) -> (Stoffnetz, Stoffwerte) {
    let n = s.x.len();
    let r: Vec<f64> = s.kollisionsabstand.iter().map(|h| h + zusatzdicke / 2.0).collect();
    let mut netz = Stoffnetz::neu(&s.dreiecke, r);
    netz.naht_partner = vec![Vec::new(); n];
    for &[i, j] in &s.naht.punkte {
        netz.naht_partner[i as usize].push(j);
        netz.naht_partner[j as usize].push(i);
    }
    // Feder der Ruhelänge 0 = derselbe Ort, wie eine Naht: das gefaltete
    // Zugabeband hängt so an der Nahtlinie des Teils (`crate::saumfalte`).
    // Ohne das sähe die Nähe-Suche zwei Lagen im Abstand 0.
    for (e, &[i, j]) in s.abstand.punkte.iter().enumerate() {
        if s.abstand.ruhe[e] != 0.0 || i == j {
            continue;
        }
        if !netz.naht_partner[i as usize].contains(&j) {
            netz.naht_partner[i as usize].push(j);
        }
        if !netz.naht_partner[j as usize].contains(&i) {
            netz.naht_partner[j as usize].push(i);
        }
    }
    // Je Punkt sein Teil und, wenn es ein gefaltetes Band ist, sein Träger.
    netz.teil_von = vec![u32::MAX; n];
    netz.band_von = vec![u32::MAX; n];
    netz.dicke = s.kollisionsabstand.clone();
    for (ti, t) in s.teile.iter().enumerate() {
        for i in t.punkte.clone() {
            netz.teil_von[i] = ti as u32;
        }
        if let Some(traeger) = t.band_von {
            for i in t.punkte.clone() {
                netz.band_von[i] = traeger as u32;
            }
        }
    }
    let mut e_punkt = vec![0.0; n];
    let mut e_dreieck = vec![0.0; s.dreiecke.len()];
    for t in &s.teile {
        for i in t.punkte.clone() {
            e_punkt[i] = t.vorgabe.material.dehnung_n_m;
        }
        for d in t.dreiecke.clone() {
            e_dreieck[d] = t.vorgabe.material.dehnung_n_m;
        }
    }
    // Ruhefläche aus der inversen Ruhematrix: A = 1/(2·|det|)
    let a_dreieck: Vec<f64> = s.ruhe_inv.iter().map(|m| 0.5 / (m[0] * m[3] - m[1] * m[2]).abs().max(1e-30)).collect();
    let mut a_punkt = vec![0.0; n];
    for (t, d) in s.dreiecke.iter().enumerate() {
        for &i in d {
            a_punkt[i as usize] += a_dreieck[t] / 3.0;
        }
    }
    let mut e_kante = Vec::with_capacity(netz.kanten.len());
    let mut a_kante = Vec::with_capacity(netz.kanten.len());
    for kd in &netz.kanten_dreiecke {
        let ts: Vec<usize> = kd.iter().filter(|&&t| t != u32::MAX).map(|&t| t as usize).collect();
        let k = ts.len().max(1) as f64;
        a_kante.push(ts.iter().map(|&t| a_dreieck[t]).sum::<f64>() / k);
        e_kante.push(ts.iter().map(|&t| e_dreieck[t]).sum::<f64>() / k);
    }
    let mut naht_partner = vec![Vec::new(); n];
    for (c, &[i, j]) in s.naht.punkte.iter().enumerate() {
        naht_partner[i as usize].push((j, c as u32));
        naht_partner[j as usize].push((i, c as u32));
    }
    let werte = Stoffwerte { e_punkt, a_punkt, e_kante, a_kante, e_dreieck, a_dreieck, mu_punkt: s.reibung.clone(), naht_partner };
    (netz, werte)
}

/// Eine Nähe-Feder im Schritt: Kraft je Seite und 3×3-Block
/// K̂ = k·[μ·I + (1 − μ)·n·nᵀ]; die Systemmatrix bekommt ihn je Paar (Punkt
/// der ersten, Punkt der zweiten Seite) mit dem Gewicht |gₖ|·|gₗ| in
/// Laplace-Form wie bei CLO. Fünf Plätze, weil die Durchdringung Kante
/// (2) gegen Dreieck (3) wirkt; Nähe-Paare lassen den fünften mit Gewicht 0.
#[derive(Debug, Clone, Copy)]
pub struct Feder {
    pub punkte: [u32; 5],
    pub gewichte: [f64; 5],
    /// Kraft auf die erste Seite (die zweite bekommt −gₖ·F)
    pub kraft: [f64; 3],
    pub block: [[f64; 3]; 3],
}

/// Kräfte und Blöcke der Nähe-Paare; dämpft dabei die Annäherung in `v`
/// (CLO `ChangeVelocityAfterProximity*`: 1/32 der Normalgeschwindigkeit und
/// der Anteil μ der ganzen Relativgeschwindigkeit, impulserhaltend).
/// `naht_ruhe`: Ruhelänge je Nahtfeder (∞ = offen).
pub fn federn(netz: &Stoffnetz, werte: &Stoffwerte, naehe: &[Naehe], steifigkeit: f64, masse: &[f64], w: &[f64], naht_ruhe: &[f64], v: &mut [[f64; 3]]) -> Vec<Feder> {
    let mut aus = Vec::with_capacity(naehe.len());
    for p in naehe {
        let idx = p.punkte.map(|i| i as usize);
        // Steifigkeit k = s·(E₁ + E₂)·(A₁ + A₂) / (1 mm²) — CLO rechnet in mm.
        let (e, a, mu) = match p.art {
            Art::PunktDreieck => {
                let t = dreieck_von(netz, &p.punkte[1..4]);
                (
                    werte.e_punkt[idx[0]] + werte.e_dreieck[t],
                    werte.a_punkt[idx[0]] + werte.a_dreieck[t],
                    0.5 * (werte.mu_punkt[idx[0]] + (werte.mu_punkt[idx[1]] + werte.mu_punkt[idx[2]] + werte.mu_punkt[idx[3]]) / 3.0),
                )
            }
            Art::KanteKante => {
                let (k1, k2) = (kante_von(netz, idx[0], idx[1]), kante_von(netz, idx[2], idx[3]));
                (
                    werte.e_kante[k1] + werte.e_kante[k2],
                    werte.a_kante[k1] + werte.a_kante[k2],
                    0.25 * (werte.mu_punkt[idx[0]] + werte.mu_punkt[idx[1]] + werte.mu_punkt[idx[2]] + werte.mu_punkt[idx[3]]),
                )
            }
        };
        let k = steifigkeit * e * a / 1e-6;
        // Genähte Nachbarn: eine Nahtfeder zwischen beiden Seiten mit
        // kleinerer Ruhelänge ersetzt den Ruheabstand.
        let mut ruhe = p.ruhe;
        'naht: for q in 0..4 {
            if p.gewichte[q] <= 0.0 {
                continue;
            }
            for &(partner, c) in &werte.naht_partner[idx[q]] {
                let auf_zweiter = (0..4).any(|r| p.gewichte[r] < 0.0 && p.punkte[r] == partner);
                let l0 = naht_ruhe[c as usize];
                if auf_zweiter && l0.is_finite() && l0 < ruhe {
                    ruhe = l0;
                    break 'naht;
                }
            }
        }
        let c = (ruhe - p.abstand).min(AUSLENKUNG_MAX);
        let n = p.normale;
        let kraft = mul(n, k * c);
        let mut block = [[0.0; 3]; 3];
        for (i, z) in block.iter_mut().enumerate() {
            for (j, b) in z.iter_mut().enumerate() {
                *b = k * ((1.0 - mu) * n[i] * n[j] + if i == j { mu } else { 0.0 });
            }
        }
        // Dämpfung der Annäherung
        let (mut m1, mut m2) = (0.0, 0.0);
        let mut dv = [0.0; 3];
        for q in 0..4 {
            let g = p.gewichte[q];
            dv = add(dv, mul(v[idx[q]], g));
            if g > 0.0 { m1 += g * masse[idx[q]] } else { m2 -= g * masse[idx[q]] }
        }
        let an = dot(dv, n) * ANNAEHERUNG_ANTEIL;
        if an < 0.0 && m1 + m2 > 0.0 {
            let u = add(mul(n, an), mul(sub(dv, mul(n, an)), mu));
            for q in 0..4 {
                let g = p.gewichte[q];
                if w[idx[q]] <= 0.0 {
                    continue;
                }
                let anteil = if g > 0.0 { m2 / (m1 + m2) } else { m1 / (m1 + m2) };
                v[idx[q]] = sub(v[idx[q]], mul(u, g * anteil));
            }
        }
        let [a, b, c, d] = p.punkte;
        let [ga, gb, gc, gd] = p.gewichte;
        aus.push(Feder { punkte: [a, b, c, d, a], gewichte: [ga, gb, gc, gd, 0.0], kraft, block });
    }
    aus
}

/// Dreiecksnummer zu drei Punkten (in der Reihenfolge des Netzes).
fn dreieck_von(netz: &Stoffnetz, p: &[u32]) -> usize {
    // Die Kante p0–p1 kennt ihre Dreiecke; eines davon ist es.
    let e = kante_von(netz, p[0] as usize, p[1] as usize);
    netz.kanten_dreiecke[e].iter().copied().find(|&t| t != u32::MAX && netz.dreiecke[t as usize].contains(&p[2])).unwrap_or(0) as usize
}

fn kante_von(netz: &Stoffnetz, a: usize, b: usize) -> usize {
    let (a, b) = (a.min(b) as u32, a.max(b) as u32);
    netz.kanten_index.get(&(a, b)).copied().unwrap_or(0) as usize
}

// ---------------------------------------------------------------------------
// Durchdringung: Kanten, die schon durch ein Dreieck stechen.

/// So viele Schritte in Folge zählt die Durchdringung eines Dreiecks
/// höchstens (CLO: 5) — die Schubkraft wächst bis dahin mit.
pub const FOLGE_MAX: u32 = 5;
/// CLOs Schubkraft k·Zähler ist eine Kraft in g·mm/s² bei einer Steifigkeit
/// in g/s²: in SI steckt darin ein Hebel von 1 mm.
pub const SCHUB_HEBEL: f64 = 1e-3;

/// Ein Schnitt Kante durch Dreieck.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schnitt {
    pub kante: u32,
    pub dreieck: u32,
    /// Lage auf der Kante (0 = erster Punkt)
    pub t: f64,
    /// Gewichte der Dreiecksecken
    pub w: [f64; 3],
    /// Richtung, in der die Kante gegen das Dreieck muss, damit die
    /// Schnittkurve kürzer wird (nach der Gebietsmittelung: je Gebiet gleich)
    pub richtung: [f64; 3],
    /// Gewicht |g|/√(|g|² + 1,5²) der Einzelrichtung
    pub gewicht: f64,
}

/// Schnitt der Kante (a, b) mit dem Dreieck (v0, v1, v2) der Normale n:
/// (t auf der Kante, Gewichte der Ecken). Endpunkte in der Ebene zählen
/// nicht, Dreiecke mit Kanten unter 0,1 mm auch nicht (wie CLO).
fn kante_durch_dreieck(a: [f64; 3], b: [f64; 3], v: [[f64; 3]; 3], n: [f64; 3]) -> Option<(f64, [f64; 3])> {
    let (d0, d1) = (dot(n, sub(a, v[0])), dot(n, sub(b, v[0])));
    if (d0 > 0.0 && d1 > 0.0) || (d0 < 0.0 && d1 < 0.0) || (d1 - d0).abs() < 1e-12 {
        return None;
    }
    let t = -d0 / (d1 - d0);
    if t.abs() < 1e-5 || (t - 1.0).abs() < 1e-5 {
        return None;
    }
    let p = lerp(a, b, t);
    let (e1, e2) = (sub(v[1], v[0]), sub(v[2], v[0]));
    if norm(e1) < STOSS_KANTE_MIN || norm(e2) < STOSS_KANTE_MIN {
        return None;
    }
    let uv = im_dreieck(v[0], v[1], v[2], p)?;
    Some((t, [1.0 - uv[0] - uv[1], uv[0], uv[1]]))
}

/// Alle Schnitte Kante durch Dreieck (ohne gemeinsamen Punkt), mit der
/// Einzelrichtung nach der Schnittkurvenlänge: je Nachbardreieck k der Kante
/// geht ein Ast mₖ der Schnittkurve von p aus; verschiebt man die Kante um δ
/// gegen das Dreieck, ändert sich die Kurvenlänge um −δ·g mit
/// g = Σₖ [mₖ − n·(ê·mₖ)/(n·ê)].
///
/// `an_naehten`: auch Kanten durch Dreiecke ihres Nahtpartners suchen (wie
/// CLO, das nur gemeinsame Ecken ausnimmt). Vorgabe in klon3d ist aus: am Rock
/// v36 sank damit die untere Schlitzlage in der Haltprobe 9,7 mm (Befund 116),
/// ohne blieb alles in Ruhe.
pub fn finde_schnitte(netz: &Stoffnetz, x: &[[f64; 3]], an_naehten: bool) -> Vec<Schnitt> {
    let nrm = Normalen::neu(netz, x);
    let qt: Vec<_> = netz.dreiecke.iter().map(|d| quader_punkte(&d.map(|i| x[i as usize]), ZUSCHLAG_QUADER)).collect();
    let laenge = netz.kanten.iter().map(|k| norm(sub(x[k[0] as usize], x[k[1] as usize]))).fold(0.0, f64::max);
    let gitter = Hashgitter::neu(&qt, laenge.max(1e-3));
    let mut aus = Vec::new();
    for (e, &[a0, a1]) in netz.kanten.iter().enumerate() {
        let kd = netz.kanten_dreiecke[e];
        if a0 == a1 || kd[0] == u32::MAX {
            continue;
        }
        let (pa, pb) = (x[a0 as usize], x[a1 as usize]);
        let qe = quader_punkte(&[pa, pb], ZUSCHLAG_QUADER);
        let ehat = einheit(sub(pb, pa));
        gitter.je_eintrag(&qe, |t, c| {
            let tu = t as usize;
            let d = netz.dreiecke[tu];
            let nachbar = if an_naehten {
                d.contains(&a0) || d.contains(&a1)
            } else {
                d.iter().any(|&i| netz.gleich_oder_partner(a0, i) || netz.gleich_oder_partner(a1, i))
            };
            if nachbar || !ueberlappt(&qt[tu], &qe) || !erste_zelle(&qe, &qt[tu], gitter.zelle, c) {
                return;
            }
            let n = nrm.dreieck[tu];
            let Some((tk, w)) = kante_durch_dreieck(pa, pb, d.map(|i| x[i as usize]), n) else { return };
            let cn = dot(n, ehat);
            if cn.abs() < 1e-6 {
                return;
            }
            let p = lerp(pa, pb, tk);
            let mut g = [0.0; 3];
            for &k in kd.iter().filter(|&&k| k != u32::MAX) {
                let dk = netz.dreiecke[k as usize];
                let Some(&o) = dk.iter().find(|&&i| i != a0 && i != a1) else { continue };
                let mut m = einheit(cross(n, nrm.dreieck[k as usize]));
                let r = sub(x[o as usize], p);
                let r_quer = sub(r, mul(ehat, dot(ehat, r)));
                if dot(m, r_quer) < 0.0 {
                    m = mul(m, -1.0);
                }
                g = add(g, sub(m, mul(n, dot(ehat, m) / cn)));
            }
            let lg = norm(g);
            aus.push(Schnitt { kante: e as u32, dreieck: t, t: tk, w, richtung: einheit(g), gewicht: lg / (lg * lg + 2.25).sqrt() });
        });
    }
    aus
}

/// Union-Find mit Pfadhalbierung; Wurzel ist der kleinste Index.
fn wurzel(eltern: &mut [usize], mut i: usize) -> usize {
    while eltern[i] != i {
        eltern[i] = eltern[eltern[i]];
        i = eltern[i];
    }
    i
}

fn vereinige(eltern: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (wurzel(eltern, a), wurzel(eltern, b));
    if ra != rb {
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        eltern[hi] = lo;
    }
}

/// Schnittgebiete wie CLO: Schnitte gleicher Rolle („Kante von A durch B“)
/// entlang der Schnittkurve verbinden, dann zweimal Gebiete gleicher Rolle
/// vereinigen, die an dasselbe Gebiet der Gegenrolle grenzen; je Gebiet die
/// Richtung Σ w·ĝ (normiert) an alle seine Schnitte. Gibt die Gebietswurzel
/// je Schnitt zurück.
pub fn gebiete(netz: &Stoffnetz, schnitte: &mut [Schnitt]) -> Vec<usize> {
    let n = schnitte.len();
    let index: std::collections::HashMap<(u32, u32), usize> = schnitte.iter().enumerate().map(|(i, s)| ((s.kante, s.dreieck), i)).collect();
    let finde = |e: u32, t: u32| index.get(&(e, t)).copied();
    let nachbardreiecke = |t: u32| -> Vec<u32> {
        let mut v = Vec::with_capacity(3);
        for &k in &netz.dreieck_kanten[t as usize] {
            for &u in &netz.kanten_dreiecke[k as usize] {
                if u != u32::MAX && u != t {
                    v.push(u);
                }
            }
        }
        v
    };
    let mut eltern: Vec<usize> = (0..n).collect();
    let mut tausch: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        let (e, t) = (schnitte[i].kante, schnitte[i].dreieck);
        let kanten_dreiecke: Vec<u32> = netz.kanten_dreiecke[e as usize].iter().copied().filter(|&k| k != u32::MAX).collect();
        // andere Kanten der Nachbardreiecke von e
        let mut andere_kanten = Vec::new();
        for &k in &kanten_dreiecke {
            for &f in &netz.dreieck_kanten[k as usize] {
                if f != e {
                    andere_kanten.push(f);
                }
            }
        }
        let tn = nachbardreiecke(t);
        let mut gleich = Vec::new();
        for &f in &andere_kanten {
            gleich.extend(finde(f, t));
        }
        for &u in &tn {
            gleich.extend(finde(e, u));
            for &f in &andere_kanten {
                gleich.extend(finde(f, u));
            }
        }
        for j in gleich {
            vereinige(&mut eltern, i, j);
        }
        // Rollentausch: eine Kante von t durch ein Nachbardreieck von e
        for &f in &netz.dreieck_kanten[t as usize] {
            for &k in &kanten_dreiecke {
                if let Some(j) = finde(f, k) {
                    tausch.push((i, j));
                }
            }
        }
    }
    for _ in 0..2 {
        let mut partner = vec![usize::MAX; n];
        for &(i, j) in &tausch {
            let (ri, rj) = (wurzel(&mut eltern, i), wurzel(&mut eltern, j));
            partner[ri] = partner[ri].min(rj);
            partner[rj] = partner[rj].min(ri);
        }
        for &(i, j) in &tausch {
            let rj = wurzel(&mut eltern, j);
            if partner[rj] != usize::MAX {
                let s = partner[rj];
                vereinige(&mut eltern, i, s);
            }
        }
    }
    let wurzeln: Vec<usize> = (0..n).map(|i| wurzel(&mut eltern, i)).collect();
    let mut summe = vec![[0.0; 3]; n];
    for (i, s) in schnitte.iter().enumerate() {
        summe[wurzeln[i]] = add(summe[wurzeln[i]], mul(s.richtung, s.gewicht));
    }
    for (i, s) in schnitte.iter_mut().enumerate() {
        s.richtung = einheit(summe[wurzeln[i]]);
    }
    wurzeln
}

/// Schubfedern der Durchdringung: konstante Kraft k·min(Folge, 5)·1 mm entlang
/// der Gebietsrichtung, k = 2·s·(E_Kante + E_Dreieck)·(A_Kante +
/// A_Dreieck)/(1 mm²); Block k·ĝĝᵀ ohne den Folgefaktor. `folge`: je Dreieck
/// die Schritte in Folge mit Schnitt (wird fortgeschrieben). `eingeklemmt`
/// (Kantenpunkte, Dreieckspunkte) schaltet ein ganzes Gebiet ab.
pub fn schubfedern(
    netz: &Stoffnetz,
    werte: &Stoffwerte,
    schnitte: &[Schnitt],
    wurzeln: &[usize],
    steifigkeit: f64,
    folge: &mut Vec<u32>,
    eingeklemmt: impl Fn([usize; 2], [usize; 3]) -> bool,
) -> Vec<Feder> {
    if folge.len() != netz.dreiecke.len() {
        *folge = vec![1; netz.dreiecke.len()];
    }
    let mut hat = vec![false; netz.dreiecke.len()];
    let mut klemm = vec![false; schnitte.len()];
    for (i, s) in schnitte.iter().enumerate() {
        hat[s.dreieck as usize] = true;
        let k = netz.kanten[s.kante as usize].map(|q| q as usize);
        let d = netz.dreiecke[s.dreieck as usize].map(|q| q as usize);
        if eingeklemmt(k, d) {
            klemm[wurzeln[i]] = true;
        }
    }
    for (t, f) in folge.iter_mut().enumerate() {
        *f = if hat[t] { *f + 1 } else { 1 };
    }
    let mut aus = Vec::with_capacity(schnitte.len());
    for (i, s) in schnitte.iter().enumerate() {
        if klemm[wurzeln[i]] {
            continue;
        }
        let (e, t) = (s.kante as usize, s.dreieck as usize);
        let k = 2.0 * steifigkeit * (werte.e_kante[e] + werte.e_dreieck[t]) * (werte.a_kante[e] + werte.a_dreieck[t]) / 1e-6;
        let g = s.richtung;
        let kraft = mul(g, k * folge[t].min(FOLGE_MAX) as f64 * SCHUB_HEBEL);
        let mut block = [[0.0; 3]; 3];
        for (a, z) in block.iter_mut().enumerate() {
            for (b, v) in z.iter_mut().enumerate() {
                *v = k * g[a] * g[b];
            }
        }
        let [k0, k1] = netz.kanten[e];
        let [v0, v1, v2] = netz.dreiecke[t];
        aus.push(Feder { punkte: [k0, k1, v0, v1, v2], gewichte: [1.0 - s.t, s.t, -s.w[0], -s.w[1], -s.w[2]], kraft, block });
    }
    aus
}

/// Stöße zwischen Teilen, die schon ineinanderstecken, verwirft CLO: die
/// Durchdringung löst die Schubkraft, ein Stoß hielte sie fest.
pub fn ohne_durchdrungene(netz: &Stoffnetz, stoesse: Vec<Stoss>, schnitte: &[Schnitt]) -> Vec<Stoss> {
    if schnitte.is_empty() {
        return stoesse;
    }
    let paare: std::collections::HashSet<(u32, u32)> = schnitte.iter().map(|s| (s.kante, s.dreieck)).collect();
    let kanten_von_dreieck = |t: usize| netz.dreieck_kanten[t];
    stoesse
        .into_iter()
        .filter(|st| match st.art {
            Art::PunktDreieck => {
                let t = dreieck_von(netz, &st.punkte[1..4]) as u32;
                // eine Kante am Punkt sticht durch dieses Dreieck
                !schnitte.iter().any(|s| s.dreieck == t && netz.kanten[s.kante as usize].contains(&st.punkte[0]))
            }
            Art::KanteKante => {
                let (i, j) = (kante_von(netz, st.punkte[0] as usize, st.punkte[1] as usize), kante_von(netz, st.punkte[2] as usize, st.punkte[3] as usize));
                let trifft = |a: usize, b: usize| {
                    netz.kanten_dreiecke[b].iter().filter(|&&t| t != u32::MAX).any(|&t| paare.contains(&(a as u32, t)) && kanten_von_dreieck(t as usize).contains(&(b as u32)))
                };
                !(trifft(i, j) || trifft(j, i))
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Stoß-Stufe: nach dem Löser den Weg x0 → x1 prüfen.

/// Sollabstand der Stoßantwort (CLO: 0,1 mm, fest, ohne Stoffdicke).
pub const STOSS_ABSTAND: f64 = 1e-4;
/// Kürzere Kanten nimmt die Kante–Kante-Stoßsuche nicht (CLO: 0,1 mm).
pub const STOSS_KANTE_MIN: f64 = 1e-4;

/// Ein Stoß: vier Punkte wie bei [`Naehe`], dazu die Seite, auf die der
/// erste Teil zurück muss (relativ zur Windungsnormale des zweiten Teils).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stoss {
    pub art: Art,
    /// Punkt–Dreieck: Punkt, Dreieck a, b, c; Kante–Kante: i0, i1, j0, j1
    pub punkte: [u32; 4],
    /// Punkt–Dreieck: (u, v) der Ecken b und c; Kante–Kante: (s, w)
    pub lage: [f64; 2],
    /// Zeitpunkt im Schritt (0…1)
    pub zeit: f64,
    /// +1: die erste Seite gehört auf die Seite der Normale, −1: dahinter
    pub seite: f64,
    /// Kante–Kante: die orientierte Normale zur Stoßzeit (CLO nimmt sie für
    /// die Antwort); Punkt–Dreieck rechnet die Normale an der aktuellen Lage.
    pub normale: [f64; 3],
}

/// Vorzeichenbehafteter Abstand von p zur Ebene (a, b, c), Normale aus der
/// Windung.
fn ebenenabstand(a: [f64; 3], b: [f64; 3], c: [f64; 3], p: [f64; 3]) -> f64 {
    dot(sub(p, a), einheit(cross(sub(b, a), sub(c, a))))
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    add(mul(a, 1.0 - t), mul(b, t))
}

/// Wann werden die vier Punkte (a, b, c, p) bei linearer Bewegung von `x0`
/// nach `x1` koplanar? Regula falsi auf dem Ebenenabstand wie CLO (höchstens
/// 16 Schritte, Abstandstoleranz `tol`, Intervall 1e-4); gefunden wird nur
/// ein Vorzeichenwechsel zwischen Anfang und Ende des Schritts.
fn koplanar_zeit(x0: [[f64; 3]; 4], x1: [[f64; 3]; 4], tol: f64) -> Option<f64> {
    let f = |t: f64| {
        let q = [0, 1, 2, 3].map(|k| lerp(x0[k], x1[k], t));
        ebenenabstand(q[0], q[1], q[2], q[3])
    };
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let (mut d0, mut d1) = (f(0.0), f(1.0));
    for _ in 0..16 {
        if d0 * d1 > 0.0 {
            return None;
        }
        let tau = t0 + (t1 - t0) * d0.abs() / (d0.abs() + d1.abs()).max(f64::MIN_POSITIVE);
        let d = f(tau);
        if d.abs() < tol || t1 - t0 < 1e-4 {
            return Some(tau);
        }
        if d0 * d > 0.0 {
            t0 = tau;
            d0 = d;
        } else {
            t1 = tau;
            d1 = d;
        }
    }
    None
}

/// Liegt p (in der Ebene) im Dreieck (a, b, c)? Gibt (u, v) der Ecken b, c.
fn im_dreieck(a: [f64; 3], b: [f64; 3], c: [f64; 3], p: [f64; 3]) -> Option<[f64; 2]> {
    let (e1, e2, r) = (sub(b, a), sub(c, a), sub(p, a));
    let (d11, d12, d22) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let det = d11 * d22 - d12 * d12;
    if det <= 1e-4 * d11 * d22 {
        return None;
    }
    let (r1, r2) = (dot(r, e1), dot(r, e2));
    let u = (d22 * r1 - d12 * r2) / det;
    let v = (d11 * r2 - d12 * r1) / det;
    if u < 0.0 || v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some([u, v])
}

/// Kreuzen sich die Kanten (a, b) und (c, d) im Innern? Gibt (s, w).
fn kanten_kreuzen(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Option<[f64; 2]> {
    let (e1, e2) = (sub(b, a), sub(d, c));
    let (l1, l2) = (norm(e1), norm(e2));
    if l1 < STOSS_KANTE_MIN || l2 < STOSS_KANTE_MIN {
        return None;
    }
    let (u1, u2) = (mul(e1, 1.0 / l1), mul(e2, 1.0 / l2));
    let g = dot(u1, u2);
    if g.abs() > 0.99999 {
        return None;
    }
    let q = sub(c, a);
    let nen = 1.0 - g * g;
    let s = (dot(q, u1) - g * dot(q, u2)) / nen;
    let w = (g * dot(q, u1) - dot(q, u2)) / nen;
    if s <= 0.0 || s >= l1 || w <= 0.0 || w >= l2 {
        return None;
    }
    Some([s / l1, w / l2])
}

/// Sucht die Stöße auf dem Weg `x0 → x1`: je Dreieck den frühesten Punkt, je
/// Kante die früheste Partnerkante (wie CLO). `w`: inverse Massen.
pub fn finde_stoesse(netz: &Stoffnetz, x0: &[[f64; 3]], x1: &[[f64; 3]], w: &[f64], punkt_dreieck: bool, kante_kante: bool) -> Vec<Stoss> {
    let mut aus = Vec::new();
    let weg = |idx: &[u32]| -> ([f64; 3], [f64; 3]) {
        let mut p: Vec<[f64; 3]> = idx.iter().map(|&i| x0[i as usize]).collect();
        p.extend(idx.iter().map(|&i| x1[i as usize]));
        quader_punkte(&p, 1e-8)
    };
    let laenge = netz.kanten.iter().map(|k| norm(sub(x1[k[0] as usize], x1[k[1] as usize]))).fold(0.0, f64::max);
    let zelle = laenge.max(1e-3);
    let bew = |a0: &[[f64; 3]; 4], a1: &[[f64; 3]; 4], k: usize| sub(a1[k], a0[k]);
    if punkt_dreieck {
        let qp: Vec<_> = (0..x0.len() as u32).map(|v| weg(&[v])).collect();
        let gitter = Hashgitter::neu(&qp, zelle);
        for d in &netz.dreiecke {
            if d[0] == d[1] || d[1] == d[2] || d[0] == d[2] {
                continue;
            }
            let qt = weg(d);
            let mut best: Option<Stoss> = None;
            gitter.je_eintrag(&qt, |v, c| {
                let vu = v as usize;
                if d.iter().any(|&i| netz.gleich_oder_partner(v, i)) || !ueberlappt(&qp[vu], &qt) || !erste_zelle(&qt, &qp[vu], zelle, c) {
                    return;
                }
                let idx = [d[0], d[1], d[2], v];
                if idx.iter().all(|&i| w[i as usize] <= 0.0) {
                    return;
                }
                let a0 = idx.map(|i| x0[i as usize]);
                let a1 = idx.map(|i| x1[i as usize]);
                let Some(zeit) = koplanar_zeit(a0, a1, STOSS_ABSTAND * 0.1) else { return };
                if best.is_some_and(|b| b.zeit <= zeit) {
                    return;
                }
                let q = [0, 1, 2, 3].map(|k| lerp(a0[k], a1[k], zeit));
                let Some(uv) = im_dreieck(q[0], q[1], q[2], q[3]) else { return };
                let n = einheit(cross(sub(q[1], q[0]), sub(q[2], q[0])));
                let g = [1.0 - uv[0] - uv[1], uv[0], uv[1]];
                let rel = sub(bew(&a0, &a1, 3), add(add(mul(bew(&a0, &a1, 0), g[0]), mul(bew(&a0, &a1, 1), g[1])), mul(bew(&a0, &a1, 2), g[2])));
                // Der Punkt kam von der Seite, gegen die er sich bewegt.
                let seite = if dot(rel, n) <= 0.0 { 1.0 } else { -1.0 };
                best = Some(Stoss { art: Art::PunktDreieck, punkte: [v, d[0], d[1], d[2]], lage: uv, zeit, seite, normale: mul(n, seite) });
            });
            aus.extend(best);
        }
    }
    if kante_kante {
        let qk: Vec<_> = netz.kanten.iter().map(|k| weg(k)).collect();
        let gitter = Hashgitter::neu(&qk, zelle);
        for (i, &[i0, i1]) in netz.kanten.iter().enumerate() {
            if i0 == i1 {
                continue;
            }
            let mut best: Option<Stoss> = None;
            gitter.je_eintrag(&qk[i], |j, c| {
                let j = j as usize;
                if j <= i || !ueberlappt(&qk[i], &qk[j]) || !erste_zelle(&qk[i], &qk[j], zelle, c) {
                    return;
                }
                let [j0, j1] = netz.kanten[j];
                if [j0, j1].iter().any(|&q| netz.gleich_oder_partner(q, i0) || netz.gleich_oder_partner(q, i1)) {
                    return;
                }
                // Reihenfolge für den Koplanaritätstest wie CLO: (i1, j0, j1 | i0)
                let idx = [i1, j0, j1, i0];
                if idx.iter().all(|&k| w[k as usize] <= 0.0) {
                    return;
                }
                let a0 = idx.map(|k| x0[k as usize]);
                let a1 = idx.map(|k| x1[k as usize]);
                let Some(zeit) = koplanar_zeit(a0, a1, STOSS_ABSTAND * 0.1) else { return };
                if best.is_some_and(|b| b.zeit <= zeit) {
                    return;
                }
                let q = [0, 1, 2, 3].map(|k| lerp(a0[k], a1[k], zeit));
                let Some(sw) = kanten_kreuzen(q[3], q[0], q[1], q[2]) else { return };
                let n = einheit(cross(sub(q[0], q[3]), sub(q[2], q[1])));
                let auf_i = add(mul(bew(&a0, &a1, 3), 1.0 - sw[0]), mul(bew(&a0, &a1, 0), sw[0]));
                let auf_j = add(mul(bew(&a0, &a1, 1), 1.0 - sw[1]), mul(bew(&a0, &a1, 2), sw[1]));
                let seite = if dot(sub(auf_i, auf_j), n) <= 0.0 { 1.0 } else { -1.0 };
                best = Some(Stoss { art: Art::KanteKante, punkte: [i0, i1, j0, j1], lage: sw, zeit, seite, normale: mul(n, seite) });
            });
            aus.extend(best);
        }
    }
    aus
}

/// Stoßantwort wie CLO: jede Iteration rechnet die Normale an der aktuellen
/// Lage, schiebt beide Seiten massengewichtet auf [`STOSS_ABSTAND`]
/// auseinander und mittelt die Beiträge je Punkt (reine Lagekorrektur).
/// Punkte mit `w = 0` bleiben. Gibt die Zahl der Korrekturen zurück.
pub fn stossantwort(stoesse: &[Stoss], x: &mut [[f64; 3]], masse: &[f64], w: &[f64], iterationen: usize) -> usize {
    let mut zahl = 0;
    for _ in 0..iterationen {
        let mut dx = vec![[0.0; 3]; x.len()];
        let mut cnt = vec![0u32; x.len()];
        for s in stoesse {
            let [p0, p1, p2, p3] = s.punkte.map(|i| i as usize);
            // erste Seite: Punkt bzw. Kante i; zweite: Dreieck bzw. Kante j
            let (erste, zweite, n, d) = match s.art {
                Art::PunktDreieck => {
                    let n = mul(einheit(cross(sub(x[p2], x[p1]), sub(x[p3], x[p1]))), s.seite);
                    let g = [1.0 - s.lage[0] - s.lage[1], s.lage[0], s.lage[1]];
                    (vec![(p0, 1.0)], vec![(p1, g[0]), (p2, g[1]), (p3, g[2])], n, dot(n, sub(x[p0], x[p1])))
                }
                Art::KanteKante => {
                    let n = s.normale;
                    let (a, b) = (lerp(x[p0], x[p1], s.lage[0]), lerp(x[p2], x[p3], s.lage[1]));
                    (vec![(p0, 1.0 - s.lage[0]), (p1, s.lage[0])], vec![(p2, 1.0 - s.lage[1]), (p3, s.lage[1])], n, dot(n, sub(a, b)))
                }
            };
            let tief = STOSS_ABSTAND - d;
            if tief <= 0.0 {
                continue;
            }
            let m1: f64 = erste.iter().map(|&(i, g)| g * masse[i]).sum();
            let m2: f64 = zweite.iter().map(|&(i, g)| g * masse[i]).sum();
            let fest1 = erste.iter().all(|&(i, _)| w[i] <= 0.0);
            let fest2 = zweite.iter().all(|&(i, _)| w[i] <= 0.0);
            let alpha = if fest2 { 1.0 } else if fest1 { 0.0 } else { m2 / (m1 + m2) };
            for &(i, _) in &erste {
                if w[i] > 0.0 {
                    dx[i] = add(dx[i], mul(n, tief * alpha));
                    cnt[i] += 1;
                }
            }
            for &(i, _) in &zweite {
                if w[i] > 0.0 {
                    dx[i] = sub(dx[i], mul(n, tief * (1.0 - alpha)));
                    cnt[i] += 1;
                }
            }
            zahl += 1;
        }
        let mut bewegt = false;
        for i in 0..x.len() {
            if cnt[i] > 0 {
                x[i] = add(x[i], mul(dx[i], 1.0 / cnt[i] as f64));
                bewegt = true;
            }
        }
        if !bewegt {
            break;
        }
    }
    zahl
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zwei waagerechte Quadrate (je zwei Dreiecke), 2 mm übereinander: mit
    /// Radius 1,45 mm (Soll 2,94 mm) liegen alle acht Punkte in Nähe zum
    /// anderen Quadrat, mit 0,5 mm keiner.
    #[test]
    fn zwei_lagen_in_naehe() {
        let q = |z: f64| -> Vec<[f64; 3]> { vec![[0.0, 0.0, z], [0.01, 0.0, z], [0.01, 0.01, z], [0.0, 0.01, z]] };
        let mut x = q(0.0);
        x.extend(q(0.002));
        let d = [[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]];
        let w = vec![1.0; 8];
        let netz = Stoffnetz::neu(&d, vec![1.45e-3; 8]);
        let n = finde(&netz, &x, &w, true, false);
        let mut punkte: Vec<u32> = n.iter().map(|p| p.punkte[0]).collect();
        punkte.sort();
        punkte.dedup();
        assert_eq!(punkte, (0..8).collect::<Vec<_>>(), "{n:?}");
        for p in &n {
            assert!((p.abstand - 0.002).abs() < 1e-12);
            assert!((p.ruhe - 2.9e-3).abs() < 1e-12);
        }
        let netz = Stoffnetz::neu(&d, vec![0.5e-3; 8]);
        assert!(finde(&netz, &x, &w, true, true).is_empty());
    }

    /// Zwei gekreuzte Kanten 1 mm übereinander: ein Kante–Kante-Paar, Normale
    /// von der zweiten zur ersten Kante.
    #[test]
    fn gekreuzte_kanten() {
        let x = vec![
            [-0.01, 0.0, 0.001],
            [0.01, 0.0, 0.001],
            [0.0, -0.01, 0.0],
            [0.0, 0.01, 0.0],
            [0.02, 0.0, 0.0],
            [0.0, 0.02, 0.001],
        ];
        // je Kante ein Dreieck in ihrer Lage, damit die Kanten Nachbarn haben
        let d = [[0, 1, 5], [2, 3, 4]];
        let netz = Stoffnetz::neu(&d, vec![1e-3; 6]);
        let n = finde(&netz, &x, &[1.0; 6], false, true);
        let kk: Vec<_> = n.iter().filter(|p| p.punkte[..2] == [0, 1]).collect();
        assert_eq!(kk.len(), 1, "{n:?}");
        assert!((kk[0].abstand - 0.001).abs() < 1e-12);
        assert!((kk[0].normale[2] - 1.0).abs() < 1e-12);
        assert_eq!(kk[0].gewichte, [0.5, 0.5, -0.5, -0.5]);
    }

    /// Ein Punkt fällt in einem Schritt 4 mm durch ein waagerechtes Dreieck:
    /// gefunden mit Zeit 0,5 und Seite oben; die Antwort setzt ihn auf 0,1 mm
    /// über die Ebene, das (festgehaltene) Dreieck bleibt.
    #[test]
    fn punkt_durch_dreieck() {
        let x0 = vec![[0.0, 0.0, 0.0], [0.02, 0.0, 0.0], [0.0, 0.02, 0.0], [0.005, 0.005, 0.002]];
        let mut x1 = x0.clone();
        x1[3][2] = -0.002;
        let netz = Stoffnetz::neu(&[[0, 1, 2]], vec![1e-3; 4]);
        let w = [0.0, 0.0, 0.0, 1.0];
        let st = finde_stoesse(&netz, &x0, &x1, &w, true, true);
        assert_eq!(st.len(), 1, "{st:?}");
        assert!((st[0].zeit - 0.5).abs() < 1e-6 && st[0].seite == 1.0, "{st:?}");
        let n = stossantwort(&st, &mut x1, &[1e-5; 4], &w, 1);
        assert_eq!(n, 1);
        assert!((x1[3][2] - STOSS_ABSTAND).abs() < 1e-12, "{:?}", x1[3]);
        assert_eq!(x1[0], x0[0]);
    }

    /// Zwei Kanten kreuzen sich in einem Schritt: die bewegte Kante kommt auf
    /// 0,1 mm vor die feste zurück.
    #[test]
    fn kante_durch_kante() {
        let x0 = vec![[-0.01, 0.0, 0.001], [0.01, 0.0, 0.001], [0.0, -0.01, 0.0], [0.0, 0.01, 0.0]];
        let mut x1 = x0.clone();
        x1[0][2] = -0.001;
        x1[1][2] = -0.001;
        let netz = Stoffnetz { kanten: vec![[0, 1], [2, 3]], ..Stoffnetz::default() };
        let w = [1.0, 1.0, 0.0, 0.0];
        let st = finde_stoesse(&netz, &x0, &x1, &w, false, true);
        assert_eq!(st.len(), 1, "{st:?}");
        stossantwort(&st, &mut x1, &[1e-5; 4], &w, 1);
        assert!((x1[0][2] - STOSS_ABSTAND).abs() < 1e-12 && (x1[1][2] - STOSS_ABSTAND).abs() < 1e-12, "{x1:?}");
    }
}
