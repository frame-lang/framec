//! Property-based tests for framec invariants (roadmap #440).
//!
//! Complements the RFC-0027 snapshot tests (regression nets) and
//! the external fuzz harness (cross-backend differential). Where
//! snapshots tell you the output didn't change and fuzz tells you
//! all backends agree, property tests tell you the output
//! satisfies a stated spec-level invariant.
//!
//! ## What goes here
//!
//! Each property is a `proptest!` block asserting "for any valid
//! X in some domain, the output satisfies Y." Examples that should
//! eventually live in this file:
//!
//! - For any valid `@@system { interface: m(); ... }`, the
//!   generated code declares a method named `m`.
//! - For any `$.foo: T = X` state-var declaration, the generated
//!   $> handler initializes the state-var to X.
//! - For any single-state machine with N interface methods, the
//!   generated code's method count == N + (lifecycle/runtime
//!   helpers).
//! - For any persist-tagged system, the generated save and load
//!   methods round-trip identical state.
//! - For any state with no transitions, the generated `__router`
//!   never sets `__next_compartment`.
//!
//! ## Scaffolding only
//!
//! This file is the v1 skeleton — one property each on the four
//! "shape" invariants (interface methods declared, system class
//! emitted, state-var initialized, persist save/load symmetry).
//! Future commits extend the corpus. The point of starting small
//! is to validate the harness shape; the corpus grows
//! incrementally as bugs surface or new invariants are codified.
//!
//! ## Strategy
//!
//! `proptest` generators (`Strategy` impls) produce synthetic
//! Frame source within a constrained shape — small state machines
//! with random valid names and method signatures. The generator
//! lives in `proptest_gen.rs` (also in `tests/`) once the corpus
//! gets bigger; for now, generators are inline in this file.
//!
//! ## Re-bless?
//!
//! No re-bless workflow. Properties are stated invariants — if
//! a property fails, the code is wrong, not the test. `proptest`
//! shrinks the failing case to the minimal counter-example, which
//! becomes a regression test naturally.

mod common;

use common::compile_fixture;
use framec::frame_c::compiler::compile_module;
use framec::frame_c::compiler::TargetLanguage;
use proptest::prelude::*;
use std::convert::TryFrom;

// Generator: a valid Frame identifier (lowercase + alphanumeric +
// underscore, starting with a letter; bounded length for tractability).
fn ident_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,7}".prop_map(|s| s.to_string())
}

// Generator: a valid state name (capitalized like Frame conventions).
fn state_name_strategy() -> impl Strategy<Value = String> {
    "[A-Z][a-zA-Z0-9]{0,7}".prop_map(|s| s.to_string())
}

// Capitalize an identifier's first character (Frame system names
// must start uppercase; identifiers are lowercase).
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => "S".to_string(),
    }
}

// Generator: a system with one state, N interface methods, and M
// domain state-vars with int initializers. Returns (source,
// system_name, methods, state_vars_with_init).
fn system_with_state_vars_strategy() -> impl Strategy<Value = (String, String, Vec<(String, i32)>)>
{
    (
        ident_strategy(),
        state_name_strategy(),
        prop::collection::vec((ident_strategy(), -1000i32..1000), 1..4),
    )
        .prop_map(|(sys_part, state, vars)| {
            let mut seen = std::collections::HashSet::new();
            let vars: Vec<(String, i32)> = vars
                .into_iter()
                .filter(|(n, _)| seen.insert(n.clone()))
                .collect();
            let system_name = capitalize_first(&sys_part);
            let domain_block = vars
                .iter()
                .map(|(n, v)| format!("        {}: i32 = {}", n, v))
                .collect::<Vec<_>>()
                .join("\n");
            let src = format!(
                "@@system {sys} {{\n    machine:\n        ${state} {{\n        }}\n\n    domain:\n{dom}\n}}\n",
                sys = system_name,
                state = state,
                dom = domain_block,
            );
            (src, system_name, vars)
        })
}

// Generator: a system tagged with the full RFC-0012 amendment
// persist contract (`@@[persist(blob)]` + `@@[save(name)]` +
// `@@[load(name)]`). Bare `@@[persist]` was rejected post-RFC-0012,
// so the generator must produce the system-level form to be valid
// Frame v4. Returns (source, system_name).
fn persist_system_strategy() -> impl Strategy<Value = (String, String)> {
    (ident_strategy(), state_name_strategy(), -1000i32..1000)
        .prop_map(|(sys_part, state, init)| {
            let system_name = capitalize_first(&sys_part);
            let src = format!(
                "@@[persist(String)]\n@@[save(snapshot)]\n@@[load(restore)]\n@@system {sys} {{\n    machine:\n        ${state} {{\n        }}\n\n    domain:\n        n: i32 = {init}\n}}\n",
                sys = system_name,
                state = state,
                init = init,
            );
            (src, system_name)
        })
}

