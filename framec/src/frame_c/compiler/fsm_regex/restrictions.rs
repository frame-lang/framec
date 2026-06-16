//! v0.1 dialect restrictions per RFC-0042 §6.3, §6.4, §6.5, §6.7, §6.8.
//!
//! Walks a [`RegexAst`] and emits diagnostics for:
//!
//! - Non-regular constructs (backrefs, recursion, variable-width
//!   lookbehind) — E720, RFC-0042 §6.4.
//! - Regular-but-v0.1-excluded constructs (Unicode classes, lookaround,
//!   lazy quantifiers) — E720, §6.5.
//! - Alphabet-invalid constructs (byte escapes in char alphabet,
//!   character classes in token alphabet) — E722, §6.7 / §6.8.
//! - Empty regex — E723.
//! - RE2 constructs that Frame replaces with native equivalents
//!   (named captures, non-capturing groups) — E720, §6.3.
//!
//! Each diagnostic includes a span and a recovery hint.

use super::ast::{
    CharClass, ClassMember, ForbiddenConstruct, Laziness, Literal, RegexAst, RegexNode, SpannedNode,
};
use super::{Alphabet, Span};

/// Validate a parsed regex against the v0.1 dialect and the declared
/// alphabet. Returns *all* violations, not just the first — this lets
/// the frontend emit batched diagnostics in one pass.
pub fn check(ast: &RegexAst, alphabet: Alphabet) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    visit(&ast.root, alphabet, &mut diagnostics);
    diagnostics
}

/// One diagnostic emitted by [`check`].
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: DiagCode,
    pub span: Span,
    pub message: String,
    /// Free-form text for the `help:` line of the rendered diagnostic.
    /// Non-normative per the RFC's diagnostic-test entries.
    pub recovery_hint: String,
}

/// Diagnostic codes this module emits. Maps 1:1 to RFC-0042 §9.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    /// Forbidden regex construct (non-regular OR regular-but-excluded-in-v0.1).
    E720,
    /// DFA size limit. Not emitted by this module; see
    /// [`super::size_check`].
    #[allow(dead_code)]
    E721,
    /// Invalid regex syntax for the current alphabet.
    E722,
    /// Empty regex (`//`) where non-empty required.
    E723,
}

/// Recursively walk a node, appending every v0.1 violation. Forbidden
/// subtrees are reported once at their root and not descended into (the
/// whole construct is rejected, so nested diagnostics would be noise).
fn visit(node: &SpannedNode, alphabet: Alphabet, diagnostics: &mut Vec<Diagnostic>) {
    match &node.node {
        RegexNode::Forbidden(c) => {
            diagnostics.push(Diagnostic {
                code: DiagCode::E720,
                span: node.span,
                message: forbidden_message(c).to_string(),
                recovery_hint: forbidden_hint(c).to_string(),
            });
        }

        RegexNode::Empty => {
            diagnostics.push(Diagnostic {
                code: DiagCode::E723,
                span: node.span,
                message: "empty regex `//` matches nothing useful".to_string(),
                recovery_hint: "remove the match or give it a non-empty pattern".to_string(),
            });
        }

        RegexNode::Literal(lit) => {
            if !literal_matches_alphabet(lit, alphabet) {
                diagnostics.push(Diagnostic {
                    code: DiagCode::E722,
                    span: node.span,
                    message: format!(
                        "literal is not valid in the {} alphabet",
                        alphabet_name(alphabet)
                    ),
                    recovery_hint: alphabet_hint(alphabet).to_string(),
                });
            }
        }

        RegexNode::Class(class) => {
            // The token alphabet has no character classes (RFC-0042 §6.8) —
            // a token is matched by name, not by composed element ranges.
            if alphabet == Alphabet::Token {
                diagnostics.push(Diagnostic {
                    code: DiagCode::E722,
                    span: node.span,
                    message: "character classes are not valid in the token alphabet".to_string(),
                    recovery_hint: "match token kinds by name, or use alternation `A|B`"
                        .to_string(),
                });
            }
            check_class_ranges(class, node.span, diagnostics);
        }

        RegexNode::Quantifier {
            inner, laziness, ..
        } => {
            // Lazy quantifiers (§11.1) compile to a Pike VM program over the
            // alphabet's *scalar* element values. The token alphabet matches by
            // name and has no scalar notion, so lazy + token stays unsupported;
            // bytes/char are supported via the Pike path.
            if *laziness == Laziness::Lazy && alphabet == Alphabet::Token {
                diagnostics.push(Diagnostic {
                    code: DiagCode::E720,
                    span: node.span,
                    message: "lazy quantifiers are not supported on the token alphabet".to_string(),
                    recovery_hint:
                        "lazy matching needs scalar elements; use the `bytes` or `char` alphabet"
                            .to_string(),
                });
            }
            visit(inner, alphabet, diagnostics);
        }

        RegexNode::Group(inner) => visit(inner, alphabet, diagnostics),

        RegexNode::Concat(items) | RegexNode::Alt(items) => {
            for item in items {
                visit(item, alphabet, diagnostics);
            }
        }

        // `.` and zero-width anchors carry no alphabet/dialect restriction
        // at this layer.
        RegexNode::Dot | RegexNode::Anchor(_) => {}
    }
}

