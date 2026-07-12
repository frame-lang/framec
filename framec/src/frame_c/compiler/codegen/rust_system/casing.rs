//! RFC-0043 layered codegen for Rust — casing/machine emission.
//!
//! Rust has its own pipeline (`rust_system.rs`) separate from the
//! 15-backend shared pipeline because of three independent reasons
//! documented in `docs/codegen_pipeline.md`. The casing emission
//! here is the Rust-idiomatic counterpart to the shared casing module
//! at `codegen/system_codegen/casing.rs`.
//!
//! Design lives in `_scratch/rfc0043_phase5_design.md`. In summary:
//!
//! - Plain `bool` and `Option<&'static str>` gate fields on the
//!   casing struct (no `Cell` / `RefCell`).
//! - Interface wrappers return `Result<T, FrameE703Error>` and emit
//!   `return Err(FrameE703Error { ... })` on gate violation (RFC-0043
//!   D5; pre-D5 emitted `panic!("E703: ...")`). Aligns Rust with every
//!   other layered backend, which all raise a recoverable error.
//! - RAII via `_GateGuard<'_>` holding split borrows on the gate
//!   fields, distinct from the `&mut self.machine` borrow that the
//!   awaited call holds. Resets fields on Drop, including on
//!   handler-side panic propagation (which IS still a panic, distinct
//!   from the gate-violation case).

use super::super::ast::{CodegenNode, Field, Param, Visibility};
use crate::frame_c::compiler::frame_ast::Type as FrameType;
use crate::frame_c::compiler::frame_ast::{InterfaceMethod, OperationAst, SystemAst};

/// Build the layered Module containing the Rust casing + the
/// `_GateGuard` helper + the machine. Mutates `machine_class` to rename
/// it to `_<Name>Machine` and mark it `Visibility::Private`.
pub(crate) fn wrap_in_casing(system: &SystemAst, mut machine_class: CodegenNode) -> CodegenNode {
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

    let casing = generate_casing(system, &machine_name);
    let gate_guard = generate_gate_guard_block();

    CodegenNode::Module {
        imports: vec![],
        items: vec![casing, gate_guard, machine_class],
    }
}

/// Emit the `_GateGuard<'a>` struct + its `Drop` impl, and the
/// `FrameE703Error` struct + Display + Error impls, as a single
/// module-level NativeBlock. Both go together because `impl X for Y`
/// blocks aren't directly representable in the `Class` IR variant.
fn generate_gate_guard_block() -> CodegenNode {
    let code = r#"struct _GateGuard<'a> {
    busy: &'a mut bool,
    in_flight: &'a mut Option<&'static str>,
}

impl<'a> Drop for _GateGuard<'a> {
    fn drop(&mut self) {
        *self.busy = false;
        *self.in_flight = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameE703Error {
    pub method: &'static str,
    pub in_flight: Option<&'static str>,
}

impl core::fmt::Display for FrameE703Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "E703: system busy: cannot enter '{}' while {:?} is in flight",
            self.method, self.in_flight
        )
    }
}

impl std::error::Error for FrameE703Error {}"#
        .to_string();

    CodegenNode::NativeBlock { code, span: None }
}

/// Build the casing struct + impl block: public, user-declared name,
/// owns the machine + gate fields, delegates everything to the machine.
fn generate_casing(system: &SystemAst, machine_name: &str) -> CodegenNode {
    let fields = generate_casing_fields(machine_name);
    let mut methods = Vec::new();

    // #167: when the system has params, the machine has no parameterless
    // `new()` (RFC-0020/#123: a param-bound struct is built only by
    // `__create`), so the casing likewise skips `new()` and builds the struct
    // directly in its `__create`.
    if system.params.is_empty() {
        methods.push(generate_casing_constructor(machine_name));
    }
    methods.push(generate_casing_factory_alias(system, machine_name));

    for ifm in &system.interface {
        methods.push(generate_casing_interface_wrapper(ifm));
    }

    for op in &system.operations {
        methods.push(generate_casing_operation_delegate(op));
    }

    if system.persist_attr.is_some() {
        if let Some(save) = generate_casing_save_delegate(system) {
            methods.push(save);
        }
        if let Some(restore) = generate_casing_restore_delegate(system) {
            methods.push(restore);
        }
    }

    methods.push(generate_casing_init_delegate());

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
        is_framework_helper: false,
        input: None,
    }
}

