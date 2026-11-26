//! A fingerprint of a file on disk, so the editor can tell its own writes from an author's or a script's (docs/roadmap.md
//! "Code-first authoring": the editor never assumes it owns a file).

use std::hash::{Hash, Hasher};
use std::path::Path;

/// Length and a hash of the content of a file: equal stamps are equal content, whatever the modification time (a touched file with
/// the same bytes is not a change). A file that cannot be read (missing) is `None`, and two missing files are the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    hash: u64,
}

impl FileStamp {
    /// The stamp of the file at `path`, or `None` when it cannot be read (missing).
    pub fn of(path: &Path) -> Option<FileStamp> {
        let bytes = std::fs::read(path).ok()?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        Some(FileStamp { len: bytes.len() as u64, hash: h.finish() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_follow_content_not_time_and_a_missing_file_is_none() {
        let d = std::env::temp_dir().join(format!("procyon-editor-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("a.json");
        assert_eq!(FileStamp::of(&f), None);
        std::fs::write(&f, "{}").unwrap();
        let first = FileStamp::of(&f);
        std::fs::write(&f, "{}").unwrap(); // rewritten with the same bytes: a new modification time
        assert_eq!(FileStamp::of(&f), first);
        std::fs::write(&f, "{ }").unwrap();
        assert_ne!(FileStamp::of(&f), first);
        let _ = std::fs::remove_dir_all(&d);
    }
}
