//! The engine's `--check-mission`, in-process: `openprocyon::rules::check::check` on the edited mission, map and media.

use serde_json::Value;

pub use openprocyon::rules::check::{Level, Problem};

pub fn run(mission: &Value, map: &Value, media: &Value) -> Vec<Problem> {
    openprocyon::rules::check::check(mission, map, media)
}

/// The findings for one condition or action of a rule. A `Problem` names the rule and `at` ("action #5 'Start Timer'", 1-based
/// position in the list, then the Type) and the field; there is no structured location, so this matches on that text.
/// PROVISIONAL: depends on the wording of `at`; Phase 1 needs a structured location (rule index, list, index, field).
pub fn for_item<'a>(problems: &'a [Problem], rule: &str, is_condition: bool, index: usize) -> Vec<&'a Problem> {
    let prefix = format!("{} #{} ", if is_condition { "condition" } else { "action" }, index + 1);
    problems.iter().filter(|p| p.rule.trim() == rule.trim() && p.at.starts_with(&prefix)).collect()
}

/// The findings about the rule as a whole (not one of its conditions or actions).
pub fn for_rule<'a>(problems: &'a [Problem], rule: &str) -> Vec<&'a Problem> {
    problems.iter().filter(|p| p.rule.trim() == rule.trim() && !p.at.starts_with("condition #") && !p.at.starts_with("action #")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::{Mission, fixture_dir};

    #[test]
    fn the_worked_example_has_no_errors() {
        let m = Mission::load(&fixture_dir()).unwrap();
        let p = run(&m.mission, &m.map, &m.media);
        assert!(p.iter().all(|p| p.level != Level::Error), "{p:?}");
    }

    #[test]
    fn a_broken_volume_name_is_a_problem_on_that_field() {
        let mut m = Mission::load(&fixture_dir()).unwrap();
        m.mission["rules"][1]["conditions"][0]["Volume Name"] = "Gate Aera".into();
        let p = run(&m.mission, &m.map, &m.media);
        let hit = p.iter().find(|p| p.field == "Volume Name").unwrap_or_else(|| panic!("no problem: {p:?}"));
        assert_eq!(hit.level, Level::Error);
        assert!(hit.message.contains("Gate Area"), "suggests the near miss: {}", hit.message);
        assert_eq!(for_item(&p, "Arrive at the gate", true, 0).len(), 1);
        assert!(for_item(&p, "Arrive at the gate", true, 1).is_empty());
        assert!(for_item(&p, "Start", true, 0).is_empty());
    }
}
