//! `-> $State` transition expansion across the 17 backends.
//!
//! This is the largest single Frame segment kind by code volume.
//! Each backend needs its own emission shape because the
//! transition mechanic — write enter_args / state_args, set the
//! new compartment, call `__transition`, return — uses different
//! container types, ownership flavors, and dispatch idioms per
//! target. The whole arm comprises:
//!
//! - **Pop short-circuit** (`-> pop$`) — delegates to
//!   `pop_transition::generate_pop_transition` (RFC-0008
//!   decorations included).
//! - **Regular + forward transition** — the bulk: per-target
//!   compartment construction, HSM ancestor chain walking
//!   (`__prepareEnter`-style for the dynamic backends; eager
//!   `parent_compartment` field threading for the static
//!   backends), state_args / enter_args positional writes, and
//!   the `__transition()` call. The forward variant
//!   (`-> => $State`, optionally decorated) runs through this SAME
//!   branch — sharing all exit/enter/state arg emission — and only
//!   additionally sets the new compartment's `forward_event` field
//!   (so the destination re-dispatches the in-flight event after
//!   enter). Keeping the two on one path means the arg channels
//!   can't drift apart again (#128).
//!
//! All language-specific emission stays here — no per-target
//! crate-level helpers escape from this arm beyond what
//! rust_system already publishes (the Rust transition has a
//! large dedicated module emitter; this arm just calls it).

use super::super::codegen_utils::{cpp_wrap_any_arg, to_snake_case, HandlerContext};
use super::expand_expression;
use super::generate_pop_transition;
use super::utility::php_prefix_params;
use crate::frame_c::compiler::native_region_scanner::{RegionSpan, SegmentMetadata};
use crate::frame_c::visitors::TargetLanguage;

