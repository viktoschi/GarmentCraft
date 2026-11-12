//! Einlesen von Seamly2D-Mustern.
//!
//! ## Warum ein eigener Ereignisparser statt `quick_xml::de::from_str`
//!
//! `quick-xmls` `serde`-Ableitung stolpert über dieses Format:
//!
//! 1. Attribute landen nur mit `@`-Präfix im `rename`-Namen, sonst werden sie
//!    kommentarlos ignoriert.
//! 2. Sobald ein Kindelement **selbst** Kindelemente trägt (`<spline>` mit
//!    `<pathPoint>`), geht der Feldzustand des Elternelements verloren und das
//!    nächste Geschwister-Element meldet `duplicate field`.
//!
//! Beides trifft hier den Normalfall, nicht eine Ausnahme. Der Parser
//! arbeitet deshalb direkt auf der Ereignisebene. Das ist zugleich schneller
//! und lässt sich gegen die echten Muster prüfen.
//!
//! ## Aufbau
//!
//! ```text
//! <pattern>
//!   <version>0.6.8</version>          <- Textknoten
//!   <unit>cm</unit>
//!   <measurements>…</measurements>
//!   <increments><increment name= formula=/></increments>
//!   <draftBlock>
//!     <calculation>… point | line | spline | arc, Spline mit <pathPoint>
//!     <modeling>…   point | spline
//!   </draftBlock>
//!   <pieces><piece><nodes><node/></nodes><CSA><node/></CSA></piece></pieces>
//! </pattern>
//! ```

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};

use crate::model::{
    Arc, Calculation, CsaNode, CustomSeamAllowance, DraftBlock, Grainline, Increments, Increment,
    InnerPath, InnerPathNode, InnerPathNodes, InnerPathRef, InnerPathRefs, Line, Modeling,
    Operation, PathPoint, Pattern, Piece, PieceNode, PieceNodes, Point, Spline,
};
use crate::PatternError;

type M = HashMap<String, String>;

fn mstr(m: &M, key: &str) -> Option<String> {
    m.get(key).cloned()
}

/// Baut eine Struktur aus einer Attributliste. Nicht genannte Felder bleiben
/// auf `None` (bzw. `""` bei Pflichtfeldern).
macro_rules! from_map {
    ($ty:ident, $m:expr, $($f:ident => $k:literal),* $(,)?) => {
        $ty { $($f: mstr($m, $k),)* ..Default::default() }
    };
}

fn attrs(e: &BytesStart<'_>) -> M {
    e.attributes()
        .flatten()
        .map(|a| {
            let k = String::from_utf8_lossy(a.key.as_ref().as_bytes()).into_owned();
            // **Entitäten müssen aufgelöst werden.** Formeln enthalten
            // Vergleiche als Zeichen: `<variable formula="… (#X &gt; #Y) …"/>`.
            // Ohne `unescape` steht da wörtlich `&gt;` da, und die
            // Formel-Engine meldet ein unerwartetes `&`.
            let v = match quick_xml::escape::unescape(&String::from_utf8_lossy(
                a.value.as_ref().as_bytes(),
            )) {
                Ok(s) => s.into_owned(),
                Err(_) => String::from_utf8_lossy(a.value.as_ref().as_bytes()).into_owned(),
            };
            (k, v)
        })
        .collect()
}

/// `LocalName` dereferenziert zu `str` — daher lässt sich der Name direkt
/// als `&str` verwenden.
fn name_of(e: &BytesStart<'_>) -> String {
    e.local_name().as_ref().to_owned()
}

/// Wohin ein `<spline>` gehört.
#[derive(Clone, Copy, PartialEq)]
enum Fach {
    Calc,
    Model,
}

/// Welches Container-Element gerade offen ist.
#[derive(Clone)]
enum Ctx {
    Calc,
    Model,
    Variablen,
    /// Innerhalb von `<pieces>`: Index des zugehörigen Blocks.
    Pieces(usize),
    Nodes(usize),
    Csa(usize),
    /// Innerhalb von `<spline>`: Pfadpunkte sammeln.
    Spline(Fach, usize),
    /// Innerhalb eines `<node>`-Paars (`<node></node>` statt `<node/>`).
    Node,
    /// Innerhalb eines `<path>` im `<modeling>`: Stelle des Pfades in
    /// `Modeling::paths` des zuletzt geöffneten Blocks.
    InnerPfad(usize),
    /// Innerhalb der `<nodes>` eines Innenpfades: derselbe Index.
    InnereKnoten(usize),
    /// Innerhalb der `<iPaths>` eines Teils: die Verweise sammeln.
    Ipaths,
    /// Innerhalb von `<operation>`; in `<source>` (false) bzw.
    /// `<destination>` (true) die `<item>`-Liste.
    Operation,
    OpListe(bool),
}

