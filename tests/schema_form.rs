//! The rule-field widgets (T3) driven headlessly: a real egui frame per step, pointer and text events, and what the form says through
//! its edits. Widgets are found by the ids `schema_form` documents (`field_id`, `child_id`), read back with `Context::read_response`.

use bevy_egui::egui::{self, Pos2, Rect};
use openprocyon::authoring::check_mission;
use openprocyon_editor::checks::Finding;
use openprocyon_editor::edit::{self, Edit};
use openprocyon_editor::schema_form::{self as sf, Names, Schema, Widget};
use serde_json::{Value, json};

const SCREEN: Rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(900.0, 1400.0));

fn mission() -> Value {
    json!({
        "players": ["Hero", "Raiders"],
        "groups": {"RaiderShips": [11, 12]},
        "ships": [
            {"id": 10, "type": "Ship_Navy_Cutter", "group": "Hero", "name": "Dauntless", "primary": true},
            {"id": 11, "type": "Ship_Pirate_Sloop", "group": "Raiders", "name": "Raider One"},
            {"id": 12, "type": "Ship_Pirate_Sloop", "group": "Raiders", "name": "Raider Two"}
        ],
        "volumes": {"Gate Area": [[300.0, 500.0], [500.0, 500.0], [500.0, 700.0], [300.0, 700.0]], "Far Area": [[0.0, 0.0], [9.0, 0.0]]},
        "paths": {"Patrol": [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]]},
        "points": [{"Name": "The Gate", "Position": [400.0, 600.0, 0.0]}],
        "tasks": [{"Name": "Reach the gate", "text": "Sail.", "Active": false, "required": true}],
        "rules": []
    })
}

fn names_of(m: &Value) -> Names {
    Names::collect(m, &Value::Null, &Value::Null)
}

/// The first Type of the wanted role with a field of `kind`: (Type, field).
fn type_with(s: &Schema, condition: bool, kind: &str) -> (String, String) {
    s.types
        .iter()
        .filter(|t| t.is_condition == condition)
        .find_map(|t| t.fields.iter().find(|f| f.kind == kind).map(|f| (t.name.clone(), f.name.clone())))
        .unwrap_or_else(|| panic!("no {kind} field in the schema"))
}

struct Rig {
    ctx: egui::Context,
    schema: Schema,
    names: Names,
    item: Value,
    is_cond: bool,
    problems: Vec<Finding>,
    edits: Vec<Edit>,
    t: f64,
}

fn at(ctx: &egui::Context, id: egui::Id) -> Pos2 {
    ctx.read_response(id).unwrap_or_else(|| panic!("no widget {id:?}")).rect.center()
}

impl Rig {
    fn new(item: Value, is_cond: bool, m: &Value) -> Rig {
        Rig { ctx: egui::Context::default(), schema: Schema::engine(), names: names_of(m), item, is_cond, problems: vec![], edits: vec![], t: 0.0 }
    }

    fn id() -> egui::Id {
        egui::Id::new("item")
    }

    /// One frame; the edits it makes are applied to the item (as the caller's `do_step` would) and collected in `self.edits`.
    fn frame(&mut self, mut events: Vec<egui::Event>) {
        self.t += 0.05;
        events.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        let raw = egui::RawInput { screen_rect: Some(SCREEN), time: Some(self.t), events, ..Default::default() };
        let (schema, names, item, is_cond) = (&self.schema, &self.names, &self.item, self.is_cond);
        let problems: Vec<&Finding> = self.problems.iter().collect();
        let mut got = vec![];
        let mut full = self.ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                got = sf::item_form_edits(ui, schema, names, item, is_cond, &problems, "/item", Rig::id());
            });
        });
        full.textures_delta.clear();
        for e in &got {
            let rel = e.pointer.strip_prefix("/item").unwrap_or("");
            let mut doc = json!({"item": std::mem::take(&mut self.item)});
            match &e.new {
                Some(v) => {
                    edit::apply(&mut doc, &Edit::set(format!("/item{rel}"), v.clone())).unwrap();
                }
                None => {
                    edit::apply(&mut doc, &Edit::remove(format!("/item{rel}"))).unwrap();
                }
            }
            self.item = doc["item"].take();
        }
        self.edits.extend(got);
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn click(&mut self, id: egui::Id) {
        let p = at(&self.ctx, id);
        let b = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.settle();
    }

    fn type_text(&mut self, id: egui::Id, text: &str) {
        self.click(id);
        self.frame(vec![egui::Event::Text(text.into())]);
        self.settle();
    }

    fn shown(&self, id: egui::Id) -> bool {
        self.ctx.read_response(id).is_some()
    }
}

