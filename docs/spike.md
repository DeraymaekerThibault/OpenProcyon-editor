# Phase 0 spike

The throwaway slice that proves the stack (`docs/roadmap.md`, Phase 0). Gate: every item works on Bevy 0.20; if one fights us,
compare against a web stack before Phase 3. Record what was found under each item, including the exact errors.

- [x] **1. The editor crate links `openprocyon` and bevy_egui on Bevy 0.20 with the engine's features.**

  Findings:
  - Setup: `openprocyon` from git tag `v0.2.0`, `bevy` 0.20 with the engine's exact feature list, `bevy_egui` 0.43 (default
    features), `rfd` 0.17. It builds (first clean dev build 7m 21s), `tools/check.sh` passes, and the window opens with the egui panel
    on Linux (Wayland, Vulkan/RADV), both with an import present and with `PROCYON_ASSETS` pointing nowhere.
  - bevy_egui needs **no `bevy` crate feature** the engine leaves off: it depends on the Bevy sub-crates directly. With its
    default features it adds what the engine does not build:
    - the `bevy_picking` crate with `mesh_picking` (bevy_egui's `picking` feature, on by default);
    - `zstd_rust` on `bevy_image` and `debug` on `bevy_utils` (feature unification onto crates the engine also uses).
    Everything else it uses (`bevy_render`, `bevy_core_pipeline`, `bevy_ui_render`, `bevy_winit`, ...) the engine already builds.
  - At startup bevy_egui warns: `The bevy_egui/picking feature is enabled, but PickingPlugin is not added (if you use Bevy's
    DefaultPlugins, make sure the bevy/bevy_picking feature is enabled too)`. FIXED: `bevy_egui` now uses
    `default-features = false` with the defaults minus `picking` (item 3 showed egui needs no Bevy picking);
    `cargo tree -e features -i bevy_picking` finds no such package any more, so the warning cannot fire (not run with a display here).
  - The engine's asset lookup `base_assets()` is `pub(crate)` at v0.2.0, and the public `assets_dir()` rebuilds the shared
    overlay folder as a side effect (#136). The editor composes the public pieces instead (`PROCYON_ASSETS`, then
    `campaign::user_dir()/assets`, status from `setup::imported`). Phase 1's `authoring` API should expose the lookup without
    side effects, plus the engine version (the editor repeats `0.2.0` by hand).

- [x] **2. Load one imported map through the engine's loaders.**

  Findings:
  - Works, on a machine with a full import (engine `main`, 2026-11-24): `map_view::load` reads `maps/red_rover/map.json` of the
    import with `openprocyon::data::try_json` (public at `v0.2.0`, the reader behind `data::map_json`); the plan shows its 20
    `objects` (islands and the two bases) at `pos` x, y in game axes. No dependency change was needed: `v0.2.0` suffices.
  - Gap: the engine has no public, side-effect-free map loader. `data::map_json(id)` goes through `assets_dir()`, which rebuilds the
    shared overlay (#136), and there is no typed map document (`scenario` reads `serde_json::Value` field by field). The `authoring`
    API on branch `claude/exciting-lamport-zxdqra` adds `base_assets()` and `overlay_into()` but no `map(root, id)`; the editor
    composes the path itself (PROVISIONAL). Requirement: `authoring::map_json(root, id)` (and later a typed map).
  - Was a gap, FIXED: a command-line import (checkout `assets/`, `PROCYON_ASSETS`) has no `import.json` and was reported "Imported by
    engine version unknown ... Import again". The engine only requires the stamp of the per-user folder; `status_of` in `main.rs` applies
    the same rule (developer folders with `ships/` are ready), and a per-user folder stamped by another version still warns.
  - Only a 2D top-down plan was built (egui painter); the 3D scene (scenery GLBs through the engine's spawners) was not attempted.

- [x] **3. Move one island with the mouse and save `map.json`.**

  Findings:
  - Works. Dragging Island02 #3 in the plan (mouse down within 12 px, drag, release) moved it from [-350.8, -617.8] to
    [-369.2, -332.3]; "Save map.json" wrote `<scratch>/maps/red_rover/map.json` (whole file: a mod `map.json` without `extends`
    replaces the base one) and a `mod.json`, with `jsonfmt::dump`. Compared by meaning with the import, only that `pos` differs.
    Driven with xdotool on Xvfb (software Vulkan) and seen on the real display under X11; screenshots in `local/` (not committed).
  - egui needs no Bevy picking for this: the plan is an egui widget with its own drag sense, so the `bevy_egui/picking` warning of
    item 1 can be settled by turning that default feature off.
  - Writer differences found on real engine data: (a) the exponent form: the import writes `2e-06` (Python), the Rust writer `2e-6`,
    so red_rover's `players[].forward` and `teams` lines change text (not value) on save; item 5's "no engine file has such numbers"
    was wrong. (b) A dragged position is f32 pointer maths widened to f64: `-369.23077392578125`. (b) is FIXED: a dragged coordinate is stored as the f32's
    shortest decimal (`-369.23077`; the engine reads positions as f32). (a) PROVISIONAL: `2e-06` vs `2e-6` is fixed engine-side
    (the engine will own the canonical writer; the editor switches to it later).
  - Gap: no undo, no snapping, no height (z) or rotation editing; the scratch folder is a text field, not a project concept.

- [x] **4. Edit one rule through a form generated from `openprocyon --dump-rule-schema`.**

  Findings:
  - Works. The schema is taken in-process: `openprocyon::rules::schema::to_json()` is public at v0.2.0 (`pub mod schema` in the private
    `mod eval`, re-exported through `rules::*`), the same value `--dump-rule-schema` prints, no assets needed. `schema_form.rs`
    builds a Type picker plus one widget per field kind: `bool` checkbox writing "TRUE"/"FALSE", `number` DragValue with `range`,
    `enum` ComboBox, `text` edit, every other kind a ComboBox of the mission's defined names. Tested: schema loads, booleans are
    strings, new items carry required defaults, names are collected from the fixture. The egui widgets themselves are compiled
    but not seen: no display here.
  - Gaps for the `authoring` API: (a) `rules::check::Defs` (what a mission defines, per kind) is private; only `known_names`
    (groups) is public. The editor re-reads the names from the files' layout (`Names::collect`, PROVISIONAL), and flags, timers
    and counters through the schema's kinds. Needs `defined_names(mission, map, media) -> per-kind sorted lists`.
    (b) The schema JSON loses nothing, but `soft` and `used` are not acted on by the form; `Kind::ConditionList`/`Condition`
    (`OP Any Of`, `OP Not`) have no widget yet. (c) Rule-level keys (`name`, `Run Once`, `Is Active`, `conditions`, `actions`) are
    not in the schema; the editor hard-codes them, and the files hold JSON booleans there, unlike the Type fields.
    The schema should describe the rule envelope too.

- [x] **5. Save `map.json` and `mission.json` in the engine's JSON layout** (indent 2, number lists on one line, final newline;
  see `tools/jsonio.py` in the engine).

  Findings:
  - Works. `jsonfmt.rs` ports `tools/jsonio.py` (indent 2, number lists on one line, `ensure_ascii` escapes, key order kept via
    serde_json `preserve_order`, final newline). The Python tool wrote the committed expectations in `tests/fixtures/gates/` and
    `edge/` once; the unit tests check the Rust writer is byte-identical on the gate-ambush files and a synthetic edge file, and
    that read-then-write is stable.
  - Known difference (PROVISIONAL): number text is serde_json's; exponent forms differ from Python (`1e-5` vs `1e-05`). No
    engine file has such numbers (wrong: imported maps do, see item 3). Requirement: the `authoring` API should own the one writer so tools cannot drift.

- [x] **6. Show `--check-mission` findings inline.**

  Findings:
  - Works. `rules::check::check(mission, map, media)` and `Problem { level, rule, at, field, message }` are public at v0.2.0 and run
    in-process; the panel lists them and marks the field a problem names (red label plus the message). Tested: the worked
    example has no errors; renaming a volume to "Gate Aera" gives an error on field "Volume Name" suggesting "Gate Area".
  - Gap: a `Problem` has no structured location. The editor finds the condition or action by parsing `at`
    ("action #5 'Start Timer'") and the rule by name (PROVISIONAL, depends on wording). Requirement: findings with rule index,
    list (conditions/actions), item index and field, plus the suggested replacement as data (it is only inside `message`).
    Also `Level`/`Problem` have no serde, so a JSON export for other tools needs the same gap closed.

- [x] **7. Launch the game with `--mod <folder> --map <id>` on the result.**

  Findings:
  - Works. The map section's "Play-test map" button spawned `openprocyon --mod <abs>/local/spike-mod --map red_rover`
    (`PROCYON_GAME_BIN` = the engine checkout's `fast` build); the battle loaded with the mod report `Editor scratch 0.0 ...
    overrides 1: maps/red_rover/map.json`. A fixed-step screenshot with and without the mod (frame 300) shows Island02 beside the
    player's start instead of behind it. The editor stays responsive while the game runs (`try_wait`).
  - The relative scratch path must be made absolute before spawning (done): the game resolves `--mod` against its own working folder.
  - Gap: the game accepts a `--mod` folder that does not exist without an error (it lists the folder with no files and carries on);
    a typo in a path from a tool silently tests the unmodded game. Requirement: `--mod <missing>` is an error, plus `--check-mod`.
  - Unchanged from before: no way to ask the game binary its engine version; `rfd` folder dialogs block the UI thread while open
    (acceptable in the spike).
