//! Quoted-string extent recognizer, **dogfooded as an `@@[scan(u8)]` system**.
//!
//! [`string_scan.frs`] recognizes the same grammar as
//! the retired hand lexer's `quoted` with `delim = b'"'`, `escapes = true`,
//! `multiline = false`: a `"`-delimited string with `\`-escapes, unterminated at a bare
//! newline. The generated machine borrows the input (`over(&[u8])`, zero copy) and scans a
//! prefix at a moving cursor (`scan_at(i)`), leaving the extent in `cursor`.
//!
//! This is the first cleanroom scanner authored as a Frame system rather than a
//! hand-rolled byte-loop — the `@@system` analogue of the shipping `string_scan_fsm`, and
//! the resolution of the fubar recorded in `docs/JOURNAL.md`. A differential test
//! ([`super::super::super`]'s `tests/string_scan.rs`) proves it agrees with the hand lexer
//! at every position. The hand lexer stays in production until the whole family is
//! converted; this proves the self-hosting loop on a real scanner.
//!
//! `.gen.rs` regen: edit `string_scan.frs`, then
//! `framec-ng -l rust --emit string_scan.frs | grep -v '^#!\[allow' > string_scan.gen.rs`
//! (the `grep -v` drops the crate-level `#![allow]`, which is not valid inside an
//! `include!`'d module — a proper `--emit-body` flag is a follow-up). Commit `.frs` + `.gen.rs`.

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    include!("string_scan.gen.rs");
}

/// Recognize a `"`-quoted string at `bytes[i..]`. Returns the offset one past the closing
/// quote, or `None` if there is no terminated string there — the same extent the
/// retired hand lexer's `quoted` computed for `b'"'` (escapes on, single-line).
///
/// The machine finds the extent; this wrapper is the native leaf, and it does nothing but
/// run it and read `cursor` — no recognition logic lives here.
pub fn scan(bytes: &[u8], i: usize) -> Option<usize> {
    let mut m = fsm::StringScan::over(bytes);
    if m.scan_at(i) {
        Some(m.cursor)
    } else {
        None
    }
}
