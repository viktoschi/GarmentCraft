# Material model — CLO-compatible

Principle: **CLO's fabric parametrization is adopted, not replaced.**
Same field names, same meaning, same units. For latex the values are set
isotropically (Warp = Weft = Bias).

Rationale: only then can values from `.zfab` files and from CLO's Property Editor
be transferred directly, and only then is a solver result comparable with CLO.

**Status 2026-10-19:** the basis is now a **measured** latex profile
(CLO Fabric Kit, Radical Rubber 0.4 mm) instead of the table of expected values from
2026-10-18. Derivation, file format and comparisons: `../latex-stoffe/` (not part of this repository).

**Status 2026-11-10 (stage 3, #127):** the solver now uses **all three**
stiffnesses — warp (`fSvK`), weft (`fSuK`) and shear (`fHK`) — as independent
inputs, one membrane per triangle. Poisson's ratio is no longer assumed anywhere;
it follows from E and G (see "Shear stiffness"). Warp and weft are carried as
separate fields even on the isotropic latex profile, where they differ by
0.002 %.

## Sources of the values

| Source | File |
|---|---|
| **Measured latex profile** | [`assets/stoffe/Latex_0.4mm_Radical_Rubber.zfab`](assets/stoffe/) |
| Thicknesses and laminates computed from it | `assets/stoffe/Latex_*_abgeleitet.zfab` (*abgeleitet* = derived) |
| Derivation, value tables, tool | `../latex-stoffe/` (not part of this repository) |
| Structure of `.zfab` and of the property tree | `../latex-stoffe/FORMAT.md` (not part of this repository) |
| Table of expected values (estimate from 2026-10-18) | [`assets/LATEX_MATERIALPROFIL.md`](assets/LATEX_MATERIALPROFIL.md) |
| CLO parameter structure | finding 09 |

Origin of the profile: project `2022_racing_miku_v2` from
<https://github.com/tabjy/clo3d-patterns> (CC BY-SA 4.0, tabjy). Across all 47
projects of this collection it is the only real latex profile.

## The CLO fields and their latex values

Values of the measured profile at 0.4 mm. Field names in the file as described in
`../latex-stoffe/FORMAT.md`; units as in CLO's Property Editor.

| CLO field | in the file | Unit | Latex 0.4 mm | scales with | Note |
|---|---|---|---:|---|---|
| Stretch-Weft / -Warp | `fSuK` / `fSvK` | g/s² | 348,274 / 348,282 | t | two separate fields, direction-independent for latex |
| Shear | `fHK` | g/s² | 174,159 | t | = Stretch / 2 here, but read on its own; see below |
| Bending-Weft / -Warp / -Bias | `fBuK` / `fBvK` / `fBhK` | g·mm²/s²/rad | 2,482 | t³ | the same in all directions |
| Buckling Ratio / Stiffness | `fBuLR` …, `fBucklingStiffness…` | 0–1 | 0 / 0 | – | no threads, no buckling |
| Density | `fDensity` | g/m² | 349 | t | corresponds to 0.87 g/cm³ |
| Thickness | `fThickness` | mm | 0.4 | t | also the collision thickness |
| Friction | `fFriction` | – | 0.03 | – | CLO default, **not a measured value**; latex grips more |
| Internal Damping | `fIDS` | – | 0.0001 | – | CLO displays 1 |
| Nonlinear | `listNonlinearSuK` … | factors | all 1.0 | – | CLO computes the profile **linearly** |

Shrinkage is not a property of the fabric but of the pattern piece; the
reduction is built into the pattern.

## Measured tensile values

Raw data of the Fabric Kit (`mapRawData`), identical in all three directions.
The units are not stored in the file, presumably % and kgf:

| Length | 12.33 | 33.33 | 40.67 | 66.67 | 100 |
|---|---:|---:|---:|---:|---:|
| Force | 0.100 | 0.230 | 0.300 | 0.470 | 0.595 |
| Force / length, relative to point 1 | 1.00 | 0.85 | 0.91 | 0.87 | 0.73 |

The material becomes about **27%** softer as it stretches, with some scatter.
The table of expected values from 2026-10-18 had 35% (E 1.08 → 0.70 MPa). CLO turned
this into one fixed value and left the curve flat; for a nonlinear model here,
the five points are the basis.

**Conversion to E (assumption, not verified):** CLO's unit g/s² is force per
width, 1 g/s² = 1 mN/m. Then Stretch = E · t:

```
K = 348,274 g/s² = 348 N/m        E = K / t = 348 N/m / 0.4 mm ≈ 0.87 MPa
```

That lies between the values of the expected-values table (1.08 / 0.84 / 0.70 MPa)
and within the range Retifist gives for soft natural rubber sheeting
(M300 ≈ 0.9–1.2 MPa).

### Shear stiffness, and Poisson's ratio

The measured profile has **Shear = Stretch / 2**. ADR-015 had assumed G = E/3 for the
membrane and derived "Shear ≈ Stretch" from it. How CLO's
Shear value relates to G is not settled; the measured value is what
counts.

Since stage 3 (#127) the solver no longer has to settle that question: `fHK` is an
input of its own. One membrane per triangle takes warp stretch `fSvK`, weft stretch
`fSuK` and shear `fHK` as three **independent** stiffnesses:

```
U/A₀ = ½·E_k·ε_k² + ½·E_s·ε_s² + 2·G·ε_ks²      on ε = ½(FᵀF − I)
ν = E / (2G) − 1                                derived, not set anywhere
```

With `fSvK` and `fHK` of the measured profile that gives

```
ν = 348,282 / (2 · 174,159) − 1 = −0,0001
```

i.e. the value the spring net could never produce. Before stage 3 every triangle was
held by distance springs along its edges, and for **any** set of such springs
`C₁₂₁₂ ≡ C₁₁₂₂`, so `ν = G/E` identically — 0.5 for latex, exactly the number a
rubber-like sheet is *not* supposed to have. That is measured, not a matter of
taste: `crates/klon3d-sim/tests/kontinuum.rs` deforms a strip of this profile
affinely and reads E and G back (`E_eff` 351.8 against 348.3 N/m, `G_eff` 174.159 N/m,
`ν = 1e-12`), and its item 4 invents an anisotropic profile with `fSvK ≠ fSuK` to show
that the two directions really are separate.

`E` and `G` are the two **independent** numbers; a profile whose shear is not exactly
half the stretch simply has a Poisson ratio of its own, positive or negative. Nothing
downstream assumes 0.5, and nothing assumes 0.

The eight `.zfab` files in `assets/stoffe/` are isotropic to 0.002 % (`fSuK` against
`fSvK`), so a real anisotropic reference fabric does not exist in this repository —
the anisotropic case is invented in the test.

### Bending stiffness

Measured: 2,482 g·mm²/s²/rad, raw overhang data 23.5 and 35 (presumably mm).
Cross-check with the plate formula D = E·t³ / (12·(1−ν²)), ν = 0.5, under
the same unit assumption: about 6,200 g·mm²/s²/rad. Measurement and formula are
thus in the same range (factor 2.5). With the ν ≈ 0 that stage 3 derives, the
formula gives 4/3 of that, i.e. about 8,300 — the factor against the measurement
grows to 3.3. Still the same range, still not settled: bending is a separate
stiffness and is **not** predicted from stretch and shear.

## Other thicknesses and laminates

Same compound assumed: stretch, shear and weight ∝ t, bending ∝ t³.
Laminates fully bonded, like a single sheet of the total thickness.

| File in `assets/stoffe/` | t mm | g/m² | Stretch g/s² | Shear g/s² | Bending |
|---|---:|---:|---:|---:|---:|
| `Latex_0.25mm_abgeleitet` | 0.25 | 218 | 217,671 | 108,849 | 606 |
| **`Latex_0.4mm_Radical_Rubber`** (measured) | 0.40 | 349 | 348,274 | 174,159 | 2,482 |
| `Latex_1.0mm_abgeleitet` | 1.00 | 872 | 870,685 | 435,398 | 38,785 |
| `Latex_Laminat_2x0.25mm_abgeleitet` | 0.50 | 436 | 435,343 | 217,699 | 4,848 |
| `Latex_Laminat_0.4+0.25mm_abgeleitet` | 0.65 | 567 | 565,946 | 283,008 | 10,651 |
| `Latex_Laminat_2x0.4mm_abgeleitet` | 0.80 | 698 | 696,548 | 348,318 | 19,858 |
| `Latex_Laminat_3x0.4mm_abgeleitet` | 1.20 | 1,047 | 1,044,822 | 522,477 | 67,021 |
| `Latex_Laminat_2x1.0mm_abgeleitet` | 2.00 | 1,745 | 1,741,371 | 870,795 | 310,280 |

The pattern files use 0.4 mm on the torso and 1.0 mm on cups and collar
(PLAN, stage 4); both are therefore available as files.

## Compliance for XPBD

XPBD does not work with a stiffness `k` but with a **compliance**
`α = 1/k`. With the membrane stiffness K (Stretch in N/m, assumption above), an
edge of length L that represents a strip of width w has:

```
k = K · w / L        →        α = L / (K · w)
```

For 10 mm edge length and 50 mm strip width:

| Thickness | K | k | α |
|---:|---:|---:|---:|
| 0.25 mm | 218 N/m | 1,088 N/m | 9.2·10⁻⁴ m/N |
| 0.40 mm | 348 N/m | 1,741 N/m | 5.7·10⁻⁴ m/N |
| 1.00 mm | 871 N/m | 4,353 N/m | 2.3·10⁻⁴ m/N |

The earlier estimate for 0.4 mm was 1,400–2,160 N/m; the measured
value lies in between.

These are very soft constraints — exactly as intended. Set too soft, however,
the garment slides **to the floor**, as reported in the CLO forum:

> "Why the fabric in the forum slid to the floor: Stretch was set far too
> soft."

**This is a validation criterion, not a detail:** the solver must let an
insufficiently held piece be pulled down. If it does not, the
stiffness is miscalibrated.

The XPBD cross-check path (`CpuReferenz`) carries the same membrane as the implicit
solver since stage 3, so the compliance above is only the edge-spring reading of the
same stiffness; the two routes no longer disagree about ν.

## Reference fabrics

`assets/stoffe/` contains eight fabric files in CLO's `.zfab` format.

All eight are **latex** (format 7 like CLO's library, black, material type
Plastic). Loaded in CLO (earlier version) on 2026-10-19 and checked in the
Property Editor:
the original, 0.25 mm, 1.0 mm, laminates 2×0.25, 0.4+0.25 and
2×0.4. The laminates 3×0.4 and 2×1.0 have not been opened in CLO.

The Retifist presets `NR-sheet-*.zfab` are not included: their values are set, not
measured, and at 0.4 mm they are about 10× softer in stretch and 40× softer in
bending; moreover, Retifist releases the `.zfab` files only for use at one's own
workstation. Comparison in `../latex-stoffe/LIESMICH.md`.

**Purpose:** simulate the same fabrics in CLO and compute them in klon3d's own
solver. If stretch, drape and distance to the skin agree, the
material model is confirmed.

## Open issues, stated plainly

- **Only 0.4 mm is measured.** All other thicknesses and the laminates are
  computed and assume the same compound (Radical Rubber). Measurements of our
  own are not possible; they are judged by plausibility.
- **The unit assumption** (g/s² = force per width) behind E ≈ 0.87 MPa and
  behind the compliance table is not confirmed.
- **No friction value.** 0.03 is CLO's default. For the question under test (does the
  sleeve cap stand off?) it is exactly the decisive parameter — see
  `PARITY.md`, row "Tack spring for stickiness".
- **Curve:** five sample points with scatter, units not stored in the file.
- **No Poisson field.** `.zfab` states `fSvK`, `fSuK` and `fHK` and nothing about
  ν; stage 3 derives it from E and G, which is all the continuum needs. A profile
  whose three numbers do not correspond to any physical sheet is accepted without
  complaint — the solver has no way to tell.
- **No compression rail.** The membrane is linear elastic in both directions:
  a rest length shorter than the material is a tensile pre-strain, never slack.
  Cloth goes slack and buckles, and the `.zfab` even carries a buckling field
  that nothing reads. This is not cosmetic: the ease of #109 works by shortening
  rest lengths so the longer seam side gathers, and with the continuum it pulls
  instead — `bluse2_stehend_einhalte` fails one ordering assertion because of it
  (`PLAN.md`, "What the continuum does to these numbers").
- **Bending is isotropic.** Warp, weft and bias bending (`fBvK`, `fBuK`, `fBhK`)
  are carried in the file but the solver takes one value for all directions;
  on latex they are equal anyway, on woven fabric they are not. Bending is read
  neither from the warp nor from the bias field, and only one bending stiffness
  enters the solver.
- **The files are read and reach the solver.** Two steps, kept apart on
  purpose: `crates/klon3d-pattern/src/zfab.rs` parses a `.zfab` into `Zfab`
  (container, head, property stream; a field the file does not state stays
  `None`), checked against `../latex-stoffe/werte/latex_profile.csv` by
  `crates/klon3d-pattern/tests/zfab.rs`. `crates/klon3d-pipeline/src/stoff.rs`
  then converts that to `klon3d_sim::Material` and *demands* the seven values the
  solver needs, because it cannot run on "not stated": weft stretch, warp
  stretch, shear, weight, thickness, friction, bending. An open point is the
  rest: `Zfab` carries more than `Material` holds (warp/bias bending, buckling,
  damping), and none of it is used yet. The nonlinear curves stay unread
  (§ "The CLO fields and their latex values").
