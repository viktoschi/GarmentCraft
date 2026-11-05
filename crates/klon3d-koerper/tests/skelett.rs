//! Abnahme von Issue #94, Stufe 7: Skelett, Hautgewichte und Armheben auf dem
//! freien MakeHuman-Körper.
//!
//! Der Test braucht **keine** CLO-Daten: `assets/3dobjs/makehuman/base.obj`
//! bringt den Körper und in denselben `joint-*`-Gruppen 125 Gelenke mit.
//!
//! Jede Zahl hier ist vorher in Python (`mh_referenz.py`, `mh_genau.py` an den
//! Handeingaben) auf **demselben** Netz nachgerechnet worden und steht dort mit
//! voller Stellenzahl. Der Test prüft also nicht die Abnahme gegen sich selbst,
//! sondern den Rust-Port gegen die unabhängige Rechnung.
//!
//! Was die Abnahme verlangt (Issue-Text):
//!
//! | Prüfung | Grenze |
//! |---|---|
//! | ferne Kanten (≥ 120 mm von beiden Schultern) über doppelter Länge | 0 |
//! | ferne Kanten mit mehr als 10 mm Zuwachs | 0 |
//! | Gewichtssumme je Punkt | 1,0 auf 1e-15 |
//! | Einflüsse je Punkt | höchstens 4 |
//! | Binde-Pose | 0 mm (auf 1e-9) |
//! | Schulter bei 90° | kein Zusammenfallen (Median ≈ 1) |

use std::path::{Path, PathBuf};

use klon3d_koerper::koerper_quelle::ordner_ab;
use klon3d_koerper::skelett::{self, Skelett};
use klon3d_koerper::{Netz, Punkt3};

/// Die 125 Gelenknamen in der Reihenfolge des Skeletts: nach Tiefe sortiert,
/// bei Gleichstand in der Reihenfolge der Datei (stabil).
const NAMEN: &str = "\
ground pelvis spine-4 l-upper-leg r-upper-leg spine-3 \
l-knee r-knee spine-2 l-ankle r-ankle spine-1 \
l-foot-1 r-foot-1 l-foot-2 l-toe-5-1 l-toe-4-1 l-toe-3-1 \
l-toe-2-1 l-toe-1-1 l-clavicle l-scapula r-foot-2 r-toe-5-1 \
r-toe-4-1 r-toe-3-1 r-toe-2-1 r-toe-1-1 r-clavicle r-scapula \
neck l-toe-2-2 l-toe-3-2 l-toe-4-2 l-toe-5-2 l-toe-1-2 \
l-shoulder head r-toe-2-2 r-toe-3-2 r-toe-4-2 r-toe-5-2 \
r-toe-1-2 r-shoulder l-eye r-eye l-toe-5-3 l-toe-4-3 \
l-toe-3-3 l-toe-2-3 l-toe-1-3 l-elbow l-lowerlid l-eye-target \
l-upperlid r-toe-5-3 r-toe-4-3 r-toe-3-3 r-toe-2-3 r-toe-1-3 \
r-elbow r-lowerlid r-eye-target r-upperlid jaw head-2 \
tongue-1 mouth l-toe-5-4 l-toe-4-4 l-toe-3-4 l-toe-2-4 \
l-hand r-toe-5-4 r-toe-4-4 r-toe-3-4 r-toe-2-4 r-hand \
tongue-2 l-finger-5-1 l-finger-4-1 l-finger-3-1 l-finger-2-1 l-finger-1-1 \
l-hand-3 l-hand-2 r-finger-5-1 r-finger-4-1 r-finger-3-1 r-finger-2-1 \
r-finger-1-1 r-hand-3 r-hand-2 tongue-3 l-finger-2-2 l-finger-3-2 \
l-finger-4-2 l-finger-5-2 l-finger-1-2 r-finger-2-2 r-finger-3-2 r-finger-4-2 \
r-finger-5-2 r-finger-1-2 tongue-4 l-finger-5-3 l-finger-4-3 l-finger-3-3 \
l-finger-2-3 l-finger-1-3 r-finger-5-3 r-finger-4-3 r-finger-3-3 r-finger-2-3 \
r-finger-1-3 l-finger-2-4 l-finger-3-4 l-finger-4-4 l-finger-5-4 l-finger-1-4 \
r-finger-2-4 r-finger-3-4 r-finger-4-4 r-finger-5-4 r-finger-1-4 \
";

