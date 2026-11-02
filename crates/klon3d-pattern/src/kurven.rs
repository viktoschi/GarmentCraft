//! Kurven: Spline, Pfad, Bogen.
//!
//! Alle Verfahren sind Seamly2Ds C++ nachgebildet; die Belege stehen in
//! `findings/13-schnitt-py-als-spezifikation.md`.
//!
//! ## Wichtig: die Länge misst Seamly nicht als Bogenlänge
//!
//! Seamly2D zerlegt eine Bezier-Kurve in einen Streckenzug
//! (`vabstractcubicbezier.cpp: GetCubicBezierPoints`, Algorithmus aus
//! Anti-Grain Geometry) und misst **diesen**. Gemessen an
//! `Spl_VQB_L_VA_L` (Bluse 1 v19, in cm):
//!
//! | Verfahren | Wert |
//! |---|---:|
//! | Seamly2D | 10,4141095 |
//! | gezeichnete Polylinie, 48 Schritte | 10,4136258 |
//! | gezeichnete Polylinie, 400 Schritte | 10,4140876 |
//! | **Seamlys Zerlegung** | **10,4141095** |
//!
//! An diesen Längen hängen Schnittpunkte, und Seamly2D rundet eine
//! Schnittlänge auf vier Stellen — eine Abweichung in der fünften Stelle
//! kann dort die Entscheidung kippen. Deshalb wird hier dieselbe Zerlegung
//! nachgebaut und nicht die wahre Bogenlänge angenähert.
//!
//! Die Zerlegung arbeitet in **Pixeln** (Seamly2D rechnet in Bildpunkten,
//! 96 je Zoll — `def.cpp: PrintDPI`), die Toleranz `0,5 px` ist deshalb
//! ein fester Zahlenwert und keine umgerechnete Länge.

use crate::geom::{Auswertung, GeomError, Pt};
use crate::model::{Arc, Spline};

/// Pixel je Millimeter -- ToPixel(1, Unit::Mm); zugleich die Klemme
/// fuer Schnitte.
pub const PX_PRO_MM: f64 = (1.0 / 25.4) * 96.0;

/// Strahlenlaenge: std::numeric_limits<int>::max() / 2, praktisch unendlich.
pub const RAY_PX: f64 = 2147483647.0 / 2.0;

/// Anti-Grain Geometry: beide Kontrollpunkte zusammen hoechstens 0,5 px neben
/// der Sehne.
const FLACH: f64 = 0.25; // (0,5 px)^2
/// `curve_collinearity_epsilon`
const EPS: f64 = 1e-30;
/// `curve_recursion_limit`
const MAX_TIEFE: u32 = 32;

/// Pixel je Zoll — `def.cpp: PrintDPI`.
const DPI: f64 = 96.0;

