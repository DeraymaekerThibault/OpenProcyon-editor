//! The "Missions" tab section: players, alliances, the human's start, ships and groups (docs/phase5.md P2, work item T6).
//!
//! The functions are the edits (each ONE undo step of the mission document, through [`Project`]; the refusals come back as `Err`
//! with the reason, and nothing changes); [`show`] is the form that calls them. Which mission is edited is the one the Missions tab's
//! picker selected ([`super::rules_view::selected_mission`]); the section opens its documents. Mission keys are named by [`super::doc`] only; the group half is [`super::groups`].
//!
//! - Players: add, delete (refused while a ship, the start or a rule names the player), rename (every ship `group`, alliance,
//!   `player_start.name` and rule field that names it follows in the same step). `players` and `alliances` are unkeyed lists, so a
//!   mission that extends another refuses add and delete with the project's message ([`check_add_delete`]).
//! - Alliances: a checkbox per pair of players; unchecked pairs are hostile (`eval.rs:44-55` of the engine, not restated here).
//! - `player_start`: the human's player, `start` and `forward`.
//! - Ships `{id, type, group, name, primary, pos, rot}`: the id comes from [`groups::next_id`] (shared with the map's objects, mines and
//!   the animals), the `type` from [`ship_types`], `name` is unique across the mission (the game finds `Group,Unit` by `name` alone),
//!   `primary` is exclusive per owner (one step), a rename of a ship follows the rules' `Group,Unit` references, a delete takes the
//!   ship out of every group and is refused while a rule names the unit.
//!
//! PROVISIONAL: [`ship_types`] scans the `ships/<id>/ship.json` folders of the overlay and the mod (the engine has no list of ship
//! classes, engine requirement 7; a wrong `type` drops the ship silently, docs/missions.md section 4); the "Player N" / "Ship N"
//! default names; the human's-ship line ([`human_ship`]) restates the game's choice of a ship for the player (primary, else first,
//! else a stand-in cutter) to SHOW it, never to check it.

use bevy_egui::egui;
use serde_json::{Value, json};

use super::doc::{self as mdoc, MissionList};
use super::groups;
use super::project::{check_add_delete, merged_map, mission_rel};
use super::refs;
use crate::edit::Edit;
use crate::project::Project;
use crate::ships_panel::list_ship_ids;

// ---- reading ----

