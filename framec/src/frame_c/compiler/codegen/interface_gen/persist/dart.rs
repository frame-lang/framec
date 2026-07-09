//! Dart persist codegen.
//!
//! `jsonEncode` / `jsonDecode` via `dart:convert`, with per-state
//! typed restore via comprehensions (`<T>[for ... ]` /
//! `<K,V>{for ... }`) so the rehydrated `state_args` / `enter_args`
//! lists carry reified element types — required for handler bodies
//! that index without defensive casts. See `dart_types` module for
//! the rationale.
//!
//! Legacy contract uses a `Sys._restore()` named constructor that
//! bypasses the regular ctor's `$>` enter dispatch. New contract
//! mutates `this` in place — the existing instance's ctor already
//! ran, so the start-state enter has fired once (acceptable per the
//! RFC amendment's "$S0 enter on restore" trade-off).

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::frame_ast::SystemAst;

use super::super::{
    child_persist_names, dart_conv_expr, extract_tagged_system_name, nested_uses_new_contract,
    parse_dart_type,
};

pub(in crate::frame_c::compiler::codegen::interface_gen) fn generate(
    system: &SystemAst,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();
    let compartment_type = format!("{}Compartment", system.name);

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

    // B1: manifest fingerprint — save writes `_manifest`, restore refuses (E751)
    // on drift (string compare after a plain jsonDecode, before any typed decode).
    // Built here (before save_body) so both save and restore can reference it.
    let manifest_fp = super::emit::escape_double_quoted(
        &super::manifest::build_persist_manifest(system).fingerprint(),
    );

    // save_state
    let mut save_body = String::new();
    save_body.push_str(
        "if (_context_stack.isNotEmpty) throw Exception(\"E700: system not quiescent\");\n",
    );
    save_body.push_str(&format!(
        "Map<String, dynamic>? serializeComp({}? comp) {{\n",
        compartment_type
    ));
    save_body.push_str("    if (comp == null) return null;\n");
    save_body.push_str("    return {\n");
    save_body.push_str("        'state': comp.state,\n");
    save_body.push_str("        'state_args': List<dynamic>.from(comp.state_args),\n");
    save_body.push_str("        'state_vars': Map<String, dynamic>.from(comp.state_vars),\n");
    save_body.push_str("        'enter_args': List<dynamic>.from(comp.enter_args),\n");
    save_body.push_str("        'exit_args': List<dynamic>.from(comp.exit_args),\n");
    save_body.push_str("        'forward_event': comp.forward_event,\n");
    save_body.push_str("        'parent_compartment': serializeComp(comp.parent_compartment),\n");
    save_body.push_str("    };\n");
    save_body.push_str("}\n");
    save_body.push_str("return jsonEncode({\n");
    save_body.push_str(&format!("    '_manifest': \"{}\",\n", manifest_fp));
    save_body.push_str("    '_compartment': serializeComp(this.__compartment),\n");
    save_body
        .push_str("    '_state_stack': this._state_stack.map((c) => serializeComp(c)).toList(),\n");
    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (child_save, _) = child_persist_names(child_sys, "saveState", "restoreState");
            save_body.push_str(&format!(
                "    '{0}': this.{0} != null ? jsonDecode(this.{0}.{1}()) : null,\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("    '{}': this.{},\n", var.name, var.name));
        }
    }
    save_body.push_str("});");

    methods.push(CodegenNode::Method {
        name: save_method_name.clone(),
        params: vec![],
        return_type: Some("String".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: save_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    // _restore named constructor (legacy only)
    if !uses_new_contract {
        methods.push(CodegenNode::NativeBlock {
            code: format!(
                "{system}._restore() : __compartment = {comp}(\"\"), __next_compartment = null {{\n\
                 \x20   _state_stack = [];\n\
                 \x20   _context_stack = [];\n\
                 }}",
                system = system.name,
                comp = compartment_type,
            ),
            span: None,
        });
    }

    // Per-state typed restore data. RFC-0054 Phase A: raw Frame type strings come
    // from ONE manifest; Dart's mapping (empty ⇒ Unknown ⇒ `dynamic`, else the
    // trimmed type string) and its non-empty filter stay here at consumption.
    let manifest = super::manifest::build_persist_manifest(system);
    let dart_ty = |raw: &str| -> String {
        if raw.is_empty() {
            "dynamic".to_string()
        } else {
            raw.trim().to_string()
        }
    };
    let dart_state_param_types: Vec<(String, Vec<String>)> = manifest
        .states
        .iter()
        .filter(|s| !s.state_args.is_empty())
        .map(|s| {
            let types: Vec<String> = s.state_args.iter().map(|raw| dart_ty(raw)).collect();
            (s.name.clone(), types)
        })
        .collect();

    // State vars carry user types but restore into `Map<String, dynamic>`, so a
    // user value stays a plain Map and a method call on it throws at runtime.
    // Re-decode each declared state var into its type BY NAME per state (via
    // dart_conv_expr → the type's fromJson route).
    let dart_state_var_types: Vec<(String, Vec<(String, String)>)> = manifest
        .states
        .iter()
        .filter(|s| !s.state_vars.is_empty())
        .map(|s| {
            let vars: Vec<(String, String)> = s
                .state_vars
                .iter()
                .map(|(name, raw)| (name.clone(), dart_ty(raw)))
                .collect();
            (s.name.clone(), vars)
        })
        .collect();

    let mut restore_body = String::new();
    restore_body.push_str(&format!(
        "{}? deserializeComp(dynamic data) {{\n",
        compartment_type
    ));
    restore_body.push_str("    if (data == null || data is! Map) return null;\n");
    restore_body.push_str(&format!(
        "    final comp = {}(data['state'] as String);\n",
        compartment_type
    ));
    restore_body
        .push_str("    comp.state_vars = Map<String, dynamic>.from(data['state_vars'] ?? {});\n");
    restore_body.push_str("    final __saRaw = (data['state_args'] as List?) ?? <dynamic>[];\n");
    restore_body.push_str("    final __eaRaw = (data['enter_args'] as List?) ?? <dynamic>[];\n");
    restore_body
        .push_str("    comp.exit_args = List<dynamic>.from(data['exit_args'] ?? <dynamic>[]);\n");
    if !dart_state_param_types.is_empty() || !dart_state_var_types.is_empty() {
        let mut state_names: Vec<String> = Vec::new();
        for (s, _) in &dart_state_param_types {
            if !state_names.contains(s) {
                state_names.push(s.clone());
            }
        }
        for (s, _) in &dart_state_var_types {
            if !state_names.contains(s) {
                state_names.push(s.clone());
            }
        }
        restore_body.push_str("    switch (comp.state) {\n");
        for state_name in &state_names {
            restore_body.push_str(&format!("        case '{}':\n", state_name));
            if let Some((_, param_types)) =
                dart_state_param_types.iter().find(|(s, _)| s == state_name)
            {
                for (i, ty_str) in param_types.iter().enumerate() {
                    let parsed = parse_dart_type(ty_str);
                    let conv_sa = dart_conv_expr(&parsed, &format!("__saRaw[{i}]"));
                    let conv_ea = dart_conv_expr(&parsed, &format!("__eaRaw[{i}]"));
                    restore_body.push_str(&format!(
                        "            if (__saRaw.length > {i}) comp.state_args.add({conv_sa});\n"
                    ));
                    restore_body.push_str(&format!(
                        "            if (__eaRaw.length > {i}) comp.enter_args.add({conv_ea});\n"
                    ));
                }
            }
            if let Some((_, vars)) = dart_state_var_types.iter().find(|(s, _)| s == state_name) {
                for (name, ty_str) in vars {
                    let parsed = parse_dart_type(ty_str);
                    let conv = dart_conv_expr(&parsed, &format!("comp.state_vars['{name}']"));
                    restore_body.push_str(&format!(
                        "            if (comp.state_vars.containsKey('{name}')) comp.state_vars['{name}'] = {conv};\n"
                    ));
                }
            }
            restore_body.push_str("            break;\n");
        }
        restore_body.push_str("        default:\n");
        restore_body.push_str("            comp.state_args.addAll(__saRaw);\n");
        restore_body.push_str("            comp.enter_args.addAll(__eaRaw);\n");
        restore_body.push_str("            break;\n");
        restore_body.push_str("    }\n");
    } else {
        restore_body.push_str("    comp.state_args.addAll(__saRaw);\n");
        restore_body.push_str("    comp.enter_args.addAll(__eaRaw);\n");
    }
    restore_body.push_str("    comp.forward_event = data['forward_event'];\n");
    restore_body
        .push_str("    comp.parent_compartment = deserializeComp(data['parent_compartment']);\n");
    restore_body.push_str("    return comp;\n");
    restore_body.push_str("}\n");
    restore_body.push_str(&format!(
        "final _parsed = jsonDecode({}) as Map<String, dynamic>;\n",
        load_param_name
    ));
    // B1: refuse a drifted snapshot before any typed decode.
    restore_body.push_str(&format!(
        "if (_parsed['_manifest'] != \"{}\") throw Exception(\"E751: persist restore refused - snapshot schema does not match this program\");\n",
        manifest_fp
    ));
    if !uses_new_contract {
        restore_body.push_str(&format!("final instance = {}._restore();\n", system.name));
    }
    restore_body.push_str(&format!(
        "{}.__compartment = deserializeComp(_parsed['_compartment'])!;\n",
        target
    ));
    restore_body.push_str(&format!("{}.__next_compartment = null;\n", target));
    restore_body.push_str(&format!(
        "{}._state_stack = (_parsed['_state_stack'] as List?)?.map((c) => deserializeComp(c)!).toList() ?? <{}>[];\n",
        target, compartment_type
    ));
    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (_, child_load) = child_persist_names(child_sys, "saveState", "restoreState");
            if nested_uses_new_contract(child_sys) {
                restore_body.push_str(&format!(
                    "{0}.{1} = {2}(); if (_parsed['{1}'] != null) {0}.{1}.{3}(jsonEncode(_parsed['{1}']));\n",
                    target, var.name, child_sys, child_load
                ));
                continue;
            }
            restore_body.push_str(&format!(
                "{0}.{1} = _parsed['{1}'] != null ? {2}.{3}(jsonEncode(_parsed['{1}'])) : {2}();\n",
                target, var.name, child_sys, child_load
            ));
        } else {
            let ty = match &var.var_type {
                crate::frame_c::compiler::frame_ast::Type::Custom(s) => s.trim(),
                _ => "dynamic",
            };
            let parsed = parse_dart_type(ty);
            let conv = dart_conv_expr(&parsed, &format!("_parsed['{}']", var.name));
            restore_body.push_str(&format!("{}.{} = {};\n", target, var.name, conv));
        }
    }
    if !uses_new_contract {
        restore_body.push_str("return instance;");
    }

    let (load_return, load_static) = if uses_new_contract {
        (None, false)
    } else {
        (Some(system.name.clone()), true)
    };
    methods.push(CodegenNode::Method {
        name: load_method_name.clone(),
        params: vec![Param::new(&load_param_name).with_type("String")],
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

    methods
}
