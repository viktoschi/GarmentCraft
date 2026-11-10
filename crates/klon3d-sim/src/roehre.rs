//! Prüffall „Röhre“ (Stufe 4, Schritt 4a).
//!
//! Ein Rechteck wird an seinen Längsseiten zur Röhre vernäht und um einen
//! senkrechten Zylinder gelegt. Die Breite ist so gewählt, dass die Röhre
//! ohne Reduktion genau passt: B = 2π·(R + t/2) — die Mittelfläche des Stoffs
//! liegt eine halbe Dicke über der Haut. Mit Reduktion r ist die Ruhelänge
//! B·(1 − r), die Röhre muss sich auf B dehnen:
//!
//! ```text
//! ε_soll = B / (B·(1 − r)) − 1 = 1/(1 − r) − 1        (5,5 % → 5,82 %, 10 % → 11,11 %)
//! ```
//!
//! Anfangslage: gleichmäßig auf einen 5 mm größeren Zylinder gelegt, die Naht
//! schon geschlossen — wie ein genähtes Rohr, das übergezogen wird. Die
//! **Nahtprobe** (`anfang_mit_spalt`) rollt das Rechteck dagegen ohne
//! Streckung auf; an der Naht bleibt ein Spalt von 2π·5 mm ≈ 31 mm, den die
//! Naht-Constraints schließen müssen.
//!
//! **Rutschprobe** (mit Schwerkraft): Der Ringzug K·ε je Ruhehöhe H·(1 − r)
//! drückt die Röhre mit N = 2π·K·ε·H·(1 − r) auf den Zylinder; sie wiegt
//! M·g = ρ_A·B·H·(1 − r)²·g (Zuschnittfläche). Coulomb hält sie, solange
//! μ·N ≥ M·g, also q = μ·K·ε / (ρ_A·g·R·(1 − r)) ≥ 1; sonst rutscht sie mit
//! a = g − μ·N/M. Verglichen wird mit der Normallast, die im Ruhezustand
//! tatsächlich anliegt (Federzug gegen die Zylinderwand), damit die Probe den
//! Solver prüft und nicht die Näherung K·ε. „Ein unterversorgtes Teil muss
//! herunterrutschen“ (`MATERIAL.md`).

use std::collections::HashMap;
use std::f64::consts::PI;

use klon3d_mesh::{vernetze, Musternetz, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pattern::read_str;

use crate::cpu::Rechenweg;
use crate::koerper::{norm, Koerper};
use crate::material::{Material, RADICAL_RUBBER_04};
use crate::messung;
use crate::netzkoerper::{zylinder, Koerpernetz, SUCHTIEFE};
use crate::szene::{Simparameter, Szene, Teilvorgabe, WEICH_FAKTOR};

/// Nahtliste der Röhre: rechte Kante B→C gegen linke Kante A→D.
pub const NAHTLISTE: &str = "R1 | Röhre | Rohr: B C v | Rohr: A D r\n";

/// Die Einstellungen des Prüffalls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Roehre {
    pub zylinder_radius_m: f64,
    pub hoehe_m: f64,
    /// Anfangslage: so viel größer als die Sollage aufgerollt.
    pub anfangsabstand_m: f64,
    /// Zielkantenlänge des Netzes (mm).
    pub kante_mm: f64,
    pub material: Material,
    /// Reduktion als Anteil.
    pub reduktion: f64,
    /// Nahtprobe: mit offenem Nahtspalt beginnen.
    pub anfang_mit_spalt: bool,
    /// Den Zylinder als Dreiecksnetz geben ([`ZYLINDER_NETZ`]) statt
    /// analytisch — die Probe für den Körper als Netz.
    pub zylinder_als_netz: bool,
}

/// Der Zylinder als Netz: Segmente je Ring (Sehnenabweichung bei 90 mm Radius
/// 3,4 µm), Ringabstand (m), Unter- und Oberkante (m) — lang genug für die
/// Rutschprobe (frei fallend 1,23 m in 0,5 s).
pub const ZYLINDER_NETZ: (usize, f64, f64, f64) = (360, 0.02, -2.0, 0.2);

impl Default for Roehre {
    fn default() -> Self {
        Roehre {
            zylinder_radius_m: 0.09,
            hoehe_m: 0.20,
            anfangsabstand_m: 0.005,
            kante_mm: 10.0,
            material: RADICAL_RUBBER_04,
            reduktion: 0.055,
            anfang_mit_spalt: false,
            zylinder_als_netz: false,
        }
    }
}

