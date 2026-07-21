//! **The first dogfooded scanner recognizes the known quoted-string extents — standalone.**
//!
//! `string_scan::scan` is generated from `string_scan.frs`, a `@@[scan(u8)]` Frame system
//! (the resolution of the fubar in `docs/JOURNAL.md`). This test proves the generated machine
//! computes the correct quoted-string extent — for `delim = '"'`, single-line, escapes on — at
//! **every** `"`-position of a corpus, against KNOWN-CORRECT extents captured from the running
//! system (no hand oracle). It exercises the string-blindness cases (a `}` inside a string, an
//! escaped quote, a bare newline, an unterminated tail), which are the whole reason the mode
//! has to be a state, not a native `in_string` byte.
//!
//! The grammar mirrored is `Quoted { '"', multiline:false, escapes:true }` with no
//! interpolation. `scan(bytes, i)` returns `Some(end)` on a terminated string opening at `i`
//! (`end` is one past the closing quote), or `None` on "no string / unterminated".
//!
//! SCAFFOLDING (white-box: calls the internal `string_scan::scan` and asserts extents). Shaped
//! as a standalone extent spec, but NOT promotable while shipping lacks `@@[scan]`-on-`@@system`.

use frame_compiler::text::scan::string_scan;

/// Assert `string_scan::scan` yields the KNOWN extent at every `"`-position of `src`. `expected`
/// must list EVERY quote position (a completeness guard: it cannot silently omit a position), and
/// each entry pins the exact `Option<end>`.
fn check(src: &str, expected: &[(usize, Option<usize>)]) {
    let bytes = src.as_bytes();
    let quote_positions: Vec<usize> = (0..bytes.len()).filter(|&i| bytes[i] == b'"').collect();
    let listed: Vec<usize> = expected.iter().map(|&(i, _)| i).collect();
    assert_eq!(
        quote_positions, listed,
        "expected must cover EVERY `\"` position of {src:?} (completeness)"
    );
    for &(i, want) in expected {
        assert_eq!(
            string_scan::scan(bytes, i),
            want,
            "extent at byte {i} of {src:?}"
        );
    }
}

#[test]
fn plain_strings() {
    // Two adjacent strings; each `"` is either an opener (Some) or a trailing/closing quote that
    // opens a fresh (unterminated or re-paired) scan — the recognizer is position-agnostic.
    check(
        r#"let a = "hello"; let b = "world";"#,
        &[(8, Some(15)), (14, Some(26)), (25, Some(32)), (31, None)],
    );
}

#[test]
fn the_string_blindness_cases() {
    // A brace inside a string must NOT be seen as code — the extent runs past `}` to the real `"`.
    check(r#"x = "a } brace and a $.ref"; y = 1;"#, &[(4, Some(27)), (26, None)]);
    // An escaped quote does not close the string: the extent reaches the real closer at 22.
    check(
        r#"s = "he said \"hi\" ok"; t = 2;"#,
        &[(4, Some(23)), (14, Some(23)), (18, Some(23)), (22, None)],
    );
    // A backslash before the closing quote (escaped backslash then quote): `"back\\"` ends at 12.
    check(r#"p = "back\\"; q = 3;"#, &[(4, Some(12)), (11, None)]);
    // Empty string `""` — opener at 4, closer at 5, extent 6.
    check(r#"e = ""; f = 4;"#, &[(4, Some(6)), (5, None)]);
    // Adjacent strings `"ab""cd"`.
    check(r#""ab""cd""#, &[(0, Some(4)), (3, Some(5)), (4, Some(8)), (7, None)]);
}

#[test]
fn unterminated_and_newline() {
    // Unterminated tail — no extent.
    check("g = \"open and never closed", &[(4, None)]);
    // A bare newline terminates (single-line) — unterminated, so both `"` reject.
    check("h = \"line one\nstill\"", &[(4, None), (19, None)]);
    // A `"` that closes a prior string, then junk.
    check(r#"z = "one" + not_a_string"#, &[(4, Some(9)), (8, None)]);
}

#[test]
fn every_position_of_a_dense_corpus() {
    // A dense mix so the check hits many `"` offsets and interleavings.
    check(
        r#"a="x";b="y\"z";c="{}";d="";e="\\";f="tail"#,
        &[
            (2, Some(5)),
            (4, Some(9)),
            (8, Some(14)),
            (11, Some(14)),
            (13, Some(18)),
            (17, Some(21)),
            (20, Some(25)),
            (24, Some(26)),
            (25, Some(30)),
            (29, Some(33)),
            (32, Some(37)),
            (36, None),
        ],
    );
}
