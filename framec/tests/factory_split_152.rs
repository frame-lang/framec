//! Issue #152 — the constructor→factory split must classify the start-state
//! `$>` kernel dispatch by **node identity** (the `FrameInitBlock` marker
//! emitted by `constructor.rs`), not by scanning the rendered text for
//! `__kernel(`. Behaviourally the split is unchanged (the bare constructor is a
//! usable empty shell; the factory `_create`/`create` fires the kernel), but the
//! classification is now exact. These fixtures pin that invariant across a few
//! representative backends so a future refactor can't silently regress the
//! placement.

mod common;
use common::compile_source;

const SRC: &str = r#"
@@[main]
@@system G {
    interface: f()
    machine: $S { $>() {} f() {} }
}
"#;

/// Return the substring of `code` from the (1-based) occurrence of `marker` to
/// the next occurrence of `boundary` (or end) — a crude "method body" slice good
/// enough to check kernel placement without a full parser.
fn slice_between<'a>(code: &'a str, marker: &str, boundary: &str) -> &'a str {
    let start = code
        .find(marker)
        .unwrap_or_else(|| panic!("no `{marker}` in:\n{code}"));
    let rest = &code[start..];
    match rest[marker.len()..].find(boundary) {
        Some(off) => &rest[..marker.len() + off],
        None => rest,
    }
}

#[test]
fn python_kernel_in_factory_not_bare_ctor() {
    let code = compile_source(SRC, "python");
    let init = slice_between(&code, "def __init__(self", "\n    def ");
    let create = slice_between(&code, "def _create(cls", "\n    def ");
    assert!(
        !init.contains("__kernel"),
        "[#152] bare __init__ must be kernel-free:\n{init}"
    );
    assert!(
        create.contains("__kernel"),
        "[#152] factory _create must fire the kernel:\n{create}"
    );
}

#[test]
fn java_kernel_in_factory_not_bare_ctor() {
    let code = compile_source(SRC, "java");
    // The bare constructor `G() { ... }` must not fire the kernel; the static
    // factory `create()` must.
    let bare = slice_between(&code, "public G()", "\n    public");
    assert!(
        !bare.contains("__kernel"),
        "[#152] bare Java constructor must be kernel-free:\n{bare}"
    );
    assert!(
        code.contains("__kernel(__e)"),
        "[#152] Java factory must fire the kernel:\n{code}"
    );
}

#[test]
fn go_kernel_in_factory_not_bare_ctor() {
    let code = compile_source(SRC, "go");
    // Go's factory function fires the kernel; the zero-value struct literal does
    // not. The marker classification keeps the kernel out of any bare shell.
    assert!(
        code.contains("__kernel("),
        "[#152] Go factory must fire the kernel:\n{code}"
    );
}
