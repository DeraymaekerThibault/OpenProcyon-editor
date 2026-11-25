# Phase 5: mission editor, plan and work list

What the mission editor needs, package by package, in build order: what it reuses from Phases 3 and 4, what is new, what it needs
from the engine, and a numbered task list for a multi-agent push. Phase 4 stays in `docs/phase4.md`, Phase 3 in `docs/phase3.md`,
the plan in `docs/roadmap.md` (Phase 5, "The low-code mission editor", backlog B1 to B6).

## Status

- **Freeze: `mission.json` and `media.json` format 1 (v0.3).** Policy (roadmap principle 5, maintainer decision): pre-release,
  formats may change freely and the editor follows them; Phase 5 starts now, the freeze gates the release only. Today neither file
  has a `format` key, a schema or a typed loader: the game reads both as untyped JSON (`campaign.rs:400-404`, `load_as::<Value>`), and
  `authoring::schemas()` has no mission or media schema (`authoring.rs:110-114`).
- **Engine checked.** Everything below about the engine was read at the editor's pin `f91542f` (engine source read only). The engine
  checkout also has an unmerged branch `claude/authoring-expose` (`ae444cb`, one commit over the pin) that makes the keyed-list merge
  (`keyed_lists`, `extend`, `merge_keyed`), `scenery_transform`, `game_rotation` and `formation_slots` public in `authoring`; it is not
  in the pin, so nothing here relies on it.
- **Maintainer decisions that shape this plan:** formats may change freely before v0.3; the export does no retail-content check;
  speed matters, so the work list is cut for many agents at once (disjoint files, stubs made first, few merge points).
- **Rule while the format moves:** as in Phase 4, one module names the keys. For missions that is `src/mission/doc.rs` (T0): nothing
  else spells a `mission.json` or `media.json` key. The mission keys that Phase 4 put in `src/map/doc.rs` (`environment.nebulae`,
  `volumes`, the `script.json` and `media.json` keys) move there, re-exported from `map::doc` so the map code does not change.

## What Phases 3 and 4 give us

| Module | What Phase 5 takes from it |
| --- | --- |
| `project/` (`Project`, `Doc`, `open_doc`, `open_new_doc`, `do_step`, `do_group`, `apply_form`, undo/redo, `save`, `poll`, `check_dir`, `guard`) | missions are three more documents per map folder; one undo stack across `mission.json`, `media.json`, `map.json` and `mod.json`; `do_group` makes a template expansion or a rename that follows references ONE step; outside-change reload for script-generated missions (code-first) |
| `project::keyed` (`KeyedRule`, `diff_list`, `can_add_delete`, `engine_extend`) | the engine's rule for `mission.json` is already in the copy: `ships` by `id`, `rules` by `name`, `points` by `Name` (`keyed.rs:30`, engine `modding.rs:211`); a diff over a retail mission changes, adds or removes single rules, ships and points. `tests/keyed_engine.rs` keeps the copy honest |
| `project::diff` | `extends` plus changed keys; `groups`, `volumes`, `paths`, `environment.point_sets` and the three `media.json` tables are objects, so they already merge key by key |
| `edit` (`Edit`, JSON pointers) | every mission change is an `Edit`; `edit::pointer` builds escaped pointers for names with `/` or `~` |
| `schema_form.rs` (`Schema`, `Names`, `field_widget`, `item_form`, `new_item`, `bool_value`) | the rule vocabulary from `authoring::rule_schema()`: typed widgets per field `kind`, defaults, ranges, enums; `"TRUE"`/`"FALSE"` written for the author. Missing: nested condition lists (`OP Any Of`, `OP Not`, task `complete_when`), free-text names for flags, timers and counters, `Group,Unit` specs |
| `checks.rs` (`run`, `for_item`, `for_rule`) | `authoring::check_mission` in-process, findings located as data (`rule_index`, `list`, `item`, `nested`, `field`, `suggestion`; engine `check.rs:26-53`) |
| `map/skirmish.rs` | rule editing on a project document (`add_rule`, `delete_rule`, `rename_rule`, `set_rule_flag`, `add_item`, `remove_item`, `replace_item`), `campaign_only_types`, `media.json` tables (`media_entries`, `add_media`, `set_media`): written for `script.json`, generic in all but the file name. T2 lifts them into `src/mission/` and skirmish calls them |
| `map_edit/` (`canvas`, `plan_view`, `snap`, `select`, `orient`) and `map/view2d.rs` | the plan canvas, picking, box select, drag as one step, snapping, heading to `rot`; the map's own scene drawn under the mission's markers |
| `map/nebula.rs` | polygon editing (drag, insert, delete a vertex) and the three-record nebula writer: mission volumes are drawn and edited the same way, and a nebula's mission records are already written from the Maps tab |
| `map/catalogue.rs`, `map/players.rs` | overlay scanning (scenery types, skies); id allocation across `objects`, `ships`, `mines` |
| `launch.rs`, `playtest_panel.rs` | `Launch` (`--mod --only-mod --map`), `--check-mod --json` before a run, `spawn_logged` (stderr to a file, which the play-test report reads, P10) |
| `which_mod.rs`, `export.rs`, `project::problems`, `compare.rs` | "which mod?" on the first edit of a retail mission, export, problems panel, compare and revert per field |
| `mission.rs`, `mission_panel.rs` (spike) | retired by T16; the fixture `tests/fixtures/gates` (the engine's `gate-ambush` worked example, byte-equal to `docs/examples/gate-ambush.mission.json`) stays and is the gate's reference |

## Packages, in build order

Sizes are for one agent in one session (S under half a day, M a day, L two days or more). "Engine needs" refers to the numbered
requirements below.

### P1. Mission documents in the project (M)

Reuses: `Project::open_doc` / `open_new_doc`, `map::project::{list_maps, open_map, map_rel}`, `project::keyed`, `guard`.
New: `src/mission/project.rs`: the missions of the mod and of the import (a map folder with a `mission.json`; retail ids
`mission01`..`mission13`, `tutorial`, `basic_tutorial` listed as retail, `docs/missions.md` section 1); open `mission.json`,
`media.json` and `map.json` of one id as project documents, merged over the import when the mod extends a retail one; "New
mission": the folder, a `mission.json` with `players`, `ships` and `rules` (the game stops without `ships` and `rules`,
`docs/missions.md` section 3), a `media.json` with the three empty tables (the game stops without the file, `campaign.rs:400-404`), a
minimal `map.json` (or an existing map picked), and the id appended to `mod.json` `missions`: one `do_group` step.
Keyed lists on an extending mission come from `project::keyed` unchanged (`ships` by `id`, `rules` by `name`, `points` by `Name`).
Lists the engine does NOT key, refused on an extending mission with the project's message until requirement 2: `tasks`,
`environment.nebulae`, `environment.animals`, `map_text`, `players`, `alliances`; a change to them restates the whole list (the
form says so). Object keys (`groups`, `volumes`, `paths`, `point_sets`, `speech`) can be added and changed, not removed (Phase 3
requirement: no removal form for `extends` files).
Engine needs: 1 (schemas, later), 2 (keyed tasks and removal), 3 (public keyed rule).

