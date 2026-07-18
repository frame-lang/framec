//! Decompose the sections: `machine:` -> states -> handlers -> **bodies**.
//!
//! This is the pass the old compiler never had. It had an AST of the system *skeleton*
//! and, below a handler's opening brace, a flat segment stream with native code as an
//! opaque `String`. All twenty-five shipped bugs live below that line.
//!
//! Everything here **partitions**: a state's members cover the state; a handler's body
//! covers the bytes between its braces; a native statement's parts cover the statement.
//! There is no "just formatting" and there is no blob.

use super::lex::Lexer;
use super::literals::Target;
use super::parts::native_parts;
use crate::tree::body::{
    AssignStmt, Body, FrameRef, NativeStmt, ReturnCallStmt, SelfCallStmt, SimpleStmt, Stmt,
    TransitionStmt,
};
use crate::tree::{
    BodyDecl, Decl, DeclSection, FrameSpan, HandlerNode, MachineMember, MachineSection, MemberDecl,
    StateMember, StateNode, TriviaNode,
};
use crate::Span;

/// A `machine:` section: states, and the trivia between them. **Now a native driver over the
/// dogfooded `MachineWalk` system** (`machine_walk::state_starts`): the system finds the `$Name`
/// state-start offsets (skipping opaque + each state body); this driver builds the Trivia + State
/// nodes from them. The boundary the system finds and the extent `state()` carries share one
/// source (`state_extent`), so they cannot drift. (Retires the hand state-dispatch loop; its
/// oracle survives as `machine_walk::state_starts_hand`.)
pub fn machine_section(lx: &Lexer, bytes: &[u8], span: Span, kw: Span) -> MachineSection {
    let starts = super::machine_walk::state_starts(bytes, kw.end, span.end, lx.target());
    let mut members = Vec::new();
    let mut cursor = kw.end;

    for &start in &starts {
        if cursor < start {
            members.push(MachineMember::Trivia(TriviaNode {
                span: Span::new(cursor, start),
            }));
        }
        let st = state(lx, bytes, start, span.end);
        cursor = st.span.end;
        members.push(MachineMember::State(st));
    }
    if cursor < span.end {
        members.push(MachineMember::Trivia(TriviaNode {
            span: Span::new(cursor, span.end),
        }));
    }

    MachineSection {
        span,
        keyword_node: FrameSpan {
            span: kw,
            kind: "Keyword",
        },
        members,
    }
}

/// `$Name(params) { …handlers… }`
fn state(lx: &Lexer, bytes: &[u8], at: usize, limit: usize) -> StateNode {
    // Name, opening `{`, and its matching `}` — all from the shared `state_extent` (the SAME
    // source the `MachineWalk` system's `state_end` leaf uses), so the boundary the walk finds and
    // the extent this node carries cannot drift, and the name-skip is done exactly once.
    let (name_end, open, end) = state_extent(bytes, at, limit, lx.target());
    let close = end.saturating_sub(1);
    let name = String::from_utf8_lossy(&bytes[at + 1..name_end]).into_owned();

    // `$B(n: int, m: str)` — the declared parameter NAMES.
    let mut params = Vec::new();
    let mut param_types = std::collections::HashMap::new();
    if name_end < limit && bytes[name_end] == b'(' {
        if let Some(pe) = balanced(lx, bytes, name_end, limit, b'(', b')') {
            let inner = String::from_utf8_lossy(&bytes[name_end + 1..pe.saturating_sub(1)]).into_owned();
            for (n, t) in super::super::emit::driver::params_split(&inner) {
                if let Some(t) = t {
                    param_types.insert(n.clone(), t);
                }
                params.push(n);
            }
        }
    }

    // `=> $Parent` — the state's parent. This is the whole of HSM.
    let mut parent = None;
    {
        let mut k = name_end;
        while k < limit && bytes[k] != b'{' && bytes[k] != b'\n' {
            if starts(bytes, k, b"=>", limit) {
                let mut p = k + 2;
                while p < limit && (bytes[p] == b' ' || bytes[p] == b'\t') {
                    p += 1;
                }
                if p < limit && bytes[p] == b'$' && is_name_start(bytes, p + 1) {
                    let ps = p + 1;
                    let mut pe = ps;
                    while pe < limit
                        && (bytes[pe].is_ascii_alphanumeric() || bytes[pe] == b'_')
                    {
                        pe += 1;
                    }
                    parent = Some(String::from_utf8_lossy(&bytes[ps..pe]).into_owned());
                }
                break;
            }
            k += 1;
        }
    }

    // The state members — **now a native driver over the dogfooded `StateWalk` system**
    // (`state_walk::member_starts`): the system finds the member-start offsets (a `$.x` state var
    // or a handler head), skipping opaque + each member's extent; this driver builds the Trivia +
    // StateVar/Handler nodes. Each member's extent is re-derived from the SAME shared source the
    // walk used (`to_end_of_line` / `handler_head`), so they cannot drift.
    let starts = super::state_walk::member_starts(bytes, open + 1, close, lx.target());
    let mut members = Vec::new();
    let mut cursor = open + 1;

    for &start in &starts {
        if cursor < start {
            members.push(StateMember::Trivia(TriviaNode {
                span: Span::new(cursor, start),
            }));
        }
        if bytes[start] == b'$' && bytes.get(start + 1) == Some(&b'.') {
            // `$.name: T = init` — a state variable (Frame's own declaration).
            let e = to_end_of_line(bytes, start, close);
            members.push(StateMember::StateVar(decl_of(bytes, start + 2, e, start)));
            cursor = e;
        } else if let Some(h) = handler_at(lx, bytes, start, close) {
            // A handler: `name(...) {` / `$>() {` / `<$() {`.
            cursor = h.span.end;
            members.push(StateMember::Handler(h));
        }
    }
    if cursor < close {
        members.push(StateMember::Trivia(TriviaNode {
            span: Span::new(cursor, close),
        }));
    }

    StateNode {
        span: Span::new(at, end),
        name,
        params,
        param_types,
        parent,
        header_node: FrameSpan {
            span: Span::new(at, open + 1),
            kind: "StateHeader",
        },
        members,
        close_node: FrameSpan {
            span: Span::new(close, end),
            kind: "Close",
        },
    }
}

/// The parsed HEADER of a handler starting at `i` (if one is there): the event name, the param
/// group `(...)`, an optional `: T` return type, the opening `{`, and the body end. **Single
/// source** — `handler_at` builds the node from it AND the `StateWalk` system's `handler_end`
/// leaf reads its `.end`, so the member boundary the walk finds and the extent the node carries
/// cannot drift. Takes `target` (not a `Lexer`) so the leaf can call it. `None` if no handler
/// opens here (no event name, no `(`, or no `{`).
struct HandlerHead {
    name: String,
    params_open: usize,
    params_close: usize,
    return_text: Option<String>,
    open: usize,
    end: usize,
}

