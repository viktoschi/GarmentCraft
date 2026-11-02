//! The tools (`tools/list`) and the commands of `command_run`, with their JSON schemas.
//!
//! Every command maps to one [`Aktion`] of the application (or closes the floating windows, as
//! Esc does). Dialog-only actions – open, choose a measurement file or seam list, quit, the
//! help windows – are left out: `open` and `suggest_seams` replace them.

use eframe::egui::Theme;
use klon3d_app::anordnung::{Bereich, Fenster};
use klon3d_app::app::Aktion;
use klon3d_app::panele::Reiter;
use serde_json::{json, Map, Value};

/// Why a call could not be served.
#[derive(Debug, Clone, PartialEq)]
pub enum Fehler {
    /// Unknown tool, missing or malformed argument: JSON-RPC error -32602.
    Parameter(String),
    /// The tool ran and failed (file missing, outside the roots, no adapter …): `isError`.
    Werkzeug(String),
}

/// One tool: name, description, input schema.
pub struct Werkzeug {
    pub name: &'static str,
    pub beschreibung: &'static str,
    pub schema: fn() -> Value,
}

fn leer() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

pub const WERKZEUGE: &[Werkzeug] = &[
    Werkzeug {
        name: "open",
        beschreibung:
            "Open a Seamly2D pattern (.sm2d) in the headless application, optionally with \
                       a measurement file (.smis, default: the one the pattern names) and a seam \
                       list (.naehte, default: <stem>.naehte next to the pattern). Paths must lie \
                       under a read root. Returns the state as doc_inspect does.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "muster": { "type": "string", "description": "Path of the .sm2d pattern" },
                    "massdatei": { "type": "string", "description": "Path of the .smis measurement file" },
                    "nahtliste": { "type": "string", "description": "Path of the .naehte seam list" }
                },
                "required": ["muster"],
                "additionalProperties": false
            })
        },
    },
    Werkzeug {
        name: "doc_inspect",
        beschreibung: "State of the application: pattern, measurement file, unit, pieces (index, \
                       name, perimeter cm, nodes), seam report (id, delta cm, status), selection, \
                       tab, 2D toggles, window arrangement, last message, simulation.",
        schema: leer,
    },
    Werkzeug {
        name: "command_list",
        beschreibung: "Commands for command_run: id, parameters (JSON schema) and a one-line doc.",
        schema: leer,
    },
    Werkzeug {
        name: "command_run",
        beschreibung: "Run one application command by id (see command_list) with its parameters, \
                       run one frame and return the new state as doc_inspect does.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Command id from command_list" },
                    "params": { "type": "object", "description": "Parameters of the command" }
                },
                "required": ["id"],
                "additionalProperties": false
            })
        },
    },
    Werkzeug {
        name: "render_preview",
        beschreibung: "Picture of the whole application window as PNG (default 1400 x 900 points, \
                       scale 1). Fails with 'kein wgpu-Adapter' when no graphics adapter exists. \
                       With 'pfad' the PNG is also written (under a write root).",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "breite": { "type": "number", "exclusiveMinimum": 0, "description": "Window width in points (default 1400)" },
                    "hoehe": { "type": "number", "exclusiveMinimum": 0, "description": "Window height in points (default 900)" },
                    "massstab": { "type": "number", "exclusiveMinimum": 0, "description": "Pixels per point (default 1)" },
                    "pfad": { "type": "string", "description": "Also write the PNG here" }
                },
                "additionalProperties": false
            })
        },
    },
    Werkzeug {
        name: "suggest_seams",
        beschreibung: "Suggest the seams of the pattern shown (as 'klon3d naehte --vorschlag'). \
                       Returns the seam list text; with 'schreiben' it is written there (under a \
                       write root) and the pattern is loaded again with it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "schreiben": { "type": "string", "description": "Path of the .naehte file to write" }
                },
                "additionalProperties": false
            })
        },
    },
    Werkzeug {
        name: "mesh_check",
        beschreibung: "Mesh the pattern shown with its checked seam list and report per piece \
                       (points, triangles, smallest angle, edge lengths) and the violations of \
                       the mesh acceptance.",
        schema: leer,
    },
    Werkzeug {
        name: "simulate",
        beschreibung: "Dress the pattern shown on a body: the first call builds the scene (mesh, \
                       body from the measurement file or 'koerper', placement from 'lage' or \
                       <stem>.lage, material), every call computes up to 'bilder' more frames \
                       (sew, settle, hold). Needs the body resizing folder (CLO_FEMALE). Returns \
                       frames, phase, timing, point count and the largest seam gap.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "bilder": { "type": "integer", "minimum": 1, "maximum": 100000, "description": "Frames to compute in this call" },
                    "koerper": { "type": "string", "description": "Body: a .smis to shape it from or a .zprj with a body (default: the pattern's measurement file)" },
                    "material": { "type": "string", "description": "Fabric: 'radical' (default) or 'retifist'" },
                    "lage": { "type": "string", "description": "Placement file (.lage), default <stem>.lage next to the pattern" }
                },
                "required": ["bilder"],
                "additionalProperties": false
            })
        },
    },
    Werkzeug {
        name: "sim_status",
        beschreibung: "State of the stepped scene (frames, phase, timing, points, seam gap).",
        schema: leer,
    },
    Werkzeug {
        name: "sim_reset",
        beschreibung: "Drop the stepped scene; the next simulate builds it again.",
        schema: leer,
    },
    Werkzeug {
        name: "export_obj",
        beschreibung:
            "Write the current scene as Wavefront OBJ (mm) to 'pfad' (under a write root).",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "pfad": { "type": "string", "description": "Path of the .obj file" }
                },
                "required": ["pfad"],
                "additionalProperties": false
            })
        },
    },
];

