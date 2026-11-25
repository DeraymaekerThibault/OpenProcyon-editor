//! Picking in the ship view: a ray against the model's triangles, a ray against the hardpoints' spheres, and a drag along an axis
//! handle. Pure. Rays come from the camera in Bevy axes; the model's triangles and the points are in game axes (`derive::Facts`).

use super::coords::{bevy_vec_to_game, to_bevy};
use bevy::math::{Vec2, Vec3};
use openprocyon::data::derive::Layer;

fn game(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)
}

/// Möller–Trumbore: the distance along the ray (direction `dir`, not necessarily unit) to the triangle, or `None` (a miss, a ray
/// parallel to the triangle's plane, or a hit behind the origin). Both sides of the triangle are hit.
pub(crate) fn triangle_hit(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    const EPS: f32 = 1e-7;
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < EPS {
        return None;
    }
    let inv = 1.0 / det;
    let t0 = origin - a;
    let u = t0.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t0.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > EPS).then_some(t)
}

/// The nearest triangle the ray hits: the layer's index and the hit point in game axes. `origin` and `dir` are in Bevy axes
/// (from `Camera::viewport_to_world`); the layers' triangles are in game axes, so the ray is converted once.
pub fn ray_hit(origin: Vec3, dir: Vec3, layers: &[Layer]) -> Option<(usize, [f64; 3])> {
    let (o, d) = (bevy_vec_to_game(origin), bevy_vec_to_game(dir));
    let mut best: Option<(f32, usize)> = None;
    // Damaged variants of a section lie over the intact layer: never pickable (the engine drops them from the hull).
    for (i, layer) in layers.iter().enumerate().filter(|(_, l)| !super::points::is_damage_layer(&l.name)) {
        for t in layer.tris.as_chunks::<9>().0 {
            let hit = triangle_hit(o, d, game([t[0], t[1], t[2]]), game([t[3], t[4], t[5]]), game([t[6], t[7], t[8]]));
            if let Some(h) = hit.filter(|h| best.is_none_or(|(b, _)| *h < b)) {
                best = Some((h, i));
            }
        }
    }
    let (t, i) = best?;
    let p = o + d * t;
    Some((i, [p.x as f64, p.y as f64, p.z as f64]))
}

/// The point (game axes) nearest to the ray's line among those within `radius` of it and in front of the origin: its index.
/// `origin` and `dir` are in Bevy axes.
pub fn pick_point(origin: Vec3, dir: Vec3, points: &[[f64; 3]], radius: f32) -> Option<usize> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    points
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let to = to_bevy(*p) - origin;
            let along = to.dot(dir);
            let off = (to - dir * along).length();
            (along > 0.0 && off <= radius).then_some((i, off))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// A mouse drag along an axis handle as a distance along that world axis. `project` maps a world point to screen pixels; `anchor` is
