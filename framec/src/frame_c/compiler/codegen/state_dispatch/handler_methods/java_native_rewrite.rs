//! Per-handler Java native-code rewriter.
//!
//! Frame source uses `self.X` for the receiver and `await EXPR` for
//! awaiting a future. Java has neither token — it's `this.X` and
//! `EXPR.join()` for `CompletableFuture<T>`. The Oceans Model's
//! passthrough emission leaves these as literal text in the handler
//! body, which doesn't compile.
//!
//! This module post-processes the per-handler body produced by
//! `emit_handler_body_via_statements` and applies two rewrites,
//! both respecting Java string-literal and comment boundaries:
//!
//! 1. `self.` → `this.` — straight token swap.
//! 2. `await EXPR;` → `EXPR.join();` — where EXPR is a single
//!    balanced primary (typically `op(args)` or `self.method(args)`).
//!    The `.join()` form is the unchecked-exception variant of
//!    `.get()` and matches what the Java fuzz driver already does
//!    on the interface boundary (e.g. `s.fetch("k").join()`).
//!
//! Why not regex? The replacement must skip string literals and
//! comments. Using `replace_outside_strings_and_comments` for the
//! literal-token swap and a hand-written balanced-paren scan for
//! `await` keeps the implementation correct in the presence of
//! both. Same shape as `erlang_system/native_rewrite.rs`.

use crate::frame_c::compiler::codegen::codegen_utils::replace_outside_strings_and_comments;
use crate::frame_c::visitors::TargetLanguage;

/// Apply the Frame → Java handler-body lowerings:
/// - `self.` → `this.`
/// - `await EXPR;` → `EXPR.join();` (EXPR is a balanced primary)
///
/// Idempotent: applying twice produces the same output as once.
pub(super) fn rewrite_java_handler_body(body: &str) -> String {
    // Pass 1: `self.` → `this.`. Boundary-safe via the shared helper.
    let after_self = replace_outside_strings_and_comments(
        body,
        TargetLanguage::Java,
        &[("self.", "this.")],
    );

    // Pass 2: `await EXPR;` → `EXPR.join();`. Hand-rolled scan so
    // the EXPR's parens balance correctly and we don't trip over
    // commas, semicolons inside argument lists, etc.
    rewrite_await(&after_self)
}

/// Walk `body` and rewrite occurrences of `await <EXPR>` to
/// `<EXPR>.join()`, where `<EXPR>` is a single identifier-rooted
/// primary expression with optional balanced parenthesized
/// argument list. Skips characters inside string literals and
/// comments using the same skipper the boundary-safe replacer
/// uses.
fn rewrite_await(body: &str) -> String {
    let lang = TargetLanguage::Java;
    let skipper = crate::frame_c::compiler::native_region_scanner::create_skipper(lang);
    let bytes = body.as_bytes();
    let end = bytes.len();
    let mut out = String::with_capacity(body.len() + 32);
    let mut i = 0;
    while i < end {
        if let Some(next) = skipper.skip_string(bytes, i, end) {
            out.push_str(&body[i..next]);
            i = next;
            continue;
        }
        if let Some(next) = skipper.skip_comment(bytes, i, end) {
            out.push_str(&body[i..next]);
            i = next;
            continue;
        }
        // Match `await ` only at a token boundary (preceded by start
        // or non-word char) so identifiers like `awaiting_x` don't
        // accidentally fire.
        let at_boundary = i == 0
            || matches!(
                bytes[i - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'(' | b',' | b';' | b'='
            );
        if at_boundary && body[i..].starts_with("await ") {
            // Skip the `await ` literal (6 bytes) + any extra spaces.
            let mut j = i + 6;
            while j < end && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Capture the expression: an identifier, optional dotted
            // suffixes, optional balanced `(...)` argument list.
            let expr_start = j;
            // Identifier head: [A-Za-z_][A-Za-z0-9_]*
            if j < end && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                j += 1;
                while j < end
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'_'
                        || bytes[j] == b'.')
                {
                    j += 1;
                }
            }
            // Optional balanced `(...)` — handles nested parens and
            // string-literal content inside arguments.
            if j < end && bytes[j] == b'(' {
                let mut depth: i32 = 0;
                while j < end {
                    if let Some(next) = skipper.skip_string(bytes, j, end) {
                        j = next;
                        continue;
                    }
                    match bytes[j] {
                        b'(' => {
                            depth += 1;
                            j += 1;
                        }
                        b')' => {
                            depth -= 1;
                            j += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => j += 1,
                    }
                }
            }
            // Emit `<EXPR>.join()` and advance past the captured slice.
            // If we couldn't actually find an identifier, fall through
            // and emit the bytes verbatim.
            if j > expr_start {
                out.push_str(&body[expr_start..j]);
                out.push_str(".join()");
                i = j;
                continue;
            }
        }
        // Plain character — copy through one byte at a time. Handler
        // bodies are emitted text, so multi-byte UTF-8 sequences are
        // rare; the `replace_outside_strings_and_comments` helper
        // does width-aware copying, but here single-byte advance is
        // safe because we only branch on ASCII keywords/operators.
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_dot_becomes_this_dot() {
        let got = rewrite_java_handler_body("self.tmp_a = 1;");
        assert_eq!(got, "this.tmp_a = 1;");
    }

    #[test]
    fn await_op_becomes_op_join() {
        let got = rewrite_java_handler_body("self.tmp_a = await op(\"init\");");
        assert_eq!(got, "this.tmp_a = op(\"init\").join();");
    }

    #[test]
    fn await_with_self_method() {
        let got = rewrite_java_handler_body("x = await self.fetch(key);");
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
        let once = rewrite_java_handler_body("self.x = await op(key);");
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
