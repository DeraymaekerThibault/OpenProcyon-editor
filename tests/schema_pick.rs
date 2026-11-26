//! A name picker closed without choosing starts unfiltered the next time it opens (a stale filter hid names such as a player
//! "Raiders" from the template pickers while "Hero" still showed).
use bevy_egui::egui::{self, Pos2, Rect};
use openprocyon_editor::schema_form::{self as sf, Field, Names};
use serde_json::{Map, Value, json};

struct R {
    ctx: egui::Context,
    names: Names,
    obj: Map<String, Value>,
    t: f64,
}
impl R {
    fn frame(&mut self, mut ev: Vec<egui::Event>) {
        self.t += 0.05;
        ev.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_max(Pos2::ZERO, Pos2::new(900.0, 900.0))), time: Some(self.t), events: ev, ..Default::default() };
        let f = Field { name: "group".into(), kind: "group".into(), default: String::new(), required: true, values: vec![], range: None };
        let (names, obj) = (&self.names, &mut self.obj);
        let mut full = self.ctx.run_ui(raw, |ui| {
            sf::field_widget(ui, &f, obj, names, egui::Id::new("p"));
        });
        full.textures_delta.clear();
    }
    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }
    fn click(&mut self, id: egui::Id) {
        let p = self.ctx.read_response(id).unwrap_or_else(|| panic!("no {id:?}")).rect.center();
        let b = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.settle();
    }
    fn shown(&self, id: egui::Id) -> bool {
        self.ctx.read_response(id).is_some()
    }
}

#[test]
fn a_filter_typed_and_abandoned_does_not_hide_names_on_reopening() {
    let m = json!({"players": ["Hero", "Raiders"], "ships": []});
    let mut r = R { ctx: egui::Context::default(), names: Names::collect(&m, &Value::Null, &Value::Null), obj: Map::new(), t: 0.0 };
    let pid = sf::field_id(egui::Id::new("p"), "group").with("group");
    r.settle();
    r.click(pid.with("btn"));
    assert!(r.shown(pid.with(("opt", "Raiders"))));
    r.click(pid.with("filter"));
    r.frame(vec![egui::Event::Text("Her".into())]);
    r.settle();
    assert!(!r.shown(pid.with(("opt", "Raiders"))) && r.shown(pid.with(("opt", "Hero"))), "the filter narrows the list");
    r.click(pid.with("btn")); // close without choosing
    r.click(pid.with("btn")); // reopen
    assert!(r.shown(pid.with(("opt", "Raiders"))) && r.shown(pid.with(("opt", "Hero"))), "reopened: every name is offered again");
}

// ---- the pickers of the template form (round 4, step 14): a list of names is all reachable ----

use openprocyon_editor::mission::templates::{self as templates, Form};

struct Tpl {
    ctx: egui::Context,
    names: Names,
    form: Form,
    t: f64,
}

impl Tpl {
    fn new(mission: Value) -> Tpl {
        let all = templates::all();
        let form = Form::new(all.iter().position(|x| x.id == "ambush-on-arrival").expect("the engine's template"));
        let mut r = Tpl { ctx: egui::Context::default(), names: Names::collect(&mission, &Value::Null, &Value::Null), form, t: 0.0 };
        r.settle();
        r
    }
    fn frame(&mut self, mut ev: Vec<egui::Event>) {
        self.t += 0.05;
        ev.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_max(Pos2::ZERO, Pos2::new(900.0, 900.0))), time: Some(self.t), events: ev, ..Default::default() };
        let (names, form) = (&self.names, &mut self.form);
        // the form sits in a scrolled panel, as in the Missions tab
        let mut full = self.ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                templates::show(ui, form, names);
            });
        });
        full.textures_delta.clear();
    }
    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }
    fn point(&self, id: egui::Id) -> Pos2 {
        self.ctx.read_response(id).unwrap_or_else(|| panic!("no widget {id:?}")).rect.center()
    }
    fn click(&mut self, id: egui::Id) {
        let p = self.point(id);
        let b = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.settle();
    }
    /// The picker of the template's `group` parameter: (its button, a name's row).
    fn ids() -> (egui::Id, impl Fn(&str) -> egui::Id) {
        let pid = sf::field_id(templates::wid(("param", "group")), "group").with("group");
        (pid.with("btn"), move |n: &str| pid.with(("opt", n.to_string())))
    }
}

fn mission_with_groups(extra: usize) -> Value {
    let groups: serde_json::Map<String, Value> = (0..extra).map(|i| (format!("G{i:02}"), json!([1]))).collect();
    json!({"players": ["Hero", "Raiders"], "groups": groups, "ships": [{"id": 1, "name": "A", "group": "Hero"}, {"id": 2, "name": "B", "group": "RaiderShips"}], "volumes": {}, "rules": []})
}

#[test]
fn the_fourth_name_of_a_picker_in_the_template_form_can_be_clicked() {
    let mut r = Tpl::new(mission_with_groups(0));
    let (btn, row) = Tpl::ids();
    // Hero, Player0 Group, RaiderShips, Raiders: the fourth was hidden under a list cut to three rows
    assert_eq!(r.names.of("group"), ["Hero", "Player0 Group", "RaiderShips", "Raiders"]);
    r.click(btn);
    r.click(row("Raiders"));
    assert_eq!(r.form.values.get("group").map(String::as_str), Some("Raiders"));
}

#[test]
fn a_long_list_in_the_template_form_scrolls_to_its_last_name() {
    let mut r = Tpl::new(mission_with_groups(30));
    let (btn, row) = Tpl::ids();
    r.click(btn);
    let first = r.point(row("G00"));
    // a name past the list's own height is out of reach until the list is scrolled (the wheel over it)
    for _ in 0..40 {
        r.frame(vec![
            egui::Event::PointerMoved(first),
            egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -60.0), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE },
        ]);
    }
    r.settle();
    r.click(row("Raiders"));
    assert_eq!(r.form.values.get("group").map(String::as_str), Some("Raiders"));
}
