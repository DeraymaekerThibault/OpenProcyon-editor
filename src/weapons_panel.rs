//! The "Weapons" tab: the entries of `data/weapons/guns.json` and `bullets.json` (merged-path files: the mod's file holds only the
//! entries and keys it changes, which the project's save computes), one entry's schema form, new and duplicated entries, and a
//! bullet picker for a gun's `BulletTypeName_*` fields. The pure parts (entry sub-schema, pointer prefixing, rows) are tested headless.

use bevy::prelude::Resource;
use bevy_egui::egui;
use serde_json::{Value, json};

use crate::edit::{Edit, dotted_to_pointer, escape, tokens};
use crate::form::FieldNote;
use crate::project::{Doc, Project};
use crate::project_panel::{self, Focus};

/// Which of the two weapon tables is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Table {
    #[default]
    Guns,
    Bullets,
}

impl Table {
    /// The file, relative to the mod folder.
    pub fn rel(self) -> &'static str {
        match self {
            Table::Guns => "data/weapons/guns.json",
            Table::Bullets => "data/weapons/bullets.json",
        }
    }

    /// The engine's schema file for it.
    pub fn schema_name(self) -> &'static str {
        match self {
            Table::Guns => "guns.schema.json",
            Table::Bullets => "bullets.schema.json",
        }
    }

    /// The word for one entry.
    pub fn word(self) -> &'static str {
        match self {
            Table::Guns => "gun",
            Table::Bullets => "bullet",
        }
    }

    /// The table a finding's file is.
    pub fn of_file(rel: &str) -> Option<Table> {
        [Table::Guns, Table::Bullets].into_iter().find(|t| t.rel() == rel)
    }
}

/// The schema of ONE entry of a table whose schema is a map (`additionalProperties`): the schema document with its root replaced by
/// the entry node, keeping `$defs` so `$ref`s still resolve. `Null` when the schema has no entry schema.
pub fn entry_schema(table_schema: &Value) -> Value {
    let Some(entry) = table_schema.get("additionalProperties").filter(|e| e.is_object()) else { return Value::Null };
    let mut out = serde_json::Map::new();
    for key in ["$schema", "$defs", "definitions"] {
        if let Some(v) = table_schema.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    out.extend(entry.as_object().cloned().unwrap_or_default());
    Value::Object(out)
}

/// Edits relative to the entry `name` turned into edits of the table: every pointer gets `/<escaped name>` in front.
pub fn prefix_edits(name: &str, edits: Vec<Edit>) -> Vec<Edit> {
    edits.iter().map(|e| crate::ships_panel::prefixed(name, e)).collect()
}

/// The findings (pointers into the table) that are about entry `name`, with the entry's pointer taken off the front.
pub fn entry_notes(notes: &[FieldNote], name: &str) -> Vec<FieldNote> {
    let prefix = format!("/{}", escape(name));
    notes
        .iter()
        .filter_map(|n| {
            let rest = n.pointer.strip_prefix(&prefix)?;
            (rest.is_empty() || rest.starts_with('/')).then(|| FieldNote { pointer: rest.to_string(), level: n.level, message: n.message.clone() })
        })
        .collect()
}

/// A finding's dotted path split into the entry name and the pointer inside it (`""` for the whole entry).
pub fn split_focus(path: &str) -> Option<(String, String)> {
    let ptr = dotted_to_pointer(path);
    let toks = tokens(&ptr);
    let (name, rest) = toks.split_first()?;
    Some((name.clone(), crate::edit::pointer(rest)))
}

/// One line of an entry list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryRow {
    pub name: String,
    /// The mod changes this entry against what is below it (the import), or adds it.
    pub changed: bool,
    pub added: bool,
}

