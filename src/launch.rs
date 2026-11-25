//! Play-testing runs the real game: `openprocyon --mod <folder> --map <id>` as a child process, never blocking the UI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The game binary: `PROCYON_GAME_BIN` if set, else `openprocyon` (found on PATH when spawned).
pub fn game_bin() -> PathBuf {
    bin_from(std::env::var_os("PROCYON_GAME_BIN"))
}

fn bin_from(env: Option<OsString>) -> PathBuf {
    env.filter(|v| !v.is_empty()).map_or_else(|| PathBuf::from("openprocyon"), PathBuf::from)
}

/// A screenshot request: the game writes the picture(s) and exits (`--shot`, `--shot-frame`, `--dist/--yaw/--pitch`).
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    /// Where the game writes the picture; absolute. With several frames the game writes `<stem>_<N>.png` next to it.
    pub file: PathBuf,
    /// The frames to capture, in order.
    pub frames: Vec<u32>,
    pub dist: Option<f32>,
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
}

/// A skirmish set up through the lobby (`--lobby "map=<id>;points=P;limit=L;slots=human,N*computer[;seed=S]"`, the engine's
/// `menu/lobby_scenario.rs`). The map must be a skirmish map (`"skirmish": true`), else the game says "no skirmish map".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lobby {
    /// Victory points (`points`); the engine takes 1000 upward in its own steps and refuses other values.
    pub points: u32,
    /// Fleet size limit (`limit`), as the engine's range allows.
    pub limit: u32,
    /// Computer rows after the one human row (`slots=human,N*computer`).
    pub computers: u32,
    pub seed: Option<u32>,
}

impl Default for Lobby {
    fn default() -> Lobby {
        Lobby { points: 1000, limit: 10, computers: 1, seed: None }
    }
}

impl Lobby {
    /// The `--lobby` value for `map`. The ranges are the engine's to check; it exits with a message on a bad spec.
    pub fn spec(&self, map: &str) -> String {
        let mut s = format!("map={map};points={};limit={};slots=human", self.points, self.limit);
        if self.computers > 0 {
            s.push_str(&format!(",{}*computer", self.computers));
        }
        if let Some(seed) = self.seed {
            s.push_str(&format!(";seed={seed}"));
        }
        s
    }
}

/// One game start: a mod, a map, optionally a ship, a lobby set-up and a screenshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    /// The mod folder, absolute.
    pub mod_dir: PathBuf,
    pub map: String,
    pub ship: Option<String>,
    pub shot: Option<Shot>,
    /// Play the map through the skirmish lobby (`--lobby`) instead of straight into the battle (`--map`).
    pub lobby: Option<Lobby>,
    /// `--only-mod` (the engine's `modding::ONLY_MOD`): the game loads this mod and none of the player's installed mods, so a run is
    /// of this mod alone. On by default.
    pub only_mod: bool,
    /// Campaign mission N (`--mission N`, which the engine turns into the map `missionNN`, `lib.rs:282`); replaces `--map`. Not with a lobby.
    pub mission: Option<u32>,
    /// `--fire-rule "<rule>"`, repeatable: after the world-initialise rules, these rules' actions run in this order (engine `lib.rs:276`,
    /// `rules/eval.rs:250`). A name that matches no rule only logs a warning.
    pub fire_rules: Vec<String>,
    /// `--start-at x,y`: places the primary ship (game units; engine `lib.rs:275`).
    pub start_at: Option<(f32, f32)>,
    /// `--rule-trace`: the game logs why rules have not fired (engine `rules/cli.rs:40`).
    pub rule_trace: bool,
    /// Unattended autoplay of this many game seconds: `PROCYON_AUTOPLAY=<n>` in the environment and `--script mission` (the autopilot).
    /// The game quits when the mission ends and logs one `autoplay end` line (see `launch_report`). The engine has no flag for it.
    pub autoplay: Option<u32>,
}

impl Launch {
    /// A launch of `map` from the mod in `mod_dir`; the folder is made absolute (the game runs in another working directory).
    pub fn new(mod_dir: &Path, map: &str) -> Launch {
        Launch {
            mod_dir: absolute(mod_dir),
            map: map.to_string(),
            ship: None,
            shot: None,
            lobby: None,
            only_mod: true,
            mission: None,
            fire_rules: vec![],
            start_at: None,
            rule_trace: false,
            autoplay: None,
        }
    }

