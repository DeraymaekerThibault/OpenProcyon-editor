//! Nebulae and mines of a map (docs/phase4.md P6, work item T8) on synthetic data made here (no game content): the three-record
//! writer (`map.json` entry, `mission.json` record, `mission.json` volume) as one undo step, polygon edits, the form's fields,
//! rename and delete, the refusals on a map that extends another, saved diffs, and mines.

use openprocyon_editor::map::doc::{self, Array, NebulaFlag};
use openprocyon_editor::map::mines;
use openprocyon_editor::map::nebula::{self, Link};
use openprocyon_editor::map::project::{Change, change_entries, delete_entry, merged_view, new_map, open_map};
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

const SQ: [[f64; 2]; 4] = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];

/// The retail map `red`, with a nebula `Clarke`, its mission record and volume; and `plain`, a retail map with a mission that has
/// no nebula and `skirm`, a retail map with no mission. The import has a bullet table with two mine bullets.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-map-nebula-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(
        &assets,
        "maps/red/map.json",
        &json!({"name": "Red", "world_half": 1500.0,
            "objects": [{"type": "Isle_A", "id": 1, "pos": [0.0, 0.0, 0.0]}],
            "mines": [{"type": "Bullet_StarMine_Small", "id": 2, "pos": [5.0, 5.0, 0.0]}],
            "nebulae": [{"name": "Clarke", "polygon": [[0, 0], [50, 0], [50, 50]], "cloud": {"effect": "Nebula", "points": [{"pos": [10, 10, 0], "magnitude": 30}], "drain": true, "occlusion": true}, "storm": {"effect": "EFFECT", "points": []}, "meteors": {"on": false, "points": []}, "sound_distance": 300.0}]}),
    );
    put(
        &assets,
        "maps/red/mission.json",
        &json!({"environment": {"point_sets": {}, "nebulae": [{"New Nebula Name": "Clarke", "Polygon Name": "Clarke", "Nebula Occlusion On/Off": "TRUE"}]},
            "volumes": {"Clarke": [[0, 0], [50, 0], [50, 50]], "Gate": [[1, 1], [2, 2], [3, 1]]}}),
    );
    put(&assets, "maps/plain/map.json", &json!({"name": "Plain", "objects": [{"type": "Isle_A", "id": 1, "pos": [0.0, 0.0, 0.0]}]}));
    put(&assets, "maps/plain/mission.json", &json!({"environment": {"point_sets": {}}, "volumes": {"Gate": [[1, 1], [2, 2], [3, 1]]}}));
    put(&assets, "maps/skirm/map.json", &json!({"name": "Skirm", "objects": []}));
    put(&assets, "data/weapons/bullets.json", &json!({"Bullet_StarMine_Med": {}, "Bullet_StarMine_Big": {}, "Bullet_Lancer": {}}));
    std::fs::create_dir_all(&m).unwrap();
    (root, assets, m)
}

fn nebulae(p: &Project, id: &str) -> Vec<Value> {
    doc::nebulae(merged_view(p, id).unwrap()).to_vec()
}

fn mission(p: &mut Project, id: &str) -> Value {
    nebula::mission_view(p, id).unwrap().clone()
}

/// The map's polygon, the record's volume and the volume agree for nebula `name`.
fn assert_three_agree(p: &mut Project, id: &str, name: &str) {
    let n = nebulae(p, id).into_iter().find(|n| doc::nebula_name(n) == Some(name)).unwrap_or_else(|| panic!("no nebula {name}"));
    let m = mission(p, id);
    let rec = doc::mission_nebulae(&m).iter().find(|r| doc::record_name(r) == Some(name)).unwrap_or_else(|| panic!("no record {name}"));
    let pn = doc::record_polygon_name(rec).unwrap();
    assert_eq!(doc::mission_volume(&m, pn), Some(doc::nebula_polygon(&n)), "the volume {pn} follows the map's polygon");
}

