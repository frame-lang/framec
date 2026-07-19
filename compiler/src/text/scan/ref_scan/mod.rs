//! Frame-reference recognizer, **dogfooded as an `@@[scan(u8)]` system** — the `@@system`
//! analogue of the hand recognizer (now `frame_ref_at_hand`, oracle-only since Item 4
//! Commit C routed its last production seat, the statement scanner's assign-LHS, here).
//!
//! [`ref_scan.frs`] recognizes `$.name` (a state var) or `@@:word` (a context ref) at the
//! cursor, classifying the kind. framec owns the shape recognition; the leaves here are
//! trivial byte predicates and the kind lookup. A differential test proves the (kind, name,
//! end) it yields matches `frame_ref_at_hand` at every position.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit ref_scan.frs | grep -v '^#!\[allow' >
//! ref_scan.gen.rs`.

use crate::tree::body::RefKind;

fn starts_statevar(src: &[u8], i: usize) -> bool {
    i + 1 < src.len() && src[i] == b'$' && src[i + 1] == b'.'
}
fn starts_context(src: &[u8], i: usize) -> bool {
    i + 2 < src.len() && src[i] == b'@' && src[i + 1] == b'@' && src[i + 2] == b':'
}
fn is_ident_at(src: &[u8], i: usize) -> bool {
    i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_')
}
fn is_ident_or_dot_at(src: &[u8], i: usize) -> bool {
    i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_' || src[i] == b'.')
}

/// Classify a `@@:word` context ref by its leading word — the same ladder as the hand
/// `frame_ref_at_hand`: `self`/`data`/`params`/`return`/`event`/`system`, else ContextSelf
/// (T-R1, carried) — by `starts_with`, not segment-match (T-R2, carried; fix Δ5).
fn classify_context(src: &[u8], ws: usize, we: usize) -> i32 {
    let word = &src[ws..we];
    if word.starts_with(b"self") {
        2
    } else if word.starts_with(b"data") {
        3
    } else if word.starts_with(b"params") {
        4
    } else if word.starts_with(b"return") {
        5
    } else if word.starts_with(b"event") {
        6
    } else if word.starts_with(b"system") {
        7
    } else {
        2
    }
}

/// The name of a context ref begins after the first `.` in the word (`self.factor` -> at
/// `factor`), or is the whole word if there is no dot (`return`).
fn name_start_ctx(src: &[u8], ws: usize, we: usize) -> usize {
    for k in ws..we {
        if src[k] == b'.' {
            return k + 1;
        }
    }
    ws
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
        classify_context, is_ident_at, is_ident_or_dot_at, name_start_ctx, starts_context,
        starts_statevar,
    };
    include!("ref_scan.gen.rs");
}

/// A Frame reference recognized at `bytes[i..]`, if any: `(kind, name, end)` where `end` is
/// the offset one past the ref. `None` if there is no reference there — the same answer
/// the hand `frame_ref_at_hand` oracle gives.
pub fn scan(bytes: &[u8], i: usize) -> Option<(RefKind, String, usize)> {
    let mut m = fsm::RefScan::over(bytes);
    if !m.scan_at(i) {
        return None;
    }
    let kind = match m.kind {
        1 => RefKind::StateVar,
        2 => RefKind::ContextSelf,
        3 => RefKind::ContextData,
        4 => RefKind::ContextParams,
        5 => RefKind::ContextReturn,
        6 => RefKind::ContextEvent,
        7 => RefKind::ContextSystemState,
        _ => return None,
    };
    let name = String::from_utf8_lossy(&bytes[m.name_out..m.name_end]).into_owned();
    Some((kind, name, m.cursor))
}
