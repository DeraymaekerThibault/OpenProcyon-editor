//! The 3D view of the open map (docs/phase4.md P7, work item T9): each scenery object drawn from its GLB (triangles, flat colours) at the
//! place the engine puts it, ships, mines and players' starts as simple markers, picking with the mouse, and a height handle that
//! moves the selection along z as one undo step. The selection is the plan's ([`super::view2d`] marker numbers), so selecting in one
//! view selects in the other.
//!
//! What is pure (placement, picking, snapping, the height edit, the model loader) is tested headless; the Bevy systems and the egui
//! canvas are thin. Nothing here names a map key: that is [`super::doc`]; what is written goes through [`super::project`].
//!
//! The 3D camera draws under egui's, in the canvas rect, on render layer [`LAYER`] so the 3D ship tab's meshes (layer 0) never show
//! here and these never show there.
//!
//! PROVISIONAL: textures are not loaded (the material's base colour only), the objects' hover is not highlighted (only markers get a
//! label), a ship's facing is the engine's `game_rotation` of its `rot` applied to the model's forward (-z) which is a guess, and the
//! load time of large retail GLBs is not measured (each model is read on its own thread, four at a time).

use super::doc::{self, Array};
use super::glb::{self, Model};
use super::project::Change;
use super::view2d::{self, Scene};
use crate::map_edit::select::Selection;
use crate::maps_panel;
use crate::ship_view::ViewInput;
use crate::ship_view::coords::{game_vec_to_bevy, to_bevy};
use crate::ship_view::pick::{axis_drag, triangle_hit};
use crate::ship_view::scene::distance_to_segment;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy_egui::egui;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The render layer of everything of this view.
pub const LAYER: usize = 1;

/// A marker is picked within this angle (radians) of its centre as seen from the camera, about ten pixels.
const PICK_ANGLE: f32 = 0.015;

/// Models read at the same time.
const IN_FLIGHT: usize = 4;

// ---- placement ----

/// The world transform of a scenery object: the engine's own rule (`authoring::scenery_transform` with `authoring::game_rotation`): the
/// translation is `pos` in Bevy axes minus the rotated origin, the origin being the object's `scene_origin` or else the height of the
/// type's first dock point in `docks`; the rotation is the game's 3x3 `rot` matrix changed to Bevy axes. The editor calls them instead
/// of restating them; a test pins the behaviour. PROVISIONAL: `docks` is `data/docks.json` as the overlay holds it (the engine also
/// merges each ship's `docks.json`, which no scenery uses).
pub fn placement(object: &Value, docks: &Value) -> Transform {
    openprocyon::authoring::scenery_transform(object, openprocyon::authoring::game_rotation(&object["rot"]), docks)
}

/// What an entry of the plan is, for how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Scenery,
    Ship,
    Mine,
    Start,
}

/// One entry of the plan as the 3D view knows it. `marker` is its number in the plan's [`Scene`] (the shared selection's key).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub marker: usize,
    pub kind: ItemKind,
    pub label: String,
    /// The `scene` (model name) of a scenery object.
    pub scene: Option<String>,
    /// `pos` (a start's `start`) in game axes.
    pub pos: [f64; 3],
    /// Where the model goes (scenery); a translation only for the others.
    pub tf: Transform,
    /// The way a ship or a start faces, a unit vector in Bevy axes; zero for the rest.
    pub facing: Vec3,
}

/// The 3D view's items for the plan `scene` of `map`: every entry the plan shows.
pub fn items(map: &Value, scene: &Scene, docks: &Value) -> Vec<Item> {
    let mut out = vec![];
    for (marker, r) in scene.refs.iter().enumerate() {
        let which = r.array();
        let Some(e) = doc::array_entries(map, which).get(r.index) else { continue };
        let Some(pos) = (if which == Array::Players { doc::player_start(e) } else { doc::pos(e) }) else { continue };
        let at = Transform::from_translation(to_bevy(pos));
        let label = scene.markers.get(marker).map(|m| m.label.clone()).unwrap_or_default();
        let (kind, tf, facing, model) = match which {
            Array::Objects => (ItemKind::Scenery, placement(e, docks), Vec3::ZERO, doc::scene(e).filter(|s| !s.is_empty()).map(str::to_string)),
            Array::Ships => (ItemKind::Ship, at, (openprocyon::authoring::game_rotation(&e["rot"]) * Vec3::NEG_Z).normalize_or_zero(), None),
            Array::Mines => (ItemKind::Mine, at, Vec3::ZERO, None),
            _ => {
                let f = doc::player_forward(e).map_or(Vec3::ZERO, |f| game_vec_to_bevy(Vec3::new(f[0] as f32, f[1] as f32, f[2] as f32)).normalize_or_zero());
                (ItemKind::Start, at, f, None)
            }
        };
        out.push(Item { marker, kind, label, scene: model, pos, tf, facing });
    }
    out
}

// ---- picking ----

/// What a ray can hit: a model placed by `tf`, or a point.
#[derive(Clone, Copy)]
pub struct Target<'a> {
    pub marker: usize,
    pub tf: Transform,
    /// The model to test the ray against; without one the target is a point at `tf`'s translation.
    pub model: Option<&'a Model>,
}

/// World size of a marker seen from `dist` away: it keeps about the same size on the screen, never below a small floor.
pub fn marker_radius(dist: f32) -> f32 {
    (dist * 0.010).max(0.8)
}

/// The distance along the unit-length ray `dir` to the model placed by `tf` (rigid: no scale), or `None`. The box of the model is
/// tried first.
pub fn ray_model(origin: Vec3, dir: Vec3, tf: &Transform, model: &Model) -> Option<f32> {
    let inv = tf.to_matrix().inverse();
    let (o, d) = (inv.transform_point3(origin), inv.transform_vector3(dir));
    // slab test against the model's box
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for k in 0..3 {
        if d[k].abs() < 1e-9 {
            if o[k] < model.aabb[0][k] || o[k] > model.aabb[1][k] {
                return None;
            }
        } else {
            let (a, b) = ((model.aabb[0][k] - o[k]) / d[k], (model.aabb[1][k] - o[k]) / d[k]);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
            if t0 > t1 {
                return None;
            }
        }
    }
    model.triangles().filter_map(|[a, b, c]| triangle_hit(o, d, a, b, c)).min_by(f32::total_cmp)
}

/// The distance along the unit ray to a marker at `p`, when the ray passes within [`PICK_ANGLE`] of it and the marker is in front.
pub fn point_hit(origin: Vec3, dir: Vec3, p: Vec3) -> Option<f32> {
    let to = p - origin;
    let along = to.dot(dir);
    (along > 0.0 && (to - dir * along).length() <= (along * PICK_ANGLE).max(marker_radius(along))).then_some(along)
}

