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

/// R3: a bare `@@:self.method(...)` in EXPRESSION position (`x = @@:self.g()`) is an OPERAND, not
/// a statement — it must stay INSIDE the native assignment and lower to the self-call spelling,
/// not split into `x =` (native) + `self.g()` (a standalone SelfCall) across two lines (which is a
/// Python SyntaxError). Fix: the no-field `embed_scan` arm + the `at_stmt_boundary` guard on the
/// self-call statement. Cleared test-env primary/39_self_call, demos/22_self_calibrating_sensor.
#[test]
fn r3_embedded_self_call_rhs_is_not_split() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system A {\n\
               \x20   interface:\n\
               \x20       f(): int\n\
               \x20       g(): int\n\
               \x20   machine:\n\
               \x20       $S {\n\
               \x20           f(): int {\n\
               \x20               x = @@:self.g()\n\
               \x20               @@:(x)\n\
               \x20           }\n\
               \x20           g(): int { @@:(5) }\n\
               \x20       }\n\
               }\n";
    let py = emit_py(frm);
    // One line, lowered — not split, no leaked `@@:`.
    assert!(
        py.contains("        x = self.g()\n"),
        "R3: embedded self-call must lower inline to `x = self.g()`:\n{py}"
    );
    assert!(!py.contains("@@:"), "R3: no `@@:` may leak:\n{py}");
    assert!(
        !py.contains("        x =\n"),
        "R3: the assignment must not be split across lines:\n{py}"
    );
}

/// R4a: a self-call's ARGUMENTS are tokenized and lowered — `@@:self.echo($.val)` must emit
/// `self.echo(compartment.state_vars["val"])`, not ship `$.val` verbatim. Fix: `SelfCallStmt.args`
/// is `ArgExpr` (tokenized), rendered through `reindent::render_args`. Cleared the arg-lowering
/// half of primary/55_nested_frame_args (patterns p2/p5).
#[test]
fn r4a_self_call_args_are_lowered() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system C {\n\
               \x20   interface:\n\
               \x20       f(): int\n\
               \x20       echo(n: int): int\n\
               \x20   machine:\n\
               \x20       $A {\n\
               \x20           $.val: int = 0\n\
               \x20           f(): int {\n\
               \x20               $.val = 7\n\
               \x20               @@:self.echo($.val)\n\
               \x20               @@:($.val)\n\
               \x20           }\n\
               \x20           echo(n: int): int { @@:(n) }\n\
               \x20       }\n\
               }\n";
    let py = emit_py(frm);
    assert!(
        py.contains("self.echo(compartment.state_vars[\"val\"])"),
        "R4a: self-call arg `$.val` must lower to the state-var read:\n{py}"
    );
    assert!(!py.contains("$."), "R4a: no `$.` may leak:\n{py}");
    assert!(!py.contains("@@:"), "R4a: no `@@:` may leak:\n{py}");
}

