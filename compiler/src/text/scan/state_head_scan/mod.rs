//! The state-head reader, **dogfooded as an `@@[scan(u8)]` system** ([`state_head_scan.frs`]) —
//! one half of the head-grammar family (its sibling: [`super::handler_head_scan`]).
//!
//! [`scan`] reads the whole `$Name(params) => $Parent {` head from the `$` into named
//! registers: `name_end`, the params-group extent (`has_params`, `params_open`/`params_close`,
//! `params_unbalanced`), the parent extent (`has_parent`, `parent_start`/`parent_end`), the
//! body `open` (`open_found`), and the body `end` (`body_clamped` when the `}` never comes —
//! the old silent `unwrap_or(limit)`, now a named register with the clamp value unchanged).
//! TOTAL: it always Accepts — the MachineWalk's `is_state_start` did the gating.
//!
//! **Single source, strengthened (the design):** `machine::state_extent` (the `MachineWalk`
//! boundary leaf) and `machine::state()` (the node driver) become projections of ONE run, so
//! the boundary the walk finds and the node the driver builds cannot drift. Params/parent are
//! separate engine scan-lets in the hand code; here every head field shares the run.
//! *Phase status:* WIRED — `machine.rs::state()` and `state_extent` are projections of one
//! [`scan`] run; [`state_head_hand`] (the hand scan-lets, new-factored verbatim) survives as
//! the differential oracle only, test callers only, retired at C-final.
//!
//! framec owns the WALK (every seek is a per-byte state); the leaves are O(1) byte facts or
//! runs of published systems: `paren_extent`/`body_end` → [`super::delim_balance`].
//! Bound discipline: leaves are limit-bounded except `is_dollar_name`'s name-start probe,
//! which mirrors the hand code's len-bound exactly (the T-S9 straddle, carried until its
//! recorded Phase-2 delta). Position precondition (T-S8): callers pass `at < limit <= len`;
//! the content at `at` is NOT part of the contract (the reader is total over any byte there).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit state_head_scan.frs | grep -v '^#!\[allow' >
//! state_head_scan.gen.rs`.

use super::literals::Target;

/// Is a name byte (`[A-Za-z0-9_]`) at `i`, inside `limit`? (The hand name/parent-ident scans'
/// exact byte class and bound.)
fn is_name_byte(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && (src[i].is_ascii_alphanumeric() || src[i] == b'_')
}

/// Is `(` at `i`, inside `limit`? (The hand params gate: `name_end < limit && bytes[name_end]
/// == b'('` — the group must be ADJACENT to the name.)
fn at_open_paren(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b'('
}

/// Is `{` at `i`, inside `limit`?
fn at_open_brace(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b'{'
}

/// Is `\n` at `i`, inside `limit`?
fn at_newline(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && src[i] == b'\n'
}

/// Is `=>` at `i`, wholly inside `limit`? (The hand `starts(bytes, k, b"=>", limit)` probe —
/// a `=` at `limit - 1` is NOT an arrow.)
fn at_arrow(src: &[u8], i: usize, limit: usize) -> bool {
    i + 2 <= limit && src[i] == b'=' && src[i + 1] == b'>'
}

/// Is `' '`/`'\t'` at `i`, inside `limit`? (The hand parent hunt's ws class — space/tab only,
/// never `\n`: the parent lives on the header's first line.)
fn is_ws(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit && (src[i] == b' ' || src[i] == b'\t')
}

/// Is `$` + a name-start byte at `i`? The `$` check is LIMIT-bounded; the name-start probe at
/// `i + 1` is **LEN-bounded** — mirroring the hand `bytes[p] == b'$' && is_name_start(bytes,
/// p + 1)` (`.get`) EXACTLY. This is the T-S9 limit-straddle, carried in Phase 1: a span cut
/// right after `=> $` with a name byte beyond `limit` reads one byte past `limit` and yields
/// an empty parent extent. Phase 2 (recorded delta D3) makes this limit-bounded.
fn is_dollar_name(src: &[u8], i: usize, limit: usize) -> bool {
    i < limit
        && src[i] == b'$'
        && src
            .get(i + 1)
            .map(|b| b.is_ascii_alphabetic() || *b == b'_')
            .unwrap_or(false)
}

/// One past the `)` matching the `(` at `open`, or `0` (the absent sentinel — a real extent
/// is always `>= open + 2 > 0`). A run-and-unwrap wrapper of the published
/// [`super::delim_balance`] system (the walk stays in the sub-system — D3); the `None → 0`
/// mapping lets the machine name the unbalanced fork itself (`params_unbalanced`, T-S6).
fn paren_extent(src: &[u8], open: usize, limit: usize, target: Target) -> usize {
    super::delim_balance::balanced(src, open, limit, b'(', b')', target).unwrap_or(0)
}

/// One past the `}` matching the `{` at `open`, or `0` (the absent sentinel). Same
/// run-and-unwrap wrapper of [`super::delim_balance`]; the machine's `$Body` maps `0` to the
/// hand's exact `unwrap_or(limit)` clamp, NAMED (`body_clamped`, T-S1).
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
        at_arrow, at_newline, at_open_brace, at_open_paren, body_end, is_dollar_name,
        is_name_byte, is_ws, paren_extent, Target,
    };
    include!("state_head_scan.gen.rs");
}

