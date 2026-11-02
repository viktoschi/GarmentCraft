# klon3d

A local 3D garment simulator for **latex and textile clothing**, driven by **Seamly2D** sewing patterns.

Rust · `MIT OR Apache-2.0` · fully offline

## What it does

klon3d reads Seamly2D patterns (`.sm2d`), turns the pieces into triangle meshes, sews them and drapes them
on a body with an implicit cloth solver (backward Euler with conjugate gradients; an XPBD solver stays as a
second path for cross-checks). It is meant for latex **and** woven or knitted fabrics, and it can read CLO
project files (`.zprj`) — pieces, seams, fabrics, avatar and the stored simulated mesh — to compare its own
results against. Garments stay on the body the way they do in real life: through tension and friction
only, nothing is pinned.

klon3d is an independent implementation based on the physics literature and open file formats. It does
not contain code from CLO or Seamly2D.

## Why latex is its own material

| Property | Woven fabric | Latex |
|---|---|---|
| Stretch | low | very high, often > 200 % |
| Bending stiffness | noticeable | very low |
| Poisson ratio | low | close to 0.5 (nearly incompressible) |
| Thickness | 0.3–1 mm | 0.3–0.8 mm |
| Surface | matte | glossy, slightly tacky |

That calls for different stiffnesses in the solver and its own contact handling. Latex was the first test
case; textiles are an equal goal and need anisotropic material with grain direction, separate shear
stiffness and self-collision (see [PLAN.md](PLAN.md)).

## Status

| Crate | What it does | State |
|---|---|---|
| `klon3d-pattern` | `.sm2d` parser, formula engine, geometry, curves, arcs, piece outlines, seam edges and seam pairs; `.zprj` reader; `.smis` measurements | 1,029 points of four test patterns within 0.001 cm; operations, partial curves, `cutArc`, `trueDarts`; the `.zprj` reader reads all 47 projects of the public test set without a seam error |
| `klon3d-mesh` | pieces as triangle meshes (boundary within 0.01 cm of the seam line, minimum angle 25°, edges ~10 mm), seams as point pairs by length fraction, acceptance check, SVG/OBJ output | test blouse: 28 pieces, 24 seams, acceptance passed |
| `klon3d-koerper` | body shaping from measurements and tape-measure style measurements on the mesh | all 58 measurements checked against reference values (needs CLO's avatar data, see below) |
| `klon3d-sim` | shared scene with two solvers: implicit CG (the main path) and XPBD (cross-check); body collider as signed distance over a grid; placement around the body; self-collision | tube test matches the analytic result; blouses dressed on the body with 94.6–99.4 % of seam pairs within 10 mm, held by tension alone |
| `klon3d-view` | `klon3d-fenster`: viewer for piece outlines, PNG output (Win32, Windows only) | works |
| `klon3d-cli` | the `klon3d` command line | works |
| `klon3d-mcp` | MCP server (stdio): agents open, inspect, command, photograph and simulate the application headless ([README](crates/klon3d-mcp/README.md)) | works |

Planned: a `wgpu` viewport and an `egui` application. The roadmap with all stages is in [PLAN.md](PLAN.md),
the material model in [MATERIAL.md](MATERIAL.md).

## Build and test

```sh
cargo build --locked --workspace --exclude klon3d-view
cargo test  --locked --workspace --exclude klon3d-view
```

`klon3d-view` talks to Win32 directly and only builds on Windows: `cargo build --locked -p klon3d-view`.

## Usage

```text
klon3d info      <pattern.sm2d>                     read a pattern, report its structure
klon3d umriss    <pattern.sm2d> [--svg <file>]      piece outlines
klon3d dxf       <pattern.sm2d> <out.dxf>           AAMA DXF export (edge = seam line)
klon3d naehte    <pattern.sm2d> [--liste <file>]    check seam edges and the seam list
klon3d netz      <pattern.sm2d> [--obj <file>]      triangle meshes and seam point pairs
klon3d roehre    [--material radical|retifist]      solver tube test
klon3d koerper   <measurements.smis> [--out <obj>] [--wie-clo]
                                                    shape a body from measurements (--wie-clo:
                                                    exactly as the reference program shapes it)
klon3d anziehen  <pattern.sm2d> [--obj <dir>]       place the pieces around the body and dress it
klon3d zprj      <project.zprj> [--obj <file>]      read a CLO project, export its simulated mesh
klon3d vergleich <a.obj> <b.obj>                    surface distance between two meshes
```

`klon3d` without arguments prints all options. Command names and messages are German for now.

## Test data

Two kinds of input, and either one is enough to exercise the whole pipeline:

- **Seamly2D patterns** (`.sm2d`) — the author's own garments under
  `crates/klon3d-pattern/tests/muster/`, published under the same license as the code. `klon3d punkte`,
  `umriss`, `naehte`, `netz`, `dxf` and `anziehen` all start here.
- **CLO projects** (`.zprj`) — `klon3d zprj <project.zprj>` reads pieces, seams, fabrics and the stored 3D
  mesh; `klon3d anziehen <pattern.sm2d> --zprj <project.zprj>` takes the **body** from the project instead of
  from a `.smis` measurement file. The 47 public projects of
  [tabjy/clo3d-patterns](https://github.com/tabjy/clo3d-patterns) (CC BY-SA 4.0) are the test corpus CI
  fetches, pinned to commit `22d0c8a`; point `KLON3D_FREMD` at a local clone to run those tests yourself. A
  test that wants to run on a CLO project therefore does not need a pattern library at all — the project
  carries pattern, seams and body in one file.
- **Not distributable:** tests that need CLO's avatar data (`*_clo.rs`, `CLO_FEMALE`) or the author's private
  reference projects. Without the data they report `übersprungen` (skipped) and pass.

Comments refer to findings ("Befund 64") in the author's private research notes; they are not part of this
repository.

## Contributing

Several agents work on klon3d at the same time: tasks are issues, claimed with `tools/task.sh`, worked on in
separate worktrees and delivered as pull requests. Rules for agents: [AGENTS.md](AGENTS.md); the workflow:
[CONTRIBUTING.md](CONTRIBUTING.md). Most existing code comments are German; new comments and commit messages
are written in English.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in klon3d shall be dual licensed as above, without any additional terms or conditions.
