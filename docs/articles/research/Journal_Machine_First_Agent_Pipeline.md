---
title: "Journal — Machine-First Development with an Agent Pipeline"
parent: Articles
nav_order: 20
---

# Journal — Machine-First Development with an Agent Pipeline

*A working research journal: the chronology, experiments, and discoveries
of building a machine-first, agent-executed development pipeline around the
framec compiler rebuild. Raw material for a future paper. Entries are
dated; claims carry their evidence; nothing here is polished argument —
that is the future paper's job.*

*Cadence (owner directive, 2026-07-19): journal **every step** — each
launch, gate verdict, landing, and correction gets an entry as it happens,
not in retrospect. The journal now feeds two consumers: the future
**article** (discoveries, §2) and a **post-mortem improvement cycle**
(process defects and frictions, §4) to be run at campaign end.*

---

## 1. Chronology

**2026-07-17 — the bias incident.** During the campaign converting the
rebuilt compiler's scanners to explicit state machines, design records
repeatedly ruled working code "a native driver — not a machine," each
ruling endorsed by the machine-specialist tooling consulted, each
terminating analysis. The correction came as a thought experiment (wrap
every parse function in a one-state system — always possible, so
machine-ness was never the question) plus a prediction: initialization and
error conditions are states being glossed. Inspection confirmed the
prediction immediately.

**2026-07-17/18 — the worldview, written and tested.** The correction was
written as a self-contained paper (*Shadows on the Wall — The Latent
Machine*) and then tested for sufficiency by falsifiable, blind
experiment: two fresh agents trained on **nothing but the paper**.
Probe A, handed the biased design record with a neutral prompt, identified
both ontological errors, re-derived the ruling on admissible grounds
("same disposition, different grounds"), and found an undispositioned
scaffolding system nobody had planted. Probe B, applying the paper's
method cold to a scanner file with a known answer key, recovered the full
key and reported three defects beyond it — all verified against source.
Conclusion: the paper alone carries both the advisory and the finding
skill; agent instruction files are packaging (recorded as RFC-0059, the
first "discovery RFC," with process rules P1–P6).

