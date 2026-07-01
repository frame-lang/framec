//! Issue #147 — a Kotlin domain field whose initializer is deferred to
//! `__frame_init` (it references a system parameter, RFC-0017) and whose type is
//! a **non-null reference type** must be emitted as `lateinit var`, not with a
//! `= null` placeholder (which kotlinc rejects: "null cannot be a value of a
//! non-null type"). Primitive/constant fields keep their zero-value placeholder.

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

const REPRO: &str = r#"
@@[target("kotlin")]
class Dep(val label: String)
@@[main]
@@system G(d: Dep) {
    interface: f()
    machine: $S { f() { } }
    domain:
        d: Dep = d
        n: Int = 0
}
"#;

#[test]
fn deferred_non_null_reference_field_is_lateinit() {
    let code = compile_source(REPRO, "kotlin");
    // The reference-typed, param-deferred field: `lateinit var d: Dep`, no `= null`.
    assert!(
        code.contains("lateinit var d: Dep"),
        "[#147] deferred non-null reference field must be `lateinit var`:\n{code}"
    );
    assert!(
        !code.contains("var d: Dep = null"),
        "[#147] a `= null` placeholder on a non-null type is invalid Kotlin:\n{code}"
    );
    // The primitive keeps its zero-value placeholder (no lateinit).
    assert!(
        code.contains("var n: Int = 0"),
        "[#147] primitive field keeps its zero-value placeholder:\n{code}"
    );
    // The real assignment still lives in __frame_init.
    assert!(
        code.contains("this.d = d"),
        "[#147] __frame_init must still perform the real assignment:\n{code}"
    );
}

/// `kotlinc` must accept the generated file. Skipped (not failed) when `kotlinc`
/// is absent, mirroring the snapshot suites.
#[test]
fn generated_kotlin_compiles() {
    let bin = match find_tool("kotlinc") {
        Some(p) => p,
        None => {
            eprintln!("#147 kotlinc-check skipped: `kotlinc` not on PATH");
            return;
        }
    };
    let code = compile_source(REPRO, "kotlin");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("repro147.kt");
    std::fs::write(&src, &code).expect("write temp");
    let out = Command::new(&bin)
        .arg(&src)
        .arg("-d")
        .arg(dir.path().join("out.jar"))
        .output()
        .unwrap_or_else(|e| panic!("spawn kotlinc: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("null cannot be a value of a non-null type"),
        "[#147] kotlinc rejected the non-null-type null placeholder:\n{stderr}"
    );
    assert!(
        out.status.success(),
        "[#147] generated Kotlin rejected by kotlinc:\n{stderr}\n--- source ---\n{code}"
    );
}
