//! Keyed lists in an `extends` file (docs/phase4.md P1 "the array problem", work item T4): a list whose entries have a stable key
//! (`id`, `name`) can be changed entry by entry instead of being written whole, so a mod that adds one island to a retail map does
//! not restate the map's `objects`.
//!
//! PROVISIONAL: this module restates the engine's keyed merge until the engine pin moves to a version that has it (engine branch
//! `claude/engine-integration`, `game/src/modding.rs`: `keyed_defaults`, `merge_keyed`, `extend`; docs/modding.md "Changing one
//! entry of a list"). The pinned engine replaces a list whole, so a file written with this rule is merged right by the editor
//! ([`extend`], also used to open such a file) but not yet by the pinned engine's overlay. The rule, and the `$remove` spelling, are
//! the engine's to define: when the pin moves, [`KeyedRule::lists`] is the one place to replace (ask the engine for it).
//!
//! The rule: in an extends file, which lists merge by key by default; a file may override with `"merge": {"/objects": "id"}` (merge
//! by that field) or `"merge": {"/objects": false}` (replace whole). A diff entry whose key matches a parent entry is deep-merged
//! into it, one that matches none is appended, `{"<key>": X, "$remove": true}` drops the parent entry. Parent entries not mentioned
//! keep their place. The whole merge comes first, then `set`.

use serde_json::{Map, Value};

use crate::edit::pointer;

/// Which lists of a file merge by key by default. PROVISIONAL, see the module doc.
pub trait KeyedRule {
    /// `(pointer, key field)` of the lists that merge by key in the extends file `rel` (a path inside the assets). The default is the
    /// engine's rule: in `maps/<id>/map.json` `objects`, `ships` and `mines` by `id`, `nebulae` and `players` by `name`; `mission.json`
    /// and `script.json` have their own entries; every other file replaces its lists whole.
    fn lists(&self, rel: &str) -> Vec<(String, String)> {
        let parts: Vec<&str> = rel.split('/').collect();
        let pairs: &[(&str, &str)] = match parts[..] {
            ["maps", _, "map.json"] => &[("/objects", "id"), ("/ships", "id"), ("/mines", "id"), ("/nebulae", "name"), ("/players", "name")],
            ["maps", _, "mission.json"] => &[("/ships", "id"), ("/rules", "name"), ("/points", "Name")],
            ["maps", _, "script.json"] => &[("/rules", "name")],
            _ => &[],
        };
        pairs.iter().map(|(p, f)| (p.to_string(), f.to_string())).collect()
    }
}

/// The engine's rule as [`KeyedRule`]'s default.
pub struct EngineRule;
impl KeyedRule for EngineRule {}

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
    let Some((_, field)) = effective(rule, rel, own_merge).into_iter().find(|(p, _)| p == ptr) else {
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

// ---- the merge itself (a model of the engine's, used to open a file and to test the diff) ----

/// Objects merge key by key, anything else is replaced (the engine's `merge`).
pub fn deep_merge(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                deep_merge(b.entry(k.clone()).or_insert(Value::Null), v)
            }
        }
        (b, o) => *b = o.clone(),
    }
}

fn object_slot<'a>(v: &'a mut Value, ptr: &str) -> Option<&'a mut Value> {
    let mut at = v;
    for token in ptr.strip_prefix('/')?.split('/') {
        let key = token.replace("~1", "/").replace("~0", "~");
        at = at.as_object_mut()?.entry(key).or_insert(Value::Null);
    }
    Some(at)
}

