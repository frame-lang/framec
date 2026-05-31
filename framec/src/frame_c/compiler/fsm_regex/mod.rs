//! `@@fsm` regex engine.
//!
//! Implements the regex-dialect compilation pipeline from RFC-0042 §6:
//!
//! ```text
//!   parser       : &str + Alphabet -> RegexAst        (§6.2–§6.8)
//!   restrictions : &RegexAst + Alphabet -> Vec<Diag>  (§6.3, §6.4, §6.5, §6.7, §6.8)
//!   thompson     : &RegexAst -> Nfa                   (§6.9 step 2)
//!   subset       : &Nfa -> Dfa                        (§6.9 step 3)
//!   hopcroft     : &Dfa -> Dfa  (minimized)           (§6.9 step 4)
//!   size_check   : &Dfa + max_states -> Result        (§9.1 E721, §9.2 W704)
//!   metrics      : &Dfa -> DfaMetrics                 (§9.3)
//! ```
//!
//! # v0.1 scope
//!
//! v0.1 builds a **pure DFA**. There is no NFA simulation at runtime; once
//! we have a minimal DFA, codegen emits a DFA executor (table-driven by
//! default; switch-driven if `@@[dispatch(switch)]`).
//!
//! Lazy quantifiers, lookaround, Unicode general-category classes, named
//! captures, backreferences, and recursion are all rejected by
//! `restrictions::check`. Their handling is deferred to v0.2 per RFC-0042
//! §11.
//!
//! # Wiring status
//!
//! This module is not yet wired into the parent `compiler::mod`. Adding
//! `pub mod fsm_regex;` to `framec/src/frame_c/compiler/mod.rs` is the
//! first deliverable of Phase 1 (lexer/parser/AST). Until then this
//! module compiles in isolation as a sketch establishing data shapes.

pub mod ast;
pub mod hopcroft;
pub mod metrics;
pub mod parser;
pub mod restrictions;
pub mod size_check;
pub mod subset;
pub mod thompson;

/// The alphabet of a regex. Determined by the `@@fsm`'s input parameter
/// type (RFC-0042 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    /// Octets 0..=255. Default for `bytes` input.
    Bytes,
    /// Unicode code points. For `char` input.
    Char,
    /// Application-defined token kinds. For `token` input.
    Token,
}

/// Half-open source span; `start..end` byte offsets into the regex
/// literal's interior (the text between the delimiting `/` characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}