**2026-07-18 — the agent family and its experiments.** A domain-focused
family replaced the omnibus-advisor idea after a four-arm controlled
comparison (identical engagement, briefs differing only in added skills):
the base worldview improvised competently; added assessment/plan-review
skills paid for themselves only in document/audit findings; both
multi-role arms ran ~3–4× longer than focused runs for no additional
code-level findings. A per-system *static-by-construction* scanner
(`source-machine-finder`, no execution tools in its envelope) shipped and
was validated on two graded single-file benchmarks (full answer-key
recall; novel verified defects on both). An intake protocol ("the
engagement is a machine; the brief is its init state") passed its
behavioral test: handed a vague request, the agent returned a grounded
proposed brief and did no unscoped work.

**2026-07-18 — the stall mis-diagnosis (a measurement lesson).** One
experimental arm ran long; a liveness heuristic (transcript file size) was
invented, enshrined as protocol, and used to kill three agents — all of
which were healthy. Measurement later showed the gauge was *disconnected*:
every agent's transcript file, including completed successes, measures the
same 155 bytes in this environment. The protocol was corrected same-day
(calibrate any liveness signal against a known-healthy agent; patience ≥
2× the largest same-shape sibling; run synchronously when a result gates
the next step; kill only on hard evidence). The reflexive finding: the
orchestration layer's own observability was a latent machine with glossed
states, and the discipline the paper preaches applied to the tooling that
watches the agents.

**2026-07-18 — the pipeline, specified and exercised.** RFC-0058 (revived
as a living draft) specified the five-step pipeline — INVENTORY → DESIGN →
INDEPENDENT REVIEW → OWNER GATE → BUILD — with named error transitions,
the **terminal ledger** as a mandatory design artifact, and the
**conversion set** (né unit) as the atom, scoped by closure and landed
atomically. Four conversion sets traversed it end to end (DeclWalk,
the head readers, ArgScan, NativeParts), including a two-lane parallel
design trial, one owner RETURN with recorded direction (the
fork-and-adjudicate ruling), and dual independent reviews of the RFC
itself.

**2026-07-18 — the graph and the build.** The architect's output was
boiled down to one data structure (the **system graph**: as-is from the
finder, to-be from the designer, the diff as the work; prose reports as
derived *explain* projections). A whole-module as-is scan produced 47
nodes, 3 entry points, and a pipeline order that matched known
architecture exactly — plus eight new findings. The build phase then
opened: a hygiene set (11 stale generated files re-blessed; the standing
regeneration check finally built) and the first conversion set ever
implemented **by a builder agent from the design record alone** — all
green, three deviations honestly reported, zero blockers.

**2026-07-18/19 — the build wave: three conversion sets landed.** Build
order was derived from the system graph's topology, not judgment: the
hygiene set first (`af0f79f`), then DeclWalk solo (`6349ba2` — hand
`decl_of` and `matching_brace` deleted), then a **two-lane parallel build
trial**: the head readers (`694c043`) and ArgScan (`b9f5162`) built
*simultaneously* by independent builder agents in isolated git worktrees
with separate build caches, on extents the graph predicted disjoint. The
prediction held: both lanes rebased onto the shared trunk with **zero
conflicts**, and the composed verification ran green — 24 systems, 0
stale regenerations, and the plan's recorded suite figures: 342/0 tests
in the compiler package, 1928/0 across the workspace. During the ArgScan
build the builder found a fresh defect in the hand code it was replacing —
Bug B(iii): the comma splitter's alphabet counts the `>` of the `$>(`
sigil itself, mangling two of the six spec examples (#4 and #6, each
reduced from three arguments to two) — and the warden then *reproduced it
on the live oracle* at the parity gate: bug-finding had reached the BUILD
stage. Two tests now hold it: `oracle_stayed_buggy` pins the defect's
both shapes (unnamed and named) on the hand oracle, and
`pin_spec_examples` pins the fixed behavior on the system with a
non-vacuity tooth against that oracle. Campaign scoreboard after the
wave, per the census loop-proxy (`tools/scan_census.py`; movements
recorded in the plan's Change Log; a proxy with known blind spots —
PM-4): production hand scan loops 77 → 58, systems 19 → 24, hand-Lexer
*production* recognition still 11 — all of it owned by the one remaining
set.

**2026-07-19 — the closer launched; journaling goes per-step.**
NativeParts — the conversion set that retires the hand Lexer's last
production recognition (11 → 0, campaign goal C2) — was handed to a
builder agent in its own lane worktree. Its brief carries two hard-won
process artifacts: the **gate amendment** (recorded in the design record
itself, dated 2026-07-18: the design's original seam mechanism was
disproved by the reviewer against four generated machines —
`native_parts_scan`, `body_walk`, `opaque_scan`, `delim_balance`, each
`.gen.rs` showing `scan_at` reset every literal-initialized field — so
the corrected constructor-parameter seam is binding), and a **stale-line
warning** — three sets landed between the design's acceptance and its
build, so every line number in the record is rot; the builder must locate
by symbol and re-verify each cited fact. Scope is Phase 1 only: byte
parity through the parity gate, all 14 ledger terminals carried and named,
the recorded owner rulings' *fixes* (DP-1, H-1) held as Phase-2 deltas.
Same day, the owner set the journal's cadence: every step, as it happens,
feeding both the article and a post-mortem improvement cycle.

**2026-07-19 — the journal entered its own pipeline.** The backfilled
entries above were adversarially verified before landing: four
independent verifiers checked 28 factual claims against the repositories
and refuted 10 — among them a misread notation ("spec examples 4/6"
means examples #4 and #6, i.e. two of six, not four of six), a wrong
test-binary count, a suite flake misattributed to the build wave when it
belonged to the earlier agent experiment, and seven citation gaps where
a sentence asserted what no named artifact could support. All ten were
fixed before commit. The journal now holds itself to the discipline it
documents — and the incident is itself D2 evidence: the writer read past
every one of those errors; the verifiers' differing obligation (refute,
don't summarize) caught them.

**2026-07-19 — the closer delivered.** The NativeParts builder returned
after ~59 minutes — twice its siblings' runtime, inside the corrected
patience protocol's window (≥2× the largest same-shape sibling; PM-1's
gauge was never consulted) — with a complete delivery and zero blockers:
17 files touched plus a new 747-line battery. Process notes worth
keeping: it **compile-probed the amendment's corrected seam before
editing** (generated a machine and read the artifact to confirm
constructor-parameter fields survive `scan_at`); it factored the hand
oracle *first* and proved the build inert before rewriting the driver;
and its differential's **teeth counters bit on the first run** — the
holed-literals counter came up 6 against a floor of 10, and the builder
fixed the *fuzz pool* (biased generation toward the underfed class), not
the bar. It also found a gap in the design's own fact base — a second
fixture consumer of the seat (`tests/reindent.rs`) the record's §0 never
enumerated — and handled it by the record's *already-recorded* policy
rather than inventing one, reporting it as a deviation. Three deviations
total, each argued from the record. Builder-reported gates: suite 398/0,
24 systems 0-stale (bootstrap-safe, re-verified after the compiler's own
assign path changed), production recognition 11 → 9 and loops 58 → 55
with the remainders named and owned by later sets. Handed to the warden
for GATE-A; nothing committed, no production hand path deleted.

**2026-07-19 — GATE-A: PASS-WITH-CONDITIONS, and the milestone claim
corrected.** The warden re-ran every predicate from a clean cache
(baseline re-derived from a `git archive` of the pre-change commit, not
trusted; the fuzz generator re-implemented in Python to count form
incidence independently) and returned PASS-WITH-CONDITIONS. The machine
is proven: regeneration fixpoint green on both the debug binary and a
freshly rebuilt release binary (the bootstrap leg — the wired compiler
re-scans all 24 systems byte-identically); the oracle diffs to the
pre-change hand code as renames-plus-comments only; the carried swallows
are pinned *identical*, not fixed; the constructor-parameter seam is
confirmed in the generated artifact. Two conditions, both non-FAIL: C-1,
a single curated nested-block-comment fixture the fuzz pool never
generated (discharged immediately — added to the corpus, differential
still green); C-2, the three deviations recorded at the landing entries.

The load-bearing finding was Finding 4, and it corrects a claim carried
in this journal and in the owner reports: **NativeParts does not retire
the campaign's last production recognition — it retires `parts.rs`'s.**
The census moves 11 → 9, not 11 → 0. The residual nine are seven `lex.rs`
token definitions and two `sections.rs::section_keyword_starts` calls,
and the plan's acceptance entry ("retires the LAST production hand-Lexer
recognition") overstated by that `sections.rs` pair — which no plan item
names in scope. The set's own scope is complete and correct (`parts.rs`
→ 0); the campaign goal C2 (recognition = 0) now has a named open thread:
the `sections.rs` owner, an owner-gate scope question. Recorded as PM-7;
the catch itself is D2 evidence — the warden refuted the plan's own
accounting, not just the builder's code.

**2026-07-19 — NativeParts Phase 1 landed (`6786092`).** The fourth
conversion set through the full pipeline, landed the same way as the
prior three: the landing gate re-run by hand (suite 398/0, regeneration
fixpoint 24/0 across a rebuild, census 11 → 9 / 58 → 55), an atomic
commit on the lane, a zero-conflict fast-forward onto trunk, and a trunk
re-verification. Its plan entries record the three deviations (C-2), and
— this is the part that matters — the accounting correction rides *in the
landing commit itself*: the commit message, the Change Log, the Audit
Log, and the Progress ledger all say NativeParts retires `parts.rs`'s
production recognition, not the campaign's last, with the `sections.rs`
and `lex.rs` residuals named and the C2 owner-question filed. The
overclaim is corrected at the same moment the work lands, in the same
artifact — the correction cannot drift from the claim it fixes.

The campaign now stands: production hand scan loops 55, systems 24, and
production recognition **9** — `parts.rs` at zero, the residual nine
awaiting an owner ruling on scope (the `sections.rs` pair and the
`lex.rs` token definitions, the latter possibly parked-`@@fsm`
territory). Four sets designed, reviewed, and landed; the build wave that
opened on 2026-07-18 closes here, with its one milestone claim corrected
rather than shipped.

