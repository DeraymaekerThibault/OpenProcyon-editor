//! The logic behind the Maps tab's plan view (docs/phase4.md P2, P3, work item T5), without any drawing: what the plan shows of a
//! map document, what a drag, a turn, a placement, a duplicate or a delete produces, as plain values the tab hands to
//! [`super::project`] (one undo step each). Names no map key: that is [`super::doc`]. Tested headless.
//!
//! Every marker the canvas draws stands for one entry, a [`Ref`] (array and index); the canvas knows only the marker's number, its
//! place in [`Scene::refs`]. A scene is rebuilt from the document every frame, so undo, redo and an outside change show at once.

use super::catalogue::{Catalogue, SceneryType};
use super::doc::{self, Array};
use super::project::Change;
use crate::map_edit::canvas::{MarkerIn, MarkerKind};
use crate::map_edit::orient;
use crate::map_edit::select::{self, GroupTransform, Placement, Selection};
use crate::map_edit::snap::{SnapSettings, snap_angle, snap_position, wrap_angle};
use bevy_egui::egui::Pos2;
use serde_json::{Value, json};

/// The arrays the plan draws, in the order their markers are numbered.
pub const DRAWN: [Array; 4] = [Array::Objects, Array::Ships, Array::Mines, Array::Players];

/// How far a duplicate lands from its original, world units. PROVISIONAL: a guess.
pub const DUPLICATE_OFFSET: f64 = 30.0;

/// One entry of the document: array and index in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ref {
    pub which: ArrayOrd,
    pub index: usize,
}

/// [`Array`] with an order (the position in [`DRAWN`] and then the rest), so that refs sort.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArrayOrd(u8);

impl ArrayOrd {
    pub fn of(a: Array) -> ArrayOrd {
        ArrayOrd(Array::ALL.iter().position(|&x| x == a).unwrap_or(0) as u8)
    }

    pub fn array(self) -> Array {
        Array::ALL[self.0 as usize]
    }
}

impl Ref {
    pub fn new(which: Array, index: usize) -> Ref {
        Ref { which: ArrayOrd::of(which), index }
    }

    pub fn array(self) -> Array {
        self.which.array()
    }
}

/// What the plan shows: the markers for the canvas and, for each, the entry it stands for (`markers[i].index == i`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub markers: Vec<MarkerIn>,
    pub refs: Vec<Ref>,
}

/// The sizes of the drawn arrays: when this changes (an add, a delete, undo of either) the selection's numbers mean other entries.
pub type Shape = [usize; 4];

pub fn shape(map: &Value) -> Shape {
    DRAWN.map(|a| doc::array_entries(map, a).len())
}

fn position(e: &Value, which: Array) -> Option<[f64; 3]> {
    if which == Array::Players { doc::player_start(e) } else { doc::pos(e) }
}

/// The heading of an entry in radians, clockwise from +y: from its `rot`, or a player's `forward`; 0 when it has neither.
fn heading(e: &Value, which: Array) -> f64 {
    match which {
        Array::Players => doc::player_forward(e).filter(|f| f[0] != 0.0 || f[1] != 0.0).map_or(0.0, |f| f[0].atan2(f[1])),
        _ => doc::rot(e).map_or(0.0, |r| orient::yaw_of(&r)),
    }
}

fn label(e: &Value, which: Array) -> String {
    match which {
        Array::Players => doc::player_name(e).unwrap_or("player").to_string(),
        _ => match (doc::kind(e), doc::id(e)) {
            (Some(k), Some(id)) => format!("{k} #{id}"),
            (Some(k), None) => k.to_string(),
            (None, Some(id)) => format!("#{id}"),
            (None, None) => String::new(),
        },
    }
}

