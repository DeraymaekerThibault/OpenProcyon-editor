//! The egui drawing of the generic schema form. [`show`] draws one document against its schema and RETURNS the edits the user made
//! this frame (it never mutates the document), so the caller's undo stack owns every change. All decisions (what an action turns
//! into) are in [`super::model`]; this file only lays widgets out.

use bevy_egui::egui::{self, Align, Color32, RichText, Stroke};
use openprocyon::authoring::Level;
use serde_json::{Value, json};

use super::model::{self, Kind, Node};
use crate::edit::{self, Edit};

/// A finding to draw under the field it names (`pointer` is a JSON pointer; see [`edit::dotted_to_pointer`] for the engine's dotted paths).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldNote {
    pub pointer: String,
    pub level: Level,
    pub message: String,
}

/// What the form needs besides the document and its schema.
pub struct FormCtx<'a> {
    /// The parent document (what a key means when the mod does not set it, e.g. the retail ship under an `extends`).
    pub base: Option<&'a Value>,
    /// Findings drawn under their field (red for errors, yellow for warnings); those that name no drawn field are listed at the end.
    pub findings: &'a [FieldNote],
    /// A JSON pointer to bring into view: the sections on its path open once, the field scrolls into view and is outlined while this stays set.
    pub focus: Option<&'a str>,
}

const ERROR: Color32 = Color32::from_rgb(230, 90, 90);
const WARNING: Color32 = Color32::from_rgb(225, 185, 60);

/// Where a row sits, which decides its extra buttons.
#[derive(Clone, Copy)]
enum Slot<'a> {
    /// A named property of an object: Set / Reset against the parent document.
    Field,
    /// An entry of a map (parent pointer, key): rename and remove.
    Entry(&'a str, &'a str),
    /// An element of a list (parent pointer, index): move and remove.
    Item(&'a str, usize),
}

struct St<'a> {
    ctx: &'a FormCtx<'a>,
    doc: &'a Value,
    /// Unknown keys as `(pointer, keys the schema lists there)`.
    unknown: Vec<(String, Vec<String>)>,
    out: Vec<Edit>,
    drawn: Vec<bool>,
    /// The focus changed this frame: open its sections and scroll to it once.
    fresh: bool,
    id: egui::Id,
}

/// Draws `doc` as a form for `schema` and returns the edits made this frame (JSON pointers relative to `doc`), empty when nothing changed.
/// `id` must be unique per open form (it keys the open sections and the text boxes of the form).
pub fn show(ui: &mut egui::Ui, schema: &Value, doc: &Value, ctx: &FormCtx, id: egui::Id) -> Vec<Edit> {
    let root = Node::from_schema(schema);
    let unknown = openprocyon::authoring::unknown_keys(schema, doc).into_iter().map(|(p, keys)| (edit::dotted_to_pointer(&p), keys)).collect();
    let focus_key = id.with("focus");
    let last: Option<String> = ui.data(|d| d.get_temp(focus_key));
    let fresh = ctx.focus.is_some() && last.as_deref() != ctx.focus;
    ui.data_mut(|d| match ctx.focus {
        Some(f) => {
            d.insert_temp(focus_key, f.to_string());
        }
        None => {
            d.remove_temp::<String>(focus_key);
        }
    });
    let mut st = St { ctx, doc, unknown, out: vec![], drawn: vec![false; ctx.findings.len()], fresh, id };
    ui.push_id(id, |ui| {
        if root.is_compound() {
            st.notes(ui, "");
            st.body(ui, &root, "");
        } else {
            st.field(ui, "", &root, "");
        }
        let rest: Vec<usize> = (0..ctx.findings.len()).filter(|i| !st.drawn[*i]).collect();
        if !rest.is_empty() {
            ui.separator();
            ui.label(RichText::new("Other findings").strong());
            for i in rest {
                let f = &ctx.findings[i];
                ui.colored_label(level_color(f.level), format!("{}: {}", edit::pointer_to_dotted(&f.pointer), f.message));
            }
        }
    });
    st.out
}

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Error => ERROR,
        Level::Warning => WARNING,
    }
}

/// A value in a few characters, for greyed text and the raw value of an unknown key.
fn brief(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    if s.chars().count() > 48 { format!("{}...", s.chars().take(45).collect::<String>()) } else { s }
}

