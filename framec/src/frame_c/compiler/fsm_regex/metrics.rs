//! DFA metrics — RFC-0042 §9.3 reported properties.
//!
//! Collects compile-time facts about a minimized DFA:
//!
//! - State count (post-minimization).
//! - Max transition fan-out per state.
//! - Statically-enumerated transition target set.
//! - Worst-case input position per element (bound by max quantifier
//!   repetition × stage count).
//!
//! Metrics are surfaced two ways:
//!
//! 1. Rendered to stderr at `-v` verbosity (one line per fsm).
//! 2. Written to a `<fsm>.metrics.json` sidecar next to the emitted
//!    output. Diagnostic tests assert on the JSON, not the rendered
//!    stderr text — see RFC-0042 execution plan §5 testing strategy.

use super::subset::Dfa;
use std::path::Path;

/// Collected properties of a compiled fsm's DFA.
#[derive(Debug, Clone)]
pub struct DfaMetrics {
    /// Post-minimization state count.
    pub state_count: usize,

    /// Maximum number of outgoing transitions on any single DFA state.
    /// Used by codegen to choose between table (dense) and switch
    /// (sparse) dispatch.
    pub max_fanout: usize,

    /// All DFA state IDs reachable as a transition target from anywhere
    /// in the DFA. For codegen and CFI reasoning.
    pub transition_target_set: Vec<super::subset::DfaStateId>,

    /// Upper bound on input positions visited per recognition call.
    /// Computed as sum of stage worst-case consumption.
    pub worst_case_input_position: usize,
}

/// Collect metrics from a minimized DFA.
pub fn collect(_dfa: &Dfa) -> DfaMetrics {
    todo!("Phase 4: traverse DFA; populate fields")
}

/// Write metrics to a JSON sidecar next to an emitted output file.
///
/// `path` should be the codegen output path; this function writes to
/// `path` with `.metrics.json` appended.
pub fn write_sidecar_json(_metrics: &DfaMetrics, _path: &Path) -> std::io::Result<()> {
    // Schema (JSON):
    // {
    //   "state_count": <usize>,
    //   "max_fanout": <usize>,
    //   "transition_target_set": [<usize>, ...],
    //   "worst_case_input_position": <usize>
    // }
    todo!("Phase 4: serialize via serde_json; write to <path>.metrics.json")
}
