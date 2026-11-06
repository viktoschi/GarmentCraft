//! The motion path as a library call on the short schedule: dress, then raise
//! both arms frame by frame and write every frame.
//!
//! Checked here, and nowhere else, are the promises of `klon3d bewegen`:
//!
//! * **No gaps.** `bild_0000.obj` (the dressed rest state) plus one OBJ, one
//!   strain map and one distance map per motion frame, and `bewegung.glb` for
//!   the whole motion — the folder holds exactly those files and nothing else.
//! * **The map belongs to its frame.** Every distance in `abstand_NNNN.csv` is
//!   the distance to a fresh rebuild of the *same* posed points, and it is
//!   **not** the distance to the bind pose. That is the whole point of writing
//!   the map per frame instead of once.
//! * **The arm arrives.** The posed body of the last frame is more than 15 cm
//!   from the bind pose at its furthest point, and the sleeve points moved with
//!   it.
//! * **Reproducible.** Two runs of the same schedule write identical files.
//! * **Format kept.** The CSV heads are the stage-5 heads.
//!
//! Motion frames are 1 to 3 of 30°, so the run is 10° per frame — coarser than
//! the 1.5° of `tests/armheben.rs`, which is the price of running in a debug
//! build. What is asserted is that the pipeline moves the garment and writes it
//! per frame, not how a lift looks; the lift itself is measured there.
//!
//! Needs the MakeHuman data (`assets/3dobjs/makehuman`, shipped with the
//! program) and the test pattern; if either is missing the test prints
//! `übersprungen: …` and returns.

use std::path::{Path, PathBuf};

use klon3d_mesh::Parameter;
use klon3d_pipeline::bewegen::{self, Bildbericht, Schreiber};
use klon3d_pipeline::{ablauf, Muster};
use klon3d_sim::netzkoerper::SUCHTIEFE;
use klon3d_sim::platzierung::Anziehen;
use klon3d_sim::{
    abstand, ImplizitCg, ImplizitParameter, Koerper, Koerpernetz, Szene, RADICAL_RUBBER_04,
};

const MUSTER: &str = "bluse2_v26";
/// Height the skeleton is fitted to (mm) — the default of `klon3d bewegen`.
const HOEHE_MM: f64 = 1600.0;
/// Motion frames and the angle they share. Three frames of 30° keep the debug
/// build cheap; the assertion on the travel is set against this, not against
/// the 90° of the test in `klon3d-sim`.
const BILDER: usize = 3;
const GRAD: f64 = 30.0;

fn wurzel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

/// The shipped MakeHuman data; `None` (with a message) when it is missing.
fn makehuman() -> Option<PathBuf> {
    let o = klon3d_koerper::koerper_quelle::ordner_ab(Path::new(env!("CARGO_MANIFEST_DIR")));
    match o {
        Some(o) => Some(o),
        None => {
            eprintln!("übersprungen: MakeHuman-Daten (assets/3dobjs/makehuman) fehlen");
            None
        }
    }
}

/// The short dress schedule of `tests/kette.rs`: 2 sewing frames, at most 2
/// settling frames, 1 hold frame.
fn kurz() -> Anziehen {
    Anziehen {
        naehbilder: 2,
        einschwingen_max: 2,
        haltebilder: 1,
        ..Anziehen::default()
    }
}

