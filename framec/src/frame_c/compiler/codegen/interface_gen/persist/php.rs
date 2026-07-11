//! PHP persist codegen.
//!
//! `json_encode` / `json_decode` field-by-field wire format. Two
//! private helpers (`__serComp` instance method, `__deserComp`
//! static — static because the legacy path calls it via `self::`
//! from the static factory). Legacy contract uses
//! `ReflectionClass::newInstanceWithoutConstructor()` to bypass
//! `__construct` so the initial-state `$>()` doesn't re-fire on
//! restore; new contract mutates `$this` in place.

use crate::frame_c::compiler::codegen::ast::{CodegenNode, Param, Visibility};
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
        "$this"
    } else {
        "$instance"
    };

    let mut ser_body = String::new();
    ser_body.push_str("if ($comp === null) return null;\n");
    ser_body.push_str("$j = ['state' => $comp->state, 'state_vars' => $comp->state_vars];\n");
    ser_body.push_str("$j['state_args'] = $comp->state_args;\n");
    ser_body.push_str("$j['enter_args'] = $comp->enter_args;\n");
    ser_body.push_str("$j['parent'] = $this->__serComp($comp->parent_compartment);\n");
    ser_body.push_str("return $j;");

    methods.push(CodegenNode::Method {
        name: "__serComp".to_string(),
        params: vec![Param::new("comp")],
        return_type: None,
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
    deser_body.push_str("if ($data === null) return null;\n");
    deser_body.push_str(&format!(
        "$c = new {}($data['state']);\n",
        compartment_class
    ));
    deser_body.push_str("if (isset($data['state_vars'])) $c->state_vars = $data['state_vars'];\n");
    deser_body.push_str("if (isset($data['state_args'])) $c->state_args = $data['state_args'];\n");
    deser_body.push_str("if (isset($data['enter_args'])) $c->enter_args = $data['enter_args'];\n");
    deser_body.push_str("if (isset($data['parent'])) $c->parent_compartment = self::__deserComp($data['parent']);\n");
    deser_body.push_str("return $c;");

    methods.push(CodegenNode::Method {
        name: "__deserComp".to_string(),
        params: vec![Param::new("data")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: deser_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    // #174 / RFC-0053 reflective route (PHP). `json_encode` drops class identity
    // (an object serializes to its public props) and `json_decode(..., true)`
    // yields arrays, so the reflective tag/revive is two recursive passes over the
    // whole tree — reaching user-typed values nested in the compartment chain, not
    // just domain fields. Static methods so both the instance save/restore and the
    // legacy static factory reach them via `self::`.

    // Save-side: tag any object with its class + all properties (reflection, so
    // private/protected round-trip too). One generic pass, no per-type branch.
    let mut encode_body = String::new();
    encode_body.push_str("if ($o === null || is_scalar($o)) return $o;\n");
    encode_body.push_str("if (is_array($o)) { $r = []; foreach ($o as $k => $v) { $r[$k] = self::_frame_persist_encode($v); } return $r; }\n");
    encode_body.push_str("$h = ['__frame_type__' => get_class($o)];\n");
    encode_body.push_str("$ro = new \\ReflectionObject($o);\n");
    encode_body.push_str("foreach ($ro->getProperties() as $p) { if (PHP_VERSION_ID < 80100) { $p->setAccessible(true); } $h[$p->getName()] = self::_frame_persist_encode($p->getValue($o)); }\n");
    encode_body.push_str("return $h;");
    methods.push(CodegenNode::Method {
        name: "_frame_persist_encode".to_string(),
        params: vec![Param::new("o")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: encode_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    // Closed-world registry: classes whose ReflectionClass file is THIS file
    // (PHP exposes it directly), minus framec's own generated runtime classes.
    // Stdlib / composer / imported classes are excluded, so a hostile snapshot
    // cannot name a foreign type.
    let mut registry_body = String::new();
    registry_body.push_str(&format!(
        "$excluded = ['{0}', '{0}Compartment', '{0}FrameEvent', '{0}FrameContext'];\n",
        sys
    ));
    registry_body.push_str("$reg = [];\n");
    registry_body.push_str("foreach (get_declared_classes() as $cn) {\n");
    registry_body.push_str("    if (in_array($cn, $excluded, true)) continue;\n");
    registry_body.push_str("    try { $rc = new \\ReflectionClass($cn); if ($rc->getFileName() === __FILE__) { $reg[$cn] = true; } } catch (\\Throwable $e) {}\n");
    registry_body.push_str("}\n");
    registry_body.push_str("return $reg;");
    methods.push(CodegenNode::Method {
        name: "_frame_persist_registry".to_string(),
        params: vec![],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: registry_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    // Restore-side: rebuild a tagged array into its class via
    // `newInstanceWithoutConstructor()` (no `__construct`) + reflection property
    // set, resolving only against `$reg`. A tagged type absent from the registry
    // is refused (E750). Untagged arrays recurse; scalars pass through.
    let mut revive_body = String::new();
    revive_body.push_str("if (!is_array($o)) return $o;\n");
    revive_body.push_str("if (array_key_exists('__frame_type__', $o)) {\n");
    revive_body.push_str("    $t = $o['__frame_type__'];\n");
    revive_body.push_str("    if (!isset($reg[$t])) throw new \\Exception(\"E750: persist restore cannot resolve type (declare it as a field type or register it for restore): \" . $t);\n");
    revive_body
        .push_str("    $obj = (new \\ReflectionClass($t))->newInstanceWithoutConstructor();\n");
    revive_body.push_str("    $ro = new \\ReflectionObject($obj);\n");
    revive_body.push_str("    foreach ($o as $k => $v) {\n");
    revive_body.push_str("        if ($k === '__frame_type__') continue;\n");
    revive_body.push_str("        $rv = self::_frame_persist_revive($v, $reg);\n");
    revive_body.push_str("        if ($ro->hasProperty($k)) { $p = $ro->getProperty($k); if (PHP_VERSION_ID < 80100) { $p->setAccessible(true); } $p->setValue($obj, $rv); } else { $obj->$k = $rv; }\n");
    revive_body.push_str("    }\n");
    revive_body.push_str("    return $obj;\n");
    revive_body.push_str("}\n");
    revive_body.push_str("$r = [];\n");
    revive_body
        .push_str("foreach ($o as $k => $v) { $r[$k] = self::_frame_persist_revive($v, $reg); }\n");
    revive_body.push_str("return $r;");
    methods.push(CodegenNode::Method {
        name: "_frame_persist_revive".to_string(),
        params: vec![Param::new("o"), Param::new("reg")],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: revive_body,
            span: None,
        }],
        is_async: false,
        is_static: true,
        visibility: Visibility::Private,
        decorators: vec![],
    });

    // RFC-0054 Phase B1: manifest fingerprint — save writes `_manifest`, restore
    // refuses (E751) on drift BEFORE reviving (plain decode, no type resolution).
    let manifest_fp = super::emit::escape_double_quoted(
        &super::manifest::build_persist_manifest(system).fingerprint(),
    );

    let mut save_body = String::new();
    save_body.push_str("if (!empty($this->_context_stack)) throw new \\Exception(\"E700: system not quiescent\");\n");
    save_body.push_str("$j = [];\n");
    save_body.push_str(&format!("$j['_manifest'] = \"{}\";\n", manifest_fp));
    save_body.push_str("$j['_compartment'] = $this->__serComp($this->__compartment);\n");
    save_body.push_str("$stack = [];\n");
    save_body
        .push_str("foreach ($this->_state_stack as $c) { $stack[] = $this->__serComp($c); }\n");
    save_body.push_str("$j['_state_stack'] = $stack;\n");
    for var in &system.domain {
        if var.attributes.iter().any(|a| a.name == "no_persist") {
            continue;
        }
        let init = var.initializer_text.as_deref().unwrap_or("");
        if let Some(child_sys) = extract_tagged_system_name(init) {
            let (child_save, _) = child_persist_names(child_sys, "save_state", "restore_state");
            save_body.push_str(&format!(
                "$j['{0}'] = $this->{0} !== null ? json_decode($this->{0}->{1}(), true) : null;\n",
                var.name, child_save
            ));
        } else {
            save_body.push_str(&format!("$j['{}'] = $this->{};\n", var.name, var.name));
        }
    }
    // #174: tag any user-typed value (anywhere in the tree) before encoding.
    save_body.push_str("return json_encode(self::_frame_persist_encode($j));");

    methods.push(CodegenNode::Method {
        name: save_method_name.clone(),
        params: vec![],
        return_type: None,
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
    // #174: revive tagged user-typed values (tree-wide) under the closed-world
    // registry, before the compartment/domain extraction reads them.
    // B1: plain-decode and refuse drift BEFORE reviving.
    restore_body.push_str(&format!(
        "$__raw = json_decode(${}, true);\n",
        load_param_name
    ));
    restore_body.push_str(&format!(
        "if (!isset($__raw['_manifest']) || $__raw['_manifest'] !== \"{}\") throw new \\Exception(\"E751: persist restore refused - snapshot schema does not match this program\");\n",
        manifest_fp
    ));
    restore_body.push_str(
        "$_parsed = self::_frame_persist_revive($__raw, self::_frame_persist_registry());\n",
    );
    if !uses_new_contract {
        restore_body.push_str(&format!(
            "$instance = (new \\ReflectionClass({}::class))->newInstanceWithoutConstructor();\n",
            sys
        ));
    }
    let deser = if uses_new_contract {
        "$this->__deserComp"
    } else {
        "self::__deserComp"
    };
    restore_body.push_str(&format!("{}->_state_stack = [];\n", target));
    restore_body.push_str(&format!("{}->_context_stack = [];\n", target));
    restore_body.push_str(&format!(
        "{}->__compartment = {}($_parsed['_compartment']);\n",
        target, deser
    ));
    restore_body.push_str("if (isset($_parsed['_state_stack'])) {\n");
    restore_body.push_str(&format!(
        "    foreach ($_parsed['_state_stack'] as $sc) {{ {}->_state_stack[] = {}($sc); }}\n",
        target, deser
    ));
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
                    "if (isset($_parsed['{1}']) && $_parsed['{1}'] !== null) {{ {0}->{1} = new {2}(); {0}->{1}->{3}(json_encode($_parsed['{1}'])); }}\n",
                    target, var.name, child_sys, child_load
                ));
            } else {
                restore_body.push_str(&format!(
                    "if (isset($_parsed['{1}']) && $_parsed['{1}'] !== null) {0}->{1} = {2}::{3}(json_encode($_parsed['{1}']));\n",
                    target, var.name, child_sys, child_load
                ));
            }
        } else {
            restore_body.push_str(&format!(
                "if (isset($_parsed['{1}'])) {0}->{1} = $_parsed['{1}'];\n",
                target, var.name
            ));
        }
    }
    if !uses_new_contract {
        restore_body.push_str("return $instance;");
    }
    methods.push(CodegenNode::Method {
        name: load_method_name.clone(),
        params: vec![Param::new(&load_param_name)],
        return_type: None,
        body: vec![CodegenNode::NativeBlock {
            code: restore_body,
            span: None,
        }],
        is_async: false,
        is_static: !uses_new_contract,
        visibility: Visibility::Public,
        decorators: vec![],
    });

    methods
}
