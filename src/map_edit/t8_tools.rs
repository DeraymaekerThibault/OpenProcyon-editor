//! The Maps tab's nebula and mine tools (docs/phase4.md P6, work item T8): a toolbar row (pick, draw, form, delete a nebula; arm a
//! mine type) and an overlay on the plan (all nebula polygons; vertex drag, insert and delete on the picked one; click-to-draw; click
//! to place a mine). Drawing and input only: what a click writes is [`crate::map::nebula`] and [`crate::map::mines`], one undo step
//! each, and their refusals (a map that extends a retail one) are shown as they come. The tab hooks this in two calls
//! ([`toolbar`], [`overlay`]); while no tool is active the overlay only paints and takes no input, so the plan behaves as before.

use bevy_egui::egui::{self, Color32, Pos2, Rect, Sense, Stroke, vec2};

use crate::map::doc::{self, NebulaFlag};
use crate::map::mines;
use crate::map::nebula::{self, Link};
use crate::map::project as maps;
use crate::map_edit::plan_view::PlanView;
use crate::map_edit::snap::snap_position;
use crate::project::Project;

/// Screen pixels within which a click grabs a vertex or an edge.
const GRAB_PX: f32 = 9.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Off,
    /// Clicking the points of a new nebula.
    Draw,
    /// Editing the picked nebula's points.
    Edit,
}

#[derive(Default)]
pub struct Tools {
    mode: Mode,
    /// The name typed for the next new nebula, and for a rename.
    name: String,
    rename: String,
    /// Points clicked so far while drawing.
    draft: Vec<[f64; 2]>,
    /// The picked nebula (index in `nebulae`) and its picked vertex.
    picked: Option<usize>,
    vertex: Option<usize>,
    /// A vertex under the pointer, and where it now is.
    dragging: Option<(usize, [f64; 2])>,
    /// The sound distance being edited, and what the document had when it was last read (a change there, such as an undo, resets it).
    sound: f64,
    sound_doc: (Option<usize>, f64),
    mine_type: String,
    /// The mine type armed for the next click.
    mine_armed: Option<String>,
    pub error: Option<String>,
    pub info: String,
    /// The tallest the picked nebula's form has been: its space is kept when it is closed, so the plan does not move.
    form_h: f32,
}

impl Tools {
    /// A tool is active: the overlay takes the pointer.
    pub fn active(&self) -> bool {
        self.mode != Mode::Off || self.mine_armed.is_some()
    }

    /// Starts drawing a nebula called `name` (what the New nebula button does).
    pub fn start_draw(&mut self, name: &str) {
        self.stop();
        self.name = name.to_string();
        self.mode = Mode::Draw;
    }

    /// Arms mine type `ty` for the next click on the plan.
    pub fn arm_mine(&mut self, ty: &str) {
        self.stop();
        self.mine_armed = Some(ty.to_string());
    }

    /// Edits the points of nebula `index`.
    pub fn start_edit(&mut self, index: usize, name: &str) {
        self.stop();
        self.picked = Some(index);
        self.rename = name.to_string();
        self.mode = Mode::Edit;
    }

