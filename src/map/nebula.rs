//! Nebulae of a map (docs/phase4.md P6, work item T8): polygon geometry for the plan view and the three-record writer.
//!
//! A nebula is described up to three times and the engine reads each (`game/src/nebula.rs`):
//!
//! 1. `map.json` `nebulae[]`: `name`, `polygon`, `cloud`, `storm`, `meteors`, `sound_distance`. `spawn` (nebula.rs:570-604) makes one
//!    effect per `cloud.points[]` / `storm.points[]`, `storm_circles` (nebula.rs:233-252) and the storm hazards read `polygon`.
//! 2. `mission.json` `environment.nebulae[]`: a raw Setup Nebula record. `polygons` (nebula.rs:553-567), `crew.rs:239-243` and
//!    `environment.rs:734-736` (occlusion) find the nebula by `New Nebula Name` and its polygon by `Polygon Name`.
//! 3. `mission.json` `volumes[<Polygon Name>]`: the polygon those two read for "inside" and for sight occlusion.
//!
//! The link is by name: record `New Nebula Name` == `nebulae[].name`, record `Polygon Name` == a key of `volumes` (docs/missions.md
//! section 8). Every function here that changes one of them changes the others in the same [`Project::do_group`] step, so one Undo
//! takes back all of it and a drag of a polygon point keeps the polygon of the map and of the mission equal. A map with no
//! `mission.json` (a skirmish map) has the map record only (PROVISIONAL: occlusion needs the mission side). A mission that has no
//! record for a nebula is left as it is and the tab says so ([`Link::Missing`]).
//!
//! The mission's `environment.nebulae` is not merged by key (the engine has no rule for it), so on a map that extends a retail mission
//! a nebula cannot be added or deleted without restating the retail list; the refusal says so. Names no map key: that is [`super::doc`].

use serde_json::Value;

use super::doc::{self, Array, NebulaFlag};
use super::project::{self as maps, check_add_delete, check_key, is_whole, map_rel};
use crate::edit::Edit;
use crate::mission::doc::{self as mdoc, RecordField};
use crate::project::{Project, keyed};

/// The fewest points of a polygon.
pub const MIN_POINTS: usize = 3;

// ---- geometry (no project) ----

/// The middle of the points (their mean) and the mean distance from it: where a new effect point goes and how big (PROVISIONAL: the
/// engine's own stand-in for a storm, `nebula.rs` `storm_circles`, uses the same mean distance).
pub fn centre_radius(pts: &[[f64; 2]]) -> ([f64; 2], f64) {
    if pts.is_empty() {
        return ([0.0, 0.0], 0.0);
    }
    let n = pts.len() as f64;
    let c = [pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n];
    let r = pts.iter().map(|p| (p[0] - c[0]).hypot(p[1] - c[1])).sum::<f64>() / n;
    (c, r)
}

/// The vertex of `pts` closest to `p` within `max` world units.
pub fn nearest_vertex(pts: &[[f64; 2]], p: [f64; 2], max: f64) -> Option<usize> {
    pts.iter().enumerate().map(|(i, q)| (i, (q[0] - p[0]).hypot(q[1] - p[1]))).filter(|(_, d)| *d <= max).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
}

/// The edge closest to `p` within `max` world units: the index of the edge's first point (the new point goes after it) and the
/// closest point on the edge. The polygon is closed: the last edge runs from the last point to the first.
pub fn nearest_edge(pts: &[[f64; 2]], p: [f64; 2], max: f64) -> Option<(usize, [f64; 2])> {
    if pts.len() < 2 {
        return None;
    }
    (0..pts.len())
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) };
            let q = [a[0] + t * dx, a[1] + t * dy];
            (i, q, (q[0] - p[0]).hypot(q[1] - p[1]))
        })
        .filter(|(_, _, d)| *d <= max)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(i, q, _)| (i, q))
}

