//! Frame statement expansion and handler body splicing.
//!
//! This module handles the core Frame-to-native-code transformation:
//! - Splicing handler bodies: scanning for Frame statements in native code
//!   and replacing them with target-language expansions
//! - Frame statement expansion: converting -> $State, => $^, push$, pop$,
//!   return sugar, $.var, @@:return, etc. to target language code
//! - Helper functions for extracting transition targets, args, state vars

mod context_data;
mod context_return;
mod context_self;
mod expression;
mod forward;
mod handler_body;
mod no_init;
mod pop_transition;
mod return_stmt;
mod scanner_dispatch;
mod self_call_guard;
mod stack;
mod state_var;
mod transition;
mod utility;

use expression::expand_expression;
use handler_body::context_return_read_typed;
use pop_transition::generate_pop_transition;
use utility::{
    c_return_assign, cpp_wrap_string_literal, is_ident_char, paren_wrap_if_multiline,
    strip_outer_parens,
};

pub(crate) use handler_body::{
    emit_handler_body_via_statements, expand_self_in_body, resolve_state_arg_key,
};
pub(crate) use no_init::generate_no_initialization;
pub(crate) use scanner_dispatch::{
    expand_system_state, expand_system_state_in_code, get_native_scanner,
};
pub(crate) use self_call_guard::generate_self_call_guard;
pub(crate) use utility::{
    extract_dot_key, extract_state_var_name, normalize_indentation, php_prefix_params,
    strip_java_unreachable,
};

use super::codegen_utils::{
    cpp_map_type, cpp_wrap_any_arg, csharp_map_type, expression_to_string, go_map_type,
    java_map_type, kotlin_map_type, replace_outside_strings_and_comments, swift_map_type,
    to_snake_case, type_to_cpp_string, HandlerContext,
};
use crate::frame_c::compiler::frame_ast::Type;
use crate::frame_c::compiler::native_region_scanner::{
    c::NativeRegionScannerC, cpp::NativeRegionScannerCpp, csharp::NativeRegionScannerCs,
    dart::NativeRegionScannerDart, gdscript::NativeRegionScannerGDScript,
    go::NativeRegionScannerGo, java::NativeRegionScannerJava, javascript::NativeRegionScannerJs,
    kotlin::NativeRegionScannerKotlin, lua::NativeRegionScannerLua, php::NativeRegionScannerPhp,
    python::NativeRegionScannerPy, ruby::NativeRegionScannerRuby, rust::NativeRegionScannerRust,
    swift::NativeRegionScannerSwift, typescript::NativeRegionScannerTs, FrameSegmentKind,
    NativeRegionScanner, Region, SegmentMetadata,
};
use crate::frame_c::compiler::splice::Splicer;
use crate::frame_c::visitors::TargetLanguage;

