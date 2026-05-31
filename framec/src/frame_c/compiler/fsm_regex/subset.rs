//! Subset construction — NFA → DFA.
//!
//! Standard powerset algorithm: each DFA state corresponds to a set of
//! NFA states reachable via ε-closure; DFA transitions are computed by
//! union-of-ε-closures over the input symbol. RFC-0042 §6.9 step 3.
//!
//! The result is a DFA that is *not yet minimal*. Minimization happens
//! in [`super::hopcroft`].
//!
//! Anchors encoded as ε-equivalent transitions in the NFA are pulled
//! into the DFA as conditions on a DFA transition (rather than as
//! standalone DFA states), so a single DFA state can carry multiple
//! position-conditional transitions on the same input.

use super::thompson::Nfa;
use super::Alphabet;

/// DFA produced by subset construction.
#[derive(Debug, Clone)]
pub struct Dfa {
    pub states: Vec<DfaState>,
    pub start: DfaStateId,
    pub alphabet: Alphabet,
}

pub type DfaStateId = usize;

#[derive(Debug, Clone)]
pub struct DfaState {
    pub id: DfaStateId,
    pub transitions: Vec<DfaTransition>,
    /// `true` if any of the NFA states this DFA state represents is the
    /// NFA's accept state.
    pub is_accept: bool,
}

#[derive(Debug, Clone)]
pub struct DfaTransition {
    /// Input condition under which this transition fires.
    pub label: DfaLabel,
    /// Destination DFA state.
    pub to: DfaStateId,
    /// Position assertions (anchors) that must also hold. Empty in the
    /// common case; populated for transitions that originate from an
    /// anchor-bearing region of the NFA.
    pub assertions: Vec<super::ast::Anchor>,
}

/// What input a DFA transition consumes. Deterministic: at most one
/// transition per (state, input) pair after subset construction.
#[derive(Debug, Clone)]
pub enum DfaLabel {
    /// Byte alphabet.
    Byte(u8),
    ByteRange { low: u8, high: u8 },

    /// Char alphabet.
    CodePoint(char),
    CodePointRange { low: char, high: char },

    /// Token alphabet.
    Token(String),
}

/// Run subset construction on `nfa`. Returns a DFA with the same
/// alphabet; uses position-conditional transitions for anchors.
pub fn construct(_nfa: &Nfa) -> Dfa {
    todo!("Phase 4: powerset construction with ε-closure caching")
}
