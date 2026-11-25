//! Maps as project documents (docs/phase4.md P1, work item T3): the maps of a mod and of the import, a map opened as the game would
//! see it (the engine's overlay of the mod over the import, as ships and weapons are in Phase 3), a new map, and the edits a map
//! editor makes (move, add, delete) as undo steps of [`Project`]. Saving is [`Project::save`]: a map that extends a retail map is
//! written as `extends` plus the changed keys (a move is a `set` pointer), a map of the mod's own is written whole, and every write
//! goes through the guard. Names no map key: that is [`super::doc`].
//!
//! A map is the document `maps/<id>/map.json`; the same id in the mod and in the import is one map (the mod's file extends the
//! import's, in place).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::doc::{self, Array};
use crate::edit::{Edit, pointer};
use crate::project::{DocKind, Project};

/// A map found in the mod or the import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub id: String,
    /// The mod has `maps/<id>/map.json`.
    pub in_mod: bool,
    /// The import has it.
    pub in_import: bool,
    /// The map's `name`, from the mod's file, else the import's; `None` when neither has one or the file cannot be read.
    pub name: Option<String>,
}

/// The path of a map's document inside the mod. `id` must be a plain folder name: letters, digits, `_`, `-` and `.`, not starting
/// with a dot.
pub fn map_rel(id: &str) -> Result<String, String> {
    let ok = !id.is_empty() && !id.starts_with('.') && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok { Ok(format!("maps/{id}/{}", doc::MAP_FILE)) } else { Err(format!("'{id}' is not a map id (letters, digits, _ - and . only)")) }
}

fn read_name(path: &Path) -> Option<String> {
    let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    doc::name(&v).map(str::to_string)
}

/// The ids of the folders under `root/maps` that hold a map file.
fn scan(root: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root.join("maps")) else { return vec![] };
    rd.flatten().filter(|e| e.path().join(doc::MAP_FILE).is_file()).filter_map(|e| e.file_name().into_string().ok()).collect()
}

/// The maps of the mod and of the import, by id. A mod file is listed as the mod's even when it only changes a retail map.
pub fn list_maps(project: &Project) -> Vec<MapEntry> {
    let mut by_id: BTreeMap<String, MapEntry> = BTreeMap::new();
    let mut note = |root: &Path, in_mod: bool| {
        for id in scan(root) {
            let name = read_name(&root.join("maps").join(&id).join(doc::MAP_FILE));
            let e = by_id.entry(id.clone()).or_insert(MapEntry { id, in_mod: false, in_import: false, name: None });
            if in_mod {
                e.in_mod = true;
            } else {
                e.in_import = true;
            }
            e.name = e.name.take().or(name);
        }
    };
    // The mod first, so its name wins over the retail one.
    note(&project.dir, true);
    if let Some(import) = &project.import {
        note(import, false);
    }
    by_id.into_values().collect()
}

/// Opens the map `id` (the mod's file laid over the import's, as the game sees it) and returns its index in `project.docs`. Errors
/// when neither the mod nor the import has it.
pub fn open_map(project: &mut Project, id: &str) -> Result<usize, String> {
    let rel = map_rel(id)?;
    if project.doc(&rel).is_none() && !project.dir.join(&rel).is_file() && !project.import.as_ref().is_some_and(|i| i.join(&rel).is_file()) {
        return Err(format!("no map '{id}' in the mod or the import"));
    }
    project.open_doc(&rel)
}

/// The map as the game would see it (the merged view), when it is open.
pub fn merged_view<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    project.doc(&map_rel(id).ok()?).map(|d| &d.edited)
}

/// Starts a new map `id` of the mod's own ([`doc::new_map`]); it is written, whole, by the next [`Project::save`], which also makes
/// the folder. `mod.json` is left alone. Errors when the mod or the import has a map of that id (open it instead: a retail map
/// edited here is saved as a diff).
pub fn new_map(project: &mut Project, id: &str) -> Result<usize, String> {
    let rel = map_rel(id)?;
    if project.import.as_ref().is_some_and(|i| i.join(&rel).is_file()) {
        return Err(format!("'{id}' is a retail map: open it (changes are saved as a diff in the mod) or pick another id"));
    }
    project.open_new_doc(&rel, None, doc::new_map(id)).map_err(|e| if e.ends_with("exists already") { format!("the mod has a map '{id}' already: open it") } else { e })
}

