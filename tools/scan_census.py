#!/usr/bin/env python3
"""scan_census — the mechanical hand-recognition census for the systems-conversion campaign.

The campaign's Definition of Done is falsifiable only if "how much hand-written recognition
remains" is a number a script can print, not a judgement. This is that script. It counts, in
`compiler/src/text/scan/`, the hand-written recognition surface the conversion must drive to
zero, and the systems that are replacing it. It is a RATCHET: the hand numbers must only go down.

What it counts (excludes generated `*.gen.rs` and `#[cfg(test)]`/`tests/`):
  - HAND_LEXER_RECOGNITION — the hand string/comment recognizer that OpaqueScan replaces:
    definitions of comment_at/literal_at/quoted/triple_quoted/rust_raw/block_comment/hole_at,
    plus every `.comment_at(` / `.literal_at(` call site. Split into two buckets:
      * PRODUCTION — recognition on a live compile path. Campaign DoD C2 target: 0.
      * ORACLE — recognition inside a `*_hand`/`hand_*` DIFFERENTIAL oracle (`#[doc(hidden)]`,
        test-only). These are the independent hand check a conversion is proven against; they
        are TRANSIENT scaffolding, deleted at C-final once each parity is locked (DoD D6). They
        legitimately use the hand lexer, so counting them as production would make writing a
        differential oracle look like a regression. Tracked separately; must ALSO reach 0 by
        campaign end, so oracles cannot hide forever.
    Only the PRODUCTION bucket is the C2 ratchet.
  - HAND_SCAN_LOOPS — `while <ident> < ...` byte-loops in scan/*.rs (a proxy for hand walks).
    Split production vs oracle on the SAME rule as HAND_LEXER_RECOGNITION (a loop inside a
    `*_hand`/`hand_*` differential oracle is transient scaffolding, deleted at C-final): retiring
    a production walk into a system must not read as no-progress just because its `*_hand` oracle
    keeps a copy of the loop. Campaign DoD C1 target (production): only Category-A leaves +
    allowlisted glue remain; ratchet down. Oracle bucket → 0 by C-final.
  - SYSTEMS — the number of `.frs` @@systems under scan/ (should grow).

Usage:
  python3 tools/scan_census.py                # print the census
  python3 tools/scan_census.py --gate         # exit 1 if PRODUCTION HAND_LEXER_RECOGNITION > 0
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
# A DIFFERENTIAL oracle by naming convention: `fn <x>_hand` or `fn hand_<x>`. Its body is
# hand recognition ON PURPOSE (the independent check) and is deleted at C-final (D6).
ORACLE_FN = re.compile(r"\bfn\s+(?:[A-Za-z0-9_]*_hand|hand_[A-Za-z0-9_]*)\b")


def rs_files():
    for p in sorted(SCAN.rglob("*.rs")):
        if p.name.endswith(".gen.rs"):
            continue
        yield p


def strip_tests(text: str) -> str:
    # Drop a trailing `#[cfg(test)] mod tests { ... }` (best-effort: from the marker to EOF).
    i = text.find("#[cfg(test)]")
    return text[:i] if i != -1 else text


def oracle_spans(text: str):
    """(start, end) byte spans of `*_hand`/`hand_*` oracle-fn bodies (best-effort brace match,
    like strip_tests — oracle bodies are plain Rust with no stray braces in strings)."""
    spans = []
    for m in ORACLE_FN.finditer(text):
        b = text.find("{", m.end())
        if b == -1:
            continue
        depth, i = 0, b
        while i < len(text):
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    spans.append((m.start(), i))
                    break
            i += 1
    return spans


def in_any(pos: int, spans) -> bool:
    return any(s <= pos <= e for s, e in spans)


def main() -> int:
    if not SCAN.is_dir():
        print(f"error: run from the cleanroom repo root (no {SCAN})", file=sys.stderr)
        return 2

    lexer_defs = prod_calls = oracle_calls = prod_loops = oracle_loops = 0
    per_file = []
    for p in rs_files():
        body = strip_tests(p.read_text(encoding="utf-8", errors="replace"))
        spans = oracle_spans(body)
        d = len(LEXER_FN_DEFS.findall(body))
        pc = sum(1 for m in LEXER_CALLS.finditer(body) if not in_any(m.start(), spans))
        oc = sum(1 for m in LEXER_CALLS.finditer(body) if in_any(m.start(), spans))
        pw = sum(1 for m in HAND_LOOP.finditer(body) if not in_any(m.start(), spans))
        ow = sum(1 for m in HAND_LOOP.finditer(body) if in_any(m.start(), spans))
        lexer_defs += d
        prod_calls += pc
        oracle_calls += oc
        prod_loops += pw
        oracle_loops += ow
        if d or pc or oc or pw or ow:
            per_file.append((str(p.relative_to(SCAN)), d, pc, oc, pw, ow))

    systems = len(list(SCAN.rglob("*.frs")))
    # Definitions live in lex.rs (the recognizer itself) — a production surface until Item 4
    # deletes the hand Lexer; the C2 ratchet is defs + production call sites.
    prod_hand_lexer = lexer_defs + prod_calls

    print("== scan_census ==")
    print(
        f"  {'file':<28} {'lex_defs':>8} {'prod_call':>9} "
        f"{'orc_call':>8} {'prod_loop':>9} {'orc_loop':>8}"
    )
    for name, d, pc, oc, pw, ow in per_file:
        print(f"  {name:<28} {d:>8} {pc:>9} {oc:>8} {pw:>9} {ow:>8}")
    print(
        f"  {'TOTAL':<28} {lexer_defs:>8} {prod_calls:>9} "
        f"{oracle_calls:>8} {prod_loops:>9} {oracle_loops:>8}"
    )
    print()
    print(f"HAND_LEXER_RECOGNITION (production) = {prod_hand_lexer}   (C2 target: 0)")
    print(f"HAND_LEXER_RECOGNITION (oracle)     = {oracle_calls}   (transient; → 0 by C-final)")
    print(f"HAND_SCAN_LOOPS (production)        = {prod_loops}   (C1: ratchet down)")
    print(f"HAND_SCAN_LOOPS (oracle)            = {oracle_loops}   (transient; → 0 by C-final)")
    print(f"SYSTEMS (.frs)                      = {systems}")

    if "--gate" in sys.argv and prod_hand_lexer > 0:
        print(
            f"\nGATE FAIL: production HAND_LEXER_RECOGNITION = {prod_hand_lexer} "
            f"(campaign DoD C2 requires 0)"
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
