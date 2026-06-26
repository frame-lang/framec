//! Issue #130 — a bare `@@:return` used as a STANDALONE STATEMENT lowers
//! to a no-op read of the context return slot and does NOT short-circuit:
//! trailing code still runs. Per the syntax taxonomy `@@:return` IS the
//! getter, so the read itself is semantically valid — but a value-less
//! getter in statement position has no effect and silently loses the
//! user's intent (they meant `@@:return(e)`, `@@:return = e`, or native
//! `return`).
//!
//! The fix WARNS (W416, `return-getter-no-effect`); it does NOT change the
//! semantics (native `return` already exits; making bare `@@:return` exit
//! would contradict its getter role).
//!
//! The warning fires ONLY for the bare-statement case. It must NOT fire
//! when `@@:return` carries a value (`= e`, `(e)`, `@@:(e)`) or when the
//! getter is read inside a larger native expression (`x = @@:return + 1`).

mod common;
use common::compile_with_warnings;

fn w416<'a>(warnings: &'a [String]) -> Vec<&'a String> {
    warnings.iter().filter(|w| w.contains("W416")).collect()
}

/// (a) Bare `@@:return` alone on its statement line → W416. The trailing
/// `self.x = 99` is the code that silently runs after the no-op read.
const BARE_RETURN_STMT: &str = r#"
@@[target("python_3")]
@@system T {
    interface:
        f(): int
    machine:
        $A {
            f(): int {
                @@:return
                self.x = 99
            }
        }
    domain:
        x: int = 0
}
"#;

#[test]
fn bare_return_statement_emits_w416() {
    let (_code, warnings) = compile_with_warnings(BARE_RETURN_STMT, "python_3");
    let w = w416(&warnings);
    assert!(
        !w.is_empty(),
        "bare `@@:return` statement must emit W416; got warnings: {warnings:?}"
    );
}

/// (b1) `@@:return = expr` (explicit setter) → NO W416.
const RETURN_ASSIGN: &str = r#"
@@[target("python_3")]
@@system T {
    interface:
        f(): int
    machine:
        $A {
            f(): int {
                @@:return = 5
            }
        }
    domain:
        x: int = 0
}
"#;

#[test]
fn return_assign_does_not_emit_w416() {
    let (_code, warnings) = compile_with_warnings(RETURN_ASSIGN, "python_3");
    assert!(
        w416(&warnings).is_empty(),
        "`@@:return = expr` must NOT emit W416; got: {warnings:?}"
    );
}

/// (b2) `@@:return(expr)` (setter + exit) → NO W416.
const RETURN_CALL: &str = r#"
@@[target("python_3")]
@@system T {
    interface:
        f(): int
    machine:
        $A {
            f(): int {
                @@:return(5)
            }
        }
    domain:
        x: int = 0
}
"#;

#[test]
fn return_call_does_not_emit_w416() {
    let (_code, warnings) = compile_with_warnings(RETURN_CALL, "python_3");
    assert!(
        w416(&warnings).is_empty(),
        "`@@:return(expr)` must NOT emit W416; got: {warnings:?}"
    );
}

/// (b3) `@@:(expr)` (concise setter) → NO W416.
const RETURN_EXPR_CONCISE: &str = r#"
@@[target("python_3")]
@@system T {
    interface:
        f(): int
    machine:
        $A {
            f(): int {
                @@:(5)
            }
        }
    domain:
        x: int = 0
}
"#;

#[test]
fn concise_return_expr_does_not_emit_w416() {
    let (_code, warnings) = compile_with_warnings(RETURN_EXPR_CONCISE, "python_3");
    assert!(
        w416(&warnings).is_empty(),
        "`@@:(expr)` must NOT emit W416; got: {warnings:?}"
    );
}

/// (c) `@@:return` read INSIDE a native expression (RHS / arithmetic) → NO
/// W416. The getter's value is genuinely consumed, so it has an effect.
const RETURN_IN_EXPRESSION: &str = r#"
@@[target("python_3")]
@@system T {
    interface:
        f(): int
    machine:
        $A {
            f(): int {
                y = @@:return + 1
                @@:(y)
            }
        }
    domain:
        x: int = 0
}
"#;

#[test]
fn return_read_in_expression_does_not_emit_w416() {
    let (_code, warnings) = compile_with_warnings(RETURN_IN_EXPRESSION, "python_3");
    assert!(
        w416(&warnings).is_empty(),
        "`@@:return` read in an expression must NOT emit W416; got: {warnings:?}"
    );
}

/// The fix only WARNS — it does NOT change semantics. A bare `@@:return`
/// statement still lowers to the no-op read (the trailing code still runs)
/// so existing behavior is unchanged; only the diagnostic is new.
#[test]
fn bare_return_semantics_unchanged() {
    let (code, _w) = compile_with_warnings(BARE_RETURN_STMT, "python_3");
    assert!(
        code.contains("self._context_stack[-1]._return"),
        "bare `@@:return` must still lower to the (no-op) read; got:\n{code}"
    );
    assert!(
        code.contains("self.x = 99"),
        "trailing code after bare `@@:return` must still be emitted; got:\n{code}"
    );
}
