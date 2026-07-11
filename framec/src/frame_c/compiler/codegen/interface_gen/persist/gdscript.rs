//! GDScript persist codegen.
//!
//! GDScript fidelity-exception: wire format is Godot binary Variant
//! (`var_to_bytes` / `bytes_to_var`), NOT JSON. The brief
//! JSON-for-all migration was reverted after a real fidelity bug:
//! Godot's `JSON.parse_string` returns every JSON number as
//! `float`, erasing the `int` vs `float` distinction Variant
//! draws. A persisted `int`-typed domain field or list element
//! came back as `float`, and `Array.has(typed_int)` after restore
//! returned false even when the value was present (the list held
//! floats). `var_to_bytes` round-trips Variants exactly. Mirrors
//! Erlang's ETF and Lua's serpent fidelity-exception rationale.
//! See `docs/per_language_guides/gdscript.md`.
//!
//! Compartment chain serialization is iterative because GDScript
//! lambdas can't recurse: collect the chain into an array, then
//! build dicts bottom-up so each level can reference its parent's
//! already-constructed Dict.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::frame_ast::SystemAst;

use super::super::{child_persist_names, extract_tagged_system_name, nested_uses_new_contract};

pub(in crate::frame_c::compiler::codegen::interface_gen) fn generate(
    system: &SystemAst,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();
    let compartment_type = format!("{}Compartment", system.name);

    // RFC-0054 Phase B1: manifest fingerprint — save writes `_manifest`, restore
    // refuses (E751) on drift. GDScript is exceptionless, so refuse = push_error +
    // early return (matching the E700/E750 idiom). `state_data` (bytes_to_var) is
    // plain; revive is per-field, never touching `_manifest`.
    let manifest_fp = super::emit::escape_double_quoted(
        &super::manifest::build_persist_manifest(system).fingerprint(),
    );

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
        .unwrap_or_else(|| "data".to_string());

    // save_state — iterative compartment-chain serialization.
    let mut save_body = String::new();
    save_body.push_str("if not self._context_stack.is_empty():\n");
    save_body.push_str("    push_error(\"E700: system not quiescent\")\n");
    save_body.push_str("    return PackedByteArray()\n");
    save_body.push_str("# Serialize compartment chain iteratively\n");
    save_body.push_str("var _ser_chain = func(comp):\n");
    save_body.push_str("    var chain = []\n");
    save_body.push_str("    var cur = comp\n");
    save_body.push_str("    while cur != null:\n");
    save_body.push_str("        chain.append(cur)\n");
    save_body.push_str("        cur = cur.parent_compartment\n");
    save_body.push_str("    chain.reverse()\n");
    save_body.push_str("    var result = null\n");
    save_body.push_str("    for c in chain:\n");
    save_body.push_str("        var d = {}\n");
    save_body.push_str("        d[\"state\"] = c.state\n");
    save_body.push_str("        d[\"state_args\"] = c.state_args.duplicate()\n");
    save_body.push_str("        d[\"state_vars\"] = c.state_vars.duplicate()\n");
    save_body.push_str("        d[\"enter_args\"] = c.enter_args.duplicate()\n");
    save_body.push_str("        d[\"exit_args\"] = c.exit_args.duplicate()\n");
    save_body.push_str("        d[\"parent_compartment\"] = result\n");
    save_body.push_str("        result = d\n");
    save_body.push_str("    return result\n");
    save_body.push_str("var state_data = {}\n");
    save_body.push_str(&format!(
        "state_data[\"_manifest\"] = \"{}\"\n",
        manifest_fp
    ));
    save_body.push_str("state_data[\"_compartment\"] = _ser_chain.call(self.__compartment)\n");
    save_body.push_str("var stack_arr = []\n");
    save_body.push_str("for c in self._state_stack:\n");
    save_body.push_str("    stack_arr.append(_ser_chain.call(c))\n");
    save_body.push_str("state_data[\"_state_stack\"] = stack_arr\n");

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            // Nested child returns Godot-binary PackedByteArray
            // (var_to_bytes shape). Decode to Variant before embedding.
            let (child_save, _) = child_persist_names(child_sys, "save_state", "restore_state");
            save_body.push_str(&format!(
                "state_data[\"{0}\"] = bytes_to_var(self.{0}.{1}()) if self.{0} != null else null\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!(
                "state_data[\"{}\"] = self.{}\n",
                var.name, var.name
            ));
        }
    }
    // RFC-0053 reflective route (GDScript): plain `var_to_bytes` encodes a user
    // Object as a dead object-id (silent loss), and `var_to_bytes_with_objects`
    // can't encode a scriptless inner class at all. So tag each user Object as a
    // Dictionary {__frame_type__, ...script vars} — now everything is a built-in
    // Variant `var_to_bytes` round-trips faithfully. Generic, no per-type branch;
    // runs tree-wide so a value in a compartment state_var is tagged too.
    save_body.push_str("return var_to_bytes(self._frame_encode(state_data))");

    methods.push(CodegenNode::Method {
        name: save_method_name.clone(),
        params: vec![],
        return_type: Some("PackedByteArray".to_string()),
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
    let target = if uses_new_contract {
        "self"
    } else {
        "instance"
    };
    let mut restore_body = String::new();
    restore_body.push_str(&format!(
        "var state_data = bytes_to_var({})\n",
        load_param_name
    ));
    // B1: refuse a drifted snapshot before reviving any compartment values.
    restore_body.push_str(&format!(
        "if state_data == null or not (state_data is Dictionary) or state_data.get(\"_manifest\", \"\") != \"{}\":\n    push_error(\"E751: persist restore refused - snapshot schema does not match this program\")\n    return{}\n",
        manifest_fp,
        if uses_new_contract { "" } else { " null" }
    ));
    restore_body.push_str("var _deser_chain = func(d):\n");
    restore_body.push_str("    if d == null:\n");
    restore_body.push_str("        return null\n");
    restore_body.push_str("    # Collect chain into array (child first)\n");
    restore_body.push_str("    var chain = []\n");
    restore_body.push_str("    var cur = d\n");
    restore_body.push_str("    while cur != null:\n");
    restore_body.push_str("        chain.append(cur)\n");
    restore_body.push_str("        cur = cur.get(\"parent_compartment\", null)\n");
    restore_body.push_str("    chain.reverse()\n");
    restore_body.push_str("    var result = null\n");
    restore_body.push_str("    for cd in chain:\n");
    restore_body.push_str(&format!(
        "        var comp = {}.new(cd[\"state\"])\n",
        compartment_type
    ));
    restore_body
        .push_str("        comp.state_args = self._frame_revive(cd.get(\"state_args\", {}))\n");
    restore_body
        .push_str("        comp.state_vars = self._frame_revive(cd.get(\"state_vars\", {}))\n");
    restore_body
        .push_str("        comp.enter_args = self._frame_revive(cd.get(\"enter_args\", {}))\n");
    restore_body
        .push_str("        comp.exit_args = self._frame_revive(cd.get(\"exit_args\", {}))\n");
    restore_body.push_str("        comp.parent_compartment = result\n");
    restore_body.push_str("        result = comp\n");
    restore_body.push_str("    return result\n");

    let _ = uses_new_contract;
    restore_body.push_str(&format!(
        "{}.__compartment = _deser_chain.call(state_data[\"_compartment\"])\n",
        target
    ));
    restore_body.push_str(&format!("{}.__next_compartment = null\n", target));
    restore_body.push_str(&format!("{}._state_stack = []\n", target));
    restore_body.push_str("for c in state_data.get(\"_state_stack\", []):\n");
    restore_body.push_str(&format!(
        "    {}._state_stack.append(_deser_chain.call(c))\n",
        target
    ));
    restore_body.push_str(&format!("{}._context_stack = []\n", target));

    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            restore_body.push_str(&format!(
                "var __raw_{0} = state_data.get(\"{0}\", null)\n",
                var.name
            ));
            let (_, child_load) = child_persist_names(child_sys, "save_state", "restore_state");
            if nested_uses_new_contract(child_sys) {
                restore_body.push_str(&format!(
                    "if __raw_{1} != null:\n    {0}.{1} = {2}.new()\n    {0}.{1}.{3}(var_to_bytes(__raw_{1}))\nelse:\n    {0}.{1} = null\n",
                    target, var.name, child_sys, child_load
                ));
            } else {
                restore_body.push_str(&format!(
                    "{0}.{1} = {2}.{3}(var_to_bytes(__raw_{1})) if __raw_{1} != null else null\n",
                    target, var.name, child_sys, child_load
                ));
            }
        } else {
            restore_body.push_str(&format!(
                "{}.{} = self._frame_revive(state_data.get(\"{}\", null))\n",
                target, var.name, var.name
            ));
        }
    }

    if !uses_new_contract {
        restore_body.push_str("return instance");
    }

    let (load_params, load_return, load_static) = if uses_new_contract {
        (
            vec![Param::new(&load_param_name).with_type("PackedByteArray")],
            None,
            false,
        )
    } else {
        (
            vec![Param::new(&load_param_name).with_type("PackedByteArray")],
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

    // Closed-world type registry. GDScript inner classes have no
    // resource_path/global_name and a method can't reach the file's constant
    // map, so the name->class map is supplied by the program via
    // `register_persist_type(name, cls)` (called from file scope, where the
    // sibling classes ARE in scope). Reconstruction resolves only against this
    // map — a snapshot naming an unregistered type is refused (E750), never
    // instantiated. `set()` populates fields without invoking `_init`'s logic.
    methods.push(CodegenNode::VarDecl {
        name: "__persist_types".to_string(),
        type_annotation: None,
        init: Some(Box::new(CodegenNode::NativeBlock {
            code: "{}".to_string(),
            span: None,
        })),
        is_const: false,
    });
    methods.push(CodegenNode::Method {
        name: "register_persist_type".to_string(),
        params: vec![Param::new("nm"), Param::new("cls")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: "self.__persist_types[nm] = cls".to_string(),
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    let mut encode_body = String::new();
    encode_body.push_str("var t = typeof(o)\n");
    encode_body.push_str("if t == TYPE_DICTIONARY:\n");
    encode_body.push_str("    var r = {}\n");
    encode_body.push_str("    for k in o:\n");
    encode_body.push_str("        r[k] = self._frame_encode(o[k])\n");
    encode_body.push_str("    return r\n");
    encode_body.push_str("if t == TYPE_ARRAY:\n");
    encode_body.push_str("    var ra = []\n");
    encode_body.push_str("    for e in o:\n");
    encode_body.push_str("        ra.append(self._frame_encode(e))\n");
    encode_body.push_str("    return ra\n");
    encode_body.push_str("if t == TYPE_OBJECT and o != null:\n");
    encode_body.push_str("    var nm = \"\"\n");
    encode_body.push_str("    for _n in self.__persist_types:\n");
    encode_body.push_str("        if self.__persist_types[_n] == o.get_script():\n");
    encode_body.push_str("            nm = _n\n");
    encode_body.push_str("            break\n");
    encode_body.push_str("    if nm == \"\":\n");
    encode_body
        .push_str("        push_error(\"E750: persist save refused an unregistered type\")\n");
    encode_body.push_str("        return null\n");
    encode_body.push_str("    var h = {\"__frame_type__\": nm}\n");
    encode_body.push_str("    for p in o.get_property_list():\n");
    encode_body.push_str("        if p.usage & PROPERTY_USAGE_SCRIPT_VARIABLE:\n");
    encode_body.push_str("            h[p.name] = self._frame_encode(o.get(p.name))\n");
    encode_body.push_str("    return h\n");
    encode_body.push_str("return o");
    methods.push(CodegenNode::Method {
        name: "_frame_encode".to_string(),
        params: vec![Param::new("o")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: encode_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    let mut revive_body = String::new();
    revive_body.push_str("var t = typeof(o)\n");
    revive_body.push_str("if t == TYPE_DICTIONARY:\n");
    revive_body.push_str("    if o.has(\"__frame_type__\"):\n");
    revive_body.push_str("        var nm = o[\"__frame_type__\"]\n");
    revive_body.push_str("        var cls = self.__persist_types.get(nm, null)\n");
    // Refuse a foreign/unregistered type WITHOUT instantiating it — GDScript has
    // no exceptions to abort the restore, so the closed-world guarantee is "never
    // call .new() on an unresolved type" + a logged E750; the field is left null.
    revive_body.push_str("        if cls == null:\n");
    revive_body.push_str("            push_error(\"E750: persist restore refused a type not defined in this module: \" + str(nm))\n");
    revive_body.push_str("            return null\n");
    revive_body.push_str("        var obj = cls.new()\n");
    revive_body.push_str("        for k in o:\n");
    revive_body.push_str("            if k != \"__frame_type__\":\n");
    revive_body.push_str("                obj.set(k, self._frame_revive(o[k]))\n");
    revive_body.push_str("        return obj\n");
    revive_body.push_str("    var r = {}\n");
    revive_body.push_str("    for k in o:\n");
    revive_body.push_str("        r[k] = self._frame_revive(o[k])\n");
    revive_body.push_str("    return r\n");
    revive_body.push_str("if t == TYPE_ARRAY:\n");
    revive_body.push_str("    var ra = []\n");
    revive_body.push_str("    for e in o:\n");
    revive_body.push_str("        ra.append(self._frame_revive(e))\n");
    revive_body.push_str("    return ra\n");
    revive_body.push_str("return o");
    methods.push(CodegenNode::Method {
        name: "_frame_revive".to_string(),
        params: vec![Param::new("o")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: revive_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    methods
}
