# Materials and decals: spec (backlog B7)

Nov 26, 2026 · status: proposed, not started. Engine read at OpenProcyon `c8b44de` (the editor pins `5e6ad33`, after `c8b44de`; nothing here
needs a newer one). Everything about the engine below is from reading its source and docs; what was not run is marked
`PROVISIONAL`.

## The request

A community member asked (Nov 17) for two things: custom decal textures with a way to configure materials to use them, and the
ability to make new material types. We could not tell from the message whether "decal" means the scars a hit leaves on a hull or
markings painted on (insignia, hull numbers, stripes). The spec covers both and says what each needs.

## What the engine does today

| Piece | Where | State |
| --- | --- | --- |
| Damage decals | `game/src/decals.rs` | A hit that is not a ricochet spawns a quad textured from `decals/<name>.png`. The pool is `data/decals.json` `{name: {size, materials}}`, keyed by the struck section's material and the bullet's `decal` class (Small, Medium, Large); at most 150 per ship; cleared by a full dock repair. Read through `assets_dir()`, which is the overlay, so a mod's files are seen (PROVISIONAL: not run). |
| Painted markings | none | No key on `ship.json`, no GLB convention. |
| Texture by name | `retail_tex.rs`, `docs/modding.md` | `textures/<retail texture name>.png` replaces that texture in every model that uses it. |
| Own textures | GLB | A mod's own model carries its own textures; an unknown `retail:<name>` material stays untextured. |
| Material look | `render/surfaces.rs` | `look.json` `surfaces`: first rule whose name fragment matches gets roughness, metallic, reflectance, bump, cavity, gloss. |
| New material types | `render/rules.rs` | `look.json` `materials.rules[]`: `{match, shader, params, params2, roughness, metallic, reflectance, opaque, shadows}`. `shader` is a WESL file in the mod; one uniform (`RuleUniform`: two `vec4`, clock, sun, ship speed/heading/seed). A new kind of material is a rule plus a shader, no Rust. |
| Schemas | `authoring::schemas()` | none for `look.json` or `decals.json`. |

So "new material types" is half done in the engine and has no editor at all, and decals work only for damage, only from the
retail pool, with no schema.

## Hard-rule check

- `data/decals.json` lists retail decal names. A mod that replaces it restates retail content (rule 1) and loses every other
  decal. The editor must never write a copy of it: see engine need E1.
- The editor lists retail decal and material names from the player's import at runtime, never writes them into a tracked file.
- Starter shaders and sample decal PNGs we ship are written from scratch by us (a plain tint, a stripe, a scorch drawn with
  code). Nothing is derived from a retail shader or picture. `tests/hygiene.rs` keeps watching `assets/`.
- Retail stays intact (B6): everything is written to the mod.

## Packages

Sizes as in the other phase docs (S under half a day, M a day, L two days or more). All of it is a new tab, "Look", beside Ships
and Maps; it opens a mod's `data/render/look.json`, `data/decals.json` and `decals/`, `shaders/`, `textures/`.

### M1. Texture and decal library (M)

A list of the mod's `textures/`, `decals/` and `shaders/` files with a thumbnail, size, and the check the engine already makes
("no model uses this texture", `LayerKind::Texture`). Add by file dialog (copy into the mod; PNG only). "Replace a retail
texture" starts from the retail names found in the import's models, shown as names only, with the retail picture as a
thumbnail from the import (read only, never copied). Reuses `project::layers`, `export`, the ship view's texture-less scene.

### M2. Damage decals (M)

A table editor for `data/decals.json`: name, `size` (class 0 to 2: Small, Medium, Large), `materials` (a multi-pick of the surface materials found in the
import). Preview: the PNG on a checkerboard at the three scar sizes (0.25, 0.5, 1.0 units, `decals.rs` `SIZE`). A "which
retail decals does this material use" lookup, read from the import's table, so an author can see what they replace or add to.
Saved as a keyed diff: needs E1.

### M3. Painted markings (L, needs the engine first)

