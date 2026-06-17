#!/usr/bin/env bash
# build.sh — local mirror of CI for ev (crate: evolving).
#
# Runs the same gate as .github/workflows/ci.yml (format check, clippy as
# errors, the full test suite) and then builds the release binary. Run this
# before you push and CI should never surprise you.
#
# Usage:
#   ./build.sh            fmt --check + clippy + test, then build target/release/ev
#   ./build.sh fix        auto-format first (cargo fmt), then the gate + build
#   ./build.sh install    the gate + build, then `cargo install --path .` (installs `ev`)
set -euo pipefail

cd "$(dirname "$0")"
mode="${1:-build}"

if [ "$mode" = "fix" ]; then
    echo "==> cargo fmt --all"
    cargo fmt --all
else
    echo "==> cargo fmt --all -- --check  (run './build.sh fix' to auto-format)"
    cargo fmt --all -- --check
fi

echo "==> cargo clippy --all-targets -- -D warnings"
cargo clippy --all-targets -- -D warnings

echo "==> cargo test --all"
cargo test --all

echo "==> cargo build --release"
cargo build --release

echo
echo "✓ release binary: $(pwd)/target/release/ev"

if [ "$mode" = "install" ]; then
    echo "==> cargo install --path ."
    cargo install --path .
    echo "✓ installed: $(command -v ev || echo "${CARGO_HOME:-$HOME/.cargo}/bin/ev")"
fi
