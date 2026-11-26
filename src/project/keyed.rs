//! Keyed lists in an `extends` file (docs/phase4.md P1 "the array problem", work item T4): a list whose entries have a stable key
//! (`id`, `name`) can be changed entry by entry instead of being written whole, so a mod that adds one island to a retail map does
//! not restate the map's `objects`.
//!
//! The engine owns the rule and the merge: [`openprocyon::authoring::keyed_lists`] says which lists merge by key, and
//! [`openprocyon::authoring::extend`] / [`openprocyon::authoring::merge_keyed`] merge. The editor only needs the rule to WRITE a file
//! (which lists may add and delete one entry, [`diff_list`]); it asks the engine ([`EngineRule`]) and keeps no copy. The `$remove`
//! spelling is the engine's PROVISIONAL one too.
//!
//! The rule: in an extends file, which lists merge by key by default; a file may override with `"merge": {"/objects": "id"}` (merge
//! by that field) or `"merge": {"/objects": false}` (replace whole). A diff entry whose key matches a parent entry is deep-merged
//! into it, one that matches none is appended, `{"<key>": X, "$remove": true}` drops the parent entry. Parent entries not mentioned
//! keep their place. The whole merge comes first, then `set`.

use serde_json::{Map, Value};

use crate::edit::pointer;

/// Which lists of a file merge by key by default: the engine's answer ([`EngineRule`]), or a stand-in in tests.
pub trait KeyedRule {
    /// `(pointer, key field)` of the lists that merge by key in the extends file `rel` (a path inside the assets).
    fn lists(&self, rel: &str) -> Vec<(String, String)>;
}

/// The engine's rule: `authoring::keyed_lists`.
pub struct EngineRule;
impl KeyedRule for EngineRule {
    fn lists(&self, rel: &str) -> Vec<(String, String)> {
        openprocyon::authoring::keyed_lists(rel).into_iter().map(|(p, f)| (p.to_string(), f.to_string())).collect()
    }
}

/// No list merges by key (what the code before the keyed merge did).
pub struct NoKeyed;
impl KeyedRule for NoKeyed {
    fn lists(&self, _rel: &str) -> Vec<(String, String)> {
        vec![]
    }
}

/// The lists that merge by key in `rel` once the file's own `merge` object (`own_merge`) is applied: `"/ptr": "field"` adds or
/// changes one, `"/ptr": false` takes a default one away. Anything else in `merge` is ignored (the engine rejects it).
pub fn effective(rule: &dyn KeyedRule, rel: &str, own_merge: Option<&Value>) -> Vec<(String, String)> {
    let mut lists = rule.lists(rel);
    for (ptr, v) in own_merge.and_then(Value::as_object).into_iter().flatten() {
        lists.retain(|(p, _)| p != ptr);
        if let Some(field) = v.as_str().filter(|f| !f.is_empty()) {
            lists.push((ptr.clone(), field.to_string()));
        }
    }
    lists
}

/// The key field of the list at `ptr` in `rel`, when it merges by key.
pub fn key_field(rule: &dyn KeyedRule, rel: &str, own_merge: Option<&Value>, ptr: &str) -> Option<String> {
    effective(rule, rel, own_merge).into_iter().find(|(p, _)| p == ptr).map(|(_, f)| f)
}

/// The text a key value is matched by: strings, numbers and booleans only.
pub fn key_text(v: Option<&Value>) -> Option<String> {
    v.filter(|v| matches!(v, Value::String(_) | Value::Number(_) | Value::Bool(_))).map(Value::to_string)
}

/// The keys of every entry of `list`, in order; an error names the first entry that has none or repeats one (such a list cannot be
/// merged by key: the engine refuses it too).
pub fn keys_of(list: &[Value], field: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for (i, e) in list.iter().enumerate() {
        let k = key_text(e.get(field)).ok_or_else(|| format!("entry {i} has no '{field}'"))?;
        if let Some(j) = out.iter().position(|o| *o == k) {
            return Err(format!("entries {j} and {i} have the same '{field}' ({k})"));
        }
        out.push(k);
    }
    Ok(out)
}

/// Whether entries can be added to and deleted from the list at `ptr` of the file `rel` that extends a parent whose list is `base`
/// (`None`: the parent has none): the list merges by key, and the parent's entries all have a unique key.
pub fn can_add_delete(rule: &dyn KeyedRule, rel: &str, own_merge: Option<&Value>, ptr: &str, base: Option<&Value>) -> Result<(), String> {
    let Some(field) = key_field(rule, rel, own_merge, ptr) else {
        return Err(format!("{ptr} of {rel} is not merged by key, so a file that extends its parent cannot add or delete one entry without restating the whole list"));
    };
    let list = base.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    keys_of(list, &field)
        .map(|_| ())
        .map_err(|e| format!("{ptr} of the file below {rel}: {e}, so its entries cannot be told apart and one cannot be added or deleted without restating the whole list"))
}

