//! Die Szene: Punkte, Constraints und Körper als Felder (Struct of Arrays).
//!
//! So liegen die Daten auch in den Puffern der GPU-Fassung (ADR-018,
//! Schritt 4b): je Größe ein Feld, Constraints nach Farbe geordnet
//! ([`crate::faerbung`]).
//!
//! ## Vom Schnittnetz zu den Constraints
//!
//! * **Masse:** Fläche des Zuschnitts × Flächengewicht, je Dreieck zu
//!   gleichen Teilen auf seine drei Punkte. Der Zuschnitt ist das um die
//!   Reduktion verkleinerte Teil, also 2D-Fläche × (1 − r)²: Latex wird
//!   kleiner zugeschnitten, das Flächengewicht gilt je ungedehnter Folie.
//!   (Steckt die Reduktion schon im Muster, wie bei der Bluse3, ist r = 0.)
//! * **Dehnung:** eine Abstandsfeder je Netzkante, Ruhelänge = 2D-Länge ×
//!   (1 − Reduktion). Ein Dreiecksnetz aus Federn ist eine Membran mit
//!   ν = 1/3; im regelmäßigen Netz gilt E_2D = 2k/√3. Für beliebige Netze
//!   verteilt die Steifigkeit über die Fläche:
//!   `k = K · (Summe der anliegenden Dreiecksflächen) / L²` — im
//!   gleichseitigen Netz genau `k = √3/2 · K`. Nachgiebigkeit α = 1/k.
//! * **Biegung:** ein Winkel-Constraint je innerer Kante (vorzeichenbehafteter
//!   Diederwinkel, Ruhewinkel 0 — der Schnitt ist eben). Steifigkeit aus der
//!   Plattensteifigkeit D: `k = D · L² / (A₁ + A₂)`. Für ein gleichseitiges
//!   Netz auf einem Zylinder ist der Diederwinkel θ = L·(3κ⊥ − κ∥)/(2√3),
//!   über die drei Kantenrichtungen gemittelt θ² = κ²L²/4; mit diesem k gibt
//!   das genau die Biegeenergie D/2·κ² je Fläche (Test
//!   `biegeenergie_auf_dem_zylinder`). Auf einer Kugel wäre es ¾ davon.
//! * **Nähte:** ein Abstand-Constraint je Punktpaar aus `klon3d-mesh`
//!   (Befund 33), Ruhelänge 0 (bündig), Nachgiebigkeit wählbar (Vorgabe 0).
//!   Dazu ein Biege-Constraint über jede genähte Randkante (Randdreieck auf
//!   beiden Seiten), damit die Naht kein Gelenk ist; Ruhewinkel 0 = beide Teile
//!   liegen flach aneinander (CLO: Sewing Line Type 180°). Liegen die Teile
//!   gespiegelt zueinander, fehlt ein flacher Ruhewinkel — dann bleibt die
//!   Kante ohne Biegung (`naht_biegung_ohne`).
//!
//! ## Ease along a seam
//!
//! Two sides of a seam are rarely the same length in woven cloth: the sleeve
//! cap is longer than the armhole and has to be eased in. The mesh pairs the
//! points at the same arc fraction on both sides, so the surplus is already
//! spread evenly along the seam — but the seam springs all have rest length 0,
//! and with equal stiffness on both sides the solver then splits the surplus
//! **half and half**: the shorter side stretches, the longer one compresses
//! (measured on 112 mm over a 100 mm chord: 106.40 mm and 106.39 mm, i.e. the
//! shorter side at +6.40 %).
//!
//! [`Simparameter::einhalteweite`] turns that into easing: the boundary springs
//! of the longer side get the rest length `l · l_short/l_long`, the shorter side
//! keeps its own. Now the shorter side stays at its pattern length and the
//! longer one has to accommodate the surplus, which it does by buckling out of
//! plane. Off by default — latex is cut to match, and a scene that does not ask
//! for it computes exactly as before.
//!
//! The rest lengths are where the three backends agree already (CPU reference,
//! CG solve, compute shader), so the ease needs no field of its own in the
//! system. [`Szene::einhalte_kanten`] counts the edges it touched.
//!
//! ## The allowance band as fabric
//!
//! A piece with an allowance carries its band as triangles
//! ([`klon3d_mesh::zugabe`]). [`Szene::aus_netz_mit_falte`] asks
//! [`crate::saumfalte`] to fold that band 180° inward at the seam line and add it
//! to the scene as cloth of its own: appended after the pattern pieces, with the
//! same material, its own mass and stretch springs, and a 180° bending constraint
//! along the fold. [`Szene::aus_netz`] does not, so every existing call site
//! computes exactly as before. The seam pairs, the points and the triangles of
//! the pattern net are the same either way; the seam line stays the mesh
//! boundary and the band is a flap on its reverse.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use klon3d_mesh::Musternetz;

