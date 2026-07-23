//! EMIT — Rust. **The third backend, and the hardest stress-test of the driver.**
//!
//! Rust breaks assumptions Java and Python let stand:
//!
//! * **Typed compartment, no erasure.** The compartment is per-system: each state's `$.`
//!   vars and `(param)` args become a variant of a generated `<Sys>Vars` / `<Sys>Args`
//!   enum, and a read pattern-matches the current variant (`<Sys>Vars::S { v, .. } =>
//!   v.clone()`). No `Box<dyn Any>`, no `downcast` — so the host serializer marshals the
//!   compartment natively and persist round-trips the whole control state + stack
//!   (RFC-0056). serde is derived on those types only when the system persists.
//! * **No null.** A domain field with no value is the user's `Option<T>`; framec does
//!   not invent one.
//! * **Ownership.** A state-var read `.clone()`s out of the borrow, so the read is a
//!   parenthesized `match` — an atom — and also side-steps the borrow checker.
//! * **`.await` is postfix**, not a prefix keyword — the one target where the await
//!   bug (#225) is structurally absent, so its spelling differs from Java/Python.
//!
//! If the shared driver survives Rust with no escape hatch, the "backends are only
//! spellings" claim is real. Everything below is a spelling.

use super::atom::Atom;
use super::driver::{param_names, Backend};
use super::Sink;
use crate::resolve::{SystemSym, TypeRef};
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

pub struct Rust;

