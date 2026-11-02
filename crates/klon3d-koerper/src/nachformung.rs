//! Post-shaping between waist and hip, klon3d's own step after the chain.
//!
//! The reference chain ([`crate::kette_referenz`]) hits the circumferences it
//! knows at the places it knows them, and keeps its statistical shape in
//! between. Between waist and hip that can be far off: on the user's
//! measurements (waist 70 at 98 cm, high hip 83 at 93 cm, hip 90 at 77 cm)
//! the chain's body sits its waist at 99.6 cm, needs 24.3 cm from waist to
//! hip instead of 21, and measures 76.7 cm at 93 cm. The chain only knows
//! the high hip about 12 cm below the waist; given at 5 cm, its statistical
//! model does not get there. This step therefore goes **beyond the
//! reference program**, and says so wherever it reports.
//!
//! Two smooth, local deformations of the torso and legs (arms and head stay
//! as they are):
//!
//! 1. **Heights.** A monotone cubic (Fritsch–Carlson) remap of `y` in a band
//!    from below the hip to above the waist sends the chain's waist and hip
//!    heights to `height_waist_side` and `height_hip`; identity at both ends
//!    of the band, so nothing outside moves.
//! 2. **Circumferences.** Every section from just below the hip to above the
//!    waist is scaled horizontally about its centre towards a target profile:
//!    a monotone curve through `waist_circ`, `highhip_circ` and `hip_circ` at
//!    their heights, **flat at the hip** (the fullest place, not overtaken on
//!    the way up) and **flat at the waist** (the narrowest place, no crease);
//!    above the waist a section keeps its own tape measure but may not fall
//!    below the waist; below the hip and towards the band top the factor
//!    fades to 1. The factors are found by a few rounds of measuring and
//!    correcting, since a section's hull also takes points from its
//!    neighbours' edges.
//!
//! Only points labelled torso or leg **and** lying near the torso section move.
//! A few points carry the torso label but belong to the hanging arm; moved
//! with the torso, they crossed the arm.
//!
//! Heights count from the lowest point of the body, as the measurement file
//! does from the floor. Uniform around the section: the file's front/back
//! split (high hip 44/41) is not built.

use crate::massband::{huelle, segment, umfangsschnitt, Messdaten, WAAGERECHT};
use crate::Punkt3;

/// Target heights (cm above the floor) and circumferences (cm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ziele {
    pub taille: (f64, f64),
    pub huefte_oben: (f64, f64),
    pub huefte: (f64, f64),
}

impl Ziele {
    /// From a measurement file: `height_waist_side`/`waist_circ`,
    /// `height_highhip` (or `height_waist_side − waist_to_highhip_side`)/
    /// `highhip_circ`, `height_hip`/`hip_circ`. `None` if any is missing or
    /// they are not in order (hip below high hip below waist).
    pub fn aus_smis(wert: &dyn Fn(&str) -> Option<f64>) -> Option<Ziele> {
        let hw = wert("height_waist_side")?;
        let hh_oben =
            wert("height_highhip").or_else(|| Some(hw - wert("waist_to_highhip_side")?))?;
        let z = Ziele {
            taille: (hw, wert("waist_circ")?),
            huefte_oben: (hh_oben, wert("highhip_circ")?),
            huefte: (wert("height_hip")?, wert("hip_circ")?),
        };
        (z.huefte.0 < z.huefte_oben.0 && z.huefte_oben.0 < z.taille.0).then_some(z)
    }
}

/// One line of the report: height (cm), before, after, target (cm).
#[derive(Clone, Debug)]
pub struct Zeile {
    pub was: &'static str,
    pub hoehe: f64,
    pub vorher: f64,
    pub nachher: f64,
    pub soll: f64,
}

/// What the step did.
#[derive(Clone, Debug)]
pub struct Bericht {
    /// Waist and hip height of the chain's body (cm) and the targets.
    pub taille_hoehe: (f64, f64),
    pub huefte_hoehe: (f64, f64),
    /// The three circumferences at their target heights.
    pub umfaenge: Vec<Zeile>,
    /// Height of the narrowest torso section between hip and band top after
    /// the step (cm).
    pub schmalste: f64,
    /// Bands of the two deformations (cm).
    pub band_hoehen: (f64, f64),
    pub band_umfang: (f64, f64),
    pub runden: usize,
}

