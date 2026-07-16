//! The statement-level classifier, **dogfooded as an `@@[scan(u8)]` system** — the `@@system`
//! analogue of the hand [`super::machine`]`::frame_stmt`'s dispatch.
//!
//! [`stmt_scan.frs`] classifies the Frame statement construct at the start of a statement
//! (`push$` / `(exit)->` / `->` / `=>`), owning the DISPATCH as a state per leading token; the
//! leaves reuse the exact hand sub-logic (`stmt_eol`, `stmt_balanced_close`, `arrow_has_target`)
//! so there is no drift. A differential test proves the (kind, end) matches the production
//! `frame_stmt_classify` at every statement start.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit stmt_scan.frs | grep -v '^#!\[allow' >
//! stmt_scan.gen.rs`.

use super::machine::{arrow_has_target, stmt_balanced_close, stmt_eol};

fn starts_with(src: &[u8], i: usize, pat: &[u8]) -> bool {
    i + pat.len() <= src.len() && &src[i..i + pat.len()] == pat
}
fn starts_push(src: &[u8], i: usize) -> bool {
    starts_with(src, i, b"push$")
}
fn starts_pop(src: &[u8], i: usize) -> bool {
    starts_with(src, i, b"pop$")
}
fn is_open_paren(src: &[u8], i: usize) -> bool {
    i < src.len() && src[i] == b'('
}
fn starts_arrow(src: &[u8], i: usize) -> bool {
    starts_with(src, i, b"->")
}
fn starts_fatarrow(src: &[u8], i: usize) -> bool {
    starts_with(src, i, b"=>")
}
fn skip_ws(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i] == b' ' || src[i] == b'\t') {
        i += 1;
    }
    i
}
/// `pop$` anywhere in `[from, to)` — the pop marker (matches the hand `window`).
fn has_pop(src: &[u8], from: usize, to: usize) -> bool {
    let needle = b"pop$";
    let hay = &src[from..to.min(src.len())];
    hay.windows(needle.len()).any(|w| w == needle)
}
fn eol(src: &[u8], i: usize) -> usize {
    stmt_eol(src, i, src.len())
}
fn balanced_close(src: &[u8], i: usize) -> usize {
    stmt_balanced_close(src, i, src.len())
}
fn arrow_target(src: &[u8], from: usize, to: usize) -> bool {
    arrow_has_target(src, from, to)
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
    use super::{
        arrow_target, balanced_close, eol, has_pop, is_open_paren, skip_ws, starts_arrow,
        starts_fatarrow, starts_pop, starts_push,
    };
    include!("stmt_scan.gen.rs");
}

/// Classify the Frame statement at `bytes[i..limit]` as `(kind, end)`, driven by the system.
/// kind: 0=none 1=Transition 2=StackPush 3=StackPop 4=Forward.
pub fn classify(bytes: &[u8], i: usize, limit: usize) -> (i32, usize) {
    let mut m = fsm::StmtScan::over(&bytes[..limit]);
    if m.scan_at(i) {
        (m.kind, m.end_out)
    } else {
        (0, i)
    }
}
