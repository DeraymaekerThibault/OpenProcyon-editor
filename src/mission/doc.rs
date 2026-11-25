//! Accessors and pointer builders over a `mission.json` and a `media.json` held as [`serde_json::Value`]s: the ONE place in the
//! editor that spells a key of either file (docs/phase5.md, "Rule while the format moves"). Read-only; edits go through
//! [`crate::edit::Edit`] with the JSON pointers built here. When the format changes, this file changes and nothing else does.
//!
//! The keys were read from the engine (`docs/missions.md` sections 3 to 8, `game/src/campaign.rs`, `scenario/spawn.rs`,
//! `environment.rs`, `rules/`) and are PROVISIONAL as a format: neither file has a `format` key or a schema yet (docs/phase5.md,
//! Status). A rule's own fields (`Type` and what follows) are not named here: they come from `authoring::rule_schema()`. The names
//! in `rules[]` that the editor must read without the schema (`name`) are below.
//!
//! The nebula, volume and `script.json` / `media.json` names that Phase 4 kept in `map::doc` live here and are re-exported there.

use serde_json::{Map, Value};

use crate::edit::pointer;

// ---- the keys ----

/// Top-level keys of `mission.json`.
pub const KEY_PLAYERS: &str = "players";
pub const KEY_ALLIANCES: &str = "alliances";
pub const KEY_PLAYER_START: &str = "player_start";
pub const KEY_SHIPS: &str = "ships";
pub const KEY_GROUPS: &str = "groups";
pub const KEY_VOLUMES: &str = "volumes";
pub const KEY_PATHS: &str = "paths";
pub const KEY_POINTS: &str = "points";
pub const KEY_ENVIRONMENT: &str = "environment";
pub const KEY_TASKS: &str = "tasks";
pub const KEY_RULES: &str = "rules";
pub const KEY_MAP_TEXT: &str = "map_text";
/// Engine-added (PROVISIONAL): `"victory": "objectives"` lets the required tasks end the mission; the two messages are ids into
/// `media.json` `messages`.
pub const KEY_VICTORY: &str = "victory";
pub const KEY_VICTORY_MESSAGE: &str = "victory_message";
pub const KEY_DEFEAT_MESSAGE: &str = "defeat_message";
/// `victory`'s value that hands the ending to the tasks.
pub const VICTORY_OBJECTIVES: &str = "objectives";

/// Keys inside `environment`.
pub const ENV_POINT_SETS: &str = "point_sets";
pub const ENV_NEBULAE: &str = "nebulae";
pub const ENV_ANIMALS: &str = "animals";

/// Keys of a `ships[]` entry.
pub const SHIP_ID: &str = "id";
pub const SHIP_TYPE: &str = "type";
pub const SHIP_GROUP: &str = "group";
pub const SHIP_NAME: &str = "name";
pub const SHIP_PRIMARY: &str = "primary";
pub const SHIP_POS: &str = "pos";
pub const SHIP_ROT: &str = "rot";

/// Keys of `player_start`.
pub const START_NAME: &str = "name";
pub const START_POS: &str = "start";
pub const START_FORWARD: &str = "forward";

/// Keys of a `points[]` entry (objective markers).
pub const POINT_NAME: &str = "Name";
pub const POINT_POSITION: &str = "Position";
/// Key of the position inside a point set's entry.
pub const POINT_SET_POS: &str = "pos";

/// Keys of a `tasks[]` entry; the last three are engine-added (PROVISIONAL, docs/phase5.md B2).
pub const TASK_NAME: &str = "Name";
pub const TASK_TEXT: &str = "text";
pub const TASK_ACTIVE: &str = "Active";
pub const TASK_COMPLETE_WHEN: &str = "complete_when";
pub const TASK_FAIL_WHEN: &str = "fail_when";
pub const TASK_REQUIRED: &str = "required";

/// Keys of a `map_text[]` entry.
pub const MAP_TEXT_NAME: &str = "name";
pub const MAP_TEXT_TEXT: &str = "text";
pub const MAP_TEXT_POS: &str = "pos";
pub const MAP_TEXT_VISIBLE: &str = "visible";

/// Key of an `environment.animals[]` entry that ties it to a group (each animal needs one).
pub const ANIMAL_GROUP: &str = "group";

/// A rule's own name (the key a keyed merge uses, `project::keyed`); every other rule field comes from the rule schema.
pub const RULE_NAME: &str = "name";

/// The three tables of `media.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaTable {
    Speech,
    Messages,
    ShipNames,
}

impl MediaTable {
    pub const ALL: [MediaTable; 3] = [MediaTable::Speech, MediaTable::Messages, MediaTable::ShipNames];

    pub fn key(self) -> &'static str {
        match self {
            MediaTable::Speech => "speech",
            MediaTable::Messages => "messages",
            MediaTable::ShipNames => "ship_names",
        }
    }
}

/// A `media.json` with the three empty tables (the game stops without the file).
pub fn new_media() -> Value {
    let mut o = Map::new();
    for t in MediaTable::ALL {
        o.insert(t.key().into(), Value::Object(Map::new()));
    }
    Value::Object(o)
}

// ---- reading ----

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn table<'a>(v: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    v.get(key)?.as_object()
}

