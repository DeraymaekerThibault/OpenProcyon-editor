//! What the mission plan writes (docs/phase5.md P3, work item T7): add, delete, rename, move, turn and edit the vertices of the
//! named geometry of a mission (volumes, paths, point sets) and of its objective points, map text, ships and start marker, each as
//! ONE undo step of [`Project`]. Names no mission key: that is [`super::doc`]. No drawing: [`super::plan`] draws and calls this.
//!
//! - Renaming follows references: every rule field that names the thing (kinds `volume`, `path`, `point`, `point_set`, `map_text`,
//!   from [`super::refs`], i.e. from the engine's rule schema) is rewritten in the same step as the definition. A volume that is a
//!   nebula's `Polygon Name` is followed too.
//! - A volume that is a nebula's polygon (the mission record's `Polygon Name`) is kept equal to the map's nebula polygon: an edit of
//!   the volume writes the map's polygon in the same step ("nebula agreement", docs/phase5.md engine requirement 5; the check on the
//!   engine's side is still missing, so this is PROVISIONAL).
//! - Deleting is refused while a rule, a task or a nebula still reads the thing, and where the file cannot say "remove this"
//!   ([`check_remove_key`], [`check_add_delete`]).
//!
//! PROVISIONAL (guesses, no engine rule behind them): a path keeps at least two points and a point set at least one (a volume keeps
//! [`MIN_POINTS`], the engine's `MIN_VOLUME_VERTICES`); the size and shape of a new volume, path and point set; the 50-unit step
//! that a new vertex past the end of a path or a point set takes; a new label starts visible with its own name as text.

use std::collections::BTreeSet;

use serde_json::Value;

use super::doc::{self as mdoc, GeoTable, MissionList};
use super::project::{add_entry, check_add_delete, check_remove_key, delete_entry, mission_rel};
use super::refs::{self, FieldIndex};
use crate::edit::{self, Edit};
use crate::map::doc as mapdoc;
use crate::map::nebula::MIN_POINTS;
use crate::map::project as maps;
use crate::map_edit::orient::{self, IDENTITY};
use crate::map_edit::select::{self, GroupTransform, Placement};
use crate::project::Project;

/// The fewest points of a path and of a point set. PROVISIONAL.
pub const MIN_PATH_POINTS: usize = 2;
pub const MIN_SET_POINTS: usize = 1;
/// How far a vertex added past the last one goes, world units. PROVISIONAL.
pub const EXTEND_STEP: f64 = 50.0;
/// Half the size of a new volume (a triangle round the click), and half the length of a new path. PROVISIONAL.
pub const NEW_SIZE: f64 = 100.0;

/// The kinds of named thing the plan adds, deletes and renames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Volume,
    Path,
    Point,
    PointSet,
    MapText,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Volume, Kind::Path, Kind::Point, Kind::PointSet, Kind::MapText];

    /// The rule schema's field kind that names one (`refs::NAME_KINDS`).
    pub fn ref_kind(self) -> &'static str {
        match self {
            Kind::Volume => "volume",
            Kind::Path => "path",
            Kind::Point => "point",
            Kind::PointSet => "point_set",
            Kind::MapText => "map_text",
        }
    }

    pub fn noun(self) -> &'static str {
        match self {
            Kind::Volume => "volume",
            Kind::Path => "path",
            Kind::Point => "point",
            Kind::PointSet => "point set",
            Kind::MapText => "map text",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Kind::Volume => "Volumes",
            Kind::Path => "Paths",
            Kind::Point => "Points",
            Kind::PointSet => "Point sets",
            Kind::MapText => "Map text",
        }
    }

    /// The object table of a kind kept as one (volumes, paths, point sets).
    pub fn table(self) -> Option<GeoTable> {
        match self {
            Kind::Volume => Some(GeoTable::Volumes),
            Kind::Path => Some(GeoTable::Paths),
            Kind::PointSet => Some(GeoTable::PointSets),
            _ => None,
        }
    }

    /// The list of a kind kept as one (points, map text).
    pub fn list(self) -> Option<MissionList> {
        match self {
            Kind::Point => Some(MissionList::Points),
            Kind::MapText => Some(MissionList::MapText),
            _ => None,
        }
    }

    /// The fewest vertices an entry of a table kind keeps.
    pub fn min_items(self) -> usize {
        match self {
            Kind::Volume => MIN_POINTS,
            Kind::Path => MIN_PATH_POINTS,
            _ => MIN_SET_POINTS,
        }
    }
}

