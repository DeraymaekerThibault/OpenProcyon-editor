//! T17 (docs/phase5.md, "Phase 5 gate"): the worked example `gate-ambush` rebuilt from an empty folder with the calls the editor's
//! buttons make, saved, read back the way the game reads it (`authoring::overlay_into` over no import, then `read_merged`) and compared
//! "in meaning" with the hand-written files of `tests/fixtures/source/` (the engine's `docs/examples/gate-ambush.mission.json` and the
//! `map.json`, `media.json`, `mod.json` of `docs/missions.md` section 1): object key order ignored, numbers equal by value, a key
//! left out equal to the rule schema's default, an empty list or object equal to a missing one only for the keys of [`EMPTY_IS_MISSING`];
//! list order significant. Then `check_mission` must give the same findings and `defined_names` / `accepted_names` must be equal.
//!
//! Every step below names the button it stands for. There are no known gaps left: the file the editor writes has no difference from
//! the example beyond the normalisation above (a missing scenery `player` and a missing ship `rot` read as the engine reads them, see
//! `is_default_of`). Synthetic and project-own data only; the temp folders are removed when the test ends.

use openprocyon::authoring::{accepted_names, check_mission, defined_names, overlay_into, read_merged};
use openprocyon_editor::edit::Edit;
use openprocyon_editor::map::catalogue::SceneryType;
use openprocyon_editor::map::doc::Array;
use openprocyon_editor::map::project as maps;
use openprocyon_editor::map::settings::{self, Text};
use openprocyon_editor::map::view2d;
use openprocyon_editor::map_edit::orient::IDENTITY;
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::geometry::{self as geo, Item, Kind};
use openprocyon_editor::mission::project as mproject;
use openprocyon_editor::mission::rules::{self, Target};
use openprocyon_editor::mission::{groups, ships, tasks};
use openprocyon_editor::project::{NewMod, Project};
use openprocyon_editor::schema_form::{self as sf, Schema};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ID: &str = "gate_ambush";

/// Keys an empty list or object equals a missing one for (PROVISIONAL, docs/phase5.md gate step 3): the engine reads each as empty
/// when it is missing (docs/missions.md section 3). The engine lines were not re-checked for this test.
const EMPTY_IS_MISSING: &[&str] = &["/alliances", "/groups", "/environment/point_sets", "/volumes", "/paths", "/points", "/environment"];

