//! Decompose native code into parts: **text | literal | Frame ref**.
//!
//! This is where the language decision becomes *structure*:
//!
//! > A string's **holes** are code. A string's **content** is not.
//!
//! The old compiler answered that question two different ways depending on which code
//! path arrived — its scanner said a sigil in a string is not a reference, its
//! expression byte-loop said it is, and both shipped (#224). Here there is one
//! recognizer, and the wrong answer is not something the reviewer has to catch: it is
//! **unrepresentable**, because a `FrameRef` can only be produced as a `NativePart` or
//! inside a `Hole`, and never inside `LiteralPart::Content`.

use super::lex::Lexer;
use crate::tree::body::{FrameRef, Hole, LiteralNode, LiteralPart, NativePart, RefKind};
use crate::tree::TriviaNode;
use crate::Span;

/// Split `[from, to)` of native code into parts that **partition it exactly**.
pub fn native_parts(lx: &Lexer, bytes: &[u8], from: usize, to: usize) -> Vec<NativePart> {
    let mut parts = Vec::new();
    let mut text_start = from;
    let mut i = from;

    while i < to {
        // A comment: opaque, and it stays opaque. (It is still a NODE, because framec
        // must know it is there in order NOT to splice a `;` into it — which is
        // precisely what the old compiler did.)
        if let Ok(Some(end)) = lx.comment_at(i) {
            flush(&mut parts, text_start, i);
            let end = end.min(to);
            parts.push(NativePart::Literal(LiteralNode {
                span: Span::new(i, end),
                delim: b'/',
                parts: vec![LiteralPart::Content(TriviaNode {
                    span: Span::new(i, end),
                })],
            }));
            i = end;
            text_start = i;
            continue;
        }

        // A literal. Its CONTENT is bytes; its HOLES are code.
        if let Ok(Some(l)) = lx.literal_at(i) {
            if l.span.end <= to {
                flush(&mut parts, text_start, i);
                parts.push(NativePart::Literal(literal_node(lx, bytes, &l)));
                i = l.span.end;
                text_start = i;
                continue;
            }
        }

        // `@@SystemName(args)` — a STRUCTURED instantiation (spec §1103). **Production runs
        // the dogfooded InstScan system** (shape) **+ the dogfooded ArgScan system**
        // (args — dual-counter angle fork, D-seam-target passes `lx.target()`); the hand
        // `instantiation_at` remains only as the differential oracle. Checked before the
        // plain ref recognizer because it consumes the arg list too.
        if let Some(inst) = super::inst_scan::scan_node(&bytes[..to], i, lx.target()) {
            flush(&mut parts, text_start, i);
            i = inst.span.end;
            parts.push(NativePart::Instantiate(inst));
            text_start = i;
            continue;
        }

        // `@@:self.field.method(args)` — an embedded-system call (RFC-0046). **Production
        // now runs the dogfooded EmbedScan system** (docs/JOURNAL.md); the hand
        // `embed_call_at` remains only as the differential-test oracle. Checked before the
        // plain ref recognizer, which would otherwise swallow `self.field.method` as one
        // context ref and leave `.method()` as invalid native on C.
        if let Some((field, method, args, end)) = super::embed_scan::scan(&bytes[..to], i) {
            flush(&mut parts, text_start, i);
            parts.push(NativePart::EmbedCall(EmbedCall {
                span: Span::new(i, end),
                field,
                method,
                args,
            }));
            i = end;
            text_start = i;
            continue;
        }

        // A Frame reference sitting mid-expression in native code. **Production now runs the
        // dogfooded RefScan system**; the hand `frame_ref_at` remains as the differential
        // oracle and for the statement scanner's LHS.
        if let Some((kind, name, end)) = super::ref_scan::scan(&bytes[..to], i) {
            flush(&mut parts, text_start, i);
            parts.push(NativePart::Ref(FrameRef {
                span: Span::new(i, end),
                kind,
                name,
            }));
            i = end;
            text_start = i;
            continue;
        }

        i += 1;
    }
    flush(&mut parts, text_start, to);
    parts
}