// ---- reading ----

/// The names of a kind, in file order.
pub fn names(mission: &Value, kind: Kind) -> Vec<String> {
    match kind {
        Kind::Point => mdoc::points(mission).iter().filter_map(|p| mdoc::point_name(p).map(str::to_string)).collect(),
        Kind::MapText => mdoc::map_text(mission).iter().filter_map(|p| mdoc::map_text_name(p).map(str::to_string)).collect(),
        _ => kind.table().and_then(|t| t.get(mission)).map_or_else(Vec::new, |o| o.keys().cloned().collect()),
    }
}

/// A name no thing of the kind has: `Volume1`, `Volume2`, ...
pub fn free_name(mission: &Value, kind: Kind) -> String {
    let have = names(mission, kind);
    let stem = match kind {
        Kind::Volume => "Volume",
        Kind::Path => "Path",
        Kind::Point => "Point",
        Kind::PointSet => "PointSet",
        Kind::MapText => "Label",
    };
    (1..).map(|i| format!("{stem}{i}")).find(|n| !have.contains(n)).expect("a free name exists")
}

/// A name this tool writes: not empty or only spaces, and not taken (`old` is the name being renamed, which may stay).
pub fn check_name(mission: &Value, kind: Kind, name: &str, old: Option<&str>) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("a {} needs a name", kind.noun()));
    }
    if old != Some(name) && names(mission, kind).iter().any(|n| n == name) {
        return Err(format!("there is a {} called '{name}' already", kind.noun()));
    }
    Ok(())
}

/// How many rule and task fields name `name` as `kind`, and for a volume how many nebula records use it as their polygon.
pub fn uses(mission: &Value, kind: Kind, name: &str) -> usize {
    let rules = refs::references_of(mission, kind.ref_kind(), name).len();
    let nebulae = if kind == Kind::Volume { mdoc::mission_nebulae(mission).iter().filter(|r| mdoc::record_polygon_name(r) == Some(name)).count() } else { 0 };
    rules + nebulae
}

fn num(x: f64) -> Value {
    crate::map_view::short(x as f32)
}

// ---- items on the plan: what a drag or a turn moves ----

/// One thing on the plan that has a position the plan can move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Ship(usize),
    /// The human's start (`player_start.start`).
    Start,
    Point(usize),
    Text(usize),
    /// Vertex `index` of volume or path `name`, or dot `index` of point set `name`.
    Vertex(GeoTable, String, usize),
}

impl Item {
    /// The pointer of the position array.
    pub fn pointer(&self) -> String {
        match self {
            Item::Ship(i) => mdoc::ship_pointer(*i, Some(mdoc::SHIP_POS)),
            Item::Start => mdoc::player_start_pointer(Some(mdoc::START_POS)),
            Item::Point(i) => mdoc::point_pointer(*i, Some(mdoc::POINT_POSITION)),
            Item::Text(i) => mdoc::map_text_pointer(*i, Some(mdoc::MAP_TEXT_POS)),
            Item::Vertex(GeoTable::PointSets, name, i) => mdoc::point_set_dot_pos_pointer(name, *i),
            Item::Vertex(t, name, i) => t.item_pointer(name, *i),
        }
    }
}

