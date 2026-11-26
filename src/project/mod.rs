//! The mod project: a mod folder opened for editing. It holds the folder, the player's import, the overlay of the two (rebuilt after a
//! save or a reload), documents opened on demand, an undo stack over JSON edits, saving as a diff against what each file is laid over,
//! and a watch on the files so a script or hand edit is noticed (docs/roadmap.md principle 4 and "Code-first authoring").
//! No window code here: everything is testable headless.

pub mod diff;
pub mod guard;
pub mod keyed;
pub mod layers;
pub mod licence;
pub mod problems;
pub mod stamp;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use openprocyon::authoring::{ENGINE_VERSION, ModEntry, ModReport, read_merged, to_json_string};
use serde_json::{Value, json};

use crate::edit::{self, Edit};
use stamp::FileStamp;

/// How a mod's file combines with the file below it, which decides how it is saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    /// Laid over a parent (`ships/<id>/*.json`, `maps/<id>/*.json`, or any file with `extends`): saved as `extends` plus changed keys.
    Extends,
    /// Keys merged into the file below (the engine's merged paths): saved as the changed keys.
    Merged,
    /// Replaces the file below whole: saved whole.
    Replaced,
}

/// PROVISIONAL: mirrors the engine's merged-path list (`docs/modding.md` "What a mod can change"). Engine requirement:
/// `authoring::merge_rule(rel)` (and the merged-path list) public, so tools do not restate it.
const MERGED_PATHS: &[&str] = &[
    "hud/scales.json",
    "data/collision.json",
    "data/docks.json",
    "data/crew_nodes.json",
    "data/weapons/guns.json",
    "data/weapons/bullets.json",
    "data/ai/weapons.json",
    "menu/strings.json",
    "data/render/skies.json",
    "data/render/nebulae.json",
];

/// PROVISIONAL: the engine's path rule (see [`MERGED_PATHS`]): a merged path is `Merged`; a `ships/<id>/x.json` or `maps/<id>/x.json`
/// file, or any other `.json` that already has `extends` in `own`, is `Extends`; everything else `Replaced`.
pub fn merge_kind(rel: &str, own: &Value) -> DocKind {
    if MERGED_PATHS.contains(&rel) {
        DocKind::Merged
    } else if rel.ends_with(".json") && (nested_id(rel).is_some() || own.get("extends").is_some()) {
        DocKind::Extends
    } else {
        DocKind::Replaced
    }
}

/// The folder id of `ships/<id>/<file>` and `maps/<id>/<file>`.
fn nested_id(rel: &str) -> Option<&str> {
    let parts: Vec<&str> = rel.split('/').collect();
    (parts.len() == 3 && matches!(parts[0], "ships" | "maps")).then(|| parts[1])
}

/// The id a file at `rel` is known by when something extends it: its folder (`ships/<id>/ship.json`) or its file stem.
fn own_id(rel: &str) -> String {
    nested_id(rel).map(str::to_string).unwrap_or_else(|| Path::new(rel).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

/// The file `parent` names for an `extends` in the file `rel` (the engine's rule: the same file of another ship or map, else a sibling).
fn parent_rel(rel: &str, parent: &str) -> String {
    match nested_id(rel) {
        Some(_) => {
            let parts: Vec<&str> = rel.split('/').collect();
            format!("{}/{parent}/{}", parts[0], parts[2])
        }
        None => match rel.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{parent}.json"),
            None => format!("{parent}.json"),
        },
    }
}

/// One document of the mod, opened by relative path (`ships/<Id>/ship.json`, `data/weapons/guns.json`, `mod.json`).
#[derive(Clone, Debug)]
pub struct Doc {
    pub rel: String,
    pub kind: DocKind,
    /// What the game sees below the mod's file (the parent for an `extends` file, the import's file at the same path otherwise);
    /// `Null` when there is none.
    pub base: Value,
    /// The mod's file as it is on disk (`Null` when the mod has none).
    pub own: Value,
    /// What the game will see: the engine's merge of `base` and `own`, plus the edits made here (`mod.json`: the file itself).
    pub edited: Value,
    /// There are edits not saved.
    pub dirty: bool,
    /// The mod's file as last read or written by the editor; `None`: the file did not exist.
    pub disk: Option<FileStamp>,
    /// The id an `extends` written by the editor names when `own` has none; `None`: the file inherits nothing.
    pub parent: Option<String>,
    /// `edited` as of the last load or save; `dirty` is `edited != clean`.
    clean: Value,
    /// Set by [`Project::poll`] when the file changed on disk while the document had edits; holds the stamp that was announced, so
    /// the notice is not repeated every second.
    pub conflict: Option<Option<FileStamp>>,
}

/// What saving a document does to its file.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Nothing: the file is equal in meaning to what is to be saved.
    Nothing,
    /// Write this content.
    Write(Value),
    /// Delete the mod's file: nothing of the mod's change is left in it.
    Delete,
}

impl Doc {
    /// The file this document is saved as (see [`diff`]), and the changes a mod file cannot express. A file that holds nothing but what
    /// is below it is deleted: a retail ship's `extends`-itself file with no key left, a merged table with no entry left.
    pub fn plan(&self) -> (Plan, Vec<String>) {
        let d = diff::compute_with(&keyed::EngineRule, &self.rel, self.kind, &self.base, &self.edited, &self.own, self.parent.as_deref());
        let in_place = self.parent.as_deref() == Some(own_id(&self.rel).as_str());
        let empty = |f: &Value, keep: &[&str]| f.as_object().is_some_and(|m| m.keys().all(|k| keep.contains(&k.as_str())));
        let plan = match d.file {
            Some(f) if !self.own.is_null() && ((self.kind == DocKind::Merged && empty(&f, &[])) || (self.kind == DocKind::Extends && in_place && empty(&f, &["extends"]))) => {
                Plan::Delete
            }
            Some(f) if f != self.own => Plan::Write(f),
            _ => Plan::Nothing,
        };
        (plan, d.warnings)
    }

    /// Sets `dirty`: there is something to save. A document that exists nowhere yet stays dirty while it differs from its start; any
    /// other is dirty only when saving would change the file (so undoing back, or resetting every field by hand, is clean again).
    fn refresh_dirty(&mut self) {
        self.dirty = if self.edited == self.clean {
            false
        } else if self.clean.is_null() {
            true
        } else {
            let (plan, warnings) = self.plan();
            plan != Plan::Nothing || !warnings.is_empty()
        };
    }

    /// Whether the document can go back to what is below it ("Revert file"): a mod file that changes a retail file (`Merged`, or an
    /// `Extends` of the retail file of the same id), or one extending another file, with something to take back. A file with nothing
    /// below it (a ship new in the mod) has nothing to revert to.
    pub fn can_revert(&self) -> bool {
        !self.base.is_null() && self.kind != DocKind::Replaced && self.edited != self.base && (!self.own.is_null() || self.dirty)
    }

    /// Whether the retail file of the same id is what is below this document (a retail ship edited as `extends` itself).
    pub fn is_retail_layer(&self) -> bool {
        self.kind == DocKind::Merged || self.parent.as_deref() == Some(own_id(&self.rel).as_str())
    }
}

/// A change the user should hear about (poll results, save problems).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub rel: String,
    pub text: String,
    /// The document has unsaved edits and the file changed on disk: the user picks "Reload from disk" or "Keep my edits".
    pub conflict: bool,
}

/// One undoable step: the edits as applied (`old` filled in) to one document.
#[derive(Clone, Debug)]
pub struct Command {
    pub rel: String,
    pub label: String,
    pub edits: Vec<Edit>,
    /// The document before the step; undoing restores it exactly (an applied edit leaves created parents behind).
    before: Value,
    /// The step came from typing or dragging a field, so the next such edit of the same field may join it. A button action (Reset,
    /// Revert, add, remove) never does, and a save ends it ([`Project::save`]).
    open: bool,
    /// T8: steps of one group (see [`Project::do_group`]) share a number and are undone and redone together; 0 is no group.
    group: u64,
}