fn environment<'a>(mission: &'a Value, key: &str) -> Option<&'a Value> {
    mission.get(KEY_ENVIRONMENT)?.get(key)
}

fn xyz(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() < 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

/// The player names (`players`, strings only).
pub fn players(mission: &Value) -> Vec<&str> {
    list(mission, KEY_PLAYERS).iter().filter_map(Value::as_str).collect()
}

/// The allied pairs (`alliances`, `[[a, b], ...]`); entries that are not two strings are skipped.
pub fn alliances(mission: &Value) -> Vec<[&str; 2]> {
    list(mission, KEY_ALLIANCES)
        .iter()
        .filter_map(|p| {
            let p = p.as_array()?;
            Some([p.first()?.as_str()?, p.get(1)?.as_str()?])
        })
        .collect()
}

/// The `player_start` object.
pub fn player_start(mission: &Value) -> Option<&Value> {
    mission.get(KEY_PLAYER_START).filter(|v| v.is_object())
}

/// The human's player (`player_start.name`).
pub fn player_start_name(mission: &Value) -> Option<&str> {
    player_start(mission)?.get(START_NAME)?.as_str()
}

/// Where the stand-in ship starts (`player_start.start`).
pub fn player_start_pos(mission: &Value) -> Option<[f64; 3]> {
    xyz(player_start(mission)?.get(START_POS)?)
}

/// Which way it faces (`player_start.forward`).
pub fn player_start_forward(mission: &Value) -> Option<[f64; 3]> {
    xyz(player_start(mission)?.get(START_FORWARD)?)
}

/// The mission's ships (empty when missing or not a list).
pub fn ships(mission: &Value) -> &[Value] {
    list(mission, KEY_SHIPS)
}

pub fn ship_id(ship: &Value) -> Option<i64> {
    ship.get(SHIP_ID)?.as_i64()
}

pub fn ship_type(ship: &Value) -> Option<&str> {
    ship.get(SHIP_TYPE)?.as_str()
}

/// The ship's owner: a player name (the key is called `group`).
pub fn ship_group(ship: &Value) -> Option<&str> {
    ship.get(SHIP_GROUP)?.as_str()
}

/// The ship's own name (the game finds `Group,Unit` by this alone).
pub fn ship_name(ship: &Value) -> Option<&str> {
    ship.get(SHIP_NAME)?.as_str()
}

pub fn ship_primary(ship: &Value) -> bool {
    ship.get(SHIP_PRIMARY).and_then(Value::as_bool).unwrap_or(false)
}

pub fn ship_pos(ship: &Value) -> Option<[f64; 3]> {
    xyz(ship.get(SHIP_POS)?)
}

/// The pointer of ship `index`, or of one of its keys.
pub fn ship_pointer(index: usize, key: Option<&str>) -> String {
    let i = index.to_string();
    match key {
        Some(k) => pointer(&[KEY_SHIPS, &i, k]),
        None => pointer(&[KEY_SHIPS, &i]),
    }
}

/// `groups`: `{Name: [ids]}`.
pub fn groups(mission: &Value) -> Option<&Map<String, Value>> {
    table(mission, KEY_GROUPS)
}

/// The ids a group lists (non-numbers skipped; empty for an unknown group).
pub fn group_members(mission: &Value, name: &str) -> Vec<i64> {
    groups(mission).and_then(|g| g.get(name)).and_then(Value::as_array).map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_i64).collect())
}

pub fn groups_pointer() -> String {
    pointer(&[KEY_GROUPS])
}

pub fn group_pointer(name: &str) -> String {
    pointer(&[KEY_GROUPS, name])
}

/// The names of the volumes, in file order.
pub fn volume_names(mission: &Value) -> Vec<&str> {
    table(mission, KEY_VOLUMES).map_or_else(Vec::new, |o| o.keys().map(String::as_str).collect())
}

/// `paths`: `{Name: [[x, y, z], ...]}`.
pub fn paths(mission: &Value) -> Option<&Map<String, Value>> {
    table(mission, KEY_PATHS)
}

/// The path `name` as `[x, y, z]` points (entries that are not three numbers are skipped).
pub fn path_points(mission: &Value, name: &str) -> Option<Vec<[f64; 3]>> {
    Some(paths(mission)?.get(name)?.as_array()?.iter().filter_map(xyz).collect())
}

pub fn paths_pointer() -> String {
    pointer(&[KEY_PATHS])
}

pub fn path_pointer(name: &str) -> String {
    pointer(&[KEY_PATHS, name])
}

/// The objective markers (`points`).
pub fn points(mission: &Value) -> &[Value] {
    list(mission, KEY_POINTS)
}

pub fn point_name(point: &Value) -> Option<&str> {
    point.get(POINT_NAME)?.as_str()
}

pub fn point_position(point: &Value) -> Option<[f64; 3]> {
    xyz(point.get(POINT_POSITION)?)
}

/// The pointer of marker `index`, or of one of its keys.
pub fn point_pointer(index: usize, key: Option<&str>) -> String {
    let i = index.to_string();
    match key {
        Some(k) => pointer(&[KEY_POINTS, &i, k]),
        None => pointer(&[KEY_POINTS, &i]),
    }
}

