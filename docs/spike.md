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
    DefaultPlugins, make sure the bevy/bevy_picking feature is enabled too)`. Harmless for panels. Two ways out, for the
    maintainer to choose: turn off bevy_egui's `picking` default (which also drops the `bevy_picking` crate), or add
    `bevy_picking` to the features, which would no longer match the engine's list. PROVISIONAL until item 3, where clicking
    in the 3D view shows whether egui needs picking at all.
  - The engine's asset lookup `base_assets()` is `pub(crate)` at v0.2.0, and the public `assets_dir()` rebuilds the shared
    overlay folder as a side effect (#136). The editor composes the public pieces instead (`PROCYON_ASSETS`, then
    `campaign::user_dir()/assets`, status from `setup::imported`). Phase 1's `authoring` API should expose the lookup without
    side effects, plus the engine version (the editor repeats `0.2.0` by hand).

- [ ] **2. Load one imported map through the engine's loaders.**

  Findings:
  - Not attempted in this session: must be run on a machine with an import (needs the player's imported assets).

- [ ] **3. Move one island with the mouse and save `map.json`.**

  Findings:
  - Not attempted in this session: must be run on a machine with an import (needs the player's imported assets).

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
    engine file has such numbers. Requirement: the `authoring` API should own the one writer so tools cannot drift.

- [x] **6. Show `--check-mission` findings inline.**

  Findings:
  - Works. `rules::check::check(mission, map, media)` and `Problem { level, rule, at, field, message }` are public at v0.2.0 and run
    in-process; the panel lists them and marks the field a problem names (red label plus the message). Tested: the worked
    example has no errors; renaming a volume to "Gate Aera" gives an error on field "Volume Name" suggesting "Gate Area".
  - Gap: a `Problem` has no structured location. The editor finds the condition or action by parsing `at`
    ("action #5 'Start Timer'") and the rule by name (PROVISIONAL, depends on wording). Requirement: findings with rule index,
    list (conditions/actions), item index and field, plus the suggested replacement as data (it is only inside `message`).
    Also `Level`/`Problem` have no serde, so a JSON export for other tools needs the same gap closed.

- [ ] **7. Launch the game with `--mod <folder> --map <id>` on the result.**

  Findings:
  - Module written and unit-tested for argument building (`--mod <folder> --map <id>`), the binary lookup
    (`PROCYON_GAME_BIN`, else `openprocyon` on PATH) and that a missing binary is an `Err`, not a panic. The UI spawns it with
    `Command::spawn` and polls `try_wait`, so it never blocks. NOT verified: no game binary or assets exist in this container, so
    nothing was launched and the item stays unticked; the window was not opened either (no display, and no GPU for a headless run).
  - Gap: the game offers no way to ask which engine version it is, nor `--check-mod`; Phase 1 should add both. `rfd` folder
    dialogs block the UI thread while open (acceptable in the spike).
