//! Interface wrapper, action, operation, and persistence code generation.
//!
//! Generates:
//! - Interface method wrappers (public API → kernel dispatch)
//! - Action method bodies (native code with self.X rewriting)
//! - Operation method bodies (static/class methods)
//! - Persistence serialization/deserialization methods

mod dart_types;
mod extract;
mod nested_registry;
mod persist;
mod utility;

pub(crate) use extract::{extract_body_content, extract_tagged_system_name, strip_body_braces};

use dart_types::{dart_conv_expr, parse_dart_type, render_dart_type, DartTypeNode};
use utility::{frame_return_default, is_dynamic_target};

pub use nested_registry::{
    child_persist_names, get_nested_system_domain_params, known_system_names,
    nested_uses_new_contract, set_local_systems, set_nested_system_domain_params,
    set_nested_system_persist_names, set_new_contract_systems, set_system_interfaces,
    unique_system_with_interface_method,
};

use std::collections::{HashMap, HashSet};

use super::ast::{CodegenNode, Param, Visibility};
use super::codegen_utils::{
    cpp_map_type, cpp_wrap_any_arg, csharp_map_type, expression_to_string, go_map_type,
    java_map_type, kotlin_map_type, swift_map_type, to_snake_case, type_to_cpp_string,
    type_to_string, HandlerContext,
};
use crate::frame_c::compiler::frame_ast::{
    ActionAst, InterfaceMethod, MethodParam, OperationAst, Span, SystemAst, Type,
};
use crate::frame_c::visitors::TargetLanguage;

