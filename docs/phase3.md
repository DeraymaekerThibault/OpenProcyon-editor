# Phase 3: editor shell, ships and weapons

What the editor does in Phase 3 (`docs/roadmap.md`), package by package: what works, what could not be verified in a cloud container
(no game import, no display), and what the engine's `authoring` API is missing, written as requirements for the engine. The Phase 0
record stays in `docs/spike.md`.

Engine pin: `openprocyon` from git at rev `b2c01f5` (OpenProcyon#620, the Phase 1 authoring contract, merged on main), until a release
tag includes it. Bevy's version and features are the engine's `game/Cargo.toml` at that rev. (Phase 3 was built on #620's head `0819c4c`; the merge only fixed bugs, and nothing the editor uses changed.)

## P1. Engine pin and API switch

Works (headless, `tools/check.sh` GATE OK):

- The editor's own JSON writer (`jsonfmt.rs`) is gone; every file is written with `authoring::to_json_string`. The spike's fixtures
  (written once by the engine's `tools/jsonio.py`) are now held against the engine's writer (`tests/engine_writer.rs`), including the
  Python exponent form (`2e-06`) that the spike's writer got wrong.
- Names for the rule dropdowns come from `authoring::defined_names`; findings are located by their structured fields
  (`rule_index`, `list`, `item`, `field`, `suggestion`) instead of parsing the printed `at` text.
- Map loading goes through `authoring::overlay_into` into the editor's own scratch folder (`<temp>/openprocyon-editor-<pid>/`) and
  `authoring::read_map`; with no mod the import is read directly (the engine builds nothing then).
- `ENGINE_VERSION`, `base_assets()` and `rule_schema()` come from `authoring`; the editor no longer repeats the version by hand.
- The crate is a library (`src/lib.rs`) plus a thin binary, so modules are testable from `tests/`; `src/edit.rs` holds the one edit
  type (a JSON pointer, old and new value) that forms, the undo stack and the 3D view share.

Not verified here: the map tab against a real import (needs assets and a display).

Engine requirements:

- `defined_names` gives `group` as the TWT groups only, but a group field accepts players and placed ships too (`known_names`). The editor
  offers `known_names` for `group` fields (PROVISIONAL). Requirement: per field kind, the names a field of that kind accepts.

## P2. Schema form engine

Works (headless): `src/form/` draws any document against a JSON Schema from `authoring::schemas()` and returns `Edit`s (it never
mutates). `form::model` (no egui) resolves `$ref`, optional types (`anyOf [X, null]`, `type: [X, "null"]`), enums, integer and number
ranges (`format: uint*` means a minimum of 0), maps (`additionalProperties`: guns by mount name, sections by label, bullet damage by
material), arrays and short number lists (`pos`, `rot`, `homing` as one row). Keys the mod does not set show the inherited (parent)
value greyed, with Set; set keys have Reset. Unknown keys (`authoring::unknown_keys`) are warning rows with Rename to / Remove. Findings
are drawn under their field; a focus pointer opens and scrolls to a field. Map rename and remove set the whole map in one edit, so the
order of sections and mounts survives an undo.

Tested: every node of all five real schemas resolves; the shapes of the ship, guns, bullets and mod schemas; every edit helper applied
and undone byte for byte; the whole form drawn headless (`egui::Context::run_ui`) for every schema with unknown keys, findings and an
inherited document, with no edits and no panic.

Not verified here: layout, drags, clicks, focus scrolling (needs a display). Typing gives one edit per change; the undo stack
coalesces them (P3).

Engine requirements:

- `Finding.path` is a dotted path (`guns.Gun_A.arc_deg`, `engines[0].fx`), ambiguous for keys holding `.` or `[` or made of digits.
  Requirement: a JSON pointer in `Finding` (the editor converts, PROVISIONAL: `edit::dotted_to_pointer`).
- `unknown_keys` returns the keys known at that place but no nearest one; `check_mod` findings carry `suggestion`. Requirement: the
  nearest key from `unknown_keys` too (the form lists all, unranked).
- `beam` (guns), `blast`, `mine`, `travel_snd` (bullets) are untyped in the schemas, so the form edits them as raw JSON. Requirement:
  typed schemas for them.

## P3. Mod project shell

Works (headless):

- Open a mod folder or make one (`mod.json` only: `id` and `name` from the folder, `version` 0.1.0, `format` 1, `game_version` =
  `ENGINE_VERSION`; an existing `mod.json` is never overwritten). `mod.json` is edited through the P2 form (the "Mod" tab).
- Each project builds its own overlay (`authoring::overlay_into`) in its own scratch folder, rebuilt after a save or a reload.
- Documents keep what the mod's file says (`own`), what it is laid over (`base`: the `extends` parent or the import's file) and what the
  game will see (`edited`, read back through `read_merged`). A retail file the mod does not have opens on the import's version.
- Undo/redo is a command stack of `Edit`s across documents (Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y); a failing edit changes nothing;
  consecutive sets of one field (typing, dragging) are one step.
- Save as diff (`project::diff`): an `extends` file holds only the changed keys, with `set` pointers when only some array elements
  changed and that is shorter; a merged-path file (`guns.json`, `bullets.json`, ...) holds only the changed entries and keys; any other
  file is written whole. Keys the mod's own file already has stay (in their order), hand-written `set` entries stay. A clean document is
  never written, nor one whose new content equals the file in meaning. Every expected result is checked against the engine's own
  overlay: laying the written file over its base with `overlay_into` + `read_merged` gives back exactly what was edited.
- Code-first: open files are stamped (length, time, content hash) and polled about once a second. A file changed outside the editor
  reloads when it has no unsaved edits and says so; with unsaved edits it is a conflict with "Reload from disk" / "Keep my edits" (the
  latter re-applies this session's edits over the new file, so keys a script added are kept). The editor's own writes are not seen as
  outside changes.
- Layer view from the engine's `ModReport` (overridden, merged, extends, added, textures, skipped with the reason) plus unsaved files.
- Problems panel fed by `authoring::check_mod` on a background thread (on open, after each save, on "Check"); errors first, the
  suggestion as "did you mean"; a click sets `Focus { file, path }`, which the ship and weapon editors (P4) open.

Not verified here: the panels and dialogs in a window, overlay rebuild time on a full import, `extends` parents from a real import.

Engine requirements:

- The path rule (merged / extends / replaced) and the merged-path list are restated in `project::merge_kind` (PROVISIONAL; the engine's
  `modding::MERGED` is not part of `authoring`). Requirement: `authoring::merge_rule(rel)`.
- A mod file cannot remove a key the file below it has; the editor warns on save and keeps the key. Requirement: a removal form for
  `extends` files (or say it is not wanted).
- The parent of an `extends` file is found by restating the rule (another ship's or map's file at depth 3, else a sibling
  `<parent>.json`), and `ModEntry` does not say which parent each `extends` file used. Requirement: the parent path per file in the
  report, or `authoring::read_parent(root, rel)`.

## P5. 3D ship view

Works (headless, pure parts tested):

- `ship_view::coords`: game axes to Bevy through the engine's `data::conv`, and its inverse (round trip tested against `conv`);
  positions are written as the f32's shortest decimal.
- `ship_view::points::hardpoints(ship.json, Facts)`: every point of a ship with where it comes from (a JSON pointer into `ship.json`,
  or a model node): gun mounts, engines, wakes, flags, tow (tower / towee), homing, boarding (model only) and crew nodes by kind. The
  model is read with the engine's `data::derive::read` (layers with their triangles, hardpoint nodes, game axes). The rules for which
  wins (ship.json over the model, `engines`/`wakes`/`flags` arrays replace the model's nodes, `tow` and `homing` win) follow
  `docs/data-format.md` (PROVISIONAL mirror, see the requirements).
- `ship_view::place`: moving a point writes `pos` (or `/tow/tower`, `/tow/towee`, `/homing`) into `ship.json`, never into the GLB; a
  model-only gun gets its own `guns` entry (`pos` plus `rot` with the node's yaw); a model-only flag restates the flags array from the
  model; model-only engines and wakes and boarding points are refused with a message (see requirements). Adding a point on the hull:
  guns (`pos`, `rot` [0,0,0], no `gun`/`arc_deg` guesses, so the checker asks for them), engines/wakes/flags where the array exists,
  crew nodes with the name grammar, tow ends, homing.
- `ship_view::pick`: ray against the model's triangles (Möller–Trumbore), nearest point to a ray, drag along an axis handle.
- A synthetic GLB built in a test goes through `derive::read` and `hardpoints`.
- The Bevy scene: the hull drawn from the same triangles the engine hits (one flat material per layer, the hovered layer highlighted
  with its `sections` entry), an orbit camera (right/middle drag, wheel) in the central rect, gizmos per point (coloured by kind, crew
  by crew kind), gun facing and firing arc, engine and wake headings, the tow line, homing, axis handles on the selected point. A drag
  is one undo step (emitted on release).

Not verified here (no display): everything drawn; the 3D camera under the egui camera on one window (if the view is black or wipes
the panels, look at camera order and clear colour first); cursor to viewport mapping; pick radius and handle distance (untuned).
The textured GLB scene is not loaded (the triangle meshes are enough to place points); `model` (another ship's model) is not followed
by the stand-alone loader; P4 feeds the view from the project.

Engine requirements:

- A public inverse of `data::conv`, and the rotation of a `rot` triple (`mount_rotation`) and its inverse in `authoring`; the editor
  mirrors the private `mount_rotation` to draw facings and arcs (PROVISIONAL).
- The resolved hardpoints of a ship as the loader sees them (which of ship.json and the model wins), so the editor does not restate it.
- Crew nodes of the model in `derive::Facts`, and the crew node kinds as a public list (the editor mirrors the private `KINDS`).
- A way for `ship.json` to move one derived engine or wake without restating the whole array (the array needs values only the engine
  derives, such as an engine's `fx`), and a `ship.json` key for boarding points.

## P4. Ship and weapon editors

Works (headless):

- Ships tab: every `ships/<id>/ship.json` of the project's overlay (import plus mod), or of the import alone when no mod is open
  (read only). Rows say retail / extends / overrides / added / new and unsaved; filter and sort. A ship opens through the project and is
  edited with the P2 form on the engine's ship schema, the retail (or `extends` parent) values shown as inherited, the problems of that
  file under their fields. A retail ship changed this way is saved as `{"extends": <id>, changed keys}` (tested: only the change).
- New ship: extending an existing one (nothing is written until something changes), or from the author's own GLB, which is copied to
  `ships/<id>/model.glb` with a minimal `ship.json`; a GLB byte-identical to any `ships/*/model.glb` of the import is refused.
- Weapons tab: `data/weapons/guns.json` and `bullets.json` entries (changed / added marks), each edited with the form on the entry
  schema; only the changed entries and keys are saved (merged-path files). New gun or bullet from scratch is the default; duplicating an
  entry warns that a duplicated retail entry is saved whole (it is new, so it has no base to diff against). A gun's bullet fields get a
  picker of the bullet ids.
- Compare with retail: points, MaxSpeed, MaxThrust, Mass, core HP, the sum of section HP, mounts and their guns, sight, against the
  import's file of the same ship, the `extends` parent or a chosen ship, with differences; a value a file leaves out shows "derived".
- "Show in 3D" feeds the P5 view from the project; each drag lands in the undo stack as its own step (`Project::do_step`), and the
  view follows undo and redo.
- Clicking a problem about a ship or weapon file switches to its tab, selects the ship or entry and brings the field into view.

Not verified here: all drawing and dialogs, the 3D view on a real ship, anything against a real import.

Engine requirements:

- The resolved values of a ship as the loader sees them (points, speed, mass, hit points, mounts after derivation from the model), so
  the compare table can show what the game uses instead of "derived".
- A schema marker for references (a gun's bullet, a mount's gun, an effect, a sound), so the form offers pickers without the editor
  knowing field names; the bullet picker finds its fields by the `BulletTypeName` prefix (PROVISIONAL).

## P6. Play-test and export

Works (headless):

- Play-test runs the real game: `--mod <absolute folder> --map <id> [--ship <id>]` (`PROCYON_GAME_BIN`, else `openprocyon` on the
  path), never blocking the editor; unsaved documents can be saved first. `<game> --version` is compared with `ENGINE_VERSION` and a
  mismatch is shown.
- Screenshot preview: `--shot <scratch>/shots/<map>-<ship>.png --shot-frame N [--dist --yaw --pitch]` into the editor's scratch
  folder, polled, shown in the panel; on failure the exit status and the last lines of the game's stderr. The game is stopped after
  180 s.
- Export: a zip of the mod folder under one top folder named after the mod's id (unzipped into `mods/` it gives `mods/<id>/mod.json`),
  sorted entries, fixed timestamps, so two exports are byte-identical; dot-files and `.git/` left out; `mod.json` required; a missing
  `LICENSE` warned; every file byte-identical to the file at the same path in the player's import warned ("game content must not be
  shipped"), as is a file in a format of the original game; the user confirms before exporting with warnings. New dependency: `zip`
  (approved), `deflate` only.

Not verified here: the panel, the save dialog, a real game run (`--version`, the `--shot` file names for a list of frames).

Engine requirements:

- Identity with the import is by bytes at the same path only (PROVISIONAL): a converted-but-edited file, or a retail file under another
  path, is not found (the GLB of a new ship is checked against every `ships/*/model.glb`, P4). Requirement: a library "compare with the
  import" in `authoring` (what `tools/compare_assets.py` does, by meaning), returning the files that are game content.

## Seen running (Xvfb, software Vulkan, synthetic data)

The editor was run in a virtual display in the cloud container (Xvfb + Mesa lavapipe) on synthetic data of our own
(`tools/synthetic_mod.py`: a box hull with a sail and hardpoint nodes made in code, a ship.json, a gun, a bullet, and a mod that extends
the ship) and driven with xdotool. `tools/shot.sh <out.png> [--mod <dir>] [--tab <name>] [--ship <id>] [--frame N]` repeats the pictures
(the editor's own `--screenshot`, plus `--mod`, `--tab`, `--ship` flags for scripted runs, `src/startup.rs`).

Seen working: every tab draws; the Mod tab form; the problems panel with `check_mod`'s finding and its suggestion; a click on the
finding switches to the Ships tab and scrolls to the unknown-key row with Rename to / Remove; the 3D view composes with egui (the 3D
camera under the egui camera, panels intact); hull layers, hover highlight with the section note, every hardpoint kind, gun arcs, the
tow line; "Add crew place" refuses without a name, then places the node on the clicked face; dragging the z and the x handle moves the
point along that axis (one undo step each); Undo twice restores both positions in the 3D view; Save writes an `extends` file that keeps
the hand-written keys and restates the crew list whole (an array replaces the inherited one).

Found and fixed: (1) every 3D pick and hover was off by the side panel's width: Bevy 0.20's `viewport_to_world` takes window
coordinates and subtracts the viewport origin itself, and the view subtracted it too; (2) faces of a model with mixed winding were
lit dark from behind (materials are now double-sided); (3) the ship form offered `extends` and `set` as fields, which the save writes
itself (hidden from the form now); (4) `--ship` with the 3D tab did not open the ship's document first.

Still only on a real machine: real retail hulls and textures, a real game for play-test and preview, frame rate, file dialogs.

## Engine requirements, all packages

For the engine's `authoring` API, in the order they block the editor most:

1. A JSON pointer in every `Finding` (the dotted `path` is ambiguous). (P2)
2. The resolved ship as the loader sees it: hardpoints with their source (ship.json or model node), points, speed, mass, hit points,
   mounts after derivation. (P4, P5)
3. `authoring::merge_rule(rel)` and the merged-path list; the parent file of an `extends` file (in the report or `read_parent`). (P3)
4. A way for an extending file to remove an inherited key, or a statement that there is none. (P3)
5. A way to move one derived engine or wake without restating the array; a `ship.json` key for boarding points. (P5)
6. Public `mount_rotation` and its inverse, and the inverse of `data::conv`. (P5)
7. Crew nodes of the model in `derive::Facts`; the crew node kinds as a public list. (P5)
8. A schema marker for references (bullet, gun, effect, sound) and typed schemas for `beam`, `blast`, `mine`, `travel_snd`. (P2, P4)
9. The nearest key from `unknown_keys`. (P2)
10. `defined_names` per field kind as a field accepts it (a `group` field takes players and placed ships too). (P1)
11. A library "compare with the import" by meaning. (P6)
