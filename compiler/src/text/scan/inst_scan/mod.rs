//! System-instantiation recognizer, **dogfooded as an `@@[scan(u8)]` system** — the
//! `@@system` analogue of the hand [`super::parts`]`::instantiation_at`, and another of this
//! session's byte-loop recognizers undone.
//!
//! [`inst_scan.frs`] recognizes `@@Name(args)` / `@@!Name(args)` at the cursor. framec owns
//! the shape; the arg extent is found by COMPOSING the string-aware [`super::paren_balance`]
//! system. A differential test proves the (name, end) it yields matches `instantiation_at`.
//!
//! `.gen.rs` regen: `framec-ng -l rust --emit inst_scan.frs | grep -v '^#!\[allow' >
//! inst_scan.gen.rs`.

use super::paren_balance;

/// `@@`-not-`:`, then an optional `!`, then an identifier start — the shape of an
/// instantiation (as opposed to a `@@:` context ref or a `@@[` attribute).
fn is_inst_start(src: &[u8], i: usize) -> bool {
    if i + 2 >= src.len() || src[i] != b'@' || src[i + 1] != b'@' || src[i + 2] == b':' {
        return false;
    }
    let k = if src[i + 2] == b'!' { i + 3 } else { i + 2 };
    k < src.len() && (src[k].is_ascii_alphabetic() || src[k] == b'_')
}

/// The offset just past `@@` and an optional `!` — where the NAME begins.
fn after_at_bang(src: &[u8], i: usize) -> usize {
    if i + 2 < src.len() && src[i + 2] == b'!' {
        i + 3
    } else {
        i + 2
    }
}

fn is_ident_at(src: &[u8], i: usize) -> bool {
    i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_')
}
fn skip_ws_at(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i] == b' ' || src[i] == b'\t') {
        i += 1;
    }
    i
}
fn is_open_paren_at(src: &[u8], i: usize) -> bool {
    i < src.len() && src[i] == b'('
}

/// The offset one past the balanced `(...)` starting at `p`, by COMPOSING ParenBalance (so a
/// `)` inside a string arg is not miscounted); `p` if unbalanced (which the caller reads as
/// reject).
fn paren_end(src: &[u8], p: usize) -> usize {
    paren_balance::scan(src, p).unwrap_or(p)
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
        after_at_bang, is_ident_at, is_inst_start, is_open_paren_at, paren_end, skip_ws_at,
    };
    include!("inst_scan.gen.rs");
}

/// A `@@Name(args)` instantiation at `bytes[i..]`, if any: `(name, end)` where `end` is one
/// past the closing paren — the same recognition [`super::parts`]`::instantiation_at` does.
pub fn scan(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    let mut m = fsm::InstScan::over(bytes);
    if !m.scan_at(i) {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[m.name_start..m.name_end]).into_owned();
    Some((name, m.cursor))
}

/// The full `Instantiation` node — the system recognizes the SHAPE, and the arg list is
/// parsed by the **ArgScan system** (`arg_scan::parse`); the hand `parse_inst_args` is
/// retired to `parse_inst_args_hand`, differential-oracle only. This is what production
/// `native_parts` calls (passing `target` — D-seam-target), so InstScan + ArgScan
/// are on the real parse path.
///
/// Pure plumbing — no decision depends on accumulated history: run InstScan (unchanged)
/// for the shape, ws-skip to the `(`, parse the interior with ArgScan, and copy
/// `primary` + `angles` into the node. When the angle hypotheses diverge, BOTH candidates
/// ride the node (`angles: ArgAngles`) and the declared-arity adjudicator downstream
/// picks; this driver never chooses, and MUST never choose — the scan layer has no
/// symbol table (declared arities exist only after resolve). `.refusal`/`.dropped_empty`
/// are dropped here (hand-shaped verbatim degradation; void condition = the §1167
/// validation layer).
pub fn scan_node(
    bytes: &[u8],
    i: usize,
    target: super::literals::Target,
) -> Option<crate::tree::body::Instantiation> {
    let mut m = fsm::InstScan::over(bytes);
    if !m.scan_at(i) {
        return None;
    }
    let name = String::from_utf8_lossy(&bytes[m.name_start..m.name_end]).into_owned();
    let end = m.cursor; // one past the closing paren
    let p = skip_ws_at(bytes, m.name_end);
    let out = super::arg_scan::parse(bytes, p + 1, end.saturating_sub(1), target);
    let angles = match out.angles {
        super::arg_scan::AngleReading::Inert => crate::tree::body::ArgAngles::Inert,
        super::arg_scan::AngleReading::Operators => crate::tree::body::ArgAngles::Operators,
        super::arg_scan::AngleReading::Forked(alt) => crate::tree::body::ArgAngles::Forked {
            alt_args: alt.args,
            alt_named: alt.named,
        },
    };
    Some(crate::tree::body::Instantiation {
        span: crate::Span::new(i, end),
        name,
        args: out.primary.args,
        named: out.primary.named,
        angles,
    })
}
