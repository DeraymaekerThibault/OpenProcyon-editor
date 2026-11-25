//! The plan canvas: an egui widget that draws the top-down map (grid, world boundary, markers, selection) and turns mouse input into
//! view changes, selection changes and a move delta. It never touches a document: the caller applies `committed_delta` (for example
//! with [`super::select::transform_group`] and [`super::select::edits_for_placements`]) and feeds the new positions back as markers.
//!
//! Input: wheel zooms about the cursor; middle or right drag pans; left click picks (shift adds, ctrl/cmd toggles); left drag on a
//! marker moves the selection (the grabbed marker snaps per [`super::snap::SnapSettings`]), left drag on empty space draws a box;
//! Escape cancels a drag in progress.

use super::plan_view::PlanView;
use super::select::{self, Mode, Selection};
use super::snap::{SnapKind, snap_position};
use bevy_egui::egui::{self, Color32, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2, vec2};

/// Click and grab radius, screen pixels.
const PICK_RADIUS: f32 = 10.0;
/// Closest grid lines, screen pixels.
const GRID_MIN_PX: f32 = 40.0;

/// What a marker stands for; sets its shape and colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Scenery,
    Start,
    Ship,
    Nebula,
    Mine,
    Point,
}

/// One thing to draw and pick, in game axes. `index` is the caller's id for it (its place in the object list); the canvas hands it
/// back in the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerIn {
    pub index: usize,
    pub kind: MarkerKind,
    /// Game (x, y).
    pub pos: [f32; 2],
    /// Heading in radians, clockwise from +y (as [`super::orient`]); drawn for starts and ships.
    pub yaw: f32,
    /// A world-unit radius drawn as a circle (nebulae); 0 for none.
    pub radius: f32,
    pub label: String,
}

impl MarkerIn {
    pub fn new(index: usize, kind: MarkerKind, pos: [f32; 2]) -> Self {
        MarkerIn { index, kind, pos, yaw: 0.0, radius: 0.0, label: String::new() }
    }
}

/// What happened this frame.
#[derive(Clone, Debug)]
pub struct CanvasResponse {
    pub response: egui::Response,
    /// The world move of the selection while a drag is under way (already snapped); markers are drawn offset by it.
    pub moving: Option<[f32; 2]>,
    /// The move to apply, once, when the drag was released.
    pub committed: Option<[f32; 2]>,
    /// The selection changed this frame.
    pub selection_changed: bool,
    /// The marker under the pointer.
    pub hovered: Option<usize>,
    /// The world position under the pointer.
    pub cursor_world: Option<[f32; 2]>,
    /// What the grabbed marker snapped to during a move.
    pub snapped: SnapKind,
}

#[derive(Clone, Debug)]
enum Drag {
    /// Moving the selection by the grabbed marker `grab` (its original position `from`), pointer first seen at `start_world`.
    Move {
        grab: usize,
        from: [f32; 2],
        start_world: [f32; 2],
    },
    Box {
        start: Pos2,
    },
}

fn drag_id(id: egui::Id) -> egui::Id {
    id.with("plan-drag")
}

fn colour(kind: MarkerKind) -> Color32 {
    match kind {
        MarkerKind::Scenery => Color32::from_rgb(120, 170, 215),
        MarkerKind::Start => Color32::from_rgb(90, 200, 110),
        MarkerKind::Ship => Color32::from_rgb(240, 200, 80),
        MarkerKind::Nebula => Color32::from_rgb(170, 110, 220),
        MarkerKind::Mine => Color32::from_rgb(230, 90, 80),
        MarkerKind::Point => Color32::from_rgb(220, 220, 220),
    }
}

/// The world move for a drag: the grabbed marker follows the pointer and snaps; everything selected moves by the same delta.
fn move_delta(view: &PlanView, markers: &[MarkerIn], sel: &Selection, from: [f32; 2], start_world: [f32; 2], now_world: [f32; 2]) -> ([f32; 2], SnapKind) {
    let target = [from[0] + now_world[0] - start_world[0], from[1] + now_world[1] - start_world[1]];
    let others: Vec<[f32; 2]> = markers.iter().filter(|m| !sel.contains(m.index)).map(|m| m.pos).collect();
    let s = snap_position(target, &others, &view.snap);
    ([s.pos[0] - from[0], s.pos[1] - from[1]], s.kind)
}

