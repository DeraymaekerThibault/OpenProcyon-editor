//! Missions as project documents on synthetic data made here (no game content): list, open merged, new mission as one undo step,
//! keyed add and delete on an extending mission saved as a diff over the import, the refusals for unkeyed lists, and the guard. Saved
//! files are checked against the engine's own overlay, as `tests/map_project.rs` does.

use openprocyon::authoring::read_merged;
use openprocyon_editor::map::doc::name as map_name;
use openprocyon_editor::mission::doc::{self, MissionList};
use openprocyon_editor::mission::project::{
    Origin, add_entry, check_add_delete, check_remove_key, delete_entry, is_retail_id, list_missions, media_rel, merged_map, merged_media, merged_mission, mission_rel,
    new_mission, open_mission,
};
use openprocyon_editor::project::{Project, keyed};
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

fn retail() -> Value {
    json!({
        "players": ["P1", "P2"], "alliances": [["P1", "P2"]],
        "ships": [
            {"id": 1, "type": "A", "group": "g", "name": "One", "pos": [0.0, 0.0, 0.0]},
            {"id": 2, "type": "B", "group": "g", "name": "Two", "pos": [5.0, 5.0, 0.0]}
        ],
        "points": [{"Name": "Gate", "Position": [1.0, 2.0, 3.0]}],
        "tasks": [{"Name": "T1", "text": "Go", "Active": true}],
        "rules": [
            {"name": "Start", "Type": "On Start", "actions": []},
            {"name": "End", "Type": "On Timer", "actions": []}
        ],
        "environment": {"animals": [{"group": "w"}]},
        "groups": {"g": [1, 2]}
    })
}

