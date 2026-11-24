//! One edit of a JSON document, addressed by a JSON pointer (RFC 6901: `/guns/Gun_A/pos/0`, `~1` for `/`, `~0` for `~`).
//! Every change the editor makes is an [`Edit`]: forms return them, the undo stack applies and inverts them, the 3D view emits them
//! when a point is dragged. Nothing here knows a file format.

use serde_json::Value;

/// Replace the value at `pointer` with `new` (`None`: remove the key or array element). `old` is what was there before (`None`: it
/// did not exist), filled in by [`apply`], so an edit can be undone by applying [`Edit::inverse`].
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub pointer: String,
    pub old: Option<Value>,
    pub new: Option<Value>,
}

impl Edit {
    /// Set `pointer` to `value` (creating missing parent objects when applied).
    pub fn set(pointer: impl Into<String>, value: Value) -> Edit {
        Edit { pointer: pointer.into(), old: None, new: Some(value) }
    }

    /// Remove the key or array element at `pointer`.
    pub fn remove(pointer: impl Into<String>) -> Edit {
        Edit { pointer: pointer.into(), old: None, new: None }
    }

    /// The edit that undoes this one (after it was applied, so `old` is known).
    pub fn inverse(&self) -> Edit {
        Edit { pointer: self.pointer.clone(), old: self.new.clone(), new: self.old.clone() }
    }
}

/// `key` escaped as one pointer token.
pub fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The pointer of `tokens` (each escaped): `["guns", "A/B", "pos"]` is `/guns/A~1B/pos`.
pub fn pointer<S: AsRef<str>>(tokens: &[S]) -> String {
    tokens.iter().map(|t| format!("/{}", escape(t.as_ref()))).collect()
}

/// The unescaped tokens of `pointer` (`""` is the whole document: no tokens).
pub fn tokens(pointer: &str) -> Vec<String> {
    if pointer.is_empty() {
        return vec![];
    }
    pointer.split('/').skip(1).map(|t| t.replace("~1", "/").replace("~0", "~")).collect()
}

/// Applies `edit` to `doc` and returns it with `old` filled in. Missing parent objects are created for a set; an array index may be
/// an existing element or the length (append) for a set, an existing element for a remove; `-` appends. A path through a scalar,
/// or a bad array index, is an error and leaves `doc` unchanged.
pub fn apply(doc: &mut Value, edit: &Edit) -> Result<Edit, String> {
    let toks = tokens(&edit.pointer);
    let Some((last, parents)) = toks.split_last() else {
        let old = std::mem::replace(doc, edit.new.clone().unwrap_or(Value::Null));
        return Ok(Edit { pointer: edit.pointer.clone(), old: Some(old), new: edit.new.clone() });
    };
    // Check the whole path first so a failure changes nothing.
    {
        let mut at: &Value = doc;
        for t in parents {
            at = match at {
                Value::Object(m) => match m.get(t) {
                    Some(v) => v,
                    None if edit.new.is_some() => &Value::Null,
                    None => return Err(format!("{}: no '{t}'", edit.pointer)),
                },
                Value::Array(a) => a.get(index(t, a.len(), false).ok_or_else(|| format!("{}: bad index '{t}'", edit.pointer))?).expect("checked"),
                Value::Null if edit.new.is_some() => &Value::Null,
                _ => return Err(format!("{}: '{t}' is inside a value that is not an object or list", edit.pointer)),
            };
        }
        match at {
            Value::Object(_) => {}
            Value::Null if edit.new.is_some() => {}
            Value::Array(a) => {
                index(last, a.len(), edit.new.is_some()).ok_or_else(|| format!("{}: bad index '{last}'", edit.pointer))?;
            }
            _ => return Err(format!("{}: the parent is not an object or list", edit.pointer)),
        }
    }
    let mut at: &mut Value = doc;
    for t in parents {
        if at.is_null() {
            *at = Value::Object(Default::default());
        }
        at = match at {
            Value::Object(m) => m.entry(t.clone()).or_insert(Value::Null),
            Value::Array(a) => {
                let i = index(t, a.len(), false).expect("checked");
                &mut a[i]
            }
            _ => unreachable!("checked"),
        };
    }
    if at.is_null() {
        *at = Value::Object(Default::default());
    }
    let old = match (at, &edit.new) {
        (Value::Object(m), Some(v)) => m.insert(last.clone(), v.clone()),
        (Value::Object(m), None) => m.shift_remove(last),
        (Value::Array(a), new) => {
            let i = index(last, a.len(), new.is_some()).expect("checked");
            match new {
                Some(v) if i == a.len() => {
                    a.push(v.clone());
                    None
                }
                Some(v) => Some(std::mem::replace(&mut a[i], v.clone())),
                None => Some(a.remove(i)),
            }
        }
        _ => unreachable!("checked"),
    };
    Ok(Edit { pointer: edit.pointer.clone(), old, new: edit.new.clone() })
}

/// An array index token: an existing element, or the length / `-` when `append` is allowed.
fn index(t: &str, len: usize, append: bool) -> Option<usize> {
    if t == "-" {
        return append.then_some(len);
    }
    if t.len() > 1 && t.starts_with('0') {
        return None;
    }
    let i: usize = t.parse().ok()?;
    (i < len || (append && i == len)).then_some(i)
}

