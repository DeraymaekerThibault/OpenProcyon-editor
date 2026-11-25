//! Mines of a map (docs/phase4.md P6, work item T8): which mine types to offer and placing one. Moving a mine is any other entry's
//! move ([`super::project::change_entries`]) and deleting one is [`super::project::delete_entry`] (both keyed by `id` on a map that
//! extends another); this module adds only what is mine-specific. Names no map key: that is [`super::doc`].
//!
//! What the engine does with them (`game/src/scenario/spawn.rs` `spawn_mines`): every `mines[]` entry with a string `type` and an
//! integer `id` becomes a mine of that bullet resting at `pos` until a script hands it to a player; an entry missing either is
//! skipped without a message, so a mine this tool writes always has both. The `type` is a bullet of `data/weapons/bullets.json`.

use serde_json::Value;

use super::doc::{self, Array};
use super::project as maps;
use crate::project::Project;
use openprocyon::authoring::read_merged;

/// The bullets of mines start with this (docs/phase4.md P6: `Bullet_StarMine_*`).
pub const MINE_PREFIX: &str = "Bullet_StarMine";

/// The bullet the engine's `Place Mines` action uses when it names none (`rules/eval/schema.rs`).
pub const DEFAULT_TYPE: &str = "Bullet_StarMine_Med";

/// The merged weapon table, where the bullets are.
const BULLETS: &str = "data/weapons/bullets.json";

/// The mine types to offer for the map `id`: the `Bullet_StarMine_*` bullets of the game's `bullets.json` as the overlay has it
/// (import and mod), plus the types the map's own mines already use, sorted. With no bullet table (no import) the engine's default is
/// offered alone (PROVISIONAL: the list is then a guess).
pub fn types(project: &Project, id: &str) -> Vec<String> {
    let mut out: Vec<String> = match read_merged(&project.root, BULLETS) {
        Ok(Value::Object(o)) => o.keys().filter(|k| k.starts_with(MINE_PREFIX)).cloned().collect(),
        _ => vec![],
    };
    if let Some(map) = maps::merged_view(project, id) {
        out.extend(doc::mines(map).iter().filter_map(|m| doc::kind(m).map(str::to_string)));
    }
    if out.is_empty() {
        out.push(DEFAULT_TYPE.to_string());
    }
    out.sort();
    out.dedup();
    out
}

/// Whether `ty` is a mine bullet the bullet table knows (or, with no table, the default).
pub fn is_known(project: &Project, id: &str, ty: &str) -> bool {
    types(project, id).iter().any(|t| t == ty)
}

/// Checks a type someone typed: not empty, no spaces. A name that is not a known mine type is allowed (PROVISIONAL: a mod may add
/// bullets), and the tab says so.
pub fn check_type(ty: &str) -> Result<(), String> {
    if ty.trim().is_empty() {
        return Err("a mine needs a type".into());
    }
    if ty.chars().any(char::is_whitespace) {
        return Err(format!("'{ty}': a bullet name has no spaces"));
    }
    Ok(())
}

/// Places a mine of `ty` at (x, y) as one undo step and returns its index in `mines`. The id is the next free one across objects,
/// ships and mines. On a map that extends another this is a keyed add, refused when the parent's list cannot take it.
pub fn place(project: &mut Project, id: &str, ty: &str, at: [f32; 2]) -> Result<usize, String> {
    check_type(ty)?;
    let at = [super::view2d::short_f64(f64::from(at[0])), super::view2d::short_f64(f64::from(at[1]))];
    maps::add_entry(project, id, Array::Mines, doc::new_mine(ty, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_types_are_checked_for_shape_only() {
        assert!(check_type("Bullet_StarMine_Big").is_ok());
        assert!(check_type("  ").is_err());
        assert!(check_type("Bullet StarMine").is_err());
    }
}
