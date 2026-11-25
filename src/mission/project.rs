//! Missions as project documents (docs/phase5.md P1, work item T1): the missions of the mod and of the import, one opened as the game
//! would see it (the engine's overlay of the mod over the import, as maps are in `map::project`), a new mission, and the add and
//! delete of list entries as undo steps of [`Project`]. Saving is [`Project::save`]: a mission that extends a retail one is written as
//! `extends` plus the changed keys, with the keyed lists (`ships` by `id`, `rules` by `name`, `points` by `Name`, [`keyed`]) written
//! entry by entry; a mission of the mod's own is written whole. Names no mission key: that is [`super::doc`].
//!
//! A mission is the map folder `maps/<id>/` with `mission.json`, `media.json` and `map.json` (docs/missions.md section 1); the same id
//! in the mod and the import is one mission (the mod's files extend the import's, in place). The mod lists its missions in `mod.json`
//! `missions` (the campaign order, T12); a new mission is appended there.
//!
//! PROVISIONAL: the retail ids (`mission01`..`mission13`, `tutorial`, `basic_tutorial`, docs/missions.md section 1), that a mission
//! needs the three files, and the content of a new mission; the keyed lists are [`keyed`]'s PROVISIONAL copy of the engine's rule.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::doc::{self, MissionList};
use crate::edit::{Edit, pointer};
use crate::map::project::map_rel;
use crate::project::{DocKind, Project, keyed};

const MOD_FILE: &str = "mod.json";
use super::doc::MOD_MISSIONS;

/// Whether `id` is a retail mission: `missionNN` (two digits), `tutorial` or `basic_tutorial`. PROVISIONAL (docs/missions.md section 1).
pub fn is_retail_id(id: &str) -> bool {
    id.strip_prefix("mission").is_some_and(|n| n.len() == 2 && n.bytes().all(|b| b.is_ascii_digit())) || matches!(id, "tutorial" | "basic_tutorial")
}

/// Checks `id` is a plain folder name (letters, digits, `_`, `-`, `.`; the map folder rule).
pub fn check_id(id: &str) -> Result<(), String> {
    map_rel(id).map(|_| ()).map_err(|_| format!("'{id}' is not a mission id (letters, digits, _ - and . only)"))
}

/// The path of a mission's `mission.json` inside the mod.
pub fn mission_rel(id: &str) -> Result<String, String> {
    check_id(id).map(|_| format!("maps/{id}/{}", doc::MISSION_FILE))
}

/// The path of a mission's `media.json` inside the mod.
pub fn media_rel(id: &str) -> Result<String, String> {
    check_id(id).map(|_| format!("maps/{id}/{}", doc::MEDIA_FILE))
}

/// Where a mission comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The mod's own mission.
    Mod,
    /// A retail mission the mod does not touch.
    Retail,
    /// A retail mission the mod changes ("retail, changed by the mod"); the mod's files are saved as a diff.
    RetailChanged,
}

/// A mission found in the mod or the import, or started and not saved yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionEntry {
    pub id: String,
    /// The mod has `maps/<id>/mission.json` (or it is new and unsaved).
    pub in_mod: bool,
    /// The import has it.
    pub in_import: bool,
    pub origin: Origin,
    /// The map's `name` (as the game lists it), `None` when the map has none or cannot be read.
    pub name: Option<String>,
    /// The id is in `mod.json` `missions` (the campaign order).
    pub listed: bool,
}

/// The three documents of one mission, as indices in `project.docs`. `media` and `map` are `None` when the mission has no such file
/// (the game needs both; the problems list says so).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionDocs {
    pub id: String,
    pub mission: usize,
    pub media: Option<usize>,
    pub map: Option<usize>,
}

/// The file `rel` exists in the mod, in the import, or as an open document that holds something (a mission started here).
fn exists(project: &Project, rel: &str) -> bool {
    project.doc(rel).is_some_and(|d| !d.edited.is_null()) || project.dir.join(rel).is_file() || project.import.as_ref().is_some_and(|i| i.join(rel).is_file())
}

fn scan(root: &std::path::Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root.join("maps")) else { return vec![] };
    rd.flatten().filter(|e| e.path().join(doc::MISSION_FILE).is_file()).filter_map(|e| e.file_name().into_string().ok()).collect()
}

