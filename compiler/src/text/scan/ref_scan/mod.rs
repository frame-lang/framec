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

/// The first segment of a `@@:word` — up to the first `.`, or the whole word if there is none
/// (`self.factor` -> `self`, `return` -> `return`, `database.k` -> `database`). This is the
/// context KEYWORD, and it is what classification matches — a proper word boundary, never a prefix.
fn first_segment_end(src: &[u8], ws: usize, we: usize) -> usize {
    (ws..we).find(|&k| src[k] == b'.').unwrap_or(we)
}

/// Classify a `@@:word` context ref by its FIRST SEGMENT (Δ5, T-R2): a proper segment/word-
/// boundary match against `self`/`data`/`params`/`return`/`event`/`system`, NOT a `starts_with`
/// prefix match — so `@@:database` is no longer `data`, `@@:selfish` no longer `self`. An
/// unrecognized keyword is **Unknown** (kind 8, Δ5 T-R1): a refusal as data, never a
/// `ContextSelf` guess. The scanner recognizes shape; membership is the validator's (E408).
fn classify_context(src: &[u8], ws: usize, we: usize) -> i32 {
    match &src[ws..first_segment_end(src, ws, we)] {
        b"self" => 2,
        b"data" => 3,
        b"params" => 4,
        b"return" => 5,
        b"event" => 6,
        b"system" => 7,
        _ => 8,
    }
}

/// The name of a context ref begins after the first `.` in the word (`self.factor` -> at
/// `factor`), or is the whole word if there is no dot (`return`). For an UNKNOWN context (kind 8)
/// the WHOLE word is the name — `validate.rs` is byte-free and renders `@@:<name>` from it.
fn name_start_ctx(src: &[u8], ws: usize, we: usize) -> usize {
    if classify_context(src, ws, we) == 8 {
        return ws;
    }
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
        8 => RefKind::Unknown, // Δ5 (T-R1): unrecognized context word — refusal as data.
        _ => return None,
    };
    let name = String::from_utf8_lossy(&bytes[m.name_out..m.name_end]).into_owned();
    Some((kind, name, m.cursor))
}
