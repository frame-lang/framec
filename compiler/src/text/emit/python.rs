//! EMIT — Python. **The second backend, chosen because it is maximally unlike Java.**
//!
//! Indentation instead of braces. No statement terminator. No casts — a dict read *is*
//! the value. If the shared driver survives Java **and** Python, it will survive the
//! rest; if it needed an escape hatch for either, the structure would be wrong and we
//! would want to know now rather than at backend eleven.
//!
//! It did not need one. Everything below is a **spelling**.
//!
//! # What Python proves about `Atom`
//!
//! Python's `$.x` read is `compartment.state_vars["x"]` — a postfix chain, already an
//! atom, no parentheses needed. Java's is `((Integer) compartment.stateVars.get("x"))` —
//! a cast, which **must** be parenthesized.
//!
//! Same node, same lowering interface, different spellings, and **neither backend had to
//! know the atom rule**. `Atom::index` returns an atom because it builds a postfix chain;
//! `Atom::cast` returns an atom because it parenthesizes. The invariant lives in the
//! type, not in sixteen authors' memories.

use super::atom::{Atom, Place};
use super::driver::{param_names, Backend};
use super::Sink;
use crate::resolve::SystemSym;
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

pub struct Python;

impl Backend for Python {
    fn name(&self) -> &'static str {
        "python"
    }

    /// Python: names only. The type annotation is dropped — Python does not need it,
    /// and inventing one would be framec pretending to have a type system.
    fn param_list(&self, params_text: &str) -> String {
        param_names(params_text)
    }

    fn file_header(&self, out: &mut Sink) {
        out.frame(PRELUDE);
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        out.frame(&format!("class {}:\n", sym.name));
        out.frame("    class Compartment:\n");
        out.frame("        def __init__(self, state):\n");
        out.frame("            self.state = state\n");
        out.frame("            self.state_vars = {}\n");
        out.frame("            self.state_args = {}\n\n");

        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        // Constructor params — state, then enter, then domain (§203). Python: names only.
        let plist = self.param_list(&super::driver::ctor_params_text(&sym.params));
        let sig = if plist.is_empty() { String::new() } else { format!(", {plist}") };
        out.frame(&format!("    def __init__(self{sig}):\n"));
        out.frame(&format!(
            "        self.__compartment = {}.Compartment(\"{first}\")\n",
            sym.name
        ));
        for v in sym
            .states
            .iter()
            .find(|s| s.name == first)
            .map(|s| &s.state_vars)
            .into_iter()
            .flatten()
        {
            // The user's initializer, VERBATIM — as Rust/C do. A hardcoded `0` dropped
            // `$.n: int = 21` on the floor.
            let val = py_state_seed(v);
            out.frame(&format!(
                "        self.__compartment.state_vars[\"{}\"] = {val}\n",
                v.name
            ));
        }
        out.frame("        self.__stack = []\n");
        // State/enter params seed the start compartment's args (§203); one `state_args`
        // map in the cleanroom, a distinct `enter_args` deferred.
        for p in sym.params.state.iter().chain(&sym.params.enter) {
            out.frame(&format!(
                "        self.__compartment.state_args[\"{}\"] = {}\n",
                p.name, p.name
            ));
        }
        for f in &sym.domain {
            // `= @@Inner()` is FRAME's instantiation syntax -> the Python constructor. Any
            // other init is the user's native expression, verbatim.
            let init = match &f.init_system {
                Some(s) => format!("{s}()"),
                None => f.init_text.clone().unwrap_or_else(|| "None".into()),
            };
            out.frame(&format!("        self.{} = {init}\n", f.name));
        }
        out.frame("\n");
    }

    fn close_system(&self, _sym: &SystemSym, out: &mut Sink) {
        out.frame("\n");
    }

    fn return_type(&self, _t: Option<&str>) -> String {
        // Python does not declare one. Inventing an annotation would be framec pretending
        // to have a type system.
        String::new()
    }

    /// Python's async is on the `def`, not the return type. There is nothing to wrap.
    fn async_return_type(&self, _t: Option<&str>) -> String {
        String::new()
    }

    fn async_wrap(&self, v: Atom) -> Atom {
        v
    }

    fn return_call(&self, rel: u32, _is_async: bool, expr: NativeText, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}return "));
        out.native(expr);
        out.frame("\n");
    }

    fn self_call(&self, rel: u32, is_async: bool, method: &str, args: &str, out: &mut Sink) {
        let p = self.pad(rel);
        // *** #225. ***
        //
        // `await self.m()` at the head means `.` binds tighter, so a following member
        // access lands on the COROUTINE, not the value. `Atom::awaited` PARENTHESIZES —
        // and it is the only constructor that can produce an `await`, so the bare form
        // is not something this function is careful to avoid; it is something it cannot
        // express.
        let call = Atom::call(format!("self.{method}"), args);
        let e = if is_async {
            Atom::awaited(call, "await")
        } else {
            call
        };
        out.frame(&format!("{p}{e}\n"));
    }

    fn forward(&self, rel: u32, owner: &str, event: &str, params: &str, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self._{owner}_{}({})\n", py_ident(event), param_names(params)));
    }

    /// Python is indent-delimited: a no-op slot (e.g. `=> $^` to a non-handling parent, alone
    /// in an `if x:` block) must be a real `pass`, or the block is a syntax error.
    fn noop(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}pass\n", self.pad(rel)));
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
        let names = param_names(params);
        let sig = if names.is_empty() {
            String::new()
        } else {
            format!(", {names}")
        };
        // Python's async is on the DEF. Nothing to wrap.
        let kw = if is_async { "async def" } else { "def" };
        out.frame(&format!("    {kw} {event}(self{sig}):\n"));
        let mut any = false;
        for (state, owner) in arms {
            let branch = if any { "elif" } else { "if" };
            out.frame(&format!(
                "        {branch} self.__compartment.state == \"{state}\":\n"
            ));
            // Always `return` in Python. A value-returning handler may have NO declared
            // return type (Python is dynamically typed — `getmag() { @@:(expr) }`), so keying
            // the return on `ret.is_some()` dropped its value. Returning is harmless for a void
            // handler (it returns `None`, which the method would return implicitly anyway).
            let _ = ret;
            let aw = if is_async { "await " } else { "" };
            out.frame(&format!(
                "            return {aw}self._{owner}_{}({names})\n",
                py_ident(event)
            ));
            any = true;
        }
        if !any {
            out.frame("        pass\n");
        }
        out.frame("\n");
    }

    fn open_action(&self, name: &str, params: &str, _ret: Option<&str>, out: &mut Sink) {
        let names = param_names(params);
        let sig = if names.is_empty() { String::new() } else { format!(", {names}") };
        out.frame(&format!("    def {name}(self{sig}):\n"));
        out.frame("        compartment = self.__compartment\n");
    }

    fn close_action(&self, out: &mut Sink) {
        out.frame("        return\n\n");
    }

    fn open_handler(
        &self,
        sym: &SystemSym,
        state: &str,
        event: &str,
        params: &str,
        _ret: Option<&str>,
        is_async: bool,
        out: &mut Sink,
    ) {
        let names = param_names(params);
        let sig = if names.is_empty() {
            String::new()
        } else {
            format!(", {names}")
        };
        let kw = if is_async { "async def" } else { "def" };
        out.frame(&format!(
            "    {kw} _{state}_{}(self{sig}):\n",
            py_ident(event)
        ));
        out.frame("        compartment = self.__compartment\n");
        // Bind the state params as locals — the handler body refers to them by name.
        for p in sym
            .states
            .iter()
            .find(|s| s.name == state)
            .map(|s| s.state_params.clone())
            .unwrap_or_default()
        {
            out.frame(&format!("        {p} = compartment.state_args[\"{p}\"]\n"));
        }
    }

    fn close_handler(&self, _ret: Option<&str>, _is_async: bool, _terminated: bool, out: &mut Sink) {
        // Python needs a body. A method that emitted nothing is a SyntaxError, not a
        // no-op — which is exactly the kind of fact that belongs in a spelling and not
        // in a shared driver that does not know what an indent is.
        out.frame("        return\n\n");
    }

    /// **In Python the indent IS the syntax.** A `@@:return` inside an `if x:` must be
    /// indented under it, or the file is a SyntaxError. Nothing else in the compiler
    /// knows that, and nothing else needs to.
    fn pad(&self, rel: u32) -> String {
        format!("        {}", " ".repeat(rel as usize))
    }

    fn native_stmt(&self, rel: u32, text: NativeText, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    fn transition(&self, _rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        self.enter(sym, target, args, out);
        out.frame("        self.__compartment = __next\n");
    }

    fn push(&self, _rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        out.frame("        self.__stack.append(self.__compartment)\n");
        self.enter(sym, target, args, out);
        out.frame("        self.__compartment = __next\n");
    }

    fn pop(&self, _rel: u32, out: &mut Sink) {
        out.frame("        self.__compartment = self.__stack.pop()\n");
    }

    fn lifecycle_call(&self, _rel: u32, _sym: &SystemSym, state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        out.frame(&format!("        self._{state}_{}({})\n", py_ident(event), args.unwrap_or("")));
    }

    fn pop_enter(&self, _rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        let a = enter_args.unwrap_or("");
        for st in &sym.states {
            if super::driver::has_lifecycle(sym, &st.name, "$>") {
                out.frame(&format!(
                    "        if self.__compartment.state == \"{}\":\n            self._{}_{}({a})\n",
                    st.name, st.name, py_ident("$>")
                ));
            }
        }
    }

    fn terminate(&self, _rel: u32, out: &mut Sink) {
        out.frame("        return\n");
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
            RefKind::ContextSelf => {
                let place = Place::field(Place::ident("self"), &lhs.name);
                out.frame(&format!("{p}{place} = "));
                out.native(rhs);
                // Python's statement terminator is the NEWLINE. A `;` here would be legal
                // but not idiomatic — and "what terminates a statement" is a SPELLING, so
                // it lives here and the driver never hears about it.
                out.frame("\n");
            }
            RefKind::StateVar => {
                out.frame(&format!(
                    "        compartment.state_vars[\"{}\"] = ",
                    lhs.name
                ));
                out.native(rhs);
                out.frame("\n");
            }
            RefKind::ContextData => {
                out.frame(&format!(
                    "        compartment.state_args[\"{}\"] = ",
                    lhs.name
                ));
                out.native(rhs);
                out.frame("\n");
            }
            RefKind::ContextReturn => {
                out.frame("        return ");
                out.native(rhs);
                out.frame("\n");
            }
            _ => {
                out.frame(&format!("{p}{} = ", lhs.name));
                out.native(rhs);
                out.frame("\n");
            }
        }
    }

    fn system_ctor_call(&self, name: &str, args: &[String]) -> Atom {
        Atom::call(name, args.join(", "))
    }

    fn embed_call(&self, _sym: &SystemSym, ec: &EmbedCall) -> Atom {
        Atom::method(Atom::field(Atom::ident("self"), &ec.field), &ec.method, &ec.args)
    }

    fn lower_ref(&self, _sym: &SystemSym, _state: &str, r: &FrameRef) -> Atom {
        let comp = Atom::ident("compartment");
        match r.kind {
            // `compartment.state_vars["x"]` — a postfix chain. ALREADY an atom; no
            // parentheses, and none added. Contrast Java, where the same node becomes a
            // cast and MUST be parenthesized. Neither backend knows the rule.
            RefKind::StateVar => Atom::index(
                Atom::field(comp, "state_vars"),
                format!("\"{}\"", r.name),
            ),
            RefKind::ContextData => Atom::index(
                Atom::field(comp, "state_args"),
                format!("\"{}\"", r.name),
            ),
            RefKind::ContextSelf => Atom::field(Atom::ident("self"), &r.name),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::field(comp, "state"),
            RefKind::ContextReturn | RefKind::ContextEvent | RefKind::SelfCall => {
                Atom::ident(&r.name)
            }
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        use super::persist::{TAG, VAL};
        let fields: Vec<&str> = m.fields.iter().map(|(n, _)| n.as_str()).collect();
        let schema = m.schema();

        // ---- snapshot() ----
        out.frame(&format!("    def {}(self):\n", m.save));
        out.frame("        import json\n");
        // The persisted state: the domain fields (minus @@[no_persist]) AND the live
        // control state. RFC-0053 requires BOTH.
        // FULL compartment fidelity (RFC-0056): control state is not the state NAME, it is the
        // whole compartment — state, state_vars, state_args — AND the stack of compartments. A
        // compartment is serialized as a plain dict so `_enc` (below) recurses its state-var
        // VALUES through the same out-of-band envelope that handles domain values (a user-typed
        // state var round-trips exactly like a user-typed domain field).
        out.frame("        def _comp(c):\n");
        out.frame("            return {\"state\": c.state, \"state_vars\": c.state_vars, \"state_args\": c.state_args}\n");
        out.frame("        _state = {\n");
        out.frame("            \"_schema\": ");
        out.frame(&format!("{schema:?},\n"));
        out.frame("            \"_control\": _comp(self.__compartment),\n");
        out.frame("            \"_stack\": [_comp(_c) for _c in self.__stack],\n");
        for f in &fields {
            out.frame(&format!("            {f:?}: self.{f},\n"));
        }
        out.frame("        }\n");
        // *** OUT-OF-BAND FRAMING + SAVE-TIME ESCAPING (this is #233). ***
        //
        // `_enc` recurses through CONTAINERS, not just unknown types. That is the whole
        // fix: the old compiler used a `default=` hook, which never fires for a plain
        // dict, so a user dict carrying the marker was emitted raw and mis-read on
        // restore. Here every value is visited.
        //
        //   * a user-defined instance  -> {TAG: "Point", VAL: {its fields, recursed}}
        //   * a plain dict/list        -> recursed, and its keys are DATA, never a tag
        //
        // The tag lives ONLY in the envelope's TAG slot. A user dict — even one whose
        // keys are exactly TAG/VAL — lands inside VAL and is data. The collision is
        // impossible, not unlikely.
        out.frame("        def _enc(o):\n");
        out.frame("            if isinstance(o, dict):\n");
        // A plain user dict is data. BUT if it happens to contain the reserved tag key,
        // an un-escaped copy would be indistinguishable from an envelope on restore. So
        // a colliding dict is itself wrapped: {TAG: "", VAL: {...}} with an EMPTY type
        // tag, which restore reads as "a plain dict that needed escaping" and unwraps
        // WITHOUT reconstructing. A non-colliding dict is emitted directly (cheap).
        out.frame(&format!("                if {TAG:?} in o:\n"));
        out.frame(&format!("                    return {{{TAG:?}: \"\", {VAL:?}: {{k: _enc(v) for k, v in o.items()}}}}\n"));
        out.frame("                return {k: _enc(v) for k, v in o.items()}\n");
        out.frame("            if isinstance(o, (list, tuple)):\n");
        out.frame("                return [_enc(v) for v in o]\n");
        out.frame("            if isinstance(o, (str, int, float, bool)) or o is None:\n");
        out.frame("                return o\n");
        // A user-defined instance: tag it out-of-band, recurse its fields into VAL.
        out.frame("            _f = dict(getattr(o, \"__dict__\", None) or {})\n");
        out.frame(&format!(
            "            return {{{TAG:?}: type(o).__qualname__, {VAL:?}: {{k: _enc(v) for k, v in _f.items()}}}}\n"
        ));
        out.frame("        return json.dumps(_enc(_state))\n\n");

        // ---- restore() ----
        out.frame(&format!("    def {}(self, data):\n", m.load));
        out.frame("        import json\n");
        out.frame("        _raw = json.loads(data)\n");
        out.frame(&format!("        if _raw.get(\"_schema\") != {schema:?}:\n"));
        out.frame("            raise RuntimeError(\"E751: persist restore refused - snapshot schema does not match this program\")\n");
        // *** CLOSED-WORLD SAFETY FLOOR (non-deferrable). ***
        //
        // Resolve a blob-named type ONLY against types this program defines — never
        // ambient globals or imports. Built from the module's own top-level classes,
        // filtered to those DEFINED here (`__module__ == this module`), so an imported
        // or reopened foreign type is excluded (which is where the old Ruby route
        // leaked). Frame's own scaffolding classes are excluded by name.
        let excl = format!(
            "{{{:?}, {:?}, {:?}, {:?}}}",
            "Compartment",
            format!("{}", "__frame_internal__"),
            "dict",
            "list"
        );
        out.frame(&format!("        _excluded = {excl}\n"));
        out.frame("        import sys as _sys\n");
        out.frame("        _mod = _sys.modules.get(__name__)\n");
        out.frame("        _known = {}\n");
        out.frame("        if _mod is not None:\n");
        out.frame("            for _n, _c in vars(_mod).items():\n");
        out.frame("                if isinstance(_c, type) and getattr(_c, \"__module__\", None) == getattr(_mod, \"__name__\", None) and _c.__qualname__ not in _excluded:\n");
        out.frame("                    _known[_c.__qualname__] = _c\n");
        // The revive walk. It reads a type ONLY from the envelope's TAG slot. A plain
        // dict — including one whose keys look like TAG/VAL — is NOT an envelope unless
        // it has BOTH slots AND the tag resolves to a known type; otherwise it is data.
        out.frame("        def _dec(o):\n");
        out.frame("            if isinstance(o, list):\n");
        out.frame("                return [_dec(v) for v in o]\n");
        out.frame("            if not isinstance(o, dict):\n");
        out.frame("                return o\n");
        out.frame(&format!(
            "            if {TAG:?} in o and {VAL:?} in o and isinstance(o.get({TAG:?}), str):\n"
        ));
        out.frame(&format!("                _t = o[{TAG:?}]\n"));
        // An EMPTY tag = an escaped plain dict (its keys collided with the marker). Unwrap
        // its VAL, do NOT reconstruct. This is the branch that makes the adversarial case
        // — a user dict whose keys are exactly the envelope slots — come back a dict.
        out.frame("                if _t == \"\":\n");
        out.frame(&format!("                    return {{k: _dec(v) for k, v in o[{VAL:?}].items()}}\n"));
        out.frame("                _cls = _known.get(_t)\n");
        out.frame("                if _cls is None:\n");
        out.frame("                    raise RuntimeError(\"E750: persist restore cannot resolve type: \" + repr(_t))\n");
        out.frame("                _obj = _cls.__new__(_cls)\n");
        out.frame(&format!("                for _k, _v in o[{VAL:?}].items():\n"));
        out.frame("                    setattr(_obj, _k, _dec(_v))\n");
        out.frame("                return _obj\n");
        // A plain container with no reserved key: recurse, keys stay data.
        out.frame("            return {k: _dec(v) for k, v in o.items()}\n");
        // Rebuild the full compartment(s) and the stack — allocate fresh Compartments and
        // repopulate state_vars/state_args (decoding each value), rather than reassign a state
        // name onto the constructed compartment (which would leave it holding the START state's
        // vars, and would lose the stack — a `pop$`-after-restore crash).
        out.frame("        def _rebuild(d):\n");
        out.frame("            _c = type(self).Compartment(d[\"state\"])\n");
        out.frame("            _c.state_vars = {_k: _dec(_v) for _k, _v in d.get(\"state_vars\", {}).items()}\n");
        out.frame("            _c.state_args = {_k: _dec(_v) for _k, _v in d.get(\"state_args\", {}).items()}\n");
        out.frame("            return _c\n");
        out.frame("        self.__compartment = _rebuild(_raw[\"_control\"])\n");
        out.frame("        self.__stack = [_rebuild(_d) for _d in _raw.get(\"_stack\", [])]\n");
        for f in &fields {
            out.frame(&format!("        self.{f} = _dec(_raw[{f:?}])\n"));
        }
        out.frame("        return self\n\n");
    }

    /// Python does not care about unreachable code. Java does — it is a compile error
    /// there, and essentially nowhere else. **A `bool` in a table, not a `match` in a
    /// pass.**
    fn dead_code_is_an_error(&self) -> bool {
        false
    }
}

