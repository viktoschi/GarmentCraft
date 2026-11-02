//! Operationen und abgeleitete Kurven.
//!
//! Seamly2D kennt Werkzeuge, die vorhandene Objekte verschieben, drehen oder
//! spiegeln (`<operation>`: `moving`, `rotation`, `flippingByAxis`,
//! `flippingByLine`). Sie erzeugen **neue** Objekte: das i-te Objekt der
//! `<source>`-Liste ergibt das i-te der `<destination>`-Liste, unter dessen
//! id und mit dem Namen der Quelle plus `suffix` (`S0BH` → `S0BHR`). Spätere
//! Punkte, Kurven und Teile verweisen auf diese ids und Namen. Gerechnet wird
//! wie in `schnitt.py` (`apply_operations`, `_register_destinations`, nach
//! Seamly2Ds `vpointf.cpp`, `vspline.cpp`, `varc.cpp`: Rotate, Flip, Move):
//!
//! * ein Punkt ist das Abbild seiner Quelle;
//! * eine Spline oder ein Pfad ist das Abbild der **Kontrollpunkte** der
//!   Quelle, Segment für Segment (die kopierten Griffwinkel taugen nicht,
//!   sie bleiben beim Spiegeln stehen);
//! * ein Bogen bildet Mitte und beide Enden ab und bestimmt die Winkel neu;
//!   eine Spiegelung kehrt seine Laufrichtung um. Seine Mitte heißt in den
//!   Formelnamen (`Arc_<Mitte>_<id>`) wie die Mitte der Quelle.
//!
//! Anders als `schnitt.py`, das jeden benannten Punkt unter jedem Suffix
//! verdoppelt, gibt es hier nur die Kopien, die die Operation wirklich nennt —
//! so wie in Seamly2D.
//!
//! Dazu die **Teilbögen** eines `cutArc`: Seamly2D legt beim Schneiden eines
//! Bogens unter Punkt-id + 1 und + 2 die beiden Teile an
//! (`vtoolcutarc.cpp: Create`), die Teile und Formeln wie Bögen nennen.

use crate::geom::{vektor, winkel_von, Auswertung, GeomError, Pt};
use crate::boegen::Bogendaten;
use crate::model::Operation;