fn handler_head(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<HandlerHead> {
    // The event name: an identifier, or `$>` (enter) / `<$` (exit).
    let (name, mut j) = if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'>') {
        ("$>".to_string(), i + 2)
    } else if bytes[i] == b'<' && bytes.get(i + 1) == Some(&b'$') {
        ("<$".to_string(), i + 2)
    } else if is_name_start(bytes, i) {
        let mut k = i;
        while k < limit && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
            k += 1;
        }
        (String::from_utf8_lossy(&bytes[i..k]).into_owned(), k)
    } else {
        return None;
    };

    // A `(` must follow (after optional space), then eventually a `{`.
    while j < limit && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    if j >= limit || bytes[j] != b'(' {
        return None;
    }
    let params_open = j;
    let params_close = super::delim_balance::balanced(bytes, j, limit, b'(', b')', target)?;
    let mut k = params_close;
    // Optional return type `: T`, then the opening brace. The `: T` SYNTAX is Frame's;
    // the type text is the USER's and is carried verbatim.
    let mut return_text = None;
    while k < limit && (bytes[k] == b' ' || bytes[k] == b'\t') {
        k += 1;
    }
    if k < limit && bytes[k] == b':' {
        k += 1;
        let ts = k;
        while k < limit && bytes[k] != b'{' && bytes[k] != b'\n' {
            k += 1;
        }
        let t = String::from_utf8_lossy(&bytes[ts..k]).trim().to_string();
        if !t.is_empty() {
            return_text = Some(t);
        }
    }
    while k < limit && bytes[k] != b'{' && bytes[k] != b'\n' {
        k += 1;
    }
    if k >= limit || bytes[k] != b'{' {
        return None;
    }
    let open = k;
    let end = super::delim_balance::balanced(bytes, open, limit, b'{', b'}', target).unwrap_or(limit);
    Some(HandlerHead {
        name,
        params_open,
        params_close,
        return_text,
        open,
        end,
    })
}