/// Whether `p` is inside the polygon (even-odd rule).
pub fn contains(pts: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The polygon with vertex `i` moved to `to`.
pub fn with_moved(pts: &[[f64; 2]], i: usize, to: [f64; 2]) -> Result<Vec<[f64; 2]>, String> {
    let mut v = pts.to_vec();
    *v.get_mut(i).ok_or_else(|| format!("no vertex {i}"))? = to;
    Ok(v)
}

/// The polygon with `p` inserted after vertex `after` (a closed polygon: after the last point is between it and the first).
pub fn with_inserted(pts: &[[f64; 2]], after: usize, p: [f64; 2]) -> Result<Vec<[f64; 2]>, String> {
    if after >= pts.len() {
        return Err(format!("no edge {after}"));
    }
    let mut v = pts.to_vec();
    v.insert(after + 1, p);
    Ok(v)
}

/// The polygon without vertex `i`; refused when it would leave fewer than [`MIN_POINTS`].
pub fn with_removed(pts: &[[f64; 2]], i: usize) -> Result<Vec<[f64; 2]>, String> {
    if i >= pts.len() {
        return Err(format!("no vertex {i}"));
    }
    if pts.len() <= MIN_POINTS {
        return Err(format!("a nebula keeps at least {MIN_POINTS} points: delete the nebula instead"));
    }
    let mut v = pts.to_vec();
    v.remove(i);
    Ok(v)
}

/// `name` is a name this tool writes: not empty or only spaces.
fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() { Err("a nebula needs a name".into()) } else { Ok(()) }
}

fn check_polygon(pts: &[[f64; 2]]) -> Result<(), String> {
    if pts.len() < MIN_POINTS {
        return Err(format!("a polygon needs at least {MIN_POINTS} points, this one has {}", pts.len()));
    }
    if pts.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
        return Err("a polygon point is not a number".into());
    }
    Ok(())
}

/// A name no nebula of `names` has: `Nebula1`, `Nebula2`, ...
pub fn free_name(names: &[String]) -> String {
    (1..).map(|i| format!("Nebula{i}")).find(|n| !names.contains(n)).expect("a free name exists")
}

// ---- reading, through the project ----

/// How a nebula's map entry is linked to the mission.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    /// The map has no `mission.json`: only the map record exists (a skirmish map; PROVISIONAL).
    NoMission,
    /// The mission has the record and its volume; `in_sync` is whether the volume's polygon equals the map's.
    Linked { record: usize, polygon_name: String, in_sync: bool },
    /// The mission exists but has no record of this name: edits touch the map record only.
    Missing,
}

/// The path of the map's mission inside the mod (`maps/<id>/mission.json`).
pub fn mission_rel(id: &str) -> Result<String, String> {
    map_rel(id)?;
    Ok(format!("maps/{id}/{}", mdoc::MISSION_FILE))
}

/// The mission of the map as the game sees it, opened as a project document; `None` when the map has none.
pub fn mission_view<'a>(project: &'a mut Project, id: &str) -> Option<&'a Value> {
    let rel = mission_rel(id).ok()?;
    let exists = project.doc(&rel).is_some_and(|d| d.edited.is_object()) || project.dir.join(&rel).is_file() || project.import.as_ref().is_some_and(|i| i.join(&rel).is_file());
    if !exists {
        return None;
    }
    let i = project.open_doc(&rel).ok()?;
    Some(&project.docs[i].edited).filter(|m| m.is_object())
}

/// The names of the nebulae of the map, in order.
pub fn names(map: &Value) -> Vec<String> {
    doc::nebulae(map).iter().map(|n| doc::nebula_name(n).unwrap_or("").to_string()).collect()
}

