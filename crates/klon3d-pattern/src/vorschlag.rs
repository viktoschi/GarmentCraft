//! Nahtvorschlag: welche Kanten zusammengehören, aus dem Muster allein.
//!
//! Seamly2D speichert keine Nähte (`work/naehte/signale.md`: kein Feld eines
//! Teils verweist auf ein anderes). Der Vorschlag erschließt sie aus dem, was
//! das Muster trägt — nach der Regelkette dieser Untersuchung, auf den
//! Randkanten von [`crate::kanten`]:
//!
//! 1. **Ketten:** zusammenhängende nähbare Randkanten eines Teils von Ecke zu
//!    Ecke (Knick ab 25°). Eine Kante mit Zugabe 0 an beiden Enden ist frei —
//!    aber nur in einem Teil, das überhaupt Zugaben trägt; ein Teil ganz ohne
//!    (Bustier-Rumpf) sagt darüber nichts.
//! 2. **Kandidaten:** zwei Ketten gleicher Länge (bis 1 mm) mit gleicher
//!    Kerbenfolge (bis 1,5 mm) in einer der beiden Richtungen, auch zwei
//!    Ketten desselben Teils (Ärmelnaht).
//! 3. **Auswahl eins zu eins:** gemeinsame Endpunkte (derselbe Rechenpunkt)
//!    zuerst, dann mehr Kerben, dann kleinere Abweichung; bei Gleichstand die
//!    Nähe in der Zeichnung und gleiche Seite im Teilnamen (links/rechts).
//! 4. **Zusammengesetzte Seiten:** An den Enden gefundener Nähte stoßen Teile
//!    aneinander; die freien Ketten, die dort beginnen oder enden, werden
//!    verbunden (Schulter über zwei Teile, Armloch, Seitennaht über drei) und
//!    wie eine Kette gepaart, bis nichts mehr dazukommt.
//! 5. **Schlitze:** Ein Schlitz steht nicht im Muster — er trägt dieselbe
//!    Zugabe und dieselben Kerben wie die Naht darüber; nur der Name seines
//!    Punktes sagt, dass dort ein Ende ist (`KN…` wie Knie, `…Schlitz`,
//!    `SLIT…`). Eine Kette, die an einem solchen Punkt in eine freie Kante
//!    ausläuft, ist die Schlitzkante und wird nicht genäht. Geteilt wird nur,
//!    wenn genau ein Kettenende am Saum liegt: ein Bund führt denselben Namen
//!    als Marke mitten in der Naht und bleibt am Stück (Rock v36). Die Regel
//!    ist eine Annahme über die Punktnamen, keine Messung; heißt ein Schlitz
//!    anders, wird er weiter zugenäht.
//!
//! **Grenze: Falten.** Ein Pleat (Bolero: sieben Kugel- und sechs Saumfalten je
//! Ärmel) steht im Muster wie eine Kerbe: drei Punktknoten `A M B` auf dem
//! Rand, die gefaltete Naht `A–M` auf `B–M`. Er ist **nicht** geometrisch zu
//! erkennen, gemessen am Bolero v37 über alle sechs Prüfmuster und 26
//! Sollfalten an beiden Ärmeln (12.11.2026):
//!
//! * **Keine Ecke:** der Knick an `M` liegt bei 0,26°–7,15° (das Muster baut
//!   ihn als `cutSpline` auf einer Kurve), eine Ecke verlangt 25°. Der Test
//!   findet die Falte also nicht — er findet an dieser Stelle auch nichts
//!   Falsches, weil die Falten in der einen 61,80-cm-Kette des Ärmels
//!   verschwinden.
//! * **Keine Kollinearität:** `A`, `M`, `B` liegen auf 0,15–2,46 mm einer
//!   Geraden — aber die Manschettenmarke `MM` (0,00 mm), die Armlochkerbe
//!   `FSM` (0,26 mm) und die Kerben `KF7B`/`KF1Ar` (2,94/2,96 mm) liegen
//!   genauso. Bei 3 mm Toleranz sind alle 26 Falten getroffen und 6 Fremde
//!   dazu; bei 0,3 mm 14 Falten und 4 Fremde.
//! * **Keine Periode:** „zwei kurze, gleich lange Stücke zwischen zwei
//!   längeren" trifft 22 der 26 Falten und die Marke `MM` (20,03/20,03 mm
//!   zwischen 40,54/40,33) — `KF1M` und `KF7M` fallen heraus, weil ihre
//!   Nachbarn selbst Faltenstücke sind. Mit einer Zone aus mindestens drei
//!   Kandidaten sind es 26 Falten und 2 Fremde (`KF7B`, `KF1Ar`).
//!
//! Deshalb bleibt der Bolero beim Vorschlag aus dem Muster allein bei 5
//! Nähten; Falten brauchen eine Angabe im Muster (Name oder Marke), nicht
//! eine weitere Schwelle. Die 182 Kanten der Handliste stehen in
//! `tests/muster/hexen_2026_10_27/bolero_v37.naehte`.
//!
//! Die Richtung folgt den Kerben oder gemeinsamen Punkten; passen beide$
//! Richtungen, die, in der die Enden relativ zur Mitte wie gezeichnet liegen.
//! Ergebnis ist eine Nahtliste wie von Hand (`klon3d naehte --vorschlag`),
//! zum Durchsehen gedacht.

use std::collections::HashSet;

use crate::geom::{Auswertung, GeomError, Pt};
use crate::kanten::{Kurvenquelle, Teilrand};
use crate::naehte::{Abschnittsbezug, Nahtdef};

/// Knick, ab dem ein Punktknoten eine Ecke ist (Grad).
pub const ECKE_GRAD: f64 = 25.0;
/// Längen gleich bis (mm).
pub const LAENGE_MM: f64 = 1.0;
/// So weit darf eine Kette an einem Innenpunkt gekürzt werden (mm): Kragen
/// und Bund stehen an den Enden um eine Überlappung über (Bluse1: 1 cm).
pub const KUERZEN_MM: f64 = 20.0;
/// Kerben gegenüber bis (mm), in der ersten Runde.
pub const KERBE_MM: f64 = 1.5;
/// Kerben gegenüber bis (mm) ab der zweiten Runde; dann darf eine Kerbe
/// ohne Gegenstück bleiben (Bluse3: an der Seitennaht liegen die Kerben bis
/// 7,4 mm auseinander, eine hat keines; `klon3d naehte` fängt bis 1 cm).
pub const KERBE_LOSE_MM: f64 = 10.0;

/// Eine Kette: Randkanten eines Teils von Punktknoten `von` vorwärts bis `bis`.
#[derive(Debug, Clone)]
struct Kette {
    teil: usize,
    von: usize,
    bis: usize,
    laenge: f64,
    /// Abstände der Kerben im Inneren vom Anfang (Mustereinheiten).
    kerben: Vec<f64>,
    /// Gerade bis auf 2 mm (Spiegelachsen wie die hintere Mitte sind gerade).
    gerade: bool,
    /// Der ganze Umriss ohne Ecke (Scheiben, Donuts): wird nicht gepaart.
    ganz: bool,
    /// Zugabe 0 an allen Kanten, in einem Teil, das Zugaben trägt.
    frei: bool,
    /// Schlitzkante: beginnt an einem Schlitzpunkt und läuft in eine freie
    /// Kante aus. Wird nicht genäht (siehe [`ist_schlitzname`]).
    oeffnung: bool,
    /// Punktknoten im Inneren mit ihrem Abstand vom Anfang.
    innen: Vec<(usize, f64)>,
    /// Randkanten der Kette.
    kanten: Vec<usize>,
}

/// Eine Seite: Ketten hintereinander, je vorwärts (`false`) oder rückwärts.
#[derive(Debug, Clone)]
struct Seite {
    stuecke: Vec<(usize, bool)>,
}

/// Der Vorschlag.
#[derive(Debug, Clone)]
pub struct Vorschlag {
    pub naehte: Vec<Nahtdef>,
    /// Ketten, die keinen Partner fanden: (Teil, von, bis, Länge in cm).
    pub allein: Vec<(String, String, String, f64)>,
    /// Teile, die nach dem Nahtgraphen von innen gezeichnet sind (siehe
    /// [`verdrehte_naehte`]); wo die Nähte nichts entscheiden, gilt „von
    /// außen gezeichnet“ für die Mehrheit der Fläche.
    pub gespiegelt: Vec<String>,
    /// Nähte, deren Richtung keiner Spiegelung genügt (verdreht).
    pub widersprueche: usize,
}

/// Vereinigung mit Parität über Teile: f(x) = par(x) · f(Wurzel), f = +1
/// von außen gezeichnet, −1 von innen.
struct Paritaet {
    eltern: Vec<usize>,
    par: Vec<f64>,
}