/// The position of an item (z is 0 when it has none), `None` when the document has no such array.
pub fn item_xyz(mission: &Value, item: &Item) -> Option<[f64; 3]> {
    let a = mission.pointer(&item.pointer())?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2).and_then(Value::as_f64).unwrap_or(0.0)])
}

/// `old` (a position array) with its x and y replaced, whatever else it holds kept.
fn with_xy(old: Option<&Value>, xy: [f64; 2]) -> Value {
    let mut a = old.and_then(Value::as_array).cloned().unwrap_or_default();
    while a.len() < 2 {
        a.push(Value::from(0));
    }
    a[0] = num(xy[0]);
    a[1] = num(xy[1]);
    Value::Array(a)
}

/// The heading of ship `index` in radians, clockwise from +y (0 without a `rot`).
pub fn ship_heading(mission: &Value, index: usize) -> f64 {
    mdoc::ships(mission).get(index).and_then(mdoc::ship_rot).map_or(0.0, |r| orient::yaw_of(&r))
}

/// The heading of the human's start (0 without a `forward`).
pub fn start_heading(mission: &Value) -> f64 {
    mdoc::player_start_forward(mission).filter(|f| f[0] != 0.0 || f[1] != 0.0).map_or(0.0, |f| f[0].atan2(f[1]))
}

/// What a finished drag by `delta` writes: the new x and y of each item, the rest of the array kept. Items already there are left out.
pub fn move_edits(mission: &Value, items: &[Item], delta: [f64; 2]) -> Vec<Edit> {
    items
        .iter()
        .filter_map(|it| {
            let p = item_xyz(mission, it)?;
            let xy = [num(p[0] + delta[0]).as_f64()?, num(p[1] + delta[1]).as_f64()?];
            (xy != [p[0], p[1]]).then(|| Edit::set(it.pointer(), with_xy(mission.pointer(&it.pointer()), xy)))
        })
        .collect()
}

/// The middle of the items (bounding box): what a turn is about.
pub fn centre(mission: &Value, items: &[Item]) -> Option<[f64; 2]> {
    let ps: Vec<(usize, Placement)> = items.iter().enumerate().filter_map(|(n, it)| Some((n, Placement { pos: item_xyz(mission, it)?, rot: IDENTITY }))).collect();
    select::group_centre(&ps)
}

/// What turning the items clockwise by `yaw` radians about their centre writes: new positions, a turned `rot` for ships (the tilt is
/// kept), a turned `forward` for the start.
pub fn turn_edits(mission: &Value, items: &[Item], yaw: f64) -> Vec<Edit> {
    if yaw == 0.0 || !yaw.is_finite() {
        return vec![];
    }
    let before: Vec<(usize, Placement)> = items
        .iter()
        .enumerate()
        .filter_map(|(n, it)| {
            let rot = match it {
                Item::Ship(i) => mdoc::ships(mission).get(*i).and_then(mdoc::ship_rot).unwrap_or(IDENTITY),
                _ => IDENTITY,
            };
            Some((n, Placement { pos: item_xyz(mission, it)?, rot }))
        })
        .collect();
    let after = select::transform_group(&before, GroupTransform { delta: [0.0, 0.0], yaw });
    let (s, c) = yaw.sin_cos();
    let mut out = vec![];
    for (n, p) in after {
        let it = &items[n];
        out.push(Edit::set(it.pointer(), with_xy(mission.pointer(&it.pointer()), [p.pos[0], p.pos[1]])));
        match it {
            Item::Ship(i) => out.push(Edit::set(mdoc::ship_pointer(*i, Some(mdoc::SHIP_ROT)), Value::Array(p.rot.iter().map(|x| num(*x)).collect()))),
            Item::Start => {
                if let Some(f) = mdoc::player_start_forward(mission) {
                    let to = [f[0] * c + f[1] * s, -f[0] * s + f[1] * c, f[2]];
                    out.push(Edit::set(mdoc::player_start_pointer(Some(mdoc::START_FORWARD)), Value::Array(to.iter().map(|x| num(*x)).collect())));
                }
            }
            _ => {}
        }
    }
    out
}

