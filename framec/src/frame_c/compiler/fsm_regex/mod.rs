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
//! §11. Edge anchors — a leading `^`/`\A` or trailing `$`/`\z`, plus (on the
//! `bytes` alphabet) a leading/trailing `\b`/`\B` — are extracted into
//! [`CompiledRegex::requires_start`]/`requires_end` and
//! [`start_boundary`](CompiledRegex::start_boundary)/`end_boundary` (§6.6),
//! enforced by the matcher. *Interior* (non-edge) anchors/boundaries, and
//! `\b`/`\B` on `char`/`token`, are deferred
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
pub mod pike;
pub mod restrictions;
pub mod size_check;
pub mod subset;
pub mod thompson;
pub mod unicode;

/// The alphabet of a regex. Determined by the `@@fsm`'s input parameter
/// type (RFC-0042 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alphabet {
    /// Octets 0..=255. Default for `bytes` input.
    #[default]
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

/// A word-boundary constraint at a pattern edge (RFC-0042 §6.6). `\b`
/// requires a boundary at that position; `\B` requires its absence. Like
/// the position anchors, an edge `\b`/`\B` is stripped from `dfa` and
/// enforced by the matcher against the live cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBoundary {
    /// `\b` — a word boundary must be present at this position.
    Required,
    /// `\B` — a word boundary must be absent at this position.
    Forbidden,
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
    /// A leading `\b`/`\B` was present (bytes alphabet only): the matcher
    /// asserts a word boundary is present/absent between the byte before
    /// the match start and the first matched byte.
    pub start_boundary: Option<WordBoundary>,
    /// A trailing `\b`/`\B` was present: the matcher asserts a word boundary
    /// is present/absent between the last matched byte and the byte after
    /// the match end.
    pub end_boundary: Option<WordBoundary>,
    /// A `\p{...}`/`\P{...}` Unicode class was resolved into ranges (char
    /// alphabet only, §11.6). The validator uses this to enforce the
    /// `@@[allow(unicode_classes)]` opt-in; the DFA itself is range-only.
    pub used_unicode_class: bool,
    /// `Some` when the regex contains a lazy quantifier (§11.1): a Pike VM
    /// program (leftmost-first match-end semantics) replaces the DFA. The
    /// `dfa` field is then an empty placeholder — matchers route to the Pike
    /// program. Bytes/char alphabets only (token + lazy is gated out).
    pub program: Option<pike::Program>,
}

/// Why a regex failed to compile.
#[derive(Debug, Clone)]
pub enum CompileError {
    /// One or more dialect/syntax/size diagnostics (E720–E723).
    Diagnostics(Vec<EngineDiagnostic>),
    /// The regex uses an anchor the v0.1 engine does not handle: an *interior*
    /// (non-edge) `^`/`$`/`\A`/`\z` or `\b`/`\B`, or a `\b`/`\B` on the
    /// `char`/`token` alphabet (no word classification yet — §11.6). Edge
    /// `^`/`$`/`\A`/`\z`, and edge `\b`/`\B` on `bytes`, *are* supported —
    /// extracted into [`CompiledRegex::requires_start`]/`requires_end` and
    /// [`start_boundary`](CompiledRegex::start_boundary)/`end_boundary`.
    /// Surfaced as a clear limitation rather than a silent miscompile.
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
    let mut ast = match parser::parse(source, alphabet) {
        Ok(ast) => ast,
        Err(e) => {
            return Err(CompileError::Diagnostics(vec![EngineDiagnostic {
                code: "E722",
                span: e.span,
                message: format!("invalid regex syntax: {:?}", e.kind),
            }]));
        }
    };

    // 1b. Resolve `\p{...}` Unicode classes to codepoint ranges (char
    //     alphabet only, §6.7/§11.6). After this the AST is range-only — no
    //     Unicode member reaches restrictions or Thompson. `used_unicode_class`
    //     drives the validator's `@@[allow(unicode_classes)]` opt-in gate.
    let used_unicode_class = match unicode::resolve(&mut ast, alphabet) {
        Ok(used) => used,
        Err(d) => return Err(CompileError::Diagnostics(vec![d])),
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

    // 3. Anchors (§6.6). Two paths:
    //    - Fast DFA path: an anchor-free core, optionally with leading `^`/`\A`,
    //      trailing `$`/`\z`, and (on `bytes`) edge `\b`/`\B` — these extract to
    //      position flags the matcher enforces around a pure-DFA match.
    //    - Pike path: any *interior* anchor, or `\b`/`\B` on `char`, or a lazy
    //      quantifier — handled by the priority NFA VM with zero-width `Assert`
    //      instructions (interior anchors, multiline, Unicode/interior `\b`).
    let span = ast.root.span;
    let (requires_start, requires_end, start_boundary, end_boundary, ast) =
        extract_boundary_anchors(ast);

    // Word boundaries need a word/non-word classification of the alphabet. The
    // fast DFA path supports `bytes` (ASCII word set); `char`/interior `\b` and
    // interior anchors route to the Pike VM (assertion path) — gated on until
    // every backend evaluates `Assert` (Phase 2). For now those remain
    // UnsupportedAnchors, exactly as before; the Pike `Assert` capability is
    // exercised by the `fsm_regex::pike` unit tests.
    if (start_boundary.is_some() || end_boundary.is_some()) && alphabet != Alphabet::Bytes {
        return Err(CompileError::UnsupportedAnchors(span));
    }

    // 3b. Lazy quantifiers (§11.1) → Pike VM (per-quantifier match-end). Token
    //     + lazy was already rejected; bytes/char only.
    if pike::contains_lazy(&ast) {
        let program = pike::compile(&ast, alphabet);
        let placeholder_dfa = subset::Dfa {
            states: Vec::new(),
            start: 0,
            alphabet,
        };
        let metrics = metrics::collect(&placeholder_dfa);
        return Ok(CompiledRegex {
            dfa: placeholder_dfa,
            metrics,
            warnings: Vec::new(),
            requires_start,
            requires_end,
            start_boundary,
            end_boundary,
            used_unicode_class,
            program: Some(program),
        });
    }

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
        start_boundary,
        end_boundary,
        used_unicode_class,
        program: None,
    })
}

