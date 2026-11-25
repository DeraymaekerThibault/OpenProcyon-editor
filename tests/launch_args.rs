//! The argument lists of game launches and of the mod check, built without a game binary.

use openprocyon_editor::launch::{Launch, Lobby, ModFinding, check_mod_args, parse_mod_check};
use std::ffi::OsString;
use std::path::Path;

fn os(a: &[&str]) -> Vec<OsString> {
    a.iter().map(OsString::from).collect()
}

#[test]
fn a_lobby_launch_has_the_spec_and_no_map_flag() {
    let mut l = Launch { only_mod: false, ..Launch::new(Path::new("/m/gates"), "red_rover") };
    l.lobby = Some(Lobby { computers: 3, ..Lobby::default() });
    assert_eq!(l.args(), os(&["--mod", "/m/gates", "--lobby", "map=red_rover;points=1000;limit=10;slots=human,3*computer"]));
    // the ship shortcut does not apply to a lobby: Auto Assign picks the fleet
    l.ship = Some("Ship_X".into());
    assert!(!l.args().contains(&"--ship".into()) && !l.args().contains(&"--map".into()));
    l.lobby = Some(Lobby { points: 2000, limit: 5, computers: 0, seed: Some(7) });
    assert_eq!(l.args()[3], OsString::from("map=red_rover;points=2000;limit=5;slots=human;seed=7"));
}

#[test]
fn only_mod_follows_the_mod_and_is_on_by_default() {
    let mut l = Launch::new(Path::new("/m/gates"), "gate_ambush");
    assert_eq!(openprocyon::modding::ONLY_MOD, "--only-mod", "the engine's flag string");
    assert_eq!(l.args(), os(&["--mod", "/m/gates", "--only-mod", "--map", "gate_ambush"]));
    l.only_mod = false;
    assert!(!l.args().contains(&"--only-mod".into()));
    l.only_mod = true;
    l.lobby = Some(Lobby::default());
    assert_eq!(l.args()[..3], os(&["--mod", "/m/gates", "--only-mod"])[..]);
    assert_eq!(l.args()[3], OsString::from("--lobby"));
}

#[test]
fn the_mod_check_runs_on_the_absolute_folder_with_json() {
    assert_eq!(check_mod_args(Path::new("/m/gates")), os(&["--check-mod", "/m/gates", "--json"]));
    assert!(Path::new(&check_mod_args(Path::new("mods/gates"))[1]).is_absolute());
}

#[test]
fn the_mod_check_output_is_read() {
    let out = r#"{"mod": "/m/g", "engine": "0.2.0", "errors": 1, "warnings": 1, "findings": [
        {"level": "error", "file": "ships/A/ship.json", "path": "guns.G", "message": "no such gun"},
        {"level": "warning", "file": "", "path": "", "message": "unchecked"},
        {"level": "warning", "file": "maps/r/map.json", "path": "", "message": "retail has it", "inherited": true}]}"#;
    let c = parse_mod_check(out).unwrap();
    assert_eq!((c.errors(), c.warnings(), c.inherited().len()), (1, 1, 1), "inherited warnings are neither errors nor the mod's warnings");
    assert_eq!(c.own().count(), 2);
    assert_eq!(c.findings[0], ModFinding { error: true, inherited: false, file: "ships/A/ship.json".into(), path: "guns.G".into(), message: "no such gun".into() });
    assert_eq!(c.findings[0].line(), "ships/A/ship.json: guns.G: no such gun");
    assert_eq!(c.findings[1].line(), "unchecked");
    assert!(parse_mod_check("not json").is_err());
    assert!(parse_mod_check("{}").is_err());
}

#[test]
fn a_mission_launch_uses_the_mission_flag() {
    let mut l = Launch { only_mod: false, ..Launch::new(Path::new("/m/gates"), "mission03") };
    l.mission = Some(3);
    assert_eq!(l.args(), os(&["--mod", "/m/gates", "--mission", "3"]));
}

#[test]
fn debug_flags_follow_in_order_and_autoplay_adds_script_and_environment() {
    let mut l = Launch { only_mod: false, ..Launch::new(Path::new("/m/gates"), "red_rover") };
    assert_eq!(l.envs(), vec![("NO_COLOR", "1".to_string())]);
    l.fire_rules = vec!["Open the gate".into(), "Say hi".into()];
    l.start_at = Some((100.0, -25.5));
    l.rule_trace = true;
    l.autoplay = Some(600);
    assert_eq!(
        l.args(),
        os(&["--mod", "/m/gates", "--map", "red_rover", "--fire-rule", "Open the gate", "--fire-rule", "Say hi", "--start-at", "100,-25.5", "--rule-trace", "--script", "mission"])
    );
    assert_eq!(l.envs(), vec![("NO_COLOR", "1".to_string()), ("PROCYON_AUTOPLAY", "600".to_string())]);
    l.shot = Some(openprocyon_editor::launch::Shot { file: "/s/a.png".into(), frames: vec![5], dist: None, yaw: None, pitch: None });
    let a = l.args();
    let shot = a.iter().position(|x| x == "--shot-frame").unwrap();
    assert!(shot < a.iter().position(|x| x == "--fire-rule").unwrap());
}

#[test]
fn the_rule_report_flag_comes_after_the_trace_and_before_the_script() {
    let mut l = Launch { only_mod: false, ..Launch::new(Path::new("/m/gates"), "mission03") };
    assert!(!l.args().iter().any(|a| a == "--rule-report"));
    l.rule_trace = true;
    l.rule_report = Some("/tmp/r.json".into());
    l.autoplay = Some(60);
    assert_eq!(l.args(), os(&["--mod", "/m/gates", "--map", "mission03", "--rule-trace", "--rule-report", "/tmp/r.json", "--script", "mission"]));
}
