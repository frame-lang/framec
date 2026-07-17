//! The full string / comment / literal EXTENT skipper, **dogfooded as an `@@[scan(u8)]`
//! system** ([`opaque_scan.frs`]) — the `@@system` that replaces the hand
//! `Lexer::comment_at` + `literal_at` for the four cleanroom targets.
//!
//! At a position it recognizes exactly what those recognize (per the target's `Form` table)
//! and returns the extent one past the string/comment, or `None` if none opens there. Every
//! consumer used only the extent (`.span.end`); holes are dead except that a Python `{…}` is
//! skipped whole while scanning, which [`hole_skip`] reproduces.
//!
//! framec owns the WALK (in the `.frs`). These native leaves answer only per-target FACTS
//! (Category A: which opener is here, is this the close) and — per the systems-only mandate —
//! delegate the two recognizers that would otherwise walk to their own systems: a Rust raw
//! string to [`super::raw_string`] and a Python interpolation hole to [`super::brace_balance`].
//!
//! `.gen.rs` regen: edit `opaque_scan.frs`, then
//! `framec-ng -l rust --emit opaque_scan.frs | grep -v '^#!\[allow' > opaque_scan.gen.rs`.

use super::literals::{Form, Target};

fn starts_with(src: &[u8], i: usize, pat: &[u8]) -> bool {
    src.len() >= i + pat.len() && &src[i..i + pat.len()] == pat
}

/// Length of this target's line-comment opener at `i` (`//` → 2, `#` → 1), or 0.
fn line_comment_len(src: &[u8], i: usize, target: Target) -> usize {
    for form in target.literals().forms {
        if let Form::LineComment(open) = form {
            if starts_with(src, i, open.as_bytes()) {
                return open.len();
            }
        }
    }
    0
}

/// Length of a block-comment opener (`/*` → 2) at `i` for this target, or 0.
fn block_open_len(src: &[u8], i: usize, target: Target) -> usize {
    for form in target.literals().forms {
        if let Form::BlockComment { open, .. } = form {
            if starts_with(src, i, open.as_bytes()) {
                return open.len();
            }
        }
    }
    0
}

/// Does this target's block comment nest (Rust yes; C/Java no)?
fn block_nests(target: Target) -> bool {
    for form in target.literals().forms {
        if let Form::BlockComment { nests, .. } = form {
            return *nests;
        }
    }
    false
}

/// Length of this target's block-comment close (`*/` → 2) at `i`, or 0 — table-driven from the
/// form's `close` field (so a target whose close is not `*/`, e.g. Ruby `=end`, is correct).
fn block_close_len(src: &[u8], i: usize, target: Target) -> usize {
    for form in target.literals().forms {
        if let Form::BlockComment { close, .. } = form {
            if starts_with(src, i, close.as_bytes()) {
                return close.len();
            }
        }
    }
    0
}

/// The triple-quote delimiter at `i` (`"""` → b'"', `'''` → b'\''), or 0 (Python only).
fn triple_delim(src: &[u8], i: usize, target: Target) -> u8 {
    for form in target.literals().forms {
        if let Form::TripleQuoted { delim } = form {
            let d = *delim;
            if starts_with(src, i, &[d, d, d]) {
                return d;
            }
        }
    }
    0
}

/// Is `delim delim delim` at `i`?
fn triple_close(src: &[u8], i: usize, delim: u8) -> bool {
    starts_with(src, i, &[delim, delim, delim])
}

/// The plain quoted-string delimiter at `i` (`"` or `'` if this target has that form), or 0.
fn string_delim(src: &[u8], i: usize, target: Target) -> u8 {
    let b = match src.get(i) {
        Some(x) => *x,
        None => return 0,
    };
    for form in target.literals().forms {
        if let Form::Quoted { delim, .. } = form {
            if *delim == b {
                return b;
            }
        }
    }
    0
}

/// Does this target's `delim` string span newlines (Rust `"` yes)?
fn string_multiline(target: Target, delim: u8) -> bool {
    for form in target.literals().forms {
        if let Form::Quoted {
            delim: d,
            multiline,
            ..
        } = form
        {
            if *d == delim {
                return *multiline;
            }
        }
    }
    false
}

