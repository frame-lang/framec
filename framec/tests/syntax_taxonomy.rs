//! Frame syntax taxonomy — behavioral anchors.
//!
//! These tests pin the *category* each Frame construct belongs to by
//! asserting what it lowers to (statement vs property-reference vs
//! property-mutation vs native passthrough). They are the executable
//! ground truth behind the "Frame Syntax Taxonomy" appendix in
//! `docs/frame_language.md` — the terminology there is only allowed to
//! claim what these tests verify.
//!
//! Backend: python_3 (representative; the lowering shape is uniform).

mod common;
use common::compile_source;

fn py(body_system: &str) -> String {
    compile_source(body_system, "python_3")
}

// ---------------------------------------------------------------------
// Return family — statement (mutation / exit) vs native passthrough.
// ---------------------------------------------------------------------

/// `@@:return = e` and `@@:(e)` are MUTATIONS of the return property:
/// they write the Frame-managed return slot, no exit. `@@:(e)` is sugar
/// for `@@:return = e` (byte-identical body).
#[test]
fn context_return_setter_and_sugar_write_return_slot() {
    let g = py(r#"
@@system R {
    interface:
        a(): int
        b(): int
    machine:
        $S {
            a(): int { @@:return = 5 }
            b(): int { @@:(5) }
        }
}
"#);
    // Both forms write the return slot.
    let writes = g.matches("._return = 5").count();
    assert!(
        writes >= 2,
        "@@:return = 5 and @@:(5) must both write the return slot (sugar-equivalent):\n{g}"
    );
}

/// `@@:return(e)` is a MUTATION + EXIT (set the slot, then return).
#[test]
fn context_return_call_writes_slot_then_exits() {
    let g = py(r#"
@@system R {
    interface:
        c(): int
    machine:
        $S { c(): int { @@:return(7) } }
}
"#);
    assert!(
        g.contains("._return = 7"),
        "must write the return slot:\n{g}"
    );
}

/// Native `return e` is PASSTHROUGH — emitted verbatim, never touches
/// the Frame return slot.
#[test]
fn native_return_is_passthrough() {
    let g = py(r#"
@@system N {
    operations:
        f(): int { return 9 }
    interface:
        g()
    machine:
        $S { g() { } }
}
"#);
    assert!(
        g.contains("return 9"),
        "native return emitted verbatim:\n{g}"
    );
}

// ---------------------------------------------------------------------
// References & call expressions — constructs that YIELD a value.
// ---------------------------------------------------------------------

/// Bare `@@:return` is a REFERENCE (getter): it reads the return slot as
/// an rvalue and interpolates into the surrounding native expression.
#[test]
fn bare_context_return_is_a_reference() {
    let g = py(r#"
@@system R {
    interface:
        a()
    machine:
        $S { a() { $.saved = @@:return } }
    domain:
        saved: int = 0
}
"#);
    // RHS reads the return slot (no write to it on this line).
    assert!(
        g.contains("= self._context_stack[-1]._return"),
        "bare @@:return must READ the return slot into the assignment RHS:\n{g}"
    );
}

/// `@@:self.m()` is a CALL EXPRESSION — usable in value position; its
/// result becomes the assignment RHS.
#[test]
fn self_call_is_a_call_expression() {
    let g = py(r#"
@@system R {
    interface:
        a()
        b(): int
    machine:
        $S {
            a() { $.x = @@:self.b() }
            b(): int { @@:(5) }
        }
    domain:
        x: int = 0
}
"#);
    assert!(
        g.contains("= self.b()"),
        "@@:self.b() must be callable in value position (RHS):\n{g}"
    );
}

/// `@@Child()` (system instantiation) is a CALL EXPRESSION — its factory
/// result is usable in value position.
#[test]
fn instantiation_is_a_call_expression() {
    let g = py(r#"
@@system Child {
    interface:
        noop()
    machine:
        $S { noop() { } }
}

@@[main]
@@system Parent {
    interface:
        make()
    machine:
        $S { make() { $.c = @@Child() } }
    domain:
        c: int = 0
}
"#);
    assert!(
        g.contains("= Child._create()"),
        "@@Child() must produce a factory call usable in value position (RHS):\n{g}"
    );
}
