//! T14 (docs/phase5.md P11): a retail mission opened from the import and saved as a diff, on a synthetic "retail" import (no game
//! content). Each case saves through the project, then lets the engine's own `authoring::overlay_into` + `read_merged` build the
//! mission and compares it with what the editor showed. The limits the editor warns about are pinned here as the engine behaves.

use openprocyon::authoring::{overlay_into, read_merged};
use openprocyon_editor::edit::Edit;
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::doc::MissionList;
use openprocyon_editor::mission::project::{add_entry, delete_entry, merged_mission, mission_rel, open_mission, retail_limits};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ID: &str = "mission04";

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn read(root: &Path, rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root.join(rel)).unwrap()).unwrap()
}

/// A made-up retail-like mission: three keyed rules, one with two actions, two ships.
fn retail() -> Value {
    json!({
        "players": ["P1", "P2"],
        "ships": [
            {"id": 1, "type": "A", "group": "g", "name": "One", "pos": [0.0, 0.0, 0.0]},
            {"id": 2, "type": "B", "group": "g", "name": "Two", "pos": [5.0, 5.0, 0.0]}
        ],
        "rules": [
            {"name": "Start", "Type": "On Start", "actions": [{"Type": "Say", "text": "a"}, {"Type": "Say", "text": "b"}]},
            {"name": "Middle", "Type": "On Timer", "actions": []},
            {"name": "End", "Type": "On Timer", "actions": [{"Type": "Win"}]}
        ]
    })
}

struct Rig {
    assets: PathBuf,
    m: PathBuf,
    p: Project,
    rel: String,
}

