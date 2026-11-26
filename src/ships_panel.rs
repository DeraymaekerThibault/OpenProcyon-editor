//! The "Ships" tab: the list of ships of the import and the mod, the schema form of one ship (`ship.json`), new ships (extending
//! one, or from the player's own GLB), the comparison table, and the link to the 3D view (the view's edits go through the project's
//! undo stack, and the view follows undo and redo). The rules are the engine's: this module only lists, opens and edits documents.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy::prelude::{ResMut, Resource};
use bevy_egui::egui;
use openprocyon::authoring::{ModEntry, read_merged};
use openprocyon::data::derive;
use serde_json::{Value, json};

use crate::compare;
use crate::edit::{dotted_to_pointer, escape};
use crate::project::{Doc, DocKind, Project};
use crate::project_panel::{self, Focus};
use crate::ship_view::{Loaded, ShipView};

/// Where a ship stands against the import and the mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShipStatus {
    /// Only in the import; the mod does not touch it.
    Retail,
    /// The mod has a `ship.json` that extends a ship.
    Extends,
    /// The mod's `ship.json` replaces the retail one whole.
    Overrides,
    /// A ship of the mod the import does not have.
    Added,
    /// Not written yet anywhere: a document started in this session.
    New,
}

impl ShipStatus {
    /// The word shown beside the ship.
    pub fn label(self) -> &'static str {
        match self {
            ShipStatus::Retail => "retail",
            ShipStatus::Extends => "extends",
            ShipStatus::Overrides => "overrides",
            ShipStatus::Added => "added",
            ShipStatus::New => "new",
        }
    }
}

/// One line of the ship list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShipRow {
    pub id: String,
    pub status: ShipStatus,
    /// The ship's document has edits not saved.
    pub unsaved: bool,
}

/// `ships/<id>/ship.json`.
pub fn ship_rel(id: &str) -> String {
    format!("ships/{id}/ship.json")
}

/// The ship id of a path `ships/<id>/ship.json`.
pub fn ship_id_of(rel: &str) -> Option<&str> {
    let rest = rel.strip_prefix("ships/")?.strip_suffix("/ship.json")?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

/// Every ship id with a `ships/<id>/ship.json` below `root`.
pub fn list_ship_ids(root: &Path) -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(root.join("ships")) else { return vec![] };
    let mut ids: Vec<String> = dir.flatten().filter(|e| e.path().join("ship.json").is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    ids.sort();
    ids
}

/// The guns table, relative to the mod folder.
const GUNS_REL: &str = "data/weapons/guns.json";

/// The pointer shape of a gun mount's `gun` field.
const GUN_FIELD: &str = "/guns/*/gun";

/// The names in a guns table (its entries; keys that start with `$` are not guns).
pub fn gun_names(table: &Value) -> Vec<String> {
    table.as_object().map(|m| m.keys().filter(|k| !k.starts_with('$')).cloned().collect()).unwrap_or_default()
}

/// The rows of the ship list, sorted by id and filtered by `filter` (case-insensitive part of the id). `listing` is the ids found
/// in the overlay root (or the import alone), `entry` what the engine says the mod does, `unsaved` the relative paths of documents
/// with unsaved edits. A ship with only an unsaved document is `New`.
pub fn ship_rows(listing: &[String], entry: Option<&ModEntry>, unsaved: &[String], filter: &str) -> Vec<ShipRow> {
    let mut ids: BTreeSet<String> = listing.iter().cloned().collect();
    ids.extend(unsaved.iter().filter_map(|r| ship_id_of(r)).map(str::to_string));
    let filter = filter.trim().to_lowercase();
    ids.into_iter()
        .filter(|id| filter.is_empty() || id.to_lowercase().contains(&filter))
        .map(|id| {
            let rel = ship_rel(&id);
            let has = |l: Option<&Vec<String>>| l.is_some_and(|l| l.contains(&rel));
            let status = if has(entry.map(|e| &e.added)) {
                ShipStatus::Added
            } else if has(entry.map(|e| &e.extends)) {
                ShipStatus::Extends
            } else if has(entry.map(|e| &e.overrides)) {
                ShipStatus::Overrides
            } else if listing.contains(&id) {
                ShipStatus::Retail
            } else {
                ShipStatus::New
            };
            ShipRow { unsaved: unsaved.contains(&rel), id, status }
        })
        .collect()
}

/// What a ship's document is, in words: a diff over the retail ship, an extension of another ship, a new file, or a replacement.
pub fn doc_note(doc: &Doc, id: &str) -> String {
    match (doc.kind, doc.parent.as_deref()) {
        (DocKind::Extends, Some(p)) if p == id => format!("Changes the retail ship {id} (saved as an `extends` file with only the changes)"),
        (DocKind::Extends, Some(p)) => format!("extends {p}"),
        (DocKind::Extends, None) => "new file".into(),
        (DocKind::Replaced, _) => "replaces the file below".into(),
        (DocKind::Merged, _) => "merged into the file below".into(),
    }
}

/// A ship id usable as a folder name (a path-safety check only; what the game accepts is the engine's rule).
pub fn check_new_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(()) } else { Err(format!("'{id}': a ship id is letters, digits, '_' and '-'")) }
}

