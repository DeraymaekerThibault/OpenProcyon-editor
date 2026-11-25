//! The skirmish section (T10) on synthetic data made here: what the lobby checklist says, the end rules, a script and media created
//! on first use and saved as the project saves them, and the engine's own answer to "does a map built only in the editor list in the
//! lobby": the engine's skirmish list (`menu::skirmish_maps`, read in a child process that has the mod's overlay as its assets) and its
//! end-rule reader (`victory::Victory::from_map`), plus `--check-mod` as a library call.

use bevy_egui::egui;
use openprocyon::authoring::{self, Level, rule_schema, schemas};
use openprocyon_editor::form::model::{Kind, Node};
use openprocyon_editor::map::doc::{self, PlayerField};
use openprocyon_editor::map::players::{add_player, add_team, set_field, set_start};
use openprocyon_editor::map::project::{merged_view, new_map, open_map};
use openprocyon_editor::map::skirmish::{self as sk, MediaTable, VICTORY_TYPES};
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

/// (root, assets, mod folder): an import with one retail-like skirmish map `red` and a ship, and an empty mod.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-skirmish-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "ships/Ship_A/ship.json", &json!({"race": "Navy", "points": 10, "skirmish": true}));
    put(
        &assets,
        "maps/red/map.json",
        &json!({"name": "Red", "skirmish": true, "assemble": true, "world_half": 400.0,
            "teams": [{"name_id": "T1"}, {"name_id": "T2"}],
            "players": [
                {"name": "A", "team": 0, "start": [0.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 0, "playable": true, "race": 0},
                {"name": "B", "team": 1, "start": [100.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 0, "playable": true, "race": 2}],
            "victory": [{"name": "end", "if": [{"Type": "Skirmish Game Complete"}]}]}),
    );
    std::fs::create_dir_all(&m).unwrap();
    (root, assets, m)
}

fn open(assets: &Path, m: &Path) -> Project {
    Project::open(m, Some(assets.to_path_buf())).unwrap()
}

fn save(p: &mut Project) {
    let r = p.save();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

fn map_of<'a>(p: &'a Project, id: &str) -> &'a Value {
    merged_view(p, id).unwrap()
}

// ---- the keys ----

#[test]
fn the_skirmish_keys_are_in_the_engines_map_schema() {
    let s = schemas().into_iter().find(|(n, _)| *n == "map.schema.json").unwrap().1;
    let node = Node::from_schema(&s);
    let at = |p: &str| node.at(p).unwrap_or_else(|| panic!("map.schema.json has no {p}"));
    assert_eq!(at(&format!("/{}", doc::KEY_SKIRMISH)).kind, Kind::Boolean);
    assert_eq!(at(&format!("/{}", doc::KEY_ASSEMBLE)).kind, Kind::Boolean);
    assert!(matches!(at(&format!("/{}", doc::KEY_VICTORY)).kind, Kind::Array { .. }));
    for p in ["/victory/0/name", "/victory/0/if", "/victory/0/winner", "/victory/0/end/winner", "/victory/0/end/loser", "/victory/0/if/0/Type"] {
        assert!(node.at(p).is_some(), "{p}");
    }
}

#[test]
fn the_victory_types_are_the_ones_the_engine_reads_and_the_rule_schema_has() {
    let schema = rule_schema();
    let types = schema["types"].as_array().unwrap();
    let map = json!({"teams": [{"name_id": "T1"}]});
    for t in VICTORY_TYPES {
        let def = types.iter().find(|d| d["name"] == t.name).unwrap_or_else(|| panic!("the rule schema has no '{}'", t.name));
        assert_eq!(def["role"], "condition", "{}", t.name);
        // the engine's end-rule reader takes the condition the form makes
        let rule = json!({"victory": [{"name": "r", "if": [sk::new_condition(t.name, &map)]}]});
        assert!(openprocyon::victory::Victory::from_map(&rule, vec![]).is_some(), "the engine does not read a fresh '{}'", t.name);
    }
    // and refuses what the form does not offer
    let other = json!({"victory": [{"name": "r", "if": [{"Type": "Timer Condition"}]}]});
    assert!(openprocyon::victory::Victory::from_map(&other, vec![]).is_none());
    // the five the schema itself marks as the victory reader's are all offered
    for d in types.iter().filter(|d| d["handler"] == "victory") {
        assert!(VICTORY_TYPES.iter().any(|t| d["name"] == t.name), "{} is a victory-handler Type the form does not offer", d["name"]);
    }
    for name in sk::STORED_ONLY {
        assert!(types.iter().any(|d| d["name"] == *name && d["role"] == "action"), "{name} is no action of the rule schema");
    }
    let campaign = sk::campaign_only_types();
    assert!(campaign.contains("Goto Next Level") || !campaign.is_empty(), "the schema marks campaign-only Types");
}

