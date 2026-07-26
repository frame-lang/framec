# Faithfulness grid — Milestone × Language

Tracks framec-ng toward **byte-identical to legacy framec 4.6.0.x (latest local build)** across targets, per canonical
milestone. "Done" for a cell = its DoD fixture inventory passes the differential gate
(`test-env/scripts/differential_test.sh`: **identical code AND run-results, unless journaled**) +
the universal gates (regen 0-stale; suite green; systems-all-the-way-down; other targets unmoved;
Rust also self-hosts). Python is the **pathfinder**: its validated milestones seed each
`docs/faithfulness/M<k>.md` core + DoD inventory for rust/java/c.

## Status (2026-07-25)

| # | Milestone | python | rust | java | c |
|---|---|---|---|---|---|
A milestone is **✅ green ONLY when it has ZERO known gaps** (every DoD fixture byte-identical or journaled). A known gap = NOT green, no matter how "mostly done."

| # | Milestone | python | rust | java | c |
|---|---|---|---|---|---|
| M1 | Foundation (kernel/dispatch) | ✅ done | ▶ migrating | ⬚ | ⬚ |
| M2 | Construction & Seeding | 🟡 **2 gaps fixing** (vending param-drop, multi-sys `_create`) | ⬚ | ⬚ | ⬚ |
| M3 | Handlers & Interface | ✅ done | ⬚ | ⬚ | ⬚ |
| M4 | Actions & Operations | ✅ done | ⬚ | ⬚ | ⬚ |
| M5 | Hierarchy (HSM `=> $^`) | 🟡 **1 gap fixing** (`forward_enter_first` `forward_event`) | ⬚ | ⬚ | ⬚ |
| M6 | State Stack (push/pop) | 🔴 blocked (`_transitioned` needs shared hook) | ⬚ | ⬚ | ⬚ |
| M7 | Persistence | 🔴 not started (37) | ⬚ | ⬚ | ⬚ |
| M8 | Native-Text fidelity | 🔴 gaps: blank-line/pass/comment (shared) + whitespace ⚖ ruling | ⬚ | ⬚ | ⬚ |
| ~~M9~~ | ~~`@@fsm` (regex DSL)~~ | ⏸ **PARKED** (owner: not now) | ⏸ | ⏸ | ⏸ |

python: **180/351** byte-identical. NOTE: M2/M3/M5 were previously mismarked ✅ despite known gaps — corrected. Only M1/M3/M4 are truly green; M2/M5 have gaps in active fix (`cleanroom-python`); M6/M8 blocked on the shared driver-refactor hooks (in flight in `cleanroom-rust`). `@@fsm` fixtures (18) parked → excluded from the in-scope denominator.

## Python milestone BACKFILL + VALIDATION plan

Python's faithfulness work landed value-ordered (commits `10e947e`→`3627a26`→`8c8bf38`→`caa29eb`),
so it has no clean per-milestone record. Backfill = give each milestone a validated DoD, retroactively.

**Per milestone (M1…M9), do:**
1. **Attribute** — map Python's landed fixes to the milestone (which commit/class).
2. **Core doc** — write `docs/faithfulness/M<k>.md` (language-neutral behavior + legacy rules +
   DoD fixture inventory), seeded from Python's done work. (M1.md exists.)
3. **DoD inventory** — the explicit fixtures this milestone owns: reuse the single-feature
   `fuzz/cases_<feature>/` anchors + targeted `positive/` fixtures; **author-and-add** any missing
   single-feature anchor to test-env. List them in `M<k>.md`.
4. **Validate** — run `differential_test.sh` over the inventory: every fixture identical to a
   freshly regenerated oracle OR a journaled delta with a runtime validator. Record PASS here.
5. The differential test **is** the durable milestone regression gate.

**Work per milestone (from current status):**
- **M1 Foundation** — done. Validate the `foundation_*` inventory (byte-identical).
- **M2 Construction & Seeding** — the ctor/`_create` split, state-var seeding, `@@Sys()` are the
  **D2/D3 intentional divergences** (ng correct; legacy's Frame runtime crashes on plain
  construction). Validate = differential PASSES with those fixtures journaled (validators exist in
  `legacy_bug_fixes.rs`; `intentional_divergences.txt` populated by the running Python cell).