fn is_whole(project: &Project, rel: &str) -> bool {
    project.doc(rel).is_some_and(|d| d.kind == DocKind::Replaced || d.parent.is_none())
}

/// Why `which` of the map document cannot take an added or a deleted entry, or `Ok`. A map that is whole in the mod (a new map)
/// can; a map that extends another cannot: the engine this editor is built on replaces a list whole, so adding or deleting one
/// entry would restate the parent map's content in the mod. PROVISIONAL until keyed lists (docs/phase4.md requirement 1) land.
fn check_add_delete(project: &Project, rel: &str, which: Array) -> Result<(), String> {
    if is_whole(project, rel) {
        return Ok(());
    }
    Err(format!(
        "{rel} extends another map, and adding or deleting one entry of {} would write the whole list into the mod (the engine cannot merge lists by key yet); moving entries is fine",
        which.key()
    ))
}

fn entry_label(map: &Value, which: Array, index: usize) -> String {
    match doc::array_entries(map, which).get(index).and_then(doc::id) {
        Some(id) => format!("{} {id}", which.key()),
        None => format!("{} #{index}", which.key()),
    }
}

/// Moves entry `index` of `which` to `to` as one undo step (a `set` of its `pos`, or a player's `start`, in the saved diff). A
/// position of two numbers stays two numbers.
pub fn move_entry(project: &mut Project, id: &str, which: Array, index: usize, to: [f64; 3]) -> Result<(), String> {
    let rel = map_rel(id)?;
    let i = open_map(project, id)?;
    let map = &project.docs[i].edited;
    let entry = doc::array_entries(map, which).get(index).ok_or_else(|| format!("{rel}: no entry {index} in {}", which.key()))?;
    let two = entry.get(doc::pos_key(which)).and_then(Value::as_array).is_some_and(|a| a.len() == 2);
    let value = if two { serde_json::json!([to[0], to[1]]) } else { serde_json::json!(to) };
    let label = format!("Move {}", entry_label(map, which, index));
    let ptr = pointer(&[which.key(), &index.to_string(), doc::pos_key(which)]);
    project.do_step(&rel, &label, vec![Edit::set(ptr, value)])
}

/// Adds `entry` to the end of `which` as one undo step and returns its index. An entry of an id-bearing array with no `id` gets
/// the next free one ([`doc::next_id`]). Refused on a map that extends another (see the module doc).
pub fn add_entry(project: &mut Project, id: &str, which: Array, mut entry: Value) -> Result<usize, String> {
    let rel = map_rel(id)?;
    let i = open_map(project, id)?;
    check_add_delete(project, &rel, which)?;
    let map = &project.docs[i].edited;
    if which.layer().is_some() && doc::id(&entry).is_none() {
        doc::set_id(&mut entry, doc::next_id(map, &[]));
    }
    let at = doc::array_entries(map, which).len();
    let label = format!("Add {}", entry_label(&serde_json::json!({which.key(): [entry.clone()]}), which, 0));
    let edit = if map.get(which.key()).is_some_and(Value::is_array) {
        Edit::set(pointer(&[which.key(), &at.to_string()]), entry)
    } else {
        Edit::set(pointer(&[which.key()]), Value::Array(vec![entry]))
    };
    project.do_step(&rel, &label, vec![edit])?;
    Ok(at)
}

/// Deletes entry `index` of `which` as one undo step and returns it. Refused on a map that extends another (see the module doc).
pub fn delete_entry(project: &mut Project, id: &str, which: Array, index: usize) -> Result<Value, String> {
    let rel = map_rel(id)?;
    let i = open_map(project, id)?;
    check_add_delete(project, &rel, which)?;
    let map = &project.docs[i].edited;
    let entry = doc::array_entries(map, which).get(index).cloned().ok_or_else(|| format!("{rel}: no entry {index} in {}", which.key()))?;
    let label = format!("Delete {}", entry_label(map, which, index));
    project.do_step(&rel, &label, vec![Edit::remove(pointer(&[which.key(), &index.to_string()]))])?;
    Ok(entry)
}