// Generator: a minimal valid @@system with one state and N interface
// methods. Returns (frame_source, system_name, method_names).
fn system_with_methods_strategy() -> impl Strategy<Value = (String, String, Vec<String>)> {
    (
        ident_strategy(),
        state_name_strategy(),
        prop::collection::vec(ident_strategy(), 1..6),
    )
        .prop_map(|(sys_part, state, methods)| {
            // Filter dupes (Frame rejects duplicate interface methods at
            // E117 / similar — keep generated source semantically valid).
            let mut seen = std::collections::HashSet::new();
            let methods: Vec<String> = methods
                .into_iter()
                .filter(|m| seen.insert(m.clone()))
                .collect();
            let system_name = capitalize_first(&sys_part);
            let interface_block = methods
                .iter()
                .map(|m| format!("        {}()", m))
                .collect::<Vec<_>>()
                .join("\n");
            let handler_block = methods
                .iter()
                .map(|m| format!("            {}() {{ }}", m))
                .collect::<Vec<_>>()
                .join("\n");
            let src = format!(
                "@@system {sys} {{\n    interface:\n{iface}\n\n    machine:\n        ${state} {{\n{hdl}\n        }}\n}}\n",
                sys = system_name,
                iface = interface_block,
                hdl = handler_block,
                state = state,
            );
            (src, system_name, methods)
        })
}

