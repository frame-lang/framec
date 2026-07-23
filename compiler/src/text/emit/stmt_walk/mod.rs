//! The handler/action **body statement walk**, dogfooded as a plain `@@system`
//! ([`stmt_walk.frs`]) — the emit-side transducer that reifies `emit_body`, and the first
//! machine to ride the **read-only borrowed domain** (the plain-`@@system` twin of a scanner's
//! `&'a [u8]`). The `StmtWalk` system carries the cursor, the one-bit `terminated` latch, and the
//! accumulating output; these native LEAVES hold the per-arm SPELLING sequences unchanged —
//! each is a byte-for-byte transcription of one `emit_body` match arm, so the system's output is
//! identical to the preserved [`super::driver::emit_body_hand`] oracle at every statement.
//!
//! The dispatch is a per-item function (a first-token classification carrying nothing beyond the
//! program counter, §3 degenerate pole), surfaced as [`kind_at`]; the machine content — the
//! terminated register read back to halt the walk and to choose the body terminal — lives in the
//! Frame states.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit stmt_walk.frs | grep -v '^#!\[allow' >
//! stmt_walk.gen.rs`.

use super::atom::Atom;
use super::driver::{has_lifecycle, lower_instantiation, Backend};
use super::reindent::{self, Lowering};
use super::Sink;
use crate::resolve::{SymbolTable, SystemSym};
use crate::text::Source;
use crate::tree::body::{EmbedCall, FrameRef, Instantiation, Stmt};

/// The Stmt variant at `i`, in the hand match's order: `-1` at end-of-slice (the walk's loop
/// bound), else `0`=Trivia, `1`=Native, `2`=Transition, `3`=StackPush, `4`=StackPopBare,
/// `5`=StackPop, `6`=Assign, `7`=ReturnCall, `8`=SelfCall, `9`=Forward. A per-item classifier —
/// the machine keys its dispatch on it. (Frame cannot match a Rust enum; this is the un-Frame-able
/// field access surfaced as one leaf.)
fn kind_at(stmts: &[Stmt], i: usize) -> i32 {
    match stmts.get(i) {
        None => -1,
        Some(s) => kind_of(s),
    }
}

/// The kind discriminant of one statement (0..9), in the hand match's order. Shared by
/// [`kind_at`] and by [`super::driver::body_parity_report`]'s coverage tally so the test proves it
/// exercised every variant with the SAME classifier the machine dispatches on.
pub(super) fn kind_of(s: &Stmt) -> i32 {
    match s {
        Stmt::Trivia(_) => 0,
        Stmt::Native(_) => 1,
        Stmt::Transition(_) => 2,
        Stmt::StackPush(_) => 3,
        Stmt::StackPopBare(_) => 4,
        Stmt::StackPop(_) => 5,
        Stmt::Assign(_) => 6,
        Stmt::ReturnCall(_) => 7,
        Stmt::SelfCall(_) => 8,
        Stmt::Forward(_) => 9,
    }
}

/// The [`Lowering`] the render leaves need — the three closures that expand a Frame ref, a system
/// instantiation, and an embed call. Reconstructed per render call (pure; identical output to the
/// hand walk's once-built `lower`). A macro so each caller owns the closures' borrows locally.
macro_rules! lowering {
    ($syms:expr, $sym:expr, $state:expr, $be:expr, $bind:ident) => {
        let reference = |r: &FrameRef| -> Atom { $be.lower_ref($sym, $state, r) };
        let instantiate = |inst: &Instantiation| -> Atom { lower_instantiation($syms, $be, inst) };
        let embed = |ec: &EmbedCall| -> Atom { $be.embed_call($sym, ec) };
        let $bind = Lowering {
            reference: &reference,
            instantiate: &instantiate,
            embed: &embed,
        };
    };
}

