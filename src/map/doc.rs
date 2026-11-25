//! Accessors over a `map.json` held as a [`serde_json::Value`]: the ONE place in the editor that spells a map key (docs/phase4.md
//! risk 1). Read-only; edits go through [`crate::edit::Edit`] with JSON pointers built by the caller.
//!
//! Key names were read from the engine (`game/src/scenario/spawn.rs`, `nebula.rs`, `rules/check.rs`, `starmap.rs`,
//! `classic/menu.rs`, `docs/missions.md` section 2) and are PROVISIONAL as a format: the engine has no map schema and no `format`
//! key yet. What the engine reads, and what this file assumes beyond it:
//!
//! - read in engine code: `objects[]` (`type`, `scene`, `id`, `pos`, `rot`), `ships[]` (`id`, `type`, `player`, `pos`, `rot`),
//!   `mines[]` (`type`, `id`, `pos`), `players[]` (`name`, `team`, `start`, `forward`, `formation`, `playable`, `race`), `teams[]`
//!   (`name_id`), `nebulae[]` (`name`, `polygon`, `cloud`, `storm`, `sound_distance`), `world_half`, `sky`, `lighting`, `backdrop`.
//! - PROVISIONAL (not seen read by the engine here, or only documented): `name` and `skirmish` as top-level keys
//!   (docs/missions.md, docs/phase4.md P8); `nebulae[].groups` and `meteors`; the smallest id the editor hands out (1); that a
//!   missing `pos` z means 0.

use serde_json::Value;
use std::collections::BTreeSet;

/// The file name of a map inside `maps/<id>/`.
pub const MAP_FILE: &str = "map.json";

/// For callers that need a default `world_half`: the engine's own fallback (`rules/check.rs`, `starmap.rs`).
pub const DEFAULT_WORLD_HALF: f64 = 1000.0;

/// The arrays whose entries carry an `id` that must be unique across all of them (docs/missions.md: "keep them unique across all
/// three"; animals live in the mission, not in the map).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Objects,
    Ships,
    Mines,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Layer::Objects, Layer::Ships, Layer::Mines];

    /// The array's key in `map.json`; also the first pointer segment of an edit (`/objects/3/pos`).
    pub fn key(self) -> &'static str {
        match self {
            Layer::Objects => "objects",
            Layer::Ships => "ships",
            Layer::Mines => "mines",
        }
    }
}

/// Every array of `map.json` that holds entries an editor adds, deletes or moves: the id-bearing [`Layer`]s and the lobby and
/// scenery setups. The name is the array's key and the first pointer segment of an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Array {
    Objects,
    Ships,
    Mines,
    Players,
    Teams,
    Nebulae,
}

impl Array {
    pub const ALL: [Array; 6] = [Array::Objects, Array::Ships, Array::Mines, Array::Players, Array::Teams, Array::Nebulae];

    /// The array's key in `map.json`.
    pub fn key(self) -> &'static str {
        match self {
            Array::Objects => Layer::Objects.key(),
            Array::Ships => Layer::Ships.key(),
            Array::Mines => Layer::Mines.key(),
            Array::Players => "players",
            Array::Teams => "teams",
            Array::Nebulae => "nebulae",
        }
    }

    /// The layer when this is one of the id-bearing arrays.
    pub fn layer(self) -> Option<Layer> {
        match self {
            Array::Objects => Some(Layer::Objects),
            Array::Ships => Some(Layer::Ships),
            Array::Mines => Some(Layer::Mines),
            _ => None,
        }
    }
}

impl From<Layer> for Array {
    fn from(l: Layer) -> Array {
        match l {
            Layer::Objects => Array::Objects,
            Layer::Ships => Array::Ships,
            Layer::Mines => Array::Mines,
        }
    }
}

/// The entries of any of the arrays (empty when the key is missing or not an array).
pub fn array_entries(map: &Value, which: Array) -> &[Value] {
    array(map, which.key())
}

/// The key an entry's position is written under: `pos`, and `start` for a player.
pub fn pos_key(which: Array) -> &'static str {
    match which {
        Array::Players => "start",
        _ => "pos",
    }
}

