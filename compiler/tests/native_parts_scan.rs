//! **The island dispatch, as a system, yields the known `(kind, span)` partition — standalone.**
//!
//! `native_parts_scan::parts` is generated from `native_parts_scan.frs`, the `@@[scan(u8)]`
//! system that IS production recognition behind `parts::native_parts` (Item 4). This battery
//! proves — by running — that its `(kind, start, end)` triples equal the KNOWN-CORRECT walk-level
//! quotient (captured from the running system; no hand oracle): the dispatch order, the text runs
//! between islands, the island extents, the comment/literal kind SPLIT
//! (0=Text 1=Literal 2=Ref 3=Instantiate 4=EmbedCall 5=Comment), and the `[from, to)` bounds. The
//! STRUCTURAL (full-tree) differential lives in tests/native_parts.rs; this file is the walk-level
//! quotient. Every asserted triple set is ALSO checked for partition well-formedness (kinds 0..=5,
//! no gap/overlap, covers the window), so a broken walk fails even where a triple set were mis-copied.
//!
//! SCAFFOLDING (white-box on the internal `native_parts_scan::parts`).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::native_parts_scan;

/// Assert the walk's `(kind, start, end)` triples over `[0, len)` equal `expected` AND form a
/// well-formed partition (kinds 0..=5, no gap/overlap, covering the whole buffer).
fn check(src: &str, target: Target, expected: &[(i32, usize, usize)]) {
    let b = src.as_bytes();
    let m = native_parts_scan::parts(b, 0, b.len(), target);
    assert_partition(&m, 0, b.len(), &format!("{src:?} ({target:?})"));
    assert_eq!(m, expected, "walk triples on {src:?} ({target:?})");
}

/// Partition well-formedness: contiguous, non-empty, valid kinds, covers `[from, to)`.
fn assert_partition(m: &[(i32, usize, usize)], from: usize, to: usize, ctx: &str) {
    let mut cursor = from;
    for &(k, s, e) in m {
        assert_eq!(s, cursor, "walk triples gap/overlap in {ctx}");
        assert!(s < e, "empty walk triple in {ctx}");
        assert!((0..=5).contains(&k), "invalid walk kind {k} in {ctx}");
        cursor = e;
    }
    assert_eq!(cursor, to, "walk triples must cover {ctx}");
}

#[test]
fn text_and_refs() {
    check(
        "total = $.count + $.n * 2",
        Target::Rust,
        &[(0, 0, 8), (2, 8, 15), (0, 15, 18), (2, 18, 21), (0, 21, 25)],
    );
    check(
        "f(@@:self.factor, @@:params.k)",
        Target::Rust,
        &[(0, 0, 2), (2, 2, 16), (0, 16, 18), (2, 18, 29), (0, 29, 30)],
    );
    check("plain text no islands here", Target::Rust, &[(0, 0, 26)]);
}

#[test]
fn all_island_kinds_interleaved() {
    // Text, Instantiate(3), EmbedCall(4).
    check(
        "let x = @@Counter(10); y = @@:self.buf.push($.v);",
        Target::Rust,
        &[(0, 0, 8), (3, 8, 21), (0, 21, 27), (4, 27, 48), (0, 48, 49)],
    );
    // Ref(2), Instantiate(3), EmbedCall(4), and a trailing Literal(1) string.
    check(
        "$.a + @@Sub() - @@:self.o.m(1) / \"str\"",
        Target::Rust,
        &[(2, 0, 3), (0, 3, 6), (3, 6, 13), (0, 13, 16), (4, 16, 30), (0, 30, 33), (1, 33, 38)],
    );
}

#[test]
fn comments_and_literals_are_split_kinds() {
    // The Item-4 completion: comment (5) and literal (1) are DIFFERENT kinds now, because the
    // driver's `to`-policies and node shapes differ (clamp vs demote; fabricated node). The
    // `//` comment (kind 5) exists only on C/Java/Rust; on Python it is ordinary text absorbed
    // into the trailing run.
    for t in [Target::C, Target::Java, Target::Rust] {
        check(
            "a = \"s\"; // tail comment\nb = 2",
            t,
            &[(0, 0, 4), (1, 4, 7), (0, 7, 9), (5, 9, 24), (0, 24, 30)],
        );
    }
    check(
        "a = \"s\"; // tail comment\nb = 2",
        Target::Python3,
        &[(0, 0, 4), (1, 4, 7), (0, 7, 30)],
    );
    // A Python `#` comment is kind 5.
    check("x = 1 # note", Target::Python3, &[(0, 0, 6), (5, 6, 12)]);
    // A string stays kind 1.
    check("y = \"s\"", Target::Rust, &[(0, 0, 4), (1, 4, 7)]);
}

#[test]
fn islands_inside_strings_and_comments_are_not_islands() {
    // A `$.x` / `@@Sub()` inside a string or comment is CONTENT — absorbed into the opaque node
    // (Literal 1 / Comment 5); only the `$.real` OUTSIDE is a Ref (2).
    check(
        r#"s = "text $.x and @@Sub() here"; z = $.real"#,
        Target::Rust,
        &[(0, 0, 4), (1, 4, 31), (0, 31, 37), (2, 37, 43)],
    );
    check(
        "a = 1 // $.x @@Sub() comment\n b = $.real",
        Target::Rust,
        &[(0, 0, 6), (5, 6, 28), (0, 28, 34), (2, 34, 40)],
    );
    check(
        "a = 1 # $.x @@Sub() comment\nb = $.real",
        Target::Python3,
        &[(0, 0, 6), (5, 6, 27), (0, 27, 32), (2, 32, 38)],
    );
}

#[test]
fn edge_and_empty() {
    check("", Target::Rust, &[]);
    check("$.only", Target::Rust, &[(2, 0, 6)]);
    check("@@Sub()", Target::Rust, &[(3, 0, 7)]);
    check("$.a$.b@@:self.c.d()", Target::Rust, &[(2, 0, 3), (2, 3, 6), (4, 6, 19)]);
}

#[test]
fn bounded_windows_partition_and_text_run_interior() {
    // Every window of this input must yield a well-formed partition (kinds 0..=5, covering
    // [from, to)); PLUS the directed Δ3 fact, self-contained: a window that STARTS mid-string
    // exposes the closing `"` as an unterminated opener, so the whole interior is ONE Text run.
    let src = "a = \"str\" /* block */ $.r";
    let b = src.as_bytes();
    for t in [Target::C, Target::Rust, Target::Java] {
        for from in 0..=b.len() {
            assert_partition(
                &native_parts_scan::parts(b, from, b.len(), t),
                from,
                b.len(),
                &format!("{src:?} [{from},{}) ({t:?})", b.len()),
            );
        }
        for to in 0..=b.len() {
            assert_partition(
                &native_parts_scan::parts(b, 0, to, t),
                0,
                to,
                &format!("{src:?} [0,{to}) ({t:?})"),
            );
        }
    }
    // Full window: Text, Literal(string), Text, Comment, Text, Ref.
    check(
        src,
        Target::Rust,
        &[(0, 0, 4), (1, 4, 9), (0, 9, 10), (5, 10, 21), (0, 21, 22), (2, 22, 25)],
    );
    // From 5 (inside the string): the interior text-runs to the window end — ONE Text part.
    assert_eq!(
        native_parts_scan::parts(b, 5, b.len(), Target::Rust),
        &[(0, 5, 25)],
        "a window starting mid-string must text-run its interior (Δ3)"
    );
}
