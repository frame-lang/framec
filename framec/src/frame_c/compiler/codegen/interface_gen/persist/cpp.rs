//! C++ persist codegen.
//!
//! `nlohmann::json` wire format with `std::any`-tagged compartment
//! args. Type-ignorant typed restore: framec emits the declared
//! type `T` verbatim into `std::any_cast<T>` and `nlohmann::json::
//! get<T>()`; nlohmann's ADL handles primitives, `std::vector`,
//! `std::map`, `std::unordered_map`, `std::string`, and user types
//! with `to_json`/`from_json` overloads — no type-string parsing
//! in framec.
//!
//! Per-state typed branches for both state_args and enter_args (D8
//! / D13 fixes) so float and vector args round-trip through the
//! correct dispatcher rather than the fallback int/double scalar
//! path.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::codegen::codegen_utils::cpp_map_type;
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
        .unwrap_or_else(|| "save_state".to_string());
    let load_method_name = system
        .load_op_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "restore_state".to_string());
    let load_param_name = system
        .load_op_param_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "json".to_string());
    let target = if uses_new_contract {
        "(*this)"
    } else {
        "__instance"
    };

    // RFC-0054 Phase A: the per-state typed slots (state vars, state args, enter
    // args) come from ONE manifest of raw Frame type strings. C++'s mapping stays
    // at consumption: an empty raw (Frame `Unknown`) becomes `int` for a state var
    // (the C++ default) and the empty string for an arg, exactly as before.
    let manifest = super::manifest::build_persist_manifest(system);
    // B1: manifest fingerprint — save writes `_manifest`, restore refuses (E751)
    // on drift (string compare after a plain parse, before decode; dual-mode throw
    // / abort under -fno-exceptions, mirroring E700).
    let manifest_fp = super::emit::escape_double_quoted(&manifest.fingerprint());
    let all_state_vars: Vec<(&str, &str, &str)> = manifest
        .states
        .iter()
        .flat_map(|s| {
            s.state_vars.iter().map(move |(name, raw)| {
                let type_str = if raw.is_empty() { "int" } else { raw.as_str() };
                (s.name.as_str(), name.as_str(), type_str)
            })
        })
        .collect();

    let cpp_state_arg_decls: Vec<(String, Vec<String>)> = manifest
        .states
        .iter()
        .map(|s| (s.name.clone(), s.state_args.clone()))
        .collect();
    let cpp_enter_arg_decls: Vec<(String, Vec<String>)> = manifest
        .states
        .iter()
        .map(|s| (s.name.clone(), s.enter_args.clone()))
        .collect();

    // save_state
    let mut save_body = String::new();
    // E700 is a PRECONDITION violation (save() called mid-dispatch) — a proper
    // use of an exception (RFC-0049 R2), not control flow. Keep the throw where
    // exceptions exist (exception-enabled builds are byte-identical) and add the
    // R3 fallback for `-fno-exceptions` (Godot web): a fail-fast abort, which is
    // arguably the more correct handling for a programmer-error precondition.
    save_body.push_str(
        "if (!_context_stack.empty()) {\n\
         #if defined(__cpp_exceptions) || defined(__EXCEPTIONS)\n\
         throw std::runtime_error(\"E700: system not quiescent\");\n\
         #else\n\
         std::fprintf(stderr, \"E700: system not quiescent\\n\"); std::abort();\n\
         #endif\n\
         }\n",
    );

    save_body.push_str(&format!(
        "std::function<nlohmann::json(const {0}*)> __ser = [&](const {0}* c) -> nlohmann::json {{\n",
        compartment_class
    ));
    save_body.push_str("    if (!c) return nullptr;\n");
    save_body.push_str("    nlohmann::json __cj;\n");
    save_body.push_str("    __cj[\"state\"] = c->state;\n");
    save_body.push_str("    nlohmann::json __sv;\n");
    save_body.push_str("    for (auto& [k, v] : c->state_vars) {\n");
    for (_state, var_name, var_type) in &all_state_vars {
        let cpp_type = cpp_map_type(var_type);
        // RFC-0049 R1: type discovery is a QUERY, not an error — use the
        // non-throwing pointer `any_cast<T>(&v)` (returns null on mismatch)
        // instead of catching a thrown exception to probe the type.
        save_body.push_str(&format!(
            "        if (k == \"{}\") {{ if (auto* __p = std::any_cast<{}>(&v)) __sv[k] = *__p; }}\n",
            var_name, cpp_type
        ));
    }
    save_body.push_str("    }\n");
    save_body.push_str("    __cj[\"state_vars\"] = __sv;\n");
    save_body.push_str("    nlohmann::json __sa = nlohmann::json::array();\n");
    save_body.push_str("    {\n");
    for (state_name, types) in &cpp_state_arg_decls {
        if types.is_empty() {
            continue;
        }
        save_body.push_str(&format!("    if (c->state == \"{}\") {{\n", state_name));
        for (i, t) in types.iter().enumerate() {
            if t.is_empty() {
                save_body.push_str(&format!(
                    "        if (c->state_args.size() > {i}) {{ if (auto* __p = std::any_cast<int>(&c->state_args[{i}])) __sa.push_back(*__p); else if (auto* __p = std::any_cast<double>(&c->state_args[{i}])) __sa.push_back(*__p); else __sa.push_back(nullptr); }}\n"
                ));
            } else {
                save_body.push_str(&format!(
                    "        if (c->state_args.size() > {i}) {{ if (auto* __p = std::any_cast<{t}>(&c->state_args[{i}])) __sa.push_back(nlohmann::json(*__p)); else __sa.push_back(nullptr); }}\n"
                ));
            }
        }
        save_body.push_str("    } else \n");
    }
    save_body.push_str("    {\n");
    save_body.push_str("        for (const auto& v : c->state_args) { if (auto* __p = std::any_cast<int>(&v)) __sa.push_back(*__p); else if (auto* __p = std::any_cast<double>(&v)) __sa.push_back(*__p); else __sa.push_back(nullptr); }\n");
    save_body.push_str("    }\n");
    save_body.push_str("    }\n");
    save_body.push_str("    __cj[\"state_args\"] = __sa;\n");
    save_body.push_str("    nlohmann::json __ea = nlohmann::json::array();\n");
    save_body.push_str("    {\n");
    for (state_name, types) in &cpp_enter_arg_decls {
        if types.is_empty() {
            continue;
        }
        save_body.push_str(&format!("    if (c->state == \"{}\") {{\n", state_name));
        for (i, t) in types.iter().enumerate() {
            if t.is_empty() {
                save_body.push_str(&format!(
                    "        if (c->enter_args.size() > {i}) {{ if (auto* __p = std::any_cast<int>(&c->enter_args[{i}])) __ea.push_back(*__p); else if (auto* __p = std::any_cast<double>(&c->enter_args[{i}])) __ea.push_back(*__p); else __ea.push_back(nullptr); }}\n"
                ));
            } else {
                save_body.push_str(&format!(
                    "        if (c->enter_args.size() > {i}) {{ if (auto* __p = std::any_cast<{t}>(&c->enter_args[{i}])) __ea.push_back(nlohmann::json(*__p)); else __ea.push_back(nullptr); }}\n"
                ));
            }
        }
        save_body.push_str("    } else\n");
    }
    save_body.push_str("    {\n");
    save_body.push_str("        for (const auto& v : c->enter_args) { if (auto* __p = std::any_cast<int>(&v)) __ea.push_back(*__p); else if (auto* __p = std::any_cast<double>(&v)) __ea.push_back(*__p); else __ea.push_back(nullptr); }\n");
    save_body.push_str("    }\n");
    save_body.push_str("    }\n");
    save_body.push_str("    __cj[\"enter_args\"] = __ea;\n");
    save_body.push_str("    __cj[\"parent\"] = __ser(c->parent_compartment.get());\n");
    save_body.push_str("    return __cj;\n");
    save_body.push_str("};\n");

    save_body.push_str("nlohmann::json __j;\n");
    save_body.push_str(&format!("__j[\"_manifest\"] = \"{}\";\n", manifest_fp));
    save_body.push_str("__j[\"_compartment\"] = __ser(__compartment.get());\n");

    save_body.push_str("nlohmann::json __stack = nlohmann::json::array();\n");
    save_body.push_str("for (auto& c : _state_stack) { __stack.push_back(__ser(c.get())); }\n");
    save_body.push_str("__j[\"_state_stack\"] = __stack;\n");

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (child_save, _) = child_persist_names(child_sys, "save_state", "restore_state");
            save_body.push_str(&format!(
                "__j[\"{0}\"] = {0} ? nlohmann::json::parse({0}->{1}()) : nlohmann::json(nullptr);\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("__j[\"{}\"] = {};\n", var.name, var.name));
        }
    }

    save_body.push_str("return __j.dump();");

    methods.push(CodegenNode::Method {
        name: save_method_name.clone(),
        params: vec![],
        return_type: Some("std::string".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: save_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    // restore_state
    let mut restore_body = String::new();
    restore_body.push_str(&format!(
        "std::function<std::unique_ptr<{0}>(const nlohmann::json&)> __deser = [&](const nlohmann::json& d) -> std::unique_ptr<{0}> {{\n",
        compartment_class
    ));
    restore_body.push_str("    if (d.is_null()) return nullptr;\n");
    restore_body.push_str(&format!(
        "    auto c = std::make_unique<{}>(std::string(d[\"state\"]));\n",
        compartment_class
    ));
    restore_body.push_str("    if (d.contains(\"state_vars\")) {\n");
    restore_body.push_str("        auto& sv = d[\"state_vars\"];\n");
    for (_state, var_name, var_type) in &all_state_vars {
        let cpp_type = cpp_map_type(var_type);
        restore_body.push_str(&format!(
            "        if (sv.contains(\"{0}\")) {{ c->state_vars[\"{0}\"] = std::any(sv[\"{0}\"].get<{1}>()); }}\n",
            var_name, cpp_type
        ));
    }
    restore_body.push_str("    }\n");
    restore_body
        .push_str("    if (d.contains(\"state_args\") && d[\"state_args\"].is_array()) {\n");
    restore_body.push_str("        const auto& __sa = d[\"state_args\"];\n");
    for (state_name, types) in &cpp_state_arg_decls {
        if types.is_empty() {
            continue;
        }
        restore_body.push_str(&format!("        if (c->state == \"{}\") {{\n", state_name));
        for (i, t) in types.iter().enumerate() {
            if t.is_empty() {
                restore_body.push_str(&format!(
                    "            if (__sa.size() > {i}) {{ if (__sa[{i}].is_number_integer()) c->state_args.push_back(std::any(__sa[{i}].get<int>())); else if (__sa[{i}].is_number_float()) c->state_args.push_back(std::any(__sa[{i}].get<double>())); }}\n"
                ));
            } else {
                restore_body.push_str(&format!(
                    "            if (__sa.size() > {i}) {{\n\
                     #if defined(__cpp_exceptions) || defined(__EXCEPTIONS)\n\
                     try {{ c->state_args.push_back(std::any(__sa[{i}].get<{t}>())); }} catch(...) {{ }}\n\
                     #else\n\
                     if (!__sa[{i}].is_null()) c->state_args.push_back(std::any(__sa[{i}].get<{t}>()));\n\
                     #endif\n\
                     }}\n"
                ));
            }
        }
        restore_body.push_str("        } else \n");
    }
    restore_body.push_str("        {\n");
    restore_body.push_str("            for (const auto& v : __sa) {\n");
    restore_body.push_str("                if (v.is_number_integer()) c->state_args.push_back(std::any(v.get<int>()));\n");
    restore_body.push_str("                else if (v.is_number_float()) c->state_args.push_back(std::any(v.get<double>()));\n");
    restore_body.push_str("            }\n");
    restore_body.push_str("        }\n");
    restore_body.push_str("    }\n");
    restore_body
        .push_str("    if (d.contains(\"enter_args\") && d[\"enter_args\"].is_array()) {\n");
    restore_body.push_str("        const auto& __ea = d[\"enter_args\"];\n");
    for (state_name, types) in &cpp_enter_arg_decls {
        if types.is_empty() {
            continue;
        }
        restore_body.push_str(&format!("        if (c->state == \"{}\") {{\n", state_name));
        for (i, t) in types.iter().enumerate() {
            if t.is_empty() {
                restore_body.push_str(&format!(
                    "            if (__ea.size() > {i}) {{ if (__ea[{i}].is_number_integer()) c->enter_args.push_back(std::any(__ea[{i}].get<int>())); else if (__ea[{i}].is_number_float()) c->enter_args.push_back(std::any(__ea[{i}].get<double>())); }}\n"
                ));
            } else {
                restore_body.push_str(&format!(
                    "            if (__ea.size() > {i}) {{\n\
                     #if defined(__cpp_exceptions) || defined(__EXCEPTIONS)\n\
                     try {{ c->enter_args.push_back(std::any(__ea[{i}].get<{t}>())); }} catch(...) {{ }}\n\
                     #else\n\
                     if (!__ea[{i}].is_null()) c->enter_args.push_back(std::any(__ea[{i}].get<{t}>()));\n\
                     #endif\n\
                     }}\n"
                ));
            }
        }
        restore_body.push_str("        } else \n");
    }
    restore_body.push_str("        {\n");
    restore_body.push_str("            for (const auto& v : __ea) {\n");
    restore_body.push_str("                if (v.is_number_integer()) c->enter_args.push_back(std::any(v.get<int>()));\n");
    restore_body.push_str("                else if (v.is_number_float()) c->enter_args.push_back(std::any(v.get<double>()));\n");
    restore_body.push_str("            }\n");
    restore_body.push_str("        }\n");
    restore_body.push_str("    }\n");
    restore_body.push_str("    if (d.contains(\"parent\") && !d[\"parent\"].is_null()) {\n");
    restore_body.push_str("        c->parent_compartment = __deser(d[\"parent\"]);\n");
    restore_body.push_str("    }\n");
    restore_body.push_str("    return c;\n");
    restore_body.push_str("};\n");

    restore_body.push_str(&format!(
        "auto __j = nlohmann::json::parse({});\n",
        load_param_name
    ));
    // B1: refuse a drifted snapshot before any typed decode (dual-mode like E700).
    restore_body.push_str(&format!(
        "if (!__j.contains(\"_manifest\") || __j[\"_manifest\"] != \"{}\") {{\n\
         #if defined(__cpp_exceptions) || defined(__EXCEPTIONS)\n\
         throw std::runtime_error(\"E751: persist restore refused - snapshot schema does not match this program\");\n\
         #else\n\
         std::fprintf(stderr, \"E751: persist restore refused - snapshot schema does not match this program\\n\"); std::abort();\n\
         #endif\n\
         }}\n",
        manifest_fp
    ));
    let _ = uses_new_contract;
    restore_body.push_str(&format!(
        "{}.__compartment = __deser(__j[\"_compartment\"]);\n",
        target
    ));

    restore_body.push_str("if (__j.contains(\"_state_stack\")) {\n");
    restore_body.push_str("    for (auto& __sc : __j[\"_state_stack\"]) {\n");
    restore_body.push_str(&format!(
        "        {}._state_stack.push_back(__deser(__sc));\n",
        target
    ));
    restore_body.push_str("    }\n");
    restore_body.push_str("}\n");

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (_, child_load) = child_persist_names(child_sys, "save_state", "restore_state");
            if nested_uses_new_contract(child_sys) {
                restore_body.push_str(&format!(
                    "if (__j.contains(\"{0}\") && !__j[\"{0}\"].is_null()) {{ {tgt}.{0} = std::make_shared<{1}>(); {tgt}.{0}->{load}(__j[\"{0}\"].dump()); }}\n",
                    var.name, child_sys, tgt = target, load = child_load
                ));
            } else {
                restore_body.push_str(&format!(
                    "if (__j.contains(\"{0}\") && !__j[\"{0}\"].is_null()) {{ {tgt}.{0} = std::make_shared<{1}>({1}::{load}(__j[\"{0}\"].dump())); }}\n",
                    var.name, child_sys, tgt = target, load = child_load
                ));
            }
        } else {
            restore_body.push_str(&format!(
                "if (__j.contains(\"{0}\")) {{ __j[\"{0}\"].get_to({tgt}.{0}); }}\n",
                var.name,
                tgt = target
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
        params: vec![Param::new(&load_param_name).with_type("const std::string&")],
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
