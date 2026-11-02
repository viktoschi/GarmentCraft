//! Schneiden auf Kurven: `cutSpline`, `cutSplinePath`, `curveIntersectAxis`.
//!
//! Alle Verfahren aus Seamly2Ds C++ nachgebildet:
//!
//! * `vtoolcutspline.cpp`, `vtoolcutsplinepath.cpp`, `vtoolcutarc.cpp: Create`
//! * `vabstractcubicbezier.cpp: CutSpline, GetParmT, LengthBezier`
//! * `vabstractcubicbezierpath.cpp: CutSplinePath`
//! * `vtoolcurveintersectaxis.cpp: Create, FindPoint`
//! * `vabstractcurve.cpp: IntersectLine, CurveIntersectLine`
//!
//! **Nicht** auf der gezeichneten Polylinie schneiden. An den 174
//! Schnittpunkten von Paket 21 lagen darauf 56 über 0,1 mm daneben, der
//! weiteste 0,97 mm (CBa33T im Trenchcoat). Auf Seamly2Ds eigener Zerlegung
//! stimmen 164 von 169 auf 0,0000003 cm.

use crate::geom::{Auswertung, GeomError, Pt};
use crate::kurven::{teile_auf_oeffentlich, PX_PRO_MM, RAY_PX};
use crate::model::Point;

/// 1 mm in Pixeln — `ToPixel(1, Unit::Mm)`, zugleich die Klemme für Schnitte.
const MM1: f64 = PX_PRO_MM;

/// Kontrollpunkte des Kurvenstücks `0..t` (de Casteljau).
///
/// Der letzte ist der Kurvenpunkt bei `t`. So teilt Seamly2D in `LengthT`
/// und `CutSpline` — dort mit `QLineF::setLength(Laenge * t)`, was dasselbe ist.
fn bezier_kopf(k: [[f64; 2]; 4], t: f64) -> [[f64; 2]; 4] {
    let anteil = |p: [f64; 2], q: [f64; 2]| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
    let p12 = anteil(k[0], k[1]);
    let p23 = anteil(k[1], k[2]);
    let p34 = anteil(k[2], k[3]);
    let p123 = anteil(p12, p23);
    let p234 = anteil(p23, p34);
    [k[0], p12, p123, anteil(p123, p234)]
}

/// Länge eines Kontrollpunktsatzes, alles in Pixeln.
fn laenge_px(k: [[f64; 2]; 4]) -> f64 {
    let mut pts = vec![k[0]];
    teile_auf_oeffentlich(
        k[0][0], k[0][1], k[1][0], k[1][1], k[2][0], k[2][1], k[3][0], k[3][1], 0, &mut pts,
    );
    pts.push(k[3]);
    pts.windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .sum()
}

/// Kurvenparameter, bei dem Seamly2D die Länge `laenge` erreicht sieht.
///
/// `vabstractcubicbezier.cpp: GetParmT`. Halbierung ab `t = 0,5` mit Schritt
/// 0,25, 0,125, …; sie endet, sobald das Stück `0..t` auf 0,1 % der verlangten
/// Länge passt. Der Schnitt liegt deshalb bis zu 0,1 % der Schnittlänge neben
/// der verlangten Stelle, und `t` ist immer ein Bruch mit einer Zweierpotenz im
/// Nenner.
fn parm_t(k: [[f64; 2]; 4], laenge: f64, voll: f64) -> Result<f64, GeomError> {
    if laenge < 0.0 {
        return Ok(0.0);
    }
    let laenge = laenge.min(voll);
    let eps = 0.001 * laenge;
    let mut t = 0.5f64;
    let mut schritt = 0.5f64;
    let mut ist = laenge_px(bezier_kopf(k, t));
    let mut n = 0u32;
    while (ist - laenge).abs() > eps {
        if n == 60 {
            // In Seamly2D liefe die Schleife hier ohne Ende weiter.
            return Err(GeomError::Geometrie {
                kind: "Schnitt",
                grund: "Seamly2Ds Suche nach dem Schnittparameter endet nicht".into(),
            });
        }
        n += 1;
        schritt /= 2.0;
        t = if ist > laenge { t - schritt } else { t + schritt };
        ist = laenge_px(bezier_kopf(k, t));
    }
    Ok(t)
}

