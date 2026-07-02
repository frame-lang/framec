//! RFC-0027 in-tree snapshot tests — ruby backend.
//!
//! Mirrors python_snapshots.rs against the ruby target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::{compile_check_all, compile_fixture, find_tool};
use std::process::Command;

/// RFC-0034: every canonical fixture's framec-emitted Ruby output
/// must parse cleanly under `ruby -c` (compile-check, no run).
#[test]
fn rfc0034_all_fixtures_compile() {
    let ruby = match find_tool("ruby") {
        Some(p) => p,
        None => {
            eprintln!("ruby RFC-0034 compile check skipped: `ruby` not on PATH");
            return;
        }
    };
    compile_check_all("ruby", "rb", |path| {
        Command::new(&ruby)
            .arg("-c")
            .arg(path)
            .output()
            .expect("ruby process")
    });
}

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "ruby"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "ruby"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "ruby"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "ruby"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "ruby"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "ruby"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "ruby"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "ruby"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "ruby"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "ruby"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "ruby"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "ruby"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "ruby"));
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "ruby"));
}
