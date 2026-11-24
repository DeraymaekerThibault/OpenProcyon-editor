//! Repository rules that keep the editor shareable (AGENTS.md "Hard rules"), the editor's version of the engine's
//! tests/test_repo_hygiene.py: no original game files, and no media outside a folder of our own art.

use std::path::Path;
use std::process::Command;

/// Original-game formats (the engine's .gitignore and hygiene test).
const GAME: &[&str] = &["wot", "twt", "mdb", "msb", "eft", "hdt", "tst", "fft", "tga", "dds", "bik", "exe", "dll"];
/// Media that could be converted from the game; allowed only under OWN_ART.
const MEDIA: &[&str] = &["glb", "png", "ogg", "wav"];
/// Folders of our own art. Empty for now: add one when the editor ships icons or meshes of its own.
const OWN_ART: &[&str] = &[];

fn problem(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    if path.starts_with("assets/") || GAME.contains(&ext.as_str()) {
        Some("original game file")
    } else if MEDIA.contains(&ext.as_str()) && !OWN_ART.iter().any(|dir| path.starts_with(&format!("{dir}/"))) {
        Some("media outside an own-art folder")
    } else {
        None
    }
}

#[test]
fn no_game_files_are_tracked() {
    let out = Command::new("git").args(["ls-files", "-z"]).current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("git runs");
    assert!(out.status.success(), "git ls-files failed");
    let bad: Vec<String> = String::from_utf8_lossy(&out.stdout).split('\0').filter_map(|p| problem(p).map(|why| format!("{p}: {why}"))).collect();
    assert!(bad.is_empty(), "never commit game content (AGENTS.md):\n{}", bad.join("\n"));
}

#[test]
fn patterns() {
    for hit in ["maps/x.WOT", "a/b.Dds", "assets/ships/x.json", "ui/icon.png", "sea.ogg"] {
        assert!(problem(hit).is_some(), "{hit}");
    }
    for ok in ["src/main.rs", "docs/spike.md", "Cargo.toml", "LICENSE"] {
        assert!(problem(ok).is_none(), "{ok}");
    }
}
