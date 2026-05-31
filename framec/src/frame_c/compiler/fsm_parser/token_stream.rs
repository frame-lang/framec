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

/// Token-kind tags relevant to fsm parsing. Populated in Phase 1
/// alongside the lexer extensions; this enum will expand to cover
/// regex literals, `.label` stage prefixes, embedding-action
/// operators, RFC-0043 statement keywords, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsmTokenKind {
    // Placeholder set; real variants land with the lexer work.
    /// `@@fsm` keyword
    KwFsm,
    /// `{` `}` `(` `)` `,` `:` `=` etc — punctuation
    Punct(char),
    /// Identifier
    Ident(String),
    /// End-of-fsm-block marker (or EOF).
    Eof,
    // ... more to come.
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
}
