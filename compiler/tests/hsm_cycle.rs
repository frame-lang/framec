//! **The HSM cycle detector — a graph-walker system, not a byte scanner — runs correctly.**
//!
//! `hsm_cycle::has_cycle` is generated from `hsm_cycle.frs`, a plain `@@system` that walks the
//! parent-chain graph (no byte input; the drive loop is in the wrapper). This proves — by
//! running — that dogfooding extends past byte scanners to AST/graph walkers.
//!
//! `parents[i]` is the parent index of state `i`, or negative for a root.

use frame_compiler::text::scan::hsm_cycle::has_cycle;

#[test]
fn acyclic_chains_are_not_cycles() {
    assert!(!has_cycle(&[]), "empty");
    assert!(!has_cycle(&[-1]), "single root");
    assert!(!has_cycle(&[-1, 0, 1]), "$A root, $B=>$A, $C=>$B");
    assert!(!has_cycle(&[-1, 0, 0, 1]), "a tree (two children of the root)");
}

#[test]
fn cycles_are_detected() {
    assert!(has_cycle(&[0]), "self-loop: $A => $A");
    assert!(has_cycle(&[1, 0]), "two-cycle: $A => $B => $A");
    assert!(has_cycle(&[-1, 2, 1]), "cycle not involving the root: $B => $C => $B");
    assert!(has_cycle(&[1, 2, 0]), "three-cycle");
}

#[test]
fn a_root_plus_a_cycle_is_a_cycle() {
    // node 0 is a root; nodes 1<->2 cycle. Any cycle anywhere is caught.
    assert!(has_cycle(&[-1, 2, 1]));
}

// --- The validator integration: a cyclic HSM is caught as E403 in the real pipeline. ---

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::validate::validate;
use frame_compiler::Source;

fn diags(src: &str) -> Vec<String> {
    let source = Source::new("t.frm", src.as_bytes().to_vec()).unwrap();
    let ast = segment(&source, Target::Rust).unwrap();
    let (syms, mut ds) = resolve(&ast);
    ds.extend(validate(&ast, &syms));
    ds.into_iter().map(|d| d.code.to_string()).collect()
}

#[test]
fn a_cyclic_hsm_is_E403() {
    let codes = diags("@@system S {\n    interface: e()\n    machine:\n        $A => $B { }\n        $B => $A { }\n}\n");
    assert!(codes.contains(&"E403".to_string()), "expected E403, got {codes:?}");
}

#[test]
fn an_acyclic_hsm_is_clean() {
    let codes = diags("@@system S {\n    interface: e()\n    machine:\n        $A { e() { } }\n        $B => $A { }\n}\n");
    assert!(!codes.contains(&"E403".to_string()), "no cycle here, got {codes:?}");
}
