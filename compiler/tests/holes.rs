//! **Holes are code. Content is not.** Proven structurally.
//!
//! The old compiler gave TWO answers to this question depending on which code path
//! arrived (#224) — proven live, on the same three characters, in the same file:
//!
//! ```text
//! E1 = f"a {$.pv} b"     ->  emitted VERBATIM.  The scanner said: NOT a reference.
//! @@:(f"c {$.pv} d")     ->  expanded.          The byte-loop said: IS a reference.
//! ```
//!
//! That is not a formatting divergence. It is two answers to *"what is the language?"*
//!
//! Here the answer is not a rule someone must remember. It is the **shape of the
//! type**: a `FrameRef` can only be produced as a `NativePart`, or inside a `Hole`.
//! There is no variant that puts one inside `LiteralPart::Content`. The wrong answer
//! is unrepresentable.

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::parts::native_parts;
use frame_compiler::tree::body::{LiteralPart, NativePart, RefKind};
use frame_compiler::tree::{check_total, Node};

fn parts_of(code: &str, t: Target) -> Vec<NativePart> {
    let b = code.as_bytes();
    native_parts(b, 0, b.len(), t)
}

/// Collect every `FrameRef` anywhere in the parts, at any depth.
fn refs(parts: &[NativePart]) -> Vec<(RefKind, String)> {
    fn walk(parts: &[NativePart], src: &str, out: &mut Vec<(RefKind, String)>) {
        for p in parts {
            match p {
                NativePart::Ref(r) => {
                    out.push((r.kind, src[r.span.start..r.span.end].to_string()))
                }
                NativePart::Literal(l) => {
                    for lp in &l.parts {
                        if let LiteralPart::Hole(h) = lp {
                            walk(&h.parts, src, out);
                        }
                        // NOTE: there is no arm for Content. There cannot be — a
                        // `Content` node holds no parts and can hold no ref. That
                        // absence IS the guarantee.
                    }
                }
                NativePart::Text(_) => {}
                NativePart::Instantiate(_) => {}
                NativePart::EmbedCall(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(parts, "", &mut out);
    out
}

fn ref_texts(code: &str, t: Target) -> Vec<String> {
    let parts = parts_of(code, t);
    fn walk(parts: &[NativePart], src: &str, out: &mut Vec<String>) {
        for p in parts {
            match p {
                NativePart::Ref(r) => out.push(src[r.span.start..r.span.end].to_string()),
                NativePart::Literal(l) => {
                    for lp in &l.parts {
                        if let LiteralPart::Hole(h) = lp {
                            walk(&h.parts, src, out);
                        }
                    }
                }
                NativePart::Text(_) => {}
                NativePart::Instantiate(_) => {}
                NativePart::EmbedCall(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&parts, code, &mut out);
    out
}

#[test]
fn a_sigil_in_string_content_is_not_a_reference() {
    // The user wrote a literal `$.x` in their string. It is DATA. framec must not
    // touch it — rewriting it would silently corrupt the user's output.
    let found = ref_texts(r#"print("a literal $.x here")"#, Target::Python3);
    assert!(
        found.is_empty(),
        "a sigil in string CONTENT must not be a reference; found {found:?}"
    );
}

#[test]
fn a_sigil_in_an_interpolation_hole_IS_a_reference() {
    // The hole is an expression position in Python's own grammar. Those bytes are
    // code, and the Python compiler will treat them as code. framec may too.
    let found = ref_texts(r#"print(f"count is {$.count}")"#, Target::Python3);
    assert_eq!(found, vec!["$.count"], "a sigil in a HOLE is a reference");
}

#[test]
fn the_two_answers_the_old_compiler_gave_are_now_one() {
    // The exact source that produced the contradiction (#224).
    let a = ref_texts(r#"E1 = f"a {$.pv} b""#, Target::Python3);
    let b = ref_texts(r#"x = f"c {$.pv} d""#, Target::Python3);
    assert_eq!(a, b, "the same three characters must mean the same thing");
    assert_eq!(a, vec!["$.pv"]);
}

#[test]
fn multiple_refs_in_one_native_statement() {
    // The statement the first draft of the tree literally could not represent.
    let found = ref_texts(
        "let total = $.count + compute(@@:self.factor, 2) * 3;",
        Target::Rust,
    );
    assert_eq!(found, vec!["$.count", "@@:self.factor"]);
}

#[test]
fn holes_nest() {
    // Re-pinned on a 4-target equivalent (Item 4 / R3: non-core targets are refused before
    // `segment()`, so the TS-template original is unreachable in production). Depth 2 for
    // real: outer f-string hole → inner single-quoted literal → inner hole → ref.
    let found = ref_texts("s = f\"a { f'b {$.deep} c' } d\"", Target::Python3);
    assert_eq!(found, vec!["$.deep"], "a ref in a NESTED hole is still a ref");
}

#[test]
fn a_raw_string_has_no_holes_so_it_has_no_refs() {
    // Rust raw strings do not interpolate. `$.x` inside one is DATA, full stop.
    let found = ref_texts(r##"let s = r#"not a ref: $.x"#;"##, Target::Rust);
    assert!(found.is_empty(), "raw strings do not interpolate; found {found:?}");
}

#[test]
fn a_sigil_in_a_comment_is_not_a_reference() {
    // The comment that broke the C++ test twice: framec's own test grepped a COMMENT
    // and concluded the code was correct while the bug was live.
    let found = ref_texts("x = 1; // TODO: use $.count here\ny = 2;", Target::Rust);
    assert!(found.is_empty(), "a sigil in a COMMENT is not a reference; found {found:?}");
}

/// And the parts must be **total** — they partition the native span exactly, so no
/// byte of the user's code is dropped or double-counted.
#[test]
fn parts_partition_the_native_span() {
    // The TS/JS cases are re-pinned on 4-target equivalents (Item 4 / R3 — the non-core
    // targets are refused before `segment()`, so their originals are unreachable in production).
    for (code, target) in [
        (r#"print(f"count is {$.count}")"#, Target::Python3),
        ("let t = $.a + f(@@:self.b) * 3; // $.c\n", Target::Rust),
        ("s = f\"x { f'y {$.z}' } w\"", Target::Python3),
        (r#"s = "a } brace"; t = $.n;"#, Target::Java),
    ] {
        let parts = parts_of(code, target);
        let mut cursor = 0usize;
        for p in &parts {
            let s = (p as &dyn Node).span();
            assert_eq!(s.start, cursor, "gap/overlap in `{code}`");
            check_total(p as &dyn Node).expect("recursive totality");
            cursor = s.end;
        }
        assert_eq!(cursor, code.len(), "parts must cover `{code}` to the last byte");
    }
}

#[allow(dead_code)]
fn silence_unused(p: &[NativePart]) {
    let _ = refs(p);
}

#[test]
fn literals_inside_handler_bodies_are_nodes() {
    // A handler body containing a plain string. The census showed ZERO Literal nodes
    // across 265 fixtures, which cannot be right — so prove it end to end.
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{census, Node};
    use frame_compiler::Source;

    let text = "@@system S {\n    interface:\n        go()\n    machine:\n        $A {\n            go() {\n                print(\"opening\")\n            }\n        }\n}\n";
    let src = Source::new("t.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).unwrap();
    let mut c = std::collections::BTreeMap::new();
    census(&ast as &dyn Node, &mut c);
    eprintln!("  census: {c:?}");
    assert!(
        c.get("Literal").copied().unwrap_or(0) >= 1,
        "a string literal in a handler body MUST be a node — framec has to know where \
         it is in order to LEAVE IT ALONE (#215). Got: {c:?}"
    );
}
