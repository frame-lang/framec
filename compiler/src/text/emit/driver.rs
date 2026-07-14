//! The shared emitter. **One walk of the tree, for every target.**
//!
//! # The rule, and how it is enforced
//!
//! > **This module does not have the target language.** It cannot branch on it.
//!
//! Not "must not" — *cannot*. [`emit`] takes a `&dyn Backend` and never a `Target`, so
//! `match lang { … }` will not compile here. That is the same trick as the text wall:
//! make the wrong thing unrepresentable rather than forbidden.
//!
//! # Why this matters more than it looks
//!
//! The old compiler had **seventeen hand-written arms** for nearly every decision, and
//! they drifted. Not occasionally — *systematically*:
//!
//! * The `$.x` reader existed **twice**, and the two copies disagreed four ways —
//!   C# parenthesization, the interpolation quote-swap, the C cast, and a latent Go
//!   field-name divergence. Three of the four were shipped bugs.
//! * The C `void*` unpack was written out **three times**, two of which bypassed the
//!   very module whose stated purpose was that "pack and unpack cannot drift."
//! * Porting one feature (borrowed input) to sixteen backends produced **six identical
//!   mistakes** — the same error made independently in Rust, Java, Go and C++.
//!
//! Sixteen arms are sixteen chances to be wrong, and a reviewer who checks fifteen of
//! them has still shipped a bug. A **table** is one chance.
//!
//! # What a backend supplies, and what it does not
//!
//! A [`Backend`] supplies **spellings** — how *this* language writes a class, a method,
//! an assignment. It does **not** supply control flow, and it never sees the tree walk.
//! If two backends need different *structure*, that is a signal the structure is wrong,
//! not that the backend needs an escape hatch.

use super::atom::Atom;
use super::Sink;
use crate::resolve::{SymbolTable, SystemSym};
use crate::text::Source;
use crate::tree::body::{Body, FrameRef, Stmt};
use crate::tree::{Decl, FileAst, Item, MachineMember, Section, StateMember};

/// What a target language must be able to spell.
///
/// Every method here is a **spelling**, never a decision. The decisions live in
/// [`emit`], once.
pub trait Backend {
    /// e.g. `java`, `python`.
    fn name(&self) -> &'static str;

    /// The FILE's preamble — imports, `using`, `require`. Emitted **once, at the very
    /// top**, before any item.
    ///
    /// It cannot live in `open_system`: the user's native code (the water) may precede
    /// the system, and Java requires imports before any class declaration. Emitting them
    /// with the class put `import java.util.*;` after the user's `class Cache {}`, which
    /// javac rejects.
    ///
    /// Note framec does NOT decide this by scanning its own output for a `package` line —
    /// which is precisely what the old compiler's assembler did, searching for a line that
    /// (in Go) never existed.
    fn file_header(&self, out: &mut Sink);

    /// The class opening.
    fn open_system(&self, sym: &SystemSym, out: &mut Sink);
    /// Close the class.
    fn close_system(&self, sym: &SystemSym, out: &mut Sink);

    /// How this language spells a **parameter list declaration**.
    ///
    /// Frame writes `amount: int`. Java wants `int amount`; Go wants `amount int`; Rust
    /// wants `amount: i32`; Python wants just `amount`.
    ///
    /// Note what framec is and is not doing here. The `name: type` form is **Frame's own
    /// syntax** — framec owns that colon, so splitting on it is RULE 1-clean. The TYPE
    /// text on the right is the **user's**, and is carried through verbatim; framec never
    /// looks inside it. It reorders; it does not interpret.
    ///
    /// This was missing, and the driver silently emitted Frame's syntax straight into
    /// Java: `public void progress(amount: int)`. Zero of fifteen corpus fixtures
    /// compiled. It was invisible because every hand-written example had used
    /// zero-argument events — the tests were shaped by what had been built.
    fn param_list(&self, params_text: &str) -> String;

