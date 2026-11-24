//! OpenProcyon Editor, Phase 0 skeleton: a Bevy window with one egui panel that says where the player's imported assets are.
//! It ships no game content and reads only what the player imported with the engine (docs/roadmap.md).

mod checks;
mod jsonfmt;
mod launch;
mod mission;
mod mission_panel;
mod schema_form;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use openprocyon::setup::{Imported, imported};
use std::path::PathBuf;

/// The engine release the editor links; an import is stamped with the version that made it (`setup::imported`).
/// PROVISIONAL: kept equal to the tag in Cargo.toml by hand; the engine exposes no version constant at v0.2.0.
const ENGINE_VERSION: &str = "0.2.0";
const IMPORT_HELP: &str = "https://github.com/DeraymaekerThibault/OpenProcyon#getting-started-from-source";

#[derive(Resource)]
struct Assets {
    dir: Option<PathBuf>,
    status: Imported,
}

/// The engine's own lookup is `base_assets()`, which is `pub(crate)` at v0.2.0, and the public `assets_dir()` rebuilds the shared
/// overlay folder as a side effect (it would clobber a running game's, see #136). So this composes the engine's public pieces in
/// the same order: `PROCYON_ASSETS`, else the per-user folder the importer fills.
/// PROVISIONAL: skips the engine's checkout `assets/` step (meaningless from a git dependency); replace with the Phase 1 `authoring` API.
fn find_assets() -> Assets {
    let dir = std::env::var_os("PROCYON_ASSETS").map(PathBuf::from).or_else(|| Some(openprocyon::campaign::user_dir()?.join("assets")));
    let status = dir.as_deref().map_or(Imported::Missing, |d| imported(d, ENGINE_VERSION));
    Assets { dir, status }
}

fn main() {
    App::new()
        .insert_resource(find_assets())
        .init_resource::<mission_panel::State>()
        .add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(Window { title: "OpenProcyon Editor".into(), ..default() }), ..default() }))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(EguiPrimaryContextPass, panel)
        .run();
}

fn panel(mut contexts: EguiContexts, assets: Res<Assets>, mut mission: ResMut<mission_panel::State>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = egui::Ui::new(ctx.clone(), "root".into(), egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()));
    egui::Panel::left("side").resizable(true).show(&mut root, |ui| {
        ui.heading("OpenProcyon Editor");
        ui.separator();
        let dir = assets.dir.as_ref().map_or("(no per-user folder on this system)".into(), |d| d.display().to_string());
        ui.label(format!("Imported assets: {dir}"));
        match &assets.status {
            Imported::Ready => ui.label("Ready."),
            Imported::Outdated { version, .. } => ui.label(format!(
                "Imported by engine version {}, this editor links {ENGINE_VERSION}. Import again with the matching engine.",
                version.as_deref().unwrap_or("unknown")
            )),
            Imported::Missing => {
                ui.label("No imported assets found. Import your own copy of the game with the engine first (or set PROCYON_ASSETS):");
                ui.hyperlink(IMPORT_HELP)
            }
        };
    });
    egui::Panel::bottom("problems").resizable(true).show(&mut root, |ui| mission_panel::problems(ui, &mission));
    egui::CentralPanel::default().show(&mut root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| mission_panel::show(ui, &mut mission));
    });
    Ok(())
}
