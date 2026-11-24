//! The Bevy side of the 3D ship view: the model's layers as flat-shaded meshes built from the engine's hit triangles (`Facts`), an
//! orbit camera that draws only in the central rect, hardpoint gizmos, and the mouse logic that selects, drags and adds points.
//! Thin on purpose: every decision that can be tested without a window is in `points`, `place` and `pick`.
//!
//! PROVISIONAL: the textured GLB is not loaded (no asset source for an arbitrary folder yet); the hit triangles are what the engine
//! uses to resolve hits, so they are the geometry placement needs.

use super::coords::{facing, to_bevy};
use super::pick::{axis_drag, pick_point, ray_hit};
use super::place::{add_point, move_point};
use super::points::{CrewKind, Hardpoint, Kind, Source};
use super::{ShipView, ViewInput};
use bevy::asset::RenderAssetUsages;
use bevy::camera::Viewport;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use openprocyon::data::derive::{Facts, Layer};

/// Registers the resources and systems of the view. The `Camera3d` draws below egui's camera (lower `order`), so egui stays on top.
pub fn plugin(app: &mut App) {
    app.init_resource::<super::ShipView>()
        .init_resource::<Orbit>()
        .init_resource::<Dragging>()
        .insert_resource(GlobalAmbientLight { brightness: 350.0, ..default() })
        .add_systems(Startup, setup)
        .add_systems(Update, (rebuild_meshes, camera, interact, highlight, draw).chain());
}

/// A hull layer's mesh entity.
#[derive(Component)]
struct HullMesh {
    layer: usize,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
struct ShipCamera;

/// The orbit camera's state: spherical coordinates around `target` (Bevy axes).
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: Vec3,
    seen: u64,
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit { yaw: -0.6, pitch: 0.45, dist: 40.0, target: Vec3::ZERO, seen: 0 }
    }
}

/// A handle drag in progress. `current` is where the point would land; nothing is edited until the button is released.
struct Drag {
    hp: Hardpoint,
    axis: usize,
    start_cursor: Vec2,
    start: [f64; 3],
    current: [f64; 3],
}

#[derive(Resource, Default)]
struct Dragging(Option<Drag>);

/// The game axes as Bevy vectors (x starboard, y forward, z up) and their colours.
const AXES: [(Vec3, Color); 3] = [(Vec3::X, Color::srgb(0.95, 0.25, 0.25)), (Vec3::NEG_Z, Color::srgb(0.3, 0.9, 0.3)), (Vec3::Y, Color::srgb(0.3, 0.5, 1.0))];

/// Flat-shaded triangle soup of a layer in Bevy axes: positions and one normal per vertex. Winding: the engine's (a, c, b) is
/// handled by the `conv` axis change, which flips handedness, so the triangle is emitted (a, c, b).
pub fn layer_mesh(layer: &Layer) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let (mut pos, mut nor) = (vec![], vec![]);
    for t in layer.tris.as_chunks::<9>().0 {
        let v = |i: usize| to_bevy([t[i], t[i + 1], t[i + 2]]);
        let (a, b, c) = (v(0), v(3), v(6));
        let n = (c - a).cross(b - a).normalize_or_zero();
        for p in [a, c, b] {
            pos.push(p.to_array());
            nor.push(n.to_array());
        }
    }
    (pos, nor)
}

/// The colour of a point by kind (crew places by crew kind), as sRGB.
pub fn kind_color(kind: Kind) -> [f32; 3] {
    match kind {
        Kind::Gun => [1.0, 0.45, 0.1],
        Kind::Engine => [1.0, 0.85, 0.2],
        Kind::Wake => [0.3, 0.8, 1.0],
        Kind::Flag => [0.9, 0.3, 0.9],
        Kind::TowTower | Kind::TowTowee => [0.6, 0.9, 0.3],
        Kind::Homing => [1.0, 0.2, 0.2],
        Kind::Boarding => [0.75, 0.75, 0.75],
        Kind::Crew(CrewKind::Walk) => [0.5, 0.7, 0.5],
        Kind::Crew(CrewKind::Gun) => [0.8, 0.55, 0.35],
        Kind::Crew(CrewKind::Post) => [0.55, 0.6, 0.9],
        Kind::Crew(CrewKind::Work) => [0.85, 0.8, 0.45],
        Kind::Crew(CrewKind::Climb) => [0.7, 0.5, 0.8],
        Kind::Crew(CrewKind::Count) => [0.6, 0.6, 0.6],
    }
}