/// The id of the import's ship whose `model.glb` is byte for byte `bytes`, if any (a mod must not ship the game's own models).
/// Compares sizes first.
pub fn retail_glb_match(bytes: &[u8], import: &Path) -> Option<String> {
    for id in list_dirs(&import.join("ships")) {
        let p = import.join("ships").join(&id).join("model.glb");
        if std::fs::metadata(&p).is_ok_and(|m| m.len() == bytes.len() as u64) && std::fs::read(&p).is_ok_and(|b| b == bytes) {
            return Some(id);
        }
    }
    None
}

fn list_dirs(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir).map(|d| d.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default()
}

/// Starts a ship from a GLB of the player's own: copies it to `<mod>/ships/<id>/model.glb` and opens a minimal `ship.json` document
/// (`race`, `points`, `skirmish`) saved with the next save. Refused for a model identical to one of the import's, and for a file
/// that is no readable binary glTF.
pub fn new_ship_from_glb(project: &mut Project, id: &str, glb: &Path) -> Result<(), String> {
    check_new_id(id)?;
    let bytes = std::fs::read(glb).map_err(|e| format!("{}: {e}", glb.display()))?;
    if derive::read(&bytes).is_none() {
        return Err(format!("{}: not a readable binary glTF", glb.display()));
    }
    if let Some(import) = &project.import
        && let Some(other) = retail_glb_match(&bytes, import)
    {
        return Err(format!("{}: identical to the game's own model of {other}; use your own model, or extend {other} instead", glb.display()));
    }
    let rel = ship_rel(id);
    if project.docs.iter().any(|d| d.rel == rel) || project.root.join(&rel).exists() || project.dir.join("ships").join(id).exists() {
        return Err(format!("{id}: a ship of that id exists already"));
    }
    let dest = project.dir.join("ships").join(id).join("model.glb");
    crate::project::guard::write(&dest, &bytes, project.import.as_deref())?;
    // the editor's own write: take the file into the layer list now, so the next poll does not announce it as an outside change
    project.rebuild()?;
    project.open_new_doc(&rel, None, json!({"race": "", "points": 0, "skirmish": false})).map(|_| ())
}

/// Starts a ship that extends `parent` (nothing is written until it is edited).
pub fn new_ship_extending(project: &mut Project, id: &str, parent: &str) -> Result<(), String> {
    check_new_id(id)?;
    project.open_new_doc(&ship_rel(id), Some(parent), Value::Null).map(|_| ())
}

// ---------------------------------------------------------------- the 3D link

/// Which project ship the 3D view shows, so its edits go to that document. Cleared by loading anything else into the view.
#[derive(Resource, Default, Debug)]
pub struct Linked {
    pub rel: Option<String>,
    generation: u64,
}

/// The ship `id` of the project as the 3D view wants it: the document's merged `ship.json`, and the model `ships/<model or id>/model.glb`
/// read from the mod folder first, then the overlay (so a model copied in this session shows before the first save).
/// PROVISIONAL: the model is looked up by the rule of `docs/data-format.md` (`model` names another ship's folder).
pub fn loaded_from_project(project: &Project, id: &str) -> Result<Loaded, String> {
    let doc = project.doc(&ship_rel(id)).ok_or_else(|| format!("{id}: not open"))?;
    let model = doc.edited.get("model").and_then(Value::as_str).unwrap_or(id);
    let sub = format!("ships/{model}/model.glb");
    let path = [project.dir.join(&sub), project.root.join(&sub)].into_iter().find(|p| p.is_file());
    let (glb, facts) = match path {
        Some(p) => {
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let facts = derive::read(&bytes).ok_or_else(|| format!("{}: not a readable binary glTF", p.display()))?;
            (Some(p), Some(facts))
        }
        None => (None, None),
    };
    Ok(Loaded { ship_id: id.to_string(), ship: doc.edited.clone(), glb, facts })
}

/// Where the project's ship `id` comes from, for the 3D tab's status line: a ship only the mod has ("from the mod project"), a retail
/// ship the mod leaves alone ("retail"), or a retail ship the mod's file changes.
pub fn ship_origin(project: &Project, id: &str) -> &'static str {
    let rel = ship_rel(id);
    let retail = project.doc(&rel).is_some_and(|d| !d.base.is_null());
    let mod_file = project.dir.join(&rel).is_file() || project.doc(&rel).is_some_and(|d| d.dirty);
    match (retail, mod_file) {
        (false, _) => "from the mod project",
        (true, false) => "retail (from the import)",
        (true, true) => "retail, changed by the mod",
    }
}

