//! The mission editor (docs/phase5.md): `mission.json`, `media.json` and `map.json` of one map folder.
//!
//! - [`doc`]: the ONE place that names a `mission.json` or `media.json` key (accessors and pointer builders).
//! - the rest are the sections of the "Missions" tab, one `show` each, filled by their own task: [`ships`] (T6: players, ships,
//!   groups), [`plan`] (T7), [`tasks`] (T8), [`rules_view`] (T5), [`speech`] (T10), [`campaign`] (T12), [`report`] (T13).
//!
//! A later task adds its own `pub mod` line below (project, rules, media, refs, catalogue, groups, templates).

pub mod campaign;
pub mod catalogue;
pub mod doc;
pub mod geometry;
pub mod groups;
pub mod plan;
pub mod project;
pub mod report;
pub mod rules_view;
pub mod ships;
pub mod speech;
pub mod tasks;
pub mod templates;
// T2
pub mod media;
pub mod refs;
pub mod rules;
