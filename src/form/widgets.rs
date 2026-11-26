//! The egui drawing of the generic schema form. [`show`] draws one document against its schema and RETURNS the edits the user made
//! this frame (it never mutates the document), so the caller's undo stack owns every change. All decisions (what an action turns
//! into) are in [`super::model`]; this file only lays widgets out.

use bevy_egui::egui::{self, Align, Color32, RichText, Stroke};
use openprocyon::authoring::Level;
use serde_json::{Value, json};

use super::model::{self, Kind, Node, Origin};
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
    /// What the document is laid over (the retail ship under an `extends`, the import's gun entry). The document is the MERGED result
    /// (what the game sees), so every field shows where its value comes from: a badge says inherited (equal to `base`), overridden
    /// (differs) or added (`base` has no such key), the last two with "Reset" (back to the base's value, or the key removed), and
    /// hovering the badge shows the base's value. Inherited fields have no Reset and no Remove. `None`: the document stands alone
    /// (`mod.json`, a ship new in the mod): no badges, and Reset removes the key.
    pub base: Option<&'a Value>,
    /// Findings drawn under their field (red for errors, yellow for warnings); those that name no drawn field are listed at the end.
    pub findings: &'a [FieldNote],
    /// A JSON pointer to bring into view: the sections on its path open once, the field scrolls into view and is outlined while this stays set.
    pub focus: Option<&'a str>,
    /// JSON pointers of fields not to draw because the caller draws them itself (the weapons tab's bullet pickers).
    pub hidden: &'a [String],
    /// Fields that take one of a list of names (a gun mount's `gun`): drawn as a drop-down instead of a text box.
    pub choices: &'a [Choice<'a>],
}

/// A string field that is picked from `options`: `pattern` is a JSON pointer in which `*` stands for any one key or index
/// (`/guns/*/gun`).
#[derive(Clone, Copy, Debug)]
pub struct Choice<'a> {
    pub pattern: &'a str,
    pub options: &'a [String],
}

impl Choice<'_> {
    /// Whether `ptr` is a field this choice is for.
    pub fn matches(&self, ptr: &str) -> bool {
        let (a, b): (Vec<&str>, Vec<&str>) = (self.pattern.split('/').collect(), ptr.split('/').collect());
        a.len() == b.len() && a.iter().zip(&b).all(|(p, x)| *p == "*" || p == x)
    }
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
    /// How many of `out` came from a text box or a number box.
    typed: usize,
    drawn: Vec<bool>,
    /// The focus changed this frame: open its sections and scroll to it once.
    fresh: bool,
    id: egui::Id,
}

/// Draws `doc` as a form for `schema` and returns the edits made this frame (JSON pointers relative to `doc`), empty when nothing changed.
/// `id` must be unique per open form (it keys the open sections and the text boxes of the form).
pub fn show(ui: &mut egui::Ui, schema: &Value, doc: &Value, ctx: &FormCtx, id: egui::Id) -> Vec<Edit> {
    show_typed(ui, schema, doc, ctx, id).0
}

