//! The seam allowance as fabric of its own: the band folded 180 degrees inwards
//! (task #123).
//!
//! Until #119 the allowance was a ring of numbers next to the mesh, and until
//! #121 a strip of triangles that nothing in the scene carried. This test asks
//! what the fold adds to a scene and whether what it adds is cloth:
//!
//! 1. **The band is fabric.** Every band triangle that survives the weld is a
//!    triangle of the scene, on top of the pattern's own; the stretch group grows
//!    by exactly the band's edges plus the holds; the band's area is reported in
//!    mm² and in % of the area of the pieces that carry it.
//! 2. **The fold line lies on the piece.** No band seam-line point is farther
//!    than [`RAND_TOLERANZ_MM`] from the piece's boundary polyline - recounted
//!    here from the mesh, not taken from the module's own report - and every band
//!    run touches the piece, so the folded band is not a loose flap.
//! 3. **The fold line is joined.** Every band seam-line point that carries a
//!    triangle is held on the boundary. A point on a boundary vertex gets a
//!    constraint of rest length 0, one inside an edge two constraints with the
//!    distance it was built with - counted in the scene's own `abstand` group, so
//!    a hold that never reached the solver shows.
//! 4. **The fold stands at 180 degrees.** One bending constraint per fold edge
//!    with rest angle of magnitude 180 degrees, and the dihedral it has *as
//!    built* sits one step on the folded side of the branch cut. Fold edges that
//!    the strip's miters prevent are counted in the report, never dropped
//!    silently.
//! 5. **The seams stay as they are.** The scene gains the band and loses no seam
//!    pair: 770 on `bluse2_v26`, 715 on `bluse3_v2`, the numbers from before the
//!    fold.
//! 6. **Nothing is pinned.** Every point of the scene, the band's included,
//!    carries a mass; the band is held by tension, friction and the fold alone
//!    (ADR-023).
//!
//! Test 9 asks for something the list above assumes: that the turn keeps the
//! band's own springs at their rest length. It does not, unless the fold line is
//! the seam-line edge leaving the point - a line across the point's own offset
//! shears the band where the seam line turns.
//!
//! ## Why the patterns are laid flat
//!
//! No body, no gravity, no reduction: the pattern is the cloth, the same device
//! [`einhalteweite`](einhalteweite.rs) uses. The fold is then an exact turn about
//! the seam line, so its dihedral can be asserted to a hundredth of a radian
//! instead of to "well inside a degree", and the run stays seconds long. The
//! scene is built, not simulated: what a *folded* band does under gravity is a
//! question for the garment runs, not for the fold's construction.
//!
//! Three patterns cover the three cases: `bluse2_v26` and `bluse3_v2` carry seams
//! and allowances, `rock_2schlitze_v34` carries allowances and **no** seam list,
//! so its band is checked without any seam number being involved.
//!
//! Needs the pattern files of `klon3d-pattern/tests/muster`; without them the
//! test says `übersprungen: … fehlt` and returns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use klon3d_mesh::{vernetze, Musternetz, Parameter, Teilnetz};
use klon3d_pattern::geom::Auswertung;
use klon3d_pattern::kanten::Kurvenquelle;
use klon3d_pattern::naehte::{lies_nahtliste, pruefe_naehte};
use klon3d_sim::roehre::param_ruhe;
use klon3d_sim::saumfalte::Faltbericht;
use klon3d_sim::szene::Teilvorgabe;
use klon3d_sim::{Simparameter, Szene, RADICAL_RUBBER_04};

/// How far a band seam-line point may sit from the piece's boundary polyline
/// (mm): the mesh's own sagitta, measured before the fold at 0.0989 / 0.0991 /
/// 0.0733 mm on the three patterns.
const RAND_TOLERANZ_MM: f64 = 0.1;
/// The fold's built dihedral against 180 degrees: the band is lifted across its
/// width by one tenth of a degree, so the angle comes out just beside the branch
/// cut. Anything inside a hundredth of a radian is that lift.
const FALZ_TOLERANZ_RAD: f64 = 0.01;
/// A band point that counts as sitting on a boundary vertex instead of inside an
/// edge (mm), the threshold the module uses.
const AUF_PUNKT_MM: f64 = 1e-6;

fn musterordner() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../klon3d-pattern/tests/muster"
    ))
}

