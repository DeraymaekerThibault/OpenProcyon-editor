//! The engine's `--check-mod` findings for repeated world object ids and disagreeing nebulae (OpenProcyon#647) reach the problems list
//! like any other finding: as warnings of the mod's own files, counted, not hidden as inherited. Synthetic mod, no assets.
use openprocyon::authoring::{Level, check_mod};
use openprocyon_editor::project::problems;
use serde_json::{Value, json};

fn put(root: &std::path::Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

#[test]
fn duplicate_ids_and_nebula_disagreement_are_own_warnings_in_the_problems_list() {
    let dir = std::env::temp_dir().join(format!("ope-modcheck-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    put(&dir, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    let tri = json!([[0, 0], [10, 0], [10, 10]]);
    put(&dir, "maps/storm/map.json", &json!({"ships": [{"id": 1, "type": "S"}], "objects": [{"id": 1, "type": "Rock"}], "nebulae": [{"name": "Cloud", "polygon": tri}]}));
    put(
        &dir,
        "maps/storm/mission.json",
        &json!({"players": ["P"], "ships": [], "rules": [], "volumes": {"Cloud": [[0, 0], [10, 0], [10, 12]]},
                "environment": {"nebulae": [{"New Nebula Name": "Cloud", "Polygon Name": "Cloud"}]}}),
    );
    put(&dir, "maps/storm/media.json", &json!({}));
    let found = problems::sorted(check_mod(&dir, None));
    let own = problems::own(&found);
    for (file, needle) in [("maps/storm/map.json", "id 1 is also used by"), ("maps/storm/mission.json", "differs from the polygon of nebula 'Cloud'")] {
        let f = own.iter().find(|f| f.file == file && f.message.contains(needle)).unwrap_or_else(|| panic!("no own finding {file} {needle:?} in {found:#?}"));
        assert_eq!(f.level, Level::Warning);
        assert!(!f.inherited);
    }
    let (errors, warnings) = problems::counts(&found);
    assert!(warnings >= 2, "{errors} errors, {warnings} warnings: {found:#?}");
    let _ = std::fs::remove_dir_all(&dir);
}
