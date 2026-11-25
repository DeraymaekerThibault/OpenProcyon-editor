//! Dock points and crew places of a mod's OWN scenery types, placed on the 3D view of the scenery (docs/phase4.md P9, work item T12).
//! A point is written to the mod's `data/docks.json` (`{"<type>": {"points": [{label, pos, yaw}]}}`) or `data/crew_nodes.json`
//! (`{"<type>": [{name, pos, rot?}]}`), never into the GLB (docs/roadmap.md principle 7); each drag, add and delete is one undo step of
//! the project. A retail type's points are shown and cannot be changed.
//!
//! What the engine does with these files (the editor calls it, it does not restate it; pinned engine f91542f):
//!
//! - docks: `docking::load_docks` reads `data/docks.json` (game/src/docking.rs:114-125) and `docking::dockable_for` turns one entry
//!   into the entity's points (docking.rs:127-141): `pos` is a game-axes triple (`game_to_bevy`, spawn.rs:96) in the scenery's own
//!   frame, `yaw` (radians, default 0) turns the node's forward `-z` about Bevy y, `label` ends in the number that picks the base's
//!   crane clip (`DockPoint3` is 3), `layer` is optional and not edited here. A non-`Ship_` type is always dockable; `tender` only
//!   counts for ships. The spawn takes each object's `type` as the key (game/src/scenario/spawn.rs:109 and :122).
//! - placement: `authoring::scenery_transform` puts the model so the FIRST dock point's height (game z) lies at the
//!   object's `pos` z unless the object has a `scene_origin`: moving that point's z moves the object. [`super::view3d::placement`]
//!   is that call, fed the edited docks, so the view shows what the engine will do.
//! - crew: `data::crew_nodes::load_scenery` reads `data/crew_nodes.json` by scenery type (game/src/data/crew_nodes.rs:69-91);
//!   `render/deckhands.rs:742-790` (`gather`) combines an entry with the model's own `Crew_*` nodes, a JSON node replacing a model node
//!   of its name. A node is `name` (`Crew_<Walk|Gun|Post|Work|Climb|Count>_...`, checked by `crew_nodes::crew_kind`), `pos` (game axes,
//!   the same frame as a dock) and an optional `rot` (kept as it is when a node is moved).
//!
//! PROVISIONAL: a new dock entry gets no `tender` and no `layer` (the engine reads `tender` for ships only); the crew `rot` of a new
//! place is left out (the engine then faces it along the model's forward, not verified); "a mod's own type" means no retail map uses
//! it and neither the import's `docks.json`, `crew_nodes.json` nor `collision.json` has it (the engine has no list of types,
//! docs/phase4.md requirement 3). Collision is not edited: no engine function derives `data/collision.json` from a modder's GLB
//! (docs/phase4.md requirement 6).
//!
//! The pure part (reading points, the edits, the retail test, applying to a project) is tested headless; the egui panel and the two
//! Bevy systems at the bottom are thin and answer through [`State`].

use super::catalogue;
use super::glb::Model;
use super::view3d::{self, Click, Item, ItemKind, MapGizmos, View3d};
use crate::edit::{Edit, pointer};
use crate::maps_panel;
use crate::project::Project;
use crate::ship_view::coords::{pos_value, read_triple, to_bevy, to_game};
use crate::ship_view::pick::axis_drag;
use crate::ship_view::scene::distance_to_segment;
use bevy::prelude::*;
use bevy_egui::egui;
use serde_json::{Value, json};
use std::sync::Arc;

pub const DOCKS: &str = "data/docks.json";
pub const CREW: &str = "data/crew_nodes.json";
pub const COLLISION: &str = "data/collision.json";

/// Shown beside every scenery type: collision is not edited here.
pub const COLLISION_NOTE: &str =
    "Collision editing is not supported yet: no engine function derives data/collision.json from a model. An object with no collision entry is only a picture.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dock,
    Crew,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Dock => "dock point",
            Kind::Crew => "crew place",
        }
    }
}

/// One point of a scenery type.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub kind: Kind,
    /// Index in the type's list (`points[i]` of docks, `[i]` of crew nodes).
    pub index: usize,
    /// A dock's `label`, a crew place's `name`.
    pub label: String,
    /// Game axes, in the scenery's own frame.
    pub pos: [f64; 3],
    /// A dock's `yaw` in radians.
    pub yaw: Option<f64>,
}

