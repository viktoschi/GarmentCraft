//! A loaded pattern as plain data – no egui types, so everything here is unit-tested.
//!
//! [`Muster::laden`] reads a Seamly2D pattern (`.sm2d`) with its measurement file, evaluates it once
//! and keeps only owned results: piece outlines in construction coordinates (pattern units), the
//! fill as triangles ([`fuellung`]), the layout offsets, the boundary edges
//! ([`Teilrand`]) and the seam report ([`Nahtbericht`]). The [`Auswertung`] borrows the pattern and
//! is dropped when loading ends.
//!
//! ## Which files are used
//!
//! * **Measurement file:** the one chosen by hand, otherwise the one the pattern names, relative to
//!   the pattern – the same rule as `klon3d naehte` and `klon3d-view`.
//! * **Seam list:** the one chosen by hand, otherwise `<pattern stem>.naehte` next to the pattern.
//!   Without one the pattern still loads; [`Nahtstand::Keine`] says where it was looked for.
//!
//! ## Numbers
//!
//! Coordinates stay in the pattern's unit (mm, cm or inch); lengths for display are converted with
//! [`Muster::in_cm`] (the factor of [`Auswertung::mm_pro_einheit`], as [`Teilrand::in_cm`]). Seams are
//! checked by [`pruefe_naehte`] with [`Kurvenquelle::Seamly`] – the numbers of `klon3d naehte`. A
//! piece's circumference is [`Teilrand::umfang`] (also Seamly's decomposition), so the edges listed for
//! a piece add up to it. A piece whose boundary cannot be resolved has none (`None`): a length across
//! a node that cannot be evaluated would be wrong, and `klon3d naehte` prints none either.
//!
//! ## Layout
//!
//! In construction coordinates neighbouring pieces overlap. [`auslegen`] lays them out in rows
//! without overlap (copied from `klon3d-view`); the view switches between both.
//!
//! Nothing here panics on a broken file: errors come back as [`Ladefehler`] with the file name, and a
//! panic inside the pattern reader is caught and reported the same way. [`Ladefehler::Massdatei`]
//! marks the errors that another measurement file can cure, so the window can offer to choose one.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use klon3d_pattern::geom::{lies_messungen, Auswertung, Pt};
use klon3d_pattern::kanten::{Kurvenquelle, Teilrand};
use klon3d_pattern::naehte::{
    lies_nahtliste, pruefe_naehte, Nahtbericht, Nahtergebnis, MIN_SEITE_CM, TOLERANZ_CM,
};
use klon3d_pattern::{read_path, Pattern};

/// Width to height of the layout ([`auslegen`]), roughly a landscape window.
pub const AUSLAGE_SEITENVERHAELTNIS: f64 = 1.6;

// ───────────────────────────────────────────────────────── Huelle

/// Axis-aligned bounding box: x from, x to, y from, y to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Huelle {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

impl Huelle {
    /// Box around the points; an empty slice gives an inverted box.
    pub fn um(punkte: &[Pt]) -> Huelle {
        let mut h = Huelle {
            x0: f64::MAX,
            x1: f64::MIN,
            y0: f64::MAX,
            y1: f64::MIN,
        };
        for p in punkte {
            h.x0 = h.x0.min(p.x);
            h.x1 = h.x1.max(p.x);
            h.y0 = h.y0.min(p.y);
            h.y1 = h.y1.max(p.y);
        }
        h
    }

    pub fn verschoben(self, v: Pt) -> Huelle {
        Huelle {
            x0: self.x0 + v.x,
            x1: self.x1 + v.x,
            y0: self.y0 + v.y,
            y1: self.y1 + v.y,
        }
    }

    pub fn vereint(self, o: Huelle) -> Huelle {
        Huelle {
            x0: self.x0.min(o.x0),
            x1: self.x1.max(o.x1),
            y0: self.y0.min(o.y0),
            y1: self.y1.max(o.y1),
        }
    }

    pub fn breite(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn hoehe(&self) -> f64 {
        self.y1 - self.y0
    }
}

// ───────────────────────────────────────────────────────── Daten

/// One piece with its finished outline – from here on nothing is evaluated any more.
#[derive(Debug, Clone)]
pub struct Teil {
    /// Full name as in the pattern (`<ohne Namen>` if it has none).
    pub name: String,
    /// The name without the prefix all pieces of the pattern share.
    pub kurz: String,
    /// The name `pruefe_naehte` uses for the piece (name, otherwise id); the seam report refers to
    /// pieces by it.
    pub schluessel: String,
    /// Seam line in construction coordinates, open (last point ≠ first point).
    pub umriss: Vec<Pt>,
    /// The inside of `umriss` as triangles for the filled drawing ([`fuellung`]).
    pub fuellung: Fuellung,
    /// Bounding box in construction coordinates.
    pub huelle: Huelle,
    /// Offset in the layout, pattern units.
    pub auslage: Pt,
    /// Where the label goes, construction coordinates.
    pub etikett: Pt,
    /// Number of nodes of the piece in the pattern file.
    pub knoten: usize,
    /// Circumference in cm; `None` when the boundary cannot be resolved (see the module docs).
    pub umfang_cm: Option<f64>,
    /// Boundary edges, or why the boundary cannot be resolved.
    pub rand: Result<Teilrand, String>,
    /// For every boundary edge the seams (indices into [`Nahtbericht::naehte`]) that use it.
    pub kanten_naehte: Vec<Vec<usize>>,
}

impl Teil {
    /// The offset the piece is drawn with.
    pub fn versatz(&self, ausgelegt: bool) -> Pt {
        if ausgelegt {
            self.auslage
        } else {
            Pt::new(0.0, 0.0)
        }
    }
}

/// What became of the seam list.
#[derive(Debug, Clone)]
pub enum Nahtstand {
    /// No list chosen and none at the default place.
    Keine { gesucht: PathBuf },
    /// The list exists but cannot be read or does not fit the pattern.
    Fehler { pfad: PathBuf, text: String },
    /// The list was checked.
    Geprueft { pfad: PathBuf, bericht: Nahtbericht },
}

impl Nahtstand {
    pub fn bericht(&self) -> Option<&Nahtbericht> {
        match self {
            Nahtstand::Geprueft { bericht, .. } => Some(bericht),
            _ => None,
        }
    }
}

/// How a boundary edge is used by the seam list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kantenart {
    /// In no seam.
    Ohne,
    /// In one seam within the tolerance.
    Genaeht,
    /// In one seam that is conspicuous ([`naht_auffaellig`]).
    Auffaellig,
    /// In more than one seam – an error of the list.
    Doppelt,
}

