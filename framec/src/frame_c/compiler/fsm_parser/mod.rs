//! `@@fsm` parser — a tree of cooperating Frame `@@system` FSMs that
//! parse `@@fsm` declarations into the AST shapes defined in
//! [`crate::frame_c::compiler::frame_ast`].
//!
//! # Scope
//!
//! - **Parses:** `@@fsm` declarations and their bodies per RFC-0042 +
//!   RFC-0043 (statement syntax inside action bodies).
//! - **Does not parse:** `@@system` declarations. The existing
//!   [`crate::frame_c::compiler::pipeline_parser`] handles those, untouched
//!   by this module. Per the design contract: no parser code is shared
//!   between the system parser and the fsm parser. (Lexer-level
//!   infrastructure is shared, since both parsers consume the same source
//!   files; only the *parsing* logic is unshared.)
//!
//! # Architecture
//!
//! Eight `@@system` FSMs, each a separate `.frs` source file that
//! framec compiles to a sibling `.gen.rs`. The tree:
//!
//! ```text
//!   FsmDeclParser           // root — parses one @@fsm declaration
//!     ├─ StateParser        // parses $Label: matches | matches | ...
//!     │    ├─ MatchParser   // parses one match (element sequence + transition)
//!     │    │    ├─ StageParser        // parses .label/regex/embed_actions
//!     │    │    │    └─ RegexParser   // delegates to fsm_regex/
//!     │    │    └─ ActionBlockParser  // parses { stmt; stmt; ... }
//!     │    │         └─ StatementParser
//!     │    │              └─ ExpressionParser  // precedence climbing
//!     │    └─ TransitionParser (inline in StateParser/MatchParser)
//!     ├─ ActionsBlockParser
//!     └─ DomainBlockParser
//! ```
//!
//! Composition is at the Rust level via [`linear ownership shuttling`]:
//! each parent FSM holds [`token_stream::FsmTokenStream`] as
//! `Option<FsmTokenStream>`, `take()`s it into the child via
//! `child.tokens = self.tokens.take()`, runs the child to its `$Done`
//! state, then `take()`s the stream back. Only one FSM holds the
//! stream at a time. No `Rc<RefCell<>>`; no borrow-checker friction.
//!
//! [`linear ownership shuttling`]: ../../../../../../_scratch/rfc_0043_parser_design.md
//!
//! # `.frs` regen workflow
//!
//! framec does not auto-compile `.frs` files during `cargo build`. The
//! workflow matches the existing dogfood pattern from
//! [`pipeline_supervisor`](crate::frame_c::compiler::pipeline_supervisor):
//!
//! 1. Edit the `.frs` source.
//! 2. Run a previously-built framec against it:
//!    ```bash
//!    framec compile -l rust \
//!      -o framec/src/frame_c/compiler/fsm_parser/ \
//!      framec/src/frame_c/compiler/fsm_parser/<name>.frs
//!    ```
//! 3. Rename the emitted `<name>.rs` to `<name>.gen.rs`.
//! 4. Commit both `<name>.frs` and `<name>.gen.rs`.
//!
//! Bootstrap framec is any recent main build (≥ 4.3.0).
//!
//! # Status
//!
//! Working front-end for a growing subset of `@@fsm`. The parser is a
//! tree of cooperating `@@system` FSMs: `fsm_decl_parser` (root) →
//! `state_parser` → { `expression_parser`, `action_block_parser` ⇄
//! `statement_parser` }, with `expression_parser` under the statement
//! and state parsers. Entry: [`lex_fsm_block`], [`parse_fsm_declaration`],
//! [`parse_fsm_block`].
//!
//! Coverage expands fixture-by-fixture. Parsed today: the header (name,
//! params, return type, default); multiple states (implicit start +
//! `$Label:`); stages (`/regex/`, `.label/regex/`); ordered-choice `|`
//! matches; transition clauses with static, stage-ref, and conditional
//! `when` targets; full expressions per RFC-0043 §3.3 (literals, probes,
//! vars, calls, member access, parens, unary, binary ops with precedence
//! + left-assoc); action blocks `{ ... }` with RFC-0043 statements
//! (assignment, call/expression, `if`/`else`/`else if`); and both body
//! sections — `actions:` (declared helpers with typed params, optional
//! return type, and a body) and `domain:` (typed fields with parsed
//! default expressions); and embedding actions on stages (`>{`, `@{`,
//! `${`, `%{`, `@eof{`, each with an action-block body). The only
//! remaining grammar gap is the `@@:return =` / `@@:(expr)` return
//! statement forms. The module is wired into
//! [`crate::frame_c::compiler`] but the framec driver does not yet route
//! real `@@fsm` blocks here (Task 14).
//!
//! # Public API
//!
//! - [`lex_fsm_block`] — bytes → token stream.
//! - [`parse_fsm_declaration`] — token stream → AST.
//! - [`parse_fsm_block`] — bytes → AST (lex + parse).

pub mod token_stream;

use crate::frame_c::compiler::frame_ast::FsmDeclAst;
use crate::frame_c::compiler::pipeline_parser::ParseError;
use token_stream::{FsmToken, FsmTokenKind, FsmTokenStream};

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// The generated `FsmLexer` state machine. Source: `fsm_lexer.frs`.
/// Regenerate after editing the `.frs` (see the module docs above), then
/// rename `fsm_lexer.rs` → `fsm_lexer.gen.rs`.
mod fsm_lexer {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::lex_helpers::{push1, scan_string, skip_ws_comments};
    use super::{FsmToken, FsmTokenKind};
    use crate::frame_c::compiler::frame_ast::Span;
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("fsm_lexer.gen.rs");
}

/// Native helpers the generated lexer's action bodies call. Kept out of
/// the `.frs` because they're pure byte utilities with no state-machine
/// content.
mod lex_helpers {
    use super::{FsmToken, FsmTokenKind};
    use crate::frame_c::compiler::frame_ast::Span;

    /// Push a single-byte token at `pos` (span `pos..pos+1`).
    pub(super) fn push1(tokens: &mut Vec<FsmToken>, kind: FsmTokenKind, pos: usize) {
        tokens.push(FsmToken {
            kind,
            span: Span::new(pos, pos + 1),
        });
    }

    /// Scan a `"..."` string literal starting at the opening quote `open`.
    /// Returns (content-between-quotes, position-after-closing-quote,
    /// terminated). `\"` and other backslash escapes are skipped during
    /// the scan; the content is returned verbatim (unescaping happens
    /// later if needed).
    pub(super) fn scan_string(src: &[u8], open: usize) -> (String, usize, bool) {
        let n = src.len();
        let mut pos = open + 1; // past the opening quote
        let content_start = pos;
        while pos < n && src[pos] != b'"' {
            if src[pos] == b'\\' && pos + 1 < n {
                pos += 2;
            } else {
                pos += 1;
            }
        }
        if pos >= n {
            return (String::new(), pos, false); // unterminated
        }
        let content = std::str::from_utf8(&src[content_start..pos])
            .unwrap_or("")
            .to_string();
        (content, pos + 1, true) // past the closing quote
    }

