//! RFC-0027 in-tree snapshot tests — php backend.
//!
//! Mirrors python_snapshots.rs against the php target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::{compile_check_all, compile_fixture, find_tool};
use std::process::Command;

/// RFC-0034: every canonical fixture's framec-emitted PHP output
/// must parse cleanly under `php -l` (lint / parse-only mode).
#[test]
fn rfc0034_all_fixtures_compile() {
    let php = match find_tool("php") {
        Some(p) => p,
        None => {
            eprintln!("php RFC-0034 compile check skipped: `php` not on PATH");
            return;
        }
    };
    compile_check_all("php", "php", |path| {
        Command::new(&php)
            .arg("-l")
            .arg(path)
            .output()
            .expect("php process")
    });
}

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "php"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "php"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "php"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "php"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "php"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "php"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "php"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "php"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "php"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "php"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "php"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "php"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "php"));
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "php"));
}
