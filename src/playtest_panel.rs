//! The "Play-test & export" tab: start the real game on the open mod, take a screenshot preview, and export the mod as a zip.
//! Drawing only; the rules are in [`crate::launch`], [`crate::preview`] and [`crate::export`].

use bevy::prelude::Resource;
use bevy_egui::egui;
use std::path::{Path, PathBuf};
use std::process::Child;

use crate::export::{self, ExportPlan};
use crate::launch::{self, Launch, Lobby, ModCheck};
use crate::preview::{self, Preview};

/// The panel's state.
#[derive(Resource)]
pub struct State {
    pub map: String,
    pub ship: String,
    /// The frame the preview picture is taken at (text as typed).
    pub frame: String,
    pub preview: Preview,
    /// Save unsaved documents before launching (the game reads files on disk).
    pub save_first: bool,
    /// Set when the panel wants the caller to save the open project; the caller clears it after saving.
    pub save_requested: bool,
    /// Play through the skirmish lobby (`--lobby`) instead of straight into the battle (`--map`).
    pub lobby: bool,
    pub lobby_set: Lobby,
    /// `--only-mod`: load this mod and none of the player's other enabled mods. On by default.
    pub only_mod: bool,
    pending: Option<Action>,
    /// The `--check-mod` run in flight, and the launch it holds back.
    check_job: Option<std::sync::Mutex<std::sync::mpsc::Receiver<Result<ModCheck, String>>>>,
    held: Option<Launch>,
    check: Option<Result<ModCheck, String>>,
    child: Option<Child>,
    status: String,
    version: Option<Result<String, String>>,
    plan: Option<Result<ExportPlan, String>>,
    confirmed: bool,
    /// Ids of the player's other enabled mods (read once; "Read the mod list again" clears it).
    others: Option<(Vec<String>, Option<PathBuf>)>,
}

/// `id (name)` of every enabled installed mod except the one in `mod_dir` (by folder, or by id when the folder is a copy of an
/// installed mod of that id), in load order.
pub fn other_enabled_mods(installed: &[openprocyon::modding::Installed], mod_dir: &Path) -> Vec<String> {
    let this = std::fs::canonicalize(mod_dir).unwrap_or_else(|_| mod_dir.to_path_buf());
    let this_id =
        std::fs::read_to_string(mod_dir.join("mod.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["id"].as_str().map(str::to_string));
    installed
        .iter()
        .filter(|m| m.enabled && std::fs::canonicalize(&m.dir).unwrap_or_else(|_| m.dir.clone()) != this && this_id.as_deref() != Some(m.id.as_str()))
        .map(|m| if m.name == m.id { m.id.clone() } else { format!("{} ({})", m.id, m.name) })
        .collect()
}

impl Default for State {
    fn default() -> State {
        State {
            map: String::new(),
            ship: String::new(),
            frame: "300".into(),
            preview: Preview::default(),
            save_first: true,
            save_requested: false,
            lobby: false,
            lobby_set: Lobby::default(),
            only_mod: true,
            pending: None,
            check_job: None,
            held: None,
            check: None,
            child: None,
            status: String::new(),
            version: None,
            plan: None,
            confirmed: false,
            others: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Play,
    Preview,
}

/// Folder names under `<import>/<kind>/` that hold `<file>`: map ids (`maps`, `map.json`) or ship ids (`ships`, `ship.json`), sorted.
pub fn ids_in(import: &Path, kind: &str, file: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(import.join(kind)) else { return Vec::new() };
    let mut ids: Vec<String> = rd.flatten().filter(|e| e.path().join(file).is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    ids.sort();
    ids
}

/// Map ids under `<import>/maps/*/map.json`.
pub fn map_ids(import: &Path) -> Vec<String> {
    ids_in(import, "maps", "map.json")
}

/// Ship ids under `<import>/ships/*/ship.json`.
pub fn ship_ids(import: &Path) -> Vec<String> {
    ids_in(import, "ships", "ship.json")
}

/// Ship ids offered in the pickers: those of the root (the import, or the merged overlay of a mod over it) and the open mod's own
/// folder, sorted and without duplicates. The overlay alone can miss a ship the mod gained since it was built.
pub fn ship_ids_with_mod(root: Option<&Path>, mod_dir: Option<&Path>) -> Vec<String> {
    ids_with_mod(root, mod_dir, ship_ids)
}

/// Map ids offered in the picker: those of the root and the open mod's own `maps/`, sorted and without duplicates (the same rule as
/// [`ship_ids_with_mod`]: a map the mod gained since the overlay was built is still offered).
pub fn map_ids_with_mod(root: Option<&Path>, mod_dir: Option<&Path>) -> Vec<String> {
    ids_with_mod(root, mod_dir, map_ids)
}

fn ids_with_mod(root: Option<&Path>, mod_dir: Option<&Path>, ids_of: fn(&Path) -> Vec<String>) -> Vec<String> {
    let mut ids: Vec<String> = root.map(ids_of).unwrap_or_default();
    ids.extend(mod_dir.map(ids_of).unwrap_or_default());
    ids.sort();
    ids.dedup();
    ids
}

fn picker(ui: &mut egui::Ui, label: &str, value: &mut String, options: &[String]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
        egui::ComboBox::from_id_salt(label).selected_text("pick").show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(value, o.clone(), o);
            }
        });
    });
}

