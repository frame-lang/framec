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
