//! Ruby reserved-name handling.
//!
//! #14: a user event named `send` must not break internal dispatch — the
//! router routes via the override-proof `__send__`, so the user's
//! `def send` is harmless.
//! #15: a user event named `initialize` collides with Ruby's constructor
//! (a `def initialize` would replace it); rejected with E501.

mod common;
use common::{compile_expect_error, compile_source};

#[test]
fn ruby_router_dispatches_via_override_proof_send() {
    let out = compile_source(
        "@@system R { interface: send() machine: $A { send() { } } }",
        "ruby",
    );
    assert!(
        out.contains("__send__(handler_name"),
        "router must dispatch via the override-proof __send__:\n{out}"
    );
    assert!(
        out.contains("def send"),
        "the user's `send` event handler must still be emitted:\n{out}"
    );
}

#[test]
fn ruby_initialize_event_is_rejected() {
    let err = compile_expect_error(
        "@@system R { interface: initialize() machine: $A { initialize() { } } }",
        "ruby",
    );
    assert!(err.contains("E501"), "expected E501, got:\n{err}");
}

#[test]
fn initialize_event_allowed_on_non_ruby_target() {
    // The collision is Ruby-specific (Python's constructor is `__init__`).
    let _ = compile_source(
        "@@system R { interface: initialize() machine: $A { initialize() { } } }",
        "python_3",
    );
}