/// The entries of the table document `doc`, in file order, filtered by `filter` (case-insensitive part of the name). An entry is
/// `changed` when its edited value differs from the base's, `added` when the base has none.
pub fn entry_rows(doc: &Doc, filter: &str) -> Vec<EntryRow> {
    let filter = filter.trim().to_lowercase();
    let Some(map) = doc.edited.as_object() else { return vec![] };
    map.iter()
        .filter(|(k, _)| !k.starts_with('$') && (filter.is_empty() || k.to_lowercase().contains(&filter)))
        .map(|(k, v)| {
            let base = doc.base.get(k);
            EntryRow { name: k.clone(), changed: base != Some(v), added: base.is_none() }
        })
        .collect()
}

/// The edit that adds an empty entry `name` (a new gun or bullet from scratch). Refused for an empty or existing name.
pub fn new_entry_edits(table: &Value, name: &str) -> Result<Vec<Edit>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the new entry a name.".into());
    }
    if table.get(name).is_some() {
        return Err(format!("'{name}' exists already."));
    }
    Ok(vec![Edit::set(format!("/{}", escape(name)), json!({}))])
}

/// The edit that adds `new` as a copy of the entry `from`'s values.
/// PROVISIONAL: a copy of a retail entry is retail data once saved whole (the diff writes a new key in full); the tab warns.
pub fn duplicate_edits(table: &Value, from: &str, new: &str) -> Result<Vec<Edit>, String> {
    let new = new.trim();
    let Some(value) = table.get(from) else { return Err(format!("'{from}' is not in the table.")) };
    new_entry_edits(table, new)?;
    Ok(vec![Edit::set(format!("/{}", escape(new)), value.clone())])
}

/// The keys of an entry that name a bullet (`BulletTypeName_Primary` always; any other `BulletTypeName*` the entry has).
/// PROVISIONAL: found by name prefix, since the schema does not mark references.
pub fn bullet_fields(gun: &Value) -> Vec<String> {
    let mut keys = vec!["BulletTypeName_Primary".to_string()];
    if let Some(m) = gun.as_object() {
        keys.extend(m.keys().filter(|k| k.starts_with("BulletTypeName") && *k != "BulletTypeName_Primary").cloned());
    }
    keys
}

/// The pointers (inside the entry) of the fields the tab draws itself as pickers, so the form leaves them out: a gun's bullet fields.
pub fn hidden_fields(table: Table, entry: &Value) -> Vec<String> {
    if table == Table::Guns { bullet_fields(entry).iter().map(|k| format!("/{}", escape(k))).collect() } else { vec![] }
}

/// The edit that sets bullet field `key` of gun `gun` to `bullet`.
pub fn bullet_pick_edit(gun: &str, key: &str, bullet: &str) -> Edit {
    Edit::set(format!("/{}/{}", escape(gun), escape(key)), json!(bullet))
}

/// The warning shown when `from` is duplicated: a copy of a retail entry is a new key, which a mod saves whole, so the mod would carry
/// the game's data. `None` for an entry the import does not have (the mod's own). PROVISIONAL (the engine has no way to copy an entry
/// by reference).
pub fn duplicate_warning(doc: &Doc, from: &str) -> Option<String> {
    doc.base.get(from).map(|_| format!("{from} is a retail entry: its copy is saved whole, so the mod would carry the game's data. Change what you need and delete what you keep unchanged, or start from scratch (PROVISIONAL)."))
}

/// The tab's own state.
#[derive(Resource, Default)]
pub struct State {
    pub table: Table,
    pub selected: Option<String>,
    filter: String,
    new_names: [String; 2],
    status: String,
    focus: Option<String>,
    schemas: Option<[Value; 2]>,
}

impl State {
    /// The name box of the table that is shown: guns and bullets each keep their own text.
    pub fn new_name(&mut self) -> &mut String {
        &mut self.new_names[self.table as usize]
    }

    /// A click in the problems list: selects the entry and sets the field to focus. Returns whether the file is a weapon table.
    pub fn take_focus(&mut self, focus: &Focus) -> bool {
        let Some(table) = Table::of_file(&focus.rel) else { return false };
        self.table = table;
        match split_focus(&focus.path) {
            Some((name, rest)) => {
                self.selected = Some(name);
                self.focus = Some(rest);
            }
            None => self.focus = None,
        }
        true
    }
}

