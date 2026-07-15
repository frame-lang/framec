---
title: "Persistence Roadmap (4.7 cleanroom)"
nav_exclude: true
---

# Persistence Roadmap — `@@[persist]` in the 4.7 rebuild

> **⚠ DEPRECATED — do not use.** This roadmap executed [RFC-0053](rfcs/rfc-0053.md) /
> [RFC-0054](rfcs/rfc-0054.md), which are superseded by [RFC-0056](rfcs/rfc-0056.md). Its phase
> plan, status ledger, and "dependency-free flat format" direction are retired. The north-star is
> now RFC-0056: single-language round-trip, value marshalling delegated to each host's serializer,
> framec owning only the manifest, control state, construction bypass, schema check, and the
> Regime-B type-identity envelope. Any new persistence work follows RFC-0056, not this file.

Implements [RFC-0053](rfcs/rfc-0053.md) (faithful persistence — the foundation) and
[RFC-0054](rfcs/rfc-0054.md) (the `PersistManifest`). This document is the **execution
plan and honest status ledger**. It is checked against behaviour — a target counts as done
only when its round-trip is proven by *running* the target's toolchain, never by emitting
plausible-looking code.

> **⚠ Scope caveat (RFC-0055).** [RFC-0055](rfcs/rfc-0055.md) (the *latest* persistence RFC,
> Draft) reframes the goal as **best-fidelity, persist-anything**: the automated tier must
> round-trip **user-defined types, nesting, and collections**, not just primitives, with
> **R2** making user-type self-marshalling through the host serializer (serde / Jackson /
> `encoding/json`) a **MUST** on the static (Regime A) route. **What is built today meets
> only the SCALAR subset of that contract.** The fixed-type route hand-rolls a flat format;
> a user-typed field does not compile — a named, `#[ignore]`d honest-gap in `honest_gaps.rs`
> and `tests/persist.rs`. So "DONE" below means **scalar round-trip + control state + schema
> refusal proven by running** — it does **not** mean the RFC-0055 automated tier is met.

> **The rule for this file:** a row is `DONE` only when a test compiles the output and runs
> a save→restore→observe cycle and checks the value. "It compiles" is not done. "It emits
> a `restore()`" is not done. The old snapshot suite blessed non-compiling code for years
> because it only compared text; persistence will not repeat that.

---

## The contract (foundation only)

For any `@@[persist]` system, a **save immediately followed by a restore into a fresh
instance** MUST reproduce the persisted state — `domain:` values (minus `@@[no_persist]`)
**and live control state** — so all later observations are indistinguishable from the
original, **identically on every target that can reconstruct the program's values**, with
no target silently degrading a value.

Three foundational constraints:

1. **Faithful round-trip** including user-defined types and control state.
2. **Type-ignorant codegen** — one mechanism per target, no `match user_type`.
3. **Unambiguous disambiguation** (out-of-band framing) so a user container carrying the
   reserved marker as data is never mis-restored — **#233**.

Plus one **non-deferrable safety floor**: the reflective route resolves a blob-named type
only against types the program defines, never ambient globals/imports.

---

## The two routes (RFC-0053 §Approach)

| route | targets | mechanism | #233 exposure |
|---|---|---|---|
| **Reflective** | Python, JavaScript/TypeScript, Ruby, PHP, Lua, GDScript | type identity travels **in the snapshot**; framec emits the reviver | **exposed** — this is where #233 and the safety floor live |
| **Fixed-type** | Java, C#, Kotlin, Swift, Dart, Go, Rust, C++, C | type is **fixed at codegen**; the target's serializer deserializes into the declared field type | **structurally immune** — no marker is read from the blob |

The routes are **not equally reachable**. Lua has no first-class type identity (all values
are tables) — a program needing a reconstructed *type* is beyond what Lua expresses; the
faithful form there is the native container, and needing more is a *reachability limit*
(diagnosability layer), not a silent erasure. GDScript reaches it via Godot's resource
machinery.

---

## Phases

### Phase 0 — Reflective foundation on ONE target ✅ DONE (Python)

The hard part: out-of-band framing, save-time escaping, the safety floor, control state.
Proven by running (`compiler/tests/persist.rs`, 6 tests):