fn flush(parts: &mut Vec<NativePart>, from: usize, to: usize) {
    if from < to {
        parts.push(NativePart::Text(TriviaNode {
            span: Span::new(from, to),
        }));
    }
}

/// A literal, decomposed into content and holes. **They partition the literal.**
fn literal_node(lx: &Lexer, bytes: &[u8], l: &super::lex::LiteralExtent) -> LiteralNode {
    let mut parts = Vec::new();
    let mut cursor = l.span.start;

    for hole in &l.holes {
        // Everything before the hole is CONTENT. framec does not look inside it.
        // A `$.x` here is NOT a Frame reference — and there is no variant of
        // `LiteralPart` that could make it one.
        if cursor < hole.start {
            parts.push(LiteralPart::Content(TriviaNode {
                span: Span::new(cursor, hole.start),
            }));
        }
        // The hole is CODE — an expression position in the target's own grammar.
        parts.push(LiteralPart::Hole(Hole {
            span: *hole,
            parts: native_parts(lx, bytes, hole.start, hole.end),
        }));
        cursor = hole.end;
    }
    if cursor < l.span.end {
        parts.push(LiteralPart::Content(TriviaNode {
            span: Span::new(cursor, l.span.end),
        }));
    }
    LiteralNode {
        span: l.span,
        delim: l.delim,
        parts,
    }
}

/// A Frame reference at `i`, if there is one.
///
/// `$` and `@@` are **Frame's own namespace**. A malformed construct there is a Frame
/// error, not native code — the old compiler could not say that, so unrecognized
/// sigil forms fell through as water and were emitted verbatim into the target, where
/// the *target* compiler complained instead of framec. With no body tree, framec could
/// not distinguish *"native code I must not interpret"* from *"Frame code I failed to
/// parse."*
/// Public for the statement scanner: `@@:self.x = ...` needs to know its LHS.
pub fn frame_ref_at_pub(bytes: &[u8], i: usize, to: usize) -> Option<FrameRef> {
    frame_ref_at(bytes, i, to)
}

/// Public for the differential test that proves the dogfooded `InstScan` system agrees with
/// this hand recognizer.
pub fn instantiation_at_pub(bytes: &[u8], i: usize, to: usize) -> Option<Instantiation> {
    instantiation_at(bytes, i, to)
}

/// Public for the `EmbedScan` differential test.
pub fn embed_call_at_pub(bytes: &[u8], i: usize, to: usize) -> Option<crate::tree::body::EmbedCall> {
    embed_call_at(bytes, i, to)
}

fn frame_ref_at(bytes: &[u8], i: usize, to: usize) -> Option<FrameRef> {
    // `$.name`
    if i + 1 < to && bytes[i] == b'$' && bytes[i + 1] == b'.' {
        let mut j = i + 2;
        while j < to && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > i + 2 {
            return Some(FrameRef {
                span: Span::new(i, j),
                kind: RefKind::StateVar,
                name: String::from_utf8_lossy(&bytes[i + 2..j]).into_owned(),
            });
        }
    }
    // `@@:...`
    if i + 2 < to && bytes[i] == b'@' && bytes[i + 1] == b'@' && bytes[i + 2] == b':' {
        let mut j = i + 3;
        while j < to && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.') {
            j += 1;
        }
        let word = &bytes[i + 3..j];
        let kind = if word.starts_with(b"self") {
            RefKind::ContextSelf
        } else if word.starts_with(b"data") {
            RefKind::ContextData
        } else if word.starts_with(b"params") {
            RefKind::ContextParams
        } else if word.starts_with(b"return") {
            RefKind::ContextReturn
        } else if word.starts_with(b"event") {
            RefKind::ContextEvent
        } else if word.starts_with(b"system") {
            RefKind::ContextSystemState
        } else {
            RefKind::ContextSelf
        };
        if j > i + 3 {
            // `self.factor` -> `factor`; `params.k` -> `k`; `return` -> `return`.
            let full = String::from_utf8_lossy(word).into_owned();
            let name = full
                .split_once('.')
                .map(|(_, rest)| rest.to_string())
                .unwrap_or(full);
            return Some(FrameRef {
                span: Span::new(i, j),
                kind,
                name,
            });
        }
    }
    None
}

