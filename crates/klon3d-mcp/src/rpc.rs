//! JSON-RPC 2.0 over lines of stdin/stdout, the subset MCP needs.
//!
//! One request per line. Answers go to stdout, one per line; nothing else is written there.
//! Malformed JSON gives -32700, an object that is no request -32600, an unknown method -32601,
//! wrong parameters -32602. A failing tool is a normal result with `isError: true`.

use serde_json::{json, Map, Value};

use crate::sitzung::Sitzung;
use crate::werkzeuge::{self, Fehler};

/// Protocol version this server implements; a client asking for another gets its own back.
pub const PROTOKOLL: &str = "2025-06-18";

const PARSE_FEHLER: i64 = -32700;
const UNGUELTIGE_ANFRAGE: i64 = -32600;
const UNBEKANNTE_METHODE: i64 = -32601;
const UNGUELTIGE_PARAMETER: i64 = -32602;

/// Something that serves tool calls; the session in the binary, a stand-in in the unit tests.
pub trait Werkzeugkasten {
    fn aufrufen(
        &mut self,
        name: &str,
        args: &Map<String, Value>,
    ) -> Result<crate::sitzung::Antwort, Fehler>;
}

impl Werkzeugkasten for Sitzung {
    fn aufrufen(
        &mut self,
        name: &str,
        args: &Map<String, Value>,
    ) -> Result<crate::sitzung::Antwort, Fehler> {
        Sitzung::aufrufen(self, name, args)
    }
}

fn fehler(id: Value, code: i64, text: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": text.into() } })
}

fn ergebnis(id: Value, r: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": r })
}

/// Handle one line; the answer to write, or `None` for a notification or an empty line.
pub fn zeile(text: &str, kasten: &mut dyn Werkzeugkasten) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }
    let anfrage: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Some(fehler(Value::Null, PARSE_FEHLER, format!("kein JSON: {e}"))),
    };
    let Some(obj) = anfrage.as_object() else {
        return Some(fehler(
            Value::Null,
            UNGUELTIGE_ANFRAGE,
            "keine Anfrage (Objekt erwartet)",
        ));
    };
    let id = obj.get("id").cloned();
    let Some(methode) = obj.get("method").and_then(Value::as_str) else {
        // An answer of the client (we send no requests) or garbage.
        return id.map(|id| fehler(id, UNGUELTIGE_ANFRAGE, "'method' fehlt"));
    };
    let leer = Map::new();
    let params = match obj.get("params") {
        None | Some(Value::Null) => &leer,
        Some(Value::Object(p)) => p,
        Some(_) => {
            return id.map(|id| fehler(id, UNGUELTIGE_PARAMETER, "'params' muss ein Objekt sein"))
        }
    };
    // Notifications get no answer, whatever they are.
    let id = id?;
    Some(match methode {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOKOLL);
            ergebnis(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "klon3d-mcp",
                        "title": "klon3d",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": "klon3d is a 3D simulator for sewn garments. Typical flow: \
                        open (a .sm2d pattern) -> doc_inspect -> command_list / command_run -> \
                        render_preview to look at the window -> suggest_seams, mesh_check -> \
                        simulate {bilder} -> sim_status -> export_obj. Paths must lie under the \
                        read/write roots the server was started with (doc_inspect.wurzeln).",
                }),
            )
        }
        "ping" => ergebnis(id, json!({})),
        "tools/list" => ergebnis(id, werkzeuge::liste()),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(fehler(id, UNGUELTIGE_PARAMETER, "tools/call: 'name' fehlt"));
            };
            let args = match params.get("arguments") {
                None | Some(Value::Null) => &leer,
                Some(Value::Object(a)) => a,
                Some(_) => {
                    return Some(fehler(
                        id,
                        UNGUELTIGE_PARAMETER,
                        "tools/call: 'arguments' muss ein Objekt sein",
                    ))
                }
            };
            match kasten.aufrufen(name, args) {
                Ok(a) => {
                    let mut inhalt = Vec::new();
                    if let Some(png) = a.png_base64 {
                        inhalt
                            .push(json!({ "type": "image", "data": png, "mimeType": "image/png" }));
                    }
                    inhalt.push(json!({ "type": "text", "text": a.json.to_string() }));
                    ergebnis(
                        id,
                        json!({ "content": inhalt, "structuredContent": a.json, "isError": false }),
                    )
                }
                Err(Fehler::Werkzeug(t)) => ergebnis(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": t }],
                        "structuredContent": { "fehler": t },
                        "isError": true,
                    }),
                ),
                Err(Fehler::Parameter(t)) => fehler(id, UNGUELTIGE_PARAMETER, t),
            }
        }
        andere => fehler(
            id,
            UNBEKANNTE_METHODE,
            format!("unbekannte Methode {andere:?}"),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sitzung::Antwort;

    struct Attrappe;

    impl Werkzeugkasten for Attrappe {
        fn aufrufen(&mut self, name: &str, _: &Map<String, Value>) -> Result<Antwort, Fehler> {
            match name {
                "gut" => Ok(json!({ "a": 1 }).into()),
                "kaputt" => Err(Fehler::Werkzeug("geht nicht".into())),
                _ => Err(Fehler::Parameter("unbekannt".into())),
            }
        }
    }

    fn rufe(text: &str) -> Option<Value> {
        zeile(text, &mut Attrappe)
    }

    #[test]
    fn initialize_nimmt_die_version_des_clients() {
        let a = rufe(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#).unwrap();
        assert_eq!(a["result"]["protocolVersion"], "2025-03-26");
        assert!(a["result"]["capabilities"]["tools"].is_object());
        let a = rufe(r#"{"jsonrpc":"2.0","id":2,"method":"initialize"}"#).unwrap();
        assert_eq!(a["result"]["protocolVersion"], PROTOKOLL);
    }

    #[test]
    fn fehlercodes() {
        assert_eq!(rufe("{kaputt").unwrap()["error"]["code"], PARSE_FEHLER);
        assert_eq!(rufe("[1]").unwrap()["error"]["code"], UNGUELTIGE_ANFRAGE);
        let a = rufe(r#"{"jsonrpc":"2.0","id":"x","method":"gibt/es/nicht"}"#).unwrap();
        assert_eq!(a["error"]["code"], UNBEKANNTE_METHODE);
        assert_eq!(a["id"], "x");
        let a = rufe(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nix"}}"#)
            .unwrap();
        assert_eq!(a["error"]["code"], UNGUELTIGE_PARAMETER);
        let a = rufe(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{}}"#).unwrap();
        assert_eq!(a["error"]["code"], UNGUELTIGE_PARAMETER);
    }

    #[test]
    fn mitteilungen_und_leerzeilen_ohne_antwort() {
        assert!(rufe(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        assert!(rufe("   ").is_none());
    }

    #[test]
    fn werkzeugergebnis_und_werkzeugfehler() {
        let a = rufe(r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"gut"}}"#)
            .unwrap();
        assert_eq!(a["result"]["isError"], false);
        assert_eq!(a["result"]["structuredContent"]["a"], 1);
        assert_eq!(a["result"]["content"][0]["type"], "text");
        let a = rufe(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"kaputt","arguments":{}}}"#).unwrap();
        assert_eq!(a["result"]["isError"], true);
        assert_eq!(a["result"]["content"][0]["text"], "geht nicht");
    }
}
