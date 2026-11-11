//! CLO-Projekte (`.zprj`) lesen (ADR-022).
//!
//! Wunsch der Anwenderin am 27.10.2026: "es wäre aber schön wenn die clo3d
//! projekte kompatibel sind. nimm das als anforderung mit auf". Damit ein
//! CLO-Projekt ohne handgeschriebene Naht- und Lagedatei angezogen werden kann,
//! liest klon3d daraus **Teile, Nähte, Stoffe und Avatar**.
//!
//! Gelesen wird nur. `klon3d dxf` bleibt der Weg nach CLO.
//!
//! ## Der Aufbau, geprüft an `japanese_school_uniform.zprj`
//!
//! ```text
//! japanese_school_uniform.zprj        ZIP, 16 Einträge
//!  ├─ *.xml, *.json, *.png, *.jpg     Beiwerk
//!  ├─ *.avt       13,7 MB             Avatar
//!  ├─ *.smp         1,7 KB            Simulationseinstellungen (Property-Stream)
//!  └─ *.zpac        5,7 MB            ZIP im ZIP: der eigentliche Inhalt
//!      ├─ *.pos     158 KB            Lage (Property-Stream mit Kopf)
//!      └─ *.pac    17,9 MB            Mustersatz (Property-Stream ohne Kopf)
//! ```
//!
//! Die Kette ist bei jedem Schritt **auf das Byte** belegt: der ZIP-Leser
//! landet auf dem zentralen Verzeichnis, [`property::hat_kopf`] prüft
//! `u32[0x20] + u32[0x24] == Dateigröße`, und der Property-Leser landet am
//! Dateiende (`japanese_school_uniform.pac` 17 886 470 Byte, 0 Byte Rest).
//!
//! ## Wo die Teile stehen
//!
//! In der `.pac` unter `mapPatternEditor` → `mapPatternList` → `listPattern`.
//! Befund 62 hatte diesen Pfad an den Bibliothekssätzen als
//! `mapPatternBlockManager`/`listPatternBlock` beschrieben; in einem echten
//! Projekt der Referenzfassung heißt er `mapPatternList`/`listPattern`. Beides kommt
//! vor, deshalb sucht der Leser nach dem **Feld**, nicht nach dem Pfad.
//!
//! Je Teil:
//!
//! | Feld | Inhalt |
//! |---|---|
//! | `mapElement.qsName`, `.uiID` | Name und Kennung des Teils |
//! | `mapShape2D.listPoint[].v2Position` | Konturpunkte, **Millimeter** |
//! | `mapShape2D.listLine[]` | Linien zwischen Punktindizes, mit Netzpunkten |
//! | `mapFabricShape2D.iFabricIndex` | Stoff des Teils (`listFabric`-Index) |
//! | `mapFabricShape2D.mapMesh2D` | 2D-Netz: Knoten- und Dreieckszahl |
//! | `listInnerShape` | Innenlinien (Abnäher, Falten) mit eigenen Punkten |
//!
//! **Wichtig:** Die Stoffzuordnung steht **nicht** in
//! `mapShape2D.iFabricIndex` - dort steht bei allen 31 Teilen `-1`. Der echte
//! Index ist `mapFabricShape2D.iFabricIndex` (geprüft: 6/18/2/5, Summe 31).
//!
//! ## Wo die Nähte stehen
//!
//! Unter `mapSeamLinePairGroup` → `listSeamLinePairGroup[]` → `mapSeamLinePairList.listSeamLinePair[]`
//! (143 Gruppen, je ein Paar). Eine Naht hat zwei Seiten
//! (`mapSeamLine0`/`mapSeamLine1`), jede mit
//!
//! * `mapParamLine`: Teil (`uiSelfIndex`), erste und letzte Linie
//!   (`uiStartIndex`, `uiEndIndex`), Anteile (`fRatioStart`, `fRatioEnd`),
//!   Laufrichtung (`bDirection`) und **`uiDepth`**,
//! * `arrMeshPointIndex`: die Netzpunkte der Naht, **lokal zum Teil**.
//!
//! ## `uiDepth`: Rand oder Innenlinie
//!
//! Das ist der Punkt, an dem eine naive Deutung scheitert - und der Grund,
//! warum hier 286 von 286 Nahtseiten aufgehen:
//!
//! | `uiDepth` | `uiStartIndex` zählt in | Seiten |
//! |---|---|---|
//! | 1 | `mapShape2D.listLine` (Teilrand) | 240 |
//! | 2 | `listInnerShape[*].listLine` (Innenlinien) | 46 |
//!
//! Ohne diese Unterscheidung liegen 46 Seiten scheinbar „neben" dem Rand
//! (3-7 mm innen) - es sind Nähte auf Abnähern und Falten.
//!
//! `arrMeshPointIndex` ist **lokal** zum Teil aus `uiSelfIndex`, nicht global:
//! die Indizes bleiben bei allen 286 Seiten unter `uiVertexCount2D` des Teils.
//! Eine globale Deutung über die kumulierte Knotenzahl hält nur bei 5 Seiten.
//!
//! ## Einheiten
//!
//! Millimeter. Beleg: `fEdgeLength` = 10 (CLOs Partikelabstand in mm), die
//! Bounding-Box der 31 Teile ist 1 230 × 1 024 mm, und die Summe der
//! `uiVertexCount2D` ist 88 718 Knoten.
//!
//! ## Grenzen
//!
//! * **Gelesen, nicht zurückgeschrieben.**
//! * **`baRest` nicht gedeutet.** Das 2D-Netz (`baRest` = 2 × f32 je Knoten,
//!   `baTri` = 3 × u32 je Dreieck) liegt in einem eigenen Bezugssystem: eine
//!   Ähnlichkeitstransformation in die Konturebene lässt bei 5 von 12
//!   geprüften Teilen über 100 mm Rest. Für die Nähte wird es nicht gebraucht,
//!   weil sie über Liniennummern und lokale Netzpunkte beschrieben sind.
//! * **Kurven als Polylinie.** `listLine[].listPoint` trägt nur die
//!   **Zwischenstützpunkte** (Anfang und Ende stehen in `listPoint` des
//!   Teils); sie werden übernommen, aber nicht als Spline interpretiert.
//! * **Avatar nur als Kennung.** `mapAvatar` liefert `qsAvatarID`; die Maße
//!   stehen in der `.avt` und sind nicht gedeutet.
//! * **Lage unausgewertet.** Die `.pos` wird gelesen und geprüft, aber die
//!   Anordnungspunkte sind noch nicht an klon3ds Hüllkörper abgebildet.

