//! RFC-0045 — reserved `@@:system` / `@@:system.state` diagnostics.
//!
//! `@@:system` requires a member access → **E604**.
//! `@@:system.state` is reserved (use `.name`) → **E608**.
//!
//! Regression for the expression-context gap: these forms were only
//! caught in statement position. Nested inside a return expression
//! (`@@:(…)`, `@@:return(…)`, `@@:return = …`) they slipped past the
//! validator and codegen emitted a `/* ERROR: bare @@:system */`
//! placeholder into the output. The validator now re-scans
//! return-expression text, so both contexts are rejected.

mod common;

use common::{compile_expect_error, compile_source};

fn sys_stmt(body: &str) -> String {
    format!("@@system S {{\n    interface: c()\n    machine: $A {{ c() {{ {body} }} }}\n}}")
}

fn sys_expr(ret_stmt: &str) -> String {
    // A value-returning handler whose body is a single return statement.
    format!(
        "@@system S {{\n    interface: c(): str\n    machine: $A {{ c(): str {{ {ret_stmt} }} }}\n}}"
    )
}

// ── statement position (the path that always worked) ──────────────────

#[test]
fn e604_bare_system_statement() {
    let err = compile_expect_error(&sys_stmt("@@:system"), "python_3");
    assert!(err.contains("E604"), "expected E604, got:\n{err}");
}

#[test]
fn e608_reserved_state_statement() {
    let err = compile_expect_error(&sys_stmt("@@:system.state"), "python_3");
    assert!(err.contains("E608"), "expected E608, got:\n{err}");
}

// ── expression position (the gap this fix closes) ─────────────────────

#[test]
fn e604_bare_system_in_return_expr() {
    let err = compile_expect_error(&sys_expr("@@:(@@:system)"), "python_3");
    assert!(err.contains("E604"), "expected E604, got:\n{err}");
}

#[test]
fn e608_reserved_state_in_return_expr() {
    let err = compile_expect_error(&sys_expr("@@:(@@:system.state)"), "python_3");
    assert!(err.contains("E608"), "expected E608, got:\n{err}");
}

#[test]
fn e608_reserved_state_in_return_call() {
    // `@@:return(expr)` form.
    let err = compile_expect_error(&sys_expr("@@:return(@@:system.state)"), "python_3");
    assert!(err.contains("E608"), "expected E608, got:\n{err}");
}

#[test]
fn e604_bare_system_in_return_assign() {
    // `@@:return = expr` form.
    let err = compile_expect_error(&sys_expr("@@:return = @@:system"), "python_3");
    assert!(err.contains("E604"), "expected E604, got:\n{err}");
}

// ── cross-scanner: the validator re-scans with the target's own scanner ──

#[test]
fn e608_in_return_expr_rust_scanner() {
    let err = compile_expect_error(&sys_expr("@@:(@@:system.state)"), "rust");
    assert!(err.contains("E608"), "expected E608 (rust), got:\n{err}");
}

// ── the valid accessor must still compile in every position ───────────

#[test]
fn valid_state_name_accessor_compiles() {
    // statement and expression positions both fine.
    let _ = compile_source(&sys_stmt("@@:system.state.name"), "python_3");
    let out = compile_source(&sys_expr("@@:(@@:system.state.name)"), "python_3");
    assert!(
        !out.contains("/* ERROR"),
        "valid accessor must not emit an error placeholder:\n{out}"
    );
}
