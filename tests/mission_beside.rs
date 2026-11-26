//! T9 (docs/phase5.md P6): the plan beside the rule list. Direction one: the selected rule's names are found on the plan (pure, and
//! painted gold in a real egui frame). Direction two: a pick on the plan sets the list's "uses this" filter, and the list shows only the
//! rules (and names the tasks) that read it. Synthetic missions in a temp folder, no game content.

mod common;

use bevy_egui::egui::{self, Pos2};
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::geometry::{Item, Kind};
use openprocyon_editor::mission::plan;
use openprocyon_editor::mission::project::open_mission;
use openprocyon_editor::mission::rules_view::{self as rv, Uses, wid};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::PathBuf;

const ID: &str = "m";
const SCREEN: (f32, f32) = (1000.0, 1400.0);
const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 205, 60);

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

/// Rule 1 names the volume Area, the path Run and the group Pack (ships 1 and 2 by id); rule 2 names the point Gate and the unit
/// `Pack,Lone`; rule 3 the volume Area again; the task names Area.
fn mission() -> Value {
    let (cv, fv) = schema_type("condition", "volume");
    let (ap, fp) = schema_type("action", "path");
    let (cg, fg) = schema_type("condition", "group");
    let (ct, ft) = schema_type("action", "point");
    json!({
        "players": ["P1", "P2"],
        "ships": [
            {"id": 1, "name": "A", "group": "P2", "pos": [100.0, 100.0, 0.0]},
            {"id": 2, "name": "B", "group": "P2", "pos": [150.0, 100.0, 0.0]},
            {"id": 3, "name": "Lone", "group": "P1", "pos": [-100.0, 0.0, 0.0]}
        ],
        "groups": {"Pack": [1, 2]},
        "player_start": {"name": "P1", "start": [0.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0]},
        "points": [{"Name": "Gate", "Position": [300.0, 300.0, 0.0]}],
        "volumes": {"Area": [[0, 0], [100, 0], [100, 100]], "Unused": [[200, 200], [250, 200], [250, 250]]},
        "paths": {"Run": [[0, 0, 0], [100, 0, 10]]},
        "tasks": [{"Name": "T", "complete_when": [{"Type": cv, (fv.as_str()): "Area"}]}],
        "rules": [
            {"name": "r1", "conditions": [{"Type": cv, (fv.as_str()): "Area"}, {"Type": cg, (fg.as_str()): "Pack"}], "actions": [{"Type": ap, (fp.as_str()): "Run"}]},
            {"name": "r2", "conditions": [{"Type": cg, (fg.as_str()): "Pack,Lone"}], "actions": [{"Type": ct, (ft.as_str()): "Gate"}]},
            {"name": "r3", "conditions": [{"Type": cv, (fv.as_str()): "Area"}], "actions": []}
        ]
    })
}

/// The project, and the temp folder it lives in (removed on drop).
struct St {
    project: Project,
    root: PathBuf,
}

/// The rule list and the plan, one above the other, in the shared rig; `rig.project` is its state.
struct Rig(common::Rig<'static, St>);

impl std::ops::Deref for Rig {
    type Target = St;
    fn deref(&self) -> &St {
        self.0.state()
    }
}

impl std::ops::DerefMut for Rig {
    fn deref_mut(&mut self) -> &mut St {
        self.0.state_mut()
    }
}

fn collect(shape: &egui::epaint::Shape, texts: &mut Vec<String>, gold: &mut usize) {
    match shape {
        egui::epaint::Shape::Text(t) => texts.push(t.galley.text().to_string()),
        egui::epaint::Shape::Circle(c) if c.stroke.color == GOLD => *gold += 1,
        egui::epaint::Shape::Path(p) if p.stroke.color == egui::epaint::ColorMode::Solid(GOLD) => *gold += 1,
        egui::epaint::Shape::LineSegment { stroke, .. } if stroke.color == GOLD => *gold += 1,
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, texts, gold)),
        _ => {}
    }
}