/// The nearest target the ray hits: its marker number. `meshes` false tests the points only (the cheap hover).
pub fn pick(origin: Vec3, dir: Vec3, targets: &[Target], meshes: bool) -> Option<usize> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    targets
        .iter()
        .filter_map(|t| {
            let hit = match t.model {
                Some(m) if meshes => ray_model(origin, dir, &t.tf, m),
                Some(_) => None,
                None => point_hit(origin, dir, t.tf.translation),
            };
            hit.map(|h| (t.marker, h))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(m, _)| m)
}

// ---- the height edit ----

/// The change in height `dz` as a z that follows the grid: with a `grid` step the result lands on a multiple of it, measured from
/// zero (not from where the drag began). The f32's shortest decimal, as every position is written.
pub fn snap_dz(z0: f64, dz: f64, grid: Option<f32>) -> f64 {
    let raw = z0 + dz;
    let z = match grid.filter(|g| *g > 0.0) {
        Some(g) => (raw / f64::from(g)).round() * f64::from(g),
        None => raw,
    };
    view2d::short_f64(z - z0)
}

/// What raising the selection by `dz` writes: each selected entry's `pos` with z + `dz`, x and y kept. Entries with no position
/// and a zero `dz` write nothing. One call is one undo step through [`super::project::change_entries`].
pub fn height_changes(map: &Value, scene: &Scene, sel: &Selection, dz: f64) -> Vec<Change> {
    if dz == 0.0 || !dz.is_finite() {
        return vec![];
    }
    view2d::selected_refs(scene, sel)
        .into_iter()
        .filter_map(|r| {
            let e = doc::array_entries(map, r.array()).get(r.index)?;
            let p = if r.array() == Array::Players { doc::player_start(e)? } else { doc::pos(e)? };
            let z = view2d::short_f64(p[2] + dz);
            (z != p[2]).then(|| Change { pos: Some([p[0], p[1], z]), ..Change::new(r.array(), r.index) })
        })
        .collect()
}

/// The middle of the selected items (their game positions, as Bevy axes): where the height handle stands.
pub fn handle_anchor(items: &[Item], selected: &[usize]) -> Option<Vec3> {
    let pts: Vec<Vec3> = items.iter().filter(|i| selected.contains(&i.marker)).map(|i| to_bevy(i.pos)).collect();
    if pts.is_empty() {
        return None;
    }
    let (lo, hi) = pts.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    Some((lo + hi) / 2.0)
}

/// Whether a press at `cursor` (window pixels) lands on the height arrow from `anchor` up `len`; `project` maps a world point to the window.
pub fn handle_hit(cursor: Vec2, anchor: Vec3, len: f32, project: impl Fn(Vec3) -> Vec2) -> bool {
    distance_to_segment(cursor, project(anchor), project(anchor + Vec3::Y * len)) < 9.0
}

// ---- models ----

/// The state of one model.
#[derive(Clone, Debug)]
pub enum Slot {
    Loading,
    Ready(Arc<Model>),
    Failed(String),
}

type Inbox = Arc<Mutex<Vec<(String, Result<Model, String>)>>>;

/// The scenery models of the overlay: found by file name under `models/` (the engine's own rule: GLBs are looked up by file name
/// wherever they lie under `models/`), read on worker threads, kept until [`Models::reset`].
#[derive(Default)]
pub struct Models {
    root: Option<PathBuf>,
    files: Option<HashMap<String, PathBuf>>,
    pub slots: HashMap<String, Slot>,
    queue: VecDeque<(String, PathBuf)>,
    in_flight: usize,
    inbox: Inbox,
    /// Bumped when a model arrives or the set is reset.
    pub rev: u64,
    /// `data/docks.json` of the root (an empty object when there is none).
    pub docks: Value,
}

fn walk_glb(dir: &Path, depth: usize, out: &mut HashMap<String, PathBuf>) {
    if depth > 8 {
        return;
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk_glb(&p, depth + 1, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("glb"))
            && let Some(name) = p.file_name().and_then(|n| n.to_str())
        {
            out.entry(name.to_string()).or_insert(p);
        }
    }
}

impl Models {
    /// Forgets every model and the file index (a mod added or changed a model; or another root).
    pub fn reset(&mut self) {
        let (root, inbox, rev) = (self.root.take(), self.inbox.clone(), self.rev + 1);
        *self = Models { inbox, rev, ..Models::default() };
        inbox_clear(&self.inbox);
        if let Some(root) = root {
            self.set_root(&root);
        }
    }

    /// Looks for models and docks under `root`; another root forgets what was read.
    pub fn set_root(&mut self, root: &Path) {
        if self.root.as_deref() == Some(root) {
            return;
        }
        if self.root.is_some() {
            let (inbox, rev) = (self.inbox.clone(), self.rev + 1);
            *self = Models { inbox, rev, ..Models::default() };
            inbox_clear(&self.inbox);
        }
        self.root = Some(root.to_path_buf());
        self.docks = std::fs::read_to_string(root.join("data/docks.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
    }

    /// Starts reading the model of `scene` (its `.glb` by file name) when nobody has.
    pub fn want(&mut self, scene: &str) {
        if self.slots.contains_key(scene) {
            return;
        }
        let Some(root) = self.root.clone() else { return };
        let files = self.files.get_or_insert_with(|| {
            let mut m = HashMap::new();
            walk_glb(&root.join("models"), 0, &mut m);
            m
        });
        match files.get(&format!("{scene}.glb")) {
            Some(path) => {
                self.slots.insert(scene.to_string(), Slot::Loading);
                self.queue.push_back((scene.to_string(), path.clone()));
            }
            None => {
                self.slots.insert(scene.to_string(), Slot::Failed(format!("no {scene}.glb under models/")));
                self.rev += 1;
            }
        }
    }

    /// Starts queued reads (up to [`IN_FLIGHT`]) and takes in the finished ones.
    pub fn pump(&mut self) {
        let done: Vec<_> = std::mem::take(&mut *self.inbox.lock().unwrap_or_else(|e| e.into_inner()));
        for (name, r) in done {
            self.in_flight = self.in_flight.saturating_sub(1);
            // a reset in between forgot this name: the result is stale
            if matches!(self.slots.get(&name), Some(Slot::Loading)) {
                self.slots.insert(
                    name,
                    match r {
                        Ok(m) => Slot::Ready(Arc::new(m)),
                        Err(e) => Slot::Failed(e),
                    },
                );
                self.rev += 1;
            }
        }
        while self.in_flight < IN_FLIGHT {
            let Some((name, path)) = self.queue.pop_front() else { break };
            self.in_flight += 1;
            let inbox = self.inbox.clone();
            std::thread::spawn(move || {
                let r = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display())).and_then(|b| glb::read(&b).map_err(|e| format!("{}: {e}", path.display())));
                inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, r));
            });
        }
    }

    pub fn model(&self, scene: &str) -> Option<&Arc<Model>> {
        match self.slots.get(scene) {
            Some(Slot::Ready(m)) => Some(m),
            _ => None,
        }
    }

    /// How many models are ready, still loading, and unavailable.
    pub fn counts(&self) -> (usize, usize, usize) {
        self.slots.values().fold((0, 0, 0), |(r, l, f), s| match s {
            Slot::Ready(_) => (r + 1, l, f),
            Slot::Loading => (r, l + 1, f),
            Slot::Failed(_) => (r, l, f + 1),
        })
    }
}

