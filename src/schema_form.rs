//! A generic form for one mission rule, generated from the engine's rule schema (`openprocyon --dump-rule-schema`).
//! The schema comes in-process from `openprocyon::authoring::rule_schema()`; nothing about Types or fields is written down here.
//! Each field gets the widget its `kind` asks for. Values are written the way the interpreter reads them: booleans as the strings
//! "TRUE"/"FALSE", numbers as JSON numbers, everything else as strings.

use bevy_egui::egui;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

use crate::checks::Finding;
use crate::edit::{self, Edit};

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
        Schema::from_json(&openprocyon::authoring::rule_schema())
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

/// What a field of each kind accepts, from the engine (`authoring::accepted_names`): the lists the dropdowns offer.
#[derive(Default)]
pub struct Names(pub BTreeMap<String, Vec<String>>);

impl Names {
    /// Per field kind (`group`, `volume`, `flag`, ... and `unit`, the part after the comma of a group spec), exactly the names a field
    /// of that kind takes without `--check-mission` reporting them: a `group` field offers players and placed ships' groups too. For
    /// `flag`, `timer` and `counter` it lists what the mission sets or starts, and any other name is accepted as a new one.
    pub fn collect(mission: &Value, map: &Value, media: &Value) -> Names {
        Names(openprocyon::authoring::accepted_names(mission, map, media))
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
        "condition_list" => json!([]),
        "condition" => Value::Null,
        _ => f.default.clone().into(),
    }
}

/// The widget a field kind asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Widget {
    Bool,
    Number,
    Enum,
    /// Plain free text (file names, effect names).
    Text,
    /// A dropdown with type-ahead of the names the mission defines; only those are offered.
    Pick,
    /// Free text with suggestions: the name needs no declaration (a flag is made by setting it), a name not listed is marked "new".
    /// PROVISIONAL: `team`, `speech`, `message` and `ship_name` are here too (the checker accepts any id there when no `media.json`
    /// is given); the roadmap names only flag, timer and counter.
    Free,
    /// `Group` or `Group,Unit`: a group dropdown and an optional unit dropdown.
    Spec,
    /// `OP Any Of`: a nested list of conditions.
    ConditionList,
    /// `OP Not`: one nested condition.
    Condition,
}

pub fn widget_for(kind: &str) -> Widget {
    match kind {
        "bool" => Widget::Bool,
        "number" => Widget::Number,
        "enum" => Widget::Enum,
        "text" => Widget::Text,
        "group" => Widget::Spec,
        "condition_list" => Widget::ConditionList,
        "condition" => Widget::Condition,
        "flag" | "timer" | "counter" | "campaign_counter" | "team" | "speech" | "message" | "ship_name" => Widget::Free,
        _ => Widget::Pick,
    }
}

/// `"Group,Unit"` as (group, unit); a spec without a comma has no unit. Both parts trimmed.
pub fn split_spec(text: &str) -> (&str, Option<&str>) {
    match text.split_once(',') {
        Some((g, u)) => (g.trim(), Some(u.trim())),
        None => (text.trim(), None),
    }
}

pub fn join_spec(group: &str, unit: Option<&str>) -> String {
    match unit {
        Some(u) => format!("{group},{u}"),
        None => group.to_string(),
    }
}

/// The names that contain `needle` (case-insensitive), prefix matches first, at most `limit`. An empty needle matches all.
pub fn matches<'a>(options: &'a [String], needle: &str, limit: usize) -> Vec<&'a String> {
    let n = needle.trim().to_lowercase();
    let (mut first, mut rest) = (vec![], vec![]);
    for o in options {
        let l = o.to_lowercase();
        if l.starts_with(&n) {
            first.push(o);
        } else if l.contains(&n) {
            rest.push(o);
        }
    }
    first.into_iter().chain(rest).take(limit).collect()
}

/// The text to write when the author clicks a finding's suggested spelling. A group spec takes it in the part that is wrong: the unit
/// part when the group part is accepted as it stands (PROVISIONAL: the finding does not say which part it is about).
pub fn apply_suggestion(kind: &str, current: &str, suggestion: &str, names: &Names) -> String {
    if widget_for(kind) != Widget::Spec {
        return suggestion.to_string();
    }
    let (g, u) = split_spec(current);
    match u {
        Some(_) if names.of("group").iter().any(|n| n == g) => join_spec(g, Some(suggestion)),
        Some(u) => join_spec(suggestion, Some(u)),
        None => suggestion.to_string(),
    }
}

