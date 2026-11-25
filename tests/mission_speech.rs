//! Speech, messages and ship names of `mission::speech` on synthetic missions (no game content; the Ogg bytes are built here, no
//! `.ogg` is committed): entries, the rename that follows the rules in one step, the sound copy through the guard and the duration
//! read from the file.

use openprocyon_editor::mission::doc::{self as mdoc, MediaTable};
use openprocyon_editor::mission::project::{media_rel, mission_rel, new_mission, open_mission};
use openprocyon_editor::mission::rules::{Target, append_rule};
use openprocyon_editor::mission::speech;
use openprocyon_editor::project::Project;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, rel: &str, v: &Value) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(v).unwrap()).unwrap();
}

fn setup(name: &str) -> (PathBuf, Project) {
    let root = std::env::temp_dir().join(format!("ope-mission-speech-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (assets, m) = (root.join("assets"), root.join("mod"));
    put(&assets, "maps/mission01/map.json", &json!({"name": "First"}));
    put(&assets, "maps/mission01/mission.json", &json!({"tasks": [], "rules": [], "ships": []}));
    put(&assets, "maps/mission01/media.json", &json!({"speech": {"HINT": {"text": "Hold"}}, "messages": {}, "ship_names": {}}));
    put(&assets, "hud/sprites/TalkingHead_Character_Amelia_Happy/x.txt", &json!(0));
    put(&m, "hud/sprites/TalkingHead_Character_Mine/x.txt", &json!(0));
    put(&m, "mod.json", &json!({"id": "t", "name": "T", "format": 1}));
    (root, Project::open(&m, Some(assets)).unwrap())
}

fn media<'a>(p: &'a Project, id: &str) -> &'a Value {
    &p.doc(&media_rel(id).unwrap()).unwrap().edited
}