/// Ergebnis eines Laufs bis zur Ruhe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lauf {
    pub bilder: usize,
    pub ruhig: bool,
    /// Mittlere Geschwindigkeit am Ende (m/s).
    pub v_mittel: f64,
}

/// Messwerte der Röhre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ergebnis {
    /// Umfang in halber Höhe (m).
    pub umfang_m: f64,
    /// Umfangsdehnung gegen die reduzierte Ruhelänge.
    pub dehnung: f64,
    pub dehnung_soll: f64,
    /// Größte Hauptdehnung der Dreiecke: Median, 95-%-Wert, Höchstwert.
    pub dehnung_median: f64,
    pub dehnung_p95: f64,
    pub dehnung_max: f64,
    /// Größter Nahtspalt (m).
    pub nahtspalt_m: f64,
    /// Größter Abstand eines Punkts von der Sollfläche R + t/2, nach außen
    /// (m) — „liegt an“.
    pub abheben_m: f64,
    pub punkte: usize,
    pub dreiecke: usize,
}

impl Roehre {
    /// Radius der Mittelfläche, wenn die Röhre anliegt.
    pub fn mittelradius_m(&self) -> f64 {
        self.zylinder_radius_m + self.material.dicke_m / 2.0
    }

    /// Breite des Rechtecks: passt ohne Reduktion genau.
    pub fn breite_m(&self) -> f64 {
        2.0 * PI * self.mittelradius_m()
    }

    /// ε_soll = 1/(1 − r) − 1.
    pub fn dehnung_soll(&self) -> f64 {
        1.0 / (1.0 - self.reduktion) - 1.0
    }

    /// Coulomb-Reibkraft durch Gewicht, analytisch (≥ 1: hält):
    /// q = μ·K·ε / (ρ_A·g·R·(1 − r)).
    pub fn halteverhaeltnis(&self, g: f64) -> f64 {
        let druck = self.material.dehnung_n_m * self.dehnung_soll() / self.mittelradius_m();
        self.material.reibung * druck / (self.material.flaechengewicht_kg_m2 * g * (1.0 - self.reduktion))
    }

    /// Das Rechteck als Seamly2D-Muster (Einheit cm).
    pub fn muster(&self) -> String {
        rechteck_muster(self.breite_m(), self.hoehe_m)
    }

    /// Der Kollisionskörper: der Zylinder, analytisch oder als Netz.
    pub fn koerper(&self) -> Result<Koerper, String> {
        if !self.zylinder_als_netz {
            return Ok(Koerper::zylinder_z(self.zylinder_radius_m));
        }
        let (seg, ring_m, z_unten, z_oben) = ZYLINDER_NETZ;
        let ringe = ((z_oben - z_unten) / ring_m).round() as usize;
        let (p, d) = zylinder(self.zylinder_radius_m, z_unten, z_oben, seg, ringe);
        Ok(Koerper::netz(Koerpernetz::neu(p, d, SUCHTIEFE)?))
    }
}

/// Ein Rechteck B × H (m) als Seamly2D-Muster (Einheit cm) mit den Punkten
/// A B C D.
pub fn rechteck_muster(breite_m: f64, hoehe_m: f64) -> String {
    let (b, h) = (breite_m * 100.0, hoehe_m * 100.0);
    format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
  <version>0.7.4</version>
  <unit>cm</unit>
  <description>klon3d Prüffall Röhre: Rechteck {b:.4} × {h:.4} cm, Längsseiten vernäht</description>
  <draftBlock name="Roehre">
    <calculation>
      <point id="1" name="A" type="single" x="0" y="0" mx="0" my="0"/>
      <point id="2" name="B" type="single" x="{b}" y="0" mx="0" my="0"/>
      <point id="3" name="C" type="single" x="{b}" y="{h}" mx="0" my="0"/>
      <point id="4" name="D" type="single" x="0" y="{h}" mx="0" my="0"/>
    </calculation>
    <modeling>
      <point id="11" idObject="1" type="modeling"/>
      <point id="12" idObject="2" type="modeling"/>
      <point id="13" idObject="3" type="modeling"/>
      <point id="14" idObject="4" type="modeling"/>
    </modeling>
    <pieces>
      <piece id="20" name="Rohr" seamAllowance="true" width="0.3">
        <nodes>
          <node idObject="11" type="NodePoint"/>
          <node idObject="12" type="NodePoint"/>
          <node idObject="13" type="NodePoint"/>
          <node idObject="14" type="NodePoint"/>
        </nodes>
      </piece>
    </pieces>
  </draftBlock>
</pattern>
"#
    )
}