// ---- vertices (pure) ----

fn vertex_xyz(table: GeoTable, v: &Value) -> Option<[f64; 3]> {
    if table == GeoTable::PointSets {
        return mdoc::point_set_dot_pos(v);
    }
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2).and_then(Value::as_f64).unwrap_or(0.0)])
}

fn make_vertex(table: GeoTable, p: [f64; 3]) -> Value {
    match table {
        GeoTable::Volumes => Value::Array(vec![num(p[0]), num(p[1])]),
        GeoTable::Paths => Value::Array(vec![num(p[0]), num(p[1]), num(p[2])]),
        GeoTable::PointSets => mdoc::new_point_set_dot([num(p[0]).as_f64().unwrap_or(p[0]), num(p[1]).as_f64().unwrap_or(p[1]), p[2]]),
    }
}

/// The vertices with a new one after `after`: halfway to the next (a volume is closed, so after the last it is halfway to the first),
/// or [`EXTEND_STEP`] further along for the end of a path or a point set. The new point's z is the mean of its neighbours'.
pub fn inserted(table: GeoTable, items: &[Value], after: usize) -> Result<Vec<Value>, String> {
    let a = items.get(after).and_then(|v| vertex_xyz(table, v)).ok_or_else(|| format!("no vertex {after}"))?;
    let next = if after + 1 < items.len() {
        items.get(after + 1).and_then(|v| vertex_xyz(table, v))
    } else if table == GeoTable::Volumes {
        items.first().and_then(|v| vertex_xyz(table, v))
    } else {
        None
    };
    let p = match next {
        Some(b) if items.len() > 1 => [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0],
        _ => {
            let prev = after.checked_sub(1).and_then(|i| items.get(i)).and_then(|v| vertex_xyz(table, v));
            match prev {
                Some(q) => {
                    let (dx, dy) = (a[0] - q[0], a[1] - q[1]);
                    let l = dx.hypot(dy);
                    if l > 0.0 { [a[0] + dx / l * EXTEND_STEP, a[1] + dy / l * EXTEND_STEP, a[2]] } else { [a[0] + EXTEND_STEP, a[1], a[2]] }
                }
                None => [a[0] + EXTEND_STEP, a[1], a[2]],
            }
        }
    };
    let mut v = items.to_vec();
    v.insert(after + 1, make_vertex(table, p));
    Ok(v)
}

/// The vertices without vertex `i`, refused below the kind's minimum.
pub fn removed(kind: Kind, items: &[Value], i: usize) -> Result<Vec<Value>, String> {
    if i >= items.len() {
        return Err(format!("no vertex {i}"));
    }
    if items.len() <= kind.min_items() {
        return Err(format!("a {} keeps at least {} point{}: delete the {} instead", kind.noun(), kind.min_items(), if kind.min_items() == 1 { "" } else { "s" }, kind.noun()));
    }
    let mut v = items.to_vec();
    v.remove(i);
    Ok(v)
}

/// The vertices of a new volume, path or point set at `at`. PROVISIONAL (shape and size).
pub fn new_items(kind: Kind, at: [f64; 2]) -> Value {
    let [x, y] = at;
    let h = NEW_SIZE * 0.866;
    match kind {
        Kind::Volume => {
            Value::Array([[x, y + NEW_SIZE], [x + h, y - NEW_SIZE / 2.0], [x - h, y - NEW_SIZE / 2.0]].iter().map(|p| make_vertex(GeoTable::Volumes, [p[0], p[1], 0.0])).collect())
        }
        Kind::Path => Value::Array(vec![make_vertex(GeoTable::Paths, [x - NEW_SIZE, y, 0.0]), make_vertex(GeoTable::Paths, [x + NEW_SIZE, y, 0.0])]),
        _ => Value::Array(vec![make_vertex(GeoTable::PointSets, [x, y, 0.0])]),
    }
}

