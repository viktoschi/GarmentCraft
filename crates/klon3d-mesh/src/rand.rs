//! Randunterteilung und Nahtaufteilung.
//!
//! ## Randpunkte
//!
//! Jede Randkante (Punktknoten zu Punktknoten, [`klon3d_pattern::kanten`])
//! wird in Stücke von höchstens der Zielkantenlänge geteilt. Ein Stück wird
//! halbiert, solange die Sehne mehr als `sehne_mm` neben der Kante verläuft
//! oder mehr Länge verliert, als ihr Anteil an `laenge_mm` je Randkante
//! erlaubt. So hält jede Randkante die Abnahme „Lage und Länge auf 0,01 cm“
//! ein, und gerade Kanten bleiben bei 10 mm.
//!
//! ## Nähte
//!
//! CLO ordnet entlang einer Naht gleiche **Längenanteile** einander zu
//! und teilt beide Seiten an einer gemeinsamen Menge von Teilungsstellen
//! (Spezifikation `naehte_2026_1.md` §4.1, §4.2); im Netz bekommen alle
//! zusammengenähten Randsegmente gleich viele Randpunkte, und wird eines
//! geteilt, dann alle an derselben relativen Stelle (§4.3). klon3d:
//!
//! 1. **Stellen:** alle Kantengrenzen beider Seiten (Punktknoten und
//!    Teilstückgrenzen) als Längenanteil ρ = Abstand vom Nahtanfang / Seitenlänge.
//! 2. **Zusammenlegen:** Liegt einer Stelle auf A eine Stelle auf B höchstens
//!    `fang_mm` gegenüber (gegenseitig nächste), werden beide miteinander
//!    vernäht — so treffen sich die Passzeichen. Der Versatz wird gemeldet.
//!    Das weicht von CLOs reiner Anteilsregel ab; dort bekäme jede Stelle
//!    einen eigenen Gegenpunkt, Bruchteile eines Millimeters daneben
//!    (Entscheidung der Anwenderin, 20.10.2026).
//! 3. **Übrige Stellen** bekommen auf der Gegenseite einen eigenen Randpunkt;
//!    zwischen den zusammengelegten Stellen wird linear nach Länge verteilt.
//! 4. **Abschnitte:** Zwischen zwei Stellen liegt jede Seite auf genau einer
//!    Randkante. Beide Seiten eines Abschnitts teilen sich eine Liste von
//!    Anteilen `f` (eine [`Gruppe`]); wird ein Randstück geteilt, bekommen
//!    beide Seiten den neuen Punkt.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use klon3d_pattern::kanten::Teilrand;
use klon3d_pattern::naehte::{Nahtergebnis, Nahtseite};

use crate::{Fadenlauf, Nahtstelle, Parameter, Stellenart};

