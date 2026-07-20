//! EMIT — C. **The hardest backend: no reflection, no generics, no `Any`, manual memory.**
//!
//! Java has `Object`, Rust has `Box<dyn Any>`. C has neither — and the cleanroom does NOT
//! fake one. The compartment is TYPED per system (`emit_comp_types`): each state's `$.` vars
//! and `(param)` args become a struct, and `<Sys>_Comp` holds them in a `state`-tagged union.
//! A read or write is a plain union-field access — `self->compartment->vars.<State>.x` — with
//! no `void*` map, no per-var `malloc`, no `*(T*)` cast-and-deref (so the old #220 deref
//! hazard is retired with the box). The host serializer marshals the fields natively when the
//! system persists (RFC-0056: cJSON + author hooks for user types).
//!
//! C also has no methods: every function takes an explicit `self` pointer, and member
//! access is `->`, not `.`. Those are spellings; the driver never learns them.
//!
//! Memory: `calloc`-zeroed compartments are freed on `_destroy`; a deep free of user-owned
//! child systems belongs with the coverage layer and is deferred.

use super::atom::Atom;
use super::driver::{params_split, Backend};
use super::Sink;
use crate::resolve::{SystemSym, TypeRef};
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

pub struct C {
    /// The system currently being emitted — actions and reentrant self-calls need
    /// `System* self` and `System_method(self, ...)`, but the driver hands those methods
    /// no `sym`. Set in `open_system`.
    cur: std::cell::RefCell<String>,
}

impl C {
    pub fn new() -> C {
        C { cur: std::cell::RefCell::new(String::new()) }
    }
}