const RED: egui::Color32 = egui::Color32::LIGHT_RED;
const MAX_OPTIONS: usize = 200;

/// Id of the widget of field `field` of the item whose id is `item_id`: what a test (or a caller that scrolls to a field) reads
/// with `ctx.read_response`. The pick button is `.with("btn")`, its filter box `.with("filter")`, an option `.with(("opt", name))`,
/// the free-text box `.with("text")`, a suggestion `.with(("sug", name))`, a finding's fix `.with(("fix", n))`; a group spec's two
/// dropdowns are `.with("group")` and `.with("unit")`, its `+ unit` / `x` buttons `.with("add-unit")` / `.with("no-unit")`.
pub fn field_id(item_id: egui::Id, field: &str) -> egui::Id {
    item_id.with(field)
}

/// Id of the `index`-th nested condition under `field` (`OP Any Of`: its position; `OP Not`: 0). Its own fields are
/// `field_id(child_id(..), name)`; its Remove button is `.with("remove")`; the add menu of the parent is `.with(("add", field))`.
pub fn child_id(item_id: egui::Id, field: &str, index: usize) -> egui::Id {
    item_id.with(("nested", field, index))
}

/// A small clickable label with a fixed id (so it can be found again).
fn chip(ui: &mut egui::Ui, id: egui::Id, text: &str, strong: bool, color: Option<egui::Color32>) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let col = color.unwrap_or(ui.visuals().text_color());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, col);
    let pad = egui::vec2(6.0, 2.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::hover());
    let resp = ui.interact(rect, id, egui::Sense::click());
    let fill = if strong { ui.visuals().selection.bg_fill } else { ui.style().interact(&resp).weak_bg_fill };
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().galley(rect.min + pad, galley, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Dropdown with type-ahead over `options`: a button with the current value; opened, a filter box and the matching names below it.
/// Enter takes the only match. Returns the name picked this frame.
fn pick(ui: &mut egui::Ui, id: egui::Id, current: &str, options: &[String], kind: &str) -> Option<String> {
    let open_key = id.with("open");
    let open = ui.data(|d| d.get_temp::<bool>(open_key)).unwrap_or(false);
    let label = if current.is_empty() { "(choose)".to_string() } else { current.to_string() };
    let weak = current.is_empty().then(|| ui.visuals().weak_text_color());
    let btn = chip(ui, id.with("btn"), &format!("{label}  v"), open, weak);
    if btn.clicked() {
        ui.data_mut(|d| {
            d.insert_temp(open_key, !open);
            d.insert_temp(id.with("fresh"), true);
        });
    }
    if !open {
        return None;
    }
    let mut out = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let fid = id.with("filter");
        let mut needle = ui.data(|d| d.get_temp::<String>(fid)).unwrap_or_default();
        let r = ui.add(egui::TextEdit::singleline(&mut needle).id(fid).hint_text("type to filter").desired_width(160.0));
        if ui.data_mut(|d| d.remove_temp::<bool>(id.with("fresh"))).is_some() {
            r.request_focus();
        }
        let found = matches(options, &needle, MAX_OPTIONS);
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && found.len() == 1 {
            out = Some(found[0].clone());
        }
        if options.is_empty() {
            ui.weak(format!("the mission defines no {kind}"));
        } else if found.is_empty() {
            ui.weak("no match");
        }
        egui::ScrollArea::vertical().id_salt(id.with("scroll")).max_height(160.0).show(ui, |ui| {
            for name in found {
                if chip(ui, id.with(("opt", name.as_str())), name, name == current, None).clicked() {
                    out = Some(name.clone());
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(fid, needle));
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.data_mut(|d| d.insert_temp(open_key, false));
        }
    });
    if out.is_some() {
        ui.data_mut(|d| {
            d.insert_temp(open_key, false);
            d.insert_temp(id.with("filter"), String::new());
        });
    }
    out
}

/// Free text with suggestions from `options`; a name that is not listed is marked "new". Returns the text when it changed (typed or
/// a suggestion taken).
fn free(ui: &mut egui::Ui, id: egui::Id, current: &str, options: &[String]) -> Option<String> {
    let mut text = current.to_string();
    let r = ui.add(egui::TextEdit::singleline(&mut text).id(id.with("text")).desired_width(180.0));
    let mut out = r.changed().then(|| text.clone());
    // The suggestions stay for the click that takes one: the box loses focus when the pointer goes down on a chip.
    let shown_key = id.with("sug-shown");
    let was = ui.data(|d| d.get_temp::<bool>(shown_key)).unwrap_or(false);
    let show = r.has_focus() || (was && ui.input(|i| i.pointer.any_down() || i.pointer.any_released()));
    ui.data_mut(|d| d.insert_temp(shown_key, show));
    if !text.is_empty() && !options.iter().any(|o| o == &text) {
        ui.weak("new");
    }
    if show {
        let found: Vec<&String> = matches(options, &text, 8).into_iter().filter(|o| *o != &text).collect();
        if !found.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for name in found {
                    if chip(ui, id.with(("sug", name.as_str())), name, false, None).clicked() {
                        out = Some(name.clone());
                    }
                }
            });
        }
    }
    out
}

