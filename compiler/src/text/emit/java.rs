//! EMIT — Java. **The faithful legacy-4.6.x KERNEL MODEL.**
//!
//! Java's legacy runtime is the *untyped* kernel model — the same shape Python emits, in
//! Java syntax: a per-system `<Sys>FrameEvent` (a `String _message` + an
//! `ArrayList<Object> _parameters`), a `<Sys>FrameContext` (event + `Object _return` +
//! `_data` map + `_transitioned`), a `<Sys>Compartment` (state name + `state_args` /
//! `state_vars` / `enter_args` / `exit_args` + `forward_event` + `parent_compartment`), and
//! the fixed kernel (`__prepareEnter` / `__prepareExit` / `__kernel` drain / `__router` /
//! `__transition`), the `__create` factory, and one public interface wrapper per event.
//!
//! It is NOT the typed compartment (that was an earlier, non-faithful RFC-0056 experiment);
//! the compartment here is `HashMap<String, Object>`-backed, exactly as legacy 4.6.x emits.
//! Rust's typed enums are Rust's OWN legacy model; each target reproduces its own.
//!
//! Everything in this file is a **spelling**. The walking of the tree, the drain-loop
//! sequencing, the handler-key ordering — all of it lives once in [`super::driver`] and the
//! shared walks (EmitInterface / StateDispatchWalk / RouterWalk / HsmChainWalk / EmitHandlers),
//! which do not have the target language and cannot branch on it. This backend only prints.

use super::atom::{Atom, Place};
use super::driver::{param_names, params_split, Backend, BodyRole, LeafCtx};
use super::Sink;
use crate::resolve::{SystemSym, TypeRef};
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

/// Java's return slot on the live context: `_context_stack.get(_context_stack.size() - 1)._return`.
const RETURN_SLOT: &str = "_context_stack.get(_context_stack.size() - 1)._return";
/// Java's `@@:data` scratch slot on the live context.
const DATA_SLOT: &str = "_context_stack.get(_context_stack.size() - 1)._data";

pub struct Java;

impl Java {
    pub fn new() -> Java {
        Java
    }
}

impl Default for Java {
    fn default() -> Self {
        Java
    }
}

