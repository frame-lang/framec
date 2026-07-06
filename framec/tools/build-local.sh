#!/usr/bin/env bash
#
# Build framec as a LOCAL (unofficial) dev build and install it to
# ~/.frame/local/bin/framec.
#
# Build policy:
#   - Official releases  -> ~/.frame/bin/framec        (see build-official.sh),
#                           versioned with the 3-number workspace semver (4.6.1).
#   - Local dev builds   -> ~/.frame/local/bin/framec  (this script),
#                           versioned with a 4-number string <last-release>.<seq>,
#                           e.g. 4.6.0.3 — the root is the last OFFICIAL release
#                           tag (the version we're evolving FROM) and the 4th
#                           number is a monotonic local build sequence that
#                           resets to 1 whenever a new release tag lands.
#
# The 4th-number scheme means local builds sort BETWEEN the last release and the
# next, so `framec --version` never confuses a local build with a release.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)" # framec/tools -> framec -> workspace root
cd "$repo_root"

# Root = last official release tag reachable from HEAD, minus the leading 'v'.
root="$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null | sed 's/^v//' || true)"
if [ -z "$root" ]; then
    echo "error: no 'v*' release tag found — cannot derive the local version root" >&2
    exit 1
fi

local_dir="$HOME/.frame/local"
seq_file="$local_dir/.build_seq"
mkdir -p "$local_dir/bin"

# Per-root sequence: continue counting within the same root, reset to 1 when the
# last-release root changes (i.e. a new official release was tagged).
saved_root=""
saved_seq=0
if [ -f "$seq_file" ]; then read -r saved_root saved_seq <"$seq_file" || true; fi
if [ "$saved_root" = "$root" ]; then
    seq=$((saved_seq + 1))
else
    seq=1
fi
echo "$root $seq" >"$seq_file"

version="$root.$seq"
echo "==> building local framec $version"
FRAME_LOCAL_VERSION="$version" cargo build --release
cp "target/release/framec" "$local_dir/bin/framec"
echo "==> installed framec $version -> $local_dir/bin/framec"
"$local_dir/bin/framec" --version
