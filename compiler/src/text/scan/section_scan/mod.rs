//! The section backbone, **dogfooded as an `@@[scan(u8)]` system** — the `@@system` analogue
//! of the shipping SystemBackbone's section dispatch, and the first GRAMMAR backbone.
//!
//! [`section_scan.frs`] walks a system body finding the section keywords at brace depth 0,
//! skipping strings/comments and nested braces. A differential test proves the
//! `(kw_start, kw_end, idx)` it finds matches [`super::sections`]`::section_keyword_starts`
//! (the very function production uses) at every input.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit section_scan.frs | grep -v '^#!\[allow' >
//! section_scan.gen.rs`.

use super::literals::Target;
use super::skip_opaque_at;

const KEYWORDS: &[&str] = &["interface", "machine", "domain", "actions", "operations"];

/// Opaque-skip leaf — the same the Segmenter uses.
fn skip_opaque(src: &[u8], i: usize, target: Target) -> usize {
    skip_opaque_at(src, i, target)
}

fn is_word_start(src: &[u8], i: usize) -> bool {
    if i > 0 && (src[i - 1].is_ascii_alphanumeric() || src[i - 1] == b'_') {
        return false;
    }
    src.get(i).map(|b| b.is_ascii_alphabetic()).unwrap_or(false)
}

/// If a section keyword (followed by `:`) starts a whole word at `j`, record
/// `(kw_start, kw_end, idx)`. The keyword-recognition leaf; the system decides WHEN to call
/// it (at depth 0), matching the hand `section_keyword_starts`.
fn record_kw(starts: &mut Vec<(usize, usize, usize)>, src: &[u8], j: usize) {
    if !is_word_start(src, j) {
        return;
    }
    for (idx, kw) in KEYWORDS.iter().enumerate() {
        let k = kw.as_bytes();
        if src.len() >= j + k.len() && &src[j..j + k.len()] == k {
            let mut p = j + k.len();
            while p < src.len() && (src[p] == b' ' || src[p] == b'\t') {
                p += 1;
            }
            if p < src.len() && src[p] == b':' {
                starts.push((j, p + 1, idx));
            }
        }
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
    use super::{record_kw, skip_opaque, Target};
    include!("section_scan.gen.rs");
}

/// The section-keyword starts in `bytes[body_start..close_start]`, driven by the system.
pub fn keyword_starts(
    bytes: &[u8],
    body_start: usize,
    close_start: usize,
    target: Target,
) -> Vec<(usize, usize, usize)> {
    // Scan `bytes[..close_start]` from `body_start`: the real prefix is kept (so
    // `is_word_start` sees the true previous byte), and `fsm_len == close_start` bounds the
    // walk at the closing brace.
    let mut m = fsm::SectionScan::over(&bytes[..close_start], target);
    m.scan_at(body_start);
    m.starts
}
