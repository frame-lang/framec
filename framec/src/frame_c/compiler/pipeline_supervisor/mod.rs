//! Pipeline supervisor — Frame FSM that observes the framec
//! compile pipeline.
//!
//! RFC-0035 round 7. Frame at the META level: this is the
//! framec pipeline expressed as a state machine. The
//! orchestrator in `pipeline/compiler.rs` calls the supervisor
//! at each phase boundary; the supervisor tracks the phase log
//! and error counts. Wrong supervisor logic produces wrong
//! `--debug` output but cannot change compilation correctness.
//!
//! Bonus deliverable: `framec compile -l graphviz
//! pipeline_supervisor.frs > docs/pipeline.svg` renders the
//! compiler's own pipeline diagram. The `.frs` source is the
//! pipeline spec; the SVG is the documentation. Self-describing
//! meta-compiler.
//!
//! Public API:
//!   `PipelineSupervisor::new()` — fresh FSM in $Idle.
//!   `.begin_phase(name)`        — transition to $Running on first call.
//!   `.complete_phase()`         — close current phase.
//!   `.record_nonfatal(code, msg)` — collected error; pipeline continues.
//!   `.abort(code, msg)`         — fatal; → $Aborted.
//!   `.finish()`                 — → $Done (clean) or $Failed (errors).
//!   `.summary()` → `Summary`    — typed snapshot for debug output.
//!
//! To regenerate after editing the `.frs`:
//!   ./target/release/framec compile -l rust \
//!     framec/src/frame_c/compiler/pipeline_supervisor/pipeline_supervisor.frs \
//!     > framec/src/frame_c/compiler/pipeline_supervisor/pipeline_supervisor.gen.rs

mod pipeline_supervisor_fsm {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    #![allow(unused_parens)]
    include!("pipeline_supervisor.gen.rs");
}

/// Public wrapper around the generated FSM. Exposes a typed
/// API so call sites don't deal with the pipe-delimited tagged
/// strings the FSM uses internally.
pub struct PipelineSupervisor {
    fsm: pipeline_supervisor_fsm::PipelineSupervisor,
}

/// Typed snapshot of the supervisor's state, returned by
/// `summary()`. Parsed from the FSM's tagged-string response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub state: SupervisorState,
    pub phases: Vec<String>,
    pub current_phase: Option<String>,
    pub error_count: i32,
    pub abort_code: Option<String>,
    pub abort_msg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    Idle,
    Running,
    Aborted,
    Failed,
    Done,
}

impl Default for PipelineSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineSupervisor {
    pub fn new() -> Self {
        Self {
            fsm: pipeline_supervisor_fsm::PipelineSupervisor::__create(),
        }
    }

    pub fn begin_phase(&mut self, name: &str) {
        let _ = self.fsm.begin_phase(name.to_string());
    }

    pub fn complete_phase(&mut self) {
        let _ = self.fsm.complete_phase();
    }

    pub fn record_nonfatal(&mut self, code: &str, msg: &str) {
        let _ = self.fsm.record_nonfatal(code.to_string(), msg.to_string());
    }

    pub fn abort(&mut self, code: &str, msg: &str) {
        let _ = self.fsm.abort(code.to_string(), msg.to_string());
    }

    pub fn finish(&mut self) {
        let _ = self.fsm.finish();
    }

    pub fn summary(&mut self) -> Summary {
        let encoded = self.fsm.summary();
        parse_summary(&encoded)
    }
}

