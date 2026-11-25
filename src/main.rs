//! OpenProcyon Editor: the Bevy app and its egui panels (mod project, ships, weapons, 3D ship view, play-test and export).
//! It ships no game content and reads only what the player imported with the engine (docs/roadmap.md).

use openprocyon_editor::ship_view::{self, ShipView};
use openprocyon_editor::{maps_panel, mission_panel, playtest_panel, project_panel, ships_panel, startup, weapons_panel, which_mod};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui};
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

/// The command-line flags (`startup::parse`), kept for the systems that act on them.
#[derive(Resource)]
struct Flags(startup::Args);

fn main() {
    let flags = match startup::parse(std::env::args().skip(1)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("procyon-editor: {e}");
            std::process::exit(2);
        }
    };
    // A scratch folder an editor that was killed left behind (symlinks into the import, merged files, pictures).
    openprocyon_editor::overlay::remove_stale();
    App::new()
        .insert_resource(find_assets())
        .insert_resource(Flags(flags))
        .init_resource::<mission_panel::State>()
        .init_resource::<maps_panel::State>()
        .init_resource::<project_panel::State>()
        .init_resource::<project_panel::Focus>()
        .init_resource::<playtest_panel::State>()
        .init_resource::<Tab>()
        .init_resource::<ships_panel::State>()
        .init_resource::<ships_panel::Linked>()
        .init_resource::<weapons_panel::State>()
        // The egui context belongs to the 2D camera, whichever camera spawns first (the 3D ship view has its own, below it).
        .insert_resource(EguiGlobalSettings { auto_create_primary_context: false, ..default() })
        .add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(Window { title: "OpenProcyon Editor".into(), ..default() }), ..default() }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(ship_view::scene::plugin)
        .add_plugins(openprocyon_editor::map::view3d::plugin) // T9
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Camera2d, PrimaryEguiContext));
        })
        .add_systems(PostStartup, apply_flags)
        .add_systems(Update, screenshot_and_exit)
        .add_systems(EguiPrimaryContextPass, panel)
        .add_systems(Update, project_panel::poll_system)
        // After the 3D view's own Update systems: its edits go to the undo stack, undo and redo show in the view.
        .add_systems(PostUpdate, ships_panel::sync_system)
        .run();
    // A normal quit: the projects have been dropped; what the previews and the map view made goes too.
    openprocyon_editor::overlay::remove_own();
}

/// `--mod`, `--tab`, `--ship` and `--map`, once at start.
#[allow(clippy::too_many_arguments)] // a Bevy system takes one parameter per resource it sets
fn apply_flags(
    flags: Res<Flags>,
    assets: Res<Assets>,
    mut project: ResMut<project_panel::State>,
    mut tab: ResMut<Tab>,
    mut ships: ResMut<ships_panel::State>,
    mut view: ResMut<ShipView>,
    mut link: ResMut<ships_panel::Linked>,
    mut maps: ResMut<maps_panel::State>,
) {
    let f = &flags.0;
    if let Some(dir) = &f.mod_dir {
        project.open(dir, assets.dir.clone().filter(|d| d.is_dir()), false);
        // the start-up mod is not "another mod": `--ship` below selects in it, and the tabs must not clear that
        project.opened = 0;
    }
    if let Some(t) = f.tab.as_deref() {
        *tab = match t {
            "ships" => Tab::Ships,
            "weapons" => Tab::Weapons,
            "3d" => Tab::Ship3d,
            "playtest" => Tab::Playtest,
            "mission" => Tab::Mission,
            "map" | "map3d" => Tab::Map,
            _ => Tab::Project,
        };
        maps.v3.on = t == "map3d";
    }
    if let Some(id) = &f.map
        && let Some(p) = &mut project.project
    {
        maps.show_map(p, id);
    }
    maps.select_at_start(&f.select);
    if let Some(id) = &f.ship {
        ships.selected = Some(id.clone());
        if *tab == Tab::Ship3d
            && let Some(p) = &mut project.project
            && let Err(e) = p.open_doc(&ships_panel::ship_rel(id)).and_then(|_| ships_panel::show_in_3d(p, id, &mut view, &mut link))
        {
            eprintln!("procyon-editor: --ship {id}: {e}");
        }
    }
}

/// `--screenshot <file> [--frame N]`: saves the window at frame N, then exits a few frames later (once the picture is written).
fn screenshot_and_exit(flags: Res<Flags>, mut frame: Local<u32>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    let Some(path) = flags.0.screenshot.clone() else { return };
    *frame += 1;
    if *frame == flags.0.frame {
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
    } else if *frame == flags.0.frame + 30 {
        exit.write(AppExit::Success);
    }
}

/// The central area's tabs. Each package adds its tab here and draws it from its own module.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Project,
    Ships,
    Weapons,
    Mission,
    Map,
    Ship3d,
    Playtest,
}

