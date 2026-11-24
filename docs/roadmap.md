# OpenProcyon modding tools roadmap

Nov 24, 2026 · @Thibault

## Summary

We build one editor for ships, weapons, maps and missions as a Rust app on Bevy 0.20 with egui, in its own repository. It stands on an authoring contract that lives in the engine: schemas, a mod checker, a public authoring API and versioned formats. The contract comes first because the editor, the game and hand-written mods all need it. Each part of the editor is built only once its file format is frozen, so nothing is redone.

Modding with files keeps working the whole time; every engine phase also makes hand-written mods easier (autocomplete in VS Code, clear errors).

| Phase | Where | Delivers | Starts when |
| --- | --- | --- | --- |
| 0. Decisions and spike | both | the stack proven on one small slice, the open decisions settled | now |
| 1. Authoring contract | engine | typed loading, schemas, `--check-mod`, authoring API, format versions, crew points and texture overrides without retail models | Phase 0 says go |
| 2. Freeze ships and weapons | engine | format 1 for `ship.json`, `guns.json`, `bullets.json`, `menu/strings.json`, `mod.json` | Phase 1 done |
| 3. Editor shell, ships and weapons | editor | open a mod, edit with forms, 3D ship preview, play-test, export | Phase 2 done |
| 4. Map editor | editor | place scenery, starts, nebulae, mines | `map.json` frozen with v0.3 |
| 5. Mission editor | editor | ships, groups, paths, volumes, tasks, rules, speech, templates | `mission.json` frozen with v0.3 |
| 6. Live tools | engine | hot reload, effect and look tuning in the running game | after Phase 3 |

About the repository rules: AGENTS.md forbids another fidelity overview, so this plan lives outside the repository. Its work items go into the existing "Modding platform" milestone and #156.

## Where we start from

The engine is already data-driven end to end, so the editor reads and writes files and never patches the game. What is missing is the contract that tells a tool what a valid file is.

**Already there** (`docs/modding.md`, `game/src/modding.rs`):

- Everything the game simulates is JSON, GLB, PNG or OGG in one tree. Mods lay over the player's own import in load order.
- `extends` plus `set` (JSON pointers) let a mod ship only its changes and reference retail content by id, so mods stay legal to share.
- `--dump-rule-schema` writes every mission rule Type with typed fields, defaults, ranges and enum values, and needs no assets.
- `--check-mission` finds unknown groups, near-miss names, unset flags and out-of-range numbers.
- `--mod`, `--map`, `--ship`, `--fire-rule` and `--shot` already cover play-testing and previews. There is also an in-game Mods screen and a developer console.
- The engine is a library (`game/src/lib.rs`, every module public), and AGENTS.md already says other front ends live in their own crates that depend on it.
- A ship placed by numbers already works: `ship.json` accepts `pos`/`rot` per gun and the `engines`, `wakes` and `flags` arrays, so placing these needs no model edit.

**Gaps that hurt today:**

| Gap | Effect on a modder | Tracked in |
| --- | --- | --- |
| Unknown keys are dropped silently | a typo such as `sigth` does nothing, with no message | `docs/data-format.md` "Silently ignored" |
| Most tables load as untyped JSON | a bad value can panic with no file or field named | #537 |
| Tables kept equal by hand in several places | the rule vocabulary and the merge list can drift | #538 |
| No JSON Schemas | no autocomplete, and a tool has to guess the rules | #156 |
| No `--check-mod` | conflicts and bad references show up only at launch | #156 |
| `game_version`, `requires` not read | no warning when a mod targets another engine version | #156 |
| One overlay folder per checkout | an editor and the game running together would clobber each other | #136 |
| Tow and homing points only as GLB nodes | an editor cannot place them without rewriting the model | none yet |

## Principles and decisions

These decisions are made once and hold for every phase.

