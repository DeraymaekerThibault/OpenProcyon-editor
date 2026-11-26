//! T6: the map settings edits on synthetic maps (no game content): each is an undo step, a retail map saves as a diff the engine's
//! overlay agrees with, the backdrop copy goes through the guard, and refusals change nothing.

use openprocyon::authoring::read_merged;
use openprocyon_editor::map::doc::{self, LightColour};
use openprocyon_editor::map::project::{merged_view, new_map, open_map};
use openprocyon_editor::map::settings::{self, Text};
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn read(root: &Path, rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root.join(rel)).unwrap()).unwrap()
}

/// (root, import, mod): the import has retail map `red` with a sky and its own lighting, and `plain` with neither.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("ope-map-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(
        &assets,
        "maps/red/map.json",
        &json!({"name": "Red", "description": "Old", "world_half": 1500.0, "sky": "skybox_05_Layer1", "backdrop": "Tex",
            "lighting": {"roof": [1, 0.9, 0.8, 1], "floor": [0.1, 0.1, 0.2], "ambient": [0.2, 0.2, 0.2, 1], "roof_direction": [0, 0, -1]},
            "objects": [{"type": "Isle_A", "id": 1, "pos": [0, 0, 0]}]}),
    );
    put(&assets, "maps/plain/map.json", &json!({"name": "Plain"}));
    std::fs::create_dir_all(&m).unwrap();
    (root, assets, m)
}

fn png(dir: &Path) -> PathBuf {
    let f = dir.join("pic.png");
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"synthetic, not a real picture");
    std::fs::write(&f, bytes).unwrap();
    f
}

#[test]
fn text_and_numbers_save_as_a_diff_over_the_retail_map_and_undo() {
    let (_root, assets, m) = setup("diff");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    settings::set_text(&mut p, "red", Text::Name, "R", true).unwrap();
    settings::set_text(&mut p, "red", Text::Name, "Ruby", true).unwrap();
    settings::set_text(&mut p, "red", Text::Description, "New", false).unwrap();
    settings::set_world_half(&mut p, "red", 2000.0, false).unwrap();
    settings::set_sky(&mut p, "red", Some("skybox_09")).unwrap();
    let v = merged_view(&p, "red").unwrap();
    assert_eq!((doc::name(v), doc::description(v), doc::world_half(v), doc::sky(v)), (Some("Ruby"), Some("New"), Some(2000.0), Some("skybox_09")));
    assert!(p.save().errors.is_empty());
    let file = read(&m, "maps/red/map.json");
    assert_eq!(file["extends"], "red");
    assert_eq!(
        (file["name"].as_str(), file["description"].as_str(), file["world_half"].as_f64(), file["sky"].as_str()),
        (Some("Ruby"), Some("New"), Some(2000.0), Some("skybox_09"))
    );
    assert!(file.get("objects").is_none() && file.get("lighting").is_none(), "only what changed is in the file");
    let merged = read_merged(&p.root, "maps/red/map.json").unwrap();
    assert_eq!(&merged, merged_view(&p, "red").unwrap());

    // typing in the name box was one step: undo of the four steps goes sky, world_half, description, name
    assert_eq!(p.undo().as_deref(), Some("Set sky"));
    assert_eq!(p.undo().as_deref(), Some("Set world_half"));
    assert_eq!(p.undo().as_deref(), Some("Set map description"));
    assert_eq!(p.undo().as_deref(), Some("Set map name"));
    assert_eq!(p.undo(), None);
    assert_eq!(merged_view(&p, "red").unwrap()["name"], "Red");
}

#[test]
fn a_sky_with_the_same_name_keeps_the_retail_layer_suffix_and_default_removes_the_key() {
    let (_root, assets, m) = setup("sky");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    settings::set_sky(&mut p, "red", Some("skybox_05")).unwrap();
    assert_eq!(doc::sky(merged_view(&p, "red").unwrap()), Some("skybox_05_Layer1"));
    assert_eq!(p.undo_label(), None, "no step for the same sky");
    settings::set_sky(&mut p, "red", None).unwrap();
    assert_eq!(doc::sky(merged_view(&p, "red").unwrap()), Some(doc::DEFAULT_SKY), "a mod cannot remove the retail key: the default is written out");
    assert!(p.save().errors.is_empty());
    let merged = read_merged(&p.root, "maps/red/map.json").unwrap();
    assert_eq!(&merged, merged_view(&p, "red").unwrap());
}

