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
        if i == 0
            || matches!(bytes[i - 1], b'&' | b'<' | b',' | b' ' | b'\t' | b'(')
        {
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
/// --emit=metadata` — fast (no codegen / link) but it parses,
/// macro-expands, and type-checks.
///
/// If you add a new fixture in `tests/fixtures/` that exercises a
/// Rust-specific feature, append it to FIXTURES below.
#[test]
fn rfc0033_all_fixtures_compile_under_rustc() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // 03_persist and 12_no_persist excluded — both emit
    // `serde_json::Value` references the in-process `rustc`
    // invocation can't resolve without dependency metadata. The
    // matrix test-env exercises persist via cargo and covers them
    // there; this in-process test catches the structural
    // (parser/type-checker) failures of the other 10 fixtures.
    const FIXTURES: &[&str] = &[
        "01_linear_fsm",
        "02_hsm",
        "04_state_args",
        "05_pushpop",
        "06_selfcall",
        "07_forward",
        "08_lifecycle",
        "09_return_explicit",
        "10_actions",
        "11_consts",
    ];

    for name in FIXTURES {
        let code = compile_fixture(name, "rust");
        // Write to a tempfile so rustc can read by path (rustc -
        // doesn't accept stdin for the libstd metadata path).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("{}.rs", name));
        let mut f = std::fs::File::create(&path).expect("create tmp .rs");
        f.write_all(code.as_bytes()).expect("write tmp .rs");
        drop(f);

        let metadata_out = dir.path().join(format!("lib{}.rmeta", name));
        let out = Command::new("rustc")
            .args(["--edition=2021", "--crate-type=lib", "--emit=metadata"])
            .arg("-o")
            .arg(&metadata_out)
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("rustc must be on PATH");
        assert!(
            out.status.success(),
            "fixture `{}` emits Rust that rustc rejects:\n{}\n--- first 60 lines of output ---\n{}",
            name,
            String::from_utf8_lossy(&out.stderr),
            code.lines().take(60).collect::<Vec<_>>().join("\n")
        );
    }
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
        format!("(no line contained `{}` — first 12 lines:\n{}", needle, lines[..lines.len().min(12)].join("\n"))
    }
}
