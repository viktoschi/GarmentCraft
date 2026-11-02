//! Drives the real `klon3d-mcp` binary over stdin/stdout, as an MCP client does: one JSON-RPC
//! message per line.
//!
//! The patterns are the ones in `klon3d-pattern/tests/muster`. `render_preview` needs a wgpu
//! adapter and `simulate` the body resizing folder (`CLO_FEMALE`); without them those steps print
//! `übersprungen: … fehlt` and the rest still runs.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

fn muster_ordner() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../klon3d-pattern/tests/muster")
}

fn muster(name: &str) -> String {
    muster_ordner().join(name).display().to_string()
}

/// A fresh, empty write root under the target folder.
fn schreibordner(name: &str) -> PathBuf {
    let o = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("mcp-{name}"));
    let _ = std::fs::remove_dir_all(&o);
    std::fs::create_dir_all(&o).unwrap();
    o
}

struct Server {
    kind: Child,
    ein: ChildStdin,
    aus: BufReader<ChildStdout>,
    id: u64,
}

impl Server {
    fn starten(schreiben: &PathBuf) -> Server {
        let mut kind = Command::new(env!("CARGO_BIN_EXE_klon3d-mcp"))
            .arg("--lesen")
            .arg(muster_ordner())
            .arg("--schreiben")
            .arg(schreiben)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("klon3d-mcp starts");
        let ein = kind.stdin.take().unwrap();
        let aus = BufReader::new(kind.stdout.take().unwrap());
        let mut s = Server {
            kind,
            ein,
            aus,
            id: 0,
        };
        let a = s.rufe(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "protokoll.rs", "version": "0" }
            }),
        );
        assert_eq!(a["result"]["protocolVersion"], "2025-06-18", "{a}");
        assert_eq!(a["result"]["serverInfo"]["name"], "klon3d-mcp");
        s.senden(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        s
    }

    fn senden(&mut self, zeile: &str) {
        writeln!(self.ein, "{zeile}").unwrap();
        self.ein.flush().unwrap();
    }

    fn lesen(&mut self) -> Value {
        let mut zeile = String::new();
        let n = self.aus.read_line(&mut zeile).unwrap();
        assert!(n > 0, "server closed stdout");
        serde_json::from_str(&zeile).unwrap_or_else(|e| panic!("no JSON ({e}): {zeile}"))
    }

    fn rufe(&mut self, methode: &str, params: Value) -> Value {
        self.id += 1;
        let anfrage =
            json!({ "jsonrpc": "2.0", "id": self.id, "method": methode, "params": params });
        self.senden(&anfrage.to_string());
        let a = self.lesen();
        assert_eq!(a["id"], self.id, "{a}");
        a
    }

    /// `tools/call`; returns the `result` (with `isError`) or panics on a JSON-RPC error.
    fn werkzeug(&mut self, name: &str, args: Value) -> Value {
        let a = self.rufe("tools/call", json!({ "name": name, "arguments": args }));
        assert!(a.get("error").is_none(), "{name}: {a}");
        a["result"].clone()
    }

    /// As `werkzeug`, and the call must succeed; returns `structuredContent`.
    fn gut(&mut self, name: &str, args: Value) -> Value {
        let r = self.werkzeug(name, args);
        assert_eq!(r["isError"], false, "{name}: {r}");
        r["structuredContent"].clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.kind.kill();
        let _ = self.kind.wait();
    }
}

fn fehlertext(r: &Value) -> String {
    assert_eq!(r["isError"], true, "{r}");
    r["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn base64_dekodieren(text: &str) -> Vec<u8> {
    const Z: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut n, mut bits) = (0u32, 0);
    for c in text.bytes().filter(|&c| c != b'=') {
        let w = Z.iter().position(|&z| z == c).expect("base64 character") as u32;
        n = (n << 6) | w;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
        }
    }
    out
}

