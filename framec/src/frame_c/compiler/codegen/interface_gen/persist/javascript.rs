//! JavaScript / TypeScript persist codegen.
//!
//! Same wire format as Python (field-by-field JSON). The two
//! languages share an emitter parameterised by `is_ts: bool` — the
//! only real differences are type annotations on the recursive
//! serialize/deserialize helpers (`any` / `<sys>Compartment | null`)
//! and the casts on the `_state_stack.map` callbacks.
//!
//! RFC-0012 amendment: under the new contract, `restoreState` is an
//! instance method that mutates `this`. Under the legacy contract,
//! it's a static factory that constructs via
//! `Object.create(Foo.prototype)` (bypassing the user constructor)
//! and returns the constructed instance. `_init` has already fired
//! the start-state enter once on the (legacy) ordinary constructor
//! path; the new-contract form skips that reset on restore — the
//! "$S0 enter on restore" trade-off per the RFC amendment.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::frame_ast::SystemAst;

use super::super::{child_persist_names, extract_tagged_system_name, nested_uses_new_contract};

pub(in crate::frame_c::compiler::codegen::interface_gen) fn generate(
    system: &SystemAst,
    is_ts: bool,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();

    // RFC-0012 amendment: branch on new contract. Save was
    // already an instance method; load was a static factory
    // using `Object.create(Foo.prototype)`. Under the new
    // contract, both become user-named instance methods that
    // mutate `this` directly — no construction bypass needed.
    let uses_new_contract = system.uses_new_persist_contract();
    let save_method_name = system
        .save_op_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "saveState".to_string());
    let load_method_name = system
        .load_op_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "restoreState".to_string());
    let load_param_name = system
        .load_op_param_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "json".to_string());
    let target = if uses_new_contract {
        "this"
    } else {
        "instance"
    };
    // `Sys` for JS, `(Sys as any)` for TS — lets us hang the optional
    // `__persistUserTypes` registration map off the class without a declared
    // static field (which `--strict` would otherwise require).
    let sys_ref = if is_ts {
        format!("({} as any)", system.name)
    } else {
        system.name.clone()
    };

    // Generate saveState method
    let mut save_body = String::new();
    save_body.push_str("if (this._context_stack.length > 0) { throw new Error(\"E700: system not quiescent\"); }\n");
    if is_ts {
        save_body.push_str("const serializeComp = (c: any): any => {\n");
    } else {
        save_body.push_str("const serializeComp = (c) => {\n");
    }
    save_body.push_str("    if (!c) return null;\n");
    save_body.push_str("    return {\n");
    save_body.push_str("        state: c.state,\n");
    save_body.push_str("        state_args: {...c.state_args},\n");
    save_body.push_str("        state_vars: {...c.state_vars},\n");
    save_body.push_str("        enter_args: {...c.enter_args},\n");
    save_body.push_str("        exit_args: {...c.exit_args},\n");
    save_body.push_str("        forward_event: c.forward_event,\n");
    save_body.push_str("        parent_compartment: serializeComp(c.parent_compartment),\n");
    save_body.push_str("    };\n");
    save_body.push_str("};\n");
    // #174 / RFC-0053 reflective route (JS/TS): tag any non-plain object with its
    // class name so restore can rebuild the real type — generic, no per-type
    // branch. JSON.stringify applies the replacer recursively, so user values
    // nested in compartment state_vars/args and domain fields are all tagged;
    // plain objects (compartment scaffolding, embedded child-system blobs) are
    // left untouched.
    if is_ts {
        save_body.push_str("const _tag = (_k: string, _v: any): any => (_v && typeof _v === \"object\" && !Array.isArray(_v) && _v.constructor && _v.constructor !== Object) ? Object.assign({ __frame_type__: _v.constructor.name }, _v) : _v;\n");
    } else {
        save_body.push_str("const _tag = (_k, _v) => (_v && typeof _v === \"object\" && !Array.isArray(_v) && _v.constructor && _v.constructor !== Object) ? Object.assign({ __frame_type__: _v.constructor.name }, _v) : _v;\n");
    }
    save_body.push_str("return JSON.stringify({\n");
    save_body.push_str("    _compartment: serializeComp(this.__compartment),\n");
    if is_ts {
        save_body
            .push_str("    _state_stack: this._state_stack.map((c: any) => serializeComp(c)),\n");
    } else {
        save_body.push_str("    _state_stack: this._state_stack.map((c) => serializeComp(c)),\n");
    }

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (child_save, _) = child_persist_names(child_sys, "saveState", "restoreState");
            save_body.push_str(&format!(
                "    {0}: this.{0} ? JSON.parse(this.{0}.{1}()) : null,\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("    {}: this.{},\n", var.name, var.name));
        }
    }

    save_body.push_str("}, _tag);\n");

    methods.push(CodegenNode::Method {
        name: save_method_name.clone(),
        params: vec![],
        return_type: Some("string".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: save_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    // Generate restoreState method
    let mut restore_body = String::new();

    // #174 / RFC-0053 reflective route (JS/TS) — HYBRID closed-world registry.
    // ES modules expose no class enumeration, so the name->constructor map is
    // built from two zero-ambient sources: (1) graph-seed — walk the receiving
    // instance's own initialized object graph (every Frame variable has an
    // initializer, so its runtime type is already present); (2) an optional user
    // hook `registerPersistType` for a type with no initializer, or the legacy
    // Object.create path (no live graph). framec's own runtime classes are
    // excluded. A tag resolving to neither is refused (E750): a hostile snapshot
    // cannot name a foreign type, and globalThis is never consulted.
    if is_ts {
        restore_body.push_str("const _reg: Map<string, any> = new Map();\n");
    } else {
        restore_body.push_str("const _reg = new Map();\n");
    }
    restore_body.push_str(&format!(
        "const _excl = new Set([\"{0}\", \"{0}Compartment\", \"{0}FrameEvent\", \"{0}FrameContext\"]);\n",
        system.name
    ));
    if is_ts {
        restore_body.push_str("const _seed = (_o: any, _d: number): void => {\n");
    } else {
        restore_body.push_str("const _seed = (_o, _d) => {\n");
    }
    restore_body.push_str("    if (!_o || typeof _o !== \"object\" || _d > 64) return;\n");
    restore_body.push_str(
        "    if (Array.isArray(_o)) { for (const _e of _o) _seed(_e, _d + 1); return; }\n",
    );
    restore_body.push_str("    const _cn = _o.constructor && _o.constructor.name;\n");
    restore_body.push_str(
        "    if (_cn && _o.constructor !== Object && !_excl.has(_cn)) _reg.set(_cn, _o.constructor);\n",
    );
    restore_body.push_str("    for (const _k in _o) { if (Object.prototype.hasOwnProperty.call(_o, _k)) _seed(_o[_k], _d + 1); }\n");
    restore_body.push_str("};\n");
    if uses_new_contract {
        restore_body.push_str("_seed(this, 0);\n");
    }
    restore_body.push_str(&format!(
        "if ({0}.__persistUserTypes) {{ for (const [_k, _v] of {0}.__persistUserTypes) _reg.set(_k, _v); }}\n",
        sys_ref
    ));
    if is_ts {
        restore_body.push_str("const _revive = (_o: any): any => {\n");
    } else {
        restore_body.push_str("const _revive = (_o) => {\n");
    }
    restore_body.push_str("    if (!_o || typeof _o !== \"object\") return _o;\n");
    restore_body.push_str(
        "    if (Array.isArray(_o)) { for (let _i = 0; _i < _o.length; _i++) _o[_i] = _revive(_o[_i]); return _o; }\n",
    );
    restore_body
        .push_str("    if (Object.prototype.hasOwnProperty.call(_o, \"__frame_type__\")) {\n");
    restore_body.push_str("        const _t = _o.__frame_type__;\n");
    restore_body.push_str("        const _c = _reg.get(_t);\n");
    restore_body.push_str("        if (!_c) throw new Error(\"E750: persist restore refused a type not defined in this module: \" + _t);\n");
    restore_body.push_str("        const _obj = Object.create(_c.prototype);\n");
    restore_body.push_str("        for (const _k in _o) { if (_k !== \"__frame_type__\" && Object.prototype.hasOwnProperty.call(_o, _k)) _obj[_k] = _revive(_o[_k]); }\n");
    restore_body.push_str("        return _obj;\n");
    restore_body.push_str("    }\n");
    restore_body.push_str("    for (const _k in _o) { if (Object.prototype.hasOwnProperty.call(_o, _k)) _o[_k] = _revive(_o[_k]); }\n");
    restore_body.push_str("    return _o;\n");
    restore_body.push_str("};\n");

    if is_ts {
        restore_body.push_str(&format!(
            "const deserializeComp = (data: any): {}Compartment | null => {{\n",
            system.name
        ));
    } else {
        restore_body.push_str("const deserializeComp = (data) => {\n");
    }
    restore_body.push_str("    if (!data) return null;\n");
    restore_body.push_str(&format!(
        "    const comp = new {}Compartment(data.state);\n",
        system.name
    ));
    // Revive tree-wide over compartment-held values so a user-typed value in a
    // state_var / state_arg / enter_arg is rebuilt, not just domain fields.
    restore_body.push_str("    comp.state_args = _revive(data.state_args || {});\n");
    restore_body.push_str("    comp.state_vars = _revive(data.state_vars || {});\n");
    restore_body.push_str("    comp.enter_args = _revive(data.enter_args || {});\n");
    restore_body.push_str("    comp.exit_args = _revive(data.exit_args || {});\n");
    restore_body.push_str("    comp.forward_event = data.forward_event;\n");
    restore_body
        .push_str("    comp.parent_compartment = deserializeComp(data.parent_compartment);\n");
    restore_body.push_str("    return comp;\n");
    restore_body.push_str("};\n");
    // Use `_parsed` for the parsed object — under the new
    // contract the user's load param name might collide with
    // a local called `data` (e.g., `unpickle(data: string)`).
    restore_body.push_str(&format!(
        "const _parsed = JSON.parse({});\n",
        load_param_name
    ));
    // Legacy only: construct via Object.create (skips constructor —
    // no initial-state enter side effects). The new contract
    // form mutates `this` in place.
    if !uses_new_contract {
        restore_body.push_str(&format!(
            "const instance = Object.create({}.prototype);\n",
            system.name
        ));
    }
    // The system's `__compartment` field is non-null typed, but
    // `deserializeComp` returns `…Compartment | null`. A persisted
    // system always has a live compartment, so under TS we assert
    // non-null (`!`) to satisfy `--strict` (TS2322). JS is untyped.
    let nonnull = if is_ts { "!" } else { "" };
    restore_body.push_str(&format!(
        "{}.__compartment = deserializeComp(_parsed._compartment){};\n",
        target, nonnull
    ));
    restore_body.push_str(&format!("{}.__next_compartment = null;\n", target));
    if is_ts {
        restore_body.push_str(&format!(
            "{}._state_stack = (_parsed._state_stack || []).map((c: any) => deserializeComp(c));\n",
            target
        ));
    } else {
        restore_body.push_str(&format!(
            "{}._state_stack = (_parsed._state_stack || []).map((c) => deserializeComp(c));\n",
            target
        ));
    }
    restore_body.push_str(&format!("{}._context_stack = [];\n", target));

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (_, child_load) = child_persist_names(child_sys, "saveState", "restoreState");
            if nested_uses_new_contract(child_sys) {
                restore_body.push_str(&format!(
                    "if (_parsed.{1} != null) {{ {0}.{1} = new {2}(); {0}.{1}.{3}(JSON.stringify(_parsed.{1})); }} else {{ {0}.{1} = null; }}\n",
                    target, var.name, child_sys, child_load
                ));
            } else {
                restore_body.push_str(&format!(
                    "{0}.{1} = _parsed.{1} != null ? {2}.{3}(JSON.stringify(_parsed.{1})) : null;\n",
                    target, var.name, child_sys, child_load
                ));
            }
        } else {
            restore_body.push_str(&format!(
                "{}.{} = _revive(_parsed.{});\n",
                target, var.name, var.name
            ));
        }
    }

    if !uses_new_contract {
        restore_body.push_str("return instance;");
    }

    let (load_params, load_return, load_static) = if uses_new_contract {
        (
            vec![Param::new(&load_param_name).with_type("string")],
            None,
            false,
        )
    } else {
        (
            vec![Param::new(&load_param_name).with_type("string")],
            Some(system.name.clone()),
            true,
        )
    };
    methods.push(CodegenNode::Method {
        name: load_method_name.clone(),
        params: load_params,
        return_type: load_return,
        body: vec![CodegenNode::NativeBlock {
            code: restore_body,
            span: None,
        }],
        is_async: false,
        is_static: load_static,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    // Hybrid-registry completion hook: register a user type that the graph-seed
    // can't reach (no initializer, or the legacy Object.create restore path).
    // Never consulted for a type the seed already found; still closed-world (the
    // caller hands over a class reference — no name/global lookup).
    let mut hook_body = String::new();
    hook_body.push_str(&format!(
        "if (!{0}.__persistUserTypes) {0}.__persistUserTypes = new Map();\n",
        sys_ref
    ));
    hook_body.push_str(&format!(
        "{0}.__persistUserTypes.set(_cls.name, _cls);\n",
        sys_ref
    ));
    methods.push(CodegenNode::Method {
        name: "registerPersistType".to_string(),
        params: vec![Param::new("_cls").with_type("any")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: hook_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    methods
}