/// The ship classes a mission can place: every `ships/<id>/ship.json` of the overlay and of the mod, sorted. PROVISIONAL (see the
/// module doc).
pub fn ship_types(project: &Project) -> Vec<String> {
    let mut ids: Vec<String> = list_ship_ids(&project.root).into_iter().chain(list_ship_ids(&project.dir)).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The types in the order a picker lists them: ships first (`Ship_...`), then bases (`Base_...`), then everything else (islands,
/// rocks, asteroids), each part sorted as `types` is. PROVISIONAL: the grouping goes by the prefix of the folder name (the naming
/// of the retail data, docs/data-format.md); the ships' folders carry no kind of their own.
pub fn picker_order(types: &[String]) -> Vec<String> {
    let part = |t: &String| match () {
        () if t.starts_with("Ship_") => 0,
        () if t.starts_with("Base_") => 1,
        () => 2,
    };
    let mut out: Vec<String> = types.to_vec();
    out.sort_by_key(part); // stable: sorted inside each part
    out
}

/// What "Add ship" starts with: the owner is the human's player (`player_start.name`) when the mission has it among its players,
/// else the first player; the type is that owner's latest ship's type, else the first ship of the mod's own `ships/` folders, else
/// the first ship type of the import ([`picker_order`]: a `Ship_` before an island). `None` with no player or no type.
pub fn new_ship_defaults(mission: &Value, types: &[String], mod_types: &[String]) -> Option<(String, String)> {
    let players = mdoc::players(mission);
    let owner = mdoc::player_start_name(mission).filter(|n| players.contains(n)).or_else(|| players.first().copied())?;
    let own_latest = mdoc::ships(mission).iter().rev().find(|s| mdoc::ship_group(s) == Some(owner)).and_then(mdoc::ship_type).filter(|t| types.iter().any(|x| x == t));
    let ordered = picker_order(types);
    let is_ship = |t: &&String| t.starts_with("Ship_");
    let ty = own_latest.map(str::to_string).or_else(|| picker_order(mod_types).iter().find(is_ship).cloned()).or_else(|| ordered.first().cloned())?;
    Some((ty, owner.to_string()))
}

/// The game's choice of the ship the human flies (docs/missions.md section 4), read from the documents: shown, not checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HumanShip {
    /// The `primary` ship (index in `ships`) of `player_start.name`.
    Primary(usize),
    /// No `primary`: that player's first ship.
    First(usize),
    /// The player owns no ship: the game places a stand-in cutter at `player_start.start`.
    StandIn,
    /// No `player_start.name`.
    NoStart,
}

/// Which ship the human flies. PROVISIONAL (restated from docs/missions.md section 4 to show it).
pub fn human_ship(mission: &Value) -> HumanShip {
    let Some(player) = mdoc::player_start_name(mission) else { return HumanShip::NoStart };
    let own = |(_, s): &(usize, &Value)| mdoc::ship_group(s) == Some(player);
    let ships: Vec<(usize, &Value)> = mdoc::ships(mission).iter().enumerate().filter(own).collect();
    if let Some((i, _)) = ships.iter().find(|(_, s)| mdoc::ship_primary(s)) {
        return HumanShip::Primary(*i);
    }
    match ships.first() {
        Some((i, _)) => HumanShip::First(*i),
        None => HumanShip::StandIn,
    }
}

/// The line under the ship list.
pub fn human_ship_line(mission: &Value) -> String {
    let name = |i: usize| mdoc::ships(mission).get(i).and_then(mdoc::ship_name).unwrap_or("(unnamed)").to_string();
    match human_ship(mission) {
        HumanShip::Primary(i) => format!("The human flies '{}' (the primary ship of {}).", name(i), mdoc::player_start_name(mission).unwrap_or("")),
        HumanShip::First(i) => format!("The human flies '{}' (the first ship of {}, none is primary).", name(i), mdoc::player_start_name(mission).unwrap_or("")),
        HumanShip::StandIn => "The human has no ship: the game places a stand-in cutter at the start.".to_string(),
        HumanShip::NoStart => "No player_start: choose the human's player.".to_string(),
    }
}

/// Whether `ty` is not among `types` (the ship would be dropped silently by the game). `types` empty (no import found) never warns.
pub fn unknown_type(types: &[String], ty: &str) -> bool {
    !types.is_empty() && !types.iter().any(|t| t == ty)
}

/// The first free "Player N" of the mission.
pub fn unique_player_name(mission: &Value) -> String {
    let taken = mdoc::players(mission);
    (1..).map(|n| format!("Player {n}")).find(|c| !taken.contains(&c.as_str())).expect("a free name exists")
}

/// The first free "Ship N" among the ship names of the mission.
pub fn unique_ship_name(mission: &Value) -> String {
    let taken: Vec<&str> = mdoc::ships(mission).iter().filter_map(mdoc::ship_name).collect();
    (1..).map(|n| format!("Ship {n}")).find(|c| !taken.contains(&c.as_str())).expect("a free name exists")
}

/// Whether players `a` and `b` are allied (either order).
pub fn allied(mission: &Value, a: &str, b: &str) -> bool {
    mdoc::alliances(mission).iter().any(|p| (p[0] == a && p[1] == b) || (p[0] == b && p[1] == a))
}

fn open<'a>(project: &'a Project, id: &str) -> Result<(&'a Value, String), String> {
    let rel = mission_rel(id)?;
    let v = project.doc(&rel).map(|d| &d.edited).filter(|v| !v.is_null()).ok_or_else(|| format!("mission '{id}' is not open"))?;
    Ok((v, rel))
}

