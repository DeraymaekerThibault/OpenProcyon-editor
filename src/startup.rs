//! Command-line flags of the editor, for scripted runs and screenshots (the editor's counterpart of the game's `--shot`):
//! `--mod <dir>` opens a mod project at start, `--tab <name>` shows a tab, `--ship <id>` selects a ship (and fills the 3D view when the
//! tab is `3d`), `--map <id>` shows a map in the Maps tab (`--tab map3d` opens it in the 3D view), `--select <array>:<index>` selects an entry there (e.g. `objects:0`, `ships:0`), `--screenshot <file.png> [--frame N]` saves the window at frame N (default 120) and exits. Nothing here is needed for
//! normal use: the editor opens with no flags.

use std::path::PathBuf;

/// What the command line asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    pub mod_dir: Option<PathBuf>,
    pub tab: Option<String>,
    pub ship: Option<String>,
    pub map: Option<String>,
    /// `--select`: entries of the map to select, as (array key, index).
    pub select: Vec<(String, usize)>,
    /// `--points`: the 3D map view starts in the scenery-points mode (T12).
    pub points: bool,
    pub screenshot: Option<PathBuf>,
    pub frame: u32,
}

/// The tab names `--tab` accepts.
pub const TABS: [&str; 8] = ["mod", "ships", "weapons", "3d", "playtest", "mission", "map", "map3d"];

/// Reads the flags from `args` (without the program name). Unknown flags and missing values are errors, so a typo in a script fails
/// instead of silently opening something else. Paths are made absolute against the working folder.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut out = Args { frame: 120, ..Args::default() };
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().filter(|v| !v.starts_with("--")).ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--mod" => out.mod_dir = Some(absolute(value()?)),
            "--tab" => {
                let t = value()?.to_lowercase();
                if !TABS.contains(&t.as_str()) {
                    return Err(format!("--tab {t}: not one of {}", TABS.join(", ")));
                }
                out.tab = Some(t);
            }
            "--ship" => out.ship = Some(value()?),
            "--map" => out.map = Some(value()?),
            "--select" => {
                let v = value()?;
                let (array, index) = v.split_once(':').and_then(|(a, i)| Some((a, i.parse::<usize>().ok()?))).ok_or("--select needs <array>:<index>, e.g. objects:0")?;
                if !["objects", "ships", "mines", "players", "teams", "nebulae"].contains(&array) {
                    return Err(format!("--select {v}: unknown array {array}"));
                }
                out.select.push((array.to_string(), index));
            }
            "--points" => out.points = true,
            "--screenshot" => out.screenshot = Some(absolute(value()?)),
            "--frame" => out.frame = value()?.parse().map_err(|_| "--frame needs a whole number".to_string())?,
            other => {
                return Err(format!(
                    "unknown flag {other} (flags: --mod <dir>, --tab <{}>, --ship <id>, --map <id>, --select <array>:<index>, --points, --screenshot <file.png>, --frame N)",
                    TABS.join("|")
                ));
            }
        }
    }
    Ok(out)
}

fn absolute(p: String) -> PathBuf {
    std::path::absolute(&p).unwrap_or_else(|_| PathBuf::from(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Args, String> {
        parse(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_flags_is_a_normal_start() {
        assert_eq!(p(&[]).unwrap(), Args { frame: 120, ..Args::default() });
    }

    #[test]
    fn every_flag_and_absolute_paths() {
        let a = p(&["--mod", "rel/mod", "--tab", "3D", "--ship", "Ship_X", "--map", "red", "--screenshot", "out.png", "--frame", "30"]).unwrap();
        assert!(a.mod_dir.as_ref().unwrap().is_absolute() && a.mod_dir.as_ref().unwrap().ends_with("rel/mod"));
        assert_eq!((a.tab.as_deref(), a.ship.as_deref(), a.map.as_deref(), a.frame), (Some("3d"), Some("Ship_X"), Some("red"), 30));
        assert!(a.screenshot.unwrap().is_absolute());
    }

    #[test]
    fn map3d_tab_and_select() {
        let a = p(&["--tab", "map3d", "--select", "objects:1", "--select", "ships:0"]).unwrap();
        assert_eq!(a.tab.as_deref(), Some("map3d"));
        assert_eq!(a.select, vec![("objects".to_string(), 1), ("ships".to_string(), 0)]);
        assert!(p(&["--points"]).unwrap().points && !p(&[]).unwrap().points);
        assert!(p(&["--select", "objects"]).is_err() && p(&["--select", "rocks:1"]).is_err() && p(&["--select", "ships:x"]).is_err());
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(p(&["--mod"]).is_err());
        assert!(p(&["--mod", "--tab"]).is_err());
        assert!(p(&["--tab", "nope"]).is_err());
        assert!(p(&["--frame", "x"]).is_err());
        assert!(p(&["--sigth"]).is_err());
    }
}
