//! RFC-0027 in-tree snapshot tests — csharp backend.
//!
//! Mirrors python_snapshots.rs against the csharp target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::{compile_fixture, compile_source};

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "csharp"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "csharp"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "csharp"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "csharp"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "csharp"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "csharp"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "csharp"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "csharp"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "csharp"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "csharp"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "csharp"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "csharp"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "csharp"));
}

// ─────────────────────────────────────────────────────────────────────
// FRAMEC_BUGS #32 regression — a type-cast directly before an inline
// `@@:self.method()` self-call must not get a spurious statement
// terminator spliced into it.
//
// `double x = (double) @@:self.b()` used to emit
// `double x = (double); this.b();` on semicolon backends — the trailing
// `)` of the cast was mistaken for a complete statement and a `;` was
// injected mid-expression, breaking C#/Java compilation. The fix only
// injects the prior-statement terminator for STANDALONE self-calls.
// ─────────────────────────────────────────────────────────────────────
#[test]
fn bug32_cast_before_inline_self_call() {
    let src = r#"
@@system M {
    interface:
        a(): double
        b(): double
    machine:
        $S {
            a(): double {
                double x = (double) @@:self.b();
                @@:(x)
            }
            b(): double { @@:(1.0) }
        }
}
"#;
    let out = compile_source(src, "csharp");

    // The inline self-call must inline as an expression, cast intact.
    assert!(
        out.contains("(double) this.b()"),
        "expected inline cast self-call `(double) this.b()` (#32)\n--- output ---\n{}",
        out
    );
    // The mangled mid-expression terminator must NOT appear.
    assert!(
        !out.contains("(double);"),
        "spurious `;` spliced after cast — regresses #32\n--- output ---\n{}",
        out
    );
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "csharp"));
}
