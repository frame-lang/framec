# ATOM — the Agent Task Orchestrator Machine (design note; two-level, per-tool-use, self-recovering)

**Status:** design for review · extends RFC-0058 §7.3 (exploratory) · dogfoods Frame `@@system`s
**Deliverable of:** the sliced-workflow pilot (proved slicing ends reap-deaths) + two design passes
(`frame-fsm-designer` inner + outer, architect runtime).

---

## 1. The problem and the one property that fixes it

Monolithic agent loops die on API/stream reaps and lose hours (this session: Python/Java sat dead
~14 h; C died ×3; the validate cell died ~13 min in then sat 6 h). Worst: **resume reloads the whole
transcript** (1.6 MB) — a huge first call, itself reap-prone, so recovery re-dies.

**The fix is one property: checkpoint after every tool-use with a SMALL durable state, so a reap
costs one tool-use and resumes from a tiny checkpoint — never a transcript reload.** Modeled
recovery and per-state timers follow from it.

Empirically the accidental cost is huge: C M1 was **~300 tool-uses**, not the ~80 of a clean run —
the 3–5× gap is *all* dead cells, relaunches, redundant gates. The harness drives that gap to ~0
without touching the essential work (the byte-exact reverse-engineering search + human rulings).

---

## 2. Grounding — this extends what exists; it doesn't invent

- **RFC-0058 §7.3 (Heron hosting)** is the coarse-grained ancestor: a Frame decide-layer, the model
  call as a **nondeterministic controller behind a task/result envelope**, an **agent-manager machine**
  whose stall states (*launching / patience / suspect / killed / single-retry / narrowed-fallback*)
  prefigure recovery, **durable per-node state files + event log** as the checkpoint substrate, and
  **branch-halt poisoning** + owner-gate-as-control-envelope. §7.3 is exploratory/non-normative → we
  refine it freely and feed changes back as a §10 evolution-log entry.
- **`pipeline_supervisor.gv`** — a real per-*phase* supervisor `@@system` (Idle→Running→Aborted/
  Failed/Done; `begin_phase/complete_phase/record_nonfatal/abort/finish/summary`). We **reuse its
  vocabulary + terminal set** and refine it to the milestone/agent-run grain.
- **Shadows disposition ruling.** Both harness machines **REIFY** (compression: a handful of named
  modes replace scattered orchestration flags; observability: a state survives a reap, a program
  counter cannot; verifiability: model-checkable laws bind to the named graph). The **bespoke
  workflow LEAVES LATENT** — it is a bundle of *predicates + values* whose engine is the harness
  (engine-reified-elsewhere); reifying it as a `@@system` before it grows its own modes would be
  costuming. (Promote only when a workflow develops a multi-mode sub-lifecycle **and** is reused
  more than ~N times.)

---

## 3. Two levels

```
  TaskOrchestrator  (outer — one milestone; a growing TREE of cells)
        │  launches a cell across an ASYNCHRONY SEAM (task envelope in → result envelope out)
        ▼
     AgentLoop         (inner — one cell; a per-TOOL-USE loop)
        │  drives  LLM turn → one tool → checkpoint → repeat
        ▼
     one tool call
```

Both are Frame `@@system`s (`@@[target("python_3")]`), both dependency-free generated classes driven
by a native Python host. Both keep a **small durable checkpoint** and a **host-owned append-only
transcript log** referenced only by offset. The seam between them is deliberate (see §6).

---

## 4. INNER — `@@system AgentLoop` (per-tool-use)

**States** (per-tool-use grain):

| state | role | `$>` enter | `<$` exit |
|---|---|---|---|
| `$Idle` | start/resume anchor (idempotent) | — | — |
| `$Live` | **HSM parent** — owns `timeout()`/`cancel()` (children `=> $^`) | — | — |
| `$Think` | one LLM turn = the **controller** (nondeterminism confined here) | arm `think_budget`; issue LLM request at `transcript_offset` | stop+journal |
| `$ExecTool` | execute **one** tool (the reap hot-spot) | **PRE-tool checkpoint**; arm per-tool budget; `invoke_tool()` | stop+journal |
| `$Record` | deterministic commit | append result to transcript log; advance cursor; **POST-tool checkpoint**; auto-route | — |
| `$Recover` | **classifier hub** → one named recovery state | `classify(cause,attempt,max)` | — |
| `$RetryTool` | bounded same-tool re-issue | `attempt+=1; backoff();` → `$ExecTool` | — |
| `$Escalate` | owner-gate / narrowed-fallback | emit control envelope up; suspend | journal decision |
| `$Abort` | **named** terminal failure | terminal-ledger + branch-halt poison; → `$Error` | — |
| `$Done` / `$Error` | terminal success / failure | emit result envelope / mark failed; final checkpoint | — |

