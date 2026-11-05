//! Das Körpernetz als Kollisionskörper (Schritte 4c/4d).
//!
//! Ein geschlossenes, einheitlich orientiertes Dreiecksnetz — der Körper aus
//! `klon3d koerper`. Dessen Gruppe `body` ist **eine** Komponente mit
//! Normalen nach außen (Volumen 55,7 l bei der Maßdatei der Prüfmuster) und
//! nur an Mund und Augen offen (188 Randkanten in drei Schleifen; am
//! 27.10.2026 nachgemessen). Die 4.170 offenen Kanten aus Befund 25 gehören zu
//! den übrigen Flicken des BaseMesh, nicht zum Körper.
//!
//! Gefragt wird je Stoffpunkt der **vorzeichenbehaftete Abstand** zur
//! Oberfläche:
//!
//! 1. nächster Punkt auf jedem Dreieck in der Nähe mit seiner Region (Fläche,
//!    Kante, Ecke) nach Ericson, „Real-Time Collision Detection“ (2005),
//!    Abschnitt 5.1.5;
//! 2. Vorzeichen aus der **winkelgewichteten Pseudonormale** dieser Region
//!    (Bærentzen & Aanæs, „Signed Distance Computation Using the Angle
//!    Weighted Pseudonormal“, 2005): Fläche → Flächennormale, Kante → Summe
//!    der beiden Flächennormalen, Ecke → Flächennormalen gewichtet mit dem
//!    Innenwinkel. Für ein geschlossenes Netz stimmt das Vorzeichen damit auch
//!    dort, wo mehrere Dreiecke einen Punkt gleich nah haben;
//! 3. Kontaktnormale = Richtung vom nächsten Punkt zum Stoffpunkt, nach außen
//!    gedreht, wenn er innen liegt — der Gradient des Abstandsfelds. Die
//!    Korrektur x + n·Tiefe legt den Punkt also genau `abstand` über den
//!    nächsten Oberflächenpunkt.
//!
//! Gesucht wird nur in der Nähe: ein festes Gitter über dem Körper mit einer
//! Dreiecksliste je Zelle, als Felder (Anfang je Zelle + Liste) wie die
//! GPU-Puffer (ADR-018). Ein Punkt, der tiefer als `suchtiefe` im Körper
//! steckt, wird **nicht** mehr gefunden. Deshalb prüft der implizite Schritt
//! zusätzlich den Weg jedes Punkts im Schritt ([`Koerpernetz::durchstoss`]):
//! durchstößt er die Haut von außen, setzt er am Eintritt auf. Das gilt für
//! einen ruhenden Körper; ein bewegter (Stufe 7) braucht die Prüfung gegen die
//! Bewegung der Dreiecke.
//!
//! An offenen Rändern (Mund, Augen) hat eine Kante nur ein Dreieck; ihre
//! Pseudonormale ist dessen Flächennormale, und nahe am Rand ist das
//! Vorzeichen unsicher. Kleidung kommt dort nicht hin.

use std::collections::HashMap;

use crate::koerper::{add, cross, dot, mul, norm, sub};

/// Vorgabe der Suchtiefe (m): so tief darf ein Stoffpunkt in einem Schritt
/// eindringen, ohne verloren zu gehen.
pub const SUCHTIEFE: f64 = 0.01;

/// Wo der nächste Punkt auf einem Dreieck liegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// an Ecke k (0, 1, 2)
    Ecke(u8),
    /// auf Kante k: 0 = p0p1, 1 = p1p2, 2 = p2p0
    Kante(u8),
    /// im Inneren
    Flaeche,
}

/// Ergebnis einer Abstandsfrage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Naechster {
    /// Abstand zur Oberfläche, außen positiv (m).
    pub abstand: f64,
    /// nächster Punkt auf der Oberfläche
    pub punkt: [f64; 3],
    /// Normale nach außen (Gradient des Abstandsfelds)
    pub normale: [f64; 3],
    pub dreieck: u32,
    pub region: Region,
}

/// Gleichmäßiges Gitter über dem Körper; je Zelle die Einträge (Dreiecke
/// bzw. Punkte), deren Hüllquader sie berührt.
#[derive(Debug, Clone)]
pub struct Gitter {
    pub ursprung: [f64; 3],
    /// Kantenlänge einer Zelle (m).
    pub zelle: f64,
    pub zahl: [usize; 3],
    /// Zelle z belegt `eintraege[anfang[z]..anfang[z + 1]]`.
    pub anfang: Vec<u32>,
    pub eintraege: Vec<u32>,
}

impl Gitter {
    /// Zellindex je Achse, ungekappt.
    fn index(&self, x: [f64; 3]) -> [i64; 3] {
        let mut k = [0i64; 3];
        for a in 0..3 {
            k[a] = ((x[a] - self.ursprung[a]) / self.zelle).floor() as i64;
        }
        k
    }

    fn zellnummer(&self, k: [usize; 3]) -> usize {
        (k[2] * self.zahl[1] + k[1]) * self.zahl[0] + k[0]
    }

    /// Ruft `f` für jeden Eintrag in den Zellen, die der Quader [lo, hi]
    /// berührt (ein Eintrag kann mehrfach kommen).
    pub fn je_eintrag(&self, lo: [f64; 3], hi: [f64; 3], mut f: impl FnMut(u32)) {
        let Some(r) = self.bereich(lo, hi) else { return };
        for kz in r[2].0..=r[2].1 {
            for ky in r[1].0..=r[1].1 {
                for kx in r[0].0..=r[0].1 {
                    let z = self.zellnummer([kx, ky, kz]);
                    for &e in &self.eintraege[self.anfang[z] as usize..self.anfang[z + 1] as usize] {
                        f(e);
                    }
                }
            }
        }
    }

    /// Zellbereich je Achse für den Quader [lo, hi], gekappt; `None`, wenn er
    /// das Gitter nicht berührt.
    fn bereich(&self, lo: [f64; 3], hi: [f64; 3]) -> Option<[(usize, usize); 3]> {
        let (a, b) = (self.index(lo), self.index(hi));
        let mut r = [(0, 0); 3];
        for d in 0..3 {
            let n = self.zahl[d] as i64;
            if b[d] < 0 || a[d] >= n {
                return None;
            }
            r[d] = (a[d].max(0) as usize, b[d].min(n - 1) as usize);
        }
        Some(r)
    }

    /// Wie [`Self::bereich`], aber aus schon bestimmten Zellindizes.
    fn klammer(&self, a: [i64; 3], b: [i64; 3]) -> [(usize, usize); 3] {
        let mut r = [(0, 0); 3];
        for d in 0..3 {
            let n = self.zahl[d] as i64;
            debug_assert!(b[d] >= 0 && a[d] < n, "Punkt außerhalb des Gitters");
            r[d] = (a[d].max(0) as usize, b[d].min(n - 1) as usize);
        }
        r
    }
}

/// Das Körpernetz mit allem, was die Abstandsfrage braucht (SI-Einheiten,
/// Szenenrahmen: z oben).
#[derive(Debug, Clone)]
pub struct Koerpernetz {
    pub punkte: Vec<[f64; 3]>,
    pub dreiecke: Vec<[u32; 3]>,
    pub flaechennormale: Vec<[f64; 3]>,
    /// Je Dreieck und Kante das andere Dreieck daran, oder das Dreieck selbst
    /// bei einer Randkante. Reine Topologie: ändert sich beim Verformen nicht.
    /// Damit sind die Kantennormalen (siehe [`Self::kantennormale`]) eine
    /// Abfrage statt eines Vorrats: eine Abstandsfrage braucht **eine**, nicht
    /// alle 80 268.
    pub nachbar: Vec<[u32; 3]>,
    /// Winkelgewichtete Pseudonormale je Punkt (normiert).
    pub punktnormale: Vec<[f64; 3]>,
    /// Hüllkugel je Dreieck: Schwerpunkt und größter Eckabstand davon.
    pub kugel: Vec<([f64; 3], f64)>,
    pub suchtiefe: f64,
    /// Gitter der Dreiecke (Abstandsfrage, Durchstoß) und der Punkte
    /// (Körperpunkt gegen Stoffdreieck), gleich aufgeteilt.
    pub gitter: Gitter,
    pub punktgitter: Gitter,
    /// Kanten mit nur einem Dreieck (offene Ränder).
    pub randkanten: usize,
    /// Umschlossenes Volumen (m³), positiv bei Normalen nach außen.
    pub volumen: f64,
    /// Geschwindigkeit je **Punkt** (m/s), leer bei einem starren oder
    /// ruhenden Körper (Stufe 7, #96). Bei einem verformten Körper ist ein
    /// einziger Wert für alle falsch: an der Schulter wandert die Haut
    /// langsam, an der Hand schnell, und die Reibung rechnet mit dem
    /// Unterschied zum Stoff. Das Feld gehört zum Punktstand in `punkte`;
    /// [`Self::nachfuehren`] räumt es ab, weil es dann veraltet ist.
    pub punktgeschwindigkeit: Vec<[f64; 3]>,
}

