//! Exporting a mod as a zip that unzips into the game's `mods/` folder (`mods/<id>/mod.json`): the plan is computed first so the UI
//! can show it (and any warnings) before anything is written. The export does not check the content (maintainer decision).

use std::io::Write;
use std::path::{Path, PathBuf};

/// What an export would contain and what the user should know first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPlan {
    /// The mod id: `mod.json`'s `id`, else the folder name. It is the zip's one top folder.
    pub id: String,
    pub mod_dir: PathBuf,
    /// Every file that goes in, relative to the mod folder with `/` separators, sorted.
    pub files: Vec<String>,
    /// Things worth a second look; the export is still allowed.
    pub warnings: Vec<String>,
}

fn skipped(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~') || name.ends_with(".tmp")
}

fn collect(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        let kind = entry.file_type().map_err(|e| format!("{}: {e}", entry.path().display()))?;
        let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if kind.is_dir() {
            collect(&entry.path(), &rel, out)?;
        } else if kind.is_file() {
            out.push(rel);
        }
    }
    Ok(())
}

fn mod_id(mod_dir: &Path) -> String {
    let from_json = std::fs::read(mod_dir.join("mod.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["id"].as_str().map(str::to_string))
        .filter(|id| !id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != "..");
    from_json.unwrap_or_else(|| std::path::absolute(mod_dir).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| "mod".into()))
}

/// Lists the files of the mod folder and the warnings. An error when `mod.json` is missing. By maintainer decision the export does
/// not check the content: users are responsible for what they share.
pub fn plan(mod_dir: &Path) -> Result<ExportPlan, String> {
    if !mod_dir.join("mod.json").is_file() {
        return Err(format!("{} has no mod.json: a mod folder needs one", mod_dir.display()));
    }
    let mut files = Vec::new();
    collect(mod_dir, "", &mut files)?;
    files.sort();
    let mut warnings = Vec::new();
    match ["LICENSE", "LICENSE.txt", "LICENSE.md"].iter().find(|n| files.iter().any(|f| f == *n)) {
        None => warnings.push("no LICENSE file: say under which licence others may use the mod".to_string()),
        // the notice the editor writes for a new mod (it names the licence but is not its text)
        Some(n) if std::fs::read_to_string(mod_dir.join(n)).is_ok_and(|t| t.contains(crate::project::licence::NOTICE_MARKER)) => {
            warnings.push(format!("{n} is only a notice; replace it with the full licence text before sharing"));
        }
        Some(_) => {}
    }
    Ok(ExportPlan { id: mod_id(mod_dir), mod_dir: mod_dir.to_path_buf(), files, warnings })
}

/// Writes the zip: every planned file under `<id>/`, sorted, with fixed timestamps and permissions so that two exports of the same
/// folder are byte-identical.
pub fn write_zip(plan: &ExportPlan, out: &Path) -> Result<(), String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", out.display());
    let file = crate::project::guard::create_file(out, None)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default()) // 1980-01-01 00:00:00
        .unix_permissions(0o644);
    let mut files: Vec<&String> = plan.files.iter().collect();
    files.sort();
    for rel in files {
        let bytes = std::fs::read(plan.mod_dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        zip.start_file(format!("{}/{rel}", plan.id), options).map_err(|e| err(&e))?;
        zip.write_all(&bytes).map_err(|e| err(&e))?;
    }
    zip.finish().map_err(|e| err(&e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn tree(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-export-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn sample(name: &str) -> (PathBuf, PathBuf) {
        let root = tree(name);
        let m = root.join("my_folder");
        put(&m, "mod.json", br#"{"id": "gates", "name": "Gates"}"#);
        put(&m, "LICENSE", b"CC0");
        put(&m, "ships/Ship_X/ship.json", b"{\"a\": 1}");
        put(&m, "maps/z/map.json", b"{}");
        put(&m, ".git/config", b"x");
        put(&m, ".hidden", b"x");
        (root, m)
    }

    #[test]
    fn plan_lists_sorted_files_skips_dot_entries_and_names_the_id() {
        let (root, m) = sample("plan");
        let p = plan(&m).unwrap();
        assert_eq!(p.id, "gates");
        assert_eq!(p.files, ["LICENSE", "maps/z/map.json", "mod.json", "ships/Ship_X/ship.json"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_licence_file_that_is_only_the_editors_notice_is_warned() {
        use crate::project::licence::Licence;
        let (root, m) = sample("notice");
        assert!(plan(&m).unwrap().warnings.is_empty(), "a real licence text is fine");
        put(&m, "LICENSE", Licence::CcBy4.notice("Gates", "Ada").as_bytes());
        let w = plan(&m).unwrap().warnings;
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("LICENSE is only a notice") && w[0].contains("full licence text"), "{w:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_licence_is_warned_and_game_like_files_are_not_checked() {
        let (root, m) = sample("warn");
        put(&m, "models/hull.MDB", b"x");
        put(&m, "tex/a.tga", b"x");
        std::fs::remove_file(m.join("LICENSE")).unwrap();
        let w = plan(&m).unwrap().warnings;
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("LICENSE"));
        put(&m, "LICENSE.md", b"MIT");
        assert!(plan(&m).unwrap().warnings.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_without_mod_json_is_an_error_and_the_id_falls_back_to_the_folder_name() {
        let root = tree("nomod");
        let m = root.join("cool_mod");
        put(&m, "x.json", b"{}");
        assert!(plan(&m).unwrap_err().contains("mod.json"));
        put(&m, "mod.json", b"{}");
        assert_eq!(plan(&m).unwrap().id, "cool_mod");
        put(&m, "mod.json", br#"{"id": "../evil"}"#);
        assert_eq!(plan(&m).unwrap().id, "cool_mod");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_zip_has_one_top_folder_sorted_entries_and_is_reproducible() {
        let (root, m) = sample("zip");
        let p = plan(&m).unwrap();
        let (a, b) = (root.join("a.zip"), root.join("b.zip"));
        write_zip(&p, &a).unwrap();
        write_zip(&p, &b).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());

        let mut z = zip::ZipArchive::new(std::fs::File::open(&a).unwrap()).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect();
        assert_eq!(names, ["gates/LICENSE", "gates/maps/z/map.json", "gates/mod.json", "gates/ships/Ship_X/ship.json"]);
        let mut text = String::new();
        z.by_name("gates/mod.json").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, std::fs::read_to_string(m.join("mod.json")).unwrap());
        let t = z.by_name("gates/LICENSE").unwrap().last_modified().unwrap();
        assert_eq!((t.year(), t.month(), t.day()), (1980, 1, 1));
        let _ = std::fs::remove_dir_all(&root);
    }
}
