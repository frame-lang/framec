//! Erlang output block lowering — two-stage lexer/parser.
//!
//! Lowers generated Erlang handler bodies from brace-delimited blocks to
//! `case … of … end`.
//!
//! Architecture:
//!   Stage 1: OutputBlockLexer tokenizes text (respects strings/comments)
//!   Stage 2: ErlangBlockParser consumes tokens, emits transformed text
//!
//! Both stages are Frame state machines (.frs → .gen.rs).
//!
//! The Lua brace-form transform that used to live here is gone: `if COND { }`
//! is not Frame, and rewriting the user's native Lua was a text hack (#232).

#[allow(unreachable_patterns)]
#[allow(unused_mut)]
#[allow(dead_code)]
#[allow(non_snake_case)]
#[allow(unused_variables)]
mod _output_block_lexer {
    include!("output_block_lexer.gen.rs");
}

#[allow(unreachable_patterns)]
#[allow(unused_mut)]
#[allow(dead_code)]
#[allow(non_snake_case)]
#[allow(unused_variables)]
mod _output_block_parser_erlang {
    include!("output_block_parser_erlang.gen.rs");
}

use _output_block_lexer::OutputBlockLexerFsm;
use _output_block_parser_erlang::ErlangBlockParserFsm;

/// Erlang `{ }` → `case … of … end` lowering: scanned by the shared
/// `OutputBlockLexerFsm` and emitted by the dogfooded `ErlangBlockParserFsm`
/// (#123) — replacing the hand-rolled line/token logic that lived in
/// `erlang_system/blocks.rs`. Both stages are now Frame state machines.
pub(crate) fn erlang_blocks_to_case(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let (kinds, starts, ends) = lex_blocks(text, b'%', false);
    let mut parser = ErlangBlockParserFsm::new();
    parser.bytes = text.as_bytes().to_vec();
    parser.token_kinds = kinds;
    parser.token_starts = starts;
    parser.token_ends = ends;
    parser.do_parse();
    parser.result
}

/// Exhaustively tokenize `text` with the shared `OutputBlockLexerFsm`, returning
/// `(kinds, starts, ends)`. This is the string/comment-safe scanner other
/// backends' block transforms should drive instead of hand-rolling line scans
/// (see #123). Token kinds: 1=IF, 2=ELSEIF, 3=ELSE, 4=WHILE, 5=FOR, 6=LBRACE,
/// 7=RBRACE, 8=RETURN, 9=END, 10=NEWLINE, 11=TEXT, 12=COMMENT, 13=STRING.
/// `comment_char`/`comment_double`: `(b'-', true)` for Lua `--`, `(b'%', false)`
/// for Erlang `%`.
pub(crate) fn lex_blocks(
    text: &str,
    comment_char: u8,
    comment_double: bool,
) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    let bytes = text.as_bytes();
    let mut lexer = OutputBlockLexerFsm::new();
    lexer.bytes = bytes.to_vec();
    lexer.end = bytes.len();
    lexer.comment_char = comment_char;
    lexer.comment_double = comment_double;
    lexer.do_lex();
    (lexer.token_kinds, lexer.token_starts, lexer.token_ends)
}

#[cfg(test)]
mod erlang_tests {
    use super::erlang_blocks_to_case;

    // Every `case … of` opened must be closed by exactly one `end`.
    fn balanced(s: &str) -> bool {
        let opens = s
            .lines()
            .filter(|l| {
                let t = l.trim();
                (t.starts_with("case ") || t.starts_with("case(")) && t.ends_with(" of")
            })
            .count();
        let ends = s.lines().filter(|l| l.trim() == "end").count();
        opens == ends
    }

    // #123: a no-else `if` followed by trailing code is an early exit — the
    // trailing code lands in the `case`'s false arm, with the `end` after it.
    #[test]
    fn early_exit_trailing_in_false_arm() {
        let out = erlang_blocks_to_case("if c == 1 {\nT\n}\nX\n");
        assert!(balanced(&out), "unbalanced:\n{out}");
        // structure: case … true -> T ; false -> X end  (no `; false -> ok`)
        assert!(out.contains("case (c == 1) of"), "{out}");
        assert!(out.contains("; false ->"), "{out}");
        assert!(
            !out.contains("; false -> ok"),
            "trailing must be the false arm:\n{out}"
        );
        let f = out.find("; false ->").unwrap();
        let e = out.rfind("end").unwrap();
        assert!(
            out[f..e].contains('X'),
            "trailing X not in false arm:\n{out}"
        );
    }

    // A no-else `if` that IS the last statement keeps `; false -> ok end`.
    #[test]
    fn no_trailing_keeps_ok_arm() {
        let out = erlang_blocks_to_case("if c == 1 {\nT\n}\n");
        assert!(balanced(&out), "{out}");
        assert!(out.contains("; false -> ok"), "{out}");
    }

    // Nested early exit: outer trailing lands in the OUTER false arm, not the
    // inner case (the bug `nest_early_exits` shipped).
    #[test]
    fn nested_early_exit_outer_trailing_outer_arm() {
        let out = erlang_blocks_to_case("if a == 1 {\nif b == 1 {\nT\n}\nINNER\n}\nOUTER\n");
        assert!(balanced(&out), "unbalanced:\n{out}");
        // exactly two cases, two ends, two `; false ->` (one per case)
        assert_eq!(out.matches("case (").count(), 2, "{out}");
        // OUTER must appear after the LAST `; false ->` (the outer false arm)
        let last_false = out.rfind("; false ->").unwrap();
        assert!(
            out[last_false..].contains("OUTER"),
            "OUTER not in outer false arm:\n{out}"
        );
    }

    // Erlang tuple braces in a handler body are NOT treated as blocks.
    #[test]
    fn tuple_braces_preserved() {
        let out = erlang_blocks_to_case("foo({call, From}, X)\n");
        assert_eq!(out.trim(), "foo({call, From}, X)", "tuple mangled:\n{out}");
    }
}
