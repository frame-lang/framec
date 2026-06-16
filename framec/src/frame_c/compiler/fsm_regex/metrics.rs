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

use super::subset::{Dfa, DfaStateId};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
pub fn collect(dfa: &Dfa) -> DfaMetrics {
    let state_count = dfa.states.len();

    let max_fanout = dfa
        .states
        .iter()
        .map(|s| s.transitions.len())
        .max()
        .unwrap_or(0);

    let targets: BTreeSet<DfaStateId> = dfa
        .states
        .iter()
        .flat_map(|s| s.transitions.iter().map(|t| t.to))
        .collect();

    DfaMetrics {
        state_count,
        max_fanout,
        transition_target_set: targets.into_iter().collect(),
        worst_case_input_position: longest_acyclic_path(dfa),
    }
}

/// Longest simple (no repeated state) path from the start state, in
/// transitions. Bounds the input positions a non-looping recognition
/// visits; a DFA with a reachable cycle has an unbounded true worst case,
/// for which this reports the acyclic bound (≤ state_count − 1).
fn longest_acyclic_path(dfa: &Dfa) -> usize {
    if dfa.states.is_empty() {
        return 0;
    }
    let mut on_path = vec![false; dfa.states.len()];
    fn dfs(dfa: &Dfa, s: DfaStateId, on_path: &mut [bool]) -> usize {
        on_path[s] = true;
        let mut best = 0;
        for t in &dfa.states[s].transitions {
            if !on_path[t.to] {
                best = best.max(1 + dfs(dfa, t.to, on_path));
            }
        }
        on_path[s] = false;
        best
    }
    dfs(dfa, dfa.start, &mut on_path)
}

/// Write metrics to a JSON sidecar next to an emitted output file.
///
/// Writes to `path` with `.metrics.json` appended. JSON is hand-formatted
/// (the four fields are simple scalars + an int array), keeping the
/// engine free of a serde dependency.
pub fn write_sidecar_json(metrics: &DfaMetrics, path: &Path) -> std::io::Result<()> {
    std::fs::write(sidecar_path(path), render_json(metrics))
}

/// The sidecar path for a codegen output path: `<path>.metrics.json`.
fn sidecar_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".metrics.json");
    PathBuf::from(s)
}

/// Render the metrics as JSON (schema in the module doc).
fn render_json(m: &DfaMetrics) -> String {
    let targets = m
        .transition_target_set
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{{\n  \"state_count\": {},\n  \"max_fanout\": {},\n  \"transition_target_set\": [{}],\n  \"worst_case_input_position\": {}\n}}\n",
        m.state_count, m.max_fanout, targets, m.worst_case_input_position
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_regex::{parser, subset, thompson, Alphabet};

    fn metrics(src: &str) -> DfaMetrics {
        let ast = parser::parse(src, Alphabet::Bytes).expect("parse");
        let nfa = thompson::build(&ast, Alphabet::Bytes);
        let dfa = super::super::hopcroft::minimize(&subset::construct(&nfa));
        collect(&dfa)
    }

    #[test]
    fn literal_path_metrics() {
        // `abc` → 4 states, fan-out 1, acyclic path length 3.
        let m = metrics("abc");
        assert_eq!(m.state_count, 4);
        assert_eq!(m.max_fanout, 1);
        assert_eq!(m.worst_case_input_position, 3);
    }

    #[test]
    fn star_is_single_looping_state() {
        let m = metrics("a*");
        assert_eq!(m.state_count, 1);
        // The acyclic path through a self-loop-only state is 0 transitions.
        assert_eq!(m.worst_case_input_position, 0);
    }

    #[test]
    fn json_round_trips_fields() {
        let m = DfaMetrics {
            state_count: 3,
            max_fanout: 2,
            transition_target_set: vec![0, 1, 2],
            worst_case_input_position: 5,
        };
        let json = render_json(&m);
        assert!(json.contains("\"state_count\": 3"));
        assert!(json.contains("\"max_fanout\": 2"));
        assert!(json.contains("\"transition_target_set\": [0, 1, 2]"));
        assert!(json.contains("\"worst_case_input_position\": 5"));
    }

    #[test]
    fn sidecar_path_appends_suffix() {
        let p = sidecar_path(Path::new("/tmp/out.py"));
        assert_eq!(p, PathBuf::from("/tmp/out.py.metrics.json"));
    }
}