    fn say<T>(&mut self, r: Result<T, String>, ok: impl Into<String>) -> Option<T> {
        match r {
            Ok(v) => {
                self.error = None;
                self.info = ok.into();
                Some(v)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    fn stop(&mut self) {
        self.mode = Mode::Off;
        self.draft.clear();
        self.vertex = None;
        self.dragging = None;
        self.mine_armed = None;
    }
}

/// The toolbar rows. `id` is the map shown.
pub fn toolbar(ui: &mut egui::Ui, t: &mut Tools, project: &mut Project, id: &str) {
    let Some(map) = maps::merged_view(project, id) else { return };
    let list: Vec<(usize, String)> = nebula::names(map).into_iter().enumerate().collect();
    if t.picked.is_some_and(|i| i >= list.len()) {
        t.picked = None;
        t.vertex = None;
        if t.mode == Mode::Edit {
            t.mode = Mode::Off;
        }
    }
    ui.horizontal_wrapped(|ui| {
        let shown = t.picked.and_then(|i| list.get(i)).map_or("Nebula...".to_string(), |(_, n)| n.clone());
        crate::layout::fixed_width(ui, 150.0, |ui| {
            egui::ComboBox::from_id_salt("t8-nebula-picker").selected_text(shown).width(150.0).truncate().show_ui(ui, |ui| {
                for (i, n) in &list {
                    if ui.selectable_label(t.picked == Some(*i), n).clicked() {
                        t.picked = Some(*i);
                        t.vertex = None;
                        t.mode = Mode::Edit;
                        t.mine_armed = None;
                        t.rename = n.clone();
                    }
                }
                if list.is_empty() {
                    ui.label("No nebulae yet.");
                }
            })
        });
        ui.separator();
        match t.mode {
            Mode::Draw => {
                ui.label(format!("Click the points ({} so far); click the first point or Close to finish", t.draft.len()));
                if ui.add_enabled(t.draft.len() >= nebula::MIN_POINTS, egui::Button::new("Close")).clicked() {
                    finish_draw(t, project, id);
                }
                if ui.button("Cancel").clicked() {
                    t.stop();
                }
            }
            _ => {
                let r = ui.add(egui::TextEdit::singleline(&mut t.name).hint_text("new nebula name").desired_width(110.0));
                if ui.button("New nebula").on_hover_text("Click the polygon's points on the plan").clicked() || crate::layout::entered(ui, &r) {
                    t.stop();
                    t.mode = Mode::Draw;
                    if t.name.trim().is_empty() {
                        t.name = nebula::free_name(&list.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>());
                    }
                }
            }
        }
        ui.separator();
        mine_row(ui, t, project, id);
    });
    // The form comes and goes with the picked nebula; its space stays (once there is a nebula to pick), and the message line is always
    // one line, so that the plan below does not move.
    let open = if t.mode == Mode::Edit { t.picked } else { None };
    // Seeded with the form's usual height (two rows, measured), so that opening it the first time does not push the plan down.
    let mut form_h = if t.form_h > 0.0 { t.form_h } else { FORM_ROWS_H };
    if !list.is_empty() || open.is_some() {
        crate::layout::reserved(ui, &mut form_h, open.is_some(), |ui| {
            if let Some(i) = open {
                form(ui, t, project, id, i);
            }
        });
    }
    t.form_h = form_h;
    let (text, colour) = match (&t.error, t.info.is_empty()) {
        (Some(e), _) => (e.clone(), Some(Color32::LIGHT_RED)),
        (None, false) => (t.info.clone(), None),
        _ => (String::new(), None),
    };
    crate::layout::status_line(ui, &text, colour);
}

/// The height the nebula form takes when first shown (two wrapped rows).
const FORM_ROWS_H: f32 = 37.0;

fn mine_row(ui: &mut egui::Ui, t: &mut Tools, project: &Project, id: &str) {
    let types = mines::types(project, id);
    let shown = t.mine_armed.as_ref().map_or("Place mine...".to_string(), |m| format!("Placing {m} (Esc stops)"));
    let mut arm = None;
    crate::layout::fixed_width(ui, 210.0, |ui| {
        egui::ComboBox::from_id_salt("t8-mine-picker").selected_text(shown).width(210.0).truncate().show_ui(ui, |ui| {
            for ty in &types {
                if ui.selectable_label(false, ty).clicked() {
                    arm = Some(ty.clone());
                }
            }
        })
    });
    ui.add(egui::TextEdit::singleline(&mut t.mine_type).hint_text("or type a bullet").desired_width(120.0));
    if ui.add_enabled(!t.mine_type.trim().is_empty(), egui::Button::new("Arm")).on_hover_text("PROVISIONAL: a typed type is not checked against the game").clicked() {
        arm = Some(t.mine_type.trim().to_string());
    }
    if let Some(ty) = arm {
        t.stop();
        t.info = if mines::is_known(project, id, &ty) {
            format!("Click on the plan to place {ty} (shift keeps it armed).")
        } else {
            format!("{ty} is not a mine bullet the game's table lists (PROVISIONAL): placed as typed. Click on the plan.")
        };
        t.error = None;
        t.mine_armed = Some(ty);
    }
}

/// The form of the picked nebula: name, switches, sound distance, link state, delete.
fn form(ui: &mut egui::Ui, t: &mut Tools, project: &mut Project, id: &str, i: usize) {
    let Some(n) = maps::merged_view(project, id).and_then(|m| doc::nebulae(m).get(i).cloned()) else { return };
    let state = nebula::link(project, id, i);
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::TextEdit::singleline(&mut t.rename).desired_width(110.0));
        if ui.button("Rename").clicked() {
            let r = nebula::rename(project, id, i, t.rename.trim());
            t.say(r, "Renamed.");
        }
        ui.separator();
        for (flag, text) in [
            (NebulaFlag::Cloud, "Cloud"),
            (NebulaFlag::Drain, "Drains"),
            (NebulaFlag::Occlusion, "Hides ships"),
            (NebulaFlag::Storm, "Storm"),
            (NebulaFlag::Lightning, "Lightning"),
            (NebulaFlag::Winds, "Winds"),
            (NebulaFlag::Meteors, "Meteors"),
        ] {
            let mut on = flag.get(&n);
            if ui.checkbox(&mut on, text).changed() {
                let r = nebula::set_flag(project, id, i, flag, on);
                t.say(r, format!("{text} {}.", if on { "on" } else { "off" }));
            }
        }
        ui.separator();
        let in_doc = (Some(i), doc::nebula_sound_distance(&n));
        if t.sound_doc != in_doc {
            t.sound_doc = in_doc;
            t.sound = in_doc.1;
        }
        ui.label("Sound distance");
        let r = ui.add(egui::DragValue::new(&mut t.sound).speed(5.0).range(0.0..=100000.0));
        if r.drag_stopped() || r.lost_focus() {
            let res = nebula::set_sound_distance(project, id, i, t.sound);
            t.say(res, "Sound distance set.");
        }
        ui.separator();
        if ui.button("Delete nebula").on_hover_text("Its mission record and volume go with it").clicked() {
            let r = nebula::delete(project, id, i);
            if t.say(r, "Nebula deleted.").is_some() {
                t.picked = None;
                t.mode = Mode::Off;
            }
        }
        if ui.button("Done").clicked() {
            t.stop();
        }
    });
    ui.label(match state {
        Link::NoMission => "No mission.json for this map: the map's entry only (a skirmish map).".to_string(),
        Link::Linked { polygon_name, in_sync: true, .. } => format!("Linked to the mission: record and volume '{polygon_name}' agree."),
        Link::Linked { polygon_name, .. } => format!("The mission's volume '{polygon_name}' differs from this polygon: the next polygon edit makes them equal."),
        Link::Missing => "The mission has no record of this nebula: only the map's entry is edited.".to_string(),
    });
}