fn parse_summary(encoded: &str) -> Summary {
    let (tag, rest) = match encoded.find('|') {
        Some(i) => (&encoded[..i], &encoded[i + 1..]),
        None => (encoded, ""),
    };
    let state = match tag {
        "IDLE" => SupervisorState::Idle,
        "RUNNING" => SupervisorState::Running,
        "ABORTED" => SupervisorState::Aborted,
        "FAILED" => SupervisorState::Failed,
        "DONE" => SupervisorState::Done,
        _ => SupervisorState::Idle,
    };
    let mut phases: Vec<String> = Vec::new();
    let mut current_phase: Option<String> = None;
    let mut error_count: i32 = 0;
    let mut abort_code: Option<String> = None;
    let mut abort_msg: Option<String> = None;
    for field in rest.split('|') {
        if let Some(rest) = field.strip_prefix("phases=") {
            if !rest.is_empty() {
                phases = rest.split(',').map(|s| s.to_string()).collect();
            }
        } else if let Some(rest) = field.strip_prefix("current=") {
            if !rest.is_empty() {
                current_phase = Some(rest.to_string());
            }
        } else if let Some(rest) = field.strip_prefix("errors=") {
            error_count = rest.parse().unwrap_or(0);
        } else if let Some(rest) = field.strip_prefix("code=") {
            abort_code = Some(rest.to_string());
        } else if let Some(rest) = field.strip_prefix("msg=") {
            abort_msg = Some(rest.to_string());
        }
    }
    Summary {
        state,
        phases,
        current_phase,
        error_count,
        abort_code,
        abort_msg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_done() {
        let mut sup = PipelineSupervisor::new();
        assert_eq!(sup.summary().state, SupervisorState::Idle);
        sup.begin_phase("segment");
        sup.complete_phase();
        sup.begin_phase("parse");
        sup.complete_phase();
        sup.begin_phase("validate");
        sup.complete_phase();
        sup.begin_phase("codegen");
        sup.complete_phase();
        sup.begin_phase("assemble");
        sup.complete_phase();
        sup.finish();
        let s = sup.summary();
        assert_eq!(s.state, SupervisorState::Done);
        assert_eq!(
            s.phases,
            vec!["segment", "parse", "validate", "codegen", "assemble"]
        );
        assert_eq!(s.error_count, 0);
    }

    #[test]
    fn abort_short_circuits() {
        let mut sup = PipelineSupervisor::new();
        sup.begin_phase("segment");
        sup.abort("E001", "Segmentation error");
        // Subsequent calls absorb; state stays $Aborted.
        sup.begin_phase("parse");
        sup.complete_phase();
        sup.finish();
        let s = sup.summary();
        assert_eq!(s.state, SupervisorState::Aborted);
        assert_eq!(s.abort_code.as_deref(), Some("E001"));
        assert_eq!(s.abort_msg.as_deref(), Some("Segmentation error"));
    }

    #[test]
    fn nonfatal_errors_route_to_failed() {
        let mut sup = PipelineSupervisor::new();
        sup.begin_phase("parse");
        sup.record_nonfatal("E002", "Parse error in system 'Foo'");
        sup.complete_phase();
        sup.begin_phase("validate");
        sup.record_nonfatal("E413", "HSM cycle");
        sup.complete_phase();
        sup.finish();
        let s = sup.summary();
        assert_eq!(s.state, SupervisorState::Failed);
        assert_eq!(s.error_count, 2);
    }

    #[test]
    fn implicit_phase_close_on_new_begin() {
        // The orchestrator commonly flows from one phase to the next
        // without explicitly calling complete_phase(). Beginning a
        // new phase from $Running closes the prior one implicitly.
        let mut sup = PipelineSupervisor::new();
        sup.begin_phase("segment");
        sup.begin_phase("parse"); // implicit close of "segment"
        sup.begin_phase("validate"); // implicit close of "parse"
        sup.complete_phase();
        sup.finish();
        let s = sup.summary();
        assert_eq!(s.state, SupervisorState::Done);
        assert_eq!(s.phases, vec!["segment", "parse", "validate"]);
    }

    #[test]
    fn idle_summary_is_observable_before_start() {
        let mut sup = PipelineSupervisor::new();
        let s = sup.summary();
        assert_eq!(s.state, SupervisorState::Idle);
        assert!(s.phases.is_empty());
        assert_eq!(s.error_count, 0);
    }
}