impl Python {
    fn enter(&self, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        out.frame(&format!(
            "        __next = type(self).Compartment(\"{target}\")\n"
        ));
        if let Some(st) = sym.states.iter().find(|s| s.name == target) {
            for v in &st.state_vars {
                let val = py_state_seed(v);
                out.frame(&format!("        __next.state_vars[\"{}\"] = {val}\n", v.name));
            }
            // *** framec does not split the args. *** It hands the blob to a
            // *-varargs helper and lets Python do the splitting — correctly, and for
            // free, including the arity error.
            if let Some(a) = args.map(str::trim).filter(|a| !a.is_empty()) {
                if !st.state_params.is_empty() {
                    let names = st
                        .state_params
                        .iter()
                        .map(|p| format!("\"{p}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.frame(&format!(
                        "        _seed_args(__next, [{names}], {a})\n"
                    ));
                }
            }
        }
    }
}

/// The one module-level helper Python needs. Emitted once per file.
///
/// Named `_seed_args`, with ONE underscore, and that is not a style choice.
///
/// Python **name-mangles** any identifier of the form `__name` that appears inside a
/// class body — `__seed_args` becomes `_Vend__seed_args` at the call site, so the
/// module-level function is invisible and you get a `NameError` at runtime.
///
/// This is precisely the kind of fact that belongs in a **spelling** and not in a shared
/// driver: it is true of Python and of nothing else, and the driver does not know what a
/// class is, let alone what Python does to its names.
pub const PRELUDE: &str = "\
def _seed_args(c, names, *vals):\n\
\x20   for i, n in enumerate(names):\n\
\x20       if i < len(vals):\n\
\x20           c.state_args[n] = vals[i]\n\n\n";


/// Frame's lifecycle event names are not Python identifiers.
/// The seed value for a state var: `= @@Sub()` -> `Sub()` (Frame's instantiation syntax),
/// else the user's init verbatim, else `None`.
fn py_state_seed(v: &crate::resolve::FieldSym) -> String {
    match &v.init_system {
        Some(s) => format!("{s}()"),
        None => v
            .init_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("None")
            .to_string(),
    }
}

fn py_ident(event: &str) -> String {
    match event {
        "$>" => "_enter".to_string(),
        "<$" => "_exit".to_string(),
        other => other.to_string(),
    }
}
