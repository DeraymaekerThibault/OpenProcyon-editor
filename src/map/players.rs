//! Players, teams and starts of a map (docs/phase4.md P5, work item T7): the list and form of the Maps tab's "Players and teams"
//! section, what its buttons and the plan's clicks write (one undo step each, through [`super::project`]'s edit API and so saved as
//! a diff on a map that extends another), and the formation preview drawn around each start. Names no map key: that is
//! [`super::doc`].
//!
//! Refusals are the project's own: a map that extends another can add or delete a player only because `players` merges by `name`
//! in the engine's keyed rule; `teams` is not keyed, so a team cannot be added or deleted there and the panel says why
//! ([`refusals`]).
//!
//! The formation preview calls the engine: [`openprocyon::authoring::formation_slots`], the function the skirmish spawner uses
//! (`game/src/scenario/spawn.rs` places a fleet at `start` turned to `forward`; the same conversion is done here). What the engine
//! does not tell the editor is which ships a player gets and how big they are (that is the lobby's, at play time): the preview shows
//! [`PREVIEW_SHIPS`] ships of [`PREVIEW_RADIUS`] each, both PROVISIONAL and changeable in the panel.

use super::doc::{self, Array, PlayerField};
use super::project::{self as maps, Change};
use super::view2d::Ref;
use crate::edit::{Edit, pointer};
use crate::map_edit::plan_view::PlanView;
use crate::project::Project;
use bevy::math::{Quat, Vec3};
use bevy_egui::egui::{self, Color32, Rect, Stroke};
use openprocyon::authoring::{Formation, formation_slots};
use serde_json::{Value, json};

/// Ships drawn in a formation preview. PROVISIONAL: a guess (a skirmish fleet has a few ships).
pub const PREVIEW_SHIPS: usize = 4;

/// Bounding-sphere radius of each previewed ship. PROVISIONAL: 17 is the Man-O-War's (`game/src/ship.rs`, comment of
/// `formation_slots`); the slot spacing grows with the largest ship.
pub const PREVIEW_RADIUS: f32 = 17.0;

/// The races the form offers by name (docs/phase4.md P5: 0 Navy, 2 Pirate, 3 Procyon, 4 open; the others are unverified, PROVISIONAL).
pub const RACES: [(u64, &str); 4] = [(0, "Navy"), (2, "Pirate"), (3, "Procyon"), (4, "Open")];

/// The race a new team gets: the first team Navy, the second Pirate, the third Procyon, then round again (a map's usual sides).
/// PROVISIONAL: a guess; the form lets the author pick.
pub fn default_team_race(teams_so_far: usize) -> u64 {
    [0, 2, 3][teams_so_far % 3]
}

/// What race 4 means (the engine's `Race::Unchosen`: "no fleet type"): a player's slot is chosen in the lobby or takes the team's race.
pub const OPEN_RACE_HELP: &str = "4 = open: no fleet type of its own. A team of this race leaves the choice to the players in the lobby; a player of this race takes its team's.";

/// The hover text of a race in a picker: the number the file holds, and what "Open" means.
pub fn race_help(n: u64) -> String {
    if n == 4 { format!("race {n}: {OPEN_RACE_HELP}") } else { format!("race {n} in the file") }
}

/// The text for a race number.
pub fn race_label(race: Option<u64>) -> String {
    match race {
        None => "(none)".into(),
        Some(r) => RACES.iter().find(|(n, _)| *n == r).map_or_else(|| format!("race {r}"), |(_, name)| (*name).to_string()),
    }
}

/// The formations the engine knows, as `(index, name)`: the engine's own order and names ([`Formation::from_index`], `Debug`), so a
/// formation added to the engine shows up here.
pub fn formation_names() -> Vec<(u64, String)> {
    (0..32u64).filter(|&i| i == 0 || Formation::from_index(i as usize) != Formation::None).map(|i| (i, format!("{:?}", Formation::from_index(i as usize)))).collect()
}

/// A player's slots in the formation, as game `(x, y)` positions (the first is the leader, on the start). The engine lays the slots
/// out in Bevy axes ([`formation_slots`]) and the spawner turns them by the heading of `forward` about the start; this does the
/// same and converts back. A player with no start has none.
pub fn formation_preview(player: &Value, ships: usize, radius: f32) -> Vec<[f32; 2]> {
    let (Some(start), kind) = (doc::player_start(player), Formation::from_index(doc::player_formation(player).unwrap_or(0) as usize)) else { return vec![] };
    let fwd = doc::player_forward(player).unwrap_or([0.0, 1.0, 0.0]);
    // game (x, y, z) is Bevy (x, z, -y) (`data::conv`); the spawner's heading is atan2(-f.x, -f.z) of the Bevy forward.
    let heading = (-fwd[0]).atan2(fwd[1]);
    let q = Quat::from_rotation_y(heading as f32);
    let origin = Vec3::new(start[0] as f32, start[2] as f32, -start[1] as f32);
    formation_slots(kind, &vec![radius; ships]).into_iter().map(|v| origin + q * v).map(|b| [b.x, -b.z]).collect()
}

/// The first free name of the form "Player N".
pub fn unique_player_name(map: &Value) -> String {
    let names = doc::player_names(map);
    (1..).map(|n| format!("Player {n}")).find(|c| !names.contains(&c.as_str())).expect("a free name exists")
}

/// The first free team text of the form "Team N". PROVISIONAL: real maps use string-table keys (`IDGS_...`) the editor does not know.
pub fn unique_team_id(map: &Value) -> String {
    let ids = doc::team_ids(map);
    (1..).map(|n| format!("Team {n}")).find(|c| !ids.contains(&c.as_str())).expect("a free name exists")
}

