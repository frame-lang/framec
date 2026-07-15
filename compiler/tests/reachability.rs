//! **State-reachability — a graph-walker system, not a byte scanner — runs correctly.**
//!
//! `reachability::reachable` is generated from `reachability.frs`, a plain `@@system` that
//! walks an edge-list graph by iterative relaxation (no byte input; the drive loop is in the
//! wrapper). This proves — by running — the second back-half graph walker after `hsm_cycle`.
//!
//! Edges are two parallel arrays: `from[e] -> to[e]`. `reachable(from, to, n, start)` returns
//! an `n`-long mask of which nodes are reachable from `start`.

use frame_compiler::text::scan::reachability::reachable;

#[test]
fn a_linear_chain_is_fully_reachable() {
    // 0 -> 1 -> 2 -> 3
    let from = [0, 1, 2];
    let to = [1, 2, 3];
    assert_eq!(reachable(&from, &to, 4, 0), vec![true, true, true, true]);
}

#[test]
fn a_node_with_no_incoming_edge_is_unreachable() {
    // 0 -> 1 -> 2 ;  node 3 is orphaned
    let from = [0, 1];
    let to = [1, 2];
    assert_eq!(reachable(&from, &to, 4, 0), vec![true, true, true, false]);
}

#[test]
fn a_cycle_does_not_break_reachability() {
    // 0 -> 1 -> 2 -> 0   (all reachable from 0 despite the back-edge)
    let from = [0, 1, 2];
    let to = [1, 2, 0];
    assert_eq!(reachable(&from, &to, 3, 0), vec![true, true, true]);
}

#[test]
fn branching_reaches_both_arms() {
    // 0 -> 1, 0 -> 2, 2 -> 3 ; node 4 orphaned
    let from = [0, 0, 2];
    let to = [1, 2, 3];
    assert_eq!(
        reachable(&from, &to, 5, 0),
        vec![true, true, true, true, false]
    );
}

#[test]
fn only_the_start_is_reachable_with_no_edges() {
    assert_eq!(reachable(&[], &[], 3, 1), vec![false, true, false]);
}

#[test]
fn reachability_is_directional() {
    // 1 -> 0 only: from start 0 you cannot reach 1 (edge points the other way)
    let from = [1];
    let to = [0];
    assert_eq!(reachable(&from, &to, 2, 0), vec![true, false]);
}

#[test]
fn a_long_chain_needs_many_passes_but_converges() {
    // 0 -> 1 -> ... -> 9, deliberately fed in REVERSE order so a single sweep would only
    // advance the frontier one hop; convergence requires multiple passes.
    let from: Vec<i32> = (0..9).rev().collect();
    let to: Vec<i32> = (1..10).rev().collect();
    assert_eq!(reachable(&from, &to, 10, 0), vec![true; 10]);
}

#[test]
fn out_of_range_edges_are_ignored_not_panics() {
    // Malformed edges (negative, out of range) must not panic — just fail to grow.
    let from = [0, 5, -1];
    let to = [1, 0, 0];
    assert_eq!(reachable(&from, &to, 2, 0), vec![true, true]);
}
