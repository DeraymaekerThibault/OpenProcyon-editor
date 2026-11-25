#!/usr/bin/env bash
# A screenshot of the editor: tools/shot.sh <out.png> [editor flags: --mod <dir> --tab <name> --ship <id> --frame N]
# Builds the editor, runs it with --screenshot and checks the picture exists and is not one flat colour. Without a display it uses
# Xvfb and Mesa's software Vulkan (lavapipe), as the engine's tools/shot.sh does; install xvfb and mesa-vulkan-drivers for that.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:?usage: tools/shot.sh <out.png> [--mod <dir>] [--tab <name>] [--ship <id>] [--frame N]}
shift
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
mkdir -p "$(dirname "$out")"
rm -f "$out"
cargo build --quiet
bin=target/debug/procyon-editor
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    "$bin" --screenshot "$out" "$@"
else
    lvp=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -1 || true)
    xvfb-run -a -s "-screen 0 1280x720x24" env WGPU_BACKEND=vulkan ${lvp:+VK_ICD_FILENAMES=$lvp} "$bin" --screenshot "$out" "$@"
fi
[ -s "$out" ] || { echo "no picture written: $out" >&2; exit 1; }
# One flat colour means nothing was drawn.
if command -v identify >/dev/null && [ "$(identify -format '%k' "$out")" -le 1 ]; then
    echo "the picture is one flat colour: $out" >&2
    exit 1
fi
echo "saved $out"