/// The value widget of one field that holds a plain value: returns the new value when the author changed it.
fn scalar(ui: &mut egui::Ui, f: &Field, cur: &Value, names: &Names, id: egui::Id) -> Option<Value> {
    let text = cur.as_str().unwrap_or_default().to_string();
    match widget_for(&f.kind) {
        Widget::Bool => {
            let mut on = is_true(cur);
            ui.checkbox(&mut on, "").changed().then(|| bool_value(on))
        }
        Widget::Number => {
            let mut x = cur.as_f64().or_else(|| cur.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0);
            let mut drag = egui::DragValue::new(&mut x).speed(0.1);
            if let Some((lo, hi)) = f.range {
                drag = drag.range(lo..=hi);
            }
            let changed = ui.add(drag).changed();
            if let Some((lo, hi)) = f.range {
                ui.weak(format!("{lo}..{hi}"));
            }
            changed.then(|| number_value(x))
        }
        Widget::Enum => {
            let mut out = None;
            egui::ComboBox::from_id_salt(id).selected_text(text.clone()).show_ui(ui, |ui| {
                for v in &f.values {
                    if ui.selectable_label(&text == v, v).clicked() && &text != v {
                        out = Some(Value::from(v.clone()));
                    }
                }
            });
            out
        }
        Widget::Text => {
            let mut t = text;
            ui.text_edit_singleline(&mut t).changed().then(|| Value::from(t))
        }
        Widget::Pick => pick(ui, id, &text, names.of(&f.kind), &f.kind).map(Value::from),
        Widget::Free => free(ui, id, &text, names.of(&f.kind)).map(Value::from),
        Widget::Spec => {
            let (g, u) = split_spec(&text);
            let (g, u) = (g.to_string(), u.map(String::from));
            let mut out = None;
            ui.vertical(|ui| {
                if let Some(ng) = pick(ui, id.with("group"), &g, names.of("group"), "group") {
                    out = Some(join_spec(&ng, u.as_deref()));
                }
                match &u {
                    Some(unit) => {
                        ui.horizontal(|ui| {
                            ui.weak("unit");
                            if chip(ui, id.with("no-unit"), "x", false, None).on_hover_text("whole group").clicked() {
                                out = Some(join_spec(&g, None));
                            }
                        });
                        if let Some(nu) = pick(ui, id.with("unit"), unit, names.of("unit"), "unit") {
                            out = Some(join_spec(&g, Some(&nu)));
                        }
                    }
                    None => {
                        if chip(ui, id.with("add-unit"), "+ unit", false, None).on_hover_text("name one unit of the group").clicked() {
                            out = Some(join_spec(&g, Some("")));
                        }
                    }
                }
            });
            out.map(Value::from)
        }
        Widget::ConditionList | Widget::Condition => None,
    }
}