/// The link of the nebula `index` (of the map's `nebulae`) to the mission of the same map. Opens the mission document.
pub fn link(project: &mut Project, id: &str, index: usize) -> Link {
    let Some(map) = maps::merged_view(project, id) else { return Link::NoMission };
    let Some(n) = doc::nebulae(map).get(index) else { return Link::Missing };
    let (name, own) = (doc::nebula_name(n).unwrap_or("").to_string(), doc::nebula_polygon(n));
    let Some(m) = mission_view(project, id) else { return Link::NoMission };
    match mdoc::mission_nebulae(m).iter().position(|r| mdoc::record_name(r) == Some(name.as_str())) {
        Some(record) => {
            let polygon_name = mdoc::record_polygon_name(&mdoc::mission_nebulae(m)[record]).unwrap_or("").to_string();
            let in_sync = mdoc::mission_volume(m, &polygon_name).is_some_and(|v| v == own);
            Link::Linked { record, polygon_name, in_sync }
        }
        None => Link::Missing,
    }
}

// ---- writing ----

/// The mission side of a write: its path, its current value, and whether it can take one more or one fewer record.
struct Mission {
    rel: String,
    value: Value,
}

fn mission_of(project: &mut Project, id: &str) -> Result<Option<Mission>, String> {
    let rel = mission_rel(id)?;
    Ok(mission_view(project, id).cloned().map(|value| Mission { rel, value }))
}

/// The mission can take a record added or removed: it is whole in the mod, or the parent has no nebula records (the list is then
/// all ours), or the list merges by key (it does not today: the engine has no rule for it, so the refusal is the usual one).
fn check_records(project: &Project, mission: &Mission) -> Result<(), String> {
    if is_whole(project, &mission.rel) {
        return Ok(());
    }
    let d = project.doc(&mission.rel).ok_or_else(|| format!("{}: not open", mission.rel))?;
    let ptr = mdoc::mission_nebulae_pointer();
    let base = d.base.pointer(&ptr);
    if base.and_then(Value::as_array).is_none_or(Vec::is_empty) {
        return Ok(());
    }
    keyed::can_add_delete(&keyed::EngineRule, &mission.rel, d.own.get("merge"), &ptr, base).map_err(|e| format!("a nebula is also a record of the mission, and {e}"))
}

/// The mission's record can be changed in place. Where [`check_records`] refuses, a change of one record would restate the retail
/// mission's whole list of them (the list is replaced whole by a diff, so every record of the game would land in the mod), so the
/// change is refused and nothing is written.
fn check_record_edit(project: &Project, mission: &Mission) -> Result<(), String> {
    check_records(project, mission).map_err(|_| {
        "this also changes the mission's record of the nebula, and a file that extends a retail mission can only write that by restating the mission's whole list of nebula records (the list is not merged by key): change the map's own copy of the map or the record by hand".to_string()
    })
}

fn nebula_at(project: &Project, id: &str, index: usize) -> Result<Value, String> {
    let map = maps::merged_view(project, id).ok_or_else(|| format!("map '{id}' is not open"))?;
    doc::nebulae(map).get(index).cloned().ok_or_else(|| format!("no nebula {index} in map '{id}'"))
}

fn run(project: &mut Project, label: &str, map_rel: &str, map_edits: Vec<Edit>, mission: Option<(&Mission, Vec<Edit>)>) -> Result<(), String> {
    let mut steps = vec![(map_rel.to_string(), map_edits)];
    if let Some((m, e)) = mission {
        steps.push((m.rel.clone(), e));
    }
    project.do_group(label, steps)
}

