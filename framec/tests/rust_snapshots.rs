//! RFC-0027 in-tree snapshot tests — rust backend.
//!
//! Mirrors python_snapshots.rs against the rust target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::{compile_fixture, compile_source};

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "rust"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "rust"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "rust"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "rust"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "rust"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "rust"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "rust"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "rust"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "rust"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "rust"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "rust"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "rust"));
}

// RFC-0025.1 prevention fixture: a COMPOUND-typed (`list`) enter arg
// delivered via `-> (items) $Loaded`. This is the exact shape #34 hid
// behind — the matrix/snapshot corpus only had primitive lifecycle args
// (and `08_lifecycle`'s `(label) ->` is a no-op, never delivering one),
// so the Rust stringify (`Vec<String>` + `parse`) and the compound
// hard-break (`Vec<i64>: FromStr`) went uncaught. Frozen across all 17
// backends so any future erasure/stringify regression surfaces in the diff.
#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "rust"));
}

// ─────────────────────────────────────────────────────────────────────
// RFC-0033 regression tests — borrowed-type promotion, lint preamble,
// expression-form state-var initializers. Inline source so the
// Rust-specific syntax doesn't break the cross-backend fixture corpus.
// ─────────────────────────────────────────────────────────────────────

/// RFC-0033 #19: every framec-emitted Rust system is wrapped in a
/// private `mod _<name>_framec { ... }` with OUTER lint-suppression
/// attributes plus a `pub use _<name>_framec::*;` re-export. The
/// wrapping is required for `include!()` build-script consumers
/// (rustc rejects inner attributes from macro expansion); the
/// re-export keeps the public API identical to the unwrapped form.
///
/// Asserts the specific outer allows are present and that the
/// blanket `clippy::all` / `pedantic` / `nursery` suppressions are
/// deliberately ABSENT (so new clippy findings surface to users).
/// Also asserts that NO inner-attribute preamble survives — those
/// break `include!()`.
#[test]
fn rfc0033_lint_wrapper_outer_attrs_and_mod() {
    let src = r#"
@@system Foo {
    interface:
        bar()
    machine:
        $A {
            bar() { }
        }
}
"#;
    let out = compile_source(src, "rust");

    // Required OUTER allows (rustc-level — inherent to codegen shape):
    for needed in [
        "#[allow(dead_code)]",
        "#[allow(non_camel_case_types)]",
        "#[allow(non_snake_case)]",
        "#[allow(unused_variables)]",
        "#[allow(unused_mut)]",
        "#[allow(unused_imports)]",
    ] {
        assert!(
            out.contains(needed),
            "wrapper missing required rustc allow: `{}`\n--- output ---\n{}",
            needed,
            &out[..out.len().min(400)]
        );
    }

    // Required OUTER allows (specific clippy lints framec patterns
    // trigger). The set was audited via `cargo clippy -D warnings`
    // against the canonical fixture corpus plus a transition-
    // containing handler that exercises the `return;`-after-
    // `__transition` codepath. If a future codegen change triggers
    // a new clippy lint, the user will see it in their build and
    // we'll add it here (deliberately not a blanket
    // clippy::all/pedantic/nursery).
    for needed in [
        "#[allow(clippy::assign_op_pattern)]",
        "#[allow(clippy::clone_on_copy)]",
        "#[allow(clippy::derivable_impls)]",
        "#[allow(clippy::match_single_binding)]",
        "#[allow(clippy::needless_return)]",
        "#[allow(clippy::new_without_default)]",
        "#[allow(clippy::single_match)]",
    ] {
        assert!(
            out.contains(needed),
            "wrapper missing required clippy allow: `{}`",
            needed
        );
    }

    // The wrapping mod + re-export.
    assert!(
        out.contains("mod _foo_framec {"),
        "expected `mod _foo_framec {{` wrapper:\n--- output (head) ---\n{}",
        &out[..out.len().min(400)]
    );
    assert!(
        out.contains("pub use _foo_framec::*;"),
        "expected `pub use _foo_framec::*;` re-export"
    );

    // Inner attributes are FORBIDDEN — they break `include!()`. None
    // of these forms should appear anywhere in the output.
    for forbidden in [
        "#![allow(dead_code)]",
        "#![allow(clippy::all)]",
        "#![allow(clippy::pedantic)]",
        "#![allow(clippy::nursery)]",
    ] {
        assert!(
            !out.contains(forbidden),
            "output unexpectedly contains inner-attribute form `{}` — breaks include!()",
            forbidden
        );
    }
}

