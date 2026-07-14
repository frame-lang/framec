//! EMIT — Rust. **The third backend, and the hardest stress-test of the driver.**
//!
//! Rust breaks assumptions Java and Python let stand:
//!
//! * **No `Object`.** framec's compartment container is `HashMap<String, Box<dyn Any>>`;
//!   pulling a typed value out means `.downcast_ref::<T>()`. That is container
//!   extraction — framec's own scaffolding, the language's own rule — never a
//!   translation of the user's declared type (same category as Java's unbox).
//! * **No null.** A domain field with no value is the user's `Option<T>`; framec does
//!   not invent one.
//! * **Ownership.** A state-var read `.clone()`s out of the borrow, so the read is a
//!   postfix chain — an atom — and also side-steps the borrow checker.
//! * **`.await` is postfix**, not a prefix keyword — the one target where the await
//!   bug (#225) is structurally absent, so its spelling differs from Java/Python.
//!
//! If the shared driver survives Rust with no escape hatch, the "backends are only
//! spellings" claim is real. Everything below is a spelling.

use super::atom::Atom;
use super::driver::{param_names, Backend};
use super::Sink;
use crate::resolve::{SystemSym, TypeRef};
use crate::tree::body::{FrameRef, RefKind};
use crate::NativeText;

pub struct Rust;

impl Backend for Rust {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn file_header(&self, out: &mut Sink) {
        // Inner attributes at the crate root: the generated file is compiled as a lib,
        // and Frame scaffolding has unused params / mut by construction.
        out.frame("#![allow(dead_code, unused_variables, unused_mut, unused_imports)]\n");
        out.frame("use std::collections::HashMap;\n");
        out.frame("use std::any::Any;\n\n");
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        let name = &sym.name;

        // The compartment: state + framec's own Any-boxed containers.
        out.frame("struct Compartment {\n");
        out.frame("    state: String,\n");
        out.frame("    state_vars: HashMap<String, Box<dyn Any>>,\n");
        out.frame("    state_args: HashMap<String, Box<dyn Any>>,\n");
        out.frame("}\n");
        out.frame("impl Compartment {\n");
        out.frame("    fn new(state: &str) -> Compartment {\n");
        out.frame("        Compartment { state: state.to_string(), state_vars: HashMap::new(), state_args: HashMap::new() }\n");
        out.frame("    }\n}\n\n");

        out.frame(&format!("pub struct {name} {{\n"));
        out.frame("    compartment: Compartment,\n");
        out.frame("    stack: Vec<Compartment>,\n");
        // Domain fields — the user's declared type, VERBATIM.
        for f in &sym.domain {
            let ty = match &f.ty {
                TypeRef::Opaque(t) => t.clone(),
                TypeRef::System(s) | TypeRef::WrappedSystem { system: s, .. } => s.clone(),
                TypeRef::None => "()".to_string(),
            };
            out.frame(&format!("    {}: {ty},\n", f.name));
        }
        out.frame("}\n\n");

        // new()
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        out.frame(&format!("impl {name} {{\n"));
        out.frame(&format!("    pub fn new() -> {name} {{\n"));
        out.frame(&format!(
            "        let mut compartment = Compartment::new(\"{first}\");\n"
        ));
        if let Some(st) = sym.states.iter().find(|s| s.name == first) {
            seed_state_vars(st, out);
        }
        out.frame(&format!("        {name} {{ compartment, stack: Vec::new()"));
        for f in &sym.domain {
            // The user's initializer is THEIR native expression, VERBATIM. Only fall back
            // to a default when the user wrote none.
            let init = f.init_text.clone().unwrap_or_else(|| "Default::default()".into());
            out.frame(&format!(", {}: {init}", f.name));
        }
        out.frame(" }\n    }\n\n");
    }

    fn close_system(&self, _sym: &SystemSym, out: &mut Sink) {
        out.frame("}\n");
    }

    fn param_list(&self, params_text: &str) -> String {
        // Rust already writes `name: type` — Frame's syntax IS Rust's here. The type is
        // the user's text, verbatim. So the parameter list passes through unchanged.
        params_text.trim().to_string()
    }

    fn return_type(&self, t: Option<&str>) -> String {
        match t {
            Some(t) => format!(" -> {t}"), // VERBATIM
            None => String::new(),
        }
    }

    fn async_return_type(&self, t: Option<&str>) -> String {
        self.return_type(t)
    }

    fn async_wrap(&self, v: Atom) -> Atom {
        v // Rust async is on `fn`, and `.await` is postfix — nothing to wrap.
    }

