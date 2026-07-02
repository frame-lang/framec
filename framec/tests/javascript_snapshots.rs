//! RFC-0027 in-tree snapshot tests — javascript backend.
//!
//! Mirrors python_snapshots.rs against the javascript target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::{compile_check_all, compile_fixture, compile_source, find_tool};
use std::process::Command;

/// Regression for the JS/TS `push$ -> $State` codegen bug: the with-transition
/// form must use the compartment model (`__prepareEnter` + `__transition`), not
/// the removed `_transition()`, which threw at runtime. The `05_pushpop` fixture
/// only covers bare `push$` followed by a separate transition, so this asserts
/// the push-with-transition path directly.
#[test]
fn push_transition() {
    let src = r#"
@@system PushTransition {
    interface:
        go()
        back()
    machine:
        $A { go() { push$ -> $B } }
        $B { back() { -> pop$ } }
}
"#;
    let out = compile_source(src, "javascript");
    assert!(
        out.contains("this._state_stack.push(this.__compartment)"),
        "push$ -> $State must push the current compartment onto the stack:\n{out}"
    );
    assert!(
        out.contains("this.__transition(__compartment)"),
        "push$ -> $State must transition via the compartment model:\n{out}"
    );
    assert!(
        !out.contains("this._transition("),
        "push$ -> $State must not call the non-existent _transition():\n{out}"
    );
}

/// RFC-0034: every canonical fixture's framec-emitted JavaScript
/// output must parse cleanly under `node --check`. Closes the
/// snapshot-doesn't-compile gap for JavaScript.
#[test]
fn rfc0034_all_fixtures_compile() {
    let node = match find_tool("node") {
        Some(p) => p,
        None => {
            eprintln!("javascript RFC-0034 compile check skipped: `node` not on PATH");
            return;
        }
    };
    // `.mjs` so node parses as an ES module — framec emits `export`
    // statements which `node --check` rejects under CommonJS mode.
    compile_check_all("javascript", "mjs", |path| {
        Command::new(&node)
            .arg("--check")
            .arg(path)
            .output()
            .expect("node process")
    });
}

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "javascript"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "javascript"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "javascript"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "javascript"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "javascript"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "javascript"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "javascript"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "javascript"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "javascript"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "javascript"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "javascript"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "javascript"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "javascript"));
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "javascript"));
}
