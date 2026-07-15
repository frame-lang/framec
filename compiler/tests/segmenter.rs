//! **The item-level segmenter walk, as a system, agrees with the hand walk.**
//!
//! `segmenter::item_starts` is generated from `segmenter.frs`, a `@@[scan(u8)]` Frame system
//! that now DRIVES production `segment`. This proves — by running — that the top-level
//! `@@`-item starts it finds match the hand walk (`hand_item_starts`, kept as the oracle) at
//! every input, including the string-blindness case (a `@@` inside a string/comment is not an
//! item) and system-body skipping (a `@@:self` inside a handler is not top-level).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::segmenter::item_starts;
use frame_compiler::text::scan::hand_item_starts;

fn agree(src: &str, target: Target) {
    let bytes = src.as_bytes();
    assert_eq!(
        item_starts(bytes, 0, target),
        hand_item_starts(bytes, 0, target),
        "segmenter disagreed on {src:?}"
    );
}

#[test]
fn plain_items_agree() {
    agree("@@[async]\n@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n", Target::Rust);
}

#[test]
fn a_pragma_inside_a_string_or_comment_is_not_an_item() {
    agree("let a = \"@@system X {}\";\n@@[main]\n", Target::Rust);
    agree("// @@system X {}\n@@[main]\n", Target::Rust);
    agree("/* @@a\n@@b */\n@@[real]\n", Target::Rust);
}

#[test]
fn a_pragma_inside_a_system_body_is_not_top_level() {
    let src = "@@system A {\n    interface:\n        e()\n    machine:\n        $S { e() { @@:self.x = 1; } }\n}\n@@system B {\n    interface:\n        f()\n    machine:\n        $S { f() { } }\n}\n";
    agree(src, Target::Rust);
}

#[test]
fn target_specific_comments_are_skipped() {
    agree("# @@system X {}\n@@[main]\n", Target::Python3);
}

#[test]
fn leading_whitespace_before_a_pragma_agrees() {
    agree("    @@[indented]\n@@[flush]\n", Target::Rust);
}