// ---- the checklist ----

#[test]
fn the_checklist_says_why_a_map_does_not_list() {
    let mut map = doc::new_map("m");
    let list = sk::checklist(&map);
    let failing = |m: &Value| sk::checklist(m).into_iter().filter(|c| !c.ok).map(|c| c.text).collect::<Vec<_>>();
    assert!(list.iter().any(|c| !c.ok && c.text.contains("skirmish")));
    assert!(!sk::lists_and_starts(&map));
    map["skirmish"] = json!(true);
    assert!(failing(&map).iter().any(|t| t.contains("playable player")), "{:?}", failing(&map));
    map["players"] = json!([doc::new_player("A", 0, [0.0, 0.0, 0.0])]);
    map["players"][0]["playable"] = json!(true);
    map["teams"] = json!([doc::new_team("T1"), doc::new_team("T2")]);
    assert!(failing(&map).iter().any(|t| t.contains("opponent")), "one playable player has no opponent");
    map["players"] = json!([map["players"][0].clone(), doc::new_player("B", 1, [50.0, 0.0, 0.0])]);
    map["players"][1]["playable"] = json!(true);
    // a historical map needs ships for its players; an open one does not
    assert!(failing(&map).iter().any(|t| t.contains("placed ships")), "{:?}", failing(&map));
    map["assemble"] = json!(true);
    assert!(sk::lists_and_starts(&map), "{:?}", failing(&map));
    // the same team: no opponent
    map["players"][1]["team"] = json!(0);
    assert!(!sk::has_opponent(&map));
    // teamless players fight each other
    map["players"][0]["team"] = json!(-1);
    map["players"][1]["team"] = json!(-1);
    assert!(sk::has_opponent(&map));
    // a player without a start
    map["players"][1].as_object_mut().unwrap().remove("start");
    assert!(failing(&map).iter().any(|t| t.contains("start")));
}

// ---- the whole map, built in the editor ----

/// Builds map `duel` the way a user does: new map, flag, teams, two players with starts, an end rule. Nothing from the import.
fn build_duel(p: &mut Project) {
    new_map(p, "duel").unwrap();
    sk::set_skirmish(p, "duel", true).unwrap();
    sk::set_assemble(p, "duel", true).unwrap();
    add_team(p, "duel", "Team A").unwrap();
    add_team(p, "duel", "Team B").unwrap();
    add_player(p, "duel", Some("Alpha"), Some([-200.0, 0.0])).unwrap();
    add_player(p, "duel", Some("Beta"), Some([200.0, 0.0])).unwrap();
    for i in 0..2 {
        set_field(p, "duel", i, PlayerField::Playable, json!(true)).unwrap();
        set_field(p, "duel", i, PlayerField::Team, json!(i)).unwrap();
    }
    set_start(p, "duel", 1, [250.0, 0.0]).unwrap();
    let rule = sk::add_victory_rule(p, "duel").unwrap();
    sk::set_victory_winner(p, "duel", rule, Some("Team A")).unwrap();
}

