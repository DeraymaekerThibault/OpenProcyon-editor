//! The skirmish part of the Maps tab (docs/phase4.md P8, work item T10): what makes a map list in the skirmish lobby, who decides how
//! the game ends, the map's own `script.json` rules and `media.json` words, and the shortcut that plays the map through the lobby.
//!
//! Everything here is an edit of a project document, one undo step each, so a map that extends a retail one is saved as a diff and a
//! new `script.json` or `media.json` is created in the mod when it is first used (never before: opening the section writes nothing).
//! Names the map keys through [`super::doc`] only; the rule vocabulary is the engine's (`authoring::rule_schema`, through
//! [`crate::schema_form`]), nothing about a Type or a field is written down here.
//!
//! **What the engine needs for a map to list in the lobby** (engine `f91542f`, `game/src/menu/skirmish.rs`):
//! - the list is every `maps/<id>/map.json` of the overlay with `"skirmish": true` (`skirmish_maps` at line 985, `skirmish_entry` at
//!   line 1000); nothing else is read for the row. The row's player count is the number of `players` with `playable: true`
//!   (`MapDoc::playable_players`, `data/maps.rs` line 169).
//! - to play, the lobby's rows are those playable players (`new_slots`, line 653; the first is the human) and it refuses to start
//!   without a live opponent: another playable player not on the human's team (`launch_error` line 891, `hostile` line 906).
//! - PROVISIONAL (read in the engine, not played here): a historical map (`assemble` false) takes each player's fleet from the
//!   `ships` that name that player (`ships_of`); an open map (`assemble` true) lets the players pick fleets.
//!
//! The checklist ([`checklist`]) takes the first two from the engine itself (`MapDoc`) and restates the opponent rule, which is
//! private there (PROVISIONAL until the engine publishes it).

use bevy::prelude::Resource;
use bevy_egui::egui;
use openprocyon::data::MapDoc;
use openprocyon::rules::schema::Equivalence;
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::process::Child;

use super::doc;
use super::project as maps;
use crate::checks::Finding;
use crate::edit::{Edit, pointer};
use crate::launch::{self, Launch, Lobby};
use crate::mission::doc as mdoc;
use crate::mission::media;
use crate::mission::rules::{self, Target, unique_name};
use crate::project::Project;
use crate::schema_form::{self, Names, Schema};

// ---------------------------------------------------------------------------------------------------------------------------------
// documents

/// `maps/<id>/script.json`.
pub fn script_rel(id: &str) -> Result<String, String> {
    maps::map_rel(id).map(|_| format!("maps/{id}/{}", mdoc::SCRIPT_FILE))
}

/// `maps/<id>/media.json`.
pub fn media_rel(id: &str) -> Result<String, String> {
    maps::map_rel(id).map(|_| format!("maps/{id}/{}", mdoc::MEDIA_FILE))
}

/// Opens the document `rel` when the mod or the import has the file (a script or media the map already has); `false` when neither
/// does or it cannot be read. Opening writes nothing.
fn open_existing(project: &mut Project, rel: &str) -> bool {
    if project.doc(rel).is_some() {
        return true;
    }
    let there = project.dir.join(rel).is_file() || project.import.as_ref().is_some_and(|i| i.join(rel).is_file());
    there && project.open_doc(rel).is_ok()
}

/// Opens the map's `script.json` and `media.json` when they exist. The error of a file that cannot be read.
pub fn open_documents(project: &mut Project, id: &str) -> Result<(), String> {
    for rel in [script_rel(id)?, media_rel(id)?] {
        let there = project.doc(&rel).is_some() || project.dir.join(&rel).is_file() || project.import.as_ref().is_some_and(|i| i.join(&rel).is_file());
        if there {
            project.open_doc(&rel)?;
        }
    }
    Ok(())
}

/// The document as the game would see it, when it is open.
fn view<'a>(project: &'a Project, rel: &str) -> Option<&'a Value> {
    project.doc(rel).map(|d| &d.edited).filter(|v| !v.is_null())
}

/// The map's script, when it has one (open).
pub fn script_of<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    view(project, &script_rel(id).ok()?)
}

/// The map's media, when it has some (open).
pub fn media_of<'a>(project: &'a Project, id: &str) -> Option<&'a Value> {
    view(project, &media_rel(id).ok()?)
}

/// What a new `script.json` starts as: no rules, and the map's own end rules decide (no `victory` key).
pub fn new_script() -> Value {
    json!({"rules": []})
}

/// What a new `media.json` starts as (docs/missions.md section 5: the smallest that loads).
pub fn new_media() -> Value {
    json!({"speech": {}, "messages": {}, "ship_names": {}})
}

/// Makes `rel` a document: the existing file when there is one, else a new one that the next save creates whole.
fn ensure(project: &mut Project, rel: &str, initial: Value) -> Result<(), String> {
    if open_existing(project, rel) {
        return Ok(());
    }
    project.open_new_doc(rel, None, initial).map(|_| ())
}

fn ensure_script(project: &mut Project, id: &str) -> Result<String, String> {
    let rel = script_rel(id)?;
    ensure(project, &rel, new_script())?;
    Ok(rel)
}

fn ensure_media(project: &mut Project, id: &str) -> Result<String, String> {
    let rel = media_rel(id)?;
    ensure(project, &rel, new_media())?;
    Ok(rel)
}

fn map_step(project: &mut Project, id: &str, label: &str, edits: Vec<Edit>) -> Result<(), String> {
    let rel = maps::map_rel(id)?;
    maps::open_map(project, id)?;
    project.do_step(&rel, label, edits)
}

/// Typing in one field joins the step before it (same label, same pointer); anything else is a step of its own.
fn form_step(project: &mut Project, rel: &str, label: &str, edits: Vec<Edit>, typing: bool) -> Result<(), String> {
    project.apply_form(rel, label, edits, typing)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// the flag, the checklist

/// Turns the map on or off in the skirmish list (`skirmish`).
pub fn set_skirmish(project: &mut Project, id: &str, on: bool) -> Result<(), String> {
    map_step(project, id, if on { "List in the skirmish lobby" } else { "Unlist from the skirmish lobby" }, vec![Edit::set(pointer(&[doc::KEY_SKIRMISH]), json!(on))])
}

/// An open map (`assemble`): the players assemble their fleets in the lobby. Off: a historical map with pre-placed `ships`.
pub fn set_assemble(project: &mut Project, id: &str, on: bool) -> Result<(), String> {
    map_step(project, id, if on { "Open map (fleets assembled)" } else { "Historical map (placed ships)" }, vec![Edit::set(pointer(&[doc::KEY_ASSEMBLE]), json!(on))])
}

/// One line of the lobby checklist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    /// What must hold.
    pub text: String,
    /// What to do when it does not (empty when it holds).
    pub hint: String,
    /// The map does not list, or cannot start, without it.
    pub blocking: bool,
}