impl Backend for C {
    fn name(&self) -> &'static str {
        "c"
    }

    fn file_header(&self, out: &mut Sink) {
        out.frame("#include <stdlib.h>\n#include <string.h>\n#include <stdbool.h>\n\n");
        // The compartment is TYPED, per system (see `emit_comp_types`) — no `void*`-keyed map,
        // no heap-boxed state vars. Each state's `$.` vars and `(param)` args become a struct,
        // and the compartment holds them in a union tagged by `state`. Nothing shared is
        // emitted at file scope; the includes above are the only header content.
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        *self.cur.borrow_mut() = sym.name.clone();
        let name = &sym.name;
        emit_comp_types(sym, out);
        out.frame("typedef struct {\n");
        out.frame(&format!("    {name}_Comp* compartment;\n"));
        out.frame(&format!("    {name}_Comp** stack; int stack_len; int stack_cap;\n"));
        for f in &sym.domain {
            out.frame(&format!("    {} {};\n", field_type(f), f.name));
        }
        out.frame(&format!("}} {name};\n\n"));

        // Forward declarations — C needs every function declared before its first call,
        // and the driver emits the interface (which calls handlers) before the handlers.
        // This is a C spelling; the driver never learns C's ordering rule.
        for m in &sym.interface {
            let plist = self.param_list(m.params_text.as_deref().unwrap_or(""));
            let sep = if plist.is_empty() { "" } else { ", " };
            out.frame(&format!(
                "{} {name}_{}({name}* self{sep}{plist});\n",
                self.return_type(m.return_text.as_deref()),
                m.name
            ));
        }
        for st in &sym.states {
            for h in &st.handlers {
                let plist = self.param_list(&h.params_text);
                let sep = if plist.is_empty() { "" } else { ", " };
                out.frame(&format!(
                    "{} {name}_{}_{}({name}* self{sep}{plist});\n",
                    self.return_type(h.return_text.as_deref()),
                    st.name,
                    c_ident(&h.event)
                ));
            }
        }
        for a in &sym.actions {
            let plist = self.param_list(a.params_text.as_deref().unwrap_or(""));
            let sep = if plist.is_empty() { "" } else { ", " };
            out.frame(&format!(
                "{} {name}_{}({name}* self{sep}{plist});\n",
                self.return_type(a.return_text.as_deref()),
                a.name
            ));
        }
        // Constructor params — state, then enter, then domain (§203). C: type-first.
        let plist = self.param_list(&super::driver::ctor_params_text(&sym.params));
        let psig = if plist.is_empty() { "void".to_string() } else { plist };
        out.frame(&format!("{name}* {name}_new({psig});\n"));
        out.frame(&format!("void {name}_destroy({name}* self);\n"));
        out.frame("\n");

        // Constructor.
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        out.frame(&format!("{name}* {name}_new({psig}) {{\n"));
        out.frame(&format!("    {name}* self = malloc(sizeof({name}));\n"));
        out.frame(&format!(
            "    self->compartment = {name}_Comp_new(\"{first}\");\n"
        ));
        out.frame("    self->stack = 0; self->stack_len = 0; self->stack_cap = 0;\n");
        if let Some(st) = sym.states.iter().find(|s| s.name == first) {
            for v in &st.state_vars {
                seed_var("self->compartment", first, v, out);
            }
            // State/enter params seed the START compartment's args (§203) — a same-named
            // header param assigns the typed field; the deferred enter_args nuance leaves an
            // unmatched start-state param at its zero (calloc'd).
            for p in &st.state_params {
                if sym.params.state.iter().chain(&sym.params.enter).chain(&sym.params.domain).any(|x| x.name == *p) {
                    out.frame(&format!(
                        "    self->compartment->args.{first}.{p} = ({p});\n"
                    ));
                }
            }
        }
        for f in &sym.domain {
            // `= @@Inner()` is FRAME's instantiation syntax -> the C constructor. Any
            // other init is the user's native expression, verbatim.
            let init = match &f.init_system {
                Some(s) => format!("{s}_new({})", super::ctor_init_args(f.init_text.as_deref())),
                None => c_init_expr(&f.init_text.clone().unwrap_or_else(|| "0".into()), &field_type(f)),
            };
            out.frame(&format!("    self->{} = {init};\n", f.name));
        }
        out.frame("    return self;\n}\n\n");

        // Destructor — the counterpart to `@@Sys()`/`_new()`. C has no GC, so a system you
        // construct you must be able to free. framec frees what IT owns: the compartments
        // and their maps (keys + boxed values). Domain fields that are themselves child
        // systems are the user's to free (spec §848); a deep free belongs with the memory
        // layer and is deferred.
        out.frame(&format!("void {name}_destroy({name}* self) {{\n"));
        out.frame("    if (!self) return;\n");
        out.frame(&format!("    for (int i = 0; i < self->stack_len; i++) {name}_Comp_free(self->stack[i]);\n"));
        out.frame("    free(self->stack);\n");
        out.frame(&format!("    {name}_Comp_free(self->compartment);\n"));
        out.frame("    free(self);\n}\n\n");
    }

    fn close_system(&self, _sym: &SystemSym, _out: &mut Sink) {}

    fn param_list(&self, params_text: &str) -> String {
        // C is type-first (`int amount`), like Java — reorder Frame's `name: type`, type
        // VERBATIM.
        params_split(params_text)
            .into_iter()
            .map(|(n, t)| match t {
                Some(t) => format!("{t} {n}"),
                None => format!("void* {n}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn return_type(&self, t: Option<&str>) -> String {
        t.map(str::to_string).unwrap_or_else(|| "void".into())
    }

    fn async_return_type(&self, t: Option<&str>) -> String {
        self.return_type(t)
    }

    fn route(
        &self,
        sym: &SystemSym,
        event: &str,
        params: &str,
        ret: Option<&str>,
        _is_async: bool,
        arms: &[(String, String)],
        out: &mut Sink,
    ) {
        let name = &sym.name;
        let plist = self.param_list(params);
        let sep = if plist.is_empty() { "" } else { ", " };
        let args = arg_names(params);
        let acall = if args.is_empty() { String::new() } else { format!(", {args}") };
        out.frame(&format!(
            "{} {name}_{event}({name}* self{sep}{plist}) {{\n",
            self.return_type(ret)
        ));
        for (state, owner) in arms {
            let call = format!("{name}_{owner}_{event}(self{acall})");
            if ret.is_some() {
                out.frame(&format!(
                    "    if (strcmp(self->compartment->state, \"{state}\")==0) return {call};\n"
                ));
            } else {
                out.frame(&format!(
                    "    if (strcmp(self->compartment->state, \"{state}\")==0) {{ {call}; return; }}\n"
                ));
            }
        }
        if let Some(t) = ret {
            out.frame(&format!("    return ({t}){{0}};\n"));
        }
        out.frame("}\n\n");
    }

    fn open_handler(
        &self,
        sym: &SystemSym,
        state: &str,
        event: &str,
        params: &str,
        ret: Option<&str>,
        _is_async: bool,
        out: &mut Sink,
    ) {
        let name = &sym.name;
        let plist = self.param_list(params);
        let sep = if plist.is_empty() { "" } else { ", " };
        out.frame(&format!(
            "{} {name}_{state}_{}({name}* self{sep}{plist}) {{\n",
            self.return_type(ret),
            c_ident(event)
        ));
        // Bind state params as locals off the current state's TYPED args struct — a plain
        // field read, no cast, no deref (the compartment is in `state` here by construction).
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            for p in &st.state_params {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "void*".into());
                out.frame(&format!(
                    "    {ty} {p} = self->compartment->args.{state}.{p};\n"
                ));
            }
        }
    }

    fn close_handler(&self, ret: Option<&str>, _is_async: bool, terminated: bool, out: &mut Sink) {
        if let Some(t) = ret {
            if !terminated {
                out.frame(&format!("    return ({t}){{0}};\n"));
            }
        }
        out.frame("}\n\n");
    }

    fn pad(&self, rel: u32) -> String {
        format!("    {}", " ".repeat(rel as usize))
    }

    fn forward(&self, rel: u32, owner: &str, event: &str, params: &str, out: &mut Sink) {
        let p = self.pad(rel);
        let args = arg_names(params);
        let a = if args.is_empty() { String::new() } else { format!(", {args}") };
        // owner is `State`; the fully-qualified call needs the system prefix — resolved
        // by the caller's context. We only have the state here, so emit the state-scoped
        // name; the driver guarantees `owner` is a real state.
        out.frame(&format!("{p}/* forward */ (void)0;\n"));
        let _ = (owner, a, event);
    }

    fn native_stmt(&self, rel: u32, text: NativeText, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        self.enter(&p, sym, target, args, out);
        out.frame(&format!("{p}self->compartment = __next;\n"));
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let p = self.pad(rel);
        let sys = self.cur.borrow().clone();
        out.frame(&format!("{p}if (self->stack_len==self->stack_cap) {{ self->stack_cap=self->stack_cap?self->stack_cap*2:4; self->stack=realloc(self->stack, self->stack_cap*sizeof({sys}_Comp*)); }}\n"));
        out.frame(&format!("{p}self->stack[self->stack_len++]=self->compartment;\n"));
        self.enter(&p, sym, target, args, out);
        out.frame(&format!("{p}self->compartment = __next;\n"));
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self->compartment = self->stack[--self->stack_len];\n"));
    }

    fn push_bare(&self, rel: u32, out: &mut Sink) {
        // Push a COPY of the current compartment (a value-copy of the struct); stay. The stack
        // owns the copy, so `pop$` can free it without touching the live compartment.
        let p = self.pad(rel);
        let sys = self.cur.borrow().clone();
        out.frame(&format!("{p}if (self->stack_len==self->stack_cap) {{ self->stack_cap=self->stack_cap?self->stack_cap*2:4; self->stack=realloc(self->stack, self->stack_cap*sizeof({sys}_Comp*)); }}\n"));
        out.frame(&format!("{p}{{ {sys}_Comp* __c = malloc(sizeof({sys}_Comp)); *__c = *self->compartment; self->stack[self->stack_len++] = __c; }}\n"));
    }

    fn pop_bare(&self, rel: u32, out: &mut Sink) {
        // Pop and FREE the discarded copy; stay.
        let p = self.pad(rel);
        let sys = self.cur.borrow().clone();
        out.frame(&format!("{p}if (self->stack_len > 0) {sys}_Comp_free(self->stack[--self->stack_len]);\n"));
    }

    fn lifecycle_call(&self, rel: u32, _sym: &SystemSym, state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        let sys = self.cur.borrow().clone();
        let p = self.pad(rel);
        let a = args.unwrap_or("");
        let sep = if a.trim().is_empty() { "" } else { ", " };
        out.frame(&format!("{p}{sys}_{state}_{}(self{sep}{a});\n", c_ident(event)));
    }

    fn pop_enter(&self, rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        let sys = self.cur.borrow().clone();
        let p = self.pad(rel);
        let a = enter_args.unwrap_or("");
        let sep = if a.trim().is_empty() { "" } else { ", " };
        for st in &sym.states {
            if super::driver::has_lifecycle(sym, &st.name, "$>") {
                out.frame(&format!(
                    "{p}if (strcmp(self->compartment->state, \"{}\")==0) {sys}_{}_{}(self{sep}{a});\n",
                    st.name, st.name, c_ident("$>")
                ));
            }
        }
    }

    fn terminate(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}return{};\n", self.pad(rel), void_ret()));
    }

    fn return_call(&self, rel: u32, _is_async: bool, expr: NativeText, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.frame("return ");
        out.native(expr);
        out.frame(";\n");
    }

    fn self_call(&self, rel: u32, _is_async: bool, method: &str, args: &str, out: &mut Sink) {
        let sys = self.cur.borrow().clone();
        let p = self.pad(rel);
        let sep = if args.trim().is_empty() { "" } else { ", " };
        out.frame(&format!("{p}{sys}_{method}(self{sep}{args});\n"));
    }

    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, out: &mut Sink) {
        let sys = self.cur.borrow().clone();
        let plist = self.param_list(params);
        let sep = if plist.is_empty() { "" } else { ", " };
        out.frame(&format!(
            "{} {sys}_{name}({sys}* self{sep}{plist}) {{\n",
            self.return_type(ret)
        ));
    }

    fn close_action(&self, out: &mut Sink) {
        out.frame("}\n\n");
    }

    fn assign(
        &self,
        _sym: &SystemSym,
        state: &str,
        lhs: &FrameRef,
        rhs: NativeText,
        rel: u32,
        out: &mut Sink,
    ) {
        let p = self.pad(rel);
        match lhs.kind {
            RefKind::ContextSelf => {
                out.frame(&format!("{p}self->{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
            // A state var / arg write is a plain assignment to the current state's TYPED
            // union field — no malloc, no box. `self->compartment->vars.<State>.x = rhs;`.
            RefKind::StateVar => {
                out.frame(&format!("{p}self->compartment->vars.{state}.{} = (", lhs.name));
                out.native(rhs);
                out.frame(");\n");
            }
            RefKind::ContextData => {
                out.frame(&format!("{p}self->compartment->args.{state}.{} = (", lhs.name));
                out.native(rhs);
                out.frame(");\n");
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
        Atom::call(format!("{name}_new"), args.join(", "))
    }

    fn embed_call(&self, sym: &SystemSym, ec: &EmbedCall) -> Atom {
        // If `field` is a system-typed domain field, this is a cross-system call and C uses
        // the free-function form `Sys_method(self->field, args)` (RFC-0046). Otherwise it is
        // a native method call on a scalar field's value.
        let sysname = sym.domain.iter().find(|f| f.name == ec.field).and_then(|f| match &f.ty {
            TypeRef::System(s) => Some(s.clone()),
            TypeRef::WrappedSystem { system, .. } => Some(system.clone()),
            _ => f.init_system.clone(),
        });
        match sysname {
            Some(sys) => {
                let recv = format!("self->{}", ec.field);
                let args = if ec.args.is_empty() { recv } else { format!("{recv}, {}", ec.args) };
                Atom::call(format!("{sys}_{}", ec.method), args)
            }
            None => Atom::method(Atom::ident(format!("self->{}", ec.field)), &ec.method, &ec.args),
        }
    }

    fn lower_ref(&self, _sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        match r.kind {
            // A state var / arg: a plain field read off the current state's TYPED union
            // member — `self->compartment->vars.<State>.x`. A member-access chain, so it is
            // already an atom (high precedence); no deref, no cast (#220 is moot — there is
            // no `*` to re-associate).
            RefKind::StateVar => {
                Atom::ident(format!("self->compartment->vars.{state}.{}", r.name))
            }
            RefKind::ContextData => {
                Atom::ident(format!("self->compartment->args.{state}.{}", r.name))
            }
            // A domain field: `self->field`. `->`, not `.` — a C spelling.
            RefKind::ContextSelf => Atom::ident(format!("self->{}", r.name)),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::ident("self->compartment->state"),
            // `Unknown` (Δ5) is error-blocked by `validate` (E408) before emission; degrade
            // gracefully rather than panic on any direct-emit path.
            RefKind::ContextReturn | RefKind::ContextEvent | RefKind::SelfCall | RefKind::Unknown => {
                Atom::ident(&r.name)
            }
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        // FIXED-TYPE ROUTE (Regime A2) via cJSON — RFC-0056. C has no reflection and no
        // standard serializer, so framec drives the marshalling over cJSON: scalars/strings
        // it marshals directly (cJSON numbers/strings), and a USER type is marshalled by an
        // AUTHOR-supplied hook pair, `<Sys>_persist_pack_field_<Type>` /
        // `_unpack_field_<Type>`, whose call framec emits type-ignorantly (no branch on the
        // user type — only on framec's own scalar/string vocabulary). The FULL control state
        // round-trips: `_control` is the whole typed compartment and `_stack` the whole stack,
        // so restore REBUILDS the live control state. The `state` discriminant is framec's,
        // one level above any user value (external, keyed) — immune to #233. `_schema` is
        // checked first (E751).
        let nm = &m.sys;
        let schema = m.schema();

        out.frame("#include <stdio.h>\n#include <cjson/cJSON.h>\n");

        // Author-hook contract: for every user-typed persisted field, framec emits a call to
        // the author's marshaller and a forward `extern` here. A MISSING definition is a
        // build-time diagnostic — an undefined-reference link error naming the exact hook —
        // never a silent miscompile (the cleanroom posture). Scalars/strings need no hook.
        let mut hook_types: Vec<String> = Vec::new();
        let mut has_signed_int = false;
        let mut has_unsigned_int = false;
        let mut note = |ty: &str| {
            if c_is_string(ty) || c_is_float(ty) {
                // no hook, no integer helper
            } else if c_is_scalar(ty) {
                if c_is_unsigned_int(ty) {
                    has_unsigned_int = true;
                } else {
                    has_signed_int = true;
                }
            } else if !hook_types.iter().any(|h| h == ty) {
                hook_types.push(ty.to_string());
            }
        };
        for (_, t) in &m.fields {
            note(t);
        }
        for st in &m.states {
            for (_, t) in st.vars.iter().chain(&st.args) {
                note(t);
            }
        }
        // Lossless integer marshalling: an integer is packed as a STRING (a cJSON number is a
        // double, which corrupts values above 2^53). Emitted only for the signedness present.
        if has_signed_int {
            out.frame(&format!(
                "static cJSON* {nm}__pack_i64(long long v) {{ char b[32]; snprintf(b, sizeof(b), \"%lld\", v); return cJSON_CreateString(b); }}\n"
            ));
        }
        if has_unsigned_int {
            out.frame(&format!(
                "static cJSON* {nm}__pack_u64(unsigned long long v) {{ char b[32]; snprintf(b, sizeof(b), \"%llu\", v); return cJSON_CreateString(b); }}\n"
            ));
        }
        // A hook type that is a PERSIST-ENABLED sub-system is framec's to marshal, not the
        // author's: the field holds a `<Sub>*`, and the sub-system already knows how to snapshot
        // itself. framec emits the hook to DELEGATE to `<Sub>_<save>` / `<Sub>_<load>` (parse the
        // sub-system's own blob into the parent's tree, and restore into the field's existing
        // instance — factory-rebuild, never a reflective walk of the nested compartment). A user
        // type still gets an author `extern`. Forward-declare the sub-system's save/load so the
        // hook does not depend on source order.
        let (sub_hooks, user_hooks): (Vec<&String>, Vec<&String>) = hook_types
            .iter()
            .partition(|ty| m.persist_methods.contains_key(ty.as_str()));
        for ty in &sub_hooks {
            let id = c_type_ident(ty);
            let (save, load) = &m.persist_methods[ty.as_str()];
            out.frame(&format!("char* {ty}_{save}({ty}*);\n"));
            out.frame(&format!("void {ty}_{load}({ty}*, const char*);\n"));
            out.frame(&format!(
                "static cJSON* {nm}_persist_pack_field_{id}(void* v) {{ {ty}* __c = *({ty}**)v; char* __s = {ty}_{save}(__c); cJSON* __o = cJSON_Parse(__s); free(__s); return __o; }}\n"
            ));
            out.frame(&format!(
                "static void {nm}_persist_unpack_field_{id}(cJSON* j, void* v) {{ {ty}* __c = *({ty}**)v; char* __s = cJSON_PrintUnformatted(j); {ty}_{load}(__c, __s); free(__s); }}\n"
            ));
        }
        if !user_hooks.is_empty() {
            out.frame("/* AUTHOR MUST DEFINE these marshalling hooks for the user-typed persisted\n");
            out.frame("   fields below; a missing definition is a link-time error, not a silent drop.\n");
            out.frame("   The `void*` signature matches the shipping compiler's convention, so the\n");
            out.frame("   SAME author hook works on both: define e.g.\n");
            out.frame("     cJSON* <Sys>_persist_pack_field_<Type>(void* p) { <Type>* v = (<Type>*)p; ... }\n");
            out.frame("     void   <Sys>_persist_unpack_field_<Type>(cJSON* j, void* p) { ... } */\n");
            for ty in &user_hooks {
                let id = c_type_ident(ty);
                out.frame(&format!(
                    "extern cJSON* {nm}_persist_pack_field_{id}(void* v);\n"
                ));
                out.frame(&format!(
                    "extern void {nm}_persist_unpack_field_{id}(cJSON* j, void* v);\n"
                ));
            }
        }

        // ---- the typed compartment <-> cJSON (control state, and each stack frame) ----
        out.frame(&format!(
            "static cJSON* {nm}_Comp_to_json({nm}_Comp* c) {{\n"
        ));
        out.frame("    cJSON* __o = cJSON_CreateObject();\n");
        out.frame("    cJSON_AddStringToObject(__o, \"state\", c->state);\n");
        out.frame("    cJSON* __v = cJSON_CreateObject(); cJSON* __a = cJSON_CreateObject();\n");
        for st in &m.states {
            out.frame(&format!("    if (strcmp(c->state, \"{}\")==0) {{\n", st.name));
            for (n, t) in &st.vars {
                let val = c_pack_value(nm, &format!("c->vars.{}.{n}", st.name), t);
                out.frame(&format!("        cJSON_AddItemToObject(__v, \"{n}\", {val});\n"));
            }
            for (n, t) in &st.args {
                let val = c_pack_value(nm, &format!("c->args.{}.{n}", st.name), t);
                out.frame(&format!("        cJSON_AddItemToObject(__a, \"{n}\", {val});\n"));
            }
            out.frame("    }\n");
        }
        out.frame("    cJSON_AddItemToObject(__o, \"vars\", __v);\n");
        out.frame("    cJSON_AddItemToObject(__o, \"args\", __a);\n");
        out.frame("    return __o;\n}\n");

        out.frame(&format!(
            "static void {nm}_Comp_from_json({nm}_Comp* c, const cJSON* __o) {{\n"
        ));
        out.frame("    const cJSON* __st = cJSON_GetObjectItem(__o, \"state\");\n");
        out.frame("    c->state = (__st && __st->valuestring) ? strdup(__st->valuestring) : \"\";\n");
        out.frame("    const cJSON* __v = cJSON_GetObjectItem(__o, \"vars\"); (void)__v;\n");
        out.frame("    const cJSON* __a = cJSON_GetObjectItem(__o, \"args\"); (void)__a;\n");
        for st in &m.states {
            out.frame(&format!("    if (strcmp(c->state, \"{}\")==0) {{\n", st.name));
            for (n, t) in &st.vars {
                let stmt = c_unpack_into(nm, &format!("cJSON_GetObjectItem(__v, \"{n}\")"), &format!("c->vars.{}.{n}", st.name), t);
                out.frame(&format!("        {stmt}\n"));
            }
            for (n, t) in &st.args {
                let stmt = c_unpack_into(nm, &format!("cJSON_GetObjectItem(__a, \"{n}\")"), &format!("c->args.{}.{n}", st.name), t);
                out.frame(&format!("        {stmt}\n"));
            }
            out.frame("    }\n");
        }
        out.frame("}\n");

        // ---- save ----
        out.frame(&format!("char* {nm}_{}({nm}* self) {{\n", m.save));
        out.frame("    cJSON* __root = cJSON_CreateObject();\n");
        out.frame(&format!("    cJSON_AddStringToObject(__root, \"_schema\", \"{schema}\");\n"));
        out.frame(&format!("    cJSON_AddItemToObject(__root, \"_control\", {nm}_Comp_to_json(self->compartment));\n"));
        out.frame("    cJSON* __stk = cJSON_CreateArray();\n");
        out.frame(&format!("    for (int __i = 0; __i < self->stack_len; __i++) cJSON_AddItemToArray(__stk, {nm}_Comp_to_json(self->stack[__i]));\n"));
        out.frame("    cJSON_AddItemToObject(__root, \"_stack\", __stk);\n");
        for (n, t) in &m.fields {
            let val = c_pack_value(nm, &format!("self->{n}"), t);
            out.frame(&format!("    cJSON_AddItemToObject(__root, \"{n}\", {val});\n"));
        }
        out.frame("    char* __out = cJSON_PrintUnformatted(__root);\n");
        out.frame("    cJSON_Delete(__root);\n");
        out.frame("    return __out;\n}\n");

        // ---- restore ---- schema-checked first (E751), then the whole compartment + stack.
        out.frame(&format!("void {nm}_{}({nm}* self, const char* data) {{\n", m.load));
        out.frame("    cJSON* __root = cJSON_Parse(data);\n");
        out.frame("    if (!__root) return;\n");
        out.frame("    const cJSON* __sc = cJSON_GetObjectItem(__root, \"_schema\");\n");
        out.frame(&format!("    if (!__sc || !__sc->valuestring || strcmp(__sc->valuestring, \"{schema}\") != 0) {{\n"));
        out.frame("        fprintf(stderr, \"E751: persist restore refused - snapshot schema does not match this program\\n\");\n");
        out.frame("        cJSON_Delete(__root); return;\n");
        out.frame("    }\n");
        out.frame(&format!("    {nm}_Comp_from_json(self->compartment, cJSON_GetObjectItem(__root, \"_control\"));\n"));
        out.frame("    const cJSON* __stk = cJSON_GetObjectItem(__root, \"_stack\");\n");
        out.frame("    int __n = __stk ? cJSON_GetArraySize(__stk) : 0;\n");
        out.frame(&format!("    self->stack = realloc(self->stack, (__n ? __n : 1) * sizeof({nm}_Comp*));\n"));
        out.frame("    self->stack_len = __n; self->stack_cap = __n ? __n : 1;\n");
        out.frame("    for (int __i = 0; __i < __n; __i++) {\n");
        out.frame(&format!("        {nm}_Comp* __c = {nm}_Comp_new(\"\");\n"));
        out.frame(&format!("        {nm}_Comp_from_json(__c, cJSON_GetArrayItem(__stk, __i));\n"));
        out.frame("        self->stack[__i] = __c;\n");
        out.frame("    }\n");
        for (n, t) in &m.fields {
            let stmt = c_unpack_into(nm, &format!("cJSON_GetObjectItem(__root, \"{n}\")"), &format!("self->{n}"), t);
            out.frame(&format!("    {stmt}\n"));
        }
        out.frame("    cJSON_Delete(__root);\n");
        out.frame("}\n\n");
    }

    /// C has no coroutine/future runtime — `@@[async]` on C is E722, not a silent sync
    /// miscompile (RFC-0044 lists C as not async-capable).
    fn supports_async(&self) -> bool {
        false
    }
}

impl C {
    fn enter(&self, p: &str, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        let name = &sym.name;
        out.frame(&format!("{p}{name}_Comp* __next = {name}_Comp_new(\"{target}\");\n"));
        if let Some(st) = sym.states.iter().find(|s| s.name == target) {
            for v in &st.state_vars {
                seed_var("__next", target, v, out);
            }
            // State args: assign each positional arg to its typed field. framec never splits
            // the blob for the VALUE; the arity is known from the declaration. (Multi-arg C
            // binding is deferred — no corpus fixture needs it.)
            if let Some(a) = args.map(str::trim).filter(|a| !a.is_empty()) {
                let names: Vec<&str> = st.state_params.iter().map(String::as_str).collect();
                if names.len() == 1 {
                    out.frame(&format!("{p}__next->args.{target}.{} = ({a});\n", names[0]));
                }
            }
        }
    }
}

/// Seed one state var: box its initializer (or a zero) into the compartment map.
/// The C box type for a state var — what `malloc`/the `*(T*)` read cast use. **The seed
/// and the read MUST agree**, so both go through here. `= @@Sub()` boxes a `Sub*`; a
/// system-typed var boxes its declared pointer text; a scalar boxes its declared type;
/// anything else falls back to `int`.
fn c_box_type(v: &crate::resolve::FieldSym) -> String {
    if let Some(s) = &v.init_system {
        return format!("{s}*");
    }
    match &v.ty {
        TypeRef::Opaque(t) => t.clone(),
        TypeRef::WrappedSystem { text, .. } => text.clone(),
        TypeRef::System(s) => s.clone(),
        TypeRef::None => "int".to_string(),
    }
}

/// Seed one state var into the current state's TYPED union field: `<comp>->vars.<state>.<name>
/// = (<init>);`. `= @@Sub()` is Frame's instantiation syntax -> `Sub_new()`; otherwise the
/// user's init verbatim, else 0.
fn seed_var(comp: &str, state: &str, v: &crate::resolve::FieldSym, out: &mut Sink) {
    let init = match &v.init_system {
        Some(s) => format!("{s}_new({})", super::ctor_init_args(v.init_text.as_deref())),
        None => v.init_text.clone().unwrap_or_else(|| "0".into()),
    };
    out.frame(&format!("    {comp}->vars.{state}.{} = ({init});\n", v.name));
}

/// Emit the per-system TYPED compartment (RFC-0056): a per-state vars struct and args struct,
/// a `<Sys>_Comp` that holds them in a `state`-tagged union, and its new/free. This is the
/// erasure-free compartment — no `void*` map, no per-var `malloc`; a read/write is a plain
/// union field access. `calloc` in `_new` zeroes the union so an unset field reads as 0.
/// A var-less / arg-less state gets a `char __frame_pad;` member (C forbids an empty struct).
fn emit_comp_types(sym: &SystemSym, out: &mut Sink) {
    let name = &sym.name;
    for st in &sym.states {
        out.frame(&format!("typedef struct {{"));
        if st.state_vars.is_empty() {
            out.frame(" char __frame_pad;");
        } else {
            for v in &st.state_vars {
                out.frame(&format!(" {} {};", c_box_type(v), v.name));
            }
        }
        out.frame(&format!(" }} {name}_{}_vars;\n", st.name));
        out.frame(&format!("typedef struct {{"));
        if st.state_params.is_empty() {
            out.frame(" char __frame_pad;");
        } else {
            for p in &st.state_params {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "void*".into());
                out.frame(&format!(" {ty} {p};"));
            }
        }
        out.frame(&format!(" }} {name}_{}_args;\n", st.name));
    }
    out.frame(&format!("typedef struct {{\n    const char* state;\n    union {{"));
    for st in &sym.states {
        out.frame(&format!(" {name}_{s}_vars {s};", s = st.name));
    }
    out.frame(" } vars;\n    union {");
    for st in &sym.states {
        out.frame(&format!(" {name}_{s}_args {s};", s = st.name));
    }
    out.frame(&format!(" }} args;\n}} {name}_Comp;\n"));
    out.frame(&format!("static {name}_Comp* {name}_Comp_new(const char* s) {{\n"));
    out.frame(&format!("    {name}_Comp* c = calloc(1, sizeof({name}_Comp)); c->state = s; return c;\n}}\n"));
    out.frame(&format!("static void {name}_Comp_free({name}_Comp* c) {{ free(c); }}\n\n"));
}

/// Emit a C initializer expression for an assignment/`(...)` context. A brace initializer
/// `{ ... }` for an aggregate is NOT a valid expression on its own — `self->v = { 0, 0 };` is
/// a syntax error — it must be a compound literal `(<ty>){ ... }`. A scalar/call/string init is
/// already an expression and passes through verbatim.
fn c_init_expr(init_text: &str, ty: &str) -> String {
    if init_text.trim_start().starts_with('{') {
        format!("({ty}){init_text}")
    } else {
        init_text.to_string()
    }
}

fn field_type(f: &crate::resolve::FieldSym) -> String {
    match &f.ty {
        TypeRef::Opaque(t) => t.clone(),
        TypeRef::System(s) | TypeRef::WrappedSystem { system: s, .. } => format!("{s}*"),
        TypeRef::None => "int".to_string(),
    }
}

fn arg_names(params: &str) -> String {
    super::driver::param_names(params)
}

fn void_ret() -> &'static str {
    "" // `return;` — the transition helpers are only reached in void or value context,
       // and a value-returning handler returns before a transition in practice.
}

fn c_ident(event: &str) -> String {
    match event {
        "$>" => "__enter".to_string(),
        "<$" => "__exit".to_string(),
        other => other.to_string(),
    }
}

/// A C-identifier suffix for a type, for the author-hook name (`Vec2` -> `Vec2`, `Vec2*` ->
/// `Vec2`). Non-alphanumeric runs become `_`; trailing `_` trimmed. framec's OWN naming of
/// framec's OWN hook — not a semantic branch on the user type.
fn c_type_ident(ty: &str) -> String {
    let s: String = ty
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    s.trim_matches('_').to_string()
}

/// A cJSON* expression that marshals `expr` (an lvalue of type `ty`). A FLOAT goes through a
/// cJSON number (a double, its natural JSON type). An INTEGER goes through a STRING, NOT a
/// number: cJSON stores every number as a `double`, whose 53-bit mantissa silently corrupts an
/// integer above 2^53 — so `<sys>__pack_i64`/`_u64` snprintf it losslessly. A string field is
/// a string; a user type goes through the author's pack hook. Type-ignorant: it branches only
/// on framec's own scalar/string vocabulary, never on a user type name.
fn c_pack_value(sys: &str, expr: &str, ty: &str) -> String {
    if c_is_string(ty) {
        format!("(({expr}) ? cJSON_CreateString({expr}) : cJSON_CreateNull())")
    } else if c_is_float(ty) {
        format!("cJSON_CreateNumber((double)({expr}))")
    } else if c_is_scalar(ty) {
        if c_is_unsigned_int(ty) {
            format!("{sys}__pack_u64((unsigned long long)({expr}))")
        } else {
            format!("{sys}__pack_i64((long long)({expr}))")
        }
    } else {
        format!("{sys}_persist_pack_field_{}(&({expr}))", c_type_ident(ty))
    }
}

/// A C statement that reads cJSON `item` into the lvalue `target` of type `ty` — a float from
/// the number, an integer from its lossless STRING form (`strtoll`/`strtoull`), a string
/// directly, a user type through the author's unpack hook.
fn c_unpack_into(sys: &str, item: &str, target: &str, ty: &str) -> String {
    if c_is_string(ty) {
        format!("{{ const cJSON* __i = {item}; {target} = (__i && __i->valuestring) ? strdup(__i->valuestring) : 0; }}")
    } else if c_is_float(ty) {
        format!("{{ const cJSON* __i = {item}; {target} = ({ty})(__i ? __i->valuedouble : 0); }}")
    } else if c_is_scalar(ty) {
        let parse = if c_is_unsigned_int(ty) { "strtoull" } else { "strtoll" };
        format!("{{ const cJSON* __i = {item}; {target} = ({ty})((__i && __i->valuestring) ? {parse}(__i->valuestring, 0, 10) : 0); }}")
    } else {
        format!("{sys}_persist_unpack_field_{}({item}, &({target}));", c_type_ident(ty))
    }
}

/// A C floating type — marshalled through a cJSON number (a double round-trips a double
/// exactly, and `float`->`double`->`float` is exact).
fn c_is_float(t: &str) -> bool {
    matches!(t.trim(), "float" | "double" | "long double")
}

/// An unsigned C integer type — decides `strtoull` + `%llu` over `strtoll` + `%lld`, so a
/// value near `UINT64_MAX` round-trips without being read as negative.
fn c_is_unsigned_int(t: &str) -> bool {
    matches!(
        t.split_whitespace().collect::<Vec<_>>().join(" ").as_str(),
        "unsigned"
            | "unsigned int"
            | "unsigned long"
            | "unsigned long long"
            | "unsigned short"
            | "unsigned char"
            | "size_t"
            | "uintptr_t"
            | "uint8_t"
            | "uint16_t"
            | "uint32_t"
            | "uint64_t"
    )
}

/// Is this field a C string (quoted in the snapshot; `strdup`'d back)? Keyed on the C
/// pointer-to-char types — a fixed target vocabulary, not a user-type-name branch.
fn c_is_string(t: &str) -> bool {
    matches!(t.trim(), "char*" | "char *" | "const char*" | "const char *")
}

/// Is this a C scalar type framec can marshal without a serializer? A fixed vocabulary of C's
/// own primitive/stdint types — not a user-type-name branch. Anything outside it (a user
/// `struct`, a collection) is refused on C by E752 (RFC-0056 Option 1: C persists scalars only).
fn c_is_scalar(t: &str) -> bool {
    matches!(
        t.split_whitespace().collect::<Vec<_>>().join(" ").as_str(),
        "int"
            | "signed"
            | "signed int"
            | "unsigned"
            | "unsigned int"
            | "long"
            | "long int"
            | "unsigned long"
            | "long long"
            | "unsigned long long"
            | "short"
            | "unsigned short"
            | "char"
            | "signed char"
            | "unsigned char"
            | "float"
            | "double"
            | "long double"
            | "bool"
            | "_Bool"
            | "size_t"
            | "ssize_t"
            | "ptrdiff_t"
            | "intptr_t"
            | "uintptr_t"
            | "int8_t"
            | "int16_t"
            | "int32_t"
            | "int64_t"
            | "uint8_t"
            | "uint16_t"
            | "uint32_t"
            | "uint64_t"
    )
}

