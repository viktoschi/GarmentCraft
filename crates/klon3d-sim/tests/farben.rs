//! Acceptance for value → colour and for the coloured OBJ export (stage 5, task #19).
//!
//! The plan line is "Colored vertex attributes in the viewer, also exportable".
//! The viewer cannot show a scene yet (it is a 2D pattern browser), so this is
//! the **exportable** half: both maps as OBJ files with a colour per vertex.
//! What is checked here is exactly that and nothing about looks:
//!
//! 1. the colour scale itself — ends, clamping, monotonicity, the mix between
//!    two stops, and that a non-number does not become a colour;
//! 2. the coloured OBJ parses, keeps the geometry of the plain OBJ, and a
//!    colour column added to each vertex is the only difference;
//! 3. the strain per **point** is the area-weighted mean of the faces around
//!    it, computed by hand on a two-face square;
//! 4. a round trip: write, read back, and the colours are the ones the scale
//!    gives for the numbers in the CSV.
//!
//! All four run without CLO data.

use klon3d_mesh::{Musternetz, Teilnetz};
use klon3d_sim::ausgabe::{obj, obj_farbig};
use klon3d_sim::dehnung::Karte;
use klon3d_sim::farbkarte::{abstand, dehnung, farbe, DEHNUNG_BIS, DEHNUNG_VON, UNGUELTIG};
use klon3d_sim::roehre::rohr_netz;
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{Simparameter, Szene, RADICAL_RUBBER_04};

/// Ein flaches Rechteck, in x um `faktor` gestreckt.
fn flach(faktor: f64) -> Szene {
    let netz = rohr_netz(0.56, 0.28, 10.0).expect("Rechteck vernetzbar");
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.0,
    };
    let lage = move |_: usize, q: [f64; 2]| [q[0] * 1e-3 * faktor, q[1] * 1e-3, 0.0];
    Szene::aus_netz(&netz, &|_| vg, &lage, Vec::new(), Simparameter::default())
}

/// Die Zahlen einer `v`-Zeile als Felder.
fn v_felder(z: &str) -> Vec<&str> {
    z.strip_prefix("v ")
        .expect("v-Zeile")
        .split_whitespace()
        .collect()
}

/// 1. Der Maßstab: die Enden sind die Endfarben, dazwischen wird nicht
/// zurückgesprungen, und ausserhalb der Skala wird geklemmt statt neu skaliert.
#[test]
fn die_farbskala_ist_eine_feste_leiter() {
    let unten = dehnung(DEHNUNG_VON);
    let oben = dehnung(DEHNUNG_BIS);
    assert_ne!(unten, oben);
    assert_eq!(dehnung(-100.0), unten, "unter der Skala: Anfangsfarbe");
    assert_eq!(dehnung(1e6), oben, "über der Skala: Endfarbe");

    // Was die Leiter wirklich garantiert: Blau nimmt von kalt nach warm
    // durchgehend ab, am kalten Ende ist Blau der stärkste Kanal, am warmen
    // Rot, und die fünf Stufen sind unterscheidbar. Eine Monotonie des reinen
    // Rotkanals gibt es nicht — Gelb ist heller als Rot und Grün steigt auf der
    // kalten Hälfte —, deshalb wird sie auch nicht behauptet.
    let mut vorher = unten;
    for i in 0..=200 {
        let v = DEHNUNG_VON + (DEHNUNG_BIS - DEHNUNG_VON) * i as f64 / 200.0;
        let c = dehnung(v);
        assert!(
            c[2] <= vorher[2],
            "Blau steigt bei {v:.2}: {vorher:?} -> {c:?}"
        );
        vorher = c;
    }
    let kalt = dehnung(DEHNUNG_VON);
    let warm = dehnung(DEHNUNG_BIS);
    assert!(kalt[2] > kalt[0], "kalt ist blau: {kalt:?}");
    assert!(
        warm[0] > warm[1] && warm[0] > warm[2],
        "warm ist rot: {warm:?}"
    );
    // Die Stufen selbst sind alle verschieden, sonst wäre die Leiter flach.
    for i in 0..5 {
        for j in (i + 1)..5 {
            let a = farbe(
                DEHNUNG_VON + (DEHNUNG_BIS - DEHNUNG_VON) * i as f64 / 4.0,
                DEHNUNG_VON,
                DEHNUNG_BIS,
            );
            let b = farbe(
                DEHNUNG_VON + (DEHNUNG_BIS - DEHNUNG_VON) * j as f64 / 4.0,
                DEHNUNG_VON,
                DEHNUNG_BIS,
            );
            assert_ne!(a, b, "Stufe {i} und {j} gleich");
        }
    }

    // Dieselbe Zahl auf derselben relativen Stelle ergibt dieselbe Farbe —
    // die Skala hängt nur am Verhältnis, nicht am Maß.
    assert_eq!(farbe(5.0, 0.0, 10.0), farbe(50.0, 0.0, 100.0));
    // Die beiden Karten haben verschiedene Grenzen, also verschiedene Farben
    // für dieselbe Zahl.
    assert_ne!(dehnung(20.0), abstand(20.0));
    assert_eq!(abstand(0.0), unten);
}

