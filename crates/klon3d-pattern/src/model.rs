//! Datenmodell für Seamly2D-Muster (`.sm2d`).
//!
//! Die Felder bilden die tatsächliche Dateistruktur ab, nicht das
//! veröffentlichte XSD (`pattern/v0.2.0.xsd`) — das deckt das aktuelle
//! `.sm2d` nicht ab.
//!
//! **Wichtig:** quick-xmls `derive` liest Kindelemente über den Feldnamen,
//! XML-Attribute aber nur, wenn der `rename`-Name ein `@` trägt.
//! `rename = "id"` wird ignoriert, `rename = "@id"` funktioniert.
//! Deshalb steht unten an jedem Attribut ein `@`.

use serde::Deserialize;

// ─────────────────────────────────────────────────────────────── Wurzel ──

/// Wurzelelement `<pattern>`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pattern {
    /// Formatversion, z. B. `0.6.8`.
    pub version: String,
    /// Messeinheit: `mm`, `cm` oder `inch`.
    pub unit: String,
    /// Pfad zur Massdatei (`.smis`), optional.
    #[serde(default)]
    pub measurements: Option<String>,

    #[serde(default)]
    pub increments: Option<Increments>,
    #[serde(rename = "draftBlock", default)]
    pub draft_blocks: Vec<DraftBlock>,
}

/// Messeinheit des Musters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Mm,
    Cm,
    Inch,
}

impl Unit {
    /// Faktor nach Millimeter.
    pub fn to_mm(self) -> f64 {
        match self {
            Self::Mm => 1.0,
            Self::Cm => 10.0,
            Self::Inch => 25.4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Cm => "cm",
            Self::Inch => "inch",
        }
    }
}

// ──────────────────────────────────────────────────────── Messvariablen ──

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Increments {
    /// Das heisst `<variable>` — **kein** `<increment>` und kein
    /// `<increments>`-Container in dieser Dateigeneration.
    #[serde(rename = "variable", default)]
    pub items: Vec<Increment>,
}

/// Eine abgeleitete Variable mit Formel.
///
/// `name` **enthält** das führende `#` (`name="#Height"`). Die Trennung
/// Messvariable/Inkrement erfolgt in der Formel: `#Bust` liest das Massblatt,
/// `Hoehe = #Bust/2` ist ein Inkrement.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Increment {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@formula")]
    pub formula: String,
    #[serde(rename = "@description", default)]
    pub description: Option<String>,
}

// ──────────────────────────────────────────────────── Konstruktion ──

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DraftBlock {
    #[serde(rename = "@name", default)]
    pub name: Option<String>,
    #[serde(default)]
    pub calculation: Option<Calculation>,
    #[serde(default)]
    pub modeling: Option<Modeling>,
    /// Die Schnittteile **dieses** Blocks — nicht die des ganzen Musters.
    #[serde(default)]
    pub pieces: Option<Pieces>,
}

/// Der rechnende Teil eines Blockes: Punkte, Linien, Splines.
///
/// `<pathPoint>` hängt als Kind an `<spline>`, nicht an `<calculation>`;
/// der Abschnitt hier führt sie der Vollständigkeit halber mit.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Calculation {
    #[serde(rename = "point", default)]
    pub points: Vec<Point>,
    #[serde(rename = "line", default)]
    pub lines: Vec<Line>,
    #[serde(rename = "spline", default)]
    pub splines: Vec<Spline>,
    #[serde(rename = "arc", default)]
    pub arcs: Vec<Arc>,
    /// Operationen (`<operation>`): Kopien von Punkten und Kurven.
    #[serde(skip)]
    pub operations: Vec<Operation>,
}

/// Der zeichnende Teil eines Blockes: Platzieren, Drehen, Verschieben.
///
/// Enthält **keine Geometrie**, nur Verweise (`idObject`) auf Objekte des
/// `calculation`-Teils.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Modeling {
    #[serde(rename = "point", default)]
    pub points: Vec<Point>,
    #[serde(rename = "spline", default)]
    pub splines: Vec<Spline>,
    /// Modellierte Bögen — im Bustier tragen 8 Stück der 154
    /// Modellierobjekte einen Bogen, und ohne sie reißt die Konturkette.
    #[serde(rename = "arc", default)]
    pub arcs: Vec<Arc>,
}

