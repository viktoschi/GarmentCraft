//! Randkanten: die Nahtlinie eines Teils, geteilt an seinen Punktknoten.
//!
//! ## Was eine Kante ist
//!
//! Eine Randkante läuft von einem **Punktknoten** des Teils zum nächsten;
//! Kurvenknoten dazwischen teilen sie nicht. Dieselbe Teilung kennen beide
//! Vorbilder:
//!
//! * Seamly2D: Das Vereinigen-Werkzeug zählt als Kante das Stück zwischen
//!   zwei aufeinanderfolgenden Punktknoten (`union_tool.cpp`, `indexD1`).
//! * CLO: Ein Umriss ist eine Liste von Linien von Segmentpunkt zu
//!   Segmentpunkt, Kurvenpunkte liegen dazwischen
//!   (`spez/naehte_2026_1.md`, Abschnitt 1).
//!
//! Dass jeder Punktknoten ein Segmentpunkt wird, ist eine **Annahme**: Wie
//! CLO einen DXF-Polygonzug beim Import in Segment- und Kurvenpunkte teilt,
//! ist offen (Spezifikation, Abschnitt 8). In CLO sitzt eine Kerbe als Anteil
//! auf einer Linie; hier ist sie ein Punktknoten und steht deshalb immer an
//! einem Kantenende. Eine Naht ist eine Folge **ganzer** Kanten
//! ([`crate::naehte`]).
//!
//! ## Woraus die Kurvenpunkte kommen
//!
//! [`Kurvenquelle::Seamly`] nimmt Seamly2Ds eigene Zerlegung der Kurven
//! (`seamly_zug`, in Seamly2D `getPoints`). Damit bildet Seamly2D seinen
//! Teileumriss, und diese Länge setzt es in Formeln wie `Spl_VT_L_VX_L` ein;
//! an der Bluse3 v2 stimmt sie mit einer 32-fach feineren Abtastung auf
//! 0,0001 cm überein. [`Kurvenquelle::Zeichnung`] nimmt die gezeichnete
//! Polylinie (48 Schritte je Spline, 12 je Pfadsegment) wie
//! [`Auswertung::stueck_umriss`] und `klon3d dxf` — das, was CLO aus der DXF
//! bekommt. Ob ein Nachbarknoten auf der Kurve liegt, wird hier aber auf
//! Seamlys Zerlegung entschieden; `umriss` und `dxf` entscheiden noch auf der
//! Zeichnung und verfehlen so Kerben auf Pfadkurven (Bischofärmel). Im
//! Musterbestand ist die Zeichnung je Kante bis 0,03 cm kürzer; an der
//! Bluse3 v2 gibt sie die Zahlen von `pruef_bluse3.py`.
//!
//! CLO misst eine Linie ebenso als Summe der Teilstrecken einer gespeicherten
//! Polylinie, nicht als Bogenlänge (Spezifikation, Abschnitt 2).

use crate::geom::{Auswertung, GeomError, Pt};
use crate::model::Piece;

/// Quelle der Kurvenpunkte einer Randkante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kurvenquelle {
    /// Seamly2Ds Zerlegung — die Länge, mit der Seamly2D selbst rechnet.
    #[default]
    Seamly,
    /// Die gezeichnete Polylinie wie im Umriss.
    Zeichnung,
}

/// Ein Punktknoten des Teils — Anfang und Ende von Randkanten.
#[derive(Debug, Clone)]
pub struct Punktknoten {
    /// Name des Rechenpunkts, ohne Namen seine id.
    pub name: String,
    /// id des Rechenpunkts.
    pub id: String,
    /// Stelle in `piece.nodes.items`.
    pub knoten_nr: usize,
    /// Passzeichen (`notch="true"`).
    pub kerbe: bool,
    /// Nahtzugabe vor und nach dem Knoten in Mustereinheiten
    /// (`before`/`after`, sonst die Breite des Teils). `None`, wenn Seamly2D
    /// am Teil keine Zugabe anlegt (`seamAllowance="false"` oder
    /// `seamAllowanceBuiltIn="true"`).
    pub zugabe_vor: Option<f64>,
    pub zugabe_nach: Option<f64>,
    /// Stelle des Punkts in [`Teilrand::umriss`].
    pub stelle: usize,
}

