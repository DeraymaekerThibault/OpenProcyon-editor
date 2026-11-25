//! The "Plan" section of the Missions tab (docs/phase5.md P3, work item T7; T9 adds the rule highlighting): the mission's ships and
//! start marker, volumes, paths, points, point sets and map text on a top-down plan, over the mission's map drawn greyed (read only:
//! scenery is edited in the Maps tab). Everything is edited here as one undo step; what is written is [`super::geometry`], what is
//! drawn is built here from the document every frame, so undo, redo and outside changes show at once.
//!
//! [`build_scene`] and [`shapes`] are the pure part (tested headless); [`show`] is the egui part. Its state (which mission, the view,
//! the selection) lives in egui's memory under one id, so the section keeps the signature the tab calls.
//!
//! Plan: left click picks, shift adds, ctrl toggles, a drag on a marker moves the selection (snapped as set in the row above),
//! wheel zooms, middle or right drag pans. "Add ..." arms a tool: the next click on the plan places the new thing there. A volume
//! has a vertex marker per corner (a ring), a path a cross per point, a point set a diamond per dot (the first one is drawn bigger:
//! a teleport puts a group round the first dot, docs/missions.md section 7), a point a diamond, a map text a dot.
//!
//! PROVISIONAL: the colours and sizes; the greyed map shows scenery, ships and mines of `map.json` as dots only.

use std::sync::{Arc, Mutex};

use bevy_egui::egui::{self, Color32, Stroke, pos2, vec2};
use serde_json::Value;

use super::doc::{self as mdoc, GeoTable};
use super::geometry::{self as geo, Item, Kind};
use super::project::{merged_map, merged_mission, open_mission};
use crate::map::doc as mapdoc;
use crate::map::view2d;
use crate::map_edit::canvas::{MarkerIn, MarkerKind, plan_canvas};
use crate::map_edit::plan_view::PlanView;
use crate::map_edit::select::Selection;
use crate::map_edit::snap::snap_position;
use crate::project::Project;

// ---- pure: what the plan shows ----

/// The markers the canvas draws and, for each, the item it stands for (`markers[i].index == i`, `items[i]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub markers: Vec<MarkerIn>,
    pub items: Vec<Item>,
}

impl Scene {
    fn push(&mut self, kind: MarkerKind, pos: [f64; 2], yaw: f64, label: String, item: Item) {
        let mut m = MarkerIn::new(self.items.len(), kind, [pos[0] as f32, pos[1] as f32]);
        m.yaw = yaw as f32;
        m.label = label;
        self.markers.push(m);
        self.items.push(item);
    }

    /// The items of the selected markers.
    pub fn selected(&self, sel: &Selection) -> Vec<Item> {
        sel.iter().filter_map(|i| self.items.get(i).cloned()).collect()
    }
}

/// The plan of a mission document: ships and the start, points, labels, and the vertices of volumes, paths and point sets.
pub fn build_scene(mission: &Value) -> Scene {
    let mut s = Scene::default();
    for (i, ship) in mdoc::ships(mission).iter().enumerate() {
        let Some(p) = mdoc::ship_pos(ship) else { continue };
        let label = mdoc::ship_name(ship).or_else(|| mdoc::ship_type(ship)).unwrap_or("ship").to_string();
        s.push(MarkerKind::Ship, [p[0], p[1]], geo::ship_heading(mission, i), label, Item::Ship(i));
    }
    if let Some(p) = mdoc::player_start_pos(mission) {
        s.push(MarkerKind::Start, [p[0], p[1]], geo::start_heading(mission), mdoc::player_start_name(mission).unwrap_or("start").to_string(), Item::Start);
    }
    for (i, pt) in mdoc::points(mission).iter().enumerate() {
        if let Some(p) = mdoc::point_position(pt) {
            s.push(MarkerKind::Point, [p[0], p[1]], 0.0, mdoc::point_name(pt).unwrap_or("").to_string(), Item::Point(i));
        }
    }
    for (i, t) in mdoc::map_text(mission).iter().enumerate() {
        if let Some(p) = mdoc::map_text_pos(t) {
            let name = mdoc::map_text_name(t).unwrap_or("");
            let label = if mdoc::map_text_visible(t) { mdoc::map_text_text(t).unwrap_or(name).to_string() } else { format!("({name}, hidden)") };
            s.push(MarkerKind::Scenery, p, 0.0, label, Item::Text(i));
        }
    }
    for (table, kind) in [(GeoTable::Volumes, MarkerKind::Nebula), (GeoTable::Paths, MarkerKind::Mine), (GeoTable::PointSets, MarkerKind::Point)] {
        for name in table.get(mission).map_or_else(Vec::new, |o| o.keys().cloned().collect()) {
            for i in 0..mdoc::geo_items(mission, table, &name).len() {
                let it = Item::Vertex(table, name.clone(), i);
                if let Some(p) = geo::item_xyz(mission, &it) {
                    let label = if i == 0 { name.clone() } else { String::new() };
                    s.push(kind, [p[0], p[1]], 0.0, label, it);
                }
            }
        }
    }
    s
}

