//! Phase 4 (map editor), the part that reads `map.json`. Everything that names a map key lives in [`doc`]; the rest of the
//! editor asks it. `map.json` has no `format` key yet (docs/phase4.md "Status"), so every key name here is PROVISIONAL until the
//! v0.3 freeze; a format change then touches `doc.rs` only.
//!
//! - [`doc`]: accessors over a `map.json` held as a `serde_json::Value`.
//! - [`catalogue`]: scenery types, scenes and skies found by scanning an overlay folder.
//! - [`project`]: maps as project documents: list, open (merged over the import), new, move, add, delete.
//! - [`settings`]: the map settings form (name, description, `world_half`, sky, lighting, backdrop) and the edits it writes.
//! - [`players`]: players, teams, starts and the formation preview (T7).
//! - [`view2d`]: what the Maps tab's plan view shows and what its drags, turns, placements and deletes write (no drawing).

pub mod catalogue;
pub mod doc;
pub mod players;
pub mod project;
pub mod settings;
pub mod view2d;
