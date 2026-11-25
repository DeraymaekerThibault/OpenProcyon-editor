# Phase 4: map editor, plan and work list

What the map editor needs, package by package, in build order: what it reuses from the spike and Phase 3, what is new, what it needs
from the engine, and what depends on the format freeze (which gates the release, not the start of work). It is written so a work session can start at "Tomorrow" at once.
Phase 3 stays in `docs/phase3.md`, the spike in `docs/spike.md`, the plan in `docs/roadmap.md` (Phase 4, principles 1 to 9).

## Status

- **Freeze: `map.json` format 1 (v0.3).** Policy (roadmap principle 5): we are pre-release, formats may change freely and the editor
  follows them; Phase 4 starts now. The freeze comes just before the v0.3 release and gates the release. Today `map.json`
  has no `format` key, no schema and no typed loader. The open decision "which formats v0.3 freezes" (`map.json`, `mission.json`,
  `media.json`, `script.json`) is still unticked in the roadmap.
- **Prototype, not merged.** A typed `map.json` with `map.schema.json` and `script.schema.json` is being prototyped on engine branch
  `claude/map-schema`. When this plan was written the branch had no commit
  beyond engine main (`42083e4`) and a clean worktree, so there was nothing to read. Treat its shape as unknown (PROVISIONAL).
