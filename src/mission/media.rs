//! The tables of a `media.json` (speech events, end messages, ship names) edited on a project document (docs/phase5.md T2). Lifted from
//! Phase 4's `map::skirmish`, which now only calls these. One undo step each (typing in one box joins the step before it); the
//! document is named by its path in the project (`rel`) and must be open or openable (the caller makes a new `media.json` first).
//! The keys and the starting value of an entry are [`super::doc`]'s. A speech entry's own fields (`text`, `file`, `duration`) are the
//! caller's: `field` of [`set`] says which. Renaming an entry and the rules that name it is [`super::refs`] plus the caller's group step.

use serde_json::Value;

use super::doc as mdoc;
use super::doc::MediaTable;
use crate::edit::{Edit, pointer};
use crate::project::Project;

/// The entries of one table, in the file's order (empty when missing).
pub fn entries(media: &Value, table: MediaTable) -> Vec<(&str, &Value)> {
    mdoc::media_table(media, table).map(|o| o.iter().map(|(k, v)| (k.as_str(), v)).collect()).unwrap_or_default()
}

/// Adds an entry named `key` to a table of the document `rel`. Refuses an empty or repeated key (the name is trimmed).
pub fn add(project: &mut Project, rel: &str, table: MediaTable, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("name the entry first".into());
    }
    project.open_doc(rel)?;
    if project.doc(rel).is_some_and(|d| entries(&d.edited, table).iter().any(|(k, _)| *k == key)) {
        return Err(format!("{} already has '{key}'", table.key()));
    }
    project.do_step(rel, "Add media entry", vec![Edit::set(mdoc::media_entry_pointer(table, key), mdoc::media_new_entry(table))])
}

/// Removes an entry.
pub fn remove(project: &mut Project, rel: &str, table: MediaTable, key: &str) -> Result<(), String> {
    project.do_step(rel, "Remove media entry", vec![Edit::remove(mdoc::media_entry_pointer(table, key))])
}

/// Sets a message or a ship name (`field` `None`) or one field of a speech entry (`text`, `file`, `duration`). `value` `None` removes
/// it. Typing joins one step.
pub fn set(project: &mut Project, rel: &str, table: MediaTable, key: &str, field: Option<&str>, value: Option<Value>) -> Result<(), String> {
    let tokens: Vec<&str> = [Some(table.key()), Some(key), field].into_iter().flatten().collect();
    let at = pointer(&tokens);
    project.apply_form(rel, &format!("Edit {}", table.key()), vec![value.map_or_else(|| Edit::remove(at.clone()), |v| Edit::set(at.clone(), v))], true)
}
