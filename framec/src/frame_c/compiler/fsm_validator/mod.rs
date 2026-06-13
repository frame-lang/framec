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
//! E713 (input-param alphabet type), E730 (duplicate stage label), E704
//! (unlabeled non-start state), E707 (domain/param type mismatch), E731 /
//! E732 (undeclared transition state / stage), E703 (read of undeclared
//! name), W701/W702/W703/W705 (unused / silent-reject / constant-guard
//! warnings), and — via the `$CheckRegex` pass over each stage's regex —
//! E720/E721/E722/E723 + W704 forwarded from [`crate::frame_c::compiler::fsm_regex`],
//! plus E701 match exhaustiveness (§4.3). E706 (assignment/return type
//! mismatch) is out of scope: Frame has no type system.
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
use crate::frame_c::compiler::fsm_regex::Alphabet;

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
    validate_fsm_in_module(decl, std::slice::from_ref(decl))
}

/// Validate `decl` with visibility of the other fsms declared in the same
/// module (`module`, which includes `decl` itself). The extra context is
/// only needed for Mode C call-out checks (§8.3): resolving a `/@Inner/`
/// reference to compare its alphabet against the outer fsm (E731). All
/// other checks are intra-fsm and ignore `module`.
pub fn validate_fsm_in_module(decl: &FsmDeclAst, module: &[FsmDeclAst]) -> Vec<FsmDiagnostic> {
    let mut v = validator_fsm::FsmValidator::__create();
    v.decl = decl.clone();
    v.validate();
    let mut diags = v.diagnostics;
    diags.extend(check_mode_c(decl, module));
    diags.extend(check_bare_names(decl));
    diags
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
    /// their own, so this is the best available location). Includes both
    /// reads and assignment targets, so "referenced" (for unused-name
    /// warnings) covers writes too.
    self_fields: Vec<(String, Span)>,
    /// Names of `self.<field>` that appear as an assignment *target* — a
    /// write. Used to pick E704 (write) over E703 (read) for an undeclared
    /// name (§4.2: writing to an undeclared name is E704).
    self_writes: HashSet<String>,
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
            // A top-level `self.<field>` on the left of `=` is a write — note
            // it so an undeclared write reports E704 rather than the E703 read
            // code. The target is still walked normally below, so it continues
            // to count as a reference for the unused-name warnings.
            if let Expression::Member { object, field } = target.as_ref() {
                if matches!(object.as_ref(), Expression::Var(o) if o == "self") {
                    refs.self_writes.insert(field.clone());
                }
            }
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
                if let Some(s) = &t.success {
                    collect_target_conditions(s, &mut refs);
                }
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
    // W701 — a conditional success target with no failure branch: if no
    // alternative's `when` holds, the match falls through to a silent
    // reject. The writer probably wanted an explicit fallback (§3.5.4.1,
    // §5). A conditional in the failure position needs no warning — it is
    // already the fallback.
    for st in &decl.states {
        for m in &st.matches {
            if let Some(t) = &m.transition {
                if let Some(s) = &t.success {
                    collect_constant_when(s, &mut out);
                }
                if let Some(f) = &t.failure {
                    collect_constant_when(f, &mut out);
                }
                if matches!(&t.success, Some(FsmTransitionTarget::Conditional(_)))
                    && t.failure.is_none()
                {
                    out.push(FsmDiagnostic {
                        code: "W701",
                        span: t.span.clone(),
                        message: "conditional transition target has no failure branch; \
                                  unmatched input is silently rejected"
                            .to_string(),
                    });
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

/// The regex alphabet implied by the fsm's input parameter type
/// (RFC-0042 §6.1). Defaults to `Bytes` when the type is absent or
/// unrecognized (E713 reports the bad type separately).
fn alphabet_of(decl: &FsmDeclAst) -> Alphabet {
    match decl.params.first().map(|p| &p.param_type) {
        Some(Type::Custom(t)) if t == "char" => Alphabet::Char,
        Some(Type::Custom(t)) if t == "token" => Alphabet::Token,
        _ => Alphabet::Bytes,
    }
}

/// Does the fsm opt into Unicode `\p{...}` classes via
/// `@@[allow(unicode_classes)]` (RFC-0042 §11.6)?
fn allows_unicode_classes(decl: &FsmDeclAst) -> bool {
    decl.attributes
        .iter()
        .any(|a| a == "allow(unicode_classes)")
}

/// The source-spelling of an alphabet, for diagnostics.
fn alphabet_name(a: Alphabet) -> &'static str {
    match a {
        Alphabet::Bytes => "bytes",
        Alphabet::Char => "char",
        Alphabet::Token => "token",
    }
}

/// E731 / E732 — Mode C call-out (`/@Fsm/`, RFC-0042 §8.3) static checks.
///
/// A match stage whose regex body is `@<name>` is a Mode C reference to an
/// inner fsm (the `/@.../ ` form; `mode_c_inner` in the backends strips the
/// same leading `@`). Two requirements are enforced statically:
///
/// - **E732 — dynamic dispatch (FSM-TEST-704).** `<name>` must name an fsm
///   *statically*, not select one at run time. If `<name>` is one of this
///   fsm's parameters or domain fields, the target is a runtime value, which
///   v0.1 does not support.
/// - **E731 — alphabet mismatch (FSM-TEST-703).** Mode C drives the inner
///   recognizer over the *same* input, so the inner fsm must declare the same
///   alphabet as the outer one (a `char` outer cannot drive a `bytes` inner).
///   Checked only when `<name>` resolves to a sibling fsm in `module`; an
///   unresolved name is left to other passes (no false E731/E732).
///
/// The two are mutually exclusive per reference: a runtime name is reported
/// E732 and not also probed for an alphabet match.
pub(crate) fn check_mode_c(decl: &FsmDeclAst, module: &[FsmDeclAst]) -> Vec<FsmDiagnostic> {
    let mut out = Vec::new();
    let outer_alpha = alphabet_of(decl);

    // Parameters and domain fields are the fsm's runtime values; a Mode C
    // name matching one of these is dynamic dispatch.
    let mut runtime: HashSet<&str> = decl.params.iter().map(|p| p.name.as_str()).collect();
    if let Some(dom) = &decl.domain {
        for v in &dom.vars {
            runtime.insert(v.name.as_str());
        }
    }

    for st in &decl.states {
        for m in &st.matches {
            for el in &m.elements {
                let MatchElement::Stage(stage) = el else {
                    continue;
                };
                let Some(target) = stage.regex.strip_prefix('@') else {
                    continue;
                };
                if runtime.contains(target) {
                    out.push(FsmDiagnostic {
                        code: "E732",
                        span: stage.span.clone(),
                        message: format!(
                            "Mode C reference `/@{target}/` is dynamic: `{target}` is a runtime \
                             value, not a statically-named fsm. v0.1 requires the inner fsm to be \
                             named statically — use a conditional target with explicit `/@Fsm/` \
                             alternatives, or Mode A composition from `@@system`."
                        ),
                    });
                    continue;
                }
                if let Some(inner) = module.iter().find(|d| d.name == target) {
                    let inner_alpha = alphabet_of(inner);
                    if inner_alpha != outer_alpha {
                        out.push(FsmDiagnostic {
                            code: "E731",
                            span: stage.span.clone(),
                            message: format!(
                                "Mode C alphabet mismatch: outer fsm `{}` is `{}` but inner fsm \
                                 `{}` is `{}`. Mode C composition requires matching alphabets — \
                                 change one declaration so both agree.",
                                decl.name,
                                alphabet_name(outer_alpha),
                                target,
                                alphabet_name(inner_alpha),
                            ),
                        });
                    }
                }
            }
        }
    }
    out
}

/// Compile every stage regex through [`fsm_regex::compile`] and surface
/// its diagnostics, plus the match-level exhaustiveness check:
///
/// - **E720–E723 / W704** — forwarded from the regex engine (forbidden
///   constructs, invalid syntax, empty regex, DFA-size limits), located
///   at the offending stage.
/// - **Anchors** — the v0.1 DFA engine does not yet fold zero-width
///   anchors in; reported as a tracked engine limitation (E722) rather
///   than miscompiled. See [`fsm_regex`]'s `subset` module note.
/// - **E701** — a match with a success transition (`-> ...`) but no
///   failure branch is permitted only when it cannot fail. A match is
///   provably non-failing when every stage's regex accepts the empty
///   string (RFC-0042 §4.3); otherwise E701.
pub(crate) fn check_regexes(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    use crate::frame_c::compiler::fsm_regex::{
        self, size_check::DEFAULT_MAX_DFA_STATES, CompileError,
    };

    let alphabet = alphabet_of(decl);
    let mut out = Vec::new();

    for st in &decl.states {
        // In a multi-match (`|`) state the first stage of each alternative
        // is the ordered-choice selector: a first-stage miss falls through
        // to the next alternative rather than to §5.6, so it does not make
        // the match "fail" for the E701 exhaustiveness check (RFC-0042 §3.4).
        let multi = st.matches.len() > 1;
        for m in &st.matches {
            let mut has_stage = false;
            // Can this match reach §5.6 (an unhandled failure)? True once a
            // fallible stage exists whose failure isn't absorbed by ordered
            // choice. A regex that fails to compile or uses anchors can fail.
            let mut can_fail = false;
            let mut stage_idx = 0usize;

            for el in &m.elements {
                if let MatchElement::Stage(stage) = el {
                    has_stage = true;
                    let is_selector = multi && stage_idx == 0;
                    stage_idx += 1;
                    // A Mode C stage (`/@Inner/`, §8.3) is a sub-fsm call-out,
                    // not a regex: skip DFA compilation. It can fail (the
                    // inner fsm may reject), so it is never nullable.
                    if stage.regex.starts_with('@') {
                        if !is_selector {
                            can_fail = true;
                        }
                        continue;
                    }
                    let nullable =
                        match fsm_regex::compile(&stage.regex, alphabet, DEFAULT_MAX_DFA_STATES) {
                            Ok(compiled) => {
                                for w in compiled.warnings {
                                    out.push(FsmDiagnostic {
                                        code: w.code,
                                        span: stage.span.clone(),
                                        message: w.message,
                                    });
                                }
                                // Opt-in gate (§11.6): a `\p{...}` Unicode class
                                // compiles only when the fsm carries
                                // `@@[allow(unicode_classes)]`; otherwise E720.
                                if compiled.used_unicode_class && !allows_unicode_classes(decl) {
                                    out.push(FsmDiagnostic {
                                        code: "E720",
                                        span: stage.span.clone(),
                                        message: "Unicode class `\\p{...}` requires the \
                                             `@@[allow(unicode_classes)]` attribute on the fsm \
                                             (RFC-0042 §11.6)"
                                            .to_string(),
                                    });
                                }
                                // Nullability (can the stage match the empty
                                // string?) drives the E701 exhaustiveness check.
                                // A lazy stage has an empty placeholder DFA and
                                // matches via the Pike program, so derive it
                                // from the program in that case (§11.1).
                                match &compiled.program {
                                    Some(prog) => {
                                        // Empty input ⇒ no word chars; pass an
                                        // empty word table for the `\b` predicate.
                                        crate::frame_c::compiler::fsm_regex::pike::run(
                                            prog,
                                            &[],
                                            0,
                                            &[],
                                        )
                                        .is_some()
                                    }
                                    None => compiled.dfa.states[compiled.dfa.start].is_accept,
                                }
                            }
                            Err(CompileError::Diagnostics(ds)) => {
                                for d in ds {
                                    out.push(FsmDiagnostic {
                                        code: d.code,
                                        span: stage.span.clone(),
                                        message: d.message,
                                    });
                                }
                                false
                            }
                            Err(CompileError::UnsupportedAnchors(_)) => {
                                out.push(FsmDiagnostic {
                                    code: "E722",
                                    span: stage.span.clone(),
                                    message:
                                        "only a leading `^`/`\\A` or trailing `$`/`\\z` anchor is \
                                          supported in v0.1; mid-pattern anchors and `\\b`/`\\B` \
                                          are deferred to v0.2"
                                            .to_string(),
                                });
                                false
                            }
                        };
                    if !nullable && !is_selector {
                        can_fail = true;
                    }
                }
            }

            // E701 — success transition with no failure branch on a match
            // that can fail.
            if let Some(clause) = &m.transition {
                if clause.failure.is_none() && has_stage && can_fail {
                    out.push(FsmDiagnostic {
                        code: "E701",
                        span: m.span.clone(),
                        message:
                            "this match can fail but has no failure branch; add `: -> $State` \
                                  (a match without a failure branch must be provably non-failing)"
                                .to_string(),
                    });
                }
            }
        }
    }

    out
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
            // A write to an undeclared name is E704; a read is E703 (§4.2).
            let (code, verb) = if refs.self_writes.contains(field) {
                ("E704", "write to")
            } else {
                ("E703", "read of")
            };
            out.push(FsmDiagnostic {
                code,
                span: span.clone(),
                message: format!(
                    "{} undeclared name `self.{}` (no such parameter or domain field)",
                    verb, field
                ),
            });
        }
    }
    out
}

/// FSM-TEST-033 / E703 — a *bare* identifier (no `self.` prefix) that names
/// a parameter or domain field. Outside `domain:` initializer expressions,
/// domain/parameter access requires the `self.` prefix (§4.2): `count` is not
/// in scope, only `self.count` is. A bare name that collides with a known
/// field is almost always a missing `self.`, so it is rejected with that hint.
///
/// The check is deliberately scoped to names that *match a declared field* so
/// it cannot misfire on the legitimate bare forms: call/action targets, `@@:`
/// probes, `$state` refs, native expressions, action-parameter locals, and
/// `domain:` initializer param references (FSM-TEST-103, which are not walked
/// here at all).
pub(crate) fn check_bare_names(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    let mut fields: HashSet<&str> = decl.params.iter().map(|p| p.name.as_str()).collect();
    if let Some(dom) = &decl.domain {
        for v in &dom.vars {
            fields.insert(v.name.as_str());
        }
    }

    // Collect bare `Var` references from body contexts only (never domain
    // initializers). Each is a candidate missing-`self.`.
    let mut bares: Vec<(String, Span)> = Vec::new();
    for st in &decl.states {
        for m in &st.matches {
            for el in &m.elements {
                match el {
                    MatchElement::BareExpression { expr, span } => {
                        collect_bare_vars(expr, span, &mut bares)
                    }
                    MatchElement::ActionBlock(b) => collect_bare_block(b, &mut bares),
                    MatchElement::Stage(s) => {
                        for ea in &s.embedding_actions {
                            collect_bare_block(&ea.body, &mut bares);
                        }
                    }
                }
            }
            if let Some(t) = &m.transition {
                if let Some(s) = &t.success {
                    collect_bare_target(s, &mut bares);
                }
                if let Some(f) = &t.failure {
                    collect_bare_target(f, &mut bares);
                }
            }
        }
    }
    // Declared-action bodies: a bare name that is one of the action's own
    // parameters is a local, not a field access — exclude those.
    if let Some(actions) = &decl.actions {
        for a in &actions.actions {
            let locals: HashSet<&str> = a.params.iter().map(|p| p.name.as_str()).collect();
            let mut body_bares = Vec::new();
            collect_bare_block(&a.body, &mut body_bares);
            bares.extend(
                body_bares
                    .into_iter()
                    .filter(|(n, _)| !locals.contains(n.as_str())),
            );
        }
    }

    let mut out = Vec::new();
    let mut reported: HashSet<String> = HashSet::new();
    for (name, span) in bares {
        if fields.contains(name.as_str()) && reported.insert(name.clone()) {
            out.push(FsmDiagnostic {
                code: "E703",
                span,
                message: format!(
                    "bare name `{name}` does not refer to the domain field `self.{name}`; \
                     domain access requires the `self.` prefix outside of initializer \
                     expressions — write `self.{name}`"
                ),
            });
        }
    }
    out
}

/// Collect bare identifier references (reads and assignment targets) from `e`,
/// EXCLUDING call/action targets, `self`, and `@@:`/`$` forms. Mirrors
/// [`walk_expr`] but records only bare `Var`s — the candidates for a missing
/// `self.` ([`check_bare_names`]).
fn collect_bare_vars(e: &Expression, ctx: &Span, out: &mut Vec<(String, Span)>) {
    match e {
        Expression::Var(name) => {
            if name != "self" && !name.starts_with("@@:") && !name.starts_with('$') {
                out.push((name.clone(), ctx.clone()));
            }
        }
        // `self.field` / `obj.field`: walk the object (so `self` is skipped and
        // a bare `obj` is still caught); the field name is not a bare ref.
        Expression::Member { object, .. } => collect_bare_vars(object, ctx, out),
        Expression::Binary { left, right, .. } => {
            collect_bare_vars(left, ctx, out);
            collect_bare_vars(right, ctx, out);
        }
        Expression::Unary { expr, .. } => collect_bare_vars(expr, ctx, out),
        // The call target is an action/native name, not a domain-field access;
        // record only the argument expressions.
        Expression::Call { args, .. } => {
            for a in args {
                collect_bare_vars(a, ctx, out);
            }
        }
        Expression::Assign { target, value } => {
            collect_bare_vars(target, ctx, out);
            collect_bare_vars(value, ctx, out);
        }
        Expression::Index { object, index } => {
            collect_bare_vars(object, ctx, out);
            collect_bare_vars(index, ctx, out);
        }
        Expression::Literal(_) | Expression::NativeExpr(_) => {}
    }
}

fn collect_bare_block(b: &BlockAst, out: &mut Vec<(String, Span)>) {
    for s in &b.statements {
        collect_bare_stmt(s, out);
    }
}

fn collect_bare_stmt(s: &Statement, out: &mut Vec<(String, Span)>) {
    match s {
        Statement::Expression(e) => collect_bare_vars(&e.expr, &e.span, out),
        Statement::If(if_ast) => {
            collect_bare_vars(&if_ast.condition, &if_ast.span, out);
            collect_bare_stmt(&if_ast.then_branch, out);
            if let Some(eb) = &if_ast.else_branch {
                collect_bare_stmt(eb, out);
            }
        }
        Statement::Block(b) => collect_bare_block(b, out),
        _ => {}
    }
}

fn collect_bare_target(t: &FsmTransitionTarget, out: &mut Vec<(String, Span)>) {
    if let FsmTransitionTarget::Conditional(alts) = t {
        for alt in alts {
            collect_bare_vars(&alt.condition, &alt.span, out);
        }
    }
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
                if let Some(s) = &t.success {
                    check_target(s, &labels, &stages, &mut out);
                }
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

/// E704 / E731 / E732 — stage-capture references read in *value* position.
///
/// [`check_transition_targets`] validates captures that appear as transition
/// targets (`-> $S.stage`). This pass validates the other place a capture can
/// name a state/stage: as an expression value — a bare expression, a statement
/// inside an action block, or an embedding-action body (§3.5.2). Without it,
/// `$0.x` against an unlabeled state compiles to a dangling capture lookup —
/// the "phantom successful compile" FSM-TEST-008 guards against.
///
/// Disambiguation mirrors §3.4 / §3.5.2 and the transition-target codes:
/// - state names a declared label: a non-numeric stage absent from that state
///   is `E732`; a numeric stage is a positional ref (§3.5.2) and is accepted.
/// - state is not a declared label: `E704` when the *enclosing* state has no
///   label (a capture ref to an unlabeled state — the fix is to label it),
///   else `E731` (reference to an undeclared state).
pub(crate) fn check_capture_refs(decl: &FsmDeclAst) -> Vec<FsmDiagnostic> {
    // Declared state labels + each labeled state's set of stage labels —
    // built exactly as in `check_transition_targets`.
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
        let enclosing_labeled = st.label.is_some();
        for m in &st.matches {
            for el in &m.elements {
                match el {
                    MatchElement::BareExpression { expr, span } => {
                        check_expr_captures(
                            expr,
                            span,
                            enclosing_labeled,
                            &labels,
                            &stages,
                            &mut out,
                        );
                    }
                    MatchElement::ActionBlock(block) => {
                        check_block_captures(block, enclosing_labeled, &labels, &stages, &mut out);
                    }
                    MatchElement::Stage(s) => {
                        for ea in &s.embedding_actions {
                            check_block_captures(
                                &ea.body,
                                enclosing_labeled,
                                &labels,
                                &stages,
                                &mut out,
                            );
                        }
                    }
                }
            }
        }
    }
    out
}

/// Walk a `{ ... }` block's statements for value-position captures.
fn check_block_captures(
    block: &BlockAst,
    enclosing_labeled: bool,
    labels: &HashSet<String>,
    stages: &HashMap<String, HashSet<String>>,
    out: &mut Vec<FsmDiagnostic>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Expression(e) => {
                check_expr_captures(&e.expr, &e.span, enclosing_labeled, labels, stages, out);
            }
            Statement::If(if_ast) => {
                check_expr_captures(
                    &if_ast.condition,
                    &if_ast.span,
                    enclosing_labeled,
                    labels,
                    stages,
                    out,
                );
                check_stmt_captures(&if_ast.then_branch, enclosing_labeled, labels, stages, out);
                if let Some(eb) = &if_ast.else_branch {
                    check_stmt_captures(eb, enclosing_labeled, labels, stages, out);
                }
            }
            Statement::Block(b) => check_block_captures(b, enclosing_labeled, labels, stages, out),
            _ => {}
        }
    }
}

