//! The "Missions" tab section: speech, messages and ship names (docs/phase5.md T10), the three tables of a mission's `media.json`.
//! The table operations are [`super::media`]'s (one undo step each); this module adds what is specific to a mission:
//!
//! - a new `media.json` for a mission that has none ([`ensure_media`], one undo step with the first entry);
//! - renaming an entry and every rule field that names it as ONE `do_group` step ([`rename_entry`], kinds `speech_event`, `message`,
//!   `ship_name` of [`super::refs`]; a message id also follows `victory_message` / `defeat_message`);
//! - the sound of a speech line: copying the author's `.ogg` to `maps/<id>/speech/` through the guard ([`import_sound`]) and reading
//!   its length into `duration` ([`read_duration`], by the engine's `authoring::sound_duration`: the game's own decoder);
//! - the portrait picker: the `TalkingHead_Character_*` folders under `hud/sprites` of the import and the mod ([`faces`]).
//!
//! A speech file is found by the game by its name without extension among every sound of all active content, so the name must be
//! unique: [`suggest_file`] prefixes the mission id (docs/missions.md section 5). A mod folder that is not a game import only; nothing
//! here writes into the import (the guard refuses).

use std::path::Path;

use bevy_egui::egui;
use serde_json::{Value, json};

use super::doc::{self as mdoc, MediaTable};
use super::media;
use super::project::{self as mproject, media_rel, mission_rel};
use super::refs::rename_edits;
use crate::edit::Edit;
use crate::project::{Project, guard};

/// The sprite folders that are portraits (`hud/sprites/TalkingHead_Character_*`, docs/missions.md section 5; PROVISIONAL as the
/// engine documents the folder, not a list).
pub const FACE_PREFIX: &str = "TalkingHead_Character_";

/// The reference kind of the names of a table ([`super::refs::NAME_KINDS`]).
pub fn kind(table: MediaTable) -> &'static str {
    match table {
        MediaTable::Speech => "speech_event",
        MediaTable::Messages => "message",
        MediaTable::ShipNames => "ship_name",
    }
}

/// Makes the mission's `media.json` when it has none (a new, empty document, written by the next save), so the first entry has
/// somewhere to go. The new file and the entry are two steps; Undo takes the entry, then the file. Returns the document's path.
pub fn ensure_media(project: &mut Project, id: &str) -> Result<String, String> {
    let rel = media_rel(id)?;
    if project.open_doc(&rel).is_err() {
        project.open_new_doc(&rel, None, Value::Null)?;
        project.do_step(&rel, "New media.json", vec![Edit::set("", mdoc::new_media())])?;
    }
    Ok(rel)
}

fn table_has(project: &Project, rel: &str, table: MediaTable, name: &str) -> bool {
    project.doc(rel).is_some_and(|d| media::entries(&d.edited, table).iter().any(|(k, _)| *k == name))
}

/// Adds an entry (a speech event with an empty text, a message or a ship name with an empty string).
pub fn add_entry(project: &mut Project, id: &str, table: MediaTable, name: &str) -> Result<(), String> {
    let rel = ensure_media(project, id)?;
    media::add(project, &rel, table, name)
}

/// Removes an entry. A `media.json` that extends another cannot lose a key (docs/phase5.md P1): the error says so. The rules that
/// name it keep the name (the problems list says so).
pub fn remove_entry(project: &mut Project, id: &str, table: MediaTable, name: &str) -> Result<(), String> {
    let rel = media_rel(id)?;
    project.open_doc(&rel)?;
    mproject::check_remove_key(project, &rel)?;
    media::remove(project, &rel, table, name)
}

/// Sets a message or a ship name (`field` `None`), or the `field` of a speech entry. Typing joins one step.
pub fn set_field(project: &mut Project, id: &str, table: MediaTable, name: &str, field: Option<&str>, value: Option<Value>) -> Result<(), String> {
    let rel = media_rel(id)?;
    project.open_doc(&rel)?;
    if !table_has(project, &rel, table, name) {
        return Err(format!("{} has no '{name}'", table.key()));
    }
    media::set(project, &rel, table, name, field, value)
}

