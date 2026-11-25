//! Maps as project documents on synthetic data made here (no game content): list, open merged, new, move, undo, save as a diff,
//! the refusal of add and delete on an extending map, and the write guard. The saved files are checked against the engine's own
//! overlay, as Phase 3's diff tests are.

use openprocyon::authoring::read_merged;
use openprocyon_editor::map::doc::{self, Array};
use openprocyon_editor::map::project::{add_entry, delete_entry, list_maps, map_rel, merged_view, move_entry, new_map, open_map};
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

fn red() -> Value {
    json!({
        "name": "Red", "world_half": 1500.0,
        "objects": [
            {"type": "Isle_A", "id": 1, "pos": [0.0, 0.0, 0.0], "scene": "Isle_A"},
            {"type": "Rock_B", "id": 2, "pos": [10.0, 20.0]},
            {"type": "Rock_B", "id": 3, "pos": [30.0, 40.0, 5.0]}
        ],
        "players": [{"name": "P1", "start": [100.0, 0.0, 0.0], "playable": true}]
    })
}

/// (root, import, mod folder): the import has maps `red` and `blue`, the mod folder is empty.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-map-project-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/red/map.json", &red());
    put(&assets, "maps/blue/map.json", &json!({"name": "Blue", "objects": []}));
    std::fs::create_dir_all(&m).unwrap();
    (root, assets, m)
}

