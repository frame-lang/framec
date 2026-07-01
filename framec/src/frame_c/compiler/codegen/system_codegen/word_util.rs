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

/// Find all whole-word occurrences of `word` in `text`, calling
/// `callback` for each. The callback receives `(start, end)` byte
/// positions and returns `true` to continue, `false` to stop.
pub(super) fn find_whole_words(
    text: &[u8],
    word: &[u8],
    extra_leading: &[u8],
    mut callback: impl FnMut(usize, usize) -> bool,
) {
    let mut i = 0;
    while i + word.len() <= text.len() {
        if let Some(found) = text[i..].windows(word.len()).position(|w| w == word) {
            let start = i + found;
            let end = start + word.len();
            if is_whole_word_at(text, start, end, extra_leading) {
                if !callback(start, end) {
                    return;
                }
            }
            i = end;
        } else {
            break;
        }
    }
}

/// True iff the init expression text contains any of the supplied
/// param names as a whole word. Used to detect `balance: int = balance`
/// where a domain field initializer references a constructor
/// parameter.
pub(crate) fn init_references_param(init_text: &str, params: &[String]) -> bool {
    if params.is_empty() || init_text.is_empty() {
        return false;
    }
    let bytes = init_text.as_bytes();
    for p in params {
        if p.is_empty() {
            continue;
        }
        let mut found = false;
        find_whole_words(bytes, p.as_bytes(), b".", |_, _| {
            found = true;
            false
        });
        if found {
            return true;
        }
    }
    false
}

/// True iff a PHP class-property default CANNOT legally hold this domain
/// initializer, so it must be assigned in the constructor body instead (#144).
///
/// PHP property defaults admit only *constant expressions*; `new X(...)`, a
/// function/method call, or a `@@<System>()` instantiation are all rejected at
/// parse time ("New expressions are not supported in this context"). We treat
/// the init as non-constant — needing the constructor — when its text contains
/// a `@@` tag, the `new` keyword, or a call paren `(`. A *single* string literal
/// is always constant even if it embeds those characters, so it stays a property
/// default. This is deliberately conservative: a constant expression we can't
/// prove (e.g. parenthesised arithmetic) is still safe to assign in the
/// constructor, just slightly less idiomatic.
///
/// NOTE: this is a substring heuristic, not a real constant-expression
/// classifier — see the pre-release hack/heuristic audit. The proper technique
/// is a token-based `is-constant-expression` predicate over the initializer,
/// shared across the backends that each answer "may this init be a field
/// default?" (Go/C strip unconditionally, OO on param-collision, C++ member-init
/// list, PHP here). Tracked for conversion.
pub(crate) fn php_init_needs_constructor(init_text: &str) -> bool {
    let t = init_text.trim();
    if t.is_empty() {
        return false;
    }
    // A *single* string literal is a constant expression regardless of what it
    // embeds. Crucially this must be ONE literal, not a quote-spanning
    // concatenation like `"x" . new Y() . "z"` — which also starts and ends with
    // a quote but is NOT constant. `is_single_string_literal` rejects an interior
    // (unescaped) closing quote so such a concat falls through to the checks
    // below and is correctly assigned in the constructor.
    if is_single_string_literal(t) {
        return false;
    }
    t.contains("@@") || t.contains("new ") || t.contains('(')
}

/// True iff `t` is exactly one string literal (`"…"` or `'…'`) with no interior
/// unescaped quote of the same kind — i.e. the opening quote's match is the
/// final character. Distinguishes a constant literal (`"foo(bar)"`) from a
/// concatenation that merely starts and ends with a quote (`"x" . f() . "y"`).
fn is_single_string_literal(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() < 2 {
        return false;
    }
    let q = b[0];
    if (q != b'"' && q != b'\'') || b[b.len() - 1] != q {
        return false;
    }
    // Walk the interior; the same-kind quote must not appear unescaped before
    // the final character (that would mean the literal closed early → concat).
    let mut i = 1;
    while i < b.len() - 1 {
        if b[i] == b'\\' {
            i += 2; // skip the escaped byte
            continue;
        }
        if b[i] == q {
            return false; // interior close → not a single literal
        }
        i += 1;
    }
    true
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