/// Does this target have a raw-string form at all?
fn has_raw_form(target: Target) -> bool {
    target
        .literals()
        .forms
        .iter()
        .any(|f| matches!(f, Form::RustRaw))
}

/// Category-B, delegated to a system: a Rust raw string at `i` via the `RawString` @@system.
/// Returns the extent end, or `i` if there is no *closed* raw string (none opens, or one opens
/// but never closes — the unterminated case is reported separately by [`raw_unterminated`]).
fn raw_scan(src: &[u8], i: usize, target: Target) -> usize {
    if !has_raw_form(target) {
        return i;
    }
    match super::raw_string::scan_kind(src, i) {
        super::raw_string::RawAt::Extent(end) => end,
        _ => i,
    }
}

/// Category-B, delegated: does a raw string OPEN at `i` but never close? Splits the raw dispatch
/// so `OpaqueScan` can distinguish an unterminated literal (a Reject carrying `unterminated`)
/// from "nothing opens here" — the `#`-counting stays entirely in the `RawString` sub-system.
fn raw_unterminated(src: &[u8], i: usize, target: Target) -> bool {
    if !has_raw_form(target) {
        return false;
    }
    matches!(
        super::raw_string::scan_kind(src, i),
        super::raw_string::RawAt::Unterminated
    )
}

/// Category-B, delegated to a system: a Python interpolation hole `{…}` at `i`, brace-balanced
/// by the `BraceBalance` @@system. Returns the position past the matching `}`, or `i` if there
/// is no hole here — matching `Lexer::hole_at` + `i = hole.end + 1`. `{{` is an escaped brace.
fn hole_skip(src: &[u8], i: usize, target: Target) -> usize {
    let hole_here = matches!(target, Target::Python3) && src.get(i) == Some(&b'{');
    if !hole_here {
        return i;
    }
    if src.get(i + 1) == Some(&b'{') {
        return i;
    }
    super::brace_balance::scan(src, i).unwrap_or(i)
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
        block_close_len, block_nests, block_open_len, hole_skip, line_comment_len, raw_scan,
        raw_unterminated, string_delim, string_multiline, triple_close, triple_delim, Target,
    };
    include!("opaque_scan.gen.rs");
}

/// The full three-way classification of position `i` for `target`, exposing the machine's
/// `kind` and `unterminated` registers: nothing opaque opens here; a comment (with extent end);
/// a string/char literal (with extent end); or a body that OPENS here but never closes (the
/// hand `Lexer` returns `Err` in exactly this case). This is the signal `close_brace` (Item 2)
/// needs — an unterminated body must abort the scan the way the hand path's `?` did, and a
/// consumer that limits comments differently from literals (Item 3) needs the `kind`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OpaqueAt {
    None,
    Comment(usize),
    Literal(usize),
    Unterminated,
}

/// Classify what opens at `bytes[i]` for `target`. The machine finds the extent and records
/// `kind`/`unterminated`; this wrapper only runs it and reads those registers.
pub fn opaque_at(bytes: &[u8], i: usize, target: Target) -> OpaqueAt {
    if i >= bytes.len() {
        return OpaqueAt::None;
    }
    let mut m = fsm::OpaqueScan::over(bytes, target);
    if m.scan_at(i) {
        if m.kind == 1 {
            OpaqueAt::Comment(m.cursor)
        } else {
            OpaqueAt::Literal(m.cursor)
        }
    } else if m.unterminated {
        OpaqueAt::Unterminated
    } else {
        OpaqueAt::None
    }
}

/// If a string or comment opens *and closes* at `bytes[i]` for `target`, return the offset one
/// past it; otherwise `None`. This is the production string/comment recognizer — the `@@system`
/// replacement for `Lexer::comment_at`/`literal_at` (which every consumer used only for the
/// extent). Extent-only adapter over [`opaque_at`]; an unterminated body maps to `None`, exactly
/// as the hand path's `Err` did to every extent-only caller.
pub fn opaque_extent(bytes: &[u8], i: usize, target: Target) -> Option<usize> {
    match opaque_at(bytes, i, target) {
        OpaqueAt::Comment(end) | OpaqueAt::Literal(end) => Some(end),
        OpaqueAt::None | OpaqueAt::Unterminated => None,
    }
}
