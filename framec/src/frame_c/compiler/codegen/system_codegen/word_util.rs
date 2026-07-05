//! Whole-word matching utilities used by domain-initializer
//! analysis.
//!
//! These helpers exist because Frame's V4 codegen needs to answer two
//! questions about user-written initializer text without parsing it:
//!
//! 1. **Does the initializer reference a constructor parameter?**
//!    `balance: int = balance` is a self-reference; we have to detect
//!    the rebind to avoid emitting a name-collision at init.
//! 2. **Which identifiers in a PHP initializer are param names that
//!    need a `$` sigil?** PHP's lexical rules require `$` on every
//!    variable read, so domain initializers written in language-
//!    agnostic Frame text have to be rewritten on the PHP path.
//!
//! Both questions reduce to "find every whole-word occurrence of
//! `name` in this string". The `extra_leading` parameter to
//! `is_whole_word_at` lets the PHP path treat `$` as already-sigiled
//! (so `$balance` doesn't double-prefix) and lets the param-detection
//! path treat `.` as a valid leading boundary (so `obj.balance`
//! doesn't trigger on `balance`).

/// Check if a word appears at a whole-word boundary in text.
/// Leading boundary excludes alphanumeric, underscore, and any chars
/// in `extra_leading`. Trailing boundary excludes alphanumeric and
/// underscore.
pub(super) fn is_whole_word_at(
    bytes: &[u8],
    start: usize,
    end: usize,
    extra_leading: &[u8],
) -> bool {
    let prev_ok = start == 0 || {
        let b = bytes[start - 1];
        !(b.is_ascii_alphanumeric() || b == b'_' || extra_leading.contains(&b))
    };
    let next_ok = end >= bytes.len() || {
        let b = bytes[end];
        !(b.is_ascii_alphanumeric() || b == b'_')
    };
    prev_ok && next_ok
}

/// True iff the init expression text references any of the supplied param
/// names as a whole word **in code** — string literals and comments are
/// skipped. Used to detect `balance: int = balance` (a domain field seeded
/// from a constructor parameter) so the assignment moves to the factory.
///
/// #123: skipping string literals is load-bearing, not cosmetic. A constant
/// init like `note: string = "rate limited"` must NOT be treated as
/// param-referencing just because its *text* contains the word `rate` — the
/// old string-blind scan silently dropped such constants from the bare
/// (`@@!Sys()`) constructor. The scan uses the shared per-language skipper so
/// the same string/comment rules the backend emits under are respected.
pub(crate) fn init_references_param(
    init_text: &str,
    params: &[String],
    lang: crate::frame_c::visitors::TargetLanguage,
) -> bool {
    if params.is_empty() || init_text.is_empty() {
        return false;
    }
    let bytes = init_text.as_bytes();
    for p in params {
        if p.is_empty() {
            continue;
        }
        // Walk every code-region occurrence of the param name and accept the
        // first that sits on whole-word boundaries (`.` allowed as a leading
        // boundary so `obj.count` does not match param `count`, but
        // `count.toString()` does).
        let mut from = 0;
        while let Some(pos) =
            crate::frame_c::compiler::codegen::codegen_utils::find_outside_strings_and_comments_from(
                init_text, lang, p, from,
            )
        {
            let end = pos + p.len();
            if is_whole_word_at(bytes, pos, end, b".") {
                return true;
            }
            from = end;
        }
    }
    false
}

