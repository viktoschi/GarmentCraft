//! Messen am Körper.
//!
//! Ein Dreiecksnetz und eine waagerechte Ebene. Was die Ebene schneidet,
//! sind geschlossene Schleifen: eine um den Rumpf, eine je Arm, eine je
//! Bein. Der Umfang des Rumpfs ist die Antwort — und die braucht das Projekt
//! an zwei Stellen:
//!
//! * als **Abnahme** der Körperformung: wird der Avatar aus den Massen
//!   geformt, muss der gemessene Umfang zurück den Zielwert treffen;
//! * als **Eingang** der Simulation: der Stoff liegt auf dieser Fläche.
//!
//! Drei Ansätze sind hier schon gescheitert und stehen deshalb nicht wieder
//! hier: alle Schnittpunkte einer Ebene addieren (zählt Arme und Beine mit),
//! die Schnittstücke zu Schleifen verketten (die Verkettung hing), und
//! Strahlen von der Achse nach aussen (trifft ein grobes Netz nicht zuver-
//! lässig). Der Weg hier ist derselbe wie beim zweiten, nur richtig: die
//! Schnittstücke zu Schleifen verketten und dann diejenige nehmen, die die
//! Körperachse umschliesst.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub mod anordnung_clo;
pub mod anordnung_netz;
pub mod anpassen;
pub mod bogenmasse;
pub mod dualquat;
pub mod formung;
pub mod kette;
pub mod kette_referenz;
pub mod koerper;
pub mod koerper_quelle;
pub mod massband;
pub mod messung;
pub mod messung_referenz;
pub mod nachformung;
pub mod nacharbeit_referenz;
pub mod person;
pub mod ringecken;
pub mod silhouette;
pub mod skelett;
pub mod spur;

/// Ein Punkt im Raum. Millimeter, y nach oben — die Konvention von
/// `klon3d-pattern` bis `Achse::in_szenene` umgedreht.
pub type Punkt3 = [f64; 3];

/// Ein Dreiecksnetz mit vorderer Seite nach aussen.
#[derive(Clone, Debug, Default)]
pub struct Netz {
    pub punkte: Vec<Punkt3>,
    pub dreiecke: Vec<[usize; 3]>,
}

impl Netz {
    /// Kleinste und grösste y-Koordinate, in derselben Einheit wie die Punkte.
    pub fn hoehe(&self) -> Option<(f64, f64)> {
        if self.punkte.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = (self.punkte[0][1], self.punkte[0][1]);
        for p in &self.punkte {
            if p[1] < lo {
                lo = p[1];
            }
            if p[1] > hi {
                hi = p[1];
            }
        }
        Some((lo, hi))
    }

    /// Zylinder um die Achse `x = z = 0`, als geschlossenes Netz.
    ///
    /// Nur für die Prüfung: ein Körper unbekannter Form hat keinen bekannten
    /// Umfang, ein Zylinder schon — `2 π r`.
    #[cfg(test)]
    pub fn zylinder(radius: f64, hoehe: f64, segmente: usize, ringe: usize) -> Netz {
        let mut n = Netz::default();
        for r in 0..=ringe {
            let y = hoehe * r as f64 / ringe as f64;
            for s in 0..segmente {
                let w = 2.0 * std::f64::consts::PI * s as f64 / segmente as f64;
                n.punkte.push([radius * w.cos(), y, radius * w.sin()]);
            }
        }
        for r in 0..ringe {
            for s in 0..segmente {
                let s1 = (s + 1) % segmente;
                let a = r * segmente + s;
                let b = r * segmente + s1;
                let c = (r + 1) * segmente + s;
                let d = (r + 1) * segmente + s1;
                n.dreiecke.push([a, c, b]);
                n.dreiecke.push([b, c, d]);
            }
        }
        n
    }
}

