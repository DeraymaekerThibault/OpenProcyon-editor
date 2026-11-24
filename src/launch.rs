//! Play-testing runs the real game: `openprocyon --mod <folder> --map <id>` as a child process, never blocking the UI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// The game binary: `PROCYON_GAME_BIN` if set, else `openprocyon` (found on PATH when spawned).
pub fn game_bin() -> PathBuf {
    bin_from(std::env::var_os("PROCYON_GAME_BIN"))
}

fn bin_from(env: Option<OsString>) -> PathBuf {
    env.filter(|v| !v.is_empty()).map_or_else(|| PathBuf::from("openprocyon"), PathBuf::from)
}

/// The arguments that start map `map_id` of the mod in `mod_dir`.
pub fn args(mod_dir: &Path, map_id: &str) -> Vec<OsString> {
    vec!["--mod".into(), mod_dir.into(), "--map".into(), map_id.into()]
}

/// Starts the game and returns at once; poll the child with `try_wait`.
pub fn spawn(mod_dir: &Path, map_id: &str) -> std::io::Result<Child> {
    Command::new(game_bin()).args(args(mod_dir, map_id)).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_mod_then_map() {
        let a = args(Path::new("/m/gates"), "gate_ambush");
        assert_eq!(a, ["--mod", "/m/gates", "--map", "gate_ambush"].map(OsString::from));
    }

    #[test]
    fn binary_from_the_environment_else_path() {
        assert_eq!(bin_from(Some("/opt/g/openprocyon".into())), PathBuf::from("/opt/g/openprocyon"));
        assert_eq!(bin_from(Some("".into())), PathBuf::from("openprocyon"));
        assert_eq!(bin_from(None), PathBuf::from("openprocyon"));
    }

    #[test]
    fn a_missing_binary_is_an_error_not_a_panic() {
        let r = Command::new("/nonexistent/openprocyon").args(args(Path::new("."), "x")).spawn();
        assert!(r.is_err());
    }
}