/// [`show`], and whether the edits are typing or dragging one box (so they may join the step before); `false` for a button (Reset,
/// Set, add, remove, move, rename), which is always a step of its own.
pub fn show_typed(ui: &mut egui::Ui, schema: &Value, doc: &Value, ctx: &FormCtx, id: egui::Id) -> (Vec<Edit>, bool) {
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
    let mut st = St { ctx, doc, unknown, out: vec![], typed: 0, drawn: vec![false; ctx.findings.len()], fresh, id };
    // An explicit id (not `push_id`): egui derives the ids of the widgets inside from the parent's widget count, so a button that appears
    // above the form (the first edit makes "Revert file" show up) would give every box a new id and the box being typed in would
    // lose focus and its text state.
    ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
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
    let typing = !st.out.is_empty() && st.typed == st.out.len();
    (st.out, typing)
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

/// A value for a hover text: all of it up to a few hundred characters.
fn long(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 300 { format!("{}...", s.chars().take(297).collect::<String>()) } else { s }
}

/// The part of an element that names it in a list's headings (`name`, `label`, ...: a generic guess at a readable name).
fn item_name(item: &Value) -> Option<&str> {
    ["name", "label", "id", "gun", "layer", "fx"].iter().find_map(|k| item.get(*k)?.as_str())
}

impl<'a> St<'a> {
    /// `extends` and `set` of a document laid over another are written by the project when it saves; they are not fields to edit.
    fn managed(&self, parent: &str, key: &str) -> bool {
        parent.is_empty() && self.ctx.base.is_some() && matches!(key, "extends" | "set")
    }

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

    /// What the base has at `ptr`.
    fn base_at(&self, ptr: &str) -> Option<&'a Value> {
        let base: &'a Value = self.ctx.base?;
        base.pointer(ptr)
    }

    /// Where the document's value at `ptr` comes from; `None` without a base or without a value.
    fn origin(&self, ptr: &str) -> Option<Origin> {
        self.ctx.base?;
        model::classify(self.doc.pointer(ptr), self.base_at(ptr))
    }

    /// The badge (inherited / overridden / added) with the base's value on hover; returns the origin.
    fn badge(&self, ui: &mut egui::Ui, ptr: &str) -> Option<Origin> {
        let o = self.origin(ptr)?;
        let color = match o {
            Origin::Inherited => ui.visuals().weak_text_color(),
            Origin::Overridden => Color32::from_rgb(110, 170, 240),
            Origin::Added => Color32::from_rgb(110, 200, 130),
        };
        let hover = match (o, self.base_at(ptr)) {
            (Origin::Inherited, Some(b)) => format!("Same as the parent: {}", long(b)),
            (_, Some(b)) => format!("The parent (retail) has: {}", long(b)),
            (_, None) => "The parent has no such key: the mod adds it".to_string(),
        };
        ui.label(RichText::new(o.label()).small().color(color)).on_hover_text(hover);
        Some(o)
    }

    /// "Reset" next to an overridden or added field: back to the base's value, or the key removed. An inherited field gets none.
    fn reset_button(&mut self, ui: &mut egui::Ui, ptr: &str, origin: Option<Origin>) {
        if matches!(origin, Some(Origin::Overridden | Origin::Added))
            && ui.small_button("Reset to parent").on_hover_text("Put the parent's value back (or remove the key the parent does not have)").clicked()
        {
            self.out.push(model::reset_to_base(ptr, self.base_at(ptr)));
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

    fn label(&self, ui: &mut egui::Ui, text: &str, node: &Node, slot: Slot) {
        let r = ui.label(shown(text, slot));
        label_hover(r, text, shown(text, slot), node);
    }

    /// One property: a collapsible section for a value with parts, else one row.
    fn field(&mut self, ui: &mut egui::Ui, label: &str, node: &Node, ptr: &str) {
        if self.ctx.hidden.iter().any(|h| h == ptr) {
            return;
        }
        if node.is_compound() { self.section(ui, label, node, ptr, Slot::Field) } else { self.row(ui, label, node, ptr, Slot::Field) }
    }

    /// The contents of a compound value (no heading).
    fn body(&mut self, ui: &mut egui::Ui, node: &Node, ptr: &str) {
        let doc = self.doc;
        let value = doc.pointer(ptr);
        match &node.kind {
            Kind::Object { props } => {
                for (name, child) in props {
                    if self.managed(ptr, name) {
                        continue;
                    }
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
            let enter = crate::layout::entered(ui, &r);
            if ui.add_enabled(free, egui::Button::new("Add")).clicked() || (enter && free) {
                self.out.extend(model::map_add(ptr, value, &name, model::default_value(vnode)));
                ui.data_mut(|d| d.remove_temp::<String>(key));
            }
        });
    }

    /// A collapsible section: the heading, its set/reset or entry buttons, findings, then the contents.
    fn section(&mut self, ui: &mut egui::Ui, label: &str, node: &Node, ptr: &str, slot: Slot) {
        let mut header = egui::CollapsingHeader::new(shown(label, slot)).id_salt(ptr);
        if self.fresh && self.focus_on_path(ptr) {
            header = header.open(Some(true));
        }
        let r = header.show(ui, |ui| {
            self.controls(ui, node, ptr, slot);
            self.notes(ui, ptr);
            self.body(ui, node, ptr);
        });
        label_hover(r.header_response.clone(), label, shown(label, slot), node);
        self.mark(ui, r.header_response.rect, ptr);
    }

    /// The buttons at the top of a section: Set / Reset against the parent document, or the entry's / element's own.
    fn controls(&mut self, ui: &mut egui::Ui, node: &Node, ptr: &str, slot: Slot) {
        let doc = self.doc;
        let present = doc.pointer(ptr);
        let inherited = self.ctx.base.and_then(|i| i.pointer(ptr));
        ui.horizontal(|ui| {
            let origin = self.badge(ui, ptr);
            match slot {
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
                    (Some(_), _) if self.ctx.base.is_some() => self.reset_button(ui, ptr, origin),
                    (Some(_), _) => {}
                },
                Slot::Entry(parent, key) => self.entry_controls(ui, parent, key),
                Slot::Item(parent, i) => self.item_controls(ui, parent, i),
            }
        });
    }

    fn entry_controls(&mut self, ui: &mut egui::Ui, parent: &str, key: &str) {
        // an entry the document below already has (or retail, being its own base) can be neither renamed nor removed here
        if self.origin(&model::child_pointer(parent, key)) == Some(Origin::Inherited) {
            return;
        }
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
        let origin = self.origin(&model::child_pointer(parent, key));
        if origin == Some(Origin::Overridden) {
            self.reset_button(ui, &model::child_pointer(parent, key), origin);
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
            self.label(ui, label, node, slot);
            match value {
                Some(v) => {
                    self.editor(ui, node, ptr, v);
                    let origin = self.badge(ui, ptr);
                    match slot {
                        Slot::Field => {
                            if self.ctx.base.is_some() {
                                self.reset_button(ui, ptr, origin);
                            } else if ui.small_button("Reset").on_hover_text("Remove this key so the default applies").clicked() {
                                self.out.push(model::reset(ptr));
                            }
                        }
                        Slot::Entry(parent, key) => {
                            if origin != Some(Origin::Inherited) {
                                ui.menu_button("...", |ui| self.entry_controls(ui, parent, key));
                            }
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
                    self.typed += 1;
                    self.out.push(e);
                }
            }
            Kind::String if self.ctx.choices.iter().any(|c| c.matches(ptr)) => {
                let current = v.as_str().unwrap_or_default();
                let choice = self.ctx.choices.iter().find(|c| c.matches(ptr)).expect("checked above");
                let text = match (current.is_empty(), choice.options.iter().any(|o| o == current)) {
                    (true, _) => "(none)".to_string(),
                    (false, true) => current.to_string(),
                    (false, false) => format!("{current} (not in the list)"),
                };
                egui::ComboBox::from_id_salt(self.id.with(("choice", ptr))).selected_text(text).show_ui(ui, |ui| {
                    for o in choice.options {
                        if ui.selectable_label(o == current, o).clicked() && o != current {
                            self.out.push(Edit::set(ptr, json!(o)));
                        }
                    }
                });
            }
            Kind::String => {
                let mut s = v.as_str().unwrap_or_default().to_string();
                let edit = if is_long_text(ptr) {
                    egui::TextEdit::multiline(&mut s).desired_rows(3).desired_width(ui.available_width().max(180.0))
                } else {
                    egui::TextEdit::singleline(&mut s).desired_width(180.0)
                };
                if ui.add(edit).changed() {
                    self.typed += 1;
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
                        self.typed += 1;
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
        let mut drag =
            egui::DragValue::new(&mut x).update_while_editing(false).range(range.min.unwrap_or(f64::MIN)..=range.max.unwrap_or(f64::MAX)).speed(if integer { 0.1 } else { 0.05 });
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
        if self.managed(parent, key) {
            return;
        }
        let doc = self.doc;
        let ptr = model::child_pointer(parent, key);
        let base_v = self.base_at(&ptr);
        // A key the file below has and the mod leaves alone (retail ships carry keys the schema does not list): shown, never a warning,
        // and there is nothing to remove or rename here.
        if base_v.is_some() && base_v == doc.pointer(&ptr) {
            let r = ui.horizontal(|ui| {
                ui.weak(format!("`{key}`"));
                if let Some(v) = doc.pointer(&ptr) {
                    ui.weak(brief(v));
                }
                ui.weak("inherited, not in the schema").on_hover_text("The parent has this key and the schema does not list it; it is kept as it is");
            });
            self.mark(ui, r.response.rect, &ptr);
            self.notes(ui, &ptr);
            return;
        }
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
            if ui.button(if base_v.is_some() { "Reset to parent" } else { "Remove" }).clicked() {
                self.out.push(model::reset_to_base(&ptr, base_v));
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

    /// One frame at `t` with `events`; `above` draws that many buttons before the form, the way the panel's buttons come and go.
    fn run(egui_ctx: &egui::Context, doc: &Value, base: &Value, above: usize, events: Vec<egui::Event>) -> (Vec<Edit>, Vec<(String, egui::Pos2, egui::Vec2)>) {
        let schema = crate::ships_panel::form_schema(schema("ship.schema.json"));
        let ctx = FormCtx { base: Some(base), findings: &[], focus: None, hidden: &[], choices: &[] };
        let mut out = vec![];
        let mut full = egui_ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
            for i in 0..above {
                let _ = ui.button(format!("above {i}"));
            }
            out = show(ui, &schema, doc, &ctx, egui::Id::new("ship form"));
        });
        full.textures_delta.clear();
        let mut texts = vec![];
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2, egui::Vec2)>) {
            match shape {
                egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos, t.galley.size())),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        full.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        (out, texts)
    }

    #[test]
    fn a_button_appearing_above_the_form_does_not_take_focus_from_the_box_being_typed_in() {
        let base = json!({"race": "Navy", "points": 10});
        let doc = json!({"race": "Navy", "points": 10});
        let egui_ctx = egui::Context::default();
        let (_, texts) = run(&egui_ctx, &doc, &base, 0, vec![]);
        let (_, texts2) = run(&egui_ctx, &doc, &base, 0, vec![]);
        assert_eq!(texts.len(), texts2.len());
        let (_, pos, size) = texts2.iter().find(|(t, _, _)| t == "Points").cloned().expect("a points label");
        let click = egui::pos2(pos.x + size.x + 8.0 + 10.0, pos.y + size.y / 2.0);
        let button = |pressed| egui::Event::PointerButton { pos: click, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        run(&egui_ctx, &doc, &base, 0, vec![egui::Event::PointerMoved(click), button(true)]);
        run(&egui_ctx, &doc, &base, 0, vec![button(false)]);
        run(&egui_ctx, &doc, &base, 0, vec![]);
        let focused = egui_ctx.memory(|m| m.focused()).expect("the click put the number box in edit mode");
        assert!(egui_ctx.read_response(focused).is_some(), "the focused box exists");
        // the first edit makes a button appear above the form: the same box is still there, still focused
        for above in [1, 2, 0, 3] {
            run(&egui_ctx, &doc, &base, above, vec![]);
            run(&egui_ctx, &doc, &base, above, vec![]);
            assert_eq!(egui_ctx.memory(|m| m.focused()), Some(focused), "{above} buttons above");
            assert!(egui_ctx.read_response(focused).is_some(), "{above} buttons above: the box kept its id");
        }
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
                let ctx = FormCtx { base: inherited, findings: &findings, focus, hidden: &[], choices: &[] };
                assert!(frame(&schema("ship.schema.json"), &ship, &ctx).is_empty(), "{focus:?}");
            }
        }
        // an empty document and a document of the wrong shape do not panic either
        let ctx = FormCtx { base: Some(&parent), findings: &[], focus: None, hidden: &[], choices: &[] };
        for doc in [json!({}), json!(null), json!({"guns": [1], "engines": {"a": 1}, "points": "x", "physics": 5})] {
            assert!(frame(&schema("ship.schema.json"), &doc, &ctx).is_empty());
        }
    }

    #[test]
    fn the_other_schemas_draw_too() {
        let ctx = FormCtx { base: None, findings: &[], focus: None, hidden: &[], choices: &[] };
        let guns = json!({"$schema": "guns.schema.json", "Test Gun": {"Mount Type": "Medium Mount", "MuzzleSpeed": 10, "BulletsPerBurst": 3, "BulletAffectedByGravity": false, "visual": {"base": "b", "pivot": [0, 0, 0], "muzzles": [[0, 1, 0], [1, 1, 0]]}, "beam": {"charge": 1}, "typo_key": 1}});
        assert!(frame(&schema("guns.schema.json"), &guns, &ctx).is_empty());
        let bullets =
            json!({"B": {"lifetime": 2.5, "impact_snd": {"Wood": ["a", "b"]}, "damage": {"Wood": [1, 2, 0, 0, 0]}, "blast": {"x": 1}, "physics": {"Mass": 3}, "oops": 1}});
        assert!(frame(&schema("bullets.schema.json"), &bullets, &ctx).is_empty());
        let crew = json!({"Ship": [{"name": "Crew_Walk_0_0", "pos": [1, 2, 3], "rot": null}]});
        assert!(frame(&schema("crew_nodes.schema.json"), &crew, &ctx).is_empty());
        let m = json!({"id": "m", "format": 1, "requires": ["a", "b"], "extra": true});
        let inh = json!({"id": "p", "name": "Parent"});
        let ctx = FormCtx { base: Some(&inh), findings: &[], focus: Some("/requires/1"), hidden: &[], choices: &[] };
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

    /// Every piece of text one frame of the form paints (labels, buttons, hover-less), in drawing order.
    fn painted(schema: &Value, doc: &Value, ctx: &FormCtx) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let egui_ctx = egui::Context::default();
        let mut texts = vec![];
        for _ in 0..2 {
            texts.clear();
            let mut full = egui_ctx.run_ui(egui::RawInput::default(), |ui| {
                show(ui, schema, doc, ctx, egui::Id::new("painted"));
            });
            full.textures_delta.clear();
            full.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        }
        texts
    }

    fn has(texts: &[String], s: &str) -> bool {
        texts.iter().any(|t| t == s)
    }

    #[test]
    fn inherited_fields_show_a_badge_and_never_reset_while_changed_ones_can_go_back() {
        let ship = schema("ship.schema.json");
        let ship = crate::ships_panel::form_schema(ship);
        let base = json!({"race": "Navy", "points": 10, "extends": "x"});
        // the merged document: race inherited, points overridden, sight added; `extends` is the project's business
        let doc = json!({"race": "Navy", "points": 12, "sight": 300, "extends": "x"});
        let ctx = FormCtx { base: Some(&base), findings: &[], focus: None, hidden: &[], choices: &[] };
        let t = painted(&ship, &doc, &ctx);
        assert!(has(&t, "inherited") && has(&t, "overridden") && has(&t, "added"), "{t:?}");
        assert_eq!(t.iter().filter(|x| *x == "Reset to parent").count(), 2, "points and sight, not race: {t:?}");
        assert!(!t.iter().any(|x| x.contains("Remove")), "{t:?}");
        // only inherited values: no Reset anywhere
        let same = json!({"race": "Navy", "points": 10});
        let t = painted(&ship, &same, &ctx);
        assert!(has(&t, "inherited") && !t.iter().any(|x| x.contains("Reset") || x.contains("Remove")), "{t:?}");
        // `extends` is not a field (and not "not set")
        assert!(!t.iter().any(|x| x.contains("extends")), "{t:?}");
        let raw = schema("ship.schema.json");
        let t = painted(&raw, &same, &ctx);
        assert!(!t.iter().any(|x| x == "extends" || x == "set"), "managed keys are skipped when there is a base: {t:?}");
        // without a base the old behaviour stays: Reset on a set key, no badges
        let alone = FormCtx { base: None, findings: &[], focus: None, hidden: &[], choices: &[] };
        let t = painted(&ship, &same, &alone);
        assert!(has(&t, "Reset") && !has(&t, "inherited"), "{t:?}");
    }

    #[test]
    fn a_retail_key_the_schema_lacks_is_shown_read_only_and_only_the_mods_own_unknown_keys_warn() {
        let ship = crate::ships_panel::form_schema(schema("ship.schema.json"));
        let base = json!({"race": "Navy", "entity": 7, "skipped_guns": ["a"]});
        let doc = json!({"race": "Navy", "entity": 7, "skipped_guns": ["a"], "sigth": 3});
        let ctx = FormCtx { base: Some(&base), findings: &[], focus: None, hidden: &[], choices: &[] };
        let t = painted(&ship, &doc, &ctx);
        assert!(has(&t, "`entity`") && has(&t, "`skipped_guns`"), "{t:?}");
        assert!(t.iter().any(|x| x.contains("unknown key `sigth`")), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("unknown key `entity`") || x.contains("unknown key `skipped_guns`")), "{t:?}");
        assert_eq!(t.iter().filter(|x| *x == "Remove").count(), 1, "Remove only for the mod's own typo: {t:?}");
    }

    #[test]
    fn with_no_mod_open_the_retail_file_is_its_own_base_so_every_field_is_inherited() {
        // what the ship and weapons tabs pass with no project: base == the document
        let ship = crate::ships_panel::form_schema(schema("ship.schema.json"));
        let doc = json!({"race": "Navy", "points": 10, "entity": 7, "skipped_guns": ["a"]});
        let t = painted(&ship, &doc, &FormCtx { base: Some(&doc), findings: &[], focus: None, hidden: &[], choices: &[] });
        assert!(has(&t, "inherited") && has(&t, "`entity`") && has(&t, "`skipped_guns`"), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("Reset") || x.contains("Remove") || x.contains("Rename") || x.contains("unknown key")), "{t:?}");
        let guns = json!({"Test Gun": {"Mount Type": "Medium Mount", "MuzzleSpeed": 10, "response": 3}});
        let entry = &guns["Test Gun"];
        let t = painted(&schema("guns.schema.json"), entry, &FormCtx { base: Some(entry), findings: &[], focus: None, hidden: &[], choices: &[] });
        assert!(!t.iter().any(|x| x.contains("Reset") || x.contains("Remove") || x.contains("unknown key")), "{t:?}");
    }

    #[test]
    fn an_inherited_map_entry_has_no_rename_or_remove_but_the_mods_own_entry_has() {
        let ship = crate::ships_panel::form_schema(schema("ship.schema.json"));
        let doc = json!({"points": 10, "guns": {"M1": {"gun": "G1", "arc_deg": 90}}});
        let t = painted(&ship, &doc, &FormCtx { base: Some(&doc), findings: &[], focus: Some("/guns/M1"), hidden: &[], choices: &[] });
        assert!(!t.iter().any(|x| x.contains("Rename") || x == "Remove"), "{t:?}");
        let base = json!({"points": 10, "guns": {}});
        let t = painted(&ship, &doc, &FormCtx { base: Some(&base), findings: &[], focus: Some("/guns/M1"), hidden: &[], choices: &[] });
        assert!(has(&t, "Rename") && has(&t, "Remove"), "{t:?}");
    }

    #[test]
    fn a_choice_field_is_a_drop_down_of_the_options() {
        let c = Choice { pattern: "/guns/*/gun", options: &[] };
        assert!(c.matches("/guns/M1/gun") && c.matches("/guns/7/gun"));
        assert!(!c.matches("/guns/M1/arc_deg") && !c.matches("/guns/gun") && !c.matches("/guns/M1/gun/x"));
        let ship = crate::ships_panel::form_schema(schema("ship.schema.json"));
        let doc = json!({"points": 10, "guns": {"M1": {"gun": "G9", "arc_deg": 90}}});
        let options = vec!["G1".to_string(), "G2".to_string()];
        let choices = [Choice { pattern: "/guns/*/gun", options: &options }];
        let t = painted(&ship, &doc, &FormCtx { base: None, findings: &[], focus: Some("/guns/M1"), hidden: &[], choices: &choices });
        assert!(has(&t, "G9 (not in the list)"), "{t:?}");
        let t = painted(&ship, &doc, &FormCtx { base: None, findings: &[], focus: Some("/guns/M1"), hidden: &[], choices: &[] });
        assert!(!has(&t, "G9 (not in the list)"), "without choices it stays a text box");
    }

    #[test]
    fn hidden_fields_are_not_drawn() {
        let ship = crate::ships_panel::form_schema(schema("ship.schema.json"));
        let doc = json!({"race": "Navy", "points": 10});
        let hidden = ["/race".to_string()];
        let t = painted(&ship, &doc, &FormCtx { base: None, findings: &[], focus: None, hidden: &hidden, choices: &[] });
        assert!(!has(&t, "race") && has(&t, "Points"), "{t:?}");
    }
}

/// The label shown for a key: humanised for a named property; map entries and list items keep their own names.
fn shown(key: &str, slot: Slot) -> String {
    if matches!(slot, Slot::Field) { humanise(key) } else { key.to_string() }
}

/// Hover text of a label: the raw key (when the shown label differs) and the schema description.
fn label_hover(r: egui::Response, key: &str, shown: String, node: &Node) {
    match (shown != key, node.description.is_empty()) {
        (false, true) => {}
        (false, false) => {
            r.on_hover_text(&node.description);
        }
        (true, true) => {
            r.on_hover_text(format!("Key: {key}"));
        }
        (true, false) => {
            r.on_hover_text(format!("{}\n\nKey: {key}", node.description));
        }
    }
}

/// A readable label for a JSON key: split on `_`, first word capitalised, known acronyms upper-case. Presentation only.
/// A key that is not a plain snake_case word list (digits-only, spaces, slashes: map keys, ids) is shown as it is.
pub fn humanise(key: &str) -> String {
    const ACRONYMS: [&str; 8] = ["hp", "fx", "id", "snd", "ai", "ui", "xp", "hud"];
    let plain = !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && key.chars().any(|c| c.is_ascii_lowercase());
    if !plain {
        return key.to_string();
    }
    let words: Vec<&str> = key.split('_').filter(|w| !w.is_empty()).collect();
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if ACRONYMS.contains(w) {
                w.to_uppercase()
            } else if i == 0 {
                let mut c = w.chars();
                c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
            } else {
                (*w).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a string field holds prose and gets a multi-line box, judged by the last segment of its pointer.
/// PROVISIONAL until the engine schema carries a hint for long text.
pub fn is_long_text(ptr: &str) -> bool {
    const LONG: [&str; 6] = ["description", "text", "briefing", "summary", "notes", "intro"];
    ptr.rsplit('/').next().is_some_and(|k| LONG.contains(&k))
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn humanises_keys() {
        assert_eq!(humanise("core_hp"), "Core HP");
        assert_eq!(humanise("explosion_fx"), "Explosion FX");
        assert_eq!(humanise("crew_posts"), "Crew posts");
        assert_eq!(humanise("game_version"), "Game version");
        assert_eq!(humanise("snd_id"), "SND ID");
        assert_eq!(humanise("Cutter-1"), "Cutter-1");
        assert_eq!(humanise("12"), "12");
    }

    #[test]
    fn long_text_by_key_name() {
        assert!(is_long_text("/description"));
        assert!(is_long_text("/missions/m1/briefing"));
        assert!(!is_long_text("/name"));
        assert!(!is_long_text("/descriptions/a"));
    }
}
