//! The "Maps" tab (docs/phase4.md P2, P3, work item T5): a top-down plan of a map document of the open mod, drawn by
//! [`crate::map_edit::canvas`]. The map is the merged view of the project's document (the game's view of the mod over the import);
//! every change is one undo step of the project and the tab never keeps a copy of the map. Drawing and buttons only: what a drag,
//! a turn, a placement, a duplicate or a delete writes is in [`crate::map::view2d`] and [`crate::map::project`], and is tested there.

use bevy::prelude::Resource;
use bevy_egui::egui::{self, Color32, Pos2, Rect, Sense, vec2};
use std::path::PathBuf;

use crate::map::catalogue::{self, Catalogue};
use crate::map::doc::{self, Array};
use crate::map::players;
use crate::map::project::{self as maps, MapEntry};
use crate::map::settings;
use crate::map::skirmish;
use crate::map::view2d::{self, Ref, Scene, Shape};
use crate::map_edit::canvas::plan_canvas;
use crate::map_edit::plan_view::PlanView;
use crate::map_edit::select::Selection;
use crate::map_edit::snap::SnapSettings;
use crate::project::Project;
use crate::project_panel;

/// Distance of the turn handle above the selection's centre, screen pixels.
const HANDLE_DIST: f32 = 36.0;

#[derive(Resource)]
pub struct State {
    /// The map shown (its id); it is open in the project.
    pub map: Option<String>,
    maps: Vec<MapEntry>,
    maps_stale: bool,
    view: PlanView,
    fit: bool,
    sel: Selection,
    shape: Shape,
    /// What to select once the next scene is built (the entries an add just made).
    select_next: Vec<Ref>,
    new_id: String,
    filter: String,
    /// The scenery type being placed: the next click on empty space adds one.
    place: Option<String>,
    cat: Option<(PathBuf, Catalogue)>,
    turn_deg: f32,
    // T6 hook (map settings): the form's own state.
    settings: settings::State,
    /// The turn under way with the handle, radians.
    turning: Option<f64>,
    error: Option<String>,
    info: String,
    /// Where the canvas was drawn last frame.
    rect: Rect,
    /// [`project_panel::State::opened`] last seen: another mod folder resets the tab.
    seen_opened: u64,
    // T7 players, teams, starts
    players: players::Panel,
    // end T7
    // T8 begin
    t8: crate::map_edit::t8_tools::Tools,
    // T8 end
    // T10
    skirmish: skirmish::Panel,
    // end T10
    // T9: the 3D view's state; the Bevy systems of `view3d::plugin` read and answer through it
    pub v3: crate::map::view3d::View3d,
    // T12: the scenery-points mode of the 3D view (dock points and crew places of a mod's own scenery)
    pub t12: crate::map::docks::State,
}

impl Default for State {
    fn default() -> Self {
        State {
            map: None,
            maps: vec![],
            maps_stale: true,
            view: PlanView::default(),
            fit: true,
            sel: Selection::new(),
            shape: [0; 4],
            select_next: vec![],
            new_id: String::new(),
            filter: String::new(),
            place: None,
            cat: None,
            turn_deg: 15.0,
            settings: settings::State::default(),
            turning: None,
            error: None,
            info: String::new(),
            rect: Rect::NOTHING,
            seen_opened: 0,
            // T7
            players: players::Panel::default(),
            // end T7
            t8: Default::default(), // T8
            // T10
            skirmish: skirmish::Panel::default(),
            // end T10
            v3: Default::default(),  // T9
            t12: Default::default(), // T12
        }
    }
}

impl State {
    fn forget_map(&mut self) {
        self.sel.clear();
        self.select_next.clear();
        self.shape = [0; 4];
        self.fit = true;
        self.place = None;
        self.turning = None;
        // T7
        self.players.reset();
        // end T7
        self.t8 = Default::default(); // T8
        // T10
        self.skirmish.reset();
        // end T10
    }