    /// The public method for one interface event.
    ///
    /// `arms` is `(state, handler_owner)` — the state the machine may be IN, and the state
    /// whose handler actually runs. They differ under HSM: an event a child does not handle
    /// is handled by its nearest ancestor that does.
    ///
    /// The driver resolves that from the SYMBOL TABLE, at compile time. The backend just
    /// spells the switch. No runtime parent-chain walk, and no text.
    #[allow(clippy::too_many_arguments)]
    fn route(
        &self,
        sym: &SystemSym,
        event: &str,
        params: &str,
        ret: Option<&str>,
        is_async: bool,
        arms: &[(String, String)],
        out: &mut Sink,
    );

    /// `=> $^` — forward the current event to the PARENT's handler.
    fn forward(&self, rel: u32, owner: &str, event: &str, params: &str, out: &mut Sink);

    /// Open one `(state, handler)` method.
    #[allow(clippy::too_many_arguments)]
    fn open_handler(
        &self,
        sym: &SystemSym,
        state: &str,
        event: &str,
        params: &str,
        ret: Option<&str>,
        is_async: bool,
        out: &mut Sink,
    );
    /// Close it.
    ///
    /// `terminated` says whether the body already returned — a fact the DRIVER knows,
    /// because it walked the tree and saw the terminal statement. It is not something
    /// the backend re-derives by scanning what it just wrote.
    ///
    /// Java needs a fallback `return` on a value-returning method whose body might fall
    /// through — and must NOT emit one after a return, because unreachable code is a
    /// COMPILE ERROR there.
    fn close_handler(&self, ret: Option<&str>, is_async: bool, terminated: bool, out: &mut Sink);

    /// The indentation prefix for a statement `rel` columns deeper than the body's base.
    ///
    /// Java returns a constant — braces carry the nesting, so the layout is cosmetic.
    /// Python **must** reproduce it: a `@@:return` inside an `if x:` has to be indented
    /// under it, or the file is a SyntaxError.
    ///
    /// The driver computes `rel` from the source columns the scanner recorded. What to
    /// do with it is a SPELLING, and it lives here.
    fn pad(&self, rel: u32) -> String;

    /// Emit a native statement, already lowered and re-indented.
    fn native_stmt(&self, rel: u32, text: crate::NativeText, out: &mut Sink);

