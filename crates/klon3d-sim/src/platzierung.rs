//! Platzierung (Schritt 4c): Schnittteile vor dem Nähen um den Körper legen.
//!
//! CLO legt Teile auf Anordnungspunkte an Hüllkörpern um Rumpf, Arme und
//! Beine (Befund 71, Nachtrag); klon3d hat einen eigenen, einfachen Weg:
//!
//! * **Hüllkörper** aus dem Körper ([`Koerperteile`]): ein senkrechtes Prisma
//!   um den Rumpf (konvexe Hülle der Rumpfpunkte über die Höhe der Teile), je
//!   ein Zylinder um die Oberarme (Achse aus den Armpunkten zwischen Schulter
//!   und Ellbogen) und ein senkrechter Zylinder um den Hals (Halslandmarken),
//!   alle um einen Abstand nach außen versetzt (Lagedatei, je Teil
//!   änderbar). Um den Arm liegt wie um den Rumpf ein Prisma, hier längs der
//!   Armachse: die konvexe Hülle der Armpunkte über die Länge des Teils. Ein
//!   Ärmel, der nur halb um den Arm reicht, schließt seine Naht nicht — die
//!   Nahtfedern ziehen die Kanten quer durch den Arm, und der hält sie fest
//!   (an der Bluse3 v2 blieb die Ärmelnaht 86 mm offen, so breit wie der Arm,
//!   mit einem Zylinder vom Radius der Schulterkappe; ein Kreis vom größten
//!   Abstand zur Achse überschätzt den Umfang des Oberarms um ein Viertel).
//!   Eine Hülle je Zentimeter schmiegt sich enger an, ist aber nicht
//!   abwickelbar: Der Ärmel begann dort mit bis zu 20 % Dehnung (Befund 96).
//! * **Lageangaben** je Teil ([`lies_lage`], Datei `<muster>.lage` neben dem
//!   Muster wie die Nahtliste): Zone, Bezugspunkt, Umfangslage, Höhe und ob das
//!   Teil gespiegelt oder gedreht wird. Der Bezugspunkt darf ein Punktknoten
//!   eines anderen Teils sein: Teile aus demselben Konstruktionsblock mit
//!   demselben Bezugspunkt liegen zueinander wie im Muster, gemeinsame
//!   Nahtpunkte also schon aufeinander. `klebt <Teil>` bindet dieses Teil an
//!   das genannte statt es zu nähen ([`crate::klebung`]). `auf <Träger>` bzw.
//!   `auf <Träger>:<Punkt>` legt einen Flicken ohne eigene Bogenlage auf ein
//!   anderes Teil: Zone, Umfangslage, Höhe, Prisma, Drehung und Abstand kommen
//!   von dort, nur der eigene Musterpunkt bleibt. Damit lassen sich Auflagen und
//!   Scheiben platzieren, die im Schnitt weit neben dem Kleidungsstück liegen
//!   (Aufgabe #102: 12 aufgeklebte Teile der Bluse3 v2).
//! * **Abwicklung:** Jedes Teil wird längentreu auf seinen Hüllkörper gelegt —
//!   Prisma und Zylinder sind abwickelbar. Musterrichtung y (nach unten) wird
//!   die Höhe bzw. die Armachse, Musterrichtung x der Umfang.
//! * **Weite:** Sind die Rumpfteile in einer Höhe zusammen breiter als das
//!   Prisma (ausgestellte Rockbahnen), weitet sich die Hülle dort um die Mitte
//!   des Rumpfs, längentreu in der Breite, je 2 cm geglättet und nach unten
//!   nie abnehmend ([`Platzierung::weite_bei`]). Auf dem senkrechten Prisma
//!   überlappten die Bahnen des Rocks v36, und die Saumecken rutschten
//!   zwischen die Beine (Befund 100); geweitet sind seine Seitennähte bis zum
//!   Saum zu (Befund 102). Oberteile, die enger sind als der Körper, bleiben
//!   auf dem Prisma.
//!
//! **Welche Seite außen liegt.** Ein Teil liegt, wie gezeichnet, „von außen
//! gesehen“: Muster-x zeigt nach rechts für den, der von außen auf die Stelle
//! schaut, Muster-y nach unten. Am Rumpf ist das gegen den Uhrzeigersinn von
//! oben gesehen (vorn nach links, hinten nach rechts). Ist ein Teil anders
//! gezeichnet — bei der Bluse3 v2 die Rückenteile und die Ärmel —, steht in
//! der Lageangabe `gespiegelt`.
//!
//! **Winkel.** Umfang am Rumpf und am Hals in Grad, von oben gesehen: 0 =
//! vorn, 90 = links, 180 = hinten, 270 = rechts (Körperseiten der Trägerin).
//! Am Arm: 0 = oben, 90 = vorn, 180 = unten, 270 = hinten. Dazu ein Versatz
//! in cm Bogenlänge auf dem Hüllkörper, in Richtung wachsender Winkel.
//!
//! Szenenrahmen wie [`crate::netzkoerper`]: m, z oben, vorn −y, links +x.

use klon3d_mesh::{Musternetz, Nahtnetz};

use crate::cpu::Rechenweg;
use crate::implizit::ImplizitCg;
use crate::koerper::{add, cross, dot, mul, norm, sub, Koerper};
use crate::szene::Szene;

/// Vorgabe des Abstands der Hüllkörper zur Haut (m).
pub const ABSTAND: f64 = 0.02;

// ---------------------------------------------------------------------------
// Ebene Geometrie
// ---------------------------------------------------------------------------

/// Konvexe Hülle gegen den Uhrzeigersinn (Andrew, monotone Kette).
pub fn huelle(mut p: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let kreuz = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut h: Vec<[f64; 2]> = Vec::new();
    for durchgang in 0..2 {
        let start = h.len();
        let folge: Box<dyn Iterator<Item = &[f64; 2]>> = if durchgang == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in folge {
            while h.len() >= start + 2 && kreuz(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Umfang eines geschlossenen Polygons.
pub fn umfang(h: &[[f64; 2]]) -> f64 {
    (0..h.len())
        .map(|i| {
            let (a, b) = (h[i], h[(i + 1) % h.len()]);
            ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
        })
        .sum()
}

/// Konvexes Polygon (gegen den Uhrzeigersinn) um `abstand` nach außen
/// versetzt: jede Kante parallel, die Ecken auf Gehrung.
pub fn versetzt(h: &[[f64; 2]], abstand: f64) -> Vec<[f64; 2]> {
    let n = h.len();
    let normale = |a: [f64; 2], b: [f64; 2]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        [dy / l, -dx / l]
    };
    (0..n)
        .map(|i| {
            let (v, p, w) = (h[(i + n - 1) % n], h[i], h[(i + 1) % n]);
            let (n1, n2) = (normale(v, p), normale(p, w));
            let k = abstand / (1.0 + n1[0] * n2[0] + n1[1] * n2[1]).max(0.1);
            [p[0] + k * (n1[0] + n2[0]), p[1] + k * (n1[1] + n2[1])]
        })
        .collect()
}

/// Punkt bei der Bogenlänge `s` (beliebig, umlaufend) auf dem geschlossenen
/// Polygon, ab der ersten Ecke.
pub fn auf_polygon(h: &[[f64; 2]], s: f64) -> [f64; 2] {
    let ganz = umfang(h);
    let mut rest = s.rem_euclid(ganz);
    for i in 0..h.len() {
        let (a, b) = (h[i], h[(i + 1) % h.len()]);
        let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        if rest <= l || i + 1 == h.len() {
            let u = if l > 0.0 { (rest / l).min(1.0) } else { 0.0 };
            return [a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1])];
        }
        rest -= l;
    }
    h[0]
}

/// Bogenlänge, bei der der Strahl von `m` in Richtung `r` das Polygon
/// verlässt (`m` liegt innen).
fn bogen_in_richtung(h: &[[f64; 2]], m: [f64; 2], r: [f64; 2]) -> f64 {
    let mut s = 0.0;
    for i in 0..h.len() {
        let (a, b) = (h[i], h[(i + 1) % h.len()]);
        let e = [b[0] - a[0], b[1] - a[1]];
        let l = (e[0] * e[0] + e[1] * e[1]).sqrt();
        // m + t·r = a + u·e
        let det = r[0] * (-e[1]) - r[1] * (-e[0]);
        if det.abs() > 1e-15 {
            let w = [a[0] - m[0], a[1] - m[1]];
            let t = (w[0] * (-e[1]) - w[1] * (-e[0])) / det;
            let u = (r[0] * w[1] - r[1] * w[0]) / det;
            if t > 0.0 && (-1e-12..=1.0 + 1e-12).contains(&u) {
                return s + u.clamp(0.0, 1.0) * l;
            }
        }
        s += l;
    }
    0.0
}

/// Richtung in der Ebene zum Umfangswinkel (0 = vorn = −y, 90 = links = +x).
fn richtung(grad: f64) -> [f64; 2] {
    let w = grad.to_radians();
    [w.sin(), -w.cos()]
}

/// Height and radius of the neck seam at circumference angle `grad`,
/// interpolated cyclically and linearly between the four support points. The
/// seam is slanted: low at the front, high at the back.
fn hals_naht_bei(naht: &[(f64, f64, f64); 4], grad: f64) -> (f64, f64) {
    let g = grad.rem_euclid(360.0);
    for i in 0..4 {
        let (a, b) = (naht[i], naht[(i + 1) % 4]);
        let ende = if i == 3 { b.0 + 360.0 } else { b.0 };
        if g >= a.0 && g <= ende {
            let f = if (ende - a.0).abs() < 1e-12 { 0.0 } else { (g - a.0) / (ende - a.0) };
            return (a.2 * (1.0 - f) + b.2 * f, a.1 * (1.0 - f) + b.1 * f);
        }
    }
    (naht[0].2, naht[0].1)
}

// ---------------------------------------------------------------------------
// Hüllkörper
// ---------------------------------------------------------------------------

/// Was die Hüllkörper vom Körper brauchen (Szenenrahmen, m).
#[derive(Debug, Clone, Copy)]
pub struct Koerperteile<'a> {
    pub punkte: &'a [[f64; 3]],
    pub dreiecke: &'a [[u32; 3]],
    /// Punkte des Rumpfs. Es zählt nur ihre größte zusammenhängende Gruppe:
    /// CLOs Segmente (ältere Fassung) führen 8 Punkte der linken Hand als Rumpf.
    pub rumpf: &'a [usize],
    /// Punkte der Arme: links, rechts.
    pub arme: [&'a [usize]; 2],
    /// vordere Halsmitte (Drosselgrube), hintere Halsmitte (7. Halswirbel)
    pub hals_vorn: [f64; 3],
    pub hals_hinten: [f64; 3],
    /// seitlicher Halspunkt links, rechts
    pub hals_seitlich: [[f64; 3]; 2],
    /// Schulterpunkt links, rechts
    pub schulter: [[f64; 3]; 2],
    /// Ellbogen links, rechts
    pub ellbogen: [[f64; 3]; 2],
}

/// Hüllkörper um einen Oberarm: Achse und die Armpunkte in Achskoordinaten.
#[derive(Debug, Clone, PartialEq)]
pub struct Armhuelle {
    /// Achsenpunkt auf Höhe des Schulterpunkts (Lot von ihm auf die Achse).
    pub schulter: [f64; 3],
    /// Achse, zur Hand hin.
    pub achse: [f64; 3],
    /// senkrecht zur Achse: nach oben, nach vorn
    pub oben: [f64; 3],
    pub vorn: [f64; 3],
    /// Armpunkte als (t längs der Achse ab dem Achsenpunkt der Schulter,
    /// Koordinate oben, Koordinate vorn), m.
    pub punkte: Vec<[f64; 3]>,
}

impl Armhuelle {
    /// Das Armprisma für `t_unten`…`t_oben`: konvexe Hülle der Armpunkte dort
    /// in den Koordinaten (oben, vorn), um `abstand` versetzt (gegen den
    /// Uhrzeigersinn).
    pub fn umriss(&self, t_unten: f64, t_oben: f64, abstand: f64) -> Vec<[f64; 2]> {
        let p: Vec<[f64; 2]> = self.punkte.iter().filter(|p| (t_unten..=t_oben).contains(&p[0])).map(|p| [p[1], p[2]]).collect();
        versetzt(&huelle(p), abstand)
    }
}

/// Bezugshöhen am Körper (m über dem Boden).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hoehen {
    pub hals_vorn: f64,
    pub hals_hinten: f64,
    pub hals_seitlich: f64,
    pub schulter: f64,
}