/// A minimal Ogg Vorbis file: the identification page and one page that ends at `samples`.
fn ogg(rate: u32, samples: u64) -> Vec<u8> {
    fn page(header_type: u8, granule: u64, seq: u32, body: &[u8]) -> Vec<u8> {
        let mut out = b"OggS\0".to_vec();
        out.push(header_type);
        out.extend(granule.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(seq.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.push(1);
        out.push(body.len() as u8);
        out.extend(body);
        out
    }
    let mut id = vec![1];
    id.extend(b"vorbis");
    id.extend(0u32.to_le_bytes());
    id.push(1);
    id.extend(rate.to_le_bytes());
    id.extend([0; 12]);
    let mut f = page(0x02, 0, 0, &id);
    f.extend(page(0x04, samples, 1, &[0; 40]));
    f
}

#[test]
fn a_new_mission_gets_its_tables_one_entry_at_a_time() {
    let (root, mut p) = setup("tables");
    new_mission(&mut p, "own", None).unwrap();
    speech::add_entry(&mut p, "own", MediaTable::Speech, "HELLO").unwrap();
    speech::set_text(&mut p, "own", "HELLO", "Ahoy").unwrap();
    speech::set_face(&mut p, "own", "HELLO", "TalkingHead_Character_Mine").unwrap();
    speech::set_file(&mut p, "own", "HELLO", "own_hello").unwrap();
    speech::set_duration(&mut p, "own", "HELLO", Some(2.5)).unwrap();
    assert!(speech::set_duration(&mut p, "own", "HELLO", Some(-1.0)).is_err());
    speech::add_entry(&mut p, "own", MediaTable::Messages, "WIN").unwrap();
    speech::set_field(&mut p, "own", MediaTable::Messages, "WIN", None, Some(json!("Well fought."))).unwrap();
    speech::add_entry(&mut p, "own", MediaTable::ShipNames, "N1").unwrap();
    assert!(speech::add_entry(&mut p, "own", MediaTable::Speech, "HELLO").is_err());
    assert!(speech::set_text(&mut p, "own", "NOPE", "x").is_err());
    let m = media(&p, "own");
    let e = &m["speech"]["HELLO"];
    assert_eq!(
        (mdoc::speech_text(e), mdoc::speech_file(e), mdoc::speech_face(e), mdoc::speech_duration(e)),
        (Some("Ahoy"), Some("own_hello"), Some("TalkingHead_Character_Mine"), Some(2.5))
    );
    assert_eq!(m["messages"]["WIN"], "Well fought.");
    speech::set_file(&mut p, "own", "HELLO", "").unwrap();
    speech::set_duration(&mut p, "own", "HELLO", None).unwrap();
    speech::remove_entry(&mut p, "own", MediaTable::ShipNames, "N1").unwrap();
    assert!(media(&p, "own")["ship_names"].as_object().unwrap().is_empty());
    assert!(media(&p, "own")["speech"]["HELLO"].get("file").is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn renaming_an_entry_follows_the_rules_in_one_step() {
    let (root, mut p) = setup("rename");
    new_mission(&mut p, "own", None).unwrap();
    speech::add_entry(&mut p, "own", MediaTable::Speech, "HINT").unwrap();
    speech::add_entry(&mut p, "own", MediaTable::Messages, "WIN").unwrap();
    let rel = mission_rel("own").unwrap();
    let rule = json!({"name": "R", "conditions": [], "actions": [
        {"Type": "Play speech event", "Speech Event Name": "HINT"},
        {"Type": "End Game", "Loser - Custom Message String ID": "WIN"}]});
    append_rule(&mut p, &Target::mission(&rel), rule, "Add rule").unwrap();
    p.do_step(&rel, "victory", vec![openprocyon_editor::edit::Edit::set("/victory_message", json!("WIN"))]).unwrap();
    let (m0, s0) = (media(&p, "own").clone(), p.doc(&rel).unwrap().edited.clone());
    let n = speech::rename_entry(&mut p, "own", MediaTable::Speech, "HINT", "TIP").unwrap();
    assert_eq!(n, 1);
    assert!(media(&p, "own")["speech"].get("HINT").is_none() && media(&p, "own")["speech"].get("TIP").is_some());
    assert!(serde_json::to_string(&p.doc(&rel).unwrap().edited).unwrap().contains("\"TIP\""));
    speech::rename_entry(&mut p, "own", MediaTable::Messages, "WIN", "VICTORY").unwrap();
    assert_eq!(p.doc(&rel).unwrap().edited["victory_message"], "VICTORY");
    assert!(speech::rename_entry(&mut p, "own", MediaTable::Speech, "TIP", "TIP").map(|n| n == 0).unwrap());
    assert!(speech::rename_entry(&mut p, "own", MediaTable::Speech, "TIP", " ").is_err());
    assert!(speech::rename_entry(&mut p, "own", MediaTable::Speech, "GONE", "X").is_err());
    p.undo().unwrap();
    p.undo().unwrap();
    assert_eq!((media(&p, "own"), &p.doc(&rel).unwrap().edited), (&m0, &s0));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_sound_is_copied_through_the_guard_and_its_length_read() {
    let (root, mut p) = setup("sound");
    new_mission(&mut p, "own", None).unwrap();
    speech::add_entry(&mut p, "own", MediaTable::Speech, "Gate Open").unwrap();
    let src = root.join("mine.ogg");
    std::fs::write(&src, ogg(44100, 110_250)).unwrap();
    assert_eq!(speech::suggest_file("own", "Gate Open"), "own_gate_open");
    let rel = speech::import_sound(&mut p, "own", "Gate Open", &src, "own_gate_open").unwrap();
    assert_eq!(rel, "maps/own/speech/own_gate_open.ogg");
    assert_eq!(std::fs::read(root.join("mod").join(&rel)).unwrap(), ogg(44100, 110_250));
    assert_eq!(media(&p, "own")["speech"]["Gate Open"]["file"], "own_gate_open");
    assert_eq!(speech::read_duration(&mut p, "own", "Gate Open").unwrap(), 2.5);
    assert_eq!(media(&p, "own")["speech"]["Gate Open"]["duration"], json!(2.5));
    // refusals: an existing file, a bad name, not an .ogg, not Ogg bytes, an unknown event
    assert!(speech::import_sound(&mut p, "own", "Gate Open", &src, "own_gate_open").unwrap_err().contains("exists already"));
    assert!(speech::import_sound(&mut p, "own", "Gate Open", &src, "../x").is_err());
    let wav = root.join("x.wav");
    std::fs::write(&wav, b"RIFF").unwrap();
    assert!(speech::import_sound(&mut p, "own", "Gate Open", &wav, "a").unwrap_err().contains(".ogg"));
    let fake = root.join("fake.ogg");
    std::fs::write(&fake, b"RIFF").unwrap();
    assert!(speech::import_sound(&mut p, "own", "Gate Open", &fake, "b").unwrap_err().contains("not an Ogg"));
    assert!(speech::import_sound(&mut p, "own", "NOPE", &src, "c").is_err());
    assert!(speech::read_duration(&mut p, "own", "NOPE").is_err());
    // the guard keeps the copy out of the import
    assert!(!root.join("assets/maps/own").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn portraits_come_from_the_mod_and_the_import() {
    let (root, p) = setup("faces");
    assert_eq!(speech::faces(&p), ["TalkingHead_Character_Amelia_Happy", "TalkingHead_Character_Mine"]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_retail_media_adds_and_changes_entries_but_cannot_remove_or_rename() {
    let (root, mut p) = setup("retail");
    open_mission(&mut p, "mission01").unwrap();
    speech::add_entry(&mut p, "mission01", MediaTable::Speech, "NEW").unwrap();
    speech::set_text(&mut p, "mission01", "HINT", "Hold the line").unwrap();
    assert!(speech::remove_entry(&mut p, "mission01", MediaTable::Speech, "HINT").unwrap_err().contains("extends"));
    assert!(speech::rename_entry(&mut p, "mission01", MediaTable::Speech, "HINT", "TIP").is_err());
    let report = p.save();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let saved: Value = serde_json::from_slice(&std::fs::read(root.join("mod/maps/mission01/media.json")).unwrap()).unwrap();
    assert_eq!(saved["speech"]["HINT"]["text"], "Hold the line");
    assert!(saved["speech"].get("NEW").is_some());
    let _ = std::fs::remove_dir_all(root);
}