/// Eine Operation (`<operation>`): verschiebt, dreht oder spiegelt die
/// Objekte ihrer Quelle; das i-te Quellobjekt ergibt das i-te Zielobjekt,
/// benannt wie die Quelle mit angehängtem `suffix`.
#[derive(Debug, Clone, Default)]
pub struct Operation {
    pub id: String,
    /// `moving`, `rotation`, `flippingByAxis`, `flippingByLine`.
    pub kind: String,
    pub suffix: String,
    /// Drehpunkt (`rotation`), Achsenpunkt (`flippingByAxis`).
    pub center: Option<String>,
    /// Formeln: Winkel (`moving`, `rotation`), Länge (`moving`).
    pub angle: Option<String>,
    pub length: Option<String>,
    /// `flippingByAxis`: `1` senkrechte Achse, sonst waagerechte.
    pub axis_type: Option<String>,
    /// `flippingByLine`: die Spiegelgerade.
    pub p1_line: Option<String>,
    pub p2_line: Option<String>,
    /// ids der Quell- und der Zielobjekte, positionsgleich.
    pub quellen: Vec<String>,
    pub ziele: Vec<String>,
}

/// Ein Punkt in der Konstruktion.
///
/// Seamly2D kennt zahlreiche Punkttypen; alle Felder sind optional, weil je
/// Typ nur eine bestimmte Kombination belegt ist. Die gültigen Kombinationen
/// stehen in [`crate::geom`] je Typ.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Point {
    #[serde(rename = "@id")]
    pub id: String,
    /// `single`, `endLine`, `alongLine`, `normal`, `modeling`,
    /// `shoulder`, `height`, `lineIntersectAxis`, `curveIntersectAxis`,
    /// `cutSpline`, `cutSplinePath`, `pointOfContact`, …
    #[serde(rename = "@type")]
    pub kind: String,
    #[serde(rename = "@name", default)]
    pub name: Option<String>,
    /// Nur im `modeling`-Teil belegt: `true` / `false`.
    #[serde(rename = "@inUse", default)]
    pub in_use: Option<String>,

    // Direkt gesetzte Koordinaten (nur Typ `single`).
    #[serde(rename = "@x", default)]
    pub x: Option<String>,
    #[serde(rename = "@y", default)]
    pub y: Option<String>,

    // Bezugspunkte.
    #[serde(rename = "@firstPoint", default)]
    pub first_point: Option<String>,
    #[serde(rename = "@secondPoint", default)]
    pub second_point: Option<String>,
    #[serde(rename = "@thirdPoint", default)]
    pub third_point: Option<String>,
    #[serde(rename = "@basePoint", default)]
    pub base_point: Option<String>,

    // Konstruktionsgrössen — jeweils eine **Formel**, kein Messwert.
    #[serde(rename = "@length", default)]
    pub length: Option<String>,
    #[serde(rename = "@angle", default)]
    pub angle: Option<String>,
    #[serde(rename = "@radius", default)]
    pub radius: Option<String>,
    #[serde(rename = "@center", default)]
    pub center: Option<String>,
    #[serde(rename = "@firstArc", default)]
    pub first_arc: Option<String>,
    #[serde(rename = "@secondArc", default)]
    pub second_arc: Option<String>,

    // Bezugsgeraden. Bei `lineIntersect` benennt die Ziffer **hinter**
    // "Line" die Gerade, die davor den Punkt
    // (`vtoollineintersect.cpp: Create`).
    #[serde(rename = "@p1Line", default)]
    pub p1_line: Option<String>,
    #[serde(rename = "@p2Line", default)]
    pub p2_line: Option<String>,
    #[serde(rename = "@p1Line1", default)]
    pub p1_line1: Option<String>,
    #[serde(rename = "@p2Line1", default)]
    pub p2_line1: Option<String>,
    #[serde(rename = "@p1Line2", default)]
    pub p1_line2: Option<String>,
    #[serde(rename = "@p2Line2", default)]
    pub p2_line2: Option<String>,

    // Achsenbezug.
    #[serde(rename = "@axisPoint1", default)]
    pub axis_point1: Option<String>,
    #[serde(rename = "@axisPoint2", default)]
    pub axis_point2: Option<String>,

    // Kontaktpunkt — belegt zusätzlich `firstPoint`, `secondPoint`,
    // `center` und `radius`.
    #[serde(rename = "@pointOfContact", default)]
    pub point_of_contact: Option<String>,
    #[serde(rename = "@firstContact", default)]
    pub first_contact: Option<String>,
    #[serde(rename = "@secondContact", default)]
    pub second_contact: Option<String>,

    // `cutSpline` / `cutSplinePath`: `forward` schneidet vom
    // Kurvenanfang, jeder andere Wert vom Kurvenende.
    #[serde(rename = "@direction", default)]
    pub direction: Option<String>,
    // Spline-Bezug. `cutSpline` nutzt `@spline`, `cutSplinePath`
    // `@splinePath`.
    #[serde(rename = "@spline", default)]
    pub spline: Option<String>,
    #[serde(rename = "@splinePath", default)]
    pub spline_path: Option<String>,
    /// `curveIntersectAxis`
    #[serde(rename = "@curve", default)]
    pub curve: Option<String>,
    /// `cutArc`: der geschnittene Bogen.
    #[serde(rename = "@arc", default)]
    pub arc: Option<String>,
    /// `pointOfIntersectionCurves`: die beiden Kurven.
    #[serde(rename = "@curve1", default)]
    pub curve1: Option<String>,
    #[serde(rename = "@curve2", default)]
    pub curve2: Option<String>,
    /// Welcher Schnittpunkt: `crossPoint` (Bögen, 1 = erster),
    /// `vCrossPoint` (1 = oberster), `hCrossPoint` (2 = rechter).
    #[serde(rename = "@crossPoint", default)]
    pub cross_point: Option<String>,
    #[serde(rename = "@vCrossPoint", default)]
    pub v_cross_point: Option<String>,
    #[serde(rename = "@hCrossPoint", default)]
    pub h_cross_point: Option<String>,
    /// `trueDarts`: Basislinie, die drei Abnäherpunkte (`dartP2` ist die
    /// Spitze) und die beiden Punkte, die das Werkzeug erzeugt — sie stehen
    /// nicht als eigener `<point>` in der Datei.
    #[serde(rename = "@baseLineP1", default)]
    pub base_line_p1: Option<String>,
    #[serde(rename = "@baseLineP2", default)]
    pub base_line_p2: Option<String>,
    #[serde(rename = "@dartP1", default)]
    pub dart_p1: Option<String>,
    #[serde(rename = "@dartP2", default)]
    pub dart_p2: Option<String>,
    #[serde(rename = "@dartP3", default)]
    pub dart_p3: Option<String>,
    #[serde(rename = "@point1", default)]
    pub point1: Option<String>,
    #[serde(rename = "@point2", default)]
    pub point2: Option<String>,
    #[serde(rename = "@name1", default)]
    pub name1: Option<String>,
    #[serde(rename = "@name2", default)]
    pub name2: Option<String>,

    // Modeling: Verweis auf das Originalobjekt.
    #[serde(rename = "@idObject", default)]
    pub id_object: Option<String>,
    #[serde(rename = "@typeObject", default)]
    pub type_object: Option<String>,
    #[serde(rename = "@idTool", default)]
    pub id_tool: Option<String>,
    /// `none`, `hair`, `dashLine`, `dashDotLine`, `dotLine` — nur Darstellung.
    #[serde(rename = "@lineType", default)]
    pub line_type: Option<String>,
    /// Verschiebung beim Modeling, in Musterskalen.
    #[serde(rename = "@mx", default)]
    pub mx: Option<String>,
    #[serde(rename = "@my", default)]
    pub my: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Line {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@firstPoint", default)]
    pub first_point: Option<String>,
    #[serde(rename = "@secondPoint", default)]
    pub second_point: Option<String>,
    #[serde(rename = "@typeLine", default)]
    pub type_line: Option<String>,
}

