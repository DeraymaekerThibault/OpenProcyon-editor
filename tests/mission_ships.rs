//! Players, alliances, the human's start, ships and groups (docs/phase5.md T6) on synthetic data made here (no game content): every
//! edit is one undo step, renames follow the rule references in the same step, the refusals, and a retail mission saved as a diff.

use openprocyon::authoring::read_merged;
use openprocyon_editor::mission::doc;
use openprocyon_editor::mission::groups::{self, MemberKind};
use openprocyon_editor::mission::project::{merged_mission, open_mission};
use openprocyon_editor::mission::ships::{self, HumanShip};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const M1: &str = "maps/mission01/mission.json";
/// The mission the tests edit: the mod's own, whole in its file (a retail one is saved as a diff and refuses more).
const ID: &str = "gates";

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
        "players": ["Hero", "Raiders", "Allies"],
        "alliances": [["Hero", "Allies"]],
        "player_start": {"name": "Hero", "start": [0.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0]},
        "ships": [
            {"id": 1, "type": "Ship_A", "group": "Hero", "name": "Dauntless", "primary": true, "pos": [0.0, 0.0, 0.0]},
            {"id": 2, "type": "Ship_B", "group": "Hero", "name": "Escort", "pos": [5.0, 5.0, 0.0]},
            {"id": 3, "type": "Ship_B", "group": "Raiders", "name": "Pirate", "pos": [9.0, 9.0, 0.0]}
        ],
        "groups": {"Fleet": [1, 2], "Pirates": [3, 10]},
        "environment": {"animals": [{"group": "Whales", "id": 20}]},
        "rules": [
            {"name": "Go", "Type": "On Start", "conditions": [], "actions": [
                {"Type": "Set Group/Unit Visibility", "Group/Unit Name": "Fleet", "Boolean": true},
                {"Type": "Set Group/Unit Visibility", "Group/Unit Name": "Hero,Escort", "Boolean": true},
                {"Type": "Set Group/Unit Visibility", "Group/Unit Name": "Raiders", "Boolean": true}
            ]}
        ]
    })
}

fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-mission-ships-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First", "objects": [{"type": "Island04", "id": 10, "pos": [1, 2, 3]}, {"type": "Rock"}], "mines": [{"id": 11}]}));
    put(&assets, "maps/mission01/mission.json", &retail());
    put(&assets, "maps/mission01/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    put(&assets, "ships/Ship_A/ship.json", &json!({"points": 1}));
    put(&assets, "ships/Ship_B/ship.json", &json!({"points": 1}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    put(&m, "maps/gates/map.json", &json!({"name": "Gates", "objects": [{"type": "Island04", "id": 10, "pos": [1, 2, 3]}, {"type": "Rock"}], "mines": [{"id": 11}]}));
    put(&m, "maps/gates/mission.json", &retail());
    put(&m, "maps/gates/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    put(&m, "ships/Mine_Own/ship.json", &json!({"points": 1}));
    (root, assets, m)
}

fn opened(name: &str) -> (Project, PathBuf) {
    let (_root, assets, m) = setup(name);
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_mission(&mut p, ID).unwrap();
    (p, m)
}

fn opened_retail(name: &str) -> (Project, PathBuf) {
    let (_root, assets, m) = setup(name);
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_mission(&mut p, "mission01").unwrap();
    (p, m)
}

fn view(p: &Project) -> Value {
    merged_mission(p, ID).unwrap().clone()
}

/// The step count of the undo stack grew by exactly one and one undo restores the document.
fn one_step(p: &mut Project, f: impl FnOnce(&mut Project)) {
    let before = view(p);
    f(p);
    let after = view(p);
    assert_ne!(before, after);
    p.undo().expect("one undo step");
    assert_eq!(view(p), before, "one undo takes the whole edit back");
    p.redo().expect("redo");
    assert_eq!(view(p), after);
}

#[test]
fn ship_types_come_from_the_overlay_and_the_mod() {
    let (p, _) = opened("types");
    let t = ships::ship_types(&p);
    assert_eq!(t, ["Mine_Own", "Ship_A", "Ship_B"]);
    assert!(ships::unknown_type(&t, "Ship_Z") && !ships::unknown_type(&t, "Ship_A"));
    assert!(!ships::unknown_type(&[], "anything"), "no import found: no warning");
}

#[test]
fn human_ship_rule_is_shown_from_the_documents() {
    let m = retail();
    assert_eq!(ships::human_ship(&m), HumanShip::Primary(0));
    let mut m2 = m.clone();
    m2["ships"][0]["primary"] = json!(false);
    assert_eq!(ships::human_ship(&m2), HumanShip::First(0));
    m2["player_start"]["name"] = json!("Allies");
    assert_eq!(ships::human_ship(&m2), HumanShip::StandIn);
    m2["player_start"] = json!({});
    assert_eq!(ships::human_ship(&m2), HumanShip::NoStart);
    assert!(ships::human_ship_line(&m).contains("Dauntless"));
}

#[test]
fn players_add_rename_delete_with_followers() {
    let (mut p, _) = opened("players");
    assert_eq!(ships::unique_player_name(&view(&p)), "Player 1");
    for bad in ["", "A,B", "Hero"] {
        assert!(ships::add_player(&mut p, ID, bad).is_err(), "{bad}");
    }
    one_step(&mut p, |p| assert_eq!(ships::add_player(p, ID, " Neutral ").unwrap(), 3));
    assert_eq!(doc::players(&view(&p)), ["Hero", "Raiders", "Allies", "Neutral"]);

    // Rename: ships, alliances, the start and rule fields (Group fields name players too) follow in the same step.
    let before = view(&p);
    let n = ships::rename_player(&mut p, ID, 0, "Captain").unwrap();
    let after = view(&p);
    assert_eq!(n, 1, "the Hero,Escort field");
    assert_eq!(doc::players(&after)[0], "Captain");
    assert_eq!(doc::ship_group(&doc::ships(&after)[0]), Some("Captain"));
    assert_eq!(doc::alliances(&after), vec![["Captain", "Allies"]]);
    assert_eq!(doc::player_start_name(&after), Some("Captain"));
    // the "Hero,Escort" rule field names the group part Hero: it follows.
    assert_eq!(after["rules"][0]["actions"][1]["Group/Unit Name"], json!("Captain,Escort"));
    p.undo().unwrap();
    assert_eq!(view(&p), before);
    assert!(ships::rename_player(&mut p, ID, 0, "Raiders").is_err());
    assert!(ships::rename_player(&mut p, ID, 0, "A,B").is_err());

    // Delete refusals: owns ships, is the human's, is named by a rule; a free one goes with its alliances.
    assert!(ships::delete_player(&mut p, ID, 0).unwrap_err().contains("owns 2 ship"));
    assert!(ships::delete_player(&mut p, ID, 1).unwrap_err().contains("owns 1 ship"));
    one_step(&mut p, |p| ships::delete_player(p, ID, 3).unwrap());
    let before = view(&p);
    ships::delete_ship(&mut p, ID, 2).unwrap();
    assert!(ships::delete_player(&mut p, ID, 1).unwrap_err().contains("rule field"), "a rule still names Raiders");
    p.undo().unwrap();
    assert_eq!(view(&p), before);
    one_step(&mut p, |p| ships::set_allied(p, ID, "Hero", "Allies", false).unwrap());
    ships::set_allied(&mut p, ID, "Hero", "Allies", true).unwrap();
    // Allies has an alliance and no ship: deleting it removes the pair in the same step.
    ships::set_allied(&mut p, ID, "Allies", "Raiders", true).unwrap();
    let before = view(&p);
    ships::delete_player(&mut p, ID, 2).unwrap();
    assert_eq!(doc::players(&view(&p)), ["Hero", "Raiders"]);
    assert!(doc::alliances(&view(&p)).is_empty());
    p.undo().unwrap();
    assert_eq!(view(&p), before);
}

#[test]
fn alliances_toggle_in_either_order() {
    let (mut p, _) = opened("alliances");
    assert!(ships::allied(&view(&p), "Allies", "Hero"));
    assert!(!ships::allied(&view(&p), "Hero", "Raiders"));
    one_step(&mut p, |p| ships::set_allied(p, ID, "Raiders", "Hero", true).unwrap());
    assert!(ships::allied(&view(&p), "Hero", "Raiders"));
    assert!(ships::set_allied(&mut p, ID, "Hero", "Hero", true).is_err());
    assert!(ships::set_allied(&mut p, ID, "Hero", "Nobody", true).is_err());
}

#[test]
fn player_start_is_set_in_steps() {
    let (mut p, _) = opened("start");
    one_step(&mut p, |p| ships::set_start_player(p, ID, "Raiders").unwrap());
    assert!(ships::set_start_player(&mut p, ID, "Nobody").is_err());
    one_step(&mut p, |p| ships::set_start_vec(p, ID, false, [10.0, 20.0, 0.0], false).unwrap());
    assert_eq!(doc::player_start_pos(&view(&p)), Some([10.0, 20.0, 0.0]));
    one_step(&mut p, |p| ships::set_start_vec(p, ID, true, [1.0, 0.0, 0.0], false).unwrap());
    assert!(ships::set_start_vec(&mut p, ID, true, [0.0; 3], false).is_err());
}

#[test]
fn ships_add_with_shared_ids_rename_with_references_and_delete_out_of_groups() {
    let (mut p, _) = opened("ships");
    // ids 1,2,3 ships, 10 object, 11 mine, 20 animal: the first free one is 4.
    assert_eq!(groups::next_id(&view(&p), p.doc("maps/gates/map.json").map(|d| &d.edited)), 4);
    assert!(ships::add_ship(&mut p, ID, "Ship_A", "Nobody").is_err());
    assert!(ships::add_ship(&mut p, ID, " ", "Hero").is_err());
    one_step(&mut p, |p| assert_eq!(ships::add_ship(p, ID, "Ship_A", "Raiders").unwrap(), 3));
    let s = &doc::ships(&view(&p))[3].clone();
    assert_eq!((doc::ship_id(s), doc::ship_name(s), doc::ship_group(s)), (Some(4), Some("Ship 1"), Some("Raiders")));

    // type and owner
    one_step(&mut p, |p| ships::set_ship_type(p, ID, 1, "Ship_A").unwrap());
    assert!(ships::set_ship_type(&mut p, ID, 1, "").is_err());
    one_step(&mut p, |p| ships::set_ship_group(p, ID, 1, "Allies").unwrap());
    assert!(ships::set_ship_group(&mut p, ID, 1, "Nobody").is_err());
    assert!(ships::set_ship_type(&mut p, ID, 9, "Ship_A").is_err());
    one_step(&mut p, |p| ships::set_ship_pos(p, ID, 0, [3.0, 4.0, 5.0], false).unwrap());

    // names are unique across the mission and a rename follows `Group,Unit` references
    assert!(ships::rename_ship(&mut p, ID, 0, "Pirate").unwrap_err().contains("already"));
    assert!(ships::rename_ship(&mut p, ID, 0, "A,B").is_err());
    assert!(ships::rename_ship(&mut p, ID, 0, " ").is_err());
    let before = view(&p);
    assert_eq!(ships::rename_ship(&mut p, ID, 2, "Cutlass").unwrap(), 0);
    p.undo().unwrap();
    assert_eq!(view(&p), before);
    // Escort (ships[1], now Allies') is named in "Hero,Escort"
    assert_eq!(ships::rename_ship(&mut p, ID, 1, "Frigate").unwrap(), 1);
    assert_eq!(view(&p)["rules"][0]["actions"][1]["Group/Unit Name"], json!("Hero,Frigate"));
    p.undo().unwrap();
    assert_eq!(view(&p)["rules"][0]["actions"][1]["Group/Unit Name"], json!("Hero,Escort"));

    // a ship a rule names as a unit is not deleted; a free one leaves its groups in the same step
    assert!(ships::delete_ship(&mut p, ID, 1).unwrap_err().contains("rule field"));
    one_step(&mut p, |p| ships::delete_ship(p, ID, 2).unwrap());
    assert_eq!(doc::group_members(&view(&p), "Pirates"), [10], "id 3 left the group with the ship");
}

#[test]
fn a_ship_id_can_be_chosen_unless_something_has_it_and_the_groups_follow() {
    let (mut p, _) = opened("ship-id");
    // ids 1,2,3 ships, 10 object, 11 mine, 20 animal; Fleet [1, 2], Pirates [3, 10]
    for taken in [1, 2, 10, 11, 20] {
        assert!(ships::set_ship_id(&mut p, ID, 2, taken).unwrap_err().contains("used already"), "{taken}");
    }
    assert!(ships::set_ship_id(&mut p, ID, 9, 99).is_err());
    ships::set_ship_id(&mut p, ID, 2, 3).unwrap();
    one_step(&mut p, |p| ships::set_ship_id(p, ID, 2, 30).unwrap());
    let v = view(&p);
    assert_eq!(doc::ship_id(&doc::ships(&v)[2]), Some(30));
    assert_eq!(doc::group_members(&v, "Pirates"), [30, 10], "the group lists the new id, in place");
    assert_eq!(doc::group_members(&v, "Fleet"), [1, 2]);
    assert_eq!(groups::next_id(&v, p.doc("maps/gates/map.json").map(|d| &d.edited)), 3, "the freed id is free");
}

#[test]
fn primary_is_exclusive_per_owner_in_one_step() {
    let (mut p, _) = opened("primary");
    // Escort becomes primary: Dauntless (same owner) is turned off in the same step; Pirate (another owner) is untouched.
    one_step(&mut p, |p| ships::set_primary(p, ID, 1, true).unwrap());
    let v = view(&p);
    assert_eq!(doc::ships(&v).iter().map(doc::ship_primary).collect::<Vec<_>>(), [false, true, false]);
    assert_eq!(ships::human_ship(&v), HumanShip::Primary(1));
    one_step(&mut p, |p| ships::set_primary(p, ID, 1, false).unwrap());
    // Pirate primary, then moved to Hero: it keeps the flag and Dauntless gives it up.
    ships::set_primary(&mut p, ID, 2, true).unwrap();
    one_step(&mut p, |p| ships::set_ship_group(p, ID, 2, "Hero").unwrap());
    let v = view(&p);
    assert_eq!(doc::ships(&v).iter().map(doc::ship_primary).collect::<Vec<_>>(), [false, false, true]);
}

#[test]
fn groups_members_span_ships_objects_animals() {
    let (mut p, _) = opened("groups");
    let map = p.doc("maps/gates/map.json").map(|d| d.edited.clone());
    let c = groups::candidates(&view(&p), map.as_ref());
    let kinds = |k| c.iter().filter(|c| c.kind == k).map(|c| c.id).collect::<Vec<_>>();
    assert_eq!((kinds(MemberKind::Ship), kinds(MemberKind::Object), kinds(MemberKind::Animal)), (vec![1, 2, 3], vec![10], vec![20]));
    assert!(groups::duplicate_ids(&view(&p), map.as_ref()).is_empty());

    one_step(&mut p, |p| assert_eq!(groups::add_group(p, ID, " Scouts ").unwrap(), "Scouts"));
    for bad in ["", "Fleet", "A,B"] {
        assert!(groups::add_group(&mut p, ID, bad).is_err(), "{bad}");
    }
    one_step(&mut p, |p| groups::add_member(p, ID, "Scouts", 20).unwrap());
    assert!(groups::add_member(&mut p, ID, "Scouts", 20).is_err(), "twice");
    assert!(groups::add_member(&mut p, ID, "Scouts", 11).is_err(), "a mine is not offered");
    assert!(groups::add_member(&mut p, ID, "Nope", 1).is_err());
    groups::add_member(&mut p, ID, "Scouts", 10).unwrap();
    one_step(&mut p, |p| groups::remove_member(p, ID, "Scouts", 20).unwrap());
    assert_eq!(doc::group_members(&view(&p), "Scouts"), [10]);
    assert!(groups::remove_member(&mut p, ID, "Scouts", 20).is_err());
}

#[test]
fn group_rename_renames_every_rule_reference_in_one_step_and_delete_is_refused_while_named() {
    let (mut p, _) = opened("grename");
    let before = view(&p);
    let n = groups::rename_group(&mut p, ID, "Fleet", "Armada").unwrap();
    assert_eq!(n, 1);
    let v = view(&p);
    assert_eq!(groups::names(&v), ["Armada", "Pirates"], "the key keeps its place");
    assert_eq!(doc::group_members(&v, "Armada"), [1, 2]);
    assert_eq!(v["rules"][0]["actions"][0]["Group/Unit Name"], json!("Armada"));
    assert_eq!(v["rules"][0]["actions"][2]["Group/Unit Name"], json!("Raiders"));
    p.undo().unwrap();
    assert_eq!(view(&p), before, "one undo step");
    assert!(groups::rename_group(&mut p, ID, "Fleet", "Pirates").is_err());
    assert!(groups::rename_group(&mut p, ID, "Fleet", "A,B").is_err());
    assert!(groups::delete_group(&mut p, ID, "Fleet").unwrap_err().contains("rule field"));
    one_step(&mut p, |p| groups::delete_group(p, ID, "Pirates").unwrap());
}

#[test]
fn duplicate_ids_across_the_three_are_reported() {
    let mut m = retail();
    m["environment"]["animals"] = json!([{"group": "W", "id": 2}]);
    let map = json!({"objects": [{"id": 1}], "mines": [{"id": 5}, {"id": 5}]});
    assert_eq!(groups::duplicate_ids(&m, Some(&map)), [1, 2, 5]);
    assert_eq!(groups::next_id(&m, Some(&map)), 4);
}

#[test]
fn a_retail_mission_is_saved_as_a_diff_and_unkeyed_lists_refuse() {
    let (mut p, m) = opened_retail("diff");
    // players and alliances are not keyed by the engine: add and delete are refused with the project's reason; renames are not
    assert!(ships::add_player(&mut p, "mission01", "Neutral").is_err());
    assert!(ships::set_allied(&mut p, "mission01", "Raiders", "Hero", true).is_err());
    assert!(ships::delete_player(&mut p, "mission01", 2).is_err());
    // group deletes and renames remove a key: refused in a file that extends another; adding a group and a member is fine
    assert!(groups::delete_group(&mut p, "mission01", "Pirates").is_err());
    assert!(groups::rename_group(&mut p, "mission01", "Fleet", "Armada").is_err());
    ships::add_ship(&mut p, "mission01", "Ship_A", "Raiders").unwrap();
    ships::set_primary(&mut p, "mission01", 1, true).unwrap();
    groups::add_group(&mut p, "mission01", "Scouts").unwrap();
    groups::add_member(&mut p, "mission01", "Scouts", 4).unwrap();
    ships::set_start_vec(&mut p, "mission01", false, [7.0, 7.0, 0.0], false).unwrap();
    let want = merged_mission(&p, "mission01").unwrap().clone();
    let r = p.save();
    assert!(r.errors.is_empty(), "{r:?}");
    let file = read(&m, M1);
    assert_eq!(file["extends"], json!("mission01"));
    assert!(file.get("rules").is_none() && file.get("players").is_none(), "nothing of the retail mission is restated: {file}");
    assert_eq!(read_merged(&p.root, M1).unwrap(), want, "the engine's overlay agrees with the document");
}

#[test]
fn the_picker_lists_ships_first_and_a_new_ship_starts_as_a_ship_of_the_humans_side() {
    let types: Vec<String> = ["Asteroid_Large_Armed", "Base_Navy", "Island_A", "Ship_A", "Ship_B"].map(String::from).to_vec();
    assert_eq!(ships::picker_order(&types), ["Ship_A", "Ship_B", "Base_Navy", "Asteroid_Large_Armed", "Island_A"]);
    // the human is Hero; no ships yet: the mod's own ship, else the first Ship_ of the import
    let m = json!({"players": ["Raiders", "Hero"], "player_start": {"name": "Hero"}, "ships": []});
    assert_eq!(ships::new_ship_defaults(&m, &types, &[]), Some(("Ship_A".to_string(), "Hero".to_string())));
    assert_eq!(ships::new_ship_defaults(&m, &types, &["Ship_B".to_string(), "Island_A".to_string()]), Some(("Ship_B".to_string(), "Hero".to_string())));
    // Hero already has a ship: the next one is of the same class; another player's ship does not count
    let m = json!({"players": ["Raiders", "Hero"], "player_start": {"name": "Hero"}, "ships": [
        {"id": 1, "type": "Ship_B", "group": "Hero"}, {"id": 2, "type": "Ship_A", "group": "Raiders"}]});
    assert_eq!(ships::new_ship_defaults(&m, &types, &[]), Some(("Ship_B".to_string(), "Hero".to_string())));
    // no start player: the first player
    let m = json!({"players": ["Raiders"], "ships": []});
    assert_eq!(ships::new_ship_defaults(&m, &types, &[]).map(|d| d.1), Some("Raiders".to_string()));
    assert_eq!(ships::new_ship_defaults(&json!({"players": []}), &types, &[]), None);
    assert_eq!(ships::new_ship_defaults(&m, &[], &[]), None);
}

#[test]
fn renaming_a_group_leaves_its_section_open() {
    use bevy_egui::egui::{self, collapsing_header::CollapsingState};
    let (mut p, _) = opened("grouprename");
    let ctx = egui::Context::default();
    let mut frame = |p: &mut Project| {
        let mut open = false;
        let mut err = None;
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 800.0))), ..Default::default() };
        ctx.run_ui(raw, |ui| {
            groups::show(ui, p, ID, &mut err);
            open = CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(groups::header_salt(0)), false).is_open();
        })
        .textures_delta
        .clear();
        open
    };
    assert!(!frame(&mut p), "closed to begin with");
    let mut set_open = |on: bool| {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 800.0))), ..Default::default() };
        ctx.run_ui(raw, |ui| {
            let mut s = CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id(groups::header_salt(0)), false);
            s.set_open(on);
            s.store(ui.ctx());
        })
        .textures_delta
        .clear();
    };
    set_open(true);
    assert!(frame(&mut p), "opened");
    groups::rename_group(&mut p, ID, "Fleet", "Navy").unwrap();
    assert_eq!(groups::names(&view(&p)), ["Navy", "Pirates"], "a rename keeps the order");
    assert!(frame(&mut p), "still open after the rename");
}