/// Expand a `-> $State` transition (and its `push$`/`pop$`/forward variants)
/// into `(body, terminator)`. The body is the state-change code WITHOUT the
/// trailing handler-exit `return`; the terminator is
/// [`transition_terminator`](super::utility::transition_terminator) — the
/// language's `return`/`return;`/`""`. Keeping them separate lets the handler
/// orchestrator hoist a same-scope `@@:(expr)` between them (#123/#169), and
/// `generate_frame_expansion` re-joins them for the plain-`String` API.
pub(super) fn expand_transition(
    body_bytes: &[u8],
    span: &RegionSpan,
    indent: usize,
    lang: TargetLanguage,
    ctx: &HandlerContext,
    metadata: &SegmentMetadata,
) -> (String, &'static str) {
    let segment_text = String::from_utf8_lossy(&body_bytes[span.start..span.end]);
    let indent_str = " ".repeat(indent);

    // Parse transition: (exit_args)? -> (enter_args)? $State(state_args)?
    // For Python/TypeScript: Create compartment and call __transition()
    // For Rust: Use simpler _transition() approach

    // Check for forward-transition: -> => $State
    let is_forward = if let SegmentMetadata::Transition { is_forward, .. } = metadata {
        *is_forward
    } else {
        false
    };

    // Check for pop-transition: -> pop$
    let is_pop = if let SegmentMetadata::Transition { is_pop, .. } = metadata {
        *is_pop
    } else {
        segment_text.contains("pop$")
    };
    // Every transition variant exits the handler with the same per-language
    // terminator; the body-builders below omit it so the orchestrator can
    // hoist a same-scope `@@:(expr)` before it (#169).
    let body = if is_pop {
        // Pop-transition with optional decorations (RFC-0008):
        // 1. Write exit_args to current compartment (if present)
        // 2. Pop from stack
        // 3. If enter_args present: clear + write fresh values
        // 4. If is_forward: set forward_event
        // 5. __transition (terminator appended by the caller)
        let (exit_str, enter_str) = match metadata {
            SegmentMetadata::Transition {
                exit_args,
                enter_args,
                ..
            } => (exit_args.clone(), enter_args.clone()),
            _ => (None, None),
        };
        generate_pop_transition(&indent_str, ctx, lang, &exit_str, &enter_str, is_forward)
    } else {
        // Transition: `(exit) -> (enter) $State(state)`, plus the
        // forward variant `(exit) -> (enter) => $State(state)`.
        //
        // Both variants share ALL arg emission — exit args via
        // __prepareExit, enter+state args via __prepareEnter — so the
        // two paths run through THIS one branch (#128: the old separate
        // forward branch emitted empty arg channels and silently
        // dropped every decoration). The ONLY difference is the forward
        // variant additionally sets the new compartment's
        // `forward_event` field (emitted by `forward_event_line` below,
        // inserted between __prepareEnter and __transition) so the
        // destination re-dispatches the in-flight event after enter.
        //
        // Transition metadata is always populated by the scanner.
        let (target, exit_args, enter_args, state_args) = match metadata {
            SegmentMetadata::Transition {
                target_state,
                exit_args,
                enter_args,
                state_args,
                ..
            } => (
                target_state.clone(),
                exit_args.clone(),
                enter_args.clone(),
                state_args.clone(),
            ),
            _ => unreachable!(
                "Transition kind segment without Transition metadata: {:?}",
                metadata
            ),
        };

        // Expand state variable references in arguments
        let exit_str = exit_args.map(|a| expand_expression(&a, lang, ctx));
        let enter_str = enter_args.map(|a| expand_expression(&a, lang, ctx));
        let state_str = state_args.map(|a| expand_expression(&a, lang, ctx));

        // Get compartment class name from system name
        let _compartment_class = format!("{}Compartment", ctx.system_name);

        // Forward variant only: the line that sets the destination
        // compartment's `forward_event` so the in-flight event is
        // re-dispatched into the target after its enter cascade. The
        // compartment-local name varies per backend (`__compartment`,
        // `__next`, `$__compartment`, …), so the caller passes it in.
        // Returns "" for a non-forward transition (regular path).
        let forward_event_line = |comp: &str| -> String {
            if !is_forward {
                return String::new();
            }
            match lang {
                TargetLanguage::Python3
                | TargetLanguage::GDScript
                | TargetLanguage::Swift
                | TargetLanguage::Kotlin
                | TargetLanguage::Ruby
                | TargetLanguage::Lua => {
                    format!("{}{}.forward_event = __e\n", indent_str, comp)
                }
                TargetLanguage::TypeScript | TargetLanguage::JavaScript | TargetLanguage::Dart => {
                    format!("{}{}.forward_event = __e;\n", indent_str, comp)
                }
                TargetLanguage::Java | TargetLanguage::CSharp => {
                    format!("{}{}.forward_event = __e;\n", indent_str, comp)
                }
                TargetLanguage::Php => {
                    format!("{}{}->forward_event = $__e;\n", indent_str, comp)
                }
                TargetLanguage::Go => {
                    format!("{}{}.forwardEvent = __e\n", indent_str, comp)
                }
                TargetLanguage::C => {
                    format!("{}{}->forward_event = __e;\n", indent_str, comp)
                }
                TargetLanguage::Cpp => format!(
                    "{}{}->forward_event = std::make_unique<{}FrameEvent>(__e);\n",
                    indent_str, comp, ctx.system_name
                ),
                // Rust + Erlang dispatch to dedicated helpers below and
                // never call this closure.
                TargetLanguage::Rust | TargetLanguage::Graphviz => String::new(),
            }
        };

        match lang {
            TargetLanguage::Python3 => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                //   __prepareExit(exit_args) — populates
                //     exit_args on every layer of the source chain.
                //   __prepareEnter(leaf, state_args, enter_args) —
                //     constructs the destination chain via the
                //     static _HSM_CHAIN topology table; every
                //     layer gets independent copies of the args
                //     (uniform parameter propagation).
                //   __transition(comp) — caches destination for
                //     the kernel to process.
                let mut code = String::new();

                // Build state_args list literal.
                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                // Build enter_args list literal.
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(enter, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                // Populate exit_args on the source chain (omitted
                // when there are no exit_args).
                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}self.__prepareExit([{}])\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                // Construct destination chain via the helper.
                code.push_str(&format!(
                    "{}__compartment = self.__prepareEnter(\"{}\", {}, {})\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                // Forward variant: re-dispatch the in-flight event.
                code.push_str(&forward_event_line("__compartment"));

                // Cache and return.
                code.push_str(&format!("{}self.__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::GDScript => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(enter, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}self.__prepareExit([{}])\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}var __compartment = self.__prepareEnter(\"{}\", {}, {})\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}self.__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
                // Per-handler architecture with helpers (see
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareExit / __prepareEnter / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(enter, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}this.__prepareExit([{}]);\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}const __compartment = this.__prepareEnter(\"{}\", {}, {});\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}this.__transition(__compartment);", indent_str));
                code
            }
            TargetLanguage::Dart => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(enter, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}this.__prepareExit([{}]);\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}final __compartment = this.__prepareEnter(\"{}\", {}, {});\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}this.__transition(__compartment);", indent_str));
                code
            }
            TargetLanguage::Rust => super::super::rust_system::rust_expand_transition(
                &indent_str,
                ctx,
                &target,
                &exit_str,
                &state_str,
                &enter_str,
                is_forward,
            ),
            TargetLanguage::C => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();
                let sys = &ctx.system_name;

                // Positional push of a handler arg, packed per its declared
                // marshal category (#81): float/double heap-box via
                // pack_double and push OWNED (the vec frees/deep-copies the
                // box — `(void*)(intptr_t)` truncates, and the handler-side
                // read derefs); everything else keeps the historical
                // intptr_t fallback (struct args unsupported on C, #72).
                // `types` is the (state, param_name) → declared-type map for
                // the slot family; `names` gives the positional param names.
                let push_typed = |idx: usize,
                                  value_expr: &str,
                                  state: &str,
                                  names: &[String],
                                  types: &std::collections::HashMap<(String, String), String>|
                 -> (String, &'static str) {
                    use super::super::c_marshal::{c_marshal_of, CMarshal};
                    let frame_type = names
                        .get(idx)
                        .and_then(|name| types.get(&(state.to_string(), name.clone())).cloned())
                        .unwrap_or_default();
                    match c_marshal_of(&frame_type) {
                        CMarshal::Dbl => (
                            format!("{}_pack_double({})", sys, value_expr),
                            "FrameVec_push_owned",
                        ),
                        _ => (
                            format!("(void*)(intptr_t)({})", value_expr),
                            "FrameVec_push",
                        ),
                    }
                };

                // exit_args via __prepareExit if any provided. Declared
                // types come from the SOURCE state's `<$` params.
                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        let exit_names: Vec<String> = ctx
                            .state_exit_param_names
                            .get(&ctx.state_name)
                            .cloned()
                            .unwrap_or_default();
                        code.push_str(&format!(
                            "{}{{ {}_FrameVec* __ea = {}_FrameVec_new();\n",
                            indent_str, sys, sys
                        ));
                        for (i, v) in vals.iter().enumerate() {
                            let (arg, push_fn) = push_typed(
                                i,
                                v,
                                &ctx.state_name,
                                &exit_names,
                                &ctx.state_exit_param_types,
                            );
                            code.push_str(&format!(
                                "{}{}_{}(__ea, {});\n",
                                indent_str, sys, push_fn, arg
                            ));
                        }
                        code.push_str(&format!("{}{}_prepareExit(self, __ea);\n", indent_str, sys));
                        code.push_str(&format!(
                            "{}{}_FrameVec_destroy(__ea); }}\n",
                            indent_str, sys
                        ));
                    }
                }

                // Build state_args / enter_args FrameVecs, call __prepareEnter.
                let state_vals: Vec<String> = if let Some(ref state) = state_str {
                    crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                        state, lang,
                    )
                    .into_iter()
                    .map(|arg| {
                        if let Some(eq_pos) = arg.find('=') {
                            arg[eq_pos + 1..].trim().to_string()
                        } else {
                            arg.to_string()
                        }
                    })
                    .collect()
                } else {
                    Vec::new()
                };
                let enter_vals: Vec<String> = if let Some(ref enter) = enter_str {
                    crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                        enter, lang,
                    )
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect()
                } else {
                    Vec::new()
                };

                // Open block scope so locals don't collide with
                // sibling transitions in the same handler (e.g.
                // separate `if` branches).
                // Look up target's state-arg names so each value
                // can be packed using its declared type. Float /
                // double heap-box via `Sys_pack_double` (#81 —
                // (intptr_t) truncates; bit-punning corrupts on
                // 32-bit pointers) and push OWNED so the vec
                // frees/deep-copies the box.
                let target_param_names: Vec<String> = ctx
                    .state_param_names
                    .get(&target)
                    .cloned()
                    .unwrap_or_default();
                let target_enter_param_names: Vec<String> = ctx
                    .state_enter_param_names
                    .get(&target)
                    .cloned()
                    .unwrap_or_default();
                code.push_str(&format!("{}{{\n", indent_str));
                if state_vals.is_empty() {
                    code.push_str(&format!(
                        "{}    {}_FrameVec* __sa = NULL;\n",
                        indent_str, sys
                    ));
                } else {
                    code.push_str(&format!(
                        "{}    {}_FrameVec* __sa = {}_FrameVec_new();\n",
                        indent_str, sys, sys
                    ));
                    for (i, v) in state_vals.iter().enumerate() {
                        let (push_arg, push_fn) =
                            push_typed(i, v, &target, &target_param_names, &ctx.state_param_types);
                        code.push_str(&format!(
                            "{}    {}_{}(__sa, {});\n",
                            indent_str, sys, push_fn, push_arg
                        ));
                    }
                }
                if enter_vals.is_empty() {
                    code.push_str(&format!(
                        "{}    {}_FrameVec* __ea = NULL;\n",
                        indent_str, sys
                    ));
                } else {
                    code.push_str(&format!(
                        "{}    {}_FrameVec* __ea = {}_FrameVec_new();\n",
                        indent_str, sys, sys
                    ));
                    // Enter args: declared types come from the TARGET
                    // state's `$>` params.
                    for (i, v) in enter_vals.iter().enumerate() {
                        let (push_arg, push_fn) = push_typed(
                            i,
                            v,
                            &target,
                            &target_enter_param_names,
                            &ctx.state_enter_param_types,
                        );
                        code.push_str(&format!(
                            "{}    {}_{}(__ea, {});\n",
                            indent_str, sys, push_fn, push_arg
                        ));
                    }
                }
                code.push_str(&format!(
                            "{}    {}_Compartment* __compartment = {}_prepareEnter(self, \"{}\", __sa, __ea);\n",
                            indent_str, sys, sys, target
                        ));
                if !state_vals.is_empty() {
                    code.push_str(&format!(
                        "{}    {}_FrameVec_destroy(__sa);\n",
                        indent_str, sys
                    ));
                }
                if !enter_vals.is_empty() {
                    code.push_str(&format!(
                        "{}    {}_FrameVec_destroy(__ea);\n",
                        indent_str, sys
                    ));
                }
                if is_forward {
                    code.push_str(&format!(
                        "{}    __compartment->forward_event = __e;\n",
                        indent_str
                    ));
                }
                code.push_str(&format!(
                    "{}    {}_transition(self, __compartment);\n",
                    indent_str, sys
                ));
                code.push_str(&format!("{}}}\n", indent_str));
                code
            }
            TargetLanguage::Cpp => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals: Vec<String> =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            state, lang,
                        )
                        .into_iter()
                        .map(|arg| {
                            let raw =
                                crate::frame_c::compiler::codegen::codegen_utils::strip_named_arg(
                                    &arg,
                                );
                            format!("std::any({})", cpp_wrap_any_arg(&raw))
                        })
                        .collect();
                    format!("std::vector<std::any>{{{}}}", vals.join(", "))
                } else {
                    "std::vector<std::any>{}".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals: Vec<String> =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        )
                        .into_iter()
                        .map(|a| format!("std::any({})", cpp_wrap_any_arg(&a)))
                        .collect();
                    format!("std::vector<std::any>{{{}}}", vals.join(", "))
                } else {
                    "std::vector<std::any>{}".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals: Vec<String> =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        )
                        .into_iter()
                        .map(|a| format!("std::any({})", cpp_wrap_any_arg(&a)))
                        .collect();
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}__prepareExit(std::vector<std::any>{{{}}});\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}auto __next = __prepareEnter(\"{}\", {}, {});\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__next"));

                code.push_str(&format!("{}__transition(std::move(__next));", indent_str));
                code
            }
            TargetLanguage::Java => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    if vals.is_empty() {
                        "new java.util.ArrayList<>()".to_string()
                    } else {
                        format!(
                            "new java.util.ArrayList<>(java.util.Arrays.asList({}))",
                            vals.join(", ")
                        )
                    }
                } else {
                    "new java.util.ArrayList<>()".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        );
                    if vals.is_empty() {
                        "new java.util.ArrayList<>()".to_string()
                    } else {
                        format!(
                            "new java.util.ArrayList<>(java.util.Arrays.asList({}))",
                            vals.join(", ")
                        )
                    }
                } else {
                    "new java.util.ArrayList<>()".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                                    "{}__prepareExit(new java.util.ArrayList<>(java.util.Arrays.asList({})));\n",
                                    indent_str,
                                    vals.join(", ")
                                ));
                    }
                }

                code.push_str(&format!(
                    "{}{}Compartment __compartment = __prepareEnter(\"{}\", {}, {});\n",
                    indent_str, ctx.system_name, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}__transition(__compartment);", indent_str));
                code
            }
            TargetLanguage::Kotlin => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    if vals.is_empty() {
                        "mutableListOf<Any?>()".to_string()
                    } else {
                        format!("mutableListOf<Any?>({})", vals.join(", "))
                    }
                } else {
                    "mutableListOf<Any?>()".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        );
                    if vals.is_empty() {
                        "mutableListOf<Any?>()".to_string()
                    } else {
                        format!("mutableListOf<Any?>({})", vals.join(", "))
                    }
                } else {
                    "mutableListOf<Any?>()".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}__prepareExit(mutableListOf<Any?>({}))\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}val __compartment = __prepareEnter(\"{}\", {}, {})\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::Swift => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        );
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}__prepareExit([{}])\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}let __compartment = {}.__prepareEnter(\"{}\", {}, {})\n",
                    indent_str, ctx.system_name, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::CSharp => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                // Note: local var is named `__next` (not
                // `__compartment`) to avoid shadowing the field
                // in stack-push handlers that reference the field
                // earlier in the same block — C# rejects that
                // even when the local is declared later.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    if vals.is_empty() {
                        "new List<object>()".to_string()
                    } else {
                        format!("new List<object> {{ {} }}", vals.join(", "))
                    }
                } else {
                    "new List<object>()".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        );
                    if vals.is_empty() {
                        "new List<object>()".to_string()
                    } else {
                        format!("new List<object> {{ {} }}", vals.join(", "))
                    }
                } else {
                    "new List<object>()".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}__prepareExit(new List<object> {{ {} }});\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                // Wrap in `{ ... }` block scope so multiple
                // transitions in the same handler (e.g. inside
                // separate `if` branches) don't trigger C#
                // CS0136 (same name used in enclosing scope).
                code.push_str(&format!(
                    "{}{{ {}Compartment __next = __prepareEnter(\"{}\", {}, {});\n",
                    indent_str, ctx.system_name, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__next"));

                code.push_str(&format!("{}__transition(__next); }}", indent_str));
                code
            }
            TargetLanguage::Go => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[]any{{{}}}", vals.join(", "))
                } else {
                    "[]any{}".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        );
                    format!("[]any{{{}}}", vals.join(", "))
                } else {
                    "[]any{}".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}s.__prepareExit([]any{{{}}})\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}__compartment := s.__prepareEnter(\"{}\", {}, {})\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}s.__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::Php => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();
                let current_params = ctx
                    .event_param_names
                    .get(&ctx.event_name)
                    .cloned()
                    .unwrap_or_default();
                let php_fix = |expr: &str| php_prefix_params(expr, &current_params);

                let state_args_list = if let Some(ref state) = state_str {
                    let vals: Vec<String> =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            state, lang,
                        )
                        .into_iter()
                        .map(|arg| {
                            let raw =
                                crate::frame_c::compiler::codegen::codegen_utils::strip_named_arg(
                                    &arg,
                                );
                            php_fix(&raw)
                        })
                        .collect();
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals: Vec<String> =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            enter, lang,
                        )
                        .into_iter()
                        .map(|arg| {
                            let raw =
                                crate::frame_c::compiler::codegen::codegen_utils::strip_named_arg(
                                    &arg,
                                );
                            php_fix(&raw)
                        })
                        .collect();
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals: Vec<String> =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        )
                        .into_iter()
                        .map(|a| php_fix(&a))
                        .collect();
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}$this->__prepareExit([{}]);\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}$__compartment = $this->__prepareEnter(\"{}\", {}, {});\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("$__compartment"));

                code.push_str(&format!(
                    "{}$this->__transition($__compartment);",
                    indent_str
                ));
                code
            }
            TargetLanguage::Ruby => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+):
                // __prepareEnter / __prepareExit / __transition.
                let mut code = String::new();

                let state_args_list = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };
                let enter_args_list = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(enter, lang);
                    format!("[{}]", vals.join(", "))
                } else {
                    "[]".to_string()
                };

                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}__prepareExit([{}])\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}__compartment = __prepareEnter(\"{}\", {}, {})\n",
                    indent_str, target, state_args_list, enter_args_list
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::Lua => {
                // Per-handler architecture with helpers (per
                // docs/frame_runtime_introduction.md Step 21+).
                // Uses table.pack(...) instead of `{}` literals
                // because the Lua block transformer mishandles
                // `{}` table literals inside if/else bodies
                // (sees them as nested block braces). nil is
                // accepted by __prepareEnter / __prepareExit
                // when there are no args.
                let mut code = String::new();

                // state_args
                let state_arg = if let Some(ref state) = state_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(state, lang);
                    if vals.is_empty() {
                        "nil".to_string()
                    } else {
                        format!("table.pack({})", vals.join(", "))
                    }
                } else {
                    "nil".to_string()
                };

                // enter_args
                let enter_arg = if let Some(ref enter) = enter_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::arg_values(enter, lang);
                    if vals.is_empty() {
                        "nil".to_string()
                    } else {
                        format!("table.pack({})", vals.join(", "))
                    }
                } else {
                    "nil".to_string()
                };

                // exit_args (only emitted when present)
                if let Some(ref exit) = exit_str {
                    let vals =
                        crate::frame_c::compiler::codegen::codegen_utils::split_top_level_args(
                            exit, lang,
                        );
                    if !vals.is_empty() {
                        code.push_str(&format!(
                            "{}self:__prepareExit(table.pack({}))\n",
                            indent_str,
                            vals.join(", ")
                        ));
                    }
                }

                code.push_str(&format!(
                    "{}local __compartment = self:__prepareEnter(\"{}\", {}, {})\n",
                    indent_str, target, state_arg, enter_arg
                ));

                code.push_str(&forward_event_line("__compartment"));

                code.push_str(&format!("{}self:__transition(__compartment)", indent_str));
                code
            }
            TargetLanguage::Graphviz => unreachable!(),
        }
    };
    (body, super::utility::transition_terminator(lang))
}
