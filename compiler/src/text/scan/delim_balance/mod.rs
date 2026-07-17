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

/// Does an opaque region OPEN at `i` but never close? The `fail_unterm` policy leaf: under FAIL,
/// the machine rejects here so a delimiter buried in an unterminated string/comment cannot
/// spuriously balance the group. Only runs OpaqueScan — no walk (D3).
fn opaque_unterminated(src: &[u8], i: usize, target: Target) -> bool {
    matches!(opaque_at(src, i, target), OpaqueAt::Unterminated)
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
    use super::{opaque_skip, opaque_unterminated, Target};
    include!("delim_balance.gen.rs");
}

/// Run the `DelimBalance` machine over `bytes[open..]` for the `(o, c)` pair, bounded by `limit`,
/// under the given unterminated-opaque policy. Accept → offset one past the matching close;
/// Reject → `None`. The only difference between the two public entry points is `fail_unterm`.
fn run(
    bytes: &[u8],
    open: usize,
    limit: usize,
    o: u8,
    c: u8,
    target: Target,
    fail_unterm: bool,
) -> Option<usize> {
    let mut m = fsm::DelimBalance::over(bytes, target, o, c, limit, fail_unterm);
    if m.scan_at(open) {
        Some(m.cursor)
    } else {
        None
    }
}

/// From an `open` byte at `bytes[open]`, return the offset one past the matching `close` (bounded
/// by `limit`), or `None` if it never balances. A delimiter inside an opaque region is skipped,
/// not counted. **TOLERATE policy**: an unterminated opaque region is treated as ordinary bytes
/// (the hand `machine.rs::balanced`).
pub fn balanced(bytes: &[u8], open: usize, limit: usize, o: u8, c: u8, target: Target) -> Option<usize> {
    run(bytes, open, limit, o, c, target, false)
}

/// As [`balanced`], but with the **FAIL policy**: an unterminated opaque region makes the group
/// malformed → `None`, so a delimiter buried in an unterminated string/comment can never
/// spuriously balance it (the hand `close_brace` semantics). This is how `close_brace` (Item 2's
/// BodyBalance residual) is discharged onto DelimBalance without changing `balanced`'s behavior.
pub fn balanced_strict(bytes: &[u8], open: usize, limit: usize, o: u8, c: u8, target: Target) -> Option<usize> {
    run(bytes, open, limit, o, c, target, true)
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

/// The FAIL-policy differential oracle for [`balanced_strict`] — the hand analogue that rejects
/// on an unterminated opaque region. Fully independent of the system under test: the
/// unterminated check uses the hand `opaque_at_hand` and the skip uses the hand
/// `skip_opaque_hand` (both the retired hand `Lexer`), never OpaqueScan. `#[doc(hidden)]`,
/// test-only, deleted at C-final with the other oracles.
#[doc(hidden)]
pub fn balanced_strict_hand(
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
        // FAIL: an opaque region that opens here but never closes makes the group malformed.
        if matches!(super::opaque_at_hand(bytes, i, target), OpaqueAt::Unterminated) {
            return None;
        }
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
