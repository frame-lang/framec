//! The item-level segmenter walk, **dogfooded as an `@@[scan(u8)]` system** — the first
//! load-bearing compiler scanner authored as a Frame machine (the fubar in
//! `docs/JOURNAL.md`, being undone).
//!
//! [`segmenter.frs`] owns the WALK: two states, `$Sol` (at start of line) and `$Mid`
//! (mid-line), and the step loop over the borrowed `&[u8]`. It finds the top-level
//! `@@`-item START offsets, skipping strings/comments (so a `@@` inside them is not an item)
//! and skipping each `@@system`/`@@fsm` body (so a `@@:self` in a handler is not a top-level
//! item). The `target` is CONSTRUCTION CONFIG — the per-target lexical forms — so the walk
//! is correct for ANY target, unlike the string-blind hand loop.
//!
//! The leaves do only transformation: `item_end_at`/`skip_opaque_at` reuse the lexer
//! ([`super::item_end_at`], [`super::skip_opaque_at`]); `at_pragma`/`record` are trivial. A
//! differential test proves the offsets match the hand `segment` on real `.frm` input.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit segmenter.frs | grep -v '^#!\[allow' >
//! segmenter.gen.rs`.

use super::literals::Target;
use super::{item_end_at, skip_opaque_at};

/// A top-level `@@` starts here?
fn at_pragma(src: &[u8], i: usize) -> bool {
    i + 1 < src.len() && src[i] == b'@' && src[i + 1] == b'@'
}

/// Record an item-start offset. (A leaf so the machine body stays free of `Vec` mechanics.)
fn record(v: &mut Vec<usize>, x: usize) {
    v.push(x);
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
    use super::{at_pragma, item_end_at, record, skip_opaque_at, Target};
    include!("segmenter.gen.rs");
}

/// The start offsets of the top-level `@@` items in `bytes`, target-aware — the walk driven
/// by the Segmenter system.
pub fn item_starts(bytes: &[u8], target: Target) -> Vec<usize> {
    let mut m = fsm::Segmenter::over(bytes, target);
    m.scan_at(0);
    m.starts
}
