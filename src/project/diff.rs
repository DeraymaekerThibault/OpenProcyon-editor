//! Save as a diff (docs/roadmap.md principle 4): the file the editor writes for a document, computed from what the file is laid over
//! (`base`), what the game should see (`edited`) and what the mod's file holds now (`own`). Pure functions on JSON values; the
//! merge itself stays the engine's (`modding::merge`: objects combine key by key, arrays and scalars replace, then `set` pointers),
//! and the tests below check every case against the engine's own overlay.

use serde_json::{Map, Value};

use super::DocKind;
use super::keyed::{self, KeyedRule, ListDiff, NoKeyed};
use crate::edit::pointer;

/// What to write for one document.
#[derive(Clone, Debug, PartialEq)]
pub struct Diff {
    /// The file's content, or `None` when there is nothing to write (the document does not exist and nothing differs from `base`).
    pub file: Option<Value>,
    /// Changes the file cannot express: a key `base` has that `edited` removed (a merge cannot delete a key).
    /// PROVISIONAL: engine requirement: a way for an extending file to remove an inherited key.
    pub warnings: Vec<String>,
}

/// The file for a document of `kind`. `parent` is the id an `Extends` file names when `own` has no `extends` of its own (`None`: the
/// file is whole, nothing is inherited, so no `extends` is written). `own` is `Value::Null` when the mod has no such file yet.
pub fn compute(kind: DocKind, base: &Value, edited: &Value, own: &Value, parent: Option<&str>) -> Diff {
    compute_with(&NoKeyed, "", kind, base, edited, own, parent)
}

/// [`compute`] for the file `rel`, with lists that merge by key ([`keyed`]) written entry by entry in an `extends` file instead of
/// whole: an entry added, deleted or changed is one diff entry; a list that cannot be written that way (an entry with no key, an
/// order the merge cannot give) is written whole with `"merge": {"<list>": false}` and a warning.
pub fn compute_with(rule: &dyn KeyedRule, rel: &str, kind: DocKind, base: &Value, edited: &Value, own: &Value, parent: Option<&str>) -> Diff {
    let mut cx = Cx { sets: vec![], warnings: vec![], allow_set: false, keyed: vec![], merge_false: vec![] };
    if edited.is_null() {
        return Diff { file: None, warnings: vec![] };
    }
    let file = match kind {
        DocKind::Replaced => Some(edited.clone()),
        DocKind::Merged => {
            let body = body(base, edited, own.as_object(), &mut cx);
            (!(body.is_empty() && own.is_null())).then_some(Value::Object(body))
        }
        DocKind::Extends => extends_file(rule, rel, base, edited, own, parent, &mut cx),
    };
    Diff { file, warnings: cx.warnings }
}

