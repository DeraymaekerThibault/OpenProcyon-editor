//! The reference walk (docs/phase5.md T2, P3, P6): every rule field that names something (a group, a volume, a path, a player, a task, a
//! point, a flag, a timer, a rule, a speech event ...), found by walking the rules and tasks of a document and looking each field's
//! `kind` up in the engine's rule schema (`authoring::rule_schema()`), so which field holds which kind is the engine's data.
//!
//! PROVISIONAL until the engine exposes `visit_fields`: the walk itself restates the engine's crate-private `visit_fields`
//! (`game/src/rules/check.rs:265`, pin `f91542f`; docs/phase5.md engine requirement 6). It visits what that does: every string a rule
//! writes in a field of its Type, `conditions` and `actions` of each rule, nested `condition_list` and `condition` fields
//! (`OP Any Of`, `OP Not`) and the `complete_when` / `fail_when` lists of tasks; an entry of a Type the schema does not know is
//! skipped. Also restated, PROVISIONAL:
//! - which kinds name something ([`NAME_KINDS`]) and which do not; a test feeds every kind of the schema through them, so a kind the
//!   engine adds and this file does not know fails there instead of renaming too little;
//! - `Group,Unit`: a `group` field's text splits at the first comma (`split_once(',')`, `check.rs:758`), the part before is a group or a
//!   player (the same namespace, as `Defs::accepts` has it: [`Kind::Group`] and [`Kind::Player`] take each other's names), the part
//!   after is a `unit` (a ship placed by name);
//! - a `rule` field is matched by its trimmed text (`Defs::accepts`, `Kind::Rule`).
//!
//! Only the rules and tasks are walked, not the definitions: renaming the volume itself, the group, the task or the rule is the
//! caller's edit, in the same `do_group` step as [`rename_edits`].

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

use super::doc as mdoc;
use crate::edit::{Edit, pointer};
use crate::project::Project;

/// The schema's field kinds whose text names something of the mission (`Kind::name`). `group` is the one with a `,unit` part.
/// PROVISIONAL (restated from the engine, see the module doc).
pub const NAME_KINDS: &[&str] = &[
    "group",
    "player",
    "path",
    "volume",
    "task",
    "point",
    "timer",
    "flag",
    "counter",
    "campaign_counter",
    "rule",
    "speech_event",
    "point_set",
    "map_text",
    "message",
    "team",
    "ship_name",
];

/// The schema's kinds that hold no name: nested conditions (walked into) and plain values.
pub const OTHER_KINDS: &[&str] = &["condition_list", "condition", "bool", "number", "enum", "text"];

/// The name kind of the part after the comma of a `group` field.
pub const UNIT: &str = "unit";

/// The two kinds of one namespace: a group field takes a player and a player field takes a group (`Defs::accepts`).
fn same_names(a: &str, b: &str) -> bool {
    a == b || (matches!(a, "group" | "player") && matches!(b, "group" | "player"))
}

/// The fields of every Type of the rule schema, with their kind, by (is a condition, Type name).
#[derive(Debug, Default)]
pub struct FieldIndex {
    types: HashMap<(bool, String), Vec<(String, String)>>,
}

impl FieldIndex {
    /// The index of the engine's own schema (built once).
    pub fn engine() -> &'static FieldIndex {
        static INDEX: OnceLock<FieldIndex> = OnceLock::new();
        INDEX.get_or_init(|| FieldIndex::from_json(&openprocyon::authoring::rule_schema()))
    }

    /// The index of a schema in the shape of `--dump-rule-schema`.
    pub fn from_json(schema: &Value) -> FieldIndex {
        let mut types = HashMap::new();
        for t in schema["types"].as_array().into_iter().flatten() {
            let Some(name) = t["name"].as_str() else { continue };
            let fields = t["fields"].as_array().into_iter().flatten().filter_map(|f| Some((f["name"].as_str()?.to_string(), f["kind"].as_str()?.to_string()))).collect();
            types.insert((t["role"] == "condition", name.to_string()), fields);
        }
        FieldIndex { types }
    }

    fn fields(&self, is_condition: bool, ty: &str) -> Option<&[(String, String)]> {
        self.types.get(&(is_condition, ty.to_string())).map(Vec::as_slice)
    }
}

/// One string a rule writes in a field of its Type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    /// JSON pointer of the field in the document.
    pub pointer: String,
    /// The field's kind (`Kind::name`: `group`, `volume`, `flag`, ...).
    pub kind: String,
    /// The field's name in its Type.
    pub field: String,
    /// The Type of the condition or action that holds the field.
    pub item_type: String,
    /// The text.
    pub text: String,
}