/// What a save did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Files written (relative paths).
    pub written: Vec<String>,
    /// Files deleted because nothing of the mod's change was left in them (a file reverted to retail).
    pub deleted: Vec<String>,
    /// Changes a mod file cannot express (a removed inherited key), with the file named.
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// What a new mod is made with.
#[derive(Clone, Debug, Default)]
pub struct NewMod {
    /// The mod's `id` and `name`; `None`: the folder's name.
    pub id: Option<String>,
    pub licence: licence::Licence,
    /// The holder named in the `LICENSE` notice; empty: the mod's name.
    pub author: String,
    /// Folders to make, relative to the mod (the parents of the file the first change writes).
    pub folders: Vec<String>,
}

const MAX_UNDO: usize = 500;
static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

/// An open mod folder.
pub struct Project {
    /// The mod folder (absolute).
    pub dir: PathBuf,
    /// The player's imported assets, when there are any.
    pub import: Option<PathBuf>,
    /// Where the game's view is read from: the overlay of the mod over the import.
    pub root: PathBuf,
    /// What the engine says the mod does, from the last overlay build.
    pub report: ModReport,
    pub docs: Vec<Doc>,
    undo: Vec<Command>,
    redo: Vec<Command>,
    notices: Vec<Notice>,
    scratch: String,
    /// The scratch copy the last mod check read ([`Project::check_dir`]), removed when the next one is made and on drop.
    check_copy: Option<PathBuf>,
    /// The files in the mod folder at the last overlay build ([`layers::listing`]); a difference in [`Project::poll`] rebuilds it.
    files: std::collections::BTreeSet<String>,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = guard::remove_dir_all(&crate::overlay::scratch(&self.scratch), self.import.as_deref());
        let _ = guard::remove_dir_all(&crate::overlay::scratch(&format!("{}-empty", self.scratch)), self.import.as_deref());
        if let Some(c) = &self.check_copy {
            let _ = guard::remove_dir_all(c, self.import.as_deref());
        }
        // The per-process folder itself is NOT removed here: another project or scratch build of this process may be creating a
        // folder inside it at the same moment (create_dir_all makes the parent, then the child), and removing the empty parent in
        // between makes that fail with NotFound. `overlay::remove_own` removes it when the editor quits, and `overlay::remove_stale`
        // removes a dead process's folder at the next start.
    }
}

/// `rel` as a safe relative path inside the mod: no `..`, no root, no backslash.
fn check_rel(rel: &str) -> Result<(), String> {
    let bad = rel.is_empty() || rel.contains('\\') || Path::new(rel).components().any(|c| !matches!(c, std::path::Component::Normal(_)));
    if bad { Err(format!("{rel}: not a path inside the mod folder")) } else { Ok(()) }
}