/// Why `players` and `teams` of map `id` cannot take an added or a deleted entry (`None`: they can).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refusals {
    pub players: Option<String>,
    pub teams: Option<String>,
}

pub fn refusals(project: &Project, id: &str) -> Refusals {
    let Ok(rel) = maps::map_rel(id) else { return Refusals::default() };
    let why = |which| maps::check_add_delete(project, &rel, which).err();
    Refusals { players: why(Array::Players), teams: why(Array::Teams) }
}

fn rel_of(id: &str) -> Result<String, String> {
    maps::map_rel(id)
}

fn map_of<'a>(project: &'a Project, id: &str) -> Result<&'a Value, String> {
    maps::merged_view(project, id).ok_or_else(|| format!("map '{id}' is not open"))
}

/// Adds a player named `name` (a free "Player N" for `None`) with its start at `at` (the world centre for `None`), on the first
/// team when the map has teams, as one undo step; returns its index.
pub fn add_player(project: &mut Project, id: &str, name: Option<&str>, at: Option<[f32; 2]>) -> Result<usize, String> {
    let map = map_of(project, id)?;
    let name = match name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => unique_player_name(map),
    };
    let team = if doc::teams(map).is_empty() { -1 } else { 0 };
    let at = at.unwrap_or([0.0, 0.0]);
    let start = [super::view2d::short_f64(f64::from(at[0])), super::view2d::short_f64(f64::from(at[1])), 0.0];
    // A player on a team takes the team's race (the lobby would do so for an open one, but the form then shows what the slot is).
    let race = doc::teams(map).get(usize::try_from(team).unwrap_or(usize::MAX)).and_then(doc::team_race).unwrap_or(doc::OPEN_RACE);
    maps::add_entry(project, id, Array::Players, doc::new_player_of(&name, team, start, race))
}

/// `Team 1`: the team's name id, or "team N" (its index) for a team with none.
pub fn team_label(map: &Value, index: usize) -> String {
    doc::teams(map).get(index).and_then(doc::team_name_id).filter(|n| !n.is_empty()).map_or_else(|| format!("team {index}"), str::to_string)
}

/// The player's name, or "player N" for a slot with none.
pub fn player_label(map: &Value, index: usize) -> String {
    doc::players(map).get(index).and_then(doc::player_name).filter(|n| !n.is_empty()).map_or_else(|| format!("player {index}"), str::to_string)
}

/// Sets one field of player `index` as one undo step. A new name must differ from every other player's (the key of a map that
/// extends another; the lobby tells slots apart by it).
pub fn set_field(project: &mut Project, id: &str, index: usize, field: PlayerField, value: Value) -> Result<(), String> {
    let (rel, map) = (rel_of(id)?, map_of(project, id)?);
    let players = doc::players(map);
    let current = players.get(index).ok_or_else(|| format!("{rel}: no player {index}"))?;
    if field == PlayerField::Name {
        let name = value.as_str().map(str::trim).filter(|n| !n.is_empty()).ok_or("a player needs a name")?;
        if players.iter().enumerate().any(|(i, p)| i != index && doc::player_name(p) == Some(name)) {
            return Err(format!("another player is called '{name}' already"));
        }
    }
    if current.get(field.key()) == Some(&value) {
        return Ok(());
    }
    let label = format!("Set {} of {}", field.key(), doc::player_name(current).map_or_else(|| format!("player {index}"), str::to_string));
    project.do_step(&rel, &label, vec![Edit::set(pointer(&["players", &index.to_string(), field.key()]), value)])
}

/// Moves player `index`'s start to `at` (z kept) as one undo step; a player with no `forward` gets the default facing.
pub fn set_start(project: &mut Project, id: &str, index: usize, at: [f32; 2]) -> Result<(), String> {
    let map = map_of(project, id)?;
    let p = doc::players(map).get(index).ok_or_else(|| format!("no player {index}"))?;
    let z = doc::player_start(p).map_or(0.0, |s| s[2]);
    let mut ch = Change::new(Array::Players, index);
    ch.pos = Some([super::view2d::short_f64(f64::from(at[0])), super::view2d::short_f64(f64::from(at[1])), z]);
    if doc::player_forward(p).is_none() {
        ch.forward = Some([0.0, 1.0, 0.0]);
    }
    let label = format!("Place start of {}", doc::player_name(p).unwrap_or("player"));
    maps::change_entries(project, id, &label, &[ch])
}

/// Deletes player `index` as one undo step. The pre-placed ships of the map name their owner by index, so the ones owned by a later
/// player are renumbered in the same step, and a player that still owns ships is not deleted.
pub fn delete_player(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    let rel = rel_of(id)?;
    maps::check_add_delete(project, &rel, Array::Players)?;
    let map = map_of(project, id)?;
    let player = doc::players(map).get(index).ok_or_else(|| format!("{rel}: no player {index}"))?;
    let name = doc::player_name(player).map_or_else(|| format!("player {index}"), str::to_string);
    let mut edits = Vec::new();
    for (i, s) in doc::ships(map).iter().enumerate() {
        match doc::ship_player(s) {
            Some(p) if p == index as u64 => return Err(format!("{name} still owns pre-placed ships (ships #{i}): delete or reassign them first")),
            Some(p) if p > index as u64 => edits.push(Edit::set(pointer(&["ships", &i.to_string(), doc::SHIP_PLAYER_KEY]), json!(p - 1))),
            _ => {}
        }
    }
    edits.push(Edit::remove(pointer(&["players", &index.to_string()])));
    project.do_step(&rel, &format!("Delete player {name}"), edits)
}