/// Which files to load. The window keeps it, so that choosing another measurement file or seam
/// list reloads the same pattern.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ladeauftrag {
    pub muster: PathBuf,
    /// Chosen by hand; `None` = the one the pattern names.
    pub massdatei: Option<PathBuf>,
    /// Chosen by hand; `None` = `<stem>.naehte` next to the pattern.
    pub nahtliste: Option<PathBuf>,
}

impl Ladeauftrag {
    pub fn neu(muster: impl Into<PathBuf>) -> Self {
        Ladeauftrag {
            muster: muster.into(),
            massdatei: None,
            nahtliste: None,
        }
    }

    /// The same request with absolute paths, so that it (and the folder remembered from it) still
    /// means the same files when the working folder changes. `.` and `..` are removed by the same
    /// rule as in the library ([`crate::dateiwahl::bereinigt`]): `std::path::absolute` keeps `..`
    /// on Unix, and one file reached two ways would be two entries of the recently opened ones.
    pub fn absolut(self) -> Self {
        let abs = |p: PathBuf| crate::dateiwahl::bereinigt(&std::path::absolute(&p).unwrap_or(p));
        Ladeauftrag {
            muster: abs(self.muster),
            massdatei: self.massdatei.map(abs),
            nahtliste: self.nahtliste.map(abs),
        }
    }
}

/// Why a pattern could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ladefehler {
    /// The measurement file is not named, missing or unreadable; another one may cure it.
    Massdatei(String),
    /// Anything else: pattern missing or broken, no usable piece, a panic of the reader.
    Sonst(String),
}

impl Ladefehler {
    /// The message for the user.
    pub fn text(&self) -> &str {
        match self {
            Ladefehler::Massdatei(t) | Ladefehler::Sonst(t) => t,
        }
    }
}

impl std::fmt::Display for Ladefehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text())
    }
}

/// A pattern with all pieces, ready to draw and to list.
#[derive(Debug, Clone)]
pub struct Muster {
    pub pfad: PathBuf,
    /// The measurement file actually read.
    pub massdatei: PathBuf,
    /// Whether `massdatei` was chosen by hand (otherwise the pattern names it).
    pub massdatei_von_hand: bool,
    /// The pattern's unit (`mm`, `cm`, `inch`).
    pub einheit: String,
    pub mm_pro_einheit: f64,
    /// Pieces with an outline, in file order.
    pub teile: Vec<Teil>,
    /// Names of pieces without a usable outline; listed, not drawn.
    pub ohne_umriss: Vec<String>,
    pub naehte: Nahtstand,
}

/// A seam is conspicuous above [`TOLERANZ_CM`] or with a side without length – as `klon3d naehte`.
pub fn naht_auffaellig(n: &Nahtergebnis) -> bool {
    n.auffaellig(TOLERANZ_CM)
}

impl Muster {
    /// Load a pattern. Errors name the file; a panic of the reader is caught and reported.
    pub fn laden(auftrag: &Ladeauftrag) -> Result<Muster, Ladefehler> {
        match catch_unwind(AssertUnwindSafe(|| laden_ungeschuetzt(auftrag))) {
            Ok(ergebnis) => ergebnis,
            Err(nutzlast) => {
                let grund = nutzlast
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| nutzlast.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unbekannter Fehler".into());
                Err(Ladefehler::Sonst(format!(
                    "{}: interner Fehler beim Lesen: {grund}",
                    auftrag.muster.display()
                )))
            }
        }
    }

    /// File name of the measurement file.
    pub fn massdatei_name(&self) -> String {
        dateiname(&self.massdatei)
    }

    /// Pattern units → cm.
    pub fn in_cm(&self, wert: f64) -> f64 {
        wert * self.mm_pro_einheit / 10.0
    }

    /// File name for titles and the status bar.
    pub fn dateiname(&self) -> String {
        dateiname(&self.pfad)
    }

    /// Bounding box over all pieces as drawn; `None` without pieces.
    pub fn huelle(&self, ausgelegt: bool) -> Option<Huelle> {
        self.teile
            .iter()
            .map(|t| t.huelle.verschoben(t.versatz(ausgelegt)))
            .reduce(Huelle::vereint)
    }

    /// The seam with the largest |Δ|.
    pub fn groesste_abweichung(&self) -> Option<&Nahtergebnis> {
        self.naehte.bericht()?.groesste_abweichung()
    }

    /// How edge `kante` of piece `teil` is used by the seam list.
    pub fn kantenart(&self, teil: usize, kante: usize) -> Kantenart {
        let Some(naehte) = self
            .teile
            .get(teil)
            .and_then(|t| t.kanten_naehte.get(kante))
        else {
            return Kantenart::Ohne;
        };
        match naehte.as_slice() {
            [] => Kantenart::Ohne,
            [n] => {
                let auff = self
                    .naehte
                    .bericht()
                    .and_then(|b| b.naehte.get(*n))
                    .is_some_and(naht_auffaellig);
                if auff {
                    Kantenart::Auffaellig
                } else {
                    Kantenart::Genaeht
                }
            }
            _ => Kantenart::Doppelt,
        }
    }

    /// Pieces (indices into [`Muster::teile`]) and their edges used by seam `naht`.
    pub fn kanten_der_naht(&self, naht: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (ti, t) in self.teile.iter().enumerate() {
            for (k, naehte) in t.kanten_naehte.iter().enumerate() {
                if naehte.contains(&naht) {
                    out.push((ti, k));
                }
            }
        }
        out
    }
}

