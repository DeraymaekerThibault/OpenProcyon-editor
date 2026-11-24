//! The editor's own merged view of the player's import plus mods: `authoring::overlay_into` into a scratch folder that belongs to
//! this editor process, never the game's shared `local/overlay` (#136), so the editor and a running game do not clobber each other.

use openprocyon::authoring::{ModReport, overlay_into};
use std::path::{Path, PathBuf};

/// `<system temp>/openprocyon-editor-<pid>/<name>`: one scratch folder per editor process and purpose.
pub fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("openprocyon-editor-{}", std::process::id())).join(name)
}

/// The root to read merged files from: `base` itself when there are no mods (the engine builds nothing then), else the overlay of
/// `mods` over `base` built into `out`, with the engine's report of what each mod did.
pub fn build(base: &Path, mods: &[PathBuf], out: &Path) -> Result<(PathBuf, ModReport), String> {
    if mods.is_empty() {
        return Ok((base.to_path_buf(), ModReport::default()));
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let report = overlay_into(base, mods, out)?;
    Ok((out.to_path_buf(), report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_mods_reads_the_base_and_a_mod_is_merged_into_the_scratch_folder() {
        let dir = std::env::temp_dir().join(format!("procyon-editor-overlay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (base, m) = (dir.join("assets"), dir.join("mod"));
        std::fs::create_dir_all(base.join("maps/red")).unwrap();
        std::fs::create_dir_all(m.join("maps/red")).unwrap();
        std::fs::write(base.join("maps/red/map.json"), r#"{"name": "Red", "world_half": 10}"#).unwrap();
        std::fs::write(m.join("maps/red/map.json"), r#"{"extends": "red", "world_half": 20}"#).unwrap();
        let (root, report) = build(&base, &[], &dir.join("out")).unwrap();
        assert_eq!(root, base);
        assert!(report.mods.is_empty() && !dir.join("out").exists());
        let (root, report) = build(&base, std::slice::from_ref(&m), &dir.join("out")).unwrap();
        assert_eq!(report.mods.len(), 1);
        let map = openprocyon::authoring::read_map(&root, "red").unwrap();
        assert_eq!((map["name"].as_str(), map["world_half"].as_i64()), (Some("Red"), Some(20)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
