# RFC-0042 `@@fsm` — Conformance Test Coverage

*Auto-generated map from the RFC's *Validation Tests* section to the backing tests in `framec/src`.*  
*Enforced by [`framec/tests/fsm_conformance_coverage.rs`](../../framec/tests/fsm_conformance_coverage.rs) — the build fails if any ✅ ID below loses its backing test.*

## Summary

- **88** RFC conformance IDs
- **82** backed by an executing test
- **6** documented deferrals (see end)

Runtime tests (`codegen/fsm_python.rs`, `pipeline/compiler.rs`) generate recognizer source and **execute it through `python3`**, asserting `accepted`/`return_value`/`cursor`. Front-end tests (`fsm_parser`/`fsm_validator`/`fsm_regex`) assert parse results and diagnostic codes. The Python backend is the reference; the other 16 backends have their own `fsm_<lang>.rs` execution tests.


## Construction and structure (§3)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-001 | Minimal fsm compiles | `fsm_test_001_minimal` <sub>(codegen/fsm_python.rs)</sub><br>`fsm_block_emitted_python_runs` <sub>(pipeline/compiler.rs)</sub> | ✅ |
| FSM-TEST-002 | Missing return type rejected | `fsm_test_002_matched_builtin` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-003 | Missing default rejected | `e705_missing_default` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-004 | Implicit start state with one match | `call_with_probe_arg` <sub>(fsm_parser/mod.rs)</sub><br>`call_expression_parses_via_child` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-005 | Auto-domain field accessible as self.text | `fsm_test_005_self_text` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-006 | Explicit labeled state with transition | `fsm_test_006_transitions` <sub>(codegen/fsm_python.rs)</sub><br>`rust_transitions_and_capture` <sub>(codegen/fsm_rust.rs)</sub> | ✅ |
| FSM-TEST-007 | Stage label and capture | `fsm_test_007_capture` <sub>(codegen/fsm_python.rs)</sub><br>`stage_label_and_capture_ref` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-008 | Unlabeled state cannot be referenced | `e704_capture_ref_unlabeled_state` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-009 | State-arg sigil rejected in fsm header | `e701_sigil_in_header` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-010 | Input parameter type validation | `e713_bad_input_type` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-011 | Explicit override of parameter-derived domain field | `domain_default_expression` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-012 | Type mismatch in explicit redeclaration | `e707_domain_param_type_mismatch` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-013 | Two consecutive unlabeled states rejected | `e704_second_unlabeled_state` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-014 | Numeric label valid | `fsm_test_014_numeric_label_valid` <sub>(fsm_parser/mod.rs)</sub> | ✅ |

## Block ordering (§3.3)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-020 | Canonical order compiles | `fsm_test_020_canonical_order_compiles` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-021 | Domain before states rejected | `e710_domain_before_actions` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-022 | Actions before states rejected | `e710_state_after_block` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-023 | Duplicate domain block rejected | `e711_duplicate_domain` <sub>(fsm_parser/mod.rs)</sub> | ✅ |

## Frame statement syntax (§3.6)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-030 | Multi-statement block with semicolons | `fsm_test_030_action_block_semicolons` <sub>(codegen/fsm_python.rs)</sub><br>`fsm_test_030_full` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-031 | Multi-statement block whitespace-separated | `fsm_test_031_action_block_whitespace` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-032 | If/else statement | `fsm_test_032_if_else` <sub>(codegen/fsm_python.rs)</sub><br>`action_block_if_else` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-033 | Bare name does not refer to domain | — | ⏸ **deferred** — bare-name (non-`self.`) E703 validation not implemented — needs context-aware design |

## Typing and variable scope (§4.1, §4.2)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-100 | Undeclared variable read | `e703_undeclared_read` <sub>(fsm_validator/mod.rs)</sub><br>`e704_undeclared_write` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-101 | Undeclared variable write | `e704_undeclared_write` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-102 | Type mismatch on bare-expression @@:return | — | ⏸ **deferred** — E706 return-type mismatch — no @@fsm type system in v0.1 |
| FSM-TEST-103 | Domain field initializer references parameter | `fsm_test_103_domain_init_references_param` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-104 | Domain variable missing initializer | `e705_domain_field_no_initializer` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-105 | Parameter accessible as self.name in body | `relational_with_call_and_member` <sub>(fsm_parser/mod.rs)</sub> | ✅ |