impl Backend for Java {
    fn name(&self) -> &'static str {
        "java"
    }

    /// Java has class-level visibility: `@@system private Name` -> package-private `class`.
    fn supports_class_visibility(&self) -> bool {
        true
    }

    /// The KERNEL model uses fully-qualified `java.util.*` names throughout, so there is NO
    /// file-level preamble — legacy 4.6.x emits none. A foundation file starts at its first
    /// item (the leading water, then the classes).
    fn file_header(&self, _out: &mut Sink) {}

    /// Java: `int amount`. Type first, name second. The TYPE is the user's text, verbatim.
    fn param_list(&self, params_text: &str) -> String {
        params_split(params_text)
            .into_iter()
            .map(|(n, t)| match t {
                // VERBATIM: the type is the user's target-language text. framec reorders
                // `name: type` -> `type name`; it does NOT translate the type token.
                Some(t) => format!("{t} {n}"),
                None => format!("Object {n}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");

        // ---- Layer A: the three system-prefixed runtime classes (fixed text modulo `n`). ----
        out.frame(&format!(
            "class {n}FrameEvent {{\n\
             \x20   String _message;\n\
             \x20   java.util.ArrayList<Object> _parameters;\n\n\
             \x20   {n}FrameEvent(String message) {{\n\
             \x20       this._message = message;\n\
             \x20       this._parameters = new java.util.ArrayList<>();\n\
             \x20   }}\n\n\
             \x20   {n}FrameEvent(String message, java.util.ArrayList<Object> parameters) {{\n\
             \x20       this._message = message;\n\
             \x20       this._parameters = parameters;\n\
             \x20   }}\n\
             }}\n\n"
        ));
        out.frame(&format!(
            "class {n}FrameContext {{\n\
             \x20   {n}FrameEvent _event;\n\
             \x20   Object _return;\n\
             \x20   java.util.HashMap<String, Object> _data;\n\
             \x20   boolean _transitioned = false;\n\n\
             \x20   {n}FrameContext({n}FrameEvent event, Object defaultReturn) {{\n\
             \x20       this._event = event;\n\
             \x20       this._return = defaultReturn;\n\
             \x20       this._data = new java.util.HashMap<>();\n\
             \x20       this._transitioned = false;\n\
             \x20   }}\n\
             }}\n\n"
        ));
        out.frame(&format!(
            "class {n}Compartment {{\n\
             \x20   String state;\n\
             \x20   java.util.ArrayList<Object> state_args;\n\
             \x20   java.util.HashMap<String, Object> state_vars;\n\
             \x20   java.util.ArrayList<Object> enter_args;\n\
             \x20   java.util.ArrayList<Object> exit_args;\n\
             \x20   {n}FrameEvent forward_event;\n\
             \x20   {n}Compartment parent_compartment;\n\n\
             \x20   {n}Compartment(String state) {{\n\
             \x20       this.state = state;\n\
             \x20       this.state_args = new java.util.ArrayList<>();\n\
             \x20       this.state_vars = new java.util.HashMap<>();\n\
             \x20       this.enter_args = new java.util.ArrayList<>();\n\
             \x20       this.exit_args = new java.util.ArrayList<>();\n\
             \x20       this.forward_event = null;\n\
             \x20       this.parent_compartment = null;\n\
             \x20   }}\n\n\
             \x20   {n}Compartment copy() {{\n\
             \x20       {n}Compartment c = new {n}Compartment(this.state);\n\
             \x20       c.state_args = new java.util.ArrayList<>(this.state_args);\n\
             \x20       c.state_vars = new java.util.HashMap<>(this.state_vars);\n\
             \x20       c.enter_args = new java.util.ArrayList<>(this.enter_args);\n\
             \x20       c.exit_args = new java.util.ArrayList<>(this.exit_args);\n\
             \x20       c.forward_event = this.forward_event;\n\
             \x20       c.parent_compartment = this.parent_compartment;\n\
             \x20       return c;\n\
             \x20   }}\n\
             }}\n\n"
        ));

        // ---- Layer B: the system class, fields, constructor, factory, kernel scaffolding. ----
        let vis = if sym.private { "" } else { "public " };
        out.frame(&format!("{vis}class {n} {{\n"));
        out.frame(&format!("    private java.util.ArrayList<{n}Compartment> _state_stack;\n"));
        out.frame(&format!("    private {n}Compartment __compartment;\n"));
        out.frame(&format!("    private {n}Compartment __next_compartment;\n"));
        out.frame(&format!("    private java.util.ArrayList<{n}FrameContext> _context_stack;\n"));
        // Domain field DECLARATIONS. Inline init unless the init references a construction
        // (ctor) param — which is not in scope at a field initializer, so it is deferred to
        // `__create`. The type is the user's text, verbatim.
        for f in &sym.domain {
            let ty = java_field_ty(&f.ty);
            match domain_field_inline_init(sym, f) {
                Some(init) => out.frame(&format!("    public {ty} {} = {init};\n", f.name)),
                None => out.frame(&format!("    public {ty} {};\n", f.name)),
            }
        }
        out.frame("\n");

        // Constructor — header params (state, enter, domain), §203.
        let plist = self.param_list(&super::driver::ctor_params_text(&sym.params));
        let ctor_args = param_names(&super::driver::ctor_params_text(&sym.params));
        let state_seed = java_list_literal(&sym.params.state.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
        let enter_seed = java_list_literal(&sym.params.enter.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
        out.frame(&format!("    public {n}({plist}) {{\n"));
        out.frame("        _state_stack = new java.util.ArrayList<>();\n");
        out.frame("        _context_stack = new java.util.ArrayList<>();\n");
        if sym.states.is_empty() {
            // Machineless system: no start state, no compartment to enter.
            out.frame("        this.__next_compartment = null;\n");
        } else {
            out.frame(&format!(
                "        this.__compartment = __prepareEnter(\"{first}\", {state_seed}, {enter_seed});\n"
            ));
            // *** DIVERGENCE-JOURNAL.md#D3 — state vars are CONSTRUCTION-SEEDED. ***
            // Legacy seeds a state's `$.` vars in a SYNTHESIZED `$>` handler
            // (`if (!containsKey) put(...)`), which only runs via the `__create` factory — a
            // plain `new Sys()` leaves them unset (KeyError/NPE). ng seeds them where the
            // compartment is BUILT — here at construction, and in `transition` — so a plain
            // constructor yields a usable machine. Same (name, value); different SITE. The
            // start state's vars are in scope here (the ctor's own compartment).
            out.frame(&java_state_var_seeds(sym, first, "        ", "this.__compartment"));
            out.frame("        this.__next_compartment = null;\n");
        }
        out.frame("    }\n\n");

        // The factory: construct, seed param-dependent domain fields, run the start `$>`.
        out.frame(&format!("    public static {n} __create({plist}) {{\n"));
        out.frame(&format!("        {n} c = new {n}({ctor_args});\n"));
        for f in &sym.domain {
            if domain_field_inline_init(sym, f).is_none() {
                if let Some(init) = domain_field_deferred_init(f) {
                    out.frame(&format!("        c.{} = {init};\n", f.name));
                }
            }
        }
        if !sym.states.is_empty() {
            out.frame(&format!(
                "        {n}FrameEvent __e = new {n}FrameEvent(\"$>\", c.__compartment.enter_args);\n"
            ));
            out.frame(&format!("        {n}FrameContext __ctx = new {n}FrameContext(__e, null);\n"));
            out.frame("        c._context_stack.add(__ctx);\n");
            out.frame("        c.__kernel(__e);\n");
            out.frame("        c._context_stack.remove(c._context_stack.size() - 1);\n");
        }
        out.frame("        return c;\n");
        out.frame("    }\n\n");

        if sym.states.is_empty() {
            // No control state: no kernel. The impl is left OPEN for actions.
            return;
        }

        // `hsm_chain()` — the rows are the HsmChainWalk machine's; the map scaffolding is the leaf.
        out.frame("    private java.util.HashMap<String, java.util.ArrayList<String>> hsm_chain() {\n");
        out.frame("        java.util.HashMap<String, java.util.ArrayList<String>> m = new java.util.HashMap<>();\n");
        out.frame(&super::hsm_chain_walk::walk(sym, self));
        out.frame("        return m;\n");
        out.frame("    }\n");

        out.frame(&format!(
            "    private {n}Compartment __prepareEnter(String leaf, java.util.ArrayList<Object> state_args, java.util.ArrayList<Object> enter_args) {{\n\
             \x20       {n}Compartment comp = null;\n\
             \x20       for (String name : hsm_chain().get(leaf)) {{\n\
             \x20           {n}Compartment new_comp = new {n}Compartment(name);\n\
             \x20           new_comp.state_args = new java.util.ArrayList<>(state_args);\n\
             \x20           new_comp.enter_args = new java.util.ArrayList<>(enter_args);\n\
             \x20           new_comp.parent_compartment = comp;\n\
             \x20           comp = new_comp;\n\
             \x20       }}\n\
             \x20       return comp;\n\
             \x20   }}\n\n"
        ));
        out.frame(&format!(
            "    private void __prepareExit(java.util.ArrayList<Object> exit_args) {{\n\
             \x20       {n}Compartment comp = __compartment;\n\
             \x20       while (comp != null) {{\n\
             \x20           comp.exit_args = new java.util.ArrayList<>(exit_args);\n\
             \x20           comp = comp.parent_compartment;\n\
             \x20       }}\n\
             \x20   }}\n\n"
        ));
        out.frame(&format!(
            "    private void __kernel({n}FrameEvent __e) {{\n\
             \x20       // Route event to current state.\n\
             \x20       __router(__e);\n\
             \x20       // Drain any transitions queued by the handler.\n\
             \x20       while (__next_compartment != null) {{\n\
             \x20           {n}Compartment next_compartment = __next_compartment;\n\
             \x20           __next_compartment = null;\n\
             \x20           // Exit the current (leaf) state.\n\
             \x20           {n}FrameEvent exit_event = new {n}FrameEvent(\"<$\", __compartment.exit_args);\n\
             \x20           __router(exit_event);\n\
             \x20           // Switch to the new compartment.\n\
             \x20           __compartment = next_compartment;\n\
             \x20           // Three-branch forward-event handling.\n\
             \x20           {n}FrameEvent forward_event = next_compartment.forward_event;\n\
             \x20           next_compartment.forward_event = null;\n\
             \x20           if (forward_event == null) {{\n\
             \x20               // No forwarded event \u{2014} synthesize a fresh $>.\n\
             \x20               {n}FrameEvent enter_event = new {n}FrameEvent(\"$>\", __compartment.enter_args);\n\
             \x20               __router(enter_event);\n\
             \x20           }} else if (forward_event._message.equals(\"$>\")) {{\n\
             \x20               // Forwarded event IS $> \u{2014} dispatch directly so the\n\
             \x20               // destination's $> handler receives the caller's payload.\n\
             \x20               __router(forward_event);\n\
             \x20           }} else {{\n\
             \x20               // Forwarded event is not $> \u{2014} initialize the destination\n\
             \x20               // with a fresh $>, then dispatch the forward.\n\
             \x20               {n}FrameEvent enter_event = new {n}FrameEvent(\"$>\", __compartment.enter_args);\n\
             \x20               __router(enter_event);\n\
             \x20               __router(forward_event);\n\
             \x20           }}\n\
             \x20           for ({n}FrameContext ctx : _context_stack) {{\n\
             \x20               ctx._transitioned = true;\n\
             \x20           }}\n\
             \x20       }}\n\
             \x20   }}\n\n"
        ));

        // `__router` — the ARMS are the RouterWalk machine's (each carries the `first` bit for
        // `if` vs `else if`). The method scaffold and the closing `\n` are the leaf.
        out.frame(&format!("    private void __router({n}FrameEvent __e) {{\n"));
        out.frame(&super::router_walk::walk(sym, self));
        out.frame("\n    }\n\n");

        out.frame(&format!(
            "    private void __transition({n}Compartment next) {{\n\
             \x20       __next_compartment = next;\n\
             \x20   }}\n"
        ));
        // impl left OPEN — interface/dispatch/handler/action walks append; close_system closes.
    }

    /// One `hsm_chain` row: `m.put("Leaf", new ArrayList<>(Arrays.asList("Root", …, "Leaf")));`.
    fn hsm_chain_entry(&self, leaf: &str, chain: &[String], out: &mut Sink) {
        let list = chain.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ");
        out.frame(&format!(
            "        m.put(\"{leaf}\", new java.util.ArrayList<>(java.util.Arrays.asList({list})));\n"
        ));
    }

    /// One `__router` arm. `first` decides `if` vs `else if` — chained on one line (`} else if`),
    /// so the arm carries no trailing newline; `open_system` closes the last line.
    fn router_arm(&self, _sym: &SystemSym, state: &str, first: bool, out: &mut Sink) {
        let lead = if first { "        if" } else { " else if" };
        out.frame(&format!(
            "{lead} (__compartment.state.equals(\"{state}\")) {{\n\
             \x20           _state_{state}(__e, __compartment);\n\
             \x20       }}"
        ));
    }

    /// One state's message dispatcher — `_state_<S>`, the method `__router` hands an event to.
    fn dispatch(&self, sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
        // The per-state dispatcher BODY is the SHARED `DispatchBody` @@system
        // (`super::dispatch_body`), spelled through the four `dispatch_*` seam methods below. The
        // byte-for-byte pre-conversion body is preserved as [`java_dispatch_hand`] and gated in
        // `tests/emit_scaffold_walks.rs`.
        super::dispatch_body::drive(self, sym, state, arms, out);
    }

    fn dispatch_open(&self, sym: &SystemSym, state: &str, out: &mut Sink) {
        let n = &sym.name;
        out.frame(&format!(
            "\n    private void _state_{state}({n}FrameEvent __e, {n}Compartment compartment) {{\n"
        ));
    }

    fn dispatch_param(&self, sym: &SystemSym, state: &str, pi: usize, out: &mut Sink) {
        // The state's own PARAMS are bound from the live compartment's `state_args`, unboxed to the
        // declared type.
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            if let Some(p) = st.state_params.get(pi) {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "Object".into());
                let slot = java_unbox(
                    &ty,
                    Atom::method(
                        Atom::field(Atom::ident("__compartment"), "state_args"),
                        "get",
                        &pi.to_string(),
                    ),
                );
                out.frame(&format!("        {ty} {p} = {slot};\n"));
            }
        }
    }

    fn dispatch_arm(&self, _sym: &SystemSym, state: &str, arms: &[String], ai: usize, out: &mut Sink) {
        if let Some(msg) = arms.get(ai) {
            let method = kernel_handler_method(state, msg);
            out.frame(&format!(
                "        if (__e._message.equals(\"{msg}\")) {{\n\
                 \x20           this.{method}(__e, compartment);\n\
                 \x20           return;\n\
                 \x20       }}\n"
            ));
        }
    }

    fn dispatch_close(&self, _sym: &SystemSym, _state: &str, _arms: &[String], _np: usize, out: &mut Sink) {
        out.frame("    }\n");
    }

    fn route(
        &self,
        sym: &SystemSym,
        event: &str,
        params: &str,
        ret: Option<&str>,
        is_async: bool,
        _arms: &[(String, String)],
        out: &mut Sink,
    ) {
        let n = &sym.name;
        let rt = if is_async { self.async_return_type(ret) } else { self.return_type(ret) };
        let names = param_names(params);
        let payload = java_list_literal(&split_names(&names));
        out.frame(&format!("\n    public {rt} {event}({}) {{\n", self.param_list(params)));
        out.frame(&format!("        {n}FrameEvent __e = new {n}FrameEvent(\"{event}\", {payload});\n"));
        out.frame(&format!("        {n}FrameContext __ctx = new {n}FrameContext(__e, null);\n"));
        out.frame("        _context_stack.add(__ctx);\n");
        out.frame("        try {\n");
        out.frame("            __kernel(_context_stack.get(_context_stack.size() - 1)._event);\n");
        match ret.map(str::trim).filter(|t| !t.is_empty()) {
            Some(rty) => {
                // A value method reads the return slot back after the kernel drains.
                out.frame(&format!(
                    "            {rty} __result = ({rty}) _context_stack.get(_context_stack.size() - 1)._return;\n"
                ));
                out.frame("            _context_stack.remove(_context_stack.size() - 1);\n");
                out.frame("            return __result;\n");
            }
            None => {
                out.frame("            _context_stack.remove(_context_stack.size() - 1);\n");
            }
        }
        out.frame("        } catch (RuntimeException __frame_err) {\n");
        out.frame("            _context_stack.remove(_context_stack.size() - 1);\n");
        out.frame("            throw __frame_err;\n");
        out.frame("        }\n");
        out.frame("    }\n");
    }

    /// The handler-method opener is the SHARED `HandlerOpen` @@system (`super::handler_open`), spelled
    /// through the four `handler_*` seam methods below. `_ret`/`_is_async` are unused (the handler is
    /// VOID and parks its value in the context slot). The byte-for-byte pre-conversion body is
    /// preserved as [`java_open_handler_hand`] and gated in `tests/emit_scaffold_walks.rs`.
    fn open_handler(
        &self,
        sym: &SystemSym,
        state: &str,
        event: &str,
        params: &str,
        _ret: Option<&str>,
        _is_async: bool,
        out: &mut Sink,
    ) {
        super::handler_open::drive(self, sym, state, event, params, out);
    }

    fn handler_open(&self, sym: &SystemSym, state: &str, event: &str, _params: &str, out: &mut Sink) {
        let n = &sym.name;
        let method = kernel_handler_method(state, event);
        out.frame(&format!(
            "\n    private void {method}({n}FrameEvent __e, {n}Compartment compartment) {{\n"
        ));
    }

    fn handler_state_param(&self, sym: &SystemSym, state: &str, si: usize, out: &mut Sink) {
        // The state's own param, bound from the (parameter) compartment's `state_args`, unboxed.
        if let Some(st) = sym.states.iter().find(|s| s.name == state) {
            if let Some(p) = st.state_params.get(si) {
                let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "Object".into());
                let slot = java_unbox(&ty, Atom::method(Atom::field(Atom::ident("compartment"), "state_args"), "get", &si.to_string()));
                out.frame(&format!("        {ty} {p} = {slot};\n"));
            }
        }
    }

    fn handler_event_param(&self, _sym: &SystemSym, _state: &str, event: &str, params: &str, ei: usize, out: &mut Sink) {
        // Lifecycle events read enter_args/exit_args off the compartment; a user event reads
        // `__e._parameters`. Each is unboxed to its declared type.
        let recv = match event {
            "$>" => "compartment.enter_args",
            "<$" => "compartment.exit_args",
            _ => "__e._parameters",
        };
        if let Some((name, ty)) = params_split(params)
            .into_iter()
            .filter(|(nm, _)| !nm.is_empty())
            .nth(ei)
        {
            let ty = ty.unwrap_or_else(|| "Object".into());
            let slot = java_unbox(&ty, Atom::method(Atom::ident(recv), "get", &ei.to_string()));
            out.frame(&format!("        {ty} {name} = {slot};\n"));
        }
    }

    fn close_handler(&self, _ret: Option<&str>, _is_async: bool, _terminated: bool, _ctx: &LeafCtx, out: &mut Sink) {
        // The handler is VOID (it parks its value in the context slot), so there is no fallback
        // return to add. The separator before the NEXT member is that member's own leading `\n`.
        out.frame("    }\n");
    }

    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, _is_operation: bool, out: &mut Sink) {
        // An action is a PLAIN method — no compartment preamble, no dispatch. Leading `\n` is the
        // separator before it.
        out.frame(&format!(
            "\n    private {} {name}({}) {{\n",
            self.return_type(ret),
            self.param_list(params)
        ));
    }

    fn close_action(&self, out: &mut Sink) {
        out.frame("    }\n");
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
            Some(t) => format!("CompletableFuture<{}>", java_box_name(t)),
            None => "CompletableFuture<Void>".to_string(),
        }
    }

    /// Java: braces carry the nesting, so indent is cosmetic; reproduce the user's relative
    /// nesting for readability. Base is the 8-space handler-body column.
    fn pad(&self, rel: u32) -> String {
        format!("        {}", " ".repeat(rel as usize))
    }

    fn native_stmt(&self, rel: u32, text: NativeText, _ctx: &LeafCtx, out: &mut Sink) {
        // framec terminates only what IT emits; a native statement carries the user's own
        // terminator, untouched.
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        self.transition_with_enter(rel, sym, target, args, None, out);
    }

    /// `-> (enter_args) $Target(state_args)` — the enter-arg-aware transition.
    ///
    /// `__prepareEnter(leaf, state_args, enter_args)` stamps BOTH payloads onto the freshly built
    /// destination compartment; the kernel drain later synthesizes `$>` from `enter_args`, so the
    /// enter payload must ride the THIRD slot here. Before this, Java took the trait default (which
    /// drops `enter_args`) and hardcoded that slot empty, so `-> ("hi") $B` reached B's `$>` with an
    /// empty `enter_args` and the handler's `get(0)` threw IndexOutOfBounds. Each arg blob is
    /// unsplit — `Arrays.asList(<blob>)` and javac splits.
    fn transition_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        let n = &sym.name;
        let p = self.pad(rel);
        out.frame(&format!(
            "{p}{n}Compartment __compartment = __prepareEnter(\"{target}\", {}, {});\n",
            java_args_literal(args),
            java_args_literal(enter_args)
        ));
        // D3: seed the DESTINATION state's `$.` vars at the build site (see the constructor's
        // note). The local `__compartment` is the freshly built destination.
        out.frame(&java_state_var_seeds(sym, target, p.as_str(), "__compartment"));
        out.frame(&format!("{p}__transition(__compartment);\n"));
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        self.push_with_enter(rel, sym, target, args, None, out);
    }

    /// `push$ -> (enter_args) $Target(state_args)` — remember the live compartment, then transition
    /// carrying the enter payload. The `_state_stack.add(__compartment)` reads the live FIELD (the
    /// local `__compartment` that `transition_with_enter` declares is scoped after it).
    fn push_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        out.frame(&format!("{}_state_stack.add(__compartment);\n", self.pad(rel)));
        self.transition_with_enter(rel, sym, target, args, enter_args, out);
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}var __saved = _state_stack.remove(_state_stack.size() - 1);\n"));
        out.frame(&format!("{p}__transition(__saved);\n"));
    }

    fn push_bare(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}_state_stack.add(__compartment);\n", self.pad(rel)));
    }

    fn pop_bare(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}_state_stack.remove(_state_stack.size() - 1);\n", self.pad(rel)));
    }

    fn lifecycle_call(&self, rel: u32, _sym: &SystemSym, _state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        // `$>`/`<$` are synthesized by the kernel drain, not called from a handler. The one thing
        // a handler stamps is the EXIT payload, delivered down the chain by `__prepareExit`.
        if event != "<$" {
            return;
        }
        let Some(_) = args else { return };
        out.frame(&format!("{}__prepareExit({});\n", self.pad(rel), java_args_literal(args)));
    }

    fn pop_enter(&self, rel: u32, _sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        out.frame(&format!(
            "{}__saved.enter_args = {};\n",
            self.pad(rel),
            java_args_literal(enter_args)
        ));
    }

    fn terminate(&self, rel: u32, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&format!("{}return;\n", self.pad(rel)));
    }

    fn return_call(&self, role: BodyRole, rel: u32, is_async: bool, _multiline: bool, expr: NativeText, _ctx: &LeafCtx, out: &mut Sink) {
        let p = self.pad(rel);
        // An action is a plain method with no live context — a real `return`. A kernel handler
        // parks its value in the context slot (VOID method) and RUNS ON.
        if role == BodyRole::Action {
            out.frame(&format!("{p}return "));
            out.native(expr);
            out.frame(";\n");
            return;
        }
        if is_async {
            out.frame(&format!("{p}{RETURN_SLOT} = CompletableFuture.completedFuture("));
            out.native(expr);
            out.frame(");\n");
        } else {
            out.frame(&format!("{p}{RETURN_SLOT} = "));
            out.native(expr);
            out.frame(";\n");
        }
    }

    fn self_call(&self, rel: u32, is_async: bool, method: &str, args: &str, out: &mut Sink) {
        let p = self.pad(rel);
        let call = Atom::call(format!("this.{method}"), args);
        // Java async is a `CompletableFuture`, so a reentrant call is `.join()`ed. Atom-built, so
        // it can never bind to the wrong thing (#225).
        let e = if is_async { Atom::method(call, "join", "") } else { call };
        out.frame(&format!("{p}{e};\n"));
    }

    fn forward(&self, rel: u32, owner: &str, _event: &str, _params: &str, out: &mut Sink) {
        // `=> $^` — hand the event to the PARENT state's dispatcher, compartment shifted up one.
        out.frame(&format!(
            "{}_state_{owner}(__e, compartment.parent_compartment);\n",
            self.pad(rel)
        ));
    }

    /// `=> $^` goes to the DECLARED parent — [`Self::forward`] shifts the compartment by exactly
    /// one level and calls the parent's dispatcher, which must match. Java now HAS a dispatcher
    /// layer (`_state_<S>`), so this can be enabled (the void condition in the driver default).
    fn forward_to_declared_parent(&self) -> bool {
        true
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
            // A domain field IS an lvalue: `this.total = rhs;`. A Place, never parenthesized.
            // framec terminates what framec emits — unconditionally (the oracle dropped this `;`
            // between adjacent assignments, #173).
            RefKind::ContextSelf => {
                let place = Place::field(Place::ident("this"), &lhs.name);
                out.frame(&format!("{p}{place} = "));
                out.native(rhs);
                out.frame(";\n");
            }
            // A state var lives in the current compartment's `state_vars` map.
            RefKind::StateVar => {
                out.frame(&format!("{p}compartment.state_vars.put(\"{}\", ", lhs.name));
                out.native(rhs);
                out.frame(");\n");
            }
            // `@@:data.k` is the EVENT's scratch map on the live context.
            RefKind::ContextData => {
                out.frame(&format!("{p}{DATA_SLOT}.put(\"{}\", ", lhs.name));
                out.native(rhs);
                out.frame(");\n");
            }
            // `@@:return = e` — set the return slot; do NOT exit (the handler runs on).
            RefKind::ContextReturn => {
                out.frame(&format!("{p}{RETURN_SLOT} = "));
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
        // `@@Sys(...)` lowers to the KERNEL factory `Sys.__create(...)` — the two-phase
        // constructor that runs the start state's `$>` drain.
        Atom::call(format!("{name}.__create"), args.join(", "))
    }

    fn embed_call(&self, _sym: &SystemSym, ec: &EmbedCall) -> Atom {
        // An EMPTY field is a bare self-call `@@:self.method(...)` (bug R3): receiver is `this`.
        if ec.field.is_empty() {
            return Atom::call(format!("this.{}", ec.method), &ec.args);
        }
        Atom::method(Atom::field(Atom::ident("this"), &ec.field), &ec.method, &ec.args)
    }

    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom {
        let comp = Atom::ident("compartment");
        match r.kind {
            // A state var: read out of the compartment's `state_vars` map, cast to the declared
            // type. `Atom::cast` PARENTHESIZES (#213: a bare `(T) m.get("x").y` would misbind).
            RefKind::StateVar => {
                let ty = sym
                    .states
                    .iter()
                    .find(|s| s.name == state)
                    .and_then(|s| s.state_vars.iter().find(|v| v.name == r.name))
                    .map(|v| java_field_ty(&v.ty))
                    .unwrap_or_else(|| "Object".into());
                Atom::cast(ty, Atom::method(Atom::field(comp, "state_vars"), "get", &format!("\"{}\"", r.name)))
            }
            RefKind::ContextData => {
                Atom::method(Atom::ident(DATA_SLOT), "get", &format!("\"{}\"", r.name))
            }
            RefKind::ContextSelf => Atom::field(Atom::ident("this"), &r.name),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::field(comp, "state"),
            RefKind::ContextReturn => Atom::ident(RETURN_SLOT),
            RefKind::ContextEvent | RefKind::SelfCall | RefKind::Unknown => Atom::ident(&r.name),
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        // NOTE: kernel-model persist (untyped compartment) is not yet ported; this emits a
        // schema-guarded stub so the trait is satisfied and non-persist systems are unaffected.
        // The foundation anchor does not persist. TODO(kernel persist) — see M-persist.
        let schema = m.schema();
        out.frame(&format!("\n    public String {}() {{\n", m.save));
        out.frame(&format!("        return \"{schema}\";\n"));
        out.frame("    }\n");
        out.frame(&format!("\n    public void {}(String data) {{\n", m.load));
        out.frame(&format!("        if (!data.equals(\"{schema}\")) {{\n"));
        out.frame("            throw new RuntimeException(\"E751: persist restore refused - snapshot schema does not match this program\");\n");
        out.frame("        }\n");
        out.frame("    }\n");
    }
}

/// The byte-for-byte **frozen oracle** for Java's per-state dispatcher — a verbatim copy of the
/// pre-conversion `Backend::dispatch` body, before it was reified as the shared
/// [`super::dispatch_body`] `DispatchBody` `@@system`. Kept as the GATE-A differential the machine is
/// proven against (`tests/emit_scaffold_walks.rs`). It does NOT route through `be.dispatch` — it
/// reproduces the original bytes standalone, so a spelling bug in a `dispatch_*` leaf is visible to
/// the gate. Doc-hidden and **not on the production path**.
#[doc(hidden)]
pub(super) fn java_dispatch_hand(sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
    let n = &sym.name;
    out.frame(&format!(
        "\n    private void _state_{state}({n}FrameEvent __e, {n}Compartment compartment) {{\n"
    ));
    if let Some(st) = sym.states.iter().find(|s| s.name == state) {
        for (i, p) in st.state_params.iter().enumerate() {
            let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "Object".into());
            let slot = java_unbox(&ty, Atom::method(Atom::field(Atom::ident("__compartment"), "state_args"), "get", &i.to_string()));
            out.frame(&format!("        {ty} {p} = {slot};\n"));
        }
    }
    for msg in arms {
        let method = kernel_handler_method(state, msg);
        out.frame(&format!(
            "        if (__e._message.equals(\"{msg}\")) {{\n\
             \x20           this.{method}(__e, compartment);\n\
             \x20           return;\n\
             \x20       }}\n"
        ));
    }
    out.frame("    }\n");
}

