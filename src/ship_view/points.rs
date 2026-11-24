//! The hardpoints of a ship as the loader will see them: `ship.json` over the model's nodes. Pure: it reads a `ship.json` value and
//! the model's [`Facts`], and says what each point is, where it is and where it can be edited.
//!
//! PROVISIONAL: this mirrors the override rules of docs/data-format.md "ship.json keys" and docs/modding.md "A ship from a model";
//! engine requirement: authoring: resolved hardpoints of a ship, as the loader sees them.

use super::coords::read_triple;
use crate::edit::escape;
use openprocyon::data::derive::{Facts, Point};
use serde_json::Value;

/// What a crew place is for, from its name `Crew_<Kind>_...`. PROVISIONAL: a mirror of the engine's private list of kinds;
/// engine requirement: authoring: the crew node kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CrewKind {
    Walk,
    Gun,
    Post,
    Work,
    Climb,
    Count,
}

impl CrewKind {
    /// Every kind, in the order a picker lists them.
    pub const ALL: [CrewKind; 6] = [CrewKind::Walk, CrewKind::Gun, CrewKind::Post, CrewKind::Work, CrewKind::Climb, CrewKind::Count];

    /// The word in the node name: `Walk` for `Crew_Walk_P_0`.
    pub fn word(self) -> &'static str {
        match self {
            CrewKind::Walk => "Walk",
            CrewKind::Gun => "Gun",
            CrewKind::Post => "Post",
            CrewKind::Work => "Work",
            CrewKind::Climb => "Climb",
            CrewKind::Count => "Count",
        }
    }

    /// The kind a node name spells (`Crew_Gun_A` is `Gun`), or `None` when it is no crew node name.
    pub fn of_name(name: &str) -> Option<CrewKind> {
        let rest = name.strip_prefix("Crew_")?;
        let word = rest.split_once('_').map_or(rest, |s| s.0);
        CrewKind::ALL.into_iter().find(|k| k.word() == word)
    }
}

/// What a hardpoint is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Gun,
    Engine,
    Wake,
    Flag,
    /// Where this ship holds the tow line.
    TowTower,
    /// Where the line fastens when another ship tows this one.
    TowTowee,
    Homing,
    /// Read-only: only the model has boarding points.
    Boarding,
    Crew(CrewKind),
}

impl Kind {
    /// The name shown on buttons and labels.
    pub fn title(self) -> String {
        match self {
            Kind::Gun => "gun".into(),
            Kind::Engine => "engine".into(),
            Kind::Wake => "wake".into(),
            Kind::Flag => "flag".into(),
            Kind::TowTower => "tow (tower)".into(),
            Kind::TowTowee => "tow (towee)".into(),
            Kind::Homing => "homing".into(),
            Kind::Boarding => "boarding".into(),
            Kind::Crew(k) => format!("crew {}", k.word().to_lowercase()),
        }
    }
}

/// Where a point is stored.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// An entry of `ship.json`: the JSON pointer of the entry (`/guns/Gun_A`, `/engines/2`, `/tow/tower`, `/homing`, `/crew_nodes/0`).
    Json(String),
    /// Only the model has it: the node's name.
    Model(String),
}

/// One point of the ship, game axes, ship frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Hardpoint {
    pub kind: Kind,
    /// The mount's label for a gun, the entry's label or node name otherwise.
    pub label: String,
    pub pos: [f64; 3],
    /// A `rot` triple (the entry's, or a model node's yaw as `[yaw, 0, 0]`).
    pub rot: Option<[f64; 3]>,
    /// A gun's firing arc, degrees.
    pub arc_deg: Option<f64>,
    pub source: Source,
}

fn model_points<'a>(facts: Option<&'a Facts>, kind: &'a str) -> impl Iterator<Item = &'a Point> {
    facts.into_iter().flat_map(|f| f.points.iter()).filter(move |p| p.kind == kind)
}