/// Die Hüllkörper eines Körpers, ohne Abstand. Das Rumpfprisma entsteht
/// erst beim Platzieren, über die Höhe der Rumpfteile.
#[derive(Debug, Clone)]
pub struct Huellen {
    /// Rumpfpunkte (Lage).
    rumpf: Vec<[f64; 3]>,
    pub arme: [Armhuelle; 2],
    /// Halsmitte (x, y) und Radius.
    pub hals_mitte: [f64; 2],
    pub hals_radius: f64,
    /// The neck seam (neck opening of the torso) as four support points
    /// (circumference angle in degrees, distance from the axis, height in m),
    /// sorted by angle. The seam is slanted: low at the front, high at the back.
    pub hals_naht: [(f64, f64, f64); 4],
    pub hoehen: Hoehen,
}

/// Hauptachse (größter Eigenwert der Streuung) durch Potenziteration.
fn hauptachse(p: &[[f64; 3]], m: [f64; 3], start: [f64; 3]) -> [f64; 3] {
    let mut c = [[0.0; 3]; 3];
    for q in p {
        let d = sub(*q, m);
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] += d[i] * d[j];
            }
        }
    }
    let mut v = mul(start, 1.0 / norm(start));
    for _ in 0..200 {
        let w = [dot(c[0], v), dot(c[1], v), dot(c[2], v)];
        let l = norm(w);
        if l == 0.0 {
            break;
        }
        v = mul(w, 1.0 / l);
    }
    if dot(v, start) < 0.0 {
        mul(v, -1.0)
    } else {
        v
    }
}

/// Die größte über Netzkanten zusammenhängende Gruppe aus `auswahl`.
fn groesste_gruppe(auswahl: &[usize], dreiecke: &[[u32; 3]], n: usize) -> Vec<usize> {
    let mut drin = vec![false; n];
    for &i in auswahl {
        drin[i] = true;
    }
    let mut eltern: Vec<usize> = (0..n).collect();
    fn wurzel(e: &mut [usize], mut i: usize) -> usize {
        while e[i] != i {
            e[i] = e[e[i]];
            i = e[i];
        }
        i
    }
    for d in dreiecke {
        for k in 0..3 {
            let (a, b) = (d[k] as usize, d[(k + 1) % 3] as usize);
            if drin[a] && drin[b] {
                let (ra, rb) = (wurzel(&mut eltern, a), wurzel(&mut eltern, b));
                eltern[ra] = rb;
            }
        }
    }
    let mut groesse = vec![0usize; n];
    for &i in auswahl {
        let w = wurzel(&mut eltern, i);
        groesse[w] += 1;
    }
    let Some(beste) = (0..n).max_by_key(|&w| groesse[w]) else { return Vec::new() };
    auswahl.iter().copied().filter(|&i| wurzel(&mut eltern, i) == beste).collect()
}

fn normiert(a: [f64; 3]) -> [f64; 3] {
    mul(a, 1.0 / norm(a))
}

impl Huellen {
    pub fn neu(k: &Koerperteile) -> Result<Huellen, String> {
        if k.rumpf.is_empty() {
            return Err("keine Rumpfpunkte".into());
        }
        let mut arme = [None, None];
        for (s, arm) in k.arme.iter().enumerate() {
            let (sp, ep) = (k.schulter[s], k.ellbogen[s]);
            let se = sub(ep, sp);
            let lse = norm(se);
            if lse < 1e-3 {
                return Err("Schulter und Ellbogen fallen zusammen".into());
            }
            // Oberarm: Armpunkte zwischen Schulter und Ellbogen.
            let ober: Vec<[f64; 3]> = arm
                .iter()
                .map(|&i| k.punkte[i])
                .filter(|&p| (0.0..=lse).contains(&(dot(sub(p, sp), se) / lse)))
                .collect();
            if ober.len() < 10 {
                return Err(format!("Arm {}: nur {} Oberarmpunkte", ["links", "rechts"][s], ober.len()));
            }
            let m = mul(ober.iter().fold([0.0; 3], |a, &p| add(a, p)), 1.0 / ober.len() as f64);
            let achse = hauptachse(&ober, m, se);
            let lot = |p: [f64; 3]| add(m, mul(achse, dot(sub(p, m), achse)));
            let oben = normiert(sub([0.0, 0.0, 1.0], mul(achse, achse[2])));
            let vorn = {
                let v = normiert(cross(achse, oben));
                if v[1] > 0.0 {
                    mul(v, -1.0)
                } else {
                    v
                }
            };
            let schulter = lot(sp);
            let punkte = arm
                .iter()
                .map(|&i| {
                    let p = k.punkte[i];
                    let d = sub(p, lot(p));
                    [dot(sub(p, schulter), achse), dot(d, oben), dot(d, vorn)]
                })
                .collect();
            arme[s] = Some(Armhuelle { schulter, achse, oben, vorn, punkte });
        }
        let rumpf = groesste_gruppe(k.rumpf, k.dreiecke, k.punkte.len());
        let hals = [k.hals_vorn, k.hals_hinten, k.hals_seitlich[0], k.hals_seitlich[1]];
        let hals_mitte = [hals.iter().map(|p| p[0]).sum::<f64>() / 4.0, hals.iter().map(|p| p[1]).sum::<f64>() / 4.0];
        let hals_radius =
            hals.iter().map(|p| ((p[0] - hals_mitte[0]).powi(2) + (p[1] - hals_mitte[1]).powi(2)).sqrt()).fold(0.0, f64::max);
        // The four neck landmarks are the neck opening: per angle (0 = front)
        // their distance from the axis and their height. The neck is not a
        // horizontal plane — the seam sits 59 mm lower at the front than at the
        // back (findings/121).
        let mut hals_naht = [(0.0, 0.0, 0.0); 4];
        for (i, p) in hals.iter().enumerate() {
            let (dx, dy) = (p[0] - hals_mitte[0], p[1] - hals_mitte[1]);
            let grad = dy.atan2(dx).to_degrees() + 90.0;
            hals_naht[i] = (grad.rem_euclid(360.0), dx.hypot(dy), p[2]);
        }
        hals_naht.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(Huellen {
            rumpf: rumpf.iter().map(|&i| k.punkte[i]).collect(),
            arme: arme.map(|a| a.expect("beide Arme")),
            hals_mitte,
            hals_radius,
            hals_naht,
            hoehen: Hoehen {
                hals_vorn: k.hals_vorn[2],
                hals_hinten: k.hals_hinten[2],
                hals_seitlich: 0.5 * (k.hals_seitlich[0][2] + k.hals_seitlich[1][2]),
                schulter: 0.5 * (k.schulter[0][2] + k.schulter[1][2]),
            },
        })
    }

    /// Das Rumpfprisma für die Höhen `z_unten`…`z_oben`: konvexe Hülle der
    /// Rumpfpunkte dort, um `abstand` versetzt (gegen den Uhrzeigersinn).
    pub fn rumpfumriss(&self, z_unten: f64, z_oben: f64, abstand: f64) -> Vec<[f64; 2]> {
        let p: Vec<[f64; 2]> = self.rumpf.iter().filter(|p| (z_unten..=z_oben).contains(&p[2])).map(|p| [p[0], p[1]]).collect();
        versetzt(&huelle(p), abstand)
    }
}

// ---------------------------------------------------------------------------
// Lageangaben
// ---------------------------------------------------------------------------

/// Hüllkörper eines Teils.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Rumpf,
    Hals,
    /// 0 = links, 1 = rechts
    Arm(usize),
}

/// Grundhöhe einer Höhenangabe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hoehenbezug {
    /// Rumpf, Hals: über dem Boden. Arm: ab dem Schulterpunkt längs der Achse.
    Null,
    HalsVorn,
    HalsHinten,
    HalsSeitlich,
    Schulter,
}

