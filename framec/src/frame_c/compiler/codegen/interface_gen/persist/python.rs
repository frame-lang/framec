//! Python persist codegen.
//!
//! Python uses field-by-field JSON (the same wire shape as
//! TS / JS / Ruby / Lua / PHP / Dart) — not whole-object pickle.
//! The blob is `bytes` (UTF-8 JSON). Wire format stays uniform
//! across backends, and `@@[no_persist]` is a per-field skip.
//!
//! RFC-0012 amendment: when `@@[save]` / `@@[load]` operations are
//! declared on the system, emit both as instance methods under the
//! user's chosen names. Otherwise emit the legacy `save_state` /
//! `restore_state` pair (factory-style `restore_state`).
//!
//! Nested `@@SystemName()` domain fields round-trip via the child's
//! own `save_state` / `restore_state` — preserves class identity
//! across the JSON boundary that would otherwise produce a plain
//! dict.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
use crate::frame_c::compiler::frame_ast::SystemAst;

use super::super::{child_persist_names, extract_tagged_system_name, nested_uses_new_contract};

pub(in crate::frame_c::compiler::codegen::interface_gen) fn generate(
    system: &SystemAst,
) -> Vec<CodegenNode> {
    let mut methods = Vec::new();

    // RFC-0012 amendment: branch on new contract. Same pattern
    // as GDScript — when @@[save] / @@[load] declared, emit
    // both as instance methods under the user's chosen names.
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

    let comp_cls = format!("{}Compartment", system.name);

    // ---- save body ----
    let mut save_body = String::new();
    save_body.push_str(
        "if self._context_stack:\n    raise RuntimeError(\"E700: system not quiescent\")\n",
    );
    save_body.push_str("import json\n");
    save_body.push_str("def _ser_comp(c):\n");
    save_body.push_str("    if c is None:\n        return None\n");
    save_body.push_str("    return {\"state\": c.state, \"state_args\": list(c.state_args), \"state_vars\": dict(c.state_vars), \"enter_args\": list(c.enter_args), \"exit_args\": list(c.exit_args), \"parent_compartment\": _ser_comp(c.parent_compartment)}\n");
    save_body.push_str("state_data = {\"_compartment\": _ser_comp(self.__compartment), \"_state_stack\": [_ser_comp(c) for c in self._state_stack]}\n");
    for var in &system.domain {
        // RFC-0016.1: `@@[no_persist]` fields are transient — skip.
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            // Nested @@SystemName(): round-trip via the child's
            // save op (which itself returns UTF-8 JSON bytes) —
            // by the child's DECLARED name (FRAMEC_BUGS #44).
            let (child_save, _) = child_persist_names(child_sys, "save_state", "restore_state");
            save_body.push_str(&format!(
                "state_data[\"{0}\"] = json.loads(self.{0}.{1}()) if self.{0} is not None else None\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("state_data[\"{0}\"] = self.{0}\n", var.name));
        }
    }
    // #174 / RFC-0053 faithful restore: a domain field (or a value nested in
    // one) may hold a user-typed object that is not JSON-native. Tag it with its
    // type name + fields on the way out — reflection, one generic hook, no
    // per-type branch — so `restore_state` can reconstruct it. Plain
    // scalars/lists/dicts are serialized natively and never reach this hook.
    save_body.push_str(
        "def _frame_persist_default(_o):\n    return {\"__frame_type__\": type(_o).__qualname__, **vars(_o)}\n",
    );
    save_body.push_str(
        "return json.dumps(state_data, default=_frame_persist_default).encode(\"utf-8\")",
    );
    methods.push(CodegenNode::Method {
        name: save_method_name.clone(),
        params: vec![],
        return_type: Some("bytes".to_string()),
        body: vec![CodegenNode::NativeBlock {
            code: save_body,
            span: None,
        }],
        is_async: false,
        is_static: false,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    // ---- load body ----
    // `target` is `self` under the new contract (instance method
    // mutating self) or `instance` under the legacy one (static
    // factory returning a fresh, construction-bypassed object).
    let target = if uses_new_contract {
        "self"
    } else {
        "instance"
    };
    let mut restore_body = String::new();
    // Capture the blob before `import json` — the user's load
    // param could be named `json`, which the import would shadow.
    restore_body.push_str(&format!("_blob = {}\n", load_param_name));
    restore_body.push_str("import json\n");
    restore_body.push_str(
        "_raw = _blob.decode(\"utf-8\") if isinstance(_blob, (bytes, bytearray)) else _blob\n",
    );
    // #174 / RFC-0053 faithful restore: rebuild user-typed values that `save`
    // tagged with `__frame_type__`. Closed-world security posture — resolve only
    // against classes DEFINED IN THIS MODULE (never imports or ambient globals),
    // and rebuild by allocating without calling the constructor and setting
    // attributes directly, so an untrusted snapshot cannot run user `__init__`
    // code or name a foreign type. A tagged type absent from this module is
    // refused. Untagged dicts (compartments, plain user dicts) pass through.
    restore_body.push_str("_frame_types = {}\n");
    restore_body.push_str("import sys as _sys\n");
    restore_body.push_str("_mod = _sys.modules.get(__name__)\n");
    restore_body.push_str("if _mod is not None:\n");
    restore_body.push_str("    for _n, _c in vars(_mod).items():\n");
    restore_body.push_str("        if isinstance(_c, type) and getattr(_c, \"__module__\", None) == getattr(_mod, \"__name__\", None):\n");
    restore_body.push_str("            _frame_types[_c.__qualname__] = _c\n");
    restore_body.push_str("def _frame_persist_revive(_d):\n");
    restore_body.push_str("    _t = _d.get(\"__frame_type__\")\n");
    restore_body.push_str("    if _t is None:\n        return _d\n");
    restore_body.push_str("    _cls = _frame_types.get(_t)\n");
    restore_body.push_str("    if _cls is None:\n        raise RuntimeError(\"E750: persist restore refused a type not defined in this module: \" + repr(_t))\n");
    restore_body.push_str("    _obj = _cls.__new__(_cls)\n");
    restore_body.push_str("    for _k, _v in _d.items():\n        if _k != \"__frame_type__\":\n            setattr(_obj, _k, _v)\n");
    restore_body.push_str("    return _obj\n");
    restore_body.push_str("_parsed = json.loads(_raw, object_hook=_frame_persist_revive)\n");
    restore_body.push_str("def _deser_comp(d):\n");
    restore_body.push_str("    if d is None:\n        return None\n");
    restore_body.push_str(&format!("    comp = {}(d[\"state\"])\n", comp_cls));
    restore_body.push_str("    comp.state_args = list(d.get(\"state_args\", []))\n");
    restore_body.push_str("    comp.state_vars = dict(d.get(\"state_vars\", {}))\n");
    restore_body.push_str("    comp.enter_args = list(d.get(\"enter_args\", []))\n");
    restore_body.push_str("    comp.exit_args = list(d.get(\"exit_args\", []))\n");
    restore_body
        .push_str("    comp.parent_compartment = _deser_comp(d.get(\"parent_compartment\"))\n");
    restore_body.push_str("    return comp\n");
    if !uses_new_contract {
        restore_body.push_str(&format!(
            "instance = {}.__new__({})\n",
            system.name, system.name
        ));
    }
    restore_body.push_str(&format!(
        "{0}.__compartment = _deser_comp(_parsed[\"_compartment\"])\n{0}.__next_compartment = None\n{0}._state_stack = [_deser_comp(c) for c in _parsed.get(\"_state_stack\", [])]\n{0}._context_stack = []\n",
        target
    ));
    for var in &system.domain {
        // RFC-0016.1: `@@[no_persist]` fields aren't in the blob —
        // leave them at their `domain:` default (set on construction).
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (_, child_load) = child_persist_names(child_sys, "save_state", "restore_state");
            if nested_uses_new_contract(child_sys) {
                restore_body.push_str(&format!(
                    "if _parsed.get(\"{1}\") is not None:\n    {0}.{1} = {2}()\n    {0}.{1}.{3}(json.dumps(_parsed[\"{1}\"]).encode(\"utf-8\"))\nelse:\n    {0}.{1} = None\n",
                    target, var.name, child_sys, child_load
                ));
            } else {
                restore_body.push_str(&format!(
                    "{0}.{1} = {2}.{3}(json.dumps(_parsed[\"{1}\"]).encode(\"utf-8\")) if _parsed.get(\"{1}\") is not None else None\n",
                    target, var.name, child_sys, child_load
                ));
            }
        } else {
            restore_body.push_str(&format!(
                "{0}.{1} = _parsed.get(\"{1}\")\n",
                target, var.name
            ));
        }
    }
    if !uses_new_contract {
        restore_body.push_str("return instance");
    }
    if uses_new_contract {
        methods.push(CodegenNode::Method {
            name: load_method_name.clone(),
            params: vec![Param::new(&load_param_name).with_type("bytes")],
            return_type: None,
            body: vec![CodegenNode::NativeBlock {
                code: restore_body,
                span: None,
            }],
            is_async: false,
            is_static: false,
            visibility: Visibility::Public,
            decorators: vec![],
        });
    } else {
        methods.push(CodegenNode::Method {
            name: "restore_state".to_string(),
            params: vec![Param::new("data").with_type("bytes")],
            return_type: Some(format!("'{}'", system.name)),
            body: vec![CodegenNode::NativeBlock {
                code: restore_body,
                span: None,
            }],
            is_async: false,
            is_static: true,
            visibility: Visibility::Public,
            decorators: vec![],
        });
    }

    methods
}
