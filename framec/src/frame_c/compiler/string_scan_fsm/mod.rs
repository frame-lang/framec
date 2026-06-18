//! String lexer, dogfooded as an RFC-0042 `@@fsm` recognizer.
//!
//! [`string_scan.frs`] recognizes the same quoted-string grammar
//! (`"([^"\\]|\\.)*"` or the single-quoted form) as
//! [`super::lexer::Lexer::lex_string`]. This module wraps the generated
//! recognizer to produce the same [`Token`] and end offset, and a differential
//! test proves it agrees with the real lexer.
//!
//! Dogfooding proof-of-concept (the hand lexer stays in production). The open
//! and close quote are the *same* fixed character per branch — two
//! ordered-choice alternatives, since "match whatever opened" would be a
//! backreference (non-regular). The recognizer finds the string extent; the
//! unescaped-content build (drop each backslash, keep the next byte) is
//! transformation and stays in the wrapper.
//!
//! `.gen.rs` regen: edit `string_scan.frs`, run `framec -l rust`, rename to
//! `string_scan.gen.rs`, commit both.

use super::lexer::Token;

mod fsm {
    #![allow(dead_code, unused_parens, non_snake_case, unused_variables, unused_mut)]
    include!("string_scan.gen.rs");
}

/// Recognize a quoted string at the start of `bytes`, returning the same
/// `(Token, end)` that [`super::lexer::Lexer::lex_string`] emits, or `None` if
/// there is no terminated string. `end` is the offset one past the closing
/// quote.
///
/// The recognizer (`@@fsm`) finds the extent; the content is rebuilt exactly as
/// `lex_string` does — each `\` is dropped and the next byte kept literally.
pub fn scan(bytes: &[u8]) -> Option<(Token, usize)> {
    // RFC-0042.1: build over the host's `&[u8]` (zero-copy) and scan from 0.
    let mut m = fsm::StringScan::over(bytes);
    if !m.scan_at(0) {
        return None;
    }
    let end = m.cursor; // one past the closing quote
    let last = end - 1; // index of the closing quote
    let mut content = String::new();
    let mut i = 1; // skip the opening quote
    while i < last {
        if bytes[i] == b'\\' && i + 1 < end {
            content.push(bytes[i + 1] as char);
            i += 2;
        } else {
            content.push(bytes[i] as char);
            i += 1;
        }
    }
    Some((Token::StringLit(content), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `@@fsm` recognizer + wrapper produce the expected unescaped
    /// `StringLit` content and end offset for the quoted-string grammar.
    ///
    /// Direct behavioral assertion, not a differential vs. the lexer: the
    /// production `lex_string` now drives this very recognizer, so a `scan`
    /// vs. `lex` comparison would be circular.
    #[test]
    fn scan_recognizes_string_literals() {
        // (input bytes, expected unescaped content, end offset)
        let cases: &[(&[u8], &str, usize)] = &[
            (br#""abc""#, "abc", 5),
            (br#""""#, "", 2),
            (br#""a b c""#, "a b c", 7),
            (br#""a\"b""#, "a\"b", 6),               // escaped quote
            (br#""a\\b""#, "a\\b", 6),               // escaped backslash
            (br#""line\nbreak""#, "linenbreak", 13), // backslash dropped, `n` kept
            (br#""tab\ttab""#, "tabttab", 10),
            (br#"'x'"#, "x", 3), // single quotes
            (br#"'it\'s'"#, "it's", 7),
            (br#""mixed ' inside""#, "mixed ' inside", 16),
            (br#""trailing"rest"#, "trailing", 10), // ends at the second quote
        ];
        for (bytes, content, end) in cases {
            assert_eq!(
                scan(bytes),
                Some((Token::StringLit((*content).to_string()), *end)),
                "on {:?}",
                std::str::from_utf8(bytes).unwrap()
            );
        }

        // Unterminated / not a string → None.
        assert_eq!(scan(b"\"unterminated"), None);
        assert_eq!(scan(b"abc"), None); // does not start with a quote
        assert_eq!(scan(b""), None);
    }
}