/// Eine Zeile der Lagedatei.
#[derive(Debug, Clone, PartialEq)]
pub struct Lageangabe {
    pub teil: String,
    pub zone: Zone,
    /// Bezugspunkt (Musterpunkt); ohne: Mitte des Teils (Hüllrechteck).
    pub punkt: Option<String>,
    /// Umfangslage in Grad (siehe Moduldoku) und Versatz als Bogenlänge (m).
    pub umfang_grad: f64,
    pub umfang_bogen: f64,
    pub hoehe: (Hoehenbezug, f64),
    pub gespiegelt: bool,
    /// Drehung des Teils um den Bezugspunkt, Grad, gegen den Uhrzeigersinn
    /// wie im Musterbild.
    pub drehung_grad: f64,
    /// Abstand zur Haut für dieses Teil (m); ohne: der der Lagedatei.
    pub abstand: Option<f64>,
    /// Dicke der Folie dieses Teils (m), Laminat als Gesamtdicke; ohne: die
    /// des Materials ([`crate::Material::mit_dicke`]).
    pub dicke: Option<f64>,
    /// Auf dieses Teil kleben statt nähen ([`crate::klebung`]): der Name des
    /// Teils darunter. `None` = nicht geklebt.
    pub klebt_auf: Option<String>,
    /// `auf <Träger>` oder `auf <Träger>:<Punkt>`: dieser Flicken hat keinen
    /// eigenen Rahmen im Muster, sondern übernimmt Zone, Umfangslage, Höhe,
    /// Drehung, Spiegelung und Abstand von seinem Träger.
    ///
    /// Für Auflagen und Scheiben, die im Muster weit neben dem Kleidungsstück
    /// gezeichnet sind: ihre Lage auf dem Körper ergibt sich daraus, wo sie
    /// kleben, nicht aus einer eigenen Bogenlänge. Die Felder Zone, Umfang und
    /// Höhe bleiben dann `-`, weil sie aus dem Träger kommen.
    ///
    /// * ohne Punktnamen: der Anker (Feld 3) ist der gleichnamige Punkt des
    ///   Trägers, der Flicken liegt so auf ihm, wie beide im Muster liegen -
    ///   für Teile, die mit gemeinsamen Knoten konstruiert sind (Trim an der
    ///   Öffnung, Außenkragen auf dem Innenkragen).
    /// * mit Punktnamen: der eigene Anker wandert auf genau diesen Trägerpunkt,
    ///   die Ausrichtung bleibt die des Trägers - für Scheiben und Donut, die
    ///   als Vorrat neben dem Schnitt liegen.
    pub auf: Option<(String, Option<String>)>,
}

/// Die Lagedatei: Abstand der Hüllkörper und je Teil eine Angabe.
#[derive(Debug, Clone, PartialEq)]
pub struct Lageliste {
    pub abstand: f64,
    pub angaben: Vec<Lageangabe>,
}

fn zahl(s: &str) -> Result<f64, String> {
    s.trim().replace(',', ".").parse::<f64>().map_err(|_| format!("keine Zahl: {s:?}"))
}

/// „basis [±zahl]“: Basis aus `namen`, sonst eine bloße Zahl mit Basis `null`.
fn mit_zusatz<T: Copy>(s: &str, namen: &[(&str, T)], null: T) -> Result<(T, f64), String> {
    let s = s.trim().to_lowercase();
    for &(n, w) in namen {
        if let Some(rest) = s.strip_prefix(n) {
            let rest = rest.trim();
            if rest.is_empty() {
                return Ok((w, 0.0));
            }
            // `hals seitlich -2,24`, auch ohne Vorzeichen (`hals seitlich 0`).
            if rest.starts_with('+') || rest.starts_with('-') || rest.starts_with(|c: char| c.is_ascii_digit()) {
                return Ok((w, zahl(&rest.replace(' ', ""))?));
            }
        }
    }
    Ok((null, zahl(&s)?))
}

/// Byte-Position des Wortes `wort` in `s`, von Leerzeichen begrenzt. Damit
/// lassen sich Optionen in der letzten Spalte finden, ohne Teilnamen (die
/// Leerzeichen enthalten: `Band Mitte`) an Leerzeichen zu zerlegen.
fn wortposition(s: &str, wort: &str) -> Option<usize> {
    let mut von = 0;
    while let Some(i) = s[von..].find(wort).map(|i| von + i) {
        let grenze = |c: Option<char>| c.map_or(true, char::is_whitespace);
        if grenze(s[..i].chars().next_back()) && grenze(s[i + wort.len()..].chars().next()) {
            return Some(i);
        }
        von = i + wort.len();
    }
    None
}

/// Liest eine Lagedatei:
///
/// ```text
/// # Kommentar
/// abstand 2                       (cm, optional; Vorgabe 2)
/// Teil | Zone | Punkt | Umfang | Höhe | Optionen
/// ```
///
/// Zone: `rumpf`, `hals`, `arm links`, `arm rechts`. Punkt: ein Musterpunkt
/// oder `-` für die Mitte des Teils. Umfang: Grad oder `vorn`, `links`,
/// `hinten`, `rechts` (am Arm `oben`, `vorn`, `unten`, `hinten`) mit
/// optionalem `+N`/`-N` Grad oder `+Ncm`. Höhe in cm: am Rumpf und Hals über dem Boden
/// oder `hals vorn`, `hals hinten`, `hals seitlich`, `schulter` ± cm; am Arm
/// ab dem Schulterpunkt längs der Achse zur Hand. Optionen: `gespiegelt`,
/// `drehung N` (Grad), `abstand N` (cm, nur für dieses Teil), `dicke N` (mm,
/// Folie bzw. Laminat dieses Teils), `klebt <Teil>` (dieses Teil wird auf das
/// genannte geklebt statt genäht, siehe [`crate::klebung`]), `auf <Träger>`
/// oder `auf <Träger>:<Punkt>` (ein Flicken ohne eigenen Rahmen im Muster:
/// Zone, Umfang, Höhe, Prisma, Drehung und Abstand kommen aus dem Träger, die
/// drei Felder davor bleiben dann `-`; `gespiegelt`, `drehung` und `abstand`
/// gelten als eigener Zusatz um den Anker). Ein Zusatz mit `cm`
/// (`vorn +19,3cm`) ist Bogenlänge statt Grad.
pub fn lies_lage(text: &str) -> Result<Lageliste, String> {
    let mut liste = Lageliste { abstand: ABSTAND, angaben: Vec::new() };
    for (nr, zeile) in text.lines().enumerate() {
        let z = zeile.split('#').next().unwrap_or("").trim();
        if z.is_empty() {
            continue;
        }
        let fehler = |m: String| format!("Zeile {}: {m}", nr + 1);
        if let Some(rest) = z.strip_prefix("abstand") {
            liste.abstand = zahl(rest).map_err(fehler)? / 100.0;
            continue;
        }
        let f: Vec<&str> = z.split('|').map(str::trim).collect();
        if !(5..=6).contains(&f.len()) {
            return Err(fehler(format!("erwartet 5 oder 6 Felder, gefunden {}", f.len())));
        }
        // `auf <Traeger>` oder `auf <Traeger>:<Punkt>` ersetzt Zone, Umfang und
        // Hoehe: sie kommen aus dem Traeger. Vorher gelesen, weil die drei
        // Felder dann `-` sind. Teilnamen enthalten Leerzeichen ("Band Mitte"),
        // deshalb endet der Name nicht am ersten Leerzeichen, sondern dort, wo
        // eine bekannte Option der letzten Spalte beginnt.
        const OPTIONEN: [&str; 5] = ["gespiegelt", "drehung", "abstand", "dicke", "auf"];
        let mut auf: Option<(String, Option<String>)> = None;
        let mut klebt_auf: Option<String> = None;
        let mut rest = if let Some(opt) = f.get(5) { opt.trim().to_string() } else { String::new() };
        if let Some(opt) = f.get(5) {
            let mut opt = opt.trim();
            // `klebt` steht hinter `auf` und reicht bis zum Spaltenende.
            if let Some(i) = wortposition(opt, "klebt") {
                let name = opt[i + "klebt".len()..].trim().to_string();
                if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(fehler("erwartet 'klebt <Teilname>'".into()));
                }
                klebt_auf = Some(name);
                opt = opt[..i].trim_end();
            }
            rest = opt.to_string();
            if let Some(i) = wortposition(opt, "auf") {
                let nach = opt[i + "auf".len()..].trim_start();
                let ende = OPTIONEN
                    .iter()
                    .filter_map(|w| wortposition(nach, w))
                    .min()
                    .unwrap_or(nach.len());
                let arg = nach[..ende].trim();
                rest = nach[ende..].trim().to_string();
                let (traeger, punkt) = match arg.rsplit_once(':') {
                    Some((t, p)) if !t.trim().is_empty() && !p.trim().is_empty() => {
                        (t.trim(), Some(p.trim().to_string()))
                    }
                    Some(_) => {
                        return Err(fehler(format!(
                            "erwartet 'auf <Traeger>' oder 'auf <Traeger>:<Punkt>', gefunden {arg:?}"
                        )))
                    }
                    None if !arg.is_empty() => (arg, None),
                    None => return Err(fehler(format!("erwartet 'auf <Traeger>', gefunden {arg:?}"))),
                };
                auf = Some((traeger.to_string(), punkt));
            }
        }
        if auf.is_some() {
            // Eine zweite, widersprechende Zahl waere schlimmer als ein Fehler.
            for (feld, inhalt) in [("Zone", f[1]), ("Umfang", f[3]), ("Hoehe", f[4])] {
                if inhalt != "-" {
                    return Err(fehler(format!("mit 'auf' bleibt {feld} '-', gefunden {inhalt:?}")));
                }
            }
        }
        let (zone, umfang_grad, umfang_bogen, hoehe) = if auf.is_some() {
            // Platzhalter; `platziere_mit` setzt sie aus dem Traeger.
            (Zone::Rumpf, 0.0, 0.0, (Hoehenbezug::Null, 0.0))
        } else {
            let zone = match f[1].to_lowercase().as_str() {
                "rumpf" => Zone::Rumpf,
                "hals" => Zone::Hals,
                "arm links" => Zone::Arm(0),
                "arm rechts" => Zone::Arm(1),
                andere => return Err(fehler(format!("unbekannte Zone {andere:?}"))),
            };
            let umfang_namen: &[(&str, f64)] = if matches!(zone, Zone::Arm(_)) {
                &[("oben", 0.0), ("vorn", 90.0), ("unten", 180.0), ("hinten", 270.0)]
            } else {
                &[("vorn", 0.0), ("links", 90.0), ("hinten", 180.0), ("rechts", 270.0)]
            };
            let (umfang, bogen_cm) = match f[3].trim().strip_suffix("cm") {
                Some(rest) => (rest, true),
                None => (f[3], false),
            };
            let (basis, plus) = mit_zusatz(umfang, umfang_namen, 0.0).map_err(fehler)?;
            let (plus_grad, umfang_bogen) = if bogen_cm { (0.0, plus / 100.0) } else { (plus, 0.0) };
            let hoehe_namen = [
                ("hals vorn", Hoehenbezug::HalsVorn),
                ("hals hinten", Hoehenbezug::HalsHinten),
                ("hals seitlich", Hoehenbezug::HalsSeitlich),
                ("schulter", Hoehenbezug::Schulter),
            ];
            let (bezug, cm) = mit_zusatz(f[4], &hoehe_namen, Hoehenbezug::Null).map_err(fehler)?;
            (zone, basis + plus_grad, umfang_bogen, (bezug, cm / 100.0))
        };
        let punkt = (f[2] != "-").then(|| f[2].to_string());
        if let Some((_, None)) = &auf {
            // Ohne Punktnamen liegt der Flicken auf dem gleichnamigen Punkt des
            // Traegers; dann braucht er in Feld 3 seinen eigenen Punkt.
            if punkt.is_none() {
                return Err(fehler("mit 'auf <Traeger>' braucht das Teil seinen eigenen Punkt in Feld 3".into()));
            }
        }
        let (mut gespiegelt, mut drehung_grad, mut abstand, mut dicke) = (false, 0.0, None, None);
        {
            let w: Vec<&str> = rest.split_whitespace().collect();
            let mut i = 0;
            while i < w.len() {
                match w[i] {
                    "gespiegelt" => gespiegelt = true,
                    "drehung" => {
                        drehung_grad = zahl(w.get(i + 1).copied().unwrap_or("")).map_err(fehler)?;
                        i += 1;
                    }
                    "abstand" => {
                        abstand = Some(zahl(w.get(i + 1).copied().unwrap_or("")).map_err(fehler)? / 100.0);
                        i += 1;
                    }
                    "dicke" => {
                        dicke = Some(zahl(w.get(i + 1).copied().unwrap_or("")).map_err(fehler)? / 1000.0);
                        i += 1;
                    }
                    "auf" => return Err(fehler("'auf' steht zweimal".into())),
                    "klebt" => return Err(fehler("'klebt' steht zweimal".into())),
                    andere => return Err(fehler(format!("unbekannte Option {andere:?}"))),
                }
                i += 1;
            }
        }
        liste.angaben.push(Lageangabe {
            teil: f[0].to_string(),
            zone,
            punkt,
            umfang_grad,
            umfang_bogen,
            hoehe,
            gespiegelt,
            drehung_grad,
            abstand,
            dicke,
            klebt_auf,
            auf,
        });
    }
    Ok(liste)
}