fn check(ok: bool, blocking: bool, text: impl Into<String>, hint: &str) -> Check {
    Check { ok, blocking, text: text.into(), hint: if ok { String::new() } else { hint.to_string() } }
}

/// The playable players as the lobby builds its rows (`new_slots`): their index in `players` and their team (-1: none).
fn playable(map: &Value) -> Vec<(usize, i64)> {
    doc::players(map).iter().enumerate().filter(|(_, p)| doc::player_playable(p)).map(|(i, p)| (i, doc::player_team(p).unwrap_or(-1))).collect()
}

/// Whether the human (the first playable player) has someone to fight (the engine's `launch_error`: a live slot on another team, or
/// a teamless one; a team is shared only when it is 0 or more and equal). PROVISIONAL: restated from `menu/skirmish.rs` line 893/906.
pub fn has_opponent(map: &Value) -> bool {
    let slots = playable(map);
    let Some(&(_, human)) = slots.first() else { return false };
    slots.iter().skip(1).any(|&(_, team)| !(human >= 0 && human == team))
}

/// The lobby checklist of `map`: what makes it list and start. The flag and the player count are the engine's own reading
/// (`MapDoc`); the rest is restated and marked in [`self`](self) module docs.
pub fn checklist(map: &Value) -> Vec<Check> {
    let engine = MapDoc::from_value(map);
    let listed = engine.skirmish == Some(true);
    let players = engine.playable_players();
    let slots = playable(map);
    let mut out = vec![
        check(listed, true, "\"skirmish\" is on", "turn on \"Skirmish map\" above: the lobby lists only maps with it"),
        check(
            players >= 1,
            true,
            format!("a playable player ({players} playable)"),
            "add a player in \"Players and teams\" and tick Playable: the lobby's rows are the playable players",
        ),
        check(
            has_opponent(map),
            true,
            "an opponent slot on another team".to_string(),
            "the game needs a second playable player on a different team than the first (the lobby says \"No opponent\")",
        ),
    ];
    let no_start: Vec<String> = slots.iter().filter(|(i, _)| doc::player_start(&doc::players(map)[*i]).is_none()).map(|(i, _)| i.to_string()).collect();
    out.push(check(
        no_start.is_empty(),
        true,
        "every playable player has a start".to_string(),
        &format!("players {} have no start: place one on the plan (PROVISIONAL: the engine's reading of a missing start is not checked here)", no_start.join(", ")),
    ));
    if doc::assemble(map) {
        out.push(Check { ok: true, blocking: false, text: "open map: the players assemble their fleets in the lobby".into(), hint: String::new() });
    } else {
        let unfunded: Vec<String> = slots.iter().filter(|(i, _)| !doc::ships(map).iter().any(|s| doc::ship_player(s) == Some(*i as u64))).map(|(i, _)| i.to_string()).collect();
        out.push(check(
            unfunded.is_empty(),
            true,
            "historical map: every playable player has placed ships".to_string(),
            &format!("players {} have no ship in \"ships\": add ships for them, or tick \"Open map\" so the players assemble fleets (PROVISIONAL)", unfunded.join(", ")),
        ));
    }
    // A hint, not a blocker: the engine reads a missing team race as 4 (open) and the lobby then lets the players choose.
    // PROVISIONAL: the engine's `--check-mod` is gaining an error for it; this stays an editor-side hint until the pin moves.
    if !doc::teams(map).is_empty() {
        let no_race: Vec<String> = doc::teams(map)
            .iter()
            .enumerate()
            .filter(|(_, t)| doc::team_race(t).is_none())
            .map(|(i, t)| doc::team_name_id(t).filter(|n| !n.is_empty()).map_or_else(|| format!("team {i}"), str::to_string))
            .collect();
        out.push(check(
            no_race.is_empty(),
            false,
            "every team has a race (its fleet type in the lobby)",
            &format!("{} have no race: pick one in \"Players and teams\" (PROVISIONAL: the engine reads a missing race as 4, open)", no_race.join(", ")),
        ));
    }
    out.push(check(doc::name(map).is_some_and(|n| !n.trim().is_empty()), false, "the map has a name (the lobby row shows it)", "set the name in Map settings"));
    out
}

/// Every blocking item holds.
pub fn lists_and_starts(map: &Value) -> bool {
    checklist(map).iter().all(|c| c.ok || !c.blocking)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// the map's own end rules (`victory`)

/// What a victory condition's field holds, which decides the widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VField {
    /// `objects`: world object ids and script group names.
    Objects,
    /// `Team Name`: a `teams` `name_id`.
    Team,
    /// `Points`: a whole number.
    Points,
    /// `Exists?`: `"TRUE"` or `"FALSE"`.
    Exists,
    /// `polygon`: the corners, game x/y (at least 3).
    Polygon,
    /// `Equivalence` (the engine's `Equivalence::NAMES`: "Greater Than", "Less Than", "Equal To") and `Damage Percent`.
    Damage,
}

/// A condition `Type` the engine's skirmish end rules read, and the fields it reads.
pub struct VType {
    pub name: &'static str,
    pub fields: &'static [VField],
}

/// The condition Types `victory.rs` reads (`game/src/victory.rs` `condition`, line 220 to 262). PROVISIONAL: a copy, held to the engine's
/// rule schema (each is a condition there) by `tests/map_skirmish.rs`; the engine has no list to ask. A Type outside it makes the
/// map's whole `victory` list unusable (`MapDoc`'s doc), which is why the form offers only these.
pub const VICTORY_TYPES: &[VType] = &[
    VType { name: "Skirmish Game Complete", fields: &[] },
    VType { name: "Team Game Complete", fields: &[] },
    VType { name: "No Team Has Ships", fields: &[] },
    VType { name: "No Human Controlled Ships Remain", fields: &[] },
    VType { name: "Team X has no ships", fields: &[VField::Team] },
    VType { name: "Team has X points", fields: &[VField::Team, VField::Points] },
    VType { name: "Group Destroyed", fields: &[VField::Objects] },
    VType { name: "Team has captured a ship from Group/Unit", fields: &[VField::Objects] },
    VType { name: "Team has destroyed a ship from Group/Unit", fields: &[VField::Objects] },
    VType { name: "Does Group Contain Unit Name", fields: &[VField::Objects, VField::Exists] },
    VType { name: "Enter Volume", fields: &[VField::Objects, VField::Polygon] },
    VType { name: "Group/Unit Vital Section has >,<,= X damage", fields: &[VField::Objects, VField::Damage] },
];

