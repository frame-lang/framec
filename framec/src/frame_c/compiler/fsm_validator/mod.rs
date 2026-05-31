//! `@@fsm` semantic validator — the E700-series checks from RFC-0042 §9.
//!
//! Runs over a parsed [`FsmDeclAst`] (from
//! [`crate::frame_c::compiler::fsm_parser`]) and returns a list of
//! [`FsmDiagnostic`]s. Like the parser, the validator is a Frame
//! `@@system` (`fsm_validator.frs`) — one validation pass per state,
//! with the per-pass check logic in native helpers here.
//!
//! # Coverage
//!
//! v1: E713 (input-param alphabet type), E731 (undeclared transition-
//! target state), E732 (undeclared stage in a stage-ref target). More
//! checks (E730 duplicate stage labels, E703/E706 name/type checks, E701
//! exhaustiveness, unused-var warnings) are added as additional passes /
//! helpers.
//!
//! # `.frs` regen
//!
//! Same workflow as `fsm_parser` — edit `fsm_validator.frs`, run framec,
//! rename to `.gen.rs`, commit both.

use std::collections::{HashMap, HashSet};

use crate::frame_c::compiler::frame_ast::{
    BlockAst, Expression, FsmDeclAst, FsmTransitionTarget, Literal, MatchElement, Span, Statement,
    Type,
};

/// One validation finding. `code` is the RFC-0042 §9 diagnostic code
/// (e.g. `"E713"`); `span` locates it; `message` describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct FsmDiagnostic {
    pub code: &'static str,
    pub span: Span,
    pub message: String,
}

/// Validate a parsed `@@fsm` declaration. Returns all findings (not just
/// the first), so the frontend can report them in one pass.
pub fn validate_fsm(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    let mut v = validator_fsm::FsmValidator::__create();
    v.decl = decl.clone();
    v.validate();
    v.diagnostics
}

// ---------------------------------------------------------------------------
// Native check helpers (called from the generated FsmValidator passes)
// ---------------------------------------------------------------------------

/// E713 — the input parameter (first positional) must be `bytes`, `char`,
/// or `token`. Returns the diagnostic if it isn't, else `None`.
pub(crate) fn check_input_param_type(decl: &FsmDeclAst) -> Option<FsmDiagnostic> {
    let p = decl.params.first()?;
    let ok =
        matches!(&p.param_type, Type::Custom(t) if t == "bytes" || t == "char" || t == "token");
    if ok {
        None
    } else {
        let got = match &p.param_type {
            Type::Custom(t) => t.clone(),
            other => format!("{:?}", other),
        };
        Some(FsmDiagnostic {
            code: "E713",
            span: p.span.clone(),
            message: format!(
                "input parameter `{}` must be `bytes`, `char`, or `token`; got `{}`",
                p.name, got
            ),
        })
    }
}