/// Die 110 Gelenke, die Gewichte tragen — aktiv **und** mit aktivem Elternteil.
const KNOCHEN: &str = "\
spine-4 l-upper-leg r-upper-leg spine-3 l-knee r-knee \
spine-2 l-ankle r-ankle spine-1 l-foot-1 r-foot-1 \
l-foot-2 l-toe-5-1 l-toe-4-1 l-toe-3-1 l-toe-2-1 l-toe-1-1 \
l-clavicle l-scapula r-foot-2 r-toe-5-1 r-toe-4-1 r-toe-3-1 \
r-toe-2-1 r-toe-1-1 r-clavicle r-scapula neck l-toe-2-2 \
l-toe-3-2 l-toe-4-2 l-toe-5-2 l-toe-1-2 l-shoulder head \
r-toe-2-2 r-toe-3-2 r-toe-4-2 r-toe-5-2 r-toe-1-2 r-shoulder \
l-toe-5-3 l-toe-4-3 l-toe-3-3 l-toe-2-3 l-toe-1-3 l-elbow \
r-toe-5-3 r-toe-4-3 r-toe-3-3 r-toe-2-3 r-toe-1-3 r-elbow \
jaw head-2 l-toe-5-4 l-toe-4-4 l-toe-3-4 l-toe-2-4 \
l-hand r-toe-5-4 r-toe-4-4 r-toe-3-4 r-toe-2-4 r-hand \
l-finger-5-1 l-finger-4-1 l-finger-3-1 l-finger-2-1 l-finger-1-1 l-hand-3 \
l-hand-2 r-finger-5-1 r-finger-4-1 r-finger-3-1 r-finger-2-1 r-finger-1-1 \
r-hand-3 r-hand-2 l-finger-2-2 l-finger-3-2 l-finger-4-2 l-finger-5-2 \
l-finger-1-2 r-finger-2-2 r-finger-3-2 r-finger-4-2 r-finger-5-2 r-finger-1-2 \
l-finger-5-3 l-finger-4-3 l-finger-3-3 l-finger-2-3 l-finger-1-3 r-finger-5-3 \
r-finger-4-3 r-finger-3-3 r-finger-2-3 r-finger-1-3 l-finger-2-4 l-finger-3-4 \
l-finger-4-4 l-finger-5-4 l-finger-1-4 r-finger-2-4 r-finger-3-4 r-finger-4-4 \
r-finger-5-4 r-finger-1-4 \
";

fn woerter(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

fn basis() -> Option<PathBuf> {
    ordner_ab(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Körper und Skelett, wie die CLI sie baut; `None`, wenn die Assets fehlen.
fn gebaut() -> Option<(Netz, Skelett)> {
    let ordner = basis()?;
    match skelett::baue(&ordner, 1600.0) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("übersprungen: {e}");
            None
        }
    }
}

/// Der Nahbereich um die Schulter, wie ihn die Abnahme zieht.
const SCHULTER_NAH: f64 = 120.0;

fn abstand(a: Punkt3, b: Punkt3) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

#[test]
fn das_skelett_liest_125_gelenke_in_der_referenzreihenfolge() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    let soll = woerter(NAMEN);
    assert_eq!(s.gelenke.len(), 125, "Gelenkzahl");
    let ist: Vec<&str> = s.gelenke.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(ist, soll, "Reihenfolge der Gelenke weicht ab");
    assert_eq!(netz.punkte.len(), 13_380, "Körperpunkte");
    assert_eq!(netz.dreiecke.len(), 26_756, "Dreiecke");
}

#[test]
fn die_knochenregel_gibt_110_tragende_gelenke() {
    let Some((_, s)) = gebaut() else {
        return;
    };
    let soll = woerter(KNOCHEN);
    let ist: Vec<&str> = s
        .knochen
        .iter()
        .map(|&i| s.gelenke[i].name.as_str())
        .collect();
    assert_eq!(ist.len(), 110, "Knochenzahl");
    assert_eq!(ist, soll, "Knochenauswahl oder -reihenfolge weicht ab");
    // 125 Gelenke, 110 tragen Gewichte: die 15 ohne sind Boden, Zunge, Mund,
    // Augen und Lider.
    assert_eq!(s.gelenke.len() - s.knochen.len(), 15);
}