    /// `-> $Target(args)` — build and install the next compartment, then return.
    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink);
    /// `push$ -> $Target(args)`
    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink);
    /// `-> pop$`
    fn pop(&self, rel: u32, out: &mut Sink);

    /// **`@@:return(<expr>)`** — set the return value and exit. Terminal.
    fn return_call(&self, rel: u32, is_async: bool, expr: crate::NativeText, out: &mut Sink);

    /// **`@@:self.method(<args>)`** — a reentrant interface call. framec authored it, so
    /// framec terminates it.
    fn self_call(&self, rel: u32, is_async: bool, method: &str, args: &str, out: &mut Sink);

    /// An `actions:` / `operations:` member — a method with a NATIVE body. The
    /// signature is Frame's; the body is the user's.
    fn open_action(&self, name: &str, params: &str, ret: Option<&str>, out: &mut Sink);
    fn close_action(&self, out: &mut Sink);

    /// How this target declares a handler's **return type**.
    ///
    /// `is_async` matters: Java wraps it (`CompletableFuture<String>`), Python does not
    /// (the `async` goes on the `def`). Same node, two spellings.
    fn return_type(&self, t: Option<&str>) -> String;

    /// The system-level async spelling for the ROUTED interface method — the thing a
    /// caller sees. Java: `CompletableFuture<T>` + `completedFuture(...)`. Python:
    /// `async def` + a plain `return`.
    fn async_return_type(&self, t: Option<&str>) -> String;

    /// Wrap a returned value for an async method. Java: `CompletableFuture.completedFuture(v)`.
    /// Python: the value itself.
    ///
    /// Returns an **[`Atom`]** — so an `await` cannot land at the head. That is #225,
    /// where `await x.f()` invoked `f` on the *Promise* on eight targets, and where
    /// `java_await_rewrite` existed downstream purely to un-do it.
    fn async_wrap(&self, v: Atom) -> Atom;

    /// **`@@:self.x = <rhs>` / `$.x = <rhs>`** — a FRAME statement.
    ///
    /// framec owns this one end to end, **including the terminator**, which this method
    /// spells in the target's own way (`;` in Java, nothing in Python).
    ///
    /// The `rhs` arrives already lowered and rendered — its own Frame refs expanded, its
    /// literals untouched. This method never reads it; it places it.
    ///
    /// Note the LHS is passed as the REF, not as a `Place`. That is deliberate: for
    /// `@@:self.field` a backend produces `this.field = rhs;` (a real lvalue), but for
    /// `$.x` it must produce `map.put("x", rhs);` (a container operation, which has NO
    /// lvalue form). Those are different statements, not different spellings of one — and
    /// pretending otherwise is exactly why `$.x += 1` emitted an invalid lvalue (#227).
    fn assign(
        &self,
        sym: &SystemSym,
        state: &str,
        lhs: &FrameRef,
        rhs: crate::NativeText,
        rel: u32,
        out: &mut Sink,
    );

    /// Lower a Frame reference. **Returns an [`Atom`]** — so a bare cast, deref or
    /// `await` is not something a backend must remember to avoid; it is something it
    /// cannot express.
    fn lower_ref(&self, sym: &SystemSym, state: &str, r: &FrameRef) -> Atom;

    /// Emit `@@[persist]` — `snapshot()` and `restore()` — for this system, if it is
    /// persistent. A no-op otherwise.
    ///
    /// The `save`/`load` method NAMES are Frame's (`@@[save(snapshot)]`). The mechanism
    /// is fixed per target and type-ignorant: one walk, no per-user-type branch.
    fn persist(&self, m: &crate::text::emit::persist::PersistManifest, out: &mut Sink);

    /// Does this target make **unreachable code a compile error**?
    ///
    /// A `bool` in a table, not a `match` in a pass. Java is essentially alone here, and
    /// the old compiler expressed the same fact as `strip_java_unreachable` — a
    /// post-emission text pass that deleted statements out of already-generated code.
    ///
    /// (We stop emitting after a transition on *every* target regardless, because the
    /// code is genuinely dead everywhere. This flag exists to record *why* it is not
    /// merely a tidiness preference on one of them.)
    fn dead_code_is_an_error(&self) -> bool;
}

