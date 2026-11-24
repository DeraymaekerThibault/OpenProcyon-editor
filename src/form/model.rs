//! A resolved, egui-free view of a JSON Schema node (draft 2020-12, as `openprocyon::authoring::schemas()` returns it), and the pure
//! helpers that turn a form action into [`Edit`]s. The schema is the engine's; nothing about a file's keys is written down here.
//!
//! Resolved: `$ref` into `$defs` (a sibling `description`/`default` of the `$ref` wins), optional types (`anyOf [X, null]`,
//! `type: [X, "null"]`), enums (`enum`, `const`, or `oneOf`/`anyOf` of constants). Anything else becomes [`Kind::Any`] (raw JSON).

use serde_json::{Map, Value, json};

use crate::edit::{self, Edit};

/// How deep `$ref`s are followed before a node is given up as [`Kind::Any`] (guards against a cyclic schema).
const MAX_DEPTH: usize = 24;

/// The bounds of a number field. An exclusive bound is kept as a flag; the widget clamps to the bound itself
/// (PROVISIONAL: no schema in use has an exclusive bound).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub exclusive_min: bool,
    pub exclusive_max: bool,
}

/// What kind of value a node holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Named properties (`$schema` left out). Keys the schema does not list are unknown keys, not part of the node.
    /// PROVISIONAL: `additionalProperties` next to `properties` is ignored (no schema in use has both).
    Object { props: Vec<(String, Node)> },
    /// Free keys with one value schema (`additionalProperties`); `hidden` are the listed properties that are not shown (`$schema`).
    /// A bare `type: object` with no properties is a map of [`Kind::Any`].
    Map { value: Box<Node>, hidden: Vec<String> },
    /// A list: `items` for every element, `prefix` for the first ones (`prefixItems`).
    Array { items: Option<Box<Node>>, prefix: Vec<Node>, min: Option<usize>, max: Option<usize> },
    /// One of these values.
    Enum(Vec<Value>),
    /// A whole number (`format: uint64` and the like mean a minimum of 0).
    Integer(Range),
    /// Any number.
    Number(Range),
    /// Text.
    String,
    /// `true` / `false`.
    Boolean,
    /// Anything else: edited as raw JSON text.
    Any,
}

/// One resolved schema node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: Kind,
    /// `null` is also valid (`anyOf [X, null]`, `type: [X, "null"]`): the key may be left unset.
    pub optional: bool,
    /// The schema's `description` (hover text), empty when none.
    pub description: String,
    /// The schema's `default`, when it has one other than `null`.
    pub default: Option<Value>,
}

impl Node {
    /// The node for the schema document `root` (follows `$ref` into it).
    pub fn from_schema(root: &Value) -> Node {
        resolve(root, root, 0)
    }

    /// The node of the child at `token` (a property name, a map key, an array index), when this node has one.
    pub fn child(&self, token: &str) -> Option<&Node> {
        match &self.kind {
            Kind::Object { props } => props.iter().find(|(k, _)| k == token).map(|(_, n)| n),
            Kind::Map { value, .. } => Some(value),
            Kind::Array { items, prefix, .. } => {
                let i: usize = token.parse().ok()?;
                prefix.get(i).or(items.as_deref())
            }
            _ => None,
        }
    }

    /// The node at a JSON pointer below this one.
    pub fn at(&self, pointer: &str) -> Option<&Node> {
        edit::tokens(pointer).iter().try_fold(self, |n, t| n.child(t))
    }

    /// Whether this is a list of 1 to 4 numbers at most, which the form draws as one row of number boxes.
    pub fn is_short_numbers(&self) -> bool {
        match &self.kind {
            Kind::Array { items: Some(item), prefix, max, .. } => prefix.is_empty() && matches!(item.kind, Kind::Number(_) | Kind::Integer(_)) && max.is_some_and(|m| m <= 4),
            _ => false,
        }
    }

