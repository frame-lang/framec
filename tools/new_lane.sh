#!/usr/bin/env bash
# new_lane.sh — create an isolated lane worktree for a conversion set. This standardizes lane
# creation so parallel, graph-disjoint sets each get a clean checkout that gates independently
# (the third speed lever; see the plan's "Speed levers" note / PM-9).
#
#   tools/new_lane.sh <branch-name> [base-commit]     # base defaults to HEAD
#
# It gives a consistent /tmp/frame-lane-<name> path, guards against clobbering an existing lane,
# and echoes the base SHA. NO build-cache wrapper is wired: `frame-compiler` has zero external
# dependencies (`cargo tree` confirms), so a fresh lane compiles exactly one crate — a true cold
# build from an empty target is ~2s. A shared compilation cache (sccache) was measured and rejected
# (PM-10): 0 cache hits over an edit-rebuild loop, and its required `incremental = false` made the
# loop ~7% slower by disabling intra-crate incremental compilation. Plain cargo is the fast path.
set -euo pipefail

BRANCH="${1:?usage: tools/new_lane.sh <branch-name> [base-commit]}"
BASE="${2:-HEAD}"
LANE="/tmp/frame-lane-${BRANCH##*/}"

[ -e "$LANE" ] && { echo "error: lane $LANE already exists (remove it first)" >&2; exit 1; }
BASE_SHA=$(git rev-parse --short "$BASE")

git worktree add "$LANE" -b "$BRANCH" "$BASE"
echo "lane ready: $LANE   (branch $BRANCH off $BASE_SHA)"