/// Merges the diff `entries` into the list at `ptr` of `base` by `field`. Returns warnings (a removal of an entry the list has not).
/// Errors as the engine does: the list is not a list, a parent entry has no key or two share one, a diff entry has no key.
pub fn merge_keyed(base: &mut Value, ptr: &str, field: &str, entries: &[Value]) -> Result<Vec<String>, String> {
    let err = |m: String| format!("merges '{ptr}': {m}");
    let slot = object_slot(base, ptr).ok_or_else(|| err("the file being extended has no object on that path".into()))?;
    if slot.is_null() {
        *slot = Value::Array(vec![]);
    }
    let Value::Array(list) = slot else { return Err(err("the file being extended has no list there".into())) };
    let keys = keys_of(list, field).map_err(|e| err(format!("the list being extended: {e}")))?;
    let mut items: Vec<Option<Value>> = std::mem::take(list).into_iter().map(Some).collect();
    let mut index: std::collections::HashMap<String, usize> = keys.into_iter().enumerate().map(|(i, k)| (k, i)).collect();
    let mut warnings = vec![];
    for (n, entry) in entries.iter().enumerate() {
        let Some(obj) = entry.as_object() else { return Err(err(format!("entry {n} is not an object"))) };
        let Some(k) = key_text(obj.get(field)) else { return Err(err(format!("entry {n} has no '{field}'"))) };
        let remove = obj.get("$remove") == Some(&Value::Bool(true));
        let mut change = entry.clone();
        if let Some(o) = change.as_object_mut() {
            o.shift_remove("$remove");
        }
        match (index.get(&k).copied(), remove) {
            (Some(i), true) => {
                items[i] = None;
                index.remove(&k);
            }
            (None, true) => warnings.push(err(format!("removes {field} {k}, which is not in the list being extended"))),
            (Some(i), false) => deep_merge(items[i].as_mut().expect("indexed entries are present"), &change),
            (None, false) => {
                index.insert(k, items.len());
                items.push(Some(change));
            }
        }
    }
    if let Some(Value::Array(out)) = object_slot(base, ptr) {
        *out = items.into_iter().flatten().collect();
    }
    Ok(warnings)
}

fn take_list(diff: &mut Value, ptr: &str) -> Result<Option<Vec<Value>>, String> {
    let Some((parent, last)) = ptr.rsplit_once('/').filter(|(_, last)| !last.is_empty()) else {
        return Err(format!("merges '{ptr}': not a JSON pointer to a key (like \"/objects\")"));
    };
    let last = last.replace("~1", "/").replace("~0", "~");
    let mut at = &mut *diff;
    for token in parent.split('/').skip(1) {
        let key = token.replace("~1", "/").replace("~0", "~");
        at = match at.as_object_mut().and_then(|o| o.get_mut(&key)) {
            Some(next) => next,
            None => return Ok(None),
        };
    }
    match at.as_object_mut().and_then(|o| o.shift_remove(&last)) {
        None => Ok(None),
        Some(Value::Array(list)) => Ok(Some(list)),
        Some(_) => Err(format!("merges '{ptr}': the file's value there is not a list")),
    }
}

