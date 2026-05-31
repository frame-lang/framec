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
//! Skeleton only. The eight `.frs` files do not exist yet. This module
//! is not yet wired into [`crate::frame_c::compiler`] — adding
//! `pub mod fsm_parser;` to the parent `mod.rs` is part of driver
//! integration (Task 14 in the execution plan).
//!
//! # Public API
//!
//! Exactly one function: [`parse_fsm_declaration`]. The framec driver
//! routes `@@fsm` blocks here and consumes the returned AST.

pub mod token_stream;

use crate::frame_c::compiler::frame_ast::FsmDeclAst;
use crate::frame_c::compiler::pipeline_parser::ParseError;
use token_stream::FsmTokenStream;

/// Parse one `@@fsm` declaration from a tokenized source range.
///
/// Drives the root `FsmDeclParser` FSM to completion. Returns either
/// the parsed AST or the first parse error encountered.
///
/// `tokens` is a freshly-built stream positioned at the `@@fsm`
/// keyword. On success, the stream is consumed through the closing `}`
/// of the declaration; on error, the stream's cursor reflects the
/// failure position.
pub fn parse_fsm_declaration(
    _tokens: FsmTokenStream,
) -> Result<FsmDeclAst, ParseError> {
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
