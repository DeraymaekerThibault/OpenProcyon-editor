//! The problems list: `authoring::check_mod` builds an overlay and is slow, so it runs on a thread and its findings come back
//! through a channel.

use openprocyon::authoring::{Finding, Level, check_mod};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

/// Runs `check_mod` in the background, one run at a time that matters: a result older than the latest request is dropped.
pub struct Checker {
    tx: Sender<(u64, Vec<Finding>)>,
    rx: Mutex<Receiver<(u64, Vec<Finding>)>>,
    latest: u64,
    running: bool,
}

impl Default for Checker {
    fn default() -> Self {
        let (tx, rx) = channel();
        Checker { tx, rx: Mutex::new(rx), latest: 0, running: false }
    }
}

impl Checker {
    /// Starts a check of the mod in `dir` against the import `base` (`None`: no import) on a new thread.
    pub fn start(&mut self, dir: PathBuf, base: Option<PathBuf>) {
        self.latest += 1;
        self.running = true;
        let (id, tx) = (self.latest, self.tx.clone());
        std::thread::spawn(move || {
            let found = std::panic::catch_unwind(|| check_mod(&dir, base.as_deref()))
                .unwrap_or_else(|_| vec![Finding::in_file(Level::Error, "", "", "the mod checker stopped unexpectedly")]);
            let _ = tx.send((id, found));
        });
    }

    /// The findings of the latest check once it has finished (errors first), else `None`.
    pub fn poll(&mut self) -> Option<Vec<Finding>> {
        let rx = self.rx.lock().ok()?;
        let mut result = None;
        while let Ok((id, found)) = rx.try_recv() {
            if id == self.latest {
                self.running = false;
                result = Some(sorted(found));
            }
        }
        result
    }

    /// A check is under way.
    pub fn running(&self) -> bool {
        self.running
    }
}

/// `found` with errors first, otherwise in the order the checker gave them.
pub fn sorted(mut found: Vec<Finding>) -> Vec<Finding> {
    found.sort_by_key(|f| f.level != Level::Error);
    found
}

/// How many errors and warnings.
pub fn counts(found: &[Finding]) -> (usize, usize) {
    let errors = found.iter().filter(|f| f.level == Level::Error).count();
    (errors, found.len() - errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procyon-editor-problems-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_misspelt_key_is_a_warning_with_file_and_path_and_comes_back_through_the_channel() {
        let dir = tmp("typo");
        std::fs::create_dir_all(dir.join("ships/Ship_New")).unwrap();
        std::fs::write(dir.join("mod.json"), r#"{"id": "t", "name": "T", "format": 1}"#).unwrap();
        std::fs::write(dir.join("ships/Ship_New/ship.json"), r#"{"sigth": 3}"#).unwrap();
        let mut c = Checker::default();
        c.start(dir.clone(), None);
        assert!(c.running());
        let mut found = None;
        for _ in 0..600 {
            if let Some(f) = c.poll() {
                found = Some(f);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let found = found.expect("the check finished");
        assert!(!c.running());
        let hit = found.iter().find(|f| f.file == "ships/Ship_New/ship.json" && f.path.contains("sigth")).unwrap_or_else(|| panic!("{found:#?}"));
        assert_eq!(hit.level, Level::Warning);
        let (e, w) = counts(&found);
        assert_eq!(e + w, found.len());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn errors_sort_first_and_keep_their_order() {
        let f = |level, m: &str| Finding::in_file(level, "f", "p", m);
        let s = sorted(vec![f(Level::Warning, "w1"), f(Level::Error, "e1"), f(Level::Warning, "w2"), f(Level::Error, "e2")]);
        assert_eq!(s.iter().map(|f| f.message.as_str()).collect::<Vec<_>>(), ["e1", "e2", "w1", "w2"]);
        assert_eq!(counts(&s), (2, 2));
    }
}
