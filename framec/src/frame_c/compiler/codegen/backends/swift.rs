//! Swift code generation backend

use crate::frame_c::compiler::codegen::ast::*;
use crate::frame_c::compiler::codegen::backend::*;
use crate::frame_c::visitors::TargetLanguage;

/// Swift backend for code generation
pub struct SwiftBackend;

impl LanguageBackend for SwiftBackend {
    fn emit(&self, node: &CodegenNode, ctx: &mut EmitContext) -> String {
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

            CodegenNode::Import { module, items, .. } => {
                if items.is_empty() {
                    format!("import {}", module)
                } else {
                    // Swift doesn't have selective imports, just import the module
                    format!("import {}", module)
                }
            }

            CodegenNode::Class {
                is_framework_helper,
                name,
                fields,
                methods,
                base_classes,
                is_abstract,
                visibility,
                ..
            } => {
                ctx.is_framework_helper = *is_framework_helper;
                let mut result = String::new();
                let vis_kw = match visibility {
                    Visibility::Public => "public ",
                    _ => "",
                };
                // Swift doesn't have abstract classes, but we can note it
                let _abstract_kw = if *is_abstract { "/* abstract */ " } else { "" };
                let extends = if base_classes.is_empty() {
                    String::new()
                } else {
                    format!(": {}", base_classes[0])
                };

                result.push_str(&format!(
                    "{}{}class {}{} {{\n",
                    ctx.get_indent(),
                    vis_kw,
                    name,
                    extends
                ));
                ctx.push_indent();

                for field in fields {
                    result.push_str(&self.emit_field(field, ctx));
                }
                if !fields.is_empty() && !methods.is_empty() {
                    result.push('\n');
                }

                // Temporarily expose this class's name as `system_name` so
                // the Constructor arm can distinguish system vs framework
                // helper classes (FrameEvent / FrameContext / Compartment)
                // and apply the RFC-0017 init-decouple split only to the
                // system class.
                let prev_system = ctx.system_name.clone();
                ctx.system_name = Some(name.clone());
                for (i, method) in methods.iter().enumerate() {
                    if i > 0 {
                        result.push('\n');
                    }
                    result.push_str(&self.emit(method, ctx));
                }
                ctx.system_name = prev_system;

                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));
                result
            }

            CodegenNode::Enum { name, variants } => {
                let mut result = format!("{}enum {} {{\n", ctx.get_indent(), name);
                ctx.push_indent();
                for variant in variants {
                    result.push_str(&format!("{}case {}\n", ctx.get_indent(), variant.name));
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));
                result
            }

            CodegenNode::Method {
                name,
                params,
                return_type,
                body,
                is_async,
                is_static,
                visibility,
                decorators,
                ..
            } => {
                let vis = self.emit_visibility_swift(*visibility);
                let vis_prefix = if vis.is_empty() {
                    String::new()
                } else {
                    format!("{} ", vis)
                };
                let params_str = self.emit_params(params);

                // Swift uses "func" keyword, return type after params with " -> "
                let return_str = return_type
                    .as_ref()
                    .filter(|t| t.as_str() != "void" && t.as_str() != "Void")
                    .map(|t| format!(" -> {}", self.map_type(t)))
                    .unwrap_or_default();

                let static_kw = if *is_static { "static " } else { "" };
                // Swift: `async` goes between the param list and return type.
                //   func foo() async -> Int { ... }
                let async_kw = if *is_async { " async" } else { "" };
                // RFC-0043 D2: the casing's gated interface wrappers carry a
                // `__swift_throws__` decorator marker, which we expand here
                // into a `throws` keyword between `async` and the return
                // type. The marker is a side-channel from
                // `system_codegen::casing.rs` because adding a `throws: bool`
                // field to the shared `Method` node would touch every
                // backend and 190+ construction sites for a Swift-only
                // signature element.
                let throws_kw = if decorators.iter().any(|d| d == "__swift_throws__") {
                    " throws"
                } else {
                    ""
                };
                let mut result = format!(
                    "{}{}{}func {}({}){}{}{} {{\n",
                    ctx.get_indent(),
                    vis_prefix,
                    static_kw,
                    name,
                    params_str,
                    async_kw,
                    throws_kw,
                    return_str
                );

                ctx.push_indent();
                for stmt in body {
                    result.push_str(&self.emit(stmt, ctx));
                    // Swift: no semicolons
                    result.push('\n');
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));
                result
            }

            CodegenNode::Constructor {
                params,
                body,
                super_call,
            } => {
                // RFC-0017 Phase A2: split the system constructor into:
                //   init()                              — bare framework
                //   func __frame_init(<params>)         — user $> + cascade
                //   static func __create(<params>) -> Self — factory
                //
                // Framework helper classes (FrameEvent / FrameContext /
                // Compartment) keep the original single-`init` emission.
                let class_name = ctx.system_name.clone().unwrap_or_default();
                let is_frame_helper = ctx.is_framework_helper;

                if is_frame_helper {
                    let params_str = self.emit_params(params);
                    let mut result = format!("{}init({}) {{\n", ctx.get_indent(), params_str);
                    ctx.push_indent();
                    if let Some(sc) = super_call {
                        result.push_str(&format!("{}{}\n", ctx.get_indent(), self.emit(sc, ctx)));
                    }
                    for stmt in body {
                        result.push_str(&self.emit(stmt, ctx));
                        result.push('\n');
                    }
                    ctx.pop_indent();
                    result.push_str(&format!("{}}}\n", ctx.get_indent()));
                    return result;
                }

                // System class: classify body items.
                let param_names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                ctx.push_indent();
                let mut framework_lines: Vec<String> = Vec::new();
                let mut frame_init_lines: Vec<String> = Vec::new();
                if let Some(sc) = super_call {
                    framework_lines.push(format!("{}{}", ctx.get_indent(), self.emit(sc, ctx)));
                }
                for stmt in body {
                    let rendered = self.emit(stmt, ctx);
                    // RFC-0020: scope to kernel call + context-stack
                    // mutation. Compartment-init lines (`__compartment =
                    // __prepareEnter(...)`) must still run in `init()` so
                    // `@@!Foo()` shells are usable. Bare `_context_stack`
                    // initialization stays in `init()`; only push/pop go
                    // to `__frame_init`.
                    // #123: route by node identity, not a param-name text scan.
                    // Factory-only statements (kernel dispatch, full-args
                    // compartment, param assigns) go to the frame_init cascade;
                    // the bare ctor gets the shared statements plus the empty-args
                    // compartment (BareCtorBlock). This retires the string-blind
                    // `mentions_param` scan and the `*_strip_param_lists` walk.
                    if matches!(
                        stmt,
                        CodegenNode::FrameInitBlock { .. } | CodegenNode::FactoryOnlyBlock { .. }
                    ) {
                        frame_init_lines.push(rendered);
                    } else {
                        framework_lines.push(rendered);
                    }
                }
                ctx.pop_indent();

                // Emit `init()` — bare framework
                let mut result = format!("{}init() {{\n", ctx.get_indent());
                ctx.push_indent();
                for line in &framework_lines {
                    result.push_str(line);
                    if !line.ends_with('\n') {
                        result.push('\n');
                    }
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));

                // Emit `func __frame_init(<params>)`
                result.push('\n');
                let frame_init_params = self.emit_params(params);
                result.push_str(&format!(
                    "{}func __frame_init({}) {{\n",
                    ctx.get_indent(),
                    frame_init_params
                ));
                ctx.push_indent();
                for line in &frame_init_lines {
                    result.push_str(line);
                    if !line.ends_with('\n') {
                        result.push('\n');
                    }
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}\n", ctx.get_indent()));

                // Emit `static func __create(<params>) -> Counter` — factory
                result.push('\n');
                let create_params = self.emit_params(params);
                result.push_str(&format!(
                    "{}static func __create({}) -> {} {{\n",
                    ctx.get_indent(),
                    create_params,
                    class_name
                ));
                ctx.push_indent();
                result.push_str(&format!("{}let c = {}()\n", ctx.get_indent(), class_name));
                // Swift: emit_params prefixes the first param with `_`
                // to disable the call-site label. Match that — call
                // positionally (no `label:` prefix).
                let arg_pass: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                result.push_str(&format!(
                    "{}c.__frame_init({})\n",
                    ctx.get_indent(),
                    arg_pass.join(", ")
                ));
                result.push_str(&format!("{}return c\n", ctx.get_indent()));
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
                let decl_kw = if *is_const { "let" } else { "var" };
                let type_str = type_annotation
                    .as_ref()
                    .map(|t| format!(": {}", self.map_type(t)))
                    .unwrap_or_default();
                if let Some(init_expr) = init {
                    format!(
                        "{}{} {}{} = {}",
                        ctx.get_indent(),
                        decl_kw,
                        name,
                        type_str,
                        self.emit(init_expr, ctx)
                    )
                } else {
                    format!("{}{} {}{}", ctx.get_indent(), decl_kw, name, type_str)
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
                    result.push_str(&self.emit(stmt, ctx));
                    result.push('\n');
                }
                ctx.pop_indent();

                if let Some(else_stmts) = else_block {
                    result.push_str(&format!("{}}} else {{\n", ctx.get_indent()));
                    ctx.push_indent();
                    for stmt in else_stmts {
                        result.push_str(&self.emit(stmt, ctx));
                        result.push('\n');
                    }
                    ctx.pop_indent();
                }
                result.push_str(&format!("{}}}", ctx.get_indent()));
                result
            }

            CodegenNode::Match { scrutinee, arms } => {
                // Swift uses "switch" with no fallthrough by default
                let mut result = format!(
                    "{}switch ({}) {{\n",
                    ctx.get_indent(),
                    self.emit(scrutinee, ctx)
                );
                ctx.push_indent();
                for arm in arms {
                    result.push_str(&format!(
                        "{}case {}:\n",
                        ctx.get_indent(),
                        self.emit(&arm.pattern, ctx)
                    ));
                    ctx.push_indent();
                    for stmt in &arm.body {
                        result.push_str(&self.emit(stmt, ctx));
                        result.push('\n');
                    }
                    ctx.pop_indent();
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}", ctx.get_indent()));
                result
            }

            CodegenNode::While { condition, body } => {
                let mut result = format!(
                    "{}while ({}) {{\n",
                    ctx.get_indent(),
                    self.emit(condition, ctx)
                );
                ctx.push_indent();
                for stmt in body {
                    result.push_str(&self.emit(stmt, ctx));
                    result.push('\n');
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}", ctx.get_indent()));
                result
            }

            CodegenNode::For {
                var,
                iterable,
                body,
            } => {
                // Swift: for item in collection
                let mut result = format!(
                    "{}for {} in {} {{\n",
                    ctx.get_indent(),
                    var,
                    self.emit(iterable, ctx)
                );
                ctx.push_indent();
                for stmt in body {
                    result.push_str(&self.emit(stmt, ctx));
                    result.push('\n');
                }
                ctx.pop_indent();
                result.push_str(&format!("{}}}", ctx.get_indent()));
                result
            }

            CodegenNode::Break => format!("{}break", ctx.get_indent()),
            CodegenNode::Continue => format!("{}continue", ctx.get_indent()),
            CodegenNode::ExprStmt(expr) => format!("{}{}", ctx.get_indent(), self.emit(expr, ctx)),
            CodegenNode::Await(expr) => self.emit(expr, ctx),

            CodegenNode::Comment { text, is_doc } => {
                if *is_doc {
                    format!("{}/// {}", ctx.get_indent(), text)
                } else {
                    format!("{}// {}", ctx.get_indent(), text)
                }
            }

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
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();
                format!(
                    "{}.{}({})",
                    self.emit(object, ctx),
                    method,
                    args_str.join(", ")
                )
            }

            CodegenNode::FieldAccess { object, field } => {
                format!("{}.{}", self.emit(object, ctx), field)
            }
            // Swift: use indexer syntax obj[index]
            CodegenNode::IndexAccess { object, index } => {
                format!("{}[{}]", self.emit(object, ctx), self.emit(index, ctx))
            }
            CodegenNode::SelfRef => "self".to_string(),

            CodegenNode::Array(elements) => {
                let elems: Vec<String> = elements.iter().map(|e| self.emit(e, ctx)).collect();
                if elems.is_empty() {
                    "[Any?]()".to_string()
                } else {
                    format!("[{}]", elems.join(", "))
                }
            }

            CodegenNode::Dict(pairs) => {
                if pairs.is_empty() {
                    "[String: Any]()".to_string()
                } else {
                    let pairs_str: Vec<String> = pairs
                        .iter()
                        .map(|(k, v)| format!("{}: {}", self.emit(k, ctx), self.emit(v, ctx)))
                        .collect();
                    format!("[{}]", pairs_str.join(", "))
                }
            }

            CodegenNode::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                format!(
                    "{} ? {} : {}",
                    self.emit(condition, ctx),
                    self.emit(then_expr, ctx),
                    self.emit(else_expr, ctx)
                )
            }

            CodegenNode::Lambda { params, body } => {
                let params_str = params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {} in {} }}", params_str, self.emit(body, ctx))
            }

            // Swift: "expr as! Type" for forced cast
            CodegenNode::Cast { expr, target_type } => {
                format!("{} as! {}", self.emit(expr, ctx), target_type)
            }

            // Swift: no "new" keyword
            CodegenNode::New { class, args } => {
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();
                format!("{}({})", class, args_str.join(", "))
            }

            // Frame-specific (expanded upstream as NativeBlock in normal pipeline)
            CodegenNode::Transition {
                target_state,
                indent,
                ..
            } => {
                let ind = format!("{}{}", ctx.get_indent(), " ".repeat(*indent));
                format!(
                    "{}self.__transition({}Compartment(state: \"{}\"))",
                    ind,
                    ctx.system_name.as_deref().unwrap_or(""),
                    target_state
                )
            }
            CodegenNode::ChangeState {
                target_state,
                indent,
                ..
            } => {
                let ind = format!("{}{}", ctx.get_indent(), " ".repeat(*indent));
                format!("{}self._changeState(self.{})", ind, target_state)
            }
            CodegenNode::StackPush { indent } => {
                let ind = format!("{}{}", ctx.get_indent(), " ".repeat(*indent));
                format!("{}self._state_stack.append(self.__compartment.copy())", ind)
            }
            CodegenNode::StackPop { indent } => {
                let ind = format!("{}{}", ctx.get_indent(), " ".repeat(*indent));
                format!("{}self.__transition(self._state_stack.removeLast())", ind)
            }
            CodegenNode::StateContext { state_name } => {
                format!("self._stateContext[\"{}\"]", state_name)
            }

            CodegenNode::SendEvent { event, args } => {
                let args_str: Vec<String> = args.iter().map(|a| self.emit(a, ctx)).collect();
                if args_str.is_empty() {
                    format!("{}self.{}()", ctx.get_indent(), event)
                } else {
                    format!(
                        "{}self.{}({})",
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
            CodegenNode::SplicePoint { id } => format!("// SPLICE_POINT: {}", id),
        }
    }

    fn emit_module_imports(
        &self,
        imports: &[crate::frame_c::compiler::frame_ast::Import],
    ) -> Vec<String> {
        // RFC-0022 — Swift treats every file in the same module as
        // mutually visible (no per-file imports). No emission is
        // needed when importer + importee are in the same module.
        // A comment marker records each declaration for tooling.
        imports
            .iter()
            .filter_map(|imp| {
                let path = imp.module.as_str();
                if path.is_empty() {
                    return None;
                }
                Some(format!(
                    "// RFC-0022: @@import \"{}\" — Swift module-local (no emission required)",
                    imp.module
                ))
            })
            .collect()
    }

    fn runtime_imports(&self) -> Vec<String> {
        vec!["import Foundation".to_string()]
    }

    fn class_syntax(&self) -> ClassSyntax {
        ClassSyntax::swift()
    }
    fn target_language(&self) -> TargetLanguage {
        TargetLanguage::Swift
    }
    fn null_keyword(&self) -> &'static str {
        "nil"
    }
}