#[test]
fn die_gelenke_sitzen_an_den_referenzpunkten() {
    let Some((_, s)) = gebaut() else {
        return;
    };
    // Aus `mh_rustwerte.py`, mm im eingepassten Körper.
    let soll: [(&str, Punkt3); 11] = [
        ("pelvis", [0.0, 830.163, -98.1941]),
        ("spine-1", [0.0, 1210.0, -109.207]),
        ("l-clavicle", [22.8526, 1285.55, -100.337]),
        ("l-shoulder", [154.888, 1270.59, -100.805]),
        ("l-elbow", [312.366, 1098.24, -96.9471]),
        ("l-hand", [422.379, 977.859, 38.5975]),
        ("head", [0.0, 1465.17, -81.0273]),
        ("l-upper-leg", [95.1427, 819.031, -104.366]),
        ("l-knee", [134.945, 448.09, -73.9014]),
        ("l-ankle", [165.559, 68.5852, -92.0001]),
        ("jaw", [0.0, 1409.94, 2.6742]),
    ];
    for (name, p) in soll {
        let i = s.finde(name).unwrap_or_else(|| panic!("{name} fehlt"));
        let ist = s.gelenke[i].ort;
        let d = abstand(ist, p);
        assert!(
            d < 0.01,
            "{name}: {ist:?} statt {p:?}, {d:.4} mm daneben"
        );
    }
}

#[test]
fn die_radien_sind_das_60_perzentil_mit_20_mm_untergrenze() {
    let Some((_, s)) = gebaut() else {
        return;
    };
    // Aus `mh_referenz.py`.
    let soll = [
        ("spine-1", 137.442_022),
        ("spine-2", 134.933_309),
        ("l-upper-leg", 101.059_285),
        ("l-clavicle", 81.606_575),
        ("l-shoulder", 71.920_315),
        ("l-elbow", 47.178_680),
        ("l-hand", 30.149_928),
        ("head-2", 74.509_679),
        ("neck", 62.350_835),
    ];
    for (name, r) in soll {
        let ist = s.radius(name).unwrap_or_else(|| panic!("{name} trägt keine Gewichte"));
        assert!(
            (ist - r).abs() < 1e-5,
            "{name}: Radius {ist:.6} statt {r:.6}"
        );
    }
    // 82 der 110 Knochen liegen auf der Untergrenze, der Median der übrigen
    // ist der Radius der Schulter.
    let auf_der_grenze = s.radien.iter().filter(|&&r| r <= 20.0 + 1e-9).count();
    assert_eq!(auf_der_grenze, 82, "Knochen auf der 20-mm-Grenze");
    let mut ueber: Vec<f64> = s.radien.iter().copied().filter(|&r| r > 20.0).collect();
    ueber.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = skelett::perzentil(&ueber, 50.0);
    assert!(
        (median - 71.920_315).abs() < 1e-5,
        "Median der Radien {median:.6}"
    );
}