impl Paritaet {
    fn neu(n: usize) -> Self {
        Paritaet { eltern: (0..n).collect(), par: vec![1.0; n] }
    }

    fn wurzel(&mut self, x: usize) -> (usize, f64) {
        if self.eltern[x] == x {
            return (x, 1.0);
        }
        let (r, p) = self.wurzel(self.eltern[x]);
        self.eltern[x] = r;
        self.par[x] *= p;
        (r, self.par[x])
    }

    /// Verlangt f(x) · f(y) = `rel`; `false` bei Widerspruch.
    fn verbinde(&mut self, x: usize, y: usize, rel: f64) -> bool {
        let ((rx, px), (ry, py)) = (self.wurzel(x), self.wurzel(y));
        if rx == ry {
            return px * py == rel;
        }
        self.eltern[ry] = rx;
        self.par[ry] = px * py * rel;
        true
    }
}

fn richtung(p: &[Pt], am_anfang: bool, min: f64) -> Option<(f64, f64)> {
    // Richtung über mindestens `min` Länge, damit kurze Stücke an Kurven nicht täuschen.
    let n = p.len();
    if n < 2 {
        return None;
    }
    let (a, mut i) = if am_anfang { (p[0], 1) } else { (p[n - 1], n - 2) };
    loop {
        let b = p[i];
        if a.dist(b) >= min || (am_anfang && i == n - 1) || (!am_anfang && i == 0) {
            let (dx, dy) = if am_anfang { (b.x - a.x, b.y - a.y) } else { (a.x - b.x, a.y - b.y) };
            let l = (dx * dx + dy * dy).sqrt();
            return (l > 0.0).then(|| (dx / l, dy / l));
        }
        if am_anfang {
            i += 1;
        } else {
            i -= 1;
        }
    }
}

/// Schlitzpunkt nach dem Punktnamen: `KN_CF` (Knie, wo der Schlitz beginnt),
/// `TL1_Schlitz`, `SLIT…`. Der Schlitz selbst steht nicht im Muster — er trägt
/// dieselbe Zugabe und dieselbe Kerbenart wie die Naht darüber; nur der Name
/// sagt, dass dort ein Ende ist. Geprüft an den Mustern mit Schlitz
/// (`rock_v36`, `rock_2schlitze_v34`).
pub(crate) fn ist_schlitzname(name: &str) -> bool {
    let n = name.to_lowercase();
    n.split(|c: char| !c.is_alphanumeric()).any(|w| matches!(w, "kn" | "slit" | "schlitz" | "schlitze"))
}

pub(crate) fn seitenwort(name: &str) -> Option<bool> {
    let n = name.to_lowercase();
    let w: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).collect();
    let links = w.iter().any(|s| matches!(*s, "links" | "left" | "l"));
    let rechts = w.iter().any(|s| matches!(*s, "rechts" | "right" | "r"));
    match (links, rechts) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Vorn (`true`) oder hinten nach dem Teilnamen.