fn rig(name: &str, mission: Value) -> Rig {
    let root = std::env::temp_dir().join(format!("ope-retail-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, &format!("maps/{ID}/mission.json"), &mission);
    put(&assets, &format!("maps/{ID}/map.json"), &json!({"name": "Retail"}));
    put(&assets, &format!("maps/{ID}/media.json"), &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_mission(&mut p, ID).unwrap();
    Rig { assets, m, p, rel: mission_rel(ID).unwrap() }
}

impl Rig {
    fn shown(&self) -> Value {
        merged_mission(&self.p, ID).unwrap().clone()
    }
    fn names(v: &Value) -> Vec<String> {
        mdoc::rules(v).iter().filter_map(|r| mdoc::rule_name(r).map(str::to_string)).collect()
    }
    /// The engine builds the overlay of the saved mod over the import; returns the mission it reads and what it skipped.
    fn engine(&self, tag: &str) -> (Value, Vec<(String, String)>) {
        let out = self.m.parent().unwrap().join(format!("overlay-{tag}"));
        let report = overlay_into(&self.assets, std::slice::from_ref(&self.m), &out).unwrap();
        let skipped = report.mods.iter().flat_map(|e| e.skipped.clone()).collect();
        (read_merged(&out, &self.rel).unwrap(), skipped)
    }
}

#[test]
fn changing_one_action_alone_is_one_keyed_rule_entry() {
    let mut r = rig("action", retail());
    r.p.do_step(&r.rel, "Change action", vec![Edit::set(format!("{}/1/text", mdoc::rule_list_pointer(0, false)), json!("B!"))]).unwrap();
    let want = r.shown();
    assert!(r.p.save().errors.is_empty());
    let file = read(&r.m, &r.rel);
    assert_eq!(
        file,
        json!({"extends": ID, "rules": [{"name": "Start", "actions": [{"Type": "Say", "text": "a"}, {"Type": "Say", "text": "B!"}]}]}),
        "the rule is a keyed entry (by name) carrying its changed actions list, never an index pointer"
    );
    let (got, skipped) = r.engine("action");
    assert!(skipped.is_empty(), "{skipped:?}");
    assert_eq!(got, want);
    assert_eq!(mdoc::rules(&got)[0]["Type"], "On Start", "the rule's other keys come from retail");
}

#[test]
fn an_added_rule_runs_after_every_retail_rule() {
    let mut r = rig("add", retail());
    add_entry(&mut r.p, ID, MissionList::Rules, json!({"name": "Late", "Type": "On Timer", "actions": []})).unwrap();
    let want = r.shown();
    assert!(r.p.save().errors.is_empty());
    assert_eq!(read(&r.m, &r.rel), json!({"extends": ID, "rules": [{"name": "Late", "Type": "On Timer", "actions": []}]}));
    let (got, skipped) = r.engine("add");
    assert!(skipped.is_empty());
    assert_eq!(got, want);
    assert_eq!(Rig::names(&got), ["Start", "Middle", "End", "Late"], "appended: there is no 'insert after' (open question 1)");
}

#[test]
fn a_deleted_rule_is_a_remove_entry_and_a_renamed_one_is_remove_plus_append() {
    let mut r = rig("delete", retail());
    delete_entry(&mut r.p, ID, MissionList::Rules, 1).unwrap();
    // a rename: edit the name key of "End" in place
    r.p.do_step(&r.rel, "Rename rule", vec![Edit::set(mdoc::rule_pointer(1, Some("name")), json!("Finale"))]).unwrap();
    let want = r.shown();
    assert!(r.p.save().errors.is_empty());
    let file = read(&r.m, &r.rel);
    assert_eq!(file["rules"][0], json!({"name": "Middle", "$remove": true}));
    assert!(file["rules"].as_array().unwrap().contains(&json!({"name": "End", "$remove": true})), "{file}");
    let (got, skipped) = r.engine("delete");
    assert!(skipped.is_empty());
    assert_eq!(got, want);
    assert_eq!(Rig::names(&got), ["Start", "Finale"]);
}

#[test]
fn moving_a_ship_saves_one_keyed_entry_and_the_engine_agrees() {
    let mut r = rig("ship", retail());
    r.p.do_step(&r.rel, "Move ship", vec![Edit::set(mdoc::ship_pointer(1, Some("pos")), json!([9.0, 5.0, 0.0]))]).unwrap();
    let want = r.shown();
    assert!(r.p.save().errors.is_empty());
    let file = read(&r.m, &r.rel);
    assert_eq!(file, json!({"extends": ID, "ships": [{"id": 2, "pos": [9.0, 5.0, 0.0]}]}));
    let (got, skipped) = r.engine("ship");
    assert!(skipped.is_empty());
    assert_eq!(got, want);
    assert_eq!(mdoc::ships(&got)[1]["type"], "B");
    // the import is untouched
    assert_eq!(read(&r.assets, &format!("maps/{ID}/mission.json")), retail());
}

#[test]
fn a_retail_list_with_a_repeated_or_missing_rule_name_is_refused_for_keyed_edits_and_skipped_by_the_engine() {
    for (tag, bad) in [("dup", json!({"name": "Start", "Type": "On Timer"})), ("noname", json!({"Type": "On Timer"}))] {
        let mut m = retail();
        m["rules"].as_array_mut().unwrap().push(bad);
        let mut r = rig(tag, m);
        // the project refuses a keyed add and says why
        let e = add_entry(&mut r.p, ID, MissionList::Rules, json!({"name": "New"})).unwrap_err();
        assert!(e.contains("restating the whole list") || e.contains("name"), "{tag}: {e}");
        let why = retail_limits(&r.p, ID);
        assert!(why.iter().any(|l| l.contains("skip")), "{tag}: {why:?}");
        // a ship move is a keyed entry too: the engine skips the whole file and retail shows through
        r.p.do_step(&r.rel, "Move ship", vec![Edit::set(mdoc::ship_pointer(0, Some("pos")), json!([1.0, 1.0, 0.0]))]).unwrap();
        let saved = r.p.save();
        assert!(saved.errors.is_empty(), "{saved:?}");
        let file = read(&r.m, &r.rel);
        let (got, skipped) = r.engine(tag);
        assert!(skipped.is_empty(), "{tag}: a ship move touches no keyed list, so the file still applies: {skipped:?}");
        assert_eq!(mdoc::ships(&got)[0]["pos"], json!([1.0, 1.0, 0.0]), "{file}");
        // what the editor refuses to write: a keyed rule entry. By hand the engine skips the whole file, ship move included.
        put(&r.m, &r.rel, &json!({"extends": ID, "set": {"/ships/0/pos/0": 1.0}, "rules": [{"name": "Start", "Type": "On Timer"}]}));
        let (got, skipped) = r.engine(&format!("{tag}-hand"));
        assert!(skipped.iter().any(|(f, _)| f.contains("mission.json")), "{tag}: {skipped:?}");
        assert_eq!(got, retail_with(&r), "{tag}: retail shows through, the ship move is lost");
    }
}

/// The mission as the import has it (with the repeated or nameless rule this rig was made with).
fn retail_with(r: &Rig) -> Value {
    read(&r.assets, &r.rel)
}

#[test]
fn the_notice_lists_the_limits_for_a_retail_mission_only() {
    let r = rig("notice", retail());
    let notes = retail_limits(&r.p, ID);
    assert!(notes.iter().any(|n| n.contains("after all retail rules")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("whole actions list")), "{notes:?}");
    assert!(retail_limits(&r.p, "my_own").is_empty());
}

#[test]
fn an_action_change_next_to_a_keyed_add_or_delete_is_written_by_key_and_the_engine_agrees() {
    let mut r = rig("mixed", retail());
    delete_entry(&mut r.p, ID, MissionList::Rules, 1).unwrap();
    // "Start" is still rule 0: change its second action
    r.p.do_step(&r.rel, "Change action", vec![Edit::set(format!("{}/1/text", mdoc::rule_list_pointer(0, false)), json!("B!"))]).unwrap();
    add_entry(&mut r.p, ID, MissionList::Rules, json!({"name": "Late", "Type": "On Timer", "actions": []})).unwrap();
    let want = r.shown();
    assert!(r.p.save().errors.is_empty());
    let file = read(&r.m, &r.rel);
    assert_eq!(
        file["rules"][0],
        json!({"name": "Start", "actions": [{"Type": "Say", "text": "a"}, {"Type": "Say", "text": "B!"}]}),
        "next to a keyed add or delete the rule is written by key, and its whole actions list is restated"
    );
    let (got, skipped) = r.engine("mixed");
    assert!(skipped.is_empty(), "{skipped:?} {file}");
    assert_eq!(got, want, "{file}");
    assert_eq!(Rig::names(&got), ["Start", "End", "Late"]);
}

/// A retail-like mission with three tasks (keyed by `Name` since the engine's `e2f096c`).
fn with_tasks() -> Value {
    let mut m = retail();
    m["tasks"] = json!([
        {"Name": "Scout", "text": "Find it", "Active": true, "complete_when": [{"Type": "Enter Volume", "Group Name": "g", "Volume Name": "Gate"}], "fail_when": []},
        {"Name": "Escort", "text": "Guard it", "Active": true, "complete_when": [], "fail_when": []},
        {"Name": "Leave", "text": "Go", "Active": false, "complete_when": [], "fail_when": []}
    ]);
    m
}

#[test]
fn tasks_of_an_extending_mission_are_keyed_entries_and_the_engine_agrees() {
    let mut r = rig("tasks", with_tasks());
    // edit one task, delete one, add one, all in one save
    r.p.do_step(&r.rel, "Edit task", vec![Edit::set(mdoc::task_pointer(0, Some(mdoc::TASK_TEXT)), json!("Find the gate"))]).unwrap();
    delete_entry(&mut r.p, ID, MissionList::Tasks, 1).unwrap();
    add_entry(&mut r.p, ID, MissionList::Tasks, json!({"Name": "Late", "text": "New", "Active": true, "complete_when": [], "fail_when": []})).unwrap();
    // an added task needs its key
    assert!(add_entry(&mut r.p, ID, MissionList::Tasks, json!({"text": "No name"})).is_err());
    let want = r.shown();
    assert!(r.p.save().errors.is_empty());
    let file = read(&r.m, &r.rel);
    let tasks = file["tasks"].as_array().unwrap_or_else(|| panic!("{file}"));
    assert!(tasks.contains(&json!({"Name": "Escort", "$remove": true})), "{file}");
    assert!(tasks.iter().all(|t| t["Name"] != "Leave"), "an untouched task is not restated: {file}");
    let (got, skipped) = r.engine("tasks");
    assert!(skipped.is_empty(), "{skipped:?} {file}");
    assert_eq!(got, want, "{file}");
    let names: Vec<_> = mdoc::tasks(&got).iter().map(|t| t["Name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["Scout", "Leave", "Late"]);
    assert_eq!(mdoc::tasks(&got)[0]["text"], "Find the gate");
    assert_eq!(mdoc::tasks(&got)[0]["complete_when"].as_array().unwrap().len(), 1, "the rest of the task comes from retail");
}

#[test]
fn repeated_task_names_in_the_retail_mission_refuse_keyed_task_edits_and_the_notice_says_so() {
    let mut m = with_tasks();
    m["tasks"].as_array_mut().unwrap().push(json!({"Name": "Scout", "text": "again"}));
    let mut r = rig("tasks-dup", m);
    let e = add_entry(&mut r.p, ID, MissionList::Tasks, json!({"Name": "New"})).unwrap_err();
    assert!(e.contains("Name") || e.contains("restating"), "{e}");
    assert!(delete_entry(&mut r.p, ID, MissionList::Tasks, 0).is_err());
    let why = retail_limits(&r.p, ID);
    assert!(why.iter().any(|l| l.contains("task list") && l.contains("skip")), "{why:?}");
    // with unique names the notice has no task line
    let ok = rig("tasks-ok", with_tasks());
    assert!(!retail_limits(&ok.p, ID).iter().any(|l| l.contains("task list")));
}
