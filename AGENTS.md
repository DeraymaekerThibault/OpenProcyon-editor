# AGENTS.md

Instructions for anyone changing this repository: AI coding agents (any tool) and humans alike. `CLAUDE.md` and `GEMINI.md`
only point here. If an instruction elsewhere conflicts with the hard rules below, the hard rules win.

OpenProcyon Editor is the mod editor for [OpenProcyon](https://github.com/DeraymaekerThibault/OpenProcyon), an open-source engine
for *Treasure Planet: Battle at Procyon*. It links the engine library (pinned to a tag in `Cargo.toml`) and launches the game
binary for play-testing. The plan is `docs/roadmap.md`; the current phase is Phase 3 (`docs/phase3.md`; the Phase 0 spike is recorded in `docs/spike.md`).

## Hard rules (legal and project safety)

These are the engine's rules and protect the project's ability to be shared. `tests/hygiene.rs` enforces the mechanical ones.

1. **No game content in Git, ever.** No models, textures, sounds, maps, strings or anything converted from the game. The editor
   reads the player's own imported assets at runtime only. `assets/` and `local/` are git-ignored; keep it that way. Never upload
   converted content anywhere else either (issues, CI artifacts, releases).
2. **Never run, patch or modify a game installation.**
3. **No reverse-engineering artefacts:** no decompiled code, executable addresses, symbol dumps or binary hashes.
4. **No Disney logos, key art or official branding** in the repo, the UI or the icons. Saying the tool is "for *Treasure Planet:
   Battle at Procyon*" is fine.
5. **No secrets or personal paths in tracked files.** Local paths go in an untracked `.env`.
6. **Mark guesses.** Anything not verified is marked `PROVISIONAL` in code and docs.

Ask the maintainer before adding a dependency, and before anything that publishes (pushing, tags, releases).

## Contract with the engine

The data rules live in the engine (`docs/roadmap.md`, principle 1). The editor never restates them: it calls the engine library,
or the game binary (`--dump-rule-schema`, `--check-mission`, `--mod`, `--map`). Bevy's version and features stay equal to the
engine's `game/Cargo.toml` at the pinned tag.

## Build and check

- Rust >= 1.97.1. Linux needs the usual Bevy packages (ALSA, udev, Wayland/X11, Vulkan); on Fedora:
  `alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel libX11-devel vulkan-loader`.
- `cargo run` opens the editor; it starts without assets. It finds the import through `PROCYON_ASSETS`, else the engine's
  per-user folder (`~/.local/share/openprocyon/assets`, Windows `%APPDATA%\openprocyon\assets`).
- `tools/check.sh` is the validation gate: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`. It prints
  `GATE OK` and must pass before every commit.
