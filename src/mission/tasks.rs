//! The "Missions" tab section: tasks and the end (docs/phase5.md T8). The objectives of one mission (`tasks`: name, text, active,
//! required, and the `complete_when` / `fail_when` condition lists, drawn with the schema forms of [`crate::schema_form`]) and how
//! the mission ends (`victory`, `victory_message`, `defeat_message`).
//!
//! The functions below are the edits, one undo step each (typing in one box joins the step before it), named by the mission id; the
//! document is `maps/<id>/mission.json`. The keys are [`super::doc`]'s. Renaming a task renames the rule and task fields that name it
//! ([`super::refs::rename_edits`], kind `task`) in the same step. Adding and deleting a task goes through [`super::project`], which
//! refuses on a mission that extends another (`tasks` is not a keyed list of the engine, so a change restates the whole list). On
//! such a mission a key that cannot be removed (`victory`, the messages) is written empty instead (PROVISIONAL: the engine reads
//! `victory` as `"objectives"` or anything else, and a message id that is no entry falls back to the default text).

use std::sync::OnceLock;

use bevy_egui::egui;
use serde_json::Value;

use super::doc::{self as mdoc, MissionList};
use super::project::{self as mproject, mission_rel};
use super::refs::{FieldIndex, rename_edits};
use crate::edit::Edit;
use crate::project::Project;
use crate::schema_form::{self, Names, Schema};

fn mission_doc<'a>(project: &'a Project, id: &str) -> Result<&'a Value, String> {
    let rel = mission_rel(id)?;
    project.doc(&rel).map(|d| &d.edited).filter(|v| !v.is_null()).ok_or_else(|| format!("mission '{id}' is not open"))
}

fn task_at(mission: &Value, index: usize) -> Result<&Value, String> {
    mdoc::tasks(mission).get(index).ok_or_else(|| format!("no task {index}"))
}

fn check_key(key: &str) -> Result<(), String> {
    if key == mdoc::TASK_COMPLETE_WHEN || key == mdoc::TASK_FAIL_WHEN { Ok(()) } else { Err(format!("'{key}' is not a task condition list")) }
}

/// Adds a task named `name` (trimmed; not empty, not the name of another task) as one undo step and returns its index.
pub fn add_task(project: &mut Project, id: &str, name: &str) -> Result<usize, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name the task first".into());
    }
    if mdoc::tasks(mission_doc(project, id)?).iter().any(|t| mdoc::task_name(t) == Some(name)) {
        return Err(format!("there is a task '{name}' already"));
    }
    mproject::add_entry(project, id, MissionList::Tasks, mdoc::new_task(name))
}

/// Deletes task `index` as one undo step. The rules that name it keep the name (the problems list says so).
pub fn delete_task(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    mproject::delete_entry(project, id, MissionList::Tasks, index).map(|_| ())
}

/// Renames task `index` and every rule and task field that names it, as ONE undo step ([`Project::do_group`]). Returns the number of
/// fields that followed. Refused for an empty name or one another task has.
pub fn rename_task(project: &mut Project, id: &str, index: usize, new: &str) -> Result<usize, String> {
    let new = new.trim();
    let rel = mission_rel(id)?;
    let mission = mission_doc(project, id)?;
    let old = mdoc::task_name(task_at(mission, index)?).unwrap_or("").to_string();
    if new.is_empty() {
        return Err("a task needs a name".into());
    }
    if mdoc::tasks(mission).iter().enumerate().any(|(i, t)| i != index && mdoc::task_name(t) == Some(new)) {
        return Err(format!("there is a task '{new}' already"));
    }
    let mut edits = vec![Edit::set(mdoc::task_pointer(index, Some(mdoc::TASK_NAME)), Value::String(new.to_string()))];
    let follow = if old.is_empty() { vec![] } else { rename_edits(mission, FieldIndex::engine(), "task", &old, new)? };
    let n = follow.len();
    edits.extend(follow);
    project.do_group(&format!("Rename task {old}"), vec![(rel, edits)])?;
    Ok(n)
}

/// Sets the text of task `index` (typing joins one step).
pub fn set_text(project: &mut Project, id: &str, index: usize, text: &str) -> Result<(), String> {
    let rel = mission_rel(id)?;
    task_at(mission_doc(project, id)?, index)?;
    project.apply_form(&rel, "Edit task text", vec![Edit::set(mdoc::task_pointer(index, Some(mdoc::TASK_TEXT)), Value::String(text.to_string()))], true)
}

