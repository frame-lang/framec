//! Issue #157 — the Kotlin companion-object factory must NOT be annotated
//! `@JvmStatic`. That annotation is JVM-only (`kotlin.jvm.JvmStatic`), so it
//! fails to resolve on Kotlin/JS (IR), Kotlin/Native, and Kotlin/wasm
//! ("Unresolved reference 'JvmStatic'"), making the generated code
//! JVM-only. A companion function is callable as `Sys.__create(...)` on every
//! target without it — the annotation only affects Java-interop bytecode shape.
//! So framec drops it, and keeps the factory inside the `companion object`
//! (keyed by the generated `fun __create` name, not the annotation).

mod common;
use common::compile_source;

const SYNC: &str = r#"
@@[main]
@@system G(d: Int) {
    interface: f()
    machine: $S { f() { } }
    domain:
        d: Int = d
        n: Int = 0
}
"#;

const ASYNC: &str = r#"
@@[main]
@@[async]
@@system A {
    interface: async f()
    machine: $S { $>() {} f() {} }
}
"#;

/// The generated factory sits inside a `companion object` and is `fun __create`.
fn assert_factory_in_companion(code: &str) {
    assert!(
        !code.contains("@JvmStatic"),
        "[#157] generated Kotlin must not use JVM-only `@JvmStatic`:\n{code}"
    );
    let comp = code
        .find("companion object")
        .unwrap_or_else(|| panic!("[#157] no companion object in:\n{code}"));
    let create = code
        .find("fun __create")
        .unwrap_or_else(|| panic!("[#157] no `fun __create` factory in:\n{code}"));
    assert!(
        create > comp,
        "[#157] factory must live inside the companion object:\n{code}"
    );
}

#[test]
fn sync_factory_has_no_jvmstatic() {
    assert_factory_in_companion(&compile_source(SYNC, "kotlin"));
}

#[test]
fn async_factory_has_no_jvmstatic() {
    // The async casing emits its own `__create`; it must be annotation-free too.
    assert_factory_in_companion(&compile_source(ASYNC, "kotlin"));
}

#[test]
fn factory_is_callable_form() {
    // Sanity: the factory keeps the `Sys.__create(args)`-callable companion form
    // (the whole point — no behavioral change for Kotlin callers).
    let code = compile_source(SYNC, "kotlin");
    assert!(
        code.contains("fun __create(d: Int): G {"),
        "[#157] factory signature/companion form changed unexpectedly:\n{code}"
    );
}