/// Ein Punkt aus CLOs Rahmen (mm, y oben, vorn +z) im Szenenrahmen (m, z oben,
/// vorn −y): Drehung um +90° um x.
pub fn clo_nach_szene(q: [f64; 3]) -> [f64; 3] {
    [q[0] * 1e-3, -q[2] * 1e-3, q[1] * 1e-3]
}

/// Nächster Punkt auf dem Dreieck (a, b, c) zu p und seine Region (Ericson
/// 2005, 5.1.5).
pub fn naechster_auf_dreieck(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> ([f64; 3], Region) {
    let quot = |z: f64, n: f64| if n > 0.0 { z / n } else { 0.0 };
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, Region::Ecke(0));
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (b, Region::Ecke(1));
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return (add(a, mul(ab, quot(d1, d1 - d3))), Region::Kante(0));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (c, Region::Ecke(2));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return (add(a, mul(ac, quot(d2, d2 - d6))), Region::Kante(2));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = quot(d4 - d3, (d4 - d3) + (d5 - d6));
        return (add(b, mul(sub(c, b), w)), Region::Kante(1));
    }
    let s = va + vb + vc;
    let (v, w) = (quot(vb, s), quot(vc, s));
    (add(a, add(mul(ab, v), mul(ac, w))), Region::Flaeche)
}

fn normiert(a: [f64; 3]) -> [f64; 3] {
    let l = norm(a);
    if l > 0.0 {
        mul(a, 1.0 / l)
    } else {
        [0.0; 3]
    }
}

/// Die drei Schwerpunktgewichte von `p` auf dem Dreieck (a, b, c) aus der
/// Projektion auf die Dreiecksebene; die Summe ist 1, ein Punkt außerhalb darf
/// negative Gewichte bekommen (Extrapolation).
fn schwerpunktgewichte(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let (e1, e2, e3) = (sub(b, a), sub(c, a), sub(p, a));
    let (d00, d01, d11) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let (d20, d21) = (dot(e3, e1), dot(e3, e2));
    let nenner = d00 * d11 - d01 * d01;
    if nenner == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let u = (d11 * d20 - d01 * d21) / nenner;
    let w = (d00 * d21 - d01 * d20) / nenner;
    [1.0 - u - w, u, w]
}

/// Winkel zwischen u und v (rad).
fn winkel(u: [f64; 3], v: [f64; 3]) -> f64 {
    norm(cross(u, v)).atan2(dot(u, v))
}

/// Achtung: `punkte` ist der *neue* Punktstand, die Dreiecke sind Topologie.
/// Alles, was von den Punkten abhängt, in einem Durchgang — der gemeinsame Weg
/// von [`Koerpernetz::neu`] und [`Koerpernetz::nachfuehren`], damit beide auf
/// denselben Punkten Bit für Bit dasselbe liefern.
///
/// Schreibt in die übergebenen Puffer: beim Nachführen sind das die Felder des
/// Netzes selbst, also fallen je Bild keine 3,5 MB Allokation an.
fn punktabhaengig(
    punkte: &[[f64; 3]],
    dreiecke: &[[u32; 3]],
    flaechennormale: &mut Vec<[f64; 3]>,
    punktnormale: &mut Vec<[f64; 3]>,
    kugel: &mut Vec<([f64; 3], f64)>,
) -> f64 {
    flaechennormale.clear();
    flaechennormale.extend(dreiecke.iter().map(|d| {
        let [a, b, c] = d.map(|i| punkte[i as usize]);
        normiert(cross(sub(b, a), sub(c, a)))
    }));

    punktnormale.clear();
    punktnormale.resize(punkte.len(), [0.0; 3]);
    let mut volumen = 0.0;
    kugel.clear();
    kugel.reserve(dreiecke.len());
    for (t, d) in dreiecke.iter().enumerate() {
        let q = d.map(|i| punkte[i as usize]);
        for k in 0..3 {
            let w = winkel(sub(q[(k + 1) % 3], q[k]), sub(q[(k + 2) % 3], q[k]));
            let p = d[k] as usize;
            punktnormale[p] = add(punktnormale[p], mul(flaechennormale[t], w));
        }
        volumen += dot(q[0], cross(q[1], q[2])) / 6.0;
        let m = mul(add(add(q[0], q[1]), q[2]), 1.0 / 3.0);
        let r = q.iter().map(|&e| norm(sub(e, m))).fold(0.0, f64::max);
        kugel.push((m, r));
    }
    for p in punktnormale.iter_mut() {
        *p = normiert(*p);
    }
    volumen
}

impl Koerpernetz {
    /// Baut das Netz aus Punkten (m, z oben) und Dreiecken. Verlangt ein
    /// mannigfaltiges, einheitlich orientiertes Netz mit Normalen nach außen;
    /// offene Ränder sind erlaubt und werden gezählt.
    pub fn neu(punkte: Vec<[f64; 3]>, dreiecke: Vec<[u32; 3]>, suchtiefe: f64) -> Result<Koerpernetz, String> {
        if dreiecke.is_empty() {
            return Err("Körpernetz ohne Dreiecke".into());
        }
        if !(suchtiefe > 0.0) {
            return Err(format!("Suchtiefe {suchtiefe} m muss positiv sein"));
        }
        let n = punkte.len();
        if let Some(d) = dreiecke.iter().find(|d| d.iter().any(|&i| i as usize >= n)) {
            return Err(format!("Dreieck {d:?} verweist auf einen Punkt ≥ {n}"));
        }

        // Kanten: je ungerichteter Kante die Dreiecke mit Kantennummer und
        // Laufrichtung.
        let mut kanten: HashMap<(u32, u32), Vec<(u32, u8, bool)>> = HashMap::new();
        for (t, d) in dreiecke.iter().enumerate() {
            for k in 0..3u8 {
                let (i, j) = (d[k as usize], d[(k as usize + 1) % 3]);
                kanten.entry((i.min(j), i.max(j))).or_default().push((t as u32, k, i < j));
            }
        }

        // Aus der Kantentabelle die Nachbarschaft je Dreieckseite - das ist
        // Topologie und wird nie wieder angefasst.
        let mut nachbar = vec![[0u32; 3]; dreiecke.len()];
        let mut randkanten = 0;
        for (&(i, j), liste) in &kanten {
            match liste[..] {
                [(t, k, _)] => {
                    randkanten += 1;
                    nachbar[t as usize][k as usize] = t;
                }
                [(t1, k1, r1), (t2, k2, r2)] => {
                    if r1 == r2 {
                        return Err(format!("Netz uneinheitlich orientiert an der Kante {i}–{j}"));
                    }
                    nachbar[t1 as usize][k1 as usize] = t2;
                    nachbar[t2 as usize][k2 as usize] = t1;
                }
                _ => return Err(format!("Kante {i}–{j} hat {} Dreiecke", liste.len())),
            }
        }

        let (mut flaechennormale, mut punktnormale, mut kugel) = (Vec::new(), Vec::new(), Vec::new());
        let volumen = punktabhaengig(
            &punkte,
            &dreiecke,
            &mut flaechennormale,
            &mut punktnormale,
            &mut kugel,
        );
        if volumen <= 0.0 {
            return Err(format!("Volumen {volumen:.4e} m³ ≤ 0: die Normalen zeigen nach innen"));
        }

        let mut gitter = Gitter { ursprung: [0.0; 3], zelle: 0.0, zahl: [0; 3], anfang: Vec::new(), eintraege: Vec::new() };
        let mut punktgitter = gitter.clone();
        gitter_beide(&mut gitter, &mut punktgitter, suchtiefe, &punkte, &dreiecke);
        Ok(Koerpernetz {
            punkte,
            dreiecke,
            flaechennormale,
            nachbar,
            punktnormale,
            kugel,
            suchtiefe,
            gitter,
            punktgitter,
            randkanten,
            volumen,
            punktgeschwindigkeit: Vec::new(),
        })
    }