fn check_player_name(mission: &Value, name: &str, except: Option<usize>) -> Result<(), String> {
    if name.is_empty() {
        return Err("a player needs a name".into());
    }
    if name.contains(',') {
        return Err("a player name cannot hold a comma (a rule writes Group,Unit)".into());
    }
    let clash = mdoc::player_entries(mission).iter().enumerate().any(|(i, p)| Some(i) != except && p.as_str() == Some(name));
    if clash { Err(format!("there is a player '{name}' already")) } else { Ok(()) }
}

// ---- players and alliances ----

/// Adds a player as one undo step and returns its index. Refused for an empty, comma or taken name and (mission that extends another)
/// by the project, `players` not merging by key.
pub fn add_player(project: &mut Project, id: &str, name: &str) -> Result<usize, String> {
    let name = name.trim().to_string();
    check_player_name(open(project, id)?.0, &name, None)?;
    super::project::add_entry(project, id, MissionList::Players, json!(name))
}

/// Deletes player `index` and every alliance that names it, as one undo step. Refused while a ship is owned by the player, the
/// start is the player's, or a rule names it; and when the mission extends another (`players`, `alliances` are not keyed).
pub fn delete_player(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    let name = mdoc::player_entries(m).get(index).and_then(Value::as_str).ok_or_else(|| format!("no player {index}"))?.to_string();
    let owned = mdoc::ships(m).iter().filter(|s| mdoc::ship_group(s) == Some(name.as_str())).count();
    if owned > 0 {
        return Err(format!("'{name}' owns {owned} ship(s): move or delete them first"));
    }
    if mdoc::player_start_name(m) == Some(name.as_str()) {
        return Err(format!("'{name}' is the human's player: choose another in the start first"));
    }
    let n = refs::references(m, "player", &name).len();
    if n > 0 {
        return Err(format!("{n} rule field(s) still name '{name}': change them first"));
    }
    check_add_delete(project, id, MissionList::Players)?;
    let (m, _) = open(project, id)?;
    let mut edits = vec![];
    let pairs: Vec<usize> =
        mdoc::alliance_entries(m).iter().enumerate().filter(|(_, p)| p.as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(name.as_str())))).map(|(i, _)| i).collect();
    if !pairs.is_empty() {
        check_add_delete(project, id, MissionList::Alliances)?;
    }
    for i in pairs.into_iter().rev() {
        edits.push(Edit::remove(mdoc::alliance_pointer(i, None)));
    }
    edits.push(Edit::remove(mdoc::player_pointer(index)));
    project.do_step(&rel, &format!("Delete player {name}"), edits)
}

/// Renames player `index` as one undo step: the player, the `group` of its ships, its alliances, `player_start.name` and every rule
/// or task field that names it. Returns the number of rule fields that followed.
pub fn rename_player(project: &mut Project, id: &str, index: usize, new: &str) -> Result<usize, String> {
    let (m, rel) = open(project, id)?;
    let new = new.trim();
    let old = mdoc::player_entries(m).get(index).and_then(Value::as_str).ok_or_else(|| format!("no player {index}"))?.to_string();
    if old == new {
        return Ok(0);
    }
    check_player_name(m, new, Some(index))?;
    let mut edits = vec![Edit::set(mdoc::player_pointer(index), json!(new))];
    for (i, s) in mdoc::ships(m).iter().enumerate() {
        if mdoc::ship_group(s) == Some(old.as_str()) {
            edits.push(Edit::set(mdoc::ship_pointer(i, Some(mdoc::SHIP_GROUP)), json!(new)));
        }
    }
    for (i, p) in mdoc::alliance_entries(m).iter().enumerate() {
        for side in 0..2 {
            if p.get(side).and_then(Value::as_str) == Some(old.as_str()) {
                edits.push(Edit::set(mdoc::alliance_pointer(i, Some(side)), json!(new)));
            }
        }
    }
    if mdoc::player_start_name(m) == Some(old.as_str()) {
        edits.push(Edit::set(mdoc::player_start_pointer(Some(mdoc::START_NAME)), json!(new)));
    }
    let followed = refs::rename_edits(m, "player", &old, new)?;
    let n = followed.len();
    edits.extend(followed);
    project.do_step(&rel, &format!("Rename player {old}"), edits)?;
    Ok(n)
}

