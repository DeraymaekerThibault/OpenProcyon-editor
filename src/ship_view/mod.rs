//! The 3D ship view (Phase 2, P5): the model's layers drawn from the hit triangles the engine itself sees, the ship's hardpoints as
//! gizmos, and moving or adding a hardpoint with the mouse. Placement lives here and is saved to `ship.json`, never written into the
//! GLB (docs/roadmap.md principle 7).
//!
//! `coords`, `points`, `place` and `pick` are pure and tested headless; `scene` (Bevy) and `panel` (egui) are thin. The project
//! (P3/P4) feeds [`ShipView`] and drains [`ShipView::pending`] into the undo stack; until then the panel has a path field.

pub mod coords;
pub mod panel;
pub mod pick;
pub mod place;
pub mod points;
pub mod scene;

use crate::edit::{Edit, apply};
use bevy::math::Vec2;
use bevy::prelude::Resource;
use openprocyon::data::derive::{self, Facts};
use points::{CrewKind, Hardpoint, Kind};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// What the panel tells the 3D systems about the pointer this frame (egui is the only reader of the mouse, so a window or a widget
/// in front of the view takes the input first). Positions are logical pixels of the window.
#[derive(Clone, Copy, Default, Debug)]
pub struct ViewInput {
    /// The central rect the 3D camera draws in: `(min, max)`; `None` while the tab is not shown.
    pub rect: Option<(Vec2, Vec2)>,
    /// Egui's pixels per point (logical to physical for the camera viewport).
    pub pixels_per_point: f32,
    pub hover: Option<Vec2>,
    pub primary_pressed: bool,
    pub primary_down: bool,
    pub primary_released: bool,
    /// Drag with the right or middle button, for orbiting.
    pub orbit_drag: Vec2,
    pub scroll: f32,
}

/// "10 layers (+2 damage variants)": the layers the view shows, and how many more are damage variants it does not draw.
pub fn layers_summary(facts: Option<&Facts>) -> String {
    let all = facts.map_or(&[][..], |f| &f.layers[..]);
    let damage = all.iter().filter(|l| points::is_damage_layer(&l.name)).count();
    let shown = all.len() - damage;
    let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    if damage == 0 { plural(shown, "layer") } else { format!("{} (+{})", plural(shown, "layer"), plural(damage, "damage variant")) }
}

/// What a click on the hull will add (set by the "Add" buttons).
#[derive(Clone, Debug, PartialEq)]
pub struct Armed {
    pub kind: Kind,
    /// The new entry's name (a crew place: the whole node name).
    pub label: String,
}

/// The ship being edited and what the view shows of it.
#[derive(Resource)]
pub struct ShipView {
    pub ship_id: String,
    /// The merged `ship.json`; edits are applied here as they are made.
    pub ship: Value,
    pub glb: Option<PathBuf>,
    pub facts: Option<Facts>,
    /// The selected point, by kind and label.
    pub selected: Option<(Kind, String)>,
    pub hovered_layer: Option<usize>,
    pub hovered_point: Option<usize>,
    /// Edits made since the project last drained them, one entry per user action (one drag is one entry), as applied (`old` filled).
    pub pending: Vec<Vec<Edit>>,
    /// The text field of the path to a ship folder, `ship.json` or `model.glb`.
    pub path: String,
    /// A ship id typed in the load box ([`looks_like_id`]); the caller that has the project looks it up and fills the view.
    pub load_id: Option<String>,
    pub status: String,
    pub armed: Option<Armed>,
    pub new_label: String,
    pub crew_kind: CrewKind,
    /// Bumped when a model is loaded; the scene rebuilds its meshes.
    pub generation: u64,
    pub input: ViewInput,
    /// A label to paint at a window position (hovered point), set by the scene and painted by the panel.
    pub hover_label: Option<(Vec2, String)>,
    /// Draw the damaged variants of sections too (faint, never pickable). Off by default: the engine drops them from the hull.
    pub show_damage: bool,
}

impl Default for ShipView {
    fn default() -> Self {
        ShipView {
            ship_id: String::new(),
            ship: json!({}),
            glb: None,
            facts: None,
            selected: None,
            hovered_layer: None,
            hovered_point: None,
            pending: vec![],
            path: String::new(),
            load_id: None,
            status: String::new(),
            armed: None,
            new_label: String::new(),
            crew_kind: CrewKind::Walk,
            generation: 0,
            input: ViewInput::default(),
            hover_label: None,
            show_damage: false,
        }
    }
}

impl ShipView {
    /// The ship's points as the loader will see them.
    pub fn points(&self) -> Vec<Hardpoint> {
        points::hardpoints(&self.ship, self.facts.as_ref())
    }

