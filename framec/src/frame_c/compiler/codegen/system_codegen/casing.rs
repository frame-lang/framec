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
use crate::frame_c::compiler::frame_ast::{InterfaceMethod, OperationAst, SystemAst};
use crate::frame_c::visitors::TargetLanguage;

/// Per-backend opt-in to layered emission. Returns `true` only for
/// backends where the casing/machine shape has been verified end-to-end.
/// Phase 4 wires Python; Phase 5 wires Rust (its emission lives in
/// `rust_system/casing.rs` but shares this predicate); subsequent
/// phases flip the remaining 8 async-capable backends as their
/// integrations land.
pub(crate) fn should_emit_layered(lang: TargetLanguage) -> bool {
    matches!(lang, TargetLanguage::Python3 | TargetLanguage::Rust)
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

    let casing = generate_casing(system, &machine_name, lang);

    CodegenNode::Module {
        imports: vec![],
        items: vec![casing, machine_class],
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

fn generate_casing_fields(_machine_name: &str, lang: TargetLanguage) -> Vec<Field> {
    match lang {
        // Python uses dynamic attributes; field decls are documentation
        // only. The constructor's NativeBlock assigns them.
        TargetLanguage::Python3 => vec![],
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
        _ => String::new(),
    };

    let params = ifm
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            type_annotation: None,
            default_value: None,
        })
        .collect();

    CodegenNode::Method {
        name: ifm.name.clone(),
        params,
        return_type: None,
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
        _ => String::new(),
    };

    let params = op
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            type_annotation: None,
            default_value: None,
        })
        .collect();

    CodegenNode::Method {
        name: op.name.clone(),
        params,
        return_type: None,
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
