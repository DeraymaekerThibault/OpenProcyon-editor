//! Play-testing runs the real game: `openprocyon --mod <folder> --map <id>` as a child process, never blocking the UI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use openprocyon::authoring::RuleReport;

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
    /// The game quits when the mission ends. What the rules did is in the rule report ([`Launch::rule_report`]).
    pub autoplay: Option<u32>,
    /// `--rule-report <file>`: the game writes what each rule of the mission did to this file when the battle ends
    /// (`openprocyon::authoring::RuleReport`; [`read_rule_report`]).
    pub rule_report: Option<PathBuf>,
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
            rule_report: None,
        }
    }

    /// The command line, in the engine's flag order:
    /// `--mod <abs> [--only-mod] --map <id> [--ship <id>] [--shot <abs> --shot-frame N[,M] [--dist D --yaw Y --pitch P]]
    /// [--fire-rule <rule>]... [--start-at X,Y] [--rule-trace] [--rule-report <abs>] [--script mission]`, with `--mission N` in place of `--map <id>` when
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
        if let Some(f) = &self.rule_report {
            a.extend(["--rule-report".into(), absolute(f).into()]);
        }
        if self.autoplay.is_some() {
            a.extend(["--script".into(), "mission".into()]);
        }
        a
    }

    /// The environment variables the game needs besides the arguments: `PROCYON_AUTOPLAY=<seconds>` for an autoplay run, and always `NO_COLOR`.
    pub fn envs(&self) -> Vec<(&'static str, String)> {
        // `NO_COLOR=1`: the output is shown as plain text in the panel, so the game's logger should not colour it.
        let mut e = vec![("NO_COLOR", "1".to_string())];
        e.extend(self.autoplay.map(|n| ("PROCYON_AUTOPLAY", n.to_string())));
        e
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

/// `text` without ANSI escape sequences (CSI colour codes and OSC strings), which a terminal draws and a text label shows as boxes.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match it.peek() {
            Some('[') => {
                it.next();
                // Parameter and intermediate bytes, then one final byte 0x40..=0x7e.
                for n in it.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&n) {
                        break;
                    }
                }
            }
            Some(']') => {
                it.next();
                // An OSC string ends with BEL or ESC \.
                while let Some(n) = it.next() {
                    if n == '\u{7}' {
                        break;
                    }
                    if n == '\u{1b}' {
                        it.next_if_eq(&'\\');
                        break;
                    }
                }
            }
            Some(_) => {
                it.next();
            }
            None => {}
        }
    }
    out
}

/// Whether a log line is a warning or an error: the level word as the game's logger prints it, or a panic.
fn is_problem_line(line: &str) -> bool {
    line.split_whitespace().take(4).any(|w| matches!(w.trim_end_matches(':'), "WARN" | "ERROR")) || line.contains("panicked at")
}

/// Whether a line reads like a failure whatever its level word: "error", "cannot" or "panicked" in any case.
fn mentions_failure(line: &str) -> bool {
    let l = line.to_lowercase();
    l.contains("error") || l.contains("cannot") || l.contains("panicked")
}

/// What the game says went wrong: the last `n` of the WARN/ERROR lines, the lines that say error, cannot or panicked, and the last
/// few lines of the output (the game's real last words often have no level word), in the order printed and each once.
pub fn error_lines(text: &str, n: usize) -> String {
    let text = strip_ansi(text);
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let tail_from = lines.len().saturating_sub(3);
    let picked: Vec<&str> = lines.iter().enumerate().filter(|(i, l)| *i >= tail_from || is_problem_line(l) || mentions_failure(l)).map(|(_, l)| *l).collect();
    picked[picked.len().saturating_sub(n)..].join("\n")
}

