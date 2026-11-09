//! Fault 2 of the test blouse, with a cap that has something to ease (#111).
//!
//! `bluse2_stehend` measures the five faults of the delivered v26 and finds
//! that fault 2 - the sleeve cap standing off the shoulder - cannot appear:
//! `#ArmholeSolved` cuts the armhole exactly as long as the cap, so there is no
//! surplus that has to be eased in (`aermelkugel.rs` measures why on the
//! pattern). A cap that is sewn to an equally long armhole lies on the shoulder,
//! and that is the correct result for v26.
//!
//! This test supplies the missing ingredient. `bluse2_v26_einhalte.sm2d` is a
//! **test copy** of v26 in which both cap seams are 15.0 mm longer than their
//! armhole (see `kappe_einhalte.rs` in `klon3d-pattern`, and `tools/kappe_kopie.py`
//! for how the copy is generated). The delivered pattern is untouched, so every
//! acceptance number of `bluse2_stehend` keeps its meaning.
//!
//! The copy is dressed twice with the settings of the other test patterns, and
//! both runs differ in one switch only:
//!
//! * [`Simparameter::einhalteweite`] **off** - the seam springs pull the pairs
//!   together and the solver splits the surplus half and half. That is the state
//!   before #109.
//! * **on** - the longer side carries shortened rest lengths along its boundary
//!   and has to gather, so the armhole keeps its length.
//!
//! The result, 719 s for both runs: the cap still lies on the shoulder. Air
//! under it 1.0 mm in the median without the switch and **0.9 mm** with it,
//! against the fault list's 10 mm. The switch does reach the seam - 520 edges
//! shortened, the armhole side drops from +18.41 % to +11.55 % of its pattern
//! length while the cap side drops from +8.16 % to +6.74 % - but the surplus
//! leaves through the armhole: sewn, the armhole side is 76.5 mm longer than its
//! pattern against 35.1 mm on the cap side. The armhole is cut 41.5 cm long
//! around a 41 cm body and has slack to give before fabric has to gather.
//!
//! So the answer to fault 2 is a negative with a reason: for this pattern the
//! ease has nowhere to lift the cap, and the switch is not what is missing. A cap
//! that stands off needs both - ease along the cap **and** an armhole that does
//! not stretch away under it. Recorded in `PLAN.md` under #111.
//!
//! Reported, not asserted: the air under the cap over the shoulder, with the
//! selection of fault 2 (sleeve fabric within 5 cm of the cap seam, above the
//! highest seam point minus 6 cm, area-weighted median). The fault list's floor
//! is 10 mm; the delivered v26 sits at 0.5-0.7 mm. The number is the result.
//!
//! Asserted is what must hold for that number to mean anything: the length
//! measurement itself (placed against pattern within 5 %, so the strain
//! percentages are not measuring the wrong thing), the copy's 15 mm of ease, that
//! the run settles, no point sits in the skin, the seams close, and that the ease
//! switch is visible both in the scene and in the cap seam's strain. If the
//! switch did nothing, the air below would be a measurement of the switch being
//! off.
//!
//! The blouse is latex (`Radical 0.4`), as the delivered pattern is. #109 showed
//! that a woven fabric holds the ease (shorter side +0.53 % of its length) and
//! latex does not (+5.08 %), because latex stretches instead of gathering. The
//! run therefore reports the material and the strains, and says with these
//! numbers that the ease does not survive a full garment in latex - it is not
//! silently dressed in a fabric the pattern was not made for.
//!
//! Needs the data of the reference installation (main machine only); without it
//! the test says `übersprungen: … fehlt` and returns. Release only, two dressing
//! runs.

use std::path::PathBuf;

use klon3d_mesh::{vernetze, Musternetz, Parameter};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::platzierung::{anziehen, lies_lage, platziere, Anziehen, Huellen, Platzierung};
use klon3d_sim::{
    messung, ImplizitCg, ImplizitParameter, Koerper, Koerpernetz, Simparameter, Szene, Teilvorgabe,
    RADICAL_RUBBER_04,
};

mod band;

/// The test copy and its seam list.
const MUSTER: &str = "bluse2_v26_einhalte";

