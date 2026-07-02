//! RFC-0043 layered codegen: casing/machine emission.
//!
//! For systems carrying `@@[async]` on async-capable backends with the
//! layered emission enabled (see [`should_emit_layered`]), framec emits
//! a two-class layered structure:
//!
//! 1. A public **casing** with the user-declared name (e.g. `Counter`).
//!    Holds the busy flag and an in-flight method-name marker; each
//!    interface method is a gated wrapper around the machine's method;
//!    operations and persist methods pass through to the machine
//!    without the gate.
//! 2. A private **machine** (`_<Name>Machine`). The existing async
//!    dispatch core (kernel, router, state methods, transition loop,
//!    lifecycle cascades) emitted exactly as today's `make_system_async`
//!    produces it.
//!
//! The two classes are returned as `CodegenNode::Module { items: [casing, machine] }`,
//! which every backend already handles in its `emit()` impl.
//!
//! Phase 4 day-1 wires Python only; [`should_emit_layered`] returns
//! `true` only for `TargetLanguage::Python3`. Other backends flip on
//! one at a time in Phase 6 of the RFC-0043 arc.

use super::super::ast::{CodegenNode, Field, Param, Visibility};
use crate::frame_c::compiler::frame_ast::Type as FrameType;
use crate::frame_c::compiler::frame_ast::{InterfaceMethod, OperationAst, SystemAst};
use crate::frame_c::visitors::TargetLanguage;

/// For typed targets (TypeScript / Java / etc.), surface the user's
/// declared type string verbatim so the backend's `convert_type` can
/// translate to the native type. For dynamic targets (Python,
/// JavaScript), no annotation is emitted.
fn type_annotation_for(t: &FrameType, lang: TargetLanguage) -> Option<String> {
    match (lang, t) {
        (TargetLanguage::Python3 | TargetLanguage::JavaScript, _) => None,
        (_, FrameType::Unknown) => None,
        (_, FrameType::Custom(s)) => Some(s.clone()),
    }
}

/// Same as [`type_annotation_for`] but for `Option<Type>` return-type
/// positions.
fn return_type_for(t: Option<&FrameType>, lang: TargetLanguage) -> Option<String> {
    match (lang, t) {
        (TargetLanguage::Python3 | TargetLanguage::JavaScript, _) => None,
        (_, None) => None,
        (_, Some(FrameType::Unknown)) => None,
        (_, Some(FrameType::Custom(s))) => Some(s.clone()),
    }
}

/// RFC-0043 D3: GDScript E703 gate falls through to an early-return
/// with a typed-zero value (replaces the pre-D3 `assert` which Godot
/// strips in release builds). This helper maps the user's declared
/// return type to a zero literal GDScript will accept under static
/// typing. Unknown / unrecognized types fall back to `null` — GDScript
/// will accept null in most slots when typing is `Variant`-like, and
/// in strict-typed code the user's runtime path was already going to
/// crash on E703 so the fallback is no worse.
fn gdscript_typed_zero(t: Option<&FrameType>) -> &'static str {
    let s = match t {
        Some(FrameType::Custom(s)) => s.as_str(),
        _ => return "null",
    };
    // Lowercase prefix match so `Int` / `int` both map. Array / Dict
    // matches the prefix because `Array[String]` / `Dictionary[String,
    // Int]` are typed variants; the zero is the empty container in
    // either case.
    let lower = s.to_lowercase();
    match lower.as_str() {
        "int" | "i32" | "i64" | "u32" | "u64" | "i16" | "u16" | "i8" | "u8" => "0",
        "float" | "f32" | "f64" | "double" | "real" => "0.0",
        "bool" | "boolean" => "false",
        "string" | "str" => "\"\"",
        _ if lower.starts_with("array") => "[]",
        _ if lower.starts_with("dictionary") => "{}",
        _ if lower.starts_with("packed") => "[]", // PackedStringArray etc.
        _ => "null",
    }
}

/// Per-backend opt-in to layered emission. Returns `true` only for
/// backends where the casing/machine shape has been verified end-to-end.
/// Phase 4 wires Python; Phase 5 wires Rust (its emission lives in
/// `rust_system/casing.rs` but shares this predicate); Phase 6 flips
/// the shared-pipeline backends one at a time as each is verified.
pub(crate) fn should_emit_layered(lang: TargetLanguage) -> bool {
    matches!(
        lang,
        TargetLanguage::Python3
            | TargetLanguage::Rust
            | TargetLanguage::TypeScript
            | TargetLanguage::JavaScript
            | TargetLanguage::Java
            | TargetLanguage::CSharp
            | TargetLanguage::Kotlin
            | TargetLanguage::Swift
            | TargetLanguage::Dart
            | TargetLanguage::GDScript
            | TargetLanguage::Cpp
    )
}