impl Rig {
    fn new(name: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("ope-t9-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("maps/m")).unwrap();
        std::fs::write(root.join("mod.json"), r#"{"id": "t", "name": "T", "format": 1}"#).unwrap();
        std::fs::write(root.join("maps/m/mission.json"), serde_json::to_string_pretty(&mission()).unwrap()).unwrap();
        std::fs::write(root.join("maps/m/map.json"), r#"{"name": "M", "world_half": 500.0}"#).unwrap();
        std::fs::write(root.join("maps/m/media.json"), r#"{"speech": {}, "messages": {}, "ship_names": {}}"#).unwrap();
        let mut project = Project::open(&root, None).unwrap();
        open_mission(&mut project, ID).unwrap();
        let rig = Rig(common::Rig::new(SCREEN, St { project, root }, |ui, s| {
            rv::show(ui, &mut s.project);
            plan::show(ui, &mut s.project);
        }));
        rv::state(rig.ctx()).lock().unwrap().mission = Some(ID.into());
        rig
    }

    fn ctx(&self) -> &egui::Context {
        self.0.ctx()
    }

    fn settle(&mut self) {
        self.0.settle();
    }

    /// What the last frame painted: its texts and how many gold shapes.
    fn painted(&self) -> (Vec<String>, usize) {
        let (mut texts, mut gold) = (vec![], 0);
        self.0.shapes().iter().for_each(|s| collect(&s.shape, &mut texts, &mut gold));
        (texts, gold)
    }

    fn rules_listed(&self) -> Vec<&'static str> {
        let (texts, _) = self.painted();
        ["1. r1", "2. r2", "3. r3"].into_iter().filter(|r| texts.iter().any(|t| t.starts_with(r))).collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_rule_names_its_things_on_the_plan() {
    let m = mission();
    let r1 = plan::rule_things(&m, 0);
    assert!(r1.has_geo(Kind::Volume, "Area") && r1.has_geo(Kind::Path, "Run"));
    assert!(!r1.has_geo(Kind::Volume, "Unused"));
    assert_eq!(r1.ships, vec![0, 1], "group Pack is ships 1 and 2 by id");
    // a Group,Unit text names the group's ships and the unit's ship; a point is a point
    let r2 = plan::rule_things(&m, 1);
    assert!(r2.has_geo(Kind::Point, "Gate"));
    assert_eq!(r2.ships, vec![0, 1, 2]);
    assert_eq!(plan::rule_things(&m, 2).geo, vec![(Kind::Volume, "Area".to_string())]);
    assert!(plan::rule_things(&m, 9).geo.is_empty());
}

#[test]
fn the_selected_rule_is_painted_gold_on_the_plan() {
    let mut rig = Rig::new("gold");
    rig.settle();
    let (_, none_selected_gold) = rig.painted();
    // rule 1 (index 0): the volume outline, the path's segment and two ships
    assert!(none_selected_gold >= 4, "gold shapes of rule 1: {none_selected_gold}");
    rv::state(rig.ctx()).lock().unwrap().rule = 2;
    rig.settle();
    let (_, gold3) = rig.painted();
    // rule 3 names only the volume Area: fewer gold shapes than rule 1
    assert!(gold3 >= 1 && gold3 < none_selected_gold, "rule 3 gold {gold3}, rule 1 gold {none_selected_gold}");
}

#[test]
fn a_pick_on_the_plan_lists_the_rules_and_tasks_that_read_it() {
    let m = mission();
    let volume = plan::uses_of_items(&m, &[Item::Vertex(mdoc::GeoTable::Volumes, "Area".into(), 1)]).unwrap();
    assert_eq!(volume.label, "volume 'Area'");
    assert_eq!(rv::rules_using(&m, &volume), (vec![0, 2], vec![0]));
    let unused = plan::uses_of_items(&m, &[Item::Vertex(mdoc::GeoTable::Volumes, "Unused".into(), 0)]).unwrap();
    assert_eq!(rv::rules_using(&m, &unused), (vec![], vec![]));
    // ship B is in group Pack (rules 1 and 2); ship Lone is named by rule 2 (and its player P1 by no rule)
    let b = plan::uses_of_items(&m, &[Item::Ship(1)]).unwrap();
    assert_eq!(rv::rules_using(&m, &b).0, vec![0, 1]);
    let lone = plan::uses_of_items(&m, &[Item::Ship(2)]).unwrap();
    assert_eq!(rv::rules_using(&m, &lone).0, vec![1]);
    let gate = plan::uses_of_items(&m, &[Item::Point(0)]).unwrap();
    assert_eq!(rv::rules_using(&m, &gate).0, vec![1]);
    assert!(plan::uses_of_items(&m, &[]).is_none());
}

#[test]
fn the_list_shows_only_the_rules_that_use_the_pick_and_its_chip_clears_it() {
    let mut rig = Rig::new("filter");
    rig.settle();
    assert_eq!(rig.rules_listed(), vec!["1. r1", "2. r2", "3. r3"]);
    let uses = plan::uses_of_items(&mission(), &[Item::Vertex(mdoc::GeoTable::Volumes, "Area".into(), 0)]).unwrap();
    rv::state(rig.ctx()).lock().unwrap().uses = Some(uses);
    rig.settle();
    assert_eq!(rig.rules_listed(), vec!["1. r1", "3. r3"]);
    let (texts, _) = rig.painted();
    assert!(texts.iter().any(|t| t.contains("also read by the task(s): T")), "tasks named: {texts:?}");
    // the chip clears it
    rig.0.click(wid("uses-clear"));
    assert_eq!(rig.rules_listed(), vec!["1. r1", "2. r2", "3. r3"]);
    assert!(rv::state(rig.ctx()).lock().unwrap().uses.is_none());
}

#[test]
fn a_click_on_the_plan_sets_the_filter() {
    let mut rig = Rig::new("click");
    rig.settle();
    // the volume's label is painted at its first corner marker
    let mut at = None;
    for s in rig.0.shapes() {
        fn find(s: &egui::epaint::Shape, at: &mut Option<Pos2>) {
            match s {
                egui::epaint::Shape::Text(t) if t.galley.text() == "Unused" => *at = Some(t.pos),
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| find(s, at)),
                _ => {}
            }
        }
        find(&s.shape, &mut at);
    }
    let marker = at.expect("the volume Unused is labelled on the plan");
    // the label sits beside its marker: try a few points around it until the canvas picks it
    let mut got = false;
    'search: for dx in [-12.0f32, -8.0, -4.0, 0.0, 4.0, 8.0, 12.0] {
        for dy in [-14.0f32, -8.0, 0.0, 8.0, 14.0] {
            let p = marker + egui::vec2(dx, dy);
            rig.0.click_at(p);
            if rv::state(rig.ctx()).lock().unwrap().uses.is_some() {
                got = true;
                break 'search;
            }
        }
    }
    assert!(got, "no click near the label of 'Unused' picked it");
    let uses: Uses = rv::state(rig.ctx()).lock().unwrap().uses.clone().unwrap();
    assert_eq!(uses.label, "volume 'Unused'");
    rig.settle();
    // no rule reads Unused; only the selected rule stays listed (the detail pane shows it, so the list may not hide it)
    assert_eq!(rig.rules_listed(), vec!["1. r1"], "no rule reads Unused");
}
