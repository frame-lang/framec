//! Frame-segment metadata extraction.
//!
//! After the scanner detects a Frame segment by its sigil
//! (`-> $State`, `@@:return = expr`, `$.varName`, etc.), this
//! function parses the raw segment text into a structured
//! `SegmentMetadata` variant the downstream stages (codegen,
//! validator, assembler) consume directly — no re-parsing of
//! raw text downstream.

use super::super::{FrameSegmentKind, SegmentMetadata};

/// Identifier extent at the start of `rest`, via the dogfooded
/// `ident_scan.frs` recognizer (issue #154 — one identifier automaton, not
/// per-site `take_while` walks). Returns the identifier slice, or `""` when
/// `rest` doesn't start with `[A-Za-z_]` (stricter than the old
/// `take_while(alnum|_)`, which accepted a leading digit — such a key was
/// invalid in every target anyway).
fn leading_ident(rest: &str) -> &str {
    match crate::frame_c::compiler::ident_scan_fsm::scan(rest.as_bytes()) {
        Some((_, end)) => &rest[..end],
        None => "",
    }
}

/// Matching depth-0 `)` for an expression starting at `from` (the byte after
/// the opening paren), via the dogfooded `ExprScannerFsm` (`)`-terminator
/// flag). String-aware — a `)` inside a string literal does not terminate,
/// which the old hand-rolled depth counter got wrong (`@@:(f(")"))`).
fn matching_close_paren(text: &str, from: usize) -> usize {
    let mut fsm = super::_expr_scanner::ExprScannerFsm::new();
    fsm.bytes = text.as_bytes().to_vec();
    fsm.pos = from;
    fsm.end = text.len();
    fsm.stop_semicolon = false;
    fsm.stop_newline = false;
    fsm.stop_close_paren = true;
    fsm.do_scan();
    fsm.result_end
}