// ---- writing, through the project ----

/// Applies `edits` to the mission `id` as ONE undo step labelled `label`; the volumes they touch are written to the map's nebula
/// polygon too when a mission record links them (one group step over both files).
pub fn commit(project: &mut Project, id: &str, label: &str, edits: Vec<Edit>) -> Result<(), String> {
    if edits.is_empty() {
        return Ok(());
    }
    let rel = mission_rel(id)?;
    project.open_doc(&rel)?;
    let mut after = project.doc(&rel).ok_or_else(|| format!("{rel} is not open"))?.edited.clone();
    let mut touched = BTreeSet::new();
    for e in &edits {
        edit::apply(&mut after, e)?;
        let t = edit::tokens(&e.pointer);
        if t.first().map(String::as_str) == Some(mdoc::KEY_VOLUMES) {
            match t.get(1) {
                Some(name) => {
                    touched.insert(name.clone());
                }
                None => touched.extend(mdoc::volume_names(&after).into_iter().map(str::to_string)),
            }
        }
    }
    let mut steps = vec![(rel, edits)];
    if let Some(step) = nebula_sync(project, id, &after, &touched) {
        steps.push(step);
    }
    project.do_group(label, steps)
}

/// The map's nebula polygons that must follow the touched volumes: a mission record `New Nebula Name` = a nebula's `name`, its
/// `Polygon Name` = the volume.
fn nebula_sync(project: &mut Project, id: &str, after: &Value, touched: &BTreeSet<String>) -> Option<(String, Vec<Edit>)> {
    if touched.is_empty() {
        return None;
    }
    maps::open_map(project, id).ok()?;
    let map = maps::merged_view(project, id)?;
    let nebulae = mapdoc::nebulae(map);
    let mut edits = vec![];
    for rec in mdoc::mission_nebulae(after) {
        let (Some(name), Some(pn)) = (mdoc::record_name(rec), mdoc::record_polygon_name(rec)) else { continue };
        if !touched.contains(pn) {
            continue;
        }
        let Some(poly) = mdoc::mission_volume(after, pn).filter(|p| p.len() >= MIN_POINTS) else { continue };
        let Some(i) = nebulae.iter().position(|n| mapdoc::nebula_name(n) == Some(name)) else { continue };
        if mapdoc::nebula_polygon(&nebulae[i]) != poly {
            edits.push(Edit::set(mapdoc::nebula_pointer(i, &[mapdoc::NEBULA_POLYGON]), mapdoc::polygon_value(&poly)));
        }
    }
    (!edits.is_empty()).then(|| (maps::map_rel(id).ok().unwrap_or_default(), edits))
}

fn mission_value(project: &mut Project, id: &str) -> Result<(String, Value), String> {
    let rel = mission_rel(id)?;
    project.open_doc(&rel)?;
    let v = project.doc(&rel).map(|d| d.edited.clone()).ok_or_else(|| format!("{rel} is not open"))?;
    Ok((rel, v))
}

/// Adds a new thing of `kind` called `name` at `at` (game x, y) as one undo step. Refused for an empty or taken name, and for a point
/// or a label where the mission's list cannot take an entry ([`add_entry`]).
pub fn add(project: &mut Project, id: &str, kind: Kind, name: &str, at: [f64; 2]) -> Result<(), String> {
    let (_, m) = mission_value(project, id)?;
    check_name(&m, kind, name, None)?;
    let label = format!("Add {} {name}", kind.noun());
    match (kind.table(), kind) {
        (Some(t), _) => commit(project, id, &label, vec![t.insert_edit(&m, name, new_items(kind, at))]),
        (None, Kind::Point) => add_entry(project, id, MissionList::Points, mdoc::new_point(name, [at[0], at[1], 0.0])).map(|_| ()),
        _ => add_entry(project, id, MissionList::MapText, mdoc::new_map_text(name, name, at)).map(|_| ()),
    }
}