/// The mesh of a pattern, with its seam list when the pattern has one.
fn netz(name: &str) -> Option<Musternetz> {
    let dir = musterordner();
    let pfad = dir.join(format!("{name}.sm2d"));
    let smis = pfad.with_file_name("measurements.smis");
    for p in [&pfad, &smis] {
        if !p.exists() {
            eprintln!("übersprungen: {} fehlt", p.display());
            return None;
        }
    }
    let m = klon3d_pattern::read_path(&pfad).unwrap();
    let ev = Auswertung::lade(&m, &smis).unwrap();
    let nahtdatei = dir.join(format!("{name}.naehte"));
    let naehte = if nahtdatei.exists() {
        let defs = lies_nahtliste(&std::fs::read_to_string(&nahtdatei).unwrap()).unwrap();
        pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly)
            .unwrap()
            .naehte
    } else {
        Vec::new()
    };
    let n = vernetze(&ev, Kurvenquelle::Seamly, &naehte, &Parameter::default()).unwrap();
    assert!(n.fehler.is_empty(), "{name}: {:?}", n.fehler);
    Some(n)
}

fn vorgabe() -> Teilvorgabe {
    Teilvorgabe {
        material: RADICAL_RUBBER_04,
        reduktion: 0.0,
    }
}

fn param() -> Simparameter {
    Simparameter {
        einhalteweite: false,
        ..param_ruhe()
    }
}

/// The scene laid flat, with the band folded in, plus the fold's report.
fn flach(netz: &Musternetz) -> (Szene, Faltbericht) {
    Szene::aus_netz_mit_falte(
        netz,
        &|_| vorgabe(),
        &|_, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0],
        Vec::new(),
        param(),
    )
}

/// The same scene without the band: what the pattern alone is.
fn flach_ohne_falte(netz: &Musternetz) -> Szene {
    Szene::aus_netz(
        netz,
        &|_| vorgabe(),
        &|_, q: [f64; 2]| [q[0] * 1e-3, q[1] * 1e-3, 0.0],
        Vec::new(),
        param(),
    )
}

fn abstand2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn flaeche2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Distance from `p` to the closed polyline `ring`, and the edge it falls on.
fn projiziere(p: [f64; 2], ring: &[[f64; 2]]) -> (f64, usize) {
    let mut best = (f64::INFINITY, 0usize);
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let q = d[0] * d[0] + d[1] * d[1];
        let f = if q <= 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / q).clamp(0.0, 1.0)
        };
        let dist = abstand2(p, [a[0] + f * d[0], a[1] + f * d[1]]);
        if dist < best.0 {
            best = (dist, k);
        }
    }
    best
}

/// Area of a piece's own triangles (mm²).
fn teilflaeche(t: &Teilnetz) -> f64 {
    t.dreiecke
        .iter()
        .map(|d| {
            flaeche2(
                t.punkte[d[0] as usize],
                t.punkte[d[1] as usize],
                t.punkte[d[2] as usize],
            )
            .abs()
                * 0.5
        })
        .sum()
}

/// Area of a band (mm²), the way its triangles read it.
fn bandflaeche(t: &Teilnetz) -> f64 {
    let s = &t.zugabe;
    s.dreiecke
        .iter()
        .map(|d| flaeche2(s.punkt(d[0]), s.punkt(d[1]), s.punkt(d[2])).abs() * 0.5)
        .sum()
}

/// What a band is, topologically, in the form the fold builds it: the inner ring
/// welded to one vertex per seam-line position, collapsed triangles dropped.
struct Bandtopo {
    /// Triangles that survive the weld - the ones the scene gets.
    dreiecke: usize,
    /// Distinct edges of the welded band.
    kanten: usize,
    /// The same edges, minus those of zero length: exactly the ones that get a
    /// stretch spring, because a spring of length 0 is not built.
    kanten_mit_laenge: usize,
    /// Connected components over shared edges.
    komponenten: usize,
    /// Components that touch no seam-line vertex: a flap hanging free.
    komponenten_frei: usize,
    /// Welded seam-line vertices that a triangle names - the points that need a
    /// hold.
    innenpunkte: usize,
    /// Vertices the band names, welded ring included.
    punkte: usize,
}