/// `Stmt::Native` — re-indent the whole native statement and spell it. (driver.rs Native arm.)
#[allow(clippy::too_many_arguments)]
fn emit_native(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) {
    if let Stmt::Native(n) = &stmts[i] {
        lowering!(syms, sym, state, be, lower);
        let r = n.logical_indent.saturating_sub(base);
        let delta = be.pad(r).len() as i32 - n.logical_indent as i32;
        let text = reindent::render_native(src, n, delta, &lower);
        be.native_stmt(r, text, out);
    }
}

/// `Stmt::Transition` — the exit->build->enter->return lifecycle. Returns whether a base-nesting
/// terminal fired (`depth == 0 && rel == 0`). (driver.rs Transition arm.)
#[allow(clippy::too_many_arguments)]
fn emit_transition(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) -> bool {
    if let Stmt::Transition(t) = &stmts[i] {
        if let Some(target) = &t.target {
            lowering!(syms, sym, state, be, lower);
            let r = t.col.saturating_sub(base);
            if has_lifecycle(sym, state, "<$") {
                let ea = reindent::render_args(src, t.exit_args.as_ref(), &lower);
                be.lifecycle_call(r, sym, state, "<$", ea.as_deref(), out);
            }
            let sa = reindent::render_args(src, t.args_text.as_ref(), &lower);
            be.transition(r, sym, target, sa.as_deref(), out);
            if has_lifecycle(sym, target, "$>") {
                let na = reindent::render_args(src, t.enter_args.as_ref(), &lower);
                be.lifecycle_call(r, sym, target, "$>", na.as_deref(), out);
            }
            be.terminate(r, out);
            return t.depth == 0 && r == 0;
        }
    }
    false
}

/// `Stmt::StackPush` — `push$ -> $T(args)` (transition, terminates) or bare `push$` (copy, stay).
/// Returns whether a base-nesting terminal fired. (driver.rs StackPush arm.)
#[allow(clippy::too_many_arguments)]
fn emit_stack_push(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) -> bool {
    if let Stmt::StackPush(t) = &stmts[i] {
        if let Some(target) = &t.target {
            lowering!(syms, sym, state, be, lower);
            let r = t.col.saturating_sub(base);
            if has_lifecycle(sym, state, "<$") {
                let ea = reindent::render_args(src, t.exit_args.as_ref(), &lower);
                be.lifecycle_call(r, sym, state, "<$", ea.as_deref(), out);
            }
            let sa = reindent::render_args(src, t.args_text.as_ref(), &lower);
            be.push(r, sym, target, sa.as_deref(), out);
            if has_lifecycle(sym, target, "$>") {
                let na = reindent::render_args(src, t.enter_args.as_ref(), &lower);
                be.lifecycle_call(r, sym, target, "$>", na.as_deref(), out);
            }
            be.terminate(r, out);
            return t.depth == 0 && r == 0;
        } else {
            be.push_bare(t.col.saturating_sub(base), out);
        }
    }
    false
}

/// Bare `pop$` — pop and DISCARD the top; stay. (driver.rs StackPopBare arm.)
fn emit_stack_pop_bare(be: &dyn Backend, base: u32, stmts: &[Stmt], i: usize, out: &mut Sink) {
    if let Stmt::StackPopBare(st) = &stmts[i] {
        be.pop_bare(st.col.saturating_sub(base), out);
    }
}

/// `-> pop$` — pop and RESTORE (a transition). Returns whether a base-nesting terminal fired.
/// (driver.rs StackPop arm.)
#[allow(clippy::too_many_arguments)]
fn emit_stack_pop(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) -> bool {
    if let Stmt::StackPop(st) = &stmts[i] {
        lowering!(syms, sym, state, be, lower);
        let r = st.col.saturating_sub(base);
        if has_lifecycle(sym, state, "<$") {
            let ea = reindent::render_args(src, st.exit_args.as_ref(), &lower);
            be.lifecycle_call(r, sym, state, "<$", ea.as_deref(), out);
        }
        be.pop(r, out);
        if st.enter_args.is_some() {
            let na = reindent::render_args(src, st.enter_args.as_ref(), &lower);
            be.pop_enter(r, sym, na.as_deref(), out);
        }
        be.terminate(r, out);
        return st.depth == 0 && r == 0;
    }
    false
}

