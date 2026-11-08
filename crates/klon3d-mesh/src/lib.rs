//! Schnittteile als Dreiecksnetz (klon3d Stufe 3).
//!
//! Jedes Teil wird ein ebenes Dreiecksnetz in Musterkoordinaten (mm) mit etwa
//! [`Parameter::kante_mm`] Kantenlänge. Der Netzrand folgt der Nahtlinie aus
//! [`klon3d_pattern::kanten`]: jeder Punktknoten ist ein Netzpunkt, jeder
//! Randpunkt liegt auf seiner Randkante. Für jedes Nahtpaar der Nahtliste
//! ([`klon3d_pattern::naehte`]) bekommen beide Seiten gleich viele
//! Randpunkte, zugeordnet nach Längenanteil wie in CLO
//! (`spez/naehte_2026_1.md` §4). Das Ergebnis sind
//! Punktpaare je Naht — die Eingabe für die Nahtfedern in Stufe 4.
//!
//! ## Ablauf
//!
//! 1. Randplan (`rand.rs`): Randkanten und Nähte in Gruppen gemeinsam
//!    unterteilter Randstücke.
//! 2. Je Teil Triangulierung und Verfeinerung (`dreieck.rs`). Randstücke
//!    teilt die Triangulierung nicht selbst; sie meldet sie, der Randplan
//!    teilt die ganze Gruppe — also auch die Gegenseite einer Naht —, und
//!    die betroffenen Teile werden neu vernetzt. Das wiederholt sich, bis
//!    keine Meldung mehr kommt.
//! 3. Glätten, dann Punktpaare der Nähte ablesen.
//!
//! Das Verfahren ist aus der Literatur gebaut (Ohrenschneiden, Lawson,
//! Ruppert), nicht aus CLO; von CLO stammt nur die Zuordnungsregel der Nähte.
//!
//! Prüfen: [`pruefung`]; Ausgabe als SVG, OBJ und Paarliste: [`ausgabe`].

mod dreieck;
mod exakt;
mod rand;

pub mod ausgabe;
pub mod pruefung;
pub mod schnitt;
pub mod zugabe;

use std::collections::BTreeSet;

use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::Nahtergebnis;

use dreieck::{Triangulierung, Ziele};
use rand::{Randindex, Randplan, Teilgeo};

pub use rand::Kantenzug;

/// Einstellungen der Vernetzung. Vorgaben: Abnahme Stufe 3, mit der
/// Anwenderin abgestimmt am 20.10.2026.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameter {
    /// Zielkantenlänge (mm). CLO: Partikelabstand 10.
    pub kante_mm: f64,
    /// Kleinster Dreieckswinkel (Grad).
    pub winkel_grad: f64,
    /// Größter Abstand zwischen Randkante und Netzrand (mm).
    pub sehne_mm: f64,
    /// Größter Längenverlust des Netzrands je Randkante (mm).
    pub laenge_mm: f64,
    /// Zwei Stellen auf beiden Nahtseiten, die höchstens so weit auseinander
    /// liegen, werden zusammengelegt (mm).
    pub fang_mm: f64,
    /// Kürzestes Randstück, das durch Teilen entstehen darf (mm).
    pub min_rand_mm: f64,
    /// Durchgänge der Glättung.
    pub glaetten: usize,
    /// Höchstzahl der Runden Teilen–Neuvernetzen.
    pub max_runden: usize,
    /// Höchstzahl der Punkte je Teil.
    pub max_punkte: usize,
}

impl Default for Parameter {
    fn default() -> Self {
        Parameter {
            kante_mm: 10.0,
            winkel_grad: 25.0,
            sehne_mm: 0.1,
            laenge_mm: 0.1,
            fang_mm: 2.0,
            min_rand_mm: 0.5,
            glaetten: 6,
            max_runden: 30,
            max_punkte: 200_000,
        }
    }
}

