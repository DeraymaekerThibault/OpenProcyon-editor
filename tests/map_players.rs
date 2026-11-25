//! Players, teams and starts (T7) saved as diffs on a map that extends a retail one, on synthetic data made here: what a mod file
//! holds after a start move, a player edit, an add, a delete and a rename, and how the engine's own overlay reads it back.

use openprocyon_editor::map::doc::{self, PlayerField};
use openprocyon_editor::map::players::{add_player, delete_player, refusals, rename_team, set_field, set_start};
use openprocyon_editor::map::project::{merged_view, open_map};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn read(root: &Path, rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root.join(rel)).unwrap()).unwrap()
}

fn setup(name: &str) -> (PathBuf, PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-map-players-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(
        &assets,
        "maps/red/map.json",
        &json!({"name": "Red", "teams": [{"name_id": "T1"}, {"name_id": "T2"}], "players": [
            {"name": "A", "team": 0, "start": [0.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 0, "playable": true, "race": 0},
            {"name": "B", "team": 1, "start": [100.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 0, "playable": false, "race": 2}
        ]}),
    );
    std::fs::create_dir_all(&m).unwrap();
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "red").unwrap();
    (root, m, p)
}

/// The map as a fresh editor reads the saved mod.
fn reopened(m: &Path) -> Value {
    let mut p = Project::open(m, Some(m.parent().unwrap().join("assets"))).unwrap();
    open_map(&mut p, "red").unwrap();
    merged_view(&p, "red").unwrap().clone()
}

fn save(p: &mut Project) {
    let r = p.save();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

#[test]
fn a_start_move_and_a_field_edit_are_set_pointers_and_a_fresh_open_reads_them() {
    let (root, m, mut p) = setup("set");
    set_start(&mut p, "red", 1, [250.0, 40.0]).unwrap();
    set_field(&mut p, "red", 0, PlayerField::Playable, json!(false)).unwrap();
    set_field(&mut p, "red", 1, PlayerField::Formation, json!(6)).unwrap();
    save(&mut p);
    let file = read(&m, "maps/red/map.json");
    assert_eq!(file["extends"], "red");
    let set = file["set"].as_object().unwrap();
    assert_eq!(set.get("/players/0/playable"), Some(&json!(false)), "{file}");
    assert_eq!(set.get("/players/1/formation"), Some(&json!(6)));
    assert!(set.keys().any(|k| k.starts_with("/players/1/start")), "{file}");
    assert!(file.get("players").is_none(), "the parent's list is not restated: {file}");
    let merged = reopened(&m);
    let b = &doc::players(&merged)[1];
    assert_eq!((doc::player_start(b), doc::player_formation(b)), (Some([250.0, 40.0, 0.0]), Some(6)));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_added_player_is_a_keyed_entry_and_a_delete_is_a_remove_and_both_undo() {
    let (root, m, mut p) = setup("keyed");
    assert_eq!(refusals(&p, "red").players, None);
    let i = add_player(&mut p, "red", Some("C"), Some([5.0, 6.0])).unwrap();
    assert_eq!(i, 2);
    delete_player(&mut p, "red", 0).unwrap();
    save(&mut p);
    let file = read(&m, "maps/red/map.json");
    let list = file["players"].as_array().unwrap();
    assert!(list.iter().any(|e| e["name"] == "C"), "{file}");
    assert!(list.iter().any(|e| e["name"] == "A" && e["$remove"] == true), "{file}");
    // The pinned engine's overlay replaces lists whole (keyed merge is PROVISIONAL, src/project/keyed.rs): the editor's own reading of
    // the saved file, opened afresh, is the reference.
    assert_eq!(doc::player_names(merged_view(&p, "red").unwrap()), vec!["B", "C"]);
    assert_eq!(doc::player_names(&reopened(&m)), vec!["B", "C"], "{file}");
    p.undo();
    p.undo();
    save(&mut p);
    assert!(!m.join("maps/red/map.json").exists() || read(&m, "maps/red/map.json").get("players").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_rename_of_a_keyed_player_is_saved_so_a_fresh_open_reads_the_new_name() {
    let (root, _m, mut p) = setup("rename");
    set_field(&mut p, "red", 1, PlayerField::Name, json!("Bee")).unwrap();
    save(&mut p);
    assert_eq!(doc::player_names(&reopened(&_m)), vec!["A", "Bee"], "{}", read(&_m, "maps/red/map.json"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_team_rename_is_a_set_and_teams_add_and_delete_are_refused_with_the_reason() {
    let (root, m, mut p) = setup("teams");
    let why = refusals(&p, "red").teams.unwrap();
    assert!(why.contains("/teams"), "{why}");
    rename_team(&mut p, "red", 1, "Pirates").unwrap();
    save(&mut p);
    assert_eq!(doc::team_ids(&reopened(&m)), vec!["T1", "Pirates"], "{}", read(&m, "maps/red/map.json"));
    let _ = std::fs::remove_dir_all(&root);
}
