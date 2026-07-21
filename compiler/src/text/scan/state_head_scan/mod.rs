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
//! Bound discipline: every leaf is limit-bounded, including `is_dollar_name`'s name-start probe
//! (Phase 2 delta D3 — the T-S9 straddle is fixed: the reader never reads past `limit`).
//! Position precondition (T-S8): callers pass `at < limit <= len`;
//! the content at `at` is NOT part of the contract (the reader is total over any byte there).
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit state_head_scan.frs | grep -v '^#!\[allow' >
//! state_head_scan.gen.rs`.

use super::literals::Target;

/// Opaque-skip leaf (Phase 2 D1): the offset past a comment/literal at `i`, or `i` unchanged. A
/// run-and-unwrap wrapper of the shared `machine::skip_opaque` (OpaqueScan policy) — no walk
/// (D3), exactly as `state_walk`/`machine_walk`/`body_walk`/`decl_walk` define their `skip`. The
/// `{`/`=>`-seeks route through it so a `{` or `=>` inside a comment/string no longer steers the
/// head (T-S3 / H1). Machine and oracle call the SAME leaf, so the differential stays locked.
fn skip(src: &[u8], i: usize, limit: usize, target: Target) -> usize {
    super::machine::skip_opaque(src, i, limit, target).unwrap_or(i)
}

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

/// Is `$` + a name-start byte at `i`, wholly inside `limit`? Both the `$` and the name-start
/// byte at `i + 1` are LIMIT-bounded (Phase 2 delta D3, T-S9 fixed): a span cut right after
/// `=> $` — with the name byte beyond `limit` — now yields NO parent instead of reading one
/// byte past `limit` for an empty parent extent. In Phase 1 the `i + 1` probe was LEN-bounded
/// (the hand `.get`); the reader no longer reads past `limit`. Shared by the machine's
/// `$ParentName` and the oracle's parent hunt, so the single edit moves both together.
fn is_dollar_name(src: &[u8], i: usize, limit: usize) -> bool {
    i + 1 < limit
        && src[i] == b'$'
        && (src[i + 1].is_ascii_alphabetic() || src[i + 1] == b'_')
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
        is_name_byte, is_ws, paren_extent, skip, Target,
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