fn inbox_clear(inbox: &Inbox) {
    inbox.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// ---- the shared state ----

/// The orbit camera: spherical coordinates around `target` (Bevy axes, y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: Vec3,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { yaw: -0.5, pitch: 0.7, dist: 1500.0, target: Vec3::ZERO }
    }
}

impl Orbit {
    /// The camera position.
    pub fn eye(&self) -> Vec3 {
        self.target + Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch) * Vec3::Z * self.dist
    }

    /// Shows the whole world square (half side `half`) from a slanted view; 3.8 half sides at a steeper angle keeps its near corner inside a wide, short canvas (a screenshot showed 2.2 cutting it off).
    pub fn fit(half: f32) -> Orbit {
        Orbit { dist: (half * 3.8).clamp(10.0, 50_000.0), pitch: 1.0, ..Orbit::default() }
    }
}

/// What the pointer did in the canvas this frame, for the Bevy systems.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input3d {
    pub base: ViewInput,
    pub pan_drag: Vec2,
    pub shift: bool,
    pub ctrl: bool,
}

/// A click the plan's selection must take: the marker it hit (or empty space) and the modifiers held.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    pub hit: Option<usize>,
    pub shift: bool,
    pub ctrl: bool,
}

/// A height drag in progress: nothing is edited until it is released.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub start_cursor: Vec2,
    pub anchor: Vec3,
    /// The anchor's game z when the drag began (what the grid snaps).
    pub z0: f64,
}

/// Everything the 3D view keeps. A field of the Maps tab's state: egui feeds it ([`View3d::feed`], [`canvas`]) and reads the answers
/// (`click`, `height`); the Bevy systems of [`plugin`] draw it and answer.
#[derive(Default)]
pub struct View3d {
    /// The 3D view is shown instead of the plan.
    pub on: bool,
    pub models: Models,
    pub items: Vec<Item>,
    /// Bumped when `items` changed.
    pub items_rev: u64,
    /// The selected markers, from the plan's selection.
    pub selected: Vec<usize>,
    pub grid: Option<f32>,
    pub half: f32,
    pub orbit: Orbit,
    /// The map the camera was framed for.
    fit_for: Option<String>,
    /// Pointer of this frame, from egui; taken by the camera system.
    pub input: Option<Input3d>,
    /// The pointer as the systems use it this frame.
    pub cur: Option<Input3d>,
    pub drag: Option<Drag>,
    /// The height change the drag shows now (the selection moves by it on screen).
    pub preview_dz: f64,
    /// Answers for egui, taken there.
    pub click: Option<Click>,
    pub height: Option<f64>,
    pub hover_label: Option<(Vec2, String)>,
    /// Top of the canvas as of the last frame (the toolbar rows above it get their background from the window).
    pub canvas_top: Option<f32>,
}

impl View3d {
    /// What the tab tells the view each frame: the plan's scene and selection. Starts reading the models the scenery names, and frames
    /// the camera on the first frame of another map. `(grid, half)`: the plan's grid step and the world square's half side.
    pub fn feed(&mut self, root: &Path, map_id: &str, map: &Value, scene: &Scene, sel: &Selection, (grid, half): (Option<f32>, f32)) {
        self.models.set_root(root);
        self.models.pump();
        self.half = half;
        if self.fit_for.as_deref() != Some(map_id) {
            self.fit_for = Some(map_id.to_string());
            self.orbit = Orbit::fit(half);
        }
        let now = items(map, scene, &self.models.docks);
        if now != self.items {
            self.items = now;
            self.items_rev += 1;
        }
        let want: std::collections::BTreeSet<String> = self.items.iter().filter_map(|i| i.scene.clone()).collect();
        for s in want {
            self.models.want(&s);
        }
        self.selected = sel.iter().collect();
        self.grid = grid;
    }

    /// Frames the camera on the whole world square.
    pub fn fit(&mut self) {
        self.orbit = Orbit::fit(self.half);
    }

    /// Looks at the selection (its middle), keeping the angle and, when the selection is far, the distance.
    pub fn frame_selection(&mut self) {
        if let Some(c) = handle_anchor(&self.items, &self.selected) {
            self.orbit.target = c;
            self.orbit.dist = self.orbit.dist.clamp(20.0, 600.0);
        }
    }

    /// The strip of the central area above the canvas, which the toolbar rows use: `area` is the whole central rect.
    pub fn bar_rect(&self, area: egui::Rect) -> egui::Rect {
        let bottom = self.canvas_top.map_or(area.max.y, |t| t.clamp(area.min.y, area.max.y));
        egui::Rect::from_min_max(area.min, egui::pos2(area.max.x, bottom))
    }

    /// The targets a ray tests: each item with its model when it is loaded, else a point.
    pub fn targets(&self) -> Vec<Target<'_>> {
        self.items.iter().map(|i| Target { marker: i.marker, tf: i.tf, model: i.scene.as_deref().and_then(|s| self.models.model(s)).map(|m| &**m) }).collect()
    }
}

// ---- egui ----

