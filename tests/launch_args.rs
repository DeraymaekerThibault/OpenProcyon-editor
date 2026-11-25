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
        {"level": "warning", "file": "", "path": "", "message": "unchecked"}]}"#;
    let c = parse_mod_check(out).unwrap();
    assert_eq!((c.errors(), c.warnings()), (1, 1));
    assert_eq!(c.findings[0], ModFinding { error: true, file: "ships/A/ship.json".into(), path: "guns.G".into(), message: "no such gun".into() });
    assert_eq!(c.findings[0].line(), "ships/A/ship.json: guns.G: no such gun");
    assert_eq!(c.findings[1].line(), "unchecked");
    assert!(parse_mod_check("not json").is_err());
    assert!(parse_mod_check("{}").is_err());
}
