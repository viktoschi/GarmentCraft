//! Ein Skelett für den freien MakeHuman-Körper: Gelenke, Hautgewichte, Verformung.
//!
//! Der Körper aus [`crate::koerper_quelle`] bringt seine Gelenke **in derselben
//! Datei** mit: 125 Gruppen `joint-*`, jede ein kleiner Würfel aus acht
//! Punkten, dessen Mitte das Gelenk ist (Befund aus Issue #94). Was fehlt,
//! sind die **Gewichte** — MakeHuman setzt sie erst in der Anwendung. Dieses
//! Modul rechnet sie selbst und verformt damit.
//!
//! ## Die Formel
//!
//! Je Gelenk `g` gibt es zwei **Welt**matrizen `B_g` (Binde-Pose) und `A_g`
//! (aktuelle Pose). Ein Punkt wandert mit
//!
//! ```text
//! p' = Σ_g w_g · (p · B_g⁻¹ · A_g)
//! ```
//!
//! Das ist Linear Blend Skinning in der Zeilenvektor-Konvention
//! ([`mul`]: Punkt links, Translation in `[12..14]`), dieselbe wie in CLOs
//! `.dan`. Summieren sich die Gewichte zu 1, ist `p' = p` bei `A_g = B_g`.
//! Die Binde-Pose ist hier die reine Verschiebung auf den Gelenkort
//! ([`binde`](Skelett::binde_pose)), also geht es im Kern um
//! [`welt_drehung`] — und die Binde-Pose trifft die Punkte exakt, weil
//! `B_g⁻¹ · B_g` die Einheitsmatrix ist.
//!
//! ## Warum die Nähe zum Gelenk **segment** zählt, nicht zum Punkt
//!
//! Ein Knochen ist eine Strecke vom Elterngelenk zu seinem Gelenk; die Haut
//! um ihn herum liegt in einem Schlauch um diese Strecke. Der Abstand zum
//! **Punkt** Oberarmmitte wäre kleiner als der zur Strecke und würde die
//! Schulter breiter ziehen, als sie ist. Gemessen wird deshalb der Abstand
//! zur Strecke ([`knochenabstand`]).
//!
//! ## Warum der Radius je Knochen normiert wird
//!
//! Der reine Abstand in Millimetern bevorzugt dicke Knochen: der Rumpf liegt
//! 130 mm von der Wirbelsäule, die Finger nur 10 mm vom Fingerknochen. Ohne
//! Normierung hätte kein Fingerpunkt je die Hand erreicht. Deshalb bekommt
//! jeder Knochen einen **eigenen Radius** `R` — das 60. Perzentil der
//! Abstände der Punkte, die ihm am nächsten liegen, mindestens 20 mm — und
//! gerechnet wird mit `DN = M / R`.
//!
//! ## Warum der Schultergürtel stehen bleibt
//!
//! Der Slot `l-shoulder` ist die Strecke **Schlüsselbein → Schultergelenk**,
//! also der Gürtel, nicht der Oberarm; der Oberarm ist `l-shoulder` →
//! `l-elbow`. Wer den Gürtel um das Schultergelenk dreht, schwingt sein
//! inneres Ende am Halsansatz mit — und dort saßen alle Risse des ersten
//! Versuchs (0,23 Schultergewicht auf einem Punkt 78 mm über dem Gelenk
//! ergeben 25 mm Weg, auf dem Nachbarn 0: die Kante reißt). Anatomisch hebt
//! der Arm nur den Oberarm und alles darunter; der Gürtel bleibt.
//!
//! Mit stehendem Gürtel, Potenz 8 und 16 Glättungsrunden bleibt der Körper
//! bei 90° Armheben rissfrei: keine einzige weit von der Schulter entfernte
//! Kante wächst über das Doppelte, keine um mehr als 10 mm.
//!
//! ## Wie geglättet wird
//!
//! Die **Auswahl** der vier Knochen je Punkt steht vor dem Glätten fest. Ein
//! Satz, der bei jeder Runde neu gewählt würde, spränge an jeder Knochengrenze
//! auf andere Knochen — genau das soll die Glättung verhindern. Geglättet wird
//! das Gewicht selbst, über die Kantennachbarn, 16 Runden lang mit
//! `w ← 0,5·w + 0,5·Mittel` und danach wieder auf Summe 1.
//!
//! Das Mittel läuft über **alle** Knochen (`np × nk`), nicht über die vier
//! Slots. Dieselbe Slot-Nummer meint bei zwei Nachbarn verschiedene Knochen;
//! ein Mittel über Slots mischt also Knochen, die gar nicht benachbart sind.

use crate::koerper_quelle::GELENK_PREFIX;
use crate::Punkt3;

/// Eine 4×4-Matrix in Zeilenvektor-Ablage: `p · M`, Translation in `[12..14]`.
pub type Mat4 = [f64; 16];

/// Die Einheitsmatrix in dieser Ablage.
pub const EINHEIT: Mat4 = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

// ── Das Rezept der Gewichte ────────────────────────────────────────────
//
// Die vier Zahlen sind gemessen, nicht geraten (Issue #94): der Abstand wird
// mit Potenz 8 gewichtet, so dass der nächste Knochen klar führt; geglättet
// wird 16-mal mit Halbierung, damit an der Achsel kein Sprung bleibt; die
// Auswahl von vier Knochen steht **vor** dem Glätten fest, sonst wandert das
// Gewicht nach jedem Durchgang auf andere Knochen.

/// Höchstens so viele Knochen tragen einen Punkt.
pub const EINFLUESSE: usize = 4;
/// Potenz, mit der `(d_min/d)` gewichtet wird.
pub const POTENZ: f64 = 8.0;
/// Runden der Glättung über die Nachbarpunkte.
pub const RUNDEN: usize = 16;
/// Anteil des Nachbarmittels je Glättungsrunde.
pub const ALPHA: f64 = 0.5;
/// Untergrenze des Knochenradius in mm.
pub const RADIUS_MIN: f64 = 20.0;
/// Perzentil, aus dem der Knochenradius kommt.
pub const PERZENTIL: f64 = 60.0;

