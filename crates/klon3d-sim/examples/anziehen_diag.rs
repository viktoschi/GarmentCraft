//! Diagnose der Platzierung und des Anziehens (Stufe 4c, Befunde 95–97) an
//! einem Prüfmuster (Vorgabe Bluse3 v2): Hüllkörper, Anker, Dehnung und Teile im Körper in der
//! Anfangslage, dann anziehen ([`klon3d_sim::platzierung::anziehen`]) mit
//! Geschwindigkeit, Nahtspalt und Kontakten je Bild; am Ende Nahtabstände je
//! Naht, Stoffkanten durch den Körper; OBJ der Anfangs- und Endlage.
//!
//! `cargo run --release --example anziehen_diag -- [an|aus] [ausgabeordner]`
//! (`aus`: nur platzieren)
//!
//! Umgebung: `NAEHEN=<m/s>`, `NAEHBILDER=<n>`, `RUHEBILDER=<n>` (höchstens so viele Bilder Einschwingen),
//! `KOERPERPUNKTE=<Durchgänge>` (0 = ohne Körperpunkt gegen Stoffdreieck),
//! `SCHWERKRAFT=<Anteil beim Nähen>`, `NAHT_K=<N/m>` ändern den Ablauf; `NAHT=<Kennung>` listet
//! am Ende alle Paare dieser Naht; `MUSTER=<Name>` wählt das Prüfmuster
//! (`<Name>.sm2d`, `.naehte`, `.lage` in den Prüfmustern).

#[path = "../tests/band/mod.rs"]
mod band;