/// `environment.point_sets`: `{Name: [{pos: [x, y, z]}, ...]}`.
pub fn point_sets(mission: &Value) -> Option<&Map<String, Value>> {
    environment(mission, ENV_POINT_SETS)?.as_object()
}

/// The positions of the point set `name`.
pub fn point_set_positions(mission: &Value, name: &str) -> Option<Vec<[f64; 3]>> {
    let a = point_sets(mission)?.get(name)?.as_array()?;
    Some(a.iter().filter_map(|p| xyz(p.get(POINT_SET_POS)?)).collect())
}

pub fn point_sets_pointer() -> String {
    pointer(&[KEY_ENVIRONMENT, ENV_POINT_SETS])
}

pub fn point_set_pointer(name: &str) -> String {
    pointer(&[KEY_ENVIRONMENT, ENV_POINT_SETS, name])
}

/// `environment.animals` (whales, dragons; each has a `group`).
pub fn animals(mission: &Value) -> &[Value] {
    environment(mission, ENV_ANIMALS).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

pub fn animal_group(animal: &Value) -> Option<&str> {
    animal.get(ANIMAL_GROUP)?.as_str()
}

pub fn animals_pointer() -> String {
    pointer(&[KEY_ENVIRONMENT, ENV_ANIMALS])
}

/// The objectives (`tasks`).
pub fn tasks(mission: &Value) -> &[Value] {
    list(mission, KEY_TASKS)
}

pub fn task_name(task: &Value) -> Option<&str> {
    task.get(TASK_NAME)?.as_str()
}

pub fn task_text(task: &Value) -> Option<&str> {
    task.get(TASK_TEXT)?.as_str()
}

/// `Active`: a JSON boolean (a missing key reads as `false`).
pub fn task_active(task: &Value) -> bool {
    task.get(TASK_ACTIVE).and_then(Value::as_bool).unwrap_or(false)
}

/// A task's condition list (`complete_when` or `fail_when`; empty when missing).
pub fn task_conditions<'a>(task: &'a Value, key: &str) -> &'a [Value] {
    list(task, key)
}

/// The pointer of task `index`, or of one of its keys.
pub fn task_pointer(index: usize, key: Option<&str>) -> String {
    let i = index.to_string();
    match key {
        Some(k) => pointer(&[KEY_TASKS, &i, k]),
        None => pointer(&[KEY_TASKS, &i]),
    }
}

/// The rule list (`rules`).
pub fn rules(mission: &Value) -> &[Value] {
    list(mission, KEY_RULES)
}

pub fn rule_name(rule: &Value) -> Option<&str> {
    rule.get(RULE_NAME)?.as_str()
}

/// The pointer of rule `index`, or of one of its fields.
pub fn rule_pointer(index: usize, key: Option<&str>) -> String {
    let i = index.to_string();
    match key {
        Some(k) => pointer(&[KEY_RULES, &i, k]),
        None => pointer(&[KEY_RULES, &i]),
    }
}

/// The star-map labels (`map_text`).
pub fn map_text(mission: &Value) -> &[Value] {
    list(mission, KEY_MAP_TEXT)
}

pub fn map_text_name(label: &Value) -> Option<&str> {
    label.get(MAP_TEXT_NAME)?.as_str()
}

pub fn map_text_visible(label: &Value) -> bool {
    label.get(MAP_TEXT_VISIBLE).and_then(Value::as_bool).unwrap_or(false)
}

/// `victory`: `"objectives"` hands the ending to the required tasks.
pub fn victory(mission: &Value) -> Option<&str> {
    mission.get(KEY_VICTORY)?.as_str()
}

/// `victory_message` and `defeat_message`: ids into `media.json` `messages`.
pub fn victory_message(mission: &Value) -> Option<&str> {
    mission.get(KEY_VICTORY_MESSAGE)?.as_str()
}

pub fn defeat_message(mission: &Value) -> Option<&str> {
    mission.get(KEY_DEFEAT_MESSAGE)?.as_str()
}

/// One table of a `media.json` (`None` when the file or the table is missing).
pub fn media_table(media: &Value, which: MediaTable) -> Option<&Map<String, Value>> {
    table(media, which.key())
}

/// The pointer of a table of `media.json`.
pub fn media_table_pointer(which: MediaTable) -> String {
    pointer(&[which.key()])
}

/// The pointer of the entry `name` of a table of `media.json`.
pub fn media_entry_pointer(which: MediaTable, name: &str) -> String {
    pointer(&[which.key(), name])
}

// ---- moved from `map::doc` (Phase 4 T8, T10): the nebula records, volumes and the script / media files ----

/// The mission file next to `map.json` (`maps/<id>/mission.json`).
pub const MISSION_FILE: &str = "mission.json";

// ---- the mission's records of a nebula (`mission.json`) ----

/// The pointer of the list of nebula setups in `mission.json` (`environment.nebulae`).
pub fn mission_nebulae_pointer() -> String {
    pointer(&[KEY_ENVIRONMENT, ENV_NEBULAE])
}

