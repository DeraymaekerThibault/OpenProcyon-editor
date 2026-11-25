//! Rule operations on a project document that has a `rules` list: a `mission.json` or a `script.json` (docs/phase5.md T2). Lifted from
//! Phase 4's `map::skirmish`, which now only calls these. Every function is one undo step (typing in one box joins the step before
//! it), takes the document by its path in the project (`rel`) and names no key itself: the keys are [`super::doc`]'s, a rule's own
//! fields come from the engine's rule schema through the forms.
//!
//! A [`Target`] says which document and what to call a rule in undo labels and errors ("rule" for a mission, "script rule" for a
//! skirmish script), so each file keeps its own wording. The document must be open or openable (the caller makes a new `script.json`
//! first). Names that other rules refer to are renamed by [`super::refs`], not here.

use serde_json::Value;

use super::doc as mdoc;
use crate::edit::Edit;
use crate::project::Project;

/// A document with a `rules` list and the words used for one of its rules.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a> {
    pub rel: &'a str,
    pub noun: &'a str,
}

impl<'a> Target<'a> {
    /// The rules of a `mission.json`.
    pub fn mission(rel: &'a str) -> Target<'a> {
        Target { rel, noun: "rule" }
    }

    /// The rules of a skirmish `script.json`.
    pub fn script(rel: &'a str) -> Target<'a> {
        Target { rel, noun: "script rule" }
    }

    fn label(&self, verb: &str) -> String {
        format!("{verb} {}", self.noun)
    }

    fn no_rule(&self, index: usize) -> String {
        format!("no {} {index}", self.noun)
    }
}

/// `stem 1`, `stem 2`, ... : the first not in `taken`.
pub fn unique_name(taken: &[&str], stem: &str) -> String {
    (1..).map(|n| format!("{stem} {n}")).find(|c| !taken.contains(&c.as_str())).expect("a free name exists")
}

/// The document as the game would see it, when it is open and not empty.
pub fn view<'a>(project: &'a Project, rel: &str) -> Option<&'a Value> {
    project.doc(rel).map(|d| &d.edited).filter(|v| !v.is_null())
}

/// The rules of the document (empty when it is not open or has none).
pub fn rules<'a>(project: &'a Project, t: &Target) -> &'a [Value] {
    view(project, t.rel).map_or(&[], mdoc::rules)
}

fn rule<'a>(project: &'a Project, t: &Target, index: usize) -> Result<&'a Value, String> {
    rules(project, t).get(index).ok_or_else(|| t.no_rule(index))
}

fn has_list(doc: &Value, key: &str) -> bool {
    doc.get(key).is_some_and(Value::is_array)
}

/// Appends `rule` to the document's rules; returns its index. Creates the list when the document has none.
pub fn append_rule(project: &mut Project, t: &Target, rule: Value, label: &str) -> Result<usize, String> {
    project.open_doc(t.rel)?;
    let doc = view(project, t.rel).ok_or_else(|| format!("{} is not open", t.rel))?;
    let at = mdoc::rules(doc).len();
    let edit = if has_list(doc, mdoc::KEY_RULES) { Edit::set(mdoc::rule_pointer(at, None), rule) } else { Edit::set(mdoc::rules_pointer(), Value::Array(vec![rule])) };
    project.do_step(t.rel, label, vec![edit])?;
    Ok(at)
}

/// Adds a rule: active, runs once, nothing in it, named "Rule n". Returns its index.
pub fn add_rule(project: &mut Project, t: &Target) -> Result<usize, String> {
    project.open_doc(t.rel)?;
    let doc = view(project, t.rel).ok_or_else(|| format!("{} is not open", t.rel))?;
    let names: Vec<&str> = mdoc::rules(doc).iter().filter_map(mdoc::rule_name).collect();
    let rule = mdoc::new_rule(&unique_name(&names, "Rule"));
    append_rule(project, t, rule, &t.label("Add"))
}

/// Deletes rule `index`.
pub fn delete_rule(project: &mut Project, t: &Target, index: usize) -> Result<(), String> {
    rule(project, t, index)?;
    project.do_step(t.rel, &t.label("Delete"), vec![Edit::remove(mdoc::rule_pointer(index, None))])
}

/// Renames rule `index` (typing joins one step). Rules that name it are not rewritten here: see [`super::refs::rename`].
pub fn rename_rule(project: &mut Project, t: &Target, index: usize, name: &str) -> Result<(), String> {
    rule(project, t, index)?;
    project.apply_form(t.rel, &t.label("Rename"), vec![Edit::set(mdoc::rule_pointer(index, Some(mdoc::RULE_NAME)), Value::from(name))], true)
}

