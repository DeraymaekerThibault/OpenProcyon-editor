//! The "Missions" tab section: the mission's place in the mod's campaign order (docs/phase5.md T12), `mod.json` `missions`. A mod
//! that lists missions there plays them in that order (the engine reads the list; a mission found in `maps/` that is not listed
//! follows by id, PROVISIONAL: docs/phase5.md P9 reads this from `campaign.rs:349-375` of the engine). The list is what
//! [`super::project`] already writes (a new mission is appended); this module reorders it, takes an entry out and adds a mission the
//! mod or the import has. Every edit writes the whole list as one edit of `mod.json`, so it is one undo step.
//!
//! A retail id (`mission01`..`mission13`, the tutorials) is never listed by this editor: a mod's own copy of one changes that retail
//! mission in place and the retail campaign plays it, and a custom list leaves retail ids out (docs/phase5.md P9, PROVISIONAL).
//! An id in the list that has no `mission.json` in the mod or the import is flagged (`check_mod` warns about it).

use bevy_egui::egui;
use serde_json::Value;

use super::doc as mdoc;
use super::project::{Origin, is_retail_id, list_missions};
use crate::edit::Edit;
use crate::project::Project;

const MOD_FILE: &str = "mod.json";

/// What a listed id is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// A mission of the mod's own or one the import has.
    Ok,
    /// No `mission.json` for it in the mod or the import.
    Missing,
    /// A retail id: the list leaves it out (PROVISIONAL).
    Retail,
}

/// One entry of the campaign order, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub status: Status,
    /// The map's name as the game lists it, when known.
    pub name: Option<String>,
}

fn open_list(project: &mut Project) -> Result<Vec<String>, String> {
    if project.doc(MOD_FILE).is_none() && !project.dir.join(MOD_FILE).is_file() {
        return Err("the mod has no mod.json: make the mod first".into());
    }
    project.open_doc(MOD_FILE)?;
    let meta = &project.doc(MOD_FILE).expect("opened").edited;
    if !meta.is_object() {
        return Err("mod.json is not an object: fix it first".into());
    }
    if mdoc::mod_missions_malformed(meta) {
        return Err("mod.json 'missions' is not a list: fix it first".into());
    }
    Ok(mdoc::mod_missions(meta).into_iter().map(str::to_string).collect())
}

/// The campaign order of `mod.json` (empty when there is none or it cannot be read).
pub fn order(project: &Project) -> Vec<String> {
    project.doc(MOD_FILE).map(|d| mdoc::mod_missions(&d.edited).into_iter().map(str::to_string).collect()).unwrap_or_default()
}

/// The campaign order with what each entry is.
pub fn rows(project: &Project) -> Vec<Row> {
    let known = list_missions(project);
    order(project)
        .into_iter()
        .map(|id| {
            let entry = known.iter().find(|m| m.id == id);
            let status = if is_retail_id(&id) {
                Status::Retail
            } else if entry.is_none() {
                Status::Missing
            } else {
                Status::Ok
            };
            Row { name: entry.and_then(|m| m.name.clone()), id, status }
        })
        .collect()
}

/// The missions the mod or the import has that the list does not name and that can be added: not a retail id (listing one has no
/// effect, [`add`] refuses it). They follow the listed ones by id.
pub fn unlisted(project: &Project) -> Vec<String> {
    let listed = order(project);
    list_missions(project).into_iter().filter(|m| m.origin == Origin::Mod && !listed.contains(&m.id)).map(|m| m.id).collect()
}

fn write(project: &mut Project, label: &str, list: Vec<String>) -> Result<(), String> {
    let value = Value::Array(list.into_iter().map(Value::String).collect());
    project.do_step(MOD_FILE, label, vec![Edit::set(mdoc::mod_missions_pointer(), value)])
}

/// Moves the entry at `from` to position `to` (both inside the list) as one undo step.
pub fn move_to(project: &mut Project, from: usize, to: usize) -> Result<(), String> {
    let mut list = open_list(project)?;
    if from >= list.len() || to >= list.len() {
        return Err(format!("no place {} in a list of {}", from.max(to), list.len()));
    }
    if from == to {
        return Ok(());
    }
    let id = list.remove(from);
    list.insert(to, id);
    write(project, "Move mission in the campaign", list)
}

/// Takes the entry at `index` out of the list as one undo step and returns its id. The mission itself stays in the mod.
pub fn remove_at(project: &mut Project, index: usize) -> Result<String, String> {
    let mut list = open_list(project)?;
    if index >= list.len() {
        return Err(format!("no place {index} in a list of {}", list.len()));
    }
    let id = list.remove(index);
    write(project, &format!("Remove mission {id} from the campaign"), list)?;
    Ok(id)
}

/// Appends the existing mission `id` to the list as one undo step. Refused for an id the list has, a retail id and an id that is no
/// mission of the mod or the import.
pub fn add(project: &mut Project, id: &str) -> Result<(), String> {
    let mut list = open_list(project)?;
    if list.iter().any(|l| l == id) {
        return Err(format!("'{id}' is in the campaign already"));
    }
    if is_retail_id(id) {
        return Err(format!("'{id}' is a retail mission id: a custom campaign leaves retail ids out"));
    }
    if !list_missions(project).iter().any(|m| m.id == id) {
        return Err(format!("no mission '{id}' in the mod or the import"));
    }
    list.push(id.to_string());
    write(project, &format!("Add mission {id} to the campaign"), list)
}

/// Draws the section.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    if let Err(e) = open_list(project) {
        ui.label(e);
        return;
    }
    let rows = rows(project);
    let root = egui::Id::new("mission-campaign");
    let mut result = Ok(());
    if rows.is_empty() {
        ui.label("The mod lists no missions: they follow by id.");
    }
    let last = rows.len().saturating_sub(1);
    let mut moves = vec![];
    for (i, row) in rows.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}.", i + 1));
            ui.label(&row.id);
            if let Some(name) = &row.name {
                ui.weak(name);
            }
            match row.status {
                Status::Ok => {}
                Status::Missing => {
                    ui.colored_label(egui::Color32::YELLOW, "no mission.json for this id");
                }
                Status::Retail => {
                    ui.colored_label(egui::Color32::YELLOW, "replaces the retail mission, not listed");
                }
            }
            if ui.add_enabled(i > 0, egui::Button::new("Up").small()).clicked() {
                moves.push((i, Some(i - 1)));
            }
            if ui.add_enabled(i < last, egui::Button::new("Down").small()).clicked() {
                moves.push((i, Some(i + 1)));
            }
            if ui.small_button("Remove").clicked() {
                moves.push((i, None));
            }
        });
    }
    for (from, to) in moves {
        result = result.and(match to {
            Some(to) => move_to(project, from, to),
            None => remove_at(project, from).map(|_| ()),
        });
    }
    let unlisted = unlisted(project);
    if !unlisted.is_empty() {
        ui.separator();
        ui.weak("Not listed (these follow the listed ones by id):");
        for id in unlisted {
            ui.horizontal(|ui| {
                ui.weak(&id);
                if ui.small_button("Add to the campaign").clicked() {
                    result = result.clone().and(add(project, &id));
                }
            });
        }
    }
    ui.data_mut(|d| match &result {
        Ok(()) => {
            d.remove_temp::<String>(root.with("error"));
        }
        Err(e) => {
            d.insert_temp(root.with("error"), e.clone());
        }
    });
    if let Some(e) = ui.data(|d| d.get_temp::<String>(root.with("error"))) {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
}
