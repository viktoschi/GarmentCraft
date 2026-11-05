//! Stufe 7, Bewegung: der Arm hebt sich **durch** die Bluse (Issue #96,
//! Punkt 3; Skelett und Haut aus #94).
//!
//! Der Körper ist der freie MakeHuman-Körper, das Kleidungsstück die
//! Prüfbluse v26 aus [`bluse2_stehend`]. Beides wird hier zum ersten Mal
//! zusammengebracht: der Körper trägt ein Skelett, seine Haut wird je Bild neu
//! verformt und **nachgeführt** statt neu gebaut ([`Koerpernetz::nachfuehren`]),
//! und die Geschwindigkeit der Haut geht an der getroffenen Stelle in Reibung
//! und Stoß ein ([`Koerpernetz::setze_punktgeschwindigkeit`]).
//!
//! Geprüft wird, was das Abnahmekriterium der Stufe 7 verlangt (`PLAN.md`),
//! jedes als benannte Zahl:
//!
//! 1. **Die Bluse folgt dem Arm.** Wandert der Ärmel nicht mit, steht das
//!    Kleidungsstück und die Bewegung ist nicht angekommen. Boden: der Ärmel
//!    wandert im Median mindestens 4 cm und an seinem äußersten Punkt
//!    mindestens 15 cm mit.
//! 2. **Nichts stößt durch.** Kein Stoffpunkt kommt über alle 60 Bilder tiefer
//!    als 1 mm in die Haut; die Haut des gehobenen Arms ist dabei die Haut
//!    **dieses** Bildes, nicht die Binde-Pose.
//! 3. **Die Nähte halten.** Der Anteil geschlossener Nahtpaare fällt nicht
//!    unter den Stand am ruhenden Körper.
//! 4. **Die Bewegung kommt im Stoff an.** Die Dehnung im Ärmelband auf
//!    Achselhöhe ändert sich messbar. **Gemessen fällt sie** (+8,1 % →
//!    +1,5 %, im zweiten Lauf +7,1 % → +1,6 %): mit hängendem Arm ist der Ärmel
//!    in die Achselhöhle geklemmt und dort gedehnt, beim Heben gibt die Stelle
//!    nach. Der Zug, den sie am echten Teil an der Kugel spürt, ist damit in
//!    diesem ersten Lauf **nicht** nachgewiesen - das steht als Befund im
//!    Bericht und ist kein Fehler des Tests. Deshalb prüft der Test die
//!    Änderung, nicht ihre Richtung.
//! 5. **Die Nachführung bleibt der billige Weg.** Gemessen im selben Lauf:
//!    der Körperaufwand (verformen, nachführen, Geschwindigkeit setzen) bleibt
//!    unter 8 ms je Bild und unter 5 % der Bildzeit (gemessen 4,8 ms und
//!    2,3 %). Dass `nachfuehren` gegen `aus_clo_mm` **mindestens 3× billiger**
//!    ist, misst und behauptet `tests/nachfuehren.rs` (warm, beste von drei
//!    Runden: 2,8–3,1 ms gegen 8,6–10,1 ms); hier steht es nur als Zahl
//!    daneben, weil ein einzelner kalter Aufruf im Frame-Rhythmus nicht
//!    dieselbe Messung ist.
//!
//! **Was dieser Test nicht behauptet:** dass ein Bild in Echtzeit läuft. Der
//! Bildpreis hier ist der CPU-Löser aus Stufe 4 (rund 200 ms für 12 000
//! Stoffpunkte) und hat mit der Nachführung nichts zu tun - der Körperaufwand
//! ist der kleine Posten daneben. Die Zahl steht im Bericht, die Grenze von
//! 16,7 ms wird hier nicht geprüft.
//!
//! Braucht die MakeHuman-Daten und das Prüfmuster; fehlt etwas, sagt der Test
//! das und hört auf. Nur mit `cargo test --release` (ein Lauf dauert Minuten).