/// What is drawn between the markers.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A closed polygon (a volume).
    Polygon { name: String, pts: Vec<[f32; 2]> },
    /// A polyline with arrows (a path).
    Path { name: String, pts: Vec<[f32; 2]> },
    /// Numbered dots (a point set); the first is drawn bigger.
    Dots { name: String, pts: Vec<[f32; 2]> },
}

impl Shape {
    pub fn name(&self) -> &str {
        match self {
            Shape::Polygon { name, .. } | Shape::Path { name, .. } | Shape::Dots { name, .. } => name,
        }
    }
}

/// The volumes, paths and point sets of a mission as shapes in file order.
pub fn shapes(mission: &Value) -> Vec<Shape> {
    let mut out = vec![];
    for table in [GeoTable::Volumes, GeoTable::Paths, GeoTable::PointSets] {
        for name in table.get(mission).map_or_else(Vec::new, |o| o.keys().cloned().collect()) {
            let pts: Vec<[f32; 2]> = (0..mdoc::geo_items(mission, table, &name).len())
                .filter_map(|i| geo::item_xyz(mission, &Item::Vertex(table, name.clone(), i)))
                .map(|p| [p[0] as f32, p[1] as f32])
                .collect();
            out.push(match table {
                GeoTable::Volumes => Shape::Polygon { name, pts },
                GeoTable::Paths => Shape::Path { name, pts },
                GeoTable::PointSets => Shape::Dots { name, pts },
            });
        }
    }
    out
}

/// The thing a single selected item belongs to: the volume, path or point set of a vertex, the point or the label of that index.
pub fn owner(mission: &Value, item: &Item) -> Option<(Kind, String)> {
    match item {
        Item::Vertex(t, name, _) => Some((
            match t {
                GeoTable::Volumes => Kind::Volume,
                GeoTable::Paths => Kind::Path,
                GeoTable::PointSets => Kind::PointSet,
            },
            name.clone(),
        )),
        Item::Point(i) => mdoc::points(mission).get(*i).and_then(mdoc::point_name).map(|n| (Kind::Point, n.to_string())),
        Item::Text(i) => mdoc::map_text(mission).get(*i).and_then(mdoc::map_text_name).map(|n| (Kind::MapText, n.to_string())),
        _ => None,
    }
}

// ---- pure: the rules beside the plan (docs/phase5.md P6, T9) ----

/// What a rule names that the plan shows: volumes, paths, points, point sets and map texts by name, and ships by index (a group named
/// by the rule stands for its ships, a `Group,Unit` text for the unit's ship, a player for the ships of that player).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Things {
    pub geo: Vec<(Kind, String)>,
    pub ships: Vec<usize>,
}

impl Things {
    pub fn has_geo(&self, kind: Kind, name: &str) -> bool {
        self.geo.iter().any(|(k, n)| *k == kind && n == name)
    }
}

