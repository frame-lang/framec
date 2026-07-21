//! **The embedded-call recognizer, as a system, yields the known tuple — standalone.**
//!
//! `embed_scan::scan` is generated from `embed_scan.frs`, a `@@[scan(u8)]` Frame system that
//! composes string-aware ParenBalance for the args. This proves — by running — that its
//! `(field, method, args, end)` matches the KNOWN-CORRECT values (captured from the running
//! system; no hand oracle) at EVERY position, including the reject cases (a field read
//! `@@:self.a.b` with no parens, `@@:self.x` with no method, `@@:self.x = 1` assignment,
//! `@@:data.k.m()` not-self).
//!
//! SCAFFOLDING (white-box on the internal `embed_scan::scan`).

use frame_compiler::text::scan::embed_scan;

/// Assert `embed_scan::scan` recognizes an embed call at exactly the listed positions (with the
/// pinned `(field, method, args, end)`), and REJECTS (`None`) at every other position of `src`.
fn check(src: &str, hits: &[(usize, &str, &str, &str, usize)]) {
    let b = src.as_bytes();
    for i in 0..b.len() {
        let got = embed_scan::scan(b, i);
        match hits.iter().find(|h| h.0 == i) {
            Some(&(_, field, method, args, end)) => assert_eq!(
                got,
                Some((field.to_string(), method.to_string(), args.to_string(), end)),
                "expected @@:self.{field}.{method}({args}) ending {end} at byte {i} of {src:?}"
            ),
            None => assert_eq!(got, None, "unexpected embed call at byte {i} of {src:?}"),
        }
    }
}

#[test]
fn plain_embed_calls() {
    check("@@:self.sensor.bump()", &[(0, "sensor", "bump", "", 21)]);
    check("x = @@:self.inner.ping() + 1;", &[(4, "inner", "ping", "", 24)]);
    check(
        "@@:self.a.f(1, 2)@@:self.b.g()",
        &[(0, "a", "f", "1, 2", 17), (17, "b", "g", "", 30)],
    );
}

#[test]
fn args_with_nesting_and_strings() {
    check("@@:self.x.m(g(h()))", &[(0, "x", "m", "g(h())", 19)]);
    // `)` inside a string arg must not close early.
    check("@@:self.x.m(\"a)b\", 2)", &[(0, "x", "m", "\"a)b\", 2", 21)]);
    // Nested embed call in args: outer `write(...)` ends at 37, inner `take()` at 36.
    check(
        "@@:self.log.write(@@:self.buf.take())",
        &[(0, "log", "write", "@@:self.buf.take()", 37), (18, "buf", "take", "", 36)],
    );
}

#[test]
fn the_reject_cases() {
    check("@@:self.a.b", &[]); // field read, no parens -> not a call
    check("@@:self.x", &[]); // no method segment
    check("@@:self.x = 1", &[]); // assignment, not a call
    check("@@:data.k.m()", &[]); // not self.
    check("plain words @@ here", &[]);
    check("", &[]);
}
