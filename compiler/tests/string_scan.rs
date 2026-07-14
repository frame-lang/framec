//! **The first dogfooded scanner agrees with the hand lexer — proven by running.**
//!
//! `string_scan::scan` is generated from `string_scan.frs`, a `@@[scan(u8)]` Frame system
//! (the resolution of the fubar in `docs/JOURNAL.md`). This test proves the generated
//! machine computes the SAME quoted-string extent as the hand-written
//! [`Lexer::quoted`] — for `delim = '"'`, single-line, escapes on — at **every** position
//! of a corpus, including the string-blindness cases (a `}` inside a string, an escaped
//! quote, a bare newline, an unterminated tail). If the machine and the hand loop ever
//! disagree, this fails.
//!
//! The reference is `Target::Java` (`Quoted { '"', multiline:false, escapes:true }`, and no
//! interpolation) — exactly StringScan's grammar.

use frame_compiler::text::scan::lex::Lexer;
use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::string_scan;

/// The hand lexer's quoted extent at `i`, as an `Option<end>`: `Some` on a terminated
/// string, `None` on "no string here" or "unterminated" (which the machine reports as
/// reject).
fn hand(bytes: &[u8], i: usize) -> Option<usize> {
    let lx = Lexer::new(bytes, Target::Java);
    match lx.literal_at(i) {
        Ok(Some(ext)) => Some(ext.span.end),
        _ => None, // Ok(None) = not a literal here; Err = unterminated
    }
}

/// Assert the machine and the hand lexer agree at every `"`-started position.
fn agree(src: &str) {
    let bytes = src.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'"' {
            continue;
        }
        let machine = string_scan::scan(bytes, i);
        let reference = hand(bytes, i);
        assert_eq!(
            machine, reference,
            "disagreement at byte {i} of {src:?}: machine={machine:?} hand={reference:?}"
        );
    }
}

#[test]
fn plain_strings_agree() {
    agree(r#"let a = "hello"; let b = "world";"#);
}

#[test]
fn the_string_blindness_cases_agree() {
    // A brace inside a string must NOT be seen as code — the whole reason the mode has to
    // be a state, not a native `in_string` byte.
    agree(r#"x = "a } brace and a $.ref"; y = 1;"#);
    // An escaped quote does not close the string.
    agree(r#"s = "he said \"hi\" ok"; t = 2;"#);
    // A backslash before the closing quote (escaped backslash then quote).
    agree(r#"p = "back\\"; q = 3;"#);
    // Empty string.
    agree(r#"e = ""; f = 4;"#);
    // Adjacent strings.
    agree(r#""ab""cd""#);
}

#[test]
fn unterminated_and_newline_agree() {
    // Unterminated tail — both report "no extent".
    agree("g = \"open and never closed");
    // A bare newline terminates (single-line) — unterminated, so reject.
    agree("h = \"line one\nstill\"");
    // A `"` that is actually the close of a prior string, then junk.
    agree(r#"z = "one" + not_a_string"#);
}

#[test]
fn every_position_of_a_dense_corpus_agrees() {
    // A dense mix so the loop hits many `"` offsets and interleavings.
    agree(r#"a="x";b="y\"z";c="{}";d="";e="\\";f="tail"#);
}
