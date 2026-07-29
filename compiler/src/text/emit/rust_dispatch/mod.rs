//! The RUST per-state message-dispatcher walk ([`rust_dispatch.frs`]), pilot-style — the private
//! `_state_<S>` method the kernel router hands an event to, one per state, spelled as a `match` over
//! the typed `<Sys>FrameEvent` enum. Rust's dispatcher is a `match`, not the `if`-chain the shared
//! [`super::dispatch_body`] `DispatchBody` sequences for Python/Java/C, so it gets its OWN system and
//! its own three rust-only leaves ([`super::rust::rust_dispatch_open`], `rust_dispatch_arm`,
//! `rust_dispatch_close`) rather than the four `Backend` seam methods.
//!
//! Rust's `Backend::dispatch` is a one-line driver into [`drive`]. The byte-for-byte ORACLE it
//! replaced is the preserved [`super::rust::rust_dispatch_hand`], gated in
//! `tests/emit_scaffold_walks.rs` (GATE-A, via [`super::driver::rust_dispatch_parity_report`]). The
//! hand oracle is a standalone frozen copy — it does NOT route through `be.dispatch` — so a spelling
//! bug in a leaf is visible to the gate.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit rust_dispatch.frs | grep -v '^#!\[allow' >
//! rust_dispatch.gen.rs`.

use super::Sink;
use crate::resolve::SystemSym;

/// The per-state event messages the walk emits an arm for — arrives pre-ordered from the shared
/// `StateDispatchWalk`. A named alias so the `@@system`'s domain declaration stays a bare identifier
/// (Frame carries the type text verbatim; it never parses `Vec<String>`).
type EventVec = Vec<String>;

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    use super::super::rust::{rust_dispatch_arm, rust_dispatch_close, rust_dispatch_open};
    use super::super::Sink;
    use super::EventVec;
    use crate::resolve::SystemSym;
    include!("rust_dispatch.gen.rs");
}

/// Spell one state's `_state_<S>` message dispatcher, driving the `RustDispatch` sequencer. The
/// scan-guard lives here (a scanner dispatches directly in `route` and emits no `_state_<S>`, so the
/// driver returns before building the machine — the 24 `@@[scan]` `.gen.rs` stay byte-frozen). Seeds
/// the machine's owned `out` with the caller's Sink (`std::mem::take`, as every landed walk does),
/// drives to a bounded fixpoint, and writes the grown Sink back. Called from rust's
/// `Backend::dispatch`. The bounded drive loop lives here: a broken machine cannot hang.
pub(super) fn drive(sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
    if sym.scan.is_some() {
        return;
    }
    let na = arms.len();
    let seed = std::mem::take(out);
    let mut m = fsm::RustDispatch::new(sym, state, arms.to_vec(), na, seed);
    // $Header(1) + $Arms(na+1) + $Close(1) + terminal/$Done slack.
    let bound = na + 8;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