/// A map to start from: a name and the engine's default `world_half` (PROVISIONAL: nothing else is needed to load).
pub fn new_map(name: &str) -> Value {
    serde_json::json!({"name": name, "world_half": DEFAULT_WORLD_HALF})
}

/// Gives `entry` (an object; anything else is left alone) the `id` `id`.
pub fn set_id(entry: &mut Value, id: i64) {
    if let Some(o) = entry.as_object_mut() {
        o.insert("id".into(), Value::from(id));
    }
}

fn array<'a>(map: &'a Value, key: &str) -> &'a [Value] {
    map.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// The entries of one id-bearing array (empty when the key is missing or not an array).
pub fn entries(map: &Value, layer: Layer) -> &[Value] {
    array(map, layer.key())
}

/// Scenery: islands, asteroids, bases (`objects`).
pub fn objects(map: &Value) -> &[Value] {
    entries(map, Layer::Objects)
}

/// Pre-placed ships of historical maps (`ships`).
pub fn ships(map: &Value) -> &[Value] {
    entries(map, Layer::Ships)
}

/// Mines resting where the map put them (`mines`).
pub fn mines(map: &Value) -> &[Value] {
    entries(map, Layer::Mines)
}

/// Skirmish lobby slots; the index is the slot order (`players`).
pub fn players(map: &Value) -> &[Value] {
    array(map, "players")
}

/// Skirmish teams (`teams`).
pub fn teams(map: &Value) -> &[Value] {
    array(map, "teams")
}

/// Nebula and storm setups (`nebulae`).
pub fn nebulae(map: &Value) -> &[Value] {
    array(map, "nebulae")
}

/// Half the side of the playable square (`world_half`); `None` when the map does not say (the engine then uses
/// [`DEFAULT_WORLD_HALF`]).
pub fn world_half(map: &Value) -> Option<f64> {
    map.get("world_half")?.as_f64()
}

/// The map's display name (`name`, PROVISIONAL).
pub fn name(map: &Value) -> Option<&str> {
    map.get("name")?.as_str()
}

/// Whether the map is listed in the skirmish lobby (`skirmish`, PROVISIONAL).
pub fn skirmish(map: &Value) -> bool {
    map.get("skirmish").and_then(Value::as_bool).unwrap_or(false)
}

/// The sky as written, with any `_LayerN` suffix (the engine drops it when it picks the sky).
pub fn sky(map: &Value) -> Option<&str> {
    map.get("sky")?.as_str()
}

/// `true` when the map turns the star-map picture on (any string in `backdrop`).
pub fn has_backdrop(map: &Value) -> bool {
    map.get("backdrop").is_some_and(Value::is_string)
}

// ---- one entry of `objects`, `ships` or `mines` ----

/// The entry's `id`.
pub fn id(entry: &Value) -> Option<i64> {
    entry.get("id")?.as_i64()
}

/// The entry's `type` (scenery type, ship class or mine bullet).
pub fn kind(entry: &Value) -> Option<&str> {
    entry.get("type")?.as_str()
}

/// The model of a scenery object (`scene`: the GLB's name without extension).
pub fn scene(entry: &Value) -> Option<&str> {
    entry.get("scene")?.as_str()
}

fn point(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    let z = match a.get(2) {
        Some(z) => z.as_f64()?,
        None => 0.0,
    };
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, z])
}

/// `pos` as `[x, y, z]` in game axes; a two-number `pos` has z 0 (PROVISIONAL).
pub fn pos(entry: &Value) -> Option<[f64; 3]> {
    point(entry.get("pos")?)
}

/// `rot`: nine numbers, the 3x3 matrix as the engine reads it.
pub fn rot(entry: &Value) -> Option<[f64; 9]> {
    let a = entry.get("rot")?.as_array()?;
    if a.len() != 9 {
        return None;
    }
    let mut m = [0.0; 9];
    for (slot, v) in m.iter_mut().zip(a) {
        *slot = v.as_f64()?;
    }
    Some(m)
}

/// The player a pre-placed ship belongs to (`ships[].player`, an index into `players`).
pub fn ship_player(ship: &Value) -> Option<u64> {
    ship.get("player")?.as_u64()
}

