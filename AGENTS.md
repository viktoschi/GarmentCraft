# Rules for agents – klon3d

klon3d is a local 3D simulator for latex and textile garments: it reads Seamly2D patterns (`.sm2d`) and CLO
projects (`.zprj`), shapes and measures bodies, meshes and sews pattern pieces and simulates them with an
implicit solver. Rust workspace, `MIT OR Apache-2.0`.

**Language:** documentation, new code comments and commit messages are written in English. Most existing
comments are German; leave them unless you touch the code anyway. Identifiers stay as they are.

Several agents work on this repository at the same time. How tasks are handed out is described in
[CONTRIBUTING.md](CONTRIBUTING.md) – **read it before your first change.**

## Your first ten minutes

1. [README.md](README.md) (purpose), [PLAN.md](PLAN.md) (stages and status), plus the module docs (`//!`)
   of the crate you are about to change. [MATERIAL.md](MATERIAL.md) for fabric parameters,
   [PARITY.md](PARITY.md) for the comparison with CLO.
2. Build and test:
   ```sh
   cargo build --locked --workspace --exclude klon3d-view
   cargo test  --locked --workspace --exclude klon3d-view
   ```
   `klon3d-view` is a Win32 window and only builds on Windows: `cargo build --locked -p klon3d-view`.
3. Pick a free task and claim it: `tools/task.sh list`, then `tools/task.sh claim <nr> <session>`. Only then
   change files.
4. Work in your own worktree on your own branch ([CONTRIBUTING.md](CONTRIBUTING.md)).
5. Finish with a pull request that lists the checks that actually ran – or with a handoff note on the issue
   and a release.

## Where things live

| Area | Crate | Start here | Checked by |
|---|---|---|---|
| Reading patterns (`.sm2d`, formulas, outlines, seams), CLO projects (`.zprj`), measurements (`.smis`) | `klon3d-pattern` | `src/lib.rs`, `src/naehte.rs`, `src/kanten.rs`, `src/zprj.rs` | `tests/`, patterns in `tests/muster/` |
| Shaping and measuring bodies | `klon3d-koerper` | `src/lib.rs`, `src/massband.rs` | `tests/*_clo.rs` – skipped without CLO's data |
| Pieces to triangle meshes, seam point pairs | `klon3d-mesh` | `src/lib.rs`, `src/rand.rs` | `tests/naht_synthetisch.rs`, `tests/bluse3_v2.rs` |
| Solver: implicit CG (main path), XPBD (cross-check), body contact | `klon3d-sim` | `src/lib.rs`, `src/implizit.rs` | `tests/implizit.rs`, `tests/roehre.rs`, `tests/selbstkollision.rs` |
| Viewer (Win32/GDI, Windows only) | `klon3d-view` | `src/main.rs` | by eye |
| Command line `klon3d` | `klon3d-cli` | `src/main.rs`, `src/anziehen.rs` | `tests/` |
| Everything from `.sm2d` to a placed scene, and the DXF writer | `klon3d-pipeline` | `src/lib.rs`, `src/dxf.rs` | `tests/` |

## Shared files

Almost every task touches these. If you change one, list it in the task's scope (label `shared-file`),
change as little as possible and rebase on `main` before opening the PR:

- `Cargo.toml`, `Cargo.lock`, `crates/*/Cargo.toml`
- `crates/*/src/lib.rs` (module declarations)
- `crates/klon3d-cli/src/main.rs` (command dispatch, more than 2,000 lines)
- `README.md`, `PLAN.md`, `AGENTS.md`, `CONTRIBUTING.md`, `.github/`, `tools/`

Put new functionality in a **new file** and add only the one line to `lib.rs` or `main.rs` that wires it in.
That keeps conflicts small.

**No `cargo fmt` over the whole workspace.** The existing code is not rustfmt-formatted; a full run changes
about 1,100 places and collides with every open branch. Format only new files of your own.

## What must not break

- **Units:** patterns and meshes in mm. CLO uses y = up, klon3d z = up; convert at the boundary to CLO data,
  not in the middle of a computation.
- **No hard-wired material:** latex is one test case, woven and knitted fabrics are equal goals. Fabric values
  come from profiles ([MATERIAL.md](MATERIAL.md), `assets/stoffe/`).
- **Nothing is pinned:** garments stay on the body through tension and friction alone.
- **Measurements on the reference body run in `f32`** (`massband::*_f32`). That is measured and intended – do not
  "improve" it to `f64`. The module docs explain why.
- **`klon3d zprj` exits with 1** when a seam side points to no line of its piece. That is the acceptance
  condition, not a cosmetic issue.
- **Tests without data skip, they never fail.** A test that needs CLO's data, third-party projects or a
  pattern library looks them up through an environment variable with a local default, prints
  `übersprungen: … fehlt` (skipped: … missing) with `eprintln!` and returns. Example:
  `klon3d-koerper/tests/kette_clo.rs` with `CLO_FEMALE`. CI only has the 47 public CLO projects from
  `tabjy/clo3d-patterns` (`KLON3D_FREMD`) – no CLO installation and no private reference projects.
- **Report honestly:** name only checks that actually ran; say "skipped" and "not checked" explicitly.
  Throw-away diagnostics do not belong in the PR – what stays is a named test that asserts something.

## Provenance and license

klon3d is licensed `MIT OR Apache-2.0`. Ideas, methods, formulas and numbers may be adopted; code is written
from scratch.

- Copy foreign code verbatim only from MIT, BSD, Apache-2.0, Zlib or ISC sources – with the copyright and
  license notice in the file.
- GPL, LGPL, AGPL (e.g. Seamly2D, EXO): read, yes; copy, no.
- **Nothing from CLO goes into the repository:** no files from its installation or asset library, no
  exported CLO meshes, nothing taken from CLO's program files – not as an excerpt, address or function name –
  in code, comments or commit messages. References to findings ("Befund 22") and specifications are fine,
  and so is naming CLO's file formats (`.zprj`).
- **Commit messages say what changes, not how it was found out** – no names of analysis tools or methods.
- Personal data (body measurements in `*.smis`, own patterns) only with explicit consent from viktoschi.
- The origin of an adopted method goes into the PR.

## On the main machine (Windows, drive D:)

- `cargo` is not on the PATH but in `D:\ai\rea\tools\rust`. In Git Bash:
  `export CARGO_HOME=/d/ai/rea/tools/rust/cargo RUSTUP_HOME=/d/ai/rea/tools/rust/rustup PATH=/d/ai/rea/tools/rust/cargo/bin:$PATH`
- The working copy is `D:\ai\rea\klon3d`, a folder inside the private repository `D:\ai\rea`. There,
  `D:\ai\rea\CLAUDE.md` applies as well: ask before every commit, push to GitHub via `git subtree split` to
  your own branch (ADR-025). A branch to be split must contain the newest `Split 'klon3d/' into commit …`
  anchor of rea's `main`; an older branch would bring back the previous history.
- Worktrees go to `D:\ai\klon3d-wt\<branch>` – build and work folders never on C:.
- Every worktree builds into its own `target/`. A running `klon3d.exe` blocks the rebuild; stop it first.
- CLO's data, reference values and measurement projects live outside the repository (`D:\ai\rea\work\…`,
  `C:\Program Files\CLO Standalone OnlineAuth\…`), the findings in `D:\ai\rea\findings\`. Only here do the
  `*_clo.rs` tests really run.