/// R4b: a transition's ENTER args are tokenized and lowered — `-> ($.v) $B` must deliver
/// `self._B__enter(compartment.state_vars["v"])`, not ship `$.v` verbatim. Fix: the transition
/// arg fields are `Option<ArgExpr>` (tokenized via `parse_after_arrow` spans), rendered through
/// `reindent::render_args`. Cleared demos/27_deployment_pipeline (`-> ($.version) $Live`).
#[test]
fn r4b_transition_enter_args_are_lowered() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system C2 {\n\
               \x20   interface:\n\
               \x20       go()\n\
               \x20   machine:\n\
               \x20       $A {\n\
               \x20           $.v: str = \"x\"\n\
               \x20           go() {\n\
               \x20               -> ($.v) $B\n\
               \x20           }\n\
               \x20       }\n\
               \x20       $B {\n\
               \x20           $.v: str = \"\"\n\
               \x20           $>(v: str) {\n\
               \x20               $.v = v\n\
               \x20           }\n\
               \x20       }\n\
               }\n";
    let py = emit_py(frm);
    assert!(
        py.contains("self._B__enter(compartment.state_vars[\"v\"])"),
        "R4b: transition enter arg `$.v` must lower to the state-var read:\n{py}"
    );
    assert!(!py.contains("$."), "R4b: no `$.` may leak:\n{py}");
}

/// R4c (STOPPED — surfaced separately, not fixed here): `@@:return` as a GETTER in expression/arg
/// position (`@@:self.echo(@@:return)`). The cleanroom Python runtime has NO return SLOT — a
/// value-returning handler emits `return <expr>` immediately, and `@@:return = x` EXITS rather
/// than storing. So a `@@:return` getter has nothing to read, and `lower_ref`'s `ContextReturn`
/// arm degrades to the Python `return` keyword (`self.echo(return)` — a SyntaxError). A correct
/// fix requires introducing a persisted return slot into the generated runtime (RFC-0053 /
/// frame_runtime.md FrameContext `_return`), a foundational change beyond R3/R4(a,b). This pin
/// records the requirement; un-ignore it when the return slot lands.
#[test]
#[ignore = "R4c: needs a runtime return slot (getter + set-without-exit); surfaced separately"]
fn r4c_return_getter_in_arg_position() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system R {\n\
               \x20   interface:\n\
               \x20       p(x: int): int\n\
               \x20       echo(n: int): int\n\
               \x20   machine:\n\
               \x20       $A {\n\
               \x20           p(x: int): int {\n\
               \x20               @@:return = x\n\
               \x20               @@:self.echo(@@:return)\n\
               \x20               @@:(@@:return)\n\
               \x20           }\n\
               \x20           echo(n: int): int { @@:(n) }\n\
               \x20       }\n\
               }\n";
    let py = emit_py(frm);
    // The eventual correct lowering reads the return SLOT, never the bare `return` keyword.
    assert!(
        !py.contains("self.echo(return)"),
        "R4c: `@@:return` getter must not lower to the `return` keyword:\n{py}"
    );
}

/// R2: a MULTI-LINE `@@:(<expr>)` return must be wrapped in parens so Python's implicit
/// line continuation makes the continuation lines legal — without them the second line is
/// `IndentationError: unexpected indent`. The `@@:(` … `)` source syntax already implied
/// the parens; framec supplies them back. Detected where the SOURCE + span live
/// (`Source::span_is_multiline`), never by inspecting the opaque native text.
/// Cleared test-env primary/92_return_expr_multiline.
#[test]
fn r2_multiline_return_is_wrapped_in_parens() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system T {\n\
               \x20   interface:\n\
               \x20       ready(): bool\n\
               \x20   machine:\n\
               \x20       $Active {\n\
               \x20           ready(): bool {\n\
               \x20               @@:(a\n\
               \x20                   and b)\n\
               \x20           }\n\
               \x20       }\n\
               }\n";
    let py = emit_py(frm);
    // The multi-line expr is wrapped: `return (a\n … and b)`.
    assert!(
        py.contains("return (a"),
        "R2: a multi-line return must open with `return (`:\n{py}"
    );
    assert!(
        py.contains("and b)\n"),
        "R2: a multi-line return must close the wrapping paren after the expr:\n{py}"
    );
}

/// R2 (no-churn guard): a SINGLE-line `@@:(<expr>)` must stay `return <expr>` — NO wrapping
/// parens. The multiline detector must not fire on one-liners, or every existing single-line
/// return snapshot would churn.
#[test]
fn r2_single_line_return_is_not_wrapped() {
    let frm = "@@[target(\"python_3\")]\n\
               @@system T {\n\
               \x20   interface:\n\
               \x20       ready(): bool\n\
               \x20   machine:\n\
               \x20       $Active {\n\
               \x20           ready(): bool {\n\
               \x20               @@:(self.flag)\n\
               \x20           }\n\
               \x20       }\n\
               \x20   domain:\n\
               \x20       flag: bool = True\n\
               }\n";
    let py = emit_py(frm);
    assert!(
        py.contains("return self.flag\n"),
        "R2: a single-line return must stay bare `return <expr>`:\n{py}"
    );
    assert!(
        !py.contains("return (self.flag"),
        "R2: a single-line return must NOT be wrapped in parens:\n{py}"
    );
}