    /// The command line, in the engine's flag order:
    /// `--mod <abs> [--only-mod] --map <id> [--ship <id>] [--shot <abs> --shot-frame N[,M] [--dist D --yaw Y --pitch P]]
    /// [--fire-rule <rule>]... [--start-at X,Y] [--rule-trace] [--script mission]`, with `--mission N` in place of `--map <id>` when
    /// `mission` is set (the environment of an autoplay run is in [`Launch::envs`]), or, for a
    /// lobby launch, `--mod <abs> [--only-mod] --lobby <spec>` (no `--map`: with it the game skips the lobby; no `--ship`: the
    /// lobby's Auto Assign picks the fleet).
    pub fn args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["--mod".into(), self.mod_dir.clone().into()];
        if self.only_mod {
            a.push(openprocyon::modding::ONLY_MOD.into());
        }
        if let Some(lobby) = &self.lobby {
            a.extend(["--lobby".into(), lobby.spec(&self.map).into()]);
        } else if let Some(n) = self.mission {
            a.extend(["--mission".into(), n.to_string().into()]);
        } else {
            a.extend(["--map".into(), self.map.clone().into()]);
        }
        if let (Some(ship), None) = (&self.ship, &self.lobby) {
            a.extend(["--ship".into(), ship.clone().into()]);
        }
        if let Some(s) = &self.shot {
            let frames = s.frames.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
            a.extend(["--shot".into(), absolute(&s.file).into(), "--shot-frame".into(), frames.into()]);
            for (flag, v) in [("--dist", s.dist), ("--yaw", s.yaw), ("--pitch", s.pitch)] {
                if let Some(v) = v {
                    a.extend([flag.into(), v.to_string().into()]);
                }
            }
        }
        for rule in &self.fire_rules {
            a.extend(["--fire-rule".into(), rule.clone().into()]);
        }
        if let Some((x, y)) = self.start_at {
            a.extend(["--start-at".into(), format!("{x},{y}").into()]);
        }
        if self.rule_trace {
            a.push("--rule-trace".into());
        }
        if self.autoplay.is_some() {
            a.extend(["--script".into(), "mission".into()]);
        }
        a
    }

    /// The environment variables the game needs besides the arguments: `PROCYON_AUTOPLAY=<seconds>` for an autoplay run.
    pub fn envs(&self) -> Vec<(&'static str, String)> {
        self.autoplay.map(|n| ("PROCYON_AUTOPLAY", n.to_string())).into_iter().collect()
    }

    /// Starts the game and returns at once; poll the child with `try_wait`.
    pub fn spawn(&self) -> std::io::Result<Child> {
        Command::new(game_bin()).args(self.args()).envs(self.envs()).spawn()
    }

    /// Like [`Launch::spawn`], with the child's stderr written to `stderr_file` (a file never fills up like a pipe nobody reads).
    pub fn spawn_logged(&self, stderr_file: &Path) -> std::io::Result<Child> {
        if let Some(parent) = stderr_file.parent() {
            crate::project::guard::create_dir_all(parent, None).map_err(std::io::Error::other)?;
        }
        let log = crate::project::guard::create_file(stderr_file, None).map_err(std::io::Error::other)?;
        Command::new(game_bin()).args(self.args()).envs(self.envs()).stdout(Stdio::null()).stderr(log).spawn()
    }
}

/// One line of `--check-mod --json`: the fields the panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModFinding {
    pub error: bool,
    pub file: String,
    pub path: String,
    pub message: String,
}

impl ModFinding {
    /// `file: path: message`, leaving out what is empty.
    pub fn line(&self) -> String {
        [self.file.as_str(), self.path.as_str(), self.message.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(": ")
    }
}

/// The result of `<game bin> --check-mod <mod> --json`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModCheck {
    pub findings: Vec<ModFinding>,
}

impl ModCheck {
    pub fn errors(&self) -> usize {
        self.findings.iter().filter(|f| f.error).count()
    }
    pub fn warnings(&self) -> usize {
        self.findings.len() - self.errors()
    }
}

/// The command line of the mod check: `--check-mod <abs mod folder> --json`.
pub fn check_mod_args(mod_dir: &Path) -> Vec<OsString> {
    vec!["--check-mod".into(), absolute(mod_dir).into(), "--json".into()]
}

/// Reads the output of `--check-mod --json` (`{"errors", "warnings", "findings": [{"level", "file", "path", "message", ..}]}`).
pub fn parse_mod_check(output: &str) -> Result<ModCheck, String> {
    let v: serde_json::Value = serde_json::from_str(output.trim()).map_err(|e| format!("unexpected output of --check-mod: {e}"))?;
    let list = v["findings"].as_array().ok_or("unexpected output of --check-mod: no \"findings\" list")?;
    let text = |f: &serde_json::Value, k: &str| f[k].as_str().unwrap_or_default().to_string();
    let findings = list.iter().map(|f| ModFinding { error: f["level"] == "error", file: text(f, "file"), path: text(f, "path"), message: text(f, "message") }).collect();
    Ok(ModCheck { findings })
}