### P2. Players, alliances, the human's start, ships and groups (M)

Reuses: `form/` widgets, `map::players` id allocation, `ships_panel`'s list of ship ids in the overlay (`ships/<id>/ship.json`).
New: `players` (names) and `alliances` (pairs, a checkbox grid: every other pair of listed players is hostile, `eval.rs:44-55`);
`player_start` (`name` from the players, `start`, `forward` placed on the plan); `ships[]` `{id, type, group, name, primary, pos,
rot}`: `type` from the ship classes of the overlay (PROVISIONAL scan until requirement 7; a wrong `type` drops the ship silently,
`docs/missions.md` section 4), `group` is the owner (a player), `primary` toggles off any other `primary` of the same player (one
group step), `name` unique across the mission (the game finds `Group,Unit` by `name` only). `groups` `{Name: [ids]}`: a member
picker over mission ships, the map's `objects` with an `id`, and `environment.animals` (ids unique across the three; Phase 4
requirement 10). The "human's ship" rule (the `primary` ship of `player_start.name`, else that player's first ship, else a stand-in
cutter, `docs/missions.md` section 4) shown as a line under the list, read from the documents, not restated as a check.
Engine needs: 7 (ship classes), 11 (unique ids).

### P3. The mission's plan: paths, volumes, points, point sets, map text (L)

Reuses: `map_edit::canvas`, `map::view2d::build_scene` (the map drawn greyed under the mission, read only here; scenery is edited
in the Maps tab), `map::nebula`'s polygon editing, `snap`.
New: `src/mission/plan.rs` (pure: what the plan shows of a mission, what a drag writes) and `src/mission/geometry.rs`.
- `ships` as markers with headings, dragged and turned (one step each), `player_start.start` as the human's marker.
- `volumes` `{Name: [[x, y], ...]}`: closed polygons, at least 3 points (`check.rs:212`, `MIN_VOLUME_VERTICES`); the engine reads two
  numbers per vertex and skips others (`types.rs:353-355`).
- `paths` `{Name: [[x, y, z], ...]}`: polylines with arrows; z kept, edited as a number.
- `points` `[{Name, Position}]`: objective markers.
- `environment.point_sets` `{Name: [{pos}]}`: numbered dots (a teleport puts a group round the FIRST point, `docs/missions.md`
  section 7: the first dot is drawn bigger).