/// Kontrollpunkte des Kurvenstücks `t..1` (de Casteljau): `[p1234, p234,
/// p34, p4]` — der zweite Teil, den `CutSpline` liefert.
fn bezier_schwanz(k: [[f64; 2]; 4], t: f64) -> [[f64; 2]; 4] {
    let anteil = |p: [f64; 2], q: [f64; 2]| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
    let p12 = anteil(k[0], k[1]);
    let p23 = anteil(k[1], k[2]);
    let p34 = anteil(k[2], k[3]);
    let p123 = anteil(p12, p23);
    let p234 = anteil(p23, p34);
    [anteil(p123, p234), p234, p34, k[3]]
}

/// Eine Bézier-Kurve bei `laenge` (Pixel) geteilt: Kopf und Schwanz, deren
/// gemeinsamer Punkt der Schnittpunkt ist.
///
/// `vabstractcubicbezier.cpp: CutSpline`. `None`, wenn die Kurve höchstens
/// 1 mm lang ist — Seamly2D gibt dann `QPointF()` zurück.
fn cut_teile(k: [[f64; 2]; 4], mut laenge: f64) -> Option<([[f64; 2]; 4], [[f64; 2]; 4])> {
    let voll = laenge_px(k);
    if voll <= MM1 {
        return None;
    }
    if laenge < MM1 {
        laenge = MM1;
    } else if laenge > voll - MM1 {
        laenge = voll - MM1;
    }
    let t = parm_t(k, laenge, voll).ok()?;
    Some((bezier_kopf(k, t), bezier_schwanz(k, t)))
}

/// Ein Pfad bei `laenge` (Pixel) geteilt: Index des geschnittenen Segments,
/// Kopf und Schwanz dieses Segments.
///
/// `vabstractcubicbezierpath.cpp: CutSplinePath`. Erst wird die Länge auf den
/// ganzen Pfad geklemmt (1 mm vom Anfang, 1 mm vom Ende), dann das Segment
/// gesucht, in dem sie endet, und dort geschnitten — mit derselben Klemme noch
/// einmal, jetzt auf das Segment. Ein Schnitt, der näher als 1 mm an einem
/// Pfadknoten verlangt ist, liegt deshalb 1 mm neben dem Knoten.
fn cut_pfad_teile(
    segmente: &[[[f64; 2]; 4]],
    laengen: &[f64],
    mut laenge: f64,
) -> Option<(usize, [[f64; 2]; 4], [[f64; 2]; 4])> {
    let voll: f64 = laengen.iter().sum();
    if voll <= MM1 {
        return None;
    }
    if laenge < MM1 {
        laenge = MM1;
    } else if laenge > voll - MM1 {
        laenge = voll - MM1;
    }
    let mut bisher = 0.0;
    for (i, (seg, seglen)) in segmente.iter().zip(laengen).enumerate() {
        bisher += seglen;
        if bisher > laenge {
            return cut_teile(*seg, laenge - (bisher - seglen)).map(|(kopf, schwanz)| (i, kopf, schwanz));
        }
    }
    None
}

/// Schnittpunkt der Strecken `p1-p2` und `q1-q2`, wenn er auf beiden liegt.
///
/// `QLineF::intersects` (Qt 6.11) mit `BoundedIntersection` — so fragt
/// Seamly2D (`vabstractcurve.cpp: CurveIntersectLine`). Die Enden zählen mit,
/// parallele Strecken schneiden sich nie, und der Punkt wird auf der
/// **ersten** Strecke gerechnet.
pub fn schnitt(p1: [f64; 2], p2: [f64; 2], q1: [f64; 2], q2: [f64; 2]) -> Option<[f64; 2]> {
    let (ax, ay) = (p2[0] - p1[0], p2[1] - p1[1]);
    let (bx, by) = (q1[0] - q2[0], q1[1] - q2[1]);
    let (cx, cy) = (p1[0] - q1[0], p1[1] - q1[1]);
    let nenner = ay * bx - ax * by;
    if nenner == 0.0 || !nenner.is_finite() {
        return None;
    }
    let k = 1.0 / nenner;
    let na = (by * cx - bx * cy) * k;
    if !(0.0..=1.0).contains(&na) {
        return None;
    }
    let nb = (ax * cy - ay * cx) * k;
    if !(0.0..=1.0).contains(&nb) {
        return None;
    }
    Some([p1[0] + ax * na, p1[1] + ay * na])
}