**2026-07-19 — the census hardened, and hid a bug while telling the
truth.** Given the choice of how to make the C2 gate honest, the owner
chose the principled route: harden the census to detect oracles by
reachability, not by name. The fix reclassified `section_keyword_starts`
correctly and re-baselined the numbers — production recognition 9 → 7 (0
live-path calls), production loops 55 → 46. But the *first* cut of the
fix had its own glossed state: blind to generated callers, it demoted
`params_close` — a production leaf the DeclRead system reaches through its
generated code — to oracle, which would have quietly hidden a real
production loop. A ground-truth caller-grep of every reclassified
loop-bearing function caught it before commit; the fix now counts
generated-system call sites as production, and an `--audit` mode makes
every demotion reviewable. The lesson folds back on itself: the tool that
measures how much hand recognition remains is itself a latent machine, and
hardening it demanded the exact verify-don't-trust discipline it exists to
enforce (D6, again — this time on the gauge's repair, not the gauge).

**2026-07-19 — the oldest open obligation closed (Item 3d GATE-B).** Asked
for the most methodical next step, the answer was not to open new work but
to close the one gate that had been deliberately *held*: DeclWalk's GATE-B,
withheld at the design gate until its Phase-B delta retired an accepted
bare-counter exception (`params_close`). Fittingly, `params_close` was the
very leaf the census hardening had just fought to keep correctly classified
— and this delta retired it for real, routing its paren count through the
DelimBalance system (T9) and replacing a string-blind brace-find with an
opaque- and params-aware `body_open_at` (T13). Both were shared-leaf swaps,
so the differential locked and the *fix* was pinned by directed tests
(the two old Phase-A bug-pins flipped to assert correct behavior; six new
directed tests, including both faces of an owner-conditioned fallback). The
methodical detail worth keeping: the census came out **flat** — production
loops 46 → 46 — because T9 removed a real counter and T13 added a
composition-dispatch loop the `while <ident> <` proxy cannot tell apart.
The warden *ruled* on that rather than escalating: the owner had already
adjudicated `body_open_at` by name at the design gate, and the code matched
the ruling with zero drift, so re-escalating would re-litigate a settled
decision (D9 sharpened — escalation discipline includes knowing when *not*
to escalate). The honest flat number, and the reason for it, rode into the
landing commit. Item 3d is now complete; the delta landed as two
file-disjoint commits exactly as the gate prescribed.

**2026-07-19 — the gate-keeper was not grounded in the worldview it
enforces.** The owner read the warden's ruling that `body_open_at` was
"design-accepted glue" and asked one question: *is the warden trained on
the shadows in the cave?* An audit answered it — only two of ten agents
(the machine-advocate and the source-machine-finder) referenced the
paper at all; the warden, the machine designer, and the test author, the
very agents that gate, design, and test a machine-first conversion, had
zero grounding in the machine-first worldview. To test whether the
verdict itself was sound, the *grounded* advocate was run on
`body_open_at` adversarially, without being told the warden's answer: it
independently ruled LEAVE LATENT and reached it the paper's way — the
function is a machine, but a degenerate one, a monotone cursor carrying no
recognition register, every real decision delegated to the two systems it
calls. So the verdict held; but the warden had reached it through a proxy
(its own DoD rule D3) whose literal text it had to argue *around* — right
on this case, unjustifiable on a harder one. The owner's directive: every
agent involved must be grounded in the document. A canonical worldview
section — the theorem, the carried-register-versus-cursor test, the
glossing/costuming policing, the reify-or-leave-latent disposition
vocabulary — was added to every ungrounded brief (its core verified
byte-identical across all nineteen copies), and the rule was made standing:
any new agent, including an ad-hoc builder brief, carries it.

**2026-07-19 — the grounding paid off at the next gate (Item 3e Phase-2).**
The three head-reader Phase-2 deltas — opaque-aware seeks routed through a
`skip` leaf, a params-skipping parent hunt, a limit-bounded probe — were
built (from the accepted record, by a builder whose brief now carried the
worldview) and gated by the *first* warden run since its brief was
grounded. The difference was legible in the verdict. Where the earlier
warden had judged a leaf by "is it a counter" and argued around D3's
literal text, the grounded warden ruled the `skip` leaf a run-and-unwrap of
the OpaqueScan sub-system — "the recognition register lives in the system,
not the leaf" — and `is_dollar_name` an O(1) fact, both Category-A *by the
paper's test*, stated as dispositions rather than checklist ticks. It also
made a call the ungrounded proxy could not have made cleanly: the builder
had rewritten two oracle seeks as guarded `loop {}` to keep the census
honest, and the warden ruled that **honest, not a proxy-dodge** — the form
preserves the exact invisibility class of the `while predicate(...)` loops
it replaced (a known census blind spot), and being oracle-code it cannot
touch the production ratchet regardless of shape. The set landed net-neutral
(every census metric byte-identical to base), suite 401/0, three
file-appropriate commits. The census's brace-matcher bit once more along the
way (a `{` in an oracle comment mis-attributed spans, caught in the gate) —
PM-4's third face, now filed as a specific pre-C-final hardening item.

**2026-07-19 — the worldview document went through its own review.** The
foundational paper — the one all the agents are now grounded in — drew a
serious editorial review (filed as issue #242) arguing two things: that its
`machine | value` ontology is *incomplete*, missing a third category
(*predicate* — a law over a machine's behaviors, where the paper's own
verifiability and alignment payoffs actually live), and that it *overclaims*
its logical status (a near-definitional identity dressed as a "theorem
provable three ways"). The critiques were verified accurate against the
article line by line, then the four that do **not** depend on the (deferred)
predicate decision were applied — softening the theorem framing, conceding
that `async/await` argues *for* latency-plus-tooling (the leave-latent
disposition at language-design scale) rather than against the thesis,
engaging the ADT "make illegal states unrepresentable" rival head-on, and
demoting a vacuous finite-memory move to the quarantine the paper reserves
for its own degenerate pole. The corrections were adversarially verified by
five independent reviewers, which caught a regression the author missed (one
section still called the identity a "theorem," now contradicting the fixed
abstract) and confirmed the load-bearing guardrail: no reviewer found the
`predicate` category smuggled in — the trichotomy remains a deferred owner
decision. The reflexive point for the future paper: the worldview document
is subject to the same find → verify → fix discipline as the code it
governs, and its own strongest payoffs (verifiability, alignment) may be the
first evidence that a third category is waiting to be named — the latent
*law* beside the latent machine.

**2026-07-19 — the third category was named, and the whole system moved
with it.** The owner ruled the trichotomy in: the worldview is now
**machine | value | predicate**, with a second-order **constraint** (a
predicate bound to a site — a guard, an assert, a type-check — the law in
force). It was built as a chain of owner refinements, each answered before
the next: is the constraint a fourth primitive? (no — the residue-test and
the owner's own virus emblem place it as a seam, the predicate's engine);
do predicates act on functions as well as data? (yes — the definition
broadened to a law over a value, a function, or a machine); deepen and
cite it, and give criteria to fingerprint it in native code. That last
directive drove a six-strand research fan-out that returned **thirty-two
citations, every one search-verified** (Floyd, Hoare, Dijkstra, Meyer,
Wadler, Freeman–Pfenning, Pnueli, Lamport, Alpern–Schneider,
Clarke–Emerson, Hume, Turner, and more), which a two-reviewer adversarial
pass then re-checked against authoritative sources — page ranges and all —
returning ALL_CORRECT. The paper gained a **fingerprint field guide**
("where laws hide") facing its machine-disguise table, and the enlarged
ontology propagated **in lockstep to all twenty-one agent briefs** so the
categorizers name four roles, not one. The methodological point is D12
sharpened: the worldview document and the agents that hold it are one
artifact — a change to the ontology is a change to the whole system, made
in a single coherent motion, research-verified before it lands.

**2026-07-19 — every per-set Phase-2 delta is landed (NativeParts closes
the set).** The campaign's Phase-2 obligations — the recorded fixes each
conversion set deferred behind its parity landing — are complete: 3d
(params_close→DelimBalance, body_open_at), 3e (the head-reader triple),
and now NativeParts' five (string-aware holes, the `{{` phantom, DP-1's
unterminated→one-Text-run, comment-delim honesty, and the H-1 RefScan fix).
The last is the one to remember: its owner-mandated verification found that
*nothing* in the compiler diagnosed an unknown `@@:` context reference —
the scanner had silently defaulted the unknown to `ContextSelf` — so the
fix was a genuine correctness gain, not a cosmetic one: a named `Unknown`
terminal in the scanner and a new **E408** diagnosis owned by the
validator, verified end-to-end at the CLI. The grounded warden read it in
the paper's own terms — a *glossing*-fix that splits a merged terminal into
a named state plus a checkable law bound at a **constraint** seam, the very
vocabulary the article had just gained. The tool caught up to the theory in
the same day the theory was written.

## 2. Discovery register (the paper-worthy claims, each with evidence)

**D1 — A worldview document alone is sufficient agent training.** Blind
two-probe experiment: bias-flagging reproduced (including the hardest
form, accepting a conclusion while rejecting its grounds) and the
machine-finding skill transferred, with novel verified defects as the
by-product. Instruction files measured as packaging, not capability.

**D2 — Layered fallibility works: every stage misses; the next stage's
*differing obligations* catch it.** Eight-plus consecutive engagements
each caught a defect the previous stage missed: finder → designer (the
blind body-fork terminal), designer → reviewer (the clamp that panics
debug builds), reviewer → owner (a guardrail exception; a plan-ledger
overclaim), designer → owner's own ruling (the defaults caveat on
tie-impossibility), reviewer → designer's mechanism claim (generated
machines reset all literal-initialized fields — disproved against four
generated artifacts), examples-agent → repo instrumentation (the doc
validator that couldn't see subdirectories), builder → design record (a
hand-code defect the finder, designer, and reviewer had all read past —
Bug B(iii)), warden → builder (independent reproduction of that claim on
the live oracle before the gate verdict), warden → the plan itself (an
independent census at GATE-A refuted the *plan's* accounting — a
milestone entry that claimed the last production recognition when a
`sections.rs` residual remained; PM-7). The chain now spans the full
pipeline, entry to landing, and its last link caught the *record*, not
the code — the obligation to re-run the census, rather than restate it,
is what found it. The layers work because the obligations differ, not
because later stages are smarter.

