//! The decl-line reader, **dogfooded as an `@@[scan(u8)]` system** ([`decl_read.frs`]) — a
//! per-decl register TRANSDUCER, the grammar-phase (M3/M4) template: offset registers + a total
//! no-`$Reject` reader + a native builder + NAMED malformedness registers (`empty_name` = ledger
//! T7, `params_clamped` = T8) instead of silent output shapes.
//!
//! [`read`] runs the `DeclRead` system over the window `[from, to)` (the walk's answer: eol for
//! a line decl, the body `{` for a body-decl signature), StmtScan-style — the input is sliced to
//! `to`, offsets stay absolute — and returns the register [`DeclShape`]. [`member_decl_of`]
//! materializes the [`MemberDecl`] node from it (slice, trim, empty→`None` — the machine records
//! geometry, the builder makes values). The hand `machine.rs::decl_of` chain is ROUTED here
//! (Item 3d M-wire — both its production callers: `decl_section`'s members/signatures and
//! `state()`'s `$.x` state-var branch); the hand reader survives ONLY as [`decl_of_hand`], the
//! differential oracle.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit decl_read.frs | grep -v '^#!\[allow' >
//! decl_read.gen.rs`.

use super::literals::Target;
use crate::tree::MemberDecl;
use crate::Span;

/// Skip `' '`/`'\t'` from `i` — the hand reader's exact indent/gap byte class (NOT full
/// whitespace: a decl line never spans a `\n`).
fn indent_end(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i] == b' ' || src[i] == b'\t') {
        i += 1;
    }
    i
}

/// The end of the identifier run at `i` (`[A-Za-z0-9_]*`, possibly empty) — the hand name/sys
/// scan's exact byte class.
fn ident_end(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_') {
        i += 1;
    }
    i
}

/// Is byte `b` at `i`? (An O(1) fact — the reader's `(`/`:`/`=` gates.)
fn at_byte(src: &[u8], i: usize, b: u8) -> bool {
    i < src.len() && src[i] == b
}

/// Is `async` at `i` a MODIFIER? Exactly the hand guard: the literal `async` followed by a
/// `' '`/`'\t'` INSIDE the window. `async(` / `asyncFoo` / bare `async` at window end are a NAME.
fn async_modifier_at(src: &[u8], i: usize) -> bool {
    i + 5 < src.len()
        && &src[i..i + 5] == b"async"
        && (src[i + 5] == b' ' || src[i + 5] == b'\t')
}

/// The offset of the first `=` at or after `i`, else the window end — the hand type scan's
/// byte-blind `=` find (ledger T12, carried: a `=` inside type text truncates it; documented,
/// revisit in the grammar phase).
fn eq_or_end(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && src[i] != b'=' {
        i += 1;
    }
    i
}

/// The `@@Sys` / `@@!Sys` initializer probe: from `i`, skip `' '`/`'\t'`; if `@@` opens here
/// (with room for a name — the hand's strict `k + 2 < to` guard) return the offset past `@@`
/// and an optional `!`, else return the window end as the absent sentinel (`ident_end` then
/// returns the same offset, so `has_sys` stays false — matching the hand's every edge,
/// including `= @@` at the window end).
fn sys_start(src: &[u8], i: usize) -> usize {
    let mut k = i;
    while k < src.len() && (src[k] == b' ' || src[k] == b'\t') {
        k += 1;
    }
    if k + 2 < src.len() && src[k] == b'@' && src[k + 1] == b'@' {
        let mut n = k + 2;
        if n < src.len() && src[n] == b'!' {
            n += 1;
        }
        n
    } else {
        src.len()
    }
}