fn index_after(pointer: &str, list: &str) -> Option<usize> {
    let mut it = pointer.split('/').skip(1);
    if it.next()? == list { it.next()?.parse().ok() } else { None }
}

/// Everything rule `rule` of the mission names that is on the plan (the reference walk, [`super::refs`], so nested `OP Any Of` lists and
/// `Group,Unit` texts count).
pub fn rule_things(mission: &Value, rule: usize) -> Things {
    let mut t = Things::default();
    super::refs::walk(mission, super::refs::FieldIndex::engine(), &mut |v| {
        if index_after(&v.pointer, "rules") != Some(rule) {
            return;
        }
        for (kind, _, name) in super::refs::names_in(v) {
            if let Some(k) = Kind::ALL.into_iter().find(|k| k.ref_kind() == kind) {
                if !t.has_geo(k, name) {
                    t.geo.push((k, name.to_string()));
                }
                continue;
            }
            for (i, s) in mdoc::ships(mission).iter().enumerate() {
                // a group and a player are one namespace (refs.rs): the ships of the player, and the members of the group
                let hit = match kind {
                    "unit" => mdoc::ship_name(s) == Some(name),
                    "group" | "player" => mdoc::ship_group(s) == Some(name) || mdoc::ship_id(s).is_some_and(|id| mdoc::group_members(mission, name).contains(&id)),
                    _ => false,
                };
                if hit && !t.ships.contains(&i) {
                    t.ships.push(i);
                }
            }
        }
    });
    t
}

/// Whether the marker of `item` is something the rule names: a ship of its groups, a point or a map text by name.
fn ruled_by(mission: &Value, item: &Item, things: &Things) -> bool {
    match item {
        Item::Ship(i) => things.ships.contains(i),
        Item::Point(_) | Item::Text(_) => owner(mission, item).is_some_and(|(k, n)| things.has_geo(k, &n)),
        _ => false,
    }
}

/// The "uses this" filter a click on `items` stands for: the thing a vertex, point or label belongs to, a ship by its name and the
/// groups it is in, the start by its player. `None` when nothing clicked names anything.
pub fn uses_of_items(mission: &Value, items: &[Item]) -> Option<super::rules_view::Uses> {
    let mut u = super::rules_view::Uses::default();
    let mut labels: Vec<String> = vec![];
    let add = |u: &mut super::rules_view::Uses, kind: &str, name: &str| {
        let k = (kind.to_string(), name.to_string());
        if !u.keys.contains(&k) {
            u.keys.push(k);
        }
    };
    for it in items {
        match it {
            Item::Ship(i) => {
                let Some(s) = mdoc::ships(mission).get(*i) else { continue };
                if let Some(n) = mdoc::ship_name(s) {
                    add(&mut u, "unit", n);
                    labels.push(format!("ship '{n}'"));
                }
                if let Some(g) = mdoc::ship_group(s) {
                    add(&mut u, "group", g);
                }
                if let (Some(id), Some(groups)) = (mdoc::ship_id(s), mdoc::groups(mission)) {
                    for g in groups.keys().filter(|g| mdoc::group_members(mission, g).contains(&id)) {
                        add(&mut u, "group", g);
                    }
                }
            }
            Item::Start => {
                if let Some(n) = mdoc::player_start_name(mission) {
                    add(&mut u, "player", n);
                    labels.push(format!("player '{n}'"));
                }
            }
            other => {
                if let Some((k, n)) = owner(mission, other) {
                    add(&mut u, k.ref_kind(), &n);
                    labels.push(format!("{} '{n}'", k.noun()));
                }
            }
        }
    }
    labels.dedup();
    if u.keys.is_empty() {
        return None;
    }
    u.label = labels.join(", ");
    Some(u)
}

// ---- egui state ----

