#!/usr/bin/env bash
# gate_evidence.sh — collect the standard conversion-gate evidence bundle in ONE pass,
# so the conversion warden JUDGES (one reasoning pass) instead of DRIVING (dozens of
# serial read-run-reason turns). See docs/SYSTEMS_CONVERSION_PLAN.md.
#
# It emits RAW command outputs — real evidence, NOT a verdict — and echoes every command
# so the warden can re-run any single check to double-check. This preserves the campaign's
# verify-don't-trust discipline: the warden still forms its own verdict from the raw
# evidence and adds any delta-specific probe; the script only removes the rote
# re-derivation of the standard predicates (build, warnings, suite, regen fixpoint,
# census-vs-base, diff, working tree, oracle presence, directed pins, new-leaf listing).
#
# Usage — run from the cleanroom repo root, ideally in the lane worktree under gate:
#   tools/gate_evidence.sh <BASE_COMMIT> \
#       [--oracles "name1 name2 ..."]  # grep each hand-oracle fn is still DEFINED (D6 negative)
#       [--tests   "pat1 pat2 ..."]    # run these directed tests, show pass/fail (D4)
#       [--probe   "shell cmd"]        # run a CLI probe end-to-end, show output+exit (repeatable)
#       [--new-fns]                    # list fn defs ADDED in changed hand-src .rs (leaf classification)
#
set -uo pipefail

BASE="${1:?usage: tools/gate_evidence.sh <BASE_COMMIT> [--oracles ..] [--tests ..] [--probe ..] [--new-fns]}"
shift || true
ORACLES=""; TESTS=""; NEWFNS=0; PROBES=()
while [ $# -gt 0 ]; do
  case "$1" in
    --oracles) ORACLES="${2:-}"; shift 2;;
    --tests)   TESTS="${2:-}";   shift 2;;
    --probe)   PROBES+=("${2:-}"); shift 2;;
    --new-fns) NEWFNS=1; shift;;
    *) echo "unknown arg: $1" >&2; exit 2;;
  esac
done

sec(){ printf '\n===== %s =====\n' "$1"; }

HEAD_SHA=$(git rev-parse --short HEAD)
BASE_SHA=$(git rev-parse --short "$BASE")

sec "GATE EVIDENCE  (HEAD=$HEAD_SHA  BASE=$BASE_SHA)"
echo "RAW evidence for the warden to JUDGE — no verdict here. Re-run any line to double-check."

sec "COMMITS UNDER GATE   \$ git log --oneline $BASE_SHA..HEAD"
git log --oneline "$BASE..HEAD"

sec "DIFF STAT            \$ git diff --stat $BASE_SHA..HEAD"
git diff --stat "$BASE..HEAD"
sec "FILE CATEGORIES"
git diff --name-only "$BASE..HEAD" | awk '
  /\.gen\.rs$/{g++; next}
  /\.frs$/{f++; next}
  /\/tests\//{t++; next}
  /\.md$/{m++; next}
  /\.rs$/{s++; next}
  {o++}
  END{printf "  systems(.frs)=%d  generated(.gen.rs)=%d  hand-src(.rs)=%d  tests=%d  docs(.md)=%d  other=%d\n",f,g,s,t,m,o}'

sec "WORKING TREE         \$ git status --porcelain   (expect CLEAN for a committed gate)"
if [ -z "$(git status --porcelain)" ]; then echo "  (clean)"; else git status --porcelain | sed 's/^/  /'; fi

sec "BUILD + WARNINGS     \$ (touch crate root) cargo build --release -p frame-compiler"
# touch the crate root so warnings are re-emitted even on a warm target (mtime only; git-clean).
touch compiler/src/lib.rs 2>/dev/null || touch compiler/src/main.rs 2>/dev/null || true
BUILD_OUT=$(cargo build --release -p frame-compiler 2>&1)
echo "$BUILD_OUT" | tail -1
echo "  warning lines: $(printf '%s\n' "$BUILD_OUT" | grep -c '^warning')"
printf '%s\n' "$BUILD_OUT" | grep '^warning' | sed 's/^/    /' | head -20

