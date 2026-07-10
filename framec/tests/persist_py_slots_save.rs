//! RFC-0055 reflective-save robustness — the Python reflective persist hook tagged
//! a user-typed value with `**vars(_o)`, which raises `TypeError: vars() argument
//! must have __dict__ attribute` on a `__slots__`-only class, crashing save. The
//! hook now gathers fields generically from `__dict__` AND the MRO's `__slots__`
//! (one hook, no per-type branch), so a __slots__ class round-trips. Runtime-
//! verified separately; this pins the emitted gatherer.

mod common;
use common::compile_source;

const SRC: &str = r#"
class Point:
    __slots__ = ("x", "y")
    def __init__(self, x=0.0, y=0.0):
        self.x = x
        self.y = y

@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Bag {
    interface:
        setp(x, y)
    machine:
        $S { setp(x, y) {} }
    domain:
        p: Point = Point()
}
"#;

#[test]
fn python_save_hook_is_slots_safe() {
    let c = compile_source(SRC, "python_3");
    // Gathers __dict__ AND __slots__ across the MRO.
    assert!(
        c.contains(r#"_f = dict(getattr(_o, "__dict__", None) or {})"#)
            && c.contains(r#"getattr(_c, "__slots__", ())"#),
        "[slots/python] reflective save hook must gather __dict__ + __slots__ generically\n{c}"
    );
    // The crashing form must be gone.
    assert!(
        !c.contains("**vars(_o)}"),
        "[slots/python] `**vars(_o)` crashes on a __slots__-only class; must not be emitted\n{c}"
    );
}