/// Eine Randkante von `punktknoten[von]` nach `punktknoten[bis]`.
#[derive(Debug, Clone)]
pub struct Randkante {
    pub von: usize,
    pub bis: usize,
    /// Punkte der Nahtlinie in Laufrichtung des Teils, beide Enden enthalten.
    pub punkte: Vec<Pt>,
    /// Länge in Mustereinheiten.
    pub laenge: f64,
}

/// Die Nahtlinie eines Teils, geteilt in Randkanten.
#[derive(Debug, Clone)]
pub struct Teilrand {
    pub teil: String,
    /// Nahtlinie wie [`Auswertung::umriss_aus_knoten`], offen.
    pub umriss: Vec<Pt>,
    /// In Knotenreihenfolge.
    pub punktknoten: Vec<Punktknoten>,
    /// `kanten[i]` läuft von `punktknoten[i]` zu `punktknoten[i + 1]`, die
    /// letzte zurück zum ersten. Ein Teil ohne Punktknoten hat keine Kante.
    pub kanten: Vec<Randkante>,
    /// Mustereinheit → mm.
    pub mm_pro_einheit: f64,
}

/// Ein zusammenhängendes Stück des Rands von einem Punktknoten zu einem
/// anderen, vorwärts oder rückwärts in der Knotenreihenfolge.
#[derive(Debug, Clone)]
pub struct Abschnitt {
    /// Indizes in [`Teilrand::kanten`], in Laufrichtung.
    pub kanten: Vec<usize>,
    pub rueckwaerts: bool,
    /// Länge in Mustereinheiten.
    pub laenge: f64,
    /// Punktknoten im Inneren des Abschnitts mit ihrem Abstand vom Anfang
    /// (Mustereinheiten), in Laufrichtung.
    pub innen: Vec<(usize, f64)>,
}

fn ja(w: Option<&str>) -> bool {
    matches!(w, Some("1") | Some("true"))
}

fn streckenzug(p: &[Pt]) -> f64 {
    p.windows(2).map(|w| w[0].dist(w[1])).sum()
}

