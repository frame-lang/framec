# Cauldron coverage map — what the ng dogfood + ATOM already validate

**Status:** groundwork draft. The Cauldron v1 design doc was reviewed in-session but not
saved to the repo, so the principle column below is reconstructed from that review. Rows for
principles I could reconstruct with confidence are filled; **P6, P10, and the full FR-\*/AG-R\***
**catalog are marked TODO — fill them from the source design doc.** The *evidence* column is
the load-bearing part: it names the concrete, already-existing artifact (file / command /
run) that satisfies each principle, so this doc doubles as the index a Cauldron build would
lift its referee from.

## The three organs (the frame this map hangs on)

Cauldron = **discovery compiler** (survey a codebase → machine-inventory graph → drive the
conversion) + **conversion compiler** (per latent machine: template the `@@system` topology,
lift the bodies) + **referee** (certify against a golden master under determinism discipline).

- **Player + orchestration = ATOM** — `AgentLoop` / `TaskOrchestrator` (themselves Frame
  `@@systems`) + the `WriteScopedHost` guard + `ControllerLLM`. Cauldron's `cauldron-agents`
  and the dogfooded `UnitPipeline`.
- **Referee = the ng tooling** — `frame-conversion-warden`, `differential_test.sh` /
  `faithfulness_diff.sh` (vs **4.6.0.33**), `regen_check.sh`, `frame-machine-advocate` /
  `source-machine-finder`, `intentional_divergences.txt` + `DIVERGENCE-JOURNAL.md`.

## Coverage matrix

| Principle (as captured) | ng / ATOM artifact that satisfies it | Status |
|---|---|---|
| **P1** — a trusted base compiler underwrites the conversion | **4.6.0.33** (`~/.frame/local/bin/framec`) bootstraps every `.frs → .gen.rs`; ng cannot launder its own scanner bug into its own generated code | ✅ present |
| **P2** — a frozen behavioral battery is the objective function; a "better" output is still a *failure* unless declared | `differential_test.sh` (byte code **and** run-results vs 4.6.0.33, fresh oracle per fixture) + `faithfulness_diff.sh` ratchet; any un-journaled delta fails, improvements included | ✅ present (richer than byte-only: byte + behavior) |
| **P3** — referee/player split; the deterministic core never calls an LLM | `WriteScopedHost` = the player's only tool-exec boundary; the warden / differential / regen are deterministic scripts that never invoke a model; the sole nondeterministic organ (`ControllerLLM`) sits behind the effector seam (RFC-0062 §3.2) | ✅ present |
| **P4 / AG-R1** — oracle integrity; agents cannot write the golden files | guard path-confinement + `.git`/build-config write bans + argv allowlist; an agent structurally cannot write 4.6.0.33 output or `intentional_divergences` | ✅ present |
| **P5** — topology is templated; only bodies are lifted | how the **37 `.frs`** were produced (mechanical topology, agent-lifted bodies). **But**: ng is past its discovery frontier (D18) — no fresh un-converted machine remains to re-exercise the *decompose→new-topology→lift* arc | ⚠️ validated **historically**; no fresh ng target → Serpent |
| **P6** — *TODO: fill from design doc* | — | ⬜ TODO |
| **P7** — determinism ledger; re-running the pipeline reproduces bytes | `regen_check.sh` self-host fixpoint (ng re-emits its own `.gen.rs` byte-identically; 37/0-stale) + the 4.6.0.33 bootstrap discipline | ✅ present |
| **P8** — explicit divergence policy (Replicate / Accept), each divergence catalogued | `intentional_divergences.txt` + `DIVERGENCE-JOURNAL.md` + a per-entry runtime validator in `legacy_bug_fixes.rs` | ✅ present |
| **P9** — machine-worthiness gate ("does behavior depend on accumulated history?") | `frame-machine-advocate` + `frame_machine_architecture.md` Decision-Tree-A + the `source-machine-finder` survey (**Part 0, this session**). Validated live: it found the reachability register *and* confirmed the campaign's NULL calls | ✅ present + freshly exercised |
| **P10** — *TODO: fill from design doc* | — | ⬜ TODO |
| **P11** — dogfood; the pipeline itself is a Frame system | ATOM's `AgentLoop`/`TaskOrchestrator` are `@@systems`; the walker is 37 `@@systems`; the #219 single-source discipline ("one engine, everyone asks it") | ✅ present |

## What ng validates vs what is Serpent-only

- **ng freshly validates:** the **discovery organ** (Part 0 survey ran end-to-end), the
  **referee / golden-master** (P2), **determinism / fixpoint** (P7), **machine-worthiness**
  (P9), the **divergence catalog** (P8), the **referee/player split** (P3/P4), and that ATOM
  (player) composes with the existing ng referee into one loop — demonstrated on the
  reachability unit below.
- **Validated historically, no fresh ng target:** **P5**'s decompose→new-topology→lift arc,
  spent producing the 37 systems. A fresh exercise needs a less-converted codebase.
- **Serpent-only (ng cannot exercise):** the **behavioral-freeze half** — `cauldron-freeze`
  trace mining, the mode×event coverage matrix, the effect-order differ, the UI. ng's oracle
  is deterministic *emission*, not a runtime battery frozen from a running program.

## Worked example — the reachability consume-and-delete (this session)

The discovery survey's one non-trivial yield was a **#219 single-source violation**, and
converting it exercised the whole referee spine on a real defect:

- **Found by P9/discovery:** `resolve.rs` hand-rolled an iterative graph-reachability
  fixpoint (`persist_reachable`) whose engine already ships as `@@system Reachability` and is
  already consumed by `validate.rs` (W401). One question, two implementations.
- **Converted (consume-and-delete, not a lift):** added a multi-source `reachable_from_seed`
  wrapper on the shipped engine (the single-source `reachable` now delegates → one drive
  path); routed `resolve.rs` through it; deleted the hand loop.
- **Certified (the referee):** the retired hand fixpoint kept transiently as its **own
  oracle** under `debug_assert_eq!(engine == hand)` → **0 trips** across the corpus (54 suites
  × 2 profiles green; `regen_check` 37/0-stale; no `.frs`/`.gen.rs` touched). A deliberate
  **falsification** (starving the engine seed) confirmed the gate had teeth — it tripped on
  the real persisted systems `Saver` / `Sys1_1`. Then the oracle was deleted; only the engine
  call remains.

This is the tighter Cauldron-organ validation: the golden-master + hand-as-oracle machinery
certifies a consume-and-delete exactly as it certifies a lift, and the player organ (already
proven on the E406 write) was not even needed for this unit.

## Gaps / to-formalize (for a real Cauldron build)

1. **Fill P6, P10, and the FR-\*/AG-R\* catalog** from the source design doc.
2. **Wire `UnitPipeline` ↔ the ng referee explicitly** — ATOM's compile/verify step must
   invoke **4.6.0.33** for `.frs → .gen.rs` and require the self-host fixpoint, so the
   composed loop is a single artifact rather than a hand-run sequence.
3. **The Serpent behavioral-freeze half is unbuilt** — trace mining, coverage matrix,
   effect-order differ. ng cannot stand in for it.
