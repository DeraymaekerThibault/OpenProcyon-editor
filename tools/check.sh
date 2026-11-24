#!/usr/bin/env bash
# The validation gate: format, lints, tests (hygiene included). Needs no game assets.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
echo "GATE OK"