use std::path::Path;

use crate::property::{self, Map, Wert};
use crate::zip::{Archiv, ZipFehler};

/// Fehler beim Lesen eines CLO-Projekts.
#[derive(Debug, thiserror::Error)]
pub enum ZprjFehler {
    #[error("Datei {0:?} nicht lesbar: {1}")]
    Io(String, #[source] std::io::Error),

    #[error("kein Projekt: {0}")]
    KeinProjekt(String),

    #[error("ZIP-Fehler: {0}")]
    Zip(#[from] ZipFehler),

    #[error("Property-Stream: {0}")]
    Stream(#[from] property::StreamFehler),

    #[error("Feld {0:?} fehlt oder hat den falschen Typ")]
    Feld(String),

    #[error("Feld {0:?} hat eine unerwartete Länge {1}")]
    Laenge(String, usize),
}

/// Ein gelesenes CLO-Projekt.
#[derive(Debug)]
pub struct Projekt {
    /// Projektname aus dem Dateinamen.
    pub name: String,
    /// Alle Teile in Dateireihenfolge.
    pub teile: Vec<Teil>,
    /// Alle Nahtpaare, in Gruppenreihenfolge.
    pub naehte: Vec<Naht>,
    /// Stoffe in der Reihenfolge von `listFabric`.
    pub stoffe: Vec<Stoff>,
    /// Kennung des Avatars (`qsAvatarID`), falls vorhanden.
    pub avatar: Option<String>,
    /// Der Körper, auf dem CLO simuliert hat (`body` aus der `.avt`).
    pub koerper: Option<Koerper>,
    /// Alle Einträge des äußeren ZIP, für die Diagnose.
    pub beilagen: Vec<Beilage>,
}

/// Ein Eintrag des äußeren ZIP.
#[derive(Debug, Clone)]
pub struct Beilage {
    pub name: String,
    /// Größe **entpackt**.
    pub groesse: usize,
}

/// Der Körper, auf dem CLO das Projekt simuliert hat.
///
/// Steckt als `.avt` (CLO-Kopf + ZIP) im Projekt, darin eine `.dan`
/// (Property-Stream ohne Kopf). Von den neun `listSkinController`-Einträgen
/// ist `body` das Kleidungsnetz: 31 422 Knoten, 62 658 Dreiecke bei der
/// Prüfdatei - genau die Zahlen des Standardkörpers aus Befund 74.
///
/// **Millimeter im Avatarraum, y = Höhe** - dasselbe Bezugssystem wie
/// [`Teil::position3d`]. Der Körper steht in der **A-Pose** (Arme waagerecht,
/// |x| bis 491 mm), das Kleidungsnetz dagegen in der entspannten Pose: an
/// Rumpf und Rock liegt es 22-26 mm über der Haut (Median), kein einziger
/// Punkt steckt darin. Als **Kollisionskörper** taugt er damit, als
/// Passform-Aussage nicht.
#[derive(Debug, Clone, Default)]
pub struct Koerper {
    pub name: String,
    /// mm im Avatarraum, y = Höhe.
    pub punkte: Vec<Punkt3>,
    pub dreiecke: Vec<[u32; 3]>,
    /// Alle Körperteile der `.dan` mit ihren Netzzahlen - `body` ist eines
    /// davon, Schuhe, Haare, Augen usw. die übrigen.
    pub teile: Vec<Koerperteil>,
}

/// Ein Netz in der `.dan` (ein `listSkinController`-Eintrag).
#[derive(Debug, Clone)]
pub struct Koerperteil {
    pub name: String,
    pub knoten: usize,
    pub dreiecke: usize,
    pub collidable: bool,
}

impl Koerper {
    /// Liest den Körper aus den Einträgen eines `.zprj`.
    ///
    /// Nimmt die erste `.avt`; fehlt sie oder ist ihr Aufbau anders, kommt
    /// `Ok(None)` zurück - ein Projekt ohne Körper ist kein Lesefehler,
    /// sondern nur ein Projekt ohne mitgelieferten Avatar.
    fn lies(aussen: &Archiv<'_>) -> Result<Option<Koerper>, ZprjFehler> {
        let Ok(avt) = aussen.lies_mit_endung(".avt") else {
            return Ok(None);
        };
        // .avt = CLO-Kopf + ZIP. Der Kopf nennt Basis und Länge des ZIP
        // (`u32[0x20] + u32[0x24] == Dateigröße`, dieselbe Regel wie .zpac) -
        // er trägt **keinen** Property-Stream, deshalb nicht `lies_mit_kopf`.
        if avt.len() < 0x28 {
            return Ok(None);
        }
        let basis = u32::from_le_bytes(avt[0x20..0x24].try_into().unwrap()) as usize;
        let laenge = u32::from_le_bytes(avt[0x24..0x28].try_into().unwrap()) as usize;
        if basis < 0x28 || laenge == 0 || basis + laenge != avt.len() {
            return Ok(None);
        }
        let Ok(avt_archiv) = Archiv::neu(&avt[basis..basis + laenge]) else {
            return Ok(None);
        };
        let Ok(dan) = avt_archiv.lies_mit_endung(".dan") else {
            return Ok(None);
        };
        // Die .dan hat **keinen** Kopf: der Map-Block beginnt bei 0.
        let mut leser = property::Leser::neu(&dan).mit_pfad("dan");
        let wurzel = leser.map(None)?;

        let Some(sk) = wurzel.pfad_liste(&["listSkinController"]) else {
            // Ein .avt ohne SkinController ist ein Textur- oder Kopf-Avatar,
            // kein Fehler des Projekts.
            return Ok(None);
        };

        let mut teile = Vec::new();
        let mut koerper: Option<Koerper> = None;
        for eintrag in &sk.eintraege {
            let Wert::Map(steuerung) = eintrag else { continue };
            let name = steuerung
                .pfad_map(&["mapElement"])
                .and_then(|m| m.feld("qsName"))
                .and_then(|w| w.als_blatt())
                .and_then(|b| b.als_text())
                .unwrap_or_default();
            let Some(Wert::Map(mesh)) = steuerung.feld("mapMesh") else { continue };
            let knoten = mesh
                .feld("uiVertexCount")
                .and_then(|w| w.als_blatt())
                .and_then(|b| b.als_u32())
                .unwrap_or(0) as usize;
            let dreiecke = mesh
                .feld("uiTriangleCount")
                .and_then(|w| w.als_blatt())
                .and_then(|b| b.als_u32())
                .unwrap_or(0) as usize;
            let collidable = mesh
                .feld("bCollidable")
                .and_then(|w| w.als_blatt())
                .and_then(|b| b.als_bool())
                .unwrap_or(false);
            teile.push(Koerperteil { name: name.clone(), knoten, dreiecke, collidable });

            // Nur `body` ist der Kleidungsträger.
            if koerper.is_some() || name != "body" {
                continue;
            }
            let (punkte, flaechen) = lies_mesh3(mesh, knoten, dreiecke)?;
            koerper = Some(Koerper { name, punkte, dreiecke: flaechen, teile: Vec::new() });
        }

        Ok(koerper.map(|mut k| {
            k.teile = teile;
            k
        }))
    }
}

/// Liest `baPosition` (3 × f32) und `baIndex` (3 × u32) eines `mapMesh`.
///
/// **Achtung:** In der `.dan` tragen die `ba*`-Felder **keinen** Array-Kopf
/// (`u32 n + u8 Typ`) wie in `.zprj`/`.pac` - sie sind reine Rohdaten.
/// `baPosition` ist genau `knoten × 12` Byte, `baIndex` genau
/// `dreiecke × 12` Byte. Wer hier `als_array` benutzt, liest die ersten
/// Koordinaten als Zähler und scheitert an der Länge.
fn lies_mesh3(
    mesh: &Map,
    knoten: usize,
    dreiecke: usize,
) -> Result<(Vec<Punkt3>, Vec<[u32; 3]>), ZprjFehler> {
    let roh = |name: &str| -> Result<Vec<u8>, ZprjFehler> {
        mesh.feld(name)
            .and_then(|w| w.als_blatt())
            .map(|b| b.roh.clone())
            .ok_or_else(|| ZprjFehler::KeinProjekt(format!("{name} fehlt im mapMesh")))
    };
    let pos = roh("baPosition")?;
    if pos.len() != knoten * 12 {
        return Err(ZprjFehler::KeinProjekt(format!(
            "baPosition {} Byte für {knoten} Knoten",
            pos.len()
        )));
    }
    let punkte = pos
        .chunks_exact(12)
        .map(|c| Punkt3 {
            x: f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64,
            y: f32::from_le_bytes([c[4], c[5], c[6], c[7]]) as f64,
            z: f32::from_le_bytes([c[8], c[9], c[10], c[11]]) as f64,
        })
        .collect();
    let idx = roh("baIndex")?;
    if idx.len() != dreiecke * 12 {
        return Err(ZprjFehler::KeinProjekt(format!(
            "baIndex {} Byte für {dreiecke} Dreiecke",
            idx.len()
        )));
    }
    let u = |c: &[u8]| u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    let flaechen = idx
        .chunks_exact(12)
        .map(|c| [u(&c[0..4]), u(&c[4..8]), u(&c[8..12])])
        .collect();
    Ok((punkte, flaechen))
}

/// Ein Schnittteil, so wie CLO es speichert.
#[derive(Debug)]
pub struct Teil {
    /// Name im Projekt, z. B. `Pattern_999671` (CLOs Vorgabe) oder ein
    /// selbst vergebener Name.
    pub name: String,
    /// `mapElement.uiID`.
    pub id: u32,
    /// Konturpunkte in **Millimetern**, in Dateireihenfolge.
    ///
    /// Das sind die Punktknoten des Teils, nicht die Stützpunkte der Linien.
    pub punkte: Vec<Punkt2>,
    /// Linien des Teilrands, in Dateireihenfolge.
    pub linien: Vec<Linie>,
    /// Innenformen (Abnäher, Falten), jede mit eigener Kontur.
    pub innenformen: Vec<Innenform>,
    /// Index in [`Projekt::stoffe`]; `None`, wenn CLO `-1` gespeichert hat.
    pub stoff: Option<usize>,
    /// Knotenzahl des 2D-Netzes (`uiVertexCount2D`).
    pub netzknoten: usize,
    /// Dreieckszahl des 2D-Netzes (`uiTriangleCount`).
    pub netzdreiecke: usize,
    /// Partikelabstand in mm (`fEdgeLength`), bei CLO 10.
    pub kantenlaenge_mm: f64,
    /// `mapShape2D.bHalfSymmetric`.
    pub halbsymmetrisch: bool,
    /// CLOs **fertig simuliertes** 3D-Netz (`baPosition3D`), je Knoten
    /// 3 × `f32` in Millimetern im Avatarraum. Leer, wenn CLO nichts
    /// gespeichert hat.
    ///
    /// Das ist die Kleidung im **gespeicherten** Zustand, nicht die flache
    /// Ruheform - siehe [`Teil::ruheform`] für den Gegensatz.
    pub position3d: Vec<Punkt3>,
    /// Die **Ruheform** des Netzes (`mapMesh2D.baRest`), 2 × `f32` je Knoten.
    ///
    /// Gegenüber der 2D-Kontur nur **verschoben**, nicht gedreht: die
    /// Konturpunkte sind Bézier-Kontrollpunkte, die Ruheform die
    /// ausgewertete Fläche. Kantenverhältnis Ruheform/3D ist 1,00 - daran
    /// lässt sich die Dehnung ablesen.
    pub ruheform: Vec<Punkt2>,
    /// Dreiecke des Netzes (`mapMesh2D.baTri`), Indizes in
    /// [`Teil::position3d`] bzw. [`Teil::ruheform`].
    pub dreiecke: Vec<[u32; 3]>,
    /// Der Name aus `mapArrange`, z. B. `Body_Front_Center_2`. Leer bei den
    /// Beiwerk-Teilen (`Arrangement Point`).
    pub lage_name: String,
    /// Körperzone aus `mapArrange.qsPanName`: `Body_L`, `Body_R`, `Arm_L`,
    /// `Arm_R`. Leer, wenn das Teil keiner Zone zugeordnet ist (bei
    /// `japanese_school_uniform` 22 von 31).
    pub zone: String,
    /// Höhe normiert (`mapArrange.fNormalizedHeight`), 0..1.
    pub lage_hoehe: f64,
    /// Umfangslage normiert (`mapArrange.fNormalizedArcLength`), um 0.
    pub lage_bogen: f64,
    /// Drehung in Grad (`mapArrange.fAngle`).
    pub lage_winkel: f64,
    /// `bDraped`: CLOs eigenes Kennzeichen am Teil. **Nicht** als
    /// Flachheits-Indikator brauchbar - über alle 47 fremden Projekte
    /// (2 109 Teile) gezählt: `bDraped=1` mit flachem Netz **139×**,
    /// `bDraped=0` mit räumlichem Netz **78×**. Die Bedeutung bleibt offen;
    /// flach erkennt man am konstanten z in [`Teil::position3d`].
    pub simuliert: bool,
}

impl Teil {
    /// Alle Linien, in der Reihenfolge, in der `uiStartIndex` einer Naht
    /// zählt: erst der Rand, dann die Innenformen.
    pub fn linien_flach(&self) -> Vec<&Linie> {
        let mut v: Vec<&Linie> = self.linien.iter().collect();
        for f in &self.innenformen {
            v.extend(f.linien.iter());
        }
        v
    }

    /// Die Linie, auf die eine Nahtseite zeigt.
    pub fn linie_fuer(&self, seite: &Nahtseite) -> Option<&Linie> {
        match (seite.tiefe, seite.innenform) {
            // Tiefe 1: Rand des Teils.
            (1, _) => self.linien.get(seite.linie_von),
            // Tiefe 2: Innenlinie; `linie_von` ist dort schon auf die
            // Innenform umgerechnet.
            (2, Some(f)) => self.innenformen.get(f)?.linien.get(seite.linie_von),
            _ => None,
        }
    }
}

/// Ein Konturpunkt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Punkt2 {
    pub x: f64,
    pub y: f64,
}

/// Ein Punkt im Avatarraum (CLOs 3D-Netz), in **Millimetern**.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Punkt3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Eine Linie: von Punktknoten zu Punktknoten, mit Stützpunkten und
/// Netzpunkten.
#[derive(Debug, Clone)]
pub struct Linie {
    /// `uiID` der Linie.
    pub id: u32,
    /// Index des Anfangspunkts (bei einer Innenform: in deren Punktliste).
    pub von: usize,
    /// Index des Endpunkts.
    pub bis: usize,
    /// Zwischenstützpunkte in Millimetern, **ohne** Anfang und Ende.
    /// Leer bei einer geraden Linie.
    pub stuetzpunkte: Vec<Punkt2>,
    /// `iLineType`: 0 = Gerade (keine Stützpunkte), 2 = Kurve mit
    /// Stützpunkten, 3 = Kurve mit zwei Stützpunkten. Geprüft an 194 Linien;
    /// die Bedeutung darüber hinaus ist offen.
    pub art: i32,
    /// `listMeshPoint[].uiMeshPointIndex` - die Netzpunkte dieser Linie vom
    /// Anfang zum Ende. Anfang und Ende sind in beiden Nachbarlinien
    /// enthalten.
    pub netzpunkte: Vec<u32>,
}

/// Eine Innenform: Abnäher, Falte oder Loch mit eigener Kontur.
#[derive(Debug, Clone)]
pub struct Innenform {
    pub name: String,
    pub punkte: Vec<Punkt2>,
    pub linien: Vec<Linie>,
}

/// Eine Naht: zwei Seiten, die zusammengenäht werden.
#[derive(Debug, Clone)]
pub struct Naht {
    /// Name der Nahtgruppe, z. B. `Sewing_3466204`.
    pub name: String,
    /// `mapElement.uiID` der Gruppe.
    pub id: u32,
    pub seite_a: Nahtseite,
    pub seite_b: Nahtseite,
    /// `bDraped` der **Gruppe**: CLO hat sie simuliert.
    ///
    /// Zusammen mit [`Naht::aktiv`] trennt das den vernähten Zustand vom
    /// ungenähten: über 48 Projekte gemessen liegen die Nahtpunkte bei
    /// `bDraped=1` **und** `aktiv` im Median bei 0,00 mm, bei `bDraped=0`
    /// oder `aktiv = false` bei 2,68 mm mit 72 von 195 Paaren über 20 mm.
    pub simuliert: bool,
    /// `bActive` des **Nahtpaars**: die Naht ist eingeschaltet.
    pub aktiv: bool,
}

/// Eine Seite einer Naht.
#[derive(Debug, Clone)]
pub struct Nahtseite {
    /// Index des Teils in [`Projekt::teile`] (`uiSelfIndex`).
    pub teil: usize,
    /// `uiDepth`: 1 = Teilrand, 2 = Innenlinie.
    pub tiefe: i32,
    /// Bei `tiefe == 2`: Index der Innenform im Teil.
    pub innenform: Option<usize>,
    /// Erste Linie (`uiStartIndex`), gezählt wie in
    /// [`Teil::linien_flach`] bzw. in den Innenlinien der Innenform.
    pub linie_von: usize,
    /// Letzte Linie (`uiEndIndex`).
    pub linie_bis: usize,
    /// Anteile auf der ersten und letzten Linie, 0..1.
    pub anteil_von: f64,
    pub anteil_bis: f64,
    /// `bDirection`: `true`, wenn die Seite rückwärts durch das Teil läuft.
    pub rueckwaerts: bool,
    /// Die Netzpunkte dieser Naht, **lokal zum Teil**, in Nährichtung.
    pub netzpunkte: Vec<i32>,
}

/// Ein Stoff.
#[derive(Debug, Clone)]
pub struct Stoff {
    pub name: String,
    /// Dicke in Millimetern (`mapPhysical.fThickness`).
    pub dicke_mm: Option<f64>,
    /// Flächengewicht in g/m². CLO speichert `fDensity` in g/mm²; der Wert
    /// wird mit 10⁶ umgerechnet (0,00034899 → 349 g/m²).
    pub gewicht_g_m2: Option<f64>,
    /// Name des Physikprofils (`qsPhysicalPropertyName`), z. B.
    /// `Latex_0.4mm_Radical_Rubber`.
    pub profil: Option<String>,
    /// Biegesteifigkeit **in Schussrichtung** (`mapPhysical.fBuK`) in
    /// g·mm²/s²/rad, so wie CLO sie speichert - die Umrechnung nach N·m macht
    /// der Aufrufer (`× 10⁻⁹`).
    ///
    /// Bis #131 las der Projektleser nur Dicke, Gewicht und Profilname: ein
    /// `.zprj` konnte damit nicht richtungsabhängig rechnen, obwohl der Solver
    /// es seit #129 kann. `None`, wenn das Feld fehlt oder keine f32 ist - die
    /// drei Biegewerte sind kein Pflichtfeld, ein Projekt mit fehlendem
    /// `mapPhysical` bleibt lesbar.
    pub biegung_schuss: Option<f64>,
    /// `fBvK` - Biegung in Kettrichtung, gleiche Einheit.
    pub biegung_kette: Option<f64>,
    /// `fBhK` - Biegung unter 45°, gleiche Einheit.
    pub biegung_schraege: Option<f64>,
    /// Dehnsteifigkeit **in Schussrichtung** (`fSuK`) in g/s² = mN/m, wie
    /// CLO sie speichert; der Aufrufer rechnet `× 10⁻³` nach N/m.
    ///
    /// Ohne Dehnung, Schub und Reibung war ein Projektstoff nur eine Anzeige:
    /// `anziehen --zprj` nahm den Körper aus dem Projekt, den Stoff aber
    /// weiter aus `--material`. Zusammen mit [`Self::schub`] und
    /// [`Self::reibung`] kann ein `.zprj` seinen eigenen Stoff stellen.
    pub dehnung_schuss: Option<f64>,
    /// `fSvK` - Dehnung in Kettrichtung, gleiche Einheit.
    pub dehnung_kette: Option<f64>,
    /// `fHK` - Schub, gleiche Einheit. Bei Latex halb so groß wie die Dehnung
    /// (`MATERIAL.md`).
    pub schub: Option<f64>,
    /// `fFriction` - Reibungszahl, dimensionslos.
    pub reibung: Option<f64>,
}

// ─────────────────────────────────────────────────────── Lesen ──

impl Projekt {
    /// Liest ein Projekt von der Platte.
    pub fn lies(pfad: impl AsRef<Path>) -> Result<Self, ZprjFehler> {
        let pfad = pfad.as_ref();
        let daten = std::fs::read(pfad)
            .map_err(|e| ZprjFehler::Io(pfad.display().to_string(), e))?;
        let name = pfad
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "projekt".to_string());
        Self::lies_bytes(&daten, &name)
    }

    /// Liest ein Projekt aus dem Speicher (für Prüfungen und Tests).
    pub fn lies_bytes(daten: &[u8], name: &str) -> Result<Self, ZprjFehler> {
        let aussen = Archiv::neu(daten)?;

        let beilagen: Vec<Beilage> = aussen
            .eintraege()
            .iter()
            .filter(|e| !e.ist_verzeichnis())
            .map(|e| Beilage {
                name: e.name.clone(),
                groesse: e.roh,
            })
            .collect();

        // Die .zpac trägt den Inhalt. Fehlt sie, ist es kein Projekt.
        let zpac = aussen
            .lies_mit_endung(".zpac")
            .map_err(|_| ZprjFehler::KeinProjekt("kein .zpac im Archiv".to_string()))?;
        // Die .zpac ist selbst ein ZIP in einem CLO-Kopf:
        // u32[0x20] + u32[0x24] == Dateigröße.
        let innen_bytes = if property::hat_kopf(&zpac) {
            let basis = u32::from_le_bytes(zpac[0x20..0x24].try_into().unwrap()) as usize;
            let laenge = u32::from_le_bytes(zpac[0x24..0x28].try_into().unwrap()) as usize;
            &zpac[basis..basis + laenge]
        } else {
            &zpac[..]
        };
        let innen = Archiv::neu(innen_bytes)?;

        let pac = innen
            .lies_mit_endung(".pac")
            .map_err(|_| ZprjFehler::KeinProjekt("kein .pac im .zpac".to_string()))?;

        // Die .pac hat **keinen** Kopf: der Map-Block beginnt bei 0
        // (Befund 58: "der 12-Byte-Kopf war keiner").
        let mut leser = property::Leser::neu(&pac).mit_pfad("pac");
        let wurzel = leser.map(None)?;
        if leser.rest() != 0 {
            return Err(ZprjFehler::KeinProjekt(format!(
                "Mustersatz endet {} Byte vor dem Dateiende - Format nicht verstanden",
                leser.rest()
            )));
        }

        let mut projekt = Self::aus_wurzel(&wurzel, name)?;

        // Die Dreiecksindizes müssen in beide Knotenlisten zeigen. Geht das
        // nicht auf, ist die Deutung von baRest/baPosition3D falsch - dann
        // lieber keine Dreiecke als ein still falsches Netz.
        for teil in &mut projekt.teile {
            let n = teil.position3d.len().max(teil.ruheform.len());
            if teil.dreiecke.iter().flatten().any(|&i| i as usize >= n) {
                teil.dreiecke.clear();
            }
        }

        // Die .pos ist Beiwerk: lesen, um zu belegen, dass die Kette stimmt.
        // Sie trägt **nur die Avatar-Pose** (mapInitPoseData,
        // mapDefaultTransformer3D, mapTransformer3D, je 7 Kinder: Joint,
        // body, eye_L/R, eyelash_L/R, tooth) - keine Teile-Lage. Die steht in
        // mapArrange am Teil.
        if let Ok(pos) = innen.lies_mit_endung(".pos") {
            if property::hat_kopf(&pos) {
                let _ = property::lies_mit_kopf(&pos)?;
            }
        }

        // Der Körper steckt als .avt im Projekt. Fehlt er oder ist sein
        // Aufbau anders, bleibt das Feld leer - das ist kein Lesefehler.
        projekt.koerper = Koerper::lies(&aussen)?;

        projekt.beilagen = beilagen;
        Ok(projekt)
    }

    fn aus_wurzel(wurzel: &Map, name: &str) -> Result<Self, ZprjFehler> {
        let stoffe = lies_stoffe(wurzel)?;
        let teile = lies_teile(wurzel)?;
        let naehte = lies_naehte(wurzel, &teile)?;
        let avatar = wurzel
            .pfad_map(&["mapAvatar"])
            .and_then(|m| m.feld("qsAvatarID"))
            .and_then(|w| w.als_blatt())
            .and_then(|b| b.als_text())
            .filter(|s| !s.is_empty());

        Ok(Self {
            name: name.to_string(),
            teile,
            naehte,
            stoffe,
            avatar,
            koerper: None,
            beilagen: Vec::new(),
        })
    }
}

/// Sucht eine Liste im Baum, gleich unter welchem Pfad.
///
/// Nötig, weil CLO den Mustersatz der Referenzfassung unter
/// `mapPatternEditor.mapPatternList.listPattern` führt, die Bibliothekssätze
/// (Befund 62) aber unter `mapPatternBlockManager.listPatternBlock`. Der Name
/// des Feldes ist stabil, der Pfad nicht.
///
/// **Nicht die erste Fundstelle nehmen.** Fremde Projekte (CLO 2024/2022)
/// führen denselben Namen mehrfach: `mapPatternBlockManager.mapPBtoPB` trägt
/// ein `listSeamLinePairGroup`, das **leer** ist, während die echte Liste
/// daneben 150 Einträge hat. Deshalb sammelt die Suche alle Fundstellen und
/// nimmt die erste **nichtleere**, die nicht unter dem Blockverwalter liegt.
fn finde_liste<'a>(wurzel: &'a Map, feldname: &str) -> Option<&'a property::Liste> {
    fn suche<'a>(
        m: &'a Map,
        feldname: &str,
        tiefe: usize,
        im_verwalter: bool,
        treffer: &mut Vec<(usize, bool, &'a property::Liste)>,
    ) {
        if tiefe > 8 {
            return;
        }
        for f in &m.felder {
            // `mapPatternBlockManager` und `mapPBtoPB` spiegeln Teile und
            // Nähte für Bibliotheksblöcke; ihre Listen sind oft leer.
            let verwalter = im_verwalter || f.name.contains("BlockManager") || f.name.contains("PBtoPB");
            match &f.wert {
                Wert::Liste(l) if f.name == feldname => {
                    treffer.push((tiefe, verwalter, l));
                }
                Wert::Map(k) => suche(k, feldname, tiefe + 1, verwalter, treffer),
                Wert::Liste(l) => {
                    for e in &l.eintraege {
                        if let Wert::Map(k) = e {
                            suche(k, feldname, tiefe + 1, verwalter, treffer);
                        }
                    }
                }
                Wert::Blatt(_) => {}
            }
        }
    }

    let mut treffer = Vec::new();
    suche(wurzel, feldname, 0, false, &mut treffer);
    if treffer.is_empty() {
        return None;
    }
    // Sortierschlüssel: nichtleer zuerst, dann nicht im Verwalter, dann flach.
    treffer.sort_by_key(|(tiefe, verwalter, l)| (l.eintraege.is_empty(), *verwalter, *tiefe));
    Some(treffer[0].2)
}