#[test]
fn die_gewichte_treffen_die_referenz_und_summe_eins() {
    let Some((_, s)) = gebaut() else {
        return;
    };
    // Aus `mh_genau.py`. Alle vier Einflüsse, auch die verschwindend kleinen —
    // sonst würde der Test die Auswahl nicht prüfen. Jede Zahl steht in der
    // kürzesten Schreibweise, die genau dieselbe f64 ergibt (geprüft).
    let soll: [(usize, [(&str, f64); 4]); 8] = [
        (
            3960,
            [
                ("spine-1", 0.853_938_350_306_398_1),
                ("spine-2", 0.143_717_633_685_730_2),
                ("l-clavicle", 0.001_304_770_147_570_970_2),
                ("l-scapula", 0.001_039_245_860_300_591_2),
            ],
        ),
        (
            8556,
            [
                ("spine-1", 0.760_583_961_172_056_2),
                ("spine-2", 0.220_492_072_781_972_53),
                ("l-scapula", 0.010_097_855_943_516_637),
                ("l-clavicle", 0.008_826_110_102_454_696),
            ],
        ),
        (
            8335,
            [
                ("l-elbow", 0.582_677_632_581_997_6),
                ("l-shoulder", 0.307_830_215_175_583_73),
                ("spine-1", 0.102_140_683_003_324_71),
                ("l-scapula", 0.007_351_469_239_093_989),
            ],
        ),
        (
            8225,
            [
                ("l-scapula", 0.546_542_759_760_015),
                ("l-shoulder", 0.415_913_873_728_655_4),
                ("l-elbow", 0.029_497_097_107_660_675),
                ("spine-1", 0.008_046_269_403_668_895),
            ],
        ),
        (
            8422,
            [
                ("l-elbow", 0.988_842_063_850_134_3),
                ("spine-1", 0.006_857_362_406_451_317),
                ("spine-2", 0.002_361_617_399_602_241_4),
                ("l-hand", 0.001_938_956_343_812_098_3),
            ],
        ),
        (
            5368,
            [
                ("head-2", 0.968_123_210_823_398_6),
                ("jaw", 0.023_829_282_792_497_936),
                ("head", 0.007_340_860_510_089_639),
                ("spine-1", 0.000_706_645_874_013_773_9),
            ],
        ),
        (
            11070,
            [
                ("l-knee", 0.905_719_009_495_800_9),
                ("l-upper-leg", 0.051_316_411_886_234_335),
                ("r-upper-leg", 0.037_666_271_398_509_18),
                ("spine-4", 0.005_298_307_219_455_612),
            ],
        ),
        (
            9653,
            [
                ("l-finger-3-2", 0.466_185_495_732_539_9),
                ("l-finger-2-2", 0.317_294_770_188_212_4),
                ("l-finger-2-1", 0.133_029_239_197_514_22),
                ("l-finger-3-1", 0.083_490_494_881_733_42),
            ],
        ),
    ];

    for (punkt, paare) in soll {
        let g = &s.gewichte[punkt];
        assert_eq!(g.len(), 4, "Punkt {punkt}: {} Einflüsse", g.len());
        for (name, w) in paare {
            let i = s.finde(name).unwrap_or_else(|| panic!("{name} fehlt"));
            let ist = g
                .iter()
                .find(|(k, _)| *k == i)
                .map(|(_, w)| *w)
                .unwrap_or_else(|| panic!("Punkt {punkt} hat kein Gewicht auf {name}"));
            assert!(
                (ist - w).abs() < 1e-9,
                "Punkt {punkt}, {name}: {ist:.17} statt {w:.17}"
            );
        }
    }

    // Kennzahlen über den ganzen Satz.
    assert!(
        s.gewichtssumme_abweichung() <= 1e-15,
        "Gewichtssumme weicht {:.3e} von 1 ab",
        s.gewichtssumme_abweichung()
    );
    assert_eq!(s.einfluesse_max(), 4, "mehr als vier Einflüsse");
    assert!(
        s.gewichte.iter().all(|g| !g.is_empty()),
        "{} Punkte ohne Gewicht",
        s.gewichte.iter().filter(|g| g.is_empty()).count()
    );
    let kleinste = s
        .gewichte
        .iter()
        .flat_map(|g| g.iter().map(|(_, w)| *w))
        .filter(|&w| w > 0.0)
        .fold(f64::INFINITY, f64::min);
    assert!(
        (kleinste - 4.822_714_794_518_218_4e-12).abs() < 1e-18,
        "kleinstes positives Gewicht {kleinste:.3e}"
    );
}

#[test]
fn die_binde_pose_laesst_den_koerper_stehen() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    let neu = s.verforme(&netz.punkte).expect("Verformung möglich");
    let weg = neu
        .iter()
        .zip(&netz.punkte)
        .map(|(a, b)| abstand(*a, *b))
        .fold(0.0, f64::max);
    // Referenz 4,55e-13 mm — reines Aufsummieren von Rundungsfehlern.
    assert!(weg < 1e-9, "Binde-Pose verschiebt um {weg:.3e} mm");
}

#[test]
fn armheben_90_grad_reisst_keine_fernen_kanten() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    let ab = s
        .abnahme(&netz.punkte, &netz.dreiecke, 90.0)
        .expect("Abnahme messbar");

    println!(
        "ferne Kanten {} (über 2x {} / über 10 mm {}), größtes Verhältnis {:.6}",
        ab.fern_kanten, ab.fern_ueber_doppelt, ab.fern_ueber_10mm, ab.fern_max
    );
    println!(
        "nahe Kanten {} (Verhältnis {:.6} … {:.6}), Schulter {:.6} / {:.6}",
        ab.nah_kanten, ab.nah_min, ab.nah_max, ab.schulter_median, ab.schulter_min
    );

    // Die Kantenzahl selbst: der Test soll merken, wenn das Netz ein anderes wird.
    assert_eq!(ab.fern_kanten + ab.nah_kanten, 40_134, "Kantenzahl");
    assert_eq!(ab.fern_kanten, 38_476, "ferne Kanten");
    assert_eq!(ab.nah_kanten, 1_658, "nahe Kanten");
    assert_eq!(ab.fern_ueber_doppelt, 0, "ferne Kanten über doppelter Länge");
    assert_eq!(ab.fern_ueber_10mm, 0, "ferne Kanten über 10 mm Zuwachs");
    assert!(
        (ab.fern_max - 1.361_206_854_827_316_4).abs() < 1e-6,
        "größtes fernes Verhältnis {:.6}",
        ab.fern_max
    );

    // Im Schulterbereich darf sich die Länge ändern, aber nichts zusammenfallen;
    // das größte Verhältnis ist die gedehnte Achsel.
    assert_eq!(ab.nah_kollaps, 0, "nahe Kanten unter einem Fünftel");
    assert!(
        (ab.nah_max - 2.739_533_589_682_508_4).abs() < 1e-6,
        "größtes nahes Verhältnis {:.6}",
        ab.nah_max
    );

    // Die Schulter selbst hält ihren Abstand zum Gelenk: Median 0,929, Minimum
    // 0,712 (Referenz). Fällt der Median deutlich unter 1, ist die Schulter
    // eingeknickt.
    assert!(
        (ab.schulter_median - 0.929_246_200_475_605_2).abs() < 1e-6,
        "Schulter-Median {:.6}",
        ab.schulter_median
    );
    assert!(
        (ab.schulter_min - 0.712_336_199_437_396_6).abs() < 1e-6,
        "Schulter-Minimum {:.6}",
        ab.schulter_min
    );

    // Was die Abnahme selbst als bestanden wertet.
    assert!(ab.bestanden(), "Abnahme nicht bestanden: {ab:?}");
}