impl SwiftBackend {
    fn emit_params(&self, params: &[Param]) -> String {
        params
            .iter()
            .map(|p| {
                let type_ann =
                    self.map_type(p.type_annotation.as_ref().unwrap_or(&"Any?".to_string()));
                format!("_ {}: {}", p.name, type_ann)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Swift visibility: internal is default (omit), private and public are explicit
    fn emit_visibility_swift(&self, vis: Visibility) -> &'static str {
        match vis {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "internal", // Swift doesn't have protected, use internal
        }
    }

    fn map_type(&self, t: &str) -> String {
        let t = t.trim();
        // Handle nullable types: "Type | nil" or "Type | null" -> "Type?"
        if let Some(pipe_pos) = t.find('|') {
            let base = t[..pipe_pos].trim();
            let suffix = t[pipe_pos + 1..].trim();
            if suffix == "nil" || suffix == "null" || suffix == "None" {
                return format!("{}?", self.map_type(base));
            }
        }
        // Handle array types like "string[]", "number[]", etc.
        if let Some(base) = t.strip_suffix("[]") {
            return format!("[{}]", self.map_type(base));
        }
        // Frame has NO type system: user-written type names pass through
        // VERBATIM (docs/frame_language.md). Write Swift's own names (`Int`,
        // `String`, `Double`, `[String]`). The arms below are framec-
        // synthesized machinery types only (untyped event params, the
        // structural `void` return, the nullable/array shapes handled above);
        // there is no `int`->`Int` / `str`->`String` alias table — it
        // contradicted the passthrough contract and was removed.
        match t {
            "Any" | "Object" | "object" => "Any?".to_string(),
            "void" => "Void".to_string(),
            "var" => "Any?".to_string(),
            other => other.to_string(),
        }
    }

    /// Reformat a raw `name: type[ = init]` domain declaration, normalizing
    /// the type slot through `map_type`. Under verbatim passthrough this is
    /// effectively identity for user types (Swift-native names emit as-is);
    /// it still resolves the nullable/array machinery shapes `map_type`
    /// handles.
    fn map_domain_types(&self, raw: &str) -> String {
        // Find the colon that separates name from type
        if let Some(colon_pos) = raw.find(':') {
            let name_part = &raw[..colon_pos];
            let rest = raw[colon_pos + 1..].trim();
            // Split type from initializer (= ...)
            let (type_part, init_part) = if let Some(eq_pos) = rest.find('=') {
                (rest[..eq_pos].trim(), Some(rest[eq_pos..].to_string()))
            } else {
                (rest.trim(), None)
            };
            let mapped_type = self.map_type(type_part);
            if let Some(init) = init_part {
                format!("{}: {} {}", name_part, mapped_type, init)
            } else {
                format!("{}: {}", name_part, mapped_type)
            }
        } else {
            raw.to_string()
        }
    }

    /// Emit a single Swift class-property declaration line:
    ///   `<indent>[<vis> ]var <name>: <type>[ = <init>]\n`
    ///
    /// Stored properties are always emitted `var`, never `let` — even for
    /// Frame `const` domain fields. A persisted `const` (e.g.
    /// `const threshold: int = threshold`, seeded from a system param)
    /// carries a per-instance value that lives only in the persist blob, so
    /// `restore_state` must be able to reassign it outside `init` — a Swift
    /// `let` would make that a compile error. Frame-level const-immutability
    /// is already enforced by the validator (E814+), so the Swift `let` is
    /// redundant — and using `var` is what keeps const-from-param values
    /// round-tripping through save/restore. The type is run through
    /// `map_type`, which passes user-written Swift names through verbatim and
    /// resolves only machinery shapes. Visibility `internal` (Frame's
    /// `Protected`) becomes Swift's default and is OMITTED rather than
    /// emitted explicitly — matches the `emit_visibility_swift` empty-string
    /// convention used elsewhere.
    fn emit_field(&self, field: &Field, ctx: &mut EmitContext) -> String {
        let vis = self.emit_visibility_swift(field.visibility);
        let var_kw = "var ";
        let mut type_str = match &field.type_annotation {
            Some(t) => self.map_type(t),
            None => "Any?".to_string(),
        };
        // A reference/protocol-typed field deferred to `__frame_init` has no
        // zero value and no initializer here — an uninitialized non-optional
        // stored property fails Swift's definite-initialization rule (#156, the
        // same family as #147's Kotlin `lateinit`). Such a field becomes an
        // implicitly-unwrapped optional (`Dep!`) below — the standard two-phase
        // init idiom; `__frame_init` assigns the real value via `__create`
        // before any use, so the IUO never traps.
        let mut make_iuo = false;
        let init_suffix = match &field.initializer {
            Some(init) => format!(" = {}", self.emit(init, ctx)),
            // Swift requires every stored property to be initialized by
            // the end of the designated `init()`. A domain field whose
            // initializer was stripped (because it references a system
            // param — the assignment moves to `__frame_init`) reaches
            // here with no initializer, so seed it with the type's zero
            // value; `__frame_init` then overwrites it. Reference/protocol
            // types have no zero value → emit an IUO (see above).
            None => match type_str.as_str() {
                "Int" => " = 0".to_string(),
                "Double" => " = 0.0".to_string(),
                "Bool" => " = false".to_string(),
                "String" => " = \"\"".to_string(),
                t if t.ends_with('?') => " = nil".to_string(),
                t if t.starts_with('[') && t.contains(':') => " = [:]".to_string(),
                t if t.starts_with('[') => " = []".to_string(),
                _ => {
                    // Only a PUBLIC (domain) field is deferred to `__frame_init`
                    // and so needs the IUO. Private framework fields
                    // (`__compartment`, …) are assigned in `init()` and stay
                    // non-optional — matching #147's `Visibility::Public` scope.
                    make_iuo = matches!(field.visibility, Visibility::Public);
                    String::new()
                }
            },
        };
        if make_iuo {
            type_str.push('!');
        }
        let comments = field.format_leading_comments(&ctx.get_indent());
        if vis.is_empty() {
            format!(
                "{}{}{}{}: {}{}\n",
                comments,
                ctx.get_indent(),
                var_kw,
                field.name,
                type_str,
                init_suffix
            )
        } else {
            format!(
                "{}{}{} {}{}: {}{}\n",
                comments,
                ctx.get_indent(),
                vis,
                var_kw,
                field.name,
                type_str,
                init_suffix
            )
        }
    }
}