/// The plan of a map document: scenery, ships, mines and players' starts that have a position.
pub fn build_scene(map: &Value) -> Scene {
    let mut scene = Scene::default();
    for which in DRAWN {
        let kind = match which {
            Array::Objects => MarkerKind::Scenery,
            Array::Ships => MarkerKind::Ship,
            Array::Mines => MarkerKind::Mine,
            _ => MarkerKind::Start,
        };
        for (i, e) in doc::array_entries(map, which).iter().enumerate() {
            let Some(p) = position(e, which) else { continue };
            let mut m = MarkerIn::new(scene.refs.len(), kind, [p[0] as f32, p[1] as f32]);
            m.yaw = heading(e, which) as f32;
            m.label = label(e, which);
            scene.markers.push(m);
            scene.refs.push(Ref::new(which, i));
        }
    }
    scene
}

/// The entries behind a selection (numbers past the scene are ignored).
pub fn selected_refs(scene: &Scene, sel: &Selection) -> Vec<Ref> {
    sel.iter().filter_map(|i| scene.refs.get(i).copied()).collect()
}

/// The marker numbers of `refs` in `scene`, for selecting what an add just made.
pub fn markers_of(scene: &Scene, refs: &[Ref]) -> Vec<usize> {
    refs.iter().filter_map(|r| scene.refs.iter().position(|x| x == r)).collect()
}

/// A number as the f32's shortest decimal, widened again (the engine reads f32; this keeps the file free of `-369.23077392578125`).
pub fn short_f64(x: f64) -> f64 {
    crate::map_view::short(x as f32).as_f64().unwrap_or(x)
}

fn entry(map: &Value, r: Ref) -> Option<&Value> {
    doc::array_entries(map, r.array()).get(r.index)
}

/// What a finished drag of the selection by `delta` (world x, y; the canvas has snapped it) writes: the new position of each
/// selected entry, z kept. Entries already there are left out.
pub fn move_changes(map: &Value, scene: &Scene, sel: &Selection, delta: [f32; 2]) -> Vec<Change> {
    selected_refs(scene, sel)
        .into_iter()
        .filter_map(|r| {
            let p = position(entry(map, r)?, r.array())?;
            let to = [short_f64(p[0] + f64::from(delta[0])), short_f64(p[1] + f64::from(delta[1])), p[2]];
            (to != p).then(|| Change { pos: Some(to), ..Change::new(r.array(), r.index) })
        })
        .collect()
}

/// The middle of the selection (bounding box of the positions): where its turn handle sits and what it turns about.
pub fn selection_centre(map: &Value, scene: &Scene, sel: &Selection) -> Option<[f32; 2]> {
    let items = placements(map, &selected_refs(scene, sel));
    select::group_centre(&items).map(|c| [c[0] as f32, c[1] as f32])
}

fn placements(map: &Value, refs: &[Ref]) -> Vec<(usize, Placement)> {
    refs.iter()
        .enumerate()
        .filter_map(|(n, &r)| {
            let e = entry(map, r)?;
            let pos = position(e, r.array())?;
            let rot = doc::rot(e).unwrap_or(orient::IDENTITY);
            Some((n, Placement { pos, rot }))
        })
        .collect()
}

/// What turning the selection clockwise by `yaw` radians about its centre writes: new positions, new `rot` for scenery and ships
/// (the tilt is kept), a rotated `forward` for a start; mines only move round. Empty for a zero turn.
pub fn turn_changes(map: &Value, scene: &Scene, sel: &Selection, yaw: f64) -> Vec<Change> {
    if yaw == 0.0 || !yaw.is_finite() {
        return vec![];
    }
    let refs = selected_refs(scene, sel);
    let before = placements(map, &refs);
    let after = select::transform_group(&before, GroupTransform { delta: [0.0, 0.0], yaw });
    let (s, c) = yaw.sin_cos();
    after
        .into_iter()
        .filter_map(|(n, p)| {
            let r = *refs.get(n)?;
            let e = entry(map, r)?;
            let which = r.array();
            let mut ch = Change::new(which, r.index);
            ch.pos = Some([short_f64(p.pos[0]), short_f64(p.pos[1]), p.pos[2]]);
            match which {
                Array::Objects | Array::Ships => ch.rot = Some(p.rot),
                Array::Players => {
                    // the same clockwise turn as the positions'
                    ch.forward = doc::player_forward(e).map(|f| [f[0] * c + f[1] * s, -f[0] * s + f[1] * c, f[2]]);
                }
                _ => {}
            }
            Some(ch)
        })
        .collect()
}

