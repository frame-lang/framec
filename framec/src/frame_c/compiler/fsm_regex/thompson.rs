//! Thompson NFA construction.
//!
//! Builds an ε-NFA from a [`RegexAst`] following the standard Thompson
//! recipe (concat, alt, quantifier, group). The resulting NFA has
//! exactly one start state and one accept state per RFC-0042 §6.9
//! step 2.
//!
//! Lazy quantifiers and other non-greedy semantics are *not* handled
//! here — they were rejected by [`super::restrictions`] before reaching
//! Thompson. Anchors are encoded as zero-width transitions consumed by
//! the matcher's position machinery; the subset construction
//! ([`super::subset`]) treats them as ε-transitions for closure
//! purposes.

use super::ast::{Anchor, RegexAst};
use super::Alphabet;

/// ε-NFA produced by Thompson construction.
#[derive(Debug, Clone)]
pub struct Nfa {
    pub states: Vec<NfaState>,
    pub start: NfaStateId,
    pub accept: NfaStateId,
    pub alphabet: Alphabet,
}

pub type NfaStateId = usize;

#[derive(Debug, Clone)]
pub struct NfaState {
    pub id: NfaStateId,
    pub transitions: Vec<NfaTransition>,
}

#[derive(Debug, Clone)]
pub struct NfaTransition {
    pub on: TransitionLabel,
    pub to: NfaStateId,
}

/// What an NFA transition consumes (or doesn't).
#[derive(Debug, Clone)]
pub enum TransitionLabel {
    /// ε — no input consumed. Used by Thompson for sequencing,
    /// alternation, and quantifier wiring.
    Epsilon,

    /// Match a single byte. Byte alphabet.
    Byte(u8),

    /// Match a byte range, low..=high inclusive. Byte alphabet, from
    /// character classes.
    ByteRange { low: u8, high: u8 },

    /// Match a Unicode code point. Char alphabet.
    CodePoint(char),

    /// Match a code-point range, low..=high inclusive. Char alphabet.
    CodePointRange { low: char, high: char },

    /// Match a single token kind by name. Token alphabet.
    Token(String),

    /// Zero-width position assertion. Treated as ε in subset
    /// construction; checked by the matcher at recognition time.
    Anchor(Anchor),
}

/// Build a Thompson ε-NFA from a validated [`RegexAst`].
///
/// **Precondition:** `ast` must have passed [`super::restrictions::check`].
/// Forbidden nodes still present in the AST will trip a debug assertion.
pub fn build(_ast: &RegexAst, _alphabet: Alphabet) -> Nfa {
    todo!("Phase 4: standard Thompson construction over RegexNode")
}