Per ship: a list of decals `{texture, section or node, pos, rot, size, opacity}` placed on the 3D hull. The editor already has
the ray against the model's triangles (`pick::ray_hit`) and the gizmo, so placing is the Phase 3 gizmo with the click point
as the anchor. Needs E2 (a `ship.json` key and the engine drawing it, reusing the quad that `decals.rs` draws). Shown on the
ship view as a flat quad, as damage decals are drawn. Not started until E2 exists; until then the tab hides this panel.

### M4. Materials: assign (M)

Edits `look.json`: `surfaces.rules` (name fragment to roughness, metallic, reflectance, bump, cavity, gloss) and
`materials.rules`. The list of materials comes from the ships and models of the overlay (glTF material names, read by the same
reader the Phase 3 ship import uses). A rule's `match` is a multi-pick of those names, with a count of how many meshes use each
(so an overly wide fragment shows). Order matters (first rule wins) and is shown as a drag-to-reorder list. Look keys are written
through the engine's schema once it exists (E3); until then through a hand-kept key list in one module, `src/look/doc.rs`
(PROVISIONAL, replaced at E3). `look.json` is replaced, not merged, by a later mod (`docs/modding.md`
troubleshooting), so a mod that edits it starts from the previous mod's rules and says so.

### M5. Materials: new types (L)

A shader file is the new type. In the editor:

1. "New material type" writes `shaders/<name>.wesl` from one of our own starter templates (flat tint, animated stripe,
   emissive pulse, texture blend) with the `RuleUniform` struct and the `frame` import already correct, plus a `materials`
   rule that points at it.
2. A parameters panel gives names to `params` and `params2` (eight numbers). Names, ranges and defaults live in a
   `// @param <slot> <name> <min> <max> <default>` header comment of the shader, so the shader stays the single source and the
   panel is generated from it (PROVISIONAL: the comment convention is ours; the engine would not read it).
3. A shader error panel: the editor runs the game with `--mod` and `--shot` on a one-ship scene and shows the log lines
   (`render profile: ... needs shaders/<name>.wesl`, the WGSL compile error). It never compiles shaders itself (principle 6).
4. A preview ball or the selected ship drawn by the real game (`--shot`), refreshed on save.

The editor does not edit WESL beyond a plain text box with the header lines; modders use their own editor. Hot reload waits for
Phase 6.

## Engine asks

| # | Ask | Why | Status |
| --- | --- | --- | --- |
| E1 | Merge `data/decals.json` by key, like `guns.json` | add or change one decal without restating retail's | not asked |
| E2 | Painted markings: `ship.json` `decals` list drawn through `decals.rs`'s quad | M3 | not asked; design open |
| E3 | `look.json` and `decals.json` schemas in `authoring::schemas()` | forms and checks from the engine (principle 3) | not asked |
| E4 | A texture slot per `materials` rule (`textures: ["textures/x.png"]` bound at a fixed binding) | a new material type is limited to its eight numbers and the mesh's own texture today; blending a modder's own picture needs a slot | not asked; PROVISIONAL that it is wanted |
| E5 | A `--check-mod` finding for a `materials` rule whose `match` names no material in the overlay and for an unreferenced decal | M4 and M2 feedback | not asked |

M1, M2 (once E1 lands, or as a full restatement of a mod's own new decals only, which needs no merge) and M4 can start now. M5
can start now with E4 left out. M3 waits for E2.

## Gate

A tester, with no JSON or shader file opened by hand: (1) adds their own scar PNG for one hull material and sees it appear on a
hit in play-test; (2) makes a new material type from a template, tunes two named parameters, and sees it on a ship in the
preview; (3) both pass `--check-mod` and neither mod holds a retail file.

## Open questions

- Which "decal" does the requester mean (scars, markings, or both)? Answer sets whether E2 and M3 are in the first cut.
- Is a texture slot per rule (E4) wanted, or are eight numbers plus the mesh's texture enough for the first types?
- Does the engine's shader language stay WESL on Bevy 0.20 across upgrades, since mods ship `.wesl` files? Ask the maintainer
  whether shader files are part of the format freeze.
- Should the shader templates live in the editor repository or the engine's `mods/` examples (the engine already has
  `mods/remaster/shaders/` as a working set)?