/// Ein Gelenk: Name, Elternteil, Binde-Pose, aktuelle Pose.
///
/// Beide Matrizen sind **Welt**matrizen. In der Binde-Pose ist das die reine
/// Verschiebung auf den Gelenkort; die Verformung ersetzt sie durch die
/// gedrehte Fassung ([`Skelett::hebe_arme`]).
#[derive(Debug, Clone)]
pub struct Gelenk {
    pub name: String,
    /// Index des Elternteils; `None` nur an der Wurzel (`ground`).
    pub eltern: Option<usize>,
    /// Der Gelenkort in der Binde-Pose, Millimeter.
    pub ort: Punkt3,
    pub binde: Mat4,
    pub aktuell: Mat4,
}

/// Ein Skelett mit Gewichten und der Liste der Knochen, die sie tragen.
#[derive(Debug, Clone, Default)]
pub struct Skelett {
    /// In **Sprossenreihenfolge**: der Elternteil steht immer vor dem Kind.
    pub gelenke: Vec<Gelenk>,
    /// Indizes der Gelenke, die Gewichte tragen — die „Knochen", je vom
    /// Elternteil zu sich selbst. Die Spalten von [`Self::gewichte`] folgen
    /// dieser Liste, nicht den Gelenknummern.
    pub knochen: Vec<usize>,
    /// Je Körperpunkt: (Knochenindex **als Gelenknummer**, Gewicht).
    pub gewichte: Vec<Vec<(usize, f64)>>,
    /// Je Knochen (in der Reihenfolge von [`Self::knochen`]): sein Radius in
    /// mm, aus dem die Gewichte normiert wurden. Leer, solange
    /// [`Self::gewichte_berechnen`] nicht gelaufen ist.
    pub radien: Vec<f64>,
}

// ── Der Elternbaum ─────────────────────────────────────────────────────

/// Der Elternteil eines Gelenknamens — die Regel, die MakeHumans
/// Gelenknamen ausdrücken.
///
/// `base.obj` trägt keine Hierarchie, nur 125 Namen. Die Regel ist an allen
/// 125 gegen den Baum des Prototyps geprüft (0 Abweichungen). Sie steht hier
/// als Code und nicht als Tabelle, weil sie die Namen *liest*: `finger-3-2`
/// hängt an `finger-3-1`, `toe-1-4` an `toe-1-3` — das sind 40 der 125
/// Gelenke, die eine Tabelle nur wiederholen würde.
pub fn eltern_von(name: &str) -> Result<Option<&'static str>, String> {
    // Die Wurzel und das Becken.
    match name {
        "ground" => return Ok(None),
        "pelvis" => return Ok(Some("ground")),
        _ => {}
    }
    // Die Wirbelsäule aufwärts, dann Hals und Kopf.
    for (kind, eltern) in [
        ("spine-4", "pelvis"),
        ("spine-3", "spine-4"),
        ("spine-2", "spine-3"),
        ("spine-1", "spine-2"),
        ("neck", "spine-1"),
        ("head", "neck"),
        ("head-2", "head"),
        ("jaw", "head"),
        ("mouth", "head"),
    ] {
        if name == kind {
            return Ok(Some(eltern));
        }
    }
    // Die Zunge: Glied 1 am Kopf, danach eine Kette.
    if let Some(g) = name.strip_prefix("tongue-") {
        return Ok(match g {
            "1" => Some("head"),
            "2" => Some("tongue-1"),
            "3" => Some("tongue-2"),
            "4" => Some("tongue-3"),
            _ => return Err(format!("unbekanntes Gelenk {name:?}")),
        });
    }
    // Alles Übrige ist seitenweise gebaut.
    let (seite, rest) = if let Some(r) = name.strip_prefix("l-") {
        ("l", r)
    } else if let Some(r) = name.strip_prefix("r-") {
        ("r", r)
    } else {
        return Err(format!("unbekanntes Gelenk {name:?}"));
    };
    Ok(Some(match rest {
        // Am Rumpf
        "upper-leg" => "pelvis",
        "clavicle" | "scapula" => "spine-1",
        // Das Bein abwärts
        "knee" => match seite {
            "l" => "l-upper-leg",
            _ => "r-upper-leg",
        },
        "ankle" => match seite {
            "l" => "l-knee",
            _ => "r-knee",
        },
        "foot-1" => match seite {
            "l" => "l-ankle",
            _ => "r-ankle",
        },
        "foot-2" => match seite {
            "l" => "l-foot-1",
            _ => "r-foot-1",
        },
        // Der Arm hinaus
        "shoulder" => match seite {
            "l" => "l-clavicle",
            _ => "r-clavicle",
        },
        "elbow" => match seite {
            "l" => "l-shoulder",
            _ => "r-shoulder",
        },
        "hand" => match seite {
            "l" => "l-elbow",
            _ => "r-elbow",
        },
        "hand-2" | "hand-3" => match seite {
            "l" => "l-hand",
            _ => "r-hand",
        },
        // Augen und Lider hängen am Kopf
        "eye" | "eye-target" | "upperlid" | "lowerlid" => "head",
        _ => {
            // Gliederketten: `toe-3-2` -> `toe-3-1`, `finger-2-1` -> `hand`.
            // Das erste Glied hängt am Fuss bzw. an der Hand, jedes weitere am
            // Glied davor — beides steht in der Tabelle in [`elternkette`].
            let (art, nr, glied) =
                gliedkette(rest).ok_or_else(|| format!("unbekanntes Gelenk {name:?}"))?;
            if glied == 1 {
                // Anker der Kette, seitenweise: `l-foot-1` bzw. `l-hand`.
                match (seite, art) {
                    ("l", "toe") => "l-foot-1",
                    ("l", _) => "l-hand",
                    (_, "toe") => "r-foot-1",
                    _ => "r-hand",
                }
            } else {
                elternkette(seite, art, nr, glied - 1)?
                    .ok_or_else(|| format!("unbekanntes Gelenk {name:?}"))?
            }
        }
    }))
}