/// What to tell the player when the game ended: `None` for a clean exit (code 0), else the exit code (or that a signal ended it)
/// and the last lines the game printed, which is where it says what went wrong.
pub fn exit_report(code: Option<i32>, success: bool, output: &str) -> Option<String> {
    if success {
        return None;
    }
    let head = code.map_or_else(|| "The game was ended by a signal".to_string(), |c| format!("The game exited with code {c}"));
    let tail = error_lines(output, 8);
    Some(if tail.is_empty() { format!("{head} and printed nothing.") } else { format!("{head}. What it printed last:\n{tail}") })
}

/// A game process the editor started, with its stdout and stderr in one log file so that a failure can be shown.
pub struct Game {
    child: Child,
    log: PathBuf,
    /// The `--rule-report` file this run was given (in the temp folder), if any; read once the game has ended ([`Game::report`]).
    report: Option<PathBuf>,
}

impl Drop for Game {
    fn drop(&mut self) {
        if self.owns_report()
            && let Some(p) = &self.report
        {
            let _ = crate::project::guard::remove_file(p, None);
        }
    }
}

/// How a [`Game`] stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameState {
    Running,
    /// Ended: `None` after a clean exit, else the report ([`exit_report`]).
    Ended(Option<String>),
}

impl Game {
    /// Starts `launch` with the output captured in a file in the temp folder. Unless `launch` brings its own, the game is given a
    /// `--rule-report` file in the temp folder too (not for a screenshot run, which ends before a battle does).
    pub fn spawn(launch: &Launch) -> std::io::Result<Game> {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stem = format!("procyon-editor-game-{}-{n}", std::process::id());
        let log = std::env::temp_dir().join(format!("{stem}.log"));
        let mut launch = launch.clone();
        let mut own_report = None;
        if launch.rule_report.is_none() && launch.shot.is_none() {
            let report = std::env::temp_dir().join(format!("{stem}.rules.json"));
            crate::project::guard::check_writable(&report, None).map_err(std::io::Error::other)?;
            // A leftover of an earlier run with the same name must not pass for this run's report.
            let _ = crate::project::guard::remove_file(&report, None);
            launch.rule_report = Some(report.clone());
            own_report = Some(report);
        }
        let out = crate::project::guard::create_file(&log, None).map_err(std::io::Error::other)?;
        let err = out.try_clone()?;
        let child = Command::new(game_bin()).args(launch.args()).envs(launch.envs()).stdout(out).stderr(err).spawn()?;
        Ok(Game { child, log, report: launch.rule_report.clone().or(own_report) })
    }

    /// What the rules did in this run, from the `--rule-report` file; call it once the game has ended ([`Game::poll`]; the game may
    /// rewrite the file while it runs, the last write wins). `Ok(None)`: no file, because the game ended before a battle with
    /// mission rules did (or crashed). The temp file goes when the [`Game`] is dropped.
    pub fn report(&self) -> Result<Option<RuleReport>, String> {
        let Some(path) = &self.report else { return Ok(None) };
        read_rule_report(path)
    }

    /// The report file is one of ours (in the temp folder, named for this run), not one the caller's `Launch` brought.
    fn owns_report(&self) -> bool {
        self.report.as_deref().is_some_and(|p| p.parent() == self.log.parent() && p.extension().is_some_and(|e| e == "json"))
    }

    /// Whether it still runs; once it ended, the report (and the log file is removed).
    pub fn poll(&mut self) -> GameState {
        match self.child.try_wait() {
            Ok(None) => GameState::Running,
            Ok(Some(status)) => {
                let report = exit_report(status.code(), status.success(), &self.output());
                let _ = crate::project::guard::remove_file(&self.log, None);
                GameState::Ended(report)
            }
            Err(e) => GameState::Ended(Some(format!("Cannot tell whether the game still runs: {e}"))),
        }
    }

    /// Ends the process (the play-test's "Stop").
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = crate::project::guard::remove_file(&self.log, None);
    }

    /// The end of the log (the last 16 KiB), lossily as text.
    fn output(&self) -> String {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut f) = std::fs::File::open(&self.log) else { return String::new() };
        let len = f.metadata().map_or(0, |m| m.len());
        let _ = f.seek(SeekFrom::Start(len.saturating_sub(16 * 1024)));
        let mut bytes = Vec::new();
        let _ = f.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// Parses the `--rule-report` JSON.