    fn route(
        &self,
        _sym: &SystemSym,
        event: &str,
        params: &str,
        ret: Option<&str>,
        is_async: bool,
        arms: &[(String, String)],
        out: &mut Sink,
    ) {
        let args = param_names(params);
        let sig = self.param_list(params);
        let sep = if sig.is_empty() { "" } else { ", " };
        let a = if is_async { "async " } else { "" };
        out.frame(&format!(
            "    pub {a}fn {event}(&mut self{sep}{sig}){} {{\n",
            self.return_type(ret)
        ));
        out.frame("        match self.compartment.state.as_str() {\n");
        for (state, owner) in arms {
            let call = format!("self.{owner}_{event}({args})");
            let call = if is_async { format!("{call}.await") } else { call };
            if ret.is_some() {
                out.frame(&format!("            \"{state}\" => return {call},\n"));
            } else {
                out.frame(&format!("            \"{state}\" => {call},\n"));
            }
        }
        out.frame("            _ => {}\n");
        out.frame("        }\n");
        if ret.is_some() {
            // Fallthrough for a value-returning routed event.
            out.frame("        Default::default()\n");
        }
        out.frame("    }\n\n");
    }

    fn open_handler(
        &self,
        sym: &SystemSym,
        state: &str,
        event: &str,
        params: &str,
        ret: Option<&str>,
        is_async: bool,
        out: &mut Sink,
    ) {
        let sig = self.param_list(params);
        let sep = if sig.is_empty() { "" } else { ", " };
        let a = if is_async { "async " } else { "" };
        out.frame(&format!(
            "    {a}fn {}_{}(&mut self{sep}{sig}){} {{\n",
            state,
            rust_ident(event),
            self.return_type(ret)
        ));
        // Bind state params from framec's state_args container.
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            for p in &st.state_params {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "()".into());
                out.frame(&format!(
                    "        let {p}: {ty} = {};\n",
                    downcast_clone("state_args", p, &ty)
                ));
            }
        }
    }

    fn close_handler(&self, ret: Option<&str>, _is_async: bool, terminated: bool, out: &mut Sink) {
        // A value-returning handler that might fall through needs a value; a `()` one
        // does not. The driver told us whether the body already returned.
        if ret.is_some() && !terminated {
            out.frame("        Default::default()\n");
        }
        out.frame("    }\n\n");
    }

    fn pad(&self, rel: u32) -> String {
        format!("        {}", " ".repeat(rel as usize))
    }

    fn forward(&self, rel: u32, owner: &str, event: &str, params: &str, out: &mut Sink) {
        // `=> $^` — run the parent's handler for this event.
        let p = self.pad(rel);
        out.frame(&format!("{p}self.{owner}_{}({});
", rust_ident(event), param_names(params)));
    }

    fn native_stmt(&self, rel: u32, text: NativeText, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        self.enter(&p, sym, target, args, out);
        out.frame(&format!("{p}self.compartment = __next;\n"));
        out.frame(&format!("{p}return Default::default();\n"));
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!(
            "{p}let __cur = std::mem::replace(&mut self.compartment, Compartment::new(\"\"));\n"
        ));
        out.frame(&format!("{p}self.stack.push(__cur);\n"));
        self.enter(&p, sym, target, args, out);
        out.frame(&format!("{p}self.compartment = __next;\n"));
        out.frame(&format!("{p}return Default::default();\n"));
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self.compartment = self.stack.pop().unwrap();\n"));
        out.frame(&format!("{p}return Default::default();\n"));
    }

    fn return_call(&self, rel: u32, _is_async: bool, expr: NativeText, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.frame("return ");
        out.native(expr);
        out.frame(";\n");
    }

    fn self_call(&self, rel: u32, is_async: bool, method: &str, args: &str, out: &mut Sink) {
        let p = self.pad(rel);
        // `.await` is POSTFIX in Rust — the one target where the await-at-the-head bug
        // (#225) simply cannot arise. So the spelling is `self.m().await`, not
        // `(await self.m())`.
        let call = Atom::call(format!("self.{method}"), args);
        if is_async {
            out.frame(&format!("{p}{}.await;\n", call));
        } else {
            out.frame(&format!("{p}{call};\n"));
        }
    }

    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, out: &mut Sink) {
        let sig = self.param_list(params);
        let sep = if sig.is_empty() { "" } else { ", " };
        out.frame(&format!(
            "    fn {name}(&mut self{sep}{sig}){} {{\n",
            self.return_type(ret)
        ));
    }

    fn close_action(&self, out: &mut Sink) {
        out.frame("    }\n\n");
    }

    fn assign(
        &self,
        _sym: &SystemSym,
        _state: &str,
        lhs: &FrameRef,
        rhs: NativeText,
        rel: u32,
        out: &mut Sink,
    ) {
        let p = self.pad(rel);
        match lhs.kind {
            // A domain field IS an lvalue — `self.field = rhs;`. A Place, never
            // parenthesized (`(self.field) = x` is not idiomatic and here would be wrong).
            RefKind::ContextSelf => {
                out.frame(&format!("{p}self.{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
            // A state var lives in framec's Any-boxed map — a write is an `insert`, not
            // an lvalue. A different statement, not a different spelling of one.
            RefKind::StateVar => {
                out.frame(&format!(
                    "{p}self.compartment.state_vars.insert(\"{}\".to_string(), Box::new(",
                    lhs.name
                ));
                out.native(rhs);
                out.frame("));\n");
            }
            RefKind::ContextData => {
                out.frame(&format!(
                    "{p}self.compartment.state_args.insert(\"{}\".to_string(), Box::new(",
                    lhs.name
                ));
                out.native(rhs);
                out.frame("));\n");
            }
            _ => {
                out.frame(&format!("{p}{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
        }
    }

    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        match r.kind {
            // A state var: `.downcast_ref::<T>().unwrap().clone()` — a postfix chain
            // rooted at `self`, so ALREADY an atom (no `*` deref at the head). `.clone()`
            // both makes it an atom and side-steps the borrow checker (it owns the value
            // out of the borrow). This is container extraction: framec's own map, Rust's
            // own rule, not a translation of the user's type.
            RefKind::StateVar => {
                let ty = sym
                    .states
                    .iter()
                    .find(|s| s.name == state)
                    .and_then(|s| s.state_vars.iter().find(|v| v.name == r.name))
                    .and_then(|v| match &v.ty {
                        TypeRef::Opaque(t) => Some(t.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "()".to_string());
                Atom::ident(downcast_clone("state_vars", &r.name, &ty))
            }
            RefKind::ContextData => {
                Atom::ident(downcast_clone("state_args", &r.name, "()"))
            }
            // A domain field — `self.field`. An identifier chain, an atom, and an lvalue
            // root (`@@:self.field = e`), so it must NOT be parenthesized.
            RefKind::ContextSelf => Atom::field(Atom::ident("self"), &r.name),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => {
                Atom::method(Atom::field(Atom::field(Atom::ident("self"), "compartment"), "state"), "clone", "")
            }
            RefKind::ContextReturn | RefKind::ContextEvent | RefKind::SelfCall => {
                Atom::ident(&r.name)
            }
        }
    }

    fn persist(&self, _m: &super::persist::PersistManifest, _out: &mut Sink) {
        // Fixed-type route (serde). Not built yet — see PERSIST_ROADMAP Phase 3. A
        // domain of scalars round-trips via serde into the declared types; deferred to
        // avoid a serde dependency in the corpus harness.
    }

    fn dead_code_is_an_error(&self) -> bool {
        // Rust warns on unreachable code but does not error (unlike Java). The `#![allow]`
        // header covers it; we stop after a transition anyway because it is genuinely dead.
        false
    }
}

impl Rust {
    fn enter(&self, p: &str, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        out.frame(&format!("{p}let mut __next = Compartment::new(\"{target}\");\n"));
        if let Some(st) = sym.states.iter().find(|s| s.name == target) {
            for v in &st.state_vars {
                // Re-seed a fresh compartment's state vars. The default is the state
                // var's declared init — a native expression; a zero stand-in for now.
                out.frame(&format!(
                    "{p}__next.state_vars.insert(\"{}\".to_string(), Box::new(<{}>::default()));\n",
                    v.name,
                    state_var_ty(v)
                ));
            }
            // State args, unsplit — Box each arg by position. framec does not split the
            // blob; but Rust is statically typed, so the args ARE known positionally from
            // the declaration, and each is boxed individually.
            if let Some(a) = args.map(str::trim).filter(|a| !a.is_empty()) {
                let names: Vec<&str> = st.state_params.iter().map(String::as_str).collect();
                if names.len() == 1 {
                    out.frame(&format!(
                        "{p}__next.state_args.insert(\"{}\".to_string(), Box::new({a}));\n",
                        names[0]
                    ));
                } else if !names.is_empty() {
                    // Multiple args: a tuple pattern binds them, then box each.
                    let pat = names
                        .iter()
                        .map(|n| format!("__a_{n}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.frame(&format!("{p}let ({pat}) = ({a});\n"));
                    for n in &names {
                        out.frame(&format!(
                            "{p}__next.state_args.insert(\"{n}\".to_string(), Box::new(__a_{n}));\n"
                        ));
                    }
                }
            }
        }
    }
}

/// `self.compartment.<container>.get("k").unwrap().downcast_ref::<T>().unwrap().clone()`
/// — a postfix chain, an atom.
fn downcast_clone(container: &str, key: &str, ty: &str) -> String {
    format!(
        "self.compartment.{container}.get(\"{key}\").unwrap().downcast_ref::<{ty}>().unwrap().clone()"
    )
}

fn state_var_ty(v: &crate::resolve::FieldSym) -> String {
    match &v.ty {
        TypeRef::Opaque(t) => t.clone(),
        _ => "()".to_string(),
    }
}

fn seed_state_vars(st: &crate::resolve::StateSym, out: &mut Sink) {
    for v in &st.state_vars {
        // The state var's own initializer, VERBATIM (else a typed default).
        let init = v
            .init_text
            .clone()
            .unwrap_or_else(|| format!("<{}>::default()", state_var_ty(v)));
        out.frame(&format!(
            "        compartment.state_vars.insert(\"{}\".to_string(), Box::new({init}));\n",
            v.name
        ));
    }
}

/// Frame's lifecycle event names are not Rust identifiers.
fn rust_ident(event: &str) -> String {
    match event {
        "$>" => "__enter".to_string(),
        "<$" => "__exit".to_string(),
        other => other.to_string(),
    }
}