/// Draws the Weapons tab.
pub fn tab(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State, focus: &mut Focus, import: Option<&std::path::Path>) {
    ui.heading("Weapons");
    if Table::of_file(&focus.rel).is_some() && st.take_focus(focus) {
        *focus = Focus::default();
    }
    let schemas = st.schemas.get_or_insert_with(|| [Table::Guns, Table::Bullets].map(|t| entry_schema(&project_panel::schema_named(t.schema_name())))).clone();
    if ps.project.is_none() {
        retail(ui, st, ps, import, &schemas);
        return;
    }
    let Some(project) = &mut ps.project else { return };
    ui.horizontal(|ui| {
        ui.selectable_value(&mut st.table, Table::Guns, "Guns");
        ui.selectable_value(&mut st.table, Table::Bullets, "Bullets");
    });
    let table = st.table;
    let rel = table.rel();
    let i = match project.open_doc(rel) {
        Ok(i) => i,
        Err(e) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
            return;
        }
    };
    let bullets: Vec<String> = match project.open_doc(Table::Bullets.rel()) {
        Ok(b) => project.docs[b].edited.as_object().map(|m| m.keys().filter(|k| !k.starts_with('$')).cloned().collect()).unwrap_or_default(),
        Err(_) => vec![],
    };
    let notes = project_panel::notes_for(&ps.findings, rel);
    ui.label(format!("{rel}{}", if project.docs[i].dirty { " (unsaved)" } else { "" }));
    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.text_edit_singleline(&mut st.filter);
    });
    let rows = entry_rows(&project.docs[i], &st.filter);
    egui::ScrollArea::vertical().id_salt("weapon-list").max_height(crate::ships_panel::list_height(ui.ctx().content_rect().height())).show(ui, |ui| {
        for r in &rows {
            let mark = if r.added {
                " [added]"
            } else if r.changed {
                " [changed]"
            } else {
                ""
            };
            if ui.selectable_label(st.selected.as_deref() == Some(r.name.as_str()), format!("{}{mark}", r.name)).clicked() {
                st.selected = Some(r.name.clone());
                st.focus = None;
            }
        }
    });
    let mut edits: Vec<Edit> = vec![];
    let mut label = String::new();
    let mut revert: Option<Option<String>> = None;
    ui.horizontal(|ui| {
        ui.label(format!("New {}", table.word()));
        ui.text_edit_singleline(st.new_name());
        if ui.button(format!("New {} from scratch", table.word())).clicked() {
            match new_entry_edits(&project.docs[i].edited, &st.new_names[table as usize]) {
                Ok(e) => {
                    edits = e;
                    label = format!("New {} {}", table.word(), st.new_names[table as usize].trim());
                    st.selected = Some(st.new_names[table as usize].trim().to_string());
                }
                Err(e) => st.status = e,
            }
        }
        if ui.add_enabled(st.selected.is_some(), egui::Button::new("Duplicate selected under this name")).clicked()
            && let Some(from) = st.selected.clone()
        {
            match duplicate_edits(&project.docs[i].edited, &from, &st.new_names[table as usize]) {
                Ok(e) => {
                    edits = e;
                    label = format!("Duplicate {from} as {}", st.new_names[table as usize].trim());
                    st.status = duplicate_warning(&project.docs[i], &from).unwrap_or_default();
                    st.selected = Some(st.new_names[table as usize].trim().to_string());
                }
                Err(e) => st.status = e,
            }
        }
    });
    ui.weak(format!(
        "A new {} from scratch needs every field the game requires before it loads (the problems list names what is missing; PROVISIONAL: the form does not mark required fields).",
        table.word()
    ));
    if !st.status.is_empty() {
        ui.label(&st.status);
    }
    ui.separator();
    if let Some(name) = st.selected.clone().filter(|n| project.docs[i].edited.get(n).is_some()) {
        let doc = &project.docs[i];
        ui.strong(&name);
        let entry = &doc.edited[&name];
        let pointer_of_entry = format!("/{}", escape(&name));
        ui.horizontal(|ui| {
            if doc.base.get(&name).is_some_and(|b| b != entry)
                && ui
                    .button(format!("Revert {name} to retail"))
                    .on_hover_text("Put this entry back to the game's values (one undo step); the mod then holds no change of it")
                    .clicked()
            {
                revert = Some(Some(pointer_of_entry.clone()));
            }
            if doc.can_revert()
                && ui
                    .button("Revert file to retail")
                    .on_hover_text("Take every change of the mod in this table back (one undo step); saving deletes the file when nothing is left")
                    .clicked()
            {
                revert = Some(None);
            }
        });
        if table == Table::Guns {
            for key in bullet_fields(entry) {
                let current = entry.get(&key).and_then(Value::as_str).unwrap_or("");
                ui.horizontal(|ui| {
                    ui.label(&key);
                    egui::ComboBox::from_id_salt(("bullet-pick", &name, &key)).selected_text(if current.is_empty() { "(none)" } else { current }).show_ui(ui, |ui| {
                        for b in &bullets {
                            if ui.selectable_label(b == current, b).clicked() && b != current {
                                edits = vec![bullet_pick_edit(&name, &key, b)];
                                label = format!("Edit {name}");
                            }
                        }
                    });
                });
            }
        }
        let pointer = st.focus.take();
        let inherited = doc.base.get(&name);
        let hidden = hidden_fields(table, entry);
        let ctx = crate::form::FormCtx { base: inherited, findings: &entry_notes(&notes, &name), focus: pointer.as_deref(), hidden: &hidden };
        let schema = &schemas[table as usize];
        let shown = crate::form::show(ui, schema, entry, &ctx, egui::Id::new(("weapon form", rel, &name)));
        if !shown.is_empty() {
            edits = prefix_edits(&name, shown);
            label = format!("Edit {name}");
        }
    } else {
        ui.label(format!("Select a {}.", table.word()));
    }
    if let Some(r) = revert {
        let done = match r {
            Some(pointer) => project.revert_at(rel, &pointer),
            None => project.revert_file(rel),
        };
        if let Err(e) = done {
            st.status = e;
        }
    }
    if !edits.is_empty()
        && let Err(e) = project.do_edits(rel, &label, edits)
    {
        st.status = e;
    }
}