/// RFC-0033 #18: `&str` in an interface parameter MUST auto-promote.
/// User-facing system method keeps `&str`; event variant holds
/// `String`; dispatch site calls `.to_string()`; handler binds back
/// as `&str` via `.as_str()`. Zero lifetime parameters on generated
/// types.
#[test]
fn rfc0033_str_ref_auto_promotion() {
    let src = r#"
@@system Shell {
    interface:
        run(input: &str): String
    machine:
        $Active {
            run(input: &str): String { @@:("ok") }
        }
}
"#;
    let out = compile_source(src, "rust");

    assert!(
        out.contains("Run { input: String }"),
        "event variant should hold owned `String`, not `&str`:\n{}",
        excerpt(&out, "FrameEvent")
    );
    assert!(
        out.contains("pub fn run(&mut self, input: &str)"),
        "user-facing system method should keep the `&str` signature:\n{}",
        excerpt(&out, "pub fn run")
    );
    assert!(
        out.contains("input: input.to_string()"),
        "dispatch site should promote with `.to_string()`:\n{}",
        excerpt(&out, "Rc::new")
    );
    assert!(
        out.contains("input.as_str()"),
        "handler call should re-borrow with `.as_str()`:\n{}",
        excerpt(&out, "_s_Active_hdl_user_run")
    );
    assert!(
        out.contains("fn _s_Active_hdl_user_run(&mut self, __e: &ShellFrameEvent, input: &str)"),
        "handler signature should bind param as `&str`:\n{}",
        excerpt(&out, "_s_Active_hdl_user_run(")
    );

    // Zero non-static lifetime parameters in emitted Rust — the
    // promotion's whole purpose. A real lifetime appears as `&'X`,
    // `<'X>`, or `<'X,` in type position; English apostrophes
    // (`destination's`, `caller's`) are excluded by requiring the
    // preceding byte to be a Rust lifetime introducer.
    //
    // Walk every `'` that is NOT inside a string literal (we strip
    // those first via a simple state machine) and check that it's
    // either `'static` or preceded by `&`/`<`/`,`/` ` AND followed
    // by an alpha-numeric continuation matching a real lifetime
    // name.
    let stripped = strip_string_literals(&out);
    let bytes = stripped.as_bytes();
    let mut hits: Vec<String> = Vec::new();
    for (i, _) in stripped.match_indices('\'') {
        let after = &stripped[i..];
        if after.starts_with("'static") {
            continue;
        }
        if after.len() < 2 || !after.as_bytes()[1].is_ascii_alphabetic() {
            continue;
        }
        // Must be in lifetime position: preceded by `&`, `<`, `,`,
        // ` `, or start-of-file — never preceded by an alphabetic
        // character (which would mean it's an English apostrophe).
        if i == 0 || matches!(bytes[i - 1], b'&' | b'<' | b',' | b' ' | b'\t' | b'(') {
            // Capture up to 16 chars for the message
            let end = (i + 16).min(stripped.len());
            hits.push(stripped[i..end].to_string());
        }
    }
    assert!(
        hits.is_empty(),
        "framec MUST NOT emit non-'static lifetime parameters; found: {:?}",
        hits
    );
}

