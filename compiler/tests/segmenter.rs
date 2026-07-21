//! **The item-level segmenter walk finds the known top-level `@@`-item starts — standalone.**
//!
//! `segmenter::item_starts` is generated from `segmenter.frs`, a `@@[scan(u8)]` Frame system
//! that now DRIVES production `segment`. This proves — by running — that the top-level
//! `@@`-item starts it finds match the KNOWN-CORRECT offsets (captured from the running system;
//! no hand oracle), including the string-blindness case (a `@@` inside a string/comment is not
//! an item) and system-body skipping (a `@@:self` inside a handler is not top-level).
//!
//! SCAFFOLDING (white-box on the internal `item_starts`), EXCEPT
//! `system_param_default_with_paren_in_string_is_string_aware`, which is a real-pipeline
//! milestone over `segment()`.

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::segmenter::item_starts;
use frame_compiler::text::scan::segment;
use frame_compiler::tree::{Item, MachineMember, Section};
use frame_compiler::Source;

/// Assert `item_starts` yields the KNOWN top-level `@@`-item offsets for `src`.
fn check(src: &str, target: Target, expected: &[usize]) {
    assert_eq!(
        item_starts(src.as_bytes(), 0, target),
        expected,
        "item starts on {src:?} ({target:?})"
    );
}

#[test]
fn plain_items() {
    // A leading `@@[async]` attribute (0) then the `@@system` (10).
    check(
        "@@[async]\n@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n",
        Target::Rust,
        &[0, 10],
    );
}

#[test]
fn a_pragma_inside_a_string_or_comment_is_not_an_item() {
    // The `@@system X {}` inside the string is NOT an item — only the `@@[main]` after it (25).
    check("let a = \"@@system X {}\";\n@@[main]\n", Target::Rust, &[25]);
    // Inside a `//` line comment — only the trailing `@@[main]` (17).
    check("// @@system X {}\n@@[main]\n", Target::Rust, &[17]);
    // Inside a `/* */` block comment — only the trailing `@@[real]` (14).
    check("/* @@a\n@@b */\n@@[real]\n", Target::Rust, &[14]);
}

#[test]
fn a_pragma_inside_a_system_body_is_not_top_level() {
    // Two top-level systems; the `@@:self.x` inside system A's handler is NOT top-level.
    let src = "@@system A {\n    interface:\n        e()\n    machine:\n        $S { e() { @@:self.x = 1; } }\n}\n@@system B {\n    interface:\n        f()\n    machine:\n        $S { f() { } }\n}\n";
    check(src, Target::Rust, &[0, 93]);
}

#[test]
fn target_specific_comments_are_skipped() {
    // Python `#` line comment hides the `@@system`; only the `@@[main]` (16) is an item.
    check("# @@system X {}\n@@[main]\n", Target::Python3, &[16]);
}

#[test]
fn leading_whitespace_before_a_pragma() {
    // Indented `@@[indented]` (4) then `@@[flush]` (17).
    check("    @@[indented]\n@@[flush]\n", Target::Rust, &[4, 17]);
}

// ============================================================================
// MILESTONE (real pipeline, self-contained): `read_name_params_brace` (scan/mod.rs)
// finds the matching `)` of a `@@system Name(params)` header via the string-AWARE
// `paren_balance::scan` @@system, not a naive `(`/`)` depth counter. A `)` inside a
// "…"-string in a param DEFAULT must be skipped, not counted, so the header closes at
// the TRUE `)`.
//
// A string-blind counter would hit the `)` inside `"oops)"`, decrement depth to 0, and
// close the paren-group MID-STRING — truncating the recovered default to `"oops` (missing
// the `)` and the closing quote). The discriminating assertion is the exact `default` text:
// intact `"oops)"` (string-aware) vs the truncated `"oops`. `segment()` returns Ok either
// way, so a weaker "does it error" check would NOT catch a regression — the DEFAULT VALUE
// is the teeth.
//
// Scope note: the fix covers "-strings only (paren_balance's `skip_string` is
// double-quote-based). A single-quoted `close: char = ')'` is NOT yet string-aware.
//
// SCAFFOLDING: conversion-internal — asserts on the internal `segment()`/tree API, not
// emitted-code behavior, so it is not promotable.
// ============================================================================
#[test]
fn system_param_default_with_paren_in_string_is_string_aware() {
    let text = "@@system S(msg: String = \"oops)\") {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n}\n";
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();

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
    // "-string INCLUDING the interior `)` and the closing quote. A string-blind counter would
    // have produced the truncated `"oops`.
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

    // The body was recovered intact past the true `)` — the header close landed correctly.
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
