//! Stage 4d: test blouse v26 standing on the shaped body - and the five faults
//! the real garment shows.
//!
//! The pattern is the user's test blouse (`bluse2_v26.sm2d`, 22 pieces), dressed
//! on the body of its own measurement file with the settings the other test
//! patterns hold with (findings 96-102): seams close at 0.1 m/s over 200 frames
//! held without gravity, seam spring 2 000 N/m, then settle in under full
//! gravity, then a 10 s hold phase (ADR-023).
//!
//! Nothing is pinned, nothing sinks, and the seams close - those are the checks
//! every other pattern already passes. On top of them this test asks the
//! question the simulation has to answer: **does it show the five faults the
//! real blouse shows?** Each one is a named number, not "looks right", and each
//! has a floor with a reason:
//!
//! 1. **Plunge edge slides aside.** How far the plunge-edge points travel
//!    around the body as arc length, and how wide the opening stands against
//!    the flat pattern. Floor 10 mm: less than that and nothing a maker would
//!    call sliding happened. The opening at the *bust* is the number that
//!    counts; the one at the neck is reported too but not asserted, because the
//!    blouse's neckline measures 37.6 cm while the body's neck opening is only
//!    32.1 cm - 5.5 cm that cannot close regardless of what the solver does.
//! 2. **Sleeve cap stands off.** Air between the sleeve fabric at the cap and
//!    the skin, over the top of the shoulder, as the area-weighted median.
//!    Floor 10 mm of air - that is what a finger finds under the cap; the real
//!    garment stands off about 3 cm.
//!
//!    This one is expected **not** to show, and `tests/aermelkugel.rs` measures
//!    why on the pattern: `#ArmholeSolved` cuts the armhole as long as the cap,
//!    so the sleeve carries no ease along the seam (`-0.00 mm` on both cap
//!    seams) and is only 14.50 cm tall against the body's 14.0 cm. Placed but
//!    not sewn it lies 10.7 mm off the skin, and pairing two equally long edges
//!    brings that down to 0.7 mm (0.5 mm on a rerun). A real cap stands off
//!    because it is *longer* than its armhole and the surplus has to be eased
//!    in; v26 is not cut that way, so a seated cap is the correct result here.
//! 3. **Tension at the shoulder blade.** Area-weighted median strain of the
//!    back pieces in the shoulder-blade zone against the median of the same
//!    pieces. Floor 2 percentage points - above the run-to-run spread of a
//!    piece median on these patterns (0.3 pp).
//! 4. **Sleeve too tight along its length.** Strain of the sleeve around the
//!    arm at armpit height. Floor 2 %: the garment was cut with 2.2 % / 4.4 %
//!    too little and its armhole measures 41.53 cm against the body's 41 cm.
//! 5. **Collar stands open at the front.** Air between the collar's free upper
//!    edge and the skin at its two front ends. Floor 10 mm, same reason as 2.
//!
//! Measured on 2026-11-01 (CPU solve, 306-340 s over four runs): **faults 1, 3, 4 and 5 appear** -
//! the plunge opening at the bust 130.0 -> 175.3 mm, the shoulder blade +11.08
//! against +5.84 %, the sleeve at armpit height +4.94 %, the collar ends 31.2 mm
//! off the skin - **fault 2 does not**: the cap lies on the shoulder (0.7 mm
//! median air). klon3d sews the cap to the armhole at matching lengths
//! (41.5308 against 41.5310 cm), so the cap carries no excess there; on the real
//! garment the stand-off comes from the cap's shape, which a seam list cannot
//! express. The test pins that finding: if the cap ever lifts, it fails and the
//! finding has to be re-read.
//!
//! The 9 decorative discs (plunge tip, hem, armhole, frog, collar-end halves)
//! are not sewn on - they are ornaments, they carry no load, and leaving them
//! off keeps them from blocking collisions. The seam list says so.
//!
//! Needs the data of the reference installation (main machine only); without it
//! the test says `übersprungen: … fehlt` and returns. Release only.

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

