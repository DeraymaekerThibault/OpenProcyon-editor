//! OpenProcyon Editor as a library: the binary (`main.rs`) is the Bevy app and its panels; everything that can be tested headless
//! lives in these modules. It ships no game content and reads only what the player imported with the engine (docs/roadmap.md).

pub mod checks;
pub mod compare;
pub mod edit;
pub mod export;
pub mod form;
pub mod launch;
pub mod launch_report;
pub mod map;
pub mod map_edit;
pub mod map_view;
pub mod maps_panel;
pub mod mission;
pub mod mission_panel;
pub mod overlay;
pub mod playtest_panel;
pub mod preview;
pub mod project;
pub mod project_panel;
pub mod schema_form;
pub mod ship_view;
pub mod ships_panel;
pub mod startup;
pub mod weapons_panel;
pub mod which_mod;