fn modifiers_mode(ui: &egui::Ui) -> Mode {
    let m = ui.input(|i| i.modifiers);
    Mode::from_modifiers(m.shift, m.command)
}

/// Draws the plan in the space left in `ui` and handles its input; see the module doc. Allocates at least 50 x 50 points.
pub fn plan_canvas(ui: &mut egui::Ui, view: &mut PlanView, markers: &[MarkerIn], sel: &mut Selection) -> CanvasResponse {
    let size = ui.available_size().max(vec2(50.0, 50.0));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let did = drag_id(response.id);
    let mut out =
        CanvasResponse { response: response.clone(), moving: None, committed: None, selection_changed: false, hovered: None, cursor_world: None, snapped: SnapKind::None };

    // view: zoom and pan
    let hover = response.hover_pos();
    if let Some(h) = hover {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            view.zoom_at(rect, h, (scroll * 0.0015).exp());
        }
        out.cursor_world = Some(view.to_world(rect, h));
    }
    if response.dragged_by(PointerButton::Middle) || response.dragged_by(PointerButton::Secondary) {
        view.pan_by_screen(response.drag_delta());
    }

    // Ships last: an exact tie goes to the later point ([`select::pick`]), so a ship standing on an objective point is the one clicked.
    let at = |m: &MarkerIn| (m.index, view.to_screen(rect, m.pos));
    let screen: Vec<(usize, Pos2)> = markers.iter().filter(|m| m.kind != MarkerKind::Ship).map(at).chain(markers.iter().filter(|m| m.kind == MarkerKind::Ship).map(at)).collect();
    out.hovered = hover.and_then(|h| select::pick(screen.iter().copied(), h, PICK_RADIUS));
    let pointer = ui.input(|i| i.pointer.latest_pos());

    // selection and moving
    let mode = modifiers_mode(ui);
    if response.drag_started_by(PointerButton::Primary) {
        let press = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(press) = press {
            let hit = select::pick(screen.iter().copied(), press, PICK_RADIUS);
            let drag = match hit.and_then(|h| markers.iter().find(|m| m.index == h)) {
                Some(m) => {
                    if !sel.contains(m.index) {
                        sel.click(Some(m.index), mode);
                        out.selection_changed = true;
                    }
                    Some(Drag::Move { grab: m.index, from: m.pos, start_world: view.to_world(rect, press) })
                }
                None => Some(Drag::Box { start: press }),
            };
            ui.data_mut(|d| d.insert_temp(did, drag));
        }
    }
    let drag: Option<Drag> = ui.data(|d| d.get_temp::<Option<Drag>>(did)).flatten();
    if drag.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.insert_temp::<Option<Drag>>(did, None));
        return out;
    }
    if let (Some(drag), Some(now)) = (&drag, pointer) {
        let primary = response.dragged_by(PointerButton::Primary);
        let released = response.drag_stopped_by(PointerButton::Primary);
        match drag {
            Drag::Move { from, start_world, grab } if primary || released => {
                let _ = grab;
                let (delta, kind) = move_delta(view, markers, sel, *from, *start_world, view.to_world(rect, now));
                out.snapped = kind;
                if released {
                    out.committed = (delta != [0.0, 0.0]).then_some(delta);
                } else {
                    out.moving = Some(delta);
                }
            }
            Drag::Box { start } if primary || released => {
                let r = Rect::from_two_pos(*start, now);
                if released {
                    let hits = select::box_select(screen.iter().copied(), r);
                    sel.apply_box(&hits, mode);
                    out.selection_changed = true;
                } else {
                    paint_box(ui, rect, r);
                }
            }
            _ => {}
        }
        if released {
            ui.data_mut(|d| d.insert_temp::<Option<Drag>>(did, None));
        }
    }
    if response.clicked_by(PointerButton::Primary)
        && let Some(at) = response.interact_pointer_pos()
    {
        sel.click(select::pick(screen.iter().copied(), at, PICK_RADIUS), mode);
        out.selection_changed = true;
    }

    paint(ui, rect, view, markers, sel, out.moving.unwrap_or([0.0, 0.0]));
    out
}

