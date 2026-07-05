#!/usr/bin/env python3
"""hack_census.py — the anti-regression ratchet for the framec hack sweep.

The July 2026 `hack-sweep` workflow inventoried every text-oracle / heuristic /
latent-FSM / silent-fallback site in the compiler (report:
`_scratch/hack_sweep_inventory.md`). This script is the mechanical half of that
audit, kept re-runnable so the audit stays *done*: it counts the seed patterns
that flag those classes, and fails if a NEW site appears in codegen that is not
on the reviewed allowlist.

It is deliberately dumb — grep with structure. It does not judge; it forces a
human to look at every new hit and either fix it or record why it is incidental
(the R7 discipline: every accepted string-op site is a *reviewed claim*, not
silence).

Usage:
    python3 tools/hack_census.py               # census + ratchet check (exit 1 on drift)
    python3 tools/hack_census.py --report      # full per-file table, no gate
    python3 tools/hack_census.py --bless        # rewrite the allowlist to current counts

The allowlist lives at `tools/hack_census_allow.json` (per-file accepted counts,
seeded from the swept baseline). Raising a count requires re-blessing, which is a
reviewable diff.
"""
import os, re, json, sys, argparse

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "src", "frame_c", "compiler"))
ALLOW = os.path.join(HERE, "hack_census_allow.json")

# Erlang target is deprecated (W901) and excluded from ongoing hardening.
SKIP_DIR_NAMES = {"erlang_system"}

SEEDS = {
    # A/B/C signature: probing or rewriting emitted/source text
    "text_probe": re.compile(r"\.(contains|starts_with|ends_with|find|rfind)\("),
    "replace":    re.compile(r"\.replacen?\("),
    # D signature: hand-rolled scanners with mode-state locals
    "mode_state": re.compile(r"\b(in_string|in_char|escaped?|depth)\b\s*[:=]"),
    # E signature: catch-all arms that can emit wrong/empty output silently
    "empty_arm":  re.compile(r'_\s*=>\s*(String::new\(\)|""|"".to_string\(\))'),
}


def is_excluded(path: str) -> bool:
    if not path.endswith(".rs"):
        return True
    if path.endswith(".gen.rs") or path.endswith("_tests.rs"):
        return True
    parts = path.split(os.sep)
    return "erlang" in path.lower() and any(
        p in SKIP_DIR_NAMES or "erlang" in p for p in parts
    )


def census():
    counts = {}
    for dp, dirs, files in os.walk(ROOT):
        dirs[:] = [d for d in dirs if d not in SKIP_DIR_NAMES]
        for fn in files:
            p = os.path.join(dp, fn)
            if is_excluded(p):
                continue
            src = open(p, errors="ignore").read()
            # crude comment strip so doc-comments don't inflate counts
            code = "\n".join(l.split("//")[0] for l in src.split("\n"))
            row = {k: len(pat.findall(code)) for k, pat in SEEDS.items()}
            if sum(row.values()) > 0:
                rel = os.path.relpath(p, ROOT)
                counts[rel] = row
    return counts


def weight(row):
    return (row["text_probe"] + row["replace"] * 2 +
            row["mode_state"] * 2 + row["empty_arm"] * 3)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", action="store_true", help="print full table, no gate")
    ap.add_argument("--bless", action="store_true", help="rewrite allowlist to current counts")
    args = ap.parse_args()

    counts = census()

    if args.bless:
        json.dump(counts, open(ALLOW, "w"), indent=1, sort_keys=True)
        print(f"blessed {len(counts)} files -> {os.path.relpath(ALLOW)}")
        return 0

    if args.report:
        for rel, row in sorted(counts.items(), key=lambda kv: -weight(kv[1])):
            print(f"  probe={row['text_probe']:3} repl={row['replace']:2} "
                  f"mode={row['mode_state']:2} empty={row['empty_arm']:2}  {rel}")
        tot = {k: sum(r[k] for r in counts.values()) for k in SEEDS}
        print(f"\n{len(counts)} files  totals={tot}")
        return 0

    if not os.path.exists(ALLOW):
        print("no allowlist yet — run with --bless to seed the baseline", file=sys.stderr)
        return 2

    allow = json.load(open(ALLOW))
    drift = []
    for rel, row in counts.items():
        base = allow.get(rel, {k: 0 for k in SEEDS})
        for k in SEEDS:
            if row[k] > base.get(k, 0):
                drift.append((rel, k, base.get(k, 0), row[k]))

    if drift:
        print("HACK-CENSUS RATCHET TRIPPED — new string-op/scanner sites appeared:\n")
        for rel, k, was, now in sorted(drift):
            print(f"  {rel}: {k} {was} -> {now}")
        print("\nEach new site must be either (a) implemented structurally, or")
        print("(b) reviewed as incidental and recorded, then re-bless the allowlist.")
        print("See _scratch/hack_sweep_inventory.md for the classification rule.")
        return 1

    print(f"hack-census OK — {len(counts)} files, no new string-op/scanner drift")
    return 0


if __name__ == "__main__":
    sys.exit(main())