/// The raw `Setup Nebula` records (`environment.nebulae`).
pub fn mission_nebulae(mission: &Value) -> &[Value] {
    environment(mission, ENV_NEBULAE).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// A mission record's own name (`New Nebula Name`), the link to `nebulae[].name`.
pub fn record_name(record: &Value) -> Option<&str> {
    record.get(RecordField::Name.key())?.as_str()
}

/// A mission record's `Polygon Name`, the link to a key of `volumes`.
pub fn record_polygon_name(record: &Value) -> Option<&str> {
    record.get(RecordField::PolygonName.key())?.as_str()
}

/// Which field of a mission nebula record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordField {
    Name,
    PolygonName,
    Occlusion,
    CloudEffect,
}

impl RecordField {
    pub fn key(self) -> &'static str {
        match self {
            RecordField::Name => "New Nebula Name",
            RecordField::PolygonName => "Polygon Name",
            RecordField::Occlusion => "Nebula Occlusion On/Off",
            RecordField::CloudEffect => "Nebula Cloud Effect Name",
        }
    }
}

/// `TRUE` / `FALSE`, as the rule records write a flag (`wot_flag` in the engine).
pub fn flag_text(on: bool) -> Value {
    Value::from(if on { "TRUE" } else { "FALSE" })
}

/// Whether the record's occlusion flag is `TRUE`.
pub fn record_occlusion(record: &Value) -> bool {
    record.get(RecordField::Occlusion.key()).and_then(Value::as_str) == Some("TRUE")
}

/// The pointer of a field of mission record `index`.
pub fn record_pointer(index: usize, field: RecordField) -> String {
    pointer(&[KEY_ENVIRONMENT, ENV_NEBULAE, &index.to_string(), field.key()])
}

/// A new mission record for the nebula `name` whose polygon is the volume `polygon_name`. PROVISIONAL: the keys written are the ones
/// the engine reads (`New Nebula Name`, `Polygon Name`, `Nebula Occlusion On/Off`, `Nebula Cloud Effect Name`, `crew.rs`); the rule
/// record's other fields (`World Object ID` and the point-set names) are left out because the engine ignores them here.
pub fn new_record(name: &str, polygon_name: &str) -> Value {
    serde_json::json!({
        "New Nebula Name": name,
        "Polygon Name": polygon_name,
        "Nebula Cloud Effect Name": "Nebula",
        "Nebula Occlusion On/Off": "FALSE",
    })
}

/// The pointer of the mission's `volumes` object.
pub fn volumes_pointer() -> String {
    pointer(&[KEY_VOLUMES])
}

/// The pointer of the volume `name`.
pub fn volume_pointer(name: &str) -> String {
    pointer(&[KEY_VOLUMES, name])
}

/// The volume `name` of a mission as `[x, y]` points.
pub fn mission_volume(mission: &Value, name: &str) -> Option<Vec<[f64; 2]>> {
    let pts = mission.get(KEY_VOLUMES)?.get(name)?.as_array()?;
    Some(pts.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect())
}

/// Whether the mission has a volume called `name`.
pub fn has_volume(mission: &Value, name: &str) -> bool {
    mission.get(KEY_VOLUMES).and_then(|v| v.get(name)).is_some()
}

/// Whether anything of `mission` other than its `volumes` and its nebula records names `name`: a string equal to it anywhere, or an
/// object key equal to it (a point set, a group). Another nebula record (not `skip`) that names it as its `Polygon Name` counts too.
pub fn mission_mentions(mission: &Value, name: &str, skip: Option<usize>) -> bool {
    fn walk(v: &Value, name: &str) -> bool {
        match v {
            Value::String(s) => s == name,
            Value::Array(a) => a.iter().any(|x| walk(x, name)),
            Value::Object(o) => o.iter().any(|(k, x)| k == name || walk(x, name)),
            _ => false,
        }
    }
    let Some(o) = mission.as_object() else { return false };
    o.iter().filter(|(k, _)| k.as_str() != KEY_VOLUMES).any(|(k, v)| match (k.as_str(), v.as_object()) {
        (KEY_ENVIRONMENT, Some(env)) => env.iter().any(|(ek, ev)| match (ek.as_str(), ev.as_array()) {
            (ENV_NEBULAE, Some(list)) => list.iter().enumerate().any(|(i, r)| Some(i) != skip && record_polygon_name(r) == Some(name)),
            _ => walk(ev, name) || ek == name,
        }),
        _ => walk(v, name) || k == name,
    })
}

/// A map's optional script (engine extension, `docs/modding.md` "Skirmish map scripts") and its words.
pub const SCRIPT_FILE: &str = "script.json";
pub const MEDIA_FILE: &str = "media.json";
/// `script.json`'s top-level key that lets the script end the game, and the value that does it.
pub const SCRIPT_KEY_VICTORY: &str = "victory";
pub const SCRIPT_DECIDES: &str = "script";

/// `true` when a `script.json` lets its own rules end the game (`"victory": "script"`; the engine also takes `"objectives"`, the name the
/// campaign gives its task-driven endings: `rules/skirmish.rs` `script_decides`).
pub fn script_decides(script: &Value) -> bool {
    matches!(script.get(SCRIPT_KEY_VICTORY).and_then(Value::as_str), Some(SCRIPT_DECIDES | "objectives"))
}

/// The rules of a `script.json` (empty when missing).
pub fn script_rules(script: &Value) -> &[Value] {
    list(script, KEY_RULES)
}

