//! Fault 2 of the test blouse, as a dose curve of cap ease (#115).
//!
//! `bluse2_stehend` measures the five faults of the delivered v26 and finds that
//! fault 2 - the sleeve cap standing off the shoulder - cannot appear: the
//! pattern carries no ease over the armhole, so the sewn cap sits at 0.5 mm
//! (`aermelkugel.rs` measures why on the pattern).
//!
//! #111 supplied the missing ingredient in the smallest plausible dose:
//! `bluse2_v26_einhalte.sm2d` has cap seams **15.0 mm** longer than their
//! armholes. Dressed with the ease switch on, the cap still lay on the shoulder -
//! 0.9 mm of air against a 10 mm floor - and the run names the reason: the
//! armhole is cut 41.5 cm long around a 41 cm body, so the surplus leaves through
//! the armhole instead of lifting the cap.
//!
//! The next obvious step, a shorter armhole, does not work. The cap reads the
//! armhole length through `cutSplinePath`, so both sides lose the same
//! millimetres: measured on a throw-away copy, cap and armhole both fell by
//! 10.0 mm and the ease stayed at +1.500 cm. So the open question is the
//! **dose**, and this test answers it with a second point:
//! `bluse2_v26_kappe40.sm2d` carries **40.0 mm** of ease per cap seam
//! (`#CapEase = 2` instead of `0.75`; one formula, no moved node, the seam list
//! and the placement of #111 apply unchanged - see `tools/kappe_dosis.py` and
//! `kappe_dosis.rs` in `klon3d-pattern`).
//!
//! Both copies are dressed here under the same conditions, so the two points sit
//! in one table: ease in, air under the cap out. That is what the fault list
//! needs - not another single case, but whether the stand-off follows the ease
//! and where it crosses the floor.
//!
//! **Measured: it does not follow far enough.** 40 mm give 1.3 mm of air in the
//! median against #111's 0.9 mm - 0.4 mm of air for 25 mm of extra ease - and
//! the floor of 10 mm stays out of reach. The run says where the surplus goes:
//! the armhole side's stretch rises with the dose (18.41 % → 24.90 %) while the
//! cap side's falls (8.16 % → 7.19 %), because the armhole is 41.5 cm around a
//! 41 cm body and latex stretches instead of gathering. Fault 2 is therefore
//! neither a solver knob nor a question of dose; the fault list keeps "not
//! shown".
//!
//! The delivered `bluse2_v26.sm2d` stays untouched, so every acceptance number of
//! `bluse2_stehend` keeps its meaning.
//!
//! Needs the reference body; without it the test prints `übersprungen: … fehlt`
//! and returns. Release only, four dressing runs.

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

/// The dose copy and the #111 copy it is compared with.
const MUSTER: &str = "bluse2_v26_kappe40";
const MUSTER_111: &str = "bluse2_v26_einhalte";

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
/// The ease this copy carries per cap seam (mm), for the self-check.
const DOSIS_MM: f64 = 40.0;

fn aufbau(name: &str) -> Option<(Platzierung, Koerpernetz, Huellen)> {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../klon3d-pattern/tests/muster"));
    let pfad = dir.join(format!("{name}.sm2d"));
    let naehte = dir.join(format!("{name}.naehte"));
    let lage = dir.join(format!("{name}.lage"));
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

/// What one dressing of a copy yields.
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
    /// The same lengths right after placing, before any solver step.
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

/// A full dressing of a copy, `einhalteweite` as given.
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
    let mut s = Szene::aus_netz(&p.netz, &vg, &|ti, q| p.lage(ti, q), vec![Koerper::netz(k.clone())], param);
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
fn vierzig_millimeter_mehrweite_am_koerper() {
    let Some((p, k, _)) = aufbau(MUSTER) else { return };
    let Some((p111, k111, _)) = aufbau(MUSTER_111) else { return };

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
        // The copy carries 40 mm of ease, so the cap is clearly longer.
        let d = l.kappe_muster_mm - l.armloch_muster_mm;
        assert!(
            (d - DOSIS_MM).abs() < 0.5,
            "{name}: Kappe {d:.1} mm länger als das Armloch, erwartet {DOSIS_MM:.1} mm"
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

    // The switch does something: it shortens rest lengths. Without it the scene
    // stays untouched.
    assert_eq!(ohne.einhalte_kanten, 0, "Ease hat ohne Schalter Kanten verkürzt");
    assert!(
        mit.einhalte_kanten > 0,
        "Ease hat keine Kante verkürzt - die Zahl oben misst den Schalter nicht"
    );

    // --- The dose curve ----------------------------------------------------
    // Both doses are dressed here, on the same body with the same settings, so
    // the two points of the curve cannot drift apart: ease in, air out.
    let ohne111 = lauf(&p111, &k111, false);
    let mit111 = lauf(&p111, &k111, true);
    eprintln!(
        "  #111 (15 mm): Einhalteweite aus Kappenluft {:.1} mm, an {:.1} mm",
        ohne111.kugel_med * 1e3,
        mit111.kugel_med * 1e3
    );

    // Four times the ease has to give at least the air of the small dose. If it
    // gives less, the extra length went somewhere the measurement does not see
    // (it left through the armhole again) and the curve is not a curve.
    assert!(
        mit.kugel_med >= mit111.kugel_med - 0.2e-3,
        "viermal so viel Mehrweite gibt {:.1} mm Luft gegen {:.1} mm bei 15 mm - der Überschuss \
         hebt die Kappe nicht",
        mit.kugel_med * 1e3,
        mit111.kugel_med * 1e3
    );

    // The dose has to reach the cap side. If the armhole takes all of it, the
    // cap side strain does not grow with the dose, and that is the reason for a
    // low figure - so it is measured, not assumed.
    let kappe_pp = ohne.kappe_prozent - mit.kappe_prozent;
    let armloch_pp = ohne.armloch_prozent - mit.armloch_prozent;
    let kappe_pp111 = ohne111.kappe_prozent - mit111.kappe_prozent;
    eprintln!(
        "  Überschuss 40 mm: mit dem Schalter dehnt sich die Kappenseite {kappe_pp:.2} pp weniger, \
         das Armloch {armloch_pp:.2} pp (bei 15 mm: Kappe {kappe_pp111:.2} pp)"
    );
    assert!(
        kappe_pp > 0.0,
        "Schalter ohne Wirkung auf die Kappenseite: {kappe_pp:.2} pp"
    );

    // The two points in one table.
    for (name, l, dosis) in [
        ("aus", &ohne111, 15.0),
        ("an", &mit111, 15.0),
        ("aus", &ohne, 40.0),
        ("an", &mit, 40.0),
    ] {
        eprintln!(
            "  Dosis {name:>3}: Mehrweite {dosis:>4.0} mm -> Kappenluft {:.1} mm (höchstens {:.1} mm), \
             Kappe {:+.2} % / Armloch {:+.2} %",
            l.kugel_med * 1e3,
            l.kugel_max * 1e3,
            l.kappe_prozent,
            l.armloch_prozent
        );
    }

    // The fault is shown exactly when the measured air reaches the floor.
    let gezeigt = mit.kugel_med >= LUFT_GRENZE;
    eprintln!(
        "  Fehler 2 mit 40 mm Mehrweite: {} ({:.1} mm gegen Boden {:.0} mm)",
        if gezeigt { "GEZEIGT" } else { "nicht gezeigt" },
        mit.kugel_med * 1e3,
        LUFT_GRENZE * 1e3
    );
}