/// Zerlegt `toe-3-2` in `("toe", "3", 2)` — Art, Nummer und Glied.
///
/// `None` für alles, was keine Kette ist oder außerhalb von 5 Zehen/Fingern
/// und 4 Gliedern liegt. Die Art ist `'static`, weil sie aus der festen Liste
/// kommt; die Nummer bleibt ein Ausschnitt des Namens.
fn gliedkette(rest: &str) -> Option<(&'static str, &str, u32)> {
    for art in ["toe", "finger"] {
        let Some(kette) = rest.strip_prefix(art) else {
            continue;
        };
        let Some(kette) = kette.strip_prefix('-') else {
            continue;
        };
        let mut teile = kette.split('-');
        let (Some(nr), Some(glied), None) = (teile.next(), teile.next(), teile.next()) else {
            return None;
        };
        let glied: u32 = glied.parse().ok()?;
        let nummer: u32 = nr.parse().ok()?;
        if !(1..=5).contains(&nummer) || !(1..=4).contains(&glied) {
            return None;
        }
        return Some((art, nr, glied));
    }
    None
}

/// Der Name des Vorglieds in einer Zehen- oder Fingerkette, als `'static`-Text.
///
/// Die Kette hat höchstens vier Glieder je Zehe und Finger und zwei Seiten,
/// also 2 · 5 · 4 · 2 Namen — die Tabelle ist kürzer als jede Rechnung und
/// hält die Rückgabe frei von geliehenen Namen.
fn elternkette(seite: &str, art: &str, nr: &str, glied: u32) -> Result<Option<&'static str>, String> {
    const TOE: [[[&str; 5]; 5]; 2] = [
        [
            ["l-toe-1-1", "l-toe-1-2", "l-toe-1-3", "l-toe-1-4", ""],
            ["l-toe-2-1", "l-toe-2-2", "l-toe-2-3", "l-toe-2-4", ""],
            ["l-toe-3-1", "l-toe-3-2", "l-toe-3-3", "l-toe-3-4", ""],
            ["l-toe-4-1", "l-toe-4-2", "l-toe-4-3", "l-toe-4-4", ""],
            ["l-toe-5-1", "l-toe-5-2", "l-toe-5-3", "l-toe-5-4", ""],
        ],
        [
            ["r-toe-1-1", "r-toe-1-2", "r-toe-1-3", "r-toe-1-4", ""],
            ["r-toe-2-1", "r-toe-2-2", "r-toe-2-3", "r-toe-2-4", ""],
            ["r-toe-3-1", "r-toe-3-2", "r-toe-3-3", "r-toe-3-4", ""],
            ["r-toe-4-1", "r-toe-4-2", "r-toe-4-3", "r-toe-4-4", ""],
            ["r-toe-5-1", "r-toe-5-2", "r-toe-5-3", "r-toe-5-4", ""],
        ],
    ];
    const FINGER: [[[&str; 5]; 5]; 2] = [
        [
            ["l-finger-1-1", "l-finger-1-2", "l-finger-1-3", "l-finger-1-4", ""],
            ["l-finger-2-1", "l-finger-2-2", "l-finger-2-3", "l-finger-2-4", ""],
            ["l-finger-3-1", "l-finger-3-2", "l-finger-3-3", "l-finger-3-4", ""],
            ["l-finger-4-1", "l-finger-4-2", "l-finger-4-3", "l-finger-4-4", ""],
            ["l-finger-5-1", "l-finger-5-2", "l-finger-5-3", "l-finger-5-4", ""],
        ],
        [
            ["r-finger-1-1", "r-finger-1-2", "r-finger-1-3", "r-finger-1-4", ""],
            ["r-finger-2-1", "r-finger-2-2", "r-finger-2-3", "r-finger-2-4", ""],
            ["r-finger-3-1", "r-finger-3-2", "r-finger-3-3", "r-finger-3-4", ""],
            ["r-finger-4-1", "r-finger-4-2", "r-finger-4-3", "r-finger-4-4", ""],
            ["r-finger-5-1", "r-finger-5-2", "r-finger-5-3", "r-finger-5-4", ""],
        ],
    ];
    let s = if seite == "l" { 0 } else { 1 };
    let n: usize = nr
        .parse()
        .map_err(|_| format!("unbekannte Kette {seite}-{art}-{nr}-{glied}"))?;
    let g = glied as usize;
    if !(1..=5).contains(&n) || !(1..=4).contains(&g) {
        return Err(format!("unbekannte Kette {seite}-{art}-{nr}-{glied}"));
    }
    let t = if art == "toe" { TOE[s][n - 1][g - 1] } else { FINGER[s][n - 1][g - 1] };
    if t.is_empty() {
        return Err(format!("unbekannte Kette {seite}-{art}-{nr}-{glied}"));
    }
    Ok(Some(t))
}

/// Trägt dieser Gelenkname Gewichte?
///
/// Boden, Zunge, Mund, Augen und Lider liegen **im** Körper oder außerhalb
/// seiner Haut; Gewichte auf ihnen würden Punkte an einer Stelle festhalten,
/// an der kein Knochen ist. Das Becken fällt mit heraus, weil sein Elternteil
/// `ground` ist — es hat keinen Knochen über sich.
fn traegt_gewichte(name: &str) -> bool {
    name != "ground"
        && !name.contains("tongue")
        && !name.contains("mouth")
        && !name.contains("eye")
        && !name.contains("lid")
}

// ── Matrizen ───────────────────────────────────────────────────────────

/// `a · b` in Zeilenvektor-Ablage.
pub fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [0.0f64; 16];
    for z in 0..4 {
        for s in 0..4 {
            let mut v = 0.0;
            for k in 0..4 {
                v += a[z * 4 + k] * b[k * 4 + s];
            }
            out[z * 4 + s] = v;
        }
    }
    out
}

/// Bildet einen Punkt mit einer Zeilenvektor-Matrix ab.
pub fn punkt_mal(m: Mat4, p: Punkt3) -> Punkt3 {
    [
        p[0] * m[0] + p[1] * m[4] + p[2] * m[8] + m[12],
        p[0] * m[1] + p[1] * m[5] + p[2] * m[9] + m[13],
        p[0] * m[2] + p[1] * m[6] + p[2] * m[10] + m[14],
    ]
}

