//! RFC-0027 in-tree snapshot tests — swift backend.
//!
//! Mirrors python_snapshots.rs against the swift target.
//! Re-bless workflow + corpus discipline documented in
//! CONTRIBUTING.md § "Snapshot tests (RFC-0027)".

mod common;

use common::compile_fixture;

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "swift"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "swift"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "swift"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "swift"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "swift"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "swift"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "swift"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "swift"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "swift"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "swift"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "swift"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "swift"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "swift"));
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "swift"));
}

/// #175 — a Frame method/field/param named after a Swift keyword (`init`,
/// `guard`, `default`) must be backtick-escaped in the emitted Swift at its
/// declaration and every call/access site, or `swiftc` rejects it. Golden
/// coverage of the `swift_escape_ident` escaping.
#[test]
fn keyword_ident() {
    insta::assert_snapshot!(compile_fixture("15_keyword_ident", "swift"));
}

/// #178 — `@@[persist]` save of a user `Codable`-typed domain field must encode
/// it through `JSONEncoder` (symmetric with the `JSONDecoder` restore), not drop
/// the raw struct into a `[String: Any]` for `JSONSerialization` (which throws
/// `Invalid type in JSON write (__SwiftValue)` at runtime). Scalar fields keep
/// the raw `j[x] = x` fast-path — golden coverage of both branches.
#[test]
fn persist_class() {
    insta::assert_snapshot!(compile_fixture("16_persist_class", "swift"));
}