// ---------------------------------------------------------------------------
// Platzieren
// ---------------------------------------------------------------------------

/// Ein platziertes Teil: wie seine Musterpunkte in den Raum kommen.
#[derive(Debug, Clone)]
struct Abwicklung {
    zone: Zone,
    /// Bezugspunkt im Muster (mm).
    anker: [f64; 2],
    drehung: f64,
    spiegel: f64,
    /// Rumpf: Bogenlänge des Ankers; Hals, Arm: Winkel des Ankers (rad).
    um: f64,
    /// Rumpf: Bogenlänge der Richtung ohne Versatz (vorn, hinten …); um sie
    /// weitet sich die Hülle ([`Platzierung::weite_bei`]).
    um_mitte: f64,
    /// Rumpf, Hals: Höhe des Ankers; Arm: Lage längs der Achse.
    hoch: f64,
    /// Rumpf: Index des Prismas; Hals: Radius mit Abstand.
    prisma: usize,
    radius: f64,
    /// Neck: the neck seam (angle, radius, height) — height of the piece point
    /// on it.
    hals_naht: [(f64, f64, f64); 4],
    /// Extra height (m) for a patch on the neck: the neck zone derives a point's
    /// height from the seam alone, so a patch glued above or below the seam
    /// carries the carrier's height here instead of in the pattern anchor.
    z_zusatz: f64,
    /// Arm: das Armprisma mit Abstand; `um` ist die Bogenlänge des Ankers.
    armumriss: Vec<[f64; 2]>,
    /// Abstand dieses Teils zur Haut (m) - der Suchradius der Klebezone.
    abstand: f64,
    dicke: Option<f64>,
}

/// Das Ergebnis: das Netz nur mit den platzierten Teilen und Nähten
/// zwischen ihnen, die Abwicklung je Teil und was weggelassen wurde.
#[derive(Debug, Clone)]
pub struct Platzierung {
    pub netz: Musternetz,
    abwicklung: Vec<Abwicklung>,
    /// Rumpfprismen je Abstand.
    prismen: Vec<Vec<[f64; 2]>>,
    arme: [Armhuelle; 2],
    hals_mitte: [f64; 2],
    /// Teile ohne Lageangabe (nicht in der Szene).
    pub weggelassen: Vec<String>,
    /// Nähte, von denen ein Teil fehlt.
    pub naehte_weggelassen: Vec<String>,
    /// Höhenbereich des Rumpfprismas (m).
    pub rumpf_hoehen: (f64, f64),
    /// Mitte des Rumpfs (x, y), um die sich die Hülle weitet.
    mitte: [f64; 2],
    /// Weite der Rumpfhülle je [`WEITE_DZ`] ab `rumpf_hoehen.0` (≥ 1).
    weite: Vec<f64>,
    /// Glue requests from the placement file: (piece index in `netz`, index of
    /// the piece below).
    klebt: Vec<(usize, usize)>,
}

/// Höhenschritt der Weite (m).
pub const WEITE_DZ: f64 = 0.02;

impl Platzierung {
    /// Weite der Rumpfhülle in der Höhe `z` (m): 1, wo die Rumpfteile
    /// zusammen nicht breiter sind als das Prisma; sonst ihr Verhältnis,
    /// geglättet und nach unten nie abnehmend (Befund 102).
    pub fn weite_bei(&self, z: f64) -> f64 {
        if self.weite.is_empty() {
            return 1.0;
        }
        let u = ((z - self.rumpf_hoehen.0) / WEITE_DZ - 0.5).clamp(0.0, (self.weite.len() - 1) as f64);
        let (i, f) = (u.floor() as usize, u - u.floor());
        let j = (i + 1).min(self.weite.len() - 1);
        self.weite[i] * (1.0 - f) + self.weite[j] * f
    }

    /// Das Rumpfprisma eines Teils (Umriss gegen den Uhrzeigersinn, m).
    pub fn rumpfumriss(&self, ti: usize) -> &[[f64; 2]] {
        &self.prismen[self.abwicklung[ti].prisma]
    }

    /// Je Teil: Bezugspunkt im Muster (mm), Umfangslage (Bogenlänge in m am
    /// Rumpf, Winkel in rad an Hals und Arm) und Höhe bzw. Lage längs der
    /// Armachse (m).
    pub fn anker(&self, ti: usize) -> ([f64; 2], f64, f64) {
        let a = &self.abwicklung[ti];
        (a.anker, a.um, a.hoch)
    }

    /// Dicke aus der Lageangabe von Teil `ti` (m), falls angegeben.
    pub fn dicke(&self, ti: usize) -> Option<f64> {
        self.abwicklung[ti].dicke
    }

    /// Glue zones requested by the placement file (`klebt <Teil>`), built
    /// against the placed scene.
    ///
    /// The pair goes into one zone per request: every point of the upper piece
    /// is bonded to the point of the lower piece it was placed on. The
    /// stiffness comes from the two materials ([`crate::klebung::Stoffpaar`]),
    /// the rest offset from where the pieces actually lie.
    ///
    /// The pair also carries the plate values of both layers, from which the
    /// zone derives its bending factor: applied through
    /// [`crate::Szene::klebungen_setzen`], the bonded area then bends like the
    /// laminate of the total thickness instead of like two separate sheets.
    ///
    /// The number returned with each zone counts the points of the upper piece
    /// that found **no** partner below - an open glued border, or a piece that
    /// does not lie on the one it is bonded to. Both go to the report, so a zone
    /// that silently bonds half its area cannot pass as complete.
    pub fn klebungen(&self, s: &Szene) -> Vec<(crate::klebung::Zone, usize)> {
        let mut aus = Vec::new();
        for &(oben_ti, unten_ti) in &self.klebt {
            let (o, u) = (&s.teile[oben_ti], &s.teile[unten_ti]);
            let paar = crate::klebung::Stoffpaar {
                dehnung_oben: o.vorgabe.material.dehnung_n_m,
                dicke_oben: o.vorgabe.material.dicke_m,
                dicke_unten: u.vorgabe.material.dicke_m,
                biegung_oben: o.vorgabe.material.biegung_nm,
                biegung_unten: u.vorgabe.material.biegung_nm,
            };
            // Suchradius: der Abstand, in dem beide Teile platziert wurden,
            // plus Dicken und ein Zuschlag - eine Kante direkt auf der anderen
            // darf nicht als offen gelten, ein weit entferntes Teil aber auch
            // nicht stillschweigend kleben.
            let radius = 2.0 * (self.abwicklung[oben_ti].abstand + self.abwicklung[unten_ti].abstand)
                + 2.0 * (o.vorgabe.material.dicke_m + u.vorgabe.material.dicke_m)
                + 1e-3;
            let (zone, ohne) = crate::klebung::Zone::flaeche(
                &format!("{} auf {}", o.name, u.name),
                &s.x,
                &s.dreiecke,
                o.punkte.clone(),
                u.punkte.clone(),
                radius,
                paar,
            );
            aus.push((zone, ohne));
        }
        aus
    }

