//! **Milestone validation — the OpaqueScan capability, end-to-end through the REAL
//! pipeline (`segment`).** SCAFFOLDING (needs the internal `Source`/`segment` entry and
//! the tree spans; conversion-internal — never promoted).
//!
//! Item 1's capability is "opaque skip": while the production item walk
//! (`segmenter::item_starts`, a `@@[scan(u8)]` system) scans for top-level `@@`-at-start-
//! of-line, it must SKIP strings and comments via `skip_opaque_at` (= `OpaqueScan`) so a
//! `@@`/`}`/section-keyword *inside* a string or comment is never mistaken for an item or
//! a boundary. That string-blindness is the whole of #219/#214: the old compiler had a
//! string-*blind* byte loop and a `}`/`@@` inside a literal closed a body that was never
//! open or spawned a phantom item.
//!
//! These tests drive `segment(&Source, target)` on sources whose strings/comments contain
//! `@@system`, `{`, `}`, `machine:`, `#` — and assert the OBSERVABLE item/section outcome.
//! A regression in OpaqueScan (or in its wiring into the segmenter) fails a NAMED test
//! here. Each strong case also pins the position a string-*blind* walk would have produced,
//! so the assertion demonstrably distinguishes right from the specific historical wrong.

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::segment;
use frame_compiler::tree::{Item, Section};
use frame_compiler::Source;

fn seg(src: &str, target: Target) -> frame_compiler::tree::FileAst {
    let source = Source::new("milestone.frs", src.as_bytes().to_vec()).expect("utf8 source");
    segment(&source, target).expect("segment should succeed")
}