use crate::faerbung::{faerbe, ordnung};
use crate::koerper::Koerper;
use crate::material::Material;

/// Faktor der Weichheitsbedingung ([`Szene::unterschritte_weich`]): 90 % der
/// Federkraft je Unterschritt.
pub const WEICH_FAKTOR: f64 = 9.0;

/// Was ein Teil an Stoff und Reduktion bekommt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Teilvorgabe {
    pub material: Material,
    /// Reduktion als Anteil (0,055 = 5,5 %). Isotrop; CLO führt sie je Punkt
    /// als float2 Kette/Schuss (Befund 59).
    pub reduktion: f64,
}

/// Zeitschritt, Kräfte, Dämpfung.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Simparameter {
    /// Länge eines Bildes (s).
    pub bild_dt: f64,
    /// Unterschritte je Bild (XPBD „Small Steps“: je Unterschritt eine
    /// Iteration).
    pub unterschritte: usize,
    /// Schwerkraft (m/s²).
    pub schwerkraft: [f64; 3],
    /// Geschwindigkeitsdämpfung (1/s): v ← v · (1 − d·h).
    pub daempfung: f64,
    /// Nachgiebigkeit der Naht-Constraints (m/N); 0 = starr.
    pub naht_nachgiebigkeit: f64,
    /// Ease a seam whose sides differ in length into the longer side
    /// (see the module docs, "Ease along a seam"). Off by default: latex is cut
    /// to match, and a scene that does not ask for it must compute exactly as
    /// before.
    pub einhalteweite: bool,
}

impl Default for Simparameter {
    fn default() -> Self {
        Simparameter {
            bild_dt: 1.0 / 60.0,
            unterschritte: 20,
            schwerkraft: [0.0, 0.0, -9.81],
            daempfung: 0.0,
            naht_nachgiebigkeit: 0.0,
            einhalteweite: false,
        }
    }
}

/// Constraints einer Art mit `N` Punkten, nach Farbe geordnet.
#[derive(Debug, Clone, Default)]
pub struct Gruppe<const N: usize> {
    pub punkte: Vec<[u32; N]>,
    /// Ruhewert: Länge (m) bzw. Winkel (rad).
    pub ruhe: Vec<f64>,
    /// Nachgiebigkeit α (Kehrwert der Steifigkeit).
    pub nachgiebigkeit: Vec<f64>,
    /// Bereiche je Farbe in den Feldern oben.
    pub farben: Vec<Range<usize>>,
}

impl<const N: usize> Gruppe<N> {
    fn geordnet(punkte: Vec<[u32; N]>, ruhe: Vec<f64>, alpha: Vec<f64>, npunkte: usize) -> Self {
        let (farbe, zahl) = faerbe(punkte.len(), npunkte, |c| &punkte[c][..]);
        let (idx, farben) = ordnung(&farbe, zahl);
        Gruppe {
            punkte: idx.iter().map(|&c| punkte[c]).collect(),
            ruhe: idx.iter().map(|&c| ruhe[c]).collect(),
            nachgiebigkeit: idx.iter().map(|&c| alpha[c]).collect(),
            farben,
        }
    }