    /// Shows the map `id` of `project` (opening it over the import): errors go to the tab's message line.
    pub fn show_map(&mut self, project: &mut Project, id: &str) {
        match maps::open_map(project, id) {
            Ok(_) => {
                self.map = Some(id.to_string());
                self.forget_map();
                self.error = None;
                self.info = format!("Opened map {id}.");
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// `--select`: selects entries of the shown map when its scene is next built.
    pub fn select_at_start(&mut self, entries: &[(String, usize)]) {
        self.select_next = entries.iter().filter_map(|(a, i)| Array::ALL.into_iter().find(|x| x.key() == a).map(|x| Ref::new(x, *i))).collect();
    }

    /// The scenery catalogue of the project's overlay, scanned when first asked for (or after `rescan`).
    fn catalogue(&mut self, project: &Project, rescan: bool) -> &Catalogue {
        if rescan || self.cat.as_ref().is_none_or(|(root, _)| *root != project.root) {
            self.cat = Some((project.root.clone(), catalogue::scan(&project.root)));
        }
        &self.cat.as_ref().expect("scanned just now").1
    }
}

/// What the toolbar asked for this frame; done after the canvas, one at a time, against the scene that was drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Duplicate,
    Delete,
    Turn(f64),
}

/// The tab. Without a project there is nothing to edit: the left panel opens or makes a mod.
pub fn tab(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State) {
    if st.seen_opened != ps.opened {
        st.seen_opened = ps.opened;
        *st = State { seen_opened: ps.opened, ..State::default() };
    }
    let Some(project) = ps.project.as_mut() else {
        ui.label("Open or make a mod folder in the left panel to edit its maps.");
        return;
    };
    if body(ui, st, project) {
        ps.save();
        st.maps_stale = true;
        st.cat = None;
    }
}

/// Everything below the "no mod" check. Returns whether Save was asked for (the project panel's state does the saving).
fn body(ui: &mut egui::Ui, st: &mut State, project: &mut Project) -> bool {
    if st.maps_stale {
        st.maps = maps::list_maps(project);
        st.maps_stale = false;
    }
    let mut save = false;
    let mut act = None;
    ui.horizontal_wrapped(|ui| {
        picker(ui, st, project);
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut st.new_id).hint_text("new map id").desired_width(110.0));
        if ui.add_enabled(!st.new_id.trim().is_empty(), egui::Button::new("New map")).clicked() {
            let id = st.new_id.trim().to_string();
            match maps::new_map(project, &id) {
                Ok(_) => {
                    st.maps_stale = true;
                    st.new_id.clear();
                    st.show_map(project, &id);
                }
                Err(e) => st.error = Some(e),
            }
        }
        ui.separator();
        save = ui.add_enabled(!project.dirty_rels().is_empty(), egui::Button::new("Save")).on_hover_text("Ctrl+S: saves every changed file of the mod").clicked();
        let (undo, redo) = (project.undo_label().map(str::to_string), project.redo_label().map(str::to_string));
        if ui.add_enabled(undo.is_some(), egui::Button::new("Undo")).on_hover_text(undo.unwrap_or_default()).clicked() {
            project.undo();
        }
        if ui.add_enabled(redo.is_some(), egui::Button::new("Redo")).on_hover_text(redo.unwrap_or_default()).clicked() {
            project.redo();
        }
    });
    let Some(id) = st.map.clone() else {
        ui.label("Pick a map above, or make a new one.");
        message(ui, st);
        return save;
    };
    // T6 hook: the map's settings, folded away until asked for.
    {
        let skies = st.catalogue(project, false).skies.clone();
        egui::CollapsingHeader::new("Map settings").id_salt(("map-settings", &id)).show(ui, |ui| settings::show(ui, &mut st.settings, project, &id, &skies));
    }
    // end T6 hook
    // T10 begin: the skirmish section (lobby listing, end rules, script and media forms, play in the lobby); it edits project documents,
    // so it is drawn before the map is read for this frame
    egui::CollapsingHeader::new("Skirmish").id_salt(("skirmish", &id)).show(ui, |ui| skirmish::show(ui, &mut st.skirmish, project, &id));
    // T10 end
    // T7
    let t7_refusals = players::refusals(project, &id);
    // end T7
    // T8 begin: drawn before the map is read for this frame, because the toolbar takes the project mutably
    crate::map_edit::t8_tools::toolbar(ui, &mut st.t8, project, &id);
    // T8 end
    crate::map::docks::ensure_open(project, st.t12.on); // T12: opens the data files the scenery-points mode reads, before the map is borrowed
    let Some(map) = maps::merged_view(project, &id) else {
        st.map = None;
        return save;
    };
    // The plan as the document is now: undo, redo and outside changes show at once.
    let scene = view2d::build_scene(map);
    let now = view2d::shape(map);
    if now != st.shape {
        st.shape = now;
        st.sel.clear();
    }
    if !st.select_next.is_empty() {
        st.sel.clear();
        for i in view2d::markers_of(&scene, &std::mem::take(&mut st.select_next)) {
            st.sel.add(i);
        }
    }
    st.view.world_half = doc::world_half(map).unwrap_or(doc::DEFAULT_WORLD_HALF) as f32;
    let selection_text = view2d::describe(map, &scene, &st.sel);
    let centre = view2d::selection_centre(map, &scene, &st.sel);
    let file_dirty = maps::map_rel(&id).ok().and_then(|rel| project.doc(&rel).map(|d| d.dirty)).unwrap_or(false);

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{id}{}", if file_dirty { " (unsaved)" } else { "" }));
        ui.separator();
        snap_row(ui, &mut st.view.snap);
        ui.separator();
        if ui.button("Fit").on_hover_text("Show the whole world square").clicked() {
            st.fit = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        placer(ui, st, project);
        ui.separator();
        let any = !st.sel.is_empty();
        if ui.add_enabled(any, egui::Button::new("Duplicate")).clicked() {
            act = Some(Act::Duplicate);
        }
        if ui.add_enabled(any, egui::Button::new("Delete")).on_hover_text("Delete key. Starts are edited with the players, not here").clicked() {
            act = Some(Act::Delete);
        }
        ui.separator();
        ui.add(egui::DragValue::new(&mut st.turn_deg).speed(1.0).range(-360.0..=360.0).suffix(" deg"));
        if ui.add_enabled(any, egui::Button::new("Turn")).on_hover_text("Turn the selection clockwise about its centre (or drag the handle above it)").clicked() {
            act = Some(Act::Turn(f64::from(st.turn_deg).to_radians()));
        }
    });
    ui.add(egui::Label::new(selection_text.as_str()).truncate()).on_hover_text(selection_text.as_str());
    message(ui, st);
    // T7: the players and teams section; what it asks for is done below with the other changes
    st.players.follow_plan(&view2d::selected_refs(&scene, &st.sel));
    if let Some(r) = st.players.take_plan_select() {
        st.select_next = vec![r];
    }
    let mut t7_action = None;
    egui::CollapsingHeader::new("Players and teams").default_open(false).show(ui, |ui| t7_action = players::show(ui, &mut st.players, map, &t7_refusals));
    if st.players.armed.is_some() {
        st.place = None;
    }
    // end T7

    // T9 begin: the 2D plan or the 3D view. The 3D branch does what the plan's does below (the buttons' actions, the players' action)
    // against the same selection and returns; everything after this block is the plan.
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut st.v3.on, false, "2D plan");
        ui.selectable_value(&mut st.v3.on, true, "3D view");
        if st.v3.on {
            crate::map::view3d::toolbar(ui, &mut st.v3);
            crate::map::docks::toolbar(ui, &mut st.t12); // T12
        }
    });
    if st.v3.on {
        st.v3.feed(&project.root, &id, map, &scene, &st.sel, (st.view.snap.grid, st.view.world_half));
        // T12 begin: the scenery-points panel above the canvas; what it asks the project to write is done below with the other changes
        crate::map::docks::feed(&mut st.t12, project, map, &scene, &mut st.v3);
        crate::map::docks::show(ui, &mut st.t12);
        // T12 end
        let response = crate::map::view3d::canvas(ui, &mut st.v3);
        if let Some(c) = st.v3.click.take() {
            st.sel.retain_below(scene.markers.len());
            st.sel.click(c.hit.filter(|h| *h < scene.markers.len()), crate::map_edit::select::Mode::from_modifiers(c.shift, c.ctrl));
        }
        let (typing, delete_key) = (ui.ctx().egui_wants_keyboard_input(), ui.input(|i| i.key_pressed(egui::Key::Delete)));
        if delete_key && !typing && response.hovered() {
            act = Some(Act::Delete);
        }
        if let Some(dz) = st.v3.height.take() {
            let changes = crate::map::view3d::height_changes(map, &scene, &st.sel, dz);
            let label = change_label("Set height of", &scene, &st.sel);
            done(st, maps::change_entries(project, &id, &label, &changes), "");
        } else if st.t12.action.is_some() {
            // T12: a point dragged, added or deleted: one undo step of data/docks.json or data/crew_nodes.json
            match crate::map::docks::apply(project, &mut st.t12) {
                Some(Ok(label)) => {
                    st.error = None;
                    st.info = label;
                }
                Some(Err(e)) => st.error = Some(e),
                None => {}
            }
        } else if let Some(a) = t7_action.take() {
            if let Some(r) = players::apply(&mut st.players, project, &id, a) {
                st.select_next = vec![r];
            }
        } else if let Some(Act::Turn(yaw)) = act {
            let changes = view2d::turn_changes(map, &scene, &st.sel, yaw);
            let label = change_label("Turn", &scene, &st.sel);
            done(st, maps::change_entries(project, &id, &label, &changes), "");
        } else if act == Some(Act::Delete) {
            delete_selected(st, project, &id, &scene);
        } else if act == Some(Act::Duplicate) {
            duplicate_selected(st, project, &id, &scene);
        }
        return save;
    }
    // T9 end

    if std::mem::take(&mut st.fit) {
        st.view.fit_world(ui.available_rect_before_wrap(), st.view.world_half, 24.0);
    }
    let out = plan_canvas(ui, &mut st.view, &scene.markers, &mut st.sel);
    let rect = out.response.rect;
    st.rect = rect;
    if let (Some(h), Some(m)) = (out.hovered, scene.markers.iter().find(|m| Some(m.index) == out.hovered)) {
        let _ = h;
        if !m.label.is_empty() {
            out.response.clone().on_hover_text(&m.label);
        }
    }

    // The turn handle: above the selection's centre; a drag turns the selection, one step on release.
    let mut turn = None;
    if let Some(c) = centre.filter(|_| out.moving.is_none()) {
        turn = turn_handle(ui, st, rect, c);
    }
    let (typing, hovering) = (ui.ctx().egui_wants_keyboard_input(), out.response.hovered());
    let (delete_key, escape) = ui.input(|i| (i.key_pressed(egui::Key::Delete), i.key_pressed(egui::Key::Escape)));
    if delete_key && !typing && hovering {
        act = Some(Act::Delete);
    }
    if escape && st.place.take().is_some() {
        st.info = "Placing stopped.".into();
    }
    // T7: Esc stops a start being placed; the formation preview is drawn over the plan
    if escape && st.players.armed.take().is_some() {
        st.players.info = "Placing stopped.".into();
    }
    players::draw_formations(&ui.painter_at(rect), &st.view, rect, map, &st.players);
    // end T7

    // At most one change per frame, against the scene that was drawn.
    // T8: the toolbar above took the project mutably, so the map is read again here (only when a change happens)
    let map_now = || maps::merged_view(project, &id).cloned().unwrap_or_default();
    if let Some(delta) = out.committed {
        let changes = view2d::move_changes(&map_now(), &scene, &st.sel, delta);
        let label = change_label("Move", &scene, &st.sel);
        done(st, maps::change_entries(project, &id, &label, &changes), "");
    } else if let Some(yaw) = turn {
        let changes = view2d::turn_changes(&map_now(), &scene, &st.sel, yaw);
        let label = change_label("Turn", &scene, &st.sel);
        done(st, maps::change_entries(project, &id, &label, &changes), "");
    } else if let Some(a) = t7_action.take() {
        // T7
        if let Some(r) = players::apply(&mut st.players, project, &id, a) {
            st.select_next = vec![r];
        }
        // end T7
    } else if let Some(Act::Turn(yaw)) = act {
        let changes = view2d::turn_changes(&map_now(), &scene, &st.sel, yaw);
        let label = change_label("Turn", &scene, &st.sel);
        done(st, maps::change_entries(project, &id, &label, &changes), "");
    } else if act == Some(Act::Delete) {
        delete_selected(st, project, &id, &scene);
    } else if act == Some(Act::Duplicate) {
        duplicate_selected(st, project, &id, &scene);
    } else if out.response.clicked_by(egui::PointerButton::Primary)
        && out.hovered.is_none()
        && st.players.armed.is_some()
        && let Some(at) = out.cursor_world
    {
        // T7: a click on empty plan adds a player's start (or moves the one being placed)
        let at = view2d::placement_pos(at, &scene, &st.view.snap);
        if let Some(r) = players::place_click(&mut st.players, project, &id, at) {
            st.select_next = vec![r];
        }
        // end T7
    } else if out.response.clicked_by(egui::PointerButton::Primary)
        && out.hovered.is_none()
        && let (Some(ty), Some(at)) = (st.place.clone(), out.cursor_world)
    {
        place_at(st, project, &id, &scene, &ty, at, ui.input(|i| i.modifiers.shift));
    }
    // T8 begin: nebula polygons and the nebula and mine tools, on top of the plan (they take the pointer only while a tool is on)
    crate::map_edit::t8_tools::overlay(ui, &mut st.t8, project, &id, &st.view, rect);
    // T8 end
    save
}

