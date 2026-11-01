//! Issue #135 — a nested `if` placed directly inside an `else { }` block must
//! lower like any other block-nested `if`.
//!
//! Scope note: Frame's brace-form control flow (`if c { … } else { … }`) is
//! lowered to native syntax only for **Erlang** (`case … of … end`, via
//! `codegen/output_block_parser_erlang.frs`). Every other target, Lua included,
//! takes target-native control flow and passes it through verbatim (#232: the
//! Lua brace transform was removed). The #135 regression therefore lives in
//! the Erlang brace-lowering path, and this test pins it there.

mod common;
use common::compile_source;

/// Lines of generated code that carry the branch payloads — the lowered
/// conditional, isolated from surrounding machinery (which legitimately uses
/// braces in some backends).
fn payload_lines(code: &str, payloads: &[&str]) -> String {
    code.lines()
        .filter(|l| payloads.iter().any(|p| l.contains(&format!("\"{p}\""))))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Erlang lowers brace-form conditionals to `case … of … end`. The multi-line
/// nested-if-in-else must produce nested `case`s with no leaked braces. (Erlang
/// brace-lowering is line-based, so it covers the multi-line authoring form.)
#[test]
fn nested_if_in_else_erlang() {
    let src = r#"
@@[target("erlang")]
@@system S {
    interface:
        classify(n)
    machine:
        $A {
            classify(n) {
                if n < 0 {
                    @@:("neg")
                } else {
                    if n == 0 {
                        @@:("zero")
                    } else {
                        @@:("pos")
                    }
                }
            }
        }
}
"#;
    let code = compile_source(src, "erlang");
    let region = payload_lines(&code, &["neg", "zero", "pos"]);
    assert!(
        !region.contains('{') && !region.contains('}'),
        "[erlang] stray Frame brace survived lowering:\n{region}"
    );
    for p in ["neg", "zero", "pos"] {
        assert!(
            region.contains(p),
            "[erlang] payload {p:?} missing:\n{region}"
        );
    }
    // Nested conditional → two `case … of` opened (outer + inner).
    assert_eq!(
        code.matches("case (").count(),
        2,
        "[erlang] expected two nested cases:\n{code}"
    );
}
