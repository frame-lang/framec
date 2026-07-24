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

/// R4b: a transition's ENTER args are tokenized and lowered — `-> ($.v) $B` must deliver the
/// state-var READ, not ship `$.v` verbatim. Fix: the transition arg fields are `Option<ArgExpr>`
/// (tokenized via `parse_after_arrow` spans), rendered through `reindent::render_args`. Cleared
/// demos/27_deployment_pipeline (`-> ($.version) $Live`).
///
/// SPELLING MIGRATED (M1, faithful emit). The delivery mechanism changed and the pin follows it:
/// a handler no longer CALLS the destination's enter handler, it builds the destination
/// compartment with the enter payload on board (`__prepareEnter(leaf, state_args, enter_args)`)
/// and queues it; the kernel synthesizes the `$>` from that compartment. The bug this pins —
/// "the arg is lowered, not shipped verbatim" — is unchanged, and is still what is asserted.
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
        py.contains("self.__prepareEnter(\"B\", [], [compartment.state_vars[\"v\"]])"),
        "R4b: transition enter arg `$.v` must lower to the state-var read, and ride on the \
         destination compartment:\n{py}"
    );
    // And the destination's `$>` must read it back off that compartment, positionally.
    assert!(
        py.contains("v = compartment.enter_args[0]"),
        "R4b: the enter handler must bind its param from the compartment's enter args:\n{py}"
    );
    assert!(!py.contains("$."), "R4b: no `$.` may leak:\n{py}");
}

/// R4c (FIXED by the M1 faithful-emit model — was `#[ignore]`d): `@@:return` as a GETTER in
/// expression/arg position (`@@:self.echo(@@:return)`).
///
/// The old cleanroom Python runtime had NO return SLOT — a value-returning handler emitted
/// `return <expr>` immediately, and `@@:return = x` EXITED rather than storing. So the getter had
/// nothing to read and `lower_ref`'s `ContextReturn` arm degraded to the Python `return` KEYWORD,
/// producing `self.echo(return)` — a SyntaxError. The pin recorded the requirement: "un-ignore it
/// when the return slot lands."
///
/// It has landed. `FrameContext._return` is the slot, the context stack makes it re-entrant (a
/// `@@:self.other()` inside a handler pushes its own context, so the outer call's answer is not
/// clobbered), and BOTH directions now spell the same place: `@@:return = x` assigns it and
/// `@@:return` reads it.
#[test]
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
    // The correct lowering reads the return SLOT, never the bare `return` keyword.
    assert!(
        !py.contains("self.echo(return)"),
        "R4c: `@@:return` getter must not lower to the `return` keyword:\n{py}"
    );
    assert!(
        py.contains("self.echo(self._context_stack[-1]._return)"),
        "R4c: `@@:return` in ARG position must read the context's return slot:\n{py}"
    );
    assert!(
        py.contains("self._context_stack[-1]._return = x"),
        "R4c: `@@:return = x` must SET the slot (and not exit):\n{py}"
    );
    // And the emitted file is valid Python — the whole point of the pin.
    assert!(!py.contains("(return"), "R4c: no bare `return` keyword in expression position:\n{py}");
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
    // The multi-line expr is wrapped. SPELLING MIGRATED (M1): `@@:(expr)` sets the CONTEXT's return
    // slot rather than emitting a `return` keyword — the handler is called from the kernel, which
    // still has a transition drain to run. The paren rule this pins is unchanged.
    assert!(
        py.contains("self._context_stack[-1]._return = (a"),
        "R2: a multi-line return must open its assignment with `(`:\n{py}"
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
    // SPELLING MIGRATED (M1) — the return-slot assignment, unwrapped.
    assert!(
        py.contains("self._context_stack[-1]._return = self.flag\n"),
        "R2: a single-line return must stay a bare assignment:\n{py}"
    );
    assert!(
        !py.contains("._return = (self.flag"),
        "R2: a single-line return must NOT be wrapped in parens:\n{py}"
    );
}

/// **M1 GOLDEN — framec-ng's Python emit for the minimal system is BYTE-IDENTICAL to the
/// shipped compiler's.**
///
/// This is the Milestone-1 gate of the faithful-emit rebuild, and it is a *byte* gate on
/// purpose: the census's central finding is that a misunderstanding and its symptom are three
/// stages and a foreign toolchain apart, so the only honest check is the emitted file itself,
/// compared whole. `MIN_PY` is the 4.6.1 oracle's output for `MIN_FRM`
/// (`framec compile -l python_3`), pasted verbatim — not a transcription of what this
/// compiler happens to produce.
///
/// What it pins, top to bottom: the `typing` preamble; the three system-prefixed runtime
/// classes (`MinFrameEvent` / `MinFrameContext` / `MinCompartment` + `copy`); `__init__`'s
/// order (`_state_stack`, `_context_stack`, DOMAIN INIT, `__prepareEnter`,
/// `__next_compartment`); the `_create` factory; `_HSM_CHAIN` + `__prepareEnter` /
/// `__prepareExit` / `__kernel` (with the transition-drain loop) / `__router` / `__transition`;
/// the per-event public wrapper (FrameEvent + context stack + `try/finally` + the `_return`
/// slot, and `val`'s `-> int` annotation); the per-state `_state_X` dispatchers; and the
/// private `_s_<state>_hdl_user_<event>` handlers.
#[test]
fn m1_min_system_python_is_byte_identical_to_the_shipped_compiler() {
    let got = emit_py(MIN_FRM);
    assert_eq!(
        got, MIN_PY,
        "M1 byte gate: framec-ng's Python emit diverged from the 4.6.1 oracle.\n\
         --- got ---\n{got}\n--- want ---\n{MIN_PY}"
    );
}