impl Project {
    /// Opens the mod folder `dir` (it must exist) over the import `import` and builds the overlay. Nothing is written.
    pub fn open(dir: &Path, import: Option<PathBuf>) -> Result<Project, String> {
        let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        let import = import.filter(|d| d.is_dir()).map(|d| std::path::absolute(&d).unwrap_or(d));
        guard::check_project_dir(&dir, import.as_deref())?;
        let scratch = format!("project-{}", NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed));
        let mut p = Project {
            root: dir.clone(),
            dir,
            import,
            report: ModReport::default(),
            docs: vec![],
            undo: vec![],
            redo: vec![],
            notices: vec![],
            scratch,
            check_copy: None,
            files: Default::default(),
        };
        p.rebuild()?;
        if p.dir.join("mod.json").is_file()
            && let Err(e) = p.open_doc("mod.json")
        {
            p.notices.push(Notice { rel: "mod.json".into(), text: e, conflict: false });
        }
        Ok(p)
    }

    /// Makes a mod of the folder `dir` with the defaults ([`NewMod::default`]); see [`Project::create_with`].
    pub fn create(dir: &Path, import: Option<PathBuf>) -> Result<(Project, bool), String> {
        Project::create_with(dir, import, &NewMod::default())
    }

    /// Makes a complete mod of the folder `dir`: `mod.json` (id = `new.id` or the folder name, format 1, this engine's version), a
    /// `LICENSE` notice (see [`licence::Licence`]) and the folders `new.folders` (relative to the mod, what the first change needs),
    /// then opens it. A `mod.json` or `LICENSE` that exists is never overwritten. Copies no retail file. The flag says whether
    /// `mod.json` was written.
    pub fn create_with(dir: &Path, import: Option<PathBuf>, new: &NewMod) -> Result<(Project, bool), String> {
        let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let imp = import.as_deref();
        guard::check_project_dir(&dir, imp)?;
        for rel in &new.folders {
            check_rel(rel)?;
        }
        let path = dir.join("mod.json");
        let created = !path.exists();
        let folder_name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "my-mod".into());
        let id = new.id.as_deref().map(str::trim).filter(|i| !i.is_empty()).map(str::to_string).unwrap_or(folder_name);
        guard::create_dir_all(&dir, imp)?;
        if created {
            let meta = json!({"id": id, "name": id, "version": "0.1.0", "description": "", "category": "Other", "format": 1, "game_version": ENGINE_VERSION});
            guard::write(&path, to_json_string(&meta), imp)?;
        }
        let licence = dir.join("LICENSE");
        if !licence.exists() {
            guard::write(&licence, new.licence.notice(&id, &new.author), imp)?;
        }
        for rel in &new.folders {
            guard::create_dir_all(&dir.join(rel), imp)?;
        }
        Ok((Project::open(&dir, import)?, created))
    }

    /// Rebuilds the overlay (after a save or when a file changed on disk) and the engine's report.
    pub fn rebuild(&mut self) -> Result<(), String> {
        let base = match &self.import {
            Some(b) => b.clone(),
            None => {
                let empty = crate::overlay::scratch(&format!("{}-empty", self.scratch));
                guard::create_dir_all(&empty, None)?;
                empty
            }
        };
        let (root, report) = crate::overlay::build(&base, std::slice::from_ref(&self.dir), &crate::overlay::scratch(&self.scratch))?;
        (self.root, self.report) = (root, report);
        self.files = layers::listing(&self.dir);
        Ok(())
    }

    /// The mod's name from `mod.json` when it has one, else the folder name.
    pub fn name(&self) -> String {
        let meta = self.doc("mod.json").map(|d| &d.edited);
        let named = |k: &str| meta.and_then(|m| m.get(k)).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        named("name").or_else(|| named("id")).unwrap_or_else(|| self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    }

    /// The engine's report entry for this mod.
    pub fn entry(&self) -> Option<&ModEntry> {
        self.report.mods.iter().find(|m| m.dir == self.dir).or(self.report.mods.last())
    }

    /// The folder the mod check reads: the mod folder itself when nothing is unsaved, else a scratch copy of it (files hard-linked or
    /// copied) with every unsaved document written as [`Project::save`] would write it, so the engine checks the unsaved edits too.
    /// Writes only into the editor's scratch folder; the mod folder is untouched.
    pub fn check_dir(&mut self) -> Result<PathBuf, String> {
        let dirty = self.dirty_rels();
        if dirty.is_empty() {
            return Ok(self.dir.clone());
        }
        let import = self.import.clone();
        let out = crate::overlay::scratch(&format!("{}-check-{}", self.scratch, NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)));
        if let Some(old) = self.check_copy.replace(out.clone()) {
            let _ = guard::remove_dir_all(&old, import.as_deref());
        }
        guard::create_dir_all(&out, import.as_deref())?;
        for rel in layers::listing(&self.dir) {
            if dirty.contains(&rel) {
                continue;
            }
            let to = out.join(&rel);
            if let Some(parent) = to.parent() {
                guard::create_dir_all(parent, import.as_deref())?;
            }
            guard::check_writable(&to, import.as_deref())?;
            if std::fs::hard_link(self.dir.join(&rel), &to).is_err() {
                guard::copy(&self.dir.join(&rel), &to, import.as_deref())?;
            }
        }
        for rel in &dirty {
            let Some(doc) = self.doc(rel) else { continue };
            let to = out.join(rel);
            match doc.plan().0 {
                Plan::Write(f) => {
                    guard::write(&to, to_json_string(&f), import.as_deref())?;
                }
                // saving would delete the file: the check sees it gone
                Plan::Delete => {}
                // saving would write nothing: the file on disk already means the same
                Plan::Nothing => {
                    if self.dir.join(rel).is_file() {
                        guard::copy(&self.dir.join(rel), &to, import.as_deref())?;
                    }
                }
            }
        }
        Ok(out)
    }

    /// The layer rows for the panel: the report plus the documents with unsaved edits.
    pub fn layer_rows(&self) -> Vec<layers::LayerRow> {
        layers::rows(self.entry(), &self.dirty_rels())
    }

    /// The open document `rel`.
    pub fn doc(&self, rel: &str) -> Option<&Doc> {
        self.docs.iter().find(|d| d.rel == rel)
    }

    /// Relative paths of the documents with unsaved edits.
    pub fn dirty_rels(&self) -> Vec<String> {
        self.docs.iter().filter(|d| d.dirty).map(|d| d.rel.clone()).collect()
    }

    /// Reads the document `rel` from disk, the import and the overlay.
    fn load(&self, rel: &str) -> Result<Doc, String> {
        check_rel(rel)?;
        let path = self.dir.join(rel);
        let disk = FileStamp::of(&path);
        let own = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{rel}: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Value::Null,
            Err(e) => return Err(format!("{rel}: {e}")),
        };
        let kind = merge_kind(rel, &own);
        let from_import = |r: &str| self.import.as_deref().and_then(|i| read_merged(i, r).ok()).unwrap_or(Value::Null);
        let (base, parent) = match (kind, own.get("extends").and_then(Value::as_str)) {
            (DocKind::Extends, Some(p)) => {
                // The parent as the engine reads it: from the overlay so far (the import, or the mod's own other file), except a
                // file that extends itself in place, whose parent is the import's.
                let from_root = (p != own_id(rel)).then(|| read_merged(&self.root, &parent_rel(rel, p)).ok()).flatten();
                (from_root.unwrap_or_else(|| from_import(&parent_rel(rel, p))), Some(p.to_string()))
            }
            (DocKind::Extends, None) => {
                let base = from_import(rel);
                let parent = (!base.is_null()).then(|| own_id(rel));
                (base, parent)
            }
            _ => (from_import(rel), None),
        };
        let edited = if rel == "mod.json" {
            own.clone()
        } else if own.is_null() {
            base.clone()
        } else {
            // The engine's overlay merges the file (keyed lists included); when it skipped it (bad `extends`), show the file itself and
            // let the problems list say why.
            read_merged(&self.root, rel).unwrap_or_else(|_| own.clone())
        };
        Ok(Doc { rel: rel.to_string(), kind, base, own, clean: edited.clone(), edited, dirty: false, disk, parent, conflict: None })
    }

    /// Opens the document `rel` (a path relative to the mod folder) if it is not open; returns its index in `docs`.
    pub fn open_doc(&mut self, rel: &str) -> Result<usize, String> {
        if let Some(i) = self.docs.iter().position(|d| d.rel == rel) {
            return Ok(i);
        }
        let doc = self.load(rel)?;
        self.docs.push(doc);
        Ok(self.docs.len() - 1)
    }

    /// Opens a document for a file that exists nowhere yet (neither in the mod nor in the import), so a new ship can be started. With
    /// `parent`, the document is that parent's merged file (as `extends` would give) and saves as `extends` plus the changes, writing
    /// nothing while there are none; without, it starts as `initial` and is written whole. Refused when the file or an open document
    /// of that path exists already.
    pub fn open_new_doc(&mut self, rel: &str, parent: Option<&str>, initial: Value) -> Result<usize, String> {
        check_rel(rel)?;
        if self.docs.iter().any(|d| d.rel == rel) || self.dir.join(rel).exists() || self.root.join(rel).exists() {
            return Err(format!("{rel}: exists already"));
        }
        let base = match parent {
            Some(p) => read_merged(&self.root, &parent_rel(rel, p)).map_err(|e| format!("{rel}: parent '{p}': {e}"))?,
            None => Value::Null,
        };
        let edited = if parent.is_some() { base.clone() } else { initial };
        let kind = merge_kind(rel, &Value::Null);
        let parent = parent.map(str::to_string);
        self.docs.push(Doc { rel: rel.to_string(), kind, base, own: Value::Null, clean: Value::Null, edited, dirty: true, disk: None, parent, conflict: None });
        Ok(self.docs.len() - 1)
    }

    /// Applies `edits` to the document `rel` as one undoable step, all or nothing: when one fails, the document is left as it was and
    /// the error names the step. Clears the redo stack.
    /// Consecutive sets of the same one field with the same label are merged into one step (typing in a form field), unless a save or
    /// a [`Project::do_step`] came between.
    pub fn do_edits(&mut self, rel: &str, label: &str, edits: Vec<Edit>) -> Result<(), String> {
        self.push_edits(rel, label, edits, true)
    }

    /// What a form's edits are saved as: typing or dragging one box ([`Project::do_edits`], joins the typing before it), anything
    /// else (a Reset, Set, add, remove button) a step of its own ([`Project::do_step`]).
    pub fn apply_form(&mut self, rel: &str, label: &str, edits: Vec<Edit>, typing: bool) -> Result<(), String> {
        if typing { self.do_edits(rel, label, edits) } else { self.do_step(rel, label, edits) }
    }

    /// [`Project::do_edits`] as a step of its own, never merged with the one before (one drag of a point in the 3D view).
    pub fn do_step(&mut self, rel: &str, label: &str, edits: Vec<Edit>) -> Result<(), String> {
        self.push_edits(rel, label, edits, false)
    }

    fn push_edits(&mut self, rel: &str, label: &str, edits: Vec<Edit>, coalesce: bool) -> Result<(), String> {
        let i = self.open_doc(rel)?;
        let before = self.docs[i].edited.clone();
        let mut next = before.clone();
        let mut applied = Vec::with_capacity(edits.len());
        for e in &edits {
            applied.push(edit::apply(&mut next, e).map_err(|err| format!("{label}: {err}"))?);
        }
        let doc = &mut self.docs[i];
        doc.edited = next;
        doc.refresh_dirty();
        // Consecutive sets of the same one field (typing a name, dragging a number) are one step: the first one's `before` is kept.
        let single = |c: &[Edit]| match c {
            [e] if e.new.is_some() => Some(e.pointer.clone()),
            _ => None,
        };
        if let Some(last) = self.undo.last_mut()
            && coalesce
            && last.open
            && self.redo.is_empty()
            && last.rel == rel
            && last.label == label
            && single(&last.edits).is_some()
            && single(&last.edits) == single(&applied)
        {
            let first_old = last.edits[0].old.clone();
            last.edits = applied;
            last.edits[0].old = first_old;
            return Ok(());
        }
        self.undo.push(Command { rel: rel.to_string(), label: label.to_string(), edits: applied, before, open: coalesce, group: 0 });
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(())
    }

    /// The label of the step Undo would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|c| c.label.as_str())
    }

    /// The label of the step Redo would apply again.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|c| c.label.as_str())
    }

    /// Takes back the last step (in whichever document it was); returns its label. A step of a group ([`Project::do_group`]) takes
    /// the rest of its group with it.
    pub fn undo(&mut self) -> Option<String> {
        self.shift(true)
    }

    /// Moves the last step of one stack to the other, and the rest of its group, undoing (`undoing`) or applying it again; returns the
    /// label of the first.
    fn shift(&mut self, undoing: bool) -> Option<String> {
        let one = |p: &mut Project| if undoing { p.undo_one() } else { p.redo_one() };
        let first = one(self)?;
        let (label, group) = (first.label.clone(), first.group);
        let mut moved = vec![first];
        while group != 0 && (if undoing { &self.undo } else { &self.redo }).last().is_some_and(|c| c.group == group) {
            let Some(next) = one(self) else { break };
            moved.push(next);
        }
        (if undoing { &mut self.redo } else { &mut self.undo }).extend(moved);
        Some(label)
    }

    /// Restores the document of the last undo step and returns the step (the caller pushes it on the redo stack).
    fn undo_one(&mut self) -> Option<Command> {
        let cmd = self.undo.pop()?;
        let Ok(i) = self.open_doc(&cmd.rel) else { return None };
        let doc = &mut self.docs[i];
        doc.edited = cmd.before.clone();
        doc.refresh_dirty();
        Some(cmd)
    }

    /// Applies the last undone step again (and the rest of its group); returns its label.
    pub fn redo(&mut self) -> Option<String> {
        self.shift(false)
    }

    fn redo_one(&mut self) -> Option<Command> {
        let cmd = self.redo.pop()?;
        let Ok(i) = self.open_doc(&cmd.rel) else { return None };
        let doc = &mut self.docs[i];
        let mut next = doc.edited.clone();
        for e in &cmd.edits {
            if edit::apply(&mut next, e).is_err() {
                return None;
            }
        }
        doc.edited = next;
        doc.refresh_dirty();
        Some(cmd)
    }

    /// T8: edits to SEVERAL documents as one undo step (a nebula is three records in two files that must stay equal). All or
    /// nothing: when one document refuses, the ones before it are put back. `steps` is `(document, edits)`; a step with no edits is
    /// skipped. Undo and redo take the whole group.
    pub fn do_group(&mut self, label: &str, steps: Vec<(String, Vec<Edit>)>) -> Result<(), String> {
        static NEXT_GROUP: AtomicUsize = AtomicUsize::new(1);
        let group = NEXT_GROUP.fetch_add(1, Ordering::Relaxed) as u64;
        for (pushed, (rel, edits)) in steps.into_iter().filter(|(_, e)| !e.is_empty()).enumerate() {
            if let Err(e) = self.push_edits(&rel, label, edits, false) {
                for _ in 0..pushed {
                    self.undo_one();
                }
                return Err(e);
            }
            if let Some(c) = self.undo.last_mut() {
                c.group = group;
            }
        }
        Ok(())
    }

    /// Writes every document with unsaved edits as a diff (see [`diff`]), then rebuilds the overlay. A document whose new file is
    /// equal in meaning to the one on disk is not written.
    pub fn save(&mut self) -> SaveReport {
        let mut report = SaveReport::default();
        // What was saved is a point to come back to: the next edit of the same field is a new step, not part of the one before.
        self.undo.iter_mut().for_each(|c| c.open = false);
        for rel in self.dirty_rels() {
            match self.save_doc(&rel) {
                Ok((outcome, warnings)) => {
                    match outcome {
                        Plan::Write(_) => report.written.push(rel.clone()),
                        Plan::Delete => report.deleted.push(rel.clone()),
                        Plan::Nothing => {}
                    }
                    report.warnings.extend(warnings.into_iter().map(|w| format!("{rel}: {w}")));
                }
                Err(e) => report.errors.push(e),
            }
        }
        if !(report.written.is_empty() && report.deleted.is_empty())
            && let Err(e) = self.rebuild()
        {
            report.errors.push(e);
        }
        report
    }

    fn save_doc(&mut self, rel: &str) -> Result<(Plan, Vec<String>), String> {
        let path = self.dir.join(rel);
        let import = self.import.clone();
        let dir = self.dir.clone();
        let doc = self.docs.iter_mut().find(|d| d.rel == rel).ok_or_else(|| format!("{rel}: not open"))?;
        let (plan, warnings) = doc.plan();
        match &plan {
            Plan::Write(file) => {
                guard::write(&path, to_json_string(file), import.as_deref())?;
                // The editor's own write is not an outside change.
                doc.disk = FileStamp::of(&path);
                doc.own = file.clone();
            }
            Plan::Delete => {
                match guard::remove_file(&path, import.as_deref()) {
                    Ok(()) => {}
                    Err(_) if !path.exists() => {}
                    Err(e) => return Err(e),
                }
                // Folders the file leaves empty go too (never the mod folder itself, never one with something in it).
                let mut at = path.parent().map(Path::to_path_buf);
                while let Some(d) = at.filter(|d| *d != dir && d.starts_with(&dir)) {
                    if guard::remove_empty_dir(&d, import.as_deref()).is_err() {
                        break;
                    }
                    at = d.parent().map(Path::to_path_buf);
                }
                doc.disk = None;
                doc.own = Value::Null;
            }
            Plan::Nothing => {}
        }
        doc.clean = doc.edited.clone();
        doc.dirty = false;
        doc.conflict = None;
        Ok((plan, warnings))
    }

    /// Takes the whole document `rel` back to what is below it (the retail file, or the parent), as one undo step. Saving then deletes
    /// the mod's file when it was a retail file's `extends`-itself file or a merged table (see [`Doc::plan`]). Errors when there is
    /// nothing below the file.
    pub fn revert_file(&mut self, rel: &str) -> Result<(), String> {
        let i = self.open_doc(rel)?;
        let doc = &self.docs[i];
        if doc.base.is_null() {
            return Err(format!("{rel}: nothing is below this file, so there is nothing to revert to"));
        }
        let (base, label) = (doc.base.clone(), if doc.is_retail_layer() { format!("Revert {rel} to retail") } else { format!("Revert {rel} to its parent") });
        self.do_step(rel, &label, vec![Edit::set("", base)])
    }

    /// Takes one part of the document `rel` (a JSON pointer, e.g. a gun's entry `/Gun_A`) back to what is below it: set to the base's
    /// value, or removed when the base has none, as one undo step.
    pub fn revert_at(&mut self, rel: &str, pointer: &str) -> Result<(), String> {
        let i = self.open_doc(rel)?;
        let base = self.docs[i].base.pointer(pointer).cloned();
        let edit = match base {
            Some(b) => Edit::set(pointer, b),
            None => Edit::remove(pointer),
        };
        self.do_step(rel, &format!("Revert {pointer} in {rel}"), vec![edit])
    }

    /// Looks at the files of the open documents (about once a second). A changed file of a clean document is reloaded; one of a
    /// document with unsaved edits is announced as a conflict (once) and both versions are kept until [`Project::resolve_reload`] or
    /// [`Project::resolve_keep`]. The overlay is rebuilt when anything changed.
    pub fn poll(&mut self) -> Vec<Notice> {
        let mut out: Vec<Notice> = std::mem::take(&mut self.notices);
        let mut reload = vec![];
        let mut touched = false;
        // Files added or removed by hand (or by a script): the layer list is the engine's report of the folder, so build it again.
        let now = layers::listing(&self.dir);
        if now != self.files {
            let part = |what: &str, a: &std::collections::BTreeSet<String>, b: &std::collections::BTreeSet<String>| {
                let names: Vec<&str> = a.difference(b).map(String::as_str).collect();
                let more = names.len().saturating_sub(3);
                (!names.is_empty()).then(|| format!("{what} {}{}", names[..names.len() - more].join(", "), if more > 0 { format!(" and {more} more") } else { String::new() }))
            };
            let parts: Vec<String> = [part("added", &now, &self.files), part("removed", &self.files, &now)].into_iter().flatten().collect();
            out.push(Notice { rel: String::new(), text: format!("mod folder changed: {}", parts.join("; ")), conflict: false });
            touched = true;
        }
        for d in &mut self.docs {
            let now = FileStamp::of(&self.dir.join(&d.rel));
            if now == d.disk {
                d.disk = now;
                d.conflict = None;
            } else if !d.dirty {
                reload.push((d.rel.clone(), now));
            } else if !d.conflict.as_ref().is_some_and(|seen| *seen == now) {
                d.conflict = Some(now);
                touched = true;
                out.push(Notice { rel: d.rel.clone(), text: format!("{} changed on disk and has unsaved edits here: reload from disk or keep your edits", d.rel), conflict: true });
            }
        }
        if (touched || !reload.is_empty())
            && let Err(e) = self.rebuild()
        {
            out.push(Notice { rel: String::new(), text: e, conflict: false });
        }
        for (rel, now) in reload {
            match self.load(&rel) {
                Ok(doc) => {
                    self.replace(doc);
                    out.push(Notice { rel: rel.clone(), text: format!("{rel} changed on disk: reloaded"), conflict: false });
                }
                Err(e) => {
                    if let Some(d) = self.docs.iter_mut().find(|d| d.rel == rel) {
                        d.disk = now;
                    }
                    out.push(Notice { rel, text: format!("changed on disk but cannot be read: {e}"), conflict: false });
                }
            }
        }
        out
    }

    /// Puts a freshly loaded document in place of the open one and forgets its undo steps.
    fn replace(&mut self, doc: Doc) {
        self.undo.retain(|c| c.rel != doc.rel);
        self.redo.retain(|c| c.rel != doc.rel);
        match self.docs.iter_mut().find(|d| d.rel == doc.rel) {
            Some(slot) => *slot = doc,
            None => self.docs.push(doc),
        }
    }

    /// "Reload from disk": drops the unsaved edits of `rel` and its undo steps.
    pub fn resolve_reload(&mut self, rel: &str) -> Result<(), String> {
        self.rebuild()?;
        let doc = self.load(rel)?;
        self.replace(doc);
        Ok(())
    }

    /// "Keep my edits": the file on disk becomes what the mod has now (`own`, and the document below it), and the steps made in this
    /// document are applied again on top of it, so what the outside change added stays. The next save writes the result. The
    /// undo steps of the document are dropped (their snapshots are of the old text).
    pub fn resolve_keep(&mut self, rel: &str) -> Result<(), String> {
        self.rebuild()?;
        let mut fresh = self.load(rel)?;
        for cmd in self.undo.iter().filter(|c| c.rel == rel) {
            for e in &cmd.edits {
                // A step that no longer fits the new text is skipped; the user sees the result before saving.
                let _ = edit::apply(&mut fresh.edited, &Edit { old: None, ..e.clone() });
            }
        }
        fresh.refresh_dirty();
        self.replace(fresh);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-project-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// A tiny synthetic import (one ship, a guns table) and an empty mod folder, both made here.
    fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = tmp(name);
        let (assets, m) = (root.join("assets"), root.join("mod"));
        write(&assets, "ships/Ship_X/ship.json", r#"{"name": "X", "speed": 3, "engines": [{"fx": "A"}, {"fx": "A"}, {"fx": "A"}, {"fx": "A"}]}"#);
        write(&assets, "data/weapons/guns.json", r#"{"Gun_A": {"range": 10}}"#);
        std::fs::create_dir_all(&m).unwrap();
        (root, assets, m)
    }

    #[test]
    fn the_check_folder_holds_the_unsaved_edits_and_the_mod_folder_is_untouched() {
        let (root, assets, m) = setup("check-dir");
        write(&m, "mod.json", r#"{"id": "t", "name": "T", "format": 1}"#);
        write(&m, "ships/Ship_X/model.glb", "glb");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        assert_eq!(p.check_dir().unwrap(), p.dir, "nothing unsaved: the mod folder itself");
        p.do_edits("ships/Ship_X/ship.json", "Typo", vec![Edit::set("/sigth", json!(3))]).unwrap();
        let dir = p.check_dir().unwrap();
        assert_ne!(dir, p.dir);
        let text = std::fs::read_to_string(dir.join("ships/Ship_X/ship.json")).unwrap();
        assert!(text.contains("sigth"), "{text}");
        assert!(dir.join("mod.json").is_file() && dir.join("ships/Ship_X/model.glb").is_file(), "the other files come along");
        assert!(!m.join("ships/Ship_X/ship.json").exists(), "nothing written to the mod folder");
        let found = openprocyon::authoring::check_mod(&dir, p.import.as_deref());
        assert!(found.iter().any(|f| f.path.contains("sigth")), "{found:#?}");
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn merge_kind_follows_the_path_rule_of_the_docs() {
        let none = Value::Null;
        assert_eq!(merge_kind("data/weapons/guns.json", &none), DocKind::Merged);
        assert_eq!(merge_kind("menu/strings.json", &none), DocKind::Merged);
        assert_eq!(merge_kind("ships/Ship_X/ship.json", &none), DocKind::Extends);
        assert_eq!(merge_kind("maps/red/map.json", &none), DocKind::Extends);
        assert_eq!(merge_kind("data/render/look.json", &none), DocKind::Replaced);
        assert_eq!(merge_kind("fx/Boom.json", &json!({"extends": "Bang"})), DocKind::Extends);
        assert_eq!(merge_kind("data/weapons/guns.json", &json!({"extends": "x"})), DocKind::Merged, "a merged path ignores extends");
        assert_eq!(parent_rel("ships/A/ship.json", "B"), "ships/B/ship.json");
        assert_eq!(parent_rel("fx/Boom.json", "Bang"), "fx/Bang.json");
        assert_eq!(own_id("ships/A/ship.json"), "A");
        assert_eq!(own_id("fx/Boom.json"), "Boom");
    }

    #[test]
    fn create_writes_mod_json_once_and_never_overwrites() {
        let root = tmp("create");
        let dir = root.join("My-Mod");
        let (p, created) = Project::create(&dir, None).unwrap();
        assert!(created);
        let meta: Value = serde_json::from_slice(&std::fs::read(dir.join("mod.json")).unwrap()).unwrap();
        assert_eq!(meta, json!({"id": "My-Mod", "name": "My-Mod", "version": "0.1.0", "description": "", "category": "Other", "format": 1, "game_version": ENGINE_VERSION}));
        let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["LICENSE", "mod.json"], "nothing else is written");
        let text = std::fs::read_to_string(dir.join("LICENSE")).unwrap();
        assert!(text.contains("SPDX-License-Identifier: CC-BY-4.0"), "{text}");
        assert_eq!(p.name(), "My-Mod");
        drop(p);
        write(&dir, "mod.json", r#"{"id": "mine", "name": "Mine"}"#);
        let (p, created) = Project::create(&dir, None).unwrap();
        assert!(!created);
        assert_eq!(p.name(), "Mine");
        assert_eq!(std::fs::read_to_string(dir.join("mod.json")).unwrap(), r#"{"id": "mine", "name": "Mine"}"#);
        assert!(std::fs::read_to_string(dir.join("LICENSE")).unwrap().contains("CC-BY-4.0"), "a missing LICENSE is added to an existing mod");
        drop(p);
        write(&dir, "LICENSE", "mine");
        drop(Project::create(&dir, None).unwrap());
        assert_eq!(std::fs::read_to_string(dir.join("LICENSE")).unwrap(), "mine", "an existing LICENSE is never overwritten");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_mod_has_the_chosen_licence_an_id_and_the_folders_the_first_change_needs_and_no_retail_file() {
        let (root, assets, _) = setup("complete");
        let dir = root.join("folder");
        let new =
            NewMod { id: Some(" cool-mod ".into()), licence: licence::Licence::Gpl3OrLater, author: "Ada".into(), folders: vec!["ships/Ship_X".into(), "data/weapons".into()] };
        let (p, created) = Project::create_with(&dir, Some(assets.clone()), &new).unwrap();
        assert!(created);
        assert_eq!(p.name(), "cool-mod");
        let text = std::fs::read_to_string(dir.join("LICENSE")).unwrap();
        assert!(text.contains("GPL-3.0-or-later") && text.contains("Copyright (c) Ada") && text.contains("gnu.org"), "{text}");
        assert!(dir.join("ships/Ship_X").is_dir() && dir.join("data/weapons").is_dir());
        // nothing of the import was copied
        let mut files = vec![];
        fn walk(d: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                if e.path().is_dir() { walk(&e.path(), out) } else { out.push(e.file_name().to_string_lossy().into_owned()) }
            }
        }
        walk(&dir, &mut files);
        files.sort();
        assert_eq!(files, ["LICENSE", "mod.json"]);
        assert!(Project::create_with(&root.join("bad"), None, &NewMod { folders: vec!["../x".into()], ..Default::default() }).is_err(), "folders stay inside the mod");
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn editing_a_retail_ship_saves_an_extends_file_the_engine_reads_back() {
        let (root, assets, m) = setup("save");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        p.do_edits("ships/Ship_X/ship.json", "Speed", vec![Edit::set("/speed", json!(5))]).unwrap();
        p.do_edits("ships/Ship_X/ship.json", "Engine", vec![Edit::set("/engines/2/fx", json!("B"))]).unwrap();
        let d = p.doc("ships/Ship_X/ship.json").unwrap();
        assert_eq!((d.kind, d.own.clone(), d.dirty), (DocKind::Extends, Value::Null, true));
        let report = p.save();
        assert_eq!(report.written, ["ships/Ship_X/ship.json"]);
        assert!(report.warnings.is_empty() && report.errors.is_empty(), "{report:?}");
        let file: Value = serde_json::from_slice(&std::fs::read(m.join("ships/Ship_X/ship.json")).unwrap()).unwrap();
        assert_eq!(file, json!({"extends": "Ship_X", "speed": 5, "set": {"/engines/2/fx": "B"}}));
        assert!(!p.doc("ships/Ship_X/ship.json").unwrap().dirty);
        // the rebuilt overlay shows the mod as extending the ship
        assert_eq!(p.entry().unwrap().extends, ["ships/Ship_X/ship.json"]);
        assert_eq!(p.layer_rows().iter().filter(|r| r.kind == layers::LayerKind::Extends).count(), 1);
        // nothing dirty: a second save writes nothing
        assert_eq!(p.save(), SaveReport::default());
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_merged_file_and_a_new_file_save_as_expected() {
        let (root, assets, m) = setup("merged");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        p.do_edits("data/weapons/guns.json", "Range", vec![Edit::set("/Gun_A/range", json!(12)), Edit::set("/Gun_B", json!({"range": 1}))]).unwrap();
        p.do_edits("data/new.json", "New", vec![Edit::set("/a", json!(1))]).unwrap();
        let rep = p.save();
        assert_eq!(rep.written.len(), 2, "{rep:?}");
        let guns: Value = serde_json::from_slice(&std::fs::read(m.join("data/weapons/guns.json")).unwrap()).unwrap();
        assert_eq!(guns, json!({"Gun_A": {"range": 12}, "Gun_B": {"range": 1}}));
        assert_eq!(std::fs::read_to_string(m.join("data/new.json")).unwrap(), "{\n  \"a\": 1\n}\n");
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn undo_and_redo_run_across_documents_and_track_dirty() {
        let (root, assets, m) = setup("undo");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let (ship, guns) = ("ships/Ship_X/ship.json", "data/weapons/guns.json");
        p.do_edits(ship, "Speed", vec![Edit::set("/speed", json!(9))]).unwrap();
        p.do_edits(guns, "Range", vec![Edit::set("/Gun_A/range", json!(99)), Edit::set("/Gun_Z/deep/er", json!(1))]).unwrap();
        assert_eq!((p.undo_label(), p.redo_label()), (Some("Range"), None));
        assert_eq!(p.undo().as_deref(), Some("Range"));
        let g = p.doc(guns).unwrap();
        assert_eq!(g.edited, json!({"Gun_A": {"range": 10}}), "created parents leave with the step");
        assert!(!g.dirty, "back to the saved state");
        assert_eq!(p.undo().as_deref(), Some("Speed"));
        assert_eq!(p.doc(ship).unwrap().edited["speed"], 3);
        assert!(p.undo().is_none());
        assert_eq!(p.redo().as_deref(), Some("Speed"));
        assert_eq!(p.redo().as_deref(), Some("Range"));
        assert_eq!(p.doc(guns).unwrap().edited["Gun_A"]["range"], 99);
        assert_eq!(p.doc(ship).unwrap().edited["speed"], 9);
        assert!(p.redo().is_none());
        // a new step clears redo
        p.undo();
        p.do_edits(ship, "Again", vec![Edit::set("/speed", json!(1))]).unwrap();
        assert!(p.redo_label().is_none());
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_failing_edit_in_the_middle_changes_nothing_and_leaves_no_step() {
        let (root, assets, m) = setup("fail");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let ship = "ships/Ship_X/ship.json";
        let before = p.open_doc(ship).map(|i| p.docs[i].edited.clone()).unwrap();
        let err = p.do_edits(ship, "Broken", vec![Edit::set("/speed", json!(7)), Edit::set("/name/inside", json!(1)), Edit::set("/never", json!(1))]).unwrap_err();
        assert!(err.starts_with("Broken: "), "{err}");
        let d = p.doc(ship).unwrap();
        assert_eq!(d.edited, before);
        assert!(!d.dirty);
        assert!(p.undo_label().is_none());
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_outside_write_reloads_a_clean_document_and_the_editors_own_save_is_not_seen() {
        let (root, assets, m) = setup("poll");
        write(&m, "data/weapons/guns.json", r#"{"Gun_A": {"range": 11}}"#);
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let guns = "data/weapons/guns.json";
        p.open_doc(guns).unwrap();
        assert!(p.poll().is_empty());
        // the editor's own save
        p.do_edits(guns, "Range", vec![Edit::set("/Gun_A/range", json!(12))]).unwrap();
        p.save();
        assert!(p.poll().is_empty(), "own write is not an outside change");
        // a script rewrites the file
        write(&m, guns, r#"{"Gun_A": {"range": 20, "extra": true}}"#);
        let notes = p.poll();
        assert_eq!(notes, [Notice { rel: guns.into(), text: format!("{guns} changed on disk: reloaded"), conflict: false }]);
        assert_eq!(p.doc(guns).unwrap().edited["Gun_A"]["range"], 20);
        assert!(p.undo_label().is_none(), "the steps of a reloaded document are gone");
        assert!(p.poll().is_empty());
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_outside_write_to_a_dirty_document_is_a_conflict_until_resolved() {
        let (root, assets, m) = setup("conflict");
        write(&m, "data/weapons/guns.json", r#"{"Gun_A": {"range": 11}}"#);
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let guns = "data/weapons/guns.json";
        p.do_edits(guns, "Mine", vec![Edit::set("/Gun_A/range", json!(50))]).unwrap();
        write(&m, guns, r#"{"Gun_A": {"range": 20}, "Gun_B": {"range": 2}}"#);
        let notes = p.poll();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].conflict && notes[0].rel == guns);
        assert_eq!(p.doc(guns).unwrap().edited["Gun_A"]["range"], 50, "the edits stay");
        assert!(p.poll().is_empty(), "announced once");
        // keep: the file on disk becomes `own`, the edit is saved over it and the hand-written key stays
        p.resolve_keep(guns).unwrap();
        assert!(p.doc(guns).unwrap().dirty && p.doc(guns).unwrap().conflict.is_none());
        p.save();
        let file: Value = serde_json::from_slice(&std::fs::read(m.join(guns)).unwrap()).unwrap();
        assert_eq!(file["Gun_A"]["range"], 50);
        assert_eq!(file["Gun_B"], json!({"range": 2}));
        // reload: a second conflict, resolved the other way
        p.do_edits(guns, "Again", vec![Edit::set("/Gun_A/range", json!(60))]).unwrap();
        write(&m, guns, r#"{"Gun_A": {"range": 30}}"#);
        assert!(p.poll()[0].conflict);
        p.resolve_reload(guns).unwrap();
        let d = p.doc(guns).unwrap();
        assert_eq!((d.edited["Gun_A"]["range"].as_i64(), d.dirty), (Some(30), false));
        assert!(p.poll().is_empty());
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_removed_inherited_key_is_reported_by_save() {
        let (root, assets, m) = setup("warn");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        p.do_edits("ships/Ship_X/ship.json", "Drop speed", vec![Edit::remove("/speed"), Edit::set("/name", json!("Y"))]).unwrap();
        let rep = p.save();
        assert_eq!(rep.written.len(), 1);
        assert_eq!(rep.warnings.len(), 1);
        assert!(rep.warnings[0].starts_with("ships/Ship_X/ship.json: /speed"), "{rep:?}");
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_added_or_removed_by_hand_shows_in_the_layers_at_the_next_poll() {
        let (root, assets, m) = setup("handfile");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        assert!(p.poll().is_empty());
        write(&m, "data/render/look.json", r#"{"a": 1}"#);
        let notes = p.poll();
        assert!(notes.iter().any(|n| n.text.contains("added data/render/look.json")), "{notes:?}");
        assert!(p.layer_rows().iter().any(|r| r.file == "data/render/look.json"), "{:?}", p.layer_rows());
        assert!(p.poll().is_empty(), "announced once");
        std::fs::remove_file(m.join("data/render/look.json")).unwrap();
        let notes = p.poll();
        assert!(notes.iter().any(|n| n.text.contains("removed data/render/look.json")), "{notes:?}");
        assert!(!p.layer_rows().iter().any(|r| r.file == "data/render/look.json"));
        // the editor's own save is not announced as a change by hand
        p.do_edits("ships/Ship_X/ship.json", "e", vec![Edit::set("/speed", json!(9))]).unwrap();
        assert!(p.save().errors.is_empty());
        assert!(p.poll().is_empty(), "own writes are known");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_folder_change_message_names_three_files_and_counts_the_rest() {
        let (root, assets, m) = setup("handmsg");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        for i in 1..=5 {
            write(&m, &format!("data/render/a{i}.json"), r#"{"a": 1}"#);
        }
        let text = |n: Vec<Notice>| n.into_iter().map(|n| n.text).find(|t| t.starts_with("mod folder changed")).unwrap_or_default();
        assert_eq!(text(p.poll()), "mod folder changed: added data/render/a1.json, data/render/a2.json, data/render/a3.json and 2 more");
        std::fs::remove_file(m.join("data/render/a1.json")).unwrap();
        write(&m, "data/render/b.json", r#"{"a": 1}"#);
        assert_eq!(text(p.poll()), "mod folder changed: added data/render/b.json; removed data/render/a1.json");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_check_folder_gets_an_unsaved_file_that_saves_as_it_is_even_when_its_folder_holds_nothing_else() {
        let (root, assets, m) = setup("checknothing");
        write(&m, "ships/Ship_X/ship.json", r#"{"extends": "Ship_X", "speed": 5}"#);
        let mut p = Project::open(&m, Some(assets)).unwrap();
        // a key of the retail ship taken away: the file cannot say so, so it saves as it is, and the edit is a warning
        p.do_step("ships/Ship_X/ship.json", "Remove name", vec![Edit::remove("/name")]).unwrap();
        assert_eq!(p.doc("ships/Ship_X/ship.json").unwrap().plan().0, Plan::Nothing);
        let out = p.check_dir().unwrap();
        assert_eq!(std::fs::read_to_string(out.join("ships/Ship_X/ship.json")).unwrap(), r#"{"extends": "Ship_X", "speed": 5}"#);
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn paths_outside_the_mod_are_refused() {
        let (root, _assets, m) = setup("paths");
        let mut p = Project::open(&m, None).unwrap();
        for bad in ["../x.json", "/etc/passwd", "a/../b.json", "", "a\\b.json"] {
            assert!(p.open_doc(bad).is_err(), "{bad}");
        }
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn repeated_sets_of_one_field_are_one_undo_step() {
        let dir = std::env::temp_dir().join(format!("procyon-editor-coalesce-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut p, _) = Project::create(&dir, None).unwrap();
        let doc = |p: &mut Project| {
            let i = p.open_doc("mod.json").unwrap();
            p.docs[i].edited.clone()
        };
        let name = doc(&mut p)["name"].clone();
        for text in ["A", "Ab", "Abc"] {
            p.do_edits("mod.json", "Edit mod.json", vec![Edit::set("/name", text.into())]).unwrap();
        }
        p.do_edits("mod.json", "Edit mod.json", vec![Edit::set("/version", "2".into())]).unwrap();
        p.do_edits("mod.json", "Edit mod.json", vec![Edit::set("/name", "X".into())]).unwrap();
        assert_eq!(doc(&mut p)["name"], "X");
        p.undo();
        assert_eq!(doc(&mut p)["name"], "Abc", "another field in between starts a new step");
        p.undo();
        p.undo();
        assert_eq!(doc(&mut p)["name"], name, "the three keystrokes were one step");
        let i = p.open_doc("mod.json").unwrap();
        assert!(!p.docs[i].dirty);
        p.redo();
        assert_eq!(doc(&mut p)["name"], "Abc");
        // a step of its own is never merged
        p.do_step("mod.json", "Move", vec![Edit::set("/version", "3".into())]).unwrap();
        p.do_step("mod.json", "Move", vec![Edit::set("/version", "4".into())]).unwrap();
        p.undo();
        assert_eq!(doc(&mut p)["version"], "3");
        drop(p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reset_button_is_its_own_step_and_a_save_ends_typing() {
        let (root, assets, m) = setup("resetstep");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let rel = "ships/Ship_X/ship.json";
        let speed = |p: &Project| p.doc(rel).unwrap().edited["speed"].clone();
        let typed = |p: &mut Project, v: i64| p.apply_form(rel, "Edit Ship_X", vec![Edit::set("/speed", json!(v))], true).unwrap();
        // type 7, 70, then the Reset button (same field, same label, one set), then Undo
        typed(&mut p, 7);
        typed(&mut p, 70);
        p.apply_form(rel, "Edit Ship_X", vec![Edit::set("/speed", json!(3))], false).unwrap();
        assert_eq!(speed(&p), json!(3));
        p.undo();
        assert_eq!(speed(&p), json!(70), "undoing the reset brings the typed value back");
        p.undo();
        assert_eq!(speed(&p), json!(3), "the typing was one step");
        p.redo();
        p.redo();
        // typing after a reset is a new step too, not part of the reset
        typed(&mut p, 9);
        p.undo();
        assert_eq!(speed(&p), json!(3));
        // a save ends the typing step
        typed(&mut p, 7);
        assert!(p.save().errors.is_empty());
        typed(&mut p, 70);
        p.undo();
        assert_eq!(speed(&p), json!(7), "back to the saved value");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_document_extends_a_parent_or_starts_whole() {
        let (root, assets, m) = setup("newdoc");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        assert!(p.open_new_doc("ships/Ship_X/ship.json", None, json!({})).is_err(), "retail file exists");
        assert!(p.open_new_doc("../x.json", None, json!({})).is_err());
        let i = p.open_new_doc("ships/Ship_New/ship.json", Some("Ship_X"), Value::Null).unwrap();
        assert_eq!((p.docs[i].edited["speed"].as_i64(), p.docs[i].parent.as_deref()), (Some(3), Some("Ship_X")));
        assert!(p.open_new_doc("ships/Ship_New/ship.json", None, json!({})).is_err(), "already open");
        assert!(p.open_new_doc("ships/Ship_Q/ship.json", Some("Nope"), Value::Null).is_err());
        p.save();
        assert!(!m.join("ships/Ship_New/ship.json").exists(), "nothing changed: nothing written");
        p.do_edits("ships/Ship_New/ship.json", "Edit", vec![Edit::set("/speed", json!(5))]).unwrap();
        p.save();
        assert_eq!(std::fs::read_to_string(m.join("ships/Ship_New/ship.json")).unwrap().replace([' ', '\n'], ""), r#"{"extends":"Ship_X","speed":5}"#);
        p.open_new_doc("ships/Ship_W/ship.json", None, json!({"race": "", "points": 0})).unwrap();
        p.save();
        assert!(m.join("ships/Ship_W/ship.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resetting_every_change_by_hand_is_clean_again() {
        let (root, assets, m) = setup("cleanagain");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let rel = "ships/Ship_X/ship.json";
        p.do_edits(rel, "a", vec![Edit::set("/speed", json!(5))]).unwrap();
        assert!(p.doc(rel).unwrap().dirty);
        p.do_edits(rel, "b", vec![Edit::set("/speed", json!(3))]).unwrap();
        assert!(!p.doc(rel).unwrap().dirty, "equal to retail again: nothing to save");
        assert_eq!(p.save(), SaveReport::default());
        // after a save, undoing back is a change to save (the file has to go), redoing is clean
        p.do_edits(rel, "c", vec![Edit::set("/speed", json!(6))]).unwrap();
        assert_eq!(p.save().written, [rel]);
        assert!(!p.doc(rel).unwrap().dirty);
        p.undo();
        assert!(p.doc(rel).unwrap().dirty);
        p.redo();
        assert!(!p.doc(rel).unwrap().dirty);
        // a document that exists nowhere yet stays unsaved until it is written
        p.open_new_doc("ships/Ship_N/ship.json", None, json!({"race": ""})).unwrap();
        assert!(p.doc("ships/Ship_N/ship.json").unwrap().dirty);
        p.do_edits("ships/Ship_N/ship.json", "n", vec![Edit::set("/race", json!("x"))]).unwrap();
        p.undo();
        assert!(p.doc("ships/Ship_N/ship.json").unwrap().dirty);
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reverting_a_retail_ship_file_deletes_the_extends_file_on_save_and_undo_brings_it_back() {
        let (root, assets, m) = setup("revertship");
        let rel = "ships/Ship_X/ship.json";
        let mut p = Project::open(&m, Some(assets.clone())).unwrap();
        assert!(!p.open_doc(rel).map(|i| p.docs[i].can_revert()).unwrap(), "an untouched retail ship has nothing to revert");
        p.do_edits(rel, "Speed", vec![Edit::set("/speed", json!(5))]).unwrap();
        assert!(p.doc(rel).unwrap().can_revert());
        assert_eq!(p.save().written, [rel]);
        assert!(m.join(rel).is_file());
        p.revert_file(rel).unwrap();
        assert_eq!(p.undo_label(), Some("Revert ships/Ship_X/ship.json to retail"));
        assert_eq!(p.doc(rel).unwrap().edited["speed"], 3);
        assert!(p.doc(rel).unwrap().dirty);
        // undo of the revert is one step and restores the edit
        p.undo();
        assert_eq!(p.doc(rel).unwrap().edited["speed"], 5);
        assert!(!p.doc(rel).unwrap().dirty, "back to what is on disk");
        p.redo();
        let report = p.save();
        assert_eq!((report.deleted.as_slice(), report.written.len(), report.errors.len()), ([rel.to_string()].as_slice(), 0, 0), "{report:?}");
        assert!(!m.join(rel).exists() && !m.join("ships/Ship_X").exists() && !m.join("ships").exists(), "the file and the folders it leaves empty are gone");
        assert!(m.is_dir());
        assert_eq!(p.entry().unwrap().extends.len(), 0, "the engine sees no extends file");
        assert!(!p.doc(rel).unwrap().can_revert() && !p.doc(rel).unwrap().dirty);
        assert!(std::fs::read_to_string(assets.join(rel)).unwrap().contains("\"speed\": 3"), "retail is untouched");
        // editing again after the delete writes a fresh file
        p.do_edits(rel, "Speed", vec![Edit::set("/speed", json!(7))]).unwrap();
        assert_eq!(p.save().written, [rel]);
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reverting_one_gun_keeps_the_other_changed_entry_and_the_last_revert_deletes_the_file() {
        let (root, assets, m) = setup("revertgun");
        write(&assets, "data/weapons/guns.json", r#"{"Gun_A": {"range": 10}, "Gun_B": {"range": 1, "dmg": 2}}"#);
        let rel = "data/weapons/guns.json";
        let mut p = Project::open(&m, Some(assets)).unwrap();
        p.do_edits(rel, "A", vec![Edit::set("/Gun_A/range", json!(12))]).unwrap();
        p.do_edits(rel, "B", vec![Edit::set("/Gun_B/range", json!(2)), Edit::set("/Gun_C", json!({"range": 9}))]).unwrap();
        assert_eq!(p.save().written, [rel]);
        let file = |m: &Path| -> Value { serde_json::from_slice(&std::fs::read(m.join(rel)).unwrap()).unwrap() };
        assert_eq!(file(&m), json!({"Gun_A": {"range": 12}, "Gun_B": {"range": 2}, "Gun_C": {"range": 9}}));
        p.revert_at(rel, "/Gun_A").unwrap();
        assert_eq!(p.doc(rel).unwrap().edited["Gun_A"], json!({"range": 10}));
        assert_eq!(p.save().written, [rel]);
        assert_eq!(file(&m), json!({"Gun_B": {"range": 2}, "Gun_C": {"range": 9}}), "Gun_A has no entry left, the others stay");
        // a new entry has no retail value: revert removes it
        p.revert_at(rel, "/Gun_C").unwrap();
        assert!(p.doc(rel).unwrap().edited.get("Gun_C").is_none());
        // one undo step each
        assert_eq!(p.undo().as_deref(), Some("Revert /Gun_C in data/weapons/guns.json"));
        assert!(p.doc(rel).unwrap().edited.get("Gun_C").is_some());
        p.redo();
        p.revert_file(rel).unwrap();
        let report = p.save();
        assert_eq!(report.deleted, [rel], "{report:?}");
        assert!(!m.join(rel).exists());
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_ship_new_in_the_mod_has_nothing_to_revert_to_and_a_clone_goes_back_to_its_parent() {
        let (root, assets, m) = setup("revertnew");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        p.open_new_doc("ships/Ship_N/ship.json", None, json!({"race": "x"})).unwrap();
        assert!(!p.doc("ships/Ship_N/ship.json").unwrap().can_revert());
        assert!(p.revert_file("ships/Ship_N/ship.json").is_err());
        p.open_new_doc("ships/Ship_C/ship.json", Some("Ship_X"), Value::Null).unwrap();
        p.do_edits("ships/Ship_C/ship.json", "e", vec![Edit::set("/speed", json!(9))]).unwrap();
        assert!(p.save().written.contains(&"ships/Ship_C/ship.json".to_string()));
        p.revert_file("ships/Ship_C/ship.json").unwrap();
        assert_eq!(p.undo_label(), Some("Revert ships/Ship_C/ship.json to its parent"));
        let report = p.save();
        assert_eq!(report.written, ["ships/Ship_C/ship.json"], "the clone itself stays");
        let file: Value = serde_json::from_slice(&std::fs::read(m.join("ships/Ship_C/ship.json")).unwrap()).unwrap();
        assert_eq!(file, json!({"extends": "Ship_X"}));
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }
}