fn v2(p: egui::Pos2) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// The canvas: reserves the rest of the tab, records the pointer for the Bevy systems and paints the hovered marker's label. The 3D
/// camera draws underneath (the tab's panel is transparent while the 3D view is on).
pub fn canvas(ui: &mut egui::Ui, v3: &mut View3d) -> egui::Response {
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    v3.canvas_top = Some(rect.min.y);
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let over = response.hovered() || response.dragged() || response.is_pointer_button_down_on();
    let (pressed, down, released, scroll, mods) = ui.input(|i| {
        (
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.pointer.primary_down(),
            i.pointer.button_released(egui::PointerButton::Primary),
            i.smooth_scroll_delta.y,
            i.modifiers,
        )
    });
    let drag = response.drag_delta();
    v3.input = Some(Input3d {
        base: ViewInput {
            rect: Some((v2(rect.min), v2(rect.max))),
            pixels_per_point: ui.ctx().pixels_per_point(),
            hover: pointer.filter(|_| over).map(v2),
            primary_pressed: pressed && response.hovered(),
            primary_down: down,
            primary_released: released,
            orbit_drag: if response.dragged_by(egui::PointerButton::Secondary) { Vec2::new(drag.x, drag.y) } else { Vec2::ZERO },
            scroll: if response.hovered() { scroll } else { 0.0 },
        },
        pan_drag: if response.dragged_by(egui::PointerButton::Middle) { Vec2::new(drag.x, drag.y) } else { Vec2::ZERO },
        shift: mods.shift,
        ctrl: mods.command,
    });
    if let Some((at, text)) = &v3.hover_label {
        let galley = ui.painter().layout_no_wrap(text.clone(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
        let r = egui::Rect::from_min_size(egui::pos2(at.x, at.y), galley.size()).expand(3.0);
        ui.painter().rect_filled(r, 3.0, egui::Color32::from_black_alpha(190));
        ui.painter().galley(egui::pos2(at.x, at.y), galley, egui::Color32::WHITE);
    }
    response
}

/// The 3D view's own buttons and the load state, one row.
pub fn toolbar(ui: &mut egui::Ui, v3: &mut View3d) {
    if ui.button("Fit").on_hover_text("Show the whole world square").clicked() {
        v3.fit();
    }
    if ui.add_enabled(!v3.selected.is_empty(), egui::Button::new("Frame selection")).clicked() {
        v3.frame_selection();
    }
    if ui.button("Reload models").on_hover_text("Read the GLBs and data/docks.json again (a mod added or changed one)").clicked() {
        v3.models.reset();
    }
    let (ready, loading, missing) = v3.models.counts();
    let mut text = format!("{ready} models");
    if loading > 0 {
        text += &format!(", {loading} loading");
    }
    let r = ui.label(text);
    if missing > 0 {
        let mut names: Vec<String> = v3.models.slots.iter().filter_map(|(k, s)| matches!(s, Slot::Failed(_)).then_some(k.clone())).collect();
        names.sort();
        ui.colored_label(egui::Color32::YELLOW, format!("{missing} without a model (drawn as markers)")).on_hover_text(names.join("\n"));
    }
    r.on_hover_text("Right drag orbits, middle drag pans, the wheel zooms. Click selects (shift adds, ctrl toggles); drag the blue arrow to change the height.");
}

// ---- Bevy ----

/// Gizmos of this view: on [`LAYER`] only, drawn over the models.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct MapGizmos;

#[derive(Component)]
pub(super) struct MapCamera;

/// A model entity: the item it draws and the height it was spawned at (the drag's preview moves it from there).
#[derive(Component)]
struct ItemEntity {
    marker: usize,
    base_y: f32,
}

#[derive(Resource)]
struct Mats {
    normal: Handle<StandardMaterial>,
    selected: Handle<StandardMaterial>,
}

/// Registers the systems of the view. Needs the ship view's plugin for the ambient light (or any `GlobalAmbientLight`).
pub fn plugin(app: &mut App) {
    app.init_gizmo_group::<MapGizmos>().add_systems(Startup, setup).add_systems(Update, (camera, sync, interact, highlight, draw).chain());
    // T12 begin
    app.add_plugins(super::docks::plugin);
    // T12 end
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>, mut gizmos: ResMut<GizmoConfigStore>) {
    let (config, _) = gizmos.config_mut::<MapGizmos>();
    config.render_layers = RenderLayers::layer(LAYER);
    config.depth_bias = -1.0;
    let layers = RenderLayers::layer(LAYER);
    commands.spawn((
        MapCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { near: 0.5, far: 200_000.0, ..default() }),
        Camera { order: -1, is_active: false, clear_color: ClearColorConfig::Custom(Color::srgb(0.04, 0.05, 0.08)), ..default() },
        Transform::default(),
        layers.clone(),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(-30.0, 60.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y),
        layers,
    ));
    let material = |color: Color| StandardMaterial { base_color: color, perceptual_roughness: 0.9, cull_mode: None, double_sided: true, ..default() };
    commands.insert_resource(Mats { normal: materials.add(material(Color::WHITE)), selected: materials.add(material(Color::srgb(1.0, 0.82, 0.45))) });
    commands.insert_resource(GlobalAmbientLight { brightness: 350.0, ..default() });
}

/// The pointer of this frame becomes current; orbit, pan and zoom; the camera draws in the canvas rect only while the tab asks.
pub(super) fn camera(mut st: ResMut<maps_panel::State>, mut cam: Single<(&mut Camera, &mut Transform), With<MapCamera>>) {
    let v3 = &mut st.v3;
    v3.cur = v3.input.take();
    let (camera, tf) = &mut *cam;
    let Some((input, (min, max))) = v3.cur.and_then(|i| i.base.rect.filter(|(a, b)| b.x > a.x + 1.0 && b.y > a.y + 1.0).map(|r| (i, r))) else {
        camera.is_active = false;
        v3.drag = None;
        return;
    };
    let o = &mut v3.orbit;
    o.yaw -= input.base.orbit_drag.x * 0.008;
    o.pitch = (o.pitch + input.base.orbit_drag.y * 0.008).clamp(-1.5, 1.5);
    o.dist = (o.dist * (1.0 - input.base.scroll * 0.002)).clamp(2.0, 50_000.0);
    if input.pan_drag != Vec2::ZERO {
        let rot = Quat::from_rotation_y(o.yaw) * Quat::from_rotation_x(-o.pitch);
        let k = o.dist * 0.0012;
        o.target += rot * (Vec3::NEG_X * input.pan_drag.x + Vec3::Y * input.pan_drag.y) * k;
    }
    **tf = Transform::from_translation(o.eye()).looking_at(o.target, Vec3::Y);
    let s = input.base.pixels_per_point.max(0.1);
    camera.viewport = Some(Viewport {
        physical_position: UVec2::new((min.x * s) as u32, (min.y * s) as u32),
        physical_size: UVec2::new(((max.x - min.x) * s) as u32, ((max.y - min.y) * s) as u32),
        ..default()
    });
    camera.is_active = true;
}

/// A mesh of a model: positions, flat normals, the base colours.
fn model_mesh(m: &Model) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, m.flat_normals());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, m.colors.clone());
    mesh
}

type MeshCache = HashMap<String, (Arc<Model>, Handle<Mesh>)>;

/// New entities when the items or the models changed.
fn sync(
    mut commands: Commands,
    mut st: ResMut<maps_panel::State>,
    mats: Res<Mats>,
    old: Query<Entity, With<ItemEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: Local<MeshCache>,
    mut seen: Local<(u64, u64)>,
) {
    let v3 = &mut st.v3;
    if v3.cur.is_none() {
        return;
    }
    v3.models.pump();
    let now = (v3.items_rev, v3.models.rev);
    if *seen == now {
        return;
    }
    *seen = now;
    for e in &old {
        commands.entity(e).despawn();
    }
    for item in v3.items.iter().filter(|i| i.kind == ItemKind::Scenery) {
        let Some((name, model)) = item.scene.as_deref().and_then(|s| Some((s, v3.models.model(s)?))) else { continue };
        if model.triangle_count() == 0 {
            continue;
        }
        let handle = match cache.get(name) {
            Some((m, h)) if Arc::ptr_eq(m, model) => h.clone(),
            _ => {
                let h = meshes.add(model_mesh(model));
                cache.insert(name.to_string(), (model.clone(), h.clone()));
                h
            }
        };
        commands.spawn((
            ItemEntity { marker: item.marker, base_y: item.tf.translation.y },
            Mesh3d(handle),
            MeshMaterial3d(mats.normal.clone()),
            item.tf,
            RenderLayers::layer(LAYER),
        ));
    }
}