proptest! {
    // Property: for any valid single-state @@system with N void
    // interface methods, the generated Python code contains a `def
    // <method>(` declaration for each method, and the system class
    // is named after the system.
    #[test]
    fn python_emits_class_and_methods(
        (source, sys_name, methods) in system_with_methods_strategy()
    ) {
        let lang = TargetLanguage::try_from("python_3").unwrap();
        let output = match compile_module(&source, lang) {
            Ok(s) => s,
            Err(e) => {
                // Generator should only produce valid Frame; a
                // compile error means the generator has a bug, not
                // a property violation. Fail loud.
                panic!("compile failed for generated source: {}\n\n--- source ---\n{}", e.error, source);
            }
        };
        // Sanity: the system class is named after the system.
        prop_assert!(
            output.contains(&format!("class {}", sys_name)),
            "expected 'class {}' in output, didn't find it.\n--- source ---\n{}\n--- output ---\n{}",
            sys_name, source, output
        );
        // Every interface method appears as a Python def in output.
        for method in &methods {
            prop_assert!(
                output.contains(&format!("def {}(", method)),
                "expected 'def {}(' in output, didn't find it.\n--- source ---\n{}\n--- output (excerpt) ---\n{}",
                method, source, &output[..output.len().min(800)]
            );
        }
    }

    // Property: the 3-fixture canonical corpus from RFC-0027
    // compiles successfully on every target. Effectively a
    // smoke-cardinality assertion over the existing fixtures —
    // duplicates what snapshot tests do BUT without the
    // "snapshots match" semantics. Useful as a sentinel: if
    // proptest is broken, this test fails loud and early.
    #[test]
    fn canonical_fixtures_compile_on_python(
        fixture_name in prop::sample::select(vec!["01_linear_fsm", "02_hsm", "03_persist"])
    ) {
        let output = compile_fixture(fixture_name, "python_3");
        prop_assert!(
            !output.is_empty(),
            "compile_fixture returned empty output for '{}'", fixture_name
        );
        prop_assert!(
            output.contains("class "),
            "expected 'class ' in Python output for '{}'", fixture_name
        );
    }

    // Property: state-var initializers reach the generated code.
    // For any `var foo: int = N`, the Python output contains the
    // literal `N` — spot-checks that domain initializers aren't
    // silently dropped on the way to codegen.
    #[test]
    fn state_var_initializers_reach_python_output(
        (source, _sys, vars) in system_with_state_vars_strategy()
    ) {
        let lang = TargetLanguage::try_from("python_3").unwrap();
        let output = match compile_module(&source, lang) {
            Ok(s) => s,
            Err(e) => panic!("compile failed: {}\n--- src ---\n{}", e.error, source),
        };
        for (name, value) in &vars {
            // Initializer value appears as a literal somewhere.
            prop_assert!(
                output.contains(&value.to_string()),
                "expected initializer literal '{}' for var '{}' in output\n--- src ---\n{}",
                value, name, source
            );
        }
    }

    // Property: persist systems emit the user-named save/load
    // methods. Per RFC-0015, `@@[save(snapshot)]` /
    // `@@[load(restore)]` mean the generated class exposes
    // `snapshot()` (save) and `restore(data)` (load) — NOT
    // framec-chosen names. Persist is a contract: if framec
    // accepts the tag, the generated module MUST expose the
    // round-trip API under the names the user picked.
    #[test]
    fn persist_tag_emits_save_and_load(
        (source, _sys) in persist_system_strategy()
    ) {
        let lang = TargetLanguage::try_from("python_3").unwrap();
        let output = match compile_module(&source, lang) {
            Ok(s) => s,
            Err(e) => panic!("compile failed: {}\n--- src ---\n{}", e.error, source),
        };
        // The generator hardcodes `snapshot` + `restore` as the
        // save/load names; assert both appear.
        prop_assert!(
            output.contains("def snapshot(") || output.contains("snapshot("),
            "persist contract violated — no 'snapshot' callable:\n--- src ---\n{}\n--- out ---\n{}",
            source, &output[..output.len().min(800)]
        );
        prop_assert!(
            output.contains("def restore(") || output.contains("restore("),
            "persist contract violated — no 'restore' callable:\n--- src ---\n{}\n--- out ---\n{}",
            source, &output[..output.len().min(800)]
        );
    }

    // Property: every interface method becomes a callable in
    // EVERY backend, not just Python. Cross-backend agreement
    // is framec's central claim — if a method gets lost on the
    // way to Rust, Java, or Go, the differential-fuzz harness
    // catches it only when the methods actually fire. This
    // property catches the regression at the symbol level.
    #[test]
    fn methods_appear_across_canonical_backends(
        (source, _sys, methods) in system_with_methods_strategy()
    ) {
        // 4-backend canonical cut, mirrors the fuzz-smoke gate.
        for lang_str in &["python_3", "rust", "java", "go"] {
            let lang = TargetLanguage::try_from(*lang_str).unwrap();
            let output = match compile_module(&source, lang) {
                Ok(s) => s,
                Err(e) => panic!(
                    "{} compile failed: {}\n--- src ---\n{}",
                    lang_str, e.error, source
                ),
            };
            for method in &methods {
                prop_assert!(
                    // Method symbol appears (loosened — different
                    // backends decorate it differently: `def m(`,
                    // `fn m(`, `m()`, `func m(`. Common substring
                    // is just the bare name.).
                    output.contains(method.as_str()),
                    "method '{}' missing from {} output\n--- src ---\n{}",
                    method, lang_str, source
                );
            }
        }
    }

    // Property: every state declared in the @@system appears as
    // a discoverable symbol in the generated output. Frame's
    // central correctness claim is that the user's state machine
    // structure survives to the target — if a state silently
    // disappears (renamed, optimized out, etc.), the resulting
    // module is broken even if it compiles. The state name (or
    // its target-language-adjusted form) must be findable.
    #[test]
    fn state_names_survive_to_output(
        (source, _sys, _methods) in system_with_methods_strategy()
    ) {
        // The generator embeds a state name in the source. Extract
        // it back out (it's the token after `$` in `machine:` —
        // a generated single-state machine has exactly one).
        let state_name = source
            .lines()
            .find_map(|l| l.trim().strip_prefix('$').and_then(|s| s.split_whitespace().next()))
            .expect("generator emitted no $StateName");
        let lang = TargetLanguage::try_from("python_3").unwrap();
        let output = compile_module(&source, lang)
            .unwrap_or_else(|e| panic!("compile failed: {}", e.error));
        // Python lowercases state names per `to_snake_case`. Check
        // both forms — the original capitalized name AND the
        // lowercased form — so the assertion catches "state name
        // disappeared entirely" rather than locking in the
        // particular convention.
        let lower = state_name.to_ascii_lowercase();
        prop_assert!(
            output.contains(state_name) || output.contains(&lower),
            "state name '{}' (or '{}') missing from output\n--- src ---\n{}",
            state_name, lower, source
        );
    }

    // Property: framec is deterministic — compiling the same
    // source twice produces byte-identical output. Non-determinism
    // would break the snapshot tests (RFC-0027), reproducible
    // builds, and any downstream caching strategy.
    #[test]
    fn compilation_is_deterministic(
        (source, _sys, _methods) in system_with_methods_strategy()
    ) {
        let lang = TargetLanguage::try_from("python_3").unwrap();
        let first = compile_module(&source, lang).expect("first compile");
        let second = compile_module(&source, lang).expect("second compile");
        prop_assert_eq!(
            first, second,
            "compilation produced different output on second run\n--- src ---\n{}",
            source
        );
    }

    // Property: @@system block name appears unchanged in output.
    // Catches symbol-mangling regressions — the user-facing system
    // name is part of the public API surface (instantiation,
    // factory dispatch). Renaming silently breaks every caller.
    #[test]
    fn system_name_survives_compilation_intact(
        (source, sys_name, _methods) in system_with_methods_strategy()
    ) {
        for lang_str in &["python_3", "rust", "java", "kotlin", "swift"] {
            let lang = TargetLanguage::try_from(*lang_str).unwrap();
            let output = compile_module(&source, lang)
                .unwrap_or_else(|e| panic!("{} compile failed: {}", lang_str, e.error));
            prop_assert!(
                output.contains(&sys_name),
                "system name '{}' missing from {} output\n--- src ---\n{}",
                sys_name, lang_str, source
            );
        }
    }
}
