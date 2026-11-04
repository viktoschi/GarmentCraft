# Plan: fit simulator in Rust

Decision of 2026-10-18: **entirely in Rust**, without Blender at runtime.

**Status: 2026-11-02.** Stages 0 and 1 are done, stage 2 done — including the
silhouette check (finding 24, `klon3d-koerper/src/silhouette.rs`) and the
post-shaping between waist and hip (finding 124, `nachformung.rs`, beyond the
reference program) — stage 6
partly brought forward. The body and its
67 measurements now come from the **reference** resizing folder (finding 64); the
older chain is kept as a backup for old folders and the Newton fit remains the
last fallback. In stage 3,
boundary edges and seam pairs are done (finding 32), as are the mesh and seam subdivision
(`klon3d-mesh`, finding 33); both acceptance tests on Bluse3 v2 have passed. From stage 4 on,
the solver runs on the GPU with a CPU reference as the yardstick, and motion is
a goal as stage 7 (ADR-018). In stage 4, the XPBD reference (finding 34) and
the implicit step with CG like CLO (finding 35) are done on the tube; since 2026-10-27 the
implicit step is the basis and XPBD remains a cross-check path
(ADR-020). Since 2026-10-27 the shaped body is a mesh collision body
in the solver (finding 94), and Bluse3 v2 can be placed around it and dressed
onto it (4c largely done, findings 95, 96: 98.6% of seam pairs closed); with Bluse1 v25 and the strapless bustier v1,
the same chain also dresses other patterns without per-pattern tuning, and everything holds by tension (finding 98:
99.4 / 99.1 / 94.6%). Findings 17–24 changed the foundations of stages 2
and 3 — they are incorporated below.

Since 2026-10-31 the placement puts the collar on the **slanted** neck seam
(finding 121): the seam is not a horizontal circle but runs from 1.3161 m at
the front to 1.3750 m at the back, so height and radius are taken per
circumference angle from the four neck landmarks. Before, 16 collar points sat
inside the skin, the deepest 5.68 mm below it at the back; now none do, and the
deepest sits 19.81 mm above the skin (reference folder), 21.71 mm (older
folder), in 4 of 4 cases.

## Starting point — what already exists