/// The status line (error or info). It always takes one line of height, empty or not, and is cut with "..." when long (the whole text
/// on hover), so that the plan or the 3D view below does not jump when a message comes or goes.
fn message(ui: &mut egui::Ui, st: &State) {
    let (text, colour) = match (&st.error, st.info.is_empty()) {
        (Some(e), _) => (e.as_str(), Some(Color32::LIGHT_RED)),
        (None, false) => (st.info.as_str(), None),
        _ => ("", None),
    };
    let height = ui.text_style_height(&egui::TextStyle::Body);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let mut rich = egui::RichText::new(text);
        if let Some(c) = colour {
            rich = rich.color(c);
        }
        ui.add(egui::Label::new(rich).truncate()).on_hover_text(text);
    });
}

/// The result of a change: the error line, or the info line with `ok`.
fn done<T>(st: &mut State, r: Result<T, String>, ok: &str) {
    match r {
        Ok(_) => {
            st.error = None;
            st.info = ok.to_string();
        }
        Err(e) => st.error = Some(e),
    }
}

fn change_label(verb: &str, scene: &Scene, sel: &Selection) -> String {
    let refs = view2d::selected_refs(scene, sel);
    match refs.as_slice() {
        [r] => format!("{verb} {} {}", r.array().key(), r.index),
        many => format!("{verb} {} entries", many.len()),
    }
}

