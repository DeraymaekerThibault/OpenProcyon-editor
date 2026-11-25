//! T11 (docs/phase5.md P8): the engine's mission templates through the editor's own path (`mission::templates`) into an empty mission
//! of a synthetic project (no game content). Each one expands into a mission that the engine's checker finds no error in, and is
//! ONE undo step.

use std::collections::BTreeMap;

use openprocyon::authoring::{self, Level, Template};
use openprocyon_editor::checks;
use openprocyon_editor::edit::Edit;
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::project::{media_rel, merged_map, merged_media, merged_mission, mission_rel, new_mission};
use openprocyon_editor::mission::templates::{self, Form};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

/// A project with an empty mission `own` that has the world the templates refer to: players, ships, groups, an area, a point, a point
/// set, one speech line and one rule to switch.
fn setup(name: &str) -> (PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-mission-templates-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First"}));
    put(&assets, "maps/mission01/mission.json", &json!({"rules": [], "ships": []}));
    put(&assets, "maps/mission01/media.json", &json!({"speech": {}, "messages": {}, "ship_names": {}}));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    let mut p = Project::open(&m, Some(assets)).unwrap();
    new_mission(&mut p, "own", None).unwrap();
    let rel = mission_rel("own").unwrap();
    let edits = vec![
        Edit::set("/players", json!(["Hero", "Raiders"])),
        Edit::set("/groups", json!({"Hero": [1], "Enemies": [2]})),
        Edit::set(
            "/ships",
            json!([
                {"id": 1, "type": "Ship_Navy_Cutter", "group": "Hero", "name": "Dauntless", "primary": true, "pos": [0.0, 0.0, 0.0]},
                {"id": 2, "type": "Ship_Pirate_Sloop", "group": "Raiders", "name": "Raider", "pos": [400.0, 600.0, 0.0]}
            ]),
        ),
        Edit::set("/volumes", json!({"Gate Area": [[300.0, 500.0], [500.0, 500.0], [500.0, 700.0], [300.0, 700.0]]})),
        Edit::set("/points", json!([{"Name": "The Gate", "Position": [400.0, 600.0, 0.0]}])),
        Edit::set("/environment", json!({"point_sets": {"Arrival": [{"pos": [10.0, 10.0, 0.0]}]}})),
    ];
    p.do_step(&rel, "World", edits).unwrap();
    p.do_step(&media_rel("own").unwrap(), "Speech", vec![Edit::set("/speech/LINE", json!({"text": "Ahoy."}))]).unwrap();
    (root, p)
}

fn mission(p: &Project) -> &Value {
    merged_mission(p, "own").unwrap()
}

fn find(id: &str) -> Template {
    templates::all().iter().find(|t| t.id == id).cloned().unwrap()
}

/// A value for each parameter without a default, taken from the world of [`setup`].
fn params(t: &Template) -> BTreeMap<String, String> {
    t.parameters
        .iter()
        .filter(|p| p.default.is_none())
        .map(|p| {
            let v = match (p.kind.as_str(), p.name.as_str()) {
                ("group", "escorted" | "target" | "ambushers") => "Enemies",
                ("group", _) => "Hero",
                ("volume", _) => "Gate Area",
                ("point", _) => "The Gate",
                ("point_set", _) => "Arrival",
                ("speech_event", _) => "LINE",
                ("rule", _) => "Other Rule",
                _ => "Text",
            };
            (p.name.clone(), v.to_string())
        })
        .collect()
}

fn errors(p: &Project) -> Vec<String> {
    let (m, mp, md) = (merged_mission(p, "own").unwrap(), merged_map(p, "own").unwrap(), merged_media(p, "own").unwrap());
    checks::run(m, mp, md).iter().filter(|f| f.level == Level::Error).map(|f| f.to_string()).collect()
}

#[test]
fn the_picker_lists_the_engines_templates() {
    let ids: Vec<&str> = templates::all().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, authoring::templates().iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
    assert!(ids.len() >= 7);
}