| Exists | Where | Meaning for this plan |
|---|---|---|
| CLO base mesh (reference folder), 47,650 vertices | `C:\Program Files\CLO Standalone OnlineAuth\Preset\Avatar\Resizing\Female\BaseMesh.obj` | cm, body 164.15 cm tall, 76,298 triangles (group `body` 62,658, model 40,465 points) - **the basis** (finding 64; the older mesh is only a backup) |
| CLO shaping data (reference folder) | there: `FeatureData.bd`, `ReducedVertexData.bd`, `PCA_weight.bd`, `..\Detail_*.txt` | finding 64: shaping matrices computed by regression at load time (PCA); specification `spez/formung_2026_1.md`. The older fields `ConvertingMat_*.bd` (finding 18) are only a backup |
| `.sm2d` calculator (Python) | `D:\ai\spacebunny\schnitt.py` | reference for the target geometry, **not** a dependency |
| Measurement file `.smis` | `Hexenensemble...\measurements.smis` | 86 measurements, 160 cm, bust 92.67, waist 70, hip 90, HighHip 83, underbust 76; reader done (finding 24) |
| 528 real `.sm2d` patterns | `latex_schnitte\` | test material, 528/528 readable |
| Formula engine (Rust) | `crates\klon3d-pattern\src\formula.rs` | part of stage 1, done |
| Circumference measurement on the mesh | `crates\klon3d-koerper\src\lib.rs` | section loop, 4 tests green |
| 2D pattern viewer | `crates\klon3d-view`, `klon3d-fenster.exe` | Win32 + GDI, PNG output — **diagnostic tool**, not stage 6 |

**Other open-source projects compared** (findings 107, 114, as of 2026-10-30):
ClothCraft (MIT) is the most complete editor but has no friction, no layers,
no body from measurements and no animation; held only by tension, the skirt v36
slides off without a waistband and sinks 17–24 mm in 10 s with one (klon3d:
−0.2 mm). EXO Sample 3D is a GPL build on a modified ClothCraft, not a solver
of its own. Neither claims parity with CLO; klon3d stays, because Seamly2D
patterns, a body from measurements and friction are what latex needs.

**No longer needed:** rebuilding the MakeHuman body. Blender is used once as a
data source (exporting the base body); after that it is out of the picture.

## Scope

**Will be built:** read pattern → 3D geometry → material → simulation → inspection output.

**For latex and fabric** (ADR-021, 2026-10-27): klon3d should also simulate garments
made of woven and knitted fabric from Seamly2D patterns, not just latex.
Latex remains the first test case; every stage is built so that fabric is not
ruled out. What fabric additionally requires is listed below in the section
"Fabric alongside latex".

**Also for CLO projects** (ADR-022, 2026-10-27): besides `.sm2d`, klon3d reads
`.zprj` — pieces, seams, fabrics, placement and avatar — so that a CLO project can be
dressed without a handwritten seam and placement file. Reading comes first;
writing back has not been decided. See the section "Reading CLO projects".

**Holding as in real life** (ADR-023, 2026-10-27): garments stay on solely through
the tension of the rubber and friction on the skin, under full gravity —
nothing is pinned. Aids during dressing are allowed if they only reproduce
the act of dressing; afterwards, a hold phase under full gravity is part of
acceptance (at least 10 s, no piece sinks more than 5 mm).

**Not built:** 2D pattern editing beyond verification purposes, layout,
grading, production-quality rendering.

---

## Stage 0 — Foundation (done)

Rust 1.99.1 (MSVC) under `tools\rust`. Workspace with `klon3d-pattern`.
Formula engine: lexer, recursive descent, QMuParser dialect, 9 tests green.

## Stage 1 — Read `.sm2d` completely

| Part | Content | Done when |
|---|---|---|
| Data model | `pattern`, `increments`, `draftBlock`, `pieces` | `male_shirt.sm2d` and `Bluse3_Plunge_Cups_parametrisch_v2.sm2d` parse |
| XML reader | quick-xml, attributes as `Option<f64>` | — |
| Point types | `single` `endLine` `alongLine` `normal` `shoulder` `height` `lineIntersectAxis` `curveIntersectAxis` `cutSpline` `modeling`; since finding 99 also `cutArc`, `pointOfIntersectionArcs`, `pointOfIntersectionCurves`, `trueDarts` | points agree with `schnitt.py` to 0.01 mm |
| Operations and sub-curves | `moving`, `rotation`, `flippingByAxis`, `flippingByLine` with point and curve copies (name + suffix); sub-curves of `cutSpline`/`cutSplinePath` and sub-arcs of `cutArc` for `Spl_`, `SplPath_`, `Arc_` | **done** (finding 99): Bolero v36/v37 and stocking v26 without errors, perimeters as in Seamly2D's own export (≤ 0.03 mm). **Since #42 (finding 123) also the sub-curves of `curveIntersectAxis`** (spline, path and arc, cut at the length up to the axis point as Seamly2D does; spline parts end at the axis point, path parts at the cut point) |
| Splines and arcs | `NodeSpline`, `NodeSplinePath`, arcs (orientation: finding 16); since #42 `cubicBezier` with its four control points and `cubicBezierPath` as segments of 3k..3k+3 (both were read wrongly before) | — |
| L036 (third-party, mm) | the only pattern with `trueDarts`, `cubicBezier`, `cubicBezierPath` and formulas on axis sub-curves; checked with a synthetic body against Seamly2D's export (`tests/l036_seamly.rs`, skips without the pattern) | **#42, finding 123**: all 204 named points within 7e-5 mm of Seamly2D (probe pieces, one per point), 4 of 5 seam lines ≤ 0.08 mm. **Since #49 all five** within 0.001 mm in both directions: the seam line is cleaned like Seamly2D's `mainPathPoints` (`schleifen.rs`: near duplicates, spikes and loops out; the centre back of "L036 - back" overlaps itself, 2294.16 instead of 2727.4 mm). Node points keep their place, and nodes inside a removed loop drop out of the edges |
| Seam line cleanup | `schleifen.rs`, called from `kontur.rs`; points of point nodes are fixed, a node in a removed loop is dropped from the boundary edges (`kanten.rs`) instead of leaving an edge of length zero | **#49**: synthetic cases (backtracking run, figure eight, spike, near duplicates, fixed points, a piece with loop nodes); Bolero v37 and stocking v26 still match Seamly2D's export; workspace tests green |
| `idObject` resolution | piece nodes → outline paths | piece outlines close |
| Seam allowance | `seamAllowance` bool, `width` as a formula, `<CSA>` | width computable, but it is a **run along the edge**, not one value - arithmetic built (`nahtzugabe.rs`, #70) and wired into the DXF export as the piece's cutting line (`saumkontur.rs`, #78, findings 132, 133, see Stage 3) |

**Acceptance criterion:** the piece outlines of 10 patterns match `schnitt.py
piece_outline`.

**Findings behind this stage:** 07 (the `.sm2d` format, read from Seamly2D's
parser); 13 (`schnitt.py` is a transliteration of Seamly2D's C++ and therefore
the yardstick for points; not where two operations share a suffix — it copies
every name under every suffix, in L036 `Ps` comes out 75 mm off); 14 (the five
reader faults found on the blouses: `@` in names, path handle lengths,
`SplPath_` decomposition, the sign in the quadratic, the counter-ray);
16 (arcs were mirrored at the horizontal through their centre, collar
11,270.8 instead of 829.0 mm); 99 (operations, sub-curves, `cutArc`,
intersections of arcs and curves, `trueDarts`); 123 (#42: sub-curves of
`curveIntersectAxis`, `cubicBezier` and `cubicBezierPath`, L036 against
Seamly2D with probe pieces, one per point).

`klon3d umriss` shows the Seamly outline since #42 (the one seams and mesh
use), the drawn one only where a piece cannot be resolved; the bishop sleeve of
Bolero v37 reads 1418.6 mm as in Seamly2D instead of the zigzag 2142.7. An
empty `<measurements/>` no longer takes the next text of the file as its path.

## Stage 2 — Measurements and body

| Part | Content | Status |
|---|---|---|
| `.smis` reader | `<m name value>`, units cm/mm/inch, `@` names, comments | **done** (`smis.rs`, 12 tests; finding 24) |
| Circumference measurement | section loop that encloses the body axis, gaps ≤ 2 cm closed like a seam | **done for the torso** (findings 22, 25) |
| Tape measure | convex hull of the section as in CLO (`massband.rs`), candidate selection for the five torso measurements, sampling | **done**: 21 of 22 reference values on the BaseMesh < 0.00001 cm, same candidates (finding 30) |
| Body base | CLO's `BaseMesh.obj`, 47,650 vertices (40,465 of them used by a face), 164.15 cm tall in the `body` group; the highest unused vertex reaches 166.60 cm | **available** |
| Body base without CLO | MakeHuman `hm08` (`data/3dobjs/base.obj`, 19,158 vertices, group `body` 13,380) shaped with MakeHuman's own **CC0** targets, then scaled to the file's height. Checked 2026-11-03 (finding 123): female target + `maxcup` cup target puts the bust at 98.1 cm against 92.67 (+5.4) where the unshaped mesh was -23.7, and the side of the breast stands 4.85 cm in front of the midline (0.28 without). No target reaches her hip (+13.3) or high hip (-5.7), and the apex-to-apex distance comes out 13.74 against her 22 | **ground body and breast depth available**, apex width and hip left to the Newton fit |
| Shaping (5.2 backup) | `P = base mesh + F_last + Σ F_k · c_k` from the `.bd` fields | **built in**, 5 torso measurements + 8 height fields (finding 24) |
| 58 measurements (5.2 set) | the 5.2 measuring methods (`messung.rs`) | **done**: 57 of 58 on the BaseMesh ≤ 5·10⁻⁷ cm, Wrist +0.035 mm (finding 31); the reference set of 67 (`messung_referenz.rs`) is the row below |
| 67 measurements (reference set) | `messung_referenz.rs`: the measurement set of the reference folder, cut in `f32` (`massband::kantenschnitt_f32`, `normale_f32`) | **done**: 0.00020 / 0.00033 / 0.00014 mm on the three reference meshes (`messung_referenz_clo.rs`) |
| Shaping chain | BASE + DETAIL Human with calibration, symmetry, centering, `.avs`, hand correction, `.smis` mapping (`kette.rs`) | **done**: all test cases of the specification, against CLO ≤ 0.51 mm (finding 31); **wired up**: `klon3d koerper <measurements.smis> [--out]` shapes the body via `koerper.rs` (group `body` in mm with triangles, also the input for stage 4); if the chain's resizing data are missing, it falls back to `anpassen.rs` (test `koerper_weg`) |
| Reference shaping chain | `kette_referenz.rs`: the folder no longer ships the finished converting fields, so the form step is computed — PCA regression over `FeatureData` / `ReducedVertexData` / `PCA_weight`, then calibration, symmetry, centring, `.avs`, hand and head post-processing (`nacharbeit_referenz.rs`) | **done**: regression residual 4.9·10⁻⁷, final values ≤ 0.037 mm, points ≤ 0.026 mm (`formung_referenz_clo.rs`, `kette_referenz_clo.rs`) |
| Three paths | `koerper.rs` tries the reference chain (`Weg::Kette`), then the older chain for old folders (`Weg::KetteAlt`), then the Newton fit (`Weg::Anpassen`) | **done** (`koerper_weg.rs`): each path is checked from its own folder; a folder with only the old `Advanced` files still shapes |
| `.smis` path and HighHip | the reference fields do not carry HighHip, so it is derived from the chain's own BASE mesh | **done** (`kette_referenz_smis.rs`): 5 BASE and 21 DETAIL form steps, both converge, final values within 0.1 mm and points within 0.05 mm of the reference |
| Target person | `person.rs`: `.smis` → CLO fields, heights, measurement lines, active fields | **available** (finding 24) |
| Measurement lines | material lines that move with the mesh (`spur.rs`) **or** fixed sections at the `.smis` target heights | **available** (findings 23/24) |
| Reference measurements (5.2 backup) | 32 values of the `DETAIL_Advanced` stage (heights in cm, apex 119.41, waist 105.71, crotch 76.87) — belongs to the older Newton route (`anpassen.rs`), the reference folder has its own set (finding 64) | **available** (finding 22) |
| Acceptance | measured circumference hits the target value | **passed with the shaping chain** (finding 31): CLO measurement bust +0.03, underbust −0.00, waist +0.02, HighHip at CLO's position −0.01, hip −0.02; sampling waist −0.89, hip −0.01. The old route via `anpassen.rs` does not reach them with the tape measure (finding 30) |
| Height frame | measurement heights from the floor: center the mesh before measuring (lowest point y = 0) | **available** in tape-measure mode; the contour route still cuts 1.95 cm too low (finding 30) |
| Post-shaping between waist and hip | **beyond the reference program** (`nachformung.rs`, #54): the chain hits the circumferences it knows at the places it knows, and keeps its statistical shape in between. On the user's file (waist 70 at 98 cm, high hip 83 at 93 cm, hip 90 at 77 cm) its body puts the waist at 99.6 cm, needs 24.3 cm from waist to hip instead of 21 and measures 76.7 cm at 93 cm; the chain knows the high hip only about 12 cm below the waist, and given at 5 cm (with waist height and waist-to-hip) its model stops after 2 form steps with the underbust at 81.9. So after the chain: a monotone cubic remap of the heights in a band below the hip to halfway up to the underbust (waist and hip to the file's heights, identity at the ends), then a horizontal scaling of the sections about their centre towards a target profile through waist, high hip and hip, flat at hip and waist, fading to 1 at the band ends; only torso and legs near the torso section move | **done**: waist 69.99 at 98.00 cm, high hip 82.99 at 93.00, hip 90.01 at 77.00, narrowest place at 98.0 cm, 83 → 90 rising without overshoot, 5 rounds; bust, underbust and everything above 103.2 cm bit-identical; silhouette: crossings 50 → 50, none new, dents 84 → 88 (4 existing ones slightly deeper at the gluteal cleft, the crotch and the navel). Without the step (`klon3d koerper --wie-clo`) the body stays exactly the chain's. Not handled: the front/back split of the arcs (high hip 44/41 in the file), the scaling is uniform around the section |
| Silhouette | self-intersection, dents on the shaped body | **done** (`crates/klon3d-koerper/src/silhouette.rs`): contacts are classified as crossing, overlap or touch, and a crossing is judged by its penetration width, not by its existence. Against the base mesh, which already carries contacts, the reference shaping reduces crossings 464 → 54, dents 114 → 82 and the widest crossing 1.46 → 1.18 mm; no new crossing is wider than one already present, no overlap, no dent grows. Same rules hold on the older chain. Checked by `tests/silhouette_clo.rs` |

**Acceptance criterion:** circumferences on the mesh within ±1 cm of the `.smis` values
(bust 92.67 / waist 70 / hip 90 / HighHip 83 / underbust 76 — waist 70 since
2026-10-21). That is
the criterion from the handover. Measured like a tape measure, as defined in
`spez/massdefinitionen.md`; test
`kette_clo::abnahme_aus_smis`; both numbers (CLO measurement at the candidate and
sampling) lie within ±1 cm, HighHip applies at CLO's position (finding 31).
That test still runs on the older 85-row copy
`crates/klon3d-pattern/tests/muster/measurements.smis` (waist 74). The current file
has 86 measurements and waist 70; the chain reads bust 92.666, underbust 76.002,
waist 69.987, hip 90.016 on it. `nachformung_clo::body_between_waist_and_hip` runs on
`crates/klon3d-pattern/tests/muster/hexen_2026_10_27/measurements.smis` — the same
measurements without the name — and adds the high hip at the user's own height
(within 0.3 cm).

**What findings 18/22 change (5.2 backup route):** the shaping there is **linear in
the coefficients**, and the 5.2 fields ship with the CLO installation. The field order is
the ID order of the measurements, the last field has the coefficient 1.0, and
`DETAIL_Advanced_TotalHeight` reproduces `BaseMesh.obj` to 0.00014 cm.
No hand-tuned deformation is needed — those fields are the
template. The reference folder has replaced them by matrices computed at load time
(`kette_referenz.rs`, finding 64); this section describes `anpassen.rs` only.

**Integration of the 5.2 route (in order):**

1. Load `BaseMesh.obj` and the 33 fields of `DETAIL_Advanced_TotalHeight`
   (format: `int32 fields, int32 vectors`, then `f32` feature by feature).
2. Check the reference vector `c_ref` (32 values, finding 22) and the starting body:
   `P = G + F_last + Σ F_k c_ref` (G = base mesh) must reproduce the reference exactly.
3. Measurement operator in cm: with `.smis` heights the section is taken **at the
   target height** (bust 111 etc.), otherwise on the reference's material line
   that moves with the mesh (finding 24).
4. Jacobian of the **active fields**, numerically per round (difference step 1.0),
   damped Newton `δ = (A + λI)⁻¹(m_target − m_actual)`, step rejection like CLO's
   driver. The full 31-column solution diverged; it dragged calf, foot and
   wrist along (finding 24).
5. Acceptance: bust/underbust/waist/HighHip/hip within ±1 cm, body
   not self-intersecting. **Reached** (finding 24): 14 rounds, 1.8 s,
   error ≤ 0.006 cm. The silhouette check is done in
   `crates/klon3d-koerper/src/silhouette.rs`.

This list describes the older Newton route (`anpassen.rs`), kept as the last
fallback. The "crotch gap at 73 cm" noted here once was no gap in the mesh
(finding 60: the `body` group is closed; the section went through both thighs).
On the current body the torso section at 73 cm is closed and the crotch lies at
71.5 cm (file: 72). Finding 25's "base mesh open at the seams" counted all groups
of the OBJ (teeth, lashes, eyes); the body itself is closed (finding 60).
`rumpf()` still closes gaps up to 2 cm, which makes measuring possible at every
torso height (finding 22).

### Body without CLO data (finding 123, 2026-11-03)

Every body path above needs CLO: `BaseMesh.obj` for the net, `ConvertingMat_*.bd`
or the reference folder for the shaping. Both are CLO's and cannot be shipped.
The landmarks no longer do — since #56 `koerper::anordnung` finds them on any
mesh (`anordnung_netz::aus_netz`, checked against CLO's own values to 2.27 cm,
mean 0.99 cm). What is still missing is a **free body with a believable form**.

The candidate is MakeHuman (CC0, so it can be shipped): `hm08`, 19,158 vertices,
group `body` 13,380. Its targets are plain text, `<index> <dx> <dy> <dz>` in
decimetres, indexed against that mesh — no matching needed. Measured on the
body group, scaled to 160 cm (findings in the author's notes, section 4b/4c):

| Line | `.smis` | plain `hm08` | + female | + female + `maxcup` |
|---|---|---|---|---|
| Neck 135 | 37.0 | 34.8 | 38.2 | 38.2 |
| **Bust 111** | **92.67** | 68.9 (−23.7) | 69.3 (−23.3) | **98.1 (+5.4)** |
| Waist 98 | 70.0 | 70.9 | 70.1 | 70.1 |
| High hip 93 | 83.0 | 81.9 | 77.3 (−5.7) | 77.3 (−5.7) |
| Hip 77 | 90.0 | 97.4 | 103.3 (+13.3) | 103.3 (+13.3) |

The female target alone does **not** create a breast — it moves the whole body
(19,150 vertices, median 5.16 cm) and makes hip and high hip worse. The volume
comes from the cup target only: at y = 111 the side of the breast then stands
4.85 cm in front of the midline, where the plain mesh has 0.28 cm. The largest
cup overshoots her (bust − underbust 32.7 against her 16.7), so the targets
give the **form**, not the size.

No target fits her hips: `hip-scale-horiz-decr` pulls the high hip to 66.6,
`hip-trans-in` the waist to 61.5, `minweight` the high hip to 72.7. That part
must come from `anpassen.rs`, which is mesh-independent already (Newton on
`Formung` + `Messlinien`). Of the chain, `anordnung_netz` and `anpassen` run on
any mesh; `nachformung` needs a per-vertex segment (`Messdaten::aus_netz` puts
every point in one segment); `formung::Formung::laden`, `kette_referenz`,
`messung_referenz` and `anordnung_clo` are bound to CLO indices and do not.

So the free body is **ground body + breast form for free, hips by Newton**.
Not built yet — it is its own task (see #65).

**What "shape the measures onto the body" has to mean.** Tight latex lies on
every point, so circumferences alone are not enough. The reference body
(`klon3d koerper`, 31,422 points, 160.66 cm) was measured on the **hull**
(`messung_referenz::umfang`, i.e. the tape) at the heights the chain uses:

- Circumferences are exact: 92.666 / 76.002 / 69.987 / 90.016 against
  92.67 / 76.00 / 70.00 / 90.00 cm (≤ 0.02).
- The bust line sits at **y = 113**, not at `height_bustpoint` 111 (hull 90.57
  there, 92.73 at 113). Waist 98, high hip 93, hip 77 are fixed and hit.
- The bust arcs are where the error is, and it is **one number**. The tape
  definition (`bauen_2.5.py`, self-test in `bauen_3.2.js`) is
  `2 · side piece + apex-to-apex = bust_arc_f`; her 56 and 22 give a side piece
  of 17.00. Measured on the hull at y = 113, without choosing any side line:
  front arc **47.95** (56), back arc **44.78** (37), apex-to-apex **13.74**
  (22), side piece **17.10** (17.00). The side piece matches to 1 mm — the whole
  arc error is the apex distance. The chest is narrow and high instead of wide
  and flat, which no circumference fit can see.
- **Cause:** `kette.rs:62` reads `bustpoint_to_bustpoint` (slot 44,
  `LENGTH_APEX_APEX`), but `Konfiguration::klon3d()` never sets `codes[44]`, so
  `zaehlt()` returns false for every step. Counter-test: the same `.smis` with
  the value raised 22 → 30 yields a body that differs by **0.000000 mm** over
  all 31,422 points. The value is carried, never formed.
- The arcs are not read either: `SMIS` has no `bust_arc*`/`waist_arc*`/
  `hip_arc*`, `messung_referenz` has no partial arcs, and `anpassen::Messort`
  knows only the five circumferences. Her `.smis` has them and her blouse uses
  `bust_arc_f` (`Bluse3_Prinzessnaht_parametrisch_v21.sm2d`, `#BustShapeF`).