fn bandtopologie(t: &Teilnetz) -> Bandtopo {
    let s = &t.zugabe;
    let n = s.innen.len();
    let v = s.vertreter();
    let map = |i: u32| -> u32 {
        if (i as usize) < n {
            v[i as usize]
        } else {
            i
        }
    };
    let innen: BTreeSet<u32> = (0..n).map(|i| v[i]).collect();
    let mut kante: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    let mut ecken: Vec<[u32; 3]> = Vec::new();
    let mut punkte: BTreeSet<u32> = BTreeSet::new();
    let mut innen_drin: BTreeSet<u32> = BTreeSet::new();
    for d in &s.dreiecke {
        let m = d.map(map);
        if m[0] == m[1] || m[1] == m[2] || m[0] == m[2] {
            continue;
        }
        let id = ecken.len();
        ecken.push(m);
        for k in 0..3 {
            punkte.insert(m[k]);
            if innen.contains(&m[k]) {
                innen_drin.insert(m[k]);
            }
            let (a, b) = (m[k], m[(k + 1) % 3]);
            kante.entry((a.min(b), a.max(b))).or_default().push(id);
        }
    }
    fn find(eltern: &mut [usize], mut i: usize) -> usize {
        while eltern[i] != i {
            eltern[i] = eltern[eltern[i]];
            i = eltern[i];
        }
        i
    }
    let mut eltern: Vec<usize> = (0..ecken.len()).collect();
    for ids in kante.values() {
        for w in ids.windows(2) {
            let (a, b) = (find(&mut eltern, w[0]), find(&mut eltern, w[1]));
            if a != b {
                eltern[a] = b;
            }
        }
    }
    let mut mit_innen: BTreeSet<usize> = BTreeSet::new();
    for (i, m) in ecken.iter().enumerate() {
        if m.iter().any(|k| innen.contains(k)) {
            mit_innen.insert(find(&mut eltern, i));
        }
    }
    let komponenten: BTreeSet<usize> = (0..ecken.len()).map(|i| find(&mut eltern, i)).collect();
    let komponenten_frei = komponenten.difference(&mit_innen).count();
    // A spring of length 0 is not built (`saumfalte` skips `l <= 0.0`), so the
    // edges that carry one are exactly those whose two ends are different places.
    let kanten_mit_laenge = kante
        .keys()
        .filter(|(a, b)| abstand2(s.punkt(*a), s.punkt(*b)) > 0.0)
        .count();
    Bandtopo {
        dreiecke: ecken.len(),
        kanten: kante.len(),
        kanten_mit_laenge,
        komponenten: komponenten.len(),
        komponenten_frei,
        innenpunkte: innen_drin.len(),
        punkte: punkte.len(),
    }
}

/// Seam point pairs of the whole net.
fn nahtpaare(netz: &Musternetz) -> usize {
    netz.naehte
        .iter()
        .flat_map(|n| &n.teilnaehte)
        .map(|t| t.paare.len())
        .sum()
}

/// The pieces that carry an allowance band.
fn mit_band(netz: &Musternetz) -> Vec<&Teilnetz> {
    netz.teile.iter().filter(|t| !t.zugabe.ist_leer()).collect()
}

/// 1. The band is fabric: every band triangle that survives the weld is a
/// triangle of the scene, on top of the pattern's own; the stretch group grows by
/// exactly the band's edges of non-zero length plus the holds; the band's area is
/// reported in mm² and in % of the area of the pieces that carry it.
#[test]
fn das_band_ist_stoff() {
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let band = mit_band(&netz);
        assert!(!band.is_empty(), "{name}: no piece carries an allowance");
        let (s, b) = flach(&netz);
        let ohne = flach_ohne_falte(&netz);

        assert_eq!(
            b.teile,
            band.len(),
            "{name}: pieces whose band became cloth"
        );
        assert_eq!(
            b.teile + b.ohne_band,
            netz.teile.len(),
            "{name}: every piece is either banded or counted as not"
        );

        // What the test recounts from the strip must be what the scene got:
        // nothing is dropped except the collapsed corners the weld creates, and
        // those are counted.
        let mut dreiecke = 0usize;
        let mut kanten = 0usize;
        let mut kanten_alle = 0usize;
        let mut punkte = 0usize;
        let mut flaeche = 0.0f64;
        for t in &band {
            let topo = bandtopologie(t);
            dreiecke += topo.dreiecke;
            kanten += topo.kanten_mit_laenge;
            kanten_alle += topo.kanten;
            punkte += topo.punkte;
            flaeche += bandflaeche(t);
        }
        assert_eq!(b.dreiecke, dreiecke, "{name}: band triangles in the scene");
        // One scene point per welded band vertex, and no more.
        assert_eq!(b.punkte, punkte, "{name}: band points in the scene");
        assert!(
            kanten <= kanten_alle,
            "{name}: {kanten} springs from {kanten_alle} band edges"
        );
        assert_eq!(
            s.dreiecke.len(),
            ohne.dreiecke.len() + b.dreiecke,
            "{name}: the scene's triangles are the pattern's plus the band's"
        );
        assert_eq!(
            s.abstand.len(),
            ohne.abstand.len() + kanten + b.halte,
            "{name}: the stretch group is the pattern's {}, the band's {kanten} edges \
             and {} holds",
            ohne.abstand.len(),
            b.halte
        );
        assert!(
            (b.flaeche_mm2 - flaeche).abs() < 1e-6,
            "{name}: band area {:.4} mm², recomputed {:.4}",
            b.flaeche_mm2,
            flaeche
        );
        let teile_flaeche: f64 = band.iter().map(|t| teilflaeche(t)).sum();
        assert!(
            teile_flaeche > 0.0,
            "{name}: the banded pieces have no area"
        );
        eprintln!(
            "{name}: {} von {} Teilen mit Band, {} Banddreiecke ({} verworfen), \
             {:.1} mm² = {:.2} % der Teilefläche, {} Punkte, {kanten} Kanten, {} Haltelaschen",
            band.len(),
            netz.teile.len(),
            b.dreiecke,
            b.dreiecke_entartet,
            b.flaeche_mm2,
            100.0 * b.flaeche_mm2 / teile_flaeche,
            b.punkte,
            b.halte
        );
    }
}

