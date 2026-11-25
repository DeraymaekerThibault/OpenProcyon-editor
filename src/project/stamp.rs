//! A fingerprint of a file on disk, so the editor can tell its own writes from an author's or a script's (docs/roadmap.md
//! "Code-first authoring": the editor never assumes it owns a file).

use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::SystemTime;

/// Length, modification time and a hash of the content of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    pub mtime: Option<SystemTime>,
    pub hash: u64,
}

impl FileStamp {
    /// The stamp of the file at `path`, or `None` when it cannot be read (missing).
    pub fn of(path: &Path) -> Option<FileStamp> {
        let bytes = std::fs::read(path).ok()?;
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        Some(FileStamp::of_bytes(&bytes, mtime))
    }

    /// The stamp of `bytes` as written at `mtime`.
    pub fn of_bytes(bytes: &[u8], mtime: Option<SystemTime>) -> FileStamp {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        FileStamp { len: bytes.len() as u64, mtime, hash: h.finish() }
    }

    /// Same content (a touched file with the same bytes is not a change).
    pub fn same_content(&self, other: &FileStamp) -> bool {
        self.len == other.len && self.hash == other.hash
    }
}

/// [`FileStamp::same_content`] for files that may be missing (two missing files are the same).
pub fn same(a: &Option<FileStamp>, b: &Option<FileStamp>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_content(b),
        _ => false,
    }
}
