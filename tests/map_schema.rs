//! The editor's key names for `map.json` (src/map/doc.rs, src/map/settings.rs) held to the engine's `map.schema.json`
//! (`authoring::schemas()`, what `--dump-schemas` writes): a key the editor writes or reads that the schema does not have fails here.
//! Synthetic data only.

use openprocyon::authoring::schemas;
use openprocyon_editor::form::model::{Kind, Node};
use openprocyon_editor::map::doc::{self, LightColour, PlayerField};
use serde_json::{Value, json};

fn is_numbers_node(n: &Node) -> bool {
    matches!(&n.kind, Kind::Array { items: Some(i), .. } if matches!(i.kind, Kind::Number(_) | Kind::Integer(_)))
}

trait IsNumbers {
    fn is_numbers(&self) -> bool;
}
impl IsNumbers for Node {
    fn is_numbers(&self) -> bool {
        is_numbers_node(self)
    }
}

fn schema() -> Node {
    let s = schemas().into_iter().find(|(n, _)| *n == "map.schema.json").expect("the engine has map.schema.json").1;
    Node::from_schema(&s)
}

/// Every object key of `v` must be a property of `node`; arrays use their `items`. Lists the keys that are not.
fn unknown(node: &Node, v: &Value, at: &str, out: &mut Vec<String>) {
    match (&node.kind, v) {
        (Kind::Object { props }, Value::Object(o)) => {
            for (k, x) in o {
                match props.iter().find(|(p, _)| p == k) {
                    Some((_, n)) => unknown(n, x, &format!("{at}/{k}"), out),
                    None => out.push(format!("{at}/{k}")),
                }
            }
        }
        (Kind::Array { items: Some(item), .. }, Value::Array(a)) => {
            for (i, x) in a.iter().enumerate() {
                unknown(item, x, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn every_key_the_editor_writes_into_a_map_is_in_the_engines_schema() {
    let mut map = doc::new_map("Synthetic");
    map["description"] = json!("d");
    map["skirmish"] = json!(true);
    map["sky"] = json!(doc::DEFAULT_SKY);
    map["backdrop"] = json!("m");
    map["lighting"] = doc::default_lighting();
    map["players"] = json!([doc::new_player("P", 0, [1.0, 2.0, 3.0])]);
    map["teams"] = json!([doc::new_team("T")]);
    map["nebulae"] = json!([doc::new_nebula("N", &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], [0.5, 0.5], 10.0)]);
    map["mines"] = json!([doc::new_mine("Mine_A", [1.0, 2.0])]);
    // the keys the readers look at, on entries of the three id lists
    map["objects"] = json!([{"id": 1, "type": "T", "scene": "S", "pos": [0, 0, 0], "rot": [0, 0, 0, 0, 0, 0, 0, 0, 0]}]);
    map["ships"] = json!([{"id": 2, "type": "T", "player": 0, "pos": [0, 0, 0]}]);
    let mut bad = vec![];
    unknown(&schema(), &map, "", &mut bad);
    assert!(bad.is_empty(), "keys the editor uses that map.schema.json does not have: {bad:?}");
}

#[test]
fn the_keys_the_settings_form_edits_are_there_with_the_type_it_edits_them_as() {
    let s = schema();
    let at = |ptr: &str| s.at(ptr).unwrap_or_else(|| panic!("map.schema.json has no {ptr}"));
    for key in [doc::KEY_NAME, doc::KEY_DESCRIPTION, doc::KEY_SKY] {
        assert_eq!(at(&format!("/{key}")).kind, Kind::String, "{key}");
    }
    assert!(matches!(at(&format!("/{}", doc::KEY_WORLD_HALF)).kind, Kind::Number(_)));
    assert!(matches!(at(&format!("/{}", doc::KEY_LIGHTING)).kind, Kind::Object { .. }));
    assert!(at(&format!("/{}", doc::KEY_BACKDROP)).description.contains("backdrop.png"), "the schema says where the picture is");
    for which in LightColour::ALL {
        assert!(at(&format!("/{}/{}", doc::KEY_LIGHTING, which.key())).is_numbers(), "{}", which.key());
    }
    assert!(at(&format!("/{}/{}", doc::KEY_LIGHTING, doc::KEY_LIGHT_DIRECTION)).is_numbers());
    assert_eq!(at("/skirmish").kind, Kind::Boolean);
    for which in [PlayerField::Name, PlayerField::Team, PlayerField::Playable, PlayerField::Race, PlayerField::Formation] {
        assert!(s.at(&format!("/players/0/{}", which.key())).is_some(), "players[].{}", which.key());
    }
    assert!(s.at(&format!("/teams/0/{}", doc::TEAM_NAME_KEY)).is_some());
    assert!(s.at(&format!("/ships/0/{}", doc::SHIP_PLAYER_KEY)).is_some());
    for key in [doc::NEBULA_POLYGON, doc::NEBULA_NAME, doc::NEBULA_SOUND_DISTANCE] {
        assert!(s.at(&format!("/nebulae/0/{key}")).is_some(), "nebulae[].{key}");
    }
}