/// The two cap seams; side A is the sleeve, side B the armhole.
const KAPPENNAEHTE: [&str; 2] = ["V13", "V14"];
const AERMEL: [&str; 2] = ["Aermel Kurz Links", "Aermel Kurz Rechts"];

/// The fault list's floor for fault 2: air under the cap a finger finds (m).
const LUFT_GRENZE: f64 = 0.010;
/// Cap fabric within this distance of the cap seam counts as the cap (mm).
const KAPPENNAEHE_MM: f64 = 50.0;
/// Only the part of the cap this far below its highest seam point is measured,
/// because that is where the real garment stands off (m).
const KAPPENBAND: f64 = 0.06;

fn aufbau() -> Option<(Platzierung, Koerpernetz, Huellen)> {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster"));
    let pfad = dir.join(format!("{MUSTER}.sm2d"));
    let naehte = dir.join(format!("{MUSTER}.naehte"));
    let lage = dir.join(format!("{MUSTER}.lage"));
    for p in [&pfad, &naehte, &lage] {
        if !p.exists() {
            eprintln!("übersprungen: {} fehlt", p.display());
            return None;
        }
    }
    let (k, h) = band::pruefkoerper()?;
    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &pfad.with_file_name("measurements.smis")).unwrap();
    let defs = lies_nahtliste(&std::fs::read_to_string(&naehte).unwrap()).unwrap();
    let b = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
    let netz = vernetze(&ev, Kurvenquelle::Seamly, &b.naehte, &Parameter::default()).unwrap();
    let liste = lies_lage(&std::fs::read_to_string(&lage).unwrap()).unwrap();
    let p = platziere(&netz, &liste, &h).unwrap();
    Some((p, k, h))
}

/// Index of a piece by its name, or by the name behind the last ` - `.
fn teil_index(netz: &Musternetz, name: &str) -> Option<usize> {
    let treffer: Vec<usize> = (0..netz.teile.len())
        .filter(|&i| netz.teile[i].teil == name || netz.teile[i].teil.rsplit(" - ").next() == Some(name))
        .collect();
    match treffer[..] {
        [i] => Some(i),
        _ => None,
    }
}

/// Global scene indices of one seam side (`seite_a` = side A of the seam list),
/// in sewing order. Intended for point sets only (neighbour search); for a
/// length use [`nahtlaenge`], because no boundary piece sits between two partial
/// seams.
fn nahtseite(netz: &Musternetz, s: &Szene, kennung: &str, seite_a: bool) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for n in netz.naehte.iter().filter(|n| n.kennung == kennung) {
        for tn in &n.teilnaehte {
            let start = s.teile[if seite_a { tn.teil_a } else { tn.teil_b }].punkte.start;
            for pp in &tn.paare {
                v.push(start + if seite_a { pp.a as usize } else { pp.b as usize });
            }
        }
    }
    v
}

/// Length of one seam side in the scene (m), summed partial seam by partial
/// seam.
///
/// **No** boundary piece lies between two partial seams: the points belong to
/// different pieces, or to two sections of the same piece that are not adjacent.
/// A single continuous point row would measure a chord across half the piece and
/// report the length in centimetres too long.
fn nahtlaenge(netz: &Musternetz, s: &Szene, kennung: &str, seite_a: bool) -> f64 {
    let mut gesamt = 0.0;
    for n in netz.naehte.iter().filter(|n| n.kennung == kennung) {
        for tn in &n.teilnaehte {
            let start = s.teile[if seite_a { tn.teil_a } else { tn.teil_b }].punkte.start;
            let reihe: Vec<usize> = tn
                .paare
                .iter()
                .map(|pp| start + if seite_a { pp.a as usize } else { pp.b as usize })
                .collect();
            gesamt += laenge(s, &reihe);
        }
    }
    gesamt
}

fn median(v: &[f64]) -> f64 {
    let mut w = v.to_vec();
    w.sort_by(f64::total_cmp);
    w[w.len() / 2]
}