// T1: the lists a mission can add to or delete from, for `mission::project`'s refusals (the keyed ones are `ships`, `rules`, `points`).

/// The path of keys to a list of `mission.json` that has entries one can add and delete (`environment` ones are nested).
pub fn list_path(list: MissionList) -> Vec<&'static str> {
    match list {
        MissionList::Players => vec![KEY_PLAYERS],
        MissionList::Alliances => vec![KEY_ALLIANCES],
        MissionList::Ships => vec![KEY_SHIPS],
        MissionList::Points => vec![KEY_POINTS],
        MissionList::Tasks => vec![KEY_TASKS],
        MissionList::Rules => vec![KEY_RULES],
        MissionList::MapText => vec![KEY_MAP_TEXT],
        MissionList::Nebulae => vec![KEY_ENVIRONMENT, ENV_NEBULAE],
        MissionList::Animals => vec![KEY_ENVIRONMENT, ENV_ANIMALS],
    }
}

/// The pointer of a list ([`list_path`]).
pub fn list_pointer(list: MissionList) -> String {
    pointer(&list_path(list))
}

/// The raw entries of a list (empty when the key is missing or not an array).
pub fn list_entries(mission: &Value, list: MissionList) -> &[Value] {
    let mut at = mission;
    for k in list_path(list) {
        match at.get(k) {
            Some(v) => at = v,
            None => return &[],
        }
    }
    at.as_array().map_or(&[], Vec::as_slice)
}

/// The key a keyed list's entries are told apart by (the engine's keyed merge: `ships` by `id`, `rules` by `name`, `points` by
/// `Name`); `None` for a list the engine does not key.
pub fn list_key(list: MissionList) -> Option<&'static str> {
    match list {
        MissionList::Ships => Some(SHIP_ID),
        MissionList::Rules => Some(RULE_NAME),
        MissionList::Points => Some(POINT_NAME),
        _ => None,
    }
}

/// What one entry of a list is called in an undo label.
pub fn list_noun(list: MissionList) -> &'static str {
    match list {
        MissionList::Players => "player",
        MissionList::Alliances => "alliance",
        MissionList::Ships => "ship",
        MissionList::Points => "point",
        MissionList::Tasks => "task",
        MissionList::Rules => "rule",
        MissionList::MapText => "map text",
        MissionList::Nebulae => "nebula",
        MissionList::Animals => "animal",
    }
}

/// The lists of `mission.json` an entry can be added to or deleted from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionList {
    Players,
    Alliances,
    Ships,
    Points,
    Tasks,
    Rules,
    MapText,
    Nebulae,
    Animals,
}

impl MissionList {
    pub const ALL: [MissionList; 9] = [
        MissionList::Players,
        MissionList::Alliances,
        MissionList::Ships,
        MissionList::Points,
        MissionList::Tasks,
        MissionList::Rules,
        MissionList::MapText,
        MissionList::Nebulae,
        MissionList::Animals,
    ];
}

// T2
// ---- rules and media entries (docs/phase5.md T2): the keys `mission::rules`, `mission::media` and `mission::refs` write through ----

/// The two item lists of a rule (`conditions`, `actions`) and its two flags; the rule's other keys come from the rule schema.
pub const RULE_CONDITIONS: &str = "conditions";
pub const RULE_ACTIONS: &str = "actions";
pub const RULE_RUN_ONCE: &str = "Run Once";
pub const RULE_IS_ACTIVE: &str = "Is Active";
/// The key that names a condition's or an action's Type.
pub const ITEM_TYPE: &str = "Type";
/// The text of a speech entry (the other tables hold plain strings).
pub const SPEECH_TEXT: &str = "text";

/// The key of a rule's list of conditions or of actions.
pub fn rule_list_key(is_condition: bool) -> &'static str {
    if is_condition { RULE_CONDITIONS } else { RULE_ACTIONS }
}

/// The pointer of the `rules` list itself.
pub fn rules_pointer() -> String {
    pointer(&[KEY_RULES])
}

/// The pointer of rule `index`'s list of conditions or actions.
pub fn rule_list_pointer(index: usize, is_condition: bool) -> String {
    rule_pointer(index, Some(rule_list_key(is_condition)))
}

/// The pointer of condition or action `item` of rule `index`.
pub fn rule_item_pointer(index: usize, is_condition: bool, item: usize) -> String {
    pointer(&[KEY_RULES, &index.to_string(), rule_list_key(is_condition), &item.to_string()])
}

/// The conditions or the actions of a rule (empty when missing).
pub fn rule_items(rule: &Value, is_condition: bool) -> &[Value] {
    list(rule, rule_list_key(is_condition))
}

/// The Type of a condition or an action.
pub fn item_type(item: &Value) -> Option<&str> {
    item.get(ITEM_TYPE)?.as_str()
}

/// A new rule: active, runs once, nothing in it (the engine's loader reads the two flags as JSON booleans).
pub fn new_rule(name: &str) -> Value {
    let mut o = Map::new();
    o.insert(RULE_NAME.into(), name.into());
    o.insert(RULE_RUN_ONCE.into(), Value::Bool(true));
    o.insert(RULE_IS_ACTIVE.into(), Value::Bool(true));
    o.insert(RULE_CONDITIONS.into(), Value::Array(vec![]));
    o.insert(RULE_ACTIONS.into(), Value::Array(vec![]));
    Value::Object(o)
}