fn system_names(ast: &frame_compiler::tree::FileAst) -> Vec<String> {
    ast.items
        .iter()
        .filter_map(|it| match it {
            Item::System(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect()
}

fn the_only_system(ast: &frame_compiler::tree::FileAst) -> &frame_compiler::tree::SystemItem {
    let mut sys = None;
    for it in &ast.items {
        if let Item::System(s) = it {
            assert!(sys.is_none(), "expected exactly one system, found a second: {}", s.name);
            sys = Some(s);
        }
    }
    sys.expect("expected exactly one system item")
}

// ---------------------------------------------------------------------------
// A `@@system` at start-of-line INSIDE a multiline literal/comment is not an item.
// This is the direct OpaqueScan-in-the-segmenter proof: the walk must skip the whole
// opaque region, so the inner `@@system Fake` never becomes a start.
// ---------------------------------------------------------------------------

#[test]
fn pragma_start_inside_a_rust_block_comment_is_not_an_item() {
    // The block comment SPANS the inner `@@system Fake {` line. If OpaqueScan under-
    // recognized the comment (string-blind), item_starts would find `@@system Fake` at
    // the start of line 2 and emit a phantom System — the #214 shape.
    let src = "/* note\n\
               @@system Fake {\n\
               machine: }\n\
               */\n\
               @@system Real {\n\
               \x20   interface:\n\
               \x20       go()\n\
               \x20   machine:\n\
               \x20       $A { go() { } }\n\
               }";
    let ast = seg(src, Target::Rust);
    assert_eq!(system_names(&ast), vec!["Real".to_string()], "only Real is a real system");
    let real = the_only_system(&ast);
    // The system runs to the true final `}` (end of file), not truncated by the `}` on the
    // `machine: }` line inside the comment.
    assert_eq!(real.span.end, src.len(), "Real must span to the true closing brace");
}

#[test]
fn pragma_start_inside_a_rust_multiline_string_is_not_an_item() {
    // Rust `"` is multiline; the string carries `@@system Fake {` on its own line.
    let src = "let x = \"line1\n\
               @@system Fake { }\n\
               line3\";\n\
               @@system Real {\n\
               \x20   interface:\n\
               \x20       go()\n\
               \x20   machine:\n\
               \x20       $A { go() { } }\n\
               }";
    let ast = seg(src, Target::Rust);
    assert_eq!(system_names(&ast), vec!["Real".to_string()]);
    assert_eq!(the_only_system(&ast).span.end, src.len());
}

#[test]
fn pragma_start_inside_a_python_triple_string_is_not_an_item() {
    // Python `"""…"""` spans lines and carries `@@system Fake {` AND a `#` that is NOT a
    // comment (it is inside the string). OpaqueScan(python) must skip the whole triple.
    let src = "x = \"\"\"\n\
               @@system Fake {\n\
               # not a comment }\n\
               \"\"\"\n\
               @@system Real {\n\
               \x20   interface:\n\
               \x20       go()\n\
               \x20   machine:\n\
               \x20       $A { go() { } }\n\
               }";
    let ast = seg(src, Target::Python3);
    assert_eq!(system_names(&ast), vec!["Real".to_string()]);
    assert_eq!(the_only_system(&ast).span.end, src.len());
}

// ---------------------------------------------------------------------------
// A whole file that IS one `@@system` whose BODY holds a string/comment full of
// `}` / `@@` / `machine:`. The body-end + section walk must not be fooled — the item
// spans the entire file and its sections are found intact.
// ---------------------------------------------------------------------------

#[test]
fn a_brace_inside_a_string_in_the_body_does_not_close_early_rust() {
    // The handler string holds ONE `{` and TWO `}` (net -1). A string-blind body counter
    // would reach depth 0 one `}` EARLY — closing the system inside the string and leaving
    // trailing water. We pin that naive position and assert the real end is strictly past it.
    let src = "@@system S {\n\
               \x20   interface:\n\
               \x20       go()\n\
               \x20   machine:\n\
               \x20       $A {\n\
               \x20           go() {\n\
               \x20               let s = \"a } b @@system Fake { c }\";\n\
               \x20           }\n\
               \x20       }\n\
               }";
    let ast = seg(src, Target::Rust);
    assert_eq!(system_names(&ast), vec!["S".to_string()], "no phantom Fake system");
    let s = the_only_system(&ast);
    assert_eq!(s.span.end, src.len(), "S spans the whole file — not closed early inside the string");
    assert_eq!(ast.items.len(), 1, "no trailing water item from an early close");

    // Strength check: where a string-BLIND counter would have stopped (the first `}` byte
    // that returns brace depth to 0 while ignoring the string). Real end must be past it.
    let naive_early_close = src.find("} b").map(|p| p + 1).unwrap();
    assert!(
        s.span.end > naive_early_close,
        "the string-aware close ({}) must be past the string-blind close ({naive_early_close})",
        s.span.end
    );

    // The section walk found the real sections (not truncated by the string's braces).
    let has_machine = s.sections.iter().any(|sec| matches!(sec, Section::Machine(_)));
    let has_interface = s.sections.iter().any(|sec| matches!(sec, Section::Interface(_)));
    assert!(has_machine && has_interface, "interface + machine sections must survive");
}

#[test]
fn a_brace_inside_a_line_comment_in_the_body_does_not_close_early_c() {
    // A `//` line comment inside the body holds `}` and `@@system Fake {`. The item walk
    // and body close must skip the comment (OpaqueScan-class behaviour).
    let src = "@@system C {\n\
               \x20   interface:\n\
               \x20       go()\n\
               \x20   machine:\n\
               \x20       $A {\n\
               \x20           go() {\n\
               \x20               // } @@system Fake { oops }\n\
               \x20               int n = 1;\n\
               \x20           }\n\
               \x20       }\n\
               }";
    let ast = seg(src, Target::C);
    assert_eq!(system_names(&ast), vec!["C".to_string()]);
    let c = the_only_system(&ast);
    assert_eq!(c.span.end, src.len());
    assert_eq!(ast.items.len(), 1);
}

// ---------------------------------------------------------------------------
// A top-level string/comment that spells `@@system X { }` is water, not a system;
// but a REAL trailing `@@[pragma]` still lands. Proves the skip is precise, not greedy.
// ---------------------------------------------------------------------------

#[test]
fn buried_top_level_pragma_is_water_but_real_pragma_lands() {
    // Line 1's `@@system X {}` is INSIDE a comment; line 2's `@@[real]` is a genuine pragma.
    let src = "// @@system X { machine: }\n@@[real]\n";
    let ast = seg(src, Target::Rust);
    assert!(system_names(&ast).is_empty(), "the commented @@system is NOT a system");
    let pragmas: Vec<_> = ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Pragma(p) => p.attr.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(pragmas, vec!["real".to_string()], "exactly the real pragma lands");
}

#[test]
fn a_top_level_string_full_of_pragmas_is_a_single_native_item() {
    // Rust top-level water: a string literal containing `@@system` twice and stray braces.
    // OpaqueScan keeps the segmenter from spawning any item out of the string's contents.
    let src = "let a = \"@@system A {} and @@system B {}\";\n";
    let ast = seg(src, Target::Rust);
    assert!(system_names(&ast).is_empty(), "no systems come out of a string literal");
    assert!(
        ast.items.iter().all(|it| matches!(it, Item::Native(_))),
        "the whole file is native water"
    );
}
