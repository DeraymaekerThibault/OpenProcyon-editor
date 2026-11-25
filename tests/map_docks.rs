//! Dock points and crew places of a mod's own scenery (docs/phase4.md P9, work item T12) on synthetic data made here (no game
//! content): what is written, one undo step per action, retail types refused, and that the ENGINE loads what was saved
//! (`docking::dockable_for`, `crew_nodes::load_scenery`, `authoring::check_mod`).

use bevy::math::{Quat, Vec3};
use openprocyon_editor::map::docks::{self, Action, Kind};
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

/// A synthetic import with a retail map using `Isle_A` and `Base_X` (which has a dock), and a mod with a map of its own type.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-map-docks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(
        &assets,
        "maps/red/map.json",
        &json!({"name": "Red", "objects": [{"type": "Isle_A", "id": 1, "pos": [0.0, 0.0, 0.0]}, {"type": "Base_X", "id": 2, "pos": [9.0, 9.0, 0.0]}]}),
    );
    put(&assets, "data/docks.json", &json!({"Base_X": {"points": [{"label": "DockPoint1", "pos": [0, 0, 0], "yaw": 0}], "tender": true}}));
    put(&m, "mod.json", &json!({"id": "docks_test"}));
    put(&m, "maps/mine/map.json", &json!({"name": "Mine", "objects": [{"type": "My_Fort", "scene": "My_Fort", "id": 1, "pos": [5.0, 5.0, 0.0]}]}));
    (root, assets, m)
}

#[test]
fn retail_types_are_read_only_and_a_mods_own_type_is_not() {
    let (_root, assets, m) = setup("retail");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    docks::ensure_open(&mut p, true);
    assert!(docks::retail_reason(&p, "Isle_A").unwrap().contains("retail map"), "used by a retail map");
    assert!(docks::retail_reason(&p, "Base_X").is_some());
    assert_eq!(docks::retail_reason(&p, "My_Fort"), None);
    let err = docks::write(&mut p, "Base_X", &Action::AddDock { pos: [1.0, 2.0, 3.0] }).unwrap_err();
    assert!(err.contains("read-only"), "{err}");
    let err = docks::write(&mut p, "Isle_A", &Action::AddCrew { name: "Crew_Walk_P_0".into(), pos: [0.0; 3] }).unwrap_err();
    assert!(err.contains("read-only"), "{err}");
    assert!(p.dirty_rels().is_empty(), "nothing was written");
    assert_eq!(p.undo_label(), None);
    // without an import every type is the mod's own
    let mut q = Project::open(&m, None).unwrap();
    docks::ensure_open(&mut q, true);
    assert_eq!(docks::retail_reason(&q, "Isle_A"), None);
}