/// A single statement that may carry captures (an `if` branch body).
fn check_stmt_captures(
    stmt: &Statement,
    enclosing_labeled: bool,
    labels: &HashSet<String>,
    stages: &HashMap<String, HashSet<String>>,
    out: &mut Vec<FsmDiagnostic>,
) {
    match stmt {
        Statement::Block(b) => check_block_captures(b, enclosing_labeled, labels, stages, out),
        Statement::Expression(e) => {
            check_expr_captures(&e.expr, &e.span, enclosing_labeled, labels, stages, out);
        }
        Statement::If(if_ast) => {
            check_expr_captures(
                &if_ast.condition,
                &if_ast.span,
                enclosing_labeled,
                labels,
                stages,
                out,
            );
            check_stmt_captures(&if_ast.then_branch, enclosing_labeled, labels, stages, out);
            if let Some(eb) = &if_ast.else_branch {
                check_stmt_captures(eb, enclosing_labeled, labels, stages, out);
            }
        }
        _ => {}
    }
}

/// Recurse through an expression, validating every `$state.stage` capture.
fn check_expr_captures(
    expr: &Expression,
    span: &Span,
    enclosing_labeled: bool,
    labels: &HashSet<String>,
    stages: &HashMap<String, HashSet<String>>,
    out: &mut Vec<FsmDiagnostic>,
) {
    match expr {
        Expression::Var(name) => {
            if let Some(body) = name.strip_prefix('$') {
                validate_capture(body, span, enclosing_labeled, labels, stages, out);
            }
        }
        Expression::Member { object, .. } => {
            // Mode C `$state.stage.return_value`: the capture lives in `object`
            // (`$state.stage`); the field is read off the inner instance.
            check_expr_captures(object, span, enclosing_labeled, labels, stages, out);
        }
        Expression::Binary { left, right, .. } => {
            check_expr_captures(left, span, enclosing_labeled, labels, stages, out);
            check_expr_captures(right, span, enclosing_labeled, labels, stages, out);
        }
        Expression::Unary { expr, .. } => {
            check_expr_captures(expr, span, enclosing_labeled, labels, stages, out);
        }
        Expression::Call { args, .. } => {
            for a in args {
                check_expr_captures(a, span, enclosing_labeled, labels, stages, out);
            }
        }
        Expression::Index { object, index } => {
            check_expr_captures(object, span, enclosing_labeled, labels, stages, out);
            check_expr_captures(index, span, enclosing_labeled, labels, stages, out);
        }
        Expression::Assign { target, value } => {
            check_expr_captures(target, span, enclosing_labeled, labels, stages, out);
            check_expr_captures(value, span, enclosing_labeled, labels, stages, out);
        }
        Expression::Literal(_) | Expression::NativeExpr(_) => {}
    }
}