/// A model node as a point. Tow ends and homing have one place each, so their label is the key (`tower`, `towee`, `homing`) whether
/// ship.json or the model holds them: a point keeps its identity when a move writes it into ship.json.
fn from_node(kind: Kind, p: &Point) -> Hardpoint {
    let label = match kind {
        Kind::TowTower => "tower".to_string(),
        Kind::TowTowee => "towee".to_string(),
        Kind::Homing => "homing".to_string(),
        _ => p.name.clone(),
    };
    Hardpoint { kind, label, pos: p.pos, rot: Some([p.yaw, 0.0, 0.0]), arc_deg: None, source: Source::Model(p.name.clone()) }
}

/// The model node of a gun mount's label (the engine matches case-insensitively).
pub fn gun_node<'a>(facts: Option<&'a Facts>, label: &str) -> Option<&'a Point> {
    model_points(facts, "GunPlacement").find(|p| p.name.eq_ignore_ascii_case(label))
}

/// The points of an array key (`engines`, `wakes`, `flags`) of `ship.json`, if the key is present: the array REPLACES the model's nodes
/// of that kind, so its entries are the only ones then. Entries without a readable `pos` are not points.
fn array_points(ship: &Value, key: &str, kind: Kind, facts: Option<&Facts>, model_kind: &str) -> Vec<Hardpoint> {
    let Some(list) = ship.get(key).and_then(Value::as_array) else {
        return model_points(facts, model_kind).map(|p| from_node(kind, p)).collect();
    };
    list.iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let pos = read_triple(&e["pos"])?;
            let label = e["label"].as_str().map_or_else(|| format!("{} {i}", kind.title()), str::to_string);
            Some(Hardpoint { kind, label, pos, rot: read_triple(&e["rot"]), arc_deg: None, source: Source::Json(format!("/{key}/{i}")) })
        })
        .collect()
}

/// Every point of the ship: `ship.json` entries first (they win), then what only the model has.
///
/// - `guns`: an entry with a `pos` is a JSON point; an entry without one uses the model node of its name (case-insensitive), and a
///   model gun node that no entry names is listed too.
/// - `engines`, `wakes`, `flags`: the array replaces the model's nodes of that kind when present.
/// - `tow` and `homing` win over the model's nodes; `crew_nodes` are JSON points (crew nodes in the model are not in `Facts`:
///   PROVISIONAL, engine requirement: crew nodes in `derive::Facts`); boarding points are the model's alone.
pub fn hardpoints(ship: &Value, facts: Option<&Facts>) -> Vec<Hardpoint> {
    let mut out = vec![];
    let mut named: Vec<String> = vec![];
    if let Some(guns) = ship.get("guns").and_then(Value::as_object) {
        for (label, g) in guns {
            named.push(label.to_lowercase());
            let arc_deg = g["arc_deg"].as_f64();
            if let Some(pos) = read_triple(&g["pos"]) {
                out.push(Hardpoint { kind: Kind::Gun, label: label.clone(), pos, rot: read_triple(&g["rot"]), arc_deg, source: Source::Json(format!("/guns/{}", escape(label))) });
            } else if let Some(n) = gun_node(facts, label) {
                out.push(Hardpoint {
                    kind: Kind::Gun,
                    label: label.clone(),
                    pos: n.pos,
                    rot: read_triple(&g["rot"]).or(Some([n.yaw, 0.0, 0.0])),
                    arc_deg,
                    source: Source::Model(n.name.clone()),
                });
            }
        }
    }
    for n in model_points(facts, "GunPlacement").filter(|n| !named.contains(&n.name.to_lowercase())) {
        out.push(from_node(Kind::Gun, n));
    }
    out.extend(array_points(ship, "engines", Kind::Engine, facts, "EnginePortPlacement"));
    out.extend(array_points(ship, "wakes", Kind::Wake, facts, "WakePlacement"));
    out.extend(array_points(ship, "flags", Kind::Flag, facts, "FlagAttachmentPoint"));
    if let Some(tow) = ship.get("tow").filter(|t| t.is_object()) {
        for (key, kind) in [("tower", Kind::TowTower), ("towee", Kind::TowTowee)] {
            if let Some(pos) = read_triple(&tow[key]) {
                out.push(Hardpoint { kind, label: key.into(), pos, rot: None, arc_deg: None, source: Source::Json(format!("/tow/{key}")) });
            }
        }
    } else {
        out.extend(model_points(facts, "TowerPoint").take(1).map(|p| from_node(Kind::TowTower, p)));
        out.extend(model_points(facts, "ToweePoint").take(1).map(|p| from_node(Kind::TowTowee, p)));
    }
    match read_triple(&ship["homing"]) {
        Some(pos) => out.push(Hardpoint { kind: Kind::Homing, label: "homing".into(), pos, rot: None, arc_deg: None, source: Source::Json("/homing".into()) }),
        None => out.extend(model_points(facts, "TorpedoHomingPoint").take(1).map(|p| from_node(Kind::Homing, p))),
    }
    out.extend(model_points(facts, "BoardingEffectPoint").map(|p| from_node(Kind::Boarding, p)));
    if let Some(list) = ship.get("crew_nodes").and_then(Value::as_array) {
        for (i, e) in list.iter().enumerate() {
            let (Some(name), Some(pos)) = (e["name"].as_str(), read_triple(&e["pos"])) else { continue };
            let Some(kind) = CrewKind::of_name(name) else { continue };
            out.push(Hardpoint { kind: Kind::Crew(kind), label: name.into(), pos, rot: read_triple(&e["rot"]), arc_deg: None, source: Source::Json(format!("/crew_nodes/{i}")) });
        }
    }
    out
}