/// Das Netz eines Teils.
#[derive(Debug, Clone)]
pub struct Teilnetz {
    pub teil: String,
    /// Punkte in Musterkoordinaten (mm, Achsen wie im Muster).
    pub punkte: Vec<[f64; 2]>,
    /// Dreiecke, positiv umlaufend in Musterkoordinaten.
    pub dreiecke: Vec<[u32; 3]>,
    /// Die ersten `nrand` Punkte sind der Rand in Knotenreihenfolge des Teils.
    pub nrand: usize,
    /// Je Randpunkt: Randkante und Bogenlänge ab ihrem Anfang (mm).
    pub randort: Vec<(usize, f64)>,
    /// Netzpunkt je Punktknoten (Name, Punkt); Punktknoten `i` ist der Anfang
    /// der Randkante `i`.
    pub punktknoten: Vec<(String, u32)>,
    /// Randkanten als Polylinien (mm), wie [`klon3d_pattern::kanten::Randkante`].
    pub kanten: Vec<Kantenzug>,
    /// Nahtlinie des Teils (mm), geschlossen gedacht.
    pub kontur: Vec<[f64; 2]>,
    /// Schnittlinie des Teils (mm), als geschlossener Ring ohne doppelten
    /// Schlusspunkt; leer, wenn das Teil keine Zugabe hat ([`schnitt`]).
    ///
    /// Dieselbe Linie wie die `Schnitt`-Spalte von `klon3d umriss`, der zweiten
    /// Linie im DXF-Export und der Zugabekontur der 2D-Ansicht. Nur Daten: kein
    /// Punkt, kein Dreieck und kein Nahtpaar hängt daran.
    pub schnitt: Vec<[f64; 2]>,
    /// Der Zugabestreifen des Teils: das Band zwischen Nahtlinie und
    /// Schnittlinie als Dreiecke, in mm, mit dem Randpunkt, an dem jeder
    /// Außenpunkt hängt ([`zugabe::Streifen`]).
    ///
    /// Leer, wenn das Teil keine Zugabe hat. Neue Daten: die Punkte, die
    /// Dreiecke und die Nahtpaare des Netzes bleiben, wie sie sind - das Band
    /// ist an den Randpunkten des Teils geschweißt, nach innen falten ist der
    /// nächste Schritt.
    pub zugabe: zugabe::Streifen,
    /// Der Fadenlauf des Teils, im selben Bezugssystem und in derselben
    /// Einheit wie [`Teilnetz::punkte`]; `None` ohne sichtbaren
    /// `<grainline>` (oder wenn er sich nicht auswerten ließ, siehe
    /// [`Musternetz::fehler`]).
    pub fadenlauf: Option<Fadenlauf>,
    /// Schlechte Dreiecke, die die Verfeinerung stehen lassen musste.
    pub aufgegeben: usize,
}

/// Der Fadenlauf eines Teils in Netzkoordinaten (mm, Achsen wie
/// [`Teilnetz::punkte`]).
///
/// `p1` ist das untere Ende der Achse, `p2` das obere. `pfeile` ist Seamly2Ds
/// `arrows`: 0 an beiden Enden eine Spitze, 1 nur an `p2`, 2 nur an `p1`. Die
/// Spitzen sind eine Zeichnungseigenschaft und erst in [`ausgabe::svg`]
/// umgesetzt, nicht Teil der Achse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fadenlauf {
    pub p1: [f64; 2],
    pub p2: [f64; 2],
    pub pfeile: u8,
}

impl Fadenlauf {
    /// The axis as a unit vector in pattern coordinates (mm, axes as
    /// [`Teilnetz::punkte`]); `None` when both ends coincide and the axis has no
    /// direction.
    ///
    /// The vector points from `p1` to `p2`. `pfeile` says nothing about the
    /// direction - it only decides which ends carry an arrowhead - so the sign
    /// is the one the pattern wrote, and a caller that needs the opposite
    /// orientation has to say so itself.
    pub fn richtung(&self) -> Option<[f64; 2]> {
        let d = [self.p2[0] - self.p1[0], self.p2[1] - self.p1[1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
        (l > 0.0).then(|| [d[0] / l, d[1] / l])
    }
}

/// Wie eine Stelle einer Naht zustande kam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stellenart {
    /// Nahtanfang oder -ende.
    Ende,
    /// Punktknoten auf beiden Seiten, zusammengelegt.
    Gemeinsam,
    /// Punktknoten nur auf Seite A; B bekommt einen eigenen Randpunkt.
    NurA,
    /// Punktknoten nur auf Seite B.
    NurB,
}

/// Eine Teilungsstelle einer Naht.
#[derive(Debug, Clone)]
pub struct Nahtstelle {
    pub art: Stellenart,
    /// Punktknoten auf A bzw. B (leer bei einem neuen Randpunkt).
    pub name_a: String,
    pub name_b: String,
    /// Längenanteil auf beiden Seiten.
    pub rho_a: f64,
    pub rho_b: f64,
    /// (ρ_A − ρ_B) × mittlere Seitenlänge: Abweichung von CLOs reiner
    /// Anteilsregel (mm).
    pub versatz_mm: f64,
}

/// Zwei Randpunkte, die vernäht werden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Punktpaar {
    /// Netzpunkt auf Teil A bzw. B.
    pub a: u32,
    pub b: u32,
    /// Längenanteil auf Seite A bzw. B.
    pub rho_a: f64,
    pub rho_b: f64,
}