use crate::tree::body::{EmbedCall, InstArg, Instantiation, ParamGroup};

/// `@@:self.<field>.<method>(args)` at `i`, if there is one (RFC-0046). Requires TWO
/// dotted segments after `self` and a `(` — that shape is what tells an embed call apart
/// from a scalar field read (`@@:self.x`) or a self-call (`@@:self.m(...)`). Whether the
/// field is actually a system (vs a scalar with a native method) is resolved at emit.
fn embed_call_at(bytes: &[u8], i: usize, to: usize) -> Option<EmbedCall> {
    let head = b"@@:self.";
    if !starts_with(bytes, i, to, head) {
        return None;
    }
    let mut j = i + head.len();
    let fs = j;
    while j < to && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    if j == fs || j >= to || bytes[j] != b'.' {
        return None;
    }
    let field = String::from_utf8_lossy(&bytes[fs..j]).into_owned();
    j += 1; // the `.`
    let ms = j;
    while j < to && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    if j == ms {
        return None;
    }
    let method = String::from_utf8_lossy(&bytes[ms..j]).into_owned();
    // A `(` (skipping spaces) confirms a call — a bare `@@:self.a.b` is a nested field read,
    // not an embed call.
    let mut p = j;
    while p < to && (bytes[p] == b' ' || bytes[p] == b'\t') {
        p += 1;
    }
    if p >= to || bytes[p] != b'(' {
        return None;
    }
    let close = match_paren(bytes, p, to)?;
    let args = String::from_utf8_lossy(&bytes[p + 1..close]).trim().to_string();
    Some(EmbedCall {
        span: Span::new(i, close + 1),
        field,
        method,
        args,
    })
}

fn starts_with(bytes: &[u8], i: usize, to: usize, needle: &[u8]) -> bool {
    i + needle.len() <= to && &bytes[i..i + needle.len()] == needle
}

/// `@@SystemName(args)` at `i`, if there is one (spec §1103). Consumes the whole call —
/// `@@`, an optional `!` (unmanaged variant), the name, and the balanced `(...)` — and
/// parses the args into groups. Inside body water the only `@@` forms are `@@:` (a ref)
/// and this; a `(` after the name confirms a call.
fn instantiation_at(bytes: &[u8], i: usize, to: usize) -> Option<Instantiation> {
    if i + 2 >= to || bytes[i] != b'@' || bytes[i + 1] != b'@' || bytes[i + 2] == b':' {
        return None;
    }
    let mut k = i + 2;
    if k < to && bytes[k] == b'!' {
        k += 1;
    }
    if k >= to || !(bytes[k].is_ascii_alphabetic() || bytes[k] == b'_') {
        return None;
    }
    let ns = k;
    let mut m = k;
    while m < to && (bytes[m].is_ascii_alphanumeric() || bytes[m] == b'_') {
        m += 1;
    }
    let name = String::from_utf8_lossy(&bytes[ns..m]).into_owned();
    // Require `(` (skipping spaces).
    let mut p = m;
    while p < to && (bytes[p] == b' ' || bytes[p] == b'\t') {
        p += 1;
    }
    if p >= to || bytes[p] != b'(' {
        return None;
    }
    // Balanced scan to the matching `)`, string-aware.
    let open = p;
    let close = match_paren(bytes, open, to)?;
    let (args, named) = parse_inst_args_hand(bytes, open + 1, close);
    Some(Instantiation {
        span: Span::new(i, close + 1),
        name,
        args,
        named,
        // The M4 oracle never evaluates the angle hypotheses (the hand had no fork);
        // mechanical `Inert` — D-tree-angles' fourth touch-point.
        angles: crate::tree::body::ArgAngles::Inert,
    })
}