    /// Lage des Musterpunkts `q` (mm) von Teil `ti` (Index in [`Self::netz`]).
    pub fn lage(&self, ti: usize, q: [f64; 2]) -> [f64; 3] {
        let a = &self.abwicklung[ti];
        // Muster: x rechts, y unten → e: x rechts, y oben (m); drehen, spiegeln.
        let e = [(q[0] - a.anker[0]) * 1e-3, -(q[1] - a.anker[1]) * 1e-3];
        let (s, c) = a.drehung.sin_cos();
        let e = [a.spiegel * (e[0] * c - e[1] * s), e[0] * s + e[1] * c];
        match a.zone {
            Zone::Rumpf => {
                // Geweitete Hülle: längentreu in der Breite, um die Richtung
                // des Teils (vorn, hinten …) und die Mitte des Rumpfs.
                let (s, z) = (a.um + e[0], a.hoch + e[1]);
                let w = self.weite_bei(z);
                let q = auf_polygon(&self.prismen[a.prisma], a.um_mitte + (s - a.um_mitte) / w);
                [self.mitte[0] + w * (q[0] - self.mitte[0]), self.mitte[1] + w * (q[1] - self.mitte[1]), z]
            }
            Zone::Hals => {
                let w = a.um + e[0] / a.radius;
                // The seam is slanted per angle (low at the front, high at the
                // back); every point sits vertically above its own seam point
                // (findings/121). The anchor (`KHM`) lies on the collar's lower
                // edge, and that edge is sewn to the neck opening along N13 —
                // so it belongs on the seam. The height in the layout file does
                // not apply here: the layout suggestion writes `v.y +
                // hals_hoehe` there, a 2D pattern height; read as a 3D offset it
                // would pull the collar below the seam again.
                let grad = w.to_degrees() + 90.0;
                let (z_naht, _) = hals_naht_bei(&a.hals_naht, grad);
                [
                    self.hals_mitte[0] + a.radius * w.cos(),
                    self.hals_mitte[1] + a.radius * w.sin(),
                    z_naht + e[1] + a.z_zusatz,
                ]
            }
            Zone::Arm(s) => {
                let z = &self.arme[s];
                // Muster-x zeigt für den Betrachter von außen nach rechts:
                // oben auf dem Arm ist das oben × Achse.
                let sigma = dot(cross(z.oben, z.achse), z.vorn).signum();
                let t = a.hoch - e[1];
                let q = auf_polygon(&a.armumriss, a.um + sigma * e[0]);
                add(add(z.schulter, mul(z.achse, t)), add(mul(z.oben, q[0]), mul(z.vorn, q[1])))
            }
        }
    }
}

/// Höhe eines Teils im Raum (m) aus der Höhenspalte seiner Lageangabe.
fn hoehe_im_raum(a: &Lageangabe, h: &Hoehen) -> f64 {
    let basis = match a.hoehe.0 {
        Hoehenbezug::Null => 0.0,
        Hoehenbezug::HalsVorn => h.hals_vorn,
        Hoehenbezug::HalsHinten => h.hals_hinten,
        Hoehenbezug::HalsSeitlich => h.hals_seitlich,
        Hoehenbezug::Schulter => h.schulter,
    };
    basis + a.hoehe.1
}

/// Der Träger eines Flickens (`auf <Träger>:<Punkt>`): der Index des Teils in
/// `teile`. Ein fehlender oder mehrdeutiger Träger ist ein Fehler und kein
/// stilles Nichtkleben; die Reihenfolge in der Lagedatei spielt keine Rolle,
/// weil [`platziere_mit`] die Flicken in Abhängigkeitsreihenfolge auflöst.
fn carrier_of(teile: &[klon3d_mesh::Teilnetz], name: &str, ti: usize, eigen: &str) -> Result<usize, String> {
    let treffer: Vec<usize> = (0..teile.len())
        .filter(|&i| teile[i].teil == name || teile[i].teil.rsplit(" - ").next() == Some(name))
        .collect();
    match treffer[..] {
        [u] if u == ti => Err(format!("{eigen}: Traeger ist das Teil selbst")),
        [u] => Ok(u),
        [] => Err(format!("{eigen}: Traeger {name:?} steht nicht in dieser Lagedatei")),
        _ => Err(format!("{eigen}: Traegername {name:?} ist mehrdeutig")),
    }
}

/// Mitte des Hüllrechtecks eines Teils (mm).
fn teilmitte(punkte: &[[f64; 2]]) -> [f64; 2] {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in punkte {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])]
}

/// Rumpfprisma im Abstand `ab` (m), notfalls neu gebaut. Ein Flicken mit
/// eigenem Abstand (`abstand` neben `auf`) braucht eines, das es unter den
/// Teilen der Lagedatei nicht gibt: die Hülle wird aus den Rumpfteilen
/// gebaut, und Flicken zählen dort nicht mit.
fn prisma_im_abstand(
    prismen: &mut Vec<(f64, Vec<[f64; 2]>)>,
    ab: f64,
    z: (f64, f64),
    huellen: &Huellen,
) -> Result<usize, String> {
    if let Some(i) = prismen.iter().position(|(x, _)| *x == ab) {
        return Ok(i);
    }
    let u = huellen.rumpfumriss(z.0, z.1, ab);
    if u.len() < 3 {
        return Err(format!(
            "keine Rumpfpunkte zwischen {:.2} und {:.2} m für den Abstand {:.1} cm",
            z.0,
            z.1,
            ab * 100.0
        ));
    }
    prismen.push((ab, u));
    Ok(prismen.len() - 1)
}

/// Legt die Teile mit Lageangabe um den Körper. Bezugspunkte sucht sie unter
/// den Punktknoten des Teils, dann unter denen aller Teile.
pub fn platziere(netz: &Musternetz, liste: &Lageliste, huellen: &Huellen) -> Result<Platzierung, String> {
    platziere_mit(netz, liste, huellen, true)
}

