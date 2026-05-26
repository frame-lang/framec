//! Domain-section parser.
//!
//! Parses Frame's `domain:` section — a list of fields, each in the canonical
//! `name [: type] = init` form. Type and initializer strings are opaque to
//! the parser (Frame's transpiler treats them as native-language source); the
//! parser only frames them with byte spans.

use super::{ParseError, Parser};
use crate::frame_c::compiler::frame_ast::DomainVar;
use crate::frame_c::compiler::lexer::Token;

impl<'a> Parser<'a> {
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
}