## Actions section (§3.7)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-120 | Action callable from match | `fsm_test_120_action_callable` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-121 | Action with domain access | `fsm_test_121_action_domain_access` <sub>(codegen/fsm_python.rs)</sub><br>`action_block_call_statement` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-122 | Transition in action rejected | `e712_transition_in_action_body` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-123 | Action callable from embedding action | `fsm_test_123_every_transition_embed` <sub>(codegen/fsm_python.rs)</sub><br>`rust_embed_every_transition` <sub>(codegen/fsm_rust.rs)</sub> | ✅ |

## Match and exhaustiveness (§4.3)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-200 | Failable match without failure_branch rejected | `e701_fallible_match_without_failure_branch` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-201 | Unfailable match without failure_branch allowed | `e701_not_fired_for_nullable_match` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-202 | Explicit failure branch | `fsm_test_400_static_transitions` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-203 | Terminal match without transition | `fsm_test_001_minimal` <sub>(codegen/fsm_python.rs)</sub> | ✅ |

## Alphabets (§6.1)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-250 | Byte alphabet | `fsm_test_250_byte_alphabet` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-251 | Char alphabet | `e713_accepts_char_and_token` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-252 | Char alphabet rejects byte escape | `fsm_test_252_char_rejects_byte_escape` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-253 | Token alphabet | `fsm_test_253_token_alphabet` <sub>(codegen/fsm_python.rs)</sub><br>`e713_accepts_char_and_token` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-254 | Token alphabet rejects character class | `fsm_test_254_token_rejects_char_class` <sub>(fsm_regex/mod.rs)</sub> | ✅ |

## Regex dialect (§6)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-300 | Backreference rejected | `fsm_test_300_backreference_rejected` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-301 | Recursion rejected | `fsm_test_301_recursion_rejected` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-302 | Lookahead rejected | `fsm_test_302_lookahead_rejected` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-303 | Character class compiles | `fsm_test_303_character_class_compiles` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-304 | Alternation precedence | `fsm_test_304_alternation_precedence` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-305 | Bounded repetition | `fsm_test_305_bounded_repetition_compiles` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-306 | Greedy quantifier semantics | `fsm_test_306b_lazy_quantifier_rejected` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-307 | Unicode class rejected | `fsm_test_307_unicode_class_rejected` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-309 | Escaped slash literal | `regex_escaped_slash` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-310 | Empty regex rejected | `rejects_empty_with_e723` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-311 | DFA size limit | `e721_when_dfa_exceeds_limit` <sub>(fsm_regex/mod.rs)</sub> | ✅ |
| FSM-TEST-312 | Anchors | `fsm_test_312_start_anchor` <sub>(codegen/fsm_python.rs)</sub><br>`rust_start_anchor` <sub>(codegen/fsm_rust.rs)</sub> | ✅ |

## Transitions and targets (§3.5.4, §4.4)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-400 | Static transition | `fsm_test_400_static_transitions` <sub>(codegen/fsm_python.rs)</sub><br>`states_and_transitions` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-401 | Stage-address transition target | `fsm_test_401_stage_ref_target` <sub>(codegen/fsm_python.rs)</sub><br>`stage_ref_transition_target` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-402 | Conditional target | `fsm_test_402_conditional_target` <sub>(codegen/fsm_python.rs)</sub><br>`rust_conditional_target` <sub>(codegen/fsm_rust.rs)</sub> | ✅ |
| FSM-TEST-403 | Reference to undeclared state | `e731_undeclared_state` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-404 | Reference to undeclared stage | `e732_undeclared_stage` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-405 | Conditional with no matching condition warns | `w701_conditional_without_failure` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-406 | Missing `when` guard on cond_alt rejected | `conditional_missing_when_errors` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-407 | Constant-true `when` guard warns | `w705_constant_true_when` <sub>(fsm_validator/mod.rs)</sub> | ✅ |

## Runtime semantics (§5)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-500 | Construction with full match | `fsm_test_006_transitions` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-501 | Construction with no match | `fsm_test_002_matched_builtin` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-502 | Cursor advances on match | `fsm_test_007_capture` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-503 | Stage capture exposes matched bytes | `fsm_test_007_capture` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-504 | Input parameter accessible as self.<name> | `fsm_test_005_self_text` <sub>(codegen/fsm_python.rs)</sub> | ✅ |

