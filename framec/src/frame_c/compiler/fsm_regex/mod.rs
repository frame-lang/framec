//! `@@fsm` regex engine.
//!
//! Implements the regex-dialect compilation pipeline from RFC-0042 §6:
//!
//! ```text
//!   parser       : &str + Alphabet -> RegexAst        (§6.2–§6.8)
//!   restrictions : &RegexAst + Alphabet -> Vec<Diag>  (§6.3, §6.4, §6.5, §6.7, §6.8)
//!   thompson     : &RegexAst -> Nfa                   (§6.9 step 2)
//!   subset       : &Nfa -> Dfa                        (§6.9 step 3)
//!   hopcroft     : &Dfa -> Dfa  (minimized)           (§6.9 step 4)
//!   size_check   : &Dfa + max_states -> Result        (§9.1 E721, §9.2 W704)
//!   metrics      : &Dfa -> DfaMetrics                 (§9.3)
//! ```
//!
//! # v0.1 scope
//!
//! v0.1 builds a **pure DFA**. There is no NFA simulation at runtime; once
//! we have a minimal DFA, codegen emits a DFA executor (table-driven by
//! default; switch-driven if `@@[dispatch(switch)]`).
//!
//! Lazy quantifiers, lookaround, Unicode general-category classes, named
//! captures, backreferences, and recursion are all rejected by
//! `restrictions::check`. Their handling is deferred to v0.2 per RFC-0042
//! §11. Boundary anchors — a leading `^`/`\A` or trailing `$`/`\z` — are
//! extracted into [`CompiledRegex::requires_start`]/`requires_end` (§6.6);
//! mid-pattern anchors and `\b`/`\B` are deferred
//! ([`CompileError::UnsupportedAnchors`]).
//!
//! # Entry point
//!
//! [`compile`] runs the whole pipeline and returns a [`CompiledRegex`]
//! (minimal DFA + metrics + W704 warnings) or a [`CompileError`]. The
//! module is wired into the parent `compiler::mod`; callers (the fsm
//! validator and the per-target backends) use [`compile`] rather than the
//! individual stages.

pub mod ast;
pub mod hopcroft;
pub mod metrics;
pub mod parser;
pub mod restrictions;
pub mod size_check;
pub mod subset;
pub mod thompson;

/// The alphabet of a regex. Determined by the `@@fsm`'s input parameter
/// type (RFC-0042 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    /// Octets 0..=255. Default for `bytes` input.
    Bytes,
    /// Unicode code points. For `char` input.
    Char,
    /// Application-defined token kinds. For `token` input.
    Token,
}

/// Half-open source span; `start..end` byte offsets into the regex
/// literal's interior (the text between the delimiting `/` characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A diagnostic emitted by the regex engine, with its RFC-0042 §9.1 code.
#[derive(Debug, Clone)]
pub struct EngineDiagnostic {
    pub code: &'static str,
    pub span: Span,
    pub message: String,
}

/// A successfully compiled regex: its minimal DFA, collected metrics, and
/// any non-fatal warnings (e.g. W704 DFA-size-approaching).
#[derive(Debug, Clone)]
pub struct CompiledRegex {
    pub dfa: subset::Dfa,
    pub metrics: metrics::DfaMetrics,
    pub warnings: Vec<EngineDiagnostic>,
    /// A leading `^`/`\A` anchor was present: the match must begin at the
    /// input start (cursor 0). The anchor is stripped from `dfa`; the
    /// matcher enforces the position (RFC-0042 §6.6).
    pub requires_start: bool,
    /// A trailing `$`/`\z` anchor was present: the match must end at the
    /// input end. Stripped from `dfa`; enforced by the matcher.
    pub requires_end: bool,
}