/// A reversed range (`[z-a]`) in a class is malformed (E722 — invalid
/// class syntax). Shorthand members carry no ordering.
fn check_class_ranges(class: &CharClass, span: Span, diagnostics: &mut Vec<Diagnostic>) {
    for m in &class.members {
        if let ClassMember::Range { low, high } = m {
            if low > high {
                diagnostics.push(Diagnostic {
                    code: DiagCode::E722,
                    span,
                    message: format!("reversed character-class range ({low} > {high})"),
                    recovery_hint: "write the range low-to-high, e.g. `a-z`".to_string(),
                });
            }
        }
    }
}

fn alphabet_name(a: Alphabet) -> &'static str {
    match a {
        Alphabet::Bytes => "bytes",
        Alphabet::Char => "char",
        Alphabet::Token => "token",
    }
}

fn alphabet_hint(a: Alphabet) -> &'static str {
    match a {
        Alphabet::Bytes => "use a byte value (e.g. `\\x41`) or change the input type to `char`",
        Alphabet::Char => "use a code point, or change the input type to `bytes`",
        Alphabet::Token => "match a token kind by name",
    }
}

/// The `help:` text for each forbidden construct.
fn forbidden_hint(c: &ForbiddenConstruct) -> &'static str {
    match c {
        ForbiddenConstruct::Backref(_) | ForbiddenConstruct::NamedBackref(_) => {
            "backreferences make the language non-regular; restructure without them"
        }
        ForbiddenConstruct::Recursion => "recursion is non-regular; use repetition instead",
        ForbiddenConstruct::PositiveLookahead(_)
        | ForbiddenConstruct::NegativeLookahead(_)
        | ForbiddenConstruct::PositiveLookbehind(_)
        | ForbiddenConstruct::NegativeLookbehind(_) => {
            "lookaround is deferred to v0.2 (RFC-0042 §11.5)"
        }
        ForbiddenConstruct::NamedCapture { .. } => "use a Frame stage label to capture (§3.5.2)",
        ForbiddenConstruct::NonCapturingGroup(_) => "use a plain group `(...)`",
    }
}