struct State {
    mission: Option<String>,
    view: PlanView,
    sel: Selection,
    items: Vec<Item>,
    /// The thing the details show.
    active: Option<(Kind, String)>,
    /// The tool armed by an "Add" button: the next click on the plan places it.
    place: Option<Kind>,
    new_name: String,
    turn_deg: f32,
    rename_buf: String,
    text_buf: String,
    /// What the two buffers were loaded for.
    buf_for: Option<(Kind, String)>,
    fit: bool,
    message: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            mission: None,
            view: PlanView::default(),
            sel: Selection::new(),
            items: vec![],
            active: None,
            place: None,
            new_name: String::new(),
            turn_deg: 15.0,
            rename_buf: String::new(),
            text_buf: String::new(),
            buf_for: None,
            fit: true,
            message: None,
        }
    }
}

fn shared(ui: &egui::Ui) -> Arc<Mutex<State>> {
    let id = egui::Id::new("mission-plan-state");
    ui.data_mut(|d| d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(State::default()))).clone())
}

/// Draws the section: pick a mission, then its plan.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    let shared = shared(ui);
    let mut st = shared.lock().unwrap_or_else(|e| e.into_inner());
    body(ui, &mut st, project);
}

fn note(st: &mut State, r: Result<(), String>) {
    st.message = r.err();
}

fn body(ui: &mut egui::Ui, st: &mut State, project: &mut Project) {
    // The Missions tab has one picker (rules_view); a change of mission resets what is selected here.
    let picked = super::rules_view::selected_mission(ui.ctx());
    if picked != st.mission {
        st.mission = picked;
        st.sel.clear();
        st.active = None;
        st.fit = true;
        st.message = None;
    }
    let Some(id) = st.mission.clone() else {
        ui.label("Choose a mission at the top of the tab to see its plan.");
        return;
    };
    if merged_mission(project, &id).is_none()
        && let Err(e) = open_mission(project, &id)
    {
        ui.label(e);
        return;
    }

    // What is drawn, read once for the frame; the borrow ends before anything is written.
    let rule_sel = super::rules_view::state(ui.ctx()).lock().map_or(0, |r| r.rule);
    let (scene, shapes_now, grey, world_half, dirty, things, ruled_markers) = {
        let Some(m) = merged_mission(project, &id) else { return };
        let map = merged_map(project, &id);
        let grey: Vec<[f32; 2]> = map.map_or_else(Vec::new, |mp| view2d::build_scene(mp).markers.iter().map(|k| k.pos).collect());
        let wh = map.and_then(mapdoc::world_half).unwrap_or(mapdoc::DEFAULT_WORLD_HALF) as f32;
        let dirty = super::project::mission_rel(&id).ok().and_then(|r| project.doc(&r).map(|d| d.dirty)).unwrap_or(false);
        let (scene, things) = (build_scene(m), rule_things(m, rule_sel));
        let ruled: Vec<usize> = scene.items.iter().enumerate().filter(|(_, it)| ruled_by(m, it, &things)).map(|(i, _)| i).collect();
        (scene, shapes(m), grey, wh, dirty, things, ruled)
    };
    if scene.items != st.items {
        st.items = scene.items.clone();
        st.sel.clear();
    }
    st.view.world_half = world_half;

    toolbar(ui, st, project, &id, dirty);
    let selected = scene.selected(&st.sel);
    selection_row(ui, st, project, &id, &selected);
    details(ui, st, project, &id);
    if let Some(m) = &st.message {
        ui.colored_label(Color32::from_rgb(230, 120, 100), m.clone());
    }
    lists(ui, st, project, &id);

    // The plan.
    let width = ui.available_width().max(200.0);
    let resp = ui.allocate_ui(vec2(width, 440.0), |ui| plan_canvas(ui, &mut st.view, &scene.markers, &mut st.sel)).inner;
    let rect = resp.response.rect;
    if st.fit {
        st.view.fit_world(rect, world_half, 20.0);
        st.fit = false;
    }
    paint_overlay(ui, rect, &st.view, &grey, &shapes_now, st.active.as_ref().map(|(_, n)| n.as_str()), &selected, &scene, &things, &ruled_markers);
    if resp.selection_changed {
        follow_selection(st, project, &id, &scene);
        pick_uses(ui.ctx(), project, &id, &scene.selected(&st.sel));
    }
    if let Some(d) = resp.committed {
        let items = scene.selected(&st.sel);
        let r = geo::move_items(project, &id, &items, [f64::from(d[0]), f64::from(d[1])]);
        note(st, r);
    }
    if st.place.is_some() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            st.place = None;
        } else if resp.response.clicked_by(egui::PointerButton::Primary)
            && let (Some(kind), Some(at)) = (st.place, resp.cursor_world)
        {
            let at = snap_position(at, &[], &st.view.snap).pos;
            let name = if st.new_name.trim().is_empty() { merged_mission(project, &id).map_or_else(String::new, |m| geo::free_name(m, kind)) } else { st.new_name.clone() };
            let r = geo::add(project, &id, kind, &name, [f64::from(at[0]), f64::from(at[1])]);
            if r.is_ok() {
                st.active = Some((kind, name));
                st.new_name.clear();
                st.place = None;
            }
            note(st, r);
        }
    }
}

