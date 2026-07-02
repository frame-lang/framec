//! RFC-0027 in-tree snapshot tests — Python backend.
//!
//! Snapshots the framec-emitted Python code for the canonical
//! 3-fixture corpus. Changes to the Python backend produce
//! reviewable `.snap` diffs in PRs.
//!
//! Re-bless workflow when an intentional codegen change is made:
//!   cargo install cargo-insta   # one-time
//!   cargo test --test python_snapshots
//!   cargo insta review
//!   git add tests/snapshots/ && git commit
//!
//! Adding a backend: copy this file to e.g. `java_snapshots.rs`
//! and change the target string in each call. Phase 2 of RFC-0027
//! rolls this out to the remaining 16 backends.

mod common;

use common::{compile_check_all, compile_fixture, compile_source, find_tool};
use std::process::Command;

/// Regression for the `push$ -> $State` codegen bug (Issue #42): the
/// with-transition form must use the compartment model (`__prepareEnter` +
/// `__transition`), not the removed `_transition()`. The `05_pushpop` fixture
/// only covers bare `push$` + a separate transition.
#[test]
fn push_transition() {
    let src = r#"
@@system PushTransition {
    interface:
        go()
        back()
    machine:
        $A { go() { push$ -> $B } }
        $B { back() { -> pop$ } }
}
"#;
    let out = compile_source(src, "python_3");
    assert!(
        out.contains("self._state_stack.append(self.__compartment)"),
        "push$ -> $State must push the current compartment:\n{out}"
    );
    assert!(
        out.contains("self.__transition(__compartment)"),
        "push$ -> $State must transition via the compartment model:\n{out}"
    );
    assert!(
        !out.contains("self._transition("),
        "push$ -> $State must not call the non-existent _transition():\n{out}"
    );
}

/// Regression for FRAMEC_BUGS Issue #44: when a parent composes a child
/// system that renamed its persist ops via `@@[save(name)]` /
/// `@@[load(name)]`, the parent's save/restore must call the child by its
/// DECLARED name — not the hardcoded language default (`save_state` /
/// `restore_state`), which doesn't exist on the renamed child and crashed
/// at runtime.
#[test]
fn nested_child_custom_persist_name() {
    let src = r#"
@@[persist(str)]
@@[save(persist_me)]
@@[load(unpersist_me)]
@@system Child {
    interface:
        ping()
    machine:
        $Idle { ping() { self.hits = self.hits + 1 } }
    domain:
        hits = 0
}

@@[main]
@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Parent {
    interface:
        poke()
    machine:
        $Run { poke() { self.child.ping() } }
    domain:
        child = @@Child()
}
"#;
    let out = compile_source(src, "python_3");
    assert!(
        out.contains("self.child.persist_me()"),
        "parent save must call the child's declared @@[save] name:\n{out}"
    );
    assert!(
        out.contains("self.child.unpersist_me("),
        "parent restore must call the child's declared @@[load] name:\n{out}"
    );
    assert!(
        !out.contains("self.child.save_state()") && !out.contains("self.child.restore_state("),
        "parent must not call the language-default persist names on the renamed child:\n{out}"
    );
}

#[test]
fn linear_fsm() {
    insta::assert_snapshot!(compile_fixture("01_linear_fsm", "python_3"));
}

#[test]
fn hsm() {
    insta::assert_snapshot!(compile_fixture("02_hsm", "python_3"));
}

#[test]
fn persist() {
    insta::assert_snapshot!(compile_fixture("03_persist", "python_3"));
}

#[test]
fn state_args() {
    insta::assert_snapshot!(compile_fixture("04_state_args", "python_3"));
}

#[test]
fn pushpop() {
    insta::assert_snapshot!(compile_fixture("05_pushpop", "python_3"));
}

#[test]
fn selfcall() {
    insta::assert_snapshot!(compile_fixture("06_selfcall", "python_3"));
}

#[test]
fn forward() {
    insta::assert_snapshot!(compile_fixture("07_forward", "python_3"));
}

#[test]
fn lifecycle() {
    insta::assert_snapshot!(compile_fixture("08_lifecycle", "python_3"));
}

#[test]
fn return_explicit() {
    insta::assert_snapshot!(compile_fixture("09_return_explicit", "python_3"));
}

#[test]
fn actions() {
    insta::assert_snapshot!(compile_fixture("10_actions", "python_3"));
}

#[test]
fn consts() {
    insta::assert_snapshot!(compile_fixture("11_consts", "python_3"));
}

#[test]
fn no_persist() {
    insta::assert_snapshot!(compile_fixture("12_no_persist", "python_3"));
}

#[test]
fn lifecycle_args() {
    insta::assert_snapshot!(compile_fixture("13_lifecycle_args", "python_3"));
}

