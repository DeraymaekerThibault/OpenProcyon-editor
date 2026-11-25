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