/// Makes players `a` and `b` allied or hostile as one undo step. Refused for the same player twice, and (mission that extends
/// another) by the project, `alliances` not merging by key.
pub fn set_allied(project: &mut Project, id: &str, a: &str, b: &str, on: bool) -> Result<(), String> {
    let (m, _) = open(project, id)?;
    if a == b {
        return Err("a player is allied with itself already".into());
    }
    let players = mdoc::players(m);
    if !players.contains(&a) || !players.contains(&b) {
        return Err("both must be players of the mission".into());
    }
    match (allied(m, a, b), on) {
        (true, true) | (false, false) => Ok(()),
        (false, true) => super::project::add_entry(project, id, MissionList::Alliances, json!([a, b])).map(|_| ()),
        (true, false) => {
            // Every listed copy of the pair, last first, so the indices stay right.
            let at: Vec<usize> = mdoc::alliance_entries(m)
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    let s = |i| p.get(i).and_then(Value::as_str);
                    (s(0) == Some(a) && s(1) == Some(b)) || (s(0) == Some(b) && s(1) == Some(a))
                })
                .map(|(i, _)| i)
                .collect();
            check_add_delete(project, id, MissionList::Alliances)?;
            let (_, rel) = open(project, id)?;
            let edits = at.into_iter().rev().map(|i| Edit::remove(mdoc::alliance_pointer(i, None))).collect();
            project.do_step(&rel, &format!("Make {a} and {b} hostile"), edits)
        }
    }
}

// ---- player_start ----

/// Sets the human's player (`player_start.name`) as one undo step.
pub fn set_start_player(project: &mut Project, id: &str, name: &str) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    if !mdoc::players(m).contains(&name) {
        return Err(format!("'{name}' is not a player of the mission"));
    }
    project.do_step(&rel, &format!("Human is {name}"), vec![Edit::set(mdoc::player_start_pointer(Some(mdoc::START_NAME)), json!(name))])
}

/// Sets `player_start.start` (`forward == false`) or `player_start.forward` (`true`), merging with the edits of the same drag when
/// `typing`. A `forward` of no length is refused.
pub fn set_start_vec(project: &mut Project, id: &str, forward: bool, v: [f64; 3], typing: bool) -> Result<(), String> {
    let (_, rel) = open(project, id)?;
    if forward && v.iter().all(|c| *c == 0.0) {
        return Err("the forward direction needs a length".into());
    }
    let key = if forward { mdoc::START_FORWARD } else { mdoc::START_POS };
    let label = if forward { "Turn the start" } else { "Move the start" };
    project.apply_form(&rel, label, vec![Edit::set(mdoc::player_start_pointer(Some(key)), json!(v))], typing)
}

// ---- ships ----

fn ship_at(mission: &Value, index: usize) -> Result<&Value, String> {
    mdoc::ships(mission).get(index).ok_or_else(|| format!("no ship {index}"))
}

/// Adds a ship of class `ty` owned by player `group`, at the origin, as one undo step; returns its index. Its id is
/// [`groups::next_id`], its name a free "Ship N". Refused for an unknown owner or an empty type.
pub fn add_ship(project: &mut Project, id: &str, ty: &str, group: &str) -> Result<usize, String> {
    let (m, _) = open(project, id)?;
    if ty.trim().is_empty() {
        return Err("a ship needs a type".into());
    }
    if !mdoc::players(m).contains(&group) {
        return Err(format!("'{group}' is not a player of the mission"));
    }
    let new = json!({
        mdoc::SHIP_ID: groups::next_id(m, merged_map(project, id)),
        mdoc::SHIP_TYPE: ty,
        mdoc::SHIP_GROUP: group,
        mdoc::SHIP_NAME: unique_ship_name(m),
        mdoc::SHIP_POS: [0.0, 0.0, 0.0],
    });
    super::project::add_entry(project, id, MissionList::Ships, new)
}

