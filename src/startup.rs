//! Command-line flags of the editor, for scripted runs and screenshots (the editor's counterpart of the game's `--shot`):
//! `--mod <dir>` opens a mod project at start, `--tab <name>` shows a tab, `--ship <id>` selects a ship (and fills the 3D view when the
//! tab is `3d`), `--map <id>` shows a map in the Maps tab, `--screenshot <file.png> [--frame N]` saves the window at frame N (default 120) and exits. Nothing here is needed for
//! normal use: the editor opens with no flags.

use std::path::PathBuf;

/// What the command line asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    pub mod_dir: Option<PathBuf>,
    pub tab: Option<String>,
    pub ship: Option<String>,
    pub map: Option<String>,
    pub screenshot: Option<PathBuf>,
    pub frame: u32,
}

/// The tab names `--tab` accepts.
pub const TABS: [&str; 7] = ["mod", "ships", "weapons", "3d", "playtest", "mission", "map"];

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
            "--screenshot" => out.screenshot = Some(absolute(value()?)),
            "--frame" => out.frame = value()?.parse().map_err(|_| "--frame needs a whole number".to_string())?,
            other => return Err(format!("unknown flag {other} (flags: --mod <dir>, --tab <{}>, --ship <id>, --map <id>, --screenshot <file.png>, --frame N)", TABS.join("|"))),
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
    fn mistakes_are_errors() {
        assert!(p(&["--mod"]).is_err());
        assert!(p(&["--mod", "--tab"]).is_err());
        assert!(p(&["--tab", "nope"]).is_err());
        assert!(p(&["--frame", "x"]).is_err());
        assert!(p(&["--sigth"]).is_err());
    }
}
