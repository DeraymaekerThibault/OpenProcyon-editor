//! Snapping while placing: positions to a grid, yaw to an angle step, a position to the nearest other object. Pure.

use std::f32::consts::PI;

/// What the canvas snaps to. `None` switches a kind off. PROVISIONAL defaults: a 10-unit grid, 15 degree steps, 12-unit object
/// snap, none of which the engine requires.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapSettings {
    /// Grid step in world units.
    pub grid: Option<f32>,
    /// Yaw step in degrees.
    pub angle_deg: Option<f32>,
    /// Snap onto another object's position when within this many world units (beats the grid).
    pub object_radius: Option<f32>,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings { grid: Some(10.0), angle_deg: Some(15.0), object_radius: Some(12.0) }
    }
}

impl SnapSettings {
    /// Everything off.
    pub fn off() -> Self {
        SnapSettings { grid: None, angle_deg: None, object_radius: None }
    }
}

/// What a snapped position landed on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapKind {
    None,
    Grid,
    /// The index (into the `others` slice) of the object snapped to.
    Object(usize),
}

/// A position after snapping and what it snapped to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapped {
    pub pos: [f32; 2],
    pub kind: SnapKind,
}

/// `p` rounded to the nearest multiple of `step` on both axes (`step <= 0` or not finite: unchanged).
pub fn snap_grid(p: [f32; 2], step: f32) -> [f32; 2] {
    if !(step.is_finite() && step > 0.0) {
        return p;
    }
    [(p[0] / step).round() * step, (p[1] / step).round() * step]
}

/// `yaw` (radians) rounded to the nearest multiple of `step_deg` degrees, result in (-pi, pi].
pub fn snap_angle(yaw: f32, step_deg: f32) -> f32 {
    if !(step_deg.is_finite() && step_deg > 0.0) {
        return yaw;
    }
    let step = step_deg.to_radians();
    wrap_angle((yaw / step).round() * step)
}

/// An angle in radians brought into (-pi, pi].
pub fn wrap_angle(a: f32) -> f32 {
    let r = (a + PI).rem_euclid(2.0 * PI) - PI;
    if r <= -PI { PI } else { r }
}

/// The nearest of `others` within `radius` of `p` (index and position); ties go to the lower index.
pub fn nearest_object(p: [f32; 2], others: &[[f32; 2]], radius: f32) -> Option<(usize, [f32; 2])> {
    let mut best: Option<(usize, f32)> = None;
    for (i, o) in others.iter().enumerate() {
        let d = ((o[0] - p[0]).powi(2) + (o[1] - p[1]).powi(2)).sqrt();
        if d <= radius && best.is_none_or(|(_, b)| d < b) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| (i, others[i]))
}

/// Snaps a dragged position: onto the nearest other object when one is within the radius, else to the grid, else unchanged.
pub fn snap_position(p: [f32; 2], others: &[[f32; 2]], s: &SnapSettings) -> Snapped {
    if let Some(r) = s.object_radius
        && let Some((i, o)) = nearest_object(p, others, r)
    {
        return Snapped { pos: o, kind: SnapKind::Object(i) };
    }
    if let Some(g) = s.grid.filter(|g| g.is_finite() && *g > 0.0) {
        return Snapped { pos: snap_grid(p, g), kind: SnapKind::Grid };
    }
    Snapped { pos: p, kind: SnapKind::None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_rounds_each_axis_to_the_nearest_step() {
        assert_eq!(snap_grid([14.0, -16.0], 10.0), [10.0, -20.0]);
        assert_eq!(snap_grid([4.9, 5.1], 10.0), [0.0, 10.0]);
        assert_eq!(snap_grid([3.3, 4.4], 0.0), [3.3, 4.4]);
        assert_eq!(snap_grid([3.3, 4.4], -1.0), [3.3, 4.4]);
        assert_eq!(snap_grid([3.3, 4.4], f32::NAN), [3.3, 4.4]);
    }

    #[test]
    fn angles_snap_to_the_step_and_wrap() {
        let a = snap_angle(20f32.to_radians(), 15.0);
        assert!((a - 15f32.to_radians()).abs() < 1e-6);
        let b = snap_angle(-100f32.to_radians(), 45.0);
        assert!((b + 90f32.to_radians()).abs() < 1e-6);
        // just past a half turn wraps to the other side
        let c = snap_angle(179f32.to_radians(), 90.0);
        assert!((c.abs() - PI).abs() < 1e-5, "{c}");
        assert_eq!(snap_angle(1.234, 0.0), 1.234);
        assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-5);
        assert!((wrap_angle(-PI) - PI).abs() < 1e-6);
    }

    #[test]
    fn nearest_object_respects_the_radius_and_picks_the_closest() {
        let others = [[0.0, 0.0], [10.0, 0.0], [10.0, 3.0]];
        assert_eq!(nearest_object([9.0, 2.0], &others, 5.0), Some((2, [10.0, 3.0])));
        assert_eq!(nearest_object([50.0, 50.0], &others, 5.0), None);
        assert_eq!(nearest_object([5.0, 0.0], &[[0.0, 0.0], [10.0, 0.0]], 5.0), Some((0, [0.0, 0.0])), "ties: lower index");
        assert_eq!(nearest_object([0.0, 0.0], &[], 5.0), None);
    }

    #[test]
    fn objects_beat_the_grid_and_the_grid_beats_nothing() {
        let s = SnapSettings { grid: Some(10.0), angle_deg: None, object_radius: Some(4.0) };
        let others = [[103.0, 57.0]];
        assert_eq!(snap_position([101.0, 55.0], &others, &s), Snapped { pos: [103.0, 57.0], kind: SnapKind::Object(0) });
        assert_eq!(snap_position([121.0, 55.0], &others, &s), Snapped { pos: [120.0, 60.0], kind: SnapKind::Grid });
        assert_eq!(snap_position([121.0, 55.0], &others, &SnapSettings::off()), Snapped { pos: [121.0, 55.0], kind: SnapKind::None });
    }
}
