//! The driver's **handler-emission walk**, dogfooded as a plain `@@system`
//! ([`emit_handlers.frs`]) — the emit-side sequencer that reifies `emit`'s
//! `(section, state, handler)` nested pass (the private per-handler methods), the third emit-side
//! machine after [`super::stmt_walk`] and [`super::base_column`] and riding the same read-only
//! borrowed domain (`&'a [Section]`, `&'a SystemSym`, `&'a dyn Backend`, …).
//!
//! The 3-level nesting is a FIXED depth-3 walk, so it needs no stack: it is three NESTED CYCLE
//! STATES (`$Section` → `$State` → `$Handler`) with explicit up/down edges, one owned cursor per
//! level (`si`/`sti`/`hi`) plus its bound (`nsec`/`nst`/`nh`). framec owns the walk; the
//! un-Frame-able work is the native LEAVES here: the structural forks/bounds ([`is_machine_section`],
//! [`member_count`], [`is_state_member`], [`state_member_count`], [`is_handler_member`] — Frame
//! cannot match a Rust enum), the two per-handler forks ([`handler_is_async`], [`handler_ret`]), and
//! [`emit_handler`], which spells one private method by calling `be.open_handler`, the StmtWalk body
//! walk ([`super::driver::emit_body`], unchanged, as a leaf), then `be.close_handler`.
//!
//! The byte-for-byte ORACLE it replaced is preserved as [`super::driver::emit_handlers_hand`] and
//! gated in `tests/emit_handlers.rs` (GATE-A, via [`super::driver::handlers_parity_report`]).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit emit_handlers.frs | grep -v '^#!\[allow' >
//! emit_handlers.gen.rs`.

use super::driver::{body_is_empty, emit_body, Backend, BodyRole};
use super::Sink;
use crate::resolve::{SymbolTable, SystemSym};
use crate::text::Source;
use crate::tree::{HandlerNode, MachineMember, Section, StateMember, StateNode};

/// Is `sections[si]` a `Section::Machine`? The `$Section` fork — only a machine section descends
/// into states (the hand walk's `let Section::Machine(mach) = sec else { continue }`). Out of
/// bounds is `false` (the `$Section` bound `si >= nsec` halts first, but this stays total).
fn is_machine_section(sections: &[Section], si: usize) -> bool {
    matches!(sections.get(si), Some(Section::Machine(_)))
}

/// The number of `MachineMember`s in the machine section at `si` — the `$State` bound `nst`, set on
/// descent. `0` for a non-machine or out-of-bounds section (never descended into).
fn member_count(sections: &[Section], si: usize) -> usize {
    match sections.get(si) {
        Some(Section::Machine(m)) => m.members.len(),
        _ => 0,
    }
}

/// Is `sections[si].members[sti]` a `MachineMember::State`? The `$State` fork — only a state
/// descends into handlers (the hand walk's `let MachineMember::State(st) = mm else { continue }`).
fn is_state_member(sections: &[Section], si: usize, sti: usize) -> bool {
    match sections.get(si) {
        Some(Section::Machine(m)) => matches!(m.members.get(sti), Some(MachineMember::State(_))),
        _ => false,
    }
}

/// The number of `StateMember`s in the state at `(si, sti)` — the `$Handler` bound `nh`, set on
/// descent. `0` for a non-state member or an out-of-bounds index.
fn state_member_count(sections: &[Section], si: usize, sti: usize) -> usize {
    if let Some(Section::Machine(m)) = sections.get(si) {
        if let Some(MachineMember::State(st)) = m.members.get(sti) {
            return st.members.len();
        }
    }
    0
}

/// Is `sections[si].members[sti].members[hi]` a `StateMember::Handler`? The `$Handler` fork — only a
/// handler is emitted (the hand walk's `let StateMember::Handler(h) = member else { continue }`).
fn is_handler_member(sections: &[Section], si: usize, sti: usize, hi: usize) -> bool {
    if let Some(Section::Machine(m)) = sections.get(si) {
        if let Some(MachineMember::State(st)) = m.members.get(sti) {
            return matches!(st.members.get(hi), Some(StateMember::Handler(_)));
        }
    }
    false
}