/// Fills the 3D view with the project's ship `id` and links the two.
pub fn show_in_3d(project: &Project, id: &str, view: &mut ShipView, link: &mut Linked) -> Result<(), String> {
    let loaded = loaded_from_project(project, id)?;
    view.status = format!("{id} {} ({})", ship_origin(project, id), crate::ship_view::layers_summary(loaded.facts.as_ref()));
    view.set(loaded);
    *link = Linked { rel: Some(ship_rel(id)), generation: view.generation };
    Ok(())
}

/// Fills the 3D view with ship `id`: from the open project (the mod's ship, or the import's as the mod sees it; the view is linked
/// to the document) or, with no project, from the import's `ships/<id>` folder. Opening a ship for viewing changes nothing.
pub fn load_by_id(project: Option<&mut Project>, import: Option<&Path>, id: &str, view: &mut ShipView, link: &mut Linked) -> Result<(), String> {
    match project {
        Some(p) => {
            let rel = ship_rel(id);
            if !p.dir.join(&rel).is_file() && !p.root.join(&rel).is_file() && !p.doc(&rel).is_some_and(|d| !d.edited.is_null()) {
                return Err(format!("no ship '{id}' in the open mod or the import"));
            }
            p.open_doc(&rel).map_err(|e| format!("ship '{id}': {e}"))?;
            show_in_3d(p, id, view, link)
        }
        None => {
            let dir = import.map(|i| i.join("ships").join(id)).filter(|d| d.is_dir()).ok_or_else(|| format!("no ship '{id}' in the import (open a mod to look in it too)"))?;
            let loaded = crate::ship_view::load(&dir)?;
            view.status = format!("{id} from the import ({})", crate::ship_view::layers_summary(loaded.facts.as_ref()));
            view.set(loaded);
            *link = Linked::default();
            Ok(())
        }
    }
}

/// Moves what the 3D view did into the undo stack (one step per user action, labelled "Move <point>") and makes the view's ship
/// equal the document again, so undo and redo show in the view. Does nothing when the view is not linked to a project ship.
/// Returns a message when an edit failed (the view is then reset to the document).
pub fn sync_view(project: &mut Project, view: &mut ShipView, link: &Linked) -> Option<String> {
    let rel = link.rel.as_deref()?;
    if link.generation != view.generation || ship_id_of(rel) != Some(view.ship_id.as_str()) {
        return None;
    }
    let mut error = None;
    let label = format!("Move {}", view.selected.as_ref().map_or("point", |(_, l)| l.as_str()));
    for step in std::mem::take(&mut view.pending) {
        if let Err(e) = project.do_step(rel, &label, step) {
            error = Some(e);
            break;
        }
    }
    if let Some(doc) = project.doc(rel)
        && doc.edited != view.ship
    {
        view.ship = doc.edited.clone();
        // Undo or redo changed the ship under the view: what the status says happened (a move, an add) no longer holds.
        view.status = "Ship changed by undo or redo".into();
    }
    error
}

/// The Bevy system running [`sync_view`] every frame.
pub fn sync_system(mut ps: ResMut<project_panel::State>, mut view: ResMut<ShipView>, link: bevy::prelude::Res<Linked>) {
    if link.rel.is_none() {
        return;
    }
    let Some(project) = &mut ps.project else { return };
    if let Some(e) = sync_view(project, &mut view, &link) {
        view.status = e;
    }
}

// ---------------------------------------------------------------- the tab

/// The tab's own state.
#[derive(Resource, Default)]
pub struct State {
    pub selected: Option<String>,
    filter: String,
    new_id: String,
    new_parent: String,
    status: String,
    compare_with: String,
    focus: Option<String>,
    schema: Option<Value>,
}

impl State {
    /// Forgets the selected ship (another mod was opened: its form is not the new mod's).
    pub fn forget(&mut self) {
        self.selected = None;
        self.focus = None;
        self.status.clear();
        self.compare_with.clear();
    }

    /// A click in the problems list: selects the ship and focuses the field. Returns whether the file is a ship's.
    pub fn take_focus(&mut self, focus: &Focus) -> bool {
        let Some(id) = ship_id_of(&focus.rel) else { return false };
        self.selected = Some(id.to_string());
        self.focus = Some(dotted_to_pointer(&focus.path));
        true
    }
}

/// The height left for a list drawn at the cursor of `ui`.
pub fn remaining_height(ui: &egui::Ui) -> f32 {
    (ui.clip_rect().bottom() - ui.cursor().top()).max(0.0)
}