fn fixture(rel: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/source").join(rel);
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

/// Every folder the test made, removed by [`Cleanup`] when the test ends (also when it panics).
static MADE: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

struct Cleanup;

impl Drop for Cleanup {
    fn drop(&mut self) {
        for d in MADE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).drain(..) {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

fn made(d: PathBuf) -> PathBuf {
    MADE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(d.clone());
    d
}

fn temp(name: &str) -> PathBuf {
    let d = made(std::env::temp_dir().join(format!("ope-gate-{name}-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A step of the by-hand rehearsal: panics naming it when the editor's function refuses.
fn step<T>(what: &str, r: Result<T, String>) -> T {
    r.unwrap_or_else(|e| panic!("GATE STEP FAILED: {what}: the editor's function refused: {e}"))
}

// ---- the editor's calls, in the order of the rehearsal ----

/// What the field widgets of `schema_form` write for a field of Type `ty`: a bool as "TRUE"/"FALSE", a number as a number, text as text.
struct Forms {
    schema: Schema,
}

impl Forms {
    /// "+ Add condition / action" in the catalogue menu: `new_item` of the Type, then each field typed or picked.
    fn item(&self, is_condition: bool, ty: &str, fields: &[(&str, Value)]) -> Value {
        let def = self.schema.find(is_condition, ty).unwrap_or_else(|| panic!("GATE STEP FAILED: '{ty}' is not a Type of the rule schema"));
        let mut item = sf::new_item(def);
        for (name, v) in fields {
            let f = def.fields.iter().find(|f| f.name == *name).unwrap_or_else(|| panic!("GATE STEP FAILED: Type '{ty}' has no field '{name}'"));
            item[*name] = match (f.kind.as_str(), v) {
                ("bool", Value::Bool(b)) => sf::bool_value(*b),
                _ => v.clone(),
            };
        }
        item
    }
}

fn build(dir: &Path) -> Project {
    let forms = Forms { schema: Schema::engine() };
    // 1. "New mod..." in an empty folder.
    let (mut p, created) = step("New mod", Project::create_with(dir, None, &NewMod { id: Some("gates".into()), ..Default::default() }));
    assert!(created);
    // The Mod tab's form: name, version, category, description.
    let edits = vec![
        Edit::set("/name", json!("Gates")),
        Edit::set("/version", json!("0.1")),
        Edit::set("/category", json!("Missions")),
        Edit::set("/description", json!("A one-mission test.")),
    ];
    step("Mod tab: name, version, category, description", p.apply_form("mod.json", "Edit mod.json", edits, false));

    // 2. Missions tab: "New mission" gate_ambush.
    step("New mission", mproject::new_mission(&mut p, ID, None));
    let rel = mproject::mission_rel(ID).unwrap();
    let t = Target::mission(&rel);

    // 3. Maps tab: name, description, sky, world_half, one Island04 placed from the catalogue.
    step("Maps tab: name", settings::set_text(&mut p, ID, Text::Name, "Gate Ambush", false));
    step("Maps tab: description", settings::set_text(&mut p, ID, Text::Description, "Two raiders wait at the gate.", false));
    step("Maps tab: sky", settings::set_sky(&mut p, ID, Some("skybox_22_Layer1")));
    step("Maps tab: world_half", settings::set_world_half(&mut p, ID, 1200.0, false));
    let island = SceneryType { name: "Island04".into(), scenes: vec!["Island04".into()], ..Default::default() };
    step("Maps tab: place Island04", maps::add_entry(&mut p, ID, Array::Objects, view2d::scenery_entry(&island, [-300.0, 300.0])));

    // 4. Missions tab, players: Hero, Raiders; the human's start.
    step("Add player Hero", ships::add_player(&mut p, ID, "Hero"));
    step("Add player Raiders", ships::add_player(&mut p, ID, "Raiders"));
    step("Human is Hero", ships::set_start_player(&mut p, ID, "Hero"));
    step("Start position", ships::set_start_vec(&mut p, ID, false, [0.0, 0.0, 0.0], false));
    step("Start forward", ships::set_start_vec(&mut p, ID, true, [0.0, 1.0, 0.0], false));

    // 5. Three ships; Dauntless primary; the raiders turned to face the gate.
    let dauntless = step("Add ship Dauntless", ships::add_ship(&mut p, ID, "Ship_Navy_Cutter", "Hero"));
    step("Name Dauntless", ships::rename_ship(&mut p, ID, dauntless, "Dauntless"));
    step("Id of Dauntless", ships::set_ship_id(&mut p, ID, dauntless, 10));
    step("Primary", ships::set_primary(&mut p, ID, dauntless, true));
    for (name, pos, ship_id) in [("Raider One", [400.0, 600.0, 0.0], 11), ("Raider Two", [440.0, 620.0, 0.0], 12)] {
        let i = step("Add ship", ships::add_ship(&mut p, ID, "Ship_Pirate_Sloop", "Raiders"));
        step("Name ship", ships::rename_ship(&mut p, ID, i, name));
        step("Id of ship", ships::set_ship_id(&mut p, ID, i, ship_id));
        step("Move ship", ships::set_ship_pos(&mut p, ID, i, pos, false));
        step("Turn ship", geo::turn_items(&mut p, ID, &[Item::Ship(i)], std::f64::consts::PI));
    }

    // 6. Group RaiderShips with the two raiders.
    step("Add group", groups::add_group(&mut p, ID, "RaiderShips"));
    let m = mproject::merged_mission(&p, ID).unwrap().clone();
    for ship in mdoc::ships(&m).iter().filter(|s| mdoc::ship_group(s) == Some("Raiders")) {
        step("Add member", groups::add_member(&mut p, ID, "RaiderShips", mdoc::ship_id(ship).unwrap()));
    }

    // 7. Plan: the volume Gate Area (a new volume is a triangle: one corner is added, then each is dragged to its place) and the point.
    step("Plan: add volume", geo::add(&mut p, ID, Kind::Volume, "Gate Area", [400.0, 600.0]));
    step("Plan: add volume corner", geo::insert_vertex(&mut p, ID, Kind::Volume, "Gate Area", 2));
    let corners = [[300.0, 500.0], [500.0, 500.0], [500.0, 700.0], [300.0, 700.0]];
    for (i, to) in corners.iter().enumerate() {
        let it = Item::Vertex(mdoc::GeoTable::Volumes, "Gate Area".into(), i);
        let now = geo::item_xyz(mproject::merged_mission(&p, ID).unwrap(), &it).unwrap();
        step("Plan: drag corner", geo::move_items(&mut p, ID, &[it], [to[0] - now[0], to[1] - now[1]]));
    }
    step("Plan: add point", geo::add(&mut p, ID, Kind::Point, "The Gate", [400.0, 600.0]));

    // 8. Tasks and the end.
    let reach = step("Add task", tasks::add_task(&mut p, ID, "Reach the gate"));
    step("Task text", tasks::set_text(&mut p, ID, reach, "Sail to the gate."));
    step("Task inactive", tasks::set_active(&mut p, ID, reach, false));
    step("Task required", tasks::set_required(&mut p, ID, reach, true));
    let cond = forms.item(true, "Is Group In Volume", &[("Group Name", json!("Hero")), ("Volume Name", json!("Gate Area")), ("Entire Group", json!(false))]);
    step("Task condition", tasks::add_condition(&mut p, ID, reach, mdoc::TASK_COMPLETE_WHEN, cond));
    let clear = step("Add task", tasks::add_task(&mut p, ID, "Clear the gate"));
    step("Task text", tasks::set_text(&mut p, ID, clear, "Sink both raiders."));
    step("Task inactive", tasks::set_active(&mut p, ID, clear, false));
    step("Task required", tasks::set_required(&mut p, ID, clear, true));
    let cond = forms.item(true, "OP Counter Condition", &[("Counter Name", json!("raiders sunk")), ("Equivalence", json!("Greater Than")), ("Value", json!(1))]);
    step("Task condition", tasks::add_condition(&mut p, ID, clear, mdoc::TASK_COMPLETE_WHEN, cond));
    let cond = forms.item(true, "Group Destroyed", &[("Group Name", json!("Hero"))]);
    step("Task fail condition", tasks::add_condition(&mut p, ID, clear, mdoc::TASK_FAIL_WHEN, cond));
    step("Victory: objectives", tasks::set_victory(&mut p, ID, true));

    // 9. Rules. (T11's templates are not in this tree: rule 2 is built by hand, see the gap list in the docs.)
    let mut rule = |name: &str, once: bool, active: bool, conditions: Vec<Value>, actions: Vec<Value>| {
        let r = step("New rule", rules::add_rule(&mut p, &t));
        step("Rename rule", rules::rename_rule(&mut p, &t, r, name));
        if !once {
            step("Run Once off", rules::set_rule_flag(&mut p, &t, r, mdoc::RULE_RUN_ONCE, false));
        }
        if !active {
            step("Is Active off", rules::set_rule_flag(&mut p, &t, r, mdoc::RULE_IS_ACTIVE, false));
        }
        for c in conditions {
            step("Add condition", rules::add_item(&mut p, &t, r, true, c));
        }
        for a in actions {
            step("Add action", rules::add_item(&mut p, &t, r, false, a));
        }
    };
    rule(
        "Start",
        true,
        true,
        vec![forms.item(true, "World Initialize", &[])],
        vec![
            forms.item(false, "Set Objective Task Active State", &[("Objective Task", json!("Reach the gate")), ("Active State", json!(true))]),
            forms.item(false, "Set Current Objective Point", &[("Objective Point", json!("The Gate"))]),
            forms.item(false, "OP Set Counter", &[("Counter Name", json!("raiders sunk")), ("Value", json!(0))]),
        ],
    );
    rule(
        "Arrive at the gate",
        true,
        true,
        vec![forms.item(true, "Enter Volume", &[("Group Name", json!("Hero")), ("Volume Name", json!("Gate Area"))])],
        vec![
            forms.item(false, "NIS Start", &[]),
            forms.item(false, "NIS Focus camera on Group/Unit", &[("GroupUnit Name", json!("RaiderShips"))]),
            forms.item(false, "OP Wait", &[("Seconds", json!(4))]),
            forms.item(false, "NIS End", &[]),
            forms.item(false, "Set Objective Task Active State", &[("Objective Task", json!("Clear the gate")), ("Active State", json!(true))]),
            forms.item(false, "GroupA to attack GroupB", &[("Attack Group", json!("RaiderShips")), ("Target Group", json!("Hero"))]),
            forms.item(false, "OP Set Rule Active", &[("Rule Name", json!("Count raiders sunk")), ("Boolean", json!(true))]),
        ],
    );
    rule(
        "Count raiders sunk",
        false,
        false,
        vec![forms.item(true, "Player Killed A Object", &[("Player Name", json!("Hero")), ("World Object Type", json!("Ship"))])],
        vec![forms.item(false, "OP Add To Counter", &[("Counter Name", json!("raiders sunk")), ("Value", json!(1))])],
    );

    // 10. Speech and messages: the example has none; media.json is the three empty tables "New mission" made.
    // 11. Save (Ctrl+S).
    let report = p.save();
    assert!(report.errors.is_empty(), "GATE STEP FAILED: Save: {:?}", report.errors);
    p
}

// ---- reading back and comparing ----

/// The engine reads the saved mod over no import: (mission, map, media) as `read_merged` gives them.
fn read_back(mod_dir: &Path, tag: &str) -> (Value, Value, Value) {
    let base = temp(&format!("{tag}-empty-import"));
    let out = made(base.parent().unwrap().join(format!("ope-gate-{tag}-overlay-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&out);
    let report = overlay_into(&base, &[mod_dir.to_path_buf()], &out).unwrap_or_else(|e| panic!("overlay_into: {e}"));
    let skipped: Vec<_> = report.mods.iter().flat_map(|m| m.skipped.clone()).collect();
    assert!(skipped.is_empty(), "the engine skipped files of the mod: {skipped:?}");
    let read = |f: &str| read_merged(&out, &format!("maps/{ID}/{f}.json")).unwrap_or_else(|e| panic!("read_merged {f}: {e}"));
    (read("mission"), read("map"), read("media"))
}

/// The hand-written files, as a mod folder.
fn hand_written(tag: &str) -> PathBuf {
    let d = temp(&format!("{tag}-hand"));
    put(&d, "mod.json", &fixture("mod.json"));
    for f in ["mission", "map", "media"] {
        put(&d, &format!("maps/{ID}/{f}.json"), &fixture(&format!("{f}.json")));
    }
    d
}

fn is_default_of(schema: &Schema, path: &str, obj: &serde_json::Map<String, Value>, key: &str, v: &Value) -> bool {
    // a rule's flags: true when left out (engine schema.rs:832-850, as docs/phase5.md says)
    if (key == "Run Once" || key == "Is Active") && path.matches('/').count() == 2 && path.starts_with("/rules/") {
        return v == &json!(true);
    }
    // a scenery object with no `player` has no owner, as `"player": -1`: the engine reads the owner with `as_u64`, which is `None` for
    // both (game/src/scenario/spawn.rs:289 and :491 at the pinned engine)
    if key == "player" && path.starts_with("/objects/") && path.matches('/').count() == 2 {
        return v == &json!(-1);
    }
    // a ship with no `rot` is upright facing north, as the identity: `game_rotation` takes each missing number from the identity
    // (game/src/scenario/spawn.rs:567-568), and the mission's ships go through it (game/src/campaign.rs:545)
    if key == "rot" && path.starts_with("/ships/") && path.matches('/').count() == 2 {
        return v.as_array().is_some_and(|a| a.len() == 9 && a.iter().zip(IDENTITY).all(|(x, i)| x.as_f64() == Some(i)));
    }
    // a Type's field the schema gives a default for
    let Some(ty) = obj.get("Type").and_then(Value::as_str) else { return false };
    let is_condition = path.contains("/conditions/") || path.contains("/complete_when/") || path.contains("/fail_when/");
    let Some(f) = schema.find(is_condition, ty).and_then(|d| d.fields.iter().find(|f| f.name == key)) else { return false };
    !f.default.is_empty() && f.kind != "condition_list" && f.kind != "condition" && same(v, &sf::default_value(f))
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// An empty list or object, or an object whose every entry is one the engine reads as empty when missing (`environment` holding only
/// an empty `point_sets`: the example has it, `New mission` writes no `environment` until something needs it).
fn is_empty(path: &str, v: &Value) -> bool {
    v.as_array().is_some_and(Vec::is_empty)
        || v.as_object().is_some_and(|o| o.iter().all(|(k, x)| EMPTY_IS_MISSING.contains(&format!("{path}/{k}").as_str()) && is_empty(&format!("{path}/{k}"), x)))
}

/// The differences between the hand-written value `want` and the editor's `got`, as text, under the normalisation of the module doc.
fn diff(schema: &Schema, path: &str, want: &Value, got: &Value, out: &mut Vec<String>) {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            let mut keys: Vec<&String> = w.keys().chain(g.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let here = format!("{path}/{k}");
                match (w.get(k), g.get(k)) {
                    (Some(a), Some(b)) => diff(schema, &here, a, b, out),
                    (Some(a), None) => {
                        let ok = is_default_of(schema, path, w, k, a) || (EMPTY_IS_MISSING.contains(&here.as_str()) && is_empty(&here, a));
                        if !ok {
                            out.push(format!("{here}: the example has {a}, the editor's file has no such key"));
                        }
                    }
                    (None, Some(b)) => {
                        let ok = is_default_of(schema, path, g, k, b) || (EMPTY_IS_MISSING.contains(&here.as_str()) && is_empty(&here, b));
                        if !ok {
                            out.push(format!("{here}: the editor's file has {b}, the example has no such key"));
                        }
                    }
                    (None, None) => {}
                }
            }
        }
        (Value::Array(w), Value::Array(g)) => {
            if w.len() != g.len() {
                out.push(format!("{path}: the example has {} entries, the editor's file {}: {want} / {got}", w.len(), g.len()));
                return;
            }
            for (i, (a, b)) in w.iter().zip(g).enumerate() {
                diff(schema, &format!("{path}/{i}"), a, b, out);
            }
        }
        _ => {
            if !same(want, got) {
                out.push(format!("{path}: the example has {want}, the editor's file has {got}"));
            }
        }
    }
}

fn differences(schema: &Schema, want: &Value, got: &Value) -> Vec<String> {
    let mut out = vec![];
    diff(schema, "", want, got, &mut out);
    out
}

#[test]
fn the_phase_5_gate_gate_ambush_is_rebuilt_from_an_empty_folder() {
    let _cleanup = Cleanup;
    let dir = temp("mod");
    let _project = build(&dir);
    let schema = Schema::engine();

    let (got_m, got_map, got_media) = read_back(&dir, "got");
    let hand = hand_written("want");
    let (want_m, want_map, want_media) = read_back(&hand, "want");

    // mod.json is not overlay content: both are read as written. `format` and `game_version` are keys "New mod" adds that the
    // hand-written file of docs/missions.md does not have (the loader treats them as optional): left out of the comparison.
    let mut got_mod: Value = serde_json::from_slice(&std::fs::read(dir.join("mod.json")).unwrap()).unwrap();
    for k in ["format", "game_version"] {
        got_mod.as_object_mut().unwrap().remove(k);
    }

    let mut found: Vec<String> = vec![];
    for (file, want, got) in [
        ("mod.json", fixture("mod.json"), got_mod),
        ("map.json", want_map.clone(), got_map.clone()),
        ("mission.json", want_m.clone(), got_m.clone()),
        ("media.json", want_media.clone(), got_media.clone()),
    ] {
        found.extend(differences(&schema, &want, &got).into_iter().map(|d| format!("{file}{d}")));
    }

    assert!(found.is_empty(), "differences from gate-ambush:\n{}", found.join("\n"));

    // Two engine views must agree: the checker's findings and the names a field accepts.
    // (the file as the editor saved it, not the closed one: findings and names do not depend on the gaps)
    let findings = |m: &Value, mp: &Value, md: &Value| format!("{:?}", check_mission(m, mp, md, false));
    assert_eq!(findings(&want_m, &want_map, &want_media), "[]", "the example itself checks clean");
    assert_eq!(findings(&got_m, &got_map, &got_media), findings(&want_m, &want_map, &want_media), "check_mission findings differ");
    assert_eq!(defined_names(&got_m, &got_map, &got_media), defined_names(&want_m, &want_map, &want_media), "defined_names differ");
    assert_eq!(accepted_names(&got_m, &got_map, &got_media), accepted_names(&want_m, &want_map, &want_media), "accepted_names differ");
}
