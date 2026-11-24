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
