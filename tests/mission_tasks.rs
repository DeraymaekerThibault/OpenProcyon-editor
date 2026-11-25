//! The task and end edits of `mission::tasks` on synthetic missions (no game content): add, rename with the references, conditions,
//! victory and messages, each one undo step, and the save as a diff over a retail mission.

use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::project::{mission_rel, new_mission, open_mission};
use openprocyon_editor::mission::tasks;
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn setup(name: &str) -> (PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-mission-tasks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First"}));
    put(&assets, "maps/mission01/mission.json", &json!({"tasks": [{"Name": "T1", "text": "Go", "Active": true}], "rules": [], "ships": []}));
    put(&assets, "maps/mission01/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    (root, Project::open(&m, Some(assets)).unwrap())
}

fn mission<'a>(p: &'a Project, id: &str) -> &'a Value {
    &p.doc(&mission_rel(id).unwrap()).unwrap().edited
}

#[test]
fn tasks_are_added_edited_and_deleted_one_step_each() {
    let (root, mut p) = setup("basic");
    new_mission(&mut p, "own", None).unwrap();
    let start = mission(&p, "own").clone();
    assert_eq!(tasks::add_task(&mut p, "own", " Scout ").unwrap(), 0);
    assert!(tasks::add_task(&mut p, "own", "Scout").is_err() && tasks::add_task(&mut p, "own", "  ").is_err());
    tasks::set_text(&mut p, "own", 0, "Find the gate").unwrap();
    tasks::set_text(&mut p, "own", 0, "Find the gate now").unwrap();
    tasks::set_active(&mut p, "own", 0, false).unwrap();
    tasks::set_required(&mut p, "own", 0, true).unwrap();
    let t = &mission(&p, "own")["tasks"][0];
    assert_eq!(t["Name"], "Scout");
    assert_eq!(t["text"], "Find the gate now");
    assert!(!mdoc::task_active(t) && mdoc::task_required(t));
    assert!(t["Active"].is_boolean() && t["required"].is_boolean());
    assert!(tasks::set_text(&mut p, "own", 5, "x").is_err());
    tasks::delete_task(&mut p, "own", 0).unwrap();
    assert!(mdoc::tasks(mission(&p, "own")).is_empty());
    // add, typing (two edits, one step), active, required, delete: five steps over the mission's creation
    for _ in 0..5 {
        assert!(p.undo().is_some());
    }
    assert_eq!(mission(&p, "own"), &start);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn renaming_a_task_renames_what_names_it_in_one_step() {
    let (root, mut p) = setup("rename");
    new_mission(&mut p, "own", None).unwrap();
    tasks::add_task(&mut p, "own", "Scout").unwrap();
    tasks::add_task(&mut p, "own", "Other").unwrap();
    let rel = mission_rel("own").unwrap();
    let rule = json!({"name": "R", "conditions": [], "actions": [{"Type": "Set Objective Task Active State", "Objective Task": "Scout", "Active State": "TRUE"}]});
    openprocyon_editor::mission::rules::append_rule(&mut p, &openprocyon_editor::mission::rules::Target::mission(&rel), rule, "Add rule").unwrap();
    tasks::add_condition(&mut p, "own", 1, mdoc::TASK_COMPLETE_WHEN, json!({"Type": "OP Task State", "Objective Task": "Scout", "State": "Complete"})).unwrap();
    let before = mission(&p, "own").clone();
    let n = tasks::rename_task(&mut p, "own", 0, "Recon").unwrap();
    assert_eq!(n, 2);
    let m = mission(&p, "own");
    assert_eq!(m["tasks"][0]["Name"], "Recon");
    assert_eq!(m["rules"][0]["actions"][0]["Objective Task"], "Recon");
    assert_eq!(m["tasks"][1]["complete_when"][0]["Objective Task"], "Recon");
    assert!(tasks::rename_task(&mut p, "own", 0, "Other").is_err() && tasks::rename_task(&mut p, "own", 0, " ").is_err());
    assert_eq!(p.undo_label(), Some("Rename task Scout"));
    p.undo().unwrap();
    assert_eq!(mission(&p, "own"), &before);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn conditions_are_added_removed_and_edited() {
    let (root, mut p) = setup("cond");
    new_mission(&mut p, "own", None).unwrap();
    tasks::add_task(&mut p, "own", "T").unwrap();
    let c = json!({"Type": "OP Flag Condition", "Flag Name": "a", "Boolean": "TRUE"});
    assert_eq!(tasks::add_condition(&mut p, "own", 0, mdoc::TASK_FAIL_WHEN, c.clone()).unwrap(), 0);
    assert_eq!(tasks::add_condition(&mut p, "own", 0, mdoc::TASK_FAIL_WHEN, c).unwrap(), 1);
    assert!(tasks::add_condition(&mut p, "own", 0, "text", json!({})).is_err());
    let base = mdoc::task_condition_pointer(0, mdoc::TASK_FAIL_WHEN, 1);
    tasks::apply_condition_edits(&mut p, "own", 0, mdoc::TASK_FAIL_WHEN, 1, vec![openprocyon_editor::edit::Edit::set(format!("{base}/Flag Name"), json!("b"))]).unwrap();
    assert_eq!(mission(&p, "own")["tasks"][0]["fail_when"][1]["Flag Name"], "b");
    tasks::remove_condition(&mut p, "own", 0, mdoc::TASK_FAIL_WHEN, 0).unwrap();
    assert_eq!(mdoc::task_conditions(&mdoc::tasks(mission(&p, "own"))[0], mdoc::TASK_FAIL_WHEN).len(), 1);
    assert!(tasks::remove_condition(&mut p, "own", 0, mdoc::TASK_FAIL_WHEN, 9).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn victory_and_end_messages() {
    let (root, mut p) = setup("victory");
    new_mission(&mut p, "own", None).unwrap();
    assert!(!tasks::victory_on(mission(&p, "own")));
    tasks::set_victory(&mut p, "own", true).unwrap();
    assert!(tasks::victory_on(mission(&p, "own")));
    tasks::set_end_message(&mut p, "own", mdoc::KEY_VICTORY_MESSAGE, Some("WIN")).unwrap();
    tasks::set_end_message(&mut p, "own", mdoc::KEY_DEFEAT_MESSAGE, Some("LOSE")).unwrap();
    assert_eq!(mdoc::victory_message(mission(&p, "own")), Some("WIN"));
    assert!(tasks::set_end_message(&mut p, "own", "text", Some("x")).is_err());
    tasks::set_end_message(&mut p, "own", mdoc::KEY_VICTORY_MESSAGE, None).unwrap();
    tasks::set_victory(&mut p, "own", false).unwrap();
    let m = mission(&p, "own");
    assert!(m.get(mdoc::KEY_VICTORY).is_none() && m.get(mdoc::KEY_VICTORY_MESSAGE).is_none());
    assert_eq!(mdoc::defeat_message(m), Some("LOSE"));
    // clearing what is not there changes nothing (no step)
    let label = p.undo_label().map(str::to_string);
    tasks::set_victory(&mut p, "own", false).unwrap();
    assert_eq!(p.undo_label().map(str::to_string), label);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_retail_mission_saves_the_end_as_a_diff_and_keeps_the_task_list_out_of_it() {
    let (root, mut p) = setup("retail");
    open_mission(&mut p, "mission01").unwrap();
    // `tasks` is keyed by `Name` (engine e2f096c): an added or deleted task is a keyed entry; see tests/mission_retail.rs
    tasks::add_task(&mut p, "mission01", "New").unwrap();
    tasks::delete_task(&mut p, "mission01", 1).unwrap();
    tasks::set_victory(&mut p, "mission01", true).unwrap();
    tasks::set_end_message(&mut p, "mission01", mdoc::KEY_VICTORY_MESSAGE, Some("WIN")).unwrap();
    let report = p.save();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let saved: Value = serde_json::from_slice(&std::fs::read(root.join("mod/maps/mission01/mission.json")).unwrap()).unwrap();
    assert_eq!(saved["victory"], "objectives");
    assert_eq!(saved["victory_message"], "WIN");
    assert!(saved.get("tasks").is_none() && saved.get("ships").is_none(), "{saved}");
    let _ = std::fs::remove_dir_all(root);
}
