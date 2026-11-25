//! The "Missions" tab, rules section (docs/phase5.md P5, work item T5): the mission picker, and the rule list a modder knows from the
//! Warcraft III and StarCraft II trigger editors: every rule with its name, Run Once and Is Active, then a Conditions list and an
//! Actions list, items added from the searchable catalogue ([`super::catalogue`]), each item an engine-generated form
//! ([`crate::schema_form`]) with the checker's findings on the field they concern and the suggested spelling one click away.
//!
//! Nothing here restates a rule: Types, fields and findings come from the engine, keys from [`super::doc`], the operations from
//! [`super::rules`], [`super::refs`] and [`super::project`]. Every edit is one undo step of the [`Project`] (typing in one box joins
//! the step before it) and saving writes the diff ([`Project::save`]). The UI state lives in egui memory ([`state`]), so the
//! `show` signatures of the other sections are not touched; [`selected_mission`] tells them which mission is open.
//!
//! PROVISIONAL: the checker runs in-process after every change of the three documents (its cost on a large mission is not measured; move
//! it to a thread if one run is over a frame); reorder is by up/down buttons, not drag; a rule of a retail mission cannot be renamed
//! (the engine merges `rules` by `name`, so a new name would add a second rule) and the order of an extending mission's rules is not
//! changed here until T14 says how a diff states it.

use std::sync::{Arc, Mutex, OnceLock};

use bevy_egui::egui;
use serde_json::Value;

use super::catalogue::{Catalogue, MenuState, Role, show_menu};
use super::doc::{self as mdoc, MissionList};
use super::project::{self as mproject, Origin};
use super::refs::{self, FieldIndex};
use super::rules::{self, Target};
use crate::checks::{self, Finding, Level};
use crate::edit::Edit;
use crate::project::Project;
use crate::schema_form::{self as sf, Names, Schema};

const RED: egui::Color32 = egui::Color32::LIGHT_RED;

/// Id of a widget of this section (what a test or a scroll-to reads with `Context::read_response`). Keys in use: `"filter"`,
/// `"only-problems"`, `"save"`, `"new-id"`, `"new-mission"`, `("mission", id)`, `"add-rule"`, `("rule", i)`, `("once", i)`,
/// `("active", i)`, `("up", i)`, `("down", i)`, `("dup", i)`, `("del", i)`, `"rename"`, `("add-menu", is_condition)`,
/// `("item", is_condition, i)` (the form: fields are [`sf::field_id`] of it), `("item-up" | "item-down" | "item-del", is_condition, i)`.
pub fn wid(key: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::new("rules_view").with(key)
}

fn schema() -> &'static Schema {
    static S: OnceLock<Schema> = OnceLock::new();
    S.get_or_init(Schema::engine)
}

fn catalogue() -> &'static Catalogue {
    static C: OnceLock<Catalogue> = OnceLock::new();
    C.get_or_init(Catalogue::engine)
}

/// The checker's result and the names the dropdowns offer, for the documents as they were when it ran.
struct Cache {
    id: String,
    mission: Value,
    map: Value,
    media: Value,
    findings: Vec<Finding>,
    names: Names,
}

/// What the section remembers between frames.
#[derive(Default)]
pub struct State {
    /// The open mission's id (the folder under `maps/`).
    pub mission: Option<String>,
    /// The selected rule.
    pub rule: usize,
    pub filter: String,
    pub only_problems: bool,
    /// The id typed in "New mission".
    pub new_id: String,
    /// The last thing that was refused or done, shown under the picker.
    pub status: String,
    /// The text of the rename box of the selected rule, while it is being typed.
    rename: Option<(usize, String)>,
    /// The open add menu: which list (true: conditions) and its search.
    menu: Option<(bool, MenuState)>,
    cache: Option<Cache>,
    auto_opened: bool,
}

impl State {
    /// The findings of the last check (empty before the first).
    pub fn findings(&self) -> &[Finding] {
        self.cache.as_ref().map_or(&[], |c| c.findings.as_slice())
    }
}

