# System Inventory — the machines the cleanroom must be built out of

**Status:** planning doc. Grounded in the 4.6.0 dogfooded source
(`/Users/marktruluck/projects/framec` → `framec/src/frame_c/compiler/*.frs`) and in the
cleanroom's current hand-written `compiler/src/`. See the journal entry *"2026-07-14 —
FUBAR: I rebuilt the scanner as the exact hand-rolled loop P9 diagnosed as the disease."*

## Why this doc exists

The cleanroom was supposed to be built **out of `@@system` machines** — Frame owning the
control structure, native functions as opaque leaves — the way 4.6.0 is (75 dogfooded
`.frs` machines, top to bottom). It was not. The segmenter, the body/statement scanner, and
the island recognizers are hand-written `while i < to` byte-loops. This doc inventories
what the 4.6.0 architecture actually is, maps each piece to the cleanroom's hand-written
equivalent, and lays out the conversion — **every damn thing in a system.**

**This iteration uses `@@system` only, never `@@fsm`.** The machines are `@@system`
cursor-drivers over a *borrowed* buffer (RFC-0042.1 "new scanning capabilities"): `over(bytes)`
borrows the input with no copy, `scan_at(i)` probes from a position, `push$`/`pop$` is a
real kind-matched pushdown for bracket nesting, and a counter automaton counts openers vs
closers. Native leaves do only *transformation* (build the unescaped string, assemble a
node) — never recognition.

## ITEM ZERO — the gating capability (DONE for Rust — #209 Option 2)

**Status: implemented for the Rust backend.** `@@[scan(u8)]` on a `@@system` now emits the
`SInput` trait (+ `&[u8]` zero-copy / `Vec` / closure impls), a machine generic over its
input, `over(src)` (construct without running), and `scan_at(i)` (positioned, iterative,
restartable; accepts iff it ends in `$Accept`, leaving the extent in `pub cursor`). Proven
end-to-end: recognizes `"he\"llo"` (escaped quote), rejects a non-string, scans a
50 000-byte input iteratively with no stack growth. This is #209 Option 2, realized in the
cleanroom. Python/Java/C emission and the string-blindness-proof `StringScan.frs` conversion
are the next steps. (Known scanner limitation surfaced en route: a transition inline with a
closing brace on one line — `if x { -> $R }` — has its trailing `}` swallowed by
`to_end_of_line`; write transitions on their own line until the scanner itself is a system.)



Verified by reading `compiler/src/text/emit/`: **`framec-ng`'s `@@system` codegen does not
emit the positioned-scanner API.** No `over()`, no `scan_at()`, no `cursor`. A plain
`@@system` compiles to the standard machine shape (interface methods + `Compartment` +
`HashMap` state) — it cannot borrow a buffer or scan at a position.

**So nothing below can be converted until `framec-ng` can compile a scanning `@@system`.**
The capability to add (RFC-0042.1 semantics, `@@system` only):

- `SystemName::over(bytes: &[u8]) -> SystemName` — construct a machine that *borrows* the
  input (no owned copy; this is the fix for the O(n²) probe that forced the hand loops).
- `scan_at(&mut self, i: usize) -> bool` — run the recognizer from position `i`; leave the
  end position in `self.cursor`.
- A `cursor: usize` the handlers advance, and a way for handlers to peek the byte(s) at the
  cursor (the native-leaf peek oracle).
- `push$`/`pop$` already exist in the cleanroom as a pushdown; confirm they hold to useful
  depth for bracket matching.

Until this exists, the conversion is blocked at the compiler-capability layer, not the
authoring layer. This is the first deliverable.

## The 4.6.0 dogfooded architecture (75 machines)

Top-to-bottom, the shipping compiler IS a set of Frame machines. `PipelineFsm`
(`pipeline_supervisor.frs`) makes the whole compile a state machine —
`Segment → Parse → ModuleGates → Graphviz → ValidateCodegen → Assemble → Done` — where each
phase is a state whose `$>` enter-handler runs the phase and transitions to the next; the
transition graph *is* the control flow. `SystemBackbone` (`system_backbone.frs`) makes the
parser's outer grammar a self-looping backbone. Below them, by phase:

| Phase | Machines (4.6.0 `.frs`) | Mult. | Role |
|---|---|---|---|
| **Orchestration** | `pipeline_supervisor` (PipelineFsm), `pipeline_parser/system_backbone` | 1 + 1 | the compile pipeline & the parser outer grammar, as state machines |
| **Lexical scanners** | `string_scan_fsm`, `ident_scan_fsm`, `number_scan_fsm`, `paren_balance_scanner` | 4 | recognize a string / ident / number extent; balanced-bracket nesting (pushdown) |
| **Segmentation / islands** | `attribute_scanner`, `call_site_scanner`, `domain_scanner`, `transition_meta_scanner` | 4 | recognize `@@[…]`, `@@Sys()`/`@@:self.x`, `domain:` decls, transition heads |
| **Native-region skippers (water)** | `native_region_scanner/*_skipper` (c, cpp, csharp, erlang, java, js, kotlin, lua, php, python, ruby, rust, swift, ts) + `expr_scanner`, `context_parser`, `state_var_parser`, `frame_structural_skipper`, `erlang_scope_scanner` | ~19 | skip over the user's native code per target grammar (the Oceans "water") |
| **FSM parser** | `fsm_parser/{fsm_lexer, fsm_decl_parser, state_parser, statement_parser, expression_parser, action_block_parser, actions_block_parser, domain_block_parser}` | 8 | the Frame grammar, level by level |
| **Validators** | `fsm_validator`, `section_order_validator`, `hsm_cycle_validator/hsm_cycle_walker`, `reachable_validator/reachable_walker` | 4 | well-formedness, section order, HSM cycles, reachability |
| **Body-closers (statement terminators)** | `body_closer/*` (c, cpp, csharp, erlang, go, java, js, kotlin, lua, php, python, ruby, rust_lang, swift, ts, frame_structural) | ~16 | where to place the statement terminator, per target |
| **Codegen** | `codegen/{output_block_lexer, output_block_parser, output_block_parser_erlang}`, `codegen/state_dispatch/handler_methods/java_await_rewrite` | 4 | parse the emitted output-block template; the Java await rewrite |
| **Type-maps / name / target** | `type_map/{cpp,csharp,go,java,kotlin,rust,swift}_map_type`, `rust_dispatch_convert`, `rust_owned_promotion`, `name/{pascal_case_variant, to_snake_case}`, `target_query/is_dynamic_target`, `erlang_classifier`, `gdscript_multisys` | ~15 | per-target type spelling, casing, dynamic-target query, target specialties |

**Wiring (self-hosting):** each machine is `spec.frs` → compiled by framec itself to a
committed `spec.gen.rs` → a thin `mod.rs` that `include!`s the generated system and wraps it
with native leaves. `build.rs` does **not** auto-regen (chicken-and-egg); regen is a
deliberate manual step (`framec -l rust` on the spec, commit both). The cleanroom will
self-host the same way, with `framec-ng -l rust` as the generator.

## The cleanroom's current hand-written surface (what must convert)

All of `compiler/src/text/scan/` is hand-written byte-loops. Mapping each to its 4.6.0
counterpart and the `@@system` it must become:

| Cleanroom (hand-written today) | 4.6.0 counterpart | Target `@@system` | Core for j/p/r/c? |
|---|---|---|---|
| `scan/mod.rs` — item segmenter (BOM / pragma / `@@system` / native discovery loop) | `PipelineFsm` *Segment* + `frame_structural_skipper` + `attribute_scanner` | `@@system Segmenter` | **yes** |
| `scan/machine.rs` — `decl_section`, state scan, `handler_at` (section/state/member outer grammar) | `SystemBackbone`, `state_parser`, `fsm_decl_parser` | `@@system SectionBackbone` | **yes** |
| `scan/machine.rs` — `frame_stmt` (transition / push$ / pop$ / forward dispatch) | `statement_parser`, `transition_meta_scanner` | `@@system StatementScanner` | **yes** |
| `scan/machine.rs` — `parse_after_arrow`, `balanced`, `to_end_of_line` | `paren_balance_scanner`, `transition_meta_scanner` | `@@system TransitionHead` (uses push$/pop$) | **yes** |
| `scan/parts.rs` — `native_parts` (decompose water into text/literal/ref/instantiate/embed) | `expr_scanner`, `native_region_scanner` (rust/py/java/c skippers), `context_parser` | `@@system NativeParts` | **yes** |
| `scan/parts.rs` — `frame_ref_at` (`$.x`, `@@:self.f`, `@@:params`, …) | `context_parser`, `state_var_parser` | `@@system RefScanner` | **yes** |
| `scan/parts.rs` — `instantiation_at` (`@@Sys(args)`) + `embed_call_at` (`@@:self.f.m()`) | `call_site_scanner` | `@@system CallSiteScanner` | **yes** |
| `scan/parts.rs` — `split_top_commas`, `match_paren`, `split_top_eq` | `paren_balance_scanner` | folded into `CallSiteScanner` / `TransitionHead` (push$/pop$) | **yes** |
| `scan/lex.rs` — string / comment / literal recognition | `string_scan_fsm`, `ident_scan_fsm`, `number_scan_fsm` | `@@system StringScan`, `IdentScan`, `NumberScan` | **yes** |
| `validate.rs` — E402 (bad transition target), E609, ref checks | `section_order_validator`, `reachable_walker`, `hsm_cycle_walker`, `fsm_validator` | `@@system ReachableWalker`, `SectionOrder`, `HsmCycleWalker` | partial |
| `emit/driver.rs` — the body-statement walk + terminator sequencing | `codegen state_dispatch`, `body_closer/*`, `output_block_parser` | `@@system EmitDriver` (PipelineFsm-style walk) | later |
| `resolve.rs` — symbol-table assembly | (native glue in 4.6.0 too — **not** a machine) | stays native | n/a |
| `tree/*`, `emit/atom.rs`, backend spellings | native constructors / glue | stays native | n/a |