/// The part of an element that names it in a list's headings (`name`, `label`, ...: a generic guess at a readable name).
fn item_name(item: &Value) -> Option<&str> {
    ["name", "label", "id", "gun", "layer", "fx"].iter().find_map(|k| item.get(*k)?.as_str())
}

impl<'a> St<'a> {
    fn focus_on_path(&self, ptr: &str) -> bool {
        self.ctx.focus.is_some_and(|f| ptr.is_empty() || f == ptr || f.starts_with(&format!("{ptr}/")))
    }

    fn is_focus(&self, ptr: &str) -> bool {
        self.ctx.focus == Some(ptr)
    }

    /// Outline the focused field and scroll it into view the first frame.
    fn mark(&self, ui: &egui::Ui, rect: egui::Rect, ptr: &str) {
        if self.is_focus(ptr) {
            ui.painter().rect_stroke(rect.expand(2.0), 3.0, Stroke::new(1.5, WARNING), egui::StrokeKind::Outside);
            if self.fresh {
                ui.scroll_to_rect(rect, Some(Align::Center));
            }
        }
    }

    /// Findings that name exactly `ptr`, under the field.
    fn notes(&mut self, ui: &mut egui::Ui, ptr: &str) {
        let ctx = self.ctx;
        for (i, f) in ctx.findings.iter().enumerate() {
            if f.pointer == ptr {
                self.drawn[i] = true;
                ui.colored_label(level_color(f.level), &f.message);
            }
        }
    }

    fn label(&self, ui: &mut egui::Ui, text: &str, node: &Node) {
        let r = ui.label(text);
        if !node.description.is_empty() {
            r.on_hover_text(&node.description);
        }
    }

    /// One property: a collapsible section for a value with parts, else one row.
    fn field(&mut self, ui: &mut egui::Ui, label: &str, node: &Node, ptr: &str) {
        if node.is_compound() { self.section(ui, label, node, ptr, Slot::Field) } else { self.row(ui, label, node, ptr, Slot::Field) }
    }

    /// The contents of a compound value (no heading).
    fn body(&mut self, ui: &mut egui::Ui, node: &Node, ptr: &str) {
        let doc = self.doc;
        let value = doc.pointer(ptr);
        match &node.kind {
            Kind::Object { props } => {
                for (name, child) in props {
                    self.field(ui, name, child, &model::child_pointer(ptr, name));
                }
                for (up, known) in self.unknown.clone() {
                    if let Some((parent, key)) = model::split_pointer(&up)
                        && parent == ptr
                    {
                        self.unknown_row(ui, &parent, &key, &known);
                    }
                }
            }
            Kind::Map { value: vnode, hidden } => {
                if let Some(map) = value.and_then(Value::as_object) {
                    for (k, _) in map.iter().filter(|(k, _)| !hidden.contains(k)) {
                        let p = model::child_pointer(ptr, k);
                        let slot = Slot::Entry(ptr, k);
                        if vnode.is_compound() { self.section(ui, k, vnode, &p, slot) } else { self.row(ui, k, vnode, &p, slot) }
                    }
                }
                self.add_entry(ui, vnode, ptr, value);
            }
            Kind::Array { items, prefix, max, .. } => {
                let arr = value.and_then(Value::as_array);
                for (i, item) in arr.into_iter().flatten().enumerate() {
                    let inode = prefix.get(i).or(items.as_deref());
                    let Some(inode) = inode else { continue };
                    let p = model::child_pointer(ptr, &i.to_string());
                    let slot = Slot::Item(ptr, i);
                    let title = match item_name(item) {
                        Some(n) => format!("[{i}] {n}"),
                        None => format!("[{i}]"),
                    };
                    if inode.is_compound() { self.section(ui, &title, inode, &p, slot) } else { self.row(ui, &format!("[{i}]"), inode, &p, slot) }
                }
                let len = arr.map_or(0, Vec::len);
                let item_node = prefix.get(len).or(items.as_deref());
                if ui.add_enabled(max.is_none_or(|m| len < m), egui::Button::new("Add item")).clicked() {
                    let item = item_node.map_or(Value::Null, model::default_value);
                    self.out.extend(model::array_add(ptr, value, item));
                }
            }
            _ => {}
        }
    }

