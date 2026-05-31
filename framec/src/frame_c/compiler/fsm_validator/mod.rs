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
    FsmDeclAst, FsmTransitionTarget, MatchElement, Span, Type,
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

    use super::{check_input_param_type, check_transition_targets, FsmDiagnostic};
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
}
