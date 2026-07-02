//! Issue #111 R2 — localized, intent-documented assertions over the RFC-0043
//! `@@[async]` casing/machine emission core. The R1 snapshots pin the *whole*
//! golden output per backend; these tests assert the specific per-language gate
//! idiom and the busy-flag lifecycle (set before dispatch, cleared after), so a
//! regression names the broken contract instead of showing an opaque snapshot
//! diff. All compile the shared `14_async_attribute` fixture.

mod common;
use common::compile_fixture;

/// The busy flag must be raised in the gate and lowered *after* the raise (a
/// `finally`/`defer`/RAII drop). Asserts a "lower" occurs somewhere after the
/// first "raise" — ignoring the constructor's `busy = false` initializer, which
/// legitimately precedes the gate.
fn assert_raise_before_lower(code: &str, raise: &str, lower: &str, lang: &str) {
    match code.find(raise) {
        Some(r) => assert!(
            code[r + raise.len()..].contains(lower),
            "[#111/{lang}] busy flag never lowered after `{raise}` (expected `{lower}` to follow)\n{code}"
        ),
        None => panic!("[#111/{lang}] gate never raises the busy flag (`{raise}` absent)\n{code}"),
    }
}

#[test]
fn python_try_finally_gate() {
    let c = compile_fixture("14_async_attribute", "python_3");
    assert!(c.contains("E703"), "python: E703 gate absent\n{c}");
    assert!(
        c.contains("finally:"),
        "python: try/finally clear absent\n{c}"
    );
    assert_raise_before_lower(&c, "_busy = True", "_busy = False", "python");
}

#[test]
fn typescript_try_finally_gate() {
    let c = compile_fixture("14_async_attribute", "typescript");
    assert!(c.contains("E703"), "ts: E703 gate absent\n{c}");
    assert!(c.contains("finally"), "ts: finally clear absent\n{c}");
    assert_raise_before_lower(&c, "busy = true", "busy = false", "typescript");
}

#[test]
fn javascript_try_finally_gate() {
    let c = compile_fixture("14_async_attribute", "javascript");
    assert!(c.contains("E703"), "js: E703 gate absent\n{c}");
    assert_raise_before_lower(&c, "busy = true", "busy = false", "javascript");
}

#[test]
fn kotlin_try_finally_gate() {
    let c = compile_fixture("14_async_attribute", "kotlin");
    assert!(c.contains("E703"), "kotlin: E703 gate absent\n{c}");
    assert!(c.contains("finally"), "kotlin: finally clear absent\n{c}");
    assert_raise_before_lower(&c, "busy = true", "busy = false", "kotlin");
}

#[test]
fn csharp_try_finally_gate() {
    let c = compile_fixture("14_async_attribute", "csharp");
    assert!(c.contains("E703"), "csharp: E703 gate absent\n{c}");
    assert!(c.contains("finally"), "csharp: finally clear absent\n{c}");
    assert_raise_before_lower(&c, "busy = true", "busy = false", "csharp");
}

#[test]
fn dart_try_finally_gate() {
    let c = compile_fixture("14_async_attribute", "dart");
    assert!(c.contains("E703"), "dart: E703 gate absent\n{c}");
    assert!(c.contains("finally"), "dart: finally clear absent\n{c}");
    assert_raise_before_lower(&c, "busy = true", "busy = false", "dart");
}

#[test]
fn rust_gate_guard_raii() {
    let c = compile_fixture("14_async_attribute", "rust");
    // Rust uses an RAII `_GateGuard` (clears on drop) + a typed E703 error.
    assert!(
        c.contains("_GateGuard"),
        "rust: _GateGuard RAII absent\n{c}"
    );
    assert!(
        c.contains("FrameE703Error"),
        "rust: typed E703 error absent\n{c}"
    );
    assert_raise_before_lower(&c, "busy = true", "_GateGuard", "rust");
}

#[test]
fn swift_defer_gate() {
    let c = compile_fixture("14_async_attribute", "swift");
    // Swift clears the flag in a `defer` block and throws E703.
    assert!(c.contains("defer {"), "swift: defer clear absent\n{c}");
    assert!(c.contains("E703"), "swift: E703 gate absent\n{c}");
    assert_raise_before_lower(&c, "busy = true", "defer {", "swift");
}

#[test]
fn cpp_exception_guarded_gate() {
    let c = compile_fixture("14_async_attribute", "cpp");
    // C++ uses an RAII `__E703Guard` and an `#if defined(__cpp_exceptions)` throw
    // / abort fallback (RFC-0049 -fno-exceptions safety).
    assert!(
        c.contains("__E703Guard"),
        "cpp: __E703Guard RAII absent\n{c}"
    );
    assert!(
        c.contains("#if defined(__cpp_exceptions)"),
        "cpp: -fno-exceptions fallback guard absent\n{c}"
    );
}

#[test]
fn java_failed_future_gate() {
    let c = compile_fixture("14_async_attribute", "java");
    // Java's async boundary is CompletableFuture; E703 surfaces via failedFuture.
    assert!(
        c.contains("CompletableFuture"),
        "java: CompletableFuture boundary absent\n{c}"
    );
    assert!(
        c.contains("failedFuture"),
        "java: E703 failedFuture path absent\n{c}"
    );
}

#[test]
fn gdscript_push_error_gate() {
    let c = compile_fixture("14_async_attribute", "gdscript");
    // GDScript has no exceptions: E703 surfaces via push_error + a typed-zero
    // return; the in_flight flag names the busy method.
    assert!(
        c.contains("push_error"),
        "gdscript: push_error gate absent\n{c}"
    );
    assert!(
        c.contains("in_flight"),
        "gdscript: in_flight flag absent\n{c}"
    );
    assert!(c.contains("E703"), "gdscript: E703 message absent\n{c}");
}
