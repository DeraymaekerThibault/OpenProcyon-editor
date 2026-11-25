//! Groups of a mission (docs/phase5.md P2, work item T6): `groups` is `{Name: [ids]}`, the names a rule can address. A member is the
//! `id` of a mission ship, of a scenery object of the map (`objects[]` with an `id`) or of an animal (`environment.animals[]`); the
//! three share one id space with the map's mines (Phase 4 requirement 10, so [`next_id`] looks at all of them, as `map::players`'s
//! allocation does through `map::doc::next_id`). Every operation here is ONE undo step of the mission document. Names no key: the
//! keys are [`super::doc`], the reference walk [`super::refs`].
//!
//! Refusals (the message says why): an empty or duplicate name, a comma in a name (`Group,Unit` would split it), a member that is
//! not a ship, object or animal id, a delete of a group a rule still names, and a delete or rename in a mission that extends another
//! (an extending file has no way to remove a key, `project::check_remove_key`).
//!
//! PROVISIONAL: that an animal's id is its `id` key (the engine's docs say animals take part in `groups` but no schema lists the key,
//! engine requirement 11), and that the first id handed out is 1.

use std::collections::BTreeSet;

use bevy_egui::egui;
use serde_json::{Value, json};

use super::doc as mdoc;
use super::project::{check_remove_key, mission_rel};
use super::refs::{self, FieldIndex};
use crate::edit::Edit;
use crate::form::model::{map_remove, rename_key};
use crate::map::doc as mapdoc;
use crate::project::Project;

/// What a group member is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberKind {
    Ship,
    Object,
    Animal,
}

/// An id a group can list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: i64,
    pub kind: MemberKind,
    /// What the picker shows: the ship's name, the object's type, the animal's group.
    pub label: String,
}

/// Every id a group can list, in the order ships, objects, animals. `map` is the mission's `map.json` when it has one.
pub fn candidates(mission: &Value, map: Option<&Value>) -> Vec<Candidate> {
    let mut out = vec![];
    for s in mdoc::ships(mission) {
        if let Some(id) = mdoc::ship_id(s) {
            out.push(Candidate { id, kind: MemberKind::Ship, label: mdoc::ship_name(s).unwrap_or("(unnamed ship)").to_string() });
        }
    }
    for o in map.map_or(&[][..], mapdoc::objects) {
        if let Some(id) = mapdoc::id(o) {
            out.push(Candidate { id, kind: MemberKind::Object, label: o.get("type").and_then(Value::as_str).unwrap_or("object").to_string() });
        }
    }
    for a in mdoc::animals(mission) {
        if let Some(id) = mdoc::animal_id(a) {
            out.push(Candidate { id, kind: MemberKind::Animal, label: mdoc::animal_group(a).unwrap_or("animal").to_string() });
        }
    }
    out
}

/// The ids the mission and its map use: ships, animals, and the map's objects, ships and mines.
pub fn used_ids(mission: &Value, map: Option<&Value>) -> BTreeSet<i64> {
    let mut ids: BTreeSet<i64> = map.map(mapdoc::used_ids).unwrap_or_default();
    ids.extend(mdoc::ships(mission).iter().filter_map(mdoc::ship_id));
    ids.extend(mdoc::animals(mission).iter().filter_map(mdoc::animal_id));
    ids
}

/// The smallest id (from 1) that no ship, object, mine or animal uses: what a new ship gets.
pub fn next_id(mission: &Value, map: Option<&Value>) -> i64 {
    let used = used_ids(mission, map);
    (1..).find(|i| !used.contains(i)).expect("an i64 range has a free id")
}

/// Ids used by more than one of the mission's ships, animals and the map's objects, ships and mines, sorted (a group would address
/// all of them).
pub fn duplicate_ids(mission: &Value, map: Option<&Value>) -> Vec<i64> {
    let mut seen = BTreeSet::new();
    let mut dup = BTreeSet::new();
    let map_ids = map.into_iter().flat_map(|m| mapdoc::Layer::ALL.iter().flat_map(|&l| mapdoc::entries(m, l)).filter_map(mapdoc::id));
    let all = map_ids.chain(mdoc::ships(mission).iter().filter_map(mdoc::ship_id)).chain(mdoc::animals(mission).iter().filter_map(mdoc::animal_id));
    for id in all {
        if !seen.insert(id) {
            dup.insert(id);
        }
    }
    dup.into_iter().collect()
}