/// Sets the `text` of a speech event.
pub fn set_text(project: &mut Project, id: &str, event: &str, text: &str) -> Result<(), String> {
    set_field(project, id, MediaTable::Speech, event, Some(mdoc::SPEECH_TEXT), Some(json!(text)))
}

/// Sets the `file` of a speech event (empty: the key is removed, so the event's own name is the file).
pub fn set_file(project: &mut Project, id: &str, event: &str, file: &str) -> Result<(), String> {
    let value = if file.is_empty() { None } else { Some(json!(file)) };
    set_field(project, id, MediaTable::Speech, event, Some(mdoc::SPEECH_FILE), value)
}

/// Sets the `face` of a speech event (empty: no portrait).
pub fn set_face(project: &mut Project, id: &str, event: &str, face: &str) -> Result<(), String> {
    let value = if face.is_empty() { None } else { Some(json!(face)) };
    set_field(project, id, MediaTable::Speech, event, Some(mdoc::SPEECH_FACE), value)
}

/// Sets the `duration` of a speech event in seconds (`None`: a text-only line). Refused for a negative or non-finite number.
pub fn set_duration(project: &mut Project, id: &str, event: &str, seconds: Option<f64>) -> Result<(), String> {
    let value = match seconds {
        Some(s) if !s.is_finite() || s < 0.0 => return Err("a duration is a number of seconds, 0 or more".into()),
        Some(s) => Some(json!(s)),
        None => None,
    };
    set_field(project, id, MediaTable::Speech, event, Some(mdoc::SPEECH_DURATION), value)
}

/// Renames an entry and every rule field that names it, as ONE undo step ([`Project::do_group`]). Returns the number of rule fields
/// that followed. The mission's `victory_message` / `defeat_message` follow a renamed message. Refused for an empty or repeated name
/// and for a `media.json` that extends another (it cannot remove the old key).
pub fn rename_entry(project: &mut Project, id: &str, table: MediaTable, old: &str, new: &str) -> Result<usize, String> {
    let new = new.trim();
    let (md, ms) = (media_rel(id)?, mission_rel(id)?);
    project.open_doc(&md)?;
    if new.is_empty() {
        return Err("name the entry first".into());
    }
    if old == new {
        return Ok(0);
    }
    if table_has(project, &md, table, new) {
        return Err(format!("{} has '{new}' already", table.key()));
    }
    let value = project
        .doc(&md)
        .and_then(|d| media::entries(&d.edited, table).into_iter().find(|(k, _)| *k == old).map(|(_, v)| v.clone()))
        .ok_or_else(|| format!("{} has no '{old}'", table.key()))?;
    mproject::check_remove_key(project, &md)?;
    let media_edits = vec![Edit::remove(mdoc::media_entry_pointer(table, old)), Edit::set(mdoc::media_entry_pointer(table, new), value)];
    let mut mission_edits = vec![];
    if project.open_doc(&ms).is_ok()
        && let Some(m) = project.doc(&ms).map(|d| &d.edited).filter(|v| !v.is_null())
    {
        mission_edits = rename_edits(m, kind(table), old, new)?;
        if table == MediaTable::Messages {
            for key in [mdoc::KEY_VICTORY_MESSAGE, mdoc::KEY_DEFEAT_MESSAGE] {
                if m.get(key).and_then(Value::as_str) == Some(old) {
                    mission_edits.push(Edit::set(mdoc::top_pointer(key), json!(new)));
                }
            }
        }
    }
    let n = mission_edits.len();
    project.do_group(&format!("Rename {} {old}", kind(table)), vec![(md, media_edits), (ms, mission_edits)])?;
    Ok(n)
}

/// The file name a speech line of `event` is suggested to have: the mission id as a prefix, then the event in lower case, letters,
/// digits and `_` only (the game finds a sound by name among all content, so it must be unique; docs/missions.md section 5).
pub fn suggest_file(id: &str, event: &str) -> String {
    let tail: String = event.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    format!("{id}_{}", tail.trim_matches('_'))
}