/// Hover, the height drag, and a click: selection is the tab's, which takes `click`.
fn interact(mut st: ResMut<maps_panel::State>, cam: Single<(&Camera, &GlobalTransform), With<MapCamera>>) {
    // T12 begin: the scenery-points mode does the hover, the click and the drags of the selected object itself
    let points_mode = st.t12.active();
    // T12 end
    let v3 = &mut st.v3;
    v3.hover_label = None;
    let Some(input) = v3.cur else {
        v3.drag = None;
        v3.preview_dz = 0.0;
        return;
    };
    // T12 begin
    if points_mode {
        v3.drag = None;
        v3.preview_dz = 0.0;
        return;
    }
    // T12 end
    let (camera, gt) = *cam;
    // Window coordinates (logical): the viewport's origin is added and subtracted by Bevy itself (see ship_view::scene::interact).
    let cursor = input.base.hover;
    let ray = cursor.and_then(|c| camera.viewport_to_world(gt, c).ok());
    let project = |p: Vec3| camera.world_to_viewport(gt, p).unwrap_or(Vec2::splat(f32::MAX / 4.0));
    let handle_len = v3.orbit.dist * 0.12;

    if let Some(d) = v3.drag {
        if let Some(c) = cursor {
            let dist = axis_drag(Vec3::Y, d.start_cursor, c, project, d.anchor);
            v3.preview_dz = snap_dz(d.z0, f64::from(dist), v3.grid);
        }
        if input.base.primary_released || !input.base.primary_down {
            v3.drag = None;
            let dz = std::mem::take(&mut v3.preview_dz);
            if dz != 0.0 {
                v3.height = Some(dz);
            }
        }
        return;
    }
    v3.preview_dz = 0.0;
    let Some(ray) = ray else { return };
    let (origin, dir) = (ray.origin, *ray.direction);
    {
        let targets = v3.targets();
        if let (Some(m), Some(c)) = (pick(origin, dir, &targets, false), cursor)
            && let Some(item) = v3.items.iter().find(|i| i.marker == m)
        {
            let what = match item.kind {
                ItemKind::Scenery => "scenery",
                ItemKind::Ship => "ship",
                ItemKind::Mine => "mine",
                ItemKind::Start => "start",
            };
            v3.hover_label = Some((c + Vec2::new(12.0, 12.0), format!("{what} {} (z {:.1})", item.label, item.pos[2])));
        }
    }
    if !input.base.primary_pressed {
        return;
    }
    let Some(c) = cursor else { return };
    if let Some(anchor) = handle_anchor(&v3.items, &v3.selected)
        && handle_hit(c, anchor, handle_len, project)
    {
        v3.drag = Some(Drag { start_cursor: c, anchor, z0: f64::from(anchor.y) });
        v3.preview_dz = 0.0;
        return;
    }
    let hit = pick(origin, dir, &v3.targets(), true);
    v3.click = Some(Click { hit, shift: input.shift, ctrl: input.ctrl });
}