/// Why a regex failed to compile.
#[derive(Debug, Clone)]
pub enum CompileError {
    /// One or more dialect/syntax/size diagnostics (E720–E723).
    Diagnostics(Vec<EngineDiagnostic>),
    /// The regex uses an anchor the v0.1 engine does not fold into the DFA:
    /// a mid-pattern `^`/`$`/`\A`/`\z`, or any `\b`/`\B` word boundary.
    /// (Leading `^`/`\A` and trailing `$`/`\z` *are* supported — extracted
    /// into [`CompiledRegex::requires_start`]/`requires_end`.) Surfaced as a
    /// clear limitation rather than a silent miscompile.
    UnsupportedAnchors(Span),
}

/// Compile a regex literal body to a minimal DFA, running the full
/// pipeline: parse → restrictions → Thompson → subset → Hopcroft →
/// size check → metrics.
///
/// `max_states` is the configured DFA-size budget
/// ([`size_check::DEFAULT_MAX_DFA_STATES`] when unset). Exceeding it is a
/// fatal E721; approaching it is a non-fatal W704 returned in
/// [`CompiledRegex::warnings`].
pub fn compile(
    source: &str,
    alphabet: Alphabet,
    max_states: usize,
) -> Result<CompiledRegex, CompileError> {
    // 1. Parse. A malformed regex is E722 (invalid regex syntax).
    let ast = match parser::parse(source, alphabet) {
        Ok(ast) => ast,
        Err(e) => {
            return Err(CompileError::Diagnostics(vec![EngineDiagnostic {
                code: "E722",
                span: e.span,
                message: format!("invalid regex syntax: {:?}", e.kind),
            }]));
        }
    };

    // 2. Dialect restrictions (E720 / E722 / E723).
    let restriction_diags = restrictions::check(&ast, alphabet);
    if !restriction_diags.is_empty() {
        return Err(CompileError::Diagnostics(
            restriction_diags
                .into_iter()
                .map(|d| EngineDiagnostic {
                    code: diag_code_str(d.code),
                    span: d.span,
                    message: d.message,
                })
                .collect(),
        ));
    }

    // 3. Boundary anchors (§6.6). A leading `^`/`\A` and/or trailing
    //    `$`/`\z` are extracted into position requirements the matcher
    //    enforces; the core is anchor-free. Anchors anywhere else, and
    //    `\b`/`\B`, are not yet folded into the DFA (deferred to v0.2) —
    //    they survive into the NFA and trip the gate below.
    let span = ast.root.span;
    let (requires_start, requires_end, ast) = extract_boundary_anchors(ast);

    // 4. Thompson NFA over the anchor-free core.
    let nfa = thompson::build(&ast, alphabet);
    if subset::nfa_has_anchors(&nfa) {
        return Err(CompileError::UnsupportedAnchors(span));
    }

    // 5. Subset construction + minimization.
    let dfa = hopcroft::minimize(&subset::construct(&nfa));

    // 6. Size check (E721 fatal / W704 warning).
    let size = size_check::check(&dfa, max_states);
    let mut warnings = Vec::new();
    match size.status {
        size_check::SizeStatus::Exceeds => {
            return Err(CompileError::Diagnostics(vec![EngineDiagnostic {
                code: "E721",
                span: ast.root.span,
                message: format!(
                    "DFA has {} states, exceeding the configured limit of {}",
                    size.state_count, size.limit
                ),
            }]));
        }
        size_check::SizeStatus::Approaching => warnings.push(EngineDiagnostic {
            code: "W704",
            span: ast.root.span,
            message: format!(
                "DFA has {} states, approaching the configured limit of {}",
                size.state_count, size.limit
            ),
        }),
        size_check::SizeStatus::Ok => {}
    }

    let metrics = metrics::collect(&dfa);
    Ok(CompiledRegex {
        dfa,
        metrics,
        warnings,
        requires_start,
        requires_end,
    })
}

