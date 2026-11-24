//! A generic form for one mission rule, generated from the engine's rule schema (`openprocyon --dump-rule-schema`).
//! The schema comes in-process from `openprocyon::rules::schema::to_json()`; nothing about Types or fields is written down here.
//! Each field gets the widget its `kind` asks for. Values are written the way the interpreter reads them: booleans as the strings
//! "TRUE"/"FALSE", numbers as JSON numbers, everything else as strings.

use bevy_egui::egui;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

use crate::checks::Problem;

pub struct Field {
    pub name: String,
    pub kind: String,
    pub default: String,
    pub required: bool,
    pub values: Vec<String>,
    pub range: Option<(f64, f64)>,
}

pub struct TypeDef {
    pub name: String,
    pub is_condition: bool,
    pub fields: Vec<Field>,
}

pub struct Schema {
    pub types: Vec<TypeDef>,
}

impl Schema {
    /// The schema the engine prints for `--dump-rule-schema`, taken in-process (needs no assets).
    pub fn engine() -> Schema {
        Schema::from_json(&openprocyon::rules::schema::to_json())
    }

    pub fn from_json(v: &Value) -> Schema {
        let strings = |v: &Value| v.as_array().into_iter().flatten().filter_map(|s| s.as_str()).map(String::from).collect::<Vec<_>>();
        let types = v["types"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| TypeDef {
                name: t["name"].as_str().unwrap_or_default().into(),
                is_condition: t["role"] == "condition",
                fields: t["fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| Field {
                        name: f["name"].as_str().unwrap_or_default().into(),
                        kind: f["kind"].as_str().unwrap_or_default().into(),
                        default: f["default"].as_str().unwrap_or_default().into(),
                        required: f["required"].as_bool().unwrap_or(false),
                        values: strings(&f["values"]),
                        range: f["range"].as_array().and_then(|r| Some((r.first()?.as_f64()?, r.get(1)?.as_f64()?))),
                    })
                    .collect(),
            })
            .collect();
        Schema { types }
    }

    pub fn find(&self, condition: bool, name: &str) -> Option<&TypeDef> {
        self.types.iter().find(|t| t.is_condition == condition && t.name == name)
    }
}

/// The names a mission defines, per field kind (`group`, `volume`, ...): what the reference dropdowns list.
#[derive(Default)]
pub struct Names(pub BTreeMap<String, Vec<String>>);

impl Names {
    /// PROVISIONAL: the engine's own collector (`rules::check::Defs`) is private at v0.2.0; only `known_names` (groups) is public.
    /// The rest is read here from the files' documented layout. Flags, timers and counters are the values the rules write, found
    /// through the schema (any field of that kind), so no Type names are listed here.
    pub fn collect(schema: &Schema, mission: &Value, map: &Value, media: &Value) -> Names {
        let keys = |v: &Value| -> BTreeSet<String> { v.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default() };
        let field = |v: &Value, key: &str| -> BTreeSet<String> { v.as_array().into_iter().flatten().filter_map(|x| x[key].as_str()).map(|s| s.trim().to_string()).collect() };
        let mut sets: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        let mut players = BTreeSet::new();
        players.extend(mission["players"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from));
        players.extend(mission["player_start"]["name"].as_str().map(String::from));
        sets.insert("player", players);
        sets.insert("group", openprocyon::rules::check::known_names(mission).into_iter().collect());
        sets.insert("path", keys(&mission["paths"]));
        sets.insert("volume", keys(&mission["volumes"]));
        sets.insert("task", field(&mission["tasks"], "Name"));
        sets.insert("point", field(&mission["points"], "Name"));
        sets.insert("rule", field(&mission["rules"], "name"));
        sets.insert("point_set", keys(&mission["environment"]["point_sets"]));
        sets.insert("map_text", keys(&mission["map_text"]));
        sets.insert("speech_event", keys(&media["speech"]).into_iter().chain(keys(&mission["opening_speech"])).collect());
        sets.insert("message", keys(&media["messages"]));
        sets.insert("ship_name", keys(&media["ship_names"]));
        sets.insert("team", field(&map["teams"], "name_id"));
        // Names written by rules: any field of kind flag / timer / counter, in any condition or action.
        for rule in mission["rules"].as_array().into_iter().flatten() {
            for (list, is_cond) in [("conditions", true), ("actions", false)] {
                for item in rule[list].as_array().into_iter().flatten() {
                    let Some(def) = item["Type"].as_str().and_then(|t| schema.find(is_cond, t)) else { continue };
                    for f in def.fields.iter().filter(|f| matches!(f.kind.as_str(), "flag" | "timer" | "counter")) {
                        if let Some(name) = item[f.name.as_str()].as_str().filter(|s| !s.is_empty()) {
                            sets.entry(if f.kind == "flag" {
                                "flag"
                            } else if f.kind == "timer" {
                                "timer"
                            } else {
                                "counter"
                            })
                            .or_default()
                            .insert(name.to_string());
                        }
                    }
                }
            }
        }
        Names(sets.into_iter().map(|(k, v)| (k.to_string(), v.into_iter().filter(|s| !s.is_empty()).collect())).collect())
    }

