#!/usr/bin/env bash
# Standing regen-fixpoint check (plan R8 / DoD C4): every committed .gen.rs must
# equal the current framec-ng's emission for its .frs, byte for byte.
#   tools/regen_check.sh          -> report STALE files, exit 1 if any
#   tools/regen_check.sh --bless  -> rewrite stale .gen.rs from current emission
set -euo pipefail
cd "$(dirname "$0")/.."
FRAMEC_NG=${FRAMEC_NG:-target/debug/framec-ng}
[ -x "$FRAMEC_NG" ] || { echo "error: $FRAMEC_NG not built (cargo build)"; exit 2; }
mode="${1:-check}"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
stale=0; total=0
while IFS= read -r frs; do
  total=$((total+1))
  gen="${frs%.frs}.gen.rs"
  "$FRAMEC_NG" -l rust --emit "$frs" | grep -v '^#!\[allow' > "$tmp"
  if ! cmp -s "$tmp" "$gen"; then
    if [ "$mode" = "--bless" ]; then cp "$tmp" "$gen"; echo "BLESSED  $gen";
    else echo "STALE    $gen"; fi
    stale=$((stale+1))
  fi
done < <(find compiler/src/text/scan -name "*.frs" | sort)
echo "----"
echo "checked $total systems; $stale $( [ "$mode" = "--bless" ] && echo blessed || echo stale )"
[ "$mode" = "--bless" ] || [ "$stale" -eq 0 ]