/// The offset one past the handler that starts at `i`, or `None` if no handler opens here — the
/// extent half of [`handler_head`], exposed (hiding the private struct) for the `StateWalk`
/// system's member walk, which skips a handler body to find the next member.
pub(crate) fn handler_end(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<usize> {
    handler_head(bytes, i, limit, target).map(|h| h.end)
}

/// A handler starting at `i`, if there is one — built from the shared [`handler_head`].
fn handler_at(lx: &Lexer, bytes: &[u8], i: usize, limit: usize) -> Option<HandlerNode> {
    let h = handler_head(bytes, i, limit, lx.target())?;
    let close = h.end.saturating_sub(1);
    Some(HandlerNode {
        span: Span::new(i, h.end),
        event: h.name,
        params_text: String::from_utf8_lossy(
            &bytes[h.params_open + 1..h.params_close.saturating_sub(1)],
        )
        .into_owned(),
        return_text: h.return_text,
        header_node: FrameSpan {
            span: Span::new(i, h.open + 1),
            kind: "HandlerHeader",
        },
        // *** THE TREE THE OLD COMPILER DID NOT HAVE ***
        body: body(lx, bytes, Span::new(h.open + 1, close)),
        close_node: FrameSpan {
            span: Span::new(close, h.end),
            kind: "Close",
        },
    })
}

/// A handler body: statements, and the trivia between them. **Partitions the span.**
///
/// **Now a native driver over the dogfooded `BodyWalk` system** (`body_walk::stmt_starts`): the
/// system finds each Frame-statement start paired with the brace **depth** there (skipping opaque
/// regions + each statement's extent, and counting `{`/`}` of native water into a running depth),
/// plus the final depth at `span.end`. This driver builds the Native gaps + Frame-statement nodes
/// from those positions. The statement extent the walk used and the node this driver builds share
/// one source (`frame_call_end`/`frame_assign_end`/`stmt_scan::classify` vs the builders), so they
/// cannot drift. Retires the hand statement-dispatch loop AND the brace counter (a stateful
/// counter that had to ride the walk — a native driver would need its own hand brace-loop, which
/// guardrail-4 forbids); the oracle survives as `body_walk::stmt_starts_hand`.
///
/// Depth is a NUMBER, never a KIND: framec counts braces; it never asks whether the block is an
/// `if`, a `while`, or a lambda — that would be a parse of native code, which framec does not do.
pub fn body(lx: &Lexer, bytes: &[u8], span: Span) -> Body {
    let (starts, final_depth) =
        super::body_walk::stmt_starts(bytes, span.start, span.end, lx.target());
    let mut stmts = Vec::new();
    let mut cursor = span.start;

    for &(start, depth) in &starts {
        let col = column_of(bytes, start, 0);
        // Re-derive + build in body()'s order (frame_call → frame_assign → frame_stmt). The walk
        // recorded `start` because one of these opens there (via the shared extent heads), so one
        // returns `Some`. `depth` is the recorded brace depth at `start` — used for BOTH the native
        // gap before the statement and the statement node's own `depth` field.
        let st = frame_call(lx, bytes, start, span.end, depth, col)
            .or_else(|| frame_assign(lx, bytes, start, span.end, col))
            .or_else(|| frame_stmt(bytes, start, span.end, depth, col));
        if let Some(st) = st {
            let sp = stmt_span(&st);
            push_native(lx, bytes, &mut stmts, cursor, sp.start, span.start, depth, target(lx));
            stmts.push(st);
            cursor = sp.end;
        }
    }
    push_native(lx, bytes, &mut stmts, cursor, span.end, span.start, final_depth, target(lx));

    Body { span, stmts }
}

/// Does this target CONSUME `block_depth`?
///
/// Only two consumers exist, and neither cares what kind of block it is:
///
/// * **unreachable-code suppression** after a transition's implicit `return` — and
///   Java is essentially alone, being the only target where dead code is a *compile
///   error*;
/// * **Python/GDScript indentation.**
///
/// Everywhere else the field is honestly `None`. That is not laziness — it is the
/// only correct answer for **Ruby**, where `x = 1 if y` (a modifier, no `end`) and
/// `if y … end` (a block) are the **same token sequence** in different grammatical
/// positions. No lexer can depth-count Ruby, and framec does not parse Ruby. Ruby does
/// not consume the field, so Ruby never needs it.
///
/// **Where framec cannot know, it says so. It does not guess.** A guess is what
/// produced the bug family.
fn depth_is_knowable(t: super::literals::Target) -> bool {
    use super::literals::Target::*;
    match t {
        // Brace targets: counting `{`/`}` tokens is exact.
        Java | C | Cpp | CSharp | Kotlin | Swift | Go | Rust | JavaScript | TypeScript | Dart
        | Php => true,
        // Indent targets: the depth is the indent level.
        Python3 | GdScript => true,
        // Word-delimited blocks. A lexer cannot do it, and we do not pretend.
        Ruby | Lua => false,
    }
}

fn target(lx: &Lexer) -> super::literals::Target {
    lx.target()
}

/// Everything between Frame statements is native. It is **delimited, never
/// interpreted** — but it is a CONTAINER: its string literals and its Frame refs are
/// nodes, because framec must know where the literals are in order to leave them
/// alone, and where the refs are in order to splice them.
#[allow(clippy::too_many_arguments)]
fn push_native(
    lx: &Lexer,
    bytes: &[u8],
    out: &mut Vec<Stmt>,
    from: usize,
    to: usize,
    body_start: usize,
    depth: u32,
    tgt: super::literals::Target,
) {
    if from >= to {
        return;
    }
    // Leading/trailing whitespace is trivia, not code. (It is still a node — the
    // terminator bug lived in exactly this whitespace, when framec spliced a `;`
    // inside a trailing comment because the comment was not a node.)
    let mut a = from;
    while a < to && bytes[a].is_ascii_whitespace() {
        a += 1;
    }
    if a > from {
        out.push(Stmt::Trivia(TriviaNode {
            span: Span::new(from, a),
        }));
    }
    if a >= to {
        return;
    }
    let mut z = to;
    while z > a && bytes[z - 1].is_ascii_whitespace() {
        z -= 1;
    }
    out.push(Stmt::Native(NativeStmt {
        span: Span::new(a, z),
        parts: native_parts(lx, bytes, a, z),
        // The statement's column. RENDER's re-indent basis: the emitted method sits at
        // the TARGET's nesting depth, while these bytes were written at FRAME's — so
        // something must re-indent, and it needs to know by how much.
        //
        // The old compiler's `normalize_indentation` did it as a post-emission
        // `.lines()` / `.min()` / slice pass over already-generated text, with no idea
        // where anything was — which is why it stripped the margin off lines INSIDE
        // string literals and silently changed the VALUE of the user's string (#215).
        logical_indent: column_of(bytes, a, body_start),
        block_depth: if depth_is_knowable(tgt) {
            Some(depth)
        } else {
            None
        },
    }));
    if z < to {
        out.push(Stmt::Trivia(TriviaNode {
            span: Span::new(z, to),
        }));
    }
}

/// Classify a Frame statement at `i` as `(kind, end)` — the reference the dogfooded
/// `StmtScan` system is proven against. kind: 0=none(native) 1=Transition 2=StackPush
/// 3=StackPop 4=Forward.
pub fn frame_stmt_classify(bytes: &[u8], i: usize, limit: usize) -> (i32, usize) {
    match frame_stmt_hand(bytes, i, limit, 0, 0) {
        Some(Stmt::Transition(t)) => (1, t.span.end),
        Some(Stmt::StackPush(t)) => (2, t.span.end),
        Some(Stmt::StackPop(s)) => (3, s.span.end),
        Some(Stmt::Forward(s)) => (4, s.span.end),
        Some(Stmt::StackPopBare(s)) => (5, s.span.end),
        _ => (0, i),
    }
}

/// Leaves for `StmtScan` — the exact hand sub-logic, reused so there is no drift.
pub fn stmt_eol(bytes: &[u8], i: usize, limit: usize) -> usize {
    to_end_of_line(bytes, i, limit)
}
/// The offset one past the balanced `(...)` at `i`, or `i` if unbalanced. NOT string-aware —
/// matches the hand `(exit)` classifier, which uses a bare paren counter.
pub fn stmt_balanced_close(bytes: &[u8], i: usize, limit: usize) -> usize {
    balanced(_lexer_none(), bytes, i, limit, b'(', b')').unwrap_or(i)
}
/// Does the arrow tail `[from, to)` resolve to a `$Target`? (The transition guard.)
pub fn arrow_has_target(bytes: &[u8], from: usize, to: usize) -> bool {
    parse_after_arrow(bytes, from, to).1.is_some()
}

/// A Frame statement at `i` — **production dispatches via the dogfooded StmtScan system**
/// (docs/JOURNAL.md), then extracts the fields (parse the arrow tail, the exit args). The
/// hand classifier survives as `frame_stmt_hand`, the StmtScan differential oracle. The
/// extraction (arg text, targets) is transformation and stays native.
fn frame_stmt(bytes: &[u8], i: usize, limit: usize, depth: u32, col: u32) -> Option<Stmt> {
    let (kind, e) = super::stmt_scan::classify(bytes, i, limit);
    if kind == 0 {
        return None;
    }
    let span = Span::new(i, e);
    match kind {
        // `push$ -> (enter) $S(state)`
        2 => {
            let arrow = find(bytes, i, e, b"->");
            let (enter_args, target, args_text) =
                parse_after_arrow(bytes, arrow.map(|a| a + 2).unwrap_or(i), e);
            Some(Stmt::StackPush(TransitionStmt {
                span,
                col,
                depth,
                target,
                args: None,
                args_text,
                exit_args: None,
                enter_args,
            }))
        }
        // Transition (1) or StackPop (3) — may carry `(exit)` args before the arrow.
        1 | 3 => {
            let (exit_args, arrow_at) = if bytes.get(i) == Some(&b'(') {
                match balanced(_lexer_none(), bytes, i, limit, b'(', b')') {
                    Some(close) => {
                        let ea = trimmed(&bytes[i + 1..close.saturating_sub(1)]);
                        let mut j = close;
                        while j < limit && (bytes[j] == b' ' || bytes[j] == b'\t') {
                            j += 1;
                        }
                        (ea, j)
                    }
                    None => (None, i),
                }
            } else {
                (None, i)
            };
            let (enter_args, target, args_text) = parse_after_arrow(bytes, arrow_at + 2, e);
            if kind == 3 {
                Some(Stmt::StackPop(SimpleStmt {
                    span,
                    col,
                    depth,
                    exit_args,
                    enter_args,
                }))
            } else {
                Some(Stmt::Transition(TransitionStmt {
                    span,
                    col,
                    depth,
                    target,
                    args: None,
                    args_text,
                    exit_args,
                    enter_args,
                }))
            }
        }
        // `=> $^`
        4 => Some(Stmt::Forward(SimpleStmt {
            span,
            col,
            depth,
            exit_args: None,
            enter_args: None,
        })),
        // bare `pop$` — pop and DISCARD (stay). Distinct from `-> pop$` (kind 3, restore).
        5 => Some(Stmt::StackPopBare(SimpleStmt {
            span,
            col,
            depth,
            exit_args: None,
            enter_args: None,
        })),
        _ => None,
    }
}

/// The hand classifier — kept ONLY as the StmtScan differential oracle (production runs the
/// system via `frame_stmt`). `-> $S(args)`, `push$ -> $S`, `-> pop$`, `=> $^`.
fn frame_stmt_hand(bytes: &[u8], i: usize, limit: usize, depth: u32, col: u32) -> Option<Stmt> {
    // `push$ -> (enter) $S(state)`
    if starts(bytes, i, b"push$", limit) {
        let e = to_end_of_line(bytes, i, limit);
        let arrow = find(bytes, i, e, b"->");
        let (enter_args, target, args_text) = parse_after_arrow(bytes, arrow.map(|a| a + 2).unwrap_or(i), e);
        return Some(Stmt::StackPush(TransitionStmt {
            span: Span::new(i, e),
            col,
            depth,
            target,
            args: None,
            args_text,
            exit_args: None,
            enter_args,
        }));
    }
    // bare `pop$` — pop and DISCARD (stay). Distinct from `-> pop$` (restore) below.
    if starts(bytes, i, b"pop$", limit) {
        let e = to_end_of_line(bytes, i, limit);
        return Some(Stmt::StackPopBare(SimpleStmt {
            span: Span::new(i, e),
            col,
            depth,
            exit_args: None,
            enter_args: None,
        }));
    }
    // `(exit) -> (enter) $S(state)` — a transition whose EXIT args precede the arrow.
    if bytes[i] == b'(' {
        if let Some(close) = balanced(_lexer_none(), bytes, i, limit, b'(', b')') {
            let mut j = close;
            while j < limit && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if starts(bytes, j, b"->", limit) {
                let e = to_end_of_line(bytes, i, limit);
                let exit_args = trimmed(&bytes[i + 1..close.saturating_sub(1)]);
                let (enter_args, target, args_text) = parse_after_arrow(bytes, j + 2, e);
                // `(reason) -> (enter) pop$` is a StackPop carrying BOTH arg sets, NOT a
                // transition. Enter args ride to the restored state's `$>` after the pop.
                if window(&bytes[j..e], b"pop$") {
                    return Some(Stmt::StackPop(SimpleStmt {
                        span: Span::new(i, e),
                        col,
                        depth,
                        exit_args,
                        enter_args,
                    }));
                }
                // ONLY a transition if the arrow resolves to a $Target. A native
                // `(*p)->field` or `(a) -> b` has no `$Target`, so it is NOT a transition
                // and falls through to native code.
                if target.is_none() {
                    return None;
                }
                return Some(Stmt::Transition(TransitionStmt {
                    span: Span::new(i, e),
                    col,
                    depth,
                    target,
                    args: None,
                    args_text,
                    exit_args,
                    enter_args,
                }));
            }
        }
    }
    // `-> pop$` / `-> (enter) $S(state)`
    if starts(bytes, i, b"->", limit) {
        let e = to_end_of_line(bytes, i, limit);
        let seg = &bytes[i..e];
        let (enter_args, target, args_text) = parse_after_arrow(bytes, i + 2, e);
        // `-> (enter) pop$` — the enter args ride to the restored state's `$>` after pop.
        if window(seg, b"pop$") {
            return Some(Stmt::StackPop(SimpleStmt {
                span: Span::new(i, e),
                col,
                depth,
                exit_args: None,
                enter_args,
            }));
        }
        return Some(Stmt::Transition(TransitionStmt {
            span: Span::new(i, e),
            col,
            depth,
            target,
            args: None,
            args_text,
            exit_args: None,
            enter_args,
        }));
    }
    // `=> $^`
    if starts(bytes, i, b"=>", limit) {
        let e = to_end_of_line(bytes, i, limit);
        return Some(Stmt::Forward(SimpleStmt {
            span: Span::new(i, e),
            col,
            depth,
            exit_args: None,
            enter_args: None,
        }));
    }
    None
}

/// Parse the part of a transition after `->`: an optional `(enter_args)`, then
/// `$Target(state_args)`. Returns `(enter_args, target, state_args)`.
fn parse_after_arrow(bytes: &[u8], from: usize, to: usize) -> (Option<String>, Option<String>, Option<String>) {
    let mut i = from;
    while i < to && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    // Optional enter args, only if the `(` comes BEFORE the `$Target`.
    let mut enter_args = None;
    if i < to && bytes[i] == b'(' {
        if let Some(close) = balanced(_lexer_none(), bytes, i, to, b'(', b')') {
            enter_args = trimmed(&bytes[i + 1..close.saturating_sub(1)]);
            i = close;
        }
    }
    (enter_args, target_of(bytes, i, to), args_of(bytes, i, to))
}

/// A lexer-less balanced-paren finder for the transition grammar (no strings expected
/// in a transition head). Reuses the real `balanced` with a throwaway lexer.
fn _lexer_none() -> &'static Lexer<'static> {
    // `balanced` only uses the lexer to skip strings/comments; a transition head has
    // none, so a minimal Python lexer over an empty slice is a safe stand-in.
    use std::sync::OnceLock;
    static LX: OnceLock<Lexer<'static>> = OnceLock::new();
    LX.get_or_init(|| Lexer::new(b"", super::literals::Target::Python3))
}

fn find(bytes: &[u8], from: usize, to: usize, pat: &[u8]) -> Option<usize> {
    let mut i = from;
    while i + pat.len() <= to {
        if &bytes[i..i + pat.len()] == pat {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn trimmed(b: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(b).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn stmt_span(s: &Stmt) -> Span {
    use crate::tree::Node;
    (s as &dyn Node).span()
}

fn target_of(bytes: &[u8], from: usize, to: usize) -> Option<String> {
    let mut i = from;
    while i < to {
        if bytes[i] == b'$' && is_name_start(bytes, i + 1) {
            let mut j = i + 1;
            while j < to && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            return Some(String::from_utf8_lossy(&bytes[i + 1..j]).into_owned());
        }
        i += 1;
    }
    None
}

// ------------------------------------------------------------------ helpers

/// Skip a comment or a literal. Returns the offset past it, if there was one.
///
/// **The dogfooded `OpaqueScan` system** (`opaque_at`) — the same recognizer the Segmenter and
/// `close_brace` ask; no hand `Lexer` here. The limit policy is `kind`-aware (Item 2's 3-way
/// signal, not just the extent): a COMMENT clamps to `limit` (a `//`/`/*` may legitimately run
/// past a span end and is still consumed up to it), while a LITERAL that OVERRUNS `limit` is
/// REJECTED (a string must fit inside the span to count). An unterminated body → `None`, exactly
/// as the hand path's `Err`-then-fallthrough did. (#219: one recognizer, asked by everyone —
/// never the fifteen per-language brace counters that each learned a different literal subset.)
pub(crate) fn skip_opaque(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<usize> {
    match super::opaque_scan::opaque_at(bytes, i, target) {
        super::opaque_scan::OpaqueAt::Comment(end) => Some(end.min(limit).max(i + 1)),
        super::opaque_scan::OpaqueAt::Literal(end) => {
            if end <= limit {
                Some(end.max(i + 1))
            } else {
                None
            }
        }
        super::opaque_scan::OpaqueAt::None | super::opaque_scan::OpaqueAt::Unterminated => None,
    }
}

/// The retired hand implementation — kept ONLY as the `skip_opaque` differential-test oracle
/// (the `machine_opaque_tests` module below) until the parity is locked and the hand lexer
/// recognition is deleted (Item 4). Self-contained (builds its own `Lexer`); not used in
/// production. Mirrors the hand policy exactly: comment clamps, literal rejects-on-overrun, an
/// `Err` (unterminated) or `Ok(None)` falls through to `None`.
#[doc(hidden)]
pub fn skip_opaque_hand(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<usize> {
    let lx = Lexer::new(bytes, target);
    if let Ok(Some(e)) = lx.comment_at(i) {
        return Some(e.min(limit).max(i + 1));
    }
    if let Ok(Some(l)) = lx.literal_at(i) {
        if l.span.end <= limit {
            return Some(l.span.end.max(i + 1));
        }
    }
    None
}

fn matching_brace(lx: &Lexer, bytes: &[u8], open: usize, limit: usize) -> usize {
    balanced(lx, bytes, open, limit, b'{', b'}').unwrap_or(limit)
}

/// The matching-closer finder — **now the dogfooded `DelimBalance` system**
/// (`delim_balance.frs`): an opaque-aware Dyck-1 counter over the `(o, c)` pair, bounded by
/// `limit`. The hand counter loop (and its per-language-brace-counter ancestors, #219) is gone;
/// `delim_balance::balanced_hand` survives as the differential oracle. `matching_brace` and the
/// state/handler param scans all route here.
fn balanced(lx: &Lexer, bytes: &[u8], open: usize, limit: usize, o: u8, c: u8) -> Option<usize> {
    super::delim_balance::balanced(bytes, open, limit, o, c, lx.target())
}

/// The `(name_end, open, end)` header extent of the state that starts at `at` (the `$`): the
/// offset past the `$Name`, the opening `{` offset, and the offset one past its matching `}`.
/// **Single source** — used by `state()` (which builds the node — the name, params, parent, and
/// extent all key off these) AND by the `MachineWalk` system's `state_end` leaf (which skips a
/// state body to find the next state start), so the boundary the walk finds and the extent the
/// node carries cannot drift, and `state()` no longer re-runs the name-skip. Replicates the hand
/// walk exactly: skip the `$Name`, naive-scan to the first `{` (the header is not opaque-aware —
/// as the old `state` was), then DelimBalance for the `}`.
pub fn state_extent(bytes: &[u8], at: usize, limit: usize, target: Target) -> (usize, usize, usize) {
    let mut j = at + 1;
    while j < limit && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    let name_end = j;
    while j < limit && bytes[j] != b'{' {
        j += 1;
    }
    let open = j;
    let end = super::delim_balance::balanced(bytes, open, limit, b'{', b'}', target).unwrap_or(limit);
    (name_end, open, end)
}

pub(crate) fn to_end_of_line(bytes: &[u8], mut i: usize, limit: usize) -> usize {
    while i < limit && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn starts(bytes: &[u8], i: usize, pat: &[u8], limit: usize) -> bool {
    i + pat.len() <= limit && &bytes[i..i + pat.len()] == pat
}

fn window(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn is_name_start(bytes: &[u8], i: usize) -> bool {
    bytes
        .get(i)
        .map(|b| b.is_ascii_alphabetic() || *b == b'_')
        .unwrap_or(false)
}

/// `interface:` / `domain:` — declarations, one per line.
pub fn decl_section(lx: &Lexer, bytes: &[u8], span: Span, kw: Span, with_bodies: bool) -> DeclSection {
    let mut members = Vec::new();
    let mut i = kw.end;
    let mut cursor = kw.end;

    while i < span.end {
        if let Some(next) = skip_opaque(bytes, i, span.end, lx.target()) {
            i = next;
            continue;
        }
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // `@@[attr]` — an attribute line, not a declaration. Without this it was parsed
        // as a field with an EMPTY name, and Java got `public Object ;`.
        if starts(bytes, i, b"@@[", span.end) {
            i = to_end_of_line(bytes, i, span.end);
            continue;
        }
        // A declaration starts here.
        if cursor < i {
            members.push(Decl::Trivia(TriviaNode {
                span: Span::new(cursor, i),
            }));
        }
        // `actions:` / `operations:` members have a NATIVE body in braces.
        let eol = to_end_of_line(bytes, i, span.end);
        let brace = (i..eol).find(|&k| bytes[k] == b'{');
        if with_bodies && brace.is_some() {
            let open = brace.unwrap();
            let end = matching_brace(lx, bytes, open, span.end);
            let close = end.saturating_sub(1);
            let sig = decl_of(bytes, i, open, i);
            members.push(Decl::WithBody(BodyDecl {
                span: Span::new(i, end),
                name: sig.name,
                params_text: sig.params_text.unwrap_or_default(),
                return_text: sig.type_text,
                signature_node: FrameSpan {
                    span: Span::new(i, open + 1),
                    kind: "Signature",
                },
                body: body(lx, bytes, Span::new(open + 1, close)),
                close_node: FrameSpan {
                    span: Span::new(close, end),
                    kind: "Close",
                },
            }));
            i = end;
            cursor = i;
            continue;
        }
        members.push(Decl::Member(decl_of(bytes, i, eol, i)));
        i = eol;
        cursor = i;
    }
    if cursor < span.end {
        members.push(Decl::Trivia(TriviaNode {
            span: Span::new(cursor, span.end),
        }));
    }

    DeclSection {
        span,
        keyword_node: FrameSpan {
            span: kw,
            kind: "Keyword",
        },
        members,
    }
}


/// The column of `at`, relative to the start of the line. (0-based.)
fn column_of(bytes: &[u8], at: usize, _body_start: usize) -> u32 {
    let mut i = at;
    while i > 0 && bytes[i - 1] != b'\n' {
        i -= 1;
    }
    (at - i) as u32
}


/// Pull the NAME and the (verbatim) type text out of a declaration line.
///
/// `go()` / `go(a: int): bool` / `n: int = 0` / `$.count: int = 0`
///
/// framec reads the *name* — Frame's vocabulary, framec's to know. It carries the
/// *type* as opaque text and never looks inside it. `Rc<RefCell<Child>>` is a string
/// here and it will still be a string when it reaches the target.
fn decl_of(bytes: &[u8], from: usize, to: usize, span_start: usize) -> MemberDecl {
    let mut i = from;
    while i < to && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }

    // `async fetch(key: String): String` — `async` is a MODIFIER, not the name.
    //
    // Without this the method was named `async`, and Python emitted `def async(self):` —
    // `async` is a Python keyword, so the file was a SyntaxError. The name is Frame's
    // vocabulary and framec must read it correctly; a modifier is not a name.
    let mut is_async = false;
    if starts(bytes, i, b"async", to) {
        let after = i + 5;
        if after < to && (bytes[after] == b' ' || bytes[after] == b'\t') {
            is_async = true;
            i = after;
            while i < to && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
        }
    }

    let ns = i;
    while i < to && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    let name = String::from_utf8_lossy(&bytes[ns..i]).into_owned();

    // Params, if this is a signature.
    let mut params_text = None;
    if i < to && bytes[i] == b'(' {
        let open = i;
        let mut d = 0i32;
        while i < to {
            match bytes[i] {
                b'(' => d += 1,
                b')' => {
                    d -= 1;
                    if d == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        params_text = Some(String::from_utf8_lossy(&bytes[open + 1..i.min(to)]).into_owned());
        i = (i + 1).min(to);
    }

    // A `: type` annotation, up to `=` or end of line. VERBATIM.
    let mut type_text = None;
    while i < to && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < to && bytes[i] == b':' {
        i += 1;
        let ts = i;
        while i < to && bytes[i] != b'=' {
            i += 1;
        }
        let t = String::from_utf8_lossy(&bytes[ts..i]).trim().to_string();
        if !t.is_empty() {
            type_text = Some(t);
        }
    }

    // The initializer, VERBATIM (everything after `=`, trimmed).
    let mut init_text = None;
    // If it is `@@Sys(...)` — FRAME's own syntax — then framec knows this field holds a
    // system, and it knows it WITHOUT looking at the user's type.
    let mut init_system = None;
    if i < to && bytes[i] == b'=' {
        let raw = String::from_utf8_lossy(&bytes[i + 1..to]).trim().to_string();
        if !raw.is_empty() {
            init_text = Some(raw);
        }
        let mut k = i + 1;
        while k < to && (bytes[k] == b' ' || bytes[k] == b'\t') {
            k += 1;
        }
        if k + 2 < to && bytes[k] == b'@' && bytes[k + 1] == b'@' {
            let mut n = k + 2;
            // `@@!Sys()` — the no-init form.
            if n < to && bytes[n] == b'!' {
                n += 1;
            }
            let ns2 = n;
            while n < to && (bytes[n].is_ascii_alphanumeric() || bytes[n] == b'_') {
                n += 1;
            }
            if n > ns2 {
                init_system =
                    Some(String::from_utf8_lossy(&bytes[ns2..n]).into_owned());
            }
        }
    }

    MemberDecl {
        span: Span::new(span_start, to),
        name,
        type_text,
        params_text,
        init_system,
        is_async,
        init_text,
    }
}


/// The args of `-> $S(args)`, **verbatim, as one blob**.
///
/// It is NOT split, and that is a decision, not an omission. In C++,
/// `f(a < b, c > d)` (two comparisons) and `f(std::map<int,int>())` (one generic) are
/// the same token shape; separating them needs name lookup over the user's types, which
/// a lexer cannot do and which C++'s own grammar cannot do either. So framec hands the
/// blob to the target compiler, which splits it correctly, and hands the arity error
/// back for free.
fn args_of(bytes: &[u8], from: usize, to: usize) -> Option<String> {
    let mut i = from;
    // Find the `$Name(` — the paren that belongs to the transition target.
    while i < to {
        if bytes[i] == b'$' && is_name_start(bytes, i + 1) {
            let mut j = i + 1;
            while j < to && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j < to && bytes[j] == b'(' {
                let mut d = 0i32;
                let open = j;
                while j < to {
                    match bytes[j] {
                        b'(' => d += 1,
                        b')' => {
                            d -= 1;
                            if d == 0 {
                                let inner =
                                    String::from_utf8_lossy(&bytes[open + 1..j]).into_owned();
                                return if inner.trim().is_empty() {
                                    None
                                } else {
                                    Some(inner)
                                };
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            return None;
        }
        i += 1;
    }
    None
}


/// `<frame-ref> = <native expr> [;]` — **a Frame statement.**
///
/// The LHS is Frame's syntax. The RHS is the user's expression (tokenized, never
/// interpreted). A trailing terminator is **part of Frame's statement** and is consumed
/// here, at scan time, by the delimiter — not guessed later by re-reading emitted text.
/// The parsed geometry of a `$.x = …` frame assignment at `i` (if one is there). **Single
/// source** — `frame_assign` builds the node from it AND the `BodyWalk` system's `stmt_end` leaf
/// reads its `.eol` (the extent), so the statement boundary the walk finds and the extent the
/// node carries cannot drift. `native_parts`-FREE (the extent never reads it — `native_parts`
/// only fills the node's `rhs`), and target-free (`frame_ref_at_pub` is RefScan-backed). `None`
/// if no single-`=` assignment opens here.
struct AssignHead {
    lhs: FrameRef,
    lhs_end: usize,
    rhs_start: usize,
    rhs_end: usize,
    eol: usize,
    terminator: Option<Span>,
}

fn frame_assign_parse(bytes: &[u8], i: usize, limit: usize) -> Option<AssignHead> {
    let lhs = super::parts::frame_ref_at_pub(bytes, i, limit)?;
    let lhs_end = lhs.span.end;

    // A single `=` must follow (not `==`, not `+=` — see below).
    let mut j = lhs_end;
    while j < limit && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    if j >= limit || bytes[j] != b'=' {
        return None;
    }
    // `==` is a comparison, not an assignment.
    if bytes.get(j + 1) == Some(&b'=') {
        return None;
    }
    // NOTE: a COMPOUND assignment (`+=`, `-=`) is deliberately NOT matched here. It is
    // not the same statement — `$.x += 1` needs a read AND a write — and pretending
    // otherwise is exactly how the old compiler emitted `((int) m.get("x")) += 1`, an
    // invalid lvalue (#227). Not matching it means it stays native and is visibly wrong,
    // rather than silently wrong. A distinct node comes next.
    let rhs_start = j + 1;

    // The RHS runs to end of line, minus any trailing terminator.
    let eol = to_end_of_line(bytes, rhs_start, limit);
    let mut rhs_end = eol;
    while rhs_end > rhs_start && bytes[rhs_end - 1].is_ascii_whitespace() {
        rhs_end -= 1;
    }
    let mut terminator = None;
    if rhs_end > rhs_start && bytes[rhs_end - 1] == b';' {
        terminator = Some(Span::new(rhs_end - 1, rhs_end));
        rhs_end -= 1;
    }

    Some(AssignHead {
        lhs,
        lhs_end,
        rhs_start,
        rhs_end,
        eol,
        terminator,
    })
}

fn frame_assign(lx: &Lexer, bytes: &[u8], i: usize, limit: usize, col: u32) -> Option<Stmt> {
    let h = frame_assign_parse(bytes, i, limit)?;
    Some(Stmt::Assign(AssignStmt {
        span: Span::new(i, h.eol),
        col,
        lhs: h.lhs,
        op: TriviaNode {
            span: Span::new(h.lhs_end, h.rhs_start),
        },
        rhs: native_parts(lx, bytes, h.rhs_start, h.rhs_end),
        rhs_span: Span::new(h.rhs_start, h.rhs_end),
        tail: if h.rhs_end < h.eol {
            Some(TriviaNode {
                span: Span::new(h.rhs_end, h.eol),
            })
        } else {
            None
        },
        terminator: h.terminator,
    }))
}

/// The offset one past a `$.x = …` assignment that starts at `i`, or `None` — the extent half of
/// [`frame_assign_parse`], for the `BodyWalk` walk. `native_parts`-free.
pub(crate) fn frame_assign_end(bytes: &[u8], i: usize, limit: usize) -> Option<usize> {
    frame_assign_parse(bytes, i, limit).map(|h| h.eol)
}


/// The parsed geometry of a `@@:…` frame call at `i` (if one is there): which form, the `(…)`
/// paren offsets, and the extent end. **Single source** — `frame_call` builds the node from it AND
/// the `BodyWalk` system's `stmt_end` leaf reads its `.end`, so the statement boundary the walk
/// finds and the extent the node carries cannot drift. `native_parts`-FREE (the extent =
/// `consume_terminator(balanced(open))`, never reads `native_parts`, which only fills the node's
/// `expr`). `None` if no `@@:…` call opens here.
enum CallHeadKind {
    /// `@@:(expr)` / `@@:return(expr)` — both build a `ReturnCall`. `open` is the `(`.
    Return { open: usize, close: usize },
    /// `@@:self.method(args)`. `name` = `[name_start, name_end)`, `open` = `name_end` (the `(`).
    SelfCall {
        name_start: usize,
        name_end: usize,
        close: usize,
    },
}

struct CallHead {
    kind: CallHeadKind,
    end: usize,
}

fn frame_call_parse(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<CallHead> {
    if !starts(bytes, i, b"@@:", limit) {
        return None;
    }
    // `@@:(expr)` — the CONCISE return form. Same statement as `@@:return(expr)`.
    if starts(bytes, i, b"@@:(", limit) {
        let open = i + b"@@:".len();
        let close = super::delim_balance::balanced(bytes, open, limit, b'(', b')', target)?;
        let end = consume_terminator(bytes, close, limit);
        return Some(CallHead {
            kind: CallHeadKind::Return { open, close },
            end,
        });
    }
    // `@@:return(`
    if starts(bytes, i, b"@@:return(", limit) {
        let open = i + b"@@:return".len();
        let close = super::delim_balance::balanced(bytes, open, limit, b'(', b')', target)?;
        let end = consume_terminator(bytes, close, limit);
        return Some(CallHead {
            kind: CallHeadKind::Return { open, close },
            end,
        });
    }
    // `@@:self.method(`
    if starts(bytes, i, b"@@:self.", limit) {
        let ns = i + b"@@:self.".len();
        let mut j = ns;
        while j < limit && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > ns && j < limit && bytes[j] == b'(' {
            let close = super::delim_balance::balanced(bytes, j, limit, b'(', b')', target)?;
            let end = consume_terminator(bytes, close, limit);
            return Some(CallHead {
                kind: CallHeadKind::SelfCall {
                    name_start: ns,
                    name_end: j,
                    close,
                },
                end,
            });
        }
    }
    None
}

/// `@@:return(<expr>)` / `@@:(<expr>)` / `@@:self.method(<args>)` — **Frame statements**, built
/// from the shared [`frame_call_parse`].
///
/// framec authored these calls, so framec terminates them. The old compiler lowered the
/// `@@:self` part to a *reference* and left `.report()` as native text with no
/// terminator (#229) — because there was no node to ask.
fn frame_call(lx: &Lexer, bytes: &[u8], i: usize, limit: usize, depth: u32, col: u32) -> Option<Stmt> {
    let h = frame_call_parse(bytes, i, limit, lx.target())?;
    match h.kind {
        CallHeadKind::Return { open, close } => Some(Stmt::ReturnCall(ReturnCallStmt {
            span: Span::new(i, h.end),
            col,
            head: TriviaNode {
                span: Span::new(i, open + 1),
            },
            tail: TriviaNode {
                span: Span::new(close - 1, h.end),
            },
            depth,
            expr: native_parts(lx, bytes, open + 1, close - 1),
            expr_span: Span::new(open + 1, close - 1),
        })),
        CallHeadKind::SelfCall {
            name_start,
            name_end,
            close,
        } => Some(Stmt::SelfCall(SelfCallStmt {
            span: Span::new(i, h.end),
            col,
            method: String::from_utf8_lossy(&bytes[name_start..name_end]).into_owned(),
            args_text: String::from_utf8_lossy(&bytes[name_end + 1..close.saturating_sub(1)])
                .into_owned(),
        })),
    }
}

/// The offset one past a `@@:…` frame call that starts at `i`, or `None` — the extent half of
/// [`frame_call_parse`], for the `BodyWalk` walk. `native_parts`-free.
pub(crate) fn frame_call_end(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<usize> {
    frame_call_parse(bytes, i, limit, target).map(|h| h.end)
}

/// A trailing target terminator is **part of Frame's statement** and is consumed here,
/// at delimit time. The backend then emits its own in its own spelling — it never looks
/// at what it just wrote to decide (#173).
fn consume_terminator(bytes: &[u8], mut at: usize, limit: usize) -> usize {
    while at < limit && (bytes[at] == b' ' || bytes[at] == b'\t') {
        at += 1;
    }
    if at < limit && bytes[at] == b';' {
        at + 1
    } else {
        at
    }
}

// ============================================================================
// skip_opaque differential parity — Item 3a (retire machine.rs::skip_opaque).
//
// `skip_opaque` (PRODUCTION) runs the dogfooded OpaqueScan system (`opaque_at`)
// under a KIND-AWARE limit policy; `skip_opaque_hand` (the retired hand `Lexer`,
// `comment_at`/`literal_at`) is the INDEPENDENT differential oracle. They must be
// equal at EVERY position i, over MULTIPLE limits per input.
//
// The whole point of the kind-aware policy is an ASYMMETRY the extent number alone
// cannot see:
//   * a COMMENT whose extent runs past `limit` still CLAMPS to `limit` (a `//`/`/*`
//     may legitimately overrun a span end and is consumed up to it);
//   * a LITERAL whose extent runs past `limit` is REJECTED to `None` (a string must
//     fit inside the span to count).
// So the corpus and the fuzz arm both drive `limit` into the INTERIOR of each opaque
// form (a full i x limit cross-product), and a TEETH gate asserts a clamp AND a
// reject actually fired (counts > 0) — the asymmetry is exercised, not agreed-upon
// vacuously. A mismatch here is a real machine/oracle divergence and is a finding.
//
// SCAFFOLDING: conversion-internal — calls the private `skip_opaque` and the hand
// oracle, and reads the internal `OpaqueAt` classification. NEVER promotes (needs the
// hand oracle + internal registers; not emitted-code behavior).
// ============================================================================
#[cfg(test)]
mod skip_opaque_tests {
    use super::{skip_opaque, skip_opaque_hand};
    use crate::text::scan::literals::Target;
    use crate::text::scan::opaque_scan::{opaque_at, OpaqueAt};

    const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

    /// Machine vs the INDEPENDENT hand oracle at one `(i, limit)`. Returns the machine
    /// result so the caller can account teeth. A divergence panics with a reproducer.
    fn agree_one(b: &[u8], i: usize, limit: usize, t: Target) -> Option<usize> {
        let m = skip_opaque(b, i, limit, t);
        let h = skip_opaque_hand(b, i, limit, t);
        assert_eq!(
            m, h,
            "skip_opaque parity broke: target {t:?}, i {i}, limit {limit} of {b:?}: \
             machine={m:?} hand={h:?}"
        );
        m
    }

    /// Teeth accounting: prove BOTH outcomes (Some/None) occur AND that a real clamp
    /// (comment overruns limit → result == limit) AND a real reject (literal overruns
    /// limit → None) actually happen, classified against the INTERNAL `opaque_at`.
    #[derive(Default)]
    struct Teeth {
        somes: usize,
        nones: usize,
        clamps: usize,
        rejects: usize,
    }
    impl Teeth {
        fn observe(&mut self, b: &[u8], i: usize, limit: usize, t: Target, m: Option<usize>) {
            match m {
                Some(_) => self.somes += 1,
                None => self.nones += 1,
            }
            match opaque_at(b, i, t) {
                // Comment extent runs PAST limit, with limit at/after the opener: the
                // kind-aware policy CLAMPS, so the result must be exactly `limit`.
                OpaqueAt::Comment(end) if end > limit && limit >= i + 1 => {
                    if m == Some(limit) {
                        self.clamps += 1;
                    }
                }
                // Literal extent runs PAST limit: the policy REJECTS to `None`.
                OpaqueAt::Literal(end) if end > limit => {
                    if m.is_none() {
                        self.rejects += 1;
                    }
                }
                _ => {}
            }
        }
    }

    /// Full `i` x `limit` cross-product sweep — every position as an opener AND every
    /// limit (crucially the ones that fall INSIDE a comment and INSIDE a string, which
    /// is where the clamp-vs-reject asymmetry bites). Feeds the shared teeth accumulator.
    fn sweep(src: &str, t: Target, teeth: &mut Teeth) {
        let b = src.as_bytes();
        for i in 0..=b.len() {
            for limit in 0..=b.len() {
                let m = agree_one(b, i, limit, t);
                teeth.observe(b, i, limit, t, m);
            }
        }
    }

    /// The opaque-form corpus (adapted from `tests/opaque_scan.rs`): every form the four
    /// cleanroom targets recognize, plus the edges, run under the FULL limit sweep so a
    /// `limit` lands inside each form. Returns teeth for the caller's gate.
    fn run_corpus() -> Teeth {
        let mut teeth = Teeth::default();

        // ---- C / Java: //, /*…*/ (no nest), "…", '…' ----
        for t in [Target::C, Target::Java] {
            sweep("int x = 1; // a comment\n y=2;", t, &mut teeth);
            sweep("a /* block */ b", t, &mut teeth);
            sweep("a /* /* not nested */ b */ c", t, &mut teeth);
            sweep(r#"s = "hello world";"#, t, &mut teeth);
            sweep(r#"s = "with \" escaped quote";"#, t, &mut teeth);
            sweep(r#"s = "trailing backslash \\";"#, t, &mut teeth);
            sweep("c = 'x';", t, &mut teeth);
            sweep(r#"c = '\'';"#, t, &mut teeth);
            sweep(r#"x = "unterminated"#, t, &mut teeth); // unterminated literal
            sweep("y /* unterminated comment", t, &mut teeth); // unterminated comment
            sweep(r#"m("a)b", c)"#, t, &mut teeth);
            sweep("empty=\"\";", t, &mut teeth);
            sweep("a /**/ b", t, &mut teeth); // minimal empty block
            sweep("s = \"a\nb\";", t, &mut teeth); // newline in string → unterminated
        }

        // ---- Rust: nesting /*…*/, r"…", r#"…"#, "…" multiline, '…' ----
        {
            let t = Target::Rust;
            sweep("x // line\n y", t, &mut teeth);
            sweep("a /* /* nested */ still */ c", t, &mut teeth);
            sweep("a /* /* deep /* three */ two */ one */ z", t, &mut teeth);
            sweep(r#"let s = "multi
line ok";"#, t, &mut teeth);
            sweep(r#"let r = r"no escapes \ here";"#, t, &mut teeth);
            sweep(r##"let r = r#"has "quote" inside"#;"##, t, &mut teeth);
            sweep(r###"let r = r##"a"#b"##;"###, t, &mut teeth);
            sweep("let c = 'a';", t, &mut teeth);
            sweep(r#"br"byte raw""#, t, &mut teeth);
            sweep("a /* unterminated /* still open", t, &mut teeth); // unterminated nested
            sweep("r#\"unterminated raw", t, &mut teeth);
            sweep("let c = '\n';", t, &mut teeth); // char is not multiline
        }

        // ---- Python: #, "…", '…', """…""", '''…''', f"…{hole}…" ----
        {
            let t = Target::Python3;
            sweep("x = 1  # a comment\n y = 2", t, &mut teeth);
            sweep(r#"s = "double""#, t, &mut teeth);
            sweep("s = 'single'", t, &mut teeth);
            sweep(r#"d = """triple
   spanning
   lines"""  # ok"#, t, &mut teeth);
            sweep("e = '''also triple'''", t, &mut teeth);
            sweep(r#"f = f"value is {x + 1}!""#, t, &mut teeth); // hole
            sweep(r#"j = "quote in hole {x['\"']} end""#, t, &mut teeth);
            sweep(r#"k = "unterminated hole {a + b"#, t, &mut teeth);
            sweep("\"\"\"unterminated triple", t, &mut teeth);
            sweep("s = \"a\nb\"", t, &mut teeth); // plain string not multiline
        }

        // ---- Edges: empty, EOF, escape-at-EOF, lone openers ----
        for &t in &TARGETS {
            sweep("", t, &mut teeth);
            sweep("x", t, &mut teeth);
            sweep("\"", t, &mut teeth); // lone opening quote at EOF
            sweep("\"a\\", t, &mut teeth); // escape at EOF inside a string
            sweep("'", t, &mut teeth);
            sweep("\n", t, &mut teeth);
        }
        sweep("/*", Target::C, &mut teeth);
        sweep("//", Target::C, &mut teeth);
        sweep("#", Target::Python3, &mut teeth);
        sweep("\"\"\"", Target::Python3, &mut teeth);
        sweep("r#\"", Target::Rust, &mut teeth);

        teeth
    }

    /// The differential over the whole corpus, plus the teeth gate. A single named test
    /// so a corpus regression AND a "the asymmetry never fired" regression both fail here.
    #[test]
    fn corpus_parity_and_teeth() {
        let teeth = run_corpus();
        assert!(teeth.somes > 0, "no Some outcome — vacuous");
        assert!(teeth.nones > 0, "no None outcome — vacuous");
        assert!(
            teeth.clamps > 0,
            "CLAMP arm never fired: no comment overran its limit — the kind-aware clamp \
             is untested (a #232 lie)"
        );
        assert!(
            teeth.rejects > 0,
            "REJECT arm never fired: no literal overran its limit — the reject-on-overrun \
             asymmetry is untested"
        );
    }

    /// Oracle-INDEPENDENT anchors: hand-computed expected values pinning the clamp-vs-reject
    /// asymmetry with a `limit` DELIBERATELY inside each form. These survive the hand oracle's
    /// retirement (they assert known extents, not `== hand`).
    #[test]
    fn limit_inside_form_known_extents() {
        // A C line comment `// comment` occupies indices 0..=9; `\n` at 10 is the extent.
        // opaque_at(0) == Comment(10).
        let c = b"// comment\nX";
        assert_eq!(opaque_at(c, 0, Target::C), OpaqueAt::Comment(10));
        // limit INSIDE the comment CLAMPS to the limit.
        assert_eq!(skip_opaque(c, 0, 5, Target::C), Some(5));
        assert_eq!(skip_opaque(c, 0, 9, Target::C), Some(9));
        // limit AT the extent (or past) yields the full extent.
        assert_eq!(skip_opaque(c, 0, 10, Target::C), Some(10));
        assert_eq!(skip_opaque(c, 0, 12, Target::C), Some(10));
        // limit == i clamps up to i+1 (min(limit) then max(i+1)); not < i+1.
        assert_eq!(skip_opaque(c, 0, 0, Target::C), Some(1));

        // A C string "abcdef" occupies indices 0..=7; extent is 8 (past the close).
        let s = b"\"abcdef\"";
        assert_eq!(opaque_at(s, 0, Target::C), OpaqueAt::Literal(8));
        // limit INSIDE the string REJECTS to None (a string must fit the span).
        assert_eq!(skip_opaque(s, 0, 4, Target::C), None);
        assert_eq!(skip_opaque(s, 0, 7, Target::C), None); // one short of the extent
        // limit AT the extent (or past) ACCEPTS the full extent.
        assert_eq!(skip_opaque(s, 0, 8, Target::C), Some(8));

        // Every one of the above also equals the hand oracle (belt and suspenders).
        for (b, i, lim) in [
            (&c[..], 0usize, 5usize),
            (&c[..], 0, 10),
            (&c[..], 0, 0),
            (&s[..], 0, 4),
            (&s[..], 0, 8),
        ] {
            assert_eq!(
                skip_opaque(b, i, lim, Target::C),
                skip_opaque_hand(b, i, lim, Target::C)
            );
        }
    }

    // ---- Deterministic xorshift fuzz: frame-ish bytes, random limit per position. ----

    struct Rng(u64);
    impl Rng {
        fn new(seed: u64) -> Rng {
            let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678);
            if s == 0 {
                s = 0xDEAD_BEEF;
            }
            Rng(s)
        }
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % (n as u64)) as usize
        }
    }

    /// Multi-byte literal fragments so the generator actually forms `//`, `/*`, `"""`,
    /// `r#"`, `"#`, holes, escapes — not two independent single-byte draws lining up.
    const FRAGMENTS: &[&[u8]] = &[
        b"\"", b"'", b"//", b"/*", b"*/", b"#", b"\"\"\"", b"'''", b"r\"", b"r#\"", b"\"#",
        b"{", b"}", b"{x}", b"\\", b"\\\"", b"\n", b" ", b"@@", b"abc", b"br\"", b";", b"1",
    ];

    fn gen_frame_ish(rng: &mut Rng, max_frags: usize) -> String {
        let n = rng.below(max_frags + 1);
        let mut v: Vec<u8> = Vec::new();
        for _ in 0..n {
            v.extend_from_slice(FRAGMENTS[rng.below(FRAGMENTS.len())]);
        }
        String::from_utf8(v).expect("fragments are ASCII")
    }

    /// Fuzz: frame-ish source, EVERY position, a RANDOM limit per position, ALL 4 targets.
    /// Differential vs the hand oracle throughout; teeth gated so the asymmetry fires here
    /// too (not only in the curated cross-product). A failing seed reproduces from its index.
    #[test]
    fn fuzz_random_limit_every_position_all_targets() {
        let mut teeth = Teeth::default();
        for &t in &TARGETS {
            for seed in 0u64..1500 {
                let mut rng = Rng::new(seed ^ 0xC3C3_0F0F);
                let src = gen_frame_ish(&mut rng, 9);
                let b = src.as_bytes();
                for i in 0..=b.len() {
                    // A random limit per position, spanning [0, len] so it can land
                    // before, inside, or past whatever opens at `i`.
                    let limit = rng.below(b.len() + 1);
                    let m = agree_one(b, i, limit, t);
                    teeth.observe(b, i, limit, t, m);
                }
            }
        }
        // The fuzz arm alone must exhibit both outcomes and both asymmetry directions.
        assert!(teeth.somes > 0 && teeth.nones > 0, "fuzz not diverse in outcome");
        assert!(
            teeth.clamps > 0,
            "fuzz never clamped a comment at a mid-comment limit — arm lacks teeth"
        );
        assert!(
            teeth.rejects > 0,
            "fuzz never rejected a literal at a mid-string limit — arm lacks teeth"
        );
    }

    /// The fuzz generator must be diverse (a generator that can't open a form tests
    /// nothing — the #232 lie). Determinism + spread check, independent of parity.
    #[test]
    fn fuzz_corpus_is_diverse() {
        use std::collections::HashSet;
        let mut distinct = HashSet::new();
        for seed in 0u64..1500 {
            let mut rng = Rng::new(seed ^ 0xC3C3_0F0F);
            distinct.insert(gen_frame_ish(&mut rng, 9));
        }
        assert!(distinct.len() > 900, "generator not diverse: {} distinct", distinct.len());
    }
}
