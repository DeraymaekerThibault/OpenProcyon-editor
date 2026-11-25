//! Reads the autoplay end line the game logs (`autoplay end outcome=.. time=.. never_fired=[..]`, engine `smoke/mod.rs:294`).
//! PROVISIONAL: this parses a log line, whose wording the engine may change (engine requirement 5 is a report file). It moves to
//! `src/mission/report.rs` once that folder is on the base.

/// What the `autoplay end` line says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoplayEnd {
    /// `won`, `lost`, `stuck in NIS`, `player stuck` or `time limit`.
    pub outcome: String,
    /// Game seconds at the end.
    pub time: Option<u32>,
    /// The rules that never fired, by name.
    pub never_fired: Vec<String>,
}

/// The last `autoplay end` line of `log` (the game's stderr), or `None` when there is none (the run did not reach its end).
pub fn parse_autoplay_end(log: &str) -> Option<AutoplayEnd> {
    let line = log.lines().rev().find(|l| l.contains("autoplay end "))?;
    let rest = &line[line.find("autoplay end ")? + "autoplay end ".len()..];
    let (outcome, after) = quoted(rest.strip_prefix("outcome=")?)?;
    let time = after.trim_start().strip_prefix("time=").and_then(|t| t.split_whitespace().next()).and_then(|t| t.parse().ok());
    let list = after.split_once("never_fired=[")?.1;
    let mut never_fired = Vec::new();
    let mut rest = list;
    loop {
        rest = rest.trim_start_matches([',', ' ']);
        if rest.starts_with(']') || rest.is_empty() {
            break;
        }
        let (name, r) = quoted(rest)?;
        never_fired.push(name);
        rest = r;
    }
    Some(AutoplayEnd { outcome, time, never_fired })
}

/// Reads one Rust `{:?}` string (`"..."` with `\"`, `\\`, `\n`, `\t`, `\'` escapes) from the start of `s`; returns it and the rest.
fn quoted(s: &str) -> Option<(String, &str)> {
    let mut chars = s.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &s[i + 2..])),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                e => out.push(e),
            },
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_end_line_is_read() {
        let log = "INFO t=1.0 something\n INFO openprocyon::smoke: autoplay state point=\"A\" tasks=\"x\"\n INFO openprocyon::smoke: autoplay end outcome=\"won\" time=412 never_fired=[\"Every frame\", \"Say \\\"hi\\\"\", \"Waits, a bit\"]\n";
        let e = parse_autoplay_end(log).unwrap();
        assert_eq!(e, AutoplayEnd { outcome: "won".into(), time: Some(412), never_fired: vec!["Every frame".into(), "Say \"hi\"".into(), "Waits, a bit".into()] });
    }

    #[test]
    fn an_empty_list_and_a_spaced_outcome() {
        let e = parse_autoplay_end("autoplay end outcome=\"stuck in NIS\" time=7 never_fired=[]").unwrap();
        assert_eq!((e.outcome.as_str(), e.time, e.never_fired.len()), ("stuck in NIS", Some(7), 0));
    }

    #[test]
    fn no_line_or_a_changed_line_is_none() {
        assert_eq!(parse_autoplay_end(""), None);
        assert_eq!(parse_autoplay_end("autoplay state point=\"x\""), None);
        assert_eq!(parse_autoplay_end("autoplay end result=won"), None);
        assert_eq!(parse_autoplay_end("autoplay end outcome=\"won\" time=1 never_fired=[\"open"), None);
    }

    #[test]
    fn the_last_end_line_wins() {
        let e = parse_autoplay_end("autoplay end outcome=\"lost\" time=1 never_fired=[\"A\"]\nautoplay end outcome=\"won\" time=2 never_fired=[]").unwrap();
        assert_eq!(e.outcome, "won");
    }
}
