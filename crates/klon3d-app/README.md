# klon3d-app – application window (stage 6)

Task: [#34](https://github.com/viktoschi/klon3d/issues/34) · claimed by `claude-app-1101` ·
branch `claude/vigilant-babbage-idsu9r` · base `ec76e25`

This file is the **handoff**: it says what is decided, what is done and which steps remain, so that any
session – Claude, another agent or a human – can continue without the chat history. Tick a step when it is
done and committed; update "Status" at the top when you stop.

## Status

**2026-11-02 - stage 6c, items 1-3 of [#40](https://github.com/viktoschi/klon3d/issues/40)**
(branch `40-grainline`, base `bfe4a58`): grainlines and notches in the 2D view, 2D-Schnitt >
"Nähte vorschlagen…" with a save mode of the file dialog. Item 4 (Netzprüfung) is not part of it
and stays greyed out (it moves after #38). `cargo test --locked -p klon3d-app` on Windows: 95
tests (48 unit + 47 UI; 9 new: 4 unit, 5 in `tests/fadenlauf_naehte.rs`), no compiler warnings;
the crate's files stay rustfmt-clean. Checked by eye on PNGs of `examples/bildschirmfoto.rs` (not
in the repository): the Bluse3 v2 shows its notches; it has no grainline element, so the
grainlines were seen on a copy with one added to every piece (arrows 0/1/2), as the test does.

* **Grainline** (`muster::Teil::fadenlauf`, `ansicht2d::pfeilspitzen`): `Auswertung::fadenlauf`
  per piece, construction coordinates like `umriss`; drawn as a line with Seamly2D's arrow heads
  (`arrows` 0 both ends, 1 at `p2`, 2 at `p1`, 20 degrees, 9 px). `Ok(None)` draws nothing; `Err`
  draws nothing and is kept in `Teil::fadenlauf_fehler`, shown in the hover text of the piece in
  the 2D view (see open points).
* **Notches** (`ansicht2d::passzeichen`): every point node with `notch="true"` as a 10 px tick
  across the outline (normal of the mean direction of the neighbouring segments).
* **Toggles** "Fadenlauf" and "Passzeichen" in Anzeige > 2D-Schnitt and in the 2D toolbar, stored
  in `Anzeige2d` (`fadenlauf=1;passzeichen=1`, a missing key means on); disabled without a pattern
  (hover "erst ein Muster öffnen").
* **Seam suggestion** (`muster::naehte_vorschlagen`, `App::naehte_vorschlagen`): reads the pattern
  again, `schlage_naehte_vor(Kurvenquelle::Seamly)`, `nahtliste_text` with the head line of
  `klon3d naehte --vorschlag`; the file dialog opens in save mode (`Zweck::NahtlisteSpeichern`:
  file name field with `<stem>.naehte`, button "Datei speichern", an existing file only after
  "Überschreiben"); then the pattern is loaded with that list and the Nähte tab shown.

The properties of a piece have a row "Fadenlauf": "ja", "keiner" or the error in the error colour
(also in the hover text of the 2D view). Test `eigenschaften_nennen_fadenlauf_und_seinen_fehler`
breaks one grainline of a copy (`arrows="7"`) and checks all three states. Item 4 (mesh check) follows
after #38 through `klon3d-pipeline`; "Netzprüfung" stays greyed out until then.

**2026-11-02 – steps 8–10 done, pull request open (step 11):** `origin/main` (with #35 from task #33)
is merged into the branch – a merge, not a rebase, so the branch never needs a force push; the PR is
squash-merged anyway. On the merged state: `cargo +1.99 build --locked --workspace --exclude
klon3d-view --all-targets` passes (one warning, in `klon3d-sim/tests/gpu.rs` from #33, not in this
crate); `cargo +1.99 test --locked --workspace --exclude klon3d-view --no-fail-fast`: 54 test
targets, 379 passed, 0 failed, 37 ignored. Screenshots of the real window are in `bilder/` (see
below). Open: the CI run of the PR (Linux and the Windows job that now builds and tests this
crate) and a first start on the main machine.

![klon3d-app, dark: Bluse3 v2, piece selected, edge lengths on](bilder/fenster_dunkel.png)

![klon3d-app, light: Nähte tab of the object browser](bilder/fenster_hell.png)

Both taken under Xvfb with software Vulkan (lavapipe), window 1400×900.

**2026-11-02 – review of step 7b done:** reviewers found 11 defects in the new arrangement (two
of them the same: the Δ column); all are fixed in one commit, each observable one with a test that
fails on the code before (checked by running the new tests against the old files):
**41 unit tests + 35 UI tests** (18 in `tests/oberflaeche.rs`, 17 in `tests/fensteranordnung.rs`),
no compiler warnings, Windows cross check clean, the crate's files rustfmt-clean. List under
"Review findings (step 7b)" below. Fresh screenshots of the real window (Xvfb 1600×1000, default
window 1400×900, checked by eye, not in the repository; `/tmp/claude-0/shots2/final_*.png` in the
cloud container, gone with it): dark with the hover name of the piece under the pointer; light
with the Nähte tab – the whole seam grid, Δ second, inside the object browser; Längen on with
"Rueckenteil Mitte Links" selected – its name above the piece, lengths around it; Längen of all
pieces with the 3D window hidden – no length over a name; Hilfe › Tastenkürzel centred, menu bar
and file toolbar free. Step 7b stays ticked; **what remains are steps 8–11** (below).

**2026-11-02 – step 7b done:** the window is arranged the way CLO users know it, in three commits
(each builds, keeps `cargo +1.99 test --locked -p klon3d-app` green and passes the Windows cross
check): **39 unit tests + 29 UI tests** (18 in `tests/oberflaeche.rs`, 11 in
`tests/fensteranordnung.rs`), no compiler warnings, the crate's files rustfmt-clean (as before; no
`cargo fmt` over the workspace).

* **Menus and shortcuts:** nine menus in CLO's order with our own German labels – Datei,
  Bearbeiten, 3D-Kleidung, 2D-Schnitt, Materialien, Avatar, Anzeige, Einstellungen, Hilfe; the old
  "Simulation", "Ansicht" and "Prüfen" menus are gone (full list below). Submenus, checkboxes for
  toggles (shortcut on the right), radio buttons for design and language, every later entry
  disabled with the hover text `SPAETER`. Datei › Zuletzt geöffnet (last 10 patterns, stored,
  missing files disabled, "Liste leeren"), Datei › Informationen… (`dialoge.rs`). Shortcuts
  Umschalt+S (Nähte), Umschalt+Z (Längen), Strg+Tab (active window) under the old rule (letters and
  Tab not in a text field or with the file dialog open).
* **Layout** (`fenster.rs`): menu bar, file toolbar (Neu, Öffnen, Speichern, Rückgängig,
  Wiederholen, Hell/Dunkel), status bar; library left (`bibliothek.rs`); object browser (tabs
  Teile | Nähte | Stoffe) above the properties right; in the middle the 3D window (placeholder
  canvas "3D-Ansicht") left of the 2D window with a draggable divider. Each window has its own
  toolbar (wrapped when narrow) and a frame, the active one highlighted, a press inside activates
  it. Both splits (3D | 2D, browser | properties) are stored *shares*, not pixel sizes – a nested
  egui panel had stored the size of the first, tiny frame. Side panels are capped at 17 % / 27 %
  of the width; with both windows shown the 2D window keeps ≥ 304 (300 inside its frame) and the
  3D window ≥ 120 while there is room (`fenster::teilung_breiten`), so at 800×500 the 2D view
  keeps its 300 px. The middle is filled with the panel colour (unfilled, the light theme showed
  black toolbar rows – seen on the screenshot).
* **Library:** `dateiwahl.rs` has the shared core `Verzeichnis` (listing, drive buttons, rows with
  single click = mark / open folder, double click = act – on the file the first click marked
  only; a double click on a folder opens just that folder); the modal `Dateiwahl` and the docked
  `Bibliothek` both use it. Places Muster / Stoffe / Zuletzt (drives on Windows), path, Hoch; `.sm2d`
  opens, `.smis`/`.naehte` apply to the pattern shown or waiting (same path as a dropped file),
  `.zfab`/`.zprj` greyed. Fabrics folder = `assets/stoffe` searched upwards from the program's and
  the current folder (`anordnung::stoffordner`).
* **Properties of a seam** (row clicked last): Kennung, Bezeichnung, `L_A cm`, `L_B cm`, `Δ cm`,
  both sides with their pieces (short names), points and edges, notch pairs with Δ, notches
  without partner. **2D toggles** Nähte, Längen (edge length in cm at the edge's middle on a
  backdrop, of the selected piece or of all; labels longer than their edge or covering a piece
  name or another label are left out), Teilnamen (where the name fits into the piece; the
  selected piece's name always – above the piece on a backdrop where it does not fit – and the
  piece under the pointer as hover text), Maßstab.
* **Persistence** (`anordnung.rs`): arrangement (visibility, both shares), 2D toggles and recent
  files as forgiving `name=wert` texts in eframe's storage; missing or damaged keys = defaults.
  Whether a recent file still exists is checked on a background thread when the Datei menu opens
  (`anordnung::ZuletztPruefung`); the menu never touches the disk.
  Anzeige › Anordnung zurücksetzen also drops egui's stored panel sizes.
* **Screenshots** (checked by eye, not in the repository): dark, light, and Längen with a piece
  selected, under Xvfb at 1600×1000 with the default window 1400×900 – nothing clipped or
  overlapping. They were made in `/tmp/claude-0/shots2/` of the cloud container and are gone with
  it; the procedure is under "Environment" below.

**2026-11-01:** steps 1–7 done. Step 7 (review of the whole crate) found 25 defects; all are fixed
in five commits (groups A–E below, each builds and keeps `cargo +1.99 test --locked -p klon3d-app`
green): 30 unit tests + 18 UI tests, no compiler warnings, Windows cross check
(`cargo +1.99 check --locked -p klon3d-app --all-targets --target x86_64-pc-windows-msvc`) clean.

**Next – steps 8–11:**

* 8: the Windows CI job already builds and tests the crate (finding 16, `.github/workflows/ci.yml`);
  what remains is to see it pass on the first CI run of the PR – the Windows-only test code
  (`attrib +h`, `SystemDrive`, drive letters) has never run. If it fails there, fix it before
  anything else. Repeat the cross check above if `src/` changed.
* 9: screenshots dark/light with the blouse (environment below; `xdotool` clicks need separate
  `mousedown`/`mouseup` with pauses, keys do not reach the window without a window manager).
* 10: full workspace build and test (`--exclude klon3d-view`).
* 11: rebase, push, PR with the template, `Closes #34`, label `review`.

Steps 1–7 (2026-11-01): the window runs: `cargo +1.99 run --locked -p klon3d-app --
crates/klon3d-pattern/tests/muster/bluse3_v2.sm2d` opens it with the blouse (checked under Xvfb with
lavapipe: 28 pieces, 24 seams, status bar `GPU: llvmpipe (…) (Vulkan)` and `größte Abweichung N07L
Δ -0.0737 cm` as `klon3d naehte` prints). Files:

* `src/muster.rs` – pattern as plain data (outlines, fill as non-overlapping triangles by the
  even-odd rule in horizontal bands, layout, boundary edges, circumference `None` when the boundary
  cannot be resolved, seam report, errors as `Ladefehler` – `Massdatei` for errors another measurement file can
  cure, `Sonst` otherwise –, reader panics caught).
* `src/ansicht2d.rs` – view math as plain functions (`Sicht`, `treffer`, scale bar) and the painting
  (fills, then the selected piece as a 6 px band, then the seam strokes, then labels); refits on
  size changes until the user pans or zooms.
* `src/dateiwahl.rs` – in-app file dialog (confirm button "Datei öffnen"/"Datei wählen", not
  "Öffnen", which is the toolbar button). Nothing touches the disk per frame: drives read once per
  dialog (Windows: `GetLogicalDrives`, no drive is opened), only visible rows laid out
  (`show_rows`); on Windows hidden/system entries are left out like dot files.
* `src/app.rs` – `App` (`eframe::App`): every command is an `Aktion` run at the end of the frame
  (`App::ausfuehren`); shortcuts (Strg+O, Strg+Q, F, L, Tab/Umschalt+Tab, Esc – letters and Tab not
  while a text field has focus or a dialog is open); loading (`App::laden` makes paths absolute; a
  pattern that fails for its measurement file waits in `App::wartend`, and "Maßdatei wählen…" – menu,
  error-window button or a dropped `.smis` – applies to it); drag and drop (one drop = one request,
  `App::dateien_ablegen`); error and help windows; theme and last folder saved under
  `klon3d.thema` / `klon3d.ordner`.
* `src/menue.rs` – menu bar and toolbar; later entries disabled with hover "folgt in einer späteren
  Aufgabe"; entries that need a pattern disabled without one.
* `src/panele.rs` – search field, measurement file in use ("Maße: …"), piece list, seam list,
  properties panel, status bar (message cut with "…", full text on hover, a click shows an error
  again). Side panels are capped at a share of the window and scroll in both directions (step 7:
  36 % / 24 %; since step 7b `fenster::SEITE_ANTEIL`, 17 % / 27 %).
* `src/main.rs` – `klon3d-app [muster.sm2d]`, wgpu renderer, title "klon3d", min size 800×500;
  a Windows release build is a GUI program (no console window), its messages also go to a message
  box.
* `tests/oberflaeche.rs` – pieces and seams against an independent `klon3d-pattern` evaluation
  (Δ formatted like the CLI), list click and view click select a piece and fill the properties,
  theme toggle + save/restore through an in-memory `eframe::Storage`, all five menus with their
  disabled entries, shortcuts incl. "no letters while the search field has focus", file dialog,
  drag and drop (also pattern + measurement file in one drop, both orders), a pattern without its
  measurement file loads after choosing one (menu and error window), relative paths stored
  absolute, "–" instead of a circumference where `klon3d naehte` prints none (measurement file with
  half the values), at 800×500 the 2D view keeps ≥ 300 px, a long status message stays inside the
  window and reopens the error window, missing files give messages, every disabled menu entry
  shows the hover text "folgt in einer späteren Aufgabe", all 10 `.sm2d` below
  `klon3d-pattern/tests/muster` load and their largest seam deviation equals an independent
  `pruefe_naehte` with the measurement file the pattern names ("keine Nahtliste" for the four
  without a list).

`Cargo.lock` change for the kittest `eframe` feature: one dependency edge (`egui_kittest` → `eframe`).

### Review findings (step 7)

Severity in brackets. Tick when fixed and committed; "won't fix" needs a reason here.

**Group A – loading** (`app.rs`, `muster.rs`, `menue.rs`, `panele.rs`, `tests/oberflaeche.rs`) – done:

- [x] 1, 6 [medium/high] a pattern whose measurement file is missing or not named could not get one:
      the failed request is kept (`App::wartend`), menu entry enabled for it (hover "für <name>"),
      error window offers "Maßdatei wählen…", typed error `Ladefehler`; a plain text file is
      reported as "kein Seamly2D-Muster mit Teilen", not as a missing measurement file
- [x] 2, 7 [low/medium] dropped files handled one by one: one drop is one request
- [x] 5, 25 [low] measurement file in use never shown: status "… geladen mit <smis>: …", left panel
      "Maße: <smis> · vom Muster genannt / von Hand gewählt" (hover: full path)
- [x] 11 [low] last folder stored relative: `Ladeauftrag::absolut` in `App::laden`
- [x] 12 [low] Prüfen › Nahtlängen and Ansicht › Einpassen enabled without a pattern; the seam-list
      scroll flag is also cleared on load

**Group B – drawing and panels** (`muster.rs`, `ansicht2d.rs`, `panele.rs`, `app.rs`) – done:

- [x] 3 [low] fill triangulation overlapped for self-overlapping outlines (bolero sleeves): the fill
      is built by the even-odd rule in horizontal bands (breaks at vertex heights and edge
      crossings), `muster::fuellung`; unit test over every piece of every test pattern on a raster
      against `ansicht2d::innen` (the old ear clipping fails it on the bolero sleeves)
- [x] 4 [low] circumference of a piece whose boundary cannot be resolved was shown unmarked:
      `umfang_cm: Option<f64>`, "–" in the error colour with the reason on hover
- [x] 8 [medium] at 800×500 the 2D view shrank to ~20 px: panels capped (`SEITE_ANTEIL`), lists
      scroll in both directions; left panel default 460 so the seam grid fits at 1400 px (step 7b
      moved the seam grid into the object browser at the right; there Δ follows the Kennung and
      the grid fits at 1400 px – review 7b, no. 2)
- [x] 9 [medium] dark mode: selection outline hidden under seam strokes: 6 px band after the
      fills and before the seams, lighter selected fill (unit test: contrast ≥ 1.8 against the
      normal fill; the band itself checked by eye under Xvfb, no test)
- [x] 15 [low] long status messages cut off: truncated with "…", full text on hover, a click
      reopens the error window

**Group C – file dialog** (`dateiwahl.rs`) – done:

- [x] 10, 17 [medium] drive letters probed every frame on Windows: read once per dialog
      (`Dateiwahl::laufwerke`) from `GetLogicalDrives` (raw `extern "system"` like `klon3d-view`,
      no new dependency); unit test of the bit mask everywhere, of the system drive on Windows
- [x] 13 [low] long folders laid out completely every frame: `show_rows` (unit test: 3000 files,
      fewer than 100 rows laid out, counted through AccessKit); "Filter: aus (alle Dateien)" and
      "keine Einträge" with "alle Dateien"
- [x] 14 [low] Windows hidden/system entries listed: skipped by file attribute (unit test with
      `attrib +h`, runs on Windows only)

Windows-only code (`cfg(windows)`) checked with `cargo +1.99 check --locked -p klon3d-app
--all-targets --target x86_64-pc-windows-msvc` (compiles, no warnings); not run on Windows yet –
the Windows CI job (finding 16) is the first place that runs it.

**Group D – Windows build, CI, manifest** (`main.rs`, `Cargo.toml` of the crate, `ci.yml`) – done:

- [x] 16 [medium] Windows CI job did not build or test klon3d-app: the `viewer` job (name kept, in
      case a branch rule requires it) now also runs `cargo build --locked -p klon3d-app
      --all-targets` and `cargo test --locked -p klon3d-app --no-fail-fast`; timeout 30 → 45 min.
      **Not yet seen running on CI** – the first CI run of the PR shows whether the tests pass on
      Windows (Windows-only test code: `attrib +h`, `SystemDrive`, `D:` → `D:\`)
- [x] 18 [low] release build on Windows opened a console window:
      `windows_subsystem = "windows"` in release builds; usage and "Fenster lässt sich nicht
      öffnen" also in a `MessageBoxW` there (cross-checked with `--release --target
      x86_64-pc-windows-msvc`, not run on Windows)
- [x] 19 [low] `rust-version` raised from 1.95 to 1.99 (checked: the default 1.97 now stops with
      "klon3d-app requires rustc 1.99"); the workspace's own `rust-version = "1.85"` is outside this
      task's scope and still too low

**Group E – test honesty** (`tests/oberflaeche.rs`, `muster.rs` tests) – done:

- [x] 20 [medium] `jedes_pruefmuster_laedt_oder_meldet` accepted any outcome: now
      `jedes_pruefmuster_laedt_wie_klon3d_naehte` asserts that every pattern loads and that the
      status bar's largest deviation equals `pruefe_naehte` + `groesste_abweichung` computed
      independently (the calls of `klon3d naehte`), or "keine Nahtliste"; no `eprintln!` left
- [x] 21 [low] precondition "Randkanten (22)" could never fail: now "nothing selected", the
      "Kein Teil gewählt" text and no "Randkanten (<real count>)" before the click
- [x] 22 [low] "clicking disabled Start changes nothing" could never fail: replaced by hovering
      every disabled entry and finding the tooltip "folgt in einer späteren Aufgabe" (checked: the
      test fails without the hover text)
- [x] 23 [low] drop test now asserts the refusal text and that the pattern shown is unchanged
- [x] 24 [low] `etikett_liegt_im_bogen` asserts the label lies in the U (by coordinates and by
      `ansicht2d::innen`); the all-patterns unit test also checks every piece's label

### Review findings (step 7b)

All fixed (2026-11-02); "test" names the test that fails on the code before the fix.

- [x] 1 [high] a double click on a folder row acted on the file that then lay under the pointer
      (library: loaded a pattern or replaced the measurement file / seam list of the pattern
      shown; dialog: chose the file): egui counts the second click of a double click for
      whatever row gets it. `Verzeichnis::zeilen` opens a folder only on the first click of a
      run and acts on a double click only for the file that was marked before the frame. Test
      `doppelklick_auf_einen_ordner_oeffnet_nur_ihn` (library, pattern shown, dialog)
- [x] 2, 8 [medium] Δ column of the seam list cut off at 1400 × 900 (right panel 378 px, grid
      ~420 px): columns now Naht | Δ | L_A | L_B | Bezeichnung, Bezeichnung cut at 20 characters
      (full text on hover and in the properties). Test `nahtliste_passt_in_den_objekt_browser`:
      every cell of every row inside the object browser at 1400 × 900, Δ also at 800 × 500
- [x] 3 [low] piece names painted over edge-length labels: names are placed first and seed the
      collision list of the lengths; a length that would cover a name is left out, a name that
      would cover another name too. Test `teilnamen_und_laengen_ueberdecken_sich_nicht` (2D
      window wide, lengths on: no length rect meets a name rect)
- [x] 4 [low] Teilnamen showed nothing in the default arrangement (no blouse name fits into
      its piece there): the selected piece's name is always shown (above the piece on a
      backdrop where it does not fit, kept inside the view), the piece under the pointer names
      itself as hover text, the button's hover text says so. Test
      `teilnamen_in_der_vorgabe_anordnung` (1400 × 900)
- [x] 5 [low] help windows (Tastenkürzel, Über klon3d) opened at the top left over the menu bar
      and the file toolbar: anchored in the centre like the other dialogs. Test
      `hilfefenster_lassen_menue_und_werkzeugleiste_frei`
- [x] 6 [low] hover text "zum helles Design wechseln": now "zum hellen / dunklen Design
      wechseln"; asserted in `design_umschalten_und_behalten`
- [x] 7 [low] Datei › Zuletzt geöffnet ran `is_file()` on every entry in every frame (a
      disconnected network drive would freeze the window): `anordnung::ZuletztPruefung` checks
      on a background thread, started once per opening of the Datei menu (at most one at a time,
      results as they come); unknown = enabled, a failed open starts a new check. Unit test
      `zuletzt_im_hintergrund_pruefen`; `zuletzt_geoeffnet_im_menue` asserts one check per
      opening over 30 frames with the pointer moving. Not run on Windows with a real network drive
- [x] 9 [low] a stored hidden 2D window stayed the active one at start: `App::mit_speicher` ends
      with `aktiv_pruefen()`; asserted in `alter_gespeicherter_stand_startet_mit_vorgaben`
- [x] 10 [low] one pattern twice in the recent files when reached through `..` (Unix keeps `..`
      in `std::path::absolute`): `Ladeauftrag::absolut` cleans the paths with
      `dateiwahl::bereinigt` like the library; `zuletzt_vorne`/`zuletzt_aus_text` compare clean
      paths (also lists stored by the version before). Test `zuletzt_ohne_doppel_ueber_punktpunkt`
      and unit test `zuletzt_neueste_vorne_ohne_doppel`
- [x] 11 [low] the old-state test wrote the storage through the current key constants and never
      checked the stored folder: it writes the literal keys of the previous version
      (`klon3d.thema`, `klon3d.ordner`) and asserts the theme, `letzter_ordner` and the library's
      start folder (a renamed key or a dropped folder read now fails it)

Notes: egui's default fonts have no arrows (→ ← ↑ ↓), check marks or emoji; use `›`, `–`, `Δ`, `≤`,
`·`, `×`, `…` only (kittest panics on a missing glyph). `egui::Id::new` is deprecated in 0.37
(`Id::unique`); panels are `egui::Panel::{top,bottom,left,right}(..).show(ui, ..)`. kittest's
harness calls `ctx.set_theme(Dark)` *after* the app is built, so the app applies its stored theme in
`ui()`, not only in `App::neu`. Dropping an `egui::FullOutput` with texture deltas panics in debug
builds (`out.textures_delta.clear()`). kittest finds `Label`s by their AccessKit *value*; grid rows are
siblings under one parent (see `zeile_nach` in the UI test). A click on a disabled menu entry closes
the menu (egui), nothing else.

Notes from step 7b: kittest feeds **one event per step** and a step is 0.25 s – the two clicks
of a double click then lie 0.5 s apart, too slow for egui (0.3 s); the library test builds its
window `with_step_dt(1/30)` (`schnelles_fenster`). A click shortly before a double click turns it
into a triple click (`double_clicked()` is false). egui's `double_clicked()` is true for whichever
widget receives the second click – also when the first one hit another widget (review 7b no. 1).
`Harness::run` stops at a delayed repaint (`request_repaint_after`); to wait for a background
thread, step and sleep in a loop (`pruefung_abwarten`). Texts painted into the 2D view are found in
`h.output().shapes`; the piece list and the properties paint the same names, so filter by the
view's rect (`gemalte_texte`). Menu-bar buttons, toolbar buttons and submenus
share names ("Avatar", "2D-Schnitt", "Zurücksetzen"): the helpers `in_der_leiste` and `im_menue`
pick by "inside a menu" (AccessKit role menu among the ancestors – the menu bar is one too) and by
position. Areas are found as AccessKit panes by name (`fenster::flaeche`), tabs by role tab. A
nested `egui::Panel` stores its size from the first frame, which can be tiny – use shares.

## Goal (from #34)

First slice of stage 6 in PLAN.md: an `eframe` window with the `wgpu` renderer and the layout the user asked
for on 2026-10-20 – **dark design** (switchable to light, saved), **menus**, **buttons** – working on the
2D pattern. 3D view, simulation, material, OBJ/DXF export and the maps are later tasks; their menu entries
exist but are disabled.

## Decisions

| Point | Decision | Why |
|---|---|---|
| Framework | `eframe` 0.37 with `wgpu_no_default_features` + our own `wgpu` 30 dependency (same features as `klon3d-sim`) | egui-wgpu 0.37 uses wgpu 30, the same as the solver – later the simulation can share the device (ADR-018); no second wgpu in `Cargo.lock` |
| eframe features | `accesskit`, `default_fonts`, `persistence`, `wgpu_no_default_features`, `x11`, `wayland` | `persistence` saves the theme; `accesskit` lets tests find widgets by label |
| Rust | the crate says `rust-version = "1.99"`; build with **1.99** like the main machine and CI (`stable`) | the workspace's `klon3d-pattern` no longer builds with 1.97 (`geom.rs:779`, elided lifetime in a `const`); eframe alone would need 1.95. With 1.97 cargo now stops with "klon3d-app requires rustc 1.99" |
| File dialog | own small dialog inside egui (directory list, filter `.sm2d`/`.smis`/`.naehte`), plus drag and drop and a command-line argument | no `rfd` – on Linux it pulls GTK or a D-Bus portal stack |
| Tests | `egui_kittest` 0.37 (dev-dependency) drives the UI headless by labels; no GPU needed | screenshots only where a GPU adapter exists, otherwise skipped (AGENTS.md: tests without data skip) |
| Language | UI text German, identifiers German like the rest of the code, **comments English** (AGENTS.md) | |
| Not touched | `PLAN.md` (held by #33), `klon3d-view` (stays the Win32 diagnostic tool) | |
| Window arrangement (2026-11-02) | layout, menu order and shortcuts follow CLO's visible arrangement where klon3d has the function (3D left, 2D right, library left, object browser and properties right); labels are our own German words; entries for later functions exist but are disabled | users who know CLO find things where they expect them; nothing of CLO's program goes into the repository (AGENTS.md) |
| Stored settings | short `name=wert` texts per key in eframe's storage, read forgivingly | an older or damaged state must never stop the start; no serde dependency |

## Planned layout of the crate

| File | Content |
|---|---|
| `src/main.rs` | command line (`klon3d-app [muster.sm2d]`), `NativeOptions` (wgpu renderer, window size, app id), `run_native` |
| `src/lib.rs` | modules, so that tests can build the app |
| `src/app.rs` | `struct App` implementing `eframe::App`: state, `Aktion` queue and `ausfuehren`, loading, shortcuts, `save` |
| `src/anordnung.rs` | what is stored besides theme and folder: `Anordnung` (visibility, shares), `Anzeige2d`, recent files; fabrics folder search |
| `src/fenster.rs` | the layout: panels, 3D and 2D windows with divider, window frames and toolbars, active window |
| `src/menue.rs` | the nine menus, the file toolbar, the toolbars of the 3D and 2D window |
| `src/bibliothek.rs` | the library panel on the shared `Verzeichnis` |
| `src/panele.rs` | object browser (tabs Teile, Nähte, Stoffe), properties of a piece or seam, status bar |
| `src/dialoge.rs` | floating windows: file dialog, error, help (shortcuts, about), Informationen, drop hint |
| `src/muster.rs` | (6c) also each piece's grainline (`Teil::fadenlauf`, `fadenlauf_fehler`) and the seam suggestion `naehte_vorschlagen` (seam list text, numbers of `klon3d naehte --vorschlag`); loading without UI: `.sm2d` → `Auswertung` → piece outlines (`ev.stueck_umriss`), circumference (`ev.umriss_laenge`), boundary edges (`ev.teilrand(stueck, Kurvenquelle::Seamly)`), seam report (`naehte::lies_nahtliste` + `pruefe_naehte`, the same numbers as `klon3d naehte`); measurement file as the pattern names it (see `klon3d-view/src/main.rs`, `Blatt::laden`) or chosen by hand |
| `src/ansicht2d.rs` | 2D view: pieces laid out side by side (as `klon3d-view` `auslegen`), pan, zoom around the cursor, fit, hit test, selection highlight, seams, edge lengths, scale bar; (6c) grainlines with arrow heads (`pfeilspitzen`) and notch ticks (`passzeichen`) |
| `src/dateiwahl.rs` | folder listing `Verzeichnis` (shared with the library) and the modal file dialog; (6c) save mode (`Zweck::NahtlisteSpeichern`: file name field, question before overwriting) |
| `tests/gemeinsam/mod.rs` | helpers of the UI tests: windows, independent Bluse3 evaluation, grid rows, menus, storage |
| `tests/oberflaeche.rs` | kittest: pieces and seams against `pruefe_naehte`, selection, theme, menus and their disabled entries, shortcuts, file dialog, drag and drop, measurement file choice, minimum size, all test patterns |
| `tests/fensteranordnung.rs` | kittest: toolbars, Fenster toggles, Strg+Tab, Umschalt+S/Z, edge lengths, recent files, library, fabrics, Informationen, seam properties, old stored state, reset; (review 7b) double click on a folder, names vs lengths, names in the default arrangement, seam grid inside the object browser, help windows centred, recent files through `..` |
| `tests/fadenlauf_naehte.rs` | kittest (6c): grainlines against `Auswertung::fadenlauf` (Bluse3 v2 and a copy with a grainline on every piece) and drawn one per piece, notch ticks one per notch, both toggles stored (old text = on), disabled without a pattern, "Nähte vorschlagen…" writes a `.naehte` with as many seams as `schlage_naehte_vor`, loads it, asks before overwriting |
| `examples/bildschirmfoto.rs` | renders the window with kittest's wgpu renderer to PNG, no window and no focus taken (`--muster`, `--aus`, `--breite`, `--hoehe`, `--massstab`, `--dunkel`/`--hell`). kittest prefers a software adapter, so the status bar shows e.g. "Microsoft Basic Render Driver" – not the GPU the app uses |

Menu bar (step 7b; `[s]` = disabled, a later task): **Datei** (Neu Strg+N [s], Öffnen… Strg+O,
Zuletzt geöffnet ›, Hinzufügen › Maßdatei… / Nahtliste…, Speichern Strg+S [s], Speichern unter…
Strg+Umschalt+S [s], Import › Seamly2D-Muster (.sm2d)… / CLO-Projekt (.zprj)… [s], Export › OBJ… [s] /
DXF… [s], Informationen…, Beenden Strg+Q) · **Bearbeiten** (Rückgängig Strg+Z [s], Wiederholen
Strg+Y [s], Alles auswählen Strg+A [s], Auswahl aufheben Esc) · **3D-Kleidung** (Simulation
starten/anhalten, Einzelschritt, Zurücksetzen, Anordnen, Netzauflösung…, Netzprüfung – all [s]) ·
**2D-Schnitt** (Auslage / Konstruktionslage L, Einpassen F, Nächstes Teil Tab, Vorheriges Teil
Umschalt+Tab, Nähen › Nahtliste wählen… / Nahtlängen prüfen / Nähte anzeigen Umschalt+S, Nähte
vorschlagen… (6c)) ·
**Materialien** (Stoff zuweisen… [s], Stoffbibliothek) · **Avatar** (Körper laden… Strg+Umschalt+A [s],
Maßdatei wählen…, Avatar anzeigen Umschalt+A [s], Maße am Körper [s]) · **Anzeige** (Blickwinkel ›
[s], 3D-Kleidung › [s], 2D-Schnitt › Nähte / Längen / Teilnamen / Maßstab / Fadenlauf / Passzeichen (6c), Avatar › [s], Fenster ›
five areas + Fenster wechseln Strg+Tab, Werkzeugleisten › three toolbars, Anordnung zurücksetzen) ·
**Einstellungen** (Design › Dunkel / Hell, Sprache › Deutsch / English [s]) · **Hilfe** (Tastenkürzel,
Über klon3d). Status bar: frames per second, GPU adapter and backend, largest seam deviation, last
message.

## Steps

- [x] 0. Issue #34 created, claimed, no overlap with #33 or open PRs; skeleton committed
- [x] 1. Build the skeleton: `cargo build --locked -p klon3d-app --all-targets` (see environment below)
- [x] 2. `muster.rs` with unit tests (Bluse3 v2: pieces, seams, Δ equal to `pruefe_naehte`)
- [x] 3. `ansicht2d.rs` (view transform with tests for fit/zoom/hit test)
- [x] 4. `dateiwahl.rs`
- [x] 5. `app.rs` + `main.rs`: menus, toolbar, panels, status bar, shortcuts, theme with persistence
- [x] 6. `tests/oberflaeche.rs` (kittest) green; `cargo test --locked -p klon3d-app`
- [x] 7. Review of the whole crate (correctness, UI against PLAN.md, Windows portability, test honesty), fixes
      (25 findings, list above)
- [x] 7b. Window arrangement like CLO: (1) menus and shortcuts; (2) layout with library,
      3D | 2D split, object browser and properties, toolbars, persistence; (3) tests
      (`tests/fensteranordnung.rs`) and screenshots checked by eye (2026-11-02)
- [x] 8. Windows: `cargo check --locked -p klon3d-app --target x86_64-pc-windows-msvc` (cross check, no linker
      needed); add `cargo build --locked -p klon3d-app` to the Windows job in `.github/workflows/ci.yml`
      – both done during step 7 (build **and** test in the job); open: the job passing on CI
- [x] 9. Screenshots dark/light with the blouse loaded (environment below): `bilder/`, shown under "Status"
- [x] 10. Full workspace check: `cargo build --locked --workspace --exclude klon3d-view --all-targets` and
      `cargo test --locked --workspace --exclude klon3d-view` (2026-11-02, on the state merged with `main`)
- [x] 11. (2026-11-02: PR #37 open, #34 labelled `review`, done comment posted; what remains is the CI run, the review and
      fixes asked for there – viktoschi merges.)
      `git fetch origin && git merge origin/main` (a rebase would need a force push), push, pull request with the template
      (`.github/pull_request_template.md`: Change / Evidence / Not checked / Limits and provenance),
      `Closes #34`; then on #34: label `review` instead of `in-progress` and a done comment
      (`tools/task.sh done 34 <session> <pr>` does both where `gh` is available)

## Environment

**Cloud container (Linux):**

```sh
rustup toolchain install 1.99 --profile minimal      # the default 1.97 is too old for the workspace
cargo +1.99 build --locked -p klon3d-app --all-targets
# software Vulkan (lavapipe) and EGL, for screenshots and GPU tests without a graphics card
apt-get update && apt-get install -y --no-install-recommends mesa-vulkan-drivers libegl1 libegl-mesa0
# a real window: Xvfb + ImageMagick (both preinstalled in the cloud image used on 2026-11-01);
# winit's X11 backend also needs libxkbcommon-x11 (not preinstalled there)
apt-get install -y --no-install-recommends libxkbcommon-x11-0
xvfb-run -s "-screen 0 1600x1000x24" sh -c 'target/debug/klon3d-app crates/klon3d-pattern/tests/muster/bluse3_v2.sm2d & sleep 8; import -window root bild.png'
```

Screenshots in a given state (step 7b): run with a throwaway `HOME` and write eframe's storage
first, e.g. `printf '{"klon3d.thema":"hell"}' > $HOME/.local/share/klon3d/app.ron` (keys of
`app.rs` / `anordnung.rs`, e.g. `"klon3d.anzeige2d":"naehte=1;laengen=1;teilnamen=1;massstab=1"`).
A click with xdotool: `xdotool mousemove X Y; sleep 1; xdotool mousedown 1; sleep 0.3; xdotool
mouseup 1` (the piece "Rueckenteil Seite Links" sits at 1103,265 in the default layout). Keys do not
reach the window without a window manager.

Disk: `target/` grows to ~17 GB with all test binaries, and throwaway probe crates in the scratch
folders add more; on 2026-11-01 the container's disk was full (263 MB free) and builds failed with
"No space left on device". Check `df -h /` first; deleting old `target/` artifacts of throwaway
crates is safe. A Windows cross check needs about 300 MB in `target/x86_64-pc-windows-msvc`
(`rustup +1.99 target add x86_64-pc-windows-msvc` if missing).

**Main machine (Windows):** cargo path as in AGENTS.md; `cargo run --locked -p klon3d-app -- <muster.sm2d>`.

## Handing over

Stopping before the end: tick the steps above, write the status, commit and push, then leave the handoff
note on #34 in the format of CONTRIBUTING.md and release the task (`tools/task.sh release 34 <session>
"<note>"`, or by hand: a comment starting with `**Released** by \`<session>\``, label `ready` instead of
`in-progress`). The next session claims #34 again and continues on this branch.