/// The M1 fixture: one system, two states, a domain field, a `@@:self.x` assign, a bare
/// transition, and a value-returning handler. Small enough to read whole; wide enough that
/// every phase of the emitter (preamble, runtime classes, ctor, factory, kernel, router,
/// interface wrapper, state dispatch, handlers) is exercised at least once.
const MIN_FRM: &str = r#"@@system Min {
    interface:
        go()
        val(): int

    machine:
        $A {
            go() {
                @@:self.x = 1
                -> $B
            }
            val(): int {
                @@:(5)
            }
        }

        $B {
        }

    domain:
        x: int = 0
}
"#;

/// The 4.6.1 oracle's Python emission for [`MIN_FRM`], verbatim.
const MIN_PY: &str = r#"from typing import Any, Optional, List, Dict, Callable

class MinFrameEvent:
    def __init__(self, message: str, parameters):
        self._message = message
        self._parameters = parameters


class MinFrameContext:
    def __init__(self, event: MinFrameEvent, default_return):
        self.event = event
        self._return = default_return
        self._data = {}
        self._transitioned = False


class MinCompartment:
    def __init__(self, state: str, parent_compartment = None):
        self.state = state
        self.state_args = []
        self.state_vars = {}
        self.enter_args = []
        self.exit_args = []
        self.forward_event = None
        self.parent_compartment = parent_compartment

    def copy(self) -> 'MinCompartment':
        c = MinCompartment(self.state, self.parent_compartment)
        c.state_args = self.state_args.copy()
        c.state_vars = self.state_vars.copy()
        c.enter_args = self.enter_args.copy()
        c.exit_args = self.exit_args.copy()
        c.forward_event = self.forward_event
        return c


class Min:
    def __init__(self):
        self._state_stack = []
        self._context_stack = []
        self.x = 0
        self.__compartment = self.__prepareEnter("A", [], [])
        self.__next_compartment = None

    @classmethod
    def _create(cls):
        c = cls()
        __e = MinFrameEvent("$>", c.__compartment.enter_args)
        __ctx = MinFrameContext(__e, None)
        c._context_stack.append(__ctx)
        c.__kernel(__e)
        c._context_stack.pop()
        return c

    _HSM_CHAIN = {
        "A": ["A"],
        "B": ["B"],
    }
    def __prepareEnter(self, leaf, state_args, enter_args):
        comp = None
        for name in self._HSM_CHAIN[leaf]:
            new_comp = MinCompartment(name)
            new_comp.state_args = list(state_args)
            new_comp.enter_args = list(enter_args)
            new_comp.parent_compartment = comp
            comp = new_comp
        return comp

    def __prepareExit(self, exit_args):
        comp = self.__compartment
        while comp is not None:
            comp.exit_args = list(exit_args)
            comp = comp.parent_compartment

    def __kernel(self, __e):
        # Route event to current state
        self.__router(__e)
        # Drain any transitions queued by the handler
        while self.__next_compartment is not None:
            next_compartment = self.__next_compartment
            self.__next_compartment = None
            # Exit the current (leaf) state
            self.__router(MinFrameEvent("<$", self.__compartment.exit_args))
            # Switch to the new compartment
            self.__compartment = next_compartment
            if next_compartment.forward_event is None:
                # No forwarded event — synthesize a fresh $>
                self.__router(MinFrameEvent("$>", self.__compartment.enter_args))
            else:
                if next_compartment.forward_event._message == "$>":
                    # Forwarded event IS $> — dispatch directly so the
                    # destination's $> receives the caller's payload
                    self.__router(next_compartment.forward_event)
                else:
                    # Forwarded event is not $> — initialize the destination
                    # with a fresh $>, then dispatch the forward to it
                    self.__router(MinFrameEvent("$>", self.__compartment.enter_args))
                    self.__router(next_compartment.forward_event)
            next_compartment.forward_event = None
            # Mark all stacked contexts as transitioned
            for ctx in self._context_stack:
                ctx._transitioned = True

    def __router(self, __e):
        if self.__compartment.state == "A":
            self._state_A(__e, self.__compartment)
        elif self.__compartment.state == "B":
            self._state_B(__e, self.__compartment)

    def __transition(self, next_compartment):
        self.__next_compartment = next_compartment

    def go(self):
        __e = MinFrameEvent("go", [])
        __ctx = MinFrameContext(__e, None)
        self._context_stack.append(__ctx)
        try:
            self.__kernel(__e)
        finally:
            __frame_ctx = self._context_stack.pop()
        return __frame_ctx._return

    def val(self) -> int:
        __e = MinFrameEvent("val", [])
        __ctx = MinFrameContext(__e, None)
        self._context_stack.append(__ctx)
        try:
            self.__kernel(__e)
        finally:
            __frame_ctx = self._context_stack.pop()
        return __frame_ctx._return

    def _state_A(self, __e, compartment):
        if __e._message == "go":
            self._s_A_hdl_user_go(__e, compartment)
            return
        if __e._message == "val":
            self._s_A_hdl_user_val(__e, compartment)
            return

    def _state_B(self, __e, compartment):
        pass

    def _s_A_hdl_user_go(self, __e, compartment):
        self.x = 1
        __compartment = self.__prepareEnter("B", [], [])
        self.__transition(__compartment)
        return

    def _s_A_hdl_user_val(self, __e, compartment):
        self._context_stack[-1]._return = 5

"#;
