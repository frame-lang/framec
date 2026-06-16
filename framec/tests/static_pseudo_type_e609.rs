//! FRAMEC_BUGS #37 (Rust-only): the Rust backend does not map the bare
//! container type names `list`/`dict`/`set`/`tuple` to a real type — Frame
//! passes type strings through verbatim, so `domain: xs: list` emitted
//! `pub xs: list` (invalid Rust). On Rust they are now rejected with
//! **E609** + a native-type hint (keeping type-passthrough intact — framec
//! does not map types).
//!
//! These names ARE supported on C (and dynamic targets) via the runtime's
//! list/dict helpers, so the rejection is **Rust-scoped**: real native
//! types, C's bare `list`/`dict`, and dynamic targets are all unaffected.

mod common;
use common::{compile_expect_error, compile_source};

#[test]
fn rust_domain_pseudo_type_list_rejected() {
    let err = compile_expect_error(
        "@@system R { interface: g() machine: $A { g() {} } domain: xs: list = [] }",
        "rust",
    );
    assert!(err.contains("E609"), "expected E609, got:\n{err}");
}

#[test]
fn rust_param_pseudo_type_dict_rejected() {
    let err = compile_expect_error(
        "@@system R { interface: g(m: dict) machine: $A { g(m: dict) {} } }",
        "rust",
    );
    assert!(err.contains("E609"), "expected E609, got:\n{err}");
}

#[test]
fn rust_real_container_type_accepted() {
    // A real native type (capitalized / generic) must NOT be flagged.
    let _ = compile_source(
        r#"@@system R { interface: g() machine: $A { g() {} } domain: xs: "Vec<i64>" = vec![] }"#,
        "rust",
    );
}

#[test]
fn dynamic_target_pseudo_type_allowed() {
    // `list` is a real type on Python — the check is Rust-only.
    let _ = compile_source(
        "@@[target(\"python_3\")]\n@@system R { interface: g() machine: $A { g() {} } domain: xs: list = [] }",
        "python_3",
    );
}

#[test]
fn c_target_list_dict_allowed() {
    // C supports `list`/`dict` as domain types via the runtime's
    // list/dict helpers — they must NOT be rejected (#37 is Rust-only).
    let _ = compile_source(
        "@@[target(\"c\")]\n@@system R { interface: g(m: dict) machine: $A { g(m: dict) {} } domain: xs: list = 0 }",
        "c",
    );
}