impl Auswertung<'_> {
    /// Kontrollpunkte einer Kurve in Pixeln.
    pub fn kontrollen_px(&self, cid: &str) -> Result<Vec<[[f64; 2]; 4]>, GeomError> {
        Ok(self
            .kontrollen(cid)?
            .into_iter()
            .map(|k| {
                [
                    [self.zu_px(k[0].x), self.zu_px(k[0].y)],
                    [self.zu_px(k[1].x), self.zu_px(k[1].y)],
                    [self.zu_px(k[2].x), self.zu_px(k[2].y)],
                    [self.zu_px(k[3].x), self.zu_px(k[3].y)],
                ]
            })
            .collect())
    }

    /// Punkt auf einer Kurve im Abstand `verlangt` — wo Seamly2D ihn setzt.
    ///
    /// * der Formelwert zählt auf **vier** Nachkommastellen der Mustereinheit
    ///   (`QString::number(CheckFormula(...), 'f', 4).toDouble()`);
    /// * `direction` außer `forward` misst vom Kurvenende;
    /// * die Länge wird geklemmt, sodass der Schnitt mindestens 1 mm vor jedem
    ///   Ende liegt — bei einem Pfad erst auf den ganzen Pfad, dann noch
    ///   einmal auf das getroffene Segment.
    pub fn kurve_schneiden(
        &self,
        cid: &str,
        verlangt: f64,
        vorwaerts: bool,
    ) -> Result<Pt, GeomError> {
        let (a, _) = self.kurve_teilen(cid, verlangt, vorwaerts)?;
        Ok(*a.last().map(|k| &k[3]).expect("Teilkurve ohne Segment"))
    }

    /// Die Kurve `cid` am Schnitt von `cutSpline`/`cutSplinePath` geteilt:
    /// die beiden Teilkurven, die Seamly2D unter Punkt-id + 1 und + 2 anlegt
    /// (`vtoolcutspline.cpp`, `vtoolcutsplinepath.cpp: Create`). Die erste
    /// endet, die zweite beginnt am Schnittpunkt; bei einem Pfad behalten die
    /// Segmente vor und nach dem geschnittenen Segment ihre Kontrollpunkte.
    pub fn kurve_teilen(
        &self,
        cid: &str,
        verlangt: f64,
        vorwaerts: bool,
    ) -> Result<(Vec<[Pt; 4]>, Vec<[Pt; 4]>), GeomError> {
        let segmente = self.kontrollen_px(cid)?;
        if segmente.is_empty() {
            return Err(GeomError::Geometrie {
                kind: "Schnitt",
                grund: format!("Kurve {cid} ohne Segment lässt sich nicht schneiden"),
            });
        }
        let laengen: Vec<f64> = segmente.iter().map(|k| laenge_px(*k)).collect();
        let vier_stellen = vier_stellen(verlangt);
        // Die Gesamtlänge kommt in Pixeln aus der Zerlegung und geht wieder
        // in die Mustereinheit zurück — vor der Klemmung.
        let laenge = if vorwaerts {
            self.zu_px(vier_stellen)
        } else {
            self.zu_px(self.von_px(laengen.iter().sum()) - vier_stellen)
        };
        self.kurve_teilen_px(cid, &segmente, &laengen, laenge)
    }

    /// The curve `cid` split at `laenge` pixels from its start, with the
    /// 1 mm clamp of `CutSpline`/`CutSplinePath` but without the four-digit
    /// rounding of a formula (the caller decides about that).
    fn kurve_teilen_px(
        &self,
        cid: &str,
        segmente: &[[[f64; 2]; 4]],
        laengen: &[f64],
        laenge: f64,
    ) -> Result<(Vec<[Pt; 4]>, Vec<[Pt; 4]>), GeomError> {
        let treffer = if segmente.len() == 1 {
            cut_teile(segmente[0], laenge).map(|(kopf, schwanz)| (0, kopf, schwanz))
        } else {
            cut_pfad_teile(segmente, laengen, laenge)
        };
        let (i, kopf, schwanz) = treffer.ok_or_else(|| GeomError::Geometrie {
            kind: "Schnitt",
            grund: format!(
                "Kurve {cid} ist zu kurz zum Schneiden — Seamly2D setzt den Punkt \
                 auf den Nullpunkt der Zeichenfläche"
            ),
        })?;
        let einheit = |k: [[f64; 2]; 4]| k.map(|q| Pt::new(self.von_px(q[0]), self.von_px(q[1])));
        let mut erste: Vec<[Pt; 4]> = segmente[..i].iter().map(|k| einheit(*k)).collect();
        erste.push(einheit(kopf));
        let mut zweite = vec![einheit(schwanz)];
        zweite.extend(segmente[i + 1..].iter().map(|k| einheit(*k)));
        Ok((erste, zweite))
    }

    /// Der Streckenzug, als den Seamly2D die Kurve `cid` führt (Pixel).
    ///
    /// Quelle `getPoints()` der Kurvenklassen. Eine Spline ist die Zerlegung
    /// ihrer Bézier-Kurve (`vspline.cpp`), ein Pfad die seiner Segmente
    /// hintereinander mit dem Knoten dazwischen nur einmal
    /// (`vabstractcubicbezierpath.cpp`), ein Bogen seine 45°-Abschnitte
    /// (`varc.cpp`).
    ///
    /// Mit diesem Streckenzug — **nicht** mit der Kurve und nicht mit ihrer
    /// Zeichnung — schneidet Seamly2D Achsen und andere Kurven.
    pub fn seamly_zug(&self, cid: &str) -> Result<Vec<[f64; 2]>, GeomError> {
        if self.ist_bogen(cid) {
            return Ok(self.bogen_zug_d(&self.bogen_daten_id(cid)?));
        }
        let mut zug: Vec<[f64; 2]> = Vec::new();
        for k in self.kontrollen_px(cid)? {
            if !zug.is_empty() {
                zug.pop();
            }
            let mut pts = vec![k[0]];
            teile_auf_oeffentlich(
                k[0][0], k[0][1], k[1][0], k[1][1], k[2][0], k[2][1], k[3][0], k[3][1],
                0, &mut pts,
            );
            pts.push(k[3]);
            zug.extend(pts);
        }
        Ok(zug)
    }

    /// Schnittpunkt einer Achse mit einer Kurve, wo Seamly2D ihn setzt.
    ///
    /// Aus dem Quelltext (`vtoolcurveintersectaxis.cpp: Create, FindPoint`):
    ///
    /// * die Achse ist zuerst ein **Strahl** vom Basispunkt in Richtung
    ///   `angle`; nur wenn er nichts trifft, zählt der Gegenstrahl;
    /// * von mehreren Treffern gilt der, der dem Basispunkt am nächsten liegt —
    ///   bei gleichem Abstand der **spätere** auf der Kurve (`QMap` mit
    ///   `operator[]`, ein gleicher Schlüssel überschreibt);
    /// * getroffen wird nur, wenn der Schnitt auch auf dem Streckenzug liegt
    ///   (`BoundedIntersection` — die Enden zählen mit).
    ///
    /// An der gezeichneten Polylinie lagen die zehn Punkte des Strumpfs v20
    /// 0,011 bis 0,275 mm daneben; im Randfall-Muster lagen fünf von 26 auf dem
    /// falschen Treffer, bis 89,24 mm.
    pub fn achsenschnitt(&self, cid: &str, basis: Pt, winkel: f64) -> Result<Pt, GeomError> {
        let q = self.achsenschnitt_px(&self.seamly_zug(cid)?, cid, basis, winkel)?;
        Ok(Pt::new(self.von_px(q[0]), self.von_px(q[1])))
    }

    /// [`Self::achsenschnitt`] on the given polyline, in pixels.
    fn achsenschnitt_px(&self, zug: &[[f64; 2]], cid: &str, basis: Pt, winkel: f64) -> Result<[f64; 2], GeomError> {
        let b = [self.zu_px(basis.x), self.zu_px(basis.y)];
        let mut treffer: Vec<[f64; 2]> = Vec::new();
        for grad in [winkel, winkel + 180.0] {
            let w = grad.to_radians();
            // Der Winkel ist der mathematische Winkel im Pixelraum — und der
            // Pixelraum ist gegenüber der Datei **gespiegelt**: die Datei rechnet
            // y nach unten, der Pixelraum y nach oben (`_to_px` ohne Vorzeichen
            //wechsel auf einer y-nach-oben-Rechnung, hier `zu_px` ohne
            // Vorzeichenwechsel auf einer y-nach-unten-Rechnung — dieselbe
            // Spiegelung). Der Gegenstrahl muss deshalb mit `−sin` gebaut
            // werden. Mit `+sin` zeigte der Strahl in die Gegenrichtung:
            // an Bluse 2 v26 traf `AB2RC` bei 1,890 px Entfernung nichts,
            // stattdessen traf der Gegenstrahl bei 4,859 px — 12,97 mm daneben.
            let fern = [b[0] + RAY_PX * w.cos(), b[1] - RAY_PX * w.sin()];
            treffer = zug
                .windows(2)
                .filter_map(|w2| schnitt(b, fern, w2[0], w2[1]))
                .collect();
            if !treffer.is_empty() {
                break;
            }
        }
        if treffer.is_empty() {
            return Err(GeomError::Geometrie {
                kind: "Achsenschnitt",
                grund: format!(
                    "kein Schnitt Kurve/Achse bei Kurve {cid} — Seamly2D setzt den \
                     Punkt auf den Nullpunkt der Zeichenfläche"
                ),
            });
        }
        let abstand = |p: [f64; 2]| ((p[0] - b[0]).powi(2) + (p[1] - b[1]).powi(2)).sqrt();
        let naechster = *treffer
            .iter()
            .reduce(|a, c| if abstand(*c) <= abstand(*a) { c } else { a })
            .unwrap();
        Ok(naechster)
    }

    /// The curve a `curveIntersectAxis` point cuts, its polyline and the
    /// point on it, in pixels.
    fn achsen_daten<'p>(&self, p: &'p Point) -> Result<(&'p str, Vec<[f64; 2]>, [f64; 2]), GeomError> {
        let cid = p.curve.as_deref().filter(|s| !s.is_empty()).ok_or(GeomError::Geometrie {
            kind: "Achsenschnitt",
            grund: "@curve fehlt".into(),
        })?;
        let basis = self.bez_pt(p.base_point.as_deref(), "basePoint")?;
        let winkel = self.feld(&p.angle, "angle")?;
        let zug = self.seamly_zug(cid)?;
        let q = self.achsenschnitt_px(&zug, cid, basis, winkel)?;
        Ok((cid, zug, q))
    }

    /// Length in pixels at which Seamly2D cuts the curve of the
    /// `curveIntersectAxis` point `p` into its two sub-curves
    /// (`vtoolcurveintersectaxis.cpp: Create` → `GetLengthByPoint`), together
    /// with the curve id and the point itself.
    pub fn achsen_schnittlaenge<'p>(&self, p: &'p Point) -> Result<(&'p str, f64, [f64; 2]), GeomError> {
        let (cid, zug, q) = self.achsen_daten(p)?;
        let laenge = laenge_bis_punkt(&zug, q).ok_or_else(|| GeomError::Geometrie {
            kind: "Achsenschnitt",
            grund: format!(
                "Punkt {} liegt nicht auf dem Streckenzug von Kurve {cid} — Seamly2D legt leere Teilkurven an",
                p.id
            ),
        })?;
        Ok((cid, laenge, q))
    }

    /// The two sub-curves of a `curveIntersectAxis` point on a spline or a
    /// path, as Seamly2D registers them under point id + 1 and + 2
    /// (`vtoolcurveintersectaxis.cpp: InitSegments`):
    ///
    /// * the curve is cut at the length up to the point
    ///   ([`Self::achsen_schnittlaenge`]) with the regular 1 mm clamp, but
    ///   without rounding to four digits;
    /// * a **spline** keeps the inner control points of that cut, but its two
    ///   parts end and start at the **axis point** itself;
    /// * a **path** is split by `VToolCutSplinePath::CutSplinePath`, so its
    ///   parts meet at the cut point, which can sit up to 0.1 % of the length
    ///   beside the axis point.
    pub fn achsen_teilkurven(&self, p: &Point) -> Result<(Vec<[Pt; 4]>, Vec<[Pt; 4]>), GeomError> {
        let (cid, laenge, q) = self.achsen_schnittlaenge(p)?;
        let segmente = self.kontrollen_px(cid)?;
        if segmente.is_empty() {
            return Err(GeomError::Geometrie {
                kind: "Achsenschnitt",
                grund: format!("Kurve {cid} ohne Segment lässt sich nicht schneiden"),
            });
        }
        let laengen: Vec<f64> = segmente.iter().map(|k| laenge_px(*k)).collect();
        let (mut a, mut b) = self.kurve_teilen_px(cid, &segmente, &laengen, laenge)?;
        if !self.ist_pfad(cid) {
            let punkt = Pt::new(self.von_px(q[0]), self.von_px(q[1]));
            a[0][3] = punkt;
            b[0][0] = punkt;
        }
        Ok((a, b))
    }

    /// Schnittpunkt zweier Kreisbögen (`pointOfIntersectionArcs`), dort, wo
    /// Seamly2D ihn setzt (`vtoolpointofintersectionarcs.cpp: FindPoint`;
    /// `schnitt.py: _arcs_intersection`):
    ///
    /// * Kandidaten sind die **exakten** Schnittpunkte der beiden Kreise
    ///   ([`seamly_kreise`]), nicht die der Streckenzüge;
    /// * ein Kandidat gilt, wenn die Strecke von der Bogenmitte bis 10 px
    ///   über ihn hinaus den Streckenzug **beider** Bögen trifft;
    /// * gelten beide, wählt `crossPoint` 1 den ersten, sonst den zweiten
    ///   in der Reihenfolge der Kreisformel.
    ///
    /// Gerechnet wird in Seamly2Ds Lage (y nach unten) — die gespeicherten
    /// Werte sind schon so, anders als in `schnitt.py`.
    pub fn schnitt_boegen(&self, p: &Point) -> Result<Pt, GeomError> {
        let mut boegen = Vec::new();
        for (feld, kurz) in [(&p.first_arc, "firstArc"), (&p.second_arc, "secondArc")] {
            let cid = feld.as_deref().filter(|s| !s.is_empty()).ok_or(GeomError::Geometrie {
                kind: "pointOfIntersectionArcs",
                grund: format!("@{kurz} fehlt"),
            })?;
            let d = self.bogen_daten_id(cid)?;
            boegen.push(([self.zu_px(d.mitte.x), self.zu_px(d.mitte.y)], self.zu_px(d.radius), self.bogen_zug_d(&d)));
        }
        let (m1, r1, zug1) = &boegen[0];
        let (m2, r2, zug2) = &boegen[1];
        let auf_bogen = |m: [f64; 2], zug: &[[f64; 2]], q: [f64; 2]| {
            let a = ((q[0] - m[0]).powi(2) + (q[1] - m[1]).powi(2)).sqrt();
            let q = if a > 0.0 {
                let f = (a + 10.0) / a;
                [m[0] + (q[0] - m[0]) * f, m[1] + (q[1] - m[1]) * f]
            } else {
                q
            };
            zug.windows(2).any(|w| schnitt(m, q, w[0], w[1]).is_some())
        };
        let gueltig: Vec<[f64; 2]> = seamly_kreise(*m1, *r1, *m2, *r2)
            .into_iter()
            .filter(|&q| auf_bogen(*m1, zug1, q) && auf_bogen(*m2, zug2, q))
            .collect();
        let wahl = match gueltig.len() {
            0 => {
                return Err(GeomError::Geometrie {
                    kind: "pointOfIntersectionArcs",
                    grund: format!("kein Schnittpunkt der Bögen (Punkt {}) — Seamly2D setzt ihn auf den Nullpunkt", p.id),
                })
            }
            2 if auswahl(&p.cross_point)? != 1 => gueltig[1],
            _ => gueltig[0],
        };
        Ok(Pt::new(self.von_px(wahl[0]), self.von_px(wahl[1])))
    }

    /// Schnittpunkt zweier Kurven (`pointOfIntersectionCurves`), dort, wo
    /// Seamly2D ihn setzt (`vtoolpointofintersectioncurves.cpp: FindPoint`;
    /// `schnitt.py: _curves_intersection`):
    ///
    /// * geschnitten werden die Streckenzüge ([`Auswertung::seamly_zug`]),
    ///   jede Strecke von `curve1` mit jeder von `curve2`, der Punkt auf der
    ///   Strecke von `curve1` gerechnet;
    /// * von mehreren Treffern bleiben die obersten (`vCrossPoint` 1, in
    ///   Seamly2D das kleine y) oder die untersten, nur bei exakt gleicher
    ///   Höhe mehrere; unter denen der rechte (`hCrossPoint` 2) oder der
    ///   linke.
    pub fn schnitt_kurven(&self, p: &Point) -> Result<Pt, GeomError> {
        let mut zuege = Vec::new();
        for (feld, kurz) in [(&p.curve1, "curve1"), (&p.curve2, "curve2")] {
            let cid = feld.as_deref().filter(|s| !s.is_empty()).ok_or(GeomError::Geometrie {
                kind: "pointOfIntersectionCurves",
                grund: format!("@{kurz} fehlt"),
            })?;
            zuege.push(self.seamly_zug(cid)?);
        }
        let mut treffer: Vec<[f64; 2]> = Vec::new();
        for a in zuege[0].windows(2) {
            for b in zuege[1].windows(2) {
                if let Some(q) = schnitt(a[0], a[1], b[0], b[1]) {
                    treffer.push(q);
                }
            }
        }
        if treffer.is_empty() {
            return Err(GeomError::Geometrie {
                kind: "pointOfIntersectionCurves",
                grund: format!("kein Schnittpunkt der Kurven (Punkt {}) — Seamly2D setzt ihn auf den Nullpunkt", p.id),
            });
        }
        let oben = auswahl(&p.v_cross_point)? == 1;
        let grenze = treffer.iter().map(|q| q[1]).fold(if oben { f64::INFINITY } else { f64::NEG_INFINITY }, |g, y| {
            if oben { g.min(y) } else { g.max(y) }
        });
        let kandidaten: Vec<[f64; 2]> = treffer.into_iter().filter(|q| q[1] == grenze).collect();
        let rechts = auswahl(&p.h_cross_point)? == 2;
        let mut wahl = kandidaten[0];
        for q in &kandidaten[1..] {
            if (rechts && q[0] > wahl[0]) || (!rechts && q[0] < wahl[0]) {
                wahl = *q;
            }
        }
        Ok(Pt::new(self.von_px(wahl[0]), self.von_px(wahl[1])))
    }

    /// Die drei Kurven-Punkttypen.
    pub fn punkt_auf_kurve(&self, p: &Point) -> Result<Pt, GeomError> {
        match p.kind.as_str() {
            "cutSpline" | "cutSplinePath" => {
                let cid = p
                    .spline
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .or(p.spline_path.as_deref().filter(|s| !s.is_empty()))
                    .ok_or(GeomError::Geometrie {
                        kind: "Schnitt",
                        grund: "weder @spline noch @splinePath gesetzt".into(),
                    })?;
                let laenge = self.feld(&p.length, "length")?;
                let vorwaerts = p
                    .direction
                    .as_deref()
                    .map(|d| d == "forward")
                    .unwrap_or(true);
                self.kurve_schneiden(cid, laenge, vorwaerts)
            }
            "curveIntersectAxis" => {
                let cid = p
                    .curve
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or(GeomError::Geometrie {
                        kind: "Achsenschnitt",
                        grund: "@curve fehlt".into(),
                    })?;
                let basis = self.bez_pt(p.base_point.as_deref(), "basePoint")?;
                let winkel = self.feld(&p.angle, "angle")?;
                self.achsenschnitt(cid, basis, winkel)
            }
            andere => Err(GeomError::UnsupportedPointType(andere.to_string())),
        }
    }
}

