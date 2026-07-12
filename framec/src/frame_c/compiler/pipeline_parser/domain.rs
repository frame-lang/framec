//! Domain-section parser.
//!
//! Parses Frame's `domain:` section — a list of fields, each in the canonical
//! `name [: type] = init` form. Type and initializer strings are opaque to
//! the parser (Frame's transpiler treats them as native-language source); the
//! parser only frames them with byte spans.

use super::{ParseError, Parser};
use crate::frame_c::compiler::frame_ast::DomainVar;
use crate::frame_c::compiler::lexer::Token;

impl Parser {
    /// Parse the domain section.
    ///
    /// Each field uses canonical Frame syntax: `name [: type] = init`.
    /// Type and init are opaque strings — Frame doesn't interpret them.
    /// Multi-line init via `= ( ... )` wrapper is supported.
    pub(super) fn parse_domain(&mut self) -> Result<Vec<DomainVar>, ParseError> {
        // RFC-0035 Round 9: the outer `domain:` line-scan is a Frame FSM
        // (`DomainScannerFsm`, see `compiler/domain_scanner/`). This method is
        // now the thin caller — it runs the FSM over the source bytes, lifts
        // out the parsed fields + resume cursor, then does the lexer-level
        // token drain (which needs `self.lexer`, not the byte view the FSM owns).
        let start = self.lexer.cursor();
        let (vars, cursor) = {
            let src = self.lexer.source();
            crate::frame_c::compiler::domain_scanner::scan_domain(src, start)?
        };
        self.lexer.set_cursor(cursor);
        // Drain any remaining tokens the lexer may have buffered for the domain section
        loop {
            let tok = self.peek()?;
            match tok {
                Token::Interface
                | Token::Machine
                | Token::Actions
                | Token::Operations
                | Token::Eof
                | Token::Domain => break,
                Token::RBrace => break,
                _ => {
                    self.advance()?;
                }
            }
        }
        Ok(vars)
    }
}

#[cfg(test)]
mod tests {
    // FRAMEC_BUGS #41: a `domain:` default that is a dict/array literal
    // spanning multiple physical lines must be captured as one balanced
    // expression (via ExprScannerFsm), not split at the first newline into
    // stray field declarations. Driven through the public `run` entry so the
    // whole parse→codegen path is exercised; python_3 (dynamic) emits the
    // initializer verbatim, so the literal showing up intact proves capture.
    use crate::run;

    fn py(domain_body: &str) -> String {
        let src = format!(
            "@@[target(\"python_3\")]\n\
             @@system Repro {{\n\
             \x20   interface:\n\
             \x20       n()\n\
             \x20   machine:\n\
             \x20       $S {{ n() {{}} }}\n\
             \x20   domain:\n\
             {domain_body}\n\
             }}\n"
        );
        run(&src, "python_3")
    }

