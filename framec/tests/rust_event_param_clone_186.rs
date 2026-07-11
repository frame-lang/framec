//! #186 — the Rust state router forwards an event param to its handler by
//! destructuring the shared `&FrameEvent` and passing the field. framec is
//! type-ignorant, so it cannot assume the field is `Copy`: moving it out with
//! `*name` is `error[E0507]: cannot move out of ... behind a shared reference`
//! for any non-Copy payload (`String`, `Vec`, `HashMap`, a user struct). The
//! router now clones by default and keeps the cheap `*name` deref only for the
//! built-in Copy scalars.

mod common;
use common::compile_source;

const SRC: &str = r#"
@@[target("rust")]
@@system Repro {
    interface:
        take(p: Payload)
        many(xs: Vec<u8>)
        named(s: String)
        scalar(x: i32)
        flag(b: bool)

    machine:
        $S {
            take(p: Payload)  {}
            many(xs: Vec<u8>) {}
            named(s: String)  {}
            scalar(x: i32)    {}
            flag(b: bool)     {}
    }
}
"#;

#[test]
fn non_copy_event_params_are_cloned_not_moved() {
    let c = compile_source(SRC, "rust");
    // A type-ignorant user struct must clone, not `*p` (the #186 crash).
    assert!(
        c.contains("_hdl_user_take(__e, p.clone())"),
        "[#186] a user-typed event param must be cloned, not moved with `*p`\n{c}"
    );
    assert!(
        !c.contains("_hdl_user_take(__e, *p)"),
        "[#186] `*p` moves a non-Copy payload out of `&FrameEvent` (E0507)\n{c}"
    );
    // Collections likewise clone.
    assert!(
        c.contains("_hdl_user_many(__e, xs.clone())"),
        "[#186] a `Vec` event param must be cloned\n{c}"
    );
    // String already cloned; keep it.
    assert!(
        c.contains("_hdl_user_named(__e, s.clone())"),
        "[#186] a `String` event param must be cloned\n{c}"
    );
}

#[test]
fn copy_scalar_event_params_keep_deref() {
    let c = compile_source(SRC, "rust");
    // Built-in Copy scalars keep the cheap `*name` (a `.clone()` here would draw
    // clippy's `clone_on_copy`).
    assert!(
        c.contains("_hdl_user_scalar(__e, *x)"),
        "[#186] an `i32` event param should keep the `*x` deref\n{c}"
    );
    assert!(
        c.contains("_hdl_user_flag(__e, *b)"),
        "[#186] a `bool` event param should keep the `*b` deref\n{c}"
    );
}
