//! Balanced-`()` extent recognizer, **dogfooded as an `@@[scan(u8)]` system**.
//!
//! [`paren_balance.frs`] is a COUNTER automaton (per the journal: framec's bracket scanners
//! count openers against closers; they are not kind-matched pushdowns). From a `(` at the
//! cursor it finds the matching `)`, leaving `cursor` one past it — the string-free case of
//! the hand `balanced`. The `depth` counter is a domain field that `scan_at` resets, so the
//! scan is restartable.
//!
//! The second cleanroom scanner authored as a Frame system. It surfaced (and drove the fix
//! for) `scan_at` domain-state reset, so counter scanners restart cleanly.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit paren_balance.frs | grep -v '^#!\[allow' >
//! paren_balance.gen.rs`.

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    include!("paren_balance.gen.rs");
}

/// From a `(` at `bytes[i]`, return the offset one past the matching `)`, or `None` if the
/// group is unbalanced before end-of-input. String/comment skipping is a follow-up (this is
/// the string-free case); the machine finds the extent, this leaf only runs it.
pub fn scan(bytes: &[u8], i: usize) -> Option<usize> {
    let mut m = fsm::ParenBalance::over(bytes);
    if m.scan_at(i) {
        Some(m.cursor)
    } else {
        None
    }
}