fn listed_in_mod_json(project: &Project) -> Vec<String> {
    let list = project.doc(MOD_FILE).and_then(|d| d.edited.get(MOD_MISSIONS)).and_then(Value::as_array);
    list.into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect()
}

/// The missions of the mod and of the import, by id, and a new one not saved yet. A mod file is listed as the mod's even when it
/// only changes a retail mission ([`Origin::RetailChanged`]).
pub fn list_missions(project: &Project) -> Vec<MissionEntry> {
    let mut found: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for id in scan(&project.dir) {
        found.entry(id).or_default().0 = true;
    }
    if let Some(import) = &project.import {
        for id in scan(import) {
            found.entry(id).or_default().1 = true;
        }
    }
    for d in &project.docs {
        let id = d.rel.strip_prefix("maps/").and_then(|r| r.strip_suffix(&format!("/{}", doc::MISSION_FILE)));
        if let Some(id) = id.filter(|id| !id.contains('/') && !d.edited.is_null()) {
            found.entry(id.to_string()).or_default().0 = true;
        }
    }
    let listed = listed_in_mod_json(project);
    found
        .into_iter()
        .map(|(id, (in_mod, in_import))| {
            let origin = match (in_import || is_retail_id(&id), in_mod) {
                (true, true) => Origin::RetailChanged,
                (true, false) => Origin::Retail,
                (false, _) => Origin::Mod,
            };
            let name = map_name(project, &id);
            let listed = listed.contains(&id);
            MissionEntry { id, in_mod, in_import, origin, name, listed }
        })
        .collect()
}

/// The map's `name`: the open document's, else the overlay's file.
fn map_name(project: &Project, id: &str) -> Option<String> {
    let rel = map_rel(id).ok()?;
    let from_doc = project.doc(&rel).filter(|d| !d.edited.is_null()).map(|d| d.edited.clone());
    let v = from_doc.or_else(|| openprocyon::authoring::read_merged(&project.root, &rel).ok())?;
    crate::map::doc::name(&v).map(str::to_string)
}

/// Opens the mission `id`: `mission.json`, `media.json` and `map.json`, each the mod's file laid over the import's, as the game sees
/// them. Errors when neither the mod nor the import has a `mission.json` for it.
pub fn open_mission(project: &mut Project, id: &str) -> Result<MissionDocs, String> {
    let (m, md, mp) = (mission_rel(id)?, media_rel(id)?, map_rel(id)?);
    if !exists(project, &m) {
        return Err(format!("no mission '{id}' in the mod or the import"));
    }
    let mission = project.open_doc(&m)?;
    let media = if exists(project, &md) { Some(project.open_doc(&md)?) } else { None };
    let map = if exists(project, &mp) { Some(project.open_doc(&mp)?) } else { None };
    Ok(MissionDocs { id: id.to_string(), mission, media, map })
}

fn view(project: &Project, rel: Result<String, String>) -> Option<&Value> {
    project.doc(&rel.ok()?).map(|d| &d.edited).filter(|v| !v.is_null())
}

/// The mission as the game would see it (the merged view), when it is open.
pub fn merged_mission<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    view(project, mission_rel(id))
}

/// The mission's `media.json` as the game would see it, when it is open.
pub fn merged_media<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    view(project, media_rel(id))
}

/// The mission's `map.json` as the game would see it, when it is open.
pub fn merged_map<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    view(project, map_rel(id))
}

/// The `mission.json` a new mission starts as: the keys the game stops without (`ships`, `rules`) and `players` (docs/missions.md
/// section 3). PROVISIONAL.
pub fn new_mission_json() -> Value {
    json!({doc::KEY_PLAYERS: [], doc::KEY_SHIPS: [], doc::KEY_RULES: []})
}

