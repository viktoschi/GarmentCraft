//! The library chain on the Bluse3 v2: pattern → mesh → body → placement →
//! frame, and the frame-by-frame dressing against the one-call
//! `platzierung::anziehen` the command line used before.
//!
//! Pattern and seam list are in the repository; the body needs the resizing
//! folder ([`klon3d_koerper::koerper::ordner`]). Without it the body tests print
//! `übersprungen: … fehlt` and return.

use std::path::PathBuf;

use klon3d_mesh::{vernetze, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_pipeline::{ablauf, Koerperquelle, Muster};
use klon3d_sim::netzkoerper::{clo_nach_szene, SUCHTIEFE};
use klon3d_sim::platzierung::{
    anziehen, lies_lage, platziere_mit, Anziehen, Huellen, Koerperteile,
};
use klon3d_sim::{
    ImplizitCg, ImplizitParameter, Koerper, Koerpernetz, Simparameter, Szene, Teilvorgabe,
    RADICAL_RUBBER_04,
};

fn wurzel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

fn resizing() -> Option<PathBuf> {
    let o = klon3d_koerper::koerper::ordner();
    if o.is_dir() {
        Some(o)
    } else {
        eprintln!("übersprungen: {} fehlt", o.display());
        None
    }
}

fn muster() -> Muster {
    Muster::laden(
        &wurzel().join("bluse3_v2.sm2d"),
        Some(&wurzel().join("measurements.smis")),
    )
    .unwrap()
}

/// A short run through all three phases (a debug-build frame of this scene
/// takes about 15 s): 2 sewing frames, at most 2 settling frames, 1 hold frame.
fn kurz() -> Anziehen {
    Anziehen {
        naehbilder: 2,
        einschwingen_max: 2,
        haltebilder: 1,
        ..Anziehen::default()
    }
}

#[test]
fn muster_netz_und_naehte() {
    let m = muster();
    let defs = klon3d_pipeline::nahtliste(&wurzel().join("bluse3_v2.naehte")).unwrap();
    let b = m.nahtbericht(&defs).unwrap();
    let g = b.groesste_abweichung().unwrap();
    assert_eq!(g.kennung, "N07L");
    assert_eq!(format!("{:+.4}", g.diff_cm), "-0.0737");
    let netz = m.netz(&defs, &Parameter::default()).unwrap();
    assert_eq!(netz.teile.len(), 28);
    assert!(netz.fehler.is_empty(), "{:?}", netz.fehler);
}

#[test]
fn fehler_sind_benannt() {
    let e = Muster::laden(&wurzel().join("gibt_es_nicht.sm2d"), None)
        .err()
        .unwrap();
    assert!(matches!(e, klon3d_pipeline::Fehler::Muster(_)), "{e:?}");
    let e = klon3d_pipeline::lageliste(&wurzel().join("gibt_es_nicht.lage"))
        .err()
        .unwrap();
    assert!(matches!(e, klon3d_pipeline::Fehler::Datei(_)), "{e:?}");
}

#[test]
fn kette_bis_zum_bild() {
    let Some(ordner) = resizing() else { return };
    let m = muster();
    let defs = klon3d_pipeline::nahtliste(&wurzel().join("bluse3_v2.naehte")).unwrap();
    let netz = m.netz(&defs, &Parameter::default()).unwrap();
    assert_eq!(netz.teile.len(), 28);
    let lage = klon3d_pipeline::lageliste(&wurzel().join("bluse3_v2.lage")).unwrap();
    let k =
        klon3d_pipeline::koerper(Koerperquelle::MassdateiMitOrdner(m.massdatei(), ordner)).unwrap();
    assert!(k.herkunft.contains("measurements.smis"), "{}", k.herkunft);

    // The scene holds exactly the placed pieces of the mesh.
    let p = klon3d_pipeline::platzieren(&netz, &lage, &klon3d_pipeline::huellen(&k).unwrap(), true)
        .unwrap();
    assert!(
        !p.netz.teile.is_empty() && p.netz.teile.len() + p.weggelassen.len() == netz.teile.len()
    );
    let mut s = klon3d_pipeline::szene(
        &netz,
        &lage,
        &k,
        RADICAL_RUBBER_04,
        0.0,
        klon3d_pipeline::standard_simparameter(),
    )
    .unwrap();
    let punkte: usize = p.netz.teile.iter().map(|t| t.punkte.len()).sum();
    let dreiecke: usize = p.netz.teile.iter().map(|t| t.dreiecke.len()).sum();
    assert_eq!(s.x.len(), punkte);
    assert_eq!(s.dreiecke.len(), dreiecke);
    assert_eq!(s.teile.len(), p.netz.teile.len());

    // One frame moves the cloth and leaves the masses alone.
    let (x0, masse0) = (s.x.clone(), s.masse.clone());
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    klon3d_pipeline::bild(&mut s, &mut rw);
    assert_ne!(s.x, x0);
    assert_eq!(s.masse, masse0);
}

/// The scene exactly as `klon3d anziehen` built it before the library
/// existed: its own evaluation, seam check, meshing, body, hulls and per-piece
/// material.
fn szene_wie_bisher(ordner: &std::path::Path) -> Szene {
    let muster = klon3d_pattern::read_path(&wurzel().join("bluse3_v2.sm2d")).unwrap();
    let ev = Auswertung::lade(&muster, &wurzel().join("measurements.smis")).unwrap();
    let defs = lies_nahtliste(&std::fs::read_to_string(wurzel().join("bluse3_v2.naehte")).unwrap())
        .unwrap();
    let bericht = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let netz = vernetze(
        &ev,
        Kurvenquelle::Seamly,
        &bericht.naehte,
        &Parameter::default(),
    )
    .unwrap();
    let liste =
        lies_lage(&std::fs::read_to_string(wurzel().join("bluse3_v2.lage")).unwrap()).unwrap();
    let smis = klon3d_pattern::lies_smis(&wurzel().join("measurements.smis")).unwrap();
    let kp = klon3d_koerper::koerper::aus_smis(ordner, &|n| smis.wert(n)).unwrap();
    let an = klon3d_koerper::koerper::anordnung(ordner, &kp.netz).unwrap();
    let koerper = Koerpernetz::aus_clo_mm(&kp.netz.punkte, &kp.netz.dreiecke, SUCHTIEFE).unwrap();
    let c = clo_nach_szene;
    let huellen = Huellen::neu(&Koerperteile {
        punkte: &koerper.punkte,
        dreiecke: &koerper.dreiecke,
        rumpf: &an.rumpf,
        arme: [&an.arme[0], &an.arme[1]],
        hals_vorn: c(an.hals_vorn),
        hals_hinten: c(an.hals_hinten),
        hals_seitlich: an.hals_seitlich.map(c),
        schulter: an.schulter.map(c),
        ellbogen: an.ellbogen.map(c),
    })
    .unwrap();
    let p = platziere_mit(&netz, &liste, &huellen, true).unwrap();
    let material = RADICAL_RUBBER_04;
    let vg = |name: &str| {
        let dicke = p
            .netz
            .teile
            .iter()
            .position(|t| t.teil == name)
            .and_then(|ti| p.dicke(ti));
        Teilvorgabe {
            material: dicke.map_or(material, |d| material.mit_dicke(d)),
            reduktion: 0.0,
        }
    };
    let param = Simparameter {
        schwerkraft: [0.0, 0.0, -9.81],
        daempfung: 1.0,
        naht_nachgiebigkeit: 0.0,
        ..Simparameter::default()
    };
    let mut s = Szene::aus_netz(
        &p.netz,
        &vg,
        &|ti, q| p.lage(ti, q),
        vec![Koerper::netz(koerper.clone())],
        param,
    );
    let zonen = p.klebungen(&s).into_iter().map(|(z, _)| z).collect();
    s.klebungen_setzen(zonen);
    s
}

/// Library chain + frame-by-frame dressing against the previous command-line
/// path (`platzierung::anziehen` in one call), bounded run: same end
/// positions to 1e-9, same report, same frame count.
#[test]
fn gleich_wie_bisheriger_weg() {
    let Some(ordner) = resizing() else { return };
    let a = kurz();

    let mut alt = szene_wie_bisher(&ordner);
    let (x_alt, masse_alt) = (alt.x.clone(), alt.masse.clone());
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let mut bilder_alt = 0;
    let erg_alt = anziehen(&mut alt, &mut rw, &a, &mut |b, _, _| bilder_alt = b);

    let m = muster();
    let defs = klon3d_pipeline::nahtliste(&wurzel().join("bluse3_v2.naehte")).unwrap();
    let netz = m.netz(&defs, &Parameter::default()).unwrap();
    let lage = klon3d_pipeline::lageliste(&wurzel().join("bluse3_v2.lage")).unwrap();
    let k =
        klon3d_pipeline::koerper(Koerperquelle::MassdateiMitOrdner(m.massdatei(), ordner)).unwrap();
    let mut neu = klon3d_pipeline::szene(
        &netz,
        &lage,
        &k,
        RADICAL_RUBBER_04,
        0.0,
        klon3d_pipeline::standard_simparameter(),
    )
    .unwrap();
    assert_eq!(neu.x, x_alt);
    assert_eq!(neu.masse, masse_alt);
    let mut rw2 = ImplizitCg::neu(ImplizitParameter::default());
    let vorher = rw2.param;
    let mut v = ablauf::Anziehvorgang::beginnen(&mut neu, &rw2, &a);
    let mut bilder_neu = 0;
    while let Some(b) = v.schritt(&mut neu, &mut rw2) {
        assert_eq!(b, bilder_neu + 1);
        bilder_neu = b;
    }
    assert_eq!(v.phase(), ablauf::Phase::Fertig);
    assert_eq!(v.schritt(&mut neu, &mut rw2), None);
    let erg_neu = v.ergebnis().unwrap().clone();

    assert_eq!(bilder_neu, bilder_alt);
    assert_eq!(bilder_neu, 2 + 2 + 1);
    assert_eq!(erg_neu, erg_alt);
    assert_eq!(rw2.param, vorher);
    assert_eq!(neu.param.schwerkraft, alt.param.schwerkraft);
    assert_eq!(neu.x.len(), alt.x.len());
    let d = neu
        .x
        .iter()
        .zip(&alt.x)
        .flat_map(|(p, q)| (0..3).map(move |i| (p[i] - q[i]).abs()))
        .fold(0.0, f64::max);
    assert!(d <= 1e-9, "largest difference {d:e} m");
    assert_eq!(rw2.statistik.loesungen, rw.statistik.loesungen);
}
