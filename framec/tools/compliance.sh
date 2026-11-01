#!/usr/bin/env bash
# COMPLIANCE — does the emitted code actually COMPILE?
#
# The snapshot tests compare TEXT. They have never invoked a compiler. So output that
# does not compile has been shipping, snapshotted and blessed — for years (#232).
#
# This runs each corpus fixture through framec and then through the TARGET'S OWN
# compiler, at syntax-check level. It is the gate that was missing.
#
#   usage:  framec/tools/compliance.sh [target ...]
#
# A test that only compares text will bless anything.

set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
FRAMEC="${FRAMEC_BIN:-$HOME/.frame/local/bin/framec}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT

# target  fixture-dir  ext  syntax-check-command ({} = file)
TARGETS=(
  "java|java|java|javac -d $W {}"
  "python|python_3|py|python3 -m py_compile {}"
  "javascript|javascript|mjs|node --check {}"
  "typescript|typescript|ts|npx --no-install tsc --noEmit --skipLibCheck {}"
  "c|c|c|gcc -fsyntax-only -std=c11 {}"
  "cpp|cpp|cpp|g++ -fsyntax-only -std=c++20 {}"
  "rust|rust|rs|rustc --edition 2021 --crate-type lib --emit=metadata --out-dir OUTDIR {}"
  "lua|lua|lua|luac -p {}"
  "ruby|ruby|rb|ruby -c {}"
  "php|php|php|php -l {}"
  "swift|swift|swift|swiftc -parse {}"
  "dart|dart|dart|dart analyze --no-fatal-warnings {}"
  "go|go|go|gofmt -e {}"
  "kotlin|kotlin|kt|kotlinc {} -d $W"
  "csharp|csharp|cs|dotnet build"   # needs a project; reported as SKIP
)

want=("$@")
grand_ok=0; grand_n=0
printf "\n  %-12s %-8s %s\n" "TARGET" "SCORE" "FAILING FIXTURES"
printf "  %s\n" "-------------------------------------------------------------------------"

for row in "${TARGETS[@]}"; do
  IFS='|' read -r name dir ext cmd <<<"$row"
  if [ ${#want[@]} -gt 0 ]; then
    case " ${want[*]} " in *" $name "*) ;; *) continue;; esac
  fi
  d="framec/tests/fixtures/$dir"
  [ -d "$d" ] || continue

  # No usable syntax check for this target in this environment.
  if [ "$name" = "csharp" ]; then
    printf "  %-12s %-8s %s\n" "$name" "SKIP" "no standalone syntax check (needs a project)"
    continue
  fi
  tool="${cmd%% *}"
  command -v "$tool" >/dev/null 2>&1 || {
    printf "  %-12s %-8s %s\n" "$name" "SKIP" "$tool not installed — verifies NOTHING"
    continue
  }

  ok=0; n=0; failing=""
  for f in "$d"/*.frm; do
    [ -f "$f" ] || continue
    n=$((n+1)); grand_n=$((grand_n+1))
    base="$(basename "$f" .frm)"
    out="$W/$base.$ext"
    if ! "$FRAMEC" -l "$name" "$f" > "$out" 2>/dev/null; then
      failing="$failing $base(emit)"; continue
    fi
    # Java/Kotlin need the file named after the public class.
    if [ "$name" = "java" ] || [ "$name" = "kotlin" ]; then
      # Java/Kotlin: the file must be named after the PUBLIC class — not the first
      # class in the file, which is the FrameEvent helper.
      cls=$(grep -oE '^public class [A-Za-z_0-9]+' "$out" | head -1 | awk '{print $NF}')
      [ -z "$cls" ] && cls=$(grep -oE '^class [A-Za-z_0-9]+' "$out" | tail -1 | awk '{print $NF}')
      [ -n "$cls" ] && { mv "$out" "$W/$cls.$ext"; out="$W/$cls.$ext"; }
    fi
    run="${cmd//\{\}/$out}"; run="${run//OUTDIR/$W}"
    if $run >/dev/null 2>&1; then
      ok=$((ok+1)); grand_ok=$((grand_ok+1))
    else
      failing="$failing $base"
    fi
  done
  [ $n -eq 0 ] && continue
  mark=""; [ $ok -eq $n ] && mark=" ✅"
  printf "  %-12s %-8s%s%s\n" "$name" "$ok/$n" "$mark" "$failing"
done

printf "  %s\n" "-------------------------------------------------------------------------"
printf "  TOTAL        %s/%s\n\n" "$grand_ok" "$grand_n"
[ "$grand_ok" -eq "$grand_n" ]