impl Bericht {
    pub fn zeilen(&self) -> Vec<String> {
        let mut z = vec![format!(
            "Nachformung (über CLO hinaus): Taille {:.2} → {:.2} cm, Hüfte {:.2} → {:.2} cm hoch",
            self.taille_hoehe.0, self.taille_hoehe.1, self.huefte_hoehe.0, self.huefte_hoehe.1
        )];
        for r in &self.umfaenge {
            z.push(format!(
                "  {:<10} auf {:6.2} cm: {:7.3} → {:7.3}  Soll {:7.3}",
                r.was, r.hoehe, r.vorher, r.nachher, r.soll
            ));
        }
        z.push(format!(
            "  schmalste Stelle {:.2} cm; Bänder {:.1}–{:.1} / {:.1}–{:.1} cm; {} Runden",
            self.schmalste,
            self.band_hoehen.0,
            self.band_hoehen.1,
            self.band_umfang.0,
            self.band_umfang.1,
            self.runden
        ));
        z
    }
}

/// Monotone cubic interpolation (Fritsch–Carlson) through `(x, y)` knots
/// with increasing `x`; constant outside.
#[derive(Clone, Debug)]
pub struct Pchip {
    x: Vec<f64>,
    y: Vec<f64>,
    m: Vec<f64>,
}

impl Pchip {
    pub fn neu(knoten: &[(f64, f64)]) -> Pchip {
        let n = knoten.len();
        let x: Vec<f64> = knoten.iter().map(|k| k.0).collect();
        let y: Vec<f64> = knoten.iter().map(|k| k.1).collect();
        let d: Vec<f64> = (0..n - 1)
            .map(|i| (y[i + 1] - y[i]) / (x[i + 1] - x[i]))
            .collect();
        let mut m = vec![0.0; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            m[i] = if d[i - 1] * d[i] <= 0.0 {
                0.0
            } else {
                (d[i - 1] + d[i]) / 2.0
            };
        }
        for i in 0..n - 1 {
            if d[i] == 0.0 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
                continue;
            }
            let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                m[i] = t * a * d[i];
                m[i + 1] = t * b * d[i];
            }
        }
        Pchip { x, y, m }
    }

    pub fn wert(&self, t: f64) -> f64 {
        let n = self.x.len();
        if t <= self.x[0] {
            return self.y[0];
        }
        if t >= self.x[n - 1] {
            return self.y[n - 1];
        }
        let i = (0..n - 1).find(|&i| t <= self.x[i + 1]).unwrap();
        let h = self.x[i + 1] - self.x[i];
        let s = (t - self.x[i]) / h;
        let (h00, h10, h01, h11) = (
            2.0 * s * s * s - 3.0 * s * s + 1.0,
            s * s * s - 2.0 * s * s + s,
            -2.0 * s * s * s + 3.0 * s * s,
            s * s * s - s * s,
        );
        h00 * self.y[i] + h10 * h * self.m[i] + h01 * self.y[i + 1] + h11 * h * self.m[i + 1]
    }
}

/// Points that move: torso and legs, not arms, head or "outside".
fn beweglich(d: &Messdaten, i: usize) -> bool {
    matches!(
        d.segment.get(i).copied(),
        Some(segment::RUMPF) | Some(segment::BEIN_LINKS) | Some(segment::BEIN_RECHTS)
    )
}

/// Tape measure of the torso section at height `y` (cm, absolute), its centre
/// in x and z, and the largest horizontal distance of the section from that
/// centre.
fn schnitt(v: &[Punkt3], d: &Messdaten, y: f64) -> Option<(f64, f64, f64, f64)> {
    let s = umfangsschnitt(v, d, [0.0, y, 0.0], WAAGERECHT, segment::RUMPF);
    if s.kette.len() < 3 {
        return None;
    }
    let (_, laenge) = huelle(&s.kette, WAAGERECHT);
    let n = s.kette.len() as f64;
    let cx = s.kette.iter().map(|p| p[0]).sum::<f64>() / n;
    let cz = s.kette.iter().map(|p| p[2]).sum::<f64>() / n;
    let r = s
        .kette
        .iter()
        .map(|p| ((p[0] - cx).powi(2) + (p[2] - cz).powi(2)).sqrt())
        .fold(0.0, f64::max);
    Some((laenge, cx, cz, r))
}

