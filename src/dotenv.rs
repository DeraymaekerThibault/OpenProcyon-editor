//! The untracked `.env` file (AGENTS.md: local paths go there). Read once at startup: plain `KEY=VALUE` lines, `#` comments and blank
//! lines ignored, one pair of surrounding quotes stripped, no variable expansion. A variable the real environment already has wins.

use std::path::{Path, PathBuf};

/// The `(key, value)` pairs of `.env` text, in order. A line without `=`, or with an empty or non-identifier key, is skipped; an
/// optional `export ` prefix is accepted.
pub fn parse(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.strip_prefix("export ").unwrap_or(line).split_once('=')?;
            let key = key.trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return None;
            }
            let value = value.trim();
            let unquoted = ['"', '\''].iter().find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)).filter(|_| value.len() >= 2));
            Some((key.to_string(), unquoted.unwrap_or(value).to_string()))
        })
        .collect()
}

/// The `.env` to read: the working folder's, else (a `cargo run` of a development build) the one next to this crate's `Cargo.toml`.
pub fn find(cwd: Option<&Path>) -> Option<PathBuf> {
    let here = cwd.map(|d| d.join(".env")).filter(|p| p.is_file());
    here.or_else(|| Some(Path::new(env!("CARGO_MANIFEST_DIR")).join(".env")).filter(|p| p.is_file()))
}

/// The pairs of `pairs` the environment does not have yet (`has` says whether a name is set): the real environment wins, and a
/// name repeated in the file keeps its first value.
pub fn unset_ones(pairs: Vec<(String, String)>, has: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for (k, v) in pairs {
        if !has(&k) && !out.iter().any(|(o, _)| *o == k) {
            out.push((k, v));
        }
    }
    out
}

/// Reads `.env` and sets what the environment lacks. Call once at the top of `main`, before any other thread exists, so that the
/// game launched from the editor (which inherits the environment) and the engine's lookups (`PROCYON_ASSETS`) see it.
pub fn load() {
    let Some(path) = find(std::env::current_dir().ok().as_deref()) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    for (k, v) in unset_ones(parse(&text), |k| std::env::var_os(k).is_some()) {
        // SAFETY: called first thing in `main`, when the process is still single-threaded, so no other thread reads or writes the
        // environment concurrently.
        unsafe { std::env::set_var(k, v) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(a: &[(&str, &str)]) -> Vec<(String, String)> {
        a.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn plain_pairs_comments_quotes_and_junk() {
        let t = "# a comment\n\nPROCYON_ASSETS=/x/assets\n  A = b c  \nQ=\"quoted value\"\nS='single'\nexport E=1\nEMPTY=\nNOEQUALS\n=nokey\nBAD KEY=1\nEQ=a=b\nHALF=\"open\nLONE=\"\n$X=$HOME\nV=$HOME/x\n";
        assert_eq!(
            parse(t),
            kv(&[
                ("PROCYON_ASSETS", "/x/assets"),
                ("A", "b c"),
                ("Q", "quoted value"),
                ("S", "single"),
                ("E", "1"),
                ("EMPTY", ""),
                ("EQ", "a=b"),
                ("HALF", "\"open"),
                ("LONE", "\""),
                ("V", "$HOME/x"),
            ])
        );
    }

    #[test]
    fn the_real_environment_wins_and_the_first_repeat_stays() {
        let pairs = kv(&[("A", "1"), ("B", "2"), ("A", "3")]);
        assert_eq!(unset_ones(pairs.clone(), |k| k == "B"), kv(&[("A", "1")]));
        assert_eq!(unset_ones(pairs, |_| false), kv(&[("A", "1"), ("B", "2")]));
    }

    #[test]
    fn the_working_folders_file_is_found_first() {
        let d = std::env::temp_dir().join(format!("ope-dotenv-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(find(Some(&d)).is_none_or(|p| p != d.join(".env")));
        std::fs::write(d.join(".env"), "A=1").unwrap();
        assert_eq!(find(Some(&d)), Some(d.join(".env")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
