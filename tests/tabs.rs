//! Round-4 steps 4, 5, 7 and 15 as tests (editor issue #5): the Maps tab, the Missions tab and the problems panel drawn whole in the
//! shared rig, over synthetic mods in a temp folder (no game content). Each test names the step of `docs/round4.md` it stands for.
mod common;

use bevy_egui::egui;
use common::Rig;
use openprocyon::authoring::{self, Level};
use openprocyon_editor::map::skirmish as sk;
use openprocyon_editor::mission::rules_view as rv;
use openprocyon_editor::project_panel::{self, Focus};
use openprocyon_editor::{maps_panel, missions_panel};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

/// (temp root, import, mod): the import has retail maps `red` and `blue`; the mod folder is empty but for its `mod.json`.
fn world(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-tabs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    for id in ["red", "blue"] {
        put(&assets, &format!("maps/{id}/map.json"), &json!({"name": id, "world_half": 1500.0, "skirmish": true, "objects": [{"type": "Isle_A", "id": 1, "pos": [0, 0, 0]}]}));
    }
    put(&assets, "ships/Ship_Retail/ship.json", &json!({"name": "Retail", "race": "Navy", "points": 10}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    (root, assets, m)
}

/// What a tab works on: the open mod and the Maps tab's own state.
struct Tab {
    ps: project_panel::State,
    maps: maps_panel::State,
}

fn open(m: &Path, assets: &Path) -> project_panel::State {
    let mut ps = project_panel::State::default();
    ps.open(m, Some(assets.to_path_buf()), false);
    assert!(ps.project.is_some(), "the mod did not open");
    ps
}

fn maps_tab(name: &str, height: f32) -> (Rig<'static, Tab>, PathBuf) {
    let (root, assets, m) = world(name);
    let rig = Rig::new((900.0, height), Tab { ps: open(&m, &assets), maps: maps_panel::State::default() }, |ui, t| maps_panel::tab(ui, &mut t.maps, &mut t.ps));
    (rig, root)
}

fn show_map(rig: &mut Rig<Tab>, id: &str) {
    let t = rig.state_mut();
    t.maps.show_map(t.ps.project.as_mut().unwrap(), id);
    rig.settle();
}

fn y_of(rig: &Rig<Tab>, text: &str) -> f32 {
    rig.texts().iter().find(|d| d.text == text).unwrap_or_else(|| panic!("no text {text:?}")).rect.min.y
}

// ---- step 4: the Skirmish section scrolls inside its own area ----

#[test]
fn the_skirmish_section_scrolls_inside_its_own_area_and_pushes_nothing_below_it() {
    const H: f32 = 800.0;
    let (mut rig, root) = maps_tab("scroll", H);
    show_map(&mut rig, "red");
    let closed = y_of(&rig, "Duplicate");
    rig.click_text("Skirmish"); // opens the section
    // a script with many rules: the section would be thousands of points tall
    let add = |rig: &mut Rig<Tab>, n: usize| {
        let p = rig.state_mut().ps.project.as_mut().unwrap();
        for _ in 0..n {
            sk::add_rule(p, "red").unwrap();
        }
        rig.settle();
    };
    sk::set_script_decides(rig.state_mut().ps.project.as_mut().unwrap(), "red", true).unwrap();
    add(&mut rig, 30);
    let thirty = y_of(&rig, "Duplicate");
    add(&mut rig, 90);
    let more = y_of(&rig, "Duplicate");
    assert_eq!(thirty, more, "the section stopped growing: it scrolls inside its own area");
    assert!(more - closed <= H * 0.5 + 100.0, "the section takes at most half the window: the plan toolbar went from {closed} to {more}");
    assert!(more < H, "the plan toolbar is still in the window");
    let _ = std::fs::remove_dir_all(&root);
}

// ---- step 5: the backdrop state belongs to its map ----

#[test]
fn the_backdrop_path_and_message_of_one_map_are_not_shown_on_another() {
    let (mut rig, root) = maps_tab("backdrop", 1000.0);
    let pic = root.join("pic.png");
    std::fs::write(&pic, [&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A][..], b"synthetic, not a real picture"].concat()).unwrap();
    let has = |rig: &Rig<Tab>, needle: &str| rig.texts().iter().any(|d| d.text.contains(needle));
    show_map(&mut rig, "red");
    rig.click_text("Map settings");
    assert!(has(&rig, "Backdrop picture (star map): off"));
    rig.click_text("path of a .png");
    rig.type_text(&pic.display().to_string());
    rig.click_text("Copy into the mod");
    assert!(has(&rig, "Copied to maps/red/backdrop.png."), "the copy is announced");
    assert!(has(&rig, "Backdrop picture (star map): on"));
    // another map: no path, no message, backdrop off
    show_map(&mut rig, "blue");
    rig.click_text("Map settings");
    assert!(has(&rig, "Backdrop picture (star map): off"));
    assert!(!has(&rig, "Copied to") && !has(&rig, "pic.png"), "the path and the message were typed for the other map");
    // back: the file's own state
    show_map(&mut rig, "red"); // its settings are still open: the header remembers that per map
    assert!(has(&rig, "Backdrop picture (star map): on") && !has(&rig, "Copied to") && !has(&rig, "pic.png"));
    let _ = std::fs::remove_dir_all(&root);
}

// ---- step 7: the Missions tab at narrow and wide sizes ----

/// The Missions tab of a small mission (`m`: two players, two ships, a volume, a point, two rules) in a window `w` wide.
fn missions_tab(name: &str, w: f32) -> (Rig<'static, project_panel::State>, PathBuf) {
    let (root, assets, m) = world(name);
    put(
        &m,
        "maps/m/mission.json",
        &json!({
            "players": ["Hero", "Raiders"],
            "ships": [
                {"id": 1, "type": "Ship_Navy_Cutter", "group": "Hero", "name": "Dauntless", "primary": true, "pos": [0, 0, 0]},
                {"id": 2, "type": "Ship_Pirate_Sloop", "group": "Raiders", "name": "Raider One", "pos": [400, 600, 0]}
            ],
            "groups": {"RaiderShips": [2]},
            "volumes": {"Gate Area": [[300, 500], [500, 500], [500, 700], [300, 700]]},
            "points": [{"Name": "The Gate", "Position": [400, 600, 0]}],
            "tasks": [],
            "rules": [{"name": "Start", "conditions": [], "actions": []}, {"name": "Arrive at the gate", "conditions": [], "actions": []}]
        }),
    );
    put(&m, "maps/m/map.json", &json!({"name": "M", "world_half": 500.0}));
    put(&m, "maps/m/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    let rig = Rig::new((w, 3000.0), open(&m, &assets), missions_panel::tab);
    rv::state(rig.ctx()).lock().unwrap().mission = Some("m".into());
    let mut rig = rig;
    rig.settle();
    (rig, root)
}

#[test]
fn the_missions_tab_fits_a_narrow_and_a_wide_window_and_puts_the_plan_under_or_beside_the_list() {
    for (w, narrow) in [(500.0, true), (1200.0, false)] {
        let (rig, root) = missions_tab(if narrow { "narrow" } else { "wide" }, w);
        let texts = rig.texts();
        let at = |t: &str| texts.iter().find(|d| d.text == t).unwrap_or_else(|| panic!("no text {t:?} at width {w}")).rect;
        let cut: Vec<_> = texts.iter().filter(|d| d.rect.min.x < -1.0 || d.rect.max.x > w + 1.0).map(|d| (d.text.clone(), d.rect)).collect();
        assert!(cut.is_empty(), "text outside the {w}-wide window: {cut:?}");
        let (list, plan) = (at("Rules (2)"), at("Add volume"));
        if narrow {
            assert!(texts.iter().any(|d| d.text == "Plan"), "the plan gets a heading of its own under the list");
            assert!(plan.min.y > at("Rule 1").min.y, "narrow: the plan is under the selected rule's details: {plan:?}");
        } else {
            assert!(!texts.iter().any(|d| d.text == "Plan"), "wide: the plan is in a column, no heading");
            assert!(plan.min.x > w / 2.0 && (plan.min.y - list.min.y).abs() < 100.0, "wide: the plan is the right column, level with the list: {plan:?} and {list:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}

// ---- step 15: the problems panel shows what `--check-mod --json` says ----

#[test]
fn the_problems_panel_shows_the_findings_check_mod_json_prints() {
    let (root, assets, m) = world("problems");
    // a mod with something wrong in it: a ship that extends nothing, and a ship with a misspelt key
    put(&m, "ships/Ship_Bad/ship.json", &json!({"extends": "Ship_Nowhere"}));
    put(&m, "ships/Ship_Typo/ship.json", &json!({"race": "Navy", "points": 10, "sigth": 5}));
    // what the game's `--check-mod <dir> --json` prints: `check_mod(dir, base_assets())`, its counts and the findings (game/src/cli.rs)
    let findings = authoring::check_mod(&m, Some(&assets));
    let errors = findings.iter().filter(|f| f.level == Level::Error).count();
    let inherited = findings.iter().filter(|f| f.inherited).count();
    let warnings = findings.len() - errors - inherited;
    assert!(errors > 0 && warnings > 0, "the mod must have both to make this a test: {findings:?}");
    let mut rig = Rig::new((900.0, 600.0), (open(&m, &assets), Focus::default()), |ui, (ps, focus)| {
        ps.tick(0.0); // takes in the finished check
        project_panel::problems_panel(ui, ps, focus);
    });
    // the check runs on its own thread: frames until its answer is in
    for _ in 0..400 {
        if !rig.state().0.findings.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        rig.frame();
    }
    rig.settle();
    let mut shown: Vec<Value> = rig.state().0.findings.iter().map(|f| serde_json::to_value(f).unwrap()).collect();
    let mut printed: Vec<Value> = findings.iter().map(|f| serde_json::to_value(f).unwrap()).collect();
    let key = |v: &Value| v.to_string();
    shown.sort_by_key(key);
    printed.sort_by_key(key);
    assert_eq!(shown, printed, "the panel holds the findings the command prints");
    // the list scrolls in a 140-point area: every row it shows while the wheel takes it from the top to the bottom
    let mut texts: Vec<String> = vec![];
    let over = egui::pos2(100.0, 60.0);
    for _ in 0..12 {
        texts.extend(rig.texts().into_iter().map(|d| d.text));
        rig.wheel_at(over, egui::vec2(0.0, -40.0));
        rig.settle();
    }
    assert!(texts.contains(&format!("Problems: {errors} errors, {warnings} warnings")), "the counts are the command's: {texts:?}");
    for f in findings.iter().filter(|f| !f.inherited) {
        let place = match (f.file.is_empty(), f.path.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!("{}: ", f.file),
            (false, false) => format!("{} {}: ", f.file, f.path),
        };
        let line = format!("{place}{}", f.message);
        assert!(texts.contains(&line), "the finding {line:?} is listed with the command's wording: {texts:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
