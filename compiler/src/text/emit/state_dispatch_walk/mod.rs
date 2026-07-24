//! The generated runtime's **per-state message-dispatch walk**, dogfooded as a plain `@@system`
//! ([`state_dispatch_walk.frs`]) — the private method the router hands an event to, one per state,
//! which matches the message against the handlers that state declares. The tenth emit-side machine,
//! riding the same read-only borrowed domain (`&'a SystemSym`, `&'a dyn Backend`).
//!
//! Two nested cycle states (`$State` → `$Handler`), one cursor per level, no stack — the
//! [`super::emit_interface`] shape. framec owns the walk; the un-Frame-able work is the leaves here
//! — [`handler_count`], [`clear_arms`], [`stamp_handler`] (symbol-table reads), and
//! [`dispatch_state`], which hands `(state, arms)` to [`super::driver::Backend::dispatch`].
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::state_dispatch_hand`] and
//! gated in `tests/emit_scaffold_walks.rs` (GATE-A, via
//! [`super::driver::state_dispatch_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit state_dispatch_walk.frs | grep -v '^#!\[allow' >
//! state_dispatch_walk.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// The per-state event messages the walk accumulates — the exact shape
/// [`super::driver::Backend::dispatch`] consumes. A named alias so the `@@system`'s domain
/// declaration stays a bare identifier (Frame carries the type text verbatim; it never parses
/// `Vec<String>`).
type EventVec = Vec<String>;

/// How many handlers the state at index `si` declares — the `$Handler` bound `nh`, set on each
/// `$State` descent. Zero for an out-of-range index (the inner cycle then dispatches an empty arm
/// list, which is the "this state handles nothing" case a backend must still spell).
pub(super) fn handler_count(sym: &SystemSym, si: usize) -> usize {
    sym.states.get(si).map(|s| s.handlers.len()).unwrap_or(0)
}

/// Reset the per-state arm accumulator, on each `$State` descent — so it holds exactly the current
/// state's messages when `$Handler` dispatches.
pub(super) fn clear_arms(arms: &mut EventVec) {
    arms.clear();
}

/// STAMP one dispatch arm: the EVENT MESSAGE of handler `hi` of state `si`, in declaration order.
/// Frame's own lifecycle messages (`$>`, `<$`) come through exactly as declared — the mapping from a
/// message to a method NAME is a target spelling, not a walk decision, and lives in the backend.
/// Out-of-range stamps nothing (total).
pub(super) fn stamp_handler(sym: &SystemSym, si: usize, hi: usize, arms: &mut EventVec) {
    let Some(st) = sym.states.get(si) else { return };
    if let Some(h) = st.handlers.get(hi) {
        arms.push(h.event.clone());
    }
}

/// DISPATCH one state: hand `(state, stamped arms)` to the backend, which spells the whole method.
/// Out-of-range emits nothing (total).
pub(super) fn dispatch_state(
    sym: &SystemSym,
    be: &dyn Backend,
    si: usize,
    arms: &EventVec,
    out: &mut Sink,
) {
    let Some(st) = sym.states.get(si) else { return };
    be.dispatch(sym, &st.name, arms, out);
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
        clear_arms, dispatch_state, handler_count, stamp_handler, Backend, EventVec, Sink, SystemSym,
    };
    include!("state_dispatch_walk.gen.rs");
}

/// Emit every state's message dispatcher, driving the `StateDispatchWalk` sequencer. Seeds the
/// machine's owned `out` with the caller's Sink (`std::mem::take`, as every landed walk does),
/// drives to fixpoint, and writes the grown Sink back — this one appends to the system's stream in
/// place (it is a PHASE of `EmitSystem`, not a block spliced into a spelling), so it takes a `&mut
/// Sink` rather than returning a `String`. The bounded drive loop lives here: a broken machine
/// cannot hang.
pub(super) fn walk(sym: &SystemSym, be: &dyn Backend, out: &mut Sink) {
    let seed = std::mem::take(out);
    let mut m = fsm::StateDispatchWalk::new(sym, be, sym.states.len(), Vec::new(), seed);
    // For each of the `ns` states, `$State` fires once (the descent) and `$Handler` fires `nh + 1`
    // times (`nh` stamps plus the dispatch), then the terminal `$State` halt. Bound `nh` by the
    // largest handler count so the product is a safe over-bound.
    let max_h = sym.states.iter().map(|s| s.handlers.len()).max().unwrap_or(0);
    let bound = sym.states.len() * (max_h + 3) + 8;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
