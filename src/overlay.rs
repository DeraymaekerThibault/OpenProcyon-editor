//! The editor's own merged view of the player's import plus mods: `authoring::overlay_into` into a scratch folder that belongs to
//! this editor process, never the game's shared `local/overlay` (#136), so the editor and a running game do not clobber each other.

use openprocyon::authoring::{ModReport, overlay_into};
use std::path::{Path, PathBuf};

/// `<system temp>/openprocyon-editor-<pid>/<name>`: one scratch folder per editor process and purpose.
pub fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("openprocyon-editor-{}", std::process::id())).join(name)
}

const PREFIX: &str = "openprocyon-editor-";

/// Removes this process's scratch folder (the editor is closing). Goes through the write guard.
pub fn remove_own() {
    let _ = crate::project::guard::remove_dir_all(&std::env::temp_dir().join(format!("{PREFIX}{}", std::process::id())), None);
}

/// Whether the scratch folder of a dead or unknown process is to be removed: `alive` is whether its process runs (`None`: cannot be
/// told on this system), `age` the time since the folder last changed. A folder of a process that does not run is stale; where the
/// process cannot be looked up (PROVISIONAL, only Linux's `/proc` is used), only one untouched for a day.
fn is_stale(alive: Option<bool>, age: std::time::Duration) -> bool {
    alive.map_or(age > std::time::Duration::from_secs(24 * 3600), |a| !a)
}

fn process_alive(pid: u32) -> Option<bool> {
    cfg!(target_os = "linux").then(|| Path::new("/proc").join(pid.to_string()).exists())
}

/// Removes the `openprocyon-editor-<pid>` folders under `temp` that an editor killed without closing left behind (they hold symlinks
/// into the import, merged retail files and preview pictures): every such folder that is a real folder (a symlink is left alone) and
/// stale per [`is_stale`], never this process's own. The removal goes through the write guard and never follows a symlink out of the
/// folder (`remove_dir_all` removes links, not what they point to). Returns what was removed.
pub fn remove_stale_in(temp: &Path, own_pid: u32, alive: impl Fn(u32) -> Option<bool>) -> Vec<PathBuf> {
    let mut removed = vec![];
    for e in std::fs::read_dir(temp).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(pid) = name.strip_prefix(PREFIX).and_then(|p| p.parse::<u32>().ok()).filter(|p| *p != own_pid) else { continue };
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else { continue };
        if !meta.is_dir() {
            continue;
        }
        let age = meta.modified().ok().and_then(|m| m.elapsed().ok()).unwrap_or_default();
        if is_stale(alive(pid), age) && crate::project::guard::remove_dir_all(&e.path(), None).is_ok() {
            removed.push(e.path());
        }
    }
    removed
}

/// [`remove_stale_in`] for the system's temp folder and this process (call once at startup).
pub fn remove_stale() -> Vec<PathBuf> {
    remove_stale_in(&std::env::temp_dir(), std::process::id(), process_alive)
}

/// The root to read merged files from: `base` itself when there are no mods (the engine builds nothing then), else the overlay of
/// `mods` over `base` built into `out`, with the engine's report of what each mod did.
pub fn build(base: &Path, mods: &[PathBuf], out: &Path) -> Result<(PathBuf, ModReport), String> {
    if mods.is_empty() {
        return Ok((base.to_path_buf(), ModReport::default()));
    }
    if let Some(parent) = out.parent() {
        crate::project::guard::create_dir_all(parent, Some(base))?;
    }
    let report = overlay_into(base, mods, out)?;
    Ok((out.to_path_buf(), report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_scratch_folders_go_and_what_their_symlinks_point_to_stays() {
        let dir = std::env::temp_dir().join(format!("procyon-editor-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (temp, import) = (dir.join("tmp"), dir.join("import"));
        std::fs::create_dir_all(import.join("ships")).unwrap();
        std::fs::write(import.join("ships/a.json"), "{}").unwrap();
        let mk = |pid: u32| {
            let d = temp.join(format!("{PREFIX}{pid}/project-0"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("merged.json"), "{}").unwrap();
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&import, d.join("link")).unwrap();
                std::os::unix::fs::symlink(import.join("ships/a.json"), d.join("file-link")).unwrap();
            }
            temp.join(format!("{PREFIX}{pid}"))
        };
        let (dead, alive, own) = (mk(111), mk(222), mk(333));
        std::fs::create_dir_all(temp.join("other-folder")).unwrap();
        std::fs::create_dir_all(temp.join(format!("{PREFIX}notapid"))).unwrap();
        let removed = remove_stale_in(&temp, 333, |p| Some(p == 222 || p == 333));
        assert_eq!(removed, std::slice::from_ref(&dead));
        assert!(!dead.exists() && alive.exists() && own.exists(), "only the dead process's folder goes");
        assert!(temp.join("other-folder").exists() && temp.join(format!("{PREFIX}notapid")).exists());
        assert!(import.join("ships/a.json").is_file(), "what the symlinks point to is untouched");
        // cannot tell if it runs: only a folder a day old goes
        assert!(remove_stale_in(&temp, 333, |_| None).is_empty());
        assert!(is_stale(None, std::time::Duration::from_secs(25 * 3600)) && !is_stale(None, std::time::Duration::from_secs(60)));
        assert!(is_stale(Some(false), std::time::Duration::ZERO) && !is_stale(Some(true), std::time::Duration::from_secs(99 * 3600)));
        let _ = std::fs::remove_dir_all(&dir);
    }

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