/// `accuracyPointOnLine` in pixels: 0.1555 mm at 96 dpi.
const AUF_LINIE_PX: f64 = 0.1555 / 25.4 * 96.0;

/// `QPointF::toPoint()`: both coordinates rounded with `qRound`.
fn ganzzahlig(p: [f64; 2]) -> [i64; 2] {
    [(p[0] + 0.5).floor() as i64, (p[1] + 0.5).floor() as i64]
}

/// Does `t` lie on the segment `p1`–`p2`, the way Seamly2D asks it
/// (`vgobject.cpp: IsPointOnLineSegment`)? Inside the bounding box, each
/// side widened by the accuracy only where the box misses, and then at most
/// the accuracy away from the **line** (perp dot product against the area of
/// a perpendicular of that length).
fn auf_strecke_seamly(t: [f64; 2], p1: [f64; 2], p2: [f64; 2]) -> bool {
    for k in 0..2 {
        let drin = (p1[k] <= t[k] && t[k] <= p2[k]) || (p2[k] <= t[k] && t[k] <= p1[k]);
        if !drin && (p1[k] - t[k]).abs() > AUF_LINIE_PX && (p2[k] - t[k]).abs() > AUF_LINIE_PX {
            return false;
        }
    }
    let pdp = |a: [f64; 2], b: [f64; 2], t: [f64; 2]| (a[0] - t[0]) * (b[1] - t[1]) - (a[1] - t[1]) * (b[0] - t[0]);
    let p = pdp(p1, p2, t).abs();
    // `GetEpsilon`: the same product for a point at the accuracy distance
    // perpendicular to the segment, i.e. its length times the accuracy.
    let e = ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt() * AUF_LINIE_PX;
    let fast_gleich = |a: f64, b: f64| {
        if a.abs() <= 1e-12 {
            return b.abs() <= 1e-12;
        }
        if b.abs() <= 1e-12 {
            return false;
        }
        (a - b).abs() * 1e12 <= a.abs().min(b.abs())
    };
    p < e || fast_gleich(p, e)
}