/// The speech file of an event inside the mod (`maps/<id>/speech/<file>.ogg`), whether it exists or not.
pub fn sound_path(project: &Project, id: &str, event: &str) -> std::path::PathBuf {
    let entry =
        project.doc(&media_rel(id).unwrap_or_default()).and_then(|d| media::entries(&d.edited, MediaTable::Speech).into_iter().find(|(k, _)| *k == event).map(|(_, v)| v.clone()));
    project.dir.join(mdoc::speech_dir(id)).join(mdoc::speech_file_name(event, &entry.unwrap_or(Value::Null)))
}

/// The length in seconds of the sound file at `path`, as the game's own decoder reads it (`openprocyon::authoring::sound_duration`).
/// The engine decodes to the end, so the answer is kept by path, modification time and size: the speech rows ask every frame.
pub fn sound_seconds(path: &Path) -> Result<f64, String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Entry = (Option<std::time::SystemTime>, u64, Result<f64, String>);
    static CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, Entry>>> = OnceLock::new();
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let stamp = (meta.modified().ok(), meta.len());
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(c) = cache.lock()
        && let Some((m, l, r)) = c.get(path)
        && (*m, *l) == stamp
    {
        return r.clone();
    }
    let r = openprocyon::authoring::sound_duration(path);
    if let Ok(mut c) = cache.lock() {
        c.insert(path.to_path_buf(), (stamp.0, stamp.1, r.clone()));
    }
    r
}

/// Reads the length of the event's `.ogg` in the mod ([`sound_seconds`], rounded to the hundredth of a second) and writes it as the
/// event's `duration`: one undo step. Returns the seconds. The game adds 0.3 s itself, so the length is written as it is.
pub fn read_duration(project: &mut Project, id: &str, event: &str) -> Result<f64, String> {
    let path = sound_path(project, id, event);
    let seconds = (sound_seconds(&path)? * 100.0).round() / 100.0;
    let rel = media_rel(id)?;
    project.open_doc(&rel)?;
    let at = mdoc::media_entry_pointer(MediaTable::Speech, event);
    if !table_has(project, &rel, MediaTable::Speech, event) {
        return Err(format!("speech has no '{event}'"));
    }
    project.do_step(&rel, "Set speech duration", vec![Edit::set(format!("{at}/{}", mdoc::SPEECH_DURATION), json!(seconds))])?;
    Ok(seconds)
}

/// Copies the author's `.ogg` `source` into the mod as `maps/<id>/speech/<name>.ogg` (through the guard: never into the import) and
/// points the event at it (`file`) as one undo step; returns the path inside the mod.
/// `name` is a plain file name without extension ([`suggest_file`]); an existing file is not overwritten, and the copy must look like
/// an Ogg file (`OggS`). The `duration` is left to [`read_duration`].
pub fn import_sound(project: &mut Project, id: &str, event: &str, source: &Path, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) || name.starts_with('.') {
        return Err(format!("'{name}' is not a sound name (letters, digits, _ - and . only)"));
    }
    if !source.extension().is_some_and(|e| e.eq_ignore_ascii_case("ogg")) {
        return Err(format!("{}: pick an .ogg file", source.display()));
    }
    let head = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if !head.starts_with(b"OggS") {
        return Err(format!("{}: not an Ogg file", source.display()));
    }
    let rel = media_rel(id)?;
    project.open_doc(&rel)?;
    if !table_has(project, &rel, MediaTable::Speech, event) {
        return Err(format!("speech has no '{event}'"));
    }
    let dir = project.dir.join(mdoc::speech_dir(id));
    let dest = dir.join(format!("{name}.ogg"));
    if dest.exists() {
        return Err(format!("{} exists already: pick another name", dest.display()));
    }
    guard::create_dir_all(&dir, project.import.as_deref())?;
    guard::copy(source, &dest, project.import.as_deref())?;
    let at = mdoc::media_entry_pointer(MediaTable::Speech, event);
    let current = project.doc(&rel).and_then(|d| d.edited.pointer(&format!("{at}/{}", mdoc::SPEECH_FILE))).and_then(Value::as_str).map(str::to_string);
    if current.as_deref() != Some(name) {
        project.do_step(&rel, "Set speech file", vec![Edit::set(format!("{at}/{}", mdoc::SPEECH_FILE), json!(name))])?;
    }
    Ok(format!("{}/{name}.ogg", mdoc::speech_dir(id)))
}

