//! OpenProcyon Editor as a library: the binary (`main.rs`) is the Bevy app and its panels; everything that can be tested headless
//! lives in these modules. It ships no game content and reads only what the player imported with the engine (docs/roadmap.md).

pub mod checks;
pub mod edit;
pub mod launch;
pub mod map_view;
pub mod mission;
pub mod mission_panel;
pub mod overlay;
pub mod project;
pub mod project_panel;
pub mod schema_form;