/// 2. The fold line lies on the piece: no band seam-line point is farther than
/// [`RAND_TOLERANZ_MM`] from the piece's boundary polyline, recounted here from
/// the mesh; every band run touches the piece, so the folded band is not a loose
/// flap floating beside it.
#[test]
fn die_falzlinie_liegt_auf_dem_teil() {
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let band = mit_band(&netz);
        let (_, b) = flach(&netz);
        let (mut max, mut punkte, mut komponenten, mut frei) = (0.0f64, 0usize, 0usize, 0usize);
        let mut auf_punkt = 0usize;
        let mut auf_kante = 0usize;
        for t in &band {
            let s = &t.zugabe;
            let ring: Vec<[f64; 2]> = t.punkte[..t.nrand.min(t.punkte.len())].to_vec();
            assert!(
                ring.len() >= 3,
                "{name}/{}: {}-point boundary",
                t.teil,
                ring.len()
            );
            let n = s.innen.len();
            let v = s.vertreter();
            // The seam-line vertices the band really names - the ones the module
            // holds. A welded vertex no triangle uses is not on the fold line.
            let drin: BTreeSet<u32> = {
                let mut m = BTreeSet::new();
                for d in &s.dreiecke {
                    for k in d {
                        if (*k as usize) < n {
                            m.insert(v[*k as usize]);
                        }
                    }
                }
                m
            };
            let mut einzeln = BTreeSet::new();
            for i in 0..n {
                let (d, _) = projiziere(s.innen[i], &ring);
                assert!(
                    d <= RAND_TOLERANZ_MM,
                    "{name}/{}: band point {i} sits {d:.4} mm off the boundary",
                    t.teil
                );
                max = max.max(d);
                if !drin.contains(&v[i]) || !einzeln.insert(v[i]) {
                    continue;
                }
                // Recounted, not taken from the report: a point this close to a
                // ring vertex is that vertex, anything else sits inside an edge.
                if ring
                    .iter()
                    .any(|q| abstand2(s.innen[i], *q) <= AUF_PUNKT_MM)
                {
                    auf_punkt += 1;
                } else {
                    auf_kante += 1;
                }
            }
            punkte += einzeln.len();
            let topo = bandtopologie(t);
            assert!(
                topo.komponenten >= 1,
                "{name}/{}: the band has no triangle left",
                t.teil
            );
            assert_eq!(
                topo.innenpunkte,
                einzeln.len(),
                "{name}/{}: welded seam-line vertices the band names",
                t.teil
            );
            komponenten += topo.komponenten;
            frei += topo.komponenten_frei;
        }
        eprintln!(
            "{name}: {punkte} Bandpunkte auf der Nahtlinie, größter Abstand {max:.4} mm, \
             {komponenten} Bandläufe auf {} Teilen, {frei} ohne Nahtlinie, \
             {auf_punkt} auf Ecken / {auf_kante} in Kanten",
            band.len()
        );
        assert!(
            max <= RAND_TOLERANZ_MM,
            "{name}: the worst band point sits {max:.4} mm off the seam line"
        );
        assert!(
            komponenten >= band.len(),
            "{name}: {komponenten} runs for {} pieces with a band",
            band.len()
        );
        assert_eq!(frei, 0, "{name}: {frei} band runs touch no seam line");
        // The same classification the module used for its holds, recounted.
        assert_eq!(
            (auf_punkt, auf_kante),
            (b.halte_punkt, b.punkte_auf_kante),
            "{name}: points on ring vertices and inside ring edges"
        );
    }
}

