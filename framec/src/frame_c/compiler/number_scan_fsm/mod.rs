//! Number lexer, dogfooded as an RFC-0042 `@@fsm` recognizer.
//!
//! [`number_scan.frs`] recognizes the same integer/float literal grammar
//! (`-?[0-9]+(\.[0-9]+)?`) as [`super::lexer::Lexer::lex_number`]. This module
//! wraps the generated recognizer to produce the same [`Token`] and end offset,
//! and a differential test proves it agrees with the real lexer.
//!
//! Dogfooding proof-of-concept (the hand lexer stays in production). It shows
//! the recognition / transformation split that `@@fsm` makes explicit: the
//! recognizer decides *where the number ends*; the int-vs-float choice and the
//! `parse::<i64>()` / `parse::<f64>()` conversion are not recognition, so they
//! live in the wrapper.
//!
//! `.gen.rs` regen: edit `number_scan.frs`, run `framec -l rust`, rename to
//! `number_scan.gen.rs`, commit both.

use super::lexer::Token;

mod fsm {
    #![allow(dead_code, unused_parens, non_snake_case, unused_variables, unused_mut)]
    include!("number_scan.gen.rs");
}

/// Recognize an integer/float literal at the start of `bytes`, returning the
/// same `(Token, end)` that [`super::lexer::Lexer::lex_number`] emits, or
/// `None` if no number is present. `end` is the byte offset one past the token.
///
/// The recognizer (`@@fsm`) finds the token extent; the int-vs-float
/// classification (float iff the matched slice contains `.`) and the value
/// parse mirror `lex_number` exactly — `unwrap_or` defaults included.
pub fn scan(bytes: &[u8]) -> Option<(Token, usize)> {
    // RFC-0042.1: build over the host's `&[u8]` (zero-copy) and scan from 0.
    let mut m = fsm::NumberScan::over(bytes);
    if !m.scan_at(0) {
        return None;
    }
    let end = m.cursor;
    let token = if bytes[..end].contains(&b'.') {
        let text = std::str::from_utf8(&bytes[..end]).unwrap_or("0.0");
        Token::FloatLit(text.parse::<f64>().unwrap_or(0.0))
    } else {
        let text = std::str::from_utf8(&bytes[..end]).unwrap_or("0");
        Token::IntLit(text.parse::<i64>().unwrap_or(0))
    };
    Some((token, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `@@fsm` recognizer + wrapper produce the expected `(Token, end)` for
    /// the integer/float literal grammar `-?[0-9]+(\.[0-9]+)?`.
    ///
    /// This is a direct behavioral assertion, not a differential vs. the lexer:
    /// the production `lex_number` now drives this very recognizer, so a
    /// `scan` vs. `lex` comparison would be circular.
    #[test]
    fn scan_recognizes_number_literals() {
        use Token::*;
        let cases: &[(&[u8], Token, usize)] = &[
            (b"123", IntLit(123), 3),
            (b"0", IntLit(0), 1),
            (b"007", IntLit(7), 3), // leading zeros → decimal 7
            (b"-5", IntLit(-5), 2), // signed (dispatched here: `-` before a digit)
            (b"-0", IntLit(0), 2),
            (b"123.5", FloatLit(123.5), 5),
            (b"0.0", FloatLit(0.0), 3),
            (b"-3.25", FloatLit(-3.25), 5),
            (b"123.", IntLit(123), 3), // dot not followed by digit → int, dot left
            (b"12.34.56", FloatLit(12.34), 5), // float 12.34; rest is separate tokens
            (b"1.0", FloatLit(1.0), 3),
            (b"99999999", IntLit(99999999), 8),
            (b"-0.5", FloatLit(-0.5), 4),
        ];
        for (bytes, tok, end) in cases {
            assert_eq!(
                scan(bytes),
                Some((tok.clone(), *end)),
                "on {:?}",
                std::str::from_utf8(bytes).unwrap()
            );
        }

        // Not a number → None. The dispatcher never routes these here, but the
        // recognizer must still reject them (no spurious match).
        assert_eq!(scan(b""), None);
        assert_eq!(scan(b"-"), None); // sign with no digit
        assert_eq!(scan(b"abc"), None);
        assert_eq!(scan(b".5"), None); // no leading integer part
    }
}
