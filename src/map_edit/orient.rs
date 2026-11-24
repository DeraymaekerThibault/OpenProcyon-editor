//! Orientation of a map object: the 3x3 `rot` of `map.json` (nine numbers) as yaw, pitch and roll, a yaw-only rotation that keeps
//! the tilt, and checks and repair for hand-edited matrices. Pure, f64 (the document's own precision).
//!
//! Layout (PROVISIONAL: a mirror of the engine's `game_rotation` in `game/src/scenario/spawn.rs`, which reads the nine numbers as
//! three triples that are the matrix's columns, with `docs/data-format.md` saying the second triple is forward): `rot[0..3]` is the
//! object's right (starboard) axis, `rot[3..6]` its forward axis, `rot[6..9]` its up axis, each in game axes (x starboard, y
//! forward, z up). The identity is `[1,0,0, 0,1,0, 0,0,1]`; a missing or non-numeric entry reads as the identity's, as the engine does.
//!
//! Angles (radians; the editor's own convention, not the engine's, so PROVISIONAL too): yaw is the heading seen from above,
//! clockwise from +y (forward = (sin yaw, cos yaw, 0) when level), pitch is the nose-up angle, roll raises the starboard side when
//! positive. The object's frame is built as yaw, then pitch about the right axis, then roll about the forward axis.

use serde_json::Value;

/// A rotation in the document's layout: right, forward, up triples.
pub type Rot = [f64; 9];

pub const IDENTITY: Rot = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Yaw, pitch and roll in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Euler {
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
}

type V = [f64; 3];

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn scale(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}

fn axes(r: &Rot) -> (V, V, V) {
    ([r[0], r[1], r[2]], [r[3], r[4], r[5]], [r[6], r[7], r[8]])
}

fn pack(r: V, f: V, u: V) -> Rot {
    [r[0], r[1], r[2], f[0], f[1], f[2], u[0], u[1], u[2]]
}

/// The nine numbers of a JSON `rot`; a missing or non-numeric entry is the identity's (as the engine's `game_rotation`), `None`
/// when `v` is not an array at all. PROVISIONAL: see the module doc.
pub fn read_rot(v: &Value) -> Option<Rot> {
    let a = v.as_array()?;
    let mut r = IDENTITY;
    for (i, x) in r.iter_mut().enumerate() {
        if let Some(n) = a.get(i).and_then(Value::as_f64) {
            *x = n;
        }
    }
    Some(r)
}

/// The matrix for yaw, pitch and roll (see the module doc for the convention).
pub fn rot_from_euler(e: Euler) -> Rot {
    let (sy, cy) = e.yaw.sin_cos();
    let (sp, cp) = e.pitch.sin_cos();
    let (sr, cr) = e.roll.sin_cos();
    let f = [sy * cp, cy * cp, sp];
    let r0 = [cy, -sy, 0.0];
    let up0 = cross(r0, f);
    let r = add(scale(r0, cr), scale(up0, sr));
    let u = cross(r, f);
    pack(r, f, u)
}

/// Yaw, pitch and roll of a (near-)orthonormal matrix; the inverse of [`rot_from_euler`] for pitch in (-pi/2, pi/2). Straight up
/// or down the heading is taken from the right axis and roll is 0.
pub fn euler_from_rot(rot: &Rot) -> Euler {
    let (r, f, _) = axes(rot);
    let fl = len(f).max(f64::MIN_POSITIVE);
    let f = scale(f, 1.0 / fl);
    let pitch = f[2].clamp(-1.0, 1.0).asin();
    if f[0].hypot(f[1]) < 1e-9 {
        return Euler { yaw: (-r[1]).atan2(r[0]), pitch, roll: 0.0 };
    }
    let yaw = f[0].atan2(f[1]);
    let r0 = [yaw.cos(), -yaw.sin(), 0.0];
    let up0 = cross(r0, f);
    Euler { yaw, pitch, roll: dot(r, up0).atan2(dot(r, r0)) }
}

/// The heading alone (radians), the `yaw` of [`euler_from_rot`].
pub fn yaw_of(rot: &Rot) -> f64 {
    euler_from_rot(rot).yaw
}