/// The turn the pointer makes about `centre` (screen) from `from` to `now`, clockwise radians in (-pi, pi], snapped to the angle
/// step when the settings have one. Screen y points down, so "clockwise" is the same on screen and seen from above.
pub fn turn_between(centre: Pos2, from: Pos2, now: Pos2, snap: &SnapSettings) -> f64 {
    let ang = |p: Pos2| (p.x - centre.x).atan2(-(p.y - centre.y));
    let raw = wrap_angle(ang(now) - ang(from));
    f64::from(match snap.angle_deg {
        Some(step) => snap_angle(raw, step),
        None => raw,
    })
}

/// Where a click places something: the cursor, snapped onto another object or to the grid (the same snap a drag has). Not clamped
/// to the world square: the mod checker warns about a position outside it (PROVISIONAL).
pub fn placement_pos(cursor: [f32; 2], scene: &Scene, snap: &SnapSettings) -> [f32; 2] {
    let others: Vec<[f32; 2]> = scene.markers.iter().map(|m| m.pos).collect();
    snap_position(cursor, &others, snap).pos
}

/// The scenery types whose name (or scene) contains `filter`, ignoring case; all of them for an empty filter.
pub fn filtered_types<'a>(cat: &'a Catalogue, filter: &str) -> Vec<&'a SceneryType> {
    let f = filter.trim().to_lowercase();
    cat.scenery.iter().filter(|t| f.is_empty() || t.name.to_lowercase().contains(&f) || t.scenes.iter().any(|s| s.to_lowercase().contains(&f))).collect()
}

/// A new scenery object of `ty` at (x, y): `type`, its usual `scene` when the catalogue knows one, `pos` and an upright `rot`. The
/// id is the project's to give ([`super::project::add_entry`]). PROVISIONAL: z 0 and an upright `rot` are guesses (docked scenery sits elsewhere, see
/// docs/phase4.md P2), and which keys a new object needs is not verified.
pub fn scenery_entry(ty: &SceneryType, at: [f32; 2]) -> Value {
    let mut e = json!({"type": ty.name, "pos": [short_f64(f64::from(at[0])), short_f64(f64::from(at[1])), 0.0], "rot": orient::IDENTITY});
    if let Some(s) = ty.scene() {
        e["scene"] = json!(s);
    }
    e
}

/// A copy of `r`'s entry for [`super::project::add_entry`]: no `id` (it gets the next free one), `pos` moved by `offset`. `None` for
/// what cannot be copied here: starts (a player's slot belongs to the players tab) and entries with no position.
pub fn duplicate_entry(map: &Value, r: Ref, offset: [f64; 2]) -> Option<(Array, Value)> {
    let which = r.array();
    which.layer()?;
    let mut e = entry(map, r)?.clone();
    let p = doc::pos(&e)?;
    let o = e.as_object_mut()?;
    o.remove("id");
    let two = o.get("pos").and_then(Value::as_array).is_some_and(|a| a.len() == 2);
    let (x, y) = (short_f64(p[0] + offset[0]), short_f64(p[1] + offset[1]));
    o.insert("pos".into(), if two { json!([x, y]) } else { json!([x, y, p[2]]) });
    Some((which, e))
}

/// `refs` that can be deleted or copied here (not starts), highest index first within an array so that deleting them one by one
/// leaves the indices of the rest right.
pub fn deletable_in_order(refs: &[Ref]) -> Vec<Ref> {
    let mut v: Vec<Ref> = refs.iter().copied().filter(|r| r.array().layer().is_some()).collect();
    v.sort_by(|a, b| b.cmp(a));
    v.dedup();
    v
}

