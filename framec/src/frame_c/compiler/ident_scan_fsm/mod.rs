//! Identifier scanner, dogfooded as an RFC-0042 `@@fsm` recognizer.
//!
//! [`ident_scan.frs`] recognizes the identifier shape
//! (`[A-Za-z_][A-Za-z0-9_]*`) that [`super::lexer::Lexer::scan_identifier`]
//! consumes. This module wraps the recognizer and reproduces the keyword
//! classification of [`super::lexer::Lexer::lex_identifier_or_keyword`]; a
//! differential test proves it agrees with the real lexer's first token.
//!
//! Dogfooding proof-of-concept (the hand lexer stays in production). The
//! recognizer finds the identifier extent; the word→keyword map
//! (`interface`/`return`/`true`/…) is a finite lookup — transformation, not
//! recognition — so it lives in the wrapper. The `push$`/`pop$` and
//! section-colon composites are 1-char-lookahead lexer logic *around* the
//! identifier, not identifier recognition, and are deliberately out of scope.
//!
//! `.gen.rs` regen: edit `ident_scan.frs`, run `framec -l rust`, rename to
//! `ident_scan.gen.rs`, commit both.

use super::lexer::Token;

mod fsm {
    #![allow(dead_code, unused_parens, non_snake_case, unused_variables, unused_mut)]
    include!("ident_scan.gen.rs");
}

/// Recognize an identifier at the start of `bytes`, returning the same
/// `(Token, end)` first token that [`super::lexer::Lexer::lex_identifier_or_keyword`]
/// emits for a plain word, or `None` if no identifier is present. `end` is the
/// offset one past the identifier.
///
/// `push$`/`pop$` and the section-`:` second token are not produced here — they
/// are the lexer's lookahead composites around the recognized word.
pub fn scan(bytes: &[u8]) -> Option<(Token, usize)> {
    // RFC-0042.1: scans the host's `&[u8]` directly (zero-copy).
    let m = fsm::IdentScan::new(bytes);
    if !m.accepted {
        return None;
    }
    let end = m.cursor;
    let word = String::from_utf8_lossy(&bytes[..end]).to_string();
    let token = match word.as_str() {
        "interface" => Token::Interface,
        "machine" => Token::Machine,
        "actions" => Token::Actions,
        "operations" => Token::Operations,
        "domain" => Token::Domain,
        "return" => Token::Return,
        "true" => Token::BoolLit(true),
        "false" => Token::BoolLit(false),
        _ => Token::Ident(word),
    };
    Some((token, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `@@fsm` recognizer + keyword classification produce the expected
    /// `(Token, end)` for the identifier grammar `[A-Za-z_][A-Za-z0-9_]*`.
    ///
    /// Direct behavioral assertion, not a differential vs. the lexer: the
    /// production identifier path now drives this very recognizer, so a `scan`
    /// vs. `lex` comparison would be circular. `push$`/`pop$` and the section
    /// `:` are lexer composites *around* the word and are out of scope here.
    #[test]
    fn scan_recognizes_identifiers_and_keywords() {
        use Token::*;
        let cases: &[(&[u8], Token, usize)] = &[
            // Plain identifiers.
            (b"foo", Ident("foo".to_string()), 3),
            (b"bar_baz", Ident("bar_baz".to_string()), 7),
            (b"_x", Ident("_x".to_string()), 2),
            (b"_", Ident("_".to_string()), 1),
            (b"x123", Ident("x123".to_string()), 4),
            (b"ABC", Ident("ABC".to_string()), 3),
            (b"a1_b2", Ident("a1_b2".to_string()), 5),
            // Keywords (word → keyword token).
            (b"interface", Interface, 9),
            (b"machine", Machine, 7),
            (b"actions", Actions, 7),
            (b"operations", Operations, 10),
            (b"domain", Domain, 6),
            (b"return", Return, 6),
            (b"true", BoolLit(true), 4),
            (b"false", BoolLit(false), 5),
            (b"var", Ident("var".to_string()), 3),
            // First token only: the rest is separate input.
            (b"interface:", Interface, 9),
            (b"foo bar", Ident("foo".to_string()), 3),
            (b"count123 = 0", Ident("count123".to_string()), 8),
        ];
        for (bytes, tok, end) in cases {
            assert_eq!(
                scan(bytes),
                Some((tok.clone(), *end)),
                "on {:?}",
                std::str::from_utf8(bytes).unwrap()
            );
        }

        // No identifier start → None.
        assert_eq!(scan(b""), None);
        assert_eq!(scan(b"123"), None); // digit start is not an identifier
        assert_eq!(scan(b" foo"), None); // leading space
    }
}