/// Draws the Ships tab. Returns `true` when "Show in 3D" was pressed and succeeded (the caller switches to the 3D tab).
pub fn tab(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State, focus: &mut Focus, view: &mut ShipView, link: &mut Linked, import: Option<&Path>) -> bool {
    ui.heading("Ships");
    if ship_id_of(&focus.rel).is_some() && st.take_focus(focus) {
        *focus = Focus::default();
    }
    let schema = st.schema.get_or_insert_with(|| form_schema(project_panel::schema_named("ship.schema.json"))).clone();
    let (root, entry, unsaved): (Option<PathBuf>, Option<ModEntry>, Vec<String>) = match &ps.project {
        Some(p) => (Some(p.root.clone()), p.entry().cloned(), p.dirty_rels()),
        None => (import.map(Path::to_path_buf), None, vec![]),
    };
    let Some(root) = root else {
        ui.label("No imported assets found: import your own copy of the game with the engine first.");
        return false;
    };
    let mut listing = list_ship_ids(&root);
    if let Some(p) = &ps.project {
        listing.extend(list_ship_ids(&p.dir));
        listing.sort();
        listing.dedup();
    }
    // The list on the left and the form on the right, each with its own scroll area at the height left in the window, so the form
    // starts at the top and uses the width.
    let height = (remaining_height(ui) - 8.0).max(300.0);
    let list_width = 260.0f32.min(ui.available_width() * 0.4);
    let mut go3d = false;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(list_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
            list_column(ui, st, ps, &listing, entry.as_ref(), &unsaved, height);
        });
        ui.separator();
        let width = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
            go3d = detail_column(ui, st, ps, &schema, &root, view, link, height);
        });
    });
    go3d
}

/// The left column of the tab: filter, the ship list, "New ship..." and the status line.
fn list_column(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State, listing: &[String], entry: Option<&ModEntry>, unsaved: &[String], height: f32) {
    if ps.project.is_none() {
        ui.weak("No mod open: your first change asks which mod to save it in (the game's own files are never changed).");
    }
    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.text_edit_singleline(&mut st.filter);
    });
    let rows = ship_rows(listing, entry, unsaved, &st.filter);
    egui::ScrollArea::vertical().id_salt("ship-list").max_height((height - 120.0).max(120.0)).auto_shrink([false, false]).show(ui, |ui| {
        for r in &rows {
            let text = format!("{}  [{}{}]", r.id, r.status.label(), if r.unsaved { ", unsaved" } else { "" });
            if ui.selectable_label(st.selected.as_deref() == Some(r.id.as_str()), text).clicked() {
                st.selected = Some(r.id.clone());
                st.focus = None;
            }
        }
    });
    if ps.project.is_some() {
        new_ship_ui(ui, st, ps, listing);
    }
    if !st.status.is_empty() {
        ui.label(&st.status);
    }
}

/// The right column: the selected ship's form in its own scroll area. Returns `true` when "Show in 3D" was pressed and succeeded.
#[allow(clippy::too_many_arguments)]
fn detail_column(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State, schema: &Value, root: &Path, view: &mut ShipView, link: &mut Linked, height: f32) -> bool {
    let Some(id) = st.selected.clone() else {
        ui.label("Select a ship.");
        return false;
    };
    let rel = ship_rel(&id);
    let scroll = |salt: &str| egui::ScrollArea::vertical().id_salt(salt.to_string()).max_height(height).auto_shrink([false, false]);
    let Some(project) = &mut ps.project else {
        match read_merged(root, &rel) {
            Ok(v) => {
                let guns = gun_names(&read_merged(root, GUNS_REL).unwrap_or(Value::Null));
                scroll("ship-detail-retail").show(ui, |ui| {
                    ui.strong(&id);
                    // no mod: the retail file is both the base and what is edited, so every field reads "inherited"
                    let choices = [crate::form::Choice { pattern: GUN_FIELD, options: &guns }];
                    let ctx = crate::form::FormCtx { base: Some(&v), findings: &[], focus: None, hidden: &[], choices: &choices };
                    let edits = crate::form::show(ui, schema, &v, &ctx, egui::Id::new(("ship form retail", &id)));
                    if !edits.is_empty() {
                        crate::which_mod::propose(ps, crate::which_mod::Pending { rel: rel.clone(), label: format!("Edit {id}"), edits });
                    }
                });
            }
            Err(e) => {
                ui.colored_label(egui::Color32::LIGHT_RED, e.to_string());
            }
        }
        return false;
    };
    let notes = project_panel::notes_for(&ps.findings, &rel);
    let i = match project.open_doc(&rel) {
        Ok(i) => i,
        Err(e) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
            return false;
        }
    };
    let mut go3d = false;
    // the guns a mount can name: the mod's guns table as edited (the import's guns and the mod's own)
    let guns = project.open_doc(GUNS_REL).map(|g| gun_names(&project.docs[g].edited)).unwrap_or_default();
    let ids: Vec<String> = list_ship_ids(&project.root).into_iter().chain(list_ship_ids(&project.dir)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let mut edits = vec![];
    let mut typing = false;
    let mut revert = false;
    scroll("ship-detail").show(ui, |ui| {
        let doc = &project.docs[i];
        ui.strong(format!("{id}{}", if doc.dirty { " (unsaved)" } else { "" }));
        ui.label(doc_note(doc, &id));
        if ui.button("Show in 3D").clicked() {
            go3d = true;
        }
        if doc.can_revert()
            && ui
                .button(if doc.is_retail_layer() { "Revert file to retail" } else { "Revert file to its parent" })
                .on_hover_text("Take every change of the mod in this ship back (one undo step); saving deletes the mod's file when nothing is left")
                .clicked()
        {
            revert = true;
        }
        egui::CollapsingHeader::new("Compare").default_open(false).show(ui, |ui| compare_ui(ui, st, project, doc, &id, &ids));
        let pointer = st.focus.take();
        let inherited = (!doc.base.is_null()).then_some(&doc.base);
        let choices = [crate::form::Choice { pattern: GUN_FIELD, options: &guns }];
        let ctx = crate::form::FormCtx { base: inherited, findings: &notes, focus: pointer.as_deref(), hidden: &[], choices: &choices };
        (edits, typing) = crate::form::show_typed(ui, schema, &doc.edited, &ctx, egui::Id::new(("ship form", &id)));
    });
    if !edits.is_empty()
        && let Err(e) = project.apply_form(&rel, &format!("Edit {id}"), edits, typing)
    {
        st.status = e;
    }
    if revert && let Err(e) = project.revert_file(&rel) {
        st.status = e;
    }
    if go3d {
        match show_in_3d(project, &id, view, link) {
            Ok(()) => return true,
            Err(e) => st.status = e,
        }
    }
    false
}