/// The portrait names offered: the `TalkingHead_Character_*` folders under `hud/sprites` of the mod and the import, sorted.
pub fn faces(project: &Project) -> Vec<String> {
    let mut out = vec![];
    for root in std::iter::once(project.dir.as_path()).chain(project.import.as_deref()) {
        if let Ok(rd) = std::fs::read_dir(root.join("hud/sprites")) {
            out.extend(rd.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.starts_with(FACE_PREFIX)));
        }
    }
    out.sort();
    out.dedup();
    out
}

// ---- the section ----

/// What one frame asks for, applied after drawing.
enum Action {
    Add(MediaTable, String),
    Remove(MediaTable, String),
    Rename(MediaTable, String, String),
    Field(MediaTable, String, Option<&'static str>, Option<Value>),
    FromFile(String),
    Import(String, String, String),
}

/// Draws the section.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    let Some(id) = super::rules_view::current_mission(ui, project) else { return };
    let media_doc = mproject::merged_media(project, &id).cloned().unwrap_or(Value::Null);
    let faces = faces(project);
    let root = egui::Id::new(("mission-speech", id.as_str()));
    let mut actions = vec![];
    if media_doc.is_null() {
        ui.label("This mission has no media.json yet; the first entry makes it.");
    }
    for table in MediaTable::ALL {
        let title = match table {
            MediaTable::Speech => "Speech",
            MediaTable::Messages => "Messages (end texts)",
            MediaTable::ShipNames => "Ship names",
        };
        ui.strong(title);
        for (name, value) in media::entries(&media_doc, table) {
            let eid = root.with((table.key(), name));
            ui.horizontal(|ui| match table {
                MediaTable::Speech => {}
                _ => {
                    ui.label(name);
                    let current = value.as_str().unwrap_or("");
                    let mut text = current.to_string();
                    ui.add(egui::TextEdit::singleline(&mut text).desired_width(280.0));
                    if text != current {
                        actions.push(Action::Field(table, name.to_string(), None, Some(json!(text))));
                    }
                    rename_box(ui, eid, table, name, &mut actions);
                    if ui.small_button("Remove").clicked() {
                        actions.push(Action::Remove(table, name.to_string()));
                    }
                }
            });
            if table == MediaTable::Speech {
                speech_row(ui, eid, &id, name, value, &faces, project, &mut actions);
            }
        }
        add_box(ui, root.with(("add", table.key())), table, &mut actions);
        ui.separator();
    }
    for a in actions {
        let result = match a {
            Action::Add(t, n) => add_entry(project, &id, t, &n),
            Action::Remove(t, n) => remove_entry(project, &id, t, &n),
            Action::Rename(t, o, n) => rename_entry(project, &id, t, &o, &n).map(|_| ()),
            Action::Field(t, n, f, v) => set_field(project, &id, t, &n, f, v),
            Action::FromFile(e) => read_duration(project, &id, &e).map(|_| ()),
            Action::Import(e, src, name) => import_sound(project, &id, &e, Path::new(&src), &name).and_then(|_| read_duration(project, &id, &e).map(|_| ())),
        };
        ui.data_mut(|d| match result {
            Ok(()) => {
                d.remove_temp::<String>(root.with("error"));
            }
            Err(e) => {
                d.insert_temp(root.with("error"), e);
            }
        });
    }
    if let Some(e) = ui.data(|d| d.get_temp::<String>(root.with("error"))) {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
}

fn add_box(ui: &mut egui::Ui, id: egui::Id, table: MediaTable, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let mut name: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("new name").desired_width(160.0));
        if ui.button("Add").clicked() || crate::layout::entered(ui, &r) {
            actions.push(Action::Add(table, name.clone()));
            name.clear();
        }
        ui.data_mut(|d| d.insert_temp(id, name));
    });
}

fn rename_box(ui: &mut egui::Ui, id: egui::Id, table: MediaTable, name: &str, actions: &mut Vec<Action>) {
    ui.menu_button("Rename", |ui| {
        let mut to: String = ui.data(|d| d.get_temp(id.with("rename"))).unwrap_or_else(|| name.to_string());
        ui.add(egui::TextEdit::singleline(&mut to).desired_width(160.0));
        if ui.button("Rename and follow the rules").clicked() {
            actions.push(Action::Rename(table, name.to_string(), to.clone()));
            ui.close();
        }
        ui.data_mut(|d| d.insert_temp(id.with("rename"), to));
    });
}

