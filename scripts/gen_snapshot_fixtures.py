#!/usr/bin/env python3
"""Generate per-backend snapshot fixtures from canonical templates.

Frame has no type system (FRAMEC_BUGS #37 — the per-backend type-alias
tables were exterminated): type names pass through to the generated code
VERBATIM. So the cross-backend snapshot corpus can no longer share one
fixture written in Rust-flavored types — each backend's fixture must use
that language's own native type names, exactly as a developer targeting
that language would write them. No type translation survives anywhere,
including the test corpus.

This reads `framec/tests/fixtures/_canonical/*.frm` (written with the
placeholder native types `i32` / `String` / `f32`) and emits
`framec/tests/fixtures/<target>/*.frm` with each placeholder substituted
to that backend's native spelling. `compile_fixture(name, target)` loads
from the per-target directory.

Re-run after editing a canonical template:
    python3 scripts/gen_snapshot_fixtures.py
"""
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = os.path.join(ROOT, "framec", "tests", "fixtures")
CANON = os.path.join(FIX, "_canonical")

# (int type, string type, float type) per target — the native type names a
# developer targeting that language would write. This table lives in TEST
# scaffolding (it chooses what types each fixture is written in); the
# compiler itself does NO type translation.
TYPES = {
    "rust":       ("i32", "String", "f32"),
    "python_3":   ("int", "str", "float"),
    "typescript": ("number", "string", "number"),
    "javascript": ("number", "string", "number"),
    "c":          ("int", "char*", "float"),
    "cpp":        ("int", "std::string", "float"),
    "csharp":     ("int", "string", "float"),
    "java":       ("int", "String", "float"),
    "go":         ("int", "string", "float64"),
    "kotlin":     ("Int", "String", "Float"),
    "swift":      ("Int", "String", "Float"),
    "php":        ("int", "string", "float"),
    "ruby":       ("Integer", "String", "Float"),
    "lua":        ("number", "string", "number"),
    "erlang":     ("integer", "string", "float"),
    "dart":       ("int", "String", "double"),
    "gdscript":   ("int", "String", "float"),
}


def main():
    templates = sorted(f for f in os.listdir(CANON) if f.endswith(".frm"))
    for lang, (int_t, str_t, float_t) in TYPES.items():
        outdir = os.path.join(FIX, lang)
        os.makedirs(outdir, exist_ok=True)
        for t in templates:
            with open(os.path.join(CANON, t)) as fh:
                src = fh.read()
            out = re.sub(r"\bi32\b", int_t, src)
            out = re.sub(r"\bString\b", str_t, out)
            out = re.sub(r"\bf32\b", float_t, out)
            with open(os.path.join(outdir, t), "w") as fh:
                fh.write(out)
    print(f"generated {len(TYPES)} backends x {len(templates)} fixtures")


if __name__ == "__main__":
    main()
