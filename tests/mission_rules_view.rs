//! T5 (docs/phase5.md): the rule list UI driven headlessly. A real egui frame per step with pointer, text and key events; widgets are found
//! by the ids `rules_view::wid` and `schema_form::field_id` document, and the add menu (which has no ids of its own) by the text it
//! paints. Synthetic data only, in a temp folder.

use bevy_egui::egui::{self, Pos2, Rect};
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::project as mproject;
use openprocyon_editor::mission::rules_view::{self as rv, State, wid};
use openprocyon_editor::project::Project;
use openprocyon_editor::schema_form::{self as sf, Schema};
use serde_json::{Value, json};
use std::path::PathBuf;

const SCREEN: Rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1000.0, 3000.0));
const ID: &str = "tst";

struct Rig {
    ctx: egui::Context,
    project: Project,
    st: State,
    root: PathBuf,
    t: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
}

fn collect_text(shape: &egui::epaint::Shape, out: &mut Vec<(String, Pos2)>) {
    match shape {
        egui::epaint::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos)),
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect_text(s, out)),
        _ => {}
    }
}

impl Rig {
    fn new(name: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("ope-t5-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mod.json"), r#"{"id": "t", "name": "T", "format": 1}"#).unwrap();
        let project = Project::open(&root, None).unwrap();
        Rig { ctx: egui::Context::default(), project, st: State::default(), root, t: 0.0, shapes: vec![] }
    }

