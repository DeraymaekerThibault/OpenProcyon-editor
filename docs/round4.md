# Round 4 of the local test

For the local agent on the maintainer's machine (real assets, a display, the game binary). Report PASS, FAIL, PARTIAL or SKIPPED per
step, with what you saw. Labels are the editor's own; a label that differs from the screen is a finding, say so.

> **Do not run this yet.** It waits for OpenProcyon#666 to land and for the maintainer to move the editor to the new engine version
> (the move is the maintainer's to trigger). Before the first step, confirm that the editor was built at that engine version
> (the Play-test tab shows the engine version) and that `PROCYON_ASSETS` and `PROCYON_GAME_BIN` are set in `.env`. If the version is
> the old pin, stop and report that.

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

## C. After the engine move: what #666 changed

15. Whatever #666 changes in the engine's authoring code or file formats: list its changes here when the maintainer moves the pin
    (this step is to be filled in then, not guessed now). Run `--check-mod <mod> --json` on the Phase 4 and Phase 5 gate mods and
    compare with the editor's problems panel: both must agree (errors and warnings, same wording).

## Report

For each step: number, PASS / FAIL / PARTIAL / SKIPPED, one line of what you saw. For a FAIL, add the exact text of any message and
what you did just before. Do not paste file contents that came from the game.