/// Ein Stück einer Naht mit je einem Teil auf beiden Seiten (CLO:
/// SeamLinePair). Beide Seiten haben gleich viele Punkte; an den Enden teilt
/// eine Teilnaht ihre Punkte mit der vorigen und der nächsten.
#[derive(Debug, Clone)]
pub struct Teilnaht {
    /// Index in [`Musternetz::teile`].
    pub teil_a: usize,
    pub teil_b: usize,
    pub paare: Vec<Punktpaar>,
}

#[derive(Debug, Clone)]
pub struct Nahtnetz {
    pub kennung: String,
    pub name: String,
    pub laenge_a_mm: f64,
    pub laenge_b_mm: f64,
    pub stellen: Vec<Nahtstelle>,
    pub teilnaehte: Vec<Teilnaht>,
}

impl Nahtnetz {
    /// Alle Punktpaare in Nährichtung.
    pub fn paare(&self) -> impl Iterator<Item = (&Teilnaht, &Punktpaar)> {
        self.teilnaehte.iter().flat_map(|t| t.paare.iter().map(move |p| (t, p)))
    }
}

#[derive(Debug, Clone)]
pub struct Musternetz {
    pub parameter: Parameter,
    pub teile: Vec<Teilnetz>,
    pub naehte: Vec<Nahtnetz>,
    /// Teile, die nicht vernetzt werden konnten (Rand nicht auflösbar).
    pub fehler: Vec<String>,
    /// Runden Teilen–Neuvernetzen.
    pub runden: usize,
    /// Teilungswünsche, die nach der letzten Runde noch offen waren.
    pub offene_teilungen: usize,
}

