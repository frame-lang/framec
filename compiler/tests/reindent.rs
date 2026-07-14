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
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::machine::body;
use frame_compiler::text::scan::lex::Lexer;
use frame_compiler::tree::body::Stmt;
use frame_compiler::{Source, Span};

/// Parse `code` as a handler body and re-indent every native statement by `delta`.
fn reindented(code: &str, target: Target, delta: i32) -> String {
    let src = Source::new("t", code.as_bytes().to_vec()).unwrap();
    // `body` needs the bytes; we are outside `crate::text`, so we cannot call
    // `Source::open`. We hand the lexer the same bytes we already own — which is the
    // point: the WALL means a test cannot reach into the compiler's buffer either.
    let bytes = code.as_bytes();
    let lx = Lexer::new(bytes, target);
    let b = body(&lx, bytes, Span::new(0, bytes.len()));

    // NOTE: this test cannot read the rendered text directly — it is outside
    // `crate::text`, so `NativeText::finish` is private to it. It must go through the
    // SINK, like every other consumer. The wall binds the tests too, and that is the
    // point: if a test could grep emitted text, so could a compiler pass.
    let mut sink = Sink::new();
    for st in &b.stmts {
        if let Stmt::Native(n) = st {
            sink.native(render_native(&src, n, delta, &identity));
        }
    }
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

/// Raw strings, template literals, C++ raw strings — every literal form, same rule.
#[test]
fn every_multiline_literal_form_is_protected() {
    for (code, target, must_survive) in [
        // Rust raw string
        (
            "let s = r#\"\n        KEEP\n        \"#;",
            Target::Rust,
            "\n        KEEP\n",
        ),
        // JS template literal
        (
            "const s = `\n        KEEP\n        `;",
            Target::JavaScript,
            "\n        KEEP\n",
        ),
        // C++ raw string
        (
            "auto s = R\"x(\n        KEEP\n        )x\";",
            Target::Cpp,
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
    let lx = Lexer::new(bytes, Target::Rust);
    let b = body(&lx, bytes, Span::new(0, bytes.len()));

    let n = b
        .stmts
        .iter()
        .find_map(|s| match s {
            Stmt::Native(n) => Some(n),
            _ => None,
        })
        .expect("a native statement");

    // The comment is a Literal node — so framec KNOWS it is there. The old compiler
    // did not, and spliced a `;` into the middle of one.
    let mut sink = Sink::new();
    sink.native(render_native(&src, n, 0, &identity));
    let out = sink.finish();
    assert_eq!(out, code, "verbatim");
    assert!(
        n.parts.len() >= 2,
        "the statement must decompose into code + comment, not one opaque blob"
    );
}