/// Die Inverse einer starren Matrix (Drehung + Verschiebung, keine Skalierung).
///
/// ```text
///     [ R  t ]⁻¹   [ Rᵀ  -t Rᵀ ]
///     [ 0  1 ]   = [ 0    1   ]
/// ```
///
/// `None`, wenn die obere 3×3 nicht orthogonal ist — eine skalierte
/// Gelenkmatrix deckt diese Funktion nicht ab und soll nicht stillschweigend
/// falsch rechnen.
pub fn starr_invers(m: Mat4) -> Option<Mat4> {
    let r = |z: usize, c: usize| m[z * 4 + c];
    for z in 0..3 {
        for c in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += r(z, k) * r(c, k);
            }
            let soll = if z == c { 1.0 } else { 0.0 };
            if (s - soll).abs() > 1e-9 {
                return None;
            }
        }
    }
    let mut out = EINHEIT;
    for z in 0..3 {
        for c in 0..3 {
            out[z * 4 + c] = r(c, z);
        }
    }
    let t = [m[12], m[13], m[14]];
    for c in 0..3 {
        out[12 + c] = -(t[0] * r(c, 0) + t[1] * r(c, 1) + t[2] * r(c, 2));
    }
    Some(out)
}

/// Eine starre Verschiebung um `v`.
pub fn verschiebung(v: Punkt3) -> Mat4 {
    let mut m = EINHEIT;
    m[12] = v[0];
    m[13] = v[1];
    m[14] = v[2];
    m
}

