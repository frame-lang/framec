//! Output block transformation — two-stage lexer/parser.
//!
//! Transforms generated Frame output from brace-delimited blocks to
//! target language syntax (Lua: if/then/end, Erlang: case/of/end).
//!
//! Architecture:
//!   Stage 1: OutputBlockLexer tokenizes text (respects strings/comments)
//!   Stage 2: OutputBlockParser consumes tokens, emits transformed text
//!
//! Both stages are Frame state machines (.frs → .gen.rs).

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
mod _output_block_parser {
    include!("output_block_parser.gen.rs");
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
use _output_block_parser::OutputBlockParserFsm;
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

/// Block transformation mode
#[derive(Clone, Copy)]
pub enum BlockTransformMode {
    /// Lua: if/then/elseif/else/end, while/do/end
    Lua = 1,
    /// Erlang: case/of/true->/false->/end (future)
    Erlang = 2,
}

/// Transform generated output from Frame brace blocks to target language syntax.
///
/// Uses two Frame state machines:
/// 1. OutputBlockLexer: tokenizes text, skipping strings/comments
/// 2. OutputBlockParser: consumes tokens, emits transformed text
pub fn transform_blocks(text: &str, mode: BlockTransformMode) -> String {
    if text.is_empty() {
        return String::new();
    }

    let bytes = text.as_bytes();

    // Configure lexer for the target language
    let (comment_char, comment_double) = match mode {
        BlockTransformMode::Lua => (b'-', true),     // -- comments
        BlockTransformMode::Erlang => (b'%', false), // % comments
    };

    // Stage 1: Lex
    let mut lexer = OutputBlockLexerFsm::new();
    lexer.bytes = bytes.to_vec();
    lexer.end = bytes.len();
    lexer.comment_char = comment_char;
    lexer.comment_double = comment_double;
    lexer.do_lex();

    // Stage 2: Parse
    let mut parser = OutputBlockParserFsm::new();
    parser.bytes = bytes.to_vec();
    parser.mode = mode as usize;
    parser.token_kinds = lexer.token_kinds;
    parser.token_starts = lexer.token_starts;
    parser.token_ends = lexer.token_ends;
    parser.do_parse();

    parser.result
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
        assert!(!out.contains("; false -> ok"), "trailing must be the false arm:\n{out}");
        let f = out.find("; false ->").unwrap();
        let e = out.rfind("end").unwrap();
        assert!(out[f..e].contains('X'), "trailing X not in false arm:\n{out}");
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
        assert!(out[last_false..].contains("OUTER"), "OUTER not in outer false arm:\n{out}");
    }

    // Erlang tuple braces in a handler body are NOT treated as blocks.
    #[test]
    fn tuple_braces_preserved() {
        let out = erlang_blocks_to_case("foo({call, From}, X)\n");
        assert_eq!(out.trim(), "foo({call, From}, X)", "tuple mangled:\n{out}");
    }
}

#[cfg(test)]
mod tests {
    use super::{transform_blocks, BlockTransformMode::Lua};

    // #122: Lua table-constructor braces `{ ... }` in handler bodies must be
    // left intact; only control-flow braces become `do/then … end`.

    #[test]
    fn table_literal_in_while_body_preserved() {
        let out = transform_blocks(
            "while i < n {\n    table.insert(t, {size = i + 1, alive = true})\n}",
            Lua,
        );
        // table braces survive; the while brace becomes do/end
        assert!(
            out.contains("{size = i + 1, alive = true}"),
            "table eaten: {out}"
        );
        assert!(out.contains("while i < n do"), "while not lowered: {out}");
        assert!(out.trim_end().ends_with("end"), "missing end: {out}");
        assert!(!out.contains("trueend"), "table close became end: {out}");
    }

    #[test]
    fn nested_and_empty_tables_preserved() {
        let out = transform_blocks(
            "if x {\n    local n = {a = {b = {c = 1}}}\n    local e = {}\n}",
            Lua,
        );
        assert!(out.contains("{a = {b = {c = 1}}}"), "nested eaten: {out}");
        assert!(out.contains("local e = {}"), "empty eaten: {out}");
        assert!(out.contains("if x then"));
    }

    #[test]
    fn return_with_table_keeps_braces() {
        let out = transform_blocks("if x {\n    return {first = 1, n = 2}\n}", Lua);
        assert!(
            out.contains("return {first = 1, n = 2}"),
            "return table eaten: {out}"
        );
        assert!(
            !out.contains("2end"),
            "return table close became end: {out}"
        );
    }

    #[test]
    fn table_in_if_else_branches() {
        let out = transform_blocks("if c {\n    t = {1, 2}\n} else {\n    t = {}\n}", Lua);
        assert!(out.contains("if c then"));
        assert!(out.contains("else"));
        assert!(out.contains("t = {1, 2}"));
        assert!(out.contains("t = {}"));
        assert!(out.trim_end().ends_with("end"));
    }

    #[test]
    fn plain_control_flow_unchanged_by_table_logic() {
        // No tables — the table-depth tracking must not perturb normal lowering.
        let out = transform_blocks("while a < b {\n    a = a + 1\n}", Lua);
        assert_eq!(out, "while a < b do\n    a = a + 1\nend");
    }
}
