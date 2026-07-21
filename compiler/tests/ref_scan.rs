//! **The Frame-reference recognizer, as a system, yields the known refs — standalone.**
//!
//! `ref_scan::scan` is generated from `ref_scan.frs`, a `@@[scan(u8)]` Frame system. This proves
//! — by running — that it yields the KNOWN-CORRECT `(kind, name, end)` (captured from the running
//! system; no hand oracle) at EVERY position of a corpus, across all the context kinds and the
//! reject cases. As of Item 4 Commit C the system holds the LAST production seat (the statement
//! scanner's assign-LHS).
//!
//! It also pins the Δ5 (T-R1/T-R2) behavior directly: an UNKNOWN context word segment-matches to
//! `RefKind::Unknown` carrying the WHOLE word (refusal as data), and a prefix like `database`
//! (segment `database` ≠ `data`) is Unknown, not `ContextData` — self-contained, no oracle.
//!
//! SCAFFOLDING (white-box on the internal `ref_scan::scan`).

use frame_compiler::text::scan::ref_scan;
use frame_compiler::tree::body::RefKind;

/// Assert `ref_scan::scan` recognizes a ref at exactly the listed positions (with the pinned
/// `(kind, name, end)`), and REJECTS (`None`) at every other position of `src`.
fn check(src: &str, hits: &[(usize, RefKind, &str, usize)]) {
    let b = src.as_bytes();
    for i in 0..b.len() {
        let got = ref_scan::scan(b, i);
        match hits.iter().find(|h| h.0 == i) {
            Some(&(_, kind, name, end)) => assert_eq!(
                got,
                Some((kind, name.to_string(), end)),
                "expected {kind:?} `{name}` ending {end} at byte {i} of {src:?}"
            ),
            None => assert_eq!(got, None, "unexpected ref at byte {i} of {src:?}"),
        }
    }
}

#[test]
fn state_vars() {
    check(
        "x = $.count + $.n; y = $.total_amount;",
        &[
            (4, RefKind::StateVar, "count", 11),
            (14, RefKind::StateVar, "n", 17),
            (23, RefKind::StateVar, "total_amount", 37),
        ],
    );
    check(
        "$.a $.b $.c",
        &[
            (0, RefKind::StateVar, "a", 3),
            (4, RefKind::StateVar, "b", 7),
            (8, RefKind::StateVar, "c", 11),
        ],
    );
}

#[test]
fn every_context_kind() {
    check("@@:self.factor", &[(0, RefKind::ContextSelf, "factor", 14)]);
    check("@@:data.k", &[(0, RefKind::ContextData, "k", 9)]);
    check("@@:params.arg", &[(0, RefKind::ContextParams, "arg", 13)]);
    check("@@:return", &[(0, RefKind::ContextReturn, "return", 9)]);
    check("@@:event", &[(0, RefKind::ContextEvent, "event", 8)]);
    check("@@:system", &[(0, RefKind::ContextSystemState, "system", 9)]);
    check(
        "a = @@:self.x + @@:params.y - @@:data.z;",
        &[
            (4, RefKind::ContextSelf, "x", 13),
            (16, RefKind::ContextParams, "y", 27),
            (30, RefKind::ContextData, "z", 39),
        ],
    );
}

#[test]
fn the_reject_cases() {
    // A bare `$` or `@@` without the ref shape, and identifiers that merely start like one.
    check("$ alone, $x no-dot, @@ pair, @@system decl, plain words", &[]);
    check("email@@host $money", &[]);
    check("", &[]);
}

#[test]
fn refs_packed_against_neighbours() {
    check(
        "f($.a,@@:self.b)+$.c",
        &[
            (2, RefKind::StateVar, "a", 5),
            (6, RefKind::ContextSelf, "b", 15),
            (17, RefKind::StateVar, "c", 20),
        ],
    );
    check("@@:self.a.b.c", &[(0, RefKind::ContextSelf, "a.b.c", 13)]); // dotted context path
    check(
        "$.x$.y",
        &[(0, RefKind::StateVar, "x", 3), (3, RefKind::StateVar, "y", 6)],
    ); // adjacent state vars
}

/// Δ5 (T-R1/T-R2), self-contained: the SYSTEM segment-matches and REFUSES an unknown context word
/// (`RefKind::Unknown`, the WHOLE word as the name), and a prefix-lookalike is refused too — it
/// does NOT `starts_with`-prefix-match `data`/`self`/`params`. Facts, not comparisons.
#[test]
fn delta5_unknown_and_prefix_are_refused() {
    for (src, word) in [
        ("@@:wat.x", "wat.x"),       // T-R1: unknown context → Unknown (whole word)
        ("@@:database.k", "database.k"), // T-R2: `database` ≠ `data` → Unknown
        ("@@:selfish.y", "selfish.y"),   // T-R2: `selfish` ≠ `self` → Unknown
        ("@@:paramsX.z", "paramsX.z"),   // T-R2: `paramsX` ≠ `params` → Unknown
    ] {
        let b = src.as_bytes();
        let (k, n, _) = ref_scan::scan(b, 0).expect("system recognizes the shape");
        assert_eq!(k, RefKind::Unknown, "system must refuse on {src:?}");
        assert_eq!(n, word, "the WHOLE word is the name (refusal as data) on {src:?}");
    }
}
