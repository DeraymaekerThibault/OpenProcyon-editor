//! The map catalogue scan on a synthetic overlay folder built here (empty stand-in files, no game content).

use openprocyon_editor::map::catalogue::{Catalogue, scan};
use serde_json::json;
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, data: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ope-catalogue-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn synthetic(root: &Path) {
    put(
        root,
        "maps/alpha/map.json",
        &json!({"objects": [
            {"type": "Isle_A", "scene": "Isle_A_Model", "id": 1, "pos": [0, 0, 0]},
            {"type": "Isle_A", "scene": "Isle_A_Model", "id": 2, "pos": [5, 0, 0]},
            {"type": "Rock_B", "scene": "Rock_B_Model", "id": 3},
            {"scene": "NoType"}
        ]})
        .to_string(),
    );
    put(root, "maps/beta/map.json", &json!({"objects": [{"type": "Rock_B", "scene": "Rock_B_Alt", "id": 1}, {"type": "Base_C", "id": 2}]}).to_string());
    put(root, "maps/gamma/map.json", "{ not json");
    put(root, "maps/delta/notes.txt", "no map here");
    put(root, "data/collision.json", &json!({"Isle_A": {}, "Other": {}}).to_string());
    put(root, "data/docks.json", &json!({"Base_C": {"points": []}, "Isle_A": {"points": []}}).to_string());
    for f in ["models/scenery/Isle_A_Model.glb", "models/deep/scenery/Rock_B_Model.GLB", "models/scenery/readme.txt", "models/ships/Ship_X.glb"] {
        put(root, f, "");
    }
    for f in ["models/sky/skybox_05_Layer1.glb", "models/sky/skybox_05_Layer2.glb", "models/sky/skybox_07.glb", "models/sky/own_cube.png", "models/sky/other.png"] {
        put(root, f, "");
    }
    put(root, "data/render/skies.json", &json!({"mine_Layer3": {}, "skybox_07": {}}).to_string());
}

#[test]
fn scenery_pairs_flags_and_counts() {
    let root = folder("scenery");
    synthetic(&root);
    let c = scan(&root);
    let names: Vec<&str> = c.scenery.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Base_C", "Isle_A", "Rock_B"], "sorted, objects without a type skipped");

    let isle = c.find("Isle_A").unwrap();
    assert_eq!((isle.scenes.as_slice(), isle.scene(), isle.uses), (["Isle_A_Model".to_string()].as_slice(), Some("Isle_A_Model"), 2));
    assert!(isle.has_collision && isle.has_docks);

    let rock = c.find("Rock_B").unwrap();
    assert_eq!(rock.scenes, ["Rock_B_Alt", "Rock_B_Model"], "one type, two scenes across maps");
    assert_eq!(rock.uses, 2);
    assert!(!rock.has_collision && !rock.has_docks);

    let base = c.find("Base_C").unwrap();
    assert_eq!((base.scene(), base.has_collision, base.has_docks), (None, false, true));
    assert!(c.find("Other").is_none(), "a collision key no map uses is not scenery here");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn scene_files_skies_and_problems() {
    let root = folder("files");
    synthetic(&root);
    let c = scan(&root);
    assert_eq!(c.scene_files, ["Isle_A_Model", "Rock_B_Model"], "only .glb under a scenery folder, extension case ignored");
    assert_eq!(c.skies, ["mine", "own", "skybox_05", "skybox_07"], "layers merged, cube PNGs and skies.json keys included, other PNGs not");
    assert_eq!(c.problems.len(), 1);
    assert!(c.problems[0].starts_with("maps/gamma/map.json"), "{:?}", c.problems);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_empty_or_missing_folder_gives_an_empty_catalogue() {
    let root = folder("empty");
    assert_eq!(scan(&root), Catalogue::default());
    assert_eq!(scan(&root.join("does-not-exist")), Catalogue::default());
    let _ = std::fs::remove_dir_all(&root);
}