    /// Applies `edits` to the local ship and records them as one undo step. On an error nothing stays applied and nothing is recorded.
    pub fn commit(&mut self, edits: Vec<Edit>) -> Result<(), String> {
        let mut doc = self.ship.clone();
        let applied = edits.iter().map(|e| apply(&mut doc, e)).collect::<Result<Vec<_>, _>>()?;
        self.ship = doc;
        if !applied.is_empty() {
            self.pending.push(applied);
        }
        Ok(())
    }

    /// Empties the view (another mod was opened); what the user switched on for display stays.
    pub fn clear(&mut self) {
        let (generation, show_damage) = (self.generation + 1, self.show_damage);
        *self = ShipView { generation, show_damage, ..ShipView::default() };
    }

    /// Replaces the ship with `loaded` (the pending edits of the old one are dropped by the caller's choice: they belong to it).
    pub fn set(&mut self, loaded: Loaded) {
        self.ship_id = loaded.ship_id;
        self.ship = loaded.ship;
        self.glb = loaded.glb;
        self.facts = loaded.facts;
        self.selected = None;
        self.hovered_layer = None;
        self.hovered_point = None;
        self.armed = None;
        self.pending.clear();
        self.generation += 1;
    }
}

/// Whether the text of the load box names a ship (`Ship_Cutter`) rather than a path: not empty, no folder separator, no file
/// extension, and no such file or folder in the working folder.
pub fn looks_like_id(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && !t.contains(['/', '\\']) && Path::new(t).extension().is_none() && !Path::new(t).exists()
}

/// A ship read from disk.
pub struct Loaded {
    pub ship_id: String,
    pub ship: Value,
    pub glb: Option<PathBuf>,
    pub facts: Option<Facts>,
}