    /// Advance past ASCII whitespace and Frame-level comments (`//` to
    /// end-of-line, `/* */` to the matching close). Returns the new
    /// position. Per RFC-0043 §3.5, both comment forms are whitespace.
    ///
    /// Note: this is called from `$Header` and `$ElementLevel`, where a
    /// `/` that begins `//` or `/*` is a comment — distinct from a `/`
    /// that opens a regex literal. The two-char lookahead disambiguates:
    /// `//` and `/*` are comments; a lone `/` is left for the caller to
    /// handle as a regex delimiter.
    pub(super) fn skip_ws_comments(src: &[u8], mut pos: usize) -> usize {
        let n = src.len();
        loop {
            // ASCII whitespace.
            while pos < n
                && (src[pos] == b' ' || src[pos] == b'\t' || src[pos] == b'\n' || src[pos] == b'\r')
            {
                pos += 1;
            }
            // Line comment.
            if pos + 1 < n && src[pos] == b'/' && src[pos + 1] == b'/' {
                pos += 2;
                while pos < n && src[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            // Block comment (non-nesting per RFC-0043 §3.5).
            if pos + 1 < n && src[pos] == b'/' && src[pos + 1] == b'*' {
                pos += 2;
                while pos + 1 < n && !(src[pos] == b'*' && src[pos + 1] == b'/') {
                    pos += 1;
                }
                pos = (pos + 2).min(n);
                continue;
            }
            break;
        }
        pos
    }
}

/// Tokenize the raw bytes of an `@@fsm` declaration into a flat token
/// stream. Drives the `FsmLexer` state machine to `$Done` and lifts out
/// either the tokens or the first lexing error.
///
/// `bytes` is the `@@fsm` block's source (from the segmenter), beginning
/// at the `@@fsm` keyword.
pub fn lex_fsm_block(bytes: &[u8]) -> Result<Vec<FsmToken>, ParseError> {
    let mut lexer = fsm_lexer::FsmLexer::__create();
    lexer.bytes = bytes.to_vec();
    lexer.tokenize();
    match lexer.error {
        Some(e) => Err(e),
        None => Ok(lexer.tokens),
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// The generated `ExpressionParser` state machine. Source:
/// `expression_parser.frs`. The first child parser in the tree; proves
/// the token-stream-shuttling composition pattern.
mod expression_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::parse_helpers::{binary_op, binding_power};
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{Expression, Literal, UnaryOp};
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("expression_parser.gen.rs");
}

/// The generated `StatementParser` state machine. Source:
/// `statement_parser.frs`. Mutually recursive with `action_block_fsm`
/// (an `if` branch is an action block; an action block holds statements).
mod statement_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::action_block_fsm::ActionBlockParser;
    use super::expression_fsm::ExpressionParser;
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{
        BlockAst, Expression, ExpressionAst, IfAst, Span, Statement,
    };
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("statement_parser.gen.rs");
}

/// The generated `ActionBlockParser` state machine. Source:
/// `action_block_parser.frs`. Parses `{ statement* }` into a `BlockAst`.
mod action_block_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::statement_fsm::StatementParser;
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{BlockAst, Span, Statement};
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("action_block_parser.gen.rs");
}

/// The generated `ActionsBlockParser` state machine. Source:
/// `actions_block_parser.frs`. Parses the `actions:` section (action
/// declarations; each body via ActionBlockParser).
mod actions_block_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::action_block_fsm::ActionBlockParser;
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{
        FsmActionDecl, FsmActionsBlock, FsmParameter, Span, Type,
    };
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("actions_block_parser.gen.rs");
}

/// The generated `DomainBlockParser` state machine. Source:
/// `domain_block_parser.frs`. Parses the `domain:` section.
mod domain_block_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::expression_fsm::ExpressionParser;
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{FsmDomainBlock, FsmDomainVar, Span, Type};
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("domain_block_parser.gen.rs");
}

/// The generated `StateParser` state machine. Source: `state_parser.frs`.
/// Parses one state declaration (label + match elements + transition).
mod state_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::action_block_fsm::ActionBlockParser;
    use super::expression_fsm::ExpressionParser;
    use super::parse_helpers::parse_target;
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{
        EmbeddingActionAst, EmbeddingOp, Expression, FsmCondAlt, FsmStateAst,
        FsmTransitionClauseAst, FsmTransitionTarget, MatchAst, MatchElement, Span, StageAst,
    };
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("state_parser.gen.rs");
}

/// The generated `FsmDeclParser` state machine. Source: `fsm_decl_parser.frs`.
mod fsm_decl_parser {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::actions_block_fsm::ActionsBlockParser;
    use super::domain_block_fsm::DomainBlockParser;
    use super::parse_helpers::token_text;
    use super::state_fsm::StateParser;
    use super::token_stream::{FsmTokenKind, FsmTokenStream};
    use crate::frame_c::compiler::frame_ast::{
        FsmActionsBlock, FsmDeclAst, FsmDomainBlock, FsmParameter, FsmStateAst, Span, Type,
    };
    use crate::frame_c::compiler::pipeline_parser::ParseError;

    include!("fsm_decl_parser.gen.rs");
}

/// Native helpers the generated parser's action bodies call. Pure token
/// → AST utilities with no state-machine content.
mod parse_helpers {
    use super::token_stream::FsmTokenKind;
    use crate::frame_c::compiler::frame_ast::BinaryOp;

    /// Render a token's surface text — used to capture single-token
    /// default expressions (`= false`, `= 0`) verbatim. v1 covers the
    /// primary tokens a default expression can be; expands when the
    /// header default becomes a full expression.
    pub(super) fn token_text(kind: &FsmTokenKind) -> String {
        match kind {
            FsmTokenKind::KwTrue => "true".to_string(),
            FsmTokenKind::KwFalse => "false".to_string(),
            FsmTokenKind::IntLit(n) => n.to_string(),
            FsmTokenKind::StringLit(s) => format!("{:?}", s),
            FsmTokenKind::Ident(s) => s.clone(),
            _ => String::new(),
        }
    }

    /// Binding powers for binary operators, per RFC-0043 §3.3 precedence
    /// (loosest `||` to tightest `* / %`). Returns `None` for tokens that
    /// are not binary operators. The `(2k-1, 2k)` pairing makes every
    /// operator left-associative: a same-level operator's left power is
    /// below the right power the climb recurses with, so the inner climb
    /// stops and the fold happens left-to-right.
    pub(super) fn binding_power(op: &FsmTokenKind) -> Option<(u8, u8)> {
        Some(match op {
            FsmTokenKind::OrOr => (1, 2),
            FsmTokenKind::AndAnd => (3, 4),
            FsmTokenKind::EqEq | FsmTokenKind::NotEq => (5, 6),
            FsmTokenKind::Lt | FsmTokenKind::Le | FsmTokenKind::Gt | FsmTokenKind::Ge => (7, 8),
            FsmTokenKind::Plus | FsmTokenKind::Minus => (9, 10),
            FsmTokenKind::Star | FsmTokenKind::Slash | FsmTokenKind::Percent => (11, 12),
            _ => return None,
        })
    }

