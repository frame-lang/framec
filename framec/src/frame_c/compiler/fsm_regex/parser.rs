//! Frame regex syntax → AST.
//!
//! Parses the body of a `/.../` regex literal into a [`RegexAst`]. The
//! parser is alphabet-aware so it can produce correct
//! [`Literal`](super::ast::Literal) variants. Forbidden constructs are
//! parsed structurally into [`ForbiddenConstruct`](super::ast::ForbiddenConstruct)
//! variants; rejection happens in
//! [`super::restrictions::check`].
//!
//! v0.1 implementation: hand-written recursive-descent.
//! Phase 4 of the RFC-0042 execution plan.

use super::ast::RegexAst;
use super::{Alphabet, Span};

/// Parse a regex literal body.
///
/// `source` is the text between the delimiting `/` characters (without
/// the slashes). `alphabet` controls how bare literals are interpreted.
///
/// Returns a complete AST including any forbidden constructs. Use
/// [`super::restrictions::check`] to validate.
pub fn parse(_source: &str, _alphabet: Alphabet) -> Result<RegexAst, ParseError> {
    todo!("Phase 4: hand-written recursive-descent parser")
}

/// Parse failure — a syntactically malformed regex that cannot be
/// recovered into an AST. Semantic restrictions (forbidden-but-parseable
/// constructs) are reported by [`super::restrictions`] instead.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ParseErrorKind {
    /// Hit an unexpected character that no production accepts.
    Unexpected(char),

    /// `(` without matching `)`.
    UnclosedGroup,

    /// `[` without matching `]`.
    UnclosedClass,

    /// `{n}` / `{n,m}` quantifier malformed (e.g., `{,}`, `{a}`).
    MalformedQuantifier,

    /// `\X` where X is not a recognized escape.
    UnknownEscape(String),

    /// Class contains no members (`[]` or `[^]`).
    EmptyClass,

    /// `\xN` where N is incomplete (need exactly two hex digits).
    IncompleteHexEscape,

    /// `\u{...}` malformed or out of Unicode range.
    InvalidUnicodeEscape,
}