    pub fn of(&self, kind: &str) -> &[String] {
        self.0.get(kind).map_or(&[], Vec::as_slice)
    }
}

/// "TRUE" / "FALSE" for a checkbox. The interpreter reads a JSON `true` as FALSE, so this never writes one.
pub fn bool_value(on: bool) -> Value {
    Value::String(if on { "TRUE" } else { "FALSE" }.into())
}

pub fn is_true(v: &Value) -> bool {
    v.as_str() == Some("TRUE")
}

fn number_value(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 { json!(x as i64) } else { json!(x) }
}

/// A fresh object for a Type: its name and the default of every required field.
pub fn new_item(def: &TypeDef) -> Value {
    let mut m = Map::new();
    m.insert("Type".into(), def.name.clone().into());
    for f in def.fields.iter().filter(|f| f.required) {
        m.insert(f.name.clone(), default_value(f));
    }
    Value::Object(m)
}

pub fn default_value(f: &Field) -> Value {
    match f.kind.as_str() {
        "bool" => bool_value(f.default == "TRUE"),
        "number" => f.default.parse::<f64>().map_or(json!(0), number_value),
        "enum" if f.default.is_empty() => f.values.first().cloned().unwrap_or_default().into(),
        _ => f.default.clone().into(),
    }
}

fn is_reference(kind: &str) -> bool {
    !matches!(kind, "bool" | "number" | "enum" | "text" | "condition" | "condition_list")
}

/// One widget for `f`, reading and writing `obj[f.name]`. Returns whether the value changed. A missing key shows the default and is
/// only written when the author edits it.
pub fn field_widget(ui: &mut egui::Ui, f: &Field, obj: &mut Map<String, Value>, names: &Names, id: egui::Id) -> bool {
    let id = id.with(&f.name);
    let cur = obj.get(&f.name).cloned().unwrap_or_else(|| default_value(f));
    let mut out: Option<Value> = None;
    match f.kind.as_str() {
        "bool" => {
            let mut on = is_true(&cur);
            if ui.checkbox(&mut on, "").changed() {
                out = Some(bool_value(on));
            }
        }
        "number" => {
            let mut x = cur.as_f64().or_else(|| cur.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0);
            let mut drag = egui::DragValue::new(&mut x).speed(0.1);
            if let Some((lo, hi)) = f.range {
                drag = drag.range(lo..=hi);
            }
            if ui.add(drag).changed() {
                out = Some(number_value(x));
            }
        }
        "enum" => {
            let text = cur.as_str().unwrap_or_default().to_string();
            egui::ComboBox::from_id_salt(id).selected_text(text.clone()).show_ui(ui, |ui| {
                for v in &f.values {
                    if ui.selectable_label(&text == v, v).clicked() && &text != v {
                        out = Some(v.clone().into());
                    }
                }
            });
        }
        "text" => {
            let mut text = cur.as_str().unwrap_or_default().to_string();
            if ui.text_edit_singleline(&mut text).changed() {
                out = Some(text.into());
            }
        }
        k if is_reference(k) => {
            let text = cur.as_str().unwrap_or_default().to_string();
            let options = names.of(k);
            egui::ComboBox::from_id_salt(id).selected_text(text.clone()).width(180.0).show_ui(ui, |ui| {
                for v in options {
                    if ui.selectable_label(&text == v, v).clicked() && &text != v {
                        out = Some(v.clone().into());
                    }
                }
                if options.is_empty() {
                    ui.weak(format!("the mission defines no {k}"));
                }
            });
        }
        _ => {
            ui.weak("nested conditions: edit the file (not in the spike)");
        }
    }
    match out {
        Some(v) => {
            obj.insert(f.name.clone(), v);
            true
        }
        None => false,
    }
}