    /// Parse a transition target: a state reference `$State` or a
    /// stage reference `$State.stage`. Consumes the target token.
    pub(super) fn parse_target(
        ts: &mut super::token_stream::FsmTokenStream,
    ) -> Result<crate::frame_c::compiler::frame_ast::FsmTransitionTarget, super::ParseError> {
        use crate::frame_c::compiler::frame_ast::{FsmTransitionTarget, Span};
        let sp: Span = ts.cur_span();
        match ts.peek_kind() {
            FsmTokenKind::StateRef(state) => {
                ts.advance();
                Ok(FsmTransitionTarget::Static {
                    state,
                    stage: None,
                    span: sp,
                })
            }
            FsmTokenKind::StageRef { state, stage } => {
                ts.advance();
                Ok(FsmTransitionTarget::Static {
                    state,
                    stage: Some(stage),
                    span: sp,
                })
            }
            _ => Err(super::ParseError {
                message: "expected a transition target (`$State` or `$State.stage`)".to_string(),
                span: sp,
            }),
        }
    }

    /// Map a binary-operator token to its [`BinaryOp`]. Precondition:
    /// `op` is one of the tokens [`binding_power`] accepts.
    pub(super) fn binary_op(op: &FsmTokenKind) -> BinaryOp {
        match op {
            FsmTokenKind::OrOr => BinaryOp::Or,
            FsmTokenKind::AndAnd => BinaryOp::And,
            FsmTokenKind::EqEq => BinaryOp::Eq,
            FsmTokenKind::NotEq => BinaryOp::Ne,
            FsmTokenKind::Lt => BinaryOp::Lt,
            FsmTokenKind::Le => BinaryOp::Le,
            FsmTokenKind::Gt => BinaryOp::Gt,
            FsmTokenKind::Ge => BinaryOp::Ge,
            FsmTokenKind::Plus => BinaryOp::Add,
            FsmTokenKind::Minus => BinaryOp::Sub,
            FsmTokenKind::Star => BinaryOp::Mul,
            FsmTokenKind::Slash => BinaryOp::Div,
            FsmTokenKind::Percent => BinaryOp::Mod,
            _ => unreachable!("binary_op called on non-operator token"),
        }
    }
}

/// Parse one `@@fsm` declaration from a token stream.
///
/// Drives the root `FsmDeclParser` FSM to completion. Returns either the
/// parsed [`FsmDeclAst`] or the first parse error encountered.
///
/// `tokens` is the stream produced by [`lex_fsm_block`], positioned at
/// the `@@fsm` keyword.
pub fn parse_fsm_declaration(tokens: FsmTokenStream) -> Result<FsmDeclAst, ParseError> {
    let mut parser = fsm_decl_parser::FsmDeclParser::__create();
    parser.tokens = Some(tokens);
    parser.parse();
    match parser.error {
        Some(e) => Err(e),
        None => Ok(parser
            .result
            .expect("FsmDeclParser reaches $Done with result set when no error")),
    }
}

/// Convenience: lex + parse an `@@fsm` block's bytes in one call.
pub fn parse_fsm_block(bytes: &[u8]) -> Result<FsmDeclAst, ParseError> {
    let tokens = lex_fsm_block(bytes)?;
    parse_fsm_declaration(FsmTokenStream::new(tokens))
}

// Child parser FSMs (split out as fixtures grow):
// mod state_fsm     { ... include!("state_parser.gen.rs"); }
// mod match_fsm     { ... include!("match_parser.gen.rs"); }
// mod stage_fsm     { ... include!("stage_parser.gen.rs"); }
// mod action_blk_fsm{ ... include!("action_block_parser.gen.rs"); }
// mod statement_fsm { ... include!("statement_parser.gen.rs"); }
// mod expression_fsm{ ... include!("expression_parser.gen.rs"); }
// mod regex_fsm     { ... include!("regex_parser.gen.rs"); }

#[cfg(test)]
mod lexer_tests {
    use super::*;
    use token_stream::FsmTokenKind::*;

    /// Collect just the token kinds (dropping spans) for terse assertions.
    fn kinds(src: &str) -> Vec<token_stream::FsmTokenKind> {
        lex_fsm_block(src.as_bytes())
            .expect("smoke fixture must lex without error")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    /// FSM-TEST-001 smoke fixture — the minimum viable @@fsm.
    /// Pins the documented token stream from the lexer design doc.
    #[test]
    fn smoke_fixture_token_stream() {
        let got = kinds("@@fsm M(text: bytes) : bool = false { /a/ true }");
        let want = vec![
            KwFsm,
            Ident("M".to_string()),
            LParen,
            Ident("text".to_string()),
            Colon,
            Ident("bytes".to_string()),
            RParen,
            Colon,
            Ident("bool".to_string()),
            Eq,
            KwFalse,
            LBrace,
            RegexLiteral("a".to_string()),
            KwTrue,
            RBrace,
            Eof,
        ];
        assert_eq!(got, want);
    }

    /// `\/` inside a regex literal is an escaped slash, not the closing
    /// delimiter (RFC-0042 §6.2, FSM-TEST-309).
    #[test]
    fn regex_escaped_slash() {
        let got = kinds("@@fsm M(text: bytes) : bool = false { /a\\/b/ true }");
        // The regex body retains the escape verbatim; fsm_regex parses it later.
        assert!(matches!(got.get(12), Some(RegexLiteral(b)) if b == "a\\/b"));
    }

    /// Frame-level line + block comments are whitespace (RFC-0043 §3.5).
    #[test]
    fn comments_are_whitespace() {
        let got = kinds(
            "@@fsm M(text: bytes) : bool = false { // a line comment\n /a/ /* block */ true }",
        );
        let want = vec![
            KwFsm,
            Ident("M".to_string()),
            LParen,
            Ident("text".to_string()),
            Colon,
            Ident("bytes".to_string()),
            RParen,
            Colon,
            Ident("bool".to_string()),
            Eq,
            KwFalse,
            LBrace,
            RegexLiteral("a".to_string()),
            KwTrue,
            RBrace,
            Eof,
        ];
        assert_eq!(got, want);
    }

    /// An unterminated regex literal is a lexing error, not a panic.
    #[test]
    fn unterminated_regex_errors() {
        let err = lex_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a true }");
        assert!(err.is_err(), "unterminated regex must surface an error");
    }

