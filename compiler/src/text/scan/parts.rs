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

        // A Frame reference sitting mid-expression in native code.
        if let Some(r) = frame_ref_at(bytes, i, to) {
            flush(&mut parts, text_start, i);
            i = r.span.end;
            parts.push(NativePart::Ref(r));
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