impl Auswertung<'_> {
    /// Die Operation und die Quelle, aus der das Objekt `id` entsteht.
    pub fn kopie(&self, id: &str) -> Option<(&Operation, &str)> {
        self.kopie_von.get(id).map(|(i, q)| (self.operationen[*i], q.as_str()))
    }

    /// Die erste Quelle einer (auch verketteten) Kopie.
    pub fn urquelle<'s>(&'s self, mut id: &'s str) -> &'s str {
        let mut schritte = 0;
        while let Some((_, q)) = self.kopie(id) {
            id = q;
            schritte += 1;
            if schritte > self.operationen.len() {
                break; // Zyklus in der Datei: lieber anhalten als hängen
            }
        }
        id
    }

    /// Formelwert eines optionalen Operationsfelds; fehlt es, gilt 0.
    fn op_zahl(&self, feld: &Option<String>, kurz: &'static str) -> Result<f64, GeomError> {
        match feld {
            Some(f) if !f.trim().is_empty() => self.feld(feld, kurz),
            _ => Ok(0.0),
        }
    }

    /// Bild des Punktes `p` unter der Operation `op`.
    pub fn abbilden(&self, op: &Operation, p: Pt) -> Result<Pt, GeomError> {
        match op.kind.as_str() {
            "moving" => {
                let (dx, dy) = vektor(self.op_zahl(&op.angle, "angle")?, self.op_zahl(&op.length, "length")?);
                Ok(Pt::new(p.x + dx, p.y + dy))
            }
            "rotation" => {
                // `QLineF::setAngle`: gegen den Uhrzeiger, wie er am
                // Bildschirm erscheint — in gespeicherten Werten (y nach
                // unten) also die Drehung um −Winkel.
                let c = self.ref_pt(op.center.as_deref().unwrap_or(""))?;
                let w = self.op_zahl(&op.angle, "angle")?.to_radians();
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                Ok(Pt::new(c.x + dx * w.cos() + dy * w.sin(), c.y - dx * w.sin() + dy * w.cos()))
            }
            "flippingByAxis" => {
                let c = self.ref_pt(op.center.as_deref().unwrap_or(""))?;
                // `axisType` 1 (Vorgabe): senkrechte Achse durch die Mitte.
                if op.axis_type.as_deref().map(str::trim).unwrap_or("1") == "1" {
                    Ok(Pt::new(2.0 * c.x - p.x, p.y))
                } else {
                    Ok(Pt::new(p.x, 2.0 * c.y - p.y))
                }
            }
            "flippingByLine" => {
                let a = self.ref_pt(op.p1_line.as_deref().unwrap_or(""))?;
                let b = self.ref_pt(op.p2_line.as_deref().unwrap_or(""))?;
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let n = dx * dx + dy * dy;
                if n == 0.0 {
                    return Err(GeomError::Geometrie { kind: "flippingByLine", grund: "Spiegelgerade der Länge 0".into() });
                }
                let f = ((p.x - a.x) * dx + (p.y - a.y) * dy) / n;
                let fuss = Pt::new(a.x + f * dx, a.y + f * dy);
                Ok(Pt::new(2.0 * fuss.x - p.x, 2.0 * fuss.y - p.y))
            }
            andere => Err(GeomError::UnsupportedPointType(format!("operation:{andere}"))),
        }
    }

    /// Kontrollpunkte `[p0, p1, p2, p3]` je Segment der Kurve `cid` — einer
    /// Spline, eines Pfads, ihrer Operationskopie oder einer Teilkurve.
    pub fn kontrollen(&self, cid: &str) -> Result<Vec<[Pt; 4]>, GeomError> {
        if let Some(s) = self.spline(cid) {
            return self.spline_kontrollen(s);
        }
        if let Some((p, teil)) = self.teilkurve_von(cid) {
            let (a, b) = if p.kind == "curveIntersectAxis" {
                self.achsen_teilkurven(p)?
            } else {
                let quelle = geschnittene_kurve(p)
                    .ok_or(GeomError::Geometrie { kind: "Schnitt", grund: "weder @spline noch @splinePath gesetzt".into() })?;
                let laenge = self.feld(&p.length, "length")?;
                let vorwaerts = p.direction.as_deref().map(|d| d == "forward").unwrap_or(true);
                self.kurve_teilen(quelle, laenge, vorwaerts)?
            };
            return Ok(if teil == 1 { a } else { b });
        }
        if let Some((op, q)) = self.kopie(cid) {
            let mut segmente = self.kontrollen(q)?;
            for k in &mut segmente {
                for p in k.iter_mut() {
                    *p = self.abbilden(op, *p)?;
                }
            }
            return Ok(segmente);
        }
        Err(GeomError::UnknownPoint(format!("Spline {cid}")))
    }

    /// Ist `cid` eine Spline, ein Pfad, eine Teilkurve oder eine Kopie davon?
    pub fn ist_spline(&self, cid: &str) -> bool {
        let u = self.urquelle(cid);
        self.spline(u).is_some() || self.teilkurve_von(u).is_some()
    }

    /// Ist `cid` ein Pfad (`pathInteractive`, `cubicBezierPath`), ein Teil
    /// eines geschnittenen Pfads oder eine Kopie davon?
    pub fn ist_pfad(&self, cid: &str) -> bool {
        let u = self.urquelle(cid);
        if let Some((p, _)) = self.teilkurve_von(u) {
            return match p.kind.as_str() {
                "curveIntersectAxis" => geschnittene_kurve(p).is_some_and(|k| self.ist_pfad(k)),
                andere => andere == "cutSplinePath",
            };
        }
        self.spline(u)
            .is_some_and(|s| matches!(s.kind.as_deref(), Some("pathInteractive") | Some("cubicBezierPath")))
    }

    /// Der `cutSpline`/`cutSplinePath`-Punkt, dessen Teilkurve `teil` (1 oder
    /// 2) `id` ist: Punkt-id + `teil`, wenn die Datei diese id frei lässt,
    /// oder der Schlüssel `teil:<Punkt-id>:<teil>` ([`Self::abgeleitete_kurven`]).
    ///
    /// Seamly2D führt die Teilkurven für Formeln über ihren Namen
    /// (`Spl_<Anfang>_<Schnitt>`), die ids + 1 und + 2 reserviert es nur.
    /// Von Hand oder per Skript erzeugte Dateien vergeben sie mitunter an
    /// andere Objekte (Bolero v36/v37: Punkte KF4B und KF5A); deshalb gehen
    /// die Namen nicht über die id.
    pub fn teilkurve_von(&self, id: &str) -> Option<(&crate::model::Point, u8)> {
        if let Some(rest) = id.strip_prefix("teil:") {
            let (pid, teil) = rest.rsplit_once(':')?;
            let teil: u8 = teil.parse().ok().filter(|t| (1..=2).contains(t))?;
            let p = *self.punkte.get(pid)?;
            return self.teilt_spline(p).then_some((p, teil));
        }
        if self.splines_enthaelt(id) {
            return None;
        }
        let n: u64 = id.parse().ok()?;
        (1..=2u8).find_map(|teil| {
            let pid = n.checked_sub(teil as u64)?.to_string();
            let p = *self.punkte.get(&pid)?;
            self.teilt_spline(p).then_some((p, teil))
        })
    }

    /// Does the point `p` split a spline or a path into two sub-curves?
    /// `cutSpline` and `cutSplinePath` always, `curveIntersectAxis` when its
    /// curve is one (on an arc it makes sub-arcs, [`Self::teilbogen_von`]).
    fn teilt_spline(&self, p: &crate::model::Point) -> bool {
        match p.kind.as_str() {
            "cutSpline" | "cutSplinePath" => true,
            "curveIntersectAxis" => geschnittene_kurve(p).is_some_and(|k| self.ist_spline(k)),
            _ => false,
        }
    }

    fn splines_enthaelt(&self, id: &str) -> bool {
        self.spline(id).is_some() || self.bogen(id).is_some() || self.linie(id).is_some() || self.punkte.contains_key(id)
    }

    /// Alle abgeleiteten Kurven: Kurvenkopien aus Operationen (unter ihrer
    /// Ziel-id) und die Teilkurven geschnittener Splines und Pfade (unter
    /// `teil:<Punkt-id>:1|2`).
    pub fn abgeleitete_kurven(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.kopie_von.keys().filter(|z| self.ist_spline(z)).cloned().collect();
        for (pid, p) in &self.punkte {
            if self.teilt_spline(p) {
                ids.extend([format!("teil:{pid}:1"), format!("teil:{pid}:2")]);
            }
        }
        ids.sort();
        ids
    }

    /// Ist `id` ein Bogen — der Berechnung, eine Operationskopie oder ein
    /// Teilbogen eines `cutArc`?
    pub fn ist_bogen(&self, id: &str) -> bool {
        self.bogen(self.urquelle(id)).is_some() || self.teilbogen_von(id).is_some()
    }

    /// Bogendaten zu einer id: Bogen der Berechnung, Operationskopie oder
    /// Teilbogen.
    pub fn bogen_daten_id(&self, id: &str) -> Result<Bogendaten, GeomError> {
        if let Some(b) = self.bogen(id) {
            return self.bogen_daten(b);
        }
        if let Some((op, q)) = self.kopie(id) {
            let d = self.bogen_daten_id(q)?;
            let ende = |w: f64| {
                let (dx, dy) = vektor(w, d.radius);
                Pt::new(d.mitte.x + dx, d.mitte.y + dy)
            };
            let mitte = self.abbilden(op, d.mitte)?;
            let p1 = self.abbilden(op, ende(d.f1))?;
            let p2 = self.abbilden(op, ende(d.f2))?;
            // `QLineF(m, p).angle()`: 0 bis unter 360 Grad.
            let winkel = |p: Pt| {
                let w = winkel_von(mitte, p).rem_euclid(360.0);
                if (w - 360.0).abs() * 1e12 <= w.abs().min(360.0) { 0.0 } else { w }
            };
            let spiegel = matches!(op.kind.as_str(), "flippingByAxis" | "flippingByLine");
            return Ok(Bogendaten {
                mitte,
                mitte_id: d.mitte_id,
                radius: d.radius,
                f1: winkel(p1),
                f2: winkel(p2),
                flipped: d.flipped != spiegel,
            });
        }
        if let Some((punkt, teil)) = self.teilbogen_von(id) {
            return self.teilbogen(punkt, teil);
        }
        Err(GeomError::UnknownPoint(format!("Bogen {id}")))
    }

    /// Der `cutArc`- oder `curveIntersectAxis`-Punkt (auf einem Bogen), dessen
    /// Teilbogen `teil` (1 oder 2) die id `id` trägt (Punkt-id + `teil`).
    pub fn teilbogen_von(&self, id: &str) -> Option<(&crate::model::Point, u8)> {
        let n: u64 = id.parse().ok()?;
        (1..=2u8).find_map(|teil| {
            let pid = n.checked_sub(teil as u64)?.to_string();
            let p = *self.punkte.get(&pid)?;
            let bogen = match p.kind.as_str() {
                "cutArc" => true,
                // Only where the file leaves the id free: a spline or a point
                // under it must not turn into a sub-arc.
                "curveIntersectAxis" => {
                    !self.splines_enthaelt(id) && geschnittene_kurve(p).is_some_and(|k| k != id && self.ist_bogen(k))
                }
                _ => false,
            };
            bogen.then_some((p, teil))
        })
    }

    /// Radius, Winkel und Laufrichtung eines Teilbogens (`varc.cpp: CutArc`):
    /// der erste läuft vom Startwinkel bis zum Schnitt, der zweite vom
    /// Schnitt bis zum Endwinkel; beide behalten Mitte, Radius und
    /// Laufrichtung. Der Schnittwinkel ist der von `QLineF`, 0 bis unter 360.
    ///
    /// A `curveIntersectAxis` on an arc cuts it at the length up to the
    /// axis point (`GetLengthByPoint`), with the same clamp as `cutArc` but
    /// without the four-digit rounding (`vtoolcurveintersectaxis.cpp:
    /// InitArc`).
    pub fn teilbogen(&self, p: &crate::model::Point, teil: u8) -> Result<Bogendaten, GeomError> {
        let (d, w) = if p.kind == "curveIntersectAxis" {
            let (cid, laenge, _) = self.achsen_schnittlaenge(p)?;
            let d = self.bogen_daten_id(cid)?;
            let radius = self.zu_px(d.radius);
            let mut voll = radius * Self::seamly_bogen_winkel(d.f1, d.f2, d.flipped).to_radians();
            if d.flipped {
                voll = -voll;
            }
            let w = Self::seamly_cut_arc(d.f1, radius, voll, d.flipped, laenge).ok_or_else(|| GeomError::Geometrie {
                kind: "curveIntersectAxis",
                grund: format!("Bogen {cid} ist zu kurz zum Schneiden"),
            })?;
            (d, w)
        } else {
            self.bogen_schnittwinkel(p)?
        };
        let w = Self::qline_winkel(w);
        Ok(if teil == 1 { Bogendaten { f2: w, ..d } } else { Bogendaten { f1: w, ..d } })
    }

    /// Namen der beiden Endpunkte einer Kurve, wie Seamly2D sie in `Spl_`
    /// und `SplPath_` nennt; für eine Kopie die Namen der Quelle mit Suffix.
    pub fn kurven_enden(&self, cid: &str) -> Option<(String, String)> {
        if let Some((op, q)) = self.kopie(cid) {
            let (a, b) = self.kurven_enden(q)?;
            return Some((format!("{a}{}", op.suffix), format!("{b}{}", op.suffix)));
        }
        if let Some((p, teil)) = self.teilkurve_von(cid) {
            let (a, b) = self.kurven_enden(geschnittene_kurve(p)?)?;
            let schnitt = p.name.clone()?;
            return Some(if teil == 1 { (a, schnitt) } else { (schnitt, b) });
        }
        let s = self.spline(cid)?;
        let (a, b) = if matches!(s.kind.as_deref(), Some("pathInteractive") | Some("cubicBezierPath")) {
            let kinder: Vec<&str> = s.path_points.iter().filter_map(|k| k.p_spline.as_deref()).collect();
            (*kinder.first()?, *kinder.last()?)
        } else {
            (s.point1.as_deref()?, s.point4.as_deref()?)
        };
        Some((self.punkt_name(a)?.to_string(), self.punkt_name(b)?.to_string()))
    }
}

/// The curve a cutting point splits: `@spline` or `@splinePath` of a
/// `cutSpline`/`cutSplinePath`, `@curve` of a `curveIntersectAxis`.
fn geschnittene_kurve(p: &crate::model::Point) -> Option<&str> {
    fn feld(f: &Option<String>) -> Option<&str> {
        f.as_deref().filter(|s| !s.is_empty())
    }
    match p.kind.as_str() {
        "curveIntersectAxis" => feld(&p.curve),
        _ => feld(&p.spline).or(feld(&p.spline_path)),
    }
}