/// The `sections` entry whose `layer` is the mesh node `layer`: its label and the entry.
pub fn section_for_layer<'a>(ship: &'a Value, layer: &str) -> Option<(&'a str, &'a Value)> {
    ship.get("sections")?.as_object()?.iter().find(|(_, s)| s["layer"].as_str() == Some(layer)).map(|(k, v)| (k.as_str(), v))
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use openprocyon::data::derive::{Layer, Point};

    pub fn point(kind: &str, name: &str, pos: [f64; 3], yaw: f64) -> Point {
        Point { kind: kind.into(), inferred: false, name: name.into(), pos, yaw }
    }

    /// A model with one hull layer (a unit triangle) and one node of each kind.
    pub fn facts() -> Facts {
        Facts {
            layers: vec![Layer { name: "Hull_Main".into(), aabb: [[-1.0, 0.0, 0.0], [1.0, 0.0, 1.0]], tris: vec![-1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] }],
            points: vec![
                point("GunPlacement", "Gun_Mount_A", [1.0, 2.0, 3.0], 0.5),
                point("GunPlacement", "Gun_Mount_B", [-1.0, 2.0, 3.0], 0.0),
                point("EnginePortPlacement", "Engine_1", [0.0, -5.0, 1.0], 0.0),
                point("WakePlacement", "Wake_1", [0.0, -4.0, 0.0], 0.0),
                point("FlagAttachmentPoint", "Flag_Top", [0.0, 0.0, 9.0], 0.0),
                point("FlagAttachmentPoint", "Flag_Bottom", [0.0, 0.0, 7.0], 0.0),
                point("BoardingEffectPoint", "Boarding_1", [2.0, 0.0, 1.0], 0.0),
                point("TowerPoint", "Tow_Back", [0.0, -6.0, 1.0], 0.0),
                point("ToweePoint", "Tow_Front", [0.0, 6.0, 1.0], 0.0),
                point("TorpedoHomingPoint", "Homing", [0.0, 0.0, 2.0], 0.0),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use serde_json::json;

    fn find<'a>(hp: &'a [Hardpoint], kind: Kind, label: &str) -> &'a Hardpoint {
        hp.iter().find(|h| h.kind == kind && h.label == label).unwrap_or_else(|| panic!("no {kind:?} {label}: {hp:?}"))
    }

    #[test]
    fn a_ship_with_no_json_shows_the_models_nodes_of_every_kind() {
        let f = facts();
        let hp = hardpoints(&json!({}), Some(&f));
        let a = find(&hp, Kind::Gun, "Gun_Mount_A");
        assert_eq!((a.pos, a.rot, &a.source), ([1.0, 2.0, 3.0], Some([0.5, 0.0, 0.0]), &Source::Model("Gun_Mount_A".into())));
        assert_eq!(find(&hp, Kind::Engine, "Engine_1").source, Source::Model("Engine_1".into()));
        find(&hp, Kind::Wake, "Wake_1");
        assert_eq!(hp.iter().filter(|h| h.kind == Kind::Flag).count(), 2);
        assert_eq!(find(&hp, Kind::TowTower, "tower").pos, [0.0, -6.0, 1.0]);
        assert_eq!(find(&hp, Kind::TowTowee, "towee").pos, [0.0, 6.0, 1.0]);
        assert_eq!(find(&hp, Kind::Homing, "homing").pos, [0.0, 0.0, 2.0]);
        assert_eq!(find(&hp, Kind::Boarding, "Boarding_1").source, Source::Model("Boarding_1".into()));
        assert_eq!(hp.len(), 10);
    }

    #[test]
    fn no_facts_and_no_json_is_no_points() {
        assert!(hardpoints(&json!({}), None).is_empty());
    }

    #[test]
    fn a_gun_entry_with_pos_wins_and_one_without_uses_the_node_of_its_name() {
        let f = facts();
        let ship = json!({"guns": {
            "Gun_Mount_A": {"gun": "G", "arc_deg": 90, "pos": [5, 5, 5], "rot": [0.25, 0, 0]},
            "gun_mount_b": {"gun": "G", "arc_deg": 45},
            "Gun_Free": {"gun": "G", "arc_deg": 45},
            "A/B": {"gun": "G", "arc_deg": 10, "pos": [0, 0, 0]}
        }});
        let hp = hardpoints(&ship, Some(&f));
        let a = find(&hp, Kind::Gun, "Gun_Mount_A");
        assert_eq!((a.pos, a.rot, a.arc_deg, &a.source), ([5.0, 5.0, 5.0], Some([0.25, 0.0, 0.0]), Some(90.0), &Source::Json("/guns/Gun_Mount_A".into())));
        let b = find(&hp, Kind::Gun, "gun_mount_b");
        assert_eq!((b.pos, b.arc_deg, &b.source), ([-1.0, 2.0, 3.0], Some(45.0), &Source::Model("Gun_Mount_B".into())));
        assert!(hp.iter().all(|h| h.label != "Gun_Free"), "no pos and no node: nothing to show");
        assert_eq!(find(&hp, Kind::Gun, "A/B").source, Source::Json("/guns/A~1B".into()), "labels are escaped in pointers");
        assert_eq!(hp.iter().filter(|h| h.kind == Kind::Gun).count(), 3, "the node of an entry is not listed twice");
    }

    #[test]
    fn model_gun_nodes_no_entry_names_are_still_listed() {
        let f = facts();
        let hp = hardpoints(&json!({"guns": {"Gun_Mount_A": {"gun": "G", "arc_deg": 1}}}), Some(&f));
        assert_eq!(find(&hp, Kind::Gun, "Gun_Mount_B").source, Source::Model("Gun_Mount_B".into()));
    }

    #[test]
    fn engines_wakes_and_flags_arrays_replace_the_models_nodes() {
        let f = facts();
        let ship = json!({
            "engines": [{"label": "Port", "pos": [-1, -5, 1], "rot": [0.1, 0, 0], "fx": "E"}, {"pos": [1, -5, 1]}, {"label": "no pos"}],
            "wakes": [],
            "flags": [{"label": "Flag_Top", "pos": [0, 0, 10]}]
        });
        let hp = hardpoints(&ship, Some(&f));
        let engines: Vec<_> = hp.iter().filter(|h| h.kind == Kind::Engine).collect();
        assert_eq!(engines.len(), 2, "the model's Engine_1 is replaced; an entry with no pos is no point");
        assert_eq!((engines[0].label.as_str(), engines[0].rot, &engines[0].source), ("Port", Some([0.1, 0.0, 0.0]), &Source::Json("/engines/0".into())));
        assert_eq!((engines[1].label.as_str(), &engines[1].source), ("engine 1", &Source::Json("/engines/1".into())));
        assert!(hp.iter().all(|h| h.kind != Kind::Wake), "an empty array removes the model's wakes");
        let flags: Vec<_> = hp.iter().filter(|h| h.kind == Kind::Flag).collect();
        assert_eq!((flags.len(), flags[0].pos), (1, [0.0, 0.0, 10.0]));
    }

    #[test]
    fn tow_and_homing_in_json_win_over_the_nodes() {
        let f = facts();
        let hp = hardpoints(&json!({"tow": {"tower": [0, -9, 1], "towee": [0, 9, 1]}, "homing": [1, 2, 3]}), Some(&f));
        let tower = find(&hp, Kind::TowTower, "tower");
        assert_eq!((tower.pos, &tower.source), ([0.0, -9.0, 1.0], &Source::Json("/tow/tower".into())));
        assert_eq!(find(&hp, Kind::TowTowee, "towee").source, Source::Json("/tow/towee".into()));
        let h = find(&hp, Kind::Homing, "homing");
        assert_eq!((h.pos, &h.source), ([1.0, 2.0, 3.0], &Source::Json("/homing".into())));
        assert_eq!(hp.iter().filter(|h| matches!(h.kind, Kind::TowTower | Kind::TowTowee | Kind::Homing)).count(), 3);
    }

    #[test]
    fn crew_nodes_are_json_points_by_kind_and_bad_names_are_skipped() {
        let ship = json!({"crew_nodes": [
            {"name": "Crew_Walk_P_0", "pos": [0, 1, 2]},
            {"name": "Crew_Gun_A", "pos": [1, 1, 2], "rot": [0.5, 0, 0]},
            {"name": "Crew_Bogus_A", "pos": [0, 0, 0]},
            {"name": "Crew_Post_X"}
        ]});
        let hp = hardpoints(&ship, None);
        assert_eq!(hp.len(), 2);
        assert_eq!((hp[0].kind, hp[0].source.clone()), (Kind::Crew(CrewKind::Walk), Source::Json("/crew_nodes/0".into())));
        assert_eq!((hp[1].kind, hp[1].rot), (Kind::Crew(CrewKind::Gun), Some([0.5, 0.0, 0.0])));
        assert_eq!(CrewKind::of_name("Crew_Count_3"), Some(CrewKind::Count));
        assert_eq!(CrewKind::of_name("Crew_Climb"), Some(CrewKind::Climb));
        assert_eq!(CrewKind::of_name("Gun_Crew"), None);
    }

    #[test]
    fn the_section_of_a_layer_is_found_by_its_layer_key() {
        let ship = json!({"sections": {"Hull Top": {"layer": "Hull_Main", "hp": 100}, "Sail": {"layer": "Sail_A", "hp": 5}}});
        let (label, s) = section_for_layer(&ship, "Hull_Main").unwrap();
        assert_eq!((label, s["hp"].as_i64()), ("Hull Top", Some(100)));
        assert!(section_for_layer(&ship, "Nope").is_none());
        assert!(section_for_layer(&json!({}), "Hull_Main").is_none());
    }
}