/// Runs `<game bin> --check-mod <mod> --json` and reads the findings. The game exits 1 when there are errors; that is still a result.
/// Blocks while the game checks: call it from a thread.
pub fn check_mod(mod_dir: &Path) -> Result<ModCheck, String> {
    let bin = game_bin();
    let out = Command::new(&bin).args(check_mod_args(mod_dir)).output().map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    parse_mod_check(&String::from_utf8_lossy(&out.stdout)).map_err(|e| {
        let err = String::from_utf8_lossy(&out.stderr);
        format!("{e} ({}{})", out.status, err.lines().next().map_or(String::new(), |l| format!(": {l}")))
    })
}

/// Starts map `map_id` of the mod in `mod_dir` without a screenshot (kept for the spike panels).
pub fn spawn(mod_dir: &Path, map_id: &str) -> std::io::Result<Child> {
    Launch::new(mod_dir, map_id).spawn()
}

fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The files the game writes for `shot`: the file itself for one frame, `<stem>_<N>.png` for each of a list of frames.
pub fn shot_outputs(shot: &Shot) -> Vec<PathBuf> {
    let file = absolute(&shot.file);
    if shot.frames.len() <= 1 {
        return vec![file];
    }
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = file.extension().map_or("png".into(), |e| e.to_string_lossy().into_owned());
    shot.frames.iter().map(|n| file.with_file_name(format!("{stem}_{n}.{ext}"))).collect()
}

/// Reads the version out of the output of `--version` (`OpenProcyon 0.2.0`).
pub fn parse_version(output: &str) -> Result<String, String> {
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix("OpenProcyon "))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("unexpected output of --version: {:?}", output.trim()))
}

/// The file `bin` runs: itself when it has a folder part, else the first match on `path` (what the OS does when it spawns it).
fn resolve_bin(bin: &Path, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if bin.components().count() > 1 {
        return Some(bin.to_path_buf());
    }
    std::env::split_paths(path?).map(|d| d.join(bin)).find(|c| c.is_file())
}

/// The folder of the `mods/` the game reads besides the player's own, for a game binary at `bin` started in `cwd`: `mods/` next to the
/// binary when there is one, else `mods/` of the source checkout found from `cwd` and then from the binary (the nearest folder with
/// `game/Cargo.toml` and `content/`). PROVISIONAL: this restates the pinned engine's `bundled_mods_dir()` (that function looks at the
/// editor's own binary, not the game's, so it cannot be called); the engine should take the binary as an argument.
pub fn bundled_mods_for(bin: Option<&Path>, cwd: Option<&Path>) -> Option<PathBuf> {
    let beside = bin.and_then(Path::parent).map(|d| d.join("mods")).filter(|d| d.is_dir());
    let checkout = |start: &Path| start.ancestors().find(|d| d.join("game/Cargo.toml").is_file() && d.join("content").is_dir()).map(|d| d.join("mods")).filter(|d| d.is_dir());
    beside.or_else(|| cwd.and_then(checkout)).or_else(|| bin.and_then(checkout))
}

/// [`bundled_mods_for`] for the game binary this editor starts (`PROCYON_GAME_BIN`, else `openprocyon` on the PATH) and this
/// process's working directory, which the game inherits.
pub fn bundled_mods() -> Option<PathBuf> {
    let bin = resolve_bin(&game_bin(), std::env::var_os("PATH").as_deref());
    let bin = bin.map(|b| std::fs::canonicalize(&b).unwrap_or(b));
    bundled_mods_for(bin.as_deref(), std::env::current_dir().ok().as_deref())
}

/// Runs `<game bin> --version` and returns the version it prints; compare it with `openprocyon::authoring::ENGINE_VERSION`.
pub fn game_version() -> Result<String, String> {
    version_of(&game_bin())
}