pub(crate) fn zonenwort(name: &str) -> Option<bool> {
    let n = name.to_lowercase();
    let vorn = ["vorn", "vorder", "front"].iter().any(|w| n.contains(w));
    let hinten = ["rück", "rueck", "ruck", "hinten", "back"].iter().any(|w| n.contains(w));
    match (vorn, hinten) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

impl Seite {
    fn laenge(&self, k: &[Kette]) -> f64 {
        self.stuecke.iter().map(|&(i, _)| k[i].laenge).sum()
    }

    /// Kerben vom Anfang der Seite.
    fn kerben(&self, k: &[Kette]) -> Vec<f64> {
        let mut out = Vec::new();
        let mut versatz = 0.0;
        for &(i, rueck) in &self.stuecke {
            let c = &k[i];
            for &d in &c.kerben {
                out.push(versatz + if rueck { c.laenge - d } else { d });
            }
            versatz += c.laenge;
        }
        out.sort_by(f64::total_cmp);
        out
    }

    /// Stellen, an denen die Seite von einem Teil ins nächste übergeht.
    fn fugen(&self, k: &[Kette]) -> Vec<f64> {
        let mut out = Vec::new();
        let mut versatz = 0.0;
        for (n, &(i, _)) in self.stuecke.iter().enumerate() {
            versatz += k[i].laenge;
            if n + 1 < self.stuecke.len() {
                out.push(versatz);
            }
        }
        out
    }

    fn umgekehrt(&self) -> Seite {
        Seite { stuecke: self.stuecke.iter().rev().map(|&(i, r)| (i, !r)).collect() }
    }

    /// (Teil, Punktknoten) am Anfang und am Ende.
    fn enden(&self, k: &[Kette]) -> ((usize, usize), (usize, usize)) {
        let ende = |&(i, r): &(usize, bool), anfang: bool| {
            let c = &k[i];
            let knoten = if r == anfang { c.bis } else { c.von };
            (c.teil, knoten)
        };
        (ende(&self.stuecke[0], true), ende(self.stuecke.last().unwrap(), false))
    }
}

impl Auswertung<'_> {
    /// Schlägt die Nähte eines Musters vor (siehe Moduldoku).
    pub fn schlage_naehte_vor(&self, quelle: Kurvenquelle) -> Result<Vorschlag, GeomError> {
        let mut raender: Vec<Teilrand> = Vec::new();
        for t in self.muster().pieces() {
            // Teile mit unlösbarem Rand nähen nicht mit.
            if let Ok(r) = self.teilrand(t, quelle) {
                if !r.kanten.is_empty() {
                    raender.push(r);
                }
            }
        }
        let mm = raender.first().map_or(10.0, |r| r.mm_pro_einheit);
        let tol_l = LAENGE_MM / mm;
        let tol_k = KERBE_MM / mm;
        let tol_lose = KERBE_LOSE_MM / mm;
        // Schwerpunkt je Teil (Mittel der Umrisspunkte): auf welcher Seite
        // einer gemeinsamen Kante ein Teil liegt.
        let schwer: Vec<Pt> = raender
            .iter()
            .map(|r| {
                let n = r.umriss.len().max(1) as f64;
                Pt::new(r.umriss.iter().map(|p| p.x).sum::<f64>() / n, r.umriss.iter().map(|p| p.y).sum::<f64>() / n)
            })
            .collect();

        // 1. Ketten je Teil.
        let mut ketten: Vec<Kette> = Vec::new();
        for (ti, r) in raender.iter().enumerate() {
            let n = r.kanten.len();
            let mit_zugabe = (0..n).any(|k| matches!(r.zugabe(k), (Some(a), _) if a > 1e-9) || matches!(r.zugabe(k), (_, Some(b)) if b > 1e-9));
            // Freie Kanten bilden eigene Ketten; sie kommen erst in der
            // dritten Runde dran, und nur mit Kerben (Rock v36: die
            // Seitennähte haben Zugabe 0, Säume haben keine Kerben).
            let naehbar: Vec<bool> = (0..n).map(|k| !(mit_zugabe && r.ist_frei(k))).collect();
            // Ecke an Punktknoten i: zwischen Kante i−1 (endet dort) und Kante i.
            let ecke: Vec<bool> = (0..n)
                .map(|i| {
                    let vor = &r.kanten[(i + n - 1) % n];
                    let nach = &r.kanten[i];
                    let min = 2.0 / mm;
                    match (richtung(&vor.punkte, false, min), richtung(&nach.punkte, true, min)) {
                        (Some(a), Some(b)) => (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0).acos().to_degrees() >= ECKE_GRAD,
                        _ => true,
                    }
                })
                .collect();
            // Bruchstellen: Ecken und Wechsel nähbar/frei.
            let bruch: Vec<usize> = (0..n).filter(|&i| ecke[i] || naehbar[i] != naehbar[(i + n - 1) % n]).collect();
            let starts: Vec<usize> = if bruch.is_empty() { vec![0] } else { bruch.clone() };
            // Eine Kette von Punktknoten `anfang` vorwärts bis `ende` (ohne die
            // Kante `ende`): Länge, Kerben, Verlauf, Innenpunkte, Kanten.
            let baue = |anfang: usize, ende: usize| {
                let mut laenge = 0.0;
                let mut kerben = Vec::new();
                let (mut innen, mut kk) = (Vec::new(), Vec::new());
                let mut i = anfang;
                loop {
                    laenge += r.kanten[i].laenge;
                    kk.push(i);
                    i = (i + 1) % n;
                    if i == ende {
                        break;
                    }
                    innen.push((i, laenge));
                    if r.punktknoten[i].kerbe {
                        kerben.push(laenge);
                    }
                }
                // Gerade: alle Punkte bis 2 mm an der Sehne.
                let a = r.umriss[r.punktknoten[anfang].stelle];
                let b = r.umriss[r.punktknoten[ende].stelle];
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let sehne = (dx * dx + dy * dy).sqrt();
                let mut gerade = sehne > 0.0 && anfang != ende;
                let mut k = anfang;
                while gerade {
                    for q in &r.kanten[k].punkte {
                        if ((q.x - a.x) * dy - (q.y - a.y) * dx).abs() / sehne > 2.0 / mm {
                            gerade = false;
                        }
                    }
                    k = (k + 1) % n;
                    if k == ende {
                        break;
                    }
                }
                (laenge, kerben, gerade, innen, kk)
            };
            for (s, &anfang) in starts.iter().enumerate() {
                let frei = !naehbar[anfang];
                let ende = if bruch.is_empty() { anfang } else { starts[(s + 1) % starts.len()] };
                // Stößt ein Kettenende an eine freie Kante (Saum)?
                let (start_frei, ende_frei) = (!naehbar[(anfang + n - 1) % n], !naehbar[ende % n]);
                // Öffnung: Die ganze Kette ist eine Schlitzkante, wenn sie an
                // einem Schlitzpunkt beginnt oder endet und das andere Ende am
                // Saum liegt.
                let oeffnung = !frei
                    && ((ist_schlitzname(&r.punktknoten[anfang].name) && ende_frei)
                        || (ist_schlitzname(&r.punktknoten[ende].name) && start_frei));
                // Ein Schlitzpunkt im Inneren teilt die Kette: darunter der
                // Schlitz, darüber die Naht. Der Schlitz steht nicht im Muster —
                // dieselbe Zugabe und dieselben Kerben wie die Naht, nur der
                // Name sagt es. Geteilt wird nur, wenn genau ein Kettenende an
                // den Saum stößt: der Bund trägt denselben Namen als Marke und
                // hat an beiden Enden nähbare Nachbarn.
                let teilung = if !frei && start_frei != ende_frei {
                    (anfang..).map(|i| i % n).take_while(|&i| i != ende).skip(1).find(|&i| ist_schlitzname(&r.punktknoten[i].name))
                } else {
                    None
                };
                match teilung {
                    Some(sp) => {
                        let (l0, k0, g0, i0, kk0) = baue(anfang, sp);
                        ketten.push(Kette { teil: ti, von: anfang, bis: sp, laenge: l0, kerben: k0, gerade: g0, ganz: false, frei, oeffnung: start_frei, innen: i0, kanten: kk0 });
                        let (l1, k1, g1, i1, kk1) = baue(sp, ende);
                        ketten.push(Kette { teil: ti, von: sp, bis: ende, laenge: l1, kerben: k1, gerade: g1, ganz: false, frei, oeffnung: ende_frei, innen: i1, kanten: kk1 });
                    }
                    None => {
                        let (laenge, kerben, gerade, innen, kk) = baue(anfang, ende);
                        ketten.push(Kette { teil: ti, von: anfang, bis: ende, laenge, kerben, gerade, ganz: anfang == ende, frei, oeffnung, innen, kanten: kk });
                    }
                }
            }
        }

        // Varianten: an einem Innenpunkt bis KUERZEN_MM vor einem Ende gekürzt.
        let kuerzen = KUERZEN_MM / mm;
        for k in 0..ketten.len() {
            let c = ketten[k].clone();
            if c.ganz {
                continue;
            }
            let mut anfaenge = vec![(c.von, 0.0)];
            anfaenge.extend(c.innen.iter().filter(|&&(_, d)| d <= kuerzen).copied());
            let mut enden = vec![(c.bis, c.laenge)];
            enden.extend(c.innen.iter().filter(|&&(_, d)| c.laenge - d <= kuerzen).copied());
            for &(s, d0) in &anfaenge {
                for &(e, d1) in &enden {
                    if (s == c.von && e == c.bis) || d1 - d0 <= kuerzen {
                        continue;
                    }
                    let erste = c.kanten.iter().position(|&x| x == s).unwrap_or(0);
                    let anzahl = c.kanten.iter().position(|&x| x == e).unwrap_or(c.kanten.len()) - erste;
                    ketten.push(Kette {
                        teil: c.teil,
                        von: s,
                        bis: e,
                        laenge: d1 - d0,
                        kerben: c.kerben.iter().filter(|&&x| x > d0 + 1e-9 && x < d1 - 1e-9).map(|x| x - d0).collect(),
                        gerade: c.gerade,
                        ganz: false,
                        frei: c.frei,
                        oeffnung: c.oeffnung,
                        innen: c.innen.iter().filter(|&&(_, x)| x > d0 + 1e-9 && x < d1 - 1e-9).map(|&(i, x)| (i, x - d0)).collect(),
                        kanten: c.kanten[erste..erste + anzahl].to_vec(),
                    });
                }
            }
        }
        // Ketten, die sich Randkanten teilen, schließen einander aus.
        let ueberlappt: Vec<Vec<usize>> = (0..ketten.len())
            .map(|i| {
                (0..ketten.len())
                    .filter(|&j| ketten[j].teil == ketten[i].teil && ketten[j].kanten.iter().any(|x| ketten[i].kanten.contains(x)))
                    .collect()
            })
            .collect();

        let mitte = |s: &Seite| -> Pt {
            let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
            for &(i, _) in &s.stuecke {
                let c = &ketten[i];
                for k in [c.von, c.bis] {
                    let p = raender[c.teil].umriss[raender[c.teil].punktknoten[k].stelle];
                    x += p.x;
                    y += p.y;
                    n += 1.0;
                }
            }
            Pt::new(x / n, y / n)
        };
        let punkt_id = |(t, k): (usize, usize)| raender[t].punktknoten[k].id.clone();

        // Zwillinge: Teile mit denselben Randkantenlängen (linkes und rechtes
        // Spiegelbild, gedoppelte Hilfsteile) und ein Teil mit sich selbst.
        // Ihre Kanten passen immer aufeinander; gepaart werden sie erst in
        // der zweiten Runde und nur an einem gemeinsamen Punkt oder geraden
        // Kanten (Spiegelachse, Ärmelnaht).
        let signatur: Vec<Vec<f64>> = raender
            .iter()
            .map(|r| {
                let mut s: Vec<f64> = r.kanten.iter().map(|k| k.laenge).collect();
                s.sort_by(f64::total_cmp);
                s
            })
            .collect();
        // Gleich viele Randkanten, der Größe nach je bis 1 mm gleich (die
        // Bischofärmel des Bolero sind nicht ganz genau gespiegelt).
        let gleich = |p: usize, q: usize| {
            signatur[p].len() == signatur[q].len() && signatur[p].iter().zip(&signatur[q]).all(|(a, b)| (a - b).abs() <= tol_l)
        };
        let zwillinge = |p: usize, q: usize| p == q || gleich(p, q);
        // Doppelteile: Zwillinge über dieselben Rechenpunkte (gedoppelte
        // Hilfsteile, Laminatlagen) — nie eine Naht.
        let punkte: Vec<std::collections::BTreeSet<String>> =
            raender.iter().map(|r| r.punktknoten.iter().map(|k| k.id.clone()).collect()).collect();
        let doppelt = |p: usize, q: usize| p != q && gleich(p, q) && punkte[p] == punkte[q];
        // Zwei Seiten sind Zwillinge, wenn ihre Teile einander paarweise
        // Zwillinge sind (auch zusammengesetzt: Band Mitte + Band seitlich
        // links gegen dieselben rechts).
        let seiten_zwillinge = |a: &Seite, b: &Seite| {
            let mut pa: Vec<usize> = a.stuecke.iter().map(|&(k, _)| ketten[k].teil).collect();
            let mut pb: Vec<usize> = b.stuecke.iter().map(|&(k, _)| ketten[k].teil).collect();
            if pa.len() != pb.len() {
                return false;
            }
            pa.sort();
            pb.sort();
            let mut frei = vec![true; pb.len()];
            pa.iter().all(|&x| {
                if let Some(j) = (0..pb.len()).find(|&j| frei[j] && zwillinge(x, pb[j])) {
                    frei[j] = false;
                    true
                } else {
                    false
                }
            })
        };
        let mut zweite_runde = false;
        let mut dritte_runde = false;
        let kettenmitte = |k: usize| {
            let c = &ketten[k];
            let r = &raender[c.teil];
            let (a, b) = (r.umriss[r.punktknoten[c.von].stelle], r.umriss[r.punktknoten[c.bis].stelle]);
            Pt::new(0.5 * (a.x + b.x), 0.5 * (a.y + b.y))
        };
        // Je Paar von Zwillingsteilen das Kettenpaar, das sich in der Zeichnung
        // am nächsten liegt: dort liegt die Spiegelachse (hintere Mitte zweier
        // getrennt gezeichneter Spiegelbilder). Andere Zwillingskanten (Säume,
        // Oberkanten) werden nie gepaart.
        let mut naechstes: std::collections::HashMap<(usize, usize), (usize, usize, f64)> = std::collections::HashMap::new();
        for i in 0..ketten.len() {
            for j in 0..ketten.len() {
                let (p, q) = (ketten[i].teil, ketten[j].teil);
                if p >= q || !zwillinge(p, q) {
                    continue;
                }
                let d = kettenmitte(i).dist(kettenmitte(j));
                let e = naechstes.entry((p, q)).or_insert((i, j, f64::INFINITY));
                if d < e.2 {
                    *e = (i, j, d);
                }
            }
        }

        // Auflagen: zwei Ketten verschiedener Teile über dieselben beiden
        // Endpunkte, beide Teile auf derselben Seite (Trim auf der Öffnung,
        // Kragen außen auf innen, Laminatlagen) — aufgeklebt, nicht genäht;
        // beide Ketten bleiben ungenäht. Bei einer Naht liegen die Teile
        // beidseits der gemeinsamen Kante.
        let mut auflage = vec![false; ketten.len()];
        // Schlitzkanten werden nie genäht (siehe `Kette::oeffnung`).
        for (m, e) in ketten.iter().enumerate() {
            if e.oeffnung {
                auflage[m] = true;
            }
        }
        for i in 0..ketten.len() {
            for j in i + 1..ketten.len() {
                let (c, d) = (&ketten[i], &ketten[j]);
                if c.teil == d.teil || (c.laenge - d.laenge).abs() > tol_l {
                    continue;
                }
                let id = |t: usize, k: usize| raender[t].punktknoten[k].id.clone();
                let (c0, c1, d0, d1) = (id(c.teil, c.von), id(c.teil, c.bis), id(d.teil, d.von), id(d.teil, d.bis));
                if !((c0 == d0 && c1 == d1) || (c0 == d1 && c1 == d0)) {
                    continue;
                }
                let r = &raender[c.teil];
                let (q0, q1) = (r.umriss[r.punktknoten[c.von].stelle], r.umriss[r.punktknoten[c.bis].stelle]);
                let seite = |s: Pt| (q1.x - q0.x) * (s.y - q0.y) - (q1.y - q0.y) * (s.x - q0.x);
                if seite(schwer[c.teil]) * seite(schwer[d.teil]) > 0.0 {
                    auflage[i] = true;
                    auflage[j] = true;
                    // Das kleinere Teil ist die aufgelegte Lage: keine seiner
                    // Kanten wird genäht (Trim, Kragen außen).
                    let flaeche = |t: usize| {
                        let u = &raender[t].umriss;
                        (0..u.len()).map(|k| u[k].x * u[(k + 1) % u.len()].y - u[(k + 1) % u.len()].x * u[k].y).sum::<f64>().abs() / 2.0
                    };
                    let klein = if flaeche(c.teil) <= flaeche(d.teil) { c.teil } else { d.teil };
                    for (m, e) in ketten.iter().enumerate() {
                        if e.teil == klein {
                            auflage[m] = true;
                        }
                    }
                }
            }
        }
        // Auch eine einzelne gemeinsame Randkante (dieselben beiden
        // Rechenpunkte, gleich lang) mit beiden Teilen auf derselben Seite
        // macht das kleinere Teil zur Auflage: der Trim der Bluse1 teilt mit
        // dem Vorderteil nur den oberen Teil der Keyhole-Kante. Welche Seite,
        // sagt die Innennormale in der Mitte der Kante; der Schwerpunkt
        // täuscht an schmalen Streifen und gekrümmten Nähten (Cups).
        let flaeche_s = |t: usize| {
            let u = &raender[t].umriss;
            (0..u.len()).map(|k| u[k].x * u[(k + 1) % u.len()].y - u[(k + 1) % u.len()].x * u[k].y).sum::<f64>() / 2.0
        };
        let flaeche_von = |t: usize| flaeche_s(t).abs();
        let innen_normale = |t: usize, e: &crate::kanten::Randkante| -> Pt {
            let n = e.punkte.len();
            let (a, b) = if n >= 2 { (e.punkte[(n - 1) / 2], e.punkte[(n - 1) / 2 + 1]) } else { (Pt::new(0.0, 0.0), Pt::new(0.0, 0.0)) };
            let s = flaeche_s(t).signum();
            Pt::new(-(b.y - a.y) * s, (b.x - a.x) * s)
        };
        let mut aufgelegt = vec![false; raender.len()];
        for p in 0..raender.len() {
            for q in p + 1..raender.len() {
                let (rp, rq) = (&raender[p], &raender[q]);
                let deckt = rp.kanten.iter().any(|e| {
                    let (e0, e1) = (&rp.punktknoten[e.von].id, &rp.punktknoten[e.bis].id);
                    rq.kanten.iter().any(|f| {
                        let (f0, f1) = (&rq.punktknoten[f.von].id, &rq.punktknoten[f.bis].id);
                        ((e0 == f0 && e1 == f1) || (e0 == f1 && e1 == f0)) && (e.laenge - f.laenge).abs() <= tol_l && {
                            let (np, nq) = (innen_normale(p, e), innen_normale(q, f));
                            np.x * nq.x + np.y * nq.y > 0.0
                        }
                    })
                });
                if deckt {
                    aufgelegt[if flaeche_von(p) <= flaeche_von(q) { p } else { q }] = true;
                }
            }
        }
        for (m, e) in ketten.iter().enumerate() {
            if aufgelegt[e.teil] {
                auflage[m] = true;
            }
        }
        for i in 0..ketten.len() {
            if auflage[i] {
                for &j in &ueberlappt[i] {
                    auflage[j] = true;
                }
            }
        }

        // 2.–4. Kandidaten, Auswahl, zusammengesetzte Seiten.
        let mut benutzt = auflage.clone();
        let mut naehte: Vec<(Seite, Seite)> = Vec::new();
        // Je Naht: Richtung sicher, beide Seiten spiegelgleich.
        let mut art: Vec<(bool, bool)> = Vec::new();
        // Jede Kette in beiden Richtungen, damit sie an beiden Enden wachsen kann.
        let mut seiten: Vec<Seite> =
            (0..ketten.len()).flat_map(|i| [Seite { stuecke: vec![(i, false)] }, Seite { stuecke: vec![(i, true)] }]).collect();
        loop {
            // Kandidaten unter den Seiten, deren Ketten alle frei sind.
            let frei: Vec<usize> = (0..seiten.len()).filter(|&s| seiten[s].stuecke.iter().all(|&(i, _)| !benutzt[i])).collect();
            let mut kandidaten: Vec<(f64, usize, usize, bool, bool, bool)> = Vec::new();
            for (x, &i) in frei.iter().enumerate() {
                for &j in &frei[x + 1..] {
                    let (a, b) = (&seiten[i], &seiten[j]);
                    // Keine Randkante auf beiden Seiten.
                    if a.stuecke.iter().any(|s| b.stuecke.iter().any(|t| ueberlappt[s.0].contains(&t.0))) {
                        continue;
                    }
                    let (la, lb) = (a.laenge(&ketten), b.laenge(&ketten));
                    if (la - lb).abs() > tol_l {
                        continue;
                    }
                    if a.stuecke.iter().chain(&b.stuecke).any(|&(k, _)| ketten[k].ganz) {
                        continue;
                    }
                    let mit_freien = a.stuecke.iter().chain(&b.stuecke).any(|&(k, _)| ketten[k].frei);
                    if mit_freien && !zweite_runde {
                        continue;
                    }
                    if a.stuecke.iter().any(|&(k, _)| b.stuecke.iter().any(|&(l, _)| doppelt(ketten[k].teil, ketten[l].teil))) {
                        continue;
                    }
                    let zwilling = seiten_zwillinge(a, b);
                    let einzeln = a.stuecke.len() == 1 && b.stuecke.len() == 1;
                    let ein_teil = einzeln && ketten[a.stuecke[0].0].teil == ketten[b.stuecke[0].0].teil;
                    // Einzelne Ketten: gerade zu gerade, gekrümmt zu gekrümmt.
                    if einzeln && ketten[a.stuecke[0].0].gerade != ketten[b.stuecke[0].0].gerade {
                        continue;
                    }
                    // Das nächste Kettenpaar zweier Zwillingsteile (siehe oben).
                    let achse = einzeln && !ein_teil && {
                        let (i, j) = (a.stuecke[0].0, b.stuecke[0].0);
                        let (p, q) = (ketten[i].teil, ketten[j].teil);
                        let key = if p < q { (p, q) } else { (q, p) };
                        naechstes.get(&key).is_some_and(|&(x, y, _)| (x == i && y == j) || (x == j && y == i))
                    };
                    // Ein Teil, das schon mit einem anderen verbunden ist.
                    let verbunden = |teil: usize| {
                        naehte.iter().any(|(s, u)| {
                            let ts: Vec<usize> = s.stuecke.iter().chain(&u.stuecke).map(|&(k, _)| ketten[k].teil).collect();
                            ts.contains(&teil) && ts.iter().any(|&x| x != teil)
                        })
                    };
                    let kragen = {
                        let n = raender[ketten[a.stuecke[0].0].teil].teil.to_lowercase();
                        n.contains("kragen") || n.contains("collar")
                    };
                    let gerade = a.stuecke.iter().chain(&b.stuecke).all(|&(k, _)| ketten[k].gerade);
                    let (ka, fa) = (a.kerben(&ketten), a.fugen(&ketten));
                    // Jede Kerbe der einen Seite liegt einer Kerbe oder einer
                    // Teilgrenze der anderen gegenüber (die Kugelmitte eines
                    // Ärmels gegenüber der Schulternaht im Armloch).
                    // Wie viele Kerben ohne Gegenstück (Kerbe oder Teilgrenze) bis `tol`.
                    let ohne = |k: &[f64], gegen_k: &[f64], gegen_f: &[f64], tol: f64| {
                        k.iter().filter(|&&x| !gegen_k.iter().chain(gegen_f).any(|&y| (x - y).abs() <= tol)).count()
                    };
                    // Beide Richtungen prüfen; passen beide, entscheiden
                    // gemeinsame Punkte, sonst die Lage der Enden in der Zeichnung.
                    let mut moeglich: Vec<(bool, u32, f64)> = Vec::new();
                    for umkehr in [false, true] {
                        let bb = if umkehr { b.umgekehrt() } else { b.clone() };
                        let (kb, fb) = (bb.kerben(&ketten), bb.fugen(&ketten));
                        let streng = ohne(&ka, &kb, &fb, tol_k) + ohne(&kb, &ka, &fa, tol_k) == 0;
                        let lose = ohne(&ka, &kb, &fb, tol_lose) + ohne(&kb, &ka, &fa, tol_lose) <= 1;
                        if !(streng || (zweite_runde && lose)) {
                            continue;
                        }
                        // Gemeinsame Endpunkte: derselbe Rechenpunkt am selben Ende.
                        let ((a0, a1), (b0, b1)) = (a.enden(&ketten), bb.enden(&ketten));
                        let gemeinsam = (punkt_id(a0) == punkt_id(b0)) as u32 + (punkt_id(a1) == punkt_id(b1)) as u32;
                        // Deckungsgleiche Ketten nie über Kreuz.
                        let gekreuzt = (punkt_id(a0) == punkt_id(b1)) as u32 + (punkt_id(a1) == punkt_id(b0)) as u32;
                        if gekreuzt > gemeinsam {
                            continue;
                        }
                        // Lage: Enden relativ zur Mitte ihrer Seite, so wie gezeichnet.
                        let (ma, mb) = (mitte(a), mitte(&bb));
                        let rel = |e: (usize, usize), m: Pt| {
                            let q = raender[e.0].umriss[raender[e.0].punktknoten[e.1].stelle];
                            Pt::new(q.x - m.x, q.y - m.y)
                        };
                        let versatz = rel(a0, ma).dist(rel(b0, mb)) + rel(a1, ma).dist(rel(b1, mb));
                        // Zwillinge: in der ersten Runde nur mit gemeinsamem
                        // Punkt, in der zweiten auch an geraden Kanten.
                        // Zwillinge: in der ersten Runde nur an der Spiegelachse
                        // (beide Enden gemeinsam). In der zweiten: als gerade
                        // Selbstnaht eines schon verbundenen Teils (Ärmel, Bund;
                        // nicht am Kragen) oder am nächsten Kettenpaar zweier
                        // Zwillingsteile, gerade und ab 3 cm.
                        let selbstnaht = zweite_runde && ein_teil && gerade && !kragen && verbunden(ketten[a.stuecke[0].0].teil);
                        // Zuletzt Zwillinge, die schon mit anderen Teilen verbunden
                        // sind, an geraden Kanten, gegenläufig wie zwei Spiegelbilder
                        // (verschobene Kopien wie zwei Manschetten laufen gleich), am
                        // nächsten Kettenpaar oder mit Kerben gegenüber: die hintere
                        // Mitte zweier Bahnen, die vom Saum weg gezeichnet sind.
                        let spiegel = {
                            let (i, j) = (a.stuecke[0].0, b.stuecke[0].0);
                            dritte_runde && einzeln && !ein_teil && gerade && umkehr
                                && verbunden(ketten[i].teil) && verbunden(ketten[j].teil)
                                && ((achse && la * mm >= 30.0) || !ka.is_empty())
                        };
                        if zwilling && gemeinsam < 2 && !selbstnaht && !spiegel {
                            continue;
                        }
                        // Freie Kanten nur mit Kerben, Kerbe für Kerbe streng
                        // gegenüber (Seitennaht des Rocks; nicht Öffnung an Säume).
                        if mit_freien {
                            let kb = bb.kerben(&ketten);
                            if ka.is_empty() || ka.len() != kb.len() || ka.iter().zip(&kb).any(|(x, y)| (x - y).abs() > tol_k) {
                                continue;
                            }
                        }
                        // Verschiedene Teile ohne Kerben und ohne gemeinsamen Punkt:
                        // erst ab 3,5 cm (kleine Hilfsteile haben zufällig gleich
                        // lange Kanten von 1 bis 3 cm; das kürzeste echte Stück
                        // der Prüfmuster, die Schulter der Bluse3 am Halsteil, ist 3,8 cm).
                        if !ein_teil && ka.is_empty() && gemeinsam == 0 && la * mm < 35.0 {
                            continue;
                        }
                        moeglich.push((umkehr, gemeinsam, versatz));
                    }
                    // Richtung sicher: nur eine passt, oder gemeinsame Punkte entscheiden.
                    let sicher = moeglich.len() == 1 || (moeglich.len() == 2 && moeglich[0].1 != moeglich[1].1);
                    // Kerben und Teilgrenzen beider Seiten spiegelgleich: dann sagt
                    // auch die Zeichnung nichts über die Richtung (Kragen, Bund).
                    let symmetrisch = |s: &Seite| {
                        let l = s.laenge(&ketten);
                        let mut m: Vec<f64> = s.kerben(&ketten).into_iter().chain(s.fugen(&ketten)).collect();
                        m.sort_by(f64::total_cmp);
                        m.iter().zip(m.iter().rev()).all(|(x, y)| (x - (l - y)).abs() <= tol_lose)
                    };
                    let symm = symmetrisch(a) && symmetrisch(b);
                    if moeglich.len() == 2 {
                        let (x, y) = (moeglich[0], moeglich[1]);
                        let besser = if x.1 != y.1 { if x.1 > y.1 { x } else { y } } else if x.2 <= y.2 { x } else { y };
                        moeglich = vec![besser];
                    }
                    for (umkehr, gemeinsam, _) in moeglich {
                        let bb = if umkehr { b.umgekehrt() } else { b.clone() };
                        let (ma, mb) = (mitte(a), mitte(&bb));
                        let seite_a = seitenwort(&raender[ketten[a.stuecke[0].0].teil].teil);
                        let seite_b = seitenwort(&raender[ketten[bb.stuecke[0].0].teil].teil);
                        let fremd = matches!((seite_a, seite_b), (Some(p), Some(q)) if p != q);
                        let wert = -(gemeinsam as f64) * 1e6 - ka.len() as f64 * 1e4
                            + (la - lb).abs() / tol_l * 10.0
                            + if fremd { 1e3 } else { 0.0 }
                            + ma.dist(mb) * 1e-3;
                        kandidaten.push((wert, i, j, umkehr, sicher, symm));
                    }
                }
            }
            kandidaten.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut neu = 0;
            for (_, i, j, umkehr, sicher, symm) in kandidaten {
                if seiten[i].stuecke.iter().chain(&seiten[j].stuecke).any(|&(k, _)| benutzt[k]) {
                    continue;
                }
                for &(k, _) in seiten[i].stuecke.iter().chain(&seiten[j].stuecke) {
                    for &m in &ueberlappt[k] {
                        benutzt[m] = true;
                    }
                }
                let b = if umkehr { seiten[j].umgekehrt() } else { seiten[j].clone() };
                naehte.push((seiten[i].clone(), b));
                art.push((sicher, symm));
                neu += 1;
            }

            // Zusammengesetzte Seiten: An jedem Nahtende stoßen zwei Teile
            // aneinander; eine freie Kette, die am einen Teil dort endet,
            // setzt sich in einer freien Kette fort, die am anderen dort beginnt.
            let mut stoesse: Vec<((usize, usize), (usize, usize))> = Vec::new();
            for (a, b) in &naehte {
                let ((a0, a1), (b0, b1)) = (a.enden(&ketten), b.enden(&ketten));
                stoesse.push((a0, b0));
                stoesse.push((a1, b1));
            }
            let mut gesehen: HashSet<Vec<(usize, bool)>> = seiten.iter().map(|s| s.stuecke.clone()).collect();
            let mut zuwachs = Vec::new();
            for s in seiten.iter().filter(|s| s.stuecke.iter().all(|&(i, _)| !benutzt[i])) {
                if s.stuecke.len() >= 4 {
                    continue;
                }
                let (_, ende) = s.enden(&ketten);
                for &(p, q) in &stoesse {
                    for (hier, dort) in [(p, q), (q, p)] {
                        if hier != ende {
                            continue;
                        }
                        // Freie Ketten am anderen Teil, die an `dort` beginnen (vorwärts) oder enden (rückwärts).
                        for (k, c) in ketten.iter().enumerate() {
                            if benutzt[k] || c.teil != dort.0 || s.stuecke.iter().any(|t| t.0 == k) {
                                continue;
                            }
                            for r in [false, true] {
                                let anfang = if r { c.bis } else { c.von };
                                if anfang == dort.1 {
                                    let mut st = s.stuecke.clone();
                                    st.push((k, r));
                                    if gesehen.insert(st.clone()) {
                                        zuwachs.push(Seite { stuecke: st });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let zuwachs_da = !zuwachs.is_empty();
            seiten.extend(zuwachs.iter().cloned());
            // Auch rückwärts: eine Seite kann am Anfang verlängert werden; die
            // umgekehrte Seite deckt das ab.
            for s in zuwachs {
                let u = s.umgekehrt();
                if gesehen.insert(u.stuecke.clone()) {
                    seiten.push(u);
                }
            }
            if neu == 0 && !zuwachs_da {
                if dritte_runde {
                    break;
                }
                if zweite_runde {
                    dritte_runde = true;
                }
                zweite_runde = true;
            }
        }

        // 5. Spiegelung und Richtung: Von außen gezeichnet laufen die beiden
        // Seiten einer Naht im Umlaufsinn ihrer Teile gegeneinander (siehe
        // [`verdrehte_naehte`]). Die sicheren Nähte legen fest, welche Teile
        // zueinander gespiegelt sind; die übrigen folgen dem, sonst der
        // Zeichnung, sonst „von außen gezeichnet“ für die Mehrheit der Fläche.
        let flaeche: Vec<f64> = raender
            .iter()
            .map(|r| {
                let u = &r.umriss;
                (0..u.len()).map(|k| u[k].x * u[(k + 1) % u.len()].y - u[(k + 1) % u.len()].x * u[k].y).sum::<f64>() / 2.0
            })
            .collect();
        let w = |&(k, r): &(usize, bool)| flaeche[ketten[k].teil].signum() * if r { -1.0 } else { 1.0 };
        // Bedingungen f(x)·f(y) = rel; `true` für die über die Naht.
        let bedingungen = |a: &Seite, b: &Seite| -> Vec<(usize, usize, f64, bool)> {
            let r = &a.stuecke[0];
            let mut v: Vec<(usize, usize, f64, bool)> =
                a.stuecke[1..].iter().map(|s| (ketten[r.0].teil, ketten[s.0].teil, w(r) * w(s), false)).collect();
            v.extend(b.stuecke.iter().map(|s| (ketten[r.0].teil, ketten[s.0].teil, -w(r) * w(s), true)));
            v
        };
        let mut pt = Paritaet::neu(raender.len());
        let mut widersprueche = 0;
        for (n, (a, b)) in naehte.iter().enumerate() {
            if art[n].0 {
                let mut gut = true;
                for (x, y, rel, _) in bedingungen(a, b) {
                    gut &= pt.verbinde(x, y, rel);
                }
                widersprueche += usize::from(!gut);
            }
        }
        // Unsichere Nähte über die Wurzeln der sicheren Gruppen.
        let unsicher: Vec<usize> = (0..naehte.len()).filter(|&n| !art[n].0).collect();
        let mut bed: Vec<Vec<(usize, usize, f64, bool)>> = Vec::new();
        for &n in &unsicher {
            let v = bedingungen(&naehte[n].0, &naehte[n].1)
                .into_iter()
                .map(|(x, y, rel, ueber)| {
                    let ((rx, px), (ry, py)) = (pt.wurzel(x), pt.wurzel(y));
                    (rx, ry, rel * px * py, ueber)
                })
                .collect();
            bed.push(v);
        }
        // Stimmen „von außen gezeichnet“ je Teil aus der Lage einer senkrechten
        // Naht im aufrecht gezeichneten Teil (Muster-y = abwärts am Körper,
        // wie die Platzierung annimmt): Von außen gesehen liegt die Mittelnaht
        // eines linken Vorderteils links (Muster-x klein), eines linken
        // Rückenteils rechts; die Seitennaht jeweils gegenüber. Mittelnaht:
        // zwischen linkem und rechtem Teil derselben Zone; Seitennaht:
        // zwischen vorn und hinten derselben Seite.
        let mut stimmen = vec![0.0; raender.len()];
        for (a, b) in &naehte {
            if a.stuecke.len() != 1 || b.stuecke.len() != 1 {
                continue;
            }
            let (ka, kb) = (a.stuecke[0].0, b.stuecke[0].0);
            let (p, q) = (ketten[ka].teil, ketten[kb].teil);
            for (k, t, u) in [(ka, p, q), (kb, q, p)] {
                let r = &raender[t];
                let (q0, q1) = (r.umriss[r.punktknoten[ketten[k].von].stelle], r.umriss[r.punktknoten[ketten[k].bis].stelle]);
                stimmen[t] += stimme(&raender[t].teil, &raender[u].teil, q0, q1, schwer[t]).unwrap_or(0.0);
            }
        }
        let mut gruppen: Vec<usize> = (0..raender.len()).map(|x| pt.wurzel(x).0).collect();
        gruppen.sort();
        gruppen.dedup();
        // Stimmen und Fläche „von außen gezeichnet“ je Gruppe bei f(Wurzel) = +1.
        let mut aussen = vec![(0.0, 0.0); raender.len()];
        for x in 0..raender.len() {
            let (r, p) = pt.wurzel(x);
            aussen[r].0 += p * stimmen[x];
            aussen[r].1 += p * flaeche[x].abs();
        }
        let beteiligt: Vec<usize> = gruppen.iter().copied().filter(|g| bed.iter().flatten().any(|b| b.0 == *g || b.1 == *g)).collect();
        let mut f_wurzel = vec![1.0; raender.len()];
        for &g in &gruppen {
            f_wurzel[g] = if (aussen[g].0, aussen[g].1) >= (0.0, 0.0) { 1.0 } else { -1.0 };
        }
        let gilt = |f: &[f64], v: &[(usize, usize, f64, bool)], dreh: bool| {
            v.iter().all(|&(x, y, rel, ueber)| f[x] * f[y] == if dreh && ueber { -rel } else { rel })
        };
        if beteiligt.len() <= 16 {
            // Alle Belegungen: möglichst viele lösbar, dann möglichst viele
            // unsymmetrische so wie gezeichnet, dann Stimmen und Fläche von außen.
            let mut best: Option<((usize, usize, f64, f64), Vec<f64>)> = None;
            for maske in 0u32..(1 << beteiligt.len()) {
                let mut f = f_wurzel.clone();
                for (z, &g) in beteiligt.iter().enumerate() {
                    f[g] = if maske >> z & 1 == 1 { -1.0 } else { 1.0 };
                }
                let mut wert = (0, 0, 0.0, 0.0);
                for (z, v) in bed.iter().enumerate() {
                    let wie = gilt(&f, v, false);
                    wert.0 += usize::from(wie || gilt(&f, v, true));
                    wert.1 += usize::from(wie && !art[unsicher[z]].1);
                }
                wert.2 = beteiligt.iter().map(|&g| f[g] * aussen[g].0).sum();
                wert.3 = beteiligt.iter().map(|&g| f[g] * aussen[g].1).sum();
                let besser = |b: &(usize, usize, f64, f64)| {
                    (wert.0, wert.1) > (b.0, b.1) || ((wert.0, wert.1) == (b.0, b.1) && (wert.2 > b.2 || (wert.2 == b.2 && wert.3 > b.3 + 1e-9)))
                };
                if best.as_ref().is_none_or(|(b, _)| besser(b)) {
                    best = Some((wert, f));
                }
            }
            if let Some((_, f)) = best {
                f_wurzel = f;
            }
        }
        for (z, v) in bed.iter().enumerate() {
            if gilt(&f_wurzel, v, false) {
                continue;
            }
            if gilt(&f_wurzel, v, true) {
                let n = unsicher[z];
                naehte[n].1 = naehte[n].1.umgekehrt();
            } else {
                widersprueche += 1;
            }
        }
        let gespiegelt = (0..raender.len())
            .filter(|&x| {
                let (r, p) = pt.wurzel(x);
                p * f_wurzel[r] < 0.0
            })
            .map(|x| raender[x].teil.clone())
            .collect();

        // Ausgabe als Nahtliste.
        let bezug = |s: &Seite| -> Vec<Abschnittsbezug> {
            s.stuecke
                .iter()
                .map(|&(i, r)| {
                    let c = &ketten[i];
                    let rd = &raender[c.teil];
                    let (von, bis) = if r { (c.bis, c.von) } else { (c.von, c.bis) };
                    Abschnittsbezug {
                        teil: rd.teil.clone(),
                        von: rd.punktknoten[von].name.clone(),
                        bis: rd.punktknoten[bis].name.clone(),
                        rueckwaerts: r,
                    }
                })
                .collect()
        };
        let kurz = |s: &Seite| -> String {
            let t = &raender[ketten[s.stuecke[0].0].teil].teil;
            t.rsplit(" - ").next().unwrap_or(t).to_string()
        };
        let defs = naehte
            .iter()
            .enumerate()
            .map(|(n, (a, b))| Nahtdef {
                kennung: format!("V{:02}", n + 1),
                name: format!("{} / {} ({:.1} cm)", kurz(a), kurz(b), a.laenge(&ketten) * mm / 10.0),
                a: bezug(a),
                b: bezug(b),
                zeile: 0,
            })
            .collect();
        let allein = ketten
            .iter()
            .enumerate()
            .filter(|(k, _)| !benutzt[*k])
            .map(|(_, c)| {
                let rd = &raender[c.teil];
                (rd.teil.clone(), rd.punktknoten[c.von].name.clone(), rd.punktknoten[c.bis].name.clone(), c.laenge * mm / 10.0)
            })
            .collect();
        Ok(Vorschlag { naehte: defs, allein, gespiegelt, widersprueche })
    }
}

/// Eine Nahtliste als Text im Format von [`crate::naehte::lies_nahtliste`].
pub fn nahtliste_text(defs: &[Nahtdef], kopf: &str) -> String {
    let teil = |t: &str| if t.contains([':', '|', '+', '"']) { format!("\"{}\"", t.replace('"', "")) } else { t.to_string() };
    let seite = |s: &[Abschnittsbezug]| {
        s.iter()
            .map(|a| format!("{}: {} {} {}", teil(&a.teil), a.von, a.bis, if a.rueckwaerts { "r" } else { "v" }))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let mut out = String::new();
    for z in kopf.lines() {
        out += &format!("# {z}\n");
    }
    out += "#\n# Kennung | Bezeichnung | Seite A | Seite B\n\n";
    for d in defs {
        out += &format!("{} | {} | {} | {}\n", d.kennung, d.name.replace('|', "/"), seite(&d.a), seite(&d.b));
    }
    out
}

/// Umlaufsinn je Teil: +1 oder −1 nach der Fläche des Umrisses (Teile
/// sind mit der Außenseite nach oben gezeichnet).
fn umlaufsinn(ev: &Auswertung<'_>, quelle: Kurvenquelle) -> std::collections::HashMap<String, f64> {
    let mut sinn = std::collections::HashMap::new();
    for t in ev.muster().pieces() {
        if let Ok(r) = ev.teilrand(t, quelle) {
            let u = &r.umriss;
            let a: f64 = (0..u.len()).map(|k| u[k].x * u[(k + 1) % u.len()].y - u[(k + 1) % u.len()].x * u[k].y).sum();
            sinn.insert(r.teil.clone(), a.signum());
        }
    }
    sinn
}

/// Nähte, die verdreht wären: Liegen zwei Teile mit der Außenseite nach oben
/// nebeneinander, läuft ihre gemeinsame Kante in den beiden Umrissen
/// gegeneinander. Eine Naht ist also nur dann unverdreht, wenn alle
/// Teilstücke einer Seite im Umlaufsinn ihres Teils gleich laufen und die
/// beiden Seiten gegeneinander. Teile in `gespiegelt` (Namen wie in der
/// Nahtliste) sind von innen gezeichnet. Gibt Kennung und Grund zurück.
pub fn verdrehte_naehte(
    ev: &Auswertung<'_>,
    quelle: Kurvenquelle,
    defs: &[Nahtdef],
    gespiegelt: &[String],
) -> Result<Vec<(String, &'static str)>, crate::naehte::NahtFehler> {
    let b = crate::naehte::pruefe_naehte(ev, defs, quelle)?;
    let mut sinn = umlaufsinn(ev, quelle);
    for (teil, s) in sinn.iter_mut() {
        if gespiegelt.iter().any(|g| g == teil || teil.rsplit(" - ").next() == Some(g.as_str())) {
            *s = -*s;
        }
    }
    let mut aus = Vec::new();
    for n in &b.naehte {
        let lauf = |s: &crate::naehte::Nahtseite| -> Option<f64> {
            let w: Vec<f64> = s.teilstuecke.iter().map(|t| sinn[&t.teil] * if t.rueckwaerts { -1.0 } else { 1.0 }).collect();
            w.iter().all(|&x| x == w[0]).then_some(w[0])
        };
        match (lauf(&n.a), lauf(&n.b)) {
            (Some(x), Some(y)) if x == y => aus.push((n.kennung.clone(), "beide Seiten gleichlaufend")),
            (Some(_), Some(_)) => {}
            _ => aus.push((n.kennung.clone(), "Teilstücke einer Seite gegenläufig")),
        }
    }
    Ok(aus)
}

/// Stimme für „Teil `eigen` von außen gezeichnet“ (+1) oder von innen (−1)
/// aus einer senkrechten Naht `q0`–`q1` an Teil `gegen` (siehe die Regel in
/// [`Auswertung::schlage_naehte_vor`], Schritt 5): Von außen gesehen liegt die
/// Mittelnaht eines linken Vorderteils links, eines linken Rückenteils
/// rechts, die Seitennaht jeweils gegenüber. `None`, wenn die Naht weder
/// Mittel- noch Seitennaht ist oder nicht senkrecht verläuft.
fn stimme(eigen: &str, gegen: &str, q0: Pt, q1: Pt, schwer: Pt) -> Option<f64> {
    let (zp, zq) = (zonenwort(eigen), zonenwort(gegen));
    let (sp, sq) = (seitenwort(eigen), seitenwort(gegen));
    let mitte_naht = zp.is_some() && zp == zq && sp.is_some() && sq.is_some() && sp != sq;
    let seiten_naht = zp.is_some() && zq.is_some() && zp != zq && !(sp.is_some() && sq.is_some() && sp != sq);
    let (Some(vorn), Some(links)) = (zp, sp) else { return None };
    if !(mitte_naht || seiten_naht) || (q1.y - q0.y).abs() <= (q1.x - q0.x).abs() {
        return None;
    }
    let dx = (q0.x + q1.x) / 2.0 - schwer.x;
    // Mittelnaht von außen rechts im Teil: vorn rechts, hinten links.
    let rechts_erwartet = (vorn != links) == mitte_naht;
    Some(if (dx > 0.0) == rechts_erwartet { 1.0 } else { -1.0 })
}

/// Spiegelung aus einer fertigen Nahtliste (etwa einer von Hand): Alle
/// Nähte gelten als richtig gerichtet; sie legen fest, welche Teile
/// zueinander gespiegelt sind ([`verdrehte_naehte`]), die Stimmen
/// [`stimme`] und sonst die Fläche, welche Gruppe von außen gezeichnet ist.
/// Gibt die von innen gezeichneten Teile und die Zahl der Nähte zurück, die
/// keiner Spiegelung genügen.
pub fn spiegelung(ev: &Auswertung<'_>, quelle: Kurvenquelle, defs: &[Nahtdef]) -> Result<(Vec<String>, usize), crate::naehte::NahtFehler> {
    let b = crate::naehte::pruefe_naehte(ev, defs, quelle)?;
    let raender: Vec<Teilrand> = ev.muster().pieces().into_iter().filter_map(|t| ev.teilrand(t, quelle).ok()).collect();
    let idx: std::collections::HashMap<&str, usize> = raender.iter().enumerate().map(|(i, r)| (r.teil.as_str(), i)).collect();
    let flaeche: Vec<f64> = raender
        .iter()
        .map(|r| {
            let u = &r.umriss;
            (0..u.len()).map(|k| u[k].x * u[(k + 1) % u.len()].y - u[(k + 1) % u.len()].x * u[k].y).sum::<f64>() / 2.0
        })
        .collect();
    let schwer: Vec<Pt> = raender
        .iter()
        .map(|r| {
            let n = r.umriss.len().max(1) as f64;
            Pt::new(r.umriss.iter().map(|p| p.x).sum::<f64>() / n, r.umriss.iter().map(|p| p.y).sum::<f64>() / n)
        })
        .collect();
    let mut pt = Paritaet::neu(raender.len());
    let mut widersprueche = 0;
    let mut stimmen = vec![0.0; raender.len()];
    let mut genaeht = vec![false; raender.len()];
    for n in &b.naehte {
        let w = |t: &crate::naehte::Teilstueck| flaeche[idx[t.teil.as_str()]].signum() * if t.rueckwaerts { -1.0 } else { 1.0 };
        let r = &n.a.teilstuecke[0];
        let ir = idx[r.teil.as_str()];
        let mut gut = true;
        for (s, ueber) in n.a.teilstuecke[1..].iter().map(|s| (s, false)).chain(n.b.teilstuecke.iter().map(|s| (s, true))) {
            let rel = if ueber { -w(r) * w(s) } else { w(r) * w(s) };
            gut &= pt.verbinde(ir, idx[s.teil.as_str()], rel);
        }
        widersprueche += usize::from(!gut);
        for t in n.a.teilstuecke.iter().chain(&n.b.teilstuecke) {
            genaeht[idx[t.teil.as_str()]] = true;
        }
        if n.a.teilstuecke.len() == 1 && n.b.teilstuecke.len() == 1 {
            for (x, y) in [(&n.a.teilstuecke[0], &n.b.teilstuecke[0]), (&n.b.teilstuecke[0], &n.a.teilstuecke[0])] {
                let t = idx[x.teil.as_str()];
                let rd = &raender[t];
                let (Ok(v), Ok(e)) = (rd.knoten_nach_name(&x.von), rd.knoten_nach_name(&x.bis)) else { continue };
                let (q0, q1) = (rd.umriss[rd.punktknoten[v].stelle], rd.umriss[rd.punktknoten[e].stelle]);
                stimmen[t] += stimme(&x.teil, &y.teil, q0, q1, schwer[t]).unwrap_or(0.0);
            }
        }
    }
    let mut aussen = vec![(0.0, 0.0); raender.len()];
    for x in 0..raender.len() {
        let (r, p) = pt.wurzel(x);
        aussen[r].0 += p * stimmen[x];
        aussen[r].1 += p * flaeche[x].abs();
    }
    let gespiegelt = (0..raender.len())
        .filter(|&x| {
            let (r, p) = pt.wurzel(x);
            let f = if (aussen[r].0, aussen[r].1) >= (0.0, 0.0) { 1.0 } else { -1.0 };
            genaeht[x] && p * f < 0.0
        })
        .map(|x| raender[x].teil.clone())
        .collect();
    Ok((gespiegelt, widersprueche))
}

/// Wie gut ein Vorschlag eine Nahtliste trifft, Kante für Kante.
#[derive(Debug, Clone, Default)]
pub struct Vergleich {
    /// Genähte Randkanten der Soll-Liste.
    pub soll: usize,
    /// … im Vorschlag an dieselben Gegenkanten genäht.
    pub richtig: usize,
    /// … davon mit umgekehrter Laufrichtung (gleiche Kanten, Enden vertauscht).
    pub verdreht: usize,
    /// … an andere Kanten genäht.
    pub falsch: usize,
    /// … im Vorschlag nicht genäht.
    pub fehlend: usize,
    /// Kanten, die der Vorschlag näht, die Soll-Liste nicht.
    pub zuviel: usize,
    /// Beispiele für fehlend, verdreht, falsch und zu viel.
    pub beispiele: Vec<String>,
}

type Kante = (String, usize);

/// Je Naht beider Seiten Kanten mit ihrem Abschnitt [Anfang, Ende) als
/// Anteil der Seitenlänge, in Nährichtung, aus den echten Kantenlängen.
fn kantenindex(ev: &Auswertung<'_>, quelle: Kurvenquelle, b: &crate::naehte::Nahtbericht) -> Vec<[Vec<(Kante, f64, f64)>; 2]> {
    let mut laengen: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for t in ev.muster().pieces() {
        if let Ok(r) = ev.teilrand(t, quelle) {
            laengen.insert(r.teil.clone(), r.kanten.iter().map(|k| k.laenge).collect());
        }
    }
    b.naehte
        .iter()
        .map(|e| {
            let mut paar: [Vec<(Kante, f64, f64)>; 2] = [Vec::new(), Vec::new()];
            for (s, seite) in [&e.a, &e.b].into_iter().enumerate() {
                let ganz: f64 = seite.teilstuecke.iter().flat_map(|t| t.kanten.iter().map(|&k| laengen[&t.teil][k])).sum();
                let mut weg = 0.0;
                for t in &seite.teilstuecke {
                    for &k in &t.kanten {
                        let l = laengen[&t.teil][k];
                        paar[s].push(((t.teil.clone(), k), weg / ganz.max(1e-12), (weg + l) / ganz.max(1e-12)));
                        weg += l;
                    }
                }
            }
            paar
        })
        .collect()
}

/// Kanten auf der Seite `seite`, die den Abschnitt [x0, x1) überdecken —
/// mit mindestens einem Viertel der kürzeren der beiden Spannen (sonst
/// zählte schon die Rundung an einer Kantengrenze).
fn gegenueber(seite: &[(Kante, f64, f64)], x0: f64, x1: f64) -> Vec<&Kante> {
    seite
        .iter()
        .filter(|(_, a, b)| (b.min(x1) - a.max(x0)) > 0.25 * (b - a).min(x1 - x0))
        .map(|(k, _, _)| k)
        .collect()
}

/// Vergleicht einen Vorschlag `ist` mit der Soll-Liste `soll` (beide über
/// [`crate::naehte::pruefe_naehte`] aufgelöst), nach Lage auf der Naht: Eine
/// Kante ist richtig, wenn ihr im Vorschlag dieselben Gegenkanten
/// gegenüberliegen wie in der Soll-Liste (mindestens eine gemeinsam, keine
/// fremde), verdreht, wenn das erst mit umgekehrter Gegenseite gilt.
pub fn vergleiche(ev: &Auswertung<'_>, quelle: Kurvenquelle, soll: &[Nahtdef], ist: &[Nahtdef]) -> Result<Vergleich, crate::naehte::NahtFehler> {
    let bs = crate::naehte::pruefe_naehte(ev, soll, quelle)?;
    let bi = crate::naehte::pruefe_naehte(ev, ist, quelle)?;
    let is = kantenindex(ev, quelle, &bs);
    let ii = kantenindex(ev, quelle, &bi);
    let mut wo_i: std::collections::HashMap<Kante, (usize, usize, f64, f64)> = std::collections::HashMap::new();
    for (m, paar) in ii.iter().enumerate() {
        for s in 0..2 {
            for (k, a, b) in &paar[s] {
                wo_i.insert(k.clone(), (m, s, *a, *b));
            }
        }
    }
    let mut v = Vergleich::default();
    let mut gesehen: HashSet<Kante> = HashSet::new();
    for (n, paar) in is.iter().enumerate() {
        for s in 0..2 {
            for (k, x0, x1) in &paar[s] {
                gesehen.insert(k.clone());
                v.soll += 1;
                let soll_gegen: HashSet<&Kante> = gegenueber(&paar[1 - s], *x0, *x1).into_iter().collect();
                let Some(&(m, t, y0, y1)) = wo_i.get(k) else {
                    v.fehlend += 1;
                    if v.beispiele.len() < 12 {
                        v.beispiele.push(format!("fehlt  {} Kante {} (Soll {})", k.0, k.1, bs.naehte[n].kennung));
                    }
                    continue;
                };
                let ist_gegen = gegenueber(&ii[m][1 - t], y0, y1);
                let umgekehrt = gegenueber(&ii[m][1 - t], 1.0 - y1, 1.0 - y0);
                let trifft = |g: &[&Kante]| !g.is_empty() && g.iter().all(|x| soll_gegen.contains(x));
                if trifft(&ist_gegen) {
                    v.richtig += 1;
                } else if trifft(&umgekehrt) {
                    v.richtig += 1;
                    v.verdreht += 1;
                    if v.beispiele.len() < 12 {
                        v.beispiele.push(format!("verdreht {} Kante {} (Soll {}, Vorschlag {})", k.0, k.1, bs.naehte[n].kennung, bi.naehte[m].kennung));
                    }
                } else {
                    v.falsch += 1;
                    if v.beispiele.len() < 12 {
                        v.beispiele.push(format!("falsch {} Kante {} (Soll {}, Vorschlag {})", k.0, k.1, bs.naehte[n].kennung, bi.naehte[m].kennung));
                    }
                }
            }
        }
    }
    let mut zuviel: Vec<&Kante> = wo_i.keys().filter(|k| !gesehen.contains(*k)).collect();
    zuviel.sort();
    v.zuviel = zuviel.len();
    for k in zuviel.into_iter().take(12usize.saturating_sub(v.beispiele.len())) {
        v.beispiele.push(format!("zu viel {} Kante {} (Vorschlag {})", k.0, k.1, bi.naehte[wo_i[k].0].kennung));
    }
    Ok(v)
}