- **Engine checked.** Everything below about the engine was read from engine main `42083e4` (and its uncommitted `authoring` work).
  The editor now pins engine main `f91542f` (OpenProcyon#626), which holds the keyed merge, `map.schema.json` and `--only-mod`.
  The editor-side restatements marked "until the pin moves" can now switch to the engine.
- **Rule while the format moves:** build everything, but keep key names behind one small module per file (`map_doc`), so a format
  change touches one place. Anything that names a key (`players[].start`, `nebulae[].cloud`) is re-checked at the freeze.

## What the spike and Phase 3 give us

| Module | What Phase 4 takes from it |
| --- | --- |
| `project/` (`Project`, `Doc`, `DocKind::Extends`, `do_step`, undo/redo, `save`, `poll`, `guard`) | one undo stack, save as diff, outside-change reload, write guard, "which mod" prompt; a map is just more documents |
| `project::diff` | `extends` plus changed keys; `set` pointers for equal-length arrays only (the array problem, P1) |
| `edit` (`Edit`, JSON pointers, `apply`, `undo`) | every map change is an `Edit` on `maps/<id>/map.json`; a drag is one step |
| `form/` (`model`, `widgets`) | map settings, one object's fields, a player's fields, a nebula's fields, from the schema |
| `ship_view/` (`coords`, `pick`, `scene`, `points`, `place`) | orbit camera, ray against triangles, nearest point to a ray, axis-handle drag, gizmo drawing, game-to-Bevy axes |
| `map_view.rs` | the 2D plan: drag with a 12 px pick, `to_screen`/`to_world`, `short` (f32 shortest decimal), save and play-test. Spike code: it holds its own `Value`, no undo |
| `mission.rs`, `mission_panel.rs`, `checks.rs`, `schema_form.rs` | `maps/<id>/{mission,map,media}.json` loading, rule forms, `check_mission` findings. Phase 5's, but P8 (skirmish script) reuses them |
| `launch.rs`, `playtest_panel.rs`, `preview.rs` | `Launch` builds the game command line; play-test, `--shot` preview, game version check |
| `which_mod.rs`, `export.rs`, `project::problems` | retail edit opens "which mod", export (no content check, by maintainer decision), problems panel with `check_mod` |

## Packages, in build order

"Before freeze" says what can be built now and kept; "After freeze" what must wait. Sizes are for one agent in one session
(S under half a day, M a day, L two days or more).

### P1. Map documents in the project, and the array problem (B5) (L)

Reuses: `Project::open_doc`, `DocKind::Extends` (already `maps/<id>/x.json`), `diff::compute`, `Edit`, undo, `guard`.
New: open `maps/<id>/map.json`, `script.json`, `media.json` as project documents; a map list in the project panel (retail from the
import, the mod's, new); "New map" (folder, empty `map.json`, `mod.json` left alone); a map doc shown as MERGED over its parent.
`mission::Mission` and `map_view::save` (whole-file writers) go away for maps and go through the project.
**The array problem.** `objects`, `ships`, `mines`, `players`, `teams`, `nebulae` are arrays. The engine's `modding::merge` replaces
an array whole and `set` reaches only existing elements (`/objects/3/pos`). So today a mod that adds one island to a retail map must
restate the map's whole `objects` array, which is the game's own map content in the mod (hard rule 1 in spirit, and the export
would warn). Moving an object is fine (`set`), adding, deleting or reordering is not. Options, in order of preference:
(a) the engine merges these arrays by key (`id` for objects, ships and mines; `name` for players and nebulae; `name_id` for
teams), with a way to remove an entry (engine requirement 1); (b) the editor writes the whole array and the export warns. Build (a)
in `diff` behind a trait the engine's rule fills; until then P1 saves moves as `set` and refuses add/delete on an `extends` map
with a message (a map whole in the mod is fine: a new map is one's own).
Built (T3, T4): `src/map/project.rs` and `src/project/keyed.rs`. `project::keyed::KeyedRule` carries the engine's rule as its default
(PROVISIONAL: a copy of the engine's private `keyed_defaults`, held to the engine by `tests/keyed_engine.rs`: `objects`/`ships`/`mines` by `id`, `nebulae`/`players` by
`name`, `"merge"` overrides, `{"<key>": X, "$remove": true}`), so add and delete on an extending map are keyed entries; they are
refused only where the list below has an entry with no key (retail decoration), the list is not keyed (`teams`), or the file says
`"merge": {"<list>": false}`. The engine's overlay merges such a file (the editor's merged view and the play-test read that); the editor's own
`keyed::extend` is only a model that the tests hold to `keyed::engine_extend` (the engine through `overlay_into` + `read_merged`).
Engine needs: 1 (keyed merge: done; still wanted public: `keyed_defaults`, so the copy can go), 2 (schemas), 13 (`merge_rule`, already requirement 3 of Phase 3).
Before freeze: documents, map list, new map, undo, diff of moves, the refusal. After freeze: keyed arrays (needs the engine's rule).

### P2. Top-down 2D view (M)

Reuses: `map_view.rs` drawing and picking, `Edit`, `Project::do_step`, `short`, `guard`.
New: the plan becomes a view over a project doc (no own `Value`): pan and zoom, grid, `world_half` square, objects by kind (island,
asteroid, base, other), ships, players' starts with facing arrows, mines, nebula polygons, labels; hover and selection; a drag is one
undo step; box select; the view follows undo and redo. A pure `plan` module (no egui) holds the transforms and hit tests so tests run
headless, as `ship_view::coords` and `pick` do.
Engine needs: none. Placement of dockable scenery is not where `pos` says: the engine subtracts the first dock's height, or
`scene_origin` (`scenario::spawn::scenery_transform`, public but not in `authoring`). In 2D only x and y matter, so no issue; in 3D it does (P7).
Before freeze: all of it, except the kind of each key (read `pos`, `rot`, `type`, `id` through one accessor module `map_doc`).
Built (T5): the "Maps" tab, `src/maps_panel.rs` (drawing and buttons) over `src/map/view2d.rs` (what the plan shows and what a drag,
turn, placement, duplicate or delete writes; unit-tested) and `map::project::change_entries` (a group move or turn is one undo step).
It shows the merged view of a project map document, never a copy: map picker (mod, retail, "retail, changed by the mod"; a new map
not saved yet is listed too), new map, Save, Undo and Redo, snap switches, Fit, pan (right or middle drag), zoom (wheel), objects,
ships, mines and players' starts with their headings, click, shift and box selection, drag (one step, snapped), a turn handle above the
selection and a typed "Turn" (about the selection's centre; `rot` keeps its tilt, a start's `forward` turns), Duplicate (a copy 30
units away with the next free id), Delete (Delete key), and "Place scenery" from the catalogue scanned over the project's overlay
(a click places one, shift keeps placing, Esc stops). An add or delete that an extending map refuses shows the project's text.
Not here: nebula polygons and teams (T8, T7), starts are moved and turned but not added, deleted or copied (T7), no z handle (T9),
duplicates of several entries are one undo step each. `src/map_view.rs` is down to `short`. `--map <id>` shows a map at start
(`tools/shot.sh ... --tab map --map test_map`; CI takes that picture of the synthetic map).

### P3. Scenery catalogue, place, rotate, snap (M)

Reuses: `Edit`, `Project::do_step`, `pick`'s nearest-point idea.
New: `catalogue` lists scenery from the player's import and the mod: `type` with its `scene`. There is no engine list (requirement 3),
so the editor builds one by reading the overlay: types and scenes used by the maps of the import, `models/**/*.glb` names under
`scenery/`, keys of `data/collision.json` and `data/docks.json`; a new object needs both `type` and `scene`, and the pairing exists
only in maps (PROVISIONAL, until the engine lists it). Place: click on the plan; new `id` the smallest unused across `objects`,
`ships`, `mines` and animals (ids must be unique across them, `docs/missions.md` section 3); `rot` as a 3x3 matrix from a heading
(`[fy,-fx,0, fx,fy,0, 0,0,1]`, `docs/missions.md` section 2); rotate by handle or typed degrees; duplicate; delete.
Snap math, pure and tested: grid, angle steps (15 degrees), snap to another object's `pos`, to `world_half` edge, keep inside
`world_half` (the checker warns outside it).
Engine needs: 3 (scenery listing). Works without it, with the guess above.
Before freeze: catalogue by scanning, snap math, heading to matrix, id allocation, all with tests. After freeze: the entry's required keys.

### P4. Map settings: name, world_half, sky, lighting, backdrop (S)

Reuses: `form/` on the map schema (the form engine takes any schema), `which_mod`.
New: a settings tab. `sky` picker from `models/sky/*.glb` of the overlay and `data/render/skies.json` keys (the `_LayerN` suffix is
dropped by the engine); `world_half` number with the map square drawn in P2; `lighting` as `ambient`, `roof`, `floor` colour pickers
and `roof_direction` (a dial in 2D); `backdrop` as a PNG chosen from the author's files and copied to `maps/<id>/backdrop.png`
(any string in `backdrop` turns it on). Retail names stay as picked, never copied.
The sun of a baked sky and `lighting` share one setting (roadmap): the engine reads `roof` as the sun and `floor` as ambient
(PROVISIONAL in the engine); where a baked sky stores its sun is not in `authoring` (requirement 8), so P4 offers one "sun" control
that writes `lighting.roof_direction` only and does not touch the sky.
Engine needs: 2 (map schema), 8 (sky list, sun).
Before freeze: pickers by directory listing, colour widgets. After freeze: key names, defaults, ranges (from the schema).

### P5. Starts, players, teams (M)

Reuses: `form/` for one player, P2 for placing `start` and `forward`.
New: a players list (skirmish lobby slots): `name`, `team`, `start` [x,y,z], `forward`, `formation`, `playable`, `race` (0 Navy, 2
Pirate, 3 Procyon, 4 open, others unverified), `color` [r,g,b,a]; `teams` (`name_id`); add, remove, reorder (index matters: slot
order); a start is a point with a facing arrow in P2, dragged and turned; formation preview from `ship::formation_slots`
(engine function, same as the spawner). Pre-placed `ships` of historical maps: list and place by `type`, `player`, `pos`, `rot`.
A mission's `players` live in `mission.json`, not here (the spike's `Names` already handles that side).
Engine needs: 1 (players are an array, keyed by `name`), 2, check for slots (4).
Before freeze: the 2D start handle and the formation preview. After freeze: everything that names a key.

### P6. Nebulae and mines (M)

Reuses: P2 polygon drawing (volumes of the mission editor look the same), `form/`, `Edit`.
New: `nebulae[]` entries `{name, groups, polygon, cloud, storm, meteors, sound_distance}`: draw the polygon, drag its points, add
and remove points; `cloud` / `storm` / `meteors` by form. **Linked pieces** (roadmap): in a MISSION a storm is three records that must
agree: `map.json` `nebulae[]`, `mission.json` `environment.nebulae[]` and a `volumes` polygon with the same name
(`docs/missions.md` section 8). "New nebula" writes all three with one name as ONE undo step; a rename and a polygon drag keep
them equal. In a skirmish only `map.json` is needed (occlusion needs the mission side; PROVISIONAL, check against the game).
`mines[]` `{type, id, pos}`: `type` from the `Bullet_StarMine_*` names of `bullets.json` (read through `read_merged`); retail
missions' mines are never read, custom missions' and skirmish maps' are.
Engine needs: 2; a nebula agreement check (5): none exists today (`rules/check.rs` has no such check).
Before freeze: polygon editing, the three-record writer behind one function. After freeze: the entry's keys.

### P7. 3D view of the map (L)

Reuses: `ship_view::scene` (orbit camera, camera under the egui camera, double-sided flat material), `pick::ray_hit`, `pick_point`,
`axis_drag`, gizmo drawing, `coords` (game axes to Bevy, through the engine's `data::conv`), the cursor-to-viewport fix `4fadcf9`.
New: each object drawn from its GLB (`scene` loaded as `<scene>.glb`, found by file name under `models/`); placement as the engine
does it: `scenery_transform` (dock height subtracted unless `scene_origin`) and `game_rotation` (public in `scenario::spawn`, not in
`authoring`; PROVISIONAL: the editor calls them for now). Pick an object by ray, move it along z with a handle (z is the altitude
the 2D view cannot show), the 2D and 3D views share one selection and one undo stack.
The models are not triangle-only like a ship's `derive::read` result: the editor needs a plain GLB loader for scenery (the engine's
`derive::read` is for ship models and reads hardpoint nodes). Start with triangle meshes, as the ship view did; textures later.
Engine needs: 3, public `scenery_transform` (12).
Before freeze: all of it except the object's key names. The biggest unknown is the load time of large retail GLBs (not measured).

### P8. Skirmish map: flag, victory, `script.json`, `media.json` (L)

Reuses: `mission_panel`, `schema_form`, `checks`, `mission::Mission` (made into project documents in P1), `defined_names`,
`check_mission(.., skirmish = true)`. This is Phase 5's rule editor in a small form: P8 builds only what a skirmish map needs and
leaves the rule editor's polish (templates, per-rule highlighting on the map) to Phase 5.
New: `"skirmish": true` toggle (puts the map in the skirmish list; a retail campaign map never gets it). Victory: `map.json`
`victory` rules (read by `victory.rs`) or `"victory": "script"` in `script.json` with `Team X Wins` / `Remaining Team Wins` / `End
Game` and `tasks` with `team`; a page that explains which one decides (docs/modding.md "Skirmish map scripts": the map's own end
rules unless the script says `victory: script`). `media.json` (`speech`, `messages`, `ship_names`) beside the rules. Types marked
`campaign_only` in the rule schema are hidden or flagged here (`--check-mission` reports them as errors in a script). Not yet in
the engine: the attack and ram orders and border zones are stored but the skirmish captain AI does not read them (PROVISIONAL, engine doc); warn in the form.
Engine needs: 2 (script schema), `defined_names` per field kind (Phase 3 requirement 10), 4.
Before freeze: documents, forms and checks already work on the current rule schema. After freeze: `script.json` top-level keys.

### P9. A mod's own scenery: collision, docks, crew (L)

Reuses: `ship_view::place` ideas (a point written to a data file, not the GLB), the merged-document path (`DocKind::Merged` for
`data/collision.json`, `data/docks.json`, `data/crew_nodes.json`), `form/`.
New: for a scenery type the mod adds, edit `data/docks.json` (`points[]` with `label`, `pos`, `yaw`; `tender`) by placing points
on the model with the P7 gizmo; `data/crew_nodes.json` the same way (grammar as a ship's `crew_nodes`); `data/collision.json` is
generated from the model, not hand-written (`vertices`, `indices`, `layers` box tree in Bevy axes), so the editor calls an engine
function that derives it. **That function does not exist** (requirement 6): `tools/export_collision.py` reads the retail's own
files, not a modder's GLB. Without it P9 does docks and crew only, and says an object with no collision entry is "only a picture".
Engine needs: 6 (derive collision from a GLB), 7 (schemas for the three data files; `crew_nodes.schema.json` exists, docks and collision do not).
Before freeze: docks and crew placement. After freeze and the engine function: collision.

### P10. Play-test, retail skirmish map as a diff, and the gate run (M)

Reuses: `launch::Launch`, `playtest_panel`, `preview` (`--shot`), `export`.
New: `Launch` gets a lobby variant: `--mod <abs> --lobby "map=<id>;points=1000;limit=10;slots=human,N*computer[;seed=n][;fleet=..]"`
(exists in the engine: `lobby_scenario.rs`; `docs/modding.md` and `docs/development.md`). The map must be `"skirmish": true` or the
lobby says "no skirmish map". Also `--map <id>` (no lobby, for a mission or a quick look) and `--shot` as today. Open a retail
skirmish map as a diff: pick `red_rover` (or any of the 14) in the map list, edit, save into a mod as `extends` plus `set` moves
(P1; adds and deletes after requirement 1), play it with the mod over the import. The gate map (below) is built from an empty folder in this package.
Engine needs: 12 (`--only-mod`: the game loads the player's other enabled mods, so a play-test is not of this mod alone; done: the pin has it, and the play-test passes it by default).

**T11 as built: the flags the editor passes** (`src/launch.rs`, `tests/launch_args.rs`; the engine owns their meaning).

| Launch | Arguments |
|---|---|
| map, straight into the battle | `--mod <abs> --map <id> [--ship <id>]` |
| map through the lobby | `--mod <abs> --lobby "map=<id>;points=P;limit=L;slots=human,N*computer[;seed=S]"` (no `--map`, which would skip the lobby; no `--ship`, the lobby's Auto Assign picks the fleet). Defaults `points=1000`, `limit=10`, `N=1`; the engine checks the ranges and the map must be `"skirmish": true` |
| either, this mod alone (default) | `--only-mod` right after `--mod <abs>` |
| screenshot | `--mod <abs> --map <id> [--ship <id>] --shot <abs> --shot-frame N[,M] [--dist D --yaw Y --pitch P]` |
| mod check, before every play-test | `--check-mod <abs> --json` (exit 1 means errors; the output is still read) |

The play-test runs the check on a thread, shows its errors and warnings in the panel, starts the game when there are no errors (or the
check could not run) and otherwise holds the launch back until "Launch anyway". `--only-mod` (the engine's `modding::ONLY_MOD`, in the
pinned `f91542f`) is passed by default ("Load only this mod" is ticked; untick it to see the other enabled mods the game then loads).
This closes engine requirement 12.

## Engine requirements for Phase 4

Checked against engine main `42083e4` (see Status). "Missing" means not found in the source or the docs.

1. **Keyed merge for arrays of objects by id, and removal.** Missing: `modding::merge` recurses into objects only; arrays and scalars
   replace; `set` needs an existing pointer and cannot add or remove (docs/modding.md "Extending a file"). Needed: `objects`, `ships`,
   `mines` by `id`; `players` by `name`; `nebulae` by `name`; `teams` by `name_id`; a marker to delete an entry; a rule for objects
   with no `id` (decoration). Blocks P1 add/delete on retail maps, P5. Largest item.
2. **Schemas for `map.json`, `script.json`, `media.json`.** Missing at main: `authoring::schemas()` returns ship, guns, bullets,
   crew_nodes, mod. Prototype on branch `claude/map-schema`, not merged, no commits seen (PROVISIONAL). Also a `format` key per file.
3. **Scenery listing in `authoring`:** the `type` and `scene` pairs, with which have collision and docks. Missing: `defined_names`
   lists mission names only; the pairing exists only inside maps.
4. **Map checks for a map without rules.** `check_mod::missions` skips a map folder with no `mission.json` or `script.json`
   (`rules_file` is `None`), so `world_half` and missing-scenery warnings do not run for a map that is only `map.json`. Also missing:
   skirmish slot checks (start inside `world_half`, `forward` not zero, team ids in `teams`, duplicate ids across objects and ships).
5. **Nebula agreement check:** the map entry, the mission environment record and the volume have the same name. Missing: the
   roadmap lists it under Phase 1; `rules/check.rs` has only name collection for nebulae.
6. **Collision derived from a model.** Missing: no function builds a `data/collision.json` entry from a GLB; `tools/export_collision.py`
   is the importer for retail files. Docks the same (`tools/export_docks.py`). Needed as a library call plus a CLI.
7. **Schemas for `data/collision.json` and `data/docks.json`.** Missing (only `crew_nodes.schema.json` exists).
8. **Sky and sun:** the list of skies (retail and `data/render/skies.json`), and where a baked sky's sun direction is read. Missing in
   `authoring`; `scenario/lighting.rs` reads `lighting` only and marks the roof-as-sun choice PROVISIONAL.
9. **Rotation helpers:** heading to the 3x3 matrix and back, as `game_rotation` does. `scenario::spawn::game_rotation` exists, public,
   not in `authoring`, and has no inverse. Needed for rotation handles that read back.
10. **Unique ids:** a check (and a helper) that `id`s are unique across `objects`, `ships`, animals and mines. Missing (the doc
    says "keep them unique"; no check found).
11. **`--lobby` is present**, with `--mod`: `map`, `points`, `limit`, `slots`, `seed`, `fleet`. Not verified: `--lobby` together with
    `--mod` for a mod's skirmish map (no game here). The editor needs `--check-mod` clean first.
12. **`--only-mod` (or `--no-installed-mods`)**, and public `scenery_transform` / `game_rotation` / `formation_slots` in `authoring`.
    First is Phase 3 requirement 12, now in the engine (`modding::ONLY_MOD`); the others are still open.
13. **`authoring::merge_rule(rel)`** and the `extends` parent per file (Phase 3 requirements 1 and 3, on an engine branch, not
    merged at last check): P1 needs both to stop restating the rules in `project::merge_kind` and `parent_rel`.

Present and used as is: `overlay_into`, `read_merged` / `read_map`, `to_json_string`, `check_mod`, `check_mission` with `skirmish`,
`defined_names`, `known_names`, `rule_schema`, `base_assets`, `ENGINE_VERSION`; `--check-mod --json`, `--lobby`, `--map`, `--mod`, `--shot`.

## Before the freeze, and after

Phase 4 does not wait on the freeze: the right column is what is re-checked against the frozen format (or needs the engine), not
what is held back.

| Stable whatever the format does | Re-check at the freeze (or needs the engine) |
| --- | --- |
| P2 plan transforms and hit tests; P3 snap math, heading to matrix, id allocation, catalogue scan; P7 camera, pick, gizmos (reuse); P1 documents, undo, moves as `set`; pure `map_doc` accessors | key names, defaults and ranges (P4 to P6, P8); keyed arrays (requirement 1); nebula linking details; collision derivation; schema-driven forms for maps |

Rule: nothing outside `src/map/doc.rs` (accessors) and the schema-driven forms may spell a map key. A format change then touches one file.

## Roadmap backlog in Phase 4

The roadmap's modder experience backlog (`docs/roadmap.md`, B1 to B6) applied to maps. This plan was first written without the backlog
(its branch predated it); the mapping below closes that gap.

- **B1, show what the engine fills in.** Map keys the engine derives or defaults (lighting from the sky, `world_half`, a dockable
  scenery's height datum, `scene_origin`) show "generated by the engine" with the value, not an empty box. Needs the default
  annotations asked for in B1 (not decided yet). Part of P4 and P9.
- **B2, retail or extension.** Engine-only map keys (`mines` on a custom map, `script.json`, `"victory": "script"`, `scene_origin`)
  carry the extension badge; campaign-only rule Types are hidden or flagged in a skirmish `script.json` (`rule_schema` already has
  `origin` and `campaign_only`). Part of P4 and P8.
- **B3, more checks.** New scenery with no `data/collision.json` entry, a dock point with no dock data, start positions outside
  `world_half` (requirement list above). Shown on the field, from `check_mod`. Part of P5, P9.
- **B4, skirmish maps first.** P8 and P10; `--lobby` play-test.
- **B5, retail as a starting point.** P1 and P10: a retail skirmish map opened from the import and saved as a diff; adding and
  removing entries waits on the keyed array merge (requirement 1).
- **B6, retail stays intact.** The Phase 3 machinery applies unchanged: "which mod?" on the first edit of a retail map, the one write
  guard (`project::guard`) on every map save, and compare and revert per field and per file. Revert of an array entry needs the keyed
  merge too. Part of P1.

## Tomorrow: ordered task list for a multi-agent push

The editor's `src/` is shared with an overnight bug-fix session; start after it merges, or keep to new files. Each task owns its
files; "touches" lists the existing files it may edit, nothing else. Parallel-safe tasks (marked PAR) have disjoint files and can
run at once; the others wait on the number named. New code goes under `src/map/` (module `map`, added to `src/lib.rs` once, in T0).

| # | Task | Size | Parallel | Files (new unless marked) |
| --- | --- | --- | --- | --- |
| T0 | Skeleton: `src/map/mod.rs`, `src/map/doc.rs` (accessors over `serde_json::Value`: objects, ships, players, teams, nebulae, mines; ids), `mod map;` in lib | S | first, alone | `src/lib.rs` (one line), `src/map/mod.rs`, `src/map/doc.rs` |
| T1 | Pure plan math with tests: transforms, hit tests, snap (grid, angle, object, world edge), heading to matrix and back, id allocation. **Started the night before** as `src/map_edit/` (`plan_view`, `snap`, `orient`, `select`, `canvas`; not wired into any tab): review it, add what is missing (world-edge snap, id allocation), and either keep `map_edit` as the module name or move it under `src/map/` in T0 | M | PAR after T0 | `src/map_edit/` (exists), `tests/map_plan.rs` |
| T2 | Catalogue scan of an overlay: scenery types and scenes, skies; tests on a synthetic folder | M | PAR after T0 | `src/map/catalogue.rs`, `tests/map_catalogue.rs`, `tools/synthetic_mod.py` (extend) |
| T3 | Project documents for maps: open, list, new map, merged view, save; moves as `set`; refusal of add/delete on an `extends` map; tests with `overlay_into` like Phase 3's diff tests | L | after T0 | `src/project/mod.rs`, `src/project/diff.rs` (edit), `src/map/project.rs`, `tests/map_project.rs` |
| T4 | Keyed-array diff behind a trait (id/name keyed, removal marker per requirement 1), unit tests against a fake rule; engine-side rule plugged later | M | after T3 | `src/project/diff.rs`, `src/project/keyed.rs` |
| T5 | 2D view over a project doc, on `map_edit::canvas`: pan, zoom, select, drag as one step, place, delete, duplicate, rotate handle; replaces `map_view.rs` | L | after T1, T3 | `src/map/view2d.rs`, `src/map_view.rs` (shrink), `src/main.rs` (one tab entry) |
| T6 | Map settings form (name, description, `world_half`, sky, lighting, backdrop copy) own widgets for sky, lighting and backdrop copy; hover texts from the engine's `map.schema.json`, and `tests/map_schema.rs` holds every key and type to it (a full `form::show_typed` switch would lose those widgets) | S | PAR after T2 | `src/map/settings.rs` |
| T7 | Players, teams, starts and formation preview | M | after T5 | `src/map/players.rs` |
| T8 | Nebulae (three-record writer, polygon edit) and mines | M | after T5 | `src/map/nebula.rs`, `src/map/mines.rs`, `tests/map_nebula.rs` |
| T9 | 3D view: scenery GLB loader (triangles), placement by `scenery_transform`, pick, z handle, shared selection | L | after T5; PAR with T7, T8 | `src/map/view3d.rs`, `src/map/glb.rs` |
| T10 | Skirmish tab: `skirmish` toggle, victory choice, `script.json` and `media.json` through `mission_panel`'s forms on project docs | L | after T3; PAR with T5 to T9 | `src/map/skirmish.rs`, `src/mission_panel.rs` (edit, minimal) |
| T11 | `Launch` lobby variant and play-test of a map; `--check-mod` before launch; docs of the flags | S | PAR after T0 | `src/launch.rs` (edit), `src/playtest_panel.rs` (edit), `tests/` launch args |
| T12 | Docks and crew placement for a mod's own scenery on the T9 gizmo; collision stays "picture only" with a message | M | after T9 | `src/map/docks.rs` |
| T13 | Engine side (a separate checkout, with the maintainer's go-ahead): requirements 1, 4, 10 first; they are what the editor waits on | L | PAR with everything | in the engine, not here |
| T14 | Gate rehearsal: build a skirmish map from an empty folder, `--check-mod`, `--lobby` play-test, write what held and what did not in this file's "Seen running" | M | last | `docs/phase4.md`, `tools/shot.sh` use |

Merge points: T3 and T4 both touch `src/project/diff.rs`: do T4 after T3 and in the same agent. T5 and T10 both read `src/main.rs`'s
tab list: let T5 add its tab and T10 add a second line after it. Nobody edits `src/ship_view/` (it is only read).
After each task: `tools/check.sh` prints `GATE OK`, as AGENTS.md demands, before the commit. Keep commits per task, no push without the maintainer.

## Phase 4 gate, and how it is checked

Roadmap: **a skirmish map built only in the editor loads, lists in the lobby and plays.** Checked in T14, by hand with the real game
(the editor cannot run the game headless here):

1. Make a new mod and a new map in the editor; no JSON file opened.
2. Place islands and asteroids from the catalogue, two starts and two teams, a nebula, mines; set `world_half`, sky, lighting, backdrop.
3. Set `"skirmish": true`; add a `script.json` with one victory rule and a `media.json` line.
4. `--check-mod <mod> --json` reports no errors (a warning for a test of the editor's own links is a bug).
5. `<game> --mod <mod> --lobby "map=<id>;points=1000;limit=4;slots=human,computer"` shows the map in the lobby list, launches, the
   scenery draws where the plan showed it and collides (own scenery: only with requirement 6), the victory rule ends the game.
6. Export writes the zip with the map's files (the export does not check content, by maintainer decision).

Also kept as tests, no assets needed: moves, adds and deletes round trip through `overlay_into` and `read_merged` to the edited map;
a retail-map diff holds only the changed entries; the three nebula records stay equal after every edit. What stays unverified until a
machine with an import and a display runs it: how any of it looks, the 3D load time of retail GLBs, `--lobby` with a mod's own map.

## Risks

1. The freeze slips or the map schema differs from what T6 to T8 assume: mitigated by `map/doc.rs` as the one place that names keys.
2. No keyed merge (requirement 1): editing a retail map by adding or deleting stays impossible, and a workaround would put the game's map in a mod.
3. No collision from a model (6): a mod's own island is only a picture; the gate map must say so or use a retail type.
4. `check_mod` skips maps without a rules file (4): the editor's own maps would pass unchecked until it is fixed.
5. 3D retail GLB load time and memory are unmeasured, and the unit conversions for dockable scenery (`scenery_transform`) are the engine's private rule.