/// `Stmt::Assign` — a FRAME assignment, terminated by the backend. (driver.rs Assign arm.)
#[allow(clippy::too_many_arguments)]
fn emit_assign(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) {
    if let Stmt::Assign(a) = &stmts[i] {
        lowering!(syms, sym, state, be, lower);
        let rhs = reindent::render_parts(src, &a.rhs, a.rhs_span, &lower);
        be.assign(sym, state, &a.lhs, rhs, a.col.saturating_sub(base), out);
    }
}

/// `@@:return(<expr>)` — set the return value AND exit. Returns whether a base-nesting terminal
/// fired. (driver.rs ReturnCall arm.)
#[allow(clippy::too_many_arguments)]
fn emit_return_call(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    is_async: bool,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) -> bool {
    if let Stmt::ReturnCall(r) = &stmts[i] {
        lowering!(syms, sym, state, be, lower);
        let e = reindent::render_parts(src, &r.expr, r.expr_span, &lower);
        be.return_call(r.col.saturating_sub(base), is_async, e, out);
        return r.depth == 0 && r.col.saturating_sub(base) == 0;
    }
    false
}

/// `@@:self.method(<args>)` — a reentrant interface call. (driver.rs SelfCall arm.)
#[allow(clippy::too_many_arguments)]
fn emit_self_call(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    be: &dyn Backend,
    base: u32,
    is_async: bool,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) {
    if let Stmt::SelfCall(c) = &stmts[i] {
        lowering!(syms, sym, state, be, lower);
        let a = reindent::render_args(src, Some(&c.args), &lower);
        be.self_call(c.col.saturating_sub(base), is_async, &c.method, a.as_deref().unwrap_or(""), out);
    }
}

/// `=> $^` — forward this event to the PARENT's handler, or a no-op if the parent does not handle
/// it. (driver.rs Forward arm.)
fn emit_forward(
    sym: &SystemSym,
    state: &str,
    event: &str,
    be: &dyn Backend,
    base: u32,
    stmts: &[Stmt],
    i: usize,
    out: &mut Sink,
) {
    if let Stmt::Forward(fwd) = &stmts[i] {
        if let Some(owner) = sym.resolve_forward(state, event) {
            let params = owner
                .handlers
                .iter()
                .find(|h| h.event == event)
                .map(|h| h.params_text.clone())
                .unwrap_or_default();
            be.forward(fwd.col.saturating_sub(base), &owner.name, event, &params, out);
        } else {
            be.noop(fwd.col.saturating_sub(base), out);
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
        emit_assign, emit_forward, emit_native, emit_return_call, emit_self_call, emit_stack_pop,
        emit_stack_pop_bare, emit_stack_push, emit_transition, kind_at, Backend, Sink, Source, Stmt,
        SymbolTable, SystemSym,
    };
    include!("stmt_walk.gen.rs");
}

/// Walk `stmts` through the `StmtWalk` transducer, seeding its output with `seed_out` (the
/// caller's Sink, so the body's text appends to the handler prologue already emitted). `base` is
/// the body's shallowest column. Returns the grown Sink and whether a base-nesting terminal fired
/// (which the caller turns into the body terminal). The bounded drive loop lives here, as in
/// [`super::super::scan::reachability`] — a broken machine cannot hang.
#[allow(clippy::too_many_arguments)]
pub(super) fn walk(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    stmts: &[Stmt],
    state: &str,
    event: &str,
    is_async: bool,
    base: u32,
    be: &dyn Backend,
    seed_out: Sink,
) -> (Sink, bool) {
    let mut m = fsm::StmtWalk::new(
        src, syms, sym, stmts, state, event, is_async, base, be, seed_out,
    );
    // Each $Walk step advances the cursor by one or halts, so the machine reaches $Done in at
    // most stmts.len()+1 steps; the slack covers the terminal step and any $Done no-ops.
    let bound = stmts.len() + 8;
    for _ in 0..bound {
        m.step();
    }
    (m.out, m.terminated)
}