// ---- one entry of `players`, `teams`, `nebulae` ----

/// A player's `name`.
pub fn player_name(player: &Value) -> Option<&str> {
    player.get("name")?.as_str()
}

/// A player's `team` (an index into `teams`, -1 for none).
pub fn player_team(player: &Value) -> Option<i64> {
    player.get("team")?.as_i64()
}

/// A player's `start` point.
pub fn player_start(player: &Value) -> Option<[f64; 3]> {
    point(player.get("start")?)
}

/// A player's `forward` direction (not necessarily unit length).
pub fn player_forward(player: &Value) -> Option<[f64; 3]> {
    point(player.get("forward")?)
}

/// A player's `formation` index.
pub fn player_formation(player: &Value) -> Option<u64> {
    player.get("formation")?.as_u64()
}

/// A player's `playable` flag (a slot a human may take).
pub fn player_playable(player: &Value) -> bool {
    player.get("playable").and_then(Value::as_bool).unwrap_or(false)
}

/// A player's `race` number (0 Navy, 2 Pirate, 3 Procyon, 4 open; others unverified).
pub fn player_race(player: &Value) -> Option<u64> {
    player.get("race")?.as_u64()
}

/// A team's `name_id` (the key a skirmish script names).
pub fn team_name_id(team: &Value) -> Option<&str> {
    team.get("name_id")?.as_str()
}

/// A nebula's `name`.
pub fn nebula_name(nebula: &Value) -> Option<&str> {
    nebula.get("name")?.as_str()
}

/// A nebula's `polygon` as `[x, y]` points (points that are not two numbers are skipped).
pub fn nebula_polygon(nebula: &Value) -> Vec<[f64; 2]> {
    let pts = nebula.get("polygon").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    pts.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect()
}

// ---- ids ----

/// Every `id` used by `objects`, `ships` and `mines`.
pub fn used_ids(map: &Value) -> BTreeSet<i64> {
    Layer::ALL.iter().flat_map(|&l| entries(map, l)).filter_map(id).collect()
}

/// The smallest id not used by `objects`, `ships` and `mines`, at least 1, and not in `also` (ids the caller knows elsewhere, such
/// as animals of a mission). The first id handed out being 1 is PROVISIONAL.
pub fn next_id(map: &Value, also: &[i64]) -> i64 {
    let used = used_ids(map);
    (1..).find(|i| !used.contains(i) && !also.contains(i)).expect("an i64 range has a free id")
}

/// Ids that appear more than once across `objects`, `ships` and `mines`, sorted.
pub fn duplicate_ids(map: &Value) -> Vec<i64> {
    let mut seen = BTreeSet::new();
    let mut dup = BTreeSet::new();
    for id in Layer::ALL.iter().flat_map(|&l| entries(map, l)).filter_map(id) {
        if !seen.insert(id) {
            dup.insert(id);
        }
    }
    dup.into_iter().collect()
}

// T8: nebulae and mines, and the two other records of a nebula in `mission.json`. Names here are read from the engine
// (`game/src/nebula.rs` spawn, storms, polygons; `scenario/spawn.rs` spawn_mines; `rules/eval/schema.rs` Setup Nebula) and the
// exporter's layout (`tools/export_nebulae.py`); the whole layout stays PROVISIONAL as a format until the freeze.

/// The mission file next to `map.json` (`maps/<id>/mission.json`).
pub const MISSION_FILE: &str = "mission.json";

/// The nebula's polygon key (`nebulae[].polygon`, `[[x, y], ...]`, the same as a mission volume).
pub const NEBULA_POLYGON: &str = "polygon";
/// The nebula's name key (the entry's key when a map extends another).
pub const NEBULA_NAME: &str = "name";
/// The nebula's `sound_distance` key.
pub const NEBULA_SOUND_DISTANCE: &str = "sound_distance";
/// The engine's fallback `sound_distance` (`nebula.rs` spawn: `unwrap_or(300.0)`).
pub const DEFAULT_SOUND_DISTANCE: f64 = 300.0;
/// The "no effect" word the exporter writes.
pub const NO_EFFECT: &str = "EFFECT";

