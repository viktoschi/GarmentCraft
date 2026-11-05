//! Kollisionskörper.
//!
//! Als Aufzählung statt Trait-Objekt, damit eine GPU-Fassung dieselben Daten
//! als Puffer bekommt (ADR-018). Jeder Körper hat eine Geschwindigkeit; für
//! die Reibung zählt die Bewegung des Stoffs **relativ** zum Körper — das
//! braucht Stufe 7 (Bewegung). Das Körpernetz aus `klon3d koerper` ist die
//! Variante [`Koerper::Netz`] ([`crate::netzkoerper`]); es wird geteilt
//! gehalten, damit die Rechenwege den Körper je Bild billig kopieren.

use std::sync::Arc;

use crate::netzkoerper::Koerpernetz;

/// Ein Kollisionskörper (SI-Einheiten).
#[derive(Debug, Clone)]
pub enum Koerper {
    /// Unendlich langer Zylinder: Punkt auf der Achse, Richtung (wird
    /// normiert), Radius, Geschwindigkeit des ganzen Körpers.
    Zylinder { punkt: [f64; 3], achse: [f64; 3], radius: f64, geschwindigkeit: [f64; 3] },
    /// Starres Dreiecksnetz, um `versatz` verschoben, mit der Geschwindigkeit
    /// des ganzen Körpers. Ein sich verformender Körper (Stufe 7) trägt
    /// stattdessen ein Feld in [`Koerpernetz::punktgeschwindigkeit`]; dann
    /// zählt je Kontakt die Geschwindigkeit an seiner Stelle, und
    /// `geschwindigkeit` ist nur der Rückfall für ein Netz ohne Feld.
    Netz { netz: Arc<Koerpernetz>, versatz: [f64; 3], geschwindigkeit: [f64; 3] },
}

/// Ergebnis eines Kontakts: Normale (nach außen), Eindringtiefe (> 0) und
/// Geschwindigkeit des Körpers **an der Stelle**.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kontakt {
    pub normale: [f64; 3],
    pub tiefe: f64,
    pub geschwindigkeit: [f64; 3],
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(crate) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

impl Koerper {
    /// Senkrechter Zylinder (Achse z) durch den Ursprung, ruhend.
    pub fn zylinder_z(radius: f64) -> Koerper {
        Koerper::Zylinder { punkt: [0.0; 3], achse: [0.0, 0.0, 1.0], radius, geschwindigkeit: [0.0; 3] }
    }

    /// Das Körpernetz an seiner Lage, ruhend.
    pub fn netz(netz: Koerpernetz) -> Koerper {
        Koerper::Netz { netz: Arc::new(netz), versatz: [0.0; 3], geschwindigkeit: [0.0; 3] }
    }

    /// Kontakt, wenn `x` näher als `abstand` an der Oberfläche oder innen
    /// liegt. Beim Netz nur bis zur Suchtiefe innen ([`crate::netzkoerper`]).
    pub fn kontakt(&self, x: [f64; 3], abstand: f64) -> Option<Kontakt> {
        match self {
            &Koerper::Zylinder { punkt, achse, radius, geschwindigkeit } => {
                let a = mul(achse, 1.0 / norm(achse));
                let d = sub(x, punkt);
                let quer = sub(d, mul(a, dot(d, a)));
                let r = norm(quer);
                let tiefe = radius + abstand - r;
                if tiefe <= 0.0 || r < 1e-12 {
                    return None;
                }
                Some(Kontakt { normale: mul(quer, 1.0 / r), tiefe, geschwindigkeit })
            }
            Koerper::Netz { netz, versatz, geschwindigkeit } => {
                let n = netz.naechster(sub(x, *versatz), netz.suchtiefe.max(abstand))?;
                let tiefe = abstand - n.abstand;
                // Die Geschwindigkeit gehört an den Trefferpunkt, nicht an den
                // Körper: am Oberarm ist die Haut langsam, an der Hand schnell.
                let vk = netz.geschwindigkeit_an(n.dreieck as usize, n.punkt, *geschwindigkeit);
                (tiefe > 0.0).then_some(Kontakt { normale: n.normale, tiefe, geschwindigkeit: vk })
            }
        }
    }

    /// Erster Durchstoß des Wegs `von` → `nach` durch die Oberfläche von außen:
    /// Punkt, Normale und Geschwindigkeit des Körpers **am Treffpunkt**. Nur
    /// beim Netz; den Zylinder findet [`Self::kontakt`] in jeder Tiefe. Liegt
    /// ein Geschwindigkeitsfeld vor ([`Koerpernetz::punktgeschwindigkeit`]),
    /// zählt dort die Stelle; sonst gilt der Körper als ruhend in seiner Lage
    /// am Schrittende.
    pub fn durchstoss(&self, von: [f64; 3], nach: [f64; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
        match self {
            Koerper::Zylinder { .. } => None,
            Koerper::Netz { netz, versatz, geschwindigkeit } => {
                let (p, n, vk) = netz.durchstoss(sub(von, *versatz), sub(nach, *versatz), *geschwindigkeit)?;
                Some((add(p, *versatz), n, vk))
            }
        }
    }

    /// Körperpunkte gegen Stoffdreiecke (nur beim Netz,
    /// [`Koerpernetz::punkte_gegen_dreiecke`]); liefert die Zahl der
    /// Korrekturen.
    #[allow(clippy::too_many_arguments)]
    pub fn punkte_gegen_dreiecke(&self, tief: f64, dreiecke: &[[u32; 3]], w: &[f64], x: &mut [[f64; 3]], v: &mut [[f64; 3]]) -> usize {
        match self {
            Koerper::Zylinder { .. } => 0,
            Koerper::Netz { netz, versatz, geschwindigkeit } => netz.punkte_gegen_dreiecke(tief, *versatz, *geschwindigkeit, dreiecke, w, x, v),
        }
    }

    /// Der Körper nach der Zeit `t` (Lage + Geschwindigkeit · t) — aus der
    /// Lage am Bildanfang gesetzt, nicht aufsummiert (siehe [`crate::cpu`]).
    pub fn verschoben(&self, t: f64) -> Koerper {
        match self {
            &Koerper::Zylinder { punkt, achse, radius, geschwindigkeit } => {
                Koerper::Zylinder { punkt: add(punkt, mul(geschwindigkeit, t)), achse, radius, geschwindigkeit }
            }
            Koerper::Netz { netz, versatz, geschwindigkeit } => Koerper::Netz {
                netz: Arc::clone(netz),
                versatz: add(*versatz, mul(*geschwindigkeit, t)),
                geschwindigkeit: *geschwindigkeit,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zylinder_kontakt() {
        let z = Koerper::zylinder_z(0.1);
        assert!(z.kontakt([0.2, 0.0, 5.0], 0.001).is_none());
        let k = z.kontakt([0.0, 0.05, -3.0], 0.001).unwrap();
        assert!((k.tiefe - 0.051).abs() < 1e-12);
        assert!((k.normale[1] - 1.0).abs() < 1e-12);
    }
}
