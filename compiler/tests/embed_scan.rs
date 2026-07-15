//! **The embedded-call recognizer, as a system, agrees with the hand `embed_call_at`.**
//!
//! `embed_scan::scan` is generated from `embed_scan.frs`, a `@@[scan(u8)]` Frame system that
//! composes string-aware ParenBalance for the args. This proves — by running — that its
//! (field, method, args, end) matches `embed_call_at` at EVERY position, including the reject
//! cases (a field read `@@:self.a.b` with no parens, `@@:self.x` with no method).

use frame_compiler::text::scan::embed_scan;
use frame_compiler::text::scan::parts::embed_call_at_pub;

fn hand(bytes: &[u8], i: usize) -> Option<(String, String, String, usize)> {
    embed_call_at_pub(bytes, i, bytes.len())
        .map(|e| (e.field, e.method, e.args, e.span.end))
}

fn agree(src: &str) {
    let bytes = src.as_bytes();
    for i in 0..bytes.len() {
        assert_eq!(
            embed_scan::scan(bytes, i),
            hand(bytes, i),
            "disagreement at byte {i} of {src:?}"
        );
    }
}

#[test]
fn plain_embed_calls_agree() {
    agree("@@:self.sensor.bump()");
    agree("x = @@:self.inner.ping() + 1;");
    agree("@@:self.a.f(1, 2)@@:self.b.g()");
}

#[test]
fn args_with_nesting_and_strings_agree() {
    agree("@@:self.x.m(g(h()))");
    agree("@@:self.x.m(\"a)b\", 2)");   // `)` inside a string arg
    agree("@@:self.log.write(@@:self.buf.take())"); // nested embed call in args
}

#[test]
fn the_reject_cases_agree() {
    agree("@@:self.a.b");        // field read, no parens -> not a call
    agree("@@:self.x");          // no method segment
    agree("@@:self.x = 1");      // assignment, not a call
    agree("@@:data.k.m()");      // not self.
    agree("plain words @@ here");
    agree("");
}
