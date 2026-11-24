//! Spike items 2, 3 and 7: an imported map's scenery as a top-down plan; drag a piece with the mouse, save `map.json` into a
//! scratch mod folder in the engine's layout, play-test it. Reads only the player's import; writes only into the chosen folder.
//! ponytail: a 2D plan of `objects[].pos` (x, y game axes), not the 3D scene; a 3D view with picking is Phase 2 work.

use bevy::prelude::Resource;
use bevy_egui::egui;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Child;

use crate::launch;
use openprocyon::authoring::to_json_string;

#[derive(Resource)]
pub struct State {
    id: String,
    out: String,
    map: Option<Value>,
    drag: Option<usize>,
    dirty: bool,
    status: String,
    child: Option<Child>,
}

impl Default for State {
    fn default() -> Self {
        State { id: "red_rover".into(), out: "local/spike-mod".into(), map: None, drag: None, dirty: false, status: String::new(), child: None }
    }
}

/// `maps/<id>/map.json` as the game would see it with the scratch mod in `out` laid over the import (`out` missing or empty: the
/// import alone), through the engine's overlay and reader (`authoring::overlay_into`, `read_map`) in the editor's own scratch folder.
pub fn load(assets: &Path, out: &Path, id: &str) -> Result<Value, String> {
    let mods: Vec<PathBuf> = has_files(out).then(|| std::path::absolute(out).unwrap_or_else(|_| out.to_path_buf())).into_iter().collect();
    let (root, _) = crate::overlay::build(assets, &mods, &crate::overlay::scratch("map-overlay"))?;
    openprocyon::authoring::read_map(&root, id).map_err(|e| e.to_string()).and_then(|v| if v.is_object() { Ok(v) } else { Err(format!("maps/{id}/map.json: not a JSON object")) })
}

fn has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}

