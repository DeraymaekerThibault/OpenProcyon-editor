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

- [ ] **3. Move one island with the mouse and save `map.json`.**

  Findings:

- [ ] **4. Edit one rule through a form generated from `openprocyon --dump-rule-schema`.**

  Findings:

- [ ] **5. Save `map.json` and `mission.json` in the engine's JSON layout** (indent 2, number lists on one line, final newline;
  see `tools/jsonio.py` in the engine).

  Findings:

- [ ] **6. Show `--check-mission` findings inline.**

  Findings:

- [ ] **7. Launch the game with `--mod <folder> --map <id>` on the result.**

  Findings:
