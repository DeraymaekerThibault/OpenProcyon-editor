//! The selection model of the plan: which objects (by index in the document's object list) are selected, picking by click and by
//! box, and moving or turning the whole group. Format-independent: positions and rotations are plain numbers keyed by object
//! index; [`edits_for_placements`] turns a result into [`Edit`]s for a JSON pointer prefix the caller supplies.

use super::orient::{self, Rot};
use crate::edit::Edit;
use crate::map_view::short;
use bevy_egui::egui::{Pos2, Rect};
use serde_json::Value;
use std::collections::BTreeSet;

/// How a click or a box combines with the current selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The hit replaces the selection (a click on nothing clears it).
    Replace,
    /// The hit joins the selection (shift).
    Add,
    /// The hit flips in or out of the selection (ctrl/cmd).
    Toggle,
}

impl Mode {
    /// The mode for held modifier keys: ctrl/cmd toggles, shift adds, none replaces.
    pub fn from_modifiers(shift: bool, ctrl_or_cmd: bool) -> Mode {
        if ctrl_or_cmd {
            Mode::Toggle
        } else if shift {
            Mode::Add
        } else {
            Mode::Replace
        }
    }
}

/// The selected object indices, kept sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    ids: BTreeSet<usize>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, i: usize) -> bool {
        self.ids.contains(&i)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The selected indices, ascending.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.ids.iter().copied()
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    pub fn set_single(&mut self, i: usize) {
        self.ids.clear();
        self.ids.insert(i);
    }

    pub fn add(&mut self, i: usize) {
        self.ids.insert(i);
    }

    /// Adds `i`, or removes it when it was selected.
    pub fn toggle(&mut self, i: usize) {
        if !self.ids.remove(&i) {
            self.ids.insert(i);
        }
    }

    /// Applies a click that hit `hit` (`None`: empty space) under `mode`.
    pub fn click(&mut self, hit: Option<usize>, mode: Mode) {
        match (hit, mode) {
            (None, Mode::Replace) => self.clear(),
            (None, _) => {}
            (Some(i), Mode::Replace) => self.set_single(i),
            (Some(i), Mode::Add) => self.add(i),
            (Some(i), Mode::Toggle) => self.toggle(i),
        }
    }

    /// Applies a box that enclosed `hits` under `mode`: replace swaps the selection for them, add unions, toggle flips each.
    pub fn apply_box(&mut self, hits: &[usize], mode: Mode) {
        match mode {
            Mode::Replace => self.ids = hits.iter().copied().collect(),
            Mode::Add => self.ids.extend(hits.iter().copied()),
            Mode::Toggle => hits.iter().for_each(|i| self.toggle(*i)),
        }
    }

    /// Drops indices at or beyond `len` (after the object list shrank).
    pub fn retain_below(&mut self, len: usize) {
        self.ids.retain(|i| *i < len);
    }
}

/// The object nearest to `at` within `radius` screen pixels among `points` (index, screen position); ties go to the later
/// point (it is drawn on top).
pub fn pick(points: impl IntoIterator<Item = (usize, Pos2)>, at: Pos2, radius: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, p) in points {
        let d = p.distance(at);
        if d <= radius && best.is_none_or(|(_, b)| d <= b) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// The indices of `points` inside `rect` (edges included), in the order given.
pub fn box_select(points: impl IntoIterator<Item = (usize, Pos2)>, rect: Rect) -> Vec<usize> {
    points.into_iter().filter(|(_, p)| rect.contains(*p)).map(|(i, _)| i).collect()
}

/// An object's placement in game axes: position and the nine-number `rot`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub pos: [f64; 3],
    pub rot: Rot,
}

/// A move and a turn of a whole group: translate by `delta` (game x, y) after turning clockwise by `yaw` radians about the group's
/// centre (the middle of the positions' bounding box).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroupTransform {
    pub delta: [f64; 2],
    pub yaw: f64,
}

/// The middle of the bounding box of the placements' x, y; `None` for an empty group.
pub fn group_centre(items: &[(usize, Placement)]) -> Option<[f64; 2]> {
    let first = items.first()?.1.pos;
    let (mut lo, mut hi) = ([first[0], first[1]], [first[0], first[1]]);
    for (_, p) in items {
        for k in 0..2 {
            lo[k] = lo[k].min(p.pos[k]);
            hi[k] = hi[k].max(p.pos[k]);
        }
    }
    Some([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0])
}