    /// Whether the node is drawn as a collapsible section (its value has parts of its own) and not as one row.
    pub fn is_compound(&self) -> bool {
        match &self.kind {
            Kind::Object { .. } | Kind::Map { .. } => true,
            Kind::Array { .. } => !self.is_short_numbers(),
            _ => false,
        }
    }
}

fn any(description: String) -> Node {
    Node { kind: Kind::Any, optional: false, description, default: None }
}

fn resolve(root: &Value, schema: &Value, depth: usize) -> Node {
    let description = schema["description"].as_str().unwrap_or_default().to_string();
    if depth > MAX_DEPTH {
        return any(description);
    }
    if let Some(r) = schema["$ref"].as_str() {
        let Some(target) = root.pointer(r.trim_start_matches('#')) else { return any(description) };
        let mut node = resolve(root, target, depth + 1);
        if !description.is_empty() {
            node.description = description;
        }
        if let Some(d) = schema.get("default").filter(|d| !d.is_null()) {
            node.default = Some(d.clone());
        }
        return node;
    }
    let default = schema.get("default").filter(|d| !d.is_null()).cloned();
    let done = |kind: Kind, optional: bool, description: String, default: Option<Value>| Node { kind, optional, description, default };

    // anyOf / oneOf: one non-null variant is that variant, optional; all constants are an enum; anything else is raw JSON.
    for key in ["anyOf", "oneOf"] {
        if let Some(variants) = schema[key].as_array() {
            let optional = variants.iter().any(|v| v["type"] == "null");
            let rest: Vec<&Value> = variants.iter().filter(|v| v["type"] != "null").collect();
            if let [one] = rest[..] {
                let mut node = resolve(root, one, depth + 1);
                node.optional |= optional;
                if !description.is_empty() {
                    node.description = description;
                }
                if default.is_some() {
                    node.default = default;
                }
                return node;
            }
            let consts: Vec<Value> = rest.iter().flat_map(|v| constants(v)).collect();
            if !rest.is_empty() && consts.len() == rest.iter().map(|v| constants(v).len()).sum::<usize>() && rest.iter().all(|v| !constants(v).is_empty()) {
                return done(Kind::Enum(consts), optional, description, default);
            }
            return done(Kind::Any, optional, description, default);
        }
    }
    if let Some(values) = schema["enum"].as_array() {
        let optional = values.iter().any(Value::is_null);
        return done(Kind::Enum(values.iter().filter(|v| !v.is_null()).cloned().collect()), optional, description, default);
    }
    if let Some(c) = schema.get("const") {
        return done(Kind::Enum(vec![c.clone()]), false, description, default);
    }

    // `type` is a name or a list of names; `null` in a list marks the node optional.
    let mut types: Vec<&str> = match &schema["type"] {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    let optional = types.contains(&"null");
    types.retain(|t| *t != "null");
    // No `type` but `properties` / `items` / `additionalProperties` is still an object / array (the guns entry has no `type` keyword in places).
    if types.is_empty() {
        if schema.get("properties").is_some() || schema.get("additionalProperties").is_some() {
            types.push("object");
        } else if schema.get("items").is_some() || schema.get("prefixItems").is_some() {
            types.push("array");
        }
    }
    let kind = match types[..] {
        ["object"] => {
            let hidden = |k: &str| k == "$schema";
            let props: Vec<(String, Node)> =
                schema["properties"].as_object().into_iter().flatten().filter(|(k, _)| !hidden(k)).map(|(k, v)| (k.clone(), resolve(root, v, depth + 1))).collect();
            let additional = match &schema["additionalProperties"] {
                Value::Object(_) => Some(resolve(root, &schema["additionalProperties"], depth + 1)),
                Value::Bool(true) => Some(any(String::new())),
                _ => None,
            };
            match (props.is_empty(), additional) {
                (true, Some(value)) => Kind::Map { value: Box::new(value), hidden: schema["properties"].as_object().into_iter().flatten().map(|(k, _)| k.clone()).collect() },
                // A bare `type: object` (a free-form table): a map of anything.
                (true, None) if schema.get("properties").is_none() => Kind::Map { value: Box::new(any(String::new())), hidden: vec![] },
                _ => Kind::Object { props },
            }
        }
        ["array"] => Kind::Array {
            items: schema.get("items").filter(|i| i.is_object()).map(|i| Box::new(resolve(root, i, depth + 1))),
            prefix: schema["prefixItems"].as_array().into_iter().flatten().map(|p| resolve(root, p, depth + 1)).collect(),
            min: schema["minItems"].as_u64().map(|n| n as usize),
            max: schema["maxItems"].as_u64().map(|n| n as usize),
        },
        ["integer"] | ["number"] => {
            let mut range = Range { min: schema["minimum"].as_f64(), max: schema["maximum"].as_f64(), ..Range::default() };
            if let Some(m) = schema["exclusiveMinimum"].as_f64() {
                range.min = Some(m);
                range.exclusive_min = true;
            }
            if let Some(m) = schema["exclusiveMaximum"].as_f64() {
                range.max = Some(m);
                range.exclusive_max = true;
            }
            if types[0] == "integer" {
                // `format: uint32` / `uint64` / ... means no negative values.
                if range.min.is_none() && schema["format"].as_str().is_some_and(|f| f.starts_with("uint")) {
                    range.min = Some(0.0);
                }
                Kind::Integer(range)
            } else {
                Kind::Number(range)
            }
        }
        ["string"] => Kind::String,
        ["boolean"] => Kind::Boolean,
        _ => Kind::Any,
    };
    done(kind, optional, description, default)
}

/// The values a constant-like schema allows (`const` or a one-value or many-value `enum`).
fn constants(v: &Value) -> Vec<Value> {
    if let Some(c) = v.get("const") { vec![c.clone()] } else { v["enum"].as_array().cloned().unwrap_or_default() }
}

/// A new value for `node`: its `default` if it has one, else an empty value of its kind (`{}`, `[]` or as many zeros as the list's
/// minimum length, the first enum value, the smallest allowed number, `""`, `false`, `null`).
pub fn default_value(node: &Node) -> Value {
    if let Some(d) = &node.default {
        return d.clone();
    }
    match &node.kind {
        Kind::Object { .. } | Kind::Map { .. } => json!({}),
        Kind::Array { items, prefix, min, .. } => {
            let n = min.unwrap_or(0).max(prefix.len());
            Value::Array((0..n).map(|i| prefix.get(i).or(items.as_deref()).map(default_value).unwrap_or(Value::Null)).collect())
        }
        Kind::Enum(values) => values.first().cloned().unwrap_or(Value::Null),
        Kind::Integer(r) | Kind::Number(r) => {
            let mut x = 0.0_f64;
            if let Some(m) = r.min {
                x = x.max(m);
            }
            if let Some(m) = r.max {
                x = x.min(m);
            }
            if matches!(node.kind, Kind::Integer(_)) { json!(x.ceil() as i64) } else { number_value(&node.kind, x, false, false) }
        }
        Kind::String => json!(""),
        Kind::Boolean => json!(false),
        Kind::Any => Value::Null,
    }
}

/// The JSON number for a value typed or dragged into a number field of `kind`. An integer field gives an integer (`1`, not `1.0`).
/// A float from a drag is written as the f32's shortest decimal (the engine reads f32 anyway: `-369.23077`, not `-369.23077392578125`).
/// A whole float stays an integer when `was_int` (the value in the file was written without a fraction).
pub fn number_value(kind: &Kind, x: f64, from_drag: bool, was_int: bool) -> Value {
    if matches!(kind, Kind::Integer(_)) || (x.fract() == 0.0 && x.abs() < 1e15 && was_int) {
        return json!(x.round() as i64);
    }
    if from_drag {
        let short: f64 = (x as f32).to_string().parse().unwrap_or(x);
        return json!(short);
    }
    json!(x)
}

// ---- edits: what a form action turns into -------------------------------------------------------------------------------

/// The pointer of `token` below `parent`.
pub fn child_pointer(parent: &str, token: &str) -> String {
    format!("{parent}/{}", edit::escape(token))
}

/// The pointer's parent and its last token (`""` for the document itself: `None`).
pub fn split_pointer(pointer: &str) -> Option<(String, String)> {
    let mut toks = edit::tokens(pointer);
    let last = toks.pop()?;
    Some((edit::pointer(&toks), last))
}

/// "Set": copy what the parent document has (or else a default) into this document.
pub fn set_from(pointer: &str, value: &Value) -> Edit {
    Edit::set(pointer, value.clone())
}

/// "Reset": remove the key, so the value falls back to the parent (or the default).
pub fn reset(pointer: &str) -> Edit {
    Edit::remove(pointer)
}

/// Adds the entry `name` (value `value`) to the map at `pointer`. Empty when the name is empty or taken.
pub fn map_add(pointer: &str, map: Option<&Value>, name: &str, value: Value) -> Vec<Edit> {
    if name.is_empty() || map.and_then(Value::as_object).is_some_and(|m| m.contains_key(name)) {
        return vec![];
    }
    vec![Edit::set(child_pointer(pointer, name), value)]
}

/// Removes the entry `name` of the map at `pointer`, as one edit that sets the whole map without it, so an undo puts the entry back
/// at its place (a plain key removal is undone at the end of the object, and the order of sections and mounts is meaningful).
pub fn map_remove(pointer: &str, map: &Value, name: &str) -> Vec<Edit> {
    let Some(obj) = map.as_object().filter(|m| m.contains_key(name)) else { return vec![] };
    let rest: Map<String, Value> = obj.iter().filter(|(k, _)| *k != name).map(|(k, v)| (k.clone(), v.clone())).collect();
    vec![Edit::set(pointer, Value::Object(rest))]
}

/// Renames the key `old` of the object at `pointer` to `new`, keeping the order of the keys (the order of sections, mounts and
/// the like is meaningful), as one edit that sets the whole object, so undo is one step. Empty when `new` is empty, taken or `old` is absent.
pub fn rename_key(pointer: &str, object: &Value, old: &str, new: &str) -> Vec<Edit> {
    let Some(obj) = object.as_object() else { return vec![] };
    if new.is_empty() || new == old || obj.contains_key(new) || !obj.contains_key(old) {
        return vec![];
    }
    let renamed: Map<String, Value> = obj.iter().map(|(k, v)| (if k == old { new.to_string() } else { k.clone() }, v.clone())).collect();
    vec![Edit::set(pointer, Value::Object(renamed))]
}

/// The list at `pointer` with `item` appended (one edit that sets the whole list; an absent list starts empty).
pub fn array_add(pointer: &str, array: Option<&Value>, item: Value) -> Vec<Edit> {
    let mut items = array.and_then(Value::as_array).cloned().unwrap_or_default();
    items.push(item);
    vec![Edit::set(pointer, Value::Array(items))]
}

/// The list at `pointer` without element `index`.
pub fn array_remove(pointer: &str, array: &Value, index: usize) -> Vec<Edit> {
    let mut items = array.as_array().cloned().unwrap_or_default();
    if index >= items.len() {
        return vec![];
    }
    items.remove(index);
    vec![Edit::set(pointer, Value::Array(items))]
}

/// The list at `pointer` with element `index` moved by `delta` places (clamped inside the list; empty when it would not move).
pub fn array_move(pointer: &str, array: &Value, index: usize, delta: isize) -> Vec<Edit> {
    let mut items = array.as_array().cloned().unwrap_or_default();
    if index >= items.len() {
        return vec![];
    }
    let to = (index as isize + delta).clamp(0, items.len() as isize - 1) as usize;
    if to == index {
        return vec![];
    }
    let item = items.remove(index);
    items.insert(to, item);
    vec![Edit::set(pointer, Value::Array(items))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{apply, undo};

    fn schema(name: &str) -> Value {
        openprocyon::authoring::schemas().into_iter().find(|(n, _)| *n == name).unwrap().1
    }

    fn node(name: &str) -> Node {
        Node::from_schema(&schema(name))
    }

    fn map_of_objects(n: &Node) -> bool {
        matches!(&n.kind, Kind::Map { value, .. } if matches!(value.kind, Kind::Object { .. }))
    }

    fn array_of_objects(n: &Node) -> bool {
        matches!(&n.kind, Kind::Array { items: Some(i), .. } if matches!(i.kind, Kind::Object { .. }))
    }

    #[test]
    fn the_ship_schema_resolves_to_the_expected_shapes() {
        let ship = node("ship.schema.json");
        assert!(matches!(ship.kind, Kind::Object { .. }));
        assert!(ship.at("/$schema").is_none(), "the editor hint is not a field");
        assert!(map_of_objects(ship.at("/guns").unwrap()));
        assert!(map_of_objects(ship.at("/sections").unwrap()));
        for p in ["/engines", "/wakes", "/flags", "/crew_nodes"] {
            assert!(array_of_objects(ship.at(p).unwrap()), "{p}");
        }
        let tow = ship.at("/tow").unwrap();
        assert!(tow.optional && matches!(tow.kind, Kind::Object { .. }));
        assert!(ship.at("/tow/tower").unwrap().is_short_numbers());
        let homing = ship.at("/homing").unwrap();
        assert!(matches!(&homing.kind, Kind::Array { min: Some(3), max: Some(3), items: Some(i), .. } if matches!(i.kind, Kind::Number(_))));
        assert!(homing.is_short_numbers() && homing.optional);
        let points = ship.at("/points").unwrap();
        assert!(matches!(points.kind, Kind::Integer(Range { min: Some(m), .. }) if m == 0.0), "uint64 means minimum 0: {points:?}");
        assert!(matches!(ship.at("/physics").unwrap().kind, Kind::Object { .. }));
        assert!(ship.at("/physics/MaxSpeed").is_some());
        assert!(!ship.at("/physics").unwrap().description.is_empty(), "the description next to a $ref is kept");
        assert!(matches!(ship.at("/crew_posts").unwrap().kind, Kind::Map { .. }));
        assert!(matches!(ship.at("/guns/Any/arc_deg").unwrap().kind, Kind::Number(_)));
        assert!(matches!(ship.at("/skirmish").unwrap().kind, Kind::Boolean));
    }

    #[test]
    fn guns_bullets_crew_nodes_and_mod_resolve() {
        let guns = node("guns.schema.json");
        assert!(map_of_objects(&guns), "guns.json is a map by gun name");
        assert!(matches!(&guns.kind, Kind::Map { hidden, .. } if hidden == &["$schema"]));
        assert!(guns.at("/Gun_A/Mount Type").is_some());
        assert!(guns.at("/Gun_A/visual/muzzles").is_some());
        let bullets = node("bullets.schema.json");
        assert!(map_of_objects(&bullets));
        let damage = bullets.at("/B/damage").unwrap();
        assert!(matches!(&damage.kind, Kind::Map { value, .. } if matches!(&value.kind, Kind::Array { items: Some(i), .. } if matches!(i.kind, Kind::Number(_)))));
        assert!(matches!(&bullets.at("/B/impact_snd").unwrap().kind, Kind::Map { .. }));
        assert!(matches!(bullets.at("/B/blast").unwrap().kind, Kind::Any), "a schema with no type is raw JSON");
        let crew = node("crew_nodes.schema.json");
        assert!(matches!(&crew.kind, Kind::Map { value, .. } if array_of_objects(value)));
        let m = node("mod.schema.json");
        assert!(matches!(&m.at("/requires").unwrap().kind, Kind::Array { items: Some(i), .. } if matches!(i.kind, Kind::String)));
        assert!(matches!(m.at("/format").unwrap().kind, Kind::Integer(_)));
    }

    fn walk_all(n: &Node, seen: &mut usize) {
        *seen += 1;
        match &n.kind {
            Kind::Object { props } => props.iter().for_each(|(_, c)| walk_all(c, seen)),
            Kind::Map { value, .. } => walk_all(value, seen),
            Kind::Array { items, prefix, .. } => {
                items.iter().for_each(|i| walk_all(i, seen));
                prefix.iter().for_each(|i| walk_all(i, seen));
            }
            _ => {}
        }
        // every node can give a default without panicking
        let _ = default_value(n);
    }

    fn no_ref(v: &Value, root: &Value) {
        match v {
            Value::Object(m) => {
                if let Some(r) = m.get("$ref").and_then(Value::as_str) {
                    assert!(root.pointer(r.trim_start_matches('#')).is_some(), "unresolved $ref {r}");
                }
                m.values().for_each(|c| no_ref(c, root));
            }
            Value::Array(a) => a.iter().for_each(|c| no_ref(c, root)),
            _ => {}
        }
    }

    #[test]
    fn every_node_of_every_schema_resolves() {
        for (name, s) in openprocyon::authoring::schemas() {
            no_ref(&s, &s);
            let mut seen = 0;
            walk_all(&Node::from_schema(&s), &mut seen);
            assert!(seen > 3, "{name}");
        }
    }

    #[test]
    fn a_cyclic_ref_gives_up_instead_of_looping() {
        let s = json!({"$defs": {"T": {"type": "object", "properties": {"next": {"$ref": "#/$defs/T"}}}}, "$ref": "#/$defs/T"});
        let n = Node::from_schema(&s);
        assert!(matches!(n.kind, Kind::Object { .. }));
        let s = json!({"$ref": "#/$defs/missing"});
        assert!(matches!(Node::from_schema(&s).kind, Kind::Any));
    }

    #[test]
    fn enums_optionals_and_ranges() {
        let s = json!({"anyOf": [{"enum": ["a", "b"]}, {"type": "null"}], "description": "d"});
        let n = Node::from_schema(&s);
        assert!(n.optional && n.kind == Kind::Enum(vec![json!("a"), json!("b")]) && n.description == "d");
        let s = json!({"oneOf": [{"const": "x", "description": "X"}, {"const": "y"}]});
        assert_eq!(Node::from_schema(&s).kind, Kind::Enum(vec![json!("x"), json!("y")]));
        let s = json!({"type": ["number", "null"], "exclusiveMinimum": 0, "maximum": 5, "default": 2});
        let n = Node::from_schema(&s);
        assert!(n.optional && n.default == Some(json!(2)));
        assert_eq!(n.kind, Kind::Number(Range { min: Some(0.0), max: Some(5.0), exclusive_min: true, exclusive_max: false }));
        let s = json!({"type": "object", "description": "free"});
        assert!(matches!(Node::from_schema(&s).kind, Kind::Map { .. }));
        let s = json!({"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}], "minItems": 2});
        let n = Node::from_schema(&s);
        assert_eq!(default_value(&n), json!(["", 0]));
        assert!(matches!(n.at("/1").unwrap().kind, Kind::Integer(_)));
    }

    #[test]
    fn numbers_keep_their_json_type() {
        let int = Kind::Integer(Range::default());
        let num = Kind::Number(Range::default());
        assert_eq!(number_value(&int, 3.0, true, false).to_string(), "3");
        assert_eq!(number_value(&num, 3.0, true, true).to_string(), "3", "a whole value that was an integer stays one");
        assert_eq!(number_value(&num, 3.0, true, false).to_string(), "3.0");
        assert_eq!(number_value(&num, -369.230_773_925_781_25, true, false).to_string(), "-369.23077");
        assert_eq!(number_value(&num, 0.1, false, false).to_string(), "0.1");
    }

    fn fixture() -> Value {
        json!({"guns": {"A": {"arc_deg": 90}, "B": {"arc_deg": 45}, "C": {}}, "l": [1, 2, 3], "n": 1})
    }

    /// Applies `edits` in order, checks `check`, then undoes them in reverse and checks the original is back.
    fn roundtrip(edits: Vec<Edit>, check: impl Fn(&Value)) {
        let mut doc = fixture();
        let orig = doc.clone();
        let applied: Vec<Edit> = edits.iter().map(|e| apply(&mut doc, e).unwrap()).collect();
        check(&doc);
        for e in applied.iter().rev() {
            undo(&mut doc, e).unwrap();
        }
        assert_eq!(doc, orig);
        assert_eq!(serde_json::to_string(&doc).unwrap(), serde_json::to_string(&orig).unwrap(), "key order restored");
    }

    #[test]
    fn set_and_reset() {
        roundtrip(vec![set_from("/guns/A/pos", &json!([1, 2, 3]))], |d| assert_eq!(d["guns"]["A"]["pos"], json!([1, 2, 3])));
        roundtrip(vec![reset("/guns/A/arc_deg")], |d| assert!(d["guns"]["A"].get("arc_deg").is_none()));
    }

    #[test]
    fn map_add_remove_rename() {
        let doc = fixture();
        let guns = &doc["guns"];
        roundtrip(map_add("/guns", Some(guns), "D", json!({})), |d| assert_eq!(d["guns"]["D"], json!({})));
        assert!(map_add("/guns", Some(guns), "A", json!({})).is_empty());
        assert!(map_add("/guns", Some(guns), "", json!({})).is_empty());
        roundtrip(map_remove("/guns", guns, "B"), |d| assert!(d["guns"].get("B").is_none()));
        roundtrip(rename_key("/guns", guns, "B", "Z"), |d| {
            let keys: Vec<&String> = d["guns"].as_object().unwrap().keys().collect();
            assert_eq!(keys, ["A", "Z", "C"], "order kept");
            assert_eq!(d["guns"]["Z"]["arc_deg"], 45);
        });
        assert!(rename_key("/guns", guns, "B", "A").is_empty());
        assert!(rename_key("/guns", guns, "nope", "X").is_empty());
        // the root object can be renamed in
        let root = fixture();
        let edits = rename_key("", &root, "n", "m");
        let mut d = root.clone();
        apply(&mut d, &edits[0]).unwrap();
        assert!(d.get("m").is_some() && d.get("n").is_none());
    }

    #[test]
    fn array_add_remove_move() {
        let doc = fixture();
        let l = &doc["l"];
        roundtrip(array_add("/l", Some(l), json!(4)), |d| assert_eq!(d["l"], json!([1, 2, 3, 4])));
        roundtrip(array_add("/new", None, json!("x")), |d| assert_eq!(d["new"], json!(["x"])));
        roundtrip(array_remove("/l", l, 1), |d| assert_eq!(d["l"], json!([1, 3])));
        assert!(array_remove("/l", l, 9).is_empty());
        roundtrip(array_move("/l", l, 0, 1), |d| assert_eq!(d["l"], json!([2, 1, 3])));
        roundtrip(array_move("/l", l, 2, -2), |d| assert_eq!(d["l"], json!([3, 1, 2])));
        assert!(array_move("/l", l, 0, -1).is_empty());
        assert!(array_move("/l", l, 2, 5).is_empty());
    }

    #[test]
    fn pointers_split() {
        assert_eq!(split_pointer("/a/b~1c"), Some(("/a".to_string(), "b/c".to_string())));
        assert_eq!(split_pointer("/a"), Some((String::new(), "a".to_string())));
        assert_eq!(split_pointer(""), None);
        assert_eq!(child_pointer("/a", "x/y"), "/a/x~1y");
    }
}
