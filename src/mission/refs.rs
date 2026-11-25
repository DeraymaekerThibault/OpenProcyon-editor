//! The reference walk (docs/phase5.md T2, P3, P6): every rule field that names something (a group, a volume, a path, a player, a task, a
//! point, a flag, a timer, a rule, a speech event ...), found by the engine's own walk, `openprocyon::authoring::visit_fields`, which
//! gives each string field of the rules and tasks with its JSON pointer and its `kind()`, `field()` and `item_type()` (OpenProcyon#646).
//! The editor holds no copy of that walk or of the rule schema's field tables.
//!
//! What the editor still decides, PROVISIONAL (the engine does not say it):
//! - which kinds name something ([`NAME_KINDS`]) and which do not; a test feeds every kind of the engine's schema through them, so a
//!   kind the engine adds and this file does not know fails there instead of renaming too little;
//! - a `group` field's text is `Group,Unit` (`authoring::split_group`): the part before is a group or a player (the same namespace,
//!   as `accepted_names` has it: kinds `group` and `player` take each other's names), the part after is a `unit` (a ship placed by name);
//! - a `rule` field is matched by its trimmed text (the engine's doc of `visit_fields`).
//!
//! Only the rules and tasks are walked, not the definitions (the engine does not either): renaming the volume itself, the group, the
//! task or the rule is the caller's edit, in the same `do_group` step as [`rename_edits`]. That is why `visit_fields_mut` is not used:
//! the editor's edits are pointer edits for the undo stack, which [`rename_edits`] builds from the read walk.

use serde_json::Value;

use openprocyon::authoring::{FieldVisit, split_group, visit_fields};

use crate::edit::Edit;
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

/// Calls `f` for every string field of every rule and task of `mission`, as the engine walks them (pointers included).
pub fn walk(mission: &Value, f: &mut dyn FnMut(&FieldVisit)) {
    visit_fields(mission, f);
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
            Part::Group => match split_group(&self.text) {
                (_, Some(unit)) => format!("{new},{unit}"),
                (_, None) => new.to_string(),
            },
            Part::Unit => match split_group(&self.text) {
                (group, Some(_)) => format!("{group},{new}"),
                (_, None) => self.text.clone(),
            },
        }
    }
}

/// The names a field's text holds: (kind, part, name) per name, the `Group,Unit` text as two.
pub fn names_in<'a>(v: &FieldVisit<'a>) -> Vec<(&'static str, Part, &'a str)> {
    let Some(kind) = NAME_KINDS.iter().find(|k| **k == v.kind()) else { return vec![] };
    if *kind == "group" {
        return match split_group(v.text) {
            (g, Some(u)) => vec![("group", Part::Group, g), (UNIT, Part::Unit, u)],
            (g, None) => vec![("group", Part::Whole, g)],
        };
    }
    vec![(kind, Part::Whole, v.text)]
}

/// Every field of the mission's rules and tasks that names `name` as `kind` (`group`, `volume`, `unit`, ... : `Kind::name`, and `unit`
/// for a ship named in `Group,Unit`). A `group` and a `player` are one namespace. Rules are matched by trimmed text. Empty names match
/// nothing.
pub fn references(mission: &Value, kind: &str, name: &str) -> Vec<Reference> {
    let mut out = vec![];
    if name.is_empty() {
        return out;
    }
    walk(mission, &mut |v| {
        for (k, part, text) in names_in(v) {
            let hit = if k == "rule" { text.trim() == name.trim() } else { text == name };
            if hit && same_names(k, kind) {
                out.push(Reference {
                    pointer: v.pointer.clone(),
                    field_kind: v.kind().to_string(),
                    field: v.field().to_string(),
                    item_type: v.item_type().to_string(),
                    text: v.text.to_string(),
                    part,
                });
            }
        }
    });
    out
}

/// The edits that make every field naming `old` as `kind` name `new` instead (one set per field). A new name may not be empty or hold
/// a comma in a group, player or unit (it would split). Nothing is edited when `old == new`.
pub fn rename_edits(mission: &Value, kind: &str, old: &str, new: &str) -> Result<Vec<Edit>, String> {
    if new.is_empty() {
        return Err("a name cannot be empty".into());
    }
    if new.contains(',') && matches!(kind, "group" | "player" | UNIT) {
        return Err("a group, player or unit name cannot hold a comma".into());
    }
    if old == new {
        return Ok(vec![]);
    }
    Ok(references(mission, kind, old).iter().map(|r| Edit::set(r.pointer.clone(), Value::String(r.renamed(new)))).collect())
}

/// Renames `old` to `new` in every rule and task field of the document `rel` that names it as `kind`, as one undo step; returns the
/// number of fields changed (no step when none). The thing's own definition (the volume, the group, the task, the rule ...) is not
/// edited: the caller adds that edit to a `do_group` step with [`rename_edits`] when the references must follow it.
pub fn rename(project: &mut Project, rel: &str, kind: &str, old: &str, new: &str) -> Result<usize, String> {
    project.open_doc(rel)?;
    let doc = project.doc(rel).map(|d| &d.edited).ok_or_else(|| format!("{rel} is not open"))?;
    let edits = rename_edits(doc, kind, old, new)?;
    let n = edits.len();
    if n > 0 {
        project.do_step(rel, &format!("Rename {kind} {old}"), edits)?;
    }
    Ok(n)
}