/// `Run Once` or `Is Active` of a rule: JSON booleans (the rule loader reads them as booleans, unlike a Type's fields).
pub fn set_rule_flag(project: &mut Project, t: &Target, index: usize, key: &str, on: bool) -> Result<(), String> {
    rule(project, t, index)?;
    let label = {
        let mut noun = t.noun.chars();
        let first = noun.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
        format!("{first}{} {key}", noun.as_str())
    };
    project.do_step(t.rel, &label, vec![Edit::set(mdoc::rule_pointer(index, Some(key)), Value::Bool(on))])
}

/// Moves rule `from` to position `to` (the rule ends up at index `to`): one step.
pub fn move_rule(project: &mut Project, t: &Target, from: usize, to: usize) -> Result<(), String> {
    let mut all = rules(project, t).to_vec();
    if from >= all.len() {
        return Err(t.no_rule(from));
    }
    if to >= all.len() {
        return Err(t.no_rule(to));
    }
    if from == to {
        return Ok(());
    }
    let moved = all.remove(from);
    all.insert(to, moved);
    project.do_step(t.rel, &t.label("Move"), vec![Edit::set(mdoc::rules_pointer(), Value::Array(all))])
}

/// Copies rule `index` right after itself, named "<name> copy n"; returns the copy's index. The copy is the rule as it is, conditions
/// and actions included.
pub fn duplicate_rule(project: &mut Project, t: &Target, index: usize) -> Result<usize, String> {
    let mut all = rules(project, t).to_vec();
    let original = all.get(index).ok_or_else(|| t.no_rule(index))?.clone();
    let names: Vec<&str> = all.iter().filter_map(mdoc::rule_name).collect();
    let stem = format!("{} copy", mdoc::rule_name(&original).unwrap_or("Rule"));
    let name = unique_name(&names, &stem);
    let mut copy = original;
    if let Some(o) = copy.as_object_mut() {
        o.insert(mdoc::RULE_NAME.into(), name.into());
    }
    all.insert(index + 1, copy);
    project.do_step(t.rel, &t.label("Duplicate"), vec![Edit::set(mdoc::rules_pointer(), Value::Array(all))])?;
    Ok(index + 1)
}

/// Appends `item` to the conditions or the actions of rule `index`; returns its position.
pub fn add_item(project: &mut Project, t: &Target, index: usize, is_condition: bool, item: Value) -> Result<usize, String> {
    let r = rule(project, t, index)?;
    let at = mdoc::rule_items(r, is_condition).len();
    let edit = if has_list(r, mdoc::rule_list_key(is_condition)) {
        Edit::set(mdoc::rule_item_pointer(index, is_condition, at), item)
    } else {
        Edit::set(mdoc::rule_list_pointer(index, is_condition), Value::Array(vec![item]))
    };
    project.do_step(t.rel, if is_condition { "Add condition" } else { "Add action" }, vec![edit])?;
    Ok(at)
}

/// Removes condition or action `item` of rule `index`.
pub fn remove_item(project: &mut Project, t: &Target, index: usize, is_condition: bool, item: usize) -> Result<(), String> {
    project.do_step(t.rel, if is_condition { "Remove condition" } else { "Remove action" }, vec![Edit::remove(mdoc::rule_item_pointer(index, is_condition, item))])
}

/// Replaces condition or action `item` of rule `index` with what its form made. `typing`: one box was typed in, so the edit joins the
/// typing before it.
pub fn replace_item(project: &mut Project, t: &Target, index: usize, is_condition: bool, item: usize, new: Value, typing: bool) -> Result<(), String> {
    let label = format!("Edit {}", if is_condition { "condition" } else { "action" });
    project.apply_form(t.rel, &label, vec![Edit::set(mdoc::rule_item_pointer(index, is_condition, item), new)], typing)
}

/// Moves condition or action `from` of rule `index` to position `to`: one step.
pub fn move_item(project: &mut Project, t: &Target, index: usize, is_condition: bool, from: usize, to: usize) -> Result<(), String> {
    let mut items = mdoc::rule_items(rule(project, t, index)?, is_condition).to_vec();
    let kind = if is_condition { "condition" } else { "action" };
    if from >= items.len() || to >= items.len() {
        return Err(format!("no {kind} {}", from.max(to)));
    }
    if from == to {
        return Ok(());
    }
    let moved = items.remove(from);
    items.insert(to, moved);
    project.do_step(t.rel, &format!("Move {kind}"), vec![Edit::set(mdoc::rule_list_pointer(index, is_condition), Value::Array(items))])
}