#[test]
fn world_half_refuses_what_the_engine_could_not_use() {
    let (_root, assets, m) = setup("half");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    for bad in [0.0, -5.0, f64::NAN, f64::INFINITY] {
        assert!(settings::set_world_half(&mut p, "red", bad, false).is_err(), "{bad}");
    }
    assert_eq!(p.undo_label(), None);
    assert!(p.dirty_rels().is_empty());
}

#[test]
fn lighting_turns_on_with_starting_values_edits_by_part_and_turns_off() {
    let (_root, assets, m) = setup("light");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    // `plain` has none: edits are refused until it is turned on
    assert!(settings::set_light_colour(&mut p, "plain", LightColour::Roof, [1.0; 4], false).is_err());
    assert!(settings::set_light_direction(&mut p, "plain", [0.0, 0.0, -1.0], false).is_err());
    settings::set_lighting_enabled(&mut p, "plain", true).unwrap();
    settings::set_lighting_enabled(&mut p, "plain", true).unwrap();
    settings::set_light_colour(&mut p, "plain", LightColour::Roof, [1.0, 0.5, 2.0, 1.0], false).unwrap();
    settings::set_light_direction(&mut p, "plain", [0.0, 1.0, -1.0], false).unwrap();
    let v = merged_view(&p, "plain").unwrap();
    assert_eq!(doc::light_colour(v, LightColour::Roof), Some([1.0, 0.5, 1.0, 1.0]), "clamped to 0..1");
    assert_eq!(doc::light_direction(v), Some([0.0, 1.0, -1.0]));
    assert!(settings::set_light_direction(&mut p, "plain", [0.0; 3], false).is_err(), "a zero direction cannot be normalised");
    // the retail map's own lighting: three-number colours read with alpha 1
    open_map(&mut p, "red").unwrap();
    assert_eq!(doc::light_colour(merged_view(&p, "red").unwrap(), LightColour::Floor), Some([0.1, 0.1, 0.2, 1.0]));
    settings::set_light_colour(&mut p, "red", LightColour::Floor, [0.0, 0.0, 0.0, 1.0], false).unwrap();
    assert!(settings::set_lighting_enabled(&mut p, "red", false).is_err(), "a retail lighting cannot be removed by a mod");
    assert_eq!(doc::light_colour(merged_view(&p, "red").unwrap(), LightColour::Floor), Some([0.0, 0.0, 0.0, 1.0]));
    settings::set_lighting_enabled(&mut p, "plain", false).unwrap();
    settings::set_lighting_enabled(&mut p, "plain", true).unwrap();
    assert!(p.save().errors.is_empty());
    for id in ["red", "plain"] {
        let rel = format!("maps/{id}/map.json");
        assert_eq!(&read_merged(&p.root, &rel).unwrap(), merged_view(&p, id).unwrap(), "{id}");
    }
}

#[test]
fn the_backdrop_picture_is_copied_into_the_mod_through_the_guard() {
    let (root, assets, m) = setup("backdrop");
    let mut p = Project::open(&m, Some(assets.clone())).unwrap();
    let pic = png(&root);
    settings::set_backdrop(&mut p, "plain", &pic).unwrap();
    assert_eq!(std::fs::read(m.join("maps/plain/backdrop.png")).unwrap(), std::fs::read(&pic).unwrap());
    assert!(doc::has_backdrop(merged_view(&p, "plain").unwrap()));
    assert!(!assets.join("maps/plain/backdrop.png").exists(), "never the import");
    assert!(p.save().errors.is_empty());
    assert_eq!(read(&m, "maps/plain/map.json")["backdrop"], "plain");
    settings::clear_backdrop(&mut p, "plain").unwrap();
    assert!(!doc::has_backdrop(merged_view(&p, "plain").unwrap()));
    settings::clear_backdrop(&mut p, "plain").unwrap();
    assert_eq!(p.undo().as_deref(), Some("Remove backdrop"), "the second clear was no step");
    assert!(doc::has_backdrop(merged_view(&p, "plain").unwrap()));
    assert_eq!(p.undo().as_deref(), Some("Set backdrop"));
    assert!(!doc::has_backdrop(merged_view(&p, "plain").unwrap()));
}