/// The section's state in egui memory (shared by every caller of the same context).
pub fn state(ctx: &egui::Context) -> Arc<Mutex<State>> {
    let id = egui::Id::new("rules_view-state");
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with::<Arc<Mutex<State>>>(id, Default::default).clone())
}

/// The id of the mission the Missions tab has open, for the other sections.
pub fn selected_mission(ctx: &egui::Context) -> Option<String> {
    state(ctx).lock().ok()?.mission.clone()
}

// ---- small widgets with fixed ids ----

fn chip(ui: &mut egui::Ui, id: egui::Id, text: &str, strong: bool, color: Option<egui::Color32>) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let col = color.unwrap_or(ui.visuals().text_color());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, col);
    let pad = egui::vec2(6.0, 2.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + pad * 2.0, egui::Sense::hover());
    let resp = ui.interact(rect, id, egui::Sense::click());
    let fill = if strong { ui.visuals().selection.bg_fill } else { ui.style().interact(&resp).weak_bg_fill };
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().galley(rect.min + pad, galley, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A button that may be disabled with the reason as its hover text.
fn action(ui: &mut egui::Ui, id: egui::Id, text: &str, refusal: &Option<String>) -> bool {
    match refusal {
        None => chip(ui, id, text, false, None).clicked(),
        Some(why) => {
            chip(ui, id, text, false, Some(ui.visuals().weak_text_color())).on_hover_text(why);
            false
        }
    }
}

// ---- what the user asked for in one frame; applied after drawing, so nothing borrows the project while it is read ----

enum Act {
    Open(String),
    NewMission(String),
    Save,
    Select(usize),
    AddRule,
    Duplicate(usize),
    Delete(usize),
    Move(usize, usize),
    Flag(usize, &'static str, bool),
    Rename(usize, String),
    AddItem(bool, String),
    RemoveItem(bool, usize),
    MoveItem(bool, usize, usize),
    Edit { condition: bool, item: usize, edits: Vec<Edit>, typing: bool },
}

fn report<T>(st: &mut State, r: Result<T, String>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            st.status = e;
            None
        }
    }
}

// ---- the mission picker ----

/// The top of the tab: the missions of the mod and the import, a "New mission" box, Save. Opening one makes it the selected mission
/// ([`selected_mission`]).
pub fn picker(ui: &mut egui::Ui, project: &mut Project) {
    let st = state(ui.ctx());
    let Ok(mut st) = st.lock() else { return };
    picker_with(ui, project, &mut st);
}