/// The tab with no mod open: the import's table, editable; the first edit asks "which mod?" (see [`crate::which_mod`]).
fn retail(ui: &mut egui::Ui, st: &mut State, ps: &mut project_panel::State, import: Option<&std::path::Path>, schemas: &[Value; 2]) {
    let Some(import) = import else {
        ui.label("No imported assets found: import your own copy of the game with the engine first.");
        return;
    };
    ui.label("No mod open: your first change asks which mod to save it in (the game's own files are never changed).");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut st.table, Table::Guns, "Guns");
        ui.selectable_value(&mut st.table, Table::Bullets, "Bullets");
    });
    let table = st.table;
    let rel = table.rel();
    let Ok(doc) = openprocyon::authoring::read_merged(import, rel) else {
        ui.label(format!("{rel} is not in the import."));
        return;
    };
    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.text_edit_singleline(&mut st.filter);
    });
    let filter = st.filter.trim().to_lowercase();
    egui::ScrollArea::vertical().id_salt("weapon-list-retail").max_height(crate::ships_panel::list_height(ui.ctx().content_rect().height())).show(ui, |ui| {
        for k in doc.as_object().into_iter().flat_map(|m| m.keys()).filter(|k| !k.starts_with('$') && (filter.is_empty() || k.to_lowercase().contains(&filter))) {
            if ui.selectable_label(st.selected.as_deref() == Some(k.as_str()), k).clicked() {
                st.selected = Some(k.clone());
                st.focus = None;
            }
        }
    });
    ui.separator();
    let Some(name) = st.selected.clone().filter(|n| doc.get(n).is_some()) else {
        ui.label(format!("Select a {}.", table.word()));
        return;
    };
    ui.strong(&name);
    let pointer = st.focus.take();
    let ctx = crate::form::FormCtx { base: None, findings: &[], focus: pointer.as_deref(), hidden: &[] };
    let shown = crate::form::show(ui, &schemas[table as usize], &doc[&name], &ctx, egui::Id::new(("weapon form retail", rel, &name)));
    if !shown.is_empty() {
        crate::which_mod::propose(ps, crate::which_mod::Pending { rel: rel.to_string(), label: format!("Edit {name}"), edits: prefix_edits(&name, shown) });
    }
}

