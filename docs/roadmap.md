# OpenProcyon modding tools roadmap

Nov 24, 2026 · @Thibault

## Summary

We build one editor for ships, weapons, maps and missions as a Rust app on Bevy 0.20 with egui, in its own repository. It stands on an authoring contract that lives in the engine: schemas, a mod checker, a public authoring API and versioned formats. The contract comes first because the editor, the game and hand-written mods all need it. Until the v0.3 release formats may change freely and the editor follows them; the formats are frozen just before that release.

Modding with files keeps working the whole time; every engine phase also makes hand-written mods easier (autocomplete in VS Code, clear errors).

| Phase | Where | Delivers | Starts when |
| --- | --- | --- | --- |
| 0. Decisions and spike | both | the stack proven on one small slice, the open decisions settled | now |
| 1. Authoring contract | engine | typed loading, schemas, `--check-mod`, authoring API, format versions, crew points and texture overrides without retail models | Phase 0 says go |
| 2. Freeze ships and weapons | engine | format 1 for `ship.json`, `guns.json`, `bullets.json`, `menu/strings.json`, `mod.json` | Phase 1 done |
| 3. Editor shell, ships and weapons | editor | open a mod, edit with forms, 3D ship preview, play-test, export | Phase 2 done |
| 4. Map editor | editor | place scenery, starts, nebulae, mines | now; `map.json` format 1 gates the v0.3 release |
| 5. Mission editor | editor | ships, groups, paths, volumes, tasks, rules, speech, templates | now; `mission.json` and `media.json` format 1 gate the v0.3 release |
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
5. **Freeze before release, not before building.** We are pre-release: formats may change freely and the editor follows them. Editor phases 4 and 5 start now and do not wait on the freeze. The v0.3 freeze (format 1 for `map.json`, `mission.json`, `media.json` and `script.json`) happens just before the v0.3 release and gates that release, not the start of work. Changes after the freeze come with a migration in the engine that upgrades old files.
6. **Play-testing uses the real game.** The editor launches the game binary with `--mod`, `--map`, `--ship` and `--fire-rule`. It never simulates a battle itself.
7. **Modelling stays in Blender; placement moves into the editor.** Meshes, UVs and materials are made in Blender. Gun mounts, engines, wakes, flags, boarding, dock and tow points are placed in the editor on the 3D model and saved to `ship.json` and the data files, never written back into the GLB. This also works on retail hulls that a mod only extends.
8. **Same legal rules as the engine.** No game content in either repository; the editor reads only the player's own import. Its export does not check a mod for retail content: by maintainer decision, users are responsible for what they share. The hygiene test is copied into the editor repository.
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

Seven phases, each closed by a gate that can be checked; the map and mission phases start now and are not held by the v0.3 format freeze, which gates the release. Sizes are rough person-weeks for one developer working full time.

&#91;embedded content: roadmap · 7 phases, 2 repositories, 1 outside gate\]

Phase 6 runs in the engine beside the map and mission phases; Phases 4 and 5 start now; the v0.3 freeze only gates their release.

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
- Retail stays intact (backlog B6): every edit is saved into a mod, the editor never writes into the player's import, and each file
  and field can be compared with retail and reverted to it.
- 3D ship view: sections, gun arcs, engines, wakes, flags; place and drag every point on the model with a gizmo.
- New ship from a GLB, with retail-texture materials listed from the import.
- Open any retail ship or gun from the import as the starting point; edits are saved as `extends` plus the changed keys (backlog B5).
- Every field shows whether it is retail or an engine extension, and what the engine fills in when it is left empty, including the
  HUD and weapon-bar pictures it draws from the model (backlog B1, B2).
- Play-test button (`--mod --map --ship`), screenshot preview (`--shot`), export to zip (it does not check content, by maintainer decision).
- **Gate:** a community tester builds a new ship and a new gun from scratch without opening a JSON file, and the result passes `--check-mod`.

### Phase 4. Map editor (about 8 to 12 weeks, editor)

- Starts now; does not wait on the freeze. `map.json` format 1 (v0.3) must be frozen before the v0.3 release.
- Scenery catalogue from the import; place, rotate and snap; top-down and 3D views; starts and teams; nebulae; mines; `world_half`; sky and lighting pickers; backdrop picture.
- Linked pieces kept in agreement: a nebula's map entry, environment entry and volume are made together; the sun of a baked sky and the map's `lighting` share one setting.
- A mod's own scenery: collision and dock data derived from its model (an engine tool the editor calls), instead of separate scripts per scene.
- Skirmish maps are the first target, not an afterthought: most community maps are skirmish maps. In scope: `"skirmish": true`,
  players, teams and start positions, `victory` rules, the optional `script.json` (engine extension) with its `media.json`, and a
  play-test through `--lobby` (backlog B4).
- Open a retail skirmish map from the import and save the changes as a diff (backlog B5).
- **Gate:** a skirmish map built only in the editor loads, lists in the lobby and plays.