fn rotate_z(v: V, d: f64) -> V {
    let (s, c) = d.sin_cos();
    [v[0] * c + v[1] * s, -v[0] * s + v[1] * c, v[2]]
}

/// Turns the object clockwise (seen from above) by `delta` radians about the world's vertical axis. Pitch and roll relative to
/// the heading are kept exactly (all three axes are rotated; nothing is decomposed).
pub fn rotate_yaw(rot: &Rot, delta: f64) -> Rot {
    let (r, f, u) = axes(rot);
    pack(rotate_z(r, delta), rotate_z(f, delta), rotate_z(u, delta))
}

/// The same tilt with the heading set to `yaw`.
pub fn with_yaw(rot: &Rot, yaw: f64) -> Rot {
    rotate_yaw(rot, yaw - yaw_of(rot))
}

/// Largest deviation from an orthonormal, right-handed frame: unit lengths, right angles between the axes, and `right x forward`
/// equal to up (so a mirrored matrix counts as bad).
pub fn orthonormal_error(rot: &Rot) -> f64 {
    let (r, f, u) = axes(rot);
    let mut e = [(len(r) - 1.0).abs(), (len(f) - 1.0).abs(), (len(u) - 1.0).abs(), dot(r, f).abs(), dot(f, u).abs(), dot(r, u).abs()].into_iter().fold(0.0, f64::max);
    let c = cross(r, f);
    e = e.max(len(sub(c, u)));
    if rot.iter().any(|x| !x.is_finite()) { f64::INFINITY } else { e }
}

/// Is the matrix a proper rotation within `eps`?
pub fn is_orthonormal(rot: &Rot, eps: f64) -> bool {
    orthonormal_error(rot) <= eps
}