    pub fn len(&self) -> usize {
        self.punkte.len()
    }

    pub fn is_empty(&self) -> bool {
        self.punkte.is_empty()
    }
}

/// Ein Schnittteil in der Szene.
#[derive(Debug, Clone)]
pub struct Teilinfo {
    pub name: String,
    pub punkte: Range<usize>,
    pub dreiecke: Range<usize>,
    pub vorgabe: Teilvorgabe,
}

/// Die ganze Szene.
#[derive(Debug, Clone)]
pub struct Szene {
    /// Positionen (m).
    pub x: Vec<[f64; 3]>,
    /// Geschwindigkeiten (m/s).
    pub v: Vec<[f64; 3]>,
    /// Masse (kg) und inverse Masse; 0 = festgehalten.
    pub masse: Vec<f64>,
    pub w: Vec<f64>,
    /// Kollisionsabstand (halbe Dicke, m) und Reibungszahl je Punkt.
    pub kollisionsabstand: Vec<f64>,
    pub reibung: Vec<f64>,
    pub abstand: Gruppe<2>,
    pub naht: Gruppe<2>,
    pub biegung: Gruppe<4>,
    /// Compliance of the bending constraints as built, before any glue zone
    /// divided it by [`crate::klebung::Zone::biegefaktor`]. Set by
    /// [`Self::klebungen_setzen`] so that changing zones cannot compound.
    pub biegung_grund: Vec<f64>,
    /// Dreiecke (globale Punktindizes).
    pub dreiecke: Vec<[u32; 3]>,
    /// Je Dreieck die Inverse der 2×2-Ruhematrix [p1−p0, p2−p0] (reduziert,
    /// m), zeilenweise, für die Dehnung.
    pub ruhe_inv: Vec<[f64; 4]>,
    /// Material direction per triangle: a unit vector in the triangle's own
    /// plane, derived from the piece's grainline through the placement
    /// (`lage`). Same length as [`Szene::dreiecke`]. Only data for now - the
    /// material is isotropic, so nothing reads it yet (stage 4).
    pub faser: Vec<[f64; 3]>,
    /// Pieces that had to be given the substitute axis (pattern x) because the
    /// pattern has no readable grainline for them.
    pub faser_ersatz: usize,
    /// Triangles whose 3D direction could not be derived (the placement
    /// collapses the triangle). They take their longest edge instead, and the
    /// count is reported so the substitute is never silent.
    pub faser_entartet: usize,
    pub teile: Vec<Teilinfo>,
    /// Glue zones: bonded areas that behave like one plate of the total
    /// thickness ([`crate::klebung`]). Empty by default, so a scene without
    /// zones computes exactly as before.
    pub klebungen: Vec<crate::klebung::Zone>,
    /// Genähte Randkanten ohne Biege-Constraint (Teile gespiegelt).
    pub naht_biegung_ohne: usize,
    /// Boundary edges whose rest length the ease shortened because they lie on
    /// the longer side of an unequal seam. 0 unless
    /// [`Simparameter::einhalteweite`] is on. Reported so the ease is never
    /// silent: an ease that reached no edge did not happen.
    pub einhalte_kanten: usize,
    pub koerper: Vec<Koerper>,
    pub param: Simparameter,
    /// Simulierte Zeit (s).
    pub zeit: f64,
}

