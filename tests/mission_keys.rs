//! Every key the editor writes into a mission, script or media document is a property of the engine's own JSON Schema for that file
//! (`authoring::schemas()`, generated from the types the loader reads), so a key the engine renames or drops fails the gate here
//! instead of being written to a file the loader ignores. Synthetic: needs no assets.
use openprocyon::authoring::schemas;
use openprocyon_editor::mission::doc as d;
use serde_json::Value;

fn schema(file: &str) -> Value {
    schemas().into_iter().find(|(n, _)| *n == file).unwrap_or_else(|| panic!("the engine has no {file}")).1
}

/// The object schemas a node can be: `$ref`s followed, `anyOf` / `oneOf` / `allOf` branches joined (an `Option<T>`), arrays through `items`.
fn objects<'a>(root: &'a Value, node: &'a Value, out: &mut Vec<&'a Value>) {
    if let Some(r) = node["$ref"].as_str() {
        let target = root.pointer(r.trim_start_matches('#')).unwrap_or_else(|| panic!("unresolved {r}"));
        return objects(root, target, out);
    }
    for k in ["anyOf", "oneOf", "allOf"] {
        for b in node[k].as_array().into_iter().flatten() {
            objects(root, b, out);
        }
    }
    if node["items"].is_object() {
        objects(root, &node["items"], out);
    }
    if node["properties"].is_object() || node["additionalProperties"].is_object() {
        out.push(node);
    }
}

/// The schema nodes at `path` (`*` is any key of a map: `additionalProperties`).
fn at<'a>(root: &'a Value, path: &[&str]) -> Vec<&'a Value> {
    let mut here = vec![root];
    for seg in path {
        let mut next = vec![];
        for node in here {
            let mut objs = vec![];
            objects(root, node, &mut objs);
            for o in objs {
                let child = if *seg == "*" { Some(&o["additionalProperties"]).filter(|v| v.is_object()) } else { o["properties"].get(*seg) };
                next.extend(child);
            }
        }
        assert!(!next.is_empty(), "the engine schema has no `{}`", path.join("."));
        here = next;
    }
    here
}

/// Asserts each of `keys` is a property of the object at `path` of `file`'s schema.
fn known(file: &str, path: &[&str], keys: &[&str]) {
    let root = schema(file);
    let mut objs = vec![];
    for n in at(&root, path) {
        objects(&root, n, &mut objs);
    }
    for k in keys {
        assert!(objs.iter().any(|o| o["properties"].get(*k).is_some()), "{file} {}: the engine's schema has no property `{k}` (the editor writes it)", path.join("."));
    }
}

#[test]
fn the_mission_keys_the_editor_writes_are_the_engines() {
    let f = "mission.schema.json";
    known(
        f,
        &[],
        &[
            d::KEY_PLAYERS,
            d::KEY_ALLIANCES,
            d::KEY_PLAYER_START,
            d::KEY_SHIPS,
            d::KEY_GROUPS,
            d::KEY_VOLUMES,
            d::KEY_PATHS,
            d::KEY_POINTS,
            d::KEY_ENVIRONMENT,
            d::KEY_TASKS,
            d::KEY_RULES,
            d::KEY_MAP_TEXT,
            d::KEY_VICTORY,
            d::KEY_VICTORY_MESSAGE,
            d::KEY_DEFEAT_MESSAGE,
        ],
    );
    known(f, &[d::KEY_ENVIRONMENT], &[d::ENV_POINT_SETS, d::ENV_NEBULAE, d::ENV_ANIMALS]);
    known(f, &[d::KEY_SHIPS], &[d::SHIP_ID, d::SHIP_TYPE, d::SHIP_GROUP, d::SHIP_NAME, d::SHIP_PRIMARY, d::SHIP_POS, d::SHIP_ROT]);
    known(f, &[d::KEY_PLAYER_START], &[d::START_NAME, d::START_POS, d::START_FORWARD]);
    known(f, &[d::KEY_POINTS], &[d::POINT_NAME, d::POINT_POSITION]);
    known(f, &[d::KEY_ENVIRONMENT, d::ENV_POINT_SETS, "*"], &[d::POINT_SET_POS]);
    known(f, &[d::KEY_TASKS], &[d::TASK_NAME, d::TASK_TEXT, d::TASK_ACTIVE, d::TASK_COMPLETE_WHEN, d::TASK_FAIL_WHEN, d::TASK_REQUIRED]);
    known(f, &[d::KEY_MAP_TEXT], &[d::MAP_TEXT_NAME, d::MAP_TEXT_TEXT, d::MAP_TEXT_POS, d::MAP_TEXT_VISIBLE]);
    known(f, &[d::KEY_ENVIRONMENT, d::ENV_ANIMALS], &[d::ANIMAL_GROUP]);
    known(f, &[d::KEY_RULES], &[d::RULE_NAME, d::RULE_CONDITIONS, d::RULE_ACTIONS, d::RULE_RUN_ONCE, d::RULE_IS_ACTIVE]);
}

#[test]
fn the_script_and_media_keys_the_editor_writes_are_the_engines() {
    known("script.schema.json", &[], &[d::SCRIPT_KEY_VICTORY, d::KEY_RULES]);
    known("script.schema.json", &[d::KEY_RULES], &[d::RULE_NAME, d::RULE_CONDITIONS, d::RULE_ACTIONS, d::RULE_RUN_ONCE, d::RULE_IS_ACTIVE]);
    let f = "media.schema.json";
    for table in d::MediaTable::ALL {
        known(f, &[], &[table.key()]);
    }
    known(f, &["speech", "*"], &[d::SPEECH_TEXT, d::SPEECH_FILE, d::SPEECH_DURATION, d::SPEECH_FACE]);
    known("mod.schema.json", &[], &[d::MOD_MISSIONS]);
}

#[test]
fn the_keyed_task_list_is_the_engines() {
    assert_eq!(d::list_key(d::MissionList::Tasks), Some(d::TASK_NAME));
    assert_eq!(d::list_key(d::MissionList::Ships), Some(d::SHIP_ID));
    assert_eq!(d::list_key(d::MissionList::Rules), Some(d::RULE_NAME));
    assert_eq!(d::list_key(d::MissionList::Points), Some(d::POINT_NAME));
}
