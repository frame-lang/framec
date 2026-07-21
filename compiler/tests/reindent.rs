//! **#215 is not fixed here. It is impossible here.**
//!
//! The old compiler's `normalize_indentation` was a post-emission `.lines()`/`.min()`
//! pass over already-generated text. It had no idea where anything *was*, because by
//! then everything was a `String` — so it stripped the left margin off every line
//! including the lines inside a multi-line string literal, and **the user's string had
//! a different value at runtime than in their source**. Exit 0. No warning.
//!
//! That broke verbatim passthrough, which is an architectural boundary: *framec
//! transforms nothing in user source — types, values and comments verbatim.*
//!
//! Here, re-indentation is a fold over nodes. It rewrites `NativePart::Text` and it
//! has **no arm** that could rewrite `LiteralPart::Content`. The fix is not "remember
//! not to touch literals" — it is that the code doing the re-indenting *cannot see*
//! literal content as something re-indentable. Different variant; the compiler
//! enumerates them.
//!
//! The bug and the missing node were the same fact.

use frame_compiler::text::emit::atom::Atom;
use frame_compiler::text::emit::reindent::render_native;
use frame_compiler::text::emit::Sink;
use frame_compiler::tree::body::FrameRef;

/// These tests are about LAYOUT, not lowering — so refs pass through as themselves.
/// (Note we still have to go through `Atom`: there is no way to hand back raw text,
/// which is the point.)
fn identity(r: &FrameRef) -> Atom {
    Atom::ident(format!("$.{}", r.name))
}
fn identity_inst(i: &frame_compiler::tree::body::Instantiation) -> Atom {
    Atom::ident(format!("@@{}()", i.name))
}
fn identity_embed(e: &frame_compiler::tree::body::EmbedCall) -> Atom {
    Atom::ident(format!("@@:self.{}.{}({})", e.field, e.method, e.args))
}
fn identity_lowering<'a>() -> frame_compiler::text::emit::reindent::Lowering<'a> {
    frame_compiler::text::emit::reindent::Lowering {
        reference: &identity,
        instantiate: &identity_inst,
        embed: &identity_embed,
    }
}
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::parts::native_parts;
use frame_compiler::tree::body::NativeStmt;
use frame_compiler::{Source, Span};

/// Build a native statement over the whole `code` (its parts via the production `native_parts`
/// decomposition — a run of consecutive native water is ONE statement, exactly as `body()` groups
/// it) and re-indent it by `delta`. `render_native` folds over the parts, so this exercises the
/// same Text-reindent / Literal-verbatim protection production uses.
fn reindented(code: &str, target: Target, delta: i32) -> String {
    let src = Source::new("t", code.as_bytes().to_vec()).unwrap();
    let bytes = code.as_bytes();
    let stmt = NativeStmt {
        span: Span::new(0, bytes.len()),
        parts: native_parts(bytes, 0, bytes.len(), target),
        logical_indent: 0,
        block_depth: Some(0),
    };

    // NOTE: this test cannot read the rendered text directly — it is outside `crate::text`, so
    // `NativeText::finish` is private to it. It must go through the SINK, like every other
    // consumer. The wall binds the tests too: if a test could grep emitted text, so could a
    // compiler pass.
    let mut sink = Sink::new();
    sink.native(render_native(&src, &stmt, delta, &identity_lowering()));
    sink.finish()
}

/// **THE BUG.** A multi-line string literal in a handler body.
///
/// Source value:            `'\n<16 spaces>ALPHA\n<16 spaces>'`
/// Old compiler printed:    `'\n<8 spaces>ALPHA\n<8 spaces>'`   <- WRONG. Silently.
#[test]
fn reindenting_never_changes_the_value_of_a_string_literal() {
    let code = "s = \"\"\"\n                ALPHA\n                \"\"\"\nprint(s)";

    // Dedent by 8 — exactly the kind of shift that produced #215.
    let out = reindented(code, Target::Python3, -8);

    assert!(
        out.contains("\n                ALPHA\n"),
        "the 16 spaces INSIDE the literal must survive verbatim.\n\
         This is #215: the old compiler emitted 8 and the user's string silently \
         changed value.\n\ngot:\n{out}"
    );
}

/// And ordinary code around it IS re-indented — otherwise we have just broken
/// re-indentation instead of fixing it. (A "fix" that does nothing also passes the
/// test above.)
#[test]
fn ordinary_code_still_gets_reindented() {
    let code = "        x = 1\n        y = 2";
    let out = reindented(code, Target::Python3, -4);
    assert!(
        out.contains("\n    y = 2"),
        "ordinary lines MUST shift — a re-indenter that does nothing would pass the \
         literal test by accident.\n\ngot:\n{out:?}"
    );
}

/// Every multiline literal form, same rule. **Re-pinned on 4-target equivalents** (Item 4 /
/// R3: non-core targets are refused before `segment()`, so the JS-template and C++-raw
/// originals are unreachable in production; they live on against the hand oracle below
/// until C-final).
#[test]
fn every_multiline_literal_form_is_protected() {
    for (code, target, must_survive) in [
        // Rust raw string
        (
            "let s = r#\"\n        KEEP\n        \"#;",
            Target::Rust,
            "\n        KEEP\n",
        ),
        // Rust plain `"` string — multiline by the language's own grammar
        (
            "let s = \"\n        KEEP\n        \";",
            Target::Rust,
            "\n        KEEP\n",
        ),
        // Python `'''` triple (the `"""` form is #215's own test above)
        (
            "s = '''\n        KEEP\n        '''",
            Target::Python3,
            "\n        KEEP\n",
        ),
    ] {
        let out = reindented(code, target, -6);
        assert!(
            out.contains(must_survive),
            "{target:?}: the literal's interior must survive a dedent verbatim.\n\
             got:\n{out}"
        );
    }
}

/// A comment is a node too — which is why a `;` can never be spliced *inside* one.
#[test]
fn a_trailing_comment_is_a_node() {
    let code = "x = 1 // a } brace and a $.ref in a comment";
    let src = Source::new("t", code.as_bytes().to_vec()).unwrap();
    let bytes = code.as_bytes();
    let stmt = NativeStmt {
        span: Span::new(0, bytes.len()),
        parts: native_parts(bytes, 0, bytes.len(), Target::Rust),
        logical_indent: 0,
        block_depth: Some(0),
    };

    // The comment is a Literal node — so framec KNOWS it is there. The old compiler did not,
    // and spliced a `;` into the middle of one.
    let mut sink = Sink::new();
    sink.native(render_native(&src, &stmt, 0, &identity_lowering()));
    let out = sink.finish();
    assert_eq!(out, code, "verbatim");
    assert!(
        stmt.parts.len() >= 2,
        "the statement must decompose into code + comment, not one opaque blob"
    );
}