- **M3 Handlers & Interface** — handler key-order (`3627a26`) + default returns + lifecycle order
  (running cell). Doc + validate the handler/interface inventory.
- **M4 Actions & Operations** — `8c8bf38`. Doc + validate the actions inventory.
- **M5 Hierarchy (HSM)** — `=> $^` + 3-deep forward (`8c8bf38`). Doc + validate the HSM inventory.
- **M6 State Stack** — `_transitioned` guard (running cell) + push/pop. Finish → doc + validate.
- **M7 Persistence** — not started; **build a persist cell** (`cleanroom-python`, with
  `frame-persistence-reviewer` lens) → doc + validate. Persist has its own intentional divergences
  (RFC-0053 rebuild vs legacy #233) — journal them.
- **M8 Native-Text** — clean-tail (running cell) + the action-trailing-whitespace class (⚖ owner
  ruling: reproduce legacy's trailing spaces vs keep ng's clean output). Finish → doc + validate.
- **M9 `@@fsm`** — not started; separate DSL subsystem; **build an `@@fsm` cell** → doc + validate.

**Outcome:** Python has a clean, differential-validated M1–M9 record, and `M2.md`–`M9.md` +
their DoD inventories are seeded for rust/java/c to reuse (spelling-swap per language).

## Sequencing
Backfill/validate the **done** milestones (M1–M5) as docs+validation first (cheap, seeds the grid).
Finish the **partial** ones (M3/M6/M8) as their cells land. Build the **not-started** ones (M7
persist, M9 fsm) as new `cleanroom-python` cells. Owner rulings still open: action-whitespace (M8),
R5 `@@[target]` per-item.

## Oracle (owner-directed 2026-07-25)
The faithfulness oracle is the **latest LOCAL build**: `$HOME/.frame/local/bin/framec` = **framec
4.6.0.33** (built via `build-local.sh` after every legacy fix). NOT the release-tree binary
`framec/target/release/framec` (4.6.1 = 4.6.0.x + post-release RFC-0054 persist work).
**Verified difference:** the two emit **identically on all non-persist fixtures** (5/6 sampled
identical); they differ **only on persist** (`primary/23_persist_basic`, by the RFC-0054 lines). So
the M1–M6/M8/M9 work and the 180/351 count are oracle-independent; **only M7 Persistence** targets
the local build's true 4.6.0 persist behavior. If a newer local build lands, re-baseline.

## Known REAL gaps (found by the Python D2/D3 audit — NOT journaled; to fix in their milestone)
These are genuine ng defects the audit surfaced (and deliberately did NOT allowlist, so they stay visible):
- **M2 Construction:** `demos/23_vending_machine` — a domain field from a same-named *defaulted* system param drops the param value (`self.inventory = {}` instead of `= inventory`). `demos/20_multi_system_composition` — system-typed domain field not lowered through `_create` (`Logger()` vs `Logger._create()`).
- **M5 HSM:** `primary/29_forward_enter_first` — missing `__compartment.forward_event = __e`.
- **M8 Native-Text (shared-walk trivia):** blank-line drops (`primary/37,38`, `robotics/01`, `behavior_trees/ai_agent`, `frame_machines/context_parser`); extra `pass` on a param-binding-only body (`scientific/06`); dropped comments (`robotics/10`, `interfaces/return_no_type_annotation`).
- **R5 (pending ruling):** `primary/90_attribute_domain_field` — per-field `@@[target(...)]` filtering ignored.

## Clean-tail classes RE-SCOPED (they are SHARED, not python-leaf)
The Python cell proved the "clean-tail" classes cannot be closed in `python.rs` alone — the trailing bytes live inside the Frame-statement span (`stmt_scan` `end_out = eol`) and self-call detection needs `n.parts` (opaque to the leaf). So they need SHARED hooks, to land with the driver refactor:
- `Backend::reentrancy_guard` — fired by the shared walk after any self-call-bearing statement (`_transitioned` guard).
- a trailing-span pass-through from `emit_transition`/`emit_forward` (or a `scan` split) — for inline-native + trailing-comment.
After those hooks land, the three deferred classes become thin per-backend leaves.
