//! C# persist codegen.
//!
//! `System.Text.Json` — type-ignorant typed restore via JSON
//! round-trip (`JsonSerializer.Serialize` then `Deserialize<T>`).
//! Per-state typed conversion (D10) reads the declared param type
//! verbatim, so framec doesn't have to parse generics or detect
//! container kinds — System.Text.Json reflection handles
//! primitives, `List<T>`, `Dictionary<K,V>`, nested structures, and
//! user types with `[JsonPropertyName]`.
//!
//! Legacy contract uses `RuntimeHelpers.GetUninitializedObject` to
//! bypass the constructor.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::codegen::codegen_utils::csharp_map_type;
use crate::frame_c::compiler::frame_ast::SystemAst;

use super::super::{child_persist_names, extract_tagged_system_name, nested_uses_new_contract};

pub(in crate::frame_c::compiler::codegen::interface_gen) fn generate(
    system: &SystemAst,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();
    let sys = &system.name;
    let compartment_class = format!("{}Compartment", sys);

    let uses_new_contract = system.uses_new_persist_contract();
    let save_method_name = system
        .save_op_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "SaveState".to_string());
    let load_method_name = system
        .load_op_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "RestoreState".to_string());
    let load_param_name = system
        .load_op_param_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "json".to_string());
    let target = if uses_new_contract {
        "this"
    } else {
        "__instance"
    };

    let mut ser_body = String::new();
    ser_body.push_str("if (comp == null) return null;\n");
    ser_body.push_str("var j = new Dictionary<string, object>();\n");
    ser_body.push_str("j[\"state\"] = comp.state;\n");
    // System.Text.Json serializes a `Dictionary<string, object>` value by its
    // DECLARED type (object) → an empty `{}` for a user struct, losing it on save.
    // Pre-serialize each value by its RUNTIME type into a JsonElement so the
    // concrete fields survive (restore re-decodes by the declared type).
    ser_body.push_str(
        "var __svopts = new System.Text.Json.JsonSerializerOptions { IncludeFields = true };\n",
    );
    ser_body.push_str("var sv = new Dictionary<string, object>();\n");
    ser_body.push_str("foreach (var __kv in comp.state_vars) { sv[__kv.Key] = __kv.Value == null ? null : System.Text.Json.JsonSerializer.SerializeToElement(__kv.Value, __kv.Value.GetType(), __svopts); }\n");
    ser_body.push_str("j[\"state_vars\"] = sv;\n");
    ser_body.push_str("j[\"state_args\"] = new List<object>(comp.state_args);\n");
    ser_body.push_str("j[\"enter_args\"] = new List<object>(comp.enter_args);\n");
    ser_body.push_str("j[\"parent\"] = __SerComp(comp.parent_compartment);\n");
    ser_body.push_str("return j;");

    methods.push(CodegenNode::Method {
        name: "__SerComp".to_string(),
        params: vec![Param::new("comp").with_type(&compartment_class)],
        return_type: Some("object".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: ser_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    let mut deser_body = String::new();
    deser_body.push_str("if (el.ValueKind == System.Text.Json.JsonValueKind.Null) return null;\n");
    deser_body.push_str(&format!(
        "var c = new {}(el.GetProperty(\"state\").GetString());\n",
        compartment_class
    ));
    deser_body.push_str("if (el.TryGetProperty(\"state_vars\", out var sv) && sv.ValueKind == System.Text.Json.JsonValueKind.Object) {\n");
    deser_body.push_str("    foreach (var kv in sv.EnumerateObject()) {\n");
    deser_body.push_str("        if (kv.Value.ValueKind == System.Text.Json.JsonValueKind.Number) { if (kv.Value.TryGetInt32(out int __ii)) c.state_vars[kv.Name] = __ii; else if (kv.Value.TryGetInt64(out long __il)) c.state_vars[kv.Name] = __il; else c.state_vars[kv.Name] = kv.Value.GetDouble(); }\n");
    deser_body.push_str("        else if (kv.Value.ValueKind == System.Text.Json.JsonValueKind.String) c.state_vars[kv.Name] = kv.Value.GetString();\n");
    deser_body.push_str("        else c.state_vars[kv.Name] = kv.Value.Clone();\n");
    deser_body.push_str("    }\n");
    deser_body.push_str("}\n");
    deser_body.push_str("if (el.TryGetProperty(\"state_args\", out var sa) && sa.ValueKind == System.Text.Json.JsonValueKind.Array) {\n");
    deser_body.push_str(
        "    foreach (var v in sa.EnumerateArray()) c.state_args.Add(__convertJsonValue(v));\n",
    );
    deser_body.push_str("}\n");
    deser_body.push_str("if (el.TryGetProperty(\"enter_args\", out var ea) && ea.ValueKind == System.Text.Json.JsonValueKind.Array) {\n");
    deser_body.push_str(
        "    foreach (var v in ea.EnumerateArray()) c.enter_args.Add(__convertJsonValue(v));\n",
    );
    deser_body.push_str("}\n");

    let cs_typed_conv = |declared_type: &str, idx: usize, slot: &str| -> String {
        let t = declared_type.trim();
        if t.is_empty() {
            return String::new();
        }
        format!(
            "    if (c.{slot}.Count > {idx} && c.{slot}[{idx}] != null) {{\n\
             \x20       try {{\n\
             \x20           var __raw = System.Text.Json.JsonSerializer.Serialize(c.{slot}[{idx}]);\n\
             \x20           c.{slot}[{idx}] = System.Text.Json.JsonSerializer.Deserialize<{t}>(__raw);\n\
             \x20       }} catch {{ /* leave generic value in place */ }}\n\
             \x20   }}\n"
        )
    };
    // State vars restore into a `Dictionary<string, object>`; an object value is
    // kept as a raw JsonElement (above), so re-serialize + Deserialize<T> into
    // the declared type BY NAME (scalars round-trip idempotently).
    let cs_typed_conv_named = |declared_type: &str, name: &str| -> String {
        let t = declared_type.trim();
        if t.is_empty() {
            return String::new();
        }
        format!(
            "    if (c.state_vars.ContainsKey(\"{name}\") && c.state_vars[\"{name}\"] != null) {{\n\
             \x20       try {{\n\
             \x20           var __raw = System.Text.Json.JsonSerializer.Serialize(c.state_vars[\"{name}\"], __svopts);\n\
             \x20           c.state_vars[\"{name}\"] = System.Text.Json.JsonSerializer.Deserialize<{t}>(__raw, __svopts);\n\
             \x20       }} catch {{ }}\n\
             \x20   }}\n"
        )
    };
    // RFC-0054 Phase A: per-state typed slots come from ONE manifest of raw Frame
    // type strings; C#'s mapping stays at consumption (cs_typed_conv{,_named}).
    let manifest = super::manifest::build_persist_manifest(system);
    // B1: manifest fingerprint — save writes `_manifest`, restore refuses (E751)
    // on drift (string compare after a plain parse, before any typed decode).
    let manifest_fp = super::emit::escape_double_quoted(&manifest.fingerprint());
    use super::emit::{emit_per_state_blocks, indexed_branch, named_branch};
    // A2: shared per-state guarded-block scaffold. C#'s arg guard carries the
    // one-time `// D10` header as a first-block side-effect (hence FnMut, reused
    // across the state_args + enter_args calls so the header emits exactly once).
    let mut any_per_state = false;
    let mut arg_guard = |state: &str, branch: &str| {
        let prefix = if !any_per_state {
            any_per_state = true;
            "// D10 per-state typed list conversion\n"
        } else {
            ""
        };
        format!("{}if (c.state == \"{}\") {{\n{}}}\n", prefix, state, branch)
    };
    emit_per_state_blocks(
        &mut deser_body,
        &manifest.states,
        |s| indexed_branch(&s.state_args, |t, i| cs_typed_conv(t, i, "state_args")),
        &mut arg_guard,
    );
    emit_per_state_blocks(
        &mut deser_body,
        &manifest.states,
        |s| indexed_branch(&s.enter_args, |t, i| cs_typed_conv(t, i, "enter_args")),
        &mut arg_guard,
    );
    // State vars: retype each declared var by name per state (see
    // cs_typed_conv_named). Runs per compartment, so every HSM-chain layer is
    // retyped by its own state. Own guard: IncludeFields so a user struct with
    // public fields round-trips (and no D10 header).
    emit_per_state_blocks(
        &mut deser_body,
        &manifest.states,
        |s| named_branch(&s.state_vars, |name, t| cs_typed_conv_named(t, name)),
        |state, branch| {
            format!(
                "if (c.state == \"{}\") {{\n    var __svopts = new System.Text.Json.JsonSerializerOptions {{ IncludeFields = true }};\n{}}}\n",
                state, branch
            )
        },
    );

    deser_body.push_str("if (el.TryGetProperty(\"parent\", out var p) && p.ValueKind != System.Text.Json.JsonValueKind.Null) {\n");
    deser_body.push_str("    c.parent_compartment = __DeserComp(p);\n");
    deser_body.push_str("}\n");
    deser_body.push_str("return c;");

    methods.push(CodegenNode::Method {
        name: "__DeserComp".to_string(),
        params: vec![Param::new("el").with_type("System.Text.Json.JsonElement")],
        return_type: Some(compartment_class.clone()),
        body: vec![CodegenNode::NativeBlock {
            code: deser_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    let mut conv_body = String::new();
    conv_body.push_str("if (v.ValueKind == System.Text.Json.JsonValueKind.Number) {\n");
    conv_body.push_str("    if (v.TryGetInt32(out int __i)) return __i;\n");
    conv_body.push_str("    if (v.TryGetInt64(out long __l)) return __l;\n");
    conv_body.push_str("    return v.GetDouble();\n");
    conv_body.push_str("}\n");
    conv_body.push_str(
        "if (v.ValueKind == System.Text.Json.JsonValueKind.String) return v.GetString();\n",
    );
    conv_body.push_str("if (v.ValueKind == System.Text.Json.JsonValueKind.True) return true;\n");
    conv_body.push_str("if (v.ValueKind == System.Text.Json.JsonValueKind.False) return false;\n");
    conv_body.push_str("if (v.ValueKind == System.Text.Json.JsonValueKind.Array) {\n");
    conv_body.push_str("    var __list = new System.Collections.Generic.List<object>();\n");
    conv_body.push_str(
        "    foreach (var __ne in v.EnumerateArray()) __list.Add(__convertJsonValue(__ne));\n",
    );
    conv_body.push_str("    return __list;\n");
    conv_body.push_str("}\n");
    conv_body.push_str("if (v.ValueKind == System.Text.Json.JsonValueKind.Object) {\n");
    conv_body.push_str(
        "    var __dict = new System.Collections.Generic.Dictionary<string, object>();\n",
    );
    conv_body.push_str("    foreach (var __prop in v.EnumerateObject()) __dict[__prop.Name] = __convertJsonValue(__prop.Value);\n");
    conv_body.push_str("    return __dict;\n");
    conv_body.push_str("}\n");
    conv_body.push_str("return v.ToString();");
    methods.push(CodegenNode::Method {
        name: "__convertJsonValue".to_string(),
        params: vec![Param::new("v").with_type("System.Text.Json.JsonElement")],
        return_type: Some("object".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: conv_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    let mut save_body = String::new();
    save_body.push_str("if (_context_stack.Count > 0) throw new System.Exception(\"E700: system not quiescent\");\n");
    save_body.push_str("var __j = new Dictionary<string, object>();\n");
    save_body.push_str(&format!("__j[\"_manifest\"] = \"{}\";\n", manifest_fp));
    save_body.push_str("__j[\"_compartment\"] = __SerComp(__compartment);\n");
    save_body.push_str("var __stack = new List<object>();\n");
    save_body.push_str("foreach (var c in _state_stack) { __stack.Add(__SerComp(c)); }\n");
    save_body.push_str("__j[\"_state_stack\"] = __stack;\n");

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (child_save, _) = child_persist_names(child_sys, "SaveState", "RestoreState");
            save_body.push_str(&format!(
                "__j[\"{0}\"] = {0} != null ? System.Text.Json.JsonDocument.Parse({0}.{1}()).RootElement.Clone() : (object)null;\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("__j[\"{}\"] = {};\n", var.name, var.name));
        }
    }

    // IncludeFields = true: System.Text.Json ignores public *fields* by default, so
    // field-based user types (structs like Vector2 / System.Numerics) would serialize
    // as {} and silently restore as default(T) (#165). The flag is strictly widening —
    // property-based types are unaffected — and must match the deserialize side below.
    save_body.push_str("var __opts = new System.Text.Json.JsonSerializerOptions { IncludeFields = true, TypeInfoResolver = new System.Text.Json.Serialization.Metadata.DefaultJsonTypeInfoResolver() };\n");
    save_body.push_str("return System.Text.Json.JsonSerializer.Serialize(__j, __opts);");

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

    let mut restore_body = String::new();
    restore_body.push_str(&format!(
        "var __doc = System.Text.Json.JsonDocument.Parse({});\n",
        load_param_name
    ));
    restore_body.push_str("var __root = __doc.RootElement;\n");
    // B1: refuse a drifted snapshot before any typed decode. The check variable
    // must NOT be `__m`: the per-field restore below names its temp `__<fieldname>`,
    // so a persisted domain field named `m` would emit a second `__m` in this same
    // method scope (CS0128). `__frameManifest` cannot collide with a field name.
    restore_body.push_str(&format!(
        "if (!__root.TryGetProperty(\"_manifest\", out var __frameManifest) || __frameManifest.GetString() != \"{}\") throw new System.Exception(\"E751: persist restore refused - snapshot schema does not match this program\");\n",
        manifest_fp
    ));
    if !uses_new_contract {
        restore_body.push_str(&format!(
            "var __instance = ({0})System.Runtime.CompilerServices.RuntimeHelpers.GetUninitializedObject(typeof({0}));\n",
            sys,
        ));
        restore_body.push_str(&format!(
            "__instance._state_stack = new List<{}>();\n",
            compartment_class,
        ));
        restore_body.push_str(&format!(
            "__instance._context_stack = new List<{}FrameContext>();\n",
            sys,
        ));
    }
    restore_body.push_str(&format!(
        "{}.__compartment = __DeserComp(__root.GetProperty(\"_compartment\"));\n",
        target
    ));
    restore_body.push_str("if (__root.TryGetProperty(\"_state_stack\", out var __stack)) {\n");
    restore_body.push_str(&format!(
        "    {}._state_stack = new List<{}>();\n",
        target, compartment_class
    ));
    restore_body.push_str(&format!(
        "    foreach (var item in __stack.EnumerateArray()) {{ {}._state_stack.Add(__DeserComp(item)); }}\n",
        target
    ));
    restore_body.push_str("}\n");

    // Must mirror the serialize side (#165): without IncludeFields the deserializer
    // maps nothing onto field-based user structs and returns default(T). Only the
    // STJ-deserialized (non-nested-system) vars use it, so skip the local when there
    // are none, to avoid an unused-variable in the generated code.
    let needs_deser_opts = system.domain.iter().any(|var| {
        !var.attributes.iter().any(|a| a.name == "no_persist")
            && extract_tagged_system_name(var.initializer_text.as_deref().unwrap_or("")).is_none()
    });
    if needs_deser_opts {
        restore_body.push_str("var __opts = new System.Text.Json.JsonSerializerOptions { IncludeFields = true, TypeInfoResolver = new System.Text.Json.Serialization.Metadata.DefaultJsonTypeInfoResolver() };\n");
    }

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (_, child_load) = child_persist_names(child_sys, "SaveState", "RestoreState");
            let body = if nested_uses_new_contract(child_sys) {
                format!(
                    "if (__root.TryGetProperty(\"{name}\", out var __{name})) {{ if (__{name}.ValueKind != System.Text.Json.JsonValueKind.Null) {{ {tgt}.{name} = new {child}(); {tgt}.{name}.{load}(__{name}.GetRawText()); }} }}\n",
                    tgt = target,
                    name = var.name,
                    child = child_sys,
                    load = child_load
                )
            } else {
                format!(
                    "if (__root.TryGetProperty(\"{name}\", out var __{name})) {{ if (__{name}.ValueKind != System.Text.Json.JsonValueKind.Null) {{ {tgt}.{name} = {child}.{load}(__{name}.GetRawText()); }} }}\n",
                    tgt = target,
                    name = var.name,
                    child = child_sys,
                    load = child_load
                )
            };
            restore_body.push_str(&body);
        } else {
            let declared = match &var.var_type {
                crate::frame_c::compiler::frame_ast::Type::Custom(t) => csharp_map_type(t),
                _ => "object".to_string(),
            };
            restore_body.push_str(&format!(
                "if (__root.TryGetProperty(\"{name}\", out var __{name})) {{ try {{ {tgt}.{name} = System.Text.Json.JsonSerializer.Deserialize<{t}>(__{name}.GetRawText(), __opts); }} catch {{ }} }}\n",
                tgt = target,
                name = var.name,
                t = declared
            ));
        }
    }

    if !uses_new_contract {
        restore_body.push_str("return __instance;");
    }

    let (load_return, load_static) = if uses_new_contract {
        (None, false)
    } else {
        (Some(sys.clone()), true)
    };
    methods.push(CodegenNode::Method {
        name: load_method_name.clone(),
        params: vec![Param::new(&load_param_name).with_type("string")],
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
