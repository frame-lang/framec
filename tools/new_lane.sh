#!/usr/bin/env bash
# new_lane.sh — create an isolated lane worktree for a conversion set, pre-wired to the
# shared build cache (sccache) so it does NOT cold-rebuild the dependency tree. This is
# the second gate-latency lever (see PM-9 in the journal / the plan's gate-process note):
# a fresh /tmp lane starts with an empty target dir; without a shared cache it recompiles
# the whole dependency tree (minutes), even though those deps are identical to every other
# lane. sccache (a shared *compilation* cache, parallel-safe) turns that into cache hits.
#
#   tools/new_lane.sh <branch-name> [base-commit]     # base defaults to HEAD
#
# The lane gets an UNTRACKED .cargo/config.toml (gitignored) routing rustc through sccache
# with incremental off (so sccache can cache the artifacts). Parallel lanes are safe: each
# keeps its own target dir; only the sccache cache is shared. If sccache is absent the lane
# is still created (cold build) — the helper degrades gracefully.
set -euo pipefail

BRANCH="${1:?usage: tools/new_lane.sh <branch-name> [base-commit]}"
BASE="${2:-HEAD}"
LANE="/tmp/frame-lane-${BRANCH##*/}"

[ -e "$LANE" ] && { echo "error: lane $LANE already exists (remove it first)" >&2; exit 1; }
BASE_SHA=$(git rev-parse --short "$BASE")

git worktree add "$LANE" -b "$BRANCH" "$BASE"
mkdir -p "$LANE/.cargo"
if command -v sccache >/dev/null 2>&1; then
  cat > "$LANE/.cargo/config.toml" <<'EOF'
# Untracked (gitignored): route this lane's builds through the shared sccache cache so a
# cold lane reuses the already-compiled dependency tree instead of rebuilding it.
[build]
rustc-wrapper = "sccache"
incremental = false
EOF
  echo "lane ready: $LANE   (branch $BRANCH off $BASE_SHA; sccache shared cache wired)"
else
  echo "lane ready: $LANE   (branch $BRANCH off $BASE_SHA; sccache NOT installed — cold build)"
fi