pub fn parse_rule_report(text: &str) -> Result<RuleReport, String> {
    serde_json::from_str(text).map_err(|e| format!("the rule report is not what this editor expects (engine version?): {e}"))
}

/// Reads a `--rule-report` file: `Ok(None)` when there is none, an error when it cannot be read or parsed.
pub fn read_rule_report(path: &Path) -> Result<Option<RuleReport>, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => parse_rule_report(&t).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// One line of `--check-mod --json`: the fields the panel shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModFinding {
    pub error: bool,
    /// A warning the base game's own files have too (`"inherited": true` in the JSON): not the mod's, never blocks.
    pub inherited: bool,
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
    /// The mod's own warnings (not the inherited ones).
    pub fn warnings(&self) -> usize {
        self.own().count() - self.errors()
    }
    /// The findings that are the mod's own: errors and warnings the base game does not have.
    pub fn own(&self) -> impl Iterator<Item = &ModFinding> {
        self.findings.iter().filter(|f| !f.inherited)
    }
    /// The warnings that come from the base game's own files.
    pub fn inherited(&self) -> Vec<&ModFinding> {
        self.findings.iter().filter(|f| f.inherited).collect()
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
    let findings = list
        .iter()
        .map(|f| ModFinding {
            error: f["level"] == "error",
            inherited: f["inherited"].as_bool().unwrap_or(false) && f["level"] != "error",
            file: text(f, "file"),
            path: text(f, "path"),
            message: text(f, "message"),
        })
        .collect();
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
    fn escape_sequences_go_and_the_warnings_and_errors_are_what_is_shown() {
        assert_eq!(strip_ansi("\u{1b}[2m2026\u{1b}[0m \u{1b}[31;1mERROR\u{1b}[0m x\u{1b}]0;title\u{7}y"), "2026 ERROR xy");
        let log = "\u{1b}[32m INFO\u{1b}[0m starting\n\u{1b}[33m WARN\u{1b}[0m bevy: slow\n\u{1b}[32m INFO\u{1b}[0m loading\n\u{1b}[31mERROR\u{1b}[0m engine: bad map\n\u{1b}[32m INFO\u{1b}[0m bye";
        assert_eq!(error_lines(log, 8), " WARN bevy: slow\n INFO loading\nERROR engine: bad map\n INFO bye");
        assert_eq!(error_lines(log, 1), " INFO bye");
        // The lobby's last line has no level word; the three unrelated WARN lines before it must not hide it.
        let lobby = "WARN xsettings\nWARN software adapter\nWARN XRandR\nINFO starting\nINFO lobby\nlobby scenario cannot launch: IDGS_X_PLEASE_SELECT_A_RACE";
        let r = error_lines(lobby, 8);
        assert!(r.ends_with("lobby scenario cannot launch: IDGS_X_PLEASE_SELECT_A_RACE") && r.contains("WARN XRandR"), "{r}");
        assert_eq!(r.matches("lobby scenario").count(), 1, "a line that is both tail and failure is listed once");
        assert!(error_lines("Something Failed: PANICKED here\nb\nc\nd\ne", 8).starts_with("Something Failed"));
        assert_eq!(error_lines("a\nb\nc", 2), "b\nc");
        let l = Launch::new(Path::new("/m"), "x");
        assert!(l.envs().contains(&("NO_COLOR", "1".to_string())));
    }

    #[test]
    fn a_failed_exit_shows_the_code_and_the_last_lines_and_a_clean_one_nothing() {
        assert_eq!(exit_report(Some(0), true, "all fine"), None);
        let out = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n") + "\n\nERROR: no such map\n";
        let r = exit_report(Some(2), false, &out).unwrap();
        assert!(r.starts_with("The game exited with code 2. What it printed last:") && r.ends_with("ERROR: no such map"), "{r}");
        let quiet = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let r = exit_report(Some(2), false, &quiet).unwrap();
        assert!(!r.contains("line 5\n") && r.contains("line 20"), "no WARN/ERROR: the last lines: {r}");
        assert!(exit_report(None, false, "").unwrap().contains("signal"));
        assert!(exit_report(Some(1), false, "").unwrap().contains("printed nothing"));
    }

    #[cfg(unix)]
    #[test]
    fn a_game_that_fails_is_reported_with_its_output_and_one_that_runs_can_be_stopped() {
        // `sh` stands in for the game binary through PROCYON_GAME_BIN would need a process-wide env; spawn the pieces directly.
        let log = std::env::temp_dir().join(format!("procyon-editor-game-test-{}.log", std::process::id()));
        let out = std::fs::File::create(&log).unwrap();
        let err = out.try_clone().unwrap();
        let child = Command::new("sh").args(["-c", "echo out line; echo 'bad map' >&2; exit 3"]).stdout(out).stderr(err).spawn().unwrap();
        let mut g = Game { child, log: log.clone(), report: None };
        let state = loop {
            match g.poll() {
                GameState::Running => std::thread::sleep(std::time::Duration::from_millis(10)),
                s => break s,
            }
        };
        let GameState::Ended(Some(r)) = state else { panic!("{state:?}") };
        assert!(r.contains("code 3") && r.contains("out line") && r.contains("bad map"), "{r}");
        assert!(!log.exists(), "the log is removed");
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let mut g = Game { child, log, report: None };
        assert_eq!(g.poll(), GameState::Running);
        g.stop();
        assert!(matches!(g.poll(), GameState::Ended(_)));
    }

    const REPORT: &str = r#"{"mission":"gates","outcome":"victory","play_time":412.5,"rules":[
        {"name":"Open the gate","fired":true,"runs":2,"first_run":10.0,"last_run":300.5,"active":true},
        {"name":"Never happens","fired":false,"runs":0,"active":false}]}"#;

    #[test]
    fn a_rule_report_is_read_and_a_missing_or_broken_one_is_told_apart() {
        let r = parse_rule_report(REPORT).unwrap();
        assert_eq!((r.mission.as_str(), r.outcome, r.play_time), ("gates", openprocyon::authoring::RuleReportOutcome::Victory, 412.5));
        assert_eq!((r.rules[0].runs, r.rules[0].first_run, r.rules[0].last_run), (2, Some(10.0), Some(300.5)));
        assert!(!r.rules[1].fired && r.rules[1].first_run.is_none());
        assert!(parse_rule_report("{\"mission\":1}").unwrap_err().contains("rule report"));
        let dir = std::env::temp_dir().join(format!("procyon-editor-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("r.json");
        assert_eq!(read_rule_report(&file), Ok(None));
        std::fs::write(&file, REPORT).unwrap();
        assert_eq!(read_rule_report(&file).unwrap().unwrap(), r);
        std::fs::write(&file, "{ cut").unwrap();
        assert!(read_rule_report(&file).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_game_reports_after_it_ended_and_its_own_file_goes_away() {
        let dir = std::env::temp_dir();
        let log = dir.join(format!("procyon-editor-game-test2-{}.log", std::process::id()));
        let report = dir.join(format!("procyon-editor-game-test2-{}.rules.json", std::process::id()));
        std::fs::write(&report, REPORT).unwrap();
        let child = Command::new("true").spawn().unwrap();
        let g = Game { child, log, report: Some(report.clone()) };
        assert_eq!(g.report().unwrap().unwrap().rules.len(), 2);
        drop(g);
        assert!(!report.exists(), "the temp report is removed");
    }

    #[test]
    fn a_missing_binary_is_an_error_not_a_panic() {
        let r = Command::new("/nonexistent/openprocyon").args(Launch::new(Path::new("."), "x").args()).spawn();
        assert!(r.is_err());
    }
}