**Events:** `run(task)`, `resume()`, `llm_response(action)`, `tool_result(data)`, `tool_error(err)`,
`timeout()`, `owner_decision(retry)`, `cancel()`, `get_status()`/`summary()`.

**Reap-survival core:** `$ExecTool -resume-> $ExecTool` re-invokes the SAME tool from the PRE-tool
checkpoint — **one-tool-use rehydrate**. Checkpoint is small (`phase, tool_name, tool_args_ref,
attempt, cursor, transcript_offset, budget`); the transcript is external, referenced by offset.

*(Frame has no transition guards → tool-vs-final and failure classification are native `if` in the
handler body.)*

---

## 5. OUTER — `@@system TaskOrchestrator` (milestone-tree)

Refines `pipeline_supervisor.gv` to the agent-run grain. `$Running` is the HSM parent owning the
cross-cutting envelope handlers (`owner_ruling`, `reap_signal`, `abort`, `record_nonfatal`,
`summary`); children inherit via `=> $^`.

| state | role | recovery analog (inner) |
|---|---|---|
| `$Idle` | seed the frontier from the bespoke `TaskSpec` | — |
| `$Plan` | derive the launchable frontier; route to launch / owner-gate / done / failed | — |
| `$Launch` | **PRE-launch checkpoint**; build task envelope; arm cell patience; spawn an `AgentLoop` cell across the seam | — |
| `$Await` | running-under-patience; `result(env)` → `$Gate` | — |
| `$Suspect` | patience blown → probe liveness (§7.3 stall) | `$Live.timeout()` |
| `$Gate` | **adjudicate the result envelope** via the *bespoke gate predicate* → Merge / Spawn / OwnerGate / Retry / Abort | `$Recover` (at cell-result grain) |
| `$Spawn` | grow the tree: append discovered cells (leaf **or** shared sub-milestone) with dep edges | — |
| `$Merge` | **the single durable-writing step** (idempotency anchor); commit artifacts; propagate to dependents; **POST-merge checkpoint**; → `$Plan` | `$Record` |
| `$Retry` | re-launch **same** cell, narrowed brief, bounded attempt | `$RetryTool` |
| `$Reap` | cell/host died → **RELAUNCH FRESH** from `durable_anchor` (not resume) | `resume→$ExecTool` (but *fresh*, not resume) |
| `$OwnerGate` | adjudication (legacy-bug vs ng-wrong); durable pause; resume on `owner_ruling` | `$Escalate` |
| `$Abort` | branch-halt poison; skip transitive dependents; **siblings survive** | `$Abort` |
| `$Done`/`$Failed`/`$Aborted` | terminals (exact `pipeline_supervisor.gv` set) | — |

Every state pairs a `$>` that **arms** a calibrated patience/eval budget + stamps telemetry with a
`<$` that **stops + journals** elapsed — the inner timer discipline lifted to the cell grain.

---

## 6. Composition — across an asynchrony seam (not in-memory)

The two machines compose **across a seam** (RFC-0058 §5), NOT as a synchronous `drives` edge and NOT
as an in-memory `domain: AgentLoop` field. Three grounds:

1. **Reap independence** — a cell must die without killing the supervisor; an in-memory field dies
   *with* its host.
2. **Blob size** — an `AgentLoop` domain field would force (`E828`) the supervisor blob to embed the
   entire inner state; the seam embeds only a checkpoint **reference** (same reason the transcript is
   offset-referenced).
3. **Nondeterminism confined to `act`** — the cell run *is* §7.3's controller; the supervisor stays
   deterministic + auditable.

- **Task envelope (in):** `{cell_id, intake_brief, subgraph, tool_set, per_tool_budgets,
  gate_predicate_ref, escalation_points, spawn_rules}`.
- **Result envelope (out):** `{cell_id, verdict, artifacts_ref, transcript_offset, discovered:[…],
  needs_ruling?, elapsed}`.

