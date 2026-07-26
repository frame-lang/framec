//! The driver's **top-level item walk**, dogfooded as a plain `@@system` ([`emit_file.frs`]) — the
//! OUTERMOST emit sequencer, the body of the public [`super::driver::emit`]. It reifies the file-item
//! loop: walk `ast.items`, passing top-level native code through verbatim (the "water") or delegating
//! a system to the [`super::emit_system`] phase spine. The seventh and outermost emit-side machine,
//! it closes the traversal composition — with it landed, the entire emit driver from the file down
//! through each system's phases, handlers, and statements runs through `@@system`s.
//!
//! It is a SINGLE CYCLE STATE (`$Item`) over one walk cursor `i`, forking structurally on each item
//! (`Item::Native` → water; otherwise → [`emit_system`] delegate). The `file_header` preamble is a
//! native bookend in [`walk`], emitted once before the cycle. framec owns the walk; the un-Frame-able
//! work is the native LEAVES: [`is_native_item`] (the structural fork), [`emit_native_item`] (the
//! water render — shared with the oracle via [`super::driver::render_native_item`]), and
//! [`emit_system_item`] (resolve the system's symbol and call the landed [`emit_system::walk`]).
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::emit_file_hand`] and gated
//! in `tests/emit_file.rs` (GATE-A, via [`super::driver::file_parity_report`]). The production
//! [`super::driver::emit`] delegates here, so the existing acceptance/snapshot corpus exercises this
//! machine transitively — a whole-file byte divergence would surface as snapshot churn.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit emit_file.frs | grep -v '^#!\[allow' >
//! emit_file.gen.rs`.

use super::driver::{prev_item_is_system, render_native_item, Backend};
use super::{emit_system, Sink};
use crate::resolve::SymbolTable;
use crate::text::Source;
use crate::tree::{FileAst, Item};

/// Is `ast.items[i]` top-level native code (the "water")? The `$Item` fork — a native item renders
/// verbatim, anything else delegates. Out of bounds is `false` (the `$Item` bound `i >= n` halts
/// first, but this stays total).
fn is_native_item(ast: &FileAst, i: usize) -> bool {
    matches!(ast.items.get(i), Some(Item::Native(_)))
}

/// Render ONE top-level native item (the "water") at `i`: the user's code outside any system, passed
/// through verbatim except `@@Sys(...)` islands (spec §1103). Delegates to the SHARED
/// [`render_native_item`] the oracle also calls, so the machine and oracle differ only in loop
/// structure. Any non-native index (unreachable behind the `$Item` fork) renders nothing (total).
fn emit_native_item(
    src: &Source,
    syms: &SymbolTable,
    be: &dyn Backend,
    ast: &FileAst,
    i: usize,
    out: &mut Sink,
) {
    let Some(Item::Native(n)) = ast.items.get(i) else {
        return;
    };
    render_native_item(src, syms, be, n, prev_item_is_system(ast, i), out);
}

/// Emit ONE system item at `i` — the `$Item` non-native arm. Resolves the system's symbol and drives
/// the landed [`emit_system::walk`] phase spine (open → interface → handlers → actions → persist →
/// close). A non-system item (Bom/Pragma/Efsm) or an unresolved system emits nothing — exactly the
/// hand loop's `let Item::System(sys) = item else { continue }` / `else { continue }` skips.
fn emit_system_item(
    src: &Source,
    syms: &SymbolTable,
    be: &dyn Backend,
    ast: &FileAst,
    i: usize,
    out: &mut Sink,
) {
    let Some(Item::System(sys)) = ast.items.get(i) else {
        return;
    };
    let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
        return;
    };
    emit_system::walk(src, syms, sym, &sys.sections, be, out);
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
        emit_native_item, emit_system_item, is_native_item, Backend, FileAst, Sink, Source,
        SymbolTable,
    };
    include!("emit_file.gen.rs");
}

/// Emit every item in the file — the body of the public [`super::driver::emit`], driving the
/// `EmitFile` top walk. Emits the `file_header` preamble once (the native bookend), then hands the
/// header'd Sink to the machine (owned outright — the file owns its output from scratch, so no
/// `mem::take` is needed), drives the `$Item` cycle to fixpoint, and returns the finished String. The
/// bounded drive loop lives here — a broken machine cannot hang.
pub(super) fn walk(src: &Source, ast: &FileAst, syms: &SymbolTable, be: &dyn Backend) -> String {
    let mut out = Sink::new();
    be.file_header_ctx(syms.systems.iter().any(|s| s.scan.is_some()), &mut out);
    let mut m = fsm::EmitFile::new(src, ast, syms, be, ast.items.len(), out);
    // A safe over-bound: `$Item` fires once per item (each advances the cursor by one) plus the
    // terminal halt at `i >= n`.
    let bound = ast.items.len() + 4;
    for _ in 0..bound {
        m.step();
    }
    m.out.finish()
}
