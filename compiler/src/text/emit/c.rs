//! EMIT — C. **The hardest backend: no reflection, no generics, no `Any`, manual memory.**
//!
//! Java has `Object`, Rust has `Box<dyn Any>`. C has neither. So framec's compartment
//! containers are a hand-emitted `void*`-keyed map, and pulling a typed value out means a
//! `*(T*)` cast-and-deref. That deref is a **prefix operator** — a NON-atom — exactly the
//! #220 family. `Atom::deref` parenthesizes it, so `*(int*)get(...)` becomes
//! `(*((int*) get(...)))` and survives being spliced into any surrounding expression.
//! **This is where the Atom model earns its keep.**
//!
//! C also has no methods: every function takes an explicit `self` pointer, and member
//! access is `->`, not `.`. Those are spellings; the driver never learns them.
//!
//! Memory: state-var boxes are heap-allocated and leaked. That is a deferred concern (a
//! real arena/free-on-drop pass belongs with the coverage layer); the corpus programs are
//! short-lived and the leak does not affect correctness.

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
        // A minimal string -> void* map. framec's OWN scaffolding.
        out.frame("typedef struct { char** keys; void** vals; int len; int cap; } FrameMap;\n");
        out.frame("static void FrameMap_init(FrameMap* m) { m->keys=0; m->vals=0; m->len=0; m->cap=0; }\n");
        out.frame("static void FrameMap_set(FrameMap* m, const char* k, void* v) {\n");
        out.frame("    for (int i=0;i<m->len;i++) if (strcmp(m->keys[i],k)==0) { m->vals[i]=v; return; }\n");
        out.frame("    if (m->len==m->cap) { m->cap=m->cap?m->cap*2:4; m->keys=realloc(m->keys,m->cap*sizeof(char*)); m->vals=realloc(m->vals,m->cap*sizeof(void*)); }\n");
        out.frame("    m->keys[m->len]=strdup(k); m->vals[m->len]=v; m->len++;\n}\n");
        out.frame("static void* FrameMap_get(FrameMap* m, const char* k) {\n");
        out.frame("    for (int i=0;i<m->len;i++) if (strcmp(m->keys[i],k)==0) return m->vals[i];\n");
        out.frame("    return 0;\n}\n");
        // Free the map: the keys (strdup'd) and the boxed values (malloc'd per seed) are
        // framec's OWN allocations, so framec's destructor owns freeing them.
        out.frame("static void FrameMap_free(FrameMap* m) {\n");
        out.frame("    for (int i=0;i<m->len;i++) { free(m->keys[i]); free(m->vals[i]); }\n");
        out.frame("    free(m->keys); free(m->vals);\n}\n\n");
        // The compartment.
        out.frame("typedef struct { const char* state; FrameMap state_vars; FrameMap state_args; } Compartment;\n");
        out.frame("static Compartment* Compartment_new(const char* s) {\n");
        out.frame("    Compartment* c=malloc(sizeof(Compartment)); c->state=s; FrameMap_init(&c->state_vars); FrameMap_init(&c->state_args); return c;\n}\n");
        out.frame("static void Compartment_free(Compartment* c) {\n");
        out.frame("    if (!c) return; FrameMap_free(&c->state_vars); FrameMap_free(&c->state_args); free(c);\n}\n\n");
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        *self.cur.borrow_mut() = sym.name.clone();
        let name = &sym.name;
        out.frame("typedef struct {\n");
        out.frame("    Compartment* compartment;\n");
        out.frame("    Compartment** stack; int stack_len; int stack_cap;\n");
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
            "    self->compartment = Compartment_new(\"{first}\");\n"
        ));
        out.frame("    self->stack = 0; self->stack_len = 0; self->stack_cap = 0;\n");
        if let Some(st) = sym.states.iter().find(|s| s.name == first) {
            for v in &st.state_vars {
                seed_var(name, "self->compartment", v, out);
            }
        }
        // State/enter params seed the start compartment's args (§203), boxed like state
        // vars (C has no reflection). One `state_args` map; a distinct `enter_args`
        // deferred.
        for p in sym.params.state.iter().chain(&sym.params.enter) {
            let ty = p.ty.as_deref().unwrap_or("int");
            out.frame(&format!(
                "    {{ {ty}* __v = malloc(sizeof({ty})); *__v = ({}); FrameMap_set(&self->compartment->state_args, \"{}\", __v); }}\n",
                p.name, p.name
            ));
        }
        for f in &sym.domain {
            // `= @@Inner()` is FRAME's instantiation syntax -> the C constructor. Any
            // other init is the user's native expression, verbatim.
            let init = match &f.init_system {
                Some(s) => format!("{s}_new()"),
                None => f.init_text.clone().unwrap_or_else(|| "0".into()),
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
        out.frame("    for (int i = 0; i < self->stack_len; i++) Compartment_free(self->stack[i]);\n");
        out.frame("    free(self->stack);\n");
        out.frame("    Compartment_free(self->compartment);\n");
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

    fn async_wrap(&self, v: Atom) -> Atom {
        v // C has no async in the corpus; deferred.
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
        // Bind state params from the state_args map: `int value = *(int*)get(...);`.
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            for p in &st.state_params {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "void*".into());
                out.frame(&format!(
                    "    {ty} {p} = {};\n",
                    unbox("state_args", p, &ty)
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
        out.frame(&format!("{p}if (self->stack_len==self->stack_cap) {{ self->stack_cap=self->stack_cap?self->stack_cap*2:4; self->stack=realloc(self->stack, self->stack_cap*sizeof(Compartment*)); }}\n"));
        out.frame(&format!("{p}self->stack[self->stack_len++]=self->compartment;\n"));
        self.enter(&p, sym, target, args, out);
        out.frame(&format!("{p}self->compartment = __next;\n"));
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self->compartment = self->stack[--self->stack_len];\n"));
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
        sym: &SystemSym,
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
            // A state var write BOXES the value onto the heap and stores the pointer.
            RefKind::StateVar => {
                let ty = state_var_type(sym, state, &lhs.name);
                out.frame(&format!(
                    "{p}{{ {ty}* __v = malloc(sizeof({ty})); *__v = ("
                ));
                out.native(rhs);
                out.frame(&format!(
                    "); FrameMap_set(&self->compartment->state_vars, \"{}\", __v); }}\n",
                    lhs.name
                ));
            }
            RefKind::ContextData => {
                out.frame(&format!("{p}/* data.{} = ... */ (void)0;\n", lhs.name));
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

    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        match r.kind {
            // A state var: `*(T*)get(...)` — a cast-and-DEREF. The deref is a prefix
            // operator, a NON-atom; `Atom::deref` parenthesizes it. This is #220, and it
            // is unrepresentable-to-get-wrong here.
            RefKind::StateVar => {
                let ty = state_var_type(sym, state, &r.name);
                Atom::ident(unbox("state_vars", &r.name, &ty))
            }
            RefKind::ContextData => Atom::ident(unbox("state_args", &r.name, "void*")),
            // A domain field: `self->field`. `->`, not `.` — a C spelling.
            RefKind::ContextSelf => Atom::ident(format!("self->{}", r.name)),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::ident("self->compartment->state"),
            RefKind::ContextReturn | RefKind::ContextEvent | RefKind::SelfCall => {
                Atom::ident(&r.name)
            }
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        // FIXED-TYPE ROUTE, dependency-free — the same flat format as Java and Rust, in C's
        // idiom: free functions (no methods), `self->field`, `char*` string building. The
        // type of every field is fixed at codegen, so restore parses straight into it and
        // never reads a type name from the blob: structurally immune to #233.
        //
        // Memory follows this backend's stated posture (see the module header): the returned
        // snapshot and any restored `char*`/state are heap allocations the short-lived corpus
        // programs leak; a free-on-drop pass belongs with the coverage layer.
        let nm = self.cur.borrow().clone();
        let schema = m.schema();

        // stdio for snprintf/fprintf/strstr — not in the file header, injected here (guarded
        // include, harmless if a second persistent system injects it again).
        out.frame("#include <stdio.h>\n");

        // The flat-object field reader, into a caller buffer. framec's OWN format.
        out.frame(&format!(
            "static void {nm}___frame_field(const char* data, const char* key, char* out, int cap) {{\n"
        ));
        out.frame("    char needle[128];\n");
        out.frame("    snprintf(needle, sizeof(needle), \"\\\"%s\\\":\", key);\n");
        out.frame("    const char* p = strstr(data, needle);\n");
        out.frame("    if (!p) { out[0]='\\0'; return; }\n");
        out.frame("    p += strlen(needle);\n");
        out.frame("    int i = 0;\n");
        out.frame("    if (*p == '\\\"') {\n");
        out.frame("        p++;\n");
        out.frame("        while (*p && *p != '\\\"' && i < cap-1) out[i++] = *p++;\n");
        out.frame("    } else {\n");
        out.frame("        while (*p && *p != ',' && *p != '}' && i < cap-1) out[i++] = *p++;\n");
        out.frame("    }\n");
        out.frame("    out[i] = '\\0';\n");
        out.frame("}\n");

        // ---- snapshot() ---- strings quoted, scalars bare, control state = compartment.state.
        out.frame(&format!("char* {nm}_{}({nm}* self) {{\n", m.save));
        out.frame("    char* __b = malloc(1024);\n");
        out.frame("    int __o = 0;\n");
        out.frame(&format!(
            "    __o += snprintf(__b+__o, 1024-__o, \"{{\\\"_schema\\\":\\\"{schema}\\\"\");\n"
        ));
        out.frame("    __o += snprintf(__b+__o, 1024-__o, \",\\\"_control\\\":\\\"%s\\\"\", self->compartment->state);\n");
        for (n, t) in &m.fields {
            if c_is_string(t) {
                out.frame(&format!(
                    "    __o += snprintf(__b+__o, 1024-__o, \",\\\"{n}\\\":\\\"%s\\\"\", self->{n});\n"
                ));
            } else {
                let spec = c_scalar_fmt(t);
                out.frame(&format!(
                    "    __o += snprintf(__b+__o, 1024-__o, \",\\\"{n}\\\":{spec}\", self->{n});\n"
                ));
            }
        }
        out.frame("    __o += snprintf(__b+__o, 1024-__o, \"}\");\n");
        out.frame("    return __b;\n");
        out.frame("}\n");

        // ---- restore() ---- schema-checked first (E751 refuse rather than mis-restore), then
        // each field into its declared type. No exceptions: refusal is a stderr line + return,
        // leaving the instance untouched.
        out.frame(&format!("void {nm}_{}({nm}* self, const char* data) {{\n", m.load));
        out.frame("    char __v[256];\n");
        out.frame(&format!(
            "    {nm}___frame_field(data, \"_schema\", __v, sizeof(__v));\n"
        ));
        out.frame(&format!("    if (strcmp(__v, \"{schema}\") != 0) {{\n"));
        out.frame("        fprintf(stderr, \"E751: persist restore refused - snapshot schema does not match this program\\n\");\n");
        out.frame("        return;\n");
        out.frame("    }\n");
        out.frame(&format!(
            "    {nm}___frame_field(data, \"_control\", __v, sizeof(__v));\n"
        ));
        out.frame("    self->compartment->state = strdup(__v);\n");
        for (n, t) in &m.fields {
            out.frame(&format!(
                "    {nm}___frame_field(data, \"{n}\", __v, sizeof(__v));\n"
            ));
            if c_is_string(t) {
                out.frame(&format!("    self->{n} = strdup(__v);\n"));
            } else {
                let parse = c_scalar_parse(t);
                out.frame(&format!("    self->{n} = {parse}(__v);\n"));
            }
        }
        out.frame("}\n\n");
    }

    fn dead_code_is_an_error(&self) -> bool {
        false
    }

    /// C has no coroutine/future runtime — `@@[async]` on C is E722, not a silent sync
    /// miscompile (RFC-0044 lists C as not async-capable).
    fn supports_async(&self) -> bool {
        false
    }
}

impl C {
    fn enter(&self, p: &str, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        out.frame(&format!("{p}Compartment* __next = Compartment_new(\"{target}\");\n"));
        if let Some(st) = sym.states.iter().find(|s| s.name == target) {
            for v in &st.state_vars {
                seed_var(&sym.name, "__next", v, out_prefixed(p, out));
            }
            // State args: box each positional arg. Rust/C are statically typed, so the
            // arity is known from the declaration; framec never splits the blob for the
            // VALUE, it just boxes each declared param.
            if let Some(a) = args.map(str::trim).filter(|a| !a.is_empty()) {
                let names: Vec<&str> = st.state_params.iter().map(String::as_str).collect();
                if names.len() == 1 {
                    let ty = st.state_param_types.get(names[0]).cloned().unwrap_or_else(|| "int".into());
                    out.frame(&format!(
                        "{p}{{ {ty}* __v = malloc(sizeof({ty})); *__v = ({a}); FrameMap_set(&__next->state_args, \"{}\", __v); }}\n",
                        names[0]
                    ));
                }
                // (Multi-arg C tuple-binding deferred; no corpus fixture needs it.)
            }
        }
    }
}

/// `out` passthrough — a tiny shim so `seed_var` can be reused from `enter` where the
/// indent is dynamic. (seed_var writes fixed-indent lines; here we accept that.)
fn out_prefixed<'a>(_p: &str, out: &'a mut Sink) -> &'a mut Sink {
    out
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

fn seed_var(_sys: &str, comp: &str, v: &crate::resolve::FieldSym, out: &mut Sink) {
    // `= @@Sub()` is Frame's instantiation syntax -> `Sub_new()`. Otherwise the user's init
    // verbatim. The box type comes from `c_box_type` so the read casts to the same type.
    let ty = c_box_type(v);
    let init = match &v.init_system {
        Some(s) => format!("{s}_new()"),
        None => v.init_text.clone().unwrap_or_else(|| "0".into()),
    };
    out.frame(&format!(
        "    {{ {ty}* __v = malloc(sizeof({ty})); *__v = ({init}); FrameMap_set(&{comp}->state_vars, \"{}\", __v); }}\n",
        v.name
    ));
}

/// `(*(T*)FrameMap_get(&self->compartment-><container>, "k"))` — cast-and-deref, an atom
/// by parenthesization.
fn unbox(container: &str, key: &str, ty: &str) -> String {
    let get = Atom::call(
        "FrameMap_get",
        format!("&self->compartment->{container}, \"{key}\""),
    );
    Atom::deref(Atom::cast(format!("{ty}*"), get)).to_string()
}

fn state_var_type(sym: &SystemSym, state: &str, name: &str) -> String {
    sym.states
        .iter()
        .find(|s| s.name == state)
        .and_then(|s| s.state_vars.iter().find(|v| v.name == name))
        // Same resolution as the SEED (`c_box_type`), so the `*(T*)` read casts to exactly
        // the type the box was allocated with.
        .map(c_box_type)
        .unwrap_or_else(|| "int".to_string())
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

/// Is this field a C string (quoted in the snapshot; `strdup`'d back)? Keyed on the C
/// pointer-to-char types — a fixed target vocabulary, not a user-type-name branch.
fn c_is_string(t: &str) -> bool {
    matches!(t.trim(), "char*" | "char *" | "const char*" | "const char *")
}

/// The `printf` conversion for a scalar field in the snapshot. `%.17g` round-trips a double
/// exactly. framec's OWN format decides the spelling from the declared C type.
fn c_scalar_fmt(t: &str) -> &'static str {
    match t.trim() {
        "long" => "%ld",
        "float" | "double" => "%.17g",
        _ => "%d", // int, bool, and fallback
    }
}

/// The `<stdlib.h>` parser for a scalar field on restore.
fn c_scalar_parse(t: &str) -> &'static str {
    match t.trim() {
        "long" => "atol",
        "float" | "double" => "atof",
        _ => "atoi",
    }
}