/// One past the `)` that closes the `(` at `open`, or `0` (the absent sentinel — a real close
/// is always `>= open + 2`).
///
/// **Phase A (parity): the hand bare `(`/`)` counter, verbatim from `machine.rs::decl_of` —
/// string-BLIND (a `)` in a string default mis-closes, a `(` mis-deepens: ledger T9). This is a
/// RECORDED guardrail-4 exception with a bounded lifetime (owner gate 2026-07-18): GATE-B does
/// not close until Phase B replaces this body with
/// `delim_balance::balanced(src, open, src.len(), b'(', b')', target).unwrap_or(0)` — one edit,
/// machine and oracle in lockstep. The counter never survives a landed capability.** `target`
/// is threaded from day one so Phase B touches only this leaf.
fn params_close(src: &[u8], open: usize, _target: Target) -> usize {
    let mut d = 0i32;
    let mut i = open;
    while i < src.len() {
        match src[i] {
            b'(' => d += 1,
            b')' => {
                d -= 1;
                if d == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    0
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
        async_modifier_at, at_byte, eq_or_end, ident_end, indent_end, params_close, sys_start,
        Target,
    };
    include!("decl_read.gen.rs");
}

/// The `DeclRead` registers — the parsed GEOMETRY of one declaration window (absolute offsets
/// + flags). `empty_name` (ledger T7) and `params_clamped` (T8) are the named malformedness
/// registers; the builder [`member_decl_of`] turns the geometry into node values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclShape {
    pub is_async: bool,
    pub empty_name: bool,
    pub name_s: usize,
    pub name_e: usize,
    pub has_params: bool,
    pub params_open: usize,
    pub params_close: usize,
    pub params_clamped: bool,
    pub has_type: bool,
    pub type_s: usize,
    pub type_e: usize,
    pub has_init: bool,
    pub init_s: usize,
    pub has_sys: bool,
    pub sys_s: usize,
    pub sys_e: usize,
}

/// Read the declaration in `bytes[from..to)` — driven by the `DeclRead` system over the sliced
/// window (`&bytes[..to]`, StmtScan-style; offsets stay absolute). Total: every window yields a
/// shape.
pub fn read(bytes: &[u8], from: usize, to: usize, target: Target) -> DeclShape {
    let mut m = fsm::DeclRead::over(&bytes[..to], target);
    m.scan_at(from);
    DeclShape {
        is_async: m.is_async,
        empty_name: m.empty_name,
        name_s: m.name_s,
        name_e: m.name_e,
        has_params: m.has_params,
        params_open: m.params_open,
        params_close: m.params_close,
        params_clamped: m.params_clamped,
        has_type: m.has_type,
        type_s: m.type_s,
        type_e: m.type_e,
        has_init: m.has_init,
        init_s: m.init_s,
        has_sys: m.has_sys,
        sys_s: m.sys_s,
        sys_e: m.sys_e,
    }
}

/// Materialize the [`MemberDecl`] node from a [`DeclShape`] — pure, total: slice the register
/// spans, trim, map empty→`None`. The clamp flag picks the params text exactly as the hand code
/// did (`[open+1..close-1]` balanced / `[open+1..to]` clamped, ledger T8). Trimming and
/// empty→`None` live HERE — the machine records geometry, this builder makes values.
pub fn member_decl_of(bytes: &[u8], shape: &DeclShape, to: usize, span_start: usize) -> MemberDecl {
    let name = String::from_utf8_lossy(&bytes[shape.name_s..shape.name_e]).into_owned();

    let params_text = if shape.has_params {
        let end = if shape.params_clamped {
            shape.params_close
        } else {
            shape.params_close - 1
        };
        Some(String::from_utf8_lossy(&bytes[shape.params_open + 1..end]).into_owned())
    } else {
        None
    };

    let type_text = if shape.has_type {
        let t = String::from_utf8_lossy(&bytes[shape.type_s..shape.type_e])
            .trim()
            .to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    } else {
        None
    };

    let init_text = if shape.has_init {
        let raw = String::from_utf8_lossy(&bytes[shape.init_s..to])
            .trim()
            .to_string();
        if raw.is_empty() {
            None
        } else {
            Some(raw)
        }
    } else {
        None
    };

    let init_system = if shape.has_sys {
        Some(String::from_utf8_lossy(&bytes[shape.sys_s..shape.sys_e]).into_owned())
    } else {
        None
    };

    MemberDecl {
        span: Span::new(span_start, to),
        name,
        type_text,
        params_text,
        init_system,
        is_async: shape.is_async,
        init_text,
    }
}

/// The hand reader, factored — kept ONLY as the differential-test oracle until the parity is
/// locked and the hand recognition is deleted. This is today's `machine.rs::decl_of` VERBATIM,
/// plus the pass-through `target` parameter (unused by the Phase-A counter) and its params scan
/// factored onto the SAME shared [`params_close`] leaf the system's `$Params` calls — machine
/// and oracle move in lockstep through the Phase-B leaf swap, so the differential proves the
/// CHAIN (state sequencing + register capture + builder), which is the thing being converted.
/// Not used in production.
#[doc(hidden)]
pub fn decl_of_hand(
    bytes: &[u8],
    from: usize,
    to: usize,
    span_start: usize,
    target: Target,
) -> MemberDecl {
    let mut i = from;
    while i < to && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }

    // `async fetch(...)` — `async` is a MODIFIER, not the name (the recorded bug fix).
    let mut is_async = false;
    if i + 5 <= to && &bytes[i..i + 5] == b"async" {
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

    // Params, if this is a signature — via the SHARED `params_close` leaf.
    let mut params_text = None;
    if i < to && bytes[i] == b'(' {
        let open = i;
        let c = params_close(&bytes[..to], open, target);
        if c > open {
            params_text = Some(String::from_utf8_lossy(&bytes[open + 1..c - 1]).into_owned());
            i = c;
        } else {
            params_text = Some(String::from_utf8_lossy(&bytes[open + 1..to]).into_owned());
            i = to;
        }
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

    // The initializer, VERBATIM; `@@Sys(...)` / `@@!Sys(...)` marks a system field.
    let mut init_text = None;
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
            if n < to && bytes[n] == b'!' {
                n += 1;
            }
            let ns2 = n;
            while n < to && (bytes[n].is_ascii_alphanumeric() || bytes[n] == b'_') {
                n += 1;
            }
            if n > ns2 {
                init_system = Some(String::from_utf8_lossy(&bytes[ns2..n]).into_owned());
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