/// Writes the edited map as `<out>/maps/<id>/map.json` (whole file: a mod's `map.json` without `extends` replaces the base one) and a
/// `mod.json` if the folder has none.
pub fn save(out: &Path, id: &str, map: &Value) -> Result<PathBuf, String> {
    let at = out.join("maps").join(id);
    std::fs::create_dir_all(&at).map_err(|e| format!("{}: {e}", at.display()))?;
    let modj = out.join("mod.json");
    if !modj.exists() {
        let m = json!({"id": "editor-scratch", "name": "Editor scratch", "version": "0.0", "description": "Written by the editor spike. Holds derived game data: never commit or share."});
        std::fs::write(&modj, to_json_string(&m)).map_err(|e| format!("{}: {e}", modj.display()))?;
    }
    let file = at.join("map.json");
    std::fs::write(&file, to_json_string(map)).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// A dragged coordinate as the f32's shortest decimal (`-369.23077`), not the widened f64 (`-369.23077392578125`).
/// The engine reads positions as f32 anyway, so nothing is lost.
pub fn short(v: f32) -> Value {
    json!(v.to_string().parse::<f64>().unwrap_or(v as f64))
}

fn pos(o: &Value) -> Option<egui::Vec2> {
    Some(egui::vec2(o["pos"][0].as_f64()? as f32, o["pos"][1].as_f64()? as f32))
}

pub fn show(ui: &mut egui::Ui, st: &mut State, assets: Option<&Path>) {
    ui.horizontal(|ui| {
        ui.label("Map id");
        ui.text_edit_singleline(&mut st.id);
        if ui.add_enabled(assets.is_some(), egui::Button::new("Load imported map")).clicked()
            && let Some(a) = assets
        {
            match load(a, Path::new(&st.out), &st.id) {
                Ok(m) => {
                    st.status = format!("Loaded {} ({} objects)", st.id, m["objects"].as_array().map_or(0, Vec::len));
                    st.map = Some(m);
                    st.dirty = false;
                }
                Err(e) => st.status = e,
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Scratch mod folder");
        ui.text_edit_singleline(&mut st.out);
        if ui.add_enabled(st.map.is_some(), egui::Button::new("Save map.json")).clicked()
            && let Some(m) = &st.map
        {
            match save(Path::new(&st.out), &st.id, m) {
                Ok(f) => {
                    st.dirty = false;
                    st.status = format!("Saved {}", f.display());
                }
                Err(e) => st.status = e,
            }
        }
        let running = st.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if !running {
            st.child = None;
        }
        if ui.add_enabled(!running && st.map.is_some(), egui::Button::new("Play-test map")).clicked() {
            let dir = std::path::absolute(&st.out).unwrap_or_else(|_| PathBuf::from(&st.out));
            match launch::spawn(&dir, &st.id) {
                Ok(c) => {
                    st.child = Some(c);
                    st.status = format!("Game started: --mod {} --map {}", dir.display(), st.id);
                }
                Err(e) => st.status = format!("cannot start {}: {e} (set PROCYON_GAME_BIN)", launch::game_bin().display()),
            }
        }
        if st.dirty {
            ui.label("unsaved changes");
        }
    });
    if !st.status.is_empty() {
        ui.label(&st.status);
    }
    let Some(map) = st.map.as_mut() else { return };
    let half = map["world_half"].as_f64().unwrap_or(1600.0) as f32;
    let side = ui.available_width().min(520.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::drag());
    let scale = side / (2.0 * half);
    // Game x to the right, game y (forward, north) up.
    let to_screen = |p: egui::Vec2| rect.center() + egui::vec2(p.x, -p.y) * scale;
    let to_world = |s: egui::Pos2| egui::vec2(s.x - rect.center().x, rect.center().y - s.y) / scale;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(12, 16, 32));
    let objects = map["objects"].as_array_mut();
    let Some(objects) = objects else { return };
    if resp.drag_started()
        && let Some(at) = resp.interact_pointer_pos()
    {
        // Nearest object within 12 px.
        st.drag = objects
            .iter()
            .enumerate()
            .filter_map(|(i, o)| Some((i, pos(o)?)))
            .map(|(i, p)| (i, (to_screen(p) - at).length()))
            .filter(|(_, d)| *d < 12.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
    }
    if resp.dragged()
        && let (Some(i), Some(at)) = (st.drag, resp.interact_pointer_pos())
    {
        let w = to_world(at);
        objects[i]["pos"][0] = short(w.x);
        objects[i]["pos"][1] = short(w.y);
        st.dirty = true;
    }
    if resp.drag_stopped()
        && let Some(i) = st.drag.take()
    {
        let o = &objects[i];
        st.status = format!("Moved {} #{} to [{:.1}, {:.1}]", o["type"].as_str().unwrap_or("?"), o["id"], o["pos"][0].as_f64().unwrap_or(0.0), o["pos"][1].as_f64().unwrap_or(0.0));
    }
    for (i, o) in objects.iter().enumerate() {
        let Some(p) = pos(o) else { continue };
        let ty = o["type"].as_str().unwrap_or_default();
        let island = ty.to_lowercase().contains("island");
        let color = if st.drag == Some(i) {
            egui::Color32::YELLOW
        } else if island {
            egui::Color32::from_rgb(110, 170, 90)
        } else {
            egui::Color32::LIGHT_BLUE
        };
        painter.circle_filled(to_screen(p), if island { 7.0 } else { 4.0 }, color);
        painter.text(to_screen(p) + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, o["id"].to_string(), egui::FontId::monospace(10.0), egui::Color32::GRAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragged_value_round_trips_to_a_short_number() {
        let v = short(-369.230_77_f32);
        assert_eq!(to_json_string(&v).trim(), "-369.23077");
        assert_eq!(v.as_f64().unwrap() as f32, -369.230_77_f32);
    }

    #[test]
    fn save_writes_map_and_mod_json_in_the_engine_layout() {
        let tmp = std::env::temp_dir().join(format!("procyon-editor-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let map = json!({"world_half": 100.0, "objects": [{"type": "Rock", "id": 1, "pos": [1.5, -2.0, 0.0]}]});
        let file = save(&tmp, "m", &map).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), to_json_string(&map));
        assert!(tmp.join("mod.json").is_file());
        // read back through the engine's overlay and loader: the mod's map over an import that has another version of it
        let base = tmp.with_extension("base");
        std::fs::create_dir_all(base.join("maps/m")).unwrap();
        std::fs::write(base.join("maps/m/map.json"), r#"{"world_half": 5.0, "objects": []}"#).unwrap();
        assert_eq!(load(&base, &tmp, "m").unwrap(), map);
        assert_eq!(load(&base, &tmp.with_extension("none"), "m").unwrap()["world_half"], 5.0, "no mod: the import alone");
        assert!(load(&base, &tmp, "nope").is_err());
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
