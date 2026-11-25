//! What is left of the Phase 0 spike's map view (docs/spike.md items 2, 3 and 7) once the Maps tab ([`crate::maps_panel`]) works on
//! project documents: the one number helper the plan, the 3D ship view and the map writers share. The spike's own plan, its whole-file
//! `map.json` writer and its play-test button are gone (the project saves maps as diffs; the Play-test tab launches the game).

use serde_json::{Value, json};

/// A dragged coordinate as the f32's shortest decimal (`-369.23077`), not the widened f64 (`-369.23077392578125`).
/// The engine reads positions as f32 anyway, so nothing is lost.
pub fn short(v: f32) -> Value {
    json!(v.to_string().parse::<f64>().unwrap_or(v as f64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openprocyon::authoring::to_json_string;

    #[test]
    fn dragged_value_round_trips_to_a_short_number() {
        let v = short(-369.230_77_f32);
        assert_eq!(to_json_string(&v).trim(), "-369.23077");
        assert_eq!(v.as_f64().unwrap() as f32, -369.230_77_f32);
    }
}
