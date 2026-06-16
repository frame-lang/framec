//! Per-handler Java native-code rewriter.
//!
//! Frame source uses `self.X` for the receiver and `await EXPR`
//! for awaiting a future. Java has neither token — it's `this.X`
//! and `EXPR.join()` for `CompletableFuture<T>`. The Oceans
//! Model's passthrough emission leaves these as literal text in
//! the handler body, which doesn't compile.
//!
//! This module post-processes the per-handler body produced by
//! `emit_handler_body_via_statements` and applies two rewrites,
//! both respecting Java string-literal and comment boundaries:
//!
//! 1. `self.` → `this.` — token swap via the shared
//!    `replace_outside_strings_and_comments` helper (which uses
//!    the per-target skipper trait — already an FSM under the
//!    hood).
//! 2. `await EXPR;` → `EXPR.join();` — dogfooded state machine.
//!    Source: `java_await_rewrite.frs`. See RFC-0035 for the
//!    dogfood-pattern rationale.
//!
//! To regenerate the await FSM:
//!   ./target/release/framec compile -l rust \
//!     -o framec/src/frame_c/compiler/codegen/state_dispatch/handler_methods/ \
//!     framec/src/frame_c/compiler/codegen/state_dispatch/handler_methods/java_await_rewrite.frs
//!   mv java_await_rewrite.rs java_await_rewrite.gen.rs

#![allow(unreachable_patterns)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

include!("java_await_rewrite.gen.rs");

/// Apply the Frame → Java handler-body lowering:
/// - `await EXPR;` → `EXPR.join();` (EXPR is a balanced primary)
///
/// RFC-0046: the former `self.` → `this.` pass was removed. Frame's
/// self-reference is `@@:self`, which the expansion lowers to `this.`
/// directly; a bare native `self.` is now ordinary native code, passed
/// through verbatim (it is the author's error on Java, which has no
/// `self`). framec no longer rewrites native code here.
///
/// Idempotent: applying twice produces the same output as once.
pub(super) fn rewrite_java_handler_body(body: &str) -> String {
    // `await EXPR;` → `EXPR.join();` via the dogfooded FSM.
    rewrite_await(body)
}

/// Run the `await`-rewriter FSM over `body`. Returns the rewritten
/// string. The FSM walks bytes, handles `//` and `/* */` comments
/// and `"..."` strings as passthrough, and rewrites `await EXPR`
/// (balanced-paren-aware) to `EXPR.join()`.
fn rewrite_await(body: &str) -> String {
    let mut fsm = JavaAwaitRewriteFsm::new();
    fsm.bytes = body.as_bytes().to_vec();
    fsm.pos = 0;
    fsm.result = String::with_capacity(body.len() + 32);
    fsm.rewrite();
    fsm.result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_self_is_not_rewritten() {
        // RFC-0046: native `self.` is passthrough now (no `self.`→`this.`).
        let got = rewrite_java_handler_body("self.tmp_a = 1;");
        assert_eq!(got, "self.tmp_a = 1;");
    }

    #[test]
    fn await_op_becomes_op_join() {
        let got = rewrite_java_handler_body("this.tmp_a = await op(\"init\");");
        assert_eq!(got, "this.tmp_a = op(\"init\").join();");
    }

    #[test]
    fn await_with_this_method() {
        let got = rewrite_java_handler_body("x = await this.fetch(key);");
        assert_eq!(got, "x = this.fetch(key).join();");
    }

    #[test]
    fn does_not_touch_strings() {
        let got = rewrite_java_handler_body("String s = \"self.x = await foo();\";");
        assert_eq!(got, "String s = \"self.x = await foo();\";");
    }

    #[test]
    fn does_not_touch_comments() {
        let got = rewrite_java_handler_body("// self.x = await foo();\nthis.y = 1;");
        assert_eq!(got, "// self.x = await foo();\nthis.y = 1;");
    }

    #[test]
    fn idempotent() {
        let once = rewrite_java_handler_body("this.x = await op(key);");
        let twice = rewrite_java_handler_body(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn does_not_touch_awaiting_identifier() {
        // `await ` only matches at token boundary; `awaiting_x` should
        // not be confused with a leading `await ` keyword.
        let got = rewrite_java_handler_body("int awaiting_x = 1;");
        assert_eq!(got, "int awaiting_x = 1;");
    }
}
