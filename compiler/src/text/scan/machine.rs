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
use super::parts::native_parts;
use crate::tree::body::{
    AssignStmt, Body, NativeStmt, ReturnCallStmt, SelfCallStmt, SimpleStmt, Stmt, TransitionStmt,
};
use crate::tree::{
    BodyDecl, Decl, DeclSection, FrameSpan, HandlerNode, MachineMember, MachineSection, MemberDecl,
    StateMember, StateNode, TriviaNode,
};
use crate::Span;

/// A `machine:` section: states, and the trivia between them.
pub fn machine_section(lx: &Lexer, bytes: &[u8], span: Span, kw: Span) -> MachineSection {
    let mut members = Vec::new();
    let mut i = kw.end;
    let mut cursor = kw.end;

    while i < span.end {
        if let Some(next) = skip_opaque(lx, i, span.end) {
            i = next;
            continue;
        }
        // A state begins with `$Name` at this level.
        if bytes[i] == b'$' && is_name_start(bytes, i + 1) {
            if cursor < i {
                members.push(MachineMember::Trivia(TriviaNode {
                    span: Span::new(cursor, i),
                }));
            }
            let st = state(lx, bytes, i, span.end);
            i = st.span.end;
            cursor = i;
            members.push(MachineMember::State(st));
            continue;
        }
        i += 1;
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
    let mut j = at + 1;
    let ns = j;
    while j < limit && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    let name = String::from_utf8_lossy(&bytes[ns..j]).into_owned();

    // `$B(n: int, m: str)` — the declared parameter NAMES.
    let mut params = Vec::new();
    let mut param_types = std::collections::HashMap::new();
    if j < limit && bytes[j] == b'(' {
        if let Some(pe) = balanced(lx, bytes, j, limit, b'(', b')') {
            let inner = String::from_utf8_lossy(&bytes[j + 1..pe.saturating_sub(1)]).into_owned();
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
        let mut k = j;
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

    // Header runs to the state's opening `{`.
    while j < limit && bytes[j] != b'{' {
        j += 1;
    }
    let open = j;
    let end = matching_brace(lx, bytes, open, limit);
    let close = end.saturating_sub(1);

    let mut members = Vec::new();
    let mut i = open + 1;
    let mut cursor = i;

    while i < close {
        if let Some(next) = skip_opaque(lx, i, close) {
            i = next;
            continue;
        }
        // `$.name: T = init` — a state variable (Frame's own declaration).
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'.') {
            if cursor < i {
                members.push(StateMember::Trivia(TriviaNode {
                    span: Span::new(cursor, i),
                }));
            }
            let e = to_end_of_line(bytes, i, close);
            members.push(StateMember::StateVar(decl_of(bytes, i + 2, e, i)));
            i = e;
            cursor = i;
            continue;
        }
        // A handler: `name(...) {` / `$>() {` / `<$() {` — anything followed by a
        // brace at this level.
        if let Some(h) = handler_at(lx, bytes, i, close) {
            if cursor < i {
                members.push(StateMember::Trivia(TriviaNode {
                    span: Span::new(cursor, i),
                }));
            }
            i = h.span.end;
            cursor = i;
            members.push(StateMember::Handler(h));
            continue;
        }
        i += 1;
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

/// A handler starting at `i`, if there is one.
fn handler_at(lx: &Lexer, bytes: &[u8], i: usize, limit: usize) -> Option<HandlerNode> {
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
    let pe = balanced(lx, bytes, j, limit, b'(', b')')?;
    let mut k = pe;
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
    let end = matching_brace(lx, bytes, open, limit);
    let close = end.saturating_sub(1);

    Some(HandlerNode {
        span: Span::new(i, end),
        event: name,
        params_text: String::from_utf8_lossy(&bytes[j + 1..pe.saturating_sub(1)]).into_owned(),
        return_text,
        header_node: FrameSpan {
            span: Span::new(i, open + 1),
            kind: "HandlerHeader",
        },
        // *** THE TREE THE OLD COMPILER DID NOT HAVE ***
        body: body(lx, bytes, Span::new(open + 1, close)),
        close_node: FrameSpan {
            span: Span::new(close, end),
            kind: "Close",
        },
    })
}

/// A handler body: statements, and the trivia between them. **Partitions the span.**
pub fn body(lx: &Lexer, bytes: &[u8], span: Span) -> Body {
    let mut stmts = Vec::new();
    let mut i = span.start;
    let mut cursor = span.start;
    // Brace depth WITHIN the body. Tracked here, once, by the lexer that already
    // knows what a brace is — never re-derived later from emitted text.
    let mut depth = 0u32;

    while i < span.end {
        // A FRAME ASSIGNMENT: `@@:self.x = expr`, `$.x = expr`, `@@:data.k = expr`.
        //
        // Frame's own statement — framec owns it, terminator and all. The old compiler
        // had no node for this: `@@:self` was a REFERENCE and the ` = expr` fell out as
        // untyped native text, so nothing could ask whether it was terminated and framec
        // searched its own emitted output to guess (#173, #229).
        if let Some(st) = frame_call(lx, bytes, i, span.end, depth, column_of(bytes, i, 0)) {
            let sp = stmt_span(&st);
            push_native(lx, bytes, &mut stmts, cursor, sp.start, span.start, depth, target(lx));
            stmts.push(st);
            i = sp.end;
            cursor = i;
            continue;
        }
        if let Some(st) = frame_assign(lx, bytes, i, span.end, column_of(bytes, i, 0)) {
            let sp = stmt_span(&st);
            push_native(lx, bytes, &mut stmts, cursor, sp.start, span.start, depth, target(lx));
            stmts.push(st);
            i = sp.end;
            cursor = i;
            continue;
        }
        // Frame's other statements. Everything else is native, and native code is
        // carried verbatim — but TOKENIZED (its literals and its Frame refs are nodes).
        if let Some(st) = frame_stmt(bytes, i, span.end, depth, column_of(bytes, i, 0)) {
            let s = stmt_span(&st);
            // Whatever came before it is a native statement (or trivia).
            push_native(lx, bytes, &mut stmts, cursor, s.start, span.start, depth, target(lx));
            stmts.push(st);
            i = s.end;
            cursor = i;
            continue;
        }
        if let Some(next) = skip_opaque(lx, i, span.end) {
            i = next;
            continue;
        }
        // Depth is a NUMBER, never a KIND. framec counts braces; it never asks
        // whether the block is an `if`, a `while`, or a lambda — and it must not,
        // because that question is a parse, and framec does not parse native code.
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    push_native(lx, bytes, &mut stmts, cursor, span.end, span.start, depth, target(lx));

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
/// **One lexer, asked by everyone.** The old compiler had fifteen hand-written brace
/// counters, each of which had learned a different subset of its own language's
/// literals — so a `}` inside a Ruby heredoc, a JS regex, or a Lua long string closed a
/// block that was never open (#219).
fn skip_opaque(lx: &Lexer, i: usize, limit: usize) -> Option<usize> {
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

fn balanced(lx: &Lexer, bytes: &[u8], open: usize, limit: usize, o: u8, c: u8) -> Option<usize> {
    let mut i = open;
    let mut depth = 0i32;
    while i < limit {
        if let Some(next) = skip_opaque(lx, i, limit) {
            i = next;
            continue;
        }
        if bytes[i] == o {
            depth += 1;
        } else if bytes[i] == c {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}

fn to_end_of_line(bytes: &[u8], mut i: usize, limit: usize) -> usize {
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
        if let Some(next) = skip_opaque(lx, i, span.end) {
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
fn frame_assign(lx: &Lexer, bytes: &[u8], i: usize, limit: usize, col: u32) -> Option<Stmt> {
    let lhs = super::parts::frame_ref_at_pub(bytes, i, limit)?;

    // A single `=` must follow (not `==`, not `+=` — see below).
    let mut j = lhs.span.end;
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

    let lhs_end = lhs.span.end;
    Some(Stmt::Assign(AssignStmt {
        span: Span::new(i, eol),
        col,
        lhs,
        op: TriviaNode {
            span: Span::new(lhs_end, rhs_start),
        },
        rhs: native_parts(lx, bytes, rhs_start, rhs_end),
        rhs_span: Span::new(rhs_start, rhs_end),
        tail: if rhs_end < eol {
            Some(TriviaNode {
                span: Span::new(rhs_end, eol),
            })
        } else {
            None
        },
        terminator,
    }))
}


/// `@@:return(<expr>)` and `@@:self.method(<args>)` — **Frame statements**, both of them.
///
/// framec authored these calls, so framec terminates them. The old compiler lowered the
/// `@@:self` part to a *reference* and left `.report()` as native text with no
/// terminator (#229) — because there was no node to ask.
fn frame_call(lx: &Lexer, bytes: &[u8], i: usize, limit: usize, depth: u32, col: u32) -> Option<Stmt> {
    if !starts(bytes, i, b"@@:", limit) {
        return None;
    }
    // `@@:(expr)` — the CONCISE return form. Same statement as `@@:return(expr)`:
    // set the return value and exit. framec owns it and terminates it.
    if starts(bytes, i, b"@@:(", limit) {
        let open = i + b"@@:".len();
        let close = balanced(lx, bytes, open, limit, b'(', b')')?;
        let e = consume_terminator(bytes, close, limit);
        return Some(Stmt::ReturnCall(ReturnCallStmt {
            span: Span::new(i, e),
            col,
            head: TriviaNode {
                span: Span::new(i, open + 1),
            },
            tail: TriviaNode {
                span: Span::new(close - 1, e),
            },
            depth,
            expr: native_parts(lx, bytes, open + 1, close - 1),
            expr_span: Span::new(open + 1, close - 1),
        }));
    }
    // `@@:return(`
    if starts(bytes, i, b"@@:return(", limit) {
        let open = i + b"@@:return".len();
        let close = balanced(lx, bytes, open, limit, b'(', b')')?;
        let e = consume_terminator(bytes, close, limit);
        return Some(Stmt::ReturnCall(ReturnCallStmt {
            span: Span::new(i, e),
            col,
            head: TriviaNode {
                span: Span::new(i, open + 1),
            },
            tail: TriviaNode {
                span: Span::new(close - 1, e),
            },
            depth,
            expr: native_parts(lx, bytes, open + 1, close - 1),
            expr_span: Span::new(open + 1, close - 1),
        }));
    }
    // `@@:self.method(`
    if starts(bytes, i, b"@@:self.", limit) {
        let ns = i + b"@@:self.".len();
        let mut j = ns;
        while j < limit && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > ns && j < limit && bytes[j] == b'(' {
            let close = balanced(lx, bytes, j, limit, b'(', b')')?;
            let e = consume_terminator(bytes, close, limit);
            return Some(Stmt::SelfCall(SelfCallStmt {
                span: Span::new(i, e),
                col,
                method: String::from_utf8_lossy(&bytes[ns..j]).into_owned(),
                args_text: String::from_utf8_lossy(&bytes[j + 1..close.saturating_sub(1)])
                    .into_owned(),
            }));
        }
    }
    None
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
