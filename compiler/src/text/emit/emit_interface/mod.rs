//! The driver's **interface/router walk**, dogfooded as a plain `@@system`
//! ([`emit_interface.frs`]) — the emit-side sequencer that reifies `emit`'s per-event router pass
//! (the PUBLIC method per interface event that dispatches to the private handler methods), the
//! fourth emit-side machine after [`super::stmt_walk`], [`super::base_column`], and
//! [`super::emit_handlers`], and riding the same read-only borrowed domain (`&'a SystemSym`,
//! `&'a dyn Backend`).
//!
//! The 2-level nesting is a FIXED depth-2 walk, so it needs no stack: it is two NESTED CYCLE STATES
//! (`$Method` → `$Arm`) with explicit up/down edges, one owned cursor per level (`mi`/`ai`) plus its
//! bound (`ni`/`na`), and a per-method arm accumulator (`arms`). framec owns the walk; the
//! un-Frame-able work is the native LEAVES here: [`state_count`] (the arm bound), [`stamp_arm`] (the
//! `resolve_handler` symbol-table lookup + arm push — Frame cannot walk a symbol table),
//! [`clear_arms`] (the per-method accumulator reset), [`method_is_async`] (the
//! `m.is_async || sym.is_async` disjunction), and [`route_method`], which spells one public method
//! by calling `be.route(...)` exactly as the hand pass did.
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::emit_interface_hand`] and
//! gated in `tests/emit_interface.rs` (GATE-A, via [`super::driver::interface_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit emit_interface.frs | grep -v '^#!\[allow' >
//! emit_interface.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// The per-method `(state, handler_owner)` router arms the walk accumulates — the exact shape
/// `be.route` consumes. A named alias so the `@@system`'s domain declaration stays a bare
/// identifier (Frame carries the type text verbatim; it never parses `Vec<(String, String)>`).
type ArmVec = Vec<(String, String)>;

/// The number of states in the machine — the `$Arm` bound `na`, set on each `$Method` descent. The
/// arm cycle visits one state per iteration; `sym.states.len()` is that cycle's length. (Frame
/// cannot walk the symbol table; this is the un-Frame-able length surfaced as one leaf.)
fn state_count(sym: &SystemSym) -> usize {
    sym.states.len()
}

/// Reset the per-method arm accumulator. Called on each `$Method` descent, before the `$Arm` cycle
/// re-stamps this method's arms — so the accumulator holds exactly the current method's arms when
/// `$Arm` routes. (The hand pass allocated a fresh `Vec` per method; the machine reuses one owned
/// buffer and clears it, which is byte-identical because `route` reads only its contents.)
fn clear_arms(arms: &mut ArmVec) {
    arms.clear();
}

/// STAMP one router arm for `(method mi, state ai)`: the HSM dispatch resolved from the symbol
/// table. When `resolve_handler(state, event)` is `Some(owner)` — the nearest ancestor (including
/// the state itself) that declares the event — push `(state.name, owner.name)`; when `None` (nobody
/// handles it, a no-op event for that state) push nothing. The exact `filter_map` the hand pass ran,
/// surfaced as the `$Arm` cycle's per-state leaf. Out-of-range indices stamp nothing (total).
fn stamp_arm(sym: &SystemSym, mi: usize, ai: usize, arms: &mut ArmVec) {
    let (Some(m), Some(st)) = (sym.interface.get(mi), sym.states.get(ai)) else {
        return;
    };
    if let Some(owner) = sym.resolve_handler(&st.name, &m.name) {
        arms.push((st.name.clone(), owner.name.clone()));
    }
}

/// The method's `is_async` — IT says so, or the SYSTEM does (`@@[async]`). The exact disjunction
/// `emit` computes (`m.is_async || sym.is_async`), surfaced as the `$Arm` route-branch leaf. `false`
/// for an out-of-range index (never routed).
fn method_is_async(sym: &SystemSym, mi: usize) -> bool {
    match sym.interface.get(mi) {
        Some(m) => m.is_async || sym.is_async,
        None => false,
    }
}

/// ROUTE one public interface method: the verbatim `be.route(...)` the hand pass ran, surfaced as
/// the `$Arm` route-branch leaf. Reads the method's name / params / return type from the symbol
/// table, threads in the stamped `arms` (borrowed, not moved) and the computed `is_async`, and lets
/// the backend spell the per-state dispatch switch. Out of range emits nothing (total).
fn route_method(sym: &SystemSym, be: &dyn Backend, mi: usize, arms: &ArmVec, is_async: bool, out: &mut Sink) {
    let Some(m) = sym.interface.get(mi) else {
        return;
    };
    be.route(
        sym,
        &m.name,
        m.params_text.as_deref().unwrap_or(""),
        m.return_text.as_deref(),
        is_async,
        arms,
        out,
    );
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
        clear_arms, method_is_async, route_method, stamp_arm, state_count, ArmVec, Backend, Sink,
        SystemSym,
    };
    include!("emit_interface.gen.rs");
}

/// Emit every PUBLIC interface router method of a system, driving the `EmitInterface` sequencer.
/// Seeds the machine's owned `out` with the caller's Sink (`std::mem::take`, as
/// [`super::driver::emit_body`] does around StmtWalk), seeds an empty arm accumulator, drives to
/// fixpoint, and writes the grown Sink back. The bounded drive loop lives here — a broken machine
/// cannot hang.
pub(super) fn walk(sym: &SystemSym, be: &dyn Backend, out: &mut Sink) {
    let seed = std::mem::take(out);
    let mut m = fsm::EmitInterface::new(sym, be, sym.interface.len(), Vec::new(), seed);
    // A safe over-bound on the number of steps: for each of the `ni` methods, `$Method` fires once
    // (the descent) and `$Arm` fires `na + 1` times (`na` per-state stamps plus the route), then the
    // terminal `$Method` halt. `na` is the constant state count. Computing it is a cheap product (no
    // emission).
    let bound = sym.interface.len() * (sym.states.len() + 2) + 4;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
