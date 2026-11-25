//! The settings of the open map (docs/phase4.md T6): name, description, `world_half`, sky, lighting and the star map's backdrop
//! picture. Every change is an undo step of the map's project document ([`Project::apply_form`]), so on a retail map it is saved as a
//! diff over it, and the picture is copied into the mod through the project's write guard.
//!
//! The fields are the ones T6 edits with their own widgets (sky picker, lighting toggle and colours, backdrop copy), so [`show`] is
//! not `form::show_typed` over the whole map schema; what the schema (`map.schema.json`, the engine's `--dump-schemas`) says about
//! each field, its description, is the hover text ([`hint`]), and `tests/map_schema.rs` holds every key and type the form edits to the
//! schema. Names no map key: that is [`super::doc`].

use std::path::Path;

use bevy_egui::egui;
use serde_json::Value;

use super::doc::{self, LightColour};
use super::project::{map_rel, merged_view, open_map};
use crate::edit::{Edit, pointer};
use crate::map_view::short;
use crate::project::{Project, guard};

/// The engine's description of the map field at `ptr` (`/world_half`, `/lighting/roof`), from `map.schema.json`; empty when the schema
/// has none.
pub fn hint(ptr: &str) -> String {
    use std::sync::OnceLock;
    static SCHEMA: OnceLock<crate::form::model::Node> = OnceLock::new();
    let node = SCHEMA.get_or_init(|| {
        let schema = openprocyon::authoring::schemas().into_iter().find(|(n, _)| *n == "map.schema.json").map(|(_, s)| s).unwrap_or_default();
        crate::form::model::Node::from_schema(&schema)
    });
    node.at(ptr).map(|n| n.description.clone()).unwrap_or_default()
}

/// A free-text setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    Name,
    Description,
}

impl Text {
    fn key(self) -> &'static str {
        match self {
            Text::Name => doc::KEY_NAME,
            Text::Description => doc::KEY_DESCRIPTION,
        }
    }
}

/// The 8 bytes every PNG starts with.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Applies `edits` to the open map `id` as one undo step labelled `label`; `typing` joins consecutive edits of one box into one step.
fn apply(project: &mut Project, id: &str, label: &str, edits: Vec<Edit>, typing: bool) -> Result<(), String> {
    let rel = map_rel(id)?;
    open_map(project, id)?;
    project.apply_form(&rel, label, edits, typing)
}

/// The map as the game would see it now (opened first when it is not yet).
fn current(project: &mut Project, id: &str) -> Result<Value, String> {
    open_map(project, id)?;
    merged_view(project, id).cloned().ok_or_else(|| format!("map '{id}' is not open"))
}

/// Sets `name` or `description`. Typing in one box is one undo step.
pub fn set_text(project: &mut Project, id: &str, which: Text, text: &str, typing: bool) -> Result<(), String> {
    let label = format!("Set map {}", which.key());
    apply(project, id, &label, vec![Edit::set(pointer(&[which.key()]), Value::from(text))], typing)
}

/// Sets `world_half`: a finite number above zero (the playable square is `2 * world_half` wide). Written as the f32's shortest decimal.
pub fn set_world_half(project: &mut Project, id: &str, half: f64, typing: bool) -> Result<(), String> {
    if !half.is_finite() || half <= 0.0 {
        return Err(format!("world_half must be a number above 0, not {half}"));
    }
    apply(project, id, "Set world_half", vec![Edit::set(pointer(&[doc::KEY_WORLD_HALF]), short(half as f32))], typing)
}

/// Picks the sky (`None`: no key, the engine's default sky). Picking the sky the map has already (same name, with or without a
/// `_LayerN` suffix) changes nothing, so a retail suffix is kept.
pub fn set_sky(project: &mut Project, id: &str, sky: Option<&str>) -> Result<(), String> {
    let now = doc::sky(&current(project, id)?).map(str::to_string);
    let same = match (now.as_deref(), sky) {
        (None, None) => true,
        (Some(a), Some(b)) => super::catalogue::sky_name(a) == super::catalogue::sky_name(b),
        _ => false,
    };
    if same {
        return Ok(());
    }
    let ptr = pointer(&[doc::KEY_SKY]);
    let edit = match sky {
        Some(s) => Edit::set(ptr, Value::from(s)),
        // A mod file cannot remove a key the retail map has (the diff keeps it), so there the default is written out.
        None if inherited(project, id, doc::KEY_SKY) => Edit::set(ptr, Value::from(doc::DEFAULT_SKY)),
        None => Edit::remove(ptr),
    };
    apply(project, id, "Set sky", vec![edit], false)
}