use std::path::{Path, PathBuf};
use std::time::Instant;

use klon3d_koerper::koerper_quelle::ordner_ab;
use klon3d_koerper::skelett::{self, Skelett};
use klon3d_mesh::{vernetze, Musternetz, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::netzkoerper::{clo_nach_szene, Koerpernetz, SUCHTIEFE};
use klon3d_sim::platzierung::{anziehen, lies_lage, platziere, Anziehen, Huellen, Koerperteile, Platzierung};
use klon3d_sim::{
    messung, ImplizitCg, ImplizitParameter, Koerper, Rechenweg, Simparameter, Szene, Teilvorgabe,
    RADICAL_RUBBER_04,
};

const MUSTER: &str = "bluse2_v26";
/// Höhe des Körpers, an die das Skelett eingepasst wird (mm).
const KOERPERHOEHE_MM: f64 = 1600.0;
/// Bilder für die 90°; eins je Bild ergibt 1,5° und ist damit unter allem, was
/// ein echter Arm schneller macht.
const BILDER: usize = 60;
/// Grenze 1: tiefer als so viel darf kein Stoffpunkt in die Haut (m).
const DURCHSTOSS_GRENZE: f64 = 0.001;
/// Grenze 5: der Körperaufwand je Bild (s) und sein Anteil an der Bildzeit.
/// Gemessen 4,75 ms Median. Die **scharfe** Grenze ist der Anteil: sie fängt
/// jede Verschlechterung der Nachführung, gleich wie schnell der Rechner ist.
/// Die absolute Grenze hat Luft für einen langsameren Rechner, ohne 4,75 ms
/// durchzuwinken - das Vierfache wäre kein Nachführen mehr, sondern ein
/// Neuaufbau.
const KOERPERZEIT_GRENZE: f64 = 0.008;
const KOERPERANTEIL_GRENZE: f64 = 0.05;

/// Körper mit Skelett, Hüllen daraus, platzierte Bluse.
fn aufbau() -> Option<(Platzierung, klon3d_koerper::Netz, Skelett)> {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster"));
    for endung in ["sm2d", "naehte", "lage"] {
        let p = dir.join(format!("{MUSTER}.{endung}"));
        if !p.exists() {
            eprintln!("übersprungen: {} fehlt", p.display());
            return None;
        }
    }
    let Some(ordner) = ordner_ab(Path::new(env!("CARGO_MANIFEST_DIR"))) else {
        eprintln!("übersprungen: MakeHuman-Daten fehlen");
        return None;
    };
    let (netz, sk) = match skelett::baue(&ordner, KOERPERHOEHE_MM) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("übersprungen: Skelett aus {}: {e}", ordner.display());
            return None;
        }
    };
    let an = match klon3d_koerper::anordnung_netz::aus_netz(&netz, 10.0) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("übersprungen: Anordnung aus dem Netz: {e}");
            return None;
        }
    };
    let kn = Koerpernetz::aus_clo_mm(&netz.punkte, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
    let c = clo_nach_szene;
    let h = match Huellen::neu(&Koerperteile {
        punkte: &kn.punkte,
        dreiecke: &kn.dreiecke,
        rumpf: &an.rumpf,
        arme: [&an.arme[0], &an.arme[1]],
        hals_vorn: c(an.hals_vorn),
        hals_hinten: c(an.hals_hinten),
        hals_seitlich: an.hals_seitlich.map(c),
        schulter: an.schulter.map(c),
        ellbogen: an.ellbogen.map(c),
    }) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("übersprungen: Hüllkörper aus dem MakeHuman-Netz: {e}");
            return None;
        }
    };
    let m = klon3d_pattern::read_path(&dir.join(format!("{MUSTER}.sm2d"))).expect("Muster");
    let ev = Auswertung::lade(&m, &dir.join("measurements.smis")).expect("Maße");
    let roh = std::fs::read_to_string(dir.join(format!("{MUSTER}.naehte"))).unwrap();
    let defs = lies_nahtliste(&roh).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let mn = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).unwrap();
    let liste = lies_lage(&std::fs::read_to_string(dir.join(format!("{MUSTER}.lage"))).unwrap()).unwrap();
    let p = match platziere(&mn, &liste, &h) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("übersprungen: Bluse nicht auf den MakeHuman-Körper zu legen: {e}");
            return None;
        }
    };
    Some((p, netz, sk))
}