/// `tools/list` result.
pub fn liste() -> Value {
    let tools: Vec<Value> = WERKZEUGE
        .iter()
        .map(|w| {
            json!({
                "name": w.name,
                "description": w.beschreibung,
                "inputSchema": (w.schema)(),
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// What a command does.
#[derive(Debug, Clone, PartialEq)]
pub enum Befehl {
    Aktion(Aktion),
    /// Close the error, help and information windows and the file dialog (as Esc).
    DialogeSchliessen,
}

/// One command of `command_run`.
struct Kommando {
    id: &'static str,
    doc: &'static str,
    /// Parameter name, JSON schema.
    params: fn() -> Vec<(&'static str, Value)>,
}

fn keine() -> Vec<(&'static str, Value)> {
    Vec::new()
}

fn bereich_namen() -> Vec<&'static str> {
    Bereich::ALLE.iter().map(|(_, n, _)| *n).collect()
}

const KOMMANDOS: &[Kommando] = &[
    Kommando {
        id: "teil_waehlen",
        doc: "Select piece 'index' (0-based, see doc_inspect.teile)",
        params: || vec![("index", json!({"type": "integer", "minimum": 0}))],
    },
    Kommando {
        id: "naht_waehlen",
        doc: "Select seam 'index' of the seam report; selecting the selected one again clears it",
        params: || vec![("index", json!({"type": "integer", "minimum": 0}))],
    },
    Kommando {
        id: "naechstes_teil",
        doc: "Select the next piece (Tab)",
        params: keine,
    },
    Kommando {
        id: "voriges_teil",
        doc: "Select the previous piece (Shift+Tab)",
        params: keine,
    },
    Kommando {
        id: "auswahl_aufheben",
        doc: "Clear piece and seam selection",
        params: keine,
    },
    Kommando {
        id: "naehte_umschalten",
        doc: "Toggle seams in the 2D view",
        params: keine,
    },
    Kommando {
        id: "laengen_umschalten",
        doc: "Toggle edge lengths in the 2D view",
        params: keine,
    },
    Kommando {
        id: "teilnamen_umschalten",
        doc: "Toggle piece names in the 2D view",
        params: keine,
    },
    Kommando {
        id: "massstab_umschalten",
        doc: "Toggle the scale bar in the 2D view",
        params: keine,
    },
    Kommando {
        id: "fadenlauf_umschalten",
        doc: "Toggle grainlines in the 2D view",
        params: keine,
    },
    Kommando {
        id: "passzeichen_umschalten",
        doc: "Toggle notches in the 2D view",
        params: keine,
    },
    Kommando {
        id: "einpassen",
        doc: "Fit the pattern into the 2D view (F)",
        params: keine,
    },
    Kommando {
        id: "lage_umschalten",
        doc: "Switch between laid-out and construction coordinates (L)",
        params: keine,
    },
    Kommando {
        id: "lage_setzen",
        doc: "Laid out (true) or construction coordinates (false)",
        params: || vec![("ausgelegt", json!({"type": "boolean"}))],
    },
    Kommando {
        id: "thema_umschalten",
        doc: "Switch between dark and light theme",
        params: keine,
    },
    Kommando {
        id: "thema_setzen",
        doc: "Dark (true) or light (false) theme",
        params: || vec![("dunkel", json!({"type": "boolean"}))],
    },
    Kommando {
        id: "bereich_umschalten",
        doc: "Show or hide a panel, window or toolbar",
        params: || {
            vec![(
                "bereich",
                json!({"type": "string", "enum": bereich_namen()}),
            )]
        },
    },
    Kommando {
        id: "fenster_wechseln",
        doc: "Make the other of the 3D and 2D window the active one",
        params: keine,
    },
    Kommando {
        id: "fenster_aktivieren",
        doc: "Make the 3D or the 2D window the active one",
        params: || vec![("fenster", json!({"type": "string", "enum": ["3d", "2d"]}))],
    },
    Kommando {
        id: "anordnung_zuruecksetzen",
        doc: "Show all panels, windows and toolbars at their default sizes",
        params: keine,
    },
    Kommando {
        id: "reiter_zeigen",
        doc: "Show a tab of the object browser",
        params: || {
            vec![(
                "reiter",
                json!({"type": "string", "enum": ["teile", "naehte", "stoffe"]}),
            )]
        },
    },
    Kommando {
        id: "nahtlaengen_zeigen",
        doc: "Show the seam tab with the seam lengths",
        params: keine,
    },
    Kommando {
        id: "stoffbibliothek_zeigen",
        doc: "Show the library at the fabrics folder",
        params: keine,
    },
    Kommando {
        id: "informationen_zeigen",
        doc: "Open the window with information about the loaded files",
        params: keine,
    },
    Kommando {
        id: "dialoge_schliessen",
        doc: "Close error, help and information windows and the file dialog (Esc)",
        params: keine,
    },
];

/// `command_list` result.
pub fn kommandos() -> Value {
    let liste: Vec<Value> = KOMMANDOS
        .iter()
        .map(|k| {
            let params = (k.params)();
            let mut props = Map::new();
            let mut pflicht = Vec::new();
            for (n, s) in params {
                props.insert(n.into(), s);
                pflicht.push(n);
            }
            json!({
                "id": k.id,
                "doc": k.doc,
                "params": {
                    "type": "object",
                    "properties": props,
                    "required": pflicht,
                    "additionalProperties": false
                }
            })
        })
        .collect();
    json!({ "commands": liste })
}

/// Map `id` and `params` to a command. An unknown id is a tool error (the agent should look at
/// `command_list`), a missing or wrong parameter a parameter error.
pub fn befehl(id: &str, params: &Map<String, Value>) -> Result<Befehl, Fehler> {
    let index = || -> Result<usize, Fehler> {
        params
            .get("index")
            .and_then(Value::as_u64)
            .map(|i| i as usize)
            .ok_or_else(|| Fehler::Parameter(format!("{id}: 'index' (ganze Zahl ≥ 0) fehlt")))
    };
    let wahr = |name: &str| -> Result<bool, Fehler> {
        params
            .get(name)
            .and_then(Value::as_bool)
            .ok_or_else(|| Fehler::Parameter(format!("{id}: '{name}' (true/false) fehlt")))
    };
    let text = |name: &str| -> Result<&str, Fehler> {
        params
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| Fehler::Parameter(format!("{id}: '{name}' (Text) fehlt")))
    };
    let a = match id {
        "teil_waehlen" => Aktion::TeilWaehlen(index()?),
        "naht_waehlen" => Aktion::NahtWaehlen(index()?),
        "naechstes_teil" => Aktion::NaechstesTeil,
        "voriges_teil" => Aktion::VorigesTeil,
        "auswahl_aufheben" => Aktion::AuswahlAufheben,
        "naehte_umschalten" => Aktion::NaehteUmschalten,
        "laengen_umschalten" => Aktion::LaengenUmschalten,
        "teilnamen_umschalten" => Aktion::TeilnamenUmschalten,
        "massstab_umschalten" => Aktion::MassstabUmschalten,
        "fadenlauf_umschalten" => Aktion::FadenlaufUmschalten,
        "passzeichen_umschalten" => Aktion::PasszeichenUmschalten,
        "einpassen" => Aktion::Einpassen,
        "lage_umschalten" => Aktion::LageUmschalten,
        "lage_setzen" => Aktion::LageSetzen(wahr("ausgelegt")?),
        "thema_umschalten" => Aktion::ThemaUmschalten,
        "thema_setzen" => Aktion::ThemaSetzen(if wahr("dunkel")? {
            Theme::Dark
        } else {
            Theme::Light
        }),
        "bereich_umschalten" => {
            let t = text("bereich")?;
            let b = Bereich::ALLE
                .iter()
                .find(|(_, n, _)| *n == t)
                .map(|(b, _, _)| *b)
                .ok_or_else(|| {
                    Fehler::Parameter(format!(
                        "{id}: unbekannter Bereich {t:?}, erlaubt: {}",
                        bereich_namen().join(", ")
                    ))
                })?;
            Aktion::BereichUmschalten(b)
        }
        "fenster_wechseln" => Aktion::FensterWechseln,
        "fenster_aktivieren" => Aktion::FensterAktivieren(match text("fenster")? {
            "3d" => Fenster::Drei,
            "2d" => Fenster::Zwei,
            t => {
                return Err(Fehler::Parameter(format!(
                    "{id}: unbekanntes Fenster {t:?}, erlaubt: 3d, 2d"
                )))
            }
        }),
        "anordnung_zuruecksetzen" => Aktion::AnordnungZuruecksetzen,
        "reiter_zeigen" => Aktion::ReiterZeigen(match text("reiter")? {
            "teile" => Reiter::Teile,
            "naehte" => Reiter::Naehte,
            "stoffe" => Reiter::Stoffe,
            t => {
                return Err(Fehler::Parameter(format!(
                    "{id}: unbekannter Reiter {t:?}, erlaubt: teile, naehte, stoffe"
                )))
            }
        }),
        "nahtlaengen_zeigen" => Aktion::NahtlaengenZeigen,
        "stoffbibliothek_zeigen" => Aktion::StoffbibliothekZeigen,
        "informationen_zeigen" => Aktion::InformationenZeigen,
        "dialoge_schliessen" => return Ok(Befehl::DialogeSchliessen),
        _ => {
            return Err(Fehler::Werkzeug(format!(
                "unbekanntes Kommando {id:?} – command_list nennt alle"
            )))
        }
    };
    Ok(Befehl::Aktion(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jedes_kommando_der_liste_laesst_sich_abbilden() {
        for k in KOMMANDOS {
            let mut p = Map::new();
            for (n, s) in (k.params)() {
                let wert = match s["type"].as_str() {
                    Some("integer") => json!(0),
                    Some("boolean") => json!(true),
                    _ => s["enum"][0].clone(),
                };
                p.insert(n.into(), wert);
            }
            assert!(befehl(k.id, &p).is_ok(), "{}", k.id);
        }
    }

    #[test]
    fn fehler_bei_id_und_parametern() {
        let leer = Map::new();
        assert!(matches!(
            befehl("gibt_es_nicht", &leer),
            Err(Fehler::Werkzeug(_))
        ));
        assert!(matches!(
            befehl("teil_waehlen", &leer),
            Err(Fehler::Parameter(_))
        ));
        let mut p = Map::new();
        p.insert("bereich".into(), json!("keller"));
        assert!(matches!(
            befehl("bereich_umschalten", &p),
            Err(Fehler::Parameter(_))
        ));
        p.insert("index".into(), json!(-1));
        assert!(matches!(
            befehl("teil_waehlen", &p),
            Err(Fehler::Parameter(_))
        ));
    }

    #[test]
    fn werkzeugnamen_sind_eindeutig() {
        let mut namen: Vec<&str> = WERKZEUGE.iter().map(|w| w.name).collect();
        namen.sort();
        namen.dedup();
        assert_eq!(namen.len(), WERKZEUGE.len());
    }
}