/// One line about the selection for the tab: what it is, where it is and which way it faces.
pub fn describe(map: &Value, scene: &Scene, sel: &Selection) -> String {
    let refs = selected_refs(scene, sel);
    match refs.as_slice() {
        [] => "Nothing selected.".into(),
        [r] => {
            let (Some(e), which) = (entry(map, *r), r.array()) else { return "Nothing selected.".into() };
            let p = position(e, which).unwrap_or([0.0; 3]);
            let name = label(e, which);
            format!("{} {}: x {:.1}, y {:.1}, z {:.1}, heading {:.0} deg", which.key(), name, p[0], p[1], p[2], heading(e, which).to_degrees())
        }
        many => format!("{} entries selected", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::pos2;

    fn map() -> Value {
        json!({
            "world_half": 1000.0,
            "objects": [
                {"type": "Isle_A", "id": 1, "pos": [0.0, 0.0, 4.0], "scene": "Isle_A", "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1]},
                {"type": "Rock", "id": 2, "pos": [100.0, 0.0]},
                {"type": "Decor"}
            ],
            "ships": [{"type": "Ship_A", "id": 3, "pos": [0.0, 100.0, 0.0]}],
            "mines": [{"type": "Mine", "id": 4, "pos": [-100.0, 0.0, 0.0]}],
            "players": [{"name": "P1", "start": [0.0, -100.0, 0.0], "forward": [0.0, 1.0, 0.0]}, {"name": "NoStart"}]
        })
    }

    fn sel(ids: &[usize]) -> Selection {
        let mut s = Selection::new();
        for &i in ids {
            s.add(i);
        }
        s
    }

    #[test]
    fn the_scene_has_every_entry_with_a_position_and_numbers_them_in_order() {
        let sc = build_scene(&map());
        // objects 0 and 1 (2 has no pos), ship 0, mine 0, player 0
        assert_eq!(sc.refs, vec![Ref::new(Array::Objects, 0), Ref::new(Array::Objects, 1), Ref::new(Array::Ships, 0), Ref::new(Array::Mines, 0), Ref::new(Array::Players, 0)]);
        assert!(sc.markers.iter().enumerate().all(|(i, m)| m.index == i));
        let kinds: Vec<MarkerKind> = sc.markers.iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [MarkerKind::Scenery, MarkerKind::Scenery, MarkerKind::Ship, MarkerKind::Mine, MarkerKind::Start]);
        assert_eq!(sc.markers[1].pos, [100.0, 0.0]);
        assert_eq!(sc.markers[0].label, "Isle_A #1");
        assert_eq!(sc.markers[4].label, "P1");
        assert_eq!(shape(&map()), [3, 1, 1, 2]);
        assert_eq!(build_scene(&json!({})), Scene::default());
    }

    #[test]
    fn headings_come_from_rot_and_from_a_players_forward() {
        let mut m = map();
        m["objects"][0]["rot"] = json!(orient::rotate_yaw(&orient::IDENTITY, 1.0));
        m["players"][0]["forward"] = json!([1.0, 0.0, 0.0]);
        let sc = build_scene(&m);
        assert!((sc.markers[0].yaw - 1.0).abs() < 1e-5);
        assert!((sc.markers[4].yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "forward +x is a quarter turn clockwise from +y");
    }

    #[test]
    fn a_drag_moves_every_selected_entry_by_the_same_delta_and_keeps_z() {
        let m = map();
        let sc = build_scene(&m);
        let ch = move_changes(&m, &sc, &sel(&[0, 1, 4]), [10.5, -20.0]);
        assert_eq!(ch.len(), 3);
        assert_eq!((ch[0].which, ch[0].index, ch[0].pos), (Array::Objects, 0, Some([10.5, -20.0, 4.0])));
        assert_eq!((ch[1].which, ch[1].pos), (Array::Objects, Some([110.5, -20.0, 0.0])));
        assert_eq!((ch[2].which, ch[2].pos), (Array::Players, Some([10.5, -120.0, 0.0])));
        assert!(move_changes(&m, &sc, &sel(&[0]), [0.0, 0.0]).is_empty(), "no move, no change");
        assert!(move_changes(&m, &sc, &sel(&[99]), [5.0, 5.0]).is_empty(), "a stale number is ignored");
        // f32 noise does not reach the file
        let ch = move_changes(&m, &sc, &sel(&[1]), [0.1, 0.2]);
        assert_eq!(ch[0].pos, Some([100.1, 0.2, 0.0]));
    }

    #[test]
    fn a_turn_about_the_centre_moves_positions_and_turns_rot_and_forward() {
        let m = map();
        let sc = build_scene(&m);
        // object 0 at (0,0) and object 1 at (100,0): the centre is (50,0); a half turn swaps them
        let ch = turn_changes(&m, &sc, &sel(&[0, 1]), std::f64::consts::PI);
        let p0 = ch[0].pos.unwrap();
        assert!((p0[0] - 100.0).abs() < 1e-3 && p0[1].abs() < 1e-3 && p0[2] == 4.0, "{p0:?}");
        assert!((orient::yaw_of(&ch[0].rot.unwrap()).abs() - std::f64::consts::PI).abs() < 1e-9);
        assert!(ch[1].rot.is_some(), "a rock with no rot gets one when it turns");
        // a start turns its forward; a mine only moves round
        let ch = turn_changes(&m, &sc, &sel(&[3, 4]), std::f64::consts::FRAC_PI_2);
        assert!(ch[0].rot.is_none() && ch[0].forward.is_none() && ch[0].pos.is_some());
        let f = ch[1].forward.unwrap();
        assert!((f[0] - 1.0).abs() < 1e-9 && f[1].abs() < 1e-9, "forward +y turned clockwise by 90 degrees is +x: {f:?}");
        assert!(turn_changes(&m, &sc, &sel(&[0]), 0.0).is_empty());
        // one entry turns about itself
        let one = turn_changes(&m, &sc, &sel(&[2]), 1.0);
        assert_eq!(one[0].pos, Some([0.0, 100.0, 0.0]));
        assert_eq!(selection_centre(&m, &sc, &sel(&[0, 1])), Some([50.0, 0.0]));
        assert_eq!(selection_centre(&m, &sc, &sel(&[])), None);
    }

    #[test]
    fn the_pointer_turn_is_clockwise_wrapped_and_snapped() {
        let c = pos2(100.0, 100.0);
        let off = SnapSettings::off();
        // from straight up to the right of the centre: a quarter turn clockwise
        let t = turn_between(c, pos2(100.0, 50.0), pos2(150.0, 100.0), &off);
        assert!((t - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        // and back past the seam stays small
        let t = turn_between(c, pos2(99.0, 50.0), pos2(101.0, 50.0), &off);
        assert!(t > 0.0 && t < 0.1, "{t}");
        let t = turn_between(c, pos2(101.0, 50.0), pos2(99.0, 50.0), &off);
        assert!(t < 0.0 && t > -0.1, "{t}");
        let snap = SnapSettings { angle_deg: Some(15.0), ..SnapSettings::off() };
        let t = turn_between(c, pos2(100.0, 50.0), pos2(150.0, 90.0), &snap).to_degrees();
        assert!((t - 75.0).abs() < 1e-3, "{t}");
    }

    #[test]
    fn a_placement_snaps_to_an_object_then_the_grid() {
        let sc = build_scene(&map());
        let snap = SnapSettings { grid: Some(10.0), angle_deg: None, object_radius: Some(12.0) };
        assert_eq!(placement_pos([97.0, 4.0], &sc, &snap), [100.0, 0.0], "onto the rock");
        assert_eq!(placement_pos([43.0, 58.0], &sc, &snap), [40.0, 60.0], "to the grid");
        assert_eq!(placement_pos([43.0, 58.0], &sc, &SnapSettings::off()), [43.0, 58.0]);
    }

    #[test]
    fn the_catalogue_filter_and_the_new_entry() {
        let ty = |n: &str, s: &[&str]| SceneryType { name: n.into(), scenes: s.iter().map(|x| x.to_string()).collect(), ..Default::default() };
        let cat = Catalogue { scenery: vec![ty("Island04", &["Isle_Big"]), ty("Rock", &[]), ty("Island05", &[])], ..Default::default() };
        assert_eq!(filtered_types(&cat, "").len(), 3);
        assert_eq!(filtered_types(&cat, "isl").iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Island04", "Island05"]);
        assert_eq!(filtered_types(&cat, "BIG").len(), 1, "the scene name matches too");
        assert!(filtered_types(&cat, "zzz").is_empty());
        let e = scenery_entry(&cat.scenery[0], [12.5, -30.0]);
        assert_eq!(e["type"], "Island04");
        assert_eq!(e["scene"], "Isle_Big");
        assert_eq!(e["pos"], json!([12.5, -30.0, 0.0]));
        assert_eq!(e["rot"], json!(orient::IDENTITY));
        assert!(e.get("id").is_none());
        assert!(scenery_entry(&cat.scenery[1], [0.0, 0.0]).get("scene").is_none());
    }

    #[test]
    fn a_duplicate_drops_the_id_and_moves_over_and_starts_are_not_copied() {
        let m = map();
        let (which, e) = duplicate_entry(&m, Ref::new(Array::Objects, 0), [30.0, 30.0]).unwrap();
        assert_eq!(which, Array::Objects);
        assert!(e.get("id").is_none());
        assert_eq!((e["type"].clone(), e["pos"].clone(), e["scene"].clone()), (json!("Isle_A"), json!([30.0, 30.0, 4.0]), json!("Isle_A")));
        let (_, rock) = duplicate_entry(&m, Ref::new(Array::Objects, 1), [30.0, 30.0]).unwrap();
        assert_eq!(rock["pos"], json!([130.0, 30.0]), "two numbers stay two");
        assert!(duplicate_entry(&m, Ref::new(Array::Players, 0), [1.0, 1.0]).is_none());
        assert!(duplicate_entry(&m, Ref::new(Array::Objects, 2), [1.0, 1.0]).is_none(), "no position");
        assert!(duplicate_entry(&m, Ref::new(Array::Objects, 9), [1.0, 1.0]).is_none());
    }

    #[test]
    fn deleting_goes_from_the_last_entry_back_and_skips_starts() {
        let refs = [Ref::new(Array::Objects, 0), Ref::new(Array::Objects, 2), Ref::new(Array::Players, 0), Ref::new(Array::Ships, 0), Ref::new(Array::Objects, 2)];
        let order = deletable_in_order(&refs);
        assert_eq!(order, vec![Ref::new(Array::Ships, 0), Ref::new(Array::Objects, 2), Ref::new(Array::Objects, 0)]);
    }

    #[test]
    fn markers_of_finds_the_numbers_of_refs_and_describe_says_what_is_selected() {
        let m = map();
        let sc = build_scene(&m);
        assert_eq!(markers_of(&sc, &[Ref::new(Array::Mines, 0), Ref::new(Array::Objects, 2)]), vec![3]);
        assert_eq!(describe(&m, &sc, &sel(&[])), "Nothing selected.");
        assert_eq!(describe(&m, &sc, &sel(&[0, 1])), "2 entries selected");
        assert!(describe(&m, &sc, &sel(&[1])).starts_with("objects Rock #2: x 100.0, y 0.0"));
    }
}