/// the handle's origin and `axis_world` its direction (Bevy axes). The result is signed, in units of `axis_world`'s length (a unit
/// axis gives world units), and 0 when the axis points at the camera (no screen direction to follow).
pub fn axis_drag(axis_world: Vec3, start_screen: Vec2, now_screen: Vec2, project: impl Fn(Vec3) -> Vec2, anchor: Vec3) -> f32 {
    let on_screen = project(anchor + axis_world) - project(anchor);
    let len2 = on_screen.length_squared();
    if len2 < 1e-6 {
        return 0.0;
    }
    (now_screen - start_screen).dot(on_screen) / len2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A triangle in the game's x-z plane at y = 0 (a unit-ish wall facing forward).
    fn wall(y: f64) -> Layer {
        Layer { name: "L".into(), aabb: [[0.0; 3]; 2], tris: vec![-1.0, y, 0.0, 1.0, y, 0.0, 0.0, y, 2.0] }
    }

    /// A ray in Bevy axes from game-axes position `from` looking along game-axes direction `d`.
    fn ray(from: [f64; 3], d: [f64; 3]) -> (Vec3, Vec3) {
        (to_bevy(from), to_bevy(d))
    }

    #[test]
    fn a_ray_hits_a_triangle_and_reports_the_point_in_game_axes() {
        let (o, d) = ray([0.0, -5.0, 0.5], [0.0, 1.0, 0.0]);
        let (i, p) = ray_hit(o, d, &[wall(0.0)]).unwrap();
        assert_eq!(i, 0);
        assert!(p.iter().zip([0.0, 0.0, 0.5]).all(|(a, b)| (a - b).abs() < 1e-5), "{p:?}");
    }

    #[test]
    fn misses_behind_and_outside_hit_nothing() {
        let (o, d) = ray([0.0, -5.0, 0.5], [0.0, 1.0, 0.0]);
        assert!(ray_hit(o, -d, &[wall(0.0)]).is_none(), "behind the origin");
        let (o, d) = ray([5.0, -5.0, 0.5], [0.0, 1.0, 0.0]);
        assert!(ray_hit(o, d, &[wall(0.0)]).is_none(), "beside the triangle");
        assert!(ray_hit(o, d, &[]).is_none());
    }

    #[test]
    fn a_ray_parallel_to_the_triangle_misses() {
        let (o, d) = ray([-5.0, 0.0, 0.5], [1.0, 0.0, 0.0]);
        assert!(ray_hit(o, d, &[wall(0.0)]).is_none());
    }

    #[test]
    fn a_damage_variant_over_a_layer_is_never_hit() {
        let over = || Layer { name: "ShipDamage_Layer2".into(), ..wall(0.5) };
        let (o, d) = ray([0.0, -5.0, 0.5], [0.0, 1.0, 0.0]);
        // The damage variant is nearer, but only the intact layer (index 0) is hit.
        assert_eq!(ray_hit(o, d, &[wall(1.0), over()]).unwrap().0, 0);
        assert!(ray_hit(o, d, &[over()]).is_none());
    }

    #[test]
    fn the_nearest_hit_wins_across_layers_and_from_either_side() {
        let layers = [wall(3.0), wall(1.0), wall(2.0)];
        let (o, d) = ray([0.0, -5.0, 0.5], [0.0, 1.0, 0.0]);
        let (i, p) = ray_hit(o, d, &layers).unwrap();
        assert_eq!(i, 1);
        assert!((p[1] - 1.0).abs() < 1e-5);
        let (o, d) = ray([0.0, 9.0, 0.5], [0.0, -1.0, 0.0]);
        assert_eq!(ray_hit(o, d, &layers).unwrap().0, 0, "from the other side");
    }

    #[test]
    fn a_point_is_picked_within_the_radius_and_the_closest_to_the_ray_wins() {
        let pts = [[0.0, 5.0, 0.0], [0.4, 5.0, 0.0], [3.0, 5.0, 0.0], [0.0, -5.0, 0.0]];
        let (o, d) = ray([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_eq!(pick_point(o, d, &pts, 0.5), Some(0));
        assert_eq!(pick_point(o, d, &pts[1..], 0.5), Some(0), "the point 0.4 away");
        assert_eq!(pick_point(o, d, &pts[1..], 0.3), None, "outside the radius");
        assert_eq!(pick_point(o, d, &pts[3..], 10.0), None, "behind the camera");
        assert_eq!(pick_point(o, Vec3::ZERO, &pts, 1.0), None);
    }

    #[test]
    fn axis_drag_follows_the_axis_on_screen_in_sign_and_scale() {
        // orthographic top view: 10 px per world unit, x to the right, world -z (game +y) up the screen
        let project = |p: Vec3| Vec2::new(p.x * 10.0, p.z * 10.0);
        let anchor = Vec3::ZERO;
        let start = Vec2::new(100.0, 100.0);
        assert!((axis_drag(Vec3::X, start, start + Vec2::new(30.0, 0.0), project, anchor) - 3.0).abs() < 1e-5);
        assert!((axis_drag(Vec3::X, start, start + Vec2::new(-20.0, 7.0), project, anchor) + 2.0).abs() < 1e-5, "sideways motion is ignored, sign flips");
        assert!((axis_drag(Vec3::Z, start, start + Vec2::new(0.0, 50.0), project, anchor) - 5.0).abs() < 1e-5);
        assert!((axis_drag(Vec3::X * 2.0, start, start + Vec2::new(30.0, 0.0), project, anchor) - 1.5).abs() < 1e-5, "a 2-unit axis: distance in its own units");
        assert_eq!(axis_drag(Vec3::Y, start, start + Vec2::new(30.0, 0.0), project, anchor), 0.0, "an axis along the view has no screen direction");
    }
}