/// A single selected vertex, point or label makes its owner the active thing.
fn follow_selection(st: &mut State, project: &Project, id: &str, scene: &Scene) {
    let items = scene.selected(&st.sel);
    if let ([one], Some(m)) = (items.as_slice(), merged_mission(project, id))
        && let Some(o) = owner(m, one)
    {
        st.active = Some(o);
    }
}

/// A click on the plan filters the rule list to the rules that use what was picked (a click on nothing leaves the filter, its chip
/// in the list clears it).
fn pick_uses(ctx: &egui::Context, project: &Project, id: &str, items: &[Item]) {
    let Some(u) = merged_mission(project, id).and_then(|m| uses_of_items(m, items)) else { return };
    if let Ok(mut r) = super::rules_view::state(ctx).lock() {
        r.uses = Some(u);
    }
}

fn toolbar(ui: &mut egui::Ui, st: &mut State, project: &Project, id: &str, dirty: bool) {
    ui.horizontal_wrapped(|ui| {
        crate::layout::title_with_unsaved(ui, id, dirty);
        ui.separator();
        step(ui, "Grid", &mut st.view.snap.grid, 10.0);
        step(ui, "Angle", &mut st.view.snap.angle_deg, 15.0);
        step(ui, "Objects", &mut st.view.snap.object_radius, 12.0);
        ui.separator();
        if ui.button("Fit").on_hover_text("Show the whole world square").clicked() {
            st.fit = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("New name");
        ui.add(egui::TextEdit::singleline(&mut st.new_name).hint_text("(automatic)").desired_width(110.0));
        for kind in Kind::ALL {
            let on = st.place == Some(kind);
            if ui.selectable_label(on, format!("Add {}", kind.noun())).on_hover_text("Then click the plan where it goes (Esc stops)").clicked() {
                st.place = if on { None } else { Some(kind) };
            }
        }
    });
    if let Some(k) = st.place {
        let m = merged_mission(project, id);
        let shown = if st.new_name.trim().is_empty() { m.map_or_else(String::new, |m| geo::free_name(m, k)) } else { st.new_name.clone() };
        ui.weak(format!("Click the plan to place the {} '{shown}'.", k.noun()));
    }
}

fn step(ui: &mut egui::Ui, label: &str, v: &mut Option<f32>, default: f32) {
    let mut on = v.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *v = on.then_some(default);
    }
    if let Some(x) = v {
        ui.add(egui::DragValue::new(x).speed(0.5).range(1.0..=500.0));
    }
}

