# Tester guide: a new ship and a new gun, without opening a JSON file

This is the script for the Phase 3 gate (`docs/roadmap.md`): a community tester builds a new ship and a new gun from scratch
without opening a JSON file, and the result passes `--check-mod`. Follow the steps in order and fill in the table at the end.
What each part of the editor does, and what is unverified, is in `docs/phase3.md`.

Labels below are copied from the editor. Anything marked PROVISIONAL is a guess: if it is wrong, say so in your report.

## 0. Before you start

- Build and run the editor: `cargo run` (Rust 1.97.1 or newer; on Linux the usual Bevy packages, see `AGENTS.md`).
- The editor reads your own import of the game, only at run time. It finds it through `PROCYON_ASSETS`, else the per-user folder
  (`~/.local/share/openprocyon/assets`, Windows `%APPDATA%\openprocyon\assets`). Without an import it starts, but the Ships and
  Weapons tabs say "No imported assets found" and you cannot go on.
- Play-test and preview run the game: set `PROCYON_GAME_BIN` to the `openprocyon` binary, or have `openprocyon` on the path. The
  same binary gives you `--check-mod` in step 5.
- A model of your own: a binary glTF (`.glb`) made in Blender, with a hull and hardpoint nodes if you like. Never use a model
  from the game. The editor refuses a GLB that is byte-identical to a model of your import (it does not catch an edited copy: do
  not try).
- Pick a ship id (for example `Ship_Test`), a gun name (`Gun_Test`) and a bullet name (`Bullet_Test`). Ids are letters, digits,
  `_` and `-`.

## 1. Make a new mod

1. In the left panel, under "Mod", click "New mod..." and pick an empty folder outside your import and outside this repository.
2. The status line says "Made <folder> with a new mod.json and LICENSE." The Mod tab shows `mod.json` as a form; the problems
   panel at the bottom starts checking.
3. Licence: this button always makes the default (CC-BY-4.0). To choose another (CC-BY-SA-4.0, CC0-1.0, GPL-3.0-or-later, All
   rights reserved) and a holder name, start with no mod open, make your first edit on the Ships or Weapons tab, and use "New mod..."
   in the dialog "Save this change into which mod?" (fields Folder, Id, Author, Licence; then "Make mod"). Either way the
   `LICENSE` file is only a notice (PROVISIONAL): replace it with the full text before you share the mod.

## 2. New ship from your GLB

1. Open the Ships tab. Open the collapsing section "New ship...".
2. Type the ship's Id, then click "New ship from a GLB of your own..." and pick your `.glb`.
3. The status line says "Copied the model into the mod; set the ship's values and save." The ship is selected and unsaved. The
   file is a minimal `ship.json` (`race`, `points`, `skirmish`).
4. Fill in the fields. The editor does not mark required fields. Click "Check" in the problems panel and read what it asks for;
   click a problem to jump to its field. Set values and read the errors again until the ship has none.
5. Save with "Save (Ctrl+S)". A save runs the check again.

Place the points in 3D:

1. On the Ships tab click "Show in 3D". The "3D ship" tab opens with your hull.
2. Orbit with right or middle drag, zoom with the wheel. Hover a layer to see its section.
3. Type a Name in the "Name (gun mount, entry label, or crew suffix)" box (for a gun, the mount name, for example `Mount_1`).
4. Click "Add gun", "Add engine", "Add wake", "Add flag", "Add tow tower", "Add tow towee" or "Add homing", then click on the
   hull (Esc cancels). Select a point in the list (or click it) and drag one of its arrows to move it. "Add crew place" needs a
   crew kind and a name suffix.
5. Add at least two guns, an engine, a wake and a flag. A new gun point has only a position and a rotation: it has no
   `gun` and no `arc_deg` yet, and the problems panel asks for them.
6. Undo and redo: the "Undo" and "Redo" buttons in the left panel, or Ctrl+Z and Ctrl+Shift+Z. One drag is one step. Undo
   twice, check the points went back, redo.
7. Save again.

Boarding points cannot be placed (the view says so). Moving an engine or wake that only the model has is refused with a message:
if you hit it, add the point again in the view instead.

## 3. New bullet, then new gun

1. Open the Weapons tab, choose "Bullets" (the "Guns" and "Bullets" switch is at the top).
2. Type a name in the "New bullet" box, click "New bullet from scratch". The entry opens in a form.
3. The form does not mark required fields (PROVISIONAL, known gap). The new entry is nearly empty: read the problems panel, which
   names what is missing, and fill those fields. Fill them from what you know of the game's data, or look at a retail bullet in
   the same tab for the sort of values. Do not use "Duplicate selected under this name" on a retail entry: it shows a warning
   because the copy would carry the game's data.