/// The nearest proper rotation that keeps the forward axis: forward is normalised, right is made perpendicular to it, up is
/// `right x forward`. A forward of (almost) zero length or a right axis parallel to it falls back to the level frame with the same
/// heading where one can be read, else the identity. Non-finite entries give the identity.
pub fn orthonormalize(rot: &Rot) -> Rot {
    if rot.iter().any(|x| !x.is_finite()) {
        return IDENTITY;
    }
    let (r, f, u) = axes(rot);
    let fl = len(f);
    if fl < 1e-9 {
        return IDENTITY;
    }
    let f = scale(f, 1.0 / fl);
    // right from the given right axis, else from the up axis, else the level right of the forward
    let pick = |c: V| {
        let p = sub(c, scale(f, dot(c, f)));
        (len(p) > 1e-9).then(|| scale(p, 1.0 / len(p)))
    };
    let r = pick(r).or_else(|| pick(cross(f, u))).or_else(|| pick([f[1], -f[0], 0.0]));
    let Some(r) = r else { return IDENTITY };
    pack(r, f, cross(r, f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn close(a: &Rot, b: &Rot, eps: f64) {
        for i in 0..9 {
            assert!((a[i] - b[i]).abs() < eps, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn identity_is_level_and_faces_forward() {
        let e = euler_from_rot(&IDENTITY);
        assert_eq!(e, Euler::default());
        close(&rot_from_euler(Euler::default()), &IDENTITY, 1e-12);
        assert!(is_orthonormal(&IDENTITY, 1e-12));
    }

    #[test]
    fn a_quarter_turn_clockwise_faces_starboard() {
        let r = rot_from_euler(Euler { yaw: std::f64::consts::FRAC_PI_2, ..Euler::default() });
        // forward is the second triple: +x
        close(&r, &[0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], 1e-12);
    }

    #[test]
    fn euler_round_trips() {
        for yaw in [-3.0, -1.0, 0.0, 0.7, 2.5, 3.1] {
            for pitch in [-1.2, -0.3, 0.0, 0.4, 1.3] {
                for roll in [-2.0, -0.5, 0.0, 0.9, 3.0] {
                    let e = Euler { yaw, pitch, roll };
                    let r = rot_from_euler(e);
                    assert!(is_orthonormal(&r, 1e-12), "{e:?}");
                    let back = euler_from_rot(&r);
                    assert!((back.yaw - yaw).abs() < 1e-9 && (back.pitch - pitch).abs() < 1e-9 && (back.roll - roll).abs() < 1e-9, "{e:?} -> {back:?}");
                    close(&rot_from_euler(back), &r, 1e-9);
                }
            }
        }
    }

    #[test]
    fn straight_up_still_gives_a_usable_heading() {
        let e = Euler { yaw: 0.8, pitch: std::f64::consts::FRAC_PI_2, roll: 0.0 };
        let r = rot_from_euler(e);
        let back = euler_from_rot(&r);
        assert!((back.pitch - e.pitch).abs() < 1e-9);
        close(&rot_from_euler(back), &r, 1e-6);
    }

    #[test]
    fn rotate_yaw_keeps_pitch_and_roll() {
        let e = Euler { yaw: 0.4, pitch: 0.3, roll: -0.6 };
        let r = rotate_yaw(&rot_from_euler(e), 1.1);
        let back = euler_from_rot(&r);
        assert!((back.yaw - 1.5).abs() < 1e-9 && (back.pitch - 0.3).abs() < 1e-9 && (back.roll + 0.6).abs() < 1e-9, "{back:?}");
        assert!(is_orthonormal(&r, 1e-12));
        close(&rotate_yaw(&rotate_yaw(&r, 0.9), -0.9), &r, 1e-12);
        close(&rotate_yaw(&r, 0.0), &r, 1e-12);
    }

    #[test]
    fn with_yaw_sets_the_heading_only() {
        let r = rot_from_euler(Euler { yaw: -2.0, pitch: 0.2, roll: 0.1 });
        let back = euler_from_rot(&with_yaw(&r, 0.5));
        assert!((back.yaw - 0.5).abs() < 1e-9 && (back.pitch - 0.2).abs() < 1e-9 && (back.roll - 0.1).abs() < 1e-9);
    }

    #[test]
    fn checks_catch_scaled_skewed_and_mirrored_matrices() {
        assert!(!is_orthonormal(&[2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], 1e-6));
        assert!(!is_orthonormal(&[1.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 1.0], 1e-6));
        assert!(!is_orthonormal(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0], 1e-6), "mirrored");
        assert!(!is_orthonormal(&[f64::NAN; 9], 1e-6));
        assert!(is_orthonormal(&rot_from_euler(Euler { yaw: 1.0, pitch: 0.5, roll: 0.2 }), 1e-12));
    }

    #[test]
    fn repair_gives_a_proper_rotation_and_keeps_the_forward_axis() {
        let bad = [1.0, 0.2, 0.0, 0.1, 2.0, 0.0, 0.0, 0.3, 0.5];
        let fixed = orthonormalize(&bad);
        assert!(is_orthonormal(&fixed, 1e-12));
        let fl = (0.1f64 * 0.1 + 4.0).sqrt();
        assert!((fixed[3] - 0.1 / fl).abs() < 1e-12 && (fixed[4] - 2.0 / fl).abs() < 1e-12);
        // already good: unchanged
        let good = rot_from_euler(Euler { yaw: 0.3, pitch: 0.2, roll: 0.1 });
        close(&orthonormalize(&good), &good, 1e-12);
        // mirrored becomes right-handed
        assert!(is_orthonormal(&orthonormalize(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0]), 1e-12));
    }

    #[test]
    fn repair_survives_degenerate_input() {
        assert_eq!(orthonormalize(&[0.0; 9]), IDENTITY);
        assert_eq!(orthonormalize(&[f64::NAN; 9]), IDENTITY);
        // right parallel to forward
        let r = orthonormalize(&[0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(is_orthonormal(&r, 1e-12));
        assert_eq!(&r[3..6], &[0.0, 1.0, 0.0]);
    }

    #[test]
    fn reading_json_defaults_missing_entries_like_the_engine() {
        assert_eq!(read_rot(&json!([0, -1, 0, 1, 0, 0, 0, 0, 1])).unwrap(), [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(read_rot(&json!([])).unwrap(), IDENTITY);
        assert_eq!(read_rot(&json!([0, 0, 0, 0, "x"])).unwrap(), [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(read_rot(&json!(null)), None);
    }
}