/// Ein interpolierter Spline oder eine Folge von Pfadpunkten.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Spline {
    #[serde(rename = "@id")]
    pub id: String,
    /// `simpleInteractive`, `interactive`, `cubicBezier`, `nonInteractive`,
    /// `modelingSpline`, `modelingPath`, …
    #[serde(rename = "@type")]
    pub kind: Option<String>,
    #[serde(rename = "@name", default)]
    pub name: Option<String>,
    #[serde(rename = "@point1", default)]
    pub point1: Option<String>,
    #[serde(rename = "@point4", default)]
    pub point4: Option<String>,
    /// The two inner control points of a `cubicBezier`.
    #[serde(rename = "@point2", default)]
    pub point2: Option<String>,
    #[serde(rename = "@point3", default)]
    pub point3: Option<String>,
    #[serde(rename = "@point1Line", default)]
    pub point1_line: Option<String>,
    #[serde(rename = "@point2Line", default)]
    pub point2_line: Option<String>,
    /// Winkel in Grad, können Formeln sein.
    #[serde(rename = "@angle1", default)]
    pub angle1: Option<String>,
    #[serde(rename = "@angle2", default)]
    pub angle2: Option<String>,
    #[serde(rename = "@length1", default)]
    pub length1: Option<String>,
    #[serde(rename = "@length2", default)]
    pub length2: Option<String>,
    /// Krümmungsradius (Typ `cubicBezier`).
    #[serde(rename = "@cRadius", default)]
    pub c_radius: Option<String>,
    #[serde(rename = "@inUse", default)]
    pub in_use: Option<String>,
    #[serde(rename = "@idObject", default)]
    pub id_object: Option<String>,
    /// `duplicate="1"` heisst in Seamly2D `SplPath_a_b_<n>` — dieser Pfad
    /// traegt also **nicht** den Namen `SplPath_a_b`
    /// (`vabstractcubicbezierpath.cpp: CreateName`).
    #[serde(rename = "@duplicate", default)]
    pub duplicate: Option<String>,
    /// Kindelemente von `<spline>` - im Gegensatz zum Namen **singulär**.
    #[serde(rename = "pathPoint", default)]
    pub path_points: Vec<PathPoint>,
}

