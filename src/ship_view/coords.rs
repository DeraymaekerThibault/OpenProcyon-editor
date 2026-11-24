//! Axes for the ship view. The game's frame is x starboard, y forward, z up; Bevy and glTF use (x, z, -y) of it
//! (`openprocyon::data::conv`). Everything the editor shows or moves is stored in game axes in `ship.json`.

use bevy::math::{Quat, Vec3};
use serde_json::Value;

pub use crate::map_view::short;

/// A game-axes point as a Bevy-axes vector: the engine's own `data::conv`, so the editor and the loader agree.
pub fn to_bevy(p: [f64; 3]) -> Vec3 {
    openprocyon::data::conv(p[0] as f32, p[1] as f32, p[2] as f32)
}

/// A Bevy-axes vector as a game-axes point (`[x, -z, y]`), the inverse of [`to_bevy`].
/// PROVISIONAL: engine requirement: a public inverse of `data::conv`.
pub fn to_game(v: Vec3) -> [f64; 3] {
    [v.x as f64, -v.z as f64, v.y as f64]
}

/// A game-axes direction or point as a Bevy-axes `Vec3` from a `Vec3` that holds game axes (x, y, z).
pub fn game_vec_to_bevy(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y)
}

/// The inverse of [`game_vec_to_bevy`].
pub fn bevy_vec_to_game(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.z, v.y)
}

/// A position as the JSON triple `ship.json` holds: each number the f32's shortest decimal ([`short`]).
pub fn pos_value(p: [f64; 3]) -> Value {
    Value::Array(p.iter().map(|v| short(*v as f32)).collect())
}

/// A JSON `[x, y, z]` as three numbers (`None` when it is not an array of at least three numbers).
pub fn read_triple(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

/// A `rot` triple of a mount or crew node as the node's rotation in Bevy axes. PROVISIONAL: a copy of the engine's private
/// `mount_rotation`, used only to draw facings and arcs; engine requirement: authoring: public mount_rotation and its inverse.
pub fn mount_rotation(rot: [f64; 3]) -> Quat {
    let r = rot.map(|v| v as f32);
    Quat::from_rotation_z(-r[1]) * Quat::from_rotation_y(r[0]) * Quat::from_rotation_x(r[2])
}

/// The way a mount faces, as a unit vector in Bevy axes (the rotated forward direction, Bevy -z). PROVISIONAL: see [`mount_rotation`].
pub fn facing(rot: [f64; 3]) -> Vec3 {
    mount_rotation(rot) * Vec3::NEG_Z
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLES: [[f64; 3]; 5] = [[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [-4.5, 10.25, -0.5], [0.0, -7.0, 2.0], [100.0, 0.125, -33.0]];

    #[test]
    fn game_to_bevy_and_back_round_trips() {
        for p in SAMPLES {
            assert_eq!(to_game(to_bevy(p)), p);
        }
    }

    #[test]
    fn to_bevy_agrees_with_the_engines_conv() {
        for p in SAMPLES {
            assert_eq!(to_bevy(p), openprocyon::data::conv(p[0] as f32, p[1] as f32, p[2] as f32));
        }
        // forward (game +y) is Bevy -z, up (game +z) is Bevy +y, starboard is +x in both
        assert_eq!(to_bevy([0.0, 1.0, 0.0]), Vec3::NEG_Z);
        assert_eq!(to_bevy([0.0, 0.0, 1.0]), Vec3::Y);
        assert_eq!(to_bevy([1.0, 0.0, 0.0]), Vec3::X);
    }

    #[test]
    fn vector_helpers_are_inverse() {
        for p in SAMPLES {
            let v = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32);
            assert_eq!(bevy_vec_to_game(game_vec_to_bevy(v)), v);
            assert_eq!(game_vec_to_bevy(v), to_bevy(p));
        }
    }

    #[test]
    fn positions_are_written_as_short_numbers() {
        assert_eq!(pos_value([1.5, -369.23077392578125, 0.0]), json!([1.5, -369.23077, 0.0]));
        assert_eq!(read_triple(&json!([1, 2.5, -3])), Some([1.0, 2.5, -3.0]));
        assert_eq!(read_triple(&json!([1, 2])), None);
        assert_eq!(read_triple(&json!("x")), None);
    }

    #[test]
    fn a_zero_rotation_faces_forward_and_yaw_turns_to_port() {
        let f = facing([0.0; 3]);
        assert!((f - Vec3::NEG_Z).length() < 1e-6, "bevy -z is game forward");
        let port = facing([std::f64::consts::FRAC_PI_2, 0.0, 0.0]);
        assert!((port - Vec3::NEG_X).length() < 1e-5, "a quarter turn to port: {port}");
    }
}