impl Auswertung<'_> {
    // ───────────────────────────────────────────── Einheitenwechsel ──

    /// Mustereinheit → Pixel.
    pub fn zu_px(&self, wert: f64) -> f64 {
        wert * self.mm_pro_einheit() / 25.4 * DPI
    }

    /// Pixel → Mustereinheit.
    pub fn von_px(&self, px: f64) -> f64 {
        px * 25.4 / DPI / self.mm_pro_einheit()
    }

    // ─────────────────────────────────────────────────── Kontrollen ──

    /// Kontrollpunkte `[p0, p1, p2, p3]` je Bézier-Segment.
    ///
    /// Zeichnung, Länge und Schnitt nehmen **dieselben** Kontrollpunkte und
    /// unterscheiden sich nur darin, wie sie abtasten.
    pub fn spline_kontrollen(&self, s: &Spline) -> Result<Vec<[Pt; 4]>, GeomError> {
        let t = s.kind.as_deref().unwrap_or("simpleInteractive");
        match t {
            "simpleInteractive" => {
                let p0 = self.bez_pt(s.point1.as_deref(), "point1")?;
                let p3 = self.bez_pt(s.point4.as_deref(), "point4")?;
                // Die Griffe hängen an den Enden und zeigen **von** p0 weg
                // bzw. **nach** p3 hin.
                let (a1, l1) = self.handle(s.angle1.as_deref(), s.length1.as_deref())?;
                let (a2, l2) = self.handle(s.angle2.as_deref(), s.length2.as_deref())?;
                Ok(vec![[
                    p0,
                    { let (dx, dy) = crate::geom::vektor(a1, l1); Pt::new(p0.x + dx, p0.y + dy) },
                    { let (dx, dy) = crate::geom::vektor(a2, l2); Pt::new(p3.x + dx, p3.y + dy) },
                    p3,
                ]])
            }

            // `cubicBezier` names its four control points `point1`..`point4`
            // (`vpattern.cpp: ParseToolCubicBezier`). It used to read the two
            // inner ones from attributes that do not exist and put them at
            // the origin: in L036 a cut on such a spline sat 109 mm off.
            "cubicBezier" => Ok(vec![[
                self.bez_pt(s.point1.as_deref(), "point1")?,
                self.bez_pt(s.point2.as_deref(), "point2")?,
                self.bez_pt(s.point3.as_deref(), "point3")?,
                self.bez_pt(s.point4.as_deref(), "point4")?,
            ]]),

            "pathInteractive" => self.pfad_kontrollen(s),
            "cubicBezierPath" => self.bezierpfad_kontrollen(s),

            "modelingSpline" | "modelingPath" => Err(GeomError::Geometrie {
                kind: "Kurve",
                grund: format!("{}: Modellierobjekt, nicht die Quelle", t),
            }),

            andere => Err(GeomError::Geometrie {
                kind: "Kurve",
                grund: format!("nicht implementierter Spline-Typ: {andere}"),
            }),
        }
    }

    pub(crate) fn handle(&self, ang: Option<&str>, laenge: Option<&str>) -> Result<(f64, f64), GeomError> {
        let a = self.formel_zahl(ang, 0.0);
        let l = self.formel_zahl(laenge, 0.0);
        Ok((a, l))
    }

    pub(crate) fn bez_pt(&self, id: Option<&str>, feld: &str) -> Result<Pt, GeomError> {
        let id = id.filter(|s| !s.is_empty()).ok_or(GeomError::Geometrie {
            kind: "Kurve",
            grund: format!("{feld} fehlt"),
        })?;
        self.punkt(id)
    }

    /// Kontrollpunkte der Segmente eines Pfads.
    ///
    /// Ein Pfad läuft durch eine Folge von **Punkten**. Trotz des
    /// irreführenden Attributnamens `pSpline` steht dort eine Punkt-id. Jeder
    /// `<pathPoint>` trägt die Griffe an seinem Punkt: `angle1`/`length1` =
    /// Zulauf, `angle2`/`length2` = Ablauf. Beim ersten Punkt ist `length1`
    /// 0, beim letzten `length2` 0 — daher die Segmentbildung aus dem Ablauf
    /// von Pi und dem Zulauf von Pi+1.
    pub fn pfad_kontrollen(&self, s: &Spline) -> Result<Vec<[Pt; 4]>, GeomError> {
        let kinder: Vec<_> = s
            .path_points
            .iter()
            .filter(|p| p.p_spline.is_some())
            .collect();
        if kinder.is_empty() {
            return Err(GeomError::Geometrie {
                kind: "Pfad",
                grund: format!("Pfad {} ohne pathPoint-Kinder", s.id),
            });
        }
        let winkel: Vec<(f64, f64)> = kinder.iter().map(|k| self.pfad_angeln(k)).collect();

        let mut segmente = Vec::with_capacity(kinder.len().saturating_sub(1));
        for i in 0..kinder.len() - 1 {
            let p0 = self.bez_pt(kinder[i].p_spline.as_deref(), "pSpline")?;
            let p3 = self.bez_pt(kinder[i + 1].p_spline.as_deref(), "pSpline")?;
            // Ablauf aus p0
            let (a_out, l_out) =
                self.handle(Some(&winkel[i].1.to_string()), kinder[i].length2.as_deref())?;
            // Zulauf in p3
            let (a_in, l_in) =
                self.handle(Some(&winkel[i + 1].0.to_string()), kinder[i + 1].length1.as_deref())?;
            segmente.push([
                p0,
                { let (dx, dy) = crate::geom::vektor(a_out, l_out); Pt::new(p0.x + dx, p0.y + dy) },
                { let (dx, dy) = crate::geom::vektor(a_in, l_in); Pt::new(p3.x + dx, p3.y + dy) },
                p3,
            ]);
        }
        Ok(segmente)
    }

    /// Control points of a `cubicBezierPath`: its `<pathPoint>`s are the
    /// control points themselves, without handles, and segment `k` runs over
    /// points `3k` to `3k + 3` (`vcubicbezierpath.cpp: GetSpline,
    /// CountSubSpl`; points past the last full segment do not count). It
    /// used to be read like a `pathInteractive`, as a curve through every
    /// point: in L036 a cut on such a path sat 11 mm off.
    pub fn bezierpfad_kontrollen(&self, s: &Spline) -> Result<Vec<[Pt; 4]>, GeomError> {
        let punkte = s
            .path_points
            .iter()
            .filter_map(|p| p.p_spline.as_deref())
            .map(|id| self.bez_pt(Some(id), "pSpline"))
            .collect::<Result<Vec<Pt>, GeomError>>()?;
        if punkte.len() < 4 {
            return Err(GeomError::Geometrie {
                kind: "Pfad",
                grund: format!("Bézier-Pfad {} mit {} Punkten, mindestens 4 nötig", s.id, punkte.len()),
            });
        }
        Ok((0..(punkte.len() - 1) / 3)
            .map(|k| [punkte[3 * k], punkte[3 * k + 1], punkte[3 * k + 2], punkte[3 * k + 3]])
            .collect())
    }

    /// Zulauf- und Ablaufwinkel eines Pfadknotens, wie Seamly2D sie führt.
    ///
    /// In Seamly2D ist jeder Pfadknoten **glatt**. Liegen `angle1` und
    /// `angle2` nicht 180° auseinander, überschreibt es beim Laden einen der
    /// beiden mit dem anderen + 180 (`vsplinepoint_p.h: VSplinePointData`).
    /// Es bleibt `angle1` — außer `angle1` ist eine einzelne Zahl und
    /// `angle2` eine Formel, dann bleibt `angle2`.
    ///
    /// Belegt am Randfall-Muster: Pfadlängen lagen bis 0,1890 cm daneben,
    /// die Achsenschnitte dahinter bis 4,52 mm. In den 39 vorhandenen
    /// Dateien haben 36 von 3167 Knoten solche Winkel.
    pub fn pfad_angeln(&self, pp: &crate::model::PathPoint) -> (f64, f64) {
        let roh1 = pp.angle1.clone().unwrap_or_default();
        let roh2 = pp.angle2.clone().unwrap_or_default();
        let a1 = self.formel_zahl(Some(&roh1), 0.0);
        let a2 = self.formel_zahl(Some(&roh2), 0.0);
        let abstand = (a1 - a2).abs();
        // `VFuzzyComparePossibleNulls` mit Genauigkeit 1e-12
        if ((abstand - 180.0).abs() * 1e12) <= abstand.min(180.0) {
            return (a1, a2);
        }
        if !Self::ist_einfach(&roh1) || Self::ist_einfach(&roh2) {
            (a1, (a1 + 180.0) % 360.0)
        } else {
            ((a2 + 180.0) % 360.0, a2)
        }
    }

    /// Ist die Formel eine einzelne Zahl? (`qmutokenparser.cpp: IsSingle`)
    ///
    /// Genau eine Zahl und kein Name einer Grösse; ein Vorzeichen zählt nicht.
    pub fn ist_einfach(formel: &str) -> bool {
        let s = formel.trim();
        let s = s.strip_prefix(['+', '-']).unwrap_or(s).trim_start();
        if s.is_empty() {
            return false;
        }
        s.parse::<f64>().is_ok()
    }

    // ─────────────────────────────────────────────── Seamlys Streckenzug ──

    /// Der Streckenzug, in den Seamly2D eine Bezier-Kurve zerlegt.
    ///
    /// Anti-Grain Geometry: halbieren, bis beide Kontrollpunkte zusammen
    /// höchstens 0,5 px neben der Sehne liegen. Je flachem Stück kommt die
    /// **Mitte seiner Kontrollpunkte** dazu (kein Kurvenpunkt); Anfang und
    /// Ende der Kurve stehen fest. Winkel- und Spitzengrenze sind in Seamly2D
    /// auf 0 gesetzt und damit außer Betrieb.
    pub fn seamly_punkte(&self, k: [Pt; 4]) -> Vec<Pt> {
        let px: Vec<[f64; 2]> = k
            .iter()
            .map(|p| [self.zu_px(p.x), self.zu_px(p.y)])
            .collect();
        let mut pts = vec![[px[0][0], px[0][1]]];
        teile_auf(
            px[0][0], px[0][1],
            px[1][0], px[1][1],
            px[2][0], px[2][1],
            px[3][0], px[3][1],
            0,
            &mut pts,
        );
        pts.push([px[3][0], px[3][1]]);
        pts.into_iter()
            .map(|p| Pt::new(self.von_px(p[0]), self.von_px(p[1])))
            .collect()
    }

    /// Länge einer Bézier-Kurve, wie Seamly2D sie misst
    /// (`LengthBezier`): die Länge des Streckenzugs aus [`Self::seamly_punkte`].
    pub fn seamly_laenge(&self, k: [Pt; 4]) -> f64 {
        Self::polyline_laenge(&self.seamly_punkte(k))
    }

    /// Länge einer Polylinie in Mustereinheiten.
    pub fn polyline_laenge(p: &[Pt]) -> f64 {
        p.windows(2).map(|w| w[0].dist(w[1])).sum()
    }

    /// Länge einer Spline oder eines Pfads — der Wert hinter `Spl_` und
    /// `SplPath_`. Ein Pfad ist die Summe seiner Segmente
    /// (`vabstractcubicbezierpath.cpp: GetLength`).
    pub fn spline_laenge(&self, s: &Spline) -> Result<f64, GeomError> {
        self.kurven_laenge(&s.id)
    }

    /// Länge der Kurve `cid` (Spline, Pfad oder Kopie) aus ihrer Zerlegung.
    pub fn kurven_laenge(&self, cid: &str) -> Result<f64, GeomError> {
        Ok(self.kontrollen(cid)?.into_iter().map(|k| self.seamly_laenge(k)).sum())
    }

    // ──────────────────────────────────────────────────── Zeichnen ──

    /// Abtastung einer Bézier-Kurve als Polylinie.
    pub fn bezier_abtasten(&self, k: [Pt; 4], schritte: usize) -> Vec<Pt> {
        let n = schritte.max(1);
        (0..=n)
            .map(|i| {
                let u = i as f64 / n as f64;
                let m = 1.0 - u;
                let (a, b, c, d) = (m * m * m, 3.0 * m * m * u, 3.0 * m * u * u, u * u * u);
                Pt::new(
                    a * k[0].x + b * k[1].x + c * k[2].x + d * k[3].x,
                    a * k[0].y + b * k[1].y + c * k[2].y + d * k[3].y,
                )
            })
            .collect()
    }

    /// Gezeichnete Polylinie eines Splines oder Pfades.
    ///
    /// Ein Pfad wird je Segment mit `schritte / 4` abgetastet, ein einfacher
    /// Spline mit `schritte`.
    pub fn spline_zeichnung(&self, s: &Spline, schritte: usize) -> Result<Vec<Pt>, GeomError> {
        self.spline_zeichnung_id(&s.id, schritte)
    }

    /// Wie [`Auswertung::spline_zeichnung`], für eine Spline, einen Pfad oder
    /// ihre Operationskopie.
    pub fn spline_zeichnung_id(&self, cid: &str, schritte: usize) -> Result<Vec<Pt>, GeomError> {
        let segmente = self.kontrollen(cid)?;
        let pfad = self.ist_pfad(cid);
        let pro = if pfad { (schritte / 4).max(1) } else { schritte.max(1) };
        let mut out = Vec::new();
        for k in segmente {
            let teil = self.bezier_abtasten(k, pro);
            out.extend(if out.is_empty() { teil } else { teil[1..].to_vec() });
        }
        Ok(out)
    }

    /// Polyylinie einer Kurve, mit Cache. Ein Linienobjekt liefert zwei
    /// Punkte, ein Spline seine Zeichnung, ein Bogen seine Kreispunkte.
    pub fn kurve(&self, id: &str) -> Result<Vec<Pt>, GeomError> {
        if let Some(k) = self.kurve_cache.borrow().get(id) {
            return Ok(k.clone());
        }
        let pts = if let Some(l) = self.linie(id) {
            vec![
                self.bez_pt(l.first_point.as_deref(), "firstPoint")?,
                self.bez_pt(l.second_point.as_deref(), "secondPoint")?,
            ]
        } else if self.ist_spline(id) {
            self.spline_zeichnung_id(id, 48)?
        } else if self.ist_bogen(id) {
            Self::bogen_zeichnung_d(&self.bogen_daten_id(id)?, 64)
        } else {
            return Err(GeomError::UnknownPoint(format!("Kurve {id}")));
        };
        self.kurve_cache
            .borrow_mut()
            .insert(id.to_string(), pts.clone());
        Ok(pts)
    }

    /// Länge der Spline zwischen den Punkten `a` und `b` — der Formelname
    /// `Spl_a_b`.
    ///
    /// Gesucht wird die Spline, deren Endpunkte `a` und `b` sind (in beiden
    /// Richtungen).
    pub fn spline_zwischen(&self, a: &str, b: &str) -> Result<f64, GeomError> {
        for s in self.alle_splines() {
            // `point1`/`point4` eines Splines sind **Ids**, keine Namen.
            let (p1, p4) = (
                s.point1.as_deref().filter(|s| !s.is_empty()),
                s.point4.as_deref().filter(|s| !s.is_empty()),
            );
            let (na, nb) = (self.id_von_name(a), self.id_von_name(b));
            if (p1 == na && p4 == nb) || (p1 == nb && p4 == na) {
                return self.spline_laenge(s);
            }
        }
        // Kopien und Teilkurven heißen nach den Namen ihrer Enden.
        for z in self.abgeleitete_kurven() {
            if self.ist_pfad(&z) {
                continue;
            }
            if let Some((p1, p4)) = self.kurven_enden(&z) {
                if (p1 == a && p4 == b) || (p1 == b && p4 == a) {
                    return self.kurven_laenge(&z);
                }
            }
        }
        Err(GeomError::Geometrie {
            kind: "Spl_",
            grund: format!("keine Spline zwischen {a} und {b}"),
        })
    }

    /// Länge des Pfades zwischen den Punkten `a` und `b` — `SplPath_a_b`.
    ///
    /// Seamly2D benennt einen Pfad nach seinem **ersten und seinem letzten**
    /// Punkt (`vabstractcubicbezierpath.cpp: CreateName`) und gibt die Länge
    /// des **ganzen** Pfads zurück, nicht die Teilstrecke zwischen den beiden
    /// Punkten. Zwei Pfade mit denselben Enden tragen denselben Namen; welcher
    /// gemeint ist, steht nirgends — dann wird keiner geraten.
    ///
    /// Vorher stand hier „irgendein Pfad, der beide Punkte enthält, Summe der
    /// Segmente dazwischen“. Das war an zwei Stellen falsch: es übersah
    /// `duplicate="1"` (deren Pfad heißt in Seamly2D `SplPath_a_b_<n>`), und es
    /// lieferte die Teilstrecke. Gemessen an Bluse 2 v26:
    /// `SplPath_B1_AM0_AE1` 23,4418 statt 23,9052 und
    /// `SplPath_B1_AM0_AE2` 24,4575 statt 25,4723 — die Kontrolllinie K13
    /// wich dadurch um 14,78 cm von ihrem Sollwert ab.
    pub fn pfad_zwischen(&self, a: &str, b: &str) -> Result<f64, GeomError> {
        let pa = self.id_von_name(a);
        let pb = self.id_von_name(b);
        let mut treffer: Vec<&str> = Vec::new();

        for s in self.alle_splines() {
            if !matches!(
                s.kind.as_deref(),
                Some("pathInteractive") | Some("cubicBezierPath")
            ) {
                continue;
            }
            // `duplicate="1"` heisst in Seamly2D `SplPath_a_b_<n>`.
            if !matches!(s.duplicate.as_deref(), None | Some("") | Some("0")) {
                continue;
            }
            let kinder: Vec<&str> = s
                .path_points
                .iter()
                .filter_map(|k| k.p_spline.as_deref())
                .collect();
            if kinder.len() < 2 {
                continue;
            }
            if Some(kinder[0]) == pa.as_deref() && Some(kinder[kinder.len() - 1]) == pb.as_deref() {
                treffer.push(s.id.as_str());
            }
        }
        // Kopien und Teile von Pfaden heißen nach den Namen ihrer Enden.
        let abgeleitet = self.abgeleitete_kurven();
        for z in &abgeleitet {
            if self.ist_pfad(z) && self.kurven_enden(z).is_some_and(|(p1, p4)| p1 == a && p4 == b) {
                treffer.push(z.as_str());
            }
        }

        match treffer.len() {
            0 => Err(GeomError::Geometrie {
                kind: "SplPath_",
                grund: format!("kein Pfad {a}->{b}"),
            }),
            1 => self.kurven_laenge(treffer[0]),
            n => Err(GeomError::Geometrie {
                kind: "SplPath_",
                grund: format!("Pfad {a}->{b} mehrdeutig: {n} Pfade"),
            }),
        }
    }
}