/// The names of the groups, in file order.
pub fn names(mission: &Value) -> Vec<&str> {
    mdoc::groups(mission).map(|g| g.keys().map(String::as_str).collect()).unwrap_or_default()
}

/// Whether `name` is usable as the name of a new group.
pub fn check_name(mission: &Value, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("a group needs a name".into());
    }
    if name.contains(',') {
        return Err("a group name cannot hold a comma (a rule writes Group,Unit)".into());
    }
    if mdoc::groups(mission).is_some_and(|g| g.contains_key(name)) {
        return Err(format!("there is a group '{name}' already"));
    }
    Ok(())
}

/// The first free "Group N".
pub fn unique_group_name(mission: &Value) -> String {
    let taken = names(mission);
    (1..).map(|n| format!("Group {n}")).find(|c| !taken.contains(&c.as_str())).expect("a free name exists")
}

fn mission<'a>(project: &'a Project, id: &str) -> Result<(&'a Value, String), String> {
    let rel = mission_rel(id)?;
    let v = project.doc(&rel).map(|d| &d.edited).filter(|v| !v.is_null()).ok_or_else(|| format!("mission '{id}' is not open"))?;
    Ok((v, rel))
}

fn map_view<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    super::project::merged_map(project, id)
}

/// Adds an empty group `name` as one undo step. Returns the name as stored (trimmed).
pub fn add_group(project: &mut Project, id: &str, name: &str) -> Result<String, String> {
    let (m, rel) = mission(project, id)?;
    let name = name.trim().to_string();
    check_name(m, &name)?;
    let edit = if m.get(mdoc::KEY_GROUPS).is_some_and(Value::is_object) {
        Edit::set(mdoc::group_pointer(&name), json!([]))
    } else {
        Edit::set(mdoc::groups_pointer(), json!({ name.clone(): [] }))
    };
    project.do_step(&rel, &format!("Add group {name}"), vec![edit])?;
    Ok(name)
}

/// Deletes group `name` as one undo step. Refused while a rule or task still names it, and in a mission that extends another.
pub fn delete_group(project: &mut Project, id: &str, name: &str) -> Result<(), String> {
    let (m, rel) = mission(project, id)?;
    if !names(m).contains(&name) {
        return Err(format!("no group '{name}'"));
    }
    check_remove_key(project, &rel)?;
    let n = refs::references_of(m, "group", name).len();
    if n > 0 {
        return Err(format!("{n} rule field(s) still name the group '{name}': rename or change them first"));
    }
    let edits = map_remove(&mdoc::groups_pointer(), m.get(mdoc::KEY_GROUPS).expect("has groups"), name);
    project.do_step(&rel, &format!("Delete group {name}"), edits)
}

/// Renames group `old` to `new` as one undo step: the key (in place, keeping the order) and every rule and task field that names it.
/// Returns the number of rule fields that followed. Refused for a taken or bad name and in a mission that extends another.
pub fn rename_group(project: &mut Project, id: &str, old: &str, new: &str) -> Result<usize, String> {
    let (m, rel) = mission(project, id)?;
    let new = new.trim();
    if !names(m).contains(&old) {
        return Err(format!("no group '{old}'"));
    }
    if new == old {
        return Ok(0);
    }
    check_name(m, new)?;
    check_remove_key(project, &rel)?;
    let mut edits = rename_key(&mdoc::groups_pointer(), m.get(mdoc::KEY_GROUPS).expect("has groups"), old, new);
    let followed = refs::rename_edits(m, FieldIndex::engine(), "group", old, new)?;
    let n = followed.len();
    edits.extend(followed);
    project.do_step(&rel, &format!("Rename group {old}"), edits)?;
    Ok(n)
}

/// Adds the id `member` (a ship, object or animal of the mission) to group `name` as one undo step.
pub fn add_member(project: &mut Project, id: &str, name: &str, member: i64) -> Result<(), String> {
    let (m, rel) = mission(project, id)?;
    if !names(m).contains(&name) {
        return Err(format!("no group '{name}'"));
    }
    if !candidates(m, map_view(project, id)).iter().any(|c| c.id == member) {
        return Err(format!("{member} is not the id of a ship, object or animal of this mission"));
    }
    if mdoc::group_members(m, name).contains(&member) {
        return Err(format!("{member} is in '{name}' already"));
    }
    let mut list = mdoc::group_entries(m, name).to_vec();
    list.push(json!(member));
    project.do_step(&rel, &format!("Add {member} to group {name}"), vec![Edit::set(mdoc::group_pointer(name), Value::Array(list))])
}

