# Validation-parity milestone — port legacy's semantic diagnostics to ng's `validate.rs`

ng emits **20 of legacy framec's 99 diagnostics**. The ~79 missing, by pipeline stage
(frame-compiler-architect inventory, workflow `wf_cf44a7b1-6f1`):

| stage | count | ng home |
|---|---|---|
| **semantic-validate** | **57** | `compiler/src/validate.rs` (+ `resolve.rs` symbol table) |
| emit | 9 | `text/emit` (target-capability gates) |
| syntactic-parse | 6 | parser |
| lexical-scan | 1 | scanner |
| other | 10 | mixed |

The dominant gap is **semantic-validate** — the post-`resolve`, pre-`emit` stage. ng silently
**accepts** programs the oracle **rejects**, so every emit milestone landed on top inherits
"ng accepts what the oracle rejects." This milestone closes that class. `validate.rs` runs
before codegen, so this is emit-neutral (no backend bytes move) but **shared** (it changes what
every target accepts) → it gets the **heavy** gate.

## DoD — per ported E-code
1. **Implement** the check in `validate.rs`, walking the AST and cross-referencing via the
   `resolve` symbol table. Match legacy's exact condition + message (see legacy site).
2. **Trigger fixture** the check must reject: the local `-l <t>` oracle errors `E<code>`; ng must
   now ALSO error `E<code>` (non-zero exit, matching message). The differential is **both reject**.
3. **No false positives** (the load-bearing gate): the full positive corpus still emits —
   `faithfulness_diff.sh` `ng-noemit` must NOT increase, suite green. A check that rejects a
   VALID program is a regression, not a win.
4. **Emit unmoved**: `validate.rs` precedes codegen; byte-identical counts unchanged; regen 0-stale.

## Port order (high-impact first; legacy = framec/src/frame_c/compiler/frame_validator/)
| # | code | rejects | legacy site |
|---|------|---------|-------------|
| 1 | **E419** | exit-args `(a) -> $D` but source state has no `<$` | transitions.rs:302 |
| 2 | **E417** | enter-args `-> (b) $D` but target state has no `$>` (mirror of E419) | transitions.rs:334 |
| 3 | E406 | handler params ≠ interface method params | transitions.rs:140 |
| 4 | E416 | system start params ≠ start-state params | attributes.rs:534 |
| 5 | E418 | domain param with no matching domain field | attributes.rs:607 |
| 6 | E610 | `$.x` state var with no initializer | transitions.rs:47 |
| 7 | E601 | `@@:self.X()` — X not an interface method / action / operation | system_checks.rs:434 |
| 8 | E602 | `@@:self.X()` arg count ≠ interface | system_checks.rs:418 |
| 9 | E400 | transition/forward not the last statement in its block | transitions.rs:218 |
| 10 | E116 | duplicate state name | structure.rs:130 |
| 11 | E117 | duplicate handler name | transitions.rs:90 |

Then the MED-impact remainder (E410/E413/E111/E114/E603/E604/E605/E614/E615/E401…) as follow-on
batches. Full 79-code table with messages + impact: the workflow journal.

**Start: the E419/E417 pair** — arg-delivery, mirror checks, both high-impact and common.