/// Das Rechteck B × H (m), an den Längsseiten zur Röhre vernäht
/// ([`NAHTLISTE`]), als Schnittnetz mit Kanten von etwa `kante_mm`.
pub fn rohr_netz(breite_m: f64, hoehe_m: f64, kante_mm: f64) -> Result<Musternetz, String> {
    let m = read_str(&rechteck_muster(breite_m, hoehe_m)).map_err(|e| e.to_string())?;
    let ev = Auswertung::new(&m, HashMap::new());
    let defs = lies_nahtliste(NAHTLISTE).map_err(|e| e.to_string())?;
    let bericht = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).map_err(|e| e.to_string())?;
    let p = Parameter { kante_mm, ..Parameter::default() };
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &bericht.naehte, &p).map_err(|e| e.to_string())?;
    if !netz.fehler.is_empty() {
        return Err(netz.fehler.join("; "));
    }
    Ok(netz)
}

impl Roehre {
    /// Szene in der Anfangslage, Zylinder als einziger Körper. Die Zahl der
    /// Unterschritte wird mindestens so hoch gesetzt, dass die
    /// Weichheitsbedingung gilt ([`Szene::unterschritte_weich`]).
    pub fn szene(&self, param: Simparameter) -> Result<Szene, String> {
        let mut s = self.szene_roh(param)?;
        s.param.unterschritte = s.param.unterschritte.max(s.unterschritte_weich(WEICH_FAKTOR));
        Ok(s)
    }

    /// Szene mit genau den Unterschritten aus `param` (auch im steifen Bereich).
    pub fn szene_roh(&self, param: Simparameter) -> Result<Szene, String> {
        let netz = rohr_netz(self.breite_m(), self.hoehe_m, self.kante_mm)?;
        let r0 = self.mittelradius_m() + self.anfangsabstand_m;
        // Winkel je Meter Breite: mit Spalt Bogenlänge, sonst ganz herum.
        let je_m = if self.anfang_mit_spalt { 1.0 / r0 } else { 2.0 * PI / self.breite_m() };
        let lage = |_: usize, q: [f64; 2]| {
            let (u, y) = (q[0] * 1e-3, q[1] * 1e-3);
            let w = u * je_m;
            [r0 * w.cos(), r0 * w.sin(), -y]
        };
        let vg = Teilvorgabe { material: self.material, reduktion: self.reduktion };
        Ok(Szene::aus_netz(
            &netz,
            &|_| vg,
            &lage,
            vec![self.koerper()?],
            param,
        ))
    }

    /// Messwerte am aktuellen Zustand.
    pub fn auswerten(&self, s: &Szene) -> Ergebnis {
        let umfang = messung::umfang_in_ebene(s, [0.0, 0.0, -self.hoehe_m / 2.0], [0.0, 0.0, 1.0]);
        let ruhe = self.breite_m() * (1.0 - self.reduktion);
        let mut dd: Vec<f64> = messung::dehnung_je_dreieck(s).iter().map(|d| d.0).collect();
        dd.sort_by(f64::total_cmp);
        let q = |a: f64| dd[((dd.len() - 1) as f64 * a).round() as usize];
        let rm = self.mittelradius_m();
        let abheben = s.x.iter().map(|p| norm([p[0], p[1], 0.0]) - rm).fold(f64::MIN, f64::max);
        Ergebnis {
            umfang_m: umfang,
            dehnung: umfang / ruhe - 1.0,
            dehnung_soll: self.dehnung_soll(),
            dehnung_median: q(0.5),
            dehnung_p95: q(0.95),
            dehnung_max: q(1.0),
            nahtspalt_m: messung::nahtspalt(s),
            abheben_m: abheben,
            punkte: s.x.len(),
            dreiecke: s.dreiecke.len(),
        }
    }
}

/// Rechnet, bis die mittlere Geschwindigkeit `bilder_ruhig` Bilder lang unter
/// `v_grenze` liegt, höchstens `max_bilder`.
pub fn bis_zur_ruhe(
    s: &mut Szene,
    rw: &mut dyn Rechenweg,
    max_bilder: usize,
    v_grenze: f64,
    bilder_ruhig: usize,
) -> Lauf {
    let mut ruhig_seit = 0;
    let mut v = f64::INFINITY;
    for b in 1..=max_bilder {
        rw.bild(s);
        v = messung::mittlere_geschwindigkeit(s);
        if !v.is_finite() {
            return Lauf { bilder: b, ruhig: false, v_mittel: v };
        }
        ruhig_seit = if v < v_grenze { ruhig_seit + 1 } else { 0 };
        if ruhig_seit >= bilder_ruhig {
            return Lauf { bilder: b, ruhig: true, v_mittel: v };
        }
    }
    Lauf { bilder: max_bilder, ruhig: false, v_mittel: v }
}

