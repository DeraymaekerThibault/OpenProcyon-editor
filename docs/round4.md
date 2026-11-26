# Round 4 of the local test

For the local agent on the maintainer's machine (real assets, a display, the game binary). Report PASS, FAIL, PARTIAL or SKIPPED per
step, with what you saw. Labels are the editor's own; a label that differs from the screen is a finding, say so.

> **Before the first step**, confirm three things, and stop and report if one is wrong. (1) The editor checkout you build has
> `rev = "5e6ad33"` for `openprocyon` in `Cargo.toml` (OpenProcyon#666 is in it). The engine's version string is `0.2.0` before and
> after, so the Play-test tab cannot tell them apart. (2) The game binary in `PROCYON_GAME_BIN` is built from OpenProcyon main at
> `5e6ad33` or newer (`git log -1` in the engine checkout it was built from). (3) `PROCYON_ASSETS` and `PROCYON_GAME_BIN` are set in
> `.env`.

**What a test covers now** (editor issue #5; `tools/check.sh` runs them on every commit, so a round does not need to repeat them). The tests draw the real tabs and panels in the shared rig (`tests/common/mod.rs`) over synthetic mods, never game content:
- 2 and 3, the number boxes: `tests/map_settings.rs` (a click into the `world_half` or sun-direction box and out again writes nothing and saves the same bytes; a typed number applies on Enter or on focus loss; the undo steps).
- 4, the Skirmish section: `tests/tabs.rs` (30 and then 120 script rules give the same plan-toolbar position, at most half a window below where it was with the section closed: the section scrolls inside its own area).
- 5, the backdrop state: `tests/tabs.rs` (a picture copied onto one map: another map shows no path, no message and "off"; back on the first, "on" and no message).
- 7, layout: `tests/tabs.rs` (the Missions tab at 500 and at 1200 wide: no text outside the window, the plan under the rule's details when narrow and in a right-hand column level with the list when wide). The pushing around of neighbours that round 4 saw is not tested; it is backlog (`docs/roadmap.md`).
- 8 and 14, the pickers: `tests/schema_pick.rs` (the filter resets on a button click and on Escape; every name of a template form's list is reachable, the fourth and, by scrolling, the last).
- 13, the gate: `tests/mission_gate.rs` builds `gate-ambush` from an empty folder with the calls the buttons make and compares it with the hand-written files; a second test does rule 2, rule 3 and the task "Clear the gate" on the Missions tab (the "Ambush on arrival" template, three names picked from the lists, Expand) and finds the same file. The rest of the 13.x steps are calls, not clicks, and 13.12 (the game) is manual.
- 15, the problems panel: `tests/tabs.rs` (the panel holds the findings `--check-mod --json` prints, the same counts and the same wording, for a mod with errors and a warning; the command is `check_mod(dir, base_assets())` and its counts, game/src/cli.rs, so the test computes what it prints). Running the game binary itself is manual.
- 1 keyed saves, in part: the existing save and keyed-merge tests (`tests/map_project.rs`, `tests/mission_retail.rs`, `tests/keyed_engine.rs`) on synthetic retail-like files; a retail map or mission from the player's import is still a manual step.
- Also pinned by a test since round 4: the ship id box selects its text on a click, the play-test outcome line, the height number while dragging (step 10, the editor's line only).

Still manual: 1 on real retail files, 6, 9, 10 in game, 11, 12, 13.12 and 15 with the game binary. Of these 6, 9, 10 and 12 need the game binary and are meant to become commands run under Xvfb (issue #5, parts 2 and 3).

Rules for the run: never save into the import folder; use a fresh empty mod folder outside this repository; do not upload any
converted game content (screenshots of the editor with game models are fine to describe, not to attach). Anything you guess is
PROVISIONAL.

## A. The round-3 fixes (headless tests only so far)

1. **Keyed saves.** Open a retail skirmish map as a diff (Maps tab, pick a retail map, answer "new mod" when asked which mod).
   Move one object, add one object, delete one other. Save (Ctrl+S). Open the saved `map.json` in a file manager only to read it.
   Expect `extends`, and the changes as keyed entries (by `id`), no `/objects/<index>` pointers, and no retail object that you did
   not touch written out. Repeat on a retail mission: edit one rule's field and one task's text; expect rules and tasks keyed by
   `name` and `Name`.
2. **Unchanged numbers are not rewritten.** Open the saved map from step 1, change nothing, Save again. Expect no change to the
   file (same bytes). Then open a number box, click in and out without typing: still no change.
3. **Number boxes.** Type `12.5` into a number box and press Enter: applied. Type another value and click elsewhere: applied on
   focus loss. Typing a value with several keystrokes makes ONE undo step: Undo once returns to the value before you typed.
4. **Maps tab scrolls.** With a map that has a long Skirmish section, the section scrolls inside its own area; the plan and the
   rest of the tab do not jump when a message appears or goes.
5. **Backdrop state per map.** Set a backdrop path on map A, switch to map B: the path and its message are cleared; switch back to
   A: A's state is what the file says.
6. **Play-test error lines.** Launch a play-test that must fail (a mod whose `map.json` has a bad key the game refuses, or point
   `PROCYON_GAME_BIN` at a binary that exits with an error). The error shown ends with the game's real last line, not an earlier one.
7. **Layout.** Resize the window narrow and wide on the Maps, Missions and Play-test tabs: things that come and go (messages,
   errors, the template form) do not push their neighbours around; no widget is cut off without a scroll.
8. **Pickers.** Open a name picker in a template form (Missions tab, "+ From template", for example "Ambush on arrival"), type a few
   letters in its filter, close it without choosing (click the button again, and once with Escape), reopen it: the list is complete
   again (a stale filter used to hide names such as a second player). Check that BOTH players of the mission are offered.

## B. Items never confirmed in a real run

9. **A `--lobby` win.** Build a skirmish map in the editor (or use the gate mod of the Phase 4 gate), play it with "Play in lobby",
   and win with the console command `win` (in every singleplayer battle since OpenProcyon#654). Expect the Game Summary screen, and
   the editor showing the run ended (outcome victory). Repeat with `lose`.
10. **A 3D height drag.** Maps tab, 3D view: drag a placed object's height handle up and down. Expect the object to move in the 3D
    view while you drag, the height number to follow, and the play-test (`--map`) to place it at that height in game.
11. **A retail mission's ship-kill rule.** Only `mission03` has one (PROVISIONAL: the maintainer's note). Open it as a diff, play to
    the point where the rule applies, sink the ship it names, and read the play-test rule report: the rule should show as fired.
    SKIPPED is acceptable if you cannot reach it; say how far you got.
12. **Sinking your own ship.** Console or play: sink a ship of your own player. Expect no kill credited to you (the rule report
    does not count it). Covered only by a unit test so far.
13. **The Phase 5 gate from an empty folder.** Follow the numbered list in `docs/phase5.md`, section "Phase 5 gate, and how it is
    checked", part 1, from "New mod..." through step 12, with no JSON file opened until the comparison step. Tick each line; write
    down every difference from the expected file.
14. **The "Raiders" player in the template pickers.** On the mission you built in step 13 (and, if you can, on the tester's own
    gate mission), open each template that asks for a group ("Reach a point", "Ambush on arrival", "Escort to a gate", "Sink a
    group to win", the dialogue and cutscene blocks). Expect `Hero`, `Raiders` and `RaiderShips` in every group picker. If a name is
    missing, copy the mission's `players` line and the ships' `group` values into the report, exactly as written (spelling, spaces,
    capitals): a name the mission does not declare, or that differs in capitals or by a space, is not offered by the engine's list.

## C. After the engine move: what #666 changed for the editor

What the pin move from `f1bf09f` to `5e6ad33` brings (engine commits #664, #653, #668, #666, #672; read from the engine's diff, not
run here): the `authoring` API the editor calls is unchanged and the editor built with no code change; the mission, script, media
and map file formats are unchanged; the short `--lobby` spec the editor writes is unchanged (a spec starting with `{` is a new whole
JSON setup, which the editor does not use). What can show in a run:

- **A fix that touches step 9.** A battle start now forgets the last battle's summary counts, lobby roster, skirmish end rules and
  nebula state. Before, after a restart the Game Summary counted both runs, and a mission started after a skirmish kept the
  skirmish's alliance roster and victory rule.
- **One rule world for missions and skirmish** (shared builder, `ActiveScript`, one rule for matching a group name, one hostility
  answer). The engine's commit says behaviour is unchanged and describes a verification run (not repeated here); steps 9, 11 and 12 go through that code, and
  `--start-at`, the console and kill credit all read the same script now.
- **`--check-mod`**: it parses a map's `mission.json` and `map.json` once, and reads only `.json` files for effect and sound names
  (it used to print an invalid-JSON error for every binary and shader of a mod, findings were not affected). A present but invalid
  optional JSON file now prints an error line in the game's log where it used to be silent: that line can show in the play-test's error
  lines.
- **Multiplayer phase 1 (#653)**: every skirmish order is now a command, and the lobby setup is written in the log. Not used by the
  editor, but a skirmish play-test (`--lobby`) runs through it.
- **Remaster (#668)**: the black hole is drawn as a raymarched gas volume; only a map with a black hole shows it.

15. Run `--check-mod <mod> --json` on the Phase 4 and Phase 5 gate mods with the game binary and compare with the editor's problems
    panel: errors and warnings must agree (same wording, same count). Then play one mission and one skirmish map for a minute
    each and report anything that differs from round 3 (the rule report, the play-test's error lines, the summary counts after a
    restart with `R`).

## Report

For each step: number, PASS / FAIL / PARTIAL / SKIPPED, one line of what you saw. For a FAIL, add the exact text of any message and
what you did just before. Do not paste file contents that came from the game.
