#!/usr/bin/env python3
"""scan_census — the mechanical hand-recognition census for the systems-conversion campaign.

The campaign's Definition of Done is falsifiable only if "how much hand-written recognition
remains" is a number a script can print, not a judgement. This is that script. It counts, in
`compiler/src/text/scan/`, the hand-written recognition surface the conversion must drive to
zero, and the systems that are replacing it. It is a RATCHET: the hand numbers must only go down.

What it counts (excludes generated `*.gen.rs` and `#[cfg(test)]`/`tests/`):
  - HAND_LEXER_RECOGNITION — the hand string/comment recognizer that OpaqueScan replaces:
    definitions of comment_at/literal_at/quoted/triple_quoted/rust_raw/block_comment/hole_at,
    plus every `.comment_at(` / `.literal_at(` call site. Campaign DoD C2 target: 0.
  - HAND_SCAN_LOOPS — `while <ident> < ...` byte-loops in scan/*.rs (a proxy for hand walks).
    Campaign DoD C1 target: only Category-A leaves + allowlisted glue remain; ratchet down.
  - SYSTEMS — the number of `.frs` @@systems under scan/ (should grow).

Usage:
  python3 tools/scan_census.py                # print the census
  python3 tools/scan_census.py --gate         # exit 1 if HAND_LEXER_RECOGNITION > 0 (campaign C2)
Run from the cleanroom repo root.
"""
import re
import sys
from pathlib import Path

SCAN = Path("compiler/src/text/scan")

LEXER_FN_DEFS = re.compile(
    r"\bfn\s+(comment_at|literal_at|quoted|triple_quoted|rust_raw|block_comment|hole_at)\b"
)
LEXER_CALLS = re.compile(r"\.(comment_at|literal_at)\s*\(")
HAND_LOOP = re.compile(r"\bwhile\s+[A-Za-z_][A-Za-z0-9_]*\s*<")


def rs_files():
    for p in sorted(SCAN.rglob("*.rs")):
        if p.name.endswith(".gen.rs"):
            continue
        yield p


def strip_tests(text: str) -> str:
    # Drop a trailing `#[cfg(test)] mod tests { ... }` (best-effort: from the marker to EOF).
    i = text.find("#[cfg(test)]")
    return text[:i] if i != -1 else text


def main() -> int:
    if not SCAN.is_dir():
        print(f"error: run from the cleanroom repo root (no {SCAN})", file=sys.stderr)
        return 2

    lexer_defs = lexer_calls = hand_loops = 0
    per_file = []
    for p in rs_files():
        body = strip_tests(p.read_text(encoding="utf-8", errors="replace"))
        d = len(LEXER_FN_DEFS.findall(body))
        c = len(LEXER_CALLS.findall(body))
        w = len(HAND_LOOP.findall(body))
        lexer_defs += d
        lexer_calls += c
        hand_loops += w
        if d or c or w:
            per_file.append((str(p.relative_to(SCAN)), d, c, w))

    systems = len(list(SCAN.rglob("*.frs")))
    hand_lexer = lexer_defs + lexer_calls

    print("== scan_census ==")
    print(f"  {'file':<28} {'lexer_defs':>10} {'lexer_calls':>11} {'hand_loops':>10}")
    for name, d, c, w in per_file:
        print(f"  {name:<28} {d:>10} {c:>11} {w:>10}")
    print(f"  {'TOTAL':<28} {lexer_defs:>10} {lexer_calls:>11} {hand_loops:>10}")
    print()
    print(f"HAND_LEXER_RECOGNITION = {hand_lexer}   (C2 target: 0)")
    print(f"HAND_SCAN_LOOPS        = {hand_loops}   (C1: ratchet down)")
    print(f"SYSTEMS (.frs)         = {systems}")

    if "--gate" in sys.argv and hand_lexer > 0:
        print(f"\nGATE FAIL: HAND_LEXER_RECOGNITION = {hand_lexer} (campaign DoD C2 requires 0)")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
