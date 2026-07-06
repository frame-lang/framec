#!/usr/bin/env bash
#
# Build framec as an OFFICIAL release build and install it to ~/.frame/bin/framec.
# The official binary reports the 3-number workspace semver from Cargo.toml
# (e.g. 4.6.1) — no FRAME_LOCAL_VERSION override, so `framec --version` shows the
# release version, not a 4-number local build string.
#
# For unofficial local dev builds, use build-local.sh (installs to
# ~/.frame/local/bin with a 4-number <last-release>.<seq> version).
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)" # framec/tools -> framec -> workspace root
cd "$repo_root"

# Ensure no stray local-version override leaks in from the environment.
unset FRAME_LOCAL_VERSION

official_dir="$HOME/.frame/bin"
mkdir -p "$official_dir"

echo "==> building official framec"
cargo build --release
cp "target/release/framec" "$official_dir/framec"
echo "==> installed -> $official_dir/framec"
"$official_dir/framec" --version