#[test]
fn each_template_expands_into_an_empty_mission_that_checks_clean_in_one_undo_step() {
    for t in templates::all() {
        let (root, mut p) = setup(&t.id);
        // a rule for `Other Rule` (the failsafe switches it) and the starting point of the undo
        let rel = mission_rel("own").unwrap();
        p.do_step(
            &rel,
            "Other rule",
            vec![Edit::set(
                "/rules/0",
                json!({"name": "Other Rule", "Run Once": true, "Is Active": false, "conditions": [{"Type": "World Initialize"}], "actions": [{"Type": "OP Wait", "Seconds": 1}]}),
            )],
        )
        .unwrap();
        let before = mission(&p).clone();
        assert!(errors(&p).is_empty(), "{}: {:?}", t.id, errors(&p));
        let added = templates::expand(&mut p, "own", t, &params(t)).unwrap_or_else(|e| panic!("{}: {e}", t.id));
        assert!(added.rules > 0, "{}", t.id);
        assert_eq!(mdoc::rules(mission(&p)).len(), 1 + added.rules);
        assert_eq!(mdoc::tasks(mission(&p)).len(), added.tasks);
        assert!(errors(&p).is_empty(), "{}: {:?}", t.id, errors(&p));
        assert!(authoring::check_mission(mission(&p), merged_map(&p, "own").unwrap(), merged_media(&p, "own").unwrap(), false).iter().all(|f| f.level != Level::Error));
        // one undo step brings back the mission as it was; redo writes it again
        assert_eq!(p.undo_label(), Some(format!("Add template {}", t.title).as_str()));
        p.undo().unwrap();
        assert_eq!(mission(&p), &before, "{}: undo restores the mission", t.id);
        assert_eq!(p.undo_label(), Some("Other rule"), "{}: one step only", t.id);
        p.redo().unwrap();
        assert_eq!(mdoc::rules(mission(&p)).len(), 1 + added.rules);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn expanding_into_the_bare_new_mission_undoes_to_it() {
    // the mission as "New mission" makes it: no `tasks` key and no `victory`; the template creates them and undo removes them again
    let (root, mut p) = setup("bare");
    let rel = mission_rel("own").unwrap();
    let before = mission(&p).clone();
    assert!(before.get("tasks").is_none() && before.get("victory").is_none());
    let t = find("reach-point");
    templates::expand(&mut p, "own", &t, &params(&t)).unwrap();
    assert_eq!(mission(&p)["victory"], "objectives");
    assert_eq!(mission(&p)["tasks"][0]["Name"], "Reach the point");
    assert_eq!(mission(&p)["tasks"][0]["Active"], json!(false));
    assert_eq!(mission(&p)["tasks"][0]["complete_when"][0]["Entire Group"], "FALSE");
    p.undo().unwrap();
    assert_eq!(mission(&p), &before);
    // a mission that already says how it ends keeps its own value
    p.do_step(&rel, "Victory", vec![Edit::set("/victory", json!("rules"))]).unwrap();
    templates::expand(&mut p, "own", &t, &params(&t)).unwrap();
    assert_eq!(mission(&p)["victory"], "rules");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_refused_expansion_changes_nothing() {
    let (root, mut p) = setup("refused");
    let t = find("sink-group");
    let before = mission(&p).clone();
    let steps = p.undo_label().map(str::to_string);
    // a missing parameter and an unknown one are the engine's errors
    assert!(templates::expand(&mut p, "own", &t, &BTreeMap::new()).unwrap_err().contains("needs a value"));
    let mut unknown = params(&t);
    unknown.insert("colour".into(), "red".into());
    assert!(templates::expand(&mut p, "own", &t, &unknown).unwrap_err().contains("no parameter"));
    // the second time the names are taken
    templates::expand(&mut p, "own", &t, &params(&t)).unwrap();
    let err = templates::expand(&mut p, "own", &t, &params(&t)).unwrap_err();
    assert!(err.contains("already"), "{err}");
    p.undo().unwrap();
    assert_eq!(mission(&p), &before);
    assert_eq!(p.undo_label().map(str::to_string), steps);
    // another name makes room, and the rules stay ordinary rules to edit
    let mut named = params(&t);
    named.insert("name".into(), "Sink them".into());
    templates::expand(&mut p, "own", &t, &named).unwrap();
    templates::expand(&mut p, "own", &t, &{
        let mut n = params(&t);
        n.insert("name".into(), "Sink more".into());
        n
    })
    .unwrap();
    assert_eq!(mdoc::tasks(mission(&p)).len(), 2);
    assert!(errors(&p).is_empty(), "{:?}", errors(&p));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_form_knows_what_is_missing() {
    let t = find("ambush-on-arrival");
    let mut form = Form::new(0);
    assert_eq!(form.missing(&t).len(), 3, "group, area and ambushers have no default");
    form.values.insert("group".into(), "Hero".into());
    form.values.insert("volume".into(), "  ".into());
    assert_eq!(form.missing(&t).len(), 2);
    assert!(!form.params().contains_key("volume"));
}