impl Backend for Rust {
    fn name(&self) -> &'static str {
        "rust"
    }

    /// Rust has visibility: `@@system private` -> a crate-private `struct` (no `pub`).
    fn supports_class_visibility(&self) -> bool {
        true
    }

    fn file_header(&self, out: &mut Sink) {
        // Inner attributes at the crate root: the generated file is compiled as a lib,
        // and Frame scaffolding has unused params / mut by construction.
        out.frame("#![allow(dead_code, unused_variables, unused_mut, unused_imports)]\n");
        out.frame("use std::collections::HashMap;\n");
        out.frame("use std::any::Any;\n\n");
        // The compartment is **typed, per system** (see `emit_compartment_types`) — NOT a
        // shared `HashMap<String, Box<dyn Any>>`. Each state's `$.` vars and `(param)` args
        // become a variant of a per-system enum, so the host serializer marshals them
        // natively (RFC-0056) and a restore rebuilds the full compartment + stack with no
        // type erasure. Nothing is emitted at file scope; the imports above are harmless
        // under `#![allow(unused_imports)]` and left for hand-written native segments.
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        let name = &sym.name;

        // `@@[scan(<elem>)]` — a positioned, borrowed-input scanner (RFC-0042.1 / #209).
        // Emits the generic input-source trait, a machine generic over `I`, and
        // `over`/`scan_at` instead of `new`. An ordinary system falls through unchanged.
        if let Some(elem) = sym.scan.clone() {
            self.open_scanner(sym, &elem, out);
            return;
        }

        emit_compartment_types(sym, out);

        // READ-ONLY BORROWED DOMAIN (the plain-`@@system` analogue of a scanner's `&'a [u8]`).
        // A tree-walker emitter reads immutable data (an AST, a symbol table, backing source
        // bytes) and returns owned text — so its domain wants a `&T` field, not an owned copy.
        // When ANY domain field is a shared borrow (`&Syms`, `&dyn Backend`), the struct, its
        // `impl`, and the constructor all take ONE lifetime `'a`, exactly as `open_scanner`
        // threads it for `src`. Owned fields are untouched, and a system with no borrowed field
        // emits byte-identically to before (`lt` is empty). `&mut` never reaches here — validate
        // rejects it (E641): the mandate is read-only.
        let lt = if sym.domain.iter().any(is_borrowed_field) { "<'a>" } else { "" };

        // A persist-reachable system is embedded BY VALUE in a parent's snapshot struct
        // (`inner: Inner`), so its own struct must clone + serialize + deserialize — serde then
        // recurses the whole sub-system (compartment, stack, domain) with no reflection and no
        // qualname lookup. An ordinary system derives nothing here (unchanged).
        if sym.persist_reachable {
            out.frame("#[derive(Clone, serde::Serialize, serde::Deserialize)]\n");
        }
        // `@@system private` -> a crate-private `struct` (no `pub`); the `pub fn` methods stay
        // usable within the crate. Default is `pub struct`.
        let svis = if sym.private { "" } else { "pub " };
        out.frame(&format!("{svis}struct {name}{lt} {{\n"));
        out.frame(&format!("    compartment: {name}Comp,\n"));
        out.frame(&format!("    stack: Vec<{name}Comp>,\n"));
        // Domain fields — the user's declared type, VERBATIM. `pub`, like a scanner's: the
        // domain IS the system's readable state (a walker's result, a counter), so a native
        // wrapper can read it after driving the machine. A borrowed field carries the system
        // lifetime (`&Syms` -> `&'a Syms`); `thread_lt` is the identity on an owned type.
        for f in &sym.domain {
            let ty = match &f.ty {
                TypeRef::Opaque(t) => thread_lt(t),
                TypeRef::System(s) | TypeRef::WrappedSystem { system: s, .. } => s.clone(),
                TypeRef::None => "()".to_string(),
            };
            out.frame(&format!("    pub {}: {ty},\n", f.name));
        }
        out.frame("}\n\n");

        // new()
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        out.frame(&format!("impl{lt} {name}{lt} {{\n"));
        // Constructor params — state, then enter, then domain (§203). Rust: `name: type`. A
        // borrowed param carries `'a` too (`syms: &Syms` -> `syms: &'a Syms`), so the field it
        // initializes and the arg it is built from agree on the one lifetime. With no borrowed
        // field this is exactly `ctor_params_text` + `param_list` (byte-identical).
        let plist = if lt.is_empty() {
            self.param_list(&super::driver::ctor_params_text(&sym.params))
        } else {
            ctor_params_lt(&sym.params)
        };
        out.frame(&format!("    pub fn new({plist}) -> {name}{lt} {{\n"));
        // The start compartment, TYPED: vars seed from their inits, args from the header's
        // state/enter params (§203) that name the start state's params (a distinct
        // `enter_args` is deferred — see `args_ctor_expr`).
        out.frame(&format!(
            "        let compartment = {name}Comp {{ state: \"{first}\".to_string(), vars: {}, args: {} }};\n",
            vars_expr(sym, first),
            args_ctor_expr(sym, first)
        ));
        out.frame(&format!("        {name} {{ compartment, stack: Vec::new()"));
        for f in &sym.domain {
            // `= @@Inner()` is FRAME's instantiation syntax -> the Rust constructor. Any
            // other init is the user's native expression, verbatim.
            let init = match &f.init_system {
                Some(s) => format!("{s}::new({})", super::ctor_init_args(f.init_text.as_deref())),
                None => f.init_text.clone().unwrap_or_else(|| "Default::default()".into()),
            };
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
        // Bind state params as handler locals, read out of the typed args variant (the
        // compartment is in `state` here by construction, so the match cannot miss).
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            for p in &st.state_params {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "()".into());
                out.frame(&format!(
                    "        let {p}: {ty} = {};\n",
                    typed_read(sym, state, "Args", "args", p)
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
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        // Build the target compartment FIRST, then swap it in and push the displaced one.
        // Building `__next` first means the swap needs no empty-state placeholder — the
        // typed compartment has no `""` state to construct.
        self.enter(&p, sym, target, args, out);
        out.frame(&format!(
            "{p}self.stack.push(std::mem::replace(&mut self.compartment, __next));\n"
        ));
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self.compartment = self.stack.pop().unwrap();\n"));
    }

    fn push_bare(&self, rel: u32, out: &mut Sink) {
        // Push a CLONE of the current compartment; stay. The typed `<Sys>Comp` derives Clone.
        out.frame(&format!("{}self.stack.push(self.compartment.clone());\n", self.pad(rel)));
    }

    fn pop_bare(&self, rel: u32, out: &mut Sink) {
        // Pop and drop the top; stay.
        out.frame(&format!("{}self.stack.pop();\n", self.pad(rel)));
    }

    fn lifecycle_call(&self, rel: u32, _sym: &SystemSym, state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self.{state}_{}({});\n", rust_ident(event), args.unwrap_or("")));
    }

    fn pop_enter(&self, rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        let a = enter_args.unwrap_or("");
        for st in &sym.states {
            if super::driver::has_lifecycle(sym, &st.name, "$>") {
                out.frame(&format!(
                    "{p}if self.compartment.state == \"{}\" {{ self.{}_{}({a}); }}\n",
                    st.name, st.name, rust_ident("$>")
                ));
            }
        }
    }

    fn terminate(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}return Default::default();\n", self.pad(rel)));
    }

    fn return_call(&self, rel: u32, _is_async: bool, _multiline: bool, expr: NativeText, out: &mut Sink) {
        // `multiline` is ignored: a `;`-terminated statement carries its own continuation
        // across newlines — no wrapping parens are needed (or wanted).
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
        sym: &SystemSym,
        state: &str,
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
            // A state var is a field of the current compartment's TYPED variant — a write
            // reaches it through the variant pattern (`if let` never fails: the compartment
            // is in `state` by construction here) and assigns the place `*field`. The RHS is
            // evaluated into a temp FIRST: `$.x = $.x + 1` reads the same variant the write
            // borrows mutably, so binding the value before taking the `&mut` avoids E0502.
            RefKind::StateVar => {
                out.frame(&format!("{p}{{ let __v = "));
                out.native(rhs);
                out.frame(&format!(
                    "; if let {}Vars::{state} {{ {n}, .. }} = &mut self.compartment.vars {{ *{n} = __v; }} }}\n",
                    sym.name,
                    n = lhs.name
                ));
            }
            RefKind::ContextData => {
                out.frame(&format!("{p}{{ let __v = "));
                out.native(rhs);
                out.frame(&format!(
                    "; if let {}Args::{state} {{ {n}, .. }} = &mut self.compartment.args {{ *{n} = __v; }} }}\n",
                    sym.name,
                    n = lhs.name
                ));
            }
            RefKind::ContextReturn => {
                out.frame(&format!("{p}return "));
                out.native(rhs);
                out.frame(";\n");
            }
            _ => {
                out.frame(&format!("{p}{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
        }
    }

    fn system_ctor_call(&self, name: &str, args: &[String]) -> Atom {
        Atom::call(format!("{name}::new"), args.join(", "))
    }

    fn embed_call(&self, _sym: &SystemSym, ec: &EmbedCall) -> Atom {
        // An EMPTY field is a bare self-call `@@:self.method(...)` embedded in an expression
        // (bug R3): the receiver is `self`, not `self.<field>`.
        if ec.field.is_empty() {
            return Atom::call(format!("self.{}", ec.method), &ec.args);
        }
        Atom::method(Atom::field(Atom::ident("self"), &ec.field), &ec.method, &ec.args)
    }

    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        match r.kind {
            // A state var: read the field out of the current compartment's TYPED variant.
            // `.clone()` both owns the value out of the borrow (Rust's rule, side-steps the
            // borrow checker) and makes the parenthesized `match` an atom. No `downcast` —
            // the variant IS the type. The read cannot be reached in another state, so the
            // fallback arm is `unreachable!()` (omitted for a single-state system, where the
            // match is already exhaustive and a `_` arm would warn).
            RefKind::StateVar => Atom::ident(typed_read(sym, state, "Vars", "vars", &r.name)),
            RefKind::ContextData => Atom::ident(typed_read(sym, state, "Args", "args", &r.name)),
            // A domain field — `self.field`. An identifier chain, an atom, and an lvalue
            // root (`@@:self.field = e`), so it must NOT be parenthesized.
            RefKind::ContextSelf => Atom::field(Atom::ident("self"), &r.name),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => {
                Atom::method(Atom::field(Atom::field(Atom::ident("self"), "compartment"), "state"), "clone", "")
            }
            // `Unknown` is diagnosed as an error by `validate` (E408), which BLOCKS emission, so
            // it is unreachable on the real pipeline; degrade gracefully (name as an identifier)
            // rather than panic on any direct-emit path.
            RefKind::ContextReturn | RefKind::ContextEvent | RefKind::SelfCall | RefKind::Unknown => {
                Atom::ident(&r.name)
            }
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        // FIXED-TYPE ROUTE (Regime A) via serde — RFC-0056. A user type self-marshals through
        // the host serializer (`#[derive(Serialize, Deserialize)]`); framec names the fields
        // and derives, and **serde does ALL type work** — nesting, collections, user types,
        // string escaping. So this is strictly type-IGNORANT: not one `match user_type` here,
        // unlike the flat format it replaces. The declared field type is the fidelity; no tag
        // is read from the blob, so a user value carrying a marker is inert data (immune to
        // type-mimicry / #233).
        //
        // The snapshot struct is a LOCAL item inside each method — an `impl` block cannot hold
        // a `struct`, and a local one keeps the field set beside its use. `_schema` is checked
        // first so a drifted snapshot is refused (E751), not mis-shaped. Requires the generated
        // crate to depend on serde + serde_json (the self-marshalling requirement).
        // FULL control-state fidelity (RFC-0056): `_control` is the ENTIRE typed compartment
        // — its state name AND its typed vars/args — and `_stack` is the whole compartment
        // stack, not just a state name. `<Sys>Comp` derives serde, so the host serializer
        // marshals the vars/args natively and a restore REBUILDS the live control state: a
        // `pop$` after restore finds the caller's compartment waiting, a state var read after
        // restore finds its value. framec writes no per-type code; serde does the work.
        let schema = m.schema();
        let comp = format!("{}Comp", m.sys);
        let snap_struct = |out: &mut Sink| {
            out.frame("        #[derive(serde::Serialize, serde::Deserialize)]\n");
            out.frame("        struct __Snap {\n");
            out.frame("            _schema: String,\n");
            out.frame(&format!("            _control: {comp},\n"));
            out.frame(&format!("            _stack: Vec<{comp}>,\n"));
            for (n, t) in &m.fields {
                out.frame(&format!("            {n}: {t},\n"));
            }
            out.frame("        }\n");
        };

        out.frame(&format!("    pub fn {}(&self) -> {} {{\n", m.save, m.blob));
        snap_struct(out);
        out.frame("        let __snap = __Snap {\n");
        out.frame(&format!("            _schema: {schema:?}.to_string(),\n"));
        out.frame("            _control: self.compartment.clone(),\n");
        out.frame("            _stack: self.stack.clone(),\n");
        for (n, _) in &m.fields {
            out.frame(&format!("            {n}: self.{n}.clone(),\n"));
        }
        out.frame("        };\n");
        out.frame("        serde_json::to_string(&__snap).unwrap()\n");
        out.frame("    }\n\n");

        out.frame(&format!("    pub fn {}(&mut self, data: {}) {{\n", m.load, m.blob));
        snap_struct(out);
        out.frame("        let __snap: __Snap = serde_json::from_str(&data).unwrap();\n");
        out.frame(&format!("        if __snap._schema != {schema:?} {{\n"));
        out.frame("            panic!(\"E751: persist restore refused - snapshot schema does not match this program\");\n");
        out.frame("        }\n");
        out.frame("        self.compartment = __snap._control;\n");
        out.frame("        self.stack = __snap._stack;\n");
        for (n, _) in &m.fields {
            out.frame(&format!("        self.{n} = __snap.{n};\n"));
        }
        out.frame("    }\n\n");
    }
}

impl Rust {
    /// Build `__next`, the TYPED compartment for entering `target`. Vars seed from their
    /// declared inits; args split positionally from the transition's arg blob (framec does
    /// not tear the blob apart — a tuple pattern binds it, so a `(a, b)` value stays whole)
    /// into the target's typed args variant. No `Box`, no map inserts.
    fn enter(&self, p: &str, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let name = &sym.name;
        let st = sym.states.iter().find(|s| s.name == target);
        let args_expr = match st {
            Some(st) if !st.state_params.is_empty() => {
                match args.map(str::trim).filter(|a| !a.is_empty()) {
                    Some(a) => {
                        let names: Vec<&str> =
                            st.state_params.iter().map(String::as_str).collect();
                        if names.len() == 1 {
                            format!("{name}Args::{target} {{ {}: {a} }}", names[0])
                        } else {
                            // Multiple args: bind the whole blob through a tuple pattern
                            // (never split by framec), then name each binding into the
                            // typed variant.
                            let pat = names
                                .iter()
                                .map(|n| format!("__a_{n}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            out.frame(&format!("{p}let ({pat}) = ({a});\n"));
                            let fields = names
                                .iter()
                                .map(|n| format!("{n}: __a_{n}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{name}Args::{target} {{ {fields} }}")
                        }
                    }
                    // Entered without args though the state declares params — defaults.
                    None => args_default_expr(sym, target),
                }
            }
            _ => format!("{name}Args::{target} {{ }}"),
        };
        out.frame(&format!(
            "{p}let mut __next = {name}Comp {{ state: \"{target}\".to_string(), vars: {}, args: {args_expr} }};\n",
            vars_expr(sym, target)
        ));
    }
}

/// `(match &self.compartment.<container> { <Sys><Kind>::<state> { field, .. } => field.clone(),
/// _ => unreachable!() })` — read a typed var/arg field out of the current compartment. The
/// `_` arm is dropped for a single-state system (the match is already exhaustive, and a `_`
/// there would be an `unreachable_patterns` warning). Parenthesized, so it is an atom.
fn typed_read(sym: &SystemSym, state: &str, kind: &str, container: &str, field: &str) -> String {
    let arm = format!("{}{kind}::{state} {{ {field}, .. }} => {field}.clone()", sym.name);
    if sym.states.len() == 1 {
        format!("(match &self.compartment.{container} {{ {arm} }})")
    } else {
        format!("(match &self.compartment.{container} {{ {arm}, _ => unreachable!() }})")
    }
}

/// Emit the per-system TYPED compartment: `<Sys>Vars` / `<Sys>Args` (one variant per state,
/// carrying exactly that state's `$.` vars / `(param)` args) and `<Sys>Comp { state, vars,
/// args }`. serde is derived ONLY when the system persists — an ordinary system must not
/// force the generated crate to depend on serde. EVERY state gets a variant (var-less states
/// get an empty one), so a compartment is constructible in any state and the stack is
/// homogeneously typed. This is the erasure-free compartment (RFC-0056): the host serializer
/// marshals the vars/args natively; framec writes no `downcast`, no `Box<dyn Any>`.
fn emit_compartment_types(sym: &SystemSym, out: &mut Sink) {
    let name = &sym.name;
    // serde on the compartment when the system's value can land in a snapshot — its own
    // `@@[persist]` OR embedded as a sub-system field of one (persist_reachable). An ordinary
    // system stays serde-free.
    let derive = if sym.persist_reachable {
        "#[derive(Clone, serde::Serialize, serde::Deserialize)]\n"
    } else {
        "#[derive(Clone)]\n"
    };
    out.frame(derive);
    out.frame(&format!("enum {name}Vars {{\n"));
    for st in &sym.states {
        let fields = st
            .state_vars
            .iter()
            .map(|v| format!("{}: {}", v.name, state_var_ty(v)))
            .collect::<Vec<_>>()
            .join(", ");
        out.frame(&format!("    {} {{ {fields} }},\n", st.name));
    }
    out.frame("}\n");
    out.frame(derive);
    out.frame(&format!("enum {name}Args {{\n"));
    for st in &sym.states {
        let fields = st
            .state_params
            .iter()
            .map(|p| {
                let ty = st.state_param_types.get(p).map(String::as_str).unwrap_or("()");
                format!("{p}: {ty}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.frame(&format!("    {} {{ {fields} }},\n", st.name));
    }
    out.frame("}\n");
    out.frame(derive);
    out.frame(&format!("struct {name}Comp {{\n"));
    out.frame("    state: String,\n");
    out.frame(&format!("    vars: {name}Vars,\n"));
    out.frame(&format!("    args: {name}Args,\n"));
    out.frame("}\n\n");
}

/// `<Sys>Vars::<State> { v: <seed>, ... }` — the typed state-vars for constructing `state`.
fn vars_expr(sym: &SystemSym, state: &str) -> String {
    let inner = sym
        .states
        .iter()
        .find(|s| s.name == state)
        .map(|s| {
            s.state_vars
                .iter()
                .map(|v| format!("{}: {}", v.name, state_seed_value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("{}Vars::{state} {{ {inner} }}", sym.name)
}

/// `<Sys>Args::<State> { p: <p or default>, ... }` — the args for the START compartment, where
/// the ctor's params ARE in scope: a state-param seeds from a same-named ctor/state/enter
/// param when present, else the type's default. (A start-state param with no matching header
/// param is the deferred enter_args nuance — it defaults rather than binding.)
fn args_ctor_expr(sym: &SystemSym, state: &str) -> String {
    let inner = sym
        .states
        .iter()
        .find(|s| s.name == state)
        .map(|s| {
            s.state_params
                .iter()
                .map(|p| {
                    let ty = s.state_param_types.get(p).map(String::as_str).unwrap_or("()");
                    let in_scope = sym
                        .params
                        .state
                        .iter()
                        .chain(&sym.params.enter)
                        .chain(&sym.params.domain)
                        .any(|x| &x.name == p);
                    let val = if in_scope { p.clone() } else { format!("<{ty}>::default()") };
                    format!("{p}: {val}")
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("{}Args::{state} {{ {inner} }}", sym.name)
}

/// `<Sys>Args::<State> { p: <default>, ... }` — args when a state is ENTERED without them
/// (no ctor params in scope here), so every field takes its type's default.
fn args_default_expr(sym: &SystemSym, state: &str) -> String {
    let inner = sym
        .states
        .iter()
        .find(|s| s.name == state)
        .map(|s| {
            s.state_params
                .iter()
                .map(|p| {
                    let ty = s.state_param_types.get(p).map(String::as_str).unwrap_or("()");
                    format!("{p}: <{ty}>::default()")
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("{}Args::{state} {{ {inner} }}", sym.name)
}

fn state_var_ty(v: &crate::resolve::FieldSym) -> String {
    match &v.ty {
        TypeRef::Opaque(t) => t.clone(),
        _ => "()".to_string(),
    }
}

/// The seed value for a state var: `= @@Sub()` -> `Sub::new()` (Frame's instantiation
/// syntax), else the user's init verbatim, else a typed default. Shared by the constructor
/// and re-entry so both agree (re-entry used to drop the init and always `default()`).
fn state_seed_value(v: &crate::resolve::FieldSym) -> String {
    match &v.init_system {
        Some(s) => format!("{s}::new({})", super::ctor_init_args(v.init_text.as_deref())),
        None => v
            .init_text
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("<{}>::default()", state_var_ty(v))),
    }
}

/// Frame's lifecycle event names are not Rust identifiers.
impl Rust {
    /// `@@[scan(<elem>)]` — emit a positioned, borrowed-input scanner (RFC-0042.1 / #209),
    /// the `@@system` analogue of an `@@fsm` recognizer. The machine is generic over its
    /// input source (so `over(&bytes)` borrows with zero copy — the fix for the O(n²) probe
    /// that forced the hand-rolled loops). The impl block is left OPEN; the driver adds the
    /// state handlers, whose native bodies peek `self.src.fsm_get(self.cursor)` and advance
    /// `self.cursor` — framec owns the control structure, the peek/advance is native.
    fn open_scanner(&self, sym: &SystemSym, elem: &str, out: &mut Sink) {
        let name = &sym.name;

        // 1. The byte-accessor trait, impl'd for the borrowed slice. `self.src.fsm_get(i)`
        //    reads a byte; `fsm_len()` is the length. The cleanroom always scans a borrowed
        //    `&[u8]` (concrete, zero-copy — NO buffer is ever copied). Because the source is
        //    a shared borrow, scanners COMPOSE for free: an outer scanner hands the SAME
        //    borrow to a sub-scanner (`Inner::over(self.src)` duplicates only the 16-byte
        //    fat pointer, never the bytes). (RFC-0042.1's generic owned/callback input is a
        //    user convenience the compiler's own self-scanning does not need, and its
        //    per-system trait obstructs this composition.)
        let _ = elem;
        out.frame(&format!(
            "pub trait {name}Input {{ fn fsm_get(&self, i: usize) -> u8; fn fsm_len(&self) -> usize; }}\n"
        ));
        out.frame(&format!(
            "impl {name}Input for &[u8] {{ fn fsm_get(&self, i: usize) -> u8 {{ self[i] }} fn fsm_len(&self) -> usize {{ self.len() }} }}\n\n"
        ));

        // 2. The machine, borrowing a `&'a [u8]`. `cursor` is public so the native wrapper
        //    can read the match extent after a scan. Its compartment is TYPED per system,
        //    like an ordinary system's (a scanner never persists, so no serde is derived).
        emit_compartment_types(sym, out);
        let svis = if sym.private { "" } else { "pub " };
        out.frame(&format!("{svis}struct {name}<'a> {{\n"));
        out.frame("    src: &'a [u8],\n");
        out.frame("    pub cursor: usize,\n");
        out.frame(&format!("    compartment: {name}Comp,\n"));
        out.frame(&format!("    stack: Vec<{name}Comp>,\n"));
        // Domain fields are `pub` on a scanner: they ARE the scanner's output (a count, an
        // accumulated list, a flag), which the native wrapper reads after `scan_at`.
        for f in &sym.domain {
            let ty = match &f.ty {
                TypeRef::Opaque(t) => t.clone(),
                TypeRef::System(s) | TypeRef::WrappedSystem { system: s, .. } => s.clone(),
                TypeRef::None => "()".to_string(),
            };
            out.frame(&format!("    pub {}: {ty},\n", f.name));
        }
        out.frame("}\n\n");

        // 3. The impl block — stays OPEN; the driver appends handlers/interface methods.
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        out.frame(&format!("impl<'a> {name}<'a> {{\n"));

        // A domain field is CONFIG if its init references a construction (domain) param —
        // it is set once at `over()` and must survive `scan_at` (the param is not in scope
        // there to re-derive it). Everything else is scan STATE and resets each scan.
        let is_config = |f: &crate::resolve::FieldSym| -> bool {
            let Some(init) = &f.init_text else { return false };
            sym.params.domain.iter().any(|p| {
                init.split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|w| w == p.name)
            })
        };

        // over(src, <config>): construct WITHOUT running (RFC-0042 construction model,
        // positioned). Domain params are the scanner's construction config (e.g. the target).
        let cfg = self.param_list(&super::driver::ctor_params_text(&sym.params));
        let sep = if cfg.is_empty() { "" } else { ", " };
        out.frame(&format!("    pub fn over(src: &'a [u8]{sep}{cfg}) -> Self {{\n"));
        out.frame(&format!(
            "        let compartment = {name}Comp {{ state: \"{first}\".to_string(), vars: {}, args: {} }};\n",
            vars_expr(sym, first),
            args_ctor_expr(sym, first)
        ));
        out.frame(&format!("        {name} {{ src, cursor: 0, compartment, stack: Vec::new()"));
        for f in &sym.domain {
            let init = match &f.init_system {
                Some(s) => format!("{s}::new({})", super::ctor_init_args(f.init_text.as_deref())),
                None => f.init_text.clone().unwrap_or_else(|| "Default::default()".into()),
            };
            out.frame(&format!(", {}: {init}", f.name));
        }
        out.frame(" }\n    }\n\n");

        // scan_at(start): position the cursor, restart at the start state, and DRIVE the
        // machine ITERATIVELY — dispatch the scanner's step event until a terminal state.
        // Iteration, not enter-handler recursion, so a self-looping scan state stays O(1)
        // stack over an arbitrarily long input (the #209 linearity goal). Each step peeks,
        // advances `self.cursor`, and transitions; a state that neither advances nor
        // transitions is a bug, so a `len*4` bound trips to a break rather than hang.
        // ACCEPTS iff it ends in a state named `$Accept`.
        out.frame("    pub fn scan_at(&mut self, start: usize) -> bool {\n");
        out.frame("        self.cursor = start;\n");
        // Reset the scanner's SCAN STATE to its inits, so scan_at is RESTARTABLE — a
        // counter or flag from a previous scan must not leak into the next one. CONFIG
        // fields (set from a construction param) are NOT reset: they are the scanner's
        // fixed configuration, and the param is not in scope here to re-derive them.
        for f in &sym.domain {
            if is_config(f) {
                continue;
            }
            let init = match &f.init_system {
                Some(s) => format!("{s}::new({})", super::ctor_init_args(f.init_text.as_deref())),
                None => f.init_text.clone().unwrap_or_else(|| "Default::default()".into()),
            };
            out.frame(&format!("        self.{} = {init};\n", f.name));
        }
        // Restart at the start state — a fresh typed compartment. `over()`'s config params
        // are not in scope here, so any start-state args default (a scanner start state
        // rarely takes params).
        out.frame(&format!(
            "        self.compartment = {name}Comp {{ state: \"{first}\".to_string(), vars: {}, args: {} }};\n",
            vars_expr(sym, first),
            args_default_expr(sym, first)
        ));
        if let Some(ev) = sym.interface.first() {
            let evname = &ev.name;
            out.frame("        let mut __steps: usize = 0;\n");
            out.frame(
                "        while self.compartment.state != \"Accept\" && self.compartment.state != \"Reject\" {\n",
            );
            out.frame(&format!("            self.{evname}();\n"));
            out.frame("            __steps += 1;\n");
            out.frame("            if __steps > self.src.fsm_len() * 4 + 64 { break; }\n");
            out.frame("        }\n");
        }
        out.frame("        self.compartment.state == \"Accept\"\n");
        out.frame("    }\n\n");
    }
}

fn rust_ident(event: &str) -> String {
    match event {
        "$>" => "__enter".to_string(),
        "<$" => "__exit".to_string(),
        other => other.to_string(),
    }
}

/// Is this domain field a **read-only shared borrow** (`&Syms`, `&dyn Backend`)? Only an
/// `Opaque` type can be — a `&Sub` sub-system is caught earlier by E640 (borrow a system via
/// `= @@Sub()`, not `&`). The check is purely lexical (a leading `&`), never a parse: the type
/// is the user's text and framec does not read inside it. `&mut` is a borrow too, but validate
/// (E641) has already refused it, so a `&mut` never reaches this predicate on the real pipeline.
fn is_borrowed_field(f: &crate::resolve::FieldSym) -> bool {
    matches!(&f.ty, TypeRef::Opaque(t) if t.trim_start().starts_with('&'))
}

/// Thread the system lifetime `'a` through a borrowed type: `&T` -> `&'a T`, `&dyn Tr` ->
/// `&'a dyn Tr`. A non-borrowed type is returned UNCHANGED, so this is the identity on every
/// owned domain field and every scalar ctor param — which is what keeps a borrow-free system
/// byte-identical. framec inserts the lifetime token right after the `&`; it never otherwise
/// reads or rewrites the user's type text.
fn thread_lt(ty: &str) -> String {
    match ty.trim_start().strip_prefix('&') {
        Some(rest) => format!("&'a {}", rest.trim_start()),
        None => ty.to_string(),
    }
}

/// Constructor params in Frame's `name: type` form, CONSTRUCTOR order (state, enter, domain),
/// with the system lifetime threaded through each borrowed type. This is the borrowed-system
/// twin of [`super::driver::ctor_params_text`]; it is Rust-specific (the `'a` spelling), so it
/// stays out of the target-blind driver. Rust already writes `name: type`, so no `param_list`
/// reorder is needed — only the per-type `thread_lt`.
fn ctor_params_lt(p: &crate::tree::SystemParams) -> String {
    p.state
        .iter()
        .chain(&p.enter)
        .chain(&p.domain)
        .map(|param| match &param.ty {
            Some(t) => format!("{}: {}", param.name, thread_lt(t)),
            None => param.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