/// Sets `Active` of task `index`.
pub fn set_active(project: &mut Project, id: &str, index: usize, on: bool) -> Result<(), String> {
    set_flag(project, id, index, mdoc::TASK_ACTIVE, on, "Set task active")
}

/// Sets `required` of task `index`.
pub fn set_required(project: &mut Project, id: &str, index: usize, on: bool) -> Result<(), String> {
    set_flag(project, id, index, mdoc::TASK_REQUIRED, on, "Set task required")
}

fn set_flag(project: &mut Project, id: &str, index: usize, key: &str, on: bool, label: &str) -> Result<(), String> {
    let rel = mission_rel(id)?;
    task_at(mission_doc(project, id)?, index)?;
    project.do_step(&rel, label, vec![Edit::set(mdoc::task_pointer(index, Some(key)), Value::Bool(on))])
}

/// Appends `condition` to `complete_when` or `fail_when` of task `index` as one undo step; returns its position.
pub fn add_condition(project: &mut Project, id: &str, index: usize, key: &str, condition: Value) -> Result<usize, String> {
    check_key(key)?;
    let rel = mission_rel(id)?;
    let list = mdoc::task_conditions(task_at(mission_doc(project, id)?, index)?, key);
    let at = list.len();
    let edit =
        if at == 0 { Edit::set(mdoc::task_pointer(index, Some(key)), Value::Array(vec![condition])) } else { Edit::set(mdoc::task_condition_pointer(index, key, at), condition) };
    project.do_step(&rel, "Add task condition", vec![edit])?;
    Ok(at)
}

/// Removes condition `item` of a task's `complete_when` or `fail_when` as one undo step.
pub fn remove_condition(project: &mut Project, id: &str, index: usize, key: &str, item: usize) -> Result<(), String> {
    check_key(key)?;
    let rel = mission_rel(id)?;
    if mdoc::task_conditions(task_at(mission_doc(project, id)?, index)?, key).len() <= item {
        return Err(format!("no condition {item}"));
    }
    project.do_step(&rel, "Remove task condition", vec![Edit::remove(mdoc::task_condition_pointer(index, key, item))])
}

/// Applies the edits a condition form made (pointers below `/tasks/<index>/<key>/<item>`). Typing in one field joins one step; a new
/// Type (the whole condition replaced) is a step of its own.
pub fn apply_condition_edits(project: &mut Project, id: &str, index: usize, key: &str, item: usize, edits: Vec<Edit>) -> Result<(), String> {
    check_key(key)?;
    if edits.is_empty() {
        return Ok(());
    }
    let rel = mission_rel(id)?;
    let base = mdoc::task_condition_pointer(index, key, item);
    let typing = edits.iter().all(|e| e.pointer != base);
    project.apply_form(&rel, "Edit task condition", edits, typing)
}

/// Whether the required tasks end the mission (`victory` is `"objectives"`).
pub fn victory_on(mission: &Value) -> bool {
    mdoc::victory(mission) == Some(mdoc::VICTORY_OBJECTIVES)
}

/// Turns `"victory": "objectives"` on or off as one undo step.
pub fn set_victory(project: &mut Project, id: &str, on: bool) -> Result<(), String> {
    set_top(project, id, mdoc::KEY_VICTORY, on.then(|| mdoc::VICTORY_OBJECTIVES.to_string()), "Set victory")
}

/// Sets `victory_message` or `defeat_message` (an id into `media.json` `messages`); `None` or empty clears it. Typing joins one step.
pub fn set_end_message(project: &mut Project, id: &str, key: &str, message: Option<&str>) -> Result<(), String> {
    if key != mdoc::KEY_VICTORY_MESSAGE && key != mdoc::KEY_DEFEAT_MESSAGE {
        return Err(format!("'{key}' is not an end message"));
    }
    set_top(project, id, key, message.filter(|m| !m.is_empty()).map(str::to_string), "Edit end message")
}

fn set_top(project: &mut Project, id: &str, key: &str, value: Option<String>, label: &str) -> Result<(), String> {
    let rel = mission_rel(id)?;
    let mission = mission_doc(project, id)?;
    if mission.get(key).is_none() && value.is_none() {
        return Ok(());
    }
    let at = mdoc::top_pointer(key);
    let edit = match value {
        Some(v) => Edit::set(at, Value::String(v)),
        // A file that extends another cannot remove a key: write it empty.
        None if mproject::check_remove_key(project, &rel).is_ok() => Edit::remove(at),
        None => Edit::set(at, Value::String(String::new())),
    };
    let typing = key != mdoc::KEY_VICTORY;
    project.apply_form(&rel, label, vec![edit], typing)
}