/// Wie [`platziere`]; `weiten = false` lässt die Rumpfhülle senkrecht (zum
/// Vergleich, Befund 102).
pub fn platziere_mit(netz: &Musternetz, liste: &Lageliste, huellen: &Huellen, weiten: bool) -> Result<Platzierung, String> {
    let punkt = |teil: &klon3d_mesh::Teilnetz, name: &str| -> Option<[f64; 2]> {
        std::iter::once(teil)
            .chain(netz.teile.iter())
            .find_map(|t| t.punktknoten.iter().find(|(n, _)| n == name).map(|&(_, i)| t.punkte[i as usize]))
    };
    // Teilnamen wie in der Nahtliste: genau, sonst der Name hinter dem
    // letzten " - " (Seamly stellt den Blocknamen voran).
    let mut angabe: Vec<Option<&Lageangabe>> = vec![None; netz.teile.len()];
    for a in &liste.angaben {
        let genau: Vec<usize> = (0..netz.teile.len()).filter(|&i| netz.teile[i].teil == a.teil).collect();
        let wahl = if genau.is_empty() {
            (0..netz.teile.len()).filter(|&i| netz.teile[i].teil.rsplit(" - ").next() == Some(a.teil.as_str())).collect()
        } else {
            genau
        };
        match wahl[..] {
            [i] if angabe[i].is_some() => return Err(format!("Teil {:?} hat zwei Lageangaben", a.teil)),
            [i] => angabe[i] = Some(a),
            [] => return Err(format!("Lageangabe für unbekanntes Teil {:?}", a.teil)),
            _ => return Err(format!("Teilname {:?} ist mehrdeutig", a.teil)),
        }
    }
    // Teile auswählen, Indizes umschreiben.
    let mut neu_index = vec![None; netz.teile.len()];
    let mut teile = Vec::new();
    let mut angaben = Vec::new();
    let mut weggelassen = Vec::new();
    for (i, t) in netz.teile.iter().enumerate() {
        match angabe[i] {
            Some(a) => {
                neu_index[i] = Some(teile.len());
                teile.push(t.clone());
                angaben.push(a);
            }
            None => weggelassen.push(t.teil.clone()),
        }
    }
    let mut naehte = Vec::new();
    let mut naehte_weggelassen = Vec::new();
    for n in &netz.naehte {
        let tn: Option<Vec<_>> = n
            .teilnaehte
            .iter()
            .map(|t| {
                let (a, b) = (neu_index[t.teil_a]?, neu_index[t.teil_b]?);
                let mut t = t.clone();
                t.teil_a = a;
                t.teil_b = b;
                Some(t)
            })
            .collect();
        match tn {
            Some(tn) => naehte.push(Nahtnetz { teilnaehte: tn, ..n.clone() }),
            None => naehte_weggelassen.push(n.kennung.clone()),
        }
    }

    // Anker (der eigene Musterpunkt), Reichweite und Höhenbereich. Ein
    // Flicken (`auf`) spannt keinen eigenen Höhenbereich auf: er liegt auf
    // seinem Träger und übernimmt dessen Rahmen.
    let h = &huellen.hoehen;
    let mut anker = Vec::with_capacity(angaben.len());
    let mut reich = Vec::with_capacity(angaben.len());
    let (mut z_unten, mut z_oben) = (f64::INFINITY, f64::NEG_INFINITY);
    for (t, a) in teile.iter().zip(&angaben) {
        let q = match &a.punkt {
            Some(n) => punkt(t, n).ok_or_else(|| format!("{}: Punktknoten {n:?} in keinem Teil", t.teil))?,
            None => teilmitte(&t.punkte),
        };
        // Reichweite um den Anker (m), auch gedreht: Höhenbereich des Prismas.
        let r = t.punkte.iter().map(|p| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt() * 1e-3).fold(0.0, f64::max);
        if a.auf.is_none() {
            let hoch = hoehe_im_raum(a, h);
            if a.zone == Zone::Rumpf {
                z_unten = z_unten.min(hoch - r);
                z_oben = z_oben.max(hoch + r);
            }
        }
        anker.push(q);
        reich.push(r);
    }
    let abstand = |a: &Lageangabe| a.abstand.unwrap_or(liste.abstand);
    // Ein Prisma je vorkommendem Abstand der Rumpfteile; Flicken zählen nicht,
    // sie liegen auf einem Träger und weiten den Rumpf nicht.
    let mut prismen: Vec<(f64, Vec<[f64; 2]>)> = Vec::new();
    for a in angaben.iter().filter(|a| a.zone == Zone::Rumpf && a.auf.is_none()) {
        let ab = abstand(a);
        if !prismen.iter().any(|(x, _)| *x == ab) {
            let u = huellen.rumpfumriss(z_unten, z_oben.min(h.schulter), ab);
            if u.len() < 3 {
                return Err(format!("keine Rumpfpunkte zwischen {:.2} und {:.2} m", z_unten, z_oben));
            }
            prismen.push((ab, u));
        }
    }
    // Mitte des Rumpfs: aus dem Prisma ohne Abstand, für alle gleich.
    let mitte = if prismen.is_empty() {
        [0.0; 2]
    } else {
        let u = huellen.rumpfumriss(z_unten, z_oben.min(h.schulter), 0.0);
        let n = u.len() as f64;
        [u.iter().map(|p| p[0]).sum::<f64>() / n, u.iter().map(|p| p[1]).sum::<f64>() / n]
    };

    // Every piece gets its frame from the body, or - for a glued-on patch
    // (`auf <Träger>:<Punkt>`) - from the piece underneath it. Patches may be
    // chained (a disc on a collar that itself lies on the neck), so they are
    // resolved in dependency order rather than file order.
    let mut abwicklung: Vec<Option<Abwicklung>> = vec![None; angaben.len()];
    let mut offen: Vec<usize> = (0..angaben.len()).collect();
    while !offen.is_empty() {
        let mut fortschritt = false;
        let mut naechste = Vec::new();
        for &ti in &offen {
            let a = &angaben[ti];
            let ab = abstand(a);
            let rahmen = match &a.auf {
                // A glued-on patch has no frame of its own in the pattern: it
                // lies on its carrier, so it takes the carrier's zone, prism,
                // neck or arm hull and offset from the body. Only its own
                // pattern points stay, and its base is shifted by the carrier
                // point's share so the reference point lands where the carrier
                // put it. Its own `gespiegelt`, `drehung` and `abstand` are an
                // additional rotation, mirror and offset about its anchor.
                Some((traeger, punktname)) => {
                    let ut = carrier_of(&teile, traeger, ti, &a.teil)?;
                    let Some(u) = &abwicklung[ut] else {
                        // Carrier not resolved yet - next pass.
                        naechste.push(ti);
                        continue;
                    };
                    let wt = &teile[ut];
                    // Without a point name the patch sits on its own named
                    // point of the carrier (pieces built with shared nodes);
                    // with one, its anchor moves onto exactly that point.
                    let name = punktname
                        .as_deref()
                        .or(a.punkt.as_deref())
                        .ok_or_else(|| format!("{}: 'auf' braucht einen Punkt in Feld 3", a.teil))?;
                    let p = punkt(wt, name).ok_or_else(|| {
                        format!("{}: Punktknoten {name:?} des Traegers {:?} nicht gefunden", a.teil, wt.teil)
                    })?;
                    // Where the carrier puts that point, as a local offset.
                    let e = [(p[0] - u.anker[0]) * 1e-3, -(p[1] - u.anker[1]) * 1e-3];
                    let (s, c) = u.drehung.sin_cos();
                    let e = [u.spiegel * (e[0] * c - e[1] * s), e[0] * s + e[1] * c];
                    let (mut um, mut hoch, mut z_zusatz) = (u.um, u.hoch, 0.0);
                    match u.zone {
                        Zone::Rumpf => {
                            um += e[0];
                            hoch += e[1];
                        }
                        Zone::Hals => {
                            // The neck zone puts every point vertically above
                            // its own seam point, so the offset along the neck
                            // is an angle and the height becomes a plain add-on.
                            um += e[0] / u.radius;
                            z_zusatz = e[1];
                        }
                        Zone::Arm(si) => {
                            // Along the arm the pattern x is mirrored per side
                            // and the axis runs head to hand.
                            let z = &huellen.arme[si];
                            let sigma = dot(cross(z.oben, z.achse), z.vorn).signum();
                            um += sigma * e[0];
                            hoch -= e[1];
                        }
                    }
                    // A patch with an offset of its own (two collar layers, a
                    // disc lifted off its carrier) may need a torso prism that
                    // no sewn piece asked for. Build it instead of pointing at
                    // whatever sits at index 0.
                    let ab_p = a.abstand.unwrap_or(u.abstand);
                    let prisma = if u.zone == Zone::Rumpf && ab_p != u.abstand {
                        prisma_im_abstand(&mut prismen, ab_p, (z_unten, z_oben.min(h.schulter)), huellen)?
                    } else {
                        u.prisma
                    };
                    // The same holds for the neck and the arm: a layer at
                    // another offset sits on another radius or another hull.
                    let radius = if u.zone == Zone::Hals { huellen.hals_radius + ab_p } else { u.radius };
                    let armumriss = match u.zone {
                        Zone::Arm(si) if ab_p != u.abstand => {
                            let z = &huellen.arme[si];
                            let u2 = z.umriss((hoch - reich[ti]).max(0.0), hoch + reich[ti], ab_p);
                            if u2.len() < 3 {
                                return Err(format!(
                                    "{}: keine Armpunkte zwischen {:.2} und {:.2} m längs der Achse",
                                    a.teil,
                                    hoch - reich[ti],
                                    hoch + reich[ti]
                                ));
                            }
                            u2
                        }
                        _ => u.armumriss.clone(),
                    };
                    Abwicklung {
                        zone: u.zone,
                        anker: anker[ti],
                        drehung: u.drehung + a.drehung_grad.to_radians(),
                        spiegel: if a.gespiegelt { -u.spiegel } else { u.spiegel },
                        um,
                        um_mitte: u.um_mitte,
                        hoch,
                        prisma,
                        radius,
                        hals_naht: huellen.hals_naht,
                        z_zusatz,
                        armumriss,
                        dicke: a.dicke,
                        abstand: ab_p,
                    }
                }
                None => {
                    let hoch = hoehe_im_raum(a, h);
                    // Am Arm: das Prisma über die Länge des Teils.
                    let armumriss = match a.zone {
                        Zone::Arm(s) => {
                            let u = huellen.arme[s].umriss((hoch - reich[ti]).max(0.0), hoch + reich[ti], ab);
                            if u.len() < 3 {
                                return Err(format!(
                                    "{}: keine Armpunkte zwischen {:.2} und {:.2} m längs der Achse",
                                    a.teil,
                                    hoch - reich[ti],
                                    hoch + reich[ti]
                                ));
                            }
                            u
                        }
                        _ => Vec::new(),
                    };
                    let prisma = prismen.iter().position(|(x, _)| *x == ab).unwrap_or(0);
                    let um_mitte = if a.zone == Zone::Rumpf {
                        bogen_in_richtung(&prismen[prisma].1, mitte, richtung(a.umfang_grad))
                    } else {
                        0.0
                    };
                    let radius = if a.zone == Zone::Hals { huellen.hals_radius + ab } else { 0.0 };
                    let um = match a.zone {
                        Zone::Rumpf => um_mitte + a.umfang_bogen,
                        // Winkel in der Ebene: 0 = vorn (−y) entspricht −90° Grad.
                        Zone::Hals => (a.umfang_grad - 90.0).to_radians() + a.umfang_bogen / radius,
                        Zone::Arm(_) => {
                            let w = a.umfang_grad.to_radians();
                            bogen_in_richtung(&armumriss, [0.0, 0.0], [w.cos(), w.sin()]) + a.umfang_bogen
                        }
                    };
                    Abwicklung {
                        zone: a.zone,
                        anker: anker[ti],
                        drehung: a.drehung_grad.to_radians(),
                        spiegel: if a.gespiegelt { -1.0 } else { 1.0 },
                        um,
                        um_mitte,
                        hoch,
                        prisma,
                        radius,
                        hals_naht: huellen.hals_naht,
                        z_zusatz: 0.0,
                        armumriss,
                        dicke: a.dicke,
                        abstand: ab,
                    }
                }
            };
            abwicklung[ti] = Some(rahmen);
            fortschritt = true;
        }
        if !fortschritt {
            // Everything left depends on something that never resolves: a
            // ring, or a carrier that itself waits. Name it instead of looping.
            let ti = naechste[0];
            let a = &angaben[ti];
            let (traeger, _) = a.auf.clone().expect("nur 'auf'-Teile bleiben offen");
            return Err(format!(
                "{}: Traeger {traeger:?} loest sich nicht auf (Kreis oder Kette ueber sich selbst)",
                a.teil
            ));
        }
        offen = naechste;
    }
    let abwicklung: Vec<Abwicklung> = abwicklung.into_iter().map(|a| a.expect("alle aufgeloest")).collect();

    // Glue requests: (piece, piece below). Names like in the seam list, so
    // also the name behind the last " - ".
    let mut klebt: Vec<(usize, usize)> = Vec::new();
    for (ti, a) in angaben.iter().enumerate() {
        if let Some(ziel) = &a.klebt_auf {
            let treffer: Vec<usize> = (0..teile.len())
                .filter(|&i| teile[i].teil == *ziel || teile[i].teil.rsplit(" - ").next() == Some(ziel.as_str()))
                .collect();
            match treffer[..] {
                [u] if u == ti => return Err(format!("{}: klebt auf sich selbst", a.teil)),
                [u] => klebt.push((ti, u)),
                [] => return Err(format!("{}: klebt auf unbekanntes Teil {ziel:?}", a.teil)),
                _ => return Err(format!("{}: Klebeziel {ziel:?} ist mehrdeutig", a.teil)),
            }
        }
    }

    // Weite der Rumpfhülle: Sind die Rumpfteile in einer Höhe zusammen
    // breiter als das Prisma (ausgestellte Rockbahnen), überlappten sie dort
    // auf ihm, und die Saumecken rutschten zwischen die Beine (Befund 100).
    // Je 2 cm: Summe der Breiten (Musterpunkte der Teile in der Abwicklung) ÷
    // Umfang, mindestens 1, nach unten nie abnehmend (sonst eine Zwiebel),
    // über ±4 cm geglättet.
    let rumpf_hoehen = (z_unten, z_oben.min(h.schulter));
    let mut weite = Vec::new();
    if weiten && !prismen.is_empty() && rumpf_hoehen.1 > rumpf_hoehen.0 {
        let fach = ((rumpf_hoehen.1 - rumpf_hoehen.0) / WEITE_DZ).ceil() as usize + 1;
        let haupt = prismen.iter().position(|(x, _)| *x == liste.abstand).unwrap_or(0);
        let p_umfang = umfang(&prismen[haupt].1);
        let mut breite = vec![0.0; fach];
        for (ti, a) in abwicklung.iter().enumerate().filter(|(ti, a)| a.zone == Zone::Rumpf && angaben[*ti].auf.is_none()) {
            let mut je: Vec<(f64, f64)> = vec![(f64::INFINITY, f64::NEG_INFINITY); fach];
            for q in &teile[ti].punkte {
                let e = [(q[0] - a.anker[0]) * 1e-3, -(q[1] - a.anker[1]) * 1e-3];
                let (s, c) = a.drehung.sin_cos();
                let e = [a.spiegel * (e[0] * c - e[1] * s), e[0] * s + e[1] * c];
                let j = (((a.hoch + e[1]) - rumpf_hoehen.0) / WEITE_DZ).floor();
                if j >= 0.0 && (j as usize) < fach {
                    let b = &mut je[j as usize];
                    b.0 = b.0.min(a.um + e[0]);
                    b.1 = b.1.max(a.um + e[0]);
                }
            }
            for (j, b) in je.iter().enumerate() {
                if b.1 > b.0 {
                    breite[j] += b.1 - b.0;
                }
            }
        }
        let roh: Vec<f64> = breite.iter().map(|b| (b / p_umfang).max(1.0)).collect();
        let abwaerts = |v: &mut Vec<f64>| {
            for j in (0..v.len().saturating_sub(1)).rev() {
                v[j] = v[j].max(v[j + 1]);
            }
        };
        let mut w = roh;
        abwaerts(&mut w);
        let glatt: Vec<f64> = (0..fach).map(|j| {
            let (a, b) = (j.saturating_sub(2), (j + 2).min(fach - 1));
            w[a..=b].iter().sum::<f64>() / (b - a + 1) as f64
        }).collect();
        w = glatt;
        abwaerts(&mut w);
        if w.iter().any(|&x| x > 1.0 + 1e-6) {
            weite = w;
        }
    }

    Ok(Platzierung {
        netz: Musternetz { teile, naehte, ..netz.clone() },
        abwicklung,
        prismen: prismen.into_iter().map(|(_, u)| u).collect(),
        arme: huellen.arme.clone(),
        hals_mitte: huellen.hals_mitte,
        weggelassen,
        naehte_weggelassen,
        rumpf_hoehen,
        mitte,
        weite,
        klebt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huelle_und_versatz() {
        let q = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]];
        let h = huelle(q);
        assert_eq!(h.len(), 4);
        assert!((umfang(&h) - 4.0).abs() < 1e-12);
        // Quadrat um 0,1 versetzt: Kanten parallel, Ecken auf Gehrung.
        let v = versetzt(&h, 0.1);
        assert!((umfang(&v) - 4.8).abs() < 1e-12, "{}", umfang(&v));
        // Strahl von der Mitte nach rechts trifft die rechte Kante bei y = 0,5.
        let s = bogen_in_richtung(&h, [0.5, 0.5], [1.0, 0.0]);
        let p = auf_polygon(&h, s);
        assert!((p[0] - 1.0).abs() < 1e-12 && (p[1] - 0.5).abs() < 1e-12, "{p:?}");
    }

    #[test]
    fn lagedatei_lesen() {
        let l = lies_lage(
            "# Prüfung\nabstand 3\nVorn | rumpf | VK0 | vorn +5 | hals vorn -2,5 |\nÄrmel | arm rechts | - | oben | 0 | gespiegelt drehung 10 abstand 1\nKragen | hals | KHM | hinten -4,5cm | 140 |\n",
        )
        .unwrap();
        assert!((l.abstand - 0.03).abs() < 1e-12);
        assert_eq!(l.angaben.len(), 3);
        let v = &l.angaben[0];
        assert_eq!((v.zone, v.punkt.as_deref(), v.umfang_grad), (Zone::Rumpf, Some("VK0"), 5.0));
        assert_eq!(v.hoehe.0, Hoehenbezug::HalsVorn);
        assert!((v.hoehe.1 + 0.025).abs() < 1e-12);
        let a = &l.angaben[1];
        assert_eq!((a.zone, a.punkt.as_ref(), a.gespiegelt, a.drehung_grad), (Zone::Arm(1), None, true, 10.0));
        assert_eq!(a.abstand, Some(0.01));
        let k = &l.angaben[2];
        assert_eq!((k.umfang_grad, k.hoehe), (180.0, (Hoehenbezug::Null, 1.4)));
        assert!((k.umfang_bogen + 0.045).abs() < 1e-12);
        // Klebezone: `klebt <Teil>` liest den Namen, ein fehlender oder
        // numerischer Name ist ein Fehler und kein stilles Nichtkleben.
        let g = lies_lage("Verst | rumpf | - | vorn | 0 | klebt Vorn\n").unwrap();
        assert_eq!(g.angaben[0].klebt_auf.as_deref(), Some("Vorn"));
        assert!(lies_lage("Verst | rumpf | - | vorn | 0 | klebt").is_err());
        assert!(lies_lage("Verst | rumpf | - | vorn | 0 | klebt 3").is_err());
        assert!(lies_lage("X | bein | - | 0 | 0").is_err());
        assert!(lies_lage("X | rumpf | - | vorn | 0 | quer").is_err());
        // `auf <Träger>` oder `auf <Träger>:<Punkt>` holt Zone, Umfang und Höhe
        // aus dem Träger; die drei Felder müssen dann `-` sein.
        let t = lies_lage("Flicken | - | HD0 | - | - | auf Band Mitte:VPB klebt Band Mitte\n").unwrap();
        assert_eq!(
            t.angaben[0].auf.as_ref().map(|(a, b)| (a.as_str(), b.as_deref())),
            Some(("Band Mitte", Some("VPB")))
        );
        assert_eq!(t.angaben[0].punkt.as_deref(), Some("HD0"));
        assert_eq!(t.angaben[0].klebt_auf.as_deref(), Some("Band Mitte"));
        // Ohne Punktnamen liegt der Flicken auf dem gleichnamigen Punkt des
        // Trägers; der Punktname darf auch aus Feld 3 kommen.
        let o = lies_lage("Kragen | - | KHM | - | - | auf Stehkragen Innen\n").unwrap();
        assert_eq!(o.angaben[0].auf.as_ref().map(|(a, b)| (a.as_str(), b.is_none())), Some(("Stehkragen Innen", true)));
        assert_eq!(o.angaben[0].punkt.as_deref(), Some("KHM"));
        // Eine eigene Drehung, Spiegelung und ein eigener Abstand sind ein
        // Zusatz um den Anker, kein Widerspruch zum Träger.
        let z = lies_lage("Scheibe | - | HS1 | - | - | auf Band Mitte:VH_L drehung 12 gespiegelt abstand 0,5\n")
            .unwrap();
        assert_eq!(z.angaben[0].drehung_grad, 12.0);
        assert!(z.angaben[0].gespiegelt);
        assert_eq!(z.angaben[0].abstand, Some(0.005));
        // Leere Seite, doppelt, oder mit einer eigenen Zahl in Feld 1/3/4:
        // jeweils ein Fehler statt einer stillen Zahl.
        assert!(lies_lage("F | - | HD0 | - | - | auf :VPB\n").is_err());
        assert!(lies_lage("F | - | HD0 | - | - | auf Band Mitte:\n").is_err());
        assert!(lies_lage("F | - | HD0 | - | - | auf A:B auf C:D\n").is_err());
        assert!(lies_lage("F | rumpf | HD0 | - | - | auf A:B\n").is_err());
        assert!(lies_lage("F | - | HD0 | vorn | - | auf A:B\n").is_err());
        assert!(lies_lage("F | - | HD0 | - | 0 | auf A:B\n").is_err());
        // Ohne Punktnamen braucht das Teil seinen eigenen Punkt in Feld 3.
        assert!(lies_lage("F | - | - | - | - | auf Band Mitte\n").is_err());
        // Mit Punktnamen darf Feld 3 leer bleiben: die Mitte des Teils.
        assert!(lies_lage("F | - | - | - | - | auf Band Mitte:VPB\n").is_ok());
    }
}