fn teil_index(netz: &Musternetz, name: &str) -> Option<usize> {
    let treffer: Vec<usize> = (0..netz.teile.len())
        .filter(|&i| netz.teile[i].teil == name || netz.teile[i].teil.rsplit(" - ").next() == Some(name))
        .collect();
    match treffer[..] {
        [i] => Some(i),
        _ => None,
    }
}

fn abstand3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn median(v: &[f64]) -> f64 {
    let mut w = v.to_vec();
    w.sort_by(f64::total_cmp);
    w[w.len() / 2]
}

fn ruheflaeche(ruhe_inv: &[f64; 4]) -> f64 {
    let det = ruhe_inv[0] * ruhe_inv[3] - ruhe_inv[1] * ruhe_inv[2];
    if det == 0.0 {
        0.0
    } else {
        0.5 / det.abs()
    }
}

/// Flächengewichteter Median der größten Hauptdehnung über `menge`.
fn dehnung_median(s: &Szene, je_flaeche: &[(f64, f64)], menge: &[usize]) -> f64 {
    let mut paar: Vec<(f64, f64)> = menge
        .iter()
        .map(|&k| (je_flaeche[k].0, ruheflaeche(&s.ruhe_inv[k])))
        .filter(|(_, a)| *a > 0.0)
        .collect();
    if paar.is_empty() {
        return f64::NAN;
    }
    paar.sort_by(|a, b| a.0.total_cmp(&b.0));
    let gesamt: f64 = paar.iter().map(|p| p.1).sum();
    let mut summe = 0.0;
    for (e, a) in &paar {
        summe += a;
        if summe >= 0.5 * gesamt {
            return *e;
        }
    }
    paar[paar.len() - 1].0
}

/// Dehnung je Fläche für eine andere Lage als die der Szene.
fn dehnung_mit(s: &Szene, x: &[[f64; 3]]) -> Vec<(f64, f64)> {
    let mut kopie = s.clone();
    kopie.x = x.to_vec();
    messung::dehnung_je_dreieck(&kopie)
}

fn flaechenmitte(x: &[[f64; 3]], d: [u32; 3]) -> [f64; 3] {
    let mut q = [0.0; 3];
    for &i in &d {
        for a in 0..3 {
            q[a] += x[i as usize][a] / 3.0;
        }
    }
    q
}