/// `n + 1` points of a flat arc in the horizontal plane around `centre`, from `facing - arc/2` to `facing + arc/2` (Bevy axes;
/// `facing` is a horizontal-projected direction; positive angles turn to port, as a heading does).
pub fn arc_points(centre: Vec3, facing: Vec3, arc_deg: f32, radius: f32, n: usize) -> Vec<Vec3> {
    let flat = Vec3::new(facing.x, 0.0, facing.z).normalize_or(Vec3::NEG_Z);
    let half = arc_deg.to_radians() / 2.0;
    (0..=n.max(1)).map(|i| centre + Quat::from_rotation_y(-half + 2.0 * half * i as f32 / n.max(1) as f32) * flat * radius).collect()
}

/// A camera distance that frames the model: from the layers' box (game axes) the centre in Bevy axes and a distance.
pub fn frame(facts: Option<&Facts>) -> (Vec3, f32) {
    let Some(f) = facts.filter(|f| !f.layers.is_empty()) else { return (Vec3::ZERO, 40.0) };
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for l in &f.layers {
        for k in 0..3 {
            lo[k] = lo[k].min(l.aabb[0][k]);
            hi[k] = hi[k].max(l.aabb[1][k]);
        }
    }
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let radius = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f64>().sqrt() as f32 / 2.0;
    (to_bevy(centre), (radius * 2.6).max(4.0))
}

fn setup(mut commands: Commands, mut gizmos: ResMut<GizmoConfigStore>) {
    // Points are inside the hull: draw them over it.
    gizmos.config_mut::<DefaultGizmoConfigGroup>().0.depth_bias = -1.0;
    // Below egui's camera (order 0), so the panels and labels stay on top; inactive until the tab is shown.
    commands.spawn((ShipCamera, Camera3d::default(), Camera { order: -1, is_active: false, ..default() }, Transform::default()));
    commands.spawn((DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(-30.0, 60.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y)));
}