/// Strip a leading `^`/`\A` and/or trailing `$`/`\z` from the regex,
/// returning `(requires_start, requires_end, core)`. The core is the
/// remaining pattern; any anchor it still contains (mid-pattern, or
/// `\b`/`\B`) survives and is rejected downstream (v0.2 work).
fn extract_boundary_anchors(regex: ast::RegexAst) -> (bool, bool, ast::RegexAst) {
    use ast::{Anchor, RegexAst, RegexNode, SpannedNode};

    fn is_start(a: Anchor) -> bool {
        matches!(a, Anchor::LineStart | Anchor::InputStart)
    }
    fn is_end(a: Anchor) -> bool {
        matches!(a, Anchor::LineEnd | Anchor::InputEnd)
    }

    let span = regex.root.span;
    let (mut requires_start, mut requires_end) = (false, false);

    let root_node = match regex.root.node {
        RegexNode::Concat(mut items) => {
            if let Some(SpannedNode {
                node: RegexNode::Anchor(a),
                ..
            }) = items.first()
            {
                if is_start(*a) {
                    requires_start = true;
                }
            }
            if requires_start {
                items.remove(0);
            }
            if let Some(SpannedNode {
                node: RegexNode::Anchor(a),
                ..
            }) = items.last()
            {
                if is_end(*a) {
                    requires_end = true;
                }
            }
            if requires_end {
                items.pop();
            }
            RegexNode::Concat(items)
        }
        // A bare anchor regex (`/^/`, `/$/`) is an empty core with the
        // position requirement.
        RegexNode::Anchor(a) if is_start(a) => {
            requires_start = true;
            RegexNode::Concat(Vec::new())
        }
        RegexNode::Anchor(a) if is_end(a) => {
            requires_end = true;
            RegexNode::Concat(Vec::new())
        }
        other => other,
    };

    (
        requires_start,
        requires_end,
        RegexAst {
            root: SpannedNode {
                node: root_node,
                span,
            },
        },
    )
}

