//! T7 (docs/phase5.md P3): the mission plan's writers (`mission::geometry`) on synthetic missions made here (no game content): add,
//! delete with refusals, rename that follows rule references in one undo step, vertex edits that keep a nebula's polygon equal to the
//! map's, move and turn, and a retail mission changed by the mod and saved as a diff.

use openprocyon::authoring::read_merged;
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::geometry::{self as geo, Item, Kind};
use openprocyon_editor::mission::plan;
use openprocyon_editor::mission::project::{merged_map, merged_mission, open_mission};
use openprocyon_editor::mission::refs;
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ID: &str = "m";
const MISSION: &str = "maps/m/mission.json";
const MAP: &str = "maps/m/map.json";

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn read(root: &Path, rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root.join(rel)).unwrap()).unwrap()
}

fn schema_type(role: &str, kind: &str) -> (String, String) {
    let s = openprocyon::authoring::rule_schema();
    s["types"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["role"] == role)
        .find_map(|t| {
            let f = t["fields"].as_array()?.iter().find(|f| f["kind"] == kind)?;
            Some((t["name"].as_str()?.to_string(), f["name"].as_str()?.to_string()))
        })
        .unwrap_or_else(|| panic!("no {role} with a {kind} field"))
}

fn mission() -> Value {
    let (ct, cf) = schema_type("condition", "volume");
    let (at, af) = schema_type("action", "path");
    json!({
        "players": ["P1"],
        "ships": [{"id": 1, "name": "One", "pos": [10.0, 20.0, 5.0], "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1]}, {"id": 2, "pos": [0, 0, 0]}],
        "player_start": {"name": "P1", "start": [0, 0, 0], "forward": [0, 1, 0]},
        "points": [{"Name": "Gate", "Position": [1.0, 2.0, 3.0]}],
        "map_text": [{"name": "L", "text": "Here", "pos": [5, 5], "visible": true}],
        "volumes": {"Area": [[0, 0], [100, 0], [100, 100]], "Other": [[0, 0], [1, 0], [1, 1]], "Neb": [[0, 0], [50, 0], [50, 50]]},
        "paths": {"Run": [[0, 0, 0], [100, 0, 10]]},
        "environment": {
            "point_sets": {"Spawn": [{"pos": [1, 1, 0]}, {"pos": [2, 2, 0]}]},
            "nebulae": [{"New Nebula Name": "Cloud", "Polygon Name": "Neb", "Nebula Cloud Effect Name": "Nebula", "Nebula Occlusion On/Off": "FALSE"}]
        },
        "rules": [
            {"name": "r1", "conditions": [{"Type": ct, (cf.as_str()): "Area"}], "actions": [{"Type": at, (af.as_str()): "Run"}]},
            {"name": "r2", "conditions": [{"Type": ct, (cf.as_str()): "Other"}], "actions": []}
        ],
        "tasks": [{"Name": "T", "complete_when": [{"Type": ct, (cf.as_str()): "Area"}]}]
    })
}

fn map() -> Value {
    json!({"name": "M", "world_half": 500.0, "nebulae": [{"name": "Cloud", "polygon": [[0, 0], [50, 0], [50, 50]]}]})
}

fn setup(name: &str) -> (PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-t7-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    put(&root, MISSION, &mission());
    put(&root, MAP, &map());
    put(&root, "maps/m/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    let mut p = Project::open(&root, None).unwrap();
    open_mission(&mut p, ID).unwrap();
    (root, p)
}

fn mis(p: &Project) -> &Value {
    merged_mission(p, ID).unwrap()
}

fn mp(p: &Project) -> &Value {
    merged_map(p, ID).unwrap()
}

/// Both documents as they are now, to compare with after an undo.
fn snap(p: &Project) -> (Value, Value) {
    (mis(p).clone(), mp(p).clone())
}

#[test]
fn add_each_kind_is_one_undo_step() {
    let (_r, mut p) = setup("add");
    let before = snap(&p);
    for (kind, at) in [(Kind::Volume, [10.0, 10.0]), (Kind::Path, [0.0, 50.0]), (Kind::PointSet, [3.0, 3.0]), (Kind::Point, [4.0, 4.0]), (Kind::MapText, [5.0, 5.0])] {
        let name = geo::free_name(mis(&p), kind);
        let was = snap(&p);
        geo::add(&mut p, ID, kind, &name, at).unwrap();
        assert!(geo::names(mis(&p), kind).contains(&name), "{kind:?}");
        p.undo();
        assert_eq!(snap(&p), was, "{kind:?}: one undo takes it back");
        geo::add(&mut p, ID, kind, &name, at).unwrap();
    }
    let m = mis(&p);
    assert_eq!(mdoc::mission_volume(m, "Volume1").unwrap().len(), 3);
    assert_eq!(mdoc::path_points(m, "Path1").unwrap().len(), 2);
    assert_eq!(mdoc::point_set_positions(m, "PointSet1").unwrap(), vec![[3.0, 3.0, 0.0]]);
    assert_eq!(mdoc::points(m).len(), 2);
    assert_eq!(mdoc::map_text(m).len(), 2);
    assert_ne!(snap(&p), before);
    // refusals: empty and taken names
    assert!(geo::add(&mut p, ID, Kind::Volume, " ", [0.0, 0.0]).is_err());
    assert!(geo::add(&mut p, ID, Kind::Volume, "Area", [0.0, 0.0]).is_err());
    assert!(geo::add(&mut p, ID, Kind::Point, "Gate", [0.0, 0.0]).is_err());
}

#[test]
fn adding_to_a_mission_without_the_tables_makes_them() {
    let root = std::env::temp_dir().join(format!("ope-t7-bare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    put(&root, MISSION, &json!({"ships": [], "rules": []}));
    let mut p = Project::open(&root, None).unwrap();
    open_mission(&mut p, ID).unwrap();
    geo::add(&mut p, ID, Kind::Volume, "V", [0.0, 0.0]).unwrap();
    geo::add(&mut p, ID, Kind::PointSet, "S", [0.0, 0.0]).unwrap();
    geo::add(&mut p, ID, Kind::Point, "P", [0.0, 0.0]).unwrap();
    geo::add(&mut p, ID, Kind::MapText, "T", [0.0, 0.0]).unwrap();
    let m = mis(&p);
    assert!(mdoc::has_volume(m, "V") && mdoc::point_set_positions(m, "S").is_some());
    assert_eq!((mdoc::points(m).len(), mdoc::map_text(m).len()), (1, 1));
}

#[test]
fn delete_is_refused_while_something_reads_it() {
    let (_r, mut p) = setup("delete");
    let e = geo::delete(&mut p, ID, Kind::Volume, "Area").unwrap_err();
    assert!(e.contains("used by 2"), "{e}");
    assert!(geo::delete(&mut p, ID, Kind::Path, "Run").is_err());
    assert!(geo::delete(&mut p, ID, Kind::Volume, "Neb").unwrap_err().contains("used by 1"), "a nebula's polygon stays");
    assert!(geo::delete(&mut p, ID, Kind::Volume, "Nope").is_err());
    let before = snap(&p);
    geo::delete(&mut p, ID, Kind::PointSet, "Spawn").unwrap();
    geo::delete(&mut p, ID, Kind::Point, "Gate").unwrap();
    geo::delete(&mut p, ID, Kind::MapText, "L").unwrap();
    assert!(mdoc::point_set_names(mis(&p)).is_empty() && mdoc::points(mis(&p)).is_empty() && mdoc::map_text(mis(&p)).is_empty());
    p.undo();
    p.undo();
    p.undo();
    assert_eq!(snap(&p), before);
    // a volume a rule reads stays
    geo::delete(&mut p, ID, Kind::Volume, "Other").unwrap_err();
}

#[test]
fn rename_follows_every_reference_in_one_step() {
    let (_r, mut p) = setup("rename");
    let before = snap(&p);
    let n = refs::references_of(mis(&p), "volume", "Area").len();
    assert_eq!(n, 2);
    geo::rename(&mut p, ID, Kind::Volume, "Area", "Door").unwrap();
    let m = mis(&p);
    assert!(refs::references_of(m, "volume", "Area").is_empty());
    assert_eq!(refs::references_of(m, "volume", "Door").len(), n);
    assert_eq!(mdoc::volume_names(m), ["Door", "Other", "Neb"], "same place in the file");
    assert_eq!(mdoc::mission_volume(m, "Door").unwrap().len(), 3);
    assert_eq!(refs::references_of(m, "volume", "Other").len(), 1, "others untouched");
    assert_eq!(p.undo_label(), Some("Rename volume Area to Door"));
    p.undo();
    assert_eq!(snap(&p), before, "ONE undo takes the definition and every reference back");
    p.redo();
    assert_eq!(mdoc::volume_names(mis(&p))[0], "Door");
    p.undo();

    // a nebula's polygon name follows
    geo::rename(&mut p, ID, Kind::Volume, "Neb", "Fog").unwrap();
    assert_eq!(mdoc::record_polygon_name(&mdoc::mission_nebulae(mis(&p))[0]), Some("Fog"));
    p.undo();

    // a path follows the rules too, a point and a label by their list entry
    geo::rename(&mut p, ID, Kind::Path, "Run", "Dash").unwrap();
    assert_eq!(refs::references_of(mis(&p), "path", "Dash").len(), 1);
    geo::rename(&mut p, ID, Kind::Point, "Gate", "Door2").unwrap();
    assert_eq!(mdoc::point_name(&mdoc::points(mis(&p))[0]), Some("Door2"));
    geo::rename(&mut p, ID, Kind::MapText, "L", "Label").unwrap();
    geo::rename(&mut p, ID, Kind::PointSet, "Spawn", "Arrive").unwrap();
    assert_eq!(mdoc::point_set_names(mis(&p)), ["Arrive"]);

    // refusals
    assert!(geo::rename(&mut p, ID, Kind::Volume, "Other", "Neb").is_err(), "taken");
    assert!(geo::rename(&mut p, ID, Kind::Volume, "Other", "").is_err(), "empty");
    assert!(geo::rename(&mut p, ID, Kind::Volume, "Nope", "X").is_err());
    assert!(geo::rename(&mut p, ID, Kind::Volume, "Other", "Other").is_ok(), "same name changes nothing");
}

#[test]
fn a_volume_edit_keeps_the_nebulas_polygon_equal() {
    let (_r, mut p) = setup("nebula");
    let before = snap(&p);
    let it = Item::Vertex(mdoc::GeoTable::Volumes, "Neb".into(), 1);
    geo::move_items(&mut p, ID, &[it], [10.0, 5.0]).unwrap();
    assert_eq!(mdoc::mission_volume(mis(&p), "Neb").unwrap()[1], [60.0, 5.0]);
    let poly = openprocyon_editor::map::doc::nebula_polygon(&openprocyon_editor::map::doc::nebulae(mp(&p))[0]);
    assert_eq!(poly[1], [60.0, 5.0], "the map's polygon follows");
    p.undo();
    assert_eq!(snap(&p), before, "both files, one step");

    geo::insert_vertex(&mut p, ID, Kind::Volume, "Neb", 2).unwrap();
    assert_eq!(openprocyon_editor::map::doc::nebula_polygon(&openprocyon_editor::map::doc::nebulae(mp(&p))[0]).len(), 4);
    geo::remove_vertex(&mut p, ID, Kind::Volume, "Neb", 3).unwrap();
    assert_eq!(openprocyon_editor::map::doc::nebula_polygon(&openprocyon_editor::map::doc::nebulae(mp(&p))[0]).len(), 3);
    assert!(geo::remove_vertex(&mut p, ID, Kind::Volume, "Neb", 0).is_err(), "a volume keeps three points");

    // a volume no nebula uses touches only the mission
    let map_before = mp(&p).clone();
    geo::insert_vertex(&mut p, ID, Kind::Volume, "Other", 0).unwrap();
    assert_eq!(mp(&p), &map_before);
}

#[test]
fn vertices_of_paths_and_point_sets() {
    let (_r, mut p) = setup("vertices");
    geo::insert_vertex(&mut p, ID, Kind::Path, "Run", 1).unwrap();
    let pts = mdoc::path_points(mis(&p), "Run").unwrap();
    assert_eq!(pts.len(), 3);
    assert_eq!(pts[2], [150.0, 0.0, 10.0], "past the end: 50 units further along, z kept");
    geo::set_z(&mut p, ID, mdoc::GeoTable::Paths, "Run", 2, 99.0).unwrap();
    assert_eq!(mdoc::path_points(mis(&p), "Run").unwrap()[2][2], 99.0);
    assert!(geo::set_z(&mut p, ID, mdoc::GeoTable::Volumes, "Area", 0, 1.0).is_err());
    geo::remove_vertex(&mut p, ID, Kind::Path, "Run", 2).unwrap();
    assert!(geo::remove_vertex(&mut p, ID, Kind::Path, "Run", 1).is_err(), "a path keeps two points");
    geo::insert_vertex(&mut p, ID, Kind::PointSet, "Spawn", 1).unwrap();
    assert_eq!(mdoc::point_set_positions(mis(&p), "Spawn").unwrap().len(), 3);
}

#[test]
fn move_and_turn_are_one_step_each() {
    let (_r, mut p) = setup("move");
    let before = snap(&p);
    let items = [
        Item::Ship(0),
        Item::Start,
        Item::Point(0),
        Item::Text(0),
        Item::Vertex(mdoc::GeoTable::PointSets, "Spawn".into(), 0),
        Item::Vertex(mdoc::GeoTable::Paths, "Run".into(), 1),
    ];
    geo::move_items(&mut p, ID, &items, [5.0, -5.0]).unwrap();
    let m = mis(&p);
    assert_eq!(mdoc::ship_pos(&mdoc::ships(m)[0]), Some([15.0, 15.0, 5.0]));
    assert_eq!(mdoc::player_start_pos(m), Some([5.0, -5.0, 0.0]));
    assert_eq!(mdoc::point_position(&mdoc::points(m)[0]), Some([6.0, -3.0, 3.0]));
    assert_eq!(mdoc::map_text_pos(&mdoc::map_text(m)[0]), Some([10.0, 0.0]));
    assert_eq!(mdoc::point_set_positions(m, "Spawn").unwrap()[0], [6.0, -4.0, 0.0]);
    assert_eq!(mdoc::path_points(m, "Run").unwrap()[1], [105.0, -5.0, 10.0]);
    p.undo();
    assert_eq!(snap(&p), before);

    geo::turn_items(&mut p, ID, &[Item::Ship(0), Item::Start], std::f64::consts::FRAC_PI_2).unwrap();
    let m = mis(&p);
    let f = mdoc::player_start_forward(m).unwrap();
    assert!((f[0] - 1.0).abs() < 1e-5 && f[1].abs() < 1e-5, "the start now faces +x: {f:?}");
    assert!((geo::ship_heading(m, 0) - std::f64::consts::FRAC_PI_2).abs() < 1e-4);
    p.undo();
    assert_eq!(snap(&p), before);
    assert!(geo::move_items(&mut p, ID, &items, [0.0, 0.0]).is_ok());
    assert_eq!(snap(&p), before, "a zero move writes nothing");
}

#[test]
fn text_and_visibility() {
    let (_r, mut p) = setup("text");
    geo::set_map_text(&mut p, ID, 0, "There").unwrap();
    geo::set_map_text_visible(&mut p, ID, 0, false).unwrap();
    let t = &mdoc::map_text(mis(&p))[0];
    assert_eq!((mdoc::map_text_text(t), mdoc::map_text_visible(t)), (Some("There"), false));
}

#[test]
fn the_scene_follows_the_document() {
    let (_r, mut p) = setup("scene");
    let a = plan::build_scene(mis(&p));
    geo::add(&mut p, ID, Kind::Volume, "V", [0.0, 0.0]).unwrap();
    let b = plan::build_scene(mis(&p));
    assert_eq!(b.items.len(), a.items.len() + 3);
    p.undo();
    assert_eq!(plan::build_scene(mis(&p)), a);
}

#[test]
fn a_retail_mission_changed_by_the_mod_is_saved_as_a_diff() {
    let root = std::env::temp_dir().join(format!("ope-t7-retail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First", "world_half": 500.0}));
    put(&assets, "maps/mission01/mission.json", &mission());
    put(&assets, "maps/mission01/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_mission(&mut p, "mission01").unwrap();

    // adding a key to an object table is a merge; removing one has no way to be said
    geo::add(&mut p, "mission01", Kind::Volume, "Mine", [0.0, 0.0]).unwrap();
    geo::move_items(&mut p, "mission01", &[Item::Vertex(mdoc::GeoTable::Paths, "Run".into(), 1)], [1.0, 2.0]).unwrap();
    assert!(geo::delete(&mut p, "mission01", Kind::Volume, "Mine").unwrap_err().contains("extends"));
    assert!(geo::rename(&mut p, "mission01", Kind::Volume, "Other", "Other2").is_err());
    // an unkeyed list cannot take an entry on an extending mission; a keyed one can
    assert!(geo::add(&mut p, "mission01", Kind::MapText, "Z", [0.0, 0.0]).is_err());
    geo::add(&mut p, "mission01", Kind::Point, "New", [1.0, 1.0]).unwrap();
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    let file = read(&m, "maps/mission01/mission.json");
    assert_eq!(file["extends"], "mission01");
    assert!(file.get("ships").is_none() && file.get("rules").is_none(), "only what changed is written: {file}");
    assert!(file["volumes"].get("Mine").is_some() && file["volumes"].get("Area").is_none());
    let merged = read_merged(&p.root, "maps/mission01/mission.json").unwrap();
    assert_eq!(&merged, merged_mission(&p, "mission01").unwrap());
    assert_eq!(mdoc::points(&merged).len(), 2);
}

#[test]
fn a_vertex_is_added_at_the_end_with_none_selected_and_moved_by_typed_numbers() {
    let (_r, mut p) = setup("append");
    let before = snap(&p);
    geo::append_vertex(&mut p, ID, Kind::Volume, "Area").unwrap();
    assert_eq!(mdoc::mission_volume(mis(&p), "Area").unwrap().len(), 4, "a volume grows by one corner");
    p.undo();
    assert_eq!(snap(&p), before, "one undo step");
    geo::append_vertex(&mut p, ID, Kind::Path, "Run").unwrap();
    assert_eq!(mdoc::path_points(mis(&p), "Run").unwrap().len(), 3, "a path grows at its end");
    geo::append_vertex(&mut p, ID, Kind::PointSet, "Spawn").unwrap();
    assert!(geo::append_vertex(&mut p, ID, Kind::Volume, "Nope").is_err());
    assert!(geo::append_vertex(&mut p, ID, Kind::Point, "Gate").is_err(), "a point has no vertices");
    // the x/y boxes move the vertex by the difference to what they were typed to
    let v = Item::Vertex(mdoc::GeoTable::Paths, "Run".into(), 1);
    let at = geo::item_xyz(mis(&p), &v).unwrap();
    geo::move_items(&mut p, ID, std::slice::from_ref(&v), [250.0 - at[0], 0.0]).unwrap();
    let now = geo::item_xyz(mis(&p), &v).unwrap();
    assert_eq!((now[0], now[1], now[2]), (250.0, at[1], at[2]));
}