/// Validate one `$<body>` capture name (`body` is `state` or `state.stage`).
fn validate_capture(
    body: &str,
    span: &Span,
    enclosing_labeled: bool,
    labels: &HashSet<String>,
    stages: &HashMap<String, HashSet<String>>,
    out: &mut Vec<FsmDiagnostic>,
) {
    let mut parts = body.splitn(2, '.');
    let state = parts.next().unwrap_or("");
    let stage = parts.next();

    if labels.contains(state) {
        if let Some(stage) = stage {
            // Positional refs (`$state.0`, §3.5.2) address unlabeled stages by
            // index — accept any all-digit stage token without checking.
            let numeric = !stage.is_empty() && stage.bytes().all(|b| b.is_ascii_digit());
            let known = numeric
                || stages
                    .get(state)
                    .map(|s| s.contains(stage))
                    .unwrap_or(false);
            if !known {
                out.push(FsmDiagnostic {
                    code: "E732",
                    span: span.clone(),
                    message: format!(
                        "reference to undeclared stage `.{}` in state `${}`",
                        stage, state
                    ),
                });
            }
        }
    } else if !enclosing_labeled {
        out.push(FsmDiagnostic {
            code: "E704",
            span: span.clone(),
            message: format!(
                "stage-capture reference `${}` requires the enclosing state to carry an explicit `$Label:`",
                body
            ),
        });
    } else {
        out.push(FsmDiagnostic {
            code: "E731",
            span: span.clone(),
            message: format!("reference to undeclared state `${}`", state),
        });
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
        check_capture_refs, check_input_param_type, check_regexes, check_structure,
        check_transition_targets, check_undeclared_reads, check_warnings, FsmDiagnostic,
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

    /// FSM-TEST-010 — input parameter type validation.
    /// E713 — input parameter type must be bytes/char/token.
    #[test]
    fn e713_bad_input_type() {
        // `float` first param. (Parser accepts any ident type; validator rejects.)
        let d = diags(b"@@fsm M(text: float) : bool = false { /a/ true }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "E713");
    }

    /// FSM-TEST-251 — char alphabet (accepted; the runtime behavior is
    /// exercised by the codegen backends). FSM-TEST-253's token alphabet is
    /// likewise accepted here.
    /// char and token alphabets are accepted by E713.
    #[test]
    fn e713_accepts_char_and_token() {
        assert!(diags(b"@@fsm M(text: char) : bool = false { /a/ true }").is_empty());
        assert!(diags(b"@@fsm M(toks: token) : bool = false { /IDENT/ true }").is_empty());
    }

    /// FSM-TEST-033 — bare name does not refer to domain. `count = count + 1`
    /// inside an action block uses the bare name `count`, but domain access
    /// requires `self.` outside initializer expressions; both the write and the
    /// read are rejected with E703 pointing at `self.count`.
    #[test]
    fn e703_bare_name_not_domain() {
        let d = diags(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { count = count + 1 } self.count \
              domain: count: int = 0 }",
        );
        assert!(
            d.iter().any(|x| x.code == "E703"),
            "expected E703 for bare `count`, got {:?}",
            d
        );
        // The `self.`-qualified form is accepted (no E703 from this check).
        let ok = diags(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count \
              domain: count: int = 0 }",
        );
        assert!(
            !ok.iter().any(|x| x.code == "E703"),
            "self.count must not raise E703, got {:?}",
            ok
        );
        // FSM-TEST-103 guard: a bare *parameter* reference inside a `domain:`
        // initializer is legal and must not be flagged.
        let init = diags(
            b"@@fsm M(text: bytes, n: int) : int = 0 { /[0-9]/ self.seed \
              domain: seed: int = n }",
        );
        assert!(
            !init.iter().any(|x| x.code == "E703"),
            "bare param in a domain initializer must not raise E703, got {:?}",
            init
        );
    }

    /// FSM-TEST-703 — Mode C alphabet mismatch. A `char`-alphabet outer fsm
    /// `/@A/`-references a `bytes`-alphabet inner fsm; the alphabets disagree,
    /// so the reference is rejected with E731 (§8.3). A matching-alphabet pair
    /// is accepted.
    #[test]
    fn e731_mode_c_alphabet_mismatch() {
        let a =
            parse_fsm_block(b"@@fsm A(text: bytes) : bool = false { /a/ true }").expect("A parses");
        let b = parse_fsm_block(b"@@fsm B(input: char) : bool = false { /@A/ true }")
            .expect("B parses");
        let module = vec![a.clone(), b.clone()];
        let d = validate_fsm_in_module(&b, &module);
        assert!(
            d.iter().any(|x| x.code == "E731"),
            "expected E731 alphabet mismatch, got {:?}",
            d
        );
        // Same alphabet on both ⇒ no mismatch.
        let a2 =
            parse_fsm_block(b"@@fsm A(text: char) : bool = false { /a/ true }").expect("A2 parses");
        let module2 = vec![a2.clone(), b.clone()];
        assert!(
            !validate_fsm_in_module(&b, &module2)
                .iter()
                .any(|x| x.code == "E731"),
            "matching alphabets must not raise E731"
        );
    }

    /// FSM-TEST-704 — Mode C dynamic dispatch rejected. `/@which/` where
    /// `which` is a runtime parameter is not a statically-named fsm, so it is
    /// rejected with E732 (§8.3).
    #[test]
    fn e732_mode_c_dynamic_dispatch() {
        let d = diags(
            b"@@fsm M(text: bytes, which: FsmRef) : int = 0 { $m: /@which/  $m.0.return_value }",
        );
        assert!(
            d.iter().any(|x| x.code == "E732"),
            "expected E732 dynamic dispatch, got {:?}",
            d
        );
        // A statically-named, alphabet-matching inner fsm is accepted.
        let inner = parse_fsm_block(b"@@fsm Digits(text: bytes) : int = 0 { /[0-9]+/ 0 }")
            .expect("inner parses");
        let outer =
            parse_fsm_block(b"@@fsm M(text: bytes) : int = 0 { $m: /@Digits/  $m.0.return_value }")
                .expect("outer parses");
        let module = vec![inner, outer.clone()];
        assert!(
            !validate_fsm_in_module(&outer, &module)
                .iter()
                .any(|x| x.code == "E731" || x.code == "E732"),
            "a statically-named matching-alphabet Mode C ref must be accepted"
        );
    }

    /// FSM-TEST-307 — Unicode general-category classes (`\p{...}`) are gated by
    /// the `@@[allow(unicode_classes)]` opt-in (RFC-0042 §6.7/§11.6). Without it
    /// a `\p{L}` on the `char` alphabet is rejected E720; with it, accepted. On
    /// a non-`char` alphabet it is E722 (no codepoint notion) regardless.
    #[test]
    fn e720_unicode_class_requires_optin() {
        let d = diags(b"@@fsm M(text: char) : bool = false { /\\p{L}+/ true }");
        assert!(
            d.iter().any(|x| x.code == "E720"),
            "expected E720 without the opt-in, got {:?}",
            d
        );
        let ok = diags(
            b"@@[allow(unicode_classes)]\n@@fsm M(text: char) : bool = false { /\\p{L}+/ true }",
        );
        assert!(
            !ok.iter().any(|x| x.code == "E720"),
            "the opt-in must admit \\p{{...}}, got {:?}",
            ok
        );
        let bytes = diags(
            b"@@[allow(unicode_classes)]\n@@fsm M(text: bytes) : bool = false { /\\p{L}+/ true }",
        );
        assert!(
            bytes.iter().any(|x| x.code == "E722"),
            "Unicode class on bytes must be E722, got {:?}",
            bytes
        );
    }

    /// FSM-TEST-403 — reference to undeclared state.
    /// E731 — a transition target naming an undeclared state.
    #[test]
    fn e731_undeclared_state() {
        let d =
            diags(b"@@fsm M(text: bytes) : bool = false { /a/ -> $nope : -> $err  $err: false }");
        assert!(d.iter().any(|x| x.code == "E731"));
    }

    /// FSM-TEST-404 — reference to undeclared stage.
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

    /// FSM-TEST-1102 — stage label collision within a state.
    /// E730 — a stage label used twice within one state.
    #[test]
    fn e730_duplicate_stage_label() {
        let d = diags(b"@@fsm M(text: bytes) : bytes = \"\" { $s: .x/[0-9]+/ .x/[a-z]+/ $s.x }");
        assert!(d.iter().any(|x| x.code == "E730"), "got {:?}", d);
    }

    /// FSM-TEST-013 — two consecutive unlabeled states rejected.
    /// E704 — a second, unlabeled state.
    #[test]
    fn e704_second_unlabeled_state() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ -> $x  /b/ true  $x: false }");
        assert!(d.iter().any(|x| x.code == "E704"), "got {:?}", d);
    }

    /// FSM-TEST-012 — type mismatch in explicit domain redeclaration.
    /// E707 — a domain field re-declaring a parameter with a different type.
    #[test]
    fn e707_domain_param_type_mismatch() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ true  domain: text: int = 0 }");
        assert!(d.iter().any(|x| x.code == "E707"), "got {:?}", d);
    }

    /// FSM-TEST-008 — a stage-capture reference (`$0.x`) read in value
    /// position against an *unlabeled* enclosing state is E704: captures of an
    /// unlabeled state cannot be addressed; the state must be given a label.
    #[test]
    fn e704_capture_ref_unlabeled_state() {
        let d = diags(b"@@fsm M(text: bytes) : bytes = \"\" { .x/[0-9]+/ $0.x }");
        assert!(d.iter().any(|x| x.code == "E704"), "got {:?}", d);
    }

    /// E731 — a value-position capture naming a state that is not a declared
    /// label (from a labeled enclosing state) is an undeclared-state ref,
    /// matching the transition-target convention.
    #[test]
    fn e731_capture_ref_undeclared_state() {
        let d = diags(b"@@fsm M(text: bytes) : bytes = \"\" { $s: .x/[0-9]+/ $undecl.x }");
        assert!(d.iter().any(|x| x.code == "E731"), "got {:?}", d);
    }

    /// E732 — a value-position capture naming a real state but a non-existent
    /// (non-numeric) stage label.
    #[test]
    fn e732_capture_ref_undeclared_stage() {
        let d = diags(b"@@fsm M(text: bytes) : bytes = \"\" { $s: .x/[0-9]+/ $s.nope }");
        assert!(d.iter().any(|x| x.code == "E732"), "got {:?}", d);
    }

    /// A valid value-position capture (`$s.x`, declared state + stage) and a
    /// positional capture (`$s.0`, §3.5.2) produce no capture diagnostics —
    /// the pass must not false-positive on legitimate references.
    #[test]
    fn capture_ref_valid_no_false_positive() {
        let ok = diags(b"@@fsm M(text: bytes) : bytes = \"\" { $s: .x/[0-9]+/ $s.x }");
        assert!(
            !ok.iter()
                .any(|x| x.code == "E704" || x.code == "E731" || x.code == "E732"),
            "labeled capture must be accepted, got {:?}",
            ok
        );
        let pos = diags(b"@@fsm M(text: bytes) : bytes = \"\" { $s: /[0-9]+/ $s.0 }");
        assert!(
            !pos.iter().any(|x| x.code == "E732"),
            "positional capture `$s.0` must be accepted, got {:?}",
            pos
        );
    }

    /// A domain field re-declaring a parameter with the SAME type is fine.
    #[test]
    fn domain_param_same_type_ok() {
        let d = diags(
            b"@@fsm M(text: bytes, n: int = 0) : int = 0 { /[0-9]/ self.n  domain: n: int = 5 }",
        );
        assert!(!d.iter().any(|x| x.code == "E707"), "got {:?}", d);
    }

    /// FSM-TEST-1103 — unused parameter warning.
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

    /// FSM-TEST-1104 — unused domain variable warning.
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

    /// FSM-TEST-407 — constant-true `when` guard warns.
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

    /// FSM-TEST-405 — conditional with no matching condition warns.
    /// W701 — a conditional success target with no failure branch can
    /// silently reject unmatched input.
    #[test]
    fn w701_conditional_without_failure() {
        let d = diags(
            b"@@fsm M(text: bytes, mode: int) : int = 0 { /[01]/ -> ( $a when self.mode == 0 )  $a: 1 }",
        );
        assert!(d.iter().any(|x| x.code == "W701"), "got {:?}", d);
    }

    /// A conditional success target WITH a failure branch is not flagged W701.
    #[test]
    fn w701_conditional_with_failure_ok() {
        let d = diags(
            b"@@fsm M(text: bytes, mode: int) : int = 0 { /[01]/ -> ( $a when self.mode == 0 ) : -> $err  $a: 1  $err: -1 }",
        );
        assert!(!d.iter().any(|x| x.code == "W701"), "got {:?}", d);
    }

    /// FSM-TEST-103 — a domain field initializer may reference a constructor
    /// parameter by bare name (`initial`); parameters are in scope inside
    /// initializers. Compiles with no diagnostics.
    #[test]
    fn fsm_test_103_domain_init_references_param() {
        let d = diags(
            b"@@fsm M(text: bytes, initial: int = 0) : int = 0 { /[0-9]/ { self.count = self.count + 1 } self.count  domain: count: int = initial }",
        );
        assert!(
            !d.iter().any(|x| x.code.starts_with('E')),
            "param-in-initializer must compile clean, got {:?}",
            d
        );
    }

    /// FSM-TEST-100 — undeclared variable read. Reading a `self.<field>` that
    /// names no parameter or domain field is E703.
    #[test]
    fn e703_undeclared_read() {
        let d = diags(b"@@fsm M(text: bytes) : int = 0 { /a/ { self.count = self.nope } self.count  domain: count: int = 0 }");
        assert!(d.iter().any(|x| x.code == "E703"), "got {:?}", d);
    }

    /// FSM-TEST-101 — undeclared variable write. Writing to a `self.<field>`
    /// that names no parameter or domain field is E704 (the write side of
    /// FSM-TEST-100), distinct from the E703 read code.
    #[test]
    fn e704_undeclared_write() {
        let d = diags(
            b"@@fsm M(text: bytes) : int = 0 { /a/ { self.nope = 5 } 0  domain: ok: int = 0 }",
        );
        assert!(
            d.iter().any(|x| x.code == "E704"),
            "undeclared write must be E704, got {:?}",
            d
        );
        assert!(
            !d.iter().any(|x| x.code == "E703"),
            "an undeclared write must not also report the E703 read code, got {:?}",
            d
        );
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

    // --- $CheckRegex: per-stage regex compilation + E701 (Task 21) ---

    /// A forbidden regex construct (lazy quantifier) surfaces E720 from the
    /// engine, located at the stage.
    #[test]
    fn regex_forbidden_is_e720() {
        // Lookahead is non-regular for v0.1 → E720 (lazy quantifiers, by
        // contrast, now compile via the Pike VM — see the codegen lazy tests).
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a(?=b)/ true }");
        assert!(d.iter().any(|x| x.code == "E720"), "got {:?}", d);
    }

    /// A malformed regex (unbalanced group) surfaces E722.
    #[test]
    fn regex_malformed_is_e722() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /(a/ true }");
        assert!(d.iter().any(|x| x.code == "E722"), "got {:?}", d);
    }

    /// A boundary anchor (`^a`) is supported and produces no diagnostic.
    #[test]
    fn regex_boundary_anchor_ok() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /^a/ true }");
        assert!(!d.iter().any(|x| x.code == "E722"), "got {:?}", d);
    }

    /// A mid-pattern anchor (`a$b`) is now supported via the Pike VM's
    /// zero-width assertions — it validates cleanly (no E722), the engine
    /// compiles it to an assertion-bearing program.
    #[test]
    fn regex_mid_anchor_is_supported() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a$b/ true }");
        assert!(
            !d.iter().any(|x| x.code == "E722"),
            "interior anchors no longer deferred: {:?}",
            d
        );
    }

    /// FSM-TEST-200 — failable match without a failure_branch rejected.
    /// E701 — a success transition with no failure branch on a match whose
    /// regex can fail (`/a/` does not accept the empty string).
    #[test]
    fn e701_fallible_match_without_failure_branch() {
        let d = diags(b"@@fsm M(text: bytes) : int = 0 { /a/ -> $x  $x: 1 }");
        assert!(d.iter().any(|x| x.code == "E701"), "got {:?}", d);
    }

    /// FSM-TEST-201 — unfailable match without a failure_branch allowed.
    /// No E701 when the match is provably non-failing: a nullable regex
    /// (`a*` accepts the empty string) can never fail.
    #[test]
    fn e701_not_fired_for_nullable_match() {
        let d = diags(b"@@fsm M(text: bytes) : int = 0 { /a*/ -> $x  $x: 1 }");
        assert!(!d.iter().any(|x| x.code == "E701"), "got {:?}", d);
    }

    /// No E701 for a multi-match (`|`) alternative whose only fallible
    /// stage is the ordered-choice selector — a first-stage miss falls
    /// through to the next alternative, not §5.6.
    #[test]
    fn e701_not_fired_multi_match_selector() {
        let d = diags(
            b"@@fsm M(text: bytes) : int = 0 { /[0-9]/ -> $a | /[a-z]/ -> $b  $a: 1  $b: 2 }",
        );
        assert!(!d.iter().any(|x| x.code == "E701"), "got {:?}", d);
    }

    /// E701 still fires for a multi-match alternative whose *later* stage is
    /// fallible and has no failure branch (that failure reaches §5.6).
    #[test]
    fn e701_fired_multi_match_later_stage() {
        let d =
            diags(b"@@fsm M(text: bytes) : int = 0 { /a/ /b/ -> $x | /c/ -> $y  $x: 1  $y: 2 }");
        assert!(d.iter().any(|x| x.code == "E701"), "got {:?}", d);
    }

    /// No E701 when a failure branch is present, even for a fallible regex.
    #[test]
    fn e701_not_fired_with_failure_branch() {
        let d = diags(b"@@fsm M(text: bytes) : int = 0 { /a/ -> $x : -> $e  $x: 1  $e: -1 }");
        assert!(!d.iter().any(|x| x.code == "E701"), "got {:?}", d);
    }

    /// No E701 for an implicit-terminal match (no transition at all), even
    /// though `/a/` can fail — failure follows §5.6 instead.
    #[test]
    fn e701_not_fired_for_implicit_terminal() {
        let d = diags(b"@@fsm M(text: bytes) : bool = false { /a/ true }");
        assert!(!d.iter().any(|x| x.code == "E701"), "got {:?}", d);
    }

    /// A clean fsm with a real regex and a proper failure branch produces
    /// no regex/E701 diagnostics.
    #[test]
    fn regex_clean_fsm_no_diagnostics() {
        let d = diags(b"@@fsm M(text: bytes) : int = 0 { /[0-9]+/ -> $n : -> $e  $n: 1  $e: -1 }");
        assert!(d.is_empty(), "expected none, got {:?}", d);
    }
}