/// The tab's edits applied through the project, for tests and scripts: `edits` relative to entry `name` of `table`.
pub fn edit_entry(project: &mut Project, table: Table, name: &str, edits: Vec<Edit>) -> Result<(), String> {
    project.do_edits(table.rel(), &format!("Edit {name}"), prefix_edits(name, edits))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::model::{Kind, Node};
    use std::path::{Path, PathBuf};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-weapons-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = tmp(name);
        let (assets, m) = (root.join("assets"), root.join("mod"));
        write(&assets, "data/weapons/guns.json", r#"{"Gun_A": {"MuzzleSpeed": 10, "BulletTypeName_Primary": "B1"}, "Gun_B": {"MuzzleSpeed": 20}}"#);
        write(&assets, "data/weapons/bullets.json", r#"{"B1": {"lifetime": 3}, "B2": {"lifetime": 4}}"#);
        std::fs::create_dir_all(&m).unwrap();
        (root, assets, m)
    }

    #[test]
    fn the_entry_schema_is_the_map_value_and_resolves_to_an_object() {
        for t in [Table::Guns, Table::Bullets] {
            let s = project_panel::schema_named(t.schema_name());
            let e = entry_schema(&s);
            assert!(matches!(Node::from_schema(&e).kind, Kind::Object { .. }), "{t:?}");
        }
        assert_eq!(entry_schema(&json!({"type": "object"})), Value::Null);
        let n = Node::from_schema(&entry_schema(&project_panel::schema_named("guns.schema.json")));
        assert!(n.at("/MuzzleSpeed").is_some() || n.at("/BulletTypeName_Primary").is_some());
    }

    #[test]
    fn pointers_are_prefixed_with_the_escaped_entry_name() {
        let edits = prefix_edits("Gun/A~1", vec![Edit::set("/range", json!(5)), Edit::set("", json!({}))]);
        assert_eq!(edits[0].pointer, "/Gun~1A~01/range");
        assert_eq!(edits[1].pointer, "/Gun~1A~01");
        let mut doc = json!({"Gun/A~1": {"range": 1}});
        crate::edit::apply(&mut doc, &edits[0]).unwrap();
        assert_eq!(doc["Gun/A~1"]["range"], 5);
        let notes = vec![
            FieldNote { pointer: "/Gun~1A~01/range".into(), level: openprocyon::authoring::Level::Error, message: "m".into() },
            FieldNote { pointer: "/Gun~1A~01x/range".into(), level: openprocyon::authoring::Level::Error, message: "other".into() },
        ];
        let got = entry_notes(&notes, "Gun/A~1");
        assert_eq!((got.len(), got[0].pointer.as_str()), (1, "/range"));
        assert_eq!(split_focus("Gun_A.arc_deg"), Some(("Gun_A".into(), "/arc_deg".into())));
        assert_eq!(split_focus("Gun_A"), Some(("Gun_A".into(), "".into())));
    }

    #[test]
    fn a_gun_edit_saves_a_guns_json_with_only_that_entry_and_key() {
        let (root, assets, m) = setup("save");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        edit_entry(&mut p, Table::Guns, "Gun_A", vec![Edit::set("/MuzzleSpeed", json!(12))]).unwrap();
        let report = p.save();
        assert_eq!(report.written, [Table::Guns.rel()]);
        let text = std::fs::read_to_string(m.join(Table::Guns.rel())).unwrap().replace([' ', '\n'], "");
        assert_eq!(text, r#"{"Gun_A":{"MuzzleSpeed":12}}"#);
        assert!(!m.join(Table::Bullets.rel()).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rows_mark_changed_and_added_entries_and_new_entries_are_checked() {
        let (root, assets, m) = setup("rows");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        let i = p.open_doc(Table::Guns.rel()).unwrap();
        assert!(entry_rows(&p.docs[i], "").iter().all(|r| !r.changed && !r.added));
        let table = p.docs[i].edited.clone();
        assert!(new_entry_edits(&table, " ").is_err() && new_entry_edits(&table, "Gun_A").is_err());
        p.do_edits(Table::Guns.rel(), "new", new_entry_edits(&table, "Gun_N").unwrap()).unwrap();
        p.do_edits(Table::Guns.rel(), "dup", duplicate_edits(&table, "Gun_B", "Gun_C").unwrap()).unwrap();
        edit_entry(&mut p, Table::Guns, "Gun_A", vec![Edit::set("/MuzzleSpeed", json!(11))]).unwrap();
        let rows = entry_rows(&p.docs[i], "gun");
        let got: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.changed, r.added)).collect();
        assert_eq!(got, [("Gun_A", true, false), ("Gun_B", false, false), ("Gun_N", true, true), ("Gun_C", true, true)]);
        assert_eq!(entry_rows(&p.docs[i], "_c").len(), 1);
        assert!(duplicate_edits(&table, "Nope", "X").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_bullet_picker_sets_the_gun_field() {
        let gun = json!({"BulletTypeName_Primary": "B1", "BulletTypeName_Alt": "B2", "range": 3});
        assert_eq!(bullet_fields(&gun), ["BulletTypeName_Primary", "BulletTypeName_Alt"]);
        assert_eq!(bullet_fields(&json!({})), ["BulletTypeName_Primary"]);
        let mut doc = json!({"G/1": {}});
        crate::edit::apply(&mut doc, &bullet_pick_edit("G/1", "BulletTypeName_Primary", "B2")).unwrap();
        assert_eq!(doc["G/1"]["BulletTypeName_Primary"], "B2");
    }

    #[test]
    fn focus_selects_the_table_entry_and_field() {
        let mut st = State::default();
        assert!(!st.take_focus(&Focus { rel: "ships/A/ship.json".into(), path: String::new() }));
        assert!(st.take_focus(&Focus { rel: Table::Bullets.rel().into(), path: "B1.lifetime".into() }));
        assert_eq!((st.table, st.selected.as_deref(), st.focus.as_deref()), (Table::Bullets, Some("B1"), Some("/lifetime")));
    }

    #[test]
    fn guns_and_bullets_keep_their_own_new_name_and_the_bullet_field_is_drawn_once() {
        let mut st = State::default();
        *st.new_name() = "Gun_N".into();
        st.table = Table::Bullets;
        assert_eq!(st.new_name(), "", "the bullet box does not show the gun's text");
        *st.new_name() = "B_N".into();
        st.table = Table::Guns;
        assert_eq!(st.new_name(), "Gun_N");
        let gun = json!({"BulletTypeName_Primary": "B1", "BulletTypeName_Alt": "B2", "MuzzleSpeed": 1});
        assert_eq!(hidden_fields(Table::Guns, &gun), ["/BulletTypeName_Primary", "/BulletTypeName_Alt"], "the combo boxes draw these, the form skips them");
        assert!(hidden_fields(Table::Bullets, &gun).is_empty());
        assert_eq!(hidden_fields(Table::Guns, &json!({})), ["/BulletTypeName_Primary"], "the primary picker is always there");
    }

    #[test]
    fn the_duplicate_warning_is_for_retail_entries_only() {
        let (root, assets, m) = setup("dupwarn");
        let mut p = Project::open(&m, Some(assets)).unwrap();
        edit_entry(&mut p, Table::Guns, "Gun_N", vec![Edit::set("/MuzzleSpeed", json!(1))]).unwrap();
        let doc = p.doc(Table::Guns.rel()).unwrap();
        assert!(duplicate_warning(doc, "Gun_A").unwrap().contains("retail entry"));
        assert!(duplicate_warning(doc, "Gun_N").is_none(), "the mod's own entry is no retail data");
        drop(p);
        let _ = std::fs::remove_dir_all(root);
    }
}