#[test]
fn each_action_is_one_undo_step_and_the_engine_loads_what_was_saved() {
    let (_root, assets, m) = setup("engine");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    let ty = "My_Fort";
    let (l1, sel) = docks::write(&mut p, ty, &Action::AddDock { pos: [1.5, 2.0, 3.0] }).unwrap();
    assert_eq!((l1.as_str(), sel), ("Add dock point DockPoint1 to My_Fort", Some((Kind::Dock, 0))));
    assert_eq!(p.undo_label(), Some("Add dock point DockPoint1 to My_Fort"));
    docks::write(&mut p, ty, &Action::AddDock { pos: [-4.0, 8.0, 3.0] }).unwrap();
    docks::write(&mut p, ty, &Action::Move { kind: Kind::Dock, index: 1, pos: [-5.0, 9.0, 3.5] }).unwrap();
    docks::write(&mut p, ty, &Action::Yaw { index: 1, yaw: 1.5 }).unwrap();
    docks::write(&mut p, ty, &Action::AddCrew { name: "Crew_Walk_P_0".into(), pos: [0.0, 1.0, 3.0] }).unwrap();
    docks::write(&mut p, ty, &Action::AddCrew { name: "Crew_Post_Helm".into(), pos: [0.0, 2.0, 3.0] }).unwrap();
    docks::write(&mut p, ty, &Action::Move { kind: Kind::Crew, index: 1, pos: [0.5, 2.5, 3.0] }).unwrap();
    // one step per action: a drag is one, and undo takes back exactly it
    assert_eq!(p.undo_label(), Some("Move crew place 1 of My_Fort"));
    p.undo();
    assert_eq!(read_edited(&p, docks::CREW)["My_Fort"][1]["pos"], json!([0.0, 2.0, 3.0]));
    p.redo();
    assert_eq!(read_edited(&p, docks::CREW)["My_Fort"][1]["pos"], json!([0.5, 2.5, 3.0]));
    // a point added and deleted again
    docks::write(&mut p, ty, &Action::AddDock { pos: [9.0, 9.0, 9.0] }).unwrap();
    docks::write(&mut p, ty, &Action::Delete { kind: Kind::Dock, index: 2 }).unwrap();
    assert!(docks::write(&mut p, ty, &Action::AddCrew { name: "Walker".into(), pos: [0.0; 3] }).is_err(), "not a crew node name");
    assert!(docks::write(&mut p, ty, &Action::Move { kind: Kind::Dock, index: 7, pos: [0.0; 3] }).is_err(), "no such point");

    let report = p.save();
    assert!(report.errors.is_empty(), "{report:?}");
    assert_eq!(report.written.len(), 2, "{report:?}");
    let d = read(&m, "data/docks.json");
    let c = read(&m, "data/crew_nodes.json");
    assert_eq!(d, json!({"My_Fort": {"points": [{"label": "DockPoint1", "pos": [1.5, 2.0, 3.0], "yaw": 0.0}, {"label": "DockPoint2", "pos": [-5.0, 9.0, 3.5], "yaw": 1.5}]}}));
    assert_eq!(c, json!({"My_Fort": [{"name": "Crew_Walk_P_0", "pos": [0.0, 1.0, 3.0]}, {"name": "Crew_Post_Helm", "pos": [0.5, 2.5, 3.0]}]}));
    assert!(!m.join("data/collision.json").exists(), "collision is never written");

    // the engine: the docks of the type, as the loader makes them (the mod's entry merged over the import's, the retail one untouched)
    let merged = {
        let mut v = read(&assets, "data/docks.json");
        v.as_object_mut().unwrap().extend(d.as_object().unwrap().clone());
        v
    };
    let dockable = openprocyon::docking::dockable_for(&merged, ty).expect("the engine reads the saved dock points");
    assert_eq!(dockable.points.len(), 2);
    assert_eq!(dockable.points[1].pos, openprocyon::authoring::game_to_bevy([-5.0, 9.0, 3.5]));
    assert!((dockable.points[1].fwd - Quat::from_rotation_y(1.5) * Vec3::NEG_Z).length() < 1e-5);
    assert_eq!(dockable.numbers, vec![1, 2], "the label numbers pick the crane clips");
    assert!(openprocyon::docking::dockable_for(&merged, "Base_X").is_some(), "the retail entry is intact");
    let crews = openprocyon::data::crew_nodes::load_scenery(&m);
    let nodes = crews.get(ty).expect("the engine reads the saved crew places");
    assert_eq!(nodes.iter().map(|n| n.0.as_str()).collect::<Vec<_>>(), ["Crew_Walk_P_0", "Crew_Post_Helm"]);
    assert_eq!(nodes[1].1.translation, openprocyon::authoring::game_to_bevy([0.5, 2.5, 3.0]));

    // and its own check finds nothing wrong with either file
    let found = openprocyon::authoring::check_mod(&m, Some(&assets));
    let bad: Vec<String> = found.iter().filter(|f| f.file.contains("docks") || f.file.contains("crew_nodes")).map(|f| format!("{}: {}: {}", f.file, f.path, f.message)).collect();
    assert!(bad.is_empty(), "{bad:?}");
}

fn read_edited(p: &Project, rel: &str) -> Value {
    p.doc(rel).unwrap().edited.clone()
}

#[test]
fn the_placement_follows_the_first_dock_height() {
    // the engine's own rule: the first dock point's height is where the object's z lands (view3d::placement calls it)
    use openprocyon_editor::map::view3d::placement;
    let object = json!({"type": "My_Fort", "pos": [5.0, 5.0, 10.0]});
    let none = placement(&object, &json!({}));
    let docked = placement(&object, &json!({"My_Fort": {"points": [{"label": "DockPoint1", "pos": [0, 0, 4], "yaw": 0}]}}));
    assert!((none.translation.y - 10.0).abs() < 1e-5 && (docked.translation.y - 6.0).abs() < 1e-5, "{none:?} {docked:?}");
}
