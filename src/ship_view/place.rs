//! Moving and adding hardpoints as edits of `ship.json` (docs/roadmap.md principle 7: placement lives in the editor and is saved to
//! `ship.json`, never written into the GLB). Pure: a ship value and the model's facts in, edits (or the reason there are none) out.
//!
//! Where the engine derives a value the editor cannot know, the editor does not guess: the move is refused with a message.
//! PROVISIONAL: engine requirement: a ship.json way to move one derived point without restating all of the array.

use super::coords::pos_value;
use super::points::{CrewKind, Hardpoint, Kind, Source, hardpoints};
use crate::edit::{Edit, escape};
use openprocyon::data::derive::Facts;
use serde_json::{Value, json};

/// The edits that put `hp` at `new_pos` (game axes). One drag is one call, so one undo step.
pub fn move_point(ship: &Value, facts: Option<&Facts>, hp: &Hardpoint, new_pos: [f64; 3]) -> Result<Vec<Edit>, String> {
    if !new_pos.iter().all(|v| v.is_finite()) {
        return Err("the position must be finite numbers".into());
    }
    match &hp.source {
        Source::Json(ptr) => Ok(vec![match hp.kind {
            Kind::TowTower | Kind::TowTowee | Kind::Homing => Edit::set(ptr.clone(), pos_value(new_pos)),
            _ => Edit::set(format!("{ptr}/pos"), pos_value(new_pos)),
        }]),
        Source::Model(_) => from_model(ship, facts, hp, new_pos),
    }
}

/// A point only the model has: write the `ship.json` entry that wins over the node.
fn from_model(ship: &Value, facts: Option<&Facts>, hp: &Hardpoint, new_pos: [f64; 3]) -> Result<Vec<Edit>, String> {
    let key = |k: &str| format!("the model's {k} nodes cannot be moved one by one: ship.json's {k} array replaces all of them and needs values only the engine derives");
    match hp.kind {
        Kind::Gun => {
            let base = format!("/guns/{}", escape(&hp.label));
            let mut edits = vec![Edit::set(format!("{base}/pos"), pos_value(new_pos))];
            // An entry needs both pos and rot to bypass the node.
            if ship.pointer(&format!("{base}/rot")).is_none() {
                edits.push(Edit::set(format!("{base}/rot"), pos_value(hp.rot.unwrap_or([0.0; 3]))));
            }
            Ok(edits)
        }
        Kind::Homing => Ok(vec![Edit::set("/homing", pos_value(new_pos))]),
        Kind::TowTower | Kind::TowTowee => Ok(vec![tow(ship, facts, hp.kind, new_pos)?]),
        Kind::Flag => {
            // Flags need only label and pos, so the whole array can be written from the nodes.
            let mut list = flags_from_model(facts);
            let at = list.iter().position(|e| e["label"] == hp.label.as_str()).ok_or_else(|| format!("flag {} is not in the model", hp.label))?;
            list[at]["pos"] = pos_value(new_pos);
            Ok(vec![Edit::set("/flags", Value::Array(list))])
        }
        Kind::Engine => {
            // The engine fills an engines entry's missing pos, rot and fx from the model node of the same label (game/src/data/ships/derive.rs:316-324),
            // so the whole array can be written as labels, with the moved one carrying its pos.
            let mut list = engines_from_model(facts);
            let at = list.iter().position(|e| e["label"] == hp.label.as_str()).ok_or_else(|| format!("engine {} is not in the model", hp.label))?;
            list[at]["pos"] = pos_value(new_pos);
            Ok(vec![Edit::set("/engines", Value::Array(list))])
        }
        Kind::Wake => Err(key("wake")),
        Kind::Boarding => {
            Err("boarding points come only from the model; ship.json has no key for them (PROVISIONAL, engine requirement: a ship.json key for boarding points)".into())
        }
        Kind::Crew(_) => Err("crew places are ship.json entries; this one has none".into()),
    }
}