/// The points of scenery type `ty`: docks first, then crew places, in file order. Entries without a readable `pos` are skipped (the
/// engine's check reports them).
pub fn points(docks: &Value, crew: &Value, ty: &str) -> Vec<Point> {
    let mut out = vec![];
    for (i, p) in docks.get(ty).and_then(|d| d.get("points")).and_then(Value::as_array).into_iter().flatten().enumerate() {
        if let Some(pos) = p.get("pos").and_then(read_triple) {
            let label = p.get("label").and_then(Value::as_str).unwrap_or("").to_string();
            out.push(Point { kind: Kind::Dock, index: i, label, pos, yaw: Some(p.get("yaw").and_then(Value::as_f64).unwrap_or(0.0)) });
        }
    }
    for (i, n) in crew.get(ty).and_then(Value::as_array).into_iter().flatten().enumerate() {
        if let Some(pos) = n.get("pos").and_then(read_triple) {
            out.push(Point { kind: Kind::Crew, index: i, label: n.get("name").and_then(Value::as_str).unwrap_or("").to_string(), pos, yaw: None });
        }
    }
    out
}

/// `DockPoint<n>` with the next free number (the number picks the base's crane clip, so a new point continues the count).
pub fn next_dock_label(docks: &Value, ty: &str) -> String {
    let max = docks
        .get(ty)
        .and_then(|d| d.get("points"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| p.get("label").and_then(Value::as_str))
        .filter_map(|l| l.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("DockPoint{}", max + 1)
}

/// `Crew_<kind>_<suffix>`, checked by the engine's own `crew_kind`. The suffix may not be empty.
pub fn crew_name(kind: &str, suffix: &str) -> Result<String, String> {
    let suffix = suffix.trim();
    if suffix.is_empty() {
        return Err("give the crew place a name suffix (for example P_0)".into());
    }
    let name = format!("Crew_{kind}_{suffix}");
    if openprocyon::data::crew_nodes::crew_kind(&name).is_none() {
        return Err(format!("{name} is not a crew node name"));
    }
    Ok(name)
}

// ---- edits ----

fn at(ty: &str, rest: &[&str]) -> String {
    let mut t = vec![ty];
    t.extend_from_slice(rest);
    pointer(&t)
}

/// Moves a point to `pos` (game axes): a `set` of its `pos`, nothing else (a crew place keeps its `rot`, a dock its `yaw`).
pub fn move_edits(kind: Kind, ty: &str, index: usize, pos: [f64; 3]) -> Result<Vec<Edit>, String> {
    if !pos.iter().all(|v| v.is_finite()) {
        return Err("the position must be finite numbers".into());
    }
    let i = index.to_string();
    Ok(vec![Edit::set(
        match kind {
            Kind::Dock => at(ty, &["points", &i, "pos"]),
            Kind::Crew => at(ty, &[&i, "pos"]),
        },
        pos_value(pos),
    )])
}

/// Sets a dock's `yaw` (radians).
pub fn yaw_edits(ty: &str, index: usize, yaw: f64) -> Result<Vec<Edit>, String> {
    if !yaw.is_finite() {
        return Err("the yaw must be a finite number".into());
    }
    Ok(vec![Edit::set(at(ty, &["points", &index.to_string(), "yaw"]), crate::map_view::short(yaw as f32))])
}

/// Appends a dock point to `ty`'s entry of `docks` (the entry and its `points` are made when missing). Returns the edits and the new
/// point's index.
pub fn add_dock_edits(docks: &Value, ty: &str, label: &str, pos: [f64; 3], yaw: f64) -> Result<(Vec<Edit>, usize), String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("give the dock point a label".into());
    }
    let list = docks.get(ty).and_then(|d| d.get("points")).and_then(Value::as_array);
    if list.is_some_and(|l| l.iter().any(|p| p.get("label").and_then(Value::as_str) == Some(label))) {
        return Err(format!("{ty} has a dock point {label} already"));
    }
    let entry = json!({"label": label, "pos": pos_value(pos), "yaw": crate::map_view::short(yaw as f32)});
    Ok(match (docks.get(ty), list) {
        (_, Some(l)) => (vec![Edit::set(at(ty, &["points", &l.len().to_string()]), entry)], l.len()),
        (Some(d), None) if d.is_object() => (vec![Edit::set(at(ty, &["points"]), json!([entry]))], 0),
        _ => (vec![Edit::set(at(ty, &[]), json!({"points": [entry]}))], 0),
    })
}