/// Given the machine class node (the post-`make_system_async` dispatch
/// core) and the source `SystemAst`, build the layered Module containing
/// a casing wrapping the machine.
///
/// Mutates `machine_class` to rename it to `_<Name>Machine` and mark it
/// `Visibility::Private`. Returns the wrapping Module.
pub(crate) fn wrap_in_casing(
    system: &SystemAst,
    mut machine_class: CodegenNode,
    lang: TargetLanguage,
) -> CodegenNode {
    let machine_name = format!("_{}Machine", system.name);

    if let CodegenNode::Class {
        ref mut name,
        ref mut visibility,
        ..
    } = machine_class
    {
        *name = machine_name.clone();
        *visibility = Visibility::Private;
    }

    // Some backends emit the original system name verbatim inside
    // NativeBlock method bodies (e.g. TypeScript's `AsyncBasic._HSM_CHAIN[leaf]`
    // in `__prepareEnter`). After rename those references would point at
    // the casing class instead of the machine. Rewrite them.
    rewrite_class_name_refs_in_machine(&mut machine_class, &system.name, &machine_name, lang);

    let casing = generate_casing(system, &machine_name, lang);

    // C++ requires the machine type to be complete before the casing's
    // `_<Name>Machine machine;` field declaration. Every other backend
    // either has dynamic dispatch (Python/JS), class hoisting
    // (Kotlin/Swift/Dart/GDScript), or compiles classes in any order
    // (Java/C#/TS) — so casing-first works there.
    let items = if matches!(lang, TargetLanguage::Cpp) {
        vec![machine_class, casing]
    } else {
        vec![casing, machine_class]
    };

    CodegenNode::Module {
        imports: vec![],
        items,
    }
}

/// Substitute `<original>.` → `<machine>.` in every `NativeBlock` body
/// inside the machine class's methods. The trailing dot anchors the match to
/// member-access syntax (e.g. `Counter._HSM_CHAIN`, `Counter.staticHelper()`).
/// String/comment-safe: a literal `"Counter.foo"` inside a string is NOT
/// rewritten (the trailing dot alone does not protect it — `replace_outside_…`
/// does).
fn rewrite_class_name_refs_in_machine(
    machine_class: &mut CodegenNode,
    original_name: &str,
    machine_name: &str,
    lang: TargetLanguage,
) {
    if let CodegenNode::Class { methods, .. } = machine_class {
        let from = format!("{}.", original_name);
        let to = format!("{}.", machine_name);
        for method in methods.iter_mut() {
            rewrite_native_blocks(method, &from, &to, lang);
        }
    }
}

fn rewrite_native_blocks(node: &mut CodegenNode, from: &str, to: &str, lang: TargetLanguage) {
    match node {
        CodegenNode::NativeBlock { code, .. } if code.contains(from) => {
            *code = super::super::codegen_utils::replace_outside_strings_and_comments(
                code,
                lang,
                &[(from, to)],
            );
        }
        CodegenNode::Method { body, .. } | CodegenNode::Constructor { body, .. } => {
            for child in body.iter_mut() {
                rewrite_native_blocks(child, from, to, lang);
            }
        }
        _ => {}
    }
}

/// Build the casing class — public, user-declared name, owns the
/// machine + gate, delegates everything to the machine.
fn generate_casing(system: &SystemAst, machine_name: &str, lang: TargetLanguage) -> CodegenNode {
    let fields = generate_casing_fields(machine_name, lang);
    let mut methods = Vec::new();

    // RFC-0043 D2: Swift's casing throws a recoverable error on E703.
    // Emit the `FrameE703Error` enum as a nested type at the top of the
    // casing class so each casing's wrappers can reference it without
    // a fully-qualified prefix (callers reach it as
    // `<Casing>.FrameE703Error.busy(...)`). One per casing.
    if matches!(lang, TargetLanguage::Swift) {
        methods.push(CodegenNode::NativeBlock {
            code: "public enum FrameE703Error: Error {\n\
                   \x20   case busy(method: String, inFlight: String)\n\
                   }"
            .to_string(),
            span: None,
        });
    }

    methods.push(generate_casing_constructor(machine_name, lang));

    for ifm in &system.interface {
        methods.push(generate_casing_interface_wrapper(ifm, lang));
    }

    for op in &system.operations {
        methods.push(generate_casing_operation_delegate(op, lang));
    }

    if system.persist_attr.is_some() {
        if let Some(save) = generate_casing_save_delegate(system, lang) {
            methods.push(save);
        }
        if let Some(restore) = generate_casing_restore_delegate(system, lang) {
            methods.push(restore);
        }
    }

    methods.push(generate_casing_init_delegate(lang));

    if let Some(factory) = generate_casing_factory(system, lang) {
        methods.push(factory);
    }

    CodegenNode::Class {
        name: system.name.clone(),
        fields,
        methods,
        base_classes: system.bases.clone(),
        is_abstract: false,
        derives: vec![],
        visibility: if system.visibility.as_deref() == Some("private") {
            Visibility::Private
        } else {
            Visibility::Public
        },
    }
}

// ---------------------------------------------------------------------------
// Per-backend casing emission. Phase 4 = Python only.
// ---------------------------------------------------------------------------