/// Eine Drehmatrix um eine beliebige Achse (Rodrigues), Winkel in Grad.
pub fn drehmatrix(achse: Punkt3, grad: f64) -> Mat4 {
    let l = (achse[0] * achse[0] + achse[1] * achse[1] + achse[2] * achse[2]).sqrt();
    if l < 1e-12 {
        return EINHEIT;
    }
    let (x, y, z) = (achse[0] / l, achse[1] / l, achse[2] / l);
    let (s, c) = grad.to_radians().sin_cos();
    let t = 1.0 - c;
    [
        t * x * x + c,
        t * x * y + s * z,
        t * x * z - s * y,
        0.0, //
        t * x * y - s * z,
        t * y * y + c,
        t * y * z + s * x,
        0.0, //
        t * x * z + s * y,
        t * y * z - s * x,
        t * z * z + c,
        0.0, //
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// Eine Drehung um eine Weltachse durch einen Weltpunkt, als rechter Faktor:
/// `M · welt_drehung(…)` dreht `M` um diese Achse an diesem Punkt.
///
/// Zeilenvektor-Ablage heißt: erst `T₋p`, dann die Drehung, dann `T₊p`; in
/// der Multiplikation steht das von links nach rechts —
/// `T₋p · R · T₊p`, und das hängt **rechts** an `M`.
pub fn welt_drehung(achse: Punkt3, punkt: Punkt3, grad: f64) -> Mat4 {
    let r = drehmatrix(achse, grad);
    mul(mul(verschiebung([-punkt[0], -punkt[1], -punkt[2]]), r), verschiebung(punkt))
}

/// Der kürzeste Abstand eines Punktes zur Strecke `a`–`e`.
///
/// Die Enden zählen mit (der Parameter wird auf `[0, 1]` geklemmt): ein
/// Punkt *über* dem Gelenk ist dem Knochen so nah wie das Ende, nicht wie die
/// Verlängerung der Strecke. Genau das hält die Schulter beim Heben zusammen.
pub fn knochenabstand(p: Punkt3, a: Punkt3, e: Punkt3) -> f64 {
    let d = [e[0] - a[0], e[1] - a[1], e[2] - a[2]];
    let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if l2 < 1e-12 {
        let v = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
        return (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    }
    let w = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let t = ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / l2).clamp(0.0, 1.0);
    let nah = [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]];
    let v = [p[0] - nah[0], p[1] - nah[1], p[2] - nah[2]];
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Das `p`-Perzentil wie `numpy.percentile(v, p)` mit linearer Interpolation.
///
/// `v` wird nicht verändert; sortiert wird eine Kopie.
pub fn perzentil(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if s.len() == 1 {
        return s[0];
    }
    let rang = (p / 100.0).clamp(0.0, 1.0) * (s.len() - 1) as f64;
    let lo = rang.floor() as usize;
    let hi = (lo + 1).min(s.len() - 1);
    let t = rang - lo as f64;
    s[lo] + (s[hi] - s[lo]) * t
}

// ── Das Skelett ────────────────────────────────────────────────────────

impl Skelett {
    /// Baut den Baum aus den Gelenken einer MakeHuman-Datei.
    ///
    /// `gelenke` sind Name und Ort in der Reihenfolge der Datei. Die
    /// Sprossenreihenfolge entsteht aus der Tiefe (Abstand zur Wurzel),
    /// **stabil** über die Dateireihenfolge — dieselbe Ordnung, die
    /// MakeHuman beim Schreiben der Gruppen benutzt; sie ist damit ohne
    /// weitere Annahme reproduzierbar und für den Elternteil-im-Vorfeld-Test
    /// geeignet.
    ///
    /// Gewichte sind danach leer; sie kommen aus [`Self::gewichte_berechnen`].
    pub fn aus_gelenken(gelenke: &[(String, Punkt3)]) -> Result<Skelett, String> {
        // Elternteil je Name, zunächst als Name.
        let mut eltern_name: Vec<Option<String>> = Vec::with_capacity(gelenke.len());
        for (name, _) in gelenke {
            eltern_name.push(eltern_von(name)?.map(str::to_string));
        }
        // Tiefe über die Namenskette; Zyklen sind über die Tabelle der Regel
        // ausgeschlossen, ein Zähler begrenzt die Suche trotzdem.
        let mut tiefe = vec![0usize; gelenke.len()];
        let pos: std::collections::HashMap<&str, usize> = gelenke
            .iter()
            .enumerate()
            .map(|(i, (n, _))| (n.as_str(), i))
            .collect();
        for (i, (name, _)) in gelenke.iter().enumerate() {
            let mut t = 0usize;
            let mut zeiger = eltern_name[i].as_deref();
            while let Some(e) = zeiger {
                let Some(&j) = pos.get(e) else {
                    return Err(format!(
                        "Gelenk {name:?}: Elternteil {e:?} fehlt in der Datei"
                    ));
                };
                t += 1;
                if t > gelenke.len() {
                    return Err(format!("Gelenk {name:?}: Kreislauf im Baum"));
                }
                zeiger = eltern_name[j].as_deref();
            }
            tiefe[i] = t;
        }
        let mut reihe: Vec<usize> = (0..gelenke.len()).collect();
        reihe.sort_by_key(|&i| tiefe[i]); // stabil: gleiche Tiefe bleibt in Dateireihenfolge

        let mut skelett = Skelett::default();
        let mut neu_von_alt = vec![usize::MAX; gelenke.len()];
        for &alt in &reihe {
            let (name, ort) = &gelenke[alt];
            let eltern = match &eltern_name[alt] {
                Some(e) => {
                    let j = *pos
                        .get(e.as_str())
                        .ok_or_else(|| format!("Gelenk {name:?}: Elternteil {e:?} fehlt"))?;
                    Some(neu_von_alt[j])
                }
                None => None,
            };
            if let Some(e) = eltern {
                if e == usize::MAX || e >= skelett.gelenke.len() {
                    return Err(format!("Gelenk {name:?}: Elternteil nicht vor dem Kind"));
                }
            }
            neu_von_alt[alt] = skelett.gelenke.len();
            skelett.gelenke.push(Gelenk {
                name: name.clone(),
                eltern,
                ort: *ort,
                binde: verschiebung(*ort),
                aktuell: verschiebung(*ort),
            });
        }

        // Die Knochen: Gelenke, die Gewichte tragen, mit aktivem Elternteil.
        skelett.knochen = skelett
            .gelenke
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                traegt_gewichte(&g.name)
                    && g.eltern
                        .map(|e| traegt_gewichte(&skelett.gelenke[e].name))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        if skelett.knochen.is_empty() {
            return Err("kein einziger Knochen trägt Gewichte".to_string());
        }
        Ok(skelett)
    }

    /// Sucht ein Gelenk am Namen.
    pub fn finde(&self, name: &str) -> Option<usize> {
        self.gelenke.iter().position(|g| g.name == name)
    }

    /// Alle Nachfahren eines Gelenks, es selbst zuerst (Sprossenreihenfolge).
    pub fn nachfahren(&self, index: usize) -> Vec<usize> {
        let mut out = vec![index];
        let mut i = 0;
        while i < out.len() {
            let e = out[i];
            for (k, g) in self.gelenke.iter().enumerate() {
                if g.eltern == Some(e) {
                    out.push(k);
                }
            }
            i += 1;
        }
        out.sort_unstable();
        out
    }

    /// Die Binde-Pose als Weltmatrizen.
    pub fn binde_pose(&self) -> Vec<Mat4> {
        self.gelenke.iter().map(|g| g.binde).collect()
    }

    /// Die aktuelle Pose als Weltmatrizen.
    pub fn aktuelle_pose(&self) -> Vec<Mat4> {
        self.gelenke.iter().map(|g| g.aktuell).collect()
    }

    /// Setzt alle Gelenke auf die Binde-Pose zurück.
    pub fn zuruecksetzen(&mut self) {
        for g in &mut self.gelenke {
            g.aktuell = g.binde;
        }
    }

    /// Hebt beide Arme um `grad` seitlich (90 = waagrecht nach außen).
    ///
    /// Die Achse ist die Sagittalachse (`z` zeigt nach vorn), das Scharnier
    /// sitzt im Schultergelenk. Die Seite mit `x > 0` dreht positiv, die
    /// andere gespiegelt — bei gleichem Vorzeichen schlüge ein Arm über den
    /// Körper.
    ///
    /// Gedreht wird **der Oberarm und alles darunter**, nicht der Slot
    /// `l-shoulder` selbst: der ist der Schultergürtel
    /// (Schlüsselbein → Gelenk), und sein inneres Ende sitzt am Halsansatz.
    /// Siehe den Modulkopf.
    pub fn hebe_arme(&mut self, grad: f64) {
        self.zuruecksetzen();
        if grad == 0.0 {
            return;
        }
        let mut aenderungen = Vec::new();
        for seite in ["l", "r"] {
            let Some(i) = self.finde(&format!("{seite}-shoulder")) else {
                continue;
            };
            let p = self.gelenke[i].ort;
            let vz = if p[0] > 0.0 { 1.0 } else { -1.0 };
            let versatz = welt_drehung([0.0, 0.0, vz], p, grad);
            for k in self.nachfahren(i) {
                if k != i {
                    aenderungen.push((k, versatz));
                }
            }
        }
        for (k, versatz) in aenderungen {
            self.gelenke[k].aktuell = mul(self.gelenke[k].binde, versatz);
        }
    }

    /// Verformt Punkte mit der aktuellen Pose.
    ///
    /// `p' = Σ w_g · (p · B_g⁻¹ · A_g)`. `None`, wenn eine Binde-Matrix nicht
    /// starr ist — dann lieber keine Verformung als eine gerissene.
    pub fn verforme(&self, punkte: &[Punkt3]) -> Option<Vec<Punkt3>> {
        if self.gewichte.len() != punkte.len() {
            return None;
        }
        let mut tf = Vec::with_capacity(self.gelenke.len());
        for g in &self.gelenke {
            tf.push(mul(starr_invers(g.binde)?, g.aktuell));
        }
        let mut out = Vec::with_capacity(punkte.len());
        for (i, p) in punkte.iter().enumerate() {
            let mut q = [0.0f64; 3];
            let mut summe = 0.0;
            for &(g, w) in &self.gewichte[i] {
                let r = punkt_mal(tf[g], *p);
                q[0] += w * r[0];
                q[1] += w * r[1];
                q[2] += w * r[2];
                summe += w;
            }
            if summe.abs() < 1e-12 {
                out.push(*p);
            } else {
                out.push([q[0] / summe, q[1] / summe, q[2] / summe]);
            }
        }
        Some(out)
    }

    /// Die grösste Abweichung der Gewichtssumme von 1,0.
    pub fn gewichtssumme_abweichung(&self) -> f64 {
        self.gewichte
            .iter()
            .filter(|gw| !gw.is_empty())
            .map(|gw| (gw.iter().map(|&(_, w)| w).sum::<f64>() - 1.0).abs())
            .fold(0.0, f64::max)
    }

    /// Die grösste Zahl von Knochen an einem Punkt.
    pub fn einfluesse_max(&self) -> usize {
        self.gewichte.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// Der Radius eines Knochens in mm, so wie ihn die Gewichte benutzt haben.
    pub fn radius(&self, gelenk: &str) -> Option<f64> {
        let g = self.finde(gelenk)?;
        let j = self.knochen.iter().position(|&k| k == g)?;
        self.radien.get(j).copied()
    }

    /// Wo das Gelenk in der **aktuellen** Pose steht.
    ///
    /// Die Binde-Pose hat den Ort schon als Feld; nach [`Self::hebe_arme`] steht
    /// er aber woanders. Der Ort ist die Translation der Weltmatrix, also die
    /// letzte Zeile (Zeilenvektor-Konvention).
    pub fn aktuell_ort(&self, index: usize) -> Punkt3 {
        let m = self.gelenke[index].aktuell;
        [m[12], m[13], m[14]]
    }

    /// Die Abnahme der Stufe 7: Armheben um 90° gegen den unverformten Körper.
    ///
    /// Gemessen wird an den **Kanten** des Netzes, in zwei Gruppen getrennt:
    ///
    /// * `fern` — Kantenmitte mindestens 120 mm von beiden Schultergelenken
    ///   entfernt. Dort darf sich die Länge nicht mehr als verdoppeln
    ///   (Verhältnis ≤ 2) und um höchstens 10 mm wachsen; sonst ist das Netz
    ///   gerissen.
    /// * `nah` — alles im Umkreis der Schulter; dort *soll* sich die Länge
    ///   ändern, nur nicht zusammenfallen (Verhältnis > 0,2).
    ///
    /// Dazu die Schulter selbst: Punkte 20–70 mm vom linken Schultergelenk
    /// sollen ihren Abstand zu ihm etwa halten (Median des Verhältnisses nahe
    /// 1); die Binde-Pose muss den Körper unverändert lassen.
    pub fn abnahme(&self, punkte: &[Punkt3], dreiecke: &[[usize; 3]], grad: f64) -> Option<Abnahme> {
        // Der Körper, wie er in der Binde-Pose steht.
        let binde = {
            let mut s = self.clone();
            s.zuruecksetzen();
            s.verforme(punkte)?
        };
        let mut s = self.clone();
        s.hebe_arme(grad);
        let neu = s.verforme(punkte)?;

        let kanten = randkanten(punkte.len(), dreiecke);
        let schultern: Vec<Punkt3> = ["l-shoulder", "r-shoulder"]
            .iter()
            .filter_map(|n| self.finde(n).map(|i| self.gelenke[i].ort))
            .collect();

        // Die Minima beginnen bei unendlich; `Default` gibt hier 0 und würde
        // jedes `.min()` verschlucken.
        let mut ab = Abnahme {
            nah_min: f64::INFINITY,
            schulter_min: f64::INFINITY,
            ..Abnahme::default()
        };
        for &(x, y) in &kanten {
            let l0 = abstand(binde[x], binde[y]);
            let l1 = abstand(neu[x], neu[y]);
            if l0 < 1e-9 {
                continue;
            }
            let r = l1 / l0;
            let zuwachs = l1 - l0;
            let mitte = [
                0.5 * (binde[x][0] + binde[y][0]),
                0.5 * (binde[x][1] + binde[y][1]),
                0.5 * (binde[x][2] + binde[y][2]),
            ];
            let fern = schultern
                .iter()
                .map(|p| abstand(mitte, *p))
                .fold(f64::INFINITY, f64::min)
                >= 120.0;
            if fern {
                ab.fern_kanten += 1;
                ab.fern_max = ab.fern_max.max(r);
                if r > 2.0 {
                    ab.fern_ueber_doppelt += 1;
                }
                if zuwachs > 10.0 {
                    ab.fern_ueber_10mm += 1;
                }
            } else {
                ab.nah_kanten += 1;
                ab.nah_max = ab.nah_max.max(r);
                ab.nah_min = ab.nah_min.min(r);
                if r < 0.2 {
                    ab.nah_kollaps += 1;
                }
            }
        }

        // Die Schulter: hält sie ihren Abstand zum Gelenk?
        if let Some(i) = self.finde("l-shoulder") {
            let g = self.gelenke[i].ort;
            let mut q = Vec::new();
            for (k, _) in punkte.iter().enumerate() {
                let d0 = abstand(binde[k], g);
                if d0 > 20.0 && d0 < 70.0 {
                    q.push(abstand(neu[k], g) / d0);
                }
            }
            if !q.is_empty() {
                ab.schulter_median = perzentil(&q, 50.0);
                ab.schulter_min = q.iter().copied().fold(f64::INFINITY, f64::min);
            }
        }
        ab.binde_abweichung = binde
            .iter()
            .zip(punkte)
            .map(|(a, b)| abstand(*a, *b))
            .fold(0.0, f64::max);
        ab.gewichtssumme = self.gewichtssumme_abweichung();
        ab.einfluesse = self.einfluesse_max();
        ab.punkte_ohne_gewicht = self.gewichte.iter().filter(|g| g.is_empty()).count();
        Some(ab)
    }
}

/// Das Ergebnis von [`Skelett::abnahme`].
#[derive(Debug, Clone, Default)]
pub struct Abnahme {
    /// Kanten weiter als 120 mm von beiden Schultern.
    pub fern_kanten: usize,
    /// Davon mit Längenverhältnis über 2.
    pub fern_ueber_doppelt: usize,
    /// Davon um mehr als 10 mm gewachsen.
    pub fern_ueber_10mm: usize,
    /// Grösstes Längenverhältnis unter den fernen Kanten.
    pub fern_max: f64,
    /// Kanten im Umkreis der Schulter.
    pub nah_kanten: usize,
    /// Grösstes bzw. kleinstes Längenverhältnis dort.
    pub nah_max: f64,
    pub nah_min: f64,
    /// Nahe Kanten, die auf unter ein Fünftel zusammengefallen sind — die
    /// Achselfalte, in der die Haut sich zumacht.
    pub nah_kollaps: usize,
    /// Median und Minimum des Abstandsverhältnisses der Schulterpunkte.
    pub schulter_median: f64,
    pub schulter_min: f64,
    /// Grösster Weg eines Punktes in der Binde-Pose (soll 0 sein).
    pub binde_abweichung: f64,
    /// Grösste Abweichung der Gewichtssumme von 1.
    pub gewichtssumme: f64,
    /// Grösste Zahl von Knochen an einem Punkt.
    pub einfluesse: usize,
    /// Punkte ganz ohne Gewicht.
    pub punkte_ohne_gewicht: usize,
}

impl Abnahme {
    /// Ob die Stufe-7-Abnahme bestanden ist.
    ///
    /// Die Bedingungen stehen in [`Skelett::abnahme`]; sie sind hier
    /// zusammengefasst, damit CLI und Test nicht zwei Wahrheiten haben.
    pub fn bestanden(&self) -> bool {
        self.fern_ueber_doppelt == 0
            && self.fern_ueber_10mm == 0
            && self.binde_abweichung < 1e-6
            && self.gewichtssumme <= 1e-15
            && self.einfluesse <= EINFLUESSE
            && self.punkte_ohne_gewicht == 0
    }
}

/// Der Abstand zweier Punkte.
fn abstand(a: Punkt3, b: Punkt3) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

impl Skelett {
    /// Rechnet die Gewichte für ein Netz.
    ///
    /// Abstand jedes Punktes zu jeder Knochenstrecke, normiert mit dem
    /// **eigenen Radius** des Knochens (60. Perzentil der Abstände der ihm
    /// nächsten Punkte, mindestens 20 mm), dann `(d_min/d)^8` auf die vier
    /// nächsten, 16 Runden über die Nachbarn geglättet und auf 1,0 gebracht.
    /// Das Rezept steht mit seinen Zahlen im Modulkopf; die Auswahl der vier
    /// Knochen steht **vor** dem Glätten fest, damit das Gewicht nicht nach
    /// jeder Runde auf andere Knochen wandert.
    pub fn gewichte_berechnen(&mut self, punkte: &[Punkt3], dreiecke: &[[usize; 3]]) {
        let nk = self.knochen.len();
        let np = punkte.len();
        // ── Knochen als Strecken ──
        let mut a = Vec::with_capacity(nk);
        let mut e = Vec::with_capacity(nk);
        for &g in &self.knochen {
            let eltern = self.gelenke[g].eltern.expect("Knochen hat einen Elternteil");
            a.push(self.gelenke[eltern].ort);
            e.push(self.gelenke[g].ort);
        }
        // ── Abstände, Revier, Radius ──
        let mut m = vec![0.0f64; np * nk];
        let mut revier = vec![0usize; np];
        for i in 0..np {
            let p = punkte[i];
            let mut best = (f64::INFINITY, 0usize);
            for j in 0..nk {
                let d = knochenabstand(p, a[j], e[j]);
                m[i * nk + j] = d;
                if d < best.0 {
                    best = (d, j);
                }
            }
            revier[i] = best.1;
        }
        let mut radius = vec![RADIUS_MIN; nk];
        let mut spalte = Vec::with_capacity(np);
        for j in 0..nk {
            spalte.clear();
            spalte.extend(
                (0..np)
                    .filter(|&i| revier[i] == j)
                    .map(|i| m[i * nk + j]),
            );
            let r = match spalte.len() {
                0 => 0.0,
                1 | 2 => spalte.iter().copied().fold(0.0, f64::max),
                _ => perzentil(&spalte, PERZENTIL),
            };
            radius[j] = r.max(RADIUS_MIN);
        }
        self.radien = radius.clone();
        // ── Auswahl und Potenz ──
        //
        // Die vier Knochen je Punkt stehen ab hier **fest** (`z`); geglättet
        // wird nur noch ihre Gewichte. Ein wandernder Satz würde an jeder
        // Knochengrenze springen, und genau das soll die Glättung verhindern.
        let mut z = vec![false; np * nk];
        let mut w = vec![0.0f64; np * nk];
        let mut reihenfolge: Vec<(f64, usize)> = Vec::with_capacity(nk);
        for i in 0..np {
            reihenfolge.clear();
            for j in 0..nk {
                reihenfolge.push((m[i * nk + j] / radius[j], j));
            }
            // stabil nach Abstand: bei Gleichstand gewinnt der frühere Knochen
            reihenfolge
                .sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            let dmin = reihenfolge[0].0;
            let mut summe = 0.0;
            for &(_, j) in reihenfolge.iter().take(EINFLUESSE) {
                z[i * nk + j] = true;
                // Der Exponent läuft über die **eigene** Distanz des Knochens,
                // nicht über dmin - sonst wäre jeder Knochen gleich.
                w[i * nk + j] = (dmin / (m[i * nk + j] / radius[j]).max(1e-9)).powf(POTENZ);
                summe += w[i * nk + j];
            }
            if summe > 0.0 {
                for j in 0..nk {
                    w[i * nk + j] /= summe;
                }
            } else if !reihenfolge.is_empty() {
                // Alle vier auf demselben Abstand (auf dem Knochen selbst):
                // gleiche Anteile, damit die Summe 1 bleibt.
                let mut beteiligt = 0;
                for j in 0..nk {
                    if z[i * nk + j] {
                        w[i * nk + j] = 1.0;
                        beteiligt += 1;
                    }
                }
                if beteiligt > 0 {
                    for j in 0..nk {
                        w[i * nk + j] /= beteiligt as f64;
                    }
                }
            }
        }
        let w0 = w.clone();
        // ── Glättung über die Nachbarn ──
        //
        // Voll über alle Knochen, wie die Vorlage: `mittel[i,j]` sammelt die
        // Gewichte des Knochens `j` an allen Nachbarn von `i`. Ein Mittel über
        // die vier *Slots* wäre etwas anderes — dieselbe Slot-Nummer meint bei
        // zwei Nachbarn verschiedene Knochen — und würde Knochen mischen, die
        // gar nicht benachbart sind.
        let kanten = randkanten(np, dreiecke);
        let mut grad = vec![0.0f64; np];
        for &(x, y) in &kanten {
            grad[x] += 1.0;
            grad[y] += 1.0;
        }
        let mut mittel = vec![0.0f64; np * nk];
        for _ in 0..RUNDEN {
            mittel.fill(0.0);
            for &(x, y) in &kanten {
                let (ix, iy) = (x * nk, y * nk);
                for j in 0..nk {
                    mittel[ix + j] += w[iy + j];
                    mittel[iy + j] += w[ix + j];
                }
            }
            for i in 0..np {
                let teiler = if grad[i] > 1.0 { grad[i] } else { 1.0 };
                let mut summe = 0.0;
                for j in 0..nk {
                    let k = i * nk + j;
                    if !z[k] {
                        w[k] = 0.0;
                        continue;
                    }
                    w[k] = (1.0 - ALPHA) * w[k] + ALPHA * (mittel[k] / teiler);
                    summe += w[k];
                }
                if summe > 0.0 {
                    for j in 0..nk {
                        w[i * nk + j] /= summe;
                    }
                } else {
                    // Punkt ohne Nachbarn oder alles weggeschnitten: der
                    // Ausgangsstand gilt.
                    w[i * nk..i * nk + nk].copy_from_slice(&w0[i * nk..i * nk + nk]);
                }
            }
        }
        // ── In die Punktliste, ohne Nullen ──
        self.gewichte = (0..np)
            .map(|i| {
                (0..nk)
                    .filter(|&j| w[i * nk + j] > 0.0)
                    .map(|j| (self.knochen[j], w[i * nk + j]))
                    .collect()
            })
            .collect();
    }
}

/// Die eindeutigen Kanten eines Dreiecksnetzes, jede einmal, klein nach groß.
fn randkanten(np: usize, dreiecke: &[[usize; 3]]) -> Vec<(usize, usize)> {
    let mut k = Vec::with_capacity(dreiecke.len() * 3);
    for d in dreiecke {
        for (x, y) in [(d[0], d[1]), (d[1], d[2]), (d[2], d[0])] {
            if x < np && y < np && x != y {
                k.push(if x < y { (x, y) } else { (y, x) });
            }
        }
    }
    k.sort_unstable();
    k.dedup();
    k
}

/// Der Name des Gelenks ohne den Gruppen-Vorsatz — für Leser der Datei.
pub fn gelenkname(gruppe: &str) -> Option<&str> {
    gruppe.strip_prefix(GELENK_PREFIX)
}

/// Körper und Skelett in einem Zug: MakeHuman lesen, Gelenkbaum bauen,
/// Gewichte rechnen.
///
/// `hoehe_mm` ist die Zielgröße des Körpers. Das Ergebnis steht in derselben
/// Reihenfolge wie [`crate::koerper_quelle::makehuman`], und die Gewichte
/// gehören zu genau dieser Punktliste — beides zusammen ist die Eingabe von
/// [`Skelett::verforme`].
pub fn baue(
    ordner: &std::path::Path,
    hoehe_mm: f64,
) -> Result<(crate::Netz, Skelett), String> {
    let (netz, gelenke) = crate::koerper_quelle::makehuman_mit_gelenken(ordner, hoehe_mm)?;
    let mut skelett = Skelett::aus_gelenken(&gelenke)?;
    skelett.gewichte_berechnen(&netz.punkte, &netz.dreiecke);
    Ok((netz, skelett))
}

#[cfg(test)]
mod pruefungen {
    use super::*;

    #[test]
    fn elternregel_ist_zyklenfrei_und_endet_bei_ground() {
        // Alle 125 Namen der Datei, aus der Meldung des Lesers.
        let namen: Vec<String> = crate::koerper_quelle::ordner()
            .map(|o| {
                crate::koerper_quelle::Rohteil::lies(
                    &o.join(crate::koerper_quelle::BASIS),
                )
                .map(|r| {
                    r.gruppen
                        .iter()
                        .filter_map(|(g, _)| gelenkname(g).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
            })
            .unwrap_or_default();
        if namen.is_empty() {
            return;
        }
        assert_eq!(namen.len(), 125, "Gelenkgruppen");
        for n in &namen {
            let mut t = 0;
            let mut zeiger = n.as_str();
            while let Some(e) = eltern_von(zeiger).expect("Regel kennt den Namen") {
                assert_ne!(e, "", "leerer Elternteil bei {zeiger}");
                t += 1;
                // Die längste echte Kette: Finger-4 -> Finger-3 -> Finger-2 ->
                // Finger-1 -> Hand -> Ellbogen -> Schulter -> Schlüsselbein ->
                // spine-1..4 -> Becken -> Boden, also 14 Glieder.
                assert!(t < 20, "Kette zu lang bei {n}");
                assert!(namen.iter().any(|x| x == e), "{e} fehlt als Gelenk");
                zeiger = e;
            }
        }
    }

    #[test]
    fn perzentil_wie_numpy() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((perzentil(&v, 60.0) - 3.4).abs() < 1e-12);
        assert!((perzentil(&v, 0.0) - 1.0).abs() < 1e-12);
        assert!((perzentil(&v, 100.0) - 5.0).abs() < 1e-12);
        assert!((perzentil(&[7.0], 60.0) - 7.0).abs() < 1e-12);
    }

    #[test]
    fn knochenabstand_klemmt_an_den_enden() {
        let a = [0.0, 0.0, 0.0];
        let e = [10.0, 0.0, 0.0];
        assert!((knochenabstand([5.0, 3.0, 0.0], a, e) - 3.0).abs() < 1e-12);
        // über das Ende hinaus: der Abstand wächst mit dem Überstand
        assert!((knochenabstand([13.0, 4.0, 0.0], a, e) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn drehung_um_einen_punkt_laesst_ihn_stehen() {
        let p = [155.0, 1271.0, -101.0];
        let d = welt_drehung([0.0, 0.0, 1.0], p, 90.0);
        let q = punkt_mal(d, p);
        for a in 0..3 {
            assert!((q[a] - p[a]).abs() < 1e-9, "Achse {a}: {q:?} gegen {p:?}");
        }
        // und um 90° dreht sie +x auf +y
        let r = punkt_mal(d, [p[0] + 100.0, p[1], p[2]]);
        assert!((r[1] - (p[1] + 100.0)).abs() < 1e-9, "{r:?}");
    }
}
