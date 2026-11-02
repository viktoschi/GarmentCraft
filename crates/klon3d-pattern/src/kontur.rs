//! Teilumrisse aus den Knotenketten.
//!
//! ## Die eine Regel, die alles entscheidet
//!
//! Eine Kurve geht **nicht als Ganzes** in den Umriss ein, sondern nur mit dem
//! Stück zwischen ihren Nachbarknoten. So bildet Seamly2D den Umriss
//! (`vpiecepath.cpp: PathPoints, StartSegment, EndSegment`):
//!
//! * Das Stück beginnt am Kurvenanfang — oder am vorigen Knoten, wenn der
//!   auf der Kurve liegt. Das Ende ebenso mit dem nächsten Knoten. Die Knoten
//!   bilden einen **Ring**.
//! * Ist der Nachbar selbst eine Kurve, zählt ihr zugewandtes Ende
//!   (`CurveStartPoint`/`CurveEndPoint`, Seamly-Issue #620).
//! * Geschnitten wird an der **nächstgelegenen** Strecke. Seamly2D nimmt für
//!   den Anfang die erste, für das Ende die letzte Strecke, auf der der Punkt
//!   liegt (`GetSegmentPoints`), und entfernt danach Punkte, die näher als
//!   0,1555 mm beieinander liegen (`mainPathPoints`). Ohne diesen zweiten
//!   Schritt entstehen Rückläufe an Kerben: Am 20.10.2026 versucht, wurden
//!   251 Teile des Bestands länger, der Bolero-Bischofärmel 0,11 cm gegen
//!   Seamlys DXF. Die nächstgelegene Strecke trifft Seamlys DXF an 257 von
//!   270 Teilen auf 0,0005 cm.
//! * Den Anfang schneidet Seamly2D ab zwei Knoten, das Ende erst ab drei: ein
//!   Teil aus einem Punkt und einer geschlossenen Kurve behält die Kurve ganz.
//! * Ausgeschlossene Knoten (`excluded`) sind weder Kante noch Nachbar.
//!
//! Belegt an `bolero_34_v28`, „Bischofärmel links":
//!
//! | | Knoten | Punkte | größter Sprung | Abweichung zu Seamly |
//! |---|---:|---:|---:|---:|
//! | ganze Kurve je Knoten | 92 | 2.219 | 43,12 cm | 83,9 mm |
//! | **Teilstück** | 92 | 260 | — | **0,51 mm** |
//!
//! Ursache des Zickzacks: ein Passzeichen auf einer Kurve steht als
//! Punktknoten **zwischen** zwei Knoten derselben Kurve. Ohne Trimmung kommt
//! die Kurve zweimal ganz und springt an ihren Anfang zurück.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::geom::{Auswertung, GeomError, Pt};
use crate::model::Piece;

/// Toleranz „liegt auf der Kurve" in Mustereinheiten.
///
/// `vgobject.cpp: accuracyPointOnLine` = 0,1555 mm / 25,4 · 96 px — ein halbes
/// Pixel bei 96 dpi.
pub fn toleranz(ev: &Auswertung<'_>) -> f64 {
    crate::geom::ON_CURVE_MM / ev.mm_pro_einheit()
}

/// Ein aufgelöster Knoten einer Teilekontur.
#[derive(Debug, Clone)]
pub struct Knoten {
    /// `true` = Punktknoten, `false` = Kurvenknoten.
    pub ist_punkt: bool,
    pub punkte: Vec<Pt>,
    /// Rechenobjekt, aus dem der Knoten stammt.
    pub ref_id: String,
    /// Laufrichtung umgekehrt.
    pub rueckwaerts: bool,
    /// Stelle in `piece.nodes.items` — für Kerbe und Nahtzugabe des Knotens.
    pub knoten_nr: usize,
}

/// Abstand des Punkts `p` von der Strecke `a`–`b`.
fn abstand_strecke(a: Pt, b: Pt, p: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2 }.clamp(0.0, 1.0);
    p.dist(Pt::new(a.x + t * dx, a.y + t * dy))
}

/// Strecke von `pts`, an der die Kurve für den Nachbarpunkt `p` geschnitten
/// wird: die nächstgelegene, wenn `p` auf der Kurve liegt. Ob er das tut,
/// entscheidet `pruef` (eine feinere Zerlegung derselben Kurve), sonst `pts`.
fn schnittstrecke(pts: &[Pt], p: Pt, tol: f64, pruef: Option<&[Pt]>) -> Option<usize> {
    let (j, d, _) = projektion(pts, p);
    let auf = match pruef {
        Some(z) => (0..z.len().saturating_sub(1)).any(|i| abstand_strecke(z[i], z[i + 1], p) <= tol),
        None => d <= tol,
    };
    auf.then_some(j)
}