#[test]
fn oeffnen_pruefen_waehlen_und_fehler() {
    let tmp = schreibordner("ablauf");
    let mut s = Server::starten(&tmp);

    let liste = s.rufe("tools/list", json!({}));
    let namen: Vec<&str> = liste["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for w in [
        "open",
        "doc_inspect",
        "command_list",
        "command_run",
        "render_preview",
        "suggest_seams",
        "mesh_check",
        "simulate",
        "sim_status",
        "sim_reset",
        "export_obj",
    ] {
        assert!(namen.contains(&w), "{w} missing in {namen:?}");
    }
    for t in liste["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
    }

    let d = s.gut("open", json!({ "muster": muster("bluse3_v2.sm2d") }));
    assert_eq!(d["muster"]["teile"].as_array().unwrap().len(), 28);
    let d = s.gut("doc_inspect", json!({}));
    assert_eq!(d["muster"]["teile"].as_array().unwrap().len(), 28);
    assert_eq!(d["muster"]["groesste_abweichung"]["kennung"], "N07L", "{d}");
    assert_eq!(d["muster"]["groesste_abweichung"]["delta_cm"], "-0.0737");
    assert_eq!(d["muster"]["naehte"]["stand"], "geprueft");
    assert_eq!(d["meldung_fehler"], false);
    assert!(d["auswahl"]["teil"].is_null());

    let k = s.gut("command_list", json!({}));
    assert!(k["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "teil_waehlen"));

    let d = s.gut(
        "command_run",
        json!({ "id": "teil_waehlen", "params": { "index": 3 } }),
    );
    assert_eq!(d["auswahl"]["teil"], 3);
    let d = s.gut(
        "command_run",
        json!({ "id": "thema_setzen", "params": { "dunkel": false } }),
    );
    assert_eq!(d["thema"], "hell");
    let d = s.gut("command_run", json!({ "id": "naehte_umschalten" }));
    assert_eq!(d["ansicht2d"]["naehte"], false);
    let d = s.gut("doc_inspect", json!({}));
    assert_eq!(d["auswahl"]["teil"], 3);

    let r = s.werkzeug("render_preview", json!({}));
    if r["isError"] == true {
        let t = fehlertext(&r);
        assert!(t.contains("kein wgpu-Adapter"), "{t}");
        eprintln!("übersprungen: kein wgpu-Adapter ({t})");
    } else {
        let bild = r["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "image")
            .expect("image content");
        assert_eq!(bild["mimeType"], "image/png");
        let png = base64_dekodieren(bild["data"].as_str().unwrap());
        let img = image::load_from_memory(&png).expect("PNG decodes");
        assert_eq!((img.width(), img.height()), (1400, 900));
        assert_eq!(r["structuredContent"]["breite_px"], 1400);
    }

    // Outside the roots.
    let fremd = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let t = fehlertext(&s.werkzeug("open", json!({ "muster": fremd.display().to_string() })));
    assert!(t.contains("Lese-Wurzel"), "{t}");
    let aussen = muster_ordner().join("neu.obj");
    let t = fehlertext(&s.werkzeug(
        "export_obj",
        json!({ "pfad": aussen.display().to_string() }),
    ));
    assert!(t.contains("Schreib-Wurzel"), "{t}");
    assert!(!aussen.exists());
    let t = fehlertext(&s.werkzeug(
        "suggest_seams",
        json!({ "schreiben": muster("bluse3_v2.naehte") }),
    ));
    assert!(t.contains("Schreib-Wurzel"), "{t}");

    // Errors leave the server alive.
    s.senden("{das ist kein json");
    let a = s.lesen();
    assert_eq!(a["error"]["code"], -32700, "{a}");
    let a = s.rufe(
        "tools/call",
        json!({ "name": "gibt_es_nicht", "arguments": {} }),
    );
    assert_eq!(a["error"]["code"], -32602, "{a}");
    let a = s.rufe("gibt/es/nicht", json!({}));
    assert_eq!(a["error"]["code"], -32601, "{a}");
    let t = fehlertext(&s.werkzeug("command_run", json!({ "id": "gibt_es_nicht" })));
    assert!(t.contains("unbekanntes Kommando"), "{t}");
    let a = s.rufe(
        "tools/call",
        json!({ "name": "command_run", "arguments": { "id": "teil_waehlen" } }),
    );
    assert_eq!(a["error"]["code"], -32602, "{a}");
    let a = s.rufe("tools/call", json!({ "name": "open", "arguments": {} }));
    assert_eq!(a["error"]["code"], -32602, "{a}");
    let t = fehlertext(&s.werkzeug(
        "command_run",
        json!({ "id": "teil_waehlen", "params": { "index": 28 } }),
    ));
    assert!(t.contains("außerhalb"), "{t}");
    let t = fehlertext(&s.werkzeug(
        "export_obj",
        json!({ "pfad": tmp.join("x.obj").display().to_string() }),
    ));
    assert!(t.contains("keine Szene"), "{t}");

    let a = s.rufe("ping", json!({}));
    assert_eq!(a["result"], json!({}));
    let d = s.gut("doc_inspect", json!({}));
    assert_eq!(d["auswahl"]["teil"], 3);
}

#[test]
fn naehte_vorschlagen_und_schreiben() {
    let tmp = schreibordner("vorschlag");
    let mut s = Server::starten(&tmp);
    let t = fehlertext(&s.werkzeug("suggest_seams", json!({})));
    assert!(t.contains("erst ein Muster"), "{t}");

    s.gut("open", json!({ "muster": muster("bluse3_v2.sm2d") }));
    let ohne = s.gut("suggest_seams", json!({}));
    assert!(ohne["geschrieben"].is_null());
    let ziel = tmp.join("bluse3_v2.naehte");
    let v = s.gut(
        "suggest_seams",
        json!({ "schreiben": ziel.display().to_string() }),
    );
    let n = v["naehte"].as_u64().unwrap() as usize;
    assert!(n > 0);
    assert_eq!(v["geladen"], true, "{v}");
    let text = std::fs::read_to_string(&ziel).unwrap();
    let defs = klon3d_pattern::naehte::lies_nahtliste(&text).expect("the list written reads back");
    assert_eq!(defs.len(), n);
    assert_eq!(v["text"].as_str().unwrap(), text);

    // The pattern now shows the list written.
    let d = s.gut("doc_inspect", json!({}));
    assert_eq!(d["muster"]["naehte"]["stand"], "geprueft", "{d}");
    assert!(d["muster"]["naehte"]["pfad"]
        .as_str()
        .unwrap()
        .ends_with("bluse3_v2.naehte"));
    assert_eq!(d["muster"]["naehte"]["naehte"].as_array().unwrap().len(), n);
}

#[test]
fn netz_pruefen() {
    let tmp = schreibordner("netz");
    let mut s = Server::starten(&tmp);
    s.gut("open", json!({ "muster": muster("bluse3_v2.sm2d") }));
    let n = s.gut("mesh_check", json!({}));
    assert_eq!(n["teile"].as_array().unwrap().len(), 28, "{n}");
    assert!(n["nicht_vernetzt"].as_array().unwrap().is_empty(), "{n}");
    assert!(n["punkte"].as_u64().unwrap() > 0);
}

#[test]
fn simulieren_vier_bilder() {
    let ordner = std::env::var_os("CLO_FEMALE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                r"C:\Program Files\CLO Standalone OnlineAuth\Preset\Avatar\Resizing\Female",
            )
        });
    let tmp = schreibordner("simulation");
    let mut s = Server::starten(&tmp);
    s.gut(
        "open",
        json!({
            "muster": muster("bluse3_v2.sm2d"),
            "massdatei": muster("measurements.smis"),
        }),
    );
    if !ordner.is_dir() {
        let t = fehlertext(&s.werkzeug("simulate", json!({ "bilder": 4 })));
        assert!(t.contains("Körperordner fehlt"), "{t}");
        eprintln!("übersprungen: {} fehlt", ordner.display());
        return;
    }
    let a = s.rufe("tools/call", json!({ "name": "simulate", "arguments": {} }));
    assert_eq!(a["error"]["code"], -32602, "{a}");

    let r = s.gut("simulate", json!({ "bilder": 4 }));
    assert_eq!(r["bilder"], 4, "{r}");
    assert_eq!(r["bilder_dieser_aufruf"], 4);
    assert_eq!(r["phase"], "naehen");
    // The scene holds exactly the points of the placed mesh.
    assert_eq!(r["punkte"], r["platziert_punkte"], "{r}");
    let weg = r["weggelassen"].as_array().unwrap();
    if weg.is_empty() {
        assert_eq!(r["punkte"], r["netz_punkte"], "{r}");
    }
    assert!(r["groesster_nahtspalt_mm"].as_f64().unwrap().is_finite());

    let st = s.gut("sim_status", json!({}));
    assert_eq!(st["bilder"], 4);
    let t = fehlertext(&s.werkzeug("simulate", json!({ "bilder": 1, "material": "retifist" })));
    assert!(t.contains("sim_reset"), "{t}");

    let obj = tmp.join("szene.obj");
    let e = s.gut("export_obj", json!({ "pfad": obj.display().to_string() }));
    let text = std::fs::read_to_string(&obj).unwrap();
    let v = text.lines().filter(|l| l.starts_with("v ")).count();
    assert_eq!(v as u64, r["punkte"].as_u64().unwrap());
    assert_eq!(e["punkte"], r["punkte"]);

    let z = s.gut("sim_reset", json!({}));
    assert_eq!(z["verworfen"], true);
    assert_eq!(s.gut("sim_status", json!({}))["szene"], false);
}
