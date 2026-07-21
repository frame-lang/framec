//! **The instantiation recognizer, as a system, yields the known `(name, end)` — standalone.**
//!
//! `inst_scan::scan` is generated from `inst_scan.frs`, a `@@[scan(u8)]` Frame system that
//! composes the string-aware ParenBalance for the arg list. This proves — by running — that its
//! `(name, end)` matches the KNOWN-CORRECT values (captured from the running system; no hand
//! oracle) at EVERY position, including the reject cases (`@@:` context, `@@[` attr, `@@x`
//! no-parens, `@@ ()` no-name) and string args carrying `)`.
//!
//! SCAFFOLDING (white-box on the internal `inst_scan::scan`).

use frame_compiler::text::scan::inst_scan;

/// Assert `inst_scan::scan` recognizes an instantiation at exactly the listed positions (with the
/// pinned `(name, end)`), and REJECTS (`None`) at every other position of `src`.
fn check(src: &str, hits: &[(usize, &str, usize)]) {
    let b = src.as_bytes();
    for i in 0..b.len() {
        let got = inst_scan::scan(b, i);
        match hits.iter().find(|h| h.0 == i) {
            Some(&(_, name, end)) => assert_eq!(
                got,
                Some((name.to_string(), end)),
                "expected @@{name}() ending {end} at byte {i} of {src:?}"
            ),
            None => assert_eq!(got, None, "unexpected instantiation at byte {i} of {src:?}"),
        }
    }
}

#[test]
fn plain_instantiations() {
    check("let a = @@Sub();", &[(8, "Sub", 15)]);
    check(
        "x = @@Counter(10) + @@Robot(7, \"R2\");",
        &[(4, "Counter", 17), (20, "Robot", 36)],
    );
    check("@@!Unmanaged(1)", &[(0, "Unmanaged", 15)]); // the `!` variant
    check("@@A()@@B()", &[(0, "A", 5), (5, "B", 10)]); // adjacent
}

#[test]
fn nested_and_string_args() {
    // Nested instantiation in args: the outer `@@Wrap(...)` ends at 20, the inner `@@Inner()` at 16.
    check("@@Wrap(@@Inner(), 3)", &[(0, "Wrap", 20), (7, "Inner", 16)]);
    // A `)` inside a string arg must not close early — `@@S("a)b", 2)` ends at 13.
    check("@@S(\"a)b\", 2)", &[(0, "S", 13)]);
    // Deep paren nesting.
    check("@@S(f(g(h())))", &[(0, "S", 14)]);
}

#[test]
fn the_reject_cases() {
    check("@@:self.x", &[]); // context ref, not instantiation
    check("@@[async]", &[]); // attribute, not instantiation
    check("@@foo bar", &[]); // name with no parens
    check("@@ ()", &[]); // no name
    check("email @@ at, plain", &[]);
    check("", &[]);
}