**Not everything is a machine.** Symbol-table assembly, tree construction, `Atom`/`Place`
builders, and the per-backend *spellings* are legitimate native glue — 4.6.0 keeps its
equivalents native too. The conversion targets the **recognizers and the control walks**,
which is exactly the hand-rolled-loop surface.

## Minimal core set (java / python / rust / c only)

The cleanroom targets 4 backends, so the ~19 per-language skippers, ~16 body-closers, and
~9 type-maps collapse to **4 each** (rust/python/java/c). The load-bearing core to deliver:

1. **ITEM ZERO** — positioned-scanner codegen for `@@system` (`over`/`scan_at`/`cursor`).
2. `StringScan` (+ comment/literal) — the string-blindness fix lives here first.
3. `Segmenter` — item-level discovery.
4. `SectionBackbone` + `StatementScanner` + `TransitionHead` — the Frame grammar.
5. `NativeParts` + `RefScanner` + `CallSiteScanner` — the water islands.
6. Validators (`ReachableWalker`, `SectionOrder`, `HsmCycleWalker`) — after the front end.
7. `EmitDriver` — last; the driver walk as a machine (PipelineFsm-style).

The peripheral 13-target skippers/body-closers/type-maps and the erlang/gdscript
specialties are **out of scope** for the cleanroom's 4-backend surface.

## Conversion order (start at the top, where the string-blindness lives)

0. **Build positioned-scanner codegen** in `framec-ng` (`@@system over/scan_at/cursor`).
   Nothing else can begin until a scanning `@@system` compiles.
1. **`StringScan.frs`** — smallest, highest-value: the string/comment recognizer is where
   the string-blind `in_string: u8` family lives. Self-host it, wire via `mod.rs` leaf,
   prove it agrees with the hand lexer by differential test, then delete the hand path.
2. **`Segmenter.frs`** — item-level discovery over the borrowed buffer.
3. **`SectionBackbone` / `StatementScanner` / `TransitionHead`** — the Frame grammar, level
   by level, mirroring `SystemBackbone`.
4. **`NativeParts` / `RefScanner` / `CallSiteScanner`** — the island recognizers
   (`instantiation_at`, `embed_call_at`, `frame_ref_at` become handler dispatch).
5. **Validators** as walker machines.
6. **`EmitDriver`** — the emit control walk as a machine, last.

Each step: author `X.frs` → `framec-ng -l rust --emit X.frs > X.gen.rs` → `mod.rs` with
`include!` + native leaves → differential/behavioral test → delete the hand-written path →
commit `.frs` + `.gen.rs` + `mod.rs`. The `frame-style-auditor` (Mandate 0) reviews each
commit's diff.

## Open questions

- **Item-zero scope:** does positioned scanning reuse the existing `@@system` compartment
  machinery (cursor as a domain field, peek as a native leaf), or does it need new codegen?
  Decide before writing any `.frs`.
- **Bootstrap ordering:** the cleanroom must be able to compile its *own* scanner specs. If
  a spec uses a construct the cleanroom doesn't yet emit, that construct is a prerequisite —
  surface it as a blocker, don't hand-roll around it (that is how we got here).
- **Parity gate:** each converted machine must be byte-for-byte (or behaviorally) identical
  to the hand path it replaces, proven by running — the corpus + the test-env category
  sweeps are the gate.
