//! A mod folder's mission: `maps/<id>/{mission,map,media}.json`, loaded, edited and saved in the engine's JSON layout.

use serde_json::Value;
use std::path::{Path, PathBuf};

use openprocyon::authoring::to_json_string;

pub struct Mission {
    pub dir: PathBuf,
    pub id: String,
    pub mission: Value,
    pub map: Value,
    pub media: Value,
}

/// The committed example mod (`tests/fixtures/gates`). Only meaningful when running from a checkout.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gates")
}

fn read(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

impl Mission {
    /// The first map folder of the mod that has a `mission.json` (a mod's `missions` order is not read in the spike).
    pub fn load(dir: &Path) -> Result<Mission, String> {
        let maps = dir.join("maps");
        let mut ids: Vec<String> = std::fs::read_dir(&maps)
            .map_err(|e| format!("{}: {e}", maps.display()))?
            .flatten()
            .filter(|e| e.path().join("mission.json").is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        ids.sort();
        let id = ids.into_iter().next().ok_or_else(|| format!("{}: no maps/<id>/mission.json", dir.display()))?;
        let at = maps.join(&id);
        let media = read(&at.join("media.json")).unwrap_or(Value::Null);
        let map = read(&at.join("map.json")).unwrap_or(Value::Null);
        Ok(Mission { dir: dir.into(), mission: read(&at.join("mission.json"))?, id, map, media })
    }

    /// Writes the three files in the engine's layout; `map.json` and `media.json` are written only if they were loaded.
    pub fn save(&self) -> Result<(), String> {
        let at = self.dir.join("maps").join(&self.id);
        for (name, v) in [("mission.json", &self.mission), ("map.json", &self.map), ("media.json", &self.media)] {
            if v.is_null() {
                continue;
            }
            crate::project::guard::write(&at.join(name), to_json_string(v), None)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_edit_save_round_trip() {
        let tmp = std::env::temp_dir().join(format!("procyon-editor-test-{}", std::process::id()));
        let at = tmp.join("maps/gate_ambush");
        std::fs::create_dir_all(&at).unwrap();
        for f in ["mission.json", "map.json", "media.json"] {
            std::fs::copy(fixture_dir().join("maps/gate_ambush").join(f), at.join(f)).unwrap();
        }
        let mut m = Mission::load(&tmp).unwrap();
        assert_eq!(m.id, "gate_ambush");
        m.save().unwrap();
        for f in ["mission.json", "map.json", "media.json"] {
            assert_eq!(std::fs::read(at.join(f)).unwrap(), std::fs::read(fixture_dir().join("maps/gate_ambush").join(f)).unwrap(), "{f}");
        }
        m.mission["rules"][0]["name"] = "Begin".into();
        m.save().unwrap();
        assert!(std::fs::read_to_string(at.join("mission.json")).unwrap().contains("\"name\": \"Begin\""));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