/// The victory Type `name`.
pub fn victory_type(name: &str) -> Option<&'static VType> {
    VICTORY_TYPES.iter().find(|t| t.name == name)
}

/// The key a field is written under.
fn field_keys(f: VField) -> &'static [&'static str] {
    match f {
        VField::Objects => &["objects"],
        VField::Team => &["Team Name"],
        VField::Points => &["Points"],
        VField::Exists => &["Exists?"],
        VField::Polygon => &["polygon"],
        VField::Damage => &["Equivalence", "Damage Percent"],
    }
}

/// A fresh condition of Type `kind`, with a value for every field its reader needs (a missing one makes the whole `victory` list
/// unusable). Teams come from `map`.
pub fn new_condition(kind: &str, map: &Value) -> Value {
    let mut c = Map::new();
    c.insert("Type".into(), kind.into());
    for f in victory_type(kind).map_or(&[][..], |t| t.fields) {
        match f {
            VField::Objects => c.insert("objects".into(), json!([])),
            VField::Team => c.insert("Team Name".into(), doc::team_ids(map).first().copied().unwrap_or("").into()),
            VField::Points => c.insert("Points".into(), json!(100)),
            VField::Exists => c.insert("Exists?".into(), schema_form::bool_value(true)),
            VField::Polygon => c.insert("polygon".into(), json!([[-100.0, -100.0], [100.0, -100.0], [0.0, 100.0]])),
            VField::Damage => {
                c.insert("Equivalence".into(), Equivalence::NAMES[0].into());
                c.insert("Damage Percent".into(), json!(0.5))
            }
        };
    }
    Value::Object(c)
}

fn victory_edit(project: &mut Project, id: &str, label: &str, edits: Vec<Edit>) -> Result<(), String> {
    map_step(project, id, label, edits)
}

fn current_map<'a>(project: &'a Project, id: &str) -> Result<&'a Value, String> {
    maps::merged_view(project, id).ok_or_else(|| format!("map '{id}' is not open"))
}

/// Adds an end rule (first of "Victory N" names free) holding one condition, "Skirmish Game Complete" (the survivors are one team);
/// returns its index. An empty `if` is a rule that always holds, so a rule never starts empty.
pub fn add_victory_rule(project: &mut Project, id: &str) -> Result<usize, String> {
    let map = current_map(project, id)?;
    let names: Vec<&str> = doc::victory_rules(map).iter().filter_map(doc::victory_name).collect();
    let at = doc::victory_rules(map).len();
    let rule = json!({"name": unique_name(&names, "Victory"), "if": [new_condition("Skirmish Game Complete", map)]});
    let edit = if map.get(doc::KEY_VICTORY).is_some_and(Value::is_array) {
        Edit::set(pointer(&[doc::KEY_VICTORY, &at.to_string()]), rule)
    } else {
        Edit::set(pointer(&[doc::KEY_VICTORY]), json!([rule]))
    };
    victory_edit(project, id, "Add end rule", vec![edit])?;
    Ok(at)
}

/// Deletes end rule `index`.
pub fn delete_victory_rule(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    let n = doc::victory_rules(current_map(project, id)?).len();
    if index >= n {
        return Err(format!("no end rule {index}"));
    }
    victory_edit(project, id, "Delete end rule", vec![Edit::remove(pointer(&[doc::KEY_VICTORY, &index.to_string()]))])
}

/// The rule's name (informational).
pub fn set_victory_name(project: &mut Project, id: &str, rule: usize, name: &str) -> Result<(), String> {
    let rel = maps::map_rel(id)?;
    maps::open_map(project, id)?;
    form_step(project, &rel, "Rename end rule", vec![Edit::set(pointer(&[doc::KEY_VICTORY, &rule.to_string(), "name"]), json!(name))], true)
}

/// The winning team (`winner`, a team `name_id`); `None` removes it: the survivors win when they are one team.
pub fn set_victory_winner(project: &mut Project, id: &str, rule: usize, team: Option<&str>) -> Result<(), String> {
    let at = pointer(&[doc::KEY_VICTORY, &rule.to_string(), "winner"]);
    victory_edit(project, id, "End rule winner", vec![team.map_or_else(|| Edit::remove(at.clone()), |t| Edit::set(at.clone(), json!(t)))])
}

/// The end messages (`end.winner` and `end.loser`, string ids such as `IDGS_...`; empty text leaves it out).
pub fn set_victory_end(project: &mut Project, id: &str, rule: usize, winners: bool, text: &str) -> Result<(), String> {
    let map = current_map(project, id)?;
    let r = doc::victory_rules(map).get(rule).ok_or_else(|| format!("no end rule {rule}"))?;
    let mut end = r.get("end").and_then(Value::as_object).cloned().unwrap_or_default();
    let key = if winners { "winner" } else { "loser" };
    if text.trim().is_empty() {
        end.remove(key);
    } else {
        end.insert(key.into(), text.into());
    }
    let at = pointer(&[doc::KEY_VICTORY, &rule.to_string(), "end"]);
    let edit = if end.is_empty() { Edit::remove(at) } else { Edit::set(at, Value::Object(end)) };
    let rel = maps::map_rel(id)?;
    form_step(project, &rel, "End rule message", vec![edit], true)
}

/// Adds a condition of Type `kind` to end rule `rule`; returns its index.
pub fn add_condition(project: &mut Project, id: &str, rule: usize, kind: &str) -> Result<usize, String> {
    if victory_type(kind).is_none() {
        return Err(format!("'{kind}' is not a condition the skirmish end rules read"));
    }
    let map = current_map(project, id)?;
    let r = doc::victory_rules(map).get(rule).ok_or_else(|| format!("no end rule {rule}"))?;
    let at = doc::victory_conditions(r).len();
    let new = new_condition(kind, map);
    let edit = if r.get("if").is_some_and(Value::is_array) {
        Edit::set(pointer(&[doc::KEY_VICTORY, &rule.to_string(), "if", &at.to_string()]), new)
    } else {
        Edit::set(pointer(&[doc::KEY_VICTORY, &rule.to_string(), "if"]), json!([new]))
    };
    victory_edit(project, id, "Add end condition", vec![edit])?;
    Ok(at)
}