/// (root, import, mod folder): the import has `mission01` (retail id) and `skirm` (a map without a mission), the mod has a mod.json.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-mission-project-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First", "world_half": 1500.0}));
    put(&assets, "maps/mission01/mission.json", &retail());
    put(&assets, "maps/mission01/media.json", &json!({"speech": {"a": {"text": "x"}}, "messages": {}, "ship_names": {}}));
    put(&assets, "maps/skirm/map.json", &json!({"name": "Skirm", "objects": []}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
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

const M1: &str = "maps/mission01/mission.json";

#[test]
fn ids_and_paths() {
    assert_eq!(mission_rel("a").unwrap(), "maps/a/mission.json");
    assert_eq!(media_rel("a").unwrap(), "maps/a/media.json");
    for bad in ["", "../x", "a/b", ".hidden", "a b"] {
        assert!(mission_rel(bad).is_err() && media_rel(bad).is_err(), "{bad}");
    }
    for retail in ["mission01", "mission13", "tutorial", "basic_tutorial"] {
        assert!(is_retail_id(retail), "{retail}");
    }
    for own in ["mission1", "mission001", "mission1a", "gates", "tutorial2"] {
        assert!(!is_retail_id(own), "{own}");
    }
}

#[test]
fn lists_the_mods_missions_and_the_imports_with_their_origin() {
    let (_root, assets, m) = setup("list");
    put(&assets, "maps/tutorial/mission.json", &retail());
    put(&m, "maps/mission01/mission.json", &json!({"extends": "mission01", "victory": "objectives"}));
    put(&m, "maps/mine/mission.json", &retail());
    put(&m, "maps/mine/map.json", &json!({"name": "Mine"}));
    put(&m, "maps/nomission/map.json", &json!({"name": "No"}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "missions": ["mine"]}));
    let p = Project::open(&m, Some(assets)).unwrap();
    let l = list_missions(&p);
    let row = |id: &str| l.iter().find(|e| e.id == id).unwrap_or_else(|| panic!("{id} in {l:?}")).clone();
    assert_eq!(l.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["mine", "mission01", "tutorial"], "sorted, maps without a mission left out");
    assert_eq!((row("mission01").origin, row("mission01").in_mod, row("mission01").in_import), (Origin::RetailChanged, true, true));
    assert_eq!(row("mission01").name.as_deref(), Some("First"), "the retail name shows when the mod's map file has none");
    assert_eq!((row("tutorial").origin, row("tutorial").in_mod), (Origin::Retail, false));
    assert_eq!((row("mine").origin, row("mine").listed, row("mine").name.as_deref()), (Origin::Mod, true, Some("Mine")));
    assert!(!row("tutorial").listed);
}

#[test]
fn a_retail_mission_opens_as_the_import_has_it_and_a_mod_file_opens_merged() {
    let (_root, assets, m) = setup("open");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    let d = open_mission(&mut p, "mission01").unwrap();
    assert!(d.media.is_some() && d.map.is_some());
    assert_eq!(merged_mission(&p, "mission01"), Some(&retail()));
    assert_eq!(merged_media(&p, "mission01").unwrap()["speech"]["a"]["text"], "x");
    assert_eq!(map_name(merged_map(&p, "mission01").unwrap()), Some("First"));
    assert!(p.dirty_rels().is_empty());
    let l = list_missions(&p);
    let row = l.iter().find(|e| e.id == "mission01").unwrap();
    assert_eq!((row.origin, row.in_mod), (Origin::Retail, false), "opening a retail mission is no change: not '(retail, changed)'");
    assert!(open_mission(&mut p, "nope").is_err());
    assert!(open_mission(&mut p, "skirm").is_err(), "a map with no mission.json is no mission");
    assert!(open_mission(&mut p, "../x").is_err());
    drop(p);
    // a hand-written mod file over the retail one: the engine's overlay is what is shown
    put(&m, M1, &json!({"extends": "mission01", "victory": "objectives", "set": {"/ships/0/pos/0": 9.0}}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_mission(&mut p, "mission01").unwrap();
    let v = merged_mission(&p, "mission01").unwrap();
    assert_eq!((doc::victory(v), doc::ships(v)[0]["pos"][0].as_f64(), doc::ships(v).len()), (Some("objectives"), Some(9.0), 2));
}

#[test]
fn a_new_mission_is_one_undo_step_and_saves_whole_with_its_missions_entry() {
    let (_root, assets, m) = setup("new");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    // refusals change nothing
    assert!(new_mission(&mut p, "mission05", None).unwrap_err().contains("retail mission id"));
    assert!(new_mission(&mut p, "mission01", None).is_err());
    assert!(new_mission(&mut p, "a/b", None).is_err());
    assert!(new_mission(&mut p, "skirm", None).unwrap_err().contains("retail map"));
    assert!(new_mission(&mut p, "gates", Some("nomap")).unwrap_err().contains("no map"));
    assert!(p.undo_label().is_none() && p.dirty_rels().is_empty() && list_missions(&p).len() == 1);

    new_mission(&mut p, "gates", None).unwrap();
    assert_eq!(p.undo_label(), Some("New mission gates"));
    assert!(new_mission(&mut p, "gates", None).unwrap_err().contains("already"), "an unsaved one counts");
    let l = list_missions(&p);
    let g = l.iter().find(|e| e.id == "gates").unwrap();
    assert_eq!((g.origin, g.in_mod, g.listed, g.name.as_deref()), (Origin::Mod, true, true, Some("gates")));
    assert_eq!(merged_mission(&p, "gates"), Some(&json!({"players": [], "ships": [], "rules": []})));
    assert_eq!(merged_media(&p, "gates"), Some(&doc::new_media()));
    let mut dirty = p.dirty_rels();
    dirty.sort();
    assert_eq!(dirty, ["maps/gates/map.json", "maps/gates/media.json", "maps/gates/mission.json", "mod.json"]);
    assert!(!m.join("maps").exists(), "nothing is written before the save");

    // undo takes the whole mission back, redo gives it again
    assert_eq!(p.undo().as_deref(), Some("New mission gates"));
    assert!(p.dirty_rels().is_empty() && p.undo_label().is_none());
    assert!(!list_missions(&p).iter().any(|e| e.id == "gates") && merged_mission(&p, "gates").is_none());
    assert_eq!(p.doc("mod.json").unwrap().edited.get("missions"), None);
    assert_eq!(p.redo().as_deref(), Some("New mission gates"));
    assert_eq!(p.dirty_rels().len(), 4);

    add_entry(&mut p, "gates", MissionList::Rules, json!({"name": "Start", "Type": "On Start", "actions": []})).unwrap();
    let r = p.save();
    assert!(r.errors.is_empty() && r.warnings.is_empty(), "{r:?}");
    let mut written = r.written.clone();
    written.sort();
    assert_eq!(written, ["maps/gates/map.json", "maps/gates/media.json", "maps/gates/mission.json", "mod.json"]);
    let file = read(&m, "maps/gates/mission.json");
    assert!(file.get("extends").is_none(), "a mission of the mod's own is whole: {file}");
    assert_eq!(file, json!({"players": [], "ships": [], "rules": [{"name": "Start", "Type": "On Start", "actions": []}]}));
    assert_eq!(read(&m, "maps/gates/media.json"), json!({"speech": {}, "messages": {}, "ship_names": {}}));
    assert_eq!(read(&m, "maps/gates/map.json")["name"], "gates");
    assert_eq!(read(&m, "mod.json"), json!({"id": "t", "name": "T", "format": 1, "missions": ["gates"]}));
    assert_eq!(&read_merged(&p.root, "maps/gates/mission.json").unwrap(), merged_mission(&p, "gates").unwrap());

    // a second new mission is appended to the list; a fresh project reads both
    new_mission(&mut p, "second", None).unwrap();
    assert!(p.save().errors.is_empty());
    assert_eq!(read(&m, "mod.json")["missions"], json!(["gates", "second"]));
    drop(p);
    let mut p = Project::open(&m, Some(assets)).unwrap();
    assert_eq!(list_missions(&p).iter().filter(|e| e.origin == Origin::Mod && e.listed).count(), 2);
    open_mission(&mut p, "gates").unwrap();
    assert_eq!(doc::rules(merged_mission(&p, "gates").unwrap()).len(), 1);
}

#[test]
fn a_new_mission_on_an_existing_map_extends_it_or_keeps_the_mods_own() {
    let (_root, assets, m) = setup("newmap");
    put(&m, "maps/mine/map.json", &json!({"name": "Mine", "objects": [{"id": 1}]}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1, "missions": ["x", "mine"]}));
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    // from a retail map: the mission's map extends it, nothing of it is restated
    new_mission(&mut p, "raid", Some("skirm")).unwrap();
    assert_eq!(map_name(merged_map(&p, "raid").unwrap()), Some("raid"), "the mission is not listed under the retail map's name");
    assert_eq!(merged_map(&p, "raid").unwrap()["id"], "raid");
    // the mod's own map of the same id is kept, not replaced
    new_mission(&mut p, "mine", None).unwrap();
    assert_eq!(map_name(merged_map(&p, "mine").unwrap()), Some("Mine"));
    assert!(new_mission(&mut p, "other", Some("mine")).is_ok());
    assert!(new_mission(&mut p, "again", Some("again")).is_err());
    assert_eq!(p.undo_label(), Some("New mission other"));
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    assert_eq!(read(&m, "maps/raid/map.json"), json!({"extends": "skirm", "id": "raid", "name": "raid"}));
    assert_eq!(read(&m, "maps/other/map.json"), json!({"extends": "mine", "id": "other", "name": "other"}));
    assert_eq!(read(&m, "maps/mine/map.json")["name"], "Mine", "left alone");
    assert_eq!(read(&m, "mod.json")["missions"], json!(["x", "mine", "raid", "other"]), "an id already listed is not repeated");
    let raid = read_merged(&p.root, "maps/raid/map.json").unwrap();
    assert_eq!((raid["name"].as_str(), raid["objects"].as_array().map(Vec::len)), (Some("raid"), Some(0)), "the retail map's own keys come through the overlay");
    p.undo();
    assert!(p.undo_label().is_some());
}

#[test]
fn a_new_mission_needs_a_mod_json() {
    let (_root, assets, m) = setup("nomod");
    std::fs::remove_file(m.join("mod.json")).unwrap();
    let mut p = Project::open(&m, Some(assets)).unwrap();
    assert!(new_mission(&mut p, "gates", None).unwrap_err().contains("mod.json"));
    assert!(p.dirty_rels().is_empty() && p.undo_label().is_none());
    assert!(!m.join("maps").exists());
}

#[test]
fn keyed_add_and_delete_on_a_retail_mission_save_entry_by_entry_through_the_overlay() {
    let (_root, assets, m) = setup("keyed");
    let before_import = files_under(&assets);
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    assert_eq!(add_entry(&mut p, "mission01", MissionList::Rules, json!({"name": "Boom", "Type": "On Timer", "actions": []})).unwrap(), 2);
    assert_eq!(p.undo_label(), Some("Add rule Boom"));
    delete_entry(&mut p, "mission01", MissionList::Rules, 0).unwrap();
    assert_eq!(p.undo_label(), Some("Delete rule Start"));
    add_entry(&mut p, "mission01", MissionList::Ships, json!({"id": 3, "type": "C", "pos": [1.0, 1.0, 0.0]})).unwrap();
    delete_entry(&mut p, "mission01", MissionList::Ships, 1).unwrap();
    add_entry(&mut p, "mission01", MissionList::Points, json!({"Name": "Exit", "Position": [9.0, 9.0, 9.0]})).unwrap();
    let view = merged_mission(&p, "mission01").unwrap().clone();
    assert_eq!(doc::rules(&view).iter().filter_map(doc::rule_name).collect::<Vec<_>>(), ["End", "Boom"]);

    let r = p.save();
    assert!(r.errors.is_empty() && r.warnings.is_empty(), "{r:?}");
    assert_eq!(r.written, [M1]);
    let file = read(&m, M1);
    assert_eq!(
        file,
        json!({
            "extends": "mission01",
            "ships": [{"id": 2, "$remove": true}, {"id": 3, "type": "C", "pos": [1.0, 1.0, 0.0]}],
            "points": [{"Name": "Exit", "Position": [9.0, 9.0, 9.0]}],
            "rules": [{"name": "Start", "$remove": true}, {"name": "Boom", "Type": "On Timer", "actions": []}]
        }),
        "nothing of the retail mission is restated"
    );
    let (merged, warnings) = keyed::engine_extend(M1, &retail(), &file).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(merged, view);
    assert_eq!(read_merged(&p.root, M1).unwrap(), view, "the engine's overlay agrees with the document");
    // a fresh project reads the same mission and has nothing to save; undo of an add and a delete
    drop(p);
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_mission(&mut p, "mission01").unwrap();
    assert_eq!(merged_mission(&p, "mission01"), Some(&view));
    assert!(p.save().written.is_empty() && p.dirty_rels().is_empty());
    delete_entry(&mut p, "mission01", MissionList::Points, 1).unwrap();
    assert_eq!(doc::points(merged_mission(&p, "mission01").unwrap()).len(), 1);
    p.undo();
    assert_eq!(merged_mission(&p, "mission01"), Some(&view));
    assert!(p.dirty_rels().is_empty(), "undone back to what is saved");
    // everything taken back: the file goes
    let mut p2 = Project::open(&m, Some(assets.clone())).unwrap();
    open_mission(&mut p2, "mission01").unwrap();
    p2.revert_file(M1).unwrap();
    let r = p2.save();
    assert_eq!(r.deleted, [M1]);
    assert_eq!(files_under(&assets), before_import, "the import is never written");
}

#[test]
fn unkeyed_lists_refuse_add_and_delete_with_the_reason_and_nothing_changes() {
    let (_root, assets, m) = setup("refuse");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    let cases = [
        (MissionList::Tasks, json!({"Name": "T2"})),
        (MissionList::Nebulae, json!({"name": "N"})),
        (MissionList::Animals, json!({"group": "x"})),
        (MissionList::MapText, json!({"name": "L"})),
        (MissionList::Players, json!("P3")),
        (MissionList::Alliances, json!(["P1", "P3"])),
    ];
    for (list, entry) in cases {
        let e = add_entry(&mut p, "mission01", list, entry).unwrap_err();
        assert!(e.contains("not merged by key") && e.contains("restating the whole list"), "{list:?}: {e}");
        assert!(delete_entry(&mut p, "mission01", list, 0).unwrap_err().contains("not merged by key"), "{list:?}");
        assert!(check_add_delete(&mut p, "mission01", list).is_err());
    }
    // keyed lists: the key field is named, a missing or repeated key is refused
    assert_eq!(check_add_delete(&mut p, "mission01", MissionList::Ships).unwrap().as_deref(), Some("id"));
    assert_eq!(check_add_delete(&mut p, "mission01", MissionList::Rules).unwrap().as_deref(), Some("name"));
    assert_eq!(check_add_delete(&mut p, "mission01", MissionList::Points).unwrap().as_deref(), Some("Name"));
    assert!(add_entry(&mut p, "mission01", MissionList::Rules, json!({"Type": "X"})).unwrap_err().contains("needs a 'name'"));
    assert!(add_entry(&mut p, "mission01", MissionList::Rules, json!({"name": "Start"})).unwrap_err().contains("already"));
    assert!(add_entry(&mut p, "mission01", MissionList::Ships, json!({"id": 2})).unwrap_err().contains("already"));
    assert!(delete_entry(&mut p, "mission01", MissionList::Rules, 9).is_err());
    assert!(p.dirty_rels().is_empty() && p.undo_label().is_none(), "a refusal changes nothing");
    // keys cannot be removed from an extending file, but a whole file can
    assert!(check_remove_key(&p, M1).unwrap_err().contains("no way to remove a key"));
    new_mission(&mut p, "own", None).unwrap();
    assert!(check_remove_key(&p, "maps/own/mission.json").is_ok());
    assert_eq!(check_add_delete(&mut p, "own", MissionList::Tasks).unwrap(), None);
    add_entry(&mut p, "own", MissionList::Tasks, json!({"Name": "T"})).unwrap();
    add_entry(&mut p, "own", MissionList::Animals, json!({"group": "w"})).unwrap();
    assert_eq!(merged_mission(&p, "own").unwrap()["environment"], json!({"animals": [{"group": "w"}]}));
    delete_entry(&mut p, "own", MissionList::Tasks, 0).unwrap();
    assert!(doc::tasks(merged_mission(&p, "own").unwrap()).is_empty());
}

#[test]
fn a_keyed_list_that_cannot_be_told_apart_below_refuses_even_a_keyed_add() {
    let (_root, assets, m) = setup("dupes");
    let mut r = retail();
    r["rules"].as_array_mut().unwrap().push(json!({"name": "Start"}));
    put(&assets, "maps/mission01/mission.json", &r);
    let mut p = Project::open(&m, Some(assets)).unwrap();
    let e = add_entry(&mut p, "mission01", MissionList::Rules, json!({"name": "New"})).unwrap_err();
    assert!(e.contains("same 'name'") && e.contains("restating the whole list"), "{e}");
    assert!(add_entry(&mut p, "mission01", MissionList::Points, json!({"Name": "P"})).is_ok(), "other lists are fine");
}

#[test]
fn a_file_that_replaces_a_list_whole_by_its_own_merge_choice_refuses_adds() {
    let (_root, assets, m) = setup("mergefalse");
    put(&m, M1, &json!({"extends": "mission01", "merge": {"/rules": false}, "rules": [{"name": "Only"}]}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_mission(&mut p, "mission01").unwrap();
    assert_eq!(doc::rules(merged_mission(&p, "mission01").unwrap()).len(), 1);
    assert!(add_entry(&mut p, "mission01", MissionList::Rules, json!({"name": "X"})).unwrap_err().contains("not merged by key"));
    add_entry(&mut p, "mission01", MissionList::Points, json!({"Name": "P"})).unwrap();
    p.save();
    assert_eq!(read(&m, M1)["merge"], json!({"/rules": false}), "the choice stays in the file");
}

#[test]
fn a_mod_inside_the_import_is_refused_by_the_guard() {
    let (_root, assets, _m) = setup("guard");
    assert!(Project::open(&assets.join("maps"), Some(assets.clone())).is_err());
    assert!(Project::create(&assets.join("mymod"), Some(assets.clone())).is_err());
}

#[test]
fn an_edited_retail_mission_is_listed_as_changed() {
    let (_root, assets, m) = setup("listedit");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_mission(&mut p, "mission01").unwrap();
    add_entry(&mut p, "mission01", MissionList::Ships, json!({"id": 3, "type": "C", "pos": [1.0, 1.0, 0.0]})).unwrap();
    let row = list_missions(&p).into_iter().find(|e| e.id == "mission01").unwrap();
    assert_eq!((row.origin, row.in_mod), (Origin::RetailChanged, true), "an edit makes it the mod's");
}
