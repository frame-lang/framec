---
title: "RFCs"
nav_order: 9
---

# Frame RFCs

Index of Frame RFCs. The RFC process is documented in
[`STYLE.md`](STYLE.md). New RFCs should follow that style guide.

**RFC numbering** is monotonically increasing. Skipped numbers
correspond to drafts that were never written, RFC slots reserved
for an idea that was then absorbed into another RFC, or pre-public
exploratory work that wasn't promoted to a numbered RFC. Skipped
numbers are not re-used.

| # | Title | Status | Cross-refs |
|---|---|---|---|
| 0001–0005 | — | (unassigned) | numbering reserved; no documents |
| [0006](rfc-0006.md) | Self Interface Call — `@@:self.iface()` | Implemented (with revisions) | foundational |
| 0007 | — | (unassigned) | numbering reserved; no document |
| [0008](rfc-0008.md) | Extended `pop$` Transition Syntax | Implemented | foundational |
| [0009](rfc-0009.md) | Static States — Compile-Time-Known Identity | Draft (parking lot) | |
| [0010](rfc-0010.md) | Interpolation-Aware String Scanning via Frame Automata | Draft | |
| [0011](rfc-0011.md) | System Base Classes | Implemented | |
| [0012](rfc-0012.md) | Persist Stress Testing | Amendment shipped (Phases A–B + D); Phase C deferred | superseded in part by [0016.1](rfc-0016-1.md) for the `@@[save]`/`@@[load]` form |
| [0013](rfc-0013.md) | Annotation Syntax — `@@[...]` | Wave 1 + Wave 2 shipped; Wave 3 open | foundational for [0014](rfc-0014.md), [0015](rfc-0015.md), [0016](rfc-0016.md) |
| [0014](rfc-0014.md) | `@@[main]` — Module-Level Primary System | Wave 1 shipped | |
| [0015](rfc-0015.md) | Factory-Only System Construction | Shipped in 4.1.0 | partially superseded by [0017](rfc-0017.md) (init mechanism); see [0016.1](rfc-0016-1.md) for save/load form |
| [0016](rfc-0016.md) | Selective Domain Persist | Draft — deferred; not shipped | partially superseded by [0016.1](rfc-0016-1.md) (the `@@[no_persist]` form shipped; `@@[persist_fields]` still deferred) |
| [0016.1](rfc-0016-1.md) | Amendment — `@@[no_persist]` codegen | Shipped (2026-05-15) | amends [0012](rfc-0012.md); companion to deferred [0016](rfc-0016.md) |
| [0017](rfc-0017.md) | Init Decouple — bare ctor + factory split | Accepted; shipped | companion to [0015](rfc-0015.md), [0018](rfc-0018.md) |
| [0018](rfc-0018.md) | Re-entrant interface dispatch from lifecycle handlers | Resolved (crash); superseded in part by [0019](rfc-0019.md) | construction-context push fix survives; lifecycle-semantics passages quote pre-0019 |
| [0019](rfc-0019.md) | Uniform `$>` / `<$` dispatch (cascade removed) | Accepted (2026-05-12); shipped | supersedes in part [0018](rfc-0018.md); breaking change in 4.2.0 |
| [0020](rfc-0020.md) | Runtime Reference Architecture | Authoritative (normative); aligned with [0019](rfc-0019.md) | companion to [0015](rfc-0015.md), [0017](rfc-0017.md), [0018](rfc-0018.md), [0019](rfc-0019.md), [0021](rfc-0021.md) |
| [0021](rfc-0021.md) | Runtime Performance Optimizations | Draft (parking lot) | companion to [0020](rfc-0020.md) |
| [0022](rfc-0022.md) | Cross-file `@@import` directive | **Superseded by [0024](rfc-0024.md)** | historical |
| [0022.1](rfc-0022-1.md) | `@@import` semantics on Java/C#/Go | **Superseded by [0024](rfc-0024.md)** | historical |
| 0023 | — | (unassigned) | numbering reserved; no document |
| [0024](rfc-0024.md) | Remove `@@import` — host-language imports via Oceans Model | Accepted; shipped in 4.2.0 | supersedes [0022](rfc-0022.md), [0022.1](rfc-0022-1.md); breaking change; **analysis half amended by [0040](rfc-0040.md)** (emission removal stands) |
| [0025](rfc-0025.md) | Quality remediation — structured errors + typed compartment payload | Accepted; shipped (Rust target) in 4.2.0 | companion to [0026](rfc-0026.md), [0027](rfc-0027.md) |
| [0025.1](rfc-0025-1.md) | Typed lifecycle args — close the stringify gap in the typed-payload contract | Accepted (2026-05-21); shipped in 4.2.1 | amends [0025](rfc-0025.md); resolves FRAMEC_BUGS #34 |
| [0026](rfc-0026.md) | Oceans Model as calculus — pre-backend normalization, preservation theorem, formal grammar | Draft (Exploration) | companion to [0025](rfc-0025.md), [0027](rfc-0027.md); no execution commitment |
| [0027](rfc-0027.md) | In-tree snapshot tests per backend (insta) | Accepted; shipped in 4.2.0 | companion to [0025](rfc-0025.md), [0026](rfc-0026.md) |
| [0028](rfc-0028.md) | In-process framec API | Draft (Forward-looking) | replaces roadmap #171 |
| [0029](rfc-0029.md) | Fuzz infrastructure status + deferred-work catalog | Draft (Status report + forward-looking) | replaces roadmap #172; resolved by [0031](rfc-0031.md) for CI integration |
| [0030](rfc-0030.md) | Fuzz infra catch-up plan — multi-RFC corpus migration | Accepted (execution committed 2026-05-18) | execution companion to [0029](rfc-0029.md) |
| [0031](rfc-0031.md) | Post-release process — RC validation, CI gates, drift detection | Accepted (Process) | resolves CI questions in [0029](rfc-0029.md); supersedes ad-hoc release process |
| [0032](rfc-0032.md) | Remove `@@codegen { ... }` — auto-inference is the path | Accepted; shipped in 4.2.0 | extends [0013](rfc-0013.md); same trajectory as [0024](rfc-0024.md); breaking change |
| [0033](rfc-0033.md) | Idiomatic Rust output — borrowed parameters, lint-clean preamble, expression-form state-var initializers | Draft | builds on [0019](rfc-0019.md), [0025](rfc-0025.md) |
| [0034](rfc-0034.md) | In-process compile checks for every backend's snapshot fixtures | Draft | builds on [0027](rfc-0027.md), [0033](rfc-0033.md) |
| [0035](rfc-0035.md) | Dogfooding inventory — existing FSMs, migration candidates, and single-state test corpus | Draft | builds on [0027](rfc-0027.md), [0033](rfc-0033.md), [0034](rfc-0034.md) |
| [0036](rfc-0036.md) | No-allocation dispatch for `no_std` / interrupt / hot-path use | Draft | builds on [0020](rfc-0020.md), [0021](rfc-0021.md), [0025](rfc-0025.md); lifts + prioritizes [0021](rfc-0021.md) item 1 |
| [0037](rfc-0037.md) | Reserved identifier namespace — the `__` prefix (validator E115) | Accepted | builds on [0025](rfc-0025.md) / [0025.1](rfc-0025-1.md); resolves the #40 residual edge |
| [0038](rfc-0038.md) | Deferred dispatch — `@@[cast]` interface methods, addressing, bring-your-own executor | Draft | builds on [0020](rfc-0020.md), [0025](rfc-0025.md); companion to [0036](rfc-0036.md), [0026](rfc-0026.md) |
| [0039](rfc-0039.md) | Parser as composed Frame state machines | Accepted | builds on [0035](rfc-0035.md) |
| [0040](rfc-0040.md) | Re-introduce `@@import` as analysis-only cross-file resolution | Draft | amends [0024](rfc-0024.md) (analysis half only); builds on [0012](rfc-0012.md), [0015](rfc-0015.md) |
| [0041](rfc-0041.md) | Web persistence — storage-bound save/load for browser targets (`@@[web_persist]`) | Draft | builds on [0012](rfc-0012.md), [0015](rfc-0015.md), [0016](rfc-0016.md) |
| [0042](rfc-0042.md) | `@@fsm` — finite-state recognizer construct | Draft | new construct; runtime model independent of [0020](rfc-0020.md); depends on [0050](rfc-0050.md) for action-body statement grammar |
| [0043](rfc-0043.md) | `@@[async]` — single-driver gate via layered casing/machine | Accepted; shipped in 4.4.0 | builds on [0015](rfc-0015.md), [0017](rfc-0017.md), [0020](rfc-0020.md) |
| [0044](rfc-0044.md) | Kernel context-stack must clean up on exception | Draft | builds on [0020](rfc-0020.md), surfaced by [0043](rfc-0043.md) |
| [0045](rfc-0045.md) | Reserve `@@:system`; relocate state name to `@@:system.state.name` | Accepted; implemented | builds on [0006](rfc-0006.md), [0013](rfc-0013.md); breaking (pre-public-beta) |
| [0046](rfc-0046.md) | `@@:self` — portable, blessed self-reference for fields, calls, embeds | Implemented | builds on [0006](rfc-0006.md), [0013](rfc-0013.md) |
| [0047](rfc-0047.md) | Guard syntax — prior-art survey and design space | Placeholder | survey only; no design decided |
| [0048](rfc-0048.md) | Self-describing argument marshalling for type-blind push sites | Accepted; implemented for C (#83) | builds on [0020](rfc-0020.md), [0008](rfc-0008.md) |
| [0049](rfc-0049.md) | Exception philosophy — errors vs. queries, cross-language fallback | Accepted | builds on [0043](rfc-0043.md), [0044](rfc-0044.md) |
| [0050](rfc-0050.md) | Frame statement syntax — assignment, call, `if/else`, expressions, comments | Draft | precursor to [0042](rfc-0042.md); strictly additive to existing Frame statement vocabulary |

## Other documents in this directory

- [`STYLE.md`](STYLE.md) — RFC style guide. New RFCs should
  follow this format.
- [`frc-future.md`](frc-future.md) — informal scratch of
  ideas that haven't been numbered yet.

## Status taxonomy

- **Draft** — proposed; not implemented; design open for revision.
- **Accepted** — design approved; implementation may or may not
  be in progress.
- **Shipped** — implementation landed in a released framec version.
  CHANGELOG entry exists.
- **Implemented** — older RFCs that predate the
  Accepted/Shipped distinction; treat as shipped.
- **Authoritative** — the RFC is the normative reference for some
  aspect of the system (e.g., RFC-0020 for the runtime kernel).
- **Resolved** — bug-class RFC where the immediate fix shipped.
- **Superseded by N** — the RFC is preserved for history; N
  contains the current contract.
- **Draft (Exploration)** — research-grade; no execution
  commitment.
- **Draft (Forward-looking)** — design captured; pending a
  prioritization decision.
- **Status report** — captures current state of a subsystem
  without committing to changes.

## Cross-reference invariants

When one RFC supersedes another, the relationship is recorded
**bidirectionally**:

- The superseded RFC's status line names the superseding RFC.
- The superseding RFC's header lists what it supersedes.

When checking the index, both directions should agree. The
[0022] / [0022.1] / [0024] cluster and the [0018] / [0019]
cluster are the current cases of this pattern.
