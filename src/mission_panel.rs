//! The spike's mission panel: open a mod folder, pick a rule, edit it through the schema form, see `--check-mission` findings,
//! save in the engine's layout, play-test.

use bevy::prelude::Resource;
use bevy_egui::egui;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Child;

use crate::checks::{self, Level, Problem};
use crate::launch;
use crate::mission::{Mission, fixture_dir};
use crate::schema_form::{self, Names, Schema};

#[derive(Resource)]
pub struct State {
    schema: Schema,
    mission: Option<Mission>,
    names: Names,
    problems: Vec<Problem>,
    rule: usize,
    status: String,
    child: Option<Child>,
    dirty: bool,
}

impl Default for State {
    fn default() -> Self {
        State { schema: Schema::engine(), mission: None, names: Names::default(), problems: vec![], rule: 0, status: String::new(), child: None, dirty: false }
    }
}

impl State {
    fn open(&mut self, dir: PathBuf) {
        match Mission::load(&dir) {
            Ok(m) => {
                self.status = format!("Opened {} ({})", m.id, dir.display());
                self.mission = Some(m);
                self.rule = 0;
                self.dirty = false;
                self.refresh();
            }
            Err(e) => self.status = e,
        }
    }

    /// After every edit: names for the dropdowns and the engine's checks, on the data as edited.
    fn refresh(&mut self) {
        if let Some(m) = &self.mission {
            self.names = Names::collect(&self.schema, &m.mission, &m.map, &m.media);
            self.problems = checks::run(&m.mission, &m.map, &m.media);
        }
    }
}

pub fn show(ui: &mut egui::Ui, st: &mut State) {
    ui.heading("Mission");
    ui.horizontal(|ui| {
        if ui.button("Open mod folder...").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            st.open(dir);
        }
        if ui.button("Open example (tests/fixtures/gates)").clicked() {
            st.open(fixture_dir());
        }
    });
    if !st.status.is_empty() {
        ui.label(&st.status);
    }
    if st.mission.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(st.dirty, egui::Button::new("Save")).clicked() {
            let r = st.mission.as_ref().map(Mission::save);
            match r {
                Some(Ok(())) => {
                    st.dirty = false;
                    st.status = "Saved in the engine's JSON layout.".into();
                }
                Some(Err(e)) => st.status = e,
                None => {}
            }
        }
        let running = st.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if !running {
            st.child = None;
        }
        if ui.add_enabled(!running, egui::Button::new("Play-test")).on_hover_text(format!("runs {}", launch::game_bin().display())).clicked()
            && let Some(m) = &st.mission
        {
            match launch::spawn(&m.dir, &m.id) {
                Ok(c) => {
                    st.child = Some(c);
                    st.status = "Game started.".into();
                }
                Err(e) => st.status = format!("cannot start {}: {e} (set PROCYON_GAME_BIN)", launch::game_bin().display()),
            }
        }
        if running {
            ui.label("game running");
        }
        if st.dirty {
            ui.label("unsaved changes");
        }
    });
    ui.separator();

    let names: Vec<String> = st
        .mission
        .as_ref()
        .map(|m| m.mission["rules"].as_array().into_iter().flatten().map(|r| r["name"].as_str().unwrap_or("(unnamed)").to_string()).collect())
        .unwrap_or_default();
    if names.is_empty() {
        ui.label("This mission has no rules.");
        return;
    }
    st.rule = st.rule.min(names.len() - 1);
    ui.horizontal(|ui| {
        ui.label("Rule");
        egui::ComboBox::from_id_salt("rule").selected_text(&names[st.rule]).show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(&mut st.rule, i, n);
            }
        });
    });
    let rule_name = names[st.rule].clone();
    let mut changed = false;
    {
        let State { schema, mission, names: defs, problems, rule, .. } = &mut *st;
        let Some(m) = mission.as_mut() else { return };
        let r = &mut m.mission["rules"][*rule];
        for p in checks::for_rule(problems, &rule_name) {
            ui.colored_label(egui::Color32::LIGHT_RED, &p.message);
        }
        ui.horizontal(|ui| {
            ui.label("Name");
            let mut n = r["name"].as_str().unwrap_or_default().to_string();
            if ui.text_edit_singleline(&mut n).changed() {
                r["name"] = n.into();
                changed = true;
            }
        });
        // Rule flags: the files hold JSON booleans here, which the rule loader reads as booleans (unlike Type fields).
        for key in ["Run Once", "Is Active"] {
            let mut on = r[key].as_bool().unwrap_or(false);
            if ui.checkbox(&mut on, key).changed() {
                r[key] = json!(on);
                changed = true;
            }
        }
        for (list, is_cond) in [("conditions", true), ("actions", false)] {
            ui.separator();
            ui.strong(if is_cond { "Conditions" } else { "Actions" });
            let count = r[list].as_array().map_or(0, Vec::len);
            let mut remove = None;
            for i in 0..count {
                let item = &mut r[list][i];
                let title = item["Type"].as_str().unwrap_or("?").to_string();
                let mine = checks::for_item(problems, &rule_name, is_cond, i);
                let id = egui::Id::new((list, *rule, i));
                let head = if mine.is_empty() { title } else { format!("{title}  ({} problem{})", mine.len(), if mine.len() == 1 { "" } else { "s" }) };
                egui::CollapsingHeader::new(format!("{}. {head}", i + 1)).id_salt(id).default_open(true).show(ui, |ui| {
                    changed |= schema_form::item_form(ui, schema, defs, item, is_cond, &mine, id);
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                r[list].as_array_mut().map(|a| a.remove(i));
                changed = true;
            }
            if ui.small_button(format!("Add {}", if is_cond { "condition" } else { "action" })).clicked()
                && let Some(def) = schema.types.iter().find(|t| t.is_condition == is_cond)
            {
                let new = schema_form::new_item(def);
                match r[list].as_array_mut() {
                    Some(a) => a.push(new),
                    None => r[list] = Value::Array(vec![new]),
                }
                changed = true;
            }
        }
    }
    if changed {
        st.dirty = true;
        st.refresh();
    }
}

/// The bottom panel: every finding of the engine's check, errors first.
pub fn problems(ui: &mut egui::Ui, st: &State) {
    let errors = st.problems.iter().filter(|p| p.level == Level::Error).count();
    ui.strong(format!("Problems: {errors} errors, {} warnings", st.problems.len() - errors));
    egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
        let mut sorted: Vec<&Problem> = st.problems.iter().collect();
        sorted.sort_by_key(|p| p.level != Level::Error);
        for p in sorted {
            let color = if p.level == Level::Error { egui::Color32::LIGHT_RED } else { egui::Color32::YELLOW };
            ui.colored_label(color, p.to_string());
        }
    });
}
