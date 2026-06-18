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
    let m = fsm::NumberScan::new(bytes.iter().map(|&b| b as char).collect());
    if !m.accepted {
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
    use crate::frame_c::compiler::frame_ast::Span;
    use crate::frame_c::compiler::lexer::lex;
    use crate::frame_c::visitors::TargetLanguage;

    /// The `@@fsm` recognizer + wrapper produce the same first token and end
    /// offset as the real structural lexer (`lex_number` via `lex`).
    #[test]
    fn fsm_matches_hand_lexer() {
        let corpus = [
            "123", "0", "007", // leading zeros
            "-5",  // signed (dispatched here: `-` before a digit)
            "-0", "123.5", // float
            "0.0", "-3.14",
            "123.",     // dot not followed by digit → int, dot left for next token
            "12.34.56", // float 12.34, rest is separate tokens
            "1.0", "99999999", // large int
            "-0.5",
        ];
        for s in corpus {
            let bytes = s.as_bytes();
            let toks =
                lex(bytes, Span::new(0, bytes.len()), TargetLanguage::Python3).expect("lexes");
            assert!(!toks.is_empty(), "no tokens for {:?}", s);
            let hand = &toks[0];

            let (tok, end) =
                scan(bytes).unwrap_or_else(|| panic!("number not recognized: {:?}", s));

            assert_eq!(hand.token, tok, "token mismatch on {:?}", s);
            assert_eq!(hand.span.start, 0, "expected start 0 on {:?}", s);
            assert_eq!(hand.span.end, end, "end mismatch on {:?}", s);
        }
    }
}