/// [`picker`] with the state given.
pub fn picker_with(ui: &mut egui::Ui, project: &mut Project, st: &mut State) {
    let mut acts = vec![];
    let missions = mproject::list_missions(project);
    // another mod was opened since: forget the mission of the one before
    if st.mission.as_ref().is_some_and(|id| !missions.iter().any(|m| &m.id == id)) {
        st.mission = None;
        st.cache = None;
        st.auto_opened = false;
    }
    // a mod with one mission of its own opens it once, so the tab is not empty at the first look
    if st.mission.is_none() && !st.auto_opened {
        st.auto_opened = true;
        let own: Vec<&mproject::MissionEntry> = missions.iter().filter(|m| m.in_mod).collect();
        if let [only] = own.as_slice() {
            open(st, project, &only.id);
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.strong("Mission");
        if missions.is_empty() {
            ui.weak("none yet: make one below");
        }
        for m in &missions {
            let badge = match m.origin {
                Origin::Mod => "",
                Origin::Retail => " (retail)",
                Origin::RetailChanged => " (retail, changed)",
            };
            let title = m.name.as_deref().filter(|n| *n != m.id).map_or(String::new(), |n| format!(" {n}"));
            let on = st.mission.as_deref() == Some(m.id.as_str());
            if chip(ui, wid(("mission", &m.id)), &format!("{}{title}{badge}", m.id), on, None).clicked() {
                acts.push(Act::Open(m.id.clone()));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("New mission");
        ui.add(egui::TextEdit::singleline(&mut st.new_id).id(wid("new-id")).hint_text("id, for example my_mission").desired_width(160.0));
        if chip(ui, wid("new-mission"), "Make", false, None).clicked() {
            acts.push(Act::NewMission(st.new_id.trim().to_string()));
        }
        let dirty = !project.dirty_rels().is_empty();
        if chip(ui, wid("save"), "Save", dirty, None).on_hover_text("Writes what changed as a diff over the import").clicked() {
            acts.push(Act::Save);
        }
    });
    if !st.status.is_empty() {
        ui.colored_label(RED, st.status.clone());
    }
    for a in acts {
        match a {
            Act::Open(id) => open(st, project, &id),
            Act::NewMission(id) => {
                if id.is_empty() {
                    st.status = "type an id for the new mission".into();
                } else if report(st, mproject::new_mission(project, &id, None)).is_some() {
                    st.new_id.clear();
                    open(st, project, &id);
                }
            }
            Act::Save => {
                let r = project.save();
                st.status = if r.errors.is_empty() { String::new() } else { r.errors.join("; ") };
            }
            _ => {}
        }
    }
}

fn open(st: &mut State, project: &mut Project, id: &str) {
    st.status.clear();
    if report(st, mproject::open_mission(project, id)).is_some() {
        st.mission = Some(id.to_string());
        st.rule = 0;
        st.rename = None;
        st.menu = None;
    }
}

// ---- the rule list ----

/// Draws the section for the selected mission.
pub fn show(ui: &mut egui::Ui, project: &mut Project) {
    let st = state(ui.ctx());
    let Ok(mut st) = st.lock() else { return };
    show_with(ui, project, &mut st);
}

/// [`show`] with the state given.
pub fn show_with(ui: &mut egui::Ui, project: &mut Project, st: &mut State) {
    let Some(id) = st.mission.clone() else {
        ui.label("Pick or make a mission above to edit its rules.");
        return;
    };
    let Ok(rel) = mproject::mission_rel(&id) else { return };
    if project.open_doc(&rel).is_err() || mproject::merged_mission(project, &id).is_none() {
        ui.colored_label(RED, format!("{rel} cannot be read"));
        return;
    }
    refresh(st, project, &id);
    // what the document says about adding, deleting and ordering rules (an extending mission may refuse)
    let refusal = mproject::check_add_delete(project, &id, MissionList::Rules).err();
    let all: Vec<Value> = rules::rules(project, &Target::mission(&rel)).to_vec();
    st.rule = st.rule.min(all.len().saturating_sub(1));
    let mut acts = vec![];

    list(ui, st, &all, &refusal, &mut acts);
    if let Some(rule) = all.get(st.rule) {
        ui.separator();
        body(ui, st, project, &rel, rule, &mut acts);
    }
    apply(st, project, &id, &rel, acts);
}

fn refresh(st: &mut State, project: &Project, id: &str) {
    let null = Value::Null;
    let m = mproject::merged_mission(project, id).unwrap_or(&null);
    let mp = mproject::merged_map(project, id).unwrap_or(&null);
    let md = mproject::merged_media(project, id).unwrap_or(&null);
    if st.cache.as_ref().is_some_and(|c| c.id == id && c.mission == *m && c.map == *mp && c.media == *md) {
        return;
    }
    st.cache = Some(Cache { id: id.to_string(), mission: m.clone(), map: mp.clone(), media: md.clone(), findings: checks::run(m, mp, md), names: Names::collect(m, mp, md) });
}

/// Rule `i` as the filter sees it: its name, the Types of its items, and every text its fields hold.
fn matches_filter(rule: &Value, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let n = needle.to_lowercase();
    rule.to_string().to_lowercase().contains(&n)
}

fn rule_findings(found: &[Finding], i: usize) -> usize {
    found.iter().filter(|f| f.rule_index == Some(i)).count()
}

fn list(ui: &mut egui::Ui, st: &mut State, all: &[Value], refusal: &Option<String>, acts: &mut Vec<Act>) {
    let found: Vec<Finding> = st.findings().to_vec();
    let mission_level = found.iter().filter(|f| f.rule_index.is_none()).count();
    let (errors, warns) = (found.iter().filter(|f| f.level == Level::Error).count(), found.iter().filter(|f| f.level != Level::Error).count());
    ui.horizontal(|ui| {
        ui.strong(format!("Rules ({})", all.len()));
        if action(ui, wid("add-rule"), "+ New rule", refusal) {
            acts.push(Act::AddRule);
        }
        ui.add(egui::TextEdit::singleline(&mut st.filter).id(wid("filter")).hint_text("filter by name, Type or field text").desired_width(200.0));
        let on = st.only_problems;
        if chip(ui, wid("only-problems"), "has problems", on, None).clicked() {
            st.only_problems = !on;
        }
        if errors + warns > 0 {
            ui.colored_label(RED, format!("{errors} errors, {warns} warnings"));
        } else {
            ui.weak("no findings");
        }
    });
    if mission_level > 0 {
        ui.weak(format!("{mission_level} findings are about the mission as a whole, not a rule: see the problems panel"));
    }
    let needle = st.filter.trim().to_string();
    egui::ScrollArea::vertical().id_salt("rules-list").max_height(240.0).show(ui, |ui| {
        for (i, r) in all.iter().enumerate() {
            let n = rule_findings(&found, i);
            if (st.only_problems && n == 0) || !matches_filter(r, &needle) {
                continue;
            }
            ui.horizontal(|ui| {
                let name = mdoc::rule_name(r).filter(|n| !n.is_empty()).unwrap_or("(unnamed)");
                let counts = format!("{} when, {} do", mdoc::rule_items(r, true).len(), mdoc::rule_items(r, false).len());
                let label = format!("{}. {name}   {counts}", i + 1);
                if chip(ui, wid(("rule", i)), &label, i == st.rule, None).clicked() {
                    acts.push(Act::Select(i));
                }
                if n > 0 {
                    ui.colored_label(RED, format!("! {n}"));
                }
                let once = flag(r, mdoc::RULE_RUN_ONCE);
                if chip(ui, wid(("once", i)), "run once", once, None).clicked() {
                    acts.push(Act::Flag(i, mdoc::RULE_RUN_ONCE, !once));
                }
                let active = flag(r, mdoc::RULE_IS_ACTIVE);
                if chip(ui, wid(("active", i)), "active", active, None).clicked() {
                    acts.push(Act::Flag(i, mdoc::RULE_IS_ACTIVE, !active));
                }
                if i > 0 && action(ui, wid(("up", i)), "up", refusal) {
                    acts.push(Act::Move(i, i - 1));
                }
                if i + 1 < all.len() && action(ui, wid(("down", i)), "down", refusal) {
                    acts.push(Act::Move(i, i + 1));
                }
                if action(ui, wid(("dup", i)), "copy", refusal) {
                    acts.push(Act::Duplicate(i));
                }
                if action(ui, wid(("del", i)), "delete", refusal) {
                    acts.push(Act::Delete(i));
                }
            });
        }
    });
}

/// `Run Once` / `Is Active` of a rule: both default to true when missing (`schema.rs:832-850` of the engine, PROVISIONAL line numbers);
/// the engine's loader reads JSON booleans.
fn flag(rule: &Value, key: &str) -> bool {
    rule.get(key).and_then(Value::as_bool).unwrap_or(true)
}

fn body(ui: &mut egui::Ui, st: &mut State, project: &Project, rel: &str, rule: &Value, acts: &mut Vec<Act>) {
    let index = st.rule;
    let name = mdoc::rule_name(rule).unwrap_or_default().to_string();
    // rename: a box that commits when it loses focus, so the rules that name this one follow once, not on every key
    if st.rename.as_ref().is_none_or(|(i, _)| *i != index) {
        st.rename = Some((index, name.clone()));
    }
    ui.horizontal(|ui| {
        ui.strong(format!("Rule {}", index + 1));
        let buf = &mut st.rename.as_mut().expect("set above").1;
        let r = ui.add(egui::TextEdit::singleline(buf).id(wid("rename")).desired_width(240.0));
        if r.lost_focus() && *buf != name {
            acts.push(Act::Rename(index, buf.clone()));
        }
        let _ = (rel, project);
    });
    let found: Vec<Finding> = st.findings().to_vec();
    for f in checks::for_rule(&found, index) {
        ui.colored_label(RED, &f.message);
    }
    for is_condition in [true, false] {
        items(ui, st, rule, index, is_condition, &found, acts);
        ui.add_space(4.0);
    }
}

fn items(ui: &mut egui::Ui, st: &mut State, rule: &Value, index: usize, is_condition: bool, found: &[Finding], acts: &mut Vec<Act>) {
    let list_key = mdoc::rule_list_key(is_condition);
    let all = mdoc::rule_items(rule, is_condition);
    let (title, noun) = if is_condition { ("Conditions", "condition") } else { ("Actions", "action") };
    ui.horizontal(|ui| {
        ui.strong(format!("{title} ({})", all.len()));
        if is_condition {
            ui.weak("all must hold");
        } else {
            ui.weak("run in this order");
        }
        let open = st.menu.as_ref().is_some_and(|(c, _)| *c == is_condition);
        if chip(ui, wid(("add-menu", is_condition)), &format!("+ Add {noun}"), open, None).clicked() {
            st.menu = if open { None } else { Some((is_condition, MenuState::new(if is_condition { Role::Condition } else { Role::Action }))) };
        }
    });
    if let Some((c, menu)) = st.menu.as_mut()
        && *c == is_condition
    {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            if let Some(picked) = show_menu(ui, catalogue(), menu) {
                acts.push(Act::AddItem(is_condition, picked));
            }
        });
    }
    let names = st.cache.as_ref().map_or_else(Names::default, |c| Names(c.names.0.clone()));
    for (i, item) in all.iter().enumerate() {
        let item_id = wid(("item", is_condition, i));
        let hits = checks::for_item(found, index, list_key, i);
        let ty = mdoc::item_type(item).unwrap_or("(no Type)");
        let role = if is_condition { Role::Condition } else { Role::Action };
        let entry = catalogue().find(role, ty);
        let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), item_id.with("open"), true);
        state
            .show_header(ui, |ui| {
                let title = egui::RichText::new(format!("{}. {ty}", i + 1));
                ui.label(if hits.is_empty() { title } else { title.color(RED).strong() });
                if let Some(e) = entry {
                    if e.engine {
                        ui.weak("OP");
                    }
                    if e.campaign_only {
                        ui.weak("campaign");
                    }
                }
                if !hits.is_empty() {
                    ui.colored_label(RED, format!("! {}", hits.len()));
                }
                if i > 0 && chip(ui, wid(("item-up", is_condition, i)), "up", false, None).clicked() {
                    acts.push(Act::MoveItem(is_condition, i, i - 1));
                }
                if i + 1 < all.len() && chip(ui, wid(("item-down", is_condition, i)), "down", false, None).clicked() {
                    acts.push(Act::MoveItem(is_condition, i, i + 1));
                }
                if chip(ui, wid(("item-del", is_condition, i)), "remove", false, None).clicked() {
                    acts.push(Act::RemoveItem(is_condition, i));
                }
            })
            .body(|ui| {
                let base = mdoc::rule_item_pointer(index, is_condition, i);
                let edits = sf::item_form_edits(ui, schema(), &names, item, is_condition, &hits, &base, item_id);
                if !edits.is_empty() {
                    // a box with the keyboard is being typed in: that joins the step before; a button or a pick is a step of its own
                    let typing = edits.len() == 1 && ui.ctx().egui_wants_keyboard_input();
                    acts.push(Act::Edit { condition: is_condition, item: i, edits, typing });
                }
            });
    }
}

