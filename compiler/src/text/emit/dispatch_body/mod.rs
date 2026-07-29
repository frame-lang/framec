//! The SHARED per-state message-dispatcher body walk ([`dispatch_body.frs`]) for the `if`-chain
//! targets — Python, Java, C. Where the shared [`super::state_dispatch_walk`] decides which arms a
//! state dispatches and in what order, this walk SPELLS the method body those arms produce, through
//! four `Backend` seam leaves ([`super::driver::Backend::dispatch_open`], `dispatch_param`,
//! `dispatch_arm`, `dispatch_close`). The walk is identical across the three targets; only the
//! per-fragment spelling differs, so each backend supplies four small leaves and shares this one
//! sequencer. Rust's dispatcher is a `match` over a typed enum and gets its own system.
//!
//! Each backend's `Backend::dispatch` is a one-line driver into [`drive`]. The byte-for-byte
//! ORACLES it replaced are each target's preserved `*_dispatch_hand`, gated in
//! `tests/emit_scaffold_walks.rs` (GATE-A, via `driver::dispatch_body_parity_report`). The hand
//! oracles are standalone frozen copies — they do NOT route through `be.dispatch` — so a spelling
//! bug in a leaf is visible to the gate.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit dispatch_body.frs | grep -v '^#!\[allow' >
//! dispatch_body.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// The per-state event messages the walk emits an arm for — arrives pre-ordered from the shared
/// `StateDispatchWalk`. A named alias so the `@@system`'s domain declaration stays a bare identifier
/// (Frame carries the type text verbatim; it never parses `Vec<String>`).
type EventVec = Vec<String>;

/// Emit the dispatcher method HEADER (the `def`/`void`/`static void` line). Per-language spelling.
pub(super) fn dispatch_open(be: &dyn Backend, sym: &SystemSym, state: &str, out: &mut Sink) {
    be.dispatch_open(sym, state, out);
}

/// Bind the state PARAM at slot `pi` off the live compartment's `state_args`. Per-language spelling
/// (untyped on Python, typed-unbox on Java, typed-cast on C). Total: out of range emits nothing.
pub(super) fn dispatch_param(be: &dyn Backend, sym: &SystemSym, state: &str, pi: usize, out: &mut Sink) {
    be.dispatch_param(sym, state, pi, out);
}

/// Emit one dispatch ARM for the event message at slot `ai` (`if <msg-compare> { call; return }`).
/// Per-language spelling. Total: out of range emits nothing.
pub(super) fn dispatch_arm(
    be: &dyn Backend,
    sym: &SystemSym,
    state: &str,
    arms: &[String],
    ai: usize,
    out: &mut Sink,
) {
    be.dispatch_arm(sym, state, arms, ai, out);
}

/// CLOSE the dispatcher. Per-language: Python emits `pass` when the whole dispatcher is empty and the
/// `=> $^` default-forward fall-through otherwise; Java and C emit the closing brace. `np` is the
/// state's param count (Python's empty test). Total.
pub(super) fn dispatch_close(
    be: &dyn Backend,
    sym: &SystemSym,
    state: &str,
    arms: &[String],
    np: usize,
    out: &mut Sink,
) {
    be.dispatch_close(sym, state, arms, np, out);
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
        dispatch_arm, dispatch_close, dispatch_open, dispatch_param, Backend, EventVec, Sink,
        SystemSym,
    };
    include!("dispatch_body.gen.rs");
}

/// Spell one state's message dispatcher, driving the shared `DispatchBody` sequencer. Reads the
/// state's param count (the `$Params` loop bound), seeds the machine's owned `out` with the caller's
/// Sink (`std::mem::take`, as every landed walk does), drives to a bounded fixpoint, and writes the
/// grown Sink back. Called from each `if`-chain backend's `Backend::dispatch`. The bounded drive
/// loop lives here: a broken machine cannot hang.
pub(super) fn drive(be: &dyn Backend, sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
    let np = sym
        .states
        .iter()
        .find(|s| s.name == state)
        .map(|s| s.state_params.len())
        .unwrap_or(0);
    let na = arms.len();
    let seed = std::mem::take(out);
    let mut m = fsm::DispatchBody::new(be, sym, state, arms.to_vec(), np, na, seed);
    // $Header(1) + $Params(np+1) + $Arms(na+1) + $Close(1) + terminal/$Done slack.
    let bound = np + na + 8;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