/// The byte-for-byte **frozen oracle** for Java's handler-opener — a verbatim copy of the
/// pre-conversion `Backend::open_handler` body, before it was reified as the shared
/// [`super::handler_open`] `HandlerOpen` `@@system`. Kept as the GATE-A differential the machine is
/// proven against (`tests/emit_scaffold_walks.rs`). It does NOT route through `be.open_handler` — it
/// reproduces the original bytes standalone, so a spelling bug in a `handler_*` leaf is visible to
/// the gate. Doc-hidden and **not on the production path**.
#[doc(hidden)]
pub(super) fn java_open_handler_hand(sym: &SystemSym, state: &str, event: &str, params: &str, _is_async: bool, out: &mut Sink) {
    let n = &sym.name;
    let method = kernel_handler_method(state, event);
    out.frame(&format!(
        "\n    private void {method}({n}FrameEvent __e, {n}Compartment compartment) {{\n"
    ));
    if let Some(st) = sym.states.iter().find(|s| s.name == state) {
        for (i, p) in st.state_params.iter().enumerate() {
            let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "Object".into());
            let slot = java_unbox(&ty, Atom::method(Atom::field(Atom::ident("compartment"), "state_args"), "get", &i.to_string()));
            out.frame(&format!("        {ty} {p} = {slot};\n"));
        }
    }
    let recv = match event {
        "$>" => "compartment.enter_args",
        "<$" => "compartment.exit_args",
        _ => "__e._parameters",
    };
    for (i, (name, ty)) in params_split(params)
        .into_iter()
        .filter(|(nm, _)| !nm.is_empty())
        .enumerate()
    {
        let ty = ty.unwrap_or_else(|| "Object".into());
        let slot = java_unbox(&ty, Atom::method(Atom::ident(recv), "get", &i.to_string()));
        out.frame(&format!("        {ty} {name} = {slot};\n"));
    }
}

