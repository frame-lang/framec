//! The SHARED per-state HANDLER-OPENER walk ([`handler_open.frs`]) for the header + binding-loop
//! targets — Python, Java, C. It SPELLS the opening of one private `(state, handler)` method through
//! four `Backend` seam leaves ([`super::driver::Backend::handler_open`], `handler_state_param`,
//! `handler_event_param`, `handler_seeds`): the method HEADER, the state's own params bound off the
//! live compartment's `state_args`, the event's own params bound off its slot
//! (`enter_args`/`exit_args`/`__e._parameters`), and a language TAIL (C seeds a `$>` state's `$.x`
//! vars; Python/Java have none). The walk is identical across the three targets; only the
//! per-fragment spelling differs, so each backend supplies small leaves and shares this one
//! sequencer. Rust's opener is a scan-branch + header-only kernel branch with no binding loops, and
//! gets its own (future) system.
//!
//! Each backend's `Backend::open_handler` is a one-line driver into [`drive`]. The byte-for-byte
//! ORACLES it replaced are each target's preserved `*_open_handler_hand`, gated in
//! `tests/emit_scaffold_walks.rs` (GATE-A, via `driver::handler_open_parity_report`). The hand
//! oracles are standalone frozen copies — they do NOT route through `be.open_handler` — so a
//! spelling bug in a leaf is visible to the gate.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit handler_open.frs | grep -v '^#!\[allow' >
//! handler_open.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// Emit the handler method HEADER (the `def`/`private void`/`static void` line). Per-language
/// spelling. `params` rides along for a signature-parity of arguments with the other leaves; the
/// header itself reads only `state`/`event` (the bindings are the two loops' job).
pub(super) fn handler_open(
    be: &dyn Backend,
    sym: &SystemSym,
    state: &str,
    event: &str,
    params: &str,
    out: &mut Sink,
) {
    be.handler_open(sym, state, event, params, out);
}

/// Bind the state PARAM at slot `si` off the live compartment's `state_args`. Per-language spelling
/// (untyped on Python, typed-unbox on Java, typed-cast on C). Total: out of range emits nothing.
pub(super) fn handler_state_param(be: &dyn Backend, sym: &SystemSym, state: &str, si: usize, out: &mut Sink) {
    be.handler_state_param(sym, state, si, out);
}

/// Bind the event PARAM at slot `ei` off its source slot — `enter_args` for `$>`, `exit_args` for
/// `<$`, `__e._parameters` for a user event. The slot MATCH lives inside the leaf. Per-language
/// spelling. Total: out of range emits nothing.
pub(super) fn handler_event_param(
    be: &dyn Backend,
    sym: &SystemSym,
    state: &str,
    event: &str,
    params: &str,
    ei: usize,
    out: &mut Sink,
) {
    be.handler_event_param(sym, state, event, params, ei, out);
}

/// The language TAIL after the two binding loops. C seeds a `$>` state's `$.x` vars into the
/// compartment (guarded, after the enter-arg binds, exactly as the oracle's frame-enter handler
/// does); Python and Java have no tail and take the no-op default. Total.
pub(super) fn handler_seeds(be: &dyn Backend, sym: &SystemSym, state: &str, event: &str, out: &mut Sink) {
    be.handler_seeds(sym, state, event, out);
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
        handler_event_param, handler_open, handler_seeds, handler_state_param, Backend, Sink,
        SystemSym,
    };
    include!("handler_open.gen.rs");
}

/// Spell one `(state, handler)` method's OPENING, driving the shared `HandlerOpen` sequencer. Reads
/// the state's param count (the `$StateParams` loop bound) and the handler's non-empty event-param
/// count (the `$EventParams` loop bound), seeds the machine's owned `out` with the caller's Sink
/// (`std::mem::take`, as every landed walk does), drives to a bounded fixpoint, and writes the grown
/// Sink back. Called from each header + binding-loop backend's `Backend::open_handler`. The bounded
/// drive loop lives here: a broken machine cannot hang.
pub(super) fn drive(
    be: &dyn Backend,
    sym: &SystemSym,
    state: &str,
    event: &str,
    params: &str,
    out: &mut Sink,
) {
    let ns = sym
        .states
        .iter()
        .find(|s| s.name == state)
        .map(|s| s.state_params.len())
        .unwrap_or(0);
    let ne = super::driver::params_split(params)
        .into_iter()
        .filter(|(n, _)| !n.is_empty())
        .count();
    let seed = std::mem::take(out);
    let mut m = fsm::HandlerOpen::new(be, sym, state, event, params.to_string(), ns, ne, seed);
    // $Header(1) + $StateParams(ns+1) + $EventParams(ne+1) + $Seeds(1) + terminal/$Done slack.
    let bound = ns + ne + 8;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
