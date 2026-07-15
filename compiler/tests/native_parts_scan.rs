//! **The island dispatch, as a system, agrees with the hand `native_parts`.**
//!
//! `native_parts_scan::parts` is generated from `native_parts_scan.frs`, a `@@[scan(u8)]`
//! Frame system that composes InstScan / EmbedScan / RefScan (and the lexer). This proves —
//! by running — that its (kind, start, end) part sequence matches the hand `native_parts` at
//! every input: the dispatch order, the text runs between islands, and the island extents.

use frame_compiler::text::scan::lex::Lexer;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::native_parts_scan;
use frame_compiler::text::scan::parts::native_parts;
use frame_compiler::tree::body::NativePart;

fn hand(src: &str, target: Target) -> Vec<(i32, usize, usize)> {
    let bytes = src.as_bytes();
    let lx = Lexer::new(bytes, target);
    native_parts(&lx, bytes, 0, bytes.len())
        .iter()
        .map(|p| match p {
            NativePart::Text(t) => (0, t.span.start, t.span.end),
            NativePart::Literal(l) => (1, l.span.start, l.span.end),
            NativePart::Ref(r) => (2, r.span.start, r.span.end),
            NativePart::Instantiate(i) => (3, i.span.start, i.span.end),
            NativePart::EmbedCall(e) => (4, e.span.start, e.span.end),
        })
        .collect()
}

fn agree(src: &str, target: Target) {
    assert_eq!(
        native_parts_scan::parts(src.as_bytes(), target),
        hand(src, target),
        "disagreement on {src:?}"
    );
}

#[test]
fn text_and_refs_agree() {
    agree("total = $.count + $.n * 2", Target::Rust);
    agree("f(@@:self.factor, @@:params.k)", Target::Rust);
    agree("plain text no islands here", Target::Rust);
}

#[test]
fn all_island_kinds_interleaved_agree() {
    agree("let x = @@Counter(10); y = @@:self.buf.push($.v);", Target::Rust);
    agree("$.a + @@Sub() - @@:self.o.m(1) / \"str\"", Target::Rust);
}

#[test]
fn islands_inside_strings_and_comments_are_not_islands() {
    // A `$.x` / `@@Sub()` inside a string or comment is CONTENT, part of the Literal.
    agree(r#"s = "text $.x and @@Sub() here"; z = $.real"#, Target::Rust);
    agree("a = 1 // $.x @@Sub() comment\n b = $.real", Target::Rust);
}

#[test]
fn edge_and_empty_agree() {
    agree("", Target::Rust);
    agree("$.only", Target::Rust);
    agree("@@Sub()", Target::Rust);
    agree("$.a$.b@@:self.c.d()", Target::Rust);
}