- faithful round-trip of a user type + scalar
- **#233**: a user dict carrying `@f:t` comes back a **dict**
- adversarial: a user dict whose keys are *exactly* the envelope slots → still a dict
- a typed value nested inside a user dict → both survive
- safety floor: a blob naming a stdlib type → refused (`E750`)
- live control state round-trips

**The design, fixed here for all reflective targets:** every framec-typed value is an
envelope `{"@f:t": "Point", "@f:v": {...}}`; the reviver reads the type only from `@f:t`; a
colliding user dict is wrapped with an **empty** tag on save and unwrapped-without-revival
on restore. Disambiguation is *structural*, not a rarer-marker-string gamble.

### Phase 1 — Fixed-type route: make `restore()` genuinely round-trip ✅ DONE (Java; P1.3 deferred)

Java emits a correct `snapshot()` and a **real `restore()`** that parses into the declared
field types + control state. Proven by running (see below). P1.3 (swap the hand reader for a
real JSON lib) stays deferred while the corpus is scalar.

- [x] **P1.1 — Java** `restore()` parses the snapshot into declared field types + control
  state. Proven by `honest_gaps::java_persist_actually_round_trips` (a running
  save→restore→observe test — `n` comes back 3, not 0). Uses the Number-ladder
  extraction, which survives JSON round-trips (`Long`/`BigDecimal`).
- [x] **P1.2** — the honest-gaps assertion now demands a **round-trip**; it was red on the
  no-op stub and is green only because restore actually restores.
- [ ] **P1.3** — replace the hand-rolled snapshot/field-reader with the target's real JSON
  serializer (Jackson etc.), removing the last type-name branch (`java_is_string`). Deferred:
  the corpus is scalar and this keeps the route dependency-free for now.

Scalars first (the corpus fixtures). A user *value type* on a fixed-type target is faithful
via the target's serialization library handed the declared type; that lands with each
backend as it is built.

### Phase 2 — Breadth: the remaining reflective targets 🔜 (blocked on backends existing)

The design is fixed (Phase 0). Each reflective backend re-expresses the SAME envelope +
escape + closed-world reviver in its idiom. **The floor fix (emitted lexical registry, not
file/module enumeration) is built here per RFC-0054 / the audit's Finding 2.**

- [ ] **P2.1 — JavaScript / TypeScript** (TS is JS at runtime — one implementation)
- [ ] **P2.2 — Ruby** — use the **emitted lexical registry**, NOT `ObjectSpace`/`__FILE__`
  membership (which leaks: a monkeypatched stdlib class becomes resolvable — audit Finding 2)
- [ ] **P2.3 — PHP** — emitted registry, not `get_declared_classes`/`__FILE__`
- [ ] **P2.4 — Lua** — native-container faithful form; a needed *type* is a reachability
  limit surfaced loudly, not silently erased
- [ ] **P2.5 — GDScript** — via Godot resource/reflection machinery

Each backend gets the Phase-0 test battery (round-trip, #233, adversarial, nested, floor,
control) ported to its toolchain. A backend is not done until all six run green on it.

### Phase 3 — Breadth: the remaining fixed-type targets 🔜 (blocked on backends existing)

- [x] **P3.6 — Rust** — dependency-free (no serde), mirrors Java: a hand-rolled flat
  snapshot, and `restore` parses into the declared types — `self.n = ...parse().unwrap()`
  infers the target from the field, so unlike Java no per-type extraction match is needed.
  Proven by `tests/persist.rs` — round-trip + schema-refusal + control-state, on running rustc.
- [x] **P3.8 — C** — the same flat format in C's idiom: free functions, no `String` (a
  `char*` built with `snprintf`, restored with `strstr`/`atoi`), refusal is a stderr line +
  return (no exceptions). Proven by `tests/persist.rs` — round-trip + schema-refusal +
  control-state, on running cc.
- [ ] **P3.1 — C#**, **P3.2 — Kotlin**, **P3.3 — Swift**, **P3.4 — Dart**,
  **P3.5 — Go**, **P3.7 — C++**

Each deserializes into the declared type via the target's serialization facility. Immune to
#233 by construction; still must **round-trip faithfully** and carry control state.