fn compare_ui(ui: &mut egui::Ui, st: &mut State, project: &Project, doc: &Doc, id: &str, ids: &[String]) {
    ui.horizontal(|ui| {
        ui.label("Against");
        let shown = if st.compare_with.is_empty() { "its base (retail or parent)".to_string() } else { st.compare_with.clone() };
        egui::ComboBox::from_id_salt("compare-with").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut st.compare_with, String::new(), "its base (retail or parent)");
            for other in ids.iter().filter(|o| *o != id) {
                ui.selectable_value(&mut st.compare_with, other.clone(), other);
            }
        });
    });
    let other = reference_ship(project, doc, &st.compare_with);
    match other {
        Some(o) => {
            compare::table(ui, &compare::compare(&doc.edited, &o), &compare::gun_lists(&doc.edited, &o), id, if st.compare_with.is_empty() { "base" } else { &st.compare_with })
        }
        None => {
            ui.label("Nothing to compare with: pick a ship.");
        }
    }
}

/// The ship to compare against: the one the user picked (as the project sees it), else the document's base (the parent of an
/// `extends` file, or the import's own file for a retail ship), else the import's file of the same id.
pub fn reference_ship(project: &Project, doc: &Doc, picked: &str) -> Option<Value> {
    if !picked.is_empty() {
        return read_merged(&project.root, &ship_rel(picked)).ok();
    }
    if !doc.base.is_null() {
        return Some(doc.base.clone());
    }
    let id = ship_id_of(&doc.rel)?;
    project.import.as_deref().and_then(|i| read_merged(i, &ship_rel(id)).ok())
}

fn new_ship_ui(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State, listing: &[String]) {
    egui::CollapsingHeader::new("New ship...").default_open(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Id");
            ui.text_edit_singleline(&mut st.new_id);
        });
        ui.horizontal(|ui| {
            ui.label("Extends");
            let shown = if st.new_parent.is_empty() { "(pick a ship)".to_string() } else { st.new_parent.clone() };
            egui::ComboBox::from_id_salt("new-ship-parent").selected_text(shown).show_ui(ui, |ui| {
                for id in listing {
                    ui.selectable_value(&mut st.new_parent, id.clone(), id);
                }
            });
            if ui.add_enabled(!st.new_parent.is_empty(), egui::Button::new("New ship from this one")).clicked()
                && let Some(p) = &mut ps.project
            {
                let id = st.new_id.trim().to_string();
                match new_ship_extending(p, &id, &st.new_parent) {
                    Ok(()) => {
                        st.selected = Some(id);
                        st.status = "Started; nothing is written until you change something and save.".into();
                    }
                    Err(e) => st.status = e,
                }
            }
        });
        if ui.button("New ship from a GLB of your own...").on_hover_text("Copies your model into the mod; a model identical to one of the game's is refused").clicked()
            && let Some(file) = rfd::FileDialog::new().add_filter("glTF binary", &["glb"]).pick_file()
            && let Some(p) = &mut ps.project
        {
            let id = st.new_id.trim().to_string();
            match new_ship_from_glb(p, &id, &file) {
                Ok(()) => {
                    st.selected = Some(id);
                    st.status = "Copied the model into the mod; set the ship's values and save.".into();
                }
                Err(e) => st.status = e,
            }
        }
    });
}

