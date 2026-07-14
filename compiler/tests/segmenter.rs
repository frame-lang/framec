//! **The load-bearing segmenter walk, as a system, agrees with the hand `segment`.**
//!
//! `segmenter::item_starts` is generated from `segmenter.frs`, a `@@[scan(u8)]` Frame system
//! that owns the item-level walk. This proves — by running — that it finds the SAME
//! top-level `@@`-item start offsets as the hand `segment`, including the string-blindness
//! cases: a `@@` inside a string or a comment is NOT an item, and a `@@:self` inside a
//! `@@system` body is NOT a top-level item (the body is skipped).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::segmenter::item_starts;
use frame_compiler::text::scan::segment;
use frame_compiler::tree::Item;
use frame_compiler::Source;

/// The top-level `@@`-item start offsets the hand `segment` finds (Pragma / System / Efsm).
fn hand(src: &str, target: Target) -> Vec<usize> {
    use frame_compiler::tree::Node;
    let source = Source::new("t.frm", src.as_bytes().to_vec()).unwrap();
    let ast = segment(&source, target).unwrap();
    ast.items
        .iter()
        .filter(|it| matches!(it, Item::Pragma(_) | Item::System(_) | Item::Efsm(_)))
        .map(|it| it.span().start)
        .collect()
}

fn agree(src: &str, target: Target) {
    let machine = item_starts(src.as_bytes(), target);
    let reference = hand(src, target);
    assert_eq!(machine, reference, "segmenter disagreed on {src:?}");
}

#[test]
fn plain_items_agree() {
    agree("@@[async]\n@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n", Target::Rust);
}

#[test]
fn a_pragma_inside_a_string_or_comment_is_not_an_item() {
    // The `@@` here is DATA, not an item — the walk skips the string / comment.
    agree("let a = \"@@system X {}\";\n@@[main]\n", Target::Rust);
    agree("// @@system X {}\n@@[main]\n", Target::Rust);
    agree("/* @@a\n@@b */\n@@[real]\n", Target::Rust);
}

#[test]
fn a_pragma_inside_a_system_body_is_not_top_level() {
    // `@@:self.x` inside the handler must NOT be picked up — the whole @@system body is
    // skipped, and the next top-level item is the second system.
    let src = "@@system A {\n    interface:\n        e()\n    machine:\n        $S { e() { @@:self.x = 1; } }\n}\n@@system B {\n    interface:\n        f()\n    machine:\n        $S { f() { } }\n}\n";
    agree(src, Target::Rust);
}

#[test]
fn target_specific_comments_are_skipped() {
    // Python `#` comment carrying a `@@` — a Rust-only walk would miss it; the config target
    // makes the walk skip the right form.
    agree("# @@system X {}\n@@[main]\n", Target::Python3);
}

#[test]
fn leading_whitespace_before_a_pragma_agrees() {
    agree("    @@[indented]\n@@[flush]\n", Target::Rust);
}
