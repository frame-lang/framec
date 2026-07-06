//! Issue #167 — the `@@[async]` casing must forward the system's constructor
//! params to the machine's factory.
//!
//! For a layered async system, the public casing constructs the private
//! machine. It used to build a bare `_<Name>Machine()` and expose a
//! param-less factory, so an async system with a domain field wired from a
//! constructor param (`dep = dep`) was unconstructable via its public class —
//! the machine's own factory was correct but never called with the args. The
//! casing now takes the params and forwards them to the machine factory (via
//! the same bare/factory split #123 gives domain fields: the bare shell builds
//! a param-less machine, the factory forwards the params).

mod common;
use common::compile_source;

/// Python: the casing `_create` classmethod must accept `dep` and forward it to
/// `_CounterMachine._create(dep)`; the bare `__init__` builds a param-less
/// machine.
#[test]
fn python_casing_forwards_params_to_machine() {
    let code = compile_source(
        r#"
@@[target("python_3")]
@@[async]
@@system Counter (dep) {
    interface: async tick()
    machine: $Running { tick() {} }
    domain: dep = dep
}
"#,
        "python",
    );
    assert!(
        code.contains("def _create(cls, dep"),
        "[#167] casing factory must accept the system param:\n{code}"
    );
    assert!(
        code.contains("_CounterMachine._create(dep)"),
        "[#167] casing factory must forward the param to the machine factory:\n{code}"
    );
    // The bare ctor builds a param-less machine (usable shell).
    assert!(
        code.contains("self._machine = _CounterMachine()"),
        "[#167] bare casing ctor must build a param-less machine:\n{code}"
    );
}

/// C# (typed): the casing `__create(Host dep)` forwards to
/// `_CounterMachine.__create(dep)`, terminated with a semicolon.
#[test]
fn csharp_casing_forwards_params_with_terminator() {
    let code = compile_source(
        r#"
@@[target("csharp")]
@@[async]
@@system Counter (dep: Host) {
    interface: async tick()
    machine: $Running { tick() {} }
    domain: dep: Host = dep
}
"#,
        "csharp",
    );
    assert!(
        code.contains("__create(Host dep)"),
        "[#167] casing factory must accept the typed param:\n{code}"
    );
    assert!(
        code.contains("this.machine = _CounterMachine.__create(dep);"),
        "[#167] casing factory must forward the param (with terminator):\n{code}"
    );
    assert!(
        code.contains("this.machine = new _CounterMachine();"),
        "[#167] bare casing ctor must build a param-less machine (with terminator):\n{code}"
    );
}

/// Rust: its separate casing pipeline has the same requirement. A param-carrying
/// machine has no `new()` (RFC-0020/#123), so the casing skips `new()` too and
/// builds the struct directly in `__create`, forwarding the param.
#[test]
fn rust_casing_forwards_params_and_skips_new() {
    let code = compile_source(
        r#"
@@[target("rust")]
@@[async]
@@system Counter (dep: Host) {
    interface: async tick()
    machine: $Running { tick() {} }
    domain: dep: Host = dep
}
"#,
        "rust",
    );
    assert!(
        code.contains("pub fn __create(dep: Host) -> Self"),
        "[#167] Rust casing factory must accept the typed param:\n{code}"
    );
    assert!(
        code.contains("machine: _CounterMachine::__create(dep)"),
        "[#167] Rust casing must forward the param to the machine factory:\n{code}"
    );
}

/// A no-param async system is unchanged: the casing has a param-less factory
/// and builds the machine bare.
#[test]
fn no_param_async_casing_unchanged() {
    let code = compile_source(
        r#"
@@[target("python_3")]
@@[async]
@@system Plain {
    interface: async tick()
    machine: $Running { tick() {} }
    domain: count: int = 0
}
"#,
        "python",
    );
    assert!(
        code.contains("self._machine = _PlainMachine()"),
        "[#167] no-param async casing must still build the machine bare:\n{code}"
    );
    assert!(
        !code.contains("_PlainMachine._create("),
        "[#167] no-param async casing must not call the machine factory:\n{code}"
    );
}
