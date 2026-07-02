//! Erlang target deprecation (4.6.1): compiling `-l erlang` emits **W901** —
//! deprecated but NOT removed. The handler-body lowering is slated for
//! redesign (#119/#125); existing Erlang systems keep compiling and running.

mod common;
use common::compile_with_warnings;

const SRC: &str = r#"
@@[main]
@@system G {
    interface: f()
    machine: $S { f() {} }
}
"#;

#[test]
fn erlang_emits_w901_and_still_compiles() {
    let (code, warnings) = compile_with_warnings(SRC, "erlang");
    assert!(
        warnings.iter().any(|w| w.starts_with("W901")),
        "[W901] erlang compile must warn deprecated; got: {warnings:?}"
    );
    // NOT removed — output still generated.
    assert!(
        code.contains("gen_statem"),
        "[W901] erlang output must still be generated:\n{code}"
    );
}

#[test]
fn other_targets_do_not_warn_w901() {
    let (_, warnings) = compile_with_warnings(SRC, "python_3");
    assert!(
        !warnings.iter().any(|w| w.starts_with("W901")),
        "[W901] must be erlang-only; python got: {warnings:?}"
    );
}