/// Liest ein OBJ als Dreiecksnetz.
///
/// `skala` macht aus den Dateieinheiten Millimeter: CLOs `BaseMesh.obj`
/// steht in Zentimetern (`skala = 10`), `assets/koerper.obj` in
/// Millimetern (`skala = 1`). Nur `v` und `f` werden gelesen; `vt`, `vn`,
/// `mtllib` und Kommentare fallen weg. Polygone werden als Fächer
/// trianguliert, negative Indizes sind erlaubt.
pub fn lies_obj(pfad: &Path, skala: f64) -> io::Result<Netz> {
    let text = fs::read_to_string(pfad)?;
    let mut netz = Netz::default();
    for (nr, zeile) in text.lines().enumerate() {
        let mut teile = zeile.split_whitespace();
        match teile.next() {
            Some("v") => {
                let mut p = [0.0f64; 3];
                for a in 0..3 {
                    let t = teile.next().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("{}:{}: Punkt zu kurz", pfad.display(), nr + 1),
                        )
                    })?;
                    p[a] = t.parse::<f64>().map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("{}:{}: {e}", pfad.display(), nr + 1),
                        )
                    })? * skala;
                }
                netz.punkte.push(p);
            }
            Some("f") => {
                let mut idx = Vec::new();
                for t in teile {
                    let roh: i64 = t
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse()
                        .map_err(|e| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("{}:{}: {e}", pfad.display(), nr + 1),
                            )
                        })?;
                    let i = if roh < 0 {
                        netz.punkte.len() as i64 + roh
                    } else {
                        roh - 1
                    };
                    if i < 0 || i as usize >= netz.punkte.len() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "{}:{}: Punktindex {roh} ausserhalb von {}",
                                pfad.display(),
                                nr + 1,
                                netz.punkte.len()
                            ),
                        ));
                    }
                    idx.push(i as usize);
                }
                for k in 1..idx.len().saturating_sub(1) {
                    netz.dreiecke.push([idx[0], idx[k], idx[k + 1]]);
                }
            }
            _ => {}
        }
    }
    Ok(netz)
}

/// Schreibt ein Netz als OBJ, Koordinaten durch `skala` geteilt.
pub fn schreibe_obj(pfad: &Path, netz: &Netz, skala: f64) -> io::Result<()> {
    let mut f = io::BufWriter::new(fs::File::create(pfad)?);
    writeln!(f, "# klon3d: {} Punkte, {} Dreiecke", netz.punkte.len(), netz.dreiecke.len())?;
    for p in &netz.punkte {
        writeln!(
            f,
            "v {:.6} {:.6} {:.6}",
            p[0] / skala,
            p[1] / skala,
            p[2] / skala
        )?;
    }
    for d in &netz.dreiecke {
        writeln!(f, "f {} {} {}", d[0] + 1, d[1] + 1, d[2] + 1)?;
    }
    f.flush()
}

/// Ein geschlossener Querschnitt mit seiner Länge.
#[derive(Clone, Debug)]
pub struct Querschnitt {
    /// Länge des Umrisses in derselben Einheit wie das Netz.
    pub laenge: f64,
    /// Die Ringpunkte in Reihenfolge, in der Ebene `y = hoehe`, als `(x, z)`.
    pub ring: Vec<(f64, f64)>,
    /// Höhe der Ebene.
    pub hoehe: f64,
}

/// Raster für die Punkte eines Schnitts, in Millimeter.
///
/// Zwei Dreiecke, die dieselbe Kante teilen, rechnen dieselbe Kreuzung von
/// verschiedenen Seiten: die beiden Ergebnisse unterscheiden sich um etwa
/// `1e-13` der Koordinatengrösse. Wer sie verketten will, muss sie vorher
/// zusammenfassen. `1e-3` mm ist um sieben Grössenordnungen feiner als der
/// Abstand zweier Netzpunkte und kann deshalb nichts zusammenfassen, was
/// verschieden ist.
const RASTER: f64 = 1.0e-3;

fn raster(p: (f64, f64)) -> (i64, i64) {
    (
        (p.0 / RASTER).round() as i64,
        (p.1 / RASTER).round() as i64,
    )
}

