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
    let m = fsm::StringScan::new(bytes.iter().map(|&b| b as char).collect());
    if !m.accepted {
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
    use crate::frame_c::compiler::frame_ast::Span;
    use crate::frame_c::compiler::lexer::lex;
    use crate::frame_c::visitors::TargetLanguage;

    /// The `@@fsm` recognizer + wrapper produce the same first token and end
    /// offset as the real structural lexer (`lex_string` via `lex`).
    #[test]
    fn fsm_matches_hand_lexer() {
        let corpus = [
            r#""abc""#,         // basic
            r#""""#,            // empty
            r#""a b c""#,       // spaces
            r#""a\"b""#,        // escaped quote
            r#""a\\b""#,        // escaped backslash
            r#""line\nbreak""#, // \n is dropped-backslash → "linenbreak"
            r#""tab\ttab""#,
            r#"'x'"#,     // single quotes
            r#"'it\'s'"#, // escaped single quote
            r#""mixed ' inside""#,
            r#""trailing"rest"#, // string ends at second quote; `rest` is separate
        ];
        for s in corpus {
            let bytes = s.as_bytes();
            let toks =
                lex(bytes, Span::new(0, bytes.len()), TargetLanguage::Python3).expect("lexes");
            assert!(!toks.is_empty(), "no tokens for {:?}", s);
            let hand = &toks[0];

            let (tok, end) =
                scan(bytes).unwrap_or_else(|| panic!("string not recognized: {:?}", s));

            assert_eq!(hand.token, tok, "token mismatch on {:?}", s);
            assert_eq!(hand.span.start, 0, "expected start 0 on {:?}", s);
            assert_eq!(hand.span.end, end, "end mismatch on {:?}", s);
        }
    }
}
