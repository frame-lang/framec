//! The handler-head reader, **dogfooded as an `@@[scan(u8)]` system** ([`handler_head_scan.frs`])
//! — the other half of the head-grammar family (its sibling: [`super::state_head_scan`]).
//!
//! [`scan`] reads `name(params) [: T] {` — with the `$>` (enter) / `<$` (exit) event forms —
//! into named registers (`name_kind`/`name_start`/`name_end`, `params_open`/`params_close`,
//! `has_return` + raw `ret_start`/`ret_end`, `open`, `end`, `body_clamped`), or Rejects.
//! The FOUR not-a-handler refusals of the hand code (no name; no `(` on the head line;
//! unbalanced params — the exit the old three-way description missed; no `{` on the head
//! line) share one `$Reject` — they have identical futures, so distinct states would be
//! costume and would violate the scan(u8) pump contract — with the cause articulated in
//! `reject_reason` (readable via [`scan_shape`]). An unbalanced body clamps to `limit`
//! exactly as before, now named (`body_clamped`).
//!
//! **Single source (the design):** `machine::handler_end` (the `StateWalk` boundary leaf) and
//! `machine::handler_at` (the node driver) become projections of ONE run — the member
//! boundary the walk finds and the extent the node carries cannot drift. The type TEXT is
//! the user's and is carried verbatim (raw extent here; trim/empty-to-None stays in the
//! driver-side adapter, which is value work). *Phase status:* WIRED —
//! `machine.rs::handler_head` is the thin adapter over [`scan`], with `handler_end`/`handler_at`
//! reading it; [`handler_head_hand`] (the verbatim offset-recording copy) survives as the
//! differential oracle only, test callers only, retired at C-final.
//!
//! framec owns the WALK; leaves are O(1) byte facts or runs of published systems
//! (`paren_extent`/`body_end` → [`super::delim_balance`]). Bound discipline: leaves are
//! limit-bounded except the two-byte `$>`/`<$` probes and the ident name-start probe, which
//! mirror the hand `.get` len-bounds exactly (output-equivalent — a straddle Rejects exactly
//! where the hand returns `None`, T-H2). Position precondition (T-H9): callers pass
//! `i < limit <= len` (the hand `bytes[i]` panics outside it; the bounds-checked leaves
//! Reject instead — out-of-contract only).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit handler_head_scan.frs | grep -v '^#!\[allow' >
//! handler_head_scan.gen.rs`.

use super::literals::Target;

/// Is `$>` (the enter-event name) at `i`? LEN-bounded two-byte probe — the hand
/// `bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'>')` with the first byte bounds-checked
/// (in-contract identical; the hand panics off-contract, T-H9).
fn at_enter(src: &[u8], i: usize) -> bool {
    i + 1 < src.len() && src[i] == b'$' && src[i + 1] == b'>'
}

/// Is `<$` (the exit-event name) at `i`? LEN-bounded, as [`at_enter`].
fn at_exit(src: &[u8], i: usize) -> bool {
    i + 1 < src.len() && src[i] == b'<' && src[i + 1] == b'$'
}

/// Is a name-start byte (`[A-Za-z_]`) at `i`? LEN-bounded — the hand `is_name_start`'s `.get`
/// exactly.
fn is_name_start_here(src: &[u8], i: usize) -> bool {
    src.get(i)
        .map(|b| b.is_ascii_alphabetic() || *b == b'_')
        .unwrap_or(false)
}

/// Is a name byte (`[A-Za-z0-9_]`) at `i`, inside `limit`? (The hand ident scan's bound.)
fn is_name_byte(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && (src[i].is_ascii_alphanumeric() || src[i] == b'_')
}

/// Is `' '`/`'\t'` at `i`, inside `limit`? (The head is single-line: never `\n`.)
fn is_ws(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && (src[i] == b' ' || src[i] == b'\t')
}

/// Is `(` at `i`, inside `limit`?
fn at_open_paren(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b'('
}

/// Is `:` at `i`, inside `limit`?
fn at_colon(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b':'
}

/// Is `{` at `i`, inside `limit`?
fn at_open_brace(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b'{'
}

