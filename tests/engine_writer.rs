//! The spike's writer fixtures, now held against the engine's own writer (`authoring::to_json_string`): `tests/fixtures/gates` and
//! `edge/` were written once by the engine's `tools/jsonio.py`, so the bytes must match.

use openprocyon::authoring::to_json_string;
use serde_json::Value;
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Every pair: the source file, and what the engine's Python writer made of it (generated once, committed).
const PAIRS: &[(&str, &str)] = &[
    ("source/mission.json", "gates/maps/gate_ambush/mission.json"),
    ("source/map.json", "gates/maps/gate_ambush/map.json"),
    ("source/media.json", "gates/maps/gate_ambush/media.json"),
    ("source/mod.json", "gates/mod.json"),
    ("source/edge.json", "edge/edge.json"),
];

#[test]
fn byte_identical_to_the_python_writer() {
    for (src, expected) in PAIRS {
        let v: Value = serde_json::from_str(&read(src)).unwrap();
        assert_eq!(to_json_string(&v), read(expected), "{src}");
    }
}

#[test]
fn the_layout_is_stable_when_read_and_written_again() {
    for (_, expected) in PAIRS {
        let text = read(expected);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(to_json_string(&v), text, "{expected}");
    }
}

#[test]
fn small_cases_and_the_python_exponent_form() {
    let v: Value = serde_json::from_str(r#"{"k": [1.5, 2], "a": [], "s": "é", "tiny": 2e-6}"#).unwrap();
    assert_eq!(to_json_string(&v), "{\n  \"k\": [1.5, 2],\n  \"a\": [],\n  \"s\": \"\\u00e9\",\n  \"tiny\": 2e-06\n}\n");
}