/// Anti-Grain-Geometry-Unterteilung, Pixel.
#[allow(clippy::too_many_arguments)]
/// Dasselbe wie [	eile_auf], von aussen erreichbar.
pub fn teile_auf_oeffentlich(
    x1: f64, y1: f64,
    x2: f64, y2: f64,
    x3: f64, y3: f64,
    x4: f64, y4: f64,
    tiefe: u32,
    pts: &mut Vec<[f64; 2]>,
) {
    teile_auf(x1, y1, x2, y2, x3, y3, x4, y4, tiefe, pts)
}

#[allow(clippy::too_many_arguments)]
fn teile_auf(
    x1: f64, y1: f64,
    x2: f64, y2: f64,
    x3: f64, y3: f64,
    x4: f64, y4: f64,
    tiefe: u32,
    pts: &mut Vec<[f64; 2]>,
) {
    if tiefe > MAX_TIEFE {
        return;
    }
    let (x12, y12) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
    let (x23, y23) = ((x2 + x3) / 2.0, (y2 + y3) / 2.0);
    let (x34, y34) = ((x3 + x4) / 2.0, (y3 + y4) / 2.0);
    let (x123, y123) = ((x12 + x23) / 2.0, (y12 + y23) / 2.0);
    let (x234, y234) = ((x23 + x34) / 2.0, (y23 + y34) / 2.0);
    let (x1234, y1234) = ((x123 + x234) / 2.0, (y123 + y234) / 2.0);

    let (dx, dy) = (x4 - x1, y4 - y1);
    let mut d2 = ((x2 - x4) * dy - (y2 - y4) * dx).abs();
    let mut d3 = ((x3 - x4) * dy - (y3 - y4) * dx).abs();
    let sehne = dx * dx + dy * dy;

    if d2 > EPS && d3 > EPS {
        if (d2 + d3) * (d2 + d3) <= FLACH * sehne {
            pts.push([x23, y23]);
            return;
        }
    } else if d2 > EPS {
        // p1, p3, p4 auf einer Geraden
        if d2 * d2 <= FLACH * sehne {
            pts.push([x23, y23]);
            return;
        }
    } else if d3 > EPS {
        // p1, p2, p4 auf einer Geraden
        if d3 * d3 <= FLACH * sehne {
            pts.push([x23, y23]);
            return;
        }
    } else {
        // Alle auf einer Geraden, oder p1 == p4
        if sehne < 1e-9 {
            d2 = (x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1);
            d3 = (x4 - x3) * (x4 - x3) + (y4 - y3) * (y4 - y3);
        } else {
            let k = 1.0 / sehne;
            d2 = k * ((x2 - x1) * dx + (y2 - y1) * dy);
            d3 = k * ((x3 - x1) * dx + (y3 - y1) * dy);
            if 0.0 < d2 && d2 < 1.0 && 0.0 < d3 && d3 < 1.0 {
                return; // 1–2–3–4: die Enden genügen
            }
            if d2 <= 0.0 {
                d2 = (x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1);
            } else if d2 >= 1.0 {
                d2 = (x2 - x4) * (x2 - x4) + (y2 - y4) * (y2 - y4);
            } else {
                let q = (x1 + d2 * dx, y1 + d2 * dy);
                d2 = (x2 - q.0) * (x2 - q.0) + (y2 - q.1) * (y2 - q.1);
            }
            if d3 <= 0.0 {
                d3 = (x3 - x1) * (x3 - x1) + (y3 - y1) * (y3 - y1);
            } else if d3 >= 1.0 {
                d3 = (x3 - x4) * (x3 - x4) + (y3 - y4) * (y3 - y4);
            } else {
                let q = (x1 + d3 * dx, y1 + d3 * dy);
                d3 = (x3 - q.0) * (x3 - q.0) + (y3 - q.1) * (y3 - q.1);
            }
        }
        if d2 > d3 {
            if d2 < FLACH {
                pts.push([x2, y2]);
                return;
            }
        } else if d3 < FLACH {
            pts.push([x3, y3]);
            return;
        }
    }

    teile_auf(x1, y1, x12, y12, x123, y123, x1234, y1234, tiefe + 1, pts);
    teile_auf(x1234, y1234, x234, y234, x34, y34, x4, y4, tiefe + 1, pts);
}

/// Platz eines Bogens im Dokument, damit `Arc_<Mitte>_<id>` auflösbar wird.
impl Auswertung<'_> {
    /// Länge des Kreisbogens mit der angegebenen Mitte und Radius.
    ///
    /// `Arc_<Mitte>_<id>` verweist auf den Bogen **dieser** Mitte mit genau
    /// dieser id — nicht auf eine Laenge zwischen zwei Punkten. Boegen haben
    /// keine Endpunkte (`varc.cpp: InitArc`).
    pub fn bogen_radius(&self, radius: f64) -> f64 {
        radius
    }
}

/// Nur damit `Arc` hier einen Platz hat; die Bogenlogik folgt.
pub type BogenNachschlag = Arc;