fn version_of(bin: &Path) -> Result<String, String> {
    let out = Command::new(bin).arg("--version").output().map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!("{} --version failed: {}", bin.display(), out.status));
    }
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A launch without `--only-mod`, so the arguments tests read the flag order of the rest.
    fn plain(dir: &Path, map: &str) -> Launch {
        Launch { only_mod: false, ..Launch::new(dir, map) }
    }

    fn shot(file: &str, frames: &[u32]) -> Shot {
        Shot { file: file.into(), frames: frames.to_vec(), dist: None, yaw: None, pitch: None }
    }

    #[test]
    fn arguments_are_mod_then_map() {
        let l = plain(Path::new("/m/gates"), "gate_ambush");
        assert_eq!(l.args(), ["--mod", "/m/gates", "--map", "gate_ambush"].map(OsString::from));
    }

    #[test]
    fn ship_and_shot_flags_follow_in_the_engines_order() {
        let mut l = plain(Path::new("/m/gates"), "gate_ambush");
        l.ship = Some("Ship_X".into());
        assert_eq!(l.args(), ["--mod", "/m/gates", "--map", "gate_ambush", "--ship", "Ship_X"].map(OsString::from));
        l.ship = None;
        l.shot = Some(shot("/s/a.png", &[300]));
        assert_eq!(l.args(), ["--mod", "/m/gates", "--map", "gate_ambush", "--shot", "/s/a.png", "--shot-frame", "300"].map(OsString::from));
        l.ship = Some("Ship_X".into());
        l.shot = Some(Shot { dist: Some(120.0), yaw: Some(30.5), pitch: Some(-10.0), ..shot("/s/a.png", &[100, 300]) });
        assert_eq!(
            l.args(),
            ["--mod", "/m/gates", "--map", "gate_ambush", "--ship", "Ship_X", "--shot", "/s/a.png", "--shot-frame", "100,300", "--dist", "120", "--yaw", "30.5", "--pitch", "-10"]
                .map(OsString::from)
        );
        // only some camera flags
        l.shot = Some(Shot { yaw: Some(5.0), ..shot("/s/a.png", &[1]) });
        let a = l.args();
        assert!(a.ends_with(&["--yaw", "5"].map(OsString::from)) && !a.contains(&"--dist".into()));
    }

    #[test]
    fn paths_are_absolute_even_from_relative_input() {
        let mut l = plain(Path::new("mods/gates"), "m");
        l.shot = Some(shot("local/a.png", &[300]));
        let a = l.args();
        assert!(Path::new(&a[1]).is_absolute());
        assert!(Path::new(&a[5]).is_absolute());
        assert!(shot_outputs(l.shot.as_ref().unwrap())[0].is_absolute());
    }

    #[test]
    fn shot_outputs_name_one_frame_and_a_list() {
        assert_eq!(shot_outputs(&shot("/s/a.png", &[300])), [PathBuf::from("/s/a.png")]);
        assert_eq!(shot_outputs(&shot("/s/a.png", &[100, 300])), [PathBuf::from("/s/a_100.png"), PathBuf::from("/s/a_300.png")]);
    }

    #[test]
    fn version_output_is_parsed() {
        assert_eq!(parse_version("OpenProcyon 0.2.0\n"), Ok("0.2.0".into()));
        assert!(parse_version("hello\n").is_err());
        assert!(parse_version("").is_err());
    }

    #[test]
    fn a_missing_binary_has_no_version() {
        assert!(version_of(Path::new("/nonexistent/openprocyon")).is_err());
    }

    #[test]
    fn binary_from_the_environment_else_path() {
        assert_eq!(bin_from(Some("/opt/g/openprocyon".into())), PathBuf::from("/opt/g/openprocyon"));
        assert_eq!(bin_from(Some("".into())), PathBuf::from("openprocyon"));
        assert_eq!(bin_from(None), PathBuf::from("openprocyon"));
    }

    #[test]
    fn the_bundled_mods_folder_is_found_the_way_the_game_finds_it() {
        let d = std::env::temp_dir().join(format!("procyon-editor-bundled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (dl, co) = (d.join("download"), d.join("checkout"));
        for p in [dl.join("mods"), co.join("mods"), co.join("game"), co.join("content"), co.join("target/debug")] {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(co.join("game/Cargo.toml"), "").unwrap();
        // next to the binary wins
        assert_eq!(bundled_mods_for(Some(&dl.join("openprocyon")), Some(&co)), Some(dl.join("mods")));
        // else the checkout found from the working directory, then from the binary
        let bin = co.join("target/debug/openprocyon");
        assert_eq!(bundled_mods_for(Some(&bin), Some(&d)), Some(co.join("mods")));
        assert_eq!(bundled_mods_for(Some(&bin), Some(&co.join("game"))), Some(co.join("mods")));
        assert_eq!(bundled_mods_for(Some(&d.join("elsewhere/openprocyon")), Some(&d)), None);
        assert_eq!(bundled_mods_for(None, None), None);
        // a bare name is looked up on the PATH like the OS does; a path is kept
        let path = std::env::join_paths([d.join("none"), dl.clone()]).unwrap();
        std::fs::write(dl.join("openprocyon"), "").unwrap();
        assert_eq!(resolve_bin(Path::new("openprocyon"), Some(&path)), Some(dl.join("openprocyon")));
        assert_eq!(resolve_bin(Path::new("openprocyon"), None), None);
        assert_eq!(resolve_bin(Path::new("/x/openprocyon"), None), Some(PathBuf::from("/x/openprocyon")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_binary_is_an_error_not_a_panic() {
        let r = Command::new("/nonexistent/openprocyon").args(Launch::new(Path::new("."), "x").args()).spawn();
        assert!(r.is_err());
    }
}
