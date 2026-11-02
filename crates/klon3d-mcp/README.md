# klon3d-mcp - the application for agents (MCP over stdio)

`klon3d-mcp` is an [MCP](https://modelcontextprotocol.io) server that lets an agent drive klon3d completely
without a desktop: no window opens, no window handle, no focus. It owns one headless session - the
application of `klon3d-app` inside an `egui_kittest` harness (as `examples/bildschirmfoto.rs` builds it) -
plus, after `simulate`, one stepped scene of `klon3d-pipeline`.

Transport: JSON-RPC 2.0, one message per line on stdin/stdout, protocol version `2025-06-18` (a client
asking for another version gets its own back). No async runtime. Only protocol lines go to stdout; messages
go to stderr.

## Start

```text
klon3d-mcp [--lesen <folder>]... [--schreiben <folder>]...
```

| Option | Meaning |
|---|---|
| `--lesen <folder>` | tools may read files below this folder (patterns, measurement files, seam lists, placement files, bodies) |
| `--schreiben <folder>` | tools may write files below this folder (seam lists, OBJ, PNG); also readable |

Both may be given several times. Roots and every path in a tool call are canonicalised (links and `..`
resolved) before the comparison; a path outside every root fails with a message naming the roots. A file
to be written may be new, its folder must exist. Without a root no tool reads or writes a file. The library
panel of the window starts in the first root, so pictures list no folder outside them. The body resizing
folder that `simulate` needs (`CLO_FEMALE`) is the server's own resource, not a tool path.

Build: `cargo build --release -p klon3d-mcp` -> `target/release/klon3d-mcp(.exe)`.

## Tools

Every result is JSON in `structuredContent` and, the same, as `text` content. A failing tool returns
`isError: true` with the reason; an unknown tool or a missing/malformed argument is JSON-RPC error -32602.

| Tool | Input | Does |
|---|---|---|
| `open` | `muster`, `massdatei?`, `nahtliste?` | loads a `.sm2d` (measurement file: the one given, else the one the pattern names; seam list: the one given, else `<stem>.naehte`); returns the state as `doc_inspect` |
| `doc_inspect` | - | pattern, measurement file, unit, pieces (index, name, perimeter cm, nodes), seam report (index, id, delta cm, status), largest deviation, selection, tab, theme, 2D toggles, arrangement, open dialogs, last message + error flag, simulation, roots |
| `command_list` | - | the commands of `command_run`: id, parameter schema, one-line doc |
| `command_run` | `id`, `params?` | one application command (`teil_waehlen {index}`, `naht_waehlen {index}`, `naehte_umschalten`, `thema_setzen {dunkel}`, `bereich_umschalten {bereich}`, `reiter_zeigen {reiter}`, `lage_setzen {ausgelegt}`, `dialoge_schliessen`, ...), runs one frame, returns the new state |
| `render_preview` | `breite?`, `hoehe?`, `massstab?`, `pfad?` | PNG of the whole window as image content (default 1400 x 900, scale 1); with `pfad` also written. No graphics adapter: `isError` starting with `kein wgpu-Adapter` |
| `suggest_seams` | `schreiben?` | seam suggestion as `klon3d naehte --vorschlag`; returns the list text; with `schreiben` writes it and loads the pattern again with it |
| `mesh_check` | - | meshes the pattern with its checked seam list: per piece points, triangles, smallest angle, edge lengths; largest seam offset; violations of the mesh acceptance |
| `simulate` | `bilder`, `koerper?`, `material?`, `lage?` | first call builds the scene (mesh, body shaped from the measurement file or `koerper` - `.smis` or a `.zprj` with a body -, placement `lage` or `<stem>.lage`, fabric `radical`/`retifist`); every call computes up to `bilder` more frames of the dressing run (sew, settle, hold). Returns frames, phase, timing, points of the scene and of the placed mesh, largest seam gap |
| `sim_status` | - | the same numbers without stepping, or `{"szene": false}` |
| `sim_reset` | - | drops the scene; the next `simulate` builds it again (needed to change body, placement or fabric) |
| `export_obj` | `pfad` | writes the scene as Wavefront OBJ (mm, one group per piece) |

Not exposed, because they only open dialogs: Datei > Öffnen, Maßdatei/Nahtliste wählen, Beenden, the help
windows; `open` and `suggest_seams` replace them. Opening another pattern drops the scene.

`simulate` needs the body resizing folder (`klon3d_koerper::koerper::ordner`, environment variable
`CLO_FEMALE`, with a default path on the main machine); without it the tool fails with `Körperordner fehlt: ...`. A debug-build frame of the Bluse3 v2
takes several seconds; use the release build for real runs.

## Client configuration

OpenCode (`opencode.json`):

```json
{
  "mcp": {
    "klon3d": {
      "type": "local",
      "command": [
        "D:/ai/klon3d/target/release/klon3d-mcp.exe",
        "--lesen", "D:/patterns",
        "--schreiben", "D:/ai/klon3d-ausgabe"
      ],
      "environment": {
        "CLO_FEMALE": "C:/Program Files/CLO Standalone OnlineAuth/Preset/Avatar/Resizing/Female"
      },
      "enabled": true
    }
  }
}
```

Generic form (Claude Desktop/Code, Codex and other clients with `command`/`args`):

```json
{
  "mcpServers": {
    "klon3d": {
      "command": "D:/ai/klon3d/target/release/klon3d-mcp.exe",
      "args": ["--lesen", "D:/patterns", "--schreiben", "D:/ai/klon3d-ausgabe"],
      "env": { "CLO_FEMALE": "C:/Program Files/CLO Standalone OnlineAuth/Preset/Avatar/Resizing/Female" }
    }
  }
}
```

A typical session: `open` -> `doc_inspect` -> `command_run teil_waehlen {index: 3}` -> `render_preview` ->
`suggest_seams {schreiben}` -> `mesh_check` -> `simulate {bilder: 50}` (repeat) -> `export_obj`.

## Tests

`tests/protokoll.rs` starts the real binary and talks to it over stdin/stdout: initialize, tools/list,
open Bluse3 v2 (28 pieces, largest deviation N07L -0.0737 cm), select piece 3, a 1400 x 900 PNG (or
`übersprungen: kein wgpu-Adapter`), refused paths outside the roots, malformed JSON, unknown tool, method and
command id, missing parameters - and the server still answers `ping`. Further: seam suggestion written and
read back with `lies_nahtliste`, mesh check, and 4 frames of `simulate` with the point count of the scene
equal to the placed mesh plus an OBJ export (skipped without the resizing folder).

## Next step: bridge mode

Today the server drives its own, invisible window. Next: `klon3d-app --control <port>` (localhost only), so
the same tools act on a window the user is looking at - the server forwards the calls instead of owning the
`App`.