    /// Führt das Netz auf neue Punkte nach, ohne die Topologie neu zu bauen
    /// (Stufe 7, Bewegung): Dreiecksliste, Nachbarschaft, Randkanten und die
    /// Gitteraufteilung bleiben, neu gerechnet wird nur, was von den Punkten
    /// abhängt — Flächen- und Punktnormalen, Hüllkugeln, Volumen und die
    /// Zellbelegung beider Gitter. Die Kantennormalen fallen ganz weg: sie sind
    /// eine Abfrage ([`Self::kantennormale`]), keine Vorberechnung.
    ///
    /// Die Gitter wachsen mit, wenn der verformte Körper über den alten
    /// Hüllquader hinausreicht, und schrumpfen **nicht**: sonst folgte die
    /// Zellenzahl der Pose, und ein Wechsel der Pose änderte die Abfragekosten.
    ///
    /// `punkte` hat dieselbe Zahl und Reihenfolge wie bei [`Self::neu`]. Die
    /// Werte sind auf denselben Punkten Bit für Bit die von
    /// [`Koerpernetz::aus_clo_mm`] — beide Wege laufen durch
    /// [`punktabhaengig`].
    ///
    /// Eine gesetzte [`Self::punktgeschwindigkeit`] wird **abgeräumt**: sie
    /// gehörte zum alten Punktstand. Ein bewegter Körper setzt sie danach neu
    /// ([`Self::setze_punktgeschwindigkeit`]).
    pub fn nachfuehren(&mut self, punkte: &[[f64; 3]]) -> Result<(), String> {
        if punkte.len() != self.punkte.len() {
            return Err(format!("nachfuehren: {} Punkte statt {}", punkte.len(), self.punkte.len()));
        }
        // Die vorhandenen Puffer übernehmen (nicht kopieren): `punktabhaengig`
        // leert sie und füllt sie neu, die Allokation bleibt also bestehen.
        let mut flaechennormale = std::mem::take(&mut self.flaechennormale);
        let mut punktnormale = std::mem::take(&mut self.punktnormale);
        let mut kugel = std::mem::take(&mut self.kugel);
        let volumen = punktabhaengig(punkte, &self.dreiecke, &mut flaechennormale, &mut punktnormale, &mut kugel);
        self.flaechennormale = flaechennormale;
        self.punktnormale = punktnormale;
        self.kugel = kugel;
        if volumen <= 0.0 {
            return Err(format!("Volumen {volumen:.4e} m³ ≤ 0 nach dem Verformen: das Netz ist gekippt"));
        }
        self.punkte.clear();
        self.punkte.extend_from_slice(punkte);
        self.volumen = volumen;
        self.punktgeschwindigkeit.clear();

        // Beide Gitter in einem Zug: dasselbe Raster, ein Zellenlauf.
        gitter_beide(&mut self.gitter, &mut self.punktgitter, self.suchtiefe, punkte, &self.dreiecke);
        Ok(())
    }

    /// Aus einem Netz im Rahmen von CLO und `klon3d koerper`: mm, y oben, vorn
    /// +z. Gedreht um +90° um x in den Szenenrahmen (z oben, vorn −y) und in m
    /// umgerechnet: (x, y, z) → (x, −z, y)·10⁻³. Die Drehung erhält die
    /// Orientierung der Dreiecke.
    pub fn aus_clo_mm(punkte_mm: &[[f64; 3]], dreiecke: &[[usize; 3]], suchtiefe: f64) -> Result<Koerpernetz, String> {
        let p = punkte_mm.iter().map(|&q| clo_nach_szene(q)).collect();
        let d = dreiecke.iter().map(|d| d.map(|i| i as u32)).collect();
        Koerpernetz::neu(p, d, suchtiefe)
    }

    /// Die Pseudonormale der Kante k am Dreieck t: an einer Randkante die
    /// eigene Flächennormale, sonst die normierte Summe mit der des Nachbarn.
    /// Wird auf Abruf gerechnet — eine Abstandsfrage braucht eine, nicht alle
    /// 80 268, und die Summe ist symmetrisch, also liefern beide Nachbarn
    /// dasselbe.
    pub fn kantennormale(&self, t: usize, k: usize) -> [f64; 3] {
        let o = self.nachbar[t][k] as usize;
        if o == t {
            self.flaechennormale[t]
        } else {
            normiert(add(self.flaechennormale[t], self.flaechennormale[o]))
        }
    }

    /// Die Pseudonormale der Region.
    fn pseudonormale(&self, t: usize, r: Region) -> [f64; 3] {
        match r {
            Region::Flaeche => self.flaechennormale[t],
            Region::Kante(k) => self.kantennormale(t, k as usize),
            Region::Ecke(k) => self.punktnormale[self.dreiecke[t][k as usize] as usize],
        }
    }

    /// Setzt die Geschwindigkeit je Punkt (m/s) — das Feld, das ein sich
    /// verformender Körper statt **einer** Geschwindigkeit braucht (#96).
    ///
    /// Verlangt einen Wert je Punkt, sonst wäre die Zuordnung still falsch.
    /// Leer räumt das Feld ab und macht den Körper wieder starr.
    pub fn setze_punktgeschwindigkeit(&mut self, v: Vec<[f64; 3]>) -> Result<(), String> {
        if !v.is_empty() && v.len() != self.punkte.len() {
            return Err(format!(
                "Geschwindigkeit für {} Punkte, das Netz hat {}",
                v.len(),
                self.punkte.len()
            ));
        }
        self.punktgeschwindigkeit = v;
        Ok(())
    }

    /// Geschwindigkeit der Oberfläche am Punkt `p` auf Dreieck `t` (m/s):
    /// linear aus den Eckgeschwindigkeiten gemischt, nach denselben
    /// Schwerpunktgewichten wie [`naechster_auf_dreieck`] sie benutzt. An der
    /// Schulter ist die Haut langsam, an der Hand schnell — ein einziger Wert
    /// für den ganzen Körper wäre dort um mehr falsch, als die Reibung groß ist.
    ///
    /// `starr` gilt, solange [`Self::punktgeschwindigkeit`] leer ist — der
    /// eine Wert eines ruhenden oder nur verschobenen Körpers.
    pub fn geschwindigkeit_an(&self, t: usize, p: [f64; 3], starr: [f64; 3]) -> [f64; 3] {
        if self.punktgeschwindigkeit.is_empty() {
            return starr;
        }
        let d = self.dreiecke[t];
        let [a, b, c] = d.map(|i| self.punkte[i as usize]);
        let gew = schwerpunktgewichte(p, a, b, c);
        let mut q = [0.0f64; 3];
        for (j, &i) in d.iter().enumerate() {
            let v = self.punktgeschwindigkeit[i as usize];
            for k in 0..3 {
                q[k] += gew[j] * v[k];
            }
        }
        q
    }

    /// Ergebnis für Dreieck t mit nächstem Punkt c in Region r.
    fn ergebnis(&self, x: [f64; 3], t: usize, c: [f64; 3], r: Region) -> Naechster {
        let pn = self.pseudonormale(t, r);
        let d = sub(x, c);
        let l = norm(d);
        let aussen = dot(d, pn) >= 0.0;
        let normale = if l > 1e-12 { mul(d, if aussen { 1.0 / l } else { -1.0 / l }) } else { pn };
        Naechster { abstand: if aussen { l } else { -l }, punkt: c, normale, dreieck: t as u32, region: r }
    }