**D3 — Differential parity cannot certify state-faithfulness; the
terminal ledger can.** Proof case: a wrong default was faithfully
duplicated from hand code into its reified replacement *because* the
differential blessed it — the hand code is the differential's own
reference, so its glosses ride across by construction. The ledger
(enumerate every terminal of the replaced code; rule each carried-or-
fixed; nothing dropped) is the countermeasure, and in practice it
out-found its inputs in every engagement that ran it.

**D4 — Bugs as a byproduct: no agent was ever asked to find bugs.**
Roughly two dozen verified defects surfaced in a codebase whose test
suite was green throughout, via five mechanisms: fingerprints that aim
reading at glossed regions (bare counters near user text; error-swallow
shapes); completeness obligations that cannot be satisfied without
noticing (the ledger); verify-don't-trust (claims re-derived by grep,
generated-artifact reads, predicate re-runs — the weeks-old regeneration
break fell to a single re-run); adversarial layering (directed
counterexamples); and execution (compile-and-run examples caught a
runtime argument-drop bug and a spec divergence no static read could).
The mechanism reached the BUILD stage in the two-lane wave: a builder
implementing to byte parity found a sigil-miscount defect (Bug B(iii)) in
the very hand code its differential uses as reference — the
implement-to-parity obligation forces a closeness of reading that review
does not.

**D5 — Bounded beats omnibus, measured.** Narrow-charter agents were
reliable in every engagement of the campaign; multi-role arms tripled
cost without additional code-level findings. Structural boundaries
(tool-set enforcement — a scanner that *cannot* execute) outperformed
instructed ones.

**D6 — The orchestration layer is itself a latent machine, and it bit
us.** The disconnected-gauge incident: an unvalidated liveness signal
killed three healthy agents. Corrective protocol now durable; the general
form — *calibrate any signal against a known-healthy instance before
trusting it* — is a measurement discipline the future paper should state.