/// Removes the id `member` from group `name` as one undo step.
pub fn remove_member(project: &mut Project, id: &str, name: &str, member: i64) -> Result<(), String> {
    let (m, rel) = mission(project, id)?;
    if !mdoc::group_members(m, name).contains(&member) {
        return Err(format!("{member} is not in '{name}'"));
    }
    let list: Vec<Value> = mdoc::group_entries(m, name).iter().filter(|v| v.as_i64() != Some(member)).cloned().collect();
    project.do_step(&rel, &format!("Remove {member} from group {name}"), vec![Edit::set(mdoc::group_pointer(name), Value::Array(list))])
}

/// The edits that take `member` out of every group that lists it (for a ship that is deleted, in the same step).
pub fn drop_member_edits(mission: &Value, member: i64) -> Vec<Edit> {
    names(mission)
        .into_iter()
        .filter(|n| mdoc::group_members(mission, n).contains(&member))
        .map(|n| {
            let list: Vec<Value> = mdoc::group_entries(mission, n).iter().filter(|v| v.as_i64() != Some(member)).cloned().collect();
            Edit::set(mdoc::group_pointer(n), Value::Array(list))
        })
        .collect()
}

// ---- the form ----

enum Op {
    Rename(String, String),
    Delete(String),
    Remove(String, i64),
    Add(String, i64),
    New(String),
}

/// Draws the groups of mission `id`. The error of an operation that was refused is put in `error` (the caller shows it).
pub fn show(ui: &mut egui::Ui, project: &mut Project, id: &str, error: &mut Option<String>) {
    let Ok((m, _)) = mission(project, id) else { return };
    let map = map_view(project, id);
    let cands = candidates(m, map);
    let dup = duplicate_ids(m, map);
    let members: Vec<(String, Vec<i64>)> = names(m).into_iter().map(|n| (n.to_string(), mdoc::group_members(m, n))).collect();
    let unique = unique_group_name(m);
    let mut ops: Vec<Op> = vec![];
    if !dup.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(220, 150, 60), format!("Ids used more than once (a group would address all of them): {dup:?}"));
    }
    for (name, ids) in &members {
        egui::CollapsingHeader::new(format!("{name} ({})", ids.len())).id_salt(("group", name)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(new) = super::ships::text_edit(ui, egui::Id::new(("group_rename", name)), name, 160.0) {
                    ops.push(Op::Rename(name.clone(), new));
                }
                if ui.button("Delete group").clicked() {
                    ops.push(Op::Delete(name.clone()));
                }
            });
            for member in ids {
                ui.horizontal(|ui| {
                    let label = cands.iter().find(|c| c.id == *member).map_or_else(|| format!("{member} (no such id)"), |c| format!("{member}  {:?}  {}", c.kind, c.label));
                    ui.label(label);
                    if ui.small_button("Remove").clicked() {
                        ops.push(Op::Remove(name.clone(), *member));
                    }
                });
            }
            egui::ComboBox::from_id_salt(("group_add", name)).selected_text("Add member...").show_ui(ui, |ui| {
                for c in cands.iter().filter(|c| !ids.contains(&c.id)) {
                    if ui.selectable_label(false, format!("{}  {:?}  {}", c.id, c.kind, c.label)).clicked() {
                        ops.push(Op::Add(name.clone(), c.id));
                    }
                }
            });
        });
    }
    if ui.button("Add group").clicked() {
        ops.push(Op::New(unique));
    }
    for op in ops {
        let r = match op {
            Op::Rename(old, new) => rename_group(project, id, &old, &new).map(|_| ()),
            Op::Delete(n) => delete_group(project, id, &n),
            Op::Remove(n, m) => remove_member(project, id, &n, m),
            Op::Add(n, m) => add_member(project, id, &n, m),
            Op::New(n) => add_group(project, id, &n).map(|_| ()),
        };
        if let Err(e) = r {
            *error = Some(e);
        }
    }
}