/// Is `\n` at `i`, inside `limit`?
fn at_newline(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b'\n'
}

/// Is the byte at `i` part of the return-type text — i.e. inside `limit` and neither `{` nor
/// `\n`? (The hand type/brace-seek loop condition — `k` inside the limit, byte neither `{`
/// nor `\n` — as one named predicate, so the `$RetType` body stays a clean two-way fork. A
/// `{` in the type text truncates it — T-H7, carried.)
fn ret_byte(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] != b'{' && src[i] != b'\n'
}

/// One past the `)` matching the `(` at `open`, or `0` (the absent sentinel — a real extent
/// is always `>= open + 2 > 0`). A run-and-unwrap wrapper of the published
/// [`super::delim_balance`] system; the `None → 0` mapping lets the machine name the
/// unbalanced fork itself (`reject_reason = 3`, T-H3 — the fourth exit).
fn paren_extent(src: &[u8], open: usize, limit: usize, target: Target) -> usize {
    super::delim_balance::balanced(src, open, limit, b'(', b')', target).unwrap_or(0)
}

/// One past the `}` matching the `{` at `open`, or `0` (the absent sentinel). Same wrapper;
/// the machine's `$Body` maps `0` to the hand's exact `unwrap_or(limit)` clamp, NAMED
/// (`body_clamped`, T-H5).
fn body_end(src: &[u8], open: usize, limit: usize, target: Target) -> usize {
    super::delim_balance::balanced(src, open, limit, b'{', b'}', target).unwrap_or(0)
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
        at_colon, at_enter, at_exit, at_newline, at_open_brace, at_open_paren, body_end,
        is_name_byte, is_name_start_here, is_ws, paren_extent, ret_byte, Target,
    };
    include!("handler_head_scan.gen.rs");
}

/// The `HandlerHeadScan` registers of an ACCEPTED head — the parsed GEOMETRY (absolute
/// offsets + flags; Strings and the return-type trim stay in the driver-side adapter).
/// `name_kind`: 0 = identifier, 1 = `$>`, 2 = `<$`. `ret_start`/`ret_end` are the RAW `: T`
/// extent (`has_return` with an empty-after-trim extent is the `f(): {` case, T-H6 —
/// observable here for the first time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerHeadParts {
    pub name_kind: i32,
    pub name_start: usize,
    pub name_end: usize,
    pub params_open: usize,
    pub params_close: usize,
    pub has_return: bool,
    pub ret_start: usize,
    pub ret_end: usize,
    pub open: usize,
    pub end: usize,
    pub body_clamped: bool,
}

/// The FULL register file of one `HandlerHeadScan` run — `accepted` plus every register,
/// including `reject_reason` (1 = no name, 2 = no `(` on the head line, 3 = unbalanced
/// params, 4 = no `{` on the head line; 0 = accepted). One machine run; [`scan`] is its
/// `Option` projection. This is the observability surface the design's `carry-and-name`
/// rulings pay for (T-H1..T-H4): the merged rejection stays merged at the interface, but the
/// fork is named in-register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerHeadShape {
    pub accepted: bool,
    pub reject_reason: i32,
    pub name_kind: i32,
    pub name_start: usize,
    pub name_end: usize,
    pub params_open: usize,
    pub params_close: usize,
    pub has_return: bool,
    pub ret_start: usize,
    pub ret_end: usize,
    pub open: usize,
    pub end: usize,
    pub body_clamped: bool,
}

/// Run the `HandlerHeadScan` system at `i` over the full source with `limit` as config (walk
/// precedent — never a slice) and return the full register file.
pub fn scan_shape(bytes: &[u8], i: usize, limit: usize, target: Target) -> HandlerHeadShape {
    let mut m = fsm::HandlerHeadScan::over(bytes, target, limit);
    let accepted = m.scan_at(i);
    HandlerHeadShape {
        accepted,
        reject_reason: m.reject_reason,
        name_kind: m.name_kind,
        name_start: m.name_start,
        name_end: m.name_end,
        params_open: m.params_open,
        params_close: m.params_close,
        has_return: m.has_return,
        ret_start: m.ret_start,
        ret_end: m.ret_end,
        open: m.open,
        end: m.end,
        body_clamped: m.body_clamped,
    }
}

