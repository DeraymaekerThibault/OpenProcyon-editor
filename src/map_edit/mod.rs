//! Phase 4 (map editor), the pieces that do not depend on `map.json` format 1: a top-down plan view and the maths and widgets
//! behind it. The Maps tab ([`crate::maps_panel`], logic in [`crate::map::view2d`]) is what uses it.
//!
//! Format-independent, and tested headless:
//!
//! - [`plan_view`]: world (game x, y) <-> screen, pan, zoom about the cursor, fit to the world square, grid steps.
//! - [`snap`]: grid, yaw-step and nearest-object snapping.
//! - [`orient`]: the nine-number `rot` <-> yaw/pitch/roll, a yaw-only turn that keeps the tilt, orthonormality check and repair.
//! - [`select`]: the selection model, click and box picking, group move and turn, and the conversion of a result into
//!   [`crate::edit::Edit`]s for a caller-supplied pointer prefix (so the undo stack can be used).
//! - [`canvas`]: the egui widget [`canvas::plan_canvas`]; it draws, reports a selection change and a move delta, and never
//!   mutates a document.
//!
//! Deliberately left for after the `map.json` freeze (v0.3): reading and writing `map.json`, turning objects, starts, nebulae and
//! mines into [`canvas::MarkerIn`]s (field names and the linked nebula entries), the scenery catalogue and placement from it,
//! diffs against the import, sky and lighting pickers, the 3D view.
//!
//! PROVISIONAL (marked again where they are defined): the `rot` layout is a mirror of the engine's `game_rotation`
//! (`game/src/scenario/spawn.rs`: three triples are the columns right, forward, up); the yaw convention (clockwise from +y) and the
//! pitch and roll signs are the editor's own; the default snap steps (10 units, 15 degrees, 12 units), the zoom limits and the
//! default `world_half` of 1600 are guesses.

pub mod canvas;
pub mod orient;
pub mod plan_view;
pub mod select;
pub mod snap;
