//! The scenery and sky catalogue of a map editor, built by scanning an overlay folder (the player's import with mods laid over it,
//! see [`crate::overlay::build`]). The engine has no list (docs/phase4.md requirement 3), so everything here is found, not asked.
//! Reads files only; names no map key (that is [`super::doc`]'s job).
//!
//! What is scanned, all PROVISIONAL until the engine lists these itself:
//!
//! - scenery `type` with its `scene`: the pairs used by `objects` of every `maps/<id>/map.json`; the pairing exists nowhere else;
//! - scene names: `*.glb` under any `scenery` folder below `models/` (the layout of the import is not verified);
//! - `has_collision` / `has_docks`: the type is a top-level key of `data/collision.json` / `data/docks.json`;
//! - skies: `models/sky/*.glb` and `models/sky/<sky>_cube.png` names and the keys of `data/render/skies.json`, with a `_LayerN`
//!   suffix dropped (the engine drops it too).

use super::doc;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One scenery type as found in the maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneryType {
    pub name: String,
    /// Every `scene` a map pairs with this type, sorted; usually one.
    pub scenes: Vec<String>,
    /// Objects of this type in all scanned maps.
    pub uses: usize,
    pub has_collision: bool,
    pub has_docks: bool,
}

impl SceneryType {
    /// The scene a new object of this type gets: the first (sorted) one.
    pub fn scene(&self) -> Option<&str> {
        self.scenes.first().map(String::as_str)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalogue {
    /// Sorted by type name.
    pub scenery: Vec<SceneryType>,
    /// Scene names of `*.glb` files under a `scenery` folder, sorted, without extension; includes ones no map uses yet.
    pub scene_files: Vec<String>,
    /// Sky names, sorted and without a `_LayerN` suffix.
    pub skies: Vec<String>,
    /// Files that could not be read or parsed (relative, with the reason); the scan goes on.
    pub problems: Vec<String>,
}

impl Catalogue {
    pub fn find(&self, ty: &str) -> Option<&SceneryType> {
        self.scenery.iter().find(|t| t.name == ty)
    }
}

/// `skybox_05_Layer2` -> `skybox_05`.
pub fn sky_name(s: &str) -> &str {
    match s.rsplit_once("_Layer") {
        Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => s,
    }
}

/// Scans `root` (an overlay folder: `maps/`, `models/`, `data/` below it). A missing folder or file is not an error: the catalogue is
/// just smaller.
pub fn scan(root: &Path) -> Catalogue {
    let mut problems = Vec::new();
    let mut pairs: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
    for map_dir in sorted_entries(&root.join("maps")).into_iter().filter(|p| p.is_dir()) {
        let file = map_dir.join(doc::MAP_FILE);
        let Some(map) = read_json(root, &file, &mut problems) else { continue };
        for o in doc::objects(&map) {
            let Some(ty) = doc::kind(o) else { continue };
            let e = pairs.entry(ty.to_string()).or_default();
            e.1 += 1;
            if let Some(s) = doc::scene(o) {
                e.0.insert(s.to_string());
            }
        }
    }
    let keys = |rel: &str, problems: &mut Vec<String>| -> BTreeSet<String> {
        read_json(root, &root.join(rel), problems).and_then(|v| v.as_object().map(|o| o.keys().cloned().collect())).unwrap_or_default()
    };
    let collision = keys("data/collision.json", &mut problems);
    let docks = keys("data/docks.json", &mut problems);
    let scenery = pairs
        .into_iter()
        .map(|(name, (scenes, uses))| SceneryType { has_collision: collision.contains(&name), has_docks: docks.contains(&name), name, scenes: scenes.into_iter().collect(), uses })
        .collect();

    let mut scene_files = BTreeSet::new();
    glb_under_scenery(&root.join("models"), false, &mut scene_files);

    let mut skies = BTreeSet::new();
    for p in sorted_entries(&root.join("models/sky")) {
        let (Some(stem), Some(ext)) = (p.file_stem().and_then(|s| s.to_str()), p.extension().and_then(|e| e.to_str())) else { continue };
        match ext.to_ascii_lowercase().as_str() {
            "glb" => skies.insert(sky_name(stem).to_string()),
            "png" => stem.strip_suffix("_cube").is_some_and(|s| skies.insert(sky_name(s).to_string())),
            _ => false,
        };
    }
    skies.extend(keys("data/render/skies.json", &mut problems).iter().map(|k| sky_name(k).to_string()));

    Catalogue { scenery, scene_files: scene_files.into_iter().collect(), skies: skies.into_iter().collect(), problems }
}

fn sorted_entries(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    v.sort();
    v
}

/// Collects `*.glb` stems in every folder named `scenery` (any depth) below `dir`.
fn glb_under_scenery(dir: &Path, inside: bool, out: &mut BTreeSet<String>) {
    for p in sorted_entries(dir) {
        if p.is_dir() {
            let here = inside || p.file_name().is_some_and(|n| n == "scenery");
            glb_under_scenery(&p, here, out);
        } else if inside
            && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("glb"))
            && let Some(s) = p.file_stem().and_then(|s| s.to_str())
        {
            out.insert(s.to_string());
        }
    }
}

/// A missing file is `None` without a problem; a file that does not parse is `None` with one.
fn read_json(root: &Path, file: &Path, problems: &mut Vec<String>) -> Option<Value> {
    let text = std::fs::read_to_string(file).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            problems.push(format!("{}: {e}", file.strip_prefix(root).unwrap_or(file).display()));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sky_name;

    #[test]
    fn layer_suffix_is_dropped_only_when_it_is_one() {
        assert_eq!(sky_name("skybox_05_Layer2"), "skybox_05");
        assert_eq!(sky_name("skybox_05"), "skybox_05");
        assert_eq!(sky_name("my_Layer"), "my_Layer");
        assert_eq!(sky_name("my_Layerx"), "my_Layerx");
    }
}