/// What a new entry of a `media.json` table starts as: a speech event with empty text, else an empty string.
pub fn media_new_entry(which: MediaTable) -> Value {
    match which {
        MediaTable::Speech => {
            let mut o = Map::new();
            o.insert(SPEECH_TEXT.into(), "".into());
            Value::Object(o)
        }
        _ => Value::String(String::new()),
    }
}

// T7
// ---- the plan (docs/phase5.md T7): geometry tables, vertices, map text ----

/// The three object tables of named geometry. `volumes` and `paths` are top-level, `point_sets` is under `environment`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoTable {
    Volumes,
    Paths,
    PointSets,
}

impl GeoTable {
    /// The keys from the document root to the table.
    pub fn tokens(self) -> Vec<&'static str> {
        match self {
            GeoTable::Volumes => vec![KEY_VOLUMES],
            GeoTable::Paths => vec![KEY_PATHS],
            GeoTable::PointSets => vec![KEY_ENVIRONMENT, ENV_POINT_SETS],
        }
    }

    pub fn pointer(self) -> String {
        pointer(&self.tokens())
    }

    /// The pointer of entry `name`.
    pub fn entry_pointer(self, name: &str) -> String {
        let mut t: Vec<&str> = self.tokens();
        t.push(name);
        pointer(&t)
    }

    /// The pointer of item `index` of entry `name` (a vertex, a dot).
    pub fn item_pointer(self, name: &str, index: usize) -> String {
        let i = index.to_string();
        let mut t: Vec<&str> = self.tokens();
        t.push(name);
        t.push(&i);
        pointer(&t)
    }

    /// The table, when the document has it as an object.
    pub fn get(self, mission: &Value) -> Option<&Map<String, Value>> {
        let mut at = mission;
        for k in self.tokens() {
            at = at.get(k)?;
        }
        at.as_object()
    }

    /// The edit that adds entry `name` = `value`: sets the entry, or the missing table (and `environment`) round it.
    pub fn insert_edit(self, mission: &Value, name: &str, value: Value) -> crate::edit::Edit {
        let tokens = self.tokens();
        let (mut have, mut at) = (0, mission);
        for k in &tokens {
            match at.get(*k) {
                Some(v) if v.is_object() => {
                    at = v;
                    have += 1;
                }
                _ => break,
            }
        }
        if have == tokens.len() {
            let mut t = tokens;
            t.push(name);
            return crate::edit::Edit::set(pointer(&t), value);
        }
        let mut v = Map::new();
        v.insert(name.to_string(), value);
        let mut v = Value::Object(v);
        for k in tokens[have + 1..].iter().rev() {
            let mut o = Map::new();
            o.insert((*k).to_string(), v);
            v = Value::Object(o);
        }
        crate::edit::Edit::set(pointer(&tokens[..=have]), v)
    }
}

/// The names of the paths, in file order.
pub fn path_names(mission: &Value) -> Vec<&str> {
    GeoTable::Paths.get(mission).map_or_else(Vec::new, |o| o.keys().map(String::as_str).collect())
}

/// The names of the point sets, in file order.
pub fn point_set_names(mission: &Value) -> Vec<&str> {
    GeoTable::PointSets.get(mission).map_or_else(Vec::new, |o| o.keys().map(String::as_str).collect())
}

/// The raw vertices of a table entry (volume, path) or the dots of a point set (objects with `pos`), as the file has them.
pub fn geo_items<'a>(mission: &'a Value, table: GeoTable, name: &str) -> &'a [Value] {
    table.get(mission).and_then(|t| t.get(name)).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// The pointer of the `pos` of dot `index` of the point set `name`.
pub fn point_set_dot_pos_pointer(name: &str, index: usize) -> String {
    let i = index.to_string();
    pointer(&[KEY_ENVIRONMENT, ENV_POINT_SETS, name, &i, POINT_SET_POS])
}

/// A new point set dot.
pub fn new_point_set_dot(pos: [f64; 3]) -> Value {
    let mut o = Map::new();
    o.insert(POINT_SET_POS.into(), Value::from(pos.to_vec()));
    Value::Object(o)
}

/// The position of a point set dot.
pub fn point_set_dot_pos(dot: &Value) -> Option<[f64; 3]> {
    xyz(dot.get(POINT_SET_POS)?)
}

/// The pointer of key `key` of the `player_start` object.
pub fn player_start_pointer(key: &str) -> String {
    pointer(&[KEY_PLAYER_START, key])
}

/// A ship's nine-number `rot`, when it has one.
pub fn ship_rot(ship: &Value) -> Option<[f64; 9]> {
    crate::map_edit::orient::read_rot(ship.get(SHIP_ROT)?)
}

/// A new objective marker.
pub fn new_point(name: &str, position: [f64; 3]) -> Value {
    let mut o = Map::new();
    o.insert(POINT_NAME.into(), name.into());
    o.insert(POINT_POSITION.into(), Value::from(position.to_vec()));
    Value::Object(o)
}

