//! The driver's **per-system phase spine**, dogfooded as a plain `@@system`
//! ([`emit_system.frs`]) — the emit-side sequencer that reifies `emit`'s per-system run of passes
//! (interface router, private handlers, native-bodied actions/operations, `@@[persist]`), the
//! sixth emit-side machine after [`super::stmt_walk`], [`super::base_column`],
//! [`super::emit_handlers`], [`super::emit_interface`], and [`super::emit_actions`], and riding the
//! same read-only borrowed domain (`&'a SystemSym`, `&'a [Section]`, `&'a dyn Backend`, …).
//!
//! It is a LINEAR 4-STATE SPINE (`$Interface` → `$Handlers` → `$Actions` → `$Persist`), each phase
//! an unconditional advance that calls one already-landed sub-system's `walk` as a leaf; `$Persist`
//! is the one guarded state (`manifest.enabled`). No cursor, no cycle. The `open_system` /
//! `close_system` bookends are native here in [`walk`] — backend spellings, not sub-systems. framec
//! owns the spine; the un-Frame-able work is the native LEAVES: [`emit_iface_phase`],
//! [`emit_handlers_phase`], [`emit_actions_phase`] (each calls one landed sub-system, unchanged),
//! [`manifest_enabled`] (the persist flag), and [`emit_persist`] (the one `be.persist(...)`).
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::emit_system_hand`] and
//! gated in `tests/emit_system.rs` (GATE-A, via [`super::driver::system_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit emit_system.frs | grep -v '^#!\[allow' >
//! emit_system.gen.rs`.

use super::driver::Backend;
use super::persist::PersistManifest;
use super::{emit_actions, emit_handlers, emit_interface, Sink};
use crate::resolve::{SymbolTable, SystemSym};
use crate::text::Source;
use crate::tree::Section;

/// Phase `$Interface`: the router pass — one public method per interface event, dispatching to the
/// private handlers. Calls the landed [`emit_interface::walk`] `@@system`, unchanged (NOT reinlined).
fn emit_iface_phase(sym: &SystemSym, be: &dyn Backend, out: &mut Sink) {
    emit_interface::walk(sym, be, out);
}

/// Phase `$Handlers`: the private-handler pass — one private method per `(state, handler)`. Calls
/// the landed [`emit_handlers::walk`] `@@system`, unchanged (NOT reinlined).
fn emit_handlers_phase(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    emit_handlers::walk(src, syms, sym, sections, be, out);
}

/// Phase `$Actions`: the `actions:` / `operations:` pass — one method per user-bodied member. Calls
/// the landed [`emit_actions::walk`] `@@system`, unchanged (NOT reinlined).
fn emit_actions_phase(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    emit_actions::walk(src, syms, sym, sections, be, out);
}

/// The `$Persist` guard: is `@@[persist]` in force for this system? Reads the FROZEN decision the
/// persist derivation already made ([`PersistManifest::enabled`]) — a stamped bit, not a carried
/// mode. Surfaced as a leaf so the spine's guard stays a bare `en == false` (the `is_action_section`
/// idiom), avoiding a nested field access in the machine condition.
fn manifest_enabled(m: &PersistManifest) -> bool {
    m.enabled
}

/// Phase `$Persist` (guarded arm): spell the one `be.persist(...)` the hand pass ran, from the
/// already-derived manifest (RFC-0054 — WHAT to persist, decided once from the symbol table; the
/// backend spells HOW). Reached only when [`manifest_enabled`] held.
fn emit_persist(be: &dyn Backend, m: &PersistManifest, out: &mut Sink) {
    be.persist(m, out);
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
        emit_actions_phase, emit_handlers_phase, emit_iface_phase, emit_persist, manifest_enabled,
        Backend, PersistManifest, Section, Sink, Source, SymbolTable, SystemSym,
    };
    include!("emit_system.gen.rs");
}

/// Emit ONE whole system — driving the `EmitSystem` phase spine bracketed by the native
/// `open_system` / `close_system` bookends. `open_system` writes the class header into the caller's
/// Sink; the machine then takes that Sink (`std::mem::take`, as every landed walk does), runs the
/// four phases into it, and hands it back; `close_system` writes the closer. Byte-identical to the
/// hand phase run because the phases append exactly where the hand pass appended and the bookends
/// bracket exactly where it bracketed. The bounded drive loop lives here — a broken machine cannot
/// hang.
pub(super) fn walk(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    be.open_system(sym, out);
    // The persist decision, derived ONCE (RFC-0054) and carried into the `$Persist` guard.
    let manifest = PersistManifest::derive(sym, syms);
    let seed = std::mem::take(out);
    let mut m = fsm::EmitSystem::new(src, syms, sym, sections, be, manifest, seed);
    // A safe over-bound: the spine is exactly four unconditional advances (`$Interface` →
    // `$Handlers` → `$Actions` → `$Persist` → `$Done`); any further step is a no-op at `$Done`.
    for _ in 0..6 {
        m.step();
    }
    *out = m.out;
    be.close_system(sym, out);
}
