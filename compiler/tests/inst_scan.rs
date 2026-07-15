//! **The instantiation recognizer, as a system, agrees with the hand `instantiation_at`.**
//!
//! `inst_scan::scan` is generated from `inst_scan.frs`, a `@@[scan(u8)]` Frame system that
//! composes the string-aware ParenBalance for the arg list. This proves — by running — that
//! its (name, end) matches the hand `instantiation_at` at EVERY position, including the
//! reject cases (`@@:` context, `@@[` attr, `@@x` no-parens) and string args carrying `)`.

use frame_compiler::text::scan::inst_scan;
use frame_compiler::text::scan::parts::instantiation_at_pub;

fn hand(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    instantiation_at_pub(bytes, i, bytes.len()).map(|inst| (inst.name, inst.span.end))
}

fn agree(src: &str) {
    let bytes = src.as_bytes();
    for i in 0..bytes.len() {
        assert_eq!(
            inst_scan::scan(bytes, i),
            hand(bytes, i),
            "disagreement at byte {i} of {src:?}"
        );
    }
}

#[test]
fn plain_instantiations_agree() {
    agree("let a = @@Sub();");
    agree("x = @@Counter(10) + @@Robot(7, \"R2\");");
    agree("@@!Unmanaged(1)");        // the `!` variant
    agree("@@A()@@B()");             // adjacent
}

#[test]
fn nested_and_string_args_agree() {
    agree("@@Wrap(@@Inner(), 3)");   // nested instantiation in args
    agree("@@S(\"a)b\", 2)");        // a `)` inside a string arg must not close early
    agree("@@S(f(g(h())))");         // deep paren nesting
}

#[test]
fn the_reject_cases_agree() {
    agree("@@:self.x");              // context ref, not instantiation
    agree("@@[async]");              // attribute, not instantiation
    agree("@@foo bar");              // name with no parens
    agree("@@ ()");                  // no name
    agree("email @@ at, plain");
    agree("");
}
