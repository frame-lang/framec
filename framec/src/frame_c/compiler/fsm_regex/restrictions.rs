//! v0.1 dialect restrictions per RFC-0042 §6.3, §6.4, §6.5, §6.7, §6.8.
//!
//! Walks a [`RegexAst`] and emits diagnostics for:
//!
//! - Non-regular constructs (backrefs, recursion, variable-width
//!   lookbehind) — E720, RFC-0042 §6.4.
//! - Regular-but-v0.1-excluded constructs (Unicode classes, lookaround,
//!   lazy quantifiers) — E720, §6.5.
//! - Alphabet-invalid constructs (byte escapes in char alphabet,
//!   character classes in token alphabet) — E722, §6.7 / §6.8.
//! - Empty regex — E723.
//! - RE2 constructs that Frame replaces with native equivalents
//!   (named captures, non-capturing groups) — E720, §6.3.
//!
//! Each diagnostic includes a span and a recovery hint.

use super::ast::{ForbiddenConstruct, Literal, RegexAst, RegexNode, SpannedNode};
use super::{Alphabet, Span};

/// Validate a parsed regex against the v0.1 dialect and the declared
/// alphabet. Returns *all* violations, not just the first — this lets
/// the frontend emit batched diagnostics in one pass.
pub fn check(_ast: &RegexAst, _alphabet: Alphabet) -> Vec<Diagnostic> {
    todo!("Phase 4: walk AST; emit per-node diagnostics")
}

/// One diagnostic emitted by [`check`].
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: DiagCode,
    pub span: Span,
    pub message: String,
    /// Free-form text for the `help:` line of the rendered diagnostic.
    /// Non-normative per the RFC's diagnostic-test entries.
    pub recovery_hint: String,
}

/// Diagnostic codes this module emits. Maps 1:1 to RFC-0042 §9.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    /// Forbidden regex construct (non-regular OR regular-but-excluded-in-v0.1).
    E720,
    /// DFA size limit. Not emitted by this module; see
    /// [`super::size_check`].
    #[allow(dead_code)]
    E721,
    /// Invalid regex syntax for the current alphabet.
    E722,
    /// Empty regex (`//`) where non-empty required.
    E723,
}

/// Internal traversal helper. Phase 4 will implement.
#[allow(dead_code)]
fn visit(_node: &SpannedNode, _alphabet: Alphabet, _diagnostics: &mut Vec<Diagnostic>) {
    // Dispatch on node kind:
    //   RegexNode::Forbidden(_)     → emit E720 with variant-specific message
    //   RegexNode::Literal(byte) in Char alphabet → E722
    //   RegexNode::Literal(cp)   in Bytes/Token   → E722
    //   RegexNode::Class(_)      in Token alphabet → E722
    //   RegexNode::Empty (top-level) → E723
    //   RegexNode::Quantifier { laziness: Lazy, .. } → E720
    //   ...descend into children
    todo!()
}

/// Convenience: produce the canonical E720 message for a forbidden
/// construct. Centralized so the diagnostic-test snapshots have a
/// single source of wording.
#[allow(dead_code)]
pub(crate) fn forbidden_message(c: &ForbiddenConstruct) -> &'static str {
    match c {
        ForbiddenConstruct::Backref(_) => "backreferences are non-regular",
        ForbiddenConstruct::NamedBackref(_) => "named backreferences are non-regular",
        ForbiddenConstruct::Recursion => "regex recursion is non-regular",
        ForbiddenConstruct::PositiveLookahead(_)
        | ForbiddenConstruct::NegativeLookahead(_) => {
            "fixed-width lookahead is not supported in v0.1"
        }
        ForbiddenConstruct::PositiveLookbehind(_)
        | ForbiddenConstruct::NegativeLookbehind(_) => {
            "lookbehind is not supported in v0.1"
        }
        ForbiddenConstruct::UnicodeClass(_) => {
            "Unicode general-category classes are not supported in v0.1"
        }
        ForbiddenConstruct::NamedCapture { .. } => {
            "named captures are replaced by stage labels (RFC-0042 §3.5.2)"
        }
        ForbiddenConstruct::NonCapturingGroup(_) => {
            "`(?:...)` is unnecessary in Frame — `()` does not capture"
        }
    }
}

/// Convenience: alphabet compatibility predicate for a literal node.
/// Phase 4 will wire this into [`visit`].
#[allow(dead_code)]
pub(crate) fn literal_matches_alphabet(lit: &Literal, alphabet: Alphabet) -> bool {
    matches!(
        (lit, alphabet),
        (Literal::Byte(_), Alphabet::Bytes)
            | (Literal::CodePoint(_), Alphabet::Char)
            | (Literal::Token(_), Alphabet::Token)
    )
}