/// Adds a team with `name_id` (a free "Team N" for an empty text) as one undo step; returns its index. Refused on a map that
/// extends another (`teams` is not keyed).
pub fn add_team(project: &mut Project, id: &str, name_id: &str) -> Result<usize, String> {
    let map = map_of(project, id)?;
    let name_id = if name_id.trim().is_empty() { unique_team_id(map) } else { name_id.trim().to_string() };
    let race = default_team_race(doc::teams(map).len());
    maps::add_entry(project, id, Array::Teams, doc::new_team_of(&name_id, race))
}

/// Sets team `index`'s fleet type (`race`, a number as a player's) as one undo step.
pub fn set_team_race(project: &mut Project, id: &str, index: usize, race: u64) -> Result<(), String> {
    let (rel, map) = (rel_of(id)?, map_of(project, id)?);
    let team = doc::teams(map).get(index).ok_or_else(|| format!("{rel}: no team {index}"))?;
    if doc::team_race(team) == Some(race) {
        return Ok(());
    }
    let name = doc::team_name_id(team).filter(|n| !n.is_empty()).map_or_else(|| format!("team {index}"), str::to_string);
    project.do_step(&rel, &format!("Set race of {name}"), vec![Edit::set(pointer(&["teams", &index.to_string(), doc::TEAM_RACE_KEY]), json!(race))])
}

/// Turns team `index`'s `lock` on or off as one undo step (written as a boolean; a retail "TRUE" text counts as on).
pub fn set_team_lock(project: &mut Project, id: &str, index: usize, on: bool) -> Result<(), String> {
    let (rel, map) = (rel_of(id)?, map_of(project, id)?);
    let team = doc::teams(map).get(index).ok_or_else(|| format!("{rel}: no team {index}"))?;
    if doc::team_lock(team) == on {
        return Ok(());
    }
    let name = doc::team_name_id(team).filter(|n| !n.is_empty()).map_or_else(|| format!("team {index}"), str::to_string);
    project.do_step(&rel, &format!("Set lock of {name}"), vec![Edit::set(pointer(&["teams", &index.to_string(), doc::TEAM_LOCK_KEY]), json!(on))])
}

/// Sets team `index`'s `name_id` as one undo step.
pub fn rename_team(project: &mut Project, id: &str, index: usize, name_id: &str) -> Result<(), String> {
    let (rel, map) = (rel_of(id)?, map_of(project, id)?);
    let team = doc::teams(map).get(index).ok_or_else(|| format!("{rel}: no team {index}"))?;
    let name_id = name_id.trim();
    if name_id.is_empty() {
        return Err("a team needs a name".into());
    }
    if doc::team_name_id(team) == Some(name_id) {
        return Ok(());
    }
    project.do_step(&rel, &format!("Rename team {index}"), vec![Edit::set(pointer(&["teams", &index.to_string(), doc::TEAM_NAME_KEY]), json!(name_id))])
}

/// Deletes team `index` as one undo step. Players name their team by index, so the ones on it go to no team (-1) and the ones on a
/// later team are renumbered in the same step. Refused on a map that extends another (`teams` is not keyed).
pub fn delete_team(project: &mut Project, id: &str, index: usize) -> Result<(), String> {
    let rel = rel_of(id)?;
    maps::check_add_delete(project, &rel, Array::Teams)?;
    let map = map_of(project, id)?;
    if index >= doc::teams(map).len() {
        return Err(format!("{rel}: no team {index}"));
    }
    let mut edits = Vec::new();
    for (i, p) in doc::players(map).iter().enumerate() {
        let to = match doc::player_team(p) {
            Some(t) if t == index as i64 => -1,
            Some(t) if t > index as i64 => t - 1,
            _ => continue,
        };
        edits.push(Edit::set(pointer(&["players", &i.to_string(), PlayerField::Team.key()]), json!(to)));
    }
    edits.push(Edit::remove(pointer(&["teams", &index.to_string()])));
    project.do_step(&rel, &format!("Delete team {index}"), edits)
}

// ---- the panel ----

/// What a click on the plan is waiting to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    /// Add a player whose start is where the click lands.
    NewPlayer,
    /// Put this player's start where the click lands.
    StartOf(usize),
}

/// What the panel asked for this frame; done after the canvas (one change per frame, against the document that was drawn).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    AddPlayerAtCentre,
    Arm(Arm),
    Set { index: usize, field: PlayerField, value: Value },
    DeletePlayer(usize),
    AddTeam,
    RenameTeam(usize, String),
    SetTeamRace(usize, u64),
    SetTeamLock(usize, bool),
    DeleteTeam(usize),
}

/// The panel's own state: the selected player, a click being waited for, the text being typed and the preview's settings.
pub struct Panel {
    pub selected: Option<usize>,
    pub armed: Option<Arm>,
    pub show_formation: bool,
    pub ships: usize,
    pub radius: f32,
    pub info: String,
    pub error: Option<String>,
    /// The plan's selection last seen, so that a list click is not undone by it.
    plan_seen: Option<usize>,
    /// The player the list wants selected on the plan.
    to_plan: Option<usize>,
    text: Option<(egui::Id, String)>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            selected: None,
            armed: None,
            show_formation: true,
            ships: PREVIEW_SHIPS,
            radius: PREVIEW_RADIUS,
            info: String::new(),
            error: None,
            plan_seen: None,
            to_plan: None,
            text: None,
        }
    }
}