**D7 — Design records are executable by fresh minds.** Confirmed across
four full traversals (DeclWalk, the head readers, ArgScan, NativeParts),
each a fresh builder agent working from the accepted record alone: no
design decisions of their own, every interpretation-level deviation
reported in the builders' completion reports (three on the first; each
later build carried its own roster), and silent drift bounded by the
mechanism, not by trust — the byte-parity differentials every landing
must pass. The fourth traversal sharpened the qualifier: the builder
compile-probed the record's corrected seam against a generated artifact
before editing (executing the record's *claims*, not just its
instructions), and found a gap in the record's own fact base — a fixture
consumer the design never enumerated — which it closed by the record's
own stated policy rather than by improvising. Executable, and
self-correcting where the record is incomplete. One qualifier earned along the way: records **rot** —
line numbers cited at design time were stale by build time after sibling
landings, so executability required a locate-by-symbol discipline stated
in the brief (see PM-2).

**D8 — Schema grown empirically converges.** Two design agents,
firewalled from each other, independently discovered the same missing
edge type (leaf-runs-a-system) and the same ledger-ruling gap
(behavior-identical-but-state-named). Independent convergence as schema
validation.

**D9 — Escalation discipline held.** Agents repeatedly *refused* to
decide owner-class questions and routed them up (a new sub-system that
contradicted a recorded ruling; an unterminated-interior policy; error
ownership between scanner and validator). The owner's rulings then
sharpened designs rather than rubber-stamping them — including one case
where the owner's own proposed mechanism (dual-hypothesis fork,
semantically adjudicated) replaced a designed heuristic outright.

**D10 — Terminology is instrumentation.** Observed misreadings drove
renames (unit → conversion set; the appendices as translation guides;
symptom replacing an overloaded term), each fixing a measured confusion
rather than a stylistic itch. Vocabulary defects behaved exactly like
code defects: found by use, fixed at the source, regression-guarded by
the glossary.

**D11 — The system graph is a parallelism budget, and it paid out on
first trial.** Two conversion sets whose extents the graph showed
disjoint were built simultaneously by independent agents in isolated
worktrees; both rebased onto the shared trunk with zero conflicts, and
the composed verification ran green with no integration work at all. The
graph's edge structure — not schedule intuition — decided what could be
parallel, and the merge was the experiment that tested the graph's
accuracy. (One boundary stated honestly: the graph budgets *code
extents*, not *machine resources* — an earlier pair of overlapped agent
runs in this campaign produced a toolchain-level test flake on the shared
machine (PM-6); the build lanes mitigated with separate build caches and
ran green.)

**D12 — The worldview document is sufficient training only while the agent
still carries it; enforcing agents drift onto proxies.** D1 measured that
the paper alone carries the skill and instruction files are packaging.
The corollary, found the hard way: eight of ten agents' packaging had
*dropped the paper*, and those agents reverted to role-specific proxies —
the warden judged machine-honesty by its DoD checklist (D3/D8) rather than
the paper's carried-register test, reaching the right verdict on an easy
case by an argument it had to make *around* its own rule's literal text.
A proxy is downstream of the worldview; it can coincide with the right
answer without being able to justify it, and that coincidence fails
silently on the first hard case. The countermeasures are two: re-ground
every agent in the source document (done — a canonical worldview section,
byte-identical across all copies), and use a *grounded* peer agent as the
validator (the machine-advocate confirmed the verdict from first
principles, which is what told us the proxy had merely gotten lucky). The
instrument that caught the drift was not any agent layer — it was the
owner's one-line probe (*"is the warden trained on the shadows?"*), which
makes the owner gate a grounding check on the agents themselves, not only
a guardrail-and-scope check (extends D2).

