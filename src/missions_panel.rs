//! The "Missions" tab (docs/phase5.md): the sections of the mission editor, each drawn by its own module under
//! [`crate::mission`]. This file only lays the sections out and passes the open project; from T5 on it is owned by the rule-list
//! task, and the other tasks fill their own module's `show` and do not edit it. The mission picker comes first; the open mission's
//! id is [`crate::mission::rules_view::selected_mission`] for the sections below.

use bevy_egui::egui;

use crate::mission::project::retail_limits;
use crate::mission::{campaign, plan, report, rules_view, ships, speech, tasks};
use crate::project::Project;
use crate::project_panel;

/// The tab: needs an open mod, then one collapsing section per module.
pub fn tab(ui: &mut egui::Ui, ps: &mut project_panel::State) {
    let Some(project) = ps.project.as_mut() else {
        ui.label("Open or make a mod folder in the left panel to edit its missions.");
        return;
    };
    sections(ui, project);
}

fn sections(ui: &mut egui::Ui, project: &mut Project) {
    rules_view::picker(ui, project);
    retail_notice(ui, project);
    ui.separator();
    section(ui, "Players, ships and groups", |ui| ships::show(ui, project));
    section(ui, "Plan", |ui| plan::show(ui, project));
    section(ui, "Tasks and the end", |ui| tasks::show(ui, project));
    section(ui, "Rules", |ui| rules_view::show(ui, project));
    section(ui, "Speech and messages", |ui| speech::show(ui, project));
    section(ui, "Campaign", |ui| campaign::show(ui, project));
    section(ui, "Play-test report", |ui| report::show(ui, project));
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, body);
}

/// The limits of a retail mission saved as a diff (docs/phase5.md P11), shown while one is open. Only says what the engine's merge does.
fn retail_notice(ui: &mut egui::Ui, project: &Project) {
    let Some(id) = rules_view::selected_mission(ui.ctx()) else { return };
    let notes = retail_limits(project, &id);
    if notes.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("{id} extends a retail mission: what saves as a diff")).default_open(true).show(ui, |ui| {
        for n in notes {
            ui.colored_label(egui::Color32::YELLOW, format!("- {n}"));
        }
    });
}
