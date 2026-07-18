//! The state-member dispatch walk, **dogfooded as an `@@[scan(u8)]` system**
//! ([`state_walk.frs`]) — the member-level analogue of [`super::machine_walk`] (states) and
//! [`super::segmenter`] (items).
//!
//! [`member_starts`] returns the member-start offsets in a state body — a `$.x` state variable or
//! a handler head — driven by the `StateWalk` system: each start is recorded and the walk jumps
//! past that member's extent (a state var to end-of-line via [`super::machine::to_end_of_line`]; a
//! handler past its body via [`super::machine::handler_end`] → `handler_head`, the SAME source
//! `handler_at` builds the node from). [`super::machine`]'s `state()` member loop is now a thin
//! native driver over these positions.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit state_walk.frs | grep -v '^#!\[allow' >
//! state_walk.gen.rs`.

use super::literals::Target;
use super::machine::{handler_end, skip_opaque, to_end_of_line};

/// Opaque-skip leaf: the offset past a comment/literal at `i`, or `i` unchanged. No walk (D3).
fn skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    skip_opaque(src, i, limit, target).unwrap_or(i)
}

/// Does a `$.x` state variable open at `i`?
fn is_statevar(src: &[u8], i: usize) -> bool {
    i + 1 < src.len() && src[i] == b'$' && src[i + 1] == b'.'
}

/// The offset one past the member that opens at `i` — a state var to end-of-line, a handler past
/// its body — or `i` unchanged if nothing opens here. Both extents come from shared `machine`
/// helpers (`to_end_of_line` / `handler_end` → `handler_head`), the same sources the driver
/// builds the nodes from, so the boundary and the extent cannot drift. No walk here (D3).
fn member_end(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    if is_statevar(src, i) {
        return to_end_of_line(src, i, limit);
    }
    if let Some(e) = handler_end(src, i, limit, target) {
        return e;
    }
    i
}

/// Record a member-start offset. (A leaf so the machine body stays free of `Vec` mechanics.)
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
    use super::{member_end, record, skip, Target};
    include!("state_walk.gen.rs");
}

/// The member-start offsets in a state body `bytes[from..close]`, target-aware — driven by the
/// `StateWalk` system. Each `$.x` state var or handler head is recorded and the walk jumps past
/// its extent, so only top-level members are returned.
pub fn member_starts(bytes: &[u8], from: usize, close: usize, target: Target) -> Vec<usize> {
    let mut m = fsm::StateWalk::over(bytes, target, close);
    m.scan_at(from);
    m.starts
}

/// The retired hand walk — kept ONLY as the `member_starts` differential-test oracle until the
/// parity is locked and the hand recognition is deleted. This is exactly the pre-conversion
/// `state()` member boundary loop (skip opaque; a `$.x` → record + skip to end-of-line; a handler
/// → record + skip past its body), factored out from the node-building driver. Shares the leaves
/// with the system (as `MachineWalk`/`Segmenter` oracles do) — the differential proves the WALK.
/// Not used in production.
#[doc(hidden)]
pub fn member_starts_hand(bytes: &[u8], from: usize, close: usize, target: Target) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut i = from;
    while i < close {
        if let Some(next) = skip_opaque(bytes, i, close, target) {
            i = next;
            continue;
        }
        if is_statevar(bytes, i) {
            starts.push(i);
            i = to_end_of_line(bytes, i, close);
            continue;
        }
        if let Some(e) = handler_end(bytes, i, close, target) {
            starts.push(i);
            i = e;
            continue;
        }
        i += 1;
    }
    starts
}