/// One widget for `f`, reading and writing `obj[f.name]` (no findings, no nested lists: those need [`item_form`]). Returns whether
/// the value changed. A missing key shows the default and is only written when the author edits it.
pub fn field_widget(ui: &mut egui::Ui, f: &Field, obj: &mut Map<String, Value>, names: &Names, id: egui::Id) -> bool {
    if matches!(widget_for(&f.kind), Widget::ConditionList | Widget::Condition) {
        ui.weak("nested conditions: use item_form");
        return false;
    }
    let cur = obj.get(&f.name).cloned().unwrap_or_else(|| default_value(f));
    match scalar(ui, f, &cur, names, field_id(id, &f.name)) {
        Some(v) => {
            obj.insert(f.name.clone(), v);
            true
        }
        None => false,
    }
}

struct Cx<'a> {
    schema: &'a Schema,
    names: &'a Names,
    problems: &'a [&'a Finding],
}

fn child_ptr(base: &str, key: &str) -> String {
    format!("{base}/{}", edit::escape(key))
}

/// Type picker plus one row per schema field for one condition or action object; nested conditions recurse. Edits go to `out`
/// with pointers below `base`; `item` is changed in step.
#[allow(clippy::too_many_arguments)]
fn item_core(ui: &mut egui::Ui, cx: &Cx, item: &mut Value, is_condition: bool, base: &str, path: &[usize], id: egui::Id, out: &mut Vec<Edit>) {
    let Some(obj) = item.as_object_mut() else {
        ui.colored_label(RED, "not an object");
        return;
    };
    let current = obj.get("Type").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut picked: Option<&TypeDef> = None;
    ui.horizontal(|ui| {
        ui.label("Type");
        egui::ComboBox::from_id_salt(id.with("type")).selected_text(current.clone()).width(260.0).show_ui(ui, |ui| {
            for t in cx.schema.types.iter().filter(|t| t.is_condition == is_condition) {
                if ui.selectable_label(t.name == current, &t.name).clicked() && t.name != current {
                    picked = Some(t);
                }
            }
        });
    });
    if let Some(def) = picked {
        let old = item.clone();
        *item = new_item(def);
        out.push(Edit { pointer: base.to_string(), old: Some(old), new: Some(item.clone()) });
        return;
    }
    let Some(def) = cx.schema.find(is_condition, &current) else {
        ui.colored_label(RED, format!("'{current}' is not a Type of the rule schema"));
        return;
    };
    for f in &def.fields {
        let hits: Vec<&Finding> = cx.problems.iter().copied().filter(|p| p.field == f.name && p.nested.as_slice() == path).collect();
        let nested_kind = matches!(widget_for(&f.kind), Widget::ConditionList | Widget::Condition);
        let mut change: Option<Value> = None;
        ui.horizontal_top(|ui| {
            let label = egui::RichText::new(&f.name);
            ui.label(if hits.is_empty() { label } else { label.color(RED).strong() });
            ui.vertical(|ui| {
                let fid = field_id(id, &f.name);
                let cur = obj.get(&f.name).cloned();
                if nested_kind {
                    nested(ui, cx, f, obj, base, path, id, out);
                } else {
                    if let Some(v) = scalar(ui, f, cur.as_ref().unwrap_or(&default_value(f)), cx.names, fid) {
                        change = Some(v);
                    }
                    if cur.is_none() && change.is_none() && !f.default.is_empty() {
                        ui.weak(format!("default: {}", f.default));
                    }
                }
                let now = change.as_ref().or(cur.as_ref()).and_then(Value::as_str).unwrap_or_default().to_string();
                for (n, p) in hits.iter().enumerate() {
                    ui.colored_label(RED, &p.message);
                    if let Some(s) = p.suggestion.as_deref() {
                        let fixed = apply_suggestion(&f.kind, &now, s, cx.names);
                        if fixed != now && chip(ui, fid.with(("fix", n)), &format!("use '{s}'"), true, None).clicked() {
                            change = Some(fixed.into());
                        }
                    }
                }
            });
        });
        if let Some(v) = change {
            out.push(Edit { pointer: child_ptr(base, &f.name), old: obj.get(&f.name).cloned(), new: Some(v.clone()) });
            obj.insert(f.name.clone(), v);
        }
    }
}