#[test]
fn a_map_built_only_in_the_editor_passes_the_checklist_the_mod_check_and_the_engines_lobby_list() {
    let (root, assets, m) = setup("lobby");
    let mut p = open(&assets, &m);
    build_duel(&mut p);
    let map = map_of(&p, "duel").clone();
    assert!(sk::lists_and_starts(&map), "{:?}", sk::checklist(&map));
    assert!(sk::checklist(&map).iter().all(|c| c.ok), "{:?}", sk::checklist(&map));
    // the engine reads the end rule the form wrote
    let teams = vec![("Team A".to_string(), vec![]), ("Team B".to_string(), vec![])];
    assert!(openprocyon::victory::Victory::from_map(&map, teams).is_some());
    save(&mut p);
    // `--check-mod` as a library call: no errors in this mod
    let found = authoring::check_mod(&m, Some(&assets));
    let errors: Vec<String> = found.iter().filter(|f| f.level == Level::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{errors:?}");
    // the engine's own skirmish list, from the overlay the game would build for this mod
    let rows = engine_lobby_rows(&root, &assets, &m);
    let row = rows.iter().find(|r| r[0] == "duel").unwrap_or_else(|| panic!("the lobby does not list duel: {rows:?}"));
    assert_eq!((row[1].as_str(), row[2].as_bool(), row[3].as_u64()), (Some("duel"), Some(true), Some(2)), "id, name, open, playable players: {row}");
    assert!(rows.iter().any(|r| r[0] == "red"), "the retail one is listed too");
    // a map without the flag is not
    let mut q = open(&assets, &m);
    open_map(&mut q, "duel").unwrap();
    sk::set_skirmish(&mut q, "duel", false).unwrap();
    save(&mut q);
    let rows = engine_lobby_rows(&root, &assets, &m);
    assert!(!rows.iter().any(|r| r[0] == "duel"), "{rows:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// The rows of `openprocyon::menu::skirmish_maps()` over the overlay of `assets` and the mod, as the game would list them. The game's
/// asset root is a process-wide setting read once from `PROCYON_ASSETS`, so the call is made by this test binary started again with
/// that variable, and its output read back.
fn engine_lobby_rows(root: &Path, assets: &Path, m: &Path) -> Vec<Value> {
    let out = root.join(format!("overlay-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    authoring::overlay_into(assets, &[m.to_path_buf()], &out).unwrap();
    let exe = std::env::current_exe().unwrap();
    let r = std::process::Command::new(exe)
        .args(["--exact", "child_prints_the_engines_skirmish_list", "--nocapture", "--test-threads=1", "--include-ignored"])
        .env("PROCYON_ASSETS", &out)
        .env("T10_CHILD", "1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&r.stdout);
    let line = text
        .lines()
        .find_map(|l| l.split_once("T10ROWS").map(|(_, rest)| rest))
        .unwrap_or_else(|| panic!("the child printed no list: {text} {}", String::from_utf8_lossy(&r.stderr)));
    serde_json::from_str::<Value>(line).unwrap().as_array().unwrap().clone()
}

/// Not a test of its own: the child of [`engine_lobby_rows`].
#[test]
#[ignore = "run by engine_lobby_rows with PROCYON_ASSETS set"]
fn child_prints_the_engines_skirmish_list() {
    if std::env::var_os("T10_CHILD").is_none() {
        return;
    }
    println!("T10ROWS{}", json!(openprocyon::menu::skirmish_maps()));
}

// ---- the end rules ----

#[test]
fn end_rules_are_edited_through_the_project_and_undo_follows() {
    let (root, assets, m) = setup("victory");
    let mut p = open(&assets, &m);
    open_map(&mut p, "red").unwrap();
    let first = sk::add_victory_rule(&mut p, "red").unwrap();
    assert_eq!(first, 1);
    sk::add_condition(&mut p, "red", 1, "Team has X points").unwrap();
    sk::add_condition(&mut p, "red", 1, "Group Destroyed").unwrap();
    sk::set_condition_field(&mut p, "red", 1, 2, "objects", Some(sk::parse_objects("12, 14, Convoy"))).unwrap();
    sk::set_condition_field(&mut p, "red", 1, 1, "Points", Some(json!(250))).unwrap();
    sk::set_victory_end(&mut p, "red", 1, true, "IDGS_WIN").unwrap();
    sk::set_victory_end(&mut p, "red", 1, false, "IDGS_LOSE").unwrap();
    let rule = &map_of(&p, "red")["victory"][1];
    assert_eq!(rule["if"][2]["objects"], json!([12, 14, "Convoy"]));
    assert_eq!(rule["if"][1], json!({"Type": "Team has X points", "Team Name": "T1", "Points": 250}));
    assert_eq!((doc::victory_end(rule, true), doc::victory_end(rule, false)), (Some("IDGS_WIN"), Some("IDGS_LOSE")));
    // the engine reads all of it, groups named by the script included
    let groups = json!({"Convoy": [20, 21]});
    assert!(openprocyon::victory::Victory::from_map_in(map_of(&p, "red"), vec![], &groups).is_some());
    // a condition's Type changed makes it new; an unknown Type is refused
    sk::set_condition_kind(&mut p, "red", 1, 2, "Enter Volume").unwrap();
    assert!(map_of(&p, "red")["victory"][1]["if"][2]["polygon"].as_array().unwrap().len() >= 3);
    assert!(sk::add_condition(&mut p, "red", 1, "Timer Condition").is_err());
    sk::delete_condition(&mut p, "red", 1, 2).unwrap();
    sk::delete_victory_rule(&mut p, "red", 0).unwrap();
    assert_eq!(doc::victory_rules(map_of(&p, "red")).len(), 1);
    // saved as a diff over the retail map: the mod's file extends it
    save(&mut p);
    let file = read(&m, "maps/red/map.json");
    assert_eq!(file["extends"], "red");
    assert!(file.get("players").is_none(), "{file}");
    // a fresh open reads what was saved
    let mut q = open(&assets, &m);
    open_map(&mut q, "red").unwrap();
    assert_eq!(map_of(&q, "red")["victory"], map_of(&p, "red")["victory"]);
    // undo walks back one step at a time
    sk::delete_victory_rule(&mut p, "red", 0).unwrap();
    assert!(doc::victory_rules(map_of(&p, "red")).is_empty());
    p.undo();
    assert_eq!(doc::victory_rules(map_of(&p, "red")).len(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn text_forms_of_the_list_fields_round_trip() {
    assert_eq!(sk::parse_objects("1, 2 ,Convoy,,"), json!([1, 2, "Convoy"]));
    assert_eq!(sk::format_objects(&json!([1, 2, "Convoy"])), "1, 2, Convoy");
    assert_eq!(sk::parse_polygon("0,0; 10,0; 5,8.5"), Some(json!([[0.0, 0.0], [10.0, 0.0], [5.0, 8.5]])));
    assert_eq!(sk::parse_polygon("0,0; x,1"), None);
    assert_eq!(sk::format_polygon(&sk::parse_polygon("0,0; 10,0; 5,8.5").unwrap()), "0,0; 10,0; 5,8.5");
}

// ---- script.json and media.json ----

#[test]
fn script_and_media_are_created_on_first_use_and_nothing_before() {
    let (root, assets, m) = setup("files");
    let mut p = open(&assets, &m);
    open_map(&mut p, "red").unwrap();
    // looking at the section writes nothing and makes no document
    sk::open_documents(&mut p, "red").unwrap();
    assert!(p.dirty_rels().is_empty() && sk::script_of(&p, "red").is_none() && sk::media_of(&p, "red").is_none());
    save(&mut p);
    assert!(!m.join("maps/red").exists(), "nothing was written");

    // a script rule, a condition and an action from the engine's schema forms
    let rule = sk::add_rule(&mut p, "red").unwrap();
    sk::add_item(&mut p, "red", rule, true, json!({"Type": "World Initialize"})).unwrap();
    sk::add_item(&mut p, "red", rule, false, json!({"Type": "Team X Wins", "Team Name": "T1"})).unwrap();
    sk::set_script_decides(&mut p, "red", true).unwrap();
    sk::add_media(&mut p, "red", MediaTable::Messages, "WIN_TEXT").unwrap();
    sk::set_media(&mut p, "red", MediaTable::Messages, "WIN_TEXT", None, Some(json!("Well fought."))).unwrap();
    sk::add_media(&mut p, "red", MediaTable::Speech, "HINT").unwrap();
    sk::set_media(&mut p, "red", MediaTable::Speech, "HINT", Some("text"), Some(json!("Hold the line."))).unwrap();
    assert!(sk::add_media(&mut p, "red", MediaTable::Speech, "HINT").is_err(), "a repeated key is refused");
    assert!(sk::add_media(&mut p, "red", MediaTable::Speech, "  ").is_err());
    let script = sk::script_of(&p, "red").unwrap().clone();
    assert!(doc::script_decides(&script));
    let media = sk::media_of(&p, "red").unwrap().clone();
    assert_eq!(media["messages"]["WIN_TEXT"], "Well fought.");
    assert_eq!(media_entries(&media, MediaTable::Speech), ["HINT"]);

    // the engine's skirmish check of the script finds no error
    let found = sk::check_script(&script, map_of(&p, "red"), &media);
    let errors: Vec<String> = found.iter().filter(|f| f.level == Level::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{errors:?}");
    save(&mut p);
    let saved = read(&m, "maps/red/script.json");
    assert_eq!(saved["victory"], "script");
    assert_eq!(saved["rules"][0]["actions"][0]["Type"], "Team X Wins");
    assert!(saved.get("extends").is_none(), "a new file is written whole: {saved}");
    assert_eq!(read(&m, "maps/red/media.json")["speech"]["HINT"]["text"], "Hold the line.");
    // and the mod check agrees
    let found = authoring::check_mod(&m, Some(&assets));
    let errors: Vec<String> = found.iter().filter(|f| f.level == Level::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{errors:?}");

    // handing the end back to the map's rules removes the key and keeps the rules
    sk::set_script_decides(&mut p, "red", false).unwrap();
    assert!(!doc::script_decides(sk::script_of(&p, "red").unwrap()));
    assert_eq!(doc::script_rules(sk::script_of(&p, "red").unwrap()).len(), 1);
    p.undo();
    assert!(doc::script_decides(sk::script_of(&p, "red").unwrap()));
    // a fresh open finds both files
    let mut q = open(&assets, &m);
    sk::open_documents(&mut q, "red").unwrap();
    assert_eq!(sk::script_of(&q, "red").unwrap()["rules"].as_array().unwrap().len(), 1);
    assert_eq!(sk::media_of(&q, "red").unwrap()["messages"]["WIN_TEXT"], "Well fought.");
    let _ = std::fs::remove_dir_all(&root);
}

fn media_entries(media: &Value, t: MediaTable) -> Vec<String> {
    sk::media_entries(media, t).into_iter().map(|(k, _)| k.to_string()).collect()
}

#[test]
fn a_campaign_only_type_is_an_error_of_the_engines_skirmish_check() {
    let (root, assets, m) = setup("campaign");
    let mut p = open(&assets, &m);
    open_map(&mut p, "red").unwrap();
    let campaign = sk::campaign_only_types();
    let name = campaign.iter().next().expect("the engine marks some Types campaign-only").clone();
    let rule = sk::add_rule(&mut p, "red").unwrap();
    let is_cond = rule_schema()["types"].as_array().unwrap().iter().find(|t| t["name"] == name.as_str()).unwrap()["role"] == "condition";
    sk::add_item(&mut p, "red", rule, is_cond, json!({"Type": name})).unwrap();
    let script = sk::script_of(&p, "red").unwrap();
    let found = sk::check_script(script, map_of(&p, "red"), &Value::Null);
    assert!(found.iter().any(|f| f.level == Level::Error && f.message.contains("campaign-only")), "{found:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rules_are_added_renamed_flagged_and_deleted_as_steps() {
    let (root, assets, m) = setup("rules");
    let mut p = open(&assets, &m);
    open_map(&mut p, "red").unwrap();
    let (a, b) = (sk::add_rule(&mut p, "red").unwrap(), sk::add_rule(&mut p, "red").unwrap());
    assert_eq!((a, b), (0, 1));
    let names: Vec<String> = doc::script_rules(sk::script_of(&p, "red").unwrap()).iter().map(|r| r["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["Rule 1", "Rule 2"], "names are unique");
    sk::rename_rule(&mut p, "red", 1, "Second").unwrap();
    sk::set_rule_flag(&mut p, "red", 1, "Run Once", false).unwrap();
    assert_eq!(sk::script_of(&p, "red").unwrap()["rules"][1]["Run Once"], json!(false), "a JSON boolean, not a string");
    sk::delete_rule(&mut p, "red", 0).unwrap();
    assert_eq!(sk::script_of(&p, "red").unwrap()["rules"][0]["name"], "Second");
    assert!(sk::delete_rule(&mut p, "red", 5).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

// ---- the drawing ----

#[test]
fn the_section_draws_for_a_map_with_and_without_script_and_changes_nothing() {
    let (root, assets, m) = setup("draw");
    let mut p = open(&assets, &m);
    open_map(&mut p, "red").unwrap();
    let ctx = egui::Context::default();
    let mut st = sk::Panel::default();
    let frame = |p: &mut Project, st: &mut sk::Panel| {
        let before = (p.undo_label().map(str::to_string), p.dirty_rels());
        ctx.run_ui(Default::default(), |ui| sk::show(ui, st, p, "red")).textures_delta.clear();
        assert_eq!((p.undo_label().map(str::to_string), p.dirty_rels()), before, "drawing is not an edit");
    };
    frame(&mut p, &mut st);
    let rule = sk::add_rule(&mut p, "red").unwrap();
    sk::add_item(&mut p, "red", rule, false, json!({"Type": "Team X Wins", "Team Name": "T1"})).unwrap();
    sk::add_media(&mut p, "red", MediaTable::Speech, "HINT").unwrap();
    sk::set_script_decides(&mut p, "red", true).unwrap();
    for _ in 0..3 {
        frame(&mut p, &mut st);
    }
    assert!(st.error().is_none(), "{:?}", st.error());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn play_in_lobby_uses_the_lobby_launch_of_the_play_test() {
    let l = sk::lobby_launch(Path::new("/abs/mod"), "duel", &openprocyon_editor::launch::Lobby { points: 1200, limit: 6, computers: 3, seed: None });
    let args: Vec<String> = l.args().iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let at = args.iter().position(|a| a == "--lobby").expect("--lobby is passed");
    assert_eq!(args[at + 1], "map=duel;points=1200;limit=6;slots=human,3*computer");
    assert!(!args.iter().any(|a| a == "--map" || a == "--ship"), "{args:?}");
    assert_eq!(args[..2], ["--mod", "/abs/mod"]);
}