/// Adds the nebula `name` with `polygon` (game x, y points): the map's entry, and when the map has a mission its record and its
/// volume (both named `name`), as ONE undo step. Returns the index of the entry. Refused for a name the map or the mission uses, and
/// on a map that extends another where the entry or the record cannot be added without restating the parent's list.
pub fn create(project: &mut Project, id: &str, name: &str, polygon: &[[f64; 2]]) -> Result<usize, String> {
    check_name(name)?;
    check_polygon(polygon)?;
    let rel = map_rel(id)?;
    maps::open_map(project, id)?;
    let field = check_add_delete(project, &rel, Array::Nebulae)?;
    let map = maps::merged_view(project, id).ok_or("map is not open")?.clone();
    if names(&map).iter().any(|n| n == name) {
        return Err(format!("the map has a nebula '{name}' already"));
    }
    let (c, r) = centre_radius(polygon);
    let entry = doc::new_nebula(name, polygon, [c[0], c[1]], r);
    if let Some(field) = field {
        check_key(&entry, doc::nebulae(&map), &field, Array::Nebulae)?;
    }
    let at = doc::nebulae(&map).len();
    let list = Array::Nebulae.key();
    let map_edit = if map.get(list).is_some_and(Value::is_array) {
        Edit::set(crate::edit::pointer(&[list, &at.to_string()]), entry)
    } else {
        Edit::set(crate::edit::pointer(&[list]), Value::Array(vec![entry]))
    };
    let mission = mission_of(project, id)?;
    let mission_edits = match &mission {
        Some(m) => {
            if mdoc::has_volume(&m.value, name) {
                return Err(format!("the mission has a volume called '{name}' already"));
            }
            if mdoc::mission_nebulae(&m.value).iter().any(|r| mdoc::record_name(r) == Some(name)) {
                return Err(format!("the mission has a nebula record '{name}' already"));
            }
            check_records(project, m)?;
            let record = mdoc::new_record(name, name);
            let n = mdoc::mission_nebulae(&m.value).len();
            let list_edit = if m.value.pointer(&mdoc::mission_nebulae_pointer()).is_some_and(Value::is_array) {
                Edit::set(format!("{}/{n}", mdoc::mission_nebulae_pointer()), record)
            } else {
                Edit::set(mdoc::mission_nebulae_pointer(), Value::Array(vec![record]))
            };
            Some(vec![list_edit, Edit::set(mdoc::volume_pointer(name), doc::polygon_value(polygon))])
        }
        None => None,
    };
    let label = format!("New nebula {name}");
    run(project, &label, &rel, vec![map_edit], mission.as_ref().zip(mission_edits))?;
    Ok(at)
}

/// Replaces the polygon of nebula `index` with `polygon`, and the mission's volume of it, as one step labelled `label`.
pub fn set_polygon(project: &mut Project, id: &str, index: usize, polygon: &[[f64; 2]], label: &str) -> Result<(), String> {
    check_polygon(polygon)?;
    let rel = map_rel(id)?;
    maps::open_map(project, id)?;
    let n = nebula_at(project, id, index)?;
    if doc::nebula_polygon(&n) == polygon {
        return Ok(());
    }
    let map_edits = vec![Edit::set(doc::nebula_pointer(index, &[doc::NEBULA_POLYGON]), doc::polygon_value(polygon))];
    let mission = mission_of(project, id)?;
    let mission_edits = mission.as_ref().and_then(|m| {
        let name = doc::nebula_name(&n)?;
        let rec = mdoc::mission_nebulae(&m.value).iter().find(|r| mdoc::record_name(r) == Some(name))?;
        let pn = mdoc::record_polygon_name(rec).filter(|p| !p.is_empty())?;
        Some(vec![Edit::set(mdoc::volume_pointer(pn), doc::polygon_value(polygon))])
    });
    run(project, label, &rel, map_edits, mission.as_ref().zip(mission_edits))
}

fn vertices(project: &Project, id: &str, index: usize) -> Result<Vec<[f64; 2]>, String> {
    Ok(doc::nebula_polygon(&nebula_at(project, id, index)?))
}

fn title(project: &Project, id: &str, index: usize) -> String {
    nebula_at(project, id, index).ok().and_then(|n| doc::nebula_name(&n).map(str::to_string)).unwrap_or_else(|| format!("#{index}"))
}

