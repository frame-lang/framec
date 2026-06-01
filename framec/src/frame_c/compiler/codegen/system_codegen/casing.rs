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
/// translate to the native type. For dynamic targets (Python), no
/// annotation is emitted.
fn type_annotation_for(t: &FrameType, lang: TargetLanguage) -> Option<String> {
    match (lang, t) {
        (TargetLanguage::Python3, _) => None,
        (_, FrameType::Unknown) => None,
        (_, FrameType::Custom(s)) => Some(s.clone()),
    }
}

/// Same as [`type_annotation_for`] but for `Option<Type>` return-type
/// positions.
fn return_type_for(t: Option<&FrameType>, lang: TargetLanguage) -> Option<String> {
    match (lang, t) {
        (TargetLanguage::Python3, _) => None,
        (_, None) => None,
        (_, Some(FrameType::Unknown)) => None,
        (_, Some(FrameType::Custom(s))) => Some(s.clone()),
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
        TargetLanguage::Python3 | TargetLanguage::Rust | TargetLanguage::TypeScript
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
    rewrite_class_name_refs_in_machine(&mut machine_class, &system.name, &machine_name);

    let casing = generate_casing(system, &machine_name, lang);

    CodegenNode::Module {
        imports: vec![],
        items: vec![casing, machine_class],
    }
}

/// Substitute `<original>.` → `<machine>.` in every `NativeBlock` body
/// inside the machine class's methods. The trailing dot anchors the
/// match to member-access syntax (e.g. `Counter._HSM_CHAIN`,
/// `Counter.staticHelper()`) and avoids rewriting bare occurrences in
/// string literals.
fn rewrite_class_name_refs_in_machine(
    machine_class: &mut CodegenNode,
    original_name: &str,
    machine_name: &str,
) {
    if let CodegenNode::Class { methods, .. } = machine_class {
        let from = format!("{}.", original_name);
        let to = format!("{}.", machine_name);
        for method in methods.iter_mut() {
            rewrite_native_blocks(method, &from, &to);
        }
    }
}

fn rewrite_native_blocks(node: &mut CodegenNode, from: &str, to: &str) {
    match node {
        CodegenNode::NativeBlock { code, .. } if code.contains(from) => {
            *code = code.replace(from, to);
        }
        CodegenNode::Method { body, .. } | CodegenNode::Constructor { body, .. } => {
            for child in body.iter_mut() {
                rewrite_native_blocks(child, from, to);
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
        TargetLanguage::TypeScript => format!(
            "this.machine = new {m}();\n\
             this.busy = false;\n\
             this.in_flight = null;",
            m = machine_name
        ),
        _ => String::new(),
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
                 \x20   self._busy = False\n\
                 \x20   self._in_flight = None",
                name = ifm.name,
                args = arg_str
            )
        }
        TargetLanguage::TypeScript => {
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
                 \x20   this.busy = false;\n\
                 \x20   this.in_flight = null;\n\
                 }}",
                name = ifm.name,
                args = arg_str
            )
        }
        _ => String::new(),
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
        decorators: vec![],
    }
}

fn generate_casing_operation_delegate(op: &OperationAst, lang: TargetLanguage) -> CodegenNode {
    let body_code = match lang {
        TargetLanguage::Python3 => {
            let arg_list: Vec<String> = op.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            format!(
                "return self._machine.{name}({args})",
                name = op.name,
                args = arg_str
            )
        }
        TargetLanguage::TypeScript => {
            let arg_list: Vec<String> = op.params.iter().map(|p| p.name.clone()).collect();
            let arg_str = arg_list.join(", ");
            format!(
                "return this.machine.{name}({args});",
                name = op.name,
                args = arg_str
            )
        }
        _ => String::new(),
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
        // Operations bypass the gate AND stay sync — they delegate to
        // the machine's operation (which is non-dispatching) without
        // awaiting.
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    }
}

fn generate_casing_save_delegate(system: &SystemAst, lang: TargetLanguage) -> Option<CodegenNode> {
    let save_name = system.save_op_name_rfc0015()?;
    let body_code = match lang {
        TargetLanguage::Python3 => format!("return self._machine.{name}()", name = save_name),
        TargetLanguage::TypeScript => {
            format!("return this.machine.{name}();", name = save_name)
        }
        _ => String::new(),
    };
    Some(CodegenNode::Method {
        name: save_name.to_string(),
        params: vec![],
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

fn generate_casing_restore_delegate(
    system: &SystemAst,
    lang: TargetLanguage,
) -> Option<CodegenNode> {
    let load_name = system.load_op_name_rfc0015()?;
    let body_code = match lang {
        TargetLanguage::Python3 => format!("self._machine.{name}(data)", name = load_name),
        TargetLanguage::TypeScript => format!("this.machine.{name}(data);", name = load_name),
        _ => String::new(),
    };
    Some(CodegenNode::Method {
        name: load_name.to_string(),
        params: vec![Param {
            name: "data".to_string(),
            type_annotation: None,
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

fn generate_casing_init_delegate(lang: TargetLanguage) -> CodegenNode {
    let body_code = match lang {
        TargetLanguage::Python3 => "await self._machine.init()".to_string(),
        TargetLanguage::TypeScript => "await this.machine.init();".to_string(),
        _ => String::new(),
    };
    CodegenNode::Method {
        name: "init".to_string(),
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