/// The nested conditions of an `OP Any Of` (`condition_list`) or `OP Not` (`condition`) field: each one a full item form, with Remove,
/// and an "Add condition" Type menu.
#[allow(clippy::too_many_arguments)]
fn nested(ui: &mut egui::Ui, cx: &Cx, f: &Field, obj: &mut Map<String, Value>, base: &str, path: &[usize], id: egui::Id, out: &mut Vec<Edit>) {
    let ptr = child_ptr(base, &f.name);
    let list = f.kind == "condition_list";
    let count = if list { obj.get(&f.name).and_then(Value::as_array).map_or(0, Vec::len) } else { usize::from(obj.get(&f.name).is_some_and(Value::is_object)) };
    let mut remove = None;
    for i in 0..count {
        let item = if list { &mut obj.get_mut(&f.name).unwrap()[i] } else { obj.get_mut(&f.name).unwrap() };
        let cptr = if list { format!("{ptr}/{i}") } else { ptr.clone() };
        let cpath: Vec<usize> = path.iter().copied().chain([i]).collect();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("{}.", i + 1));
                if chip(ui, child_id(id, &f.name, i).with("remove"), "Remove", false, None).clicked() {
                    remove = Some(i);
                }
            });
            item_core(ui, cx, item, true, &cptr, &cpath, child_id(id, &f.name, i), out);
        });
    }
    if let Some(i) = remove {
        let old = obj.get(&f.name).cloned();
        if list {
            if let Some(a) = obj.get_mut(&f.name).and_then(Value::as_array_mut) {
                a.remove(i);
            }
            out.push(Edit { pointer: ptr, old, new: obj.get(&f.name).cloned() });
        } else {
            obj.remove(&f.name);
            out.push(Edit { pointer: ptr, old, new: None });
        }
        return;
    }
    if list || count == 0 {
        let mut add: Option<&TypeDef> = None;
        egui::ComboBox::from_id_salt(id.with(("add", f.name.as_str()))).selected_text("Add condition").show_ui(ui, |ui| {
            for t in cx.schema.types.iter().filter(|t| t.is_condition) {
                if ui.selectable_label(false, &t.name).clicked() {
                    add = Some(t);
                }
            }
        });
        if let Some(def) = add {
            let old = obj.get(&f.name).cloned();
            let new_c = new_item(def);
            let new = if list {
                let mut a = old.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
                a.push(new_c);
                Value::Array(a)
            } else {
                new_c
            };
            obj.insert(f.name.clone(), new.clone());
            out.push(Edit { pointer: ptr, old, new: Some(new) });
        }
    }
}

/// The edits one frame of the form for one condition or action makes, as pointers below `base` (the item's own pointer in its
/// document, for example `/rules/2/conditions/0`): a changed field is one edit of that field, a new Type one edit of the whole item,
/// a nested list one edit of the list. `item` itself is not changed. `problems` are the checker's findings for this item (nested
/// ones included: they are matched to the nested condition by `nested`); a field a finding names is drawn in red with the message
/// and, when the checker has a suggested spelling, a button that writes it.
#[allow(clippy::too_many_arguments)]
pub fn item_form_edits(ui: &mut egui::Ui, schema: &Schema, names: &Names, item: &Value, is_condition: bool, problems: &[&Finding], base: &str, id: egui::Id) -> Vec<Edit> {
    let mut work = item.clone();
    let mut out = vec![];
    item_core(ui, &Cx { schema, names, problems }, &mut work, is_condition, base, &[], id, &mut out);
    out
}

/// Like [`item_form_edits`], but changes `item` in place and says whether anything changed.
pub fn item_form(ui: &mut egui::Ui, schema: &Schema, names: &Names, item: &mut Value, is_condition: bool, problems: &[&Finding], id: egui::Id) -> bool {
    let mut out = vec![];
    item_core(ui, &Cx { schema, names, problems }, item, is_condition, "", &[], id, &mut out);
    !out.is_empty()
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
        let (mission, map, media) = crate::checks::tests::gates_fixture();
        let n = Names::collect(&mission, &map, &media);
        assert_eq!(n.of("volume"), ["Gate Area"]);
        assert_eq!(n.of("point"), ["The Gate"]);
        assert_eq!(n.of("task"), ["Clear the gate", "Reach the gate"]);
        assert!(n.of("group").contains(&"RaiderShips".to_string()) && n.of("group").contains(&"Hero".to_string()));
        assert_eq!(n.of("counter"), ["raiders sunk"]);
        assert_eq!(n.of("rule"), ["Arrive at the gate", "Count raiders sunk", "Start"]);
    }
}
