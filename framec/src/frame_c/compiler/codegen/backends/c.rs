//! C code generation backend

use crate::frame_c::compiler::codegen::ast::*;
use crate::frame_c::compiler::codegen::backend::*;
use crate::frame_c::visitors::TargetLanguage;

/// C backend for code generation
pub struct CBackend;

impl LanguageBackend for CBackend {
    fn emit(&self, node: &CodegenNode, ctx: &mut EmitContext) -> String {
        let system_name = ctx.system_name.clone().unwrap_or_default();

        match node {
            CodegenNode::Module { imports, items } => {
                let mut result = String::new();
                for import in imports {
                    result.push_str(&self.emit(import, ctx));
                    result.push('\n');
                }
                if !imports.is_empty() && !items.is_empty() {
                    result.push('\n');
                }
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        result.push_str("\n\n");
                    }
                    result.push_str(&self.emit(item, ctx));
                }
                result
            }

            CodegenNode::Import { module, .. } => format!("#include <{}.h>", module),

            CodegenNode::Class {
                name,
                fields,
                methods,
                input,
                ..
            } => {
                ctx.input = input.clone();
                // RFC-0056 P9 (#209): a system declaring an alphabet-typed header
                // param BORROWS its input. The adapter holds the caller's buffer
                // by reference — zero copy.
                let mut input_adapter_src = String::new();
                if let Some(spec) = input {
                    input_adapter_src =
                        crate::frame_c::compiler::codegen::codegen_utils::input_adapter(
                            TargetLanguage::C,
                            name,
                            &spec.elem,
                        );
                }
                let mut result = String::new();
                result.push_str(&input_adapter_src);

                // Forward declarations for the struct and functions
                result.push_str(&format!("// Forward declarations\n"));
                result.push_str(&format!("typedef struct {} {};\n", name, name));
                result.push_str(&format!(
                    "static void {}_kernel({}* self, {}_FrameEvent* __e);\n",
                    name, name, name
                ));
                result.push_str(&format!(
                    "static void {}_router({}* self, {}_FrameEvent* __e);\n",
                    name, name, name
                ));
                result.push_str(&format!(
                    "static void {}_transition({}* self, {}_Compartment* next);\n",
                    name, name, name
                ));
                // Cascade helper forward declarations (HSM cascade
                // architecture per docs/frame_runtime.md Step 21+).
                result.push_str(&format!(
                    "static int {}_hsm_chain({}* self, const char* leaf, const char*** out_chain);\n",
                    name, name
                ));
                result.push_str(&format!(
                    "static {}_Compartment* {}_prepareEnter({}* self, const char* leaf, {}_FrameVec* state_args, {}_FrameVec* enter_args);\n",
                    name, name, name, name, name
                ));
                result.push_str(&format!(
                    "static void {}_prepareExit({}* self, {}_FrameVec* exit_args);\n",
                    name, name, name
                ));
                // RFC-0020: __route_to_state inlined into __router;
                // __process_transition_loop inlined into __kernel.
                // No forward decls needed for either.

                // Add forward declarations for state handler methods
                // AND per-handler methods (`_s_<State>_hdl_<kind>_<event>`).
                // Per-handler architecture adds `compartment` as a third arg
                // (see docs/frame_runtime.md § "Dispatch Model").
                for method in methods {
                    if let CodegenNode::Method {
                        name: method_name, ..
                    } = method
                    {
                        if method_name.starts_with("_state_") || method_name.starts_with("_s_") {
                            result.push_str(&format!(
                                "static void {}_{}({}* self, {}_FrameEvent* __e, {}_Compartment* compartment);\n",
                                name,
                                method_name.trim_start_matches('_'),
                                name,
                                name,
                                name
                            ));
                        }
                    }
                }

                // Add forward declarations for actions and operations
                for method in methods {
                    if let CodegenNode::Method {
                        name: method_name,
                        params,
                        return_type,
                        is_static,
                        ..
                    } = method
                    {
                        // Skip state handlers, per-handler methods, kernel,
                        // router, transition (already declared).
                        if method_name.starts_with("_state_")
                            || method_name.starts_with("_s_")
                            || method_name.starts_with("__")
                            || method_name == "new"
                            || method_name == "destroy"
                        {
                            continue;
                        }
                        // Skip interface methods (they get public declarations)
                        // Actions/Operations are not interface methods - they're internal
                        // Check if method is an action or operation by visibility and not being interface
                        let return_str = if return_type.is_none() {
                            "void".to_string()
                        } else {
                            self.convert_type_to_c(return_type, &system_name)
                        };
                        let params_str =
                            self.emit_params_with_self(params, ctx, !*is_static, &system_name);
                        // User-named actions/operations starting with `_`
                        // are emitted `static` in the function definition
                        // (see the Method arm below). The forward declaration
                        // must match — otherwise the C compiler reports
                        // `static declaration follows non-static declaration`.
                        let static_kw = if *is_static || method_name.starts_with('_') {
                            "static "
                        } else {
                            ""
                        };
                        result.push_str(&format!(
                            "{}{} {}_{} ({});\n",
                            static_kw, return_str, name, method_name, params_str
                        ));
                    }
                }
                result.push('\n');

                // Struct definition
                // C struct fields are emitted from parsed field info (name + type).
                result.push_str(&format!("{}struct {} {{\n", ctx.get_indent(), name));
                ctx.push_indent();
                for field in fields {
                    // Cross-system domain reference (`inner: Counter
                    // = @@Counter()`): the assembler emits
                    // `Counter_new()` which returns `Counter*`, so
                    // the field has to be a pointer for the assignment
                    // to type-check. Same shape as the Go fix —
                    // recognized via `ctx.defined_systems`.
                    let raw_type = field.type_annotation.as_deref().unwrap_or("");
                    // Array-typed domain field (`counters: Counter*[4]`, #159):
                    // C's declarator puts the bracket group after the NAME
                    // (`Counter* counters[4];`), so split a trailing `[..]`
                    // off the declared type and emit it there. This makes the
                    // element system visible to the indexed cross-system call
                    // resolution (rule 1 — structural), instead of forcing a
                    // native typedef that hides it.
                    let (elem_type, brackets) = match raw_type.find('[') {
                        Some(b) if raw_type.ends_with(']') => {
                            (raw_type[..b].trim_end(), &raw_type[b..])
                        }
                        _ => (raw_type, ""),
                    };
                    let c_type = if ctx.defined_systems.contains(elem_type) {
                        format!("{}*", elem_type)
                    } else if brackets.is_empty() {
                        self.convert_type_to_c(&field.type_annotation, &system_name)
                    } else {
                        self.convert_type_to_c(&Some(elem_type.to_string()), &system_name)
                    };
                    result.push_str(&format!(
                        "{}{} {}{};\n",
                        ctx.get_indent(),
                        c_type,
                        field.name,
                        brackets
                    ));
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}};\n\n", ctx.get_indent()));