/// 3. The fold line is joined: every band seam-line point that a triangle names
/// is held on the piece's boundary. A point on a boundary vertex gets a
/// constraint of rest length 0, one inside an edge two constraints with the
/// distance it was built with - counted in the scene's own `abstand` group, so a
/// hold that never reached the solver shows.
#[test]
fn die_falzlinie_ist_verbunden() {
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let (s, b) = flach(&netz);
        let ohne = flach_ohne_falte(&netz);

        assert_eq!(
            b.halte,
            b.halte_punkt + b.halte_kante,
            "{name}: the holds are vertex holds plus edge holds"
        );
        assert_eq!(
            b.halte_kante,
            2 * b.punkte_auf_kante,
            "{name}: every point inside an edge takes two holds"
        );
        assert!(
            b.halte_abstand_mm <= RAND_TOLERANZ_MM,
            "{name}: a held point started {:.4} mm off the boundary",
            b.halte_abstand_mm
        );

        // Every band vertex a triangle names is held: none slides freely along
        // the seam.
        let erwartet: usize = mit_band(&netz)
            .iter()
            .map(|t| bandtopologie(t).innenpunkte)
            .sum();
        assert_eq!(
            b.halte_punkt + b.punkte_auf_kante,
            erwartet,
            "{name}: held band vertices against the band vertices in triangles"
        );

        // Rest length 0 is a hold and nothing else: the pattern's own stretch
        // group has none, since a spring of length 0 is not built.
        let null = |sc: &Szene| sc.abstand.ruhe.iter().filter(|&&r| r == 0.0).count();
        assert_eq!(
            null(&ohne),
            0,
            "{name}: the pattern alone holds nothing at rest 0"
        );
        assert_eq!(
            null(&s),
            b.halte_punkt,
            "{name}: rest-0 holds in the scene against vertex holds"
        );
        assert!(
            s.abstand
                .nachgiebigkeit
                .iter()
                .all(|a| a.is_finite() && *a >= 0.0),
            "{name}: a hold without a finite compliance"
        );
        assert!(
            b.halte_ohne_kante <= s.abstand.len(),
            "{name}: more ring edges without a band triangle than constraints"
        );
        eprintln!(
            "{name}: {} Haltelaschen ({} auf Punkten, {} auf Kanten bei {} Punkten), \
             größter Anfangsabstand {:.4} mm, {} Randkanten ohne Banddreieck",
            b.halte,
            b.halte_punkt,
            b.halte_kante,
            b.punkte_auf_kante,
            b.halte_abstand_mm,
            b.halte_ohne_kante
        );
    }
}

/// 4. The fold stands at 180 degrees: one bending constraint per fold edge with a
/// rest angle of magnitude 180 degrees, and the dihedral it has *as built* sits
/// one step beside the branch cut - a rotation by one tenth of a degree short of
/// half a turn about the fold axis, the same angle across the whole band because
/// the band is turned, not lifted. Fold edges that the strip's miters prevent are
/// counted, not dropped silently.
#[test]
fn der_falz_steht_bei_180_grad() {
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let (s, b) = flach(&netz);
        let ohne = flach_ohne_falte(&netz);
        let pi = std::f64::consts::PI;

        assert!(b.falzkanten > 0, "{name}: no fold edge got a constraint");
        assert_eq!(
            b.falz_falsche_seite, 0,
            "{name}: {} fold edges were built on the wrong side of the piece",
            b.falz_falsche_seite
        );
        assert_eq!(
            b.falz_dreieck_fehlt, 0,
            "{name}: {} fold edges have no dihedral",
            b.falz_dreieck_fehlt
        );
        assert_eq!(
            b.falz_ohne_richtung, 0,
            "{name}: {} cutting-line points had no fold line to turn about",
            b.falz_ohne_richtung
        );

        // A fold constraint is one whose rest angle is ±180 degrees; the
        // fabric's own bending rests at 0.
        let falz = |sc: &Szene| {
            sc.biegung
                .ruhe
                .iter()
                .filter(|r| r.abs() > 0.5 * pi)
                .count()
        };
        assert_eq!(falz(&ohne), 0, "{name}: the pattern alone folds nothing");
        assert_eq!(
            falz(&s),
            b.falzkanten,
            "{name}: fold constraints in the scene against built fold edges"
        );
        assert_eq!(
            s.biegung.len(),
            ohne.biegung.len() + b.falzkanten,
            "{name}: the bending group is the pattern's plus the fold line"
        );
        // No fold constraint may take a degree of freedom away (ADR-023): a
        // compliance of 0 would be a hard hinge.
        for (k, r) in s.biegung.ruhe.iter().enumerate() {
            if r.abs() > 0.5 * pi {
                assert!(
                    s.biegung.nachgiebigkeit[k] > 0.0,
                    "{name}: fold constraint {k} is rigid"
                );
            }
        }

        // As built: at 180 degrees to a hundredth of a radian.
        for (was, winkel) in [
            ("kleinste", b.falz_winkel_min_rad),
            ("größte", b.falz_winkel_max_rad),
        ] {
            assert!(
                (winkel.abs() - pi).abs() <= FALZ_TOLERANZ_RAD,
                "{name}: {was} Faltwinkel {winkel:.6} rad, erwartet ±π"
            );
        }
        eprintln!(
            "{name}: {} Falzkanten, {} von einer Gehrung verhindert, {} ohne Banddreieck, \
             {} ohne Randkante, Winkel {:.4}..{:.4} Grad",
            b.falzkanten,
            b.falzkanten_gehrung,
            b.falzkanten_ohne_band,
            b.falzkanten_keine_randkante,
            b.falz_winkel_min_rad.to_degrees(),
            b.falz_winkel_max_rad.to_degrees()
        );
    }
}

