//! A generic form for any JSON document that has a JSON Schema (the engine's `authoring::schemas()`: ship, guns, bullets, crew nodes,
//! mod). [`model`] is the pure, egui-free resolved schema and the helpers that build [`crate::edit::Edit`]s; [`widgets`] draws the form
//! and returns the edits it wants. Nothing here knows a file's keys: the schema does.

pub mod model;
pub mod widgets;

pub use widgets::{Choice, FieldNote, FormCtx, show, show_typed};