/// Moves vertex `vertex` of nebula `index` to `to`: one undo step for the map's polygon and the mission's.
pub fn move_vertex(project: &mut Project, id: &str, index: usize, vertex: usize, to: [f64; 2]) -> Result<(), String> {
    let pts = with_moved(&vertices(project, id, index)?, vertex, to)?;
    let label = format!("Move point {vertex} of nebula {}", title(project, id, index));
    set_polygon(project, id, index, &pts, &label)
}

/// Inserts a point after vertex `after` of nebula `index`.
pub fn insert_vertex(project: &mut Project, id: &str, index: usize, after: usize, at: [f64; 2]) -> Result<(), String> {
    let pts = with_inserted(&vertices(project, id, index)?, after, at)?;
    let label = format!("Add point to nebula {}", title(project, id, index));
    set_polygon(project, id, index, &pts, &label)
}

/// Removes vertex `vertex` of nebula `index` (never below [`MIN_POINTS`]).
pub fn remove_vertex(project: &mut Project, id: &str, index: usize, vertex: usize) -> Result<(), String> {
    let pts = with_removed(&vertices(project, id, index)?, vertex)?;
    let label = format!("Delete point {vertex} of nebula {}", title(project, id, index));
    set_polygon(project, id, index, &pts, &label)
}

/// Sets a switch of nebula `index`. The map's entry gets it; the mission record gets the ones it also has (the cloud effect and the
/// occlusion). Switching on an effect whose point list is empty gives it one point at the polygon's middle, because the engine makes
/// one effect per point and an empty list shows nothing.
pub fn set_flag(project: &mut Project, id: &str, index: usize, flag: NebulaFlag, on: bool) -> Result<(), String> {
    let rel = map_rel(id)?;
    maps::open_map(project, id)?;
    let n = nebula_at(project, id, index)?;
    if flag.get(&n) == on && {
        let [a, b] = flag.path();
        n.get(a).and_then(|o| o.get(b)).is_some()
    } {
        return Ok(());
    }
    let mut map_edits = vec![Edit::set(doc::nebula_pointer(index, &flag.path()), flag.value(on))];
    if on && let Some(key) = flag.points_key().filter(|k| doc::nebula_point_count(&n, k) == 0) {
        let (c, r) = centre_radius(&doc::nebula_polygon(&n));
        map_edits.push(Edit::set(doc::nebula_points_pointer(index, key), Value::Array(vec![doc::effect_point(c, r)])));
    }
    let record = |m: &Mission| -> Option<usize> {
        let name = doc::nebula_name(&n)?;
        mdoc::mission_nebulae(&m.value).iter().position(|r| mdoc::record_name(r) == Some(name))
    };
    let mission = mission_of(project, id)?;
    let mission_edits = mission.as_ref().and_then(|m| {
        let ri = record(m)?;
        if !matches!(flag, NebulaFlag::Cloud | NebulaFlag::Occlusion) {
            return None;
        }
        Some((m, ri))
    });
    if let Some((m, _)) = mission_edits {
        check_record_edit(project, m)?;
    }
    let mission_edits = mission_edits.and_then(|(_, ri)| match flag {
        NebulaFlag::Cloud => Some(vec![Edit::set(mdoc::record_pointer(ri, RecordField::CloudEffect), flag.value(on))]),
        NebulaFlag::Occlusion => Some(vec![Edit::set(mdoc::record_pointer(ri, RecordField::Occlusion), mdoc::flag_text(on))]),
        _ => None,
    });
    let label = format!("Nebula {} {:?} {}", title(project, id, index), flag, if on { "on" } else { "off" });
    run(project, &label, &rel, map_edits, mission.as_ref().zip(mission_edits))
}

