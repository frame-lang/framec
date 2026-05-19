//! RFC-0027 in-tree snapshot tests — Python backend.
//!
//! Snapshots the framec-emitted Python code for the canonical
//! 3-fixture corpus. Changes to the Python backend produce
//! reviewable `.snap` diffs in PRs.
//!
//! Re-bless workflow when an intentional codegen change is made:
//!   cargo install cargo-insta   # one-time
//!   cargo test --test python_snapshots
//!   cargo insta review
//!   git add tests/snapshots/ && git commit
//!
//! Adding a backend: copy this file to e.g. `java_snapshots.rs`
//! and change the target string in each call. Phase 2 of RFC-0027
//! rolls this out to the remaining 16 backends.

mod common;

use common::{compile_check_all, compile_fixture, compile_source, find_tool};
use std::process::Command;

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "python_3"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "python_3"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "python_3"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "python_3"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "python_3"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "python_3"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "python_3"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "python_3"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "python_3"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "python_3"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "python_3"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "python_3"));
}

/// RFC-0034: every canonical fixture's framec-emitted Python
/// output must parse cleanly under `python3 -m py_compile`. Closes
/// the snapshot-doesn't-compile gap for Python — snapshots only
/// diff text, so without this check a fixture could freeze
/// syntactically invalid Python and the test suite would still
/// pass.
#[test]
fn rfc0034_all_fixtures_compile() {
    let py3 = match find_tool("python3") {
        Some(p) => p,
        None => {
            eprintln!("python_3 RFC-0034 compile check skipped: `python3` not on PATH");
            return;
        }
    };
    compile_check_all("python_3", "py", |path| {
        Command::new(&py3)
            .args(["-m", "py_compile"])
            .arg(path)
            .output()
            .expect("python3 process")
    });
}

/// RFC-0033 #12 (cross-backend generalization): the parser fix for
/// path-expression call forms (`String::from(args)`) also covers
/// bare function-call initializers (`list()`, `dict()`, `MyClass(x)`).
/// Before the fix, the parser dropped everything after the
/// identifier and Python emitted `state_vars["x"] = list` (a
/// reference to the type), not `state_vars["x"] = list()` (a fresh
/// instance). Same parser bug — the user's code is silently wrong.
#[test]
fn rfc0033_state_var_call_initializers_python() {
    let src = r#"
@@system Repro {
    interface:
        get_x()
    machine:
        $A {
            $.lst: list = list()
            $.dct: dict = dict()
            $.s: str = str("hello")
            get_x() { @@:(self.lst) }
        }
}
"#;
    let out = compile_source(src, "python_3");

    for expected in [
        "compartment.state_vars[\"lst\"] = list()",
        "compartment.state_vars[\"dct\"] = dict()",
        "compartment.state_vars[\"s\"] = str(\"hello\")",
    ] {
        assert!(
            out.contains(expected),
            "state-var call initializer not preserved — expected `{}` in output",
            expected
        );
    }
}
