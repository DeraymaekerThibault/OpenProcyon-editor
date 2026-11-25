//! The engine's `--check-mission`, in-process: `authoring::check_mission` on the edited mission, map and media. Findings carry their
//! location as data (`rule_index`, `list`, `item`, `field`), so nothing here reads the printed text.

use serde_json::Value;

pub use openprocyon::authoring::{Finding, Level};

pub fn run(mission: &Value, map: &Value, media: &Value) -> Vec<Finding> {
    openprocyon::authoring::check_mission(mission, map, media, false)
}

/// The findings for one condition or action (`list` "conditions" / "actions", `index` in it) of rule `rule` (its index in `rules`),
/// nested conditions included (they belong to the item that holds them).
pub fn for_item<'a>(problems: &'a [Finding], rule: usize, list: &str, index: usize) -> Vec<&'a Finding> {
    problems.iter().filter(|p| p.rule_index == Some(rule) && p.list.as_deref() == Some(list) && p.item == Some(index)).collect()
}

/// The findings about rule `rule` as a whole (not one of its conditions or actions).
pub fn for_rule(problems: &[Finding], rule: usize) -> Vec<&Finding> {
    problems.iter().filter(|p| p.rule_index == Some(rule) && p.item.is_none()).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The committed worked example (`tests/fixtures/gates`, the engine's `gate-ambush`): (mission, map, media). Only meaningful when
    /// running from a checkout.
    pub(crate) fn gates_fixture() -> (Value, Value, Value) {
        let at = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gates/maps/gate_ambush");
        let read = |f: &str| serde_json::from_slice::<Value>(&std::fs::read(at.join(f)).unwrap()).unwrap();
        (read("mission.json"), read("map.json"), read("media.json"))
    }

    #[test]
    fn the_worked_example_has_no_errors() {
        let (mission, map, media) = gates_fixture();
        let p = run(&mission, &map, &media);
        assert!(p.iter().all(|p| p.level != Level::Error), "{p:?}");
    }

    #[test]
    fn a_broken_volume_name_is_a_problem_on_that_field() {
        let (mut mission, map, media) = gates_fixture();
        mission["rules"][1]["conditions"][0]["Volume Name"] = "Gate Aera".into();
        let p = run(&mission, &map, &media);
        let hit = p.iter().find(|p| p.field == "Volume Name").unwrap_or_else(|| panic!("no problem: {p:?}"));
        assert_eq!(hit.level, Level::Error);
        assert_eq!(hit.suggestion.as_deref(), Some("Gate Area"), "the near miss as data");
        assert_eq!((hit.rule_index, hit.list.as_deref(), hit.item), (Some(1), Some("conditions"), Some(0)));
        assert_eq!(for_item(&p, 1, "conditions", 0).len(), 1);
        assert!(for_item(&p, 1, "conditions", 1).is_empty());
        assert!(for_item(&p, 0, "conditions", 0).is_empty());
        assert!(for_rule(&p, 1).is_empty());
    }
}
