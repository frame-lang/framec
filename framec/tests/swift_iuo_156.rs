//! Issue #156 — a Swift domain field whose initializer is deferred to
//! `__frame_init` (it references a system parameter, RFC-0017) and whose type is
//! a reference/protocol type must be emitted as an **implicitly-unwrapped
//! optional** (`Dep!`), not an uninitialized non-optional (`Dep`) which fails
//! Swift's definite-initialization rule. Primitive fields keep their zero-value
//! placeholder; private framework fields are unaffected (assigned in `init()`).
//! Same family as #147 (Kotlin `lateinit var`).

mod common;
use common::{compile_source, find_tool};
use std::process::Command;

const REPRO: &str = r#"
@@[target("swift")]
public class Dep {
    public var label: String
    public init(_ label: String) { self.label = label }
}
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
fn deferred_reference_field_is_iuo() {
    let code = compile_source(REPRO, "swift");
    assert!(
        code.contains("var d: Dep!"),
        "[#156] deferred reference field must be an implicitly-unwrapped optional:\n{code}"
    );
    // Primitive keeps its zero-value placeholder.
    assert!(
        code.contains("var n: Int = 0"),
        "[#156] primitive field keeps its zero-value placeholder:\n{code}"
    );
    // A private framework field is assigned in init() and must stay non-optional
    // (no stray `!`) — the fix is scoped to public domain fields.
    assert!(
        code.contains("var __compartment: GCompartment\n")
            || code.contains("var __compartment: GCompartment "),
        "[#156] private framework field must not become IUO:\n{code}"
    );
    // The real assignment still lives in __frame_init.
    assert!(
        code.contains("self.d = d"),
        "[#156] __frame_init must still perform the real assignment:\n{code}"
    );
}

/// `swiftc` must accept the generated file (definite-init satisfied). Skipped
/// when `swiftc` is absent.
#[test]
fn generated_swift_compiles() {
    let bin = match find_tool("swiftc") {
        Some(p) => p,
        None => {
            eprintln!("#156 swiftc-check skipped: `swiftc` not on PATH");
            return;
        }
    };
    let code = compile_source(REPRO, "swift");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("repro156.swift");
    std::fs::write(&src, &code).expect("write temp");
    let out = Command::new(&bin)
        .arg(&src)
        .arg("-o")
        .arg(dir.path().join("run"))
        .output()
        .unwrap_or_else(|e| panic!("spawn swiftc: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("without initializing all stored properties"),
        "[#156] swiftc definite-init error (uninitialized stored property):\n{stderr}"
    );
    assert!(
        out.status.success(),
        "[#156] generated Swift rejected by swiftc:\n{stderr}\n--- source ---\n{code}"
    );
}