/// 2. Ein Wert, der keine Zahl ist, wird grau und nicht rot: eine kaputte
/// Messung darf nicht wie eine starke Dehnung aussehen.
#[test]
fn kein_wert_keine_farbe() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(dehnung(v), UNGUELTIG, "{v}");
        assert_eq!(abstand(v), UNGUELTIG, "{v}");
    }
    // Eine leere oder verdrehte Skala ist ein Fehler im Aufruf, kein Absturz.
    assert_eq!(farbe(1.0, 5.0, 5.0), UNGUELTIG);
    assert_eq!(farbe(1.0, 5.0, 1.0), UNGUELTIG);
}

/// 3. Das farbige OBJ hat dieselbe Geometrie wie das einfache: gleiche
/// Eckpunkte, gleiche Gruppen, gleiche Flächen — nur drei Zahlen mehr je `v`.
#[test]
fn farbige_obj_hat_dieselbe_geometrie() {
    let s = flach(1.12);
    let schlicht = obj(&s);
    let bunt = obj_farbig(&s, |i| dehnung(i as f64 % DEHNUNG_BIS));

    let sv: Vec<&str> = schlicht.lines().filter(|z| z.starts_with("v ")).collect();
    let bv: Vec<&str> = bunt.lines().filter(|z| z.starts_with("v ")).collect();
    assert_eq!(sv.len(), s.x.len());
    assert_eq!(bv.len(), s.x.len(), "kein Eckpunkt verloren");
    for (a, b) in sv.iter().zip(&bv) {
        let fa = v_felder(a);
        let fb = v_felder(b);
        assert_eq!(fa.len(), 3);
        assert_eq!(fb.len(), 6, "Farbe fehlt: {b}");
        assert_eq!(&fa[..], &fb[..3], "Koordinaten verschoben: {a} / {b}");
        for wert in &fb[3..] {
            let n: u32 = wert.parse().expect("Farbe als Zahl");
            assert!(n <= 255, "Kanal ausserhalb 0..255: {wert}");
        }
    }

    // Flächen und Gruppen sind unverändert.
    let sf: Vec<&str> = schlicht
        .lines()
        .filter(|z| z.starts_with("f ") || z.starts_with("g "))
        .collect();
    let bf: Vec<&str> = bunt
        .lines()
        .filter(|z| z.starts_with("f ") || z.starts_with("g "))
        .collect();
    assert_eq!(sf, bf, "Gruppen oder Flächen geändert");

    // Und die Gegenprobe: Farbe abschneiden ergibt wieder das schlichte OBJ.
    let mut zurueck = String::from("# klon3d-sim, Einheit mm\n");
    for z in bunt.lines().filter(|z| z.starts_with("v ")) {
        let f = v_felder(z);
        zurueck.push_str(&format!("v {} {} {}\n", f[0], f[1], f[2]));
    }
    for z in bunt
        .lines()
        .filter(|z| z.starts_with("g ") || z.starts_with("f "))
    {
        zurueck.push_str(z);
        zurueck.push('\n');
    }
    assert_eq!(zurueck, schlicht, "Farben sind die einzige Änderung");
}

