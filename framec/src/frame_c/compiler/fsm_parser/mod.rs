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
//! Lexer landed: [`fsm_lexer.frs`] tokenizes an `@@fsm` block into a
//! [`token_stream::FsmToken`] stream via [`lex_fsm_block`]. The parser
//! FSMs (`fsm_decl_parser.frs` and its children) are not yet written;
//! [`parse_fsm_declaration`] is `unimplemented!()` pending Tasks 12–13.
//! The module is wired into [`crate::frame_c::compiler`] but the framec
//! driver does not yet route `@@fsm` blocks here (Task 14).
//!
//! # Public API
//!
//! - [`lex_fsm_block`] — bytes → token stream (working).
//! - [`parse_fsm_declaration`] — token stream → AST (pending).

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

    use super::lex_helpers::{push1, skip_ws_comments};
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

/// Parse one `@@fsm` declaration from a tokenized source range.
///
/// Drives the root `FsmDeclParser` FSM to completion. Returns either
/// the parsed AST or the first parse error encountered.
///
/// `tokens` is a freshly-built stream positioned at the `@@fsm`
/// keyword. On success, the stream is consumed through the closing `}`
/// of the declaration; on error, the stream's cursor reflects the
/// failure position.
pub fn parse_fsm_declaration(_tokens: FsmTokenStream) -> Result<FsmDeclAst, ParseError> {
    // Implementation lands in Task 13 (Implement composition parsers).
    // At that point the body becomes:
    //
    //     let mut parser = root_fsm::FsmDeclParser::__create();
    //     parser.tokens = Some(tokens);
    //     parser.parse();
    //     match parser.error {
    //         Some(e) => Err(e),
    //         None => Ok(parser.result.expect("must succeed if no error")),
    //     }
    //
    // and the corresponding `mod root_fsm { include!("fsm_decl_parser.gen.rs"); }`
    // module gets uncommented below.
    unimplemented!("fsm_parser not yet implemented; see _scratch/rfc_0043_parser_design.md")
}

// Generated FSM modules (commented out until each .frs lands):
//
// mod root_fsm {
//     #![allow(unreachable_patterns, unused_mut, dead_code, non_snake_case,
//              unused_variables, unused_parens)]
//     use super::*;
//     include!("fsm_decl_parser.gen.rs");
// }
//
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
}
