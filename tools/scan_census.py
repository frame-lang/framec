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
      * ORACLE — recognition inside a DIFFERENTIAL oracle: the independent hand check a
        conversion is proven against. TRANSIENT scaffolding, deleted at C-final once each parity
        is locked (DoD D6). Counting an oracle as production would make writing a differential
        oracle look like a regression. Tracked separately; must ALSO reach 0 by campaign end, so
        oracles cannot hide forever.
    Only the PRODUCTION bucket is the C2 ratchet.

    An oracle FUNCTION is detected two ways (PM-4 hardening, 2026-07-19). (1) By NAME:
    `fn <x>_hand` / `fn hand_<x>`, the campaign convention. (2) By REACHABILITY: a consumer whose
    every call site (across compiler/src + compiler/tests) is a test or is itself inside an oracle
    function — a differential oracle that simply was not given the `_hand` name. This is what
    catches `sections.rs::section_keyword_starts` (the SectionScan oracle; production is
    `section_scan::keyword_starts`; sole caller is a test), which the name-only rule mislabelled
    as production and which then steered a warden verdict (journal PM-4). The reachability rule
    does NOT touch the seven hand-recognizer METHODS themselves (comment_at/literal_at/… — see
    RECOGNIZER_METHODS): they are the deletion target and stay production-counted until the hand
    Lexer is physically removed at C-final, even after all their callers are oracles. Whether a
    recognizer method reached only from oracles should stop counting as production is a
    metric-SHAPE question, deliberately left to the owner (do not silently reclassify it here).
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
SRC = Path("compiler/src")
TESTS = Path("compiler/tests")

LEXER_FN_DEFS = re.compile(
    r"\bfn\s+(comment_at|literal_at|quoted|triple_quoted|rust_raw|block_comment|hole_at)\b"
)
LEXER_CALLS = re.compile(r"\.(comment_at|literal_at)\s*\(")
HAND_LOOP = re.compile(r"\bwhile\s+[A-Za-z_][A-Za-z0-9_]*\s*<")
# A DIFFERENTIAL oracle by naming convention: `fn <x>_hand` or `fn hand_<x>`.
ORACLE_NAME = re.compile(r"(?:[A-Za-z0-9_]*_hand|hand_[A-Za-z0-9_]*)\Z")
FN_DEF = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\b")

# The hand recognizer methods themselves — the C-final deletion target. NEVER reclassified by
# reachability: they stay production-counted until the hand Lexer is physically removed, even
# once all their callers are oracles. (That reclassification is a metric-SHAPE call for the owner.)
RECOGNIZER_METHODS = frozenset(
    {"comment_at", "literal_at", "quoted", "triple_quoted", "rust_raw", "block_comment", "hole_at"}
)


def rs_files():
    for p in sorted(SCAN.rglob("*.rs")):
        if p.name.endswith(".gen.rs"):
            continue
        yield p


def strip_tests(text: str) -> str:
    # Drop a trailing `#[cfg(test)] mod tests { ... }` (best-effort: from the marker to EOF).
    i = text.find("#[cfg(test)]")
    return text[:i] if i != -1 else text