/// True iff a PHP class-property default CANNOT legally hold this domain
/// initializer, so it must be assigned in the constructor body instead (#144).
///
/// PHP property defaults admit only *constant expressions*; `new X(...)`, a
/// function/method call, or a `@@<System>()` instantiation are all rejected at
/// parse time ("New expressions are not supported in this context").
///
/// #153: delegates to the shared, scanner-based
/// [`codegen_utils::init_is_runtime_expression`] token classifier — replacing
/// the previous `.contains("@@")||.contains("new ")||.contains('(')` substring
/// heuristic (which mis-flagged parenthesised constants like `(1 + 2)` and
/// needed a special whole-string-literal case). The classifier skips string
/// literals/comments and detects `new`/calls/`@@` at token boundaries, so
/// `"foo(bar)"` is constant while `f(1,2)`, `new X()`, and `"a" . new Y()` are
/// runtime. Same predicate every backend's field-default decision can use.
pub(crate) fn php_init_needs_constructor(init_text: &str) -> bool {
    crate::frame_c::compiler::codegen::codegen_utils::init_is_runtime_expression(
        init_text,
        crate::frame_c::visitors::TargetLanguage::Php,
    )
}

/// Prefix `$` to identifiers in `text` that match system param names.
/// Used for PHP domain initializer expressions (e.g.
/// `initial_balance` → `$initial_balance`).
pub(super) fn prefix_php_vars(text: &str, params: &[String]) -> String {
    let mut result = text.to_string();
    for p in params {
        if p.is_empty() {
            continue;
        }
        let mut new_result = String::new();
        let bytes = result.as_bytes();
        let pb = p.as_bytes();
        let mut i = 0usize;
        while i + pb.len() <= bytes.len() {
            if let Some(found) = bytes[i..].windows(pb.len()).position(|w| w == pb) {
                let start = i + found;
                let end = start + pb.len();
                new_result.push_str(&result[i..start]);
                if is_whole_word_at(bytes, start, end, b"$") {
                    new_result.push('$');
                }
                new_result.push_str(p);
                i = end;
            } else {
                new_result.push_str(&result[i..]);
                i = bytes.len();
            }
        }
        if i < result.len() {
            new_result.push_str(&result[i..]);
        }
        result = new_result;
    }
    result
}

#[cfg(test)]
mod php_const_tests {
    use super::php_init_needs_constructor;

    #[test]
    fn constants_stay_property_defaults() {
        for c in [
            "5",
            "-3",
            "3.14",
            "true",
            "false",
            "null",
            "[]",
            "[1, 2, 3]",
        ] {
            assert!(
                !php_init_needs_constructor(c),
                "constant `{c}` should stay a property default"
            );
        }
    }

    #[test]
    fn string_literals_are_constant_even_with_parens_or_new() {
        assert!(!php_init_needs_constructor("\"foo(bar)\""));
        assert!(!php_init_needs_constructor("'has new inside'"));
        assert!(!php_init_needs_constructor("\"@@notatag\""));
    }

    #[test]
    fn new_calls_and_tags_need_the_constructor() {
        assert!(php_init_needs_constructor("new Pt(3)"));
        assert!(php_init_needs_constructor("@@Sensor()"));
        assert!(php_init_needs_constructor("make_thing()"));
        assert!(php_init_needs_constructor("Vec2(640, 480)"));
    }

    #[test]
    fn empty_init_is_not_deferred() {
        assert!(!php_init_needs_constructor(""));
        assert!(!php_init_needs_constructor("   "));
    }

    #[test]
    fn quote_spanning_concat_is_not_a_constant_literal() {
        // Audit regression: a concat that merely starts+ends with a quote but
        // hides a `new`/call in the middle must go to the constructor, not stay
        // a property default (which would emit un-compilable PHP).
        assert!(php_init_needs_constructor("\"x\" . new Y() . \"z\""));
        assert!(php_init_needs_constructor("'a' . foo() . 'b'"));
        assert!(php_init_needs_constructor("\"pre\" . @@Sensor()"));
    }

    #[test]
    fn single_literal_with_embedded_specials_stays_constant() {
        assert!(!php_init_needs_constructor("\"foo(bar)\""));
        assert!(!php_init_needs_constructor("\"has new inside\""));
        assert!(!php_init_needs_constructor("'@@notatag'"));
        assert!(!php_init_needs_constructor("\"escaped \\\" quote (ok)\""));
    }
}
