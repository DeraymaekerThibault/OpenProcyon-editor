# OpenProcyon Editor

The mod editor for [OpenProcyon](https://github.com/DeraymaekerThibault/OpenProcyon), the open-source engine for
*Treasure Planet: Battle at Procyon*: ships, weapons, maps and missions, edited with forms and saved as ordinary mod files.
It is at the Phase 0 spike (`docs/roadmap.md`, `docs/spike.md`); there are no editing features yet.

**It contains no game content.** It reads only the assets you imported from your own copy of the game with the engine's
importer ([instructions](https://github.com/DeraymaekerThibault/OpenProcyon#getting-started-from-source)). Without them it still
starts and says where it looked.

Licence: GPL-3.0-or-later (`LICENSE`), because it links the GPL engine library. Mods you make with it are your own.

## Build

Requirements: Rust 1.97.1 or newer and, on Linux, the usual Bevy system packages (ALSA, udev, Wayland/X11, Vulkan).

```sh
cargo run              # open the editor
tools/check.sh         # format, lints and tests
```

Set `PROCYON_ASSETS=/path/to/assets` (for example in an untracked `.env`) if your import is not in the engine's default folder.
