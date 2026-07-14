//! Re-indentation. **The bytes inside a string literal are never touched.**
//!
//! # The bug this makes impossible
//!
//! A native statement's bytes were written at *Frame's* nesting depth. The emitted
//! method sits at the *target's*. So something must re-indent — there is no way around
//! it.
//!
//! The old compiler did it as `normalize_indentation` (`utility.rs:127`): a
//! **post-emission** pass over already-generated text — `.lines()`, `.min()`, slice off
//! the common margin. It had no idea where anything was, because by then everything was
//! a `String`. So it stripped the margin off **every** line, including the lines inside
//! a multi-line string literal:
//!
//! ```text
//! source literal value    : '\n<16 spaces>ALPHA\n<16 spaces>'
//! generated program prints: '\n<8 spaces>ALPHA\n<8 spaces>'      <- exit 0
//! ```
//!
//! **The user's string had a different value at runtime than in their source** (#215),
//! silently. That breaks verbatim passthrough — an architectural boundary. It is
//! Python where it shows first; Rust `r#"…"#`, C++ `R"(…)"`, JS templates and Go
//! backtick strings are all exposed.
//!
//! # Why it cannot happen here
//!
//! Re-indentation is a **fold over nodes**, and it only ever rewrites
//! [`NativePart::Text`]. A [`LiteralNode`]'s bytes are emitted **verbatim**, because
//! they are a different node and the fold has no arm that would touch them.
//!
//! The fix is not "remember not to re-indent inside literals." It is that the code
//! that re-indents **cannot see** literal content as something re-indentable — it is a
//! different variant, and the compiler enumerates them.
//!
//! That is the whole thesis in one function: *the bug and the missing node were the
//! same fact.*

use super::atom::Atom;
use super::super::Source;
use crate::tree::body::{FrameRef, LiteralPart, NativePart, NativeStmt};
use crate::NativeText;

/// How a backend turns a Frame reference into target code.
///
/// It returns an [`Atom`] — not a `String`. So a backend **cannot** hand back a bare
/// cast, a bare deref, or a bare `await`, because there is no constructor that produces
/// one. The precedence bug (#213: compiled clean, exit 0, wrong answer) is not caught
/// here; it is *unrepresentable* here.
pub type LowerRef<'a> = &'a dyn Fn(&FrameRef) -> Atom;

/// Render a native statement: re-indent by `delta`, and expand its Frame references.
pub fn render_native(src: &Source, stmt: &NativeStmt, delta: i32, lower: LowerRef) -> NativeText {
    let bytes = src.open();
    let mut out = String::with_capacity(stmt.span.len() + 16);

    for part in &stmt.parts {
        emit_part(bytes, part, delta, lower, &mut out);
    }

    NativeText::new(out, stmt.span)
}

/// Render a run of parts (e.g. the RHS of a Frame assignment). Same rules: literals
/// verbatim, refs lowered to atoms, ordinary text re-indented.
pub fn render_parts(
    src: &Source,
    parts: &[NativePart],
    span: crate::Span,
    lower: LowerRef,
) -> NativeText {
    let bytes = src.open();
    let mut out = String::new();
    for p in parts {
        emit_part(bytes, p, 0, lower, &mut out);
    }
    NativeText::new(out.trim().to_string(), span)
}

fn emit_part(bytes: &[u8], part: &NativePart, delta: i32, lower: LowerRef, out: &mut String) {
    match part {
        // Ordinary target code. framec may re-indent this — these bytes are layout,
        // and layout is the one thing about the user's code framec is allowed to
        // change (it must, to nest the statement inside a generated method).
        NativePart::Text(t) => {
            let s = &bytes[t.span.start..t.span.end];
            reindent_text(s, delta, out);
        }

        // *** A LITERAL. VERBATIM. ***
        //
        // Note there is no `delta` in this arm, and there cannot be one: re-indenting
        // here would change the VALUE of the user's string. The old compiler could not
        // express this distinction, because by the time it re-indented, the literal was
        // not a node — it was just more characters in a `String`.
        NativePart::Literal(l) => {
            for lp in &l.parts {
                match lp {
                    // String CONTENT. Bytes. Copied, never touched.
                    LiteralPart::Content(c) => {
                        out.push_str(&String::from_utf8_lossy(&bytes[c.span.start..c.span.end]));
                    }
                    // A HOLE is code — but it lives *inside* a literal, so re-indenting
                    // it would still change the string's value. Holes are where framec
                    // may SPLICE (a Frame ref becomes target code); they are not where
                    // framec may REFLOW.
                    LiteralPart::Hole(h) => {
                        for p in &h.parts {
                            emit_part(bytes, p, 0, lower, out); // delta = 0. Always.
                        }
                    }
                }
            }
        }

        // A Frame reference. The backend lowers it — to an ATOM, so that splicing it into
        // an expression framec has not parsed is SOUND regardless of what surrounds it.
        //
        // Note that this arm can reach inside a string literal, but only via a HOLE
        // (above). A ref in string CONTENT does not exist as a node, so it cannot be
        // lowered — which is the language rule, made structural.
        NativePart::Ref(r) => {
            out.push_str(lower(r).as_str());
        }
    }
}

/// Shift the indentation of every line **start** in this run of ordinary code.
fn reindent_text(s: &[u8], delta: i32, out: &mut String) {
    if delta == 0 {
        out.push_str(&String::from_utf8_lossy(s));
        return;
    }
    let text = String::from_utf8_lossy(s);
    let mut first = true;
    for (n, line) in text.split_inclusive('\n').enumerate() {
        // The first chunk continues whatever line we were already on — its leading
        // bytes are not an indent, they are mid-line code. Only shift a line we can
        // see the *start* of.
        if n == 0 && first {
            first = false;
            out.push_str(line);
            continue;
        }
        let ws = line.len() - line.trim_start_matches([' ', '\t']).len();
        let new_ws = (ws as i32 + delta).max(0) as usize;
        out.push_str(&" ".repeat(new_ws));
        out.push_str(&line[ws..]);
    }
}


/// Emit a span **verbatim**. This is the water: the user's native code, carried, never
/// interpreted, never re-indented.
pub fn render_span(src: &Source, span: crate::Span) -> NativeText {
    let bytes = src.open();
    NativeText::new(
        String::from_utf8_lossy(&bytes[span.start..span.end]).into_owned(),
        span,
    )
}
