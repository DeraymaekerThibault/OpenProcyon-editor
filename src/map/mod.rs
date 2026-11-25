//! Phase 4 (map editor), the part that reads `map.json`. Everything that names a map key lives in [`doc`]; the rest of the
//! editor asks it. `map.json` has no `format` key yet (docs/phase4.md "Status"), so every key name here is PROVISIONAL until the
//! v0.3 freeze; a format change then touches `doc.rs` only.
//!
//! - [`doc`]: accessors over a `map.json` held as a `serde_json::Value`.
//! - [`catalogue`]: scenery types, scenes and skies found by scanning an overlay folder.
//! - [`project`]: maps as project documents: list, open (merged over the import), new, move, add, delete.
//! - [`settings`]: the map settings form (name, description, `world_half`, sky, lighting, backdrop) and the edits it writes.
//! - [`skirmish`] (T10): the skirmish flag and lobby checklist, end rules, `script.json` rules, `media.json`, play in the lobby.
//! - [`players`]: players, teams, starts and the formation preview (T7).
//! - [`nebula`], [`mines`] (T8): nebula polygons and the three-record writer; mine types and placing.
//! - [`glb`], [`view3d`] (T9): a plain GLB reader for scenery; the 3D view of the map: placement, picking, the height handle.
//! - [`docks`] (T12): dock points and crew places of a mod's own scenery, placed on the 3D view.
//! - [`view2d`]: what the Maps tab's plan view shows and what its drags, turns, placements and deletes write (no drawing).

pub mod catalogue;
pub mod doc;
pub mod docks; // T12
pub mod glb;
pub mod mines;
pub mod nebula;
pub mod players;
pub mod project;
pub mod settings;
pub mod skirmish;
pub mod view2d;
pub mod view3d;
