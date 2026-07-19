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

## 2. Discovery register (the paper-worthy claims, each with evidence)

**D1 — A worldview document alone is sufficient agent training.** Blind
two-probe experiment: bias-flagging reproduced (including the hardest
form, accepting a conclusion while rejecting its grounds) and the
machine-finding skill transferred, with novel verified defects as the
by-product. Instruction files measured as packaging, not capability.

**D2 — Layered fallibility works: every stage misses; the next stage's
*differing obligations* catch it.** Seven-plus consecutive engagements
each caught a defect the previous stage missed: finder → designer (the
blind body-fork terminal), designer → reviewer (the clamp that panics
debug builds), reviewer → owner (a guardrail exception; a plan-ledger
overclaim), designer → owner's own ruling (the defaults caveat on
tie-impossibility), reviewer → designer's mechanism claim (generated
machines reset all literal-initialized fields — disproved against four
generated artifacts), examples-agent → repo instrumentation (the doc
validator that couldn't see subdirectories). The layers work because the
obligations differ, not because later stages are smarter.

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

**D7 — Design records are executable by fresh minds.** The first
builder-agent implementation worked from the accepted record alone: no
design decisions of its own, three interpretation-level deviations all
reported, zero blockers, all gates green. The pipeline's central claim —
records self-contained enough that a fresh agent can execute them — held
on first trial.

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

## 4. Open threads (for the paper's future-work section)

The global analysis tier (boundary preprocessing, port joining) gated on
the completed local benchmark; the explain renderer as a deterministic
projection with a linting side effect; hosting the agent tree on a
supervised Frame-machine runtime (agents as effectors, the stall protocol
reified as machine states); the parallel *build* trial; the fsm-designer
re-alignment (RFC-0059 P6) with the recipes guide as its craft companion;
and the filed-not-worked findings awaiting their phases (the parked
recognizer-language findings; the remaining naive-splitter seats).