/// The `StateHeadScan` registers — the parsed GEOMETRY of one state head (absolute offsets +
/// flags; Strings/`params_split` stay in the driver, which is value work). Offsets, not
/// Strings, so the T-S9 empty-parent case (`has_parent` with `parent_start == parent_end`)
/// stays distinguishable from no-parent. The named malformedness registers: `params_unbalanced`
/// (T-S6), `open_found == false` (T-S2), `body_clamped` (T-S1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHeadParts {
    pub name_end: usize,
    pub has_params: bool,
    pub params_open: usize,
    pub params_close: usize,
    pub params_unbalanced: bool,
    pub has_parent: bool,
    pub parent_start: usize,
    pub parent_end: usize,
    pub open: usize,
    pub open_found: bool,
    pub end: usize,
    pub body_clamped: bool,
}

/// Read the whole state head at `at` (the `$`) — driven by the `StateHeadScan` system over the
/// full source with `limit` as config (NOT a slice: Phase-1 fidelity for the T-S9 len-vs-limit
/// straddle requires the bytes past `limit` to be visible to the len-bounded probe, exactly as
/// they are to the hand code). TOTAL — always returns parts.
pub fn scan(bytes: &[u8], at: usize, limit: usize, target: Target) -> StateHeadParts {
    let mut m = fsm::StateHeadScan::over(bytes, target, limit);
    m.scan_at(at);
    StateHeadParts {
        name_end: m.name_end,
        has_params: m.has_params,
        params_open: m.params_open,
        params_close: m.params_close,
        params_unbalanced: m.params_unbalanced,
        has_parent: m.has_parent,
        parent_start: m.parent_start,
        parent_end: m.parent_end,
        open: m.open,
        open_found: m.open_found,
        end: m.end,
        body_clamped: m.body_clamped,
    }
}

/// The hand reader, new-factored (Phase 0) — kept ONLY as the differential-test oracle. This
/// is `machine.rs`'s M3 machinery, loop-for-loop: `state_extent`'s name skip + open seek +
/// clamp, `state()`'s params scan-let and parent hunt — composed in the hand's own shape
/// (independent scans from `name_end`, NOT the system's fused chain — the differential proves
/// the fusion/sequentialization, which is the thing being converted), with each silent fork
/// surfaced as the same named flag the system records (`carry-and-name`: values byte-identical,
/// forks observable). The O(1) byte facts and the DelimBalance extents route through the SAME
/// shared leaves the system's states call (`at_open_brace`/`at_newline`/`is_dollar_name`/
/// `body_end`) — the decl_read/state_walk oracle precedent: machine and oracle move in
/// lockstep through any Phase-2 leaf edit, and the differential proves the CHAIN. (It also
/// keeps this body free of brace char-literals, which the census's oracle-span matcher cannot
/// brace-match.) Production now runs the system; this factored
/// copy has no production callers.
#[doc(hidden)]
pub fn state_head_hand(bytes: &[u8], at: usize, limit: usize, target: Target) -> StateHeadParts {
    // `state_extent`'s name skip, verbatim.
    let mut j = at + 1;
    while j < limit && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    let name_end = j;

    // `state()`'s params scan-let, verbatim (the if-let's silent None arm named T-S6).
    let mut has_params = false;
    let mut params_open = 0usize;
    let mut params_close = 0usize;
    let mut params_unbalanced = false;
    if name_end < limit && bytes[name_end] == b'(' {
        if let Some(pe) =
            super::delim_balance::balanced(bytes, name_end, limit, b'(', b')', target)
        {
            has_params = true;
            params_open = name_end;
            params_close = pe;
        } else {
            params_unbalanced = true;
        }
    }

    // `state()`'s parent hunt, verbatim (offsets instead of the String; the `.get` name-start
    // probe LEN-bounded via the shared `is_dollar_name` — the T-S9 straddle, reproduced).
    let mut has_parent = false;
    let mut parent_start = 0usize;
    let mut parent_end = 0usize;
    let mut k = name_end;
    while k < limit && !at_open_brace(bytes, k, limit) && !at_newline(bytes, k, limit) {
        if k + 2 <= limit && &bytes[k..k + 2] == b"=>" {
            let mut p = k + 2;
            while p < limit && (bytes[p] == b' ' || bytes[p] == b'\t') {
                p += 1;
            }
            if is_dollar_name(bytes, p, limit) {
                let ps = p + 1;
                let mut pe = ps;
                while pe < limit && (bytes[pe].is_ascii_alphanumeric() || bytes[pe] == b'_') {
                    pe += 1;
                }
                has_parent = true;
                parent_start = ps;
                parent_end = pe;
            }
            break;
        }
        k += 1;
    }

    // `state_extent`'s open seek (crossing newlines, not opaque-aware — T-S3) + body extent,
    // verbatim; the `unwrap_or(limit)` clamp NAMED (`body_clamped` fires only when an open
    // was found — the machine's `$Body` else-arm; with no open, `open == end == limit` and
    // `open_found` stays false, T-S2).
    let mut o = name_end;
    while o < limit && !at_open_brace(bytes, o, limit) {
        o += 1;
    }
    let open = o;
    let open_found = open < limit;
    let mut end = limit;
    let mut body_clamped = false;
    if open_found {
        let e = body_end(bytes, open, limit, target);
        if e > 0 {
            end = e;
        } else {
            end = limit;
            body_clamped = true;
        }
    }

    StateHeadParts {
        name_end,
        has_params,
        params_open,
        params_close,
        params_unbalanced,
        has_parent,
        parent_start,
        parent_end,
        open,
        open_found,
        end,
        body_clamped,
    }
}