impl State {
    fn launch(&self, dir: &Path, with_shot: bool) -> Launch {
        let mut l = Launch::new(dir, self.map.trim());
        l.ship = Some(self.ship.trim().to_string()).filter(|s| !s.is_empty());
        l.only_mod = self.only_mod;
        if with_shot {
            l.shot = Some(preview::shot(self.map.trim(), l.ship.as_deref(), self.frame.trim().parse().unwrap_or(300)));
        } else if self.lobby {
            l.lobby = Some(self.lobby_set.clone());
        }
        l
    }

    fn start(&mut self, l: &Launch) {
        match l.spawn() {
            Ok(c) => {
                self.child = Some(c);
                self.status = format!("Game started on map {}.", l.map);
            }
            Err(e) => self.status = format!("cannot start {}: {e} (set PROCYON_GAME_BIN)", launch::game_bin().display()),
        }
    }

    /// Takes the result of the `--check-mod` run once it is in; launches the held game unless the check found errors.
    fn poll_check(&mut self) {
        let Some(job) = &self.check_job else { return };
        let result = match job.lock().map_or(Err(std::sync::mpsc::TryRecvError::Disconnected), |r| r.try_recv()) {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(e) => Err(format!("the mod check stopped: {e}")),
        };
        self.check_job = None;
        // A check that could not run, or found only warnings, does not hold the launch back; errors do, until the player says go.
        let blocked = result.as_ref().is_ok_and(|c| c.errors() > 0);
        self.check = Some(result);
        if !blocked && let Some(l) = self.held.take() {
            self.start(&l);
        }
    }

    fn run(&mut self, dir: &Path, action: Action) {
        match action {
            Action::Play => {
                // The mod is checked by the game binary first (`--check-mod`), off the UI thread; the launch follows the result.
                self.held = Some(self.launch(dir, false));
                self.check = None;
                self.status = "Checking the mod...".into();
                let (tx, rx) = std::sync::mpsc::channel();
                let dir = dir.to_path_buf();
                std::thread::spawn(move || {
                    let _ = tx.send(launch::check_mod(&dir));
                });
                self.check_job = Some(std::sync::Mutex::new(rx));
            }
            Action::Preview => match self.preview.start(&self.launch(dir, true)) {
                // The status line of a preview is `Preview::phase()`, which follows the job; `status` is for the game run.
                Ok(()) => self.status.clear(),
                Err(e) => self.status = e,
            },
        }
    }
}