**Nested grains, one transcript:** inner checkpoints per-tool-use (PRE/POST-tool); outer per-
milestone-step (PRE-launch/POST-merge); the transcript is a host-owned append log at both levels,
the outer only *referencing* the offset out of the result envelope.

---

## 7. Emergent scope — the milestone is a growing tree, held as data

The milestone is a **worklist over a growing cell tree, entirely in persisted domain** (survives a
host reap). **No `push$`/`pop$`** — the pushdown stack is a *runtime* structure that dies with the
host; reap-survival requires the resume structure to be *data in the blob*. So it is a flat worklist
with parent pointers, and the Frame control state stays small + checkpointed.

```
Cell { id, parent_id, kind:{Leaf|SubMilestone},
       status:{Pending|InFlight|Merged|Poisoned|AwaitingRuling},
       brief, subgraph_ref, tool_set, budgets, attempt,
       deps, dependents, durable_anchor, provenance }
```

- **Frontier is derived, never stored:** `$Plan` computes `launchable = Pending ∧ all(deps merged) ∧
  ¬poisoned` — finite control, data register (not a Turing tarpit).
- **Growth at `$Spawn`:** a leaf gap appends a `Leaf`; a *"gap is shared"* discovery appends a
  `SubMilestone` that becomes a **dep** of the dependents that need it (Python M5 gap-3 → an
  all-backend scanner sub-milestone blocking the E-codes that need it, not the others).
- **Poison at `$Abort`:** mark `Poisoned`, mark transitive dependents skipped; **siblings survive**.
  `$Failed` only when a *mandatory* (DoD-root) cell is poisoned and nothing else is launchable.
- **Durability:** `@@[persist]` saves the whole tree; `@@[no_persist]` excludes transcript scratch →
  small, reload-cheap blob. Two reap boundaries, two data-driven recoveries.

---

## 8. Recovery — the load-bearing asymmetry

| grain | on death | mechanism |
|---|---|---|
| **inner** (tool) | `restore_state` + `resume` → `$ExecTool` re-invokes the **same tool** from PRE-tool checkpoint | **resume** (one-tool-use rehydrate) |
| **outer** (cell/host) | `$Reap` → **relaunch a FRESH cell** from `durable_anchor` | **relaunch fresh** — *not* resume (transcript reload is itself reap-prone, empirically) |

**Idempotency ruling that makes relaunch-fresh safe:** `$Merge` is the *only* durable-writing step;
in-flight cells write only to `scratch`; relaunch-fresh discards scratch and re-derives — so a
mid-verify reap of E406 cannot double-apply a half-made edit.

---

## 9. THE CONTRACT — general harness vs bespoke workflow

The harness owns the **machines** (process); the bespoke spec owns the **predicates** (laws) +
**values** (config), and contributes **zero states**.

**Harness GUARANTEES (built once, two `@@system`s):** per-tool-use *and* per-milestone-step
checkpoints; recovery (inner resume-same-tool, outer relaunch-fresh); per-state timers + telemetry
at both grains; the stall protocol (`$Await→$Suspect→$Reap/$Retry`), branch-halt poison, owner-gate;
tree management (frontier derivation, spawn, dep propagation, persistence); the controller protocol.

**Bespoke workflow SUPPLIES (bound at named sites):**

| supplied | role | bound at | mechanism |
|---|---|---|---|
| step-shape (envelope template, subgraph extractor, result schema) | value | `$Launch $>` | `TaskSpec` param |
| **DoD / gate predicate** `(result,state)→{Pass,Fail(recov\|fatal),NeedsRuling,Discovered}` | **predicate** | `$Gate` | `operations:` override / fn ref |
| escalation / owner-gate points | predicate | `$Gate → $OwnerGate` | verdict classifier |
| spawn rules (discovered → cells + edges) | predicate/value | `$Spawn` | `operations:` override |
| tool-set + per-tool budgets | value | inner `$ExecTool $>` | envelope fields |

**Form:** general harness = **`@@system`** (reify); bespoke = **declarative spec + a small predicate
script**, NOT a per-task `@@system`. Promote to a `@@system` only at the named void condition
(own multi-mode sub-lifecycle ∧ reused > ~N×).

---

## 10. First pilot — the validation-parity E-code port, E406 through both levels

The port supplies **only** an envelope schema, budgets, a gate predicate (`reject-matching-oracle ∧
zero false positives`), an owner-gate trigger (legacy-bug-vs-ng-wrong), and a spawn rule (shared-
helper gap). **Zero new states.**