impl Auswertung<'_> {
    /// Randkanten eines Teils.
    ///
    /// Anders als [`Auswertung::stueck_umriss`] ist ein unauflösbarer Knoten
    /// ein Fehler: Seamly2D lädt ein solches Teil nicht, und jede Länge über
    /// den Knoten hinweg wäre falsch.
    pub fn teilrand(&self, stueck: &Piece, quelle: Kurvenquelle) -> Result<Teilrand, GeomError> {
        let teil = stueck.name.clone().unwrap_or_else(|| stueck.id.clone());
        let (mut knoten, meldungen) = self.stueck_knoten(stueck);
        if !meldungen.is_empty() {
            return Err(GeomError::Geometrie {
                kind: "Teilrand",
                grund: format!("{teil}: {}", meldungen.join("; ")),
            });
        }
        // Kurvenknoten mit Seamlys Zerlegung.
        let mut zerlegt = knoten.clone();
        for k in zerlegt.iter_mut().filter(|k| !k.ist_punkt) {
            if self.linie(&k.ref_id).is_some() {
                continue;
            }
            let mut p: Vec<Pt> = self
                .seamly_zug(&k.ref_id)?
                .iter()
                .map(|q| Pt::new(self.von_px(q[0]), self.von_px(q[1])))
                .collect();
            if k.rueckwaerts {
                p.reverse();
            }
            k.punkte = p;
        }
        let (umriss, marken) = match quelle {
            Kurvenquelle::Seamly => {
                knoten = zerlegt;
                self.umriss_mit_marken(&knoten)
            }
            Kurvenquelle::Zeichnung => self.umriss_entschieden(&knoten, Some(&zerlegt)),
        };

        // Nahtzugabe: ohne `seamAllowance` oder mit eingebauter Zugabe
        // (`seamAllowanceBuiltIn`, die Kontur ist dann schon die Schnittlinie)
        // keine; sonst Knotenwert, und wo er fehlt oder negativ ist, die
        // Breite des Teils. Nicht berücksichtigt: `<CSA>`-Pfade, die die
        // Zugabe zwischen zwei Knoten ersetzen, und der Anstieg auf die Breite
        // an Kurvenübergängen innerhalb einer Kante.
        let mit_zugabe =
            ja(stueck.seam_allowance.as_deref()) && !ja(stueck.seam_allowance_built_in.as_deref());
        let breite = self.formel_zahl(stueck.width.as_deref(), 0.0);
        let items = stueck.nodes.as_ref().map(|n| n.items.as_slice()).unwrap_or(&[]);
        let zugabe = |w: Option<&String>| -> Option<f64> {
            if !mit_zugabe {
                return None;
            }
            let z = self.formel_zahl(w.map(String::as_str), breite);
            Some(if z < 0.0 { breite } else { z })
        };

        let mut punktknoten = Vec::new();
        let mut eigene: Vec<Pt> = Vec::new();
        for (k, stelle) in knoten.iter().zip(&marken) {
            let (true, Some(stelle)) = (k.ist_punkt, *stelle) else { continue };
            let roh = &items[k.knoten_nr];
            eigene.push(k.punkte[0]);
            punktknoten.push(Punktknoten {
                name: self.punkt_name(&k.ref_id).unwrap_or(&k.ref_id).to_string(),
                id: k.ref_id.clone(),
                knoten_nr: k.knoten_nr,
                kerbe: ja(roh.notch.as_deref()),
                zugabe_vor: zugabe(roh.before.as_ref()),
                zugabe_nach: zugabe(roh.after.as_ref()),
                stelle,
            });
        }
        // A point node inside a loop that the seam line lost
        // ([`crate::schleifen`]) has been moved to the point that replaced
        // the loop, where another node already sits. It bounds no edge any
        // more and is dropped, instead of leaving edges of length zero.
        let tol = crate::kontur::toleranz(self);
        let geteilt = |s: usize| punktknoten.iter().filter(|p| p.stelle == s).count() > 1;
        let weg: Vec<bool> = punktknoten
            .iter()
            .zip(&eigene)
            .map(|(p, e)| e.dist(umriss[p.stelle]) > tol && geteilt(p.stelle))
            .collect();
        if weg.iter().any(|&w| w) && weg.iter().any(|&w| !w) {
            let mut w = weg.iter();
            punktknoten.retain(|_| !*w.next().unwrap());
        }

        let n = punktknoten.len();
        let mut kanten = Vec::with_capacity(n);
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b) = (punktknoten[i].stelle, punktknoten[j].stelle);
            // Die letzte Kante (und bei einem einzigen Punktknoten die
            // einzige) läuft über das Ende des Umrisses zurück an den Anfang.
            // A node whose point fell together with the first point of the
            // ring ([`crate::schleifen`]) stands at index 0 at the end of the
            // ring, so the order of the indices decides, not of the nodes.
            let roh: Vec<Pt> = if a < b || (a == b && n > 1) {
                umriss[a..=b].to_vec()
            } else {
                umriss[a..].iter().chain(&umriss[..=b]).copied().collect()
            };
            let mut punkte: Vec<Pt> = Vec::with_capacity(roh.len());
            for p in roh {
                if punkte.last().map_or(true, |q: &Pt| q.dist(p) > 1e-12) {
                    punkte.push(p);
                }
            }
            let laenge = streckenzug(&punkte);
            kanten.push(Randkante { von: i, bis: j, punkte, laenge });
        }

        Ok(Teilrand {
            teil,
            umriss,
            punktknoten,
            kanten,
            mm_pro_einheit: self.mm_pro_einheit(),
        })
    }
}

impl Teilrand {
    /// Umfang der Nahtlinie in Mustereinheiten (Summe der Kanten).
    pub fn umfang(&self) -> f64 {
        self.kanten.iter().map(|k| k.laenge).sum()
    }

    /// Mustereinheiten → cm.
    pub fn in_cm(&self, wert: f64) -> f64 {
        wert * self.mm_pro_einheit / 10.0
    }

    /// Punktknoten mit diesem Rechenpunktnamen. Fehler, wenn er fehlt oder
    /// mehrfach vorkommt.
    pub fn knoten_nach_name(&self, name: &str) -> Result<usize, String> {
        let treffer: Vec<usize> = (0..self.punktknoten.len())
            .filter(|&i| self.punktknoten[i].name == name)
            .collect();
        match treffer.as_slice() {
            [i] => Ok(*i),
            [] => Err(format!("{}: kein Punktknoten {name}", self.teil)),
            _ => Err(format!("{}: Punktknoten {name} kommt {}-mal vor", self.teil, treffer.len())),
        }
    }

