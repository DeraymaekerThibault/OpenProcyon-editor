//! "Save this change into which mod?": with no mod open, a retail ship, gun or bullet can still be edited. The first edit is held
//! here as a [`Pending`] change and the user picks a mod folder (open one, or make a new one); the project opens and the SAME edit is
//! applied through it, so it saves as a diff like any other. Cancel drops the edit. Retail is never written (docs/roadmap.md B6).
//! The state machine ([`choose`]) is pure apart from opening the folder and is tested headless; [`window`] only draws.

use std::path::{Path, PathBuf};

use bevy_egui::egui;

use crate::edit::Edit;
use crate::project::licence::Licence;
use crate::project::{NewMod, Project};
use crate::project_panel::State;

/// A change made with no mod open: edits to document `rel`, as one undo step labelled `label`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub rel: String,
    pub label: String,
    pub edits: Vec<Edit>,
}

/// The user's answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    /// An existing mod folder.
    Open(PathBuf),
    /// A new mod in `dir` (made if needed): its id, licence and the holder named in the `LICENSE`.
    New { dir: PathBuf, id: String, licence: Licence, author: String },
    /// Drop the change.
    Cancel,
}

/// The question on screen: the held change and the "new mod" form.
#[derive(Clone, Debug, Default)]
pub struct Asking {
    pub pending: Option<Pending>,
    /// `Some` once "New mod..." was pressed.
    pub form: Option<NewForm>,
}

#[derive(Clone, Debug, Default)]
pub struct NewForm {
    pub dir: Option<PathBuf>,
    pub id: String,
    pub licence: Licence,
    pub author: String,
}

/// Holds `pending` and starts the question, unless one is already open (then the new change is dropped: the user answers first).
pub fn propose(st: &mut State, pending: Pending) {
    if st.ask.pending.is_none() {
        st.ask = Asking { pending: Some(pending), form: None };
    }
}

/// Answers the question: opens or makes the mod, then applies the held change through it (one undo step). Returns the line to show.
/// Nothing is written when the answer is `Cancel`, or when the mod cannot be opened (the change stays held so another folder can be tried).
pub fn choose(st: &mut State, choice: Choice, import: Option<&Path>) -> Result<String, String> {
    let Some(pending) = st.ask.pending.clone() else { return Err("no change is waiting".into()) };
    let import = import.map(Path::to_path_buf);
    let (project, made) = match choice {
        Choice::Cancel => {
            st.ask = Asking::default();
            return Ok("The change was dropped.".into());
        }
        Choice::Open(dir) => (Project::open(&dir, import)?, false),
        Choice::New { dir, id, licence, author } => {
            let folders: Vec<String> = Path::new(&pending.rel).parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_string_lossy().into_owned()).into_iter().collect();
            let (p, created) = Project::create_with(&dir, import, &NewMod { id: Some(id), licence, author, folders })?;
            (p, created)
        }
    };
    let dir = project.dir.clone();
    st.install(project, if made { format!("Made {} with a new mod.json and LICENSE.", dir.display()) } else { format!("Opened {}.", dir.display()) });
    st.ask = Asking::default();
    let p = st.project.as_mut().expect("just installed");
    p.do_edits(&pending.rel, &pending.label, pending.edits)?;
    Ok(format!("{} The change is in the mod; save to write it.", if made { "New mod made." } else { "Mod opened." }))
}