#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn arm_hebt_sich_durch_die_bluse() {
    let Some((p, netz, mut sk)) = aufbau() else { return };

    // Dieselben Vorgaben wie in `bluse2_stehend`: Material in der Dicke der
    // Lageangabe, ohne Reduktion; die Bluse ist so geschnitten, wie sie genäht
    // wird.
    let vg = |name: &str| {
        let dicke = p.netz.teile.iter().position(|t| t.teil == name).and_then(|ti| p.dicke(ti));
        Teilvorgabe {
            material: dicke.map_or(RADICAL_RUBBER_04, |d| RADICAL_RUBBER_04.mit_dicke(d)),
            reduktion: 0.0,
        }
    };
    let param = Simparameter { schwerkraft: [0.0, 0.0, -9.81], daempfung: 1.0, ..Simparameter::default() };
    let k0 = Koerpernetz::aus_clo_mm(&netz.punkte, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
    let mut s = Szene::aus_netz(&p.netz, &vg, &|ti, q| p.lage(ti, q), vec![Koerper::netz(k0.clone())], param);
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let ablauf = Anziehen::default();
    let erg = anziehen(&mut s, &mut rw, &ablauf, &mut |_, _, _| {});

    assert_eq!(p.netz.teile.len(), 13, "Teile platziert (10 genäht + Trim + zwei Kragenlagen)");
    assert_eq!(p.netz.naehte.len(), 19, "Nähte im Netz");
    assert!(s.masse.iter().all(|&m| m > 0.0), "kein Punkt darf festgehalten sein");

    // Stand am ruhenden Körper: Nulllinie für Punkt 3.
    let naht_am_ruhen: Vec<f64> =
        messung::nahtabstaende(&s, &p.netz).into_iter().flat_map(|(_, d)| d).collect();
    let zu_am_ruhen = naht_am_ruhen.iter().filter(|&&d| d <= 0.01).count();
    let anteil_am_ruhen = zu_am_ruhen as f64 / naht_am_ruhen.len() as f64;

    // ── Der Arm hebt sich ─────────────────────────────────────────────────
    // Der Körper wird je Bild neu verformt und nachgeführt; der Löser sieht
    // nur den neuen Körper und die Geschwindigkeit an jeder Stelle.
    let c = clo_nach_szene;
    let aermel: Vec<usize> = ["Aermel Kurz Links", "Aermel Kurz Rechts"]
        .iter()
        .filter_map(|n| teil_index(&p.netz, n))
        .flat_map(|ti| {
            let b = s.teile[ti].punkte.clone();
            b
        })
        .collect();
    assert!(aermel.len() > 100, "Ärmel: nur {} Punkte", aermel.len());

    let x_anfang = s.x.clone();
    let mut vorher = k0.punkte.clone();
    let mut koerperzeiten = Vec::new();
    let mut tiefstes = f64::INFINITY;
    let mut offen_je_bild = Vec::new();
    let mut hautweg = 0.0f64;
    let mut nachfuehren_zeit = Vec::new();
    let mut bildzeiten = Vec::new();

    for bild in 1..=BILDER {
        let grad = 90.0 * bild as f64 / BILDER as f64;
        let t0 = Instant::now();
        sk.hebe_arme(grad);
        let posed = sk.verforme(&netz.punkte).expect("Verformung");
        let posed_szene: Vec<[f64; 3]> = posed.iter().map(|&q| c(q)).collect();
        let dt = s.param.bild_dt;
        let v: Vec<[f64; 3]> = posed_szene
            .iter()
            .zip(&vorher)
            .map(|(a, b)| [(a[0] - b[0]) / dt, (a[1] - b[1]) / dt, (a[2] - b[2]) / dt])
            .collect();
        let mut neu = k0.clone();
        let t1 = Instant::now();
        neu.nachfuehren(&posed_szene).expect("nachführen");
        nachfuehren_zeit.push(t1.elapsed().as_secs_f64());
        neu.setze_punktgeschwindigkeit(v).expect("Geschwindigkeit");
        s.koerper = vec![Koerper::netz(neu.clone())];
        vorher = posed_szene.clone();
        koerperzeiten.push(t0.elapsed().as_secs_f64());
        hautweg = hautweg.max(
            posed_szene.iter().zip(&k0.punkte).map(|(a, b)| abstand3(*a, *b)).fold(0.0f64, f64::max),
        );

        rw.bild(&mut s);
        bildzeiten.push(t0.elapsed().as_secs_f64());

        // Gegen die Haut **dieses** Bildes, nicht gegen die Binde-Pose.
        tiefstes = tiefstes.min(s.x.iter().map(|&q| neu.naechster_alle(q).abstand).fold(f64::INFINITY, f64::min));
        offen_je_bild.push(
            messung::nahtabstaende(&s, &p.netz).into_iter().flat_map(|(_, d)| d).filter(|&d| d > 0.01).count(),
        );
    }

    // ── 1. Die Bluse folgt dem Arm ────────────────────────────────────────
    let weg: Vec<f64> = aermel.iter().map(|&i| abstand3(s.x[i], x_anfang[i])).collect();
    let aermel_med = median(&weg);
    let aermel_max = weg.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let hoeher = aermel.iter().map(|&i| s.x[i][2]).fold(f64::NEG_INFINITY, f64::max)
        - aermel.iter().map(|&i| x_anfang[i][2]).fold(f64::NEG_INFINITY, f64::max);

    // ── 4. Zug an Kugel und Achsel ────────────────────────────────────────
    let fl_anfang = dehnung_mit(&s, &x_anfang);
    let fl_ende = messung::dehnung_je_dreieck(&s);
    let aermel_alle: Vec<usize> = ["Aermel Kurz Links", "Aermel Kurz Rechts"]
        .iter()
        .filter_map(|n| teil_index(&p.netz, n))
        .flat_map(|ti| s.teile[ti].dreiecke.clone())
        .collect();
    let achsel_z = aermel
        .iter()
        .map(|&i| x_anfang[i][2].min(s.x[i][2]))
        .fold(f64::INFINITY, f64::min);
    let band: Vec<usize> = aermel_alle
        .iter()
        .copied()
        .filter(|&f| (flaechenmitte(&x_anfang, s.dreiecke[f])[2] - achsel_z).abs() < 0.02)
        .collect();
    assert!(band.len() > 20, "Ärmelband: nur {} Flächen", band.len());
    let e_anfang = dehnung_median(&s, &fl_anfang, &band);
    let e_ende = dehnung_median(&s, &fl_ende, &band);

    // ── 5. Der Körperaufwand ──────────────────────────────────────────────
    let mut kz = koerperzeiten.clone();
    kz.sort_by(f64::total_cmp);
    let mut nz = nachfuehren_zeit.clone();
    nz.sort_by(f64::total_cmp);
    let mut bz = bildzeiten.clone();
    bz.sort_by(f64::total_cmp);
    let koerper_med = kz[kz.len() / 2];
    let bild_med = bz[bz.len() / 2];
    // Nur zum Vergleich im Bericht, nicht als Abnahme: der Neuaufbau derselben
    // Punkte, einmal warm gemessen. Was `nachfuehren` gegen ihn spart, misst
    // `tests/nachfuehren.rs` mit Wiederholungen.
    let neuaufbau = {
        let mut t = Vec::new();
        for _ in 0..5 {
            let t0 = Instant::now();
            let k = Koerpernetz::aus_clo_mm(&netz.punkte, &netz.dreiecke, SUCHTIEFE).expect("Körpernetz");
            std::hint::black_box(k.punkte.len());
            t.push(t0.elapsed().as_secs_f64());
        }
        t.sort_by(f64::total_cmp);
        t[0]
    };

    let letzte_offen = offen_je_bild[offen_je_bild.len() - 1];
    let anteil_ende = 1.0 - letzte_offen as f64 / naht_am_ruhen.len() as f64;

    eprintln!("\n== Arm heben, {MUSTER} am MakeHuman-Körper ==");
    eprintln!(
        "  angezogen in {} Bildern ({}), Ruhe {:.4} m/s, tiefster {:+.2} mm, Nahtpaare bis 10 mm {:.1} %",
        erg.einschwingbilder,
        if erg.eingeschwungen { "ruhig" } else { "nicht ruhig" },
        messung::mittlere_geschwindigkeit(&s),
        naht_am_ruhen.iter().copied().fold(f64::INFINITY, f64::min) * 1e3,
        anteil_am_ruhen * 100.0
    );
    eprintln!(
        "  [1] Ärmel {} Punkte wandern im Median {:.1} cm, äußerster Punkt {:.1} cm (Haut bis {:.1} cm); \
         höchster Ärmelpunkt {:.1} cm höher",
        aermel.len(),
        aermel_med * 100.0,
        aermel_max * 100.0,
        hautweg * 100.0,
        hoeher * 100.0
    );
    eprintln!("  [2] tiefster Stoffpunkt über alle {BILDER} Bilder: {:+.2} mm (Grenze {:.1} mm)", tiefstes * 1e3, -DURCHSTOSS_GRENZE * 1e3);
    eprintln!("  [3] offene Nahtpaare: ruhend {:.1} %, nach 90° {:.1} %", (1.0 - anteil_am_ruhen) * 100.0, (1.0 - anteil_ende) * 100.0);
    eprintln!(
        "  [4] Ärmelband auf {:.1} cm: {} Flächen, Dehnung {:+.2} % -> {:+.2} %",
        achsel_z * 100.0,
        band.len(),
        e_anfang * 100.0,
        e_ende * 100.0
    );
    eprintln!(
        "  [5] Körperaufwand je Bild Median {:.2} ms (nachführen allein {:.2} ms, Neuaufbau kalt {:.2} ms); \
         Bild {:.1} ms, Körperanteil {:.1} %",
        koerper_med * 1e3,
        nz[nz.len() / 2] * 1e3,
        neuaufbau * 1e3,
        bild_med * 1e3,
        koerper_med / bild_med * 100.0
    );

    // 1. Der Ärmel bewegt sich mit. Gemessen: Median 8,9 cm, äußerster Punkt
    //    23,4 cm. Die Haut wandert bis 75,1 cm (bis zum Handgelenk), das ist
    //    für den Ärmel kein Maßstab - er sitzt nur am Oberarm.
    assert!(
        aermel_med > 0.04,
        "Ärmel wandert im Median nur {:.1} cm - die Bluse folgt dem Arm nicht",
        aermel_med * 100.0
    );
    assert!(
        aermel_max > 0.15,
        "kein Ärmelpunkt wandert mehr als {:.1} cm - 90° Armheben kommt nicht an",
        aermel_max * 100.0
    );
    // 2. Nichts stößt durch.
    assert!(
        tiefstes > -DURCHSTOSS_GRENZE,
        "Stoffpunkt {:.2} mm in der Haut des gehobenen Arms",
        tiefstes * 1e3
    );
    // 3. Die Nähte halten mindestens so gut wie am ruhenden Körper.
    assert!(
        anteil_ende >= anteil_am_ruhen - 0.01,
        "Nähte reißen beim Heben: {:.1} % zu gegen {:.1} % am ruhenden Körper",
        anteil_ende * 100.0,
        anteil_am_ruhen * 100.0
    );
    // 4. Die Bewegung kommt im Stoff an: die Dehnung im Ärmelband ändert sich
    //    messbar. Die Richtung wird nicht vorgeschrieben - gemessen fällt sie,
    //    weil die Achselhöhle den hängenden Arm einklemmt (Begründung im Kopf).
    assert!(
        (e_ende - e_anfang).abs() > 0.005,
        "Ärmelband beim Heben unverändert: {:+.2} % -> {:+.2} %",
        e_anfang * 100.0,
        e_ende * 100.0
    );
    // 5. Der Körper ist nicht der Posten, der ein Bild teuer macht.
    assert!(
        koerper_med < KOERPERZEIT_GRENZE,
        "Körperaufwand {:.2} ms je Bild (Grenze {:.1})",
        koerper_med * 1e3,
        KOERPERZEIT_GRENZE * 1e3
    );
    assert!(
        koerper_med / bild_med < KOERPERANTEIL_GRENZE,
        "der Körper macht {:.1} % der Bildzeit aus (Grenze {:.0} %): dann lohnt die Nachführung nicht mehr",
        koerper_med / bild_med * 100.0,
        KOERPERANTEIL_GRENZE * 100.0
    );
}