/// Selected models take the highlight material and the drag's preview height.
fn highlight(mut st: ResMut<maps_panel::State>, mats: Res<Mats>, mut q: Query<(&ItemEntity, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>) {
    let v3 = &mut st.v3;
    for (e, mut tf, mut mat) in &mut q {
        let selected = v3.selected.contains(&e.marker);
        let want = if selected { &mats.selected } else { &mats.normal };
        if mat.0 != *want {
            mat.0 = want.clone();
        }
        let y = e.base_y + if selected { v3.preview_dz as f32 } else { 0.0 };
        if tf.translation.y != y {
            tf.translation.y = y;
        }
    }
}

/// The ground square, the markers, the selection's boxes and the height arrow.
fn draw(mut gizmos: Gizmos<MapGizmos>, st: Res<maps_panel::State>, cam: Single<&Transform, With<MapCamera>>) {
    let points_mode = st.t12.active(); // T12
    let v3 = &st.v3;
    if v3.cur.is_none() {
        return;
    }
    let eye = cam.translation;
    let half = v3.half.max(1.0);
    gizmos.rect(Isometry3d::new(Vec3::ZERO, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), Vec2::splat(half * 2.0), Color::srgb(0.35, 0.4, 0.5));
    gizmos.line(Vec3::new(-half, 0.0, 0.0), Vec3::new(half, 0.0, 0.0), Color::srgba(0.35, 0.4, 0.5, 0.5));
    gizmos.line(Vec3::new(0.0, 0.0, -half), Vec3::new(0.0, 0.0, half), Color::srgba(0.35, 0.4, 0.5, 0.5));
    let dz = v3.preview_dz as f32;
    for item in &v3.items {
        let selected = v3.selected.contains(&item.marker);
        let lift = if selected { Vec3::Y * dz } else { Vec3::ZERO };
        let p = item.tf.translation + lift;
        let has_model = item.scene.as_deref().and_then(|s| v3.models.model(s)).is_some();
        let (r, g, b) = match item.kind {
            ItemKind::Scenery => (0.7, 0.75, 0.8),
            ItemKind::Ship => (0.35, 0.9, 0.4),
            ItemKind::Mine => (1.0, 0.35, 0.3),
            ItemKind::Start => (0.4, 0.6, 1.0),
        };
        let mut color = Color::srgb(r, g, b);
        if selected {
            color = Color::srgb(1.0, 0.85, 0.2);
        }
        let radius = marker_radius(eye.distance(p));
        if !has_model {
            gizmos.sphere(Isometry3d::from_translation(p), radius, color).resolution(10);
        }
        if item.facing != Vec3::ZERO {
            gizmos.arrow(p, p + item.facing * radius * 4.0, color);
        }
        if selected {
            if let Some(model) = item.scene.as_deref().and_then(|s| v3.models.model(s)) {
                let size = (model.aabb[1] - model.aabb[0]).max(Vec3::splat(0.01));
                let local = Transform::from_translation((model.aabb[0] + model.aabb[1]) / 2.0).with_scale(size);
                let mut world = item.tf * local;
                world.translation += lift;
                gizmos.cube(world, color);
            }
            // the altitude: a line down to the ground
            gizmos.line(p, Vec3::new(p.x, 0.0, p.z), Color::srgba(1.0, 0.85, 0.2, 0.4));
        }
    }
    if let Some(anchor) = handle_anchor(&v3.items, &v3.selected).filter(|_| !points_mode) {
        let a = anchor + Vec3::Y * dz;
        gizmos.arrow(a, a + Vec3::Y * v3.orbit.dist * 0.12, Color::srgb(0.3, 0.5, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::glb::fixtures::cube;
    use serde_json::json;

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn the_engine_rule_places_scenery_and_the_editor_calls_it() {
        let docks = json!({"Base": {"points": [{"pos": [0.0, 0.0, 3.0]}]}});
        // no rot: identity; a dock 3 up puts the model 3 below `pos` (game z 5 is Bevy y 5)
        let plain = placement(&json!({"type": "Base", "pos": [10.0, 20.0, 5.0]}), &docks);
        assert!(near(plain.translation, Vec3::new(10.0, 2.0, -20.0)), "{:?}", plain.translation);
        assert!(plain.rotation.angle_between(Quat::IDENTITY) < 1e-5);
        // a type without a dock is not shifted
        assert!(near(placement(&json!({"type": "Isle", "pos": [10.0, 20.0, 5.0]}), &docks).translation, Vec3::new(10.0, 5.0, -20.0)));
        // a two-number pos has z 0
        assert!(near(placement(&json!({"type": "Isle", "pos": [10.0, 20.0]}), &docks).translation, Vec3::new(10.0, 0.0, -20.0)));
        // a quarter turn about the game's z (up), `scene_origin` given: the origin is rotated and subtracted, the dock is ignored
        let turned = placement(&json!({"type": "Base", "pos": [10.0, 20.0, 5.0], "rot": [0, 1, 0, -1, 0, 0, 0, 0, 1], "scene_origin": [1.0, 3.0, -2.0]}), &docks);
        assert!(near(turned.translation, Vec3::new(13.0, 7.0, -19.0)), "{:?}", turned.translation);
        // game +y (the nose) goes to game -x here: Bevy -z turns to Bevy -x
        assert!(near(turned.rotation * Vec3::NEG_Z, Vec3::NEG_X), "{:?}", turned.rotation * Vec3::NEG_Z);
        assert!(near(turned.rotation * Vec3::Y, Vec3::Y), "up stays up");
    }

    fn plan() -> (Value, Scene) {
        let map = json!({
            "objects": [
                {"type": "Isle", "id": 1, "scene": "Isle", "pos": [0.0, 0.0, 4.0]},
                {"type": "Rock", "id": 2, "pos": [100.0, 0.0]}
            ],
            "ships": [{"type": "Ship_A", "id": 3, "pos": [0.0, 100.0, 10.0], "rot": [1, 0, 0, 0, 1, 0, 0, 0, 1]}],
            "mines": [{"type": "Mine", "id": 4, "pos": [-100.0, 0.0, 0.0]}],
            "players": [{"name": "P1", "start": [0.0, -100.0, 0.0], "forward": [0.0, 1.0, 0.0]}, {"name": "NoStart"}]
        });
        let scene = view2d::build_scene(&map);
        (map, scene)
    }

    #[test]
    fn every_plan_entry_becomes_an_item_with_the_plans_marker_number() {
        let (map, scene) = plan();
        let it = items(&map, &scene, &json!({}));
        assert_eq!(it.len(), scene.refs.len());
        assert_eq!(it.iter().map(|i| i.marker).collect::<Vec<_>>(), (0..it.len()).collect::<Vec<_>>());
        assert_eq!(it.iter().map(|i| i.kind).collect::<Vec<_>>(), [ItemKind::Scenery, ItemKind::Scenery, ItemKind::Ship, ItemKind::Mine, ItemKind::Start]);
        assert_eq!((it[0].scene.as_deref(), it[1].scene.as_deref()), (Some("Isle"), None), "a scene is only what the entry says");
        assert!(near(it[0].tf.translation, Vec3::new(0.0, 4.0, 0.0)));
        // the ship's identity rot faces Bevy -z (game +y); the start's forward (game +y) is the same way
        assert!(near(it[2].facing, Vec3::NEG_Z) && near(it[4].facing, Vec3::NEG_Z), "{:?} {:?}", it[2].facing, it[4].facing);
        assert_eq!(it[3].facing, Vec3::ZERO);
    }

    #[test]
    fn a_ray_hits_the_nearest_model_by_its_triangles_and_a_marker_by_its_angle() {
        let model = glb::read(&cube([0.0; 3], [1.0; 4])).unwrap();
        let near_cube = Target { marker: 0, tf: Transform::from_xyz(0.0, 0.0, -20.0), model: Some(&model) };
        let far_cube = Target { marker: 1, tf: Transform::from_xyz(0.0, 0.0, -50.0), model: Some(&model) };
        let marker = Target { marker: 2, tf: Transform::from_xyz(30.0, 0.0, -10.0), model: None };
        let targets = [far_cube, near_cube, marker];
        let o = Vec3::ZERO;
        assert_eq!(pick(o, Vec3::NEG_Z, &targets, true), Some(0), "the nearer cube hides the far one");
        assert_eq!(pick(o, Vec3::new(0.0, 0.0, -1.0) * 7.0, &targets, true), Some(0), "the direction need not be unit");
        assert_eq!(pick(o, Vec3::new(0.05, 0.0, -1.0), &targets, true), Some(0), "a shot at the cube's edge region still hits it (x 1 at z -20)");
        assert_eq!(pick(o, Vec3::new(0.2, 0.0, -1.0), &targets, true), None, "beside both cubes");
        assert_eq!(pick(o, Vec3::new(3.0, 0.0, -1.0), &targets, true), Some(2), "the marker, 10 units ahead and 30 across");
        assert_eq!(pick(o, Vec3::NEG_Z, &targets, false), None, "the hover tests markers only");
        assert_eq!(pick(o, Vec3::Z, &targets, true), None, "nothing behind the camera");
        assert_eq!(pick(o, Vec3::ZERO, &targets, true), None);
        // a turned model: the cube's box (side 2) turned 45 degrees about y reaches 1.414 on x
        let turned = Target { marker: 3, tf: Transform::from_xyz(0.0, 0.0, -20.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)), model: Some(&model) };
        assert_eq!(pick(Vec3::new(1.3, 0.0, 0.0), Vec3::NEG_Z, &[turned], true), Some(3));
        assert_eq!(pick(Vec3::new(1.5, 0.0, 0.0), Vec3::NEG_Z, &[turned], true), None);
    }

    #[test]
    fn a_marker_keeps_a_screen_sized_hit_area() {
        // twice as far: twice the hit radius, until the floor
        assert!(point_hit(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(14.0, 0.0, -1000.0)).is_some());
        assert!(point_hit(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(16.0, 0.0, -1000.0)).is_none());
        assert!(point_hit(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(0.7, 0.0, -2.0)).is_some(), "the floor");
        assert!(marker_radius(0.0) > 0.0);
    }

    #[test]
    fn the_height_change_keeps_x_and_y_and_has_no_step_for_nothing() {
        let (map, scene) = plan();
        let mut sel = Selection::new();
        sel.add(0);
        sel.add(1);
        sel.add(2);
        let ch = height_changes(&map, &scene, &sel, 2.5);
        assert_eq!(ch.len(), 3);
        assert_eq!(ch[0].pos, Some([0.0, 0.0, 6.5]));
        assert_eq!(ch[1].pos, Some([100.0, 0.0, 2.5]), "a two-number pos gets a z");
        assert_eq!(ch[2].pos, Some([0.0, 100.0, 12.5]));
        assert!(ch.iter().all(|c| c.rot.is_none() && c.forward.is_none()));
        assert!(height_changes(&map, &scene, &sel, 0.0).is_empty());
        assert!(height_changes(&map, &scene, &sel, f64::NAN).is_empty());
        assert!(height_changes(&map, &scene, &Selection::new(), 1.0).is_empty());
        // a start's height goes to its `start`
        let mut start = Selection::new();
        start.add(4);
        assert_eq!(height_changes(&map, &scene, &start, 1.0)[0].pos, Some([0.0, -100.0, 1.0]));
    }

    #[test]
    fn the_grid_snaps_the_height_not_the_distance_dragged() {
        assert_eq!(snap_dz(4.0, 2.4, None), 2.4);
        assert_eq!(snap_dz(4.0, 2.4, Some(5.0)), 1.0, "4 + 2.4 = 6.4 lands on 5");
        assert_eq!(snap_dz(4.0, -0.4, Some(5.0)), 1.0);
        assert_eq!(snap_dz(0.0, 7.6, Some(10.0)), 10.0);
        assert_eq!(snap_dz(4.0, 2.4, Some(0.0)), snap_dz(4.0, 2.4, None), "a zero step is no grid");
    }

    #[test]
    fn the_handle_stands_in_the_middle_of_the_selection_and_is_hit_along_its_arrow() {
        let (map, scene) = plan();
        let it = items(&map, &scene, &json!({}));
        assert_eq!(handle_anchor(&it, &[]), None);
        assert!(near(handle_anchor(&it, &[0]).unwrap(), Vec3::new(0.0, 4.0, 0.0)));
        // objects 0 (0,0,4) and ship (0,100,10): the box middle in Bevy axes
        assert!(near(handle_anchor(&it, &[0, 2]).unwrap(), Vec3::new(0.0, 7.0, -50.0)));
        // a top-down "screen": x to the right, world -z up the screen, 1 unit one pixel; the arrow rises along y (the screen's y)
        let project = |p: Vec3| Vec2::new(p.x, -p.y);
        assert!(handle_hit(Vec2::new(0.0, -20.0), Vec3::ZERO, 50.0, project));
        assert!(handle_hit(Vec2::new(5.0, -30.0), Vec3::ZERO, 50.0, project));
        assert!(!handle_hit(Vec2::new(20.0, -20.0), Vec3::ZERO, 50.0, project));
        assert!(!handle_hit(Vec2::new(0.0, -70.0), Vec3::ZERO, 50.0, project), "past the tip");
    }

    #[test]
    fn a_height_edit_is_one_undo_step_through_the_project() {
        use crate::map::project as maps;
        use crate::project::Project;
        let root = std::env::temp_dir().join(format!("ope-view3d-z-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (assets, modr) = (root.join("assets"), root.join("mod"));
        std::fs::create_dir_all(assets.join("maps/red")).unwrap();
        std::fs::create_dir_all(&modr).unwrap();
        std::fs::write(
            assets.join("maps/red/map.json"),
            json!({"name": "Red", "objects": [{"type": "Isle", "id": 1, "pos": [1.0, 2.0, 3.0], "scene": "Isle"}, {"type": "Rock", "id": 2, "pos": [5.0, 6.0]}]}).to_string(),
        )
        .unwrap();
        let mut project = Project::open(&modr, Some(assets)).unwrap();
        maps::open_map(&mut project, "red").unwrap();
        let (map, scene) = {
            let m = maps::merged_view(&project, "red").unwrap().clone();
            let s = view2d::build_scene(&m);
            (m, s)
        };
        let mut sel = Selection::new();
        sel.add(0);
        sel.add(1);
        let changes = height_changes(&map, &scene, &sel, 1.5);
        let before = project.undo_label().map(str::to_string);
        maps::change_entries(&mut project, "red", "Set height of 2 entries", &changes).unwrap();
        let now = maps::merged_view(&project, "red").unwrap();
        assert_eq!(doc::pos(&doc::objects(now)[0]), Some([1.0, 2.0, 4.5]));
        assert_eq!(doc::pos(&doc::objects(now)[1]), Some([5.0, 6.0, 1.5]), "the two-number pos became three");
        assert_eq!(project.undo_label(), Some("Set height of 2 entries"));
        project.undo();
        let back = maps::merged_view(&project, "red").unwrap();
        assert_eq!(doc::pos(&doc::objects(back)[0]), Some([1.0, 2.0, 3.0]));
        assert_eq!(doc::pos(&doc::objects(back)[1]), Some([5.0, 6.0, 0.0]), "both entries came back with one undo");
        assert_eq!(back.pointer("/objects/1/pos"), Some(&json!([5.0, 6.0])), "the two-number pos is two numbers again");
        assert_eq!(project.undo_label().map(str::to_string), before, "that was the only step");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn put_glb(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn settle(m: &mut Models) {
        for _ in 0..200 {
            m.pump();
            if m.counts().1 == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("models did not load");
    }

    #[test]
    fn models_are_found_by_file_name_anywhere_under_models_and_read_off_thread() {
        let root = std::env::temp_dir().join(format!("ope-view3d-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        put_glb(&root, "models/scenery/islands/Isle.glb", &cube([0.0; 3], [1.0; 4]));
        put_glb(&root, "models/other/Broken.glb", b"not a glb");
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data/docks.json"), r#"{"Isle": {"points": [{"pos": [0, 0, 1]}]}}"#).unwrap();
        let mut m = Models::default();
        m.set_root(&root);
        assert_eq!(m.docks["Isle"]["points"][0]["pos"][2], 1);
        for s in ["Isle", "Broken", "Absent"] {
            m.want(s);
        }
        settle(&mut m);
        assert_eq!(m.counts(), (1, 0, 2));
        assert_eq!(m.model("Isle").map(|x| x.triangle_count()), Some(12));
        assert!(m.model("Broken").is_none() && m.model("Absent").is_none());
        assert!(matches!(m.slots.get("Absent"), Some(Slot::Failed(e)) if e.contains("Absent.glb")));
        // a reset forgets all, and reads again when asked
        let rev = m.rev;
        m.reset();
        assert!(m.slots.is_empty() && m.rev > rev);
        m.want("Isle");
        settle(&mut m);
        assert_eq!(m.counts(), (1, 0, 0));
        // another root forgets what was read
        let other = root.join("elsewhere");
        std::fs::create_dir_all(&other).unwrap();
        m.set_root(&other);
        assert!(m.slots.is_empty() && m.docks == json!({}));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn feeding_the_view_reads_models_for_the_scenery_and_frames_each_map_once() {
        let root = std::env::temp_dir().join(format!("ope-view3d-feed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        put_glb(&root, "models/scenery/Isle.glb", &cube([0.0; 3], [1.0; 4]));
        let (map, scene) = plan();
        let mut v = View3d::default();
        let mut sel = Selection::new();
        sel.add(2);
        v.feed(&root, "red", &map, &scene, &sel, (Some(10.0), 500.0));
        assert_eq!((v.selected.clone(), v.grid, v.items.len(), v.items_rev), (vec![2], Some(10.0), 5, 1));
        assert_eq!(v.orbit, Orbit::fit(500.0));
        v.orbit.yaw = 1.0;
        v.feed(&root, "red", &map, &scene, &sel, (None, 500.0));
        assert_eq!((v.orbit.yaw, v.items_rev), (1.0, 1), "the same map is not framed again and unchanged items are not a change");
        v.feed(&root, "blue", &map, &scene, &sel, (None, 800.0));
        assert_eq!(v.orbit, Orbit::fit(800.0));
        settle(&mut v.models);
        assert_eq!(v.models.counts(), (1, 0, 0));
        assert_eq!(v.targets().iter().filter(|t| t.model.is_some()).count(), 1, "only the scenery with a loaded model is a mesh target");
        // the camera sits at `dist` from its target
        assert!((v.orbit.eye() - v.orbit.target).length() - v.orbit.dist < 1e-3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_canvas_records_the_pointer_in_its_rect_and_the_bar_above_it_is_known() {
        let ctx = egui::Context::default();
        let mut v = View3d::default();
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
        let mut full = ctx.run_ui(raw, |ui| {
            ui.label("toolbar");
            canvas(ui, &mut v);
        });
        full.textures_delta.clear();
        let input = v.input.expect("recorded");
        let (min, max) = input.base.rect.unwrap();
        assert!(min.y > 0.0 && max.x > 700.0 && max.y > 500.0, "{min:?} {max:?}");
        assert_eq!(v.canvas_top, Some(min.y));
        let area = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));
        assert_eq!(v.bar_rect(area).max.y, min.y);
        assert_eq!(View3d::default().bar_rect(area), area, "before the first frame the whole area is bar");
    }

    #[test]
    fn the_tab_in_3d_shares_its_selection_with_the_plan_and_a_height_is_one_undo_step() {
        use crate::map::project as maps;
        use crate::project::Project;
        use crate::project_panel;
        let root = std::env::temp_dir().join(format!("ope-view3d-tab-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (assets, modr) = (root.join("assets"), root.join("mod"));
        std::fs::create_dir_all(assets.join("maps/red")).unwrap();
        std::fs::create_dir_all(&modr).unwrap();
        let map = json!({"name": "Red", "world_half": 400.0, "objects": [
            {"type": "Isle", "id": 1, "pos": [-100.0, 50.0, 0.0], "scene": "Isle"},
            {"type": "Rock", "id": 2, "pos": [100.0, 50.0, 1.0], "scene": "Rock"}]});
        std::fs::write(assets.join("maps/red/map.json"), map.to_string()).unwrap();
        put_glb(&assets, "models/scenery/Isle.glb", &cube([0.0; 3], [1.0; 4]));
        let mut ps = project_panel::State::default();
        ps.project = Some(Project::open(&modr, Some(assets)).unwrap());
        let mut st = maps_panel::State::default();
        let ctx = egui::Context::default();
        let mut t = 0.0;
        let mut frame = |st: &mut maps_panel::State, ps: &mut project_panel::State| {
            t += 0.05;
            let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0))), time: Some(t), ..Default::default() };
            let mut full = ctx.run_ui(raw, |ui| maps_panel::tab(ui, st, ps));
            full.textures_delta.clear();
        };
        frame(&mut st, &mut ps);
        st.show_map(ps.project.as_mut().unwrap(), "red");
        st.v3.on = true;
        for _ in 0..3 {
            frame(&mut st, &mut ps);
        }
        assert!(st.v3.input.is_some(), "the canvas recorded the pointer");
        assert_eq!(st.v3.items.len(), 2);
        assert!(st.v3.selected.is_empty());
        // a click the Bevy side answered: marker 1 is selected, and the plan's selection is the one the view shows
        st.v3.click = Some(Click { hit: Some(1), shift: false, ctrl: false });
        frame(&mut st, &mut ps);
        frame(&mut st, &mut ps);
        assert_eq!(st.v3.selected, vec![1]);
        // shift adds, ctrl toggles, empty space clears
        st.v3.click = Some(Click { hit: Some(0), shift: true, ctrl: false });
        frame(&mut st, &mut ps);
        frame(&mut st, &mut ps);
        assert_eq!(st.v3.selected, vec![0, 1]);
        st.v3.click = Some(Click { hit: Some(0), shift: false, ctrl: true });
        frame(&mut st, &mut ps);
        frame(&mut st, &mut ps);
        assert_eq!(st.v3.selected, vec![1]);
        // the height handle's answer: one step, labelled, both entries when both are selected
        st.v3.click = Some(Click { hit: Some(0), shift: true, ctrl: false });
        frame(&mut st, &mut ps);
        st.v3.height = Some(3.0);
        frame(&mut st, &mut ps);
        frame(&mut st, &mut ps);
        let project = ps.project.as_mut().unwrap();
        let now = maps::merged_view(project, "red").unwrap();
        assert_eq!((doc::pos(&doc::objects(now)[0]), doc::pos(&doc::objects(now)[1])), (Some([-100.0, 50.0, 3.0]), Some([100.0, 50.0, 4.0])));
        assert_eq!(project.undo_label(), Some("Set height of 2 entries"));
        project.undo();
        let back = maps::merged_view(project, "red").unwrap();
        assert_eq!((doc::pos(&doc::objects(back)[0]), doc::pos(&doc::objects(back)[1])), (Some([-100.0, 50.0, 0.0]), Some([100.0, 50.0, 1.0])), "one undo restores both");
        assert!(st.v3.models.counts().0 + st.v3.models.counts().1 + st.v3.models.counts().2 >= 2, "both scenes were asked for");
        let _ = std::fs::remove_dir_all(&root);
    }
}
