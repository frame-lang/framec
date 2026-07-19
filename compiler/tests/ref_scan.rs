//! **The Frame-reference recognizer, as a system, agrees with the hand `frame_ref_at_hand`.**
//!
//! `ref_scan::scan` is generated from `ref_scan.frs`, a `@@[scan(u8)]` Frame system. This
//! proves — by running — that it yields the SAME (kind, name, end) as the hand recognizer
//! at EVERY position of a corpus, across all the context kinds and the reject cases. As of
//! Item 4 Commit C the system holds the LAST production seat (the statement scanner's
//! assign-LHS); the hand fn is oracle-only, deleted at C-final.

use frame_compiler::text::scan::parts::frame_ref_at_hand;
use frame_compiler::text::scan::ref_scan;

fn hand(bytes: &[u8], i: usize) -> Option<(String, String, usize)> {
    frame_ref_at_hand(bytes, i, bytes.len())
        .map(|r| (format!("{:?}", r.kind), r.name, r.span.end))
}

fn machine(bytes: &[u8], i: usize) -> Option<(String, String, usize)> {
    ref_scan::scan(bytes, i).map(|(k, n, e)| (format!("{k:?}"), n, e))
}

fn agree(src: &str) {
    let bytes = src.as_bytes();
    for i in 0..bytes.len() {
        assert_eq!(
            machine(bytes, i),
            hand(bytes, i),
            "disagreement at byte {i} of {src:?}"
        );
    }
}

#[test]
fn state_vars_agree() {
    agree("x = $.count + $.n; y = $.total_amount;");
    agree("$.a $.b $.c");
}

#[test]
fn every_context_kind_agrees() {
    agree("@@:self.factor");
    agree("@@:data.k");
    agree("@@:params.arg");
    agree("@@:return");
    agree("@@:event");
    agree("@@:system");
    agree("a = @@:self.x + @@:params.y - @@:data.z;");
}

#[test]
fn the_reject_cases_agree() {
    // A bare `$` or `@@` without the ref shape, and identifiers that merely start like one.
    agree("$ alone, $x no-dot, @@ pair, @@system decl, plain words");
    agree("email@@host $money");
    agree("");
}

#[test]
fn refs_packed_against_neighbours_agree() {
    agree("f($.a,@@:self.b)+$.c");
    agree("@@:self.a.b.c");   // dotted context path
    agree("$.x$.y");          // adjacent state vars
}
