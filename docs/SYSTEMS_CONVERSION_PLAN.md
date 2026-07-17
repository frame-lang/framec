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

## The four guardrails (non-negotiable)

1. Each capability → a `.frs` `@@system`, compiled by framec-ng to a committed `.gen.rs`,
   wrapped by a thin `mod.rs` with native **leaves**.
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

### Audit Log (append-only — warden verdicts)
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
| 1 OpaqueScan | **warden PASS (GATE-A + GATE-B pending commit)** — landing | OpaqueScan, RawString, BraceBalance | skip path is the system; try_island routed; residual = holes (Item 4), close_brace (Item 2), machine skip (Item 3), + 3 oracles — all named; batteries+fuzz+milestone green |
| 2 Segmenter | system exists; oracle not self-contained | segmenter | no (oracle) |
| 3 Grammar | not started | stmt_scan (partial) | no |
| 4 Islands | not started | ref/inst/embed_scan (partial) | no |
| 5 Validators | not started | hsm_cycle, reachability | no |
| 6 EmitDriver | not started | — | n/a (transducer, last) |