    /// Multi-state transition fixture (FSM-TEST-400 shape): arrows, state
    /// refs, state labels, and the failure-branch colon. Exercises the
    /// element-level state/transition tokens and the bare-expression /
    /// element-level handoff (`true` / `false` bordered by `$labels`).
    #[test]
    fn states_and_transitions() {
        let got = kinds(
            "@@fsm M(text: bytes) : bool = false { /a/ -> $next : -> $error  $next: /b/ true  $error: false }",
        );
        // Body tokens only (skip the header prefix the smoke test pins).
        let body: Vec<_> = got
            .iter()
            .skip_while(|k| !matches!(k, LBrace))
            .cloned()
            .collect();
        assert_eq!(
            body,
            vec![
                LBrace,
                RegexLiteral("a".to_string()),
                Arrow,
                StateRef("next".to_string()),
                Colon,
                Arrow,
                StateRef("error".to_string()),
                StateLabel("next".to_string()),
                RegexLiteral("b".to_string()),
                KwTrue,
                StateLabel("error".to_string()),
                KwFalse,
                RBrace,
                Eof,
            ]
        );
    }

    /// Stage label `.n` and stage-capture ref `$state.n` (FSM-TEST-007 shape).
    #[test]
    fn stage_label_and_capture_ref() {
        let got = kinds("@@fsm M(text: bytes) : bytes = \"\" { $main: .x/[0-9]+/ $main.x }");
        let body: Vec<_> = got
            .iter()
            .skip_while(|k| !matches!(k, LBrace))
            .cloned()
            .collect();
        assert_eq!(
            body,
            vec![
                LBrace,
                StateLabel("main".to_string()),
                StageLabel("x".to_string()),
                RegexLiteral("[0-9]+".to_string()),
                StageRef {
                    state: "main".to_string(),
                    stage: "x".to_string()
                },
                RBrace,
                Eof,
            ]
        );
    }

    /// Embedding-action sigils on a stage (RFC-0042 §3.5.5). Each sigil is
    /// its own token; the following `{` opens a block (LBrace). Longest-
    /// match: `@eof{` is EmbedEof, not EmbedAccept.
    #[test]
    fn embedding_action_sigils() {
        let got = kinds(
            "@@fsm M(text: bytes) : int = 0 { /[0-9]+/ >{ self.a = 1 } @{ self.b = 2 } ${ self.c = 3 } %{ self.d = 4 } @eof{ self.e = 5 } self.a }",
        );
        // Pull out just the embedding-op tokens in order.
        let ops: Vec<_> = got
            .iter()
            .filter(|k| {
                matches!(
                    k,
                    EmbedStart | EmbedAccept | EmbedEvery | EmbedLeave | EmbedEof
                )
            })
            .cloned()
            .collect();
        assert_eq!(
            ops,
            vec![EmbedStart, EmbedAccept, EmbedEvery, EmbedLeave, EmbedEof]
        );
        // Each sigil is followed by an LBrace; plus the fsm body's own `{`.
        let n_lbrace = got.iter().filter(|k| matches!(k, LBrace)).count();
        assert_eq!(n_lbrace, 6); // 1 body brace + 5 embed-block braces
    }

    /// Action block as a match element, followed by another stage — proves
    /// the lexer enters block mode on `{`, tracks brace depth (including the
    /// nested `if { }` block), and returns to element level after the block
    /// so the trailing `/c/` lexes as a regex (not division).
    #[test]
    fn action_block_then_stage() {
        let got = kinds(
            "@@fsm M(text: bytes) : int = 0 { /a/ { self.x = 1  if self.x > 0 { self.y = 2 } } /c/ self.y }",
        );
        let body: Vec<_> = got
            .iter()
            .skip_while(|k| !matches!(k, RegexLiteral(_)))
            .cloned()
            .collect();
        assert_eq!(
            body,
            vec![
                RegexLiteral("a".to_string()),
                LBrace,
                Ident("self".to_string()),
                Dot,
                Ident("x".to_string()),
                Eq,
                IntLit(1),
                KwIf,
                Ident("self".to_string()),
                Dot,
                Ident("x".to_string()),
                Gt,
                IntLit(0),
                LBrace,
                Ident("self".to_string()),
                Dot,
                Ident("y".to_string()),
                Eq,
                IntLit(2),
                RBrace,
                RBrace,
                // back at element level — `/c/` is a regex, not division:
                RegexLiteral("c".to_string()),
                Ident("self".to_string()),
                Dot,
                Ident("y".to_string()),
                RBrace,
                Eof,
            ]
        );
    }

    /// FSM-TEST-004 body: a regex stage followed by a call bare-expression
    /// with a `@@:` probe argument. Exercises $ExprLevel (probe, call,
    /// parens) and the regex-at-element-level / call-at-expr-level split.
    #[test]
    fn call_with_probe_arg() {
        let got = kinds("@@fsm M(text: bytes) : int = 0 { /[0-9]/ to_int(@@:matched) }");
        let want = vec![
            KwFsm,
            Ident("M".to_string()),
            LParen,
            Ident("text".to_string()),
            Colon,
            Ident("bytes".to_string()),
            RParen,
            Colon,
            Ident("int".to_string()),
            Eq,
            IntLit(0),
            LBrace,
            RegexLiteral("[0-9]".to_string()),
            Ident("to_int".to_string()),
            LParen,
            Probe("matched".to_string()),
            RParen,
            RBrace,
            Eof,
        ];
        assert_eq!(got, want);
    }
}

#[cfg(test)]
mod parser_tests {
    use super::*;
    use crate::frame_c::compiler::frame_ast::{Expression, Literal, MatchElement, Type};

    /// FSM-TEST-001 smoke fixture parses into a complete FsmDeclAst.
    #[test]
    fn smoke_fixture_parses() {
        let ast = parse_fsm_block(b"@@fsm M(text: bytes) : bool = false { /a/ true }")
            .expect("smoke fixture must parse");

        assert_eq!(ast.name, "M");
        assert_eq!(ast.default_expr, "false");
        assert!(matches!(ast.return_type, Type::Custom(ref t) if t == "bool"));

        // One parameter: text: bytes.
        assert_eq!(ast.params.len(), 1);
        assert_eq!(ast.params[0].name, "text");
        assert!(matches!(ast.params[0].param_type, Type::Custom(ref t) if t == "bytes"));

        // One implicit (unlabeled) state, one match, two elements.
        assert_eq!(ast.states.len(), 1);
        assert!(ast.states[0].label.is_none());
        assert_eq!(ast.states[0].matches.len(), 1);
        let elems = &ast.states[0].matches[0].elements;
        assert_eq!(elems.len(), 2);

        // Element 0: stage /a/.
        match &elems[0] {
            MatchElement::Stage(s) => {
                assert!(s.label.is_none());
                assert_eq!(s.regex, "a");
                assert!(s.embedding_actions.is_empty());
            }
            other => panic!("expected Stage, got {:?}", other),
        }

        // Element 1: bare expression `true`.
        match &elems[1] {
            MatchElement::BareExpression { expr, .. } => {
                assert!(matches!(expr, Expression::Literal(Literal::Bool(true))));
            }
            other => panic!("expected BareExpression, got {:?}", other),
        }

        // No actions / domain blocks.
        assert!(ast.actions.is_none());
        assert!(ast.domain.is_none());
    }