/// A switch of a nebula entry, as the form shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NebulaFlag {
    /// `cloud.effect` is `Nebula` (the engine draws the cloud) rather than `EFFECT` (none).
    Cloud,
    /// `cloud.drain`: a ship inside is slowed (`nebula.rs` slowdown).
    Drain,
    /// `cloud.occlusion`: the map side's copy of the mission record's `Nebula Occlusion On/Off` (PROVISIONAL: the engine reads the
    /// mission's, `environment.rs`; this one only mirrors it, as the exporter writes it).
    Occlusion,
    /// `storm.effect` is `SolarStorm`.
    Storm,
    /// `storm.lightning`.
    Lightning,
    /// `storm.winds`.
    Winds,
    /// `meteors.on`.
    Meteors,
}

impl NebulaFlag {
    pub const ALL: [NebulaFlag; 7] =
        [NebulaFlag::Cloud, NebulaFlag::Drain, NebulaFlag::Occlusion, NebulaFlag::Storm, NebulaFlag::Lightning, NebulaFlag::Winds, NebulaFlag::Meteors];

    /// The pointer tokens under the entry.
    pub fn path(self) -> [&'static str; 2] {
        match self {
            NebulaFlag::Cloud => ["cloud", "effect"],
            NebulaFlag::Drain => ["cloud", "drain"],
            NebulaFlag::Occlusion => ["cloud", "occlusion"],
            NebulaFlag::Storm => ["storm", "effect"],
            NebulaFlag::Lightning => ["storm", "lightning"],
            NebulaFlag::Winds => ["storm", "winds"],
            NebulaFlag::Meteors => ["meteors", "on"],
        }
    }

    /// The value to write for `on`.
    pub fn value(self, on: bool) -> Value {
        match self {
            NebulaFlag::Cloud => Value::from(if on { "Nebula" } else { NO_EFFECT }),
            NebulaFlag::Storm => Value::from(if on { "SolarStorm" } else { NO_EFFECT }),
            _ => Value::from(on),
        }
    }

    /// Whether the entry has the switch on.
    pub fn get(self, nebula: &Value) -> bool {
        let [a, b] = self.path();
        let v = nebula.get(a).and_then(|o| o.get(b));
        match self {
            NebulaFlag::Cloud => v.and_then(Value::as_str) == Some("Nebula"),
            NebulaFlag::Storm => v.and_then(Value::as_str) == Some("SolarStorm"),
            _ => v.and_then(Value::as_bool).unwrap_or(false),
        }
    }

    /// The key of the point list this switch needs filled to show anything (the engine spawns one effect per point).
    pub fn points_key(self) -> Option<&'static str> {
        match self {
            NebulaFlag::Cloud => Some("cloud"),
            NebulaFlag::Storm => Some("storm"),
            NebulaFlag::Meteors => Some("meteors"),
            _ => None,
        }
    }
}

/// `nebulae[].sound_distance`, the engine's fallback when missing.
pub fn nebula_sound_distance(nebula: &Value) -> f64 {
    nebula.get(NEBULA_SOUND_DISTANCE).and_then(Value::as_f64).unwrap_or(DEFAULT_SOUND_DISTANCE)
}

/// How many effect points the nebula has under `cloud`, `storm` or `meteors` (`points[]`, each `{pos, magnitude}`).
pub fn nebula_point_count(nebula: &Value, key: &str) -> usize {
    nebula.get(key).and_then(|o| o.get("points")).and_then(Value::as_array).map_or(0, Vec::len)
}

/// One effect point: `{pos: [x, y, 0], magnitude: radius}` (the magnitude is the cloud radius in world units, PROVISIONAL, see
/// `docs/re/nebula-landmark.md`).
pub fn effect_point(centre: [f64; 2], radius: f64) -> Value {
    serde_json::json!({"pos": [centre[0], centre[1], 0.0], "magnitude": radius})
}

/// The pointer of the effect point list `key` (`cloud`, `storm`, `meteors`) of nebula `index`.
pub fn nebula_points_pointer(index: usize, key: &str) -> String {
    crate::edit::pointer(&[Array::Nebulae.key(), &index.to_string(), key, "points"])
}