/// Index of the `)` matching the `(` at `open`, or None if unbalanced within `to`.
fn match_paren(bytes: &[u8], open: usize, to: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut j = open;
    while j < to {
        match bytes[j] {
            b'"' | b'\'' => {
                let q = bytes[j];
                j += 1;
                while j < to && bytes[j] != q {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

// The hand arg parser `parse_inst_args` and its two splitters (`split_top_commas`,
// `split_top_eq`) are RETIRED from production: the arg list is parsed by the dogfooded
// **ArgScan** system (`super::arg_scan`), reached through `inst_scan::scan_node`. The
// verbatim copies below (`parse_inst_args_hand` + `_hand` splitters) are the differential
// oracle ONLY — deleted at C-final.

/// The retired hand arg parser — kept ONLY as the ArgScan differential oracle until
/// parity-lock (its two known bugs are pinned by `tests/arg_scan.rs::oracle_stayed_buggy`
/// and must NOT be fixed here; the fixes live in the system). A verbatim, self-contained
/// copy of `parse_inst_args` + its two splitters (delim_balance `balanced_hand`
/// precedent): fully independent of the system under test. The M4 oracle
/// `instantiation_at` calls THIS name, so the oracle chain stays internally consistent.
/// Deleted at C-final with the other `*_hand` oracles.
#[doc(hidden)]
pub fn parse_inst_args_hand(bytes: &[u8], from: usize, to: usize) -> (Vec<InstArg>, bool) {
    let mut args = Vec::new();
    let mut named = false;
    for raw in split_top_commas_hand(bytes, from, to) {
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        // Group by sigil.
        let (group, inner) = if let Some(rest) = s.strip_prefix("$>(") {
            (ParamGroup::Enter, rest.trim_end_matches(')').trim())
        } else if let Some(rest) = s.strip_prefix("$(") {
            (ParamGroup::State, rest.trim_end_matches(')').trim())
        } else {
            (ParamGroup::Domain, s)
        };
        // Named (`name = value`) vs positional. Only a top-level `=` that is not `==`.
        let (name, value) = match split_top_eq_hand(inner) {
            Some((n, v)) => {
                named = true;
                (Some(n.trim().to_string()), v.trim().to_string())
            }
            None => (None, inner.to_string()),
        };
        args.push(InstArg { group, name, value });
    }
    (args, named)
}

/// `split_top_commas`, verbatim — `parse_inst_args_hand`'s private internal (oracle-only).
fn split_top_commas_hand(bytes: &[u8], from: usize, to: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = from;
    let mut j = from;
    while j < to {
        match bytes[j] {
            b'"' | b'\'' => {
                let q = bytes[j];
                j += 1;
                while j < to && bytes[j] != q {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
            }
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b',' if depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..j]).into_owned());
                start = j + 1;
            }
            _ => {}
        }
        j += 1;
    }
    if start < to {
        out.push(String::from_utf8_lossy(&bytes[start..to]).into_owned());
    }
    out
}

/// `split_top_eq`, verbatim — `parse_inst_args_hand`'s private internal (oracle-only).
fn split_top_eq_hand(s: &str) -> Option<(&str, &str)> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut j = 0;
    while j < b.len() {
        match b[j] {
            b'"' | b'\'' => {
                let q = b[j];
                j += 1;
                while j < b.len() && b[j] != q {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                let prev = if j > 0 { b[j - 1] } else { 0 };
                let next = if j + 1 < b.len() { b[j + 1] } else { 0 };
                if !matches!(prev, b'=' | b'!' | b'<' | b'>') && next != b'=' {
                    return Some((&s[..j], &s[j + 1..]));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