    /// Nahtzugabe einer Kante: nach ihrem Anfang und vor ihrem Ende.
    pub fn zugabe(&self, kante: usize) -> (Option<f64>, Option<f64>) {
        let k = &self.kanten[kante];
        (self.punktknoten[k.von].zugabe_nach, self.punktknoten[k.bis].zugabe_vor)
    }

    /// Freie Kante: das Teil hat Nahtzugabe, an beiden Enden ist sie 0.
    pub fn ist_frei(&self, kante: usize) -> bool {
        matches!(self.zugabe(kante), (Some(a), Some(b)) if a.abs() < 1e-9 && b.abs() < 1e-9)
    }

    /// Abschnitt von Punktknoten `von` nach `bis`. Vorwärts läuft er über
    /// die Kanten `von, von + 1, …`, rückwärts über `von − 1, von − 2, …`.
    /// `von == bis` ist der ganze Umfang.
    pub fn abschnitt(&self, von: usize, bis: usize, rueckwaerts: bool) -> Abschnitt {
        let n = self.punktknoten.len();
        let mut kanten = Vec::new();
        let mut innen = Vec::new();
        let mut laenge = 0.0;
        let mut i = von;
        loop {
            let (kante, weiter) = if rueckwaerts {
                let k = (i + n - 1) % n;
                (k, k)
            } else {
                (i, (i + 1) % n)
            };
            kanten.push(kante);
            laenge += self.kanten[kante].laenge;
            i = weiter;
            if i == bis {
                break;
            }
            innen.push((i, laenge));
        }
        Abschnitt { kanten, rueckwaerts, laenge, innen }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_str;
    use std::collections::HashMap;

    /// Quadrat 10 × 10 cm aus vier Punkten, eine Kante als Spline, eine
    /// Kerbe auf der Unterkante, Nahtzugabe 1 außer an der Oberkante.
    const QUADRAT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
  <version>0.7.4</version>
  <unit>cm</unit>
  <draftBlock name="A">
    <calculation>
      <point id="1" name="A" type="single" x="0" y="0" mx="0" my="0"/>
      <point id="2" name="B" type="single" x="10" y="0" mx="0" my="0"/>
      <point id="3" name="C" type="single" x="10" y="10" mx="0" my="0"/>
      <point id="4" name="D" type="single" x="0" y="10" mx="0" my="0"/>
      <point id="5" name="M" type="single" x="5" y="10" mx="0" my="0"/>
      <spline id="6" type="simpleInteractive" point1="2" point4="3" angle1="0" angle2="180" length1="0" length2="0" color="black"/>
    </calculation>
    <modeling>
      <point id="11" idObject="1" type="modeling"/>
      <point id="12" idObject="2" type="modeling"/>
      <point id="13" idObject="3" type="modeling"/>
      <point id="14" idObject="4" type="modeling"/>
      <point id="15" idObject="5" type="modeling"/>
      <spline id="16" idObject="6" type="modelingSpline"/>
    </modeling>
    <pieces>
      <piece id="20" name="Quadrat" seamAllowance="true" width="1">
        <nodes>
          <node idObject="11" type="NodePoint" before="1" after="0"/>
          <node idObject="12" type="NodePoint" before="0"/>
          <node idObject="16" type="NodeSpline"/>
          <node idObject="13" type="NodePoint"/>
          <node idObject="15" type="NodePoint" notch="true"/>
          <node idObject="14" type="NodePoint"/>
        </nodes>
      </piece>
    </pieces>
  </draftBlock>
</pattern>"#;

    fn rand(quelle: Kurvenquelle) -> Teilrand {
        let m = read_str(QUADRAT).unwrap();
        let ev = Auswertung::new(&m, HashMap::new());
        let teil = m.pieces()[0];
        ev.teilrand(teil, quelle).unwrap()
    }

    #[test]
    fn kanten_zwischen_punktknoten() {
        let r = rand(Kurvenquelle::Seamly);
        let namen: Vec<&str> = r.punktknoten.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(namen, ["A", "B", "C", "M", "D"]);
        assert_eq!(r.kanten.len(), 5);
        let l: Vec<f64> = r.kanten.iter().map(|k| k.laenge).collect();
        assert!((l[0] - 10.0).abs() < 1e-9, "A-B {}", l[0]);
        // B-C ist die gerade Spline (Griffe 0): Länge 10.
        assert!((l[1] - 10.0).abs() < 1e-6, "B-C {}", l[1]);
        assert!((l[2] - 5.0).abs() < 1e-9 && (l[3] - 5.0).abs() < 1e-9);
        // D-A schließt den Ring.
        assert!((l[4] - 10.0).abs() < 1e-9, "D-A {}", l[4]);
        assert!((r.umfang() - 40.0).abs() < 1e-6);
        assert!(r.punktknoten[3].kerbe && !r.punktknoten[2].kerbe);
    }

    #[test]
    fn zugabe_und_freie_kante() {
        let r = rand(Kurvenquelle::Seamly);
        // A after=0, B before=0: Oberkante frei; die übrigen Breite 1.
        assert!(r.ist_frei(0));
        assert_eq!(r.zugabe(1), (Some(1.0), Some(1.0)));
        assert_eq!(r.punktknoten[0].zugabe_vor, Some(1.0));
        assert!(!r.ist_frei(4));
    }

    /// Ein Muster aus einem Block mit Punkten, Splines und einem Teil.
    fn muster(punkte: &str, splines: &str, modell: &str, teil: &str, rest: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern><version>0.7.4</version><unit>cm</unit>
  <draftBlock name="A">
    <calculation>{punkte}{splines}</calculation>
    <modeling>{modell}</modeling>
    <pieces>{teil}</pieces>
  </draftBlock>{rest}
</pattern>"#
        )
    }