                // Function declarations and definitions
                for method in methods {
                    result.push_str(&self.emit(method, ctx));
                    result.push('\n');
                }

                // RFC-0046 d-cross: a cross-system call `@@:self.inner.bump()`
                // is emitted directly as `Inner_bump(self->inner)` by the
                // kind-15 segment expansion (frame_expansion::context_self), so
                // the former textual `rewrite_c_cross_system_calls` post-pass is
                // gone. (Bare native `self.inner.bump()` is passthrough and is
                // the author's error on C, which has no `self`.)
                result
            }

            CodegenNode::Enum { name, variants } => {
                let mut result = format!("{}typedef enum {{\n", ctx.get_indent());
                ctx.push_indent();
                for (i, variant) in variants.iter().enumerate() {
                    let comma = if i < variants.len() - 1 { "," } else { "" };
                    result.push_str(&format!(
                        "{}{}_{}{}\n",
                        ctx.get_indent(),
                        name,
                        variant.name,
                        comma
                    ));
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}} {};\n", ctx.get_indent(), name));
                result
            }

            CodegenNode::Method {
                name,
                params,
                return_type,
                body,
                is_static,
                ..
            } => {
                // Convert return type - but for Frame machinery methods with no return type, use void not void*
                let return_str = if return_type.is_none() {
                    "void".to_string()
                } else {
                    self.convert_type_to_c(return_type, &system_name)
                };

                // For Frame system methods, add self parameter
                let is_frame_method = !*is_static && !system_name.is_empty();
                let params_str =
                    self.emit_params_with_self(params, ctx, is_frame_method, &system_name);

                // Method name - prefix with system name for ALL methods in Frame systems
                let func_name = if !system_name.is_empty() {
                    if name.starts_with("__") {
                        // Private methods like __kernel, __router -> System_kernel
                        format!("{}_{}", system_name, name.trim_start_matches('_'))
                    } else if name.starts_with("_state_") || name.starts_with("_s_") {
                        // State handlers like _state_Start -> System_state_Start.
                        // Per-handler emission uses `_s_*`. Both shapes match
                        // the forward-declaration loop that already strips `_`.
                        format!("{}_{}", system_name, name.trim_start_matches('_'))
                    } else {
                        // Public methods, and user-named private methods
                        // (e.g. action `_read`). The leading `_` is part of
                        // the user's name and must be preserved so the
                        // forward declaration (which uses `name` verbatim)
                        // matches the function definition at link time.
                        format!("{}_{}", system_name, name)
                    }
                } else {
                    name.clone()
                };

                let static_kw = if *is_static || name.starts_with("_") {
                    "static "
                } else {
                    ""
                };
                let mut result = format!(
                    "{}{}{} {}({}) {{\n",
                    ctx.get_indent(),
                    static_kw,
                    return_str,
                    func_name,
                    params_str
                );
                ctx.push_indent();

                for stmt in body {
                    let stmt_str = self.emit(stmt, ctx);
                    result.push_str(&stmt_str);
                    // Add semicolon if needed
                    if !stmt_str.trim().is_empty()
                        && !stmt_str.trim().ends_with('}')
                        && !stmt_str.trim().ends_with(';')
                        && !matches!(
                            stmt,
                            CodegenNode::If { .. }
                                | CodegenNode::While { .. }
                                | CodegenNode::For { .. }
                                | CodegenNode::Match { .. }
                                | CodegenNode::Comment { .. }
                                | CodegenNode::NativeBlock { .. }
                                | CodegenNode::FrameInitBlock { .. }
                                | CodegenNode::Empty
                        )
                    {
                        result.push_str(";\n");
                    } else if !stmt_str.trim().is_empty() {
                        result.push('\n');
                    }
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));
                result
            }

            CodegenNode::Constructor { params, body, .. } => {
                // RFC-0020: the C system constructor emits two artifacts
                // (was three under RFC-0017):
                //   Counter* Counter_new(void)         — bare framework; IS @@!Counter()
                //   Counter* Counter_create(...)       — factory + start-$>; IS @@Counter(args)
                //
                // The intermediate `Counter_frame_init` that RFC-0017
                // emitted is gone — its body is absorbed inline into
                // `Counter_create` after the `Counter_new()` call.
                //
                // Call-site lowering:
                //   - `@@Counter(7)` → `Counter_create(7)`
                //   - `@@!Counter()` → `Counter_new()`
                //
                // Body classification by LINE (rendered text), since C's
                // compartment setup is a multi-line NativeBlock:
                //   - Lines containing cascade triggers (`__kernel(...)`,
                //     `__fire_*_cascade`) → `_create` only (skipped in bare).
                //   - Lines mentioning any ctor param name → `_create`
                //     only (skipped in bare).
                //   - Other lines → BOTH (bare gets framework setup;
                //     `_create` re-runs them with full args so it can
                //     rebuild the compartment with the user's enter_args).
                // The double-set of state_stack / compartment etc. is
                // harmless — the second assignment replaces the first.
                let class_name = system_name.clone();

                // Render body to text WITH function-body indent + semicolons
                // applied (matching the original Constructor arm's logic).
                // `bare_text` additionally excludes the start-state `$>`
                // kernel-dispatch statement — classified STRUCTURALLY by the
                // `FrameInitBlock` marker node (#152/#123), not by scanning
                // rendered lines for `_kernel(`. This also drops that block's
                // sibling event/context lines from the bare allocator, where
                // they were dead weight (an event created, a context
                // pushed/popped/destroyed, no dispatch).
                // #123: route each constructor statement by node identity, not by
                // scanning rendered text for param names. The factory
                // (`Sys_create`) gets everything except the bare-only compartment
                // form; the bare allocator (`Sys_new`) gets the shared statements
                // plus the empty-args compartment (`BareCtorBlock`), excluding the
                // start-`$>` kernel dispatch (`FrameInitBlock`) and any
                // param-referencing statement (`FactoryOnlyBlock`).
                ctx.push_indent();
                let body_indent = ctx.get_indent();
                let mut body_text = String::new();
                let mut bare_body = String::new();
                for stmt in body {
                    let s = self.emit(stmt, ctx);
                    let trimmed = s.trim();
                    let mut rendered = s.clone();
                    if !trimmed.is_empty()
                        && !trimmed.ends_with('}')
                        && !trimmed.ends_with(';')
                        && !matches!(
                            stmt,
                            CodegenNode::If { .. }
                                | CodegenNode::While { .. }
                                | CodegenNode::Comment { .. }
                                | CodegenNode::Empty
                        )
                    {
                        rendered.push_str(";\n");
                    } else if !trimmed.is_empty() && !s.ends_with('\n') {
                        rendered.push('\n');
                    }
                    if !matches!(stmt, CodegenNode::BareCtorBlock { .. }) {
                        body_text.push_str(&rendered);
                    }
                    if !matches!(
                        stmt,
                        CodegenNode::FrameInitBlock { .. } | CodegenNode::FactoryOnlyBlock { .. }
                    ) {
                        bare_body.push_str(&rendered);
                    }
                }
                ctx.pop_indent();

                // Emit `Counter* Counter_new(void)` — bare framework
                let mut result = format!(
                    "{}{}* {}_new(void) {{\n",
                    ctx.get_indent(),
                    class_name,
                    class_name
                );
                result.push_str(&format!(
                    "{}{}* self = calloc(1, sizeof({}));\n",
                    body_indent, class_name, class_name
                ));
                result.push_str(&bare_body);
                result.push_str(&format!("{}return self;\n", body_indent));
                result.push_str(&format!("{}}}\n", ctx.get_indent()));

                // Emit `Counter* Counter_create(<params>)` — factory +
                // start-$>. Per RFC-0020 the body that used to live
                // in `Counter_frame_init` is absorbed inline here. C
                // already uses `self->` throughout, so no rewrite is
                // needed — the local var `self` is the
                // newly-allocated instance, exactly what `body_text`
                // expects.
                let create_params = if params.is_empty() {
                    "void".to_string()
                } else {
                    params
                        .iter()
                        .map(|p| {
                            let type_str = self.convert_type_to_c(&p.type_annotation, &class_name);
                            format!("{} {}", type_str, p.name)
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                result.push('\n');
                result.push_str(&format!(
                    "{}{}* {}_create({}) {{\n",
                    ctx.get_indent(),
                    class_name,
                    class_name,
                    create_params
                ));
                ctx.push_indent();
                result.push_str(&format!(
                    "{}{}* self = {}_new();\n",
                    ctx.get_indent(),
                    class_name,
                    class_name
                ));
                result.push_str(&body_text);
                result.push_str(&format!("{}return self;\n", ctx.get_indent()));
                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));

                result
            }

            CodegenNode::VarDecl {
                name,
                type_annotation,
                init,
                is_const,
            } => {
                let type_str = self.convert_type_to_c(type_annotation, &system_name);
                let const_kw = if *is_const { "const " } else { "" };
                if let Some(init_expr) = init {
                    format!(
                        "{}{}{} {} = {}",
                        ctx.get_indent(),
                        const_kw,
                        type_str,
                        name,
                        self.emit(init_expr, ctx)
                    )
                } else {
                    format!("{}{}{} {}", ctx.get_indent(), const_kw, type_str, name)
                }
            }

            CodegenNode::Assignment { target, value } => {
                format!(
                    "{}{} = {}",
                    ctx.get_indent(),
                    self.emit(target, ctx),
                    self.emit(value, ctx)
                )
            }

            CodegenNode::Return { value } => {
                if let Some(val) = value {
                    format!("{}return {}", ctx.get_indent(), self.emit(val, ctx))
                } else {
                    format!("{}return", ctx.get_indent())
                }
            }

            CodegenNode::If {
                condition,
                then_block,
                else_block,
            } => {
                let mut result = format!(
                    "{}if ({}) {{\n",
                    ctx.get_indent(),
                    self.emit(condition, ctx)
                );
                ctx.push_indent();
                for stmt in then_block {
                    let s = self.emit(stmt, ctx);
                    result.push_str(&s);
                    if !s.trim().is_empty() && !s.trim().ends_with('}') && !s.trim().ends_with(';')
                    {
                        result.push_str(";\n");
                    } else if !s.trim().is_empty() {
                        result.push('\n');
                    }
                }
                ctx.pop_indent();

                if let Some(else_stmts) = else_block {
                    result.push_str(&format!("{}}} else {{\n", ctx.get_indent()));
                    ctx.push_indent();
                    for stmt in else_stmts {
                        let s = self.emit(stmt, ctx);
                        result.push_str(&s);
                        if !s.trim().is_empty()
                            && !s.trim().ends_with('}')
                            && !s.trim().ends_with(';')
                        {
                            result.push_str(";\n");
                        } else if !s.trim().is_empty() {
                            result.push('\n');
                        }
                    }
                    ctx.pop_indent();
                }
                result.push_str(&format!("{}}}", ctx.get_indent()));
                result
            }

            CodegenNode::Match { scrutinee, arms } => {
                // For string comparison, use if-else chain instead of switch.
                // Classified STRUCTURALLY by the arm patterns (#123): a
                // string-literal pattern needs `strcmp`, not `switch` — the
                // old check scanned the rendered scrutinee text for
                // `_message`/`state` substrings.
                let scrutinee_str = self.emit(scrutinee, ctx);
                let is_string_match = arms.iter().any(|arm| {
                    matches!(
                        arm.pattern.as_ref(),
                        CodegenNode::Literal(
                            crate::frame_c::compiler::codegen::ast::Literal::String(_)
                        )
                    )
                });

                if is_string_match {
                    let mut result = String::new();
                    for (i, arm) in arms.iter().enumerate() {
                        let cond = if i == 0 { "if" } else { "} else if" };
                        let pattern_str = self.emit(&arm.pattern, ctx);
                        result.push_str(&format!(
                            "{}{} (strcmp({}, {}) == 0) {{\n",
                            ctx.get_indent(),
                            cond,
                            scrutinee_str,
                            pattern_str
                        ));
                        ctx.push_indent();
                        for stmt in &arm.body {
                            let s = self.emit(stmt, ctx);
                            result.push_str(&s);
                            if !s.trim().is_empty()
                                && !s.trim().ends_with('}')
                                && !s.trim().ends_with(';')
                            {
                                result.push_str(";\n");
                            } else if !s.trim().is_empty() {
                                result.push('\n');
                            }
                        }
                        ctx.pop_indent();
                    }
                    result.push_str(&format!("{}}}", ctx.get_indent()));
                    result
                } else {
                    let mut result = format!("{}switch ({}) {{\n", ctx.get_indent(), scrutinee_str);
                    ctx.push_indent();
                    for arm in arms {
                        result.push_str(&format!(
                            "{}case {}:\n",
                            ctx.get_indent(),
                            self.emit(&arm.pattern, ctx)
                        ));
                        ctx.push_indent();
                        for stmt in &arm.body {
                            let s = self.emit(stmt, ctx);
                            result.push_str(&s);
                            if !s.trim().is_empty()
                                && !s.trim().ends_with('}')
                                && !s.trim().ends_with(';')
                            {
                                result.push_str(";\n");
                            } else if !s.trim().is_empty() {
                                result.push('\n');
                            }
                        }
                        result.push_str(&format!("{}break;\n", ctx.get_indent()));
                        ctx.pop_indent();
                    }
                    ctx.pop_indent();
                    result.push_str(&format!("{}}}", ctx.get_indent()));
                    result
                }
            }

            CodegenNode::While { condition, body } => {
                let mut result = format!(
                    "{}while ({}) {{\n",
                    ctx.get_indent(),
                    self.emit(condition, ctx)
                );
                ctx.push_indent();
                for stmt in body {
                    let s = self.emit(stmt, ctx);
                    result.push_str(&s);
                    if !s.trim().is_empty() && !s.trim().ends_with('}') && !s.trim().ends_with(';')
                    {
                        result.push_str(";\n");
                    } else if !s.trim().is_empty() {
                        result.push('\n');
                    }
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}", ctx.get_indent()));
                result
            }

            CodegenNode::For {
                var,
                iterable,
                body: _,
            } => {
                // C doesn't have for-each, generate a comment
                let mut result = format!(
                    "{}/* for {} in {} */\n",
                    ctx.get_indent(),
                    var,
                    self.emit(iterable, ctx)
                );
                result.push_str(&format!(
                    "{}/* C ForEach: not reachable — Frame uses native passthrough for loops */",
                    ctx.get_indent()
                ));
                result
            }

            CodegenNode::Break => format!("{}break", ctx.get_indent()),
            CodegenNode::Continue => format!("{}continue", ctx.get_indent()),
            CodegenNode::ExprStmt(expr) => format!("{}{}", ctx.get_indent(), self.emit(expr, ctx)),
            CodegenNode::Await(expr) => self.emit(expr, ctx),
            CodegenNode::Comment { text, .. } => format!("{}/* {} */", ctx.get_indent(), text),
            CodegenNode::Empty => String::new(),

            CodegenNode::Ident(name) => name.clone(),
            CodegenNode::Literal(lit) => self.emit_literal(lit, ctx),
            CodegenNode::BinaryOp { op, left, right } => self.emit_binary_op(op, left, right, ctx),
            CodegenNode::UnaryOp { op, operand } => self.emit_unary_op(op, operand, ctx),

            CodegenNode::Call { target, args } => {
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();
                format!("{}({})", self.emit(target, ctx), args_str.join(", "))
            }

            CodegenNode::MethodCall {
                object,
                method,
                args,
            } => {
                // Convert method calls to C function calls
                let obj_str = self.emit(object, ctx);
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();

                // Special handling for common patterns
                if method == "push" || method == "append" {
                    // Convert to FrameVec_push
                    if args_str.is_empty() {
                        format!("{}_FrameVec_push({})", system_name, obj_str)
                    } else {
                        format!(
                            "{}_FrameVec_push({}, {})",
                            system_name,
                            obj_str,
                            args_str.join(", ")
                        )
                    }
                } else if method == "pop" {
                    format!("{}_FrameVec_pop({})", system_name, obj_str)
                } else if method == "copy" {
                    // Compartment copy
                    format!("{}_Compartment_copy({})", system_name, obj_str)
                } else if method == "get" {
                    // Dict get
                    format!(
                        "{}_FrameDict_get({}, {})",
                        system_name,
                        obj_str,
                        args_str.join(", ")
                    )
                } else {
                    // General method call -> function call with object as first arg
                    let all_args = if args_str.is_empty() {
                        obj_str
                    } else {
                        format!("{}, {}", obj_str, args_str.join(", "))
                    };
                    format!("{}({})", method, all_args)
                }
            }

            CodegenNode::FieldAccess { object, field } => {
                let obj_str = self.emit(object, ctx);
                // `self` is a pointer (`Sensor *self`) — decided structurally
                // from the node kind. A chained access is a pointer when it
                // carries a `->` in *code* (not inside a string literal): the
                // `->` test now skips literals/comments (#155), so a native
                // string like `arr["a->b"]` no longer forces `->`.
                let is_pointer = matches!(**object, CodegenNode::SelfRef)
                    || obj_str == "self"
                    || crate::frame_c::compiler::codegen::codegen_utils::find_outside_strings_and_comments(
                        &obj_str,
                        crate::frame_c::visitors::TargetLanguage::C,
                        "->",
                    )
                    .is_some();
                if is_pointer {
                    format!("{}->{}", obj_str, field)
                } else {
                    format!("{}.{}", obj_str, field)
                }
            }

            CodegenNode::IndexAccess { object, index } => {
                format!("{}[{}]", self.emit(object, ctx), self.emit(index, ctx))
            }
            CodegenNode::SelfRef => "self".to_string(),

            CodegenNode::Array(elements) => {
                if elements.is_empty() {
                    // Empty array initialization - in C we'd initialize to NULL/0
                    "NULL".to_string()
                } else {
                    let elems: Vec<String> = elements.iter().map(|e| self.emit(e, ctx)).collect();
                    format!("{{ {} }}", elems.join(", "))
                }
            }

            CodegenNode::Dict(_) => {
                // Create a new FrameDict
                format!("{}_FrameDict_new()", system_name)
            }

            CodegenNode::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                format!(
                    "({}) ? ({}) : ({})",
                    self.emit(condition, ctx),
                    self.emit(then_expr, ctx),
                    self.emit(else_expr, ctx)
                )
            }

            CodegenNode::Lambda { .. } => "/* Lambda not supported in C */".to_string(),
            CodegenNode::Cast { expr, target_type } => {
                format!("({})({})", target_type, self.emit(expr, ctx))
            }

            CodegenNode::New { class, args } => {
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();
                // Convert to C constructor call
                let c_class = if class.contains("Compartment") {
                    format!("{}_Compartment", system_name)
                } else if class.contains("FrameEvent") {
                    format!("{}_FrameEvent", system_name)
                } else if class.contains("FrameContext") {
                    format!("{}_FrameContext", system_name)
                } else {
                    class.clone()
                };
                format!("{}_new({})", c_class, args_str.join(", "))
            }

            // Frame-specific
            CodegenNode::Transition {
                target_state,
                indent,
                ..
            } => {
                let ind = " ".repeat(*indent);
                format!(
                    "{}{}{}_transition(self, {}_Compartment_new(\"{}\"))",
                    ctx.get_indent(),
                    ind,
                    system_name,
                    system_name,
                    target_state
                )
            }
            CodegenNode::ChangeState {
                target_state,
                indent,
                ..
            } => {
                let ind = " ".repeat(*indent);
                format!(
                    "{}{}/* change_state to {} */",
                    ctx.get_indent(),
                    ind,
                    target_state
                )
            }
            CodegenNode::StackPush { indent } => {
                let ind = " ".repeat(*indent);
                format!("{}{}{}_FrameVec_push(self->_state_stack, {}_Compartment_copy(self->__compartment))",
                    ctx.get_indent(), ind, system_name, system_name)
            }
            CodegenNode::StackPop { indent } => {
                let ind = " ".repeat(*indent);
                format!(
                    "{}{}{}_FrameVec_pop(self->_state_stack)",
                    ctx.get_indent(),
                    ind,
                    system_name
                )
            }
            CodegenNode::StateContext { state_name } => {
                format!("/* state context for {} */", state_name)
            }

            CodegenNode::SendEvent { event, args } => {
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();
                if args_str.is_empty() {
                    format!("{}{}(self)", ctx.get_indent(), event)
                } else {
                    format!(
                        "{}{}(self, {})",
                        ctx.get_indent(),
                        event,
                        args_str.join(", ")
                    )
                }
            }

            CodegenNode::NativeBlock { code, .. }
            | CodegenNode::FrameInitBlock { code, .. }
            | CodegenNode::FactoryOnlyBlock { code, .. }
            | CodegenNode::BareCtorBlock { code, .. } => {
                let indent = ctx.get_indent();
                code.lines()
                    .map(|line| {
                        if line.trim().is_empty() {
                            String::new()
                        } else {
                            format!("{}{}", indent, line)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            CodegenNode::SplicePoint { id } => format!("/* SPLICE_POINT: {} */", id),
        }
    }

    fn emit_module_imports(
        &self,
        imports: &[crate::frame_c::compiler::frame_ast::Import],
    ) -> Vec<String> {
        // RFC-0022 Phase 1 lax — `#include "x.h"`. C headers expose
        // struct + function declarations file-globally after include.
        // Caller is responsible for providing a matching `.h` file
        // (framec currently emits `.c` only; a header-generation
        // mode is a separate concern).
        imports
            .iter()
            .filter_map(|imp| {
                let path = imp.module.as_str();
                if path.is_empty() {
                    return None;
                }
                let stem = match path.rfind('.') {
                    Some(idx) => &path[..idx],
                    None => path,
                };
                Some(format!("#include \"{}.h\"", stem))
            })
            .collect()
    }

    fn runtime_imports(&self) -> Vec<String> {
        // Runtime imports are now included in generate_c_compartment_types
        vec![]
    }

    fn class_syntax(&self) -> ClassSyntax {
        ClassSyntax::c()
    }
    fn target_language(&self) -> TargetLanguage {
        TargetLanguage::C
    }

    fn null_keyword(&self) -> &'static str {
        "NULL"
    }
    fn true_keyword(&self) -> &'static str {
        "true"
    }
    fn false_keyword(&self) -> &'static str {
        "false"
    }
}

