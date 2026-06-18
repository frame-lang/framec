//! Body closer for `@@fsm` blocks (RFC-0042).
//!
//! An `@@fsm` body is Frame content, not target-language code, so the
//! per-language body closers — which apply that language's string/comment
//! rules — mis-handle the construct unique to `@@fsm`: the `/.../` **regex
//! literal**, whose `{`, `}`, `"`, `'` are regex content, not structural
//! delimiters. Counting those breaks brace matching and the block fails to
//! close (framec#103: a string-scanner regex `/"([^"\\]|\\.)*"/` has an odd
//! number of `"`, and `/ab}cd/` carries a literal `}`).
//!
//! This closer tracks brace depth and skips — in any context — line/block
//! comments and string literals, and, **at the fsm body's own brace level**,
//! `/.../` regex literals (to the next unescaped `/`, honoring `\/` exactly as
//! the regex lexer does). Inside deeper `{ }` action blocks the context is
//! expression-level, where `/` is the division operator and is an ordinary
//! byte. This mirrors the fsm lexer's element-vs-expression `/`
//! disambiguation, scoped to what brace matching needs.
//!
//! Limitation: a `/` used as **division at the body's own brace level** — in a
//! bare-expression match tail or a `when` guard, outside any action block — is
//! read as a regex delimiter. No fixture relies on this; wrap such arithmetic
//! in an action block or a declared `actions:` helper.

use super::{BodyCloser, CloseError, CloseErrorKind};

/// Finds the matching `}` of an `@@fsm` block, regex-literal aware.
pub struct FsmBodyCloser;

impl BodyCloser for FsmBodyCloser {
    /// `open_brace_index` is the absolute offset of the body-opening `{` in
    /// `bytes`; returns the absolute offset of the matching `}`.
    fn close_byte(&mut self, bytes: &[u8], open_brace_index: usize) -> Result<usize, CloseError> {
        let n = bytes.len();
        let mut i = open_brace_index;
        let mut depth: i32 = 0;
        while i < n {
            match bytes[i] {
                b'{' => {
                    depth += 1;
                    i += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(i);
                    }
                    i += 1;
                }
                // Line / block comments — any depth, checked before the
                // regex-delimiter case so `//` and `/*` are never regexes.
                b'/' if i + 1 < n && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < n && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < n && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    i = (i + 2).min(n);
                }
                // Regex literal at the body level: opaque to the next
                // unescaped `/`. Inner braces and quotes are regex content.
                b'/' if depth == 1 => {
                    i += 1;
                    while i < n {
                        if bytes[i] == b'\\' && i + 1 < n {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'/' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
                // String / char literal (action-body or expression) — any
                // depth. Skip to the matching unescaped quote.
                b'"' | b'\'' => {
                    let q = bytes[i];
                    i += 1;
                    while i < n {
                        if bytes[i] == b'\\' && i + 1 < n {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == q {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        Err(CloseError {
            kind: CloseErrorKind::UnmatchedBraces,
            message: "Unterminated @@fsm block".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(src: &str) -> Option<usize> {
        let bytes = src.as_bytes();
        let open = bytes.iter().position(|&b| b == b'{').unwrap();
        FsmBodyCloser.close_byte(bytes, open).ok()
    }

    /// The close brace is found past regex literals containing braces, quotes,
    /// and odd quote counts — the framec#103 cases — and past action blocks
    /// with their own braces / strings / division.
    #[test]
    fn closes_past_regex_literals() {
        // Literal `}` inside a regex must not close the block.
        let s = "@@fsm T(src: bytes) : bool = false { /ab}cd/ true }";
        assert_eq!(close(s), Some(s.rfind('}').unwrap()));

        // A string-scanner regex: three `"` (odd) plus a literal that the
        // language closers would mis-pair.
        let s = r#"@@fsm T(src: bytes) : bool = false { /"([^"\\]|\\.)*"/ true }"#;
        assert_eq!(close(s), Some(s.rfind('}').unwrap()));

        // Brace quantifiers stay opaque too.
        let s = "@@fsm T(src: bytes) : bool = false { /[0-9]{2,4}/ true }";
        assert_eq!(close(s), Some(s.rfind('}').unwrap()));

        // Action block with its own braces, a string, and division.
        let s = "@@fsm T(src: bytes) : int = 0 { /[0-9]/ { self.n = self.n / 2 } self.n }";
        assert_eq!(close(s), Some(s.rfind('}').unwrap()));

        // Labeled stage (identifier before the regex `/`).
        let s = "@@fsm T(src: bytes) : bool = false { .a/[0-9]/ .b/[a-z]/ true }";
        assert_eq!(close(s), Some(s.rfind('}').unwrap()));
    }

    /// An unterminated block reports an error rather than running off the end.
    #[test]
    fn unterminated_is_error() {
        let s = "@@fsm T(src: bytes) : bool = false { /a/ true";
        assert!(close(s).is_none());
    }
}