fn fid(field: &str) -> egui::Id {
    sf::field_id(Rig::id(), field)
}

#[test]
fn every_field_kind_has_a_widget_and_the_nested_kinds_are_not_plain() {
    let s = Schema::engine();
    for t in &s.types {
        for f in &t.fields {
            let w = sf::widget_for(&f.kind);
            assert_eq!(matches!(w, Widget::ConditionList | Widget::Condition), f.kind.starts_with("condition"), "{} / {}", t.name, f.name);
        }
    }
    assert_eq!(sf::widget_for("group"), Widget::Spec);
    for k in ["volume", "path", "player", "task", "point"] {
        assert_eq!(sf::widget_for(k), Widget::Pick, "{k}");
    }
    for k in ["flag", "timer", "counter"] {
        assert_eq!(sf::widget_for(k), Widget::Free, "{k}");
    }
}

#[test]
fn accepted_names_feed_the_dropdowns_per_kind() {
    let n = names_of(&mission());
    assert!(n.of("volume").contains(&"Gate Area".to_string()), "{:?}", n.of("volume"));
    assert!(!n.of("volume").contains(&"Far Area".to_string()), "a volume under three vertices is an empty region, not offered");
    assert_eq!(n.of("path"), ["Patrol"]);
    assert_eq!(n.of("point"), ["The Gate"]);
    assert_eq!(n.of("task"), ["Reach the gate"]);
    assert!(n.of("player").contains(&"Hero".to_string()));
    assert!(n.of("group").contains(&"RaiderShips".to_string()) && n.of("group").contains(&"Hero".to_string()));
    assert!(!n.of("group").contains(&"Dauntless".to_string()), "a unit's own name goes after the comma");
    assert!(n.of("unit").contains(&"Dauntless".to_string()));
}

#[test]
fn spec_helpers_split_and_join_the_group_unit_composite() {
    assert_eq!(sf::split_spec("RaiderShips"), ("RaiderShips", None));
    assert_eq!(sf::split_spec("Raiders, Raider One"), ("Raiders", Some("Raider One")));
    assert_eq!(sf::join_spec("Raiders", Some("Raider One")), "Raiders,Raider One");
    assert_eq!(sf::join_spec("Raiders", None), "Raiders");
    let pool: Vec<String> = ["Gate Area", "Far Gate", "Other"].map(String::from).to_vec();
    let hits = sf::matches(&pool, "gate", 10);
    assert_eq!(hits, ["Gate Area", "Far Gate"], "prefix matches first, case-insensitive");
}

