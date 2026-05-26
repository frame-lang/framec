//! `@@SystemName(args)` call-site scanner for native code regions — wrapper
//! for the Frame FSM in `call_site_scanner.gen.rs` (RFC-0035 Round 10).
//!
//! `scan_call_sites` lexes a native region into a [`CallToken`] stream:
//! `Literal` runs (comments, strings, plain text, unmatched `@@`) pass through
//! verbatim, `Call` carries each `@@[!]Name(args)` instantiation. The
//! assembler turns each `Call` into the target-language constructor (it holds
//! the system-params maps); the FSM is a pure lexer and never sees them.
//!
//! To regenerate after editing the `.frs` (then rename to `.gen.rs`):
//!   framec compile -l rust -o \
//!     framec/src/frame_c/compiler/call_site_scanner/ \
//!     framec/src/frame_c/compiler/call_site_scanner/call_site_scanner.frs

use crate::frame_c::visitors::TargetLanguage;

/// One lexical unit of a native code region.
#[derive(Debug, Clone)]
pub enum CallToken {
    /// Verbatim text (comments, strings, plain code, unmatched `@@`).
    Literal(String),
    /// A `@@[!]Name(args)` system instantiation to expand.
    Call {
        name: String,
        args: String,
        /// `@@!Name()` no-initialization form (RFC-0015 D7).
        no_init: bool,
    },
}

mod scanner {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    #![allow(unused_parens)]

    use super::CallToken;
    use crate::frame_c::compiler::native_region_scanner::create_skipper;
    use crate::frame_c::compiler::native_region_scanner::unified::SyntaxSkipper;
    use crate::frame_c::visitors::TargetLanguage;

    include!("call_site_scanner.gen.rs");
}

/// Lex `text` (a native code region) into its `CallToken` stream, using
/// `lang`'s `SyntaxSkipper` for comment/string/balanced-paren detection.
pub fn scan_call_sites(text: &str, lang: TargetLanguage) -> Vec<CallToken> {
    let mut fsm = scanner::CallSiteScannerFsm::__create();
    fsm.bytes = text.as_bytes().to_vec();
    fsm.end = text.len();
    fsm.skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    fsm.scan();
    std::mem::take(&mut fsm.tokens)
}