/// Grösster Spalt, der als Naht geschlossen wird, in Millimetern.
///
/// Das CLO-Basismesh ist an den Nähten offen (Befund 25): der Rumpfschnitt
/// ist dann eine offene Kette, deren Enden 0,02–1,25 cm auseinanderliegen.
/// Ein Spalt bis 2 cm wird als Naht gelesen und geschlossen; alles darüber
/// ist eine echte Lücke (z. B. der Halsausschnitt mit 12,8 cm).
const MAX_SPALT: f64 = 20.0;

/// Richtung vom Punkt `p` zum anderen Ende des Stücks.
fn richtung_ab(s: &((f64, f64), (f64, f64)), p: (f64, f64)) -> (f64, f64) {
    let q = if raster(s.0) == raster(p) { s.1 } else { s.0 };
    (q.0 - p.0, q.1 - p.1)
}

/// Querschnitt des Rumpfs auf der Höhe `y`.
///
/// Gesucht ist die geschlossene Schleife, die die Körperachse `x = z = 0`
/// umschliesst. `Ok(None)`, wenn keine Schleife sie umschliesst — auf einem
/// Körper ohne Rumpf, oder wenn der Ausschnitt durch eine Lücke geht.
pub fn rumpf(netz: &Netz, y: f64) -> Option<Querschnitt> {
    // Die Ebene wird um einen Hauch verschoben. Liegt sie genau auf einem
    // Netzring — und das tut sie bei einem Zylinder mit vier Ringen auf
    // halber Höhe —, dann fallen für ein Dreieck beide Kreuzungen auf
    // denselben Punkt: das Stück hätte die Länge null und die Verkettung
    // risse auf. Ein Versatz von einem Milliardstel der Körperhöhe ist
    // sieben Grössenordnungen unter der Messgenauigkeit und macht die
    // Schnittlage dafür eindeutig.
    let (lo, hi) = netz.hoehe()?;
    let y = y + 1.0e-9 * (hi - lo).max(1.0);

    // 1. Jedes Dreieck liefert ein Stück des Querschnitts.
    let mut stuecke: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for &[a, b, c] in &netz.dreiecke {
        let mut treffer: Vec<(f64, f64)> = Vec::with_capacity(2);
        for e in 0..3 {
            let p = netz.punkte[[a, b, c][e]];
            let q = netz.punkte[[a, b, c][(e + 1) % 3]];
            if (p[1] - y) * (q[1] - y) > 0.0 {
                continue;
            }
            if p[1] == q[1] {
                continue;
            }
            let t = (y - p[1]) / (q[1] - p[1]);
            treffer.push((p[0] + t * (q[0] - p[0]), p[2] + t * (q[2] - p[2])));
        }
        if treffer.len() == 2 {
            stuecke.push((treffer[0], treffer[1]));
        }
    }
    if stuecke.is_empty() {
        return None;
    }

    // 2. Endpunkte verketten. In einem geschlossenen Netz hängt an jedem
    //    Endpunkt genau ein weiteres Stück. Das Basismesh ist aber an den
    //    Nähten offen (Befund 25): dann bleibt eine Kette offen und ihr
    //    Schlussstück fehlt. Beides wird hier zu einer Kette verarbeitet.
    let mut an_endpunkt: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, s) in stuecke.iter().enumerate() {
        an_endpunkt.entry(raster(s.0)).or_default().push(i);
        an_endpunkt.entry(raster(s.1)).or_default().push(i);
    }

    let mut benutzt = vec![false; stuecke.len()];
    // Zwei Arten von Rumpfring, zwei Kandidaten: der Ring um die Körperachse
    // (Ursprung der Ebene) und - wenn es keinen gibt - der Ring um den eigenen
    // Flächenschwerpunkt. Siehe [`rumpf_ring`].
    let mut best: Option<(f64, Vec<(f64, f64)>)> = None;
    let mut best_mitte: Option<(f64, Vec<(f64, f64)>)> = None;

    for start in 0..stuecke.len() {
        if benutzt[start] {
            continue;
        }
        benutzt[start] = true;

        // Vom Austrittspunkt des Startstücks dem Netz nachlaufen. Jeder
        // Austrittspunkt ist zugleich Eintrittspunkt des nächsten Stücks.
        let mut kette = vec![start];
        let mut vorher = stuecke[start].0;
        let mut punkt = stuecke[start].1;

        loop {
            let kandidaten: Vec<usize> = an_endpunkt
                .get(&raster(punkt))
                .map(|k| k.iter().copied().filter(|j| !benutzt[*j]).collect())
                .unwrap_or_default();
            let weiter = match kandidaten.len() {
                0 => break,
                1 => kandidaten[0],
                _ => {
                    // Mehrere Stücke treffen sich in einem Punkt: an einer
                    // Naht oder an einer Netzkante in der Ebene. Nimm das,
                    // das die Richtung am besten fortsetzt — sonst springt
                    // die Verkettung auf einen anderen Ring.
                    let ein = (punkt.0 - vorher.0, punkt.1 - vorher.1);
                    kandidaten
                        .into_iter()
                        .max_by(|&i, &j| {
                            let di = richtung_ab(&stuecke[i], punkt);
                            let dj = richtung_ab(&stuecke[j], punkt);
                            let si = ein.0 * di.0 + ein.1 * di.1;
                            let sj = ein.0 * dj.0 + ein.1 * dj.1;
                            si.partial_cmp(&sj).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .unwrap()
                }
            };
            benutzt[weiter] = true;
            let (p, q) = stuecke[weiter];
            let naechster = if raster(p) == raster(punkt) { q } else { p };
            vorher = punkt;
            punkt = naechster;
            kette.push(weiter);
        }

        // Punkte der Kette: Anfangspunkt, dann die Austrittspunkte.
        let mut punkte = Vec::with_capacity(kette.len() + 1);
        punkte.push(stuecke[kette[0]].0);
        for &i in &kette {
            punkte.push(stuecke[i].1);
        }

        let geschlossen = raster(punkte[punkte.len() - 1]) == raster(punkte[0]);
        if !geschlossen {
            let (a, b) = (punkte[punkte.len() - 1], punkte[0]);
            let spalt = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            if spalt > MAX_SPALT {
                continue;
            }
        }

        if !umschliesst(&punkte, (0.0, 0.0)) {
            // Zweiter Kandidat: derselbe Ring um den eigenen Schwerpunkt.
            // Bei MakeHuman liegt das Becken hinter `z = 0` (der Körper ist
            // über den Mittelwert aller Punkte zentriert, nicht über das
            // Becken), bei 77 cm liegt der Ursprung sogar **vor** dem Körper:
            // vorderster Punkt z = +1,58 gegen eine Beckenmitte bei z = −99.
            // Der Ring ist trotzdem ein sauberer Rumpfquerschnitt - gemessen
            // wird auf seiner Hülle, und die liegt um den eigenen Schwerpunkt.
            // Ohne diesen Kandidaten ist die Hüfte unter 83 cm nicht messbar,
            // obwohl der Schnitt dort einwandfrei ist.
            //
            // Die Körpermitte in `x` ist dagegen bekannt und steht nicht zur
            // Debatte: der Körper ist spiegelsymmetrisch aufgebaut. Der Ring
            // muss also die Achse `x = 0` in seiner eigenen Tiefe umschliessen.
            // Das ist die Bedingung, die einen Oberschenkel aussortiert - er
            // liegt ganz auf einer Seite und umschliesst `x = 0` nie.
            let m = flaechen_mitte(&punkte);
            if umschliesst(&punkte, (0.0, m.1)) && umschliesst(&punkte, m) {
                let laenge = ringlaenge(&punkte);
                if best_mitte.as_ref().is_none_or(|(l, _)| laenge > *l) {
                    best_mitte = Some((laenge, punkte));
                }
            }
            continue;
        }

        let laenge = ringlaenge(&punkte);

        if best.as_ref().is_none_or(|(l, _)| laenge > *l) {
            best = Some((laenge, punkte));
        }
    }

    best.or(best_mitte)
        .map(|(laenge, ring)| Querschnitt { laenge, ring, hoehe: y })
}

/// Länge der geschlossenen Kette, letzter Punkt zum ersten.
fn ringlaenge(ring: &[(f64, f64)]) -> f64 {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(p, q)| ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt())
        .sum()
}