// ---- applying ----

fn apply(st: &mut State, project: &mut Project, id: &str, rel: &str, acts: Vec<Act>) {
    let t = Target::mission(rel);
    let mut changed = false;
    for a in acts {
        let r: Result<(), String> = match a {
            Act::Open(_) | Act::NewMission(_) | Act::Save => Ok(()),
            Act::Select(i) => {
                st.rule = i;
                st.menu = None;
                Ok(())
            }
            Act::AddRule => add_rule(project, id, rel).map(|at| st.rule = at),
            Act::Duplicate(i) => mproject::check_add_delete(project, id, MissionList::Rules).and_then(|_| rules::duplicate_rule(project, &t, i)).map(|at| st.rule = at),
            Act::Delete(i) => mproject::delete_entry(project, id, MissionList::Rules, i).map(|_| ()),
            Act::Move(from, to) => mproject::check_add_delete(project, id, MissionList::Rules).and_then(|_| rules::move_rule(project, &t, from, to)).map(|_| {
                if st.rule == from {
                    st.rule = to;
                }
            }),
            Act::Flag(i, key, on) => rules::set_rule_flag(project, &t, i, key, on),
            Act::Rename(i, name) => rename_rule(project, rel, i, name.trim()),
            Act::AddItem(is_condition, ty) => {
                let item = schema().find(is_condition, &ty).map(sf::new_item).ok_or_else(|| format!("'{ty}' is not a Type of the rule schema"));
                item.and_then(|item| rules::add_item(project, &t, st.rule, is_condition, item)).map(|_| st.menu = None)
            }
            Act::RemoveItem(c, i) => rules::remove_item(project, &t, st.rule, c, i),
            Act::MoveItem(c, from, to) => rules::move_item(project, &t, st.rule, c, from, to),
            Act::Edit { condition, item, edits, typing } => {
                let label = format!("Edit {}", if condition { "condition" } else { "action" });
                let _ = item;
                project.apply_form(rel, &label, edits, typing)
            }
        };
        match r {
            Ok(()) => changed = true,
            Err(e) => st.status = e,
        }
    }
    if changed {
        st.status.clear();
    }
}