fn finish_draw(t: &mut Tools, project: &mut Project, id: &str) {
    let name = t.name.trim().to_string();
    let r = nebula::create(project, id, &name, &t.draft);
    if let Some(i) = t.say(r, format!("Nebula {name} made.")) {
        t.draft.clear();
        t.picked = Some(i);
        t.rename = name;
        t.name.clear();
        t.mode = Mode::Edit;
    }
}

fn to_screen(view: &PlanView, rect: Rect, p: [f64; 2]) -> Pos2 {
    view.to_screen(rect, [p[0] as f32, p[1] as f32])
}

fn world(view: &PlanView, rect: Rect, p: Pos2) -> [f64; 2] {
    let w = view.to_world(rect, p);
    [f64::from(w[0]), f64::from(w[1])]
}

fn snapped(view: &PlanView, w: [f64; 2]) -> [f64; 2] {
    let s = snap_position([w[0] as f32, w[1] as f32], &[], &view.snap).pos;
    [crate::map::view2d::short_f64(f64::from(s[0])), crate::map::view2d::short_f64(f64::from(s[1]))]
}

/// How a polygon is drawn: its colour, the label at its middle, whether it closes, and whether (and with which vertex picked) it has handles.
struct Style<'a> {
    col: Color32,
    name: &'a str,
    closed: bool,
    handles: Option<Option<usize>>,
}

