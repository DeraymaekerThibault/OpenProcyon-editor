//! The editor's use of the engine's keyed merge (src/project/keyed.rs) held to the engine, through `authoring::extend`, on synthetic
//! JSON (no assets). The rule and the merge are the engine's; what stays editor code is [`keyed::effective`] (a file's own `merge`
//! object applied to the engine's table) and the diff the editor writes ([`keyed::diff_list`]).
use openprocyon_editor::project::keyed::{self, EngineRule};
use serde_json::{Value, json};

#[test]
fn a_files_own_merge_object_is_the_engines() {
    for (merge, rel, ptr, field) in [
        (json!({"/teams": "name_id"}), "maps/m/map.json", "/teams", "name_id"),
        (json!({"/objects": false}), "maps/m/map.json", "/objects", "id"),
        (json!({"/objects": "type"}), "maps/m/map.json", "/objects", "id"),
        (json!({"/items": "k"}), "data/other.json", "/items", "k"),
        (json!({}), "maps/m/map.json", "/objects", "id"),
        (json!({}), "maps/m/mission.json", "/points", "Name"),
        (json!({}), "ships/S/ship.json", "/objects", "id"),
    ] {
        let key = &ptr[1..];
        let parent = json!({ key: [{ field: 1, "v": 1 }] });
        let diff = json!({ "extends": "m", "merge": merge, key: [{ field: 1, "w": 2 }] });
        let engine_merges = keyed::engine_extend(rel, &parent, &diff).is_ok_and(|(v, _)| v[key] == json!([{ field: 1, "v": 1, "w": 2 }]));
        let ours = keyed::effective(&EngineRule, rel, diff.get("merge")).contains(&(ptr.to_string(), field.to_string()));
        assert_eq!(engine_merges, ours, "{merge} on {rel} {ptr} by {field}");
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