- `map_text` `[{name, text, pos, visible}]`: labels.
**Rename follows references.** Renaming a volume, path, point, point set, task, group, player or rule rewrites every rule field of
that kind that names it (kinds from the rule schema, nested conditions and task lists included) in the same `do_group` step. Which
fields hold which kind is the rule schema's data (`kind` per field), so this restates nothing; the walk itself mirrors the engine's
private `visit_fields` (`check.rs:263-264`; PROVISIONAL until requirement 6). A `Group,Unit` spec renames its part.
Engine needs: 6 (references as data), 5 (nebula agreement, for volumes that are a nebula's polygon).

### P4. Tasks and how the mission ends (S)

Reuses: P5's condition list widget, `form/`.
New: `tasks[]` `{Name, text, Active, required, complete_when, fail_when}` (`Active` and `required` are JSON booleans; engine
`ext.rs:95-96` reads the two condition lists), the envelope keys and their notes from the rule schema's `task.keys`
(`schema.rs:871`, `to_json` `"task"`), so no key is spelt twice. The end: `victory: "objectives"` (required tasks decide) or the
mission's own `Goto Next Level` / `End Game` rules, as a radio with the one-line explanation of `docs/missions.md` section 6;
`victory_message` / `defeat_message` from `media.json` `messages`. A task with conditions but no `required` and `victory:
"objectives"` shows the checker's own warning on the field.
Engine needs: 2 (tasks keyed by `Name`, for retail diffs), 1.

### P5. The rule list (trigger-list UI) (L)

Reuses: `schema_form`, `checks`, the lifted rule operations (T2), `authoring::accepted_names` (in the pin, `check.rs:244`: the names
a field of each kind accepts, so a `group` field offers players and placed ships' groups; the editor still uses `defined_names` plus
`known_names` with a PROVISIONAL note in `schema_form.rs:74-80`, which T3 replaces).
New, the roadmap's "low-code mission editor" section made concrete:
- **List:** every rule with its name, Run Once and Is Active toggles (JSON booleans, defaults `true` and `true`, `schema.rs:832-850`),
  a count of conditions and actions, a red mark when it has findings; filter by text, by "has problems", by "uses <name>"; drag to
  reorder (order matters: rules run in list order each step); duplicate; delete; "new rule" at the selection.
- **Rule body:** Conditions then Actions, each item a collapsible row with its Type, retail or engine badge (`origin`),
  `campaign_only` badge, and its fields; items reorder by drag (order of actions is the sequence; `OP Wait` splits it).
- **Add menu:** searchable by Type name and field names, grouped by area (Mission flow; Objectives; Triggers and counting; Groups and
  ships; Orders; Damage; Docking, towing, boarding; Camera and NIS; Sound, speech, HUD; Environment; Fleets and alliances; Skirmish;
  State init; Engine `OP`). The rule schema has no area (`schema.rs:969-977`), so the grouping is a PROVISIONAL table taken from
  the headings of the engine's `docs/rule-types.md`, held to the schema by a test (every Type in exactly one area) until requirement
  4. `campaign_only` Types are hidden for a `script.json` (Phase 4 T10 behaviour kept).
- **Typed fields:** dropdowns of `accepted_names` per kind with type-ahead; flags, timers and counters are dropdowns PLUS free text
  (they need no declaration, `docs/missions.md` section 6), and a new name is marked "new"; `Group,Unit` as a group dropdown and a
  unit dropdown of the ships' names; bool as a toggle writing `"TRUE"`/`"FALSE"`; numbers with the schema `range` (damage is 0..1);
  enums list their values; `condition_list` and `condition` kinds (`OP Any Of`, `OP Not`) open a nested condition list with the
  same widgets (recursive `item_form`).
- **Inline checks:** `check_mission` runs after every edit on the edited documents (in-process; its cost on a large mission is not
  measured, PROVISIONAL: move it to a thread if one run is over a frame); each finding shows under its field, located by
  `rule_index`, `list`, `item`, `nested`, `field`; `suggestion` is a one-click "use 'Gate Area'". Findings of `check_mod` (ship
  classes, sounds) arrive on the problems panel's thread and are matched the same way when they carry a rule location.
Engine needs: 4 (area and summary per Type), 6, 8 (task findings by index).

### P6. The map beside the rules (M)

Reuses: P3's plan, P5's selection, P3's reference walk.
New: selecting a rule highlights on the plan every group (its ships), volume, path, point and point set its fields name; clicking a
volume (or any named geometry) lists the rules and tasks that read it, each a link that selects the rule; a rule list filter "uses
this". Read only: nothing is written.
Engine needs: 6.

### P7. Speech and messages (`media.json`) (M)

Reuses: T2's media table operations (from Phase 4 T10), `guard` (copying files into the mod), the overlay scan.
New: the three tables beside the rules that use them: a `Play speech event` field has "edit line" and "new line" next to it.
`speech.<EVENT>` `{text, duration, file, face, color}` (`docs/missions.md` section 5): `file` picked from the author's own `.ogg`
files and copied to `maps/<id>/speech/<file>.ogg` (the name must be unique across all sounds, so the editor suggests the mission id as
a prefix and `check_mod`'s sound lookup confirms it); `duration` read from the OGG: the last page's granule position over the
sample rate of the identification header, a small pure reader with no new dependency (PROVISIONAL: Vorbis only, no chained
streams; requirement 9 would make it the engine's); `face` from the overlay's `hud/sprites/TalkingHead_Character_*` folders; `color`
as a colour picker. `messages.<ID>` and `ship_names.<ID>` as text rows. An event that no rule plays and a played event with no entry
are the checker's findings.
No `.ogg` is committed for tests: `tests/hygiene.rs` forbids media outside an own-art folder, so the reader's test builds Ogg pages
in code.
Engine needs: 9 (optional).

### P8. Templates (M)

Reuses: `do_group`, T2's rule operations, P3's geometry writers.
New: a template is a data file (our own JSON, no game content): parameters (a name, a group, a volume, a speech list ...) and the
rules, tasks and geometry it writes with those parameters substituted. Expanding is one undo step and produces ordinary rules that
stay editable; nothing remembers the template afterwards. First the three blocks the roadmap names (dialogue: say a line, wait
until done, next; cutscene: `NIS Start`, teleports and waits, `NIS End`; failsafe: `OP Any Of` and `OP Not` conditions switching
rules with `OP Set Rule Active`), then "reach a point", "ambush on arrival" (this is `gate-ambush`'s rule 2), "escort to a gate",
"sink a group to win". Each template has a test: expand into an empty mission, `check_mission` gives no errors.
Where the files live is an open question (requirement 10): the engine (so scripts use the same ones) or the editor's `templates/`.

### P9. Campaign order (S)

Reuses: the `mod.json` document and `form/`.
New: the mod's missions as a list the author drags, written to `mod.json` `missions` (`modding.rs:292-294`); unlisted missions
follow by id, shown greyed below (`campaign.rs:349-375`); a retail id is shown as "replaces the retail mission, not listed"; an id
with no `mission.json` is `check_mod`'s warning (`check_mod.rs:165-169`). Campaign counters (`OP Set Campaign Counter`) carry to the
next mission of the same mod, so a counter read but set only by an earlier mission is shown as "set by <mission>" (read from the
other missions' documents).

### P10. Play-test from here, and the report (M)

Reuses: `Launch`, `playtest_panel`, `spawn_logged`, `check_dir` (unsaved edits played from a copy).
New: `Launch` gains `fire_rules` (`--fire-rule "<rule>"`, repeatable, in order), `start_at` (`--start-at x,y`), `rule_trace`
(`--rule-trace`), and an autoplay mode; all exist in the engine (`lib.rs:275-276`, game `cli.rs:26`, `docs/missions.md` section 10).
"Play from this rule" on a rule fires it and, by default, the rules before it that set the flags, counters and tasks it reads (the
reference walk of P3; the author can untick them); "start here" on the plan sets `--start-at`.
**The report (which rules fired, which never did): needs an engine feature.** Today the engine keeps per rule `fired` and `ran`
(private, `state.rs:40-43`) and `Script::never_fired()` (public, `state.rs:423-425`), but writes them only as one log line at the end
of an unattended autoplay run (`PROCYON_AUTOPLAY=<seconds>` with `--script mission`, `smoke/mod.rs:259-295`: `autoplay end
outcome=.. never_fired=[..]`) and in the console's `rules` command. Until requirement 5, the editor offers "Autoplay report" (that
autoplay run, its stderr parsed; PROVISIONAL: parsing a log line) and shows the never-fired rules in the rule list. A human
play-test has no report until the engine writes one.
Engine needs: 5.

### P11. A retail mission as a diff (M)

Reuses: P1, `which_mod`, keyed saves, `compare` / revert.
New: open `mission01`..`mission13` or a tutorial from the import (never copied), edit, save into a mod as `extends` plus keyed
entries (`docs/missions.md` section 1 now says a diff is fine, engine `docs/missions.md:7-8`: backlog B5's rewording is done). The
mod's file is `maps/missionNN/mission.json` extending the same id in place, so the retail campaign plays it (custom mission lists
leave retail ids out, `campaign.rs:366`). Play-test with `--map missionNN` (PROVISIONAL: `--map` with a retail mission id is read
from `is_mission`, `campaign.rs:278-280`, not tried).
Limits the editor shows, from the engine's merge (`modding.rs:154-199`): a rule the mod adds is APPENDED to the rule list (no
"insert after"), so it runs after all retail rules; a renamed retail rule is a removal plus an append; changing one action of a
retail rule restates that rule's whole `actions` list (arrays inside an entry replace); a retail list with two rules of one name,
or a rule with no name, makes the engine skip the whole file (`modding.rs:168-175`; PROVISIONAL whether any retail mission has
one: no import here), and the editor then refuses keyed saves for that mission and says why. Requirement 2 covers these.
Engine needs: 2, 3.

### P12. The gate run (M)

See "Phase 5 gate" below.

## Engine requirements for Phase 5

Checked against the pin `f91542f`. "Missing" means not found in the source or the docs.

1. **Schemas and typed documents for `mission.json` and `media.json`** (and `script.json`, still open from Phase 4 requirement 2),
   with a `format` key each. Missing: `authoring::schemas()` lists ship, guns, bullets, crew nodes, mod, map (`authoring.rs:110-114`);
   the game reads both files as `Value` (`campaign.rs:400-404`). Why: hover texts and unknown-key warnings for the top-level and
   ship/task/point keys (the forms otherwise spell them in `mission/doc.rs` with no check), the v0.3 freeze, and a typed load the
   gate can compare (below).
2. **Keyed lists and removal in mission files.** `keyed_defaults` keys `ships`, `rules`, `points` (`modding.rs:211`). Also wanted:
   `tasks` by `Name`, `environment.nebulae` by `New Nebula Name`, `environment.animals` by `group`, `map_text` by `name`; removal of an
   object key in an `extends` file (`volumes`, `groups`, `paths`, `point_sets`, `speech`); a position for an added keyed entry
   (`"$after": "<name>"` or similar), because rules run in order and new ones are appended (`modding.rs:192-195`). Why: P1, P4, P11
   on retail missions without restating retail lists.
3. **The keyed rule public in `authoring`** (`keyed_lists` / `extend` / `merge_keyed`): done on the engine branch
   `claude/authoring-expose` (`ae444cb`), not merged. Why: `project::keyed`'s PROVISIONAL copy goes when the pin moves.
4. **Rule schema: an `area` and a one-line `summary` per Type** (and a description per field). Missing: `to_json` gives `name`, `role`,
   `origin`, `handler`, `campaign_only`, `fields` only (`schema.rs:969-977`); the areas exist only as headings of `docs/rule-types.md`.
   Why: the searchable, grouped add menu (P5) without an editor-side table.
5. **A play-test report file:** `--rule-report <file>` (name PROVISIONAL) writing JSON when the mission ends or the game quits: per rule
   (by index and name) whether it fired, how many times and first at which game time, its state at the end (inactive, waiting in an
   `OP Wait`), tasks with their end state, the outcome. Missing: the data is in `Script` (`fired`, `ran`, `state.rs:40-43`;
   `never_fired`, `state.rs:423`) but leaves the game only as an autoplay log line (`smoke/mod.rs:294`). Why: P10's report for a human
   play-test, without parsing logs.
6. **Name references as data:** `authoring::references(mission, kind, name)` (or `visit_fields` public, `check.rs:263-264`, it is
   `pub(crate)`) returning a JSON pointer per field that names `name` as `kind`, `Group,Unit` split. Why: rename that follows
   references (P3), the map beside the rules (P6), "play from this rule" prerequisites (P10), without the editor re-walking rules.
7. **Ship class list in `authoring`** (the roadmap's "listing ids"): the classes a `ships[].type` may name, from the overlay.
   Missing; `check_mod` checks a type (`check_mod.rs:966` test) but lists none. Why: P2's type picker (the editor scans
   `ships/*/ship.json` meanwhile).
8. **In-process mission check with lookups, and task findings by index.** `check_mission` does not look up ship types, sounds or
   pictures (`authoring.rs:192-194`); `rules::cli::check_map_in(dir, lookups)` does (`rules/cli.rs:117`) but is not in `authoring`
   and needs files. Task findings carry `rule: "task 'X'"` and no task index (`check.rs:35`). Why: a wrong ship `type` and a missing
   `.ogg` shown on the field while typing (P2, P7), task findings on the task row (P4).
9. **Sound duration** (optional): `authoring::sound_duration(path)`; the engine decodes Vorbis through Bevy's `vorbis` feature
   (`game/Cargo.toml:10`). Why: P7 reads `duration` the way the game would; without it the editor's own Ogg header reader is used.
10. **Templates as data** (decision first): a template format and the first files in the engine (with an expander in `authoring`),
    if templates are to be shared with authoring scripts (roadmap: "a template is a data file, so the community and authoring
    scripts can use the same ones"). Why: P8; otherwise they live in the editor.
11. **Unique ids across ships, objects, animals and mines** (Phase 4 requirement 10, still missing). Why: groups address ids (P2).
12. **Nebula agreement check** (Phase 4 requirement 5, still missing). Why: a volume that is a nebula's polygon (P3).

Present and used as is: `check_mission` (with `nested`), `accepted_names`, `defined_names`, `known_names`, `rule_schema` (with
`rule.keys` and `task.keys`), `merge_rule`, `extends_parent`, `overlay_into`, `read_merged`, `to_json_string`, `check_mod`;
`--map`, `--mod`, `--only-mod`, `--fire-rule`, `--start-at`, `--rule-trace`, `--why`, `--script mission` with `PROCYON_AUTOPLAY`.

## Before the freeze, and after

| Stable whatever the format does | Re-check at the freeze (or needs the engine) |
| --- | --- |
| P3 plan math and polygon editing (reuse), P5 list, add menu, widgets by `kind`, findings by location, P8 expander, P10 launch flags, P7 Ogg reader, undo and diff (reuse) | every top-level key and the ship, task, point, point-set and media entry keys (`mission/doc.rs`), keyed tasks and removal (2), areas (4), report (5), references (6), templates' home (10) |

Rule: nothing outside `src/mission/doc.rs` (mission and media keys) and `src/map/doc.rs` (map keys) spells a key. Rule Types and
their fields are never spelt: they come from `rule_schema()`.

## Roadmap backlog in Phase 5

- **B1, show what the engine fills in.** Rule envelope defaults (`Run Once` and `Is Active` true) and field defaults ("reads as 0")
  come from the rule schema and show greyed, not as empty boxes; the human's ship (stand-in cutter when none) shown in P2.
- **B2, retail or extension.** Every Type carries `origin` and `campaign_only` (already in the schema); engine-added mission keys
  (`victory`, `complete_when`, `fail_when`, `required`, counters) get the extension badge from the schema once requirement 1 adds
  `x-origin`; until then from the rule schema's `task.keys` origins (`schema.rs:871`).
- **B3, more checks.** On the field: unknown names with the suggestion (P5), ship `type` (8), missing `.ogg` (8), ids not unique (11),
  nebula agreement (12).
- **B4, skirmish.** `script.json` keeps the Phase 4 T10 section; it now uses the P5 rule list (T5), with `campaign_only` hidden.
- **B5, retail as a starting point.** P11.
- **B6, retail stays intact.** "Which mod?" on the first edit of a retail mission, the one write guard, compare and revert per rule
  and per field (a rule is a keyed entry, so revert removes it from the diff).

## Tomorrow: ordered task list for a multi-agent push

Each task owns its files; "touches" means an existing file it may edit, nothing else. PAR tasks have disjoint files and can run at
once; the others wait on the numbers named. T0 makes the module, the tab and one stub `show` per section, so that later tasks fill
their own file and nobody edits the tab's file but T5. New code goes under `src/mission/` (module `mission`, which today is the
spike's `src/mission.rs`: T0 turns it into a folder).

| # | Task | Size | Depends | Files (new unless marked) |
| --- | --- | --- | --- | --- |
| T0 | Skeleton: `src/mission.rs` becomes `src/mission/mod.rs` with the spike's `Mission` in `src/mission/spike.rs`; `src/mission/doc.rs`, the ONE place naming `mission.json` and `media.json` keys (accessors and pointer builders for players, alliances, player_start, ships, groups, volumes, paths, points, environment.point_sets / nebulae / animals, tasks, rules, map_text, victory keys, media tables); the mission parts of `src/map/doc.rs` moved there and re-exported; the "Missions" tab in `src/missions_panel.rs` with sections calling stub `show` functions of `ships`, `plan`, `tasks`, `rules_view`, `speech`, `campaign`, `report` | S | first, alone | `src/mission/{mod,spike,doc}.rs`, stub files, `src/missions_panel.rs`; touches `src/map/doc.rs`, `src/checks.rs`, `src/mission_panel.rs` (imports), `src/main.rs` (one tab line) |
| T1 | Mission documents: list (mod, retail, "retail, changed by the mod"), open the three files, new mission (one group step, `mod.json` `missions` appended), refusals for unkeyed lists on an extending mission; tests through `overlay_into` + `read_merged` like `tests/map_project.rs` | M | T0 | `src/mission/project.rs`, `tests/mission_project.rs` |
| T2 | Lift the rule and media operations out of `map/skirmish.rs` into file-generic functions (add, delete, rename, flags, add/remove/replace/move item, move rule, duplicate rule; media tables), plus the pure reference walk (`refs`: every field naming a name of a kind, nested and task lists included, `Group,Unit` split; PROVISIONAL until requirement 6); skirmish switched to them, its tests unchanged | M | T0; PAR with T1, T3, T4 | `src/mission/rules.rs`, `src/mission/media.rs`, `src/mission/refs.rs`, `tests/mission_rules.rs`; touches `src/map/skirmish.rs` (calls only) |
| T3 | Field widgets v2 in `schema_form`: `accepted_names` instead of `defined_names` + `known_names`; type-ahead dropdowns; free text plus "new" for flag, timer, counter; `Group,Unit`; nested `condition_list` / `condition`; range on numbers; finding under the field with the suggestion button; returns `Edit`s; headless egui tests | M | none (PAR from the start) | `src/schema_form.rs`, `tests/schema_form.rs` |
| T4 | Rule Type catalogue for the add menu: search over names and field names, areas (PROVISIONAL table from `docs/rule-types.md` headings, test: every schema Type in exactly one area), origin and `campaign_only` filters | S | T0; PAR | `src/mission/catalogue.rs`, `tests/mission_catalogue.rs` |
| T5 | The rule list UI: list, filter, select, header toggles, conditions and actions with T3 widgets and the T4 menu, drag reorder of rules and items, duplicate, inline findings (`for_item` with `nested`), problems panel focus to a rule field; the Phase 4 skirmish section switched to it | L | T1, T2, T3, T4 | `src/mission/rules_view.rs`; touches `src/missions_panel.rs` (sole owner after T0), `src/checks.rs`, `src/map/skirmish.rs` (draw call) |
| T6 | Players, alliances, `player_start`, ships (type picker by overlay scan, `primary` exclusive, unique names), groups (members across ships, map objects, animals; id allocation shared with `map::players`) | M | T1; PAR with T5, T7 | `src/mission/ships.rs`, `src/mission/groups.rs`, `tests/mission_ships.rs` |
| T7 | Mission plan: map scene greyed underneath, ships and start markers, volumes (polygon edit reused from `map::nebula`), paths, points, point sets, map text; add, delete, rename following references (T2 `refs`) as one group step | L | T1, T2; PAR with T5, T6 | `src/mission/plan.rs`, `src/mission/geometry.rs`, `tests/mission_plan.rs` |
| T8 | Tasks and the end: task list and form, `complete_when` / `fail_when` with the T3 nested lists, `victory` radio, end messages | S | T1, T3; PAR | `src/mission/tasks.rs`, `tests/mission_tasks.rs` |
| T9 | The map beside the rules: rule selection highlights its geometry and groups on the plan, a click on geometry lists the rules and tasks that read it | M | T5, T7 (same agent as T7 if possible) | `src/mission/plan.rs` (edit), `src/mission/rules_view.rs` (one hook) |
| T10 | Speech and messages: tables beside the rules, "edit line" / "new line" from a speech field, `.ogg` copy through the guard, Ogg duration reader (pure, Ogg pages built in the test), face picker | M | T2; PAR | `src/mission/speech.rs`, `src/mission/ogg.rs`, `tests/mission_speech.rs` |
| T11 | Templates: format, expander (one group step), dialogue / cutscene / failsafe, then the four mission templates; each expanded into an empty mission checks clean | M | T2, T3; PAR | `src/mission/templates.rs`, `templates/*.json` (our own text), `tests/mission_templates.rs` |
| T12 | Campaign order: drag list into `mod.json` `missions`, unlisted and retail ids shown, campaign counters' "set by" | S | T1; PAR | `src/mission/campaign.rs` |
| T13 | Launch from here: `fire_rules`, `start_at`, `rule_trace`, autoplay (`PROCYON_AUTOPLAY` + `--script mission`), the `autoplay end` line parsed into fired / never fired (PROVISIONAL), shown in the rule list through the `report` stub | M | none for `launch.rs` (PAR from the start); the panel part after T0 | `src/launch.rs` (edit), `tests/launch_args.rs` (edit), `src/mission/report.rs` |
| T14 | Retail mission as a diff: open from the import, keyed saves, the P11 limits as messages (append order, duplicate or missing rule names refuse), revert per rule; tests on a synthetic retail-like folder in a temp dir (never game content) | M | T1, T5 | `src/mission/project.rs` (after T1, same agent), `tests/mission_retail.rs` |
| T15 | Engine side (a separate checkout, with the maintainer's go-ahead): requirements 5, 2, 1, 4 first, then 6 and 8; merge `ae444cb` (3) and move the pin | L | PAR with everything | in the engine, not here; then `Cargo.toml` pin (maintainer) |
| T16 | Retire the spike: `src/mission/spike.rs`, `src/mission_panel.rs`, the "Mission (spike)" tab; `checks.rs` reads project documents | S | T5 | delete those; touches `src/main.rs`, `src/lib.rs`, `src/checks.rs` |
| T17 | Gate: the recipe test and the rehearsal by hand (below); write what held in this file's "Seen running" | M | last | `tests/mission_gate.rs`, `docs/phase5.md` |

**Can start at once, in parallel:** T0 (alone in its files), T3 (only `schema_form.rs`), T13's `launch.rs` part, T15 (engine).
Right after T0: T1, T2, T4. After T1: T6, T8 (also needs T3), T12; after T1 and T2: T7, T10, T11.

**Merge points.** `src/main.rs`: T0 adds the tab, T16 removes the spike's; nobody else. `src/missions_panel.rs`: T0 writes it with stub
calls, T5 owns it after; other tasks fill their own module's `show`. `src/mission/doc.rs`: T0 writes it; a later task that needs a
new key adds one accessor there in its own commit (small, append-only, rebase on conflict). `src/map/doc.rs`: only T0 (the move).
`src/map/skirmish.rs`: T2 (calls), then T5 (draw call). `src/schema_form.rs`: T3 only. `src/checks.rs`: T0 (imports), T5, T16 in that
order. `src/mission/project.rs`: T1 then T14. `src/mission/plan.rs`: T7 then T9. `src/project/keyed.rs` stays as is until the engine's
requirement 2 or 3 lands (then one task updates the copy and `tests/keyed_engine.rs`).
After each task: `tools/check.sh` prints `GATE OK` before the commit (AGENTS.md); commits per task; no push without the maintainer.

## Phase 5 gate, and how it is checked

Roadmap: **the worked example `gate-ambush` is rebuilt in the editor from an empty folder and its file matches the hand-written
one in meaning.** The hand-written files are the engine's `docs/examples/gate-ambush.mission.json` and the `map.json`, `media.json`
and `mod.json` of `docs/missions.md` section 1, already in this repository as `tests/fixtures/source/` (and as a mod in
`tests/fixtures/gates/`, byte-equal to the engine's example).

1. **By hand (T17, needs a display):** a new mod `gates` in an empty folder; "New mission" `gate_ambush`; in the Maps tab the
   name, description, sky, `world_half` and one `Island04`; in the Missions tab players `Hero`, `Raiders`; the start; three ships
   (`Dauntless` primary); group `RaiderShips`; volume `Gate Area`; point `The Gate`; the two tasks with their conditions; `victory:
   objectives`; the three rules (rule 2 from the "ambush on arrival" template, then edited). No JSON file opened.
2. **The same steps as a test** (`tests/mission_gate.rs`, headless): it calls only the functions the buttons call (`mission::project`,
   `ships`, `geometry`, `tasks`, `rules`), in that order, on an empty temp folder, then saves.
3. **"Matches in meaning", checked by the test:** each written file is read back the way the game reads it (`overlay_into` over no
   import, then `read_merged`) and compared with the hand-written one after one normalisation: object key order ignored; numbers
   equal as f64 (`4` and `4.0`); a key left out equals the schema's default where the rule schema gives one (`Run Once`, `Is Active`
   true), and an empty object or list equals a missing one only for the keys the engine reads as empty when missing (PROVISIONAL list:
   `alliances`, `groups`, `environment.point_sets`, `volumes`, `paths`, `points`, kept in the test with the engine line that reads
   each); list order is significant everywhere else (rules, actions, conditions, ships, tasks). Then two engine views must agree:
   `check_mission` gives the same findings for both (none), and `defined_names` / `accepted_names` are equal. When requirement 1
   gives a typed `MissionDoc`, the test compares the typed values instead of the normalised JSON.
4. `--check-mod <mod> --json` clean, and `<game> --mod <mod> --only-mod --map gate_ambush` plays to a win (by hand, with the import);
   with requirement 5, the report shows all three rules fired.

What stays unverified until a machine with an import and a display runs it: how any of it looks, drag and drop in the lists, the
check's speed on a large mission, retail missions as diffs (duplicate rule names, order), `--map` with a retail mission id.

## Risks

1. Keyed merge limits on retail missions (append order, whole `actions` lists, duplicate names skipping a file): P11 may be much
   weaker than P1 suggests until requirement 2. Mitigation: the editor says so per rule; the maintainer decides how far retail
   diffs go for v0.3.
2. No mission or media schema (1): forms spell keys by hand in `mission/doc.rs`; a format change touches that file only.
3. The play-test report rests on a log line until requirement 5; a change of the line's wording breaks it silently (a test pins
   the parser to the engine's format string only by copy, PROVISIONAL).
4. Rule list performance: `check_mission` on every edit and a few hundred egui rows (a 433-item generated mission) are not measured.
5. Templates' home (10) undecided: built in the editor first, they may have to move.
6. The reference walk (P3, P6) restates the engine's private `visit_fields` until requirement 6; a schema kind added to the engine
   without the walk's update would rename too little (a test feeds every schema kind through it).