/// The new placement of every item under `t`: positions turn about the group centre and then move by the delta (z is kept);
/// each `rot` turns by the yaw only, so pitch and roll stay. A zero transform returns the inputs unchanged.
pub fn transform_group(items: &[(usize, Placement)], t: GroupTransform) -> Vec<(usize, Placement)> {
    let Some(c) = group_centre(items) else { return vec![] };
    let (s, co) = t.yaw.sin_cos();
    items
        .iter()
        .map(|(i, p)| {
            let (dx, dy) = (p.pos[0] - c[0], p.pos[1] - c[1]);
            let pos = if t.yaw == 0.0 {
                [p.pos[0] + t.delta[0], p.pos[1] + t.delta[1], p.pos[2]]
            } else {
                // clockwise seen from above, the same sense as `orient::rotate_yaw`
                [c[0] + dx * co + dy * s + t.delta[0], c[1] - dx * s + dy * co + t.delta[1], p.pos[2]]
            };
            let rot = if t.yaw == 0.0 { p.rot } else { orient::rotate_yaw(&p.rot, t.yaw) };
            (*i, Placement { pos, rot })
        })
        .collect()
}

/// The edits that take each object from `before` to `after` (matched by index; objects missing from `before` are skipped):
/// `<prefix>/<index>/pos` and `<prefix>/<index>/rot` as arrays of the f32's shortest decimals, only for what changed. `prefix` is
/// the pointer of the object list (`"/objects"`), without a trailing slash.
pub fn edits_for_placements(prefix: &str, before: &[(usize, Placement)], after: &[(usize, Placement)]) -> Vec<Edit> {
    let mut out = vec![];
    for (i, new) in after {
        let Some((_, old)) = before.iter().find(|(j, _)| j == i) else { continue };
        let num = |v: &[f64]| Value::Array(v.iter().map(|x| short(*x as f32)).collect());
        if new.pos != old.pos {
            out.push(Edit::set(format!("{prefix}/{i}/pos"), num(&new.pos)));
        }
        if new.rot != old.rot {
            out.push(Edit::set(format!("{prefix}/{i}/rot"), num(&new.rot)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply;
    use bevy_egui::egui::pos2;
    use serde_json::json;

    fn pl(x: f64, y: f64) -> Placement {
        Placement { pos: [x, y, 5.0], rot: orient::IDENTITY }
    }

    #[test]
    fn modes_follow_the_modifiers() {
        assert_eq!(Mode::from_modifiers(false, false), Mode::Replace);
        assert_eq!(Mode::from_modifiers(true, false), Mode::Add);
        assert_eq!(Mode::from_modifiers(true, true), Mode::Toggle);
        assert_eq!(Mode::from_modifiers(false, true), Mode::Toggle);
    }

    #[test]
    fn clicks_replace_add_and_toggle() {
        let mut s = Selection::new();
        s.click(Some(3), Mode::Replace);
        s.click(Some(1), Mode::Add);
        assert_eq!(s.iter().collect::<Vec<_>>(), [1, 3]);
        s.click(Some(3), Mode::Toggle);
        assert_eq!(s.iter().collect::<Vec<_>>(), [1]);
        s.click(Some(3), Mode::Toggle);
        assert!(s.contains(3) && s.len() == 2);
        s.click(None, Mode::Add);
        s.click(None, Mode::Toggle);
        assert_eq!(s.len(), 2, "empty space with a modifier keeps the selection");
        s.click(Some(7), Mode::Replace);
        assert_eq!(s.iter().collect::<Vec<_>>(), [7]);
        s.click(None, Mode::Replace);
        assert!(s.is_empty());
    }

    #[test]
    fn boxes_replace_union_and_toggle() {
        let mut s = Selection::new();
        s.set_single(9);
        s.apply_box(&[1, 2], Mode::Add);
        assert_eq!(s.iter().collect::<Vec<_>>(), [1, 2, 9]);
        s.apply_box(&[2, 4], Mode::Toggle);
        assert_eq!(s.iter().collect::<Vec<_>>(), [1, 4, 9]);
        s.apply_box(&[5], Mode::Replace);
        assert_eq!(s.iter().collect::<Vec<_>>(), [5]);
        s.apply_box(&[], Mode::Replace);
        assert!(s.is_empty());
        s.apply_box(&[3, 8], Mode::Add);
        s.retain_below(5);
        assert_eq!(s.iter().collect::<Vec<_>>(), [3]);
    }

    #[test]
    fn pick_takes_the_nearest_within_the_radius() {
        let pts = [(0, pos2(10.0, 10.0)), (1, pos2(14.0, 10.0)), (2, pos2(100.0, 100.0))];
        assert_eq!(pick(pts, pos2(13.0, 10.0), 8.0), Some(1));
        assert_eq!(pick(pts, pos2(11.0, 10.0), 8.0), Some(0));
        assert_eq!(pick(pts, pos2(50.0, 50.0), 8.0), None);
        assert_eq!(pick(pts, pos2(12.0, 10.0), 8.0), Some(1), "an exact tie goes to the later point");
        assert_eq!(pick([], pos2(0.0, 0.0), 8.0), None);
    }

    #[test]
    fn box_select_includes_edges_and_ignores_the_corner_order_of_the_rect() {
        let pts = [(0, pos2(0.0, 0.0)), (1, pos2(10.0, 10.0)), (2, pos2(11.0, 5.0)), (3, pos2(5.0, 5.0))];
        assert_eq!(box_select(pts, Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0))), [0, 1, 3]);
        assert_eq!(box_select(pts, Rect::from_two_pos(pos2(10.0, 10.0), pos2(0.0, 0.0))), [0, 1, 3]);
        assert!(box_select(pts, Rect::from_min_max(pos2(20.0, 20.0), pos2(30.0, 30.0))).is_empty());
    }

    #[test]
    fn group_move_translates_every_position_and_keeps_z_and_rot() {
        let items = [(2, pl(0.0, 0.0)), (5, pl(10.0, 4.0))];
        let out = transform_group(&items, GroupTransform { delta: [3.0, -2.0], yaw: 0.0 });
        assert_eq!(out[0], (2, Placement { pos: [3.0, -2.0, 5.0], rot: orient::IDENTITY }));
        assert_eq!(out[1].1.pos, [13.0, 2.0, 5.0]);
        assert_eq!(transform_group(&items, GroupTransform::default()), items);
        assert!(transform_group(&[], GroupTransform { delta: [1.0, 1.0], yaw: 1.0 }).is_empty());
    }

    #[test]
    fn group_turn_is_clockwise_about_the_bounding_box_centre() {
        let items = [(0, pl(-10.0, 0.0)), (1, pl(10.0, 0.0)), (2, pl(0.0, 10.0))];
        assert_eq!(group_centre(&items), Some([0.0, 5.0]));
        let out = transform_group(&items, GroupTransform { delta: [0.0, 0.0], yaw: std::f64::consts::FRAC_PI_2 });
        // (-10, -5) from the centre turns clockwise to (-5, 10): the west end swings north, the east end south
        assert!((out[0].1.pos[0] + 5.0).abs() < 1e-9 && (out[0].1.pos[1] - 15.0).abs() < 1e-9, "{:?}", out[0]);
        assert!((out[1].1.pos[0] + 5.0).abs() < 1e-9 && (out[1].1.pos[1] + 5.0).abs() < 1e-9, "{:?}", out[1]);
        // each object turned to face east; turning back restores everything
        assert!((orient::yaw_of(&out[0].1.rot) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        let back = transform_group(&out, GroupTransform { delta: [0.0, 0.0], yaw: -std::f64::consts::FRAC_PI_2 });
        for (a, b) in back.iter().zip(&items) {
            for k in 0..3 {
                assert!((a.1.pos[k] - b.1.pos[k]).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn turning_keeps_each_objects_tilt() {
        let tilted = Placement { pos: [0.0, 0.0, 0.0], rot: orient::rot_from_euler(orient::Euler { yaw: 0.2, pitch: 0.3, roll: 0.4 }) };
        let out = transform_group(&[(0, tilted)], GroupTransform { delta: [0.0, 0.0], yaw: 1.0 });
        let e = orient::euler_from_rot(&out[0].1.rot);
        assert!((e.yaw - 1.2).abs() < 1e-9 && (e.pitch - 0.3).abs() < 1e-9 && (e.roll - 0.4).abs() < 1e-9);
    }

    #[test]
    fn edits_cover_only_what_changed_and_apply_to_a_document() {
        let before = [(1, pl(0.0, 0.0)), (2, pl(10.0, 0.0))];
        let after = transform_group(&before, GroupTransform { delta: [5.0, 0.0], yaw: 0.0 });
        let edits = edits_for_placements("/objects", &before, &after);
        assert_eq!(edits.iter().map(|e| e.pointer.as_str()).collect::<Vec<_>>(), ["/objects/1/pos", "/objects/2/pos"]);
        let mut doc = json!({"objects": [{}, {"pos": [0, 0, 5]}, {"pos": [10, 0, 5], "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1]}]});
        for e in &edits {
            apply(&mut doc, e).unwrap();
        }
        assert_eq!(doc["objects"][1]["pos"], json!([5.0, 0.0, 5.0]));
        assert_eq!(doc["objects"][2]["pos"], json!([15.0, 0.0, 5.0]));
        // a turn also writes rot, and an index missing from `before` is skipped
        let turned = transform_group(&before, GroupTransform { delta: [0.0, 0.0], yaw: 0.5 });
        let e = edits_for_placements("/o", &before, &turned);
        assert_eq!(e.len(), 4);
        assert!(edits_for_placements("/o", &[], &turned).is_empty());
        assert!(edits_for_placements("/o", &before, &before).is_empty());
    }
}