/// One `{"label"}` entry per model engine port: the engine derives pos, rot, fx and layer of an entry from the node of that name.
fn engines_from_model(facts: Option<&Facts>) -> Vec<Value> {
    facts.into_iter().flat_map(|f| f.points.iter()).filter(|p| p.kind == "EnginePortPlacement").map(|p| json!({"label": p.name})).collect()
}

/// Why a click-to-add of `kind` would be refused right now, whatever the position: shown before the click. `None` when it can work.
pub fn add_refusal(ship: &Value, facts: Option<&Facts>, kind: Kind) -> Option<String> {
    add_point(ship, facts, kind, "probe", [0.0; 3]).err()
}

fn flags_from_model(facts: Option<&Facts>) -> Vec<Value> {
    facts.into_iter().flat_map(|f| f.points.iter()).filter(|p| p.kind == "FlagAttachmentPoint").map(|p| json!({"label": p.name, "pos": pos_value(p.pos)})).collect()
}

/// The `/tow` edit that sets one end; the other end is the ship's `tow`, else the model's node, else the edit is refused (the hull ends
/// the engine falls back to are not known here).
fn tow(ship: &Value, facts: Option<&Facts>, which: Kind, pos: [f64; 3]) -> Result<Edit, String> {
    let (this, other) = if which == Kind::TowTower { ("tower", Kind::TowTowee) } else { ("towee", Kind::TowTower) };
    let other_key = if this == "tower" { "towee" } else { "tower" };
    let existing = hardpoints(ship, facts).into_iter().find(|h| h.kind == other).map(|h| h.pos);
    let Some(other_pos) = existing else {
        return Err(format!("the {other_key} point is not in ship.json or the model, and ship.json's tow needs both ends (the engine's hull-end default is not known here)"));
    };
    let mut tow = serde_json::Map::new();
    let (tower, towee) = if this == "tower" { (pos, other_pos) } else { (other_pos, pos) };
    tow.insert("tower".into(), pos_value(tower));
    tow.insert("towee".into(), pos_value(towee));
    Ok(Edit::set("/tow", Value::Object(tow)))
}

/// The node name for a new crew place: `Crew_<Kind>_<suffix>`. The suffix must be non-empty (`Count` takes the number).
pub fn crew_name(kind: CrewKind, suffix: &str) -> Result<String, String> {
    let suffix = suffix.trim();
    if suffix.is_empty() {
        return Err("give the crew place a name suffix (for example P_0)".into());
    }
    Ok(format!("Crew_{}_{suffix}", kind.word()))
}