/// Nächster Punkt auf der Polylinie: (Segmentindex, Abstand, Fußpunkt).
pub fn projektion(pts: &[Pt], p: Pt) -> (usize, f64, Pt) {
    let mut best = (0usize, f64::INFINITY, pts[0]);
    for i in 0..pts.len().saturating_sub(1) {
        let (x1, y1) = (pts[i].x, pts[i].y);
        let (dx, dy) = (pts[i + 1].x - x1, pts[i + 1].y - y1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 == 0.0 {
            0.0
        } else {
            ((p.x - x1) * dx + (p.y - y1) * dy) / l2
        }
        .clamp(0.0, 1.0);
        let q = Pt::new(x1 + t * dx, y1 + t * dy);
        let d = p.dist(q);
        if d < best.1 {
            best = (i, d, q);
        }
    }
    best
}

impl Auswertung<'_> {
    /// Modellier-Objekte: Modellier-Id → (Punkt oder Kurve, Rechenobjekt-Id).
    pub fn modeling_index(&self) -> HashMap<String, (bool, String)> {
        let mut out = HashMap::new();
        for b in &self.muster().draft_blocks {
            if let Some(m) = &b.modeling {
                for p in &m.points {
                    if let Some(z) = &p.id_object {
                        out.insert(p.id.clone(), (true, z.clone()));
                    }
                }
                for s in &m.splines {
                    if let Some(z) = &s.id_object {
                        out.insert(s.id.clone(), (false, z.clone()));
                    }
                }
                // Modellierte Bögen zählen als Kurvenknoten. Im Bustier sind
                // 8 der 154 Modellierobjekte Bögen; ohne sie fehlen genau die
                // Knoten, an denen die Kontur sonst auseinanderfällt.
                for a in &m.arcs {
                    if let Some(z) = &a.id_object {
                        out.insert(a.id.clone(), (false, z.clone()));
                    }
                }
            }
        }
        out
    }

    /// Knotenkette eines Stücks aufgelöst — **vor** dem Zuschneiden.
    ///
    /// Jeder Knoten braucht seine beiden Nachbarn, deshalb werden erst alle
    /// aufgelöst. Knoten, deren Ziel nicht auflösbar ist, werden übergangen und
    /// gemeldet. (Seamly2D lädt ein solches Teil gar nicht; der Umriss hier
    /// bleibt trotzdem sichtbar, [`Auswertung::teilrand`] lehnt ihn ab.)
    pub fn stueck_knoten(&self, stueck: &Piece) -> (Vec<Knoten>, Vec<String>) {
        let index = self.modeling_index();
        let mut knoten = Vec::new();
        let mut unauflösbar = Vec::new();
        let knoten_liste = stueck
            .nodes
            .as_ref()
            .map(|n| n.items.as_slice())
            .unwrap_or(&[]);

        for (knoten_nr, k) in knoten_liste.iter().enumerate() {
            // Ausgeschlossene Knoten sind weder Kante noch Nachbar.
            if matches!(k.exclude.as_deref(), Some("1") | Some("true")) {
                continue;
            }
            let Some((ist_punkt, ref_id)) = index.get(&k.id_object) else {
                unauflösbar.push(format!("Knoten {} ohne Modellierziel", k.id_object));
                continue;
            };
            let rueckwaerts = matches!(k.reverse.as_deref(), Some("1") | Some("true"));
            let r = if *ist_punkt {
                self.punkt(ref_id).map(|p| vec![p])
            } else {
                self.kurve(ref_id)
            };
            match r {
                Ok(mut punkte) => {
                    if punkte.is_empty() {
                        continue;
                    }
                    if rueckwaerts {
                        punkte.reverse();
                    }
                    knoten.push(Knoten {
                        ist_punkt: *ist_punkt,
                        punkte,
                        ref_id: ref_id.clone(),
                        rueckwaerts,
                        knoten_nr,
                    });
                }
                Err(e) => unauflösbar.push(format!("Knoten {}: {e}", k.id_object)),
            }
        }
        (knoten, unauflösbar)
    }

    /// Nahtlinie eines Stücks, wie Seamly2D sie bildet.
    pub fn stueck_umriss(&self, stueck: &Piece) -> Result<Vec<Pt>, GeomError> {
        let (knoten, _meldung) = self.stueck_knoten(stueck);
        Ok(self.umriss_aus_knoten(&knoten))
    }

    /// Nahtlinie aus bereits aufgelösten Knoten.
    pub fn umriss_aus_knoten(&self, knoten: &[Knoten]) -> Vec<Pt> {
        self.umriss_mit_marken(knoten).0
    }

    /// Nahtlinie wie [`Self::umriss_aus_knoten`], dazu je Knoten die Stelle
    /// seines **letzten** Punkts im Umriss (`None`, wenn er nichts beiträgt).
    /// Bei einem Punktknoten ist das der Punkt selbst — daran wird der Umriss
    /// in Randkanten geteilt ([`crate::kanten`]).
    pub fn umriss_mit_marken(&self, knoten: &[Knoten]) -> (Vec<Pt>, Vec<Option<usize>>) {
        self.umriss_entschieden(knoten, None)
    }

    /// Wie [`Self::umriss_mit_marken`]; ob ein Nachbar auf einer Kurve liegt,
    /// entscheidet aber `entscheid` (gleiche Knoten, andere Kurvenpunkte).
    ///
    /// Seamly2D prüft das auf seiner Zerlegung. Die gezeichnete Polylinie (12
    /// Schritte je Pfadsegment) liegt bis 0,18 mm daneben und verfehlte so die
    /// Kerben am Bischofärmel — die Kante lief im Zickzack, 38 cm zu lang.
    pub fn umriss_entschieden(
        &self,
        knoten: &[Knoten],
        entscheid: Option<&[Knoten]>,
    ) -> (Vec<Pt>, Vec<Option<usize>>) {
        let tol = toleranz(self);
        let n = knoten.len();
        let mut out: Vec<Pt> = Vec::new();
        let mut marken = vec![None; n];

        for i in 0..n {
            let k = &knoten[i];
            let mut seg = k.punkte.clone();

            if !k.ist_punkt && seg.len() >= 2 {
                let prev = &knoten[(i + n - 1) % n];
                let next = &knoten[(i + 1) % n];
                let pruef = entscheid.map(|e| e[i].punkte.as_slice());
                // Anfang: am vorigen Punkt oder am Ende der vorigen Kurve.
                if n > 1 {
                    let p = *prev.punkte.last().unwrap();
                    if let Some(j) = schnittstrecke(&seg, p, tol, pruef) {
                        seg = std::iter::once(p).chain(seg[j + 1..].iter().copied()).collect();
                    }
                }
                // Ende: am nächsten Punkt oder am Anfang der nächsten Kurve.
                if n > 2 && seg.len() >= 2 {
                    let p = next.punkte[0];
                    if let Some(j) = schnittstrecke(&seg, p, tol, pruef) {
                        seg = seg[..j + 1].iter().copied().chain(std::iter::once(p)).collect();
                    }
                }
            }

            if seg.is_empty() {
                continue;
            }
            if let Some(letzte) = out.last() {
                if letzte.dist(seg[0]) <= 1e-9 {
                    out.extend(seg[1..].iter().copied());
                } else {
                    out.extend(seg);
                }
            } else {
                out.extend(seg);
            }
            marken[i] = Some(out.len() - 1);
        }
        let mut fest = vec![false; out.len()];
        for (k, m) in knoten.iter().zip(&marken) {
            if let (true, Some(s)) = (k.ist_punkt, m) {
                fest[*s] = true;
            }
        }
        self.naht_bereinigt(out, marken, &fest)
    }

    /// The chained points cleaned like Seamly2D's `mainPathPoints`
    /// ([`crate::schleifen`]): duplicates within 0.1555 mm, spikes and loops
    /// out. Surviving points keep their exact values; a node whose point was
    /// removed is moved to the point that replaces it. The points of point
    /// nodes (`fest`) are not merged away.
    fn naht_bereinigt(
        &self,
        out: Vec<Pt>,
        marken: Vec<Option<usize>>,
        fest: &[bool],
    ) -> (Vec<Pt>, Vec<Option<usize>>) {
        let px: Vec<[f64; 2]> = out.iter().map(|p| [self.zu_px(p.x), self.zu_px(p.y)]).collect();
        let (neu, karte) = crate::schleifen::naht_bereinigen_fest(&px, fest);
        if neu.len() == px.len() && karte.iter().enumerate().all(|(i, &k)| i == k) {
            return (out, marken);
        }
        let mut herkunft: Vec<Option<usize>> = vec![None; neu.len()];
        for (k, &o) in karte.iter().enumerate() {
            if herkunft[o].is_none() && px[k] == neu[o] {
                herkunft[o] = Some(k);
            }
        }
        let umriss = neu
            .iter()
            .zip(&herkunft)
            .map(|(q, h)| match h {
                Some(k) => out[*k],
                None => Pt::new(self.von_px(q[0]), self.von_px(q[1])),
            })
            .collect();
        (umriss, marken.into_iter().map(|m| m.map(|s| karte[s])).collect())
    }

    /// Länge einer Nahtlinie in Mustereinheiten.
    pub fn umriss_laenge(&self, umriss: &[Pt]) -> f64 {
        umriss.windows(2).map(|w| w[0].dist(w[1])).sum()
    }
}

/// Cache, damit der Modelling-Index nicht je Stück neu gebaut wird.
#[derive(Default)]
pub struct Konturcache {
    modeling: RefCell<Option<HashMap<String, (bool, String)>>>,
}

impl Konturcache {
    pub fn index(&self, ev: &Auswertung<'_>) -> HashMap<String, (bool, String)> {
        {
            if let Some(i) = self.modeling.borrow().as_ref() {
                return i.clone();
            }
        }
        let i = ev.modeling_index();
        *self.modeling.borrow_mut() = Some(i.clone());
        i
    }
}