#[test]
fn a_backdrop_that_is_not_a_png_is_refused_and_writes_nothing() {
    let (root, assets, m) = setup("notpng");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    let txt = root.join("pic.png.txt");
    std::fs::write(&txt, "x").unwrap();
    let fake = root.join("fake.png");
    std::fs::write(&fake, "not a picture at all").unwrap();
    for f in [txt, fake, root.join("missing.png")] {
        assert!(settings::set_backdrop(&mut p, "plain", &f).is_err(), "{}", f.display());
    }
    assert!(!m.join("maps").exists());
    assert!(p.dirty_rels().is_empty());
}

#[test]
fn a_new_map_takes_every_setting_and_is_written_whole() {
    let (_root, assets, m) = setup("new");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    new_map(&mut p, "mine").unwrap();
    settings::set_text(&mut p, "mine", Text::Description, "A test", false).unwrap();
    settings::set_world_half(&mut p, "mine", 800.0, false).unwrap();
    settings::set_sky(&mut p, "mine", Some("skybox_07")).unwrap();
    settings::set_lighting_enabled(&mut p, "mine", true).unwrap();
    assert!(p.save().errors.is_empty());
    let f = read(&m, "maps/mine/map.json");
    assert_eq!((f["name"].as_str(), f["description"].as_str(), f["world_half"].as_f64(), f["sky"].as_str()), (Some("mine"), Some("A test"), Some(800.0), Some("skybox_07")));
    assert!(f["lighting"].is_object() && f.get("extends").is_none());
}

#[test]
fn a_retail_backdrop_is_turned_off_with_false_since_a_mod_cannot_remove_a_key() {
    let (_root, assets, m) = setup("retail-backdrop");
    let mut p = Project::open(&m, Some(assets)).unwrap();
    open_map(&mut p, "red").unwrap();
    assert!(doc::has_backdrop(merged_view(&p, "red").unwrap()));
    settings::clear_backdrop(&mut p, "red").unwrap();
    assert!(!doc::has_backdrop(merged_view(&p, "red").unwrap()));
    assert!(p.save().errors.is_empty());
    assert_eq!(read(&m, "maps/red/map.json")["backdrop"], false);
    assert_eq!(&read_merged(&p.root, "maps/red/map.json").unwrap(), merged_view(&p, "red").unwrap());
}

// ---- the sun direction's boxes (round 4, step 2) ----

use bevy_egui::egui::{self, Pos2, Rect};

/// One `direction_box` driven by real pointer and key events: its value, its rectangle, and whether it ever said "moved".
struct Box {
    ctx: egui::Context,
    value: f64,
    rect: Rect,
    moved: bool,
    t: f64,
}

impl Box {
    fn new(value: f64) -> Box {
        let mut b = Box { ctx: egui::Context::default(), value, rect: Rect::NOTHING, moved: false, t: 0.0 };
        b.frame(vec![]);
        b.frame(vec![]);
        b
    }
    fn frame(&mut self, mut events: Vec<egui::Event>) {
        self.t += 0.05;
        events.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_max(Pos2::ZERO, Pos2::new(600.0, 300.0))), time: Some(self.t), events, ..Default::default() };
        let (value, rect, moved) = (&mut self.value, &mut self.rect, &mut self.moved);
        let mut full = self.ctx.run_ui(raw, |ui| {
            let (r, m) = settings::direction_box(ui, value, "x");
            *rect = r.rect;
            *moved |= m;
        });
        full.textures_delta.clear();
    }
    fn click(&mut self, at: Pos2) {
        let b = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        self.frame(vec![egui::Event::PointerMoved(at)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.frame(vec![]);
        self.frame(vec![]);
    }
}

#[test]
fn a_click_into_the_sun_box_and_out_again_writes_nothing() {
    let mut b = Box::new(0.123456789);
    let inside = b.rect.center();
    b.click(inside); // into the box: it becomes a text field
    assert!(b.ctx.memory(|m| m.focused()).is_some(), "the click put the box in edit mode");
    b.click(Pos2::new(400.0, 250.0)); // out again, nothing typed
    assert!(b.ctx.memory(|m| m.focused()).is_none());
    assert!(!b.moved, "focus loss alone is not a change");
    assert_eq!(b.value, 0.123456789, "the value is the one the file holds, not the shown text");
}

#[test]
fn a_typed_sun_value_is_applied_on_enter() {
    let mut b = Box::new(0.123456789);
    let inside = b.rect.center();
    b.click(inside);
    b.frame(vec![egui::Event::Text("0.5".into())]);
    b.frame(vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }]);
    b.frame(vec![]);
    b.frame(vec![]);
    assert!(b.moved);
    assert_eq!(b.value, 0.5);
}