fn paint_polygon(p: &egui::Painter, view: &PlanView, rect: Rect, pts: &[[f64; 2]], style: Style) {
    let Style { col, name, closed, handles } = style;
    let s: Vec<Pos2> = pts.iter().map(|&q| to_screen(view, rect, q)).collect();
    if closed && s.len() >= 3 {
        p.add(egui::Shape::Path(egui::epaint::PathShape {
            points: s.clone(),
            closed: true,
            fill: Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 36),
            stroke: Stroke::new(1.5, col).into(),
        }));
    } else {
        p.add(egui::Shape::line(s.clone(), Stroke::new(1.5, col)));
    }
    if !name.is_empty() && !s.is_empty() {
        let c = s.iter().fold(vec2(0.0, 0.0), |a, q| a + q.to_vec2()) / s.len() as f32;
        p.text(c.to_pos2(), egui::Align2::CENTER_CENTER, name, egui::FontId::monospace(11.0), col);
    }
    if let Some(picked) = handles {
        for (i, q) in s.iter().enumerate() {
            let on = picked == Some(i);
            p.circle_filled(*q, if on { 6.0 } else { 4.5 }, if on { Color32::WHITE } else { col });
            p.circle_stroke(*q, 5.5, Stroke::new(1.0, Color32::BLACK));
        }
    }
}

