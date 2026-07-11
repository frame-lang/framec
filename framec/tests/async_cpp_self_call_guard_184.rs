//! #184 — C++ async: the `_transitioned` guard emitted after a reentrant
//! `@@:self.method(...)` call must be `co_return;`, not a bare `return;`. An
//! `@@[async]` C++ handler is lowered to a coroutine (`FrameTask`), where a bare
//! `return;` is a hard compile error ("a 'return' statement is not allowed in a
//! coroutine"). This is the C++ half of #181 (the await fix), which was validated
//! by reading generated code but never compiled, so the guard defect slipped
//! through and blocked the entire cpp_23 async column. A runnable end-to-end
//! fixture lives in framec-test-env (tests/cpp/positive/async_reentrant_self_call);
//! this pins the emitted keyword so a regression is caught by `cargo test`.

mod common;
use common::compile_source;

// Reentrant self-call: `twice()` calls the interface method `add` (NOT an action),
// so the transition guard IS emitted after the (co_awaited) call.
const ASYNC_SRC: &str = r#"
@@[async]
@@system Acc {
    interface:
        async add(x: int): int
        async twice(): int
    machine:
        $Active {
            $>() { @@:self.n = 0; }
            add(x: int): int  { @@:self.n = @@:self.n + x;  @@:(@@:self.n) }
            twice(): int      { @@:self.add(@@:self.n);     @@:(@@:self.n) }
        }
    domain:
        n: int = 0;
}
"#;

// The same system without @@[async]: a plain `void` handler where `return;` is
// correct and `co_return;` would be wrong.
const SYNC_SRC: &str = r#"
@@system Acc {
    interface:
        add(x: int): int
        twice(): int
    machine:
        $Active {
            $>() { @@:self.n = 0; }
            add(x: int): int  { @@:self.n = @@:self.n + x;  @@:(@@:self.n) }
            twice(): int      { @@:self.add(@@:self.n);     @@:(@@:self.n) }
        }
    domain:
        n: int = 0;
}
"#;

#[test]
fn async_cpp_guard_is_co_return() {
    let c = compile_source(ASYNC_SRC, "cpp_23");
    assert!(
        c.contains("_transitioned) co_return;"),
        "[#184/cpp] async self-call guard must be `co_return;` in a coroutine\n{c}"
    );
    assert!(
        !c.contains("_transitioned) return;"),
        "[#184/cpp] a bare `return;` guard is a compile error inside a FrameTask coroutine\n{c}"
    );
}

#[test]
fn sync_cpp_guard_stays_plain_return() {
    // The flag must be inert when the system is not async: a non-coroutine `void`
    // handler needs a plain `return;` (co_return would not compile there).
    let c = compile_source(SYNC_SRC, "cpp_23");
    assert!(
        c.contains("_transitioned) return;"),
        "[#184/cpp] a sync (non-coroutine) handler must keep the plain `return;` guard\n{c}"
    );
    assert!(
        !c.contains("_transitioned) co_return;"),
        "[#184/cpp] co_return must NOT leak into a sync handler\n{c}"
    );
}
