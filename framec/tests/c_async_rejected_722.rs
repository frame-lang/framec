//! Issue #111 R4 — E722: the C target has no async runtime, so `@@[async]`
//! (or a bare async member) can't realize RFC-0043's single-driver gate. Rather
//! than silently downgrade to a sync, ungated system (the old behaviour — a
//! warning to stderr, easy to miss), framec now rejects it at validation so the
//! dropped contract can't pass silently.

mod common;
use common::{compile_expect_error, compile_source};

const ASYNC_ATTR: &str = r#"
@@[main]
@@[async]
@@system G {
    interface: async f()
    machine: $S { $>() {} f() {} }
}
"#;

const ASYNC_MEMBER_ONLY: &str = r#"
@@[main]
@@system G {
    interface: async f()
    machine: $S { $>() {} f() {} }
}
"#;

const SYNC: &str = r#"
@@[main]
@@system G {
    interface: f()
    machine: $S { $>() {} f() {} }
}
"#;

#[test]
fn c_async_attribute_is_e722() {
    let err = compile_expect_error(ASYNC_ATTR, "c");
    assert!(
        err.contains("E722"),
        "[#111 R4] expected E722 on C + @@[async]; got:\n{err}"
    );
}

#[test]
fn c_bare_async_member_is_e722() {
    // Even without the attribute (E720 territory), the async surface on C is a
    // dropped contract — E722 fires alongside E720.
    let err = compile_expect_error(ASYNC_MEMBER_ONLY, "c");
    assert!(
        err.contains("E722"),
        "[#111 R4] expected E722 on C + async member; got:\n{err}"
    );
}

#[test]
fn c_sync_system_still_compiles() {
    // The gate is scoped to async — plain C systems are unaffected.
    let code = compile_source(SYNC, "c");
    assert!(!code.is_empty(), "[#111 R4] a sync C system must compile");
}

#[test]
fn async_backend_unaffected() {
    // The same async system targeting an async-capable backend must compile.
    let code = compile_source(ASYNC_ATTR, "python_3");
    assert!(
        code.contains("E703") || code.contains("busy"),
        "[#111 R4] @@[async] must still emit the gate on an async backend"
    );
}