const MUSTER: &str = "bluse2_v26";

/// The floor of fault 1: below this the plunge edge has not moved anywhere a
/// maker would call sliding (m).
const PLUNGE_GRENZE: f64 = 0.010;
/// The floors of faults 2 and 5: air under the fabric that a finger finds (m).
const LUFT_GRENZE: f64 = 0.010;
/// The floor of fault 3 (engineering strain, 2 percentage points).
const SCHULTERBLATT_GRENZE: f64 = 0.02;
/// The floor of fault 4 (engineering strain around the arm, 2 %).
const AERMEL_GRENZE: f64 = 0.02;
/// The plunge opening in the flat pattern: VK0_L to VK0_R and VPW_L to VPW_R (m).
const MUSTER_OEFFNUNG_HALS: f64 = 0.0354;
const MUSTER_OEFFNUNG_BRUST: f64 = 0.1300;

/// Netz und Lageliste des Prüfmusters samt den Hüllen des eigenen Körpers.
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

/// Index eines Teils über seinen Namen oder den Namen hinter dem letzten ` - `.
fn teil_index(netz: &Musternetz, name: &str) -> Option<usize> {
    let treffer: Vec<usize> = (0..netz.teile.len())
        .filter(|&i| netz.teile[i].teil == name || netz.teile[i].teil.rsplit(" - ").next() == Some(name))
        .collect();
    match treffer[..] {
        [i] => Some(i),
        _ => None,
    }
}

/// Netzpunkt eines Punktknotens als globaler Szenenindex.
fn szenenpunkt(netz: &Musternetz, s: &Szene, teil: &str, knoten: &str) -> Option<usize> {
    let ti = teil_index(netz, teil)?;
    let (_, lokal) = netz.teile[ti].punktknoten.iter().find(|(n, _)| n == knoten)?;
    Some(s.teile[ti].punkte.start + *lokal as usize)
}

/// Globale Szenenindizes einer Nahtseite (`seite_a` = Seite A der Nahtliste).
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
    v.sort_unstable();
    v.dedup();
    v
}

/// Flächenindizes der Szene, die zu den Teilen mit diesen Namen gehören.
fn teilflaechen(s: &Szene, netz: &Musternetz, namen: &[&str]) -> Vec<usize> {
    let mut v = Vec::new();
    for name in namen {
        if let Some(ti) = teil_index(netz, name) {
            v.extend(s.teile[ti].dreiecke.clone());
        }
    }
    v
}

/// Flächenschwerpunkt (m).
fn schwerpunkt(s: &Szene, k: usize) -> [f64; 3] {
    let d = s.dreiecke[k];
    let mut q = [0.0; 3];
    for &i in &d {
        for a in 0..3 {
            q[a] += s.x[i as usize][a] / 3.0;
        }
    }
    q
}

/// Fläche in der Ruhelage (m²) aus der inversen Ruhematrix, wie in
/// [`klon3d_sim::dehnung`].
fn ruheflaeche(ruhe_inv: &[f64; 4]) -> f64 {
    let det = ruhe_inv[0] * ruhe_inv[3] - ruhe_inv[1] * ruhe_inv[2];
    if det == 0.0 {
        0.0
    } else {
        0.5 / det.abs()
    }
}

/// Flächengewichteter Median der größten Hauptdehnung über die Flächen `menge`.
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

/// Abstand eines Punktes zur Körperoberfläche (m, außen positiv).
fn haut(k: &Koerpernetz, q: [f64; 3]) -> f64 {
    k.naechster_alle(q).abstand
}

