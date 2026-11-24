//! The mod project's panels: the left section (open, new, save, undo, layers, notices), the problems list at the bottom and the
//! "Mod" tab. Drawing only: the state and every rule live in [`crate::project`].

use bevy::prelude::{Res, ResMut, Resource, Time};
use bevy_egui::egui;
use openprocyon::authoring::{Finding, Level};
use std::path::{Path, PathBuf};

use crate::project::layers::LayerKind;
use crate::project::problems::{self, Checker};
use crate::project::{Notice, Project};

/// What the user clicked in the problems list: later packages open the file and the field. `path` is the finding's dotted path.
#[derive(Resource, Default, Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub rel: String,
    pub path: String,
}

/// The open mod, its problems and the messages to show.
#[derive(Resource, Default)]
pub struct State {
    pub project: Option<Project>,
    checker: Checker,
    /// The latest `check_mod` result, errors first.
    pub findings: Vec<Finding>,
    notices: Vec<Notice>,
    pub(crate) status: String,
    seconds: f32,
    /// A change made with no mod open, waiting for the answer to "which mod?" (see [`crate::which_mod`]).
    pub ask: crate::which_mod::Asking,
}

impl State {
    /// Opens the mod folder `dir` (or makes one of it with `create`).
    pub fn open(&mut self, dir: &Path, import: Option<PathBuf>, create: bool) {
        let made = if create { Project::create(dir, import) } else { Project::open(dir, import).map(|p| (p, false)) };
        match made {
            Ok((p, created)) => {
                let status = match created {
                    true => format!("Made {} with a new mod.json and LICENSE.", dir.display()),
                    false => format!("Opened {}.", dir.display()),
                };
                self.install(p, status);
            }
            Err(e) => self.status = e,
        }
    }

    /// Makes `project` the open mod, showing `status`.
    pub fn install(&mut self, project: Project, status: String) {
        self.status = status;
        self.project = Some(project);
        self.notices.clear();
        self.findings.clear();
        self.check();
    }

    /// Starts the mod check on a thread (`authoring::check_mod`).
    pub fn check(&mut self) {
        if let Some(p) = &self.project {
            self.checker.start(p.dir.clone(), p.import.clone());
        }
    }

    /// Saves every changed document and checks again.
    pub fn save(&mut self) {
        let Some(p) = &mut self.project else { return };
        let report = p.save();
        self.status = match (report.written.len() + report.deleted.len(), report.errors.is_empty()) {
            (0, true) => "Nothing to save.".into(),
            (n, true) if report.deleted.is_empty() => format!("Saved {n} file{}.", if n == 1 { "" } else { "s" }),
            (n, true) => format!("Saved {n} file{} ({} deleted: back to retail).", if n == 1 { "" } else { "s" }, report.deleted.len()),
            (_, false) => report.errors.join("; "),
        };
        for w in report.warnings {
            self.notices.push(Notice { rel: String::new(), text: w, conflict: false });
        }
        self.check();
    }

    /// Called every frame: about once a second looks at the files on disk, and takes in a finished check.
    pub fn tick(&mut self, dt: f32) {
        if let Some(found) = self.checker.poll() {
            self.findings = found;
        }
        self.seconds += dt;
        if self.seconds < 1.0 {
            return;
        }
        self.seconds = 0.0;
        if let Some(p) = &mut self.project {
            let notes = p.poll();
            if !notes.is_empty() {
                self.notices.extend(notes.into_iter().filter(|n| !n.conflict));
                self.check();
            }
        }
    }
}

/// The Bevy system that drives [`State::tick`].
pub fn poll_system(time: Res<Time>, mut state: ResMut<State>) {
    state.tick(time.delta_secs());
}

/// Ctrl+S, Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y. Undo and redo are left to a focused text field.
pub fn shortcuts(ctx: &egui::Context, st: &mut State) {
    use egui::{Key, Modifiers};
    let typing = ctx.egui_wants_keyboard_input();
    let (save, redo, undo) = ctx.input_mut(|i| {
        let save = i.consume_key(Modifiers::COMMAND, Key::S);
        let redo = !typing && (i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y));
        let undo = !typing && i.consume_key(Modifiers::COMMAND, Key::Z);
        (save, redo, undo)
    });
    if save {
        st.save();
    }
    if let Some(p) = &mut st.project {
        if redo {
            p.redo();
        } else if undo {
            p.undo();
        }
    }
}