fn diag_code_str(code: restrictions::DiagCode) -> &'static str {
    use restrictions::DiagCode::*;
    match code {
        E720 => "E720",
        E721 => "E721",
        E722 => "E722",
        E723 => "E723",
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn compiles_clean_regex() {
        let r = compile(
            "[0-9]+",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("should compile");
        assert!(r.warnings.is_empty());
        assert!(r.metrics.state_count >= 1);
    }

    #[test]
    fn rejects_forbidden_with_e720() {
        let err = compile("a*?", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err();
        match err {
            CompileError::Diagnostics(ds) => assert!(ds.iter().any(|d| d.code == "E720")),
            other => panic!("got {:?}", other),
        }
    }

    /// Assert that `pattern` fails to compile with diagnostic `code`.
    fn assert_code(pattern: &str, alphabet: Alphabet, code: &str) {
        let err = compile(pattern, alphabet, size_check::DEFAULT_MAX_DFA_STATES)
            .expect_err(&format!("`{pattern}` should fail to compile"));
        match err {
            CompileError::Diagnostics(ds) => assert!(
                ds.iter().any(|d| d.code == code),
                "expected {code} for `{pattern}`, got {ds:?}"
            ),
            other => panic!("expected {code} diagnostic for `{pattern}`, got {other:?}"),
        }
    }

    /// FSM-TEST-250 — byte alphabet: a class over the ASCII byte range compiles
    /// to a DFA over the 0–255 byte alphabet.
    #[test]
    fn fsm_test_250_byte_alphabet() {
        compile(
            "[\\x00-\\x7F]+",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("byte-range class compiles");
    }

    /// FSM-TEST-252 — the `char` alphabet rejects the `\xNN` byte escape with
    /// E722 (code points use `\u{NNNN}` instead, §6.7).
    #[test]
    fn fsm_test_252_char_rejects_byte_escape() {
        assert_code("\\x41", Alphabet::Char, "E722");
        // The code-point form is accepted in the char alphabet.
        compile(
            "\\u{4E2D}",
            Alphabet::Char,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("`\\u{4E2D}` compiles in the char alphabet");
    }

    /// FSM-TEST-254 — the `token` alphabet rejects character classes with E722
    /// (literal byte/char syntax has no meaning over token kinds, §6.8).
    #[test]
    fn fsm_test_254_token_rejects_char_class() {
        assert_code("[a-z]", Alphabet::Token, "E722");
    }

    /// FSM-TEST-300 — backreferences are non-regular and rejected with E720.
    #[test]
    fn fsm_test_300_backreference_rejected() {
        assert_code("(.)\\1", Alphabet::Bytes, "E720");
    }

    /// FSM-TEST-301 — recursion is non-regular and rejected with E720.
    #[test]
    fn fsm_test_301_recursion_rejected() {
        assert_code("a(?R)?b", Alphabet::Bytes, "E720");
    }

    /// FSM-TEST-302 — lookahead is deferred in v0.1 and rejected with E720.
    #[test]
    fn fsm_test_302_lookahead_rejected() {
        assert_code("foo(?=bar)", Alphabet::Bytes, "E720");
    }

    /// FSM-TEST-303 — a multi-class concatenation with quantifiers (the
    /// identifier pattern) compiles.
    #[test]
    fn fsm_test_303_character_class_compiles() {
        compile(
            "[a-zA-Z_][a-zA-Z0-9_]*",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("identifier class compiles");
    }

    /// FSM-TEST-305 — bounded repetition `{n}` compiles.
    #[test]
    fn fsm_test_305_bounded_repetition_compiles() {
        compile(
            "[0-9]{4}",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("bounded repetition compiles");
    }

    /// FSM-TEST-306b — lazy quantifiers are deferred in v0.1 and rejected with
    /// E720 (the greedy-semantics companion FSM-TEST-306 is a runtime test).
    #[test]
    fn fsm_test_306b_lazy_quantifier_rejected() {
        assert_code("a.*?b", Alphabet::Bytes, "E720");
    }

    /// FSM-TEST-307 — Unicode general-category classes are deferred in v0.1 and
    /// rejected with E720.
    #[test]
    fn fsm_test_307_unicode_class_rejected() {
        assert_code("\\p{L}+", Alphabet::Bytes, "E720");
    }

    /// FSM-TEST-310 — an empty regex (`//`) is rejected with E723.
    #[test]
    fn rejects_empty_with_e723() {
        let err = compile("", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err();
        match err {
            CompileError::Diagnostics(ds) => assert!(ds.iter().any(|d| d.code == "E723")),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn rejects_malformed_syntax_with_e722() {
        let err = compile("(ab", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err();
        match err {
            CompileError::Diagnostics(ds) => assert!(ds.iter().any(|d| d.code == "E722")),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn boundary_anchors_compile() {
        // Leading `^` → requires_start; the core DFA is anchor-free.
        let r = compile("^foo", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES)
            .expect("^foo compiles");
        assert!(r.requires_start);
        assert!(!r.requires_end);
        // Trailing `$` → requires_end.
        let r2 = compile("foo$", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES)
            .expect("foo$ compiles");
        assert!(!r2.requires_start);
        assert!(r2.requires_end);
        // Both.
        let r3 = compile("^foo$", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES)
            .expect("^foo$ compiles");
        assert!(r3.requires_start && r3.requires_end);
    }

    #[test]
    fn defers_mid_pattern_anchors() {
        // A `$` in the middle is not a boundary anchor → deferred (v0.2).
        let err = compile("a$b", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err();
        assert!(matches!(err, CompileError::UnsupportedAnchors(_)));
        // Word boundaries are deferred regardless of position.
        let wb = compile(
            "\\bfoo",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .unwrap_err();
        assert!(matches!(wb, CompileError::UnsupportedAnchors(_)));
    }

    /// FSM-TEST-311 — a pattern whose minimal DFA exceeds the configured
    /// `max_dfa_states` limit is rejected with E721.
    #[test]
    fn e721_when_dfa_exceeds_limit() {
        // `abc` minimizes to 4 states; a limit of 2 is exceeded.
        let err = compile("abc", Alphabet::Bytes, 2).unwrap_err();
        match err {
            CompileError::Diagnostics(ds) => assert!(ds.iter().any(|d| d.code == "E721")),
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn w704_when_approaching_limit() {
        // 4 states, limit 5 → warn threshold 3; warns but still compiles.
        let r = compile("abc", Alphabet::Bytes, 5).expect("compiles with warning");
        assert!(r.warnings.iter().any(|w| w.code == "W704"));
    }
}
