//! Save as a diff (docs/roadmap.md principle 4): the file the editor writes for a document, computed from what the file is laid over
//! (`base`), what the game should see (`edited`) and what the mod's file holds now (`own`). Pure functions on JSON values; the
//! merge itself stays the engine's (`modding::merge`: objects combine key by key, arrays and scalars replace, then `set` pointers),
//! and the tests below check every case against the engine's own overlay.

use serde_json::{Map, Value};

use super::DocKind;
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
    let mut cx = Cx { sets: vec![], warnings: vec![], allow_set: false };
    if edited.is_null() {
        return Diff { file: None, warnings: vec![] };
    }
    let file = match kind {
        DocKind::Replaced => Some(edited.clone()),
        DocKind::Merged => {
            let body = body(base, edited, own.as_object(), &mut cx);
            (!(body.is_empty() && own.is_null())).then_some(Value::Object(body))
        }
        DocKind::Extends => extends_file(base, edited, own, parent, &mut cx),
    };
    Diff { file, warnings: cx.warnings }
}

fn extends_file(base: &Value, edited: &Value, own: &Value, parent: Option<&str>, cx: &mut Cx) -> Option<Value> {
    cx.allow_set = true;
    let own_map = own.as_object();
    let own_parent = own_map.and_then(|m| m.get("extends")).and_then(Value::as_str);
    let parent = own_parent.or(parent);
    // `extends` and `set` are written by this function; the other own keys are kept.
    let mut kept = own_map.cloned().unwrap_or_default();
    kept.shift_remove("extends");
    let own_set = kept.shift_remove("set");
    let body = body(base, edited, own_map.map(|_| &kept), cx);
    let mut sets: Vec<(String, Value)> = vec![];
    // Hand-written `set` entries stay while the pointer exists; their value follows the document.
    if let Some(Value::Object(set)) = &own_set {
        for (ptr, _) in set {
            if let Some(cur) = edited.pointer(ptr) {
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
                out.insert(key.clone(), kept(bv, ev, ov, path, cx));
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
}