fn selection_row(ui: &mut egui::Ui, st: &mut State, project: &mut Project, id: &str, selected: &[Item]) {
    ui.horizontal_wrapped(|ui| {
        ui.label(match selected {
            [] => "Nothing selected".to_string(),
            [one] => describe(one),
            many => format!("{} selected", many.len()),
        });
        ui.separator();
        ui.add(egui::DragValue::new(&mut st.turn_deg).speed(1.0).range(-360.0..=360.0).suffix(" deg"));
        if ui.add_enabled(!selected.is_empty(), egui::Button::new("Turn")).on_hover_text("Turn the selection clockwise about its centre").clicked() {
            let r = geo::turn_items(project, id, selected, f64::from(st.turn_deg).to_radians());
            note(st, r);
        }
        if let [Item::Vertex(t, name, i)] = selected {
            let kind = match t {
                GeoTable::Volumes => Kind::Volume,
                GeoTable::Paths => Kind::Path,
                GeoTable::PointSets => Kind::PointSet,
            };
            if ui.button("Add vertex after").on_hover_text("A new point halfway to the next one").clicked() {
                let r = geo::insert_vertex(project, id, kind, name, *i);
                note(st, r);
            }
            if ui.button("Delete vertex").clicked() {
                let r = geo::remove_vertex(project, id, kind, name, *i);
                note(st, r);
            }
            if *t != GeoTable::Volumes
                && let Some(z) = merged_mission(project, id).and_then(|m| geo::item_xyz(m, &selected[0])).map(|p| p[2])
            {
                let mut z32 = z as f32;
                if ui.add(egui::DragValue::new(&mut z32).speed(1.0).prefix("z ")).changed() {
                    let r = geo::set_z(project, id, *t, name, *i, f64::from(z32));
                    note(st, r);
                }
            }
        }
    });
}

fn describe(item: &Item) -> String {
    match item {
        Item::Ship(i) => format!("Ship #{i}"),
        Item::Start => "The human's start".into(),
        Item::Point(i) => format!("Point #{i}"),
        Item::Text(i) => format!("Map text #{i}"),
        Item::Vertex(t, n, i) => format!(
            "{} '{n}', point {i}",
            match t {
                GeoTable::Volumes => "Volume",
                GeoTable::Paths => "Path",
                GeoTable::PointSets => "Point set",
            }
        ),
    }
}

/// The active thing: rename (rules follow), delete (refused while read), a label's text, what reads it.
fn details(ui: &mut egui::Ui, st: &mut State, project: &mut Project, id: &str) {
    let Some((kind, name)) = st.active.clone() else { return };
    let Some(m) = merged_mission(project, id) else { return };
    if !geo::names(m, kind).contains(&name) {
        st.active = None;
        return;
    }
    if st.buf_for.as_ref() != Some(&(kind, name.clone())) {
        st.buf_for = Some((kind, name.clone()));
        st.rename_buf = name.clone();
        st.text_buf =
            geo::index_of(m, kind, name.as_str()).filter(|_| kind == Kind::MapText).and_then(|i| mdoc::map_text(m).get(i)).and_then(mdoc::map_text_text).unwrap_or("").to_string();
    }
    let used = geo::uses(m, kind, &name);
    let label_state = (kind == Kind::MapText).then(|| geo::index_of(m, kind, &name).and_then(|i| mdoc::map_text(m).get(i).map(|t| (i, mdoc::map_text_visible(t))))).flatten();
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{} '{name}'", kind.noun()));
            ui.weak(match used {
                0 => "not used by any rule".to_string(),
                1 => "used by 1 field".into(),
                n => format!("used by {n} fields"),
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut st.rename_buf).desired_width(160.0));
            let changed = st.rename_buf != name;
            if ui.add_enabled(changed, egui::Button::new("Rename")).on_hover_text("Every rule field that names it follows, in one undo step").clicked() {
                let r = geo::rename(project, id, kind, &name, &st.rename_buf.clone());
                if r.is_ok() {
                    st.active = Some((kind, st.rename_buf.clone()));
                    st.buf_for = None;
                }
                note(st, r);
            }
            if ui.button("Delete").on_hover_text("Refused while a rule, task or nebula reads it").clicked() {
                let r = geo::delete(project, id, kind, &name);
                if r.is_ok() {
                    st.active = None;
                    st.sel.clear();
                }
                note(st, r);
            }
        });
        if let Some((index, visible)) = label_state {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut st.text_buf).desired_width(220.0));
                if ui.button("Set text").clicked() {
                    let r = geo::set_map_text(project, id, index, &st.text_buf.clone());
                    note(st, r);
                }
                let mut v = visible;
                if ui.checkbox(&mut v, "Visible").changed() {
                    let r = geo::set_map_text_visible(project, id, index, v);
                    note(st, r);
                }
            });
        }
    });
}

