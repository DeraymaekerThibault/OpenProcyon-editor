//! The mod project: a mod folder opened for editing. It holds the folder, the player's import, the overlay of the two (rebuilt after a
//! save or a reload), documents opened on demand, an undo stack over JSON edits, saving as a diff against what each file is laid over,
//! and a watch on the files so a script or hand edit is noticed (docs/roadmap.md principle 4 and "Code-first authoring").
//! No window code here: everything is testable headless.

pub mod diff;
pub mod layers;
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
}

/// What a save did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Files written (relative paths).
    pub written: Vec<String>,
    /// Changes a mod file cannot express (a removed inherited key), with the file named.
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
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
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(crate::overlay::scratch(&self.scratch));
        let _ = std::fs::remove_dir_all(crate::overlay::scratch(&format!("{}-empty", self.scratch)));
        // The per-process folder goes too once no project of this process uses it (fails, harmlessly, while another does).
        if let Some(parent) = crate::overlay::scratch(&self.scratch).parent() {
            let _ = std::fs::remove_dir(parent);
        }
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
        let scratch = format!("project-{}", NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed));
        let mut p = Project { root: dir.clone(), dir, import, report: ModReport::default(), docs: vec![], undo: vec![], redo: vec![], notices: vec![], scratch };
        p.rebuild()?;
        if p.dir.join("mod.json").is_file()
            && let Err(e) = p.open_doc("mod.json")
        {
            p.notices.push(Notice { rel: "mod.json".into(), text: e, conflict: false });
        }
        Ok(p)
    }

    /// Makes a mod of the folder `dir`: writes `mod.json` (id = folder name, format 1, this engine's version) unless the folder
    /// already has one, which is never overwritten, then opens it. Writes nothing else. The flag says whether a file was written.
    pub fn create(dir: &Path, import: Option<PathBuf>) -> Result<(Project, bool), String> {
        let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join("mod.json");
        let created = !path.exists();
        if created {
            let id = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "my-mod".into());
            let meta = json!({"id": id, "name": id, "version": "0.1.0", "description": "", "category": "Other", "format": 1, "game_version": ENGINE_VERSION});
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            std::fs::write(&path, to_json_string(&meta)).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok((Project::open(&dir, import)?, created))
    }

    /// Rebuilds the overlay (after a save or when a file changed on disk) and the engine's report.
    pub fn rebuild(&mut self) -> Result<(), String> {
        let base = match &self.import {
            Some(b) => b.clone(),
            None => {
                let empty = crate::overlay::scratch(&format!("{}-empty", self.scratch));
                std::fs::create_dir_all(&empty).map_err(|e| format!("{}: {e}", empty.display()))?;
                empty
            }
        };
        let (root, report) = crate::overlay::build(&base, std::slice::from_ref(&self.dir), &crate::overlay::scratch(&self.scratch))?;
        (self.root, self.report) = (root, report);
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
            // PROVISIONAL: when the engine skipped the file (bad `extends`), show the file itself; the problems list says why.
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

    /// Applies `edits` to the document `rel` as one undoable step, all or nothing: when one fails, the document is left as it was and
    /// the error names the step. Clears the redo stack.
    pub fn do_edits(&mut self, rel: &str, label: &str, edits: Vec<Edit>) -> Result<(), String> {
        let i = self.open_doc(rel)?;
        let before = self.docs[i].edited.clone();
        let mut next = before.clone();
        let mut applied = Vec::with_capacity(edits.len());
        for e in &edits {
            applied.push(edit::apply(&mut next, e).map_err(|err| format!("{label}: {err}"))?);
        }
        let doc = &mut self.docs[i];
        doc.edited = next;
        doc.dirty = doc.edited != doc.clean;
        // Consecutive sets of the same one field (typing a name, dragging a number) are one step: the first one's `before` is kept.
        let single = |c: &[Edit]| match c {
            [e] if e.new.is_some() => Some(e.pointer.clone()),
            _ => None,
        };
        if let Some(last) = self.undo.last_mut()
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
        self.undo.push(Command { rel: rel.to_string(), label: label.to_string(), edits: applied, before });
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

    /// Takes back the last step (in whichever document it was); returns its label.
    pub fn undo(&mut self) -> Option<String> {
        let cmd = self.undo.pop()?;
        let Ok(i) = self.open_doc(&cmd.rel) else { return None };
        let doc = &mut self.docs[i];
        doc.edited = cmd.before.clone();
        doc.dirty = doc.edited != doc.clean;
        let label = cmd.label.clone();
        self.redo.push(cmd);
        Some(label)
    }

    /// Applies the last undone step again; returns its label.
    pub fn redo(&mut self) -> Option<String> {
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
        doc.dirty = doc.edited != doc.clean;
        let label = cmd.label.clone();
        self.undo.push(cmd);
        Some(label)
    }

    /// Writes every document with unsaved edits as a diff (see [`diff`]), then rebuilds the overlay. A document whose new file is
    /// equal in meaning to the one on disk is not written.
    pub fn save(&mut self) -> SaveReport {
        let mut report = SaveReport::default();
        for rel in self.dirty_rels() {
            match self.save_doc(&rel) {
                Ok((wrote, warnings)) => {
                    if wrote {
                        report.written.push(rel.clone());
                    }
                    report.warnings.extend(warnings.into_iter().map(|w| format!("{rel}: {w}")));
                }
                Err(e) => report.errors.push(e),
            }
        }
        if !report.written.is_empty()
            && let Err(e) = self.rebuild()
        {
            report.errors.push(e);
        }
        report
    }

    fn save_doc(&mut self, rel: &str) -> Result<(bool, Vec<String>), String> {
        let path = self.dir.join(rel);
        let doc = self.docs.iter_mut().find(|d| d.rel == rel).ok_or_else(|| format!("{rel}: not open"))?;
        let d = diff::compute(doc.kind, &doc.base, &doc.edited, &doc.own, doc.parent.as_deref());
        let mut wrote = false;
        if let Some(file) = d.file.filter(|f| *f != doc.own) {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            let text = to_json_string(&file);
            std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
            // The editor's own write is not an outside change.
            doc.disk = FileStamp::of(&path);
            doc.own = file;
            wrote = true;
        }
        doc.clean = doc.edited.clone();
        doc.dirty = false;
        doc.conflict = None;
        Ok((wrote, d.warnings))
    }

    /// Looks at the files of the open documents (about once a second). A changed file of a clean document is reloaded; one of a
    /// document with unsaved edits is announced as a conflict (once) and both versions are kept until [`Project::resolve_reload`] or
    /// [`Project::resolve_keep`]. The overlay is rebuilt when anything changed.
    pub fn poll(&mut self) -> Vec<Notice> {
        let mut out: Vec<Notice> = std::mem::take(&mut self.notices);
        let mut reload = vec![];
        let mut touched = false;
        for d in &mut self.docs {
            let now = FileStamp::of(&self.dir.join(&d.rel));
            if stamp::same(&now, &d.disk) {
                d.disk = now;
                d.conflict = None;
            } else if !d.dirty {
                reload.push((d.rel.clone(), now));
            } else if !d.conflict.as_ref().is_some_and(|seen| stamp::same(seen, &now)) {
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
        fresh.dirty = fresh.edited != fresh.clean;
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
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing else is written");
        assert_eq!(p.name(), "My-Mod");
        drop(p);
        write(&dir, "mod.json", r#"{"id": "mine", "name": "Mine"}"#);
        let (p, created) = Project::create(&dir, None).unwrap();
        assert!(!created);
        assert_eq!(p.name(), "Mine");
        assert_eq!(std::fs::read_to_string(dir.join("mod.json")).unwrap(), r#"{"id": "mine", "name": "Mine"}"#);
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
        drop(p);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
