//! FRAMEC_BUGS #37: bare Python-ish container type names (`list`, `dict`,
//! `set`, `tuple`) have no meaning on a statically-typed target. Frame
//! passes type strings through verbatim, so `domain: xs: list` used to emit
//! `pub xs: list` (invalid Rust). They are now rejected with **E609** + a
//! native-type hint (keeping the type-passthrough architecture intact —
//! framec does not map types). Real native types and dynamic targets are
//! unaffected.

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
    // `list` is a real type on Python — the check is static-target only.
    let _ = compile_source(
        "@@[target(\"python_3\")]\n@@system R { interface: g() machine: $A { g() {} } domain: xs: list = [] }",
        "python_3",
    );
}