fn files_under(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(d: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.push((p.strip_prefix(root).unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn map_ids_are_plain_folder_names() {
    assert_eq!(map_rel("red").unwrap(), "maps/red/map.json");
    for bad in ["", "../x", "a/b", ".hidden", "a b", "a\\b"] {
        assert!(map_rel(bad).is_err(), "{bad}");
    }
}

#[test]
fn lists_the_mods_maps_and_the_imports() {
    let (_root, assets, m) = setup("list");
    put(&m, "maps/red/map.json", &json!({"extends": "red", "world_half": 9.0}));
    put(&m, "maps/mine/map.json", &json!({"name": "Mine"}));
    put(&m, "maps/empty/notes.txt", &json!(null));
    let p = Project::open(&m, Some(assets)).unwrap();
    let l = list_maps(&p);
    let row = |id: &str| l.iter().find(|e| e.id == id).unwrap_or_else(|| panic!("{id} in {l:?}")).clone();
    assert_eq!(l.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["blue", "mine", "red"], "sorted, folders without a map file left out");
    assert_eq!((row("red").in_mod, row("red").in_import, row("red").name.as_deref()), (true, true, Some("Red")), "the retail name shows when the mod's file has none");
    assert_eq!((row("blue").in_mod, row("blue").in_import), (false, true));
    assert_eq!((row("mine").in_mod, row("mine").in_import, row("mine").name.as_deref()), (true, false, Some("Mine")));
}

#[test]
fn a_retail_map_opens_as_the_import_has_it_and_a_mod_file_opens_merged() {
    let (_root, assets, m) = setup("open");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_map(&mut p, "red").unwrap();
    assert_eq!(merged_view(&p, "red"), Some(&red()));
    assert!(!p.doc("maps/red/map.json").unwrap().dirty);
    assert!(open_map(&mut p, "nope").is_err());
    assert!(open_map(&mut p, "../red").is_err());
    drop(p);
    // a hand-written mod file over the retail one: the engine's overlay is what is shown
    put(&m, "maps/red/map.json", &json!({"extends": "red", "world_half": 9.0, "set": {"/objects/0/pos": [1.0, 1.0, 1.0]}}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "red").unwrap();
    let v = merged_view(&p, "red").unwrap();
    assert_eq!((doc::world_half(v), doc::pos(&doc::objects(v)[0]), doc::objects(v).len()), (Some(9.0), Some([1.0, 1.0, 1.0]), 3));
}

#[test]
fn moves_are_set_pointers_in_an_extends_diff_undone_and_saved_through_the_overlay() {
    let (_root, assets, m) = setup("move");
    let before_import = files_under(&assets);
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    move_entry(&mut p, "red", Array::Objects, 1, [11.5, 22.5, 99.0]).unwrap();
    move_entry(&mut p, "red", Array::Objects, 2, [31.0, 41.0, 6.0]).unwrap();
    move_entry(&mut p, "red", Array::Players, 0, [101.0, 1.0, 0.0]).unwrap();
    assert_eq!(p.undo_label(), Some("Move players #0"));
    let v = merged_view(&p, "red").unwrap();
    assert_eq!(doc::pos(&doc::objects(v)[1]), Some([11.5, 22.5, 0.0]), "a two-number pos stays two numbers");
    assert_eq!(doc::player_start(&doc::players(v)[0]), Some([101.0, 1.0, 0.0]));
    assert_eq!(p.dirty_rels(), ["maps/red/map.json"]);

    // undo and redo go step by step
    assert_eq!(p.undo().as_deref(), Some("Move players #0"));
    assert_eq!(p.undo().as_deref(), Some("Move objects 3"));
    assert_eq!(doc::pos(&doc::objects(merged_view(&p, "red").unwrap())[2]), Some([30.0, 40.0, 5.0]));
    assert_eq!(p.redo().as_deref(), Some("Move objects 3"));
    assert_eq!(p.redo().as_deref(), Some("Move players #0"));

    let report = p.save();
    assert!(report.errors.is_empty() && report.warnings.is_empty(), "{report:?}");
    assert_eq!(report.written, ["maps/red/map.json"]);
    let file = read(&m, "maps/red/map.json");
    assert_eq!(
        file,
        json!({"extends": "red", "set": {
            "/objects/1/pos/0": 11.5, "/objects/1/pos/1": 22.5,
            "/objects/2/pos/0": 31.0, "/objects/2/pos/1": 41.0, "/objects/2/pos/2": 6.0,
            "/players/0/start/0": 101.0, "/players/0/start/1": 1.0
        }})
    );
    // the engine's overlay agrees with the document
    let merged = read_merged(&p.root, "maps/red/map.json").unwrap();
    assert_eq!(&merged, merged_view(&p, "red").unwrap());
    assert!(!p.doc("maps/red/map.json").unwrap().dirty);
    assert!(p.save().written.is_empty(), "nothing more to write");
    // a fresh project reads the same map back
    drop(p);
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_map(&mut p, "red").unwrap();
    assert_eq!(&merged, merged_view(&p, "red").unwrap());
    assert_eq!(files_under(&assets), before_import, "the import is never written");
}

#[test]
fn moving_back_to_retail_leaves_nothing_to_save_and_deletes_the_file() {
    let (_root, assets, m) = setup("moveback");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    move_entry(&mut p, "red", Array::Objects, 0, [5.0, 5.0, 5.0]).unwrap();
    assert_eq!(p.save().written, ["maps/red/map.json"]);
    move_entry(&mut p, "red", Array::Objects, 0, [0.0, 0.0, 0.0]).unwrap();
    let r = p.save();
    assert_eq!((r.written.len(), r.deleted.as_slice()), (0, ["maps/red/map.json".to_string()].as_slice()));
    assert!(!m.join("maps").exists(), "the empty folders go with it");
}

#[test]
fn a_bad_move_changes_nothing() {
    let (_root, assets, m) = setup("badmove");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    assert!(move_entry(&mut p, "red", Array::Objects, 9, [0.0; 3]).is_err());
    assert!(move_entry(&mut p, "red", Array::Mines, 0, [0.0; 3]).is_err());
    assert!(p.dirty_rels().is_empty() && p.undo_label().is_none());
}

#[test]
fn a_new_map_is_the_mods_own_and_saves_whole_leaving_mod_json_alone() {
    let (_root, assets, m) = setup("new");
    let (mut p, _) = Project::create(&m, Some(assets.clone())).unwrap();
    let mod_json = std::fs::read(m.join("mod.json")).unwrap();
    assert!(new_map(&mut p, "red").unwrap_err().contains("retail"));
    assert!(new_map(&mut p, "a/b").is_err());
    new_map(&mut p, "skirm").unwrap();
    assert!(new_map(&mut p, "skirm").unwrap_err().contains("already"));
    let at = add_entry(&mut p, "skirm", Array::Objects, json!({"type": "Isle_A", "scene": "Isle_A", "pos": [1.0, 2.0, 3.0]})).unwrap();
    assert_eq!(at, 0);
    add_entry(&mut p, "skirm", Array::Objects, json!({"type": "Rock_B", "id": 7, "pos": [4.0, 5.0, 6.0]})).unwrap();
    add_entry(&mut p, "skirm", Array::Objects, json!({"type": "Rock_B", "pos": [0.0, 0.0, 0.0]})).unwrap();
    add_entry(&mut p, "skirm", Array::Players, json!({"name": "P1", "start": [0.0, 0.0, 0.0]})).unwrap();
    let ids: Vec<_> = doc::objects(merged_view(&p, "skirm").unwrap()).iter().map(|e| doc::id(e).unwrap()).collect();
    assert_eq!(ids, [1, 7, 2], "ids are the next free ones");
    delete_entry(&mut p, "skirm", Array::Objects, 1).unwrap();
    assert_eq!(p.undo_label(), Some("Delete objects 7"));
    move_entry(&mut p, "skirm", Array::Objects, 1, [9.0, 9.0, 9.0]).unwrap();
    assert!(!m.join("maps").exists(), "nothing is written before the save");

    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    assert_eq!(r.written, ["maps/skirm/map.json"]);
    let file = read(&m, "maps/skirm/map.json");
    assert!(file.get("extends").is_none() && file.get("set").is_none(), "a map of the mod's own is whole: {file}");
    assert_eq!(&file, merged_view(&p, "skirm").unwrap());
    assert_eq!(doc::objects(&file).len(), 2);
    assert_eq!(std::fs::read(m.join("mod.json")).unwrap(), mod_json, "mod.json is left alone");
    let l = list_maps(&p);
    assert!(l.iter().any(|e| e.id == "skirm" && e.in_mod && !e.in_import));
    // undo works after a save, and a reopened project sees the saved map
    p.undo();
    assert!(p.dirty_rels() == ["maps/skirm/map.json"]);
    drop(p);
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "skirm").unwrap();
    assert_eq!(merged_view(&p, "skirm"), Some(&file));
}

#[test]
fn add_and_delete_are_refused_on_a_map_that_extends_another_and_nothing_changes() {
    let (_root, assets, m) = setup("refuse");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    let e = add_entry(&mut p, "red", Array::Objects, json!({"type": "X", "pos": [0, 0, 0]})).unwrap_err();
    assert!(e.contains("extends another map"), "{e}");
    assert!(delete_entry(&mut p, "red", Array::Objects, 0).is_err());
    assert!(add_entry(&mut p, "blue", Array::Objects, json!({"id": 5})).is_err());
    assert!(p.dirty_rels().is_empty() && p.undo_label().is_none());
    // a move is still fine
    move_entry(&mut p, "blue", Array::Objects, 0, [0.0; 3]).unwrap_err();
    move_entry(&mut p, "red", Array::Objects, 0, [1.0, 1.0, 1.0]).unwrap();
}

#[test]
fn a_mod_inside_the_import_is_refused_by_the_guard() {
    let (_root, assets, _m) = setup("guard");
    assert!(Project::open(&assets.join("maps"), Some(assets.clone())).is_err());
    assert!(Project::create(&assets.join("mymod"), Some(assets.clone())).is_err());
    assert!(!assets.join("mymod").exists());
}