fn generate_casing_fields(machine_name: &str) -> Vec<Field> {
    vec![
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
            type_annotation: Some("Option<&'static str>".to_string()),
            visibility: Visibility::Private,
            is_static: false,
            is_const: false,
            initializer: None,
            leading_comments: vec![],
        },
    ]
}

fn generate_casing_constructor(machine_name: &str) -> CodegenNode {
    let body_code = format!(
        "Self {{\n\
         \x20   machine: {m}::new(),\n\
         \x20   busy: false,\n\
         \x20   in_flight: None,\n\
         }}",
        m = machine_name
    );
    // Rust constructors emit as `pub fn new() -> Self`. We model them
    // as a regular Method (not Constructor) because the IR's
    // Constructor variant uses Python's `__init__` shape.
    CodegenNode::Method {
        name: "new".to_string(),
        params: vec![],
        return_type: Some("Self".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Public,
        decorators: vec![],
    }
}

/// Factory alias: `pub fn __create() -> Self { Self::new() }`. Under
/// the layered architecture, construction happens via `new()` and the
/// enter cascade fires later via `init().await`. The factory alias
/// keeps the existing user idiom (`AsyncBasic::__create()` then
/// `s.init().await`) working without re-wiring callers.
///
/// RFC-0015 `@@[create(<name>)]` rename: if the user supplied a
/// factory name, use that instead of `__create`.
fn generate_casing_factory_alias(system: &SystemAst, machine_name: &str) -> CodegenNode {
    let name = system.create_op_name().unwrap_or("__create").to_string();
    // #167: forward the system's params to the machine's factory so a domain
    // field seeded from a constructor param is actually set. A param-carrying
    // machine has no parameterless `new()`, so build the casing struct directly
    // with the param-wired machine (`_<Name>Machine::__create(args)`).
    let (params, body_code) = if system.params.is_empty() {
        (Vec::new(), "Self::new()".to_string())
    } else {
        let params: Vec<Param> = system
            .params
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                type_annotation: Some(rust_type_to_string(Some(&p.param_type))),
                default_value: None,
            })
            .collect();
        let args = system
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let body = format!(
            "Self {{\n\
             \x20   machine: {m}::__create({args}),\n\
             \x20   busy: false,\n\
             \x20   in_flight: None,\n\
             }}",
            m = machine_name
        );
        (params, body)
    };
    CodegenNode::Method {
        name,
        params,
        return_type: Some("Self".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Public,
        decorators: vec![],
    }
}