// ======================================================================================
// Helpers — pure spellings, no walking.
// ======================================================================================

/// The private handler method name for `(state, event)`, kernel-model spelling.
fn kernel_handler_method(state: &str, event: &str) -> String {
    match event {
        "$>" => format!("_s_{state}_hdl_frame_enter"),
        "<$" => format!("_s_{state}_hdl_frame_exit"),
        other => format!("_s_{state}_hdl_user_{other}"),
    }
}

/// A comma-joined name list -> the Java list literal legacy uses for an event payload / seed:
/// `new java.util.ArrayList<>()` when empty, else `new ...ArrayList<>(java.util.Arrays.asList(a, b))`.
fn java_list_literal(names: &[String]) -> String {
    if names.is_empty() {
        "new java.util.ArrayList<>()".to_string()
    } else {
        format!(
            "new java.util.ArrayList<>(java.util.Arrays.asList({}))",
            names.join(", ")
        )
    }
}

/// A transition/lifecycle ARG BLOB -> the Java list literal. framec does NOT split the blob; it
/// hands the whole thing to `Arrays.asList(<blob>)` and javac splits it.
fn java_args_literal(args: Option<&str>) -> String {
    match args.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => format!("new java.util.ArrayList<>(java.util.Arrays.asList({a}))"),
        None => "new java.util.ArrayList<>()".to_string(),
    }
}

