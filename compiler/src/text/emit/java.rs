//! EMIT — Java. **The first backend.**
//!
//! Java, not Python, and the reason is not sentiment. Java has the simplest possible
//! delimiter (braces and `;`) and it is the **only target that exercises three of the
//! sixteen bugs at once**: the statement terminator, unreachable-code suppression (Java
//! is essentially alone in making dead code a *compile error*), and `await`-at-the-head.
//!
//! Everything in this file is a **spelling**. The control flow — walking the tree,
//! stopping after a transition, lowering the refs — lives once, in [`super::driver`],
//! which does not have the target language and therefore cannot branch on it.

use super::atom::{Atom, Place};
use super::driver::{param_names, params_split, Backend};
use super::Sink;
use crate::resolve::{SystemSym, TypeRef};
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

#[derive(Default)]
pub struct Java {
    /// The state params of the state currently being emitted: `(name, type)`.
    ///
    /// Set by `open_handler` from the SYMBOL TABLE — a fact framec put there. Not
    /// recovered by scanning anything.
    params: std::cell::RefCell<Vec<(String, String)>>,
}

impl Java {
    pub fn new() -> Java {
        Java::default()
    }
    fn state_params(&self, _state: &str) -> Vec<(String, String)> {
        self.params.borrow().clone()
    }
}