/// The pointer of a field of nebula `index` (`name`, `polygon`, `sound_distance`, or a [`NebulaFlag`] path).
pub fn nebula_pointer(index: usize, path: &[&str]) -> String {
    let mut t = vec![Array::Nebulae.key().to_string(), index.to_string()];
    t.extend(path.iter().map(|s| s.to_string()));
    crate::edit::pointer(&t)
}

/// A point as the file writes it: `[x, y]`, in the shortest decimals of an f32 (the engine reads f32).
pub fn xy(p: [f64; 2]) -> Value {
    Value::Array(p.iter().map(|&x| crate::map_view::short(x as f32)).collect())
}

/// A polygon as the file writes it.
pub fn polygon_value(points: &[[f64; 2]]) -> Value {
    Value::Array(points.iter().map(|&p| xy(p)).collect())
}

/// A new nebula entry: the polygon, a plain cloud with one effect point at the polygon's middle, no storm, no meteors. PROVISIONAL:
/// the layout is the exporter's (`tools/export_nebulae.py`); which keys a hand-made entry needs is not verified, only that the engine
/// reads `name`, `polygon`, `cloud`, `storm`, `meteors` and `sound_distance` and defaults the rest.
pub fn new_nebula(name: &str, polygon: &[[f64; 2]], centre: [f64; 2], radius: f64) -> Value {
    serde_json::json!({
        "name": name,
        "groups": [],
        "polygon": polygon_value(polygon),
        "cloud": {"effect": "Nebula", "points": [effect_point(centre, radius)], "drain": false, "occlusion": false},
        "storm": {"effect": NO_EFFECT, "points": [], "lightning": false, "winds": false},
        "meteors": {"effect": NO_EFFECT, "points": [], "on": false},
        "sound_distance": DEFAULT_SOUND_DISTANCE,
    })
}

// ---- the mission's records of a nebula (`mission.json`) ----

/// The pointer of the list of nebula setups in `mission.json` (`environment.nebulae`).
pub fn mission_nebulae_pointer() -> String {
    crate::edit::pointer(&["environment", "nebulae"])
}