/// Split a comma-joined `param_names` string into individual names (dropping empties).
fn split_names(names: &str) -> Vec<String> {
    names
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// A field/type text for a `TypeRef`, emitted VERBATIM (Frame has no type system).
fn java_field_ty(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Opaque(t) => t.clone(),
        TypeRef::System(s) => s.clone(),
        TypeRef::WrappedSystem { text, .. } => text.clone(),
        TypeRef::None => "Object".to_string(),
    }
}

/// D3 construction-seed lines for `state`'s `$.` vars, written onto compartment `recv` at indent
/// `pad`: `{pad}{recv}.state_vars.put("<name>", <seed>);\n` for each var, in declaration order.
/// Empty for a state with no vars — so a var-less state (the foundation anchor's `$A`) is
/// byte-unchanged and the seeding is invisible until a fixture declares `$.`.
fn java_state_var_seeds(sym: &SystemSym, state: &str, pad: &str, recv: &str) -> String {
    let Some(st) = sym.states.iter().find(|s| s.name == state) else {
        return String::new();
    };
    st.state_vars
        .iter()
        .map(|v| format!("{pad}{recv}.state_vars.put(\"{}\", {});\n", v.name, java_state_seed(v)))
        .collect()
}

/// The seed value for a state var: `= @@Sub()` -> the kernel factory `Sub.__create(...)`, else
/// the user's init verbatim, else `null`. Shared by the constructor and each transition so the
/// two agree.
fn java_state_seed(v: &crate::resolve::FieldSym) -> String {
    match &v.init_system {
        Some(s) => format!("{s}.__create({})", super::ctor_init_args(v.init_text.as_deref())),
        None => v
            .init_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("null")
            .to_string(),
    }
}