/// Liegt der Parse im Rechen- oder Modellier-Teil eines Blocks?
///
/// `<line>` und `<point>` kommen auch im `<patternLabel>` vor; nur hier sind
/// sie Konstruktionsobjekte.
fn in_konstruktion(stapel: &[Ctx]) -> Option<bool> {
    stapel.iter().rev().find_map(|c| match c {
        Ctx::Model => Some(true),
        Ctx::Calc => Some(false),
        _ => None,
    })
}

/// Hängt einen `<node>` an das passende Teile-Element.
///
/// `<nodes>` und `<CSA>` tragen beide `<node>`; unterschieden werden sie über
/// den umgebenden Kontext.
fn knoten_anhaengen(stapel: &[Ctx], teile: &mut [Piece], m: &M) {
    let ziel = stapel.iter().rev().find_map(|c| match c {
        Ctx::Nodes(i) => Some((*i, false)),
        Ctx::Csa(i) => Some((*i, true)),
        _ => None,
    });
    let Some((i, ist_csa)) = ziel else { return };
    // `<nodes>` ohne vorausgehendes `<piece>` kommt in fremd erzeugten
    // Dateien vor; dann gibt es nichts anzuhängen.
    let Some(t) = teile.get_mut(i) else { return };
    if ist_csa {
        t.csa.get_or_insert_with(CustomSeamAllowance::default)
            .items
            .push(from_map!(CsaNode, m,
                id_object => "idObject", kind => "type",
                type_line => "typeLine", angle => "angle",
                chord_length => "chordLength"));
    } else {
        let mut n = from_map!(PieceNode, m,
            reverse => "reverse", before => "before", after => "after",
            exclude => "excluded",
            notch => "notch", notch_type => "notchType",
            notch_subtype => "notchSubtype", notch_length => "notchLength",
            angle => "angle",
            first_to_first => "firstToFirst",
            first_to_second => "firstToSecond",
            second_to_first => "secondToFirst",
            second_to_second => "secondToSecond");
        // Seamly2D schreibt `excluded` (`vabstractpattern.cpp: AttrNodeExcluded`);
        // `exclude` lehnt es als unbekanntes Attribut ab. Hier trotzdem
        // angenommen, weil fremd erzeugte Dateien es so schreiben könnten.
        if n.exclude.is_none() {
            n.exclude = mstr(m, "exclude");
        }
        // Pflichtfelder einer Konturkette.
        n.id_object = mstr(m, "idObject").unwrap_or_default();
        n.kind = mstr(m, "type").unwrap_or_default();
        t.nodes
            .get_or_insert_with(PieceNodes::default)
            .items
            .push(n);
    }
}

/// Nimmt ein geschlossenes Container-Element vom Kontextstapel.
///
/// Leere `Ctx`-Varianten werden ignoriert: `calc` und `model` tragen keine
/// Indexinformation und werden ohnehin nur zum Unterscheiden geführt.
fn schliesse(name: &str, stapel: &mut Vec<Ctx>) {
    if matches!(
        name,
        "spline" | "nodes" | "CSA" | "node" | "draftBlock" | "calculation" | "modeling"
            | "variables" | "pieces"
    ) {
        stapel.pop();
    }
    // Nur schließen, was geöffnet wurde: `<path>` außerhalb des Modellierteils
    // und `<source>`/`<destination>` anderswo legen nichts ab.
    let offen = match name {
        "path" => matches!(stapel.last(), Some(Ctx::InnerPfad(_))),
        "iPaths" => matches!(stapel.last(), Some(Ctx::Ipaths)),
        "operation" => matches!(stapel.last(), Some(Ctx::Operation)),
        "source" | "destination" => matches!(stapel.last(), Some(Ctx::OpListe(_))),
        _ => false,
    };
    if offen {
        stapel.pop();
    }
}

/// Liest eine `.sm2d`-Datei vom Pfad.
pub fn read_path(path: &std::path::Path) -> Result<Pattern, PatternError> {
    let s = std::fs::read_to_string(path)
        .map_err(|e| PatternError::Xml(format!("{}: {e}", path.display())))?;
    read_str(&s)
}

/// Liest eine `.sm2d`-Datei aus dem Speicher.
pub fn read_str(xml: &str) -> Result<Pattern, PatternError> {
    parse(xml)
}