impl Panel {
    /// Follows the plan: one selected start selects its player in the list. Called every frame with the plan's selection.
    pub fn follow_plan(&mut self, refs: &[Ref]) {
        let one = match refs {
            [r] if r.array() == Array::Players => Some(r.index),
            _ => None,
        };
        if one != self.plan_seen {
            self.plan_seen = one;
            if one.is_some() {
                self.selected = one;
            }
        }
    }

    /// The start the list wants selected on the plan, once.
    pub fn take_plan_select(&mut self) -> Option<Ref> {
        self.to_plan.take().map(|i| Ref::new(Array::Players, i))
    }

    /// Forgets what belongs to another map or another document state (a map switch, an add, a delete).
    pub fn reset(&mut self) {
        *self = Panel { show_formation: self.show_formation, ships: self.ships, radius: self.radius, ..Panel::default() };
    }
}

/// A text box that keeps what is typed until it loses focus; returns the text then, if it changed.
fn text_box(ui: &mut egui::Ui, typing: &mut Option<(egui::Id, String)>, id: egui::Id, current: &str, width: f32) -> Option<String> {
    let mut buf = match typing {
        Some((i, b)) if *i == id => b.clone(),
        _ => current.to_string(),
    };
    let r = ui.add(egui::TextEdit::singleline(&mut buf).id(id).desired_width(width));
    if r.lost_focus() {
        *typing = None;
        return (buf != current).then_some(buf);
    }
    if r.has_focus() || r.changed() {
        *typing = Some((id, buf));
    }
    None
}

/// The "Players and teams" section: list and form of the players, the teams, and the formation preview's settings. Reads `map` and
/// returns what was asked; [`apply`] does it.
pub fn show(ui: &mut egui::Ui, st: &mut Panel, map: &Value, refusals: &Refusals) -> Option<Action> {
    let mut action = None;
    let players = doc::players(map);
    let teams = doc::team_ids(map);
    if st.selected.is_some_and(|i| i >= players.len()) {
        st.selected = None;
    }
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(refusals.players.is_none(), egui::Button::new("Add player")).on_hover_text("A new slot at the middle of the map").clicked() {
            action = Some(Action::AddPlayerAtCentre);
        }
        let armed = st.armed == Some(Arm::NewPlayer);
        if ui.add_enabled(refusals.players.is_none(), egui::Button::new("Add player on plan").selected(armed)).on_hover_text("Then click on the plan: Esc stops").clicked() {
            action = Some(Action::Arm(Arm::NewPlayer));
        }
        if ui
            .add_enabled(st.selected.is_some(), egui::Button::new("Set start on plan").selected(matches!(st.armed, Some(Arm::StartOf(_)))))
            .on_hover_text("Then click on the plan: moves the selected player's start")
            .clicked()
            && let Some(i) = st.selected
        {
            action = Some(Action::Arm(Arm::StartOf(i)));
        }
        if ui.add_enabled(st.selected.is_some() && refusals.players.is_none(), egui::Button::new("Delete player")).clicked()
            && let Some(i) = st.selected
        {
            action = Some(Action::DeletePlayer(i));
        }
    });
    if let Some(why) = &refusals.players {
        ui.colored_label(Color32::LIGHT_RED, format!("Players cannot be added or deleted here: {why}"));
    }
    egui::ScrollArea::vertical().id_salt("players-list").max_height(110.0).show(ui, |ui| {
        for (i, p) in players.iter().enumerate() {
            let team = doc::player_team(p).filter(|&t| t >= 0).and_then(|t| teams.get(t as usize).copied());
            let text = format!(
                "{i}  {}{}{}",
                doc::player_name(p).unwrap_or("(no name)"),
                team.map_or(String::new(), |t| format!("  [{t}]")),
                if doc::player_start(p).is_none() { "  (no start)" } else { "" }
            );
            if ui.selectable_label(st.selected == Some(i), text).clicked() {
                st.selected = Some(i);
                st.to_plan = Some(i);
            }
        }
        if players.is_empty() {
            ui.label("No players.");
        }
    });
    if let Some(i) = st.selected {
        let p = &players[i];
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            if let Some(n) = text_box(ui, &mut st.text, egui::Id::new(("player-name", i)), doc::player_name(p).unwrap_or(""), 140.0) {
                action = Some(Action::Set { index: i, field: PlayerField::Name, value: json!(n) });
            }
            ui.label("Team");
            let team = doc::player_team(p).unwrap_or(-1);
            let label = if team < 0 { "none".to_string() } else { teams.get(team as usize).map_or_else(|| format!("team {team}"), |t| (*t).to_string()) };
            // Fixed width: a longer team name must not widen the box and push Playable and Race right.
            crate::layout::fixed_width(ui, 130.0, |ui| {
                egui::ComboBox::from_id_salt(("player-team", i)).selected_text(label).width(130.0).truncate().show_ui(ui, |ui| {
                    if ui.selectable_label(team < 0, "none").clicked() {
                        action = Some(Action::Set { index: i, field: PlayerField::Team, value: json!(-1) });
                    }
                }
            });
            let mut playable = doc::player_playable(p);
            if ui.checkbox(&mut playable, "Playable").on_hover_text("A slot a human may take").changed() {
                action = Some(Action::Set { index: i, field: PlayerField::Playable, value: json!(playable) });
            }
            ui.label("Race");
            let race = doc::player_race(p);
            egui::ComboBox::from_id_salt(("player-race", i)).selected_text(race_label(race)).show_ui(ui, |ui| {
                for (n, name) in RACES {
                    if ui.selectable_label(race == Some(n), name).on_hover_text(race_help(n)).clicked() {
                        action = Some(Action::Set { index: i, field: PlayerField::Race, value: json!(n) });
                    }
                }
            });
            ui.label("Formation");
            let f = doc::player_formation(p).unwrap_or(0);
            let names = formation_names();
            let shown = names.iter().find(|(n, _)| *n == f).map_or_else(|| format!("formation {f}"), |(_, name)| name.clone());
            egui::ComboBox::from_id_salt(("player-formation", i)).selected_text(shown).show_ui(ui, |ui| {
                for (n, name) in &names {
                    if ui.selectable_label(f == *n, name).clicked() {
                        action = Some(Action::Set { index: i, field: PlayerField::Formation, value: json!(n) });
                    }
                }
            });
        });
        match doc::player_start(p) {
            Some(s) => ui.label(format!("Start {:.0}, {:.0}, {:.0}. Drag or turn it on the plan.", s[0], s[1], s[2])),
            None => ui.label("No start yet: use \"Set start on plan\"."),
        };
    }
    ui.separator();
    ui.label("Teams");
    for (t, name) in teams.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            // The name is the label; the number is the one a player's "Team" refers to, shown only as a hover.
            if let Some(n) = text_box(ui, &mut st.text, egui::Id::new(("team-name", t)), name, 110.0) {
                action = Some(Action::RenameTeam(t, n));
            }
            let team = &doc::teams(map)[t];
            let race = doc::team_race(team);
            egui::ComboBox::from_id_salt(("team-race", t))
                .selected_text(match race {
                    None => "race: none".to_string(),
                    Some(_) => format!("race: {}", race_label(race)),
                })
                .show_ui(ui, |ui| {
                    for (n, rname) in RACES {
                        if ui.selectable_label(race == Some(n), rname).on_hover_text(race_help(n)).clicked() {
                            action = Some(Action::SetTeamRace(t, n));
                        }
                    }
                });
            let mut lock = doc::team_lock(team);
            if ui.checkbox(&mut lock, "Lock").on_hover_text("The game file's \"Race Lock\" (PROVISIONAL: the engine does not read it)").changed() {
                action = Some(Action::SetTeamLock(t, lock));
            }
            if ui.add_enabled(refusals.teams.is_none(), egui::Button::new("x")).on_hover_text("Delete this team; its players go to no team").clicked() {
                action = Some(Action::DeleteTeam(t));
            }
        });
    }
    if ui.add_enabled(refusals.teams.is_none(), egui::Button::new("Add team")).clicked() {
        action = Some(Action::AddTeam);
    }
    if let Some(why) = &refusals.teams {
        ui.colored_label(Color32::LIGHT_RED, format!("Teams cannot be added or deleted here: {why}"));
    }
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut st.show_formation, "Formation preview");
        ui.add(egui::DragValue::new(&mut st.ships).range(1..=12).suffix(" ships"));
        ui.add(egui::DragValue::new(&mut st.radius).range(1.0..=200.0).speed(0.5).prefix("radius "));
        ui.label("(PROVISIONAL: which ships a slot gets is the lobby's)");
    });
    if let Some(e) = &st.error {
        ui.colored_label(Color32::LIGHT_RED, e);
    } else if !st.info.is_empty() {
        ui.label(&st.info);
    }
    action
}

