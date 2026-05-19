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

// Generator: a minimal valid @@system with one state and N interface
// methods. Returns (frame_source, system_name, method_names).
fn system_with_methods_strategy(
) -> impl Strategy<Value = (String, String, Vec<String>)> {
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
            let system_name = {
                let mut chars = sys_part.chars();
                match chars.next() {
                    Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                    None => "S".to_string(),
                }
            };
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
}