    /// Two parameters, int default — exercises the param loop and a
    /// non-bool default token.
    #[test]
    fn two_params_int_default() {
        let ast = parse_fsm_block(b"@@fsm Counter(text: bytes, n: int = 0) : int = 0 { /x/ n }")
            .expect("must parse");
        assert_eq!(ast.name, "Counter");
        assert_eq!(ast.params.len(), 2);
        assert_eq!(ast.params[1].name, "n");
        assert_eq!(ast.params[1].default.as_deref(), Some("0"));
        assert_eq!(ast.default_expr, "0");
    }

    /// FSM-TEST-030 shape: an action block with two `;`-separated
    /// assignments. Proves ActionBlockParser + StatementParser parse the
    /// block into MatchElement::ActionBlock with Assign statements.
    #[test]
    fn action_block_assignments() {
        use crate::frame_c::compiler::frame_ast::Statement;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1; self.flag = true } self.count }",
        )
        .expect("action-block fixture must parse");

        let elems = &ast.states[0].matches[0].elements;
        // [ Stage(/[0-9]/), ActionBlock{2 stmts}, BareExpr(self.count) ]
        assert_eq!(elems.len(), 3);
        match &elems[1] {
            MatchElement::ActionBlock(block) => {
                assert_eq!(block.statements.len(), 2);
                // Both statements are assignments (Expression::Assign).
                for s in &block.statements {
                    match s {
                        Statement::Expression(e) => {
                            assert!(matches!(e.expr, Expression::Assign { .. }));
                        }
                        other => panic!("expected assignment statement, got {:?}", other),
                    }
                }
            }
            other => panic!("expected ActionBlock element, got {:?}", other),
        }
    }

    /// FSM-TEST-032 shape: an `if/else` statement inside an action block,
    /// with a relational condition and assignment branches.
    #[test]
    fn action_block_if_else() {
        use crate::frame_c::compiler::frame_ast::{BinaryOp, Statement};
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { if to_int(@@:matched) > 5 { self.flag = true } else { self.flag = false } } self.flag }",
        )
        .expect("if/else fixture must parse");

        let elems = &ast.states[0].matches[0].elements;
        match &elems[1] {
            MatchElement::ActionBlock(block) => {
                assert_eq!(block.statements.len(), 1);
                match &block.statements[0] {
                    Statement::If(if_ast) => {
                        // condition is `... > 5`
                        assert!(matches!(
                            if_ast.condition,
                            Expression::Binary {
                                op: BinaryOp::Gt,
                                ..
                            }
                        ));
                        // then-branch and else-branch are blocks
                        assert!(matches!(*if_ast.then_branch, Statement::Block(_)));
                        match &if_ast.else_branch {
                            Some(b) => assert!(matches!(**b, Statement::Block(_))),
                            None => panic!("expected an else branch"),
                        }
                    }
                    other => panic!("expected If statement, got {:?}", other),
                }
            }
            other => panic!("expected ActionBlock element, got {:?}", other),
        }
    }

    /// FSM-TEST-121 shape: a bare call statement inside an action block.
    #[test]
    fn action_block_call_statement() {
        use crate::frame_c::compiler::frame_ast::Statement;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { increment() } self.count }",
        )
        .expect("call-statement fixture must parse");
        let elems = &ast.states[0].matches[0].elements;
        match &elems[1] {
            MatchElement::ActionBlock(block) => {
                assert_eq!(block.statements.len(), 1);
                match &block.statements[0] {
                    Statement::Expression(e) => {
                        assert!(matches!(e.expr, Expression::Call { .. }));
                    }
                    other => panic!("expected call statement, got {:?}", other),
                }
            }
            other => panic!("expected ActionBlock element, got {:?}", other),
        }
    }

    /// `else if` chain nests as an If in the else branch.
    #[test]
    fn action_block_else_if_chain() {
        use crate::frame_c::compiler::frame_ast::Statement;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /x/ { if a { self.x = 1 } else if b { self.x = 2 } else { self.x = 3 } } self.x }",
        )
        .expect("else-if fixture must parse");
        let elems = &ast.states[0].matches[0].elements;
        match &elems[1] {
            MatchElement::ActionBlock(block) => match &block.statements[0] {
                Statement::If(outer) => {
                    // else branch is itself an If (the `else if`).
                    match &outer.else_branch {
                        Some(b) => assert!(matches!(**b, Statement::If(_))),
                        None => panic!("expected else-if"),
                    }
                }
                other => panic!("expected If, got {:?}", other),
            },
            other => panic!("expected ActionBlock, got {:?}", other),
        }
    }

    /// `domain:` section with two fields, parsed into FsmDomainBlock with
    /// parsed default expressions (RFC-0042 §3.8).
    #[test]
    fn domain_section() {
        use crate::frame_c::compiler::frame_ast::Type;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ self.count  domain: count: int = 0  flag: bool = false }",
        )
        .expect("domain fixture must parse");

        let domain = ast.domain.as_ref().expect("domain block present");
        assert_eq!(domain.vars.len(), 2);

        assert_eq!(domain.vars[0].name, "count");
        assert!(matches!(domain.vars[0].var_type, Type::Custom(ref t) if t == "int"));
        assert!(matches!(
            domain.vars[0].default,
            Expression::Literal(Literal::Int(0))
        ));

        assert_eq!(domain.vars[1].name, "flag");
        assert!(matches!(domain.vars[1].var_type, Type::Custom(ref t) if t == "bool"));
        assert!(matches!(
            domain.vars[1].default,
            Expression::Literal(Literal::Bool(false))
        ));
    }

    /// A domain default may itself be an expression (FSM-TEST-011 shape:
    /// `initial: int = initial * 2`).
    #[test]
    fn domain_default_expression() {
        use crate::frame_c::compiler::frame_ast::BinaryOp;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes, initial: int = 0) : int = 0 { /[0-9]/ self.count  domain: count: int = initial * 2 }",
        )
        .expect("must parse");
        let d = ast.domain.as_ref().unwrap();
        assert_eq!(d.vars.len(), 1);
        assert!(matches!(
            d.vars[0].default,
            Expression::Binary {
                op: BinaryOp::Mul,
                ..
            }
        ));
    }

    /// FSM-TEST-030 end to end: action block (assignments) + bare expr +
    /// domain section — the full fixture parses into one coherent AST.
    #[test]
    fn fsm_test_030_full() {
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1; self.flag = true } self.count  domain: count: int = 0  flag: bool = false }",
        )
        .expect("FSM-TEST-030 must parse");
        // Implicit start state: stage, action block (2 stmts), bare expr.
        assert_eq!(ast.states.len(), 1);
        assert_eq!(ast.states[0].matches[0].elements.len(), 3);
        // Domain: count, flag.
        assert_eq!(ast.domain.as_ref().unwrap().vars.len(), 2);
    }

    /// `actions:` section: a declared helper with a typed param and a
    /// return type, body via ActionBlockParser (RFC-0042 §3.7).
    #[test]
    fn actions_section() {
        use crate::frame_c::compiler::frame_ast::{Statement, Type};
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]+/ parse_int(@@:matched)  actions: parse_int(s: bytes): int { to_int(s) } }",
        )
        .expect("actions fixture must parse");

        let actions = ast.actions.as_ref().expect("actions block present");
        assert_eq!(actions.actions.len(), 1);
        let a = &actions.actions[0];
        assert_eq!(a.name, "parse_int");
        assert_eq!(a.params.len(), 1);
        assert_eq!(a.params[0].name, "s");
        assert!(matches!(a.params[0].param_type, Type::Custom(ref t) if t == "bytes"));
        assert!(matches!(a.return_type, Some(Type::Custom(ref t)) if t == "int"));
        // body: one expression statement `to_int(s)`
        assert_eq!(a.body.statements.len(), 1);
        assert!(matches!(&a.body.statements[0], Statement::Expression(_)));
    }

    /// Both sections present, canonical order states → actions → domain.
    /// An action with no params + a no-return action + two domain fields.
    #[test]
    fn actions_and_domain_sections() {
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { tally() } self.count  actions: tally() { self.count = self.count + 1 }  helper(): int { 42 }  domain: count: int = 0  flag: bool = false }",
        )
        .expect("actions+domain fixture must parse");

        let actions = ast.actions.as_ref().expect("actions present");
        assert_eq!(actions.actions.len(), 2);
        assert_eq!(actions.actions[0].name, "tally");
        assert_eq!(actions.actions[0].params.len(), 0);
        assert!(actions.actions[0].return_type.is_none());
        assert_eq!(actions.actions[1].name, "helper");
        assert!(actions.actions[1].return_type.is_some());

        let domain = ast.domain.as_ref().expect("domain present");
        assert_eq!(domain.vars.len(), 2);
    }

    /// Embedding actions attach to a stage (RFC-0042 §3.5.5). All five
    /// operator kinds on one stage, each with an assignment body.
    #[test]
    fn embedding_actions_on_stage() {
        use crate::frame_c::compiler::frame_ast::EmbeddingOp;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]+/ >{ self.a = 1 } @{ self.b = 2 } ${ self.c = 3 } %{ self.d = 4 } @eof{ self.e = 5 } self.a  domain: a: int = 0  b: int = 0  c: int = 0  d: int = 0  e: int = 0 }",
        )
        .expect("embedding-actions fixture must parse");

        let elems = &ast.states[0].matches[0].elements;
        // First element is the stage carrying all five embedding actions.
        match &elems[0] {
            MatchElement::Stage(s) => {
                assert_eq!(s.regex, "[0-9]+");
                let ops: Vec<EmbeddingOp> = s.embedding_actions.iter().map(|e| e.op).collect();
                assert_eq!(
                    ops,
                    vec![
                        EmbeddingOp::Start,
                        EmbeddingOp::Accept,
                        EmbeddingOp::EveryTransition,
                        EmbeddingOp::LeaveAccept,
                        EmbeddingOp::Eof,
                    ]
                );
                // Each embedding body is a one-statement block.
                for e in &s.embedding_actions {
                    assert_eq!(e.body.statements.len(), 1);
                }
            }
            other => panic!("expected Stage with embedding actions, got {:?}", other),
        }
    }

    /// A single embedding action that calls a declared action (the
    /// composability point of §3.5.5 — embedding bodies can call actions).
    #[test]
    fn embedding_action_calls_action() {
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]+/ ${ tally() } self.count  actions: tally() { self.count = self.count + 1 }  domain: count: int = 0 }",
        )
        .expect("must parse");
        match &ast.states[0].matches[0].elements[0] {
            MatchElement::Stage(s) => {
                assert_eq!(s.embedding_actions.len(), 1);
                assert_eq!(s.embedding_actions[0].body.statements.len(), 1);
            }
            other => panic!("expected Stage, got {:?}", other),
        }
    }

    /// A header missing its return type is a parse error (RFC-0042
    /// E705 territory — the parser surfaces the missing `:` / type).
    #[test]
    fn missing_return_type_errors() {
        let err = parse_fsm_block(b"@@fsm M(text: bytes) = false { /a/ true }");
        assert!(err.is_err(), "missing return type must error");
    }

    /// FSM-TEST-400 shape: three states with static transitions. The
    /// implicit first state has a success + failure branch; the labeled
    /// states are terminals. Proves multi-state parsing (FsmDeclParser
    /// looping StateParser) and transition clauses.
    #[test]
    fn multi_state_with_transitions() {
        use crate::frame_c::compiler::frame_ast::FsmTransitionTarget;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : bool = false { /a/ -> $next : -> $error  $next: /b/ true  $error: false }",
        )
        .expect("multi-state fixture must parse");

        assert_eq!(ast.states.len(), 3);

        // State 0: implicit (unlabeled) start, stage /a/, success→next, fail→error.
        assert!(ast.states[0].label.is_none());
        let m0 = &ast.states[0].matches[0];
        match &m0.elements[0] {
            MatchElement::Stage(s) => assert_eq!(s.regex, "a"),
            other => panic!("expected stage /a/, got {:?}", other),
        }
        let t = m0.transition.as_ref().expect("state 0 has a transition");
        match &t.success {
            FsmTransitionTarget::Static { state, stage, .. } => {
                assert_eq!(state, "next");
                assert!(stage.is_none());
            }
            other => panic!("expected static success target, got {:?}", other),
        }
        match t.failure.as_ref().expect("state 0 has a failure branch") {
            FsmTransitionTarget::Static { state, .. } => assert_eq!(state, "error"),
            other => panic!("expected static failure target, got {:?}", other),
        }

        // State 1: `$next` — stage /b/ + bare expr `true`, no transition.
        assert_eq!(ast.states[1].label.as_deref(), Some("next"));
        assert!(ast.states[1].matches[0].transition.is_none());
        assert_eq!(ast.states[1].matches[0].elements.len(), 2);

        // State 2: `$error` — bare expr `false`.
        assert_eq!(ast.states[2].label.as_deref(), Some("error"));
        match &ast.states[2].matches[0].elements[0] {
            MatchElement::BareExpression { expr, .. } => {
                assert!(matches!(expr, Expression::Literal(Literal::Bool(false))));
            }
            other => panic!("expected bare `false`, got {:?}", other),
        }
    }

    /// Ordered-choice `|` matches within one state (RFC-0042 §3.4): a
    /// single state holding two matches. Proves StateParser's match loop.
    #[test]
    fn ordered_choice_matches() {
        let ast = parse_fsm_block(b"@@fsm M(text: bytes) : int = 0 { /a/ 1 | /b/ 2 }")
            .expect("ordered-choice fixture must parse");

        // One (implicit) state, two matches.
        assert_eq!(ast.states.len(), 1);
        let ms = &ast.states[0].matches;
        assert_eq!(ms.len(), 2);

        // Match 0: stage /a/ then bare `1`.
        match &ms[0].elements[0] {
            MatchElement::Stage(s) => assert_eq!(s.regex, "a"),
            other => panic!("expected stage /a/, got {:?}", other),
        }
        match &ms[0].elements[1] {
            MatchElement::BareExpression { expr, .. } => {
                assert!(matches!(expr, Expression::Literal(Literal::Int(1))));
            }
            other => panic!("expected bare `1`, got {:?}", other),
        }

        // Match 1: stage /b/ then bare `2`.
        match &ms[1].elements[0] {
            MatchElement::Stage(s) => assert_eq!(s.regex, "b"),
            other => panic!("expected stage /b/, got {:?}", other),
        }
        match &ms[1].elements[1] {
            MatchElement::BareExpression { expr, .. } => {
                assert!(matches!(expr, Expression::Literal(Literal::Int(2))));
            }
            other => panic!("expected bare `2`, got {:?}", other),
        }
    }

    /// `|` matches each with their own transition clause.
    #[test]
    fn ordered_choice_matches_with_transitions() {
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : bool = false { /a/ -> $x | /b/ -> $y  $x: true  $y: false }",
        )
        .expect("must parse");
        let ms = &ast.states[0].matches;
        assert_eq!(ms.len(), 2);
        assert!(ms[0].transition.is_some());
        assert!(ms[1].transition.is_some());
    }

    /// FSM-TEST-402: conditional transition target with `when` guards.
    /// `-> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error`
    /// Proves the lexer's `$ref`-inside-parens handling and StateParser's
    /// $CondTarget loop (each condition parsed by ExpressionParser).
    #[test]
    fn conditional_when_target() {
        use crate::frame_c::compiler::frame_ast::{BinaryOp, FsmTransitionTarget};
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes, mode: int) : int = 0 { /[01]/ -> ( $zero when self.mode == 0, $one when self.mode == 1 ) : -> $error  $zero: 0  $one: 1  $error: -1 }",
        )
        .expect("conditional-target fixture must parse");

        let t = ast.states[0].matches[0]
            .transition
            .as_ref()
            .expect("transition present");
        match &t.success {
            FsmTransitionTarget::Conditional(alts) => {
                assert_eq!(alts.len(), 2);
                // alt 0: $zero when self.mode == 0
                match &alts[0].target {
                    FsmTransitionTarget::Static { state, .. } => assert_eq!(state, "zero"),
                    other => panic!("expected static `$zero`, got {:?}", other),
                }
                assert!(matches!(
                    &alts[0].condition,
                    Expression::Binary {
                        op: BinaryOp::Eq,
                        ..
                    }
                ));
                match &alts[1].target {
                    FsmTransitionTarget::Static { state, .. } => assert_eq!(state, "one"),
                    other => panic!("expected static `$one`, got {:?}", other),
                }
            }
            other => panic!("expected conditional target, got {:?}", other),
        }
        // Failure branch is the static `$error`.
        match t.failure.as_ref().expect("failure branch present") {
            FsmTransitionTarget::Static { state, .. } => assert_eq!(state, "error"),
            other => panic!("expected static failure `$error`, got {:?}", other),
        }
    }

    /// A conditional alternative missing its `when` guard is an error
    /// (RFC-0042 E715, FSM-TEST-406).
    #[test]
    fn conditional_missing_when_errors() {
        let err = parse_fsm_block(
            b"@@fsm M(text: bytes, mode: int) : int = 0 { /[01]/ -> ( $zero when self.mode == 0, $one ) : -> $error  $zero: 0  $one: 1  $error: -1 }",
        );
        assert!(err.is_err(), "missing `when` guard must error (E715)");
    }

    /// Stage-capture target: `$0.start` re-entry reference (FSM-TEST-401
    /// shape). Proves StageRef transition targets parse.
    #[test]
    fn stage_ref_transition_target() {
        use crate::frame_c::compiler::frame_ast::FsmTransitionTarget;
        let ast = parse_fsm_block(
            b"@@fsm M(text: bytes) : bool = false { $other: /x/ -> $main.start : -> $err  $main: /a/ true  $err: false }",
        )
        .expect("stage-ref target fixture must parse");
        let t = ast.states[0].matches[0]
            .transition
            .as_ref()
            .expect("transition present");
        match &t.success {
            FsmTransitionTarget::Static { state, stage, .. } => {
                assert_eq!(state, "main");
                assert_eq!(stage.as_deref(), Some("start"));
            }
            other => panic!("expected `$main.start` static target, got {:?}", other),
        }
    }

    /// FSM-TEST-004: a call bare-expression with a `@@:` probe argument
    /// parses via the child ExpressionParser. Proves the cooperating-
    /// systems composition (parent FsmDeclParser shuttles the token
    /// stream into the child ExpressionParser and back).
    #[test]
    fn call_expression_parses_via_child() {
        let ast = parse_fsm_block(b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ to_int(@@:matched) }")
            .expect("FSM-TEST-004 must parse");

        let elems = &ast.states[0].matches[0].elements;
        assert_eq!(elems.len(), 2);

        // Element 0: stage /[0-9]/.
        match &elems[0] {
            MatchElement::Stage(s) => assert_eq!(s.regex, "[0-9]"),
            other => panic!("expected Stage, got {:?}", other),
        }

        // Element 1: call `to_int(@@:matched)`.
        match &elems[1] {
            MatchElement::BareExpression { expr, .. } => match expr {
                Expression::Call { func, args } => {
                    assert_eq!(func, "to_int");
                    assert_eq!(args.len(), 1);
                    // The lone argument is the probe @@:matched.
                    assert!(matches!(&args[0], Expression::Var(v) if v == "@@:matched"));
                }
                other => panic!("expected Call, got {:?}", other),
            },
            other => panic!("expected BareExpression, got {:?}", other),
        }
    }

    /// Helper: extract the single bare-expression from a one-stage,
    /// one-bare-expr body. Panics if the shape differs.
    fn bare_expr(src: &[u8]) -> Expression {
        let ast = parse_fsm_block(src).expect("must parse");
        let elems = &ast.states[0].matches[0].elements;
        match elems.last().expect("at least one element") {
            MatchElement::BareExpression { expr, .. } => expr.clone(),
            other => panic!("expected BareExpression tail, got {:?}", other),
        }
    }

    /// Precedence: `1 + 2 * 3` must nest as `1 + (2 * 3)` — multiplication
    /// binds tighter. Proves the precedence-climbing $Climb state.
    #[test]
    fn precedence_mul_binds_tighter_than_add() {
        use crate::frame_c::compiler::frame_ast::BinaryOp;
        let e = bare_expr(b"@@fsm M(text: bytes) : int = 0 { /x/ 1 + 2 * 3 }");
        // Top node is `+` with right = `2 * 3`.
        match e {
            Expression::Binary {
                left,
                op: BinaryOp::Add,
                right,
            } => {
                assert!(matches!(*left, Expression::Literal(Literal::Int(1))));
                match *right {
                    Expression::Binary {
                        left: rl,
                        op: BinaryOp::Mul,
                        right: rr,
                    } => {
                        assert!(matches!(*rl, Expression::Literal(Literal::Int(2))));
                        assert!(matches!(*rr, Expression::Literal(Literal::Int(3))));
                    }
                    other => panic!("expected `2 * 3` on the right, got {:?}", other),
                }
            }
            other => panic!("expected top-level `+`, got {:?}", other),
        }
    }

    /// Left-associativity: `1 - 2 - 3` must nest as `(1 - 2) - 3`.
    #[test]
    fn subtraction_left_associates() {
        use crate::frame_c::compiler::frame_ast::BinaryOp;
        let e = bare_expr(b"@@fsm M(text: bytes) : int = 0 { /x/ 1 - 2 - 3 }");
        match e {
            Expression::Binary {
                left,
                op: BinaryOp::Sub,
                right,
            } => {
                // Left is `(1 - 2)`, right is `3`.
                assert!(matches!(*right, Expression::Literal(Literal::Int(3))));
                assert!(matches!(
                    *left,
                    Expression::Binary {
                        op: BinaryOp::Sub,
                        ..
                    }
                ));
            }
            other => panic!("expected top-level `-`, got {:?}", other),
        }
    }

    /// Parentheses override precedence: `(1 + 2) * 3` nests as `(1+2) * 3`.
    #[test]
    fn parens_override_precedence() {
        use crate::frame_c::compiler::frame_ast::BinaryOp;
        let e = bare_expr(b"@@fsm M(text: bytes) : int = 0 { /x/ (1 + 2) * 3 }");
        match e {
            Expression::Binary {
                left,
                op: BinaryOp::Mul,
                right,
            } => {
                assert!(matches!(*right, Expression::Literal(Literal::Int(3))));
                assert!(matches!(
                    *left,
                    Expression::Binary {
                        op: BinaryOp::Add,
                        ..
                    }
                ));
            }
            other => panic!("expected top-level `*`, got {:?}", other),
        }
    }

    /// Member access: `len(self.text)` — call with a `self.text` Member arg.
    #[test]
    fn member_access_in_call_arg() {
        let e = bare_expr(b"@@fsm M(text: bytes) : int = 0 { /[0-9]+/ len(self.text) }");
        match e {
            Expression::Call { func, args } => {
                assert_eq!(func, "len");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expression::Member { object, field } => {
                        assert!(matches!(&**object, Expression::Var(v) if v == "self"));
                        assert_eq!(field, "text");
                    }
                    other => panic!("expected Member arg, got {:?}", other),
                }
            }
            other => panic!("expected Call, got {:?}", other),
        }
    }

    /// FSM-TEST-105 shape: `to_int(@@:matched) > self.threshold` —
    /// relational operator with a call LHS and a member-access RHS.
    #[test]
    fn relational_with_call_and_member() {
        use crate::frame_c::compiler::frame_ast::BinaryOp;
        let e = bare_expr(
            b"@@fsm M(text: bytes, threshold: int = 10) : bool = false { /[0-9]+/ to_int(@@:matched) > self.threshold }",
        );
        match e {
            Expression::Binary {
                left,
                op: BinaryOp::Gt,
                right,
            } => {
                assert!(matches!(*left, Expression::Call { .. }));
                match *right {
                    Expression::Member { object, field } => {
                        assert!(matches!(*object, Expression::Var(v) if v == "self"));
                        assert_eq!(field, "threshold");
                    }
                    other => panic!("expected Member RHS, got {:?}", other),
                }
            }
            other => panic!("expected top-level `>`, got {:?}", other),
        }
    }

    /// Unary not: `!self.flag` → Unary(Not, Member(self, flag)). Member
    /// binds tighter than unary, so the operand is the whole `self.flag`.
    #[test]
    fn unary_not_over_member() {
        use crate::frame_c::compiler::frame_ast::UnaryOp;
        let e = bare_expr(b"@@fsm M(text: bytes) : bool = false { /x/ !self.flag }");
        match e {
            Expression::Unary {
                op: UnaryOp::Not,
                expr,
            } => match *expr {
                Expression::Member { object, field } => {
                    assert!(matches!(*object, Expression::Var(v) if v == "self"));
                    assert_eq!(field, "flag");
                }
                other => panic!("expected Member operand, got {:?}", other),
            },
            other => panic!("expected Unary(Not), got {:?}", other),
        }
    }

    /// Unary binds tighter than binary: `-a * b` → `(-a) * b`.
    #[test]
    fn unary_neg_binds_tighter_than_mul() {
        use crate::frame_c::compiler::frame_ast::{BinaryOp, UnaryOp};
        let e = bare_expr(b"@@fsm M(text: bytes) : int = 0 { /x/ -a * b }");
        match e {
            Expression::Binary {
                left,
                op: BinaryOp::Mul,
                right,
            } => {
                assert!(matches!(
                    *left,
                    Expression::Unary {
                        op: UnaryOp::Neg,
                        ..
                    }
                ));
                assert!(matches!(*right, Expression::Var(v) if v == "b"));
            }
            other => panic!("expected top-level `*` with `(-a)` left, got {:?}", other),
        }
    }

    /// Unary composes with binary on the right: `!a && b` → `(!a) && b`.
    #[test]
    fn unary_then_binary() {
        use crate::frame_c::compiler::frame_ast::{BinaryOp, UnaryOp};
        let e = bare_expr(b"@@fsm M(text: bytes) : bool = false { /x/ !a && b }");
        match e {
            Expression::Binary {
                left,
                op: BinaryOp::And,
                right,
            } => {
                assert!(matches!(
                    *left,
                    Expression::Unary {
                        op: UnaryOp::Not,
                        ..
                    }
                ));
                assert!(matches!(*right, Expression::Var(v) if v == "b"));
            }
            other => panic!("expected top-level `&&` with `(!a)` left, got {:?}", other),
        }
    }

    /// Nested call — exercises the child-of-child recursion in the
    /// ExpressionParser tree.
    #[test]
    fn nested_call_parses() {
        let ast =
            parse_fsm_block(b"@@fsm M(text: bytes) : int = 0 { /x/ outer(inner(@@:cursor)) }")
                .expect("nested call must parse");
        let elems = &ast.states[0].matches[0].elements;
        match &elems[1] {
            MatchElement::BareExpression {
                expr: Expression::Call { func, args },
                ..
            } => {
                assert_eq!(func, "outer");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expression::Call {
                        func: inner_f,
                        args: inner_a,
                    } => {
                        assert_eq!(inner_f, "inner");
                        assert_eq!(inner_a.len(), 1);
                        assert!(matches!(&inner_a[0], Expression::Var(v) if v == "@@:cursor"));
                    }
                    other => panic!("expected inner Call, got {:?}", other),
                }
            }
            other => panic!("expected outer Call, got {:?}", other),
        }
    }
}