/// Deletes ship `index` as one undo step, taking its id out of every group. Refused while a rule names the ship as a unit.
pub fn delete_ship(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    let ship = ship_at(m, index)?;
    if let Some(name) = mdoc::ship_name(ship) {
        let n = refs::references(m, refs::UNIT, name).len();
        if n > 0 {
            return Err(format!("{n} rule field(s) still name the ship '{name}': change them first"));
        }
    }
    let member = mdoc::ship_id(ship);
    let label = format!("Delete ship {}", mdoc::ship_name(ship).unwrap_or("#"));
    let mut edits = member.map(|i| groups::drop_member_edits(m, i)).unwrap_or_default();
    check_add_delete(project, id, MissionList::Ships)?;
    edits.push(Edit::remove(mdoc::ship_pointer(index, None)));
    project.do_step(&rel, &label, edits)
}

/// Sets the class of ship `index` as one undo step. An empty type is refused; a type outside [`ship_types`] is allowed (the form
/// warns: the game drops such a ship silently).
pub fn set_ship_type(project: &mut Project, id: &str, index: usize, ty: &str) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    ship_at(m, index)?;
    if ty.trim().is_empty() {
        return Err("a ship needs a type".into());
    }
    project.do_step(&rel, "Set ship type", vec![Edit::set(mdoc::ship_pointer(index, Some(mdoc::SHIP_TYPE)), json!(ty.trim()))])
}

/// Gives ship `index` to player `group` as one undo step. A `primary` ship stays primary and takes the flag off the new owner's other
/// primary ship, in the same step.
pub fn set_ship_group(project: &mut Project, id: &str, index: usize, group: &str) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    let ship = ship_at(m, index)?;
    if !mdoc::players(m).contains(&group) {
        return Err(format!("'{group}' is not a player of the mission"));
    }
    let mut edits = vec![Edit::set(mdoc::ship_pointer(index, Some(mdoc::SHIP_GROUP)), json!(group))];
    if mdoc::ship_primary(ship) {
        edits.extend(clear_primary_edits(m, group, Some(index)));
    }
    project.do_step(&rel, "Set ship owner", edits)
}

/// Renames ship `index` as one undo step; the `Group,Unit` references of the rules follow. Refused for an empty name, a comma, or a
/// name another ship has (the game finds the unit by `name` alone).
pub fn rename_ship(project: &mut Project, id: &str, index: usize, new: &str) -> Result<usize, String> {
    let (m, rel) = open(project, id)?;
    let new = new.trim();
    let old = mdoc::ship_name(ship_at(m, index)?).unwrap_or("").to_string();
    if new.is_empty() {
        return Err("a ship needs a name".into());
    }
    if new.contains(',') {
        return Err("a ship name cannot hold a comma (a rule writes Group,Unit)".into());
    }
    if new == old {
        return Ok(0);
    }
    if mdoc::ships(m).iter().enumerate().any(|(i, s)| i != index && mdoc::ship_name(s) == Some(new)) {
        return Err(format!("another ship is named '{new}' already"));
    }
    let mut edits = vec![Edit::set(mdoc::ship_pointer(index, Some(mdoc::SHIP_NAME)), json!(new))];
    let followed = if old.is_empty() { vec![] } else { refs::rename_edits(m, refs::UNIT, &old, new)? };
    let n = followed.len();
    edits.extend(followed);
    project.do_step(&rel, &format!("Rename ship {old}"), edits)?;
    Ok(n)
}

/// Gives ship `index` the id `new` as one undo step; every group that lists the old id lists the new one. Refused when another ship,
/// animal, or object, ship or mine of the map has the id already (a group would address both), as [`groups::next_id`] avoids.
pub fn set_ship_id(project: &mut Project, id: &str, index: usize, new: i64) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    let old = mdoc::ship_id(ship_at(m, index)?);
    if old == Some(new) {
        return Ok(());
    }
    if groups::used_ids(m, merged_map(project, id)).contains(&new) {
        return Err(format!("the id {new} is used already (by a ship, an animal, or an object or mine of the map)"));
    }
    let mut edits = vec![Edit::set(mdoc::ship_pointer(index, Some(mdoc::SHIP_ID)), json!(new))];
    if let Some(old) = old {
        edits.extend(groups::renumber_member_edits(m, old, new));
    }
    project.do_step(&rel, "Set ship id", edits)
}

