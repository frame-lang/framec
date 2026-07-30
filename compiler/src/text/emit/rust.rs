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
use super::driver::{param_names, Backend, BodyRole, LeafCtx};
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

    /// KERNEL model (an ordinary system) carries NO file-level preamble — legacy 4.6.1 emits none,
    /// so a foundation file starts at its first item (the leading water, then the per-item
    /// `#[allow]`s). The thin `@@[scan]` model keeps its `use std::collections::HashMap; use
    /// std::any::Any;` header (the 24 self-hosted `.gen.rs` are byte-frozen with it). So the header
    /// is emitted IFF the file scans.
    fn file_header_ctx(&self, has_scan: bool, out: &mut Sink) {
        if has_scan {
            self.file_header(out);
        }
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
        // `@@[scan(<elem>)]` — a positioned, borrowed-input scanner (RFC-0042.1 / #209).
        // Emits the generic input-source trait, a machine generic over `I`, and
        // `over`/`scan_at` instead of `new`. An ordinary system falls through unchanged.
        // SCAN SYSTEMS STAY ON THE THIN/SCANNER MODEL — the 24 `@@[scan]` `.gen.rs` are
        // byte-frozen (M1 bootstrap constraint), so every leaf below gates on `sym.scan`.
        if let Some(elem) = sym.scan.clone() {
            self.open_scanner(sym, &elem, out);
            return;
        }

        // M1 FOUNDATION — the faithful legacy-4.6.1 KERNEL MODEL (docs/faithfulness/M1.md,
        // lang-rust.md). A per-system `mod _<snake>_framec { ... }` wrapper holding the six
        // runtime types + the fixed kernel, then the interface/dispatch/handler walks append,
        // then `close_system` closes the mod and re-exports. Borrowed-domain systems thread
        // `'a` through the struct + impl + ctor (crux-proven).
        emit_kernel_open(sym, self, out);
    }

    fn close_system(&self, sym: &SystemSym, out: &mut Sink) {
        // SCAN systems: a bare `impl` at column 0, closed with `}`.
        if sym.scan.is_some() {
            out.frame("}\n");
            return;
        }
        // KERNEL systems: close the 4-indent `impl`, close the `mod`, re-export.
        out.frame("    }\n}\n");
        out.frame(&format!("pub use _{}_framec::*;\n", snake_system(&sym.name)));
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
        sym: &SystemSym,
        event: &str,
        params: &str,
        ret: Option<&str>,
        is_async: bool,
        arms: &[(String, String)],
        out: &mut Sink,
    ) {
        // SCAN systems: the thin/scanner direct-dispatch router (24 `.gen.rs` byte-frozen).
        if sym.scan.is_some() {
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
                out.frame("        Default::default()\n");
            }
            out.frame("    }\n\n");
            return;
        }

        // KERNEL systems: the PUBLIC interface wrapper — build the event, push a context, run
        // the kernel, then (value method) read the typed return slot back.
        let n = &sym.name;
        let sig = self.param_list(params);
        let sep = if sig.is_empty() { "" } else { ", " };
        let names: Vec<String> = super::driver::params_split(params)
            .into_iter()
            .filter(|(nm, _)| !nm.is_empty())
            .map(|(nm, _)| nm)
            .collect();
        let ctor_fields = if names.is_empty() {
            String::new()
        } else {
            format!(
                " {} ",
                names.iter().map(|p| format!("{p}: {p}")).collect::<Vec<_>>().join(", ")
            )
        };
        let ev = pascal_event(event);
        out.frame(&format!(
            "\n        pub fn {event}(&mut self{sep}{sig}){} {{\n",
            self.return_type(ret)
        ));
        out.frame(&format!(
            "            let __e = alloc::rc::Rc::new({n}FrameEvent::{ev} {{{ctor_fields}}});\n"
        ));
        out.frame(&format!(
            "            let mut __ctx = {n}FrameContext::new(alloc::rc::Rc::clone(&__e), None);\n"
        ));
        out.frame("            self._context_stack.push(__ctx);\n");
        out.frame("            self.__kernel(&__e);\n");
        match ret.map(str::trim).filter(|t| !t.is_empty()) {
            Some(rt) => {
                out.frame("            let __ctx = self._context_stack.pop().expect(\"invariant: handler must have pushed a context before reading return\");\n");
                out.frame("            match __ctx._return {\n");
                out.frame(&format!("                Some({n}FrameReturn::{ev}(v)) => v,\n"));
                out.frame(&format!(
                    "                Some({n}FrameReturn::_Lifecycle(v)) => v.downcast_ref::<{rt}>().cloned().unwrap_or_default(),\n"
                ));
                out.frame("                _ => Default::default(),\n");
                out.frame("            }\n");
                out.frame("        }\n");
            }
            None => {
                out.frame("            self._context_stack.pop();\n");
                out.frame("        }\n");
            }
        }
    }

    /// KERNEL state dispatcher `_state_<S>` (SCAN systems dispatch directly in `route`, so this
    /// emits nothing for them — the 24 `.gen.rs` stay byte-frozen).
    ///
    /// The per-state dispatcher is the rust-only `RustDispatch` `@@system` (`super::rust_dispatch`),
    /// pilot-style — rust's `match`-over-a-typed-enum dispatcher is a different control structure
    /// from the `if`-chain targets' shared `DispatchBody`, so it does not join that walk; it is
    /// sequenced through the three `rust_dispatch_*` leaves below. The byte-for-byte pre-conversion
    /// body is preserved as [`rust_dispatch_hand`] and gated in `tests/emit_scaffold_walks.rs`.
    fn dispatch(&self, sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
        super::rust_dispatch::drive(sym, state, arms, out);
    }

    /// One `__router` arm (KERNEL only — `router_walk::walk` is driven from `emit_kernel_open`).
    fn router_arm(&self, _sym: &SystemSym, state: &str, _first: bool, out: &mut Sink) {
        out.frame(&format!("                {state:?} => self._state_{state}(__ev),\n"));
    }

    /// One `__hsm_chain` arm (KERNEL only — driven from `emit_kernel_open`).
    fn hsm_chain_entry(&self, leaf: &str, chain: &[String], out: &mut Sink) {
        let list = chain.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(", ");
        out.frame(&format!("                {leaf:?} => &[{list}],\n"));
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
        // SCAN systems: the thin `<State>_<event>` handler, state params bound as locals from
        // the typed Args variant (24 `.gen.rs` byte-frozen).
        if sym.scan.is_some() {
            let sig = self.param_list(params);
            let sep = if sig.is_empty() { "" } else { ", " };
            let a = if is_async { "async " } else { "" };
            out.frame(&format!(
                "    {a}fn {}_{}(&mut self{sep}{sig}){} {{\n",
                state,
                rust_ident(event),
                self.return_type(ret)
            ));
            if let Some(st) = sym.states.iter().find(|s| s.name == state) {
                for p in &st.state_params {
                    let ty = st.state_param_types.get(p).cloned().unwrap_or_else(|| "()".into());
                    out.frame(&format!(
                        "        let {p}: {ty} = {};\n",
                        typed_read(sym, state, "Args", "args", p)
                    ));
                }
            }
            return;
        }

        // KERNEL systems: the private `_s_<state>_hdl_...` method — VOID (parks the return in
        // the slot), event params / enter-args ride on the signature (bound by the dispatcher).
        let n = &sym.name;
        let sig = self.param_list(params);
        let sep = if sig.is_empty() { "" } else { ", " };
        let method = kernel_handler_method(state, event);
        out.frame(&format!(
            "\n        fn {method}(&mut self, __e: &{n}FrameEvent{sep}{sig}) {{\n"
        ));
    }

    fn close_handler(&self, ret: Option<&str>, _is_async: bool, terminated: bool, ctx: &LeafCtx, out: &mut Sink) {
        // KERNEL: the private `_s_<S>_hdl_...` method is VOID — it parks its value in the context
        // return slot, so there is never a fallback value, and it closes at the method's own
        // 8-space `impl` indent with no trailing blank (the NEXT method's leading `\n` supplies the
        // separator). SCAN: a value-returning thin dispatch method that might fall through needs a
        // `Default::default()`, and closes at 4-space with a trailing blank.
        if !ctx.is_scan {
            out.frame("        }\n");
            return;
        }
        if ret.is_some() && !terminated {
            out.frame("        Default::default()\n");
        }
        out.frame("    }\n\n");
    }

    fn pad(&self, rel: u32) -> String {
        // The SCAN model's base: a bare `impl` at column 0, method at 4, body at 8.
        format!("        {}", " ".repeat(rel as usize))
    }

    /// Rust nests methods two levels inside the `mod`/`impl` — `pub fn` sits at column 8 — so a
    /// member-level comment lands there too.
    fn member_indent(&self) -> &'static str {
        "        "
    }

    /// Rust actions are TRAILING-separated: [`Self::close_action`] emits `}\n\n`, so a blank line
    /// already precedes the next action, and [`Self::open_action`] opens straight with `    fn`
    /// (no leading `\n`). A member comment before an action must therefore emit each line
    /// newline-TERMINATED with no leading blank of its own — Model B — or the missing terminator
    /// would run `// comment` and `fn act(...)` onto one line and comment the method out.
    fn actions_comment(&self, lines: &[String], out: &mut Sink) {
        let indent = self.member_indent();
        for line in lines {
            out.frame(indent);
            out.frame(line);
            out.frame("\n");
        }
    }

    /// Rust SCAN handlers open at `    fn` (4-space impl level; no `mod` wrapper adds depth) with no
    /// leading `\n`, and are trailing-separated by the previous handler's close. A comment before one
    /// must therefore emit each line newline-TERMINATED at that 4-space column with no leading blank
    /// (Model B) — Model A would swallow the `fn` onto the last comment line, commenting the method
    /// out. KERNEL handlers open with `\n        fn`, whose leading `\n` terminates the comment, so
    /// they keep Model A.
    fn handler_comment(&self, lines: &[String], is_scan: bool, out: &mut Sink) {
        if is_scan {
            for line in lines {
                out.frame("    ");
                out.frame(line);
                out.frame("\n");
            }
        } else {
            self.member_comment(lines, out);
        }
    }

    /// KERNEL handler bodies sit one `impl`-level deeper than a scanner's (the `mod _<sys>_framec`
    /// wrapper adds a level): `mod`(0) > `impl`(4) > `fn`(8) > body(12). So the kernel base is 12,
    /// the scanner's 8 (= [`Self::pad`]).
    fn pad_ctx(&self, rel: u32, is_scan: bool) -> String {
        if is_scan {
            self.pad(rel)
        } else {
            format!("            {}", " ".repeat(rel as usize))
        }
    }

    fn forward_to_declared_parent(&self) -> bool {
        // Rust has the `_state_<Name>` dispatcher layer (like java), so `=> $^` forwards to the
        // DECLARED parent unconditionally — matching legacy's `self._state_<parent>(__e)` — rather
        // than climbing (resolve_forward) to an ancestor that HANDLES the event, which drops the
        // call entirely when the immediate parent is handler-less (e.g. an empty `$P {}`).
        true
    }

    fn forward(&self, rel: u32, owner: &str, _event: &str, _params: &str, out: &mut Sink) {
        // `=> $^` — dispatch this event to the declared parent's state handler via the router.
        // This lives in a KERNEL handler body, so it indents at the kernel base (12), like every
        // other kernel statement (transition/lifecycle) — `pad_ctx`, not the scanner's `pad` (8).
        let p = self.pad_ctx(rel, false);
        out.frame(&format!("{p}self._state_{owner}(__e);\n"));
    }

    fn native_stmt(&self, rel: u32, text: NativeText, ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&self.pad_ctx(rel, ctx.is_scan));
        out.native(text);
        out.frame("\n");
    }

    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        // SCAN systems: build the typed compartment and swap it in (24 `.gen.rs` byte-frozen).
        if sym.scan.is_some() {
            let p = self.pad(rel);
            self.enter(&p, sym, target, args, out);
            out.frame(&format!("{p}self.compartment = __next;\n"));
            return;
        }
        // KERNEL systems: build via `__prepareEnter` (HSM chain) and QUEUE on the kernel drain.
        self.transition_with_enter(rel, sym, target, args, None, out);
    }

    /// KERNEL transition with an enter payload: `-> (enter) $Target` writes the enter args into
    /// the destination's TYPED state ctx before queueing. SCAN systems fall back to the thin
    /// `transition` (default ignores the payload, matching the 24 byte-frozen `.gen.rs`).
    fn transition_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        if sym.scan.is_some() {
            self.transition(rel, sym, target, args, out);
            return;
        }
        let n = &sym.name;
        let p = self.pad_ctx(rel, false);
        out.frame(&format!("{p}let mut __compartment = self.__prepareEnter({target:?});\n"));
        // `-> (enter) $Target` writes the enter payload into the destination's `$>` ENTER params
        // (stored in its unified Context) — NOT the state header params.
        let params: Vec<String> = sym
            .states
            .iter()
            .find(|s| s.name == target)
            .map(|s| enter_handler_params(s).into_iter().map(|(nm, _)| nm).collect())
            .unwrap_or_default();
        if let Some(a) = enter_args.map(str::trim).filter(|a| !a.is_empty()) {
            if !params.is_empty() {
                out.frame(&format!("{p}{{\n"));
                out.frame(&format!(
                    "{p}    if let {n}StateContext::{target}(ref mut ctx) = __compartment.state_context {{\n"
                ));
                if params.len() == 1 {
                    out.frame(&format!("{p}        ctx.{} = {a};\n", params[0]));
                } else {
                    let pat = params
                        .iter()
                        .map(|q| format!("__a_{q}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.frame(&format!("{p}        let ({pat}) = ({a});\n"));
                    for q in &params {
                        out.frame(&format!("{p}        ctx.{q} = __a_{q};\n"));
                    }
                }
                out.frame(&format!("{p}    }}\n"));
                out.frame(&format!("{p}}}\n"));
            }
        }
        out.frame(&format!("{p}self.__transition(__compartment);\n"));
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        // KERNEL statement: kernel base (pad_ctx), and the struct fields are `_state_stack` and
        // `__compartment` (NOT `self.stack`/`self.compartment`, which don't exist -> E0609).
        let p = self.pad_ctx(rel, false);
        // Build the target compartment FIRST, then swap it in and push the displaced one.
        // Building `__next` first means the swap needs no empty-state placeholder — the
        // typed compartment has no `""` state to construct.
        self.enter(&p, sym, target, args, out);
        out.frame(&format!(
            "{p}self._state_stack.push(std::mem::replace(&mut self.__compartment, __next));\n"
        ));
    }

    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad_ctx(rel, false);
        out.frame(&format!("{p}self.__compartment = self._state_stack.pop().unwrap();\n"));
    }

    fn push_bare(&self, rel: u32, out: &mut Sink) {
        // Push a CLONE of the current compartment; stay. The typed `<Sys>Comp` derives Clone.
        out.frame(&format!(
            "{}self._state_stack.push(self.__compartment.clone());\n",
            self.pad_ctx(rel, false)
        ));
    }

    fn pop_bare(&self, rel: u32, out: &mut Sink) {
        // Pop and drop the top; stay.
        out.frame(&format!("{}self._state_stack.pop();\n", self.pad_ctx(rel, false)));
    }

    fn lifecycle_call(&self, rel: u32, sym: &SystemSym, state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        // KERNEL systems: `$>`/`<$` are synthesized by the kernel DRAIN, never called from a
        // handler — so this is a no-op (matching Python's kernel model).
        if sym.scan.is_none() {
            return;
        }
        // SCAN systems: the thin direct lifecycle-handler call (24 `.gen.rs` byte-frozen).
        let p = self.pad(rel);
        out.frame(&format!("{p}self.{state}_{}({});\n", rust_ident(event), args.unwrap_or("")));
    }

    fn pop_enter(&self, rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        // CONSUMED (inc4): the pop-enter walk is now the Cauldron-mechanized `PopEnter` @@system —
        // the first mechanized system driven in anger. The live differential is the emit snapshot
        // suite (which captured the original walk's output); a dedicated frozen-oracle GATE-A parity
        // report (as the other reified systems have) is the remaining follow-up.
        super::pop_enter::drive(rel, sym, enter_args, out);
    }

    fn terminate(&self, rel: u32, ctx: &LeafCtx, out: &mut Sink) {
        // KERNEL: the transition queued the next compartment and the handler is VOID — a bare
        // `return;` hands control back to the kernel drain (matching legacy). SCAN: the thin
        // dispatch method returns the machine's value type, so its terminal is
        // `return Default::default();`.
        if ctx.is_scan {
            out.frame(&format!("{}return Default::default();\n", self.pad(rel)));
        } else {
            out.frame(&format!("{}return;\n", self.pad_ctx(rel, false)));
        }
    }

    fn return_call(&self, role: BodyRole, rel: u32, _is_async: bool, _multiline: bool, expr: NativeText, ctx: &LeafCtx, out: &mut Sink) {
        // `multiline` is ignored: a `;`-terminated statement carries its own continuation across
        // newlines — no wrapping parens are needed (or wanted).
        //
        // A SCANNER's thin dispatch handler, or an ordinary `actions:`/`operations:` method (no
        // live kernel context to park on), spells a real `return <expr>;`.
        if ctx.is_scan || role == BodyRole::Action {
            out.frame(&self.pad_ctx(rel, ctx.is_scan));
            out.frame("return ");
            out.native(expr);
            out.frame(";\n");
            return;
        }
        // KERNEL machine handler: the private method is VOID. `@@:(expr)` builds the typed return
        // slot `<Sys>FrameReturn::<Method>(expr)` and parks it on the live `FrameContext`; the
        // handler RUNS ON (no `return` — `return_call_terminates` is false), and the public
        // interface wrapper reads the slot back after the kernel returns. The slot-write's second
        // line sits at legacy's fixed 28-space continuation indent.
        out.frame(&format!(
            "{}let __return_val = {}FrameReturn::{}(",
            self.pad_ctx(rel, false),
            ctx.sym.name,
            pascal_event(ctx.event)
        ));
        out.native(expr);
        out.frame(");\n");
        out.frame("                            if let Some(ctx) = self._context_stack.last_mut() { ctx._return = Some(__return_val); }\n");
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

    /// KERNEL reentrancy guard: after a `@@:self.<method>()` self interface call re-enters
    /// dispatch, bail if it queued a transition — otherwise the remaining statements run in the
    /// wrong state. Rust's `.last().map_or(false, …)` folds the empty-stack case (a `$>`/`<$`
    /// lifecycle body, dispatched with no context pushed) into a safe no-op, so no separate
    /// non-empty check is needed. Emitted at the call statement's own indent (kernel base 12 + rel).
    fn reentrancy_guard(&self, rel: u32, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&format!(
            "{}if self._context_stack.last().map_or(false, |ctx| ctx._transitioned) {{ return; }}\n",
            self.pad_ctx(rel, false)
        ));
    }

    /// A KERNEL statement bearing a self-call (native or Frame assignment) is NOT base-subtracted
    /// (the shipped Rust quirk): it lands at `12 + full source column`, and its guard follows at the
    /// same indent. A scan system keeps the base-relative basis so its byte-frozen `.gen.rs` do not
    /// move.
    fn selfcall_stmt_rel(&self, col: u32, base: u32, is_scan: bool) -> u32 {
        if is_scan {
            col.saturating_sub(base)
        } else {
            col
        }
    }

    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, is_operation: bool, is_static: bool, out: &mut Sink) {
        let sig = self.param_list(params);
        // KERNEL member: a LEADING blank separates each action/operation, the signature sits at the
        // impl member column (8), and `operations:` are `pub` while `actions:` are private
        // (categorical by section, NOT keyed on return type — a void operation is still `pub`, a
        // value-returning action is still private).
        let vis = if is_operation { "pub " } else { "" };
        // A `static` operation has NO `self` receiver (frame_language: "no self/this access").
        let receiver = if is_static {
            sig
        } else {
            let sep = if sig.is_empty() { "" } else { ", " };
            format!("&mut self{sep}{sig}")
        };
        out.frame(&format!(
            "\n        {vis}fn {name}({receiver}){} {{\n",
            self.return_type(ret)
        ));
    }

    fn close_action(&self, out: &mut Sink) {
        // 8-space impl-member close, no trailing blank (the next member supplies its own leading one).
        out.frame("        }\n");
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
        let scan = sym.scan.is_some();
        let p = self.pad_ctx(rel, scan);
        match lhs.kind {
            // A domain field IS an lvalue — `self.field = rhs;`. A Place, never
            // parenthesized (`(self.field) = x` is not idiomatic and here would be wrong).
            RefKind::ContextSelf => {
                out.frame(&format!("{p}self.{} = ", lhs.name));
                out.native(rhs);
                out.frame(";\n");
            }
            // A state var write.
            //
            // SCAN model: a field of the current compartment's typed `<Sys>Vars` variant.
            //
            // KERNEL model (legacy): the var lives in the state's unified Context, reached by
            // climbing the compartment's PARENT CHAIN to the owning state. The RHS is bound to
            // `__rhs` FIRST — `$.x = $.x + 1` reads the same context the write borrows mutably, so
            // evaluating the value before taking `&mut` avoids E0502.
            RefKind::StateVar if scan => {
                out.frame(&format!("{p}{{ let __v = "));
                out.native(rhs);
                out.frame(&format!(
                    "; if let {}Vars::{state} {{ {n}, .. }} = &mut self.compartment.vars {{ *{n} = __v; }} }}\n",
                    sym.name,
                    n = lhs.name
                ));
            }
            RefKind::StateVar => {
                let n = &sym.name;
                out.frame(&format!("{p}{{\n{p}    let __rhs = "));
                out.native(rhs);
                out.frame(&format!(";\n{p}    let mut __cursor: Option<&mut {n}Compartment> = Some(&mut self.__compartment);\n"));
                out.frame(&format!("{p}    while let Some(__c) = __cursor {{\n"));
                out.frame(&format!("{p}        if __c.state == {state:?} {{\n"));
                out.frame(&format!("{p}            if let {n}StateContext::{state}(ref mut ctx) = __c.state_context {{\n"));
                out.frame(&format!("{p}                ctx.{} = __rhs;\n", lhs.name));
                out.frame(&format!("{p}            }}\n"));
                out.frame(&format!("{p}            break;\n"));
                out.frame(&format!("{p}        }}\n"));
                out.frame(&format!("{p}        __cursor = __c.parent_compartment.as_deref_mut();\n"));
                out.frame(&format!("{p}    }}\n"));
                out.frame(&format!("{p}}}\n"));
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
        // `@@Sys(...)` in native water lowers to the KERNEL factory `Sys::__create(...)` — the
        // two-phase constructor that runs the start state's `$>` drain (legacy). (`new()` exists
        // too, but constructs WITHOUT entering; `@@Sys()` is the entered form.)
        Atom::call(format!("{name}::__create"), args.join(", "))
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
            // SCAN model: read out of the typed `<Sys>Vars` variant. KERNEL model (legacy): climb
            // the compartment parent chain to the owning state and read the field out of its
            // unified Context.
            RefKind::StateVar if sym.scan.is_some() => {
                Atom::ident(typed_read(sym, state, "Vars", "vars", &r.name))
            }
            RefKind::StateVar => Atom::ident(kernel_state_read(sym, state, &r.name)),
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
        // KERNEL model field/type names + `impl`-level (8-space) indent: `_control` is the whole
        // typed `<Sys>Compartment` (state name + unified state_context) and `_stack` is the
        // compartment stack `_state_stack`; both derive serde (see `kderive`), so the host
        // serializer marshals the control state natively and a restore rebuilds it.
        let schema = m.schema();
        let comp = format!("{}Compartment", m.sys);
        let snap_struct = |out: &mut Sink| {
            out.frame("            #[derive(serde::Serialize, serde::Deserialize)]\n");
            out.frame("            struct __Snap {\n");
            out.frame("                _schema: String,\n");
            out.frame(&format!("                _control: {comp},\n"));
            out.frame(&format!("                _stack: Vec<{comp}>,\n"));
            for (n, t) in &m.fields {
                out.frame(&format!("                {n}: {t},\n"));
            }
            out.frame("            }\n");
        };

        out.frame(&format!("        pub fn {}(&self) -> {} {{\n", m.save, m.blob));
        snap_struct(out);
        out.frame("            let __snap = __Snap {\n");
        out.frame(&format!("                _schema: {schema:?}.to_string(),\n"));
        out.frame("                _control: self.__compartment.clone(),\n");
        out.frame("                _stack: self._state_stack.clone(),\n");
        for (n, _) in &m.fields {
            out.frame(&format!("                {n}: self.{n}.clone(),\n"));
        }
        out.frame("            };\n");
        out.frame("            serde_json::to_string(&__snap).unwrap()\n");
        out.frame("        }\n\n");

        out.frame(&format!("        pub fn {}(&mut self, data: {}) {{\n", m.load, m.blob));
        snap_struct(out);
        out.frame("            let __snap: __Snap = serde_json::from_str(&data).unwrap();\n");
        out.frame(&format!("            if __snap._schema != {schema:?} {{\n"));
        out.frame("                panic!(\"E751: persist restore refused - snapshot schema does not match this program\");\n");
        out.frame("            }\n");
        out.frame("            self.__compartment = __snap._control;\n");
        out.frame("            self._state_stack = __snap._stack;\n");
        for (n, _) in &m.fields {
            out.frame(&format!("            self.{n} = __snap.{n};\n"));
        }
        out.frame("        }\n\n");
    }
}

// ======================================================================================
// RUST PER-STATE DISPATCHER LEAVES — the three fragments the `RustDispatch` `@@system`
// (`super::rust_dispatch`) sequences. They live HERE, not in the walk module, because they
// need rust.rs's private helpers (`pascal_event`, `kernel_handler_method`,
// `enter_handler_params`, `super::driver::params_split`). Byte-exact against the frozen
// [`rust_dispatch_hand`] via GATE-A (`tests/emit_scaffold_walks.rs`).
// ======================================================================================

/// The dispatcher method HEADER + the `match __e {` line — the bytes before the arms.
pub(super) fn rust_dispatch_open(sym: &SystemSym, state: &str, out: &mut Sink) {
    let n = &sym.name;
    out.frame(&format!("\n        fn _state_{state}(&mut self, __e: &{n}FrameEvent) {{\n"));
    out.frame("            match __e {\n");
}

/// One dispatch ARM — the loop body for the event message at slot `ai`. Recomputes `n`/`st`/`ev`/
/// `method` inside (the walk carries no register): `$>` enter (parent-chain ctx-climb), `<$` exit,
/// else a user event's variant destructure. Out of range emits nothing (total).
pub(super) fn rust_dispatch_arm(sym: &SystemSym, state: &str, arms: &[String], ai: usize, out: &mut Sink) {
    let Some(msg) = arms.get(ai) else { return };
    let n = &sym.name;
    let st = sym.states.iter().find(|s| s.name == state);
    let ev = pascal_event(msg);
    let method = kernel_handler_method(state, msg);
    if msg == "$>" {
        // Enter: deliver the `$>` ENTER params from the destination's typed ctx (climb the
        // parent chain to the owning state, read each field, pass positionally). These are
        // the enter handler's own params — NOT the state header params — and are stored in
        // the unified state Context (`state_ctx_fields`).
        let params: Vec<String> = st
            .map(|s| enter_handler_params(s).into_iter().map(|(nm, _)| nm).collect())
            .unwrap_or_default();
        if params.is_empty() {
            out.frame(&format!(
                "                {n}FrameEvent::FrameEnter {{ .. }} => {{ self.{method}(__e); }}\n"
            ));
        } else {
            out.frame(&format!("                {n}FrameEvent::FrameEnter {{ .. }} => {{\n"));
            for p in &params {
                out.frame(&format!("                    let {p} = {{\n"));
                out.frame("                        let mut __sc = &self.__compartment;\n");
                out.frame(&format!("                        while __sc.state != {state:?} {{\n"));
                out.frame("                            match __sc.parent_compartment.as_deref() {\n");
                out.frame("                                Some(p) => __sc = p,\n");
                out.frame("                                None => break,\n");
                out.frame("                            }\n");
                out.frame("                        }\n");
                out.frame("                        match &__sc.state_context {\n");
                out.frame(&format!(
                    "                            {n}StateContext::{state}(ctx) => ctx.{p}.clone(),\n"
                ));
                out.frame("                            _ => Default::default(),\n");
                out.frame("                        }\n");
                out.frame("                    };\n");
            }
            let args = params.iter().map(String::as_str).collect::<Vec<_>>().join(", ");
            out.frame(&format!("                    self.{method}(__e, {args});\n"));
            out.frame("                }\n");
        }
    } else if msg == "<$" {
        out.frame(&format!(
            "                {n}FrameEvent::FrameExit {{ .. }} => {{ self.{method}(__e); }}\n"
        ));
    } else {
        // User event: destructure the variant's fields, pass positionally.
        let iface = sym.interface.iter().find(|m| &m.name == msg);
        let params: Vec<(String, Option<String>)> = iface
            .map(|m| {
                super::driver::params_split(m.params_text.as_deref().unwrap_or(""))
                    .into_iter()
                    .filter(|(nm, _)| !nm.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if params.is_empty() {
            out.frame(&format!(
                "                {n}FrameEvent::{ev} {{ .. }} => {{ self.{method}(__e); }}\n"
            ));
        } else {
            let pat = params.iter().map(|(nm, _)| nm.as_str()).collect::<Vec<_>>().join(", ");
            // #186: framec is type-ignorant and cannot assume a param is `Copy`, so a destructured
            // event field is CLONED by default — moving it out with `*name` from the shared
            // `&FrameEvent` is E0507 for any non-Copy payload (String/Vec/map/Rc/user struct). Only
            // the built-in Copy scalars keep the cheap `*name` deref (where `.clone()` would draw
            // clippy's clone_on_copy).
            let pass = params
                .iter()
                .map(|(nm, ty)| {
                    if ty.as_deref().map(is_rust_copy_scalar).unwrap_or(false) {
                        format!("*{nm}")
                    } else {
                        format!("{nm}.clone()")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            // A param handler's arm is MULTI-LINE (matching legacy and the `$>` case above): the
            // destructure + positional call sit on their own lines. Only the paramless arm above
            // stays single-line.
            out.frame(&format!(
                "                {n}FrameEvent::{ev} {{ {pat}, .. }} => {{\n"
            ));
            out.frame(&format!("                    self.{method}(__e, {pass});\n"));
            out.frame("                }\n");
        }
    }
}

/// CLOSE the dispatcher — the `_ => {}` default arm and the two closing braces.
pub(super) fn rust_dispatch_close(out: &mut Sink) {
    out.frame("                _ => {}\n");
    out.frame("            }\n");
    out.frame("        }\n");
}

/// The byte-for-byte **frozen oracle** for the Rust per-state dispatcher — a verbatim copy of the
/// pre-conversion `Backend::dispatch` body (scan-guard included), before it was reified as the
/// [`super::rust_dispatch`] `RustDispatch` `@@system`. Kept as the GATE-A differential the machine
/// is proven against (`tests/emit_scaffold_walks.rs`, via
/// [`super::driver::rust_dispatch_parity_report`]). It does NOT route through `be.dispatch` — it
/// reproduces the original bytes standalone, so a spelling bug in a `rust_dispatch` leaf is visible
/// to the gate. Doc-hidden and **not on the production path**. Do not edit it to add behavior: it
/// exists only to reproduce the pre-conversion value exactly, so any divergence is the machine's
/// bug, not the oracle's.
#[doc(hidden)]
pub(super) fn rust_dispatch_hand(sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
    if sym.scan.is_some() {
        return;
    }
    let n = &sym.name;
    out.frame(&format!("\n        fn _state_{state}(&mut self, __e: &{n}FrameEvent) {{\n"));
    out.frame("            match __e {\n");
    let st = sym.states.iter().find(|s| s.name == state);
    for msg in arms {
        let ev = pascal_event(msg);
        let method = kernel_handler_method(state, msg);
        if msg == "$>" {
            // Enter: deliver the `$>` ENTER params from the destination's typed ctx (climb the
            // parent chain to the owning state, read each field, pass positionally). These are
            // the enter handler's own params — NOT the state header params — and are stored in
            // the unified state Context (`state_ctx_fields`).
            let params: Vec<String> = st
                .map(|s| enter_handler_params(s).into_iter().map(|(nm, _)| nm).collect())
                .unwrap_or_default();
            if params.is_empty() {
                out.frame(&format!(
                    "                {n}FrameEvent::FrameEnter {{ .. }} => {{ self.{method}(__e); }}\n"
                ));
            } else {
                out.frame(&format!("                {n}FrameEvent::FrameEnter {{ .. }} => {{\n"));
                for p in &params {
                    out.frame(&format!("                    let {p} = {{\n"));
                    out.frame("                        let mut __sc = &self.__compartment;\n");
                    out.frame(&format!("                        while __sc.state != {state:?} {{\n"));
                    out.frame("                            match __sc.parent_compartment.as_deref() {\n");
                    out.frame("                                Some(p) => __sc = p,\n");
                    out.frame("                                None => break,\n");
                    out.frame("                            }\n");
                    out.frame("                        }\n");
                    out.frame("                        match &__sc.state_context {\n");
                    out.frame(&format!(
                        "                            {n}StateContext::{state}(ctx) => ctx.{p}.clone(),\n"
                    ));
                    out.frame("                            _ => Default::default(),\n");
                    out.frame("                        }\n");
                    out.frame("                    };\n");
                }
                let args = params.iter().map(String::as_str).collect::<Vec<_>>().join(", ");
                out.frame(&format!("                    self.{method}(__e, {args});\n"));
                out.frame("                }\n");
            }
        } else if msg == "<$" {
            out.frame(&format!(
                "                {n}FrameEvent::FrameExit {{ .. }} => {{ self.{method}(__e); }}\n"
            ));
        } else {
            // User event: destructure the variant's fields, pass positionally.
            let iface = sym.interface.iter().find(|m| &m.name == msg);
            let fields: Vec<String> = iface
                .map(|m| {
                    super::driver::params_split(m.params_text.as_deref().unwrap_or(""))
                        .into_iter()
                        .filter(|(nm, _)| !nm.is_empty())
                        .map(|(nm, _)| nm)
                        .collect()
                })
                .unwrap_or_default();
            if fields.is_empty() {
                out.frame(&format!(
                    "                {n}FrameEvent::{ev} {{ .. }} => {{ self.{method}(__e); }}\n"
                ));
            } else {
                let pat = fields.join(", ");
                let pass = fields.iter().map(|f| format!("*{f}")).collect::<Vec<_>>().join(", ");
                out.frame(&format!(
                    "                {n}FrameEvent::{ev} {{ {pat}, .. }} => {{ self.{method}(__e, {pass}); }}\n"
                ));
            }
        }
    }
    out.frame("                _ => {}\n");
    out.frame("            }\n");
    out.frame("        }\n");
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
///
/// A free fn (not a `Backend` method), and a one-line driver into the
/// [`super::rust_compartment_types`] `RustCompartmentTypes` `@@system`. The byte-for-byte ORACLE it
/// replaced is the preserved [`rust_compartment_types_hand`], gated in `tests/emit_scaffold_walks.rs`
/// (GATE-A, via [`super::driver::rust_compartment_types_parity_report`]). `pub(super)` so the parity
/// report can drive the machine path directly.
pub(super) fn emit_compartment_types(sym: &SystemSym, out: &mut Sink) {
    super::rust_compartment_types::drive(sym, out);
}

// ======================================================================================
// RUST TYPED-COMPARTMENT LEAVES — the six fragments the `RustCompartmentTypes` `@@system`
// (`super::rust_compartment_types`) sequences. They live HERE, not in the walk module, because
// they need rust.rs's private helpers (`state_var_ty`) and `sym.persist_reachable` /
// `state_param_types`. The serde `derive` is recomputed inside each opener/`ct_comp` from
// `sym.persist_reachable` (an ordinary system stays serde-free; a persist-reachable one derives
// serde). Byte-exact against the frozen [`rust_compartment_types_hand`] via GATE-A
// (`tests/emit_scaffold_walks.rs`).
// ======================================================================================

/// Open the `<Sys>Vars` enum: the serde `derive` + `enum {name}Vars {`.
pub(super) fn ct_vars_open(sym: &SystemSym, out: &mut Sink) {
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
}

/// One `<Sys>Vars` variant — state `vi`'s `$.` state vars as `{v.name}: {state_var_ty(v)}`.
pub(super) fn ct_vars_variant(sym: &SystemSym, vi: usize, out: &mut Sink) {
    let st = &sym.states[vi];
    let fields = st
        .state_vars
        .iter()
        .map(|v| format!("{}: {}", v.name, state_var_ty(v)))
        .collect::<Vec<_>>()
        .join(", ");
    out.frame(&format!("    {} {{ {fields} }},\n", st.name));
}

/// Open the `<Sys>Args` enum: the serde `derive` + `enum {name}Args {`.
pub(super) fn ct_args_open(sym: &SystemSym, out: &mut Sink) {
    let name = &sym.name;
    let derive = if sym.persist_reachable {
        "#[derive(Clone, serde::Serialize, serde::Deserialize)]\n"
    } else {
        "#[derive(Clone)]\n"
    };
    out.frame(derive);
    out.frame(&format!("enum {name}Args {{\n"));
}

/// One `<Sys>Args` variant — state `ai`'s `(param)` args as `{p}: {ty}` (`ty` from
/// `state_param_types`, else `()`).
pub(super) fn ct_args_variant(sym: &SystemSym, ai: usize, out: &mut Sink) {
    let st = &sym.states[ai];
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

/// Close an enum — the `}` line shared by the `<Sys>Vars` and `<Sys>Args` closers.
pub(super) fn ct_close(out: &mut Sink) {
    out.frame("}\n");
}

/// The `<Sys>Comp` struct — the serde `derive` + the fixed `{ state, vars, args }` fields,
/// self-terminated with the trailing blank line.
pub(super) fn ct_comp(sym: &SystemSym, out: &mut Sink) {
    let name = &sym.name;
    let derive = if sym.persist_reachable {
        "#[derive(Clone, serde::Serialize, serde::Deserialize)]\n"
    } else {
        "#[derive(Clone)]\n"
    };
    out.frame(derive);
    out.frame(&format!("struct {name}Comp {{\n"));
    out.frame("    state: String,\n");
    out.frame(&format!("    vars: {name}Vars,\n"));
    out.frame(&format!("    args: {name}Args,\n"));
    out.frame("}\n\n");
}

/// The byte-for-byte **frozen oracle** for the Rust typed-compartment emitter — a verbatim copy of
/// the pre-conversion `emit_compartment_types` body, before it was reified as the
/// [`super::rust_compartment_types`] `RustCompartmentTypes` `@@system`. Kept as the GATE-A
/// differential the machine is proven against (`tests/emit_scaffold_walks.rs`, via
/// [`super::driver::rust_compartment_types_parity_report`]). It does NOT route through the machine —
/// it reproduces the original bytes standalone, so a spelling bug in a `ct_*` leaf is visible to the
/// gate. Doc-hidden and **not on the production path**. Do not edit it to add behavior: it exists
/// only to reproduce the pre-conversion value exactly, so any divergence is the machine's bug, not
/// the oracle's.
#[doc(hidden)]
pub(super) fn rust_compartment_types_hand(sym: &SystemSym, out: &mut Sink) {
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

/// Legacy's per-type default LITERAL for a KERNEL Context field with no initializer (a state
/// header param or a `$>` enter param). Legacy hard-codes a narrow map and falls back to
/// `Default::default()` for everything else — verified byte-for-byte against 4.6.0.33:
/// `{i32,i64,u32,u64} => 0`, `{f32,f64} => 0.0`, `bool => false`, `String => String::new()`, and
/// `i8`/`u8`/`i16`/`usize`/`isize`/custom all take the `Default::default()` fallback. (This is a
/// faithful reproduction of the oracle's type-aware seed, NOT a general type mapping.)
fn ctx_field_default(ty: &str) -> String {
    match ty.trim() {
        "i32" | "i64" | "u32" | "u64" => "0".to_string(),
        "f32" | "f64" => "0.0".to_string(),
        "bool" => "false".to_string(),
        "String" => "String::new()".to_string(),
        _ => "Default::default()".to_string(),
    }
}

/// A state's `$>` enter handler's params as `(name, type)` pairs — the ENTER params, which the
/// kernel model stores in the state's Context (delivered to the enter handler on drain, written by
/// `-> (args) $S`). Empty if the state declares no `$>` or a paramless one.
fn enter_handler_params(st: &crate::resolve::StateSym) -> Vec<(String, String)> {
    st.handlers
        .iter()
        .find(|h| h.event == "$>")
        .map(|h| {
            super::driver::params_split(&h.params_text)
                .into_iter()
                .filter(|(n, _)| !n.is_empty())
                .map(|(n, t)| (n, t.unwrap_or_default().trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// A state's UNIFIED per-state Context fields, in legacy order: state header params, then `$>`
/// enter params, then state vars — deduped by name (first occurrence wins). Each entry is
/// `(name, type, seed)` where `seed` is the field's `Default::default()` value: header/enter params
/// take [`ctx_field_default`] (a per-type literal), state vars take their declared init
/// ([`state_seed_value`]). This is the RFC-0056 unification the kernel model builds one
/// `<State>Context` struct + one `Default` impl from.
fn state_ctx_fields(st: &crate::resolve::StateSym) -> Vec<(String, String, String)> {
    let mut fields: Vec<(String, String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &st.state_params {
        if seen.insert(p.clone()) {
            let ty = st.state_param_types.get(p).map(String::as_str).unwrap_or("()").to_string();
            let seed = ctx_field_default(&ty);
            fields.push((p.clone(), ty, seed));
        }
    }
    for (n, t) in enter_handler_params(st) {
        if seen.insert(n.clone()) {
            let seed = ctx_field_default(&t);
            fields.push((n, t, seed));
        }
    }
    for v in &st.state_vars {
        if seen.insert(v.name.clone()) {
            fields.push((v.name.clone(), state_var_ty(v), state_seed_value(v)));
        }
    }
    fields
}

/// Read a state var / enter param / state arg out of the current compartment's Context by
/// climbing the parent chain to the OWNING state — the KERNEL-model state-var READ (legacy). Not
/// `.clone()`d (legacy relies on the field being `Copy`; a non-`Copy` state var is a latent legacy
/// bug this reproduces faithfully). Parenthesized, so it is an atom.
fn kernel_state_read(sym: &SystemSym, state: &str, field: &str) -> String {
    format!(
        "{{ let mut __sv_comp = &self.__compartment; while __sv_comp.state != {state:?} {{ __sv_comp = __sv_comp.parent_compartment.as_ref().expect(\"invariant: state-var target found in ancestor chain\"); }} match &__sv_comp.state_context {{ {sys}StateContext::{state}(ctx) => ctx.{field}, _ => unreachable!() }} }}",
        sys = sym.name
    )
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

/// The built-in Rust scalar types that are `Copy` — a destructured event field of one of these is
/// forwarded by `*name` deref; everything else (String/Vec/map/Rc/user struct — all unknown to a
/// type-ignorant compiler) is `.clone()`d, because moving out of the shared `&FrameEvent` is E0507
/// for any non-Copy payload (#186).
fn is_rust_copy_scalar(t: &str) -> bool {
    matches!(
        t.trim(),
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
    )
}

pub(super) fn rust_ident(event: &str) -> String {
    match event {
        "$>" => "__enter".to_string(),
        "<$" => "__exit".to_string(),
        other => other.to_string(),
    }
}

// ======================================================================================
// M1 FOUNDATION — the faithful legacy-4.6.1 KERNEL MODEL (docs/faithfulness/lang-rust.md).
// These emit the per-system `mod _<snake>_framec { ... }` wrapper + six runtime types +
// fixed kernel. Borrowed-domain systems thread `'a` (crux-proven, spike compiles + composes).
// ======================================================================================

/// PascalCase system name -> `snake_case` for the module name. `RouterWalk` -> `router_walk`.
fn snake_system(name: &str) -> String {
    let mut s = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                s.push('_');
            }
            s.push(ch.to_ascii_lowercase());
        } else {
            s.push(ch);
        }
    }
    s
}

/// The `<Sys>FrameEvent` variant identifier for an event message. `$>` -> `FrameEnter`,
/// `<$` -> `FrameExit`, a user event `go` -> `Go`, `on_click` -> `OnClick`.
fn pascal_event(event: &str) -> String {
    match event {
        "$>" => "FrameEnter".to_string(),
        "<$" => "FrameExit".to_string(),
        other => other
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect(),
    }
}

/// The private handler method name for `(state, event)`, kernel-model spelling.
fn kernel_handler_method(state: &str, event: &str) -> String {
    match event {
        "$>" => format!("_s_{state}_hdl_frame_enter"),
        "<$" => format!("_s_{state}_hdl_frame_exit"),
        other => format!("_s_{state}_hdl_user_{other}"),
    }
}

/// `"<'a>"` when any domain field is a read-only shared borrow, else `""`.
fn borrowed_lt(sym: &SystemSym) -> &'static str {
    if sym.domain.iter().any(is_borrowed_field) {
        "<'a>"
    } else {
        ""
    }
}

/// One domain field's Rust type, lifetime-threaded.
fn domain_field_ty(f: &crate::resolve::FieldSym) -> String {
    match &f.ty {
        TypeRef::Opaque(t) => thread_lt(t),
        TypeRef::System(s) | TypeRef::WrappedSystem { system: s, .. } => s.clone(),
        TypeRef::None => "()".to_string(),
    }
}

/// One domain field's construction init expression. A sub-system field `= @@Inner(..)` is FRAME
/// instantiation, which is the **two-phase factory** `Inner::__create(..)` — the same lowering
/// `@@Inner(..)` gets in native water (line 668), and what legacy emits: a domain sub-system runs
/// its start-state `$>` at the owner's construction time, so a plain `Inner::new(..)` (which skips
/// the enter lifecycle) is wrong. Any other init is the user's native expression, verbatim. (The
/// scanner path — [`open_scanner`] — deliberately uses `new`: a scanner constructs WITHOUT running,
/// RFC-0042's positioned `over()` model, and is a separate site.)
fn domain_field_init(f: &crate::resolve::FieldSym) -> String {
    match &f.init_system {
        Some(s) => format!("{s}::__create({})", super::ctor_init_args(f.init_text.as_deref())),
        None => f.init_text.clone().unwrap_or_else(|| "Default::default()".into()),
    }
}

/// Emit the KERNEL-MODEL system opening: the per-system module, the six runtime types, and the
/// fixed kernel methods (`new`, `__create`, `__hsm_chain`, `__prepareEnter`, `__kernel`,
/// `__router`, `__transition`). Leaves the `impl` OPEN — the interface/dispatch/handler walks
/// append, then `close_system` closes the `impl` + `mod` and re-exports.
fn emit_kernel_open(sym: &SystemSym, be: &dyn Backend, out: &mut Sink) {
    let n = &sym.name;
    let lt = borrowed_lt(sym);
    let snake = snake_system(n);
    let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");

    // A persist-reachable system's CONTROL STATE (the FrameEvent it may forward, each state's
    // Context, the StateContext, the Compartment) must serialize into the snapshot — so serde is
    // derived on exactly those types (RFC-0056). An ordinary system derives only `Clone`
    // (byte-unchanged). FrameReturn is NOT included: it carries `Rc<dyn Any>` and is never
    // snapshotted (the system is quiescent at save, its context stack empty).
    let kderive = if sym.persist_reachable {
        "    #[derive(Clone, serde::Serialize, serde::Deserialize)]\n"
    } else {
        "    #[derive(Clone)]\n"
    };

    // ---- the 13 outer #[allow] attributes on the module ----
    for a in [
        "dead_code",
        "non_camel_case_types",
        "non_snake_case",
        "unused_variables",
        "unused_mut",
        "unused_imports",
        "clippy::assign_op_pattern",
        "clippy::clone_on_copy",
        "clippy::derivable_impls",
        "clippy::match_single_binding",
        "clippy::needless_return",
        "clippy::new_without_default",
        "clippy::single_match",
    ] {
        out.frame(&format!("#[allow({a})]\n"));
    }
    out.frame(&format!("mod _{snake}_framec {{\n"));
    out.frame("    use super::*;\n");
    out.frame("    extern crate alloc;\n");
    out.frame("    use alloc::{vec, format};\n");

    // ---- L1 FrameEvent enum ----
    out.frame(kderive);
    out.frame("    #[allow(dead_code, non_camel_case_types)]\n");
    out.frame(&format!("    enum {n}FrameEvent {{\n"));
    for m in &sym.interface {
        let fields = super::driver::params_split(m.params_text.as_deref().unwrap_or(""))
            .into_iter()
            .filter(|(nm, _)| !nm.is_empty())
            .map(|(nm, t)| format!("{nm}: {}", t.unwrap_or_default().trim()))
            .collect::<Vec<_>>()
            .join(", ");
        out.frame(&format!("        {} {{ {fields} }},\n", pascal_event(&m.name)));
    }
    out.frame("        FrameEnter {},\n");
    out.frame("        FrameExit {},\n");
    out.frame("    }\n\n");

    // ---- L2 FrameReturn union: one tuple variant per value-returning method + _Lifecycle ----
    out.frame("    #[derive(Clone)]\n");
    out.frame("    #[allow(dead_code, non_camel_case_types)]\n");
    out.frame(&format!("    enum {n}FrameReturn {{\n"));
    for m in &sym.interface {
        if let Some(rt) = m.return_text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            out.frame(&format!("        {}({rt}),\n", pascal_event(&m.name)));
        }
    }
    out.frame("        _Lifecycle(alloc::rc::Rc<dyn core::any::Any>),\n");
    out.frame("    }\n\n");

    // ---- name(): variant -> message string ----
    out.frame("    #[allow(dead_code)]\n");
    out.frame(&format!("    impl {n}FrameEvent {{\n"));
    out.frame("        fn name(&self) -> &'static str {\n");
    out.frame("            match self {\n");
    for m in &sym.interface {
        out.frame(&format!(
            "                {n}FrameEvent::{} {{ .. }} => {:?},\n",
            pascal_event(&m.name),
            m.name
        ));
    }
    out.frame(&format!("                {n}FrameEvent::FrameEnter {{ .. }} => \"$>\",\n"));
    out.frame(&format!("                {n}FrameEvent::FrameExit {{ .. }} => \"<$\",\n"));
    out.frame("            }\n");
    out.frame("        }\n");
    out.frame("    }\n\n");

    // ---- L3 FrameValue (fixed hard-typed value enum) ----
    out.frame("    #[derive(Clone, Debug)]\n");
    out.frame("    #[allow(dead_code, non_camel_case_types)]\n");
    out.frame(&format!("    enum {n}FrameValue {{\n"));
    out.frame("        Int(i64),\n");
    out.frame("        Float(f64),\n");
    out.frame("        Bool(bool),\n");
    out.frame("        Str(String),\n");
    out.frame("        List(Vec<Self>),\n");
    out.frame("        Dict(alloc::collections::BTreeMap<String, Self>),\n");
    out.frame("    }\n\n");

    // ---- L3 FrameContext ----
    out.frame("    #[allow(dead_code, non_camel_case_types)]\n");
    out.frame(&format!("    struct {n}FrameContext {{\n"));
    out.frame(&format!("        event: alloc::rc::Rc<{n}FrameEvent>,\n"));
    out.frame(&format!("        _return: Option<{n}FrameReturn>,\n"));
    out.frame(&format!("        _data: alloc::collections::BTreeMap<String, {n}FrameValue>,\n"));
    out.frame("        _transitioned: bool,\n");
    out.frame("    }\n\n");
    out.frame(&format!("    impl {n}FrameContext {{\n"));
    out.frame(&format!(
        "        fn new(event: alloc::rc::Rc<{n}FrameEvent>, default_return: Option<{n}FrameReturn>) -> Self {{\n"
    ));
    out.frame("            Self {\n");
    out.frame("                event,\n");
    out.frame("                _return: default_return,\n");
    out.frame("                _data: alloc::collections::BTreeMap::new(),\n");
    out.frame("                _transitioned: false,\n");
    out.frame("            }\n");
    out.frame("        }\n");
    out.frame("    }\n\n");

    // ---- L4 per-state context structs (only for states whose UNIFIED context carries a field:
    //       state header params ++ `$>` enter params ++ state vars — RFC-0056) ----
    for st in &sym.states {
        let fields = state_ctx_fields(st);
        if fields.is_empty() {
            continue;
        }
        out.frame(kderive);
        out.frame(&format!("    struct {}Context {{\n", st.name));
        for (name, ty, _) in &fields {
            out.frame(&format!("        {name}: {ty},\n"));
        }
        out.frame("    }\n\n");
        out.frame(&format!("    impl Default for {}Context {{\n", st.name));
        out.frame("        fn default() -> Self {\n");
        out.frame("            Self {\n");
        for (name, _, seed) in &fields {
            out.frame(&format!("                {name}: {seed},\n"));
        }
        out.frame("            }\n");
        out.frame("        }\n");
        out.frame("    }\n\n");
    }

    // ---- L4 StateContext enum ----
    out.frame("    #[allow(dead_code, non_camel_case_types)]\n");
    out.frame(kderive);
    out.frame(&format!("    enum {n}StateContext {{\n"));
    for st in &sym.states {
        if state_ctx_fields(st).is_empty() {
            out.frame(&format!("        {},\n", st.name));
        } else {
            out.frame(&format!("        {}({}Context),\n", st.name, st.name));
        }
    }
    out.frame("        __NoContext,\n");
    out.frame("    }\n\n");
    out.frame(&format!("    impl Default for {n}StateContext {{\n"));
    out.frame("        fn default() -> Self {\n");
    let first_has_ctx = sym
        .states
        .iter()
        .find(|s| s.name == first)
        .map(|s| !state_ctx_fields(s).is_empty())
        .unwrap_or(false);
    if first_has_ctx {
        out.frame(&format!("            {n}StateContext::{first}({first}Context::default())\n"));
    } else {
        out.frame(&format!("            {n}StateContext::{first}\n"));
    }
    out.frame("        }\n");
    out.frame("    }\n\n");

    // ---- L4 Compartment ----
    out.frame("    #[allow(dead_code, non_camel_case_types)]\n");
    out.frame(kderive);
    out.frame(&format!("    struct {n}Compartment {{\n"));
    out.frame("        state: String,\n");
    out.frame(&format!("        state_context: {n}StateContext,\n"));
    out.frame(&format!("        forward_event: Option<{n}FrameEvent>,\n"));
    out.frame(&format!("        parent_compartment: Option<Box<{n}Compartment>>,\n"));
    out.frame("    }\n\n");
    out.frame(&format!("    impl {n}Compartment {{\n"));
    out.frame("        fn new(state: &str) -> Self {\n");
    out.frame("            let state_context = match state {\n");
    for st in &sym.states {
        if state_ctx_fields(st).is_empty() {
            out.frame(&format!("                {:?} => {n}StateContext::{},\n", st.name, st.name));
        } else {
            out.frame(&format!(
                "                {:?} => {n}StateContext::{}({}Context::default()),\n",
                st.name, st.name, st.name
            ));
        }
    }
    out.frame(&format!("                _ => {n}StateContext::__NoContext,\n"));
    out.frame("            };\n");
    out.frame("            Self {\n");
    out.frame("                state: state.to_string(),\n");
    out.frame("                state_context,\n");
    out.frame("                forward_event: None,\n");
    out.frame("                parent_compartment: None,\n");
    out.frame("            }\n");
    out.frame("        }\n");
    out.frame("    }\n\n");

    // ---- L5 the system struct ----
    out.frame("    #[allow(dead_code)]\n");
    let svis = if sym.private { "" } else { "pub " };
    out.frame(&format!("    {svis}struct {n}{lt} {{\n"));
    out.frame(&format!("        _state_stack: Vec<{n}Compartment>,\n"));
    out.frame(&format!("        __compartment: {n}Compartment,\n"));
    out.frame(&format!("        __next_compartment: Option<{n}Compartment>,\n"));
    out.frame(&format!("        _context_stack: Vec<{n}FrameContext>,\n"));
    for f in &sym.domain {
        out.frame(&format!("        pub {}: {},\n", f.name, domain_field_ty(f)));
    }
    out.frame("    }\n\n");

    // ---- L5 impl open: new + __create ----
    out.frame("    #[allow(non_snake_case)]\n");
    out.frame(&format!("    impl{lt} {n}{lt} {{\n"));

    let plist = if lt.is_empty() {
        Rust.param_list(&super::driver::ctor_params_text(&sym.params))
    } else {
        ctor_params_lt(&sym.params)
    };
    let ctor_args = super::driver::param_names(&super::driver::ctor_params_text(&sym.params));

    out.frame(&format!("        pub fn new({plist}) -> Self {{\n"));
    out.frame("            Self {\n");
    out.frame("                _state_stack: Vec::new(),\n");
    out.frame("                _context_stack: Vec::new(),\n");
    for f in &sym.domain {
        out.frame(&format!("                {}: {},\n", f.name, domain_field_init(f)));
    }
    out.frame(&format!("                __compartment: {n}Compartment::new({first:?}),\n"));
    out.frame("                __next_compartment: None,\n");
    out.frame("            }\n");
    out.frame("        }\n\n");

    out.frame(&format!("        pub fn __create({plist}) -> Self {{\n"));
    out.frame(&format!("            let mut c = Self::new({ctor_args});\n"));
    out.frame(&format!("            c.__compartment = c.__prepareEnter({first:?});\n"));
    out.frame(&format!("            let __e = alloc::rc::Rc::new({n}FrameEvent::FrameEnter {{}});\n"));
    out.frame(&format!(
        "            let __ctx = {n}FrameContext::new(alloc::rc::Rc::clone(&__e), None);\n"
    ));
    out.frame("            c._context_stack.push(__ctx);\n");
    out.frame("            c.__kernel(&__e);\n");
    out.frame("            c._context_stack.pop();\n");
    out.frame("            c\n");
    out.frame("        }\n\n");

    // ---- L6 __hsm_chain (arms via HsmChainWalk) ----
    out.frame("        fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str] {\n");
    out.frame("            match leaf {\n");
    out.frame(&super::hsm_chain_walk::walk(sym, be));
    out.frame("                _ => &[],\n");
    out.frame("            }\n");
    out.frame("        }\n\n");

    // ---- L6 __prepareEnter ----
    out.frame(&format!(
        "        fn __prepareEnter(&mut self, leaf: &str) -> {n}Compartment {{\n"
    ));
    out.frame("            let chain = self.__hsm_chain(leaf);\n");
    out.frame(&format!("            let mut comp: Option<{n}Compartment> = None;\n"));
    out.frame("            for name in chain.iter() {\n");
    out.frame(&format!("                let mut new_comp = {n}Compartment::new(name);\n"));
    out.frame("                if let Some(parent) = comp.take() {\n");
    out.frame("                    new_comp.parent_compartment = Some(Box::new(parent));\n");
    out.frame("                }\n");
    out.frame("                comp = Some(new_comp);\n");
    out.frame("            }\n");
    out.frame("            comp.expect(\"chain must contain at least the leaf state\")\n");
    out.frame("        }\n\n");

    // ---- L6 __kernel (fixed drain) ----
    out.frame(&format!(
        "        fn __kernel(&mut self, __e: &alloc::rc::Rc<{n}FrameEvent>) {{\n"
    ));
    out.frame("            // Route event to current state.\n");
    out.frame("            self.__router(__e);\n");
    out.frame("            // Drain any transitions queued by the handler.\n");
    out.frame("            while self.__next_compartment.is_some() {\n");
    out.frame("                let next_compartment = self.__next_compartment.take().expect(\"invariant: while-loop guard checked is_some()\");\n");
    out.frame("                // Exit the current (leaf) state. RFC-0025.1: exit args live in the\n");
    out.frame("                // source state's typed ctx (written at the transition site), so the\n");
    out.frame("                // synthesized `<$` event carries no payload.\n");
    out.frame(&format!(
        "                let exit_event = alloc::rc::Rc::new({n}FrameEvent::FrameExit {{}});\n"
    ));
    out.frame("                self.__router(&exit_event);\n");
    out.frame("                // Switch to the new compartment.\n");
    out.frame("                self.__compartment = next_compartment;\n");
    out.frame("                // Three-branch forward-event handling (RFC-0025 Track B.1: forward\n");
    out.frame("                // event is matched on enum variant; $> recognition is now a\n");
    out.frame("                // structural match, not a string compare).\n");
    out.frame("                match self.__compartment.forward_event.take() {\n");
    out.frame("                    None => {\n");
    out.frame("                        // No forwarded event — synthesize a fresh $>. RFC-0025.1:\n");
    out.frame("                        // enter args live in the destination's typed ctx.\n");
    out.frame(&format!(
        "                        let enter_event = alloc::rc::Rc::new({n}FrameEvent::FrameEnter {{}});\n"
    ));
    out.frame("                        self.__router(&enter_event);\n");
    out.frame("                    }\n");
    out.frame(&format!(
        "                    Some(fwd) if matches!(fwd, {n}FrameEvent::FrameEnter {{ .. }}) => {{\n"
    ));
    out.frame("                        // Forwarded event IS $> — dispatch directly so the\n");
    out.frame("                        // destination's $> handler receives the caller's payload.\n");
    out.frame("                        let fwd_rc = alloc::rc::Rc::new(fwd);\n");
    out.frame("                        self.__router(&fwd_rc);\n");
    out.frame("                    }\n");
    out.frame("                    Some(fwd) => {\n");
    out.frame("                        // Forwarded event is not $> — initialize the destination\n");
    out.frame("                        // with a fresh $>, then dispatch the forward.\n");
    out.frame(&format!(
        "                        let enter_event = alloc::rc::Rc::new({n}FrameEvent::FrameEnter {{}});\n"
    ));
    out.frame("                        self.__router(&enter_event);\n");
    out.frame("                        let fwd_rc = alloc::rc::Rc::new(fwd);\n");
    out.frame("                        self.__router(&fwd_rc);\n");
    out.frame("                    }\n");
    out.frame("                }\n");
    out.frame("                for ctx in self._context_stack.iter_mut() {\n");
    out.frame("                    ctx._transitioned = true;\n");
    out.frame("                }\n");
    out.frame("            }\n");
    out.frame("        }\n\n");

    // ---- L6 __router (arms via RouterWalk) ----
    out.frame(&format!(
        "        fn __router(&mut self, __e: &alloc::rc::Rc<{n}FrameEvent>) {{\n"
    ));
    out.frame(&format!("            let __ev: &{n}FrameEvent = __e;\n"));
    out.frame("            match self.__compartment.state.as_str() {\n");
    out.frame(&super::router_walk::walk(sym, be));
    out.frame("                _ => {}\n");
    out.frame("            }\n");
    out.frame("        }\n\n");

    // ---- L6 __transition ----
    out.frame(&format!(
        "        fn __transition(&mut self, next_compartment: {n}Compartment) {{\n"
    ));
    out.frame("            self.__next_compartment = Some(next_compartment);\n");
    out.frame("        }\n");
    // impl left OPEN — interface/dispatch/handler walks append; close_system closes.
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
/// `&'a dyn Tr`, and references nested in a generic (`Option<&str>` -> `Option<&'a str>`,
/// `Vec<&T>` -> `Vec<&'a T>`). A non-borrowed type is returned UNCHANGED, so this is the identity
/// on every owned domain field and every scalar ctor param — which keeps a borrow-free system
/// byte-identical.
///
/// framec inserts the lifetime token right after each `&`, but ONLY at PAREN depth 0: a reference
/// inside a fn-arg list (`&dyn Fn(&X) -> Atom`) elides to its own higher-ranked lifetime and must
/// be left alone, so `Fn(&X)`'s inner `&` is skipped. An already-annotated `&'x` is left as-is.
/// It never otherwise reads or rewrites the user's type text (type-ignorant). This is identical to
/// the old top-level-only threading for every case except a ref genuinely nested in a generic —
/// which never compiled before (a struct field cannot elide a lifetime), so nothing existing moves.
fn thread_lt(ty: &str) -> String {
    let chars: Vec<char> = ty.chars().collect();
    let mut out = String::new();
    let mut paren: i32 = 0;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' => {
                paren += 1;
                out.push('(');
                i += 1;
            }
            ')' => {
                paren -= 1;
                out.push(')');
                i += 1;
            }
            '&' if paren == 0 => {
                // Peek past whitespace: an already-lifetimed `&'x` is left untouched.
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '\'' {
                    out.push('&');
                    i += 1;
                } else {
                    // Insert `'a` and collapse the whitespace after `&` (matches the old trim_start).
                    out.push_str("&'a ");
                    i = j;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
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