impl Tab {
    const ALL: [(Tab, &'static str); 7] = [
        (Tab::Project, "Mod"),
        (Tab::Ships, "Ships"),
        (Tab::Weapons, "Weapons"),
        (Tab::Map, "Maps"),
        (Tab::Ship3d, "3D ship"),
        (Tab::Playtest, "Play-test & export"),
        (Tab::Mission, "Mission (spike)"),
    ];
}

#[allow(clippy::too_many_arguments)] // a Bevy system takes one parameter per resource it draws
fn panel(
    mut contexts: EguiContexts,
    assets: Res<Assets>,
    mut tab: ResMut<Tab>,
    mut mission: ResMut<mission_panel::State>,
    mut map: ResMut<maps_panel::State>,
    mut project: ResMut<project_panel::State>,
    mut focus: ResMut<project_panel::Focus>,
    mut ship: ResMut<ShipView>,
    mut ships: ResMut<ships_panel::State>,
    mut link: ResMut<ships_panel::Linked>,
    mut weapons: ResMut<weapons_panel::State>,
    mut playtest: ResMut<playtest_panel::State>,
    mut seen_opened: Local<u64>,
) -> Result {
    project_panel::shortcuts(contexts.ctx_mut()?, &mut project);
    project_panel::forget_selection(&mut seen_opened, &project, &mut ships, &mut weapons, &mut ship, &mut link);
    let ctx = contexts.ctx_mut()?;
    let mut root = egui::Ui::new(ctx.clone(), "root".into(), egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()));
    egui::Panel::left("side").resizable(true).default_size(300.0).size_range(260.0..=420.0).show(&mut root, |ui| {
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
        project_panel::side(ui, &mut project, assets.dir.as_deref().filter(|d| d.is_dir()));
    });
    egui::Panel::bottom("problems").resizable(true).show(&mut root, |ui| {
        project_panel::problems_panel(ui, &mut project, &mut focus);
        ui.separator();
        egui::CollapsingHeader::new("Mission problems (spike)").default_open(false).show(ui, |ui| mission_panel::problems(ui, &mission));
    });
    // A click in the problems list names a file: show the tab that edits it (the tab then selects the ship or entry and the field).
    if !focus.rel.is_empty() {
        if ships_panel::ship_id_of(&focus.rel).is_some() {
            *tab = Tab::Ships;
        } else if weapons_panel::Table::of_file(&focus.rel).is_some() {
            *tab = Tab::Weapons;
        }
    }
    egui::Panel::top("tabs").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            for (t, label) in Tab::ALL {
                ui.selectable_value(&mut *tab, t, label);
            }
        });
    });
    if *tab == Tab::Ship3d {
        egui::Panel::right("ship3d_controls").resizable(true).show(&mut root, |ui| ship_view::panel::controls(ui, &mut ship));
        // Transparent: the 3D camera draws underneath, in this panel's rect.
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(&mut root, |ui| ship_view::panel::central(ui, &mut ship));
        return Ok(());
    }
    ship_view::panel::hidden(&mut ship);
    if *tab == Tab::Map {
        // No scroll area: the plan fills the space left under its toolbars.
        // T9: with the 3D map view on, the 3D camera draws underneath this panel, so it is transparent; the toolbar rows above the
        // canvas get their background here (the strip above the canvas of the last frame).
        let mut central = egui::CentralPanel::default();
        if map.v3.on && project.project.is_some() {
            let strip = map.v3.bar_rect(root.available_rect_before_wrap());
            root.painter().rect_filled(strip, 0.0, ctx.global_style().visuals.panel_fill);
            central = central.frame(egui::Frame::NONE);
        }
        central.show(&mut root, |ui| maps_panel::tab(ui, &mut map, &mut project));
        which_mod::window(ctx, &mut project, assets.dir.as_deref().filter(|d| d.is_dir()));
        return Ok(());
    }
    let mut go3d = false;
    egui::CentralPanel::default().show(&mut root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| match *tab {
            Tab::Project => project_panel::tab(ui, &mut project),
            Tab::Ships => go3d = ships_panel::tab(ui, &mut ships, &mut project, &mut focus, &mut ship, &mut link, assets.dir.as_deref().filter(|d| d.is_dir())),
            Tab::Weapons => weapons_panel::tab(ui, &mut weapons, &mut project, &mut focus, assets.dir.as_deref().filter(|d| d.is_dir())),
            Tab::Mission => mission_panel::show(ui, &mut mission),
            Tab::Map => {}
            Tab::Ship3d => {}
            Tab::Playtest => {
                // The mod's maps and ships too, when a mod is open (its overlay holds the import plus the mod).
                let root = project.project.as_ref().map(|p| p.root.clone()).or_else(|| assets.dir.clone()).filter(|d| d.is_dir());
                let maps = playtest_panel::map_ids_with_mod(root.as_deref(), project.project.as_ref().map(|p| p.dir.as_path()));
                let ships = playtest_panel::ship_ids_with_mod(root.as_deref(), project.project.as_ref().map(|p| p.dir.as_path()));
                let unsaved = project.project.as_ref().map_or(0, |p| p.dirty_rels().len());
                playtest_panel::show(ui, &mut playtest, project.project.as_ref().map(|p| p.dir.as_path()), unsaved, &maps, &ships);
                if std::mem::take(&mut playtest.save_requested) {
                    project.save();
                }
            }
        });
    });
    which_mod::window(ctx, &mut project, assets.dir.as_deref().filter(|d| d.is_dir()));
    if go3d {
        *tab = Tab::Ship3d;
    }
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