/// Generate interface wrapper methods
///
/// For Python/TypeScript: Create FrameEvent and call __kernel
/// For Rust: Use match-based dispatch directly
///
/// If no explicit interface is defined, auto-generate interface methods from
/// unique event handlers found in the machine states (excluding lifecycle events).
pub(crate) fn generate_interface_wrappers(
    system: &SystemAst,
    syntax: &super::backend::ClassSyntax,
) -> Vec<CodegenNode> {
    // Get the target language from the syntax
    let lang = syntax.language;
    let event_class = format!("{}FrameEvent", system.name);
    // RFC-0043 / #158: async systems get their `await`s AT EMISSION (asyncness
    // is known here) instead of a post-pass rescanning emitted text for
    // dispatch-call prefixes. `aw` is the prefix-await keyword for the
    // languages that use one; Rust/C++/Java/Kotlin sites handle their own
    // idiom (postfix `.await` / `co_await` / sync internals / bare suspend).
    let sys_async = system.is_async_layered();
    let aw = if sys_async { "await " } else { "" };

    // If explicit interface is defined, use it
    // Otherwise, collect unique events from state handlers
    let interface_methods: Vec<InterfaceMethod> = if !system.interface.is_empty() {
        system.interface.clone()
    } else {
        // Auto-generate interface from event handlers
        // `events` only dedups; `order` records first-seen source order so the
        // derived interface is emitted deterministically (the same source
        // declaration order the explicit-`interface:` path uses). Iterating the
        // HashSet was non-deterministic across runs — the same class of bug as
        // commit 72f3ea5 (deterministic state + handler iteration), missed on
        // this auto-derive path.
        let mut events: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut method_info: std::collections::HashMap<String, (Vec<MethodParam>, Option<Type>)> =
            std::collections::HashMap::new();

        if let Some(ref machine) = system.machine {
            for state in &machine.states {
                for handler in &state.handlers {
                    // Skip lifecycle events
                    if handler.event == "$>"
                        || handler.event == "<$"
                        || handler.event == "$>|"
                        || handler.event == "<$|"
                    {
                        continue;
                    }
                    if events.insert(handler.event.clone()) {
                        order.push(handler.event.clone());
                        // First time seeing this event - capture its params and return type
                        let params: Vec<MethodParam> = handler
                            .params
                            .iter()
                            .map(|p| MethodParam {
                                name: p.name.clone(),
                                param_type: p.param_type.clone(),
                                default: None,
                                span: Span::new(0, 0),
                            })
                            .collect();
                        method_info
                            .insert(handler.event.clone(), (params, handler.return_type.clone()));
                    }
                }
            }
        }

        order
            .into_iter()
            .map(|event| {
                let (params, return_type) = method_info.get(&event).cloned().unwrap_or_default();
                InterfaceMethod {
                    name: event,
                    params,
                    return_type,
                    return_init: None,
                    is_async: false,
                    is_static: false,
                    leading_comments: Vec::new(),
                    attributes: Vec::new(),
                    span: Span::new(0, 0),
                }
            })
            .collect()
    };

    interface_methods.iter().flat_map(|method| {
        let params: Vec<Param> = method.params.iter().map(|p| {
            let type_str = type_to_string(&p.param_type);
            Param::new(&p.name).with_type(&type_str)
        }).collect();

        let _args: Vec<CodegenNode> = method.params.iter()
            .map(|p| CodegenNode::ident(&p.name))
            .collect();

        // Language-specific dispatch - all languages now use kernel pattern
        let body_stmt = match lang {
            TargetLanguage::Rust => {
                super::rust_system::generate_rust_interface_body(
                    &system.name,
                    method,
                    &event_class,
                    sys_async,
                )
            }
            TargetLanguage::Python3 => {
                // Python: Create FrameEvent + FrameContext, push context, call __kernel, pop and return
                // Parameters are packed positionally into a list
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "[]".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    format!("[{}]", param_items.join(", "))
                };

                // Python is dynamic per docs/frame_runtime.md § "Return
                // values across target languages": source-level type
                // annotations are documentation only. The wrapper
                // returns whatever's in the FrameContext's return slot
                // — including None when no @@:return was written.
                // Don't init the slot to a type default here; that
                // would contradict the documented dynamic-lang
                // contract and break test 14_system_return_default.
                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n__ctx._return = {}", init_expr)
                } else {
                    String::new()
                };

                // Python is a dynamic target: the wrapper always exposes
                // the FrameContext's return slot, regardless of declared
                // return type (see docs/frame_runtime.md § "Return values
                // across target languages"). Source-level `: type` is
                // documentation only here.
                let has_return = is_dynamic_target(lang)
                    || method.return_type.is_some()
                    || method.return_init.is_some();
                if has_return {
                    CodegenNode::NativeBlock {
                        // D-PY-1: the pop MUST run even if __kernel raises,
                        // or a failed dispatch leaks a stale context-stack
                        // entry. try/finally guarantees it (async injects
                        // `await` before self.__kernel — line-based, so the
                        // try/finally lines are untouched).
                        code: format!(
                            r#"__e = {}("{}", {})
__ctx = {}(__e, None){}
self._context_stack.append(__ctx)
try:
    {aw}self.__kernel(__e)
finally:
    __frame_ctx = self._context_stack.pop()
return __frame_ctx._return"#,
                            event_class, method.name, params_code, context_class, default_init, aw = aw
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        // D-PY-1: pop in finally so a raising handler can't
                        // leak a context-stack entry.
                        code: format!(
                            r#"__e = {}("{}", {})
__ctx = {}(__e, None){}
self._context_stack.append(__ctx)
try:
    {aw}self.__kernel(__e)
finally:
    self._context_stack.pop()"#,
                            event_class, method.name, params_code, context_class, default_init, aw = aw
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
                // TypeScript/JavaScript: Create FrameEvent + FrameContext, push context, call __kernel, pop and return
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "[]".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    format!("[{}]", param_items.join(", "))
                };

                // Per docs/frame_runtime.md § "Return values across
                // target languages": all backends return the slot's
                // natural null/undefined when no @@:return was
                // written. Source-level type annotations don't
                // change the runtime default.
                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n__ctx._return = {};", init_expr)
                } else {
                    String::new()
                };

                // TypeScript uses ! (non-null assertion), JavaScript doesn't
                let pop_suffix = if matches!(lang, TargetLanguage::TypeScript) { "!" } else { "" };

                // JavaScript is dynamic (always returns); TypeScript is
                // strongly-typed (conditional on declared type).
                if is_dynamic_target(lang)
                    || method.return_type.is_some()
                    || method.return_init.is_some()
                {
                    CodegenNode::NativeBlock {
                        // D-PY-1: pop on both success and exception paths so a
                        // throwing handler can't leak a context-stack entry.
                        code: format!(
                            "const __e = new {}(\"{}\", {});\nconst __ctx = new {}(__e, null);{}\nthis._context_stack.push(__ctx);\ntry {{\n    {aw}this.__kernel(__e);\n    return this._context_stack.pop(){}._return;\n}} catch (__frame_err) {{\n    this._context_stack.pop();\n    throw __frame_err;\n}}",
                            event_class, method.name, params_code, context_class, default_init, pop_suffix, aw = aw
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            "const __e = new {}(\"{}\", {});\nconst __ctx = new {}(__e, null);{}\nthis._context_stack.push(__ctx);\ntry {{\n    {aw}this.__kernel(__e);\n}} finally {{\n    this._context_stack.pop();\n}}",
                            event_class, method.name, params_code, context_class, default_init, aw = aw
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::Php => {
                // PHP: Create FrameEvent + FrameContext, push context, call __kernel, pop and return
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "[]".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| format!("${}", p.name))
                        .collect();
                    format!("[{}]", param_items.join(", "))
                };

                // PHP is dynamic — see Python branch comment.
                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n$__ctx->_return = {};", init_expr)
                } else {
                    String::new()
                };

                if is_dynamic_target(lang)
                    || method.return_type.is_some()
                    || method.return_init.is_some()
                {
                    CodegenNode::NativeBlock {
                        // D-PY-1: pop on both paths so a throwing handler
                        // can't leak a context-stack entry.
                        code: format!(
                            "$__e = new {}(\"{}\", {});\n$__ctx = new {}($__e, null);{}\n$this->_context_stack[] = $__ctx;\ntry {{\n    $this->__kernel($__e);\n    return array_pop($this->_context_stack)->_return;\n}} catch (\\Throwable $__frame_err) {{\n    array_pop($this->_context_stack);\n    throw $__frame_err;\n}}",
                            event_class, method.name, params_code, context_class, default_init
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            "$__e = new {}(\"{}\", {});\n$__ctx = new {}($__e, null);{}\n$this->_context_stack[] = $__ctx;\ntry {{\n    $this->__kernel($__e);\n}} finally {{\n    array_pop($this->_context_stack);\n}}",
                            event_class, method.name, params_code, context_class, default_init
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::Ruby => {
                // Ruby: Create FrameEvent + FrameContext, push context, call __kernel, pop and return
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "[]".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    format!("[{}]", param_items.join(", "))
                };

                // Ruby is dynamic — see Python branch comment.
                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n__ctx._return = {}", init_expr)
                } else {
                    String::new()
                };

                if is_dynamic_target(lang)
                    || method.return_type.is_some()
                    || method.return_init.is_some()
                {
                    CodegenNode::NativeBlock {
                        // D-PY-1: ensure the pop runs even if a handler raises.
                        code: format!(
                            "__e = {}.new(\"{}\", {})\n__ctx = {}.new(__e, nil){}\n@_context_stack.push(__ctx)\nbegin\n    __kernel(__e)\n    return @_context_stack.last._return\nensure\n    @_context_stack.pop\nend",
                            event_class, method.name, params_code, context_class, default_init
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            "__e = {}.new(\"{}\", {})\n__ctx = {}.new(__e, nil){}\n@_context_stack.push(__ctx)\nbegin\n    __kernel(__e)\nensure\n    @_context_stack.pop\nend",
                            event_class, method.name, params_code, context_class, default_init
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::C => {
                // C: Create FrameEvent + FrameContext, push context, call kernel, pop and return
                let sys = &system.name;

                // Build parameters dict creation (with semicolon).
                // Marshalling per `c_marshal_of` (#72, #81): float/double
                // and struct-by-value params travel as the address of a
                // STACK box (the kernel dispatch is synchronous, so the
                // wrapper's locals outlive every reader; see c_marshal.rs
                // § ownership). Doubles never heap-allocate on the param
                // path and never bit-pun into the pointer (wasm32-unsafe).
                let params_code = if method.params.is_empty() {
                    format!("{}_FrameEvent* __e = {}_FrameEvent_new(\"{}\", NULL, 0);", sys, sys, method.name)
                } else {
                    let mut code = format!("{}_FrameVec* __params = {}_FrameVec_new();\n", sys, sys);
                    for p in &method.params {
                        let p_type = type_to_string(&p.param_type);
                        let push_arg = match super::c_marshal::c_marshal_of(&p_type) {
                            super::c_marshal::CMarshal::Dbl => {
                                code.push_str(&format!(
                                    "double __box_{} = {};\n",
                                    p.name, p.name
                                ));
                                format!("(void*)&__box_{}", p.name)
                            }
                            super::c_marshal::CMarshal::Boxed => {
                                code.push_str(&format!(
                                    "{} __box_{} = {};\n",
                                    p_type.trim(),
                                    p.name,
                                    p.name
                                ));
                                format!("(void*)&__box_{}", p.name)
                            }
                            _ => format!("(void*)(intptr_t){}", p.name),
                        };
                        code.push_str(&format!(
                            "{}_FrameVec_push(__params, {});\n",
                            sys, push_arg
                        ));
                    }
                    code.push_str(&format!("{}_FrameEvent* __e = {}_FrameEvent_new(\"{}\", __params, 1);", sys, sys, method.name));
                    code
                };

                // Check if method has a non-void return type
                let return_type_str = method.return_type.as_ref().map(|t| type_to_string(t));
                // Frame has no type system: return type names pass through
                // VERBATIM (FRAMEC_BUGS #37 — the str→char*/int→int/… alias
                // table was exterminated). Only the framework's no-return
                // spelling is normalized (structural). Write C's own types.
                let return_type_str = return_type_str.map(|s| {
                    match s.as_str() {
                        "void" | "None" => "void".to_string(),
                        _ => s,
                    }
                });
                let has_return_value = return_type_str.as_ref()
                    .map(|s| s != "void" && s != "None")
                    .unwrap_or(false);

                // Set default return value after context creation —
                // marshalled per `c_return_write` (#72, #81): doubles and
                // structs heap-box (`(void*)(intptr_t)(3.14)` truncates;
                // bit-punning corrupts on 32-bit pointers).
                let default_init = if let Some(ref init_expr) = method.return_init {
                    let ty = return_type_str.as_deref().unwrap_or("int");
                    format!(
                        "\n{}",
                        super::c_marshal::c_return_write(sys, "__ctx->_return", init_expr, ty)
                    )
                } else {
                    String::new()
                };

                if let (true, Some(return_type_str)) = (has_return_value, return_type_str) {
                    // Unpack via `c_return_read` (#72) — the categorization
                    // is shared with the write side (c_return_assign /
                    // c_marshal) so pack and unpack cannot drift apart:
                    //   bool/int  → `(T)(intptr_t)…`
                    //   float/dbl → `Sys_unpack_double(…)` (NULL-safe heap-
                    //               box deref, #81), then free the box
                    //   str/ptr   → `(T)…` (pointer already fits)
                    //   struct    → `*(T*)…` deref-copy, then free the box
                    let extract = super::c_marshal::c_return_read(
                        sys,
                        "__result_ctx->_return",
                        &return_type_str,
                    );
                    // Boxed AND Dbl returns are heap boxes (#72, #81): free
                    // after the final read (the single owner-free; see
                    // c_marshal.rs). Guard against a never-written NULL slot
                    // — memset-zero gives 0/+0.0 for the unwritten case.
                    let owns_return = matches!(
                        super::c_marshal::c_marshal_of(&return_type_str),
                        super::c_marshal::CMarshal::Boxed | super::c_marshal::CMarshal::Dbl
                    );
                    let result_decl = if owns_return {
                        format!(
                            "{t} __result; memset(&__result, 0, sizeof(__result));\nif (__result_ctx->_return) {{ __result = {extract}; free(__result_ctx->_return); }}",
                            t = return_type_str, extract = extract
                        )
                    } else {
                        format!("{} __result = {};", return_type_str, extract)
                    };
                    CodegenNode::NativeBlock {
                        code: format!(
                            r#"{}
{}_FrameContext* __ctx = {}_FrameContext_new(__e, NULL);{}
{}_FrameVec_push(self->_context_stack, __ctx);
{}_kernel(self, __e);
{}_FrameContext* __result_ctx = ({}_FrameContext*){}_FrameVec_pop(self->_context_stack);
{}
{}_FrameContext_destroy(__result_ctx);
{}_FrameEvent_destroy(__e);
return __result;"#,
                            params_code, sys, sys, default_init, sys, sys, sys, sys, sys,
                            result_decl, sys, sys
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            r#"{}
{}_FrameContext* __ctx = {}_FrameContext_new(__e, NULL);{}
{}_FrameVec_push(self->_context_stack, __ctx);
{}_kernel(self, __e);
{}_FrameContext* __result_ctx = ({}_FrameContext*){}_FrameVec_pop(self->_context_stack);
{}_FrameContext_destroy(__result_ctx);
{}_FrameEvent_destroy(__e);"#,
                            params_code, sys, sys, default_init, sys, sys, sys, sys, sys, sys, sys
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::Cpp => {
                let context_class = format!("{}FrameContext", system.name);
                // Map the Frame return type to its C++ equivalent BEFORE
                // any further use. Without this `: str` returns leak into
                // `std::any_cast<str>` (undefined identifier) and `: void`
                // returns leak into `std::any_cast<void>` (which is a
                // template substitution failure). The mapper translates
                // `str`/`string`/`String` to `std::string`, `bool`/`boolean`
                // to `bool`, etc.
                let raw_return_type = method.return_type.as_ref()
                    .map(|t| type_to_string(t))
                    .unwrap_or_else(|| "void".to_string());
                let return_type_str = cpp_map_type(&raw_return_type);
                // A method with `: void` declared has no value to extract
                // from the return slot — treat it as no-return for the
                // any_cast/return path. The context still needs to be
                // pushed so the handler can run, but we never read back.
                let returns_value = (method.return_type.is_some() || method.return_init.is_some())
                    && return_type_str != "void";
                // System is async when any interface method is declared
                // async. Bodies `co_return` instead of `return`; internal
                // dispatch calls get `co_await` prefix (see `add_await_to_string`).
                let system_is_async = system.interface.iter().any(|m| m.is_async);

                let mut code = String::new();

                // Build params list
                if !method.params.is_empty() {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    code.push_str(&format!("{} __e(\"{}\", std::vector<std::any>{{{}}});\n",
                        event_class, method.name, param_items.join(", ")));
                } else {
                    code.push_str(&format!("{} __e(\"{}\");\n", event_class, method.name));
                }

                // Create context with default return
                if let Some(ref init) = method.return_init {
                    // Wrap string literals for std::any (const char* != std::string)
                    let init_wrapped = if init.starts_with('"') && return_type_str == "std::string" {
                        format!("std::string({})", init)
                    } else {
                        init.clone()
                    };
                    code.push_str(&format!("{} __ctx(std::move(__e), std::any({}));\n", context_class, init_wrapped));
                } else if returns_value {
                    code.push_str(&format!("{} __ctx(std::move(__e), std::any({}()));\n", context_class, return_type_str));
                } else {
                    code.push_str(&format!("{} __ctx(std::move(__e));\n", context_class));
                }

                code.push_str("_context_stack.push_back(std::move(__ctx));\n");
                // Exception Policy (#86, RFC-0044): the context-stack balance
                // invariant `len_after == len_before` is maintained by an RAII
                // scope-guard, NOT a catch-and-rethrow. The guard's destructor
                // pops on every scope exit — normal return AND (if exceptions
                // are enabled) unwinding — so the previous `try/catch(...){ pop;
                // throw; }` is unnecessary. Frame handlers are state transitions
                // and never throw, so that catch path was dead; emitting it
                // forced a hard link-time dependency on the C++ exception
                // runtime, which Godot's web (wasm) engine is built WITHOUT
                // (-fno-exceptions). The guard keeps the same safety while making
                // framec C++ output compile and link with exceptions disabled.
                // A method-local struct typed via `decltype(_context_stack)&`
                // is ODR-safe and needs no shared preamble.
                code.push_str(
                    "struct __CtxGuard { decltype(_context_stack)& s; ~__CtxGuard() { s.pop_back(); } } __ctx_guard{_context_stack};\n",
                );
                if system_is_async {
                    code.push_str("co_await __kernel(_context_stack.back()._event);\n");
                } else {
                    code.push_str("__kernel(_context_stack.back()._event);\n");
                }

                // __result is captured while the entry is still on the stack;
                // __ctx_guard pops AFTER the (co_)return value is constructed.
                let ret_kw = if system_is_async { "co_return" } else { "return" };
                if returns_value {
                    code.push_str(&format!("auto __result = std::any_cast<{}>(std::move(_context_stack.back()._return));\n", return_type_str));
                    code.push_str(&format!("{} __result;", ret_kw));
                } else if system_is_async {
                    code.push_str("co_return;");
                }

                CodegenNode::NativeBlock { code, span: None }
            }
            TargetLanguage::Java => {
                let context_class = format!("{}FrameContext", system.name);
                let has_return = method.return_type.is_some() || method.return_init.is_some();
                let return_type_str = method.return_type.as_ref()
                    .map(|t| type_to_string(t))
                    .unwrap_or_else(|| "void".to_string());
                // Java has no native async/await; methods declared
                // `async` in the Frame interface return
                // `CompletableFuture<T>` with the body running
                // synchronously and wrapping its result via
                // `completedFuture(...)`. Sync methods in the same
                // interface keep their plain return — Java doesn't
                // cascade async-ness across the whole interface
                // (unlike e.g. TypeScript or Kotlin). The user opts
                // each method in by declaring it `async`.
                let method_is_async = method.is_async;

                let mut code = String::new();

                // Build params list
                if !method.params.is_empty() {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    code.push_str(&format!("{} __e = new {}(\"{}\", new java.util.ArrayList<>(java.util.Arrays.asList({})));\n",
                        event_class, event_class, method.name, param_items.join(", ")));
                } else {
                    code.push_str(&format!("{} __e = new {}(\"{}\", new java.util.ArrayList<>());\n", event_class, event_class, method.name));
                }

                // Create context with default return. Per
                // docs/frame_runtime.md, all backends return the
                // slot's natural null when no @@:return is written.
                // Java's `(int) null` (auto-unbox) does throw NPE if
                // the user-written interface declares `: int` and
                // no handler writes @@:return — but that's a
                // user-bug pattern that the existing test corpus
                // doesn't exercise (every existing typed-int
                // handler explicitly writes @@:return). Documented
                // contract takes precedence.
                if let Some(ref init) = method.return_init {
                    code.push_str(&format!("{} __ctx = new {}(__e, {});\n", context_class, context_class, init));
                } else {
                    code.push_str(&format!("{} __ctx = new {}(__e, null);\n", context_class, context_class));
                }

                // D-PY-1: pop on both paths so a throwing handler can't leak
                // a context-stack entry (catch+rethrow; Java dispatch is not
                // declared `throws`, so only unchecked exceptions propagate).
                code.push_str("_context_stack.add(__ctx);\n");
                code.push_str("try {\n");
                code.push_str("    __kernel(_context_stack.get(_context_stack.size() - 1)._event);\n");

                if has_return && return_type_str != "void" && return_type_str != "Any" && return_type_str != "Object" {
                    let java_type = java_map_type(&return_type_str);
                    code.push_str(&format!("    {} __result = ({}) _context_stack.get(_context_stack.size() - 1)._return;\n", java_type, java_type));
                    code.push_str("    _context_stack.remove(_context_stack.size() - 1);\n");
                    if method_is_async {
                        code.push_str("    return java.util.concurrent.CompletableFuture.completedFuture(__result);\n");
                    } else {
                        code.push_str("    return __result;\n");
                    }
                } else {
                    code.push_str("    _context_stack.remove(_context_stack.size() - 1);\n");
                    if method_is_async {
                        code.push_str("    return java.util.concurrent.CompletableFuture.completedFuture(null);\n");
                    }
                }
                code.push_str("} catch (RuntimeException __frame_err) {\n");
                code.push_str("    _context_stack.remove(_context_stack.size() - 1);\n");
                code.push_str("    throw __frame_err;\n");
                code.push_str("}");

                CodegenNode::NativeBlock { code, span: None }
            }
            TargetLanguage::Kotlin => {
                let context_class = format!("{}FrameContext", system.name);
                let has_return = method.return_type.is_some() || method.return_init.is_some();
                let return_type_str = method.return_type.as_ref()
                    .map(|t| type_to_string(t))
                    .unwrap_or_else(|| "void".to_string());

                let mut code = String::new();

                // Build params list — Kotlin: no new, no semicolons, mutableListOf
                if !method.params.is_empty() {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    code.push_str(&format!("val __e = {}(\"{}\", mutableListOf({}))\n",
                        event_class, method.name, param_items.join(", ")));
                } else {
                    code.push_str(&format!("val __e = {}(\"{}\", mutableListOf())\n", event_class, method.name));
                }

                // Create context with type-appropriate default return.
                // If handler is suppressed (e.g., transition guard), _return
                // keeps this default — wrappers never see null for typed returns.
                if let Some(ref init) = method.return_init {
                    code.push_str(&format!("val __ctx = {}(__e, {})\n", context_class, init));
                } else if has_return && return_type_str != "void" {
                    let kotlin_type = kotlin_map_type(&return_type_str);
                    let kt_default = match kotlin_type.as_str() {
                        "Int" | "Long" => "0",
                        "Double" | "Float" => "0.0",
                        "Boolean" => "false",
                        "String" => "\"\"",
                        _ => "null",
                    };
                    code.push_str(&format!("val __ctx = {}(__e, {})\n", context_class, kt_default));
                } else {
                    code.push_str(&format!("val __ctx = {}(__e, null)\n", context_class));
                }

                // D-PY-1: pop on both paths so a throwing handler can't leak.
                code.push_str("_context_stack.add(__ctx)\n");
                code.push_str("try {\n");
                code.push_str("    __kernel(_context_stack[_context_stack.size - 1]._event)\n");

                if has_return && return_type_str != "void" && return_type_str != "Any" && return_type_str != "Any?" {
                    let kotlin_type = kotlin_map_type(&return_type_str);
                    code.push_str(&format!("    val __result = _context_stack[_context_stack.size - 1]._return as {}\n", kotlin_type));
                    code.push_str("    _context_stack.removeAt(_context_stack.size - 1)\n");
                    code.push_str("    return __result\n");
                } else {
                    code.push_str("    _context_stack.removeAt(_context_stack.size - 1)\n");
                }
                code.push_str("} catch (__frame_err: Throwable) {\n");
                code.push_str("    _context_stack.removeAt(_context_stack.size - 1)\n");
                code.push_str("    throw __frame_err\n");
                code.push_str("}");

                CodegenNode::NativeBlock { code, span: None }
            }
            TargetLanguage::Swift => {
                let context_class = format!("{}FrameContext", system.name);
                let has_return = method.return_type.is_some() || method.return_init.is_some();
                let return_type_str = method.return_type.as_ref()
                    .map(|t| type_to_string(t))
                    .unwrap_or_else(|| "void".to_string());

                let mut code = String::new();

                // Build params list — Swift: [Any] array literal.
                // #175: a param whose name is a Swift keyword must be
                // backtick-escaped here too (it references the escaped
                // func-signature param).
                if !method.params.is_empty() {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| crate::frame_c::compiler::codegen::codegen_utils::swift_escape_ident(&p.name).into_owned())
                        .collect();
                    code.push_str(&format!("let __e = {}(message: \"{}\", parameters: [{}])\n",
                        event_class, method.name, param_items.join(", ")));
                } else {
                    code.push_str(&format!("let __e = {}(message: \"{}\", parameters: [])\n", event_class, method.name));
                }

                // Create context with type-appropriate default return
                if let Some(ref init) = method.return_init {
                    code.push_str(&format!("let __ctx = {}(event: __e, defaultReturn: {})\n", context_class, init));
                } else if has_return && return_type_str != "void" {
                    let swift_type = swift_map_type(&return_type_str);
                    let sw_default = if swift_type.ends_with('?') {
                        "nil"
                    } else {
                        match swift_type.as_str() {
                            "Int" => "0",
                            "Double" | "Float" => "0.0",
                            "Bool" => "false",
                            "String" => "\"\"",
                            _ => "nil",
                        }
                    };
                    code.push_str(&format!("let __ctx = {}(event: __e, defaultReturn: {})\n", context_class, sw_default));
                } else {
                    code.push_str(&format!("let __ctx = {}(event: __e)\n", context_class));
                }

                // Exception Policy (RFC-0049 R4): pop via `defer` so the
                // context-stack balance invariant holds on every exit path —
                // including a throwing handler — Swift's idiomatic scope-exit
                // cleanup (the family member alongside C++ RAII, Go `defer`,
                // managed `try/finally`). The prior unconditional post-kernel
                // pop was exception-free but exception-UNSAFE (a throw skipped
                // it, leaking a stale context entry).
                code.push_str("_context_stack.append(__ctx)\n");
                code.push_str("defer { _context_stack.removeLast() }\n");
                code.push_str(&format!("{aw}__kernel(_context_stack[_context_stack.count - 1]._event)\n"));

                if has_return && return_type_str != "void" && return_type_str != "Any" && return_type_str != "Any?" {
                    let swift_type = swift_map_type(&return_type_str);
                    code.push_str(&format!("let __result = _context_stack[_context_stack.count - 1]._return as! {}\n", swift_type));
                    code.push_str("return __result");
                }

                CodegenNode::NativeBlock { code, span: None }
            }
            TargetLanguage::CSharp => {
                let context_class = format!("{}FrameContext", system.name);
                let has_return = method.return_type.is_some() || method.return_init.is_some();
                let return_type_str = method.return_type.as_ref()
                    .map(|t| type_to_string(t))
                    .unwrap_or_else(|| "void".to_string());

                let mut code = String::new();

                // Build params list
                if !method.params.is_empty() {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    code.push_str(&format!("{} __e = new {}(\"{}\", new List<object> {{ {} }});\n",
                        event_class, event_class, method.name, param_items.join(", ")));
                } else {
                    code.push_str(&format!("{} __e = new {}(\"{}\", new List<object>());\n", event_class, event_class, method.name));
                }

                // C# follows the same documented contract as Java
                // (see Java branch above): return slot's natural
                // null when no @@:return is written.
                if let Some(ref init) = method.return_init {
                    code.push_str(&format!("{} __ctx = new {}(__e, {});\n", context_class, context_class, init));
                } else {
                    code.push_str(&format!("{} __ctx = new {}(__e, null);\n", context_class, context_class));
                }

                // D-PY-1: pop on both paths so a throwing handler can't leak.
                code.push_str("_context_stack.Add(__ctx);\n");
                code.push_str("try {\n");
                code.push_str(&format!("    {aw}__kernel(_context_stack[_context_stack.Count - 1]._event);\n"));

                if has_return && return_type_str != "void" && return_type_str != "Any" && return_type_str != "object" {
                    let cs_type = csharp_map_type(&return_type_str);
                    code.push_str(&format!("    var __result = ({}) _context_stack[_context_stack.Count - 1]._return;\n", cs_type));
                    code.push_str("    _context_stack.RemoveAt(_context_stack.Count - 1);\n");
                    code.push_str("    return __result;\n");
                } else {
                    code.push_str("    _context_stack.RemoveAt(_context_stack.Count - 1);\n");
                }
                code.push_str("} catch {\n");
                code.push_str("    _context_stack.RemoveAt(_context_stack.Count - 1);\n");
                code.push_str("    throw;\n");
                code.push_str("}");

                CodegenNode::NativeBlock { code, span: None }
            }
            TargetLanguage::Go => {
                let has_return = method.return_type.is_some() || method.return_init.is_some();
                let return_type_str = method.return_type.as_ref()
                    .map(|t| type_to_string(t))
                    .unwrap_or_else(|| "void".to_string());

                let mut code = String::new();

                // Build params list
                if !method.params.is_empty() {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    code.push_str(&format!("__e := {}FrameEvent{{_message: \"{}\", _parameters: []any{{{}}}}}\n",
                        system.name, method.name, param_items.join(", ")));
                } else {
                    code.push_str(&format!("__e := {}FrameEvent{{_message: \"{}\", _parameters: []any{{}}}}\n", system.name, method.name));
                }

                // Create context with default return
                if let Some(ref init) = method.return_init {
                    code.push_str(&format!("__ctx := {}FrameContext{{_event: __e, _return: {}, _data: make(map[string]any)}}\n",
                        system.name, init));
                } else {
                    code.push_str(&format!("__ctx := {}FrameContext{{_event: __e, _data: make(map[string]any)}}\n",
                        system.name));
                }

                code.push_str("s._context_stack = append(s._context_stack, __ctx)\n");
                // D-PY-1: defer the pop so a panicking handler can't leak a
                // context-stack entry (defer runs on panic unwinding too).
                code.push_str("defer func() { s._context_stack = s._context_stack[:len(s._context_stack)-1] }()\n");
                code.push_str("s.__kernel(&s._context_stack[len(s._context_stack)-1]._event)\n");

                if has_return && return_type_str != "void" && return_type_str != "Any" {
                    let go_type = go_map_type(&return_type_str);
                    if !go_type.is_empty() {
                        // Read the return value while __ctx is still on the
                        // stack; the deferred pop runs after the return value
                        // is evaluated.
                        code.push_str(&format!("var __result {}\nif __rv := s._context_stack[len(s._context_stack)-1]._return; __rv != nil {{ __result = __rv.({}) }}\n", go_type, go_type));
                        code.push_str("return __result");
                    }
                    // go_type empty (unit-like return) → defer handles the pop.
                }
                // void → defer handles the pop.

                CodegenNode::NativeBlock { code, span: None }
            }
            TargetLanguage::Lua => {
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "{}".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    format!("{{{}}}", param_items.join(", "))
                };

                // Lua is dynamic — see Python branch comment.
                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n__ctx._return = {}", init_expr)
                } else {
                    String::new()
                };

                if is_dynamic_target(lang)
                    || method.return_type.is_some()
                    || method.return_init.is_some()
                {
                    // Lua's block transformer treats `return` as terminal
                    // and strips any token that follows it on the same
                    // line. Emitting `local __ret = ...; pop; return __ret`
                    // would have `return __ret` collapse to bare `return`,
                    // discarding the value. Use `table.remove(...)._return`
                    // as a single expression so the return statement has
                    // the value embedded inline — no trailing token to
                    // strip.
                    // D-PY-1: pcall the kernel so a handler `error()` can't
                    // leak a context-stack entry — pop, then re-raise. The
                    // final `return` stays an inline expression (block
                    // transformer strips a token after a bare `return`).
                    CodegenNode::NativeBlock {
                        code: format!(
                            "local __e = {}.new(\"{}\", {})\nlocal __ctx = {}.new(__e, nil){}\nself._context_stack[#self._context_stack + 1] = __ctx\nlocal __ok, __err = pcall(self.__kernel, self, __e)\nif not __ok then\n    self._context_stack[#self._context_stack] = nil\n    error(__err)\nend\nreturn table.remove(self._context_stack)._return",
                            event_class, method.name, params_code, context_class, default_init
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            "local __e = {}.new(\"{}\", {})\nlocal __ctx = {}.new(__e, nil){}\nself._context_stack[#self._context_stack + 1] = __ctx\nlocal __ok, __err = pcall(self.__kernel, self, __e)\nself._context_stack[#self._context_stack] = nil\nif not __ok then error(__err) end",
                            event_class, method.name, params_code, context_class, default_init
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::Dart => {
                // Dart: similar to TypeScript
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "[]".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    format!("[{}]", param_items.join(", "))
                };

                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n__ctx._return = {};", init_expr)
                } else {
                    String::new()
                };

                if method.return_type.is_some() || method.return_init.is_some() {
                    let rt = method.return_type.as_ref().map(|t| type_to_string(t)).unwrap_or_default();
                    let dart_default = match rt.as_str() {
                        "int" | "num" => "0",
                        "double" | "float" => "0.0",
                        "bool" => "false",
                        "String" | "str" | "string" => "''",
                        _ => "null",
                    };
                    CodegenNode::NativeBlock {
                        // D-PY-1: pop on both paths so a throwing handler can't leak.
                        code: format!(
                            "final __e = {}(\"{}\", {});\nfinal __ctx = {}(__e, {});{}\n_context_stack.add(__ctx);\ntry {{\n    {aw}__kernel(__e);\n    return _context_stack.removeLast()._return;\n}} catch (__frame_err) {{\n    _context_stack.removeLast();\n    rethrow;\n}}",
                            event_class, method.name, params_code, context_class, dart_default, default_init, aw = aw
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            "final __e = {}(\"{}\", {});\nfinal __ctx = {}(__e, null);{}\n_context_stack.add(__ctx);\ntry {{\n    {aw}__kernel(__e);\n}} finally {{\n    _context_stack.removeLast();\n}}",
                            event_class, method.name, params_code, context_class, default_init, aw = aw
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::GDScript => {
                // GDScript: similar to Python
                let context_class = format!("{}FrameContext", system.name);
                let params_code = if method.params.is_empty() {
                    "[]".to_string()
                } else {
                    let param_items: Vec<String> = method.params.iter()
                        .map(|p| p.name.clone())
                        .collect();
                    format!("[{}]", param_items.join(", "))
                };

                // GDScript is dynamic — see Python branch comment.
                let default_init = if let Some(ref init_expr) = method.return_init {
                    format!("\n__ctx._return = {}", init_expr)
                } else {
                    String::new()
                };

                let has_return = is_dynamic_target(lang)
                    || method.return_type.is_some()
                    || method.return_init.is_some();
                if has_return {
                    CodegenNode::NativeBlock {
                        code: format!(
                            r#"var __e = {}.new("{}", {})
var __ctx = {}.new(__e, null){}
self._context_stack.append(__ctx)
{aw}self.__kernel(__e)
return self._context_stack.pop_back()._return"#,
                            event_class, method.name, params_code, context_class, default_init, aw = aw
                        ),
                        span: None,
                    }
                } else {
                    CodegenNode::NativeBlock {
                        code: format!(
                            r#"var __e = {}.new("{}", {})
var __ctx = {}.new(__e, null){}
self._context_stack.append(__ctx)
{aw}self.__kernel(__e)
self._context_stack.pop_back()"#,
                            event_class, method.name, params_code, context_class, default_init, aw = aw
                        ),
                        span: None,
                    }
                }
            }
            TargetLanguage::Erlang => {
                // Erlang interface wrappers are generated by erlang_system.rs
                CodegenNode::Empty
            }
            TargetLanguage::Graphviz => unreachable!(),
        };

        // Source-level comments preceding this `interface:` method
        // declaration emit as `NativeBlock` nodes before the
        // `Method` itself. Per-backend `NativeBlock` rendering
        // already handles indentation, so the comment lines land
        // above the wrapper at the correct class-body depth. The
        // text passes through verbatim — Frame source for a target
        // already uses that target's comment leader (Oceans Model).
        let mut nodes = Vec::new();
        for comment in &method.leading_comments {
            nodes.push(CodegenNode::NativeBlock {
                code: comment.clone(),
                span: None,
            });
        }
        nodes.push(CodegenNode::Method {
            name: method.name.clone(),
            params,
            return_type: method.return_type.as_ref().map(|t| type_to_string(t)),
            body: vec![body_stmt],
            is_async: false,
            is_static: false,
            visibility: Visibility::Public,
            decorators: vec![],
        });
        nodes
    }).collect()
}

/// Build the minimal `HandlerContext` needed to lower `@@:self.*` in an
/// action/operation body (RFC-0046): the system name, the set of defined
/// systems, and the domain field → type map (for embed `->` vs scalar `.`
/// on C++). All handler-only fields (state vars, params, transitions) stay
/// empty — they don't occur in action/operation bodies.
fn self_expansion_ctx(
    system_name: &str,
    arcanum: &crate::frame_c::compiler::arcanum::Arcanum,
) -> super::codegen_utils::HandlerContext {
    let defined_systems: std::collections::HashSet<String> =
        arcanum.systems.keys().cloned().collect();
    let domain_field_types: std::collections::HashMap<String, String> = arcanum
        .systems
        .get(system_name)
        .map(|entry| {
            entry
                .domain_symbols
                .iter()
                .filter_map(|(name, sym)| {
                    Some((name.clone(), sym.symbol_type.as_deref()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    super::codegen_utils::HandlerContext {
        system_name: system_name.to_string(),
        defined_systems,
        domain_field_types,
        ..Default::default()
    }
}

/// Generate action method (and any leading-comment trivia).
///
/// Returns a `Vec<CodegenNode>`: zero-or-more `NativeBlock` comment
/// nodes followed by the action's `Method` node. Callers `.extend`
/// the result into the class's `methods` list. Extracts native code
/// from source using the body span (Oceans Model).
pub(crate) fn generate_action(
    action: &ActionAst,
    syntax: &super::backend::ClassSyntax,
    source: &[u8],
    system_name: &str,
    arcanum: &crate::frame_c::compiler::arcanum::Arcanum,
) -> Vec<CodegenNode> {
    let params: Vec<Param> = action
        .params
        .iter()
        .map(|p| {
            let type_str = type_to_string(&p.param_type);
            Param::new(&p.name).with_type(&type_str)
        })
        .collect();

    // RFC-0046: lower `@@:self.*` in the action body (native passthrough
    // otherwise leaks the sigil). Boundary-safe scanner pass; `@@:(expr)` /
    // `@@:system.state` are left for the textual pass below.
    let ctx = self_expansion_ctx(system_name, arcanum);
    let mut code = super::frame_expansion::expand_self_in_body(
        &action.body.span,
        source,
        syntax.language,
        &ctx,
    );

    // Lower `@@:(expr)` and `@@:system.state` in the action body. Per
    // `_scratch/bug_at_at_colon_paren_in_actions_passthrough.md`, the
    // concise return-sigil `@@:(expr)` is intuitive enough that users
    // reach for it in actions even though the doc table prescribes
    // native `return`. The shared expansion lowers it to
    // `return expr` (target-language idiomatic) so action bodies
    // get the DTRT treatment instead of passing the literal sigil
    // through to the target compiler. Same path as `generate_operation`.
    code = super::frame_expansion::expand_system_state_in_code(&code, syntax.language);

    let ret_type = match &action.return_type {
        crate::frame_c::compiler::frame_ast::Type::Unknown => None,
        t => Some(type_to_string(t)),
    };

    let mut nodes: Vec<CodegenNode> = action
        .leading_comments
        .iter()
        .map(|c| CodegenNode::NativeBlock {
            code: c.clone(),
            span: None,
        })
        .collect();
    nodes.push(CodegenNode::Method {
        name: action.name.clone(),
        params,
        return_type: ret_type,
        body: vec![CodegenNode::NativeBlock {
            code,
            span: Some(action.body.span.clone()),
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Private,
        decorators: vec![],
    });
    nodes
}

/// Generate operation method
///
/// Extracts native code from source using the body span
pub(crate) fn generate_operation(
    operation: &OperationAst,
    syntax: &super::backend::ClassSyntax,
    source: &[u8],
    system_name: &str,
    arcanum: &crate::frame_c::compiler::arcanum::Arcanum,
) -> Vec<CodegenNode> {
    let params: Vec<Param> = operation
        .params
        .iter()
        .map(|p| {
            let type_str = type_to_string(&p.param_type);
            Param::new(&p.name).with_type(&type_str)
        })
        .collect();

    // RFC-0046: lower `@@:self.*` in the operation body (non-static operations
    // can touch domain fields). Boundary-safe scanner pass; `@@:(expr)` /
    // `@@:system.state` are left for the textual pass below.
    let ctx = self_expansion_ctx(system_name, arcanum);
    let mut code = super::frame_expansion::expand_self_in_body(
        &operation.body.span,
        source,
        syntax.language,
        &ctx,
    );

    // Expand @@:system.state and @@:(expr) in operation bodies.
    // @@:system.state requires `self` (non-static only), but @@:(expr) works in both.
    code = super::frame_expansion::expand_system_state_in_code(&code, syntax.language);

    let mut nodes: Vec<CodegenNode> = operation
        .leading_comments
        .iter()
        .map(|c| CodegenNode::NativeBlock {
            code: c.clone(),
            span: None,
        })
        .collect();
    // Backend handles is_static flag for @staticmethod decorator
    nodes.push(CodegenNode::Method {
        name: operation.name.clone(),
        params,
        return_type: match &operation.return_type {
            crate::frame_c::compiler::frame_ast::Type::Unknown => None, // void
            t => Some(type_to_string(t)),
        },
        body: vec![CodegenNode::NativeBlock {
            code,
            span: Some(operation.body.span.clone()),
        }],
        is_async: false,
        is_static: operation.is_static,
        visibility: Visibility::Public,
        decorators: vec![],
    });
    nodes
}

/// Generate persistence methods (save_state, restore_state) for @@persist
pub(crate) fn generate_persistence_methods(
    system: &SystemAst,
    syntax: &super::backend::ClassSyntax,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();

    match syntax.language {
        TargetLanguage::Python3 => methods.extend(persist::python::generate(system)),
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            let is_ts = matches!(syntax.language, TargetLanguage::TypeScript);
            methods.extend(persist::javascript::generate(system, is_ts));
        }
        TargetLanguage::Rust => {
            methods.extend(super::rust_system::generate_rust_persistence_methods(
                system,
            ));
        }
        TargetLanguage::C => methods.extend(persist::c::generate(system)),
        TargetLanguage::Cpp => methods.extend(persist::cpp::generate(system)),
        TargetLanguage::Java => methods.extend(persist::java::generate(system)),
        TargetLanguage::CSharp => methods.extend(persist::csharp::generate(system)),
        TargetLanguage::Php => methods.extend(persist::php::generate(system)),
        TargetLanguage::Kotlin => methods.extend(persist::kotlin::generate(system)),
        TargetLanguage::Swift => methods.extend(persist::swift::generate(system)),
        TargetLanguage::Ruby => methods.extend(persist::ruby::generate(system)),
        TargetLanguage::Go => methods.extend(persist::go::generate(system)),
        TargetLanguage::Erlang => {
            // Persistence handled in erlang_system.rs via gen_statem save_state/load_state
        }
        TargetLanguage::Dart => methods.extend(persist::dart::generate(system)),
        TargetLanguage::Lua => methods.extend(persist::lua::generate(system)),
        TargetLanguage::GDScript => methods.extend(persist::gdscript::generate(system)),
        TargetLanguage::Graphviz => unreachable!(),
    }

    methods
}

// ============================================================================
// Erlang gen_statem code generation
// ============================================================================

#[cfg(test)]
mod derived_interface_order_tests {
    // The interface auto-derived from state handlers (no explicit `interface:`
    // section) must be emitted in source/declaration order, deterministically.
    // It used to iterate a HashSet (`events.into_iter()`), randomizing the
    // method order across runs — the residual of commit 72f3ea5 (deterministic
    // state + handler iteration) on this derive path.
    use crate::run;

    #[test]
    fn derived_interface_methods_in_source_order() {
        // `e1` declared before `e2`; no explicit `interface:` section.
        let src = "@@[target(\"csharp\")]\n\
                   @@system S {\n\
                   \x20   machine:\n\
                   \x20       $A => $P {\n\
                   \x20           e1() {{ => $^; }}\n\
                   \x20           e2() {{ => $^; }}\n\
                   \x20       }\n\
                   \x20       $P { }\n\
                   }\n";
        let out = run(src, "csharp");
        let p1 = out.find("public void e1(").expect("e1 method missing");
        let p2 = out.find("public void e2(").expect("e2 method missing");
        assert!(
            p1 < p2,
            "derived interface methods out of source order (e2 before e1):\n{out}"
        );
    }
}