// ---------------------------------------------------------------------------
// Anziehen
// ---------------------------------------------------------------------------

/// Der Ablauf beim Anziehen (Befunde 96, 98).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anziehen {
    /// Nähen: die Ruhelänge jeder Nahtfeder schrumpft mit dieser
    /// Geschwindigkeit (m/s) bis 0 ([`Szene::naht_kuerzen`]).
    pub naht_geschwindigkeit: f64,
    /// Bilder des Nähens; währenddessen Körperpunkt gegen Stoffdreieck.
    pub naehbilder: usize,
    /// Danach, ohne diese Korrektur und mit voller Schwerkraft, Einschwingen:
    /// mindestens so viele Bilder, dann bis sich in zwei Sekunden
    /// hintereinander kein Teil im Mittel um mehr als
    /// [`Anziehen::ruhe_sinken`] je Sekunde hebt oder senkt, höchstens
    /// `einschwingen_max` Bilder. Der lange Rock v36 setzte sich nach 2 s noch
    /// (6,3 mm in der Haltephase, nach 12 s Einschwingen 0,0 mm; Befund 102).
    pub einschwingen_min: usize,
    pub einschwingen_max: usize,
    /// Höhenänderung je Sekunde (m), unter der ein Teil als eingeschwungen gilt:
    /// die Hälfte dessen, was ADR-023 in der Haltephase erlaubt.
    pub ruhe_sinken: f64,
    /// Bilder der Haltephase nach dem Einschwingen (ADR-023: 10 s).
    pub haltebilder: usize,
    /// Durchgänge Körperpunkt gegen Stoffdreieck je Schritt beim Nähen.
    pub koerperpunkt_durchgaenge: usize,
    /// Anteil der Schwerkraft beim Nähen (1 = volle Schwerkraft, 0 = keine);
    /// beim Ruhen gilt sie wieder voll. 0 bildet nach, dass man ein Teil
    /// hält, bis es zu ist (ADR-023, Punkt 2): Trägerlose Teile rutschen
    /// sonst ab, bevor die Nähte zu sind (Bustier v1: 24 cm).
    pub schwerkraft_naehen: f64,
    /// Steifigkeit der Nahtfeder beim Anziehen (N/m), statt
    /// [`crate::ImplizitParameter::naht_k`] (500 N/m wie CLOs `fSeamSpringK`,
    /// Umrechnung nicht belegt). 2.000 N/m schließt an allen Prüfmustern mehr
    /// Nahtpaare (Befund 98); 5.000 N/m bringt die Bluse3 v2 nicht zur Ruhe.
    pub naht_k: f64,
    /// Höchstzahl der CG-Iterationen beim Anziehen statt
    /// [`crate::ImplizitParameter::cg_max`] (50 wie CLO). Mit 50 erreichte an
    /// Bluse3, Bluse1, Bolero und Rock keine Lösung die Toleranz 10⁻⁵, und
    /// der Rock kam nicht zur Ruhe — seine Bewegung nahm zu, und 0,2 mm
    /// andere Anfangslage entschieden, ob er hält (Befund 102). Mit 600
    /// konvergiert fast jede Lösung (Rock: 2 von 1.835 nicht, im Mittel 116
    /// Iterationen); die Grenze greift nur bei den schweren, die Rechenzeit
    /// ist kaum höher als mit 300 (dort 182 nicht konvergiert, und der Rock
    /// setzte sich in der Haltephase noch 6,7 mm).
    pub cg_max: usize,
}

