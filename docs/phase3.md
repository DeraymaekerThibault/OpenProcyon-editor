# Phase 3: editor shell, ships and weapons

What the editor does in Phase 3 (`docs/roadmap.md`), package by package: what works, what could not be verified in a cloud container
(no game import, no display), and what the engine's `authoring` API is missing, written as requirements for the engine. The Phase 0
record stays in `docs/spike.md`.

Engine pin: `openprocyon` from git at rev `0819c4c` (OpenProcyon#620, the Phase 1 authoring contract), until #620 is merged and tagged.
Bevy's version and features are the engine's `game/Cargo.toml` at that rev.

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