/// Appends a crew place to `ty`'s list of `crew`. Returns the edits and the new place's index.
pub fn add_crew_edits(crew: &Value, ty: &str, name: &str, pos: [f64; 3]) -> Result<(Vec<Edit>, usize), String> {
    if openprocyon::data::crew_nodes::crew_kind(name).is_none() {
        return Err(format!("{name} is not a crew node name"));
    }
    let list = crew.get(ty).and_then(Value::as_array);
    if list.is_some_and(|l| l.iter().any(|n| n.get("name").and_then(Value::as_str) == Some(name))) {
        return Err(format!("{ty} has a crew place {name} already"));
    }
    let entry = json!({"name": name, "pos": pos_value(pos)});
    Ok(match list {
        Some(l) => (vec![Edit::set(at(ty, &[&l.len().to_string()]), entry)], l.len()),
        None => (vec![Edit::set(at(ty, &[]), json!([entry]))], 0),
    })
}

pub fn delete_edits(kind: Kind, ty: &str, index: usize) -> Vec<Edit> {
    let i = index.to_string();
    vec![Edit::remove(match kind {
        Kind::Dock => at(ty, &["points", &i]),
        Kind::Crew => at(ty, &[&i]),
    })]
}

// ---- own or retail ----

/// Why the type's points cannot be changed: it is a retail type (a retail map uses it, or the import's `docks.json`,
/// `crew_nodes.json` or `collision.json` has an entry for it). `None` for a type of the mod's own. PROVISIONAL, see the module notes.
pub fn retail_reason(project: &Project, ty: &str) -> Option<String> {
    let import = project.import.as_ref()?;
    if catalogue::scan(import).find(ty).is_some() {
        return Some(format!("{ty} is a retail scenery type (a retail map uses it)"));
    }
    // `base` of a document is what the import has at that path; the three are opened by [`ensure_open`]
    [DOCKS, CREW, COLLISION].into_iter().find(|rel| project.doc(rel).is_some_and(|d| d.base.get(ty).is_some())).map(|rel| format!("{ty} has an entry in the retail {rel}"))
}

/// Opens the three data files this mode reads (nothing is written by opening). Called before the map is read for the frame, because
/// opening takes the project mutably.
pub fn ensure_open(project: &mut Project, on: bool) {
    if !on {
        return;
    }
    for rel in [DOCKS, CREW, COLLISION] {
        let _ = project.open_doc(rel);
    }
}

// ---- applying to a project ----

/// What the view or the panel asks the project to write; each is one undo step.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Move { kind: Kind, index: usize, pos: [f64; 3] },
    Yaw { index: usize, yaw: f64 },
    AddDock { pos: [f64; 3] },
    AddCrew { name: String, pos: [f64; 3] },
    Delete { kind: Kind, index: usize },
}

fn edited(project: &mut Project, rel: &str) -> Result<Value, String> {
    let i = project.open_doc(rel)?;
    Ok(project.docs[i].edited.clone())
}

/// Writes `action` for scenery type `ty` as one undo step of the right file and returns what was done, plus the point to select
/// afterwards. Refused for a retail type.
pub fn write(project: &mut Project, ty: &str, action: &Action) -> Result<(String, Option<(Kind, usize)>), String> {
    ensure_open(project, true);
    if let Some(why) = retail_reason(project, ty) {
        return Err(format!("{why}: its points are read-only here"));
    }
    let file = |k: Kind| if k == Kind::Dock { DOCKS } else { CREW };
    let (rel, label, edits, select) = match action {
        Action::Move { kind, index, pos } => (file(*kind), format!("Move {} {index} of {ty}", kind.word()), move_edits(*kind, ty, *index, *pos)?, Some((*kind, *index))),
        Action::Yaw { index, yaw } => (DOCKS, format!("Turn dock point {index} of {ty}"), yaw_edits(ty, *index, *yaw)?, Some((Kind::Dock, *index))),
        Action::AddDock { pos } => {
            let docks = edited(project, DOCKS)?;
            let label = next_dock_label(&docks, ty);
            let (e, i) = add_dock_edits(&docks, ty, &label, *pos, 0.0)?;
            (DOCKS, format!("Add dock point {label} to {ty}"), e, Some((Kind::Dock, i)))
        }
        Action::AddCrew { name, pos } => {
            let (e, i) = add_crew_edits(&edited(project, CREW)?, ty, name, *pos)?;
            (CREW, format!("Add crew place {name} to {ty}"), e, Some((Kind::Crew, i)))
        }
        Action::Delete { kind, index } => (file(*kind), format!("Delete {} {index} of {ty}", kind.word()), delete_edits(*kind, ty, *index), None),
    };
    project.do_step(rel, &label, edits)?;
    Ok((label, select))
}