// ── kleine Lesehelfer ──

fn zahl(m: &Map, name: &str) -> Option<i32> {
    m.feld(name)?.als_blatt()?.als_i32()
}

fn komma(m: &Map, name: &str) -> Option<f64> {
    m.feld(name)?.als_blatt()?.als_f32().map(f64::from)
}

fn text(m: &Map, name: &str) -> Option<String> {
    m.feld(name)?.als_blatt()?.als_text()
}

fn wahr(m: &Map, name: &str) -> Option<bool> {
    m.feld(name)?.als_blatt()?.als_bool()
}

fn vektor(m: &Map, name: &str) -> Option<[f32; 2]> {
    m.feld(name)?.als_blatt()?.als_f32_2()
}

fn zaehler(m: &Map, name: &str) -> Option<u32> {
    m.feld(name)?.als_blatt()?.als_u32()
}

/// Punkte aus einer `listPoint`: je Eintrag `v2Position`.
fn punkte_aus(liste: Option<&property::Liste>) -> Vec<Punkt2> {
    liste
        .map(|l| {
            l.eintraege
                .iter()
                .filter_map(|e| e.als_map())
                .filter_map(|m| vektor(m, "v2Position"))
                .map(|[x, y]| Punkt2 {
                    x: f64::from(x),
                    y: f64::from(y),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Linien aus einer `listLine`.
fn linien_aus(liste: Option<&property::Liste>) -> Vec<Linie> {
    liste
        .map(|l| {
            l.eintraege
                .iter()
                .filter_map(|e| e.als_map())
                .map(|m| Linie {
                    id: zaehler(m, "uiID").unwrap_or_default(),
                    von: zahl(m, "uiStartPointIndex").unwrap_or_default().max(0) as usize,
                    bis: zahl(m, "uiEndPointIndex").unwrap_or_default().max(0) as usize,
                    stuetzpunkte: punkte_aus(m.pfad_liste(&["listPoint"])),
                    art: zahl(m, "iLineType").unwrap_or_default(),
                    netzpunkte: m
                        .pfad_liste(&["listMeshPoint"])
                        .map(|mp| {
                            mp.eintraege
                                .iter()
                                .filter_map(|e| e.als_map())
                                .filter_map(|k| zaehler(k, "uiMeshPointIndex"))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── Netz: 3D-Lage, Ruheform, Dreiecke ──

/// `baPosition3D`: je Knoten 3 × `f32` (12 Byte), Millimeter im Avatarraum.
///
/// **Beleg:** bei `japanese_school_uniform` 31 von 31 Teilen mit exakt
/// `12 · uiVertexCount2D` Byte; Summe 88 718 Knoten = Summe von
/// `uiVertexCount2D`. Ein Rest, der nicht aufgeht, wird verworfen statt
/// halb gedeutet.
fn punkte3d_aus(blatt: Option<&property::Blatt>) -> Vec<Punkt3> {
    let Some(b) = blatt else {
        return Vec::new();
    };
    if b.roh.len() % 12 != 0 {
        return Vec::new();
    }
    b.roh
        .chunks_exact(12)
        .map(|c| Punkt3 {
            x: f64::from(f32::from_le_bytes(c[0..4].try_into().unwrap())),
            y: f64::from(f32::from_le_bytes(c[4..8].try_into().unwrap())),
            z: f64::from(f32::from_le_bytes(c[8..12].try_into().unwrap())),
        })
        .collect()
}

/// `baRest`: je Knoten 2 × `f32` (8 Byte), Ruheform in Millimetern.
fn punkte2d_aus(blatt: Option<&property::Blatt>) -> Vec<Punkt2> {
    let Some(b) = blatt else {
        return Vec::new();
    };
    if b.roh.len() % 8 != 0 {
        return Vec::new();
    }
    b.roh
        .chunks_exact(8)
        .map(|c| Punkt2 {
            x: f64::from(f32::from_le_bytes(c[0..4].try_into().unwrap())),
            y: f64::from(f32::from_le_bytes(c[4..8].try_into().unwrap())),
        })
        .collect()
}

/// `baTri`: je Dreieck 3 × `u32` (12 Byte).
///
/// Die Prüfung, dass jeder Index unter der Knotenzahl liegt, macht
/// [`Projekt::lies_bytes`] - dort ist die Knotenzahl bekannt. Hier wird nur
/// die Bytezahl geprüft; ein Rest, der nicht aufgeht, ergibt keine Dreiecke.
fn dreiecke_aus(blatt: Option<&property::Blatt>) -> Vec<[u32; 3]> {
    let Some(b) = blatt else {
        return Vec::new();
    };
    if b.roh.len() % 12 != 0 {
        return Vec::new();
    }
    b.roh
        .chunks_exact(12)
        .map(|c| {
            [
                u32::from_le_bytes(c[0..4].try_into().unwrap()),
                u32::from_le_bytes(c[4..8].try_into().unwrap()),
                u32::from_le_bytes(c[8..12].try_into().unwrap()),
            ]
        })
        .collect()
}

// ── Stoffe ──

fn lies_stoffe(wurzel: &Map) -> Result<Vec<Stoff>, ZprjFehler> {
    let liste = match finde_liste(wurzel, "listFabric") {
        Some(l) => l,
        None => return Ok(Vec::new()),
    };
    Ok(liste
        .eintraege
        .iter()
        .filter_map(|e| e.als_map())
        .map(|m| {
            let physik = m.pfad_map(&["mapPhysical"]);
            Stoff {
                // CLO schreibt denselben Namen mehrfach (UTF8-Variante);
                // `qsName` ist die Kurzform und reicht.
                name: text(m, "qsName").unwrap_or_default(),
                dicke_mm: physik.and_then(|p| komma(p, "fThickness")),
                // g/mm² → g/m²
                gewicht_g_m2: physik
                    .and_then(|p| komma(p, "fDensity"))
                    .map(|d| d * 1.0e6),
                profil: physik.and_then(|p| text(p, "qsPhysicalPropertyName")),
                biegung_schuss: physik.and_then(|p| komma(p, "fBuK")),
                biegung_kette: physik.and_then(|p| komma(p, "fBvK")),
                biegung_schraege: physik.and_then(|p| komma(p, "fBhK")),
                dehnung_schuss: physik.and_then(|p| komma(p, "fSuK")),
                dehnung_kette: physik.and_then(|p| komma(p, "fSvK")),
                schub: physik.and_then(|p| komma(p, "fHK")),
                reibung: physik.and_then(|p| komma(p, "fFriction")),
            }
        })
        .collect())
}

// ── Teile ──

fn lies_teile(wurzel: &Map) -> Result<Vec<Teil>, ZprjFehler> {
    let liste = finde_liste(wurzel, "listPattern")
        .or_else(|| finde_liste(wurzel, "listPatternBlock"))
        .ok_or_else(|| {
            ZprjFehler::KeinProjekt(
                "weder listPattern noch listPatternBlock gefunden".to_string(),
            )
        })?;

    let mut teile = Vec::with_capacity(liste.eintraege.len());
    for e in &liste.eintraege {
        let m = e
            .als_map()
            .ok_or_else(|| ZprjFehler::Feld("listPattern-Eintrag ist keine Map".to_string()))?;
        let element = m.pfad_map(&["mapElement"]);
        let form = m.pfad_map(&["mapShape2D"]);
        let stoffform = m.pfad_map(&["mapFabricShape2D"]);
        let netz = stoffform.and_then(|s| s.pfad_map(&["mapMesh2D"]));
        let arrange = m.pfad_map(&["mapArrange"]);

        // Innenformen: eigene Kontur mit eigenen Punkten und Linien.
        let innenformen = m
            .pfad_liste(&["listInnerShape"])
            .map(|l| {
                l.eintraege
                    .iter()
                    .filter_map(|e| e.als_map())
                    .map(|f| Innenform {
                        name: text(f, "qsInnerShapeName").unwrap_or_default(),
                        punkte: punkte_aus(f.pfad_liste(&["listPoint"])),
                        linien: linien_aus(f.pfad_liste(&["listLine"])),
                    })
                    .collect()
            })
            .unwrap_or_default();

        teile.push(Teil {
            name: element
                .and_then(|e| text(e, "qsName"))
                .unwrap_or_default(),
            id: element.and_then(|e| zaehler(e, "uiID")).unwrap_or_default(),
            punkte: punkte_aus(form.and_then(|f| f.pfad_liste(&["listPoint"]))),
            linien: linien_aus(form.and_then(|f| f.pfad_liste(&["listLine"]))),
            innenformen,
            // Der echte Stoffindex steht in mapFabricShape2D; in
            // mapShape2D.iFabricIndex steht bei allen Teilen -1.
            stoff: stoffform
                .and_then(|s| zahl(s, "iFabricIndex"))
                .filter(|i| *i >= 0)
                .map(|i| i as usize),
            netzknoten: netz
                .and_then(|n| zaehler(n, "uiVertexCount2D"))
                .unwrap_or_default() as usize,
            netzdreiecke: netz
                .and_then(|n| zaehler(n, "uiTriangleCount"))
                .unwrap_or_default() as usize,
            kantenlaenge_mm: m
                .feld("fEdgeLength")
                .and_then(|w| w.als_blatt())
                .and_then(|b| b.als_f32())
                .map(f64::from)
                .unwrap_or(0.0),
            halbsymmetrisch: form.and_then(|f| wahr(f, "bHalfSymmetric")).unwrap_or(false),
            position3d: punkte3d_aus(m.feld("baPosition3D").and_then(|w| w.als_blatt())),
            ruheform: punkte2d_aus(netz.and_then(|n| n.feld("baRest")).and_then(|w| w.als_blatt())),
            dreiecke: dreiecke_aus(netz.and_then(|n| n.feld("baTri")).and_then(|w| w.als_blatt())),
            lage_name: arrange.and_then(|a| text(a, "qsName")).unwrap_or_default(),
            zone: arrange.and_then(|a| text(a, "qsPanName")).unwrap_or_default(),
            lage_hoehe: arrange
                .and_then(|a| komma(a, "fNormalizedHeight"))
                .unwrap_or(0.0),
            lage_bogen: arrange
                .and_then(|a| komma(a, "fNormalizedArcLength"))
                .unwrap_or(0.0),
            lage_winkel: arrange.and_then(|a| komma(a, "fAngle")).unwrap_or(0.0),
            simuliert: m
                .feld("bDraped")
                .and_then(|w| w.als_blatt())
                .and_then(|b| b.als_bool())
                .unwrap_or(false),
        });
    }
    Ok(teile)
}

// ── Nähte ──

fn lies_naehte(wurzel: &Map, teile: &[Teil]) -> Result<Vec<Naht>, ZprjFehler> {
    let gruppen = match finde_liste(wurzel, "listSeamLinePairGroup") {
        Some(l) => l,
        None => return Ok(Vec::new()),
    };

    let mut naehte = Vec::new();
    for g in &gruppen.eintraege {
        let Some(gm) = g.als_map() else { continue };
        let name = gm
            .pfad_map(&["mapElement"])
            .and_then(|e| text(e, "qsName"))
            .unwrap_or_default();
        let id = gm
            .pfad_map(&["mapElement"])
            .and_then(|e| zaehler(e, "uiID"))
            .unwrap_or_default();

        let paare = match gm.pfad_liste(&["mapSeamLinePairList", "listSeamLinePair"]) {
            Some(p) => p,
            None => continue,
        };
        let gruppe_simuliert = gm
            .feld("bDraped")
            .and_then(|w| w.als_blatt())
            .and_then(|b| b.als_bool())
            .unwrap_or(false);
        for p in &paare.eintraege {
            let Some(pm) = p.als_map() else { continue };
            let seite_a = lies_nahtseite(pm, "mapSeamLine0", teile);
            let seite_b = lies_nahtseite(pm, "mapSeamLine1", teile);
            if let (Some(seite_a), Some(seite_b)) = (seite_a, seite_b) {
                naehte.push(Naht {
                    name: name.clone(),
                    id,
                    seite_a,
                    seite_b,
                    simuliert: gruppe_simuliert,
                    aktiv: pm
                        .feld("bActive")
                        .and_then(|w| w.als_blatt())
                        .and_then(|b| b.als_bool())
                        .unwrap_or(false),
                });
            }
        }
    }
    Ok(naehte)
}

/// Liest eine Nahtseite. `None`, wenn die Pflichtfelder fehlen oder der
/// Teilindex außerhalb der Teileliste liegt.
fn lies_nahtseite(pm: &Map, feld: &str, teile: &[Teil]) -> Option<Nahtseite> {
    let seite = pm.pfad_map(&[feld])?;
    let param = seite.pfad_map(&["mapParamLine"])?;

    let teil = zahl(param, "uiSelfIndex")?;
    if teil < 0 {
        return None;
    }
    let teil_index = teil as usize;
    let teil_dat = teile.get(teil_index)?;

    let tiefe = zahl(param, "uiDepth").unwrap_or(1);
    let linie_von = zahl(param, "uiStartIndex").unwrap_or_default().max(0) as usize;
    let linie_bis = zahl(param, "uiEndIndex").unwrap_or_default().max(0) as usize;

    // Tiefe 2: `uiStartIndex` zählt in die flache Liste aller Innenlinien
    // des Teils. Die Innenform, in der die Linie liegt, wird hier aufgelöst,
    // damit der Aufrufer nicht selbst zählen muss.
    let innenform = if tiefe == 2 {
        let mut vorher = 0;
        let mut treffer = None;
        for (i, f) in teil_dat.innenformen.iter().enumerate() {
            if linie_von < vorher + f.linien.len() {
                treffer = Some((i, vorher));
                break;
            }
            vorher += f.linien.len();
        }
        // Hinter allen Innenlinien: dann ist die Deutung falsch, und die
        // Seite wird gemeldet statt geraten.
        let (i, vorher) = treffer?;
        // Auf die Linie innerhalb der Innenform umrechnen.
        (linie_von - vorher, linie_bis.saturating_sub(vorher), Some(i))
    } else {
        (linie_von, linie_bis, None)
    };
    let (linie_von, linie_bis, innenform) = innenform;

    let netzpunkte = seite
        .feld("arrMeshPointIndex")?
        .als_blatt()?
        .als_i32_array()?;

    Some(Nahtseite {
        teil: teil_index,
        tiefe,
        innenform,
        linie_von,
        linie_bis,
        anteil_von: komma(param, "fRatioStart").unwrap_or(0.0),
        anteil_bis: komma(param, "fRatioEnd").unwrap_or(0.0),
        rueckwaerts: zahl(param, "bDirection").unwrap_or(0) != 0,
        netzpunkte,
    })
}