/// Generate code expansion for a Frame segment
///
/// NOTE: The scanner leaves a gap between NativeText and FrameSegment where leading
/// whitespace lives. Since the splicer doesn't copy this gap, we MUST include the
/// indentation in the expansion to preserve proper code structure.
pub(crate) fn generate_frame_expansion(
    body_bytes: &[u8],
    span: &crate::frame_c::compiler::native_region_scanner::RegionSpan,
    kind: FrameSegmentKind,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> String {
    let segment_text = String::from_utf8_lossy(&body_bytes[span.start..span.end]);
    let indent_str = " ".repeat(indent);

    match kind {
        FrameSegmentKind::Transition
        | FrameSegmentKind::Forward
        | FrameSegmentKind::StackPush
        | FrameSegmentKind::StackPop => {
            // Control-flow segments build their body and terminator separately
            // (#169); re-join them here for the plain-`String` expansion API.
            let (body, terminator) = generate_frame_transition_parts(
                body_bytes, span, kind, indent, lang, ctx, metadata,
            );
            join_transition_terminator(body, terminator, &indent_str)
        }
        FrameSegmentKind::StateVar => {
            state_var::expand_state_var(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::StateVarAssign => {
            state_var::expand_state_var_assign(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextReturn => {
            context_return::expand_context_return(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextReturnExpr => context_return::expand_context_return_expr(
            body_bytes, span, indent, lang, ctx, metadata,
        ),
        FrameSegmentKind::ContextEvent => {
            context_data::expand_context_event(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextData => {
            context_data::expand_context_data(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextDataAssign => {
            context_data::expand_context_data_assign(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextParams => {
            context_data::expand_context_params(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::SystemInstantiation => {
            return_stmt::expand_system_instantiation(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ReturnCall => {
            return_stmt::expand_return_call(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextSystemBare => {
            context_data::expand_context_system_bare(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextSystemState => expand_system_state(lang),
        // Reserved (RFC-0045) — validation rejects bare `@@:system.state` with
        // E608 before codegen; this fallback only fires if validation is
        // bypassed, in which case it passes the text through like a bare member.
        FrameSegmentKind::ContextSystemStateReserved => {
            context_data::expand_context_system_bare(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextSelf => {
            context_self::expand_context_self(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextSelfCall => {
            context_self::expand_context_self_call(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::ContextSelfFieldCall => context_self::expand_context_self_field_call(
            body_bytes, span, indent, lang, ctx, metadata,
        ),
        FrameSegmentKind::ReturnStatement => {
            return_stmt::expand_return_statement(body_bytes, span, indent, lang, ctx, metadata)
        }
    }
}

/// Expand a control-flow segment (`->`/`=>`/`push$`/`pop$`) into its
/// `(body, terminator)` parts.
///
/// The body is the state-change code WITHOUT the trailing handler-exit
/// `return`; the terminator is the language's `return`/`return;`/`""` (see
/// [`transition_terminator`](utility::transition_terminator)). The handler
/// orchestrator calls this directly so it can hoist a same-scope `@@:(expr)`
/// between the body and the terminator — replacing the old
/// `split_transition_return` text oracle that recovered the split by
/// suffix-probing the emitter's own output (#123/#169).
///
/// Only the four control-flow kinds are valid here (the orchestrator gates on
/// them); any other kind is a caller bug.
pub(crate) fn generate_frame_transition_parts(
    body_bytes: &[u8],
    span: &crate::frame_c::compiler::native_region_scanner::RegionSpan,
    kind: FrameSegmentKind,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> (String, &'static str) {
    match kind {
        FrameSegmentKind::Transition => {
            transition::expand_transition(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::Forward => {
            forward::expand_forward(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::StackPush => {
            stack::expand_stack_push(body_bytes, span, indent, lang, ctx, metadata)
        }
        FrameSegmentKind::StackPop => {
            stack::expand_stack_pop(body_bytes, span, indent, lang, ctx, metadata)
        }
        other => unreachable!(
            "generate_frame_transition_parts called for non-control-flow kind {other:?}"
        ),
    }
}

/// Re-join a `(body, terminator)` control-flow expansion into a single string,
/// the terminator on its own line at the body's indent. Reproduces the
/// pre-#169 baked-in output byte-for-byte. `body.trim_end()` mirrors the
/// trailing-whitespace trim the old `split_transition_return` applied before
/// re-emitting the return.
fn join_transition_terminator(body: String, terminator: &str, indent_str: &str) -> String {
    if terminator.is_empty() {
        body
    } else {
        format!("{}\n{}{}", body.trim_end(), indent_str, terminator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_c::compiler::native_region_scanner::FrameSegmentKind;
    use crate::frame_c::visitors::TargetLanguage;

    fn make_ctx(state_var_types: Vec<(&str, &str)>) -> HandlerContext {
        HandlerContext {
            system_is_async: false,
            system_name: "TestSys".to_string(),
            state_name: "S1".to_string(),
            event_name: "foo".to_string(),
            parent_state: None,
            defined_systems: std::collections::HashSet::new(),
            use_sv_comp: false,
            per_handler: false,
            state_var_types: state_var_types
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            state_param_names: std::collections::HashMap::new(),
            state_enter_param_names: std::collections::HashMap::new(),
            state_exit_param_names: std::collections::HashMap::new(),
            event_param_names: std::collections::HashMap::new(),
            state_hsm_parents: std::collections::HashMap::new(),
            current_return_type: None,
            state_param_types: std::collections::HashMap::new(),
            state_enter_param_types: std::collections::HashMap::new(),
            state_exit_param_types: std::collections::HashMap::new(),
            domain_field_types: std::collections::HashMap::new(),
            actions: std::collections::HashSet::new(),
        }
    }

    /// Helper: call generate_frame_expansion with text as bytes + span
    fn expand(
        kind: FrameSegmentKind,
        text: &str,
        lang: TargetLanguage,
        ctx: &HandlerContext,
    ) -> String {
        let bytes = text.as_bytes();
        let span = crate::frame_c::compiler::native_region_scanner::RegionSpan {
            start: 0,
            end: bytes.len(),
        };
        generate_frame_expansion(bytes, &span, kind, 0, lang, ctx, &SegmentMetadata::None)
    }

    // =========================================================
    // Rust @@:(expr) — string literals wrapped with String::from
    // =========================================================

    #[test]
    fn test_context_return_expr_rust_string_wraps() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturnExpr,
            "@@:(\"green\")",
            TargetLanguage::Rust,
            &ctx,
        );
        assert!(
            result.contains("String::from(\"green\")"),
            "Rust @@:(\"green\") should wrap with String::from, got: {}",
            result
        );
    }

    #[test]
    fn test_context_return_expr_rust_int_no_wrap() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturnExpr,
            "@@:(42)",
            TargetLanguage::Rust,
            &ctx,
        );
        // RFC-0025 Track B.2: `@@:(42)` emits the typed
        // `<System>FrameReturn::<Variant>(42)` constructor — no
        // `Box::new` and no `String::from` wrapping.
        assert!(
            result.contains("FrameReturn::"),
            "Rust @@:(42) should construct the typed return variant, got: {}",
            result
        );
        assert!(
            result.contains("(42)"),
            "Integer literal should pass through unwrapped, got: {}",
            result
        );
        assert!(
            !result.contains("String::from"),
            "Integer should not get String::from wrapping, got: {}",
            result
        );
    }

    #[test]
    fn test_context_return_expr_python_no_wrap() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturnExpr,
            "@@:(\"green\")",
            TargetLanguage::Python3,
            &ctx,
        );
        assert!(
            !result.contains("String::from"),
            "Python should NOT wrap string literals, got: {}",
            result
        );
        assert!(
            result.contains("\"green\""),
            "Python should pass through the literal, got: {}",
            result
        );
    }

    // =========================================================
    // Rust @@:return = expr — same wrapping
    // =========================================================

    #[test]
    fn test_context_return_assign_rust_string_wraps() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturn,
            "@@:return = \"hello\"",
            TargetLanguage::Rust,
            &ctx,
        );
        assert!(
            result.contains("String::from(\"hello\")"),
            "Rust @@:return = \"hello\" should wrap, got: {}",
            result
        );
    }

    #[test]
    fn test_context_return_assign_rust_int_no_wrap() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturn,
            "@@:return = 42",
            TargetLanguage::Rust,
            &ctx,
        );
        assert!(
            !result.contains("String::from"),
            "Rust @@:return = 42 should NOT wrap, got: {}",
            result
        );
    }

    // =========================================================
    // paren_wrap_if_multiline — multi-line @@:(expr) assignments
    // need re-wrapping in `(...)` so indent-sensitive targets
    // (Python, GDScript) parse the continuation lines as part of
    // the expression. Single-line expressions stay unwrapped.
    // =========================================================

    #[test]
    fn paren_wrap_if_multiline_singleline_unchanged() {
        assert_eq!(paren_wrap_if_multiline("self.x"), "self.x");
        assert_eq!(paren_wrap_if_multiline("a + b"), "a + b");
        assert_eq!(paren_wrap_if_multiline(""), "");
    }

    #[test]
    fn paren_wrap_if_multiline_wraps_multiline() {
        let inp = "self.timer >= self.threshold\n    and self.count < self.limit";
        let want = "(self.timer >= self.threshold\n    and self.count < self.limit)";
        assert_eq!(paren_wrap_if_multiline(inp), want);
    }

    #[test]
    fn context_return_expr_gdscript_multiline_wraps() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturnExpr,
            "@@:(self.timer >= self.threshold\n    and self.count < self.limit)",
            TargetLanguage::GDScript,
            &ctx,
        );
        assert!(
            result.contains(
                "_return = (self.timer >= self.threshold\n    and self.count < self.limit)"
            ),
            "GDScript multi-line @@:() should re-wrap in parens, got:\n{}",
            result
        );
    }

    #[test]
    fn context_return_expr_python_multiline_wraps() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturnExpr,
            "@@:(self.timer >= self.threshold\n    and self.count < self.limit)",
            TargetLanguage::Python3,
            &ctx,
        );
        assert!(
            result.contains(
                "_return = (self.timer >= self.threshold\n    and self.count < self.limit)"
            ),
            "Python multi-line @@:() should re-wrap in parens, got:\n{}",
            result
        );
    }

    #[test]
    fn context_return_expr_singleline_no_wrap() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ContextReturnExpr,
            "@@:(true)",
            TargetLanguage::GDScript,
            &ctx,
        );
        assert!(
            result.contains("_return = true"),
            "single-line @@:() should NOT add parens, got:\n{}",
            result
        );
        assert!(
            !result.contains("_return = (true)"),
            "single-line @@:() must not gain parens, got:\n{}",
            result
        );
    }

    // =========================================================
    // Rust state var READ — .clone() for non-Copy types only
    // =========================================================

    #[test]
    fn test_state_var_read_rust_string_clones() {
        let ctx = make_ctx(vec![("name", "String")]);
        let result = expand_expression("$.name", TargetLanguage::Rust, &ctx);
        assert!(
            result.contains(".clone()"),
            "String state var read should add .clone(), got: {}",
            result
        );
    }

    #[test]
    fn test_state_var_read_rust_int_no_clone() {
        let ctx = make_ctx(vec![("count", "i32")]);
        let result = expand_expression("$.count", TargetLanguage::Rust, &ctx);
        assert!(
            !result.contains(".clone()"),
            "i32 state var read should NOT add .clone(), got: {}",
            result
        );
    }

    #[test]
    fn test_state_var_read_rust_bool_no_clone() {
        let ctx = make_ctx(vec![("flag", "bool")]);
        let result = expand_expression("$.flag", TargetLanguage::Rust, &ctx);
        assert!(
            !result.contains(".clone()"),
            "bool state var read should NOT add .clone(), got: {}",
            result
        );
    }

    #[test]
    fn test_state_var_read_rust_unknown_type_clones() {
        let ctx = make_ctx(vec![]);
        let result = expand_expression("$.mystery", TargetLanguage::Rust, &ctx);
        assert!(
            result.contains(".clone()"),
            "Unknown-type state var should clone for safety, got: {}",
            result
        );
    }

    // =========================================================
    // #141 — void `@@:return()` in a handler: early exit with
    // NO write to the `_return` slot (no empty assignment leak).
    // =========================================================

    #[test]
    fn void_return_call_python_no_assignment() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ReturnCall,
            "@@:return()",
            TargetLanguage::Python3,
            &ctx,
        );
        assert_eq!(result, "return");
        assert!(
            !result.contains("_return ="),
            "void @@:return() must not emit an empty `_return =` assignment, got: {:?}",
            result
        );
    }

    #[test]
    fn void_return_call_typescript_no_assignment() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ReturnCall,
            "@@:return()",
            TargetLanguage::TypeScript,
            &ctx,
        );
        assert_eq!(result, "return;");
        assert!(
            !result.contains("_return ="),
            "void @@:return() must not emit an empty `_return =` assignment, got: {:?}",
            result
        );
    }

    #[test]
    fn void_return_call_ruby_no_assignment() {
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ReturnCall,
            "@@:return()",
            TargetLanguage::Ruby,
            &ctx,
        );
        assert_eq!(result, "return");
    }

    #[test]
    fn nonvoid_return_call_still_assigns() {
        // Guard: the value-carrying form keeps writing the slot + returns.
        let ctx = make_ctx(vec![]);
        let result = expand(
            FrameSegmentKind::ReturnCall,
            "@@:return(1)",
            TargetLanguage::Python3,
            &ctx,
        );
        assert!(
            result.contains("_return = 1") && result.contains("return"),
            "value @@:return(1) must set the slot and return, got: {:?}",
            result
        );
    }
}