/// Which state-member the `$Handler` cycle visits at loop SLOT `hi`.
///
/// The cycle is a plain 0..`nh` walk over `st.members`, so "slot" and "member index" are the same
/// thing — except on a backend that orders handlers the way the shipped compiler does
/// ([`super::driver::handler_sort_key`]: handler-key order, not declaration order). There the two
/// differ, and this is the projection between them.
///
/// It PERMUTES THE HANDLER SLOTS AMONG THEMSELVES and leaves every other member (trivia, state
/// vars) where it is. That is what keeps the walk's structural forks honest: `is_handler_member`
/// asks "is slot `hi` a handler?" against the unpermuted list, and stays right, because a handler
/// slot can only map to another handler slot.
fn member_slot(sections: &[Section], si: usize, sti: usize, hi: usize, be: &dyn Backend) -> usize {
    if !be.orders_handlers_by_key() {
        return hi;
    }
    let Some(Section::Machine(m)) = sections.get(si) else {
        return hi;
    };
    let Some(MachineMember::State(st)) = m.members.get(sti) else {
        return hi;
    };
    // Where the handlers sit in declaration order …
    let slots: Vec<usize> = (0..st.members.len())
        .filter(|&i| matches!(st.members.get(i), Some(StateMember::Handler(_))))
        .collect();
    // … and the same set, in the shipped compiler's key order.
    let mut sorted = slots.clone();
    sorted.sort_by(|&a, &b| {
        let ka = match &st.members[a] {
            StateMember::Handler(h) => super::driver::handler_sort_key(&h.event),
            _ => "",
        };
        let kb = match &st.members[b] {
            StateMember::Handler(h) => super::driver::handler_sort_key(&h.event),
            _ => "",
        };
        ka.cmp(kb)
    });
    match slots.iter().position(|&s| s == hi) {
        Some(nth) => sorted[nth],
        None => hi,
    }
}

/// The `(state, handler)` node pair at slot `(si, sti, hi)`, or `None` when any index misses (a
/// non-machine section, non-state member, or non-handler state-member). The single un-Frame-able
/// descent through the three heterogeneous slices, shared by the three per-handler leaves so they
/// agree on what "the handler at `(si, sti, hi)`" is — including WHICH handler that slot means,
/// which is [`member_slot`]'s answer and therefore the backend's.
fn handler_at<'a>(
    sections: &'a [Section],
    si: usize,
    sti: usize,
    hi: usize,
    be: &dyn Backend,
) -> Option<(&'a StateNode, &'a HandlerNode)> {
    if let Some(Section::Machine(m)) = sections.get(si) {
        if let Some(MachineMember::State(st)) = m.members.get(sti) {
            if let Some(StateMember::Handler(h)) = st.members.get(member_slot(sections, si, sti, hi, be)) {
                return Some((st, h));
            }
        }
    }
    None
}

/// The handler's `is_async` — IT says so, or the SYSTEM does (`@@[async]`). The exact disjunction
/// `emit` computes (`sym.is_async || sym.interface.iter().any(|m| m.name == h.event && m.is_async)`),
/// surfaced as a `$Handler` leaf.
fn handler_is_async(
    sym: &SystemSym,
    sections: &[Section],
    si: usize,
    sti: usize,
    hi: usize,
    be: &dyn Backend,
) -> bool {
    match handler_at(sections, si, sti, hi, be) {
        Some((_, h)) => sym.is_async || sym.interface.iter().any(|m| m.name == h.event && m.is_async),
        None => false,
    }
}

/// The handler's inherited RETURN type: its own `h.return_text`, else the interface method's
/// (`.or_else`). The exact fork `emit` computes, surfaced as a `$Handler` leaf. Borrows the `'a`
/// tree/symbol data (the returned `&'a str` lives in `h` or in `sym.interface`), not `self`.
fn handler_ret<'a>(
    sym: &'a SystemSym,
    sections: &'a [Section],
    si: usize,
    sti: usize,
    hi: usize,
    be: &dyn Backend,
) -> Option<&'a str> {
    let (_, h) = handler_at(sections, si, sti, hi, be)?;
    h.return_text.as_deref().or_else(|| {
        sym.interface
            .iter()
            .find(|m| m.name == h.event)
            .and_then(|m| m.return_text.as_deref())
    })
}