fn picker(ui: &mut egui::Ui, st: &mut State, project: &mut Project) {
    let shown = match &st.map {
        Some(id) => st.maps.iter().find(|m| &m.id == id).map_or(id.clone(), map_text),
        None => "Map...".into(),
    };
    let mut pick = None;
    egui::ComboBox::from_id_salt("map-picker").selected_text(shown).width(220.0).show_ui(ui, |ui| {
        for m in &st.maps {
            if ui.selectable_label(st.map.as_deref() == Some(m.id.as_str()), map_text(m)).clicked() {
                pick = Some(m.id.clone());
            }
        }
        if st.maps.is_empty() {
            ui.label("No maps in the mod or the import yet.");
        }
    });
    if ui.small_button("Refresh").on_hover_text("List the maps again").clicked() {
        st.maps_stale = true;
    }
    if let Some(id) = pick {
        st.show_map(project, &id);
    }
}

fn map_text(m: &MapEntry) -> String {
    let origin = match (m.in_mod, m.in_import) {
        (true, true) => "retail, changed by the mod",
        (true, false) => "mod",
        _ => "retail",
    };
    match &m.name {
        Some(n) if n != &m.id => format!("{} - {n} ({origin})", m.id),
        _ => format!("{} ({origin})", m.id),
    }
}

/// An optional snap: a checkbox and, when on, its step.
fn opt_step(ui: &mut egui::Ui, label: &str, v: &mut Option<f32>, default: f32, range: std::ops::RangeInclusive<f32>) {
    let mut on = v.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *v = on.then_some(default);
    }
    if let Some(x) = v {
        ui.add(egui::DragValue::new(x).range(range).speed(0.5));
    }
}