/// Ein Pfadpunkt eines `cutSplinePath`.
///
/// `pSpline` **nennt trotz des Namens eine Punkt-id** — ein Pfad läuft durch
/// eine Folge von Punkten. `angle1`/`length1` ist der Zulauf, `angle2`/
/// `length2` der Ablauf am Punkt. In der Bibliothek tragen alle 4 627
/// Pfadpunkte beide Paare.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PathPoint {
    #[serde(rename = "@id", default)]
    pub id: Option<String>,
    #[serde(rename = "@pSpline", default)]
    pub p_spline: Option<String>,
    #[serde(rename = "@type", default)]
    pub kind: Option<String>,
    #[serde(rename = "@angle1", default)]
    pub angle1: Option<String>,
    #[serde(rename = "@angle2", default)]
    pub angle2: Option<String>,
    #[serde(rename = "@length1", default)]
    pub length1: Option<String>,
    #[serde(rename = "@length2", default)]
    pub length2: Option<String>,
    #[serde(rename = "@angle", default)]
    pub angle: Option<String>,
    #[serde(rename = "@length", default)]
    pub length: Option<String>,
    #[serde(rename = "@point1", default)]
    pub point1: Option<String>,
    #[serde(rename = "@point4", default)]
    pub point4: Option<String>,
    #[serde(rename = "@exclude", default)]
    pub exclude: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Arc {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@type", default)]
    pub kind: Option<String>,
    #[serde(rename = "@center", default)]
    pub center: Option<String>,
    #[serde(rename = "@angle1", default)]
    pub angle1: Option<String>,
    #[serde(rename = "@angle2", default)]
    pub angle2: Option<String>,
    #[serde(rename = "@radius", default)]
    pub radius: Option<String>,
    #[serde(rename = "@length", default)]
    pub length: Option<String>,
    #[serde(rename = "@point1", default)]
    pub point1: Option<String>,
    #[serde(rename = "@point2", default)]
    pub point2: Option<String>,
    #[serde(rename = "@inUse", default)]
    pub in_use: Option<String>,
    #[serde(rename = "@idObject", default)]
    pub id_object: Option<String>,
}

// ─────────────────────────────────────────────────────── Schnittteile ──

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pieces {
    #[serde(rename = "piece", default)]
    pub items: Vec<Piece>,
}