/// Length along the polyline `zug` from its start to the point `p`, as
/// `VAbstractCurve::GetLengthByPoint` measures it (pixels):
///
/// * 0, when the start rounds to the same whole pixel as `p`;
/// * the whole length, when the end does (`ToEnd` keeps the polyline);
/// * otherwise up to the **last** segment that holds `p` (`ToEnd` searches
///   from the end) plus the piece of that segment up to `p`;
/// * `None` (Seamly2D: −1) when no segment holds it.
pub fn laenge_bis_punkt(zug: &[[f64; 2]], p: [f64; 2]) -> Option<f64> {
    if zug.len() < 2 {
        return None;
    }
    let abstand = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    if ganzzahlig(zug[0]) == ganzzahlig(p) {
        return Some(0.0);
    }
    let bis = |j: usize| zug[..=j].windows(2).map(|w| abstand(w[0], w[1])).sum::<f64>();
    let n = zug.len();
    if ganzzahlig(zug[n - 1]) == ganzzahlig(p) {
        return Some(bis(n - 1));
    }
    let j = (0..n - 1).rev().find(|&j| auf_strecke_seamly(p, zug[j + 1], zug[j]))?;
    Some(bis(j) + if p == zug[j] { 0.0 } else { abstand(zug[j], p) })
}