#[allow(clippy::too_many_arguments)]
fn speech_row(ui: &mut egui::Ui, id: egui::Id, mission: &str, name: &str, entry: &Value, faces: &[String], project: &Project, actions: &mut Vec<Action>) {
    egui::CollapsingHeader::new(name).id_salt(id).show(ui, |ui| {
        let table = MediaTable::Speech;
        let field = |actions: &mut Vec<Action>, f: &'static str, v: Option<Value>| actions.push(Action::Field(table, name.to_string(), Some(f), v));
        let text = mdoc::speech_text(entry).unwrap_or("");
        let mut edited = text.to_string();
        ui.horizontal(|ui| {
            ui.label("Text");
            ui.add(egui::TextEdit::multiline(&mut edited).desired_rows(2).desired_width(360.0));
        });
        if edited != text {
            field(actions, mdoc::SPEECH_TEXT, Some(json!(edited)));
        }
        let file = mdoc::speech_file(entry).unwrap_or("");
        let mut f = file.to_string();
        ui.horizontal(|ui| {
            ui.label("File");
            ui.add(egui::TextEdit::singleline(&mut f).hint_text(format!("{name} (the event's name)")).desired_width(220.0));
            ui.label(".ogg");
            let there = sound_path(project, mission, name).is_file();
            ui.label(if there { "in the mod" } else { "not in the mod" });
            if there {
                match sound_seconds(&sound_path(project, mission, name)) {
                    Ok(s) => ui.label(format!("plays {s:.2} s")),
                    Err(e) => ui.colored_label(ui.visuals().warn_fg_color, "cannot be played").on_hover_text(e),
                };
            }
        });
        if f != file {
            field(actions, mdoc::SPEECH_FILE, (!f.is_empty()).then(|| json!(f)));
        }
        ui.horizontal(|ui| {
            ui.label("Duration");
            let mut seconds = mdoc::speech_duration(entry);
            let mut has = seconds.is_some();
            if ui.checkbox(&mut has, "spoken").on_hover_text("Without a duration the line is text only: no sound").changed() {
                field(actions, mdoc::SPEECH_DURATION, has.then(|| json!(0.0)));
            } else if let Some(s) = seconds.as_mut() {
                let before = *s;
                ui.add(egui::DragValue::new(s).speed(0.05).range(0.0..=600.0).suffix(" s"));
                if *s != before {
                    field(actions, mdoc::SPEECH_DURATION, Some(json!(*s)));
                }
            }
            if ui.button("Read from the .ogg").on_hover_text("The length of the file in the mod, as the game decodes it").clicked() {
                actions.push(Action::FromFile(name.to_string()));
            }
        });
        // Copy the author's own file into the mod.
        let src_id = id.with("source");
        let mut src: String = ui.data(|d| d.get_temp(src_id)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Copy in");
            ui.add(egui::TextEdit::singleline(&mut src).hint_text("path of your .ogg").desired_width(260.0));
            if ui.button("Copy to the mod").clicked() {
                actions.push(Action::Import(name.to_string(), src.clone(), suggest_file(mission, name)));
            }
        });
        ui.data_mut(|d| d.insert_temp(src_id, src));
        let face = mdoc::speech_face(entry).unwrap_or("");
        let mut chosen = face.to_string();
        ui.horizontal(|ui| {
            ui.label("Face");
            egui::ComboBox::from_id_salt(id.with("face")).selected_text(if chosen.is_empty() { "none" } else { chosen.as_str() }.to_string()).show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, String::new(), "none");
                for fc in faces {
                    ui.selectable_value(&mut chosen, fc.clone(), fc);
                }
            });
        });
        if chosen != face {
            field(actions, mdoc::SPEECH_FACE, (!chosen.is_empty()).then(|| json!(chosen)));
        }
        rename_box(ui, id, table, name, actions);
        if ui.button("Remove line").clicked() {
            actions.push(Action::Remove(table, name.to_string()));
        }
    });
}
