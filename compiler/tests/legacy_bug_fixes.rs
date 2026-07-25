//! **The deliberate divergences: where ng is RIGHT and the shipped compiler is WRONG.**
//!
//! framec-ng is held to byte-faithfulness against the 4.6.1 oracle everywhere *except* where
//! the oracle emits a program that is provably broken. Those exceptions are not accidents and
//! they are not "close enough" — each one is a bug with a name, and each one is guarded here by
//! a test that **builds the emitted Python and runs it**, asserting the observable outcome.
//!
//! A byte-comparison could not do this job. The whole reason these bugs shipped is that the
//! emitted file looked plausible: `__prepareEnter("Paid", [], [])` is well-formed Python that
//! raises `IndexError` two frames later, and `def __init__(self):` is a well-formed constructor
//! that cannot construct. So every test below **executes** the program and asserts what it
//! prints or returns — the only evidence that settles it.
//!
//! Each test records: the LEGACY behavior (verified by running the 4.6.1 oracle's output), NG's
//! behavior, and why legacy is wrong. Consequence, stated once: a corpus fixture that exercises
//! any of these shapes **can never be byte-identical to the oracle**, and that is the correct
//! outcome, not a faithfulness failure.
//!
//! The harness is `two_backends.rs`'s: emit through the real driver, write the file, run it.

use frame_compiler::resolve::resolve;
use frame_compiler::scan::{literals::Target, segment};
use frame_compiler::text::emit::{driver, python::Python};
use frame_compiler::Source;
use std::process::Command;

fn have_python() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Emit `frm` for Python through the production driver.
fn emit_py(frm: &str) -> String {
    let src = Source::new("t.frm", frm.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Python3).expect("segment");
    let (syms, diags) = resolve(&ast);
    assert!(diags.is_empty(), "{diags:#?}");
    driver::emit(&src, &ast, &syms, &Python)
}

/// Emit `frm`, append `main`, RUN it, and return `(stdout, stderr, ok)`.
fn run_py(name: &str, frm: &str, main: &str) -> (String, String, bool) {
    let code = emit_py(frm);
    let d = std::env::temp_dir().join(format!("frame_bugfix_{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let f = d.join("m.py");
    std::fs::write(
        &f,
        format!(
            "{}{code}{main}",
            frame_compiler::text::emit::python::PRELUDE
        ),
    )
    .unwrap();
    let out = Command::new("python3").arg(&f).output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
        out.status.success(),
    )
}

/// **DIVERGENCE 1 — `push$ -> $S(args)` DELIVERS the state args. Legacy drops them.**
///
/// LEGACY (4.6.1, verified by running its output): `push$ -> $Paid("cola, diet", 5)` emits
/// `self.__prepareEnter("Paid", [], [])` — an EMPTY state-arg list. The destination's dispatcher
/// still emits `item = self.__compartment.state_args[0]`, so the generated program dies with
/// `IndexError: list index out of range` the moment the machine enters `$Paid`. The plain
/// (non-push) `-> $Paid(...)` form on the same compiler passes the args correctly, which is what
/// makes this a slip rather than a design: the two transition forms disagree.
///
/// NG: emits `self.__prepareEnter("Paid", ["cola, diet", 5], [])`.
///
/// WHY LEGACY IS WRONG: the args are in the source, the destination declares parameters to
/// receive them, and the emitted program crashes. There is no reading under which dropping them
/// is intended.
///
/// The literal `"cola, diet"` is deliberate: it contains the argument separator, so this also
/// pins that the arg list is not re-split on commas somewhere downstream.
#[test]
fn push_transition_delivers_state_args() {
    if !have_python() {
        eprintln!("SKIPPED python3");
        return;
    }
    let frm = r#"@@system V {
    interface:
        buy()
        done()
    machine:
        $Idle {
            buy() { push$ -> $Paid("cola, diet", 5) }
        }
        $Paid(item: str, amount: int) {
            $>() { print(f"item={item} amount={amount}") }
            done() { -> pop$ }
        }
}
"#;
    let (out, err, ok) = run_py("push_args", frm, "\nv = V()\nv.buy()\nprint('done')\n");
    assert!(ok, "the emitted program must run: {err}");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["item=cola, diet amount=5", "done"],
        "push$ must carry the state args into the destination compartment"
    );
}

