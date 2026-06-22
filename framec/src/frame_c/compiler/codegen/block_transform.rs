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

use _output_block_lexer::OutputBlockLexerFsm;
use _output_block_parser::OutputBlockParserFsm;

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