/// 4. Dehnung je **Punkt** ist das flächengewichtete Mittel der Flächen um ihn:
/// ein Punkt an der Grenze zweier Flächen mit 10 % und 0 % liest 5 %, und eine
/// größere Nachbarfläche zieht das Mittel zu sich.
#[test]
fn dehnung_je_punkt_ist_flaechengewichtet() {
    // Zwei Dreiecke, die sich nur den Punkt `m` teilen: links ein großes
    // (12 mm² Ruhefläche), rechts ein kleines (8 mm²). Der Punkt ganz links
    // wird nach aussen gezogen, dadurch dehnt sich nur das linke Dreieck.
    let p = [
        [0.0, 0.0],  // 0: nur im großen Dreieck
        [6.0, 0.0],  // 1: gemeinsam
        [6.0, 4.0],  // 2: nur im großen Dreieck
        [10.0, 0.0], // 3: nur im kleinen
        [10.0, 4.0], // 4: nur im kleinen
    ];
    let dreiecke = vec![[0u32, 1, 2], [1, 3, 4]];
    let netz = Musternetz {
        parameter: Default::default(),
        teile: vec![Teilnetz {
            teil: "Prüf".into(),
            punkte: p.to_vec(),
            dreiecke: dreiecke.clone(),
            nrand: 0,
            randort: Vec::new(),
            punktknoten: Vec::new(),
            kanten: Vec::new(),
            kontur: Vec::new(),
            schnitt: Vec::new(),
            fadenlauf: None,
            aufgegeben: 0,
        }],
        naehte: Vec::new(),
        fehler: Vec::new(),
        runden: 0,
        offene_teilungen: 0,
    };
    let vg = Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.0,
    };
    let mut s = Szene::aus_netz(
        &netz,
        &|_| vg,
        &|_, q| [q[0] * 1e-3, q[1] * 1e-3, 0.0],
        Vec::new(),
        Simparameter::default(),
    );

    let flaeche = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs()
    };
    let a0 = flaeche(p[0], p[1], p[2]);
    let a1 = flaeche(p[1], p[3], p[4]);
    assert!(
        a0 > a1,
        "das linke Dreieck ist das größere: {a0} gegen {a1}"
    );

    // Ist-Lage (m): alles auf der Musterlage, nur Punkt 0 wird 3 mm nach
    // links gezogen. Das linke Dreieck wird dadurch gestreckt, das rechte nicht.
    for (i, q) in p.iter().enumerate() {
        s.x[i] = [q[0] * 1e-3, q[1] * 1e-3, 0.0];
    }
    s.x[0] = [-3.0e-3, 0.0, 0.0];

    let k = Karte::messe(&s);
    let e = k.je_punkt(&s);
    assert_eq!(e.len(), s.x.len());

    let e0 = k.je_flaeche[0].groesste * 100.0;
    let e1 = k.je_flaeche[1].groesste * 100.0;
    assert!(e0 > 1.0, "das linke Dreieck ist gedehnt: {e0:.3} %");
    assert!(e1.abs() < 1e-9, "das rechte bleibt bei 0: {e1:.3e} %");

    // Eckpunkte, die nur einer Fläche angehören, tragen deren Wert.
    for i in [0, 2] {
        assert!(
            (e[i] - e0).abs() < 1e-9,
            "Punkt {i}: {:.6} gegen {e0:.6}",
            e[i]
        );
    }
    for i in [3, 4] {
        assert!(
            (e[i] - e1).abs() < 1e-9,
            "Punkt {i}: {:.6} gegen {e1:.6}",
            e[i]
        );
    }

    // Der gemeinsame Punkt ist das nach **Ruhefläche** gewichtete Mittel.
    let w0 = k.je_flaeche[0].ruheflaeche_m2;
    let w1 = k.je_flaeche[1].ruheflaeche_m2;
    let soll = (k.je_flaeche[0].groesste * w0 + k.je_flaeche[1].groesste * w1) / (w0 + w1) * 100.0;
    assert!(
        (e[1] - soll).abs() < 1e-9,
        "Punkt 1: {:.6} gegen {soll:.6} %",
        e[1]
    );
    // Und weil w0 > w1, liegt das Mittel näher an der großen Fläche.
    assert!(
        (e[1] - e0).abs() < (e[1] - e1).abs(),
        "das Mittel liegt näher an der größeren Fläche: {:.3} zu {e0:.3}/{e1:.3}",
        e[1]
    );
}

/// 5. Hin und zurück: was im OBJ als Farbe steht, ist die Farbe, die die Skala
/// für den Wert aus der CSV-Spalte liefert. Damit ist die Datei ohne die
/// Tabelle lesbar und die Tabelle ohne die Datei.
#[test]
fn farbe_im_obj_passt_zur_zahl_in_der_csv() {
    let s = flach(1.12);
    let k = Karte::messe(&s);
    let pro_punkt = k.je_punkt(&s);
    let bunt = obj_farbig(&s, |i| dehnung(pro_punkt[i]));

    let farben: Vec<[u8; 3]> = bunt
        .lines()
        .filter(|z| z.starts_with("v "))
        .map(|z| {
            let f = v_felder(z);
            [
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
                f[5].parse().unwrap(),
            ]
        })
        .collect();
    assert_eq!(farben.len(), pro_punkt.len());
    for (i, (&wert, &c)) in pro_punkt.iter().zip(&farben).enumerate() {
        assert_eq!(c, dehnung(wert), "Punkt {i}: {wert:.4} %");
    }
    // Bei 12 % liegt alles in derselben Farbe — die Karte ist flach gestreckt.
    assert!(farben.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(farben[0], dehnung(12.0));
}
