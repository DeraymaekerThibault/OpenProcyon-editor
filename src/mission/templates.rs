//! Mission templates in the "Missions" tab (docs/phase5.md P8, T11). The templates are the engine's (`openprocyon::authoring::templates`,
//! files in its `content/templates/`): nothing about their contents is written here. This module lists them, draws the parameter form
//! with the editor's own pickers, and writes an expansion into a mission as ONE undo step ([`expand`]); what it writes is ordinary
//! rules, tasks and (where missing) `victory`, which stay editable and remember nothing of the template.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use bevy_egui::egui;
use openprocyon::authoring::{self, Template};
use serde_json::{Map, Value};

use super::doc::{self as mdoc, MissionList};
use super::project::{self as mproject, mission_rel};
use crate::edit::Edit;
use crate::project::Project;
use crate::schema_form::{self as sf, Field, Names};

/// The engine's templates, read once.
pub fn all() -> &'static [Template] {
    static T: OnceLock<Vec<Template>> = OnceLock::new();
    T.get_or_init(authoring::templates)
}

/// What [`expand`] added.
#[derive(Debug, PartialEq, Eq)]
pub struct Added {
    pub rules: usize,
    pub tasks: usize,
}

/// Fills in `template` with `params` (the engine's [`authoring::expand_template`]: a parameter left out takes its default) and
/// writes the rules, the tasks, and the `victory` key when the mission has none, into mission `id` as ONE undo step
/// ([`Project::do_group`]). Nothing changes when anything is refused: the engine's errors (a missing parameter), a rule or task name
/// the mission has already (or the template repeats), and a mission that extends another where the list cannot take new entries
/// ([`mproject::check_add_delete`]).
pub fn expand(project: &mut Project, id: &str, template: &Template, params: &BTreeMap<String, String>) -> Result<Added, String> {
    let rel = mission_rel(id)?;
    project.open_doc(&rel)?;
    let x = authoring::expand_template(template, params)?;
    mproject::check_add_delete(project, id, MissionList::Rules)?;
    if !x.tasks.is_empty() {
        mproject::check_add_delete(project, id, MissionList::Tasks)?;
    }
    let mission = &project.doc(&rel).ok_or_else(|| format!("{rel} is not open"))?.edited;
    let mut edits = vec![];
    for (list, key, entries) in [(MissionList::Rules, mdoc::rule_name as fn(&Value) -> Option<&str>, &x.rules), (MissionList::Tasks, mdoc::task_name, &x.tasks)] {
        let have = mdoc::list_entries(mission, list);
        let mut taken: Vec<&str> = have.iter().filter_map(key).collect();
        for e in entries {
            let name = key(e).ok_or_else(|| format!("template '{}' writes an entry with no name", template.id))?;
            if taken.contains(&name) {
                return Err(format!("the mission has a {} '{name}' already: give the template another name", mdoc::list_noun(list)));
            }
            taken.push(name);
        }
        let ptr = mdoc::list_pointer(list);
        if entries.is_empty() {
        } else if mission.pointer(&ptr).is_some_and(Value::is_array) {
            edits.extend(entries.iter().enumerate().map(|(i, e)| Edit::set(format!("{ptr}/{}", have.len() + i), e.clone())));
        } else {
            edits.push(Edit::set(ptr, Value::Array(entries.clone())));
        }
    }
    for (k, v) in &x.mission {
        if mission.get(k).is_none() {
            edits.push(Edit::set(mdoc::top_pointer(k), v.clone()));
        }
    }
    project.do_group(&format!("Add template {}", template.title), vec![(rel, edits)])?;
    Ok(Added { rules: x.rules.len(), tasks: x.tasks.len() })
}

/// The open parameter form: which template and what has been typed or picked (a parameter left out takes its default).
#[derive(Clone, Debug, Default)]
pub struct Form {
    pub template: usize,
    pub values: BTreeMap<String, String>,
}

impl Form {
    pub fn new(template: usize) -> Form {
        Form { template, values: BTreeMap::new() }
    }

    /// The parameters to hand to [`expand`]: what was entered, nothing empty.
    pub fn params(&self) -> BTreeMap<String, String> {
        self.values.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Names of the parameters that have neither a value nor a default, in the template's order.
    pub fn missing(&self, t: &Template) -> Vec<String> {
        let given = self.params();
        t.parameters.iter().filter(|p| p.default.is_none() && !given.contains_key(&p.name)).map(|p| p.label.clone()).collect()
    }
}

/// Id of a widget of the picker (`("template", id)`, `"expand"`, `"cancel"`, a parameter is `("param", name)`).
pub fn wid(key: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::new("templates").with(key)
}

/// What the author did this frame.
pub enum Outcome {
    None,
    Close,
    /// Expand now with these parameters.
    Expand(usize, BTreeMap<String, String>),
}

/// The picker and the form, drawn under the "+ Template" chip: the list of templates, and for the chosen one a field per parameter
/// (a picker of the mission's names for `group`, `volume`, `point`, `point_set`, `speech_event` and `rule`; a number box; a text
/// box) and Expand / Cancel.
pub fn show(ui: &mut egui::Ui, form: &mut Form, names: &Names) -> Outcome {
    let all = all();
    let mut outcome = Outcome::None;
    ui.horizontal_wrapped(|ui| {
        for (i, t) in all.iter().enumerate() {
            let on = form.template == i;
            if ui.selectable_label(on, &t.title).on_hover_text(&t.description).clicked() && !on {
                *form = Form::new(i);
            }
        }
    });
    let Some(t) = all.get(form.template) else { return Outcome::Close };
    ui.weak(&t.description);
    for p in &t.parameters {
        ui.horizontal(|ui| {
            ui.label(if p.default.is_some() { p.label.clone() } else { format!("{} *", p.label) });
            let current = form.values.get(&p.name).cloned().or_else(|| p.default.clone()).unwrap_or_default();
            if let Some(new) = param_widget(ui, &p.kind, &p.name, &current, names) {
                form.values.insert(p.name.clone(), new);
            }
        });
    }
    let missing = form.missing(t);
    ui.horizontal(|ui| {
        let ready = missing.is_empty();
        if ui.add_enabled(ready, egui::Button::new("Expand")).on_hover_text("Adds the rules and tasks to the mission as one undo step").clicked() {
            outcome = Outcome::Expand(form.template, form.params());
        }
        if ui.button("Cancel").clicked() {
            outcome = Outcome::Close;
        }
    });
    // Its own line, always there, cut when long: beside the buttons it ran under the plan area.
    let text = if missing.is_empty() { String::new() } else { format!("still to fill in: {}", missing.join(", ")) };
    crate::layout::status_line(ui, &text, Some(ui.visuals().weak_text_color()));
    outcome
}

/// One parameter's widget, the schema form's own for its kind; returns the new value (as text) when it changed.
fn param_widget(ui: &mut egui::Ui, kind: &str, name: &str, current: &str, names: &Names) -> Option<String> {
    let field = Field { name: name.to_string(), kind: kind.to_string(), default: String::new(), required: true, values: vec![], range: None };
    let mut obj = Map::new();
    if !current.is_empty() {
        let value = if kind == "number" { current.parse::<f64>().map_or_else(|_| Value::from(current), Value::from) } else { Value::from(current) };
        obj.insert(name.to_string(), value);
    }
    if !sf::field_widget(ui, &field, &mut obj, names, wid(("param", name))) {
        return None;
    }
    obj.get(name).map(|v| match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), |f| if f.fract() == 0.0 { format!("{}", f as i64) } else { f.to_string() }),
        other => other.to_string(),
    })
}
