//! **Composition proof: a scan system that composes a sub-scanner.**
//!
//! [`string_counter.frs`] walks the whole input and skips each `"`-string by calling the
//! native leaf [`skip_string`], which runs the **StringScan system** over the SAME borrowed
//! `&[u8]` (no buffer copy — just a shared re-borrow). framec owns the walk (the states +
//! the step loop); the leaf does the transformation (invoke the sub-scanner). This is the
//! exact mechanism the `Segmenter` needs to skip opaque regions mid-walk, proven small
//! before it is used large.
//!
//! The wiring is the shipping pattern: the generated handlers call leaves **unqualified**,
//! and `mod fsm` brings them into scope with `use super::*`.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit string_counter.frs | grep -v '^#!\[allow' >
//! string_counter.gen.rs`.

use super::string_scan;

/// The native leaf: skip a `"`-string by running the StringScan system, returning the
/// offset one past it. If there is no terminated string at `i` (there always is — the caller
/// only calls this on a `"`), advance one byte so the walk cannot stall.
fn skip_string(src: &[u8], i: usize) -> usize {
    string_scan::scan(src, i).unwrap_or(i + 1)
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
    include!("string_counter.gen.rs");
}

/// Count the `"`-strings in `bytes` — driving the walk system, which composes StringScan to
/// skip over each string's interior (so a `"` inside a string is not counted twice).
pub fn count(bytes: &[u8]) -> i32 {
    let mut m = fsm::StringCounter::over(bytes);
    m.scan_at(0);
    m.count
}