/// 5. The seams stay as they are: the band is appended to the scene and the seam
/// pair count is the mesh's, unchanged - 770 on `bluse2_v26`, 715 on `bluse3_v2`,
/// the numbers from before the fold.
#[test]
fn die_naehte_bleiben_wie_sie_sind() {
    for (name, paare) in [("bluse2_v26", 770usize), ("bluse3_v2", 715)] {
        let Some(netz) = netz(name) else { return };
        assert_eq!(nahtpaare(&netz), paare, "{name}: seam pairs in the net");
        let (s, _) = flach(&netz);
        let ohne = flach_ohne_falte(&netz);
        assert_eq!(s.naht.len(), paare, "{name}: seam constraints in the scene");
        assert_eq!(
            s.naht.len(),
            ohne.naht.len(),
            "{name}: the fold changes no seam"
        );
        assert!(
            s.naht
                .punkte
                .iter()
                .flatten()
                .all(|i| (*i as usize) < s.x.len()),
            "{name}: a seam names a point the scene does not have"
        );
    }
}

/// 6. Nothing is pinned: every point of the scene, the band's included, carries a
/// mass, so the band hangs on tension, friction and the fold alone (ADR-023).
#[test]
fn nichts_ist_festgehalten() {
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let (s, b) = flach(&netz);
        assert!(
            s.masse.iter().all(|m| *m > 0.0),
            "{name}: the scene has a point without mass"
        );
        assert!(
            s.teile.iter().any(|t| t.name.ends_with(" (Zugabe)")),
            "{name}: no band piece in the scene"
        );
        let mut bandmasse = 0.0f64;
        for t in s.teile.iter().filter(|t| t.name.ends_with(" (Zugabe)")) {
            assert!(
                t.dreiecke.start < t.dreiecke.end,
                "{name}/{}: band piece without a triangle",
                t.name
            );
            bandmasse += s.masse[t.punkte.clone()].iter().sum::<f64>();
        }
        assert!(bandmasse > 0.0, "{name}: the band weighs nothing");
        assert_eq!(
            s.teile.len(),
            netz.teile.len() + b.teile,
            "{name}: one piece per pattern piece plus one per band"
        );
        eprintln!(
            "{name}: Bandmasse {:.4} g bei {:.1} g gesamt",
            bandmasse * 1e3,
            s.masse.iter().sum::<f64>() * 1e3
        );
    }
}