/// Prefixes the pointer of `edit`, relative to a ship's part, with `/<escaped key>` (shared with the weapons tab's entries).
pub fn prefixed(key: &str, edit: &crate::edit::Edit) -> crate::edit::Edit {
    crate::edit::Edit { pointer: format!("/{}{}", escape(key), edit.pointer), old: edit.old.clone(), new: edit.new.clone() }
}

/// The ship schema as the form shows it: without `extends` and `set`, the merge keywords. The form edits the merged ship (what the
/// game sees); the project writes `extends` and `set` itself when it saves the diff, so offering them as fields would only confuse.
pub fn form_schema(mut schema: Value) -> Value {
    if let Some(props) = schema["properties"].as_object_mut() {
        props.shift_remove("extends");
        props.shift_remove("set");
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Edit;
    use crate::ship_view::test_glb;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-ships-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// A synthetic import with two ships, one with a model, and an empty mod folder.
    fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = tmp(name);
        let (assets, m) = (root.join("assets"), root.join("mod"));
        write(&assets, "ships/Ship_A/ship.json", r#"{"points": 10, "physics": {"MaxSpeed": 8}, "sight": 400, "guns": {"M1": {"gun": "G1", "arc_deg": 90}}}"#);
        write(&assets, "ships/Ship_B/ship.json", r#"{"points": 5}"#);
        std::fs::create_dir_all(assets.join("ships/Ship_A")).unwrap();
        std::fs::write(assets.join("ships/Ship_A/model.glb"), test_glb::synthetic()).unwrap();
        std::fs::create_dir_all(&m).unwrap();
        (root, assets, m)
    }

    #[test]
    fn gun_names_skip_the_schema_key() {
        assert_eq!(gun_names(&json!({"$schema": "x", "G1": {}, "G2": {}})), ["G1", "G2"]);
        assert!(gun_names(&Value::Null).is_empty());
    }

    #[test]
    fn rows_mark_what_the_mod_does_and_filter_and_sort() {
        let entry = ModEntry { extends: vec![ship_rel("Ship_A")], overrides: vec![ship_rel("Ship_C")], added: vec![ship_rel("Ship_N")], ..Default::default() };
        let listing: Vec<String> = ["Ship_N", "Ship_C", "Ship_B", "Ship_A"].iter().map(|s| s.to_string()).collect();
        let rows = ship_rows(&listing, Some(&entry), &[ship_rel("Ship_B"), ship_rel("Ship_Z"), "mod.json".into()], "");
        let got: Vec<(&str, ShipStatus, bool)> = rows.iter().map(|r| (r.id.as_str(), r.status, r.unsaved)).collect();
        assert_eq!(
            got,
            [
                ("Ship_A", ShipStatus::Extends, false),
                ("Ship_B", ShipStatus::Retail, true),
                ("Ship_C", ShipStatus::Overrides, false),
                ("Ship_N", ShipStatus::Added, false),
                ("Ship_Z", ShipStatus::New, true),
            ]
        );
        let f = ship_rows(&listing, None, &[], " b");
        assert_eq!(f.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["Ship_B"]);
        assert!(f.iter().all(|r| r.status == ShipStatus::Retail));
    }

    #[test]
    fn the_listing_reads_ship_folders_with_a_ship_json() {
        let (root, assets, _) = setup("list");
        std::fs::create_dir_all(assets.join("ships/Empty")).unwrap();
        assert_eq!(list_ship_ids(&assets), ["Ship_A", "Ship_B"]);
        assert!(list_ship_ids(&root.join("nowhere")).is_empty());
        assert_eq!(ship_id_of("ships/X/ship.json"), Some("X"));
        assert_eq!((ship_id_of("ships/X/model.glb"), ship_id_of("ships/a/b/ship.json")), (None, None));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn editing_a_retail_ship_saves_an_extends_file_with_only_the_change() {
        let (root, assets, m) = setup("edit");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let rel = ship_rel("Ship_A");
        p.open_doc(&rel).unwrap();
        assert!(doc_note(p.doc(&rel).unwrap(), "Ship_A").contains("retail ship Ship_A"));
        p.do_edits(&rel, "Edit Ship_A", vec![Edit::set("/points", json!(7))]).unwrap();
        let report = p.save();
        assert_eq!((report.written.as_slice(), report.errors.len()), ([rel.clone()].as_slice(), 0));
        let text = std::fs::read_to_string(m.join(&rel)).unwrap().replace([' ', '\n'], "");
        assert_eq!(text, r#"{"extends":"Ship_A","points":7}"#);
        let rows = ship_rows(&list_ship_ids(&p.root), p.entry(), &[], "");
        assert_eq!(rows[0].status, ShipStatus::Extends);
        assert_eq!(rows[1].status, ShipStatus::Retail);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_ship_extends_a_parent_and_is_written_after_a_change() {
        let (root, assets, m) = setup("extend");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        assert!(new_ship_extending(&mut p, "bad/id", "Ship_A").is_err());
        assert!(new_ship_extending(&mut p, "Ship_A", "Ship_B").is_err(), "exists");
        new_ship_extending(&mut p, "Ship_A2", "Ship_A").unwrap();
        let rel = ship_rel("Ship_A2");
        assert_eq!(doc_note(p.doc(&rel).unwrap(), "Ship_A2"), "extends Ship_A");
        assert_eq!(p.doc(&rel).unwrap().edited["points"], 10);
        let rows = ship_rows(&list_ship_ids(&p.root), p.entry(), &p.dirty_rels(), "");
        assert!(rows.iter().any(|r| r.id == "Ship_A2" && r.status == ShipStatus::New && r.unsaved));
        p.save();
        assert!(!m.join(&rel).exists());
        p.do_edits(&rel, "Edit", vec![Edit::set("/physics/MaxSpeed", json!(12))]).unwrap();
        p.save();
        let text = std::fs::read_to_string(m.join(&rel)).unwrap().replace([' ', '\n'], "");
        assert_eq!(text, r#"{"extends":"Ship_A","physics":{"MaxSpeed":12}}"#);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_glb_identical_to_a_retail_model_is_refused_and_another_is_copied() {
        let (root, assets, m) = setup("glb");
        let mut p = Project::open(&m, Some(assets.clone())).unwrap();
        let same = root.join("same.glb");
        std::fs::write(&same, test_glb::synthetic()).unwrap();
        let err = new_ship_from_glb(&mut p, "Mine", &same).unwrap_err();
        assert!(err.contains("identical") && err.contains("Ship_A"), "{err}");
        assert!(!m.join("ships/Mine").exists(), "nothing copied");
        assert_eq!(retail_glb_match(&test_glb::synthetic(), &assets).as_deref(), Some("Ship_A"));
        // The same bytes with one more padding byte in the JSON chunk's trailing space is not byte-identical: use a different node name.
        let mut other = test_glb::synthetic();
        let at = other.windows(9).position(|w| w == b"Hull_Main").unwrap();
        other[at..at + 9].copy_from_slice(b"Hull_Mine");
        let own = root.join("own.glb");
        std::fs::write(&own, &other).unwrap();
        new_ship_from_glb(&mut p, "Mine", &own).unwrap();
        assert_eq!(std::fs::read(m.join("ships/Mine/model.glb")).unwrap(), other);
        assert!(p.poll().iter().all(|n| !n.text.contains("mod folder changed")), "the editor's own copy is not announced");
        assert_eq!(p.doc(&ship_rel("Mine")).unwrap().edited, json!({"race": "", "points": 0, "skirmish": false}));
        p.save();
        let text = std::fs::read_to_string(m.join("ships/Mine/ship.json")).unwrap().replace([' ', '\n'], "");
        assert_eq!(text, r#"{"race":"","points":0,"skirmish":false}"#);
        let bad = root.join("bad.glb");
        std::fs::write(&bad, b"nope").unwrap();
        assert!(new_ship_from_glb(&mut p, "Bad", &bad).is_err());
        assert!(new_ship_from_glb(&mut p, "Mine", &own).is_err(), "id taken");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_ship_id_in_the_load_box_is_looked_up_in_the_mod_and_the_import() {
        use crate::ship_view::looks_like_id;
        assert!(looks_like_id("Ship_A") && looks_like_id("  Ship_A ") && !looks_like_id("") && !looks_like_id("a/b") && !looks_like_id("x.glb") && !looks_like_id("C:\\x"));
        let (root, assets, m) = setup("byid");
        let mut p = Project::open(&m, Some(assets.clone())).unwrap();
        let (mut view, mut link) = (ShipView::default(), Linked::default());
        load_by_id(Some(&mut p), Some(&assets), "Ship_A", &mut view, &mut link).unwrap();
        assert_eq!((view.ship_id.as_str(), link.rel.as_deref()), ("Ship_A", Some(ship_rel("Ship_A").as_str())));
        assert!(p.dirty_rels().is_empty(), "looking at a ship changes nothing");
        let e = load_by_id(Some(&mut p), Some(&assets), "Ship_Nope", &mut view, &mut link).unwrap_err();
        assert!(e.contains("no ship 'Ship_Nope'"), "{e}");
        // with no project open the import's folder is read
        let (mut view, mut link) = (ShipView::default(), Linked::default());
        load_by_id(None, Some(&assets), "Ship_A", &mut view, &mut link).unwrap();
        assert_eq!(view.ship_id, "Ship_A");
        assert!(load_by_id(None, Some(&assets), "Ship_Nope", &mut view, &mut link).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_view_is_filled_from_the_project_and_its_edits_go_through_the_undo_stack() {
        let (root, assets, m) = setup("view");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let rel = ship_rel("Ship_A");
        p.open_doc(&rel).unwrap();
        let (mut view, mut link) = (ShipView::default(), Linked::default());
        show_in_3d(&p, "Ship_A", &mut view, &mut link).unwrap();
        assert!(view.status.starts_with("Ship_A retail (from the import)"), "a ship loaded from the import says so: {}", view.status);
        assert_eq!((view.ship_id.as_str(), view.facts.is_some(), view.ship["points"].as_i64()), ("Ship_A", true, Some(10)));
        // The view moves the homing point: one pending step.
        view.commit(vec![Edit::set("/homing", json!([1, 2, 3]))]).unwrap();
        view.selected = Some((crate::ship_view::points::Kind::Homing, "homing".into()));
        assert_eq!(sync_view(&mut p, &mut view, &link), None);
        assert!(view.pending.is_empty());
        view.status = "Moved Homing homing".into();
        assert_eq!(sync_view(&mut p, &mut view, &link), None);
        assert_eq!(view.status, "Moved Homing homing", "nothing changed under the view: the status stays");
        assert_eq!(p.doc(&rel).unwrap().edited["homing"], json!([1, 2, 3]));
        assert_eq!(p.undo_label(), Some("Move homing"));
        // Undo: the view follows.
        p.undo();
        assert_eq!(sync_view(&mut p, &mut view, &link), None);
        assert!(view.ship.get("homing").is_none() && !p.doc(&rel).unwrap().dirty);
        assert!(!view.status.contains("Moved"), "the status no longer claims the undone move: {}", view.status);
        p.redo();
        sync_view(&mut p, &mut view, &link);
        assert_eq!(view.ship["homing"], json!([1, 2, 3]));
        // Loading anything else into the view unlinks it.
        view.set(Loaded { ship_id: "Ship_A".into(), ship: json!({}), glb: None, facts: None });
        view.commit(vec![Edit::set("/homing", json!([9, 9, 9]))]).unwrap();
        sync_view(&mut p, &mut view, &link);
        assert_eq!(p.doc(&rel).unwrap().edited["homing"], json!([1, 2, 3]), "an unlinked view is left alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_reference_is_the_base_a_picked_ship_or_the_import() {
        let (root, assets, m) = setup("ref");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        p.open_doc(&ship_rel("Ship_A")).unwrap();
        p.do_edits(&ship_rel("Ship_A"), "Edit", vec![Edit::set("/points", json!(12)), Edit::set("/guns/M2", json!({"gun": "G2", "arc_deg": 30}))]).unwrap();
        let doc = p.doc(&ship_rel("Ship_A")).unwrap().clone();
        let base = reference_ship(&p, &doc, "").unwrap();
        let rows = compare::compare(&doc.edited, &base);
        let find = |l: &str| rows.iter().find(|r| r.label == l).unwrap().diff.clone();
        assert_eq!((find("Points").as_str(), find("Gun mounts").as_str(), find("MaxSpeed").as_str()), ("+2", "+1", "same"));
        assert_eq!(reference_ship(&p, &doc, "Ship_B").unwrap()["points"], 5);
        assert!(reference_ship(&p, &doc, "Nope").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn focus_from_the_problems_list_selects_the_ship_and_the_field() {
        let mut st = State::default();
        assert!(!st.take_focus(&Focus { rel: "data/weapons/guns.json".into(), path: "G.range".into() }));
        assert!(st.take_focus(&Focus { rel: ship_rel("Ship_A"), path: "guns.M1.arc_deg".into() }));
        assert_eq!((st.selected.as_deref(), st.focus.as_deref()), (Some("Ship_A"), Some("/guns/M1/arc_deg")));
    }

    #[test]
    fn the_form_schema_has_no_merge_keywords() {
        let full = project_panel::schema_named("ship.schema.json");
        assert!(full["properties"].get("extends").is_some(), "the engine's schema has them");
        let f = form_schema(full);
        assert!(f["properties"].get("extends").is_none() && f["properties"].get("set").is_none());
        assert!(f["properties"].get("points").is_some());
        // a file that has them is not flagged as having unknown keys by the form (the engine's own schema knows them)
        assert!(openprocyon::authoring::unknown_keys(&project_panel::schema_named("ship.schema.json"), &serde_json::json!({"extends": "X"})).is_empty());
    }
}
