//! Decompose native code into parts: **text | literal | Frame ref | instantiation |
//! embed-call**.
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
//!
//! Recognition is the dogfooded `NativePartsScan` `@@[scan(u8)]` system (the walk: island
//! boundaries + kinds + text runs); THIS module is the construction driver — it folds the
//! walk's `(kind, start, end)` triples into `NativePart` nodes, sources each literal's
//! delim + holes from `opaque_scan::opaque_probe` (one machine, one source), and recurses
//! into holes (`literal_node` ↔ `native_parts` — a pushdown whose stack is the host call
//! stack; per-level walks are independent, so the flat system loses nothing). The guarantee
//! stands for lexed literals. **Boundary, named:** an UNTERMINATED literal's interior is
//! water today (the lexer's refusal is carried, not honored — ledger T-N1/T-N2, fix
//! scheduled at Δ3), so the guarantee is conditional there.

use super::literals::Target;
use crate::tree::body::{FrameRef, Hole, LiteralNode, LiteralPart, NativePart};
use crate::tree::TriviaNode;
use crate::Span;

/// Split `[from, to)` of native code into parts that **partition it exactly**.
///
/// The construction driver over the `NativePartsScan` system: the walk finds every island
/// boundary and kind; this fold re-runs each island's OWNING system for its node fields
/// (single-source/double-run — each recognizer is the one source of both the walk extent and
/// the node; every `debug_assert_eq!` is a drift tripwire) and recurses into holes.
pub fn native_parts(bytes: &[u8], from: usize, to: usize, target: Target) -> Vec<NativePart> {
    debug_assert!(
        from <= to && to <= bytes.len(),
        "native_parts bounds: from {from} <= to {to} <= len {}",
        bytes.len()
    );
    let mut parts = Vec::new();
    for (kind, s, e) in super::native_parts_scan::parts(bytes, from, to, target) {
        parts.push(match kind {
            0 => NativePart::Text(TriviaNode {
                span: Span::new(s, e),
            }),
            // A comment: opaque, and it stays opaque. (It is still a NODE, because framec
            // must know it is there in order NOT to splice a `;` into it — which is
            // precisely what the old compiler did.) Δ4 (T-N5): the `delim` is the ACTUAL opener
            // byte (`#` for Python, `/` for a block/line comment) sourced from the SAME machine
            // the walk ran (`opaque_probe`) — no longer the fabricated `b'/'`.
            5 => {
                let p = super::opaque_scan::opaque_probe(bytes, s, target)
                    .expect("walk-confirmed comment start must probe");
                NativePart::Literal(LiteralNode {
                    span: Span::new(s, e),
                    delim: p.delim,
                    parts: vec![LiteralPart::Content(TriviaNode {
                        span: Span::new(s, e),
                    })],
                })
            }
            // A literal. Its CONTENT is bytes; its HOLES are code. delim + holes come from
            // the SAME machine the walk ran (`opaque_probe` — one source).
            1 => {
                let p = super::opaque_scan::opaque_probe(bytes, s, target)
                    .expect("walk-confirmed literal start must probe");
                debug_assert_eq!(p.end, e, "walk/probe extent drift at literal {s}");
                NativePart::Literal(literal_node(bytes, s, e, p.delim, &p.holes, target))
            }
            // `@@SystemName(args)` — a STRUCTURED instantiation (spec §1103): the InstScan
            // system (shape) + the ArgScan system (args), exactly as the walk saw it.
            3 => {
                let inst = super::inst_scan::scan_node(&bytes[..to], s, target)
                    .expect("walk-confirmed instantiation must re-scan");
                debug_assert_eq!(inst.span.end, e, "walk/inst extent drift at {s}");
                NativePart::Instantiate(inst)
            }
            // `@@:self.field.method(args)` — an embedded-system call (RFC-0046).
            4 => {
                let (field, method, args, end) = super::embed_scan::scan(&bytes[..to], s)
                    .expect("walk-confirmed embed call must re-scan");
                debug_assert_eq!(end, e, "walk/embed extent drift at {s}");
                NativePart::EmbedCall(EmbedCall {
                    span: Span::new(s, end),
                    field,
                    method,
                    args,
                })
            }
            // A Frame reference sitting mid-expression in native code — the RefScan system.
            2 => {
                let (kind, name, end) = super::ref_scan::scan(&bytes[..to], s)
                    .expect("walk-confirmed ref must re-scan");
                debug_assert_eq!(end, e, "walk/ref extent drift at {s}");
                NativePart::Ref(FrameRef {
                    span: Span::new(s, end),
                    kind,
                    name,
                })
            }
            k => unreachable!("NativePartsScan emits kinds 0..=5, got {k}"),
        });
    }
    parts
}

/// A literal, decomposed into content and holes. **They partition the literal.** The fold
/// shape is the hand `literal_node`'s, verbatim; delim + holes arrive from the walk's own
/// machine (`opaque_probe`) instead of the hand `LiteralExtent`.
fn literal_node(
    bytes: &[u8],
    start: usize,
    end: usize,
    delim: u8,
    holes: &[(usize, usize)],
    target: Target,
) -> LiteralNode {
    let mut parts = Vec::new();
    let mut cursor = start;

    for &(hs, he) in holes {
        // Everything before the hole is CONTENT. framec does not look inside it.
        // A `$.x` here is NOT a Frame reference — and there is no variant of
        // `LiteralPart` that could make it one.
        if cursor < hs {
            parts.push(LiteralPart::Content(TriviaNode {
                span: Span::new(cursor, hs),
            }));
        }
        // The hole is CODE — an expression position in the target's own grammar. The
        // recursion re-enters the same system one level down (T-N6: a pushdown whose
        // stack is the host call stack — leave-latent, plea in the design record).
        parts.push(LiteralPart::Hole(Hole {
            span: Span::new(hs, he),
            parts: native_parts(bytes, hs, he, target),
        }));
        cursor = he;
    }
    if cursor < end {
        parts.push(LiteralPart::Content(TriviaNode {
            span: Span::new(cursor, end),
        }));
    }
    LiteralNode {
        span: Span::new(start, end),
        delim,
        parts,
    }
}

use crate::tree::body::EmbedCall;