/// Removes condition `cond` of end rule `rule`.
pub fn delete_condition(project: &mut Project, id: &str, rule: usize, cond: usize) -> Result<(), String> {
    victory_edit(project, id, "Delete end condition", vec![Edit::remove(pointer(&[doc::KEY_VICTORY, &rule.to_string(), "if", &cond.to_string()]))])
}

/// Changes a condition's Type: the condition is made new (its old fields would not mean the same).
pub fn set_condition_kind(project: &mut Project, id: &str, rule: usize, cond: usize, kind: &str) -> Result<(), String> {
    if victory_type(kind).is_none() {
        return Err(format!("'{kind}' is not a condition the skirmish end rules read"));
    }
    let new = new_condition(kind, current_map(project, id)?);
    victory_edit(project, id, "End condition type", vec![Edit::set(pointer(&[doc::KEY_VICTORY, &rule.to_string(), "if", &cond.to_string()]), new)])
}

/// Writes one field of a condition (`None` removes it). Typing in one field is one undo step.
pub fn set_condition_field(project: &mut Project, id: &str, rule: usize, cond: usize, key: &str, value: Option<Value>) -> Result<(), String> {
    let rel = maps::map_rel(id)?;
    maps::open_map(project, id)?;
    let at = pointer(&[doc::KEY_VICTORY, &rule.to_string(), "if", &cond.to_string(), key]);
    form_step(project, &rel, &format!("End condition {key}"), vec![value.map_or_else(|| Edit::remove(at.clone()), |v| Edit::set(at.clone(), v))], true)
}

// ---- the text forms of list-valued fields ----

/// `1, 2, Convoy` to `[1, 2, "Convoy"]`: whole numbers are object ids, the rest are the script's group names.
pub fn parse_objects(text: &str) -> Value {
    Value::Array(text.split(',').map(str::trim).filter(|t| !t.is_empty()).map(|t| t.parse::<i64>().map_or_else(|_| Value::from(t), Value::from)).collect())
}

/// [`parse_objects`] the other way.
pub fn format_objects(v: &Value) -> String {
    v.as_array().map_or(String::new(), |a| a.iter().map(|o| o.as_i64().map_or_else(|| o.as_str().unwrap_or("?").to_string(), |i| i.to_string())).collect::<Vec<_>>().join(", "))
}

/// `-100,-100; 100,-100; 0,100` to the corner list; `None` when a corner is not two numbers.
pub fn parse_polygon(text: &str) -> Option<Value> {
    let mut out = vec![];
    for corner in text.split(';').map(str::trim).filter(|c| !c.is_empty()) {
        let (x, y) = corner.split_once(',')?;
        out.push(json!([x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?]));
    }
    Some(Value::Array(out))
}

/// [`parse_polygon`] the other way.
pub fn format_polygon(v: &Value) -> String {
    let n = |x: &Value| x.as_f64().map_or("?".to_string(), |f| if f.fract() == 0.0 { format!("{}", f as i64) } else { f.to_string() });
    v.as_array().map_or(String::new(), |a| a.iter().map(|p| format!("{},{}", n(&p[0]), n(&p[1]))).collect::<Vec<_>>().join("; "))
}

// ---------------------------------------------------------------------------------------------------------------------------------
// who decides: the script's `victory`

/// Lets the script's own end rules decide (`"victory": "script"` at the top of `script.json`, creating the file when it is first
/// needed), or hands the end back to the map's `victory` rules (the key is removed; a script nobody else uses stays as it is).
pub fn set_script_decides(project: &mut Project, id: &str, on: bool) -> Result<(), String> {
    if !on && script_of(project, id).is_none_or(|s| s.get(mdoc::SCRIPT_KEY_VICTORY).is_none()) {
        return Ok(());
    }
    let rel = ensure_script(project, id)?;
    let at = pointer(&[mdoc::SCRIPT_KEY_VICTORY]);
    let edit = if on { Edit::set(at, json!(mdoc::SCRIPT_DECIDES)) } else { Edit::remove(at) };
    project.do_step(&rel, if on { "Script decides the end" } else { "Map rules decide the end" }, vec![edit])
}

// ---------------------------------------------------------------------------------------------------------------------------------
// script.json rules

/// The rule Types the skirmish captain AI does not carry out yet: the engine stores them and nothing steers (docs/modding.md "Skirmish
/// map scripts": attack and ram orders, border zones, and `OP Follow Group`). PROVISIONAL (engine doc): a copy, held to the rule
/// schema by `tests/map_skirmish.rs`.
pub const STORED_ONLY: &[&str] = &["GroupA to attack GroupB", "GroupA to Ram GroupB", "Set Group/Unit Border Zone", "Set Group/Unit Border Zone Return Point", "OP Follow Group"];

/// The names of the rule Types a skirmish script cannot use (`campaign_only` in the engine's rule schema).
pub fn campaign_only_types() -> BTreeSet<String> {
    openprocyon::authoring::rule_schema()["types"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["campaign_only"] == true)
        .filter_map(|t| t["name"].as_str().map(str::to_string))
        .collect()
}

/// Adds a rule to the script (creating `script.json` in the mod on first use); returns its index. A new rule is active, runs once and
/// has nothing in it.
pub fn add_rule(project: &mut Project, id: &str) -> Result<usize, String> {
    let rel = ensure_script(project, id)?;
    rules::add_rule(project, &Target::script(&rel))
}

/// Deletes script rule `index`.
pub fn delete_rule(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    let rel = script_rel(id)?;
    rules::delete_rule(project, &Target::script(&rel), index)
}

/// Renames script rule `index` (typing joins one step).
pub fn rename_rule(project: &mut Project, id: &str, index: usize, name: &str) -> Result<(), String> {
    let rel = script_rel(id)?;
    rules::rename_rule(project, &Target::script(&rel), index, name)
}

