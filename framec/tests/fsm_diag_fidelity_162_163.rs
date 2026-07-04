//! #162 / #163 — @@fsm diagnostic fidelity end-to-end (the "diagnostic
//! survives to the binary" guarantee from framec#100).

mod common;
use common::compile_expect_error;

/// #162 — E712 (transition in an action body) must SURVIVE the parser→CLI
/// path with its specific code, for the INLINE action-block shape (the
/// `actions:`-section shape was already covered by a parser unit test; the
/// inline shape crossed StatementParser/StateParser hops that dropped the
/// code to the generic E700).
#[test]
fn e712_survives_inline_action_block() {
    let err = compile_expect_error(
        r#"
@@[target("python_3")]

@@fsm M(text: bytes) : bool = false { /a/ { -> $x } true : -> $x  $x: false }
"#,
        "python_3",
    );
    assert!(
        err.contains("E712"),
        "[#162] inline action-block transition must surface as E712:\n{err}"
    );
    assert!(
        !err.starts_with("E700"),
        "[#162] must not flatten to the generic E700:\n{err}"
    );
}

/// #163 — `//` inside @@fsm is lexically a LINE COMMENT (RFC-0043 §3.5), so
/// an empty regex is unwritable; the comment swallows the closing brace and
/// the diagnostic must say so — an @@fsm-specific unterminated-block message
/// with the `//`-comment note, not the generic "@@system" one.
#[test]
fn empty_regex_spelling_gets_fsm_specific_guidance() {
    let err = compile_expect_error(
        r#"
@@[target("python_3")]

@@fsm T(text: bytes) : bool = false { // true }
"#,
        "python_3",
    );
    assert!(
        err.contains("Unterminated @@fsm block 'T'"),
        "[#163] must name the @@fsm block, not @@system:\n{err}"
    );
    assert!(
        err.contains("line comment") && err.contains("empty regex"),
        "[#163] must carry the //-comment + empty-regex guidance:\n{err}"
    );
}
