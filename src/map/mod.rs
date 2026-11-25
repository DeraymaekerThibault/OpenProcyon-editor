//! Phase 4 (map editor), the part that reads `map.json`. Everything that names a map key lives in [`doc`]; the rest of the
//! editor asks it. `map.json` has no `format` key yet (docs/phase4.md "Status"), so every key name here is PROVISIONAL until the
//! v0.3 freeze; a format change then touches `doc.rs` only.
//!
//! - [`doc`]: accessors over a `map.json` held as a `serde_json::Value`.

pub mod doc;