fn snap_row(ui: &mut egui::Ui, snap: &mut SnapSettings) {
    opt_step(ui, "Grid", &mut snap.grid, 10.0, 1.0..=500.0);
    opt_step(ui, "Angle", &mut snap.angle_deg, 15.0, 1.0..=90.0);
    opt_step(ui, "Objects", &mut snap.object_radius, 12.0, 1.0..=200.0);
}

/// The "Place" chooser: scenery types of the catalogue with a filter. Choosing one arms the plan: the next click on empty space
/// adds an object (shift keeps it armed).
fn placer(ui: &mut egui::Ui, st: &mut State, project: &Project) {
    let text = match &st.place {
        Some(t) => format!("Placing {t} (Esc stops)"),
        None => "Place scenery...".into(),
    };
    let mut chosen = None;
    let mut rescan = false;
    let filter = &mut st.filter;
    // The catalogue is scanned when the list is first opened; the closure cannot borrow `st` twice, so it is taken out for it.
    let cat_now = st.cat.take();
    let mut cat = cat_now.filter(|(root, _)| *root == project.root);
    // The filter box and "Rescan" are inside the popup: the default `CloseOnClick` would close it on a click in the box. A pick closes
    // it by `ui.close()` below; a click outside or Esc closes it too.
    egui::ComboBox::from_id_salt("place-picker").selected_text(text).width(220.0).close_behavior(egui::containers::PopupCloseBehavior::CloseOnClickOutside).show_ui(ui, |ui| {
        let (_, c) = cat.get_or_insert_with(|| (project.root.clone(), catalogue::scan(&project.root)));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(filter).hint_text("filter").desired_width(120.0));
            if ui.small_button("Rescan").on_hover_text("Read the types of the maps again").clicked() {
                rescan = true;
            }
        });
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for t in view2d::filtered_types(c, filter) {
                let note = if t.scene().is_none() { " (no scene known)" } else { "" };
                if ui.selectable_label(false, format!("{}{note}", t.name)).clicked() {
                    chosen = Some(t.name.clone());
                    ui.close();
                }
            }
            if c.scenery.is_empty() {
                ui.label("No scenery found: the maps of the import hold the types.");
            }
        });
    });
    st.cat = if rescan { None } else { cat };
    if let Some(t) = chosen {
        st.info = format!("Click on the plan to place {t}.");
        st.place = Some(t);
    }
}

/// Draws the handle and returns the turn to apply when it was just released.
fn turn_handle(ui: &mut egui::Ui, st: &mut State, rect: Rect, centre: [f32; 2]) -> Option<f64> {
    let cs = st.view.to_screen(rect, centre);
    let hp = cs + vec2(0.0, -HANDLE_DIST);
    if !rect.contains(hp) {
        return None;
    }
    let resp = ui.interact(Rect::from_center_size(hp, vec2(18.0, 18.0)), ui.id().with("turn-handle"), Sense::drag());
    let resp = resp.on_hover_text("Drag to turn the selection about its centre").on_hover_cursor(egui::CursorIcon::Grab);
    let now: Option<Pos2> = ui.input(|i| i.pointer.latest_pos());
    let p = ui.painter_at(rect);
    let col = Color32::from_rgb(255, 170, 60);
    let mut released = None;
    if resp.dragged()
        && let Some(now) = now
    {
        let yaw = view2d::turn_between(cs, hp, now, &st.view.snap);
        st.turning = Some(yaw);
        p.line_segment([cs, now], egui::Stroke::new(1.5, col));
        p.text(now + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, format!("{:+.0} deg", yaw.to_degrees()), egui::FontId::monospace(12.0), col);
    } else {
        p.line_segment([cs, hp], egui::Stroke::new(1.0, col.gamma_multiply(0.5)));
    }
    if resp.drag_stopped() {
        released = st.turning.take().filter(|y| *y != 0.0);
    }
    p.circle_filled(hp, 6.0, col);
    p.circle_stroke(hp, 6.0, egui::Stroke::new(1.0, Color32::WHITE));
    released
}

fn place_at(st: &mut State, project: &mut Project, id: &str, scene: &Scene, ty: &str, cursor: [f32; 2], keep_armed: bool) {
    let found = st.catalogue(project, false).find(ty).cloned();
    let Some(found) = found else {
        st.error = Some(format!("'{ty}' is not in the catalogue any more: pick it again"));
        st.place = None;
        return;
    };
    let at = view2d::placement_pos(cursor, scene, &st.view.snap);
    match maps::add_entry(project, id, Array::Objects, view2d::scenery_entry(&found, at)) {
        Ok(index) => {
            st.select_next = vec![Ref::new(Array::Objects, index)];
            st.error = None;
            st.info = format!("Placed {ty} at {:.0}, {:.0}.", at[0], at[1]);
            if !keep_armed {
                st.place = None;
            }
        }
        // an extending map that cannot take an added entry says why: shown as it is
        Err(e) => st.error = Some(e),
    }
}