fn generate_casing_interface_wrapper(ifm: &InterfaceMethod) -> CodegenNode {
    let raw_return = rust_type_to_string(ifm.return_type.as_ref());
    let inner_type = if raw_return.is_empty() {
        "()".to_string()
    } else {
        raw_return
    };
    let arg_call: Vec<String> = ifm.params.iter().map(|p| p.name.clone()).collect();
    let arg_call_str = arg_call.join(", ");

    // RFC-0043 D5: gate violation returns `Err(FrameE703Error { ... })`
    // instead of `panic!("E703: ...")`. Caller propagates with `?` or
    // matches the Err variant. Handler panics (a different error class
    // — programming bugs in user handler bodies) still propagate as
    // panics, with `_GateGuard::drop` cleaning up the gate on unwind.
    //
    // The happy path wraps the machine's return value in `Ok(...)`.
    // For void methods (`-> ()`), wrap the unit value: `Ok({ ...; () })`.
    let success_expr = if inner_type == "()" {
        format!(
            "{{ self.machine.{name}({args}).await; Ok(()) }}",
            name = ifm.name,
            args = arg_call_str
        )
    } else {
        format!(
            "Ok(self.machine.{name}({args}).await)",
            name = ifm.name,
            args = arg_call_str
        )
    };

    let body_code = format!(
        "if self.busy {{\n\
         \x20   return Err(FrameE703Error {{\n\
         \x20       method: \"{name}\",\n\
         \x20       in_flight: self.in_flight,\n\
         \x20   }});\n\
         }}\n\
         self.busy = true;\n\
         self.in_flight = Some(\"{name}\");\n\
         let _guard = _GateGuard {{\n\
         \x20   busy: &mut self.busy,\n\
         \x20   in_flight: &mut self.in_flight,\n\
         }};\n\
         {success}",
        name = ifm.name,
        success = success_expr
    );

    let params = ifm
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            type_annotation: Some(rust_type_to_string(Some(&p.param_type))),
            default_value: None,
        })
        .collect();

    CodegenNode::Method {
        name: ifm.name.clone(),
        params,
        return_type: Some(format!("Result<{}, FrameE703Error>", inner_type)),
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

fn generate_casing_operation_delegate(op: &OperationAst) -> CodegenNode {
    let return_type_str = rust_type_to_string_op(&op.return_type);
    let arg_call: Vec<String> = op.params.iter().map(|p| p.name.clone()).collect();
    let arg_call_str = arg_call.join(", ");

    let body_code = format!(
        "self.machine.{name}({args})",
        name = op.name,
        args = arg_call_str
    );

    let params = op
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            type_annotation: Some(rust_type_to_string(Some(&p.param_type))),
            default_value: None,
        })
        .collect();

    CodegenNode::Method {
        name: op.name.clone(),
        params,
        return_type: if return_type_str.is_empty() {
            None
        } else {
            Some(return_type_str)
        },
        body: vec![CodegenNode::NativeBlock {
            code: body_code,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    }
}

fn generate_casing_save_delegate(system: &SystemAst) -> Option<CodegenNode> {
    let save_name = system.save_op_name_rfc0015()?;
    let body_code = format!("self.machine.{name}()", name = save_name);
    Some(CodegenNode::Method {
        name: save_name.to_string(),
        params: vec![],
        // The persist blob type is dictated by `@@[persist(<type>)]`;
        // emitting `String` here is a placeholder. The machine's
        // save_op signature already reflects the right type — emitting
        // `String` here matches the common case and any user-typed
        // mismatch surfaces at host-compile time. Future refinement:
        // mirror the persist_attr's declared type onto the casing
        // surface too.
        return_type: Some("String".to_string()),
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

fn generate_casing_restore_delegate(system: &SystemAst) -> Option<CodegenNode> {
    let load_name = system.load_op_name_rfc0015()?;
    let body_code = format!("self.machine.{name}(data);", name = load_name);
    Some(CodegenNode::Method {
        name: load_name.to_string(),
        params: vec![Param {
            name: "data".to_string(),
            type_annotation: Some("String".to_string()),
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

fn generate_casing_init_delegate() -> CodegenNode {
    let body_code = "self.machine.init().await".to_string();
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

// ---------------------------------------------------------------------------
// Type-to-string helpers (Rust-flavored).
// ---------------------------------------------------------------------------

/// Render an optional `FrameType` to a Rust type string. Empty for
/// `None` / `Unknown` (representing a void return).
fn rust_type_to_string(t: Option<&FrameType>) -> String {
    match t {
        None => String::new(),
        Some(FrameType::Unknown) => String::new(),
        Some(FrameType::Custom(s)) => s.clone(),
    }
}

/// Operations carry `return_type: FrameType` (not Option<FrameType>).
/// `Unknown` is the void case.
fn rust_type_to_string_op(t: &FrameType) -> String {
    match t {
        FrameType::Unknown => String::new(),
        FrameType::Custom(s) => s.clone(),
    }
}
