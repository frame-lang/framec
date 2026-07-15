//! **The section backbone, as a system, agrees with the hand `section_keyword_starts`.**
//!
//! `section_scan::keyword_starts` is generated from `section_scan.frs`, a `@@[scan(u8)]`
//! Frame system — the first grammar backbone. This proves — by running — that the section
//! keywords it finds match the production `section_keyword_starts` at every input, including
//! the string-blindness case (a `machine:` inside a string is NOT a section) and nested
//! braces (a keyword-looking token inside a handler body is not a top-level section).

use frame_compiler::text::scan::lex::Lexer;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::section_scan;
use frame_compiler::text::scan::sections::section_keyword_starts;
use frame_compiler::text::scan::segment;
use frame_compiler::tree::{Item, Node};
use frame_compiler::Source;

/// Segment `src`, find the (first) system's body bounds, and run both the hand helper and
/// the dogfooded system over them; assert they agree.
fn agree(src: &str) {
    let target = Target::Rust;
    let source = Source::new("t.frm", src.as_bytes().to_vec()).unwrap();
    let ast = segment(&source, target).unwrap();
    let sys = ast
        .items
        .iter()
        .find_map(|it| match it {
            Item::System(_) => Some(it.span()),
            _ => None,
        })
        .expect("a system");
    let bytes = src.as_bytes();
    let mut i = sys.start;
    while i < sys.end && bytes[i] != b'{' {
        i += 1;
    }
    let body_start = (i + 1).min(sys.end);
    let close_start = sys.end.saturating_sub(1);

    let lx = Lexer::new(bytes, target);
    let reference = section_keyword_starts(&lx, bytes, body_start, close_start);
    let machine = section_scan::keyword_starts(bytes, body_start, close_start, target);
    assert_eq!(machine, reference, "section disagreement on {src:?}");
}

#[test]
fn all_sections_agree() {
    agree("@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n    domain:\n        n: int = 0\n}\n");
    agree("@@system S {\n    actions:\n        a()\n    operations:\n        b()\n    machine:\n        $A { }\n}\n");
}

#[test]
fn a_keyword_inside_a_string_or_body_is_not_a_section() {
    // `machine:` inside a native string is DATA, not a section.
    agree("@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { let s = \"machine: not a section\"; } }\n}\n");
    // A keyword-looking token nested inside a handler body's braces is not top-level.
    agree("@@system S {\n    machine:\n        $A { e() { if x { domain: y } } }\n}\n");
}

#[test]
fn minimal_and_reordered_agree() {
    agree("@@system S {\n    machine:\n        $A { }\n}\n");
    agree("@@system S {\n    domain:\n        n: int = 0\n    interface:\n        go()\n    machine:\n        $A { }\n}\n");
}