/// Emit ONE private `(state, handler)` method: `open_handler`, the StmtWalk body walk
/// ([`emit_body`], the production path — unchanged, NOT reinlined), then `close_handler` with the
/// body's `terminated` bit. The verbatim per-handler spelling of the hand walk's loop body;
/// `is_async` + `ret` are computed by the [`handler_is_async`]/[`handler_ret`] leaves and threaded
/// in, so this leaf is a pure materialization sequence.
#[allow(clippy::too_many_arguments)]
fn emit_handler(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    be: &dyn Backend,
    sections: &[Section],
    si: usize,
    sti: usize,
    hi: usize,
    is_async: bool,
    ret: Option<&str>,
    out: &mut Sink,
) {
    if let Some((st, h)) = handler_at(sections, si, sti, hi, be) {
        // The standalone user COMMENTS that lead this handler in source — the trivia between it
        // and the previous handler (or the state open). They travel WITH the handler, so on a
        // backend that emits handlers in key order they land before this handler wherever it sorts
        // to, reindented to the member column. `member_slot` is the handler's SOURCE slot (the same
        // one `handler_at` resolved), which is what the leading-comment gap is measured against.
        let slot = member_slot(sections, si, sti, hi, be);
        let lead = super::driver::handler_leading_comments(src, sections, si, sti, slot);
        if !lead.is_empty() {
            be.member_comment(&lead, out);
        }
        be.open_handler(sym, &st.name, &h.event, &h.params_text, ret, is_async, out);
        // A body that emits NOTHING (all-`Trivia`, all-comment, or empty) still owes the target a
        // statement on an indent-delimited language. The fact is read from the TREE
        // ([`body_is_empty`]), never from the text just written, and the spelling is the backend's
        // `noop` (nothing at all on a brace target).
        //
        // [`Backend::empty_body_keeps_text`] decides whether the body's own text is emitted first:
        // the shipped compiler drops a comment-only body's comments and emits the bare `pass`
        // (a comment was never a segment in its body model). Python reproduces that; every other
        // target keeps its bytes.
        let empty = body_is_empty(&h.body);
        let end = if !empty || be.empty_body_keeps_text() {
            emit_body(
                src,
                syms,
                sym,
                BodyRole::Handler,
                &st.name,
                &h.event,
                is_async,
                &h.body,
                be,
                out,
            )
        } else {
            super::driver::BodyEnd::Fell
        };
        if empty && !super::driver::handler_emits_bindings(sym, &st.name, &h.params_text) {
            be.noop(0, out);
        }
        be.close_handler(ret, is_async, end.terminated(), &super::driver::LeafCtx::new(sym, &h.event, &st.name), out);
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
        emit_handler, handler_is_async, handler_ret, is_handler_member, is_machine_section,
        is_state_member, member_count, state_member_count, Backend, Section, Sink, Source,
        SymbolTable, SystemSym,
    };
    include!("emit_handlers.gen.rs");
}

/// Emit every private `(state, handler)` method of a system, driving the `EmitHandlers` sequencer.
/// Seeds the machine's owned `out` with the caller's Sink (the interface routes already emitted, so
/// handler text appends exactly where the hand walk appended it — `std::mem::take`, as
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
    let mut m = fsm::EmitHandlers::new(src, syms, sym, sections, be, sections.len(), seed);
    // A safe over-bound on the number of steps: each `step()` advances exactly one cursor by one
    // (or descends/ascends/halts), so the walk visits each section, each machine member, and each
    // state-member once, plus one descent per machine section and per state, plus the terminal.
    // Computing it is a cheap structural sum (no emission).
    let mut bound = sections.len() + 8;
    for sec in sections {
        if let Section::Machine(mach) = sec {
            bound += mach.members.len() + 1;
            for mm in &mach.members {
                if let MachineMember::State(st) = mm {
                    bound += st.members.len() + 1;
                }
            }
        }
    }
    for _ in 0..bound {
        m.step();
    }
    *out = m.out;
}
