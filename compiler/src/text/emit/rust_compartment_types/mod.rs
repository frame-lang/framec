//! The RUST per-system TYPED-COMPARTMENT emitter ([`rust_compartment_types.frs`]), pilot-style — the
//! `<Sys>Vars` / `<Sys>Args` enums (one variant per state) and the `<Sys>Comp { state, vars, args }`
//! struct that give every rust system a typed, per-state compartment (RFC-0056: the host serializer
//! marshals the vars/args natively; framec writes no `downcast`, no `Box<dyn Any>`). A rust-only
//! shape — no other backend spells it — so, like [`super::rust_dispatch`], it gets its OWN system and
//! its own six rust-only leaves ([`super::rust::ct_vars_open`], `ct_vars_variant`, `ct_args_open`,
//! `ct_args_variant`, `ct_close`, `ct_comp`) rather than a `Backend` seam.
//!
//! Rust's free `emit_compartment_types` is a one-line driver into [`drive`]. The byte-for-byte ORACLE
//! it replaced is the preserved [`super::rust::rust_compartment_types_hand`], gated in
//! `tests/emit_scaffold_walks.rs` (GATE-A, via
//! [`super::driver::rust_compartment_types_parity_report`]). The hand oracle is a standalone frozen
//! copy — it does NOT route through the machine — so a spelling bug in a leaf is visible to the gate.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit rust_compartment_types.frs | grep -v '^#!\[allow' >
//! rust_compartment_types.gen.rs`.

use super::Sink;
use crate::resolve::SystemSym;

mod fsm {
    #![allow(
        dead_code,
        unused_parens,
        non_snake_case,
        unused_variables,
        unused_mut,
        unused_imports
    )]
    use super::super::rust::{
        ct_args_open, ct_args_variant, ct_close, ct_comp, ct_vars_open, ct_vars_variant,
    };
    use super::super::Sink;
    use crate::resolve::SystemSym;
    include!("rust_compartment_types.gen.rs");
}

/// Spell one system's typed compartment (`<Sys>Vars` / `<Sys>Args` / `<Sys>Comp`), driving the
/// `RustCompartmentTypes` sequencer. Seeds the machine's owned `out` with the caller's Sink
/// (`std::mem::take`, as every landed walk does), drives to a bounded fixpoint, and writes the grown
/// Sink back. Called from rust's free `emit_compartment_types`. The bounded drive loop lives here: a
/// broken machine cannot hang. (No scan-guard — the typed compartment is emitted for scanner systems
/// too, from `open_scanner`.)
pub(super) fn drive(sym: &SystemSym, out: &mut Sink) {
    let ns = sym.states.len();
    let seed = std::mem::take(out);
    let mut m = fsm::RustCompartmentTypes::new(sym, ns, seed);
    // $VarsOpen(1) + $VarsLoop(ns+1) + $VarsClose(1) + $ArgsOpen(1) + $ArgsLoop(ns+1) +
    // $ArgsClose(1) + $Comp(1) + terminal/$Done slack.
    let bound = 2 * ns + 8;
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