/// Paints every nebula polygon of the map over the plan and, while a tool is active, takes the pointer: draw clicks, vertex drag,
/// insert and delete, mine placement. Call after the plan canvas, with the rect it was drawn in.
pub fn overlay(ui: &mut egui::Ui, t: &mut Tools, project: &mut Project, id: &str, view: &PlanView, rect: Rect) {
    let Some(map) = maps::merged_view(project, id) else { return };
    let polys: Vec<(String, Vec<[f64; 2]>)> = doc::nebulae(map).iter().map(|n| (doc::nebula_name(n).unwrap_or("").to_string(), doc::nebula_polygon(n))).collect();
    let painter = ui.painter_at(rect);
    let editing = (t.mode == Mode::Edit).then_some(t.picked).flatten();
    for (i, (name, pts)) in polys.iter().enumerate() {
        if editing == Some(i) {
            continue;
        }
        paint_polygon(&painter, view, rect, pts, Style { col: Color32::from_rgb(170, 110, 220), name, closed: true, handles: None });
    }
    if !t.active() {
        return;
    }
    let resp = ui.interact(rect, ui.id().with("t8-overlay"), Sense::click_and_drag());
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let hover = resp.hover_pos().map(|p| world(view, rect, p));
    let reach = f64::from(GRAB_PX / view.scale.max(1e-6));
    if esc {
        if t.mode == Mode::Edit {
            t.vertex = None;
        } else {
            t.stop();
        }
        t.info = "Stopped.".into();
        return;
    }
    if let Some(ty) = t.mine_armed.clone() {
        if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos()
        {
            let w = snapped(view, world(view, rect, p));
            let r = mines::place(project, id, &ty, [w[0] as f32, w[1] as f32]);
            t.say(r, format!("Placed {ty} at {:.0}, {:.0}.", w[0], w[1]));
            if !ui.input(|i| i.modifiers.shift) {
                t.mine_armed = None;
            }
        }
        return;
    }
    match t.mode {
        Mode::Draw => {
            let col = Color32::from_rgb(255, 210, 120);
            paint_polygon(&painter, view, rect, &t.draft, Style { col, name: "", closed: false, handles: Some(None) });
            if let (Some(h), Some(last)) = (hover, t.draft.last()) {
                painter.line_segment([to_screen(view, rect, *last), to_screen(view, rect, snapped(view, h))], Stroke::new(1.0, col.gamma_multiply(0.6)));
            }
            if resp.clicked_by(egui::PointerButton::Primary)
                && let Some(p) = resp.interact_pointer_pos()
            {
                let w = world(view, rect, p);
                if t.draft.len() >= nebula::MIN_POINTS && nebula::nearest_vertex(&t.draft[..1], w, reach).is_some() {
                    finish_draw(t, project, id);
                } else {
                    t.draft.push(snapped(view, w));
                }
            }
        }
        Mode::Edit => {
            let Some(i) = editing else { return };
            let Some((name, pts)) = polys.get(i) else { return };
            let mut shown = pts.clone();
            if let Some((v, now)) = t.dragging
                && let Some(p) = shown.get_mut(v)
            {
                *p = now;
            }
            paint_polygon(&painter, view, rect, &shown, Style { col: Color32::from_rgb(230, 170, 255), name, closed: true, handles: Some(t.vertex) });
            let press = ui.input(|i| i.pointer.press_origin());
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(v) = press.and_then(|p| nebula::nearest_vertex(pts, world(view, rect, p), reach))
            {
                t.dragging = Some((v, pts[v]));
                t.vertex = Some(v);
            }
            if let Some((v, _)) = t.dragging {
                if let Some(h) = hover {
                    t.dragging = Some((v, snapped(view, h)));
                }
                if resp.drag_stopped() {
                    let to = t.dragging.take().map(|d| d.1).unwrap_or(pts[v]);
                    let r = nebula::move_vertex(project, id, i, v, to);
                    t.say(r, "Point moved.");
                }
            } else if resp.clicked_by(egui::PointerButton::Primary)
                && let Some(p) = resp.interact_pointer_pos()
            {
                let w = world(view, rect, p);
                if let Some(v) = nebula::nearest_vertex(pts, w, reach) {
                    t.vertex = Some(v);
                } else if let Some((after, at)) = nebula::nearest_edge(pts, w, reach) {
                    let r = nebula::insert_vertex(project, id, i, after, snapped(view, at));
                    if t.say(r, "Point added.").is_some() {
                        t.vertex = Some(after + 1);
                    }
                } else {
                    t.vertex = None;
                }
            }
            let del = ui.input(|i| i.key_pressed(egui::Key::Delete));
            if del
                && !ui.ctx().egui_wants_keyboard_input()
                && let Some(v) = t.vertex
            {
                let r = nebula::remove_vertex(project, id, i, v);
                if t.say(r, "Point deleted.").is_some() {
                    t.vertex = None;
                }
            }
        }
        Mode::Off => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_is_active_while_drawing_editing_or_armed() {
        let mut t = Tools::default();
        assert!(!t.active());
        t.mode = Mode::Draw;
        assert!(t.active());
        t.stop();
        assert!(!t.active());
        t.mine_armed = Some("Bullet_StarMine_Med".into());
        assert!(t.active());
        t.stop();
        assert!(!t.active());
    }

    #[test]
    fn say_keeps_the_error_or_the_info() {
        let mut t = Tools::default();
        assert_eq!(t.say::<u8>(Err("no".into()), "ok"), None);
        assert_eq!(t.error.as_deref(), Some("no"));
        assert_eq!(t.say(Ok(3), "done"), Some(3));
        assert!(t.error.is_none() && t.info == "done");
    }
}