## Embedding actions (§3.5.5)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-600 | Entry and per-element actions | `embed_start_captures_entry_cursor` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-601 | Final-state action | `embed_accept_fires_on_accepting_states` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-602 | EOF action | `fsm_test_602_eof_action` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-603 | Leave-final action | — | ⏸ **deferred** — `%{}` leave-final firing needs a dedicated DFA-step scenario — deferred |

## Composition (§8)

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-700 | Mode C composition | `fsm_mode_c_call_out` <sub>(pipeline/compiler.rs)</sub><br>`fsm_mode_c_chained` <sub>(pipeline/compiler.rs)</sub> | ✅ |
| FSM-TEST-701 | Mode C bytes-and-return | `fsm_mode_c_call_out` <sub>(pipeline/compiler.rs)</sub> | ✅ |
| FSM-TEST-702 | Mode C type mismatch | — | ⏸ **deferred** — E706 Mode C type mismatch — no @@fsm type system in v0.1 |
| FSM-TEST-703 | Mode C alphabet mismatch | — | ⏸ **deferred** — Mode C alphabet-mismatch E731 not enforced — needs cross-fsm validation |
| FSM-TEST-704 | Mode C dynamic dispatch rejected | — | ⏸ **deferred** — Mode C dynamic-dispatch E732 not enforced — needs static-resolvability check |

## Edge cases

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-1000 | Empty input | `fsm_test_001_minimal` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-1001 | Input exactly matching | `fsm_test_001_minimal` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-1002 | Input longer than match | `fsm_test_007_capture` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-1003 | Input strictly prefix of match | `fsm_test_1003_prefix_of_match_rejected` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-1004 | Anchored match with leading non-match | `fsm_test_312_start_anchor` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-1005 | Zero-length match | `fsm_test_1005_zero_length_match` <sub>(codegen/fsm_python.rs)</sub> | ✅ |
| FSM-TEST-1006 | @@:matched before any stage completes | `fsm_test_1006_matched_before_stage` <sub>(codegen/fsm_python.rs)</sub> | ✅ |

## Additional diagnostic coverage

| ID | Title | Backing test(s) | Status |
|---|---|---|---|
| FSM-TEST-1100 | Malformed declaration (missing fsm name) | `fsm_test_1100_missing_name` <sub>(fsm_parser/mod.rs)</sub><br>`generic_error_is_e700` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-1101 | Malformed declaration (missing body braces) | `fsm_test_1101_missing_body_braces` <sub>(fsm_parser/mod.rs)</sub><br>`generic_error_is_e700` <sub>(fsm_parser/mod.rs)</sub> | ✅ |
| FSM-TEST-1102 | Stage label collision within a state | `e730_duplicate_stage_label` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-1103 | Unused parameter warning | `w702_unused_parameter` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-1104 | Unused domain variable warning | `w703_unused_domain_field` <sub>(fsm_validator/mod.rs)</sub> | ✅ |
| FSM-TEST-1105 | DFA size approaching limit warning | `(module reference)` <sub>(fsm_regex/size_check.rs)</sub> | ✅ |

## Deferrals (tracked in the enforcement allowlist)

These 6 IDs have no backing test by design; each is an explicit, justified entry in the allowlist so it stays visible rather than silently uncovered.

| ID | Reason |
|---|---|
| FSM-TEST-102 | E706 return-type mismatch — no @@fsm type system in v0.1 |
| FSM-TEST-702 | E706 Mode C type mismatch — no @@fsm type system in v0.1 |
| FSM-TEST-033 | bare-name (non-`self.`) E703 validation not implemented — needs context-aware design |
| FSM-TEST-603 | `%{}` leave-final firing needs a dedicated DFA-step scenario — deferred |
| FSM-TEST-703 | Mode C alphabet-mismatch E731 not enforced — needs cross-fsm validation |
| FSM-TEST-704 | Mode C dynamic-dispatch E732 not enforced — needs static-resolvability check |

## Running the tests
```bash
cargo test --lib fsm                        # all @@fsm unit + execution tests (418)
cargo test --test fsm_conformance_coverage  # the coverage guard
cargo test                                  # full suite (1177 tests)
```