fn abstand(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn punkt_strecke(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    if l2 <= 0.0 {
        return abstand(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
    abstand(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Polylinie einer Randkante in mm mit Bogenlänge.
#[derive(Debug, Clone)]
pub struct Kantenzug {
    pub punkte: Vec<[f64; 2]>,
    kum: Vec<f64>,
    pub laenge: f64,
}

impl Kantenzug {
    pub fn neu(roh: Vec<[f64; 2]>) -> Self {
        let mut punkte: Vec<[f64; 2]> = Vec::with_capacity(roh.len());
        let mut kum = Vec::with_capacity(roh.len());
        for p in roh {
            match punkte.last() {
                None => kum.push(0.0),
                Some(&q) => {
                    let d = abstand(q, p);
                    if d <= 1e-9 {
                        continue;
                    }
                    kum.push(kum.last().unwrap() + d);
                }
            }
            punkte.push(p);
        }
        let laenge = kum.last().copied().unwrap_or(0.0);
        Kantenzug { punkte, kum, laenge }
    }

    /// Punkt bei Bogenlänge `s` (mm) ab dem Anfang.
    pub fn ort(&self, s: f64) -> [f64; 2] {
        if s <= 0.0 {
            return self.punkte[0];
        }
        if s >= self.laenge {
            return *self.punkte.last().unwrap();
        }
        let i = self.kum.partition_point(|&x| x <= s) - 1;
        let (a, b) = (self.punkte[i], self.punkte[i + 1]);
        let t = (s - self.kum[i]) / (self.kum[i + 1] - self.kum[i]);
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
    }

    /// Sehne von `s0` nach `s1`: größter Abstand der Kante von ihr und
    /// Längenverlust gegenüber der Kante (mm).
    pub fn sehne(&self, s0: f64, s1: f64) -> (f64, f64) {
        let (hoehe, verlust, _) = self.sehne_ecke(s0, s1);
        (hoehe, verlust)
    }

    /// Wie [`Kantenzug::sehne`], dazu die Bogenlänge des Polylinienpunkts
    /// mit dem größten Abstand von der Sehne.
    fn sehne_ecke(&self, s0: f64, s1: f64) -> (f64, f64, Option<f64>) {
        let (s0, s1) = (s0.min(s1), s0.max(s1));
        let (a, b) = (self.ort(s0), self.ort(s1));
        let (mut hoehe, mut ecke): (f64, Option<f64>) = (0.0, None);
        for i in self.kum.partition_point(|&x| x <= s0)..self.kum.len() {
            if self.kum[i] >= s1 {
                break;
            }
            let h = punkt_strecke(self.punkte[i], a, b);
            if ecke.is_none() || h > hoehe {
                (hoehe, ecke) = (h, Some(self.kum[i]));
            }
        }
        (hoehe, (s1 - s0) - abstand(a, b), ecke)
    }
}

/// Ein Teil, wie die Vernetzung es braucht (mm).
#[derive(Debug, Clone)]
pub(crate) struct Teilgeo {
    pub teil: String,
    pub kanten: Vec<Kantenzug>,
    /// Name des Punktknotens am Anfang jeder Kante; leer bei einem Teil ohne
    /// Punktknoten (dann ist der ganze Umriss eine Kante).
    pub knoten: Vec<String>,
    pub kontur: Vec<[f64; 2]>,
    /// Fadenlauf in mm, schon im Bezugssystem der Kontur.
    pub fadenlauf: Option<Fadenlauf>,
}

impl Teilgeo {
    /// `fl` ist der bereits gelesene Fadenlauf des Teils in Mustereinheiten
    /// ([`klon3d_pattern::fadenlauf::Auswertung::fadenlauf`]); `None` heißt
    /// ohne sichtbaren Fadenlauf.
    pub fn aus(r: &Teilrand, fl: Option<&klon3d_pattern::fadenlauf::Fadenlauf>) -> Result<Self, String> {
        let mm = r.mm_pro_einheit;
        let f = |p: &klon3d_pattern::Pt| [p.x * mm, p.y * mm];
        let kontur: Vec<[f64; 2]> = r.umriss.iter().map(f).collect();
        if kontur.len() < 3 {
            return Err(format!("Umriss mit {} Punkten", kontur.len()));
        }
        let (kanten, knoten) = if r.kanten.is_empty() {
            let mut p = kontur.clone();
            p.push(kontur[0]);
            (vec![Kantenzug::neu(p)], Vec::new())
        } else {
            (
                r.kanten.iter().map(|k| Kantenzug::neu(k.punkte.iter().map(f).collect())).collect(),
                r.punktknoten.iter().map(|p| p.name.clone()).collect(),
            )
        };
        if let Some(k) = kanten.iter().position(|k| k.laenge <= 1e-6) {
            return Err(format!("Randkante {k} hat keine Länge"));
        }
        // The grainline comes from `Auswertung::fadenlauf` in the same stored
        // frame as `Teilrand::umriss`, so the same factor converts it.
        let fadenlauf = fl.map(|g| Fadenlauf {
            p1: f(&g.p1),
            p2: f(&g.p2),
            pfeile: g.pfeile,
        });
        Ok(Teilgeo { teil: r.teil.clone(), kanten, knoten, kontur, fadenlauf })
    }

    fn knotenname(&self, i: usize) -> String {
        if self.knoten.is_empty() {
            String::new()
        } else {
            self.knoten[i % self.knoten.len()].clone()
        }
    }
}

/// Ein Stück Randkante zwischen zwei Stellen, `s0 → s1` in Nährichtung
/// (`s0 > s1`, wenn die Naht die Kante rückwärts durchläuft).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Glied {
    pub teil: usize,
    pub kante: usize,
    pub s0: f64,
    pub s1: f64,
}

impl Glied {
    pub fn s(&self, f: f64) -> f64 {
        if f <= 0.0 {
            self.s0
        } else if f >= 1.0 {
            self.s1
        } else {
            self.s0 + f * (self.s1 - self.s0)
        }
    }

    fn laenge(&self) -> f64 {
        (self.s1 - self.s0).abs()
    }
}

/// Glieder, die gemeinsam unterteilt werden: eine freie Randkante (ein
/// Glied) oder ein Abschnitt einer Naht (zwei Glieder).
#[derive(Debug, Clone)]
pub(crate) struct Gruppe {
    pub glieder: Vec<Glied>,
    /// Anteile 0 = f₀ < f₁ < … < fₘ = 1.
    pub f: Vec<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct Abschnittplan {
    pub gruppe: usize,
    /// Abstand vom Nahtanfang am Anfang und Ende des Abschnitts (mm).
    pub sigma_a: [f64; 2],
    pub sigma_b: [f64; 2],
}

#[derive(Debug, Clone)]
pub(crate) struct Teilnahtplan {
    pub teil_a: usize,
    pub teil_b: usize,
    pub abschnitte: Vec<Abschnittplan>,
}

#[derive(Debug, Clone)]
pub(crate) struct Nahtplan {
    pub kennung: String,
    pub name: String,
    pub la: f64,
    pub lb: f64,
    pub stellen: Vec<Nahtstelle>,
    pub teilnaehte: Vec<Teilnahtplan>,
}

pub(crate) struct Randplan {
    pub teile: Vec<Teilgeo>,
    pub gruppen: Vec<Gruppe>,
    pub naehte: Vec<Nahtplan>,
}

/// Randpunkte eines Teils in Umlaufreihenfolge.
pub(crate) struct Randindex {
    /// (Randkante, Bogenlänge ab ihrem Anfang in mm) je Randpunkt.
    pub orte: Vec<(usize, f64)>,
    pub lagen: Vec<[f64; 2]>,
    index: HashMap<(usize, u64), u32>,
    /// Randstück `i → i + 1`: (Gruppe, Fach).
    pub besitzer: Vec<(usize, usize)>,
}

impl Randindex {
    /// Netzpunkt am Anfang der Randkante `kante` (ihr Punktknoten).
    pub fn knoten(&self, kante: usize) -> u32 {
        self.index[&(kante, 0)]
    }
}

/// Eine Kante einer Nahtseite in Nährichtung.
#[derive(Debug, Clone, Copy)]
struct Stueck {
    teil: usize,
    kante: usize,
    rueck: bool,
    laenge: f64,
    /// Index des Teilstücks (Abschnitt der Nahtliste) auf dieser Seite.
    teilstueck: usize,
}

impl Stueck {
    fn anfang(&self) -> f64 {
        if self.rueck { self.laenge } else { 0.0 }
    }
    fn ende(&self) -> f64 {
        if self.rueck { 0.0 } else { self.laenge }
    }
}

fn seite(ns: &Nahtseite, namen: &HashMap<&str, usize>, teile: &[Teilgeo]) -> Result<Vec<Stueck>, String> {
    let mut out = Vec::new();
    for (nr, ts) in ns.teilstuecke.iter().enumerate() {
        let &teil = namen.get(ts.teil.as_str()).ok_or_else(|| format!("Teil {} nicht vernetzbar", ts.teil))?;
        for &kante in &ts.kanten {
            let kz = teile[teil].kanten.get(kante).ok_or_else(|| format!("{}: keine Kante {kante}", ts.teil))?;
            out.push(Stueck { teil, kante, rueck: ts.rueckwaerts, laenge: kz.laenge, teilstueck: nr });
        }
    }
    if out.is_empty() {
        return Err("Seite ohne Kanten".into());
    }
    Ok(out)
}

/// Unterteilt eine Gruppe: gleichmäßig auf höchstens `kante_mm`, dann wird
/// jedes Stück geteilt, dessen Sehne auf einem Glied zu weit neben der Kante
/// liegt oder zu viel Länge verliert — am Polylinienpunkt mit dem größten
/// Abstand (wie Douglas–Peucker), sonst in der Mitte.
///
/// Die Teilung am Polylinienpunkt ist nötig, weil die Kante selbst eine
/// Polylinie ist: Bei kleinen Scheiben liegen deren Punkte so dicht wie die
/// Randpunkte, und gleichmäßig verteilte Sehnen schneiden jede Ecke ab
/// (Scheibe R 10 mm: 0,128 mm Verlust je Halbkreis, 20.10.2026).
pub(crate) fn unterteile(g: &mut Gruppe, teile: &[Teilgeo], p: &Parameter) {
    let lmax = g.glieder.iter().map(Glied::laenge).fold(0.0, f64::max);
    let m = ((lmax / p.kante_mm).ceil() as usize).max(1);
    g.f = (0..=m).map(|j| j as f64 / m as f64).collect();
    let mut j = 0;
    while j + 1 < g.f.len() {
        let (f0, f1) = (g.f[j], g.f[j + 1]);
        let mut grob = false;
        let mut ecke = None;
        for gl in &g.glieder {
            let kz = &teile[gl.teil].kanten[gl.kante];
            let (s0, s1) = (gl.s(f0), gl.s(f1));
            let (hoehe, verlust, e) = kz.sehne_ecke(s0, s1);
            if hoehe > p.sehne_mm || verlust > p.laenge_mm * (s1 - s0).abs() / kz.laenge {
                grob = true;
                ecke = ecke.or(e.map(|s| (s - gl.s0) / (gl.s1 - gl.s0)));
            }
        }
        let teilung = match ecke {
            _ if !grob => None,
            Some(f) if (f - f0) * lmax >= p.min_rand_mm && (f1 - f) * lmax >= p.min_rand_mm => Some(f),
            _ if (f1 - f0) * lmax >= 2.0 * p.min_rand_mm => Some(0.5 * (f0 + f1)),
            _ => None,
        };
        match teilung {
            Some(f) => g.f.insert(j + 1, f),
            None => j += 1,
        }
    }
}

/// Lineare Abbildung über Ankerpaare (ρ_A, ρ_B); `rueck`: von B nach A.
fn abbilden(anker: &[(f64, f64)], x: f64, rueck: bool) -> f64 {
    let paar = |a: &(f64, f64)| if rueck { (a.1, a.0) } else { (a.0, a.1) };
    let k = anker.iter().rposition(|a| paar(a).0 <= x).unwrap_or(0).min(anker.len() - 2);
    let ((x0, y0), (x1, y1)) = (paar(&anker[k]), paar(&anker[k + 1]));
    y0 + (x - x0) / (x1 - x0) * (y1 - y0)
}

/// Eine Stelle einer Naht vor der Zuordnung.
#[derive(Debug, Clone)]
struct Roh {
    ra: f64,
    rb: f64,
    /// Kantengrenze vor Stück `i` (0 = Anfang, Anzahl = Ende), sonst
    /// neuer Randpunkt.
    ga: Option<usize>,
    gb: Option<usize>,
    art: Stellenart,
}

/// Ort einer Stelle auf einer Seite: (Stück davor, s), (Stück danach, s).
fn orte(s: &[Stueck], kum: &[f64], l: f64, rho: f64, grenze: Option<usize>) -> [Option<(usize, f64)>; 2] {
    match grenze {
        Some(i) => [(i > 0).then(|| (i - 1, s[i - 1].ende())), (i < s.len()).then(|| (i, s[i].anfang()))],
        None => {
            let sigma = rho * l;
            let k = kum.partition_point(|&x| x <= sigma).saturating_sub(1).min(s.len() - 1);
            let d = (sigma - kum[k]).clamp(0.0, s[k].laenge);
            let pos = if s[k].rueck { s[k].laenge - d } else { d };
            [Some((k, pos)), Some((k, pos))]
        }
    }
}

fn stellenname(s: &[Stueck], teile: &[Teilgeo], i: usize) -> String {
    let anfang = |x: &Stueck| teile[x.teil].knotenname(if x.rueck { x.kante + 1 } else { x.kante });
    let ende = |x: &Stueck| teile[x.teil].knotenname(if x.rueck { x.kante } else { x.kante + 1 });
    if i == 0 {
        anfang(&s[0])
    } else if i == s.len() {
        ende(&s[i - 1])
    } else if s[i - 1].teilstueck == s[i].teilstueck {
        anfang(&s[i])
    } else {
        format!("{}/{}", ende(&s[i - 1]), anfang(&s[i]))
    }
}

fn naht_planen(
    n: &Nahtergebnis,
    a: &[Stueck],
    b: &[Stueck],
    teile: &[Teilgeo],
    p: &Parameter,
    gruppen: &mut Vec<Gruppe>,
) -> Result<Nahtplan, String> {
    let kum = |s: &[Stueck]| -> Vec<f64> {
        let mut v = vec![0.0];
        for x in s {
            v.push(v.last().unwrap() + x.laenge);
        }
        v
    };
    let (ka, kb) = (kum(a), kum(b));
    let (la, lb) = (*ka.last().unwrap(), *kb.last().unwrap());
    let lm = 0.5 * (la + lb);
    // Innere Kantengrenzen: ra[i] liegt vor Stück i + 1.
    let ra: Vec<f64> = ka[1..a.len()].iter().map(|x| x / la).collect();
    let rb: Vec<f64> = kb[1..b.len()].iter().map(|x| x / lb).collect();
    let naechst = |x: f64, v: &[f64]| -> Option<usize> {
        (0..v.len()).min_by(|&i, &j| (v[i] - x).abs().total_cmp(&(v[j] - x).abs()))
    };
    // Stellen unter 1 µm auseinander sind dieselbe Stelle, auch ohne Fang
    // (Teilstückgrenzen, die in beiden Seiten an derselben Konstruktion hängen).
    let fang = p.fang_mm.max(1e-3);
    let mut anker = vec![(0.0, 0.0)];
    let mut gemeinsam = Vec::new();
    for (i, &x) in ra.iter().enumerate() {
        let Some(j) = naechst(x, &rb) else { continue };
        let letzt = *anker.last().unwrap();
        if (rb[j] - x).abs() * lm <= fang && naechst(rb[j], &ra) == Some(i) && x > letzt.0 && rb[j] > letzt.1 {
            anker.push((x, rb[j]));
            gemeinsam.push((i, j));
        }
    }
    anker.push((1.0, 1.0));

    let mut st = vec![Roh { ra: 0.0, rb: 0.0, ga: Some(0), gb: Some(0), art: Stellenart::Ende }];
    let ia: HashSet<usize> = gemeinsam.iter().map(|x| x.0).collect();
    let jb: HashSet<usize> = gemeinsam.iter().map(|x| x.1).collect();
    for &(i, j) in &gemeinsam {
        st.push(Roh { ra: ra[i], rb: rb[j], ga: Some(i + 1), gb: Some(j + 1), art: Stellenart::Gemeinsam });
    }
    for (i, &x) in ra.iter().enumerate().filter(|(i, _)| !ia.contains(i)) {
        st.push(Roh { ra: x, rb: abbilden(&anker, x, false), ga: Some(i + 1), gb: None, art: Stellenart::NurA });
    }
    for (j, &y) in rb.iter().enumerate().filter(|(j, _)| !jb.contains(j)) {
        st.push(Roh { ra: abbilden(&anker, y, true), rb: y, ga: None, gb: Some(j + 1), art: Stellenart::NurB });
    }
    st.push(Roh { ra: 1.0, rb: 1.0, ga: Some(a.len()), gb: Some(b.len()), art: Stellenart::Ende });
    st.sort_by(|x, y| x.ra.total_cmp(&y.ra));
    for w in st.windows(2) {
        if (w[1].ra - w[0].ra) * la < 1e-3 || (w[1].rb - w[0].rb) * lb < 1e-3 {
            return Err(format!("Nahtstellen bei {:.3} mm (A) fallen zusammen", w[0].ra * la));
        }
    }

    let mut abschnitte = Vec::new();
    for w in st.windows(2) {
        let [_, Some((sa, s0a))] = orte(a, &ka, la, w[0].ra, w[0].ga) else { unreachable!() };
        let [Some((sa1, s1a)), _] = orte(a, &ka, la, w[1].ra, w[1].ga) else { unreachable!() };
        let [_, Some((sb, s0b))] = orte(b, &kb, lb, w[0].rb, w[0].gb) else { unreachable!() };
        let [Some((sb1, s1b)), _] = orte(b, &kb, lb, w[1].rb, w[1].gb) else { unreachable!() };
        if sa != sa1 || sb != sb1 {
            return Err(format!("Abschnitt ab {:.3} mm läuft über eine Kantengrenze", w[0].ra * la));
        }
        let ga = Glied { teil: a[sa].teil, kante: a[sa].kante, s0: s0a, s1: s1a };
        let gb = Glied { teil: b[sb].teil, kante: b[sb].kante, s0: s0b, s1: s1b };
        let mut g = Gruppe { glieder: vec![ga, gb], f: Vec::new() };
        unterteile(&mut g, teile, p);
        gruppen.push(g);
        abschnitte.push((
            (a[sa].teilstueck, b[sb].teilstueck),
            (a[sa].teil, b[sb].teil),
            Abschnittplan { gruppe: gruppen.len() - 1, sigma_a: [w[0].ra * la, w[1].ra * la], sigma_b: [w[0].rb * lb, w[1].rb * lb] },
        ));
    }
    let mut teilnaehte: Vec<Teilnahtplan> = Vec::new();
    let mut letzt = None;
    for (schl, (teil_a, teil_b), ab) in abschnitte {
        if letzt != Some(schl) {
            teilnaehte.push(Teilnahtplan { teil_a, teil_b, abschnitte: Vec::new() });
            letzt = Some(schl);
        }
        teilnaehte.last_mut().unwrap().abschnitte.push(ab);
    }

    let stellen = st
        .iter()
        .map(|s| Nahtstelle {
            art: s.art,
            name_a: s.ga.map(|i| stellenname(a, teile, i)).unwrap_or_default(),
            name_b: s.gb.map(|j| stellenname(b, teile, j)).unwrap_or_default(),
            rho_a: s.ra,
            rho_b: s.rb,
            versatz_mm: (s.ra - s.rb) * lm,
        })
        .collect();
    Ok(Nahtplan { kennung: n.kennung.clone(), name: n.name.clone(), la, lb, stellen, teilnaehte })
}

impl Randplan {
    pub fn neu(teile: Vec<Teilgeo>, naehte: &[Nahtergebnis], p: &Parameter) -> Result<Self, (String, String)> {
        let mut namen: HashMap<&str, usize> = HashMap::new();
        for (i, t) in teile.iter().enumerate() {
            namen.entry(t.teil.as_str()).or_insert(i);
        }
        let mut gruppen = Vec::new();
        let mut belegt: HashSet<(usize, usize)> = HashSet::new();
        let mut plaene = Vec::new();
        for n in naehte {
            let fehler = |g: String| (n.kennung.clone(), g);
            let a = seite(&n.a, &namen, &teile).map_err(fehler)?;
            let b = seite(&n.b, &namen, &teile).map_err(fehler)?;
            for s in a.iter().chain(&b) {
                if !belegt.insert((s.teil, s.kante)) {
                    return Err(fehler(format!("{} Kante {} in zwei Nähten", teile[s.teil].teil, s.kante)));
                }
            }
            plaene.push(naht_planen(n, &a, &b, &teile, p, &mut gruppen).map_err(fehler)?);
        }
        for (ti, t) in teile.iter().enumerate() {
            for (k, kz) in t.kanten.iter().enumerate() {
                if !belegt.contains(&(ti, k)) {
                    let mut g = Gruppe { glieder: vec![Glied { teil: ti, kante: k, s0: 0.0, s1: kz.laenge }], f: Vec::new() };
                    unterteile(&mut g, &teile, p);
                    gruppen.push(g);
                }
            }
        }
        Ok(Randplan { teile, gruppen, naehte: plaene })
    }

    /// Schlüssel eines Randorts: das Ende einer Kante ist der Anfang der nächsten.
    fn schluessel(&self, teil: usize, kante: usize, s: f64) -> (usize, u64) {
        let t = &self.teile[teil];
        if s >= t.kanten[kante].laenge {
            ((kante + 1) % t.kanten.len(), 0)
        } else if s <= 0.0 {
            (kante, 0)
        } else {
            (kante, s.to_bits())
        }
    }

    pub fn randindex(&self, teil: usize) -> Randindex {
        let t = &self.teile[teil];
        let mut menge: BTreeMap<(usize, u64), (usize, f64)> = BTreeMap::new();
        for g in &self.gruppen {
            for gl in g.glieder.iter().filter(|gl| gl.teil == teil) {
                for &f in &g.f {
                    let s = gl.s(f);
                    let k = self.schluessel(teil, gl.kante, s);
                    menge.entry(k).or_insert(if k.1 == 0 { (k.0, 0.0) } else { (gl.kante, s) });
                }
            }
        }
        let orte: Vec<(usize, f64)> = menge.values().copied().collect();
        let lagen = orte.iter().map(|&(k, s)| t.kanten[k].ort(s)).collect();
        let index: HashMap<(usize, u64), u32> = menge.keys().enumerate().map(|(i, k)| (*k, i as u32)).collect();
        let n = orte.len();
        let mut besitzer = vec![(usize::MAX, 0); n];
        for (gi, g) in self.gruppen.iter().enumerate() {
            for gl in g.glieder.iter().filter(|gl| gl.teil == teil) {
                for j in 0..g.f.len() - 1 {
                    let u = index[&self.schluessel(teil, gl.kante, gl.s(g.f[j]))] as usize;
                    let v = index[&self.schluessel(teil, gl.kante, gl.s(g.f[j + 1]))] as usize;
                    let i = if v == (u + 1) % n {
                        u
                    } else if u == (v + 1) % n {
                        v
                    } else {
                        panic!("{}: Randpunkte {u} und {v} nicht benachbart", t.teil)
                    };
                    besitzer[i] = (gi, j);
                }
            }
        }
        assert!(besitzer.iter().all(|b| b.0 != usize::MAX), "{}: Randstück ohne Gruppe", t.teil);
        Randindex { orte, lagen, index, besitzer }
    }

    /// Netzpunkt eines Glieds bei Anteil `f`.
    pub fn punkt(&self, ri: &Randindex, gl: &Glied, f: f64) -> u32 {
        ri.index[&self.schluessel(gl.teil, gl.kante, gl.s(f))]
    }

    /// Teilt die gewünschten Fächer (Gruppe, Fach) in der Mitte. Gibt die
    /// betroffenen Teile zurück.
    pub fn teile_auf(&mut self, wuensche: &BTreeSet<(usize, usize)>) -> BTreeSet<usize> {
        let mut betroffen = BTreeSet::new();
        // Absteigend, damit die Fachnummern einer Gruppe gültig bleiben.
        for &(g, j) in wuensche.iter().rev() {
            let gr = &mut self.gruppen[g];
            let mitte = 0.5 * (gr.f[j] + gr.f[j + 1]);
            gr.f.insert(j + 1, mitte);
            betroffen.extend(gr.glieder.iter().map(|gl| gl.teil));
        }
        betroffen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kantenzug_ort_und_sehne() {
        let k = Kantenzug::neu(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(k.punkte.len(), 3);
        assert_eq!(k.laenge, 20.0);
        assert_eq!(k.ort(15.0), [10.0, 5.0]);
        assert_eq!(k.ort(25.0), [10.0, 10.0]);
        let (h, v) = k.sehne(5.0, 15.0);
        assert!((h - 5.0f64.hypot(5.0) / 2.0).abs() < 1e-12, "{h}");
        assert!((v - (10.0 - 50f64.sqrt())).abs() < 1e-12);
        assert_eq!(k.sehne(0.0, 10.0), (0.0, 0.0));
    }

    #[test]
    fn abbildung_ueber_anker() {
        let anker = [(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)];
        assert!((abbilden(&anker, 0.25, false) - 0.3).abs() < 1e-12);
        assert!((abbilden(&anker, 0.8, true) - 0.75).abs() < 1e-12);
        assert_eq!(abbilden(&anker, 1.0, false), 1.0);
    }

    #[test]
    fn unterteilung_nach_kruemmung() {
        // Viertelkreis R = 20 mm, fein abgetastet, und eine Gerade.
        let bogen: Vec<[f64; 2]> =
            (0..=400).map(|i| (i as f64 / 400.0) * std::f64::consts::FRAC_PI_2).map(|w| [20.0 * w.cos(), 20.0 * w.sin()]).collect();
        let teile = vec![Teilgeo {
            teil: "T".into(),
            kanten: vec![Kantenzug::neu(bogen), Kantenzug::neu(vec![[0.0, 20.0], [0.0, 0.0]]), Kantenzug::neu(vec![[0.0, 0.0], [20.0, 0.0]])],
            knoten: vec!["A".into(), "B".into(), "C".into()],
            kontur: Vec::new(),
            fadenlauf: None,
        }];
        let p = Parameter::default();
        let mut g = Gruppe { glieder: vec![Glied { teil: 0, kante: 0, s0: 0.0, s1: teile[0].kanten[0].laenge }], f: Vec::new() };
        unterteile(&mut g, &teile, &p);
        // Sehnenhöhe s²/8R ≤ 0,1 mm → s ≤ 4 mm: 31,4 mm Bogen in mindestens 8 Stücke.
        assert!(g.f.len() - 1 >= 8, "{}", g.f.len() - 1);
        let kz = &teile[0].kanten[0];
        for w in g.f.windows(2) {
            let (h, v) = kz.sehne(w[0] * kz.laenge, w[1] * kz.laenge);
            assert!(h <= p.sehne_mm && v <= p.laenge_mm * (w[1] - w[0]));
        }
        let mut gerade = Gruppe { glieder: vec![Glied { teil: 0, kante: 1, s0: 20.0, s1: 0.0 }], f: Vec::new() };
        unterteile(&mut gerade, &teile, &p);
        assert_eq!(gerade.f, [0.0, 0.5, 1.0]);
    }
}
