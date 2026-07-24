//! The generated runtime's **state-chain table walk**, dogfooded as a plain `@@system`
//! ([`hsm_chain_walk.frs`]) — for every leaf state, the ROOT..LEAF path the target's compartment
//! factory walks when entering it. The eighth emit-side machine, riding the same read-only borrowed
//! domain (`&'a SystemSym`, `&'a dyn Backend`) as the rest.
//!
//! Three cycle states — `$State` (the state cursor) → `$Climb` (the ancestor climb) → `$Emit` (the
//! per-state entry) → back up. No stack: the climb's depth is bounded by the state count.
//!
//! framec owns the walk; the un-Frame-able work is the leaves here — [`clear_chain`],
//! [`push_state_name`], [`parent_index`] (symbol-table reads), and [`stamp_chain`], which reverses
//! the leaf-first path and hands it to [`super::driver::Backend::hsm_chain_entry`]. The SPELLING is
//! the target's, so a target with no such table emits nothing.
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::hsm_chain_hand`] and gated
//! in `tests/emit_scaffold_walks.rs` (GATE-A, via [`super::driver::hsm_chain_parity_report`]). That
//! oracle calls these SAME leaves, so the gate isolates exactly the SEQUENCING (hand loops vs cycle
//! states) and nothing else.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit hsm_chain_walk.frs | grep -v '^#!\[allow' >
//! hsm_chain_walk.gen.rs`.

use super::driver::Backend;
use super::Sink;
use crate::resolve::SystemSym;

/// The per-state ancestor path being accumulated, LEAF-FIRST while the climb runs. A named alias so
/// the `@@system`'s domain declaration stays a bare identifier (Frame carries the type text
/// verbatim; it never parses `Vec<String>`).
type ChainVec = Vec<String>;

/// Reset the per-state path accumulator, on each `$State` descent — so it holds exactly the current
/// leaf's path when `$Emit` stamps. (One owned buffer, cleared, rather than a fresh `Vec` per state:
/// byte-identical, because the spelling reads only its contents.)
pub(super) fn clear_chain(chain: &mut ChainVec) {
    chain.clear();
}

/// Push the name of the state at index `ci` onto the path. Out-of-range pushes nothing (total), so
/// an over-run drive loop cannot lengthen a chain.
pub(super) fn push_state_name(sym: &SystemSym, ci: usize, chain: &mut ChainVec) {
    if let Some(st) = sym.states.get(ci) {
        chain.push(st.name.clone());
    }
}

/// The INDEX of state `ci`'s parent (`$Child => $Parent`), or `-1` when it has none — and also when
/// the declared parent is not itself a declared state, which stops the climb at the last real state
/// rather than inventing a table row for a name the machine does not contain. Indices are `usize`
/// and always fit an `i64`, so `-1` is a clean out-of-band sentinel the machine keys its exit on.
pub(super) fn parent_index(sym: &SystemSym, ci: usize) -> i64 {
    let Some(st) = sym.states.get(ci) else {
        return -1;
    };
    let Some(parent) = st.parent.as_deref() else {
        return -1;
    };
    match sym.states.iter().position(|s| s.name == parent) {
        Some(idx) => idx as i64,
        None => -1,
    }
}

/// STAMP one table entry for leaf state `si`: reverse the leaf-first accumulator into ROOT-FIRST
/// order (the order the factory walks it) and hand it to the backend. Out-of-range stamps nothing.
pub(super) fn stamp_chain(
    sym: &SystemSym,
    be: &dyn Backend,
    si: usize,
    chain: &mut ChainVec,
    out: &mut Sink,
) {
    let Some(st) = sym.states.get(si) else { return };
    chain.reverse();
    be.hsm_chain_entry(&st.name, chain, out);
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
        clear_chain, parent_index, push_state_name, stamp_chain, Backend, ChainVec, Sink, SystemSym,
    };
    include!("hsm_chain_walk.gen.rs");
}

/// Emit the whole state-chain table body, driving the `HsmChainWalk` sequencer, and hand back the
/// accumulated text. Returns a `String` because the caller is a BACKEND SPELLING splicing this block
/// between the table's own braces. The bounded drive loop lives here: a broken machine cannot hang.
pub(super) fn walk(sym: &SystemSym, be: &dyn Backend) -> String {
    let n = sym.states.len();
    let mut m = fsm::HsmChainWalk::new(sym, be, n, Vec::new(), Sink::new());
    // Per state: one `$State` descent, at most `n + 1` `$Climb` steps (the depth guard), and one
    // `$Emit`; plus the terminal `$State` halt. The slack covers `$Done` no-ops.
    let bound = n * (n + 4) + 8;
    for _ in 0..bound {
        m.step();
    }
    m.out.finish()
}