fn extends_file(rule: &dyn KeyedRule, rel: &str, base: &Value, edited: &Value, own: &Value, parent: Option<&str>, cx: &mut Cx) -> Option<Value> {
    cx.allow_set = true;
    let own_map = own.as_object();
    let own_parent = own_map.and_then(|m| m.get("extends")).and_then(Value::as_str);
    let parent = own_parent.or(parent);
    // `extends`, `merge` and `set` are written by this function; the other own keys are kept.
    let mut kept = own_map.cloned().unwrap_or_default();
    kept.shift_remove("extends");
    let own_set = kept.shift_remove("set");
    let own_merge = kept.shift_remove("merge");
    // A file that inherits nothing is not merged with anything: nothing is keyed.
    if parent.is_some() {
        cx.keyed = keyed::effective(rule, rel, own_merge.as_ref());
    }
    let body = body(base, edited, own_map.map(|_| &kept), cx);
    let mut sets: Vec<(String, Value)> = vec![];
    // Hand-written `set` entries stay while the pointer exists; their value follows the document. A pointer into a list written by
    // key (`/objects/7/pos`) is not kept: its index may now name another entry once a keyed `$remove` or an insert shifted the list.
    // The list's own diff writes the changes again: as entries, or as fresh `set` pointers computed against `base`.
    if let Some(Value::Object(set)) = &own_set {
        for (ptr, own_value) in set {
            if cx.keyed.iter().any(|(list, _)| ptr.strip_prefix(list.as_str()).is_some_and(|rest| rest.starts_with('/'))) {
                continue;
            }
            if let Some(cur) = edited.pointer(ptr) {
                // A `set` that was changed back to what the parent has is dropped.
                if base.pointer(ptr) == Some(cur) && own_value != cur {
                    continue;
                }
                sets.push((ptr.clone(), cur.clone()));
            }
        }
    }
    for (ptr, v) in std::mem::take(&mut cx.sets) {
        if !sets.iter().any(|(p, _)| *p == ptr) {
            sets.push((ptr, v));
        }
    }
    if own.is_null() && body.is_empty() && sets.is_empty() {
        return None;
    }
    let mut out = Map::new();
    if let Some(p) = parent {
        out.insert("extends".into(), Value::String(p.into()));
    }
    // The file's own `merge` choices stay; a list written whole although it merges by key is said to replace.
    let mut merge = match own_merge {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    for ptr in std::mem::take(&mut cx.merge_false) {
        merge.insert(ptr, Value::Bool(false));
    }
    if !merge.is_empty() {
        out.insert("merge".into(), Value::Object(merge));
    }
    out.extend(body);
    if !sets.is_empty() {
        out.insert("set".into(), Value::Object(sets.into_iter().collect()));
    }
    Some(Value::Object(out))
}

struct Cx {
    /// Pointer to value, for arrays that changed in a few elements (Extends only).
    sets: Vec<(String, Value)>,
    warnings: Vec<String>,
    allow_set: bool,
    /// `(pointer, key field)` of the lists written entry by entry (an `extends` file with a parent).
    keyed: Vec<(String, String)>,
    /// Keyed lists that had to be written whole: their pointers go in the file's `merge` as `false`.
    merge_false: Vec<String>,
}

impl Cx {
    fn key_field(&self, path: &[String]) -> Option<String> {
        let ptr = pointer(path);
        self.keyed.iter().find(|(p, _)| *p == ptr).map(|(_, f)| f.clone())
    }
}

/// The top-level object of a file: `edited` against `base` and the kept keys of `own`.
fn body(base: &Value, edited: &Value, own: Option<&Map<String, Value>>, cx: &mut Cx) -> Map<String, Value> {
    let Some(edited) = edited.as_object() else { return Map::new() };
    diff_obj(base.as_object(), edited, own, &mut vec![], cx)
}

/// Keys of `own` first (with the edited values; a key edited away is dropped, and warned about when `base` has it), then the keys of
/// `edited` that differ from `base`.
fn diff_obj(base: Option<&Map<String, Value>>, edited: &Map<String, Value>, own: Option<&Map<String, Value>>, path: &mut Vec<String>, cx: &mut Cx) -> Map<String, Value> {
    let mut out = Map::new();
    for key in own.into_iter().flat_map(Map::keys) {
        path.push(key.clone());
        match edited.get(key) {
            Some(ev) => {
                let (bv, ov) = (base.and_then(|b| b.get(key)), own.and_then(|o| o.get(key)));
                // A key the mod wrote that now equals what is below it was reset to the parent: it is dropped (a key that always
                // equalled the parent stays, as the author wrote it).
                if bv == Some(ev) && ov != bv {
                    path.pop();
                    continue;
                }
                if let (Some(field), Value::Array(e)) = (cx.key_field(path), ev)
                    && bv.is_none_or(Value::is_array)
                {
                    if let Some(v) = keyed_array(&field, bv, e, path, cx) {
                        out.insert(key.clone(), v);
                    }
                    path.pop();
                    continue;
                }
                let v = kept(bv, ev, ov, path, cx);
                if matches!(&v, Value::Object(m) if m.is_empty()) && bv.is_some_and(Value::is_object) && ov.is_some_and(|o| o.as_object().is_some_and(|m| !m.is_empty())) {
                    path.pop();
                    continue;
                }
                out.insert(key.clone(), v);
            }
            None if base.is_some_and(|b| b.contains_key(key)) => cx.warnings.push(removed(path)),
            None => {}
        }
        path.pop();
    }
    if let Some(base) = base {
        for key in base.keys().filter(|k| !edited.contains_key(*k) && !own.is_some_and(|o| o.contains_key(*k))) {
            path.push(key.clone());
            cx.warnings.push(removed(path));
            path.pop();
        }
    }
    for (key, ev) in edited {
        if out.contains_key(key) || own.is_some_and(|o| o.contains_key(key)) {
            continue;
        }
        let bv = base.and_then(|b| b.get(key));
        if bv == Some(ev) {
            continue;
        }
        path.push(key.clone());
        if let Some(v) = changed(bv, ev, path, cx) {
            out.insert(key.clone(), v);
        }
        path.pop();
    }
    out
}

fn removed(path: &[String]) -> String {
    format!("{} was removed, but a mod file cannot remove a key the file below it has; it stays", pointer(path))
}

/// A key the mod's file already has: written whole, except that an object keeps its own keys and drops nothing it does not need.
fn kept(base: Option<&Value>, edited: &Value, own: Option<&Value>, path: &mut Vec<String>, cx: &mut Cx) -> Value {
    match (edited, own) {
        (Value::Object(e), Some(Value::Object(o))) => Value::Object(diff_obj(base.and_then(Value::as_object), e, Some(o), path, cx)),
        _ => edited.clone(),
    }
}

/// A key the mod's file does not have yet and that differs from `base`: the value to write, or `None` when nothing is to be written
/// (only removals below it, or the array change went into `set`).
fn changed(base: Option<&Value>, edited: &Value, path: &mut Vec<String>, cx: &mut Cx) -> Option<Value> {
    match (base, edited) {
        (_, Value::Array(e)) if cx.key_field(path).is_some() && base.is_none_or(Value::is_array) => {
            let field = cx.key_field(path).expect("checked");
            keyed_array(&field, base, e, path, cx)
        }
        (Some(Value::Object(b)), Value::Object(e)) => {
            let sub = diff_obj(Some(b), e, None, path, cx);
            (!sub.is_empty()).then_some(Value::Object(sub))
        }
        (Some(Value::Array(b)), Value::Array(e)) if cx.allow_set && b.len() == e.len() => {
            let mut found = vec![];
            collect_sets(b, e, path, &mut found);
            let whole = edited.to_string().len();
            let set_size: usize = found.iter().map(|(p, v)| p.len() + v.to_string().len() + 4).sum();
            if !found.is_empty() && set_size < whole {
                cx.sets.extend(found);
                None
            } else {
                Some(edited.clone())
            }
        }
        _ => Some(edited.clone()),
    }
}

/// A list that merges by key, changed: what to write for it (`None`: nothing, or `set` pointers). Changed, added or deleted entries
/// become diff entries ([`keyed::diff_list`]); a
/// list that cannot be said that way is written whole, with `merge: false` and a warning (unless it has the same length, where
/// `set` pointers say what changed).
fn keyed_array(field: &str, base: Option<&Value>, edited: &[Value], path: &mut Vec<String>, cx: &mut Cx) -> Option<Value> {
    let ptr = pointer(path);
    let b: &[Value] = base.and_then(Value::as_array).map_or(&[], Vec::as_slice);
    match keyed::diff_list(field, b, edited, &ptr) {
        ListDiff::Equal => None,
        // The same entries in the same order, some changed: still keyed entries (a list the file already writes by key must not turn
        // into index pointers, whose index may name another entry after the parent changes).
        ListDiff::SameKeys { entries, warnings } | ListDiff::Entries { entries, warnings } => {
            cx.warnings.extend(warnings);
            (!entries.is_empty()).then_some(Value::Array(entries))
        }
        ListDiff::Unkeyable(_) if base.is_some() && b.len() == edited.len() => {
            // Entries changed in place: `set` pointers say it without the keys.
            let mut found = vec![];
            collect_sets(b, edited, path, &mut found);
            cx.sets.extend(found);
            None
        }
        ListDiff::Unkeyable(why) => {
            cx.warnings.push(format!("{ptr} is written whole ({why}): the file replaces the list below instead of changing entries"));
            cx.merge_false.push(ptr);
            Some(Value::Array(edited.to_vec()))
        }
    }
}

/// The `set` entries that turn array `base` into `edited` (the same length): the deepest pointers that exist in `base`.
fn collect_sets(base: &[Value], edited: &[Value], path: &mut Vec<String>, out: &mut Vec<(String, Value)>) {
    for (i, (b, e)) in base.iter().zip(edited).enumerate() {
        if b == e {
            continue;
        }
        path.push(i.to_string());
        collect_value(b, e, path, out);
        path.pop();
    }
}

fn collect_value(b: &Value, e: &Value, path: &mut Vec<String>, out: &mut Vec<(String, Value)>) {
    match (b, e) {
        // The same keys: change the values that differ. Any other change of an object (a key added or removed) replaces it whole,
        // since a `set` pointer must exist and cannot remove anything.
        (Value::Object(bo), Value::Object(eo)) if bo.len() == eo.len() && bo.keys().all(|k| eo.contains_key(k)) => {
            for (k, ev) in eo {
                if bo[k] != *ev {
                    path.push(k.clone());
                    collect_value(&bo[k], ev, path, out);
                    path.pop();
                }
            }
        }
        (Value::Array(ba), Value::Array(ea)) if ba.len() == ea.len() => collect_sets(ba, ea, path, out),
        _ => out.push((pointer(path), e.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openprocyon::authoring::{overlay_into, read_merged, to_json_string};
    use serde_json::json;
    use std::path::PathBuf;

    /// Lays `file` over `base` as the mod `rel` with the engine's own merge and reads `rel` back.
    fn engine_result(rel: &str, base: &Value, file: &Value, tag: &str) -> Value {
        let dir: PathBuf = std::env::temp_dir().join(format!("procyon-editor-diff-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (assets, m, out) = (dir.join("assets"), dir.join("mod"), dir.join("out"));
        for (root, v) in [(&assets, base), (&m, file)] {
            if v.is_null() {
                continue;
            }
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, to_json_string(v)).unwrap();
        }
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::create_dir_all(&m).unwrap();
        let report = overlay_into(&assets, std::slice::from_ref(&m), &out).unwrap();
        assert!(report.mods[0].skipped.is_empty(), "{:?}", report.mods[0].skipped);
        let merged = read_merged(&out, rel).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        merged
    }

    fn ship() -> Value {
        json!({
            "name": "Cutter", "speed": 3, "hull": {"hp": 100, "armour": 5, "layer": "Hull"},
            "engines": [{"fx": "A", "pos": [0, 1, 2]}, {"fx": "A", "pos": [1, 1, 2]}, {"fx": "A", "pos": [2, 1, 2]}, {"fx": "A", "pos": [3, 1, 2]}],
            "flags": ["a", "b", "c"]
        })
    }
    const REL: &str = "ships/Ship_X/ship.json";

    fn extends(base: &Value, edited: &Value, own: &Value, tag: &str) -> (Value, Vec<String>) {
        let d = compute(DocKind::Extends, base, edited, own, Some("Ship_X"));
        let file = d.file.expect("a file");
        assert_eq!(engine_result(REL, base, &file, tag), *edited, "the engine merges {file} back to the edited document");
        (file, d.warnings)
    }

    #[test]
    fn extends_writes_changed_scalar_nested_and_new_keys_only() {
        let base = ship();
        let mut edited = base.clone();
        edited["speed"] = json!(4);
        edited["hull"]["hp"] = json!(150);
        edited["added"] = json!({"x": 1});
        let (file, warn) = extends(&base, &edited, &Value::Null, "scalar");
        assert_eq!(file, json!({"extends": "Ship_X", "speed": 4, "hull": {"hp": 150}, "added": {"x": 1}}));
        assert!(warn.is_empty());
        assert!(file.get("name").is_none(), "an unchanged key is left out");
    }

    #[test]
    fn nothing_changed_and_no_file_means_nothing_to_write() {
        let d = compute(DocKind::Extends, &ship(), &ship(), &Value::Null, Some("Ship_X"));
        assert_eq!(d.file, None);
        let d = compute(DocKind::Merged, &ship(), &ship(), &Value::Null, None);
        assert_eq!(d.file, None);
    }

    #[test]
    fn own_keys_stay_even_when_equal_to_base_and_in_their_order() {
        let base = ship();
        let own = json!({"extends": "Ship_X", "hull": {"layer": "Hull", "hp": 120}, "note": "hand written", "speed": 3});
        let mut edited = base.clone();
        edited["hull"]["hp"] = json!(120);
        edited["note"] = json!("hand written");
        edited["speed"] = json!(3);
        edited["name"] = json!("Renamed");
        let (file, _) = extends(&base, &edited, &own, "own");
        assert_eq!(file, json!({"extends": "Ship_X", "hull": {"layer": "Hull", "hp": 120}, "note": "hand written", "speed": 3, "name": "Renamed"}));
        assert_eq!(file.as_object().unwrap().keys().collect::<Vec<_>>(), ["extends", "hull", "note", "speed", "name"]);
        // equal in meaning to the file on disk: the caller sees no change
        edited["name"] = json!("Cutter");
        let (again, _) = extends(&base, &edited, &own, "own2");
        assert_eq!(again, own);
    }

    #[test]
    fn a_parent_of_the_files_own_beats_the_given_one() {
        let own = json!({"extends": "Other", "speed": 9});
        let mut edited = ship();
        edited["speed"] = json!(9);
        let d = compute(DocKind::Extends, &ship(), &edited, &own, Some("Ship_X"));
        assert_eq!(d.file.unwrap()["extends"], "Other");
    }

    #[test]
    fn a_few_changed_array_elements_become_set_pointers() {
        let base = ship();
        let mut edited = base.clone();
        edited["engines"][1]["fx"] = json!("B");
        edited["engines"][3]["pos"][2] = json!(9);
        let (file, _) = extends(&base, &edited, &Value::Null, "set");
        assert_eq!(file, json!({"extends": "Ship_X", "set": {"/engines/1/fx": "B", "/engines/3/pos/2": 9}}));
    }

    #[test]
    fn an_array_is_written_whole_when_that_is_shorter_or_its_length_changed() {
        let base = ship();
        // every element changes: the whole list is shorter than the pointers
        let mut edited = base.clone();
        for e in edited["flags"].as_array_mut().unwrap() {
            *e = json!("z");
        }
        let (file, _) = extends(&base, &edited, &Value::Null, "whole");
        assert_eq!(file, json!({"extends": "Ship_X", "flags": ["z", "z", "z"]}));
        // a different length
        let mut edited = base.clone();
        edited["flags"].as_array_mut().unwrap().push(json!("d"));
        let (file, _) = extends(&base, &edited, &Value::Null, "len");
        assert_eq!(file, json!({"extends": "Ship_X", "flags": ["a", "b", "c", "d"]}));
        // an element that gains a key is replaced whole (a pointer cannot create it)
        let mut edited = base.clone();
        edited["engines"][0]["extra"] = json!(true);
        let (file, _) = extends(&base, &edited, &Value::Null, "elem");
        assert_eq!(file["set"], json!({"/engines/0": {"fx": "A", "pos": [0, 1, 2], "extra": true}}));
    }

    #[test]
    fn hand_written_set_entries_stay_and_follow_the_document() {
        let base = ship();
        let own = json!({"extends": "Ship_X", "set": {"/engines/0/fx": "X", "/engines/9/fx": "gone"}});
        let mut edited = base.clone();
        edited["engines"][0]["fx"] = json!("Y");
        edited["engines"][2]["fx"] = json!("Z");
        let d = compute(DocKind::Extends, &base, &edited, &own, Some("Ship_X"));
        let file = d.file.unwrap();
        assert_eq!(file["set"], json!({"/engines/0/fx": "Y", "/engines/2/fx": "Z"}));
        assert_eq!(engine_result(REL, &base, &file, "setown"), edited);
    }

    #[test]
    fn a_removed_inherited_key_is_a_warning_and_stays_out_of_the_file() {
        let base = ship();
        let mut edited = base.clone();
        edited["hull"].as_object_mut().unwrap().shift_remove("armour");
        edited.as_object_mut().unwrap().shift_remove("speed");
        edited["name"] = json!("N");
        let d = compute(DocKind::Extends, &base, &edited, &Value::Null, Some("Ship_X"));
        assert_eq!(d.file, Some(json!({"extends": "Ship_X", "name": "N"})));
        assert_eq!(d.warnings.len(), 2);
        assert!(d.warnings.iter().any(|w| w.starts_with("/hull/armour")) && d.warnings.iter().any(|w| w.starts_with("/speed")), "{:?}", d.warnings);
        // a key only the mod added can be removed again without a warning
        let own = json!({"extends": "Ship_X", "mine": 1});
        let d = compute(DocKind::Extends, &base, &base, &own, Some("Ship_X"));
        assert!(d.warnings.is_empty());
        assert_eq!(d.file, Some(json!({"extends": "Ship_X"})));
    }

    #[test]
    fn a_whole_file_with_no_parent_is_written_whole() {
        let edited = ship();
        let d = compute(DocKind::Extends, &Value::Null, &edited, &Value::Null, None);
        assert_eq!(d.file, Some(edited));
    }

    #[test]
    fn merged_files_get_changed_keys_only_with_arrays_whole() {
        let rel = "data/weapons/guns.json";
        let base = json!({"Gun_A": {"range": 10, "arc": [1, 2, 3], "fx": "F"}, "Gun_B": {"range": 20}});
        let mut edited = base.clone();
        edited["Gun_A"]["range"] = json!(12);
        edited["Gun_A"]["arc"][1] = json!(5);
        edited["Gun_C"] = json!({"range": 1});
        let d = compute(DocKind::Merged, &base, &edited, &Value::Null, Some("ignored"));
        let file = d.file.unwrap();
        assert_eq!(file, json!({"Gun_A": {"range": 12, "arc": [1, 5, 3]}, "Gun_C": {"range": 1}}));
        assert!(file.get("extends").is_none() && file.get("set").is_none());
        assert_eq!(engine_result(rel, &base, &file, "merged"), edited);
        // an own key stays (equal to base or not)
        let own = json!({"Gun_B": {"range": 20}});
        let d = compute(DocKind::Merged, &base, &edited, &own, None);
        let file = d.file.unwrap();
        assert_eq!(file["Gun_B"], json!({"range": 20}));
        assert_eq!(engine_result(rel, &base, &file, "merged2"), edited);
        // a merged file the mod does not have and nothing changed
        assert_eq!(compute(DocKind::Merged, &base, &base, &Value::Null, None).file, None);
    }

    #[test]
    fn replaced_files_are_written_whole() {
        let rel = "data/render/look.json";
        let base = json!({"a": 1, "b": {"c": 2}});
        let edited = json!({"a": 1, "b": {"c": 3}});
        let d = compute(DocKind::Replaced, &base, &edited, &Value::Null, None);
        assert_eq!(d.file, Some(edited.clone()));
        assert_eq!(engine_result(rel, &base, d.file.as_ref().unwrap(), "replaced"), edited);
    }

    #[test]
    fn a_key_reset_to_the_parent_is_dropped_from_the_file_but_one_that_always_matched_stays() {
        let base = ship();
        // the mod's file overrides speed and hull.hp, repeats `name` (equal to the parent), adds `extra` and sets an engine
        let own = json!({"extends": "Ship_X", "name": "Cutter", "speed": 5, "hull": {"hp": 90, "armour": 6}, "extra": 1, "set": {"/engines/1/fx": "B"}});
        let mut edited = base.clone();
        edited["hull"]["armour"] = json!(6);
        // speed and hull.hp are back at the parent's values, `extra` was removed, the engine is back
        let (file, warnings) = extends(&base, &edited, &own, "reset");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(file, json!({"extends": "Ship_X", "name": "Cutter", "hull": {"armour": 6}}), "only what matched all along stays");
        // nothing differs from the parent any more: only `extends` remains
        let own = json!({"extends": "Ship_X", "speed": 5, "hull": {"hp": 90}, "set": {"/engines/1/fx": "B"}});
        let (file, _) = extends(&base, &base, &own, "reset-all");
        assert_eq!(file, json!({"extends": "Ship_X"}));
    }

    #[test]
    fn a_merged_entry_reset_to_the_parent_is_dropped() {
        let base = json!({"Gun_A": {"range": 10, "dmg": 1}, "Gun_B": {"range": 1}});
        let own = json!({"Gun_A": {"range": 12}, "Gun_B": {"range": 2}});
        let mut edited = base.clone();
        edited["Gun_B"]["range"] = json!(2);
        let d = compute(DocKind::Merged, &base, &edited, &own, None);
        assert_eq!(d.file, Some(json!({"Gun_B": {"range": 2}})), "Gun_A is back at retail: no entry of it remains");
        let d = compute(DocKind::Merged, &base, &base, &own, None);
        assert_eq!(d.file, Some(json!({})), "an empty file: the project deletes it");
    }

    // ---- keyed lists (T4): checked against the engine's own merge ([`keyed::engine_extend`]) ----

    const MAP: &str = "maps/red/map.json";

    fn red() -> Value {
        json!({
            "name": "Red",
            "objects": [{"id": 1, "type": "A", "pos": [0, 0, 0]}, {"id": 2, "type": "B", "pos": [5, 5, 0]}, {"id": 3, "type": "C", "pos": [9, 9, 0]}],
            "players": [{"name": "P1", "start": [1, 1, 0]}, {"name": "P2", "start": [2, 2, 0]}],
            "teams": [{"name_id": "T1"}]
        })
    }

    fn keyed_file(base: &Value, edited: &Value, own: &Value) -> Diff {
        let d = compute_with(&keyed::EngineRule, MAP, DocKind::Extends, base, edited, own, Some("red"));
        let file = d.file.clone().expect("a file");
        let (merged, _) = keyed::engine_extend(MAP, base, &file).unwrap();
        assert_eq!(merged, *edited, "the rule merges {file} back to the edited document");
        d
    }

    #[test]
    fn keyed_lists_write_added_deleted_and_changed_entries_only() {
        let base = red();
        let mut edited = base.clone();
        edited["objects"].as_array_mut().unwrap().remove(1);
        edited["objects"][1]["pos"] = json!([10, 10, 0]);
        edited["objects"].as_array_mut().unwrap().push(json!({"id": 4, "type": "D", "pos": [1, 2, 3]}));
        edited["players"][0]["start"][0] = json!(7);
        let d = keyed_file(&base, &edited, &Value::Null);
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        assert_eq!(
            d.file.unwrap(),
            json!({
                "extends": "red",
                "objects": [{"id": 2, "$remove": true}, {"id": 3, "pos": [10, 10, 0]}, {"id": 4, "type": "D", "pos": [1, 2, 3]}],
                "players": [{"name": "P1", "start": [7, 1, 0]}]
            }),
            "a change in place is a keyed entry too"
        );
    }

    #[test]
    fn keyed_lists_same_keys_stay_keyed_entries() {
        let base = red();
        let mut edited = base.clone();
        edited["objects"][2]["pos"][1] = json!(8);
        let d = keyed_file(&base, &edited, &Value::Null);
        assert_eq!(d.file.unwrap(), json!({"extends": "red", "objects": [{"id": 3, "pos": [9, 8, 0]}]}));
        // an entry that gains a field: only the field is written
        let mut edited = base.clone();
        edited["objects"][0] = json!({"id": 1, "type": "Z", "pos": [4, 4, 4], "scene": "S"});
        let d = keyed_file(&base, &edited, &Value::Null);
        assert_eq!(d.file.unwrap(), json!({"extends": "red", "objects": [{"id": 1, "type": "Z", "pos": [4, 4, 4], "scene": "S"}]}));
    }

    #[test]
    fn keyed_lists_a_height_change_writes_a_keyed_entry_with_the_other_values_untouched() {
        let mut base = red();
        base["objects"][1]["pos"] = json!([1077.426548, 12.5, 0.0]);
        let mut edited = base.clone();
        edited["objects"][1]["pos"][2] = json!(110.0);
        let d = keyed_file(&base, &edited, &Value::Null);
        let file = d.file.unwrap();
        assert!(file.get("set").is_none(), "{file}");
        assert_eq!(file["objects"], json!([{"id": 2, "pos": [1077.426548, 12.5, 110.0]}]));
    }

    #[test]
    fn keyed_lists_an_existing_keyed_rule_edit_stays_keyed_after_an_unrelated_change() {
        const SCRIPT: &str = "maps/mission10/script.json";
        let base = json!({"name": "m10", "rules": [{"name": "start", "Run Once": true}, {"name": "setup rules", "Run Once": true}], "players": [{"name": "A"}]});
        let own = json!({"extends": "mission10", "rules": [{"name": "setup rules", "Run Once": false}]});
        let (mut edited, _) = keyed::engine_extend(SCRIPT, &base, &own).unwrap();
        assert_eq!(edited["rules"][1]["Run Once"], json!(false));
        edited["name"] = json!("m10b");
        let d = compute_with(&keyed::EngineRule, SCRIPT, DocKind::Extends, &base, &edited, &own, Some("mission10"));
        let file = d.file.unwrap();
        assert!(file.get("set").is_none(), "{file}");
        assert_eq!(file["rules"], json!([{"name": "setup rules", "Run Once": false}]));
        let (merged, _) = keyed::engine_extend(SCRIPT, &base, &file).unwrap();
        assert_eq!(merged, edited);
    }

    #[test]
    fn keyed_lists_nothing_changed_writes_nothing_and_a_list_new_to_the_parent_is_its_entries() {
        let base = red();
        assert_eq!(compute_with(&keyed::EngineRule, MAP, DocKind::Extends, &base, &base, &Value::Null, Some("red")).file, None);
        let mut edited = base.clone();
        edited["ships"] = json!([{"id": 9, "type": "S"}]);
        let d = keyed_file(&base, &edited, &Value::Null);
        assert_eq!(d.file.unwrap(), json!({"extends": "red", "ships": [{"id": 9, "type": "S"}]}));
    }

    #[test]
    fn keyed_lists_a_list_that_cannot_be_keyed_is_written_whole_and_said_to_replace() {
        let mut base = red();
        base["objects"].as_array_mut().unwrap().push(json!({"type": "Decor", "pos": [0, 0, 0]}));
        let mut edited = base.clone();
        edited["objects"].as_array_mut().unwrap().remove(0);
        let d = compute_with(&keyed::EngineRule, MAP, DocKind::Extends, &base, &edited, &Value::Null, Some("red"));
        let file = d.file.unwrap();
        assert_eq!(file["merge"], json!({"/objects": false}));
        assert_eq!(file["objects"], edited["objects"]);
        assert_eq!(d.warnings.len(), 1);
        assert!(d.warnings[0].starts_with("/objects is written whole"), "{:?}", d.warnings);
        let (merged, _) = keyed::engine_extend(MAP, &base, &file).unwrap();
        assert_eq!(merged, edited);
        // a moved decoration in the same list is a set pointer, not a restated list
        let mut edited = red();
        edited["objects"][0]["pos"][0] = json!(3);
        let mut base = red();
        base["objects"].as_array_mut().unwrap().push(json!({"type": "Decor"}));
        edited["objects"].as_array_mut().unwrap().push(json!({"type": "Decor"}));
        let d = compute_with(&keyed::EngineRule, MAP, DocKind::Extends, &base, &edited, &Value::Null, Some("red"));
        assert_eq!(d.file.unwrap(), json!({"extends": "red", "set": {"/objects/0/pos/0": 3}}));
    }

    #[test]
    fn keyed_lists_a_file_with_no_parent_and_a_file_below_a_non_keyed_rule_are_written_as_before() {
        let edited = red();
        let d = compute_with(&keyed::EngineRule, MAP, DocKind::Extends, &Value::Null, &edited, &Value::Null, None);
        assert_eq!(d.file, Some(edited.clone()), "a whole map has no merge to speak of");
        let mut with_extra = red();
        with_extra["objects"].as_array_mut().unwrap().push(json!({"id": 4}));
        let d = compute_with(&keyed::EngineRule, "ships/X/ship.json", DocKind::Extends, &red(), &with_extra, &Value::Null, Some("X"));
        assert_eq!(d.file.unwrap()["objects"].as_array().unwrap().len(), 4, "another file: the list is written whole");
        let d = compute(DocKind::Extends, &red(), &with_extra, &Value::Null, Some("red"));
        assert_eq!(d.file.unwrap()["objects"].as_array().unwrap().len(), 4, "compute() keys nothing");
    }

    #[test]
    fn keyed_lists_the_files_own_entries_and_merge_choices_stay() {
        let base = red();
        // own file: one hand-written entry change and a merge override of another list
        let own = json!({"extends": "red", "merge": {"/teams": "name_id"}, "objects": [{"id": 2, "type": "Bee"}], "teams": [{"name_id": "T2"}]});
        let (mut edited, _) = keyed::engine_extend(MAP, &base, &own).unwrap();
        assert_eq!(edited["teams"], json!([{"name_id": "T1"}, {"name_id": "T2"}]));
        edited["objects"][2]["pos"] = json!([0, 0, 1]);
        edited["objects"].as_array_mut().unwrap().remove(0);
        let d = keyed_file(&base, &edited, &own);
        let file = d.file.unwrap();
        assert_eq!(file["merge"], json!({"/teams": "name_id"}));
        assert_eq!(file["teams"], json!([{"name_id": "T2"}]), "the team the mod added is still its own entry");
        assert_eq!(file["objects"], json!([{"id": 1, "$remove": true}, {"id": 2, "type": "Bee"}, {"id": 3, "pos": [0, 0, 1]}]));
        // a list the file replaces whole by choice is written whole as before
        let own = json!({"extends": "red", "merge": {"/objects": false}, "objects": [{"id": 1}]});
        let mut edited = keyed::engine_extend(MAP, &base, &own).unwrap().0;
        edited["objects"][0]["pos"] = json!([1, 1, 1]);
        let d = compute_with(&keyed::EngineRule, MAP, DocKind::Extends, &base, &edited, &own, Some("red"));
        assert_eq!(d.file.unwrap(), json!({"extends": "red", "merge": {"/objects": false}, "objects": [{"id": 1, "pos": [1, 1, 1]}]}));
        // everything back at the parent's: the entries go, the merge choice stays
        let own = json!({"extends": "red", "objects": [{"id": 2, "type": "Bee"}]});
        let d = compute_with(&keyed::EngineRule, MAP, DocKind::Extends, &base, &base, &own, Some("red"));
        assert_eq!(d.file, Some(json!({"extends": "red"})));
    }

    #[test]
    fn keyed_lists_earlier_set_pointers_do_not_follow_shifted_indices() {
        // Save 1: a move (set pointers). Save 2: delete a lower-index object, add one, duplicate one.
        let base = red();
        let mut saved = base.clone();
        saved["objects"][2]["pos"] = json!([1, 1, 1]);
        saved["objects"][1]["pos"] = json!([2, 2, 2]);
        let first = keyed_file(&base, &saved, &Value::Null).file.unwrap();
        assert!(first.get("set").is_none() && first["objects"].as_array().unwrap().len() == 2, "{first}");
        let mut edited = saved.clone();
        edited["objects"].as_array_mut().unwrap().remove(0);
        edited["objects"].as_array_mut().unwrap().push(json!({"id": 4, "type": "D", "pos": [1, 2, 3]}));
        let mut dup = edited["objects"][1].clone();
        dup["id"] = json!(5);
        edited["objects"].as_array_mut().unwrap().push(dup);
        let d = keyed_file(&base, &edited, &first);
        let file = d.file.unwrap();
        let (merged, _) = keyed::engine_extend(MAP, &base, &file).unwrap();
        assert_eq!(merged, edited);
        // No pointer is left that names the position of an entry the edit moved.
        assert!(file.get("set").is_none_or(|s| s.as_object().is_some_and(|m| m.keys().all(|k| !k.starts_with("/objects/")))), "{file}");
    }

    #[test]
    fn keyed_lists_mission_rules_by_name_do_not_keep_stale_set_pointers() {
        const SCRIPT: &str = "maps/red/script.json";
        let base = json!({"rules": [{"name": "A", "do": 1}, {"name": "B", "do": 2}, {"name": "C", "do": 3}]});
        let mut saved = base.clone();
        saved["rules"][2]["do"] = json!(30);
        let d = compute_with(&keyed::EngineRule, SCRIPT, DocKind::Extends, &base, &saved, &Value::Null, Some("red"));
        let first = d.file.unwrap();
        assert_eq!(first["rules"], json!([{"name": "C", "do": 30}]), "{first}");
        let mut edited = saved.clone();
        edited["rules"].as_array_mut().unwrap().remove(0);
        edited["rules"].as_array_mut().unwrap().push(json!({"name": "D", "do": 4}));
        let d = compute_with(&keyed::EngineRule, SCRIPT, DocKind::Extends, &base, &edited, &first, Some("red"));
        let file = d.file.unwrap();
        let (merged, _) = keyed::engine_extend(SCRIPT, &base, &file).unwrap();
        assert_eq!(merged, edited, "{file}");
        assert!(file.get("set").is_none(), "{file}");
    }

    #[test]
    fn keyed_lists_follow_any_rule_the_trait_gives() {
        struct Fake;
        impl KeyedRule for Fake {
            fn lists(&self, rel: &str) -> Vec<(String, String)> {
                (rel == "x.json").then(|| ("/items".to_string(), "k".to_string())).into_iter().collect()
            }
        }
        let base = json!({"items": [{"k": "a", "v": 1}, {"k": "b", "v": 2}]});
        let edited = json!({"items": [{"k": "a", "v": 1}, {"k": "b", "v": 3}, {"k": "c"}]});
        let d = compute_with(&Fake, "x.json", DocKind::Extends, &base, &edited, &Value::Null, Some("p"));
        let file = d.file.unwrap();
        assert_eq!(file, json!({"extends": "p", "items": [{"k": "b", "v": 3}, {"k": "c"}]}));
        let d = compute_with(&Fake, "y.json", DocKind::Extends, &base, &edited, &Value::Null, Some("p"));
        assert_eq!(d.file.unwrap()["items"], edited["items"], "another file: whole list");
    }
}
