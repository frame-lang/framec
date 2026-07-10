//! Issue #181 — a reentrant self-INTERFACE call (`@@:self.method()`, the form
//! that also emits the `_transitioned` guard) inside an `@@[async]` handler body
//! re-enters async dispatch and returns an awaitable, so it MUST be awaited. An
//! un-awaited coroutine / future / task is discarded — the call is a silent
//! no-op and the handler returns the wrong result. framec awaits dispatch
//! everywhere else (the casing gate, the router); only the body's reentrant
//! self-call was missed. This path had no snapshot coverage (the shared
//! `14_async_attribute` fixture has no reentrant self-interface call), so these
//! inline-source tests pin the per-backend await against regression.

mod common;
use common::compile_source;

/// `twice()` reentrantly calls the async interface method `add` — the reentrant
/// self-call that emits the transition guard.
const ASYNC_SRC: &str = r#"
@@[async]
@@system Acc {
    interface:
        async add(x: int)
        async twice()
    machine:
        $Active {
            add(x: int) {}
            twice() {
                @@:self.add(1)
            }
    }
}
"#;

/// Same reentrant self-call in a NON-async system — must stay bare.
const SYNC_SRC: &str = r#"
@@system Acc {
    interface:
        add(x: int)
        twice()
    machine:
        $Active {
            add(x: int) {}
            twice() {
                @@:self.add(1)
            }
    }
}
"#;

#[test]
fn reentrant_async_self_call_is_awaited() {
    // Prefix-await backends: the reentrant call carries the await keyword.
    for (target, expected) in [
        ("python_3", "await self.add(1)"),
        ("typescript", "await this.add(1)"),
        ("javascript", "await this.add(1)"),
        ("dart", "await this.add(1)"),
        ("swift", "await self.add(1)"),
        ("csharp", "await this.add(1)"),
        ("cpp", "co_await this->add(1)"),
        // Rust uses a `.await` suffix on the call.
        ("rust", "self.add(1).await"),
    ] {
        let c = compile_source(ASYNC_SRC, target);
        assert!(
            c.contains(expected),
            "[#181/{target}] reentrant async self-interface call not awaited (expected `{expected}`)\n{c}"
        );
    }
}

#[test]
fn reentrant_async_self_call_is_bare_where_no_keyword_is_needed() {
    // Kotlin (bare suspend calls) and Go take no await keyword — the reentrant
    // call stays bare and MUST NOT gain a stray await.
    for (target, bare, awaited) in [
        ("kotlin", "this.add(1)", "await this.add"),
        ("go", "s.Add(1)", "await s.Add"),
    ] {
        let c = compile_source(ASYNC_SRC, target);
        assert!(
            c.contains(bare),
            "[#181/{target}] bare reentrant call missing (expected `{bare}`)\n{c}"
        );
        assert!(
            !c.contains(awaited),
            "[#181/{target}] must not await a bare-suspend/sync reentrant call\n{c}"
        );
    }
}

#[test]
fn reentrant_self_call_not_awaited_when_system_is_sync() {
    // Guard against over-injection: a non-async system must emit the reentrant
    // self-call bare — the await is conditioned on `system_is_async`.
    let c = compile_source(SYNC_SRC, "python_3");
    assert!(
        c.contains("self.add(1)"),
        "[#181] sync reentrant self-call missing\n{c}"
    );
    assert!(
        !c.contains("await self.add(1)"),
        "[#181] a non-async system must NOT await the reentrant self-call\n{c}"
    );
}