impl CBackend {
    fn emit_params(&self, params: &[Param], _ctx: &EmitContext) -> String {
        if params.is_empty() {
            "void".to_string()
        } else {
            params
                .iter()
                .map(|p| {
                    let type_ann = p
                        .type_annotation
                        .as_ref()
                        .unwrap_or(&"int".to_string())
                        .clone();
                    format!("{} {}", type_ann, p.name)
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn emit_params_with_self(
        &self,
        params: &[Param],
        _ctx: &EmitContext,
        add_self: bool,
        system_name: &str,
    ) -> String {
        let mut result = Vec::new();

        if add_self && !system_name.is_empty() {
            result.push(format!("{}* self", system_name));
        }

        for p in params {
            let type_str = self.convert_type_to_c(&p.type_annotation, system_name);
            result.push(format!("{} {}", type_str, p.name));
        }

        if result.is_empty() {
            "void".to_string()
        } else {
            result.join(", ")
        }
    }

    /// Convert a Frame type annotation to a C type.
    ///
    /// Frame has no type system: type names pass through VERBATIM. The
    /// name-alias table (str→char*, int→int, Any→int, float→double, …) was
    /// exterminated (FRAMEC_BUGS #37) — write C's own type names (`int`,
    /// `char*`, `double`, …). What remains here is purely STRUCTURAL: the
    /// no-type / no-return spellings, C's runtime container ABI (Frame
    /// list/dict have no native C type — they're the generated FrameVec /
    /// FrameDict), framework pointer types, and the `T | null` optional form.
    fn convert_type_to_c(&self, type_ann: &Option<String>, system_name: &str) -> String {
        match type_ann.as_ref().map(|s| s.as_str()) {
            None => "void*".to_string(),
            Some("void") | Some("None") => "void".to_string(),
            Some("list") | Some("List") | Some("Array") | Some("Array<any>") => {
                format!("{}_FrameVec*", system_name)
            }
            Some("dict") | Some("Dict") | Some("Record<string, any>") => {
                format!("{}_FrameDict*", system_name)
            }
            Some(t) if t.contains("Compartment") => {
                format!("{}_Compartment*", system_name)
            }
            Some(t) if t.contains("FrameEvent") => {
                format!("{}_FrameEvent*", system_name)
            }
            Some(t) if t.contains("FrameContext") => {
                format!("{}_FrameContext*", system_name)
            }
            Some(t) if t.ends_with("| null") || t.ends_with("| None") => {
                // Optional type - just use the base type (will be pointer)
                let base = match t.split('|').next() {
                    Some(b) => b.trim(),
                    None => t.trim(),
                };
                self.convert_type_to_c(&Some(base.to_string()), system_name)
            }
            Some(other) => other.to_string(),
        }
    }
}