/// The handler head at `i`, if one opens there — the adapter-facing `Option` projection of
/// [`scan_shape`] (`None` exactly where the hand `handler_head` returns `None`).
pub fn scan(bytes: &[u8], i: usize, limit: usize, target: Target) -> Option<HandlerHeadParts> {
    let s = scan_shape(bytes, i, limit, target);
    if s.accepted {
        Some(HandlerHeadParts {
            name_kind: s.name_kind,
            name_start: s.name_start,
            name_end: s.name_end,
            params_open: s.params_open,
            params_close: s.params_close,
            has_return: s.has_return,
            ret_start: s.ret_start,
            ret_end: s.ret_end,
            open: s.open,
            end: s.end,
            body_clamped: s.body_clamped,
        })
    } else {
        None
    }
}

/// The hand reader, factored (Phase 0) — kept ONLY as the differential-test oracle. This is
/// `machine.rs::handler_head`, loop-for-loop, recording offsets instead of building Strings
/// (so the parts stay comparable without value work), with the body clamp's silent
/// `unwrap_or(limit)` surfaced as the same `body_clamped` flag the system records
/// (`carry-and-name`: values byte-identical, the fork observable). `bytes[i]` panics when
/// `i >= len` exactly as the hand does (T-H9 — the precondition kept loud). The head-line
/// byte facts and the DelimBalance extents route through the SAME shared leaves the system's
/// states call (`ret_byte`/`at_open_brace`/`body_end`) — the decl_read/state_walk oracle
/// precedent: machine and oracle move in lockstep through any Phase-2 leaf edit, and the
/// differential proves the CHAIN. (It also keeps this body free of brace char-literals,
/// which the census's oracle-span matcher cannot brace-match.) Production now runs the
/// system; this factored copy has test callers only.
#[doc(hidden)]
pub fn handler_head_hand(
    bytes: &[u8],
    i: usize,
    limit: usize,
    target: Target,
) -> Option<HandlerHeadParts> {
    // The event name: an identifier, or `$>` (enter) / `<$` (exit).
    let (name_kind, name_start, name_end, mut j) =
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'>') {
            (1i32, i, i + 2, i + 2)
        } else if bytes[i] == b'<' && bytes.get(i + 1) == Some(&b'$') {
            (2i32, i, i + 2, i + 2)
        } else if bytes
            .get(i)
            .map(|b| b.is_ascii_alphabetic() || *b == b'_')
            .unwrap_or(false)
        {
            let mut k = i;
            while k < limit && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                k += 1;
            }
            (0i32, i, k, k)
        } else {
            return None;
        };

    // A `(` must follow (after optional space), then eventually the body opener.
    while j < limit && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    if !at_open_paren(bytes, j, limit) {
        return None;
    }
    let params_open = j;
    let params_close = super::delim_balance::balanced(bytes, j, limit, b'(', b')', target)?;
    let mut k = params_close;
    // Optional return type `: T`, then the opening brace.
    let mut has_return = false;
    let mut ret_start = 0usize;
    let mut ret_end = 0usize;
    while k < limit && (bytes[k] == b' ' || bytes[k] == b'\t') {
        k += 1;
    }
    if at_colon(bytes, k, limit) {
        k += 1;
        has_return = true;
        ret_start = k;
        while ret_byte(bytes, k, limit) {
            k += 1;
        }
        ret_end = k;
    }
    while ret_byte(bytes, k, limit) {
        k += 1;
    }
    if !at_open_brace(bytes, k, limit) {
        return None;
    }
    let open = k;
    let e = body_end(bytes, open, limit, target);
    let (end, body_clamped) = if e > 0 { (e, false) } else { (limit, true) };
    Some(HandlerHeadParts {
        name_kind,
        name_start,
        name_end,
        params_open,
        params_close,
        has_return,
        ret_start,
        ret_end,
        open,
        end,
        body_clamped,
    })
}
