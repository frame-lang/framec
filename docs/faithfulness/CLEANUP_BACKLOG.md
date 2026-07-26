# Faithfulness — cleanup backlog (incidental improvements)

Non-blocking improvements surfaced during the milestone waves. **These do NOT block landing a
milestone** — they are logged here and cleaned up in a dedicated pass **after all waves are done**.

**Convention (for cells):** when a cell finds an improvement that is real but *not* required to land
its milestone's anchor, it **logs it here and moves on** — it does not chase it or block on it. A
milestone's anchor going byte-identical is the bar; everything else is batch/cleanup.

Each entry: what · where · why non-blocking · the fix.

---

## Correctness / faithfulness gaps (the batch-validation pass will hit these broadly)

- **Rust kernel-native indent (general).** Legacy indents *every* kernel-handler native at
  `source_col + 12` (e.g. 28); ng emits at `base + 12` (12). The reentrancy cell scoped the fix to
  *self-call* natives (`selfcall_stmt_rel`, `rust.rs`) to close `foundation_selfcall` with zero
  `.gen.rs` churn. *Non-blocking:* the anchor is byte-identical. *Fix:* generalize (Rust kernel
  natives → `source_col`), re-bless ~13 self-hosted `.gen.rs` + verify fixpoint. Owner leans
  generalize (faithful; deep indent only in generated source).
- **Bare self-call guard + indent.** `@@:self.m()` as a *bare* statement (`Stmt::SelfCall`, k==8)
  still lacks the `_transitioned` guard and is mis-indented (8 vs 12) in Rust/Python. *Non-blocking:*
  no M1 fixture exercises it (`foundation_selfcall` is expression-position). *Fix:* fire
  `reentrancy_guard` on the `Stmt::SelfCall` path + its indent; add a bare-self-call fixture.
- **`-> (enter) pop$` (pop with enter args).** Spelling (`python.rs::pop_enter`: `= [...]` vs
  oracle `.append(...)`) + ordering (driver stamps enter-args after `__transition`, oracle before).
  *Non-blocking:* zero corpus fixtures exercise it. *Fix:* driver interleaves the stamp between pop
  and transition (shared change) + the leaf spelling.
- **EmbedCall.args.** An embedded self-call carrying a frame-ref arg still ships the args verbatim
  (earlier-session finding). *Fix:* lower the frame-ref through `render_args` in the embed path.

## Consistency / test hygiene (do once at the end)

- **Action trailing-whitespace journaling.** Owner ruled *keep ng clean* (journal as ng-cleaner,
  run-result identical). *When M8 lands:* enumerate the ~16 `linux/*`/`capabilities/*` fixtures whose
  ng-vs-oracle delta is trailing-whitespace-only and add them to `intentional_divergences.txt`.
- **Python +1 clean-tail partial.** Saved as `scratchpad/python_celltail_wip.patch`; fold into M8.

## Deferred features (own milestone, not incidental cleanup — listed for tracking)

- **Persist (M7).** Java persist stubbed; RFC-0056 persist ported in M7. Confirm the persist oracle
  (local 4.6.0.x vs RFC-0054/4.6.1) before starting M7.
- **Emitted-Python typing preamble modernization** — GitHub #254 (`List/Dict/Optional` → PEP
  585/604). PRE-RELEASE, after all faithfulness lands, applied to BOTH compilers together.

---

## Log (append as cells surface incidentals)
<!-- date · milestone/cell · item · one-line -->
- 2026-07-26 · Rust M1 reentrancy cell · Rust kernel-native indent (general) + bare-self-call guard.
- 2026-07-26 · Python M6 cell · `-> (enter) pop$` spelling+ordering (no corpus fixture).