impl Musternetz {
    pub fn teil(&self, name: &str) -> Option<&Teilnetz> {
        self.teile.iter().find(|t| t.teil == name || t.teil.rsplit(" - ").next() == Some(name))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetzFehler {
    #[error("Teil {teil}: {grund}")]
    Teil { teil: String, grund: String },
    #[error("Naht {kennung}: {grund}")]
    Naht { kennung: String, grund: String },
}

/// Vernetzt alle auflösbaren Teile des Musters und teilt die Nähte auf.
/// `naehte` aus [`klon3d_pattern::naehte::pruefe_naehte`] mit derselben
/// Kurvenquelle; leer für ein Netz ohne Nähte.
pub fn vernetze(
    ev: &Auswertung<'_>,
    quelle: Kurvenquelle,
    naehte: &[Nahtergebnis],
    p: &Parameter,
) -> Result<Musternetz, NetzFehler> {
    let mut geo = Vec::new();
    let mut fehler = Vec::new();
    for t in ev.muster().pieces() {
        let name = t.name.clone().unwrap_or_else(|| t.id.clone());
        let rand = match ev.teilrand(t, quelle).map_err(|e| e.to_string()) {
            Ok(r) => r,
            Err(e) => {
                fehler.push(format!("{name}: {e}"));
                continue;
            }
        };
        // A grainline that does not evaluate is a message, not a lost piece:
        // Seamly2D refuses to draw such a pattern, but the mesh of the piece
        // is fine without its axis.
        let fadenlauf = match ev.fadenlauf(t) {
            Ok(fl) => fl,
            Err(e) => {
                fehler.push(format!("{name}: Fadenlauf: {e}"));
                None
            }
        };
        match Teilgeo::aus(&rand, fadenlauf.as_ref()) {
            Ok(g) => geo.push(g),
            Err(e) => fehler.push(format!("{name}: {e}")),
        }
    }
    let mut plan = Randplan::neu(geo, naehte, p).map_err(|(kennung, grund)| NetzFehler::Naht { kennung, grund })?;
    let ziele = Ziele {
        kante_mm: p.kante_mm,
        winkel_grad: p.winkel_grad,
        min_rand_mm: p.min_rand_mm,
        max_punkte: p.max_punkte,
    };

    let n = plan.teile.len();
    let mut netze: Vec<Option<(Triangulierung, Randindex, usize)>> = (0..n).map(|_| None).collect();
    let mut offen: BTreeSet<usize> = (0..n).collect();
    let (mut runden, mut offene_teilungen) = (0, 0);
    loop {
        runden += 1;
        let mut wuensche = BTreeSet::new();
        for &ti in &offen {
            let ri = plan.randindex(ti);
            let mut tr = Triangulierung::neu(&ri.lagen)
                .map_err(|grund| NetzFehler::Teil { teil: plan.teile[ti].teil.clone(), grund })?;
            let erg = tr.verfeinere(&ziele);
            wuensche.extend(erg.teilen.iter().map(|&i| ri.besitzer[i]));
            netze[ti] = Some((tr, ri, erg.aufgegeben));
        }
        if wuensche.is_empty() {
            break;
        }
        if runden >= p.max_runden {
            offene_teilungen = wuensche.len();
            break;
        }
        offen = plan.teile_auf(&wuensche);
    }

    let mut netze: Vec<(Triangulierung, Randindex, usize)> = netze.into_iter().map(Option::unwrap).collect();
    let mut teile = Vec::with_capacity(n);
    for (ti, (tr, ri, aufgegeben)) in netze.iter_mut().enumerate() {
        tr.glaetten(p.glaetten);
        let g = &plan.teile[ti];
        let punktknoten = g.knoten.iter().enumerate().map(|(k, name)| (name.clone(), ri.knoten(k))).collect();
        // The band is welded to the boundary points of the finished mesh: the
        // points only exist after triangulation, so the weld happens here and
        // not in `Teilgeo::aus`.
        let zugabe = zugabe::verschweisst(&g.zugabe, &tr.pf[..tr.nrand], &g.kanten, &ri.orte);
        teile.push(Teilnetz {
            teil: g.teil.clone(),
            dreiecke: tr.dreiecke(),
            punkte: tr.pf.clone(),
            nrand: tr.nrand,
            randort: ri.orte.clone(),
            punktknoten,
            kanten: g.kanten.clone(),
            kontur: g.kontur.clone(),
            schnitt: g.schnitt.clone(),
            zugabe,
            fadenlauf: g.fadenlauf,
            aufgegeben: *aufgegeben,
        });
    }

    let mut nahtnetze = Vec::with_capacity(plan.naehte.len());
    for np in &plan.naehte {
        let mut teilnaehte = Vec::new();
        for tn in &np.teilnaehte {
            let (ria, rib) = (&netze[tn.teil_a].1, &netze[tn.teil_b].1);
            let mut paare = Vec::new();
            for (k, ab) in tn.abschnitte.iter().enumerate() {
                let g = &plan.gruppen[ab.gruppe];
                for (j, &f) in g.f.iter().enumerate() {
                    if k > 0 && j == 0 {
                        continue;
                    }
                    paare.push(Punktpaar {
                        a: plan.punkt(ria, &g.glieder[0], f),
                        b: plan.punkt(rib, &g.glieder[1], f),
                        rho_a: (ab.sigma_a[0] + f * (ab.sigma_a[1] - ab.sigma_a[0])) / np.la,
                        rho_b: (ab.sigma_b[0] + f * (ab.sigma_b[1] - ab.sigma_b[0])) / np.lb,
                    });
                }
            }
            teilnaehte.push(Teilnaht { teil_a: tn.teil_a, teil_b: tn.teil_b, paare });
        }
        nahtnetze.push(Nahtnetz {
            kennung: np.kennung.clone(),
            name: np.name.clone(),
            laenge_a_mm: np.la,
            laenge_b_mm: np.lb,
            stellen: np.stellen.clone(),
            teilnaehte,
        });
    }

    Ok(Musternetz { parameter: *p, teile, naehte: nahtnetze, fehler, runden, offene_teilungen })
}