/// Länge der Haltephase: 10 s bei 1/60 s je Bild (ADR-023).
pub const HALTEBILDER: usize = 600;

/// Was [`anziehen`] meldet.
#[derive(Debug, Clone, PartialEq)]
pub struct Angezogen {
    /// Vor dem Nähen aus dem Körper geschobene Punkte und ihre größte Tiefe
    /// (m, innen positiv; [`heraus_schieben`]).
    pub heraus: usize,
    pub heraus_tiefe: f64,
    /// Bilder des Einschwingens; ob es vor `einschwingen_max` ruhig wurde.
    pub einschwingbilder: usize,
    pub eingeschwungen: bool,
    /// Nach diesem Bild begann die Haltephase.
    pub halt_ab: usize,
    /// Sinken je Teil in der Haltephase (m, nach unten positiv), das
    /// stärkste zuerst ([`crate::messung::sinken_je_teil`]).
    pub sinken: Vec<(String, f64)>,
}

impl Default for Anziehen {
    /// 0,1 m/s über 200 Bilder schließt Lücken bis 33 cm, gehalten (ohne
    /// Schwerkraft), Nahtfeder 2.000 N/m, bis 600 CG-Iterationen; dann
    /// Einschwingen 2–20 s, bis kein Teil mehr als 0,25 mm/s sinkt; dann 10 s
    /// Haltephase.
    fn default() -> Self {
        Anziehen {
            naht_geschwindigkeit: 0.1,
            naehbilder: 200,
            einschwingen_min: 120,
            einschwingen_max: 1200,
            ruhe_sinken: 0.25e-3,
            haltebilder: HALTEBILDER,
            koerperpunkt_durchgaenge: 2,
            schwerkraft_naehen: 0.0,
            naht_k: 2000.0,
            cg_max: 600,
        }
    }
}


/// Abstand zur Haut, auf den [`heraus_schieben`] Stoffpunkte legt (m).
pub const HERAUS_RAND: f64 = 0.005;

/// Schiebt Stoffpunkte, die nach dem Auflegen im Körper liegen (näher als
/// ihr Kollisionsabstand), längs der Normale auf [`HERAUS_RAND`] nach außen;
/// die übrigen bleiben, wo sie sind (auch die Punkte bis 5 mm vor der Haut
/// mitzuschieben kostete an der Bluse3 v2 1,8 % der Nahtpaare). Die
/// Hüllkörper decken den Körper nicht überall: Über dem Rumpfprisma ragt die
/// Schulter heraus, am Hals das Kinn (Bluse1 v25: 35 Punkte je Seite bis
/// 11 mm tief, tiefer als der Kollider sucht). Gesucht wird bis 5 cm von der
/// Haut. Rückgabe: verschobene Punkte, größte Tiefe (m, innen positiv).
pub fn heraus_schieben(s: &mut Szene) -> (usize, f64) {
    let (mut zahl, mut tiefste) = (0usize, 0.0f64);
    for k in &s.koerper {
        let Koerper::Netz { netz, versatz, .. } = k else { continue };
        for (i, x) in s.x.iter_mut().enumerate() {
            let Some(n) = netz.naechster(sub(*x, *versatz), 0.05) else { continue };
            if n.abstand < s.kollisionsabstand[i] {
                *x = add(add(n.punkt, *versatz), mul(n.normale, s.kollisionsabstand[i] + HERAUS_RAND));
                zahl += 1;
                tiefste = tiefste.max(-n.abstand);
            }
        }
    }
    (zahl, tiefste)
}

/// Ein Rechenweg, mit dem sich anziehen lässt: was er je Phase umstellt.
/// Der implizite Weg nimmt Nahtfeder, CG-Grenze und Körperpunkt gegen
/// Stoffdreieck aus [`Anziehen`]; XPBD ([`crate::CpuReferenz`]) hat seine
/// Nahtnachgiebigkeit in der Szene und keinen Körperpunkt-Durchgang
/// (Vergleich der Löser, Befund 109).
pub trait Anziehbar: Rechenweg {
    type Vorgaben;
    /// Was vor dem Anziehen galt, zum Zurücksetzen.
    fn vorgaben(&self) -> Self::Vorgaben;
    fn setze_vorgaben(&mut self, v: Self::Vorgaben);
    /// Vor jedem Bild: nähen oder ruhen.
    fn phase(&mut self, a: &Anziehen, naehen: bool);
}

impl Anziehbar for ImplizitCg {
    type Vorgaben = crate::implizit::ImplizitParameter;
    fn vorgaben(&self) -> Self::Vorgaben {
        self.param
    }
    fn setze_vorgaben(&mut self, v: Self::Vorgaben) {
        self.param = v;
    }
    fn phase(&mut self, a: &Anziehen, naehen: bool) {
        self.param.naht_k = a.naht_k;
        self.param.cg_max = a.cg_max;
        self.param.koerperpunkt_durchgaenge = if naehen { a.koerperpunkt_durchgaenge } else { 0 };
    }
}

impl Anziehbar for crate::cpu::CpuReferenz {
    type Vorgaben = ();
    fn vorgaben(&self) {}
    fn setze_vorgaben(&mut self, _: ()) {}
    fn phase(&mut self, _: &Anziehen, _: bool) {}
}

/// Zieht die platzierten Teile an: erst Stoffpunkte aus dem Körper schieben
/// ([`heraus_schieben`]), dann nähen mit begrenzter Geschwindigkeit und
/// Körperpunkt gegen Stoffdreieck, dann mit voller Schwerkraft einschwingen,
/// bis kein Teil mehr sinkt, dann die Haltephase (ADR-023). `nach_bild` sieht
/// jedes Bild (Nummer ab 1, Szene, Rechenweg).
pub fn anziehen<R: Anziehbar>(s: &mut Szene, rw: &mut R, a: &Anziehen, nach_bild: &mut dyn FnMut(usize, &Szene, &R)) -> Angezogen {
    let (heraus, heraus_tiefe) = heraus_schieben(s);
    let vorher = rw.vorgaben();
    let g = s.param.schwerkraft;
    s.naht_offen();
    let mut bild = 0;
    let mut schritt = |s: &mut Szene, rw: &mut R, naehen: bool| {
        bild += 1;
        rw.phase(a, naehen);
        s.param.schwerkraft = if naehen { mul(g, a.schwerkraft_naehen) } else { g };
        s.naht_kuerzen(a.naht_geschwindigkeit * s.param.bild_dt);
        rw.bild(s);
        nach_bild(bild, s, rw);
        bild
    };
    for _ in 0..a.naehbilder {
        schritt(s, rw, true);
    }
    // Einschwingen: je Sekunde die Höhenänderung der Teile prüfen, im
    // Betrag — ein Teil, das zurückfedert, ist nicht ruhig (der Rock v36 galt
    // sonst nach 4 s als ruhig und sank danach 7,9 mm). Zwei ruhige Sekunden
    // hintereinander.
    let je_sekunde = (1.0 / s.param.bild_dt).round().max(1.0) as usize;
    let (mut einschwingbilder, mut eingeschwungen, mut ruhig) = (0, false, 0);
    let mut probe = s.x.clone();
    while einschwingbilder < a.einschwingen_max {
        schritt(s, rw, false);
        einschwingbilder += 1;
        if einschwingbilder % je_sekunde == 0 {
            let bewegt = crate::messung::sinken_je_teil(s, &probe).iter().map(|x| x.1.abs()).fold(0.0, f64::max);
            probe.clone_from(&s.x);
            ruhig = if bewegt <= a.ruhe_sinken * je_sekunde as f64 * s.param.bild_dt { ruhig + 1 } else { 0 };
            if einschwingbilder >= a.einschwingen_min && ruhig >= 2 {
                eingeschwungen = true;
                break;
            }
        }
    }
    let halt_ab = a.naehbilder + einschwingbilder;
    let x0 = s.x.clone();
    for _ in 0..a.haltebilder {
        schritt(s, rw, false);
    }
    rw.setze_vorgaben(vorher);
    s.param.schwerkraft = g;
    Angezogen { heraus, heraus_tiefe, einschwingbilder, eingeschwungen, halt_ab, sinken: crate::messung::sinken_je_teil(s, &x0) }
}