/// The measurement file the pattern names, relative to the pattern.
pub fn massdatei_des_musters(muster_pfad: &Path, muster: &Pattern) -> Result<PathBuf, String> {
    let m = muster.measurements.as_ref().ok_or_else(|| {
        format!(
            "{}: Muster nennt keine Maßdatei – bitte über Avatar › Maßdatei wählen… eine angeben",
            muster_pfad.display()
        )
    })?;
    let p = Path::new(m);
    Ok(if p.is_absolute() {
        p.to_path_buf()
    } else {
        muster_pfad.parent().unwrap_or(Path::new(".")).join(p)
    })
}

/// The file name of `pfad`, or the whole path if it has none.
pub fn dateiname(pfad: &Path) -> String {
    pfad.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| pfad.display().to_string())
}

/// Default place of the seam list: `<stem>.naehte` next to the pattern.
pub fn nahtliste_neben(muster_pfad: &Path) -> PathBuf {
    muster_pfad.with_extension("naehte")
}

fn laden_ungeschuetzt(auftrag: &Ladeauftrag) -> Result<Muster, Ladefehler> {
    let pfad = &auftrag.muster;
    if !pfad.is_file() {
        return Err(Ladefehler::Sonst(format!(
            "Muster nicht gefunden: {}",
            pfad.display()
        )));
    }
    let muster =
        read_path(pfad).map_err(|e| Ladefehler::Sonst(format!("{}: {e}", pfad.display())))?;
    // Before the measurement file: a file without pieces is no pattern, and asking for a
    // measurement file would send the user the wrong way.
    if muster.pieces().is_empty() {
        return Err(Ladefehler::Sonst(format!(
            "{}: kein Seamly2D-Muster mit Teilen",
            pfad.display()
        )));
    }
    let massdatei = match &auftrag.massdatei {
        Some(m) => m.clone(),
        None => massdatei_des_musters(pfad, &muster).map_err(Ladefehler::Massdatei)?,
    };
    if !massdatei.is_file() {
        return Err(Ladefehler::Massdatei(format!(
            "Maßdatei fehlt: {} (für {})",
            massdatei.display(),
            dateiname(pfad)
        )));
    }
    let mess = lies_messungen(&massdatei, &muster.unit)
        .map_err(|e| Ladefehler::Massdatei(format!("{}: {e}", massdatei.display())))?;
    let ev = Auswertung::new(&muster, mess);

    let mut teile = Vec::new();
    let mut ohne_umriss = Vec::new();
    for p in muster.pieces() {
        let name = p.name.clone().unwrap_or_else(|| "<ohne Namen>".into());
        let umriss = bereinigt(&ev.stueck_umriss(p).unwrap_or_default());
        if umriss.len() < 3 {
            ohne_umriss.push(name);
            continue;
        }
        let rand = ev
            .teilrand(p, Kurvenquelle::Seamly)
            .map_err(|e| e.to_string());
        let umfang_cm = rand.as_ref().ok().map(|r| r.in_cm(r.umfang()));
        let kanten = rand.as_ref().map(|r| r.kanten.len()).unwrap_or(0);
        teile.push(Teil {
            schluessel: p.name.clone().unwrap_or_else(|| p.id.clone()),
            name,
            kurz: String::new(),
            fuellung: fuellung(&umriss),
            huelle: Huelle::um(&umriss),
            auslage: Pt::new(0.0, 0.0),
            etikett: etikett(&umriss),
            knoten: p.nodes.as_ref().map(|n| n.items.len()).unwrap_or(0),
            umfang_cm,
            rand,
            kanten_naehte: vec![Vec::new(); kanten],
            umriss,
        });
    }
    if teile.is_empty() {
        return Err(Ladefehler::Sonst(format!(
            "{}: kein Teil mit brauchbarer Nahtlinie",
            pfad.display()
        )));
    }
    let namen: Vec<String> = teile.iter().map(|t| t.name.clone()).collect();
    let vorsatz = vorsatz_laenge(&namen);
    let huellen: Vec<Huelle> = teile.iter().map(|t| t.huelle).collect();
    for (t, v) in teile
        .iter_mut()
        .zip(auslegen(&huellen, AUSLAGE_SEITENVERHAELTNIS))
    {
        let kurz = t.name[vorsatz..].trim();
        t.kurz = if kurz.is_empty() {
            t.name.clone()
        } else {
            kurz.to_string()
        };
        t.auslage = v;
    }

    let naehte = nahtstand(&ev, pfad, auftrag.nahtliste.as_deref());
    if let Nahtstand::Geprueft { bericht, .. } = &naehte {
        belegen(&mut teile, bericht);
    }
    Ok(Muster {
        pfad: pfad.clone(),
        massdatei,
        massdatei_von_hand: auftrag.massdatei.is_some(),
        einheit: muster.unit.clone(),
        mm_pro_einheit: ev.mm_pro_einheit(),
        teile,
        ohne_umriss,
        naehte,
    })
}

/// Read and check the seam list, as `klon3d naehte` does.
fn nahtstand(ev: &Auswertung<'_>, muster_pfad: &Path, gewaehlt: Option<&Path>) -> Nahtstand {
    let pfad = match gewaehlt {
        Some(p) => p.to_path_buf(),
        None => {
            let p = nahtliste_neben(muster_pfad);
            if !p.is_file() {
                return Nahtstand::Keine { gesucht: p };
            }
            p
        }
    };
    let fehler = |text: String| Nahtstand::Fehler {
        pfad: pfad.clone(),
        text,
    };
    let text = match std::fs::read_to_string(&pfad) {
        Ok(t) => t,
        Err(e) => return fehler(format!("{}: {e}", pfad.display())),
    };
    let defs = match lies_nahtliste(&text) {
        Ok(d) => d,
        Err(e) => return fehler(format!("{}: {e}", pfad.display())),
    };
    match pruefe_naehte(ev, &defs, Kurvenquelle::Seamly) {
        Ok(bericht) => Nahtstand::Geprueft { pfad, bericht },
        Err(e) => fehler(format!("{}: {e}", pfad.display())),
    }
}