/// RFC-0034: every canonical fixture's framec-emitted Python
/// output must parse cleanly under `python3 -m py_compile`. Closes
/// the snapshot-doesn't-compile gap for Python — snapshots only
/// diff text, so without this check a fixture could freeze
/// syntactically invalid Python and the test suite would still
/// pass.
#[test]
fn rfc0034_all_fixtures_compile() {
    let py3 = match find_tool("python3") {
        Some(p) => p,
        None => {
            eprintln!("python_3 RFC-0034 compile check skipped: `python3` not on PATH");
            return;
        }
    };
    compile_check_all("python_3", "py", |path| {
        Command::new(&py3)
            .args(["-m", "py_compile"])
            .arg(path)
            .output()
            .expect("python3 process")
    });
}

/// RFC-0033 #12 (cross-backend generalization): the parser fix for
/// path-expression call forms (`String::from(args)`) also covers
/// bare function-call initializers (`list()`, `dict()`, `MyClass(x)`).
/// Before the fix, the parser dropped everything after the
/// identifier and Python emitted `state_vars["x"] = list` (a
/// reference to the type), not `state_vars["x"] = list()` (a fresh
/// instance). Same parser bug — the user's code is silently wrong.
/// Regression for FRAMEC_BUGS Issue #47: a `$.var` interpolated inside a
/// double-quoted f-string used to lower its dict-subscript key with double
/// quotes too (`f"...{state_vars["k"]}..."`) — a `SyntaxError` on Python
/// < 3.12 (pre-PEP-701). The key must take the OPPOSITE quote of the
/// surrounding string. This assertion is version-independent: it checks the
/// emitted quote choice directly, so it fails on a 3.12 host where
/// `py_compile` would wrongly pass (PEP 701 accepts the nested same-quote).
#[test]
fn fstring_state_var_quote_swap_47() {
    // (a) double-quoted f-string -> key must use single quotes
    let out_a = compile_source(
        r#"
@@system A {
    interface: status(): str = ""
    machine:
        $Active {
            $.failures: int = 0
            status(): str { @@:(f"closed ({$.failures} failures)") }
        }
}
"#,
        "python_3",
    );
    // Scope to the f-string interpolation (the `} failures)` suffix) so the
    // correctly double-quoted state-var INITIALIZER line
    // (`state_vars["failures"] = 0`, not inside any string) isn't matched.
    assert!(
        out_a.contains("state_vars['failures']} failures)"),
        "state-var key inside a double-quoted f-string must use single quotes:\n{out_a}"
    );
    assert!(
        !out_a.contains("state_vars[\"failures\"]} failures)"),
        "must not nest same-quote (double-in-double) — SyntaxError pre-3.12:\n{out_a}"
    );

    // (b) single-quoted f-string -> key must flip to double quotes
    let out_b = compile_source(
        r#"
@@system B {
    interface: status(): str = ""
    machine:
        $Active {
            $.failures: int = 0
            status(): str { @@:(f'closed ({$.failures} failures)') }
        }
}
"#,
        "python_3",
    );
    assert!(
        out_b.contains("state_vars[\"failures\"]} failures)"),
        "state-var key inside a single-quoted f-string must use double quotes:\n{out_b}"
    );

    // (c) plain (non-string) expression -> key stays double-quoted
    let out_c = compile_source(
        r#"
@@system C {
    interface: bump(): int = 0
    machine:
        $Active {
            $.count: int = 0
            bump(): int { @@:($.count + 1) }
        }
}
"#,
        "python_3",
    );
    assert!(
        out_c.contains("state_vars[\"count\"] + 1"),
        "state-var key outside any string must remain double-quoted:\n{out_c}"
    );
}

#[test]
fn rfc0033_state_var_call_initializers_python() {
    let src = r#"
@@system Repro {
    interface:
        get_x()
    machine:
        $A {
            $.lst: list = list()
            $.dct: dict = dict()
            $.s: str = str("hello")
            get_x() { @@:(self.lst) }
        }
}
"#;
    let out = compile_source(src, "python_3");

    for expected in [
        "compartment.state_vars[\"lst\"] = list()",
        "compartment.state_vars[\"dct\"] = dict()",
        "compartment.state_vars[\"s\"] = str(\"hello\")",
    ] {
        assert!(
            out.contains(expected),
            "state-var call initializer not preserved — expected `{}` in output",
            expected
        );
    }
}

/// RFC-0043 `@@[async]` — golden coverage of the casing/machine layering (issue
/// #111 R1). Previously the async emission core had zero snapshot coverage.
#[test]
fn async_attribute() {
    insta::assert_snapshot!(compile_fixture("14_async_attribute", "python_3"));
}