/// Does what the panel asked, as one undo step. Returns the entry to select on the plan, when the action made or moved a start.
/// `label(map, index)` of the map as it is now (the index itself when the map cannot be read).
fn label_in(project: &Project, id: &str, index: usize, label: fn(&Value, usize) -> String) -> String {
    map_of(project, id).map_or_else(|_| index.to_string(), |m| label(m, index))
}

pub fn apply(st: &mut Panel, project: &mut Project, id: &str, action: Action) -> Option<Ref> {
    let mut select = None;
    let r: Result<String, String> = match action {
        Action::AddPlayerAtCentre => add_player(project, id, None, None).map(|i| {
            st.selected = Some(i);
            select = Some(Ref::new(Array::Players, i));
            format!("Added player {}.", label_in(project, id, i, player_label))
        }),
        Action::Arm(a) => {
            st.armed = if st.armed == Some(a) { None } else { Some(a) };
            Ok(if st.armed.is_some() { "Click on the plan to place the start (Esc stops).".into() } else { String::new() })
        }
        Action::Set { index, field, value } => set_field(project, id, index, field, value).map(|()| String::new()),
        Action::DeletePlayer(i) => {
            let label = label_in(project, id, i, player_label);
            delete_player(project, id, i).map(|()| {
                st.selected = None;
                format!("Deleted player {label}.")
            })
        }
        Action::AddTeam => add_team(project, id, "").map(|i| format!("Added team {}.", label_in(project, id, i, team_label))),
        Action::RenameTeam(i, n) => rename_team(project, id, i, &n).map(|()| String::new()),
        Action::SetTeamRace(i, r) => set_team_race(project, id, i, r).map(|()| String::new()),
        Action::SetTeamLock(i, on) => set_team_lock(project, id, i, on).map(|()| String::new()),
        Action::DeleteTeam(i) => {
            let label = label_in(project, id, i, team_label);
            delete_team(project, id, i).map(|()| format!("Deleted team {label}."))
        }
    };
    match r {
        Ok(info) => {
            st.error = None;
            st.info = info;
        }
        Err(e) => st.error = Some(e),
    }
    select
}