sec "FULL SUITE           \$ cargo test -p frame-compiler"
cargo test -p frame-compiler 2>&1 | awk '
  /test result: ok/{p+=$4; f+=$6}
  /FAILED/{print "  *** "$0}
  /error\[/{print "  *** "$0}
  END{printf "  TOTAL passed=%d failed=%d\n",p,f}'

sec "REGEN FIXPOINT       \$ tools/regen_check.sh   (expect: N systems / 0 stale, across a rebuild)"
bash tools/regen_check.sh 2>&1 | tail -2

sec "CENSUS @ HEAD        \$ python3 tools/scan_census.py"
python3 tools/scan_census.py 2>/dev/null | grep -E "RECOGNITION|LOOPS|SYSTEMS"
sec "CENSUS @ BASE        (text-only base tree; NO build) — compare the deltas"
WT=$(mktemp -d 2>/dev/null || echo "/tmp/gate_base_$$")
if git worktree add --detach "$WT" "$BASE" >/dev/null 2>&1; then
  if [ -f "$WT/tools/scan_census.py" ]; then
    ( cd "$WT" && python3 tools/scan_census.py 2>/dev/null | grep -E "RECOGNITION|LOOPS|SYSTEMS" )
  else
    echo "  (scan_census.py absent at base — compare HEAD numbers to the plan's recorded base census)"
  fi
  git worktree remove --force "$WT" >/dev/null 2>&1
else
  echo "  (could not create base worktree — compare HEAD numbers to the plan's recorded base census)"
  rmdir "$WT" 2>/dev/null || true
fi

if [ -n "$ORACLES" ]; then
  sec "ORACLE PRESENCE      (D6 negative: each named hand-oracle fn must still be DEFINED; Phase-2 ≠ C-final)"
  for o in $ORACLES; do
    n=$(grep -rEn "fn $o\b" compiler/src 2>/dev/null | wc -l | tr -d ' ')
    if [ "$n" -ge 1 ]; then echo "  fn $o : $n def(s) PRESENT"; else echo "  fn $o : *** MISSING (0 defs) ***"; fi
  done
fi

if [ -n "$TESTS" ]; then
  sec "DIRECTED TESTS       (D4: flipped pins / teeth assert the FIX)"
  for t in $TESTS; do
    echo "-- \$ cargo test -p frame-compiler $t --"
    cargo test -p frame-compiler "$t" 2>&1 | grep -E "test result|running [0-9]+ test|panicked|$t" | tail -4
  done
fi

if [ "$NEWFNS" = 1 ]; then
  sec "NEW fn DEFINITIONS in changed hand-src (.rs, non-test) — classify each by the paper's carried-register-vs-cursor test"
  for f in $(git diff --name-only "$BASE..HEAD" | grep '\.rs$' | grep -v '/tests/' | grep -v '\.gen\.rs$'); do
    [ -f "$f" ] || continue
    added=$(git diff "$BASE..HEAD" -- "$f" | grep -E '^\+.*\bfn ' | sed -E 's/^\+[[:space:]]*//')
    if [ -n "$added" ]; then echo "  $f:"; printf '%s\n' "$added" | sed 's/^/    /'; fi
  done
fi

if [ ${#PROBES[@]} -gt 0 ]; then
  sec "CLI PROBES           (capability-specific end-to-end reproduction)"
  for p in "${PROBES[@]}"; do
    echo "-- \$ $p --"
    bash -c "$p"; echo "  [exit $?]"
  done
fi

sec "END OF EVIDENCE"
echo "Warden: judge each DoD predicate against the raw evidence above; add any delta-specific"
echo "check the design demands; do NOT re-derive the standard bundle. Cite the lines you used."