/// Type picker plus one row per schema field for one condition or action object. `problems` are the checker's findings for this
/// item (matched by field name); a field a problem names is drawn in red with the message beneath.
pub fn item_form(ui: &mut egui::Ui, schema: &Schema, names: &Names, item: &mut Value, is_condition: bool, problems: &[&Problem], id: egui::Id) -> bool {
    let mut changed = false;
    let Some(obj) = item.as_object_mut() else {
        ui.colored_label(egui::Color32::LIGHT_RED, "not an object");
        return false;
    };
    let current = obj.get("Type").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut picked: Option<&TypeDef> = None;
    ui.horizontal(|ui| {
        ui.label("Type");
        egui::ComboBox::from_id_salt(id.with("type")).selected_text(current.clone()).width(260.0).show_ui(ui, |ui| {
            for t in schema.types.iter().filter(|t| t.is_condition == is_condition) {
                if ui.selectable_label(t.name == current, &t.name).clicked() && t.name != current {
                    picked = Some(t);
                }
            }
        });
    });
    if let Some(def) = picked {
        *item = new_item(def);
        return true;
    }
    let Some(def) = schema.find(is_condition, &current) else {
        ui.colored_label(egui::Color32::LIGHT_RED, format!("'{current}' is not a Type of the rule schema"));
        return false;
    };
    for f in &def.fields {
        let hits: Vec<&&Problem> = problems.iter().filter(|p| p.field == f.name).collect();
        ui.horizontal(|ui| {
            let label = egui::RichText::new(&f.name);
            ui.label(if hits.is_empty() { label } else { label.color(egui::Color32::LIGHT_RED).strong() });
            changed |= field_widget(ui, f, obj, names, id);
        });
        for p in hits {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("    {}", p.message));
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Schema {
        Schema::engine()
    }

    #[test]
    fn the_engine_schema_is_available_in_process() {
        let s = schema();
        assert!(s.types.len() > 50, "{} types", s.types.len());
        let t = s.find(false, "Set Group/Unit Hold Position").expect("a known action");
        assert!(t.fields.iter().any(|f| f.kind == "bool"));
        assert!(s.types.iter().flat_map(|t| &t.fields).any(|f| f.kind == "enum" && !f.values.is_empty()));
        assert!(s.types.iter().flat_map(|t| &t.fields).any(|f| f.range.is_some()));
    }

    #[test]
    fn booleans_are_strings_never_json_booleans() {
        assert_eq!(bool_value(true), json!("TRUE"));
        assert_eq!(bool_value(false), json!("FALSE"));
        assert!(is_true(&json!("TRUE")) && !is_true(&json!(true)));
        let s = schema();
        let def = s.find(false, "Set Group/Unit Hold Position").unwrap();
        for f in def.fields.iter().filter(|f| f.kind == "bool") {
            assert!(default_value(f).is_string(), "{}", f.name);
        }
    }

    #[test]
    fn a_new_item_has_its_type_and_required_defaults() {
        let s = schema();
        let def = s.find(false, "Start Timer").unwrap();
        let v = new_item(def);
        assert_eq!(v["Type"], "Start Timer");
        for f in def.fields.iter().filter(|f| f.required) {
            assert!(v.get(&f.name).is_some(), "{}", f.name);
        }
    }

    #[test]
    fn names_come_from_the_mission() {
        let fx = crate::mission::Mission::load(&crate::mission::fixture_dir()).unwrap();
        let s = schema();
        let n = Names::collect(&s, &fx.mission, &fx.map, &fx.media);
        assert_eq!(n.of("volume"), ["Gate Area"]);
        assert_eq!(n.of("point"), ["The Gate"]);
        assert_eq!(n.of("task"), ["Clear the gate", "Reach the gate"]);
        assert!(n.of("group").contains(&"RaiderShips".to_string()) && n.of("group").contains(&"Hero".to_string()));
        assert_eq!(n.of("counter"), ["raiders sunk"]);
        assert_eq!(n.of("rule"), ["Arrive at the gate", "Count raiders sunk", "Start"]);
    }
}