/// Deletes `name` as one undo step. Refused while a rule, a task or a nebula reads it (the error counts them), and where the file
/// cannot remove an entry.
pub fn delete(project: &mut Project, id: &str, kind: Kind, name: &str) -> Result<(), String> {
    let (rel, m) = mission_value(project, id)?;
    if !names(&m, kind).iter().any(|n| n == name) {
        return Err(format!("no {} '{name}'", kind.noun()));
    }
    let n = uses(&m, kind, name);
    if n > 0 {
        return Err(format!("'{name}' is used by {n} rule, task or nebula field{}: change those first", if n == 1 { "" } else { "s" }));
    }
    match kind.table() {
        Some(t) => {
            check_remove_key(project, &rel)?;
            commit(project, id, &format!("Delete {} {name}", kind.noun()), vec![Edit::remove(t.entry_pointer(name))])
        }
        None => {
            let list = kind.list().expect("a list kind");
            let index = index_of(&m, kind, name).ok_or_else(|| format!("no {} '{name}'", kind.noun()))?;
            delete_entry(project, id, list, index).map(|_| ())
        }
    }
}

/// The index of a point or a label in its list.
pub fn index_of(mission: &Value, kind: Kind, name: &str) -> Option<usize> {
    match kind {
        Kind::Point => mdoc::points(mission).iter().position(|p| mdoc::point_name(p) == Some(name)),
        Kind::MapText => mdoc::map_text(mission).iter().position(|p| mdoc::map_text_name(p) == Some(name)),
        _ => None,
    }
}

/// Renames `old` to `new` and every rule field that names it, in ONE undo step. Refused for an empty or taken name, and where the
/// file cannot remove the old key (a mission that extends another cannot remove a table key, nor a keyed point).
pub fn rename(project: &mut Project, id: &str, kind: Kind, old: &str, new: &str) -> Result<(), String> {
    let (rel, m) = mission_value(project, id)?;
    check_name(&m, kind, new, Some(old))?;
    if old == new {
        return Ok(());
    }
    if !names(&m, kind).iter().any(|n| n == old) {
        return Err(format!("no {} '{old}'", kind.noun()));
    }
    let mut edits = match kind.table() {
        Some(t) => {
            check_remove_key(project, &rel)?;
            let table = t.get(&m).ok_or("the table is gone")?;
            // the same entries in the same order, the one key renamed
            let renamed: serde_json::Map<String, Value> = table.iter().map(|(k, v)| (if k == old { new.to_string() } else { k.clone() }, v.clone())).collect();
            vec![Edit::set(t.pointer(), Value::Object(renamed))]
        }
        None => {
            let index = index_of(&m, kind, old).ok_or_else(|| format!("no {} '{old}'", kind.noun()))?;
            let key = if kind == Kind::Point {
                check_add_delete(project, id, MissionList::Points).map_err(|e| format!("a point's name is its key: {e}"))?;
                mdoc::point_pointer(index, Some(mdoc::POINT_NAME))
            } else {
                mdoc::map_text_pointer(index, Some(mdoc::MAP_TEXT_NAME))
            };
            vec![Edit::set(key, Value::from(new))]
        }
    };
    edits.extend(refs::rename_edits(&m, FieldIndex::engine(), kind.ref_kind(), old, new)?);
    if kind == Kind::Volume {
        for (i, rec) in mdoc::mission_nebulae(&m).iter().enumerate() {
            if mdoc::record_polygon_name(rec) == Some(old) {
                edits.push(Edit::set(mdoc::record_pointer(i, mdoc::RecordField::PolygonName), Value::from(new)));
            }
        }
    }
    commit(project, id, &format!("Rename {} {old} to {new}", kind.noun()), edits)
}

