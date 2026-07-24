//! The system constructor's **domain-field initializer walk**, dogfooded as a plain `@@system`
//! ([`domain_init_walk.frs`]) — the emit-side sequencer that reifies the `for f in &sym.domain`
//! loop [`super::driver::Backend::open_system`] ran inline, and the seventh emit-side machine after
//! [`super::stmt_walk`], [`super::base_column`], [`super::emit_handlers`],
//! [`super::emit_interface`], [`super::emit_actions`], and [`super::emit_system`].
//!
//! It is a ONE-LEVEL CYCLE (`$Field`), one cursor, one bound, no stack. framec owns the walk; the
//! un-Frame-able work is the single [`stamp_domain_init`] leaf, which hands field `i` to the
//! BACKEND ([`super::driver::Backend::domain_init`]) — the walk never spells anything itself, so a
//! target with no constructor-time domain seeding simply emits nothing.
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::domain_init_hand`] and
//! gated in `tests/domain_init_walk.rs` (GATE-A, via [`super::driver::domain_init_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit domain_init_walk.frs | grep -v '^#!\[allow' >
//! domain_init_walk.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// STAMP one domain field's constructor initializer: ask the backend to spell `sym.domain[i]`.
/// Out-of-range indices stamp nothing (total), so an over-run drive loop cannot duplicate a line.
/// (Frame cannot walk a symbol table, and the *spelling* is the target's — this leaf is the whole
/// un-Frame-able part of the pass.)
fn stamp_domain_init(sym: &SystemSym, be: &dyn Backend, i: usize, out: &mut Sink) {
    if i >= sym.domain.len() {
        return;
    }
    be.domain_init(sym, i, out);
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
    use super::{stamp_domain_init, Backend, Sink, SystemSym};
    include!("domain_init_walk.gen.rs");
}

/// Emit every domain field's constructor initializer, driving the `DomainInitWalk` sequencer, and
/// hand back the accumulated text. Returns a `String` (not a borrowed `Sink`) because the caller is
/// a BACKEND SPELLING — `open_system` splices this block into the middle of a constructor it is
/// building — so the walk owns its own sink for the duration and the spelling places the result.
/// The bounded drive loop lives here: a broken machine cannot hang.
pub(super) fn walk(sym: &SystemSym, be: &dyn Backend) -> String {
    let mut m = fsm::DomainInitWalk::new(sym, be, sym.domain.len(), Sink::new());
    // Each `$Field` step stamps one field and advances, or halts; `nd + 1` steps reach `$Done`,
    // and the slack covers the terminal step and any `$Done` no-ops.
    let bound = sym.domain.len() + 8;
    for _ in 0..bound {
        m.step();
    }
    m.out.finish()
}
