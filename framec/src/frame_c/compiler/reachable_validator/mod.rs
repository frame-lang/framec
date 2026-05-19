//! W414 reachable-states validator — Frame multi-state BFS.
//!
//! RFC-0035 round 6. Round 5 applied Frame to a graph algorithm
//! where each chain walk was an independent FSM. Round 6 takes
//! the next step: ONE FSM instance drives an entire BFS over
//! the state-machine graph, threading visited+queue through
//! domain fields that evolve across many events.
//!
//! Three states with real transitions:
//!
//!   $Initial → $Walking on seed(start)
//!   $Walking → $Done when next() finds an empty queue
//!   $Done    terminal (next() returns "DONE" idempotently;
//!            unreachable() works in either $Walking or $Done)
//!
//! Public API:
//!   `validate_reachable_states(start, edges, all_states)
//!        -> Vec<String>` (the unreachable state names)
//!
//! `edges` maps each state name to the list of states it can
//! reach in one step (transition targets from handler/enter/
//! exit bodies + HSM parent ancestors). `pop$` transitions are
//! pre-filtered by the caller (the pop destination is dynamic
//! — wherever the runtime stack last held — so it cannot be
//! statically resolved here).
//!
//! Frame ergonomic observation continued from Round 5: HashSet
//! / Vec interface types are still missing, so visited+queue
//! are encoded as comma-separated strings. For HSM machines
//! (typically ≤50 states) the O(n) lookup on visited and the
//! O(n) split-on-comma operations are negligible. The
//! first-class Set / Map proposal noted in Round 5's RFC entry
//! would clean this up.
//!
//! To regenerate after editing the `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/reachable_validator/reachable_walker.frs \
//!     > framec/src/frame_c/compiler/reachable_validator/reachable_walker.gen.rs

use std::collections::HashMap;

mod reachable_walker_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("reachable_walker.gen.rs");
}

/// Walk the state graph in BFS order starting from `start`,
/// using `edges` to expand each visited node. Return the names
/// of states in `all_states` that the walk never reached.
///
/// `edges`: `state_name → list of one-step-reachable state names`.
/// Caller is responsible for including HSM parent ancestors in
/// the edge list and for filtering out `pop$` targets.
///
/// `all_states`: the full set of state names in the machine —
/// the diff against `visited` produces the W414 unreachable
/// list.
pub(crate) fn validate_reachable_states(
    start: &str,
    edges: &HashMap<String, Vec<String>>,
    all_states: &[String],
) -> Vec<String> {
    if start.is_empty() {
        return Vec::new();
    }
    let mut walker = reachable_walker_fsm::ReachableWalker::__create();
    let _ = walker.seed(start.to_string());
    loop {
        let head = walker.next();
        if head == "DONE" {
            break;
        }
        if let Some(neighbors) = edges.get(&head) {
            for n in neighbors {
                let _ = walker.enqueue(n.clone());
            }
        }
    }
    let all_csv = all_states.join(",");
    let result = walker.unreachable(all_csv);
    if result.is_empty() {
        Vec::new()
    } else {
        result.split(',').map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(pairs: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(s, ns)| (s.to_string(), ns.iter().map(|n| n.to_string()).collect()))
            .collect()
    }

    fn all(states: &[&str]) -> Vec<String> {
        states.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn single_start_no_edges() {
        let e = edges(&[("A", &[])]);
        let result = validate_reachable_states("A", &e, &all(&["A"]));
        assert!(result.is_empty(), "A should be reachable: {:?}", result);
    }

    #[test]
    fn linear_chain_all_reachable() {
        // A → B → C
        let e = edges(&[("A", &["B"]), ("B", &["C"]), ("C", &[])]);
        let result = validate_reachable_states("A", &e, &all(&["A", "B", "C"]));
        assert!(result.is_empty(), "all should be reachable: {:?}", result);
    }

    #[test]
    fn isolated_state_unreachable() {
        // A → B; C is orphaned
        let e = edges(&[("A", &["B"]), ("B", &[]), ("C", &[])]);
        let result = validate_reachable_states("A", &e, &all(&["A", "B", "C"]));
        assert_eq!(result, vec!["C".to_string()]);
    }

    #[test]
    fn diamond_no_dup_visit() {
        // A → B, A → C, B → D, C → D
        // BFS must not double-visit D.
        let e = edges(&[("A", &["B", "C"]), ("B", &["D"]), ("C", &["D"]), ("D", &[])]);
        let result = validate_reachable_states("A", &e, &all(&["A", "B", "C", "D"]));
        assert!(result.is_empty(), "diamond walk: {:?}", result);
    }

    #[test]
    fn cycle_terminates() {
        // A → B → C → A — must not loop forever.
        let e = edges(&[("A", &["B"]), ("B", &["C"]), ("C", &["A"])]);
        let result = validate_reachable_states("A", &e, &all(&["A", "B", "C"]));
        assert!(result.is_empty(), "cycle walk: {:?}", result);
    }

    #[test]
    fn empty_start_returns_empty() {
        let e = edges(&[("A", &[])]);
        let result = validate_reachable_states("", &e, &all(&["A"]));
        assert!(result.is_empty());
    }
}