/// New meshes when a model was loaded.
fn rebuild_meshes(
    mut commands: Commands,
    view: Res<ShipView>,
    mut orbit: ResMut<Orbit>,
    old: Query<Entity, With<HullMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if orbit.seen == view.generation {
        return;
    }
    orbit.seen = view.generation;
    for e in &old {
        commands.entity(e).despawn();
    }
    let (target, dist) = frame(view.facts.as_ref());
    (orbit.target, orbit.dist) = (target, dist);
    for (i, layer) in view.facts.iter().flat_map(|f| f.layers.iter()).enumerate() {
        let (pos, nor) = layer_mesh(layer);
        if pos.is_empty() {
            continue;
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
        // One colour per layer, spread around the hue circle.
        let color = Color::hsl((i as f32 * 47.0) % 360.0, 0.25, 0.62);
        let material = materials.add(StandardMaterial { base_color: color, perceptual_roughness: 0.9, cull_mode: None, double_sided: true, ..default() });
        commands.spawn((HullMesh { layer: i, material: material.clone() }, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default()));
    }
}

/// Orbit with the right or middle button, zoom with the wheel, and draw only in the central rect while the tab is shown.
fn camera(view: Res<ShipView>, mut orbit: ResMut<Orbit>, mut cam: Single<(&mut Camera, &mut Transform), With<ShipCamera>>) {
    let input = view.input;
    let (camera, tf) = &mut *cam;
    let Some((min, max)) = input.rect.filter(|(a, b)| b.x > a.x + 1.0 && b.y > a.y + 1.0) else {
        camera.is_active = false;
        return;
    };
    orbit.yaw -= input.orbit_drag.x * 0.008;
    orbit.pitch = (orbit.pitch + input.orbit_drag.y * 0.008).clamp(-1.5, 1.5);
    orbit.dist = (orbit.dist * (1.0 - input.scroll * 0.002)).clamp(1.0, 5000.0);
    let eye = orbit.target + Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(-orbit.pitch) * Vec3::Z * orbit.dist;
    **tf = Transform::from_translation(eye).looking_at(orbit.target, Vec3::Y);
    let s = input.pixels_per_point.max(0.1);
    camera.viewport = Some(Viewport {
        physical_position: UVec2::new((min.x * s) as u32, (min.y * s) as u32),
        physical_size: UVec2::new(((max.x - min.x) * s) as u32, ((max.y - min.y) * s) as u32),
        ..default()
    });
    camera.is_active = true;
}

/// Points as drawn: the dragged one at its preview place.
fn shown_points(view: &ShipView, drag: &Dragging) -> Vec<Hardpoint> {
    let mut pts = view.points();
    if let Some(d) = &drag.0
        && let Some(h) = pts.iter_mut().find(|h| h.kind == d.hp.kind && h.label == d.hp.label)
    {
        h.pos = d.current;
    }
    pts
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Hover, select, drag a handle, add on click. The panel recorded the pointer in `ShipView::input`.
fn interact(mut view: ResMut<ShipView>, mut drag: ResMut<Dragging>, orbit: Res<Orbit>, cam: Single<(&Camera, &GlobalTransform), With<ShipCamera>>) {
    let view = &mut *view;
    view.hover_label = None;
    view.hovered_layer = None;
    view.hovered_point = None;
    let input: ViewInput = view.input;
    let Some(_) = input.rect else {
        drag.0 = None;
        return;
    };
    let (camera, gt) = *cam;
    // Window coordinates (logical): Bevy 0.20's `viewport_to_world` / `world_to_viewport` subtract and add the viewport's origin
    // themselves, so the cursor is not made relative to the rect here (it was, and every ray was off by the side panel's width).
    let cursor = input.hover;
    let ray = cursor.and_then(|c| camera.viewport_to_world(gt, c).ok());
    let project = |p: Vec3| camera.world_to_viewport(gt, p).unwrap_or(Vec2::splat(f32::MAX / 4.0));
    let points = shown_points(view, &drag);
    let handle_len = orbit.dist * 0.12;

    if let Some(d) = drag.0.as_mut() {
        if let Some(c) = cursor {
            let (axis_world, _) = AXES[d.axis];
            let dist = axis_drag(axis_world, d.start_cursor, c, project, to_bevy(d.start));
            let mut g = d.start;
            g[d.axis] += dist as f64;
            d.current = g;
        }
        if input.primary_released || !input.primary_down {
            let d = drag.0.take().expect("checked");
            if d.current != d.start {
                match move_point(&view.ship, view.facts.as_ref(), &d.hp, d.current).and_then(|e| view.commit(e)) {
                    Ok(()) => view.status = format!("Moved {} {}", d.hp.kind.title(), d.hp.label),
                    Err(e) => view.status = e,
                }
            }
        }
        return;
    }

    let positions: Vec<[f64; 3]> = points.iter().map(|h| h.pos).collect();
    if let Some(ray) = ray {
        let (origin, dir) = (ray.origin, *ray.direction);
        view.hovered_point = pick_point(origin, dir, &positions, orbit.dist * 0.015);
        view.hovered_layer = view.facts.as_ref().and_then(|f| ray_hit(origin, dir, &f.layers)).map(|(i, _)| i);
        if let (Some(i), Some(c)) = (view.hovered_point, cursor) {
            let h = &points[i];
            view.hover_label = Some((c + Vec2::new(12.0, 12.0), format!("{} {}", h.kind.title(), h.label)));
        }
    }
    if !input.primary_pressed {
        return;
    }
    let (Some(ray), Some(c)) = (ray, cursor) else { return };
    if let Some(armed) = view.armed.clone() {
        let Some((_, hit)) = view.facts.as_ref().and_then(|f| ray_hit(ray.origin, *ray.direction, &f.layers)) else {
            view.status = "Click on the hull to place the point (Esc in the panel disarms).".into();
            return;
        };
        match add_point(&view.ship, view.facts.as_ref(), armed.kind, &armed.label, hit).and_then(|e| view.commit(e)) {
            Ok(()) => {
                view.status = format!("Added {} {}", armed.kind.title(), armed.label);
                view.selected = Some((
                    armed.kind,
                    if matches!(armed.kind, Kind::TowTower) {
                        "tower".into()
                    } else if matches!(armed.kind, Kind::TowTowee) {
                        "towee".into()
                    } else if armed.kind == Kind::Homing {
                        "homing".into()
                    } else {
                        armed.label.trim().to_string()
                    },
                ));
                view.armed = None;
            }
            Err(e) => view.status = e,
        }
        return;
    }
    // A handle of the selected point first (its arrows stand out of the sphere), then the points themselves.
    let selected = view.selected.as_ref().and_then(|(k, l)| points.iter().find(|h| h.kind == *k && h.label == *l)).cloned();
    if let Some(h) = selected {
        let anchor = to_bevy(h.pos);
        let best =
            (0..3).map(|i| (i, distance_to_segment(c, project(anchor), project(anchor + AXES[i].0 * handle_len)))).filter(|(_, d)| *d < 9.0).min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((axis, _)) = best {
            drag.0 = Some(Drag { start: h.pos, current: h.pos, hp: h, axis, start_cursor: c });
            return;
        }
    }
    view.selected = view.hovered_point.map(|i| (points[i].kind, points[i].label.clone()));
}

/// The hovered layer's mesh is lighter.
fn highlight(view: Res<ShipView>, meshes: Query<&HullMesh>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for m in &meshes {
        if let Some(mut mat) = materials.get_mut(&m.material) {
            let hovered = view.hovered_layer == Some(m.layer);
            let color = Color::hsl((m.layer as f32 * 47.0) % 360.0, if hovered { 0.8 } else { 0.25 }, if hovered { 0.7 } else { 0.62 });
            if mat.base_color != color {
                mat.base_color = color;
            }
        }
    }
}

/// Points, facings, arcs, tow line, selection handles.
fn draw(mut gizmos: Gizmos, view: Res<ShipView>, drag: Res<Dragging>, orbit: Res<Orbit>) {
    if view.input.rect.is_none() {
        return;
    }
    let pts = shown_points(&view, &drag);
    let unit = orbit.dist * 0.012;
    let len = orbit.dist * 0.1;
    let selected = view.selected.as_ref();
    for (i, h) in pts.iter().enumerate() {
        let p = to_bevy(h.pos);
        let [r, g, b] = kind_color(h.kind);
        let mut color = Color::srgb(r, g, b);
        let is_selected = selected.is_some_and(|(k, l)| *k == h.kind && *l == h.label);
        if view.hovered_point == Some(i) || is_selected {
            color = color.lighter(0.3);
        }
        gizmos.sphere(p, if is_selected { unit * 1.5 } else { unit }, color).resolution(12);
        match h.kind {
            Kind::Gun => {
                if let Some(rot) = h.rot {
                    let f = facing(rot);
                    gizmos.line(p, p + f * len, color);
                    // PROVISIONAL: arcs are a provisional engine area; drawn flat around the facing, `arc_deg` wide in total.
                    if let Some(arc) = h.arc_deg.filter(|a| *a > 0.0) {
                        let arc_pts = arc_points(p, f, arc as f32, len * 1.5, 24);
                        gizmos.linestrip(arc_pts.iter().copied(), color.with_alpha(0.7));
                        gizmos.line(p, arc_pts[0], color.with_alpha(0.5));
                        gizmos.line(p, *arc_pts.last().expect("non-empty"), color.with_alpha(0.5));
                    }
                }
            }
            Kind::Engine | Kind::Wake => {
                // The port's heading is the yaw of its rot (rot[0]); no rot: aft, where an exhaust goes.
                let dir = h.rot.map_or(Vec3::Z, |r| facing([r[0], 0.0, 0.0]) * -1.0);
                gizmos.line(p, p + dir * len * 0.8, color);
            }
            Kind::Homing => {
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    gizmos.line(p - axis * unit * 2.0, p + axis * unit * 2.0, color);
                }
            }
            _ => {}
        }
    }
    let tower = pts.iter().find(|h| h.kind == Kind::TowTower);
    let towee = pts.iter().find(|h| h.kind == Kind::TowTowee);
    if let (Some(a), Some(b)) = (tower, towee) {
        gizmos.line(to_bevy(a.pos), to_bevy(b.pos), Color::srgb(0.6, 0.9, 0.3));
    }
    if let Some(h) = selected.and_then(|(k, l)| pts.iter().find(|h| h.kind == *k && h.label == *l)) {
        let anchor = to_bevy(h.pos);
        for (axis, color) in AXES {
            gizmos.arrow(anchor, anchor + axis * len, color);
        }
    }
}

/// Where a point comes from, for the panel: "ship.json" or the model node.
pub fn source_text(h: &Hardpoint) -> String {
    match &h.source {
        Source::Json(p) => format!("ship.json {p}"),
        Source::Model(n) => format!("model node {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ship_view::points::fixtures::facts;

    #[test]
    fn a_layer_becomes_one_flat_shaded_triangle_in_bevy_axes() {
        let f = facts();
        let (pos, nor) = layer_mesh(&f.layers[0]);
        assert_eq!((pos.len(), nor.len()), (3, 3));
        // game (-1,0,0), (1,0,0), (0,0,1): the plane y = 0 of the game is Bevy z = 0
        assert!(pos.iter().all(|p| p[2] == 0.0));
        assert!(nor.iter().all(|n| n[0] == 0.0 && n[1] == 0.0 && n[2].abs() == 1.0), "{nor:?}");
        assert!(pos.contains(&[-1.0, 0.0, 0.0]) && pos.contains(&[0.0, 1.0, 0.0]));
    }

    #[test]
    fn the_triangle_faces_the_way_the_game_winding_says() {
        // The engine's winding is (a, c, b) of the game triangle: with the hull wall facing game -y (starboard-up triangle seen from
        // aft is counter-clockwise a, b, c in the game), the Bevy normal must be +z (toward the viewer at +z).
        let layer = Layer { name: "L".into(), aabb: [[0.0; 3]; 2], tris: vec![-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };
        let (pos, nor) = layer_mesh(&layer);
        let n = Vec3::from(nor[0]);
        let (a, b, c) = (Vec3::from(pos[0]), Vec3::from(pos[1]), Vec3::from(pos[2]));
        assert!(((b - a).cross(c - a)).dot(n) > 0.0, "the emitted winding agrees with the normal");
    }

    #[test]
    fn colours_differ_by_kind_and_by_crew_kind() {
        let mut seen = std::collections::HashSet::new();
        for k in [Kind::Gun, Kind::Engine, Kind::Wake, Kind::Flag, Kind::TowTower, Kind::Homing, Kind::Boarding] {
            assert!(seen.insert(kind_color(k).map(f32::to_bits)), "{k:?}");
        }
        for c in CrewKind::ALL {
            assert!(seen.insert(kind_color(Kind::Crew(c)).map(f32::to_bits)), "{c:?}");
        }
        assert_eq!(kind_color(Kind::TowTower), kind_color(Kind::TowTowee));
    }

    #[test]
    fn an_arc_is_flat_symmetric_and_the_right_width() {
        let pts = arc_points(Vec3::new(1.0, 5.0, 2.0), Vec3::NEG_Z, 90.0, 2.0, 8);
        assert_eq!(pts.len(), 9);
        assert!(pts.iter().all(|p| p.y == 5.0 && ((*p - Vec3::new(1.0, 5.0, 2.0)).length() - 2.0).abs() < 1e-5));
        let (a, b) = (pts[0] - Vec3::new(1.0, 5.0, 2.0), pts[8] - Vec3::new(1.0, 5.0, 2.0));
        assert!((a.angle_between(b).to_degrees() - 90.0).abs() < 1e-3);
        assert!((pts[4] - Vec3::new(1.0, 5.0, 0.0)).length() < 1e-5, "the middle is along the facing");
    }

    #[test]
    fn framing_uses_the_layers_box_and_defaults_without_a_model() {
        assert_eq!(frame(None), (Vec3::ZERO, 40.0));
        let f = facts();
        let (c, d) = frame(Some(&f));
        assert!((c - to_bevy([0.0, 0.0, 0.5])).length() < 1e-5);
        assert!(d >= 4.0);
    }

    #[test]
    fn distance_to_a_segment_clamps_at_the_ends() {
        let (a, b) = (Vec2::ZERO, Vec2::new(10.0, 0.0));
        assert_eq!(distance_to_segment(Vec2::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(distance_to_segment(Vec2::new(-4.0, 3.0), a, b), 5.0);
        assert_eq!(distance_to_segment(Vec2::new(1.0, 1.0), a, a), 2f32.sqrt());
    }
}
