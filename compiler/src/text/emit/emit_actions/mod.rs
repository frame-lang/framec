//! The driver's **actions/operations walk**, dogfooded as a plain `@@system`
//! ([`emit_actions.frs`]) — the emit-side sequencer that reifies `emit`'s `actions:` / `operations:`
//! pass (one method per user-bodied member; the signature is Frame's, the body the user's), the
//! fifth emit-side machine after [`super::stmt_walk`], [`super::base_column`],
//! [`super::emit_handlers`], and [`super::emit_interface`], and riding the same read-only borrowed
//! domain (`&'a [Section]`, `&'a SystemSym`, `&'a dyn Backend`, …).
//!
//! The 2-level nesting is a FIXED depth-2 walk, so it needs no stack: it is two NESTED CYCLE STATES
//! (`$Section` → `$Member`) with explicit up/down edges, one owned cursor per level (`si`/`mi`) plus
//! its bound (`nsec`/`nm`). framec owns the walk; the un-Frame-able work is the native LEAVES here:
//! the structural forks/bounds ([`is_action_section`], [`action_member_count`],
//! [`is_withbody_member`] — Frame cannot match a Rust enum), and [`emit_action`], which spells one
//! method by calling `be.open_action`, the StmtWalk body walk ([`super::driver::emit_body`],
//! unchanged, as a leaf — its `BodyEnd` discarded exactly as the hand pass discarded it), then
//! `be.close_action`.
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::emit_actions_hand`] and
//! gated in `tests/emit_actions.rs` (GATE-A, via [`super::driver::actions_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit emit_actions.frs | grep -v '^#!\[allow' >
//! emit_actions.gen.rs`.

use super::driver::{action_section_in_phase, body_is_empty, emit_body, Backend, BodyRole};
use super::Sink;
use crate::resolve::{SymbolTable, SystemSym};
use crate::text::Source;
use crate::tree::{Decl, Section};

/// Is `sections[si]` a `Section::Actions` or `Section::Operations`? The `$Section` fork — only such
/// a section descends into members (the hand pass's `let (Section::Actions(d) | Section::Operations(d))
/// = sec else { continue }`). Out of bounds is `false` (the `$Section` bound `si >= nsec` halts
/// first, but this stays total).
fn is_action_section(sections: &[Section], si: usize, phase: usize, nphase: usize) -> bool {
    sections
        .get(si)
        .and_then(|s| action_section_in_phase(s, phase, nphase))
        .is_some()
}

/// The number of `Decl` members in the actions/operations section at `si` — the `$Member` bound
/// `nm`, set on descent. `0` for any other section kind or an out-of-bounds index (never descended
/// into).
fn action_member_count(sections: &[Section], si: usize, phase: usize, nphase: usize) -> usize {
    sections
        .get(si)
        .and_then(|s| action_section_in_phase(s, phase, nphase))
        .map(|d| d.members.len())
        .unwrap_or(0)
}

/// Is `sections[si].members[mi]` a `Decl::WithBody`? The `$Member` fork — only a bodied member is
/// emitted (the hand pass's `let Decl::WithBody(b) = m else { continue }`); a bare signature or
/// trivia decl is skipped.
fn is_withbody_member(
    sections: &[Section],
    si: usize,
    mi: usize,
    phase: usize,
    nphase: usize,
) -> bool {
    sections
        .get(si)
        .and_then(|s| action_section_in_phase(s, phase, nphase))
        .map(|d| matches!(d.members.get(mi), Some(Decl::WithBody(_))))
        .unwrap_or(false)
}

