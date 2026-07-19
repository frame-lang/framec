//! **The island dispatch, as a system, agrees with the hand walk — at the (kind, span)
//! level, bounded.**
//!
//! `native_parts_scan::parts` is generated from `native_parts_scan.frs`, the `@@[scan(u8)]`
//! system that IS production recognition behind `parts::native_parts` (Item 4). This battery
//! proves — by running — that its `(kind, start, end)` triples match the hand
//! `native_parts_hand` walk at every input: the dispatch order, the text runs between
//! islands, the island extents, the comment/literal kind SPLIT (0=Text 1=Literal 2=Ref
//! 3=Instantiate 4=EmbedCall 5=Comment), and the `[from, to)` bounds. The STRUCTURAL
//! (full-tree) differential lives in tests/native_parts.rs; this file is the walk-level
//! quotient.

use frame_compiler::text::scan::lex::Lexer;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::native_parts_scan;
use frame_compiler::text::scan::parts::native_parts_hand;
use frame_compiler::tree::body::NativePart;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// The hand walk's (kind, span) quotient. The hand tree merges comments and strings into
/// `NativePart::Literal`; the fabricated comment `delim: b'/'` (ledger T-N5) is — for the
/// four cleanroom targets, whose string delims are `"`/`'` — exactly the comment/literal
/// discriminant, so the hand side maps onto the walk's kind 5 without re-recognizing.
fn hand(src: &str, target: Target, from: usize, to: usize) -> Vec<(i32, usize, usize)> {
    let bytes = src.as_bytes();
    let lx = Lexer::new(bytes, target);
    native_parts_hand(&lx, bytes, from, to)
        .iter()
        .map(|p| match p {
            NativePart::Text(t) => (0, t.span.start, t.span.end),
            NativePart::Literal(l) if l.delim == b'/' => (5, l.span.start, l.span.end),
            NativePart::Literal(l) => (1, l.span.start, l.span.end),
            NativePart::Ref(r) => (2, r.span.start, r.span.end),
            NativePart::Instantiate(i) => (3, i.span.start, i.span.end),
            NativePart::EmbedCall(e) => (4, e.span.start, e.span.end),
        })
        .collect()
}

/// Full-buffer agreement, every target.
fn agree(src: &str, target: Target) {
    assert_eq!(
        native_parts_scan::parts(src.as_bytes(), 0, src.len(), target),
        hand(src, target, 0, src.len()),
        "disagreement on {src:?} ({target:?})"
    );
}

/// Bounded agreement — the walk's `(from, to)` seam vs the hand walk's, same window.
fn agree_bounded(src: &str, target: Target, from: usize, to: usize) {
    assert_eq!(
        native_parts_scan::parts(src.as_bytes(), from, to, target),
        hand(src, target, from, to),
        "disagreement on {src:?} [{from},{to}) ({target:?})"
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
fn comments_and_literals_are_split_kinds() {
    // The Item-4 completion: comment (5) and literal (1) are DIFFERENT kinds now, because
    // the driver's `to`-policies and node shapes differ (clamp vs demote; fabricated node).
    for t in TARGETS {
        agree("a = \"s\"; // tail comment\nb = 2", t);
    }
    let py = native_parts_scan::parts(b"x = 1 # note", 0, 12, Target::Python3);
    assert!(
        py.iter().any(|&(k, _, _)| k == 5),
        "a Python # comment must be kind 5, got {py:?}"
    );
    let rs = native_parts_scan::parts(b"y = \"s\"", 0, 7, Target::Rust);
    assert!(
        rs.iter().any(|&(k, _, _)| k == 1),
        "a string must stay kind 1, got {rs:?}"
    );
}

#[test]
fn islands_inside_strings_and_comments_are_not_islands() {
    // A `$.x` / `@@Sub()` inside a string or comment is CONTENT, part of the opaque node.
    agree(r#"s = "text $.x and @@Sub() here"; z = $.real"#, Target::Rust);
    agree("a = 1 // $.x @@Sub() comment\n b = $.real", Target::Rust);
    agree("a = 1 # $.x @@Sub() comment\nb = $.real", Target::Python3);
}

#[test]
fn edge_and_empty_agree() {
    agree("", Target::Rust);
    agree("$.only", Target::Rust);
    agree("@@Sub()", Target::Rust);
    agree("$.a$.b@@:self.c.d()", Target::Rust);
}

#[test]
fn bounded_windows_agree_with_the_hand_walk() {
    // The `to`-policy asymmetry at the WALK level (T-N3/T-N4): a comment straddling `to`
    // clamps (kind 5 to `to`); a literal straddling `to` demotes to water; plus windows
    // that start mid-island and the empty window.
    let src = "a = \"str\" /* block */ $.r";
    for t in [Target::C, Target::Rust, Target::Java] {
        for from in 0..=src.len() {
            agree_bounded(src, t, from, src.len());
        }
        for to in 0..=src.len() {
            agree_bounded(src, t, 0, to);
        }
    }
    let py = "s = 'q' # tail";
    for from in 0..=py.len() {
        for to in from..=py.len() {
            agree_bounded(py, Target::Python3, from, to);
        }
    }
}