fn paint_box(ui: &egui::Ui, clip: Rect, r: Rect) {
    let p = ui.painter_at(clip);
    p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(120, 170, 255, 30));
    p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgb(120, 170, 255)), StrokeKind::Inside);
}

fn paint(ui: &egui::Ui, rect: Rect, view: &PlanView, markers: &[MarkerIn], sel: &Selection, delta: [f32; 2]) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::from_rgb(12, 16, 32));
    for l in view.grid_lines(rect, GRID_MIN_PX) {
        let axis = l.world == 0.0;
        let c = match (axis, l.major) {
            (true, _) => Color32::from_rgb(70, 80, 110),
            (false, true) => Color32::from_rgb(40, 48, 72),
            _ => Color32::from_rgb(26, 32, 52),
        };
        let stroke = Stroke::new(1.0, c);
        if l.vertical {
            p.line_segment([pos2(l.at, rect.top()), pos2(l.at, rect.bottom())], stroke);
        } else {
            p.line_segment([pos2(rect.left(), l.at), pos2(rect.right(), l.at)], stroke);
        }
    }
    let h = view.world_half;
    let world = Rect::from_two_pos(view.to_screen(rect, [-h, -h]), view.to_screen(rect, [h, h]));
    p.rect_stroke(world, 0.0, Stroke::new(1.5, Color32::from_rgb(200, 90, 90)), StrokeKind::Middle);
    for m in markers {
        let moved = sel.contains(m.index);
        let w = if moved { [m.pos[0] + delta[0], m.pos[1] + delta[1]] } else { m.pos };
        let c = view.to_screen(rect, w);
        if !rect.expand(40.0).contains(c) {
            continue;
        }
        draw_marker(&p, view, m, c);
        if moved {
            p.circle_stroke(c, 9.0, Stroke::new(1.5, Color32::WHITE));
        }
        if !m.label.is_empty() && view.scale > 0.1 {
            p.text(c + vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, &m.label, egui::FontId::monospace(10.0), Color32::GRAY);
        }
    }
}