#[test]
fn a_volume_field_is_a_typeahead_dropdown_that_writes_the_pick() {
    let m = mission();
    let s = Schema::engine();
    let (ty, field) = type_with(&s, true, "volume");
    let mut rig = Rig::new(json!({"Type": ty}), true, &m);
    rig.settle();
    assert!(!rig.shown(fid(&field).with(("opt", "Gate Area"))), "closed: no options");
    rig.click(fid(&field).with("btn"));
    assert!(rig.shown(fid(&field).with(("opt", "Gate Area"))), "open lists what the mission defines");
    // Type-ahead narrows the list.
    rig.type_text(fid(&field).with("filter"), "zzz");
    assert!(!rig.shown(fid(&field).with(("opt", "Gate Area"))), "no match: the list is empty");
    rig.frame(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    rig.frame(vec![egui::Event::Text("gate".into())]);
    rig.settle();
    rig.click(fid(&field).with(("opt", "Gate Area")));
    assert_eq!(rig.item[&field], "Gate Area");
    assert_eq!(rig.edits.last().unwrap(), &Edit { pointer: format!("/item/{field}"), old: None, new: Some(json!("Gate Area")) });
    assert!(!rig.shown(fid(&field).with(("opt", "Gate Area"))), "picking closes the list");
}

#[test]
fn path_player_task_and_point_fields_offer_their_own_names() {
    let m = mission();
    let s = Schema::engine();
    for (kind, name) in [("path", "Patrol"), ("player", "Raiders"), ("task", "Reach the gate"), ("point", "The Gate")] {
        let found = [true, false]
            .into_iter()
            .find_map(|c| s.types.iter().filter(|t| t.is_condition == c).find_map(|t| t.fields.iter().find(|f| f.kind == kind).map(|f| (c, t.name.clone(), f.name.clone()))));
        let (c, ty, field) = found.unwrap_or_else(|| panic!("no {kind}"));
        let mut rig = Rig::new(json!({"Type": ty}), c, &m);
        rig.settle();
        rig.click(fid(&field).with("btn"));
        rig.click(fid(&field).with(("opt", name)));
        assert_eq!(rig.item[&field], name, "{kind}");
    }
}

#[test]
fn a_flag_is_free_text_with_suggestions_and_new_names_are_allowed() {
    let mut m = mission();
    m["rules"] =
        json!([{"name": "r", "Run Once": true, "Is Active": true, "conditions": [], "actions": [{"Type": "Set Flag Action", "Flag Name": "gate open", "Boolean Value": "TRUE"}]}]);
    let s = Schema::engine();
    let (ty, field) = type_with(&s, true, "flag");
    let mut rig = Rig::new(json!({"Type": ty}), true, &m);
    rig.settle();
    assert!(rig.shown(fid(&field).with("text")), "a text box, not a closed list");
    rig.type_text(fid(&field).with("text"), "gat");
    assert_eq!(rig.item[&field], "gat");
    assert!(rig.shown(fid(&field).with(("sug", "gate open"))), "the defined flag is suggested while typing");
    rig.click(fid(&field).with(("sug", "gate open")));
    assert_eq!(rig.item[&field], "gate open");
    // A name nobody sets is fine: it is the author's new flag.
    rig.click(fid(&field).with("text"));
    rig.frame(vec![egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    rig.frame(vec![egui::Event::Text("brand new".into())]);
    rig.settle();
    assert_eq!(rig.item[&field], "brand new");
}

#[test]
fn timers_and_counters_are_free_text_too() {
    let m = mission();
    let s = Schema::engine();
    for kind in ["timer", "counter"] {
        let c = s.types.iter().any(|t| t.is_condition && t.fields.iter().any(|f| f.kind == kind));
        let (ty, field) = type_with(&s, c, kind);
        let mut rig = Rig::new(json!({"Type": ty}), c, &m);
        rig.settle();
        rig.type_text(fid(&field).with("text"), "t1");
        assert_eq!(rig.item[&field], "t1", "{kind}");
    }
}

#[test]
fn a_group_field_takes_group_and_optional_unit() {
    let m = mission();
    let s = Schema::engine();
    let (ty, field) = type_with(&s, true, "group");
    let mut rig = Rig::new(json!({"Type": ty}), true, &m);
    rig.settle();
    rig.click(sf::field_id(Rig::id(), &field).with("group").with("btn"));
    rig.click(fid(&field).with("group").with(("opt", "Raiders")));
    assert_eq!(rig.item[&field], "Raiders");
    // "+ unit" makes it Group,Unit and the unit dropdown lists the ships' names.
    rig.click(fid(&field).with("add-unit"));
    assert_eq!(rig.item[&field], "Raiders,");
    rig.click(fid(&field).with("unit").with("btn"));
    rig.click(fid(&field).with("unit").with(("opt", "Raider One")));
    assert_eq!(rig.item[&field], "Raiders,Raider One");
    // Changing the group keeps the unit; the x drops it.
    rig.click(fid(&field).with("group").with("btn"));
    rig.click(fid(&field).with("group").with(("opt", "RaiderShips")));
    assert_eq!(rig.item[&field], "RaiderShips,Raider One");
    rig.click(fid(&field).with("no-unit"));
    assert_eq!(rig.item[&field], "RaiderShips");
}

#[test]
fn an_untouched_ranged_number_writes_nothing_and_booleans_default_to_strings() {
    let s = Schema::engine();
    let f = s.types.iter().flat_map(|t| &t.fields).find(|f| f.kind == "number" && f.range.is_some()).expect("a ranged number");
    assert!(f.range.is_some());
    let mut obj = serde_json::Map::new();
    let ctx = egui::Context::default();
    let m = Names::default();
    let (lo, hi) = f.range.unwrap();
    let raw = egui::RawInput { screen_rect: Some(SCREEN), ..Default::default() };
    let mut full = ctx.run_ui(raw, |ui| {
        sf::field_widget(ui, f, &mut obj, &m, egui::Id::new("n"));
    });
    full.textures_delta.clear();
    assert!(obj.is_empty(), "an untouched field writes nothing (the default shows greyed)");
    assert!(lo < hi);
    let b = s.types.iter().flat_map(|t| &t.fields).find(|f| f.kind == "bool").unwrap();
    assert!(sf::default_value(b).is_string());
}

#[test]
fn nested_any_of_edits_its_conditions_with_the_same_widgets() {
    let m = mission();
    let s = Schema::engine();
    let (any_of, list) = type_with(&s, true, "condition_list");
    let (inner, vfield) = type_with(&s, true, "volume");
    let item = json!({"Type": any_of, list.clone(): [{"Type": inner}]});
    let mut rig = Rig::new(item, true, &m);
    rig.settle();
    let child = sf::child_id(Rig::id(), &list, 0);
    rig.click(sf::field_id(child, &vfield).with("btn"));
    rig.click(sf::field_id(child, &vfield).with(("opt", "Gate Area")));
    assert_eq!(rig.item[&list][0][&vfield], "Gate Area");
    assert_eq!(rig.edits.last().unwrap().pointer, format!("/item/{}/0/{}", edit::escape(&list), edit::escape(&vfield)));
    // Remove drops the child (one edit of the list); the add menu has a Type per condition.
    rig.click(child.with("remove"));
    assert_eq!(rig.item[&list], json!([]));
}

#[test]
fn nested_not_holds_one_condition() {
    let m = mission();
    let s = Schema::engine();
    let (not, field) = type_with(&s, true, "condition");
    let (inner, vfield) = type_with(&s, true, "volume");
    let mut rig = Rig::new(json!({"Type": not, field.clone(): {"Type": inner}}), true, &m);
    rig.settle();
    let child = sf::child_id(Rig::id(), &field, 0);
    rig.click(sf::field_id(child, &vfield).with("btn"));
    rig.click(sf::field_id(child, &vfield).with(("opt", "Gate Area")));
    assert_eq!(rig.item[&field][&vfield], "Gate Area");
    rig.click(child.with("remove"));
    assert!(rig.item.get(&field).is_none(), "{}", rig.item);
    assert_eq!(rig.edits.last().unwrap(), &Edit { pointer: format!("/item/{}", edit::escape(&field)), old: Some(json!({"Type": inner, vfield.clone(): "Gate Area"})), new: None });
}

fn findings_for(m: &Value, rule: Value) -> (Value, Vec<Finding>) {
    let mut mm = m.clone();
    mm["rules"] = json!([rule]);
    let f = check_mission(&mm, &Value::Null, &Value::Null, false);
    (mm, f)
}

#[test]
fn a_finding_sits_under_its_field_and_one_click_writes_the_suggested_spelling() {
    let s = Schema::engine();
    let (ty, field) = type_with(&s, true, "volume");
    let (mm, found) = findings_for(&mission(), json!({"name": "r", "Run Once": true, "Is Active": true, "conditions": [{"Type": ty, field.clone(): "Gate Aera"}], "actions": []}));
    let hit = found.iter().find(|p| p.field == field).unwrap_or_else(|| panic!("{found:?}")).clone();
    assert_eq!(hit.suggestion.as_deref(), Some("Gate Area"));
    let mut rig = Rig::new(mm["rules"][0]["conditions"][0].clone(), true, &mm);
    rig.problems = vec![hit];
    rig.settle();
    rig.click(fid(&field).with(("fix", 0)));
    assert_eq!(rig.item[&field], "Gate Area");
    assert_eq!(rig.edits.len(), 1);
    assert_eq!(rig.edits[0], Edit { pointer: format!("/item/{field}"), old: Some(json!("Gate Aera")), new: Some(json!("Gate Area")) });
    // The finding is stale once the field reads right: a click on the old button position changes nothing more.
    rig.settle();
    assert_eq!(rig.edits.len(), 1);
}

#[test]
fn a_finding_inside_a_nested_condition_is_drawn_on_the_nested_field() {
    let s = Schema::engine();
    let (any_of, list) = type_with(&s, true, "condition_list");
    let (inner, vfield) = type_with(&s, true, "volume");
    let rule =
        json!({"name": "r", "Run Once": true, "Is Active": true, "conditions": [{"Type": any_of, list.clone(): [{"Type": inner, vfield.clone(): "Gate Aera"}]}], "actions": []});
    let (mm, found) = findings_for(&mission(), rule);
    let hit = found.iter().find(|p| p.field == vfield).unwrap_or_else(|| panic!("{found:?}")).clone();
    assert_eq!(hit.nested, vec![0]);
    let mut rig = Rig::new(mm["rules"][0]["conditions"][0].clone(), true, &mm);
    rig.problems = vec![hit];
    rig.settle();
    let child = sf::child_id(Rig::id(), &list, 0);
    assert!(rig.shown(sf::field_id(child, &vfield).with(("fix", 0))), "the fix button is under the nested field");
    assert!(!rig.shown(sf::field_id(Rig::id(), &vfield).with(("fix", 0))));
    rig.click(sf::field_id(child, &vfield).with(("fix", 0)));
    assert_eq!(rig.item[&list][0][&vfield], "Gate Area");
}

#[test]
fn a_group_finding_suggestion_goes_to_the_part_that_is_wrong() {
    let n = names_of(&mission());
    assert_eq!(sf::apply_suggestion("group", "RaiderShips,Raider Onee", "Raider One", &n), "RaiderShips,Raider One");
    assert_eq!(sf::apply_suggestion("group", "RaiderShip", "RaiderShips", &n), "RaiderShips");
    assert_eq!(sf::apply_suggestion("group", "RaiderShip,Raider One", "RaiderShips", &n), "RaiderShips,Raider One");
    assert_eq!(sf::apply_suggestion("volume", "Gate Aera", "Gate Area", &n), "Gate Area");
}

#[test]
fn drawing_alone_makes_no_edit() {
    let s = Schema::engine();
    let m = mission();
    let (ty, _) = type_with(&s, true, "volume");
    let mut rig = Rig::new(json!({"Type": ty}), true, &m);
    rig.settle();
    assert!(rig.edits.is_empty(), "drawing alone changes nothing: {:?}", rig.edits);
}
