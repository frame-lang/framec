//! Lua persist codegen.
//!
//! Lua fidelity-exception: wire format is Lua's native textual
//! table-literal serialization via the `serpent` library
//! (https://github.com/pkulchenko/serpent), NOT JSON. Rationale:
//! lua-cjson decodes every JSON number as a Lua float
//! (`lua_Number`), erasing the Lua 5.3+ integer subtype
//! distinction. Most user code is unaffected (Lua's `==` is
//! numeric-equal across int/float) but code that uses
//! `math.type()` subtype queries or bitwise ops on persisted
//! integers silently breaks. Serpent dumps each value with the
//! syntax Lua's parser will read back as the same type — integers
//! stay integers, floats stay floats. Mirrors Erlang's ETF and
//! GDScript's `var_to_bytes` fidelity-exception rationale.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::frame_ast::SystemAst;

use super::super::{child_persist_names, extract_tagged_system_name, nested_uses_new_contract};

pub(in crate::frame_c::compiler::codegen::interface_gen) fn generate(
    system: &SystemAst,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();
    let compartment_type = format!("{}Compartment", system.name);

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
        .unwrap_or_else(|| "json_str".to_string());
    let target = if uses_new_contract {
        "self"
    } else {
        "instance"
    };

    // RFC-0054 Phase B1: manifest fingerprint — save writes `_manifest`, restore
    // refuses (E751) on drift. `_parsed` (serpent.load) is plain; revive is applied
    // per-field, never to `_manifest`, so the check reads a plain string.
    let manifest_fp =
        super::emit::escape_double_quoted(&super::manifest::build_persist_manifest(system).fingerprint());

    let mut save_body = String::new();
    save_body
        .push_str("if #self._context_stack > 0 then error(\"E700: system not quiescent\") end\n");
    save_body.push_str("local serpent = require(\"serpent\")\n");
    save_body.push_str("local function serialize_comp(comp)\n");
    save_body.push_str("    if not comp then return nil end\n");
    save_body.push_str("    local t = {}\n");
    save_body.push_str("    t.state = comp.state\n");
    save_body.push_str("    t.state_args = comp.state_args\n");
    save_body.push_str("    t.state_vars = comp.state_vars\n");
    save_body.push_str("    t.enter_args = comp.enter_args\n");
    save_body.push_str("    t.exit_args = comp.exit_args\n");
    save_body.push_str("    t.forward_event = comp.forward_event\n");
    save_body.push_str("    t.parent_compartment = serialize_comp(comp.parent_compartment)\n");
    save_body.push_str("    return t\n");
    save_body.push_str("end\n");
    save_body.push_str("local stack = {}\n");
    save_body.push_str("for _, c in ipairs(self._state_stack) do\n");
    save_body.push_str("    stack[#stack + 1] = serialize_comp(c)\n");
    save_body.push_str("end\n");
    save_body.push_str("local result = {}\n");
    save_body.push_str(&format!("result._manifest = \"{}\"\n", manifest_fp));
    save_body.push_str("result._compartment = serialize_comp(self.__compartment)\n");
    save_body.push_str("result._state_stack = stack\n");
    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (child_save, _) = child_persist_names(child_sys, "save_state", "restore_state");
            save_body.push_str(&format!(
                "result.{0} = (self.{0} ~= nil) and (select(2, serpent.load(self.{0}:{1}()))) or nil\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("result.{} = self.{}\n", var.name, var.name));
        }
    }
    // RFC-0053 reflective route (Lua): serpent serializes a table's DATA, never
    // its metatable — so a "class instance" (table + method-bearing metatable)
    // would lose its type. Tag any table whose metatable carries a `__name` with
    // that name + its fields; restore rebuilds the type by re-attaching the
    // metatable. Generic, no per-type branch; runs tree-wide so a user value in a
    // compartment state_var/arg is tagged too. Plain tables (compartment
    // scaffolding) pass through untouched.
    save_body.push_str("local function _frame_encode(o)\n");
    save_body.push_str("    if type(o) ~= \"table\" then return o end\n");
    save_body.push_str("    local mt = getmetatable(o)\n");
    save_body.push_str("    if mt and mt.__name then\n");
    save_body.push_str("        local h = { __frame_type__ = mt.__name }\n");
    save_body.push_str("        for k, v in pairs(o) do h[k] = _frame_encode(v) end\n");
    save_body.push_str("        return h\n");
    save_body.push_str("    end\n");
    save_body.push_str("    local r = {}\n");
    save_body.push_str("    for k, v in pairs(o) do r[k] = _frame_encode(v) end\n");
    save_body.push_str("    return r\n");
    save_body.push_str("end\n");
    save_body.push_str("return serpent.dump(_frame_encode(result))");

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
    restore_body.push_str("local serpent = require(\"serpent\")\n");
    restore_body.push_str(&format!(
        "local ok, _parsed = serpent.load({})\n",
        load_param_name
    ));
    restore_body
        .push_str("if not ok then error(\"persist load failed: \" .. tostring(_parsed)) end\n");
    // B1: refuse a drifted snapshot before reviving any compartment values.
    restore_body.push_str(&format!(
        "if _parsed._manifest ~= \"{}\" then error(\"E751: persist restore refused - snapshot schema does not match this program\") end\n",
        manifest_fp
    ));

    // RFC-0053 reflective route (Lua) — closed-world metatable registry, hybrid.
    // Lua has no class enumeration, so the name->metatable map is built from two
    // zero-ambient sources: (1) graph-seed — walk the receiving instance's own
    // initialized graph, keying each metatable by its `__name` (every Frame
    // variable has an initializer, so its type is present); (2) an optional
    // `register_persist_type(mt)` hook for a type with no initializer or the
    // legacy path (no live graph). A tag resolving to neither is refused (E750).
    restore_body.push_str("local _reg = {}\n");
    restore_body.push_str("local function _frame_seed(o, d)\n");
    restore_body.push_str("    if type(o) ~= \"table\" or d > 64 then return end\n");
    restore_body.push_str("    local mt = getmetatable(o)\n");
    restore_body.push_str(
        "    if mt and mt.__name and _reg[mt.__name] == nil then _reg[mt.__name] = mt end\n",
    );
    restore_body.push_str("    for _, v in pairs(o) do _frame_seed(v, d + 1) end\n");
    restore_body.push_str("end\n");
    if uses_new_contract {
        restore_body.push_str("_frame_seed(self, 0)\n");
    }
    restore_body.push_str(&format!(
        "if {0}.__persistUserTypes then for k, v in pairs({0}.__persistUserTypes) do _reg[k] = v end end\n",
        system.name
    ));
    // #182: also seed the registry from DECLARED field / state-var / arg types. The
    // live-graph seed misses a persisted user type not reachable from the fresh
    // restore target's graph (e.g. a field defaulting to nil). Lua has no class
    // enumeration and — unlike JS's lexical class resolution — no canonical mapping
    // from a declared type NAME to its metatable when the metatable is a separate
    // local (the `Vec2Meta` idiom). We can seed only the **class-is-metatable**
    // convention (`Vec2 = {}; Vec2.__index = Vec2; Vec2.__name = "..."`), where the
    // declared name resolves (lexically or as a global) to a table carrying
    // `__name` — keyed by that `__name` (the same key the save-side tag uses).
    // A separate-metatable type still needs `register_persist_type`. Reading an
    // undefined name yields nil in Lua, and the `type(...) == "table"` guard skips
    // primitives / functions / nil, so this never errors and never registers a
    // non-class name. framec stays type-ignorant: it emits a runtime probe, not an
    // assertion that the name IS a class.
    {
        use crate::frame_c::compiler::frame_ast::Type;
        use std::collections::BTreeSet;
        let mut decl_types: BTreeSet<&str> = BTreeSet::new();
        for var in &system.domain {
            if let Type::Custom(t) = &var.var_type {
                decl_types.insert(t.as_str());
            }
        }
        let manifest = super::manifest::build_persist_manifest(system);
        for st in &manifest.states {
            for (_n, t) in &st.state_vars {
                decl_types.insert(t.as_str());
            }
            for t in st
                .state_args
                .iter()
                .chain(&st.enter_args)
                .chain(&st.exit_args)
            {
                decl_types.insert(t.as_str());
            }
        }
        let is_class_ident = |s: &str| {
            let mut cs = s.chars();
            matches!(cs.next(), Some(c) if c.is_ascii_uppercase())
                && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        };
        for t in &decl_types {
            if t.is_empty() || !is_class_ident(t) {
                continue;
            }
            restore_body.push_str(&format!(
                "if type({0}) == \"table\" and {0}.__name then _reg[{0}.__name] = {0} end\n",
                t
            ));
        }
    }
    restore_body.push_str("local function _frame_revive(o)\n");
    restore_body.push_str("    if type(o) ~= \"table\" then return o end\n");
    restore_body.push_str("    if o.__frame_type__ ~= nil then\n");
    restore_body.push_str("        local mt = _reg[o.__frame_type__]\n");
    restore_body.push_str("        if mt == nil then error(\"E750: persist restore refused a type not defined in this module: \" .. tostring(o.__frame_type__)) end\n");
    restore_body.push_str("        local obj = {}\n");
    restore_body.push_str("        for k, v in pairs(o) do if k ~= \"__frame_type__\" then obj[k] = _frame_revive(v) end end\n");
    restore_body.push_str("        return setmetatable(obj, mt)\n");
    restore_body.push_str("    end\n");
    restore_body.push_str("    local r = {}\n");
    restore_body.push_str("    for k, v in pairs(o) do r[k] = _frame_revive(v) end\n");
    restore_body.push_str("    return r\n");
    restore_body.push_str("end\n");

    restore_body.push_str("local function deserialize_comp(d)\n");
    restore_body.push_str("    if not d then return nil end\n");
    restore_body.push_str(&format!(
        "    local comp = {}.new(d.state)\n",
        compartment_type
    ));
    restore_body.push_str("    comp.state_args = _frame_revive(d.state_args or {})\n");
    restore_body.push_str("    comp.state_vars = _frame_revive(d.state_vars or {})\n");
    restore_body.push_str("    comp.enter_args = _frame_revive(d.enter_args or {})\n");
    restore_body.push_str("    comp.exit_args = _frame_revive(d.exit_args or {})\n");
    restore_body.push_str("    comp.forward_event = d.forward_event\n");
    restore_body.push_str("    comp.parent_compartment = deserialize_comp(d.parent_compartment)\n");
    restore_body.push_str("    return comp\n");
    restore_body.push_str("end\n");
    if !uses_new_contract {
        restore_body.push_str("local instance = {}\n");
        restore_body.push_str(&format!(
            "setmetatable(instance, {{__index = {}}})\n",
            system.name
        ));
    }
    restore_body.push_str(&format!(
        "{}.__compartment = deserialize_comp(_parsed._compartment)\n",
        target
    ));
    restore_body.push_str(&format!("{}.__next_compartment = nil\n", target));
    restore_body.push_str(&format!("{}._state_stack = {{}}\n", target));
    restore_body.push_str(&format!("{}._context_stack = {{}}\n", target));
    restore_body.push_str("if _parsed._state_stack then\n");
    restore_body.push_str("    for _, c in ipairs(_parsed._state_stack) do\n");
    restore_body.push_str(&format!(
        "        {0}._state_stack[#{0}._state_stack + 1] = deserialize_comp(c)\n",
        target
    ));
    restore_body.push_str("    end\n");
    restore_body.push_str("end\n");
    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (_, child_load) = child_persist_names(child_sys, "save_state", "restore_state");
            if nested_uses_new_contract(child_sys) {
                restore_body.push_str(&format!(
                    "if _parsed.{1} ~= nil then {0}.{1} = {2}:new(); {0}.{1}:{3}(serpent.dump(_parsed.{1})) else {0}.{1} = nil end\n",
                    target, var.name, child_sys, child_load
                ));
            } else {
                restore_body.push_str(&format!(
                    "if _parsed.{1} ~= nil then {0}.{1} = {2}.{3}(serpent.dump(_parsed.{1})) else {0}.{1} = nil end\n",
                    target, var.name, child_sys, child_load
                ));
            }
            continue;
        }
        restore_body.push_str(&format!(
            "{}.{} = _frame_revive(_parsed.{})\n",
            target, var.name, var.name
        ));
    }
    if !uses_new_contract {
        restore_body.push_str("return instance");
    }

    let (load_return, load_static) = if uses_new_contract {
        (None, false)
    } else {
        (Some(system.name.clone()), true)
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

    // Hybrid-registry completion hook (Lua): register a metatable the graph-seed
    // can't reach (a type with no initializer, or the legacy path with no live
    // graph). Keyed by the metatable's own `__name`; still closed-world — the
    // caller hands over the metatable, no name/global lookup.
    let mut hook_body = String::new();
    hook_body.push_str(&format!(
        "if {0}.__persistUserTypes == nil then {0}.__persistUserTypes = {{}} end\n",
        system.name
    ));
    hook_body.push_str(&format!(
        "{}.__persistUserTypes[mt.__name] = mt",
        system.name
    ));
    methods.push(CodegenNode::Method {
        name: "register_persist_type".to_string(),
        params: vec![Param::new("mt")],
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
