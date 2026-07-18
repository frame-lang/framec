//! The handler-body statement dispatch walk, **dogfooded as an `@@[scan(u8)]` system**
//! ([`body_walk.frs`]) — the statement-level analogue of [`super::state_walk`] /
//! [`super::machine_walk`] / [`super::segmenter`], and the first to FUSE a segmenter-style
//! accumulator (`starts`) with a DelimBalance-style running counter (`depth`).
//!
//! [`stmt_starts`] returns each Frame-statement start offset in a handler body **paired with the
//! brace depth at that point**, driven by the `BodyWalk` system: it skips opaque regions, skips
//! each statement's extent (the shared `frame_call_end`/`frame_assign_end`/`stmt_scan::classify`
//! heads — the SAME sources the driver builds nodes from), and counts `{`/`}` of native water into
//! `depth`. [`super::machine`]'s `body()` is now a thin native driver over `(start, depth)` (+ the
//! final depth for the trailing native gap).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit body_walk.frs | grep -v '^#!\[allow' >
//! body_walk.gen.rs`.

use super::literals::Target;
use super::machine::{frame_assign_end, frame_call_end, skip_opaque};

/// Opaque-skip leaf: the offset past a comment/literal at `i`, or `i` unchanged. No walk (D3).
fn skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    skip_opaque(src, i, limit, target).unwrap_or(i)
}

/// The offset one past the Frame statement that opens at `i`, or `i` unchanged if none does —
/// tried in `body()`'s order (`frame_call` → `frame_assign` → `frame_stmt`). Each detector is the
/// `native_parts`-free extent head, so this is O(statement) recognition, no construction, no walk.
fn stmt_end(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    if let Some(e) = frame_call_end(src, i, limit, target) {
        return e;
    }
    if let Some(e) = frame_assign_end(src, i, limit) {
        return e;
    }
    // The frame_stmt extent via the StmtScan SYSTEM (`stmt_scan::classify`) — the SAME source the
    // driver's `frame_stmt` uses (machine.rs), so walk-found and driver-built extents are truly
    // single-source (NOT the `frame_stmt_classify` hand oracle, which would wire a retired hand
    // recognizer into production).
    let (kind, end) = super::stmt_scan::classify(src, i, limit);
    if kind != 0 {
        return end;
    }
    i
}

/// Record a statement start together with the brace depth at that point. (A leaf so the machine
/// body stays free of `Vec` mechanics.)
fn record(v: &mut Vec<(usize, u32)>, start: usize, depth: u32) {
    v.push((start, depth));
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
    use super::{record, skip, stmt_end, Target};
    include!("body_walk.gen.rs");
}

/// The Frame-statement starts (each paired with its brace depth) in a handler body
/// `bytes[from..limit]`, plus the final brace depth at `limit` (for the trailing native gap),
/// target-aware — driven by the `BodyWalk` system.
pub fn stmt_starts(bytes: &[u8], from: usize, limit: usize, target: Target) -> (Vec<(usize, u32)>, u32) {
    let mut m = fsm::BodyWalk::over(bytes, target, limit);
    m.scan_at(from);
    (m.starts, m.depth)
}

/// The retired hand walk — kept ONLY as the `stmt_starts` differential-test oracle until the
/// parity is locked and the hand recognition is deleted. This is exactly the pre-conversion
/// `body()` boundary loop (a Frame statement → record `(start, depth)` + skip its extent; else
/// opaque-skip; else count one brace of native water), factored out from the node-building driver.
/// Shares the leaves with the system (as the other walk oracles do) — the differential proves the
/// WALK (dispatch + the running depth counter). Not used in production.
#[doc(hidden)]
pub fn stmt_starts_hand(bytes: &[u8], from: usize, limit: usize, target: Target) -> (Vec<(usize, u32)>, u32) {
    let mut starts = Vec::new();
    let mut i = from;
    let mut depth = 0u32;
    while i < limit {
        let se = stmt_end(bytes, i, limit, target);
        if se > i {
            starts.push((i, depth));
            i = se;
            continue;
        }
        if let Some(next) = skip_opaque(bytes, i, limit, target) {
            i = next;
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    (starts, depth)
}