    /// Nächster Oberflächenpunkt näher als `radius` (ohne Vorzeichen
    /// gemessen); `None`, wenn keiner so nah liegt.
    pub fn naechster(&self, x: [f64; 3], radius: f64) -> Option<Naechster> {
        let g = &self.gitter;
        let r = g.bereich(sub(x, [radius; 3]), add(x, [radius; 3]))?;
        let mut best = radius;
        let mut treffer: Option<(usize, [f64; 3], Region)> = None;
        for kz in r[2].0..=r[2].1 {
            for ky in r[1].0..=r[1].1 {
                for kx in r[0].0..=r[0].1 {
                    let z = g.zellnummer([kx, ky, kz]);
                    for &t in &g.eintraege[g.anfang[z] as usize..g.anfang[z + 1] as usize] {
                        let t = t as usize;
                        let (m, rk) = self.kugel[t];
                        if norm(sub(x, m)) - rk >= best {
                            continue;
                        }
                        let [a, b, c] = self.dreiecke[t].map(|i| self.punkte[i as usize]);
                        let (q, reg) = naechster_auf_dreieck(x, a, b, c);
                        let l = norm(sub(x, q));
                        if l < best {
                            best = l;
                            treffer = Some((t, q, reg));
                        }
                    }
                }
            }
        }
        treffer.map(|(t, q, reg)| self.ergebnis(x, t, q, reg))
    }