/// The edits that add a point of `kind` at `pos` (game axes) as the entry `label` (for a crew place the whole node name, see
/// [`crew_name`]; ignored for tow ends and homing).
pub fn add_point(ship: &Value, facts: Option<&Facts>, kind: Kind, label: &str, pos: [f64; 3]) -> Result<Vec<Edit>, String> {
    let label = label.trim();
    let need_label = || if label.is_empty() { Err("give the new point a name".to_string()) } else { Ok(()) };
    match kind {
        Kind::Gun => {
            need_label()?;
            if ship.get("guns").and_then(Value::as_object).is_some_and(|g| g.keys().any(|k| k.eq_ignore_ascii_case(label))) {
                return Err(format!("there is already a gun mount named {label}"));
            }
            // PROVISIONAL: no `gun` and no `arc_deg` guesses; the form fills them and the checker flags them until then.
            Ok(vec![Edit::set(format!("/guns/{}", escape(label)), json!({"pos": pos_value(pos), "rot": pos_value([0.0; 3])}))])
        }
        Kind::Engine | Kind::Wake | Kind::Flag => {
            need_label()?;
            let (key, model_kind) = match kind {
                Kind::Engine => ("engines", "EnginePortPlacement"),
                Kind::Wake => ("wakes", "WakePlacement"),
                _ => ("flags", "FlagAttachmentPoint"),
            };
            let entry = json!({"label": label, "pos": pos_value(pos)});
            let has_array = ship.get(key).is_some_and(Value::is_array);
            let model_nodes = facts.is_some_and(|f| f.points.iter().any(|p| p.kind == model_kind));
            match (has_array, model_nodes, kind) {
                (true, _, _) => Ok(vec![Edit::set(format!("/{key}/-"), entry)]),
                (false, false, _) => Ok(vec![Edit::set(format!("/{key}"), Value::Array(vec![entry]))]),
                (false, true, Kind::Flag) => {
                    let mut list = flags_from_model(facts);
                    list.push(entry);
                    Ok(vec![Edit::set("/flags", Value::Array(list))])
                }
                // The engine fills an entry's pos, rot and fx from the model node of the same label, so the nodes can be listed by label.
                (false, true, Kind::Engine) => {
                    let mut list = engines_from_model(facts);
                    list.push(entry);
                    Ok(vec![Edit::set("/engines", Value::Array(list))])
                }
                // A wake entry needs pos and fx and has no node lookup (ships.rs WakeDoc); the fx comes from the hull size, unknown here.
                (false, true, _) => Err(format!(
                    "the model has {key} nodes and ship.json's {key} array replaces all of them; each entry needs an fx the engine derives from the hull size, which the editor does not know (PROVISIONAL, engine requirement: wake entries that fill pos and fx from the node, as engines do)"
                )),
            }
        }
        Kind::Crew(k) => {
            need_label()?;
            if CrewKind::of_name(label) != Some(k) {
                return Err(format!("{label} is not a Crew_{}_... node name", k.word()));
            }
            if hardpoints(ship, facts).iter().any(|h| matches!(h.kind, Kind::Crew(_)) && h.label == label) {
                return Err(format!("there is already a crew place named {label}"));
            }
            let entry = json!({"name": label, "pos": pos_value(pos)});
            Ok(vec![if ship.get("crew_nodes").is_some_and(Value::is_array) { Edit::set("/crew_nodes/-", entry) } else { Edit::set("/crew_nodes", Value::Array(vec![entry])) }])
        }
        Kind::TowTower | Kind::TowTowee => Ok(vec![tow(ship, facts, kind, pos)?]),
        Kind::Homing => Ok(vec![Edit::set("/homing", pos_value(pos))]),
        Kind::Boarding => {
            Err("boarding points come only from the model; ship.json has no key for them (PROVISIONAL, engine requirement: a ship.json key for boarding points)".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::points::fixtures::*;
    use super::*;
    use crate::edit::apply;

    fn run(ship: &Value, edits: &[Edit]) -> Value {
        let mut s = ship.clone();
        for e in edits {
            apply(&mut s, e).unwrap();
        }
        s
    }

    fn pick(ship: &Value, f: Option<&Facts>, kind: Kind, label: &str) -> Hardpoint {
        hardpoints(ship, f).into_iter().find(|h| h.kind == kind && h.label == label).unwrap_or_else(|| panic!("no {label}"))
    }

    /// Moves `hp` and checks that the result has the point at `to` and every other point where it was.
    fn moved_only(ship: &Value, f: Option<&Facts>, kind: Kind, label: &str, to: [f64; 3]) -> Value {
        let hp = pick(ship, f, kind, label);
        let before = hardpoints(ship, f);
        let edits = move_point(ship, f, &hp, to).unwrap();
        let after_ship = run(ship, &edits);
        let after = hardpoints(&after_ship, f);
        assert_eq!(before.len(), after.len(), "{before:?} -> {after:?}");
        for b in &before {
            // JSON entries are listed before model nodes, so a point may change place in the list: compare by kind and label.
            let a = after.iter().find(|a| a.kind == b.kind && a.label == b.label).unwrap_or_else(|| panic!("{} vanished", b.label));
            let tow_pair = matches!((b.kind, kind), (Kind::TowTower, Kind::TowTowee) | (Kind::TowTowee, Kind::TowTower));
            if b.kind == kind && b.label == label {
                assert_eq!(a.pos, to);
            } else {
                assert_eq!(a.pos, b.pos, "{label} moved {} {}{}", b.kind.title(), b.label, if tow_pair { " (tow pair)" } else { "" });
            }
        }
        after_ship
    }

    #[test]
    fn a_json_point_moves_its_pos_with_short_numbers() {
        let ship = json!({"guns": {"A/B": {"gun": "G", "arc_deg": 9, "pos": [0, 0, 0], "rot": [0, 0, 0]}},
            "engines": [{"label": "E", "pos": [1, 1, 1], "fx": "X"}], "wakes": [{"pos": [0, 0, 0], "fx": "W"}], "flags": [{"label": "Flag_Top", "pos": [0, 0, 1]}],
            "tow": {"tower": [0, -1, 0], "towee": [0, 1, 0]}, "homing": [0, 0, 0], "crew_nodes": [{"name": "Crew_Walk_P_0", "pos": [0, 0, 0]}]});
        let to = [1.5, -369.23077392578125, 2.0];
        let edits = move_point(&ship, None, &pick(&ship, None, Kind::Gun, "A/B"), to).unwrap();
        assert_eq!(edits, vec![Edit::set("/guns/A~1B/pos", json!([1.5, -369.23077, 2.0]))]);
        for (kind, label) in [
            (Kind::Engine, "E"),
            (Kind::Wake, "wake 0"),
            (Kind::Flag, "Flag_Top"),
            (Kind::TowTower, "tower"),
            (Kind::TowTowee, "towee"),
            (Kind::Homing, "homing"),
            (Kind::Crew(CrewKind::Walk), "Crew_Walk_P_0"),
        ] {
            let out = moved_only(&ship, None, kind, label, [3.0, 4.0, 5.0]);
            assert_ne!(out, ship, "{label}");
        }
        let e = move_point(&ship, None, &pick(&ship, None, Kind::Homing, "homing"), [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(e, vec![Edit::set("/homing", json!([1.0, 2.0, 3.0]))]);
        let e = move_point(&ship, None, &pick(&ship, None, Kind::TowTowee, "towee"), [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(e, vec![Edit::set("/tow/towee", json!([1.0, 2.0, 3.0]))]);
    }

    #[test]
    fn json_moves_change_only_that_value() {
        let ship = json!({"guns": {"A": {"gun": "G", "arc_deg": 9, "pos": [0, 0, 0], "rot": [0, 0, 0]}, "B": {"pos": [1, 1, 1]}}});
        let out = moved_only(&ship, None, Kind::Gun, "A", [9.0, 8.0, 7.0]);
        assert_eq!(out["guns"]["A"], json!({"gun": "G", "arc_deg": 9, "pos": [9.0, 8.0, 7.0], "rot": [0, 0, 0]}));
        assert_eq!(out["guns"]["B"], ship["guns"]["B"]);
    }

    #[test]
    fn a_model_gun_gets_a_pos_and_a_rot_and_keeps_its_facing() {
        let f = facts();
        let ship = json!({"guns": {"Gun_Mount_A": {"gun": "G", "arc_deg": 90}}});
        let out = moved_only(&ship, Some(&f), Kind::Gun, "Gun_Mount_A", [2.0, 2.0, 2.0]);
        assert_eq!(out["guns"]["Gun_Mount_A"], json!({"gun": "G", "arc_deg": 90, "pos": [2.0, 2.0, 2.0], "rot": [0.5, 0.0, 0.0]}));
        // a node no entry names gets a fresh entry; an existing rot is kept
        let out = moved_only(&json!({}), Some(&f), Kind::Gun, "Gun_Mount_B", [0.0, 1.0, 1.0]);
        assert_eq!(out["guns"]["Gun_Mount_B"], json!({"pos": [0.0, 1.0, 1.0], "rot": [0.0, 0.0, 0.0]}));
        let kept = json!({"guns": {"Gun_Mount_A": {"gun": "G", "arc_deg": 1, "rot": [0.75, 0, 0]}}});
        let edits = move_point(&kept, Some(&f), &pick(&kept, Some(&f), Kind::Gun, "Gun_Mount_A"), [0.0; 3]).unwrap();
        assert_eq!(edits.len(), 1);
    }

    #[test]
    fn model_homing_and_flags_become_json_and_the_others_stay_put() {
        let f = facts();
        let out = moved_only(&json!({}), Some(&f), Kind::Homing, "homing", [1.0, 1.0, 1.0]);
        assert_eq!(out["homing"], json!([1.0, 1.0, 1.0]));
        let out = moved_only(&json!({}), Some(&f), Kind::Flag, "Flag_Bottom", [0.0, 0.0, 6.0]);
        assert_eq!(out["flags"], json!([{"label": "Flag_Top", "pos": [0.0, 0.0, 9.0]}, {"label": "Flag_Bottom", "pos": [0.0, 0.0, 6.0]}]));
    }

    #[test]
    fn a_model_tow_end_writes_both_ends_and_refuses_when_the_other_is_unknown() {
        let f = facts();
        let out = moved_only(&json!({}), Some(&f), Kind::TowTower, "tower", [0.0, -8.0, 1.0]);
        assert_eq!(out["tow"], json!({"tower": [0.0, -8.0, 1.0], "towee": [0.0, 6.0, 1.0]}));
        let only_one = Facts { points: vec![super::super::points::fixtures::point("TowerPoint", "Tow_Back", [0.0, -6.0, 1.0], 0.0)], layers: vec![] };
        let hp = pick(&json!({}), Some(&only_one), Kind::TowTower, "tower");
        let err = move_point(&json!({}), Some(&only_one), &hp, [0.0; 3]).unwrap_err();
        assert!(err.contains("towee"), "{err}");
    }

    #[test]
    fn model_only_wakes_and_boarding_are_refused_with_a_reason() {
        let f = facts();
        for (kind, label) in [(Kind::Wake, "Wake_1"), (Kind::Boarding, "Boarding_1")] {
            let hp = pick(&json!({}), Some(&f), kind, label);
            let err = move_point(&json!({}), Some(&f), &hp, [1.0, 1.0, 1.0]).unwrap_err();
            assert!(err.contains("wake") || err.contains("boarding"), "{err}");
        }
        let hp = pick(&json!({}), Some(&f), Kind::Wake, "Wake_1");
        assert!(move_point(&json!({}), Some(&f), &hp, [f64::NAN, 0.0, 0.0]).is_err());
    }

    #[test]
    fn a_model_engine_moves_as_a_label_list_the_engine_fills_from_the_nodes() {
        let f = facts();
        let out = moved_only(&json!({}), Some(&f), Kind::Engine, "Engine_1", [2.0, 2.0, 2.0]);
        assert_eq!(out["engines"], json!([{"label": "Engine_1", "pos": [2.0, 2.0, 2.0]}]));
    }

    #[test]
    fn adding_a_gun_has_pos_and_rot_only_and_refuses_duplicates() {
        let ship = json!({"guns": {"Gun_A": {"gun": "G", "arc_deg": 1, "pos": [0, 0, 0], "rot": [0, 0, 0]}}});
        let edits = add_point(&ship, None, Kind::Gun, "Gun/B", [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(edits, vec![Edit::set("/guns/Gun~1B", json!({"pos": [1.0, 2.0, 3.0], "rot": [0.0, 0.0, 0.0]}))]);
        let out = run(&ship, &edits);
        assert_eq!(pick(&out, None, Kind::Gun, "Gun/B").pos, [1.0, 2.0, 3.0]);
        assert!(add_point(&ship, None, Kind::Gun, "gun_a", [0.0; 3]).is_err());
        assert!(add_point(&ship, None, Kind::Gun, "  ", [0.0; 3]).is_err());
    }

    #[test]
    fn adding_to_arrays_follows_the_replace_rule() {
        let f = facts();
        // array present: append
        let ship = json!({"engines": [{"label": "E", "pos": [0, 0, 0], "fx": "X"}]});
        let out = run(&ship, &add_point(&ship, Some(&f), Kind::Engine, "E2", [1.0, 1.0, 1.0]).unwrap());
        assert_eq!(out["engines"][1], json!({"label": "E2", "pos": [1.0, 1.0, 1.0]}));
        assert_eq!(out["engines"][0], ship["engines"][0]);
        // no array and model nodes: engines list the nodes by label (the engine fills the rest from the node of that name), wakes
        // refuse, flags restate the nodes
        let out = run(&json!({}), &add_point(&json!({}), Some(&f), Kind::Engine, "E2", [1.0, 1.0, 1.0]).unwrap());
        assert_eq!(out["engines"], json!([{"label": "Engine_1"}, {"label": "E2", "pos": [1.0, 1.0, 1.0]}]));
        assert!(add_point(&json!({}), Some(&f), Kind::Wake, "W2", [0.0; 3]).is_err());
        let out = run(&json!({}), &add_point(&json!({}), Some(&f), Kind::Flag, "Flag_Mid", [0.0, 0.0, 8.0]).unwrap());
        assert_eq!(out["flags"].as_array().unwrap().len(), 3);
        // no array and no model nodes: a new array of one
        let out = run(&json!({}), &add_point(&json!({}), None, Kind::Wake, "W", [0.0; 3]).unwrap());
        assert_eq!(out["wakes"], json!([{"label": "W", "pos": [0.0, 0.0, 0.0]}]));
    }

    #[test]
    fn a_refusal_is_known_before_the_click() {
        let f = facts();
        let ship = json!({});
        assert!(add_refusal(&ship, Some(&f), Kind::Wake).is_some_and(|m| m.contains("wakes")));
        assert_eq!(add_refusal(&ship, Some(&f), Kind::Engine), None);
        assert_eq!(add_refusal(&ship, Some(&f), Kind::Gun), None);
        assert_eq!(add_refusal(&ship, None, Kind::Wake), None, "no model nodes: a new array");
        assert_eq!(add_refusal(&json!({"wakes": []}), Some(&f), Kind::Wake), None, "an array in ship.json: append");
        assert!(add_refusal(&ship, None, Kind::TowTower).is_some(), "a tow end needs the other end");
    }

    #[test]
    fn adding_crew_nodes_checks_the_name_grammar() {
        assert_eq!(crew_name(CrewKind::Walk, " P_0 ").unwrap(), "Crew_Walk_P_0");
        assert!(crew_name(CrewKind::Walk, "").is_err());
        let ship = json!({});
        let edits = add_point(&ship, None, Kind::Crew(CrewKind::Gun), "Crew_Gun_A", [1.0, 0.0, 2.0]).unwrap();
        let out = run(&ship, &edits);
        assert_eq!(out["crew_nodes"], json!([{"name": "Crew_Gun_A", "pos": [1.0, 0.0, 2.0]}]));
        let edits = add_point(&out, None, Kind::Crew(CrewKind::Gun), "Crew_Gun_B", [1.0, 0.0, 2.0]).unwrap();
        assert_eq!(run(&out, &edits)["crew_nodes"].as_array().unwrap().len(), 2);
        assert!(add_point(&out, None, Kind::Crew(CrewKind::Gun), "Crew_Gun_A", [0.0; 3]).is_err(), "duplicate");
        assert!(add_point(&out, None, Kind::Crew(CrewKind::Walk), "Crew_Gun_Z", [0.0; 3]).is_err(), "kind mismatch");
        assert!(add_point(&out, None, Kind::Crew(CrewKind::Walk), "Walk_A", [0.0; 3]).is_err());
    }

    #[test]
    fn adding_tow_ends_and_homing_and_boarding() {
        let f = facts();
        let out = run(&json!({}), &add_point(&json!({}), Some(&f), Kind::TowTowee, "", [0.0, 7.0, 1.0]).unwrap());
        assert_eq!(out["tow"], json!({"tower": [0.0, -6.0, 1.0], "towee": [0.0, 7.0, 1.0]}));
        assert!(add_point(&json!({}), None, Kind::TowTower, "", [0.0; 3]).is_err());
        let out = run(&json!({}), &add_point(&json!({}), None, Kind::Homing, "", [1.0, 2.0, 3.0]).unwrap());
        assert_eq!(out["homing"], json!([1.0, 2.0, 3.0]));
        assert!(add_point(&json!({}), None, Kind::Boarding, "b", [0.0; 3]).is_err());
    }
}