fn groesster(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

/// Length of a point row in the scene (m).
fn laenge(s: &Szene, indizes: &[usize]) -> f64 {
    indizes
        .windows(2)
        .map(|w| {
            let (a, b) = (s.x[w[0]], s.x[w[1]]);
            ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
        })
        .sum()
}

/// What one dressing of the copy yields.
struct Lauf {
    /// Air under the cap over the shoulder, area-weighted median (m).
    kugel_med: f64,
    kugel_max: f64,
    kugel_punkte: usize,
    /// Edges whose rest length the ease switch shortened.
    einhalte_kanten: usize,
    /// Length of both cap sides and both armhole sides once sewn, in mm
    /// (mean of the two seams).
    kappe_mm: f64,
    armloch_mm: f64,
    /// Strain of both sides in per cent of the pattern length.
    kappe_prozent: f64,
    armloch_prozent: f64,
    /// The same lengths right after placing, before any solver step. They have
    /// to show the pattern length, otherwise [`nahtlaenge`] measures wrongly.
    kappe_gelegt_mm: f64,
    armloch_gelegt_mm: f64,
    /// Pattern length of both sides, likewise as a mean (mm).
    kappe_muster_mm: f64,
    armloch_muster_mm: f64,
    /// Share of seam pairs that are closed.
    naht_anteil: f64,
    tiefster: f64,
    v_ende: f64,
}

/// Mean cap and armhole length over V13 and V14 (mm).
fn kappenlaengen(netz: &Musternetz, s: &Szene) -> (f64, f64) {
    let (mut kappe, mut armloch) = (0.0, 0.0);
    for kennung in KAPPENNAEHTE {
        kappe += nahtlaenge(netz, s, kennung, true) * 1e3;
        armloch += nahtlaenge(netz, s, kennung, false) * 1e3;
    }
    (kappe / 2.0, armloch / 2.0)
}

/// A full dressing of the copy, `einhalteweite` as given.
fn lauf(p: &Platzierung, k: &Koerpernetz, einhalteweite: bool) -> Lauf {
    let vg = |name: &str| {
        let dicke = p.netz.teile.iter().position(|t| t.teil == name).and_then(|ti| p.dicke(ti));
        Teilvorgabe {
            material: dicke.map_or(RADICAL_RUBBER_04, |d| RADICAL_RUBBER_04.mit_dicke(d)),
            reduktion: 0.0,
        }
    };
    let param = Simparameter {
        schwerkraft: [0.0, 0.0, -9.81],
        daempfung: 1.0,
        einhalteweite,
        ..Simparameter::default()
    };
    // Mit gefalteter Nahtzugabe, wie die Abnahme ohne Einhalteweite.
    let (mut s, falt) =
        Szene::aus_netz_mit_falte(&p.netz, &vg, &|ti, q| p.lage(ti, q), vec![Koerper::netz(k.clone())], param);
    eprintln!(
        "  Zugabe: {} Teile mit Band, {} Punkte, {} Dreiecke, {:.1} mm², {} Falzkanten \
         ({} von einer Gehrung verhindert), Winkel {:.4}..{:.4} Grad, \
         {} Dehnkanten in der Form gedeckelt",
        falt.teile,
        falt.punkte,
        falt.dreiecke,
        falt.flaeche_mm2,
        falt.falzkanten,
        falt.falzkanten_gehrung,
        falt.falz_winkel_min_rad.to_degrees(),
        falt.falz_winkel_max_rad.to_degrees(),
        falt.kanten_form_gedeckelt
    );
    let (kappe_gelegt_mm, armloch_gelegt_mm) = kappenlaengen(&p.netz, &s);
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let ablauf = Anziehen::default();
    anziehen(&mut s, &mut rw, &ablauf, &mut |_, _, _| {});

    // Cap air: sleeve fabric within 5 cm of the cap seam, above the shoulder.
    let mut kugel: Vec<(usize, f64)> = Vec::new();
    for (kennung, teil) in KAPPENNAEHTE.iter().zip(AERMEL) {
        let ti = teil_index(&p.netz, teil).unwrap();
        let naht = nahtseite(&p.netz, &s, kennung, true);
        let start = s.teile[ti].punkte.start;
        let naehe: Vec<[f64; 2]> = naht.iter().map(|&i| p.netz.teile[ti].punkte[i - start]).collect();
        let oben = naht.iter().map(|&i| s.x[i][2]).fold(f64::NEG_INFINITY, f64::max) - KAPPENBAND;
        for (lokal, q) in p.netz.teile[ti].punkte.iter().enumerate() {
            let dicht = naehe
                .iter()
                .any(|n| ((n[0] - q[0]).powi(2) + (n[1] - q[1]).powi(2)).sqrt() < KAPPENNAEHE_MM);
            let i = start + lokal;
            if dicht && s.x[i][2] > oben {
                kugel.push((i, k.naechster_alle(s.x[i]).abstand));
            }
        }
    }
    assert!(kugel.len() > 40, "Ärmelkugel: nur {} Punkte", kugel.len());
    let kugel_d: Vec<f64> = kugel.iter().map(|x| x.1).collect();

    let (kappe_mm, armloch_mm) = kappenlaengen(&p.netz, &s);
    let (kappe_muster_mm, armloch_muster_mm) = {
        let mut a = 0.0;
        let mut b = 0.0;
        for kennung in KAPPENNAEHTE {
            let n = p.netz.naehte.iter().find(|n| n.kennung == kennung).unwrap();
            a += n.laenge_a_mm;
            b += n.laenge_b_mm;
        }
        (a / 2.0, b / 2.0)
    };
    // Strain in per cent of the pattern length: a positive number means longer.
    let kappe_prozent = (kappe_mm / kappe_muster_mm - 1.0) * 100.0;
    let armloch_prozent = (armloch_mm / armloch_muster_mm - 1.0) * 100.0;

    let (mut alle, mut zu) = (0usize, 0usize);
    for (_, d) in messung::nahtabstaende(&s, &p.netz) {
        alle += d.len();
        zu += d.iter().filter(|&&x| x <= 0.01).count();
    }

    Lauf {
        kugel_med: median(&kugel_d),
        kugel_max: groesster(&kugel_d),
        kugel_punkte: kugel.len(),
        einhalte_kanten: s.einhalte_kanten,
        kappe_mm,
        armloch_mm,
        kappe_prozent,
        armloch_prozent,
        kappe_gelegt_mm,
        armloch_gelegt_mm,
        kappe_muster_mm,
        armloch_muster_mm,
        naht_anteil: zu as f64 / alle as f64,
        tiefster: s.x.iter().map(|&q| k.naechster_alle(q).abstand).fold(f64::INFINITY, f64::min),
        v_ende: messung::mittlere_geschwindigkeit(&s),
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn kappe_mit_einhalteweite_am_koerper() {
    let Some((p, k, _)) = aufbau() else { return };

    // The copy carries the ease as part of the pattern, not through the switch:
    // that is the basis of both runs.
    let mut weiten = Vec::new();
    for kennung in KAPPENNAEHTE {
        let n = p.netz.naehte.iter().find(|n| n.kennung == kennung).unwrap();
        weiten.push(n.laenge_a_mm - n.laenge_b_mm);
    }
    eprintln!(
        "\n== {MUSTER} am Körper (Latex, Radical 0.4): Kappe {:.1} / {:.1} mm länger als das Armloch ==",
        weiten[0], weiten[1]
    );

    let ohne = lauf(&p, &k, false);
    let mit = lauf(&p, &k, true);

    eprintln!(
        "  Einhalteweite aus: Kappe {:.1} mm / Armloch {:.1} mm genäht = Kappenseite {:+.2} %, \
         Armlochseite {:+.2} %, {} Kanten verkürzt",
        ohne.kappe_mm, ohne.armloch_mm, ohne.kappe_prozent, ohne.armloch_prozent, ohne.einhalte_kanten
    );
    eprintln!(
        "  Einhalteweite an:  Kappe {:.1} mm / Armloch {:.1} mm genäht = Kappenseite {:+.2} %, \
         Armlochseite {:+.2} %, {} Kanten verkürzt",
        mit.kappe_mm, mit.armloch_mm, mit.kappe_prozent, mit.armloch_prozent, mit.einhalte_kanten
    );
    for (name, l) in [("aus", &ohne), ("an", &mit)] {
        eprintln!(
            "  Kappenluft [2] {}: {} Punkte, Luft im Median {:.1} mm, höchstens {:.1} mm (Boden {:.0} mm)",
            name,
            l.kugel_punkte,
            l.kugel_med * 1e3,
            l.kugel_max * 1e3,
            LUFT_GRENZE * 1e3
        );
        eprintln!(
            "      Nahtpaare bis 10 mm {:.1} %, Ruhe {:.4} m/s, tiefster Punkt {:+.2} mm",
            l.naht_anteil * 100.0,
            l.v_ende,
            l.tiefster * 1e3
        );
    }

    // --- The measurement itself has to be right -----------------------------
    // Right after placing the net still lies on the pattern: the sewn sides have
    // to show roughly their pattern length there. The placement maps the pattern
    // onto the body shell and may gain a few millimetres doing so; if it is off
    // by per cent, `nahtlaenge` measures wrongly and the strain figures above
    // are worthless.
    for (name, l) in [("aus", &ohne), ("an", &mit)] {
        for (seite, gelegt, muster) in [
            ("Kappe", l.kappe_gelegt_mm, l.kappe_muster_mm),
            ("Armloch", l.armloch_gelegt_mm, l.armloch_muster_mm),
        ] {
            let abw = (gelegt - muster).abs() / muster * 100.0;
            eprintln!("  {name}: {seite} aufgelegt {gelegt:.1} mm gegen Muster {muster:.1} mm ({abw:+.2} %)");
            assert!(
                abw < 5.0,
                "{name}: {seite} aufgelegt {gelegt:.1} mm gegen Muster {muster:.1} mm - Längenmessung falsch"
            );
        }
        // The copy carries 15 mm of ease, so the cap is longer than the armhole.
        assert!(
            l.kappe_muster_mm - l.armloch_muster_mm > 14.0,
            "{name}: Kappe nur {:.1} mm länger als das Armloch",
            l.kappe_muster_mm - l.armloch_muster_mm
        );
    }

    // --- What must hold for the figure above to mean anything --------------
    for (name, l) in [("aus", &ohne), ("an", &mit)] {
        assert!(
            l.kugel_punkte > 40,
            "{name}: Ärmelkugel nur {} Punkte",
            l.kugel_punkte
        );
        assert!(l.tiefster > -0.1e-3, "{name}: Punkt {:.2} mm in der Haut", l.tiefster * 1e3);
        assert!(l.v_ende < 0.015, "{name}: nicht in Ruhe, {:.4} m/s", l.v_ende);
        assert!(
            l.naht_anteil >= 0.98,
            "{name}: nur {:.1} % der Nahtpaare zu",
            l.naht_anteil * 100.0
        );
    }

    // The switch does something: it shortens rest lengths, and at both cap
    // seams. Without it the scene stays untouched.
    assert_eq!(ohne.einhalte_kanten, 0, "Ease hat ohne Schalter Kanten verkürzt");
    assert!(
        mit.einhalte_kanten > 0,
        "Ease hat keine Kante verkürzt - die Zahl oben misst den Schalter nicht"
    );

    // The switch reaches both sides of the seam, and it is the armhole it
    // relieves: with it the armhole side stretches 6.86 pp less, the cap side
    // 1.41 pp. So the armhole is the side that was carrying the slack - the
    // reason the cap cannot lift off. Without that ordering the surplus went
    // somewhere the measurement does not see, and the figure above is mute.
    let kappe_pp = ohne.kappe_prozent - mit.kappe_prozent;
    let armloch_pp = ohne.armloch_prozent - mit.armloch_prozent;
    eprintln!(
        "  Überschuss: mit dem Schalter dehnt sich die Kappenseite {kappe_pp:.2} pp weniger, \
         das Armloch {armloch_pp:.2} pp"
    );
    assert!(
        kappe_pp > 0.0,
        "Schalter ohne Wirkung auf die Kappenseite: {kappe_pp:.2} pp"
    );
    assert!(
        armloch_pp > kappe_pp,
        "Armloch gibt {armloch_pp:.2} pp nach, Kappe {kappe_pp:.2} pp - nicht das Armloch trägt den Überschuss"
    );
}