/// Draws the question while one is open and carries out the answer.
pub fn window(ctx: &egui::Context, st: &mut State, import: Option<&Path>) {
    let Some(pending) = st.ask.pending.clone() else { return };
    let mut answer: Option<Choice> = None;
    let mut form = st.ask.form.clone();
    let mut back = false;
    egui::Window::new("Save this change into which mod?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.label(format!("{}. The game's own files are never changed: the change is saved in a mod, as a difference.", pending.label));
        match &mut form {
            None => {
                ui.horizontal(|ui| {
                    if ui.button("Open mod folder...").clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        answer = Some(Choice::Open(dir));
                    }
                    if ui.button("New mod...").clicked() {
                        form = Some(NewForm::default());
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(Choice::Cancel);
                    }
                });
            }
            Some(f) => {
                ui.horizontal(|ui| {
                    ui.label("Folder");
                    ui.label(f.dir.as_ref().map_or("(none picked)".to_string(), |d| d.display().to_string()));
                    if ui.button("Pick...").clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        if f.id.is_empty() {
                            f.id = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        }
                        f.dir = Some(dir);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Id");
                    ui.text_edit_singleline(&mut f.id);
                });
                ui.horizontal(|ui| {
                    ui.label("Author");
                    ui.text_edit_singleline(&mut f.author);
                });
                ui.horizontal(|ui| {
                    ui.label("Licence");
                    egui::ComboBox::from_id_salt("which-mod-licence").selected_text(f.licence.label()).show_ui(ui, |ui| {
                        for l in Licence::ALL {
                            ui.selectable_value(&mut f.licence, l, l.label());
                        }
                    });
                });
                ui.weak("LICENSE is a notice, not the full text: replace it before you share the mod (PROVISIONAL).");
                ui.horizontal(|ui| {
                    if ui.add_enabled(f.dir.is_some() && !f.id.trim().is_empty(), egui::Button::new("Make mod")).clicked()
                        && let Some(dir) = f.dir.clone()
                    {
                        answer = Some(Choice::New { dir, id: f.id.clone(), licence: f.licence, author: f.author.clone() });
                    }
                    back = ui.button("Back").clicked();
                    if ui.button("Cancel").clicked() {
                        answer = Some(Choice::Cancel);
                    }
                });
            }
        }
    });
    st.ask.form = if back { None } else { form };
    if let Some(choice) = answer {
        st.status = match choose(st, choice, import) {
            Ok(s) | Err(s) => s,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-whichmod-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn import(root: &Path) -> PathBuf {
        let a = root.join("assets");
        std::fs::create_dir_all(a.join("ships/Ship_X")).unwrap();
        std::fs::write(a.join("ships/Ship_X/ship.json"), r#"{"name": "X", "speed": 3}"#).unwrap();
        a
    }

    fn pending() -> Pending {
        Pending { rel: "ships/Ship_X/ship.json".into(), label: "Edit Ship_X".into(), edits: vec![Edit::set("/speed", json!(9))] }
    }

    #[test]
    fn a_new_mod_is_made_and_the_same_edit_saves_as_a_diff() {
        let root = tmp("new");
        let a = import(&root);
        let mut st = State::default();
        propose(&mut st, pending());
        propose(&mut st, Pending { rel: "x".into(), label: "other".into(), edits: vec![] });
        assert_eq!(st.ask.pending.as_ref().unwrap().rel, "ships/Ship_X/ship.json", "a second change waits for the first answer");
        let dir = root.join("my-mod");
        choose(&mut st, Choice::New { dir: dir.clone(), id: "my-mod".into(), licence: Licence::Cc0, author: "Ada".into() }, Some(&a)).unwrap();
        assert!(st.ask.pending.is_none());
        assert!(dir.join("LICENSE").is_file() && dir.join("mod.json").is_file() && dir.join("ships/Ship_X").is_dir());
        assert!(!dir.join("ships/Ship_X/ship.json").exists(), "nothing is written before a save, and never a copy of the ship");
        let p = st.project.as_mut().unwrap();
        assert_eq!(p.dirty_rels(), ["ships/Ship_X/ship.json"]);
        assert_eq!(p.undo_label(), Some("Edit Ship_X"));
        let report = p.save();
        assert!(report.errors.is_empty(), "{report:?}");
        let file: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("ships/Ship_X/ship.json")).unwrap()).unwrap();
        assert_eq!(file, json!({"extends": "Ship_X", "speed": 9}));
        assert_eq!(std::fs::read_to_string(a.join("ships/Ship_X/ship.json")).unwrap(), r#"{"name": "X", "speed": 3}"#, "retail is untouched");
        st.project = None;
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn opening_a_mod_applies_the_edit_and_cancel_and_a_bad_folder_do_not() {
        let root = tmp("open");
        let a = import(&root);
        let m = root.join("mod");
        std::fs::create_dir_all(&m).unwrap();
        let mut st = State::default();
        propose(&mut st, pending());
        assert!(choose(&mut st, Choice::Open(root.join("nowhere")), Some(&a)).is_err());
        assert!(st.ask.pending.is_some() && st.project.is_none(), "the change stays held after a failure");
        choose(&mut st, Choice::Open(m.clone()), Some(&a)).unwrap();
        assert_eq!(st.project.as_ref().unwrap().dirty_rels(), ["ships/Ship_X/ship.json"]);
        st.project = None;
        propose(&mut st, pending());
        assert_eq!(choose(&mut st, Choice::Cancel, Some(&a)).unwrap(), "The change was dropped.");
        assert!(st.ask.pending.is_none() && st.project.is_none());
        assert!(choose(&mut st, Choice::Cancel, Some(&a)).is_err(), "nothing is waiting");
        // a mod folder cannot be made inside the import
        propose(&mut st, pending());
        assert!(choose(&mut st, Choice::New { dir: a.join("mod"), id: "m".into(), licence: Licence::Cc0, author: String::new() }, Some(&a)).is_err());
        assert!(!a.join("mod").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