/// Parameter für den Ruhelauf ohne Schwerkraft.
pub fn param_ruhe() -> Simparameter {
    Simparameter { schwerkraft: [0.0; 3], daempfung: 5.0, ..Simparameter::default() }
}

/// Ergebnis der Rutschprobe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rutschprobe {
    /// Coulomb-Reibkraft durch Gewicht, analytisch (≥ 1: hält).
    pub halteverhaeltnis: f64,
    /// Dasselbe mit der Normallast, die im Ruhezustand anliegt.
    pub halteverhaeltnis_gemessen: f64,
    /// Weg des Schwerpunkts nach unten (m) in `sekunden`.
    pub weg_m: f64,
    /// Erwarteter Weg ½·(g − μN/M)·t² mit der gemessenen Normallast (0, wenn
    /// sie hält).
    pub weg_soll_m: f64,
    pub sekunden: f64,
}

/// Summe der Normallasten gegen die Körper (N): je Punkt im Kontakt der
/// Federzug k·max(0, L − L₀) der anliegenden Kanten, Anteil gegen die
/// Kontaktnormale — dieselbe Last, mit der die Reibung rechnet.
///
/// Distance constraints carried by a membrane are skipped: the fabric's pull
/// there comes from the triangle ([`crate::kontinuum`]), not from the spring
/// whose `k` the solver ignores.
pub fn normallast(s: &Szene) -> f64 {
    let mut f = vec![[0.0f64; 3]; s.x.len()];
    for c in 0..s.abstand.len() {
        if s.membran_kante.get(c).copied().unwrap_or(false) {
            continue;
        }
        let [i, j] = s.abstand.punkte[c].map(|p| p as usize);
        let d = crate::koerper::sub(s.x[j], s.x[i]);
        let l = norm(d);
        let zug = (l - s.abstand.ruhe[c]).max(0.0) / s.abstand.nachgiebigkeit[c];
        if l > 1e-12 && zug > 0.0 {
            let fij = crate::koerper::mul(d, zug / l);
            f[i] = crate::koerper::add(f[i], fij);
            f[j] = crate::koerper::sub(f[j], fij);
        }
    }
    for m in &s.membran_elemente {
        let idx = m.punkte.map(|p| p as usize);
        let p = [s.x[idx[0]], s.x[idx[1]], s.x[idx[2]]];
        let fz = m.element.krafte_zug(p);
        for k in 0..3 {
            f[idx[k]] = crate::koerper::add(f[idx[k]], fz[k]);
        }
    }
    let mut summe = 0.0;
    for i in 0..s.x.len() {
        for k in &s.koerper {
            // kleine Reserve: anliegend, auch wenn genau auf der Oberfläche
            if let Some(kt) = k.kontakt(s.x[i], s.kollisionsabstand[i] + 1e-6) {
                summe += (-crate::koerper::dot(f[i], kt.normale)).max(0.0);
            }
        }
    }
    summe
}

impl Roehre {
    /// Erst ohne Schwerkraft zur Ruhe bringen (die Ringspannung baut sich
    /// auf), dann `sekunden` mit Schwerkraft und ohne Dämpfung rechnen.
    pub fn rutschprobe(&self, rw: &mut dyn Rechenweg, sekunden: f64) -> Result<Rutschprobe, String> {
        let mut s = self.szene(param_ruhe())?;
        bis_zur_ruhe(&mut s, rw, 1200, 1e-4, 10);
        let g = 9.81;
        s.param.schwerkraft = [0.0, 0.0, -g];
        s.param.daempfung = 0.0;
        for v in s.v.iter_mut() {
            *v = [0.0; 3];
        }
        let z0 = messung::schwerpunkt(&s)[2];
        let m = s.gesamtmasse();
        let q_mess = self.material.reibung * normallast(&s) / (m * g);
        let bilder = (sekunden / s.param.bild_dt).round() as usize;
        for _ in 0..bilder {
            rw.bild(&mut s);
        }
        let weg = z0 - messung::schwerpunkt(&s)[2];
        let t = bilder as f64 * s.param.bild_dt;
        let weg_soll = if q_mess >= 1.0 { 0.0 } else { 0.5 * g * (1.0 - q_mess) * t * t };
        Ok(Rutschprobe {
            halteverhaeltnis: self.halteverhaeltnis(g),
            halteverhaeltnis_gemessen: q_mess,
            weg_m: weg,
            weg_soll_m: weg_soll,
            sekunden: t,
        })
    }
}
