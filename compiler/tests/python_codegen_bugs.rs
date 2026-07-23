//! Regression pins for the Python codegen bugs surfaced by running framec-ng over the
//! test-env positive corpus (9 fixtures whose emitted Python failed `python3 -m py_compile`).
//! Each test asserts the CORRECTED emitted Python; the corresponding fix made the real
//! test-env fixture py_compile-clean. Minimal repros are the ones the diagnosis confirmed.
use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, python::Python};
use frame_compiler::Source;

fn emit_py(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).expect("segment");
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "resolve diagnostics: {diags:#?}");
    driver::emit(&src, &ast, &syms, &Python)
}

/// R1: a `$.x = …` (state-var / `@@:data.x =` / `@@:return =`) nested in a native block must honor
/// the block's indent (`pad(rel)`), not the hardcoded 8-space method base — else Python raises
/// IndentationError. python.rs `assign()` StateVar/ContextData/ContextReturn arms. Cleared the
/// test-env fixtures demos/28_auth_flow, demos/29_game_level, max_coverage/max_python_3.
#[test]
fn r1_statevar_assign_respects_nested_indent() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system B {\n\
               \x20   interface:\n\
               \x20       f(): int\n\
               \x20   machine:\n\
               \x20       $S {\n\
               \x20           $.n: int = 0\n\
               \x20           f(): int {\n\
               \x20               if True:\n\
               \x20                   $.n = 1\n\
               \x20               @@:($.n)\n\
               \x20           }\n\
               \x20       }\n\
               }\n";
    let py = emit_py(frm);
    // `$.n = 1` sits inside `if True:` — it must be at 12 spaces, not the 8-space method base.
    assert!(
        py.contains("            compartment.state_vars[\"n\"] = 1"),
        "R1: state-var assign nested in `if True:` must indent to 12 spaces:\n{py}"
    );
    // and it must NOT land at the broken 8-space column (the old hardcoded prefix).
    assert!(
        !py.contains("\n        compartment.state_vars[\"n\"] = 1\n"),
        "R1: the assign must not land at the 8-space method base:\n{py}"
    );
}
