//! The campaign order of `mission::campaign` on a synthetic mod: reorder, remove, add an existing mission, the refusals and the undo
//! step of each, and the file it saves.

use openprocyon_editor::mission::campaign::{self, Status};
use openprocyon_editor::mission::project::new_mission;
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn setup(name: &str, mod_json: Value) -> (PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-mission-campaign-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First"}));
    put(&assets, "maps/mission01/mission.json", &json!({"rules": [], "ships": []}));
    put(&assets, "maps/extra/map.json", &json!({"name": "Extra"}));
    put(&assets, "maps/extra/mission.json", &json!({"rules": [], "ships": []}));
    put(&m, "mod.json", &mod_json);
    (root, Project::open(&m, Some(assets)).unwrap())
}

fn ids(p: &Project) -> Vec<String> {
    campaign::order(p)
}

#[test]
fn reorder_remove_and_add_are_one_undo_step_each() {
    let (root, mut p) = setup("order", json!({"id": "t", "name": "T", "format": 1}));
    for id in ["a", "b", "c"] {
        new_mission(&mut p, id, None).unwrap();
    }
    assert_eq!(ids(&p), ["a", "b", "c"]);
    campaign::move_to(&mut p, 2, 0).unwrap();
    assert_eq!(ids(&p), ["c", "a", "b"]);
    campaign::move_to(&mut p, 1, 1).unwrap();
    assert!(campaign::move_to(&mut p, 0, 3).is_err() && campaign::move_to(&mut p, 5, 0).is_err());
    assert_eq!(campaign::remove_at(&mut p, 1).unwrap(), "a");
    assert_eq!(ids(&p), ["c", "b"]);
    assert_eq!(campaign::unlisted(&p), ["a"]);
    campaign::add(&mut p, "a").unwrap();
    assert_eq!(ids(&p), ["c", "b", "a"]);
    assert!(campaign::add(&mut p, "a").unwrap_err().contains("already"));
    // the steps were: move, remove, add (the move to the same place made none)
    for _ in 0..3 {
        p.undo().unwrap();
    }
    assert_eq!(ids(&p), ["a", "b", "c"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn what_cannot_be_added_and_what_is_flagged() {
    let (root, mut p) = setup("flags", json!({"id": "t", "name": "T", "format": 1, "missions": ["ghost", "mission01", "own"]}));
    new_mission(&mut p, "own2", None).unwrap();
    let rows = campaign::rows(&p);
    let status: Vec<_> = rows.iter().map(|r| (r.id.as_str(), r.status)).collect();
    assert_eq!(status, [("ghost", Status::Missing), ("mission01", Status::Retail), ("own", Status::Missing), ("own2", Status::Ok)]);
    // a retail id and an unknown id are refused; a mission only the import has (not retail named) can be added
    assert!(campaign::add(&mut p, "mission02").is_err());
    assert!(campaign::add(&mut p, "nothing").unwrap_err().contains("no mission"));
    assert_eq!(campaign::unlisted(&p), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_import_missions_that_are_not_retail_ids_can_be_listed_and_the_file_saves() {
    let (root, mut p) = setup("save", json!({"id": "t", "name": "T", "format": 1}));
    assert_eq!(ids(&p), Vec::<String>::new());
    // `extra` is in the import, not the mod: it is not "unlisted" of the mod's own, but it can be added by name
    campaign::add(&mut p, "extra").unwrap();
    assert_eq!(ids(&p), ["extra"]);
    let report = p.save();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let saved: Value = serde_json::from_slice(&std::fs::read(root.join("mod/mod.json")).unwrap()).unwrap();
    assert_eq!(saved["missions"], json!(["extra"]));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_bad_mod_json_is_a_message_not_a_panic() {
    let (root, mut p) = setup("bad", json!({"id": "t", "name": "T", "format": 1, "missions": "a"}));
    assert!(campaign::add(&mut p, "extra").unwrap_err().contains("not a list"));
    assert!(campaign::move_to(&mut p, 0, 1).is_err());
    assert!(campaign::rows(&p).is_empty());
    let _ = std::fs::remove_dir_all(root);
}