/// The left panel's mod section.
pub fn side(ui: &mut egui::Ui, st: &mut State, import: Option<&Path>) {
    ui.separator();
    ui.strong("Mod");
    ui.horizontal(|ui| {
        if ui.button("Open mod...").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            st.open(&dir, import.map(Path::to_path_buf), false);
        }
        if ui.button("New mod...").on_hover_text("Pick a folder; writes mod.json there unless it has one").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            st.open(&dir, import.map(Path::to_path_buf), true);
        }
    });
    if !st.status.is_empty() {
        ui.label(&st.status);
    }
    let Some(p) = &mut st.project else { return };
    ui.label(format!("{} ({})", p.name(), p.dir.display()));
    let unsaved = p.dirty_rels().len();
    let mut do_save = false;
    ui.horizontal(|ui| {
        do_save = ui.add_enabled(unsaved > 0, egui::Button::new(format!("Save (Ctrl+S) [{unsaved}]"))).clicked();
        let undo = p.undo_label().map(str::to_string);
        if ui.add_enabled(undo.is_some(), egui::Button::new("Undo")).on_hover_text(undo.unwrap_or_default()).clicked() {
            p.undo();
        }
        let redo = p.redo_label().map(str::to_string);
        if ui.add_enabled(redo.is_some(), egui::Button::new("Redo")).on_hover_text(redo.unwrap_or_default()).clicked() {
            p.redo();
        }
    });
    // Files changed on disk while they have unsaved edits.
    let conflicts: Vec<String> = p.docs.iter().filter(|d| d.conflict.is_some()).map(|d| d.rel.clone()).collect();
    let mut resolve: Option<(String, bool)> = None;
    for rel in &conflicts {
        ui.colored_label(egui::Color32::YELLOW, format!("{rel} changed on disk and has unsaved edits"));
        ui.horizontal(|ui| {
            if ui.button("Reload from disk").clicked() {
                resolve = Some((rel.clone(), true));
            }
            if ui.button("Keep my edits").clicked() {
                resolve = Some((rel.clone(), false));
            }
        });
    }
    let mut error = None;
    if let Some((rel, reload)) = resolve {
        let r = if reload { p.resolve_reload(&rel) } else { p.resolve_keep(&rel) };
        error = r.err();
    }
    let rows = p.layer_rows();
    if let Some(e) = error {
        st.status = e;
    }
    let mut dismiss = None;
    for (i, n) in st.notices.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(&n.text);
            if ui.small_button("x").clicked() {
                dismiss = Some(i);
            }
        });
    }
    if let Some(i) = dismiss {
        st.notices.remove(i);
    }
    let mut revert: Option<String> = None;
    egui::CollapsingHeader::new(format!("Layers ({})", rows.len())).default_open(true).show(ui, |ui| {
        if rows.is_empty() {
            ui.label("The mod has no files yet.");
        }
        for r in &rows {
            let color = match r.kind {
                LayerKind::Skipped => egui::Color32::LIGHT_RED,
                LayerKind::Unsaved => egui::Color32::YELLOW,
                _ => ui.visuals().text_color(),
            };
            let text = if r.note.is_empty() { format!("{}: {}", r.kind.label(), r.file) } else { format!("{}: {} ({})", r.kind.label(), r.file, r.note) };
            ui.horizontal(|ui| {
                ui.colored_label(color, text);
                let can = matches!(r.kind, LayerKind::Extends | LayerKind::Merged | LayerKind::Unsaved) && p.open_doc(&r.file).is_ok_and(|i| p.docs[i].can_revert());
                if can
                    && ui
                        .small_button("Revert file")
                        .on_hover_text("Take every change of the mod in this file back (one undo step); saving deletes the file when nothing is left")
                        .clicked()
                {
                    revert = Some(r.file.clone());
                }
            });
        }
    });
    if let Some(rel) = revert
        && let Err(e) = p.revert_file(&rel)
    {
        st.status = e;
    }
    if do_save {
        st.save();
    }
}