/// Applies the post-shaping to `punkte` (mm, y up) in place.
///
/// `chain_heights` = (waist, hip, underbust) height of the chain's body in cm
/// above its lowest point (`HEIGHT_Waist`, `HEIGHT_LowHip`,
/// `HEIGHT_UnderBust`). The upper end of both bands lies halfway between
/// waist and underbust, so bust and underbust do not move.
pub fn nachformen(
    punkte: &mut [Punkt3],
    d: &Messdaten,
    chain_heights: (f64, f64, f64),
    z: &Ziele,
) -> Result<Bericht, String> {
    let n = punkte.len();
    if d.segment.len() < n {
        return Err(format!("{} Segmente für {n} Punkte", d.segment.len()));
    }
    let boden = punkte.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) / 10.0;
    let (w0, h0, ub) = (chain_heights.0, chain_heights.1, chain_heights.2);
    let (hw, hh_oben, hh) = (z.taille.0, z.huefte_oben.0, z.huefte.0);
    let oben = (w0.max(hw) + 0.5 * (ub - w0.max(hw))).max(w0.max(hw) + 3.0);
    let unten = h0.min(hh) - 6.0;
    if !(unten < h0.min(hh) && hh < hh_oben && hh_oben < hw && hw < oben && oben < ub) {
        return Err(format!(
            "Höhen nicht in Ordnung: Band {unten:.1}–{oben:.1}, Hüfte {hh:.1}, Hochhüfte {hh_oben:.1}, Taille {hw:.1}, Unterbrust {ub:.1}"
        ));
    }

    let cm = |p: &[Punkt3]| -> Vec<Punkt3> {
        p.iter()
            .map(|q| [q[0] / 10.0, q[1] / 10.0 - boden, q[2] / 10.0])
            .collect()
    };
    let vorher_cm = cm(punkte);
    let ziel_umf = [
        ("Taille", hw, z.taille.1),
        ("Hochhüfte", hh_oben, z.huefte_oben.1),
        ("Hüfte", hh, z.huefte.1),
    ];
    let mut vorher = Vec::new();
    for (was, h, _) in ziel_umf {
        vorher.push(
            schnitt(&vorher_cm, d, h)
                .ok_or_else(|| format!("{was}: kein Rumpfschnitt auf {h:.1} cm"))?
                .0,
        );
    }

    // Which points move: torso and legs within the band, and only those near
    // the torso section. A few points carry the torso label but lie at the
    // hanging arm (x ≈ 33 cm at 89 cm on the user's body); moved with the
    // torso they crossed the arm.
    let mut schnitte: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut y = unten;
    while y <= oben + 1e-9 {
        if let Some((_, cx, cz, r)) = schnitt(&vorher_cm, d, y) {
            schnitte.push((y, cx, cz, r));
        }
        y += 0.5;
    }
    let bewegt: Vec<bool> = (0..n)
        .map(|i| {
            let p = vorher_cm[i];
            if !(beweglich(d, i) && p[1] > unten && p[1] < oben) || schnitte.is_empty() {
                return false;
            }
            let s = schnitte
                .iter()
                .min_by(|a, b| (a.0 - p[1]).abs().total_cmp(&(b.0 - p[1]).abs()))
                .unwrap();
            ((p[0] - s.1).powi(2) + (p[2] - s.2).powi(2)).sqrt() <= s.3 + 2.0
        })
        .collect();

    // 1. heights
    let hoehe = Pchip::neu(&[(unten, unten), (h0, hh), (w0, hw), (oben, oben)]);
    let mut v = vorher_cm.clone();
    for (i, p) in v.iter_mut().enumerate() {
        if bewegt[i] {
            p[1] = hoehe.wert(p[1]);
        }
    }

    // 2. circumferences. A target profile between hip and waist through the
    // three measurements, flat at the hip (the fullest place) and at the
    // waist (the narrowest): the hip is not overtaken on the way up, and the
    // waist has no crease (with a corner there, 33 points sank 3-5 mm below
    // their ring).
    // Above the waist the section keeps its own size but may not fall below
    // the waist; below the hip and towards the band top the factor fades to
    // 1. Sample heights every 0.5 cm, refined by measuring.
    let unten_u = (hh - 4.0).max(unten + 0.5);
    let profil = Pchip::neu(&[
        (hh - 2.0, z.huefte.1),
        (hh, z.huefte.1),
        (hh_oben, z.huefte_oben.1),
        (hw, z.taille.1),
        (hw + 1.0, z.taille.1),
    ]);
    let oben_halb = hw + 0.5 * (oben - hw);
    let glatt = |t: f64| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let mut proben: Vec<f64> = Vec::new();
    let mut y = unten_u;
    while y <= oben + 1e-9 {
        proben.push(y);
        y += 0.5;
    }
    let mut mitten = Vec::with_capacity(proben.len());
    let mut roh = Vec::with_capacity(proben.len());
    for &y in &proben {
        let (u, cx, cz, _) =
            schnitt(&v, d, y).ok_or_else(|| format!("kein Rumpfschnitt auf {y:.1} cm"))?;
        mitten.push((cx, cz));
        roh.push(u);
    }
    let soll_bei = |i: usize| -> Option<f64> {
        let y = proben[i];
        if y < hh {
            None
        } else if y <= hw {
            Some(profil.wert(y))
        } else if y <= oben_halb {
            Some(roh[i].max(z.taille.1 + 0.1 * (y - hw).powi(2)))
        } else {
            None
        }
    };
    let mut faktor: Vec<f64> = (0..proben.len())
        .map(|i| soll_bei(i).map_or(1.0, |s| s / roh[i]))
        .collect();
    let mut ergebnis = v.clone();
    let mut runden = 0;
    for runde in 0..30 {
        runden = runde + 1;
        // fade below the hip and above the upper half
        let i_huefte = proben.iter().position(|&y| y >= hh).unwrap_or(0);
        let i_halb = proben
            .iter()
            .rposition(|&y| y <= oben_halb)
            .unwrap_or(proben.len() - 1);
        let (f_huefte, f_halb) = (faktor[i_huefte], faktor[i_halb]);
        for (i, &y) in proben.iter().enumerate() {
            if y < hh {
                faktor[i] = 1.0 + (f_huefte - 1.0) * glatt((y - unten_u) / (hh - unten_u));
            } else if y > oben_halb {
                faktor[i] =
                    1.0 + (f_halb - 1.0) * (1.0 - glatt((y - oben_halb) / (oben - oben_halb)));
            }
        }
        let s = Pchip::neu(
            &proben
                .iter()
                .copied()
                .zip(faktor.iter().copied())
                .collect::<Vec<_>>(),
        );
        let mitte = |y: f64| -> (f64, f64) {
            let i = proben
                .partition_point(|&q| q < y)
                .clamp(1, proben.len() - 1);
            let t = ((y - proben[i - 1]) / (proben[i] - proben[i - 1])).clamp(0.0, 1.0);
            (
                mitten[i - 1].0 + t * (mitten[i].0 - mitten[i - 1].0),
                mitten[i - 1].1 + t * (mitten[i].1 - mitten[i - 1].1),
            )
        };
        ergebnis = v.clone();
        for (i, p) in ergebnis.iter_mut().enumerate() {
            if bewegt[i] && p[1] > unten_u && p[1] < oben {
                let f = s.wert(p[1]);
                let (cx, cz) = mitte(p[1]);
                p[0] = cx + f * (p[0] - cx);
                p[2] = cz + f * (p[2] - cz);
            }
        }
        let mut groesste = 0.0f64;
        for i in 0..proben.len() {
            if let Some(soll) = soll_bei(i) {
                let ist = schnitt(&ergebnis, d, proben[i]).map_or(soll, |m| m.0);
                groesste = groesste.max((ist - soll).abs());
                faktor[i] *= soll / ist;
            }
        }
        if groesste < 0.02 {
            break;
        }
    }

    let mut umfaenge = Vec::new();
    for (k, (was, h, soll)) in ziel_umf.iter().enumerate() {
        let nachher = schnitt(&ergebnis, d, *h).map_or(f64::NAN, |m| m.0);
        umfaenge.push(Zeile {
            was,
            hoehe: *h,
            vorher: vorher[k],
            nachher,
            soll: *soll,
        });
    }
    // narrowest torso section between hip and band top, every 0.25 cm
    let mut schmalste = (f64::INFINITY, 0.0);
    let mut y = hh;
    while y <= oben {
        if let Some((u, _, _, _)) = schnitt(&ergebnis, d, y) {
            if u < schmalste.0 {
                schmalste = (u, y);
            }
        }
        y += 0.25;
    }

    // Only the points in the band are written back; the rest keep their
    // exact values.
    for (i, (p, q)) in punkte.iter_mut().zip(&ergebnis).enumerate() {
        if bewegt[i] {
            *p = [q[0] * 10.0, (q[1] + boden) * 10.0, q[2] * 10.0];
        }
    }
    Ok(Bericht {
        taille_hoehe: (w0, hw),
        huefte_hoehe: (h0, hh),
        umfaenge,
        schmalste: schmalste.1,
        band_hoehen: (unten, oben),
        band_umfang: (unten_u, oben),
        runden,
    })
}