1. **The contract lives in the engine.** Schemas, validation, merge rules and format migrations are engine code and change in the same commit as the loader. The editor never restates a data rule.
2. **The editor is a separate repository.** It is a Rust crate on Bevy 0.20 and bevy\_egui that links a tagged `openprocyon` release. Its dependencies, release cadence and contributors stay out of the engine's check gate.
3. **Schema first.** Forms, dropdowns and checks are generated from the engine's schemas and the rule schema, never hand-built per file.
4. **The editor writes diffs, not copies.** A change to a retail ship is saved as `extends` plus the changed keys, in the importer's JSON layout (`tools/jsonio.py`). Mods stay small, legal and readable by hand.
5. **Freeze before building.** Each editor phase starts only once its formats are at a frozen, versioned format. Later changes come with a migration in the engine that upgrades old files.
6. **Play-testing uses the real game.** The editor launches the game binary with `--mod`, `--map`, `--ship` and `--fire-rule`. It never simulates a battle itself.
7. **Modelling stays in Blender; placement moves into the editor.** Meshes, UVs and materials are made in Blender. Gun mounts, engines, wakes, flags, boarding, dock and tow points are placed in the editor on the 3D model and saved to `ship.json` and the data files, never written back into the GLB. This also works on retail hulls that a mod only extends.
8. **Same legal rules as the engine.** No game content in either repository; the editor reads only the player's own import. Its export warns when a mod contains a file identical to the player's import (reusing `tools/compare_assets.py`). The hygiene test is copied into the editor repository.
9. **Licence.** The editor links GPL-3.0 code, so it is GPL-3.0. Mods made with it are the author's own; the tool claims nothing over them.

**Code-first authoring is a first-class path.** Large mods are often generated by scripts (a real expansion writes its whole mission, 433 conditions and actions, from Python). An editor edit to a generated file is lost on the next run, so the contract serves scripts as fully as the editor: schemas, `--check-mod --json`, `--dump-rule-schema` and data-file templates. The editor never assumes it owns a file.

## Architecture

The editor talks to the engine in two ways: it links the engine library for loading, checking and drawing, and it launches the game binary for play-testing. Mod folders are the only thing they share on disk.

&#91;embedded content: architecture · two repositories, one shared mod folder\]

The accented box is the contract: everything the editor knows about valid data comes through it.

**In the engine repository** (new or changed):