    fn p(id: u32, name: &str, x: f64, y: f64) -> String {
        format!(r#"<point id="{id}" name="{name}" type="single" x="{x}" y="{y}" mx="0" my="0"/>"#)
    }

    /// Gerade Spline (Griffe 0) von `a` nach `b`.
    fn s(id: u32, a: u32, b: u32) -> String {
        format!(
            r#"<spline id="{id}" type="simpleInteractive" point1="{a}" point4="{b}" angle1="0" angle2="0" length1="0" length2="0" color="black"/>"#
        )
    }

    fn mp(id: u32, ziel: u32) -> String {
        format!(r#"<point id="{id}" idObject="{ziel}" type="modeling"/>"#)
    }

    fn ms(id: u32, ziel: u32) -> String {
        format!(r#"<spline id="{id}" idObject="{ziel}" type="modelingSpline"/>"#)
    }

    fn ein_rand(xml: &str) -> Teilrand {
        let m = read_str(xml).unwrap();
        let ev = Auswertung::new(&m, HashMap::new());
        ev.teilrand(m.pieces()[0], Kurvenquelle::Seamly).unwrap()
    }

    /// Seamly-Issue #620: Beginnt die nächste Kurve mitten auf dieser, endet
    /// diese dort. A(0,0)→B(10,0), dann M(5,0)→D(5,10): A→D ist 15, nicht 25.
    #[test]
    fn kurve_an_kurve_wird_geschnitten() {
        let pk = [p(1, "A", 0.0, 0.0), p(2, "B", 10.0, 0.0), p(3, "M", 5.0, 0.0), p(4, "D", 5.0, 10.0)].concat();
        let sp = [s(5, 1, 2), s(6, 3, 4)].concat();
        let mo = [mp(11, 1), mp(14, 4), ms(15, 5), ms(16, 6)].concat();
        let teil = r#"<piece id="20" name="KK" seamAllowance="false"><nodes>
            <node idObject="11" type="NodePoint"/><node idObject="15" type="NodeSpline"/>
            <node idObject="16" type="NodeSpline"/><node idObject="14" type="NodePoint"/></nodes></piece>"#;
        let r = ein_rand(&muster(&pk, &sp, &mo, teil, ""));
        assert_eq!(r.kanten.len(), 2);
        assert!((r.kanten[0].laenge - 15.0).abs() < 1e-6, "A→D {}", r.kanten[0].laenge);
        assert!((r.umfang() - (15.0 + 125f64.sqrt())).abs() < 1e-6, "{}", r.umfang());
    }

    /// Zwei Knoten (Punkt und Kurve): Seamly2D schneidet das Kurvenende erst
    /// ab drei Knoten. Die Kurve bleibt ganz, der Ring schließt mit der Sehne.
    #[test]
    fn zwei_knoten_behalten_die_kurve() {
        let pk = [p(1, "A", 0.0, 0.0), p(2, "B", 10.0, 0.0)].concat();
        let mo = [mp(11, 1), ms(15, 5)].concat();
        let teil = r#"<piece id="20" name="Zwei" seamAllowance="false"><nodes>
            <node idObject="11" type="NodePoint"/><node idObject="15" type="NodeSpline"/></nodes></piece>"#;
        let r = ein_rand(&muster(&pk, &s(5, 1, 2), &mo, teil, ""));
        assert_eq!(r.kanten.len(), 1);
        assert!((r.kanten[0].laenge - 20.0).abs() < 1e-6, "{}", r.kanten[0].laenge);
    }

    /// `excluded="true"`: weder Ecke noch Kerbe.
    #[test]
    fn ausgeschlossener_knoten_zaehlt_nicht() {
        let xml = QUADRAT.replace(
            r#"<node idObject="15" type="NodePoint" notch="true"/>"#,
            r#"<node idObject="15" type="NodePoint" notch="true" excluded="true"/>"#,
        );
        assert_ne!(xml, QUADRAT);
        let r = ein_rand(&xml);
        let namen: Vec<&str> = r.punktknoten.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(namen, ["A", "B", "C", "D"]);
        assert!(r.punktknoten.iter().all(|p| !p.kerbe));
    }

    /// Knoten eines Innenpfads (`<modeling><path>`) in einem späteren Block
    /// gehören nicht zum Teil davor (vor dem 20.10.2026: Bund des Rocks v34).
    #[test]
    fn innenpfad_haengt_nicht_am_teil() {
        let block_b = r#"
  <draftBlock name="B">
    <calculation><point id="30" name="P1" type="single" x="50" y="50" mx="0" my="0"/>
      <point id="31" name="P2" type="single" x="90" y="50" mx="0" my="0"/></calculation>
    <modeling><point id="40" idObject="30" type="modeling"/><point id="41" idObject="31" type="modeling"/>
      <path id="42" type="2" name="Klebezone"><nodes>
        <node idObject="40" type="NodePoint"/><node idObject="41" type="NodePoint"/></nodes></path>
    </modeling>
    <pieces/>
  </draftBlock>"#;
        let xml = QUADRAT.replace("</draftBlock>\n</pattern>", &format!("</draftBlock>{block_b}\n</pattern>"));
        assert_ne!(xml, QUADRAT);
        let m = read_str(&xml).unwrap();
        assert_eq!(m.pieces()[0].nodes.as_ref().unwrap().items.len(), 6);
        let r = ein_rand(&xml);
        assert_eq!(r.kanten.len(), 5);
        assert!((r.umfang() - 40.0).abs() < 1e-6);
    }

    /// `seamAllowanceBuiltIn`: Seamly2D legt keine Zugabe an.
    #[test]
    fn eingebaute_zugabe_ist_keine() {
        let xml = QUADRAT.replace(r#"seamAllowance="true""#, r#"seamAllowance="true" seamAllowanceBuiltIn="true""#);
        assert_ne!(xml, QUADRAT);
        let r = ein_rand(&xml);
        assert!(r.punktknoten.iter().all(|p| p.zugabe_vor.is_none() && p.zugabe_nach.is_none()));
        assert!(!r.ist_frei(0));
    }

    #[test]
    fn abschnitt_vorwaerts_rueckwaerts_ganz() {
        let r = rand(Kurvenquelle::Zeichnung);
        let (b, d) = (r.knoten_nach_name("B").unwrap(), r.knoten_nach_name("D").unwrap());
        let v = r.abschnitt(b, d, false);
        assert_eq!(v.kanten, [1, 2, 3]);
        assert!((v.laenge - 20.0).abs() < 1e-6);
        assert_eq!(v.innen.iter().map(|x| x.0).collect::<Vec<_>>(), [2, 3]);
        assert!((v.innen[1].1 - 15.0).abs() < 1e-6);
        let r2 = r.abschnitt(b, d, true);
        assert_eq!(r2.kanten, [0, 4]);
        assert!((r2.laenge - 20.0).abs() < 1e-6);
        let ganz = r.abschnitt(b, b, false);
        assert_eq!(ganz.kanten.len(), 5);
        assert!(r.knoten_nach_name("X").is_err());
    }
}