fn lists(ui: &mut egui::Ui, st: &mut State, project: &Project, id: &str) {
    let Some(m) = merged_mission(project, id) else { return };
    let mut picked = None;
    egui::CollapsingHeader::new("Names").id_salt("plan-names").default_open(false).show(ui, |ui| {
        for kind in Kind::ALL {
            let names = geo::names(m, kind);
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} ({})", kind.plural(), names.len()));
                for n in names {
                    let on = st.active.as_ref() == Some(&(kind, n.clone()));
                    if ui.selectable_label(on, &n).clicked() {
                        let item = match kind {
                            Kind::Point => geo::index_of(m, kind, &n).map(Item::Point),
                            Kind::MapText => geo::index_of(m, kind, &n).map(Item::Text),
                            _ => kind.table().map(|t| Item::Vertex(t, n.clone(), 0)),
                        };
                        if let Some(u) = item.and_then(|it| uses_of_items(m, &[it])) {
                            picked = Some(u);
                        }
                        st.active = Some((kind, n));
                    }
                }
            });
        }
    });
    if let Some(u) = picked
        && let Ok(mut r) = super::rules_view::state(ui.ctx()).lock()
    {
        r.uses = Some(u);
    }
}

// ---- drawing over the canvas ----

#[allow(clippy::too_many_arguments)]
fn paint_overlay(
    ui: &egui::Ui,
    rect: egui::Rect,
    view: &PlanView,
    grey: &[[f32; 2]],
    shapes: &[Shape],
    active: Option<&str>,
    selected: &[Item],
    scene: &Scene,
    things: &Things,
    ruled_markers: &[usize],
) {
    let p = ui.painter_at(rect);
    for w in grey {
        let c = view.to_screen(rect, *w);
        if rect.contains(c) {
            p.circle_filled(c, 2.0, Color32::from_rgba_unmultiplied(150, 150, 150, 70));
        }
    }
    let on = |name: &str| active == Some(name) || selected.iter().any(|i| matches!(i, Item::Vertex(_, n, _) if n == name));
    // what the selected rule names (P6): gold rings on ships and points, gold outlines on volumes, paths and point sets
    let gold = Color32::from_rgb(255, 205, 60);
    for m in ruled_markers.iter().filter_map(|i| scene.markers.get(*i)) {
        p.circle_stroke(view.to_screen(rect, m.pos), 13.0, Stroke::new(2.5, gold));
    }
    for s in shapes {
        let kind = match s {
            Shape::Polygon { .. } => Kind::Volume,
            Shape::Path { .. } => Kind::Path,
            Shape::Dots { .. } => Kind::PointSet,
        };
        let ruled = things.has_geo(kind, s.name());
        let hot = on(s.name()) || ruled;
        match s {
            Shape::Polygon { name, pts } if pts.len() >= 2 => {
                let col = if ruled {
                    gold
                } else if hot {
                    Color32::from_rgb(210, 150, 255)
                } else {
                    Color32::from_rgb(150, 100, 200)
                };
                let sp: Vec<_> = pts.iter().map(|q| view.to_screen(rect, *q)).collect();
                p.add(egui::Shape::closed_line(
                    sp.clone(),
                    Stroke::new(
                        if ruled {
                            3.0
                        } else if hot {
                            2.0
                        } else {
                            1.2
                        },
                        col,
                    ),
                ));
                let c = sp.iter().fold(vec2(0.0, 0.0), |a, q| a + q.to_vec2()) / sp.len() as f32;
                p.text(pos2(c.x, c.y), egui::Align2::CENTER_CENTER, name, egui::FontId::monospace(10.0), col);
            }
            Shape::Path { pts, .. } if pts.len() >= 2 => {
                let col = if ruled {
                    gold
                } else if hot {
                    Color32::from_rgb(255, 150, 120)
                } else {
                    Color32::from_rgb(220, 110, 90)
                };
                let sp: Vec<_> = pts.iter().map(|q| view.to_screen(rect, *q)).collect();
                for w in sp.windows(2) {
                    p.line_segment(
                        [w[0], w[1]],
                        Stroke::new(
                            if ruled {
                                3.0
                            } else if hot {
                                2.0
                            } else {
                                1.2
                            },
                            col,
                        ),
                    );
                    let d = w[1] - w[0];
                    if d.length() > 24.0 {
                        let (mid, dir) = (w[0] + d * 0.5, d.normalized());
                        let side = vec2(-dir.y, dir.x);
                        p.add(egui::Shape::convex_polygon(vec![mid + dir * 6.0, mid - dir * 4.0 + side * 4.0, mid - dir * 4.0 - side * 4.0], col, Stroke::NONE));
                    }
                }
            }
            Shape::Dots { pts, .. } => {
                let col = if ruled {
                    gold
                } else if hot {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(200, 200, 200)
                };
                for (i, q) in pts.iter().enumerate() {
                    let c = view.to_screen(rect, *q);
                    if i == 0 {
                        p.circle_stroke(c, 9.0, Stroke::new(1.5, col));
                    }
                    p.text(c + vec2(8.0, -8.0), egui::Align2::LEFT_BOTTOM, (i + 1).to_string(), egui::FontId::monospace(10.0), col);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m() -> Value {
        json!({
            "ships": [{"id": 1, "name": "One", "pos": [1.0, 2.0, 3.0]}, {"id": 2}],
            "player_start": {"name": "Hero", "start": [5, 5, 0], "forward": [1, 0, 0]},
            "points": [{"Name": "Gate", "Position": [7, 8, 9]}],
            "map_text": [{"name": "L", "text": "Here", "pos": [1, 1], "visible": false}],
            "volumes": {"A": [[0, 0], [4, 0], [4, 4]]},
            "paths": {"P": [[0, 0, 1], [2, 2, 2]]},
            "environment": {"point_sets": {"S": [{"pos": [3, 3, 0]}, {"pos": [4, 4, 0]}]}}
        })
    }

    #[test]
    fn the_scene_has_one_marker_per_placed_thing() {
        let s = build_scene(&m());
        // ship 1 (ship 2 has no position), start, point, text, 3 + 2 + 2 vertices
        assert_eq!(s.items.len(), 1 + 1 + 1 + 1 + 3 + 2 + 2);
        assert_eq!(s.markers.iter().map(|k| k.index).collect::<Vec<_>>(), (0..s.items.len()).collect::<Vec<_>>());
        assert_eq!(s.items[0], Item::Ship(0));
        assert!((s.markers[1].yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(s.markers[3].label.contains("hidden"));
    }

    #[test]
    fn shapes_follow_the_tables() {
        let sh = shapes(&m());
        assert_eq!(sh.len(), 3);
        assert!(matches!(&sh[0], Shape::Polygon { name, pts } if name == "A" && pts.len() == 3));
        assert!(matches!(&sh[2], Shape::Dots { pts, .. } if pts.len() == 2));
    }

    #[test]
    fn a_vertex_belongs_to_its_volume() {
        let v = m();
        assert_eq!(owner(&v, &Item::Vertex(GeoTable::Volumes, "A".into(), 1)), Some((Kind::Volume, "A".into())));
        assert_eq!(owner(&v, &Item::Point(0)), Some((Kind::Point, "Gate".into())));
        assert_eq!(owner(&v, &Item::Ship(0)), None);
    }
}
