//! RFC-0027 in-tree snapshot tests — typescript backend.
//!
//! Mirrors python_snapshots.rs against the typescript target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::compile_fixture;

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "typescript"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "typescript"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "typescript"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "typescript"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "typescript"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "typescript"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "typescript"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "typescript"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "typescript"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "typescript"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "typescript"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "typescript"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "typescript"));
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "typescript"));
}