fn delete_selected(st: &mut State, project: &mut Project, id: &str, scene: &Scene) {
    let refs = view2d::deletable_in_order(&view2d::selected_refs(scene, &st.sel));
    if refs.is_empty() {
        st.info = "Starts are not deleted here: they belong to the players.".into();
        return;
    }
    let mut n = 0;
    for r in &refs {
        if let Err(e) = maps::delete_entry(project, id, r.array(), r.index) {
            st.error = Some(e);
            break;
        }
        n += 1;
    }
    if n > 0 {
        st.sel.clear();
        st.error = None;
        st.info = format!("Deleted {n} entr{}.", if n == 1 { "y" } else { "ies" });
    }
}

fn duplicate_selected(st: &mut State, project: &mut Project, id: &str, scene: &Scene) {
    let refs = view2d::selected_refs(scene, &st.sel);
    let copies: Vec<(Array, serde_json::Value)> = match maps::merged_view(project, id) {
        Some(map) => refs.iter().filter_map(|&r| view2d::duplicate_entry(map, r, [view2d::DUPLICATE_OFFSET; 2])).collect(),
        None => vec![],
    };
    if copies.is_empty() {
        st.info = "Nothing to duplicate: starts belong to the players.".into();
        return;
    }
    let mut added = vec![];
    for (which, entry) in copies {
        match maps::add_entry(project, id, which, entry) {
            Ok(i) => added.push(Ref::new(which, i)),
            Err(e) => {
                st.error = Some(e);
                break;
            }
        }
    }
    if !added.is_empty() {
        st.error = None;
        st.info = format!("Duplicated {} entr{}.", added.len(), if added.len() == 1 { "y" } else { "ies" });
        st.select_next = added;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::path::Path;

    fn put(root: &Path, rel: &str, v: &Value) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
    }

    /// A project over a synthetic import with one map `red`; returns (root, project).
    fn setup(name: &str) -> (PathBuf, project_panel::State) {
        let root = std::env::temp_dir().join(format!("ope-maps-panel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (assets, m) = (root.join("assets"), root.join("mod"));
        put(
            &assets,
            "maps/red/map.json",
            &json!({"name": "Red", "world_half": 400.0, "objects": [
                {"type": "Isle_A", "id": 1, "pos": [-100.0, 50.0, 0.0], "scene": "Isle_A"},
                {"type": "Rock_B", "id": 2, "pos": [100.0, 50.0, 0.0], "scene": "Rock_B"}
            ]}),
        );
        std::fs::create_dir_all(&m).unwrap();
        let mut ps = project_panel::State::default();
        ps.project = Some(Project::open(&m, Some(assets)).unwrap());
        (root, ps)
    }

    struct Rig {
        ctx: egui::Context,
        st: State,
        ps: project_panel::State,
        t: f64,
    }

    const SCREEN: Rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1000.0, 700.0));

    impl Rig {
        fn new(name: &str) -> (PathBuf, Rig) {
            let (root, ps) = setup(name);
            (root, Rig { ctx: egui::Context::default(), st: State::default(), ps, t: 0.0 })
        }

        fn frame(&mut self, mut events: Vec<egui::Event>) {
            self.t += 0.05;
            events.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            let raw = egui::RawInput { screen_rect: Some(SCREEN), time: Some(self.t), events, ..Default::default() };
            let (st, ps) = (&mut self.st, &mut self.ps);
            let mut full = self.ctx.run_ui(raw, |ui| tab(ui, st, ps));
            full.textures_delta.clear();
        }

        fn project(&mut self) -> &mut Project {
            self.ps.project.as_mut().unwrap()
        }

        /// Opens map `red` in the tab and lays it out (the first frames fit the view and learn the canvas rect).
        fn open_red(&mut self) {
            self.frame(vec![]);
            let id = "red";
            let p = self.ps.project.as_mut().unwrap();
            self.st.show_map(p, id);
            self.st.view.snap = SnapSettings::off();
            for _ in 0..3 {
                self.frame(vec![]);
            }
        }

        fn at(&self, w: [f32; 2]) -> Pos2 {
            self.st.view.to_screen(self.st.rect, w)
        }

        fn drag(&mut self, from: Pos2, to: Pos2) {
            self.frame(vec![egui::Event::PointerMoved(from)]);
            self.frame(vec![button(from, true)]);
            self.frame(vec![egui::Event::PointerMoved(from + (to - from) * 0.5)]);
            self.frame(vec![egui::Event::PointerMoved(to)]);
            self.frame(vec![button(to, false)]);
            self.frame(vec![]);
        }

        fn click(&mut self, at: Pos2) {
            self.frame(vec![egui::Event::PointerMoved(at)]);
            self.frame(vec![button(at, true)]);
            self.frame(vec![button(at, false)]);
            self.frame(vec![]);
        }

        fn map(&mut self) -> Value {
            maps::merged_view(self.project(), "red").unwrap().clone()
        }
    }

    fn button(p: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    #[test]
    fn the_status_line_takes_the_same_height_empty_or_full() {
        let height = |error: Option<&str>, info: &str| {
            let st = State { error: error.map(str::to_string), info: info.to_string(), ..State::default() };
            let ctx = egui::Context::default();
            let mut h = 0.0;
            ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))), ..Default::default() }, |ui| {
                let before = ui.cursor().min.y;
                message(ui, &st);
                h = ui.cursor().min.y - before;
            })
            .textures_delta
            .clear();
            h
        };
        let empty = height(None, "");
        assert!(empty > 5.0);
        assert_eq!(height(None, "Click on the plan to place Rock."), empty);
        assert_eq!(height(Some(&"a very long refusal ".repeat(40)), ""), empty, "a long text is cut, not wrapped onto more lines");
    }

    #[test]
    fn the_tab_draws_without_a_project_and_with_a_map_and_changes_nothing() {
        let mut st = State::default();
        let mut ps = project_panel::State::default();
        let ctx = egui::Context::default();
        let raw = egui::RawInput { screen_rect: Some(SCREEN), ..Default::default() };
        ctx.run_ui(raw, |ui| tab(ui, &mut st, &mut ps)).textures_delta.clear();
        let (root, mut rig) = Rig::new("draw");
        rig.open_red();
        assert_eq!(rig.st.map.as_deref(), Some("red"));
        assert!(rig.st.error.is_none(), "{:?}", rig.st.error);
        assert!(rig.project().dirty_rels().is_empty());
        assert_eq!(rig.st.view.world_half, 400.0);
        assert!(rig.st.rect.width() > 500.0 && rig.st.rect.height() > 300.0, "the canvas gets the space under the toolbars: {:?}", rig.st.rect);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_drag_on_the_plan_moves_the_object_as_one_undo_step_and_undo_follows() {
        let (root, mut rig) = Rig::new("drag");
        rig.open_red();
        let from = rig.at([-100.0, 50.0]);
        let k = rig.st.view.scale;
        rig.drag(from, from + vec2(40.0 * k, -20.0 * k)); // 40 right, 20 up in world units
        let m = rig.map();
        let pos = doc::pos(&doc::objects(&m)[0]).unwrap();
        assert!((pos[0] + 60.0).abs() < 0.2 && (pos[1] - 70.0).abs() < 0.2, "{pos:?}");
        let p = rig.project();
        assert_eq!(p.undo_label(), Some("Move objects 0"));
        p.undo();
        assert!(p.undo_label().is_none(), "the drag was ONE step");
        rig.frame(vec![]);
        let m = rig.map();
        assert_eq!(doc::pos(&doc::objects(&m)[0]).unwrap(), [-100.0, 50.0, 0.0], "the plan follows undo");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn placing_adds_an_object_on_an_extending_map_and_the_refusal_text_shows() {
        let (root, mut rig) = Rig::new("place");
        rig.open_red();
        // the import's map has keyed entries (every object has an id), so a placed object is a keyed add
        rig.st.cat = Some((rig.project().root.clone(), catalogue::scan(&rig.project().root)));
        assert!(rig.st.cat.as_ref().unwrap().1.find("Isle_A").is_some(), "the catalogue comes from the overlay");
        rig.st.place = Some("Isle_A".into());
        let at = rig.at([0.0, -150.0]);
        rig.click(at);
        let m = rig.map();
        assert_eq!(doc::objects(&m).len(), 3, "{:?} {:?}", rig.st.error, rig.st.info);
        let new = &doc::objects(&m)[2];
        assert_eq!((doc::kind(new), doc::id(new), doc::scene(new)), (Some("Isle_A"), Some(3), Some("Isle_A")));
        assert_eq!(rig.st.place, None, "one click places one");
        assert_eq!(rig.st.sel.iter().count(), 1, "the new object is selected");
        // a retail entry with no id makes the list unkeyed: add and delete are refused with the project's text
        let (root2, mut rig2) = Rig::new("refuse");
        put(
            &root2.join("assets"),
            "maps/red/map.json",
            &json!({"name": "Red", "world_half": 400.0, "objects": [{"type": "Decor", "pos": [0.0, 0.0, 0.0]}, {"type": "Isle_A", "id": 1, "pos": [50.0, 50.0, 0.0], "scene": "Isle_A"}]}),
        );
        rig2.ps.project.as_mut().unwrap().rebuild().unwrap();
        rig2.open_red();
        rig2.st.cat = Some((rig2.project().root.clone(), catalogue::scan(&rig2.project().root)));
        rig2.st.place = Some("Isle_A".into());
        let at = rig2.at([0.0, -150.0]);
        rig2.click(at);
        assert!(rig2.st.error.is_some(), "the refusal is shown");
        assert_eq!(doc::objects(&rig2.map()).len(), 2);
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&root2));
    }

    #[test]
    fn delete_and_duplicate_are_undo_steps_and_select_what_they_made() {
        let (root, mut rig) = Rig::new("dupdel");
        rig.open_red();
        let at = rig.at([100.0, 50.0]);
        rig.click(at);
        assert_eq!(rig.st.sel.iter().collect::<Vec<_>>(), [1]);
        let scene = view2d::build_scene(&rig.map());
        duplicate_selected(&mut rig.st, rig.ps.project.as_mut().unwrap(), "red", &scene);
        rig.frame(vec![]);
        let m = rig.map();
        let new = &doc::objects(&m)[2];
        assert_eq!((doc::kind(new), doc::id(new), doc::pos(new)), (Some("Rock_B"), Some(3), Some([130.0, 80.0, 0.0])));
        assert_eq!(rig.st.sel.iter().collect::<Vec<_>>(), [2], "the copy is selected");
        let scene = view2d::build_scene(&m);
        delete_selected(&mut rig.st, rig.ps.project.as_mut().unwrap(), "red", &scene);
        rig.frame(vec![]);
        assert_eq!(doc::objects(&rig.map()).len(), 2);
        assert!(rig.st.sel.is_empty());
        let p = rig.project();
        assert_eq!(p.undo_label(), Some("Delete objects 3"));
        p.undo();
        p.undo();
        assert_eq!(doc::objects(maps::merged_view(p, "red").unwrap()).len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    // T7
    #[test]
    fn an_armed_click_on_the_plan_adds_a_player_with_its_start_as_one_undo_step() {
        let (root, mut rig) = Rig::new("t7click");
        rig.open_red();
        rig.st.players.armed = Some(players::Arm::NewPlayer);
        let at = rig.at([60.0, -40.0]);
        rig.click(at);
        let m = rig.map();
        let p = &doc::players(&m)[0];
        let s = doc::player_start(p).unwrap();
        assert!((s[0] - 60.0).abs() < 0.5 && (s[1] + 40.0).abs() < 0.5, "{s:?}");
        assert_eq!((rig.st.players.armed, rig.st.players.selected, rig.st.error.clone()), (None, Some(0), None));
        assert_eq!(rig.st.sel.len(), 1, "the new start is selected on the plan");
        rig.project().undo();
        assert!(rig.project().undo_label().is_none(), "ONE step");
        let _ = std::fs::remove_dir_all(&root);
    }
    // end T7

    #[test]
    fn t8_draw_a_nebula_by_clicks_drag_a_point_insert_one_and_place_a_mine() {
        let (root, mut rig) = Rig::new("t8");
        rig.open_red();
        rig.st.t8.start_draw("Storm");
        rig.frame(vec![]); // the toolbar changes, the canvas moves: let it settle
        rig.frame(vec![]);
        for w in [[-100.0, -100.0], [100.0, -100.0], [100.0, -200.0]] {
            let at = rig.at(w);
            rig.click(at);
        }
        assert_eq!(rig.st.t8.error, None);
        let first = rig.at([-100.0, -100.0]);
        rig.click(first); // the first point again closes it
        let m = rig.map();
        assert_eq!(doc::nebulae(&m).len(), 1, "{:?} {:?}", rig.st.t8.error, rig.st.t8.info);
        assert_eq!(doc::nebula_polygon(&doc::nebulae(&m)[0]), vec![[-100.0, -100.0], [100.0, -100.0], [100.0, -200.0]]);
        assert_eq!(rig.project().undo_label(), Some("New nebula Storm"));
        // drag point 1 (now editing) 30 units right
        let (from, k) = (rig.at([100.0, -100.0]), rig.st.view.scale);
        rig.drag(from, from + vec2(30.0 * k, 0.0));
        let poly = doc::nebula_polygon(&doc::nebulae(&rig.map())[0]);
        assert!((poly[1][0] - 130.0).abs() < 0.2 && (poly[1][1] + 100.0).abs() < 0.2, "{poly:?}");
        assert_eq!(rig.project().undo_label(), Some("Move point 1 of nebula Storm"));
        // a click on an edge adds a point
        let mid = rig.at([0.0, -100.0]);
        rig.click(mid);
        assert_eq!(doc::nebula_polygon(&doc::nebulae(&rig.map())[0]).len(), 4, "{:?}", rig.st.t8.error);
        // a mine
        rig.st.t8.arm_mine("Bullet_StarMine_Med");
        rig.frame(vec![]);
        rig.frame(vec![]);
        let at = rig.at([0.0, 150.0]);
        rig.click(at);
        let m = rig.map();
        assert_eq!(doc::mines(&m).len(), 1, "{:?}", rig.st.t8.error);
        assert_eq!((doc::kind(&doc::mines(&m)[0]), doc::id(&doc::mines(&m)[0])), (Some("Bullet_StarMine_Med"), Some(3)));
        // with no tool on, a click on the plan selects as before
        rig.st.t8.start_edit(0, "Storm");
        rig.st.t8.start_draw("x");
        rig.st.t8.arm_mine("x");
        rig.frame(vec![]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_map_lists_and_a_new_map_opens_on_a_mod_of_its_own() {
        let (root, mut rig) = Rig::new("new");
        rig.frame(vec![]);
        assert!(rig.st.maps.iter().any(|m| m.id == "red" && m.in_import && !m.in_mod));
        maps::new_map(rig.project(), "mine").unwrap();
        rig.st.maps_stale = true;
        let p = rig.ps.project.as_mut().unwrap();
        rig.st.show_map(p, "mine");
        rig.frame(vec![]);
        assert_eq!(rig.st.map.as_deref(), Some("mine"));
        assert!(rig.st.maps.iter().any(|m| m.id == "mine" && m.in_mod));
        assert_eq!(rig.st.view.world_half, 1000.0, "the new map's own world_half");
        let _ = std::fs::remove_dir_all(&root);
    }
}
