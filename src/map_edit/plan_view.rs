//! The top-down plan's view transform: game (x, y) <-> screen, pan, zoom about the cursor, fit to the world, grid lines.
//! Pure math; the only egui types are `Pos2`, `Rect` and `Vec2`. Game axes: x starboard (screen right), y forward (screen up).

use super::snap::SnapSettings;
use bevy_egui::egui::{Pos2, Rect, Vec2, pos2};

/// Smallest and largest zoom, in screen pixels per world unit. PROVISIONAL: comfortable bounds for a map a few thousand units wide.
pub const MIN_SCALE: f32 = 0.005;
pub const MAX_SCALE: f32 = 200.0;

/// Where the plan looks: the world point at the middle of the canvas and the zoom. Also carries the snapping settings the canvas
/// uses, so one value is all a tab keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanView {
    /// The world (x, y) shown at the centre of the canvas rectangle.
    pub center: [f32; 2],
    /// Screen pixels per world unit.
    pub scale: f32,
    pub snap: SnapSettings,
    /// Half the side of the square world, drawn as the map's boundary (`world_half`). PROVISIONAL default 1600, the spike's fallback.
    pub world_half: f32,
}

impl Default for PlanView {
    fn default() -> Self {
        PlanView { center: [0.0, 0.0], scale: 0.2, snap: SnapSettings::default(), world_half: 1600.0 }
    }
}

/// One grid line to draw: a vertical line at screen x (`vertical`) or a horizontal one at screen y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLine {
    pub vertical: bool,
    /// Screen coordinate (x for a vertical line, y for a horizontal one).
    pub at: f32,
    /// The world value (x or y) the line stands for.
    pub world: f32,
    /// Every fifth line is a major one.
    pub major: bool,
}

impl PlanView {
    /// The world point `w` on screen inside `rect`.
    pub fn to_screen(&self, rect: Rect, w: [f32; 2]) -> Pos2 {
        let c = rect.center();
        pos2(c.x + (w[0] - self.center[0]) * self.scale, c.y - (w[1] - self.center[1]) * self.scale)
    }

    /// The world point under the screen point `p`; the inverse of [`PlanView::to_screen`].
    pub fn to_world(&self, rect: Rect, p: Pos2) -> [f32; 2] {
        let c = rect.center();
        [self.center[0] + (p.x - c.x) / self.scale, self.center[1] - (p.y - c.y) / self.scale]
    }

    /// Drags the plan by `delta` screen pixels (the world follows the pointer).
    pub fn pan_by_screen(&mut self, delta: Vec2) {
        self.center[0] -= delta.x / self.scale;
        self.center[1] += delta.y / self.scale;
    }

