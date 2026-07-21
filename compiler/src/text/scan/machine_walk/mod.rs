//! The machine-section dispatch walk, **dogfooded as an `@@[scan(u8)]` system**
//! ([`machine_walk.frs`]) — the state-level analogue of the item-level [`super::segmenter`].
//!
//! [`state_starts`] returns the `$Name` state-start offsets in a `machine:` span, driven by the
//! `MachineWalk` system: each start is recorded and the walk jumps past that state's whole body
//! (via the `state_end` leaf → [`super::machine::state_extent`], the SAME source `state()` builds
//! the node from), so a `$.x`/`$Ref` inside a handler is never a top-level state start.
//! [`super::machine::machine_section`] is now a thin native driver over these positions.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit machine_walk.frs | grep -v '^#!\[allow' >
//! machine_walk.gen.rs`.

use super::literals::Target;
use super::machine::{skip_opaque, state_extent};

/// Opaque-skip leaf: the offset past a comment/literal at `i` (kind-aware limit policy), or `i`
/// unchanged. Runs OpaqueScan via the shared `machine::skip_opaque`; no walk here (D3).
fn skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    skip_opaque(src, i, limit, target).unwrap_or(i)
}

/// Does a `$Name` state open at `i`? (`$` followed by an identifier start — not `$.`, not `$>`.)
fn is_state_start(src: &[u8], i: usize) -> bool {
    i + 1 < src.len()
        && src[i] == b'$'
        && (src[i + 1].is_ascii_alphabetic() || src[i + 1] == b'_')
}

/// The offset one past the body of the state that starts at `i` — the SAME extent `state()`
/// carries (via `machine::state_extent`), so the boundary this walk finds and the node the
/// driver builds cannot drift.
fn state_end(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    state_extent(src, i, limit, target).2
}

/// Record a state-start offset. (A leaf so the machine body stays free of `Vec` mechanics.)
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
    use super::{is_state_start, record, skip, state_end, Target};
    include!("machine_walk.gen.rs");
}

/// The `$Name` state-start offsets in `bytes[from..limit]`, target-aware — driven by the
/// `MachineWalk` system. Each start is recorded and the walk jumps past that state's body, so
/// only top-level states are returned.
pub fn state_starts(bytes: &[u8], from: usize, limit: usize, target: Target) -> Vec<usize> {
    let mut m = fsm::MachineWalk::over(bytes, target, limit);
    m.scan_at(from);
    m.starts
}
