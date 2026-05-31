//! Token stream shared across the cooperating fsm-parser FSMs.
//!
//! `FsmTokenStream` wraps a flat `Vec<FsmToken>` and a cursor. Parser
//! FSMs hold it as `Option<FsmTokenStream>` in their domain; the
//! parent moves the stream into the child via `Option::take` and
//! reclaims it after the child finishes. Linear ownership; no
//! `Rc<RefCell<>>`; no shared mutable state.

use crate::frame_c::compiler::frame_ast::Span;

/// One token recognized by the framec lexer's fsm-aware mode.
///
/// The lexer's actual token type lives in
/// [`crate::frame_c::compiler::lexer`] and will gain fsm-specific
/// variants in Phase 1 (regex literals, embedding-action operators,
/// stage labels, RFC-0043 statement keywords). This module's
/// `FsmToken` is a re-export or wrapper over that type — final shape
/// pinned down when the lexer extensions land.
#[derive(Debug, Clone)]
pub struct FsmToken {
    pub kind: FsmTokenKind,
    pub span: Span,
}

/// Token-kind tags produced by the fsm lexer (`fsm_lexer.frs`).
///
/// Final token set per `_scratch/rfc_0042_lexer_design.md`. Covers the
/// `@@fsm` header, regex literals, stage/state references, embedding-action
/// operators, the RFC-0043 statement/expression token set, and `@@:`
/// context probes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmTokenKind {
    // --- Keywords ---
    /// `@@fsm`
    KwFsm,
    /// `if`
    KwIf,
    /// `else`
    KwElse,
    /// `true`
    KwTrue,
    /// `false`
    KwFalse,
    /// `when` — transition-guard keyword (RFC-0042 §3.5.4.1)
    KwWhen,
    /// `actions` section header
    KwActions,
    /// `domain` section header
    KwDomain,

    // --- Identifiers & literals ---
    Ident(String),
    IntLit(i64),
    StringLit(String),

    // --- Regex + stage labels ---
    /// Regex literal body, verbatim between the delimiting `/`s
    /// (escapes like `\/` resolved). Not parsed here — handed to
    /// `StageAst.regex` and parsed later by `fsm_regex`.
    RegexLiteral(String),
    /// `.name` stage label preceding a `/regex/`.
    StageLabel(String),

    // --- State references ---
    /// `$Name:` — a state-label declaration.
    StateLabel(String),
    /// `$Name` — a transition target / state reference.
    StateRef(String),
    /// `$State.stage` — a stage-capture or stage-target reference.
    StageRef {
        state: String,
        stage: String,
    },

    // --- Context probes ---
    /// `@@:cursor`, `@@:matched`, `@@:fc`, `@@:return`, etc. The string
    /// is the probe name without the `@@:` prefix.
    Probe(String),

    // --- Embedding-action operators (op fused with its opening `{`) ---
    /// `>{`
    EmbedStart,
    /// `@{`
    EmbedAccept,
    /// `${`
    EmbedEvery,
    /// `%{`
    EmbedLeave,
    /// `@eof{`
    EmbedEof,

    // --- Operators ---
    AndAnd,  // &&
    OrOr,    // ||
    Bang,    // !
    EqEq,    // ==
    NotEq,   // !=
    Le,      // <=
    Ge,      // >=
    Lt,      // <
    Gt,      // >
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /  (division — only emitted in expression context)
    Percent, // %
    Eq,      // =
    Arrow,   // ->

    // --- Punctuation ---
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    Pipe, // |

    /// End of the fsm block.
    Eof,
}

/// Mutable cursor over a fsm declaration's tokens.
///
/// Parser FSMs take this by `Option::take` from their parent, run, and
/// hand it back. The cursor advances as tokens are consumed; the
/// caller can `peek`, `at`, `advance`, or `expect` specific kinds.
#[derive(Debug)]
pub struct FsmTokenStream {
    tokens: Vec<FsmToken>,
    cursor: usize,
}

impl FsmTokenStream {
    pub fn new(tokens: Vec<FsmToken>) -> Self {
        Self { tokens, cursor: 0 }
    }

    /// Look at the current token without consuming.
    pub fn peek(&self) -> &FsmToken {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }

    /// Consume and return the current token; advances the cursor.
    pub fn advance(&mut self) -> FsmToken {
        let tok = self.tokens[self.cursor].clone();
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
        tok
    }

    /// True if the current token matches `kind`.
    pub fn at(&self, kind: &FsmTokenKind) -> bool {
        self.peek().kind == *kind
    }

    /// Cursor position — useful for span construction.
    pub fn position(&self) -> usize {
        self.cursor
    }

    /// True if we're at end-of-tokens.
    pub fn is_eof(&self) -> bool {
        matches!(self.peek().kind, FsmTokenKind::Eof)
    }

    /// Clone of the current token's kind — convenient for `match` in
    /// parser action bodies where borrowing `peek()` would conflict
    /// with a subsequent `advance()`.
    pub fn peek_kind(&self) -> FsmTokenKind {
        self.peek().kind.clone()
    }

    /// If the current token matches `kind`, consume it and return true;
    /// otherwise leave the cursor put and return false.
    pub fn eat(&mut self, kind: &FsmTokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Span of the current token (for diagnostics / AST node spans).
    pub fn cur_span(&self) -> Span {
        self.peek().span.clone()
    }
}