/// Calls `f` for every string field of every rule and task of `mission` (the `visit_fields` of the engine, restated: see the module
/// doc), pointers included.
pub fn walk(mission: &Value, index: &FieldIndex, f: &mut dyn FnMut(&Visit)) {
    fn entry(index: &FieldIndex, e: &Value, is_condition: bool, at: &str, f: &mut dyn FnMut(&Visit)) {
        let Some(ty) = mdoc::item_type(e) else { return };
        let Some(fields) = index.fields(is_condition, ty) else { return };
        for (name, kind) in fields {
            let here = format!("{at}{}", pointer(&[name]));
            match (e.get(name), kind.as_str()) {
                (Some(Value::Array(list)), "condition_list") => {
                    for (i, c) in list.iter().enumerate() {
                        entry(index, c, true, &format!("{here}/{i}"), f);
                    }
                }
                (Some(v @ Value::Object(_)), "condition") => entry(index, v, true, &here, f),
                (Some(Value::String(text)), _) => f(&Visit { pointer: here, kind: kind.clone(), field: name.clone(), item_type: ty.to_string(), text: text.clone() }),
                _ => {}
            }
        }
    }
    for (r, rule) in mdoc::rules(mission).iter().enumerate() {
        for is_condition in [true, false] {
            for (i, e) in mdoc::rule_items(rule, is_condition).iter().enumerate() {
                entry(index, e, is_condition, &mdoc::rule_item_pointer(r, is_condition, i), f);
            }
        }
    }
    for (t, task) in mdoc::tasks(mission).iter().enumerate() {
        for key in [mdoc::TASK_COMPLETE_WHEN, mdoc::TASK_FAIL_WHEN] {
            for (i, e) in mdoc::task_conditions(task, key).iter().enumerate() {
                entry(index, e, true, &format!("{}/{i}", mdoc::task_pointer(t, Some(key))), f);
            }
        }
    }
}

/// Which part of a field's text a reference is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The whole text.
    Whole,
    /// The group (or player) before the comma of `Group,Unit`.
    Group,
    /// The unit after the comma.
    Unit,
}

/// A field that names `name` as a kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// JSON pointer of the field.
    pub pointer: String,
    /// The field's own kind (a `player` field can hold the reference of a renamed `group`).
    pub field_kind: String,
    pub field: String,
    pub item_type: String,
    /// The field's whole text.
    pub text: String,
    pub part: Part,
}

impl Reference {
    /// The field's text with this reference's part replaced by `new`.
    pub fn renamed(&self, new: &str) -> String {
        match self.part {
            Part::Whole => new.to_string(),
            Part::Group => match self.text.split_once(',') {
                Some((_, unit)) => format!("{new},{unit}"),
                None => new.to_string(),
            },
            Part::Unit => match self.text.split_once(',') {
                Some((group, _)) => format!("{group},{new}"),
                None => self.text.clone(),
            },
        }
    }
}

/// The names a field's text holds: (kind, part, name) per name, the `Group,Unit` text as two.
pub fn names_in(v: &Visit) -> Vec<(&'static str, Part, &str)> {
    let Some(kind) = NAME_KINDS.iter().find(|k| **k == v.kind) else { return vec![] };
    if *kind == "group" {
        return match v.text.split_once(',') {
            Some((g, u)) => vec![("group", Part::Group, g), (UNIT, Part::Unit, u)],
            None => vec![("group", Part::Whole, v.text.as_str())],
        };
    }
    vec![(kind, Part::Whole, v.text.as_str())]
}

/// Every field of the mission's rules and tasks that names `name` as `kind` (`group`, `volume`, `unit`, ... : `Kind::name`, and `unit`
/// for a ship named in `Group,Unit`). A `group` and a `player` are one namespace. Rules are matched by trimmed text. Empty names match
/// nothing.
pub fn references(mission: &Value, index: &FieldIndex, kind: &str, name: &str) -> Vec<Reference> {
    let mut out = vec![];
    if name.is_empty() {
        return out;
    }
    walk(mission, index, &mut |v| {
        for (k, part, text) in names_in(v) {
            let hit = if k == "rule" { text.trim() == name.trim() } else { text == name };
            if hit && same_names(k, kind) {
                out.push(Reference { pointer: v.pointer.clone(), field_kind: v.kind.clone(), field: v.field.clone(), item_type: v.item_type.clone(), text: v.text.clone(), part });
            }
        }
    });
    out
}

/// [`references`] against the engine's schema.
pub fn references_of(mission: &Value, kind: &str, name: &str) -> Vec<Reference> {
    references(mission, FieldIndex::engine(), kind, name)
}

/// The edits that make every field naming `old` as `kind` name `new` instead (one set per field). A new name may not be empty or hold
/// a comma in a group, player or unit (it would split). Nothing is edited when `old == new`.
pub fn rename_edits(mission: &Value, index: &FieldIndex, kind: &str, old: &str, new: &str) -> Result<Vec<Edit>, String> {
    if new.is_empty() {
        return Err("a name cannot be empty".into());
    }
    if new.contains(',') && matches!(kind, "group" | "player" | UNIT) {
        return Err("a group, player or unit name cannot hold a comma".into());
    }
    if old == new {
        return Ok(vec![]);
    }
    Ok(references(mission, index, kind, old).iter().map(|r| Edit::set(r.pointer.clone(), Value::String(r.renamed(new)))).collect())
}

/// Renames `old` to `new` in every rule and task field of the document `rel` that names it as `kind`, as one undo step; returns the
/// number of fields changed (no step when none). The thing's own definition (the volume, the group, the task, the rule ...) is not
/// edited: the caller adds that edit to a `do_group` step with [`rename_edits`] when the references must follow it.
pub fn rename(project: &mut Project, rel: &str, kind: &str, old: &str, new: &str) -> Result<usize, String> {
    project.open_doc(rel)?;
    let doc = project.doc(rel).map(|d| &d.edited).ok_or_else(|| format!("{rel} is not open"))?;
    let edits = rename_edits(doc, FieldIndex::engine(), kind, old, new)?;
    let n = edits.len();
    if n > 0 {
        project.do_step(rel, &format!("Rename {kind} {old}"), edits)?;
    }
    Ok(n)
}