// ---- the panel's and the view's state ----

/// A drag of one axis arrow of the selected point: nothing is written until it is released.
#[derive(Clone, Copy, Debug)]
pub struct PDrag {
    pub kind: Kind,
    pub index: usize,
    /// 0, 1, 2: game x, y, z.
    pub axis: usize,
    pub start_cursor: Vec2,
    pub start: [f64; 3],
}

/// Everything the scenery-points mode keeps. A field of the Maps tab's state; [`feed`] fills it, [`show`] and the systems answer.
#[derive(Default)]
pub struct State {
    /// The mode is switched on (the 3D toolbar's check box, `--points`).
    pub on: bool,
    /// The scenery type shown: set while exactly one scenery object is selected in the 3D view.
    pub ty: Option<String>,
    /// Marker number of that object.
    pub marker: Option<usize>,
    pub frame: Transform,
    pub model: Option<Arc<Model>>,
    pub points: Vec<Point>,
    pub sel: Option<(Kind, usize)>,
    /// Why the points cannot be changed (a retail type); `None` for a type of the mod's own.
    pub retail: Option<String>,
    retail_for: Option<String>,
    /// A click on the model adds a point of this kind.
    pub arm: Option<Kind>,
    pub crew_kind: usize,
    pub crew_suffix: String,
    pub drag: Option<PDrag>,
    /// The position the dragged point shows now.
    pub preview: Option<(Kind, usize, [f64; 3])>,
    pub action: Option<Action>,
    pub note: String,
    pub has_collision: bool,
    /// 0: not framed; 1: framed on the object; 2: framed once the model was known (the camera goes to the object when it is first shown).
    framed: u8,
    yaw_edit: f64,
    yaw_for: Option<(String, usize)>,
}

impl State {
    /// Points mode with a scenery object to show: the view's own click, height arrow and hover step aside.
    pub fn active(&self) -> bool {
        self.on && self.ty.is_some()
    }

    fn shown(&self, p: &Point) -> [f64; 3] {
        match self.preview {
            Some((k, i, pos)) if k == p.kind && i == p.index => pos,
            _ => p.pos,
        }
    }

    fn world(&self, pos: [f64; 3]) -> Vec3 {
        self.frame.transform_point(to_bevy(pos))
    }
}