/// A new star-map label (`pos` is `[x, y]`).
pub fn new_map_text(name: &str, text: &str, pos: [f64; 2]) -> Value {
    let mut o = Map::new();
    o.insert(MAP_TEXT_NAME.into(), name.into());
    o.insert(MAP_TEXT_TEXT.into(), text.into());
    o.insert(MAP_TEXT_POS.into(), Value::from(pos.to_vec()));
    o.insert(MAP_TEXT_VISIBLE.into(), Value::Bool(true));
    Value::Object(o)
}

pub fn map_text_text(label: &Value) -> Option<&str> {
    label.get(MAP_TEXT_TEXT)?.as_str()
}

/// A label's position as the file has it (`[x, y]`; a third number is tolerated).
pub fn map_text_pos(label: &Value) -> Option<[f64; 2]> {
    let a = label.get(MAP_TEXT_POS)?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

/// The pointer of label `index`, or of one of its keys.
pub fn map_text_pointer(index: usize, key: Option<&str>) -> String {
    let i = index.to_string();
    match key {
        Some(k) => pointer(&[KEY_MAP_TEXT, &i, k]),
        None => pointer(&[KEY_MAP_TEXT, &i]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "players": ["Hero", "Raiders", 7],
            "alliances": [["Hero", "Allies"], ["bad"], [1, 2]],
            "player_start": {"name": "Hero", "start": [1, 2, 3], "forward": [0, 1, 0]},
            "ships": [
                {"id": 10, "type": "Ship_A", "group": "Hero", "name": "Dauntless", "primary": true, "pos": [0, 0, 0]},
                {"id": 11, "type": "Ship_B", "group": "Raiders"}
            ],
            "groups": {"RaiderShips": [11, "x", 12]},
            "volumes": {"Gate Area": [[0, 0], [1, 0], [1, 1]], "Other": []},
            "paths": {"Run": [[0, 0, 0], [1, 2], [3, 4, 5]]},
            "points": [{"Name": "The Gate", "Position": [4, 5, 6]}],
            "environment": {
                "point_sets": {"Spawn": [{"pos": [1, 1, 1]}, {"nope": 1}]},
                "nebulae": [{"New Nebula Name": "N", "Polygon Name": "Gate Area"}],
                "animals": [{"group": "Whales", "whale": {}}]
            },
            "tasks": [{"Name": "Reach", "text": "Sail.", "Active": true, "complete_when": [{"Type": "T"}]}],
            "rules": [{"name": "Start"}, {"name": "End"}],
            "map_text": [{"name": "Label", "text": "Here", "pos": [0, 0], "visible": true}],
            "victory": "objectives", "victory_message": "IDGS_WIN", "defeat_message": "IDGS_LOSE"
        })
    }

    #[test]
    fn players_alliances_start() {
        let m = sample();
        assert_eq!(players(&m), vec!["Hero", "Raiders"]);
        assert_eq!(alliances(&m), vec![["Hero", "Allies"]]);
        assert_eq!(player_start_name(&m), Some("Hero"));
        assert_eq!((player_start_pos(&m), player_start_forward(&m)), (Some([1.0, 2.0, 3.0]), Some([0.0, 1.0, 0.0])));
        assert_eq!(player_start(&json!({"player_start": 3})), None);
    }

    #[test]
    fn ships_and_groups() {
        let m = sample();
        assert_eq!(ships(&m).len(), 2);
        let s = &ships(&m)[0];
        assert_eq!((ship_id(s), ship_type(s), ship_group(s), ship_name(s)), (Some(10), Some("Ship_A"), Some("Hero"), Some("Dauntless")));
        assert_eq!((ship_primary(s), ship_pos(s)), (true, Some([0.0, 0.0, 0.0])));
        let t = &ships(&m)[1];
        assert_eq!((ship_primary(t), ship_pos(t), ship_name(t)), (false, None, None));
        assert_eq!(group_members(&m, "RaiderShips"), vec![11, 12]);
        assert!(group_members(&m, "Nope").is_empty());
        assert_eq!(ship_pointer(1, Some("name")), "/ships/1/name");
        assert_eq!(ship_pointer(0, None), "/ships/0");
        assert_eq!((groups_pointer(), group_pointer("a/b")), ("/groups".to_string(), "/groups/a~1b".to_string()));
    }

    #[test]
    fn plan_tables() {
        let m = sample();
        assert_eq!(volume_names(&m), vec!["Gate Area", "Other"]);
        assert_eq!(mission_volume(&m, "Gate Area"), Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]));
        assert_eq!(path_points(&m, "Run"), Some(vec![[0.0, 0.0, 0.0], [3.0, 4.0, 5.0]]));
        assert_eq!(path_points(&m, "Nope"), None);
        let p = &points(&m)[0];
        assert_eq!((point_name(p), point_position(p)), (Some("The Gate"), Some([4.0, 5.0, 6.0])));
        assert_eq!(point_set_positions(&m, "Spawn"), Some(vec![[1.0, 1.0, 1.0]]));
        assert_eq!(point_sets(&m).map(Map::len), Some(1));
        assert_eq!(animals(&m).len(), 1);
        assert_eq!(animal_group(&animals(&m)[0]), Some("Whales"));
        assert_eq!(mission_nebulae(&m).len(), 1);
        assert_eq!((point_pointer(2, Some("Name")), point_set_pointer("S 1")), ("/points/2/Name".to_string(), "/environment/point_sets/S 1".to_string()));
        assert_eq!((paths_pointer(), path_pointer("Run")), ("/paths".to_string(), "/paths/Run".to_string()));
        assert_eq!((point_sets_pointer(), animals_pointer()), ("/environment/point_sets".to_string(), "/environment/animals".to_string()));
    }

    #[test]
    fn tasks_rules_text_and_victory() {
        let m = sample();
        let t = &tasks(&m)[0];
        assert_eq!((task_name(t), task_text(t), task_active(t)), (Some("Reach"), Some("Sail."), true));
        assert_eq!(task_conditions(t, TASK_COMPLETE_WHEN).len(), 1);
        assert!(task_conditions(t, TASK_FAIL_WHEN).is_empty());
        assert_eq!(rules(&m).iter().filter_map(rule_name).collect::<Vec<_>>(), vec!["Start", "End"]);
        assert_eq!((rule_pointer(1, None), rule_pointer(0, Some("Run Once"))), ("/rules/1".to_string(), "/rules/0/Run Once".to_string()));
        assert_eq!(task_pointer(0, Some("Active")), "/tasks/0/Active");
        let l = &map_text(&m)[0];
        assert_eq!((map_text_name(l), map_text_visible(l)), (Some("Label"), true));
        assert_eq!((victory(&m), victory_message(&m), defeat_message(&m)), (Some("objectives"), Some("IDGS_WIN"), Some("IDGS_LOSE")));
    }

    #[test]
    fn missing_keys_read_as_empty() {
        let m = json!({});
        assert!(players(&m).is_empty() && alliances(&m).is_empty() && ships(&m).is_empty() && tasks(&m).is_empty() && rules(&m).is_empty());
        assert!(points(&m).is_empty() && animals(&m).is_empty() && map_text(&m).is_empty() && volume_names(&m).is_empty());
        assert!(groups(&m).is_none() && paths(&m).is_none() && point_sets(&m).is_none());
        assert_eq!((player_start_name(&m), victory(&m), victory_message(&m)), (None, None, None));
        assert!(ships(&json!({"ships": 5})).is_empty());
        assert!(!task_active(&json!({})) && !ship_primary(&json!({"primary": "yes"})));
    }

    #[test]
    fn media_tables() {
        let n = new_media();
        for t in MediaTable::ALL {
            assert!(media_table(&n, t).is_some_and(Map::is_empty), "{t:?}");
        }
        let media = json!({"speech": {"a": {"text": "x"}}, "messages": {}});
        assert_eq!(media_table(&media, MediaTable::Speech).map(Map::len), Some(1));
        assert!(media_table(&media, MediaTable::ShipNames).is_none());
        assert!(media_table(&Value::Null, MediaTable::Messages).is_none());
        assert_eq!((media_table_pointer(MediaTable::ShipNames), media_entry_pointer(MediaTable::Speech, "a/b")), ("/ship_names".to_string(), "/speech/a~1b".to_string()));
    }

    #[test]
    fn nebula_records_and_mentions() {
        let m = sample();
        let r = &mission_nebulae(&m)[0];
        assert_eq!((record_name(r), record_polygon_name(r)), (Some("N"), Some("Gate Area")));
        assert!(!record_occlusion(r));
        assert_eq!(record_pointer(0, RecordField::PolygonName), "/environment/nebulae/0/Polygon Name");
        assert!(has_volume(&m, "Other") && !has_volume(&m, "Zed"));
        // the nebula record names the volume; nothing else does but the record itself
        assert!(mission_mentions(&m, "Gate Area", None));
        assert!(!mission_mentions(&m, "Gate Area", Some(0)));
        assert_eq!(new_record("N", "V")["Polygon Name"], "V");
        assert_eq!(flag_text(true), "TRUE");
        assert_eq!((mission_nebulae_pointer(), volumes_pointer(), volume_pointer("a")), ("/environment/nebulae".to_string(), "/volumes".to_string(), "/volumes/a".to_string()));
    }

    #[test]
    fn list_paths_and_keys() {
        let m = json!({"rules": [{}], "environment": {"animals": [{}, {}]}});
        assert_eq!((list_entries(&m, MissionList::Rules).len(), list_entries(&m, MissionList::Animals).len(), list_entries(&m, MissionList::Tasks).len()), (1, 2, 0));
        assert_eq!((list_pointer(MissionList::Nebulae), list_pointer(MissionList::MapText)), ("/environment/nebulae".to_string(), "/map_text".to_string()));
        let keyed: Vec<_> = MissionList::ALL.iter().filter_map(|l| list_key(*l)).collect();
        assert_eq!(keyed, ["id", "Name", "name"]);
        assert!(MissionList::ALL.iter().all(|l| !list_noun(*l).is_empty()));
    }

    #[test]
    fn script_keys() {
        assert!(script_decides(&json!({"victory": "script"})) && script_decides(&json!({"victory": "objectives"})));
        assert!(!script_decides(&json!({})));
        assert_eq!(script_rules(&json!({"rules": [{}, {}]})).len(), 2);
        assert_eq!((MISSION_FILE, MEDIA_FILE, SCRIPT_FILE), ("mission.json", "media.json", "script.json"));
    }
}