**D13 — The worldview predicts NULL conversions, and the pipeline confirmed
one adversarially.** Item 5 of the campaign ("validators") was expected to
convert hand code into `@@systems` like every item before it. Examined, it
did the opposite: the pure-AST validator pass (`validate.rs`, whose own
header boasts it "cannot read source bytes") contains *no hand recognition
machine left to convert*. An 11-agent inventory (4 independent lenses —
machine-finder, trichotomy-classifier, scope-resolver, expressibility —
then a perspective-diverse adversarial pass of 16 verdicts, a reify-advocate
and a leave-latent-skeptic per candidate) returned a unanimous
`stays-native / recognition_register_present:false` at high confidence on
every one of the eight candidates — *including every reify-advocate, whose
assigned job was to argue the machine case*. The two genuine machines the
pass needs (HSM-cycle E403, reachability W401) were already reified as
`@@system` graph-walkers over integer edge lists; the residue (E402 target
membership, E407 arity adjudication, E408 unknown-context) is
predicate/constraint that reifying would *costume*, not compress. This is
the trichotomy earning its keep as an analysis tool rather than a
conversion mandate: §4.4's prediction "not everything is a machine; name the
laws as laws" came true at a real seam, and the adversarial harness is what
licensed *believing* the null result instead of forcing a costume to hit a
scoreboard. Two process notes rode along. (i) The named target "SectionOrder"
is a **phantom** in the cleanroom — no user-facing section-order validator
exists (only the walled-off legacy `SectionOrderValidator` and an
unimplemented E113); the plan text naming it, and describing E402/E609 as
"section-order," is imprecise and predates the code. (ii) *Verify-don't-trust
caught the delegated search under-reporting*: the scope agent declared
section-order "not implemented anywhere," but the owner's own confirming grep
surfaced `TreeDefect::OutOfOrder`/`Defect::OutOfOrder` — which on inspection
are I1 byte-coverage *partition* invariants ("COMPILER BUG, not a user
error"), a different construct that leaves the agent's substantive conclusion
intact but its literal claim wrong. The lesson compounds D2/D12: an agent's
conclusion can be right while its coverage claim is incomplete — the cheap
independent check is not optional even when every lens agrees.

**D14 — A *genuine* machine can still be correctly left native, and the campaign
converged by finding where the machines run out.** Item 6 (the emit driver) was
the campaign's last and most deliberate conversion, and — unlike Item 5's
degenerate walks — `emit_body` is a *bona fide* Mealy transducer: it carries a
real recognition register (`terminated`), read back at the loop head to suppress
dead code and again by `close_handler`. The naive campaign expectation was
"genuine machine ⇒ reify." It closed NULL anyway, for two independent reasons the
understand-phase surfaced and an independent warden re-derived. (i) *Reify does
not pay.* The adversarial pass *inverted*: the reify-advocate lens concluded
stays-native at HIGH confidence while the costume-skeptic reached reify-pays at
only MEDIUM — a 1-bit monotone absorbing latch gives negative compression and
trivial verifiability, leaving observability the sole payoff, and §5 says the
right home for a set-of-distinct-outcomes is the *value channel*, not a reified
walk. (ii) *It is blocked on a real, nameable compiler capability that does not
exist*: framec-ng emits a lifetime-parametric borrowed domain only on the
`@@[scan(u8)]` path, hardcoded to `src: &'a [u8]` (verified independently: 22 of
24 generated systems carry exactly that, the 2 plain `@@systems` are fully owned),
whereas emit_body's context is an irreducible *non-`src`* borrow — so per
guardrail 3 the blocker is surfaced, never hand-faked. The disciplined outcome was
to bank the one real payoff *natively* — rename the `bool` return to a
`BodyEnd{Terminated,Fell}` sum, the §5 faithful-terminal-structure fix in the value
channel — behavior-preserving (suite unchanged), and leave the walk native. Two
things make this the campaign's cleanest note. First, the trichotomy stopped being
a conversion mandate and became a *disposition* instrument: *existence of a machine
was never the question; whether naming it pays was* — and here, twice at the tail
(Items 5 and 6), it did not. Second, the campaign converged not by converting
everything but by correctly locating where the recognition registers run out — the
two that remained (HSM-cycle, reachability) were already systems, and everything
past them is predicate, value, or a machine whose reification would be costume or
is blocked. A conversion campaign that knows when to *stop* converting is the
strongest evidence the worldview is load-bearing and not a hammer.

**D15 — The reify-or-leave-latent vocabulary was a false binary; an adversarial
agent chain found the missing third term — *engine reified elsewhere* — and,
with it, the three-organ shape of type-ignorant recognition.** The campaign's
dispositions had hardened into two poles: *reify* the latent machine, or *leave
it latent* as a degenerate costume. Roughly twelve folds still carried the
leave-latent tag (docket items 32–43), and a fresh maximalist agent —
`ghost-buster`, chartered to "convert everything" and to refuse every
leave-latent as an unbusted ghost — was run at them deliberately, to see what
the refusal would shake loose. It won *no* conversions. But the chain it forced
is the finding. The `frame-machine-advocate` it provoked conceded the twelve as
machines-in-principle yet compressed *why* none should convert into a single
hand-wave; the `frame-compiler-architect` then ruled them costume / leave-latent
— the same terminating binary the 2026-07-17 bias incident had already been
caught making once, now reached from the opposite direction. The owner broke the
loop not with a verdict but with a memory: the already-shipped `ArgScan` fork
(Option C, `SYSTEMS_CONVERSION_PLAN.md:520`, gated 2026-07-18), which the
architect had *mislabeled*. The `fsm-designer`, ruling on that pointer and
re-grounding every claim against source, named the category both poles had been
missing: a fold that **consumes** a machine's stamped output is neither a costume
(it was never dressed as a state machine) nor a gloss (nothing is hidden) — its
**engine is reified elsewhere**, upstream, as a named and shipped `@@system`. Its
Shadows plea is the existing one — *a spec/value whose engine is a machine
someone owns* — made concrete. The twelve folds' *verdict* stayed UNCHANGED
(convert none; no `.frs`, no regen, no snapshot churn); their *disposition* was
corrected from "leave-latent / costume" to "engine reified elsewhere —
downstream consumer of a shipped machine" (ArgScan's fork for 42/43; the shipped
byte-walks BodyWalk/StateWalk/MachineWalk/DeclWalk/Segmenter/SectionScan/
NativePartsScan for 32–40; 41 consumes ParamSplit).