    #[test]
    fn multiline_dict_default_captured_whole() {
        let out = py("        target = {107: 1, 112: 1,\n            133: 1, 136: 1}");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("107: 1"), "missing first dict entry:\n{out}");
        // Closing brace + last entry present ⇒ the literal was not truncated
        // at the first newline (the #41 symptom).
        assert!(
            out.contains("136: 1}"),
            "literal truncated / not closed:\n{out}"
        );
    }

    #[test]
    fn multiline_array_default_captured_whole() {
        let out = py("        dirs = [\"north\", \"south\",\n            \"ne\", \"nw\"]");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("\"north\""), "missing first element:\n{out}");
        assert!(
            out.contains("\"nw\"]"),
            "array truncated / not closed:\n{out}"
        );
    }

    #[test]
    fn multiline_dict_does_not_swallow_following_field() {
        // The field AFTER a multi-line default must still parse (scanning stops
        // at the depth-0 newline, not at EOF).
        let out = py("        a = {1: 1,\n            2: 2}\n        b = 9");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("2: 2}"), "first literal truncated:\n{out}");
        assert!(
            out.contains("9"),
            "following field 'b' was swallowed:\n{out}"
        );
    }

    #[test]
    fn single_line_scalar_default_unchanged() {
        let out = py("        count = 42");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("42"), "scalar default lost:\n{out}");
    }

    #[test]
    fn apostrophe_in_trailing_comment_does_not_swallow_following_field() {
        // #113: a lone apostrophe in a trailing `#` comment opened a string in
        // the single-line RHS scanner that ran past the newline, swallowing the
        // next field into the first field's initializer (emitted verbatim, not
        // `self.b = ...`). The string scan must stop at the depth-0 newline.
        let out = py("        a = None   # it's here\n        b = 0");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(
            out.contains("self.b = 0"),
            "field after an apostrophe-comment was swallowed:\n{out}"
        );
        // A balanced double-quote in a comment was never affected; keep it green.
        let out2 = py("        a = 0   # say \"hi\"\n        b = 1");
        assert!(
            out2.contains("self.b = 1"),
            "field after a quoted comment was swallowed:\n{out2}"
        );
    }

    #[test]
    fn apostrophe_in_own_line_comment_does_not_drop_following_fields() {
        // #113 (own-line form): a `#` comment on its own line *before* fields,
        // containing a lone apostrophe (`don't`), must be consumed as a comment.
        // The apostrophe must NOT open a string scan that swallows the lines
        // below it, dropping the subsequent domain fields.
        let out = py("        # don't drop the fields below this comment\n        x: int = 1\n        y: int = 2");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(
            out.contains("self.x = 1"),
            "field x after an own-line apostrophe-comment was dropped:\n{out}"
        );
        assert!(
            out.contains("self.y = 2"),
            "field y after an own-line apostrophe-comment was dropped:\n{out}"
        );
    }

    #[test]
    fn double_quote_in_own_line_comment_does_not_drop_following_fields() {
        // Guard: a lone double-quote in an own-line comment must not open a
        // string scan either.
        let out = py("        # he said \"go\n        x: int = 1\n        y: int = 2");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("self.x = 1"), "field x dropped:\n{out}");
        assert!(out.contains("self.y = 2"), "field y dropped:\n{out}");
    }

    #[test]
    fn apostrophe_in_trailing_comment_on_typed_no_init_field_is_safe() {
        // A typed field with no `= init` but a trailing apostrophe comment: the
        // type/line scan must consume to the newline, not open a string.
        let out = py("        x: int   # the system's counter\n        y: int = 2");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("self.y = 2"), "field y was dropped:\n{out}");
    }

    #[test]
    fn real_string_initializer_with_apostrophe_preserved() {
        // Guard: a genuine string initializer containing an apostrophe must
        // still be captured verbatim, and the following field must survive.
        let out = py("        s: str = \"it's a string\"\n        n: int = 5");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(
            out.contains("\"it's a string\""),
            "real string initializer lost:\n{out}"
        );
        assert!(
            out.contains("self.n = 5"),
            "field after string dropped:\n{out}"
        );
    }

    #[test]
    fn trailing_operator_continuation_captured_whole() {
        // #185: a line ending in a binary operator at depth 0 continues onto the
        // next line (JS ASI, and most fluent syntaxes). The `2` must not be
        // dropped, and the following field `y` must still parse.
        let out = py("        x = 1 +\n            2\n        y = 99");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(
            out.contains("self.x = 1 +") && out.contains("2"),
            "trailing-operator continuation truncated (the `2` was dropped):\n{out}"
        );
        assert!(
            out.contains("self.y = 99"),
            "field after a continued expression was swallowed:\n{out}"
        );
    }

    #[test]
    fn leading_dot_chain_continuation_captured_whole() {
        // #185: the next line beginning with `.` continues a method chain even
        // though the current line ends with `)` (not an operator).
        let out = py("        x = foo()\n            .bar()\n        y = 7");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(
            out.contains(".bar()"),
            "leading-dot chain was severed (`.bar()` dropped or stray):\n{out}"
        );
        assert!(
            out.contains("self.y = 7"),
            "following field swallowed:\n{out}"
        );
    }

    #[test]
    fn complete_expression_still_terminates_at_newline() {
        // Regression guard: a complete single-line expression must still stop at
        // the depth-0 newline — the continuation heuristic must not over-capture
        // the next field.
        let out = py("        a = 1\n        b = 2");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(out.contains("self.a = 1"), "field a lost:\n{out}");
        assert!(
            out.contains("self.b = 2"),
            "the next field was swallowed into a's initializer:\n{out}"
        );
    }

    #[test]
    fn trailing_generic_close_does_not_over_continue() {
        // A line ending in a bare `>` closes a generic (`Map<K, V>`), NOT an
        // arrow — it must terminate so the following field survives. (`=>` still
        // continues; that is the arrow case, tested implicitly by other syntaxes.)
        let out = py("        a = m<u8>\n        b = 2");
        assert!(out.contains("class Repro"), "did not compile:\n{out}");
        assert!(
            out.contains("self.b = 2"),
            "a bare trailing `>` over-continued and swallowed `b`:\n{out}"
        );
    }
}
