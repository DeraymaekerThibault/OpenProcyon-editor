//! The layer view: what this mod does to the player's import, file by file, as the engine's [`ModReport`] says it, plus the documents
//! that have unsaved edits.

use openprocyon::authoring::ModEntry;

/// How a file of the mod combines with the one below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// Edited in the editor, not written yet.
    Unsaved,
    /// The engine refused the file; the note says why.
    Skipped,
    /// Replaces the file below it whole.
    Overridden,
    /// Keys merged into the file below it.
    Merged,
    /// Laid over the file named by its `extends`.
    Extends,
    /// A file the import does not have.
    Added,
    /// Replaces a retail texture by name.
    Texture,
}

impl LayerKind {
    /// The word shown beside the file.
    pub fn label(self) -> &'static str {
        match self {
            LayerKind::Unsaved => "unsaved",
            LayerKind::Skipped => "skipped",
            LayerKind::Overridden => "overridden",
            LayerKind::Merged => "merged",
            LayerKind::Extends => "extends",
            LayerKind::Added => "added",
            LayerKind::Texture => "texture",
        }
    }
}

/// One line of the layer view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerRow {
    pub kind: LayerKind,
    pub file: String,
    /// Why a file was skipped, or a hint (a texture no model uses); empty otherwise.
    pub note: String,
}

/// The files in the mod folder `dir` (relative paths with `/`), without dot entries and without following symlinks; the layer list
/// is rebuilt when this changes. Capped, so a huge folder cannot stall the once-a-second poll.
pub fn listing(dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    fn walk(dir: &std::path::Path, prefix: &str, out: &mut std::collections::BTreeSet<String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let (Some(kind), true) = (e.file_type().ok(), !name.starts_with('.') && out.len() < 50_000) else { continue };
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if kind.is_dir() {
                walk(&e.path(), &rel, out);
            } else if kind.is_file() {
                out.insert(rel);
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(dir, "", &mut out);
    out
}

/// The rows for one mod: unsaved documents, skipped files (with the reason), then overridden, merged, extends, added and texture
/// files. `entry` is `None` before the overlay was built.
pub fn rows(entry: Option<&ModEntry>, dirty: &[String]) -> Vec<LayerRow> {
    let row = |kind, file: &str, note: &str| LayerRow { kind, file: file.to_string(), note: note.to_string() };
    let mut out: Vec<LayerRow> = dirty.iter().map(|f| row(LayerKind::Unsaved, f, "")).collect();
    let Some(e) = entry else { return out };
    out.extend(e.skipped.iter().map(|(f, why)| row(LayerKind::Skipped, f, why)));
    for (kind, files) in [(LayerKind::Overridden, &e.overrides), (LayerKind::Merged, &e.merged), (LayerKind::Extends, &e.extends), (LayerKind::Added, &e.added)] {
        out.extend(files.iter().map(|f| row(kind, f, "")));
    }
    out.extend(e.textures.iter().map(|f| {
        let unused = e.unused_textures.contains(f);
        row(LayerKind::Texture, f, if unused { "no model uses this texture" } else { "" })
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_entry_becomes_rows_in_a_fixed_order() {
        let entry = ModEntry {
            name: "m".into(),
            overrides: vec!["data/render/look.json".into()],
            merged: vec!["data/weapons/guns.json".into()],
            extends: vec!["ships/A/ship.json".into(), "ships/B/ship.json".into()],
            added: vec!["ships/New/ship.json".into()],
            textures: vec!["textures/Hull.png".into(), "textures/Typo.png".into()],
            unused_textures: vec!["textures/Typo.png".into()],
            skipped: vec![("data/x.json".into(), "extends unknown 'Q'".into())],
            ..Default::default()
        };
        let r = rows(Some(&entry), &["ships/A/ship.json".to_string()]);
        let kinds: Vec<_> = r.iter().map(|r| (r.kind, r.file.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (LayerKind::Unsaved, "ships/A/ship.json"),
                (LayerKind::Skipped, "data/x.json"),
                (LayerKind::Overridden, "data/render/look.json"),
                (LayerKind::Merged, "data/weapons/guns.json"),
                (LayerKind::Extends, "ships/A/ship.json"),
                (LayerKind::Extends, "ships/B/ship.json"),
                (LayerKind::Added, "ships/New/ship.json"),
                (LayerKind::Texture, "textures/Hull.png"),
                (LayerKind::Texture, "textures/Typo.png"),
            ]
        );
        assert_eq!(r[1].note, "extends unknown 'Q'");
        assert_eq!((r[7].note.as_str(), r[8].note.as_str()), ("", "no model uses this texture"));
    }

    #[test]
    fn no_entry_shows_only_the_unsaved_documents() {
        assert!(rows(None, &[]).is_empty());
        assert_eq!(rows(None, &["mod.json".to_string()]).len(), 1);
    }
}
