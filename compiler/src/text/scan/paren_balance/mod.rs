//! Balanced-`()` extent recognizer, **dogfooded as an `@@[scan(u8)]` system**, STRING-AWARE.
//!
//! [`paren_balance.frs`] is a COUNTER automaton (per the journal: framec's bracket scanners
//! count openers against closers; they are not kind-matched pushdowns) that composes
//! StringScan to skip a `"`-string, so a `)` inside a string is not miscounted — the
//! string-free case of the hand `balanced`, plus string-awareness. `depth` is a domain
//! field that `scan_at` resets, so the scan is restartable.
//!
//! Drove the `scan_at` domain-state reset, and now demonstrates composition inside a counter
//! scanner (the `skip_string` leaf runs the StringScan SYSTEM over the same borrow).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit paren_balance.frs | grep -v '^#!\[allow' >
//! paren_balance.gen.rs`.

use super::string_scan;

/// Skip a `"`-string via the StringScan system (composition), returning the offset past it;
/// if there is no string at `i`, return `i` unchanged.
fn skip_string(src: &[u8], i: usize) -> usize {
    if i < src.len() && src[i] == b'"' {
        string_scan::scan(src, i).unwrap_or(i)
    } else {
        i
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
    use super::skip_string;
    include!("paren_balance.gen.rs");
}

/// From a `(` at `bytes[i]`, return the offset one past the matching `)`, or `None` if the
/// group is unbalanced before end-of-input. A `)` inside a `"`-string is skipped, not
/// counted. The machine finds the extent; this leaf only runs it.
pub fn scan(bytes: &[u8], i: usize) -> Option<usize> {
    let mut m = fsm::ParenBalance::over(bytes);
    if m.scan_at(i) {
        Some(m.cursor)
    } else {
        None
    }
}