    fn frame(&mut self, mut events: Vec<egui::Event>) {
        self.t += 0.05;
        events.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        let raw = egui::RawInput { screen_rect: Some(SCREEN), time: Some(self.t), events, ..Default::default() };
        let (project, st) = (&mut self.project, &mut self.st);
        let mut full = self.ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                rv::picker_with(ui, project, st);
                rv::show_with(ui, project, st);
            });
        });
        full.textures_delta.clear();
        self.shapes = full.shapes;
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn click_at(&mut self, p: Pos2) {
        let b = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.settle();
    }

    fn pos(&self, id: egui::Id) -> Pos2 {
        self.ctx.read_response(id).unwrap_or_else(|| panic!("no widget {id:?}")).rect.center()
    }

    fn click(&mut self, id: egui::Id) {
        let p = self.pos(id);
        self.click_at(p);
    }

    fn shown(&self, id: egui::Id) -> bool {
        self.ctx.read_response(id).is_some()
    }

    /// The painted text starting with `text`, as a point inside it.
    fn text_pos(&self, text: &str) -> Pos2 {
        let mut all = vec![];
        self.shapes.iter().for_each(|s| collect_text(&s.shape, &mut all));
        let (_, p) = all.iter().rfind(|(t, _)| t.starts_with(text)).unwrap_or_else(|| panic!("no text '{text}' painted in {:?}", all.iter().map(|(t, _)| t).collect::<Vec<_>>()));
        *p + egui::vec2(6.0, 6.0)
    }

    fn click_text(&mut self, text: &str) {
        let p = self.text_pos(text);
        self.click_at(p);
    }

    fn type_text(&mut self, id: egui::Id, text: &str) {
        self.click(id);
        self.frame(vec![egui::Event::Text(text.into())]);
        self.settle();
    }

    fn key(&mut self, key: egui::Key) {
        let e = |pressed| egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        self.frame(vec![e(true)]);
        self.frame(vec![e(false)]);
        self.settle();
    }

    fn rel(&self) -> String {
        mproject::mission_rel(ID).unwrap()
    }

    fn mission(&self) -> &Value {
        &self.project.doc(&self.rel()).unwrap().edited
    }

    fn rules(&self) -> Vec<Value> {
        mdoc::rules(self.mission()).to_vec()
    }

    /// Makes the mission `tst` through the picker and gives it a volume and a player, as the Ships/Plan tabs would.
    fn open_mission(&mut self) {
        self.settle();
        self.type_text(wid("new-id"), ID);
        self.click(wid("new-mission"));
        assert_eq!(self.st.mission.as_deref(), Some(ID), "status: {}", self.st.status);
        let rel = self.rel();
        let gate = json!({"Gate Area": [[300.0, 500.0], [500.0, 500.0], [500.0, 700.0], [300.0, 700.0]]});
        self.project.do_step(&rel, "Add volume", vec![openprocyon_editor::edit::Edit::set(mdoc::volumes_pointer(), gate)]).unwrap();
        self.settle();
    }

    /// The first Type of the wanted role with a field of `kind`: (Type, field).
    fn type_with(condition: bool, kind: &str) -> (String, String) {
        let s = Schema::engine();
        s.types
            .iter()
            .filter(|t| t.is_condition == condition)
            .find_map(|t| t.fields.iter().find(|f| f.kind == kind).map(|f| (t.name.clone(), f.name.clone())))
            .unwrap_or_else(|| panic!("no {kind} field"))
    }

    /// Adds a condition of `ty` through the add menu: opens it, searches, picks.
    fn add_condition(&mut self, ty: &str) {
        self.click(wid(("add-menu", true)));
        self.click_text("Search types");
        self.frame(vec![egui::Event::Text(ty.into())]);
        self.settle();
        self.click_text(ty);
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_new_mission_then_a_new_rule_are_one_undo_step_each() {
    let mut r = Rig::new("add");
    r.open_mission();
    assert!(r.rules().is_empty());
    assert!(r.project.doc("mod.json").is_some_and(|d| d.edited["missions"].as_array().is_some_and(|m| m.contains(&json!(ID)))), "listed in mod.json");
    r.click(wid("add-rule"));
    assert_eq!(r.rules().len(), 1);
    assert_eq!(mdoc::rule_name(&r.rules()[0]), Some("Rule 1"));
    assert_eq!(r.rules()[0]["Run Once"], json!(true));
    r.click(wid("add-rule"));
    assert_eq!(mdoc::rule_name(&r.rules()[1]), Some("Rule 2"));
    r.project.undo();
    assert_eq!(r.rules().len(), 1, "one step");
    r.settle();
    assert!(r.shown(wid(("rule", 0))) && !r.shown(wid(("rule", 1))));
}

#[test]
fn flags_reorder_duplicate_delete_and_rename_follow_the_rule_operations() {
    let mut r = Rig::new("list");
    r.open_mission();
    for _ in 0..3 {
        r.click(wid("add-rule"));
    }
    r.click(wid(("once", 0)));
    assert_eq!(r.rules()[0]["Run Once"], json!(false));
    r.click(wid(("active", 0)));
    assert_eq!(r.rules()[0]["Is Active"], json!(false));
    r.click(wid(("down", 0)));
    let names = |r: &Rig| r.rules().iter().map(|x| mdoc::rule_name(x).unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(names(&r), ["Rule 2", "Rule 1", "Rule 3"]);
    r.click(wid(("dup", 1)));
    assert_eq!(names(&r), ["Rule 2", "Rule 1", "Rule 1 copy 1", "Rule 3"]);
    assert_eq!(r.st.rule, 2, "the copy is selected");
    r.click(wid(("del", 2)));
    assert_eq!(names(&r), ["Rule 2", "Rule 1", "Rule 3"]);
    // rename: select Rule 3, type, Enter; another rule's name is refused
    r.click(wid(("rule", 2)));
    r.type_text(wid("rename"), "!");
    r.key(egui::Key::Enter);
    assert_eq!(names(&r)[2], "Rule 3!");
    r.click(wid(("rule", 1)));
    r.click(wid("rename"));
    r.frame(vec![egui::Event::Key { key: egui::Key::A, physical_key: Some(egui::Key::A), pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    r.frame(vec![egui::Event::Text("Rule 2".into())]);
    r.key(egui::Key::Enter);
    assert_eq!(names(&r)[1], "Rule 1", "a repeated name is refused");
    assert!(r.st.status.contains("another rule"), "{}", r.st.status);
}

#[test]
fn add_a_condition_from_the_catalogue_edit_its_field_and_undo() {
    let mut r = Rig::new("item");
    r.open_mission();
    r.click(wid("add-rule"));
    let (ty, field) = Rig::type_with(true, "volume");
    r.add_condition(&ty);
    let items = mdoc::rule_items(&r.rules()[0], true).to_vec();
    assert_eq!(items.len(), 1, "{:?}", r.rules());
    assert_eq!(items[0]["Type"], json!(ty));
    assert_eq!(r.project.undo_label(), Some("Add condition"));
    // the volume dropdown offers what the mission defines
    let item_id = wid(("item", true, 0));
    let fid = sf::field_id(item_id, &field);
    r.click(fid.with("btn"));
    r.click(fid.with(("opt", "Gate Area")));
    assert_eq!(mdoc::rule_items(&r.rules()[0], true)[0][&field], json!("Gate Area"));
    assert_eq!(r.project.undo_label(), Some("Edit condition"));
    r.project.undo();
    assert_ne!(mdoc::rule_items(&r.rules()[0], true)[0].get(&field), Some(&json!("Gate Area")), "the edit is one step");
    r.project.undo();
    assert!(mdoc::rule_items(&r.rules()[0], true).is_empty(), "then the add");
}

#[test]
fn a_finding_shows_on_its_field_and_one_click_fixes_it() {
    let mut r = Rig::new("fix");
    r.open_mission();
    r.click(wid("add-rule"));
    let (ty, field) = Rig::type_with(true, "volume");
    r.add_condition(&ty);
    // a near miss written by another section or by hand
    let rel = r.rel();
    let ptr = mdoc::rule_item_pointer(0, true, 0) + "/" + &field;
    r.project.do_step(&rel, "typo", vec![openprocyon_editor::edit::Edit::set(ptr, json!("Gate Aera"))]).unwrap();
    r.settle();
    let found: Vec<_> = r.st.findings().iter().filter(|f| f.rule_index == Some(0) && f.field == field).collect();
    assert_eq!(found.len(), 1, "{:?}", r.st.findings());
    assert_eq!(found[0].suggestion.as_deref(), Some("Gate Area"));
    let fix = sf::field_id(wid(("item", true, 0)), &field).with(("fix", 0));
    assert!(r.shown(fix), "the fix is drawn under the field");
    r.click(fix);
    assert_eq!(mdoc::rule_items(&r.rules()[0], true)[0][&field], json!("Gate Area"));
    assert!(r.st.findings().iter().all(|f| f.field != field), "the check ran again: {:?}", r.st.findings());
    r.project.undo();
    assert_eq!(mdoc::rule_items(&r.rules()[0], true)[0][&field], json!("Gate Aera"), "the fix is one undo step");
    r.settle();
    assert!(r.st.findings().iter().any(|f| f.field == field), "and the finding is back");
}

#[test]
fn items_reorder_and_remove_each_as_one_undo_step() {
    let mut r = Rig::new("typing");
    r.open_mission();
    r.click(wid("add-rule"));
    // two Types with a plain text field would do; use any two conditions and move them
    let (ty, _) = Rig::type_with(true, "volume");
    r.add_condition(&ty);
    let (ty2, _) = Rig::type_with(true, "flag");
    r.add_condition(&ty2);
    let types = |r: &Rig| mdoc::rule_items(&r.rules()[0], true).iter().map(|i| i["Type"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(types(&r), [ty.clone(), ty2.clone()]);
    r.click(wid(("item-down", true, 0)));
    assert_eq!(types(&r), [ty2.clone(), ty.clone()]);
    r.click(wid(("item-del", true, 0)));
    assert_eq!(types(&r), std::slice::from_ref(&ty));
    r.project.undo();
    r.project.undo();
    assert_eq!(types(&r), [ty, ty2]);
}

#[test]
fn every_section_reads_the_pickers_mission() {
    use openprocyon_editor::mission::{ships, speech, tasks};
    let mut r = Rig::new("one-selection");
    r.open_mission();
    // the rig keeps its own State; the tab keeps the same one in egui memory, which the sections read
    rv::state(&r.ctx).lock().unwrap().mission = r.st.mission.clone();
    let (ctx, project) = (r.ctx.clone(), &mut r.project);
    let mut seen = vec![];
    let mut full = ctx.run_ui(egui::RawInput { screen_rect: Some(SCREEN), ..Default::default() }, |ui| {
        seen.push(rv::current_mission(ui, project));
        ships::show(ui, project);
        tasks::show(ui, project);
        speech::show(ui, project);
    });
    full.textures_delta.clear();
    assert!(!seen.is_empty() && seen.iter().all(|s| s.as_deref() == Some(ID)), "{seen:?}");
    assert_eq!(rv::selected_mission(&ctx).as_deref(), Some(ID));
}

#[test]
fn save_writes_the_mission_files() {
    let mut r = Rig::new("save");
    r.open_mission();
    r.click(wid("add-rule"));
    r.click(wid("save"));
    let written: Value = serde_json::from_slice(&std::fs::read(r.root.join("maps/tst/mission.json")).unwrap()).unwrap();
    assert_eq!(mdoc::rules(&written).len(), 1);
    assert!(r.root.join("maps/tst/media.json").is_file() && r.root.join("maps/tst/map.json").is_file());
    assert!(r.project.dirty_rels().is_empty());
}

#[test]
fn the_list_filters_by_text_and_by_problems() {
    let mut r = Rig::new("filter");
    r.open_mission();
    for _ in 0..3 {
        r.click(wid("add-rule"));
    }
    r.type_text(wid("filter"), "Rule 2");
    assert!(r.shown(wid(("rule", 1))));
    // hidden rows are not drawn: their id keeps the last frame's response, so ask the painted text
    let mut all = vec![];
    r.shapes.iter().for_each(|s| collect_text(&s.shape, &mut all));
    assert!(all.iter().any(|(t, _)| t.starts_with("2. Rule 2")));
    assert!(!all.iter().any(|(t, _)| t.starts_with("1. Rule 1") || t.starts_with("3. Rule 3")), "{all:?}");
}