def _match_body(text: str, open_brace: int):
    """End index of the `{`-delimited body starting at open_brace (best-effort brace match)."""
    depth, i = 0, open_brace
    while i < len(text):
        ch = text[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def fn_spans(text: str):
    """[(name, def_start, body_end)] for every `fn NAME ... { ... }` in text (skips bodiless
    signatures, e.g. trait method decls ending in `;`)."""
    out = []
    for m in FN_DEF.finditer(text):
        b = text.find("{", m.end())
        if b == -1:
            continue
        semi = text.find(";", m.end())
        if semi != -1 and semi < b:
            continue  # bodiless signature
        end = _match_body(text, b)
        if end is not None:
            out.append((m.group(1), m.start(), end))
    return out


def _call_sites(name: str, text: str):
    """Positions of `name(` call sites in text, excluding the `fn name(` definition site."""
    pat = re.compile(r"\b" + re.escape(name) + r"\s*\(")
    for m in pat.finditer(text):
        pre = text[:m.start()].rstrip()
        if pre.endswith("fn"):  # this is the definition, not a call
            continue
        yield m.start()


def compute_oracle_spans(scan_bodies, src_corpus, test_corpus, gen_corpus):
    """Per-scan-file oracle-fn body spans, by NAME or by REACHABILITY (PM-4 hardening).

    scan_bodies: {path: stripped_text}. src_corpus/test_corpus: lists of (path,)text from
    compiler/src (non-test hand .rs) and compiler/tests. gen_corpus: text of the generated
    systems (`*.gen.rs`) and their `*.frs` sources — a call to a hand leaf from there is a
    PRODUCTION caller (a live @@system invokes it), so the leaf must NOT be reclassified as an
    oracle just because its only hand-`.rs` caller is a `_hand` oracle. Returns
    {path: [(start,end), ...]}.
    """
    # All fns defined in scan/, with a stable key.
    fns = {}  # (path, name, start) -> (path, name, start, end)
    for path, text in scan_bodies.items():
        for name, start, end in fn_spans(text):
            fns[(path, name, start)] = (path, name, start, end)

    # Seed: oracle by name.
    oracle = {
        key for key, (_, name, _, _) in fns.items() if ORACLE_NAME.search(name)
    }

    def spans_for(path):
        return [(s, e) for (p, _, s, e) in (fns[k] for k in oracle) if p == path]

    # A call from a generated system (`*.gen.rs`) or its `*.frs` source is a PRODUCTION caller —
    # a leaf a live system invokes must never be demoted to oracle. Cache per-name counts.
    _gen_cache = {}

    def gen_calls(name):
        if name not in _gen_cache:
            _gen_cache[name] = sum(
                sum(1 for _ in _call_sites(name, gtext)) for gtext in gen_corpus
            )
        return _gen_cache[name]

    # Fixpoint: a consumer fn is an oracle iff it has >=1 caller and EVERY caller is a test or
    # sits inside an oracle span — AND it is not invoked by any live system. The recognizer
    # methods are never reclassified (they are the C-final deletion target).
    changed = True
    while changed:
        changed = False
        cur_spans = {p: spans_for(p) for p in scan_bodies}
        for key, (path, name, start, end) in fns.items():
            if key in oracle or name in RECOGNIZER_METHODS:
                continue
            if gen_calls(name) > 0:
                continue  # invoked by a live @@system → production
            n_prod = n_all = 0
            # callers in test files -> always 'test'
            for ttext in test_corpus:
                for _ in _call_sites(name, ttext):
                    n_all += 1
            # callers in src (non-test): 'oracle' if inside an oracle span, else 'prod'
            for spath, stext in src_corpus:
                osp = cur_spans.get(spath, [])
                for pos in _call_sites(name, stext):
                    if spath == path and start <= pos <= end:
                        continue  # self-recursion
                    n_all += 1
                    if not any(s <= pos <= e for s, e in osp):
                        n_prod += 1
            if n_all > 0 and n_prod == 0:
                oracle.add(key)
                changed = True

    name_keys = {key for key in oracle if ORACLE_NAME.search(fns[key][1])}
    reachability_added = sorted(
        (fns[k][0].name, fns[k][1]) for k in oracle - name_keys
    )
    return {p: spans_for(p) for p in scan_bodies}, reachability_added


def in_any(pos: int, spans) -> bool:
    return any(s <= pos <= e for s, e in spans)


def main() -> int:
    if not SCAN.is_dir():
        print(f"error: run from the cleanroom repo root (no {SCAN})", file=sys.stderr)
        return 2

    # Bodies of the scan/*.rs files under census (tests stripped).
    scan_bodies = {
        p: strip_tests(p.read_text(encoding="utf-8", errors="replace")) for p in rs_files()
    }
    # Caller corpora for reachability: all compiler/src (non-gen, tests stripped) and
    # compiler/tests. A caller in tests/ is a test; in src/ it is oracle iff inside an oracle span.
    src_corpus = [
        (p, strip_tests(p.read_text(encoding="utf-8", errors="replace")))
        for p in sorted(SRC.rglob("*.rs"))
        if not p.name.endswith(".gen.rs")
    ]
    test_corpus = [
        p.read_text(encoding="utf-8", errors="replace")
        for p in sorted(TESTS.rglob("*.rs"))
    ] if TESTS.is_dir() else []
    # Generated systems + their sources: any hand-leaf call here is a PRODUCTION caller.
    gen_corpus = [
        p.read_text(encoding="utf-8", errors="replace")
        for p in sorted(SRC.rglob("*.gen.rs"))
    ] + [
        p.read_text(encoding="utf-8", errors="replace")
        for p in sorted(SRC.rglob("*.frs"))
    ]
    oracle_span_map, reachability_added = compute_oracle_spans(
        scan_bodies, src_corpus, test_corpus, gen_corpus
    )

    lexer_defs = prod_calls = oracle_calls = prod_loops = oracle_loops = 0
    per_file = []
    for p, body in scan_bodies.items():
        spans = oracle_span_map.get(p, [])
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

    if "--audit" in sys.argv:
        print("\n-- reachability-demoted oracles (not `_hand`-named; every caller is a test or")
        print("   another oracle, and no live @@system invokes them) --")
        for fname, name in reachability_added:
            print(f"   {fname}::{name}")
        print(f"   ({len(reachability_added)} fns; run `git grep '<name>('` to audit any one)")

    if "--gate" in sys.argv and prod_hand_lexer > 0:
        print(
            f"\nGATE FAIL: production HAND_LEXER_RECOGNITION = {prod_hand_lexer} "
            f"(campaign DoD C2 requires 0)"
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