/// Starts a new mission `id` of the mod's own as ONE undo step: `maps/<id>/mission.json` ([`new_mission_json`]), `media.json` (the
/// three empty tables), `map.json` and the id appended to `mod.json` `missions`. The files are written, whole, by the next
/// [`Project::save`]. `map`:
///
/// - `None`: a minimal map ([`crate::map::doc::new_map`]) when the mod has no `maps/<id>/map.json` yet, else that map is kept.
/// - `Some(other)`: the new map `extends` the map `other` (of the mod or the import), saved as `extends` plus its own `id` and `name`
///   (the mission id; a file with nothing of its own is not written).
///
/// Refused (nothing changes) for a retail id, an id the mod or the import has a mission or media under, an id the import has a map
/// under, a map to start from that does not exist, a mod with no `mod.json` (make the mod first) or a `missions` that is not a list.
pub fn new_mission(project: &mut Project, id: &str, map: Option<&str>) -> Result<MissionDocs, String> {
    let (m, md, mp) = (mission_rel(id)?, media_rel(id)?, map_rel(id)?);
    if is_retail_id(id) {
        return Err(format!("'{id}' is a retail mission id: open it (changes are saved as a diff in the mod) or pick another id"));
    }
    if exists(project, &m) || exists(project, &md) {
        return Err(format!("the mod or the import has a mission '{id}' already: open it"));
    }
    if project.import.as_ref().is_some_and(|i| i.join(&mp).is_file()) {
        return Err(format!("'{id}' is a retail map: pick another id (a mission of its own can extend it)"));
    }
    let parent = match map {
        Some(other) => {
            let other_rel = map_rel(other)?;
            if other == id || !exists(project, &other_rel) {
                return Err(format!("no map '{other}' in the mod or the import to start from"));
            }
            if exists(project, &mp) {
                return Err(format!("the mod has a map '{id}' already: leave the map choice empty to use it"));
            }
            Some(other)
        }
        None => None,
    };
    // mod.json: the campaign list.
    if project.doc(MOD_FILE).is_none() && !project.dir.join(MOD_FILE).is_file() {
        return Err("the mod has no mod.json: make the mod first".into());
    }
    project.open_doc(MOD_FILE)?;
    let meta = &project.doc(MOD_FILE).expect("opened").edited;
    if !meta.is_object() {
        return Err("mod.json is not an object: fix it first".into());
    }
    let missions_edit = match meta.get(MOD_MISSIONS) {
        None => Some(Edit::set(pointer(&[MOD_MISSIONS]), json!([id]))),
        Some(Value::Array(a)) if a.iter().any(|v| v.as_str() == Some(id)) => None,
        Some(Value::Array(a)) => Some(Edit::set(pointer(&[MOD_MISSIONS, &a.len().to_string()]), json!(id))),
        Some(_) => return Err("mod.json 'missions' is not a list: fix it first".into()),
    };

    // The documents start empty and one group step fills them, so Undo takes the whole mission back.
    let before = project.docs.len();
    let result = (|| -> Result<MissionDocs, String> {
        let mission = project.open_new_doc(&m, None, Value::Null)?;
        let media = project.open_new_doc(&md, None, Value::Null)?;
        let mut steps = vec![(m.clone(), vec![Edit::set("", new_mission_json())]), (md.clone(), vec![Edit::set("", doc::new_media())])];
        let map_doc = match parent {
            Some(other) => {
                let i = project.open_new_doc(&mp, Some(other), Value::Null)?;
                // `id` and `name` of the new map: the file then has something of its own to say (a bare `extends` is not written), and the
                // mission is not listed under the name of the map it starts from.
                let mut base = std::mem::replace(&mut project.docs[i].edited, Value::Null);
                if let Some(o) = base.as_object_mut() {
                    o.insert("id".into(), json!(id));
                    o.insert("name".into(), json!(id));
                }
                steps.push((mp.clone(), vec![Edit::set("", base)]));
                Some(i)
            }
            None if !exists(project, &mp) => {
                let i = project.open_new_doc(&mp, None, Value::Null)?;
                steps.push((mp.clone(), vec![Edit::set("", crate::map::doc::new_map(id))]));
                Some(i)
            }
            None => project.open_doc(&mp).ok(),
        };
        steps.push((MOD_FILE.to_string(), missions_edit.into_iter().collect()));
        project.do_group(&format!("New mission {id}"), steps)?;
        Ok(MissionDocs { id: id.to_string(), mission, media: Some(media), map: map_doc })
    })();
    if result.is_err() {
        project.docs.truncate(before);
    }
    result
}

// ---- list entries on an extending mission ----

fn whole(project: &Project, rel: &str) -> bool {
    project.doc(rel).is_some_and(|d| d.kind == DocKind::Replaced || d.parent.is_none())
}