/// Sets `sound_distance` of nebula `index`. The engine reads it from the map entry only (`nebula.rs` spawn), so the mission's record
/// (`Ambient sound max distance`) is left alone.
pub fn set_sound_distance(project: &mut Project, id: &str, index: usize, metres: f64) -> Result<(), String> {
    if !metres.is_finite() || metres < 0.0 {
        return Err("a sound distance is zero or more".into());
    }
    let rel = map_rel(id)?;
    maps::open_map(project, id)?;
    let n = nebula_at(project, id, index)?;
    if n.get(doc::NEBULA_SOUND_DISTANCE).is_some() && doc::nebula_sound_distance(&n) == metres {
        return Ok(());
    }
    let value = crate::map_view::short(metres as f32);
    let map_edits = vec![Edit::set(doc::nebula_pointer(index, &[doc::NEBULA_SOUND_DISTANCE]), value)];
    let label = format!("Nebula {} sound distance", title(project, id, index));
    run(project, &label, &rel, map_edits, None)
}

/// Renames nebula `index`: the entry's `name` and the record's `New Nebula Name`, and the volume too (the record's `Polygon Name` and
/// the key in `volumes`) when it carries the old name and nothing else of the mission uses it; otherwise the volume keeps its name.
pub fn rename(project: &mut Project, id: &str, index: usize, new: &str) -> Result<(), String> {
    check_name(new)?;
    let rel = map_rel(id)?;
    maps::open_map(project, id)?;
    let n = nebula_at(project, id, index)?;
    let old = doc::nebula_name(&n).unwrap_or("").to_string();
    if old == new {
        return Ok(());
    }
    let map = maps::merged_view(project, id).ok_or("map is not open")?;
    if names(map).iter().any(|x| x == new) {
        return Err(format!("the map has a nebula '{new}' already"));
    }
    let map_edits = vec![Edit::set(doc::nebula_pointer(index, &[doc::NEBULA_NAME]), Value::from(new))];
    let mission = mission_of(project, id)?;
    let mission_edits = match &mission {
        Some(m) => {
            let list = mdoc::mission_nebulae(&m.value);
            if list.iter().any(|r| mdoc::record_name(r) == Some(new)) {
                return Err(format!("the mission has a nebula record '{new}' already"));
            }
            let found = list.iter().position(|r| mdoc::record_name(r) == Some(old.as_str()));
            if found.is_some() {
                check_record_edit(project, m)?;
            }
            found.map(|ri| {
                let mut e = vec![Edit::set(mdoc::record_pointer(ri, RecordField::Name), Value::from(new))];
                let pn = mdoc::record_polygon_name(&list[ri]).unwrap_or("");
                let movable = pn == old && mdoc::has_volume(&m.value, &old) && !mdoc::has_volume(&m.value, new) && !mdoc::mission_mentions(&m.value, &old, Some(ri));
                if movable && let Some(poly) = mdoc::mission_volume(&m.value, &old) {
                    e.push(Edit::remove(mdoc::volume_pointer(&old)));
                    e.push(Edit::set(mdoc::volume_pointer(new), doc::polygon_value(&poly)));
                    e.push(Edit::set(mdoc::record_pointer(ri, RecordField::PolygonName), Value::from(new)));
                }
                e
            })
        }
        None => None,
    };
    let label = format!("Rename nebula {old} to {new}");
    run(project, &label, &rel, map_edits, mission.as_ref().zip(mission_edits))
}