/// Ein fertiges Schnittteil.
///
/// Enthält **keine Geometrie**, nur eine geordnete Liste von Verweisen auf
/// Objekte in den Konstruktionsblöcken.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Piece {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name", default)]
    pub name: Option<String>,
    /// **Bool**, nicht die Breite.
    #[serde(rename = "@seamAllowance", default)]
    pub seam_allowance: Option<String>,
    /// Bool: Die Nahtzugabe steckt schon in der Kontur — Seamly2D legt
    /// keine an, die Kontur ist zugleich Schnittlinie (`vpiece.cpp`).
    #[serde(rename = "@seamAllowanceBuiltIn", default)]
    pub seam_allowance_built_in: Option<String>,
    /// Breite — **kann eine Formel sein**.
    #[serde(rename = "@width", default)]
    pub width: Option<String>,
    #[serde(rename = "@version", default)]
    pub version: Option<String>,
    #[serde(rename = "@closed", default)]
    pub closed: Option<String>,
    #[serde(rename = "@mx", default)]
    pub mx: Option<String>,
    #[serde(rename = "@my", default)]
    pub my: Option<String>,
    /// Schnittteil-Id, aus der das Teil stammt (Freemode/Abgeleitetes).
    #[serde(rename = "@idSource", default)]
    pub id_source: Option<String>,
    #[serde(rename = "@inLayout", default)]
    pub in_layout: Option<String>,
    #[serde(default)]
    pub nodes: Option<PieceNodes>,
    /// Benutzerdefinierte Nahtzugabe.
    #[serde(rename = "CSA", default)]
    pub csa: Option<CustomSeamAllowance>,
    /// The grainline (`<grainline>`) — raw attributes, evaluated by
    /// [`crate::fadenlauf`]. `Some` whenever the element exists on the piece,
    /// even with `visible="false"`.
    #[serde(rename = "grainline", default)]
    pub grainline: Option<Grainline>,
}

// ────────────────────────────────────────────────────────── Grainline ──

/// The raw attributes of a piece's `<grainline>` element, taken over verbatim.
///
/// Nothing is evaluated here: `rotation`/`length`/`arrowLength` are formulas in
/// the pattern unit, the anchors name construction points, and `mx`/`my` are
/// screen pixels. See [`crate::fadenlauf`] for the meaning of each attribute.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Grainline {
    /// `true`/`1` — Seamly2D only draws the grainline then; missing counts as
    /// `false` (Seamly2D's default).
    #[serde(rename = "@visible", default)]
    pub visible: Option<String>,
    /// `0` arrowheads at both ends, `1` only at the top, `2` only at the
    /// bottom. Purely a drawing property; the axis is the same.
    #[serde(rename = "@arrows", default)]
    pub arrows: Option<String>,
    /// Axis rotation in degrees as a formula; fallback `90`.
    #[serde(rename = "@rotation", default)]
    pub rotation: Option<String>,
    /// Axis length as a formula in the pattern unit; fallback `1`.
    #[serde(rename = "@length", default)]
    pub length: Option<String>,
    /// Arrowhead length as a formula in the pattern unit; fallback `0.5`.
    #[serde(rename = "@arrowLength", default)]
    pub arrow_length: Option<String>,
    /// Axis start, x in screen pixels (96 per inch, y down) — not a formula,
    /// a plain `xs:double`; fallback `0`.
    #[serde(rename = "@mx", default)]
    pub mx: Option<String>,
    /// Axis start, y in screen pixels (96 per inch, y down); fallback `0`.
    #[serde(rename = "@my", default)]
    pub my: Option<String>,
    /// Top end anchor — a modelling point id or, directly, a construction
    /// point id; `0`/empty/unknown = no anchor.
    #[serde(rename = "@topAnchor", default)]
    pub top_anchor: Option<String>,
    /// Bottom end anchor, same rules as `topAnchor`.
    #[serde(rename = "@bottomAnchor", default)]
    pub bottom_anchor: Option<String>,
    /// Centre anchor, same rules as `topAnchor`.
    #[serde(rename = "@centerAnchor", default)]
    pub center_anchor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PieceNodes {
    #[serde(rename = "node", default)]
    pub items: Vec<PieceNode>,
}