/// Emit every system in the file. **This function has no `Target`.**
pub fn emit(src: &Source, ast: &FileAst, syms: &SymbolTable, be: &dyn Backend) -> String {
    let mut out = Sink::new();
    be.file_header(&mut out);
    for item in &ast.items {
        // *** THE WATER. ***
        //
        // Native code outside a system is the USER'S code and passes through VERBATIM.
        // That is the Oceans model, and leaving it out meant every type the user defined
        // alongside their system silently vanished from the output.
        if let Item::Native(n) = item {
            out.native(super::reindent::render_span(src, n.span));
            continue;
        }
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };

        be.open_system(sym, &mut out);

        // The interface: one public method per event.
        //
        // HIERARCHICAL DISPATCH, resolved here, once, from the symbol table: for every
        // state the machine may be in, which state's handler actually runs? Under HSM
        // that is the nearest ancestor that declares the handler — possibly not the
        // state itself.
        for m in &sym.interface {
            let arms: Vec<(String, String)> = sym
                .states
                .iter()
                .filter_map(|st| {
                    sym.resolve_handler(&st.name, &m.name)
                        .map(|owner| (st.name.clone(), owner.name.clone()))
                })
                .collect();
            // A method is async if IT says so, or if the SYSTEM does (`@@[async]`).
            let is_async = m.is_async || sym.is_async;
            be.route(
                sym,
                &m.name,
                m.params_text.as_deref().unwrap_or(""),
                m.return_text.as_deref(),
                is_async,
                &arms,
                &mut out,
            );
        }

        // One private method per (state, handler).
        for sec in &sys.sections {
            let Section::Machine(mach) = sec else { continue };
            for mm in &mach.members {
                let MachineMember::State(st) = mm else { continue };
                for member in &st.members {
                    let StateMember::Handler(h) = member else {
                        continue;
                    };
                    let is_async = sym.is_async
                        || sym
                            .interface
                            .iter()
                            .any(|m| m.name == h.event && m.is_async);
                    be.open_handler(
                        sym,
                        &st.name,
                        &h.event,
                        &h.params_text,
                        h.return_text.as_deref(),
                        is_async,
                        &mut out,
                    );
                    let terminated =
                        emit_body(src, sym, &st.name, &h.event, is_async, &h.body, be, &mut out);
                    be.close_handler(h.return_text.as_deref(), is_async, terminated, &mut out);
                }
            }
        }

        // `actions:` / `operations:` — methods with NATIVE bodies. The signature is
        // Frame's; the body is the user's, decomposed like any other native code.
        for sec in &sys.sections {
            let (Section::Actions(d) | Section::Operations(d)) = sec else {
                continue;
            };
            for m in &d.members {
                let Decl::WithBody(b) = m else { continue };
                be.open_action(&b.name, &b.params_text, b.return_text.as_deref(), &mut out);
                emit_body(src, sym, "", "", false, &b.body, be, &mut out);
                be.close_action(&mut out);
            }
        }

        // `@@[persist]` — save/restore. Derived ONCE from the symbol table (RFC-0054),
        // then spelled per target. The disambiguation (out-of-band framing) is fixed in
        // each backend; the manifest just says WHAT to persist.
        let manifest = super::persist::PersistManifest::derive(sym);
        if manifest.enabled {
            be.persist(&manifest, &mut out);
        }

        be.close_system(sym, &mut out);
    }
    out.finish()
}

