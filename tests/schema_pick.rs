//! The name pickers: one closed without choosing starts unfiltered the next time it opens (a stale filter hid names such as a player
//! "Raiders" from the template pickers while "Hero" still showed), and in the template form every name of a list is reachable.
mod common;

use bevy_egui::egui;
use common::Rig;
use openprocyon_editor::mission::templates::{self as templates, Form};
use openprocyon_editor::schema_form::{self as sf, Field, Names};
use serde_json::{Map, Value, json};

#[test]
fn a_filter_typed_and_abandoned_does_not_hide_names_on_reopening() {
    let m = json!({"players": ["Hero", "Raiders"], "ships": []});
    let f = Field { name: "group".into(), kind: "group".into(), default: String::new(), required: true, values: vec![], range: None };
    let mut r = Rig::new((900.0, 900.0), (Names::collect(&m, &Value::Null, &Value::Null), Map::new()), move |ui, (names, obj)| {
        sf::field_widget(ui, &f, obj, names, egui::Id::new("p"));
    });
    let pid = sf::field_id(egui::Id::new("p"), "group").with("group");
    r.click(pid.with("btn"));
    assert!(r.shown(pid.with(("opt", "Raiders"))));
    r.type_into(pid.with("filter"), "Her");
    assert!(!r.shown(pid.with(("opt", "Raiders"))) && r.shown(pid.with(("opt", "Hero"))), "the filter narrows the list");
    r.click(pid.with("btn")); // close without choosing
    r.click(pid.with("btn")); // reopen
    assert!(r.shown(pid.with(("opt", "Raiders"))) && r.shown(pid.with(("opt", "Hero"))), "reopened: every name is offered again");
}

#[test]
fn closing_a_picker_with_escape_also_forgets_the_filter() {
    let m = json!({"players": ["Hero", "Raiders"], "ships": []});
    let f = Field { name: "group".into(), kind: "group".into(), default: String::new(), required: true, values: vec![], range: None };
    let mut r = Rig::new((900.0, 900.0), (Names::collect(&m, &Value::Null, &Value::Null), Map::new()), move |ui, (names, obj)| {
        sf::field_widget(ui, &f, obj, names, egui::Id::new("p"));
    });
    let pid = sf::field_id(egui::Id::new("p"), "group").with("group");
    r.click(pid.with("btn"));
    r.type_into(pid.with("filter"), "Her");
    r.key(egui::Key::Escape);
    assert!(!r.shown(pid.with(("opt", "Hero"))), "Escape closed the list");
    r.click(pid.with("btn"));
    assert!(r.shown(pid.with(("opt", "Raiders"))) && r.shown(pid.with(("opt", "Hero"))), "reopened: every name is offered again");
}

/// The template form of "Ambush on arrival" in a scrolled panel, as in the Missions tab, over a mission with `extra` groups.
fn template_form(extra: usize) -> Rig<'static, (Names, Form)> {
    let index = templates::all().iter().position(|x| x.id == "ambush-on-arrival").expect("the engine's template");
    let mission = mission_with_groups(extra);
    Rig::new((900.0, 900.0), (Names::collect(&mission, &Value::Null, &Value::Null), Form::new(index)), |ui, (names, form)| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            templates::show(ui, form, names);
        });
    })
}

/// The picker of the template's `group` parameter: its button and a function from a name to its row.
fn group_picker() -> (egui::Id, impl Fn(&str) -> egui::Id) {
    let pid = sf::field_id(templates::wid(("param", "group")), "group").with("group");
    (pid.with("btn"), move |n: &str| pid.with(("opt", n.to_string())))
}

fn mission_with_groups(extra: usize) -> Value {
    let groups: serde_json::Map<String, Value> = (0..extra).map(|i| (format!("G{i:02}"), json!([1]))).collect();
    json!({"players": ["Hero", "Raiders"], "groups": groups, "ships": [{"id": 1, "name": "A", "group": "Hero"}, {"id": 2, "name": "B", "group": "RaiderShips"}], "volumes": {}, "rules": []})
}

#[test]
fn the_fourth_name_of_a_picker_in_the_template_form_can_be_clicked() {
    let mut r = template_form(0);
    let (btn, row) = group_picker();
    // Hero, Player0 Group, RaiderShips, Raiders: the fourth was hidden under a list cut to three rows
    assert_eq!(r.state().0.of("group"), ["Hero", "Player0 Group", "RaiderShips", "Raiders"]);
    r.click(btn);
    r.click(row("Raiders"));
    assert_eq!(r.state().1.values.get("group").map(String::as_str), Some("Raiders"));
}

#[test]
fn a_long_list_in_the_template_form_scrolls_to_its_last_name() {
    let mut r = template_form(30);
    let (btn, row) = group_picker();
    r.click(btn);
    let first = r.point(row("G00"));
    // a name past the list's own height is out of reach until the list is scrolled (the wheel over it)
    for _ in 0..40 {
        r.wheel_at(first, egui::vec2(0.0, -60.0));
    }
    r.settle();
    r.click(row("Raiders"));
    assert_eq!(r.state().1.values.get("group").map(String::as_str), Some("Raiders"));
}