/// 7. The band's stretch stiffness stays inside the range the solver resolves.
///
/// The strip carries sliver triangles that span the whole band width at a length
/// of a tenth of a millimetre; uncapped, `dehnung_n_m · A / l²` reaches 7.8 · 10⁵
/// N/m there against the pattern piece's 574 N/m, and the flat scene runs from
/// 4 · 10⁻⁵ J to 10⁷³ J in 180 frames. The cap holds every band stretch edge at
/// `dehnung_n_m · HOECHSTE_FORM` at most, the stiffness of the piece's own
/// stiffest edge - no band edge is stiffer than the fabric it is part of. Every
/// capped edge is counted, and the cap changes no length, no point and no
/// triangle, so the rest of the test file's numbers stay what they were.
#[test]
fn die_bandkanten_sind_nicht_zu_steif() {
    let e = RADICAL_RUBBER_04.dehnung_n_m;
    let grenze = e * klon3d_sim::saumfalte::HOECHSTE_FORM;
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let (s, b) = flach(&netz);
        let bandstart = netz.teile.iter().map(|t| t.punkte.len()).sum::<usize>();
        let ist_band = |i: u32| (i as usize) >= bandstart;

        let mut bandkanten = 0usize;
        let mut gedeckelt = 0usize;
        let mut steifste = 0.0f64;
        for (c, p) in s.abstand.punkte.iter().enumerate() {
            if !(ist_band(p[0]) && ist_band(p[1])) {
                continue;
            }
            bandkanten += 1;
            let k = 1.0 / s.abstand.nachgiebigkeit[c];
            assert!(
                k.is_finite() && k >= 0.0,
                "{name}: band edge {c} has no stiffness"
            );
            assert!(
                k <= grenze * (1.0 + 1e-9),
                "{name}: band edge {c} is {k:.6e} N/m, above the cap {grenze:.6e}"
            );
            // The cap is applied only where it bites: an edge at the cap has the
            // shape measure it was capped to.
            if (k - grenze).abs() <= grenze * 1e-9 {
                gedeckelt += 1;
            }
            steifste = steifste.max(k);
        }
        assert!(bandkanten > 0, "{name}: the band carries no stretch edge");
        assert_eq!(
            gedeckelt, b.kanten_form_gedeckelt,
            "{name}: band edges at the cap against the report"
        );
        assert!(
            gedeckelt > 0,
            "{name}: no band edge needed the cap - the needle triangles are gone"
        );
        eprintln!(
            "{name}: {bandkanten} Bandkanten, {gedeckelt} gedeckelt, steifste {steifste:.1} N/m \
             (Grenze {grenze:.1} N/m, Stueck hoechste {:.1} N/m)",
            piece_max(&s, bandstart)
        );
    }
}

/// Largest stretch stiffness of the pattern pieces, for the comparison above.
fn piece_max(s: &Szene, bandstart: usize) -> f64 {
    let ist_band = |i: u32| (i as usize) >= bandstart;
    s.abstand
        .punkte
        .iter()
        .enumerate()
        .filter(|(_, p)| !ist_band(p[0]) && !ist_band(p[1]))
        .map(|(c, _)| 1.0 / s.abstand.nachgiebigkeit[c])
        .fold(0.0f64, f64::max)
}

/// 8. The fold adds the band and changes nothing the pattern built.
///
/// `aus_netz` and `aus_netz_mit_falte` are the same builder with one switch, and
/// the switch may only append: every pattern point, triangle, seam pair, rest
/// matrix, mass, collision distance and piece entry must be the same value, bit
/// for bit, or a seam, a measurement or a glue zone would silently shift.
#[test]
fn die_falte_haengt_nur_an() {
    for name in ["bluse2_v26", "bluse3_v2", "rock_2schlitze_v34"] {
        let Some(netz) = netz(name) else { return };
        let (s, b) = flach(&netz);
        let ohne = flach_ohne_falte(&netz);

        let np = ohne.x.len();
        assert_eq!(
            s.x[..np]
                .iter()
                .map(|p| p.map(f64::to_bits))
                .collect::<Vec<_>>(),
            ohne.x
                .iter()
                .map(|p| p.map(f64::to_bits))
                .collect::<Vec<_>>(),
            "{name}: a pattern point moved"
        );
        assert_eq!(
            &s.dreiecke[..ohne.dreiecke.len()],
            &ohne.dreiecke[..],
            "{name}: a pattern triangle changed"
        );
        assert_eq!(
            s.dreiecke.len(),
            ohne.dreiecke.len() + b.dreiecke,
            "{name}: the scene is not the pattern's triangles plus the band's"
        );
        assert_eq!(s.naht.len(), ohne.naht.len(), "{name}: seam count");
        assert_eq!(s.naht.punkte, ohne.naht.punkte, "{name}: a seam pair moved");
        assert_eq!(
            &s.ruhe_inv[..np],
            &ohne.ruhe_inv[..np],
            "{name}: a pattern rest matrix changed"
        );
        assert_eq!(
            &s.masse[..np],
            &ohne.masse[..np],
            "{name}: a pattern mass changed"
        );
        assert_eq!(
            &s.kollisionsabstand[..np],
            &ohne.kollisionsabstand[..np],
            "{name}: a pattern collision distance changed"
        );
        assert!(
            s.teile[..ohne.teile.len()]
                .iter()
                .zip(ohne.teile.iter())
                .all(|(a, c)| a.name == c.name && a.punkte == c.punkte && a.dreiecke == c.dreiecke),
            "{name}: a pattern Teilinfo moved"
        );
        assert!(
            s.teile[ohne.teile.len()..]
                .iter()
                .all(|t| t.name.ends_with(" (Zugabe)")),
            "{name}: an appended Teilinfo is not a band"
        );
        eprintln!(
            "{name}: {} Teile-Punkte, {} Dreiecke, {} Nahtpaare bitgleich, \
             +{} Bandpunkte, +{} Banddreiecke",
            np,
            ohne.dreiecke.len(),
            ohne.naht.len(),
            b.punkte,
            b.dreiecke
        );
    }
}