/// What the extends file `diff` (named `rel`) does to its `parent`: the engine's `extend`. Lists of [`effective`] merge by key, the
/// rest as the plain merge does, then `set`; `extends`, `merge` and `set` are dropped. Returns the result and the warnings.
pub fn extend(rule: &dyn KeyedRule, rel: &str, mut parent: Value, diff: &Value) -> Result<(Value, Vec<String>), String> {
    let mut diff = diff.clone();
    let own_merge = diff.as_object_mut().filter(|o| o.get("merge").is_some_and(Value::is_object)).and_then(|o| o.shift_remove("merge"));
    let mut lists = vec![];
    for (ptr, field) in effective(rule, rel, own_merge.as_ref()) {
        if diff.pointer(&ptr).is_some_and(Value::is_array) {
            lists.push((ptr.clone(), field, take_list(&mut diff, &ptr)?.unwrap_or_default()));
        }
    }
    // A pointer the file names that nothing merges by default and the diff has no list for is an error in the engine.
    for (ptr, v) in own_merge.as_ref().and_then(Value::as_object).into_iter().flatten() {
        if v.is_string() && !lists.iter().any(|(p, _, _)| p == ptr) && !diff.pointer(ptr).is_some_and(Value::is_array) {
            return Err(format!("merges '{ptr}': the file has no list there"));
        }
    }
    deep_merge(&mut parent, &diff);
    let obj = parent.as_object_mut().ok_or("the merged file is not a JSON object")?;
    obj.shift_remove("extends");
    let set = obj.shift_remove("set");
    let mut warnings = vec![];
    for (ptr, field, list) in lists {
        warnings.extend(merge_keyed(&mut parent, &ptr, &field, &list)?);
    }
    if let Some(Value::Object(set)) = set {
        for (ptr, value) in set {
            match parent.pointer_mut(&ptr) {
                Some(slot) => *slot = value,
                None => return Err(format!("sets '{ptr}', which does not exist")),
            }
        }
    }
    Ok((parent, warnings))
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
        let (out, _) = extend(&Fake, "x.json", json!({"items": [{"k": 1, "v": 1}]}), &json!({"items": [{"k": 1, "v": 2}, {"k": 2}]})).unwrap();
        assert_eq!(out, json!({"items": [{"k": 1, "v": 2}, {"k": 2}]}));
        // another file: the list is replaced whole
        let (out, _) = extend(&Fake, "y.json", json!({"items": [{"k": 1, "v": 1}]}), &json!({"items": [{"k": 2}]})).unwrap();
        assert_eq!(out, json!({"items": [{"k": 2}]}));
    }

    #[test]
    fn extend_merges_matches_appends_and_removes_by_key() {
        let diff = json!({
            "extends": "red",
            "objects": [{"id": 2, "pos": [6, 6, 0]}, {"id": 1, "$remove": true}, {"id": 7, "type": "New", "pos": [1, 1, 1]}],
            "nebulae": [{"name": "N2", "storm": true}]
        });
        let (out, warn) = extend(&EngineRule, MAP, base(), &diff).unwrap();
        assert!(warn.is_empty());
        assert_eq!(out["objects"], json!([{"id": 2, "type": "B", "pos": [6, 6, 0]}, {"id": 3, "type": "C", "pos": [9, 9, 0]}, {"id": 7, "type": "New", "pos": [1, 1, 1]}]));
        assert_eq!(out["nebulae"], json!([{"name": "N1", "storm": false}, {"name": "N2", "storm": true}]));
        assert_eq!(out["teams"], base()["teams"]);
        assert!(out.get("extends").is_none());
    }

    #[test]
    fn extend_removal_of_a_missing_entry_warns_and_set_counts_after_the_merge() {
        let diff = json!({"objects": [{"id": 9, "$remove": true}, {"id": 1, "$remove": true}], "set": {"/objects/0/type": "Z"}});
        let (out, warn) = extend(&EngineRule, MAP, base(), &diff).unwrap();
        assert_eq!(warn.len(), 1, "{warn:?}");
        assert_eq!(out["objects"][0], json!({"id": 2, "type": "Z", "pos": [5, 5, 0]}), "the pointer counts the entries after the keyed merge");
        let bad = extend(&EngineRule, MAP, base(), &json!({"set": {"/objects/9/type": "Z"}}));
        assert!(bad.is_err());
        let nokey = extend(&EngineRule, MAP, base(), &json!({"objects": [{"type": "x"}]}));
        assert!(nokey.unwrap_err().contains("has no 'id'"));
        let dup = extend(&EngineRule, MAP, json!({"objects": [{"id": 1}, {"id": 1}]}), &json!({"objects": [{"id": 1}]}));
        assert!(dup.unwrap_err().contains("same 'id'"));
    }

    #[test]
    fn extend_merge_false_replaces_whole_and_a_named_field_merges_other_lists() {
        let (out, _) = extend(&EngineRule, MAP, base(), &json!({"merge": {"/objects": false}, "objects": [{"id": 5}]})).unwrap();
        assert_eq!(out["objects"], json!([{"id": 5}]));
        assert!(out.get("merge").is_none());
        let (out, _) = extend(&EngineRule, MAP, base(), &json!({"merge": {"/teams": "name_id"}, "teams": [{"name_id": "T1", "x": 1}, {"name_id": "T2"}]})).unwrap();
        assert_eq!(out["teams"], json!([{"name_id": "T1", "x": 1}, {"name_id": "T2"}]));
        // a list the parent has not: the entries are the list
        let (out, _) = extend(&EngineRule, MAP, json!({}), &json!({"ships": [{"id": 1}]})).unwrap();
        assert_eq!(out["ships"], json!([{"id": 1}]));
    }

    fn roundtrip(base: &Value, edited: &Value, field: &str) -> ListDiff {
        let (b, e) = (base.as_array().unwrap(), edited.as_array().unwrap());
        let d = diff_list(field, b, e, "/l");
        if let ListDiff::Entries { entries, .. } | ListDiff::SameKeys { entries, .. } = &d {
            let mut v = json!({"l": base});
            merge_keyed(&mut v, "/l", field, entries).unwrap();
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
