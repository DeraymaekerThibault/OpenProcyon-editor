//! Speech, messages and ship names of `mission::speech` on synthetic missions (no game content; a synthetic 0.5 s tone is embedded as
//! hex, no `.ogg` is committed): entries, the rename that follows the rules in one step, the sound copy through the guard and the duration
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

/// A synthetic test tone, made for this test and nothing else: 0.5 s of a 440 Hz sine, mono, 22050 Hz, Ogg Vorbis (libvorbis,
/// lowest quality; 4 kB). It is our own signal, not a game file.
const TONE: &str = "\
4f676753000200000000000000000000000000000000e57011ec011e01766f72626973000000000122560000000000001e9d000000000000a9014f67\
6753000000000000000000000000000001000000f1ec78a50e30ffffffffffffffffffffffffc503766f726269730600000066666d70656701000000\
16000000656e636f6465723d4c617663206c6962766f726269730105766f726269732242435601004000001842102a05ad638e3ac815218c19a2a042\
ca29c71d42d021a32443883ac635c7186347b9648a42c981d090550000400000a41c575072492de79c73a31857cc71e820e79c73e52067cc710925e7\
9c738e39e792728e31e79c73a318570e72292de79c738114478a71a718e79c73a41c478a71a818e79c736d31b79272ce39e79c73e620875272ae35e7\
9c73a418670e720b25e79c73c62067cc71eb20e79c738c35b7d472ce39e79c73ce39e79c73ce39e79c738c31e79c73ce39e79c736e31e71673ae39e7\
9c73ce39e71c73ce39e79c732034641500900000a0a1288ae2280e101ab20a00c80000104071144791144bb11ccbd1240d080d590500000100080000\
a04886a4488aa5588e66699e267aa2289aa22aabb269cab22ccbb2ebba2e101ab20a00480000505114c5701407080d5905006400000860288aa3388e\
e45892a5599e078486ac0200800000040000500c47b1144df124cff23ccff33ccff33ccff33ccff33ccff33ccff33c0d080d59050020000000822864\
1803424356010040000008211a19439d52125c0a16421c11431d42ce43a9a583e029852563d253ac4108217cef3df7de7bef81d09055000010000061\
14388881c7240821846214274471a620082184e52458ca79e82408dd831042b89c7bcbb9f7de7b2034641500000800c02084104208218410420829a4\
94524829a698628a29c71c73cc31c720830c32e8a0934e3ac9a4924e3aca24a38e526b29b514534cb1e51663adb5d69c73af4129638c31c618638c31\
c618638c31c61823080d59050080000010061964904108218414524829a69872cc31c71c034243560100800000020000001c455224477224479224c9\
922c49933ccbb33ccbb33c4dd4444d1555d5556dd7f66d5ff66ddfd565dff665dbd5655d9665ddb56d5dd65d5dd7755dd7755dd7755dd7755dd7755d\
d781d0905500800400808ee4388ee4388ee4488ea4480a101ab20a00900100100080a3388ae3488ee4588e255992266996677996a7799aa8891e101a\
b20a0000040010000000000080a2288aa3388e245996a6699ea77aa2289aaaaa8aa6a9aaaa6a9aa6699aa6699aa6699aa6699aa6699aa6699aa6699a\
a6699aa6699aa6699aa66902a121ab00000900001dc7711c47711cc77124479224203464150020030020000043511c45722cc792344bb33ccbd344cf\
f45c51367553576d203464150000080020000000000000c7733cc7733cc9933ccb733cc7933c49d3344dd3344dd3344dd3344dd3344dd3344dd3344d\
d3344dd3344dd3344dd3344dd3344dd3344dd3344d0342435602006400001093904a4eb1574629c624b45e2aa41493d47ba898624c3aeda942062907\
b9874a21a5a0d3de32a590520c7ba79842c818eaa183903185b0d7da73cfbdf71e080d59110044010000c620c61063c8312625831231c724645222e7\
9c944e4a26a5a4565acca484984a8b91734e4a272593525a0ba965924a6b25a6020000021c0000022c84424356040051000088314829a414524a31a7\
98434a29c794634829e59c724e39c79874102ae718740e4aa494728e39a79c7312320795730e42269d00008000070080000ba1d0901501409c000080\
90738a3108116310420929855052aa9c93d24149a983925249a9c592528c9573523a09297512522a29c558528a2da45463692dd7d2528d2dc69c5b8c\
bd8694622da9d55a5aabb9c558738b35f7c8394a9d94d63a29ada5d66a4dadd5da49692da4d662692dc6d662cd29c69c3329ad8596622ba9c5d862cb\
35b59873692dd71463cf29c69e6bacb9c79c8330add59c5acb39c5987bccb1e798730f9273943a29ad75525a4badd59a5aab3593d25a69adc6905a8b\
2dc69c5b8b3167525a2ca9c5585a8a31c598738b2dd7d05aae29c69c538b39c75a8392b1f65e5aab39c5987b8aade798733036c79e3b4ab996d67a2e\
adf55e732e42d6dc8b682de7d46a0f2ac69e73cec1d8dc8310ade59c6aec3dc5d87bee3918db73f0add6e05bcd45c89c83d0b9f8a67b3046d5da83cc\
b5089973103ae82274f0c978946a2eade55c5aeb3dd61a7ccd3908d15aee29c6de538bbdd79e9bb0bd07215acb3dc5d8838a31f89a73303ae76254ad\
c1c79c8390b516a17b2f4ae72094aab507996b5032d72274f0c5e8a08b2f000060c0010020c084325068c88a00204e008041c839a518844a2908a184\
94422829558c49c89883923127a594525a0825b58a310899635232c7a484125a2a25b4124a69a994d25a28a5b5965a8c29b5164329a985525a2ba5b4\
965aaa31b55663c49894cc39299963524a29ad95525aab1c93923128a983904a2929c552528b95735232e8a874104a2aa9c4545269ada4d25229a5c5\
92526c29c5545b8bb586525a2ca9c456526a31b5545b8b31d788312919735232e7a49452522ba5b4963927a5838e4ae6a0a492526ba5a41433e6a474\
0e4aca20a352528a2da5125328a5b592526ca5a4d65a8cb5a6d45a2d25b556526ab194125b8b31d7164b4d9d94d64a2a3186525a6b31e69a5a8b3194\
125b2929c6924a6cadc59a5b6c3986525a2ca9c4564a6ab1d596636bb1e6d4528d29b59a5b6cb9c694538fb5f69c5aab35b554636bb1e6585b6fb5d6\
9c3b29ad85525a2b25c5985a8bb1c558732825b692526ca5a4185b6cb9b616630fa1b4584a6ab1a412636b31e6185b8ea9b55a5b6cb9a6d462adb5f6\
1c5b6e3da5166b8bb1e6d2528d35d7de634d3915000030e000001060421928346425001005000018c31863101aa59c734e4a839473ce49c99c831042\
4a997310424829734e424a2d65ce4148a9b5504a4aadc5164a49a9b5160b00002870000008b04153627180424356020051000088314a3106a1314629\
e72034c628c518844a29c69c935029c598735032c79c83504ae69c73104a092194524a4a2184524a49a900008002070080001b342516072834644500\
1005000018639c33ce210a9da5ce5224a9a3d6516b28a51a4b8c9dc6567bebb9d31a7b6db937944a8da9d68e6bcbb9d5de694d3db71c0b00003b7000\
003bb0100a0d590900e4010010c628c59873ce19851873ce39e70c528c39e79c738a31e79c831042c59873ce41082173ce3908a1849239e71c84104a\
e89c83504a29a574ce4108a194523ae72084524a299d73104a29a5940200800a1c0000026c14d99c6024a8d0909500401e0000600c42ce4969ad61cc\
39082dd5d830c61c9494628b9c8390528bb946cc414829c6a03b2829b5186cf09d84945a8b390793528b35e7de8348a9b59a83ce3dd55673cfbdf79c\
62ac35e7de732f000077c10100ecc04691cd0946820a0d590900e40100100829c59873ce19a51873cc39e78c528c31e69c738a31c69c73ce41c51863\
ce39072163cc39e720849031e69c731042e89c730e42082174ce39072184103ae7a0831042089d7310420821840200800a1c0000026c14d99c6024a8\
d09095004038000000218410420821841042082184104208218410420821841042082184104208218410420821841042082184104208218410420821\
841042082184104208218410420821841042082184104208218410420821841042082184104208218410420821841042082184104208218410420821\
8410420821841042e89c73ce39e79c73ce39e79c73ce39e79c73ce392700c8b7c201c0ffc1c6195692ce0a47830b0d590900840300000a4128a56210\
4a2925924e3a299d93504a299183524ae9a494524a09a594524a08a594524a081d94524229a594524a29a594524a29a594523a29a594524a29a594ca\
3929a593524a29a544ce49292194524a29a584524a29a594524a29a594524a29a594524a29a584104208218410420821841042082184104208218410\
42082184104208218410420821840200b81b1c0020126c9c6125e9ac7034b8d090950040480000a014738e4a0829949052a898a28e422929a4524a0a\
1163ce49ea1c85505228a983ca3908a5a494422a2175ce4107258590520921958e3ae8289450522a2594d23928a584144a4a2995904248a974945228\
259594422a2195524a482595104a0a9da4544a0aa9a45452089d7490422725a4924a0aa99394522a25a5944a4a257452422a29a510424aa994104a48\
29a54e5249a9a4144228218594524a25a5924a4a2195544209a5a4945228a1a454524a29a59252290000e0c0010020c0083ac9a8b2081b4db8f00014\
1ab21200200300409474d669a7492208314599270d29c620b5a42cc310539289f114638c392846430e31e49418174a08a183623c269543ca5051b9b7\
d43905c516637cefb117010000080200048404001820289801000607082307021d01040e6d0080810899090c0aa1c14126003c404448050089098ad2\
852e082182741164f1c08513379eb8e1840e6d1000000000001000f001009050001111d1cc55585c6064686c7074787c808404000000000008007c00\
002422404444347315161718191a1b1c1d1e1f202101000000000000000040404000000000002000000040404f6767530004112b0000000000000000\
000002000000a3e60133172a1b1719191a1a191a191918191a1a1a191818191918353ca7ad1274c1d15f0600820120d485a315bda5a5a5e53d80ce70\
38b46ddbb66ddbb66ddbd86c63d83b00fa896a3b210050c2ae020000000000d414110059b8fb8e26eb14029e8a5a765a00604037d82580ff7d010000\
0000b0b622009e7a9a375203cc808eb0ab000000000000353400287b1f8f0b9e8a5a765c0330600d7609e08fef090000000040157c4103009e8a5a76\
520330a023ec2a000000000040e3b2014072ea81c701be8a5a764c0130e018ec2a0000000000409b220050f6b0e038007e7a9a773200302023ec12c0\
bfa6b60000000000f4c13f34009e8a5a76520330a003ec2a0000000000406e0800240b5f1067005e8a1a774a00604077d855000000000080b10d0018\
791d2f047e8a9a773200302023ec2a0000000000404c190078eefae2049e7a9a77520330a003ec12c0bfcf0a00000000407f2fb5009e8a5a76520330\
a023ec2a0000000000402d0800947feb8b0d9e8a5a765c0330600d76150000000000a07139004862df2216009e8a5a365203cc800eb04b007f34b507\
00000000a09af4a20b00be8a5a76440330000676150000000000a0168b0050e234df38035e7a1a774a00604076d82580ff7d070000000080f50fbf00\
009e8a5a765c03306023ec2a000000000080d00040fde9c5069e8a5a765203306003ec12c01fdf0700000000a07a2c14009e8a5a765c0330600d7615\
000000000020370800922963e2049e7a9a775c0330600d7609e0e7540100000000a0ff7fa20f003e8a1a76b402a00b7609e0a7a9150000000000d2f7\
3808009e391a6e5514954840a203f0fe121700731400a47065d759ace9edaf8842bbdbb4005687f37dbaa1e4a67df40e5e4a394f87a76b00";