/// Emit ONE `actions:`/`operations:` method at `(si, mi)`: `open_action` (Frame's signature), the
/// StmtWalk body walk ([`emit_body`], the production path — unchanged, NOT reinlined; its `BodyEnd`
/// is discarded exactly as the hand pass discarded it, because an action has no fallback-return
/// obligation), then `close_action`. The verbatim per-member spelling of the hand walk's loop body.
/// Any index miss (non-action section, non-bodied member) emits nothing (total).
fn emit_action(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    be: &dyn Backend,
    sections: &[Section],
    si: usize,
    mi: usize,
    phase: usize,
    nphase: usize,
    out: &mut Sink,
) {
    let Some(d) = sections
        .get(si)
        .and_then(|s| action_section_in_phase(s, phase, nphase))
    else {
        return;
    };
    let Some(Decl::WithBody(b)) = d.members.get(mi) else {
        return;
    };
    // `operations:` members are `pub`, `actions:` are private — categorical by section. The phase
    // walk collapses both to the same inner DeclSection, so read the original Section here.
    let is_operation = matches!(sections.get(si), Some(Section::Operations(_)));
    be.open_action(&b.name, &b.params_text, b.return_text.as_deref(), is_operation, out);
    // A body with NO executable statement (empty, or only comments) still owes an
    // indent-delimited target a statement — `def f(self):` with nothing under it is a
    // SyntaxError, and `def f(self):` followed only by a comment is an IndentationError. The
    // fact is read from the TREE ([`body_is_empty`], which asks `LiteralNode::is_comment` — a
    // fact the SCANNER put there), never from the text just written, and the spelling is the
    // backend's `noop` (nothing at all on a brace target, so no brace target's bytes move).
    //
    // Same shape as [`super::emit_handlers::emit_handler`]. The one extra move is
    // [`Backend::empty_body_keeps_text`]: the shipped compiler DROPS a comment-only body's
    // comments and emits the bare `pass`, because its body model held statement segments and a
    // comment was never one. Python reproduces that; every other target keeps its bytes.
    let empty = body_is_empty(&b.body);
    if !empty || be.empty_body_keeps_text() {
        emit_body(src, syms, sym, BodyRole::Action, "", "", false, &b.body, be, out);
    }
    if empty {
        be.noop(0, out);
    }
    be.close_action(out);
}

/// Emit the standalone user COMMENT carried by the `actions:`/`operations:` member at `(si, mi)`
/// when it is a `Decl::Trivia` holding one (a comment BETWEEN or BEFORE bodied members).
/// Whitespace-only trivia, a bare signature, or any out-of-phase section emits nothing. Actions
/// are emitted in SOURCE order (never key-sorted), so a comment at its declaration slot lands
/// before the member that follows it — reindented to the backend's member column, matching the
/// shipped compiler.
#[allow(clippy::too_many_arguments)]
fn emit_action_trivia(
    src: &Source,
    be: &dyn Backend,
    sections: &[Section],
    si: usize,
    mi: usize,
    phase: usize,
    nphase: usize,
    out: &mut Sink,
) {
    if let Some(d) = sections
        .get(si)
        .and_then(|s| action_section_in_phase(s, phase, nphase))
    {
        if let Some(Decl::Trivia(t)) = d.members.get(mi) {
            let lines = super::driver::comment_lines(src, t.span);
            if !lines.is_empty() {
                be.actions_comment(&lines, out);
            }
        }
    }
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
        action_member_count, emit_action, emit_action_trivia, is_action_section,
        is_withbody_member, Backend, Section, Sink, Source, SymbolTable, SystemSym,
    };
    include!("emit_actions.gen.rs");
}

/// Emit every `actions:`/`operations:` method of a system, driving the `EmitActions` sequencer.
/// Seeds the machine's owned `out` with the caller's Sink (`std::mem::take`, as
/// [`super::driver::emit_body`] does around StmtWalk), drives to fixpoint, and writes the grown Sink
/// back. The bounded drive loop lives here — a broken machine cannot hang.
pub(super) fn walk(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    let seed = std::mem::take(out);
    // Two PASSES when the target orders actions before operations, one otherwise — see
    // [`Backend::orders_actions_before_operations`]. The count is FIXED and known before the walk,
    // which is why the machine carries it as a bound (`nphase`) and a cursor (`phase`) rather than
    // as states.
    let nphase = if be.orders_actions_before_operations() { 2 } else { 1 };
    let mut m = fsm::EmitActions::new(src, syms, sym, sections, be, sections.len(), nphase, seed);
    // A safe over-bound on the number of steps: each `step()` advances one cursor by one (or
    // descends/ascends/halts), so the walk visits each section once plus, per actions/operations
    // section, each member once and one descent. Computing it is a cheap structural sum (no
    // emission).
    let mut bound = 8;
    for _ in 0..nphase {
        bound += sections.len() + 1;
        for sec in sections {
            if let Section::Actions(d) | Section::Operations(d) = sec {
                bound += d.members.len() + 1;
            }
        }
    }
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