// ---- the section ----

fn schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(Schema::engine)
}

/// What one frame of the section asks for, applied after drawing (the drawing reads a copy of the mission).
enum Action {
    Add(String),
    Delete(usize),
    Rename(usize, String),
    Text(usize, String),
    Active(usize, bool),
    Required(usize, bool),
    AddCondition(usize, &'static str, Value),
    RemoveCondition(usize, &'static str, usize),
    Condition(usize, &'static str, usize, Vec<Edit>),
    Victory(bool),
    Message(&'static str, Option<String>),
}

/// The mission the "Missions" tab sections work on, picked in a drop-down shared by the sections that call this (the selection is in
/// egui's memory under one id). Opens the mission's documents when picked. `None` when the mod and the import have no mission.
pub fn pick_mission(ui: &mut egui::Ui, project: &mut Project) -> Option<String> {
    let key = egui::Id::new("mission-section-pick");
    let missions = mproject::list_missions(project);
    let mut ids: Vec<String> = missions.iter().filter(|m| m.in_mod || m.in_import).map(|m| m.id.clone()).collect();
    ids.sort();
    if ids.is_empty() {
        ui.label("No mission in the mod or the import yet.");
        return None;
    }
    let mut picked: String = ui.data(|d| d.get_temp(key)).filter(|p| ids.contains(p)).unwrap_or_else(|| ids[0].clone());
    ui.horizontal(|ui| {
        ui.label("Mission");
        egui::ComboBox::from_id_salt(key.with("combo")).selected_text(picked.clone()).show_ui(ui, |ui| {
            for id in &ids {
                ui.selectable_value(&mut picked, id.clone(), id);
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(key, picked.clone()));
    if let Err(e) = mproject::open_mission(project, &picked) {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
        return None;
    }
    Some(picked)
}

/// Draws the section.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    let Some(id) = pick_mission(ui, project) else { return };
    let Some(mission) = mproject::merged_mission(project, &id).cloned() else { return };
    let names = Names::collect(&mission, mproject::merged_map(project, &id).unwrap_or(&Value::Null), mproject::merged_media(project, &id).unwrap_or(&Value::Null));
    let root = egui::Id::new(("mission-tasks", id.as_str()));
    let mut actions = vec![];

    ui.strong("Tasks");
    for (i, task) in mdoc::tasks(&mission).iter().enumerate() {
        task_row(ui, root.with(i), i, task, &mission, &names, &mut actions);
    }
    ui.horizontal(|ui| {
        let new_id = root.with("new");
        let mut name: String = ui.data(|d| d.get_temp(new_id)).unwrap_or_default();
        ui.add(egui::TextEdit::singleline(&mut name).hint_text("new task name").desired_width(160.0));
        if ui.button("Add task").clicked() {
            actions.push(Action::Add(name.clone()));
            name.clear();
        }
        ui.data_mut(|d| d.insert_temp(new_id, name));
    });

    ui.separator();
    ui.strong("The end");
    let mut on = victory_on(&mission);
    if ui
        .checkbox(&mut on, "The required tasks end the mission (victory: objectives)")
        .on_hover_text("Complete every required task to win; a required task that fails loses.")
        .changed()
    {
        actions.push(Action::Victory(on));
    }
    if on && !mdoc::tasks(&mission).iter().any(mdoc::task_required) {
        ui.colored_label(egui::Color32::YELLOW, "No task is required, so the mission never ends this way.");
    }
    for (key, label) in [(mdoc::KEY_VICTORY_MESSAGE, "Victory message"), (mdoc::KEY_DEFEAT_MESSAGE, "Defeat message")] {
        let current = mission.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let mut text = current.clone();
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(&mut text).hint_text("id in media.json messages").desired_width(200.0));
        });
        if text != current {
            actions.push(Action::Message(key, Some(text)));
        }
    }

    for a in actions {
        if let Err(e) = apply(project, &id, a) {
            ui.data_mut(|d| d.insert_temp(root.with("error"), e));
        } else {
            ui.data_mut(|d| d.remove_temp::<String>(root.with("error")));
        }
    }
    if let Some(e) = ui.data(|d| d.get_temp::<String>(root.with("error"))) {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
}

fn apply(project: &mut Project, id: &str, a: Action) -> Result<(), String> {
    match a {
        Action::Add(name) => add_task(project, id, &name).map(|_| ()),
        Action::Delete(i) => delete_task(project, id, i),
        Action::Rename(i, name) => rename_task(project, id, i, &name).map(|_| ()),
        Action::Text(i, text) => set_text(project, id, i, &text),
        Action::Active(i, on) => set_active(project, id, i, on),
        Action::Required(i, on) => set_required(project, id, i, on),
        Action::AddCondition(i, key, c) => add_condition(project, id, i, key, c).map(|_| ()),
        Action::RemoveCondition(i, key, item) => remove_condition(project, id, i, key, item),
        Action::Condition(i, key, item, edits) => apply_condition_edits(project, id, i, key, item, edits),
        Action::Victory(on) => set_victory(project, id, on),
        Action::Message(key, m) => set_end_message(project, id, key, m.as_deref()),
    }
}

fn task_row(ui: &mut egui::Ui, id: egui::Id, index: usize, task: &Value, mission: &Value, names: &Names, actions: &mut Vec<Action>) {
    let name = mdoc::task_name(task).unwrap_or("").to_string();
    let title = if name.is_empty() { format!("task {index}") } else { name.clone() };
    egui::CollapsingHeader::new(title).id_salt(id).show(ui, |ui| {
        // The name: the rename (and the references that follow) is made when the box loses focus.
        let (edit_id, buf_id) = (id.with("name"), id.with("name-buf"));
        let focused = ui.memory(|m| m.has_focus(edit_id));
        let mut buf: String = if focused { ui.data(|d| d.get_temp(buf_id)).unwrap_or_else(|| name.clone()) } else { name.clone() };
        let r = ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut buf).id(edit_id).desired_width(220.0))
        });
        ui.data_mut(|d| d.insert_temp(buf_id, buf.clone()));
        if r.inner.lost_focus() && buf != name {
            actions.push(Action::Rename(index, buf));
        }
        let text = mdoc::task_text(task).unwrap_or("");
        let mut edited = text.to_string();
        ui.horizontal(|ui| {
            ui.label("Text");
            ui.add(egui::TextEdit::multiline(&mut edited).desired_rows(2).desired_width(360.0));
        });
        if edited != text {
            actions.push(Action::Text(index, edited));
        }
        let (mut active, mut required) = (mdoc::task_active(task), mdoc::task_required(task));
        ui.horizontal(|ui| {
            if ui.checkbox(&mut active, "Active").on_hover_text("Shown to the player from the start").changed() {
                actions.push(Action::Active(index, active));
            }
            if ui.checkbox(&mut required, "Required").on_hover_text("Counts for victory: objectives").changed() {
                actions.push(Action::Required(index, required));
            }
        });
        for (key, label) in [(mdoc::TASK_COMPLETE_WHEN, "Complete when"), (mdoc::TASK_FAIL_WHEN, "Fail when")] {
            conditions(ui, id.with(key), index, key, label, task, mission, names, actions);
        }
        if ui.button("Delete task").clicked() {
            actions.push(Action::Delete(index));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn conditions(ui: &mut egui::Ui, id: egui::Id, index: usize, key: &'static str, label: &str, task: &Value, _mission: &Value, names: &Names, actions: &mut Vec<Action>) {
    let list = mdoc::task_conditions(task, key);
    ui.label(egui::RichText::new(format!("{label} (all of these hold)")).strong());
    for (i, c) in list.iter().enumerate() {
        ui.group(|ui| {
            let base = mdoc::task_condition_pointer(index, key, i);
            let edits = schema_form::item_form_edits(ui, schema(), names, c, true, &[], &base, id.with(i));
            if !edits.is_empty() {
                actions.push(Action::Condition(index, key, i, edits));
            }
            if ui.small_button("Remove").clicked() {
                actions.push(Action::RemoveCondition(index, key, i));
            }
        });
    }
    egui::ComboBox::from_id_salt(id.with("add")).selected_text("Add condition").show_ui(ui, |ui| {
        for t in schema().types.iter().filter(|t| t.is_condition) {
            if ui.selectable_label(false, &t.name).clicked() {
                actions.push(Action::AddCondition(index, key, schema_form::new_item(t)));
            }
        }
    });
}