/// Moves the items by `delta` as one undo step.
pub fn move_items(project: &mut Project, id: &str, items: &[Item], delta: [f64; 2]) -> Result<(), String> {
    let (_, m) = mission_value(project, id)?;
    commit(project, id, &move_label(items), move_edits(&m, items, delta))
}

/// Turns the items clockwise by `yaw` radians about their centre as one undo step.
pub fn turn_items(project: &mut Project, id: &str, items: &[Item], yaw: f64) -> Result<(), String> {
    let (_, m) = mission_value(project, id)?;
    commit(project, id, &move_label(items).replacen("Move", "Turn", 1), turn_edits(&m, items, yaw))
}

fn move_label(items: &[Item]) -> String {
    match items {
        [Item::Ship(i)] => format!("Move ship #{i}"),
        [Item::Start] => "Move start".into(),
        [Item::Point(i)] => format!("Move point #{i}"),
        [Item::Text(i)] => format!("Move map text #{i}"),
        [Item::Vertex(t, name, i)] => format!("Move vertex {i} of {} {name}", vertex_noun(*t)),
        _ => format!("Move {} items", items.len()),
    }
}

fn vertex_noun(t: GeoTable) -> &'static str {
    match t {
        GeoTable::Volumes => "volume",
        GeoTable::Paths => "path",
        GeoTable::PointSets => "point set",
    }
}

/// Adds a vertex (a dot, for a point set) after vertex `after` of `name` as one undo step.
pub fn insert_vertex(project: &mut Project, id: &str, kind: Kind, name: &str, after: usize) -> Result<(), String> {
    let t = kind.table().ok_or_else(|| format!("a {} has no vertices", kind.noun()))?;
    let (_, m) = mission_value(project, id)?;
    let items = inserted(t, mdoc::geo_items(&m, t, name), after)?;
    commit(project, id, &format!("Add point to {} {name}", kind.noun()), vec![Edit::set(t.entry_pointer(name), Value::Array(items))])
}

/// Removes vertex `index` of `name` (never below the kind's minimum) as one undo step.
pub fn remove_vertex(project: &mut Project, id: &str, kind: Kind, name: &str, index: usize) -> Result<(), String> {
    let t = kind.table().ok_or_else(|| format!("a {} has no vertices", kind.noun()))?;
    let (_, m) = mission_value(project, id)?;
    let items = removed(kind, mdoc::geo_items(&m, t, name), index)?;
    commit(project, id, &format!("Delete point {index} of {} {name}", kind.noun()), vec![Edit::set(t.entry_pointer(name), Value::Array(items))])
}

/// Sets a vertex's z (a path's height, a point set dot's) as one undo step.
pub fn set_z(project: &mut Project, id: &str, table: GeoTable, name: &str, index: usize, z: f64) -> Result<(), String> {
    if table == GeoTable::Volumes {
        return Err("a volume is flat".into());
    }
    let (_, m) = mission_value(project, id)?;
    let item = Item::Vertex(table, name.to_string(), index);
    let mut a = m.pointer(&item.pointer()).and_then(Value::as_array).cloned().ok_or_else(|| format!("no point {index} in '{name}'"))?;
    while a.len() < 3 {
        a.push(Value::from(0));
    }
    a[2] = num(z);
    commit(project, id, &format!("Set height of point {index} of {} {name}", vertex_noun(table)), vec![Edit::set(item.pointer(), Value::Array(a))])
}

/// Sets a label's text as one undo step.
pub fn set_map_text(project: &mut Project, id: &str, index: usize, text: &str) -> Result<(), String> {
    commit(project, id, "Edit map text", vec![Edit::set(mdoc::map_text_pointer(index, Some(mdoc::MAP_TEXT_TEXT)), Value::from(text))])
}

