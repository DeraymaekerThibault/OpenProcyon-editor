//! T2 (docs/phase5.md): the file-generic rule and media operations (`mission::rules`, `mission::media`) on a `mission.json` and a
//! `script.json`, and the reference walk (`mission::refs`) held to the engine's rule schema. Synthetic data only.

use openprocyon::authoring::rule_schema;
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::media;
use openprocyon_editor::mission::refs::{self, NAME_KINDS, OTHER_KINDS, Part};
use openprocyon_editor::mission::rules::{self, Target};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::PathBuf;

const MISSION: &str = "maps/m/mission.json";
const SCRIPT: &str = "maps/m/script.json";
const MEDIA: &str = "maps/m/media.json";

fn setup(name: &str, files: &[(&str, Value)]) -> (PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-t2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (rel, v) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
    }
    let mut p = Project::open(&root, None).unwrap();
    for (rel, _) in files {
        p.open_doc(rel).unwrap();
    }
    (root, p)
}

fn doc<'a>(p: &'a Project, rel: &str) -> &'a Value {
    &p.doc(rel).unwrap().edited
}

fn names(p: &Project, rel: &str) -> Vec<String> {
    mdoc::rules(doc(p, rel)).iter().filter_map(|r| mdoc::rule_name(r).map(str::to_string)).collect()
}

