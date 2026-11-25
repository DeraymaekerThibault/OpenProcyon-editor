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
use crate::project::{DocKind, Project, keyed};

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

/// The maps of the mod and of the import, by id, and a new map not saved yet. A mod file is listed as the mod's even when it only
/// changes a retail map.
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
    // A new map is only in the project until the next save: it is listed too, so it can be switched back to.
    for d in &project.docs {
        let id = d.rel.strip_prefix("maps/").and_then(|r| r.strip_suffix(&format!("/{}", doc::MAP_FILE)));
        if let Some(id) = id.filter(|id| !id.contains('/') && !by_id.contains_key(*id)) {
            by_id.insert(id.to_string(), MapEntry { id: id.to_string(), in_mod: true, in_import: false, name: doc::name(&d.edited).map(str::to_string) });
        }
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

pub(super) fn is_whole(project: &Project, rel: &str) -> bool {
    project.doc(rel).is_some_and(|d| d.kind == DocKind::Replaced || d.parent.is_none())
}

/// Whether `which` of the map document can take an added or a deleted entry. A map that is whole in the mod (a new map) always can
/// (`Ok(None)`). A map that extends another can when the list merges by key ([`keyed`], the engine's `authoring::keyed_lists`) and the list below has a unique key on every entry; the key field is returned, an added entry must
/// carry it. Otherwise the error says why: adding or deleting one entry would write the parent's whole list into the mod.
pub(super) fn check_add_delete(project: &Project, rel: &str, which: Array) -> Result<Option<String>, String> {
    if is_whole(project, rel) {
        return Ok(None);
    }
    let d = project.doc(rel).ok_or_else(|| format!("{rel}: not open"))?;
    let (merge, ptr) = (d.own.get("merge"), keyed::list_pointer(which.key()));
    keyed::can_add_delete(&keyed::EngineRule, rel, merge, &ptr, d.base.get(which.key()))?;
    Ok(keyed::key_field(&keyed::EngineRule, rel, merge, &ptr))
}

/// `entry` can be added to `list` under `field`: it has the key and no entry of `list` has the same.
pub(super) fn check_key(entry: &Value, list: &[Value], field: &str, which: Array) -> Result<(), String> {
    let key = keyed::key_text(entry.get(field)).ok_or_else(|| format!("an entry of {} in a map that extends another needs a '{field}' to be told apart", which.key()))?;
    if list.iter().any(|e| keyed::key_text(e.get(field)).as_deref() == Some(key.as_str())) {
        return Err(format!("{} already has an entry with '{field}' {key}", which.key()));
    }
    Ok(())
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

/// A new placement for one entry: any of its position (`pos`, a player's `start`), its `rot` and, for a player, its `forward`.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub which: Array,
    pub index: usize,
    pub pos: Option<[f64; 3]>,
    pub rot: Option<[f64; 9]>,
    pub forward: Option<[f64; 3]>,
}

impl Change {
    pub fn new(which: Array, index: usize) -> Change {
        Change { which, index, pos: None, rot: None, forward: None }
    }
}

/// `to` as the number list an entry already has under `key`: two numbers stay two while z is 0 (a height, T9, makes it three), anything else is three.
fn like(entry: &Value, key: &str, to: [f64; 3]) -> Value {
    let old = entry.get(key).and_then(Value::as_array);
    let two = old.is_some_and(|a| a.len() == 2) && to[2] == 0.0;
    // A component the entry has already, to f32 precision, keeps its own digits: moving z must not rewrite x and y.
    let num = |i: usize| match old.and_then(|a| a.get(i)) {
        Some(o) if o.as_f64().is_some_and(|o| o as f32 == to[i] as f32) => o.clone(),
        _ => crate::map_view::short(to[i] as f32),
    };
    if two { Value::Array(vec![num(0), num(1)]) } else { Value::Array((0..3).map(num).collect()) }
}

/// Places any number of entries (several arrays) in ONE undo step labelled `label`: a group move or turn from the plan view. The
/// numbers are written as the f32's shortest decimals (the engine reads f32). No change at all pushes no step.
pub fn change_entries(project: &mut Project, id: &str, label: &str, changes: &[Change]) -> Result<(), String> {
    let rel = map_rel(id)?;
    let i = open_map(project, id)?;
    let map = &project.docs[i].edited;
    let mut edits = Vec::new();
    for c in changes {
        let entry = doc::array_entries(map, c.which).get(c.index).ok_or_else(|| format!("{rel}: no entry {} in {}", c.index, c.which.key()))?;
        let at = |key: &str| pointer(&[c.which.key(), &c.index.to_string(), key]);
        // A value the entry already has is not written (a step that changes nothing would put a `set` pointer in the file).
        let mut put = |key: &str, v: Value| {
            if entry.get(key) != Some(&v) {
                edits.push(Edit::set(at(key), v));
            }
        };
        if let Some(p) = c.pos {
            put(doc::pos_key(c.which), like(entry, doc::pos_key(c.which), p));
        }
        if let Some(r) = c.rot {
            put("rot", Value::Array(r.iter().map(|&x| crate::map_view::short(x as f32)).collect()));
        }
        if let Some(f) = c.forward {
            put("forward", like(entry, "forward", f));
        }
    }
    if edits.is_empty() {
        return Ok(());
    }
    project.do_step(&rel, label, edits)
}

/// Adds `entry` to the end of `which` as one undo step and returns its index. An entry of an id-bearing array with no `id` gets
/// the next free one ([`doc::next_id`]). Refused on a map that extends another unless the list merges by key ([`check_add_delete`]).
pub fn add_entry(project: &mut Project, id: &str, which: Array, mut entry: Value) -> Result<usize, String> {
    let rel = map_rel(id)?;
    let i = open_map(project, id)?;
    let field = check_add_delete(project, &rel, which)?;
    let map = &project.docs[i].edited;
    if which.layer().is_some() && doc::id(&entry).is_none() {
        doc::set_id(&mut entry, doc::next_id(map, &[]));
    }
    if let Some(field) = field {
        check_key(&entry, doc::array_entries(map, which), &field, which)?;
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

/// Deletes entry `index` of `which` as one undo step and returns it. Refused on a map that extends another unless the list merges by key ([`check_add_delete`]).
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