    fn add_entry(&mut self, ui: &mut egui::Ui, vnode: &Node, ptr: &str, value: Option<&Value>) {
        let key = self.id.with(("add", ptr));
        let mut name: String = ui.data(|d| d.get_temp(key)).unwrap_or_default();
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("new entry name").desired_width(140.0));
            if r.changed() {
                ui.data_mut(|d| d.insert_temp(key, name.clone()));
            }
            let free = !name.is_empty() && !value.and_then(Value::as_object).is_some_and(|m| m.contains_key(&name));
            if ui.add_enabled(free, egui::Button::new("Add")).clicked() {
                self.out.extend(model::map_add(ptr, value, &name, model::default_value(vnode)));
                ui.data_mut(|d| d.remove_temp::<String>(key));
            }
        });
    }

    /// A collapsible section: the heading, its set/reset or entry buttons, findings, then the contents.
    fn section(&mut self, ui: &mut egui::Ui, label: &str, node: &Node, ptr: &str, slot: Slot) {
        let mut header = egui::CollapsingHeader::new(label).id_salt(ptr);
        if self.fresh && self.focus_on_path(ptr) {
            header = header.open(Some(true));
        }
        let r = header.show(ui, |ui| {
            self.controls(ui, node, ptr, slot);
            self.notes(ui, ptr);
            self.body(ui, node, ptr);
        });
        if !node.description.is_empty() {
            r.header_response.clone().on_hover_text(&node.description);
        }
        self.mark(ui, r.header_response.rect, ptr);
    }

    /// The buttons at the top of a section: Set / Reset against the parent document, or the entry's / element's own.
    fn controls(&mut self, ui: &mut egui::Ui, node: &Node, ptr: &str, slot: Slot) {
        let doc = self.doc;
        let present = doc.pointer(ptr);
        let inherited = self.ctx.base.and_then(|i| i.pointer(ptr));
        ui.horizontal(|ui| match slot {
            Slot::Field => match (present, inherited) {
                (None, Some(inh)) => {
                    ui.weak("not set here: the parent's value applies");
                    if ui.button("Set").on_hover_text("Copy the parent's value here to change it").clicked() {
                        self.out.push(model::set_from(ptr, inh));
                    }
                }
                (None, None) => {
                    if !matches!(node.kind, Kind::Object { .. }) {
                        ui.weak("not set");
                        if ui.button("Set").clicked() {
                            self.out.push(model::set_from(ptr, &model::default_value(node)));
                        }
                    }
                }
                (Some(_), inh) => {
                    if (inh.is_some() || self.ctx.base.is_some()) && ui.button("Reset").on_hover_text("Remove this key so the parent's value applies").clicked() {
                        self.out.push(model::reset(ptr));
                    }
                }
            },
            Slot::Entry(parent, key) => self.entry_controls(ui, parent, key),
            Slot::Item(parent, i) => self.item_controls(ui, parent, i),
        });
    }

    fn entry_controls(&mut self, ui: &mut egui::Ui, parent: &str, key: &str) {
        let doc = self.doc;
        let rkey = self.id.with(("rename", parent, key));
        let mut name: String = ui.data(|d| d.get_temp(rkey)).unwrap_or_else(|| key.to_string());
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(140.0));
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(rkey, name.clone()));
        }
        let parent_value = doc.pointer(parent);
        let can = name != key && !name.is_empty() && !parent_value.and_then(Value::as_object).is_some_and(|m| m.contains_key(&name));
        if ui.add_enabled(can, egui::Button::new("Rename")).clicked()
            && let Some(obj) = parent_value
        {
            self.out.extend(model::rename_key(parent, obj, key, &name));
            ui.data_mut(|d| d.remove_temp::<String>(rkey));
        }
        if ui.button("Remove").clicked()
            && let Some(map) = parent_value
        {
            self.out.extend(model::map_remove(parent, map, key));
        }
    }

    fn item_controls(&mut self, ui: &mut egui::Ui, parent: &str, i: usize) {
        let doc = self.doc;
        let Some(arr) = doc.pointer(parent) else { return };
        let len = arr.as_array().map_or(0, Vec::len);
        if ui.add_enabled(i > 0, egui::Button::new("Up")).clicked() {
            self.out.extend(model::array_move(parent, arr, i, -1));
        }
        if ui.add_enabled(i + 1 < len, egui::Button::new("Down")).clicked() {
            self.out.extend(model::array_move(parent, arr, i, 1));
        }
        if ui.button("Remove").clicked() {
            self.out.extend(model::array_remove(parent, arr, i));
        }
    }

    /// One row: label, the editor (or the greyed parent value with Set), buttons, findings.
    fn row(&mut self, ui: &mut egui::Ui, label: &str, node: &Node, ptr: &str, slot: Slot) {
        let doc = self.doc;
        let value = doc.pointer(ptr);
        let inherited = self.ctx.base.and_then(|i| i.pointer(ptr));
        let r = ui.horizontal(|ui| {
            self.label(ui, label, node);
            match value {
                Some(v) => {
                    self.editor(ui, node, ptr, v);
                    match slot {
                        Slot::Field => {
                            if ui.small_button("Reset").on_hover_text("Remove this key so the parent's value (or the default) applies").clicked() {
                                self.out.push(model::reset(ptr));
                            }
                        }
                        Slot::Entry(parent, key) => {
                            ui.menu_button("...", |ui| self.entry_controls(ui, parent, key));
                        }
                        Slot::Item(parent, i) => {
                            self.item_controls(ui, parent, i);
                        }
                    }
                }
                None => {
                    let shown = match (inherited, &node.default) {
                        (Some(v), _) => format!("{} (parent)", brief(v)),
                        (None, Some(d)) => format!("{} (default)", brief(d)),
                        (None, None) => "not set".to_string(),
                    };
                    ui.weak(shown);
                    let hint = if inherited.is_some() { "Copy the parent's value here to change it" } else { "Write this key" };
                    if ui.small_button("Set").on_hover_text(hint).clicked() {
                        let v = inherited.cloned().unwrap_or_else(|| model::default_value(node));
                        self.out.push(model::set_from(ptr, &v));
                    }
                }
            }
        });
        self.mark(ui, r.response.rect, ptr);
        self.notes(ui, ptr);
    }

    /// The widget for the value of a one-row node.
    fn editor(&mut self, ui: &mut egui::Ui, node: &Node, ptr: &str, v: &Value) {
        match &node.kind {
            Kind::Boolean => {
                let mut b = v.as_bool().unwrap_or(false);
                if ui.checkbox(&mut b, "").changed() {
                    self.out.push(Edit::set(ptr, json!(b)));
                }
            }
            Kind::Integer(_) | Kind::Number(_) => {
                if let Some(e) = self.number(ui, &node.kind, ptr, v) {
                    self.out.push(e);
                }
            }
            Kind::String => {
                let mut s = v.as_str().unwrap_or_default().to_string();
                if ui.add(egui::TextEdit::singleline(&mut s).desired_width(180.0)).changed() {
                    self.out.push(Edit::set(ptr, json!(s)));
                }
            }
            Kind::Enum(values) => {
                let current = values.iter().find(|x| *x == v);
                let text = if current.is_some() { brief(v) } else { format!("{} (not an allowed value)", brief(v)) };
                egui::ComboBox::from_id_salt(self.id.with(("enum", ptr))).selected_text(text).show_ui(ui, |ui| {
                    for x in values {
                        if ui.selectable_label(current == Some(x), brief(x)).clicked() {
                            self.out.push(Edit::set(ptr, x.clone()));
                        }
                    }
                });
            }
            Kind::Array { items, min, max, .. } => {
                // a short list of numbers: one row of boxes, with + / - when its length may change
                let arr = v.as_array().cloned().unwrap_or_default();
                let item = items.as_deref();
                for (i, x) in arr.iter().enumerate() {
                    if let Some(item) = item
                        && let Some(e) = self.number(ui, &item.kind, &model::child_pointer(ptr, &i.to_string()), x)
                    {
                        self.out.push(e);
                    }
                }
                let more = max.is_none_or(|m| arr.len() < m);
                let fewer = arr.len() > min.unwrap_or(0);
                if (more || fewer) && ui.add_enabled(more, egui::Button::new("+").small()).clicked() {
                    let new = item.map_or(json!(0), model::default_value);
                    self.out.extend(model::array_add(ptr, Some(v), new));
                }
                if (more || fewer) && ui.add_enabled(fewer, egui::Button::new("-").small()).clicked() {
                    self.out.extend(model::array_remove(ptr, v, arr.len() - 1));
                }
            }
            _ => self.raw(ui, ptr, v),
        }
    }

    /// A number box for `v` (an integer stays an integer); the edit when it changed.
    fn number(&self, ui: &mut egui::Ui, kind: &Kind, ptr: &str, v: &Value) -> Option<Edit> {
        let range = match kind {
            Kind::Integer(r) | Kind::Number(r) => *r,
            _ => model::Range::default(),
        };
        let integer = matches!(kind, Kind::Integer(_));
        let mut x = v.as_f64().unwrap_or(0.0);
        let mut drag = egui::DragValue::new(&mut x).range(range.min.unwrap_or(f64::MIN)..=range.max.unwrap_or(f64::MAX)).speed(if integer { 0.1 } else { 0.05 });
        if integer {
            drag = drag.max_decimals(0);
        }
        let r = ui.add(drag);
        r.changed().then(|| Edit::set(ptr, model::number_value(kind, x, r.dragged(), v.is_i64() || v.is_u64())))
    }

    /// Raw JSON text, committed when the box loses focus and the text parses.
    fn raw(&mut self, ui: &mut egui::Ui, ptr: &str, v: &Value) {
        let key = self.id.with(("raw", ptr));
        let mut text: String = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| v.to_string());
        let parsed = serde_json::from_str::<Value>(&text);
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(200.0).text_color_opt(parsed.is_err().then_some(ERROR)));
        if parsed.is_err() {
            r.clone().on_hover_text("Not valid JSON: the change is kept only when it parses");
        }
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        }
        if r.lost_focus() {
            ui.data_mut(|d| d.remove_temp::<String>(key));
            if let Ok(p) = parsed
                && p != *v
            {
                self.out.push(Edit::set(ptr, p));
            }
        }
    }

    /// A key the schema does not list: yellow row with its raw value, a "Rename to" list of the known keys and Remove.
    fn unknown_row(&mut self, ui: &mut egui::Ui, parent: &str, key: &str, known: &[String]) {
        let doc = self.doc;
        let ptr = model::child_pointer(parent, key);
        let obj = doc.pointer(parent);
        let taken = |k: &str| obj.and_then(Value::as_object).is_some_and(|m| m.contains_key(k));
        let r = ui.horizontal(|ui| {
            ui.colored_label(WARNING, format!("unknown key `{key}`"));
            if let Some(v) = doc.pointer(&ptr) {
                ui.weak(brief(v));
            }
            egui::ComboBox::from_id_salt(self.id.with(("unknown", &ptr))).selected_text("Rename to").show_ui(ui, |ui| {
                for k in known.iter().filter(|k| !taken(k)) {
                    if ui.selectable_label(false, k).clicked()
                        && let Some(o) = obj
                    {
                        self.out.extend(model::rename_key(parent, o, key, k));
                    }
                }
            });
            if ui.button("Remove").clicked() {
                self.out.push(model::reset(&ptr));
            }
        });
        self.mark(ui, r.response.rect, &ptr);
        self.notes(ui, &ptr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply;

    /// Runs one frame of the form for `schema` (a headless egui context) and returns its edits.
    fn frame(schema: &Value, doc: &Value, ctx: &FormCtx) -> Vec<Edit> {
        let egui_ctx = egui::Context::default();
        let mut out = vec![];
        for _ in 0..2 {
            // two frames: the first lays out, the second sees the memory of the first
            let mut full = egui_ctx.run_ui(egui::RawInput::default(), |ui| {
                out = show(ui, schema, doc, ctx, egui::Id::new("form"));
            });
            full.textures_delta.clear();
        }
        out
    }

    fn schema(name: &str) -> Value {
        openprocyon::authoring::schemas().into_iter().find(|(n, _)| *n == name).unwrap().1
    }

    #[test]
    fn the_ship_form_draws_and_returns_no_edits_without_input() {
        let ship: Value = serde_json::from_str(include_str!("../../tests/fixtures/form/ship.json")).unwrap();
        let parent = json!({"race": "Other", "sections": {"Hull": {"hp": 10}, "Keel": {"hp": 5}}, "guns": {"Gun_A": {"arc_deg": 10}}, "crew_posts": {"Gunners": 1}, "ring": 0.7, "tow": {"tower": [1, 1, 1]}});
        let findings = [
            FieldNote { pointer: "/guns/Gun_A/arc_deg".into(), level: Level::Error, message: "too wide".into() },
            FieldNote { pointer: "/engines/1/fx".into(), level: Level::Warning, message: "unknown effect".into() },
            FieldNote { pointer: "/nowhere".into(), level: Level::Warning, message: "not drawn".into() },
        ];
        for focus in [None, Some("/guns/Gun_B/arc_deg"), Some("/sections/Hull/destruct/fx"), Some("/sigth")] {
            for inherited in [None, Some(&parent)] {
                let ctx = FormCtx { base: inherited, findings: &findings, focus };
                assert!(frame(&schema("ship.schema.json"), &ship, &ctx).is_empty(), "{focus:?}");
            }
        }
        // an empty document and a document of the wrong shape do not panic either
        let ctx = FormCtx { base: Some(&parent), findings: &[], focus: None };
        for doc in [json!({}), json!(null), json!({"guns": [1], "engines": {"a": 1}, "points": "x", "physics": 5})] {
            assert!(frame(&schema("ship.schema.json"), &doc, &ctx).is_empty());
        }
    }

    #[test]
    fn the_other_schemas_draw_too() {
        let ctx = FormCtx { base: None, findings: &[], focus: None };
        let guns = json!({"$schema": "guns.schema.json", "Test Gun": {"Mount Type": "Medium Mount", "MuzzleSpeed": 10, "BulletsPerBurst": 3, "BulletAffectedByGravity": false, "visual": {"base": "b", "pivot": [0, 0, 0], "muzzles": [[0, 1, 0], [1, 1, 0]]}, "beam": {"charge": 1}, "typo_key": 1}});
        assert!(frame(&schema("guns.schema.json"), &guns, &ctx).is_empty());
        let bullets =
            json!({"B": {"lifetime": 2.5, "impact_snd": {"Wood": ["a", "b"]}, "damage": {"Wood": [1, 2, 0, 0, 0]}, "blast": {"x": 1}, "physics": {"Mass": 3}, "oops": 1}});
        assert!(frame(&schema("bullets.schema.json"), &bullets, &ctx).is_empty());
        let crew = json!({"Ship": [{"name": "Crew_Walk_0_0", "pos": [1, 2, 3], "rot": null}]});
        assert!(frame(&schema("crew_nodes.schema.json"), &crew, &ctx).is_empty());
        let m = json!({"id": "m", "format": 1, "requires": ["a", "b"], "extra": true});
        let inh = json!({"id": "p", "name": "Parent"});
        let ctx = FormCtx { base: Some(&inh), findings: &[], focus: Some("/requires/1") };
        assert!(frame(&schema("mod.schema.json"), &m, &ctx).is_empty());
    }

    #[test]
    fn the_unknown_key_rename_produces_a_working_edit() {
        let ship: Value = serde_json::from_str(include_str!("../../tests/fixtures/form/ship.json")).unwrap();
        let s = schema("ship.schema.json");
        let unknown = openprocyon::authoring::unknown_keys(&s, &ship);
        let sigth = unknown.iter().find(|(p, _)| p == "sigth").expect("the typo is reported");
        assert!(sigth.1.iter().any(|k| k == "sight"), "known keys are offered");
        let ptr = edit::dotted_to_pointer(&sigth.0);
        let (parent, key) = model::split_pointer(&ptr).unwrap();
        let mut doc = ship.clone();
        let edits = model::rename_key(&parent, doc.pointer(&parent).unwrap(), &key, "sight");
        let applied: Vec<Edit> = edits.iter().map(|e| apply(&mut doc, e).unwrap()).collect();
        assert_eq!(doc["sight"], 300);
        assert!(doc.get("sigth").is_none());
        for e in applied.iter().rev() {
            edit::undo(&mut doc, e).unwrap();
        }
        assert_eq!(doc, ship);
    }
}
