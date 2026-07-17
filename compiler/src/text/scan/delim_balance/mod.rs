//! Opaque-aware balanced-delimiter extent recognizer, **dogfooded as an `@@[scan(u8)]` counter
//! automaton** ([`delim_balance.frs`]) — the `@@system` that replaces the hand
//! `machine.rs::balanced`/`matching_brace` and (Item 2's deferral) `close_brace`'s `{}` counter.
//!
//! From an opener at `i` it finds the matching closer (bounded by `limit`), skipping any
//! delimiter that lives inside a comment/string/char/raw/triple literal via [`opaque_skip`],
//! which composes the OpaqueScan system. A COUNTER (single `depth`, one pair — Dyck-1), not a
//! kind-matched pushdown; STRONGER than [`super::paren_balance`] (which skips only `"`).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit delim_balance.frs | grep -v '^#!\[allow' >
//! delim_balance.gen.rs`.

use super::literals::Target;
use super::opaque_scan::{opaque_at, OpaqueAt};

/// Skip a whole opaque region (comment/literal) at `i`, returning the offset past it — or `i`
/// unchanged if nothing opaque opens here. This is the grammar's kind-aware limit policy (the
/// same one `machine.rs::skip_opaque` applies): a COMMENT clamps to `limit`, a LITERAL that
/// overruns `limit` is not consumed (returns `i`, so the byte is counted normally), an
/// unterminated body is likewise not consumed. The walk stays entirely in OpaqueScan (D3); this
/// leaf only runs it and applies the O(1) policy.
fn opaque_skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    match opaque_at(src, i, target) {
        OpaqueAt::Comment(end) => end.min(limit).max(i + 1),
        OpaqueAt::Literal(end) => {
            if end <= limit {
                end.max(i + 1)
            } else {
                i
            }
        }
        OpaqueAt::None | OpaqueAt::Unterminated => i,
    }
}

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    use super::{opaque_skip, Target};
    include!("delim_balance.gen.rs");
}

/// From an `open` byte at `bytes[open]`, return the offset one past the matching `close` (bounded
/// by `limit`), or `None` if it never balances. A delimiter inside an opaque region is skipped,
/// not counted. The machine finds the extent; this wrapper only runs it.
pub fn balanced(
    bytes: &[u8],
    open: usize,
    limit: usize,
    o: u8,
    c: u8,
    target: Target,
) -> Option<usize> {
    let mut m = fsm::DelimBalance::over(bytes, target, o, c, limit);
    if m.scan_at(open) {
        Some(m.cursor)
    } else {
        None
    }
}

/// The retired hand implementation — kept ONLY as the `balanced` differential-test oracle until
/// the parity is locked and the hand lexer recognition is deleted (Item 4). Self-contained
/// (builds its own `Lexer` for the opaque skip); not used in production. This is exactly the
/// pre-conversion `machine.rs::balanced`, using the hand `skip_opaque_hand`.
#[doc(hidden)]
pub fn balanced_hand(
    bytes: &[u8],
    open: usize,
    limit: usize,
    o: u8,
    c: u8,
    target: Target,
) -> Option<usize> {
    let mut i = open;
    let mut depth = 0i32;
    while i < limit {
        if let Some(next) = super::machine::skip_opaque_hand(bytes, i, limit, target) {
            i = next;
            continue;
        }
        if bytes[i] == o {
            depth += 1;
        } else if bytes[i] == c {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}
