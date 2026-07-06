//! RFC-0043.1 — `@@[async(casing: false)]` optional casing/gate.
//!
//! `@@[async]` (and the explicit `@@[async(casing: true)]`) keep the RFC-0043
//! layered casing/machine with the E703 single-driver gate. `@@[async(casing:
//! false)]` emits the flat single-class async form — the dispatch core + `init()`
//! but no casing, no gate, no `_<Name>Machine`. A malformed casing arg is E724.

mod common;
use common::{compile_expect_error, compile_source};

fn gated(casing: Option<bool>) -> String {
    let attr = match casing {
        None => "@@[async]".to_string(),
        Some(b) => format!("@@[async(casing: {b})]"),
    };
    format!(
        r#"
@@[target("python_3")]
@@[main]
{attr}
@@system Counter (dep) {{
    interface: async tick()
    machine: $Running {{ tick() {{}} }}
    domain: dep = dep
}}
"#
    )
}

/// `@@[async]` default: layered casing + private machine + E703 gate.
#[test]
fn default_async_is_gated_and_layered() {
    let code = compile_source(&gated(None), "python");
    assert!(
        code.contains("class Counter:"),
        "casing class present:\n{code}"
    );
    assert!(
        code.contains("class _CounterMachine:"),
        "private machine present:\n{code}"
    );
    assert!(code.contains("self._busy"), "busy gate present:\n{code}");
    assert!(code.contains("E703"), "E703 gate present:\n{code}");
}

/// `@@[async(casing: true)]` is byte-identical to `@@[async]`.
#[test]
fn casing_true_equals_bare_async() {
    assert_eq!(
        compile_source(&gated(Some(true)), "python"),
        compile_source(&gated(None), "python"),
        "[43.1] `casing: true` must equal `@@[async]`"
    );
}

/// `@@[async(casing: false)]`: flat single class — no casing, gate, or machine.
#[test]
fn casing_false_is_flat_ungated() {
    let code = compile_source(&gated(Some(false)), "python");
    assert!(
        code.contains("class Counter:"),
        "single public class present:\n{code}"
    );
    assert!(
        !code.contains("_CounterMachine"),
        "[43.1] flat form must NOT emit the machine split:\n{code}"
    );
    assert!(
        !code.contains("self._busy") && !code.contains("E703"),
        "[43.1] flat form must NOT emit the busy/E703 gate:\n{code}"
    );
    // The async dispatch core is kept: interface method + init boundary.
    assert!(
        code.contains("async def tick(") && code.contains("async def init("),
        "[43.1] flat form must keep the async interface and init():\n{code}"
    );
    // Params reach the single class directly (no casing->machine hop).
    assert!(
        code.contains("def _create(cls, dep"),
        "[43.1] flat factory must accept the param directly:\n{code}"
    );
}

/// Rust's separate pipeline honors the flag too.
#[test]
fn casing_false_flat_on_rust() {
    let code = compile_source(
        r#"
@@[target("rust")]
@@[main]
@@[async(casing: false)]
@@system Counter (dep: Dep) {
    interface: async tick()
    machine: $Running { tick() {} }
    domain: dep: Dep = dep
}
"#,
        "rust",
    );
    assert!(
        !code.contains("_CounterMachine") && !code.contains("busy"),
        "[43.1] Rust flat form must not emit the machine/gate:\n{code}"
    );
    assert!(
        code.contains("pub struct Counter") && code.contains("pub async fn init"),
        "[43.1] Rust flat form is a single struct with init():\n{code}"
    );
}

/// A malformed `casing:` argument is E724.
#[test]
fn malformed_casing_arg_is_e724() {
    for bad in [
        "casing: yes", // non-bool value
        "gate: false", // unknown key
        "casing",      // no value
        "false",       // positional / malformed
    ] {
        let src = format!(
            r#"
@@[target("python_3")]
@@[main]
@@[async({bad})]
@@system K {{ interface: async t() machine: $S {{ t() {{}} }} }}
"#
        );
        let err = compile_expect_error(&src, "python");
        assert!(
            err.contains("E724"),
            "[43.1] `@@[async({bad})]` must be E724; got:\n{err}"
        );
    }
}
