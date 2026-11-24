# Test fixtures

No game content here: everything is the OpenProcyon project's own GPL-3.0 documentation material.

- `source/mission.json`: copied unchanged from the engine's `docs/examples/gate-ambush.mission.json` (the worked example of
  `docs/missions.md`).
- `source/map.json`, `source/media.json`, `source/mod.json`: the minimal files of `docs/missions.md` section 1, as written there.
- `gates/`: the same four files as a mod folder (`mod.json`, `maps/gate_ambush/{map,mission,media}.json`), generated once with
  the engine's `tools/jsonio.py` `dump` (`jsonio.dump(json.load(f))`). `tests/jsonfmt` style tests check that the Rust writer
  reproduces these bytes from `source/`. Do not edit them by hand.
- `edge/edge.json`: a synthetic file (escapes, non-ASCII, empty containers, nested number lists) written by the same tool from `source/edge.json`.