fn draw_marker(p: &egui::Painter, view: &PlanView, m: &MarkerIn, c: Pos2) {
    let col = colour(m.kind);
    // forward on screen: world (sin yaw, cos yaw), y flipped
    let fwd = vec2(m.yaw.sin(), -m.yaw.cos());
    let side = vec2(-fwd.y, fwd.x);
    match m.kind {
        MarkerKind::Scenery => {
            p.circle_filled(c, 4.0, col);
            // a short tick shows which way the object faces, so a turn can be seen
            p.line_segment([c, c + fwd * 10.0], Stroke::new(1.5, col));
        }
        MarkerKind::Point => {
            let d = 4.0;
            p.add(Shape::convex_polygon(vec![c + vec2(0.0, -d), c + vec2(d, 0.0), c + vec2(0.0, d), c + vec2(-d, 0.0)], col, Stroke::NONE));
        }
        MarkerKind::Mine => {
            let d = 4.0;
            p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], Stroke::new(2.0, col));
            p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], Stroke::new(2.0, col));
        }
        MarkerKind::Nebula => {
            let r = m.radius * view.scale;
            if r > 1.0 {
                p.circle_filled(c, r, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 30));
                p.circle_stroke(c, r, Stroke::new(1.0, col));
            }
            p.circle_filled(c, 3.0, col);
        }
        MarkerKind::Start | MarkerKind::Ship => {
            let (l, w) = if m.kind == MarkerKind::Start { (9.0, 5.0) } else { (8.0, 4.0) };
            let pts = vec![c + fwd * l, c - fwd * l * 0.6 + side * w, c - fwd * l * 0.6 - side * w];
            let stroke = if m.kind == MarkerKind::Start { Stroke::new(1.5, Color32::WHITE) } else { Stroke::NONE };
            p.add(Shape::convex_polygon(pts, col, stroke));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_edit::snap::SnapSettings;

    const SCREEN: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));

    fn view() -> PlanView {
        PlanView { center: [0.0, 0.0], scale: 1.0, snap: SnapSettings::off(), world_half: 150.0 }
    }

    fn markers() -> Vec<MarkerIn> {
        use MarkerKind::*;
        let mut v = vec![
            MarkerIn::new(0, Scenery, [-50.0, 20.0]),
            MarkerIn::new(1, Start, [50.0, 20.0]),
            MarkerIn::new(2, Ship, [0.0, -60.0]),
            MarkerIn::new(3, Nebula, [90.0, -40.0]),
            MarkerIn::new(4, Mine, [-90.0, -40.0]),
            MarkerIn::new(5, Point, [0.0, 100.0]),
        ];
        v[2].yaw = 1.0;
        v[3].radius = 40.0;
        v[0].label = "rock".into();
        v
    }

    /// A headless run of the canvas over a sequence of frames, each with its own events.
    struct Rig {
        ctx: egui::Context,
        view: PlanView,
        markers: Vec<MarkerIn>,
        sel: Selection,
        t: f64,
        last: Option<CanvasResponse>,
        rect: Rect,
    }

    impl Rig {
        fn new(view: PlanView) -> Rig {
            let mut r = Rig { ctx: egui::Context::default(), view, markers: markers(), sel: Selection::new(), t: 0.0, last: None, rect: SCREEN };
            r.frame(vec![], egui::Modifiers::NONE); // lay out once so `rect` is known
            r
        }

        fn frame(&mut self, mut events: Vec<egui::Event>, modifiers: egui::Modifiers) -> &CanvasResponse {
            self.t += 0.05;
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
            let raw = egui::RawInput { screen_rect: Some(SCREEN), time: Some(self.t), events, ..Default::default() };
            let (view, markers, sel) = (&mut self.view, &self.markers, &mut self.sel);
            let mut got = None;
            let mut full = self.ctx.run_ui(raw, |ui| got = Some(plan_canvas(ui, view, markers, sel)));
            full.textures_delta.clear();
            let r = got.expect("the canvas ran");
            self.rect = r.response.rect;
            self.last = Some(r);
            self.last.as_ref().unwrap()
        }

        fn at(&self, w: [f32; 2]) -> Pos2 {
            self.view.to_screen(self.rect, w)
        }
    }

    fn mv(p: Pos2) -> egui::Event {
        egui::Event::PointerMoved(p)
    }

    fn button(p: Pos2, b: PointerButton, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton { pos: p, button: b, pressed, modifiers }
    }

    #[test]
    fn it_draws_every_kind_without_input_and_changes_nothing() {
        let mut rig = Rig::new(view());
        for _ in 0..3 {
            let r = rig.frame(vec![], egui::Modifiers::NONE);
            assert!(r.committed.is_none() && r.moving.is_none() && !r.selection_changed);
        }
        assert!(rig.sel.is_empty());
        // also zoomed far in and far out, and with no markers at all
        for scale in [0.001, 0.05, 50.0, 500.0] {
            rig.view.scale = scale;
            rig.frame(vec![], egui::Modifiers::NONE);
        }
        rig.markers.clear();
        rig.frame(vec![], egui::Modifiers::NONE);
    }

    #[test]
    fn a_click_selects_the_marker_under_it_and_empty_space_clears() {
        let mut rig = Rig::new(view());
        let p = rig.at([50.0, 20.0]);
        rig.frame(vec![mv(p)], egui::Modifiers::NONE);
        rig.frame(vec![button(p, PointerButton::Primary, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        let r = rig.frame(vec![button(p, PointerButton::Primary, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        assert!(r.selection_changed, "{r:?}");
        assert_eq!(rig.sel.iter().collect::<Vec<_>>(), [1]);

        // a click on empty space clears it
        let q = rig.at([-120.0, 100.0]);
        rig.frame(vec![mv(q)], egui::Modifiers::NONE);
        rig.frame(vec![button(q, PointerButton::Primary, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        rig.frame(vec![button(q, PointerButton::Primary, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        assert!(rig.sel.is_empty());
    }

    #[test]
    fn a_drag_on_a_marker_selects_it_and_reports_the_move_on_release() {
        let mut rig = Rig::new(view());
        let from = rig.at([-50.0, 20.0]);
        let to = from + vec2(30.0, -10.0); // 30 right, 10 up in world terms
        rig.frame(vec![mv(from)], egui::Modifiers::NONE);
        rig.frame(vec![button(from, PointerButton::Primary, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        rig.frame(vec![mv(from + vec2(15.0, -5.0))], egui::Modifiers::NONE);
        let r = rig.frame(vec![mv(to)], egui::Modifiers::NONE);
        let live = r.moving.expect("a move in progress");
        assert!((live[0] - 30.0).abs() < 1e-3 && (live[1] - 10.0).abs() < 1e-3, "{live:?}");
        assert!(r.committed.is_none());
        let r = rig.frame(vec![button(to, PointerButton::Primary, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        let done = r.committed.expect("the move is committed once");
        assert!((done[0] - 30.0).abs() < 1e-3 && (done[1] - 10.0).abs() < 1e-3, "{done:?}");
        assert_eq!(rig.sel.iter().collect::<Vec<_>>(), [0]);
        assert!(rig.frame(vec![], egui::Modifiers::NONE).committed.is_none());
        assert_eq!(rig.markers[0].pos, [-50.0, 20.0], "the canvas never edits the markers it was given");
    }

    #[test]
    fn a_move_snaps_the_grabbed_marker_to_the_grid() {
        let mut v = view();
        v.snap = SnapSettings { grid: Some(10.0), angle_deg: None, object_radius: None };
        let mut rig = Rig::new(v);
        let from = rig.at([-50.0, 20.0]);
        let to = from + vec2(33.0, 0.0);
        rig.frame(vec![mv(from)], egui::Modifiers::NONE);
        rig.frame(vec![button(from, PointerButton::Primary, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        rig.frame(vec![mv(from + vec2(10.0, 0.0))], egui::Modifiers::NONE);
        rig.frame(vec![mv(to)], egui::Modifiers::NONE);
        let r = rig.frame(vec![button(to, PointerButton::Primary, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        let d = r.committed.expect("moved");
        assert!((d[0] - 30.0).abs() < 1e-3 && d[1].abs() < 1e-3, "-50 + 33 = -17 snaps to -20, so the delta is 30: {d:?}");
        assert_eq!(r.snapped, SnapKind::Grid);
    }

    #[test]
    fn a_drag_on_empty_space_box_selects_and_shift_adds() {
        let mut rig = Rig::new(view());
        let a = rig.at([-70.0, 40.0]);
        let b = rig.at([70.0, -10.0]);
        let drag = |rig: &mut Rig, m: egui::Modifiers| {
            rig.frame(vec![mv(a)], m);
            rig.frame(vec![button(a, PointerButton::Primary, true, m)], m);
            rig.frame(vec![mv(a + (b - a) * 0.5)], m);
            rig.frame(vec![mv(b)], m);
            rig.frame(vec![button(b, PointerButton::Primary, false, m)], m);
        };
        drag(&mut rig, egui::Modifiers::NONE);
        assert_eq!(rig.sel.iter().collect::<Vec<_>>(), [0, 1], "scenery and start are inside the box");
        // a second box lower down with shift adds to it
        rig.sel.set_single(5);
        drag(&mut rig, egui::Modifiers::SHIFT);
        assert_eq!(rig.sel.iter().collect::<Vec<_>>(), [0, 1, 5]);
    }

    #[test]
    fn a_right_drag_pans_the_view() {
        let mut rig = Rig::new(view());
        let a = rig.rect.center();
        let w = rig.view.to_world(rig.rect, a);
        rig.frame(vec![mv(a)], egui::Modifiers::NONE);
        rig.frame(vec![button(a, PointerButton::Secondary, true, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        rig.frame(vec![mv(a + vec2(20.0, 0.0))], egui::Modifiers::NONE);
        rig.frame(vec![mv(a + vec2(40.0, 10.0))], egui::Modifiers::NONE);
        rig.frame(vec![button(a + vec2(40.0, 10.0), PointerButton::Secondary, false, egui::Modifiers::NONE)], egui::Modifiers::NONE);
        assert!(rig.sel.is_empty());
        let moved = rig.view.to_screen(rig.rect, w);
        assert!(moved.x > a.x + 1.0, "the world followed the pointer: {moved:?}");
    }
}