#[test]
fn der_schulterguertel_bleibt_stehen() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    // Die Falle: `l-shoulder` ist die Strecke Schlüsselbein → Schulter, also
    // der **Schultergürtel**. Dreht man ihn mit, schwingt sein inneres Ende am
    // Halsansatz mit und reißt die Haut auf. Richtig ist: nur die Nachfahren
    // **ohne** den Gürtel selbst.
    let gurt = s.finde("l-shoulder").expect("Schultergelenk");
    let mut gedreht = s.clone();
    gedreht.hebe_arme(90.0);

    let gurt_weg = abstand(
        gedreht.aktuell_ort(gurt),
        s.aktuell_ort(gurt),
    );
    assert!(
        gurt_weg < 1e-9,
        "der Schultergürtel selbst hat sich um {gurt_weg:.3e} mm bewegt"
    );

    // Der Arm dagegen muss hochkommen: die Hand steigt deutlich.
    let hand = s.finde("l-hand").expect("Handgelenk");
    let vorher = s.gelenke[hand].ort;
    let nachher = gedreht.aktuell_ort(hand);
    assert!(
        nachher[1] > vorher[1] + 50.0,
        "die Hand ist nur von y {:.1} auf {:.1} gestiegen",
        vorher[1],
        nachher[1]
    );

    // Und der Körper bewegt sich wirklich (kein Leerlauf).
    let neu = gedreht.verforme(&netz.punkte).expect("Verformung möglich");
    let weg = neu
        .iter()
        .zip(&netz.punkte)
        .map(|(a, b)| abstand(*a, *b))
        .fold(0.0, f64::max);
    assert!(weg > 100.0, "größter Punktweg nur {weg:.1} mm");
}

#[test]
fn der_nahe_bereich_ist_der_umkreis_beider_schultern() {
    let Some((netz, s)) = gebaut() else {
        return;
    };
    // Gegenprobe zur Einteilung: keine Kantenmitte darf gleichzeitig als fern
    // gezählt und von einer Schulter näher als 120 mm sein.
    let schultern: Vec<Punkt3> = ["l-shoulder", "r-shoulder"]
        .iter()
        .map(|n| s.gelenke[s.finde(n).expect("Schulter")].ort)
        .collect();
    let mut nah = 0;
    let mut fern = 0;
    for d in &netz.dreiecke {
        for (x, y) in [(d[0], d[1]), (d[1], d[2]), (d[2], d[0])] {
            let (a, b) = (netz.punkte[x], netz.punkte[y]);
            let m = [
                0.5 * (a[0] + b[0]),
                0.5 * (a[1] + b[1]),
                0.5 * (a[2] + b[2]),
            ];
            let d_min = schultern
                .iter()
                .map(|p| abstand(m, *p))
                .fold(f64::INFINITY, f64::min);
            if d_min >= SCHULTER_NAH {
                fern += 1;
            } else {
                nah += 1;
            }
        }
    }
    // Jede Kante zählt doppelt (zwei anliegende Dreiecke), außer am Rand; das
    // Netz ist geschlossen, also ist `fern + nah` das Doppelte der Kantenzahl.
    assert_eq!(nah + fern, 2 * 40_134, "Kantenzählung der Gegenprobe");
    assert_eq!(nah / 2, 1_658, "nahe Kanten in der Gegenprobe");
}