/// Shows or hides a label as one undo step.
pub fn set_map_text_visible(project: &mut Project, id: &str, index: usize, visible: bool) -> Result<(), String> {
    commit(project, id, "Show or hide map text", vec![Edit::set(mdoc::map_text_pointer(index, Some(mdoc::MAP_TEXT_VISIBLE)), Value::Bool(visible))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m() -> Value {
        json!({
            "ships": [{"id": 1, "pos": [10.0, 20.0, 5.0], "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1]}],
            "player_start": {"start": [0, 0, 0], "forward": [0, 1, 0]},
            "volumes": {"A": [[0, 0], [10, 0], [10, 10]]},
            "paths": {"P": [[0, 0, 1], [10, 0, 3]]},
            "environment": {"point_sets": {"S": [{"pos": [1, 2, 3], "x": 1}]}}
        })
    }

    #[test]
    fn move_keeps_z_and_other_keys() {
        let e = move_edits(&m(), &[Item::Ship(0), Item::Vertex(GeoTable::Paths, "P".into(), 1), Item::Vertex(GeoTable::PointSets, "S".into(), 0)], [5.0, -5.0]);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].new, Some(json!([15.0, 15.0, 5.0])));
        assert_eq!(e[1].new, Some(json!([15.0, -5.0, 3])));
        assert_eq!(e[2].pointer, "/environment/point_sets/S/0/pos");
        assert!(move_edits(&m(), &[Item::Ship(0)], [0.0, 0.0]).is_empty());
    }

    #[test]
    fn insert_midpoint_wraps_and_extends() {
        let v = m();
        let a = inserted(GeoTable::Volumes, mdoc::geo_items(&v, GeoTable::Volumes, "A"), 2).unwrap();
        assert_eq!(a.len(), 4);
        assert_eq!(a[3], json!([5.0, 5.0]));
        let p = inserted(GeoTable::Paths, mdoc::geo_items(&v, GeoTable::Paths, "P"), 1).unwrap();
        assert_eq!(p[2], json!([60.0, 0.0, 3.0]));
        let s = inserted(GeoTable::PointSets, mdoc::geo_items(&v, GeoTable::PointSets, "S"), 0).unwrap();
        assert_eq!(s.len(), 2);
        assert!(inserted(GeoTable::Paths, &[], 0).is_err());
    }

    #[test]
    fn removal_keeps_the_minimum() {
        let v = m();
        assert!(removed(Kind::Volume, mdoc::geo_items(&v, GeoTable::Volumes, "A"), 0).is_err());
        assert!(removed(Kind::Path, mdoc::geo_items(&v, GeoTable::Paths, "P"), 0).is_err());
        assert!(removed(Kind::PointSet, mdoc::geo_items(&v, GeoTable::PointSets, "S"), 0).is_err());
        assert_eq!(removed(Kind::Volume, &[json!([0, 0]), json!([1, 0]), json!([1, 1]), json!([0, 1])], 3).unwrap().len(), 3);
    }

    #[test]
    fn turn_rotates_positions_rot_and_forward() {
        let e = turn_edits(&m(), &[Item::Ship(0), Item::Start], std::f64::consts::FRAC_PI_2);
        let fwd = e.iter().find(|x| x.pointer == "/player_start/forward").unwrap().new.clone().unwrap();
        let f: Vec<f64> = fwd.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        assert!((f[0] - 1.0).abs() < 1e-5 && f[1].abs() < 1e-5, "{f:?}");
        assert!(e.iter().any(|x| x.pointer == "/ships/0/rot"));
        assert!(turn_edits(&m(), &[Item::Ship(0)], 0.0).is_empty());
    }

    #[test]
    fn names_and_free_names() {
        let v = m();
        assert_eq!(names(&v, Kind::Volume), ["A"]);
        assert_eq!(free_name(&v, Kind::Volume), "Volume1");
        assert!(check_name(&v, Kind::Volume, "A", None).is_err());
        assert!(check_name(&v, Kind::Volume, "A", Some("A")).is_ok());
        assert!(check_name(&v, Kind::Path, "  ", None).is_err());
    }
}
