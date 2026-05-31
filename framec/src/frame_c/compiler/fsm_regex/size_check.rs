//! DFA size limits per RFC-0042 §9.1 E721 and §9.2 W704.
//!
//! Frame fsms can configure their DFA size budget via the
//! `@@[max_dfa_states(N)]` attribute on the fsm declaration. When the
//! attribute is absent, [`DEFAULT_MAX_DFA_STATES`] applies.
//!
//! A DFA whose state count *exceeds* the configured limit triggers
//! E721 (compile error). A DFA whose state count meets or exceeds
//! [`WARN_THRESHOLD_PERCENT`] of the limit triggers W704 (warning but
//! compilation continues).
//!
//! Decision: 10,000 default and 75 % warn threshold match RE2's
//! published heuristics and the FSM-TEST-1105 expected behavior.

use super::subset::Dfa;

/// Default `max_dfa_states` when `@@[max_dfa_states(N)]` is absent.
pub const DEFAULT_MAX_DFA_STATES: usize = 10_000;

/// Percent-of-limit threshold above which W704 fires.
pub const WARN_THRESHOLD_PERCENT: usize = 75;

#[derive(Debug, Clone)]
pub struct SizeCheckResult {
    pub state_count: usize,
    pub limit: usize,
    pub status: SizeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeStatus {
    /// `state_count` is below the warn threshold. No diagnostic.
    Ok,
    /// `state_count` is at or above [`WARN_THRESHOLD_PERCENT`] of the
    /// limit, but still under the limit. W704.
    Approaching,
    /// `state_count` strictly exceeds the limit. E721.
    Exceeds,
}

/// Compute the size status for a (minimized) DFA against a configured
/// limit.
pub fn check(dfa: &Dfa, max_states: usize) -> SizeCheckResult {
    let state_count = dfa.states.len();
    let warn_threshold = max_states.saturating_mul(WARN_THRESHOLD_PERCENT) / 100;
    let status = if state_count > max_states {
        SizeStatus::Exceeds
    } else if state_count >= warn_threshold {
        SizeStatus::Approaching
    } else {
        SizeStatus::Ok
    };
    SizeCheckResult {
        state_count,
        limit: max_states,
        status,
    }
}
