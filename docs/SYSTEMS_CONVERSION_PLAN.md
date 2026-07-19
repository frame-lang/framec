---
title: "Converting the Cleanroom Scanner/Parser to @@systems — the Plan"
nav_exclude: true
---

# Converting the cleanroom's scanning/parsing to `@@systems`

**Status: LIVE.** This is the authoritative, methodical plan for making the cleanroom's
recognizers and control-walks Frame `@@system` machines instead of hand-written Rust
byte-loops. It supersedes the planning notes in `SYSTEM_INVENTORY.md` (kept for its 4.6.0
architecture map, but its item-status claims are stale — trust this file for status).

## The goal, stated once

Every **recognizer** and **control-walk** in the compiler becomes an `@@system`. What stays
native, by deliberate design (4.6.0 keeps its equivalents native too): the symbol-table
assembly (`resolve.rs`), tree construction (`tree/*`), `Atom`/`Place` builders, and each
backend's **spellings** (how it writes a class/method/persist blob). The conversion targets
the hand-rolled-loop surface — the ~5,300 lines under `text/scan/` and, last, the emit
*walk* — not the spellings.

**Terminology (2026-07-18):** a *capability* below = a **conversion set** in RFC-0058's
vocabulary — the atomically-landed closure of changes (one oracle, one gate pair, one commit;
it does not map 1:1 with a system). GATE-A = RFC-0058's *parity gate*; GATE-B = its *landed
gate*. The per-capability DoD applies per conversion set.

## The four guardrails (non-negotiable)

1. Each capability → a `.frs` `@@system`, compiled by framec-ng to a committed `.gen.rs`,
   wrapped by a thin `mod.rs` with native **leaves**. The campaign dialect is
   `@@[scan(u8)]` **systems only**: `@@fsm` is parked for this phase of the rebuild
   (owner ruling 2026-07-16, reaffirmed 2026-07-18) — a design proposing one is a
   guardrail violation, not a style choice.
2. The hand path is **deleted** the moment its system passes the parity gate and its last
   consumer is routed off it. Never leave both in production. "Done" = hand path gone.
3. If a system needs a capability framec-ng can't emit, **STOP and surface it as a blocker** —
   do NOT hand-roll around it. Hand-rolling around blockers is how we got here.
4. Residual native **leaves** are of two kinds. **Category A** — pure per-target *facts*
   (byte compares, form-table lookups: "is there a `//` here"). **Category B** — anything that
   *walks/recognizes* (counting, bracket-balancing). Category B goes **into a sub-system**, not
   a leaf. New leaves are **named and signed off** before they land — nothing native lands
   silently.

## The proven conversion loop (validated end-to-end by Item 1)

For each capability `X`:

1. **Author `X.frs`** — the WALK (dispatch + body loops + counters) in Frame. Identify the
   leaves it needs; classify each A or B. Get sign-off on the leaf surface. Category-B →
   author a sub-system (`RawString`, `BraceBalance` were Item 1's).
2. **Generate**: `framec-ng -l rust --emit X.frs | grep -v '^#!\[allow' > X.gen.rs`.
3. **Seam**: `X/mod.rs` — `mod fsm { use super::{leaves…}; include!("X.gen.rs") }` + a thin
   `pub fn` entry that runs the machine and reads `cursor`. Register `pub mod X;`.
4. **Parity gate — differential test.** Rename the hand implementation `foo` → `foo_hand`
   (keep it as the oracle). Write `tests/X.rs`: for representative + adversarial inputs, at
   **every** byte position, assert `machine(i) == foo_hand(i)`. This is the anti-"compiles-but-
   wrong" gate — the hand path is a free byte-for-byte oracle; that is *why we convert in
   place, not greenfield*.
5. **Wire production**: point the production consumer(s) at the system.
6. **Corpus gate**: run the **full cleanroom suite** — it drives the whole pipeline over the
   corpus, so a green suite is the behavioral proof on real inputs.
7. **Delete**: when the *last* consumer of a hand function is routed off it, delete the
   function. When a hand path's only remaining user is its own differential oracle, convert the
   test to **self-contained** (assert known extents, no `_hand`) and delete the oracle too.
8. **Commit** `.frs` + `.gen.rs` + `mod.rs` + test together. One capability per commit.

## Definition of Done — falsifiable (the warden evaluates against THIS)

Every criterion below is a mechanical check that can FAIL. "Done" is never a judgement call; it
is the conjunction of these predicates, each verified by running/grepping, never asserted. The
`frame-conversion-warden` agent (`.claude/agents/frame-conversion-warden.md`) evaluates a
milestone against exactly this list and returns PASS only if every applicable predicate holds.

### Per-capability DoD (a converted capability `X` is done iff ALL hold)

- **D1 Authored-as-system.** `X.frs` exists and contains the WALK; `cargo build -p
  frame-compiler` rc=0. *Falsify:* file missing / build fails.
- **D2 Regen fixpoint.** `framec-ng -l rust --emit X.frs | grep -v '^#!\[allow'` equals the
  committed `X.gen.rs` byte-for-byte, and stays equal across a rebuild (regen→rebuild→regen).
  *Falsify:* any diff.
- **D3 Leaf discipline (Category-A only).** Every native fn in `X/mod.rs` is a per-target FACT —
  O(1) byte-compare / form-table lookup / a fixed-size lookahead / a run-and-unwrap wrapper of a
  sub-system. *Falsify:* a leaf contains a loop/counter/recursion that consumes unbounded input
  to *decide* (that is a walk → must be a sub-system).
- **D4 Parity gate.** `tests/X.rs` asserts `machine(i) == hand_oracle(i)` at EVERY position over
  (a) curated inputs covering every form for every applicable target, AND (b) a fuzz/property
  generator. Green. *Falsify:* test absent, no fuzz, fails, or the curated set demonstrably omits
  a form the target has.
- **D5 Production wired.** Every PRODUCTION consumer of the hand function `X` replaces now calls
  `X`. *Falsify:* grep finds a production caller of the hand fn that is not (i) a named+scheduled
  residual in this plan or (ii) the differential oracle.
- **D6 Hand path retired-or-deferred.** When the hand fn has no remaining production consumer, it
  is DELETED (grep 0 refs). If deferred, this plan names the item that will delete it. *Falsify:*
  grep count ≠ expected, or a deferral with no named owning item.
- **D7 Suite green.** `cargo test -p frame-compiler` rc=0, 0 failed. *Falsify:* any failure.
- **D8 Machine honesty.** `X`'s claimed computational class matches its shape (a counter is a
  counter; kind-matched brackets use `push$`/`pop$`; a first-token dispatch is NOT dressed as a
  sequencer). *Falsify:* the warden classifies it and the claim is wrong.
- **D9 Atomic commit.** `X`'s `.frs` + `.gen.rs` + `mod.rs` + test land in one commit; nothing
  uncommitted after. *Falsify:* `git status` dirty, or the artifacts split across commits.

### Campaign DoD (the whole conversion is done iff ALL hold)

- **C1** A committed census (`tools/scan_census.*`, target 0) reports zero hand recognition/walk
  loops in `text/scan/` outside Category-A leaves and `.gen.rs`.
- **C2** The hand `Lexer` recognition is gone: grep `comment_at|literal_at|fn quoted|fn
  triple_quoted|fn rust_raw|fn block_comment|fn hole_at` == 0.
- **C3** Every recognizer/walk in the item plan is a system whose per-capability DoD is met.
- **C4** All `.gen.rs` pass the regen fixpoint; **C5** full suite green + a fuzz gate exists +
  every system has a parity or self-contained test.
- **C6** I1 (byte coverage) + I2 (island) invariants hold — the `check_coverage` assert plus a
  test. **C7** Emit consumes only nodes: grep `Lexer::new|segment(|comment_at|literal_at` in
  `text/emit/` == 0.