/// **DIVERGENCE 2 — a plain `Sys(args)` constructor builds a USABLE system. Legacy's cannot.**
///
/// LEGACY (verified by running its output): `__init__` takes NO parameters — `def __init__(self):`
/// — and the header params are bound in the `_create` classmethod instead (`c.flag = initial`).
/// So `Flag(True)` raises `TypeError: Flag.__init__() takes 1 positional argument but 2 were
/// given`, and `Flag()` returns an object whose domain fields were never assigned. The class's own
/// constructor cannot construct the class.
///
/// NG: `__init__(self, initial: bool)` seeds the domain fields and builds the start compartment,
/// and `_create` calls `cls(initial)`. Both entry points produce a working machine.
///
/// WHY LEGACY IS WRONG: a Python class whose `__init__` leaves the object unusable is broken by
/// construction — subclassing it, pickling it, or calling it the obvious way all fail. `_create`
/// exists to run the start state's `$>` through the kernel, which is a *lifecycle* concern; it is
/// not a reason for `__init__` to refuse its own arguments.
#[test]
fn plain_constructor_builds_a_usable_system() {
    if !have_python() {
        eprintln!("SKIPPED python3");
        return;
    }
    let frm = r#"@@system Flag(initial: bool) {
    interface:
        get(): bool
    machine:
        $Active { get(): bool { @@:(self.flag) } }
    domain:
        flag = initial
}
"#;
    let (out, err, ok) = run_py(
        "plain_ctor",
        frm,
        "\nprint('get:', Flag(True).get())\nprint('get:', Flag(False).get())\n",
    );
    assert!(ok, "the emitted program must run: {err}");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["get: True", "get: False"],
        "the declared header param must reach the domain field through `__init__`"
    );
}

/// **DIVERGENCE 3 — a state's `$.x` is seeded for a plain `Sys()` too.**
///
/// LEGACY: state-var initializers are emitted into a SYNTHESIZED `$>` handler, guarded by
/// `if "n" not in compartment.state_vars:`. That handler only runs when the kernel dispatches the
/// start state's `$>`, which only happens via `_create`. Construct with `Sys()` and every `$.x`
/// read is a `KeyError`.
///
/// NG: the start compartment's state vars are seeded where the compartment is BUILT — in
/// `__init__` for the start state, and in the transition for every destination. Same values, one
/// step earlier, and independent of which entry point built the object.
///
/// WHY LEGACY IS WRONG: same reason as divergence 2 — it makes the object's validity depend on
/// which of two constructors the caller happened to use.
#[test]
fn state_vars_are_seeded_for_a_plain_constructor() {
    if !have_python() {
        eprintln!("SKIPPED python3");
        return;
    }
    let frm = r#"@@system SV {
    interface:
        peek(): int
    machine:
        $A {
            $.n: int = 7
            peek(): int { @@:($.n) }
        }
}
"#;
    let (out, err, ok) = run_py("statevar_seed", frm, "\nprint('n:', SV().peek())\n");
    assert!(ok, "the emitted program must run: {err}");
    assert_eq!(out, "n: 7", "$.n must be seeded by the plain constructor");
}

/// **NG BUG, FIXED — statements after `@@:(expr)` still RUN on Python, so they must be emitted.**
///
/// This one is ng's own defect, not legacy's; the shipped compiler gets it right. It is recorded
/// here because the mechanism is the same family: an emitter deciding a statement is unreachable
/// when the program it emits keeps running.
///
/// THE BUG: the body walk stops at a "base-nesting terminal", and `@@:(expr)` was counted as one
/// on every target. On Java/Rust/C that is true — the spelling is a `return`. On **Python it is
/// not**: `@@:(expr)` spells `self._context_stack[-1]._return = expr`, an assignment, and the
/// handler runs on (the caller reads the slot back after the kernel finishes). So every statement
/// the user wrote after `@@:(expr)` was SILENTLY DELETED from the output — statements that would
/// have executed. A green compile hid it completely; the emitted file was valid Python that
/// simply did less than the source said.
///
/// THE FIX: `Backend::return_call_terminates` — the walk asks whether THIS target's spelling
/// actually returns before latching the terminal bit.
#[test]
fn ng_bug_statements_after_frame_return_still_run() {
    if !have_python() {
        eprintln!("SKIPPED python3");
        return;
    }
    let frm = r#"@@system R {
    interface:
        go(): int
    machine:
        $A {
            go(): int {
                @@:(5)
                log("after")
            }
        }
}
"#;
    let (out, err, ok) = run_py(
        "after_return",
        frm,
        "\ndef log(m): print('ran ' + m)\nprint('got:', R().go())\n",
    );
    assert!(ok, "the emitted program must run: {err}");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["ran after", "got: 5"],
        "a statement after `@@:(expr)` is REACHABLE on Python and must be emitted AND run"
    );
}
