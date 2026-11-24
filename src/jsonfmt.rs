//! The engine's one JSON layout (`tools/jsonio.py` `dump` in the engine repository): indent 2, a list of numbers on one line
//! (`[1.0, 2.0]`), non-ASCII escaped as `\uXXXX` (Python's `ensure_ascii`), key order kept, a final newline.
//! A port of that behaviour, so a saved file differs from the importer's output only where the data differs.
//! PROVISIONAL: number text is serde_json's, which matches Python's `repr` for ordinary values; exponent forms differ
//! (`1e-5` here, `1e-05` there). The engine's files hold no such numbers; Phase 1's `authoring` writer should own this.

use serde_json::Value;
use std::fmt::Write;

/// `v` in the engine's layout, ending with a newline.
pub fn dump(v: &Value) -> String {
    let mut out = String::new();
    value(v, 0, &mut out);
    out.push('\n');
    out
}

fn value(v: &Value, depth: usize, out: &mut String) {
    match v {
        Value::Array(a) if a.is_empty() => out.push_str("[]"),
        Value::Object(m) if m.is_empty() => out.push_str("{}"),
        Value::Array(a) if a.iter().all(Value::is_number) => {
            out.push('[');
            for (i, n) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{n}");
            }
            out.push(']');
        }
        Value::Array(a) => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                indent(depth + 1, out);
                value(x, depth + 1, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            indent(depth, out);
            out.push(']');
        }
        Value::Object(m) => {
            out.push_str("{\n");
            for (i, (k, x)) in m.iter().enumerate() {
                indent(depth + 1, out);
                string(k, out);
                out.push_str(": ");
                value(x, depth + 1, out);
                out.push_str(if i + 1 < m.len() { ",\n" } else { "\n" });
            }
            indent(depth, out);
            out.push('}');
        }
        Value::String(s) => string(s, out),
        other => {
            let _ = write!(out, "{other}");
        }
    }
}

fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// A JSON string the way Python's `json.dumps` writes it (`ensure_ascii=True`).
fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (' '..='\u{7e}').contains(&c) || c == '\u{7f}' => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixtures() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
    }

    fn read(rel: &str) -> String {
        std::fs::read_to_string(fixtures().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
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
            assert_eq!(dump(&v), read(expected), "{src}");
        }
    }

    #[test]
    fn the_layout_is_stable_when_read_and_written_again() {
        for (_, expected) in PAIRS {
            let text = read(expected);
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(dump(&v), text, "{expected}");
        }
    }

    #[test]
    fn small_cases() {
        let v: Value = serde_json::from_str(r#"{"k": [1.5, 2], "a": [], "s": "é"}"#).unwrap();
        assert_eq!(dump(&v), "{\n  \"k\": [1.5, 2],\n  \"a\": [],\n  \"s\": \"\\u00e9\"\n}\n");
    }
}