### Phase 4 — Cross-target parity gate 🔜

- [ ] **P4.1** — one battery, run across **every** implemented target, asserting the SAME
  observable behaviour: the RFC's "a program that round-trips on one such target round-trips
  on all of them." A target that cannot reconstruct a given program's value must **say so
  loudly** (`E750`/diagnostic), never restore a degraded value.
- [ ] **P4.2** — the RFC-0054 manifest is the single derivation feeding every backend; add a
  test that the manifest, not per-backend logic, decides *what* is persisted.

---

## Deferred LAYERS (RFC-0053 — named, not built; each gates on its own decision)

These are **not** part of the foundation and are correctly out of scope for this roadmap's
DONE bar. Listed so none is forgotten.

- **L1 — Full untrusted-snapshot security.** Beyond the minimal closed-world floor (which
  co-ships): allocate-without-init hardening, an identity-checked allow-set, the trust
  model. *Open decision: the trust boundary for a snapshot from outside the program.*
- **L2 — Compile-time diagnosability.** Reject a program that cannot be persisted faithfully
  *before it runs*, rather than failing at restore with `E750`. *Open decision: first-save-
  error vs. require-annotation (needs the #123 class-detection oracle).*
- **L3 — Coverage of pathological shapes.** Reference cycles, shared object identity, types
  with no reflectable field map. *Open decision: which the borrowed encoding must represent
  vs. reject.*
- **L4 — Owned encoding format.** A format framec defines itself rather than borrowing each
  target's serializer. *= RFC-0054 Phase B/C (wire manifest, portable revival).*

---

## Status ledger (behaviour-verified)

| item | target | status | proof |
|---|---|---|---|
| Reflective foundation (user type + scalar) | Python | ✅ DONE | `tests/persist.rs` — running |
| #233 impossible | Python | ✅ DONE | adversarial + nested tests run green |
| Safety floor (visible-but-foreign type refused) | Python | ✅ DONE | imported-type-in-blob refused (`E750`) |
| Control state (distinguishing, A→B) | Python | ✅ DONE | round-trips |
| Schema-drift refusal (`E751`) | Python/Rust/C | ✅ DONE | mismatched schema refused (running) |
| Fixed-type route — **SCALARS only** | Java | ✅ DONE | `honest_gaps::java_persist_actually_round_trips` — `c2.n==3` (running javac/java) |
| Fixed-type route — **SCALARS only** | Rust | ✅ DONE | round-trip + schema-refusal + control-state (running rustc) |
| Fixed-type route — **SCALARS only** | C | ✅ DONE | round-trip + schema-refusal + control-state (running cc) |
| Fixed-type **user types / nesting / collections** (RFC-0055 R2) | Java/Rust/C | ⛔ OPEN | `#[ignore]`d gap tests fail to compile — needs the host serializer |
| All other reflective targets | JS/TS/Ruby/PHP/Lua/GDScript | ⛔ no backend yet | — |
| All other fixed-type targets | C#/Kotlin/Swift/Dart/Go/C++ | ⛔ no backend yet | — |
| Cross-target parity gate | all | ⛔ TODO | Phase 4 |

**Bottom line (honest scope).** Every backend that currently exists carries a **scalar**
save/restore proven by running its toolchain — Python (reflective, where #233 and the
closed-world floor live, and where a user type + nesting also round-trip) and the three
fixed-type backends (Java, Rust, C: scalars + control state + schema refusal). Two things are
**NOT** yet true, and are now named as failing `#[ignore]`d honest-gap tests rather than
implied by a green suite:

1. **The RFC-0055 automated tier on the fixed-type route.** User-defined types, nesting, and
   collections do not round-trip on Java/Rust/C — a user-typed field does not compile, because
   the hand-rolled flat format bypasses the host serializer that **R2** mandates. This is the
   single largest open item; closing it means adopting serde / Jackson / `encoding/json` on
   Regime A.
2. **Breadth** — the remaining 13 targets need their backends built first.

The manual tier (graphs, cycles, live resources) and Regime C (Lua/GDScript) are unbuilt.
Everything past L1 is a deliberately deferred layer.
