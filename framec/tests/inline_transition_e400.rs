//! FRAMEC_BUGS #13: a transition must be the last statement on its line.
//!
//! Inline native control flow with transition arms — `if c then -> $A else
//! -> $B end` — can't scope the transition's implicit return through an
//! opaque native block (Oceans Model), so it used to silently miscompile
//! (wrong arm, dropped `else`, or a broken `end`). It is now rejected with
//! **E400**. The multi-line if/then/else form works, as does a brace-
//! delimited inline block (the braces scope the return).

mod common;
use common::{compile_expect_error, compile_source};

#[test]
fn inline_if_then_else_with_transitions_is_rejected() {
    let err = compile_expect_error(
        r#"
@@[target("lua")]
@@system R {
    interface: go(x: int)
    machine:
        $A { go(x: int) { if x > 10 then -> $B else -> $C end } }
        $B { }
        $C { }
}
"#,
        "lua",
    );
    assert!(err.contains("E400"), "expected E400, got:\n{err}");
}

#[test]
fn inline_if_single_arm_transition_is_rejected() {
    let err = compile_expect_error(
        r#"
@@[target("lua")]
@@system R {
    interface: go()
    machine:
        $A { go() { if true then -> $B end } }
        $B { }
}
"#,
        "lua",
    );
    assert!(err.contains("E400"), "expected E400, got:\n{err}");
}

#[test]
fn multiline_if_with_transitions_compiles() {
    // The supported form: each transition on its own line.
    let out = compile_source(
        r#"
@@[target("lua")]
@@system R {
    interface: go(x: int)
    machine:
        $A { go(x: int) {
            if x > 10 then
                -> $B
            else
                -> $C
            end
        } }
        $B { }
        $C { }
}
"#,
        "lua",
    );
    assert!(
        out.contains("__transition"),
        "multi-line if must still transition:\n{out}"
    );
}

#[test]
fn braced_inline_block_transition_compiles() {
    // A brace-delimited inline block scopes the implicit return correctly,
    // so it must NOT be rejected (only the non-brace `}`-less forms are).
    let _ = compile_source(
        r#"
@@[target("c")]
@@system R {
    interface: go(c: int)
    machine:
        $A { go(c: int) { if (c) { -> $B } } }
        $B { }
}
"#,
        "c",
    );
}

#[test]
fn transition_with_trailing_semicolon_compiles() {
    let _ = compile_source(
        r#"
@@[target("typescript")]
@@system R {
    interface: go()
    machine:
        $A { go() { -> $B; } }
        $B { }
}
"#,
        "typescript",
    );
}