/// Deletes nebula `index`: the entry, the mission's record of it and, when nothing else of the mission uses it, its volume; one undo
/// step. Returns the entry. Refused where the entry or the record cannot be removed without restating a parent's list.
pub fn delete(project: &mut Project, id: &str, index: usize) -> Result<Value, String> {
    let rel = map_rel(id)?;
    maps::open_map(project, id)?;
    check_add_delete(project, &rel, Array::Nebulae)?;
    let n = nebula_at(project, id, index)?;
    let name = doc::nebula_name(&n).unwrap_or("").to_string();
    let map_edits = vec![Edit::remove(crate::edit::pointer(&[Array::Nebulae.key(), &index.to_string()]))];
    let mission = mission_of(project, id)?;
    let mission_edits = match &mission {
        Some(m) => match mdoc::mission_nebulae(&m.value).iter().position(|r| mdoc::record_name(r) == Some(name.as_str())) {
            Some(ri) => {
                check_records(project, m)?;
                let mut e = vec![Edit::remove(format!("{}/{ri}", mdoc::mission_nebulae_pointer()))];
                if let Some(pn) =
                    mdoc::record_polygon_name(&mdoc::mission_nebulae(&m.value)[ri]).filter(|p| mdoc::has_volume(&m.value, p) && !mdoc::mission_mentions(&m.value, p, Some(ri)))
                {
                    e.push(Edit::remove(mdoc::volume_pointer(pn)));
                }
                Some(e)
            }
            None => None,
        },
        None => None,
    };
    let label = format!("Delete nebula {name}");
    run(project, &label, &rel, map_edits, mission.as_ref().zip(mission_edits))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQ: [[f64; 2]; 4] = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];

    #[test]
    fn centre_and_radius_are_the_mean_and_the_mean_distance() {
        let (c, r) = centre_radius(&SQ);
        assert_eq!(c, [50.0, 50.0]);
        assert!((r - 50.0 * 2f64.sqrt()).abs() < 1e-9);
        assert_eq!(centre_radius(&[]), ([0.0, 0.0], 0.0));
    }

    #[test]
    fn nearest_vertex_and_edge_respect_the_reach() {
        assert_eq!(nearest_vertex(&SQ, [98.0, 3.0], 5.0), Some(1));
        assert_eq!(nearest_vertex(&SQ, [50.0, 50.0], 5.0), None);
        // the middle of the bottom edge: after vertex 0
        assert_eq!(nearest_edge(&SQ, [50.0, 4.0], 5.0), Some((0, [50.0, 0.0])));
        // the closing edge runs from the last point back to the first: after vertex 3
        assert_eq!(nearest_edge(&SQ, [3.0, 50.0], 5.0), Some((3, [0.0, 50.0])));
        assert_eq!(nearest_edge(&SQ, [50.0, 50.0], 5.0), None);
        assert_eq!(nearest_edge(&SQ[..1], [0.0, 0.0], 5.0), None);
    }

    #[test]
    fn containment_is_even_odd() {
        assert!(contains(&SQ, [50.0, 50.0]));
        assert!(!contains(&SQ, [150.0, 50.0]));
        assert!(!contains(&[], [0.0, 0.0]));
    }

    #[test]
    fn vertex_edits_keep_at_least_three_points() {
        let moved = with_moved(&SQ, 2, [120.0, 120.0]).unwrap();
        assert_eq!(moved[2], [120.0, 120.0]);
        assert!(with_moved(&SQ, 9, [0.0, 0.0]).is_err());
        let more = with_inserted(&SQ, 3, [-10.0, 50.0]).unwrap();
        assert_eq!((more.len(), more[4]), (5, [-10.0, 50.0]));
        let fewer = with_removed(&SQ, 0).unwrap();
        assert_eq!(fewer.len(), 3);
        assert!(with_removed(&fewer, 0).is_err(), "three points stay");
        assert!(with_removed(&SQ, 9).is_err());
    }

    #[test]
    fn free_names_skip_the_used_ones() {
        assert_eq!(free_name(&[]), "Nebula1");
        assert_eq!(free_name(&["Nebula1".into(), "Nebula3".into()]), "Nebula2");
    }

    #[test]
    fn polygons_and_names_are_checked() {
        assert!(check_polygon(&SQ).is_ok());
        assert!(check_polygon(&SQ[..2]).is_err());
        assert!(check_polygon(&[[0.0, 0.0], [1.0, f64::NAN], [2.0, 2.0]]).is_err());
        assert!(check_name("  ").is_err() && check_name("Clarke").is_ok());
    }
}
