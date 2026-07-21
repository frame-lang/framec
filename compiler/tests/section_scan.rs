//! **The section backbone, as a system, finds the known section-keyword starts — standalone.**
//!
//! `section_scan::keyword_starts` is generated from `section_scan.frs`, a `@@[scan(u8)]`
//! Frame system — the first grammar backbone. This proves — by running — that the section
//! keywords it finds at brace depth 0 match the KNOWN-CORRECT `(kw_start, kw_end, idx)` triples
//! (captured from the running system; no hand oracle), including the string-blindness case (a
//! `machine:` inside a string is NOT a section) and nested braces (a keyword-looking token inside
//! a handler body is not a top-level section).
//!
//! The `(body_start, close_start)` window is computed exactly as production does (via
//! `segment()`), so the pinned triples are the real backbone output over the real system bounds.
//!
//! SCAFFOLDING (white-box on the internal `keyword_starts` over the real `segment()` window).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::section_scan;
use frame_compiler::text::scan::segment;
use frame_compiler::tree::{Item, Node};
use frame_compiler::Source;

/// Segment `src`, find the (first) system's body bounds exactly as production does, run the
/// dogfooded backbone over them, and assert its `(kw_start, kw_end, section_idx)` triples equal
/// the KNOWN-CORRECT `expected`. (`section_idx`: 0=interface 1=machine 2=domain 3=actions
/// 4=operations, the `KEYWORDS` order in `section_scan`.)
fn check(src: &str, expected: &[(usize, usize, usize)]) {
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

    let starts = section_scan::keyword_starts(bytes, body_start, close_start, target);
    assert_eq!(starts, expected, "section starts on {src:?}");
}

#[test]
fn all_sections() {
    check(
        "@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { } }\n    domain:\n        n: int = 0\n}\n",
        &[(17, 27, 0), (45, 53, 1), (82, 89, 2)],
    );
    check(
        "@@system S {\n    actions:\n        a()\n    operations:\n        b()\n    machine:\n        $A { }\n}\n",
        &[(17, 25, 3), (42, 53, 4), (70, 78, 1)],
    );
}

#[test]
fn a_keyword_inside_a_string_or_body_is_not_a_section() {
    // `machine:` inside a native string is DATA, not a section — only interface (0) + machine (1)
    // are found; the string's `machine:` does NOT add a third.
    check(
        "@@system S {\n    interface:\n        go()\n    machine:\n        $A { go() { let s = \"machine: not a section\"; } }\n}\n",
        &[(17, 27, 0), (45, 53, 1)],
    );
    // A keyword-looking token nested inside a handler body's braces is not top-level — only the
    // top-level `machine:` (17) is a section, NOT the `domain:` inside the `if` block.
    check(
        "@@system S {\n    machine:\n        $A { e() { if x { domain: y } } }\n}\n",
        &[(17, 25, 1)],
    );
}

#[test]
fn minimal_and_reordered() {
    check("@@system S {\n    machine:\n        $A { }\n}\n", &[(17, 25, 1)]);
    check(
        "@@system S {\n    domain:\n        n: int = 0\n    interface:\n        go()\n    machine:\n        $A { }\n}\n",
        &[(17, 24, 2), (48, 58, 0), (76, 84, 1)],
    );
}