/// 9. The fold does not shear the band.
///
/// The band is turned about the seam line, so every stretch spring with both ends
/// band points must still measure its rest length afterwards. Where the seam line
/// turns, that only holds if the fold line is the seam-line edge leaving the
/// point: a line across the point's own offset also runs along the seam there,
/// and the turn flips that component instead of carrying it along, so the springs
/// stretching across the corner come out short. Measured with the offset line on
/// a 20 × 20 cm rectangle: 60 of 326 band springs compressed by up to 43 %, and the
/// corner point sank 3,16 mm through the piece; with the seam-line edge the
/// deepest point without self-collision is 0,37 mm (`tests/falte_lage.rs`).
///
/// The patterns are laid flat and nothing is solved, so this is the built geometry
/// and not a relaxation result. A spring that is off counts with the share of its
/// rest length it misses by; the allowance tolerates 1e-4 on the seam-bearing
/// blouses and the bustier, 2 % on the slit skirt, whose corners the strip carries
/// as a single point.
#[test]
fn die_faltung_schert_das_band_nicht() {
    for (name, toleranz) in [
        ("bluse2_v26", 1e-4),
        ("bluse3_v2", 1e-4),
        ("bustier_v1", 1e-4),
        ("rock_2schlitze_v34", 2e-2),
    ] {
        let Some(netz) = netz(name) else { return };
        let (s, b) = flach(&netz);
        let bandstart = netz.teile.iter().map(|t| t.punkte.len()).sum::<usize>();
        let ist_band = |i: u32| (i as usize) >= bandstart;

        let mut federn = 0usize;
        let mut schlecht: Vec<(f64, u32, u32)> = Vec::new();
        for (c, p) in s.abstand.punkte.iter().enumerate() {
            // Only the band's own springs: a hold to the piece has rest length 0
            // and is not a length the turn could shear.
            if !(ist_band(p[0]) && ist_band(p[1])) || s.abstand.ruhe[c] <= 0.0 {
                continue;
            }
            federn += 1;
            let (i, j) = (p[0] as usize, p[1] as usize);
            let l = ((s.x[i][0] - s.x[j][0]).powi(2)
                + (s.x[i][1] - s.x[j][1]).powi(2)
                + (s.x[i][2] - s.x[j][2]).powi(2))
            .sqrt();
            let d = (l - s.abstand.ruhe[c]) / s.abstand.ruhe[c];
            if d.abs() > toleranz {
                schlecht.push((d, p[0], p[1]));
            }
        }
        assert!(federn > 0, "{name}: das Band trägt keine Dehnfeder");
        schlecht.sort_by(|a, b| a.0.abs().partial_cmp(&b.0.abs()).unwrap());
        eprintln!(
            "{name}: {federn} Bandfedern, {} über {:.0e} Abweichung{}",
            schlecht.len(),
            toleranz,
            match schlecht.last() {
                Some(z) => format!(" (schlimmste {:+.3} %)", z.0 * 100.0),
                None => String::new(),
            }
        );
        assert!(
            schlecht.is_empty(),
            "{name}: {} von {federn} Bandfedern über {:.0e} Abweichung, schlimmste {:+.3} % \
             (b{}–b{}) - die Faltung schert das Band",
            schlecht.len(),
            toleranz,
            schlecht.last().map_or(0.0, |z| z.0 * 100.0),
            schlecht.last().map_or(0, |z| z.1 as usize - bandstart),
            schlecht.last().map_or(0, |z| z.2 as usize - bandstart)
        );
        assert!(
            b.falzkanten_gehrung > 0 || b.falzkanten > 0,
            "{name}: keine Falzkante - das Muster trägt keine Faltung"
        );
    }
}