### Phase 5. Mission editor (about 12 to 16 weeks, editor)

- Starts now; does not wait on the freeze. `mission.json` and `media.json` format 1 (v0.3) must be frozen before the v0.3 release.
- Ships and groups, paths, volumes, points, tasks, rules, speech and messages, campaign order; detailed in the next section.
- Play-test report: which rules fired and which never did, so losing and optional branches are not shipped untested.
- Open a retail mission from the import and save the changes as a diff, as for ships and maps (backlog B5).
- **Gate:** the worked example `gate-ambush` is rebuilt in the editor from an empty folder and its file matches the hand-written one in meaning.
- Plan, engine requirements and the task list: `docs/phase5.md`.

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

## Modder experience backlog

Ideas found while building the spike, each checked against the engine's main branch (24 November 2026, after #620, which is not
in a tag yet: the editor still pins `v0.2.0`). Each item names the phase it belongs to and what it needs from the engine, because the
editor must not restate a data rule (principle 1). Items marked PROVISIONAL are not verified yet.

**B1. Show what the engine fills in.** Many keys may be left out and the engine derives them; the form should say so instead of
showing an empty box that looks required. Phase 3 for ships and weapons, then the same for maps.

| Left out | What the engine does (engine docs) |
| --- | --- |
| `hud/sprites/ShipBar_ShipIcon_<art>`, `WeaponBar_ShipIcon_<art>` | draws them from the model when the overlay is built: a pale side silhouette for the command bar and fleet cells, a top-down outline for the weapon panel (`game/src/shipart.rs`) |
| a gun's `bank` | takes the weapon-panel slot nearest to the gun on that outline (80 fixed slots) |
| `engines`, `wakes`, `flags`, a gun's `pos`/`rot` | read from the model's nodes |
| `core_hp`, `physics`, `sections`, `sphere`, extents | computed from the model, or a Navy Cutter's numbers |
| `crew_posts`, `explosion_fx`, `repair_fx`, `engine_snd` | chosen by race and size |
| `tow`, `homing` | the model's nodes, else the hull ends or the vital hull's centre |

- In the editor: the field shows "generated by the engine" with a preview of the value or picture, and an "override" action that
  creates the file or key. For the HUD pictures, show both generated pictures next to the "add your own PNG" button.
- Engine ask: the schemas do not carry these defaults; they are only in `docs/data-format.md`. Expose them as data (a schema
  annotation such as `x-default-from`, or an `authoring` call that returns the derived ship) and expose the HUD-art drawing
  through `authoring`, so the preview is the engine's own picture. PROVISIONAL: the annotation name.

**B2. Mark retail and extension fields everywhere.** Each field, rule Type and feature carries a badge: *retail* (behaves as the
original) or *engine extension* (OpenProcyon only), and for extensions what kind of change it is: `simulation`, `presentation` or
`control`. A mod's summary lists the extensions it uses, so a player knows it needs OpenProcyon.

- Already in the engine: `--dump-rule-schema` gives every rule Type an `origin` (`retail` or `engine`) and a `campaign_only` flag; the
  `Source` column of "ship.json keys" in `docs/data-format.md`; `ENGINE_SHIP_KEYS` and `ENGINE_GUN_KEYS` in `game/src/data.rs`; the
  table "Engine extensions (not retail)" in `docs/modding.md`, which a test keeps equal to the `Not retail:` markers in the code.
- Engine ask: the JSON Schemas do not carry the origin. Add it per key (for example `x-origin: engine`), and expose the extension
  table (gate, `Changes`) through `authoring`, so the editor reads it instead of copying it.

**B3. More checks and helpers found in the engine docs.** Each is a finding the engine already knows about or a value it already
computes; the editor only shows it at the right place.

- Inherited, overridden or added, per field, with "reset to parent" (the layer view, made field-level).
- A misspelt key: `--check-mod` already names the nearest known key, so offer a one-click fix.
- A new gun that computer captains will never fit, because it is not in `data/ai/weapons.json`.
- A ship with `"skirmish": true` but no `points`, or a `display_name` with no entry in `menu/strings.json`.
- New scenery with no `data/collision.json` entry: it is drawn but nothing hits it; offer to create the entry. The same for dock points.
- Features that only show with a render profile (`crew_nodes` needs the `deckhands` block): say so on the field.
- Rule Types marked `campaign_only` hidden or flagged in a skirmish `script.json`.
- A balance view: a ship's speed, HP, guns and `points` next to retail ships of the same `race`, read from the import (only
  shown, never saved).
- Weapon panel preview: the 80 slots with each gun's bank, and gun arcs on the 3D model (Phase 3 already lists the arcs).
- `game_version` filled from the engine's `ENGINE_VERSION` on save.
- Numbers checked in play: `PROCYON_TRACE` CSV plotted against the gun range or ship speed that was set (Phase 6). PROVISIONAL.

**B4. Skirmish maps.** Most community content is skirmish maps, so Phase 4 targets them first (see Phase 4).