/// The bottom panel's mod problems: errors first, a click sets [`Focus`].
pub fn problems_panel(ui: &mut egui::Ui, st: &mut State, focus: &mut Focus) {
    let (errors, warnings) = problems::counts(&st.findings);
    ui.horizontal(|ui| {
        ui.strong(format!("Problems: {errors} errors, {warnings} warnings"));
        if st.checker.running() {
            ui.label("checking...");
        }
        if ui.add_enabled(st.project.is_some() && !st.checker.running(), egui::Button::new("Check")).clicked() {
            st.check();
        }
    });
    egui::ScrollArea::vertical().id_salt("mod-problems").max_height(140.0).show(ui, |ui| {
        for f in &st.findings {
            let color = if f.level == Level::Error { egui::Color32::LIGHT_RED } else { egui::Color32::YELLOW };
            let place = match (f.file.is_empty(), f.path.is_empty()) {
                (true, _) => String::new(),
                (false, true) => format!("{}: ", f.file),
                (false, false) => format!("{} {}: ", f.file, f.path),
            };
            let text = egui::RichText::new(format!("{place}{}", f.message)).color(color);
            if ui.add(egui::Label::new(text).sense(egui::Sense::click())).clicked() && !f.file.is_empty() {
                *focus = Focus { rel: f.file.clone(), path: f.path.clone() };
            }
            if let Some(s) = suggestion_line(f) {
                ui.label(s);
            }
        }
    });
}

/// The "did you mean" line under a finding, only when its message does not say so already (the engine puts it in both).
pub fn suggestion_line(f: &Finding) -> Option<String> {
    let s = f.suggestion.as_deref()?;
    (!f.message.contains("did you mean")).then(|| format!("    did you mean \"{s}\"?"))
}

/// The "Mod" tab: `mod.json`.
pub fn tab(ui: &mut egui::Ui, st: &mut State) {
    ui.heading("Mod");
    let Some(p) = &mut st.project else {
        ui.label("Open or make a mod folder in the left panel.");
        return;
    };
    match p.open_doc("mod.json") {
        Ok(i) => {
            ui.label(format!("{}/mod.json{}", p.dir.display(), if p.docs[i].dirty { " (unsaved)" } else { "" }));
            let schema = mod_schema();
            let notes = notes_for(&st.findings, "mod.json");
            let (edits, typing) = crate::form::show_typed(
                ui,
                &schema,
                &p.docs[i].edited,
                &crate::form::FormCtx { base: None, findings: &notes, focus: None, hidden: &[] },
                egui::Id::new("mod.json form"),
            );
            if !edits.is_empty()
                && let Err(e) = p.apply_form("mod.json", "Edit mod.json", edits, typing)
            {
                st.status = e;
            }
        }
        Err(e) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
    }
}

/// The engine's schema of `mod.json` (`authoring::schemas`).
pub fn mod_schema() -> serde_json::Value {
    schema_named("mod.schema.json")
}

/// One of the engine's schemas by file name (`ship.schema.json`, `guns.schema.json`, ...); `Null` when the engine has no such schema.
pub fn schema_named(name: &str) -> serde_json::Value {
    openprocyon::authoring::schemas().into_iter().find(|(n, _)| *n == name).map(|(_, s)| s).unwrap_or_default()
}

/// The findings about file `rel` as form notes, located by JSON pointer (`edit::dotted_to_pointer`, PROVISIONAL until findings carry one).
pub fn notes_for(findings: &[Finding], rel: &str) -> Vec<crate::form::FieldNote> {
    findings
        .iter()
        .filter(|f| f.file == rel)
        .map(|f| crate::form::FieldNote { pointer: crate::edit::dotted_to_pointer(&f.path), level: f.level, message: f.message.clone() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_you_mean_is_printed_once() {
        let mut f = Finding::new(Level::Error, "", "", "", "gun 'Gunn' is not in guns.json (did you mean 'Gun'?)");
        f.suggestion = Some("Gun".into());
        assert_eq!(suggestion_line(&f), None, "the engine's message has it already");
        f.message = "gun 'Gunn' is not in guns.json".into();
        assert_eq!(suggestion_line(&f).as_deref(), Some("    did you mean \"Gun\"?"));
        f.suggestion = None;
        assert_eq!(suggestion_line(&f), None);
    }
}
