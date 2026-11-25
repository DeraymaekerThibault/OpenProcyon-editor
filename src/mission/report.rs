//! The "Missions" tab section: the play-test report (docs/phase5.md P10, T13). The game is started from here with
//! `--rule-report <file>`; when it has ended the file is read with `openprocyon::authoring::RuleReport` and shown: how the play
//! ended, for how long, and per rule whether it fired and how often. Rules that never fired are highlighted.
//! A game that wrote no file (it was closed before a battle with mission rules ended, or it crashed) has no report: the section
//! says so, with the game's own exit message when it has one. The autoplay log line is no longer read.

use std::sync::{Arc, Mutex};

use bevy_egui::egui;
use openprocyon::authoring::{RuleReport, RuleReportOutcome};

use super::rules_view;
use crate::launch::{self, Game, GameState, Launch};
use crate::project::Project;

/// What the section keeps between frames (in egui memory, like the rule list's state).
#[derive(Default)]
struct State {
    game: Option<Game>,
    /// The mission the running or last game played.
    mission: String,
    /// The game's own exit message when it failed, else a status line.
    status: String,
    failed: bool,
    /// The last run's report: `Ok(None)` when the game wrote none.
    report: Option<Result<Option<RuleReport>, String>>,
}

fn state(ctx: &egui::Context) -> Arc<Mutex<State>> {
    let id = egui::Id::new("mission_playtest_report");
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with::<Arc<Mutex<State>>>(id, Default::default).clone())
}

/// Draws the section: the play-test button of the open mission and the last report.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    let st = state(ui.ctx());
    let Ok(mut st) = st.lock() else { return };
    if let Some(g) = st.game.as_mut()
        && let GameState::Ended(exit) = g.poll()
    {
        let report = g.report();
        st.failed = exit.is_some();
        st.status = exit.unwrap_or_else(|| "The game ended.".into());
        st.report = Some(report);
        st.game = None;
    }
    if st.game.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    let id = rules_view::selected_mission(ui.ctx());
    let dirty = !project.dirty_rels().is_empty();
    ui.horizontal(|ui| {
        let ready = id.is_some() && !dirty && st.game.is_none();
        let tip = if st.game.is_some() {
            "A game started from here is still running.".to_string()
        } else if id.is_none() {
            "Choose a mission above.".to_string()
        } else if dirty {
            "The game reads the files on disk: save the mod first.".to_string()
        } else {
            format!("runs {} --mod <the mod> --map <mission> --rule-report <file>", launch::game_bin().display())
        };
        if ui.add_enabled(ready, egui::Button::new("Play-test this mission")).on_hover_text(tip).clicked()
            && let Some(id) = &id
        {
            st.failed = false;
            st.report = None;
            st.mission = id.clone();
            match Game::spawn(&Launch::new(&project.dir, id)) {
                Ok(g) => {
                    st.game = Some(g);
                    st.status = format!("Game started on {id}. Play until the mission ends, then close the game to see the report.");
                }
                Err(e) => {
                    st.failed = true;
                    st.status = format!("cannot start {}: {e} (set PROCYON_GAME_BIN)", launch::game_bin().display());
                }
            }
        }
        if st.game.is_some() && ui.button("Stop").clicked() {
            if let Some(mut g) = st.game.take() {
                g.stop();
            }
            st.status = "The game was stopped.".into();
        }
    });
    if dirty {
        ui.weak("Unsaved edits: save the mod to play-test them.");
    }
    if !st.status.is_empty() {
        if st.failed {
            ui.colored_label(ui.visuals().error_fg_color, &st.status);
        } else {
            ui.label(&st.status);
        }
    }
    match &st.report {
        None => {}
        Some(Ok(Some(r))) => draw(ui, r),
        Some(Ok(None)) if st.game.is_none() => {
            ui.weak("The game wrote no rule report: it ended before a battle with mission rules did, or it did not finish.");
        }
        Some(Ok(None)) => {}
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().warn_fg_color, e);
        }
    }
}

/// `m:ss` of a number of game seconds.
pub fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// How the play ended, in words.
pub fn outcome_text(o: RuleReportOutcome) -> &'static str {
    match o {
        RuleReportOutcome::Victory => "Victory",
        RuleReportOutcome::Defeat => "Defeat",
        RuleReportOutcome::Quit => "Left before the rules ended it",
    }
}

/// The names of the rules that never ran, in script order.
pub fn never_fired(r: &RuleReport) -> Vec<&str> {
    r.rules.iter().filter(|e| !e.fired).map(|e| e.name.as_str()).collect()
}

/// The one-line summary: outcome, play time, how many rules fired.
pub fn summary(r: &RuleReport) -> String {
    let fired = r.rules.iter().filter(|e| e.fired).count();
    format!("{}: {} after {}; {fired} of {} rules fired, {} never", r.mission, outcome_text(r.outcome), clock(r.play_time), r.rules.len(), r.rules.len() - fired)
}

/// Draws a report: the summary line and one row per rule (name, fired or never fired, runs, first and last run, still active).
pub fn draw(ui: &mut egui::Ui, r: &RuleReport) {
    ui.strong(summary(r));
    egui::Grid::new(("rule_report", &r.mission)).striped(true).show(ui, |ui| {
        for h in ["Rule", "Fired", "Runs", "First", "Last", "Active"] {
            ui.strong(h);
        }
        ui.end_row();
        for e in &r.rules {
            let name = egui::RichText::new(&e.name);
            if e.fired {
                ui.label(name);
                ui.label("yes");
            } else {
                let warn = ui.visuals().warn_fg_color;
                ui.label(name.color(warn));
                ui.colored_label(warn, "never");
            }
            ui.label(e.runs.to_string());
            ui.label(e.first_run.map_or("-".into(), clock));
            ui.label(e.last_run.map_or("-".into(), clock));
            ui.label(if e.active { "on" } else { "off" });
            ui.end_row();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_is_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(61.4), "1:01");
        assert_eq!(clock(-3.0), "0:00");
        assert_eq!(clock(754.6), "12:35");
    }
}