/// Eine Schnittlänge, wie Seamly2D sie nimmt: `QString::number(x, 'f', 4)
/// .toDouble()` — auf vier Stellen, korrekt gerundet aus dem Binärwert.
/// `(x · 1e4).round() / 1e4` rundet an der Grenze falsch: 3,46555 (im
/// Binärwert knapp darunter) wird mal 1e4 genau 34655,5 und damit 3,4656
/// statt 3,4655 (Bolero v37, `KVP_L`, 0,001 mm daneben).
pub fn vier_stellen(x: f64) -> f64 {
    format!("{x:.4}").parse().unwrap_or(x)
}

/// Wert von `crossPoint`, `vCrossPoint`, `hCrossPoint`; fehlt er, gilt 1
/// (`vpattern.cpp: GetParametrUInt(…, "1")`).
fn auswahl(roh: &Option<String>) -> Result<u32, GeomError> {
    match roh.as_deref().map(str::trim) {
        None | Some("") => Ok(1),
        Some(w) => w.parse().map_err(|_| GeomError::Geometrie {
            kind: "Schnittpunkt",
            grund: format!("Auswahl des Schnittpunkts ist keine Zahl: {w:?}"),
        }),
    }
}

/// Schnittpunkte zweier Kreise in Seamly2Ds Reihenfolge, in seiner Lage
/// (y nach unten), wörtlich nach `vgobject.cpp: IntersectionCircles` (wie
/// `schnitt.py: _seamly_circles`). Leer, wenn die Kreise sich nicht
/// schneiden oder gleich sind; ein Punkt, wenn sie sich berühren.
pub fn seamly_kreise(c1: [f64; 2], r1: f64, c2: [f64; 2], r2: f64) -> Vec<[f64; 2]> {
    // `VFuzzyComparePossibleNulls`
    let fast_gleich = |p: f64, q: f64| {
        if p.abs() <= 1e-12 {
            return q.abs() <= 1e-12;
        }
        if q.abs() <= 1e-12 {
            return false;
        }
        (p - q).abs() * 1e12 <= p.abs().min(q.abs())
    };
    if fast_gleich(c1[0], c2[0]) && fast_gleich(c1[1], c2[1]) && fast_gleich(r1, r2) {
        return Vec::new();
    }
    let a = -2.0 * (c2[0] - c1[0]);
    let b = -2.0 * (c2[1] - c1[1]);
    let c = (c2[0] - c1[0]) * (c2[0] - c1[0]) + (c2[1] - c1[1]) * (c2[1] - c1[1]) + r1 * r1 - r2 * r2;
    let n = a * a + b * b;
    let (x0, y0) = (-a * c / n, -b * c / n);
    if c * c > r1 * r1 * n {
        return Vec::new();
    }
    if fast_gleich(c * c, r1 * r1 * n) {
        return vec![[x0 + c1[0], y0 + c1[1]]];
    }
    let d = r1 * r1 - c * c / n;
    let mult = (d / n).sqrt();
    vec![[x0 + b * mult + c1[0], y0 - a * mult + c1[1]], [x0 - b * mult + c1[0], y0 + a * mult + c1[1]]]
}