fn add_rule(project: &mut Project, id: &str, rel: &str) -> Result<usize, String> {
    mproject::check_add_delete(project, id, MissionList::Rules)?;
    let names: Vec<String> = rules::rules(project, &Target::mission(rel)).iter().filter_map(|r| mdoc::rule_name(r).map(str::to_string)).collect();
    let taken: Vec<&str> = names.iter().map(String::as_str).collect();
    mproject::add_entry(project, id, MissionList::Rules, mdoc::new_rule(&rules::unique_name(&taken, "Rule")))
}

/// Renames rule `index` and every field of the mission that names it, as one undo step. Refused for an empty name, a name another
/// rule has, and a rule of the retail mission underneath (a new `name` would add a second rule there).
fn rename_rule(project: &mut Project, rel: &str, index: usize, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a rule needs a name".into());
    }
    let doc = project.doc(rel).ok_or_else(|| format!("{rel} is not open"))?;
    let all = mdoc::rules(&doc.edited);
    let old = all.get(index).and_then(mdoc::rule_name).ok_or_else(|| format!("no rule {index}"))?.to_string();
    if old == name {
        return Ok(());
    }
    if all.iter().enumerate().any(|(i, r)| i != index && mdoc::rule_name(r) == Some(name)) {
        return Err(format!("another rule is named '{name}': rule names are how rules refer to each other"));
    }
    if mdoc::rules(&doc.base).iter().any(|r| mdoc::rule_name(r) == Some(old.as_str())) {
        return Err(format!("'{old}' is a rule of the mission this one extends: rename a copy instead"));
    }
    let mut edits = refs::rename_edits(&doc.edited, FieldIndex::engine(), "rule", &old, name)?;
    edits.push(Edit::set(mdoc::rule_pointer(index, Some(mdoc::RULE_NAME)), Value::from(name)));
    project.do_group(&format!("Rename rule {old}"), vec![(rel.to_string(), edits)])
}