/// Structural checks that need no type inference:
/// - E730: duplicate stage label within a state.
/// - E704: only the first state may be unlabeled (a subsequent unlabeled
///   state has no syntactic separator / cannot be referenced).
/// - E707: a `domain:` field that re-declares a parameter name must have
///   a matching type.
pub(crate) fn check_structure(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    let mut out = Vec::new();

    // E730 — duplicate stage labels within a single state.
    for st in &decl.states {
        let mut seen: HashSet<String> = HashSet::new();
        for m in &st.matches {
            for el in &m.elements {
                if let MatchElement::Stage(s) = el {
                    if let Some(sl) = &s.label {
                        if !seen.insert(sl.clone()) {
                            out.push(FsmDiagnostic {
                                code: "E730",
                                span: s.span.clone(),
                                message: format!(
                                    "stage label `.{}` is used more than once in this state",
                                    sl
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    // E704 — only the first state may be unlabeled.
    for st in decl.states.iter().skip(1) {
        if st.label.is_none() {
            out.push(FsmDiagnostic {
                code: "E704",
                span: st.span.clone(),
                message: "only the first state may be unlabeled; add a `$Label:`".to_string(),
            });
        }
    }

    // E707 — a domain field re-declaring a parameter must match its type.
    if let Some(domain) = &decl.domain {
        let mut param_types: HashMap<&str, &Type> = HashMap::new();
        for p in &decl.params {
            param_types.insert(p.name.as_str(), &p.param_type);
        }
        for v in &domain.vars {
            if let Some(pt) = param_types.get(v.name.as_str()) {
                if !types_equal(pt, &v.var_type) {
                    out.push(FsmDiagnostic {
                        code: "E707",
                        span: v.span.clone(),
                        message: format!(
                            "domain field `{}` re-declares parameter `{}` with a different type",
                            v.name, v.name
                        ),
                    });
                }
            }
        }
    }

    out
}

/// Structural type equality for the opaque `Type::Custom` strings (and
/// the `Unknown` placeholder). Frame has no type system; this is a
/// surface-string comparison, which is all E707 needs.
fn types_equal(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Custom(x), Type::Custom(y)) => x == y,
        (Type::Unknown, Type::Unknown) => true,
        _ => false,
    }
}

/// Names referenced across an fsm body, partitioned by access form:
/// `self.<field>` references (domain / auto-promoted params) and bare
/// identifiers (call targets, initializer-scope param refs). Built by a
/// full expression walk; reused by the unused-name warnings (and, later,
/// by E703 read-of-undeclared).
#[derive(Default)]
struct RefSet {
    /// `self.<field>` references, each with the span of the enclosing
    /// top-level expression/statement (Expression nodes carry no span of
    /// their own, so this is the best available location).
    self_fields: Vec<(String, Span)>,
    /// Bare identifiers (call targets, initializer-scope param refs).
    bare: HashSet<String>,
}

impl RefSet {
    /// The set of distinct `self.<field>` names referenced.
    fn self_field_names(&self) -> HashSet<&str> {
        self.self_fields.iter().map(|(n, _)| n.as_str()).collect()
    }
}

fn walk_expr(e: &Expression, ctx: &Span, refs: &mut RefSet) {
    match e {
        Expression::Var(name) => {
            // Skip `@@:`-probes and `$state.stage` refs — neither names a
            // domain field or parameter.
            if !name.starts_with("@@:") && !name.starts_with('$') {
                refs.bare.insert(name.clone());
            }
        }
        Expression::Member { object, field } => {
            if let Expression::Var(o) = object.as_ref() {
                if o == "self" {
                    refs.self_fields.push((field.clone(), ctx.clone()));
                }
            }
            walk_expr(object, ctx, refs);
        }
        Expression::Binary { left, right, .. } => {
            walk_expr(left, ctx, refs);
            walk_expr(right, ctx, refs);
        }
        Expression::Unary { expr, .. } => walk_expr(expr, ctx, refs),
        Expression::Call { func, args } => {
            refs.bare.insert(func.clone());
            for a in args {
                walk_expr(a, ctx, refs);
            }
        }
        Expression::Assign { target, value } => {
            walk_expr(target, ctx, refs);
            walk_expr(value, ctx, refs);
        }
        Expression::Index { object, index } => {
            walk_expr(object, ctx, refs);
            walk_expr(index, ctx, refs);
        }
        Expression::Literal(_) | Expression::NativeExpr(_) => {}
    }
}

fn walk_block(b: &BlockAst, refs: &mut RefSet) {
    for s in &b.statements {
        walk_stmt(s, refs);
    }
}

fn walk_stmt(s: &Statement, refs: &mut RefSet) {
    match s {
        Statement::Expression(e) => walk_expr(&e.expr, &e.span, refs),
        Statement::If(if_ast) => {
            walk_expr(&if_ast.condition, &if_ast.span, refs);
            walk_stmt(&if_ast.then_branch, refs);
            if let Some(eb) = &if_ast.else_branch {
                walk_stmt(eb, refs);
            }
        }
        Statement::Block(b) => walk_block(b, refs),
        _ => {}
    }
}

/// Walk the whole declaration, collecting every referenced name (with a
/// best-available span for `self.<field>` references).
fn collect_refs(decl: &FsmDeclAst) -> RefSet {
    let mut refs = RefSet::default();
    for st in &decl.states {
        for m in &st.matches {
            for el in &m.elements {
                match el {
                    MatchElement::BareExpression { expr, span } => walk_expr(expr, span, &mut refs),
                    MatchElement::ActionBlock(b) => walk_block(b, &mut refs),
                    MatchElement::Stage(s) => {
                        for ea in &s.embedding_actions {
                            walk_block(&ea.body, &mut refs);
                        }
                    }
                }
            }
            if let Some(t) = &m.transition {
                collect_target_conditions(&t.success, &mut refs);
                if let Some(f) = &t.failure {
                    collect_target_conditions(f, &mut refs);
                }
            }
        }
    }
    if let Some(actions) = &decl.actions {
        for a in &actions.actions {
            walk_block(&a.body, &mut refs);
        }
    }
    if let Some(domain) = &decl.domain {
        for v in &domain.vars {
            walk_expr(&v.default, &v.span, &mut refs);
        }
    }
    refs
}

fn collect_target_conditions(t: &FsmTransitionTarget, refs: &mut RefSet) {
    if let FsmTransitionTarget::Conditional(alts) = t {
        for alt in alts {
            walk_expr(&alt.condition, &alt.span, refs);
        }
    }
}

/// Warning-level checks:
/// - W702: an unused parameter (auto-promoted but never referenced). The
///   input parameter (first positional) is exempt — it's implicitly the
///   recognizer's input.
/// - W703: an unused explicit `domain:` field.
/// - W705: a `when` guard whose condition is the constant `true`.
pub(crate) fn check_warnings(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    let mut out = Vec::new();
    let refs = collect_refs(decl);
    let self_names = refs.self_field_names();

    let used = |name: &str| self_names.contains(name) || refs.bare.contains(name);

    // W702 — unused parameters (skip the input parameter).
    for p in decl.params.iter().skip(1) {
        if !used(&p.name) {
            out.push(FsmDiagnostic {
                code: "W702",
                span: p.span.clone(),
                message: format!("parameter `{}` is never referenced", p.name),
            });
        }
    }

    // W703 — unused domain fields (accessed via `self.<name>`).
    if let Some(domain) = &decl.domain {
        for v in &domain.vars {
            if !self_names.contains(v.name.as_str()) {
                out.push(FsmDiagnostic {
                    code: "W703",
                    span: v.span.clone(),
                    message: format!("domain field `{}` is never referenced", v.name),
                });
            }
        }
    }

    // W705 — constant-true `when` guards.
    for st in &decl.states {
        for m in &st.matches {
            if let Some(t) = &m.transition {
                collect_constant_when(&t.success, &mut out);
                if let Some(f) = &t.failure {
                    collect_constant_when(f, &mut out);
                }
            }
        }
    }

    out
}

fn collect_constant_when(t: &FsmTransitionTarget, out: &mut Vec<FsmDiagnostic>) {
    if let FsmTransitionTarget::Conditional(alts) = t {
        for alt in alts {
            if matches!(alt.condition, Expression::Literal(Literal::Bool(true))) {
                out.push(FsmDiagnostic {
                    code: "W705",
                    span: alt.span.clone(),
                    message:
                        "constant-true `when` guard; use an unguarded target or the failure branch"
                            .to_string(),
                });
            }
        }
    }
}

/// E703 — a `self.<field>` read of an undeclared name. The declared
/// names are the parameters (each auto-promotes to a same-named domain
/// field, §3.2) plus the explicit `domain:` fields. Each undeclared
/// field is reported once, at its first reference.
pub(crate) fn check_undeclared_reads(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    let mut symbols: HashSet<&str> = HashSet::new();
    for p in &decl.params {
        symbols.insert(p.name.as_str());
    }
    if let Some(domain) = &decl.domain {
        for v in &domain.vars {
            symbols.insert(v.name.as_str());
        }
    }

    let refs = collect_refs(decl);
    let mut out = Vec::new();
    let mut reported: HashSet<&str> = HashSet::new();
    for (field, span) in &refs.self_fields {
        if !symbols.contains(field.as_str()) && reported.insert(field.as_str()) {
            out.push(FsmDiagnostic {
                code: "E703",
                span: span.clone(),
                message: format!(
                    "read of undeclared name `self.{}` (no such parameter or domain field)",
                    field
                ),
            });
        }
    }
    out
}

/// E731 / E732 — every transition target must name a declared state
/// (E731), and a stage-ref target `$State.stage` must name a stage that
/// exists in that state (E732).
pub(crate) fn check_transition_targets(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    // Declared state labels, and each state's set of stage labels.
    let mut labels: HashSet<String> = HashSet::new();
    let mut stages: HashMap<String, HashSet<String>> = HashMap::new();
    for st in &decl.states {
        if let Some(label) = &st.label {
            labels.insert(label.clone());
            let mut sset = HashSet::new();
            for m in &st.matches {
                for el in &m.elements {
                    if let MatchElement::Stage(s) = el {
                        if let Some(sl) = &s.label {
                            sset.insert(sl.clone());
                        }
                    }
                }
            }
            stages.insert(label.clone(), sset);
        }
    }

    let mut out = Vec::new();
    for st in &decl.states {
        for m in &st.matches {
            if let Some(t) = &m.transition {
                check_target(&t.success, &labels, &stages, &mut out);
                if let Some(f) = &t.failure {
                    check_target(f, &labels, &stages, &mut out);
                }
            }
        }
    }
    out
}

/// Recursively check one transition target (static or conditional).
fn check_target(
    target: &FsmTransitionTarget,
    labels: &HashSet<String>,
    stages: &HashMap<String, HashSet<String>>,
    out: &mut Vec<FsmDiagnostic>,
) {
    match target {
        FsmTransitionTarget::Static { state, stage, span } => {
            if !labels.contains(state) {
                out.push(FsmDiagnostic {
                    code: "E731",
                    span: span.clone(),
                    message: format!("reference to undeclared state `${}`", state),
                });
                return;
            }
            if let Some(stage_name) = stage {
                let has = stages
                    .get(state)
                    .map(|s| s.contains(stage_name))
                    .unwrap_or(false);
                if !has {
                    out.push(FsmDiagnostic {
                        code: "E732",
                        span: span.clone(),
                        message: format!(
                            "reference to undeclared stage `${}.{}`",
                            state, stage_name
                        ),
                    });
                }
            }
        }
        FsmTransitionTarget::Conditional(alts) => {
            for alt in alts {
                check_target(&alt.target, labels, stages, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Generated FSM
// ---------------------------------------------------------------------------

mod validator_fsm {
    #![allow(
        unreachable_patterns,
        unused_mut,
        dead_code,
        non_snake_case,
        unused_variables,
        unused_parens
    )]

    use super::{
        check_input_param_type, check_structure, check_transition_targets, check_undeclared_reads,
        check_warnings, FsmDiagnostic,
    };
    use crate::frame_c::compiler::frame_ast::FsmDeclAst;

    include!("fsm_validator.gen.rs");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::fsm_parser::parse_fsm_block;

    fn diags(src: &[u8]) -> Vec<FsmDiagnostic> {
        let ast = parse_fsm_block(src).expect("fixture must parse");
        validate_fsm(&ast)
    }

    /// A valid fsm produces no diagnostics.
    #[test]
    fn clean_fsm_has_no_diagnostics() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ true }");
        assert!(d.is_empty(), "expected no diagnostics, got {:?}", d);
    }

    /// E713 — input parameter type must be bytes/char/token.
    #[test]
    fn e713_bad_input_type() {
        // `float` first param. (Parser accepts any ident type; validator rejects.)
        let d = diags(b"@@fsm M(text: float) : bool = false { /a/ true }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "E713");
    }

    /// char and token alphabets are accepted by E713.
    #[test]
    fn e713_accepts_char_and_token() {
        assert!(diags(b"@@fsm M(text: char) : bool = false { /a/ true }").is_empty());
        assert!(diags(b"@@fsm M(toks: token) : bool = false { /IDENT/ true }").is_empty());
    }

    /// E731 — a transition target naming an undeclared state.
    #[test]
    fn e731_undeclared_state() {
        let d =
            diags(b"@@fsm M(text: bytes) : bool = false { /a/ -> $nope : -> $err  $err: false }");
        assert!(d.iter().any(|x| x.code == "E731"));
    }

    /// E732 — a stage-ref target naming an undeclared stage.
    #[test]
    fn e732_undeclared_stage() {
        let d = diags(
            b"@@fsm M(text: bytes) : bool = false { $a: /x/ -> $a.nosuch : -> $err  $err: false }",
        );
        assert!(d.iter().any(|x| x.code == "E732"));
    }

    /// A valid stage-ref target (declared stage) is accepted.
    #[test]
    fn valid_stage_ref_ok() {
        let d = diags(
            b"@@fsm M(text: bytes) : bool = false { $other: /y/ -> $a.s : -> $err  $a: .s/x/ true  $err: false }",
        );
        assert!(
            !d.iter().any(|x| x.code == "E731" || x.code == "E732"),
            "got {:?}",
            d
        );
    }

    /// E730 — a stage label used twice within one state.
    #[test]
    fn e730_duplicate_stage_label() {
        let d = diags(b"@@fsm M(text: bytes) : bytes = \"\" { $s: .x/[0-9]+/ .x/[a-z]+/ $s.x }");
        assert!(d.iter().any(|x| x.code == "E730"), "got {:?}", d);
    }

    /// E704 — a second, unlabeled state.
    #[test]
    fn e704_second_unlabeled_state() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ -> $x  /b/ true  $x: false }");
        assert!(d.iter().any(|x| x.code == "E704"), "got {:?}", d);
    }

    /// E707 — a domain field re-declaring a parameter with a different type.
    #[test]
    fn e707_domain_param_type_mismatch() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ true  domain: text: int = 0 }");
        assert!(d.iter().any(|x| x.code == "E707"), "got {:?}", d);
    }

    /// A domain field re-declaring a parameter with the SAME type is fine.
    #[test]
    fn domain_param_same_type_ok() {
        let d = diags(
            b"@@fsm M(text: bytes, n: int = 0) : int = 0 { /[0-9]/ self.n  domain: n: int = 5 }",
        );
        assert!(!d.iter().any(|x| x.code == "E707"), "got {:?}", d);
    }

    /// W702 — an unused (non-input) parameter.
    #[test]
    fn w702_unused_parameter() {
        let d = diags(b"@@fsm M(text: bytes, unused: int = 0) : bool = false { /a/ true }");
        assert!(d.iter().any(|x| x.code == "W702"), "got {:?}", d);
    }

    /// A referenced parameter (via self.<name>) is not flagged W702; nor is
    /// the input parameter ever flagged (it's the implicit input).
    #[test]
    fn w702_referenced_and_input_ok() {
        let d = diags(
            b"@@fsm M(text: bytes, threshold: int = 0) : bool = false { /[0-9]+/ to_int(@@:matched) > self.threshold }",
        );
        assert!(!d.iter().any(|x| x.code == "W702"), "got {:?}", d);
    }

    /// W703 — an unused domain field.
    #[test]
    fn w703_unused_domain_field() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ true  domain: unused: int = 0 }");
        assert!(d.iter().any(|x| x.code == "W703"), "got {:?}", d);
    }

    /// A referenced domain field is not flagged W703.
    #[test]
    fn w703_referenced_domain_ok() {
        let d = diags(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count  domain: count: int = 0 }",
        );
        assert!(!d.iter().any(|x| x.code == "W703"), "got {:?}", d);
    }

    /// W705 — a constant-true `when` guard.
    #[test]
    fn w705_constant_true_when() {
        let d = diags(
            b"@@fsm M(text: bytes) : int = 0 { /[01]/ -> ( $a when true ) : -> $err  $a: 1  $err: -1 }",
        );
        assert!(d.iter().any(|x| x.code == "W705"), "got {:?}", d);
    }

    /// A real `when` condition is not flagged W705.
    #[test]
    fn w705_real_condition_ok() {
        let d = diags(
            b"@@fsm M(text: bytes, mode: int) : int = 0 { /[01]/ -> ( $a when self.mode == 0 ) : -> $err  $a: 1  $err: -1 }",
        );
        assert!(!d.iter().any(|x| x.code == "W705"), "got {:?}", d);
    }

    /// E703 — reading a `self.<field>` that names no parameter or domain field.
    #[test]
    fn e703_undeclared_read() {
        let d = diags(b"@@fsm M(text: bytes) : int = 0 { /a/ { self.count = self.nope } self.count  domain: count: int = 0 }");
        assert!(d.iter().any(|x| x.code == "E703"), "got {:?}", d);
    }

    /// Reads of a declared domain field and of an auto-promoted parameter
    /// (`self.<param>`) are NOT flagged E703.
    #[test]
    fn e703_declared_and_promoted_ok() {
        // self.count (domain) + self.threshold (auto-promoted param) both declared.
        let d = diags(
            b"@@fsm M(text: bytes, threshold: int = 0) : int = 0 { /[0-9]+/ { self.count = self.threshold } self.count  domain: count: int = 0 }",
        );
        assert!(!d.iter().any(|x| x.code == "E703"), "got {:?}", d);
    }
}