/// `Run Once` or `Is Active` of a script rule: JSON booleans (the rule loader reads them as booleans, unlike a Type's fields).
pub fn set_rule_flag(project: &mut Project, id: &str, index: usize, key: &str, on: bool) -> Result<(), String> {
    let rel = script_rel(id)?;
    rules::set_rule_flag(project, &Target::script(&rel), index, key, on)
}

/// Appends `item` to the conditions or the actions of script rule `index`; returns its position.
pub fn add_item(project: &mut Project, id: &str, index: usize, is_condition: bool, item: Value) -> Result<usize, String> {
    let rel = script_rel(id)?;
    rules::add_item(project, &Target::script(&rel), index, is_condition, item)
}

/// Removes condition or action `item` of script rule `index`.
pub fn remove_item(project: &mut Project, id: &str, index: usize, is_condition: bool, item: usize) -> Result<(), String> {
    let rel = script_rel(id)?;
    rules::remove_item(project, &Target::script(&rel), index, is_condition, item)
}

/// Replaces condition or action `item` of script rule `index` with what its form made. `typing`: one box was typed in, so the edit
/// joins the typing before it.
pub fn replace_item(project: &mut Project, id: &str, index: usize, is_condition: bool, item: usize, new: Value, typing: bool) -> Result<(), String> {
    let rel = script_rel(id)?;
    rules::replace_item(project, &Target::script(&rel), index, is_condition, item, new, typing)
}

/// The engine's `--check-mission` of the map's script, as a skirmish script (a campaign-only Type is an error there).
pub fn check_script(script: &Value, map: &Value, media: &Value) -> Vec<Finding> {
    openprocyon::authoring::check_mission(script, map, media, true)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// media.json

/// The three tables of `media.json` (the editor's one definition is [`mdoc::MediaTable`]).
pub use mdoc::MediaTable;

fn table_title(table: MediaTable) -> &'static str {
    match table {
        MediaTable::Speech => "Speech events (subtitle, sound)",
        MediaTable::Messages => "Messages (end texts)",
        MediaTable::ShipNames => "Ship names",
    }
}

/// The entries of one table, in the file's order (empty when missing).
pub fn media_entries(media: &Value, table: MediaTable) -> Vec<(&str, &Value)> {
    media::entries(media, table)
}

/// Adds an entry named `key` to a table (creating `media.json` in the mod on first use). Refuses an empty or repeated key.
pub fn add_media(project: &mut Project, id: &str, table: MediaTable, key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("name the entry first".into());
    }
    let rel = ensure_media(project, id)?;
    media::add(project, &rel, table, key)
}

/// Removes an entry.
pub fn remove_media(project: &mut Project, id: &str, table: MediaTable, key: &str) -> Result<(), String> {
    let rel = media_rel(id)?;
    media::remove(project, &rel, table, key)
}