/// Strip Rust `"..."` string literals from `s` (replacing them
/// with spaces of equal length so byte offsets remain stable).
/// Used by the lifetime-detection assertion so apostrophes inside
/// string literals don't false-fire as lifetimes.
fn strip_string_literals(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            out.push(b' ');
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    continue;
                }
                out.push(b' ');
                i += 1;
            }
            if i < bytes.len() {
                out.push(b' ');
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// RFC-0033 #18: `&[T]` slice in an interface parameter promotes to
/// `Vec<T>` in the variant; dispatch uses `.to_vec()`; handler re-
/// borrows via `.as_slice()`.
#[test]
fn rfc0033_slice_ref_auto_promotion() {
    let src = r#"
@@system Batch {
    interface:
        process(items: &[i32]): i32
    machine:
        $Active {
            process(items: &[i32]): i32 { @@:(0) }
        }
}
"#;
    let out = compile_source(src, "rust");

    assert!(
        out.contains("Process { items: Vec<i32> }"),
        "event variant should hold owned `Vec<i32>`, not `&[i32]`:\n{}",
        excerpt(&out, "FrameEvent")
    );
    assert!(
        out.contains("pub fn process(&mut self, items: &[i32])"),
        "user-facing method should keep `&[i32]` signature:\n{}",
        excerpt(&out, "pub fn process")
    );
    assert!(
        out.contains("items: items.to_vec()"),
        "dispatch site should promote with `.to_vec()`:\n{}",
        excerpt(&out, "Rc::new")
    );
    assert!(
        out.contains("items.as_slice()"),
        "handler call should re-borrow with `.as_slice()`:\n{}",
        excerpt(&out, "_s_Active_hdl_user_process")
    );
}

/// RFC-0033 #12: state-var initializers with function-call expression
/// forms (`String::from(...)`, `Vec::with_capacity(...)`,
/// `Box::new(...)`) MUST reach the generated output verbatim,
/// including nested calls. The pre-fix parser dropped everything
/// between the parens, producing `String::from(,`.
#[test]
fn rfc0033_state_var_expression_initializers() {
    let src = r#"
@@system Logger {
    interface:
        info()
    machine:
        $A {
            $.s: String = String::from("default")
            $.v: Vec<i32> = Vec::with_capacity(8)
            $.b: Box<String> = Box::new(String::from("nested"))
            info() { }
        }
}
"#;
    let out = compile_source(src, "rust");

    for expr in [
        "String::from(\"default\")",
        "Vec::with_capacity(8)",
        "Box::new(String::from(\"nested\"))",
    ] {
        assert!(
            out.contains(expr),
            "state-var expression initializer `{}` missing from output:\n{}",
            expr,
            excerpt(&out, "AContext")
        );
    }
}

/// RFC-0033 #21: every Rust snapshot fixture must produce code
/// that rustc accepts. Snapshot tests historically only diff TEXT;
/// the output could be syntactically invalid Rust and the test would
/// still pass as long as the text matched. This test closes the
/// gap by piping each fixture through `rustc --crate-type lib
/// metadata` — fast (no codegen / link) but it parses,
/// macro-expands, and type-checks.
///
/// Migrated to the RFC-0034 shared helper after that RFC's
/// infrastructure shipped. Fixture exclusions for serde_json-
/// dependent fixtures live in `common::excluded_for("rust")`.
#[test]
fn rfc0034_all_fixtures_compile() {
    use std::process::Command;
    let rustc = match common::find_tool("rustc") {
        Some(p) => p,
        None => {
            eprintln!("rust RFC-0034 compile check skipped: `rustc` not on PATH");
            return;
        }
    };
    common::compile_check_all("rust", "rs", |path| {
        // Use a distinct output path so rustc doesn't try writing
        // to /dev/null (which fails on macOS).
        let metadata_out = path.with_extension("rmeta");
        Command::new(&rustc)
            .args(["--edition=2021", "--crate-type=lib", "--emit=metadata"])
            .arg("-o")
            .arg(&metadata_out)
            .arg(path)
            .output()
            .expect("rustc process")
    });
}

/// Issue #23 (FRAMEC_BUGS): an untyped domain field used to flow
/// through framec silently and emit `pub <field>: ()` (unit type)
/// for Rust, cascading into rustc errors that didn't trace to the
/// missing annotation. The validator (E605) now rejects with a
/// clear source-level diagnostic that points at the actual problem.
///
/// Regression test for the validator-error path. Uses
/// `compile_module` directly so the test can assert on the error
/// instead of panicking — `compile_source` panics on failure.
#[test]
fn issue23_untyped_domain_field_rejected_for_rust() {
    use framec::frame_c::compiler::compile_module;
    use framec::frame_c::compiler::TargetLanguage;
    use std::convert::TryFrom;

    let src = r#"
@@system Foo {
    interface:
        get(): bool
    machine:
        $S {
            get(): bool { @@:(self.a) }
        }
    domain:
        a: bool = true
        b = false
}
"#;
    let lang = TargetLanguage::try_from("rust").unwrap();
    let result = compile_module(src, lang);
    let err = result.expect_err("E605 must reject untyped domain field");
    let msg = err.error;
    assert!(msg.contains("E605"), "expected E605 in error, got: {}", msg);
    assert!(
        msg.contains("'b'"),
        "error should name the offending field 'b', got: {}",
        msg
    );
    assert!(
        msg.contains("missing type annotation"),
        "error should explain the missing annotation, got: {}",
        msg
    );
}

/// Issue #24 (FRAMEC_BUGS): apostrophe inside a `//` line comment
/// in a state body used to trip the GraphViz pipeline's brace-
/// matching because the graphviz target shared Python's syntax
/// skipper — Python treats `'` as a string opener and doesn't
/// recognize `//` comments. The fix routes graphviz through a
/// permissive `FrameStructuralSkipper` that handles both `//` and
/// `#` line comments and leaves `'` as an ordinary byte.
///
/// Source compiles to BOTH targets; the bug was per-target
/// divergence (Rust accepted, graphviz rejected with E002 or E402).
#[test]
fn issue24_apostrophe_in_state_body_comment_graphviz() {
    use framec::frame_c::compiler::compile_module;
    use framec::frame_c::compiler::TargetLanguage;
    use std::convert::TryFrom;

    // Source with `@@[target("rust")]` matches the real bug shape:
    // segmenter detects rust as the source language and uses
    // RustSkipper; lexer/parser receive `TargetLanguage::Graphviz`
    // (from `-l graphviz` CLI) and used to fall back to the Python
    // skipper, which tripped on `'` inside `//` comments.
    let src = r#"
@@[target("rust")]

@@system Foo {
    machine:
        $A {
            // bar's note
            $>() { }
        }
}
"#;
    // Rust target was already correct — assert that still works.
    let rust_lang = TargetLanguage::try_from("rust").unwrap();
    let rust_out = compile_module(src, rust_lang).expect("rust compile");
    assert!(
        rust_out.contains("struct Foo"),
        "rust output missing struct Foo:\n{}",
        &rust_out[..rust_out.len().min(200)]
    );

    // GraphViz target used to fail with E002 "Unmatched '{' for
    // state A". After the fix it produces valid DOT.
    let dot_lang = TargetLanguage::try_from("graphviz").unwrap();
    let dot_out = compile_module(src, dot_lang).expect("graphviz compile after #24 fix");
    assert!(
        dot_out.contains("digraph Foo"),
        "graphviz output missing `digraph Foo`:\n{}",
        &dot_out[..dot_out.len().min(200)]
    );
}

/// Issue #25 sub-case A (FRAMEC_BUGS): the `'"'` char literal
/// (double-quote inside single-quote) was a regression introduced
/// by the #24 fix — the body_closer used to consume `'"'` as a
/// char literal (handled by the per-language Rust closer), but
/// the new FrameStructuralSkipper didn't recognize `'...'` and
/// the `"` inside opened a phantom string. Fixed by adding
/// `scan_char_literal` to the structural body_closer.
#[test]
fn issue25a_double_quote_inside_char_literal_graphviz() {
    use framec::frame_c::compiler::compile_module;
    use framec::frame_c::compiler::TargetLanguage;
    use std::convert::TryFrom;

    let src = r#"
@@[target("rust")]

@@system Foo {
    interface:
        go(c: char)
    machine:
        $A {
            go(c: char) {
                if c == '"' {
                    -> $B
                }
            }
        }
        $B { }
}
"#;
    let dot = compile_module(src, TargetLanguage::try_from("graphviz").unwrap())
        .expect("graphviz compile after #25 sub-case A fix");
    assert!(dot.contains("digraph Foo"), "missing digraph Foo:\n{}", dot);
}

/// Issue #25 sub-case B: multiple apostrophe-bearing `//` comments
/// in the SAME state body. The #24 fix made the body_closer handle
/// a single apostrophe correctly, but the enrichment scanner
/// (called from the validator path) still used Python's scanner —
/// which tripped on the same `'`. Two such comments compounded the
/// failure into "unterminated string". Fixed by routing the
/// graphviz enrichment scanner through FrameStructuralSkipper too.
#[test]
fn issue25b_multiple_apostrophe_comments_in_same_state_body() {
    use framec::frame_c::compiler::compile_module;
    use framec::frame_c::compiler::TargetLanguage;
    use std::convert::TryFrom;

    let src = r#"
@@[target("rust")]

@@system Foo {
    machine:
        $A {
            // first comment with bar's apostrophe
            $>() {
                // second comment with baz's apostrophe
            }
        }
}
"#;
    let dot = compile_module(src, TargetLanguage::try_from("graphviz").unwrap())
        .expect("graphviz compile after #25 sub-case B fix");
    assert!(dot.contains("digraph Foo"), "missing digraph Foo:\n{}", dot);
}

/// Issue #26 (FRAMEC_BUGS): GraphViz silently dropped transitions
/// inside `if c == '"' { ... }` branches because the hand-coded
/// structural scanner from #25 over-consumed the branch body
/// when scanning past the `'"'` char literal. Replaced the
/// hand-coded scanner with a Frame-generated FSM
/// (`frame_structural_skipper.frs` →
/// `frame_structural_skipper.gen.rs`) modeled after
/// `rust_skipper.frs`. The FSM correctly handles the char
/// literal AND keeps scanning the surrounding tokens, so
/// transitions inside the branch are collected and emitted as
/// edges.
///
/// This test asserts the canonical reproducer — `A -> B [label]`
/// must appear in the DOT output despite the `if c == '"'` guard.
#[test]
fn issue26_transition_in_char_quote_branch_appears_in_graphviz() {
    use framec::frame_c::compiler::compile_module;
    use framec::frame_c::compiler::TargetLanguage;
    use std::convert::TryFrom;

    let src = r#"
@@[target("rust")]

@@system Foo {
    interface:
        go(c: char)

    machine:
        $A {
            go(c: char) {
                if c == '"' {
                    -> $B
                }
            }
        }

        $B { }
}
"#;
    let dot = compile_module(src, TargetLanguage::try_from("graphviz").unwrap())
        .expect("graphviz compile after #26 fix");
    assert!(
        dot.contains("A -> B"),
        "missing `A -> B` transition edge in graphviz output:\n{}",
        dot
    );
}

/// Helper: grab a 12-line window around the first match of `needle`,
/// for clearer assertion failure messages than a 5,000-line dump.
fn excerpt(haystack: &str, needle: &str) -> String {
    let lines: Vec<&str> = haystack.lines().collect();
    if let Some(pos) = lines.iter().position(|l| l.contains(needle)) {
        let lo = pos.saturating_sub(2);
        let hi = (pos + 10).min(lines.len());
        lines[lo..hi].join("\n")
    } else {
        format!(
            "(no line contained `{}` — first 12 lines:\n{}",
            needle,
            lines[..lines.len().min(12)].join("\n")
        )
    }
}

// ─────────────────────────────────────────────────────────────────────
// FRAMEC_BUGS #31 regression — no_std-portable paths.
//
// The runtime must never emit `std::`-prefixed paths (they don't exist
// under `#![no_std]`, blocking bare-metal consumers like Frame OS). It
// emits `core::any::Any`, `alloc::rc::Rc`, `alloc::collections::BTreeMap`
// instead, plus an `extern crate alloc;` + `use alloc::{vec, format};`
// module preamble so the file is self-contained in hosted *and* no_std
// builds. Reintroducing any `std::` path here regresses #31.
// ─────────────────────────────────────────────────────────────────────
#[test]
fn bug31_no_std_portable_paths() {
    // An interface return value exercises FrameReturn
    // (`_Lifecycle(Rc<dyn Any>)`), FrameContext (`Rc<Event>` + the
    // `_data` map), and FrameValue (`Dict(...)`) — every site that
    // previously hardcoded `std::`.
    let src = r#"
@@system Foo {
    interface:
        bar(): i64
    machine:
        $A {
            bar(): i64 { @@:(1) }
        }
}
"#;
    let out = compile_source(src, "rust");

    for forbidden in ["std::rc::Rc", "std::collections::HashMap", "std::any::Any"] {
        assert!(
            !out.contains(forbidden),
            "generated Rust still emits `{}` — regresses no_std (#31)\n--- excerpt ---\n{}",
            forbidden,
            excerpt(&out, forbidden)
        );
    }

    // The preamble carries `extern crate alloc;` (for the type paths)
    // AND `use alloc::{vec, format};` (so `vec!`/`format!` resolve under
    // no_std with no consumer help — #33). framec's Rust output targets
    // edition 2018+; the crate-relative `use alloc::...` does not resolve
    // under edition 2015 (bare `rustc`, no `--edition`), which is not a
    // supported configuration (see FRAMEC_BUGS #31/#33).
    for needed in [
        "extern crate alloc;",
        "use alloc::{vec, format};",
        "alloc::rc::Rc",
        "alloc::collections::BTreeMap",
        "core::any::Any",
    ] {
        assert!(
            out.contains(needed),
            "generated Rust missing no_std-portable token `{}` (#31)\n--- first 30 lines ---\n{}",
            needed,
            out.lines().take(30).collect::<Vec<_>>().join("\n")
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// FRAMEC_BUGS #33 regression — generated Rust must actually COMPILE in a
// `#![no_std]` + alloc consumer that provides only the heap *types*
// (String/Vec/Box), with NO `#[macro_use]`. #33 was a regression where
// the `use alloc::{vec, format};` import was dropped: the text was absent
// AND, more importantly, `vec!`/`format!` failed to resolve under no_std
// (`cannot find macro vec`). A string-presence check (bug31 above) can't
// prove resolution; this test does an end-to-end `x86_64-unknown-none`
// compile — the exact Frame OS bare-metal scenario.
//
// Skipped (not failed) when the bare-metal target's precompiled core/alloc
// aren't installed (`rustup target add x86_64-unknown-none`), so it's a
// no-op on dev/CI hosts without it rather than a false failure.
// ─────────────────────────────────────────────────────────────────────
#[test]
fn bug33_generated_compiles_no_std() {
    use std::process::Command;

    const TARGET: &str = "x86_64-unknown-none";

    // Probe: are the target's precompiled core/alloc available? Compile a
    // trivial `#![no_std]` lib; if core is missing rustc errors with E0463
    // ("can't find crate for `core`"). Treat that as "skip", anything else
    // as "target usable".
    let tmp = std::env::temp_dir().join(format!("framec_bug33_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let probe = tmp.join("probe.rs");
    std::fs::write(&probe, "#![no_std]\npub fn p() {}\n").expect("write probe");
    let probe_out = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "--crate-type",
            "lib",
            "--target",
            TARGET,
            "-o",
        ])
        .arg(tmp.join("probe.rlib"))
        .arg(&probe)
        .output();
    let target_available = match &probe_out {
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            o.status.success() || !err.contains("can't find crate for `core`")
        }
        Err(_) => false, // rustc not invokable — skip
    };
    if !target_available {
        eprintln!(
            "bug33_generated_compiles_no_std: SKIP — `{}` core/alloc not \
             installed (run `rustup target add {}`)",
            TARGET, TARGET
        );
        return;
    }

    // Generate a system that exercises Rc / BTreeMap / Any / vec! /
    // (interface return) — i.e. every no_std-sensitive emission.
    let generated = compile_source(
        r#"
@@system Bug33 {
    interface:
        bump()
        total(): i64
    machine:
        $A {
            bump()       { -> $B }
            total(): i64 { @@:(0) }
        }
        $B {
            total(): i64 { @@:(1) }
        }
}
"#,
        "rust",
    );

    // The exact #33 consumer shape: `#![no_std]`, heap TYPES only via the
    // include site, NO `#[macro_use]`. If `use alloc::{vec, format};` is
    // dropped again, `vec!` in `__prepareEnter(...)` fails to resolve here.
    let lib = format!(
        "#![no_std]\n\
         extern crate alloc;\n\
         mod frame_systems {{\n\
         pub use alloc::boxed::Box;\n\
         pub use alloc::string::{{String, ToString}};\n\
         pub use alloc::vec::Vec;\n\
         {generated}\n\
         }}\n"
    );
    let lib_path = tmp.join("lib.rs");
    std::fs::write(&lib_path, &lib).expect("write lib.rs");

    let out = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "--crate-type",
            "lib",
            "--target",
            TARGET,
            "-o",
        ])
        .arg(tmp.join("out.rlib"))
        .arg(&lib_path)
        .output()
        .expect("invoke rustc");

    assert!(
        out.status.success(),
        "generated Rust failed to compile under #![no_std] ({}) — regresses #33\n\
         --- rustc stderr ---\n{}",
        TARGET,
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ─────────────────────────────────────────────────────────────────────
// FRAMEC_BUGS #34 / RFC-0025.1 regression — enter args are type-faithful
// (Vec<Rc<dyn Any>> + downcast), NOT stringified (Vec<String> + parse).
//
// The old path bound `let n: i64 = args.get(0)...parse::<i64>()...`, which
// silently defaulted on a parse miss (a literal `42` would round-trip, but
// the contract was fragile) and was a HARD COMPILE BREAK for compound types
// (`Vec<i64>: FromStr` not satisfied). This test asserts the dispatcher
// downcasts, and compiles + RUNS a system exercising both a literal `int`
// enter arg (Rust-inferred `i32`, widened to the declared `i64`) and a
// compound `Vec<i64>` enter arg.
// ─────────────────────────────────────────────────────────────────────
#[test]
fn bug34_enter_args_type_faithful() {
    use std::process::Command;

    let generated = compile_source(
        r#"
@@system Bug34 {
    interface:
        run()
        sum(): i64
        head(): i64
    machine:
        $Start { run() { -> (42) $Mid } }
        $Mid {
            $>(n: i64)   { self.total = n + 1 }
            sum(): i64   { @@:(self.total) }
            run()        { -> (vec![99]) $End }
        }
        $End {
            $>(xs: Vec<i64>) { self.total = xs[0] }
            head(): i64      { @@:(self.total) }
        }
    domain:
        total: i64 = 0
}
"#,
        "rust",
    );

    // Static contract (RFC-0025.1): enter args are carried in the typed
    // per-state StateContext, NOT stringified or type-erased. The literal
    // `int` enter arg is written into the typed ctx field (`ctx.n = 42`,
    // coercing to i64) and read back typed — no `Vec<String>`, no
    // `parse::<T>()`, no `Rc<dyn Any>` lifecycle channel.
    assert!(
        !generated.contains("parse::<i64>") && !generated.contains("parse::<Vec"),
        "enter arg must NOT stringify-and-parse — regresses #34\n{generated}"
    );
    assert!(
        !generated.contains("enter_args: Vec<String>") && !generated.contains("FrameEnter { args:"),
        "enter args must not ride a stringified/erased Vec — regresses RFC-0025.1\n{generated}"
    );
    assert!(
        generated.contains("StateContext::Mid(ref mut ctx)") && generated.contains("ctx.n ="),
        "enter arg must be written into the typed StateContext ctx field\n{generated}"
    );

    // Runtime contract: literal int enter arg widens to i64 (n+1==43), and
    // a compound Vec<i64> enter arg round-trips (head==99).
    let tmp = std::env::temp_dir().join(format!("framec_bug34_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let main_rs = format!(
        "{generated}\n\
         fn main() {{\n\
         let mut m = Bug34::new();\n\
         m.run();\n\
         assert_eq!(m.sum(), 43, \"literal int enter arg did not widen to i64\");\n\
         m.run();\n\
         assert_eq!(m.head(), 99, \"compound Vec<i64> enter arg did not round-trip\");\n\
         }}\n"
    );
    let src = tmp.join("main.rs");
    std::fs::write(&src, &main_rs).expect("write main.rs");
    let bin = tmp.join("bug34");

    let build = Command::new("rustc")
        .args(["--edition", "2021", "-o"])
        .arg(&bin)
        .arg(&src)
        .output();
    let build = match build {
        Ok(o) => o,
        Err(_) => {
            eprintln!("bug34_enter_args_type_faithful: SKIP — rustc not invokable");
            return;
        }
    };
    assert!(
        build.status.success(),
        "generated Rust failed to compile (regresses #34)\n--- rustc stderr ---\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new(&bin).output().expect("run bug34 bin");
    assert!(
        run.status.success(),
        "enter args did not arrive type-faithfully at runtime (regresses #34)\n\
         --- stderr ---\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ─────────────────────────────────────────────────────────────────────
// FRAMEC_BUGS #35 / RFC-0025.1 regression — a `<$` exit handler with
// params, on the START state, must bind its exit args from the typed
// ctx. The start-state lifecycle special-case (which drops params for
// the START `$>` enter handler, since those come from the system header)
// wrongly swallowed the START `<$` exit handler too — the dispatcher
// emitted the no-arg arm and the method signature dropped the param, so
// `<$(code)` produced uncompilable Rust (unbound `code`). Gating that
// special-case to `$>`-only fixes it. `$A` here IS the start state.
// ─────────────────────────────────────────────────────────────────────
#[test]
fn bug35_start_state_exit_arg_binds() {
    use std::process::Command;

    let generated = compile_source(
        r#"
@@system Bug35 {
    interface:
        go()
        seen(): i64
    machine:
        $A {
            go()          { (99) -> $B }
            <$(code: i64) { self.s = code }
        }
        $B {
            seen(): i64 { @@:(self.s) }
        }
    domain:
        s: i64 = 0
}
"#,
        "rust",
    );

    // The start state's exit handler must take + bind `code` (not the
    // no-arg start-state special-case).
    assert!(
        generated.contains("fn _s_A_hdl_frame_exit(&mut self, __e: &Bug35FrameEvent, code:"),
        "start-state `<$` exit handler must keep its `code` param (#35)\n{generated}"
    );
    assert!(
        generated.contains("StateContext::A(ref mut ctx)") && generated.contains("ctx.code = 99"),
        "exit arg must write into the typed source ctx (#35)\n{generated}"
    );

    // Compile + run: the exit arg round-trips (seen == 99).
    let tmp = std::env::temp_dir().join(format!("framec_bug35_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let main_rs = format!(
        "{generated}\n\
         fn main() {{\n\
         let mut m = Bug35::new();\n\
         m.go();\n\
         assert_eq!(m.seen(), 99, \"start-state exit arg did not bind\");\n\
         }}\n"
    );
    let src = tmp.join("main.rs");
    std::fs::write(&src, &main_rs).expect("write main.rs");
    let bin = tmp.join("bug35");
    let build = match Command::new("rustc")
        .args(["--edition", "2021", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            eprintln!("bug35_start_state_exit_arg_binds: SKIP — rustc not invokable");
            return;
        }
    };
    assert!(
        build.status.success(),
        "start-state exit-arg fixture failed to compile (regresses #35)\n--- rustc stderr ---\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&bin).output().expect("run bug35 bin");
    assert!(
        run.status.success(),
        "exit arg did not arrive at runtime (regresses #35)\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ─────────────────────────────────────────────────────────────────────
// RFC-0025.1 — decorated `pop$` args ride the typed StateContext, never a
// stringified Vec. Exit args (`(7) ->`) write the SOURCE state's ctx
// (known); enter args (`-> (42)`) write the RESTORED state's ctx via a
// `match __popped` over poppable variants (the restored state is dynamic).
// `$Idle` is pushed, then popped back into with both decorations.
// ─────────────────────────────────────────────────────────────────────
#[test]
fn pop_decorated_args_typed() {
    use std::process::Command;

    let generated = compile_source(
        r#"
@@system PopArgs {
    interface:
        go()
        nest()
        back()
        entered(): i64
        exited(): i64
    machine:
        $Start { go() { -> $Idle } }
        $Idle {
            $>(tag: i64)   { self.e = self.e + tag }
            nest()         { push$ -> $Nested }
            entered(): i64 { @@:(self.e) }
            exited(): i64  { @@:(self.x) }
        }
        $Nested {
            <$(code: i64) { self.x = code }
            back()        { (7) -> (42) pop$ }
        }
    domain:
        e: i64 = 0
        x: i64 = 0
}
"#,
        "rust",
    );

    // Exit arg → source ctx; enter arg → match over restored ctx variants.
    // No stringified pop arg Vec.
    assert!(
        generated.contains("if let PopArgsStateContext::Nested(ref mut ctx) =")
            && generated.contains("ctx.code = 7"),
        "pop exit arg must write the source state's typed ctx\n{generated}"
    );
    assert!(
        generated.contains("match __popped.state_context")
            && generated.contains("PopArgsStateContext::Idle(ref mut ctx)")
            && generated.contains("ctx.tag = 42"),
        "pop enter arg must write the restored state's typed ctx via a match\n{generated}"
    );
    assert!(
        !generated.contains("enter_args") && !generated.contains("exit_args"),
        "pop args must not ride a stringified Vec (RFC-0025.1)\n{generated}"
    );

    // Compile + run: exit arg (7) reaches `$Nested.<$`, enter arg (42)
    // reaches the restored `$Idle.$>`.
    let tmp = std::env::temp_dir().join(format!("framec_popargs_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let main_rs = format!(
        "{generated}\n\
         fn main() {{\n\
         let mut m = PopArgs::new();\n\
         m.go();\n\
         m.nest();\n\
         m.back();\n\
         assert_eq!(m.exited(), 7, \"pop exit arg -> source <$\");\n\
         assert_eq!(m.entered(), 42, \"pop enter arg -> restored $>\");\n\
         }}\n"
    );
    let src = tmp.join("main.rs");
    std::fs::write(&src, &main_rs).expect("write main.rs");
    let bin = tmp.join("popargs");
    let build = match Command::new("rustc")
        .args(["--edition", "2021", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            eprintln!("pop_decorated_args_typed: SKIP — rustc not invokable");
            return;
        }
    };
    assert!(
        build.status.success(),
        "decorated pop fixture failed to compile (RFC-0025.1)\n--- rustc stderr ---\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&bin).output().expect("run popargs bin");
    assert!(
        run.status.success(),
        "decorated pop args did not arrive at runtime\n--- stderr ---\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ─────────────────────────────────────────────────────────────────────
// FRAMEC_BUGS #40 — a state named `$Empty` must not collide with the
// synthesized no-context sentinel. The StateContext enum emits a variant
// per state PLUS a catch-all sentinel; the sentinel used to be hardcoded
// as `Empty`, so a user `$Empty` state produced a second `Empty` variant
// → `error[E0428]: the name 'Empty' is defined multiple times`. The
// sentinel is now `__NoContext` (reserved prefix). Compile-only is enough
// (the bug was a hard compile break).
// ─────────────────────────────────────────────────────────────────────
#[test]
fn bug40_empty_state_name_no_variant_collision() {
    let generated = compile_source(
        r#"
@@system EmptyName {
    interface:
        go()
        poke()
    machine:
        $Empty {
            go() { -> $Active }
        }
        $Active {
            poke() {}
        }
}
"#,
        "rust",
    );

    // The sentinel is namespaced, and the user `$Empty` state keeps its
    // own `Empty` variant — no duplicate.
    assert!(
        generated.contains("__NoContext"),
        "no-context sentinel must be the reserved `__NoContext`, not `Empty`\n{generated}"
    );
    assert_eq!(
        generated.matches("    Empty,\n").count(),
        1,
        "exactly one `Empty` variant (the user state) — the sentinel must \
         not add a second (#40)\n{generated}"
    );

    // Compile the generated Rust — this is the actual #40 repro (E0428).
    let tmp = std::env::temp_dir().join(format!("framec_bug40_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let src = tmp.join("lib.rs");
    std::fs::write(&src, &generated).expect("write lib.rs");
    let out = tmp.join("libbug40.rlib");
    let build = match std::process::Command::new("rustc")
        .args(["--edition", "2021", "--crate-type", "lib", "-o"])
        .arg(&out)
        .arg(&src)
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            eprintln!("bug40_empty_state_name_no_variant_collision: SKIP — rustc not invokable");
            return;
        }
    };
    assert!(
        build.status.success(),
        "`$Empty` state must compile (regresses #40 / E0428)\n--- rustc stderr ---\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Regression for issue #129 — a bare context-read used as the WHOLE
/// return value must not emit redundant parens around the expanded
/// `(match …)` rvalue.
///
/// `@@:(@@:data.k)` (and the sibling `@@:(@@:return)`) lower the inner
/// read to a self-delimiting `(match …)` group; re-wrapping that in the
/// `FrameReturn::Variant(…)` constructor without peeling the existing
/// parens produced `Variant((match …))`, which rustc flags as
/// `unused_parens` ("unnecessary parentheses around function argument").
/// The fix peels one balanced outer layer in `build_return_val_expr`.
#[test]
fn bug129_bare_context_read_return_no_double_paren() {
    // @@:data read as sole return value.
    let data_src = r#"
@@[target("rust")]
@@system Q {
    interface:
        f(): String
    machine:
        $A {
            f(): String {
                @@:data.k = String::from("x")
                @@:(@@:data.k)
            }
        }
}
"#;
    let gen = compile_source(data_src, "rust");
    let ret_line = gen
        .lines()
        .find(|l| l.contains("let __return_val ="))
        .expect("generated return-val assignment");
    assert!(
        ret_line.contains("QFrameReturn::F(match "),
        "expected a single-paren variant wrap, got: {ret_line}"
    );
    assert!(
        !ret_line.contains("QFrameReturn::F((match "),
        "redundant double paren regressed (#129): {ret_line}"
    );

    // Sibling spelling: @@:return read as sole return value.
    let return_src = r#"
@@[target("rust")]
@@system Q {
    interface:
        f(): String
    machine:
        $A {
            f(): String {
                @@:return = String::from("x")
                @@:(@@:return)
            }
        }
}
"#;
    let gen = compile_source(return_src, "rust");
    assert!(
        !gen.contains("QFrameReturn::F((match "),
        "redundant double paren regressed for @@:return (#129):\n{gen}"
    );
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "rust"));
}
