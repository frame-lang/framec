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
//! §11. Zero-width anchors are recognized but not yet folded into the DFA
//! ([`CompileError::UnsupportedAnchors`]; tracked follow-up — see the
//! [`subset`] module note).
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
}

/// Why a regex failed to compile.
#[derive(Debug, Clone)]
pub enum CompileError {
    /// One or more dialect/syntax/size diagnostics (E720–E723).
    Diagnostics(Vec<EngineDiagnostic>),
    /// The regex uses zero-width anchors, which the v0.1 DFA engine does
    /// not yet fold in (tracked follow-up; see [`subset`] module note).
    /// Surfaced as a clear compiler limitation rather than a silent
    /// miscompile or a fabricated spec code.
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

    // 3. Thompson NFA.
    let nfa = thompson::build(&ast, alphabet);

    // 4. Anchors are not yet folded into the DFA (tracked follow-up).
    if subset::nfa_has_anchors(&nfa) {
        return Err(CompileError::UnsupportedAnchors(ast.root.span));
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
    })
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
    fn defers_anchors() {
        let err = compile("^a", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err();
        assert!(matches!(err, CompileError::UnsupportedAnchors(_)));
    }

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