/// Called each frame the 3D view is shown, after [`View3d::feed`]: finds the one selected scenery object, reads its points from the
/// project's `data/docks.json` and `data/crew_nodes.json` (so an edit shows at once) and gives the placement the edited docks.
pub fn feed(t: &mut State, project: &Project, map: &Value, scene: &super::view2d::Scene, v3: &mut View3d) {
    t.ty = None;
    t.marker = None;
    let one = match v3.selected.as_slice() {
        [m] => v3.items.iter().find(|i: &&Item| i.marker == *m && i.kind == ItemKind::Scenery),
        _ => None,
    };
    let ty = one.and_then(|i| scene.refs.get(i.marker)).and_then(|r| super::doc::array_entries(map, r.array()).get(r.index)).and_then(super::doc::kind).map(str::to_string);
    let (Some(item), Some(ty), true) = (one, ty, t.on) else {
        t.drag = None;
        t.preview = None;
        t.arm = None;
        return;
    };
    let (Some(docks), Some(crew), Some(collision)) = (project.doc(DOCKS), project.doc(CREW), project.doc(COLLISION)) else {
        t.note = "data/docks.json or data/crew_nodes.json cannot be read".into();
        return;
    };
    let (docks, crew) = (&docks.edited, &crew.edited);
    if t.retail_for.as_deref() != Some(ty.as_str()) {
        t.retail_for = Some(ty.clone());
        t.retail = retail_reason(project, &ty);
        t.sel = None;
        t.drag = None;
        t.preview = None;
        t.arm = None;
        t.framed = 0;
    }
    t.points = points(docks, crew, &ty);
    t.has_collision = collision.edited.get(&ty).is_some();
    if t.sel.is_some_and(|(k, i)| !t.points.iter().any(|p| p.kind == k && p.index == i)) {
        t.sel = None;
    }
    // The placement of the object follows the edited docks (the first point's height decides it).
    if v3.models.docks != *docks && docks.is_object() {
        v3.models.docks = docks.clone();
    }
    t.frame = item.tf;
    t.model = item.scene.as_deref().and_then(|s| v3.models.model(s)).cloned();
    t.marker = Some(item.marker);
    let known = t.model.is_some() as u8 + 1;
    if t.framed < known {
        t.framed = known;
        let size = t.model.as_ref().map_or(40.0, |m| (m.aabb[1] - m.aabb[0]).length());
        v3.orbit.target = item.tf.translation;
        v3.orbit.dist = (size * 1.4).clamp(20.0, 2000.0);
    }
    t.ty = Some(ty);
}

/// Does what the view asked: writes it and selects the new point. The message goes to `Ok`/`Err`.
pub fn apply(project: &mut Project, t: &mut State) -> Option<Result<String, String>> {
    let action = t.action.take()?;
    let ty = t.ty.clone()?;
    Some(write(project, &ty, &action).map(|(label, select)| {
        t.sel = select;
        t.arm = None;
        t.yaw_for = None;
        label
    }))
}

// ---- egui ----

/// The toolbar's check box.
pub fn toolbar(ui: &mut egui::Ui, t: &mut State) {
    ui.checkbox(&mut t.on, "Scenery points").on_hover_text("Show and place the dock points and crew places of the one selected scenery object");
}

