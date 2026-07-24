//! The generated runtime's **state router walk**, dogfooded as a plain `@@system`
//! ([`router_walk.frs`]) — one arm per state of "if the live compartment is in this state, hand the
//! event to that state's dispatcher". The ninth emit-side machine, riding the same read-only
//! borrowed domain (`&'a SystemSym`, `&'a dyn Backend`).
//!
//! A one-level cycle (`$Arm`) carrying a single `first` latch, so the SPELLING never has to
//! re-derive "have I written an arm yet?" from the text it already emitted — the exact
//! post-emission oracle RFC-0056 P6 forbids. framec owns the walk; the un-Frame-able work is the
//! [`stamp_router_arm`] leaf, which hands `(state, first)` to
//! [`super::driver::Backend::router_arm`].
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::router_hand`] and gated in
//! `tests/emit_scaffold_walks.rs` (GATE-A, via [`super::driver::router_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit router_walk.frs | grep -v '^#!\[allow' >
//! router_walk.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// STAMP one router arm for the state at index `si`, telling the backend whether it is the LEADING
/// arm. Out-of-range stamps nothing (total), so an over-run drive loop cannot duplicate an arm.
pub(super) fn stamp_router_arm(
    sym: &SystemSym,
    be: &dyn Backend,
    si: usize,
    first: bool,
    out: &mut Sink,
) {
    let Some(st) = sym.states.get(si) else { return };
    be.router_arm(sym, &st.name, first, out);
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
    use super::{stamp_router_arm, Backend, Sink, SystemSym};
    include!("router_walk.gen.rs");
}

/// Emit the router's whole arm chain, driving the `RouterWalk` sequencer, and hand back the
/// accumulated text. Returns a `String` because the caller is a BACKEND SPELLING splicing this block
/// into the router method it is building. The bounded drive loop lives here: a broken machine cannot
/// hang.
pub(super) fn walk(sym: &SystemSym, be: &dyn Backend) -> String {
    let n = sym.states.len();
    let mut m = fsm::RouterWalk::new(sym, be, n, Sink::new());
    // One `$Arm` step per state plus the terminal halt; the slack covers `$Done` no-ops.
    let bound = n + 8;
    for _ in 0..bound {
        m.step();
    }
    m.out.finish()
}