Three things make this more than a relabel. (i) *The pattern has a shape.* Where
Frame cannot resolve an ambiguity locally because it does not parse native types
(is `<` a generic-open or a less-than? is a `,` a separator or inside
`Map<K,V>`?), the correct architecture is **fork-and-adjudicate**, and it
decomposes into three organs that must never be conflated: a **recognition
machine** that carries BOTH hypotheses in ONE byte pass, stamping a viability bit
at every boundary (ArgScan's `depth`+`adepth` counters, `arg_scan.frs:127-145`)
— this is the machine, reify it; a **materialization fold** that reads the
stamped bits to produce a candidate (`merge_g`, `mod.rs:230`) — a function over
already-decided data, engine reified elsewhere, do not reify; and an
**adjudication predicate** that picks the reading using Frame-side knowledge only,
as a point-law (`validate.rs::adjudicate`, `validate.rs:279`, E407 on tie/miss)
— this reads no bytes and carries no register, it is the #242 *predicate*, not a
machine. (ii) *The diagnostic that tells consumer from costume:* before calling a
driver a costume, check whether its "register" is a FROZEN decision it *reads* out
of a producer's output versus one it *carries* — `body()` reads `depth` out of
the `(start, depth)` triples `BodyWalk` already computed (`body_walk.frs:39,48-57`)
and counts nothing itself; `merge_g`'s control decision is driven by the frozen
`g_end` bit (`mod.rs:238`), its `run_start` only supplying a slice index, never
gating a transition. Both are engine-elsewhere, not costume. (iii) *The one
genuine reify candidate the pass surfaced (F5, owner-deferred):*
`split_system_params` / `args_of` comma-split is **blind to angles** — the exact
Bug-B angle-blindness ArgScan cured, left un-fixed for system-params and
transition-args, still carrying the Bug-A-shape `trim_end_matches(')')`
(`mod.rs:331,333`; `argscan_design.md:1225-1232`). Unlike the twelve, this *is* an
unbusted ghost — a byte-level recognizer blind to an ambiguity it should carry —
and reifying it is one new fork-and-adjudicate byte walk with a real payoff
(correct `@@system Foo(x: Vec<A,B>)` and `-> $S(args)`) fixing a latent
correctness bug today. It is not one of the twelve folds; it is an owner decision.

The methodological point closes a loop the journal keeps re-finding (D2/D9/D12):
the maximalist agent that lost every argument was the load-bearing one — its
refusal to accept "leave latent" is what forced the re-examination that surfaced
*both* the advocate's twelve-item gloss and the honest third category the binary
framing had no word for; and the correction that finally landed came from the
owner's *memory of a shipped precedent* overriding an architect's confident
mislabel — the same "is the agent grounded in what already shipped?" probe PM-8
turned into a standing audit, here applied to a claim about the codebase rather
than about the paper.

## 3. Supporting artifacts (where the evidence lives)

- The worldview: `docs/articles/Shadows_on_the_Wall.md` (with the two
  translation-guide appendices).
- The process record: RFC-0059 (discovery genre; P-rules) and RFC-0058
  (living agent architecture; pipeline; conversion sets; system graph;
  evolution log with dated entries for every change above).
- The craft text: `docs/guides/recognition_recipes.md` — 18 recipes, each
  with a compile-and-run-verified Frame example, guarded permanently by
  the repository's doc-sample validator.
- The campaign ledger: the cleanroom conversion plan's Change Log and
  Audit Log (append-only; every gate verdict and owner ruling dated).
- Agent definitions: the repository's versioned roster; the finder's
  brief includes the Phase-4 architecture-map deliverable.

## 4. Post-mortem register (the improvement cycle's input)

*Process defects and frictions, logged as they occur. Format: what
happened → evidence → correction applied → durable improvement (adopted,
proposed, or owed). At campaign end each open row gets ruled: durable
process change (agent brief, plan guardrail, tool) or explicit won't-fix.
The discovery register (§2) holds what we learned; this register holds
what it cost to learn it.*

**PM-1 — Unvalidated liveness gauge killed three healthy agents**
(2026-07-18). The transcript-size heuristic was invented under time
pressure, enshrined as protocol, and acted on — before anyone checked it
against a known-healthy agent (it reads 155 bytes in *every* state).
*Correction:* same-day protocol rewrite, made durable in the process doc
and memory. *Improvement adopted:* calibrate any liveness signal against
a known-healthy instance first; patience ≥ 2× the largest same-shape
sibling; run synchronously when the result gates the next step; kill only
on hard evidence. *Root pattern:* we shipped an instrument without a
calibration step — the same defect class as PM-4.

**PM-2 — Design records rot between acceptance and build** (2026-07-19).
Three sibling landings between NativeParts' design acceptance and its
build launch invalidated every line number the record cites; one cited
seat had changed mechanism entirely (the instantiation island seat in
`parts.rs` — the record cites the hand `parse_inst_args`; since the
ArgScan landing `b9f5162` the seat is `inst_scan::scan_node`).
*Correction applied:* a stale-line
warning in the builder's brief — locate by symbol, re-verify each cited
fact. *Improvement proposed:* records should cite symbols and structural
anchors, never raw line numbers; alternatively a mandatory freshness pass
(diff the record's citations against HEAD) at builder launch.

**PM-3 — Reports drifted cryptic; the owner had to say so twice**
(2026-07-18). Dense internal shorthand (codenames, gate labels, compressed
scoreboards) leaked into owner-facing reports; the owner's "this summary
is impenetrable" and "too many things to address at once" were the
measurements. *Correction applied:* plain-language rewrites, scoreboards
with named columns, questionnaires for multi-decision asks, designs
presented as node trees. *Improvement adopted:* the owner-facing report
is a genre with its own obligations; multi-decision asks are always a
questionnaire. *Improvement proposed:* report templates in the process
doc, so the genre survives context resets.

**PM-4 — The census instrument has known blind spots, and one steered a
gate finding** (2026-07-18/19). The loop-proxy misses `while
predicate(...)` shapes and byte-literal brace matching; every wave
re-confirmed it. Then, at the NativeParts gate, it bit for real: the
proxy classifies a differential oracle as *production* recognition unless
the function carries the `_hand` name it keys on — and `section_keyword_starts`
(the SectionScan oracle, self-documented as such, called only from a
test) does not. So the census reported two production recognition calls
that do not exist on any live compile path, and the warden's Finding 4
inherited the number, escalating a naming gap into an apparent
"unowned-scope" question. Investigation dissolved it: every remaining
recognition call sits inside a `_hand` oracle except that one misnamed
one; the true count of production recognition *calls* on the live path is
zero. *Correction applied (owner directive, same day — "harden census
first"):* the proxy now classifies oracles by **reachability** — a
consumer whose every caller is a test or another oracle, and which no live
`@@system` invokes, is an oracle — not the `_hand` name alone. And
hardening the instrument exposed a *second* latent bug in the instrument:
the first cut, blind to generated callers, demoted `params_close` — a
production leaf the DeclRead system invokes through its generated code —
to oracle, which would have hidden a real production loop; the fix counts
generated-system (`.gen.rs`/`.frs`) call sites as production callers. All
five metric-bearing reclassifications were then verified against ground
truth by direct caller grep, and an `--audit` mode makes every demotion
reviewable. Re-baselined: production recognition 9 → 7 (0 live-path
calls), production loops 55 → 46. *Still owed (a different blind spot, not
this fix):* the `while <ident> <` loop shape misses `while predicate(...)`.
Same defect class as PM-1 — an uncalibrated gauge steering decisions, this
time into a warden verdict — with the added lesson that *fixing* a gauge
is itself gauge-work: the hardening had its own glossed state (the
generated caller) and needed the same verify-don't-trust discipline the
gauge is meant to enforce.

**PM-5 — Small-tool frictions with outsized cost** (2026-07-17/18).
BSD sed lacking `\b` left a rename half-done; exact-match edits failed on
line-wrap differences; one scripted chain in the staging docs repo (the
RFC-0059 landing arc) reached its commit step after an intermediate edit
in the chain had failed. *Correction applied:* residuals swept by grep
after every bulk edit; the missed edit landed in the following docs
commit.
*Improvement adopted:* after any bulk text operation, a verification grep
is part of the operation, not optional; commit steps run alone, never as
the tail of a compound chain.

**PM-6 — Overlapped agent runs shared machine resources and a suite
flaked** (2026-07-18). During the four-arm agent experiment, the two arms
that overlapped for ~25 minutes produced a JVM-toolchain test failure in
one arm; disjointness of *work* does not give disjointness of *machine*.
The graph-disjoint two-lane build wave that followed applied the lesson —
separate build caches per lane — and ran green. *Correction applied:*
the flaked result is docketed (this row is the docket) for solo
re-verification before it is trusted. *Improvement proposed:* isolation
policy names its two axes — code extent (the graph's job) and machine
resources (the scheduler's job) — and any result produced under overlap
needs a solo-green run before it counts.

**PM-7 — A milestone claim outran its evidence** (2026-07-19). The
NativeParts set was described — in the plan's acceptance entry, in this
journal, and in owner reports — as retiring "the last production
hand-Lexer recognition" (11 → 0). It retires `parts.rs`'s recognition
only; the census moves 11 → 9, with seven `lex.rs` token definitions and
two `sections.rs::section_keyword_starts` calls remaining, the latter
named by no plan item. *How it survived:* the acceptance entry was
written from the design's fact base, which scoped `parts.rs`; the
`sections.rs` residual was never enumerated, so every downstream
restatement inherited the gap. *Caught by:* the warden's independent
census at GATE-A (Finding 4). *Correction applied:* the milestone is
restated as "retires `parts.rs`'s production recognition." *Sharpened on
investigation (same day):* the "residual" is not an unowned scope gap.
The two `sections.rs` calls are the SectionScan differential oracle
(owned by Item 3, deleted at C-final) that the census miscounts because
it lacks the `_hand` name (PM-4); the seven `lex.rs` entries are the hand
Lexer's own recognizer *method definitions*, which zero only when the
Lexer is deleted at C-final. So C2 (production recognition = 0) is a
C-final milestone *by construction* — it counts definitions, so it cannot
close until the Lexer is gone — and production recognition on the live
compile path is already fully converted. The original claim was wrong
about "0", right that this set does not reach C2; the real reason is the
metric's shape, not a missing owner. *Improvement proposed:* completion
claims cite the instrument and the *residual*, never a bare target; and
the residual must be *classified* (transient oracle vs live path vs
by-design C-final), because a raw census number conflates all three.

**PM-8 — The foundational worldview was not propagated to the agents that
enforce it** (2026-07-19). Eight of ten agents — including the warden that
gates every milestone, the designer that rules dispositions, and the test
author — carried no reference to `Shadows_on_the_Wall.md`, the document the
whole pipeline exists to apply. They enforced role-specific proxies instead
(the warden's D3/D8 checklist), which reached a correct verdict on an easy
case by reasoning it had to bend around its own rule. *How it survived:*
agent briefs accreted role machinery over time; nobody re-checked that the
machinery still traced to the source worldview. *Caught by:* an owner probe,
not the pipeline. *Correction applied:* a canonical worldview section added
to every ungrounded brief (core byte-identical across all copies), the
already-grounded two path-fixed to the absolute paper location, and a
standing rule that any new agent — including ad-hoc builder briefs — must
carry the grounding. *Improvement adopted:* grounding is a property to
*audit*, not assume; a periodic check that every agent's brief references
the source document belongs in the process doc. Same defect class as PM-1
and PM-4 — an unvalidated instrument (here, the agents themselves) steering
decisions until someone calibrated it against ground truth (the paper).

**PM-9 — The gate agents *drove* verification when they could *judge* it**
(2026-07-19). Asked why the campaign was slow, the honest answer was
measured, not guessed: an incremental recompile is two seconds, so
compilation was never the bottleneck — the cost was the warden running the
standard predicates across dozens of serial read-run-reason turns (one
GATE-B took 103 minutes over 38 tool-uses, ≈2.7 min/turn, most of it model
latency between rote checks). *Correction adopted:* `tools/gate_evidence.sh`
collects the whole standard bundle — diff, build+warnings, suite, regen
fixpoint, census at HEAD **and** base (net change, no build), oracle
presence, flipped pins, new-leaf listing, CLI probes — in one ~1-minute run,
emitting raw re-runnable command outputs and **no verdict**. The warden now
judges the bundle and spends its turns on the delta-specific checks, not on
re-deriving boilerplate. The discipline is untouched — same commands, same
verify-don't-trust rule, the agent still forms its own verdict — only the
serial *driving* is removed. The general lesson for an agent pipeline:
distinguish the *judgment* (which is the agent's irreducible value) from the
*evidence-gathering* (which is scriptable), and don't pay model-latency,
one turn at a time, for the latter. Not a defect in a result — a defect in
the *shape* of the work, found by asking where the wall-clock went.

**PM-10 — A plausible optimization was committed, then the measurement it
skipped refuted it** (2026-07-19). PM-9's same "make the gates faster" push
produced a second lever that *sounded* right and was wrong: wire each lane
worktree to a shared `sccache` compilation cache, on the premise that a fresh
`/tmp` lane cold-rebuilds the whole dependency tree in minutes. It was
committed and pushed before being measured. Measured afterward, every load-
bearing assumption failed: `frame-compiler` has an intentionally-empty
`[dependencies]` (`cargo tree` shows zero external crates), so a fresh lane
compiles exactly one crate and a true cold build from an *empty* target is
~2 s — there is no dependency tree to cache; sccache scored **0 hits** across
an edit-rebuild loop, because every conversion edit changes the single
crate's content and content-keyed caching is therefore always a miss; and the
`incremental = false` that sccache requires made the loop **~7% slower**
(~1.82 s vs ~1.70 s) by disabling the intra-crate incremental compilation that
actually helps. *Correction adopted:* the lever was backed out in the open —
`new_lane.sh` reduced to a plain lane-creation helper, the local cache config
deleted, and the plan's "Speed levers" note now records the rejected lever and
its lesson rather than silently dropping it. The general lesson mirrors the
host-runtime rule *check that the evidence supports this specific action
before changing state*: an optimization is a state change, and "everyone knows
cold Rust builds are slow" is a training prior, not a measurement of **this**
repo. A two-minute probe (`cargo tree`; one real `rm -rf target` build; an A/B
edit-rebuild loop) would have pre-empted the whole detour. The honest close is
not that the tool was harmless — it was a net regression — but that the
correction is itself part of the record: verify-don't-trust binds the pipeline's
own tooling, not only the code it converts.

## 5. Open threads (for the paper's future-work section)

The global analysis tier (boundary preprocessing, port joining) gated on
the completed local benchmark; the explain renderer as a deterministic
projection with a linting side effect; hosting the agent tree on a
supervised Frame-machine runtime (agents as effectors, the stall protocol
reified as machine states); a wider-than-two-lane parallel build (the
two-lane trial having landed — D11) and the PM-6 solo re-verification it
still owes; the fsm-designer
re-alignment (RFC-0059 P6) with the recipes guide as its craft companion;
and the filed-not-worked findings awaiting their phases (the parked
recognizer-language findings; the remaining naive-splitter seats).