4. Switch to "Guns", type a name, click "New gun from scratch". Fill the fields the problems panel names. The bullet fields have
   a picker listing the bullet ids: pick yours. Click your bullet's name.
5. Save.

## 4. Mount the gun on the ship

1. Back on the Ships tab, select your ship.
2. In the form open `guns`. Each mount you placed is an entry (named as you typed it in the 3D tab). Set `gun` to your gun's name
   (a text field) and `arc_deg` to a firing arc. A mount name you did not place in 3D can be added with "Add" and a name under
   "new entry name", but then it has no position: place it in 3D instead.
3. Open "Show in 3D" again: each gun shows its facing and arc.
4. Save.

## 5. Check (this is the gate)

1. The problems panel reads "Problems: 0 errors, N warnings". Click "Check" once more after saving. Read the warnings; a
   warning is allowed, but write down any you cannot explain.
2. In a terminal run:

   ```
   openprocyon --check-mod <your mod folder>
   ```

   (or `$PROCYON_GAME_BIN --check-mod <folder>`). It must report no errors. Copy its last lines into your report.
3. If the editor shows 0 errors and `--check-mod` shows errors, or the other way round, that is a bug in the editor: report it
   with the message.

## 6. Play-test and preview

1. Open the "Play-test & export" tab. If it warns that the game binary has another version than the editor, note both.
2. If it lists other enabled mods: the game loads them too, so the run does not test your mod alone (PROVISIONAL until the engine
   has `--only-mod`). Switch them off in the game's Mods menu first.
3. Pick a Map and your Ship with the "pick" menus. The ship menu lists the import's ships (PROVISIONAL: it may not list
   yours): if yours is missing, type its id in the box.
4. Keep "Save unsaved documents before launching" ticked. Click "Play-test": the game opens on your ship. Fire your gun.
5. Click "Preview screenshot" (frame 300 by default). A picture appears below. It differs between runs (known engine issue).
6. Do not post the picture or any game file if it shows retail art (see the end).

## 7. Export

1. Click "Export zip..." The panel says how many files go into `<id>/` in the zip, and lists warnings. A mod made only of your
   own files should give no warning. The export only warns when `LICENSE` is missing; it cannot tell a notice from the full text,
   so replace the notice yourself.
2. A warning "identical to the player's import: game content must not be shipped" means a file is a copy of the game's. Do not
   export. Tell us which file.
3. Tick "I have read the warnings and want to export anyway" only for a warning you understand. Click "Choose where to save..."
   and pick a place.
4. Unzip the result into the game's `mods/` folder and check `mods/<id>/mod.json` exists.

## Report back

Copy this table into an issue and fill it in. Pass, fail or skipped, with a line of notes. Do not attach game files, converted
assets, or screenshots that show retail models or textures: the project cannot take them. Your own GLB and a screenshot of the
editor's panels are fine.

| Step | Result | Notes |
| --- | --- | --- |
| 0. Editor starts, import found | | OS, GPU, how the import was found |
| 1. New mod, LICENSE made | | licence chosen, how |
| 2. New ship from own GLB | | any refusal; GLB tool and size |
| 2. Fields filled from the problems panel | | which fields were hard to find; anything required that no message named |
| 2. Points placed in 3D (gun, engine, wake, flag, tow, homing) | | wrong picks or hover, black view, panels wiped, drag off |
| 2. Undo and redo | | |
| 3. New bullet from scratch | | fields the problems panel named |
| 3. New gun from scratch, bullet picked | | |
| 4. Gun mounted on the ship | | |
| 5. Problems panel: 0 errors | | |
| 5. `openprocyon --check-mod`: no errors | | **the gate**; paste the output |
| 6. Play-test starts, gun fires | | game version shown, other mods listed |
| 6. Preview screenshot | | |
| 7. Export zip, warnings | | which warnings |
| Did you open a JSON file? | | the gate requires no |

Things to watch, from `docs/phase3.md` ("Not verified here"): every window and dialog was only drawn headless or on synthetic
data, so report layout trouble. In particular: the 3D view against a real model (hover, pick radius and handle distance are
untuned; a black view or a view that covers the panels); the panels on a real import (slow overlay rebuild after a save); file
dialogs; the form's layout and focus scroll when you click a problem; "Compare with retail" showing "derived" for values the engine
fills in; a play-test or preview that does not start (note the message and the game's version); the preview file name; and any
time the problems panel and the game disagree.