fn generate_casing_fields(machine_name: &str, lang: TargetLanguage) -> Vec<Field> {
    match lang {
        // Python uses dynamic attributes; field decls are documentation
        // only. The constructor's NativeBlock assigns them.
        TargetLanguage::Python3 => vec![],
        TargetLanguage::TypeScript => vec![
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("boolean".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "in_flight".to_string(),
                type_annotation: Some("string | null".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::Java => vec![
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("boolean".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "in_flight".to_string(),
                // Java's String is a reference type, nullable. `null`
                // is the unset state matching the other backends'
                // `None` / `null`.
                type_annotation: Some("String".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::CSharp => vec![
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("bool".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                // C# 8+ nullable-reference annotation: `string?` for a
                // nullable string. Matches the existing C# codegen's
                // `AsyncWorkerCompartment? __next_compartment;` pattern.
                name: "in_flight".to_string(),
                type_annotation: Some("string?".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::Cpp => vec![
            // C++: by-value fields with default-construction. The
            // machine type must be complete at this point (wrap_in_casing
            // emits machine BEFORE casing for C++). `bool` is POD so it
            // requires explicit initialization — emit `bool busy = false`
            // via the field's initializer NativeBlock. `std::string`
            // default-constructs to empty so no initializer needed.
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("bool".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "in_flight".to_string(),
                type_annotation: Some("std::string".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::GDScript => vec![
            // GDScript: dynamically typed; the Class emitter ignores
            // type_annotation and emits `var name`. No access modifiers
            // either — the `_`-prefix convention does not affect the
            // class-level field. Constructor body assigns the three.
            Field {
                name: "machine".to_string(),
                type_annotation: None,
                visibility: Visibility::Public,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: None,
                visibility: Visibility::Public,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "in_flight".to_string(),
                type_annotation: None,
                visibility: Visibility::Public,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::Dart => vec![
            // Dart's `Visibility::Private` would prepend an `_` to the field
            // name (library-private convention). To keep the field names
            // consistent across backends (and the NativeBlock body
            // references uniform), declare them `Public` — the casing
            // class itself isn't reachable from outside the module in
            // any meaningful sense, so library-public on the fields
            // costs nothing. `late` is auto-added by `emit_field` for
            // the non-nullable types because they have no inline
            // initializer; the casing's constructor body assigns them.
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Public,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("bool".to_string()),
                visibility: Visibility::Public,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                // Nullable string — Dart's `Type?` syntax. No `late`
                // is added because nullable fields default to null and
                // can be uninitialized.
                name: "in_flight".to_string(),
                type_annotation: Some("String?".to_string()),
                visibility: Visibility::Public,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::Swift => vec![
            // Swift: private + var + nullable-marker String?. All three
            // declared without inline initializers; assigned in the
            // casing's `init()` body that the Swift Constructor render
            // emits from the Constructor's NativeBlock.
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("Bool".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                // Swift's nullable-string syntax: `String?`.
                name: "in_flight".to_string(),
                type_annotation: Some("String?".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        TargetLanguage::Kotlin => vec![
            // All three declared without inline initializers; assigned in
            // the `init {}` block that the Kotlin Constructor render emits
            // from the Constructor body. `var` (is_const: false) because
            // busy / in_flight are mutated by the gate; machine reassignment
            // never happens but we use `var` uniformly to match existing
            // codegen style (see baseline `_state_stack` etc.).
            Field {
                name: "machine".to_string(),
                type_annotation: Some(machine_name.to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "busy".to_string(),
                type_annotation: Some("Boolean".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
            Field {
                name: "in_flight".to_string(),
                // Kotlin nullable-string syntax: `String?`.
                type_annotation: Some("String?".to_string()),
                visibility: Visibility::Private,
                is_static: false,
                is_const: false,
                initializer: None,
                leading_comments: vec![],
            },
        ],
        _ => vec![],
    }
}

fn generate_casing_constructor(machine_name: &str, lang: TargetLanguage) -> CodegenNode {
    let body_code = match lang {
        TargetLanguage::Python3 => format!(
            "self._machine = {m}()\n\
             self._busy = False\n\
             self._in_flight = None",
            m = machine_name
        ),
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => format!(
            "this.machine = new {m}();\n\
             this.busy = false;\n\
             this.in_flight = null;",
            m = machine_name
        ),
        TargetLanguage::Java => format!(
            // Java's async boundary is CompletableFuture, but the machine's
            // internals are sync — `make_java_interface_async` makes the
            // machine fire $> synchronously in its `__create()` factory and
            // emit `init()` as a no-op completedFuture. The casing's
            // constructor mirrors that by calling the machine's `__create()`
            // so $> fires synchronously during casing construction; the
            // casing's `init()` delegates to the machine's no-op future.
            "this.machine = {m}.__create();\n\
             this.busy = false;\n\
             this.in_flight = null;",
            m = machine_name
        ),
        TargetLanguage::CSharp => format!(
            "this.machine = new {m}();\n\
             this.busy = false;\n\
             this.in_flight = null;",
            m = machine_name
        ),
        TargetLanguage::Kotlin => format!(
            // Kotlin's primary-constructor body lives in `init {}` (which
            // the Kotlin Constructor render emits). No `new` keyword — a
            // class call is the constructor invocation. No semicolons.
            "this.machine = {m}()\n\
             this.busy = false\n\
             this.in_flight = null",
            m = machine_name
        ),
        TargetLanguage::Swift => format!(
            // Swift: `self.foo = X()` style (no `new`); `nil` for the
            // optional. The Swift Constructor render emits this body
            // inside `init()` for the casing class.
            "self.machine = {m}()\n\
             self.busy = false\n\
             self.in_flight = nil",
            m = machine_name
        ),
        TargetLanguage::Dart => format!(
            // Dart: `this.foo = X();` — explicit `new` not needed in
            // modern Dart; semicolons required. The Dart Constructor
            // render emits this inside the casing's bare constructor.
            "this.machine = {m}();\n\
             this.busy = false;\n\
             this.in_flight = null;",
            m = machine_name
        ),
        TargetLanguage::GDScript => format!(
            // GDScript: `<Class>.new()` is the constructor call. No
            // semicolons; `null` instead of `nil`. The Constructor
            // arm emits this body inside `func _init():`.
            "self.machine = {m}.new()\n\
             self.busy = false\n\
             self.in_flight = null",
            m = machine_name
        ),
        // C++: `_<Name>Machine machine` and `std::string in_flight` are
        // auto-default-constructed; only `bool busy` (POD) needs explicit
        // initialization. Assigning in the constructor body keeps it
        // straightforward — both the bare `<Name>()` ctor and the
        // `__create()` factory run this. `this->` prefix is optional in
        // C++ but used here for parity with the wrapper bodies and to
        // suppress any name-shadowing surprise from member-initializer
        // syntax.
        TargetLanguage::Cpp => "this->busy = false;".to_string(),
        // Reachable backends are gated by `should_emit_layered`; each has an
        // explicit arm above. A silent `String::new()` would emit an empty,
        // gate-less async wrapper for a future backend — the #116/#117 failure
        // mode. Fail loudly instead.
        _ => unreachable!(
            "async layered casing (constructor body): unhandled backend {lang:?} \
             — should_emit_layered() admitted a target with no explicit arm"
        ),
    };

    CodegenNode::Constructor {
        params: vec![],
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        super_call: None,
    }
}

fn generate_casing_interface_wrapper(ifm: &InterfaceMethod, lang: TargetLanguage) -> CodegenNode {
    let body_code = match lang {
        TargetLanguage::Python3 => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            format!(
                "if self._busy:\n\
                 \x20   raise RuntimeError(\n\
                 \x20       f\"E703: system busy: cannot enter '{name}' while \"\n\
                 \x20       f\"'{{self._in_flight}}' is in flight\"\n\
                 \x20   )\n\
                 self._busy = True\n\
                 self._in_flight = \"{name}\"\n\
                 try:\n\
                 \x20   return await self._machine.{name}({args})\n\
                 finally:\n\
                 \x20   self._in_flight = None\n\
                 \x20   self._busy = False",
                name = ifm.name,
                args = arg_str
            )
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            format!(
                "if (this.busy) {{\n\
                 \x20   throw new Error(\n\
                 \x20       `E703: system busy: cannot enter '{name}' while '${{this.in_flight}}' is in flight`\n\
                 \x20   );\n\
                 }}\n\
                 this.busy = true;\n\
                 this.in_flight = \"{name}\";\n\
                 try {{\n\
                 \x20   return await this.machine.{name}({args});\n\
                 }} finally {{\n\
                 \x20   this.in_flight = null;\n\
                 \x20   this.busy = false;\n\
                 }}",
                name = ifm.name,
                args = arg_str
            )
        }
        TargetLanguage::Kotlin => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // Kotlin: `suspend fun X()` — no explicit `await` keyword;
            // calling a suspend fun from within one chains naturally.
            // void return = no `return`; value return = `return`.
            let has_return = !matches!(ifm.return_type, None | Some(FrameType::Unknown));
            let delegate_line = if has_return {
                format!("return this.machine.{}({})", ifm.name, arg_str)
            } else {
                format!("this.machine.{}({})", ifm.name, arg_str)
            };
            format!(
                "if (this.busy) {{\n\
                 \x20   throw IllegalStateException(\n\
                 \x20       \"E703: system busy: cannot enter '{name}' while '${{this.in_flight}}' is in flight\"\n\
                 \x20   )\n\
                 }}\n\
                 this.busy = true\n\
                 this.in_flight = \"{name}\"\n\
                 try {{\n\
                 \x20   {delegate}\n\
                 }} finally {{\n\
                 \x20   this.in_flight = null\n\
                 \x20   this.busy = false\n\
                 }}",
                name = ifm.name,
                delegate = delegate_line
            )
        }
        TargetLanguage::CSharp => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // C# distinguishes `async Task` (void) from `async Task<T>` —
            // a void async method body cannot use `return await ...`.
            let has_return = !matches!(ifm.return_type, None | Some(FrameType::Unknown));
            let delegate_line = if has_return {
                format!("return await this.machine.{}({});", ifm.name, arg_str)
            } else {
                format!("await this.machine.{}({});", ifm.name, arg_str)
            };
            format!(
                "if (this.busy) {{\n\
                 \x20   throw new System.InvalidOperationException(\n\
                 \x20       $\"E703: system busy: cannot enter '{name}' while '{{this.in_flight}}' is in flight\"\n\
                 \x20   );\n\
                 }}\n\
                 this.busy = true;\n\
                 this.in_flight = \"{name}\";\n\
                 try {{\n\
                 \x20   {delegate}\n\
                 }} finally {{\n\
                 \x20   this.in_flight = null;\n\
                 \x20   this.busy = false;\n\
                 }}",
                name = ifm.name,
                delegate = delegate_line
            )
        }
        TargetLanguage::Cpp => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // RFC-0049: the busy/in_flight gate cleanup uses an RAII
            // scope-guard (resets both on scope exit — co_return AND, when
            // exceptions are enabled, unwind), not a try/catch(...)+rethrow.
            // The guard captures member pointers (`bool*`/`std::string*`) so
            // it needs no casing type name. The E703 busy violation is a
            // proper precondition error (R2): `throw` where exceptions exist,
            // `abort`-with-message fallback under `-fno-exceptions` (R3). The
            // `if (this->busy)` check runs BEFORE the guard is armed, so an
            // E703 rejection never resets a gate it didn't set.
            //
            // This makes the async casing compile under `-fno-exceptions`:
            // the FrameTask's `std::rethrow_exception` is a function call
            // (legal with exceptions off; dead because Frame handlers never
            // throw), so the wrapper's `throw`/`try` were the only blockers.
            let has_return = !matches!(ifm.return_type, None | Some(FrameType::Unknown));
            let success_block = if has_return {
                format!(
                    "auto __result = co_await this->machine.{name}({args});\n\
                     co_return __result;",
                    name = ifm.name,
                    args = arg_str
                )
            } else {
                format!(
                    "co_await this->machine.{name}({args});\n\
                     co_return;",
                    name = ifm.name,
                    args = arg_str
                )
            };
            format!(
                "if (this->busy) {{\n\
                 #if defined(__cpp_exceptions) || defined(__EXCEPTIONS)\n\
                 \x20   throw std::runtime_error(\n\
                 \x20       \"E703: system busy: cannot enter '{name}' while '\" + this->in_flight + \"' is in flight\"\n\
                 \x20   );\n\
                 #else\n\
                 \x20   std::fprintf(stderr, \"E703: system busy: cannot enter '{name}' while '%s' is in flight\\n\", this->in_flight.c_str());\n\
                 \x20   std::abort();\n\
                 #endif\n\
                 }}\n\
                 this->busy = true;\n\
                 this->in_flight = \"{name}\";\n\
                 struct __E703Guard {{ bool* __b; std::string* __f; ~__E703Guard() {{ __f->clear(); *__b = false; }} }} __e703_guard{{&this->busy, &this->in_flight}};\n\
                 {success}",
                name = ifm.name,
                success = success_block
            )
        }
        TargetLanguage::GDScript => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // GDScript has no try/finally. Original RFC-0043 emission
            // used `assert(not self.busy, ...)` — but Godot strips
            // `assert()` calls in --release builds, so the gate
            // becomes a silent no-op in shipped games. The pilot user
            // flagged that exact "silent failure" risk in his original
            // feedback that started RFC-0043.
            //
            // RFC-0043 D3 replaces the assert with:
            //   if self.busy:
            //       push_error("E703: ...")
            //       return <typed-zero>
            //
            // `push_error()` logs to the debugger AND prints to stderr
            // in ALL builds. The early-return prevents reentry and
            // returns a typed-zero value that satisfies the declared
            // return type. Callers see a sentinel value (typed zero)
            // rather than a crash — not as obvious as `throw`, but
            // survives release-build stripping.
            let has_return = !matches!(ifm.return_type, None | Some(FrameType::Unknown));
            let body = if has_return {
                let zero = gdscript_typed_zero(ifm.return_type.as_ref());
                format!(
                    "if self.busy:\n\
                     \x20   push_error(\"E703: system busy: cannot enter '{name}' while '%s' is in flight\" % str(self.in_flight))\n\
                     \x20   return {zero}\n\
                     self.busy = true\n\
                     self.in_flight = \"{name}\"\n\
                     var __result = await self.machine.{name}({args})\n\
                     self.in_flight = null\n\
                     self.busy = false\n\
                     return __result",
                    name = ifm.name,
                    args = arg_str,
                    zero = zero
                )
            } else {
                format!(
                    "if self.busy:\n\
                     \x20   push_error(\"E703: system busy: cannot enter '{name}' while '%s' is in flight\" % str(self.in_flight))\n\
                     \x20   return\n\
                     self.busy = true\n\
                     self.in_flight = \"{name}\"\n\
                     await self.machine.{name}({args})\n\
                     self.in_flight = null\n\
                     self.busy = false",
                    name = ifm.name,
                    args = arg_str
                )
            };
            body
        }
        TargetLanguage::Dart => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // Dart: `StateError` is the conventional unrecoverable-
            // programming-error exception. `try { ... } finally { ... }`
            // matches Python / TS / JS / Kotlin / Java / C#. Void async
            // methods (`Future<void>`) cannot `return await ...` — emit
            // a bare `await ...;` instead.
            let has_return = !matches!(ifm.return_type, None | Some(FrameType::Unknown));
            let delegate_line = if has_return {
                format!("return await this.machine.{}({});", ifm.name, arg_str)
            } else {
                format!("await this.machine.{}({});", ifm.name, arg_str)
            };
            format!(
                "if (this.busy) {{\n\
                 \x20   throw StateError(\n\
                 \x20       \"E703: system busy: cannot enter '{name}' while '${{this.in_flight ?? \"?\"}}' is in flight\"\n\
                 \x20   );\n\
                 }}\n\
                 this.busy = true;\n\
                 this.in_flight = \"{name}\";\n\
                 try {{\n\
                 \x20   {delegate}\n\
                 }} finally {{\n\
                 \x20   this.in_flight = null;\n\
                 \x20   this.busy = false;\n\
                 }}",
                name = ifm.name,
                delegate = delegate_line
            )
        }
        TargetLanguage::Swift => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // Swift: `defer { ... }` runs when the function scope exits,
            // including after a `throw`, after an `await`, and on early
            // `return`. The casing's gated interface wrappers are
            // `async throws -> T` (RFC-0043 §Swift D2): E703 is a
            // RECOVERABLE error the caller can `try?` / `catch`, aligning
            // Swift with every other layered backend (Python's
            // RuntimeError, JS's Error, Java's RuntimeException, etc.).
            // Pre-D2 emitted `fatalError(...)` which terminated the
            // program; switching to `throw FrameE703Error.busy(...)`
            // requires callers to add `try` at every interface-method
            // call site — a one-time migration.
            //
            // The Method emitter detects the `__swift_throws__`
            // decorator marker below and emits `throws` between
            // `async` and `->`.
            let has_return = !matches!(ifm.return_type, None | Some(FrameType::Unknown));
            let delegate_line = if has_return {
                format!("return await self.machine.{}({})", ifm.name, arg_str)
            } else {
                format!("await self.machine.{}({})", ifm.name, arg_str)
            };
            format!(
                "if self.busy {{\n\
                 \x20   throw FrameE703Error.busy(method: \"{name}\", inFlight: self.in_flight ?? \"?\")\n\
                 }}\n\
                 self.busy = true\n\
                 self.in_flight = \"{name}\"\n\
                 defer {{\n\
                 \x20   self.in_flight = nil\n\
                 \x20   self.busy = false\n\
                 }}\n\
                 {delegate}",
                name = ifm.name,
                delegate = delegate_line
            )
        }
        TargetLanguage::Java => {
            let arg_list: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            // The machine's interface method has TWO possible shapes:
            //
            //   - User-declared `async fetch(...): T` →
            //     `make_java_interface_async` marks it async, so the
            //     machine emits `CompletableFuture<T> fetch(...)`. The
            //     casing's wrapper returns the machine's future
            //     directly.
            //
            //   - User-declared SYNC `get(): T` (no `async`) on an
            //     `@@[async]` system → machine stays sync and emits
            //     `T get(...)`. The casing still exposes
            //     `CompletableFuture<T> get(...)` for API uniformity,
            //     so the body wraps the sync result via
            //     `CompletableFuture.completedFuture(...)`.
            //
            // Two ways the user can see a failure:
            //
            //   1. E703 gate-violation: `CompletableFuture.failedFuture(...)`
            //      so the caller sees the failure through the SAME
            //      mechanism as a successful result — chain via
            //      `.exceptionally(...)`, `.handle(...)`, or `.get()`
            //      (rethrows wrapped in ExecutionException).
            //
            //   2. Handler-thrown RuntimeException from the machine
            //      call: catch and convert to `failedFuture(e)` so the
            //      contract matches the rest of the layered backends.
            //
            // Pre-fix (D-JAVA-1): both paths threw synchronously out of
            // the casing, diverging from every other backend's contract
            // and breaking `cf.exceptionally(...)` recovery.
            let success_expr = if ifm.is_async {
                // Async machine method already returns CompletableFuture<T>.
                format!(
                    "return this.machine.{name}({args});",
                    name = ifm.name,
                    args = arg_str
                )
            } else {
                // Sync machine method returns plain T; wrap.
                format!(
                    "return java.util.concurrent.CompletableFuture.completedFuture(this.machine.{name}({args}));",
                    name = ifm.name,
                    args = arg_str
                )
            };
            format!(
                "if (this.busy) {{\n\
                 \x20   return java.util.concurrent.CompletableFuture.failedFuture(\n\
                 \x20       new RuntimeException(\n\
                 \x20           \"E703: system busy: cannot enter '{name}' while '\" + this.in_flight + \"' is in flight\"\n\
                 \x20       )\n\
                 \x20   );\n\
                 }}\n\
                 this.busy = true;\n\
                 this.in_flight = \"{name}\";\n\
                 try {{\n\
                 \x20   {success}\n\
                 }} catch (RuntimeException __e) {{\n\
                 \x20   return java.util.concurrent.CompletableFuture.failedFuture(__e);\n\
                 }} finally {{\n\
                 \x20   this.in_flight = null;\n\
                 \x20   this.busy = false;\n\
                 }}",
                name = ifm.name,
                success = success_expr
            )
        }
        _ => unreachable!(
            "async layered casing (interface-wrapper body): unhandled backend {lang:?} \
             — should_emit_layered() admitted a target with no explicit arm"
        ),
    };

    let params = ifm
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            type_annotation: type_annotation_for(&p.param_type, lang),
            default_value: None,
        })
        .collect();

    // RFC-0043 D2: Swift's casing wrappers are `async throws -> T`.
    // The `__swift_throws__` decorator is a side-channel to Swift's
    // Method emitter; see `backends/swift.rs` for the expansion.
    let decorators = if matches!(lang, TargetLanguage::Swift) {
        vec!["__swift_throws__".to_string()]
    } else {
        vec![]
    };

    CodegenNode::Method {
        name: ifm.name.clone(),
        params,
        return_type: return_type_for(ifm.return_type.as_ref(), lang),
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        // Casing interface methods are always async because they
        // delegate via `await` to the machine.
        is_async: true,
        is_static: false,
        visibility: Visibility::Public,
        decorators,
    }
}

fn generate_casing_operation_delegate(op: &OperationAst, lang: TargetLanguage) -> CodegenNode {
    // Operations bypass the gate — they're non-dispatching by declaration,
    // never touch `__kernel` or the busy flag. The casing's delegate
    // mirrors the user's `async` annotation: a user-sync op stays sync
    // on the casing (and on the machine, per `make_system_async`'s
    // skip-non-async-ops rule), and a user-async op becomes a coroutine
    // that awaits the machine's coroutine.
    let arg_list: Vec<String> = op.params.iter().map(|p| p.name.clone()).collect();
    let arg_str = arg_list.join(", ");
    let has_return = !matches!(op.return_type, FrameType::Unknown);
    let body_code = match lang {
        TargetLanguage::Python3 => {
            // `await` keyword only when the user marked the op async.
            let await_kw = if op.is_async { "await " } else { "" };
            format!(
                "return {a}self._machine.{name}({args})",
                a = await_kw,
                name = op.name,
                args = arg_str
            )
        }
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            let await_kw = if op.is_async { "await " } else { "" };
            format!(
                "return {a}this.machine.{name}({args});",
                a = await_kw,
                name = op.name,
                args = arg_str
            )
        }
        TargetLanguage::Java => {
            // Java async returns CompletableFuture<T>; no `await` keyword,
            // just return the machine's future directly. Sync ops are
            // plain `return this.machine.X();`. Same form either way.
            format!(
                "return this.machine.{name}({args});",
                name = op.name,
                args = arg_str
            )
        }
        TargetLanguage::CSharp => {
            // C# void async cannot `return await X()`; emit a bare
            // `await ...;`. Sync void emits a bare call with no return.
            let await_kw = if op.is_async { "await " } else { "" };
            if has_return {
                format!(
                    "return {a}this.machine.{name}({args});",
                    a = await_kw,
                    name = op.name,
                    args = arg_str
                )
            } else {
                format!(
                    "{a}this.machine.{name}({args});",
                    a = await_kw,
                    name = op.name,
                    args = arg_str
                )
            }
        }
        TargetLanguage::Kotlin => {
            // Kotlin suspend functions don't use an `await` keyword;
            // calling a suspend fun from within one chains naturally.
            // No semicolons.
            if has_return {
                format!(
                    "return this.machine.{name}({args})",
                    name = op.name,
                    args = arg_str
                )
            } else {
                format!(
                    "this.machine.{name}({args})",
                    name = op.name,
                    args = arg_str
                )
            }
        }
        TargetLanguage::Cpp => {
            // C++ coroutine: `co_return co_await ...` for value-returning
            // async ops, `co_await ...; co_return;` for void. Sync ops
            // use plain return/no-return.
            if op.is_async {
                if has_return {
                    format!(
                        "co_return co_await this->machine.{name}({args});",
                        name = op.name,
                        args = arg_str
                    )
                } else {
                    format!(
                        "co_await this->machine.{name}({args});\nco_return;",
                        name = op.name,
                        args = arg_str
                    )
                }
            } else if has_return {
                format!(
                    "return this->machine.{name}({args});",
                    name = op.name,
                    args = arg_str
                )
            } else {
                format!(
                    "this->machine.{name}({args});",
                    name = op.name,
                    args = arg_str
                )
            }
        }
        TargetLanguage::GDScript => {
            let await_kw = if op.is_async { "await " } else { "" };
            format!(
                "return {a}self.machine.{name}({args})",
                a = await_kw,
                name = op.name,
                args = arg_str
            )
        }
        TargetLanguage::Dart => {
            // Dart void async cannot `return await X();` — emit a bare
            // `await ...;` instead.
            let await_kw = if op.is_async { "await " } else { "" };
            if has_return {
                format!(
                    "return {a}this.machine.{name}({args});",
                    a = await_kw,
                    name = op.name,
                    args = arg_str
                )
            } else {
                format!(
                    "{a}this.machine.{name}({args});",
                    a = await_kw,
                    name = op.name,
                    args = arg_str
                )
            }
        }
        TargetLanguage::Swift => {
            // Swift: `await` keyword required on the call; void vs value
            // split. Positional call args (machine's emit_params prefixes
            // every param with `_`).
            let await_kw = if op.is_async { "await " } else { "" };
            if has_return {
                format!(
                    "return {a}self.machine.{name}({args})",
                    a = await_kw,
                    name = op.name,
                    args = arg_str
                )
            } else {
                format!(
                    "{a}self.machine.{name}({args})",
                    a = await_kw,
                    name = op.name,
                    args = arg_str
                )
            }
        }
        _ => unreachable!(
            "async layered casing (operation-delegate body): unhandled backend {lang:?} \
             — should_emit_layered() admitted a target with no explicit arm"
        ),
    };

    let params = op
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            type_annotation: type_annotation_for(&p.param_type, lang),
            default_value: None,
        })
        .collect();

    CodegenNode::Method {
        name: op.name.clone(),
        params,
        return_type: type_annotation_for(&op.return_type, lang),
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        // Mirror the user's declaration: a sync op produces a sync
        // delegate; an `async` op produces an async delegate that awaits
        // the machine's coroutine. Either way, the gate is bypassed —
        // operations are non-dispatching.
        is_async: op.is_async,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    }
}

/// Return the language-idiomatic type for the persist serialization
/// blob. Today framec only emits String-serialized persist (JSON for
/// most backends, serde for Rust). When `@@[persist(<type>)]` carries
/// a richer typed signature in the future, this helper extends to
/// surface the declared type.
fn persist_blob_type(lang: TargetLanguage) -> Option<String> {
    match lang {
        // Dynamic targets: no annotation needed.
        TargetLanguage::Python3 | TargetLanguage::JavaScript | TargetLanguage::GDScript => None,
        TargetLanguage::TypeScript => Some("string".to_string()),
        TargetLanguage::Java | TargetLanguage::Swift | TargetLanguage::Kotlin => {
            Some("String".to_string())
        }
        TargetLanguage::CSharp => Some("string".to_string()),
        TargetLanguage::Dart => Some("String".to_string()),
        TargetLanguage::Cpp => Some("std::string".to_string()),
        _ => None,
    }
}

fn generate_casing_save_delegate(system: &SystemAst, lang: TargetLanguage) -> Option<CodegenNode> {
    let save_name = system.save_op_name_rfc0015()?;
    let body_code = match lang {
        TargetLanguage::Python3 => format!("return self._machine.{name}()", name = save_name),
        TargetLanguage::TypeScript
        | TargetLanguage::JavaScript
        | TargetLanguage::Java
        | TargetLanguage::CSharp => {
            format!("return this.machine.{name}();", name = save_name)
        }
        TargetLanguage::Kotlin => format!("return this.machine.{name}()", name = save_name),
        TargetLanguage::Swift => format!("return self.machine.{name}()", name = save_name),
        TargetLanguage::Dart => format!("return this.machine.{name}();", name = save_name),
        TargetLanguage::GDScript => format!("return self.machine.{name}()", name = save_name),
        TargetLanguage::Cpp => format!("return this->machine.{name}();", name = save_name),
        _ => unreachable!(
            "async layered casing (persist save-delegate): unhandled backend {lang:?} \
             — should_emit_layered() admitted a target with no explicit arm"
        ),
    };
    Some(CodegenNode::Method {
        name: save_name.to_string(),
        params: vec![],
        // The casing's save delegate returns the persist blob type
        // (matches the machine's save signature). Pre-fix this was
        // `None` which produced `void save_state()` on typed backends
        // — incompatible with the `return this.machine.X()` body
        // (Java / C# / Kotlin / Swift / Dart / C++ all rejected it).
        return_type: persist_blob_type(lang),
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    })
}

fn generate_casing_restore_delegate(
    system: &SystemAst,
    lang: TargetLanguage,
) -> Option<CodegenNode> {
    let load_name = system.load_op_name_rfc0015()?;
    let body_code = match lang {
        TargetLanguage::Python3 => format!("self._machine.{name}(data)", name = load_name),
        TargetLanguage::TypeScript
        | TargetLanguage::JavaScript
        | TargetLanguage::Java
        | TargetLanguage::CSharp => {
            format!("this.machine.{name}(data);", name = load_name)
        }
        TargetLanguage::Kotlin => format!("this.machine.{name}(data)", name = load_name),
        TargetLanguage::Swift => format!("self.machine.{name}(data)", name = load_name),
        TargetLanguage::Dart => format!("this.machine.{name}(data);", name = load_name),
        TargetLanguage::GDScript => format!("self.machine.{name}(data)", name = load_name),
        TargetLanguage::Cpp => format!("this->machine.{name}(data);", name = load_name),
        _ => unreachable!(
            "async layered casing (persist restore-delegate): unhandled backend {lang:?} \
             — should_emit_layered() admitted a target with no explicit arm"
        ),
    };
    Some(CodegenNode::Method {
        name: load_name.to_string(),
        params: vec![Param {
            name: "data".to_string(),
            // Match the machine's restore param type (the persist blob
            // type, currently always String / std::string). Pre-fix
            // this was `None` which produced `Object data` (Java) or
            // `object data` (C#) — incompatible with the caller passing
            // a String/string snapshot.
            type_annotation: persist_blob_type(lang),
            default_value: None,
        }],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    })
}

/// Kotlin requires the `__create` factory to live inside a
/// `companion object` on the user-facing class. The machinery prelude
/// emits one on the machine (now `_<Name>Machine`), but the user calls
/// `<Name>.__create()` on the casing — so we emit a sibling factory
/// here that constructs the casing.
///
/// Other backends (Java / C# / TypeScript / JavaScript / Python) derive
/// their `__create` from the Constructor render via `ctx.system_name`,
/// which is set per-Class — so each class (casing AND machine) ends up
/// with its own correctly-typed factory automatically. Kotlin uniquely
/// generates the factory from the machinery prelude using the original
/// `system.name`, which the rename pass cannot easily target.
fn generate_casing_factory(system: &SystemAst, lang: TargetLanguage) -> Option<CodegenNode> {
    match lang {
        TargetLanguage::Kotlin => {
            use crate::frame_c::compiler::codegen::codegen_utils::{
                kotlin_map_type, type_to_string,
            };
            let create_params: Vec<String> = system
                .params
                .iter()
                .map(|p| {
                    let ty = type_to_string(&p.param_type);
                    format!("{}: {}", p.name, kotlin_map_type(&ty))
                })
                .collect();
            let arg_pass: Vec<String> = system.params.iter().map(|p| p.name.clone()).collect();
            // No `@JvmStatic` — JVM-only, breaks Kotlin/JS/Native/wasm (#157).
            let body = format!(
                "fun __create({params}): {sys} {{\n    val c = {sys}()\n    c.__frame_init({args})\n    return c\n}}",
                sys = system.name,
                params = create_params.join(", "),
                args = arg_pass.join(", "),
            );
            Some(CodegenNode::NativeBlock {
                code: body,
                span: None,
            })
        }
        _ => None,
    }
}

fn generate_casing_init_delegate(lang: TargetLanguage) -> CodegenNode {
    let body_code = match lang {
        TargetLanguage::Python3 => "await self._machine.init()".to_string(),
        TargetLanguage::TypeScript | TargetLanguage::JavaScript => {
            "await this.machine.init();".to_string()
        }
        TargetLanguage::Java => {
            // Java's `init()` returns CompletableFuture<Void>. The machine's
            // init() is a no-op already-completed future per the existing
            // make_java_interface_async pattern; the casing delegates to it
            // directly, no extra wrapping needed.
            "return this.machine.init();".to_string()
        }
        TargetLanguage::CSharp => "await this.machine.init();".to_string(),
        TargetLanguage::Kotlin => "this.machine.init()".to_string(),
        // Swift renames Frame's `init` interface method to `initAsync`
        // because `init` is a reserved constructor keyword. The machine's
        // method (post-`async_wrap`) is named `initAsync`, and the casing
        // delegate must match so the user's `await s.initAsync()` call
        // resolves through the casing.
        TargetLanguage::Swift => "await self.machine.initAsync()".to_string(),
        TargetLanguage::Dart => "await this.machine.init();".to_string(),
        TargetLanguage::GDScript => "await self.machine.init()".to_string(),
        // C++ coroutine: `co_await` the machine's `init()` FrameTask
        // and `co_return;` (void coroutine). The casing's init is a
        // coroutine itself (is_async=true) so it returns FrameTask<void>.
        TargetLanguage::Cpp => "co_await this->machine.init();\nco_return;".to_string(),
        _ => unreachable!(
            "async layered casing (init-delegate): unhandled backend {lang:?} \
             — should_emit_layered() admitted a target with no explicit arm"
        ),
    };
    let init_name = match lang {
        TargetLanguage::Swift => "initAsync",
        _ => "init",
    };
    CodegenNode::Method {
        name: init_name.to_string(),
        params: vec![],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        is_async: true,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    }
}