/// Undoing an applied edit: the inverse applied. A removed array element comes back at its index, an added one goes.
pub fn undo(doc: &mut Value, applied: &Edit) -> Result<(), String> {
    let inv = applied.inverse();
    // An append (`-` or the length) is undone by removing the last element.
    if applied.old.is_none() && applied.new.is_some() {
        let toks = tokens(&applied.pointer);
        if let Some(last) = toks.last() {
            let parent = pointer(&toks[..toks.len() - 1]);
            if let Some(Value::Array(a)) = doc.pointer_mut(&parent)
                && (last == "-" || last.parse::<usize>().is_ok_and(|i| i + 1 == a.len()))
            {
                a.pop();
                return Ok(());
            }
        }
    }
    // A removed array element is put back by inserting, not replacing.
    if let (Some(old), None) = (&applied.old, &applied.new) {
        let toks = tokens(&applied.pointer);
        if let Some(last) = toks.last() {
            let parent = pointer(&toks[..toks.len() - 1]);
            if let Some(Value::Array(a)) = doc.pointer_mut(&parent)
                && let Ok(i) = last.parse::<usize>()
                && i <= a.len()
            {
                a.insert(i, old.clone());
                return Ok(());
            }
        }
    }
    apply(doc, &inv).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pointers_escape_and_unescape() {
        assert_eq!(pointer(&["guns", "A/B", "x~y"]), "/guns/A~1B/x~0y");
        assert_eq!(tokens("/guns/A~1B/x~0y"), ["guns", "A/B", "x~y"]);
        assert!(tokens("").is_empty());
        let none: [&str; 0] = [];
        assert_eq!(pointer(&none), "");
    }

    #[test]
    fn set_creates_parents_and_undo_restores() {
        let mut doc = json!({"a": 1});
        let orig = doc.clone();
        let e = apply(&mut doc, &Edit::set("/guns/Gun A/pos", json!([1, 2, 3]))).unwrap();
        assert_eq!(doc["guns"]["Gun A"]["pos"], json!([1, 2, 3]));
        assert_eq!(e.old, None);
        undo(&mut doc, &e).unwrap();
        // the created parents stay (empty), the value is gone
        assert_eq!(doc["guns"]["Gun A"].get("pos"), None);
        assert_eq!(doc["a"], orig["a"]);
    }

    #[test]
    fn replace_and_remove_in_objects_keep_key_order() {
        let mut doc = json!({"a": 1, "b": 2, "c": 3});
        let e = apply(&mut doc, &Edit::set("/b", json!(20))).unwrap();
        assert_eq!(e.old, Some(json!(2)));
        assert_eq!(serde_json::to_string(&doc).unwrap(), r#"{"a":1,"b":20,"c":3}"#);
        let r = apply(&mut doc, &Edit::remove("/b")).unwrap();
        assert_eq!(serde_json::to_string(&doc).unwrap(), r#"{"a":1,"c":3}"#);
        undo(&mut doc, &r).unwrap();
        undo(&mut doc, &e).unwrap();
        assert_eq!(doc, json!({"a": 1, "c": 3, "b": 2}), "an undone removal re-adds the key (at the end)");
    }

    #[test]
    fn arrays_append_remove_and_undo() {
        let mut doc = json!({"l": [1, 2, 3]});
        let add = apply(&mut doc, &Edit::set("/l/-", json!(4))).unwrap();
        assert_eq!(doc["l"], json!([1, 2, 3, 4]));
        let add2 = apply(&mut doc, &Edit::set("/l/4", json!(5))).unwrap();
        let rm = apply(&mut doc, &Edit::remove("/l/0")).unwrap();
        assert_eq!(doc["l"], json!([2, 3, 4, 5]));
        undo(&mut doc, &rm).unwrap();
        assert_eq!(doc["l"], json!([1, 2, 3, 4, 5]));
        undo(&mut doc, &add2).unwrap();
        undo(&mut doc, &add).unwrap();
        assert_eq!(doc["l"], json!([1, 2, 3]));
    }

    #[test]
    fn bad_paths_are_errors_and_change_nothing() {
        let mut doc = json!({"n": 5, "l": [1]});
        let before = doc.clone();
        assert!(apply(&mut doc, &Edit::set("/n/x", json!(1))).is_err());
        assert!(apply(&mut doc, &Edit::set("/l/5", json!(1))).is_err());
        assert!(apply(&mut doc, &Edit::set("/l/01", json!(1))).is_err());
        assert!(apply(&mut doc, &Edit::remove("/missing/x")).is_err());
        assert!(apply(&mut doc, &Edit::remove("/l/1")).is_err());
        assert_eq!(doc, before);
    }

    #[test]
    fn whole_document_replace() {
        let mut doc = json!({"a": 1});
        let e = apply(&mut doc, &Edit::set("", json!([1]))).unwrap();
        assert_eq!(doc, json!([1]));
        undo(&mut doc, &e).unwrap();
        assert_eq!(doc, json!({"a": 1}));
    }
}