1. `$Idle -begin_task(ECodePortSpec)-> $Plan`; frontier = {E401…E829}; E406 launchable.
2. `$Plan → $Launch`: PRE-launch checkpoint; build E406 envelope
   (`tool_set:[read, edit validate.rs, faithfulness_diff (LONG/reap-prone), battery]`); spawn an
   AgentLoop cell across the seam; `-> $Await`.
3. **Inner cell** runs: `$Think→$ExecTool(read legacy E406)→$Record→…→$ExecTool(faithfulness_diff:
   PRE-tool checkpoint, LONG budget)→$Record→…→$Done`.
   - *Mid-verify tool reap* → inner `resume` re-runs `faithfulness_diff` from its checkpoint;
     supervisor never notices.
   - *Whole-cell death* → `$Await→$Suspect→$Reap` → relaunch a **fresh** E406 cell (no transcript
     reload).
4. `result(env) -> $Gate`; the **bespoke predicate** routes: pass+clean → `$Merge`; pass+shared-gap
   → `$Spawn` (append the shared sub-milestone as a dep) → `$Merge` → `$Plan`; needs-ruling →
   `$OwnerGate` (durable pause; `owner_ruling("legacy bug")` → `$Gate` re-adjudicate / `$Spawn` a
   journal-the-bug cell); recoverable fail → `$Retry`.
5. `$Merge`: single durable write; POST-merge checkpoint; `-> $Plan`. Loop to `$Done`.

**Checkable laws realized on the pilot:** *every path to `$Merge` passes through `$Gate`*; *no
`$Merge` of a `NeedsRuling` cell without a prior `owner_ruling`*.

---

## 11. Invariants (RFC-0058 §§4,7,8 — binding)

1. Nondeterminism confined to the controller; orchestration deterministic + auditable.
2. Every error/terminal transition is a **named state** — no silent drop.
3. **Any watchdog/liveness signal is calibrated against a known-healthy baseline first** — the family
   killed 3 healthy agents on an uncalibrated one (we re-lived this exactly). Budgets per-tool +
   calibrated (`faithfulness_diff ~1200s`, `cargo test ~600s`, fs/grep ~60s, `$Think ~120s`).
4. Sync when a result gates the next step.
5. Structural boundaries: illegal transitions unrepresentable; a machine writes only its own durable
   state.
6. Versioned durable state; the milestone tree is a **register, not a `push$`/`pop$` stack**.

---

## 12. Open decisions (my lean; ★ = wants your eye)

| # | decision | lean |
|---|---|---|
| 1 | inner `$ExecTool` resume: one event vs split states | one `resume()` |
| 2 | budgets: static table vs adaptive p95 | static-from-telemetry now |
| 3 ★ | `$Escalate`/`$OwnerGate`: sync block vs async suspend | **async durable suspend** (human-scale; §8 sync is for machine-scale gating) |
| 4 | `$Record` reify vs fold | reify |
| 5 ★ | host now: in-session Python vs presume Heron | **in-session native host first** (§7.3 sequencing); feed back to §7.3 |
| 6 | seam protocol owner: supervisor vs Heron | Heron owns transport; supervisor owns the *decision* (its recovery states) |
| 7 | verify placement | verify lives **inside** the cell's `$ExecTool`; `$Gate` only *adjudicates* |
| 8 | parallel cells | single-cell pilot first; `inflight: Set` as an orthogonal extension after |
| 9 | owner-abandoned terminal | fold into `$Aborted(owner_abandoned)` — no new terminal |

---

## 13. Build plan (incremental, each step verifiable)

1. **`AgentLoop.fpy`** — author + compile to python_3; unit-test transitions.
2. **Native Python host** — Anthropic SDK controller + tool executor + `threading.Timer` watchdog +
   append-only transcript log + `@@[persist]` checkpoint I/O.
3. **Calibrate** per-tool budgets from the pilot telemetry.
4. **Kill-test** — real task, `kill -9` mid-`$ExecTool`, confirm one-tool-use resume (load-bearing).
5. **`TaskOrchestrator.fpy`** — author + compile; wire the seam (task/result envelopes) to
   AgentLoop; single-cell.
6. **Pilot E406** — plug the E-code-port bespoke spec into the contract; run it end-to-end; compare
   death-cost + telemetry to pattern A.
