//! Erlang paren-balance lexer — wrapper for the Frame FSM in
//! `paren_balance_scanner.gen.rs` (RFC-0035 Round 12).
//!
//! `is_unclosed` reports whether a processed Erlang line has more open
//! brackets than closes (i.e. it continues on the next line), tracking
//! string/atom/escape lexical modes as FSM states. Backs
//! `erlang_system::lexical::paren_balance_unclosed`.
//!
//! To regenerate after editing the `.frs` (then rename to `.gen.rs`):
//!   framec compile -l rust -o \
//!     framec/src/frame_c/compiler/paren_balance_scanner/ \
//!     framec/src/frame_c/compiler/paren_balance_scanner/paren_balance_scanner.frs

mod scanner {
    #![allow(unreachable_patterns)]
    #![allow(unused_mut)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(unused_variables)]
    #![allow(unused_parens)]

    include!("paren_balance_scanner.gen.rs");
}

/// True if `line` has more open brackets than closes (excluding string/atom
/// literals and `%` comments) — i.e. the line is mid-expression.
pub fn is_unclosed(line: &str) -> bool {
    let mut fsm = scanner::ParenBalanceFsm::__create();
    fsm.bytes = line.as_bytes().to_vec();
    fsm.end = line.len();
    fsm.scan();
    fsm.depth > 0
}