/// Abstand zweier 2D-Punkte.
fn l2(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Fläche eines 2D-Dreiecks (positiv bei positivem Umlauf).
fn flaeche(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
}

impl Szene {
    /// Baut die Szene aus einem Schnittnetz. `vorgabe(teilname)` liefert Stoff
    /// und Reduktion, `lage(teilindex, punkt_mm)` die Anfangslage in m.
    pub fn aus_netz(
        netz: &Musternetz,
        vorgabe: &dyn Fn(&str) -> Teilvorgabe,
        lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3],
        koerper: Vec<Koerper>,
        param: Simparameter,
    ) -> Szene {
        Self::bauen(netz, vorgabe, lage, koerper, param, false).0
    }

    /// The same scene, with every piece's allowance band folded 180° inwards and
    /// carried as fabric of its own.
    ///
    /// The band is appended after the pattern pieces as extra [`Teilinfo`]s named
    /// `<piece> (Zugabe)`; see [`crate::saumfalte`] for what it adds and what it
    /// costs. The seam point pairs, [`klon3d_mesh::Teilnetz::punkte`] and
    /// [`klon3d_mesh::Teilnetz::dreiecke`] are untouched: the seam line stays the
    /// mesh boundary and only gains a flap on its reverse.
    ///
    /// The [`crate::saumfalte::Faltbericht`] is returned rather than stored,
    /// because it describes one build: which fold edges the strip's miters
    /// prevented, how far the band's seam line sat from the boundary before the
    /// holds took over, how much fabric the band is, and how many of its edges
    /// needed the stiffness cap ([`crate::saumfalte::HOECHSTE_FORM`]).
    pub fn aus_netz_mit_falte(
        netz: &Musternetz,
        vorgabe: &dyn Fn(&str) -> Teilvorgabe,
        lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3],
        koerper: Vec<Koerper>,
        param: Simparameter,
    ) -> (Szene, crate::saumfalte::Faltbericht) {
        Self::bauen(netz, vorgabe, lage, koerper, param, true)
    }

    /// Builds the scene; `falte` appends the folded allowance band (see
    /// [`Self::aus_netz_mit_falte`]).
    fn bauen(
        netz: &Musternetz,
        vorgabe: &dyn Fn(&str) -> Teilvorgabe,
        lage: &dyn Fn(usize, [f64; 2]) -> [f64; 3],
        koerper: Vec<Koerper>,
        param: Simparameter,
        falte: bool,
    ) -> (Szene, crate::saumfalte::Faltbericht) {
        let mut x = Vec::new();
        let mut masse = Vec::new();
        let mut kollisionsabstand = Vec::new();
        let mut reibung = Vec::new();
        let mut dreiecke = Vec::new();
        let mut ruhe_inv = Vec::new();
        let mut faser = Vec::new();
        let mut faser_ersatz = 0usize;
        let mut faser_entartet = 0usize;
        let mut teile = Vec::new();
        let (mut a_p, mut a_r, mut a_a) = (Vec::new(), Vec::new(), Vec::new());
        let (mut b_p, mut b_r, mut b_a) = (Vec::new(), Vec::new(), Vec::new());
        let mut versatz = Vec::new();
        // Je Teil: Randkante (lokal, sortiert) → (Gegenpunkt, Dreiecksfläche);
        // 2D-Punkte in m; Plattensteifigkeit.
        let mut randkanten: Vec<BTreeMap<(u32, u32), (u32, f64)>> = Vec::new();
        // Per piece: boundary edge (local, sorted) → index in `a_r`, so the
        // ease below can shorten the right springs.
        let mut randkanten_index: Vec<BTreeMap<(u32, u32), usize>> = Vec::new();
        let mut punkte_m: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut biegesteif: Vec<f64> = Vec::new();

        for (ti, t) in netz.teile.iter().enumerate() {
            let vg = vorgabe(&t.teil);
            let skala = 1.0 - vg.reduktion;
            let off = x.len();
            versatz.push(off as u32);
            let p_m: Vec<[f64; 2]> = t.punkte.iter().map(|p| [p[0] * 1e-3, p[1] * 1e-3]).collect();
            // Grainline of the piece in pattern coordinates; a piece without
            // one takes the substitute axis and is counted as such.
            let (dir2, ersatz) = crate::faser::richtung(t.fadenlauf);
            if ersatz {
                faser_ersatz += 1;
            }
            for (k, p) in t.punkte.iter().enumerate() {
                let _ = k;
                x.push(lage(ti, *p));
                masse.push(0.0);
                kollisionsabstand.push(vg.material.dicke_m / 2.0);
                reibung.push(vg.material.reibung);
            }
            // Kanten mit anliegender Fläche, innere Kanten mit Gegenpunkten.
            let mut kanten: BTreeMap<(u32, u32), (f64, Vec<u32>)> = BTreeMap::new();
            let mut rand_index: BTreeMap<(u32, u32), usize> = BTreeMap::new();
            let d0 = dreiecke.len();
            for d in &t.dreiecke {
                let (pa, pb, pc) = (p_m[d[0] as usize], p_m[d[1] as usize], p_m[d[2] as usize]);
                let a = flaeche(pa, pb, pc).abs();
                for &i in d {
                    masse[off + i as usize] += a * skala * skala * vg.material.flaechengewicht_kg_m2 / 3.0;
                }
                for k in 0..3 {
                    let (i, j, o) = (d[k], d[(k + 1) % 3], d[(k + 2) % 3]);
                    let e = kanten.entry((i.min(j), i.max(j))).or_insert((0.0, Vec::new()));
                    e.0 += a;
                    e.1.push(o);
                }
                // Ruhematrix (reduziert) und ihre Inverse.
                let s = |q: [f64; 2]| [q[0] * skala, q[1] * skala];
                let (qa, qb, qc) = (s(pa), s(pb), s(pc));
                let (m00, m10, m01, m11) = (qb[0] - qa[0], qb[1] - qa[1], qc[0] - qa[0], qc[1] - qa[1]);
                let det = m00 * m11 - m01 * m10;
                ruhe_inv.push([m11 / det, -m01 / det, -m10 / det, m00 / det]);
                dreiecke.push([d[0] + off as u32, d[1] + off as u32, d[2] + off as u32]);
                // The grainline through this triangle's plane. A collapsed
                // triangle has no plane: it takes its longest edge and is
                // counted, so the substitute never passes as a measurement.
                let p3 = [x[off + d[0] as usize], x[off + d[1] as usize], x[off + d[2] as usize]];
                match crate::faser::im_dreieck(dir2, ruhe_inv[ruhe_inv.len() - 1], p3) {
                    Some(v) => faser.push(v),
                    None => {
                        // No plane to put a direction in: the longest edge, or -
                        // when all three points coincide - a zero vector that
                        // only keeps the field aligned with `dreiecke`.
                        faser_entartet += 1;
                        faser.push(crate::faser::kante(p3).unwrap_or([0.0; 3]));
                    }
                }
            }
            for (&(i, j), (asum, gegen)) in &kanten {
                let l = l2(p_m[i as usize], p_m[j as usize]);
                if l <= 0.0 {
                    continue;
                }
                let k = vg.material.dehnung_n_m * asum / (l * l);
                a_p.push([i + off as u32, j + off as u32]);
                a_r.push(l * skala);
                a_a.push(1.0 / k);
                if gegen.len() == 1 {
                    rand_index.insert((i, j), a_r.len() - 1);
                }
                if let [o1, o2] = gegen[..] {
                    // Orientierung: Dreieck 1 = (i, j, o) mit positivem Umlauf.
                    let (pi, pj) = (p_m[i as usize], p_m[j as usize]);
                    let (o_a, o_b) = if flaeche(pi, pj, p_m[o1 as usize]) > 0.0 { (o1, o2) } else { (o2, o1) };
                    let a12 = flaeche(pi, pj, p_m[o_a as usize]).abs() + flaeche(pj, pi, p_m[o_b as usize]).abs();
                    let kb = vg.material.biegung_nm * l * l / a12;
                    b_p.push([i + off as u32, j + off as u32, o_a + off as u32, o_b + off as u32]);
                    b_r.push(0.0);
                    b_a.push(1.0 / kb);
                }
            }
            let rand: BTreeMap<(u32, u32), (u32, f64)> = kanten
                .iter()
                .filter_map(|(&k, (asum, gegen))| match gegen[..] {
                    [o] => Some((k, (o, *asum))),
                    _ => None,
                })
                .collect();
            randkanten.push(rand);
            randkanten_index.push(rand_index);
            punkte_m.push(p_m);
            biegesteif.push(vg.material.biegung_nm);
            teile.push(Teilinfo {
                name: t.teil.clone(),
                punkte: off..x.len(),
                dreiecke: d0..dreiecke.len(),
                vorgabe: vg,
            });
        }

        // Biegung über die Nähte: aufeinanderfolgende Punktpaare (aₖ, bₖ),
        // (aₖ₊₁, bₖ₊₁), deren a- und b-Strecke je eine Randkante ist.
        let mut naht_biegung_ohne = 0usize;
        let mut naht_kanten = BTreeSet::new();
        for n in &netz.naehte {
            for tn in &n.teilnaehte {
                let (ta, tb) = (tn.teil_a, tn.teil_b);
                for w in tn.paare.windows(2) {
                    let (a0, a1, b0, b1) = (w[0].a, w[1].a, w[0].b, w[1].b);
                    let (Some(&(oa, fa)), Some(&(ob, fb))) =
                        (randkanten[ta].get(&(a0.min(a1), a0.max(a1))), randkanten[tb].get(&(b0.min(b1), b0.max(b1))))
                    else {
                        continue;
                    };
                    if !naht_kanten.insert((versatz[ta] + a0.min(a1), versatz[ta] + a0.max(a1))) {
                        continue;
                    }
                    let (pa, pb) = (&punkte_m[ta], &punkte_m[tb]);
                    // Dreieck 1 = (x0, x1, oa) positiv in Teil A; Dreieck 2 =
                    // (x1, x0, ob) muss dann in Teil B positiv sein (die
                    // b-Punkte zu x1, x0) — sonst liegen die Teile gespiegelt.
                    let ((x0a, x1a), (x0b, x1b)) = if flaeche(pa[a0 as usize], pa[a1 as usize], pa[oa as usize]) > 0.0 {
                        ((a0, a1), (b0, b1))
                    } else {
                        ((a1, a0), (b1, b0))
                    };
                    if flaeche(pb[x1b as usize], pb[x0b as usize], pb[ob as usize]) <= 0.0 {
                        naht_biegung_ohne += 1;
                        continue;
                    }
                    let l = l2(pa[a0 as usize], pa[a1 as usize]);
                    let kb = 0.5 * (biegesteif[ta] + biegesteif[tb]) * l * l / (fa + fb);
                    b_p.push([x0a + versatz[ta], x1a + versatz[ta], oa + versatz[ta], ob + versatz[tb]]);
                    b_r.push(0.0);
                    b_a.push(1.0 / kb);
                }
            }
        }

        // Nähte: Punktpaare, doppelte und Selbstpaare weg.
        let mut paare = BTreeSet::new();
        for n in &netz.naehte {
            for (tn, pp) in n.paare() {
                let a = versatz[tn.teil_a] + pp.a;
                let b = versatz[tn.teil_b] + pp.b;
                if a != b {
                    paare.insert((a.min(b), a.max(b)));
                }
            }
        }
        let n_p: Vec<[u32; 2]> = paare.iter().map(|&(a, b)| [a, b]).collect();
        let n_r = vec![0.0; n_p.len()];
        let n_a = vec![param.naht_nachgiebigkeit; n_p.len()];

        // Ease: a seam whose sides differ in length gathers the surplus into
        // the **longer** side. The seam pairs already sit at the same arc
        // fraction on both sides, so the surplus is spread evenly along the
        // seam; what is missing is the solver's rest length. Shortening the
        // boundary springs of the longer side by `l_short/l_long` makes the
        // shorter side keep its length and the longer one buckle instead.
        //
        // Doing it through the rest lengths leaves the seam springs as they
        // are, so the CPU reference, the CG solve and the compute shader all
        // follow without a new field of their own.
        let mut einhalte_kanten = 0usize;
        if param.einhalteweite {
            let mut gesetzt = BTreeSet::new();
            for n in &netz.naehte {
                let (la, lb) = (n.laenge_a_mm, n.laenge_b_mm);
                if !(la > 0.0 && lb > 0.0) {
                    continue;
                }
                // Same length within rounding: nothing to ease.
                let faktor = la.min(lb) / la.max(lb);
                if faktor >= 1.0 - 1e-9 {
                    continue;
                }
                // The longer side is the one that has to give.
                let lang = if la > lb { 0usize } else { 1usize };
                for tn in &n.teilnaehte {
                    let index = if lang == 0 {
                        &randkanten_index[tn.teil_a]
                    } else {
                        &randkanten_index[tn.teil_b]
                    };
                    for w in tn.paare.windows(2) {
                        let (p0, p1) = if lang == 0 { (w[0].a, w[1].a) } else { (w[0].b, w[1].b) };
                        if p0 == p1 {
                            continue;
                        }
                        let Some(&e) = index.get(&(p0.min(p1), p0.max(p1))) else {
                            continue;
                        };
                        if gesetzt.insert(e) {
                            a_r[e] *= faktor;
                            einhalte_kanten += 1;
                        }
                    }
                }
            }
        }

        // The folded allowance band, if asked for: appended after the pattern
        // pieces, before the constraint groups are colour-ordered so its indices
        // take part in that ordering like every other point.
        let faltbericht = if falte {
            let f = crate::saumfalte::falte_band(netz, &x, &teile, lage);
            x.extend(f.punkte);
            masse.extend(f.masse);
            kollisionsabstand.extend(f.kollisionsabstand);
            reibung.extend(f.reibung);
            dreiecke.extend(f.dreiecke);
            ruhe_inv.extend(f.ruhe_inv);
            faser.extend(f.faser);
            teile.extend(f.teile);
            for (p, r, a) in f.abstand {
                a_p.push(p);
                a_r.push(r);
                a_a.push(a);
            }
            for (p, r, a) in f.biegung {
                b_p.push(p);
                b_r.push(r);
                b_a.push(a);
            }
            f.bericht
        } else {
            crate::saumfalte::Faltbericht::default()
        };
        let np = x.len();
        let w = masse.iter().map(|&m| if m > 0.0 { 1.0 / m } else { 0.0 }).collect();
        let biegung = Gruppe::geordnet(b_p, b_r, b_a, np);
        let biegung_grund = biegung.nachgiebigkeit.clone();
        let szene = Szene {
            v: vec![[0.0; 3]; np],
            w,
            masse,
            kollisionsabstand,
            reibung,
            abstand: Gruppe::geordnet(a_p, a_r, a_a, np),
            naht: Gruppe::geordnet(n_p, n_r, n_a, np),
            biegung,
            biegung_grund,
            dreiecke,
            ruhe_inv,
            faser,
            faser_ersatz,
            faser_entartet,
            teile,
            klebungen: Vec::new(),
            naht_biegung_ohne,
            einhalte_kanten,
            koerper,
            param,
            zeit: 0.0,
            x,
        };
        (szene, faltbericht)
    }

    /// Bending energy of the current shape, in J: `½·θ²/α` summed over the
    /// bending constraints, `θ` the dihedral angle.
    ///
    /// This is the quantity plate theory calls `½·D·κ²·A`, and unlike a
    /// deflection measurement it does not depend on how stiff the mesh happens
    /// to be against other deformation modes. A piece bent into a circular arc
    /// has no membrane strain at all, so the number is the bending stiffness and
    /// nothing else - the measure for whether a bonded pair carries `8·D` like
    /// the laminate or only `2·D` like two loose layers.
    pub fn biegeenergie(&self) -> f64 {
        let mut e = 0.0;
        for (c, p) in self.biegung.punkte.iter().enumerate() {
            let q = p.map(|i| self.x[i as usize]);
            let Some((theta, _)) = crate::cpu::dieder(q) else { continue };
            e += 0.5 * theta * theta / self.biegung.nachgiebigkeit[c];
        }
        e
    }

    /// Sets the glue zones and applies their bending term to the bending
    /// constraints inside the bonded area.
    ///
    /// Every call starts from the compliance the constraints were built with
    /// ([`Self::biegung_grund`]), so replacing or repeating the zones cannot
    /// compound the factor. A zone whose factor is 1, or that covers no
    /// interior constraint, changes nothing.
    ///
    /// The bending term is limited to the bonded area: only a constraint whose
    /// two triangles are **both** bonded is stiffened, so a glued
    /// reinforcement stiffens the patch it covers, not the whole piece.
    pub fn klebungen_setzen(&mut self, zonen: Vec<crate::klebung::Zone>) {
        self.klebungen = zonen;
        self.biegung.nachgiebigkeit.clone_from(&self.biegung_grund);
        for z in &self.klebungen {
            let f = z.biegefaktor;
            if f <= 0.0 || (f - 1.0).abs() < 1e-12 {
                continue;
            }
            for c in z.biege_constraints(self) {
                self.biegung.nachgiebigkeit[c] /= f;
            }
        }
    }

    /// Nähen mit begrenzter Geschwindigkeit: Jede Nahtfeder bekommt als
    /// Ruhelänge ihren Abstand, höchstens die bisherige Ruhelänge (beim ersten
    /// Aufruf nach [`Self::naht_offen`] der Abstand selbst), um `schritt`
    /// gekürzt, nicht unter 0. Je Bild aufgerufen, schließt sich eine Naht mit
    /// `schritt`/Bild statt in einem Bild: Ein Stoffpunkt wiegt rund 10 mg, und
    /// der implizite Schritt springt bei jeder Federstärke bis ins
    /// Gleichgewicht (Befund 96: bis 14 m/s im ersten Bild).
    pub fn naht_kuerzen(&mut self, schritt: f64) {
        for c in 0..self.naht.len() {
            let [i, j] = self.naht.punkte[c].map(|p| p as usize);
            let l = crate::koerper::norm(crate::koerper::sub(self.x[i], self.x[j]));
            self.naht.ruhe[c] = (self.naht.ruhe[c].min(l) - schritt).max(0.0);
        }
    }

    /// Setzt die Ruhelänge aller Nahtfedern auf unendlich: der nächste
    /// [`Self::naht_kuerzen`] beginnt beim Abstand.
    pub fn naht_offen(&mut self) {
        for r in self.naht.ruhe.iter_mut() {
            *r = f64::INFINITY;
        }
    }

    /// Gesamtmasse (kg).
    pub fn gesamtmasse(&self) -> f64 {
        self.masse.iter().sum()
    }

    /// Unterschritte je Bild, damit jede Dehnungskante im **weichen Bereich**
    /// von XPBD liegt: α/h² ≥ `faktor` · (wᵢ + wⱼ). Dann überträgt eine
    /// Iteration je Unterschritt den Anteil α̃/(Σw + α̃) ≥ faktor/(faktor + 1)
    /// der Federkraft — bei Faktor 9 also 90 %. Im steifen Bereich stimmt nur
    /// die Lage, nicht die Kraft (Befund 34).
    pub fn unterschritte_weich(&self, faktor: f64) -> usize {
        let mut h_max = f64::INFINITY;
        for c in 0..self.abstand.len() {
            let [i, j] = self.abstand.punkte[c].map(|p| p as usize);
            let wsum = self.w[i] + self.w[j];
            if wsum > 0.0 {
                h_max = h_max.min((self.abstand.nachgiebigkeit[c] / (faktor * wsum)).sqrt());
            }
        }
        if !h_max.is_finite() {
            return self.param.unterschritte;
        }
        (self.param.bild_dt / h_max).ceil().max(1.0) as usize
    }
}
