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
use frame_compiler::text::scan::segment;
use frame_compiler::tree::{Item, MachineMember, Section};
use frame_compiler::Source;

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

// ============================================================================
// FIX-WITH-TEETH: `read_name_params_brace` (scan/mod.rs) now finds the matching `)`
// of a `@@system Name(params)` header via the string-AWARE `paren_balance::scan`
// @@system, not a naive `(`/`)` depth counter. A `)` inside a "…"-string in a param
// DEFAULT must therefore be skipped, not counted, so the header closes at the TRUE `)`.
//
// The old naive counter would have hit the `)` inside `"oops)"`, decremented depth to 0,
// and closed the paren-group MID-STRING — truncating the recovered default to `"oops`
// (missing the `)` and the closing quote) while the real `)` after it was left dangling.
// The discriminating assertion is the exact `default` text: intact `"oops)"` (string-aware)
// vs the truncated `"oops` the string-blind counter would have produced. `segment()` returns
// Ok either way (the body brace is still found afterward), so a weaker "does it error" check
// would NOT catch a regression — the DEFAULT VALUE is the teeth.
//
// Scope note: the fix (and this test) covers "-strings only — StringScan / paren_balance's
// `skip_string` is double-quote-based. A single-quoted `close: char = ')'` is NOT yet
// string-aware (see coverage gap in the campaign notes), so this test deliberately uses the
// "-string form the conversion actually delivers.
//
// SCAFFOLDING: conversion-internal — asserts on the internal `segment()`/tree API
// (`SystemParams`, `Section`, `MachineMember`), not emitted-code behavior, so it is not
// promotable to the cross-language test-env.
#[test]
fn system_param_default_with_paren_in_string_is_string_aware() {
    // `msg`'s default carries a `)` inside a "…"-string. The header must close at the
    // final `)` after the closing quote, not at the one buried in the string.
    let text = "@@system S(msg: String = \"oops)\") {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n";
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();

    // Exactly one system item was recovered, and it is named `S` (the header parse did not
    // derail onto the mid-string `)`).
    let systems: Vec<_> = ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::System(sys) => Some(sys),
            _ => None,
        })
        .collect();
    assert_eq!(systems.len(), 1, "expected exactly one @@system, got {:#?}", ast.items);
    let sys = systems[0];
    assert_eq!(sys.name, "S", "system name mis-parsed");

    // THE TEETH: the sole domain param is `msg: String`, and its default is the INTACT
    // "-string INCLUDING the interior `)` and the closing quote. Under the retired
    // string-blind counter this would be the truncated `"oops`.
    assert_eq!(sys.params.domain.len(), 1, "domain params mis-split: {:#?}", sys.params);
    assert!(sys.params.state.is_empty(), "no $() state params expected");
    assert!(sys.params.enter.is_empty(), "no $>() enter params expected");
    let p = &sys.params.domain[0];
    assert_eq!(p.name, "msg", "param name mis-parsed");
    assert_eq!(p.ty.as_deref(), Some("String"), "param type mis-parsed");
    assert_eq!(
        p.default.as_deref(),
        Some("\"oops)\""),
        "param default not recovered intact — the `)` inside the \"…\"-string was miscounted \
         (string-blind), truncating the default; string-aware paren_balance must keep it whole"
    );

    // The body was recovered intact past the true `)` — proving the header close landed at
    // the right place, not mid-string. The machine section must contain state `$A`.
    let machine = sys
        .sections
        .iter()
        .find_map(|s| match s {
            Section::Machine(m) => Some(m),
            _ => None,
        })
        .expect("machine section not recovered — header parse derailed the body");
    let has_state_a = machine.members.iter().any(|m| match m {
        MachineMember::State(st) => st.name == "A",
        _ => false,
    });
    assert!(has_state_a, "state $A not recovered inside the machine section");
}

#[test]
fn leading_whitespace_before_a_pragma_agrees() {
    agree("    @@[indented]\n@@[flush]\n", Target::Rust);
}