/// The INLINE field-declaration initializer for a domain field, or `None` if it must be deferred
/// to `__create` (its init references a construction param, out of scope at a field initializer).
/// `= @@Sub()` is Frame's instantiation syntax -> the kernel factory.
fn domain_field_inline_init(sym: &SystemSym, f: &crate::resolve::FieldSym) -> Option<String> {
    if let Some(s) = &f.init_system {
        // A system construction never references a bare ctor param name here; keep it inline.
        return Some(format!("{s}.__create({})", super::ctor_init_args(f.init_text.as_deref())));
    }
    let init = f.init_text.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match init {
        None => Some("null".to_string()),
        Some(t) if init_references_ctor_param(sym, t) => None,
        Some(t) => Some(t.to_string()),
    }
}

/// The DEFERRED (`__create`) initializer for a domain field whose inline init was refused.
fn domain_field_deferred_init(f: &crate::resolve::FieldSym) -> Option<String> {
    if let Some(s) = &f.init_system {
        return Some(format!("{s}.__create({})", super::ctor_init_args(f.init_text.as_deref())));
    }
    f.init_text.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Does an init expression reference a construction (state/enter/domain) param by name? A purely
/// lexical word-boundary scan over the user's text — framec never parses the expression.
fn init_references_ctor_param(sym: &SystemSym, init: &str) -> bool {
    let words: std::collections::HashSet<&str> = init
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect();
    sym.params
        .state
        .iter()
        .chain(&sym.params.enter)
        .chain(&sym.params.domain)
        .any(|p| words.contains(p.name.as_str()))
}

/// Pull a value of declared type `t` out of framec's own `Object` container. Java's own rule
/// that an `Object` cannot be cast to a primitive, applied to framec's own scaffolding — keyed on
/// Java's fixed primitive set, with a VERBATIM `(t) x` cast for every reference type (NO branch on
/// user type names). The Number-ladder survives a persist round-trip's `Long`/`BigDecimal`.
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
        other => Atom::cast(other, x),
    }
}

/// The boxed spelling of a primitive, for a Java generic parameter that forbids a primitive
/// (framec's `CompletableFuture<...>` wrapper). Reference types pass verbatim.
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

/// Exposed for the acceptance test. Kernel-model state-var read: `(<Object>) compartment
/// .state_vars.get("name")` — a parenthesized cast, an atom and a valid lvalue root. (Type-aware
/// reads go through [`Backend::lower_ref`], which has the symbol table; this typeless helper casts
/// to `Object`.)
pub fn state_var_read(_state: &str, name: &str) -> Atom {
    Atom::cast(
        "Object",
        Atom::method(Atom::field(Atom::ident("compartment"), "state_vars"), "get", &format!("\"{name}\"")),
    )
}