/// What turns a list into another, by key.
#[derive(Clone, Debug, PartialEq)]
pub enum ListDiff {
    /// The lists are equal.
    Equal,
    /// The same entries in the same order, some changed (a move): `entries` are the keyed diff entries; the project writes `set` pointers.
    SameKeys { entries: Vec<Value>, warnings: Vec<String> },
    /// Entries added, removed or both: the diff entries, in the order [`merge_keyed`] needs.
    Entries { entries: Vec<Value>, warnings: Vec<String> },
    /// The change cannot be written as a keyed diff: an entry with no key, a repeated key, or an entry that is not an object. The
    /// reason is a sentence.
    Unkeyable(String),
}

/// The keyed diff from `base` to `edited` by `field`; `at` names the list in warnings (`/objects`).
///
/// Entries come out as: for each parent entry in order, `{"<key>": X, "$remove": true}` when `edited` dropped it, or the key plus the
/// changed fields when it differs (objects deep, everything else whole, as the merge does); then the new entries whole, in order.
/// The merge keeps the parent's order and appends, so an entry that moved, or follows one added in the middle, is removed and written
/// whole again at the end. A field `edited` took away from an entry cannot be removed by a merge: it is a warning and stays.
pub fn diff_list(field: &str, base: &[Value], edited: &[Value], at: &str) -> ListDiff {
    let (bk, ek) = match (keys_of(base, field), keys_of(edited, field)) {
        (Ok(b), Ok(e)) => (b, e),
        (Err(e), _) => return ListDiff::Unkeyable(format!("the list below has {e}")),
        (_, Err(e)) => return ListDiff::Unkeyable(format!("the edited list has {e}")),
    };
    if base == edited {
        return ListDiff::Equal;
    }
    if let Some(i) = base.iter().chain(edited).position(|e| !e.is_object()) {
        return ListDiff::Unkeyable(format!("entry {i} is not an object"));
    }
    // The merge keeps the parent's order and appends. `head` is the longest start of `edited` that is the parent's entries in the
    // parent's order (the ones the edit left in place); the rest, the tail, is written as removals of what the parent has of it plus
    // the entries whole, in order: new entries, and entries that moved (one added in the middle pushes the entries after it).
    let kept: Vec<&String> = bk.iter().filter(|k| ek.contains(k)).collect();
    let head = ek.iter().zip(&kept).take_while(|(e, k)| e == *k).count();
    let mut entries = vec![];
    let mut warnings = vec![];
    for (b, bkey) in base.iter().zip(&bk) {
        match ek.iter().position(|k| k == bkey).filter(|&j| j < head) {
            None => {
                let mut m = Map::new();
                m.insert(field.to_string(), b[field].clone());
                m.insert("$remove".into(), Value::Bool(true));
                entries.push(Value::Object(m));
            }
            Some(j) if *b != edited[j] => {
                let mut path = vec![at.to_string(), bkey.trim_matches('"').to_string()];
                let part = entry_diff(b.as_object().expect("checked"), edited[j].as_object().expect("checked"), &mut path, &mut warnings);
                if !part.is_empty() {
                    let mut m = Map::new();
                    m.insert(field.to_string(), b[field].clone());
                    m.extend(part.into_iter().filter(|(k, _)| k != field));
                    entries.push(Value::Object(m));
                }
            }
            Some(_) => {}
        }
    }
    entries.extend(edited[head..].iter().cloned());
    if bk == ek { ListDiff::SameKeys { entries, warnings } } else { ListDiff::Entries { entries, warnings } }
}

/// The fields of `edited` that differ from `base` (objects deep, so only the changed leaves of a nested object); a field `edited`
/// lacks is a warning.
fn entry_diff(base: &Map<String, Value>, edited: &Map<String, Value>, path: &mut Vec<String>, warnings: &mut Vec<String>) -> Map<String, Value> {
    let mut out = Map::new();
    for k in base.keys().filter(|k| !edited.contains_key(*k)) {
        path.push(k.clone());
        warnings.push(format!("{} was removed, but a merge cannot remove a key the entry below has; it stays", path.join("/")));
        path.pop();
    }
    for (k, ev) in edited {
        match base.get(k) {
            Some(bv) if bv == ev => {}
            Some(Value::Object(bo)) if ev.is_object() => {
                path.push(k.clone());
                let sub = entry_diff(bo, ev.as_object().expect("checked"), path, warnings);
                path.pop();
                if !sub.is_empty() {
                    out.insert(k.clone(), Value::Object(sub));
                }
            }
            _ => {
                out.insert(k.clone(), ev.clone());
            }
        }
    }
    out
}