/// Draws the tab. `mod_dir` is the open project's folder, `unsaved` its number of unsaved documents, `maps` and `ships` the ids
/// offered in the pickers. Saving is the caller's: when `save_requested` is set after this returns, save the project (the action
/// that asked for it starts on the next frame).
pub fn show(ui: &mut egui::Ui, st: &mut State, mod_dir: Option<&Path>, unsaved: usize, maps: &[String], ships: &[String]) {
    let Some(dir) = mod_dir else {
        ui.label("Open a mod folder first (Mod tab).");
        return;
    };
    // An action waiting for the save to finish.
    if !st.save_requested
        && let Some(a) = st.pending.take()
    {
        st.run(dir, a);
    }
    if st.child.as_mut().is_some_and(|c| !matches!(c.try_wait(), Ok(None))) {
        st.child = None;
    }
    st.poll_check();
    if st.check_job.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    let playing = st.child.is_some() || st.check_job.is_some();
    st.preview.poll();

    ui.heading("Play-test");
    let version = st.version.get_or_insert_with(launch::game_version).clone();
    match version {
        Ok(v) if v != openprocyon::authoring::ENGINE_VERSION => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("The game binary is version {v}; this editor links {}. Files may not mean the same.", openprocyon::authoring::ENGINE_VERSION),
            );
        }
        Ok(_) => {}
        Err(e) => {
            ui.colored_label(ui.visuals().warn_fg_color, format!("Cannot check the game version: {e} (set PROCYON_GAME_BIN)"));
        }
    }
    if ui.small_button("Check the game version again").clicked() {
        st.version = None;
    }
    let (others, bundled) = st
        .others
        .get_or_insert_with(|| {
            let bundled = launch::bundled_mods();
            (other_enabled_mods(&openprocyon::modding::installed(bundled.as_deref()), dir), bundled)
        })
        .clone();
    if !others.is_empty() && !st.only_mod {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Not this mod alone: the game also loads {} other enabled mods ({}) after the engine's files, so they can change what you see. Tick \"Load only this mod\" to leave them out.", others.len(), others.join(", ")),
        );
        ui.weak(format!(
            "Read from your mods folder and the game's bundled mods ({}). PROVISIONAL: the bundled folder is looked up as the game does it (next to {}, else the source checkout found from this editor's working directory); if the game runs from elsewhere the list can differ.",
            bundled.as_ref().map_or("none found".to_string(), |b| b.display().to_string()),
            launch::game_bin().display()
        ));
        if ui.small_button("Read the mod list again").clicked() {
            st.others = None;
        }
    }
    picker(ui, "Map", &mut st.map, maps);
    picker(ui, "Ship", &mut st.ship, ships);
    ui.checkbox(&mut st.lobby, "Play through the skirmish lobby (--lobby)")
        .on_hover_text("The map must be a skirmish map. The ship above is not used: the lobby assigns the fleet.");
    if st.lobby {
        ui.horizontal(|ui| {
            ui.label("Computers");
            ui.add(egui::DragValue::new(&mut st.lobby_set.computers).range(0..=7));
            ui.label("Points");
            ui.add(egui::DragValue::new(&mut st.lobby_set.points).range(0..=100_000).speed(10));
            ui.label("Fleet limit");
            ui.add(egui::DragValue::new(&mut st.lobby_set.limit).range(1..=100));
        });
        ui.weak("Points and limit follow the lobby's own steps and ranges; the game refuses a value it does not accept.");
    }
    ui.checkbox(&mut st.only_mod, "Load only this mod (--only-mod)")
        .on_hover_text("The game loads this mod and none of the other mods you have enabled, so the run is of this mod alone.");
    ui.horizontal(|ui| {
        ui.checkbox(&mut st.save_first, "Save unsaved documents before launching");
        if unsaved > 0 {
            ui.label(format!("({unsaved} unsaved)"));
        }
    });
    let ready = !st.map.trim().is_empty();
    let mut clicked = None;
    ui.horizontal(|ui| {
        if ui.add_enabled(ready && !playing, egui::Button::new("Play-test")).on_hover_text(format!("runs {}", launch::game_bin().display())).clicked() {
            clicked = Some(Action::Play);
        }
        ui.label("Preview at frame");
        ui.add(egui::TextEdit::singleline(&mut st.frame).desired_width(50.0));
        if ui.add_enabled(ready && !st.preview.running(), egui::Button::new("Preview screenshot")).clicked() {
            clicked = Some(Action::Preview);
        }
    });
    if let Some(a) = clicked {
        if st.save_first && unsaved > 0 {
            st.save_requested = true;
            st.pending = Some(a);
        } else {
            st.run(dir, a);
        }
    }
    if unsaved > 0 && !st.save_first {
        ui.label("The game reads the files on disk: unsaved documents are not in the test.");
    }
    if !st.status.is_empty() {
        ui.label(&st.status);
    }
    match &st.check {
        None => {}
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().warn_fg_color, format!("Could not check the mod before launching: {e}"));
        }
        Some(Ok(c)) if c.findings.is_empty() => {
            ui.label("Mod check: no errors, no warnings.");
        }
        Some(Ok(c)) => {
            ui.label(format!("Mod check (--check-mod): {} errors, {} warnings.", c.errors(), c.warnings()));
            egui::ScrollArea::vertical().id_salt("modcheck").max_height(160.0).show(ui, |ui| {
                for f in &c.findings {
                    let color = if f.error { ui.visuals().error_fg_color } else { ui.visuals().warn_fg_color };
                    ui.colored_label(color, f.line());
                }
            });
        }
    }
    if st.held.is_some() && st.check_job.is_none() {
        ui.label("The game was not started because the check found errors.");
        let mut go = false;
        ui.horizontal(|ui| {
            go = ui.button("Launch anyway").clicked();
            if ui.button("Cancel").clicked() {
                st.held = None;
                st.status.clear();
            }
        });
        if go && let Some(l) = st.held.take() {
            st.start(&l);
        }
    }
    let line = st.preview.phase().line();
    if !line.is_empty() {
        ui.label(line);
    }

    ui.separator();
    ui.heading("Export");
    if ui.button("Export zip...").clicked() {
        st.plan = Some(export::plan(dir));
        st.confirmed = false;
    }
    let mut finish = false;
    match &st.plan {
        None => {}
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        Some(Ok(plan)) => {
            ui.label(format!("{} files go into {}/ in the zip.", plan.files.len(), plan.id));
            for w in &plan.warnings {
                ui.colored_label(ui.visuals().warn_fg_color, w);
            }
            if plan.warnings.is_empty() {
                finish = ui.button("Choose where to save...").clicked();
            } else {
                ui.checkbox(&mut st.confirmed, "I have read the warnings and want to export anyway");
                finish = ui.add_enabled(st.confirmed, egui::Button::new("Choose where to save...")).clicked();
            }
        }
    }
    if finish && let Some(Ok(plan)) = &st.plan {
        let out: Option<PathBuf> = rfd::FileDialog::new().set_file_name(format!("{}.zip", plan.id)).add_filter("zip", &["zip"]).save_file();
        if let Some(out) = out {
            st.status = match export::write_zip(plan, &out) {
                Ok(()) => format!("Exported {}", out.display()),
                Err(e) => e,
            };
        }
    }
    // The picture goes last, scaled to fit under a maximum height, so nothing above it is pushed off screen.
    if st.preview.phase() != preview::Phase::Idle {
        ui.separator();
        st.preview.show(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_the_sorted_folders_that_hold_the_file() {
        let d = std::env::temp_dir().join(format!("procyon-editor-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (rel, f) in [("maps/b", "map.json"), ("maps/a", "map.json"), ("maps/empty", "other.json"), ("ships/s1", "ship.json")] {
            std::fs::create_dir_all(d.join(rel)).unwrap();
            std::fs::write(d.join(rel).join(f), "{}").unwrap();
        }
        std::fs::write(d.join("maps/loose.json"), "{}").unwrap();
        assert_eq!(map_ids(&d), ["a", "b"]);
        assert_eq!(ship_ids(&d), ["s1"]);
        assert!(map_ids(&d.join("nowhere")).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_ship_menu_lists_the_mods_own_ships_with_the_imports() {
        let d = std::env::temp_dir().join(format!("procyon-editor-shipmenu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for rel in ["import/ships/Ship_B", "import/ships/Ship_C", "mod/ships/Ship_A", "mod/ships/Ship_B"] {
            std::fs::create_dir_all(d.join(rel)).unwrap();
            std::fs::write(d.join(rel).join("ship.json"), "{}").unwrap();
        }
        let (imp, m) = (d.join("import"), d.join("mod"));
        assert_eq!(ship_ids_with_mod(Some(&imp), Some(&m)), ["Ship_A", "Ship_B", "Ship_C"]);
        assert_eq!(ship_ids_with_mod(Some(&imp), None), ["Ship_B", "Ship_C"]);
        assert_eq!(ship_ids_with_mod(None, Some(&m)), ["Ship_A", "Ship_B"]);
        assert!(ship_ids_with_mod(None, None).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_map_menu_lists_the_mods_own_maps_with_the_imports() {
        let d = std::env::temp_dir().join(format!("procyon-editor-mapmenu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for rel in ["import/maps/red_rover", "import/maps/border", "mod/maps/my_map", "mod/maps/border"] {
            std::fs::create_dir_all(d.join(rel)).unwrap();
            std::fs::write(d.join(rel).join("map.json"), "{}").unwrap();
        }
        // a folder without map.json is not a map
        std::fs::create_dir_all(d.join("mod/maps/notes")).unwrap();
        let (imp, m) = (d.join("import"), d.join("mod"));
        assert_eq!(map_ids_with_mod(Some(&imp), Some(&m)), ["border", "my_map", "red_rover"]);
        assert_eq!(map_ids_with_mod(Some(&imp), None), ["border", "red_rover"]);
        assert_eq!(map_ids_with_mod(None, Some(&m)), ["border", "my_map"]);
        assert!(map_ids_with_mod(None, None).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_list_has_the_users_enabled_mods_and_the_bundled_default_mods_the_game_loads() {
        let d = std::env::temp_dir().join(format!("procyon-editor-gameloads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (user, bundled) = (d.join("user"), d.join("bundled"));
        for (root, id, default) in [
            (user.join("mods"), "mine", false),
            (user.join("mods"), "off", false),
            (bundled.clone(), "speed", true),
            (bundled.clone(), "plain", false),
            (bundled.clone(), "cam", true),
        ] {
            std::fs::create_dir_all(root.join(id)).unwrap();
            std::fs::write(root.join(id).join("mod.json"), format!(r#"{{"id": "{id}", "default": {default}}}"#)).unwrap();
        }
        std::fs::write(user.join("mods.json"), r#"{"enabled": ["mine"], "disabled": ["cam"]}"#).unwrap();
        let all = openprocyon::modding::installed_in(&user, Some(&bundled));
        // the user's `mine`, the bundled default `speed`; not `off`, `plain` (never switched on) or `cam` (switched off)
        assert_eq!(other_enabled_mods(&all, &d.join("editing")), ["mine", "speed"]);
        // without the bundled folder the editor used to show only the first
        assert_eq!(other_enabled_mods(&openprocyon::modding::installed_in(&user, None), &d.join("editing")), ["mine"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn other_enabled_mods_leave_out_this_one_and_the_switched_off() {
        use openprocyon::modding::Installed;
        let d = std::env::temp_dir().join(format!("procyon-editor-others-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("mine")).unwrap();
        std::fs::write(d.join("mine/mod.json"), r#"{"id": "mine-id"}"#).unwrap();
        let m = |id: &str, name: &str, dir: &str, enabled: bool| Installed { id: id.into(), name: name.into(), dir: d.join(dir), enabled, ..Default::default() };
        let all = [m("a", "a", "a", true), m("b", "Bee", "b", true), m("c", "c", "c", false), m("mine-id", "Mine", "installed-copy", true), m("x", "x", "mine", true)];
        assert_eq!(other_enabled_mods(&all, &d.join("mine")), ["a", "b (Bee)"]);
        assert!(other_enabled_mods(&[], &d.join("mine")).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_launch_carries_map_ship_and_frame_from_the_fields() {
        let mut st = State { map: " m ".into(), ship: " ".into(), frame: "120".into(), ..State::default() };
        let l = st.launch(Path::new("/mods/x"), true);
        assert_eq!((l.map.as_str(), l.ship.clone()), ("m", None));
        assert_eq!(l.shot.unwrap().frames, [120]);
        st.ship = "Ship_X".into();
        st.frame = "junk".into();
        let l = st.launch(Path::new("/mods/x"), true);
        assert_eq!(l.ship.as_deref(), Some("Ship_X"));
        assert_eq!(l.shot.unwrap().frames, [300]);
        assert!(st.launch(Path::new("/mods/x"), false).shot.is_none());
    }
}