/// Reads a ship from a folder (`ship.json` and `model.glb` in it), a `ship.json`, or a `.glb` (with a `ship.json` beside it if there
/// is one). A missing `ship.json` is an empty ship; a missing or unreadable model is no facts (the points of `ship.json` still show).
/// PROVISIONAL: the ship's `model` key (another ship's folder) is not followed; the project will resolve it.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let (dir, json_file, glb_file) = if path.is_dir() {
        (path.to_path_buf(), path.join("ship.json"), path.join("model.glb"))
    } else {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("json") => (dir.clone(), path.to_path_buf(), dir.join("model.glb")),
            Some("glb") => (dir.clone(), dir.join("ship.json"), path.to_path_buf()),
            _ => return Err(format!("{}: not a ship folder, ship.json or .glb", path.display())),
        }
    };
    let ship = if json_file.is_file() {
        let text = std::fs::read_to_string(&json_file).map_err(|e| format!("{}: {e}", json_file.display()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", json_file.display()))?;
        if !v.is_object() {
            return Err(format!("{}: not a JSON object", json_file.display()));
        }
        v
    } else {
        json!({})
    };
    let (glb, facts) = match std::fs::read(&glb_file) {
        Ok(bytes) => (Some(glb_file.clone()), Some(derive::read(&bytes).ok_or_else(|| format!("{}: not a readable binary glTF", glb_file.display()))?)),
        Err(_) => (None, None),
    };
    if !json_file.is_file() && glb.is_none() {
        return Err(format!("{}: no ship.json and no model.glb found", dir.display()));
    }
    let ship_id = std::path::absolute(&dir).ok().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
    Ok(Loaded { ship_id, ship, glb, facts })
}

#[cfg(test)]
pub(crate) mod test_glb {
    use serde_json::json;

    /// A minimal binary glTF: one triangle mesh node `Hull_Main` and the empty nodes the hardpoint rules know, in glTF axes.
    pub fn synthetic() -> Vec<u8> {
        let mut bin = vec![];
        for v in [[-1.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
            for c in v {
                bin.extend(c.to_le_bytes());
            }
        }
        let doc = json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0, 1, 2, 3, 4]}],
            "nodes": [
                {"name": "Hull_Main", "mesh": 0},
                {"name": "Gun_Mount_A", "translation": [1.0, 3.0, -2.0], "extras": {"kind": "GunPlacement"}},
                {"name": "Engine_1", "translation": [0.0, 1.0, 5.0]},
                {"name": "Tow_Front", "translation": [0.0, 1.0, -6.0]},
                {"name": "Homing", "translation": [0.0, 2.0, 0.0]}
            ],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [-1.0, 0.0, 0.0], "max": [1.0, 2.0, 0.0]}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}],
            "buffers": [{"byteLength": 36}]
        });
        let mut text = serde_json::to_vec(&doc).unwrap();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let total = 12 + 8 + text.len() + 8 + bin.len();
        let mut out = b"glTF".to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((text.len() as u32).to_le_bytes());
        out.extend(0x4E4F534Au32.to_le_bytes());
        out.extend(text);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(0x004E4942u32.to_le_bytes());
        out.extend(bin);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::points::{Kind, Source};
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-shipview-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_engine_reads_the_synthetic_model_into_a_layer_and_points() {
        let facts = derive::read(&test_glb::synthetic()).expect("a readable glb");
        assert_eq!(facts.layers.len(), 1);
        assert_eq!(facts.layers[0].name, "Hull_Main");
        assert_eq!(facts.layers[0].tris.len(), 9);
        let by_name = |n: &str| facts.points.iter().find(|p| p.name == n).unwrap_or_else(|| panic!("no point {n}"));
        assert_eq!((by_name("Gun_Mount_A").kind.as_str(), by_name("Gun_Mount_A").pos), ("GunPlacement", [1.0, 2.0, 3.0]));
        assert_eq!((by_name("Engine_1").kind.as_str(), by_name("Engine_1").pos), ("EnginePortPlacement", [0.0, -5.0, 1.0]));
        assert_eq!((by_name("Tow_Front").kind.as_str(), by_name("Tow_Front").pos), ("ToweePoint", [0.0, 6.0, 1.0]));
        assert_eq!((by_name("Homing").kind.as_str(), by_name("Homing").pos), ("TorpedoHomingPoint", [0.0, 0.0, 2.0]));
    }

    #[test]
    fn hardpoints_of_the_synthetic_model_with_a_ship_json_overriding_one() {
        let facts = derive::read(&test_glb::synthetic()).unwrap();
        let ship = json!({"guns": {"Gun_Mount_A": {"gun": "G", "arc_deg": 60, "pos": [9, 9, 9], "rot": [0, 0, 0]}}, "homing": [0, 1, 1]});
        let hp = points::hardpoints(&ship, Some(&facts));
        let get = |k: Kind| hp.iter().find(|h| h.kind == k).unwrap();
        assert_eq!((get(Kind::Gun).pos, &get(Kind::Gun).source), ([9.0, 9.0, 9.0], &Source::Json("/guns/Gun_Mount_A".into())));
        assert_eq!((get(Kind::Homing).pos, &get(Kind::Homing).source), ([0.0, 1.0, 1.0], &Source::Json("/homing".into())));
        assert_eq!(get(Kind::Engine).source, Source::Model("Engine_1".into()));
        assert_eq!(get(Kind::TowTowee).pos, [0.0, 6.0, 1.0]);
        assert_eq!(hp.len(), 4);
    }

    #[test]
    fn load_reads_a_folder_a_ship_json_or_a_glb_and_refuses_the_rest() {
        let d = tmp("load").join("Ship_Test");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("model.glb"), test_glb::synthetic()).unwrap();
        std::fs::write(d.join("ship.json"), r#"{"homing": [1, 2, 3]}"#).unwrap();
        for p in [d.clone(), d.join("ship.json"), d.join("model.glb")] {
            let l = load(&p).unwrap();
            assert_eq!((l.ship_id.as_str(), l.ship["homing"][2].as_i64(), l.facts.as_ref().map(|f| f.layers.len())), ("Ship_Test", Some(3), Some(1)), "{p:?}");
        }
        std::fs::remove_file(d.join("ship.json")).unwrap();
        assert_eq!(load(&d).unwrap().ship, json!({}), "no ship.json: an empty ship");
        std::fs::write(d.join("ship.json"), "[1]").unwrap();
        assert!(load(&d).is_err());
        std::fs::write(d.join("ship.json"), "{").unwrap();
        assert!(load(&d).is_err());
        std::fs::write(d.join("model.glb"), b"nope").unwrap();
        assert!(load(&d).is_err(), "an unreadable model is reported");
        assert!(load(&d.join("x.txt")).is_err());
        assert!(load(&tmp("empty")).is_err(), "nothing there");
        let _ = std::fs::remove_dir_all(d.parent().unwrap());
        let _ = std::fs::remove_dir_all(tmp("empty"));
    }

    #[test]
    fn commit_applies_edits_records_one_undo_step_and_is_all_or_nothing() {
        let mut v = ShipView { ship: json!({"homing": [0, 0, 0]}), ..Default::default() };
        v.commit(vec![Edit::set("/homing", json!([1, 2, 3])), Edit::set("/tow", json!({}))]).unwrap();
        assert_eq!(v.pending.len(), 1);
        assert_eq!(v.pending[0][0].old, Some(json!([0, 0, 0])));
        let before = v.ship.clone();
        assert!(v.commit(vec![Edit::set("/homing", json!(1)), Edit::set("/homing/x/y", json!(1))]).is_err());
        assert_eq!((v.ship, v.pending.len()), (before, 1));
    }

    #[test]
    fn set_replaces_the_ship_and_clears_the_selection() {
        let mut v = ShipView { selected: Some((Kind::Homing, "homing".into())), pending: vec![vec![]], ..Default::default() };
        v.set(Loaded { ship_id: "S".into(), ship: json!({"a": 1}), glb: None, facts: None });
        assert_eq!((v.ship_id.as_str(), v.selected.is_none(), v.pending.len(), v.generation), ("S", true, 0, 1));
    }
}