fn parse(xml: &str) -> Result<Pattern, PatternError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    // Standard: passende End-Tags erzwingen. Ohne das akzeptiert der Leser
    // auch abgeschnittenes XML, was stillschweigend falsche Muster ergaebe.

    let mut pat = Pattern::default();
    let mut bloecke: Vec<DraftBlock> = Vec::new();
    let mut incr: Vec<Increment> = Vec::new();
    let mut teile: Vec<Piece> = Vec::new();
    // Blockindex je Stueck — <pieces> steht innerhalb von <draftBlock>.
    let mut teile_block: Vec<usize> = Vec::new();
    let mut stapel: Vec<Ctx> = Vec::new();

    // Textknoten, der gerade erwartet wird.
    let mut textfeld: Option<&'static str> = None;

    // Verschachtelungstiefe unabhaengig vom Kontextstapel: damit fallen auch
    // abgebrochene Dokumente auf, deren Wurzel (``<pattern>``) im Stapel
    // gar nicht auftaucht.
    let mut tiefe: i32 = 0;

    loop {
        let ev = reader
            .read_event()
            .map_err(|e| PatternError::Xml(e.to_string()))?;

        // Selbstschliessende Elemente kommen als `Empty` und erzeugen kein
        // Ende-Ereignis. Sie werden wie ein Start behandelt und das Ende
        // direkt nachgeliefert — sonst verschiebt sich der Kontextstapel.
        let war_leer = matches!(ev, Event::Empty(_));
        let ev = match ev {
            Event::Empty(e) => Event::Start(e),
            andere => andere,
        };

        match ev {
            Event::Start(ref e) => {
                tiefe += 1;
                let m = attrs(e);
                let name = name_of(e);
                // Das Label ist noetig: ein `break`/`continue` im Zweig
                // wuerde sonst die Tiefenkorrektur fuer selbstschliessende
                // Elemente ueberspringen und die Zaehlung verfaelschen.
                'start: {
                match name.as_str() {
                    // ── Textknoten ──
                    "version" => textfeld = Some("version"),
                    "unit" => textfeld = Some("unit"),
                    "measurements" => textfeld = Some("measurements"),

                    // ── Messvariablen ──
                    "variables" => stapel.push(Ctx::Variablen),
                    "variable" => {
                        let mut i = from_map!(Increment, &m, description => "description");
                        i.name = mstr(&m, "name").unwrap_or_default();
                        i.formula = mstr(&m, "formula").unwrap_or_default();
                        incr.push(i);
                    }

                    // ── Konstruktion ──
                    "draftBlock" => {
                        let mut blk = DraftBlock::default();
                        blk.name = mstr(&m, "name");
                        bloecke.push(blk);
                    },
                    "calculation" => stapel.push(Ctx::Calc),
                    "modeling" => stapel.push(Ctx::Model),

                    "point" => {
                        let mut p = from_map!(Point, &m,
                            name => "name", in_use => "inUse",
                            x => "x", y => "y",
                            first_point => "firstPoint", second_point => "secondPoint",
                            third_point => "thirdPoint", base_point => "basePoint",
                            length => "length", angle => "angle", radius => "radius",
                            center => "center",
                            first_arc => "firstArc", second_arc => "secondArc",
                            axis_point1 => "axisPoint1", axis_point2 => "axisPoint2",
                            point_of_contact => "pointOfContact",
                            first_contact => "firstContact",
                            second_contact => "secondContact",
                            p1_line => "p1Line", p2_line => "p2Line",
                            p1_line1 => "p1Line1", p2_line1 => "p2Line1",
                            p1_line2 => "p1Line2", p2_line2 => "p2Line2",
                            direction => "direction",
                            spline => "spline", spline_path => "splinePath",
                            curve => "curve", arc => "arc",
                            curve1 => "curve1", curve2 => "curve2",
                            cross_point => "crossPoint",
                            v_cross_point => "vCrossPoint", h_cross_point => "hCrossPoint",
                            base_line_p1 => "baseLineP1", base_line_p2 => "baseLineP2",
                            dart_p1 => "dartP1", dart_p2 => "dartP2", dart_p3 => "dartP3",
                            point1 => "point1", point2 => "point2",
                            name1 => "name1", name2 => "name2",
                            id_object => "idObject", type_object => "typeObject",
                            id_tool => "idTool", line_type => "lineType",
                            mx => "mx", my => "my");
                        // `id` und `type` sind Pflicht, sonst ist der Punkt
                        // nicht auflösbar.
                        p.id = mstr(&m, "id").unwrap_or_default();
                        p.kind = mstr(&m, "type").unwrap_or_default();
                        // `<point>` steht auch im Label-Bereich; nur im
                        // Rechen- oder Modellier-Teil ist es ein Objekt.
                        let Some(im_modeling) = in_konstruktion(&stapel) else {
                            break 'start;
                        };
                        if p.id.is_empty() {
                            break 'start;
                        }
                        let b = bloecke.last_mut().ok_or_else(|| {
                            PatternError::Xml("Punkt ausserhalb eines draftBlock".into())
                        })?;
                        if im_modeling {
                            b.modeling.get_or_insert_with(Modeling::default).points.push(p);
                        } else {
                            b.calculation
                                .get_or_insert_with(Calculation::default)
                                .points
                                .push(p);
                        }
                    }

                    "line" => {
                        // `<line>` gibt es auch im `<patternLabel>` — dort ist
                        // es eine Textzeile, keine Konstruktionslinie.
                        if in_konstruktion(&stapel) != Some(false) {
                            break 'start;
                        }
                        let mut l = from_map!(Line, &m,
                            first_point => "firstPoint",
                            second_point => "secondPoint", type_line => "typeLine");
                        l.id = mstr(&m, "id").unwrap_or_default();
                        bloecke
                            .last_mut()
                            .ok_or_else(|| {
                                PatternError::Xml("Linie ausserhalb eines draftBlock".into())
                            })?
                            .calculation
                            .get_or_insert_with(Calculation::default)
                            .lines
                            .push(l);
                    }

                    "spline" => {
                        let mut s = from_map!(Spline, &m,
                            kind => "type", name => "name",
                            point1 => "point1", point4 => "point4",
                            point2 => "point2", point3 => "point3",
                            point1_line => "point1Line", point2_line => "point2Line",
                            angle1 => "angle1", angle2 => "angle2",
                            length1 => "length1", length2 => "length2",
                            c_radius => "cRadius", in_use => "inUse",
                            id_object => "idObject", duplicate => "duplicate");
                        // `id` ist Pflicht: ohne sie ist die Kurve nicht
                        // auflösbar und alle Splines fielen auf einen Eintrag
                        // zusammen.
                        s.id = mstr(&m, "id").unwrap_or_default();
                        if s.id.is_empty() {
                            break 'start;
                        }
                        let b = bloecke.last_mut().ok_or_else(|| {
                            PatternError::Xml("Spline ausserhalb eines draftBlock".into())
                        })?;
                        let (fach, idx) = match in_konstruktion(&stapel) {
                            Some(true) => {
                                let v =
                                    &mut b.modeling.get_or_insert_with(Modeling::default).splines;
                                v.push(s);
                                (Fach::Model, v.len() - 1)
                            }
                            Some(false) => {
                                let v = &mut b
                                    .calculation
                                    .get_or_insert_with(Calculation::default)
                                    .splines;
                                v.push(s);
                                (Fach::Calc, v.len() - 1)
                            }
                            None => break 'start, // Spline ausserhalb der Konstruktion
                        };
                        stapel.push(Ctx::Spline(fach, idx));
                    }

                    "pathPoint" => {
                        let pp = from_map!(PathPoint, &m,
                            id => "id", p_spline => "pSpline", kind => "type",
                            angle => "angle", length => "length",
                            point1 => "point1", point4 => "point4",
                            angle1 => "angle1", angle2 => "angle2",
                            length1 => "length1", length2 => "length2",
                            exclude => "exclude");
                        if let (Some(Ctx::Spline(f, i)), Some(b)) =
                            (stapel.last().cloned(), bloecke.last_mut())
                        {
                            match f {
                                Fach::Calc => {
                                    b.calculation.as_mut().unwrap().splines[i]
                                        .path_points.push(pp)
                                }
                                Fach::Model => {
                                    b.modeling.as_mut().unwrap().splines[i]
                                        .path_points.push(pp)
                                }
                            }
                        }
                    }

                    "arc" => {
                        let mut a = from_map!(Arc, &m,
                            kind => "type", center => "center",
                            angle1 => "angle1", angle2 => "angle2",
                            radius => "radius", length => "length",
                            point1 => "point1", point2 => "point2",
                            in_use => "inUse", id_object => "idObject");
                        a.id = mstr(&m, "id").unwrap_or_default();
                        if a.id.is_empty() {
                            // `simple` ist zugleich der Name eines Bogentyps
                            // und einer Punkteigenschaft. Ohne `center` ist es
                            // kein Bogen, sondern ein Rechenpunkt.
                            break 'start;
                        }
                        let b = bloecke.last_mut().ok_or_else(|| {
                            PatternError::Xml("Bogen ausserhalb eines draftBlock".into())
                        })?;
                        // `type="modeling"` ist eine Kopie, keine Berechnung.
                        // Im Bustier sind 8 der 154 Modellierobjekte Boegen —
                        // ohne sie reisst die Konturkette genau dort.
                        match in_konstruktion(&stapel) {
                            Some(true) => {
                                b.modeling.get_or_insert_with(Modeling::default).arcs.push(a)
                            }
                            _ => b
                                .calculation
                                .get_or_insert_with(Calculation::default)
                                .arcs
                                .push(a),
                        }
                    }

                    // ── Operationen ──
                    //
                    // `<operation>` steht im Rechenteil; `<source>` und
                    // `<destination>` tragen je eine `<item idObject=…>`-Liste.
                    "operation" => {
                        if in_konstruktion(&stapel) != Some(false) {
                            break 'start;
                        }
                        let op = Operation {
                            id: mstr(&m, "id").unwrap_or_default(),
                            kind: mstr(&m, "type").unwrap_or_default(),
                            suffix: mstr(&m, "suffix").unwrap_or_default(),
                            center: mstr(&m, "center"),
                            angle: mstr(&m, "angle"),
                            length: mstr(&m, "length"),
                            axis_type: mstr(&m, "axisType"),
                            p1_line: mstr(&m, "p1Line"),
                            p2_line: mstr(&m, "p2Line"),
                            ..Operation::default()
                        };
                        bloecke
                            .last_mut()
                            .ok_or_else(|| {
                                PatternError::Xml("Operation ausserhalb eines draftBlock".into())
                            })?
                            .calculation
                            .get_or_insert_with(Calculation::default)
                            .operations
                            .push(op);
                        stapel.push(Ctx::Operation);
                    }
                    "source" | "destination" if matches!(stapel.last(), Some(Ctx::Operation)) => {
                        stapel.push(Ctx::OpListe(name == "destination"));
                    }
                    "item" => {
                        let Some(Ctx::OpListe(ziel)) = stapel.last().cloned() else {
                            break 'start;
                        };
                        let Some(id) = mstr(&m, "idObject") else { break 'start };
                        if let Some(op) = bloecke
                            .last_mut()
                            .and_then(|b| b.calculation.as_mut())
                            .and_then(|c| c.operations.last_mut())
                        {
                            if ziel {
                                op.ziele.push(id);
                            } else {
                                op.quellen.push(id);
                            }
                        }
                    }

                    // ── Innenpfade ──
                    //
                    // `<path>` steht im `<modeling>` und ist **keine**
                    // Nahtlinie: Klebezonen, Verzierungen, ein auszuschneidendes
                    // Loch. Welches Teil ihn trägt, sagt dessen `<iPaths>`.
                    "path" => {
                        if in_konstruktion(&stapel) != Some(true) {
                            break 'start;
                        }
                        let mut pfad = from_map!(InnerPath, &m, name => "name",
                            cut => "cut", kind => "type");
                        pfad.id = mstr(&m, "id").unwrap_or_default();
                        let Some(b) = bloecke.last_mut() else {
                            return Err(PatternError::Xml(
                                "Innenpfad ausserhalb eines draftBlock".into(),
                            ));
                        };
                        let v = &mut b.modeling.get_or_insert_with(Modeling::default).paths;
                        v.push(pfad);
                        stapel.push(Ctx::InnerPfad(v.len() - 1));
                    }

                    // ── Schnittteile ──
                    //
                    // `<pieces>` steht **innerhalb** von `<draftBlock>`; die
                    // Zuordnung erfolgt deshalb ueber den Index des
                    // geoeffneten Blocks.
                    "pieces" => match bloecke.len().checked_sub(1) {
                        Some(i) => stapel.push(Ctx::Pieces(i)),
                        None => return Err(PatternError::Xml(
                            "<pieces> ausserhalb eines draftBlock".into(),
                        )),
                    },

                    "piece" => {
                        let mut t = from_map!(Piece, &m,
                            name => "name",
                            seam_allowance => "seamAllowance",
                            seam_allowance_built_in => "seamAllowanceBuiltIn",
                            width => "width", version => "version",
                            closed => "closed", mx => "mx", my => "my",
                            id_source => "idSource", in_layout => "inLayout");
                        t.id = mstr(&m, "id").unwrap_or_default();
                        teile.push(t);
                        teile_block.push(
                            stapel
                                .iter()
                                .rev()
                                .find_map(|c| match c {
                                    Ctx::Pieces(i) => Some(*i),
                                    _ => None,
                                })
                                .unwrap_or(0),
                        );
                    }

                    // Nur `<nodes>` eines Teils gehört zur Nahtlinie. Innenpfade
                    // (`<modeling><path><nodes>`) sind eigene Objekte; bis
                    // 20.10.2026 hingen ihre Knoten am zuletzt gelesenen Teil —
                    // daher der Bund des Rocks v34 mit 22 m Naht.
                    "nodes" => stapel.push(match stapel.last() {
                        Some(Ctx::InnerPfad(i)) => Ctx::InnereKnoten(*i),
                        _ => Ctx::Nodes(
                            if stapel.iter().any(|c| matches!(c, Ctx::Pieces(_))) {
                                teile.len().saturating_sub(1)
                            } else {
                                usize::MAX
                            },
                        ),
                    }),
                    "CSA" => stapel.push(Ctx::Csa(teile.len().saturating_sub(1))),
                    "node" => {
                        // Ein Knoten eines Innenpfades hängt an diesem Pfad,
                        // nicht an einem Teil.
                        match stapel.last() {
                            Some(Ctx::InnereKnoten(i)) => {
                                let mut n = from_map!(InnerPathNode, &m, reverse => "reverse");
                                n.id_object = mstr(&m, "idObject").unwrap_or_default();
                                n.kind = mstr(&m, "type").unwrap_or_default();
                                if let Some(b) = bloecke.last_mut() {
                                    if let Some(p) = b
                                        .modeling
                                        .as_mut()
                                        .and_then(|mo| mo.paths.get_mut(*i))
                                    {
                                        p.nodes
                                            .get_or_insert_with(InnerPathNodes::default)
                                            .items
                                            .push(n);
                                    }
                                }
                            }
                            _ => knoten_anhaengen(&stapel, &mut teile, &m),
                        }
                        stapel.push(Ctx::Node);
                    }

                    // `<iPaths>` eines Teils: nur Verweise auf Innenpfade.
                    "iPaths" => {
                        if !stapel.iter().any(|c| matches!(c, Ctx::Pieces(_))) {
                            break 'start;
                        }
                        if let Some(teil) = teile.last_mut() {
                            teil.i_paths.push(InnerPathRefs::default());
                        }
                        stapel.push(Ctx::Ipaths);
                    }
                    "record" => {
                        if !matches!(stapel.last(), Some(Ctx::Ipaths)) {
                            break 'start;
                        }
                        let Some(ziel) = mstr(&m, "path") else { break 'start };
                        if let Some(teil) = teile.last_mut() {
                            if let Some(block) = teil.i_paths.last_mut() {
                                block.paths.push(InnerPathRef { path: ziel });
                            }
                        }
                    }

                    // The piece's grainline — self-closing, attributes only,
                    // taken over verbatim. Outside `<pieces>` it attaches to
                    // no piece.
                    "grainline" => {
                        if stapel.iter().any(|c| matches!(c, Ctx::Pieces(_))) {
                            let Some(teil) = teile.last_mut() else { break 'start };
                            teil.grainline = Some(from_map!(Grainline, &m,
                                visible => "visible", arrows => "arrows",
                                rotation => "rotation", length => "length",
                                arrow_length => "arrowLength",
                                mx => "mx", my => "my",
                                top_anchor => "topAnchor",
                                bottom_anchor => "bottomAnchor",
                                center_anchor => "centerAnchor"));
                        }
                    }

                    _ => {}
                }
                }

                if war_leer {
                    // An empty text element (`<measurements/>`) has no text;
                    // left open, it would take the next text of the file.
                    textfeld = None;
                    schliesse(&name, &mut stapel);
                    tiefe -= 1;
                }
            }

            Event::End(ref e) => {
                textfeld = None;
                tiefe -= 1;
                schliesse(&name_of_end(e), &mut stapel);
            }

            Event::Text(ref t) => {
                if let Some(feld) = textfeld {
                    let roh: &str = t.as_ref();
                    let wert = quick_xml::escape::unescape(roh)
                        .map_err(|e| PatternError::Xml(e.to_string()))?;
                    match feld {
                        "version" => pat.version = wert.trim().to_owned(),
                        "unit" => pat.unit = wert.trim().to_owned(),
                        "measurements" => {
                            pat.measurements = Some(wert.trim().to_owned()).filter(|w| !w.is_empty())
                        }
                        _ => {}
                    }
                    textfeld = None;
                }
            }

            Event::Eof => {
                // quick-xml meldet offene Elemente am Dateiende nicht von
                // selbst; ein abgeschnittenes Muster waere sonst stillschweigend
                // unvollstaendig.
                if tiefe != 0 || !stapel.is_empty() {
                    return Err(PatternError::Xml(format!(
                        "unvollstaendiges XML: {tiefe} Element(e) offen, Stapel: {}",
                        stapel.len()
                    )));
                }
                break;
            }
            _ => {}
        }
    }

    pat.increments = (!incr.is_empty()).then_some(Increments { items: incr });

    // Die Stuecke gehoeren zu ihrem Block, nicht zum Muster als Ganzes.
    let mut rest = teile.into_iter();
    for block_idx in &teile_block {
        let stueck = rest
            .next()
            .ok_or_else(|| PatternError::Xml("Stueck ohne zugehoerigen Block".into()))?;
        bloecke
            .get_mut(*block_idx)
            .ok_or_else(|| PatternError::Xml("Stueck verweist auf unbekannten Block".into()))?
            .pieces
            .get_or_insert_with(crate::model::Pieces::default)
            .items
            .push(stueck);
    }
    pat.draft_blocks = bloecke;
    Ok(pat)
}

fn name_of_end(e: &quick_xml::events::BytesEnd<'_>) -> String {
    e.local_name().as_ref().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hinweis: `r##` statt `r#`, weil Formeln wie "#Bust/2" sonst den
    // Raw-String vorzeitig schliessen wuerden.
    const MINIMAL: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
    <version>0.6.8</version>
    <unit>cm</unit>
    <measurements>/pfad/measurements.smis</measurements>
    <variables>
        <variable name="#Hoehe" formula="#Bust/2" description="Testvariable"/>
        <variable name="Bust2" formula="10+5"/>
    </variables>
    <draftBlock name="Block1">
        <calculation>
            <point id="1" mx="0.2" my="-1.0" name="A" type="single" x="1.0" y="2.0"/>
            <point id="2" angle="270" basePoint="1" length="#Hemline" type="endLine"/>
            <point id="3" firstPoint="1" secondPoint="2" length="#ArmSkyeHeightBack"
                   type="alongLine"/>
            <line id="10" firstPoint="1" secondPoint="2" typeLine="none"/>
            <spline id="20" type="simpleInteractive" point1="1" point4="2">
                <pathPoint id="21" pSpline="20" type="simpleInteractive" angle="90"
                           length="5"/>
                <pathPoint id="22" pSpline="20" type="simpleInteractive" angle="90"
                           length="8"/>
            </spline>
            <arc id="30" type="startOfCurve" center="1" angle1="0" angle2="90"
                 radius="4"/>
            <point id="4" type="single" x="9.0" y="9.0"/>
        </calculation>
        <modeling>
            <point id="40" idObject="1" type="modeling" idTool="rotate" inUse="true"
                   mx="1.5"/>
            <spline id="41" idObject="19" type="modelingPath" inUse="true"/>
        </modeling>
        <pieces>
            <piece id="174" name="Yoke" seamAllowance="1" version="2" width="1" closed="1">
                <nodes>
                    <node idObject="1" type="NodePoint"/>
                    <node idObject="20" reverse="0" type="NodeSplinePath"/>
                    <node idObject="2" type="NodePoint" notch="true" notchType="slit"
                          notchSubtype="straightforward" notchLength="0.6"/>
                </nodes>
                <CSA>
                    <node idObject="1" type="NodePoint" typeLine="none"/>
                </CSA>
            </piece>
        </pieces>
    </draftBlock>
</pattern>"##;

    #[test]
    fn liest_Grundstruktur() {
        let p = read_str(MINIMAL).expect("muss lesen");
        assert_eq!(p.version, "0.6.8");
        assert_eq!(p.unit, "cm");
        assert_eq!(p.measurements.as_deref(), Some("/pfad/measurements.smis"));
        assert_eq!(p.draft_blocks.len(), 1);
        assert_eq!(p.draft_blocks[0].name.as_deref(), Some("Block1"));
        assert_eq!(p.piece_names(), vec!["Yoke"]);
        // Stuecke liegen am Block, nicht am Muster.
        assert_eq!(p.draft_blocks[0].pieces.as_ref().unwrap().items.len(), 1);
        assert_eq!(p.unit_to_mm().unwrap(), 10.0);
    }

    /// `<measurements/>` names no file. It used to take the next text of the
    /// file instead (a pattern read "…\1540" as its measurement file).
    #[test]
    fn empty_measurements_name_no_file() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<pattern>
    <version>0.7.2</version>
    <unit>mm</unit>
    <description>text</description>
    <measurements/>
    <patternNumber>1540</patternNumber>
    <variables><variable name="#A" formula="1"/></variables>
</pattern>"##;
        let p = read_str(xml).expect("must read");
        assert_eq!(p.measurements, None);
        assert_eq!(p.unit, "mm");
        let xml = MINIMAL.replace("<measurements>/pfad/measurements.smis</measurements>", "<measurements> </measurements>");
        assert_eq!(read_str(&xml).unwrap().measurements, None);
    }

    #[test]
    fn liest_Increments_mit_Formeln() {
        let p = read_str(MINIMAL).unwrap();
        let inc = p.increments.as_ref().unwrap();
        assert_eq!(inc.items.len(), 2);
        // Der Name traegt das fuehrende # selbst.
        assert_eq!(inc.items[0].name, "#Hoehe");
        assert_eq!(inc.items[0].formula, "#Bust/2");
        assert_eq!(inc.items[0].description.as_deref(), Some("Testvariable"));
        assert_eq!(inc.items[1].formula, "10+5");
    }

    #[test]
    fn liest_Punkte_mit_Konstruktion() {
        let p = read_str(MINIMAL).unwrap();
        let calc = p.draft_blocks[0].calculation.as_ref().unwrap();
        assert_eq!(calc.points.len(), 4);
        assert_eq!(calc.points[0].kind, "single");
        assert_eq!(calc.points[0].x.as_deref(), Some("1.0"));
        assert_eq!(calc.points[0].name.as_deref(), Some("A"));
        assert_eq!(calc.points[1].kind, "endLine");
        assert_eq!(calc.points[1].base_point.as_deref(), Some("1"));
        assert_eq!(calc.points[1].length.as_deref(), Some("#Hemline"));
        assert_eq!(calc.points[2].kind, "alongLine");
        assert_eq!(calc.points[2].length.as_deref(), Some("#ArmSkyeHeightBack"));
        assert_eq!(calc.lines.len(), 1);
        assert_eq!(calc.lines[0].type_line.as_deref(), Some("none"));
        assert_eq!(calc.arcs.len(), 1);
        assert_eq!(calc.arcs[0].radius.as_deref(), Some("4"));
    }

    #[test]
    fn liest_Spline_mit_Pfadpunkten() {
        let p = read_str(MINIMAL).unwrap();
        let calc = p.draft_blocks[0].calculation.as_ref().unwrap();
        assert_eq!(calc.splines.len(), 1);
        let s = &calc.splines[0];
        assert_eq!(s.point1.as_deref(), Some("1"));
        assert_eq!(s.path_points.len(), 2, "zwei <pathPoint> am Spline");
        assert_eq!(s.path_points[0].id.as_deref(), Some("21"));
        assert_eq!(s.path_points[1].length.as_deref(), Some("8"));
    }

    #[test]
    fn Geschwister_um_Kindelemente_bleiben_erhalten() {
        // Der Fall, an dem quick-xmls derive scheitert: <spline> mit Kindern
        // zwischen zwei <point>.
        let p = read_str(MINIMAL).unwrap();
        let calc = p.draft_blocks[0].calculation.as_ref().unwrap();
        let ids: Vec<&str> = calc.points.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "2", "3", "4"], "alle vier Punkte gelesen");
        assert_eq!(calc.splines.len(), 1, "Spline trotz Kindern erkannt");
    }

    #[test]
    fn trennt_Calculation_und_Modeling() {
        let p = read_str(MINIMAL).unwrap();
        let b = &p.draft_blocks[0];
        let calc = b.calculation.as_ref().unwrap();
        let m = b.modeling.as_ref().unwrap();
        assert_eq!(calc.points.len(), 4);
        assert_eq!(calc.splines.len(), 1);
        assert_eq!(m.points.len(), 1);
        assert_eq!(m.splines.len(), 1);
        assert_eq!(m.points[0].id_tool.as_deref(), Some("rotate"));
        assert_eq!(m.points[0].id_object.as_deref(), Some("1"));
        assert_eq!(m.points[0].in_use.as_deref(), Some("true"));
        assert_eq!(m.splines[0].kind.as_deref(), Some("modelingPath"));
        assert_eq!(m.splines[0].id_object.as_deref(), Some("19"));
    }

    #[test]
    fn liest_Nahtzugabe_richtig() {
        let p = read_str(MINIMAL).unwrap();
        let piece = p.pieces()[0];
        // seamAllowance ist Boolean, nicht die Breite
        assert_eq!(piece.seam_allowance.as_deref(), Some("1"));
        assert_eq!(piece.width.as_deref(), Some("1"));
        let nodes = &piece.nodes.as_ref().unwrap().items;
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].id_object, "1");
        assert_eq!(nodes[1].kind, "NodeSplinePath");
        assert_eq!(nodes[1].reverse.as_deref(), Some("0"));
        assert_eq!(nodes[2].notch.as_deref(), Some("true"));
        assert_eq!(nodes[2].notch_type.as_deref(), Some("slit"));
        assert_eq!(nodes[2].notch_length.as_deref(), Some("0.6"));
        let csa = &piece.csa.as_ref().unwrap().items;
        assert_eq!(csa.len(), 1);
        assert_eq!(csa[0].type_line.as_deref(), Some("none"));
    }

    #[test]
    fn zaehlt_Punkte_Typen_und_Splines() {
        let p = read_str(MINIMAL).unwrap();
        assert_eq!(p.point_count(), 5, "4 in calculation + 1 in modeling");
        assert_eq!(p.spline_count(), 2, "1 in calculation + 1 in modeling");
        let kinds = p.point_kinds();
        assert_eq!(
            kinds,
            vec![
                ("single".to_owned(), 2),
                ("alongLine".to_owned(), 1),
                ("endLine".to_owned(), 1),
                ("modeling".to_owned(), 1),
            ]
        );
    }

    #[test]
    fn meldet_falsches_XML() {
        assert!(read_str("<pattern><version>1.0</version>").is_err());
    }

    #[test]
    fn unbekannte_Elemente_stoeren_nicht() {
        let xml = MINIMAL.replace(
            "<increments>",
            "<patternLabel>Rückenteil</patternLabel><draw name=\"x\"/><variables>",
        );
        assert!(read_str(&xml).is_ok());
    }

    #[test]
    fn ohne_optionale_Abschnitte_liest_auch() {
        let xml = r#"<pattern><version>0.6.8</version><unit>mm</unit></pattern>"#;
        let p = read_str(xml).unwrap();
        assert!(p.increments.is_none());
        assert!(p.draft_blocks.is_empty());
        assert_eq!(p.unit_to_mm().unwrap(), 1.0);
    }
}