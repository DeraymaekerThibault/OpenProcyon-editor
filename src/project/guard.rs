//! The one write guard: every file the editor writes or deletes goes through this module, which refuses a path inside the player's
//! import (docs/roadmap.md B6 "Retail always stays intact"; AGENTS.md hard rule 2). Editing retail content always lands in a mod, as
//! a diff; the import is read only. A test (`tests/hygiene.rs`) keeps direct `std::fs` writes out of the rest of the code.
//!
//! The import is the project's import and also the engine's own `authoring::base_assets()` (the second is checked even when a
//! project has another import, so a mistyped path never writes into the folder the game reads).

use std::fs::File;
use std::path::{Component, Path, PathBuf};

/// `path` made absolute with every symlink on its way resolved, even when the end of it does not exist yet: the nearest existing
/// parent is canonicalized and the rest is appended; `..` is applied to what is resolved so far. A dangling symlink is an error
/// (what it would create cannot be told).
fn resolve(path: &Path) -> Result<PathBuf, String> {
    let abs = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Normal(n) => {
                out.push(n);
                if std::fs::symlink_metadata(&out).is_ok() {
                    out = out.canonicalize().map_err(|e| format!("{}: {e}", out.display()))?;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

/// The import folders to protect: `import` (when given and present) and the engine's `base_assets()`, each resolved.
fn protected(import: Option<&Path>) -> Vec<PathBuf> {
    let engine = openprocyon::authoring::base_assets();
    import.map(Path::to_path_buf).into_iter().chain(engine).filter_map(|d| resolve(&d).ok()).collect()
}

/// Whether `path` may be written or deleted by the editor: `Err` (a sentence for the user) when it is inside one of `imports`
/// (already resolved), also when it does not exist yet and its nearest existing parent is.
fn check_against(path: &Path, imports: &[PathBuf]) -> Result<(), String> {
    let resolved = resolve(path)?;
    match imports.iter().find(|i| resolved.starts_with(i)) {
        Some(i) => Err(format!("{}: refused, it is inside the game import {} (retail files are never changed; changes go into a mod)", path.display(), i.display())),
        None => Ok(()),
    }
}

/// Refuses a path inside the import (`import` is the project's, else the engine's `base_assets()`; both are checked when both exist).
pub fn check_writable(path: &Path, import: Option<&Path>) -> Result<(), String> {
    check_against(path, &protected(import))
}

/// Whether `dir` may be a mod folder: refused (with the guard's message) when it is inside the import, is the import, or contains
/// the import (a parent of it): such a folder would list retail files as the mod's own and a save could reach into the import.
pub fn check_project_dir(dir: &Path, import: Option<&Path>) -> Result<(), String> {
    let imports = protected(import);
    check_against(dir, &imports)?;
    let resolved = resolve(dir)?;
    match imports.iter().find(|i| i.starts_with(&resolved)) {
        Some(i) => Err(format!(
            "{}: refused, it contains the game import {} (a mod folder is a separate folder; retail files are never changed, changes go into a mod)",
            dir.display(),
            i.display()
        )),
        None => Ok(()),
    }
}

fn io(path: &Path, e: std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// `std::fs::write` through the guard.
pub fn write(path: &Path, contents: impl AsRef<[u8]>, import: Option<&Path>) -> Result<(), String> {
    check_writable(path, import)?;
    std::fs::write(path, contents).map_err(|e| io(path, e))
}

/// `std::fs::create_dir_all` through the guard.
pub fn create_dir_all(path: &Path, import: Option<&Path>) -> Result<(), String> {
    check_writable(path, import)?;
    std::fs::create_dir_all(path).map_err(|e| io(path, e))
}

/// `std::fs::remove_file` through the guard.
pub fn remove_file(path: &Path, import: Option<&Path>) -> Result<(), String> {
    check_writable(path, import)?;
    std::fs::remove_file(path).map_err(|e| io(path, e))
}

/// `std::fs::remove_dir_all` through the guard.
pub fn remove_dir_all(path: &Path, import: Option<&Path>) -> Result<(), String> {
    check_writable(path, import)?;
    std::fs::remove_dir_all(path).map_err(|e| io(path, e))
}

/// `std::fs::remove_dir` (an empty folder only) through the guard.
pub fn remove_empty_dir(path: &Path, import: Option<&Path>) -> Result<(), String> {
    check_writable(path, import)?;
    std::fs::remove_dir(path).map_err(|e| io(path, e))
}

/// `std::fs::copy` through the guard (checks the destination only; the source is read).
pub fn copy(from: &Path, to: &Path, import: Option<&Path>) -> Result<u64, String> {
    check_writable(to, import)?;
    std::fs::copy(from, to).map_err(|e| io(to, e))
}

/// `std::fs::File::create` through the guard.
pub fn create_file(path: &Path, import: Option<&Path>) -> Result<File, String> {
    check_writable(path, import)?;
    File::create(path).map_err(|e| io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-guard-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn imports(dir: &Path) -> Vec<PathBuf> {
        vec![resolve(dir).unwrap()]
    }

    #[test]
    fn a_file_inside_the_import_is_refused() {
        let root = tmp("inside");
        let imp = root.join("assets");
        std::fs::create_dir_all(imp.join("ships/A")).unwrap();
        std::fs::write(imp.join("ships/A/ship.json"), "{}").unwrap();
        let err = check_against(&imp.join("ships/A/ship.json"), &imports(&imp)).unwrap_err();
        assert!(err.contains("inside the game import"), "{err}");
        assert!(check_against(&imp, &imports(&imp)).is_err(), "the import folder itself");
        assert!(write(&imp.join("ships/A/ship.json"), "x", Some(&imp)).is_err());
        assert!(remove_file(&imp.join("ships/A/ship.json"), Some(&imp)).is_err());
        assert_eq!(std::fs::read_to_string(imp.join("ships/A/ship.json")).unwrap(), "{}", "nothing was changed");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_nested_file_is_refused_by_its_nearest_existing_parent() {
        let root = tmp("nested");
        let imp = root.join("assets");
        std::fs::create_dir_all(&imp).unwrap();
        for p in [imp.join("new/deep/er/file.json"), imp.join("a/../b/x.json"), root.join("assets/./c.json")] {
            assert!(check_against(&p, &imports(&imp)).is_err(), "{}", p.display());
        }
        assert!(create_dir_all(&imp.join("new/deep"), Some(&imp)).is_err());
        assert!(!imp.join("new").exists(), "the folder was not made");
        // a `..` that climbs out of the import is not inside it
        assert!(check_against(&imp.join("../outside/x.json"), &imports(&imp)).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_into_the_import_is_refused() {
        let root = tmp("link");
        let imp = root.join("assets");
        std::fs::create_dir_all(imp.join("ships")).unwrap();
        let modd = root.join("mod");
        std::fs::create_dir_all(&modd).unwrap();
        std::os::unix::fs::symlink(&imp, modd.join("retail")).unwrap();
        std::os::unix::fs::symlink(imp.join("ships"), modd.join("ships")).unwrap();
        for p in [modd.join("retail/x.json"), modd.join("ships/A/ship.json"), modd.join("retail/not/yet/x.json")] {
            assert!(check_against(&p, &imports(&imp)).is_err(), "{}", p.display());
        }
        assert!(write(&modd.join("ships/B.json"), "x", Some(&imp)).is_err());
        assert!(!imp.join("ships/B.json").exists());
        // a dangling link cannot be told apart: refused too
        std::os::unix::fs::symlink(root.join("nowhere"), modd.join("dangling")).unwrap();
        assert!(check_against(&modd.join("dangling/x.json"), &imports(&imp)).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_sibling_folder_is_allowed_even_with_the_same_name_prefix() {
        let root = tmp("sibling");
        let imp = root.join("assets");
        std::fs::create_dir_all(&imp).unwrap();
        for p in [root.join("mod/ships/A/ship.json"), root.join("assets-mod/x.json"), root.join("assets2/x.json")] {
            assert!(check_against(&p, &imports(&imp)).is_ok(), "{}", p.display());
        }
        let f = root.join("mod/a/b.json");
        create_dir_all(f.parent().unwrap(), Some(&imp)).unwrap();
        write(&f, "{}", Some(&imp)).unwrap();
        copy(&f, &root.join("mod/c.json"), Some(&imp)).unwrap();
        remove_file(&f, Some(&imp)).unwrap();
        drop(create_file(&root.join("mod/z.zip"), Some(&imp)).unwrap());
        assert!(root.join("mod/c.json").is_file() && !f.exists() && root.join("mod/z.zip").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_mod_folder_made_inside_the_import_is_refused() {
        let root = tmp("modinside");
        let imp = root.join("assets");
        std::fs::create_dir_all(&imp).unwrap();
        let err = crate::project::Project::create(&imp.join("my-mod"), Some(imp.clone())).err().expect("refused");
        assert!(err.contains("inside the game import"), "{err}");
        assert!(!imp.join("my-mod").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn opening_or_creating_a_project_in_or_around_the_import_is_refused() {
        use crate::project::Project;
        let root = tmp("openguard");
        let imp = root.join("assets");
        std::fs::create_dir_all(imp.join("data")).unwrap();
        std::fs::write(imp.join("data/x.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("sibling")).unwrap();
        for d in [imp.join("data"), imp.clone(), root.clone()] {
            let e = Project::open(&d, Some(imp.clone())).err().unwrap_or_else(|| panic!("open {} accepted", d.display()));
            assert!(e.contains("refused"), "{e}");
            let e = Project::create(&d, Some(imp.clone())).err().unwrap_or_else(|| panic!("create {} accepted", d.display()));
            assert!(e.contains("refused"), "{e}");
        }
        assert!(!imp.join("mod.json").exists() && !root.join("mod.json").exists() && !imp.join("data/mod.json").exists());
        assert!(Project::open(&root.join("sibling"), Some(imp.clone())).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
}