MakeHuman + `maxcup` is therefore the right **form donor for the breast depth**
(5.91 cm against 0.85), but it brings the wrong apex width (side piece 12.39)
and a hip 6.6 cm too wide. The Newton fit would have to take apex distance and
partial arcs as targets of their own, and those measurements do not exist yet —
that is the core of #65.

The fit is not ready for that either: `anpassen` requires **as many targets as
lines as active fields** (`anpassen.rs:432–438`), and `Messlinien::aktiv`
(`:213`) maps each line to exactly one field (`CIRCUMFERENCE_Bust` …). Five
lines, five fields, quadratic. Adding the apex distance makes the system
overdetermined. It needs damped least squares on the Jacobian already computed
(`a[i][j]`, `:477–485`), or real shape degrees of freedom (radial offset of the
torso loop per height and angle, driven by the arc difference) — the latter is
new modelling, and CLO's own field set has no columns for the arcs either.

## Stage 3 — Pattern to mesh

| Part | Content | Status |
|---|---|---|
| **Boundary edges** | seam line per piece split at the point nodes, length, notches, seam allowance per edge (`kanten.rs`) | **done** (finding 32) |
| **Seam allowance contour** | the allowance is not one width per piece: the seam line is offset **point by point** by the allowance, with a run along the edge - no offset polygon, no self-intersection clean-up (`kanten.rs` keeps only `before`/`after` per node) | **done** for the DXF export (`saumkontur.rs` #74 joins edges and arithmetic, `Muster::dxf_teile` writes the result as the piece's cutting line on layer 8, #78; findings 132, 133). Reconstruction from `kanten.rs` alone, `Ecken::Eigen`; the `<CSA>` paths and the line's own stored direction are still approximations |
| **Seam assignment** | seam list `<pattern>.naehte` next to the pattern: sides made of whole boundary edges, 1:1 up to M:N; `klon3d naehte` checks lengths, notches, coverage (`naehte.rs`) | **done** (finding 32); **seam suggestion** `klon3d naehte --vorschlag` (finding 101): Bluse3, Bluse1, bustier and skirt edge for edge like the hand-written list, direction and mirroring from the seam graph; open: pleats (bolero), slits |
| **Triangulation** | seam line → triangle mesh per piece, approx. 10 mm edge length: boundary split by curvature, ear clipping, Lawson, Ruppert with a minimum angle of 25°, smoothing; exact on an integer grid (`klon3d-mesh`) | **done** (finding 33) |
| **Subdivision** | subdivide both seam partners equally by **length fraction** as in CLO (§4 of the specification), point nodes of both sides merged up to 2 mm; result: point pairs per sub-seam; `klon3d netz` | **done** (finding 33) |
| Seams | support springs between the partners, 6 mm overlap seam in the middle | flush seam with bending across the seam **done** (finding 34); the overlap seam comes with the glue zones |
| Reduction | scale rest lengths: torso 5.5%, sleeves 1–2%, cuff 5% | **done** (finding 34): rest length × (1 − r), mass of the cut piece × (1 − r)²; if the reduction is already in the pattern (Bluse3), r = 0 |
| **Glue zones** | areas that bond two layers firmly: laminate (the whole area of a piece), reinforcement and appliqué (internal shape), overlap seam as a glued strip. Model: CLO's *Gluing* (Glue Fill by outline, Glue Brush painted freely) | **done** as a bond that holds the offset vector (`klebung.rs`, task #11): no sliding, load carried across, `klebt <Teil>` in the placement file; **done** also the bending of the bonded pair (`Stoffpaar::biegefaktor`, task #13): a zone over the whole face bends like the laminate of the same thickness; not against the body ("Tack on Avatar") |

**Acceptance criterion:** all seam pairs of equal length, deviation ≤ 0.08 cm —
the criterion from `pruef_bluse3.py`. **Passed** (finding 32,
test `naehte::abnahme_stufe3_bluse3_v2`): 24 seam pairs, largest deviation
N07 front upper panel seam −0.0737 cm (Seamly decomposition; with the
drawn polyline −0.0758, as in `pruef_bluse3.py`).

**The seam model per CLO** (`spez/naehte_2026_1.md`):
an outline is a list of lines from segment point to segment point; a
seam side is (shape, start and end line with fraction, direction); a seam has
two sides of 1…n seam sides each. The side sums are compared
(Check Sewing Length: red from 5 mm or 10%); when sewing, equal
length fractions are matched to each other. **Notches do not occur in the seam model.**
Auto-Sewing pairs via piece types and edge types on the avatar
(finding 71).

What the reference version adds to that model:

- **Fraction = chord length** (finding 93): a line is sampled into a polyline,
  and the fraction along it is the normalised cumulative chord length of that
  polyline (0 at the start, 1 at the end). A "partial stretch of an edge" is a
  pair (line, fraction), not a new line.
- **Edge length** (finding 110): the sum of its segment lengths; a replaced
  length exists only after merging two edges. The length thresholds of the
  Auto-Sewing labelling rules (100 / 300 / 1000) are millimetres.
- **Edge divider** (finding 111): to match a target length, the run of segments
  with the smallest deviation (below 10) is searched and the edge is split
  into at most three pieces — the model for partial stretches below.
- **DXF import** (finding 97): the split into lines follows the point type, not
  the layer — two consecutive segment points make a straight line, each run of
  curve points between two segment points exactly one curve line; every DXF
  piece is built closed. That is what `klon3d dxf` has to write for CLO.

**Older version, backup only (finding 19):** pairing via notches as `alpha`
with the thresholds 0.1 and 0.03 - no longer present in the reference
version (finding 60).

**Acceptance of mesh and subdivision** (agreed with the user on
2026-10-20, test `klon3d-mesh/tests/bluse3_v2.rs`): mesh boundary within 0.01 cm of
the boundary edge (position and length), mesh area = boundary polygon, minimum angle 25°
(except in piece corners that are themselves sharper), edges about 10 mm, every
sub-seam with the same number of boundary points in sequence on both sides, offset of merged
points ≤ 2 mm. **Passed:** 28 pieces, 6,471 points, 10,734 triangles,
smallest angle 25.1°, median of the inner edges 9.9 mm, 24 seams with 715
point pairs, largest offset 0.25 mm (N02), 34 ms. In the pattern collection (6,193 pieces)
every piece passes.

**Mesh fineness from fabric and seams** (finding 122): Zhang et al., SIGGRAPH
2025, estimate the edge length before the simulation from bending over
stretch stiffness (`c₁r⁶ + c₂r⁴ = B/E`) and refine near seams with ease (only
the longer side), folds and gathers. For latex that gives **6.8 mm**
(Radical Rubber 0.4 mm) and **5.5 mm** (Retifist NR 0.40) where klon3d meshes
a fixed 10 mm. CLO users do the same by hand per piece: 90.7 % of the
triangles in the 47 public projects have particle distance 5, and CLO's mean
edge is 0.807 × the particle distance — klon3d's 10 mm corresponds to a
particle distance of about 12, not 10. The constants are a calibration to
Style3D's solver; the form of the rule carries over, the numbers do not.

**Still missing:** seam springs on the point pairs, partial stretches of an edge
(CLO's Free Sewing), internal shapes (holes, internal lines) and
darts as seam sides, an automatic seam list as a suggestion.

**How the reference computes the seam allowance** (findings 132, 133) - read;
the arithmetic is rebuilt in `crates/klon3d-pattern/src/nahtzugabe.rs` (#70,
PR #72), joined to the piece edges in `saumkontur.rs` (#74) and written as the
cutting line by the DXF export (#78). The allowance is a **run along the arc
length**, not a constant:

```
S   = Σ |P_k − P_{k−1}|          polyline length, without the offset
s_k                              cumulative length up to point k
o_k = a + (b − a) · (s_k / S)    linear, no magnitude, no sign
```

The run is **linear, not absolute**: the earlier reading `a ± |b − a| · (s_k/S)`
with the sign taken from `|b| > |a|` was wrong and is withdrawn. The reference does
compare the magnitudes and picks one of two branches, but both come out as
`a + t·(b−a)`: one branch adds `t·(b−a)`, the other subtracts `t·(a−b)` - the same
value. The sign question therefore decides *which* branch runs, not *what* comes
out (finding 132, §4.3).

At least **four edges**, otherwise nothing is computed. At the ends the two
neighbouring lines are averaged with the factor **0.5**, crossed: the start takes
its own width with the neighbour's inner width, the end the other way round. The
contour itself is a **per-point displacement**, not an offset polygon - the
point's **stored direction** times the width, `new = point + direction × width`,
two point objects per edge. At the two ends of the run one extra step applies: if the displaced
point lies on the line of the neighbouring edge, it is pushed one more allowance
along that neighbour's stored direction - the neighbour of the first edge for the
first point, of the last edge for the last. Where the stored direction points is
not settled by the findings; that it is the outward normal is the assumption a
contour needs, not a result.

Two markers sit on the run: the first is a **one-shot gate** (`0` returns at once,
`1` computes once and clears itself), the second means *the run is computed* (set by
the distributor, read by the contour and by the length check). Both live in the
seam line's **common entry point**, which five line classes share - a class of its
own does not exist, and none of the four candidate classes reaches the contour.

**In klon3d the arithmetic exists and is wired into the export.** `nahtzugabe.rs` has
the run (`verlauf`), the crossed 0.5 averaging (`startwert`/`endwert`) and the contour
(`kontur`), 15 tests with the numbers of the findings. `kanten.rs` holds two values per
point node (`before`/`after`) and reports edges that carry an allowance without a seam.
`saumkontur.rs` (#74) joins both: it turns a `Teilrand` into the displaced segments and
takes the outward normal per chord from the winding of the seam line - the direction the
findings leave to the line data. `Muster::dxf_teile` (#78) calls it and writes the result
as the piece's second, closed line on layer 8; `klon3d dxf` prints both lengths. Not
modelled: the `<CSA>` paths that replace the allowance between two nodes, and the way the
reference steps at a corner where both sides carry an allowance (we average, it mitres -
5.05 mm on the skirt's waistband, documented as a limit, not fixed).

**Meshes in the viewer:** `klon3d netz --svg` draws meshes and
seam points, `--obj` writes them; `klon3d-fenster` does not show them yet.

**For a 3D display of the seam** the viewer lacks the
**3D projection**. `klon3d-view` only draws 2D so far.
That is a separate step, not part of the pairing.

## Stage 4 — Solver (the core)

**Implicit step with CG like CLO** (backward Euler, preconditioned
CG, contact filter; ADR-020, 2026-10-27); **XPBD** remains as a second,
independent computation path for checks on the CPU (this supersedes ADR-009).
The solver runs on the **GPU via `wgpu` compute shaders**; the
**CPU reference** `ImplizitCg` (`f64`) with the same method is the
yardstick (ADR-018). Reason: the goal also
includes **motion** (stage 7), and that needs the computing power of the GPU.

**Steps:**

| Step | Content | Done when |
|---|---|---|
| **4a Tube with XPBD (cross-check path)** | crate `klon3d-sim`, Rust `f64`, data as arrays (struct of arrays), constraints ordered by graph coloring, substeps. Distance constraints from the mesh edges (finding 33) with rest length × (1 − reduction); seam constraints from the point pairs, bending also across the seams; gravity, damping; collision against a cylinder with friction from the physical load; position, displacement and correction kept separate (for f32). Test case: rectangle sewn into a tube, around a cylinder; `klon3d roehre` | **done** (finding 34): both fabrics, 0/5.5/10%, hoop strain −0.06 pp (tolerance ±0.5), seam 0.0000 mm, lies against the cylinder, mesh resolution 0.05 pp; the slip test follows Coulomb to 0.05%. Condition: **softness condition** α/h² ≥ 9·(wᵢ+wⱼ) — tube 116–434 substeps per frame |
| **4a′ Tube, implicit with CG like CLO (CPU reference, basis)** | second computation path `ImplizitCg` in `klon3d-sim`: backward Euler, matrix-free PCG with 3×3 diagonal blocks, contact filter, normal force from the system, Coulomb friction, warm start; CLO setting of 1 step per frame, 50 CG iterations, seam spring 500 N/m | **done** (finding 35): tube, both fabrics at 5.5/10%, with the same acceptance as 4a, slip test at most 1.6% off the discrete path; frictionless, the same equilibrium as XPBD; blouse 13–137 ms per frame instead of 1.2–4 s, on the GPU around 310 instead of 140,000 dispatches per frame. Limits: a stiff seam only just converges, loose pieces come to rest poorly with 50 iterations. The basis since ADR-020 |
| **Substeps per frame** | `schritte_je_bild` in `ImplizitParameter` | **done**: default 5, matching the `Nonlinear` preset (residual 1e-5). With one substep a 5.5% Radical band on the reference body does not settle and sits 2-4% off the tape-measure circumference; the error falls monotonically with the substep count (0.06% at five) |
| **Body as collision body** (before 4c) | body from `klon3d koerper` as a mesh in `klon3d-sim` (`netzkoerper.rs`, `Koerper::Netz`): signed distance via the nearest point and angle-weighted pseudonormals, grid search (20 mm cells, 10 mm search depth), in the implicit step a penetration check along the path with Coulomb friction on the impact. Test cases: tube on a cylinder mesh, band around the waist, point rain | **done** (finding 94): tube on the mesh, ε within 0.002 pp of the analytic case, frictionless the same equilibrium, slip test −0.11/+0.41%; band (10 mm mesh) at rest in all four cases, edges against the tape measure in their plane: Retifist ≤ 0.25%, Radical ≤ 2.7%, no point inside the skin; point rain at 6 m/s 100/100 at the impact location (without the penetration check 10/100). Open: the Radical band in the 5 mm mesh folds and does not come to rest without self-collision; body vertex against cloth triangle; moving body |
| **4b GPU** | the implicit step from 4a′ as a WGSL compute shader via `wgpu`, `f32` (ADR-020): forces and 3×3 Jacobian blocks per edge, seam and bending, gathered per point via neighbor lists (no atomics); matrix-free A·p, block-Jacobi preconditioner with contact filter, dot products as reductions in a fixed order, warm start; contact, normal force and friction per point; collision and self-collision via spatial hashing. To be settled with 4b (finding 35, section 5): a better preconditioner for stiffer seams and loose pieces; whether edge springs are replaced by a triangle continuum | the same equilibrium as `ImplizitCg` on the tube and on the blouse in a standing pose (define and justify the tolerance), also at body height; slip test as in finding 35; Bluse3 v2 at ≥ 30 frames per second on the RTX 3090. **Partly done** (#27, #29, #33): the solve runs on the GPU and is **2.09× faster than the CPU per solve** on the blouse (5.34 against 11.15 ms, 120 solves after a warm-up, same `cg_max` and tolerance), 1.77× per frame (**38.7 against 68.6 ms**, was 47.3 before #33); on the CLO body 382/278/281 ms per frame against 455/383/423 — still above 33 ms. The operator runs over the whole grid (0.037 against 0.46 ms per pass), the reductions on one workgroup, all passes in one submission; the release pass reuses `Δv` on the device (#33: 43.2 → 37.2 ms, 14 %, and the second solve drops out when no point is held). **30 fps reached with #36:** the system build moved to the device as well, the frame is **27.6–29.1 ms against 33.3** (three runs, 2026-11-02), and `bluse_schafft_30_bilder` runs in the suite. Every record of the device build agrees with the CPU build (`aufbau_wie_cpu`). Still open, each as a change of its own: the solves run into `cg_max` unconverged (the CPU needs **182** iterations to reach 1e-5; every preconditioner tried in #41 lowers iterations only at more operator work, see below), contact queries and self-collision on the GPU, the CLO body (278–382 ms per frame before #36, not measured again), a CPU fallback for small systems such as the tube; GPU tests skip themselves without an adapter and report it |
| **4c Placement** | place the 2D pieces around the body before sewing. CLO puts them on arrangement points on hull bodies around torso, arms and legs (finding 71, addendum); klon3d needs its own simple approach, e.g. zone and side per piece (in the seam list or a separate file) and cylinders or ellipses from the body measurements of stage 2. Then the seam constraints pull the pieces together. **Built** (`platzierung.rs`, placement file `<pattern>.lage`, `klon3d anziehen`): vertical torso prism from the convex hull of the torso points, around the arm the cross-section hull per centimeter, neck seam with height and radius per angle from the four landmarks (slanted, not a horizontal circle; finding 121); per piece: zone, reference point (also from another piece of the same block), circumferential position in degrees or cm, height at a reference height, `gespiegelt` (mirrored), `drehung` (rotation), `abstand` (offset) | all pieces of Bluse3 v2 lie around the body without penetration, the seams close. **Largely done** (findings 95, 96): 16 sewn pieces placed (12 glued-on ones are missing until glue zones exist); dressed with a limited sewing speed (0.1 m/s) and body vertex against cloth triangle during sewing, around the arm a length-preserving arm prism: no point inside the skin, every seam median < 0.8 mm, **98.6% of seam pairs ≤ 10 mm**, 18 of 24 seams fully closed (finding 95 without both: 92.4%). **Generality** (finding 98): Bluse1 v25 and bustier v1 as further test patterns, all three with the same settings — sewn while held (without gravity), cloth points pushed out of the skin beforehand, seam spring 2,000 N/m during dressing, thickness per piece (`dicke` in the placement file), then 2 s of settling and a 10 s hold phase under full gravity (ADR-023): **Bluse3 99.4%, Bluse1 99.1%, bustier 94.6%**, no piece sinks more than 0.4 mm. Finding 100: Bolero v37 96.5% (pleats as darts), skirt v36 95.1% (holds, but not fully at rest: flared panels overlap on the vertical prism); finding 101: seam list and placement **automatic** (`klon3d naehte --vorschlag`, `klon3d lage`); dressed entirely without hand-written files, all five hold (Bluse3 98.6%, Bluse1 98.0%, bustier 96.0%, bolero without sleeves 96.0%, skirt 95.7% — the skirt reacts chaotically to tenths of a millimeter in the initial placement); the placement suggestion lies 0.3–2.8 cm off the hand-made placements. Finding 102: the unrest came from truncated CG solves (50 iterations); with up to 600 iterations during dressing, a torso hull that widens under flared panels, and settling until at rest (2–20 s) before the hold phase: **Bluse3 98.9%, Bluse1 98.9%, bustier 93.6%, bolero 94.5%, skirt 99.2%** with hand-written files, all hold (±0.2 mm), the skirt is insensitive to the initial placement; computing time up to three times longer. Open: corners (sleeve hem, collar end, cup attachments), zones for legs and head, gaps in the pattern reader (`cutArc`, `pointOfIntersectionArcs`, `pointOfIntersectionCurves`, curve names with an underscore) |
| **4d Test blouse v26 standing** | the whole blouse on the shaped body from `klon3d koerper` | acceptance criterion below; in addition "holds" per ADR-023: after dressing ≥ 10 s of full gravity, no piece sinks more than 5 mm, nothing pinned. **Built** (`klon3d-sim/tests/bluse2_stehend.rs`, task #31): the pattern has no seam list, so `bluse2_v26.naehte` is written by hand — the 16 seams of `klon3d naehte --vorschlag` plus the collar on the neckline (V17), the outer collar on the inner at the fold (V18) and the plunge trim on the plunge edge (V19); the 9 decorative discs are left off on purpose. `bluse2_v26.lage` places 13 of the 22 pieces. Dressed with the settings of 4c (200 frames sewing, settle, 10 s hold), 6 022 points, about 5 min per run, **99.2 % of the seam pairs ≤ 10 mm** (764 of 770), at rest at 0.0002 m/s, **no piece sinks at all**, deepest point +0.20 mm. **Four of the five faults show**: the plunge opening at the bust goes from 130.0 mm flat to 175.3 worn (+45.3); the shoulder blade carries +11.08 % against +5.84 % for the back pieces as a whole (+5.24 pp); the sleeve at armpit height reads +4.94 % (the pattern was cut 4.4 % / 2.2 % too small); the collar's free upper edge stands 31.2 mm off the skin at its two front ends, and the ends are 143.0 mm apart. **The sleeve cap does not stand off** — 0.7 mm median air over the shoulder — because the seam list sews the cap to the armhole at matching lengths (41.5308 against 41.5310 cm), so no excess can lift; on the real garment the stand-off comes from the cap's shape, which a seam list cannot express. The test pins that finding. Open: seams V08/V09/V10/V13/V17 keep one pair each at 20–30 mm (the shoulder corner and the armhole, where the body is not covered by the hull), and the donut and discs are not simulated |

**Status of 4b (tasks #27, #29): the CG solve runs on the GPU and is faster
than the CPU on the blouse.** `gpu.rs` solves `A·Δv = b` with the same
matrix-free PCG in `f32`: the operator over edges, seams and bending gathered
per point over a neighbour list, the block-Jacobi preconditioner, the contact
filter, fixed-order reductions, warm start and the same stopping rule.

The operator runs over the **whole grid**, everything else on **one workgroup**.
`A·p` has no dependency between points, so it needs no barrier inside and is
about 15 times faster spread over the card than gathered by a single workgroup
(0.036 against 0.57 ms per pass on the blouse). The reductions, the
preconditioner and the vector updates do need the barrier and stay in one
workgroup of 1024 lanes. The passes of a solve go into **one submission**; the
pass boundary is the global barrier between them and costs about 6 µs. The CG
state sits in the data buffer, so it survives the boundary.

The number of recorded iterations follows the previous solve. A pass past
convergence returns at once but still costs its dispatch — recording the full
`cg_max` wasted about 30 ms per solve.

**The release pass reuses the solve's result on the device.** After a solve the
step computes `A·Δv` again to decide which held points pull and can be released.
That operator is unfiltered, so it does not depend on *which* points are held —
only the second solve does. The GPU backend keeps `Δv` in its buffer and applies
the operator there instead of uploading the whole system a second time
(`Loeser::mal_a_geloest`). The blouse has no body, hence no held points: the
release pass is skipped outright and the second solve never runs. Measured
alternately against the same commit, five runs each: **43.2 against 37.2 ms per
frame** (median, 14 %).

Measurements: `cargo run --release -p klon3d-sim --example gpu_takt` solves a
fixed system 120 times after a warm-up, same `cg_max`, same tolerance. Runs on
different days repeat within a few percent; the numbers below are from
2026-11-02 with nothing else on the GPU.

| | CPU (`f64`) | GPU (`f32`, wgpu) |
|---|---|---|
| blouse, 6 471 points, one solve | 11.15 ms | **5.34 ms** (2.09×) |
| blouse, whole frame | 68.6 ms | **38.7 ms** (1.77×) |
| tube, 1 236 points, one solve | 2.10 ms | 3.45 ms |
| tube, whole frame | 2.23 ms | 9.5 ms |
| `A·p`, one pass on the blouse | - | **0.037 ms** (whole grid) / 0.46 ms (one workgroup) |
| empty pass | - | 0.008 ms |

Repeated runs of the blouse solve gave 2.07–2.21×, the frame 37.2–40.1 ms.
**The tube stays slower**, in the frame by 4×: at 1 236 points the fixed cost per
solve (upload 0.1 ms, readback and wait about 3 ms) dwarfs the arithmetic, and
its frame converges in a few iterations where the benchmark forces 50. The GPU
pays off from a few thousand points; a solver choice that falls back to the CPU
for small systems would be a change of its own.

On the CLO body (`japanese_school_uniform.zprj`), full settings,
`klon3d anziehen [--rechenweg gpu]`, medians per phase:

| | CPU (`f64`) | GPU (`f32`, wgpu) |
|---|---|---|
| frame, sewing | 455 ms | **382 ms** |
| frame, settling | 383 ms | **278 ms** |
| frame, holding | 423 ms | **281 ms** |
| CG iterations per solve | 122 | 135 |
| solves that did not converge | 0 of 9 792 | 1 of 9 792 |

**The 30 fps criterion holds since task #36: 27.6–29.1 ms per frame against
33.3 ms** (three runs of `gpu_takt`, 2026-11-02; was 38.7 ms). The system build
runs on the device: three passes write direction, cross factor and force per
distance constraint, the dihedral gradients per bending constraint, and per
point force, load, `H·v`, right-hand side and the inverted diagonal block - one
writer per constraint, one gather per point, no atomics. The CPU only hands
over topology, stiffnesses and rest lengths, and uploads those only when they
change; per substep positions (in two `f32` parts, `hi + lo`, so a difference
`x_j − x_i` keeps `f32` precision of its own size), velocities, the warm start
and the contact set go up. A step with springs (glue zones, self-collision) is
built in full on the CPU and solved there, as before.

| per substep, blouse | before #36 | after #36 |
|---|---|---|
| system build on the CPU | 2.06–2.12 ms (full) | 0.36–0.53 ms (topology and contacts) |
| upload | ~0.6 ms | 0.32–0.38 ms |
| system build on the device | – | 0.067 ms (three passes) |

Every record of the device build is checked against the CPU build on Bluse3 v2
(`aufbau_wie_cpu`): indices exactly equal; relative to the largest magnitude of
each field, stiffness 5.9e-8, direction 1.7e-7, cross factor 2.0e-7, bending
gradients 2.2e-7, force 2.2e-6, load 1.2e-6, right-hand side 9.3e-7 - all under
1e-5. The inverse diagonal block reads 3.7e-5 and is held to `κ·ε` instead:
the blocks reach a condition number of 692 (in-plane `h²·k` against the
out-of-plane mass), and an `f32` inverse is no better than 692 · 1.19e-7 =
8.2e-5. It only steers the CG; the operator stays at 1e-4.

What the frame was made of before #36, measured 2026-11-02 on the blouse:

| share | per frame | what |
|---|---|---|
| 26.7 ms | 5 × 5.34 ms | the five CG solves (50 iterations each) |
| 10.5 ms | 5 × 2.10 ms | building the system on the CPU — edge and bending geometry 1.65 ms, `diag` 0.27, `h·H·v` 0.16 |
| ~1.5 ms | | the rest (friction, position update, warm start) |

Two levers were measured and **rejected**, not assumed away:

* **Parallelising the system build** (edge and bending geometry are independent
  per constraint; forces still applied in index order, so bit-identical). With
  `std::thread::scope` per substep 2.10 → 4.24 ms, with a `rayon` pool
  2.10 → 2.61 ms. The build is memory-bound, not compute-bound; spawning costs
  more than the work. Reverted, no new dependency.
* **A split of the one-workgroup passes.** The iteration costs 0.107 ms, of which
  `A·p` over the whole grid is 0.037; the other 0.070 ms are the reductions and
  the vector updates on one workgroup. Splitting them into more passes does not
  pay: an empty pass costs 0.005–0.008 ms and a wide update pass 0.0115 ms, so
  six passes would cost more than the single workgroup they replace.

Also measured and rejected: a matrix-free preconditioner assembled linearly in
the operator (`P⁻¹·ap` in one pass, saving a barrier) — 5.09 against 5.15 ms per
solve, i.e. nothing; the earlier 2 % was noise from a busy GPU.

**The solver does not converge, and that is the real finding.** With
`cg_max` 50 it stops at a relative residual of 6.0e-3. The CPU with the same
system needs **182 iterations** to reach the tolerance of 1e-5
(`tests/vorkond.rs` measures both ends through the real solver: 50 iterations →
`nicht_konvergiert`, rest 6.02e-3; budget 400 → converged at 182). Every solve in
the frame therefore runs into its budget and stops unconverged. `cg_max` was left
untouched — lowering it to buy frames is named as forbidden in the task, and
raising it makes the frame slower, not better (see the cost table below).

The CG curve falls geometrically by 0.942–0.958 per iteration over the whole run
(5.36e-2 at 10, 1.75e-2 at 30, 6.02e-3 at 50, 2.42e-4 at 120, 1e-5 at 182): one
rate, no plateau and no fast phase to exploit. The condition number is
**κ(A) = 1961** (λ_min 1.31e-5, λ_max 2.5677e-2, power and shifted power
iteration), and the 3×3 block diagonal changes almost nothing:
**κ(D⁻¹A) = 2097** in the shorter run and **3434** in the longer one, because
λ_min of the preconditioned operator keeps falling (7.4e-4, see
`vorkond_diag11`). The diagonal is therefore nearly the identity — the mass
share of its trace is 9.2e-6 to 2.4e-3, and mass-*only* preconditioning makes
things far worse (κ 7039). The measured rate 0.9513 corresponds to κ_eff ≈ 1600,
in the same range as κ(A) = 1961; CG once again beats the classical bound (a rate
of 0.9558 would need 255 iterations, it takes 182), so the 182 iterations are
simply the cost of an h⁻²-type spectrum.

**#41 measured every candidate and came up short (acceptance point 4).** The
decisive quantity is not iterations but work: a V-cycle applies the fine operator
several times per CG iteration, and the per-iteration overhead (reductions,
vector updates) is charged per iteration, not per pass. Cost per substep is
`iterations × (passes/iter × 0.0382 ms + 0.0658 ms)`, frame is five substeps
plus 0.48 ms topology build each, and point 2 allows 33.3 ms:

| variant | aggregates | iterations to 1e-5 | fine passes | vs. 182 | ms/solve | frame | point 2 |
|---|---|---|---|---|---|---|---|
| block-Jacobi, budget 50 (today) | — | stops at 50 | 50 | — | 5.20 | 28.3 | met, **unconverged** |
| block-Jacobi, converged | — | 182 | 182 | 1.00× | 18.9 | 97 | 2.9× over |
| additive two-level | 16 … 512 | 112 … 177 | = iterations | 0.62–0.97× | 11.6–18.4 | 61–94 | 1.8–2.8× over |
| one-level V(1,1), ω 0.7 | 16 | 55 | 223 | 1.23× | 12.1 | 63 | 1.9× over |
| one-level V(2,2), ω 0.7 | 16 | 43 | 263 | 1.45× | 12.9 | 67 | 2.0× over |
| one-level V(2,2), ω 0.7 | 32 | 49 | 299 | 1.64× | 14.6 | 76 | 2.3× over |
| one-level V(3,3), ω 0.7 | 16 | 38 | 311 | 1.71× | 14.4 | 74 | 2.2× over |
| four-level V(1,1), ω 0.7 | 965→121→16→2 | 61 | 247 | 1.36× | 13.5 | 70 | 2.1× over |
| four-level V(2,2), ω 0.7 | 965→121→16→2 | 44 | 269 | 1.48× | 13.2 | 68 | 2.0× over |
| Chebyshev, degree 1 | — | 182 | 182 | 1.00× | 18.9 | 97 | identical to block-Jacobi |
| Chebyshev, degree 3 | — | 135 | 407 | 2.24× | 24.4 | 125 | 3.7× over |
| mass-only diagonal | — | never | — | — | — | — | κ 7039 |

Acceptance point 1 counts **iterations**, and there the target *is* met: V(3,3) at
16 aggregates needs 38, four-level V(2,2) 44, V(2,2) at 16 aggregates 43 and at
32 aggregates 49. What is not met is the purpose behind it, and the frame column
shows why. The 33.3 ms of point 2 are **6.18 ms per substep, i.e. 59
block-Jacobi iteration-equivalents** at 0.104 ms each. Fifty iterations at one
operator pass each fit into that with room to spare — but the only preconditioner
in that class is the block diagonal, which needs 182. Every variant that meets the
iteration bound pays for it in passes per iteration and lands at two to three
times the frame budget.

The cheapest solve in the table is the one that does not converge: block-Jacobi at
its 50-iteration budget needs 5.20 ms per substep, converging it needs 18.9 ms.
The 28.27 ms of today are bought by stopping at a residual of 6.0e-3, so the frame
target is currently met *because* the solve is not solved. ω = 1.0 diverges
at V(1,1); damping 0.7 is required, and the asymmetric V-cycle variants (1,2),
(2,1), (3,1) land on the same pass band (1.56×, 1.56×, 1.45×). The frame column is
a *lower* bound: it charges the extra fine passes and leaves out the smoothing on
the coarse levels, which comes on top.

Three measurements were retracted on the way and must not be reused:

* an earlier "κ ≈ 1.3–1.4" came from a broken diagnostic — full
  reorthogonalisation destroys the three-term recurrence and ≤400 Ritz values
  only resolve the bulk. Control: Lanczos on a Euclidean-symmetric `A` gave
  λ_max 1.43e-2 against power iteration's 2.5677e-2.
* the first two-level and multigrid runs assembled the coarse operator with the
  edge off-diagonal blocks **positive** (it is `A_ij = −K`) and the bending
  blocks **without `h²`**. With both fixed and the coarse matrix proven against
  `R·A·Rᵀ` (3.5e-16 relative, checked before the Cholesky factorisation, which
  overwrites the matrix), the hierarchy does help iterations — but never passes.
  `tests/vorkond.rs` keeps that guard, so the mistake cannot come back silently.
* "deflating the 40 slowest modes lowers κ from 479 to 388" came from the same
  class of error: that Rayleigh–Ritz run reports its smallest mode at 5.27e-3,
  while shifted power iteration puts λ_min at 7.4e-4, so it never reached the
  low end and the "40 slowest modes" were 40 modes out of the bulk. Whether
  deflation pays on this system is therefore **open**, not measured.

What is left for a next attempt is a change of the operator or of the frame
structure, not of the diagonal. Three directions the measurements point to, each a
task of its own: an incomplete Cholesky (needs a factorisation the GPU cannot do
without atomics); a cheaper iteration rather than fewer iterations, since 0.066 of
the 0.104 ms per iteration is reductions and vector updates on a single workgroup
and halving the iteration count is worth less than halving that overhead; and an
operator that resolves the stiff bending term more directly, given that the
bending blocks carry the `h²` that makes the spectrum h⁻²-like. What does *not*
follow from these measurements is a cheaper tolerance: the curve has no fast
phase to stop in (#41 measured one rate from 10 to 182 iterations).


For scale, the reference program measured while running (finding 113, on
`japanese_school_uniform`): it takes **24 solver steps per second** (a fixed
step of 1/24 s, 41 ms apart, stable to 1.7 % over three runs) and draws two
frames per step; about **336 GPU dispatches per step**, the CG iterations
running inside a single dispatch rather than one dispatch each; the GPU at
57–60 % and 12–17 CPU cores busy with driving it. So it does not run at 30
steps per second either, and the iteration count per step cannot be read from
the dispatches.

What is proven (tests, `cargo test --release -p klon3d-sim --test gpu`, all
green on the RTX 3090, and skipped without an adapter):

* `A·p` of the same system agrees with the CPU operator to **1e-4 relative**
  (constant, random and velocity vector);
* the tube with 5.5 % Radical reaches the **same equilibrium** as `ImplizitCg`
  (circumference strain within 0.02 pp, seam gap < 1 mm, radius per point
  within 0.05 mm);
* the slip test follows the discrete path within 2 %;
* without a body, both backends shrink the tube to the same minimum (largest
  point distance 1e-5 m);
* every record of the device build agrees with the CPU build (`aufbau_wie_cpu`,
  numbers above);
* `bluse_schafft_30_bilder` asserts the 30 fps criterion (33.3 ms) and runs in
  the suite since #36. The tests of the binary take a common lock, because
  measured beside the other tests it read 77.7 ms.

Both acceptance runs on the CLO body were reproducible to the byte up to #33:
the CPU run identical to the reference of task #25 (`E86DDF85…`), the GPU run to
the one of task #27 (`F9CE8A40…`). The GPU run computes its forces in `f32`
since #36, so its hash changes; it was not run again.

The CPU and GPU states still differ by median 3.5 mm, mean 5.0 mm, max 48 mm
point to point; that is the f32 solver's own drift on a frictional garment
(task #27), not a change of this task.


Not in this slice: contact distance queries, the penetration push, friction and
the position update stay on the CPU (they are geometry queries); the release
pass reads `A·Δv − b` from the device;
springs of glue zones and self-collision go to the CPU backend and are counted
(`GpuLoeser::ausweichungen`) instead of being dropped; self-collision is not on
the GPU; nothing is claimed about bodies at height or moving bodies.

**Other solvers on the same scenes** (finding 109, klon3d as of finding 102,
same scene, start pose, phases and measuring scripts, three runs each): the
PPF contact solver (ZOZO, Apache-2.0) and ClothCraft dress Bluse3 v2 and skirt
v36 **without a single penetration** and with 100 % of the seams closed, while
klon3d let cloth through the body (6,436 / 1,061 triangle pairs) and through
itself (413 / 1,036 pairs away from seams). klon3d held most steadily (0.0 /
0.35 mm in 10 s; PPF 0.02 / 1.7 mm; ClothCraft without friction lost the
skirt). Cost per simulated second: ClothCraft 2.1 / 1.5 s, klon3d on one CPU
thread 4.7 / 11.6 s, PPF 25.6 / 27.3 s; klon3d and ClothCraft repeat
bit-identically, PPF does not (end poses up to 54 / 142 mm apart). The
penetrations are the open contact work of stages 4 and 7 (cloth against body
vertex, self-collision on the GPU).

| Constant | Meaning |
|---|---|
| Distance | distance constraint, stiff for woven fabric, soft for latex |
| Bending | very low stiffness — latex barely kinks |
| Volume | for "body hug" (5% reduction) |
| Body collision | cloth vertex against body triangle, Coulomb friction, penetration check within the step (finding 94); body vertex against cloth triangle as a post-correction, during dressing while sewing (finding 96) |
| Self-collision | as in CLO (finding 116): proximity springs point-triangle and edge-edge up to r₁ + r₂ + 0.04 mm with r = (thickness + additional thickness 2.5 mm)/2, intersections resolved via intersection regions with a constant push, impulse with position correction to 0.1 mm; spatial hashing |
| Air damping | per material profile |
| Gluing | points of both layers in a glue zone rigidly coupled; the glued area bends like a plate of the total thickness |

**Latex material profile:**

| Quantity | Value | Source |
|---|---|---|
| Thickness | 0.4 mm (torso), 1.0 mm (cups, collar) | pattern files |
| Density | 0.95 g/cm³ → 0.38 kg/m² | assumption, to be tried out |
| Stretch | 5–10% almost linear, high elongation at break | assumption |
| Bending stiffness | very low | assumption |
| Friction | high on skin | assumption |

The handover document explicitly marks these values as **an assumption, not
confirmed**. They are among the first things to calibrate.

**Acceptance criterion:** standing, the simulation shows the five known faults
of test blouse v26 (see below).

**Gluing (the user's request, 2026-10-19):** latex is glued, not sewn. klon3d
should be able to do this itself, not via a detour through CLO. **Done** (task
#11) as glue zones: a bonded point holds its **offset vector** to a triangle of
the layer below (CLO's tack spring, finding 59), one spring per point, stiffness
from the upper fabric's in-plane value. The placement file carries it as
`klebt <Teil>`, and `klon3d anziehen` reports coupled points, the largest rest
gap and the points without a counterpart.

What that does and does not give: two bonded layers no longer slide, and a
displacement of one layer carries the other along (test `klebung`). The
comparison planned here does **not** separate the cases: on a 200 × 40 mm strip
held at both ends, two loose layers, two bonded layers and one 0.8 mm laminate
sag within 0.2 % of each other (9.254 / 9.254 / 9.235 mm), because the sag is
carried by membrane tension and `2 × 348 N/m` in parallel equals the laminate's
`696 N/m`. That test stands, with its explanation, and would fail if the strip
ever started to depend on bending.

**Bending of the bonded pair** (task #13): the bond is a shear spring and
cannot make a plate, so the zone carries a bending term of its own. Every
bending constraint whose two triangles lie **inside** the bonded area has its
compliance divided by the pair's factor, derived from the common neutral axis
(`Stoffpaar::biegefaktor`, `4` for two equal layers). That reproduces the
`Latex_Laminat_2x0.4mm` row of `MATERIAL.md` exactly — measured on a strip bent
onto a circular arc (no membrane strain), the bonded pair carries `0.0178 µJ`
against the laminate's `0.0178 µJ`, and `4.0000×` two loose layers
(`0.0044 µJ`). The factor stays inside the zone: a patch covering half a piece
stiffens that half plus one mesh element of border, never the rest.
`Szene::klebungen_setzen` applies it and starts from the as-built compliance, so
repeating or removing zones cannot compound.

Limits of the term: the factor is one number for the pair, so layers of
*different* thickness get the parallel-axis share distributed in proportion to
`D` rather than to `D + E·t·c²`; the equal-layer case of `MATERIAL.md` is exact.
Deflections of very floppy pieces are not measurable in this model at useful
mesh sizes — a 40 mm cantilever shears instead of bending and responds to a 10×
stiffer bending constraint by 0.03 %, which is why the acceptance measures the
plate stiffness through bending energy at a prescribed curvature.

Model for the user interface: CLO help "New in…" (Glue Fill, Glue Brush).

## Stage 5 — Inspection output

This is the actual purpose, not the display:

| Output | Shows |
|---|---|
| Strain map | **done for the measure** (`dehnung`, #17): principal strains per face, engineering strain `λ − 1`, in two frames — against the rest shape the solver uses (pattern × `1 − r`) and against the pattern itself, `(1 + e)(1 − r) − 1`; per-piece table in `anziehen` (faces, rest area, min/median/p90/max, count above 10 %, pattern median), full map as `dehnung.csv` via `--dehnung <dir>`. Colouring and per-frame output still open. Bluse3 v2 after the run: median 0.87 %, 90 % 10.0 %, 942 of 9 435 faces above 10 % (sleeve and side panel), 0 % of the front and cup pieces above. Exact checks without data: a flat piece at 12 % reads exactly 12 %, rest area = pattern area × `(1 − r)²` to 1e-12 |
| Distance map | **done for the measure** (`abstand`, #15): signed distance cloth point → skin surface, outside positive, with the nearest skin point and its normal; per-piece table in `anziehen` (min/median/p90/max/count inside), full map as `abstand.csv` via `--abstand <dir>`. Colouring and per-frame output still open. Bluse3 v2 after the run: min +0.2 mm, median 2.4 mm, max 36.6 mm, 0 of 5 538 points inside the skin. Cross-check on the reference garment mesh: median 20.4 mm, min 4.3 mm, 0 inside, 88 718 points in 0.6 s |
| Seam position | **done for the measure** (`nahtlage`, #25): smallest distance of every seam to the six named body points - armpit, shoulder point, bust point, left and right - with pair index and share along the seam, per-seam table in `anziehen`, `--nahtlage <dir>` writes `nahtlage.csv`. Two landmarks were added to `Anordnung` for it, the armpit (`IndexSet.txt` rows 16/17, the lowest of the three armhole points and the only one in the torso segment) and the bust point (rows 18/19); both are checked against `symmetry_data.sym` and `range_list.rlist` like the existing pairs. The seam is the **polyline through the pair midpoints**, not the sampled points: on a 20 mm mesh a nearest-vertex search reads 1.6 mm too much, which is the size of the numbers this map reports. Exact checks without data: 50 mm off the seam end reads 50.000 mm; 30 mm beside the middle of a segment reads 30.000 mm while the nearest vertex says 31.6; a 10 mm shift reads 10 mm less; the share is 0 at the first pair, 1 at the last and matches the length share to 1e-9. Bluse3 v2 after the run: `ende.obj` byte-identical, and the seam closest to each body point is the seam that runs over it - armpit 5.0 / 5.3 mm (armhole seams N05L/N05R), shoulder point 17.1 / 10.9 mm (the same), bust point 16.5 / 12.3 mm (bust seams N08L/N08R). **The numbers are not zero and are not claimed to be**: the pattern shoulder point sits 17 mm from the body landmark and the side seam starts 22 mm from the armpit, which is the placement offset this map exists to show. Colouring and `klon3d-view` stay open |
| Stress map | **done for the measure and the display** (`spannung`, #23): principal membrane forces per face in N/m, `strain × the piece's stiffness`, with the piece's own 10 % line in the table and the count above it, `--spannung <dir>` writes `spannung.csv`, `--farben` writes `spannung.obj` on a fourth fixed scale (0–35 N/m). **The honest limit is in the module doc**: with one material the solver's linear membrane makes this the strain map with other axis labels, so the map earns its place through absolute units (35 N/m is the documented 10 % line of the reference latex) and through mixed stiffness per piece, which `anziehen` really builds (`material.mit_dicke` per piece). Exact checks without data: 12 % on the documented latex reads 41.79 N/m hand-computed; two pieces in **one** scene keep their own stiffness (348.27 against 696.55 N/m) at identical strain. Bluse3 v2 after the run: median 3.04 N/m, p90 34.78, max 3612.48 (one face at the known 1037 % outlier), 942 of 9 435 faces above the 10 % line — the same 942 as the strain map, the cross-check that both describe one state; `spannung.csv` reproduces `dehnung.csv` × E for all 9 435 faces within the CSV rounding, `ende.obj` byte-identical, colour matches the scale for 5 538 of 5 538 points (≤ 1 channel). Per-frame output and `klon3d-view` stay open |
| Comparison map | **done for the measure and the display** (`vergleich`, `vergleichskarte`, #21): distance of every vertex to the **surface** of the other mesh, both directions, per-group table, `--csv` per vertex, and `--farben <dir>` writes `a_zu_b.obj` / `b_zu_a.obj` with `v x y z r g b`. The scale starts at **2 mm, not 0**: on the one comparison where the noise floor could be measured (the same project, fabric changed only) the surface distance had a median of 2.14 mm, so anything at or below reads as the cold end "inside the noise, no statement". Coordinates are printed to four decimals and `o` comes out as `g` — names and faces survive, both normalisations are documented in the writer. Reference meshes: 88 718 points, median 2.141 mm forward / 2.173 back, p90 9.489, max 58.050, matching the values in this file; colour matches the scale recomputed from the CSV for 88 718 of 88 718 points, exactly. Drawing it in `klon3d-view` is stage 6, the per-run scatter triple stays a measurement question |

**Colored vertex attributes** — **done for the export** (`farbkarte`, #19): `anziehen --farben <dir>`
writes `dehnung.obj`, `spannung.obj` (#23) and `abstand.obj` with `v x y z r g b`, the same geometry
as `ende.obj`. The scale is fixed per measure (strain 0–20 %, stress 0–35 N/m, distance 0–30 mm) and
printed with the run, so a colour in a picture reads back as a number and does not mean something
different in the next run. A value that is not a number is grey, not red. On Bluse3 v2 the colour of
all 5 538 points matches the scale applied to the numbers recomputed from the CSVs (≤ 1 channel of
rounding). Still open: drawing a scene in `klon3d-view` — it is a 2D pattern browser and a 3D view
belongs to stage 6.

### Comparing two runs (model: drape-delta-demo)

The model is <https://github.com/ZHOUCHENHAO1221/drape-delta-demo> (MIT,
as of 2026-10-18; local clone in `downloads/drape-delta-demo`). There, the same
garment is simulated twice in CLO, once with a
library fabric and once with a measured fabric, and compared point by
point. The method is adopted, not code or data: the
repository contains only the website, no latex fabric and no
raw measurements.

| Point | Decision for klon3d |
|---|---|
| Metric | mean point distance **and median**; the mean alone is skewed by individual outliers (there 24.5 mm mean against 12.0 mm median) |
| Display | distance per point as a color map on the garment, plus the largest single value |
| Different meshes | klon3d and CLO have different meshes: distance to the nearest point of the other surface, in both directions; not by matching point numbers |
| Scatter | CLO varies from run to run by about **10 mm per point** (particle distance 10). Differences below that do not count as a deviation. Compute each side three times and put each side's own scatter next to the difference |
| Garment | a difference only holds for the garment it was measured on; report draping and fitted garments separately |
| Interpretation | a difference says that two runs disagree, not which one is closer to reality |

Important for latex: with elastic fabrics, a difference in
**stretch** persists even on the fitted garment (jersey there: 95 mm draping,
49 mm fitted). Differences in **bending** shrink to
16–36% when fitted. For tight latex pieces, compare the stretch value first. First
use case: the same piece with `Latex_0.4mm_Radical_Rubber` and with the
Retifist preset `NR-sheet-0.40` (stretch 10× softer, see
`MATERIAL.md`).

## Stage 6 — User interface

`wgpu` + `egui`. No Qt — the Rust bindings are thin, Qt 4 is end-of-life.
Operation: load pattern, choose body, choose material, simulate. The
solver runs on the same `wgpu` device (ADR-018); meshes can be drawn without a detour
through the CPU.

**Requirements from the user (2026-10-20):** a **dark design**, **menus** and
**buttons**.

| Part | Content |
|---|---|
| Design | dark by default (egui `Visuals::dark`), switchable to light, the choice is saved; colors of the inspection maps (strain, distance) readable on a dark background |
| Menu bar | **File** (open pattern, measurement file, seam list, body, export OBJ/DXF, quit) · **Simulation** (start, pause, single step, reset, material) · **View** (2D pattern, 3D, mesh, seams, strain and distance map) · **Check** (seam lengths, mesh check) · **Help** |
| Toolbar | buttons for the most frequent commands: open, simulate/pause, reset, 2D/3D, choose map; with keyboard shortcuts |
| Side panels | piece list with seams and deviations (from `klon3d naehte`), properties of the selected piece (material, reduction) |
| Status bar | frames per second, GPU, largest seam deviation, messages |

Today's viewer `klon3d-fenster` (Win32 + GDI, finding 15) remains a
diagnostic tool; the application becomes a crate of its own (`klon3d-app`, `eframe`
with `wgpu`).

## Stage 7 — Motion

New since 2026-10-20 (ADR-018): the garment should also be checked in motion
— how it pulls when the arm is raised, where it slips.

| Part | Content | Status |
|---|---|---|
| Body with skeleton | joints and skin weights for the body from stage 2, deformation via skinning (linear or dual quaternion). Source open: CLO avatar data (`.pos`/`.iks`/`.map`, finding 53; binding via triangles as in `.acc`, finding 69) or our own rigging | open |
| Motion data | read CLO's `.mtn` (joint sets with one 4×4 matrix per frame, finding 69); plus simple poses of our own (raise arm, take a step, twist torso) | format read (finding 69), open |
| Contact with a moving body | body velocity in friction and collision, small substeps or continuous collision detection so that nothing slips through the arm; check the artificial damping of backward Euler (ADR-020); latex fits tightly, this is where usefulness is decided | open; for a body at rest or rigidly moved the penetration check already exists (finding 94), for deforming triangles it does not |
| Self-collision | point–triangle and edge–edge handled separately (like CLO, finding 43), spatial hashing on the GPU | built on the CPU (finding 116: `selbstkollision.rs`, `anziehen --selbstkollision`, off by default); without layers/sub-layers; GPU open |
| Output | frame sequence (OBJ per frame, later glTF), strain and distance map per frame (stage 5) | open |

**Acceptance criterion (proposal, to be agreed with the user):** raise the arm of
test blouse v26 to 90° within one second: no penetration over 1 mm,
the seams hold, the pull at sleeve cap and armpit shows up the way the user
describes it on the real test garment; computation in real time or faster.

---

## Test case: test blouse v26

The simulation is only useful once it shows these five faults:

1. Plunge edges slide to the side, the bust falls out
2. Sleeve cap stands off ~3 cm at the top
3. Tension at the shoulder blade
4. Sleeve too tight along its whole length (reduction 4.4% / 2.2%)
5. Collar stands open at the front

Known beforehand: armhole 41.53 cm against body 41 — does not fit; according to
the *Catsuit* book (pp. 310/312), ~38.7 cm would be right.

**If the simulation cannot do this, it is useless as a check.** That is
how the handover document states it, and it still holds.

What `bluse2_stehend` measures on the shaped body (2026-11-01, stage 4d):

| Fault | Metric | Measured | Floor | Shown |
|---|---|---|---|---|
| 1 Plunge edges slide | opening at the bust, against the 130.0 mm of the flat pattern | 130.0 → 175.3 mm (+45.3) | +10 mm | **yes** |
| 2 Sleeve cap stands off | air under the cap over the shoulder, area-weighted median | 0.7 mm | 10 mm | **no** |
| 3 Tension at the shoulder blade | strain there against the back pieces as a whole | +11.08 % against +5.84 % (+5.24 pp) | 2 pp | **yes** |
| 4 Sleeve too tight | strain around the arm at armpit height | +4.94 % | 2 % | **yes** |
| 5 Collar stands open | air under its free upper edge at the two front ends | 31.2 mm (ends 143.0 mm apart) | 10 mm | **yes** |

Fault 2 is the one that does not appear, and the reason is in the seam list, not
in the material: cap and armhole measure 41.5308 and 41.5310 cm, so sewing them
point to point leaves no excess to lift. On the real blouse the cap stands off
because its *shape* is too full for the armhole, and a seam list that only knows
pairs of edges cannot express that. The test records the negative result so that
a later change makes it visible.

The plunge opening at the neck is not part of the table: the blouse's neckline
measures 37.6 cm while the body's neck opening is 32.1 cm, so 5.5 cm cannot close
there whatever the solver does. The opening widens from 35.4 mm flat to 142.7 mm
worn, and that number says as much about the pattern as about the simulation.

---

## Fabric alongside latex (ADR-021)

The user's request of 2026-10-27: klon3d should also work for real fabric,
with fabric patterns from Seamly2D. What that requires beyond latex,
per stage:

| Stage | needed for fabric | Status |
|---|---|---|
| 1 Pattern reading | grainline per piece: read Seamly2D `<grainline>` (position, rotation), otherwise a fixed piece axis | open; the reader skips `<grainline>` |
| 3 Mesh | carry the grainline into the mesh (direction per triangle); sewn seam with the seam allowance turned inward instead of an overlap seam; glue zone = fusible interfacing | open |
| 4 Solver | **anisotropic material**: warp, weft and shear separate; for this a **triangle continuum** instead of edge springs (those have ν = 1/3 and no independent shear); noticeable, direction-dependent bending; **self-collision** and layers, because fabric falls into folds | open; edge springs isotropic; self-collision since finding 116 (switch, CPU), layers missing |
| Material | `.zfab` reader in Rust, values from CLO's fabric library (ADR-015), no measurements of our own | open; only the Python reader |
| 5 Inspection output | drape and folds; comparison against CLO with the same fabric, draping and fitted reported separately (already specified) | open |

**Acceptance case for fabric:** a fabric pattern from Seamly2D chosen by the user,
with a CLO fabric from the library, compared standing against CLO
as described in stage 5. The criterion is to be agreed with her.

---

## Reading CLO projects (ADR-022)

The user's request of 2026-10-27: CLO projects should be compatible.
Test material: 47 `.zprj` in `downloads/clo3d-patterns` (third-party files, used only
as data), plus our own test projects from CLO in which the same pattern
also exists as `.sm2d`. Format knowledge comes from the findings, `spez/`
and `work/` (ADR-024).

| Step | Content | Status |
|---|---|---|
| Container | header and ZIP block (findings 10, 50, 51), property tree (finding 57) read in Rust | **done** — `zip.rs` (own reader, preamble of 436,425 B in `.zprj`) and `property.rs`; 2034/2034 arrays with `u32 n + u8 type` (finding 117) |
| Pieces | `mapShape2D` of the blocks (finding 62) → seam line, internal lines, grainline, unit; from this the mesh, as for `.sm2d` | **read** — 31 pieces, 194 boundary lines, 107 internal shapes with 131 lines; unit mm confirmed; the mesh is still built by klon3d itself |
| Seams | `listSeamLinePairGroup` → seam list (edge, direction, sections) instead of `.naehte` | **done** — 143 pairs / 286 sides, `uiDepth` 1 = boundary (240), 2 = internal line (46), `arrMeshPointIndex` local; 0 broken sides |
| Placement | `mapArrange` and `.pos` → placement data instead of `.lage`, on klon3d's hull bodies (new zones for legs) | **`mapArrange` read** (finding 118): `qsPanName`/`fNormalizedHeight`/`fNormalizedArcLength`/`fAngle` per piece, but a zone only for 9 of 31 pieces; `.pos` carries **only the avatar pose**, not the piece placement. Mapping onto the hull bodies open |
| 3D mesh | read `baPosition3D` (CLO's simulated mesh) and `baRest` (rest shape), output as OBJ — comparison reference for stage 5 | **done** — 12 B/vertex (3 × f32, mm), 88,718 vertices / 169,181 triangles, 0 deviation from the 2D data; `--obj` writes 31 objects, 47/47 third-party projects; seams meet only with `bDraped=1` + `bActive` (mean 0.171 mm) (finding 118) |
| Comparison baseline | How much does CLO itself scatter? The same pattern with a different fabric as a calibration value | **done** - `backup` state bit-identical to the download, identical topology. Two measures: displacement by index **median 6.10 mm**, 90% 20.55, 99% 41.46, max 68.92; distance to the **surface** (the measure for klon3d) **median 2.14 mm**, 90% 9.49, max 58.05. The difference is sliding along the surface. `klon3d vergleich <a.obj> <b.obj>` computes it in Rust (finding 118, section 11) |
| Fabric | fabric per piece from the project, values as in `.zfab` (ADR-015, ADR-021) | **done** — `mapFabricShape2D.iFabricIndex` per piece, `mapPhysical` with thickness/density/profile; 6/18/2/5 = 31 |
| Avatar | avatar or measurements of the project, otherwise the user's `.smis` | **body done** (finding 119): `body` from the `.avt` (ZIP inside the CLO header, containing a `.dan` without header, `listSkinController`), **31,422 vertices / 62,658 triangles**, mm in avatar space (y = height, the same space as the garment); `--koerper-obj` writes it, **40 of 47** third-party projects have an `.avt`. **Segment and landmark mapping done** (finding 120): `anordnung_clo.rs` applies CLO's `range_list.rlist` + `IndexSet.txt` — confirmed via edges 98.56%, neighborhoods 92.2%, displacement 0.70 mm against 629.86 mm for a random permutation; the 2020 L52 is wrong (8.91 mm), the check rejects it. Measurements from the `.avt` (resizing) still open |

Acceptance of the reader: `klon3d zprj <project.zprj> [--teile] [--naehte] [--stoffe] [--netz] [--obj <file>]`
against `work/messung/japanese_school_uniform` (0 broken seam sides, exit code 0) and
against the **47 third-party `.zprj`** in `downloads/clo3d-patterns`: 47/47 read,
2,109 pieces, 4,256 seam pairs, 0 errors (finding 117, sections 7 and 8).

Acceptance of the 3D mesh (finding 118): 88,718 vertices / 169,181 triangles, 0 deviation from the 2D data;
`--obj` writes 31 objects with 0 degenerate triangles, 47/47 third-party projects with a mesh (0 errors).
Reference for the comparison: `work/messung/japanese_school_uniform/clo3d_netz.obj`.
Touchstone for every seam measurement: only `bDraped=1` (group) **and** `bActive` (pair) is the sewn
state — mean 0.171 mm against 21.2 / 44.2 mm otherwise.

Acceptance of the body (finding 119): `klon3d zprj <project.zprj> --koerper-obj <file.obj>` writes `body`
with 31,422 vertices / 62,658 triangles (0 triangle indices out of range), 40/47 third-party projects with a body, 7 without
an `.avt` (not a read error). Cross-check against an independent Python parser: largest deviation 0.00005 mm
(OBJ rounding only).

**The body is usable without re-posing** (finding 119, 4.1/4.2): it stands in an A-pose with the arms **22° from the
vertical** (not horizontal) — and that **is** the pose CLO computed in, because the sleeve sits on
the arm (98–99% of the nearest body points lie on the arm themselves). Nothing is inside the body (0/1,117 left sleeve,
0/1,198 right, 0/65,339 skirt, 0/16,082 torso). Garment→skin per group: sleeves 14.0/14.5, back torso 7.7, front 24.7, skirt
24.5 mm; nowhere below 4.4 mm. The `.pos` is only needed for a **different** pose (fitting).

Acceptance of the distance map (stage 5, `crates/klon3d-sim/src/abstand.rs`, #15): `klon3d anziehen
<pattern.sm2d> --abstand <dir>` prints a per-piece table (points, min, median, p90, max, points inside) and writes
`abstand.csv` (point, piece, position in cm, signed distance in mm, skin normal). The same measure reproduces the
unsigned comparison exactly on the reference garment: median 20.4 mm, p90 60.2, p99 90.7, max 106.0 mm for 88,718
points in 0.6 s, min 4.3 mm against finding 119's 4.4. Sign is checked against ray parity on the real body across the
clothing band (z 0.5–1.45 m, 160 samples, 0 deviations); outside that band the pseudo-normal is unreliable by known
cases at nose, lip and toes — the test enforces that those deviations exist rather than hiding them.
Bluse3 v2: 0 of 5,538 points inside the skin, min +0.2 mm, median 2.4 mm, max 36.6 mm.

**What that cross-check does not cover** (finding 118, section 11): it measures CLO's garment mesh against CLO's body,
not a klon3d run against it — for the reference project there is only the `.zprj` and no `.sm2d` + `.naehte` + `.lage`,
and the axes need rotating (CLO y = height, klon3d z = height). The **body** was never the blocker
(finding 119), and **neither is its segment and landmark mapping** (finding 120):
`klon3d anziehen <pattern.sm2d> --zprj <project.zprj>` takes `body` from the `.avt`, plus segments from
`range_list.rlist` and landmarks from `IndexSet.txt`. CLO's numbering applies unchanged — confirmed via edges
(98.56%), neighborhoods (92.2%) and the smoothness of the displacement (0.70 mm against 629.86 mm for a random permutation).
First run with bustier v1: 298 of 299 seam pairs within 10 mm. **The comparison baseline, however, already exists:**
the `backup` state is
bit-identical to the download (same topology, `baRest`/`baTri` identical in 31/31 pieces). CLO's own
result shifts from the fabric change alone

- **by index** (same vertex, only possible with identical topology) by **median 6.10 mm**
  (90% 20.55 / 99% 41.46 / max 68.92),
- in **distance to the surface** - the measure that applies to klon3d against CLO - by **median 2.14 mm**
  (90% 9.49 / max 58.05).

The difference is tangential sliding; below **2 mm surface distance** a difference cannot be told apart from
CLO's own variation. Tool: `klon3d vergleich <a.obj> <b.obj> [--zelle mm] [--csv file]`.

Writing back as `.zprj` has not been decided; the route to CLO remains
`klon3d dxf`.

---

## Order and dependencies

```
Stage 1  Patterns ──┬─→ Stage 3  Mesh ──┐
                    │                   ├─→ Stage 4  Solver ─→ Stage 5  Inspection output
Stage 2  Body ──────┴───────────────────┘   4a Tube (CPU)
                                            4b GPU
                                            4c Placement
                                            4d Blouse standing ─→ Stage 7  Motion
```

Stages 1 and 2 are independent and can be done in parallel. Stage 4 depends on both.
Stage 7 needs 4b (GPU) and 4d as well as a body with a skeleton.

---

## Honest assessment

This is a **multi-month project**, not a one-week project. Realistically:

| Stage | Size |
|---|---|
| 1 Pattern reading | the most laborious single stage, but a finite set |
| 2 Body | small, a template exists |
| 3 Mesh | medium, standard methods |
| 4 Solver | second most laborious, the core; with the GPU version and placement larger than originally thought |
| 5 Output | small |
| 6 User interface | medium |
| 7 Motion | large: skeleton and skin weights, contact with a moving body, self-collision |

The work proceeds stage by stage and shows confirmed results after each stage, instead of
delivering something untested at the end. The solver is first checked against the
simple "tube" case, which the existing stage 2 has already solved
— this shows whether the physics holds before a sleeve depends on it.

Still open is the calibration of the latex material: the numbers in the
handover are marked as an assumption, and there are no calibrated values for
latex with reduction on the skin. That is the biggest uncertainty in the
entire simulation.