/// Strip leading/trailing *edge* anchors from the regex, returning
/// Does the regex AST contain any anchor node (`^ $ \A \z \b \B`)? Used to
/// route interior-anchor stages to the Pike path (the fast DFA path only
/// handles cleanly-extracted edge anchors). Wired in Phase 2 (the per-backend
/// `Assert` evaluation); kept here as the engine capability lands first.
#[allow(dead_code)]
fn ast_has_anchor(regex: &ast::RegexAst) -> bool {
    use ast::{RegexNode, SpannedNode};
    fn walk(n: &SpannedNode) -> bool {
        match &n.node {
            RegexNode::Anchor(_) => true,
            RegexNode::Quantifier { inner, .. } | RegexNode::Group(inner) => walk(inner),
            RegexNode::Concat(items) | RegexNode::Alt(items) => items.iter().any(walk),
            _ => false,
        }
    }
    walk(&regex.root)
}

/// Strip a leading/trailing edge anchor run, returning
/// `(requires_start, requires_end, start_boundary, end_boundary, core)`.
///
/// Leading `^`/`\A` → `requires_start`; trailing `$`/`\z` → `requires_end`;
/// leading `\b`/`\B` → `start_boundary`; trailing `\b`/`\B` → `end_boundary`.
/// A run of edge anchors is peeled (e.g. `^\bfoo\b$`). The core is the
/// remaining pattern; any anchor it *still* contains (genuinely mid-pattern,
/// or an edge `\b`/`\B` whose kind conflicts with one already peeled) survives
/// and is rejected downstream as unsupported — exactly as interior `^`/`$`
/// are. Word boundaries are enforced by the matcher (bytes alphabet only).
fn extract_boundary_anchors(
    regex: ast::RegexAst,
) -> (
    bool,
    bool,
    Option<WordBoundary>,
    Option<WordBoundary>,
    ast::RegexAst,
) {
    use ast::{Anchor, RegexAst, RegexNode, SpannedNode};

    let span = regex.root.span;
    let (mut requires_start, mut requires_end) = (false, false);
    let (mut start_boundary, mut end_boundary): (Option<WordBoundary>, Option<WordBoundary>) =
        (None, None);

    // A bare-anchor root (`/^/`, `/\b/`) is treated as a one-element concat so
    // the peel loops below produce an empty core with the requirement set.
    let mut items = match regex.root.node {
        RegexNode::Concat(items) => items,
        node @ RegexNode::Anchor(_) => vec![SpannedNode { node, span }],
        other => {
            return (
                false,
                false,
                None,
                None,
                RegexAst {
                    root: SpannedNode { node: other, span },
                },
            );
        }
    };

    let word_kind = |a: Anchor| match a {
        Anchor::WordBoundary => Some(WordBoundary::Required),
        Anchor::NonWordBoundary => Some(WordBoundary::Forbidden),
        _ => None,
    };

    // Peel leading edge anchors: `^`/`\A` (position) and `\b`/`\B` (boundary).
    while let Some(SpannedNode {
        node: RegexNode::Anchor(a),
        ..
    }) = items.first()
    {
        let a = *a;
        if matches!(a, Anchor::LineStart | Anchor::InputStart) {
            requires_start = true;
            items.remove(0);
        } else if let Some(kind) = word_kind(a) {
            // Idempotent on a repeated same-kind boundary; a conflicting
            // kind (`\b\B…`) is left in the core to be rejected downstream.
            match start_boundary {
                None => start_boundary = Some(kind),
                Some(k) if k == kind => {}
                Some(_) => break,
            }
            items.remove(0);
        } else {
            break; // an end-type anchor in leading position → leave for the gate
        }
    }

    // Peel trailing edge anchors: `$`/`\z` (position) and `\b`/`\B` (boundary).
    while let Some(SpannedNode {
        node: RegexNode::Anchor(a),
        ..
    }) = items.last()
    {
        let a = *a;
        if matches!(a, Anchor::LineEnd | Anchor::InputEnd) {
            requires_end = true;
            items.pop();
        } else if let Some(kind) = word_kind(a) {
            match end_boundary {
                None => end_boundary = Some(kind),
                Some(k) if k == kind => {}
                Some(_) => break,
            }
            items.pop();
        } else {
            break;
        }
    }

    (
        requires_start,
        requires_end,
        start_boundary,
        end_boundary,
        RegexAst {
            root: SpannedNode {
                node: RegexNode::Concat(items),
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
        // A backreference is non-regular — still E720.
        let err = compile(
            "(a)\\1",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .unwrap_err();
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

    /// FSM-TEST-306b — lazy quantifiers (v0.2, §11.1). On bytes/char a stage
    /// with a lazy quantifier compiles to a Pike program (the runtime
    /// leftmost-first semantics are exercised by `super::pike` + the backend
    /// execution tests). On the `token` alphabet lazy is unsupported (E720).
    #[test]
    fn fsm_test_306b_lazy_quantifier_compiles_to_program() {
        let r = compile("a.*?b", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES)
            .expect("lazy compiles on bytes");
        assert!(r.program.is_some(), "lazy stage must carry a Pike program");
        assert!(
            compile("a.*b", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES)
                .unwrap()
                .program
                .is_none()
        );
        assert_code("A*?", Alphabet::Token, "E720");
    }

    /// Unicode shorthands (§6.7): on `char`, `\d`/`\w`/`\s` resolve to Unicode
    /// sets (many ranges); on `bytes` they stay ASCII (a handful).
    #[test]
    fn shorthands_are_unicode_on_char() {
        // A Unicode `\w` produces far more ranges than the 4 ASCII ones.
        let ch = compile("\\w", Alphabet::Char, size_check::DEFAULT_MAX_DFA_STATES)
            .expect("\\w compiles on char");
        let by = compile("\\w", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES)
            .expect("\\w compiles on bytes");
        let ranges = |r: &CompiledRegex| {
            r.dfa
                .states
                .iter()
                .map(|s| s.transitions.len())
                .sum::<usize>()
        };
        assert!(
            ranges(&ch) > ranges(&by),
            "char \\w must be Unicode (more ranges) than bytes ASCII \\w"
        );
    }

    /// Unicode classes (§6.7/§11.6): on the `char` alphabet `\p{...}` resolves
    /// to codepoint ranges (engine accepts; the `@@[allow(unicode_classes)]`
    /// opt-in is the validator's job — see the `e720_unicode_class_*` tests,
    /// which carry the FSM-TEST-307 conformance tag). On `bytes`/`token` there
    /// is no codepoint notion, so it is E722.
    #[test]
    fn unicode_class_resolves_on_char() {
        let r = compile(
            "\\p{L}+",
            Alphabet::Char,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("\\p{L}+ compiles on char");
        assert!(r.used_unicode_class);
        // `\P{N}` (negated) also resolves.
        assert!(
            compile("\\P{N}", Alphabet::Char, size_check::DEFAULT_MAX_DFA_STATES)
                .unwrap()
                .used_unicode_class
        );
        // An ordinary regex does not set the flag.
        assert!(
            !compile("[a-z]+", Alphabet::Char, size_check::DEFAULT_MAX_DFA_STATES)
                .unwrap()
                .used_unicode_class
        );
        // Unknown class name → E722.
        assert_code("\\p{Nonsense}", Alphabet::Char, "E722");
        // Unicode class on a non-char alphabet → E722.
        assert_code("\\p{L}", Alphabet::Bytes, "E722");
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
        // An *interior* word boundary (neither leading nor trailing) is still
        // deferred — exactly as interior `^`/`$` are.
        let mid =
            compile("a\\bb", Alphabet::Bytes, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err();
        assert!(matches!(mid, CompileError::UnsupportedAnchors(_)));
    }

    #[test]
    fn edge_word_boundaries_compile() {
        // Leading `\b` → start_boundary; trailing `\b` → end_boundary; the
        // core (`foo`) is anchor-free.
        let r = compile(
            "\\bfoo\\b",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("\\bfoo\\b compiles");
        assert_eq!(r.start_boundary, Some(WordBoundary::Required));
        assert_eq!(r.end_boundary, Some(WordBoundary::Required));
        // `\B` → Forbidden; mixes with position anchors in one edge run.
        let r2 = compile(
            "^\\Bfoo",
            Alphabet::Bytes,
            size_check::DEFAULT_MAX_DFA_STATES,
        )
        .expect("^\\Bfoo compiles");
        assert!(r2.requires_start);
        assert_eq!(r2.start_boundary, Some(WordBoundary::Forbidden));
        // Word boundaries need byte word-classes: rejected on char/token.
        assert!(matches!(
            compile("\\bfoo", Alphabet::Char, size_check::DEFAULT_MAX_DFA_STATES).unwrap_err(),
            CompileError::UnsupportedAnchors(_)
        ));
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