#[test]
fn rule_operations_on_a_mission_and_a_script() {
    let (root, mut p) = setup("ops", &[(MISSION, json!({"rules": [{"name": "A", "conditions": [], "actions": []}]})), (SCRIPT, json!({"rules": []}))]);
    for (rel, noun) in [(MISSION, "rule"), (SCRIPT, "script rule")] {
        let t = if noun == "rule" { Target::mission(rel) } else { Target::script(rel) };
        let before = names(&p, rel).len();
        let a = rules::add_rule(&mut p, &t).unwrap();
        assert_eq!(a, before);
        assert_eq!(p.undo_label(), Some(format!("Add {noun}").as_str()));
        let b = rules::add_rule(&mut p, &t).unwrap();
        assert_ne!(names(&p, rel)[a], names(&p, rel)[b], "names stay unique");
        assert_eq!(doc(&p, rel)["rules"][a]["Run Once"], json!(true));
        rules::set_rule_flag(&mut p, &t, a, "Run Once", false).unwrap();
        assert_eq!(doc(&p, rel)["rules"][a]["Run Once"], json!(false));
        assert_eq!(p.undo_label(), Some(format!("{}{} Run Once", noun[..1].to_uppercase(), &noun[1..]).as_str()));
        // items
        let at = rules::add_item(&mut p, &t, a, true, json!({"Type": "X", "n": 1})).unwrap();
        assert_eq!(at, 0);
        rules::add_item(&mut p, &t, a, true, json!({"Type": "Y"})).unwrap();
        rules::add_item(&mut p, &t, a, false, json!({"Type": "Z"})).unwrap();
        rules::replace_item(&mut p, &t, a, true, 0, json!({"Type": "X", "n": 2}), false).unwrap();
        rules::move_item(&mut p, &t, a, true, 0, 1).unwrap();
        let types: Vec<&str> = mdoc::rule_items(&mdoc::rules(doc(&p, rel))[a], true).iter().filter_map(mdoc::item_type).collect();
        assert_eq!(types, ["Y", "X"]);
        rules::remove_item(&mut p, &t, a, true, 0).unwrap();
        assert_eq!(mdoc::rule_items(&mdoc::rules(doc(&p, rel))[a], true).len(), 1);
        assert!(rules::move_item(&mut p, &t, a, true, 0, 5).is_err());
        // rename, move, duplicate, delete
        rules::rename_rule(&mut p, &t, a, "Renamed").unwrap();
        assert_eq!(names(&p, rel)[a], "Renamed");
        let n = names(&p, rel).len();
        let copy = rules::duplicate_rule(&mut p, &t, a).unwrap();
        assert_eq!(copy, a + 1);
        assert_eq!(names(&p, rel).len(), n + 1);
        assert_eq!(names(&p, rel)[copy], "Renamed copy 1");
        assert_eq!(doc(&p, rel)["rules"][copy]["actions"], doc(&p, rel)["rules"][a]["actions"]);
        let last = names(&p, rel).len() - 1;
        rules::move_rule(&mut p, &t, copy, last).unwrap();
        assert_eq!(names(&p, rel)[last], "Renamed copy 1");
        rules::delete_rule(&mut p, &t, last).unwrap();
        assert_eq!(names(&p, rel).len(), n);
        assert_eq!(rules::delete_rule(&mut p, &t, 99).unwrap_err(), format!("no {noun} 99"));
        assert!(rules::move_rule(&mut p, &t, 0, 99).is_err() && rules::duplicate_rule(&mut p, &t, 99).is_err());
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn one_step_per_operation() {
    let (root, mut p) = setup("undo", &[(MISSION, json!({"rules": [{"name": "A"}, {"name": "B"}]}))]);
    let t = Target::mission(MISSION);
    let start = doc(&p, MISSION).clone();
    rules::duplicate_rule(&mut p, &t, 0).unwrap();
    rules::move_rule(&mut p, &t, 0, 2).unwrap();
    rules::add_item(&mut p, &t, 0, false, json!({"Type": "Z"})).unwrap();
    for _ in 0..3 {
        assert!(p.undo().is_some());
    }
    assert_eq!(doc(&p, MISSION), &start);
    // a document with no `rules` list gets one
    let (root2, mut q) = setup("undo2", &[(MISSION, json!({"players": []}))]);
    assert_eq!(rules::add_rule(&mut q, &Target::mission(MISSION)).unwrap(), 0);
    assert_eq!(doc(&q, MISSION)["rules"].as_array().map(Vec::len), Some(1));
    let _ = (std::fs::remove_dir_all(root), std::fs::remove_dir_all(root2));
}

#[test]
fn media_tables() {
    let (root, mut p) = setup("media", &[(MEDIA, json!({"speech": {}, "messages": {}, "ship_names": {}}))]);
    media::add(&mut p, MEDIA, mdoc::MediaTable::Messages, " WIN ").unwrap();
    media::set(&mut p, MEDIA, mdoc::MediaTable::Messages, "WIN", None, Some(json!("Well fought."))).unwrap();
    media::add(&mut p, MEDIA, mdoc::MediaTable::Speech, "HINT").unwrap();
    media::set(&mut p, MEDIA, mdoc::MediaTable::Speech, "HINT", Some("text"), Some(json!("Hold."))).unwrap();
    assert!(media::add(&mut p, MEDIA, mdoc::MediaTable::Speech, "HINT").is_err());
    assert!(media::add(&mut p, MEDIA, mdoc::MediaTable::Speech, "  ").is_err());
    let m = doc(&p, MEDIA);
    assert_eq!(media::entries(m, mdoc::MediaTable::Messages), [("WIN", &json!("Well fought."))]);
    assert_eq!(m["speech"]["HINT"]["text"], json!("Hold."));
    media::set(&mut p, MEDIA, mdoc::MediaTable::Speech, "HINT", Some("text"), None).unwrap();
    media::remove(&mut p, MEDIA, mdoc::MediaTable::Messages, "WIN").unwrap();
    assert!(media::entries(doc(&p, MEDIA), mdoc::MediaTable::Messages).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

// ---- refs ----

fn schema() -> Value {
    rule_schema()
}

/// The first Type of `role` with a field of `kind`.
fn type_with(role: &str, kind: &str) -> Option<(String, String)> {
    schema()["types"].as_array()?.iter().filter(|t| t["role"] == role).find_map(|t| {
        let f = t["fields"].as_array()?.iter().find(|f| f["kind"] == kind)?;
        Some((t["name"].as_str()?.to_string(), f["name"].as_str()?.to_string()))
    })
}

#[test]
fn every_schema_kind_is_classified() {
    let s = schema();
    for k in s["kinds"].as_array().unwrap() {
        let name = k["name"].as_str().unwrap();
        assert!(NAME_KINDS.contains(&name) || OTHER_KINDS.contains(&name), "the engine has a field kind '{name}' that refs.rs does not know");
    }
    for k in NAME_KINDS.iter().chain(OTHER_KINDS) {
        assert!(s["kinds"].as_array().unwrap().iter().any(|x| x["name"] == *k), "refs.rs lists kind '{k}' that the engine no longer has");
    }
    // and every field of every Type has one of them
    for t in s["types"].as_array().unwrap() {
        for f in t["fields"].as_array().unwrap() {
            let k = f["kind"].as_str().unwrap();
            assert!(NAME_KINDS.contains(&k) || OTHER_KINDS.contains(&k), "{}: kind '{k}'", t["name"]);
        }
    }
}

#[test]
fn the_walk_finds_every_name_field_of_the_schema() {
    let s = schema();
    let (mut seen, mut want) = (0, 0);
    for t in s["types"].as_array().unwrap() {
        let is_condition = t["role"] == "condition";
        let mut item = json!({"Type": t["name"]});
        let mut named = vec![];
        for f in t["fields"].as_array().unwrap() {
            let (name, kind) = (f["name"].as_str().unwrap(), f["kind"].as_str().unwrap());
            if NAME_KINDS.contains(&kind) {
                item[name] = json!(format!("N-{name}"));
                named.push(name.to_string());
            }
        }
        let rule = if is_condition { json!({"name": "r", "conditions": [item], "actions": []}) } else { json!({"name": "r", "conditions": [], "actions": [item]}) };
        let mission = json!({"rules": [rule]});
        let mut found = vec![];
        refs::walk(&mission, &mut |v| found.push((v.field(), v.pointer.clone(), v.text.to_string())));
        assert_eq!(found.len(), named.len(), "{}: walk found {:?}, schema names {:?}", t["name"], found.iter().map(|v| v.0).collect::<Vec<_>>(), named);
        for v in &found {
            assert_eq!(mission.pointer(&v.1).and_then(Value::as_str), Some(v.2.as_str()), "pointer {} points at the field", v.1);
        }
        seen += found.len();
        want += named.len();
    }
    assert_eq!((seen > 100, seen), (true, want));
}

#[test]
fn nested_conditions_and_task_lists_are_walked() {
    let (any_type, list_field) = type_with("condition", "condition_list").expect("the engine has a condition list (OP Any Of)");
    let (not_type, not_field) = type_with("condition", "condition").expect("the engine has a nested condition (OP Not)");
    let (vol_type, vol_field) = type_with("condition", "volume").expect("a condition with a volume");
    let inner = json!({"Type": vol_type, (vol_field.as_str()): "Gate"});
    let mission = json!({
        "rules": [{"name": "r", "conditions": [
            {"Type": any_type, (list_field.as_str()): [inner.clone()]},
            {"Type": not_type, (not_field.as_str()): inner.clone()}], "actions": []}],
        "tasks": [{"Name": "T", "complete_when": [inner.clone()], "fail_when": [{"Type": any_type, (list_field.as_str()): [inner.clone()]}]}],
    });
    let hits = refs::references(&mission, "volume", "Gate");
    let mut at: Vec<&str> = hits.iter().map(|r| r.pointer.as_str()).collect();
    at.sort();
    assert_eq!(hits.len(), 4, "{at:?}");
    for r in &hits {
        assert_eq!(mission.pointer(&r.pointer).and_then(Value::as_str), Some("Gate"));
    }
    assert!(at.iter().any(|p| p.starts_with("/tasks/0/fail_when/0/")), "{at:?}");
    assert!(refs::references(&mission, "volume", "Other").is_empty());
    assert!(refs::references(&mission, "path", "Gate").is_empty(), "another kind is another name");
    assert!(refs::references(&mission, "volume", "").is_empty());
}

#[test]
fn group_unit_specs_and_the_shared_namespace() {
    let (ty, field) = type_with("action", "group").expect("an action with a group field");
    let item = |text: &str| json!({"Type": ty, (field.as_str()): text});
    let mission = json!({"rules": [{"name": "r", "conditions": [], "actions": [item("Raiders"), item("Raiders,Boss"), item("Others,Raiders"), item(" Raiders")]}]});
    let g = refs::references(&mission, "group", "Raiders");
    assert_eq!(g.iter().map(|r| (r.pointer.rsplit('/').nth(1).unwrap().to_string(), r.part)).collect::<Vec<_>>(), [("0".into(), Part::Whole), ("1".into(), Part::Group)]);
    // a player is a group's namespace
    assert_eq!(refs::references(&mission, "player", "Raiders").len(), 2);
    let u = refs::references(&mission, "unit", "Boss");
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].renamed("Chief"), "Raiders,Chief");
    assert_eq!(g[1].renamed("Pirates"), "Pirates,Boss");
    assert_eq!(refs::references(&mission, "unit", "Raiders")[0].renamed("X"), "Others,X");
    assert!(refs::rename_edits(&mission, "group", "Raiders", "A,B").is_err());
    assert!(refs::rename_edits(&mission, "group", "Raiders", "").is_err());
    assert!(refs::rename_edits(&mission, "group", "Raiders", "Raiders").unwrap().is_empty());
}

#[test]
fn rules_are_matched_by_trimmed_name() {
    let (ty, field) = type_with("action", "rule").expect("an action with a rule field (OP Set Rule Active)");
    let mission = json!({"rules": [{"name": "r", "conditions": [], "actions": [{"Type": ty, (field.as_str()): " Boss Fight "}]}]});
    assert_eq!(refs::references(&mission, "rule", "Boss Fight").len(), 1);
}

#[test]
fn rename_is_one_undo_step_across_rules_and_tasks() {
    let (vol_type, vol_field) = type_with("condition", "volume").unwrap();
    let (act_type, act_field) = type_with("action", "volume").expect("an action with a volume field");
    let cond = |v: &str| json!({"Type": vol_type, (vol_field.as_str()): v});
    let act = |v: &str| json!({"Type": act_type, (act_field.as_str()): v});
    let start = json!({
        "volumes": {"Gate": [[0, 0], [1, 0], [1, 1]]},
        "rules": [
            {"name": "a", "conditions": [cond("Gate")], "actions": [act("Gate")]},
            {"name": "b", "conditions": [cond("Elsewhere")], "actions": []},
            {"name": "c", "conditions": [], "actions": [act("Gate")]}],
        "tasks": [{"Name": "t", "complete_when": [cond("Gate")]}],
    });
    let (root, mut p) = setup("rename", &[(MISSION, start.clone())]);
    let n = refs::rename(&mut p, MISSION, "volume", "Gate", "Door").unwrap();
    assert_eq!(n, 4);
    let d = doc(&p, MISSION);
    assert_eq!(d["rules"][1]["conditions"][0][&vol_field], json!("Elsewhere"));
    assert_eq!(d["tasks"][0]["complete_when"][0][&vol_field], json!("Door"));
    assert!(refs::references(d, "volume", "Gate").is_empty());
    assert_eq!(refs::references(d, "volume", "Door").len(), n);
    assert_eq!(d["volumes"], start["volumes"], "the definition is the caller's edit");
    assert!(p.undo().is_some());
    assert_eq!(doc(&p, MISSION), &start, "one undo puts every reference back");
    assert!(p.undo().is_none());
    // nothing to rename: no step
    assert_eq!(refs::rename(&mut p, MISSION, "volume", "Nope", "X").unwrap(), 0);
    assert!(p.undo().is_none());
    assert!(refs::rename(&mut p, MISSION, "volume", "Gate", "").is_err());
    let _ = std::fs::remove_dir_all(root);
}
