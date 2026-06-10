//! Frame regex AST.
//!
//! The AST represents *every* construct the regex grammar can express,
//! including those forbidden by v0.1. Forbidden constructs are captured
//! as [`Forbidden`] variants and rejected downstream by
//! [`crate::compiler::fsm_regex::restrictions::check`]. This lets the
//! parser produce a complete tree before diagnostics run, and gives
//! diagnostics full positional context.
//!
//! All nodes carry a [`super::Span`] so diagnostics can point at the
//! exact subexpression.

use super::Span;

/// A regex AST node with its source span.
#[derive(Debug, Clone)]
pub struct SpannedNode {
    pub node: RegexNode,
    pub span: Span,
}

/// Regex node kinds. Alphabet-generic; alphabet-specific validation
/// happens in [`crate::compiler::fsm_regex::restrictions`].
#[derive(Debug, Clone)]
pub enum RegexNode {
    /// Single literal element: one byte, one code point, or one token
    /// kind. Interpretation depends on the [`super::Alphabet`].
    Literal(Literal),

    /// Character class — set of element values defined by ranges and
    /// shorthand escapes. `negated` flips the membership.
    Class(CharClass),

    /// `.` — any element except `\n` by default. The
    /// `@@[dot_matches_newline]` attribute on the fsm changes this.
    Dot,

    /// Concatenation; children evaluate left-to-right. An empty
    /// `Concat([])` represents the empty regex (E723 if standalone).
    Concat(Vec<SpannedNode>),

    /// Alternation; first-match wins per RE2 semantics.
    Alt(Vec<SpannedNode>),

    /// Quantified repetition.
    Quantifier {
        inner: Box<SpannedNode>,
        kind: QuantifierKind,
        laziness: Laziness,
    },

    /// Non-capturing group. Frame has no capturing groups; captures
    /// are stage-based (RFC-0042 §3.5.2).
    Group(Box<SpannedNode>),

    /// Zero-width position anchor.
    Anchor(Anchor),

    /// `//` — empty regex literal. Produces E723.
    Empty,

    /// Constructs the parser recognizes but [`restrictions`] rejects.
    Forbidden(ForbiddenConstruct),
}

/// Literal element. Variant chosen per alphabet:
///
/// - `Alphabet::Bytes` → `Literal::Byte`
/// - `Alphabet::Char`  → `Literal::CodePoint`
/// - `Alphabet::Token` → `Literal::Token`
///
/// Wrong-alphabet literals produce E722.
#[derive(Debug, Clone)]
pub enum Literal {
    Byte(u8),
    CodePoint(char),
    Token(String),
}

/// Character class — a set of elements expressed as ranges and shorthand
/// escapes.
#[derive(Debug, Clone)]
pub struct CharClass {
    pub negated: bool,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    /// Single-element range — `[abc]` → three `Single` members.
    Single(u32),

    /// Inclusive range — `[a-z]`.
    Range { low: u32, high: u32 },

    /// `\d`, `\w`, `\s` (negated forms via `Shorthand::negated`).
    Shorthand { kind: ShorthandKind, negated: bool },

    /// `\p{Name}` / `\P{Name}` — a Unicode general-category or script class
    /// (`negated` for `\P`). Resolved to codepoint ranges by
    /// [`super::unicode`] before restrictions/Thompson (char alphabet only,
    /// RFC-0042 §6.7/§11.6); the engine never carries an unresolved member
    /// into the DFA.
    Unicode { name: String, negated: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShorthandKind {
    /// `\d` / `\D` — ASCII digit; Unicode-aware in char alphabet.
    Digit,
    /// `\w` / `\W` — word character `[A-Za-z0-9_]`; Unicode-aware in
    /// char alphabet.
    Word,
    /// `\s` / `\S` — whitespace `[ \t\n\r\f\v]`.
    Whitespace,
}

#[derive(Debug, Clone, Copy)]
pub enum QuantifierKind {
    /// `?` — zero or one.
    ZeroOrOne,
    /// `*` — zero or more.
    ZeroOrMore,
    /// `+` — one or more.
    OneOrMore,
    /// `{n}` — exactly `n`.
    Exact(u32),
    /// `{n,m}` — between `n` and `m` inclusive.
    Bounded { min: u32, max: u32 },
    /// `{n,}` — at least `n`.
    AtLeast(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Laziness {
    /// `?`, `*`, `+`, `{n}`, `{n,m}`, `{n,}` — longest match. The only
    /// form admitted in v0.1.
    Greedy,
    /// `??`, `*?`, `+?`, `{n}?`, `{n,m}?`, `{n,}?` — shortest match.
    /// Rejected with E720 in v0.1; deferred to v0.2 per RFC-0042 §11.1.
    Lazy,
}

#[derive(Debug, Clone, Copy)]
pub enum Anchor {
    /// `^` — start of input, or post-`\n` in `@@[multiline]` mode.
    LineStart,
    /// `$` — end of input, or pre-`\n` in `@@[multiline]` mode.
    LineEnd,
    /// `\A` — absolute start of input (always).
    InputStart,
    /// `\z` — absolute end of input (always).
    InputEnd,
    /// `\b` — word boundary.
    WordBoundary,
    /// `\B` — non-word boundary.
    NonWordBoundary,
}

/// Constructs the parser admits structurally but [`restrictions`]
/// rejects. Each variant carries enough context for a precise
/// diagnostic.
#[derive(Debug, Clone)]
pub enum ForbiddenConstruct {
    /// `\1`, `\2`, … — backreference. Non-regular. E720.
    Backref(u32),

    /// `(?P=name)` — named backreference. Non-regular. E720.
    NamedBackref(String),

    /// `(?R)`, `(?-1)` — recursion. Non-regular. E720.
    Recursion,

    /// `(?=foo)` — positive lookahead. Regular but v0.1-excluded. E720;
    /// deferred to v0.2 per RFC-0042 §11.5.
    PositiveLookahead(Box<SpannedNode>),

    /// `(?!foo)` — negative lookahead. E720.
    NegativeLookahead(Box<SpannedNode>),

    /// `(?<=foo)` — positive lookbehind. E720.
    PositiveLookbehind(Box<SpannedNode>),

    /// `(?<!foo)` — negative lookbehind. E720.
    NegativeLookbehind(Box<SpannedNode>),

    /// `(?P<name>...)` — named capture. Frame uses stage labels
    /// instead (RFC-0042 §3.5.2). E720.
    NamedCapture {
        name: String,
        inner: Box<SpannedNode>,
    },

    /// `(?:...)` — non-capturing group. Unnecessary in Frame since
    /// `()` doesn't capture anyway. Currently flagged with E720; may
    /// become a warning in a future revision.
    NonCapturingGroup(Box<SpannedNode>),
}

/// Top-level parsed regex AST.
#[derive(Debug, Clone)]
pub struct RegexAst {
    pub root: SpannedNode,
}