    /// Erster Durchstoß der Strecke a → b durch die Oberfläche **von außen nach
    /// innen** (Möller & Trumbore, „Fast, Minimum Storage Ray/Triangle
    /// Intersection“, 1997): Punkt und Flächennormale. Fängt Stoffpunkte, die
    /// in einem Schritt tiefer als die Suchtiefe oder ganz durch ein dünnes Teil
    /// (Arm, Finger) laufen würden; Austritte von innen zählen nicht.
    ///
    /// Dritter Wert ist die Geschwindigkeit der Oberfläche am Treffpunkt
    /// ([`Self::geschwindigkeit_an`]) — beim verformten Körper je Stelle
    /// verschieden, sonst der Wert des ganzen Körpers.
    pub fn durchstoss(&self, a: [f64; 3], b: [f64; 3], starr: [f64; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
        let d = sub(b, a);
        if dot(d, d) < 1e-24 {
            return None;
        }
        let mut lo = a;
        let mut hi = a;
        for k in 0..3 {
            lo[k] = lo[k].min(b[k]);
            hi[k] = hi[k].max(b[k]);
        }
        let g = &self.gitter;
        let r = g.bereich(lo, hi)?;
        let mut best: Option<(f64, usize)> = None;
        // Ränder leicht einschließen, damit ein Strahl durch eine gemeinsame
        // Kante nicht beiden Dreiecken entgeht.
        let eps = 1e-12;
        for kz in r[2].0..=r[2].1 {
            for ky in r[1].0..=r[1].1 {
                for kx in r[0].0..=r[0].1 {
                    let z = g.zellnummer([kx, ky, kz]);
                    for &t in &g.eintraege[g.anfang[z] as usize..g.anfang[z + 1] as usize] {
                        let t = t as usize;
                        if dot(d, self.flaechennormale[t]) >= 0.0 {
                            continue;
                        }
                        let [p0, p1, p2] = self.dreiecke[t].map(|i| self.punkte[i as usize]);
                        let (e1, e2) = (sub(p1, p0), sub(p2, p0));
                        let pv = cross(d, e2);
                        let det = dot(e1, pv);
                        if det.abs() < 1e-30 {
                            continue;
                        }
                        let tv = sub(a, p0);
                        let u = dot(tv, pv) / det;
                        if u < -eps || u > 1.0 + eps {
                            continue;
                        }
                        let qv = cross(tv, e1);
                        let v = dot(d, qv) / det;
                        if v < -eps || u + v > 1.0 + eps {
                            continue;
                        }
                        let s = dot(e2, qv) / det;
                        if (-eps..=1.0 + eps).contains(&s) && best.is_none_or(|(sb, _)| s < sb) {
                            best = Some((s, t));
                        }
                    }
                }
            }
        }
        best.map(|(s, t)| {
            let p = add(a, mul(d, s.clamp(0.0, 1.0)));
            (p, self.flaechennormale[t], self.geschwindigkeit_an(t, p, starr))
        })
    }

    /// **Körperpunkt gegen Stoffdreieck** (CLO:
    /// `bSolidVertexToClothTriangleCollisionHandling`, Befund 49): Ein
    /// Körperpunkt darf nicht durch den Stoff stechen. Für jeden Körperpunkt p
    /// mit Normale n und jedes Stoffdreieck in der Nähe wird die Gerade
    /// p + t·n mit dem Dreieck geschnitten (Möller–Trumbore); liegt der
    /// Schnitt bei −`tief` < t < 0 — das Dreieck ist unter dem Körperpunkt,
    /// der Körper sticht durch die Mittelfläche des Stoffs —, wird das
    /// Dreieck an dieser Stelle auf t = 0 gehoben: Verschiebung −t·n, verteilt nach
    /// Schwerpunktgewichten und inverser Masse wie ein Punkt-Dreieck-Kontakt
    /// bei PBD; die Geschwindigkeit des Schnittpunkts relativ zum Körper in
    /// den Körper hinein entfällt ebenso. Das fängt, was die Stoffpunkte allein
    /// nicht sehen: Kanten, die zwischen zwei Stoffpunkten durch Schulter,
    /// Armansatz oder Fingerspitze schneiden.
    ///
    /// Nur auf t = 0, nicht auf den Kollisionsabstand: Ein ebenes Dreieck aus
    /// 10-mm-Kanten hängt auf einem Arm von 4,5 cm Radius in der Mitte 0,28 mm
    /// durch — mehr als die halbe Dicke. Auf volle Dicke gehoben, verloren
    /// seine Ecken den Kontakt, die Spannung zog sie durch die Haut zurück, und
    /// das wiederholte sich jedes Bild (Bluse3 v2: 390 Hebungen und 500
    /// Durchstöße je Bild in Ruhe).
    ///
    /// `x`, `v`, `w` sind Lage, Geschwindigkeit und inverse Masse der
    /// Stoffpunkte; `versatz` und `geschwindigkeit` gelten für den **ganzen**
    /// Körper. Ist [`Self::punktgeschwindigkeit`] gesetzt, gilt sie je
    /// gestochenem Körperpunkt und ersetzt `geschwindigkeit` (#96): an der
    /// Schulter langsam, an der Hand schnell. Liefert die Zahl der Korrekturen.
    #[allow(clippy::too_many_arguments)]
    pub fn punkte_gegen_dreiecke(
        &self,
        tief: f64,
        versatz: [f64; 3],
        geschwindigkeit: [f64; 3],
        dreiecke: &[[u32; 3]],
        w: &[f64],
        x: &mut [[f64; 3]],
        v: &mut [[f64; 3]],
    ) -> usize {
        // Je Dreieck der tiefste Stich; danach ist das Dreieck schon gehoben.
        // Der Körperpunkt steht dabei, weil seine Geschwindigkeit zählt.
        let mut tiefster: Vec<Option<(f64, [f64; 3], [f64; 3], u32)>> = vec![None; dreiecke.len()];
        // Rundung: was schon gehoben ist, liegt bei t ≈ 0.
        self.stiche(tief, -1e-9, versatz, dreiecke, x, |di, kir, bary, n, t| {
            if tiefster[di].is_none_or(|(d, _, _, _)| -t > d) {
                tiefster[di] = Some((-t, n, bary, kir));
            }
        });
        let mut zahl = 0;
        for (d, stich) in dreiecke.iter().zip(tiefster) {
            let Some((delta, n, bary, kir)) = stich else { continue };
            let idx = d.map(|i| i as usize);
            let gew = idx.map(|i| w[i]);
            let nenner: f64 = (0..3).map(|j| bary[j] * bary[j] * gew[j]).sum();
            if nenner <= 0.0 {
                continue;
            }
            let vk = self.geschwindigkeit_je_punkt(kir as usize, geschwindigkeit);
            let vq = sub((0..3).fold([0.0; 3], |s, j| add(s, mul(v[idx[j]], bary[j]))), vk);
            let vn = dot(vq, n).min(0.0);
            for j in 0..3 {
                let f = bary[j] * gew[j] / nenner;
                x[idx[j]] = add(x[idx[j]], mul(n, f * delta));
                v[idx[j]] = sub(v[idx[j]], mul(n, f * vn));
            }
            zahl += 1;
        }
        zahl
    }

    /// Geschwindigkeit des Körperpunkts `k`: das Feld, wenn gesetzt, sonst der
    /// eine Wert des ganzen Körpers.
    fn geschwindigkeit_je_punkt(&self, k: usize, starr: [f64; 3]) -> [f64; 3] {
        if self.punktgeschwindigkeit.is_empty() {
            starr
        } else {
            self.punktgeschwindigkeit[k]
        }
    }

    /// Alle Stiche: für jedes Stoffdreieck (Index in `dreiecke`) jeder
    /// Körperpunkt, dessen Gerade p + t·n das Dreieck bei −`tief` < t <
    /// `band` trifft; gemeldet mit dem **Körperpunkt**, den
    /// Schwerpunktgewichten des Treffpunkts, der Normale und t.
    #[allow(clippy::too_many_arguments)]
    pub fn stiche(
        &self,
        tief: f64,
        band: f64,
        versatz: [f64; 3],
        dreiecke: &[[u32; 3]],
        x: &[[f64; 3]],
        mut f: impl FnMut(usize, u32, [f64; 3], [f64; 3], f64),
    ) {
        let rand = tief.max(band);
        for (di, d) in dreiecke.iter().enumerate() {
            let idx = d.map(|i| i as usize);
            let [a, b, c] = idx.map(|i| x[i]);
            let mut lo = a;
            let mut hi = a;
            for q in [b, c] {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            let lo = sub(sub(lo, versatz), [rand; 3]);
            let hi = add(sub(hi, versatz), [rand; 3]);
            let (e1, e2) = (sub(b, a), sub(c, a));
            self.punktgitter.je_eintrag(lo, hi, |k| {
                let k = k as usize;
                let p = add(self.punkte[k], versatz);
                let n = self.punktnormale[k];
                let pv = cross(n, e2);
                let det = dot(e1, pv);
                if det.abs() < 1e-18 {
                    return;
                }
                let tv = sub(p, a);
                let bu = dot(tv, pv) / det;
                if !(0.0..=1.0).contains(&bu) {
                    return;
                }
                let qv = cross(tv, e1);
                let bv = dot(n, qv) / det;
                if bv < 0.0 || bu + bv > 1.0 {
                    return;
                }
                let t = dot(e2, qv) / det;
                if t > -tief && t < band {
                    f(di, k as u32, [1.0 - bu - bv, bu, bv], n, t);
                }
            });
        }
    }

    /// Vorzeichenbehafteter Abstand ohne Obergrenze: wie [`Self::naechster`],
    /// aber der Suchradius wächst, bis ein Dreieck gefunden ist. Für die
    /// Abstandskarte ([`crate::abstand`]) — ein stehender Ärmel ist 30 mm von
    /// der Haut weg, die Suchtiefe des Kolliders ist 10 mm.
    ///
    /// Das Gitter wird nicht neu gebaut: der Würfel um `x` wächst, bis er ein
    /// Dreieck enthält. Alles, was weiter als der Hüllquader entfernt ist,
    /// beantwortet [`Self::naechster_alle`] — der einzige Fall, in dem die
    /// Abfrage die Dreiecksliste ganz durchläuft.
    pub fn abstand_ohne_grenze(&self, x: [f64; 3]) -> Naechster {
        let g = &self.gitter;
        // Abstand zur fernsten Ecke des Gitters: weiter kann kein Dreieck
        // liegen, das noch im Gitter steht.
        let mut quadrat = 0.0;
        for k in 0..3 {
            let e = g.ursprung[k] + g.zelle * g.zahl[k] as f64;
            let d = (x[k] - g.ursprung[k]).abs().max((x[k] - e).abs());
            quadrat += d * d;
        }
        let weit = quadrat.sqrt() + g.zelle;
        let mut r = self.suchtiefe.max(g.zelle);
        loop {
            if g.bereich(sub(x, [r; 3]), add(x, [r; 3])).is_some() {
                // `naechster` liefert nur, was näher als `r` liegt — und alles
                // Nähere steht in den Zellen, die der Würfel berührt.
                if let Some(n) = self.naechster(x, r) {
                    return n;
                }
            }
            if r >= weit {
                return self.naechster_alle(x);
            }
            r = (r * 2.0).min(weit);
        }
    }

    /// Nächster Oberflächenpunkt über alle Dreiecke, ohne Gitter — der
    /// Vergleich für die Tests.
    pub fn naechster_alle(&self, x: [f64; 3]) -> Naechster {
        let mut best = (f64::INFINITY, 0usize, [0.0; 3], Region::Flaeche);
        for (t, d) in self.dreiecke.iter().enumerate() {
            let [a, b, c] = d.map(|i| self.punkte[i as usize]);
            let (q, reg) = naechster_auf_dreieck(x, a, b, c);
            let l = norm(sub(x, q));
            if l < best.0 {
                best = (l, t, q, reg);
            }
        }
        self.ergebnis(x, best.1, best.2, best.3)
    }
}

/// Hüllquader aller Punkte (je Achse kleinstes und größtes).
fn huelle_punkte(punkte: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in punkte {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    (lo, hi)
}

/// Zellspanne eines Dreiecks aus schon bestimmten Eckzellen: das Raster ist
/// achsenparallel und `floor` monoton, also ist die Spanne der kleinsten und
/// größten Eckzelle genau die Spanne des Hüllquaders.
fn dreieck_klammer(zellen: &[[i64; 3]], d: &[u32; 3]) -> ([i64; 3], [i64; 3]) {
    let (mut lo, mut hi) = (zellen[d[0] as usize], zellen[d[0] as usize]);
    for &i in &d[1..] {
        let k = zellen[i as usize];
        for a in 0..3 {
            lo[a] = lo[a].min(k[a]);
            hi[a] = hi[a].max(k[a]);
        }
    }
    (lo, hi)
}

/// Gitter mit Zellen der Kantenlänge 2·Suchtiefe über dem Hüllquader der
/// Punkte, um die Suchtiefe erweitert; jedes Dreieck steht in allen Zellen, die
/// sein Hüllquader berührt. Eine Abfrage mit Radius ≤ Suchtiefe berührt so
/// höchstens 2×2×2 Zellen.
///
/// Das Dreiecksgitter und das Punktgitter liegen **auf demselben Raster** (es
/// hängt nur an den Punkten und der Suchtiefe), deshalb bestimmt
/// [`gitter_beide`] es einmal für beide und füllt sie danach getrennt.
fn gitter_beide(g: &mut Gitter, pg: &mut Gitter, suchtiefe: f64, punkte: &[[f64; 3]], dreiecke: &[[u32; 3]]) {
    let (lo, hi) = huelle_punkte(punkte);
    let zelle = 2.0 * suchtiefe;
    let bedarf_lo = sub(lo, [suchtiefe; 3]);
    let mut bedarf_hi = hi;
    for a in 0..3 {
        bedarf_hi[a] += suchtiefe;
    }
    // An den alten Kasten beider Gitter anschließen: das Raster schrumpft nie,
    // sonst hinge die Zellenzahl an der Pose und ein Posenwechsel änderte die
    // Abfragekosten. Ein leeres Gitter (`zahl == [0; 3]`) hat keinen Kasten.
    let mut ursprung = bedarf_lo;
    let mut ende = bedarf_hi;
    for alt in [&*g, &*pg] {
        if alt.zahl == [0; 3] {
            continue;
        }
        for a in 0..3 {
            ursprung[a] = ursprung[a].min(alt.ursprung[a]);
            ende[a] = ende[a].max(alt.ursprung[a] + alt.zelle * alt.zahl[a] as f64);
        }
    }
    let mut zahl = [1usize; 3];
    for a in 0..3 {
        zahl[a] = (((ende[a] - ursprung[a]) / zelle).ceil() as usize).max(1);
    }
    for gg in [&mut *g, &mut *pg] {
        gg.ursprung = ursprung;
        gg.zelle = zelle;
        gg.zahl = zahl;
    }

    // Eckzellen der Punkte einmal bestimmen: das Punktgitter braucht sie als
    // Eintrag, das Dreiecksgitter als Spannenendpunkte.
    let zellen: Vec<[i64; 3]> = punkte.iter().map(|&p| g.index(p)).collect();
    let nz = zahl[0] * zahl[1] * zahl[2];
    gitter_punkte(pg, &zellen, nz);
    gitter_dreiecke(g, &zellen, dreiecke, nz);
}

/// Belegt ein Gitter, dessen Einträge je **eine** Zelle belegen (Punkte).
fn gitter_punkte(g: &mut Gitter, zellen: &[[i64; 3]], nz: usize) {
    leere_tabelle(g, nz);
    for k in zellen {
        let r = g.klammer(*k, *k);
        let z = g.zellnummer([r[0].0, r[1].0, r[2].0]);
        g.anfang[z + 1] += 1;
    }
    summiere(g, nz);
    g.eintraege.clear();
    g.eintraege.resize(g.anfang[nz] as usize, 0);
    let mut frei = g.anfang.clone();
    for (e, k) in zellen.iter().enumerate() {
        let r = g.klammer(*k, *k);
        let z = g.zellnummer([r[0].0, r[1].0, r[2].0]);
        g.eintraege[frei[z] as usize] = e as u32;
        frei[z] += 1;
    }
}

/// Belegt ein Gitter, dessen Einträge einen Zellbereich belegen (Dreiecke).
fn gitter_dreiecke(g: &mut Gitter, zellen: &[[i64; 3]], dreiecke: &[[u32; 3]], nz: usize) {
    leere_tabelle(g, nz);
    for d in dreiecke {
        let (a, b) = dreieck_klammer(zellen, d);
        let r = g.klammer(a, b);
        for kz in r[2].0..=r[2].1 {
            for ky in r[1].0..=r[1].1 {
                for kx in r[0].0..=r[0].1 {
                    let z = g.zellnummer([kx, ky, kz]);
                    g.anfang[z + 1] += 1;
                }
            }
        }
    }
    summiere(g, nz);
    g.eintraege.clear();
    g.eintraege.resize(g.anfang[nz] as usize, 0);
    let mut frei = g.anfang.clone();
    for (e, d) in dreiecke.iter().enumerate() {
        let (a, b) = dreieck_klammer(zellen, d);
        let r = g.klammer(a, b);
        for kz in r[2].0..=r[2].1 {
            for ky in r[1].0..=r[1].1 {
                for kx in r[0].0..=r[0].1 {
                    let z = g.zellnummer([kx, ky, kz]);
                    g.eintraege[frei[z] as usize] = e as u32;
                    frei[z] += 1;
                }
            }
        }
    }
}

/// Setzt die Zählertabelle auf null, ohne sie je Bild neu anzulegen.
fn leere_tabelle(g: &mut Gitter, nz: usize) {
    if g.anfang.len() != nz + 1 {
        g.anfang = vec![0u32; nz + 1];
    } else {
        g.anfang.iter_mut().for_each(|v| *v = 0);
    }
}

/// Wandelt die Zähler in Anfangspositionen um: `anfang[z]` ist danach der
/// erste Platz der Zelle z.
fn summiere(g: &mut Gitter, nz: usize) {
    for z in 0..nz {
        g.anfang[z + 1] += g.anfang[z];
    }
}
/// Geschlossener Zylinder (Achse z) als Netz, Normalen nach außen:
/// `segmente` Punkte je Ring, `ringe` Abschnitte zwischen `z_unten` und
/// `z_oben`, Deckel als Fächer. Der Prüffall Röhre rechnet damit gegen den
/// analytischen Zylinder.
pub fn zylinder(radius: f64, z_unten: f64, z_oben: f64, segmente: usize, ringe: usize) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let (s, r) = (segmente.max(3), ringe.max(1));
    let mut p = Vec::with_capacity((r + 1) * s + 2);
    for i in 0..=r {
        let z = z_unten + (z_oben - z_unten) * i as f64 / r as f64;
        for j in 0..s {
            let w = 2.0 * std::f64::consts::PI * j as f64 / s as f64;
            p.push([radius * w.cos(), radius * w.sin(), z]);
        }
    }
    let unten = p.len() as u32;
    p.push([0.0, 0.0, z_unten]);
    let oben = p.len() as u32;
    p.push([0.0, 0.0, z_oben]);
    let id = |i: usize, j: usize| (i * s + j % s) as u32;
    let mut d = Vec::with_capacity(2 * r * s + 2 * s);
    for i in 0..r {
        for j in 0..s {
            let (a, b, c, e) = (id(i, j), id(i, j + 1), id(i + 1, j + 1), id(i + 1, j));
            d.push([a, b, c]);
            d.push([a, c, e]);
        }
    }
    for j in 0..s {
        d.push([unten, id(0, j + 1), id(0, j)]);
        d.push([oben, id(r, j), id(r, j + 1)]);
    }
    (p, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Würfel [−h, h]³, Normalen nach außen.
    fn wuerfel(h: f64) -> Koerpernetz {
        let p: Vec<[f64; 3]> = (0..8)
            .map(|i| [if i & 1 == 0 { -h } else { h }, if i & 2 == 0 { -h } else { h }, if i & 4 == 0 { -h } else { h }])
            .collect();
        // je Seite zwei Dreiecke, gegen den Uhrzeigersinn von außen
        let d = vec![
            [0, 2, 3], [0, 3, 1], // z = −h
            [4, 5, 7], [4, 7, 6], // z = +h
            [0, 1, 5], [0, 5, 4], // y = −h
            [2, 6, 7], [2, 7, 3], // y = +h
            [0, 4, 6], [0, 6, 2], // x = −h
            [1, 3, 7], [1, 7, 5], // x = +h
        ];
        Koerpernetz::neu(p, d, 0.5).unwrap()
    }

    /// Vorzeichenbehafteter Abstand zum Würfel, analytisch.
    fn abstand_wuerfel(x: [f64; 3], h: f64) -> f64 {
        let q = x.map(|c| c.abs() - h);
        let aussen = norm(q.map(|c| c.max(0.0)));
        aussen + q[0].max(q[1]).max(q[2]).min(0.0)
    }

    /// Zufallszahlen ohne Abhängigkeit (xorshift).
    fn zufall(s: &mut u64) -> f64 {
        *s ^= *s << 13;
        *s ^= *s >> 7;
        *s ^= *s << 17;
        (*s >> 11) as f64 / (1u64 << 53) as f64
    }

    #[test]
    fn wuerfel_abstand_exakt_auch_an_kanten_und_ecken() {
        let w = wuerfel(1.0);
        assert_eq!(w.randkanten, 0);
        assert!((w.volumen - 8.0).abs() < 1e-12);
        let mut s = 0x1234_5678_9abc_def1u64;
        for _ in 0..20_000 {
            let x = [0, 1, 2].map(|_| 3.0 * zufall(&mut s) - 1.5);
            let n = w.naechster_alle(x);
            let soll = abstand_wuerfel(x, 1.0);
            assert!((n.abstand - soll).abs() < 1e-12, "{x:?}: {} gegen {soll}", n.abstand);
            // Normale zeigt nach außen und führt auf den nächsten Punkt.
            let zurueck = sub(x, mul(n.normale, n.abstand));
            assert!(norm(sub(zurueck, n.punkt)) < 1e-9, "{x:?}");
        }
        // vor einer Ecke und vor einer Kante: Ecken- bzw. Kantenregion
        let n = w.naechster_alle([2.0, 2.0, 2.0]);
        assert!(matches!(n.region, Region::Ecke(_)), "{n:?}");
        assert!((n.abstand - 3f64.sqrt()).abs() < 1e-12);
        let n = w.naechster_alle([1.5, 0.3, 1.5]);
        assert!(matches!(n.region, Region::Kante(_)), "{n:?}");
        assert!((n.abstand - 0.5f64.sqrt()).abs() < 1e-12);
        // innen nahe einer Seite
        let n = w.naechster_alle([0.9, 0.95, 0.0]);
        assert!((n.abstand + 0.05).abs() < 1e-12);
    }

    /// Mit Gitter dasselbe wie über alle Dreiecke, für Punkte innerhalb der
    /// Suchtiefe; weiter weg nichts.
    #[test]
    fn gitter_wie_alle_dreiecke() {
        let (p, d) = zylinder(0.09, -0.3, 0.1, 96, 20);
        let k = Koerpernetz::neu(p, d, 0.01).unwrap();
        assert_eq!(k.randkanten, 0);
        let mut s = 0xfeed_beef_dead_cafeu64;
        let (mut innen, mut aussen, mut weit) = (0, 0, 0);
        for _ in 0..20_000 {
            let r = 0.07 + 0.04 * zufall(&mut s);
            let w = 2.0 * std::f64::consts::PI * zufall(&mut s);
            let x = [r * w.cos(), r * w.sin(), -0.32 + 0.44 * zufall(&mut s)];
            let alle = k.naechster_alle(x);
            match k.naechster(x, k.suchtiefe) {
                Some(n) => {
                    assert!((n.abstand - alle.abstand).abs() < 1e-12, "{x:?}");
                    assert!(norm(sub(n.punkt, alle.punkt)) < 1e-12, "{x:?}");
                    if n.abstand < 0.0 { innen += 1 } else { aussen += 1 }
                }
                None => {
                    assert!(alle.abstand.abs() >= k.suchtiefe, "{x:?}: {} übersehen", alle.abstand);
                    weit += 1;
                }
            }
        }
        assert!(innen > 1000 && aussen > 1000 && weit > 1000, "{innen} {aussen} {weit}");
    }

    /// Durchstoß von außen am nächsten Eintritt; von innen nach außen nichts;
    /// ganz durch eine dünne Platte: der Eintritt oben.
    #[test]
    fn durchstoss_von_aussen() {
        let w = wuerfel(1.0);
        let (p, n, _) = w.durchstoss([0.2, 0.3, 3.0], [0.2, 0.3, -3.0], [0.0; 3]).unwrap();
        assert!(norm(sub(p, [0.2, 0.3, 1.0])) < 1e-12 && (n[2] - 1.0).abs() < 1e-12, "{p:?} {n:?}");
        assert!(w.durchstoss([0.2, 0.3, 0.0], [0.2, 0.3, -3.0], [0.0; 3]).is_none());
        assert!(w.durchstoss([0.2, 0.3, 3.0], [0.2, 0.3, 1.5], [0.0; 3]).is_none());
        // genau durch die Diagonale der Oberseite, die Kante ihrer zwei Dreiecke
        let (p, _, _) = w.durchstoss([0.5, 0.5, 2.0], [0.5, 0.5, 0.0], [0.0; 3]).unwrap();
        assert!((p[2] - 1.0).abs() < 1e-12);
        // dünne Platte 2 mm: ganz hindurch, Eintritt oben
        let (q, d) = zylinder(0.05, -0.001, 0.001, 64, 1);
        let platte = Koerpernetz::neu(q, d, 0.01).unwrap();
        let (p, n, _) = platte.durchstoss([0.01, 0.0, 0.05], [0.01, 0.0, -0.05], [0.0; 3]).unwrap();
        assert!((p[2] - 0.001).abs() < 1e-12 && n[2] > 0.999);
    }

    /// Eine Spitze (Tetraeder, Spitze 1 cm über der Stoffebene) sticht durch
    /// ein großes Stoffdreieck, dessen Ecken weit außerhalb liegen: Die
    /// Stoffpunkte allein sehen nichts, Körperpunkt gegen Stoffdreieck hebt das
    /// Dreieck über die Spitze.
    #[test]
    fn spitze_sticht_nicht_durch() {
        let p = vec![[-0.02, -0.02, -0.03], [0.02, -0.02, -0.03], [0.0, 0.03, -0.03], [0.0, 0.0, 0.01]];
        let d = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
        let k = Koerpernetz::neu(p, d, 0.02).unwrap();
        let mut x = vec![[-0.3, -0.3, 0.0], [0.3, -0.3, 0.0], [0.0, 0.4, 0.0]];
        let mut v = vec![[0.0, 0.0, -1.0]; 3];
        for &q in &x {
            assert!(k.naechster_alle(q).abstand > 0.0, "Stoffpunkt {q:?} liegt außen");
        }
        let n = k.punkte_gegen_dreiecke(0.02, [0.0; 3], [0.0; 3], &[[0, 1, 2]], &[1.0; 3], &mut x, &mut v);
        assert_eq!(n, 1);
        // Die Mittelfläche des Stoffs geht jetzt durch die Spitze, nicht mehr
        // unter ihr; die Ecken heben sich nach ihren Schwerpunktgewichten.
        let (a, b, c) = (x[0], x[1], x[2]);
        let nrm = normiert(cross(sub(b, a), sub(c, a)));
        let hoehe = dot(sub(a, [0.0, 0.0, 0.01]), nrm);
        assert!(hoehe.abs() < 1e-12, "{hoehe} {x:?}");
        assert!(x.iter().all(|q| q[2] > 0.0), "{x:?}");
        // Keine Geschwindigkeit mehr in die Spitze hinein, am Schnittpunkt der
        // Geraden durch die Spitze längs ihrer Normale.
        let ns = k.punktnormale[3];
        let (e1, e2) = (sub(b, a), sub(c, a));
        let pv = cross(ns, e2);
        let det = dot(e1, pv);
        let tv = sub([0.0, 0.0, 0.01], a);
        let bu = dot(tv, pv) / det;
        let bv = dot(ns, cross(tv, e1)) / det;
        let vq = add(add(mul(v[0], 1.0 - bu - bv), mul(v[1], bu)), mul(v[2], bv));
        assert!(dot(vq, ns) >= -1e-12, "{vq:?}");
        // Ein zweiter Durchgang findet nichts mehr.
        let n2 = k.punkte_gegen_dreiecke(0.02, [0.0; 3], [0.0; 3], &[[0, 1, 2]], &[1.0; 3], &mut x, &mut v);
        assert_eq!(n2, 0);
    }

    /// Ein Feld je Punkt wird nach den Schwerpunktgewichten des Treffpunkts
    /// gemischt: eine lineare Verteilung über das Netz kommt am Treffpunkt
    /// linear heraus (Schwerpunktinterpolation ist für lineare Funktionen exakt).
    #[test]
    fn geschwindigkeit_wird_gemischt() {
        let k = wuerfel(1.0);
        // v = (z, 0, 0): linear in z, also interpoliert jedes Dreieck exakt.
        let feld: Vec<[f64; 3]> = k.punkte.iter().map(|&q| [q[2], 0.0, 0.0]).collect();
        let mut k2 = k.clone();
        k2.setze_punktgeschwindigkeit(feld).expect("Feld passt");
        for t in 0..k2.dreiecke.len() {
            let [a, b, c] = k2.dreiecke[t].map(|i| k2.punkte[i as usize]);
            let mitte = mul(add(add(a, b), c), 1.0 / 3.0);
            for p in [a, b, c, mitte, add(a, mul(sub(b, a), 0.25))] {
                let v = k2.geschwindigkeit_an(t, p, [9.0; 3]);
                assert!(
                    (v[0] - p[2]).abs() < 1e-12 && v[1] == 0.0 && v[2] == 0.0,
                    "Dreieck {t}, Punkt {p:?}: {v:?}"
                );
            }
        }
        // Ein gleichförmiges Feld bleibt überall gleich (Summe der Gewichte 1).
        let mut k3 = k.clone();
        k3.setze_punktgeschwindigkeit(vec![[0.25, -0.5, 1.5]; k.punkte.len()]).expect("Feld passt");
        for t in 0..k3.dreiecke.len() {
            let v = k3.geschwindigkeit_an(t, k3.punkte[k3.dreiecke[t][0] as usize], [9.0; 3]);
            assert!((v[0] - 0.25).abs() < 1e-15 && (v[1] + 0.5).abs() < 1e-15 && (v[2] - 1.5).abs() < 1e-15, "{v:?}");
        }
    }

    /// Ohne Feld gilt weiter **eine** Geschwindigkeit für den ganzen Körper,
    /// Bit für Bit: das ist der starre Fall, und der darf sich nicht ändern.
    #[test]
    fn ohne_feld_gilt_die_eine_geschwindigkeit() {
        use crate::koerper::Koerper;
        let starr = [0.3, -0.2, 0.1];
        let k = wuerfel(1.0);
        assert!(k.punktgeschwindigkeit.is_empty(), "frisch gebaut trägt kein Feld");
        // Kontakt und Durchstoß geben genau den einen Wert weiter.
        for p in [[1.02, 0.2, 0.5], [0.2, 1.02, 0.3]] {
            let n = k.naechster(p, 0.05).expect("Kontakt in 2 cm");
            assert_eq!(k.geschwindigkeit_an(n.dreieck as usize, n.punkt, starr), starr);
        }
        let (_, _, v) = k.durchstoss([1.5, 0.2, 0.5], [0.5, 0.2, 0.5], starr).expect("durch die Seite");
        assert_eq!(v, starr);
        let koerper = Koerper::Netz { netz: std::sync::Arc::new(k.clone()), versatz: [0.0; 3], geschwindigkeit: starr };
        let kt = koerper.kontakt([1.02, 0.2, 0.5], 0.05).expect("Kontakt");
        assert_eq!(kt.geschwindigkeit, starr);
    }

    /// Ein Feld, das nur an **einer** Ecke schnell ist, macht den Kontakt dort
    /// genau so schnell und an der Gegenseite genau gar nicht — das ist der
    /// Fall, den ein einzelner Wert für den ganzen Körper nicht kann (#96).
    /// Die Proben liegen an Stellen, deren Schwerpunktgewichte exakt bekannt
    /// sind: auf der Ecke (Gewicht 1), auf halber Kante (0,5 / 0,5) und auf der
    /// Gegenseite, die die Ecke gar nicht kennt (drei Gewichte 0).
    #[test]
    fn geschwindigkeit_je_stelle_verschieden() {
        use crate::koerper::Koerper;
        let mut k = wuerfel(1.0);
        let mut feld = vec![[0.0; 3]; k.punkte.len()];
        // Punkt 7 = (1, 1, 1) bewegt sich nach unten, alle anderen stehen.
        feld[7] = [0.0, 0.0, -1.0];
        k.setze_punktgeschwindigkeit(feld).expect("Feld passt");
        let koerper = Koerper::Netz { netz: std::sync::Arc::new(k), versatz: [0.0; 3], geschwindigkeit: [0.0; 3] };

        // Auf der bewegten Ecke selbst: Gewicht 1, also die volle Geschwindigkeit.
        let ecke = koerper.kontakt([1.02, 1.0, 1.0], 0.05).expect("Kontakt auf der Ecke 7");
        assert_eq!(ecke.geschwindigkeit, [0.0, 0.0, -1.0], "auf der Ecke gilt die volle Geschwindigkeit");
        // Auf halber Kante zwischen den Ecken 5 (1,−1,1) und 7 (1,1,1): halb.
        let kante = koerper.kontakt([1.02, 0.0, 1.0], 0.05).expect("Kontakt auf halber Kante");
        assert!(
            (kante.geschwindigkeit[2] + 0.5).abs() < 1e-12,
            "auf halber Kante muss es halb so schnell sein, nicht {:?}",
            kante.geschwindigkeit
        );
        // Gegenseite x = −1 (Ecken 0, 2, 4, 6): kennt Ecke 7 nicht, bleibt stehen.
        let gegenseite = koerper.kontakt([-1.02, 0.5, 0.5], 0.05).expect("Kontakt auf der Gegenseite");
        assert_eq!(gegenseite.geschwindigkeit, [0.0; 3], "die Gegenseite steht still");
    }

    /// Ein Feld falscher Länge wird abgelehnt, statt still falsch zuzuordnen;
    /// leer räumt es ab.
    #[test]
    fn punktgeschwindigkeit_laenge_geprueft() {
        let mut k = wuerfel(1.0);
        assert!(k.setze_punktgeschwindigkeit(vec![[0.0; 3]; 3]).unwrap_err().contains("Punkte"));
        assert!(k.setze_punktgeschwindigkeit(vec![[1.0, 0.0, 0.0]; k.punkte.len()]).is_ok());
        assert_eq!(k.punktgeschwindigkeit.len(), k.punkte.len());
        assert!(k.setze_punktgeschwindigkeit(Vec::new()).is_ok());
        assert!(k.punktgeschwindigkeit.is_empty());
    }

    /// Nachführen räumt das Geschwindigkeitsfeld ab: es gehörte zum alten
    /// Punktstand. Ein bewegter Körper setzt es danach neu.
    #[test]
    fn nachfuehren_raeumt_das_feld_ab() {
        let (p, d) = zylinder(0.15, -0.4, 0.4, 24, 8);
        let mut k = Koerpernetz::neu(p.clone(), d, 0.01).unwrap();
        k.setze_punktgeschwindigkeit(vec![[1.0, 0.0, 0.0]; k.punkte.len()]).unwrap();
        k.nachfuehren(&p).expect("nachfuehren");
        assert!(k.punktgeschwindigkeit.is_empty(), "das Feld ist nach dem Nachführen veraltet");
    }

    /// Falsch orientiert, nicht mannigfaltig oder nach innen gekehrt: Fehler.
    #[test]
    fn fehlerhafte_netze_abgelehnt() {
        let w = wuerfel(1.0);
        let mut d = w.dreiecke.clone();
        d[0] = [d[0][0], d[0][2], d[0][1]];
        assert!(Koerpernetz::neu(w.punkte.clone(), d, 0.1).unwrap_err().contains("orientiert"));
        let innen: Vec<[u32; 3]> = w.dreiecke.iter().map(|t| [t[0], t[2], t[1]]).collect();
        assert!(Koerpernetz::neu(w.punkte.clone(), innen, 0.1).unwrap_err().contains("nach innen"));
        let mut drei = w.dreiecke.clone();
        drei.push(w.dreiecke[0]);
        assert!(Koerpernetz::neu(w.punkte.clone(), drei, 0.1).is_err());
    }

    /// Der Rahmen von CLO wird gedreht, nicht gespiegelt: Volumen bleibt
    /// positiv, y oben wird z oben, vorn +z wird −y.
    #[test]
    fn clo_rahmen() {
        let (p, d) = zylinder(90.0, 0.0, 400.0, 64, 4);
        // Zylinder mit Achse y statt z (mm): (x, y, z) → (x, z, −y) dreht −90° um x
        let p_clo: Vec<[f64; 3]> = p.iter().map(|q| [q[0], q[2], -q[1]]).collect();
        let d_clo: Vec<[usize; 3]> = d.iter().map(|t| t.map(|i| i as usize)).collect();
        let k = Koerpernetz::aus_clo_mm(&p_clo, &d_clo, 0.01).unwrap();
        assert!(k.volumen > 0.0);
        let zmax = k.punkte.iter().map(|q| q[2]).fold(f64::MIN, f64::max);
        assert!((zmax - 0.4).abs() < 1e-12);
        // 5 mm vor dem Mantel in CLOs Rahmen (vorn +z, Höhe y = 200 mm) liegt
        // im Szenenrahmen bei −y, auf Höhe z = 0,2 m.
        let vorn = [0.0, 200.0, 95.0];
        let x = [vorn[0] * 1e-3, -vorn[2] * 1e-3, vorn[1] * 1e-3];
        let n = k.naechster(x, 0.01).unwrap();
        assert!((n.abstand - 0.005).abs() < 1e-12 && n.normale[1] < -0.999, "{n:?}");
    }
}