/// Flächenschwerpunkt einer geschlossenen Punktfolge (x, z), in derselben
/// Einheit wie die Punkte.
///
/// Bei einem entarteten Ring (Fläche null) die Mitte der Bounding-Box: dann
/// ist ohnehin nichts zu messen, aber der Rückfall auf `(0, 0)` würde einen
/// Ring, der gar nicht um den Ursprung liegt, stillschweigend verwerfen.
fn flaechen_mitte(ring: &[(f64, f64)]) -> (f64, f64) {
    let mut a = 0.0;
    let mut cx = 0.0;
    let mut cz = 0.0;
    for i in 0..ring.len() {
        let p = ring[i];
        let q = ring[(i + 1) % ring.len()];
        let k = p.0 * q.1 - q.0 * p.1;
        a += k;
        cx += (p.0 + q.0) * k;
        cz += (p.1 + q.1) * k;
    }
    if a.abs() > 1.0e-12 * ring.len() as f64 {
        return (cx / (3.0 * a), cz / (3.0 * a));
    }
    let mut lo = ring[0];
    let mut hi = ring[0];
    for p in ring {
        lo = (lo.0.min(p.0), lo.1.min(p.1));
        hi = (hi.0.max(p.0), hi.1.max(p.1));
    }
    (0.5 * (lo.0 + hi.0), 0.5 * (lo.1 + hi.1))
}

