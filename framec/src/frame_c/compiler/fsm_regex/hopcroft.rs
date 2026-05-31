//! Hopcroft DFA minimization.
//!
//! Partition-refinement algorithm. Splits the state set into
//! distinguishable equivalence classes; the resulting DFA has the
//! minimum number of states that recognizes the same language. RFC-0042
//! §6.9 step 4.
//!
//! Implementation notes for Phase 4:
//!
//! - Initial partition: `{accept_states, non_accept_states}`.
//! - Refinement worklist: a set of "splitter" partitions; for each
//!   splitter S and input symbol c, split any partition P into
//!   (P_to_S_on_c, P_not_to_S_on_c) when those are non-empty distinct
//!   subsets.
//! - Worst-case O(n log n) for n states; in practice closer to linear
//!   for the small DFAs we'll see from typical `@@fsm` patterns.

use super::subset::Dfa;

/// Minimize a DFA. Returns a new DFA recognizing the same language with
/// the minimum possible state count.
///
/// The returned DFA's state IDs are not the same as the input's; if
/// callers need to map old → new, they should track the mapping at
/// construction time (Phase 4 will expose this if needed for codegen
/// metrics).
pub fn minimize(_dfa: &Dfa) -> Dfa {
    todo!("Phase 4: Hopcroft partition refinement")
}