/// Sets the position of ship `index`, merging with the edits of the same drag when `typing`.
pub fn set_ship_pos(project: &mut Project, id: &str, index: usize, pos: [f64; 3], typing: bool) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    ship_at(m, index)?;
    project.apply_form(&rel, "Move ship", vec![Edit::set(mdoc::ship_pointer(index, Some(mdoc::SHIP_POS)), json!(pos))], typing)
}

/// The edits that take `primary` off every ship of `group` but `keep`.
fn clear_primary_edits(mission: &Value, group: &str, keep: Option<usize>) -> Vec<Edit> {
    mdoc::ships(mission)
        .iter()
        .enumerate()
        .filter(|(i, s)| Some(*i) != keep && mdoc::ship_group(s) == Some(group) && mdoc::ship_primary(s))
        .map(|(i, _)| Edit::set(mdoc::ship_pointer(i, Some(mdoc::SHIP_PRIMARY)), json!(false)))
        .collect()
}

/// Makes ship `index` the `primary` ship of its owner (turning every other primary ship of that owner off) or takes the flag off, as
/// one undo step.
pub fn set_primary(project: &mut Project, id: &str, index: usize, on: bool) -> Result<(), String> {
    let (m, rel) = open(project, id)?;
    let ship = ship_at(m, index)?;
    let mut edits = vec![Edit::set(mdoc::ship_pointer(index, Some(mdoc::SHIP_PRIMARY)), json!(on))];
    if on {
        edits.extend(clear_primary_edits(m, mdoc::ship_group(ship).unwrap_or(""), Some(index)));
    }
    project.do_step(&rel, if on { "Make ship primary" } else { "Clear primary" }, edits)
}

// ---- the form ----

/// The ship type dropdown: a filter box above the list (like the scenery picker; the popup stays open for a click in the box and
/// closes on a pick or a click outside). Returns the type picked this frame.
fn type_picker(ui: &mut egui::Ui, id: egui::Id, current: &str, types: &[String]) -> Option<String> {
    let mut picked = None;
    let filter_id = id.with("filter");
    egui::ComboBox::from_id_salt(id)
        .selected_text(if current.is_empty() { "(no type)" } else { current })
        .close_behavior(egui::containers::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
            let r = ui.add(egui::TextEdit::singleline(&mut filter).hint_text("filter").desired_width(160.0));
            if r.changed() {
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            }
            let needle = filter.trim().to_lowercase();
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                // the order of `types` is kept: ships, then bases, then the rest
                for t in types.iter().filter(|t| needle.is_empty() || t.to_lowercase().contains(&needle)) {
                    if ui.selectable_label(t == current, t).clicked() {
                        picked = Some(t.clone());
                        ui.close();
                    }
                }
            });
        });
    picked
}

/// A text box that keeps what is typed until it loses focus; returns the text then when it changed.
pub(crate) fn text_edit(ui: &mut egui::Ui, id: egui::Id, current: &str, width: f32) -> Option<String> {
    let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut buf).id(id).desired_width(width));
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return (buf != current).then_some(buf);
    }
    if r.has_focus() || r.changed() {
        ui.data_mut(|d| d.insert_temp(id, buf));
    }
    None
}

fn vec3(ui: &mut egui::Ui, v: [f64; 3]) -> Option<[f64; 3]> {
    let mut out = v;
    let mut changed = false;
    for c in &mut out {
        changed |= ui.add(egui::DragValue::new(c).update_while_editing(false).speed(1.0)).changed();
    }
    changed.then_some(out)
}

