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
    let m = fsm::IdentScan::new(bytes.iter().map(|&b| b as char).collect());
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
    use crate::frame_c::compiler::frame_ast::Span;
    use crate::frame_c::compiler::lexer::lex;
    use crate::frame_c::visitors::TargetLanguage;

    /// The `@@fsm` recognizer + classification produce the same first token and
    /// end offset as the real structural lexer's identifier/keyword path.
    #[test]
    fn fsm_matches_hand_lexer() {
        let corpus = [
            // Plain identifiers.
            "foo",
            "bar_baz",
            "_x",
            "_",
            "x123",
            "ABC",
            "a1_b2",
            // Keywords (word → keyword token).
            "interface",
            "machine",
            "actions",
            "operations",
            "domain",
            "return",
            "true",
            "false",
            "var", // maps to Ident("var")
            // First token only: the rest is a separate token.
            "interface:", // [Interface, SectionColon] — compare Interface
            "foo bar",    // [Ident("foo"), ...] — compare Ident("foo")
            "count123 = 0",
        ];
        for s in corpus {
            let bytes = s.as_bytes();
            let toks =
                lex(bytes, Span::new(0, bytes.len()), TargetLanguage::Python3).expect("lexes");
            assert!(!toks.is_empty(), "no tokens for {:?}", s);
            let hand = &toks[0];

            let (tok, end) =
                scan(bytes).unwrap_or_else(|| panic!("identifier not recognized: {:?}", s));

            assert_eq!(hand.token, tok, "token mismatch on {:?}", s);
            assert_eq!(hand.span.start, 0, "expected start 0 on {:?}", s);
            assert_eq!(hand.span.end, end, "end mismatch on {:?}", s);
        }
    }
}
