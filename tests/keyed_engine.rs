//! The editor's copy of the engine's keyed merge (src/project/keyed.rs) held to the engine, through its public entry points
//! (`authoring::overlay_into` + `read_merged`), on synthetic JSON (no assets). If one of these fails, the engine's rule changed (or
//! the copy is wrong): fix `keyed::KeyedRule::lists` / `keyed::extend`.
use openprocyon_editor::project::keyed::{self, EngineRule, KeyedRule};
use serde_json::{Value, json};

const FILES: &[&str] =
    &["maps/m/map.json", "maps/m/mission.json", "maps/m/script.json", "maps/m/other.json", "maps/m/x/map.json", "ships/S/ship.json", "data/weapons/guns.json", "data/other.json"];
const POINTERS: &[&str] = &["/objects", "/ships", "/mines", "/nebulae", "/players", "/rules", "/points", "/teams", "/items", "/engines", "/actions"];
const FIELDS: &[&str] = &["id", "name", "Name"];

/// The extends file for a list at `ptr` keyed by `field`: parent entry `{field: 1, v: 1}`, diff entry `{field: 1, w: 2}`. Returns
/// whether the engine merged the entry (kept `v`, added `w`) rather than replacing the list (or skipping the file).
fn engine_merges(rel: &str, ptr: &str, field: &str) -> bool {
    let key = &ptr[1..];
    let parent = json!({ key: [{ field: 1, "v": 1 }] });
    let diff = json!({ "extends": "m", key: [{ field: 1, "w": 2 }] });
    match keyed::engine_extend(rel, &parent, &diff) {
        Ok((merged, _)) => merged[key] == json!([{ field: 1, "v": 1, "w": 2 }]),
        Err(_) => false,
    }
}

#[test]
fn the_default_lists_are_the_engines() {
    for rel in FILES {
        for ptr in POINTERS {
            for field in FIELDS {
                let ours = EngineRule.lists(rel).contains(&(ptr.to_string(), field.to_string()));
                assert_eq!(engine_merges(rel, ptr, field), ours, "{rel} {ptr} by {field}: engine vs EngineRule");
            }
        }
    }
}

#[test]
fn a_files_own_merge_object_is_the_engines() {
    for (merge, rel, ptr, field) in [
        (json!({"/teams": "name_id"}), "maps/m/map.json", "/teams", "name_id"),
        (json!({"/objects": false}), "maps/m/map.json", "/objects", "id"),
        (json!({"/objects": "type"}), "maps/m/map.json", "/objects", "id"),
        (json!({"/items": "k"}), "data/other.json", "/items", "k"),
    ] {
        let key = &ptr[1..];
        let parent = json!({ key: [{ field: 1, "v": 1 }] });
        let diff = json!({ "extends": "m", "merge": merge, key: [{ field: 1, "w": 2 }] });
        let theirs = keyed::engine_extend(rel, &parent, &diff).map(|(v, _)| v);
        let ours = keyed::extend(&EngineRule, rel, parent, &diff).map(|(v, _)| v);
        assert_eq!(theirs.is_ok(), ours.is_ok(), "{merge} on {rel}: {theirs:?} vs {ours:?}");
        if let (Ok(t), Ok(o)) = (theirs, ours) {
            assert_eq!(t, o, "{merge} on {rel}");
        }
    }
}

#[test]
fn extend_gives_the_engines_result_and_warnings() {
    let parent = json!({
        "name": "Red", "teams": [{"name_id": "T1"}], "meta": {"a": 1, "b": 2},
        "objects": [{"id": 1, "type": "A", "pos": [0, 0, 0], "n": {"x": 1, "y": 2}}, {"id": 2, "type": "B"}, {"id": 3, "type": "C"}],
        "players": [{"name": "P1", "start": [1, 1, 0]}]
    });
    let diffs = [
        json!({"extends": "m", "objects": [{"id": 1, "n": {"y": 9}}, {"id": 2, "$remove": true}, {"id": 7, "type": "New"}]}),
        json!({"extends": "m", "objects": [{"id": 9, "$remove": true}, {"id": 3, "$remove": true}], "set": {"/objects/0/type": "Z"}}),
        json!({"extends": "m", "merge": {"/objects": false}, "objects": [{"id": 5}]}),
        json!({"extends": "m", "merge": {"/teams": "name_id"}, "teams": [{"name_id": "T1", "x": 1}, {"name_id": "T2"}]}),
        json!({"extends": "m", "meta": {"b": 3}, "players": [{"name": "P2"}, {"name": "P1", "start": [2, 2, 2]}], "ships": [{"id": 1}]}),
        json!({"extends": "m", "objects": [{"type": "no key"}]}),
        json!({"extends": "m", "set": {"/objects/9/type": "Z"}}),
        json!({"extends": "m", "merge": {"/nothing": "id"}}),
    ];
    for diff in &diffs {
        let theirs = keyed::engine_extend("maps/m/map.json", &parent, diff);
        let ours = keyed::extend(&EngineRule, "maps/m/map.json", parent.clone(), diff);
        match (theirs, ours) {
            (Ok((t, tw)), Ok((o, ow))) => {
                assert_eq!(t, o, "{diff}");
                assert_eq!(tw.len(), ow.len(), "{diff}: {tw:?} vs {ow:?}");
            }
            (Err(_), Err(_)) => {}
            (t, o) => panic!("{diff}: engine {t:?} vs editor {o:?}"),
        }
    }
}

#[test]
fn the_diff_the_editor_writes_merges_back_through_the_engine() {
    let base = json!({"objects": [{"id": 1, "a": 1}, {"id": 2, "a": 2}, {"id": 3, "a": 3}]});
    let entries = match keyed::diff_list("id", base["objects"].as_array().unwrap(), json!([{"id": 3, "a": 3}, {"id": 2, "a": 5}, {"id": 4}]).as_array().unwrap(), "/objects") {
        keyed::ListDiff::Entries { entries, .. } => entries,
        other => panic!("{other:?}"),
    };
    let (merged, warnings) = keyed::engine_extend("maps/m/map.json", &base, &json!({"extends": "m", "objects": entries})).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let ids: Vec<Value> = merged["objects"].as_array().unwrap().iter().map(|o| o["id"].clone()).collect();
    assert_eq!(ids, [json!(3), json!(2), json!(4)]);
}