/// What the engine makes of the extends file `diff` (named `rel`) laid over `parent`: [`openprocyon::authoring::extend`], the loader's
/// own merge. `Err`: the engine would skip the file (its reason). This is the oracle the tests compare the diff against; the editor
/// never merges itself.
pub fn engine_extend(rel: &str, parent: &Value, diff: &Value) -> Result<(Value, Vec<String>), String> {
    openprocyon::authoring::extend(rel, parent.clone(), diff)
}

/// The pointer of the list `key` at the top of a file (`objects` is `/objects`).
pub fn list_pointer(key: &str) -> String {
    pointer(&[key])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MAP: &str = "maps/red/map.json";

    fn base() -> Value {
        json!({
            "name": "Red", "world_half": 100,
            "objects": [{"id": 1, "type": "A", "pos": [0, 0, 0]}, {"id": 2, "type": "B", "pos": [5, 5, 0]}, {"id": 3, "type": "C", "pos": [9, 9, 0]}],
            "nebulae": [{"name": "N1", "storm": false}],
            "teams": [{"name_id": "T1"}]
        })
    }

    #[test]
    fn the_default_rule_is_the_engines() {
        let r = EngineRule;
        let l = r.lists(MAP);
        assert_eq!(l.len(), 5);
        assert!(l.contains(&("/objects".into(), "id".into())) && l.contains(&("/ships".into(), "id".into())) && l.contains(&("/mines".into(), "id".into())));
        assert!(l.contains(&("/nebulae".into(), "name".into())) && l.contains(&("/players".into(), "name".into())));
        assert!(!l.iter().any(|(p, _)| p == "/teams"), "teams are not keyed by the engine's rule");
        assert!(r.lists("ships/X/ship.json").is_empty() && r.lists("maps/a/b/map.json").is_empty() && r.lists("data/weapons/guns.json").is_empty());
        assert_eq!(r.lists("maps/red/script.json"), [("/rules".to_string(), "name".to_string())]);
        assert!(NoKeyed.lists(MAP).is_empty());
    }

    #[test]
    fn a_files_merge_object_overrides_the_default() {
        let m = json!({"/objects": false, "/teams": "name_id", "/ships": 5});
        let l = effective(&EngineRule, MAP, Some(&m));
        assert!(!l.iter().any(|(p, _)| p == "/objects"), "false takes it away");
        assert!(l.contains(&("/teams".into(), "name_id".into())), "a field name adds it");
        assert!(!l.iter().any(|(p, _)| p == "/ships"), "a value that is neither is no key field: the list is not merged by key");
        assert!(l.contains(&("/mines".into(), "id".into())));
        assert_eq!(effective(&EngineRule, MAP, None).len(), 5);
    }

    #[test]
    fn a_fake_rule_fills_the_trait() {
        struct Fake;
        impl KeyedRule for Fake {
            fn lists(&self, rel: &str) -> Vec<(String, String)> {
                (rel == "x.json").then(|| ("/items".to_string(), "k".to_string())).into_iter().collect()
            }
        }
        assert_eq!(effective(&Fake, "x.json", None), [("/items".to_string(), "k".to_string())]);
        assert!(effective(&Fake, "y.json", None).is_empty());
    }

    fn roundtrip(base: &Value, edited: &Value, field: &str) -> ListDiff {
        let (b, e) = (base.as_array().unwrap(), edited.as_array().unwrap());
        let d = diff_list(field, b, e, "/l");
        if let ListDiff::Entries { entries, .. } | ListDiff::SameKeys { entries, .. } = &d {
            let mut v = json!({"l": base});
            openprocyon::authoring::merge_keyed(&mut v, "/l", field, entries).unwrap();
            assert_eq!(v["l"], *edited, "merging {entries:?}");
        }
        d
    }

    #[test]
    fn diff_list_adds_removes_and_changes_by_key() {
        let b = json!([{"id": 1, "a": 1, "n": {"x": 1, "y": 2}}, {"id": 2}, {"id": 3, "pos": [1, 2]}]);
        let e = json!([{"id": 1, "a": 1, "n": {"x": 1, "y": 5}}, {"id": 3, "pos": [1, 2]}, {"id": 4, "t": "new"}]);
        let ListDiff::Entries { entries, warnings } = roundtrip(&b, &e, "id") else { panic!() };
        assert_eq!(entries, vec![json!({"id": 1, "n": {"y": 5}}), json!({"id": 2, "$remove": true}), json!({"id": 4, "t": "new"})]);
        assert!(warnings.is_empty());
        assert_eq!(roundtrip(&b, &b, "id"), ListDiff::Equal);
    }

    #[test]
    fn diff_list_same_keys_and_moves() {
        let b = json!([{"name": "A", "start": [0, 0]}, {"name": "B", "start": [1, 1]}]);
        let e = json!([{"name": "A", "start": [0, 0]}, {"name": "B", "start": [9, 9]}]);
        let ListDiff::SameKeys { entries, .. } = roundtrip(&b, &e, "name") else { panic!() };
        assert_eq!(entries, vec![json!({"name": "B", "start": [9, 9]})]);
    }

    #[test]
    fn diff_list_a_taken_away_field_warns_and_stays() {
        let b = json!([{"id": 1, "a": 1, "b": 2}]);
        let e = json!([{"id": 1, "a": 1}]);
        let ListDiff::SameKeys { entries, warnings } = diff_list("id", b.as_array().unwrap(), e.as_array().unwrap(), "/objects") else { panic!() };
        assert!(entries.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("/objects/1/b was removed"), "{warnings:?}");
    }

    #[test]
    fn diff_list_a_moved_entry_or_one_added_in_the_middle_goes_to_the_end() {
        let b = json!([{"id": 1, "a": 1}, {"id": 2, "a": 2}, {"id": 3, "a": 3}]);
        // reordered: 3 and 2 are removed and written again, in the new order
        let ListDiff::Entries { entries, .. } = roundtrip(&b, &json!([{"id": 1, "a": 1}, {"id": 3, "a": 3}, {"id": 2, "a": 2}]), "id") else { panic!() };
        assert_eq!(entries, vec![json!({"id": 2, "$remove": true}), json!({"id": 3, "$remove": true}), json!({"id": 3, "a": 3}), json!({"id": 2, "a": 2})]);
        // deleted and added again (undo-like): the entry is last now
        let ListDiff::Entries { entries, .. } = roundtrip(&b, &json!([{"id": 1, "a": 1}, {"id": 3, "a": 3}, {"id": 2, "a": 2}]), "id") else { panic!() };
        assert_eq!(entries.len(), 4);
        // a new entry first pushes everything after it
        let d = roundtrip(&b, &json!([{"id": 9}, {"id": 1, "a": 1}, {"id": 2, "a": 2}, {"id": 3, "a": 3}]), "id");
        assert!(matches!(d, ListDiff::Entries { ref entries, .. } if entries.len() == 7), "{d:?}");
        // a new entry last, and a removal first: nothing else is written
        let ListDiff::Entries { entries, .. } = roundtrip(&b, &json!([{"id": 2, "a": 2}, {"id": 3, "a": 3}, {"id": 4}]), "id") else { panic!() };
        assert_eq!(entries, vec![json!({"id": 1, "$remove": true}), json!({"id": 4})]);
    }

    #[test]
    fn diff_list_refuses_what_a_merge_cannot_produce() {
        let b = json!([{"id": 1}, {"id": 2}]);
        let un = |e: Value| matches!(diff_list("id", b.as_array().unwrap(), e.as_array().unwrap(), "/l"), ListDiff::Unkeyable(_));
        assert!(un(json!([{"id": 1}, {"id": 2}, {"x": 1}])), "no key");
        assert!(un(json!([{"id": 1}, {"id": 1}])), "same key twice");
        assert!(un(json!([{"id": 1}, 5])), "not an object");
        let keyless = json!([{"id": 1}, {"type": "decor"}]);
        assert!(matches!(diff_list("id", keyless.as_array().unwrap(), b.as_array().unwrap(), "/l"), ListDiff::Unkeyable(_)), "the list below has an entry with no key");
    }

    #[test]
    fn can_add_delete_needs_a_keyed_list_with_unique_keys_below() {
        let b = base();
        assert!(can_add_delete(&EngineRule, MAP, None, "/objects", b.get("objects")).is_ok());
        assert!(can_add_delete(&EngineRule, MAP, None, "/ships", None).is_ok(), "no list below: every entry is new");
        assert!(can_add_delete(&EngineRule, MAP, None, "/teams", b.get("teams")).is_err());
        assert!(can_add_delete(&EngineRule, MAP, Some(&json!({"/objects": false})), "/objects", b.get("objects")).is_err());
        let keyless = json!({"objects": [{"id": 1}, {"type": "decor"}]});
        let e = can_add_delete(&EngineRule, MAP, None, "/objects", keyless.get("objects")).unwrap_err();
        assert!(e.contains("has no 'id'"), "{e}");
        assert!(can_add_delete(&NoKeyed, MAP, None, "/objects", None).is_err());
    }
}