fn tone() -> Vec<u8> {
    (0..TONE.len()).step_by(2).map(|i| u8::from_str_radix(&TONE[i..i + 2], 16).unwrap()).collect()
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
    std::fs::write(&src, tone()).unwrap();
    assert_eq!(speech::suggest_file("own", "Gate Open"), "own_gate_open");
    let rel = speech::import_sound(&mut p, "own", "Gate Open", &src, "own_gate_open").unwrap();
    assert_eq!(rel, "maps/own/speech/own_gate_open.ogg");
    assert_eq!(std::fs::read(root.join("mod").join(&rel)).unwrap(), tone());
    assert_eq!(media(&p, "own")["speech"]["Gate Open"]["file"], "own_gate_open");
    let secs = speech::read_duration(&mut p, "own", "Gate Open").unwrap();
    // The stream ends at 0.5 s; the decoder returns whole blocks, so it may run on by a few hundredths.
    assert!((0.5..0.6).contains(&secs), "{secs}");
    assert_eq!(media(&p, "own")["speech"]["Gate Open"]["duration"], json!(secs));
    assert_eq!(speech::sound_seconds(&root.join("mod").join(&rel)).unwrap(), speech::sound_seconds(&root.join("mod").join(&rel)).unwrap());
    // an unplayable file: the OggS check of the copy passes, the decoder refuses
    let cut = root.join("cut.ogg");
    std::fs::write(&cut, b"OggS and nothing else").unwrap();
    assert!(speech::sound_seconds(&cut).unwrap_err().contains("cut.ogg"));
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