/// A click on the plan while a placement is armed: adds the player or moves the start, at `at`, and disarms. `None` when nothing was
/// armed. Returns the entry to select.
pub fn place_click(st: &mut Panel, project: &mut Project, id: &str, at: [f32; 2]) -> Option<Ref> {
    let arm = st.armed.take()?;
    let r = match arm {
        Arm::NewPlayer => add_player(project, id, None, Some(at)).map(|i| (i, format!("Added player {} at {:.0}, {:.0}.", label_in(project, id, i, player_label), at[0], at[1]))),
        Arm::StartOf(i) => set_start(project, id, i, at).map(|()| (i, format!("Start of player {} placed at {:.0}, {:.0}.", label_in(project, id, i, player_label), at[0], at[1]))),
    };
    match r {
        Ok((i, info)) => {
            st.selected = Some(i);
            st.error = None;
            st.info = info;
            Some(Ref::new(Array::Players, i))
        }
        Err(e) => {
            st.error = Some(e);
            None
        }
    }
}

/// Draws every start's formation slots on the plan: a circle of the previewed ship's size at each slot, the selected player's
/// brighter, and its slot numbers.
pub fn draw_formations(painter: &egui::Painter, view: &PlanView, rect: Rect, map: &Value, st: &Panel) {
    if !st.show_formation {
        return;
    }
    for (i, p) in doc::players(map).iter().enumerate() {
        let slots = formation_preview(p, st.ships, st.radius);
        if slots.len() < 2 {
            continue;
        }
        let on = st.selected == Some(i);
        let col = if on { Color32::from_rgb(140, 255, 160) } else { Color32::from_rgb(90, 200, 110).gamma_multiply(0.55) };
        let r = (st.radius * view.scale).max(3.0);
        let lead = view.to_screen(rect, slots[0]);
        for (n, s) in slots.iter().enumerate().skip(1) {
            let c = view.to_screen(rect, *s);
            painter.line_segment([lead, c], Stroke::new(1.0, col.gamma_multiply(0.5)));
            painter.circle_stroke(c, r, Stroke::new(if on { 1.5 } else { 1.0 }, col));
            if on {
                painter.text(c, egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::monospace(10.0), col);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::project::open_map;
    use std::path::{Path, PathBuf};

    fn put(root: &Path, rel: &str, v: &Value) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
    }

    fn red() -> Value {
        json!({
            "name": "Red", "world_half": 1000.0,
            "teams": [{"name_id": "T1"}, {"name_id": "T2"}],
            "players": [
                {"name": "A", "team": 0, "start": [0.0, 0.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 5, "playable": true, "race": 0},
                {"name": "B", "team": 1, "start": [500.0, 500.0, 0.0], "forward": [1.0, 0.0, 0.0], "formation": 2, "playable": false, "race": 2},
                {"name": "C", "team": 1, "start": [-500.0, 0.0], "forward": [0.0, -1.0, 0.0], "formation": 0, "playable": false, "race": 3}
            ],
            "ships": [{"id": 1, "type": "S", "player": 2, "pos": [0, 0, 0]}, {"id": 2, "type": "S", "player": 1, "pos": [0, 0, 0]}]
        })
    }

    /// A project with an import holding `red` (an extending map when opened) and a new whole map `mine` copied from it.
    fn setup(name: &str) -> (PathBuf, Project) {
        let root = std::env::temp_dir().join(format!("ope-players-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (assets, m) = (root.join("assets"), root.join("mod"));
        put(&assets, "maps/red/map.json", &red());
        std::fs::create_dir_all(&m).unwrap();
        let mut p = Project::open(&m, Some(assets)).unwrap();
        open_map(&mut p, "red").unwrap();
        let mut mine = red();
        mine["ships"] = json!([]);
        p.open_new_doc("maps/mine/map.json", None, mine).unwrap();
        (root, p)
    }

    fn map<'a>(p: &'a Project, id: &str) -> &'a Value {
        maps::merged_view(p, id).unwrap()
    }

    #[test]
    fn formation_slots_come_from_the_engine_and_turn_with_forward() {
        let facing_north = json!({"start": [100.0, 200.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 5}); // Column
        let s = formation_preview(&facing_north, 3, 17.0);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], [100.0, 200.0], "the leader is on the start");
        // a column trails behind the leader: the engine's 2R + 30 = 64 each
        assert!((s[1][0] - 100.0).abs() < 0.01 && (s[1][1] - 136.0).abs() < 0.01, "{:?}", s[1]);
        assert!((s[2][1] - 72.0).abs() < 0.01);
        let facing_east = json!({"start": [100.0, 200.0, 0.0], "forward": [1.0, 0.0, 0.0], "formation": 5});
        let e = formation_preview(&facing_east, 2, 17.0);
        assert!((e[1][0] - 36.0).abs() < 0.01 && (e[1][1] - 200.0).abs() < 0.01, "{:?}", e[1]);
        // line abreast spreads sideways: the first follower one R + 30 to the left of a north-facing leader
        let line = json!({"start": [0.0, 0.0], "forward": [0.0, 1.0, 0.0], "formation": 2});
        let l = formation_preview(&line, 3, 17.0);
        assert!((l[1][0] + 47.0).abs() < 0.01 && l[1][1].abs() < 0.01, "{:?}", l[1]);
        assert!((l[2][0] - 47.0).abs() < 0.01);
        // none: everyone on the start; no start: nothing
        assert!(formation_preview(&json!({"start": [1.0, 2.0, 0.0], "formation": 0}), 3, 17.0).iter().all(|s| *s == [1.0, 2.0]));
        assert!(formation_preview(&json!({"formation": 5}), 3, 17.0).is_empty());
    }

    #[test]
    fn formation_and_race_names_follow_the_engine() {
        let f = formation_names();
        assert_eq!(f[0], (0, "None".to_string()));
        assert!(f.iter().any(|(_, n)| n == "Diamond") && f.len() >= 8);
        assert_eq!((race_label(Some(0)), race_label(Some(9)), race_label(None)), ("Navy".into(), "race 9".into(), "(none)".into()));
    }

    #[test]
    fn free_names() {
        let m = red();
        assert_eq!(unique_player_name(&m), "Player 1");
        assert_eq!(unique_player_name(&json!({"players": [{"name": "Player 1"}]})), "Player 2");
        assert_eq!(unique_team_id(&json!({"teams": [{"name_id": "Team 1"}]})), "Team 2");
    }

    #[test]
    fn add_set_rename_and_undo_on_a_map_of_the_mods_own() {
        let (root, mut p) = setup("add");
        let i = add_player(&mut p, "mine", None, Some([12.0, 34.0])).unwrap();
        assert_eq!(i, 3);
        let n = &doc::players(map(&p, "mine"))[3];
        assert_eq!((doc::player_name(n), doc::player_team(n), doc::player_start(n)), (Some("Player 1"), Some(0), Some([12.0, 34.0, 0.0])));
        assert!(p.undo_label().unwrap().starts_with("Add players"));
        set_field(&mut p, "mine", 3, PlayerField::Name, json!("Zed")).unwrap();
        set_field(&mut p, "mine", 3, PlayerField::Playable, json!(true)).unwrap();
        set_field(&mut p, "mine", 3, PlayerField::Race, json!(2)).unwrap();
        set_field(&mut p, "mine", 3, PlayerField::Formation, json!(6)).unwrap();
        set_field(&mut p, "mine", 3, PlayerField::Team, json!(1)).unwrap();
        let n = &doc::players(map(&p, "mine"))[3];
        assert_eq!(
            (doc::player_name(n), doc::player_playable(n), doc::player_race(n), doc::player_formation(n), doc::player_team(n)),
            (Some("Zed"), true, Some(2), Some(6), Some(1))
        );
        let before = p.undo_label().map(str::to_string);
        set_field(&mut p, "mine", 3, PlayerField::Team, json!(1)).unwrap();
        assert_eq!(p.undo_label().map(str::to_string), before, "setting the same value pushes no step");
        assert!(set_field(&mut p, "mine", 3, PlayerField::Name, json!("A")).unwrap_err().contains("already"));
        assert!(set_field(&mut p, "mine", 3, PlayerField::Name, json!("  ")).is_err());
        assert!(set_field(&mut p, "mine", 9, PlayerField::Race, json!(0)).is_err());
        for _ in 0..6 {
            p.undo();
        }
        assert_eq!(doc::players(map(&p, "mine")).len(), 3, "every edit was one undo step");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deleting_a_player_renumbers_its_ships_and_refuses_when_it_owns_some() {
        let (root, mut p) = setup("delplayer");
        // on the extending map `red`: players are keyed by name, so a delete is allowed; player 2 owns ship 1
        let e = delete_player(&mut p, "red", 2).unwrap_err();
        assert!(e.contains("still owns") && e.contains('C'), "{e}");
        delete_player(&mut p, "red", 0).unwrap();
        let m = map(&p, "red");
        assert_eq!(doc::player_names(m), vec!["B", "C"]);
        assert_eq!(doc::ship_player(&doc::ships(m)[0]), Some(1), "the ships of later players follow");
        assert_eq!(doc::ship_player(&doc::ships(m)[1]), Some(0));
        p.undo();
        assert_eq!(doc::player_names(map(&p, "red")), vec!["A", "B", "C"]);
        assert_eq!(doc::ship_player(&doc::ships(map(&p, "red"))[0]), Some(2));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn teams_cannot_be_added_or_deleted_on_an_extending_map_and_say_why() {
        let (root, mut p) = setup("teams");
        let r = refusals(&p, "red");
        assert!(r.players.is_none(), "players merge by name: {:?}", r.players);
        let why = r.teams.expect("teams are not keyed");
        assert!(why.contains("/teams") && why.contains("not merged by key"), "{why}");
        assert_eq!(add_team(&mut p, "red", "X").unwrap_err(), why);
        assert_eq!(delete_team(&mut p, "red", 0).unwrap_err(), why);
        assert!(p.undo_label().is_none(), "a refusal changes nothing");
        // a rename is a set, which an extending map can write
        rename_team(&mut p, "red", 0, "Blue").unwrap();
        assert_eq!(doc::team_ids(map(&p, "red")), vec!["Blue", "T2"]);
        assert_eq!(refusals(&p, "mine"), Refusals::default(), "a whole map takes both");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_player_takes_its_teams_race_and_the_messages_name_the_team() {
        let (root, mut p) = setup("playerrace");
        set_team_race(&mut p, "mine", 0, 2).unwrap();
        let i = add_player(&mut p, "mine", None, None).unwrap();
        assert_eq!(doc::player_race(&doc::players(map(&p, "mine"))[i]), Some(2), "the first team is Pirate");
        let mut st = Panel::default();
        apply(&mut st, &mut p, "mine", Action::AddTeam);
        let n = doc::teams(map(&p, "mine")).len();
        assert_eq!(st.info, format!("Added team {}.", team_label(map(&p, "mine"), n - 1)));
        assert!(st.info.contains("Team") && !st.info.contains(&format!("team {}", n - 1)), "{}", st.info);
        apply(&mut st, &mut p, "mine", Action::DeleteTeam(n - 1));
        assert!(st.info.starts_with("Deleted team Team"), "{}", st.info);
        apply(&mut st, &mut p, "mine", Action::AddPlayerAtCentre);
        assert!(st.info.starts_with("Added player Player"), "{}", st.info);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_team_has_a_race_and_the_form_sets_race_and_lock_as_one_step_each() {
        let (root, mut p) = setup("teamrace");
        let t = add_team(&mut p, "mine", "").unwrap();
        let team = &doc::teams(map(&p, "mine"))[t];
        assert_eq!((doc::team_race(team), doc::team_lock(team)), (Some(default_team_race(2)), false));
        set_team_race(&mut p, "mine", t, 3).unwrap();
        set_team_lock(&mut p, "mine", t, true).unwrap();
        let team = &doc::teams(map(&p, "mine"))[t];
        assert_eq!((doc::team_race(team), doc::team_lock(team), team["lock"].clone()), (Some(3), true, json!(true)));
        let label = p.undo_label().map(str::to_string);
        set_team_lock(&mut p, "mine", t, true).unwrap();
        assert_eq!(p.undo_label().map(str::to_string), label, "no change, no step");
        assert!(set_team_race(&mut p, "mine", 9, 0).is_err());
        // an old team with no race gets one on an extending map too (a set of a key the parent lacks)
        set_team_race(&mut p, "red", 0, 0).unwrap();
        assert_eq!(doc::team_race(&doc::teams(map(&p, "red"))[0]), Some(0));
        assert!(doc::team_lock(&json!({"lock": "TRUE"})) && !doc::team_lock(&json!({"lock": "FALSE"})), "retail's text spelling reads too");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deleting_a_team_moves_its_players_and_renumbers_the_later_ones_in_one_step() {
        let (root, mut p) = setup("delteam");
        delete_team(&mut p, "mine", 0).unwrap();
        let m = map(&p, "mine");
        assert_eq!(doc::team_ids(m), vec!["T2"]);
        let teams: Vec<_> = doc::players(m).iter().map(|x| doc::player_team(x).unwrap()).collect();
        assert_eq!(teams, vec![-1, 0, 0]);
        p.undo();
        let teams: Vec<_> = doc::players(map(&p, "mine")).iter().map(|x| doc::player_team(x).unwrap()).collect();
        assert_eq!((teams, doc::teams(map(&p, "mine")).len()), (vec![0, 1, 1], 2), "one undo restores both");
        let t = add_team(&mut p, "mine", "").unwrap();
        assert_eq!((t, doc::team_ids(map(&p, "mine"))[2]), (2, "Team 1"));
        assert!(delete_team(&mut p, "mine", 7).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_start_is_placed_with_a_click_and_keeps_z_and_the_two_number_shape() {
        let (root, mut p) = setup("start");
        let mut st = Panel::default();
        assert!(place_click(&mut st, &mut p, "mine", [1.0, 2.0]).is_none(), "nothing armed, nothing done");
        st.armed = Some(Arm::NewPlayer);
        let r = place_click(&mut st, &mut p, "mine", [10.5, -20.0]).unwrap();
        assert_eq!((r, st.armed, st.selected), (Ref::new(Array::Players, 3), None, Some(3)));
        assert_eq!(doc::player_start(&doc::players(map(&p, "mine"))[3]), Some([10.5, -20.0, 0.0]));
        st.armed = Some(Arm::StartOf(2)); // "C" has a two-number start and a forward
        place_click(&mut st, &mut p, "mine", [7.0, 8.0]).unwrap();
        let c = &doc::players(map(&p, "mine"))[2];
        assert_eq!((doc::player_start(c), doc::player_forward(c)), (Some([7.0, 8.0, 0.0]), Some([0.0, -1.0, 0.0])));
        assert_eq!(c["start"], json!([7.0, 8.0]));
        st.armed = Some(Arm::StartOf(9));
        assert!(place_click(&mut st, &mut p, "mine", [0.0, 0.0]).is_none() && st.error.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_plan_selection_selects_the_player_and_the_list_selects_on_the_plan() {
        let mut st = Panel::default();
        st.follow_plan(&[Ref::new(Array::Players, 2)]);
        assert_eq!(st.selected, Some(2));
        st.selected = Some(1); // a list click
        st.follow_plan(&[Ref::new(Array::Players, 2)]);
        assert_eq!(st.selected, Some(1), "the same plan selection does not undo a list click");
        st.follow_plan(&[Ref::new(Array::Objects, 0)]);
        assert_eq!(st.selected, Some(1));
        st.to_plan = Some(1);
        assert_eq!(st.take_plan_select(), Some(Ref::new(Array::Players, 1)));
        assert!(st.take_plan_select().is_none());
    }

    #[test]
    fn the_panel_draws_and_asks_for_what_a_click_on_a_button_means() {
        let (root, p) = setup("ui");
        let mut st = Panel { selected: Some(0), ..Panel::default() };
        let ctx = egui::Context::default();
        let m = map(&p, "red").clone();
        let r = refusals(&p, "red");
        let screen = Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(900.0, 600.0));
        let mut asked = vec![];
        ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            asked.push(show(ui, &mut st, &m, &r));
            let view = PlanView::default();
            draw_formations(&ui.painter_at(screen), &view, screen, &m, &st);
        })
        .textures_delta
        .clear();
        assert_eq!(asked, vec![None], "drawing alone asks for nothing");
        let _ = std::fs::remove_dir_all(&root);
    }
}