- The engine gaps today: there is no `map.schema.json` or `script.schema.json` yet (`--dump-schemas` writes ship, guns, bullets,
  crew nodes and mod). The map editor needs both before the v0.3 release; add them to the v0.3 freeze list.
- Multiplayer: retail plays skirmish maps in multiplayer, but the engine does not have multiplayer yet (its menu shows the retail
  button, not implemented). The editor play-tests skirmish maps against computer players through `--lobby`. A map script is
  deterministic (no wall clock, no randomness), which is what multiplayer will need; keep the editor from adding anything that
  breaks that. Whether a multiplayer check belongs in the editor waits for the engine's multiplayer plan.

**B5. Open retail content as a starting point.** Load a retail ship, gun, skirmish map or mission from the player's import, change
it, and save only the difference. Nothing retail is written into the mod (principles 4 and 8).

- Ships, guns, bullets, strings: already possible. `extends` (ships) and the key-by-key merge (`guns.json`, `bullets.json`,
  `menu/strings.json`) keep the diff small.
- Maps and missions: `extends` works for every file in `maps/<id>/` (`map.json`, `mission.json`, `script.json`), but arrays are
  replaced, not merged. Moving one island in a retail map would save the whole `objects` list, which is retail content. The
  editor can write `set` pointers (`/objects/12/pos`) for changes inside an array, but pointers break if the list order changes
  and cannot add an entry. PROVISIONAL: whether `set` can append.
- Engine ask: merge arrays of entries that have a stable key (map `objects` by `id`, mission `rules` by `name`, `ships` by `id`) so
  a diff can change, add or remove one entry.
- Retail missions and skirmish maps are edited like retail ships (maintainer's decision, 24 November 2026): a mod ships only the
  diff, never a copy. The engine's `docs/missions.md` still says "do not copy or edit a retail mission"; reword it there to "do not
  copy" so the two repositories agree.

**B6. Retail always stays intact.** Editing retail content never changes the player's import; the change lives in a mod, as a
diff. A mod holding full copies of retail files is not the way to get this: it would be game content (hard rule 1), large, hard to
read, and frozen against later importer fixes and format migrations. A diff already leaves retail untouched, because the engine lays
mods over the import in a separate overlay folder; switching the mod off gives pure retail back. Phase 3.

- Every edit lands in a mod. Editing a retail ship, gun, map or mission with no mod open asks "which mod?", with "new mod" as one
  answer, which creates a complete mod folder (`mod.json`, `LICENSE`, the folders the change needs) and copies no retail file.
- The editor never writes into the import. Every save goes through one function that refuses a path inside the import folder
  (`PROCYON_ASSETS` or the engine's per-user folder), with a unit test. This also enforces hard rule 2.
- Compare and revert. Each file and each field can show the retail value next to the mod's, and "revert to retail" removes the
  key or the file from the mod. Since the mod holds only the change, reverting is a deletion.

## Risks and mitigations

The biggest risk is building on formats that still move; the answer is that the editor follows formats while they move, the v0.3 freeze gates the release, and engine-side migrations cover later changes.

| Risk | Mitigation |
| --- | --- |
| Formats change after an editor phase starts | editor phases wait for format 1; later changes ship with a migration in `authoring` |
| Engine refactors break the editor | the editor uses only `authoring`, pinned to a tag; CI builds it against the engine's main branch |
| Bevy releases every few months with breaking changes | upgrade the editor right after each engine upgrade; bevy\_egui followed 0.20 within two days |
| bevy\_egui needs Bevy features the engine leaves off | checked in the Phase 0 spike, before any real work |
| Editor and game running together share one overlay folder | #136 lands in Phase 1 |
| A modder ships retail content by accident | accepted: by maintainer decision users are responsible for what they share; the export does not check or warn |
| Building the engine library is slow for editor contributors | release builds of the editor for testers; contributors share the engine's build cache tips (`tools/worktree.sh`) |
| The work is mostly one maintainer | the contract phases also make hand-written modding better, so value lands even if the editor phases slow down |

## Open decisions for the maintainer

Checked items are settled (November 2026); the rest remain open.

- [x] Approve `schemars` as an engine dependency.
- [x] Approve the new `ship.json` keys for tow and homing points, and the `mod.json` keys `format`, `game_version`, `requires`.
- [x] Approve crew nodes from JSON and retail texture override by name.
- [ ] Editor repository: `openprocyon-editor` exists, GPL-3.0; owner of its reviews still to name.
- [ ] Which formats v0.3 freezes: `map.json`, `mission.json`, `media.json`, `script.json`.
- [ ] Whether the editor ships inside the game download or as its own download.
- [x] A mod may ship a diff over a retail mission or skirmish map, exactly as over a retail ship (backlog B5).
- [ ] The engine asks from the backlog: origin and default annotations in the schemas (B1, B2), the HUD-art preview in
  `authoring` (B1), `map.json` and `script.json` schemas in the v0.3 freeze (B4), keyed array merge for maps and missions (B5).