#[test]
fn a_new_nebula_writes_three_records_as_one_undo_step_on_a_missions_own_map() {
    let (_root, assets, m) = setup("new3");
    let (mut p, _) = Project::create(&m, Some(assets)).unwrap();
    new_map(&mut p, "mine").unwrap();
    // a skirmish map has no mission: the map record alone
    let at = nebula::create(&mut p, "mine", "Storm", &SQ).unwrap();
    assert_eq!(at, 0);
    assert!(nebula::mission_view(&mut p, "mine").is_none());
    assert_eq!(nebula::link(&mut p, "mine", 0), Link::NoMission);
    let n = &nebulae(&p, "mine")[0];
    assert_eq!((doc::nebula_name(n), doc::nebula_polygon(n)), (Some("Storm"), SQ.to_vec()));
    assert!(NebulaFlag::Cloud.get(n) && !NebulaFlag::Storm.get(n));
    assert_eq!(doc::nebula_point_count(n, "cloud"), 1, "a cloud with no point shows nothing, so a new one has one");
    assert!(nebula::create(&mut p, "mine", "Storm", &SQ).unwrap_err().contains("already"));
    assert!(nebula::create(&mut p, "mine", " ", &SQ).is_err());
    assert!(nebula::create(&mut p, "mine", "Two", &SQ[..2]).is_err());
    p.undo();
    assert!(nebulae(&p, "mine").is_empty());

    // a mod's own mission: all three
    put(&m, "maps/own/map.json", &json!({"name": "Own", "objects": []}));
    put(&m, "maps/own/mission.json", &json!({"environment": {"point_sets": {}}, "volumes": {"Gate": [[1, 1], [2, 2], [3, 1]]}}));
    let mut p = Project::open(&m, p.import.clone()).unwrap();
    open_map(&mut p, "own").unwrap();
    let before_undo = p.undo_label().map(str::to_string);
    nebula::create(&mut p, "own", "Storm", &SQ).unwrap();
    let (n, mis) = (nebulae(&p, "own"), mission(&mut p, "own"));
    assert_eq!(n.len(), 1);
    let rec = &doc::mission_nebulae(&mis)[0];
    assert_eq!((doc::record_name(rec), doc::record_polygon_name(rec)), (Some("Storm"), Some("Storm")));
    assert_eq!(doc::mission_volume(&mis, "Storm"), Some(SQ.to_vec()));
    assert!(doc::has_volume(&mis, "Gate"), "the mission's other volumes stay");
    assert_eq!(nebula::link(&mut p, "own", 0), Link::Linked { record: 0, polygon_name: "Storm".into(), in_sync: true });
    assert_eq!(p.dirty_rels().len(), 2, "both files changed");
    assert!(nebula::create(&mut p, "own", "Gate", &SQ).unwrap_err().contains("volume"), "a volume of that name exists");
    assert_eq!(p.undo_label(), Some("New nebula Storm"));
    // ONE undo step takes back all three, ONE redo puts them back
    p.undo();
    assert!(nebulae(&p, "own").is_empty());
    assert!(doc::mission_nebulae(&mission(&mut p, "own")).is_empty());
    assert!(!doc::has_volume(&mission(&mut p, "own"), "Storm"));
    assert_eq!(p.undo_label().map(str::to_string), before_undo);
    assert!(p.dirty_rels().is_empty());
    p.redo();
    assert_three_agree(&mut p, "own", "Storm");
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    assert_eq!(read(&m, "maps/own/map.json")["nebulae"][0]["name"], "Storm");
    assert_eq!(read(&m, "maps/own/mission.json")["volumes"]["Storm"], json!([[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]));
}

#[test]
fn polygon_edits_keep_the_three_equal_and_are_one_step_each() {
    let (_root, assets, m) = setup("poly");
    put(&m, "maps/own/map.json", &json!({"name": "Own"}));
    put(&m, "maps/own/mission.json", &json!({"environment": {}, "volumes": {}}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "own").unwrap();
    nebula::create(&mut p, "own", "Storm", &SQ).unwrap();
    nebula::move_vertex(&mut p, "own", 0, 2, [120.5, 130.0]).unwrap();
    assert_eq!(p.undo_label(), Some("Move point 2 of nebula Storm"));
    nebula::insert_vertex(&mut p, "own", 0, 3, [-10.0, 50.0]).unwrap();
    assert_eq!(doc::nebula_polygon(&nebulae(&p, "own")[0]).len(), 5);
    nebula::remove_vertex(&mut p, "own", 0, 0).unwrap();
    let poly = doc::nebula_polygon(&nebulae(&p, "own")[0]);
    assert_eq!(poly, vec![[100.0, 0.0], [120.5, 130.0], [0.0, 100.0], [-10.0, 50.0]]);
    assert_three_agree(&mut p, "own", "Storm");
    // refusals change nothing
    assert!(nebula::move_vertex(&mut p, "own", 0, 9, [0.0, 0.0]).is_err());
    assert!(nebula::move_vertex(&mut p, "own", 3, 0, [0.0, 0.0]).is_err());
    nebula::remove_vertex(&mut p, "own", 0, 0).unwrap();
    assert!(nebula::remove_vertex(&mut p, "own", 0, 0).unwrap_err().contains("at least 3"));
    assert_eq!(doc::nebula_polygon(&nebulae(&p, "own")[0]).len(), 3);
    // each undo takes one edit, in both files
    p.undo();
    assert_eq!(doc::nebula_polygon(&nebulae(&p, "own")[0]).len(), 4);
    assert_three_agree(&mut p, "own", "Storm");
    p.undo();
    p.undo();
    p.undo();
    assert_eq!(doc::nebula_polygon(&nebulae(&p, "own")[0]), SQ.to_vec());
    assert_three_agree(&mut p, "own", "Storm");
    // no change, no step
    let label = p.undo_label().map(str::to_string);
    nebula::move_vertex(&mut p, "own", 0, 0, [0.0, 0.0]).unwrap();
    assert_eq!(p.undo_label().map(str::to_string), label);
}

#[test]
fn fields_rename_and_delete_follow_into_the_mission() {
    let (_root, assets, m) = setup("fields");
    put(&m, "maps/own/map.json", &json!({"name": "Own"}));
    put(&m, "maps/own/mission.json", &json!({"environment": {}, "volumes": {}, "rules": [{"name": "r", "area": "Other"}]}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "own").unwrap();
    nebula::create(&mut p, "own", "Storm", &SQ).unwrap();
    nebula::set_flag(&mut p, "own", 0, NebulaFlag::Storm, true).unwrap();
    nebula::set_flag(&mut p, "own", 0, NebulaFlag::Lightning, true).unwrap();
    nebula::set_flag(&mut p, "own", 0, NebulaFlag::Occlusion, true).unwrap();
    nebula::set_flag(&mut p, "own", 0, NebulaFlag::Cloud, false).unwrap();
    nebula::set_sound_distance(&mut p, "own", 0, 450.0).unwrap();
    let n = nebulae(&p, "own")[0].clone();
    assert!(NebulaFlag::Storm.get(&n) && NebulaFlag::Lightning.get(&n) && NebulaFlag::Occlusion.get(&n) && !NebulaFlag::Cloud.get(&n));
    assert_eq!(doc::nebula_point_count(&n, "storm"), 1, "an effect switched on gets a point to show");
    assert_eq!(doc::nebula_sound_distance(&n), 450.0);
    let mis = mission(&mut p, "own");
    let rec = &doc::mission_nebulae(&mis)[0];
    assert!(doc::record_occlusion(rec), "the mission's occlusion flag follows");
    assert_eq!(rec["Nebula Cloud Effect Name"], "EFFECT");
    nebula::set_flag(&mut p, "own", 0, NebulaFlag::Occlusion, false).unwrap();
    assert!(!doc::record_occlusion(&doc::mission_nebulae(&mission(&mut p, "own"))[0]));
    // rename takes the record and the volume with it
    assert!(nebula::rename(&mut p, "own", 0, " ").is_err());
    nebula::rename(&mut p, "own", 0, "Tempest").unwrap();
    assert_three_agree(&mut p, "own", "Tempest");
    let mis = mission(&mut p, "own");
    assert!(doc::has_volume(&mis, "Tempest") && !doc::has_volume(&mis, "Storm"));
    p.undo();
    assert_three_agree(&mut p, "own", "Storm");
    assert!(doc::has_volume(&mission(&mut p, "own"), "Storm") && !doc::has_volume(&mission(&mut p, "own"), "Tempest"));
    p.redo();
    // a second nebula; a name already used is refused
    nebula::create(&mut p, "own", "Other", &SQ).unwrap();
    assert!(nebula::rename(&mut p, "own", 1, "Tempest").unwrap_err().contains("already"));
    // a volume a rule names (`Other`) keeps its name when its nebula is renamed, and stays when the nebula goes
    nebula::rename(&mut p, "own", 1, "Other2").unwrap();
    let mis = mission(&mut p, "own");
    assert_eq!(doc::record_name(&doc::mission_nebulae(&mis)[1]), Some("Other2"));
    assert_eq!(doc::record_polygon_name(&doc::mission_nebulae(&mis)[1]), Some("Other"), "the rule's volume keeps its name");
    assert!(doc::has_volume(&mis, "Other") && !doc::has_volume(&mis, "Other2"));
    assert_three_agree(&mut p, "own", "Other2");
    // delete: entry and record and (only when unused) volume, one step
    nebula::delete(&mut p, "own", 1).unwrap();
    assert!(doc::has_volume(&mission(&mut p, "own"), "Other"), "a rule still names the volume");
    nebula::delete(&mut p, "own", 0).unwrap();
    assert!(nebulae(&p, "own").is_empty());
    let mis = mission(&mut p, "own");
    assert!(doc::mission_nebulae(&mis).is_empty() && !doc::has_volume(&mis, "Storm"));
    assert_eq!(p.undo_label(), Some("Delete nebula Tempest"));
    p.undo();
    assert_three_agree(&mut p, "own", "Tempest");
}

#[test]
fn a_retail_nebula_edits_as_a_diff_and_a_new_one_is_refused_where_the_missions_list_is_not_keyed() {
    let (_root, assets, m) = setup("retail");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_map(&mut p, "red").unwrap();
    assert_eq!(nebula::link(&mut p, "red", 0), Link::Linked { record: 0, polygon_name: "Clarke".into(), in_sync: true });
    // a drag of a retail polygon: the map entry (keyed) and the mission's volume, as a diff
    nebula::move_vertex(&mut p, "red", 0, 2, [60.0, 70.0]).unwrap();
    assert_three_agree(&mut p, "red", "Clarke");
    nebula::set_sound_distance(&mut p, "red", 0, 500.0).unwrap();
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    let (map_file, mission_file) = (read(&m, "maps/red/map.json"), read(&m, "maps/red/mission.json"));
    let text = map_file.to_string();
    assert!(!text.contains("Isle_A") && !text.contains("StarMine"), "nothing of the retail map is restated: {text}");
    assert!(text.contains("/nebulae/0/sound_distance") && text.contains("/nebulae/0/polygon/2/0"), "{text}");
    assert!(!mission_file.to_string().contains("Gate"), "the retail volumes are not restated: {mission_file}");
    assert!(mission_file["volumes"]["Clarke"].is_array() && !mission_file.to_string().contains("New Nebula Name"), "only the volume is written: {mission_file}");
    // a fresh project reads both files back as the same documents
    let (want_map, want_mis) = (merged_view(&p, "red").unwrap().clone(), mission(&mut p, "red"));
    drop(p);
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_map(&mut p, "red").unwrap();
    assert_eq!((merged_view(&p, "red").unwrap().clone(), mission(&mut p, "red")), (want_map, want_mis));
    // a switch the mission's record mirrors would restate the retail list of records: refused, nothing changed
    let err = nebula::set_flag(&mut p, "red", 0, NebulaFlag::Occlusion, false).unwrap_err();
    assert!(err.contains("restating"), "{err}");
    assert!(NebulaFlag::Occlusion.get(&nebulae(&p, "red")[0]));
    assert!(nebula::rename(&mut p, "red", 0, "Other").unwrap_err().contains("restating"));
    nebula::set_flag(&mut p, "red", 0, NebulaFlag::Lightning, true).unwrap();
    p.undo();
    // a new nebula needs a record in the mission's unkeyed list: refused with the project's reason, and NOTHING changed
    let label = p.undo_label().map(str::to_string);
    let err = nebula::create(&mut p, "red", "Fresh", &SQ).unwrap_err();
    assert!(err.contains("mission") && err.contains("not merged by key"), "{err}");
    assert_eq!(nebulae(&p, "red").len(), 1);
    assert_eq!(p.undo_label().map(str::to_string), label);
    assert!(p.dirty_rels().is_empty());
    let err = nebula::delete(&mut p, "red", 0).unwrap_err();
    assert!(err.contains("not merged by key"), "{err}");
    assert_eq!(nebulae(&p, "red").len(), 1);
    // a retail mission with no nebula records yet takes one: the list is all ours
    open_map(&mut p, "plain").unwrap();
    let at = nebula::create(&mut p, "plain", "Fresh", &SQ).unwrap();
    assert_eq!(at, 0);
    assert_three_agree(&mut p, "plain", "Fresh");
    // a retail map with no mission: the map entry alone, a keyed add
    open_map(&mut p, "skirm").unwrap();
    nebula::create(&mut p, "skirm", "Gale", &SQ).unwrap();
    assert_eq!(nebula::link(&mut p, "skirm", 0), Link::NoMission);
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    let plain = read(&m, "maps/plain/map.json");
    assert_eq!(plain["nebulae"][0]["name"], "Fresh");
    assert!(!plain.to_string().contains("Isle_A"), "{plain}");
    assert_eq!(read(&m, "maps/plain/mission.json")["volumes"]["Fresh"].as_array().unwrap().len(), 4);
    assert!(!m.join("maps/skirm/mission.json").exists());
    assert!(!read(&m, "maps/plain/mission.json").to_string().contains("Gate"));
}

#[test]
fn a_group_is_all_or_nothing() {
    let (_root, assets, m) = setup("atomic");
    put(&m, "maps/own/map.json", &json!({"name": "Own"}));
    put(&m, "maps/own/mission.json", &json!({"volumes": {"A": 5}}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "own").unwrap();
    nebula::mission_view(&mut p, "own").unwrap();
    let before = (merged_view(&p, "own").unwrap().clone(), mission(&mut p, "own"));
    use openprocyon_editor::edit::Edit;
    let bad =
        p.do_group("both", vec![("maps/own/map.json".into(), vec![Edit::set("/x", json!(1))]), ("maps/own/mission.json".into(), vec![Edit::set("/volumes/A/deeper", json!(1))])]);
    assert!(bad.is_err());
    assert_eq!((merged_view(&p, "own").unwrap().clone(), mission(&mut p, "own")), before, "the first document was put back");
    assert!(p.undo_label().is_none() && p.dirty_rels().is_empty());
    // a good group, then undo and redo twice over
    p.do_group("both", vec![("maps/own/map.json".into(), vec![Edit::set("/x", json!(1))]), ("maps/own/mission.json".into(), vec![Edit::set("/y", json!(2))])]).unwrap();
    p.do_step("maps/own/map.json", "later", vec![Edit::set("/z", json!(3))]).unwrap();
    assert_eq!(p.undo().as_deref(), Some("later"));
    assert_eq!(p.undo().as_deref(), Some("both"));
    assert!(p.undo_label().is_none(), "one undo took the whole group");
    assert_eq!((merged_view(&p, "own").unwrap().clone(), mission(&mut p, "own")), before);
    assert_eq!(p.redo().as_deref(), Some("both"));
    assert_eq!(merged_view(&p, "own").unwrap()["x"], 1);
    assert_eq!(mission(&mut p, "own")["y"], 2);
    assert_eq!(p.redo().as_deref(), Some("later"));
    assert!(p.redo_label().is_none());
}

#[test]
fn mines_are_offered_from_the_bullet_table_placed_moved_and_deleted() {
    let (_root, assets, m) = setup("mines");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    open_map(&mut p, "red").unwrap();
    // the engine's bullets of the import, and the types the map's own mines use
    assert_eq!(mines::types(&p, "red"), ["Bullet_StarMine_Big", "Bullet_StarMine_Med", "Bullet_StarMine_Small"]);
    assert!(mines::is_known(&p, "red", "Bullet_StarMine_Med") && !mines::is_known(&p, "red", "Bullet_Lancer"));
    let at = mines::place(&mut p, "red", "Bullet_StarMine_Med", [12.5, -30.0]).unwrap();
    assert_eq!(at, 1);
    let e = doc::mines(merged_view(&p, "red").unwrap())[1].clone();
    assert_eq!((doc::kind(&e), doc::id(&e), doc::pos(&e)), (Some("Bullet_StarMine_Med"), Some(3), Some([12.5, -30.0, 0.0])), "the next free id across objects, ships and mines");
    assert!(mines::place(&mut p, "red", "", [0.0, 0.0]).is_err());
    assert!(mines::place(&mut p, "red", "A B", [0.0, 0.0]).is_err());
    // a typed type the table does not know is allowed
    mines::place(&mut p, "red", "Bullet_StarMine_Custom", [1.0, 1.0]).unwrap();
    assert!(mines::types(&p, "red").contains(&"Bullet_StarMine_Custom".to_string()), "once a map uses it, it is offered");
    // move is the ordinary entry move, delete the ordinary delete: both keyed by id
    let mut ch = Change::new(Array::Mines, 0);
    ch.pos = Some([6.0, 7.0, 0.0]);
    change_entries(&mut p, "red", "Move mines 0", &[ch]).unwrap();
    delete_entry(&mut p, "red", Array::Mines, 2).unwrap();
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    let file = read(&m, "maps/red/map.json");
    let text = file.to_string();
    assert!(!text.contains("Isle_A") && !text.contains("Clarke"), "only the mine entries are written: {text}");
    assert_eq!(file["mines"].as_array().unwrap().len(), 2, "{text}");
    // a retail map whose list cannot be told apart refuses a placed mine, with the project's text
    put(&assets, "maps/blue/map.json", &json!({"name": "Blue", "mines": [{"type": "Bullet_StarMine_Med", "pos": [0, 0, 0]}]}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "blue").unwrap();
    let err = mines::place(&mut p, "blue", "Bullet_StarMine_Med", [3.0, 3.0]).unwrap_err();
    assert!(err.contains("mines"), "{err}");
    assert_eq!(doc::mines(merged_view(&p, "blue").unwrap()).len(), 1);
}