/// Extract structured metadata from a Frame segment's raw text.
///
/// This is the scanner's parsing phase — it produces structured data that
/// downstream stages (codegen, validator, assembler) consume directly,
/// eliminating the need for re-parsing raw segment text.
pub(super) fn extract_segment_metadata(kind: FrameSegmentKind, text: &str) -> SegmentMetadata {
    match kind {
        // --- Context accessors ---
        FrameSegmentKind::ContextParams => {
            // @@:params.key → extract key
            if let Some(rest) = text.strip_prefix("@@:params.") {
                let key = leading_ident(rest).to_string();
                SegmentMetadata::ContextParams { key }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextData => {
            // @@:data.key → extract key
            if let Some(rest) = text.strip_prefix("@@:data.") {
                let key = leading_ident(rest).to_string();
                SegmentMetadata::ContextData {
                    key,
                    assign_expr: None,
                }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextDataAssign => {
            // @@:data.key = expr → extract key and expr
            if let Some(rest) = text.strip_prefix("@@:data.") {
                let key = leading_ident(rest).to_string();
                let after_key = &rest[key.len()..];
                let expr = after_key
                    .trim()
                    .strip_prefix('=')
                    .map(|e| e.trim().trim_end_matches(';').trim().to_string());
                SegmentMetadata::ContextData {
                    key,
                    assign_expr: expr,
                }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextReturn => {
            // @@:return = expr (assignment) or @@:return (bare read)
            let trimmed = text.trim();
            if let Some(rest) = trimmed.strip_prefix("@@:return") {
                let rest = rest.trim();
                if rest.starts_with('=') && !rest.starts_with("==") {
                    let expr = rest[1..].trim().trim_end_matches(';').trim().to_string();
                    SegmentMetadata::ContextReturn {
                        assign_expr: Some(expr),
                    }
                } else {
                    SegmentMetadata::ContextReturn { assign_expr: None }
                }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextReturnExpr => {
            // @@:(expr) → extract the expression between parens. The
            // depth-0 close is found by the dogfooded `ExprScannerFsm`.
            let trimmed = text.trim();
            if let Some(start) = trimmed.find("@@:(") {
                let after_open = start + 4;
                let p = matching_close_paren(trimmed, after_open);
                let expr = trimmed[after_open..p].to_string();
                SegmentMetadata::ReturnExpr { expr }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ReturnCall => {
            // @@:return(expr) → extract expr.
            //
            // Strip exactly ONE trailing `)` — the closing paren of
            // `@@:return(...)`. trim_end_matches(')') is greedy and
            // would also strip closing parens from a nested call
            // expression like `@@:return(self.f(1, 2))`, producing
            // `self.f(1, 2` and breaking codegen.
            let trimmed = text.trim();
            if let Some(rest) = trimmed.strip_prefix("@@:return(") {
                let inner = rest.trim_end();
                let expr = inner.strip_suffix(')').unwrap_or(inner).to_string();
                SegmentMetadata::ReturnCall { expr }
            } else {
                SegmentMetadata::None
            }
        }

        // --- Self and system ---
        FrameSegmentKind::ContextSelfCall => {
            // @@:self.method(args) → extract method and args
            if let Some(rest) = text.strip_prefix("@@:self.") {
                if let Some(paren) = rest.find('(') {
                    let method = rest[..paren].to_string();
                    let args = rest[paren..].to_string(); // includes parens
                    SegmentMetadata::SelfCall { method, args }
                } else {
                    SegmentMetadata::None
                }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextSelfFieldCall => {
            // `@@:self.field.method(args)` (RFC-0046) → field, method, args.
            if let Some(rest) = text.strip_prefix("@@:self.") {
                if let Some(dot) = rest.find('.') {
                    let field = rest[..dot].to_string();
                    let after = &rest[dot + 1..];
                    if let Some(paren) = after.find('(') {
                        let method = after[..paren].to_string();
                        let args = after[paren..].to_string(); // includes parens
                        return SegmentMetadata::SelfFieldCall {
                            field,
                            method,
                            args,
                        };
                    }
                }
            }
            SegmentMetadata::None
        }

        FrameSegmentKind::ContextSelf => {
            // `@@:self.field` (RFC-0046) → SelfField; bare `@@:self` → None.
            // The parser segments both as ContextSelf; the trailing member
            // (if any, and not a call — calls are ContextSelfCall) tells the
            // two apart. Mirrors the `@@:params.key` extraction above.
            if let Some(rest) = text.strip_prefix("@@:self.") {
                let field = leading_ident(rest).to_string();
                if field.is_empty() {
                    SegmentMetadata::None
                } else {
                    SegmentMetadata::SelfField { field }
                }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextSystemState
        | FrameSegmentKind::ContextSystemBare
        | FrameSegmentKind::ContextSystemStateReserved
        | FrameSegmentKind::ContextEvent => {
            // These carry no variable content — the kind is sufficient
            SegmentMetadata::None
        }

        // --- State variables ---
        FrameSegmentKind::StateVar | FrameSegmentKind::StateVarAssign => {
            // $.varName or $.varName = expr → extract name. Delegates the
            // `$.` grammar to the dogfooded `StateVarParserFsm` (the same
            // machine the scanner used to classify this segment), rather
            // than re-walking the identifier by hand (#154).
            if text.starts_with("$.") {
                let mut parser = super::StateVarParserFsm::new();
                parser.bytes = text.as_bytes().to_vec();
                parser.pos = 0;
                parser.end = text.len();
                parser.do_parse();
                let name = text[2..parser.ident_end].to_string();
                SegmentMetadata::StateVar {
                    name,
                    interp_quote: None,
                }
            } else {
                SegmentMetadata::None
            }
        }

        // --- Transitions ---
        FrameSegmentKind::Transition => {
            // RFC-0035 Round 11: the transition-string grammar parse
            // ((exit)? -> (=>)? (enter)? ($State(args)? | pop$) "label"?)
            // is a Frame FSM (compiler/transition_meta_scanner/).
            crate::frame_c::compiler::transition_meta_scanner::parse_transition_meta(text.trim())
        }
        // --- System instantiation ---
        FrameSegmentKind::SystemInstantiation => {
            // @@SystemName(args) → Factory
            // @@!SystemName()    → NoInitialization (RFC-0015 D7)
            use crate::frame_c::compiler::frame_ast::InstantiationKind;
            if let Some(rest) = text.strip_prefix("@@") {
                let (rest, kind) = match rest.strip_prefix('!') {
                    Some(stripped) => (stripped, InstantiationKind::NoInitialization),
                    None => (rest, InstantiationKind::Factory),
                };
                if let Some(paren) = rest.find('(') {
                    let system_name = rest[..paren].to_string();
                    let args = rest[paren..].to_string();
                    SegmentMetadata::SystemInstantiation {
                        system_name,
                        args,
                        kind,
                    }
                } else {
                    SegmentMetadata::None
                }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::StackPush => {
            // Detect push-with-transition: `push$ -> $State`. The `-> …`
            // suffix is transition grammar, so it is parsed by the same
            // dogfooded `TransitionMetaScannerFsm` the Transition kind uses
            // (#154 — no separate hand-rolled `$Target` walk).
            let transition_target = text.find("->").and_then(|arrow_pos| {
                match crate::frame_c::compiler::transition_meta_scanner::parse_transition_meta(
                    text[arrow_pos..].trim(),
                ) {
                    SegmentMetadata::Transition { target_state, .. }
                        if !target_state.is_empty() =>
                    {
                        Some(target_state)
                    }
                    _ => None,
                }
            });
            SegmentMetadata::StackPush { transition_target }
        }

        // --- Others ---
        FrameSegmentKind::Forward
        | FrameSegmentKind::StackPop
        | FrameSegmentKind::ReturnStatement => SegmentMetadata::None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{FrameSegmentKind, SegmentMetadata};
    use super::extract_segment_metadata;

    /// #154 — ident extraction is the `ident_scan.frs` automaton.
    #[test]
    fn ident_leaves_via_fsm() {
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::ContextParams, "@@:params.key_1 extra"),
            SegmentMetadata::ContextParams {
                key: "key_1".into()
            }
        );
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::ContextSelf, "@@:self.field9"),
            SegmentMetadata::SelfField {
                field: "field9".into()
            }
        );
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::StateVar, "$.hp = 3"),
            SegmentMetadata::StateVar {
                name: "hp".into(),
                interp_quote: None
            }
        );
    }

    /// #154 — `@@:(expr)` close-paren is the string-aware `ExprScannerFsm`:
    /// a `)` inside a string literal must NOT terminate (the old depth
    /// counter mis-sliced `@@:(f(")")))`).
    #[test]
    fn return_expr_paren_is_string_aware() {
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::ContextReturnExpr, r#"@@:(f(")"))"#),
            SegmentMetadata::ReturnExpr {
                expr: r#"f(")")"#.into()
            }
        );
        // Plain nesting still exact.
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::ContextReturnExpr, "@@:(a + (b * c))"),
            SegmentMetadata::ReturnExpr {
                expr: "a + (b * c)".into()
            }
        );
    }

    /// #154 — `push$ -> $State` target parses via the transition-grammar FSM.
    #[test]
    fn stack_push_target_via_transition_fsm() {
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::StackPush, "push$ -> $Working"),
            SegmentMetadata::StackPush {
                transition_target: Some("Working".into())
            }
        );
        assert_eq!(
            extract_segment_metadata(FrameSegmentKind::StackPush, "push$"),
            SegmentMetadata::StackPush {
                transition_target: None
            }
        );
    }
}