use std::path::PathBuf;

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::platzierung::{anziehen, lies_lage, platziere, Anziehen};
use klon3d_sim::{ausgabe, messung, ImplizitCg, ImplizitParameter, Koerper, Simparameter, Szene, Teilvorgabe, RADICAL_RUBBER_04};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let anziehen_an = a.first().map(String::as_str) != Some("aus");
    let aus = a.get(1).map(PathBuf::from);
    let (k, h) = band::pruefkoerper().expect("Körper");
    for (s, z) in h.arme.iter().enumerate() {
        println!(
            "Arm {}: Schulter {:?} cm, Achse {:?}, Umfang des Prismas über 0–21 cm {:.1} cm",
            ["links", "rechts"][s],
            z.schulter.map(|v| (v * 1000.0).round() / 10.0),
            z.achse.map(|v| (v * 1000.0).round() / 1000.0),
            klon3d_sim::platzierung::umfang(&z.umriss(0.0, 0.21, 0.0)) * 100.0
        );
    }
    println!("Hals: Mitte {:?} cm, Radius {:.1} cm; Höhen {:?}", h.hals_mitte.map(|v| (v * 1000.0).round() / 10.0), h.hals_radius * 100.0, h.hoehen);

    let w = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster"));
    let name = std::env::var("MUSTER").unwrap_or_else(|_| "bluse3_v2".into());
    let m = klon3d_pattern::read_path(&w.join(format!("{name}.sm2d"))).unwrap();
    let ev = Auswertung::lade(&m, &w.join("measurements.smis")).unwrap();
    let defs = lies_nahtliste(&std::fs::read_to_string(w.join(format!("{name}.naehte"))).unwrap()).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).unwrap();
    let liste = lies_lage(&std::fs::read_to_string(w.join(format!("{name}.lage"))).unwrap()).unwrap();
    let p = platziere(&netz, &liste, &h).unwrap();
    println!(
        "platziert {} Teile, {} Nähte; weggelassen {:?}, Nähte {:?}; Rumpfprisma {:.1}–{:.1} cm",
        p.netz.teile.len(),
        p.netz.naehte.len(),
        p.weggelassen,
        p.naehte_weggelassen,
        p.rumpf_hoehen.0 * 100.0,
        p.rumpf_hoehen.1 * 100.0
    );

    let u = p.rumpfumriss(0);
    println!("Rumpfprisma: {} Ecken, Umfang {:.1} cm", u.len(), klon3d_sim::platzierung::umfang(u) * 100.0);
    for (ti, t) in p.netz.teile.iter().enumerate() {
        let (q, um, hoch) = p.anker(ti);
        let l: Vec<[f64; 3]> = t.punkte.iter().map(|&x| p.lage(ti, x)).collect();
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for x in &l {
            for k in 0..3 {
                lo[k] = lo[k].min(x[k]);
                hi[k] = hi[k].max(x[k]);
            }
        }
        println!(
            "  {:<40} Anker ({:.0}, {:.0}) um {:.3} hoch {:.3}  x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0} cm",
            t.teil, q[0], q[1], um, hoch, lo[0] * 100.0, hi[0] * 100.0, lo[1] * 100.0, hi[1] * 100.0, lo[2] * 100.0, hi[2] * 100.0
        );
    }
    let vg = Teilvorgabe { material: RADICAL_RUBBER_04, reduktion: 0.0 };
    let param = Simparameter { schwerkraft: [0.0, 0.0, -9.81], daempfung: 1.0, ..Simparameter::default() };
    let mut s = Szene::aus_netz(&p.netz, &|_| vg, &|ti, q| p.lage(ti, q), vec![Koerper::netz(k.clone())], param);
    // Nahtabstand je Naht.
    let je_naht = |s: &Szene| {
        let alle: Vec<f64> = messung::nahtabstaende(s, &p.netz).into_iter().flat_map(|(_, d)| d).collect();
        let zu = alle.iter().filter(|&&x| x <= 0.01).count();
        println!("  {zu} von {} Nahtpaaren bis 10 mm ({:.1} %)", alle.len(), 100.0 * zu as f64 / alle.len() as f64);
        for (kennung, mut d) in messung::nahtabstaende(s, &p.netz) {
            let offen: Vec<String> = d
                .iter()
                .enumerate()
                .filter(|(_, &v)| v > 0.01)
                .map(|(k, &v)| format!("{k}:{:.0}", v * 1e3))
                .collect();
            d.sort_by(f64::total_cmp);
            println!(
                "  {:<5} Abstand Median {:6.1} mm, größter {:6.1} mm; über 10 mm (Paar:mm von {}) {}",
                kennung,
                d[d.len() / 2] * 1e3,
                d[d.len() - 1] * 1e3,
                d.len(),
                offen.join(" ")
            );
        }
    };
    if !anziehen_an {
        je_naht(&s);
    }
    // Dehnung der Anfangslage je Teil (größte Hauptdehnung der Dreiecke).
    {
        let d = messung::dehnung_je_dreieck(&s);
        for t in &s.teile {
            let mut g: Vec<f64> = d[t.dreiecke.clone()].iter().map(|x| x.0).collect();
            let mut k: Vec<f64> = d[t.dreiecke.clone()].iter().map(|x| x.1).collect();
            g.sort_by(f64::total_cmp);
            k.sort_by(f64::total_cmp);
            println!("  Dehnung {:<40} größte: Median {:+.2} %, max {:+.2} %; kleinste: min {:+.2} %", t.name, g[g.len() / 2] * 100.0, g[g.len() - 1] * 100.0, k[0] * 100.0);
        }
    }
    let drin = |s: &Szene| s.x.iter().map(|&q| k.naechster_alle(q).abstand).fold(f64::INFINITY, f64::min);
    for t in &s.teile {
        let d: Vec<f64> = s.x[t.punkte.clone()].iter().map(|&q| k.naechster_alle(q).abstand).collect();
        let innen = d.iter().filter(|&&v| v < 0.0).count();
        if innen > 0 {
            let (i, v) = d.iter().enumerate().fold((0, f64::INFINITY), |m, (i, &v)| if v < m.1 { (i, v) } else { m });
            let q = s.x[t.punkte.start + i];
            println!("  innen: {:<40} {innen:4} Punkte, tiefster {:+.1} mm bei ({:.1}, {:.1}, {:.1}) cm", t.name, v * 1e3, q[0] * 100.0, q[1] * 100.0, q[2] * 100.0);
        }
    }
    println!("Anfang: {} Punkte, tiefster Punkt {:+.1} mm", s.x.len(), drin(&s) * 1e3);
    if let Some(o) = &aus {
        std::fs::create_dir_all(o).unwrap();
        std::fs::write(o.join("anfang.obj"), ausgabe::obj(&s)).unwrap();
        let mut kobj = String::new();
        for q in &k.punkte {
            kobj += &format!("v {:.2} {:.2} {:.2}
", q[0] * 1e3, q[1] * 1e3, q[2] * 1e3);
        }
        for d in &k.dreiecke {
            kobj += &format!("f {} {} {}
", d[0] + 1, d[1] + 1, d[2] + 1);
        }
        std::fs::write(o.join("koerper.obj"), kobj).unwrap();
    }

    if !anziehen_an {
        return;
    }
    let zahl = |n: &str| std::env::var(n).ok().and_then(|v| v.parse::<f64>().ok());
    let mut ablauf = Anziehen::default();
    if let Some(v) = zahl("NAEHEN") {
        ablauf.naht_geschwindigkeit = v;
    }
    if let Some(v) = zahl("NAEHBILDER") {
        ablauf.naehbilder = v as usize;
    }
    if let Some(v) = zahl("RUHEBILDER") {
        ablauf.einschwingen_max = v as usize;
    }
    if let Some(v) = zahl("KOERPERPUNKTE") {
        ablauf.koerperpunkt_durchgaenge = v as usize;
    }
    if let Some(v) = zahl("SCHWERKRAFT") {
        ablauf.schwerkraft_naehen = v;
    }
    if let Some(v) = zahl("NAHT_K") {
        ablauf.naht_k = v;
    }
    println!("Anziehen: {ablauf:?}");
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    anziehen(&mut s, &mut rw, &ablauf, &mut |bild, s, rw| {
        if bild <= 3 || bild % 50 == 0 {
            let mut spalt: Vec<f64> = s.naht.punkte.iter().map(|&[i, j]| {
                let (i, j) = (i as usize, j as usize);
                let e = [s.x[i][0] - s.x[j][0], s.x[i][1] - s.x[j][1], s.x[i][2] - s.x[j][2]];
                (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt()
            }).collect();
            spalt.sort_by(f64::total_cmp);
            let st = rw.statistik;
            println!(
                "Bild {bild:4}: v {:.3} m/s, Naht Median {:.1} mm, 95 % {:.1} mm, größter {:.1} mm; Kontakte {}, Durchstöße {}, Körperpunkte {}, CG nicht konv. {}/{}",
                messung::mittlere_geschwindigkeit(s),
                spalt[spalt.len() / 2] * 1e3,
                spalt[spalt.len() * 95 / 100] * 1e3,
                spalt[spalt.len() - 1] * 1e3,
                st.kontakte,
                st.durchstoesse,
                st.koerperpunkte,
                st.nicht_konvergiert,
                st.loesungen
            );
        }
    });
    {
        println!("Ende: tiefster Punkt {:+.2} mm", drin(&s) * 1e3);
        je_naht(&s);
        if let Some(o) = &aus {
            std::fs::write(o.join("ende.obj"), ausgabe::obj(&s)).unwrap();
        }
        // Stoffkanten, die den Körper kreuzen (Durchstoß von einem Ende aus).
        let kreuzt = |a: [f64; 3], b: [f64; 3]| k.durchstoss(a, b, [0.0; 3]).is_some() || k.durchstoss(b, a, [0.0; 3]).is_some();
        let kanten = s.abstand.punkte.iter().filter(|&&[i, j]| kreuzt(s.x[i as usize], s.x[j as usize])).count();
        // Tiefe: fünf Stellen auf der Kante, Abstand mit Vorzeichen.
        let tief = |a: [f64; 3], b: [f64; 3]| {
            (1..6)
                .map(|m| {
                    let u = m as f64 / 6.0;
                    let q = [a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1]), a[2] + u * (b[2] - a[2])];
                    k.naechster(q, 0.02).map_or(0.0, |n| n.abstand)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let tiefe = s.abstand.punkte.iter().filter(|&&[i, j]| tief(s.x[i as usize], s.x[j as usize]) < -1e-3).count();
        println!("Stoffkanten durch den Körper: {kanten} von {}, davon tiefer als 1 mm: {tiefe}", s.abstand.len());
        // Wo der Stoff am Ende noch läuft: Geschwindigkeit je Teil.
        for t in &s.teile {
            let v: Vec<f64> = s.v[t.punkte.clone()].iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).collect();
            let mittel = v.iter().sum::<f64>() / v.len() as f64;
            println!("  Geschwindigkeit {:<40} Mittel {:.4} m/s, größte {:.4} m/s", t.name, mittel, v.iter().cloned().fold(0.0, f64::max));
        }
        // NAHT=<Kennung>: alle Paare dieser Naht der Reihe nach.
        if let Ok(kennung) = std::env::var("NAHT") {
            for n in p.netz.naehte.iter().filter(|n| n.kennung == kennung) {
                for (k, (tn, pp)) in n.paare().enumerate() {
                    let (i, j) = (s.teile[tn.teil_a].punkte.start + pp.a as usize, s.teile[tn.teil_b].punkte.start + pp.b as usize);
                    let e = [s.x[i][0] - s.x[j][0], s.x[i][1] - s.x[j][1], s.x[i][2] - s.x[j][2]];
                    // Kanten am Punkt A, die den Körper kreuzen.
                    let an_a = s.abstand.punkte.iter().filter(|&&[u, w]| (u as usize == i || w as usize == i) && kreuzt(s.x[u as usize], s.x[w as usize])).count();
                    println!(
                        "    {k:3} A {}#{} ({:.1}, {:.1}, {:.1}) B {}#{} ({:.1}, {:.1}, {:.1}) {:.1} mm; A→B durch den Körper {}, Kanten an A durch den Körper {an_a}",
                        tn.teil_a, pp.a, s.x[i][0] * 100.0, s.x[i][1] * 100.0, s.x[i][2] * 100.0,
                        tn.teil_b, pp.b, s.x[j][0] * 100.0, s.x[j][1] * 100.0, s.x[j][2] * 100.0,
                        (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt() * 1e3,
                        kreuzt(s.x[i], s.x[j])
                    );
                }
            }
        }
    }
}