/// Names in a folder, sorted, so two runs can be compared flat.
fn dateien(ordner: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(ordner)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn abstand3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn median(v: &[f64]) -> f64 {
    let mut w = v.to_vec();
    w.sort_by(f64::total_cmp);
    w[w.len() / 2]
}

/// The body net of a scene frame, by value: what the garment of that frame was
/// actually checked against.
fn koerper_punkte(s: &Szene) -> Vec<[f64; 3]> {
    match s.koerper.as_slice() {
        [Koerper::Netz { netz, .. }] => netz.punkte.clone(),
        k => panic!("erwartet wird genau ein Körpernetz, gefunden {}", k.len()),
    }
}

/// Dresses the blouse on the MakeHuman body and stops at the rest state.
fn angezogen(
    mh: &Path,
    netz: &klon3d_mesh::Musternetz,
    lage: &klon3d_sim::platzierung::Lageliste,
) -> Szene {
    let (geladen, _) = bewegen::koerper_mit_skelett(mh, HOEHE_MM).expect("Körper mit Skelett");
    let h = klon3d_pipeline::huellen(&geladen).expect("Hüllen");
    let p = klon3d_pipeline::platzieren(netz, lage, &h, true).expect("platzieren");
    let mut s = klon3d_pipeline::szene_aus(
        &p,
        &geladen,
        RADICAL_RUBBER_04,
        0.0,
        klon3d_pipeline::standard_simparameter(),
    );
    let zonen = p.klebungen(&s).into_iter().map(|(z, _)| z).collect();
    s.klebungen_setzen(zonen);
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let erg = ablauf::anziehen(&mut s, &mut rw, &kurz(), &mut |_, _, _| {});
    eprintln!(
        "angezogen: {} Bilder nähen, {} einschwingen, ruhig: {}",
        kurz().naehbilder,
        erg.einschwingbilder,
        erg.eingeschwungen
    );
    assert!(
        s.masse.iter().all(|&m| m > 0.0),
        "kein Stoffpunkt darf festgehalten sein"
    );
    s
}

/// The pieces for which `name` matches, in scene order.
fn teile_mit(s: &Szene, name: &str) -> Vec<usize> {
    (0..s.teile.len())
        .filter(|&i| s.teile[i].name == name || s.teile[i].name.rsplit(" - ").next() == Some(name))
        .collect()
}

#[test]
fn jedes_bild_wird_geschrieben() {
    let Some(mh) = makehuman() else { return };
    for endung in ["sm2d", "naehte", "lage"] {
        let p = wurzel().join(format!("{MUSTER}.{endung}"));
        if !p.is_file() {
            eprintln!("übersprungen: {} fehlt", p.display());
            return;
        }
    }
    let m = Muster::laden(&wurzel().join(format!("{MUSTER}.sm2d")), None).expect("Muster");
    let defs = klon3d_pipeline::nahtliste(&wurzel().join(format!("{MUSTER}.naehte"))).expect("Nähte");
    let netz = m.netz(&defs, &Parameter::default()).expect("Netz");
    let lage = klon3d_pipeline::lageliste(&wurzel().join(format!("{MUSTER}.lage"))).expect("Lage");

    let s0 = angezogen(&mh, &netz, &lage);
    let (geladen, mut sk) = bewegen::koerper_mit_skelett(&mh, HOEHE_MM).expect("Körper");
    let binde: Vec<[f64; 3]> = geladen.netz.punkte.clone();
    let koerperdreiecke = geladen.netz.dreiecke.clone();
    assert_eq!(
        binde.len(),
        sk.gewichte.len(),
        "Skelett und Körper der Szene gehören nicht zum selben Netz"
    );

    // Two runs from the same dressed state. The skeleton resets itself in
    // `hebe_arme`, and the solver is fresh in both, so the frames have to come
    // out equal; that is the reproducibility claim, checked byte for byte.
    let mut ordner: Vec<PathBuf> = Vec::new();
    let mut verlauf: Vec<Vec<(Bildbericht, Szene)>> = Vec::new();
    for lauf in 0..2 {
        let d = std::env::temp_dir().join(format!(
            "klon3d-bewegen-{}-{lauf}",
            std::process::id()
        ));
        if d.exists() {
            std::fs::remove_dir_all(&d).expect("alten Ordner räumen");
        }
        let mut s = s0.clone();
        let mut rw = ImplizitCg::neu(ImplizitParameter::default());
        let mut schreiber = Schreiber::neu(&d);
        let mut gesehen: Vec<(Bildbericht, Szene)> = Vec::new();
        bewegen::bewegen(&mut s, &mut rw, &mut sk, &netz, BILDER, GRAD, &mut |b, s| {
            schreiber.bild(b, s).expect("Bild schreiben");
            gesehen.push((b.clone(), s.clone()));
        })
        .expect("bewegen");
        schreiber.abschluss().expect("glTF schreiben");
        eprintln!(
            "Lauf {lauf}: {} Bilder, zuletzt {:.1}°, tiefster {:+.2} mm, offene Nahtpaare {}",
            gesehen.len(),
            gesehen.last().unwrap().0.grad,
            gesehen.last().unwrap().0.tiefster * 1e3,
            gesehen.last().unwrap().0.offene_nahtpaare
        );
        ordner.push(d);
        verlauf.push(gesehen);
    }

    // ── Keine Lücken in der Folge ─────────────────────────────────────────
    let (berichte, szenen): (Vec<Bildbericht>, Vec<Szene>) = verlauf[0].iter().cloned().unzip();
    assert_eq!(berichte.len(), BILDER + 1, "Bild 0 plus {BILDER} Bewegungsbilder");
    for (i, b) in berichte.iter().enumerate() {
        assert_eq!(b.bild, i, "die Bilder kommen in der Reihenfolge 0..={BILDER}");
        let erwartet = GRAD * i as f64 / BILDER as f64;
        assert!(
            (b.grad - erwartet).abs() < 1e-12,
            "Bild {i}: {:.3}° statt {:.3}°",
            b.grad,
            erwartet
        );
    }
    assert_eq!(berichte[0].grad, 0.0, "Bild 0 ist der ruhende Zustand");

    let mut erwartet_namen: Vec<String> = Vec::new();
    for b in 0..=BILDER {
        for p in Schreiber::pfade(&ordner[0], b) {
            erwartet_namen.push(p.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    erwartet_namen.push(
        Schreiber::glb_pfad(&ordner[0])
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    erwartet_namen.sort();
    assert_eq!(
        dateien(&ordner[0]),
        erwartet_namen,
        "je Bild OBJ, Dehnung und Abstand, dazu die eine glTF, sonst nichts"
    );
    assert!(ordner[0].join("bild_0000.obj").is_file(), "der Ruhezustand fehlt");

    // ── Die Karte gehört zu ihrem Bild ────────────────────────────────────
    let mut kopf_abstand = String::new();
    let mut kopf_dehnung = String::new();
    for bild in 0..=BILDER {
        // Neu gebaut aus genau den verformten Punkten dieses Bildes. Der
        // gemessene Körper ist derselbe Punktesatz, nur mit dem weiteren
        // Raster der Binde-Pose - die Abstände müssen übereinstimmen, die
        // Dreiecke dürfen es nicht (Gleichstand bei zwei gleich nahen).
        let mut kopie = szenen[bild].clone();
        let neu = Koerpernetz::neu(koerper_punkte(&szenen[bild]), koerperdreiecke.clone(), SUCHTIEFE)
            .expect("Körpernetz aus den verformten Punkten");
        kopie.koerper = vec![Koerper::netz(neu)];

        let geschrieben = abstand::Karte::messe(&szenen[bild]).expect("Abstandskarte");
        let erwartet = abstand::Karte::messe(&kopie).expect("Abstandskarte neu");
        assert_eq!(geschrieben.je_punkt.len(), erwartet.je_punkt.len());
        for (i, (g, e)) in geschrieben.je_punkt.iter().zip(&erwartet.je_punkt).enumerate() {
            let d = (g.abstand - e.abstand).abs() * 1e3;
            assert!(
                d <= 1e-9,
                "Bild {bild}, Punkt {i}: {:.6} mm Abweichung zum Neuaufbau derselben Punkte",
                d
            );
        }
        // Und die geschriebene Datei ist die Karte dieses Bildes, Zeichen für
        // Zeichen.
        let [_, _, pfad] = Schreiber::pfade(&ordner[0], bild);
        let datei = std::fs::read_to_string(&pfad).expect("abstand.csv lesen");
        assert_eq!(
            datei,
            erwartet.csv(&kopie),
            "Bild {bild}: abstand.csv weicht von der Karte dieses Bildes ab"
        );
        kopf_abstand = datei.lines().next().unwrap_or("").to_string();

        let [_, pfad_dehn, _] = Schreiber::pfade(&ordner[0], bild);
        let dehn = std::fs::read_to_string(&pfad_dehn).expect("dehnung.csv lesen");
        assert_eq!(
            dehn,
            klon3d_sim::dehnung::Karte::messe(&szenen[bild]).csv(&szenen[bild]),
            "Bild {bild}: dehnung.csv weicht von der Dehnung dieses Bildes ab"
        );
        kopf_dehnung = dehn.lines().next().unwrap_or("").to_string();
    }

    // Gegen die Binde-Pose darf die letzte Karte gerade **nicht** stimmen.
    let mut bind = szenen[BILDER].clone();
    bind.koerper = vec![Koerper::netz(geladen.netz.clone())];
    let gegen_binde = abstand::Karte::messe(&bind).expect("Abstandskarte Binde-Pose");
    let stellte = abstand::Karte::messe(&szenen[BILDER]).expect("Abstandskarte letztes Bild");
    let groesste = stellte
        .je_punkt
        .iter()
        .zip(&gegen_binde.je_punkt)
        .map(|(a, b)| (a.abstand - b.abstand).abs())
        .fold(0.0f64, f64::max);
    assert!(
        groesste > 1e-3,
        "die Karte des letzten Bildes ist auch gegen die Binde-Pose richtig \
         (größte Abweichung {:.3} mm) - sie gehört nicht zu ihrem Bild",
        groesste * 1e3
    );

    // ── Der Arm kommt an ──────────────────────────────────────────────────
    let posed = koerper_punkte(&szenen[BILDER]);
    let hautweg = posed
        .iter()
        .zip(&binde)
        .map(|(a, b)| abstand3(*a, *b))
        .fold(0.0f64, f64::max);
    assert!(
        hautweg > 0.15,
        "der verformte Körper ist dem ruhenden nur {:.1} cm fern - der Arm hebt sich nicht",
        hautweg * 100.0
    );

    let mut aermel: Vec<usize> = Vec::new();
    for name in ["Aermel Kurz Links", "Aermel Kurz Rechts"] {
        let teile = teile_mit(&szenen[0], name);
        assert_eq!(teile.len(), 1, "Ärmel {name:?} nicht eindeutig im Netz");
        aermel.extend(szenen[0].teile[teile[0]].punkte.clone());
    }
    assert!(aermel.len() > 100, "Ärmel: nur {} Punkte", aermel.len());
    let weg: Vec<f64> = aermel
        .iter()
        .map(|&i| abstand3(szenen[BILDER].x[i], szenen[0].x[i]))
        .collect();
    let (weg_med, weg_max) = (median(&weg), weg.iter().copied().fold(0.0f64, f64::max));
    assert!(
        weg_med > 0.005,
        "der Ärmel wandert im Median nur {:.2} cm",
        weg_med * 100.0
    );
    assert!(
        weg_max > 0.02,
        "kein Ärmelpunkt wandert mehr als {:.2} cm",
        weg_max * 100.0
    );

    // ── Reproduzierbar ────────────────────────────────────────────────────
    assert_eq!(
        dateien(&ordner[1]),
        dateien(&ordner[0]),
        "der zweite Lauf schreibt andere Dateien"
    );
    for b in 0..=BILDER {
        for (i, pa) in Schreiber::pfade(&ordner[0], b).iter().enumerate() {
            let pb = &Schreiber::pfade(&ordner[1], b)[i];
            assert_eq!(
                std::fs::read(pa).unwrap(),
                std::fs::read(pb).unwrap(),
                "{} unterscheidet sich zwischen zwei Läufen",
                pa.file_name().unwrap().to_string_lossy()
            );
        }
    }
    let ga = Schreiber::glb_pfad(&ordner[0]);
    let gb = Schreiber::glb_pfad(&ordner[1]);
    assert_eq!(
        std::fs::read(&ga).unwrap(),
        std::fs::read(&gb).unwrap(),
        "bewegung.glb unterscheidet sich zwischen zwei Läufen"
    );

    // ── Format bleibt das der Stufe 5 ────────────────────────────────────
    assert_eq!(kopf_abstand, "punkt;teil;x_cm;y_cm;z_cm;abstand_mm;nx;ny;nz");
    assert_eq!(
        kopf_dehnung,
        "dreieck;teil;flaeche_cm2;groesste_prozent;kleinste_prozent;muster_prozent"
    );

    eprintln!(
        "== bewegen, {MUSTER} am MakeHuman-Körper ==\n  \
         {BILDER} Bilder bis {GRAD:.0}°: Haut bis {:.1} cm, Ärmel {:.1} cm Median / {:.1} cm äußerst, \
         tiefster {:+.2} mm, offene Nahtpaare {} → {}",
        hautweg * 100.0,
        weg_med * 100.0,
        weg_max * 100.0,
        berichte[BILDER].tiefster * 1e3,
        berichte[0].offene_nahtpaare,
        berichte[BILDER].offene_nahtpaare
    );

    for d in &ordner {
        let _ = std::fs::remove_dir_all(d);
    }
}
