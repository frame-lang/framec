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
use super::driver::{param_names, Backend, BodyRole, LeafCtx};
use super::Sink;
use crate::resolve::SystemSym;
use crate::tree::body::{EmbedCall, FrameRef, RefKind};
use crate::NativeText;

pub struct Python;

impl Backend for Python {
    fn name(&self) -> &'static str {
        "python"
    }

    /// Frame writes `amount: int`; **Python writes exactly the same thing**. So the declaration is
    /// the name and — when the user annotated one — the user's type text, verbatim, reassembled
    /// from Frame's own `name: type` split. framec reorders; it never reads the type.
    ///
    /// It is not "names only" any more, and that is not a preference: the shipped compiler emits
    /// `def msg(self, a: str, b: int) -> int:`, and this backend's job in Milestone 1 is to emit
    /// what the shipped compiler emits.
    ///
    /// **An UNTYPED param is annotated `Any`** — `e1(u)` becomes `def e1(self, u: Any):`. This is
    /// the shipped compiler's rule and it is why its file preamble hard-codes
    /// `from typing import Any, …` ([`Self::file_header`]): `Any` is Python's explicit spelling of
    /// "no constraint", so an unannotated Frame param becomes an annotated Python one rather than
    /// a bare name, and every declaration in the emitted class is uniformly annotated.
    ///
    /// framec still reads nothing: `Any` is framec's own text supplied where the user wrote NO
    /// type, and a type the user DID write passes through verbatim, untrimmed of meaning. Verified
    /// against the 4.6.1 oracle at all three declaration sites this feeds — the interface wrapper
    /// ([`Self::route`]), the `_create` factory ([`Self::open_system`]), and an
    /// `actions:`/`operations:` member ([`Self::open_action`]) — which are exactly the three the
    /// oracle annotates. A private `(state, handler)` method takes `(self, __e, compartment)` and
    /// has no user params at all, so it is untouched.
    fn param_list(&self, params_text: &str) -> String {
        super::driver::params_split(params_text)
            .into_iter()
            .filter(|(n, _)| !n.is_empty())
            .map(|(n, t)| match t {
                Some(t) if !t.trim().is_empty() => format!("{n}: {}", t.trim()),
                _ => format!("{n}: Any"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The file preamble: the `typing` import, then ONE blank line before the first class. The
    /// system-prefixed runtime classes are per-SYSTEM and are emitted in [`Self::open_system`].
    fn file_header(&self, out: &mut Sink) {
        out.frame("from typing import Any, Optional, List, Dict, Callable\n\n");
    }

    fn open_system(&self, sym: &SystemSym, out: &mut Sink) {
        let n = &sym.name;
        let first = sym.states.first().map(|s| s.name.as_str()).unwrap_or("");
        let adef = if py_system_async(sym) { "async def" } else { "def" };
        let aw = if py_system_async(sym) { "await " } else { "" };
        // A MACHINELESS system (`operations:` only, no `machine:` section, or a machine with no
        // states) has no control state to construct: there is no start state to enter, so no
        // compartment, no drain, and nothing for the factory to run. Its methods are emitted
        // directly. The shipped compiler makes the same three cuts, and they are cuts rather than
        // guards because the alternative is a `_HSM_CHAIN[""]` KeyError at construction time.
        let machineless = sym.states.is_empty();

        // ---- Layer A — the three system-prefixed runtime classes. ----
        //
        // A LEAF, and provably so: two structurally different systems produce byte-identical text
        // here apart from the name substitution. There is no walk to reify, because there is
        // nothing in the tree it varies with.
        out.frame(&format!(
            "class {n}FrameEvent:\n\
             \x20   def __init__(self, message: str, parameters):\n\
             \x20       self._message = message\n\
             \x20       self._parameters = parameters\n\n\n"
        ));
        out.frame(&format!(
            "class {n}FrameContext:\n\
             \x20   def __init__(self, event: {n}FrameEvent, default_return):\n\
             \x20       self.event = event\n\
             \x20       self._return = default_return\n\
             \x20       self._data = {{}}\n\
             \x20       self._transitioned = False\n\n\n"
        ));
        out.frame(&format!(
            "class {n}Compartment:\n\
             \x20   def __init__(self, state: str, parent_compartment = None):\n\
             \x20       self.state = state\n\
             \x20       self.state_args = []\n\
             \x20       self.state_vars = {{}}\n\
             \x20       self.enter_args = []\n\
             \x20       self.exit_args = []\n\
             \x20       self.forward_event = None\n\
             \x20       self.parent_compartment = parent_compartment\n\n\
             \x20   def copy(self) -> '{n}Compartment':\n\
             \x20       c = {n}Compartment(self.state, self.parent_compartment)\n\
             \x20       c.state_args = self.state_args.copy()\n\
             \x20       c.state_vars = self.state_vars.copy()\n\
             \x20       c.enter_args = self.enter_args.copy()\n\
             \x20       c.exit_args = self.exit_args.copy()\n\
             \x20       c.forward_event = self.forward_event\n\
             \x20       return c\n\n\n"
        ));

        // ---- Layer B — the system class, its constructor, and the fixed kernel scaffolding. ----
        //
        // Constructor params — state, then enter, then domain (§203). The state/enter groups seed
        // the START compartment's positional arg lists; the domain group is seeded by the
        // `DomainInitWalk` machine below.
        //
        // **This is the D2 intentional divergence** (intentional_divergences.txt, owner ruling
        // 2026-07-24): the legacy oracle keeps `__init__(self)` param-free and binds header params
        // only in `_create`, so a plain `Sys(args)` is a `TypeError` and `Sys()` leaves the domain
        // unassigned. ng threads the params through `__init__` so BOTH entry points build a usable
        // object (validated by `legacy_bug_fixes.rs::plain_constructor_builds_a_usable_system`). Do
        // not "fix" this to the oracle's paramless form — that reverses a sanctioned divergence.
        let plist = self.param_list(&super::driver::ctor_params_text(&sym.params));
        let sig = if plist.is_empty() { String::new() } else { format!(", {plist}") };
        let ctor_args = param_names(&super::driver::ctor_params_text(&sym.params));
        let state_seed = sym
            .params
            .state
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let enter_seed = sym
            .params
            .enter
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        out.frame(&format!(
            "class {n}:\n\
             \x20   def __init__(self{sig}):\n\
             \x20       self._state_stack = []\n\
             \x20       self._context_stack = []\n"
        ));
        // The per-field domain seeds. The LOOP is framec's, reified as the `DomainInitWalk`
        // `@@system`; `domain_init` below is only the SPELLING.
        out.frame(&super::domain_init_walk::walk(sym, self));
        if !machineless {
            out.frame(&format!(
                "        self.__compartment = self.__prepareEnter(\"{first}\", [{state_seed}], [{enter_seed}])\n"
            ));
            // The START state's `$.x` seeds. (A destination state's are seeded where its compartment
            // is built — see `transition` / `push`. The shipped compiler instead prepends them to a
            // synthesized `$>` handler, which only runs via the `_create` factory; seeding at
            // construction is the same value delivered one step earlier, and it also survives a
            // plain `Sys()`.)
            out.frame(&py_state_var_seeds(sym, first, "        self.__compartment"));
            out.frame("        self.__next_compartment = None\n");
        }
        out.frame("\n");

        // The factory: construct, then run the start state's `$>` through the kernel. With no start
        // state there is no `$>` to run, so it is just the construction.
        if machineless {
            out.frame(&format!(
                "    @classmethod\n\
                 \x20   {adef} _create(cls{sig}):\n\
                 \x20       c = cls({ctor_args})\n\
                 \x20       return c\n\n"
            ));
        } else {
            out.frame(&format!(
                "    @classmethod\n\
                 \x20   {adef} _create(cls{sig}):\n\
                 \x20       c = cls({ctor_args})\n\
                 \x20       __e = {n}FrameEvent(\"$>\", c.__compartment.enter_args)\n\
                 \x20       __ctx = {n}FrameContext(__e, None)\n\
                 \x20       c._context_stack.append(__ctx)\n\
                 \x20       {aw}c.__kernel(__e)\n\
                 \x20       c._context_stack.pop()\n\
                 \x20       return c\n\n"
            ));
        }

        // `_HSM_CHAIN` — root..leaf per leaf state. The BRACES are the leaf; the ROWS are the
        // `HsmChainWalk` machine's (outer state cursor + inner ancestor climb).
        out.frame("    _HSM_CHAIN = {\n");
        out.frame(&super::hsm_chain_walk::walk(sym, self));
        out.frame("    }\n");
        out.frame(&format!(
            "    def __prepareEnter(self, leaf, state_args, enter_args):\n\
             \x20       comp = None\n\
             \x20       for name in self._HSM_CHAIN[leaf]:\n\
             \x20           new_comp = {n}Compartment(name)\n\
             \x20           new_comp.state_args = list(state_args)\n\
             \x20           new_comp.enter_args = list(enter_args)\n\
             \x20           new_comp.parent_compartment = comp\n\
             \x20           comp = new_comp\n\
             \x20       return comp\n\n"
        ));
        out.frame(
            "    def __prepareExit(self, exit_args):\n\
             \x20       comp = self.__compartment\n\
             \x20       while comp is not None:\n\
             \x20           comp.exit_args = list(exit_args)\n\
             \x20           comp = comp.parent_compartment\n\n",
        );
        // The KERNEL — route, then drain the transitions the handler queued. This is the piece that
        // makes `lifecycle_call` a no-op on this target: `$>`/`<$` are not calls a handler emits,
        // they are events the drain loop synthesizes from the compartment it just installed.
        out.frame(&format!(
            "    {adef} __kernel(self, __e):\n\
             \x20       # Route event to current state\n\
             \x20       {aw}self.__router(__e)\n\
             \x20       # Drain any transitions queued by the handler\n\
             \x20       while self.__next_compartment is not None:\n\
             \x20           next_compartment = self.__next_compartment\n\
             \x20           self.__next_compartment = None\n\
             \x20           # Exit the current (leaf) state\n\
             \x20           {aw}self.__router({n}FrameEvent(\"<$\", self.__compartment.exit_args))\n\
             \x20           # Switch to the new compartment\n\
             \x20           self.__compartment = next_compartment\n\
             \x20           if next_compartment.forward_event is None:\n\
             \x20               # No forwarded event \u{2014} synthesize a fresh $>\n\
             \x20               {aw}self.__router({n}FrameEvent(\"$>\", self.__compartment.enter_args))\n\
             \x20           else:\n\
             \x20               if next_compartment.forward_event._message == \"$>\":\n\
             \x20                   # Forwarded event IS $> \u{2014} dispatch directly so the\n\
             \x20                   # destination's $> receives the caller's payload\n\
             \x20                   {aw}self.__router(next_compartment.forward_event)\n\
             \x20               else:\n\
             \x20                   # Forwarded event is not $> \u{2014} initialize the destination\n\
             \x20                   # with a fresh $>, then dispatch the forward to it\n\
             \x20                   {aw}self.__router({n}FrameEvent(\"$>\", self.__compartment.enter_args))\n\
             \x20                   {aw}self.__router(next_compartment.forward_event)\n\
             \x20           next_compartment.forward_event = None\n\
             \x20           # Mark all stacked contexts as transitioned\n\
             \x20           for ctx in self._context_stack:\n\
             \x20               ctx._transitioned = True\n\n"
        ));

        // `__router` — the ARMS are the `RouterWalk` machine's (one per state, carrying the
        // `first` bit); the `def` line and the trailing blank are the leaf.
        out.frame(&format!("    {adef} __router(self, __e):\n"));
        out.frame(&super::router_walk::walk(sym, self));
        // No states, no arms — and an empty block is a SyntaxError in python, not a no-op. The
        // fact is read from the SYMBOL TABLE (`sym.states.is_empty()`), never from the text the
        // walk just wrote.
        if machineless {
            out.frame("        pass\n");
        }
        out.frame("\n");
        out.frame(
            "    def __transition(self, next_compartment):\n\
             \x20       self.__next_compartment = next_compartment\n",
        );
    }

    /// One state's message dispatcher — `_state_X`, the method `__router` hands an event to.
    ///
    /// The arms are the state's declared event messages, in declaration order, stamped by the
    /// `StateDispatchWalk` machine; this spells the match. The state's own PARAMS are bound first
    /// (from the live compartment's positional `state_args`), so a handler body that names them
    /// resolves. A state that declares nothing needs a `pass` — in python an empty block is a
    /// SyntaxError, not a no-op.
    fn dispatch(&self, sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
        let kw = if py_system_async(sym) { "async def" } else { "def" };
        let aw = if py_system_async(sym) { "await " } else { "" };
        out.frame(&format!("\n    {kw} _state_{state}(self, __e, compartment):\n"));
        let params = sym
            .states
            .iter()
            .find(|s| s.name == state)
            .map(|s| s.state_params.clone())
            .unwrap_or_default();
        for (i, p) in params.iter().enumerate() {
            out.frame(&format!(
                "        {p} = self.__compartment.state_args[{i}]\n"
            ));
        }
        // The state's DEFAULT FORWARD — `=> $^` written as a state MEMBER — is a fall-through at
        // the very END of the dispatcher: an event that matched no arm above goes to the parent's
        // dispatcher, one level up, with the parent's compartment. Two facts, both read from the
        // symbol table (`StateSym::default_forward`, set by the resolver from the
        // `StateMember::DefaultForward` node, and `declared_parent`); nothing is derived from
        // text. `=> $^` in a ROOT state forwards nowhere and emits nothing — matching the oracle,
        // which spells `pass` there because the block would otherwise be empty.
        let forward_to = sym
            .states
            .iter()
            .find(|s| s.name == state)
            .filter(|s| s.default_forward)
            .and_then(|_| sym.declared_parent(state))
            .map(|p| p.name.clone());
        if arms.is_empty() && params.is_empty() && forward_to.is_none() {
            out.frame("        pass\n");
            return;
        }
        // `arms` ARRIVE in the shipped compiler's handler-KEY order — the `StateDispatchWalk`
        // machine's `handler_slot` projection put them there, the same order the `EmitHandlers`
        // walk's `member_slot` gives the private handler METHODS, so the two lists agree by
        // construction. This spelling does not re-sort them: the order is a decision, and the
        // decision is not a backend's to make (it used to be made here, in one backend, for a
        // behavior legacy applies to all four).
        for msg in arms {
            out.frame(&format!(
                "        if __e._message == \"{msg}\":\n\
                 \x20           {aw}self.{}(__e, compartment)\n\
                 \x20           return\n",
                py_handler_method(state, msg)
            ));
        }
        if let Some(parent) = forward_to {
            out.frame(&format!(
                "        {aw}self._state_{parent}(__e, compartment.parent_compartment)\n"
            ));
        }
    }

    /// One `__router` arm. `first` decides `if` vs `elif` — a bit the `RouterWalk` machine CARRIES,
    /// so this spelling never asks "have I written an arm yet?" of the text it already emitted.
    fn router_arm(&self, sym: &SystemSym, state: &str, first: bool, out: &mut Sink) {
        let kw = if first { "if" } else { "elif" };
        let aw = if py_system_async(sym) { "await " } else { "" };
        out.frame(&format!(
            "        {kw} self.__compartment.state == \"{state}\":\n\
             \x20           {aw}self._state_{state}(__e, self.__compartment)\n"
        ));
    }

    /// One `_HSM_CHAIN` row: `"Leaf": ["Root", …, "Leaf"],`. The climb that produced `chain` is the
    /// `HsmChainWalk` machine's; this is only the Python literal.
    fn hsm_chain_entry(&self, leaf: &str, chain: &[String], out: &mut Sink) {
        let list = chain
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        out.frame(&format!("        \"{leaf}\": [{list}],\n"));
    }

    fn domain_init(&self, sym: &SystemSym, idx: usize, out: &mut Sink) {
        let Some(f) = sym.domain.get(idx) else { return };
        // Precedence:
        //  1. A field whose NAME matches a header domain param is seeded FROM THAT PARAM
        //     (`self.inventory = inventory`) — under the D2 model the param is a `__init__` arg and
        //     must win over the field's own block default, else the caller's value is dropped (#gap1,
        //     23_vending_machine). See `py_field_seed_param`.
        //  2. `= @@Inner()` is FRAME's instantiation syntax -> the `_create` FACTORY, not the bare
        //     `Inner()` constructor. `__init__` builds the object but does not run the start state's
        //     `$>`; the factory does, and Frame's `@@Sys(...)` always means "a fully constructed,
        //     entered instance". The top-level call site (`lower_instantiation`) already lowers
        //     `@@Sys(a)` to `Sys._create(a)`; a system-typed domain field is the SAME instantiation
        //     (#gap2, 20_multi_system_composition).
        //  3. Otherwise the user's native init expression, verbatim (or `None`).
        let init = if let Some(p) = py_field_seed_param(sym, &f.name) {
            p.to_string()
        } else {
            match &f.init_system {
                Some(s) => format!("{s}._create({})", super::ctor_init_args(f.init_text.as_deref())),
                None => f.init_text.clone().unwrap_or_else(|| "None".into()),
            }
        };
        out.frame(&format!("        self.{} = {init}\n", f.name));
    }

    /// Nothing. The last method's body is the last thing this system contributes; every method
    /// after the constructor block opens with its OWN leading blank line, so all inter-member
    /// separation is already accounted for and the system ends exactly where its last handler
    /// ended. (The file's final newline comes from the trailing water — the user's own bytes after
    /// the closing `}` — not from here.)
    fn close_system(&self, _sym: &SystemSym, _out: &mut Sink) {}

    fn return_type(&self, _t: Option<&str>) -> String {
        // Python does not declare one. Inventing an annotation would be framec pretending
        // to have a type system.
        String::new()
    }

    /// Python's async is on the `def`, not the return type. There is nothing to wrap.
    fn async_return_type(&self, _t: Option<&str>) -> String {
        String::new()
    }

    /// `@@:(<expr>)` / `@@:return(<expr>)` — set the CONTEXT's return slot.
    ///
    /// Not a Python `return`: the handler is called from the kernel, and the kernel still has a
    /// transition drain to run after it. The value is parked on the live context, and the public
    /// wrapper reads it back after the kernel returns (`return __frame_ctx._return`). This is also
    /// what makes `@@:return` READABLE in expression position — see [`Self::lower_ref`], and R4c.
    fn return_call(&self, role: BodyRole, rel: u32, _is_async: bool, multiline: bool, expr: NativeText, _ctx: &LeafCtx, out: &mut Sink) {
        let p = self.pad(rel);
        // An `actions:` / `operations:` member is an ORDINARY METHOD. The user may call it
        // directly — `vm.check_stock("cola")` — with no dispatch in flight, so
        // `self._context_stack` is EMPTY and the handler spelling would raise `IndexError` at the
        // `[-1]`. There is nothing to park a value on and nobody to read it back; the only correct
        // statement is Python's own `return`. This is also exactly what the shipped compiler emits
        // (`def check_stock(self, product: str) -> int: return …`), so it is a faithfulness fix and
        // a correctness fix at once. Guarded by
        // `tests/legacy_bug_fixes.rs::ng_bug_action_return_outside_dispatch`, which CALLS an action
        // from outside any dispatch and asserts the value comes back.
        if role == BodyRole::Action {
            out.frame(&format!("{p}return "));
            if multiline {
                // DELIBERATE DIVERGENCE from the shipped compiler, on the same ground as bug R2
                // above: legacy emits a bare `return a\n    + b` here, which is an
                // `IndentationError` — the emitted module does not even import. framec supplies
                // the implicit-continuation parens the `@@:( … )` syntax already implied. No
                // corpus fixture writes a multi-line `@@:()` inside an action, so this moves no
                // measured byte; it is the rule that keeps the emitted program valid when one
                // does.
                out.frame("(");
                out.native(expr);
                out.frame(")\n");
            } else {
                out.native(expr);
                out.frame("\n");
            }
            return;
        }
        if multiline {
            // *** bug R2. ***
            //
            // A multi-line native expression needs Python's implicit line-continuation
            // parens: without them the second line is `IndentationError: unexpected
            // indent`. The user wrote `@@:(a\n and b)` — those parens are exactly what
            // the `@@:(` … `)` syntax already implied, and what a native `x = (a\n and b)`
            // would have carried. framec supplies the same parens back.
            out.frame(&format!("{p}{RETURN_SLOT} = ("));
            out.native(expr);
            out.frame(")\n");
        } else {
            out.frame(&format!("{p}{RETURN_SLOT} = "));
            out.native(expr);
            out.frame("\n");
        }
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

    /// KERNEL reentrancy guard: after a `@@:self.<method>()` self interface call re-enters
    /// dispatch, bail if it queued a transition. The non-empty check guards a `$>`/`<$` lifecycle
    /// body (dispatched with no context pushed): an unconditional `[-1]` there is an `IndexError`
    /// at construction. Emitted at the call statement's own indent; the `return` one level deeper.
    fn reentrancy_guard(&self, rel: u32, _ctx: &LeafCtx, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!(
            "{p}if self._context_stack and self._context_stack[-1]._transitioned:\n{p}    return\n"
        ));
    }

    /// `=> $^` — hand this event to the PARENT state's DISPATCHER, with the compartment shifted
    /// up one level so `$.var` reads resolve in the parent's scope. Not the parent's handler
    /// method: the dispatcher is what the shipped compiler calls, and it is also what makes the
    /// parent's own state-var navigation land in the right compartment. `params` is not needed —
    /// every handler has the same fixed signature here and the event's arguments ride on `__e`.
    fn forward(&self, rel: u32, owner: &str, _event: &str, _params: &str, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}self._state_{owner}(__e, compartment.parent_compartment)\n"));
    }

    /// `=> $^` goes to the DECLARED parent, because [`Self::forward`] shifts the compartment by
    /// exactly one level and the callee has to match.
    ///
    /// **The decision is universal — see [`super::driver::Backend::forward_to_declared_parent`] for
    /// the four-target oracle evidence.** It is still opted into HERE only because Python is the one
    /// backend whose [`Self::forward`] already spells the legacy dispatcher call; java/rust/c spell
    /// the parent's handler method, so flipping the shared default regresses their ratchet by
    /// +44/+42/+61 lines without recovering a single legacy line. That doc comment carries the void
    /// condition; when it is met, delete this override rather than copying it three more times.
    fn forward_to_declared_parent(&self) -> bool {
        true
    }

    /// **`@@:(expr)` DOES NOT END THE BODY.** [`Self::return_call`] spells it as a return-SLOT
    /// assignment; the handler runs on and the caller reads the slot back after the kernel finishes.
    /// The statements after it must still be emitted — they RUN.
    ///
    /// **This is universal too — legacy emits the trailing statements on all four targets, in both
    /// body roles; see [`super::driver::Backend::return_call_terminates`] for the measurements.** It
    /// is opted into here only because java/rust/c do not yet spell the recovered statements the
    /// legacy way, so the shared flip costs them +1525/+1430/+1570 unmatched lines. Same void
    /// condition, same instruction: when it is met, delete this override.
    fn return_call_terminates(&self) -> bool {
        false
    }

    /// A body whose only content is a RECOGNIZED PYTHON COMMENT emits only the [`Self::noop`]
    /// (`pass`) — the comment itself is dropped.
    ///
    /// **This one is genuinely per-language, and the cross-target probes say so.** The shipped
    /// compiler's body was a list of statement SEGMENTS, and what counts as a segment is decided by
    /// that target's own lexer, so the same construct is a comment in one target and a statement in
    /// another. Measured against the 4.6.1 oracle on ONE system whose only handler body and only
    /// action body are a single comment line, compiled to all four targets, so the comment SPELLING
    /// is the only variable:
    ///
    /// | body text | python_3 | java | rust | c |
    /// |---|---|---|---|---|
    /// | `# only a comment` | **DROPPED**, bare `pass` | kept, `# only a comment;` | kept | kept |
    /// | `// only a comment` | kept, and NO `pass` | kept, `// only a comment;` | kept, `;` spliced | kept |
    ///
    /// Read the diagonal: legacy drops the text only when the TARGET's own lexer calls it a
    /// comment. Python's `#` is a comment there and reaches the emitter as nothing; java/rust/c
    /// treat `//` as an ordinary statement segment and emit it — provably, because they splice a
    /// `;` onto it (`// only a comment;`), which no comment-aware path would do. Give python a
    /// `//` body and it is kept verbatim; give java a `#` body and it is kept verbatim with a `;`.
    ///
    /// So the DEFAULT (`true`, keep the text) is right for java/rust/c and this override is right
    /// for python. Note also that comments in a NON-empty body survive on all four (probe
    /// `# c1` / `real_stmt()` / `# c2`) — the drop is specific to a body with ZERO statement segments.
    fn empty_body_keeps_text(&self) -> bool {
        false
    }

    /// `=> $^` from a state with no parent. Legacy's exact spelling, comment and all (two spaces
    /// before the `#`); a bare `pass` would be equivalent Python but different bytes.
    fn forward_no_parent(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!(
            "{}return  # Forward to parent (no parent)\n",
            self.pad(rel)
        ));
    }

    /// Python is indent-delimited: a no-op slot (e.g. a Frame construct that lowers to nothing,
    /// alone in an `if x:` block) must be a real `pass`, or the block is a syntax error.
    fn noop(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}pass\n", self.pad(rel)));
    }

    /// The PUBLIC interface method — Layer C.
    ///
    /// It builds a `FrameEvent` carrying the caller's arguments, pushes a `FrameContext`, runs the
    /// KERNEL, pops the context in a `finally`, and returns the context's return slot. Per-state
    /// dispatch is deliberately NOT here: that is `__router` -> `_state_X` ([`Self::dispatch`]), so
    /// the wrapper is uniform per event and ignores `arms` entirely. `arms` still arrives because
    /// the driver resolves it from the symbol table for the targets that DO dispatch here (Java,
    /// Rust, C), and one shared walk feeds them all.
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
        // The DECLARATION carries the user's annotations; the FrameEvent payload carries the bare
        // names, positionally (the handler reads them back off `__e._parameters`).
        let decl = self.param_list(params);
        let sig = if decl.is_empty() { String::new() } else { format!(", {decl}") };
        let names = param_names(params);
        // One `async` bit for the whole class: the kernel spine is shared, so it cannot be a
        // coroutine for one event and a plain function for another.
        let is_async = is_async || py_system_async(sym);
        let kw = if is_async { "async def" } else { "def" };
        let aw = if is_async { "await " } else { "" };
        // The declared return type is carried VERBATIM as a `-> T` annotation (`val(): int` ->
        // `def val(self) -> int:`). framec never reads inside it.
        let ann = ret
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| format!(" -> {t}"))
            .unwrap_or_default();
        // A DECLARED DEFAULT RETURN (`get_status(): str = "idle"`) seeds the context's return
        // slot before the kernel runs, so an event that no state handles still answers with the
        // declared value. It is a separate assignment, not the `FrameContext` ctor's
        // `default_return` argument, which stays `None` — that is the shipped compiler's shape.
        let seed = sym
            .interface
            .iter()
            .find(|m| m.name == event)
            .and_then(|m| m.default_return.as_deref())
            .map(|d| format!("        __ctx._return = {d}\n"))
            .unwrap_or_default();
        out.frame(&format!("\n    {kw} {event}(self{sig}){ann}:\n"));
        out.frame(&format!(
            "        __e = {n}FrameEvent(\"{event}\", [{names}])\n\
             \x20       __ctx = {n}FrameContext(__e, None)\n\
             {seed}\
             \x20       self._context_stack.append(__ctx)\n\
             \x20       try:\n\
             \x20           {aw}self.__kernel(__e)\n\
             \x20       finally:\n\
             \x20           __frame_ctx = self._context_stack.pop()\n\
             \x20       return __frame_ctx._return\n"
        ));
    }

    /// An action's signature carries the user's DECLARED TYPES, verbatim — `log(m: str)` becomes
    /// `def log(self, m: str):`, and `calc(a: int): int` becomes `def calc(self, a: int) -> int:`.
    ///
    /// It emitted bare names before, which silently DROPPED text the user wrote. Verbatim type
    /// passthrough is an architectural boundary, not a nicety: framec has no type system, so a
    /// declared type is the user's target-language text and the only correct thing to do with it
    /// is carry it. [`Self::route`] already did this for interface methods; actions were the one
    /// place that did not, so the same `name: type` reassembly is used here.
    ///
    /// **An action is a PLAIN METHOD, and nothing else.** No `compartment = self.__compartment`
    /// preamble: an action has no state, is not dispatched, and may be called by the user directly
    /// — binding a compartment there is both unused and a lie about where the method runs (worse,
    /// it would raise if the system were not yet constructed). The leading `\n` is the separator
    /// BEFORE the method, matching [`Self::open_handler`], so the blank line belongs to the member
    /// that follows rather than trailing off the last one.
    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, out: &mut Sink) {
        let decl = self.param_list(params);
        let sig = if decl.is_empty() { String::new() } else { format!(", {decl}") };
        let ann = ret
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| format!(" -> {t}"))
            .unwrap_or_default();
        out.frame(&format!("\n    def {name}(self{sig}){ann}:\n"));
    }

    /// Nothing. An action's body ends where the user's last statement ends: there is no return
    /// slot to flush and no kernel to hand control back to, so a synthesized trailing `return`
    /// would be pure noise (and, after a user `return`, dead). The separator before the NEXT
    /// member is emitted by [`Self::open_action`].
    fn close_action(&self, _out: &mut Sink) {}

    /// One private `(state, handler)` method — Layer E.
    ///
    /// The signature is FIXED: `(self, __e, compartment)`. An event's own parameters do NOT ride on
    /// the signature — they ride on `__e._parameters`, because the caller of this method is the
    /// state dispatcher, which has only the event. Frame's two lifecycle messages read their
    /// payload from the COMPARTMENT instead (`enter_args` / `exit_args`), because the kernel
    /// synthesizes those events from the compartment it installed.
    ///
    /// Bindings are emitted in one fixed order — the state's own params, then the event's — so a
    /// body that names either resolves. The leading blank line separates this method from the
    /// previous one.
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
        let is_async = is_async || py_system_async(sym);
        let kw = if is_async { "async def" } else { "def" };
        out.frame(&format!(
            "\n    {kw} {}(self, __e, compartment):\n",
            py_handler_method(state, event)
        ));
        for (i, p) in sym
            .states
            .iter()
            .find(|s| s.name == state)
            .map(|s| s.state_params.clone())
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            out.frame(&format!("        {p} = compartment.state_args[{i}]\n"));
        }
        let slot = match event {
            "$>" => "compartment.enter_args",
            "<$" => "compartment.exit_args",
            _ => "__e._parameters",
        };
        for (i, (name, _ty)) in super::driver::params_split(params)
            .into_iter()
            .filter(|(n, _)| !n.is_empty())
            .enumerate()
        {
            out.frame(&format!("        {name} = {slot}[{i}]\n"));
        }
    }

    /// Nothing. A transition body already ended with its own `return`; a `@@:(expr)` body ended
    /// with the return-slot assignment; and there is no fallback return to add, because the value
    /// a caller sees comes from the CONTEXT, not from this method. Inter-method separation is the
    /// NEXT method's leading blank line.
    ///
    /// The one case that would leave an illegal empty block — a body with nothing to emit — is not
    /// this method's to fix: the driver reads it off the TREE (`body_is_empty`) and asks for a
    /// `noop`, which python spells `pass`.
    fn close_handler(&self, _ret: Option<&str>, _is_async: bool, _terminated: bool, _ctx: &LeafCtx, _out: &mut Sink) {}

    /// **In Python the indent IS the syntax.** A `@@:return` inside an `if x:` must be
    /// indented under it, or the file is a SyntaxError. Nothing else in the compiler
    /// knows that, and nothing else needs to.
    fn pad(&self, rel: u32) -> String {
        format!("        {}", " ".repeat(rel as usize))
    }

    fn native_stmt(&self, rel: u32, text: NativeText, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&self.pad(rel));
        out.native(text);
        out.frame("\n");
    }

    /// `-> $Target(state_args)`, no enter payload. Delegates to the enter-aware form so the two
    /// spellings cannot drift.
    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        self.transition_with_enter(rel, sym, target, args, None, out);
    }

    /// `-> (enter_args) $Target(state_args)` — Layer F.
    ///
    /// Build the destination compartment through the `__prepareEnter` factory (which walks the HSM
    /// chain) and QUEUE it on the kernel. The kernel drives exit and enter later, from the
    /// compartment — which is why this backend's [`Self::lifecycle_call`] emits no enter call at
    /// all. `terminate` adds the `return`.
    fn transition_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        let p = self.pad(rel);
        out.frame(&format!(
            "{p}__compartment = self.__prepareEnter(\"{target}\", [{}], [{}])\n",
            py_args(args),
            py_args(enter_args)
        ));
        out.frame(&py_state_var_seeds(sym, target, &format!("{p}__compartment")));
        out.frame(&format!("{p}self.__transition(__compartment)\n"));
    }

    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink) {
        self.push_with_enter(rel, sym, target, args, None, out);
    }

    /// `push$ -> (enter_args) $Target(state_args)` — remember the live compartment, then transition.
    fn push_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        out.frame(&format!(
            "{}self._state_stack.append(self.__compartment)\n",
            self.pad(rel)
        ));
        self.transition_with_enter(rel, sym, target, args, enter_args, out);
    }

    /// `-> pop$` — restore the remembered compartment by QUEUEING it, exactly as a transition
    /// queues a freshly built one, so the kernel runs the same exit/enter drain over it.
    fn pop(&self, rel: u32, out: &mut Sink) {
        let p = self.pad(rel);
        out.frame(&format!("{p}__saved = self._state_stack.pop()\n"));
        out.frame(&format!("{p}self.__transition(__saved)\n"));
    }

    fn push_bare(&self, rel: u32, out: &mut Sink) {
        // Push the current compartment; stay. (Deep copy deferred — corpus pushes then pops.)
        out.frame(&format!(
            "{}self._state_stack.append(self.__compartment)\n",
            self.pad(rel)
        ));
    }

    fn pop_bare(&self, rel: u32, out: &mut Sink) {
        out.frame(&format!("{}self._state_stack.pop()\n", self.pad(rel)));
    }

    /// **A no-op for `$>` on this target, and that is the model, not an omission.**
    ///
    /// A handler does not CALL the destination's enter handler here. It queues a compartment; the
    /// kernel's drain loop then synthesizes `$>` (and `<$`) from that compartment and routes them
    /// like any other event, so the enter payload travels on `compartment.enter_args` — delivered
    /// by `__prepareEnter` in [`Self::transition_with_enter`].
    ///
    /// `<$` is the one thing left to spell: the EXIT payload has no compartment of its own to ride
    /// on, so it is stamped onto the live compartment chain (`__prepareExit`) before the transition
    /// queues its successor, and the kernel reads it back when it synthesizes `<$`.
    fn lifecycle_call(&self, rel: u32, _sym: &SystemSym, _state: &str, event: &str, args: Option<&str>, out: &mut Sink) {
        if event != "<$" {
            return;
        }
        // `__prepareExit` exists to DELIVER A PAYLOAD: it stamps the exit args down the
        // compartment chain for the `<$` handler to read. The exit itself is the kernel's job
        // (its drain routes a synthesized `<$` after the transition is queued), so a transition
        // that supplies no args has nothing to prepare, and the shipped compiler emits no call —
        // even when the source state DOES declare a `<$` handler. Verified on the 4.6.1 oracle:
        // `("bye") -> $B` emits `self.__prepareExit(["bye"])`; a bare `-> $B` from the same state
        // emits nothing.
        let Some(a) = args else { return };
        out.frame(&format!(
            "{}self.__prepareExit([{}])\n",
            self.pad(rel),
            py_args(Some(a))
        ));
    }

    /// `-> (enter) pop$` — the enter payload for the RESTORED state. No runtime state dispatch is
    /// needed (which is what the old spelling did): the restored compartment is right there in
    /// `__saved`, so the payload is stamped straight onto it. `__transition` has already stored a
    /// REFERENCE to that compartment, and the kernel reads `enter_args` when it later synthesizes
    /// the `$>`, so stamping after the queue call is the same object either way.
    fn pop_enter(&self, rel: u32, _sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink) {
        out.frame(&format!(
            "{}__saved.enter_args = [{}]\n",
            self.pad(rel),
            py_args(enter_args)
        ));
    }

    fn terminate(&self, rel: u32, _ctx: &LeafCtx, out: &mut Sink) {
        out.frame(&format!("{}return\n", self.pad(rel)));
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
                // #bug-R1: honor the statement's re-indent `p` (pad(rel)) like ContextSelf and
                // the `_` fallback — a hardcoded 8-space method-base prefix put a `$.x = …` nested
                // in a native block at the wrong column (IndentationError in Python).
                out.frame(&format!(
                    "{p}compartment.state_vars[\"{}\"] = ",
                    lhs.name
                ));
                out.native(rhs);
                out.frame("\n");
            }
            RefKind::ContextData => {
                // `@@:data.k` is the EVENT's scratch map, which lives on the live context — not on
                // the compartment (that is `$.x`, the state's own storage, above).
                out.frame(&format!("{p}{DATA_SLOT}[\"{}\"] = ", lhs.name));
                out.native(rhs);
                out.frame("\n");
            }
            RefKind::ContextReturn => {
                // SET the slot; do NOT exit. `@@:return = x` parks a value the wrapper reads back
                // after the kernel finishes — the handler keeps running (and so does the drain).
                out.frame(&format!("{p}{RETURN_SLOT} = "));
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

    /// `@@Sys(a, b)` lowers to the FACTORY, `Sys._create(a, b)` — never `Sys(a, b)`.
    ///
    /// `__init__` only builds the object; it is `_create` that runs the start-state's `$>`
    /// through the kernel. A bare `Sys(...)` therefore hands back a machine that never
    /// entered its initial state. This is the shipped compiler's spelling for every
    /// instantiation site — top-level water AND a system-typed domain field
    /// (`inner: Inner = @@Inner(7)` → `self.inner = Inner._create(7)`).
    fn system_ctor_call(&self, name: &str, args: &[String]) -> Atom {
        Atom::call(format!("{name}._create"), args.join(", "))
    }

    fn embed_call(&self, _sym: &SystemSym, ec: &EmbedCall) -> Atom {
        // An EMPTY field is a bare self-call `@@:self.method(...)` embedded in an expression
        // (bug R3): the receiver is `self`, not `self.<field>`. `self.<field>` would spell
        // `self..method(...)`.
        if ec.field.is_empty() {
            return Atom::call(format!("self.{}", ec.method), &ec.args);
        }
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
            RefKind::ContextData => Atom::index(Atom::ident(DATA_SLOT), format!("\"{}\"", r.name)),
            RefKind::ContextSelf => Atom::field(Atom::ident("self"), &r.name),
            RefKind::ContextParams => Atom::ident(&r.name),
            RefKind::ContextSystemState => Atom::field(comp, "state"),
            // *** R4c. *** `@@:return` in EXPRESSION position is a GETTER, and it now has
            // something to read: the context's return slot. Before the kernel model there was no
            // slot — a value-returning handler emitted `return <expr>` immediately — so this arm
            // degraded to the bare `return` KEYWORD and produced `self.echo(return)`, a
            // SyntaxError. The slot is the fix; the spelling follows it.
            RefKind::ContextReturn => Atom::ident(RETURN_SLOT),
            // `Unknown` (Δ5) is error-blocked by `validate` (E408) before emission; degrade
            // gracefully rather than panic on any direct-emit path.
            RefKind::ContextEvent | RefKind::SelfCall | RefKind::Unknown => Atom::ident(&r.name),
        }
    }

    fn persist(&self, m: &super::persist::PersistManifest, out: &mut Sink) {
        use super::persist::{SYS, TAG, VAL};
        let fields: Vec<&str> = m.fields.iter().map(|(n, _)| n.as_str()).collect();
        let schema = m.schema();
        // The runtime compartment class is a MODULE-level, system-prefixed class now
        // (`<Sys>Compartment`), not a nested `type(self).Compartment` — see `open_system`, Layer A.
        // And the pushdown stack is `_state_stack`, a plain attribute, so it is NOT name-mangled.
        let comp_cls = format!("{}Compartment", m.sys);
        // The closed world of sub-system types, as a Python literal set. A domain value whose
        // runtime class name is in here is a system and is framed via `@f:s` (factory-rebuild),
        // never walked reflectively. `set()` (not `{}`, which is a dict) when the program
        // declares none.
        let sys_set = if m.systems.is_empty() {
            "set()".to_string()
        } else {
            format!(
                "{{{}}}",
                m.systems
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        // ---- snapshot() ----
        out.frame(&format!("\n    def {}(self):\n", m.save));
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
        out.frame("            \"_stack\": [_comp(_c) for _c in self._state_stack],\n");
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
        // A SUB-SYSTEM value (declared type is a sibling `@@system`). Frame it out-of-band via
        // `@f:s` and record its control state STRUCTURALLY (compartment + stack, as plain
        // dicts) plus its own domain fields — NOT the reflective `@f:t` walk, which would tag
        // the nested `<Sys>.Compartment` and fail to resolve it on restore, and would violate
        // the factory-only contract. Detection is exact: the runtime class name is a declared
        // system (a user type cannot shadow one). The compartment/stack attributes are private
        // (name-mangled) on the sub-system, so reach them by their mangled spelling.
        out.frame(&format!("            _frame_systems = {sys_set}\n"));
        out.frame("            _cn = type(o).__name__\n");
        out.frame("            if _cn in _frame_systems:\n");
        out.frame("                _ca = \"_\" + _cn.lstrip(\"_\") + \"__compartment\"\n");
        // The compartment is private (name-mangled) on the sub-system; the pushdown stack and the
        // context stack are plain attributes. All three are CONTROL state, not domain, so all three
        // are excluded from the reflective `_domain` walk below.
        out.frame("                _sa = \"_state_stack\"\n");
        out.frame("                _xa = \"_context_stack\"\n");
        out.frame("                _na = \"_\" + _cn.lstrip(\"_\") + \"__next_compartment\"\n");
        out.frame(&format!(
            "                return {{{SYS:?}: _cn, \"_control\": _comp(getattr(o, _ca)), \"_stack\": [_comp(_c) for _c in getattr(o, _sa, [])], \"_domain\": {{k: _enc(v) for k, v in (getattr(o, \"__dict__\", None) or {{}}).items() if k not in (_ca, _sa, _xa, _na)}}}}\n"
        ));
        // A user-defined instance: tag it out-of-band, recurse its fields into VAL.
        out.frame("            _f = dict(getattr(o, \"__dict__\", None) or {})\n");
        out.frame(&format!(
            "            return {{{TAG:?}: type(o).__qualname__, {VAL:?}: {{k: _enc(v) for k, v in _f.items()}}}}\n"
        ));
        out.frame("        return json.dumps(_enc(_state))\n\n");

        // ---- restore() ----
        out.frame(&format!("\n    def {}(self, data):\n", m.load));
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
        // A `@f:s` sub-system envelope. Resolve the system name in the CLOSED WORLD (systems
        // are top-level classes, so they are in `_known`; a foreign name is refused — E750),
        // blank-allocate via the factory (`__new__`, never a user-arg ctor), and rebuild its
        // control state through the system's OWN compartment class. Its domain fields recurse
        // back through `_dec` (a nested sub-system recurses right here).
        out.frame(&format!(
            "            if {SYS:?} in o and isinstance(o.get({SYS:?}), str):\n"
        ));
        out.frame(&format!("                _cn = o[{SYS:?}]\n"));
        out.frame("                _cls = _known.get(_cn)\n");
        out.frame("                if _cls is None:\n");
        out.frame("                    raise RuntimeError(\"E750: persist restore cannot resolve type: \" + repr(_cn))\n");
        out.frame("                _inst = _cls.__new__(_cls)\n");
        out.frame("                _ca = \"_\" + _cn.lstrip(\"_\") + \"__compartment\"\n");
        out.frame("                _sa = \"_state_stack\"\n");
        // The sub-system's compartment class is its own module-level `<Sub>Compartment`. Resolved
        // in the SAME closed world as the system itself — a name this program does not define
        // cannot be reached (the E750 floor), so a blob cannot smuggle in a foreign class here
        // either.
        out.frame("                _cc = _known.get(_cn + \"Compartment\")\n");
        out.frame("                if _cc is None:\n");
        out.frame("                    raise RuntimeError(\"E750: persist restore cannot resolve type: \" + repr(_cn + \"Compartment\"))\n");
        out.frame("                setattr(_inst, _ca, _rebuild_c(_cc, o[\"_control\"]))\n");
        out.frame("                setattr(_inst, _sa, [_rebuild_c(_cc, _c) for _c in o.get(\"_stack\", [])])\n");
        out.frame("                setattr(_inst, \"_context_stack\", [])\n");
        out.frame("                setattr(_inst, \"_\" + _cn.lstrip(\"_\") + \"__next_compartment\", None)\n");
        out.frame("                for _k, _v in o.get(\"_domain\", {}).items():\n");
        out.frame("                    setattr(_inst, _k, _dec(_v))\n");
        out.frame("                return _inst\n");
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
        // Rebuild a compartment of a GIVEN class — `type(self).Compartment` for this system's
        // own control state, or a sub-system's `Compartment` when `_dec` revives a `@f:s`
        // envelope. Allocate fresh and repopulate state_vars/state_args (decoding each value)
        // rather than reassign a state name onto a constructed compartment.
        out.frame("        def _rebuild_c(_cc, d):\n");
        out.frame("            _c = _cc(d[\"state\"])\n");
        out.frame("            _c.state_vars = {_k: _dec(_v) for _k, _v in d.get(\"state_vars\", {}).items()}\n");
        // `state_args` is a POSITIONAL list on this runtime (a state's params are bound by index,
        // not by name), so it round-trips as a list.
        out.frame("            _c.state_args = [_dec(_v) for _v in d.get(\"state_args\", [])]\n");
        out.frame("            return _c\n");
        out.frame(&format!(
            "        self.__compartment = _rebuild_c({comp_cls}, _raw[\"_control\"])\n"
        ));
        out.frame(&format!(
            "        self._state_stack = [_rebuild_c({comp_cls}, _d) for _d in _raw.get(\"_stack\", [])]\n"
        ));
        for f in &fields {
            out.frame(&format!("        self.{f} = _dec(_raw[{f:?}])\n"));
        }
        out.frame("        return self\n\n");
    }
}

/// The context's RETURN slot — where a handler parks the value its caller will get. It is the
/// live context (`[-1]`), not the system, because events NEST: a `@@:self.other()` re-entry pushes
/// its own context and must not clobber the outer call's answer.
const RETURN_SLOT: &str = "self._context_stack[-1]._return";

/// The context's `@@:data` scratch map, scoped to the event in flight for the same reason.
const DATA_SLOT: &str = "self._context_stack[-1]._data";

/// `Some("a, b")` -> `a, b`; `None` or blank -> "". The arg blob framec was handed, placed between
/// the brackets of a positional arg LIST.
///
/// Verbatim in the ordinary case: framec did not write those commas or that spacing and does not
/// reformat them. The ONE intervention is Frame's NAMED-argument form, `-> $B(1, 2, k=3)`: `k=3`
/// is legal inside a call but is a SyntaxError inside a list literal, and the destination's args
/// are a positional list on this runtime. So a named element contributes its VALUE, positionally —
/// which is what the shipped compiler emits (`[1, 2, 3]`).
///
/// **Recorded gap**, and it is the shipped compiler's too: this keeps the argument's POSITION, not
/// its NAME. `-> $B(k=3, x=1)` against `$B(x, y, k)` therefore binds by order, not by name. Doing
/// it properly means matching named args against the declared state params the way
/// [`super::driver::lower_instantiation`] already does for `@@Sys(...)` — which needs the state's
/// arg list carried as structured `InstArg`s in the tree, not as one opaque `ArgExpr`.
fn py_args(args: Option<&str>) -> String {
    let Some(t) = args.map(str::trim).filter(|a| !a.is_empty()) else {
        return String::new();
    };
    let parts = crate::text::scan::param_scan::parse_decl(t.as_bytes());
    // Nothing named: hand back exactly what the user wrote.
    if parts.iter().all(|(_g, b)| named_arg_value(b).is_none()) {
        return t.to_string();
    }
    parts
        .iter()
        .map(|(_g, b)| named_arg_value(b).unwrap_or_else(|| b.trim().to_string()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `k = 3` -> `Some("3")`; anything else -> `None`.
///
/// Deliberately NARROW. It recognizes only Frame's own named-argument shape — a BARE IDENTIFIER,
/// optional whitespace, a single `=` that is not part of `==`/`!=`/`<=`/`>=`/`+=`/… — so an
/// ordinary argument EXPRESSION cannot be mangled by it. (`a == b`, `x >= 1`, `f(k=1)` and
/// `p.q = 1` all fall through to `None` and are carried verbatim.) It is framec reading framec's
/// own syntax, not the user's expression: the `(...)` after `$B` is Frame's.
fn named_arg_value(body: &str) -> Option<String> {
    let b = body.trim();
    let bytes = b.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    // A name, and it must not start with a digit (that is a numeric literal, not a param name).
    if i == 0 || bytes[0].is_ascii_digit() {
        return None;
    }
    let mut j = i;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if bytes.get(j) != Some(&b'=') {
        return None;
    }
    // `==` is a comparison, not a binding.
    if bytes.get(j + 1) == Some(&b'=') {
        return None;
    }
    let v = b[j + 1..].trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// The `$.x` seeds for `state`, written onto the compartment named by `recv` (which carries its own
/// indentation prefix). Empty when the state declares no state vars — the common case, and the one
/// the M1 byte target exercises.
///
/// The shipped compiler prepends these to a SYNTHESIZED `$>` handler instead, which only runs when
/// the instance is built through the `_create` factory. Seeding where the compartment is BUILT
/// delivers the same values one step earlier and also survives a plain `Sys()`. (Recorded as a
/// deliberate divergence: matching it exactly needs a desugaring pass that synthesizes a handler
/// into the tree, which every other backend would then see.)
/// The header DOMAIN param that SEEDS the domain field named `field` — the param whose name equals
/// the field's, if any. Under the D2 construction model (`__init__(self, <params>)`) this param is
/// in scope at construction, so the field must be bound FROM IT (`self.<field> = <param>`), not from
/// its own block default. Without this, a header param whose matching field carries an independent
/// default (`inventory: dict = {}` beside header `inventory: dict = {}`) is silently dropped — the
/// field keeps `{}` and the caller's `@@Sys(inventory=…)` value never lands (#gap1). The
/// init-REFERENCE form (`value = initial`) needs no help: the field's init expression already IS the
/// param name, so `DomainInitWalk` binds it. This is only the NAME-MATCH form.
fn py_field_seed_param<'a>(sym: &'a SystemSym, field: &str) -> Option<&'a str> {
    sym.params
        .domain
        .iter()
        .find(|p| p.name == field)
        .map(|p| p.name.as_str())
}

fn py_state_var_seeds(sym: &SystemSym, state: &str, recv: &str) -> String {
    let Some(st) = sym.states.iter().find(|s| s.name == state) else {
        return String::new();
    };
    st.state_vars
        .iter()
        .map(|v| format!("{recv}.state_vars[\"{}\"] = {}\n", v.name, py_state_seed(v)))
        .collect()
}

/// **Retired, and deliberately kept as an empty string.**
///
/// The `_seed_args` varargs helper existed to splat a transition's state-arg blob into a
/// dict-shaped compartment. The compartment's `state_args` is a positional LIST now, seeded by
/// `__prepareEnter`, so there is nothing to splat and no module-level helper to emit — and no
/// python name-mangling hazard to route around either.
///
/// It stays as a `pub const` because it is part of this module's published surface and callers
/// concatenate it ahead of the emitted code; an empty string keeps every one of them correct.
pub const PRELUDE: &str = "";


/// The seed value for a state var: `= @@Sub()` -> `Sub()` (Frame's instantiation syntax),
/// else the user's init verbatim, else `None`.
fn py_state_seed(v: &crate::resolve::FieldSym) -> String {
    match &v.init_system {
        Some(s) => format!("{s}({})", super::ctor_init_args(v.init_text.as_deref())),
        None => v
            .init_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("None")
            .to_string(),
    }
}

/// Is this system's generated class ASYNC — i.e. does the kernel chain (`__kernel`, `__router`,
/// `_state_X`, every handler and every public wrapper) have to be `async def`?
///
/// One bit for the whole class, not one per method, and that is forced by the SHAPE of the runtime:
/// every public method funnels through the same `__kernel`/`__router`, so a single `async` member
/// makes that shared spine a coroutine, and a spine that is a coroutine can only be awaited. Mixing
/// would mean `_state_X` awaiting a plain `def` — a `TypeError` at the first event. `@@[async]`
/// already sets `sym.is_async` for every member; this also catches a per-member `async fetch()`
/// declared without it.
fn py_system_async(sym: &SystemSym) -> bool {
    sym.is_async || sym.interface.iter().any(|m| m.is_async)
}

/// The private method name for one `(state, event)` handler — `_s_<state>_hdl_user_<event>` for an
/// interface event, `_s_<state>_hdl_frame_enter` / `_hdl_frame_exit` for Frame's own lifecycle
/// messages. framec AUTHORED this name, so framec may compose it; nothing ever reads it back out of
/// emitted text (that is the wire-format-as-a-name mistake — the dispatcher is handed the message
/// and composes the same name from the same rule, here, once).
fn py_handler_method(state: &str, event: &str) -> String {
    match event {
        "$>" => format!("_s_{state}_hdl_frame_enter"),
        "<$" => format!("_s_{state}_hdl_frame_exit"),
        other => format!("_s_{state}_hdl_user_{other}"),
    }
}