    /// Multiplies the zoom by `factor` (clamped to [`MIN_SCALE`], [`MAX_SCALE`]) keeping the world point under `cursor` where it is.
    pub fn zoom_at(&mut self, rect: Rect, cursor: Pos2, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let before = self.to_world(rect, cursor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let after = self.to_world(rect, cursor);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }

    /// Centres on `center` and zooms so the world rectangle `half_extent` (half width, half height) fits in `rect` with `margin_px`
    /// to spare on every side.
    pub fn fit(&mut self, rect: Rect, center: [f32; 2], half_extent: [f32; 2], margin_px: f32) {
        let avail = [(rect.width() - 2.0 * margin_px).max(1.0), (rect.height() - 2.0 * margin_px).max(1.0)];
        let sx = avail[0] / (2.0 * half_extent[0].max(f32::EPSILON));
        let sy = avail[1] / (2.0 * half_extent[1].max(f32::EPSILON));
        self.center = center;
        self.scale = sx.min(sy).clamp(MIN_SCALE, MAX_SCALE);
    }

    /// [`PlanView::fit`] for the square of side `2 * world_half` around the origin.
    pub fn fit_world(&mut self, rect: Rect, world_half: f32, margin_px: f32) {
        self.fit(rect, [0.0, 0.0], [world_half, world_half], margin_px);
    }

    /// The world rectangle (min, max) visible in `rect`.
    pub fn visible_world(&self, rect: Rect) -> ([f32; 2], [f32; 2]) {
        let a = self.to_world(rect, rect.left_bottom());
        let b = self.to_world(rect, rect.right_top());
        (a, b)
    }

    /// The grid lines inside `rect` at [`grid_step`] for the current zoom (at least `min_px` pixels apart). At most a few hundred.
    pub fn grid_lines(&self, rect: Rect, min_px: f32) -> Vec<GridLine> {
        let step = grid_step(self.scale, min_px);
        let (lo, hi) = self.visible_world(rect);
        let mut out = vec![];
        for (vertical, a, b) in [(true, lo[0], hi[0]), (false, lo[1], hi[1])] {
            let first = (a / step).ceil() as i64;
            let last = (b / step).floor() as i64;
            if last - first > 1000 {
                continue; // a degenerate zoom; nothing useful to draw
            }
            for k in first..=last {
                let world = k as f32 * step;
                let p = if vertical { self.to_screen(rect, [world, 0.0]).x } else { self.to_screen(rect, [0.0, world]).y };
                out.push(GridLine { vertical, at: p, world, major: k % 5 == 0 });
            }
        }
        out
    }
}

/// The grid step in world units: the smallest of 1, 2, 5 times a power of ten that is at least `min_px` pixels at `scale`.
pub fn grid_step(scale: f32, min_px: f32) -> f32 {
    let target = (min_px / scale.max(f32::EPSILON)).max(f32::MIN_POSITIVE);
    let exp = target.log10().floor();
    let base = 10f32.powf(exp);
    for m in [1.0, 2.0, 5.0, 10.0] {
        if base * m >= target * (1.0 - 1e-6) {
            return base * m;
        }
    }
    base * 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), Vec2::new(400.0, 300.0))
    }

    fn near(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3, "{a:?} vs {b:?}");
    }

    #[test]
    fn centre_maps_to_the_rect_centre_and_y_points_up() {
        let v = PlanView { center: [10.0, 20.0], scale: 2.0, snap: SnapSettings::default(), ..PlanView::default() };
        assert_eq!(v.to_screen(rect(), [10.0, 20.0]), rect().center());
        let p = v.to_screen(rect(), [11.0, 23.0]);
        assert_eq!(p, pos2(rect().center().x + 2.0, rect().center().y - 6.0));
    }

    #[test]
    fn screen_and_world_round_trip() {
        let v = PlanView { center: [-30.0, 7.5], scale: 0.37, snap: SnapSettings::default(), ..PlanView::default() };
        for w in [[0.0, 0.0], [100.0, -250.0], [-1600.0, 1600.0]] {
            near(v.to_world(rect(), v.to_screen(rect(), w)), w);
        }
    }

    #[test]
    fn panning_moves_the_world_with_the_pointer() {
        let mut v = PlanView::default();
        let w = [40.0, -10.0];
        let before = v.to_screen(rect(), w);
        v.pan_by_screen(Vec2::new(25.0, -15.0));
        let after = v.to_screen(rect(), w);
        assert!((after.x - before.x - 25.0).abs() < 1e-3 && (after.y - before.y + 15.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut v = PlanView { center: [5.0, 5.0], scale: 1.0, snap: SnapSettings::default(), ..PlanView::default() };
        let cursor = pos2(150.0, 90.0);
        let w = v.to_world(rect(), cursor);
        v.zoom_at(rect(), cursor, 2.5);
        assert!((v.scale - 2.5).abs() < 1e-6);
        near(v.to_world(rect(), cursor), w);
        v.zoom_at(rect(), cursor, 1e9);
        assert_eq!(v.scale, MAX_SCALE);
        v.zoom_at(rect(), cursor, 1e-30);
        assert_eq!(v.scale, MIN_SCALE);
        let s = v.scale;
        v.zoom_at(rect(), cursor, f32::NAN);
        v.zoom_at(rect(), cursor, -1.0);
        assert_eq!(v.scale, s);
    }

    #[test]
    fn fit_world_shows_the_whole_square_with_the_margin() {
        let mut v = PlanView::default();
        v.fit_world(rect(), 1600.0, 10.0);
        // the limiting side is the height: 300 - 20 px for 3200 units
        assert!((v.scale - 280.0 / 3200.0).abs() < 1e-6);
        let top = v.to_screen(rect(), [0.0, 1600.0]).y;
        assert!((top - (rect().top() + 10.0)).abs() < 1e-3);
        near(v.center, [0.0, 0.0]);
    }

    #[test]
    fn grid_step_is_a_nice_number_that_is_wide_enough() {
        assert_eq!(grid_step(1.0, 40.0), 50.0);
        assert_eq!(grid_step(0.1, 40.0), 500.0);
        assert_eq!(grid_step(10.0, 40.0), 5.0);
        assert_eq!(grid_step(1.0, 10.0), 10.0);
        assert_eq!(grid_step(1.0, 20.0), 20.0);
        assert_eq!(grid_step(0.2, 40.0), 200.0);
        for scale in [0.01, 0.03, 0.2, 1.0, 7.0, 150.0] {
            let step = grid_step(scale, 30.0);
            assert!(step * scale >= 30.0 * 0.999, "{scale}: {step}");
            assert!(step * scale < 30.0 * 5.0, "{scale}: {step} is far wider than needed");
        }
    }

    #[test]
    fn grid_lines_fall_on_multiples_of_the_step_inside_the_rect() {
        let v = PlanView { center: [0.0, 0.0], scale: 1.0, snap: SnapSettings::default(), ..PlanView::default() };
        let lines = v.grid_lines(rect(), 40.0);
        assert!(!lines.is_empty());
        let step = grid_step(1.0, 40.0);
        for l in &lines {
            assert!((l.world / step - (l.world / step).round()).abs() < 1e-4);
            if l.vertical {
                assert!(l.at >= rect().left() - 1e-3 && l.at <= rect().right() + 1e-3);
            } else {
                assert!(l.at >= rect().top() - 1e-3 && l.at <= rect().bottom() + 1e-3);
            }
        }
        // the axis through the origin is a line, and a major one
        assert!(lines.iter().any(|l| l.vertical && l.world == 0.0 && l.major));
        assert!(lines.iter().any(|l| !l.vertical && l.world == 0.0 && l.major));
    }
}