/// What one frame of the form asks for (applied after drawing, so the documents are not borrowed while drawing).
enum Op {
    AddPlayer,
    DeletePlayer(usize),
    RenamePlayer(usize, String),
    Allied(String, String, bool),
    StartPlayer(String),
    StartVec(bool, [f64; 3]),
    AddShip,
    DeleteShip(usize),
    ShipType(usize, String),
    ShipGroup(usize, String),
    ShipName(usize, String),
    ShipId(usize, i64),
    ShipPos(usize, [f64; 3]),
    Primary(usize, bool),
}

/// Draws the section: the mission to edit, its players, alliances, start, ships and groups.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    let ctx = ui.ctx().clone();
    let error_id = egui::Id::new("missions_ships_error");
    let mut error: Option<String> = ctx.data_mut(|d| d.get_temp::<String>(error_id));
    let Some(id) = super::rules_view::current_mission(ui, project) else { return };
    let Ok((m, _)) = open(project, &id) else { return };
    let types = ship_types(project);
    let picker_types = picker_order(&types);
    let players: Vec<String> = mdoc::players(m).into_iter().map(str::to_string).collect();
    let mut ops: Vec<Op> = vec![];

    ui.strong("Players");
    for (i, name) in players.iter().enumerate() {
        ui.horizontal(|ui| {
            if let Some(new) = text_edit(ui, egui::Id::new(("player_name", &id, i)), name, 160.0) {
                ops.push(Op::RenamePlayer(i, new));
            }
            if ui.small_button("Delete").clicked() {
                ops.push(Op::DeletePlayer(i));
            }
        });
    }
    if ui.button("Add player").clicked() {
        ops.push(Op::AddPlayer);
    }
    if players.len() >= 2 {
        ui.label("Alliances (a pair that is not ticked is hostile):");
        for (i, a) in players.iter().enumerate() {
            for b in &players[i + 1..] {
                let mut on = allied(m, a, b);
                if ui.checkbox(&mut on, format!("{a} + {b}")).changed() {
                    ops.push(Op::Allied(a.clone(), b.clone(), on));
                }
            }
        }
    }

    ui.separator();
    ui.strong("The human's start");
    ui.horizontal(|ui| {
        ui.label("Player");
        let current = mdoc::player_start_name(m).unwrap_or("(none)");
        egui::ComboBox::from_id_salt("start_player").selected_text(current).show_ui(ui, |ui| {
            for p in &players {
                if ui.selectable_label(current == p, p).clicked() {
                    ops.push(Op::StartPlayer(p.clone()));
                }
            }
        });
    });
    // `start` and `forward` are written only when typed: until then the boxes show the defaults, greyed.
    for (forward, label, value, default) in [(false, "start", mdoc::player_start_pos(m), [0.0; 3]), (true, "forward", mdoc::player_start_forward(m), [0.0, 1.0, 0.0])] {
        ui.horizontal(|ui| {
            ui.label(label);
            let set = value.is_some();
            ui.scope(|ui| {
                if !set {
                    let weak = ui.visuals().weak_text_color();
                    ui.visuals_mut().override_text_color = Some(weak);
                }
                if let Some(v) = vec3(ui, value.unwrap_or(default)) {
                    ops.push(Op::StartVec(forward, v));
                }
            });
            if !set {
                ui.weak("(default, not in the file)");
            }
        });
    }
    ui.weak("The game reads start and forward only when the human's player owns no ship (then it places a stand-in cutter there); a player with a ship ignores them. PROVISIONAL: restated from docs/missions.md section 4.");

    ui.separator();
    ui.strong("Ships");
    for (i, s) in mdoc::ships(m).iter().enumerate() {
        ui.push_id(("ship", i), |ui| {
            ui.horizontal_wrapped(|ui| {
                let cur = mdoc::ship_id(s).map_or(String::new(), |v| v.to_string());
                if let Some(new) = text_edit(ui, egui::Id::new(("ship_id", &id, i)), &cur, 40.0) {
                    match new.trim().parse::<i64>() {
                        Ok(n) => ops.push(Op::ShipId(i, n)),
                        Err(_) => error = Some(format!("'{}' is not a whole number: a ship's id is an integer", new.trim())),
                    }
                }
                if let Some(new) = text_edit(ui, egui::Id::new(("ship_name", &id, i)), mdoc::ship_name(s).unwrap_or(""), 120.0) {
                    ops.push(Op::ShipName(i, new));
                }
                let ty = mdoc::ship_type(s).unwrap_or("");
                if let Some(t) = type_picker(ui, egui::Id::new(("ship_type", &id, i)), ty, &picker_types) {
                    ops.push(Op::ShipType(i, t));
                }
                if unknown_type(&types, ty) {
                    ui.colored_label(egui::Color32::from_rgb(220, 150, 60), "unknown type: the game drops this ship")
                        .on_hover_text("PROVISIONAL: the types are the ships/ folders of the overlay");
                }
                let group = mdoc::ship_group(s).unwrap_or("");
                egui::ComboBox::from_id_salt("ship_owner").selected_text(if group.is_empty() { "(no owner)" } else { group }).show_ui(ui, |ui| {
                    for p in &players {
                        if ui.selectable_label(p == group, p).clicked() {
                            ops.push(Op::ShipGroup(i, p.clone()));
                        }
                    }
                });
                let mut primary = mdoc::ship_primary(s);
                if ui.checkbox(&mut primary, "primary").changed() {
                    ops.push(Op::Primary(i, primary));
                }
                if let Some(v) = vec3(ui, mdoc::ship_pos(s).unwrap_or([0.0; 3])) {
                    ops.push(Op::ShipPos(i, v));
                }
                if ui.small_button("Delete").clicked() {
                    ops.push(Op::DeleteShip(i));
                }
            });
        });
    }
    if ui.add_enabled(!players.is_empty(), egui::Button::new("Add ship")).on_disabled_hover_text("Add a player first").clicked() {
        ops.push(Op::AddShip);
    }
    ui.label(human_ship_line(m));

    ui.separator();
    ui.strong("Groups");
    let mod_types = list_ship_ids(&project.dir);
    let new_ship = open(project, &id).ok().and_then(|(m, _)| new_ship_defaults(m, &types, &mod_types));
    for op in ops {
        let r = match op {
            Op::AddPlayer => {
                let name = unique_player_name(open(project, &id).map(|x| x.0).unwrap_or(&Value::Null));
                add_player(project, &id, &name).map(|_| ())
            }
            Op::DeletePlayer(i) => delete_player(project, &id, i),
            Op::RenamePlayer(i, n) => rename_player(project, &id, i, &n).map(|_| ()),
            Op::Allied(a, b, on) => set_allied(project, &id, &a, &b, on),
            Op::StartPlayer(p) => set_start_player(project, &id, &p),
            Op::StartVec(fwd, v) => set_start_vec(project, &id, fwd, v, true),
            Op::AddShip => match &new_ship {
                Some((t, p)) => add_ship(project, &id, t, p).map(|_| ()),
                None if types.is_empty() => Err("no ship class found in the overlay or the mod: import the game's assets first".to_string()),
                None => Err("add a player first".to_string()),
            },
            Op::DeleteShip(i) => delete_ship(project, &id, i),
            Op::ShipType(i, t) => set_ship_type(project, &id, i, &t),
            Op::ShipGroup(i, g) => set_ship_group(project, &id, i, &g),
            Op::ShipId(i, n) => set_ship_id(project, &id, i, n),
            Op::ShipName(i, n) => rename_ship(project, &id, i, &n).map(|_| ()),
            Op::ShipPos(i, v) => set_ship_pos(project, &id, i, v, true),
            Op::Primary(i, on) => set_primary(project, &id, i, on),
        };
        match r {
            Ok(()) => error = None,
            Err(e) => error = Some(e),
        }
    }
    groups::show(ui, project, &id, &mut error);
    if let Some(e) = &error {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
    ctx.data_mut(|d| match error {
        Some(e) => {
            d.insert_temp(error_id, e);
        }
        None => {
            d.remove::<String>(error_id);
        }
    });
}