/// Whether the map below the mod's file (the retail map, or the one it extends) has `key` at the top: such a key cannot be removed by
/// a mod, only given another value (engine requirement, see [`crate::project::diff`]'s removal warning).
fn inherited(project: &Project, id: &str, key: &str) -> bool {
    map_rel(id).ok().and_then(|rel| project.doc(&rel)).is_some_and(|d| d.base.get(key).is_some())
}

/// Turns the map's own `lighting` on (the editor's starting values, [`doc::default_lighting`]) or off (the engine's plain white light).
pub fn set_lighting_enabled(project: &mut Project, id: &str, on: bool) -> Result<(), String> {
    if doc::has_lighting(&current(project, id)?) == on {
        return Ok(());
    }
    if !on && inherited(project, id, doc::KEY_LIGHTING) {
        return Err("the retail map has its own lighting and a mod cannot remove it: edit its colours instead".into());
    }
    let edit = if on { Edit::set(pointer(&[doc::KEY_LIGHTING]), doc::default_lighting()) } else { Edit::remove(pointer(&[doc::KEY_LIGHTING])) };
    apply(project, id, if on { "Turn lighting on" } else { "Turn lighting off" }, vec![edit], false)
}

/// Sets one lighting colour (RGBA, each 0..1; values outside are clamped). Refused when the map has no `lighting` (turn it on first:
/// the engine applies a `lighting` only when it has every colour and a direction).
pub fn set_light_colour(project: &mut Project, id: &str, which: LightColour, rgba: [f32; 4], typing: bool) -> Result<(), String> {
    if !doc::has_lighting(&current(project, id)?) {
        return Err("the map has no lighting: turn it on first".into());
    }
    let v = Value::Array(rgba.iter().map(|c| short(c.clamp(0.0, 1.0))).collect());
    apply(project, id, &format!("Set {} light", which.key()), vec![Edit::set(pointer(&[doc::KEY_LIGHTING, which.key()]), v)], typing)
}

/// Sets the sun's travel direction. Refused when it is the zero vector or not finite (the engine could not normalise it).
pub fn set_light_direction(project: &mut Project, id: &str, dir: [f32; 3], typing: bool) -> Result<(), String> {
    if !dir.iter().all(|c| c.is_finite()) || dir.iter().all(|&c| c == 0.0) {
        return Err("the sun direction needs a non-zero vector".into());
    }
    if !doc::has_lighting(&current(project, id)?) {
        return Err("the map has no lighting: turn it on first".into());
    }
    let v = Value::Array(dir.iter().map(|&c| short(c)).collect());
    apply(project, id, "Set sun direction", vec![Edit::set(pointer(&[doc::KEY_LIGHTING, doc::KEY_LIGHT_DIRECTION]), v)], typing)
}

/// Whether `file` is a PNG the star map can show: named `.png` and starting with the PNG signature. Errors say why not.
pub fn check_picture(file: &Path) -> Result<(), String> {
    if !file.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Err(format!("{}: the backdrop must be a .png picture", file.display()));
    }
    let mut head = [0u8; 8];
    let mut f = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    std::io::Read::read_exact(&mut f, &mut head).map_err(|_| format!("{}: not a PNG picture", file.display()))?;
    if head == PNG_MAGIC { Ok(()) } else { Err(format!("{}: not a PNG picture", file.display())) }
}

/// Gives the map a star map picture: copies `file` to `maps/<id>/backdrop.png` of the mod, through the write guard (never into the
/// import), and sets `backdrop` to the map id (the engine reads any string as "on" and loads that file; PROVISIONAL). The copy is a
/// file write, not an undo step: undo turns the key off again and leaves the picture in the mod.
pub fn set_backdrop(project: &mut Project, id: &str, file: &Path) -> Result<(), String> {
    map_rel(id)?;
    check_picture(file)?;
    let to = project.dir.join(doc::backdrop_rel(id));
    if let Some(parent) = to.parent() {
        guard::create_dir_all(parent, project.import.as_deref())?;
    }
    guard::copy(file, &to, project.import.as_deref())?;
    let edit = Edit::set(pointer(&[doc::KEY_BACKDROP]), Value::from(id));
    apply(project, id, "Set backdrop", vec![edit], false)
}