/// Kürzester Abstand zweier Punkte (m).
fn abstand(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Winkel eines Punktes um die Körperachse (rad); vorn = −y, links = +x.
fn winkel(q: [f64; 3], mitte: [f64; 2]) -> f64 {
    (q[0] - mitte[0]).atan2(q[1] - mitte[1])
}

fn winkel_abstand(a: f64, b: f64) -> f64 {
    let mut d = a - b;
    while d > std::f64::consts::PI {
        d -= 2.0 * std::f64::consts::PI;
    }
    while d < -std::f64::consts::PI {
        d += 2.0 * std::f64::consts::PI;
    }
    d.abs()
}

fn median(v: &[f64]) -> f64 {
    let mut w = v.to_vec();
    w.sort_by(f64::total_cmp);
    w[w.len() / 2]
}

fn groesster(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

fn prozent(v: f64) -> f64 {
    v * 100.0
}

#[test]
#[cfg_attr(debug_assertions, ignore = "nur mit --release")]
fn bluse2_stehend() {
    let Some((p, k, h)) = aufbau() else { return };

    // Je Teil das Material in der Dicke der Lageangabe, ohne Reduktion: das
    // Muster ist so geschnitten, wie es genäht wird.
    let vg = |name: &str| {
        let dicke = p.netz.teile.iter().position(|t| t.teil == name).and_then(|ti| p.dicke(ti));
        Teilvorgabe { material: dicke.map_or(RADICAL_RUBBER_04, |d| RADICAL_RUBBER_04.mit_dicke(d)), reduktion: 0.0 }
    };
    let param = Simparameter { schwerkraft: [0.0, 0.0, -9.81], daempfung: 1.0, ..Simparameter::default() };
    // Mit gefalteter Nahtzugabe: das Band ist Stoff und hängt an der Nahtlinie,
    // die Nahtpaare und das Musternetz bleiben, wie sie sind.
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
    let x_gelegt = s.x.clone();
    let mut rw = ImplizitCg::neu(ImplizitParameter::default());
    let ablauf = Anziehen::default();

    // Am Ende des Nähens einmal abgreifen: Fehler 1 trennt, was das Anziehen
    // bewegt, von dem, was das Tragen unter Schwerkraft bewegt.
    let mut x_genaeht: Option<Vec<[f64; 3]>> = None;
    let naehbilder = ablauf.naehbilder;
    let mut schritt = |bild: usize, s: &Szene, _: &ImplizitCg| {
        if bild == naehbilder {
            x_genaeht = Some(s.x.clone());
        }
    };
    let erg = anziehen(&mut s, &mut rw, &ablauf, &mut schritt);
    let x_genaeht = x_genaeht.expect("Bild am Ende des Nähens gesehen");

    // --- Was jedes Prüfmuster können muss -------------------------------------
    assert_eq!(p.netz.teile.len(), 13, "Teile platziert (10 genäht + Trim + zwei Kragenlagen)");
    assert_eq!(p.netz.naehte.len(), 19, "Nähte im Netz");
    assert_eq!(p.weggelassen.len(), 9, "die neun Zierscheiben bleiben ohne Lage");
    assert!(s.masse.iter().all(|&m| m > 0.0), "kein Punkt darf festgehalten sein (ADR-023)");

    let v_ende = messung::mittlere_geschwindigkeit(&s);
    let tiefster = s.x.iter().map(|&q| haut(&k, q)).fold(f64::INFINITY, f64::min);
    let (mut alle, mut zu, mut je_naht) = (0usize, 0usize, Vec::new());
    for (kennung, mut d) in messung::nahtabstaende(&s, &p.netz) {
        alle += d.len();
        zu += d.iter().filter(|&&x| x <= 0.01).count();
        d.sort_by(f64::total_cmp);
        je_naht.push((kennung, d[d.len() / 2], d[d.len() - 1]));
    }
    let anteil = zu as f64 / alle as f64;

    eprintln!("\n== {MUSTER} am Körper, {} Punkte, {} Nähte ==", s.x.len(), p.netz.naehte.len());
    eprintln!(
        "  Ruhe {v_ende:.4} m/s, tiefster Punkt {:+.2} mm, Nahtpaare bis 10 mm {zu} von {alle} ({:.1} %), \
         Einschwingen {} Bilder ({}), Halt {} Bilder",
        tiefster * 1e3,
        anteil * 100.0,
        erg.einschwingbilder,
        if erg.eingeschwungen { "ruhig" } else { "nicht ruhig" },
        ablauf.haltebilder
    );

    // --- Dehnung je Fläche, einmal gerechnet ----------------------------------
    let je_flaeche = messung::dehnung_je_dreieck(&s);
    let mitte = h.hals_mitte;

    // === Fehler 1: Plunge-Kante rutscht zur Seite ============================
    // Die Plunge-Kante der Vorderteile (Seite A von V19) im Umlauf um die
    // Körperachse. Zwei Strecken, weil sie zwei Dinge sagen: Auflegen -> Ende
    // (was das Anziehen verschiebt) und Nähende -> Ende (was das Tragen unter
    // Schwerkraft verschiebt). Die Zahl, die den Fehler benennt, ist die
    // Öffnung: im Muster liegen VK0 35.4 mm und VPW 130.0 mm auseinander;
    // steht sie offen, fällt die Brust heraus.
    let plunge = nahtseite(&p.netz, &s, "V19", true);
    assert!(plunge.len() > 50, "Plunge-Kante: nur {} Punkte", plunge.len());
    let kriechen_ab = |von: &[[f64; 3]]| -> Vec<f64> {
        plunge
            .iter()
            .map(|&i| {
                let r = ((s.x[i][0] - mitte[0]).powi(2) + (s.x[i][1] - mitte[1]).powi(2)).sqrt();
                winkel_abstand(winkel(von[i], mitte), winkel(s.x[i], mitte)) * r
            })
            .collect()
    };
    let kriechen = kriechen_ab(&x_gelegt);
    let kriechen_tragen = kriechen_ab(&x_genaeht);
    let (kriechen_med, kriechen_max) = (median(&kriechen), groesster(&kriechen));
    let (tragen_med, tragen_max) = (median(&kriechen_tragen), groesster(&kriechen_tragen));
    let punkt = |name: &str, knoten: &str| szenenpunkt(&p.netz, &s, name, knoten).unwrap();
    let vpw_l = punkt("Vorderteil Mitte Plunge Links", "VPW_L");
    let vpw_r = punkt("Vorderteil Mitte Plunge Rechts", "VPW_R");
    let vk0_l = punkt("Vorderteil Mitte Plunge Links", "VK0_L");
    let vk0_r = punkt("Vorderteil Mitte Plunge Rechts", "VK0_R");
    let oeffnung = |x: &[[f64; 3]]| (abstand(x[vpw_l], x[vpw_r]), abstand(x[vk0_l], x[vk0_r]));
    let (brust_gelegt, hals_gelegt) = oeffnung(&x_gelegt);
    let (brust_genaeht, hals_genaeht) = oeffnung(&x_genaeht);
    let (oeffnung_brust, oeffnung_hals) = oeffnung(&s.x);
    eprintln!(
        "  [1] Plunge-Kante kriecht um den Körper: Auflegen -> Ende Median {:.1} mm / höchstens {:.1} mm, \
         Nähende -> Ende Median {:.1} mm / höchstens {:.1} mm",
        kriechen_med * 1e3,
        kriechen_max * 1e3,
        tragen_med * 1e3,
        tragen_max * 1e3
    );
    eprintln!(
        "      Plunge-Öffnung an der Brust: aufgelegt {:.1} mm, genäht {:.1} mm, Ende {:.1} mm \
         (flach im Muster {:.1} mm) - also {:+.1} mm gegen das Muster",
        brust_gelegt * 1e3,
        brust_genaeht * 1e3,
        oeffnung_brust * 1e3,
        MUSTER_OEFFNUNG_BRUST * 1e3,
        (oeffnung_brust - MUSTER_OEFFNUNG_BRUST) * 1e3
    );
    eprintln!(
        "      Plunge-Öffnung am Hals: aufgelegt {:.1} mm, genäht {:.1} mm, Ende {:.1} mm \
         (flach im Muster {:.1} mm) - also {:+.1} mm gegen das Muster",
        hals_gelegt * 1e3,
        hals_genaeht * 1e3,
        oeffnung_hals * 1e3,
        MUSTER_OEFFNUNG_HALS * 1e3,
        (oeffnung_hals - MUSTER_OEFFNUNG_HALS) * 1e3
    );
    // Wie groß ist das Halsloch des Körpers, an dem die Halsnaht (37.6 cm) sitzen
    // müsste? Liegt es deutlich darunter, kann die Naht nicht schließen, und die
    // Öffnung oben ist die Folge davon, nicht ein Rutschen.
    let halsumfang: f64 = (0..4)
        .map(|i| {
            let (a, b) = (h.hals_naht[i], h.hals_naht[(i + 1) % 4]);
            let da = (b.0 - a.0).rem_euclid(360.0).to_radians();
            0.5 * (a.1 + b.1) * da
        })
        .sum();
    let seiten = |i: usize| (h.hals_naht[i].0, h.hals_naht[i].1, h.hals_naht[i].2);
    eprintln!(
        "      Halsloch des Körpers {:.1} cm Umfang (Halsnaht im Muster 37.6 cm); Stützpunkte \
         (Winkel, Radius, Höhe): {:?}",
        halsumfang * 100.0,
        (0..4).map(seiten).collect::<Vec<_>>()
    );
    let vk0_l_hoehe = s.x[vk0_l][2] - h.hoehen.hals_seitlich;
    let vk0_r_hoehe = s.x[vk0_r][2] - h.hoehen.hals_seitlich;
    eprintln!(
        "      VK0 liegt {:.1} / {:.1} cm gegen den seitlichen Halspunkt (im Muster 7.30 cm darunter)",
        vk0_l_hoehe * 100.0,
        vk0_r_hoehe * 100.0
    );

    // === Fehler 2: Ärmelkugel steht ab ======================================
    // Ärmelgewebe bis 5 cm neben der Kugelnaht (Seite A von V13/V14), über der
    // Schulter: Luft zur Haut.
    let mut kugel: Vec<(usize, f64)> = Vec::new();
    for (kennung, teil) in [("V13", "Aermel Kurz Links"), ("V14", "Aermel Kurz Rechts")] {
        let ti = teil_index(&p.netz, teil).unwrap();
        let naht = nahtseite(&p.netz, &s, kennung, true);
        let start = s.teile[ti].punkte.start;
        let naehe: Vec<[f64; 2]> = naht.iter().map(|&i| p.netz.teile[ti].punkte[i - start]).collect();
        // Nur der obere Teil der Kugel: über der höchsten Stelle der Kugelnaht
        // minus 6 cm, denn dort steht sie am echten Teil ab.
        let oben = naht.iter().map(|&i| s.x[i][2]).fold(f64::NEG_INFINITY, f64::max) - 0.06;
        for (lokal, q) in p.netz.teile[ti].punkte.iter().enumerate() {
            let dicht = naehe.iter().any(|n| ((n[0] - q[0]).powi(2) + (n[1] - q[1]).powi(2)).sqrt() < 50.0);
            let i = start + lokal;
            if dicht && s.x[i][2] > oben {
                kugel.push((i, haut(&k, s.x[i])));
            }
        }
    }
    assert!(kugel.len() > 40, "Ärmelkugel: nur {} Punkte", kugel.len());
    let kugel_d: Vec<f64> = kugel.iter().map(|x| x.1).collect();
    let (kugel_med, kugel_max) = (median(&kugel_d), groesster(&kugel_d));
    eprintln!(
        "  [2] Ärmelkugel über der Schulter: {} Punkte, Luft zur Haut im Median {:.1} mm, höchstens {:.1} mm",
        kugel.len(),
        kugel_med * 1e3,
        kugel_max * 1e3
    );

    // === Fehler 3: Spannung am Schulterblatt ================================
    let ruecken = ["Rueckenteil Mitte Links", "Rueckenteil Mitte Rechts", "Rueckenteil Seite Links", "Rueckenteil Seite Rechts"];
    let ruecken_alle = teilflaechen(&s, &p.netz, &ruecken);
    // Schulterblatt: 2 bis 17 cm unter dem Schulterpunkt, auf der Rückenhälfte.
    let (z_oben, z_unten) = (h.hoehen.schulter - 0.02, h.hoehen.schulter - 0.17);
    let schulterblatt: Vec<usize> = ruecken_alle
        .iter()
        .copied()
        .filter(|&f| {
            let q = schwerpunkt(&s, f);
            (z_unten..=z_oben).contains(&q[2]) && q[1] > mitte[1]
        })
        .collect();
    assert!(schulterblatt.len() > 40, "Schulterblatt: nur {} Flächen", schulterblatt.len());
    let e_ruecken = dehnung_median(&s, &je_flaeche, &ruecken_alle);
    let e_schulterblatt = dehnung_median(&s, &je_flaeche, &schulterblatt);
    eprintln!(
        "  [3] Schulterblatt: {} Flächen, Dehnung im Median {:+.2} %, Rückenteile insgesamt {:+.2} %, \
         Unterschied {:+.2} pp",
        schulterblatt.len(),
        prozent(e_schulterblatt),
        prozent(e_ruecken),
        prozent(e_schulterblatt - e_ruecken)
    );

    // === Fehler 4: Ärmel der Länge nach zu eng ==============================
    // Ein Band um den Ärmel auf Achselhöhe (tiefster Punkt der Kugelnaht).
    let achsel_z = ["V13", "V14"]
        .iter()
        .flat_map(|kennung| nahtseite(&p.netz, &s, kennung, true))
        .map(|i| s.x[i][2])
        .fold(f64::INFINITY, f64::min);
    let aermel = ["Aermel Kurz Links", "Aermel Kurz Rechts"];
    let aermel_alle = teilflaechen(&s, &p.netz, &aermel);
    let band: Vec<usize> = aermel_alle
        .iter()
        .copied()
        .filter(|&f| (schwerpunkt(&s, f)[2] - achsel_z).abs() < 0.02)
        .collect();
    assert!(band.len() > 40, "Ärmelband: nur {} Flächen", band.len());
    let e_arm = dehnung_median(&s, &je_flaeche, &band);
    let e_arm_ganz = dehnung_median(&s, &je_flaeche, &aermel_alle);
    let e_arm_max = band.iter().map(|&f| je_flaeche[f].0).fold(f64::NEG_INFINITY, f64::max);
    eprintln!(
        "  [4] Ärmel auf Achselhöhe ({:.1} cm über dem Boden): {} Flächen, Dehnung im Median {:+.2} %, \
         größte {:+.2} %; ganzer Ärmel im Median {:+.2} %",
        achsel_z * 100.0,
        band.len(),
        prozent(e_arm),
        prozent(e_arm_max),
        prozent(e_arm_ganz)
    );

    // === Fehler 5: Kragen steht vorn offen ==================================
    // Die freie Oberkante des Kragens (Seite A von V18): Luft zur Haut, im
    // Ganzen und an den beiden vorderen Enden. Dazu der Abstand der Enden.
    let kragen_oben = nahtseite(&p.netz, &s, "V18", true);
    assert!(kragen_oben.len() > 10, "Kragenoberkante: nur {} Punkte", kragen_oben.len());
    let kragen_d: Vec<f64> = kragen_oben.iter().map(|&i| haut(&k, s.x[i])).collect();
    let n_ende = (kragen_oben.len() / 8).max(3);
    let enden: Vec<usize> = kragen_oben[..n_ende]
        .iter()
        .chain(kragen_oben[kragen_oben.len() - n_ende..].iter())
        .copied()
        .collect();
    let kragen_enden = enden.iter().map(|&i| haut(&k, s.x[i])).fold(f64::NEG_INFINITY, f64::max);
    let kragen_med = median(&kragen_d);
    let kul = punkt("Stehkragen Innen (Plunge)", "KUL");
    let kur = punkt("Stehkragen Innen (Plunge)", "KUR");
    eprintln!(
        "  [5] Kragen: Oberkante im Median {:.1} mm von der Haut, an den beiden Enden höchstens {:.1} mm; \
         die Enden stehen {:.1} mm auseinander",
        kragen_med * 1e3,
        kragen_enden * 1e3,
        abstand(s.x[kul], s.x[kur]) * 1e3
    );

    // --- Abnahme -------------------------------------------------------------
    assert!(v_ende < 0.015, "nicht in Ruhe: {:.4} m/s", v_ende);
    assert!(tiefster > -0.1e-3, "Punkt {:.2} mm in der Haut", tiefster * 1e3);
    let sinken = erg.sinken.iter().map(|(n, d)| (n.clone(), *d)).collect::<Vec<_>>();
    for (n, d) in &sinken {
        if *d > 5e-3 {
            panic!("ADR-023: {n} sinkt {:.1} mm in der Haltephase", d * 1e3);
        }
    }
    eprintln!("  Halt   am stärksten sinkt {} um {:+.2} mm (ADR-023: höchstens 5 mm)", sinken[0].0, sinken[0].1 * 1e3);
    assert!(anteil >= 0.98, "nur {:.1} % der Nahtpaare zu", anteil * 100.0);
    for (kennung, med, max) in &je_naht {
        eprintln!("  {kennung:<5} Median {:5.1} mm, größter {:6.1} mm", med * 1e3, max * 1e3);
    }

    // Die fünf Fehler gegen ihre Böden aus dem Testkopf.
    // Fehler 1: die Öffnung an der Brust, nicht die am Hals - die Halsnaht des
    // Musters ist 5.5 cm länger als das Halsloch des Körpers und kann dort
    // ohnehin nicht schließen (gemessen 37.6 gegen 32.1 cm).
    assert!(
        oeffnung_brust - MUSTER_OEFFNUNG_BRUST > PLUNGE_GRENZE,
        "Fehler 1 nicht gezeigt: Plunge-Öffnung an der Brust {:+.1} mm gegen das Muster",
        (oeffnung_brust - MUSTER_OEFFNUNG_BRUST) * 1e3
    );
    // Fehler 2 wird **nicht** gezeigt, und das ist bei diesem Muster richtig so
    // (belegt in `tests/aermelkugel.rs`): `#ArmholeSolved` schneidet das
    // Armloch genau so lang wie die Kugel (-0,00 mm Unterschied), die Kugel ist
    // im Zuschnitt 14,50 statt 14,0 cm hoch, und die Naht paart zwei gleich
    // lange Kanten. Ein echter Überschuss, der eingehalten werden müsste und
    // die Kugel abhebt, ist in v26 nicht vorhanden. Der Test hält den Befund
    // fest, damit ein gehobener Stand auffällt.
    assert!(
        kugel_med < LUFT_GRENZE,
        "Fehler 2 jetzt gezeigt: Ärmelkugel steht im Median {:.1} mm ab - Befund im Testkopf neu lesen",
        kugel_med * 1e3
    );
    assert!(
        e_schulterblatt - e_ruecken > SCHULTERBLATT_GRENZE,
        "Fehler 3 nicht gezeigt: Schulterblatt {:+.2} pp gegen die Rückenteile",
        prozent(e_schulterblatt - e_ruecken)
    );
    assert!(e_arm > AERMEL_GRENZE, "Fehler 4 nicht gezeigt: Ärmel nur {:+.2} % gedehnt", prozent(e_arm));
    assert!(
        kragen_enden > LUFT_GRENZE,
        "Fehler 5 nicht gezeigt: Kragenende nur {:.1} mm von der Haut",
        kragen_enden * 1e3
    );
}
