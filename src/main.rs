//! OpenProcyon Editor, Phase 0 skeleton: a Bevy window with one egui panel that says where the player's imported assets are.
//! It ships no game content and reads only what the player imported with the engine (docs/roadmap.md).

use openprocyon_editor::{map_view, mission_panel};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use openprocyon::setup::{Imported, imported};
use std::path::PathBuf;

/// The engine release the editor links; an import is stamped with the version that made it (`setup::imported`).
use openprocyon::authoring::ENGINE_VERSION;
const IMPORT_HELP: &str = "https://github.com/DeraymaekerThibault/OpenProcyon#getting-started-from-source";

#[derive(Resource)]
struct Assets {
    dir: Option<PathBuf>,
    status: Imported,
}

/// The engine's rule (`run` in its lib.rs): only the per-user folder the download imports into must carry this version's stamp;
/// `PROCYON_ASSETS` and a checkout's `assets/` (made by the command-line importer, no stamp) are used as they are if they hold `ships/`.
fn status_of(dir: &std::path::Path, env_set: bool, user_assets: Option<&std::path::Path>, version: &str) -> Imported {
    let player = !env_set && user_assets.is_some_and(|u| u == dir);
    match player {
        true => imported(dir, version),
        false if dir.join("ships").is_dir() => Imported::Ready,
        false => Imported::Missing,
    }
}

/// The engine's own side-effect-free lookup (`authoring::base_assets`: `PROCYON_ASSETS`, the checkout's `assets/`, the per-user
/// folder); the stamp rule above needs to know whether the folder found is the per-user one.
fn find_assets() -> Assets {
    let env_set = std::env::var_os("PROCYON_ASSETS").is_some();
    let user = openprocyon::campaign::user_dir().map(|d| d.join("assets"));
    let dir = openprocyon::authoring::base_assets().or_else(|| user.clone());
    let status = dir.as_deref().map_or(Imported::Missing, |d| status_of(d, env_set, user.as_deref(), ENGINE_VERSION));
    Assets { dir, status }
}

fn main() {
    App::new()
        .insert_resource(find_assets())
        .init_resource::<mission_panel::State>()
        .init_resource::<map_view::State>()
        .init_resource::<Tab>()
        .add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(Window { title: "OpenProcyon Editor".into(), ..default() }), ..default() }))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(EguiPrimaryContextPass, panel)
        .run();
}

/// The central area's tabs. Each package adds its tab here and draws it from its own module.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Mission,
    Map,
}

impl Tab {
    const ALL: [(Tab, &'static str); 2] = [(Tab::Mission, "Mission (spike)"), (Tab::Map, "Map (spike)")];
}

fn panel(mut contexts: EguiContexts, assets: Res<Assets>, mut tab: ResMut<Tab>, mut mission: ResMut<mission_panel::State>, mut map: ResMut<map_view::State>) -> Result {
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
    egui::Panel::top("tabs").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            for (t, label) in Tab::ALL {
                ui.selectable_value(&mut *tab, t, label);
            }
        });
    });
    egui::CentralPanel::default().show(&mut root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| match *tab {
            Tab::Mission => mission_panel::show(ui, &mut mission),
            Tab::Map => map_view::show(ui, &mut map, assets.dir.as_deref().filter(|d| d.is_dir())),
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-main-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("ships")).unwrap();
        d
    }

    #[test]
    fn unstamped_developer_import_is_ready_like_in_the_engine() {
        let d = tmp("dev");
        assert_eq!(status_of(&d, true, None, "0.2.0"), Imported::Ready); // PROCYON_ASSETS
        assert_eq!(status_of(&d, false, Some(Path::new("/elsewhere")), "0.2.0"), Imported::Ready); // checkout assets/
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn per_user_folder_needs_a_matching_stamp() {
        let d = tmp("user");
        assert!(matches!(status_of(&d, false, Some(&d), "0.2.0"), Imported::Outdated { version: None, .. }));
        std::fs::write(d.join("import.json"), r#"{"version": "0.1.0"}"#).unwrap();
        assert!(matches!(status_of(&d, false, Some(&d), "0.2.0"), Imported::Outdated { version: Some(_), .. }));
        std::fs::write(d.join("import.json"), r#"{"version": "0.2.0"}"#).unwrap();
        assert_eq!(status_of(&d, false, Some(&d), "0.2.0"), Imported::Ready);
        // the same folder named by PROCYON_ASSETS is a developer's: no stamp needed
        std::fs::remove_file(d.join("import.json")).unwrap();
        assert_eq!(status_of(&d, true, Some(&d), "0.2.0"), Imported::Ready);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_ships_folder_is_missing() {
        let d = std::env::temp_dir().join("procyon-editor-main-none");
        assert_eq!(status_of(&d, true, None, "0.2.0"), Imported::Missing);
    }
}