/// Liegt `punkt` im Inneren des Rings?
///
/// Strahl von `punkt` nach `+x`, nach jedem Schnitt mit einem Randstück
/// einmal zählen; ungerade heisst innen.
fn umschliesst(ring: &[(f64, f64)], punkt: (f64, f64)) -> bool {
    let mut kreuzungen = 0;
    for i in 0..ring.len() {
        let (x1, z1) = (ring[i].0 - punkt.0, ring[i].1 - punkt.1);
        let (x2, z2) = (
            ring[(i + 1) % ring.len()].0 - punkt.0,
            ring[(i + 1) % ring.len()].1 - punkt.1,
        );
        if (z1 > 0.0) != (z2 > 0.0) {
            let t = -z1 / (z2 - z1);
            if x1 + t * (x2 - x1) > 0.0 {
                kreuzungen += 1;
            }
        }
    }
    kreuzungen % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Umfang des einbeschriebenen `n`-Ecks, nicht des Kreises.
    ///
    /// Ein Netz aus `n` Segmenten kann den Kreis nicht genauer treffen: die
    /// Sehne ist kürzer als der Bogen, um `(π/n)²/2`. Wer gegen `2 π r`
    /// prüft, prüft die Auflösung des Netzes und nicht die Rechnung — genau
    /// das ist hier zweimal als Fehlschlag aufgetaucht. Geprüft wird
    /// deshalb gegen den Wert, den das Netz hergibt; die Toleranz darf dann
    /// eng sein.
    fn vieleck(r: f64, n: usize) -> f64 {
        n as f64 * 2.0 * r * (std::f64::consts::PI / n as f64).sin()
    }

    /// Ein Zylinder hat den Umfang seines Vielecks — daran ist die Messung zu
    /// prüfen, bevor sie an einen Körper gestellt wird, von dem niemand die
    /// Antwort weiss.
    #[test]
    fn zylinder_hat_seinen_vielecksumfang() {
        for (r, n) in [(10.0, 720), (100.0, 720), (147.5, 720)] {
            let netz = Netz::zylinder(r, 400.0, n, 4);
            let q = rumpf(&netz, 200.0).expect("Zylinder wird geschnitten");
            let soll = vieleck(r, n);
            assert!(
                (q.laenge - soll).abs() < soll * 1e-12,
                "r = {r}: {} statt {soll}",
                q.laenge
            );
        }
    }

    /// Auch bei grobem Netz: der Fehler ist von der Auflösung bestimmt und
    /// bleibt unter einem Prozent.
    #[test]
    fn grobes_netz_bleibt_unter_einem_prozent() {
        let netz = Netz::zylinder(147.0, 400.0, 24, 2);
        let q = rumpf(&netz, 200.0).unwrap();
        let soll = vieleck(147.0, 24);
        let fehler = (q.laenge - soll).abs() / soll;
        assert!(fehler < 1e-9, "Fehler {fehler:.4} bei 24 Segmenten");
    }

    /// Zwischen zwei Ringhöhen liegt kein Schnitt, ausser der Ebene selbst.
    #[test]
    fn ausserhalb_des_koerpers_kein_schnitt() {
        let n = Netz::zylinder(100.0, 400.0, 64, 2);
        assert!(rumpf(&n, 900.0).is_none());
    }

    /// Zylinder mit einem senkrechten Schlitz: `breite` Segmente fehlen.
    ///
    /// So sieht der Rumpfschnitt eines Netzes aus, das an einer Naht offen
    /// ist (Befund 25) — die Kette ist da, nur ihr Schlussstück fehlt.
    fn zylinder_mit_schlitz(
        radius: f64,
        hoehe: f64,
        segmente: usize,
        ringe: usize,
        breite: usize,
    ) -> Netz {
        let voll = Netz::zylinder(radius, hoehe, segmente, ringe);
        let mut n = Netz::default();
        n.punkte = voll.punkte;
        for r in 0..ringe {
            for s in 0..segmente {
                if s < breite {
                    continue;
                }
                let i = (r * segmente + s) * 2;
                n.dreiecke.push(voll.dreiecke[i]);
                n.dreiecke.push(voll.dreiecke[i + 1]);
            }
        }
        n
    }

    /// Ein offener Schnitt mit kleinem Spalt ist eine Naht und wird
    /// geschlossen. Die Länge bleibt der volle Umfang, weil das Schlussstück
    /// genau die fehlende Sehne ist.
    #[test]
    fn offene_naht_wird_geschlossen() {
        let (r, n) = (100.0, 720);
        let netz = zylinder_mit_schlitz(r, 400.0, n, 4, 1);
        let q = rumpf(&netz, 200.0).expect("Schlitz ist eine Naht");
        let soll = vieleck(r, n);
        assert!(
            (q.laenge - soll).abs() < soll * 1e-12,
            "{} statt {soll} — der Spalt wurde nicht geschlossen",
            q.laenge
        );
    }

    /// Ein Spalt über 2 cm ist keine Naht mehr, sondern eine Lücke.
    #[test]
    fn zu_grosser_spalt_ist_keine_naht() {
        let netz = zylinder_mit_schlitz(100.0, 400.0, 720, 4, 30);
        assert!(
            rumpf(&netz, 200.0).is_none(),
            "26 mm Spalt darf nicht als Naht gelesen werden"
        );
    }

    /// Zwei Zylinder nebeneinander: nur der um die Achse zählt. Ohne diese
    /// Prüfung hätte die alte Methode beide Umfänge addiert.
    #[test]
    fn nur_der_rumpf_zaehlt() {
        let mut netz = Netz::zylinder(150.0, 400.0, 720, 4);
        // Arme links und rechts, frei stehend: Rumpfrand bei 150, Arm von
        // 320 bis 460. Ein Arm, der den Rumpf berührt, wäre kein zweiter
        // Körper und die Prüfung wertlos.
        for versatz in [-390.0f64, 390.0] {
            let arm = Netz::zylinder(70.0, 400.0, 360, 2);
            let basis = netz.punkte.len();
            for p in &arm.punkte {
                netz.punkte.push([p[0] + versatz, p[1], p[2]]);
            }
            for d in &arm.dreiecke {
                netz.dreiecke.push([d[0] + basis, d[1] + basis, d[2] + basis]);
            }
        }
        let q = rumpf(&netz, 200.0).expect("Rumpf wird geschnitten");
        let soll = vieleck(150.0, 720);
        assert!(
            (q.laenge - soll).abs() < soll * 1e-12,
            "{} statt {soll} — die Arme sind mitgezaehlt",
            q.laenge
        );
    }
}