impl Backend for Java {
    fn name(&self) -> &'static str {
        "java"
    }

    /// Java: `int amount`. Type first, name second. The TYPE is the user's text.
    fn param_list(&self, params_text: &str) -> String {
        params_split(params_text)
            .into_iter()
            .map(|(n, t)| match t {
                // VERBATIM: the type is the user's target-language text (Q1 ruling).
                // framec reorders `name: type` -> `type name`; it does NOT translate the
                // type token. `str`/`bool` are the USER's names, valid or not — not
                // Frame keywords framec may rewrite.
                Some(t) => format!("{t} {n}"),
                None => format!("Object {n}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn file_header(&self, out: &mut Sink) {
        out.frame("import java.util.*;\n");
        out.frame("import java.util.concurrent.CompletableFuture;\n\n");
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        let name = &sym.name;
        out.frame(&format!("public class {name} {{\n"));
        out.frame("    static class Compartment {\n");
        out.frame("        String state;\n");
        out.frame("        Map<String, Object> stateVars = new HashMap<>();\n");
        out.frame("        Map<String, Object> stateArgs = new HashMap<>();\n");
        out.frame("        Compartment(String s) { this.state = s; }\n");
        out.frame("    }\n\n");
        out.frame("    private Compartment compartment;\n");
        // `push$`/`pop$` is a genuine PUSHDOWN. Frame has always had one.
        out.frame("    private Deque<Compartment> stack = new ArrayDeque<>();\n\n");

        // *** framec DOES NOT SPLIT THE ARGS. ***
        //
        // It hands them to a VARARGS helper and lets javac split them — correctly, for
        // free, arity error included.
        //
        // #218: in C++, `f(a < b, c > d)` (two comparisons) and `f(std::map<int,int>())`
        // (one generic) are the SAME token shape. Separating them needs name lookup over
        // the user's types — which a lexer cannot do, and which C++'s own grammar cannot
        // do either. A smarter splitter is not the answer, and neither is a parser. The
        // answer is to not compute the fact at all.
        out.frame(
            "    private static void __seedArgs(Compartment c, String[] names, Object... vals) {\n",
        );
        out.frame("        for (int i = 0; i < names.length && i < vals.length; i++) {\n");
        out.frame("            c.stateArgs.put(names[i], vals[i]);\n");
        out.frame("        }\n    }\n\n");

        // Domain fields. The type is emitted VERBATIM — Frame has no type system, and
        // the alias table (str->String, …) was exterminated as a contract violation.
        // A field typed `str` on Java emits `public str x;` and javac rejects it: that
        // is the USER writing a non-Java type name, not framec's job to fix.
        // Domain fields are DECLARED here without an initializer; they are ASSIGNED in the
        // constructor body, so a domain param (a constructor arg) is in scope for the
        // init (spec §88). A field-level `= init` could not see a constructor param.
        for f in &sym.domain {
            let ty = match &f.ty {
                TypeRef::Opaque(t) => t.clone(),
                TypeRef::System(s) => s.clone(),
                TypeRef::WrappedSystem { text, .. } => text.clone(),
                TypeRef::None => "Object".to_string(),
            };
            out.frame(&format!("    public {ty} {};\n", f.name));
        }
        out.frame("\n");

        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        // The constructor takes the header params — state, then enter, then domain (§203).
        let plist = self.param_list(&super::driver::ctor_params_text(&sym.params));
        out.frame(&format!("    public {name}({plist}) {{\n"));
        out.frame(&format!(
            "        this.compartment = new Compartment(\"{first}\");\n"
        ));
        if let Some(st) = sym.states.iter().find(|s| s.name == first) {
            for v in &st.state_vars {
                // The initializer is the USER'S — emitted verbatim, exactly as Rust/C do.
                // Seeding a hardcoded `0` dropped `$.n: int = 21` on the floor.
                let val = java_state_seed(v);
                out.frame(&format!(
                    "        this.compartment.stateVars.put(\"{}\", {val});\n",
                    v.name
                ));
            }
        }
        // State/enter params seed the start compartment's args (spec §203). The cleanroom
        // keeps one `stateArgs` map; a distinct `enter_args` is a deferred refinement.
        for p in sym.params.state.iter().chain(&sym.params.enter) {
            out.frame(&format!(
                "        this.compartment.stateArgs.put(\"{}\", {});\n",
                p.name, p.name
            ));
        }
        // Domain field assignments — now that domain params are in scope.
        for f in &sym.domain {
            let init = match (&f.init_system, &f.init_text) {
                (Some(s), _) => format!("new {s}()"),
                (None, Some(init)) => init.clone(),
                (None, None) => "null".to_string(),
            };
            out.frame(&format!("        this.{} = {init};\n", f.name));
        }
        out.frame("    }\n\n");
    }

    fn close_system(&self, _sym: &SystemSym, out: &mut Sink) {
        out.frame("}\n");
    }

    fn return_type(&self, t: Option<&str>) -> String {
        match t {
            Some(t) => t.to_string(), // VERBATIM
            None => "void".to_string(),
        }
    }

    /// Java's async is a **wrapped return type**: `CompletableFuture<String>`.
    fn async_return_type(&self, t: Option<&str>) -> String {
        match t {
            // The wrapped type is the user's, verbatim — but a Java generic parameter
            // cannot be a primitive, so a bare `int` must be its boxed form. That is
            // Java's own generics rule on framec's own CompletableFuture wrapper, not a
            // translation of the user's declared type (same category as container unbox).
            Some(t) => format!("CompletableFuture<{}>", java_box_name(t)),
            None => "CompletableFuture<Void>".to_string(),
        }
    }

    /// `CompletableFuture.completedFuture(v)` — a CALL, so it is already an atom.
    fn async_wrap(&self, v: Atom) -> Atom {
        Atom::call("CompletableFuture.completedFuture", v)
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
        let rt = if is_async {
            self.async_return_type(ret)
        } else {
            self.return_type(ret)
        };
        let ret_kw = if ret.is_some() { "return " } else { "" };
        out.frame(&format!(
            "    public {rt} {event}({}) {{\n",
            self.param_list(params)
        ));
        out.frame("        switch (this.compartment.state) {\n");
        // `state` is where the machine IS; `owner` is whose handler RUNS. Under HSM they
        // differ — the driver already resolved the parent chain.
        for (state, owner) in arms {
            out.frame(&format!(
                "            case \"{state}\": {ret_kw}{owner}_{event}({args});{}\n",
                if ret.is_some() { "" } else { " break;" }
            ));
        }
        out.frame("            default: break;\n");
        out.frame("        }\n");
        // A value-returning method needs a value on every path. framec KNOWS the method
        // returns a value, because the node says so — it does not scan its own output.
        if ret.is_some() || is_async {
            let z = if is_async {
                "CompletableFuture.completedFuture(null)".to_string()
            } else {
                java_zero(&rt).to_string()
            };
            out.frame(&format!("        return {z};\n"));
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
        // The state's declared params, from the SYMBOL TABLE. Types come from the state
        // header, which is Frame's syntax; the type TEXT is the user's and is untouched.
        *self.params.borrow_mut() = sym
            .states
            .iter()
            .find(|s| s.name == state)
            .map(|s| {
                s.state_params
                    .iter()
                    .map(|p| (p.clone(), s.state_param_types.get(p).cloned().unwrap_or_else(|| "Object".into())))
                    .collect()
            })
            .unwrap_or_default();
        out.frame(&format!(
            "    private {} {state}_{}({}) {{\n",
            if is_async {
                self.async_return_type(ret)
            } else {
                self.return_type(ret)
            },
            java_ident(event),
            self.param_list(params)
        ));
        out.frame("        Compartment compartment = this.compartment;\n");
        // *** BIND THE STATE PARAMS. ***
        //
        // `$Holding(value: int)` declares a state param, and the handler body refers to
        // it by bare name (`@@:(value)`). It lives in the compartment, so framec must
        // bind it as a local — otherwise the name simply does not exist in the method.
        for (n, t) in &self.state_params(state) {
            // Declared type VERBATIM; the RHS is a container extraction (Q3/Q4).
            let get = Atom::method(
                Atom::field(Atom::ident("compartment"), "stateArgs"),
                "get",
                format!("\"{n}\""),
            );
            out.frame(&format!("        {t} {n} = {};\n", java_unbox(t, get)));
        }
    }

    fn close_handler(&self, ret: Option<&str>, is_async: bool, terminated: bool, out: &mut Sink) {
        // A value-returning method that might FALL THROUGH needs a return. One that
        // already returned must NOT get another — unreachable code is a COMPILE ERROR in
        // Java, and this is exactly what `strip_java_unreachable` existed to clean up
        // after the fact.
        //
        // The driver KNOWS whether the body terminated, because it walked the tree. No
        // scanning of emitted text.
        if !terminated {
            if is_async {
                out.frame("        return CompletableFuture.completedFuture(null);\n");
            } else if let Some(t) = ret {
                out.frame(&format!("        return {};\n", java_zero(t)));
            }
        }
        out.frame("    }\n\n");
    }

    fn return_call(&self, rel: u32, is_async: bool, expr: NativeText, out: &mut Sink) {
        let p = self.pad(rel);
        if is_async {
            // `CompletableFuture.completedFuture(v)` — a CALL, so an ATOM. There is no
            // way to emit a bare `await` at the head here, because `Atom` has no
            // constructor for one (#225).
            out.frame(&format!("{p}return CompletableFuture.completedFuture("));
            out.native(expr);
            out.frame(");\n");
        } else {
            out.frame(&format!("{p}return "));
            out.native(expr);
            out.frame(";\n");
        }
    }

    fn self_call(&self, rel: u32, is_async: bool, method: &str, args: &str, out: &mut Sink) {
        let p = self.pad(rel);
        // Java's async is a `CompletableFuture`, so a reentrant call is `.join()`ed rather
        // than awaited. It is an ATOM by construction — `Atom::method` builds a postfix
        // chain rooted at an atom, so it can never bind to the wrong thing (#225).
        let call = Atom::call(format!("this.{method}"), args);
        let e = if is_async {
            Atom::method(call, "join", "")
        } else {
            call
        };
        out.frame(&format!("{p}{e};\n"));
    }

    fn forward(&self, rel: u32, owner: &str, event: &str, params: &str, out: &mut Sink) {
        let p = self.pad(rel);
        // `=> $^` — run the PARENT's handler for this same event.
        out.frame(&format!("{p}{owner}_{event}({});\n", param_names(params)));
    }

    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, out: &mut Sink) {
        out.frame(&format!(
            "    private {} {name}({}) {{\n",
            self.return_type(ret),
            self.param_list(params)
        ));
        out.frame("        Compartment compartment = this.compartment;\n");
    }

    fn close_action(&self, out: &mut Sink) {
        out.frame("    }\n\n");
    }

    /// Java: braces carry the nesting, so the indent is cosmetic. But reproducing the
    /// user's relative nesting keeps their code readable in the output, and costs nothing.
    fn pad(&self, rel: u32) -> String {
        format!("        {}", " ".repeat(rel as usize))
    }

    fn native_stmt(&self, rel: u32, text: NativeText, out: &mut Sink) {
        // framec terminates only what IT emits. It never adds, removes, or moves a
        // native terminator — that `;` is the user's, and it is not ours to touch. (The
        // old compiler searched its own emitted string for the last non-whitespace byte
        // to decide, and landed a `;` inside a comment.)
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    fn transition(&self, _rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        self.enter(sym, target, args, out);
        out.frame("        this.compartment = __next;\n");
    }

    fn push(&self, _rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        out.frame("        this.stack.push(this.compartment);\n");
        self.enter(sym, target, args, out);
        out.frame("        this.compartment = __next;\n");
    }

    fn pop(&self, _rel: u32, out: &mut Sink) {
        out.frame("        this.compartment = this.stack.pop();\n");
    }

    fn lifecycle_call(&self, _rel: u32, _sym: &SystemSym, state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        out.frame(&format!("        {state}_{}({});\n", java_ident(event), args.unwrap_or("")));
    }

    fn pop_enter(&self, _rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        let a = enter_args.unwrap_or("");
        for st in &sym.states {
            if super::driver::has_lifecycle(sym, &st.name, "$>") {
                out.frame(&format!(
                    "        if (this.compartment.state.equals(\"{}\")) {}_{}({a});\n",
                    st.name, st.name, java_ident("$>")
                ));
            }
        }
    }

    fn terminate(&self, _rel: u32, out: &mut Sink) {
        out.frame("        return;\n");
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
            // A domain field IS an lvalue. `this.total = rhs;`
            //
            // And note it is NOT parenthesized. `(this.total) = 3;` is a compile error in
            // Java — which is exactly why `Place` is a separate type from `Atom` and has
            // no `group()` constructor.
            RefKind::ContextSelf => {
                let place = Place::field(Place::ident("this"), &lhs.name);
                out.frame(&format!("{p}{place} = "));
                out.native(rhs);
                // *** framec terminates what framec emits. ***
                // Unconditionally. It does not look at what it just wrote to decide —
                // that oracle put a `;` inside a comment (#173) and omitted it entirely
                // on other paths (#229).
                out.frame(";\n");
            }
            // A state variable lives in a Map. There is NO lvalue: it is a container
            // operation. A different statement, not a different spelling.
            RefKind::StateVar => {
                out.frame(&format!(
                    "        compartment.stateVars.put(\"{}\", ",
                    lhs.name
                ));
                out.native(rhs);
                out.frame(");\n");
            }
            RefKind::ContextData => {
                out.frame(&format!(
                    "        compartment.stateArgs.put(\"{}\", ",
                    lhs.name
                ));
                out.native(rhs);
                out.frame(");\n");
            }
            // `@@:return = e` / `@@:(e)` — set the return value. Without a context slot,
            // that is `return e;` (matching the concise `@@:(e)` form). The `_` fallthrough
            // used to emit `return = e;`, which is invalid.
            RefKind::ContextReturn => {
                out.frame("        return ");
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
        Atom::call(format!("new {name}"), args.join(", "))
    }

    fn embed_call(&self, _sym: &SystemSym, ec: &EmbedCall) -> Atom {
        // Java spells the system and scalar cases identically: `this.field.method(args)`.
        Atom::method(Atom::field(Atom::ident("this"), &ec.field), &ec.method, &ec.args)
    }

    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        let comp = Atom::ident("compartment");
        match r.kind {
            // A state variable lives in a `Map<String,Object>`, so reading it needs a
            // cast — and `Atom::cast` PARENTHESIZES. That is #213, and it is not
            // something this function has to remember: there is no constructor that
            // would produce the bare form.
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
                    .unwrap_or_else(|| "Object".to_string());
                // Container extraction out of framec's own `Map<String,Object>` — Q3.
                java_unbox(
                    &ty,
                    Atom::method(
                        Atom::field(comp, "stateVars"),
                        "get",
                        format!("\"{}\"", r.name),
                    ),
                )
            }
            RefKind::ContextData => Atom::method(
                Atom::field(comp, "stateArgs"),
                "get",
                format!("\"{}\"", r.name),
            ),
            // `this.field`. An identifier chain — already an atom, and it MUST NOT be
            // parenthesized, because it is also an lvalue root (`@@:self.field = 3`).
            // That asymmetry is exactly why `Place` is a separate type from `Atom`.
            RefKind::ContextSelf => Atom::field(Atom::ident("this"), &r.name),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::field(comp, "state"),
            RefKind::ContextReturn | RefKind::ContextEvent | RefKind::SelfCall => {
                Atom::ident(&r.name)
            }
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        // FIXED-TYPE ROUTE (Regime A) via Gson — RFC-0056. A user type self-marshals through
        // the host serializer; framec names the fields and Gson does ALL type work — nesting,
        // collections, user types, string escaping. Strictly type-IGNORANT: no `match
        // user_type`. Gson deserializes each field INTO its declared type (read off the `__Snap`
        // field via reflection), never a type name from the blob — immune to #233 by
        // construction. Java permits a `static` nested class inside the class, so `__Snap` (the
        // schema + control state + fields) lives here beside its use. Requires Gson on the
        // classpath (the self-marshalling requirement). `_schema` is checked first (E751).
        let schema = m.schema();

        out.frame(&format!("    public String {}() {{\n", m.save));
        out.frame("        __Snap __s = new __Snap();\n");
        out.frame(&format!("        __s._schema = \"{schema}\";\n"));
        out.frame("        __s._control = this.compartment.state;\n");
        for (n, _) in &m.fields {
            out.frame(&format!("        __s.{n} = this.{n};\n"));
        }
        out.frame("        return new com.google.gson.Gson().toJson(__s);\n");
        out.frame("    }\n\n");

        out.frame(&format!("    public void {}(String data) {{\n", m.load));
        out.frame("        __Snap __s = new com.google.gson.Gson().fromJson(data, __Snap.class);\n");
        out.frame(&format!("        if (!__s._schema.equals(\"{schema}\")) {{\n"));
        out.frame("            throw new RuntimeException(\"E751: persist restore refused - snapshot schema does not match this program\");\n");
        out.frame("        }\n");
        out.frame("        this.compartment.state = __s._control;\n");
        for (n, _) in &m.fields {
            out.frame(&format!("        this.{n} = __s.{n};\n"));
        }
        out.frame("    }\n\n");

        // The snapshot shape — a plain data class Gson reflects over. Each field carries its
        // DECLARED Java type, so `fromJson` reconstructs into exactly that type.
        out.frame("    private static class __Snap {\n");
        out.frame("        String _schema;\n");
        out.frame("        String _control;\n");
        for (n, t) in &m.fields {
            out.frame(&format!("        {t} {n};\n"));
        }
        out.frame("    }\n\n");
    }

    /// **Java is essentially the only target where dead code is a compile error.**
    /// A `bool` in a table. The old compiler expressed the same fact as
    /// `strip_java_unreachable` — a post-emission pass that deleted statements out of
    /// text it had just generated.
    fn dead_code_is_an_error(&self) -> bool {
        true
    }
}

impl Java {
    fn enter(&self, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        out.frame(&format!(
            "        Compartment __next = new Compartment(\"{target}\");\n"
        ));
        if let Some(st) = sym.states.iter().find(|s| s.name == target) {
            for v in &st.state_vars {
                let val = java_state_seed(v);
                out.frame(&format!(
                    "        __next.stateVars.put(\"{}\", {val});\n",
                    v.name
                ));
            }
            if let Some(a) = args.map(str::trim).filter(|a| !a.is_empty()) {
                if !st.state_params.is_empty() {
                    let names = st
                        .state_params
                        .iter()
                        .map(|p| format!("\"{p}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.frame(&format!(
                        "        __seedArgs(__next, new String[]{{{names}}}, {a});\n"
                    ));
                }
            }
        }
    }
}

/// The seed value for a state var: `= @@Sub()` -> `new Sub()` (Frame's instantiation
/// syntax), else the user's init verbatim, else `null`.
fn java_state_seed(v: &crate::resolve::FieldSym) -> String {
    match &v.init_system {
        Some(s) => format!("new {s}()"),
        None => v
            .init_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("null")
            .to_string(),
    }
}


/// **Container extraction (Q3/Q4).** Pull a value of declared type `t` out of framec's
/// own `Object` container (a `Map`/`List` it generates for the compartment). This is NOT
/// a translation of the user's type — it is Java's own rule that an `Object` cannot be
/// cast to a primitive, applied to framec's own scaffolding.
///
/// Keyed on Java's fixed primitive keyword set, with a VERBATIM `(t) x` fallback for every
/// reference type — no branch on user type names.
///
/// Prefers the Number-ladder `((Number) x).intValue()` over `(Integer) x`: a value that
/// round-tripped through persist's JSON comes back `Long`/`BigDecimal`, and the ladder
/// survives that where a hard `(Integer)` cast throws `ClassCastException`.
fn java_unbox(t: &str, x: Atom) -> Atom {
    match t.trim() {
        "int" | "long" | "short" | "byte" | "double" | "float" => {
            let m = match t.trim() {
                "int" => "intValue",
                "long" => "longValue",
                "short" => "shortValue",
                "byte" => "byteValue",
                "double" => "doubleValue",
                "float" => "floatValue",
                _ => unreachable!(),
            };
            Atom::method(Atom::cast("Number", x), m, "")
        }
        "boolean" => Atom::method(Atom::cast("Boolean", x), "booleanValue", ""),
        "char" => Atom::method(Atom::cast("Character", x), "charValue", ""),
        // Reference type: a plain verbatim cast. NO user-type-name branch.
        other => Atom::cast(other, x),
    }
}

/// The boxed spelling of a primitive, for use ONLY where a Java generic parameter forbids
/// a primitive (framec's `CompletableFuture<...>` wrapper). Reference types pass verbatim.
fn java_box_name(t: &str) -> String {
    match t.trim() {
        "int" => "Integer",
        "long" => "Long",
        "short" => "Short",
        "byte" => "Byte",
        "double" => "Double",
        "float" => "Float",
        "boolean" => "Boolean",
        "char" => "Character",
        other => other,
    }
    .to_string()
}

/// Exposed for the acceptance test: the state-var read must be an ATOM.
pub fn state_var_read(java_type: &str, name: &str) -> Atom {
    Atom::cast(
        java_type,
        Atom::method(
            Atom::field(Atom::ident("compartment"), "stateVars"),
            "get",
            format!("\"{name}\""),
        ),
    )
}


/// Frame's lifecycle event names are not Java identifiers.
fn java_ident(event: &str) -> String {
    match event {
        "$>" => "__enter".to_string(),
        "<$" => "__exit".to_string(),
        other => other.to_string(),
    }
}

/// A zero value for a Java type, so a routed method returns something on every path.
fn java_zero(t: &str) -> &'static str {
    match t {
        "int" | "long" | "short" | "byte" => "0",
        // `0.0` is a DOUBLE in Java; assigning it to a float is a lossy conversion and a
        // compile error. The suffix is not a detail — it is the difference between
        // compiling and not.
        "float" => "0.0f",
        "double" => "0.0",
        "boolean" => "false",
        "char" => "'\\0'",
        _ => "null",
    }
}