/// Note for every boundary edge which seams use it.
fn belegen(teile: &mut [Teil], bericht: &Nahtbericht) {
    let nach_schluessel: HashMap<String, usize> = teile
        .iter()
        .enumerate()
        .rev()
        .map(|(i, t)| (t.schluessel.clone(), i))
        .collect();
    for (ni, n) in bericht.naehte.iter().enumerate() {
        for seite in [&n.a, &n.b] {
            for ts in &seite.teilstuecke {
                let Some(&ti) = nach_schluessel.get(&ts.teil) else {
                    continue;
                };
                let belegung = &mut teile[ti].kanten_naehte;
                for &k in &ts.kanten {
                    if let Some(liste) = belegung.get_mut(k) {
                        if !liste.contains(&ni) {
                            liste.push(ni);
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────────────────────────────────── Formatierung

/// A length in cm as `klon3d naehte` prints it (four decimals).
pub fn cm4(wert: f64) -> String {
    format!("{wert:.4}")
}

/// Δ as `klon3d naehte` prints it: sign, four decimals, no `-0.0000`.
pub fn delta4(wert: f64) -> String {
    let w = if wert.abs() < 5e-5 { 0.0 } else { wert };
    format!("{w:+.4}")
}

/// The marker `klon3d naehte` puts behind a seam, or `""`.
pub fn naht_vermerk(n: &Nahtergebnis) -> &'static str {
    if n.a.laenge_cm < MIN_SEITE_CM || n.b.laenge_cm < MIN_SEITE_CM {
        "Seite ohne Länge"
    } else if naht_auffaellig(n) {
        "über Toleranz"
    } else {
        ""
    }
}

// ───────────────────────────────────────────────────── Geometrie

/// Drop consecutive duplicate points and a closing point equal to the first.
pub fn bereinigt(punkte: &[Pt]) -> Vec<Pt> {
    const EPS: f64 = 1e-9;
    let mut out: Vec<Pt> = Vec::with_capacity(punkte.len());
    for &p in punkte {
        if out.last().is_none_or(|q| q.dist(p) > EPS) {
            out.push(p);
        }
    }
    while out.len() > 1 && out[0].dist(*out.last().unwrap()) <= EPS {
        out.pop();
    }
    out
}

/// Signed area of a ring (shoelace formula); positive for counter-clockwise in a y-up frame.
pub fn flaeche(punkte: &[Pt]) -> f64 {
    let n = punkte.len();
    (0..n)
        .map(|i| {
            let (p, q) = (punkte[i], punkte[(i + 1) % n]);
            p.x * q.y - q.x * p.y
        })
        .sum::<f64>()
        / 2.0
}

/// A filled area as triangles; `dreiecke` index into `punkte`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fuellung {
    pub punkte: Vec<Pt>,
    pub dreiecke: Vec<[u32; 3]>,
}

impl Fuellung {
    /// The sum of the triangle areas.
    pub fn flaeche(&self) -> f64 {
        self.dreiecke
            .iter()
            .map(|d| {
                let [a, b, c] = d.map(|i| self.punkte[i as usize]);
                flaeche(&[a, b, c]).abs()
            })
            .sum()
    }
}

/// The inside of a ring by the even-odd rule – the rule of the hit test
/// [`crate::ansicht2d::innen`] – as triangles that do not overlap.
///
/// egui only fills convex paths, and pattern pieces are rarely convex; some outlines also run back
/// along a curve they already traced, or cross themselves. The ring is cut into horizontal bands
/// at every vertex height and at every height where two edges cross. Inside a band no two edges
/// cross, so the edges spanning it, sorted by x, pair up – first with second, third with fourth –
/// into trapezoids of the inside. Bands and trapezoids are disjoint, so the semi-transparent fill
/// is never painted twice. O(n²) in the number of points, once at load time.
pub fn fuellung(ring: &[Pt]) -> Fuellung {
    let mut out = Fuellung::default();
    let n = ring.len();
    if n < 3 {
        return out;
    }
    // Horizontal edges span no band.
    let kanten: Vec<(Pt, Pt)> = (0..n)
        .map(|i| (ring[i], ring[(i + 1) % n]))
        .filter(|(a, b)| a.y != b.y)
        .collect();
    let mut hoehen: Vec<f64> = ring.iter().map(|p| p.y).collect();
    for i in 0..kanten.len() {
        for j in i + 1..kanten.len() {
            if let Some(y) = kreuzungshoehe(kanten[i], kanten[j]) {
                hoehen.push(y);
            }
        }
    }
    hoehen.sort_by(f64::total_cmp);
    hoehen.dedup();
    let mut im_band: Vec<(f64, (Pt, Pt))> = Vec::new();
    for band in hoehen.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        let mitte = 0.5 * (y0 + y1);
        im_band.clear();
        im_band.extend(
            kanten
                .iter()
                .filter(|(a, b)| a.y.min(b.y) <= y0 && a.y.max(b.y) >= y1)
                .map(|&k| (x_bei(k, mitte), k)),
        );
        im_band.sort_by(|p, q| p.0.total_cmp(&q.0));
        for paar in im_band.chunks_exact(2) {
            let (l, r) = (paar[0].1, paar[1].1);
            let (l0, r0, l1, r1) = (x_bei(l, y0), x_bei(r, y0), x_bei(l, y1), x_bei(r, y1));
            // A width at or below zero is a corner (or rounding at a crossing): no triangle.
            let (unten, oben) = (r0 > l0, r1 > l1);
            if !unten && !oben {
                continue;
            }
            let i = out.punkte.len() as u32;
            out.punkte.extend([
                Pt::new(l0, y0),
                Pt::new(r0, y0),
                Pt::new(r1, y1),
                Pt::new(l1, y1),
            ]);
            if unten {
                out.dreiecke.push([i, i + 1, i + 2]);
            }
            if oben {
                out.dreiecke.push([i, i + 2, i + 3]);
            }
        }
    }
    out
}

/// x of the edge `a b` at height `y`; exact at the end points.
fn x_bei((a, b): (Pt, Pt), y: f64) -> f64 {
    if y == a.y {
        a.x
    } else if y == b.y {
        b.x
    } else {
        a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y)
    }
}

/// The height at which two edges cross inside both (not at an end point), if they do.
fn kreuzungshoehe((a, b): (Pt, Pt), (c, d): (Pt, Pt)) -> Option<f64> {
    let getrennt = |p: f64, q: f64, u: f64, v: f64| p.max(q) < u.min(v) || u.max(v) < p.min(q);
    if getrennt(a.y, b.y, c.y, d.y) || getrennt(a.x, b.x, c.x, d.x) {
        return None;
    }
    let (rx, ry) = (b.x - a.x, b.y - a.y);
    let (sx, sy) = (d.x - c.x, d.y - c.y);
    let nenner = rx * sy - ry * sx;
    if nenner == 0.0 {
        // Parallel: no single crossing; collinear overlaps change no order.
        return None;
    }
    let (qx, qy) = (c.x - a.x, c.y - a.y);
    let t = (qx * sy - qy * sx) / nenner;
    let u = (qx * ry - qy * rx) / nenner;
    (t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0).then(|| a.y + t * ry)
}

/// Lay pieces out in rows; the result is the offset per piece.
///
/// The tallest piece first, left to right, with a gap of 6 % of the tallest piece; a row ends when
/// it would get wider than needed for an overall shape of about `seitenverhaeltnis` (width to
/// height). Pieces of equal height – left and right – stay side by side in their order. Nothing is
/// rotated: the orientation in the pattern is the grain line. (Copied from `klon3d-view`.)
pub fn auslegen(huellen: &[Huelle], seitenverhaeltnis: f64) -> Vec<Pt> {
    let abstand = 0.06 * huellen.iter().map(Huelle::hoehe).fold(0.0, f64::max);
    let flaeche: f64 = huellen
        .iter()
        .map(|h| (h.breite() + abstand) * (h.hoehe() + abstand))
        .sum();
    let reihe_max = (flaeche * seitenverhaeltnis)
        .sqrt()
        .max(huellen.iter().map(Huelle::breite).fold(0.0, f64::max));

    let mut folge: Vec<usize> = (0..huellen.len()).collect();
    folge.sort_by(|&a, &b| huellen[b].hoehe().total_cmp(&huellen[a].hoehe()));

    let mut versatz = vec![Pt::new(0.0, 0.0); huellen.len()];
    let (mut x, mut y, mut reihe_h) = (0.0, 0.0, 0.0f64);
    for i in folge {
        let h = &huellen[i];
        if x > 0.0 && x + h.breite() > reihe_max {
            x = 0.0;
            y += reihe_h + abstand;
            reihe_h = 0.0;
        }
        versatz[i] = Pt::new(x - h.x0, y - h.y0);
        x += h.breite() + abstand;
        reihe_h = reihe_h.max(h.hoehe());
    }
    versatz
}

/// Length of the prefix **all** names share, up to and including the last `" - "` in it.
/// "Bustier - Obercup rechts" becomes "Obercup rechts"; without a common separator the name stays
/// whole. (Copied from `klon3d-view`.)
pub fn vorsatz_laenge(namen: &[String]) -> usize {
    let Some(erster) = namen.first() else {
        return 0;
    };
    let mut n = erster.len();
    for name in &namen[1..] {
        n = erster
            .bytes()
            .zip(name.bytes())
            .take(n)
            .take_while(|(a, b)| a == b)
            .count();
    }
    while !erster.is_char_boundary(n) {
        n -= 1;
    }
    erster[..n].rfind(" - ").map(|i| i + 3).unwrap_or(0)
}

/// Area centroid of a ring; for degenerate rings the mean of the points.
fn schwerpunkt(punkte: &[Pt]) -> Pt {
    let n = punkte.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (punkte[i], punkte[(i + 1) % n]);
        let k = p.x * q.y - q.x * p.y;
        a += k;
        cx += (p.x + q.x) * k;
        cy += (p.y + q.y) * k;
    }
    if a.abs() < 1e-12 {
        let m = n.max(1) as f64;
        return Pt::new(
            punkte.iter().map(|p| p.x).sum::<f64>() / m,
            punkte.iter().map(|p| p.y).sum::<f64>() / m,
        );
    }
    Pt::new(cx / (3.0 * a), cy / (3.0 * a))
}

/// The intervals of the horizontal line at `y` that lie inside the ring, left to right.
fn abschnitte(punkte: &[Pt], y: f64) -> Vec<(f64, f64)> {
    let n = punkte.len();
    let mut xs = Vec::new();
    for i in 0..n {
        let (a, b) = (punkte[i], punkte[(i + 1) % n]);
        if (a.y > y) != (b.y > y) {
            xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
        }
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    xs.chunks_exact(2).map(|p| (p[0], p[1])).collect()
}

/// Where a piece's name goes: on the horizontal through the area centroid, in the middle of the
/// widest interval inside the piece. The centroid alone lies outside arcs and crescents (collars,
/// trims). (After `klon3d-view`.)
pub fn etikett(punkte: &[Pt]) -> Pt {
    let s = schwerpunkt(punkte);
    abschnitte(punkte, s.y)
        .into_iter()
        .max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0)))
        .map(|(a, b)| Pt::new((a + b) / 2.0, s.y))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn muster_ordner() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
    }

    fn bluse() -> PathBuf {
        muster_ordner().join("bluse3_v2.sm2d")
    }

    fn pt(x: f64, y: f64) -> Pt {
        Pt::new(x, y)
    }

    /// Smallest signed distance of `p` to the edges of triangle `t`, positive inside (either
    /// orientation); `None` for a degenerate triangle.
    fn innenabstand(p: Pt, t: &[Pt; 3]) -> Option<f64> {
        let f2 = (t[1].x - t[0].x) * (t[2].y - t[0].y) - (t[1].y - t[0].y) * (t[2].x - t[0].x);
        if f2 == 0.0 {
            return None;
        }
        let mut m = f64::INFINITY;
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            let l = a.dist(b);
            if l == 0.0 {
                return None;
            }
            let kreuz = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            m = m.min(kreuz * f2.signum() / l);
        }
        Some(m)
    }

    /// Raster check of a fill against the even-odd rule of the hit test
    /// (`ansicht2d::innen`, independent code): on a grid over the ring's box no point lies
    /// strictly inside two triangles, every point inside the ring lies in a triangle and no
    /// point outside lies strictly inside one. The grid is offset by irrational fractions, so its
    /// points do not fall on vertex heights.
    fn pruefe_fuellung(ring: &[Pt], f: &Fuellung, was: &str) {
        const N: usize = 64;
        let h = Huelle::um(ring);
        let (bw, bh) = (h.breite().max(1e-9), h.hoehe().max(1e-9));
        let eps = 1e-9 * bw.max(bh);
        let punkt = |ix: usize, iy: usize| {
            pt(
                h.x0 + (ix as f64 + 0.618_033_988_7) * bw / N as f64,
                h.y0 + (iy as f64 + 0.414_213_562_3) * bh / N as f64,
            )
        };
        let (mut streng, mut locker) = (vec![0u32; N * N], vec![0u32; N * N]);
        for d in &f.dreiecke {
            let t = d.map(|i| f.punkte[i as usize]);
            let th = Huelle::um(&t);
            // Grid indices whose points can lie in the triangle's box.
            let von = |w: f64, w0: f64, b: f64| {
                (((w - w0) / b * N as f64 - 1.0).floor().max(0.0) as usize).min(N)
            };
            let bis = |w: f64, w0: f64, b: f64| {
                (((w - w0) / b * N as f64 + 1.0).ceil().max(0.0) as usize).min(N)
            };
            for iy in von(th.y0, h.y0, bh)..bis(th.y1, h.y0, bh) {
                for ix in von(th.x0, h.x0, bw)..bis(th.x1, h.x0, bw) {
                    if let Some(m) = innenabstand(punkt(ix, iy), &t) {
                        streng[iy * N + ix] += (m > eps) as u32;
                        locker[iy * N + ix] += (m > -eps) as u32;
                    }
                }
            }
        }
        for iy in 0..N {
            for ix in 0..N {
                let p = punkt(ix, iy);
                let (s, l) = (streng[iy * N + ix], locker[iy * N + ix]);
                assert!(s <= 1, "{was}: {p:?} {s}-fach gefüllt");
                if crate::ansicht2d::innen(ring, p) {
                    assert!(l >= 1, "{was}: {p:?} innen, aber nicht gefüllt");
                } else {
                    assert_eq!(s, 0, "{was}: {p:?} außen, aber gefüllt");
                }
            }
        }
    }

    /// All `.sm2d` below `ordner`.
    fn alle_muster(ordner: &Path, out: &mut Vec<PathBuf>) {
        let mut e: Vec<PathBuf> = std::fs::read_dir(ordner)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        e.sort();
        for p in e {
            if p.is_dir() {
                alle_muster(&p, out);
            } else if p.extension().is_some_and(|x| x == "sm2d") {
                out.push(p);
            }
        }
    }

    #[test]
    fn bluse3_v2_teile_wie_der_musterleser() {
        let m = Muster::laden(&Ladeauftrag::neu(bluse())).expect("Bluse3 v2 lädt");
        // Independent count: pieces whose outline has at least three points.
        let muster = read_path(&bluse()).unwrap();
        let mess =
            lies_messungen(&muster_ordner().join("measurements.smis"), &muster.unit).unwrap();
        let ev = Auswertung::new(&muster, mess);
        let mit_umriss: Vec<String> = muster
            .pieces()
            .into_iter()
            .filter(|p| ev.stueck_umriss(p).map(|u| u.len() >= 3).unwrap_or(false))
            .map(|p| p.name.clone().unwrap())
            .collect();
        assert_eq!(m.teile.len(), mit_umriss.len());
        assert_eq!(
            m.teile.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
            mit_umriss
        );
        assert_eq!(m.einheit, "cm");
        assert!((m.mm_pro_einheit - 10.0).abs() < 1e-12);
        for t in &m.teile {
            assert!(!t.kurz.starts_with("Bluse 3 - "), "{}", t.kurz);
            let r = t.rand.as_ref().expect("Rand auflösbar");
            assert_eq!(t.umfang_cm, Some(r.in_cm(r.umfang())));
            assert_eq!(t.kanten_naehte.len(), r.kanten.len());
        }
    }

    #[test]
    fn bluse3_v2_naehte_wie_pruefe_naehte() {
        let m = Muster::laden(&Ladeauftrag::neu(bluse())).unwrap();
        let Nahtstand::Geprueft { pfad, bericht } = &m.naehte else {
            panic!("Nahtliste nicht geprüft: {:?}", m.naehte);
        };
        assert_eq!(pfad, &bluse().with_extension("naehte"));

        let muster = read_path(&bluse()).unwrap();
        let mess =
            lies_messungen(&muster_ordner().join("measurements.smis"), &muster.unit).unwrap();
        let ev = Auswertung::new(&muster, mess);
        let defs = lies_nahtliste(&std::fs::read_to_string(pfad).unwrap()).unwrap();
        let soll = pruefe_naehte(&ev, &defs, Kurvenquelle::Seamly).unwrap();
        assert_eq!(bericht.naehte.len(), 24);
        assert_eq!(bericht.naehte.len(), soll.naehte.len());
        for (a, b) in bericht.naehte.iter().zip(&soll.naehte) {
            assert_eq!(a.kennung, b.kennung);
            assert_eq!(a.diff_cm, b.diff_cm);
            assert_eq!(a.a.laenge_cm, b.a.laenge_cm);
        }
        // `klon3d naehte` prints N07L with Δ -0.0737 as the largest deviation.
        let g = m.groesste_abweichung().unwrap();
        assert_eq!(g.kennung, "N07L");
        assert_eq!(delta4(g.diff_cm), "-0.0737");

        // Every seam marks its edges; the Bluse3 list uses no edge twice.
        for ni in 0..bericht.naehte.len() {
            assert!(
                !m.kanten_der_naht(ni).is_empty(),
                "{}",
                bericht.naehte[ni].kennung
            );
        }
        for (ti, t) in m.teile.iter().enumerate() {
            for k in 0..t.kanten_naehte.len() {
                assert_ne!(m.kantenart(ti, k), Kantenart::Doppelt);
                assert_ne!(m.kantenart(ti, k), Kantenart::Auffaellig);
            }
        }
    }

    #[test]
    fn fehlende_dateien_geben_meldungen() {
        let weg = muster_ordner().join("gibt_es_nicht.sm2d");
        let e = Muster::laden(&Ladeauftrag::neu(&weg)).unwrap_err();
        assert!(matches!(e, Ladefehler::Sonst(_)), "{e:?}");
        assert!(e.text().contains("gibt_es_nicht.sm2d"), "{e}");

        let mut a = Ladeauftrag::neu(bluse());
        a.massdatei = Some(muster_ordner().join("fehlt.smis"));
        let e = Muster::laden(&a).unwrap_err();
        assert!(matches!(e, Ladefehler::Massdatei(_)), "{e:?}");
        assert!(e.text().starts_with("Maßdatei fehlt"), "{e}");
        assert!(e.text().contains("bluse3_v2.sm2d"), "{e}");

        let mut a = Ladeauftrag::neu(bluse());
        a.nahtliste = Some(muster_ordner().join("fehlt.naehte"));
        let m = Muster::laden(&a).unwrap();
        assert!(matches!(m.naehte, Nahtstand::Fehler { .. }));

        // bluse2_v29 has no seam list next to it.
        let m = Muster::laden(&Ladeauftrag::neu(muster_ordner().join("bluse2_v29.sm2d"))).unwrap();
        assert!(matches!(m.naehte, Nahtstand::Keine { .. }));
        assert!(m.groesste_abweichung().is_none());
    }

    #[test]
    fn kein_muster_sondern_text() {
        let dir = std::env::temp_dir().join(format!("klon3d-app-muster-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("kaputt.sm2d");
        std::fs::write(&p, "das ist kein XML <<<").unwrap();
        let e = Muster::laden(&Ladeauftrag::neu(&p)).unwrap_err();
        assert!(e.text().contains("kaputt.sm2d"), "{e}");
        // Plain text is no pattern; the error must not ask for a measurement file.
        let p = dir.join("text.sm2d");
        std::fs::write(&p, "Hallo Welt\nnur Text\n").unwrap();
        let e = Muster::laden(&Ladeauftrag::neu(&p)).unwrap_err();
        assert!(matches!(e, Ladefehler::Sonst(_)), "{e:?}");
        assert!(!e.text().contains("Maßdatei"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn muster_ohne_massdatei_ist_ein_massdateifehler() {
        // bluse3_v2 without its <measurements> element: the pattern names no measurement file.
        let dir = std::env::temp_dir().join(format!("klon3d-app-ohne-mass-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = std::fs::read_to_string(bluse()).unwrap();
        let ohne: String = text
            .lines()
            .filter(|z| !z.contains("<measurements"))
            .map(|z| format!("{z}\n"))
            .collect();
        assert_ne!(ohne.len(), text.len(), "bluse3_v2 nennt eine Maßdatei");
        let p = dir.join("ohne.sm2d");
        std::fs::write(&p, ohne).unwrap();
        let e = Muster::laden(&Ladeauftrag::neu(&p)).unwrap_err();
        assert!(matches!(e, Ladefehler::Massdatei(_)), "{e:?}");
        // With a measurement file chosen by hand it loads.
        let mut a = Ladeauftrag::neu(&p);
        a.massdatei = Some(muster_ordner().join("measurements.smis"));
        let m = Muster::laden(&a).unwrap();
        assert_eq!(m.teile.len(), 28);
        assert!(m.massdatei_von_hand);
        assert_eq!(m.massdatei_name(), "measurements.smis");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn formatierung_wie_klon3d_naehte() {
        assert_eq!(delta4(-0.0737), "-0.0737");
        assert_eq!(delta4(0.0), "+0.0000");
        assert_eq!(delta4(-0.00001), "+0.0000");
        assert_eq!(delta4(0.0007), "+0.0007");
        assert_eq!(cm4(42.857_14), "42.8571");
    }

    #[test]
    fn fuellung_deckt_ein_l() {
        // L shape, clockwise in y-down drawing coordinates, area 3.
        let l = [
            pt(0.0, 0.0),
            pt(2.0, 0.0),
            pt(2.0, 1.0),
            pt(1.0, 1.0),
            pt(1.0, 2.0),
            pt(0.0, 2.0),
        ];
        let f = fuellung(&l);
        assert!((f.flaeche() - 3.0).abs() < 1e-12, "{}", f.flaeche());
        pruefe_fuellung(&l, &f, "L");
        // Reverse orientation gives the same area.
        let mut r = l.to_vec();
        r.reverse();
        assert!((fuellung(&r).flaeche() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn fuellung_kamm_und_entartetes() {
        // A comb with three teeth: strongly concave.
        let kamm = [
            pt(0.0, 0.0),
            pt(5.0, 0.0),
            pt(5.0, 3.0),
            pt(4.0, 3.0),
            pt(4.0, 1.0),
            pt(3.0, 1.0),
            pt(3.0, 3.0),
            pt(2.0, 3.0),
            pt(2.0, 1.0),
            pt(1.0, 1.0),
            pt(1.0, 3.0),
            pt(0.0, 3.0),
        ];
        let f = fuellung(&kamm);
        assert!((f.flaeche() - flaeche(&kamm).abs()).abs() < 1e-9);
        pruefe_fuellung(&kamm, &f, "Kamm");
        // Collinear points and too few points: no area.
        assert_eq!(
            fuellung(&[pt(0.0, 0.0), pt(1.0, 0.0), pt(2.0, 0.0)]).flaeche(),
            0.0
        );
        assert!(fuellung(&[pt(0.0, 0.0), pt(1.0, 0.0)]).dreiecke.is_empty());
    }

    #[test]
    fn fuellung_bei_selbstueberschneidung_nach_gerade_ungerade() {
        // A figure eight: two triangles of area 1 each (the shoelace formula gives 0).
        let acht = [pt(0.0, 0.0), pt(2.0, 2.0), pt(2.0, 0.0), pt(0.0, 2.0)];
        let f = fuellung(&acht);
        assert!((f.flaeche() - 2.0).abs() < 1e-12, "{}", f.flaeche());
        pruefe_fuellung(&acht, &f, "Acht");
        // A pentagram: the five points are inside, the pentagon in the middle is not.
        let stern: Vec<Pt> = (0..5)
            .map(|i| {
                let w = std::f64::consts::TAU * (2 * i) as f64 / 5.0;
                pt(10.0 * w.sin(), -10.0 * w.cos())
            })
            .collect();
        pruefe_fuellung(&stern, &fuellung(&stern), "Pentagramm");
        // An outline that runs back along a curve it traced: a spike out of a square and back
        // over the same points, then a second lap over part of the square.
        let zurueck = [
            pt(0.0, 0.0),
            pt(4.0, 0.0),
            pt(4.0, 4.0),
            pt(2.0, 4.0),
            pt(2.0, 6.0),
            pt(2.5, 7.0),
            pt(2.0, 6.0),
            pt(2.0, 4.0),
            pt(0.0, 4.0),
            pt(0.0, 0.0),
            pt(4.0, 0.0),
            pt(4.0, 2.0),
            pt(0.0, 2.0),
        ];
        pruefe_fuellung(&zurueck, &fuellung(&zurueck), "zurückgelaufen");
    }

    #[test]
    fn fuellung_jedes_teils_jedes_pruefmusters() {
        // Every piece of every test pattern, among them the bolero sleeves, whose outline runs
        // back along a curve it already traced.
        let mut muster = Vec::new();
        alle_muster(&muster_ordner(), &mut muster);
        assert!(muster.len() >= 10, "{} Muster", muster.len());
        let mut teile = 0;
        for p in &muster {
            let m = Muster::laden(&Ladeauftrag::neu(p)).unwrap();
            for t in &m.teile {
                let was = format!("{}: {}", p.display(), t.name);
                pruefe_fuellung(&t.umriss, &t.fuellung, &was);
                // The label (and the click of the UI test) lies inside the piece.
                assert!(
                    crate::ansicht2d::innen(&t.umriss, t.etikett),
                    "{was}: Etikett {:?}",
                    t.etikett
                );
                teile += 1;
            }
        }
        assert!(teile > 100, "{teile} Teile");
    }

    #[test]
    fn fuellung_der_bluse_hat_die_flaeche_der_teile() {
        // The Bluse3 outlines are simple: the fill has exactly their area.
        let m = Muster::laden(&Ladeauftrag::neu(bluse())).unwrap();
        for t in &m.teile {
            let soll = flaeche(&t.umriss).abs();
            let ist = t.fuellung.flaeche();
            assert!(
                (ist - soll).abs() <= 1e-6 * soll,
                "{}: {ist} statt {soll}",
                t.name
            );
        }
    }

    #[test]
    fn bereinigt_entfernt_doppelte_und_schluss() {
        let p = [
            pt(0.0, 0.0),
            pt(0.0, 0.0),
            pt(1.0, 0.0),
            pt(1.0, 1.0),
            pt(0.0, 0.0),
        ];
        assert_eq!(
            bereinigt(&p),
            vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)]
        );
    }

    #[test]
    fn vorsatz_und_auslage() {
        let namen: Vec<String> = [
            "Bustier - Obercup rechts",
            "Bustier - Obercup links",
            "Bustier - Band",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(&namen[0][vorsatz_laenge(&namen)..], "Obercup rechts");
        assert_eq!(vorsatz_laenge(&["Rock".to_string(), "Bund".to_string()]), 0);

        let h = [
            Huelle {
                x0: 0.0,
                x1: 10.0,
                y0: 0.0,
                y1: 20.0,
            },
            Huelle {
                x0: 5.0,
                x1: 15.0,
                y0: 5.0,
                y1: 15.0,
            },
            Huelle {
                x0: -3.0,
                x1: 3.0,
                y0: -3.0,
                y1: 3.0,
            },
        ];
        let v = auslegen(&h, 1.6);
        let gelegt: Vec<Huelle> = h.iter().zip(&v).map(|(h, v)| h.verschoben(*v)).collect();
        for i in 0..gelegt.len() {
            for j in i + 1..gelegt.len() {
                let (a, b) = (gelegt[i], gelegt[j]);
                let ueber = a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
                assert!(!ueber, "{i} und {j} überlappen");
            }
        }
    }

    #[test]
    fn etikett_liegt_im_bogen() {
        // A U shape: the centroid lies in the gap, the label must not.
        let u = [
            pt(0.0, 0.0),
            pt(3.0, 0.0),
            pt(3.0, 3.0),
            pt(2.0, 3.0),
            pt(2.0, 1.0),
            pt(1.0, 1.0),
            pt(1.0, 3.0),
            pt(0.0, 3.0),
        ];
        let e = etikett(&u);
        // Inside one of the two legs or the bottom bar – checked without app code …
        let im_bein =
            ((e.x > 0.0 && e.x < 1.0) || (e.x > 2.0 && e.x < 3.0)) && e.y > 0.0 && e.y < 3.0;
        let im_steg = e.x > 0.0 && e.x < 3.0 && e.y > 0.0 && e.y < 1.0;
        assert!(im_bein || im_steg, "{e:?}");
        // … and with the hit test.
        assert!(crate::ansicht2d::innen(&u, e), "{e:?}");
    }
}
