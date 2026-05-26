//! Frame-segment metadata extraction.
//!
//! After the scanner detects a Frame segment by its sigil
//! (`-> $State`, `@@:return = expr`, `$.varName`, etc.), this
//! function parses the raw segment text into a structured
//! `SegmentMetadata` variant the downstream stages (codegen,
//! validator, assembler) consume directly — no re-parsing of
//! raw text downstream.

use super::super::{FrameSegmentKind, SegmentMetadata};

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
                let key: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                SegmentMetadata::ContextParams { key }
            } else {
                SegmentMetadata::None
            }
        }

        FrameSegmentKind::ContextData => {
            // @@:data.key → extract key
            if let Some(rest) = text.strip_prefix("@@:data.") {
                let key: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
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
                let key: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
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
            // @@:(expr) → extract the expression between parens
            let trimmed = text.trim();
            if let Some(start) = trimmed.find("@@:(") {
                let after_open = start + 4;
                let bytes = trimmed.as_bytes();
                let mut depth = 1i32;
                let mut p = after_open;
                while p < bytes.len() && depth > 0 {
                    match bytes[p] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        p += 1;
                    }
                }
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

        FrameSegmentKind::ContextSelf
        | FrameSegmentKind::ContextSystemState
        | FrameSegmentKind::ContextSystemBare
        | FrameSegmentKind::ContextEvent => {
            // These carry no variable content — the kind is sufficient
            SegmentMetadata::None
        }

        // --- State variables ---
        FrameSegmentKind::StateVar | FrameSegmentKind::StateVarAssign => {
            // $.varName or $.varName = expr → extract name
            if let Some(rest) = text.strip_prefix("$.") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
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
            // Detect push-with-transition: `push$ -> $State`
            let transition_target = if let Some(arrow_pos) = text.find("->") {
                let after_arrow = &text[arrow_pos + 2..];
                let bytes = after_arrow.as_bytes();
                let mut target_start = None;
                for i in 0..bytes.len() {
                    if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_uppercase()
                    {
                        target_start = Some(i + 1);
                    }
                }
                target_start.map(|start| {
                    let after_dollar = &after_arrow[start..];
                    let end = after_dollar
                        .find(|c: char| !c.is_alphanumeric() && c != '_')
                        .unwrap_or(after_dollar.len());
                    after_dollar[..end].to_string()
                })
            } else {
                None
            };
            SegmentMetadata::StackPush { transition_target }
        }

        // --- Others ---
        FrameSegmentKind::Forward
        | FrameSegmentKind::StackPop
        | FrameSegmentKind::ReturnStatement => SegmentMetadata::None,
    }
}