- **C8** The native residual equals EXACTLY the allowlist (resolve.rs, tree/*, Atom/Place,
  per-backend spellings) — the census confirms no recognition/walk outside it.
- **C9** R3 target-coverage resolved (no silent narrowing). **C10** ledger: every item marked
  hand-path-deleted.

## Testing strategy & corpus growth

Nearly every gate IS a test. Two kinds, and the kind decides where it lives:

- **Conversion-consistency (SCAFFOLDING — stays cleanroom):** the differential parity test (system
  vs hand oracle at every position), the regen fixpoint, the I1/I2 invariant asserts. They need
  the hand oracle / internal spans; the test-env has neither. The parity tests convert to
  **self-contained behavioral specs** when the oracle is deleted at end-of-item.
- **System behavioral specs (PROMOTABLE):** the per-`@@system` unit batteries are language-agnostic
  statements of what the machine does — harvestable as `@@[scan]` cross-backend test-env fixtures.
  **Gated on shipping supporting `@@[scan(u8)]`-on-`@@system`** (RFC-0042.1/#209, a cleanroom-only
  capability today); build them now, shaped for promotion (target-parameterized input→extent), and
  promote when shipping can compile them.

**Mandates (the warden's D4 enforces these):**
- A **unit battery per system** — exhaustive over the forms it recognizes (read the `.frs` +
  `literals.rs` table to enumerate), plus edges and adversarial inputs, plus a **deterministic fuzz
  generator**.
- **≥1 milestone-validation test per milestone** — end-to-end through the real pipeline, so a
  regression fails a named test.
- The corpus of system+milestone tests grows monotonically; nothing lands without its battery.

`frame-test-author` (`.claude/agents/frame-test-author.md`) authors and RUNS these; the warden
judges them. Author → warden-gate → land.

## Milestones & consultation gates (fine-grained; when the warden is consulted)

A **capability** = one `.frs` system landing + the retirement of the hand code it replaces.
(Item 1 = one capability, "opaque skip", though it shipped 3 systems; Item 3 = several.) Each
capability passes through four milestones, of which TWO are warden gates:

1. **M-author** — `.frs` + any sub-systems authored, generated, unit-tested. (self-check)
2. **GATE-A (parity)** — differential + fuzz green, BEFORE wiring production. **Warden consulted**
   → checks D1–D4, D8. Purpose: never wire a wrong machine.
3. **M-wire** — production consumers routed; suite green. (self-check)
4. **GATE-B (landed)** — hand path deleted/deferred, committed. **Warden consulted** → checks
   D5–D9 + drift (code vs plan) + ledger update. Purpose: never call a capability done with a
   surviving hand path or an unrecorded deviation.

A **campaign gate** runs the Campaign DoD (C1–C10) at the end. The warden's verdict on every gate
is written (PASS/FAIL + findings) and appended to the Audit Log (below). A FAIL blocks progress
to the next milestone until resolved.

## Course corrections, negotiation, and recording changes

The plan is a contract; it changes only through a recorded, evaluated process — never silently.

- **What counts as a plan change:** a new/removed sub-system, a reordering, a native leaf that
  isn't Category-A, a deferral, a discovered wrong dependency, a blocker resolution, or any
  deviation from the DoD/guardrails.
- **Process:** (1) I append a dated entry to the **Change Log** below — *what, why, which
  DoD/guardrail/invariant it touches, alternatives considered.* (2) I consult the warden; it
  evaluates the change against DoD + the four guardrails + I1/I2/consume-only-from-nodes and
  returns **ACCEPT** / **REJECT (reasons)** / **ESCALATE**. (3) A change touching a **guardrail**,
  the **native allowlist**, or **target coverage** is auto-ESCALATE → Mark decides (human).
  (4) The Change Log entry records the resolution (accepted/rejected/escalated + decision).
- **Drift is a defect.** At GATE-B the warden diffs code-reality against this plan; any deviation
  with no Change Log entry FAILS the gate — it is either recorded as a change or reverted.
- **Blockers (guardrail 3):** a needed capability framec-ng can't emit is logged as a blocker
  Change Log entry and ESCALATED — never hand-rolled around.

### Change Log (append-only)
- *2026-07-17* — R1–R11 folded from the two agent reviews (see next section). Touches D4/D5/D6
  (residual consumer count, gate strength), R3 target coverage (ESCALATED — awaiting Mark).
- *2026-07-17* — **R3 RESOLVED (Mark: GATE).** Gate `target` to the 4 supported backends
  (python_3/java/rust/c) BEFORE `segment()` in `main.rs`; an unsupported `-l` refuses at parse.
  This removes the 16→4 narrowing (the two halves of a parse can no longer disagree) and matches
  what the cleanroom actually supports. Touches target coverage (was ESCALATE) — decided.
- *2026-07-17* — **R2 REFINED (deferral; within guardrails — warden to confirm, not ESCALATE).**
  Only `native_parts_scan::try_island` is a clean extent-only route-now target; route it to
  `skip_opaque_at`. `close_brace` DEFERRED to **Item 2** — it propagates an unterminated-opaque
  `Err` via `?`, which is more than the extent (a 3-way None/Extent/Unterminated signal), and it
  is the Segmenter's item-end logic. `machine.rs::skip_opaque` DEFERRED to **Item 3** — it applies
  a different limit policy to comments (clamp) vs literals (reject), so it needs the opaque *type*,
  not just the extent; it is Item-3 grammar code. Both deferrals carry a named owning item (D6
  satisfied). Rationale: routing either through the extent-only `skip_opaque_at` would change
  behavior on malformed input (close_brace) or lose the limit-policy type (machine.rs). They agree
  with OpaqueScan on well-formed input (differential-proven), so the residual dual window is benign
  until its owning item.

- *2026-07-17* — **Item 2 DESIGN accepted (frame-fsm-designer).** The 3-way opaque signal is a
  DOMAIN REGISTER, not a new terminal state or a state-name read (both blocked: `scan_at` halts
  only on Accept/Reject; `compartment` is private — zero-codegen-change is the register).
  OpaqueScan gains `unterminated: bool` (set on the 4 body-reject edges + the raw arm) and
  `kind: i32` (1=comment/2=literal, set on the 5 accept edges); RawString gains `unterminated`
  (set on its `$Body`/`$MaybeClose` EOF rejects) and a `RawAt`/`scan_kind` 3-way, composed by
  OpaqueScan via two run-and-unwrap leaves (the `#`-counter stays IN RawString — no walk in a
  leaf, D3 holds). Wrapper: `OpaqueAt { None, Comment(usize), Literal(usize), Unterminated }` +
  `opaque_at()`; `opaque_extent()` becomes a thin adapter (extent-only consumers byte-unchanged).
  `kind` also serves Item 3's `machine.rs::skip_opaque` (comment→clamp, literal→reject) — one
  mechanism, built once. **DECISION (mine, within protocol):** `Unterminated`→a single
  `UnclosedBody` error (names the system — a better diagnostic than the hand `UnterminatedComment`/
  `String`); the differential gates `is_err()`-parity on unterminated, not variant-equality.
  **Capability FILED (not a blocker): R10** — a real Frame-level scan-system→sub-system invocation
  reading the sub-machine's terminal classification in-Frame; would make raw single-run and reduce
  the run-and-unwrap wrapper growth in Items 3/4. To weigh before Item 4.
  Gate: Level-1 `opaque_at` vs a new `opaque_at_hand` 4-way oracle at every position + fuzz (the
  current extent test is blind to this class); Level-2 `close_brace` vs `close_brace_hand`.

- *2026-07-17* — **Item 2 EXECUTED (implementer).** As designed. OpaqueScan `opaque_scan.frs`:
  `unterminated: bool` set on the 4 body-reject edges ($BlockBody/$StrBody-EOF, $StrBody-newline,
  $TripleBody-EOF) + a new raw-unterminated arm in $Start; `kind: i32` written at dispatch (1 on
  the two comment dispatches, 2 on raw/triple/string). RawString `raw_string.frs`: `unterminated`
  on $Body/$MaybeClose EOF. Both `.gen.rs` regenerated + **D2 fixpoint byte-identical across a
  rebuild**. mod.rs: `OpaqueAt`/`opaque_at` + extent-only `opaque_extent` adapter; `RawAt`/
  `scan_kind` + `scan` adapter; `raw_scan`/`raw_unterminated` leaves compose RawString (counter
  stays in the sub-system — D3). `scan/mod.rs`: `close_brace` rewritten off the hand `Lexer` onto
  `opaque_at` (Unterminated→`UnclosedBody`); `close_brace_hand` + `opaque_at_hand` added as
  `#[doc(hidden)]` differential oracles. Tests (frame-test-author, all green, mutation-verified
  teeth): opaque_scan.rs 14 (4-way `opaque_at`==`opaque_at_hand` at every position + Unterminated
  arm + all-variants-occur + fuzz); `close_brace_tests` 7 (is_err parity + Ok-equality, `}` hidden
  in every opaque form + unterminated + xorshift fuzz 1500×4 both-arm teeth-gated); raw_string
  `scan_kind` battery +3 (NotRaw/Extent/Unterminated). Full suite green; clippy clean on touched
  code. Production `close_brace` no longer touches the hand lexer.
- *2026-07-17* — **R12 (tooling refinement; warden to confirm).** `tools/scan_census.py` now
  SPLITS `HAND_LEXER_RECOGNITION` into **production** (C2 ratchet → 0) and **oracle** (recognition
  inside a `*_hand`/`hand_*` `#[doc(hidden)]` differential oracle — transient, deleted at C-final
  per D6). Rationale: a differential oracle uses the hand lexer ON PURPOSE (it is the independent
  check the system is proven against); counting it as production made Item 2 read as a ratchet
  regression (19→21) when production recognition actually **DROPPED** (close_brace: 15→13). The
  split measures what C2 means; `--gate` now trips only on the production bucket; the oracle bucket
  is reported and must ALSO reach 0 by campaign end (oracles cannot hide). No production behavior
  change. Post-Item-2: production=13 (lex.rs 7 defs + machine.rs 2 + parts.rs 2 + sections.rs 2 —
  all owned by Items 3/4), oracle=8, loops=86, systems=15.

- *2026-07-17* — **Item 2 scope split + two deferrals (warden GATE-A+B PASS-pending-commit
  findings 2 & 3; within guardrails).** This execution delivers the **`close_brace` / body-end
  capability** in full — NOT all of Item 2. Two named residuals remain:
  (a) **`hand_item_starts` (the Segmenter differential oracle) is NOT retired.** It is still live
  and `tests/segmenter.rs` compares `segmenter::item_starts` against it. Retiring it now would
  delete the Segmenter's independent check while its parity is still oracle-gated. DEFERRED to the
  **C-final oracle sweep** (owned with the other `*_hand` oracles — all die when Item 4 deletes the
  lex.rs `comment_at`/`literal_at` defs they call). Census now counts its 2 lexer calls as ORACLE
  (R12), consistent with this. So **do not mark Item 2 "hand-path fully retired"** — only the
  close_brace capability is retired this commit.
  (b) **`close_brace` still contains a native `{}` depth-counter** (a literal-aware Dyck-1 walk;
  counted under `HAND_SCAN_LOOPS`, the C1 ratchet — not overclaimed). Per guardrail 4 (recognition
  = counting/balancing → sub-system) it must become a system. FILED as **item "BodyBalance"** — an
  OpaqueScan-composing brace counter (skip opaque via `opaque_at`, count `{`/`}`), the Segmenter's
  body-structure recognizer. To build alongside the remaining Segmenter work, before campaign
  close. Named owner recorded; not unowned residue.

- *2026-07-17* — **Item 3 DESIGN (inline synthesis; two design subagents stalled early).** The
  fsm-designer + compiler-architect agents both stalled ~early (architect still opening prior-art
  files) with no doc + no completion after ~50 min — stopped and re-derived inline from a full
  read of machine.rs + prior-art systems. Design → `_scratch/item3_grammar_design.md`. Findings:
  the grammar is **(regular per-level dispatch) ∘ (Dyck-1 counter) ∘ (OpaqueScan)** — no true PDA
  at any layer; `balanced`/`matching_brace` is a literal-aware Dyck-1 counter (composes
  `skip_opaque`), the SAME machine as `close_brace`'s counter (the BodyBalance residual) and
  stronger than `ParenBalance` (which skips only `"` via `skip_string`). Consolidation:
  **ONE `DelimBalance` system** (opaque-aware Dyck-1 over a configurable open/close pair, cursor-
  bounded by a `limit`) retires `balanced` + `matching_brace` + `close_brace`'s counter — i.e.
  **discharges the BodyBalance named-residual.** Sub-capabilities: **3a** skip_opaque→opaque_at
  (DONE); **3b** DelimBalance; **3c** the dispatch walks (machine_section/state/handler_at/body),
  decided per-walk against the `segment()` precedent (a native driver over sub-systems may be
  legitimate — judge after 3b). **OPEN (verify with frame-fsm-expert before 3b):** is the
  @@[scan] dialect expressive enough for a multi-param constructor (`target,open,close,limit`),
  a domain-`limit` cursor bound, and configurable delimiter bytes — else escalate, do not
  hand-roll. I1 (byte-coverage/partition) is the 3c invariant: span-partition differential +
  `check_coverage`.
- *2026-07-17* — **Item 3a EXECUTED (implementer).** `machine.rs::skip_opaque` rewritten off the
  hand `Lexer` onto `opaque_at`, kind-aware limit policy (comment CLAMPS via `.min(limit)`,
  literal REJECTS on overrun, None/Unterminated→None); 5 call sites pass `bytes,…,lx.target()`.
  `skip_opaque_hand` added as the `#[doc(hidden)]` oracle. Differential (`skip_opaque_tests`,
  test-author): 4 green, NO mismatch at every position × every limit × 4 targets; clamp+reject
  teeth fired non-vacuously (corpus 238/842, fuzz 780/4887). Full suite green. Production
  hand-lexer recognition **13→11**, oracle 8→10 (transient, `skip_opaque_hand`).

- *2026-07-17* — **Item 3b EXECUTED (build + machine.rs wire).** DelimBalance expressibility PROVEN
  by a compiled probe (`_scratch/delim_probe.frs` → clean Rust; `scan_at` resets only cursor/depth,
  so ctor-config `open`/`close`/`limit` survive per scan — Q1/Q2/Q3 all YES, no escalation).
  NEW system `delim_balance/{delim_balance.frs,.gen.rs}`: `DelimBalance(target,open,close,limit)`,
  opaque-aware Dyck-1 counter composing OpaqueScan via the `opaque_skip` leaf (grammar limit
  policy: comment clamps, literal rejects on overrun — the walk stays in OpaqueScan, D3). mod.rs:
  `balanced()` wrapper + `balanced_hand()` `#[doc(hidden)]` oracle. `machine.rs::balanced` now
  delegates to `delim_balance::balanced` (hand counter loop GONE); `matching_brace` routes here.
  Build clean; D2 fixpoint byte-identical across rebuild; full suite green. **SYSTEMS 15→16;
  production HAND_SCAN_LOOPS 86→81** (the balanced counter retired). Differential (test-author,
  running) + warden GATE next. close_brace's `{}` counter (BodyBalance) discharge = a follow-on
  bite (same system, mod.rs).
- *2026-07-17* — **R13 (tooling refinement; warden to confirm).** Extended R12's oracle-bucketing
  to `HAND_SCAN_LOOPS`: a `while` loop inside a `*_hand`/`hand_*` differential oracle is transient
  scaffolding (deleted at C-final), so the census now reports HAND_SCAN_LOOPS split production
  (C1 ratchet) vs oracle. Rationale: retiring a production walk into a system must not read as
  no-progress just because the `*_hand` oracle keeps a copy of the loop (3b removed `balanced`'s
  production loop but `balanced_hand` re-adds one). Post-3b: production loops 81, oracle 5
  (balanced_hand, close_brace_hand, hand_item_starts, frame_stmt_hand, …). No production behavior
  change. Same conservative direction as R12 (only named oracles excluded; nothing production
  hides as oracle).

- *2026-07-17* — **BodyBalance DISCHARGED (Item 2 residual closed; Mark chose Option A).** Empirical
  check before forcing the consolidation (warden's condition 2) caught that `close_brace` and
  `balanced`/DelimBalance have DIFFERENT unterminated-opaque policies — `close_brace` FAILS on an
  unterminated body (safe; a `}` buried in a never-closed string can't spuriously close it),
  `balanced` TOLERATES (treats it as bytes, keeps counting). Not the same machine — a real finding,
  surfaced to Mark. **Decision (Mark): Option A — parameterize DelimBalance** with a `fail_unterm:
  bool` config (bool ctor param verified expressible; survives scan_at) + an `opaque_unterminated`
  leaf. `balanced()` passes false (TOLERATE, unchanged); new `balanced_strict()` passes true (FAIL).
  `close_brace` now delegates to `balanced_strict` — its hand `{}` counter loop GONE (BodyBalance
  discharged; the Item 2 residual is closed). Byte-behavior preserved: close_brace_tests 7→8 green
  (added `fail_unterm_policy_is_load_bearing` — proves the flag BITES: on `{ "unterminated }`,
  FAIL→Err while TOLERATE→Some, and close_brace picks FAIL, matching close_brace_hand). D2 fixpoint
  stable; full suite green. **Production HAND_SCAN_LOOPS 81→80** (close_brace's counter retired).
  No new system (DelimBalance extended, not duplicated).

- *2026-07-17* — **BodyBalance cleanups (from-scratch source review + Mark "best methodical thing").**
  #1 DRY: `balanced`/`balanced_strict` folded onto a private `run(…, fail_unterm)` helper (was two
  copies differing only in the bool). #2 dual-arm: added `balanced_strict_hand` (the INDEPENDENT
  FAIL-policy hand oracle — opaque_at_hand + skip_opaque_hand, no OpaqueScan) and a full STRICT
  differential arm in tests/delim_balance.rs — `balanced_strict==balanced_strict_hand` over BOTH
  pairs, every opener × every limit (incl. limit<len) × 4 targets + fuzz, closing the gap that
  balanced_strict was only proven transitively via close_brace (`{}`, limit=len). Teeth: FAIL
  diverges from TOLERATE non-vacuously (14 explicit + 127 fuzz: TOLERATE Some vs FAIL None). Tests
  15→21 (delim); full suite green. Behavior-preserving (DRY) + test-only additions. Metrics:
  oracle HAND_SCAN_LOOPS 5→6 (balanced_strict_hand, transient). Residual note (maintenance, not
  correctness): the STRICT arm's CURATED_CORPUS mirrors the TOLERATE arm's inline strings — a
  future consolidation could share one const.

- *2026-07-17* — **Item 3c DESIGN (inline; segmenter precedent).** The `segmenter` system is the
  pattern: a `@@[scan(u8)]` system walks a span and ACCUMULATES boundary positions into a
  `Vec<usize>` domain field, leaves do the transformations (extent-skip via DelimBalance,
  opaque-skip), and a thin NATIVE driver builds the AST nodes from those positions (the
  recognition/construction seam). 3c applies this to the three inner dispatch loops; linear
  header-parsing/tokenization stays native. Three bites, lowest-risk first: **3c-1**
  `machine_section` → a system accumulating `$Name` state-start positions (skipping opaque +
  skipping each state body via a `state_end` leaf); machine_section becomes a driver building
  Trivia+State nodes. **Safety:** extract `state_extent(bytes,at,limit,target)` used by BOTH
  `state()` and the `state_end` leaf so the found boundary and the built extent cannot drift.
  **3c-2** `state`'s member loop → `(pos,kind)` system + handler_at-extent leaf. **3c-3** `body`'s
  statement loop (already StmtScan-dispatched) + brace-depth. Per bite: system + native driver +
  `*_hand` oracle + differential (positions match the hand walk at every input) + **I1 byte-
  partition check** (node spans identical to the hand walk, `check_coverage` green) + warden gate.
  Expressibility already proven (segmenter uses a `Vec<usize>` accumulator + config `target`;
  DelimBalance proved multi-param config + `limit` bound) — no new dialect unknowns for 3c-1.

- *2026-07-17* — **Item 3c-1 EXECUTED (machine_section → MachineWalk system).** NEW system
  `machine_walk/{machine_walk.frs,.gen.rs}`: `MachineWalk(target, limit)` walks a `machine:` span
  and accumulates `$Name` state-start offsets into a `Vec<usize>` (the Segmenter accumulator
  pattern), skipping opaque + each state body via a `state_end` leaf. mod.rs: `state_starts()`
  wrapper + `state_starts_hand()` oracle + leaves (skip/is_state_start/state_end/record).
  `machine.rs::machine_section` rewritten as a thin NATIVE DRIVER over the positions (builds
  Trivia+State nodes). **Drift-safety:** extracted `machine::state_extent(bytes,at,limit,target)`
  = the (open,end) state extent, used by BOTH `state()` (node) AND the `state_end` leaf (walk) —
  one source, verified behavior-preserving (full suite green before + after). `skip_opaque` →
  pub(crate) for the leaf. Regen + D2 fixpoint byte-identical; SYSTEMS 16→17; full suite green.
  **HONEST CENSUS NOTE:** production HAND_SCAN_LOOPS stayed 80 — the machine_section DISPATCH walk
  (recognition) moved into the system, but `state_extent`'s two extracted LINEAR leaf scans
  (name-skip, brace-find) are legitimate native tokenization (Category-A-ish, no dispatch/nesting),
  not recognition walks, and the `while <ident> <` proxy can't tell them apart. The conversion is
  real (dispatch → system, SYSTEMS +1); the proxy is coarse here. Differential
  (state_starts==state_starts_hand + I1 check_coverage, test-author running) + warden GATE next.

- *2026-07-17* — **Item 3c-2 EXECUTED (state's member loop → StateWalk system).** NEW system
  `state_walk/{state_walk.frs,.gen.rs}`: `StateWalk(target, limit)` walks a state BODY and
  accumulates member-start offsets (`$.x` state var OR handler head) into a `Vec<usize>`, skipping
  opaque + each member's extent. mod.rs: `member_starts()` wrapper + `member_starts_hand()` oracle
  + leaves (skip/member_end/record). `state()`'s member loop rewritten as a native DRIVER.
  **Drift-safety:** extracted `machine::handler_head` (the handler header parse) — `handler_at`
  builds the node from it AND the walk's `handler_end` leaf reads its `.end`; `to_end_of_line`/
  `handler_end` exposed pub(crate). **DRY finding from 3c-1 RESOLVED:** `state_extent` now returns
  `(name_end, open, end)` and `state()` keys its name/params/parent/extent off it — the duplicate
  name-skip is gone. Three verified steps, each behavior-preserving (suite green after each): (1)
  state_extent 3-tuple + state() header restructure, (2) handler_head extraction, (3) StateWalk +
  driver. Regen + D2 fixpoint byte-identical; full suite green. **Metrics moved this time:**
  production HAND_SCAN_LOOPS **80→78** (member loop + duplicate name-skip both retired), SYSTEMS
  **17→18**; oracle loops 7→8 (member_starts_hand, transient). Differential
  (member_starts==member_starts_hand + I1 check_coverage, test-author running) + warden GATE next.

- *2026-07-17* — **Item 3c-3 DESIGN (fsm agents; Mark delegated the fork to them).** The fork —
  BodyWalk system (A) vs native-driver-signoff (B) — was put to the two fsm agents (narrow briefs;
  no stall). **frame-fsm-expert:** (A) is EXPRESSIBLE (compiled probe `_scratch/body_probe.frs`) —
  one system can hold a `depth: i32` counter AND a `Vec<(usize,i32)>` accumulator; a leaf can take
  live `self.depth`; already-shipped machine class (DelimBalance counter + segmenter accumulator),
  no escalation. **frame-fsm-designer:** verdict CONVERT (A), and it REFUTED the double-construction
  premise (my inline lean to B was WRONG): frame_call extent = `consume_terminator(balanced(...))`,
  frame_assign extent = `to_end_of_line(rhs_start)` — NOT from native_parts (which only fills the
  node's expr/rhs field). So a light native_parts-free extent leaf is achievable via the 3c-2
  `handler_head` mechanism (factor `frame_call_head`/`frame_assign_head`) — no build-and-discard,
  no drift. **Clincher:** body()'s brace-DEPTH counter is stateful across the whole body; a native
  driver would have to re-scan the water counting braces = a NEW Category-B hand loop (guardrail-4
  forbidden) — so the counter REQUIRES the system. body() is NOT the segment() precedent (it runs
  the dispatch loop + counter by hand, like machine_section/state() pre-conversion). **DECISION:
  (A) BodyWalk**, built the clean 3c-2 way. `BodyWalk(target, limit)`: `depth: u32` counter +
  `starts: Vec<(usize, i32, u32)>` (start, kind, depth) accumulator; `stmt_end` leaf dispatching
  the 3 kinds construction-free; body() → a segment()-shape driver re-running the shared heads +
  doing the native native_parts/field extraction. Analyses: `_scratch/item3c3_body_*.md`.

- *2026-07-17* — **Item 3c-3 EXECUTED (body's statement loop → BodyWalk system).** NEW system
  `body_walk/{body_walk.frs,.gen.rs}`: `BodyWalk(target, limit)` — the FIRST system fusing a
  segmenter ACCUMULATOR (`starts: Vec<(usize, u32)>` = (start, depth)) with a DelimBalance-style
  running COUNTER (`depth: u32` over native water, opaque-skipped). Walks a handler body, records
  each Frame-statement start + the brace depth there, skips opaque + each statement's extent.
  mod.rs: `stmt_starts()` wrapper (returns the pairs + final depth) + `stmt_starts_hand()` oracle
  + leaves (skip/stmt_end/record). `body()` rewritten as a native DRIVER over `(start, depth)` +
  final depth. **Drift-safety (3c-2 mechanism, twice):** factored `frame_assign_parse`
  (+ `frame_assign_end`) and `frame_call_parse` (+ `frame_call_end`) — the builders build from the
  heads AND the walk's `stmt_end` leaf reads their extent; both `native_parts`-FREE (the designer
  refuted the double-construction premise — extent = `consume_terminator(balanced)` /
  `to_end_of_line`, never native_parts). `frame_stmt` uses `stmt_scan::classify`. Three verified
  steps, each behavior-preserving (suite green after each): (1) frame_assign head, (2) frame_call
  head, (3) BodyWalk + driver. Regen + D2 fixpoint byte-identical; full suite green (body() drives
  EVERY handler body — heavy exercise). **Metrics:** production HAND_SCAN_LOOPS **78→77** (body's
  dispatch loop + the hand brace counter both retired into the system), SYSTEMS **18→19**; oracle
  loops 8→9 (stmt_starts_hand). **This CLOSES Item 3's dispatch-walk conversion (3c-1/3c-2/3c-3).**
  Differential (stmt_starts==stmt_starts_hand incl. depth pairs + final depth, teeth on depth
  variance/nonzero-final/brace-in-string-ignored + I1, test-author running) + warden GATE next.

- *2026-07-18* — **DESIGN GATE BATCH — owner rulings (first three units through the RFC-0058
  pipeline: finder inventory → fsm-designer → warden review → owner gate).**
  (1) **DeclWalk/DeclRead ACCEPTED as Item 3d.** Item 3's ledger "COMPLETE" is rescoped to
  "dispatch-walks complete" — decl_section/decl_of (the fourth section walk + line reader) were
  in Item 3's declared scope and are now a named item. Conditions folded: ledger row **T15**
  (the `saturating_sub(1)` clamp at machine.rs:833 + inverted-span debug panic at :844 — CARRY
  with a written reachability argument + battery coverage); the Phase-A `params_close`
  bare-counter leaf is a **guardrail-4 exception with a bounded lifetime: GATE-B does not close
  until Phase B (T9 → DelimBalance routing) lands**; T9/T13 land as recorded behavior-change
  deltas with directed tests (`body_open_at`'s unbalanced-params fallback pinned).
  (2) **StateHeadScan/HandlerHeadScan ACCEPTED as Item 3e** (the head grammar). Conditions
  folded: T-S3 widened to name the arrow-in-opaque phantom parent (with a Phase-1 pin); the
  state-side `close_node` clamp artifact recorded on T-S1/state_head_driver; each Phase-2 delta
  lands with its own Change Log entry.
  (3) **ArgScan RETURNED with recorded direction — Option C, "fork and adjudicate":** the
  AngleProbe look-ahead guess is replaced by a dual-counter scan (angles-as-brackets vs
  angles-as-operators), an explicit fork when the two comma sets diverge, adjudicated downstream
  by the target system's DECLARED ARITY (Frame-side knowledge only; the two splits always differ
  in count so a tie is impossible; neither-matches → a diagnostic showing both readings). The
  hypothesis is binary per argument list; mixed lists fail loudly; the escape hatch is
  parenthesization. The same mechanism is filed as the future path for transition args
  (converging the machine.rs:1003 no-split ruling later). Revision in flight.
  (4) **Campaign default declared: PARITY-FIRST.** Reproduce hand behavior first (bugs included,
  pinned), fix as separate recorded deltas. Fix-at-landing is a per-capability exception recorded
  in this log — ArgScan is the first, carried by its partitioned carry/fix differential and
  `oracle_stayed_buggy` anti-vacuity machinery. Bugs are logged as data in the design's graph
  node (ledger rows + deltas), never only in prose.

- *2026-07-18* — **ArgScan (Option C) ACCEPTED at owner gate** (re-gate PASS-WITH-CONDITIONS;
  conditions folded as dated amendments in the design record + shard). The recorded
  tie-impossibility ruling is **refined**: the two fork candidates always differ in count, and
  adjudication by declared arity is decisive **provided named coverage is required** (every
  declared parameter not provided by name has a default); with defaults, BothAdmissible is
  reachable ⇒ **E407 — diagnose, never guess**. Named-coverage clause + run-initial Lemma 3(i)
  restatement + minors folded. ArgScan joins 3d/3e as a build-ready unit; the NativeParts
  design (Item 4 core) is at the owner gate.

- *2026-07-18* — **Item 4 NativeParts design ACCEPTED at owner gate** (warden PASS-WITH-CONDITIONS;
  conditions folded as dated amendments: the false `scan_at` seam claim corrected — `from` becomes a
  ctor config param since generated machines reset all literal-initialized fields; ledger tally 14;
  R10 weighed, not triggered). **Owner rulings recorded: DP-1** — an unterminated literal's rescued
  interior is ONE plain Text run (no diagnostics channel in native_parts); **H-1** — the validator
  owns `@@:` membership (scanner = shape + word boundary + Unknown-as-data; validate.rs diagnoses
  non-membership in the context arcanum). With 3d, 3e, and ArgScan, the accepted set now covers the
  section walker through the islands; NativeParts completion retires the LAST production
  hand-Lexer recognition (the C2 path).

- *2026-07-18* — **HYGIENE BITE LANDED: regen fixpoint restored + made standing.** The 11
  pre-campaign `.gen.rs` (embed/hsm_cycle/inst/native_parts_scan/paren_balance/reachability/
  ref/section/segmenter/string_counter/string_scan) were stale against the current framec-ng
  codegen (typed compartments) — C4 was false and R8's "standing check" had never been built
  (found independently by two assessment agents, warden-confirmed). Re-blessed via NEW
  `tools/regen_check.sh` (check mode = the standing R8 predicate, exit 1 on any stale file;
  `--bless` rewrites); fixpoint verified stable across regen→rebuild→regen; full suite green
  (32/32 test binaries, 0 failures). C4 is TRUE again and now continuously checkable.

- *2026-07-19* — **Item 3d DeclWalk/DeclRead LANDED (M-wire complete; hand path deleted).**
  `decl_section` is now the thin native driver over `decl_walk::decl_starts` + the shared
  `decl_extent` head + `decl_read::member_decl_of`; `state()`'s state-var branch routed to
  decl_read; sections.rs unchanged (signature stable, verified). DELETED: the hand `decl_of`
  (~115 lines incl. its bare params counter) and `matching_brace` (subsumed by `decl_extent` on
  the same DelimBalance); verbatim copies live on as `decl_of_hand`/`decl_starts_hand`,
  oracle-only (test callers only, grep-proven). Suite 277/0 on the NEW path incl. the I1
  partition test; regen fixpoint 21/0 across a rebuild of the wired binary — **framec-ng's own
  decl scanning now runs through DeclWalk/DeclRead and re-scans all 21 `.frs` sources
  byte-identically (the self-scan hazard did not bite)**. **Census, both movements stated:**
  SYSTEMS 19→21; production HAND_SCAN_LOOPS 77→73 = machine.rs retired ~9 (the decl dispatch
  walk + decl_of's tokenization) MINUS decl_read's +5 transient leaf surface (4 linear
  tokenization leaves, 3c-1 census-proxy species, + the `params_close` bare counter — the
  recorded guardrail-4 exception that dies at Phase B); oracle loops 9→17 (transient, C-final
  sweep owns them); hand-Lexer recognition untouched at 11 (Item 4's surface). **GATE-B held
  open until Phase B (T9 → DelimBalance) per the recorded exception.**

- *2026-07-19* — **Item 3e StateHeadScan + HandlerHeadScan LANDED (built in isolated lane
  worktree; M-wire complete; hand head parse deleted).** `state()` builds its node from ONE
  `state_head_scan::scan` run; `state_extent` is the extent projection of the same run (walk
  leaf and node driver single-source); `handler_head` is the thin adapter over
  `handler_head_scan` with `handler_end`/`handler_at` as projections. DELETED: 10 hand `while`
  loops (parent hunt ×3, handler head parse ×5, state_extent name-skip/seek ×2); verbatim
  copies survive as `state_head_hand`/`handler_head_hand`, test callers only (grep-proven).
  T-S8/T-H9 debug_asserts live in the adapters. Suite 36 binaries 309/0 on the wired path incl.
  both anti-drift gates and both every-position differentials; regen fixpoint 23/0 across a
  rebuild — framec-ng re-scans its own state/handler heads through the new systems
  byte-identically. **Census:** SYSTEMS 21→23; production HAND_SCAN_LOOPS 73→63 (−10, per-fn
  exact, zero transient additions); recognition flat 11/10. **Design-record sync (GATE-A
  carry):** T-S5's design example (`=> b` phantom) is impossible in the hand code
  (machine.rs `$`+name-start guard); the battery pins BOTH real faces — `=> $b` phantom AND the
  lost real `=> $Real` after `)` — recorded here as the dated correction, with the §2.1
  `state_head/`→`state_head_scan/` naming. **Phase-2 obligation restated:** deltas D1 (opaque
  seeks), D2 (params-skipping hunt), D3 (limit-bounded probe) each land with their own Change
  Log entry. Census-proxy hardening (call-condition loop blindness, scan_census.py:44) owed
  before C-final.

- *2026-07-19* — **ArgScan LANDED (built in isolated lane worktree; rebased cleanly onto the 3e
  landing — zero conflicts, the graph's lane-disjointness prediction held; one production seat).**
  `inst_scan::scan_node(bytes, i, target)` carries the Option C machine: the two-counter
  ArgScan system (adepth at bracket-depth-0 only, digraph guards, refusal-supersedes-fork),
  the wrapper's merge_g fold, the tree's `Instantiation.angles` field, and the adjudication
  seam (`validate.rs::adjudicate` + E407 [code provisional] + the driver consult; unresolved
  name renders primary-G). DELETED: `parse_inst_args`, `split_top_commas`, `split_top_eq`
  (verbatim `_hand` oracle copies remain, test/oracle callers only, C-final owns them).
  **The 12 recorded deltas of design §11.8 land with this entry** (incl. D-fork-adjudicate,
  D-tree-angles, D-e407, D-adjudication-seam, D-seam-target) — fix-at-landing per this set's
  recorded D4-shape exception, carried by the partitioned carry/fix differential (carry proven
  == hand on 40 curated + 8000-seed fuzz ×4 targets; all 18 fix teeth != hand). **Bug B(iii)
  (the enter-sigil find, builder-found + warden-reproduced on the live oracle):** the hand
  splitter counted the `>` of `$>(`, mangling non-final enter groups (spec examples 4/6 = 2
  args today); fixed at landing, `oracle_stayed_buggy` pins all THREE bug families both-faces.
  Suite 342/0 (-p) / 1928/0 (workspace) wired; regen fixpoint 24/0 across rebuild (post-rebase
  composed state with 3e). **Census (composed, from the 3e baseline 63):** production
  HAND_SCAN_LOOPS 63→58 (parts.rs 14→10, inst_scan 2→1); oracle 25→29 (+4 transient);
  recognition untouched 11/10; SYSTEMS 24.

### Audit Log (append-only — warden verdicts)
- *2026-07-19* — GATE-A (pre-wire), ArgScan: **PASS-WITH-CONDITIONS — wiring proceeded.**
  D1–D4,D8 run-verified (batteries 57+8 name-for-name vs §11.7; carry == hand proven; fix teeth
  != hand; oracle copies verbatim-diffed; scan_node_sys zero production callers). Beyond-design
  find warden-reproduced on the live oracle (the `$>(` sigil miscount). 7 builder deviations
  faithful; fork-rule order: code right, design prose corrected (roster-required order).
- *2026-07-19* — GATE-B, ArgScan: **PASS pending commit → LANDED.** One seat verified
  (scan_node+target; injection bridge gone; pin_mixed_list_e407 asserts on the
  production-parsed node); hand splitters deleted, `_hand` verbatim oracle-only;
  oracle_stayed_buggy pins Bug A + B + B(iii) both faces on the live oracle; suite 342/0 (-p)
  and 1928/0 (workspace); regen 22/0 ×2 pre-rebase and 24/0 post-rebase; census exact
  (73→68 in-lane; 63→58 composed); self-scan note verified honest. Conditions discharged in
  this commit: this landing entry + ledger row 4; shard exercises B(iii) sync + stale "One"
  labels + header wording fixed.

- *2026-07-19* — GATE-A, Item 3e "StateHeadScan+HandlerHeadScan": **PASS — proceeded to M-wire.**
  D1–D4,D8 verified by run/grep: build 0-warn; regen 23/0-stale stable across rebuild; batteries
  17+15 green in suite 36-binary/309/0 (every-position parts-struct rectangles ×4 targets; one
  test per ledger row; H1 phantom-parent pin; H2/T-H5 close-byte-not-`}` asserted; teeth on every
  register incl. reject reasons 1–4; fuzz 3000×4; anti-drift system==node==state_extent through
  segment()); Phase-2 deltas correctly absent. Six builder deviations judged faithful — T-S5
  resolved in favor of the code; census brace-literal mechanic verified; ret_byte proxy-blindness
  verified oracle-only.
- *2026-07-19* — GATE-B, Item 3e: **PASS pending commit → LANDED.** D5/D6/D7/D2 + carry-items
  verified: projections single-source per side, 10 hand whiles deleted / 0 added, oracles
  test-only, suite 309/0 wired, regen 23/0 across rebuild (self-scan), census honest 73→63,
  debug_asserts live. Conditions discharged in the atomic commit: this landing entry (incl.
  T-S5 both-faces sync) + 6 stale pre-wire doc lines corrected. Census-proxy hardening carried,
  dated, owed before C-final.

- *2026-07-19* — GATE-A, Item 3d "DeclWalk/DeclRead": **PASS — wiring proceeded; GATE-B held
  until Phase B (params_close exception).** D1–D4,D8 verified by run/grep: regen fixpoint
  21/0-stale across rebuild; decl_walk 15/15 + decl_read 13/13 + suite 277/0 (every-position
  rectangles × 4 targets × both with_bodies, full-struct read differential, one test per ledger
  row T1–T15, teeth threshold-asserted, xorshift fuzz 3000×4, T15 driver-exclusion STATED);
  oracles verified verbatim vs the hand code; zero production callers of new symbols pre-wire;
  params_close annotated in-code as the recorded exception; 3 builder deviations judged
  faithful. Findings: census +5 transient production loops (recorded honestly at landing);
  stray .claude/agents files excluded from the atomic commit.

- *2026-07-18* — DESIGN GATE, Item 4 NativeParts (nativeparts_design.md + 8 shards):
  **PASS-WITH-CONDITIONS → ACCEPTED at owner gate; DP-1 and H-1 ruled.** All load-bearing claims
  source-verified (3× wrong-for-seat on native_parts_scan/; parts.rs:29/:45 = last production
  hand-Lexer recognition; full-buffer-vs-slice clamp/water divergence reproduced; T-N8 {{-phantom
  + T-R2 prefix-overmatch reproduced; 14-row ledger complete; extent-independence attacked and
  held; @@fsm grep 0). Conditions folded at acceptance: ctor-param seam fix (generated scan_at
  resets ALL literal-initialized fields — gen-verified ×4); tally 14 = 8+3+2+1 with the driver
  shard's T-N6 aligned; R10 weighing recorded. Nits scheduled for Commit-A shard sync.

- *2026-07-18* — DESIGN RE-GATE, ArgScan §11 (Option C fork-and-adjudicate): **PASS-WITH-CONDITIONS
  → ACCEPTED at owner gate.** Lemmas 1/2/3(ii) survived adversarial attack; digraph correction
  (the hand counts the `<` of `<=`, parts.rs:400) verified; adjudication seams
  (validate.rs:205, driver.rs:670–675), skip_opaque byte-identity, shard/roster exactness,
  angle_probe deletion, prior conditions A3–A6 all confirmed. Conditions folded at acceptance:
  named-form admissibility gains the unprovided-params-must-have-defaults clause (+
  `adjudicate_named_coverage`); Lemma 3(i) restated run-initial; minors (refusal⇒Inert,
  refusal-4 via $VerbatimTail, fork_g_matches_hand domain, L3/L21 refs, schema header,
  instantiation_at touch-point). Resolution line carries the refined tie-claim.

- *2026-07-18* — DESIGN GATE, DeclWalk/DeclRead (Item 3d): **PASS-WITH-CONDITIONS → ACCEPTED at
  owner gate.** All cited file:line facts warden-verified exact (incl. the two-caller pair
  architecture and the T13 fork at :829); sibling conformance real. Conditions (now folded): T15
  ledger addition; Phase-A counter-leaf = guardrail-4 exception, GATE-B held open until Phase B
  lands; Item-3 rescope recorded; T9/T13 as recorded deltas with `body_open_at` fallback pinned.
- *2026-07-18* — DESIGN GATE, Head readers (StateHeadScan+HandlerHeadScan, Item 3e):
  **PASS-WITH-CONDITIONS → ACCEPTED at owner gate.** 18-row ledger complete; the fourth handler
  refusal (:200), T-S9 limit-straddle (:95/:793), T-S5, and the T-S2 driver overrun all
  warden-confirmed real; pump-contract-forced one-$Reject honest; Phase 0/1/2 staging matches
  campaign precedent. Conditions folded: T-S3 arrow-in-opaque widening + pin; state-side
  close_node clamp artifact recorded; per-delta Change Log entries at landing.
- *2026-07-18* — DESIGN GATE, ArgScan (M6 + AngleProbe): **PASS-WITH-CONDITIONS → RETURNED at
  owner gate with direction** (Option C replaces AngleProbe — see Change Log). Warden-reverified:
  Bug A :363/:365, Bug B :400–402, sibling alphabets, seams; 32-row ledger complete. Batch
  verdict: no design conflict between lanes; schema unification (routes-through, carry-and-name,
  deltas-as-hooks) recorded in RFC-0058 §7.2 before graph consolidation.
- *2026-07-18* — GATE-A+B, Item 3c-3 "body statement loop → BodyWalk": **FAIL (D3) → FIXED,
  re-gate pending.** D1,D2,D4,D5,D7,D8,I1,census + all 3 refactors' behavior-preservation PASSED
  (regen byte-identical ×2; 11/11 differential incl. depth pairs+final depth, teeth [0,1,2,0]/
  final-2/brace-in-opaque non-vacuous; SYSTEMS 18→19, prod loops 78→77; BodyWalk honest
  counter+accumulator, no PDA). **D3 FAIL (warden caught a real drift defect):** the `stmt_end`
  leaf decided the frame_stmt extent via `frame_stmt_classify` = the StmtScan HAND ORACLE, not the
  `stmt_scan::classify` SYSTEM the driver's `frame_stmt` uses — wiring a retired hand recognizer
  into production + contradicting the leaf's own comments. **FIX (warden-prescribed one-liner):**
  body_walk/mod.rs stmt_end → `stmt_scan::classify` (drop-in, StmtScan-proven-equal, strictly
  safer — walk-found and driver-built frame_stmt extents now truly single-source; hand oracle
  removed from production, frame_stmt_classify now has zero production callers). Rebuild + body_walk
  11/11 + full suite green + census unchanged. Re-gate to confirm D3 PASS, then land.
- *2026-07-18* — GATE-A+B RE-GATE, Item 3c-3 "body statement loop → BodyWalk": **PASS pending
  commit.** D3 FIX confirmed — stmt_end leaf calls `stmt_scan::classify` SYSTEM, single-source with
  the driver's frame_stmt; frame_stmt_classify has ZERO production callers (only the StmtScan
  oracle test). D1/D2/D4/D5/D7/D8/I1 re-verified green (regen byte-identical; body_walk 11/11; full
  suite 0-failed); census flat (SYSTEMS 19, prod loops 77, prod recognition 11). **Closes Item 3's
  dispatch-walk conversion (3c-1 MachineWalk + 3c-2 StateWalk + 3c-3 BodyWalk — all three inner
  dispatch walks are now @@[scan(u8)] systems).** Stale doc-ref (tests/body_walk.rs:21) fixed. D9
  open → land the 7-artifact atomic commit.
- *2026-07-17* — GATE-A dry-run, Item 1 "opaque skip": **FAIL** (D4 no fuzz) / GATE-B **FAIL**
  (D5/D6 surviving hand consumers close_brace + machine.rs::skip_opaque; D9 uncommitted). The DoD
  bit on real, incomplete work.
- *2026-07-17* — GATE-A, Item 1 "opaque skip" (OpaqueScan/RawString/BraceBalance): **PASS.**
  D1–D4,D8 verified by run/grep; regen byte-identical across rebuild; D4 fuzz arms have teeth
  (accepts>200, raw>5, hole>5); machines honestly classified as counters.
- *2026-07-17* — GATE-B, Item 1 "opaque skip": **PASS pending commit.** skip_opaque_at + try_island
  wired to the system; every residual hand-lexer caller is an oracle or a named+scheduled residual
  (close_brace→Item 2, skip_opaque→Item 3, native_parts→Item 4); suite green; drift none. D9 (the
  only open predicate) is intentionally uncommitted → LAND the atomic commit.
- *2026-07-17* — GATE-A+B, Item 3c-2 "state member loop → StateWalk": **PASS pending commit.**
  state()'s member dispatch is now a native driver over the StateWalk @@[scan(u8)] system
  (member_starts Vec accumulator); member while-loop + duplicate name-skip retired; 3c-1 DRY
  finding RESOLVED (state_extent→(name_end,open,end)). D1–D8 verified by run/grep — regen
  byte-identical across rebuild; 8/8 differential (member_starts==member_starts_hand every
  (from,close)×4 + fuzz 3000×4, teeth distinct 2494/multi 7525, negative-control non-vacuous); all
  3 refactors behavior-preserving (name_end≡old j; handler_head.end≡matching_brace, both by
  construction; suite exercises state-params+HSM-parent); **I1 proven for the member-driver via
  segment()+check_coverage+unparse+check_total recursion into StateVar/Handler/Trivia**; census
  SYSTEMS 17→18, prod loops 80→78 (REAL retirement, not reclassification), oracle 7→8, recognition
  11. Finding (non-blocking, recorded): member_starts_hand has a latent-UNREACHABLE spin if an
  extent leaf ever returned ==i (guarded by scan_at step-cap; can't happen — statevar advances
  past `\n`, handler needs a found `{`); optional 1-line oracle hardening deferred. D9 open → land
  the 7-artifact atomic commit.
- *2026-07-17* — GATE-A+B, Item 3c-1 "machine_section → MachineWalk": **PASS pending commit.**
  machine_section rewritten as a native driver over the MachineWalk @@[scan(u8)] system
  (state_starts accumulator, Segmenter pattern); dispatch walk retired (grep: no while in
  machine_section). D1–D8 verified by run/grep — regen byte-identical; 8/8 differential
  (state_starts==state_starts_hand every (from,limit)×4 targets + fuzz 3000×4; teeth
  multi/zero/buried-in-opaque all fire); **I1 proven through segment() + check_coverage +
  check_total recursion (Gap/Overlap into State/Trivia) + unparse round-trip**; SYSTEMS 16→17;
  prod loops 80 (coarse-proxy flat, documented+honest — dispatch→system, residual = state_extent
  Category-A tokenization); state_starts_hand = C-final oracle. Findings (non-blocking):
  state_extent duplicates state()'s name-skip (DRY) → RESOLVE in 3c-2 (which restructures state());
  state_extent not independently `*_hand`-gated (covered by extraction-invariance + delim_balance
  + I1). D9 open → land the 7-artifact atomic commit.
- *2026-07-17* — GATE-B (re-grade, final), Item 3b "BodyBalance discharge + cleanups": **PASS
  pending commit.** close_brace→balanced_strict (hand {} counter gone); DRY behavior-preserving;
  balanced_strict now DIRECTLY dual-armed vs independent balanced_strict_hand (hand Lexer, not
  OpaqueScan) — every pos × every limit (incl. <len) × both pairs × 4 targets + fuzz, teeth ≥14
  explicit / >20 fuzz. D1–D8 + R13 verified by run/grep; regen byte-identical; census oracle loops
  5→6, prod loops 80, recognition 11, SYSTEMS 16, no prod mis-bucketed. D9 open → land the 6-file
  atomic commit.
- *2026-07-17* — GATE-A+B, Item 3b "DelimBalance": **PASS pending commit.** machine.rs::balanced/
  matching_brace routed off the hand counter onto the DelimBalance @@[scan(u8)] Dyck-1 counter
  (opaque-aware via OpaqueScan); D1–D8 + R13 verified by run/grep (regen byte-identical across
  rebuild; balanced==balanced_hand every pos × every limit × both pairs × 4 targets, teeth
  non-vacuous opaque_matters≫20; census SYSTEMS 16, prod loops 81, oracle 5; R3 gate makes
  4-target coverage complete). D9 open → land the 8-file atomic commit. Finding (fixed): ledger
  row corrected — BodyBalance/close_brace counter is a NAMED follow-on, not discharged by 3b.
- *2026-07-17* — GATE-A+B, Item 3a "skip_opaque → opaque_at": **PASS pending commit.** Production
  skip_opaque routed off the hand Lexer onto OpaqueScan under a kind-aware clamp(comment)/reject
  (literal) policy; D1,D3–D8 + R12 verified by run/grep (skip_opaque==skip_opaque_hand every pos ×
  every limit × 4 targets, teeth non-vacuous; census production 13→11, oracle 8→10, machine.rs
  prod=0). No drift. D9 open → land the 2-file atomic commit (machine.rs + plan). Ruled: 3b
  proceeds to the frame-fsm-expert expressibility check FIRST (Q1–Q3), escalate-not-hand-roll if
  inexpressible; re-verify the DelimBalance/close_brace consolidation at 3b GATE-A.
- *2026-07-17* — GATE-A+B, Item 2 "close_brace / body-end": **PASS pending commit.** close_brace
  routed off the hand Lexer onto OpaqueScan (`opaque_at`) with a 3-way None/Comment-Literal/
  Unterminated register signal; D1–D8 + R12 verified by run/grep (regen byte-identical across
  rebuild; 4-way `opaque_at`==`opaque_at_hand` + close_brace `is_err`/Ok differentials, both
  fuzz-gated with teeth; production recognition 15→13). D9 uncommitted → land the 10-file atomic
  commit. Findings resolved pre-commit: stale `tests/close_brace.rs` doc refs fixed (→ in-file
  `close_brace_tests`); `hand_item_starts` retirement + close_brace's `{}`-counter→`BodyBalance`
  sub-system deferred with named owners (Change Log above) — Item 2 marked **close_brace capability
  done, NOT hand-path fully retired.**

## Review revisions (frame-fsm-designer + frame-compiler-architect, both grounded in the code)

Two agent reviews stress-tested this plan + Item 1. The machines are classified honestly
(counter automata, not false PDAs), the walk/leaves boundary holds, and regen is byte-identical.
The following findings **amend this plan** and are binding:

- **R1 — the residual consumer list was wrong (5, not 3).** Beyond `parts.rs::literal_node`
  (holes, Item 4) and `machine.rs::skip_opaque` (Item 3), TWO more production/near-production
  consumers still call the hand lexer and were unassigned: `mod.rs::close_brace` (:455,
  extent-only, runs in production via `read_pragma`) and `native_parts_scan::try_island`
  (:22-25, the "converted" NativeParts system's own leaf re-calls the hand lexer, and the
  system is **not even wired to production** — an unwired hybrid). So the hand lexer is NOT
  deletable "after Items 3+4" as first written.
- **R2 — collapse the extent-only consumers onto OpaqueScan NOW (finishes Item 1).**
  *(SUPERSEDED by Change Log "R2 REFINED" (2026-07-17): only `try_island` is truly extent-only and
  was routed now; `close_brace`→Item 2 and `machine.rs::skip_opaque`→Item 3 need a richer-than-
  extent signal and are named deferrals. The original text below is kept for the record.)*
  `close_brace`, `try_island`, and `machine.rs::skip_opaque` use only the *extent*. Route all
  three to `skip_opaque_at` immediately, each behind the differential gate (one delta to verify:
  `close_brace` returns `Err(UnclosedBody)` via `?` on an unterminated interior comment;
  OpaqueScan returns `i` and the loop falls off to the same `UnclosedBody` — same outcome, gate
  it). This shrinks the dual-recognizer window to the ONE honestly-different capability: **holes**
  (`literal_node`), Item 4.
- **R3 — Item 1 silently narrowed opaque-skip from 16 targets to 4, and `segment()` runs for
  all 16 before the backend gate (`main.rs:69` before `:92`).** For non-core targets OpaqueScan
  under-recognizes (no cpp-raw / heredoc / lua-long / regex / template) while `close_brace` in
  the same parse still uses the 16-target hand lexer — the two halves disagree (reproduced:
  `-l cpp` `R"(…}…)"`, `-l php` heredocs). **DECISION NEEDED (before Item 3):** (a) gate `target`
  to the 4 supported backends *before* `segment()`; (b) make unmodeled-form leaves surface a
  blocker; or (c) expand OpaqueScan + differential to all 16. Until decided, this is documented,
  not silent.
- **R4 — I1/I2 are NOT a scanner gate; the corpus gate is over-credited.** `check_coverage`
  passes for ANY partition (the #214 dual — coverage is blind to a mis-skip), so a green suite
  does not prove skip correctness. The hand-curated differential test is position-exhaustive but
  **input-sparse**. **Add a fuzz/property generator** feeding `agree()` (random bytes + random
  Frame-ish source) — it targets exactly the literal long-tail (#219). State plainly in each
  item that the differential test, not the suite, is the correctness gate.
- **R5 — the gate must become STRUCTURAL before Items 3/4.** "assert `machine(i) == hand(i)` at
  every position" fits a single-extent probe. Items 3/4 emit `Vec<Part>`/nested nodes; the gate
  must compare the full structure, and adversarial input curation becomes load-bearing (per-
  position sweeping no longer enumerates the space). Design the structural differential + a
  nesting/water input corpus *before* authoring Item 3.
- **R6 — parity proves sameness, not correctness; Item 4 must add correctness fixtures.**
  BraceBalance shares `hole_at`'s not-string-aware behaviour, so a hole whose code holds a string
  containing `}` mis-delimits identically in both — a green test, a wrong extent. Harmless while
  holes are only *skipped* (Item 1), but Item 4 makes holes into `LiteralPart::Hole` nodes read
  as code. Item 4 MUST add correctness fixtures (holes containing target strings/brackets) OR
  make the hole scanner string-aware (compose OpaqueScan inside BraceBalance — a real capability
  step, not a refactor).
- **R7 — `block_close` must be table-driven** (opaque_scan/mod.rs:59 hardcodes `*/`; the `.frs`
  advances `+2` not opener/closer length). Correct for the 4 core targets, wrong the instant a
  target with a different block-comment close (Ruby `=end`) becomes core. Fix to match the form
  table like `block_open_len`.
- **R8 — bootstrap fixpoint is a guardrail.** framec-ng IS frame-compiler; `segment` now runs
  OpaqueScan to parse `.frs` files including `opaque_scan.frs`. Every regen MUST be: regenerate →
  rebuild → regenerate → assert byte-identical `.gen.rs`, from a known-clean binary. (Verified
  byte-identical today; make it a standing check.)
- **R9 — multi-target string forms are unnamed future sub-systems.** The hand lexer also does
  `LuaLongBracket`, `PhpHeredoc`, `RubyHeredoc`, `RubyPercent`, `Template`, `CppRaw`,
  `RegexLiteral` (regex is context-sensitive — needs previous-token state). Each is a Category-B
  sub-system when targets grow; name them so "delete the hand lexer" is never blocked by a silent
  parity gap.
- **R10 — composition is always via a native leaf-wrapper** (`raw_scan`/`hole_skip`/`skip_string`
  each carry `unwrap_or(i)` + a guard). Thin, but it multiplies with every composition in
  Items 3-4. **DECISION:** whether direct Frame-level system→system invocation is a framec
  capability worth building before the wrapper count grows.
- **R11 — machine-design checklist for Items 3/4** (confirm per function, do not assume):
  kind-matched brackets → `push$`/`pop$` PDA, not one counter; first-token statement dispatch is a
  FUNCTION not a machine; precedence expressions are recursive descent not a machine; `@@Sys(args)`
  / `@@:self.f.m()` arg chains are pushdown.

## The item plan (ordered; grounded in the real consumer map)

Dependencies are real: a hand function is deletable only when *every* consumer is converted,
and consumers are spread across items. The order below is the roadmap order, annotated with
what each item **retires**.

### Item 1 — OpaqueScan: the string/comment SKIP recognizer  ✅ CORE DONE
- Systems: `OpaqueScan(target)` (full per-target string+comment skipper), `RawString` (rust
  `r#*"…"#*` counter), `BraceBalance` (`{}` counter for python holes). All authored,
  generated, unit- and differential-tested; `skip_opaque_at` runs `OpaqueScan`; full suite
  green.
- **Retires (eventually):** the hand `Lexer` recognition — `comment_at`, `literal_at`,
  `quoted`, `triple_quoted`, `rust_raw`, `block_comment`, `hole_at`.
- **Residual (NOT yet deletable) — FIVE consumers (see R1/R2), not three:**
  - EXTENT-ONLY → route to OpaqueScan NOW to close Item 1 (R2): `mod.rs::close_brace` (production,
    via `read_pragma`), `native_parts_scan::try_island` (unwired system leaf), and
    `machine.rs::skip_opaque`.
  - `parts.rs::literal_node` needs `literal_at`'s **holes + delim** to build `LiteralPart::Hole`
    nodes (holes are NOT dead) → **Item 4**; this is the one honestly-different residual.
  - `sections.rs::hand_*` / `mod.rs::hand_item_starts` are the Segmenter/SectionScan differential
    **oracles** → self-contained in **Item 2**.

### Item 2 — Segmenter (item-level discovery)
- Already a production system (`segmenter.frs`, drives `segment`). **Retires:** the
  `hand_item_starts` oracle → make its differential test self-contained; drop the hand walk.
  Also lets `skip_opaque_at_hand` (Item 1's oracle) retire if no other oracle needs it.

### Item 3 — the Frame grammar: SectionBackbone / StatementScanner / TransitionHead
- Converts `machine.rs` — `decl_section`, state scan, `handler_at` (section/state/member outer
  grammar), `frame_stmt` dispatch, `parse_after_arrow`/`balanced`/`to_end_of_line`.
- **Retires:** `machine.rs::skip_opaque` (its string/comment skip → `skip_opaque_at`), and the
  hand grammar loops. `StatementScanner`/`stmt_scan` is already a system; this finishes the
  outer grammar around it.

### Item 4 — the water islands: NativeParts / RefScanner / CallSiteScanner
- Converts `parts.rs` — `native_parts` (decompose water), `frame_ref_at`, `instantiation_at`,
  `embed_call_at`, `split_top_commas`/`match_paren`. `ref_scan`/`inst_scan`/`embed_scan` are
  already systems; this converts the `native_parts` **driver** and `literal_node`.
- **Retires the last hand-Lexer users** → `comment_at`/`literal_at`/`quoted`/`triple_quoted`/
  `rust_raw`/`block_comment`/`hole_at` become deletable. Holes get produced by a
  `BraceBalance`-backed pass (the system already exists from Item 1), delim from `OpaqueScan`.

### Item 5 — validators: ReachableWalker / SectionOrder / HsmCycleWalker
- `hsm_cycle` and `reachability` are already systems; this converts the `validate.rs` E402/E609
  section-order and ref checks that remain hand.

### Item 6 — the EmitDriver walk (LAST)
- The emit body-statement walk (`text/emit/driver.rs`). Per the journal it is a *transducer*,
  not a traversal; converting it is deliberate and last, and the per-backend **spellings**
  stay native regardless.

## Hand-Lexer retirement — the explicit dependency

`comment_at`/`literal_at` and their helpers are deletable **only after**: Item 3 routes
`machine.rs`'s skip through the system, Item 4 replaces `parts.rs::literal_node`
(holes+delim), and Items 2/4 make the segmenter/section oracles self-contained. Until then they
survive as oracles + un-converted consumers — tracked here, not forgotten.

## What "excellent" means for this campaign

- Never claim a conversion done until the hand path is **deleted** and the suite is green.
- Every system carries its differential test; a converted machine is byte-for-byte (or
  behaviorally, over the corpus) identical to what it replaced, proven by running.
- Surface blockers; do not hand-roll around them.
- One capability per commit, reviewable, with its `.frs`/`.gen.rs`/`mod.rs`/test.

## Progress ledger (living)

| Item | State | Systems | Hand path retired? |
|---|---|---|---|
| 1 OpaqueScan | **warden PASS (GATE-A + GATE-B) — committed** | OpaqueScan, RawString, BraceBalance | skip path is the system; try_island routed; residual = holes (Item 4), close_brace (Item 2), machine skip (Item 3), + 3 oracles — all named; batteries+fuzz+milestone green |
| 2 Segmenter | **close_brace capability: warden PASS (GATE-A+B, pending commit).** Body-end recognition off the hand Lexer onto OpaqueScan (`opaque_at`, 3-way signal). Remaining Item-2 scope: `hand_item_starts` oracle (→ C-final sweep) + `BodyBalance` `{}`-counter sub-system (named, before close) | segmenter, +OpaqueScan `kind`/`unterminated` registers | close_brace: yes. Item-2 whole: no (oracle + brace-counter named) |
| 3 Grammar | **Dispatch-walks COMPLETE** (3c-3 d352021). **3d DeclWalk/DeclRead LANDED 2026-07-19** (M-wire done; hand decl_of + matching_brace DELETED; GATE-A PASS; GATE-B held until Phase B). **3e Head readers LANDED 2026-07-19** (lane worktree; hand head parse deleted; GATE-A+B PASS; Phase-2 deltas pending with per-delta entries). 3a (fbde61e), 3b (03671f1), BodyBalance (2f9d95c), 3c-1 MachineWalk (fa38988), 3c-2 StateWalk (c7637b3), **3c-3 body→BodyWalk** (warden PASS after D3 fix). All three inner dispatch walks (machine_section/state-member/body) are @@[scan(u8)] systems; BodyWalk fuses a brace COUNTER + a (start,depth) ACCUMULATOR. I1 proven each; loops 86→77, SYSTEMS 15→19. | stmt_scan; DelimBalance; MachineWalk; StateWalk; **BodyWalk** (19th) | 3a/3b/BodyBalance/3c-1/3c-2/3c-3: yes |
| 4 Islands | **ArgScan LANDED 2026-07-19** (one seat; hand splitters deleted; 12 deltas incl. Bug B(iii); E407 provisional). **NativeParts design ACCEPTED, build NEXT** (retires the last hand-Lexer recognition; DP-1/H-1 ruled) | arg_scan (22nd..24th systems with 3d/3e); adjudication seam live; native_parts_scan completion pending | ArgScan hand path: yes (oracles C-final). NativeParts: no |
| 5 Validators | not started | hsm_cycle, reachability | no |
| 6 EmitDriver | not started | — | n/a (transducer, last) |