/// Turns the star map picture off: removes `backdrop`, or writes `false` over one the retail map has (a mod cannot remove a key; the
/// engine reads `false` as no picture). The copied file stays in the mod.
pub fn clear_backdrop(project: &mut Project, id: &str) -> Result<(), String> {
    if !doc::has_backdrop(&current(project, id)?) {
        return Ok(());
    }
    let ptr = pointer(&[doc::KEY_BACKDROP]);
    let edit = if inherited(project, id, doc::KEY_BACKDROP) { Edit::set(ptr, Value::Bool(false)) } else { Edit::remove(ptr) };
    apply(project, id, "Remove backdrop", vec![edit], false)
}

/// What the panel remembers between frames.
#[derive(Default)]
pub struct State {
    /// The picture path typed in the box.
    pub picture: String,
    /// The last refusal, shown under the form.
    pub error: Option<String>,
    pub info: String,
}

impl State {
    fn run(&mut self, r: Result<(), String>) {
        match r {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e),
        }
    }
}

fn rgba(c: [f64; 4]) -> [f32; 4] {
    c.map(|x| x as f32)
}

/// The settings form of map `id` (open in `project`); `skies` are the catalogue's. Edits are applied as they are made.
pub fn show(ui: &mut egui::Ui, st: &mut State, project: &mut Project, id: &str, skies: &[String]) {
    let Ok(map) = current(project, id) else {
        ui.label("The map is not open.");
        return;
    };

    let mut name = doc::name(&map).unwrap_or_default().to_string();
    ui.horizontal(|ui| {
        ui.label("Name");
        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0)).on_hover_text(hint("/name")).changed() {
            let r = set_text(project, id, Text::Name, &name, true);
            st.run(r);
        }
    });
    let mut desc = doc::description(&map).unwrap_or_default().to_string();
    ui.label("Description");
    if ui.add(egui::TextEdit::multiline(&mut desc).desired_rows(3).desired_width(f32::INFINITY)).on_hover_text(hint("/description")).changed() {
        let r = set_text(project, id, Text::Description, &desc, true);
        st.run(r);
    }

    let mut half = doc::world_half(&map).unwrap_or(doc::DEFAULT_WORLD_HALF);
    ui.horizontal(|ui| {
        ui.label("world_half");
        let r = ui.add(egui::DragValue::new(&mut half).speed(10.0).range(10.0..=100_000.0)).on_hover_text(hint("/world_half"));
        if r.changed() {
            let r = set_world_half(project, id, half, true);
            st.run(r);
        }
    });

    ui.separator();
    let now = doc::sky(&map).map(str::to_string);
    let shown = now.as_deref().map(|s| super::catalogue::sky_name(s).to_string());
    let mut pick: Option<Option<String>> = None;
    ui.horizontal(|ui| {
        ui.label("Sky").on_hover_text(hint("/sky"));
        egui::ComboBox::from_id_salt(("map-sky", id)).selected_text(shown.clone().unwrap_or_else(|| "(default)".into())).show_ui(ui, |ui| {
            if ui.selectable_label(shown.is_none(), "(default)").clicked() {
                pick = Some(None);
            }
            // A sky the catalogue does not list (a typo, a missing import) stays selectable-looking so it is not lost silently.
            let unlisted = shown.iter().filter(|s| !skies.contains(s));
            for s in skies.iter().chain(unlisted) {
                if ui.selectable_label(shown.as_deref() == Some(s), s).clicked() {
                    pick = Some(Some(s.clone()));
                }
            }
        });
        if skies.is_empty() {
            ui.label(egui::RichText::new("no skies found in the import").weak());
        }
    });
    if let Some(p) = pick {
        let r = set_sky(project, id, p.as_deref());
        st.run(r);
    }

    ui.separator();
    let mut on = doc::has_lighting(&map);
    if ui.checkbox(&mut on, "Own lighting").on_hover_text(hint("/lighting")).changed() {
        let r = set_lighting_enabled(project, id, on);
        st.run(r);
    }
    if on {
        for which in LightColour::ALL {
            let mut c = rgba(doc::light_colour(&map, which).unwrap_or([1.0; 4]));
            ui.horizontal(|ui| {
                ui.label(match which {
                    LightColour::Roof => "Sun (roof)",
                    LightColour::Floor => "Ambient (floor)",
                    LightColour::Ambient => "Ambient (unused by the engine, PROVISIONAL)",
                })
                .on_hover_text(hint(&pointer(&[doc::KEY_LIGHTING, which.key()])));
                if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() {
                    let r = set_light_colour(project, id, which, c, true);
                    st.run(r);
                }
            });
        }
        let mut d = doc::light_direction(&map).unwrap_or([0.0, 0.0, -1.0]).map(|x| x as f32);
        ui.horizontal(|ui| {
            ui.label("Sun travels along").on_hover_text(hint(&pointer(&[doc::KEY_LIGHTING, doc::KEY_LIGHT_DIRECTION])));
            let mut changed = false;
            for (axis, v) in ["x", "y", "z"].iter().zip(d.iter_mut()) {
                changed |= ui.add(egui::DragValue::new(v).speed(0.02).range(-1.0..=1.0).prefix(*axis)).changed();
            }
            if changed {
                let r = set_light_direction(project, id, d, true);
                st.run(r);
            }
        });
    }

    ui.separator();
    ui.label(format!("Backdrop picture (star map): {}", if doc::has_backdrop(&map) { "on" } else { "off" })).on_hover_text(hint("/backdrop"));
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.picture).hint_text("path of a .png").desired_width(220.0));
        if ui.button("Browse").clicked()
            && let Some(f) = rfd::FileDialog::new().add_filter("PNG picture", &["png"]).pick_file()
        {
            st.picture = f.display().to_string();
        }
        if ui.add_enabled(!st.picture.trim().is_empty(), egui::Button::new("Copy into the mod")).clicked() {
            let r = set_backdrop(project, id, Path::new(st.picture.trim()));
            if r.is_ok() {
                st.info = format!("Copied to {}.", doc::backdrop_rel(id));
            }
            st.run(r);
        }
        if ui.add_enabled(doc::has_backdrop(&map), egui::Button::new("Remove")).clicked() {
            let r = clear_backdrop(project, id);
            st.run(r);
        }
    });
    // One line always: "Copied..." coming must not push the rows below it.
    match (&st.error, st.info.is_empty()) {
        (Some(e), _) => crate::layout::status_line(ui, e, Some(egui::Color32::LIGHT_RED)),
        (None, false) => crate::layout::status_line(ui, &st.info, None),
        _ => crate::layout::status_line(ui, "", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::project::new_map;

    #[test]
    fn the_form_draws_for_a_map_with_and_without_lighting_and_changes_nothing() {
        let dir = std::env::temp_dir().join(format!("ope-settings-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut project = Project::open(&dir, None).unwrap();
        new_map(&mut project, "m").unwrap();
        let (ctx, mut st) = (egui::Context::default(), State::default());
        let skies = vec!["skybox_05".to_string()];
        for lit in [false, true] {
            if lit {
                set_lighting_enabled(&mut project, "m", true).unwrap();
            }
            let before = project.undo_label().map(str::to_string);
            ctx.run_ui(Default::default(), |ui| show(ui, &mut st, &mut project, "m", &skies)).textures_delta.clear();
            assert_eq!(project.undo_label().map(str::to_string), before, "drawing is not an edit");
            assert!(st.error.is_none());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_hover_texts_are_the_engines_schema_descriptions() {
        for ptr in ["/name", "/description", "/world_half", "/sky", "/backdrop", "/lighting", "/lighting/roof", "/lighting/floor", "/lighting/roof_direction"] {
            assert!(!hint(ptr).is_empty(), "{ptr}");
        }
        assert!(hint("/world_half").contains("1000"), "the schema's own words: {}", hint("/world_half"));
        assert!(hint("/no_such_key").is_empty());
    }

    #[test]
    fn pictures_are_checked_by_name_and_signature() {
        let dir = std::env::temp_dir().join(format!("ope-settings-pic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (ok, short, wrong) = (dir.join("a.PNG"), dir.join("b.png"), dir.join("c.jpg"));
        std::fs::write(&ok, PNG_MAGIC).unwrap();
        std::fs::write(&short, [0x89, b'P']).unwrap();
        std::fs::write(&wrong, PNG_MAGIC).unwrap();
        assert!(check_picture(&ok).is_ok());
        assert!(check_picture(&short).is_err() && check_picture(&wrong).is_err() && check_picture(&dir.join("none.png")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
