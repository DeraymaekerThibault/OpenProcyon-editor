//! T5 (docs/phase5.md): the rule list UI driven headlessly. A real egui frame per step with pointer, text and key events; widgets are found
//! by the ids `rules_view::wid` and `schema_form::field_id` document, and the add menu (which has no ids of its own) by the text it
//! paints. Synthetic data only, in a temp folder.

mod common;

use bevy_egui::egui::{self, Pos2};
use openprocyon_editor::mission::doc as mdoc;
use openprocyon_editor::mission::project as mproject;
use openprocyon_editor::mission::rules_view::{self as rv, State, wid};
use openprocyon_editor::project::Project;
use openprocyon_editor::schema_form::{self as sf, Schema};
use serde_json::{Value, json};
use std::path::PathBuf;

const SCREEN: (f32, f32) = (1000.0, 3000.0);
const ID: &str = "tst";

/// What the panels work on: the project, the rule list's state and the temp folder (removed on drop).
struct St {
    project: Project,
    st: State,
    root: PathBuf,
}

/// The rule list in a scrolled panel, in the shared rig; `r.project`, `r.st` and `r.root` are its state.
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

impl Rig {
    fn new(name: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("ope-t5-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("mod.json"), r#"{"id": "t", "name": "T", "format": 1}"#).unwrap();
        let project = Project::open(&root, None).unwrap();
        Rig(common::Rig::new(SCREEN, St { project, st: State::default(), root }, |ui, s| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                rv::picker_with(ui, &mut s.project, &mut s.st);
                rv::show_with(ui, &mut s.project, &mut s.st);
            });
        }))
    }

    fn ctx(&self) -> &egui::Context {
        self.0.ctx()
    }

    fn settle(&mut self) {
        self.0.settle();
    }

    fn click_at(&mut self, p: Pos2) {
        self.0.click_at(p);
    }

    fn click(&mut self, id: egui::Id) {
        self.0.click(id);
    }

    fn shown(&self, id: egui::Id) -> bool {
        self.0.shown(id)
    }

    /// Whether a text starting with `text` was painted in the last frame (a hidden row keeps the last response of its id, so ask the text).
    fn painted(&self, text: &str) -> bool {
        self.0.texts().iter().any(|d| d.text.starts_with(text))
    }

    /// The painted text starting with `text` (the last such), as a point inside it.
    fn text_pos(&self, text: &str) -> Pos2 {
        let all = self.0.texts();
        let d = all.iter().rfind(|d| d.text.starts_with(text)).unwrap_or_else(|| panic!("no text '{text}' painted in {:?}", all.iter().map(|d| &d.text).collect::<Vec<_>>()));
        d.rect.min + egui::vec2(6.0, 6.0)
    }

    fn click_text(&mut self, text: &str) {
        let p = self.text_pos(text);
        self.click_at(p);
    }

    fn type_text(&mut self, id: egui::Id, text: &str) {
        self.0.type_into(id, text);
    }

    /// Select all of the focused box (Ctrl+A) and type `text` over it.
    fn replace_text(&mut self, text: &str) {
        self.0.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
        self.0.type_text(text);
    }

    fn key(&mut self, key: egui::Key) {
        self.0.key(key);
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
        self.0.type_text(ty);
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
    r.replace_text("Rule 2");
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
    rv::state(r.ctx()).lock().unwrap().mission = r.st.mission.clone();
    let (ctx, project) = (r.ctx().clone(), &mut r.project);
    let mut seen = vec![];
    let mut full = ctx.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(SCREEN.0, SCREEN.1))), ..Default::default() }, |ui| {
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
    r.click(wid(("rule", 1))); // the selected rule is always listed, so select the one the filter keeps
    r.type_text(wid("filter"), "Rule 2");
    assert!(r.shown(wid(("rule", 1))));
    // hidden rows are not drawn: their id keeps the last frame's response, so ask the painted text
    assert!(r.painted("2. Rule 2"));
    assert!(!r.painted("1. Rule 1") && !r.painted("3. Rule 3"));
}

#[test]
fn the_uses_filter_never_hides_the_selected_rule_and_goes_when_a_rule_is_added() {
    let mut r = Rig::new("uses");
    r.open_mission();
    for _ in 0..3 {
        r.click(wid("add-rule"));
    }
    // no rule names this volume, so the filter lists none: the selected rule (the last added) stays
    r.st.uses = Some(rv::Uses { label: "volume 'Nowhere'".into(), keys: vec![("volume".into(), "Nowhere".into())] });
    r.settle();
    assert!(r.painted("3. Rule 3"), "the selected rule is listed");
    assert!(!r.painted("1. Rule 1") && !r.painted("2. Rule 2"), "the others are filtered out");
    // a new rule clears the filter and is listed with the rest
    r.click(wid("add-rule"));
    assert!(r.st.uses.is_none(), "the stale filter is gone");
    assert!(r.painted("4. Rule 4") && r.painted("1. Rule 1"));
}
