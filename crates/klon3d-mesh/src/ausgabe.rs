//! Ausgabe zur Kontrolle: SVG (Netze und Nahtpunkte in Musterlage), OBJ und
//! Paarliste.

use std::fmt::Write as _;

use crate::Musternetz;

/// Farben für die Nähte, reihum.
const FARBEN: [&str; 8] = ["#d62728", "#1f77b4", "#2ca02c", "#9467bd", "#ff7f0e", "#17becf", "#e377c2", "#8c564b"];

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn rahmen(punkte: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in punkte {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Verschiebung je Teil: in Reihen nebeneinander, die höchsten zuerst, 20 mm
/// Abstand. Im Muster liegen Teile oft übereinander (Rückenteil Mitte und
/// Seite, Kragen auf dem Rücken).
fn reihen(netz: &Musternetz) -> Vec<[f64; 2]> {
    let rahmen: Vec<_> = netz.teile.iter().map(|t| rahmen(&t.punkte)).collect();
    let flaeche: f64 = rahmen.iter().map(|(lo, hi)| (hi[0] - lo[0] + 20.0) * (hi[1] - lo[1] + 20.0)).sum();
    let breiteste = rahmen.iter().map(|(lo, hi)| hi[0] - lo[0]).fold(0.0, f64::max);
    let zeile_max = (1.6 * flaeche.sqrt()).max(breiteste);
    let mut folge: Vec<usize> = (0..rahmen.len()).collect();
    folge.sort_by(|&a, &b| {
        let h = |i: usize| rahmen[i].1[1] - rahmen[i].0[1];
        h(b).total_cmp(&h(a))
    });
    let mut versatz = vec![[0.0; 2]; rahmen.len()];
    let (mut x, mut y, mut zeilenhoehe) = (0.0, 0.0, 0.0f64);
    for i in folge {
        let (lo, hi) = rahmen[i];
        let (b, h) = (hi[0] - lo[0], hi[1] - lo[1]);
        if x > 0.0 && x + b > zeile_max {
            (x, y, zeilenhoehe) = (0.0, y + zeilenhoehe + 20.0, 0.0);
        }
        versatz[i] = [x - lo[0], y - lo[1]];
        x += b + 20.0;
        zeilenhoehe = zeilenhoehe.max(h);
    }
    versatz
}

/// SVG aller Teile (mm, y nach unten wie in Seamly2D): Dreiecke grau, Rand
/// schwarz, Punktknoten als Kreis, Nahtpunkte in der Farbe ihrer Naht mit
/// Kennung an der Nahtmitte. `nebeneinander`: Teile in Reihen statt in
/// Musterlage.
pub fn svg(netz: &Musternetz, nebeneinander: bool) -> String {
    let versatz = if nebeneinander { reihen(netz) } else { vec![[0.0; 2]; netz.teile.len()] };
    let lage = |ti: usize, v: u32| {
        let p = netz.teile[ti].punkte[v as usize];
        [p[0] + versatz[ti][0], p[1] + versatz[ti][1]]
    };
    let alle: Vec<[f64; 2]> =
        (0..netz.teile.len()).flat_map(|ti| (0..netz.teile[ti].punkte.len() as u32).map(move |v| (ti, v))).map(|(ti, v)| lage(ti, v)).collect();
    let (mut lo, mut hi) = rahmen(&alle);
    if !lo[0].is_finite() {
        lo = [0.0; 2];
        hi = [1.0; 2];
    }
    let rand = 20.0;
    let (b, h) = (hi[0] - lo[0] + 2.0 * rand, hi[1] - lo[1] + 2.0 * rand);
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.2} {:.2} {b:.2} {h:.2}" width="{:.0}mm" height="{:.0}mm">"#,
        lo[0] - rand,
        lo[1] - rand,
        b,
        h
    );
    let _ = writeln!(s, r#"<rect x="{:.2}" y="{:.2}" width="{b:.2}" height="{h:.2}" fill="white"/>"#, lo[0] - rand, lo[1] - rand);
    // One arrowhead for the grainlines; `orient="auto"` points it along the
    // line, so both ends of an axis use the same marker.
    let _ = writeln!(
        s,
        r##"<defs><marker id="pfeil" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="4" markerHeight="4" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="#0a7"/></marker></defs>"##
    );
    for (ti, t) in netz.teile.iter().enumerate() {
        let _ = writeln!(s, r#"<g id="{}">"#, xml(&t.teil));
        let mut d = String::new();
        for tri in &t.dreiecke {
            let q = |k: usize| lage(ti, tri[k]);
            let _ = write!(d, "M{:.3} {:.3}L{:.3} {:.3}L{:.3} {:.3}Z", q(0)[0], q(0)[1], q(1)[0], q(1)[1], q(2)[0], q(2)[1]);
        }
        let _ = writeln!(s, r##"<path d="{d}" fill="#f4f4f4" stroke="#9a9a9a" stroke-width="0.15"/>"##);
        let rand: Vec<String> = (0..t.nrand as u32).map(|v| lage(ti, v)).map(|p| format!("{:.3},{:.3}", p[0], p[1])).collect();
        let _ = writeln!(s, r##"<polygon points="{}" fill="none" stroke="#000" stroke-width="0.4"/>"##, rand.join(" "));
        if let Some(f) = &t.fadenlauf {
            // The axis with Seamly2D's arrowheads: `pfeile` 0 puts one at both
            // ends, 1 only at `p2`, 2 only at `p1`.
            let a = [f.p1[0] + versatz[ti][0], f.p1[1] + versatz[ti][1]];
            let b = [f.p2[0] + versatz[ti][0], f.p2[1] + versatz[ti][1]];
            let _ = writeln!(
                s,
                r##"<g class="fadenlauf"><line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" stroke="#0a7" stroke-width="0.35" marker-start="{}" marker-end="{}"/></g>"##,
                a[0],
                a[1],
                b[0],
                b[1],
                if f.pfeile == 1 { "none" } else { "url(#pfeil)" },
                if f.pfeile == 2 { "none" } else { "url(#pfeil)" },
            );
        }
        for (name, v) in &t.punktknoten {
            let p = lage(ti, *v);
            let _ = writeln!(s, r##"<circle cx="{:.3}" cy="{:.3}" r="0.9" fill="none" stroke="#000" stroke-width="0.25"><title>{}</title></circle>"##, p[0], p[1], xml(name));
        }
        let n = t.punkte.len() as f64;
        let m = (0..t.punkte.len() as u32).map(|v| lage(ti, v)).fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
        let kurz = t.teil.rsplit(" - ").next().unwrap_or(&t.teil);
        let _ = writeln!(s, r##"<text x="{:.2}" y="{:.2}" font-size="7" font-family="sans-serif" text-anchor="middle" fill="#555">{}</text>"##, m[0], m[1], xml(kurz));
        let _ = writeln!(s, "</g>");
    }
    for (i, naht) in netz.naehte.iter().enumerate() {
        let farbe = FARBEN[i % FARBEN.len()];
        let _ = writeln!(s, r#"<g id="naht-{}">"#, xml(&naht.kennung));
        let alle: Vec<_> = naht.paare().collect();
        for (tn, pp) in &alle {
            for (ti, v) in [(tn.teil_a, pp.a), (tn.teil_b, pp.b)] {
                let p = lage(ti, v);
                let _ = writeln!(s, r#"<circle cx="{:.3}" cy="{:.3}" r="0.6" fill="{farbe}"/>"#, p[0], p[1]);
            }
        }
        if let Some((tn, pp)) = alle.get(alle.len() / 2) {
            for (ti, v) in [(tn.teil_a, pp.a), (tn.teil_b, pp.b)] {
                let p = lage(ti, v);
                let _ = writeln!(s, r#"<text x="{:.2}" y="{:.2}" font-size="5" font-family="sans-serif" fill="{farbe}">{}</text>"#, p[0] + 1.5, p[1] - 1.5, xml(&naht.kennung));
            }
        }
        let _ = writeln!(s, "</g>");
    }
    s.push_str("</svg>\n");
    s
}

/// OBJ aller Teile in der Ebene z = 0 (mm). Die y-Achse ist gespiegelt,
/// damit das Bild wie in Seamly2D steht; die Dreiecke laufen dann gegen den
/// Uhrzeigersinn, die Normalen zeigen nach +z.
pub fn obj(netz: &Musternetz) -> String {
    let mut s = String::from("# klon3d Stufe 3: Schnittteile als Dreiecksnetz (mm, y gespiegelt)\n");
    let mut basis = 1;
    for t in &netz.teile {
        let _ = writeln!(s, "o {}", t.teil.replace(' ', "_"));
        for p in &t.punkte {
            let _ = writeln!(s, "v {:.5} {:.5} 0", p[0], -p[1]);
        }
        for d in &t.dreiecke {
            let _ = writeln!(s, "f {} {} {}", basis + d[0] as usize, basis + d[2] as usize, basis + d[1] as usize);
        }
        basis += t.punkte.len();
    }
    s
}

/// Paarliste: je Zeile Naht, Teilnaht, Teil A, Punkt A, Teil B, Punkt B,
/// ρ_A, ρ_B (tabulatorgetrennt, Punkte ab 0 im Teilnetz).
pub fn paarliste(netz: &Musternetz) -> String {
    let mut s = String::from("# klon3d Nahtpaare\n# Naht\tTeilnaht\tTeil A\tPunkt A\tTeil B\tPunkt B\trho_A\trho_B\n");
    for n in &netz.naehte {
        for (k, tn) in n.teilnaehte.iter().enumerate() {
            for p in &tn.paare {
                let _ = writeln!(
                    s,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{:.6}\t{:.6}",
                    n.kennung,
                    k + 1,
                    netz.teile[tn.teil_a].teil,
                    p.a,
                    netz.teile[tn.teil_b].teil,
                    p.b,
                    p.rho_a,
                    p.rho_b
                );
            }
        }
    }
    s
}