/// Walk a handler body. **The control flow lives here, once, for every language.**
/// Returns TRUE if the body ended in a terminal statement (a transition, a pop, or a
/// `@@:return`) — so nothing after it would be reachable.
#[allow(clippy::too_many_arguments)]
fn emit_body(
    src: &Source,
    sym: &SystemSym,
    state: &str,
    event: &str,
    is_async: bool,
    body: &Body,
    be: &dyn Backend,
    out: &mut Sink,
) -> bool {
    // A transition emits an implicit `return`, so everything after it in the same block
    // is dead. In Java that is a COMPILE ERROR; everywhere else it is merely wrong. The
    // old compiler expressed this as `strip_java_unreachable` — deleting statements out
    // of text it had just generated, to recover a fact it already knew.
    //
    // Here the tree knows the order, so the emitter stops. There is no pass; there is a
    // `bool`.
    let mut terminated = false;

    let lower = |r: &FrameRef| -> Atom { be.lower_ref(sym, state, r) };

    // The body's BASE column: the shallowest statement in it. Everything else is
    // measured relative to that, so the user's nesting is reproduced without framec ever
    // having to know what an `if` is.
    let base = body
        .stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::Native(n) => Some(n.logical_indent),
            Stmt::Transition(t) | Stmt::StackPush(t) => Some(t.col),
            Stmt::StackPop(x) | Stmt::Forward(x) => Some(x.col),
            Stmt::Assign(a) => Some(a.col),
            Stmt::ReturnCall(r) => Some(r.col),
            Stmt::SelfCall(c) => Some(c.col),
            Stmt::Trivia(_) => None,
        })
        .min()
        .unwrap_or(0);
    let rel = |c: u32| c.saturating_sub(base);

    for stmt in &body.stmts {
        if terminated {
            break;
        }
        match stmt {
            Stmt::Trivia(_) => {}
            Stmt::Native(n) => {
                let text = super::reindent::render_native(src, n, 0, &lower);
                be.native_stmt(rel(n.logical_indent), text, out);
            }
            // A terminal statement only terminates the BODY when it sits at the body's
            // BASE NESTING. Inside an `if` it returns from that branch, and the code
            // after the block is still reachable.
            //
            // ONE rule, both target families, using the two facts the scanner recorded:
            //
            //   * `depth == 0` — brace nesting. Catches Java's `if (x) { return; }`.
            //   * `rel == 0`   — source column. Catches Python's `if x:\n    return`,
            //                    where there ARE no braces and `depth` is always 0.
            //
            // Getting this wrong on Python DELETED the statement after the `if` block —
            // the emitter stopped, and `return "fail"` silently vanished from the output.
            Stmt::Transition(t) => {
                if let Some(target) = &t.target {
                    be.transition(rel(t.col), sym, target, t.args_text.as_deref(), out);
                    terminated = t.depth == 0 && rel(t.col) == 0;
                }
            }
            Stmt::StackPush(t) => {
                if let Some(target) = &t.target {
                    be.push(rel(t.col), sym, target, t.args_text.as_deref(), out);
                    terminated = t.depth == 0 && rel(t.col) == 0;
                }
            }
            Stmt::StackPop(s) => {
                be.pop(rel(s.col), out);
                terminated = s.depth == 0 && rel(s.col) == 0;
            }
            // A FRAME assignment. framec authored it, so framec terminates it — in the
            // backend's spelling, unconditionally, without ever looking at what it just
            // wrote.
            Stmt::Assign(a) => {
                let rhs = super::reindent::render_parts(src, &a.rhs, a.rhs_span, &lower);
                be.assign(sym, state, &a.lhs, rhs, rel(a.col), out);
            }
            Stmt::ReturnCall(r) => {
                let e = super::reindent::render_parts(src, &r.expr, r.expr_span, &lower);
                be.return_call(rel(r.col), is_async, e, out);
                // Terminal — but only for the BODY if it is at the base nesting.
                terminated = r.depth == 0 && rel(r.col) == 0;
            }
            Stmt::SelfCall(c) => be.self_call(rel(c.col), is_async, &c.method, &c.args_text, out),
            // `=> $^` — forward this event to the PARENT's handler. The driver knows
            // which state that is, because the symbol table knows the parent chain.
            Stmt::Forward(_) => {
                if let Some(owner) = sym.resolve_forward(state, event) {
                    let params = owner
                        .handlers
                        .iter()
                        .find(|h| h.event == event)
                        .map(|h| h.params_text.clone())
                        .unwrap_or_default();
                    be.forward(rel(0), &owner.name, event, &params, out);
                }
            }
        }
    }

    let _ = be.dead_code_is_an_error();
    terminated
}

/// `a: int, b: String` -> `a, b`. The CALL SITE — shared, because Frame names the
/// parameters and every target passes them positionally by that name.
///
/// (The DECLARATION is a different matter, and is a per-target spelling — see
/// [`Backend::param_list`].)
pub fn param_names(params: &str) -> String {
    params
        .split(',')
        .filter_map(|p| p.split(':').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Split Frame's `name: type, name: type` into `(name, type)` pairs.
///
/// **Frame's syntax, so framec may split it** (RULE 1). The type on the right is the
/// user's text and is returned untouched — a backend reorders it, never reads it.
pub fn params_split(params_text: &str) -> Vec<(String, Option<String>)> {
    params_text
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once(':') {
            Some((n, t)) => (n.trim().to_string(), Some(t.trim().to_string())),
            None => (p.to_string(), None),
        })
        .collect()
}