- `authoring` module: one public, documented API that the editor uses and that semver protects. It covers loading a mod stack, building the overlay into a folder of the caller's choice (#136), and listing ids (ships, guns, bullets, effects, sounds, scenery types, skies, strings). It also checks a mod and returns structured findings with file, field and severity, and writes JSON in the importer's layout.
- Typed documents (#537) that derive `JsonSchema` with `schemars`. `--dump-schemas <dir>` writes one schema per file type; each release attaches them.
- `--check-mod <dir> [--json]`: everything `--check-mission` does plus unknown keys (warnings, not errors), unknown ids, bad `extends`, conflicts between mods and the `game_version` check.
- `mod.json` gains `format`, `game_version` and `requires`, all read. Each file type has a format number, with migrations in `authoring` that upgrade older files on load.
- `ship.json` keys for the tow and homing points, so the editor can place every point without touching the GLB.

**In the editor repository:**

- One Bevy app with an edit mode and the engine's renderer for the 3D views; egui for panels and forms (bevy\_egui 0.43 supports Bevy 0.20).
- Forms generated from the schemas, a problems panel fed by the `authoring` checks, a layer view (base, each mod) showing what is overridden, merged or added.
- Native open and save dialogs (`rfd`) and an export that zips the mod with its `LICENSE`.
- Pinned to an engine tag. A CI job builds it against the engine's main branch, so a breaking change shows up before the next release.

## Phases

Seven phases, each closed by a gate that can be checked; the map and mission phases wait on the v0.3 format freeze, not on a date. Sizes are rough person-weeks for one developer working full time.

&#91;embedded content: roadmap · 7 phases, 2 repositories, 1 outside gate\]

Phase 6 runs in the engine beside the map and mission phases; only the v0.3 freeze sets when Phases 4 and 5 can start.

### Phase 0. Decisions and spike (about 2 weeks, both repositories)

- Settle the open decisions at the end of this doc.
- Throwaway spike: a crate that links `openprocyon` and bevy\_egui. It loads one imported map, moves one island with the mouse, edits one rule through a form built from `--dump-rule-schema`, saves `map.json` and `mission.json` in the importer's layout, shows `--check-mission` findings inline and launches the game on the result.
- **Gate:** every spike step works on Bevy 0.20. If one fights us, compare against a web stack before Phase 3, not after.

### Phase 1. Authoring contract (about 5 to 7 weeks, engine)

- Already on main (November 2026): most of #537, namely typed guns, bullets, effects, look profile and HUD files, mission checks at load and the per-mod load report. What remains of #537 and #538 (one source for the merge list and the rule vocabulary) finishes here.
- `authoring` module, first slice done (engine branch `claude/exciting-lamport-zxdqra`): side-effect-free asset lookup, engine version, overlay into a caller-chosen folder (part of #136), rule schema and mission check in-process. Next, from the spike's findings (`docs/spike.md`): defined names per kind, findings with a structured location and the suggested fix as data, the rule envelope in the schema, the engine's JSON writer, `--version` on the game binary.
- `schemars` on the typed documents and `--dump-schemas` (approved); schemas attached to each release so VS Code can use them through `$schema`.
- `--check-mod [--json]`, usable by scripts as well as the editor. It checks every name in `ship.json` against the model's nodes (section layers, `adjacent`, damage variants, hardpoints) and warns when a node is matched only by a substring of its name.
- `mod.json` `format`, `game_version` and `requires` read (approved); a format number per file type and the migration hook.
- `ship.json` keys for tow and homing points (approved).
- Crew nodes from JSON (approved): walk lanes, posts, work places and climbs can be listed beside the model, so a mod adds deck crews to a retail hull without shipping that hull's model.
- Retail texture override by name (approved): a PNG in the mod replaces a retail texture wherever it is used, so a texture pack or a backport ships no retail model.
- Mission-check warnings for behaviour that authors only found by experiment (a teleport sends a whole group to the first point of a set, a nebula needs its map entry, its environment entry and a volume to agree, whales start hidden), each added once confirmed against the original's behaviour.
- **Gate:** every bundled mod and every retail map passes `--check-mod` with no errors; a misspelt key in a test mod gives a warning that names file and field; the schemas validate every imported file; a test mod adds crew lanes and a new texture to a retail hull with no GLB in the mod.

### Phase 2. Freeze ships and weapons (about 1 week, engine)

- Format 1 for `ship.json`, `guns.json`, `bullets.json`, `menu/strings.json` and `mod.json`, documented in `docs/data-format.md` as a public contract.
- **Gate:** a change to these files after this point needs a migration and a format bump.

### Phase 3. Editor shell, ships and weapons (about 8 to 10 weeks, editor)

- Open or create a mod; layer view of base and mods; forms from schemas; problems panel; undo and redo; save as diffs.
- 3D ship view: sections, gun arcs, engines, wakes, flags; place and drag every point on the model with a gizmo.
- New ship from a GLB, with retail-texture materials listed from the import.
- Play-test button (`--mod --map --ship`), screenshot preview (`--shot`), export to zip with the retail-content warning.
- **Gate:** a community tester builds a new ship and a new gun from scratch without opening a JSON file, and the result passes `--check-mod`.

### Phase 4. Map editor (about 8 to 12 weeks, editor)

- Waits on `map.json` format 1 (v0.3).
- Scenery catalogue from the import; place, rotate and snap; top-down and 3D views; starts and teams; nebulae; mines; `world_half`; sky and lighting pickers; backdrop picture.
- Linked pieces kept in agreement: a nebula's map entry, environment entry and volume are made together; the sun of a baked sky and the map's `lighting` share one setting.
- A mod's own scenery: collision and dock data derived from its model (an engine tool the editor calls), instead of separate scripts per scene.
- **Gate:** a skirmish map built only in the editor loads, lists in the lobby and plays.

### Phase 5. Mission editor (about 12 to 16 weeks, editor)

- Waits on `mission.json` and `media.json` format 1 (v0.3).
- Ships and groups, paths, volumes, points, tasks, rules, speech and messages, campaign order; detailed in the next section.
- Play-test report: which rules fired and which never did, so losing and optional branches are not shipped untested.
- **Gate:** the worked example `gate-ambush` is rebuilt in the editor from an empty folder and its file matches the hand-written one in meaning.

### Phase 6. Live tools (about 4 to 6 weeks, engine)

- Hot reload of JSON in a running battle; effect and look tuning with live preview in the game; the editor asks a running game to reload.
- **Gate:** changing a gun's range in the editor changes it in the running test battle without a restart.

## The low-code mission editor

Missions are already low-code data: a list of rules, each "when all these conditions hold, run these actions", with typed fields. The editor shows them as a trigger list, the model modders know from the Warcraft III and StarCraft II editors, generated from `--dump-rule-schema` rather than hand-built.

- **Rule list.** Name, run once, active, then a Conditions list and an Actions list. Add a condition or action from a searchable menu grouped by area (ships, objectives, timers, camera, speech); retail and engine Types are marked.
- **Typed fields.** Each field takes the widget its kind asks for: group, volume, path, player, task and point fields are dropdowns of what the mission defines; booleans are toggles (the strings `"TRUE"`/`"FALSE"` are written for the author); numbers carry their range; enums list their values.
- **Inline checks.** `--check-mission` findings appear on the field they concern while typing, with the suggested spelling one click away.
- **The map beside the rules.** Selecting a rule highlights the groups, volumes, paths and points it uses on the map; clicking a volume on the map lists the rules that read it.
- **Templates.** "Reach a point", "ambush on arrival", "escort to a gate", "sink a group to win" each expand into ordinary rules and tasks that stay editable. Three blocks come first, because a real 37-rule mission is mostly made of them: a dialogue block (say a line, wait until the speech is done, say the next), a cutscene block (NIS start, teleports and waits, NIS end) and a failsafe block (any-of and not conditions that switch rules on and off). A template is a data file, so the community and authoring scripts can use the same ones.
- **Speech and messages.** `media.json` entries edited beside the rules that play them; the line's duration is read from the OGG.
- **Play-test from here.** Launch the mission from any rule with `--fire-rule`, and from any point with `--start-at`.
- **Campaigns.** Mission order in `mod.json` `missions`, shown as a list the author drags.

Blockly or a node graph is not needed: retail rules do not nest, so a list with typed fields shows them exactly as they are.

## Prior art and options considered

No existing mission editor can be adopted; we take building blocks from the Rust ecosystem and interface ideas from other games' editors. Versions were checked in November 2026.

| Option | What it is | Verdict |
| --- | --- | --- |
| [bevy\_egui](https://github.com/vladbat00/bevy_egui) 0.43 | egui inside Bevy; 0.43 targets Bevy 0.20 (released 9 and 10 October 2026) | **Use** for all panels and forms |
| [schemars](https://docs.rs/crate/schemars/latest) 1.2 | JSON Schema (2020-12) from Rust types; follows serde attributes | **Use** in the engine for the schemas (new dependency, needs approval) |
| [rfd](https://docs.rs/crate/rfd/latest) 0.17 | native open and save dialogs on Windows, macOS, Linux | **Use** in the editor |
| [bevy-yoleck](https://github.com/idanarye/bevy-yoleck) 0.34 | level-editor kit for Bevy 0.20 | **Do not use**: it saves its own `.yol` format; copy its idea of one app with edit and play modes |
| Bevy's official editor ([discussion](https://github.com/bevyengine/bevy/discussions/24862)) | prototype, no release date | **Do not wait** for it |
| [Blenvy](https://github.com/kaosat-dev/Blenvy) | Blender-to-Bevy workflow, self-described alpha | **Do not depend** on it |
| [egui-snarl](https://docs.rs/crate/egui-snarl/0.12.0) | node graphs for egui | **Not needed**: rules are lists |
| [Blockly](https://developers.google.com/blockly/guides/create-custom-blocks/define/json-and-js) | blocks defined from JSON, now at the Raspberry Pi Foundation | **Not chosen**: brings a web stack and nesting we do not need |
| JSON Forms, [react-jsonschema-form](https://dev.to/yanggmtl/schema-driven-forms-in-react-comparing-rjsf-json-forms-uniforms-formio-and-formitiva-2fg2) | web forms from JSON Schema | **Idea only**: we generate egui forms from the same schemas |
| [Tiled](https://doc.mapeditor.org/en/latest/manual/export-custom/) | 2D map editor with JavaScript exporters | **Rejected**: 2D only, no rules, would be thrown away |
| FreeSpace FRED ([licence](https://wikipedia.com/wiki/FreeSpace_2_Source_Code_Project), [docs](https://fredzone.hard-light.net/freddocs/editors.html)) | the closest fleet-battle mission editor | **Inspiration only**: source licence is non-commercial |
| [OpenRA](https://github.com/OpenRA/OpenRA/wiki/Mapping) | map editor inside the game, missions in [Lua](https://github.com/OpenRA/OpenRA/wiki/Map-scripting) | **Idea**: edit and play in one place; our rules stay data |
| [StarCraft II trigger editor](https://s2editor-guides.readthedocs.io/New_Tutorials/03_Trigger_Editor/033_Trigger_Editor_Interface/) | events, conditions, actions | **Interface model** for the rule list |
| Web stack (Tauri, three.js, Blockly) | ready-made web pieces | **Fallback** only if the Phase 0 spike fails: it would re-implement merging and checks, and its 3D view would not match the game |

Still to confirm in the spike: the Bevy features bevy\_egui 0.43 needs, against the reduced feature set the engine builds with.

## Risks and mitigations

The biggest risk is building on formats that still move; the freeze gates and engine-side migrations are the answer to it.

| Risk | Mitigation |
| --- | --- |
| Formats change after an editor phase starts | editor phases wait for format 1; later changes ship with a migration in `authoring` |
| Engine refactors break the editor | the editor uses only `authoring`, pinned to a tag; CI builds it against the engine's main branch |
| Bevy releases every few months with breaking changes | upgrade the editor right after each engine upgrade; bevy\_egui followed 0.20 within two days |
| bevy\_egui needs Bevy features the engine leaves off | checked in the Phase 0 spike, before any real work |
| Editor and game running together share one overlay folder | #136 lands in Phase 1 |
| A modder ships retail content by accident | the export compares the mod with the player's own import and warns |
| Building the engine library is slow for editor contributors | release builds of the editor for testers; contributors share the engine's build cache tips (`tools/worktree.sh`) |
| The work is mostly one maintainer | the contract phases also make hand-written modding better, so value lands even if the editor phases slow down |

## Open decisions for the maintainer

Three are settled (November 2026); three remain open.

- [x] Approve `schemars` as an engine dependency.
- [x] Approve the new `ship.json` keys for tow and homing points, and the `mod.json` keys `format`, `game_version`, `requires`.
- [x] Approve crew nodes from JSON and retail texture override by name.
- [ ] Editor repository: `openprocyon-editor` exists, GPL-3.0; owner of its reviews still to name.
- [ ] Which formats v0.3 freezes: `map.json`, `mission.json`, `media.json`, `script.json`.
- [ ] Whether the editor ships inside the game download or as its own download.