/// The raw `Setup Nebula` records (`environment.nebulae`).
pub fn mission_nebulae(mission: &Value) -> &[Value] {
    mission.get("environment").and_then(|e| e.get("nebulae")).and_then(Value::as_array).map_or(&[], Vec::as_slice)
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
    crate::edit::pointer(&["environment", "nebulae", &index.to_string(), field.key()])
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

/// A new mine entry: `{type, pos}`; the id is the project's to give. PROVISIONAL: z 0 (the engine reads `pos` as game x, y, z).
pub fn new_mine(ty: &str, at: [f64; 2]) -> Value {
    serde_json::json!({"type": ty, "pos": [at[0], at[1], 0.0]})
}

/// The pointer of the mission's `volumes` object.
pub fn volumes_pointer() -> String {
    crate::edit::pointer(&["volumes"])
}

/// The pointer of the volume `name`.
pub fn volume_pointer(name: &str) -> String {
    crate::edit::pointer(&["volumes", name])
}

/// The volume `name` of a mission as `[x, y]` points.
pub fn mission_volume(mission: &Value, name: &str) -> Option<Vec<[f64; 2]>> {
    let pts = mission.get("volumes")?.get(name)?.as_array()?;
    Some(pts.iter().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect())
}

/// Whether the mission has a volume called `name`.
pub fn has_volume(mission: &Value, name: &str) -> bool {
    mission.get("volumes").and_then(|v| v.get(name)).is_some()
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
    o.iter().filter(|(k, _)| k.as_str() != "volumes").any(|(k, v)| match (k.as_str(), v.as_object()) {
        ("environment", Some(env)) => env.iter().any(|(ek, ev)| match (ek.as_str(), ev.as_array()) {
            ("nebulae", Some(list)) => list.iter().enumerate().any(|(i, r)| Some(i) != skip && record_polygon_name(r) == Some(name)),
            _ => walk(ev, name) || ek == name,
        }),
        _ => walk(v, name) || k == name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "name": "Small", "world_half": 1500.0, "sky": "skybox_05_Layer1", "skirmish": true, "backdrop": "x",
            "objects": [
                {"type": "Island04", "id": 1, "pos": [-300.0, 300.0, 5.0], "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1], "scene": "Island04"},
                {"type": "Rock", "id": 3, "pos": [10, 20]},
                {"type": "Decor"}
            ],
            "ships": [{"id": 2, "type": "Ship_A", "player": 0, "pos": [0, 0, 0]}],
            "mines": [{"type": "Bullet_StarMine_Med", "id": 4, "pos": [1, 2, 3]}],
            "players": [{"name": "P1", "team": 0, "start": [100, 0, 0], "forward": [0, 1, 0], "formation": 2, "playable": true, "race": 0}],
            "teams": [{"name_id": "IDGS_TEAM_A"}],
            "nebulae": [{"name": "Storm", "polygon": [[0, 0], [10, 0], [10, 10], "bad"]}]
        })
    }

    #[test]
    fn arrays_and_top_level_values() {
        let m = sample();
        assert_eq!((objects(&m).len(), ships(&m).len(), mines(&m).len()), (3, 1, 1));
        assert_eq!((players(&m).len(), teams(&m).len(), nebulae(&m).len()), (1, 1, 1));
        assert_eq!(world_half(&m), Some(1500.0));
        assert_eq!((name(&m), sky(&m), skirmish(&m), has_backdrop(&m)), (Some("Small"), Some("skybox_05_Layer1"), true, true));
    }

    #[test]
    fn missing_keys_read_as_empty() {
        let m = json!({});
        assert!(objects(&m).is_empty() && players(&m).is_empty() && nebulae(&m).is_empty());
        assert_eq!((world_half(&m), name(&m), skirmish(&m), has_backdrop(&m)), (None, None, false, false));
        assert!(objects(&json!({"objects": 5})).is_empty());
    }

    #[test]
    fn entry_fields() {
        let m = sample();
        let o = &objects(&m)[0];
        assert_eq!((id(o), kind(o), scene(o), pos(o)), (Some(1), Some("Island04"), Some("Island04"), Some([-300.0, 300.0, 5.0])));
        assert_eq!(rot(o), Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]));
        let rock = &objects(&m)[1];
        assert_eq!((pos(rock), rot(rock), scene(rock)), (Some([10.0, 20.0, 0.0]), None, None));
        let decor = &objects(&m)[2];
        assert_eq!((id(decor), pos(decor)), (None, None));
        assert_eq!(ship_player(&ships(&m)[0]), Some(0));
        assert_eq!(rot(&json!({"rot": [1, 2]})), None);
        assert_eq!(pos(&json!({"pos": [1, "x", 3]})), None);
        assert_eq!(pos(&json!({"pos": [1]})), None);
    }

    #[test]
    fn players_teams_nebulae() {
        let m = sample();
        let p = &players(&m)[0];
        assert_eq!((player_name(p), player_team(p), player_formation(p), player_race(p)), (Some("P1"), Some(0), Some(2), Some(0)));
        assert_eq!((player_start(p), player_forward(p), player_playable(p)), (Some([100.0, 0.0, 0.0]), Some([0.0, 1.0, 0.0]), true));
        assert_eq!(team_name_id(&teams(&m)[0]), Some("IDGS_TEAM_A"));
        let n = &nebulae(&m)[0];
        assert_eq!(nebula_name(n), Some("Storm"));
        assert_eq!(nebula_polygon(n), vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        assert!(!player_playable(&json!({})));
    }

    #[test]
    fn ids_span_objects_ships_and_mines() {
        let m = sample();
        assert_eq!(used_ids(&m), BTreeSet::from([1, 2, 3, 4]));
        assert_eq!(next_id(&m, &[]), 5);
        assert_eq!(next_id(&json!({"objects": [{"id": 2}]}), &[1]), 3);
        assert_eq!(next_id(&json!({}), &[]), 1);
        assert!(duplicate_ids(&m).is_empty());
        let d = json!({"objects": [{"id": 7}, {"id": 7}], "mines": [{"id": 7}, {"id": 8}, {"id": 8}]});
        assert_eq!(duplicate_ids(&d), vec![7, 8]);
    }
}