/// Convenience: produce the canonical E720 message for a forbidden
/// construct. Centralized so the diagnostic-test snapshots have a
/// single source of wording.
pub(crate) fn forbidden_message(c: &ForbiddenConstruct) -> &'static str {
    match c {
        ForbiddenConstruct::Backref(_) => "backreferences are non-regular",
        ForbiddenConstruct::NamedBackref(_) => "named backreferences are non-regular",
        ForbiddenConstruct::Recursion => "regex recursion is non-regular",
        ForbiddenConstruct::PositiveLookahead(_) | ForbiddenConstruct::NegativeLookahead(_) => {
            "fixed-width lookahead is not supported in v0.1"
        }
        ForbiddenConstruct::PositiveLookbehind(_) | ForbiddenConstruct::NegativeLookbehind(_) => {
            "lookbehind is not supported in v0.1"
        }
        ForbiddenConstruct::NamedCapture { .. } => {
            "named captures are replaced by stage labels (RFC-0042 §3.5.2)"
        }
        ForbiddenConstruct::NonCapturingGroup(_) => {
            "`(?:...)` is unnecessary in Frame — `()` does not capture"
        }
    }
}

/// Convenience: alphabet compatibility predicate for a literal node.
pub(crate) fn literal_matches_alphabet(lit: &Literal, alphabet: Alphabet) -> bool {
    matches!(
        (lit, alphabet),
        (Literal::Byte(_), Alphabet::Bytes)
            | (Literal::CodePoint(_), Alphabet::Char)
            | (Literal::Token(_), Alphabet::Token)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_regex::parser;

    fn diags(src: &str, alphabet: Alphabet) -> Vec<Diagnostic> {
        let ast = parser::parse(src, alphabet).expect("fixture must parse");
        check(&ast, alphabet)
    }

    fn codes(src: &str, alphabet: Alphabet) -> Vec<DiagCode> {
        diags(src, alphabet).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn clean_regex_has_no_diagnostics() {
        assert!(diags("[a-z]+(ab|cd)*\\d", Alphabet::Bytes).is_empty());
    }

    #[test]
    fn empty_regex_is_e723() {
        assert_eq!(codes("", Alphabet::Bytes), vec![DiagCode::E723]);
    }

    /// Lazy quantifiers are no longer a dialect restriction on bytes/char — a
    /// stage with one routes through the Pike VM (§11.1), so
    /// `restrictions::check` is silent on `a*?`.
    #[test]
    fn lazy_quantifier_is_not_a_restriction() {
        assert!(codes("a*?", Alphabet::Bytes).is_empty());
    }

    #[test]
    fn forbidden_constructs_are_e720() {
        // NOTE: `\p{L}` is no longer a restriction-level forbidden construct —
        // it parses to a Unicode class member that the `super::unicode` pass
        // resolves (char) or rejects (bytes/token, E722) before restrictions
        // run; the opt-in is enforced by the validator. See the engine test
        // `unicode_class_resolves_on_char`.
        for src in ["\\1", "(?:ab)", "(?=ab)", "(?!ab)", "(?<=ab)", "(?P<n>ab)"] {
            assert_eq!(
                codes(src, Alphabet::Bytes),
                vec![DiagCode::E720],
                "src {src:?}"
            );
        }
    }

    #[test]
    fn forbidden_reported_once_not_descended() {
        // A lazy quantifier nested inside a forbidden group is not double
        // reported: the forbidden root is reported and not descended.
        assert_eq!(codes("(?:a*?)", Alphabet::Bytes), vec![DiagCode::E720]);
    }

    #[test]
    fn wrong_alphabet_literal_is_e722() {
        // A non-ASCII code point in the bytes alphabet.
        assert_eq!(codes("é", Alphabet::Bytes), vec![DiagCode::E722]);
    }

    #[test]
    fn char_class_in_token_alphabet_is_e722() {
        assert!(codes("[abc]", Alphabet::Token).contains(&DiagCode::E722));
    }

    #[test]
    fn reversed_range_is_e722() {
        assert_eq!(codes("[z-a]", Alphabet::Bytes), vec![DiagCode::E722]);
    }

    #[test]
    fn batches_multiple_violations() {
        // A non-capturing group and a backreference in one regex → two diags.
        let cs = codes("(?:x)\\1", Alphabet::Bytes);
        assert_eq!(cs.len(), 2);
        assert!(cs.contains(&DiagCode::E720));
    }
}