/// Whether `list` of mission `id` can take an added or a deleted entry. A mission that is whole in the mod (a new one) always can
/// (`Ok(None)`). One that extends another can when the list merges by key ([`keyed`]: `ships` by `id`, `rules` by `name`, `points`
/// by `Name`, PROVISIONAL, held to the engine by `tests/keyed_engine.rs`) and every entry below has a unique key; the key field is
/// returned and an added entry must carry it. Otherwise the error says why (`tasks`, `environment.nebulae`, `environment.animals`,
/// `map_text`, `players`, `alliances` are not keyed by the engine: a change restates the whole list, which the form says,
/// docs/phase5.md requirement 2).
pub fn check_add_delete(project: &mut Project, id: &str, list: MissionList) -> Result<Option<String>, String> {
    let rel = mission_rel(id)?;
    project.open_doc(&rel)?;
    if whole(project, &rel) {
        return Ok(None);
    }
    let d = project.doc(&rel).expect("opened");
    let ptr = doc::list_pointer(list);
    keyed::can_add_delete(&keyed::EngineRule, &rel, d.own.get("merge"), &ptr, d.base.pointer(&ptr))?;
    Ok(keyed::key_field(&keyed::EngineRule, &rel, d.own.get("merge"), &ptr))
}

/// Whether a key of an object table (`groups`, `volumes`, `paths`, `point_sets`, `speech`...) can be removed from the document `rel`:
/// only in a file that is whole. A file that extends another has no way to say "remove this key" (docs/phase5.md P1), so a key can
/// be added and changed there, not removed.
pub fn check_remove_key(project: &Project, rel: &str) -> Result<(), String> {
    if whole(project, rel) { Ok(()) } else { Err(format!("{rel} extends another file, which has no way to remove a key: set it to something else, or leave it")) }
}

fn entry_label(prefix: &str, list: MissionList, entry: &Value, index: usize) -> String {
    let key = doc::list_key(list).and_then(|k| keyed::key_text(entry.get(k)));
    let noun = doc::list_noun(list);
    match key {
        Some(k) => format!("{prefix} {noun} {}", k.trim_matches('"')),
        None => format!("{prefix} {noun} #{index}"),
    }
}

/// Adds `entry` to the end of `list` of mission `id` as one undo step and returns its index. Refused on an extending mission unless
/// the list merges by key ([`check_add_delete`]), and for a keyed list when the entry has no key or the list has it already.
pub fn add_entry(project: &mut Project, id: &str, list: MissionList, entry: Value) -> Result<usize, String> {
    let rel = mission_rel(id)?;
    let field = check_add_delete(project, id, list)?;
    let mission = &project.doc(&rel).expect("opened").edited;
    let entries = doc::list_entries(mission, list);
    let name = doc::list_path(list).join("/");
    if let Some(field) = field {
        let key = keyed::key_text(entry.get(&field)).ok_or_else(|| format!("an entry of {name} in a mission that extends another needs a '{field}' to be told apart"))?;
        if entries.iter().any(|e| keyed::key_text(e.get(&field)).as_deref() == Some(key.as_str())) {
            return Err(format!("{name} already has an entry with '{field}' {key}"));
        }
    }
    let at = entries.len();
    let label = entry_label("Add", list, &entry, at);
    let ptr = doc::list_pointer(list);
    let edit = if mission.pointer(&ptr).is_some_and(Value::is_array) { Edit::set(format!("{ptr}/{at}"), entry) } else { Edit::set(ptr, Value::Array(vec![entry])) };
    project.do_step(&rel, &label, vec![edit])?;
    Ok(at)
}

/// Deletes entry `index` of `list` of mission `id` as one undo step and returns it. Refused on an extending mission unless the list
/// merges by key ([`check_add_delete`]).
pub fn delete_entry(project: &mut Project, id: &str, list: MissionList, index: usize) -> Result<Value, String> {
    let rel = mission_rel(id)?;
    check_add_delete(project, id, list)?;
    let mission = &project.doc(&rel).expect("opened").edited;
    let entry = doc::list_entries(mission, list).get(index).cloned().ok_or_else(|| format!("{rel}: no entry {index} in {}", doc::list_path(list).join("/")))?;
    let label = entry_label("Delete", list, &entry, index);
    project.do_step(&rel, &label, vec![Edit::remove(format!("{}/{index}", doc::list_pointer(list)))])?;
    Ok(entry)
}