/// Ein Knoten der Teilekontur.
///
/// Zusammen mit [`PieceNodes::items`] ergibt das eine geschlossene Kette:
/// jeder Knoten verweist auf einen Punkt, eine Linie oder einen Spline im
/// `calculation`-Teil.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PieceNode {
    /// Verweis auf ein Objekt in den Konstruktionsblöcken.
    #[serde(rename = "@idObject")]
    pub id_object: String,
    /// `NodePoint`, `NodeSpline` oder `NodeSplinePath`.
    #[serde(rename = "@type")]
    pub kind: String,
    /// `1` kehrt die Laufrichtung um — für die Orientierung entscheidend.
    #[serde(rename = "@reverse", default)]
    pub reverse: Option<String>,
    /// Nahtzugabe **vor** dem Knoten — Formel in Mustereinheiten, nur an
    /// `NodePoint`. Fehlend, leer, ungültig oder negativ: die Breite des
    /// Teils (`vpiecenode.cpp`, `vabstractpiece.cpp`).
    #[serde(rename = "@before", default)]
    pub before: Option<String>,
    /// Kerbe (`true`): in 106 von 318 Knoten der Bluse gesetzt; nur an
    /// `NodePoint`, an ausgeschlossenen Knoten unterdrückt.
    #[serde(rename = "@notch", default)]
    pub notch: Option<String>,
    /// `slit`, `straightforward`, `vShape`, `dart`.
    #[serde(rename = "@notchType", default)]
    pub notch_type: Option<String>,
    #[serde(rename = "@notchSubtype", default)]
    pub notch_subtype: Option<String>,
    #[serde(rename = "@notchLength", default)]
    pub notch_length: Option<String>,
    #[serde(rename = "@angle", default)]
    pub angle: Option<String>,
    /// Nahtzugabe **nach** dem Knoten, sonst wie `before`.
    #[serde(rename = "@after", default)]
    pub after: Option<String>,
    /// Ausgeschlossen (`excluded="true"`): weder Kante noch Nachbar noch Kerbe.
    #[serde(rename = "@excluded", alias = "@exclude", default)]
    pub exclude: Option<String>,
    #[serde(rename = "@firstToFirst", default)]
    pub first_to_first: Option<String>,
    #[serde(rename = "@firstToSecond", default)]
    pub first_to_second: Option<String>,
    #[serde(rename = "@secondToFirst", default)]
    pub second_to_first: Option<String>,
    #[serde(rename = "@secondToSecond", default)]
    pub second_to_second: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CustomSeamAllowance {
    #[serde(rename = "node", default)]
    pub items: Vec<CsaNode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CsaNode {
    #[serde(rename = "@idObject", default)]
    pub id_object: Option<String>,
    #[serde(rename = "@type", default)]
    pub kind: Option<String>,
    /// `straight`, `bisector`, `consecutive`, `fixedLength`.
    #[serde(rename = "@typeLine", default)]
    pub type_line: Option<String>,
    #[serde(rename = "@angle", default)]
    pub angle: Option<String>,
    #[serde(rename = "@chordLength", default)]
    pub chord_length: Option<String>,
}

// ─────────────────────────────────────────────────────────── Zugriff ──

impl Pattern {
    /// Zahl der Konstruktionspunkte über alle Blöcke.
    pub fn point_count(&self) -> usize {
        self.draft_blocks
            .iter()
            .map(|b| {
                b.calculation.as_ref().map_or(0, |c| c.points.len())
                    + b.modeling.as_ref().map_or(0, |m| m.points.len())
            })
            .sum()
    }

    /// Zahl der Splines (inklusive Pfadsplines) über alle Blöcke.
    pub fn spline_count(&self) -> usize {
        self.draft_blocks
            .iter()
            .map(|b| {
                b.calculation.as_ref().map_or(0, |c| c.splines.len())
                    + b.modeling.as_ref().map_or(0, |m| m.splines.len())
            })
            .sum()
    }

    /// Alle Punkttypen im Muster mit Häufigkeit, absteigend sortiert.
    pub fn point_kinds(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for b in &self.draft_blocks {
            if let Some(c) = &b.calculation {
                for p in &c.points {
                    *counts.entry(p.kind.as_str()).or_default() += 1;
                }
            }
            if let Some(m) = &b.modeling {
                for p in &m.points {
                    *counts.entry(p.kind.as_str()).or_default() += 1;
                }
            }
        }
        let mut v: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(k, n)| (k.to_owned(), n))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }

    /// Teile in Dateireihenfolge, über alle Blöcke.
    ///
    /// Die Stücke liegen im Dokument je Block; für die Auswertung ist die
    /// flache Liste die gebräuchliche Form.
    pub fn pieces(&self) -> Vec<&Piece> {
        self.draft_blocks
            .iter()
            .filter_map(|b| b.pieces.as_ref())
            .flat_map(|p| p.items.iter())
            .collect()
    }

    /// Teile nach Name.
    pub fn piece_names(&self) -> Vec<&str> {
        self.pieces().into_iter().filter_map(|p| p.name.as_deref()).collect()
    }

    /// Rechnet die Einheit des Musters in Millimeter um.
    pub fn unit_to_mm(&self) -> Result<f64, crate::PatternError> {
        match self.unit.as_str() {
            "mm" => Ok(1.0),
            "cm" => Ok(10.0),
            "inch" => Ok(25.4),
            other => Err(crate::PatternError::UnsupportedUnit(other.to_owned())),
        }
    }
}