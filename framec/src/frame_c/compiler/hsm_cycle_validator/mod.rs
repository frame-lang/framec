//! E413 HSM cycle detector — Frame multi-state graph walker.
//!
//! RFC-0035 round 5. The previous rounds documented Frame's fit
//! across pure utilities (rounds 1-2), classifiers (round 3),
//! and stateful validators (round 4). Round 5 puts Frame on a
//! graph algorithm — the HSM parent-chain cycle check.
//!
//! Each state's parent chain is walked by a fresh FSM instance.
//! The FSM has four states:
//!
//!   $Initial   → $Walking
//!   $Walking   → $CycleFound | $ChainRoot | (self-loop)
//!   $CycleFound (terminal)
//!   $ChainRoot  (terminal)
//!
//! Per-call shape: `step(parent: String) → String` returns one
//! of `"WALKING"`, `"CYCLE|<parent>"`, or `"ROOT"`. Visited set
//! threads through `domain.visited` (a comma-separated string —
//! Frame interface types don't yet include `HashSet`, but a
//! string CSV is a clean enough stand-in for chain-walk scale
//! where chains are typically depth ≤10).
//!
//! Public API:
//!   `validate_hsm_cycles(parents: &[(String, Option<String>)])
//!        -> Vec<(String, String)>`
//!
//! Returns `(state_name, cycle_at)` pairs for each detected
//! cycle. The validator caller (`frame_validator/machine.rs`)
//! wraps each pair in a `ValidationError` with code E413.
//!
//! To regenerate after editing the `.frs` source:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/hsm_cycle_validator/hsm_cycle_walker.frs \
//!     > framec/src/frame_c/compiler/hsm_cycle_validator/hsm_cycle_walker.gen.rs

use std::collections::HashMap;

mod hsm_cycle_walker_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    include!("hsm_cycle_walker.gen.rs");
}

/// For each state with a non-empty parent, walk its parent
/// chain looking for a revisit. Returns `(state_name, cycle_at)`
/// pairs.
///
/// `parents`: a slice of `(state_name, parent_name_or_none)`
/// pairs. The order is preserved in the output so error
/// diagnostics match the source order.
pub(crate) fn validate_hsm_cycles(parents: &[(String, Option<String>)]) -> Vec<(String, String)> {
    // Map for O(1) parent lookups during chain walking.
    let parent_map: HashMap<&str, Option<&str>> = parents
        .iter()
        .map(|(s, p)| (s.as_str(), p.as_deref()))
        .collect();

    let mut cycles = Vec::new();
    for (state_name, parent) in parents {
        let Some(first_parent) = parent.as_deref() else {
            continue;
        };
        // Fresh FSM per chain walk — visited set is per-start.
        let mut walker = hsm_cycle_walker_fsm::HsmCycleWalker::__create();
        // Seed with the starting node so the visited set
        // includes the state we are walking the chain FOR.
        let _ = walker.step(state_name.to_string());
        // Walk the parent chain.
        let mut current = Some(first_parent);
        loop {
            let parent_str = current.unwrap_or("").to_string();
            let result = walker.step(parent_str);
            if let Some(at) = result.strip_prefix("CYCLE|") {
                cycles.push((state_name.clone(), at.to_string()));
                break;
            }
            if result == "ROOT" {
                break;
            }
            // result == "WALKING" → follow the chain one step further.
            let Some(now) = current else {
                break;
            };
            current = parent_map.get(now).and_then(|p| *p);
        }
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(pairs: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        pairs
            .iter()
            .map(|(s, p)| (s.to_string(), p.map(|x| x.to_string())))
            .collect()
    }

    #[test]
    fn no_cycle_simple_chain() {
        // A → B → C → (root)
        let map = make(&[("A", Some("B")), ("B", Some("C")), ("C", None)]);
        assert_eq!(validate_hsm_cycles(&map), Vec::new());
    }

    #[test]
    fn no_cycle_orphan() {
        let map = make(&[("A", None)]);
        assert_eq!(validate_hsm_cycles(&map), Vec::new());
    }

    #[test]
    fn detects_self_cycle() {
        // A → A (state declares itself as parent)
        let map = make(&[("A", Some("A"))]);
        let cycles = validate_hsm_cycles(&map);
        assert_eq!(cycles, vec![("A".to_string(), "A".to_string())]);
    }

    #[test]
    fn detects_two_node_cycle() {
        // A → B → A
        let map = make(&[("A", Some("B")), ("B", Some("A"))]);
        let cycles = validate_hsm_cycles(&map);
        // Each state in the cycle reports it from its own start.
        assert_eq!(cycles.len(), 2);
        assert!(cycles.contains(&("A".to_string(), "A".to_string())));
        assert!(cycles.contains(&("B".to_string(), "B".to_string())));
    }

    #[test]
    fn detects_three_node_cycle() {
        // A → B → C → A
        let map = make(&[("A", Some("B")), ("B", Some("C")), ("C", Some("A"))]);
        let cycles = validate_hsm_cycles(&map);
        assert_eq!(cycles.len(), 3);
    }

    #[test]
    fn cycle_in_subgraph_only_affects_participants() {
        // Root chain D→(root) is clean. A→B→A is a cycle.
        let map = make(&[("A", Some("B")), ("B", Some("A")), ("D", None)]);
        let cycles = validate_hsm_cycles(&map);
        // Only A and B should report cycles. D is clean.
        assert_eq!(cycles.len(), 2);
        assert!(cycles.iter().all(|(s, _)| s != "D"));
    }
}