/// Sets a message or a ship name (`field` `None`) or one field of a speech entry (`text`, `file`, `duration`; an empty text removes
/// all but `text`). Typing joins one step.
pub fn set_media(project: &mut Project, id: &str, table: MediaTable, key: &str, field: Option<&str>, value: Option<Value>) -> Result<(), String> {
    let rel = media_rel(id)?;
    media::set(project, &rel, table, key, field, value)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// playing

/// The `--lobby` launch of map `id` from the folder `dir` with the settings `lobby`.
pub fn lobby_launch(dir: &std::path::Path, id: &str, lobby: &Lobby) -> Launch {
    let mut l = Launch::new(dir, id);
    l.lobby = Some(lobby.clone());
    l
}

// ---------------------------------------------------------------------------------------------------------------------------------
// the panel

/// The section's own state: nothing the document holds.
#[derive(Resource)]
pub struct Panel {
    schema: Schema,
    campaign_only: BTreeSet<String>,
    /// The victory rule and the script rule being edited.
    vrule: usize,
    rule: usize,
    /// Text being typed in a list-valued field, by field: kept so a half-typed `1, ` is not rewritten under the cursor.
    buffers: HashMap<String, String>,
    new_key: String,
    lobby: Lobby,
    child: Option<Child>,
    info: String,
    error: Option<String>,
    cache: Option<(Value, Value, Value, Vec<Finding>)>,
}

impl Default for Panel {
    fn default() -> Panel {
        Panel {
            schema: Schema::engine(),
            campaign_only: campaign_only_types(),
            vrule: 0,
            rule: 0,
            buffers: HashMap::new(),
            new_key: String::new(),
            lobby: Lobby::default(),
            child: None,
            info: String::new(),
            error: None,
            cache: None,
        }
    }
}

impl Panel {
    /// Forgets what belongs to the map shown (another map was opened).
    pub fn reset(&mut self) {
        self.vrule = 0;
        self.rule = 0;
        self.buffers.clear();
        self.new_key.clear();
        self.info.clear();
        self.error = None;
        self.cache = None;
    }

    /// The last refusal or failure shown under the section.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn run<T>(&mut self, r: Result<T, String>) -> Option<T> {
        match r {
            Ok(v) => {
                self.error = None;
                Some(v)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    fn findings(&mut self, script: &Value, map: &Value, media: &Value) -> &[Finding] {
        if self.cache.as_ref().is_none_or(|(s, m, d, _)| s != script || m != map || d != media) {
            self.cache = Some((script.clone(), map.clone(), media.clone(), if script.is_null() { vec![] } else { check_script(script, map, media) }));
        }
        &self.cache.as_ref().expect("filled above").3
    }
}

const RED: egui::Color32 = egui::Color32::LIGHT_RED;

/// A text box for a list-valued field: shows what is typed, writes what it parses to (`None` from `parse` is not written and shown
/// red). Returns the new value when it changed.
fn parsed_text(ui: &mut egui::Ui, buffers: &mut HashMap<String, String>, key: &str, current: &Value, fmt: fn(&Value) -> String, parse: fn(&str) -> Option<Value>) -> Option<Value> {
    let canonical = fmt(current);
    let buf = buffers.entry(key.to_string()).or_insert_with(|| canonical.clone());
    // The document moved on without this box (undo, another map): show it again.
    if parse(buf).as_ref().map(fmt).as_deref() != Some(canonical.as_str()) && !ui.memory(|m| m.has_focus(egui::Id::new(key))) {
        *buf = canonical;
    }
    let ok = parse(buf).is_some();
    let r = ui.add(egui::TextEdit::singleline(buf).id(egui::Id::new(key)).desired_width(220.0).text_color_opt((!ok).then_some(RED)));
    if r.changed() { parse(buf) } else { None }
}

/// The section's contents. `id` is the map shown (open in `project`). Everything it changes is a step of the project; nothing is
/// written to disk (Save does that, and "Play in lobby" plays a copy of the unsaved mod).
pub fn show(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str) {
    let opened = open_documents(project, id);
    st.run(opened);
    let Some(map) = maps::merged_view(project, id).cloned() else { return };
    let script = script_of(project, id).cloned().unwrap_or(Value::Null);
    let media = media_of(project, id).cloned().unwrap_or(Value::Null);

    lobby_listing(ui, st, project, id, &map);
    ui.separator();
    victory_section(ui, st, project, id, &map, &script);
    ui.separator();
    script_section(ui, st, project, id, &map, &script, &media);
    ui.separator();
    media_section(ui, st, project, id, &media);
    ui.separator();
    play_section(ui, st, project, id, &map);
    if let Some(e) = &st.error {
        ui.colored_label(RED, e);
    } else if !st.info.is_empty() {
        ui.label(&st.info);
    }
}

fn lobby_listing(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str, map: &Value) {
    ui.strong("Lobby listing");
    let mut on = doc::skirmish(map);
    if ui.checkbox(&mut on, "Skirmish map").on_hover_text("\"skirmish\": the skirmish lobby lists the map. A retail campaign map never gets it.").changed() {
        let r = set_skirmish(project, id, on);
        st.run(r);
    }
    let mut open = doc::assemble(map);
    if ui.checkbox(&mut open, "Open map (the players assemble their fleets)").on_hover_text("\"assemble\". Off: a historical map whose fleets are the placed ships.").changed() {
        let r = set_assemble(project, id, open);
        st.run(r);
    }
    for c in checklist(map) {
        let (mark, colour) = if c.ok {
            ("[x]", ui.visuals().text_color())
        } else if c.blocking {
            ("[ ]", RED)
        } else {
            ("[ ]", ui.visuals().warn_fg_color)
        };
        ui.colored_label(colour, format!("{mark} {}", c.text));
        if !c.hint.is_empty() {
            ui.weak(format!("      {}", c.hint));
        }
    }
}

fn victory_section(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str, map: &Value, script: &Value) {
    ui.strong("How the game ends");
    let decides = mdoc::script_decides(script);
    ui.label(
        "The map's own end rules (below) decide, as in the retail maps, unless the script says \"victory\": \"script\". Then the script's \
         Team X Wins, Remaining Team Wins and End Game actions (and its tasks with a team) decide, and the map's rules are still tested first.",
    );
    ui.horizontal(|ui| {
        if ui.radio(!decides, "Map rules decide").clicked() && decides {
            let r = set_script_decides(project, id, false);
            st.run(r);
        }
        if ui.radio(decides, "Script decides").clicked() && !decides {
            let r = set_script_decides(project, id, true);
            st.run(r);
        }
    });
    let rules = doc::victory_rules(map);
    if rules.is_empty() {
        ui.label("The map has no end rules: the game ends when no enemy ships are left (the engine's plain rule).");
    }
    st.vrule = st.vrule.min(rules.len().saturating_sub(1));
    let names: Vec<String> = rules.iter().enumerate().map(|(i, r)| format!("{}. {}", i + 1, doc::victory_name(r).unwrap_or("(unnamed)"))).collect();
    ui.horizontal(|ui| {
        if !names.is_empty() {
            egui::ComboBox::from_id_salt("t10-vrule").selected_text(&names[st.vrule]).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut st.vrule, i, n);
                }
            });
        }
        if ui.button("Add end rule").clicked()
            && let Some(at) = st.run(add_victory_rule(project, id))
        {
            st.vrule = at;
        }
        if ui.add_enabled(!names.is_empty(), egui::Button::new("Delete end rule")).clicked() {
            let r = delete_victory_rule(project, id, st.vrule);
            st.run(r);
        }
    });
    let Some(rule) = rules.get(st.vrule) else { return };
    let ri = st.vrule;
    let teams = doc::team_ids(map);
    ui.horizontal(|ui| {
        ui.label("Name");
        let mut n = doc::victory_name(rule).unwrap_or_default().to_string();
        if ui.text_edit_singleline(&mut n).changed() {
            let r = set_victory_name(project, id, ri, &n);
            st.run(r);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Winner");
        let cur = doc::victory_winner(rule);
        egui::ComboBox::from_id_salt("t10-winner").selected_text(cur.unwrap_or("the survivors, when they are one team")).show_ui(ui, |ui| {
            if ui.selectable_label(cur.is_none(), "the survivors, when they are one team").clicked() {
                let r = set_victory_winner(project, id, ri, None);
                st.run(r);
            }
            for t in teams.iter().filter(|t| !t.is_empty()) {
                if ui.selectable_label(cur == Some(*t), *t).clicked() {
                    let r = set_victory_winner(project, id, ri, Some(t));
                    st.run(r);
                }
            }
        });
    });
    for (winners, label) in [(true, "Winners' message"), (false, "Losers' message")] {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut t = doc::victory_end(rule, winners).unwrap_or_default().to_string();
            if ui.add(egui::TextEdit::singleline(&mut t).hint_text("IDGS_... (empty: the default line)")).changed() {
                let r = set_victory_end(project, id, ri, winners, &t);
                st.run(r);
            }
        });
    }
    ui.label("Conditions (all must hold)");
    let mut remove = None;
    for (ci, cond) in doc::victory_conditions(rule).iter().enumerate() {
        let kind = doc::condition_kind(cond).unwrap_or("?");
        egui::CollapsingHeader::new(format!("{}. {kind}", ci + 1)).id_salt(("t10-cond", id, ri, ci)).default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Type");
                egui::ComboBox::from_id_salt(("t10-ckind", ri, ci)).selected_text(kind).width(300.0).show_ui(ui, |ui| {
                    for t in VICTORY_TYPES {
                        if ui.selectable_label(t.name == kind, t.name).clicked() && t.name != kind {
                            let r = set_condition_kind(project, id, ri, ci, t.name);
                            st.run(r);
                        }
                    }
                });
            });
            let Some(def) = victory_type(kind) else {
                ui.colored_label(RED, format!("'{kind}' is not a condition the end rules read: the whole victory list of the map is unusable until it is changed."));
                if ui.small_button("Remove").clicked() {
                    remove = Some(ci);
                }
                return;
            };
            for f in def.fields {
                victory_field(ui, st, project, id, (ri, ci), cond, *f, &teams);
            }
            if ui.small_button("Remove").clicked() {
                remove = Some(ci);
            }
        });
    }
    if let Some(ci) = remove {
        let r = delete_condition(project, id, ri, ci);
        st.run(r);
    }
    ui.horizontal(|ui| {
        ui.label("Add condition");
        egui::ComboBox::from_id_salt("t10-cadd").selected_text("choose a Type").width(300.0).show_ui(ui, |ui| {
            for t in VICTORY_TYPES {
                if ui.selectable_label(false, t.name).clicked() {
                    let r = add_condition(project, id, ri, t.name);
                    st.run(r);
                }
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn victory_field(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str, at: (usize, usize), cond: &Value, f: VField, teams: &[&str]) {
    let (ri, ci) = at;
    let key = field_keys(f)[0];
    let set = |st: &mut Panel, project: &mut Project, key: &str, v: Value| {
        let r = set_condition_field(project, id, ri, ci, key, Some(v));
        st.run(r);
    };
    ui.horizontal(|ui| match f {
        VField::Objects => {
            ui.label("Objects").on_hover_text("World object ids, and groups of the map's script, separated by commas: 12, 14, Convoy");
            let cur = cond.get(key).cloned().unwrap_or_else(|| json!([]));
            let bk = format!("t10-{id}-{ri}-{ci}-objects");
            if let Some(v) = parsed_text(ui, &mut st.buffers, &bk, &cur, format_objects, |t| Some(parse_objects(t))) {
                set(st, project, key, v);
            }
        }
        VField::Team => {
            ui.label("Team");
            let cur = cond.get(key).and_then(Value::as_str).unwrap_or_default();
            egui::ComboBox::from_id_salt(("t10-team", ri, ci)).selected_text(cur).show_ui(ui, |ui| {
                for t in teams.iter().filter(|t| !t.is_empty()) {
                    if ui.selectable_label(cur == *t, *t).clicked() {
                        set(st, project, key, json!(t));
                    }
                }
            });
            if teams.iter().all(|t| t.is_empty()) {
                ui.colored_label(RED, "the map has no team yet");
            }
        }
        VField::Points => {
            ui.label("Points");
            let mut n = cond.get(key).and_then(Value::as_i64).unwrap_or(0);
            if ui.add(egui::DragValue::new(&mut n).range(0..=1_000_000)).changed() {
                set(st, project, key, json!(n));
            }
        }
        VField::Exists => {
            ui.label("Exists?");
            let mut on = schema_form::is_true(cond.get(key).unwrap_or(&Value::Null));
            if ui.checkbox(&mut on, "the unit is there").changed() {
                set(st, project, key, schema_form::bool_value(on));
            }
        }
        VField::Polygon => {
            ui.label("Corners").on_hover_text("Game x,y of each corner, at least three, separated by semicolons: -100,-100; 100,-100; 0,100");
            let cur = cond.get(key).cloned().unwrap_or_else(|| json!([]));
            let bk = format!("t10-{id}-{ri}-{ci}-polygon");
            if let Some(v) = parsed_text(ui, &mut st.buffers, &bk, &cur, format_polygon, parse_polygon) {
                set(st, project, key, v);
            }
        }
        VField::Damage => {
            ui.label("Damage is");
            let cur = cond.get("Equivalence").and_then(Value::as_str).unwrap_or(Equivalence::NAMES[0]);
            egui::ComboBox::from_id_salt(("t10-eq", ri, ci)).selected_text(cur).width(110.0).show_ui(ui, |ui| {
                for e in Equivalence::NAMES {
                    if ui.selectable_label(cur == e, e).clicked() {
                        set(st, project, "Equivalence", json!(e));
                    }
                }
            });
            let mut x = cond.get("Damage Percent").and_then(Value::as_f64).unwrap_or(0.0);
            if ui
                .add(egui::DragValue::new(&mut x).speed(0.01).range(0.0..=1.0))
                .on_hover_text("A fraction, 0 to 1 (PROVISIONAL: the engine compares the base's damage fraction with it)")
                .changed()
            {
                set(st, project, "Damage Percent", json!(x));
            }
        }
    });
}

fn script_section(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str, map: &Value, script: &Value, media: &Value) {
    ui.strong("Script rules (script.json)");
    if script.is_null() {
        ui.label("This map has no script. Adding a rule creates maps/<id>/script.json in the mod.");
    }
    let count = mdoc::script_rules(script).len();
    st.rule = st.rule.min(count.saturating_sub(1));
    let names: Vec<String> =
        mdoc::script_rules(script).iter().enumerate().map(|(i, r)| format!("{}. {}", i + 1, r.get("name").and_then(Value::as_str).unwrap_or("(unnamed)"))).collect();
    ui.horizontal(|ui| {
        if !names.is_empty() {
            egui::ComboBox::from_id_salt("t10-rule").selected_text(&names[st.rule]).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut st.rule, i, n);
                }
            });
        }
        if ui.button("Add rule").clicked()
            && let Some(at) = st.run(add_rule(project, id))
        {
            st.rule = at;
        }
        if ui.add_enabled(count > 0, egui::Button::new("Delete rule")).clicked() {
            let r = delete_rule(project, id, st.rule);
            st.run(r);
        }
    });
    let Some(rule) = mdoc::script_rules(script).get(st.rule) else { return };
    let ri = st.rule;
    let names_for_forms = Names::collect(script, map, media);
    let findings: Vec<Finding> = st.findings(script, map, media).to_vec();
    let errors = findings.iter().filter(|f| f.level == crate::checks::Level::Error).count();
    ui.label(format!("{errors} errors, {} warnings in the script (the engine's --check-mission for a skirmish script)", findings.len() - errors));
    for p in crate::checks::for_rule(&findings, ri) {
        ui.colored_label(RED, &p.message);
    }
    ui.horizontal(|ui| {
        ui.label("Name");
        let mut n = rule.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        if ui.text_edit_singleline(&mut n).changed() {
            let r = rename_rule(project, id, ri, &n);
            st.run(r);
        }
    });
    for key in ["Run Once", "Is Active"] {
        let mut on = rule.get(key).and_then(Value::as_bool).unwrap_or(true);
        if ui.checkbox(&mut on, key).changed() {
            let r = set_rule_flag(project, id, ri, key, on);
            st.run(r);
        }
    }
    for is_cond in [true, false] {
        ui.separator();
        ui.strong(if is_cond { "Conditions" } else { "Actions" });
        let items = rule.get(mdoc::rule_list_key(is_cond)).and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
        let mut remove = None;
        for (i, item) in items.iter().enumerate() {
            let ty = item.get("Type").and_then(Value::as_str).unwrap_or("?");
            let mine = crate::checks::for_item(&findings, ri, mdoc::rule_list_key(is_cond), i);
            let head = if mine.is_empty() { ty.to_string() } else { format!("{ty}  ({} problem{})", mine.len(), if mine.len() == 1 { "" } else { "s" }) };
            let wid = egui::Id::new(("t10-item", id, ri, is_cond, i));
            egui::CollapsingHeader::new(format!("{}. {head}", i + 1)).id_salt(wid).default_open(true).show(ui, |ui| {
                if st.campaign_only.contains(ty) {
                    ui.colored_label(RED, "campaign only: a skirmish script cannot use this Type (the rule runs and does nothing)");
                }
                if STORED_ONLY.contains(&ty) {
                    ui.colored_label(ui.visuals().warn_fg_color, "stored but not carried out: the skirmish captain AI does not read this order yet (PROVISIONAL, engine doc)");
                }
                for p in mine.iter().filter(|p| p.field == "Type" || p.field.is_empty()) {
                    ui.colored_label(RED, &p.message);
                }
                let mut edited = item.clone();
                if schema_form::item_form(ui, &st.schema, &names_for_forms, &mut edited, is_cond, &mine, wid) && &edited != item {
                    let typing = edited.get("Type") == item.get("Type");
                    let r = replace_item(project, id, ri, is_cond, i, edited, typing);
                    st.run(r);
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            let r = remove_item(project, id, ri, is_cond, i);
            st.run(r);
        }
        let mut pick = None;
        ui.horizontal(|ui| {
            ui.label(if is_cond { "Add condition" } else { "Add action" });
            egui::ComboBox::from_id_salt(("t10-add", is_cond)).selected_text("choose a Type").width(300.0).show_ui(ui, |ui| {
                for t in st.schema.types.iter().filter(|t| t.is_condition == is_cond && !st.campaign_only.contains(&t.name)) {
                    if ui.selectable_label(false, &t.name).clicked() {
                        pick = Some(schema_form::new_item(t));
                    }
                }
            });
        });
        if let Some(new) = pick {
            let r = add_item(project, id, ri, is_cond, new);
            st.run(r);
        }
    }
}

fn media_section(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str, media: &Value) {
    ui.strong("Words (media.json)");
    if media.is_null() {
        ui.label("This map has no media.json. Adding an entry creates maps/<id>/media.json in the mod; without one a speech event does nothing.");
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.new_key).hint_text("entry name").desired_width(160.0));
        for t in MediaTable::ALL {
            if ui.button(format!("Add to {}", t.key())).clicked() {
                let key = st.new_key.clone();
                if st.run(add_media(project, id, t, &key)).is_some() {
                    st.new_key.clear();
                }
            }
        }
    });
    for t in MediaTable::ALL {
        let entries = media_entries(media, t);
        if entries.is_empty() {
            continue;
        }
        egui::CollapsingHeader::new(format!("{} ({})", table_title(t), entries.len())).id_salt(("t10-media", id, t.key())).default_open(false).show(ui, |ui| {
            let mut remove = None;
            for (key, v) in entries {
                ui.horizontal_wrapped(|ui| {
                    ui.label(key);
                    match t {
                        MediaTable::Speech => {
                            for (field, label) in [("text", "text"), ("file", "sound file")] {
                                let mut s = v.get(field).and_then(Value::as_str).unwrap_or_default().to_string();
                                ui.label(label);
                                if ui.add(egui::TextEdit::singleline(&mut s).desired_width(160.0)).changed() {
                                    let value = (!s.is_empty() || field == "text").then(|| json!(s));
                                    let r = set_media(project, id, t, key, Some(field), value);
                                    st.run(r);
                                }
                            }
                            let mut d = v.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
                            ui.label("seconds");
                            if ui.add(egui::DragValue::new(&mut d).speed(0.1).range(0.0..=600.0)).on_hover_text("0: text only, no sound").changed() {
                                let r = set_media(project, id, t, key, Some("duration"), (d > 0.0).then(|| json!(d)));
                                st.run(r);
                            }
                        }
                        _ => {
                            let mut s = v.as_str().unwrap_or_default().to_string();
                            if ui.add(egui::TextEdit::singleline(&mut s).desired_width(300.0)).changed() {
                                let r = set_media(project, id, t, key, None, Some(json!(s)));
                                st.run(r);
                            }
                        }
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(key.to_string());
                    }
                });
            }
            if let Some(k) = remove {
                let r = remove_media(project, id, t, &k);
                st.run(r);
            }
        });
    }
}

fn play_section(ui: &mut egui::Ui, st: &mut Panel, project: &mut Project, id: &str, map: &Value) {
    ui.strong("Play");
    let running = st.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    if !running {
        st.child = None;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Computers");
        ui.add(egui::DragValue::new(&mut st.lobby.computers).range(1..=7));
        ui.label("Points");
        ui.add(egui::DragValue::new(&mut st.lobby.points).range(0..=100_000).speed(10));
        ui.label("Fleet limit");
        ui.add(egui::DragValue::new(&mut st.lobby.limit).range(1..=100));
    });
    let ready = lists_and_starts(map);
    let tip = if ready {
        format!("runs {} --mod <a copy of the mod with your unsaved edits> --lobby ...", launch::game_bin().display())
    } else {
        "the lobby checklist above has something missing".into()
    };
    if ui.add_enabled(!running, egui::Button::new("Play in lobby")).on_hover_text(tip).clicked() {
        if !ready {
            st.error = Some("The map would not list or start in the lobby yet: see the checklist.".into());
        } else {
            let started = project
                .check_dir()
                .and_then(|dir| lobby_launch(&dir, id, &st.lobby).spawn().map_err(|e| format!("cannot start {}: {e} (set PROCYON_GAME_BIN)", launch::game_bin().display())));
            if let Some(child) = st.run(started) {
                st.child = Some(child);
                st.info = format!("Game started through the lobby on {id}. The lobby opens on the map.");
            }
        }
    }
    if running {
        ui.label("game running");
    }
}
