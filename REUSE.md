# The Cleanroom Wall

The rebuilt compiler lives in **`compiler/`**. The existing compiler lives in **`framec/`**.

`compiler/` **does not depend on `framec`**. Not by convention — by `Cargo.toml`. There is no path
dependency, so `use framec::…` is a **link error**, not a code-review finding. The wall is enforced
by the toolchain, because a wall that depends on discipline is a wall that erodes. (The old compiler
grew two copies of its `$.x` expander, and they silently drifted four ways. Discipline lost.)

---

## The rule

> **Tests cross. Code does not.**

**Tests and fixtures cross freely.** The corpus *is* the specification (RFC-0056 P13). The rebuilt
compiler earns its existence by passing it. Reading it is the point.

**Code does not cross by default.** The default is: **write it fresh.** A file may be transplanted
from `framec/` into `compiler/` only with an entry in the log below stating:

1. **What** — the exact file/function.
2. **Why** — what makes it worth keeping rather than rewriting.
3. **What was verified** — how we know it is correct. Not "it looks fine." *Reading it and finding
   nothing wrong is not verification* — that is precisely the review standard that shipped 25 bugs.

If you cannot fill in **(3)** with something you actually ran, do not transplant it. Write it fresh.

---

## Why the default is "write it fresh" and not "reuse what looks good"

Because "it looked fine" is how we got here. Attacking the old architecture for one session turned up
**nine bugs on top of the sixteen already known** — and every one of the nine was in code that had
been read, reviewed, and shipped. Three were **silent miscompiles**: a program that compiles clean,
exits 0, and prints the wrong answer.

The treasure in `framec/` is real, and it is *specific*: **seventeen targets' worth of idiomatic
spelling** — how a state machine should actually look in Swift, what C needs for a boxed struct, how
Go wants its type assertions. That knowledge cost years and it is worth keeping.

But the *treasure* is the **spelling**. The *disease* is the **structure** — the text oracles, the
duplicated expanders, the arms that branch on facts framec already knew and threw away. Transplanting
a file brings both. So: **read the old backend for its spelling, then write the new one against the
new IR.** If the new arm ends up looking like the old arm, that is because the target language did
not change — not because the structure survived.

---

## What is explicitly NOT in scope for the rebuild

- **The `@@efsm` compiler** (`framec/src/frame_c/compiler/fsm_*`, `codegen/fsm_*.rs`, ~40k LOC).
  It is a separate construct with its own front end, its own AST, and its own 17 backends, and it
  **never opens a source byte**. Measured coupling to the system side: three imports. It is not being
  rebuilt, it is not being touched, and it is not being transplanted. It will be re-attached at the
  end, as a sibling item in the tree.

---

## Transplant log

*(Empty. Every entry here is a deliberate decision with a name on it.)*

| # | What | Why | What was verified | Date |
|---|------|-----|-------------------|------|
| — | — | — | — | — |