/// The panel above the canvas: what the selected type is, the add buttons, the point list and the selected point's fields.
pub fn show(ui: &mut egui::Ui, t: &mut State) {
    let Some(ty) = t.ty.clone() else {
        if t.on {
            ui.label("Scenery points: select one scenery object (click it in the 3D view or the plan).");
        }
        return;
    };
    let own = t.retail.is_none();
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("Scenery points of {ty}"));
        match &t.retail {
            Some(why) => ui.colored_label(egui::Color32::YELLOW, format!("{why}: points shown read-only")),
            None => ui.label("a type of this mod"),
        };
    });
    ui.label(format!("{}{}", if t.has_collision { "It has a collision entry. " } else { "" }, COLLISION_NOTE));
    if own {
        ui.horizontal_wrapped(|ui| {
            let armed = t.arm == Some(Kind::Dock);
            if ui.selectable_label(armed, "Add dock point").on_hover_text("Then click the model: the point lands where the click hits it").clicked() {
                t.arm = if armed { None } else { Some(Kind::Dock) };
            }
            ui.separator();
            egui::ComboBox::from_id_salt("t12-crew-kind").selected_text(openprocyon::data::crew_nodes::CREW_KINDS[t.crew_kind]).show_ui(ui, |ui| {
                for (i, k) in openprocyon::data::crew_nodes::CREW_KINDS.iter().enumerate() {
                    ui.selectable_value(&mut t.crew_kind, i, *k);
                }
            });
            ui.add(egui::TextEdit::singleline(&mut t.crew_suffix).desired_width(60.0).hint_text("P_0"));
            let armed = t.arm == Some(Kind::Crew);
            if ui.selectable_label(armed, "Add crew place").on_hover_text("Crew_<kind>_<suffix>; then click the model").clicked() {
                t.arm = if armed { None } else { Some(Kind::Crew) };
            }
            if t.arm.is_some() {
                ui.label("Click the model to place it.");
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        for p in t.points.clone() {
            let text = format!("{} {}", if p.kind == Kind::Dock { "dock" } else { "crew" }, p.label);
            if ui.selectable_label(t.sel == Some((p.kind, p.index)), text).clicked() {
                t.sel = Some((p.kind, p.index));
            }
        }
        if t.points.is_empty() {
            ui.label("no dock points or crew places yet");
        }
    });
    if let Some(p) = t.sel.and_then(|(k, i)| t.points.iter().find(|p| p.kind == k && p.index == i)).cloned() {
        ui.horizontal_wrapped(|ui| {
            let pos = t.shown(&p);
            ui.label(format!("{}: x {:.2}  y {:.2}  z {:.2}", p.label, pos[0], pos[1], pos[2]));
            if let Some(yaw) = p.yaw {
                if t.yaw_for != Some((ty.clone(), p.index)) {
                    t.yaw_for = Some((ty.clone(), p.index));
                    t.yaw_edit = yaw;
                }
                let r = ui.add_enabled(own, egui::DragValue::new(&mut t.yaw_edit).update_while_editing(false).speed(0.01).prefix("yaw ").suffix(" rad"));
                if (r.drag_stopped() || r.lost_focus()) && (t.yaw_edit - yaw).abs() > 1e-6 {
                    t.action = Some(Action::Yaw { index: p.index, yaw: t.yaw_edit });
                }
            }
            if own && ui.button("Delete point").clicked() {
                t.action = Some(Action::Delete { kind: p.kind, index: p.index });
            }
        });
        if own && p.kind == Kind::Dock && p.index == 0 {
            ui.label("The first dock point's height places the object: the engine sets it at the object's z (unless it has a scene_origin).");
        }
    }
    if !t.note.is_empty() {
        ui.label(&t.note);
    }
}

// ---- Bevy ----

/// Registers the systems; called from [`view3d::plugin`].
pub fn plugin(app: &mut App) {
    app.add_systems(Update, (interact, draw).chain().after(view3d::camera));
}

/// The local axes of the game frame as Bevy vectors, with the colours the ship view uses.
const AXES: [(Vec3, Color); 3] = [(Vec3::X, Color::srgb(1.0, 0.25, 0.25)), (Vec3::NEG_Z, Color::srgb(0.3, 0.9, 0.3)), (Vec3::Y, Color::srgb(0.3, 0.5, 1.0))];

fn snap(v: f64, grid: Option<f32>) -> f64 {
    match grid.filter(|g| *g > 0.0) {
        Some(g) => (v / f64::from(g)).round() * f64::from(g),
        None => v,
    }
}

fn interact(mut st: ResMut<maps_panel::State>, cam: Single<(&Camera, &GlobalTransform), With<view3d::MapCamera>>) {
    let st = &mut *st;
    let (t, v3) = (&mut st.t12, &mut st.v3);
    let Some(input) = v3.cur.filter(|_| t.active()) else {
        t.drag = None;
        t.preview = None;
        return;
    };
    let (camera, gt) = *cam;
    let cursor = input.base.hover;
    let project = |p: Vec3| camera.world_to_viewport(gt, p).unwrap_or(Vec2::splat(f32::MAX / 4.0));
    let own = t.retail.is_none();

    if let Some(d) = t.drag {
        if let Some(c) = cursor {
            let dir = t.frame.rotation * AXES[d.axis].0;
            let dist = axis_drag(dir, d.start_cursor, c, project, t.world(d.start));
            let mut p = d.start;
            p[d.axis] = snap(p[d.axis] + f64::from(dist), v3.grid);
            t.preview = Some((d.kind, d.index, p));
        }
        if input.base.primary_released || !input.base.primary_down {
            t.drag = None;
            if let Some((kind, index, pos)) = t.preview.take().filter(|(_, _, p)| *p != d.start) {
                t.action = Some(Action::Move { kind, index, pos });
            }
        }
        return;
    }
    t.preview = None;
    let ray = cursor.and_then(|c| camera.viewport_to_world(gt, c).ok());
    let Some(ray) = ray else { return };
    let (origin, dir) = (ray.origin, *ray.direction);
    let near = |t: &State| -> Option<(Kind, usize)> {
        t.points.iter().filter_map(|p| view3d::point_hit(origin, dir, t.world(t.shown(p))).map(|d| (p.kind, p.index, d))).min_by(|a, b| a.2.total_cmp(&b.2)).map(|(k, i, _)| (k, i))
    };
    if !input.base.primary_pressed {
        return;
    }
    let Some(c) = cursor else { return };
    let len = v3.orbit.dist * 0.12;
    // the arrows of the selected point first
    if own && let Some(p) = t.sel.and_then(|(k, i)| t.points.iter().find(|p| p.kind == k && p.index == i)) {
        let anchor = t.world(p.pos);
        let axis = (0..3).find(|a| distance_to_segment(c, project(anchor), project(anchor + t.frame.rotation * AXES[*a].0 * len)) < 9.0);
        if let Some(axis) = axis {
            t.drag = Some(PDrag { kind: p.kind, index: p.index, axis, start_cursor: c, start: p.pos });
            return;
        }
    }
    if let Some(kind) = t.arm.filter(|_| own) {
        let hit = t.model.as_ref().and_then(|m| view3d::ray_model(origin, dir.normalize_or_zero(), &t.frame, m));
        let Some(d) = hit else {
            t.note = "Click on the model to place the point.".into();
            return;
        };
        let local = t.frame.to_matrix().inverse().transform_point3(origin + dir.normalize_or_zero() * d);
        let pos = to_game(local).map(|v| f64::from(v as f32));
        t.action = Some(match kind {
            Kind::Dock => Action::AddDock { pos },
            Kind::Crew => match crew_name(openprocyon::data::crew_nodes::CREW_KINDS[t.crew_kind], &t.crew_suffix) {
                Ok(name) => Action::AddCrew { name, pos },
                Err(e) => {
                    t.note = e;
                    return;
                }
            },
        });
        t.note.clear();
        return;
    }
    if let Some(sel) = near(t) {
        t.sel = Some(sel);
        return;
    }
    // anything else is the view's own click: select (or clear) an object
    let hit = view3d::pick(origin, dir, &v3.targets(), true);
    v3.click = Some(Click { hit, shift: input.shift, ctrl: input.ctrl });
}

fn draw(mut gizmos: Gizmos<MapGizmos>, st: Res<maps_panel::State>, cam: Single<&Transform, With<view3d::MapCamera>>) {
    let t = &st.t12;
    if !t.active() || st.v3.cur.is_none() {
        return;
    }
    let eye = cam.translation;
    let len = st.v3.orbit.dist * 0.12;
    for p in &t.points {
        let pos = t.shown(p);
        let w = t.world(pos);
        let selected = t.sel == Some((p.kind, p.index));
        let color = match (selected, p.kind, t.retail.is_some()) {
            (true, _, _) => Color::srgb(1.0, 0.85, 0.2),
            (_, Kind::Dock, false) => Color::srgb(0.2, 0.9, 0.95),
            (_, Kind::Dock, true) => Color::srgb(0.2, 0.55, 0.6),
            (_, Kind::Crew, false) => Color::srgb(0.95, 0.4, 0.9),
            (_, Kind::Crew, true) => Color::srgb(0.6, 0.3, 0.6),
        };
        let r = view3d::marker_radius(eye.distance(w)) * 0.8;
        gizmos.sphere(Isometry3d::from_translation(w), r, color).resolution(10);
        if let Some(yaw) = p.yaw {
            let fwd = t.frame.rotation * (Quat::from_rotation_y(yaw as f32) * Vec3::NEG_Z);
            gizmos.arrow(w, w + fwd * r * 4.0, color);
        }
        if selected && t.retail.is_none() {
            for (axis, c) in AXES {
                gizmos.arrow(w, w + t.frame.rotation * axis * len, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply as apply_edit;

    fn run(doc: &mut Value, edits: &[Edit]) {
        for e in edits {
            apply_edit(doc, e).unwrap();
        }
    }

    #[test]
    fn points_are_read_from_both_files_in_order() {
        let docks = json!({"Fort": {"points": [{"label": "DockPoint1", "pos": [1, 2, 3], "yaw": 0.5}, {"label": "DockPoint2"}]}, "Other": {"points": []}});
        let crew = json!({"Fort": [{"name": "Crew_Walk_P_0", "pos": [0, 1, 0], "rot": [0, 0, 1]}]});
        let p = points(&docks, &crew, "Fort");
        assert_eq!(p.len(), 2, "the dock without a pos is skipped");
        assert_eq!((p[0].kind, p[0].index, p[0].pos, p[0].yaw), (Kind::Dock, 0, [1.0, 2.0, 3.0], Some(0.5)));
        assert_eq!((p[1].kind, p[1].label.as_str(), p[1].yaw), (Kind::Crew, "Crew_Walk_P_0", None));
        assert!(points(&docks, &crew, "Nothing").is_empty());
        assert!(points(&Value::Null, &Value::Null, "Fort").is_empty());
    }

    #[test]
    fn dock_labels_continue_the_numbering() {
        assert_eq!(next_dock_label(&Value::Null, "T"), "DockPoint1");
        assert_eq!(next_dock_label(&json!({"T": {"points": [{"label": "DockPoint1"}, {"label": "Berth7"}]}}), "T"), "DockPoint8");
    }

    #[test]
    fn adding_a_dock_makes_the_entry_the_list_or_appends() {
        let mut docks = Value::Null;
        let (e, i) = add_dock_edits(&docks, "My/Type", "DockPoint1", [1.0, 2.0, 3.0], 0.0).unwrap();
        run(&mut docks, &e);
        assert_eq!(i, 0);
        assert_eq!(docks, json!({"My/Type": {"points": [{"label": "DockPoint1", "pos": [1.0, 2.0, 3.0], "yaw": 0.0}]}}));
        let (e, i) = add_dock_edits(&docks, "My/Type", "DockPoint2", [4.0, 5.0, 6.0], 1.5).unwrap();
        run(&mut docks, &e);
        assert_eq!((i, docks["My/Type"]["points"][1]["yaw"].clone()), (1, json!(1.5)));
        assert!(add_dock_edits(&docks, "My/Type", "DockPoint2", [0.0; 3], 0.0).unwrap_err().contains("already"));
        assert!(add_dock_edits(&docks, "T", " ", [0.0; 3], 0.0).is_err());
        let mut bare = json!({"T": {"tender": true}});
        let (e, _) = add_dock_edits(&bare, "T", "A1", [0.0; 3], 0.0).unwrap();
        run(&mut bare, &e);
        assert_eq!((bare["T"]["tender"].clone(), bare["T"]["points"].as_array().map(Vec::len)), (json!(true), Some(1)));
    }

    #[test]
    fn crew_places_are_checked_by_the_engine_and_keep_their_rot_when_moved() {
        assert_eq!(crew_name("Walk", " P_0 ").unwrap(), "Crew_Walk_P_0");
        assert!(crew_name("Walk", "").is_err());
        assert!(crew_name("Dance", "x").is_err());
        let mut crew = Value::Null;
        let (e, i) = add_crew_edits(&crew, "T", "Crew_Walk_P_0", [0.0, 1.0, 0.0]).unwrap();
        run(&mut crew, &e);
        assert_eq!(i, 0);
        crew["T"][0]["rot"] = json!([0, 0, 1]);
        run(&mut crew, &move_edits(Kind::Crew, "T", 0, [2.0, 3.0, 4.0]).unwrap());
        assert_eq!(crew, json!({"T": [{"name": "Crew_Walk_P_0", "pos": [2.0, 3.0, 4.0], "rot": [0, 0, 1]}]}));
        assert!(add_crew_edits(&crew, "T", "Crew_Walk_P_0", [0.0; 3]).unwrap_err().contains("already"));
        assert!(add_crew_edits(&crew, "T", "Walker", [0.0; 3]).is_err());
        run(&mut crew, &delete_edits(Kind::Crew, "T", 0));
        assert_eq!(crew, json!({"T": []}));
    }

    #[test]
    fn moves_write_the_short_f32_decimals_and_refuse_nan() {
        let e = move_edits(Kind::Dock, "T", 1, [0.1f32 as f64, 2.0, f64::from(3.3f32)]).unwrap();
        assert_eq!(e[0].pointer, "/T/points/1/pos");
        assert_eq!(e[0].new, Some(json!([0.1, 2.0, 3.3])));
        assert!(move_edits(Kind::Dock, "T", 1, [f64::NAN, 0.0, 0.0]).is_err());
        assert!(yaw_edits("T", 0, f64::INFINITY).is_err());
        assert_eq!(snap(2.4, Some(0.5)), 2.5);
        assert_eq!(snap(2.4, None), 2.4);
    }
}
