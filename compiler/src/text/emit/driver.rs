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
use crate::text::scan::{param_scan, parse_one_param};
use crate::text::Source;
use crate::tree::body::{Body, EmbedCall, FrameRef, InstArg, Instantiation, ParamGroup, Stmt};
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

    /// A do-nothing statement, at `rel`. Emitted where a Frame construct lowers to nothing but
    /// the slot still needs a statement — e.g. `=> $^` forwarding to a parent that does not
    /// handle the event (a no-op), sitting alone inside an `if x:` block. Brace targets can
    /// leave the block empty (the default is nothing); an indent-delimited target (python)
    /// MUST emit `pass`, or the empty block is a syntax error.
    fn noop(&self, rel: u32, out: &mut Sink) {
        let _ = (rel, out);
    }

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
    /// `-> pop$` — restore the caller's compartment. **No return** (the driver adds it).
    fn pop(&self, rel: u32, out: &mut Sink);
    /// Bare `push$` — push a COPY of the current compartment onto the stack; stay in the
    /// current state. No transition, no return. (Default no-op; every real backend overrides.)
    fn push_bare(&self, rel: u32, out: &mut Sink) {
        let _ = (rel, out);
    }
    /// Bare `pop$` — pop and DISCARD the top of the stack; stay in the current state.
    fn pop_bare(&self, rel: u32, out: &mut Sink) {
        let _ = (rel, out);
    }

    /// Call a lifecycle handler — `$>` enter or `<$` exit — with its args, unsplit.
    /// framec authored this call and terminates it.
    fn lifecycle_call(&self, rel: u32, sym: &SystemSym, state: &str, event: &str, args: Option<&str>, out: &mut Sink);

    /// Deliver `-> (enter) pop$` enter args to the RESTORED state's `$>` handler. The
    /// popped state is runtime-determined, so this emits a state dispatch: for each state
    /// that declares a `$>`, a guard that (when the restored compartment is in that state)
    /// calls its enter handler with the enter args.
    fn pop_enter(&self, rel: u32, sym: &SystemSym, enter_args: Option<&str>, out: &mut Sink);

    /// The return that ends a transition/push/pop. Spelled per target.
    fn terminate(&self, rel: u32, out: &mut Sink);

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

    /// The target's constructor NAME-PREFIX for `@@SystemName(...)` (spec §1103): Java
    /// `new Sub`, Rust `Sub::new`, C `Sub_new`, Python `Sub`. Context-free — the `(...)`
    /// args are native water that follows and completes the call. Used both from
    /// top-level native water.
    ///
    /// The `args` are the final constructor arguments in ctor order (state, then enter,
    /// then domain), already matched against the declared params and defaults by
    /// [`lower_instantiation`]. The backend only spells the call: `new Sub(a, b)` /
    /// `Sub::new(a, b)` / `Sub_new(a, b)` / `Sub(a, b)`.
    fn system_ctor_call(&self, name: &str, args: &[String]) -> Atom;

    /// `@@:self.field.method(args)` — an embedded-system (or scalar-field) call (RFC-0046).
    /// The backend inspects `sym.domain` to see whether `field` is a system-typed domain
    /// field: if so it emits the cross-system idiom (C's `Sys_method(self->field, args)`);
    /// otherwise a native method call on the field. Most targets spell both the same
    /// (`receiver.field.method(args)`); only C's system case diverges.
    fn embed_call(&self, sym: &SystemSym, ec: &EmbedCall) -> Atom;

    /// Emit `@@[persist]` — `snapshot()` and `restore()` — for this system, if it is
    /// persistent. A no-op otherwise.
    ///
    /// The `save`/`load` method NAMES are Frame's (`@@[save(snapshot)]`). The mechanism
    /// is fixed per target and type-ignorant: one walk, no per-user-type branch.
    fn persist(&self, m: &crate::text::emit::persist::PersistManifest, out: &mut Sink);

    /// Does this target have an async runtime that can realize `@@[async]`? Most do; C
    /// (and a few others, per RFC-0044) do not — there is no coroutine/future primitive
    /// to hang the single-driver gate on. A `false` here turns an async system on this
    /// target into an E722 (see [`target_diagnostics`]) rather than a silent sync
    /// miscompile that drops the async contract.
    fn supports_async(&self) -> bool {
        true
    }

    /// Does this target have class-level visibility, so `@@system private Name` can be realized
    /// (Java: `class` vs `public class`; Rust: `struct` vs `pub struct`)? Targets with no such
    /// concept (Python, C, …) return `false`, turning `private` into an E731 rather than a
    /// silently-ignored modifier. Default `false` — a backend opts IN.
    fn supports_class_visibility(&self) -> bool {
        false
    }

}

/// Target-aware validation: diagnostics that depend on the BACKEND's capabilities, not
/// just the program. Kept out of [`crate::validate::validate`] (which is target-blind) and
/// out of [`emit`] (which must not branch on the target at all).
///
/// **E722** — an async system (`@@[async]` or any `async` member) targeting a backend with
/// no async runtime. C has none; emitting sync code would silently drop the async contract,
/// so framec refuses (matching the shipped compiler's E722).
pub fn target_diagnostics(
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<crate::resolve::Diagnostic> {
    let mut out = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };

        // E722 — an async system on a target with no async runtime.
        if !be.supports_async() && (sym.is_async || sym.interface.iter().any(|m| m.is_async)) {
            out.push(crate::resolve::Diagnostic {
                code: "E722",
                severity: crate::resolve::Severity::Error,
                span: sym.span,
                message: format!(
                    "system `{}` is async, but target `{}` has no async runtime — \
                     `@@[async]` cannot be realized here",
                    sym.name,
                    be.name()
                ),
            });
        }

        // E731 — `@@system private Name` on a target without class-level visibility (Python, C,
        // …). Realizing it is impossible, and silently ignoring it would leave the user believing
        // a system is hidden when it is not. Java/Rust (which have visibility) opt in and are fine.
        if sym.private && !be.supports_class_visibility() {
            out.push(crate::resolve::Diagnostic {
                code: "E731",
                severity: crate::resolve::Severity::Error,
                span: sym.span,
                message: format!(
                    "system `{}` is `private`, but target `{}` has no class-level visibility — \
                     the modifier cannot be realized here; drop it for this target.",
                    sym.name,
                    be.name()
                ),
            });
        }

        // (E752 retired — RFC-0056 now gives C an author-hook route for user types: framec
        // emits the hook call + `extern`, and a missing definition is a build-time link
        // error, not a framec-time refusal. So there is no target that rejects a persisted
        // field outright, and `persistable_field` is gone.)
    }
    out
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
            // Water — verbatim, EXCEPT `@@SystemName(...)` islands (spec §1103), which are
            // Frame's own syntax even out here and lower to the target constructor. There
            // is no compartment at top level, so a plain ref cannot legally occur here; if
            // one did it renders as its original text.
            let bytes = src.open();
            let reference = |r: &FrameRef| -> Atom {
                Atom::ident(String::from_utf8_lossy(&bytes[r.span.start..r.span.end]).into_owned())
            };
            let instantiate = |inst: &Instantiation| -> Atom { lower_instantiation(syms, be, inst) };
            // No `@@:self` at top level — an embed call cannot occur here; render verbatim.
            let embed = |ec: &EmbedCall| -> Atom {
                Atom::ident(String::from_utf8_lossy(&bytes[ec.span.start..ec.span.end]).into_owned())
            };
            let lower = super::reindent::Lowering {
                reference: &reference,
                instantiate: &instantiate,
                embed: &embed,
            };
            out.native(super::reindent::render_water(src, &n.parts, n.span, &lower));
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

        // One private method per (state, handler) — the driver's `(section, state, handler)`
        // nested pass, reified as the `EmitHandlers` @@system (`emit_handlers.frs`): three nested
        // cycle states carrying the three walk cursors, one private method emitted per handler. The
        // byte-for-byte oracle it replaced is preserved as [`emit_handlers_hand`], gated in
        // `tests/emit_handlers.rs` (GATE-A, via [`handlers_parity_report`]).
        super::emit_handlers::walk(src, syms, sym, &sys.sections, be, &mut out);

        // `actions:` / `operations:` — methods with NATIVE bodies. The signature is
        // Frame's; the body is the user's, decomposed like any other native code.
        for sec in &sys.sections {
            let (Section::Actions(d) | Section::Operations(d)) = sec else {
                continue;
            };
            for m in &d.members {
                let Decl::WithBody(b) = m else { continue };
                be.open_action(&b.name, &b.params_text, b.return_text.as_deref(), &mut out);
                emit_body(src, syms, sym, "", "", false, &b.body, be, &mut out);
                be.close_action(&mut out);
            }
        }

        // `@@[persist]` — save/restore. Derived ONCE from the symbol table (RFC-0054),
        // then spelled per target. The disambiguation (out-of-band framing) is fixed in
        // each backend; the manifest just says WHAT to persist.
        let manifest = super::persist::PersistManifest::derive(sym, syms);
        if manifest.enabled {
            be.persist(&manifest, &mut out);
        }

        be.close_system(sym, &mut out);
    }
    out.finish()
}

/// How a handler or action body ended — the two distinct terminals the statement walk
/// reaches, named so they stay distinct at the call site instead of collapsing into a
/// bare `bool`.
///
/// The walk emits statements until a *base-nesting* terminal (a transition / stack-push /
/// pop / `@@:return` at `depth == 0 && rel == 0`) fires, then stops — so any trailing
/// statements after it are never spelled (they are dead: a compile error on Java, merely
/// wrong elsewhere). The old compiler recovered this fact with a post-emission text pass
/// (`strip_java_unreachable`); here the tree knows the order, so the emitter simply stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyEnd {
    /// A base-nesting terminal fired: the walk halted; any trailing statements were dead and
    /// never spelled. The handler needs no fallback return.
    Terminated,
    /// Statements ran out with no base-nesting terminal: control falls through, so a target
    /// that requires an explicit tail return (Java) gets its fallback.
    Fell,
}

impl BodyEnd {
    /// Did the body terminate at base nesting? (The single bit `close_handler` consults to
    /// decide whether a fallback return is needed.)
    pub(super) fn terminated(self) -> bool {
        matches!(self, BodyEnd::Terminated)
    }
}

/// Walk a handler/action body — **the emit-side transducer**, reified as the
/// [`super::stmt_walk`] `@@system` (`StmtWalk`). The control flow lives in that machine, once,
/// for every language; this is the thin driver that (1) computes the body's BASE column (the
/// shallowest statement, everything else measured relative to it, so the user's nesting is
/// reproduced without framec knowing what an `if` is), (2) seeds the machine's owned output with
/// the caller's `Sink` (the handler prologue already emitted, so body text appends exactly where
/// the hand walk appended it), (3) drives it to fixpoint, and (4) reads back the grown `Sink` and
/// the `terminated` latch as a [`BodyEnd`]. The byte-for-byte ORACLE it replaced is preserved as
/// [`emit_body_hand`], gated at every statement in `tests/stmt_walk.rs`.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_body(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    event: &str,
    is_async: bool,
    body: &Body,
    be: &dyn Backend,
    out: &mut Sink,
) -> BodyEnd {
    // The body's BASE column — the shallowest statement, everything else measured relative to it —
    // reified as the `BaseColumn` min-fold `@@system` (`base_column.frs`). The byte-for-byte oracle
    // it replaced is preserved as [`base_column_hand`], gated per body in `tests/base_column.rs`.
    let base = super::base_column::compute(&body.stmts);
    let seed = std::mem::take(out);
    let (grown, terminated) = super::stmt_walk::walk(
        src, syms, sym, &body.stmts, state, event, is_async, base, be, seed,
    );
    *out = grown;
    if terminated {
        BodyEnd::Terminated
    } else {
        BodyEnd::Fell
    }
}

/// The preserved byte-for-byte **oracle** for [`emit_body`] — the original hand statement walk,
/// kept as the differential check the [`super::stmt_walk`] machine is proven against (GATE-A,
/// `tests/stmt_walk.rs`, via [`body_parity_report`]). Doc-hidden and **not on the production
/// path**. Do not edit it to add behavior: it exists only to reproduce the pre-conversion output
/// exactly, so any divergence is the machine's bug, not the oracle's.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
fn emit_body_hand(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    state: &str,
    event: &str,
    is_async: bool,
    body: &Body,
    be: &dyn Backend,
    out: &mut Sink,
) -> BodyEnd {
    // A transition emits an implicit `return`, so everything after it in the same block
    // is dead. In Java that is a COMPILE ERROR; everywhere else it is merely wrong. The
    // old compiler expressed this as `strip_java_unreachable` — deleting statements out
    // of text it had just generated, to recover a fact it already knew.
    //
    // Here the tree knows the order, so the emitter stops. There is no pass; there is a
    // `bool`.
    let mut terminated = false;

    let reference = |r: &FrameRef| -> Atom { be.lower_ref(sym, state, r) };
    let instantiate = |inst: &Instantiation| -> Atom { lower_instantiation(syms, be, inst) };
    let embed = |ec: &EmbedCall| -> Atom { be.embed_call(sym, ec) };
    let lower = super::reindent::Lowering {
        reference: &reference,
        instantiate: &instantiate,
        embed: &embed,
    };

    // The body's BASE column: the shallowest statement in it. Everything else is
    // measured relative to that, so the user's nesting is reproduced without framec ever
    // having to know what an `if` is. The oracle reads it from the preserved [`base_column_hand`]
    // fold (the same one the `BaseColumn` machine is gated against), so this hand walk stays a
    // single-source byte-for-byte reference for both conversions at once.
    let base = base_column_hand(&body.stmts);
    let rel = |c: u32| c.saturating_sub(base);

    for stmt in &body.stmts {
        if terminated {
            break;
        }
        match stmt {
            Stmt::Trivia(_) => {}
            Stmt::Native(n) => {
                // Re-indent the WHOLE native statement, not just its first line. A multi-line
                // native block (an `if/elif/else` the user wrote at Frame's nesting) must sit
                // at the TARGET's nesting, with its internal structure preserved. `native_stmt`
                // pads the first line by `pad(rel)`; the CONTINUATION lines are shifted by the
                // same amount here — `delta = target indent width − the statement's source
                // column`. This goes through the literal-safe reindent path (only Text parts
                // move; a newline inside a string literal is never touched — the #215 rule).
                // Without it, continuation lines kept their original source columns and python
                // (indent-sensitive) raised IndentationError.
                let r = rel(n.logical_indent);
                let delta = be.pad(r).len() as i32 - n.logical_indent as i32;
                let text = super::reindent::render_native(src, n, delta, &lower);
                be.native_stmt(r, text, out);
            }
            // A transition orchestrates the LIFECYCLE, and the order is Frame's, uniform
            // across every target: exit the source state, build+install the target
            // compartment, enter the target state, return. The backend only SPELLS each
            // step. Before this, lifecycle handlers were emitted but NEVER CALLED — `$>`
            // and `<$` did not run, and exit/enter args were dropped.
            //
            // A terminal statement terminates the BODY only at the base nesting
            // (`depth == 0 && rel == 0`) — see below.
            Stmt::Transition(t) => {
                if let Some(target) = &t.target {
                    let r = rel(t.col);
                    if has_lifecycle(sym, state, "<$") {
                        be.lifecycle_call(r, sym, state, "<$", t.exit_args.as_deref(), out);
                    }
                    be.transition(r, sym, target, t.args_text.as_deref(), out);
                    if has_lifecycle(sym, target, "$>") {
                        be.lifecycle_call(r, sym, target, "$>", t.enter_args.as_deref(), out);
                    }
                    be.terminate(r, out);
                    terminated = t.depth == 0 && r == 0;
                }
            }
            Stmt::StackPush(t) => {
                if let Some(target) = &t.target {
                    let r = rel(t.col);
                    if has_lifecycle(sym, state, "<$") {
                        be.lifecycle_call(r, sym, state, "<$", t.exit_args.as_deref(), out);
                    }
                    be.push(r, sym, target, t.args_text.as_deref(), out);
                    if has_lifecycle(sym, target, "$>") {
                        be.lifecycle_call(r, sym, target, "$>", t.enter_args.as_deref(), out);
                    }
                    be.terminate(r, out);
                    terminated = t.depth == 0 && r == 0;
                } else {
                    // bare `push$` — push a COPY of the current compartment; STAY (no
                    // transition, so no exit/enter lifecycle and no terminating return).
                    be.push_bare(rel(t.col), out);
                }
            }
            // bare `pop$` — pop and DISCARD the top; STAY (no restore, no terminate).
            Stmt::StackPopBare(st) => {
                be.pop_bare(rel(st.col), out);
            }
            Stmt::StackPop(st) => {
                let r = rel(st.col);
                if has_lifecycle(sym, state, "<$") {
                    be.lifecycle_call(r, sym, state, "<$", st.exit_args.as_deref(), out);
                }
                be.pop(r, out);
                // `-> (enter) pop$` — deliver the enter args to the RESTORED state's `$>`,
                // dispatched at runtime (the popped state is dynamic).
                if st.enter_args.is_some() {
                    be.pop_enter(r, sym, st.enter_args.as_deref(), out);
                }
                be.terminate(r, out);
                terminated = st.depth == 0 && r == 0;
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
            Stmt::Forward(fwd) => {
                if let Some(owner) = sym.resolve_forward(state, event) {
                    let params = owner
                        .handlers
                        .iter()
                        .find(|h| h.event == event)
                        .map(|h| h.params_text.clone())
                        .unwrap_or_default();
                    // Indent at the forward's OWN nesting — `rel(fwd.col)`, not a hardcoded
                    // `rel(0)`. A `=> $^` inside `if x:` must sit under it; emitting at the
                    // body base put it outside the block (a python IndentationError, and
                    // wrong-but-tolerated on brace targets).
                    be.forward(rel(fwd.col), &owner.name, event, &params, out);
                } else {
                    // The parent does not handle this event: `=> $^` is a no-op (the parent
                    // state's dispatch would run and do nothing). Emit a no-op so the enclosing
                    // block is not left empty — a `pass` on python, nothing on brace targets.
                    be.noop(rel(fwd.col), out);
                }
            }
        }
    }

    if terminated {
        BodyEnd::Terminated
    } else {
        BodyEnd::Fell
    }
}

/// The preserved byte-for-byte **oracle** for the driver's HANDLER-EMISSION pass — the original
/// `(section, state, handler)` nested loops [`emit`] ran before they were reified as the
/// [`super::emit_handlers`] `@@system` (`EmitHandlers`). Kept as the differential check that machine
/// is proven against (GATE-A, `tests/emit_handlers.rs`, via [`handlers_parity_report`]). It calls
/// the SAME production [`emit_body`] the machine's `emit_handler` leaf calls — the two paths differ
/// only in how the three-level walk is SEQUENCED (hand loops vs cycle states), which is exactly what
/// the gate isolates. Doc-hidden and **not on the production path**. Do not edit it to add behavior:
/// it exists only to reproduce the pre-conversion sequencing exactly, so any divergence is the
/// machine's bug, not the oracle's.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
fn emit_handlers_hand(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    for sec in sections {
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
                // A handler inherits the interface method's return TYPE when it does not
                // declare one itself. The router returns the interface type, so the handler
                // method must produce it — otherwise a value-returning event typed only on
                // the interface (`getmag(): f64` + `getmag() { @@:(...) }`) mismatches: a
                // `-> ()` handler returning a value (E0308 on Rust, a void method on Java/C).
                let ret = h.return_text.as_deref().or_else(|| {
                    sym.interface
                        .iter()
                        .find(|m| m.name == h.event)
                        .and_then(|m| m.return_text.as_deref())
                });
                be.open_handler(sym, &st.name, &h.event, &h.params_text, ret, is_async, out);
                let end =
                    emit_body(src, syms, sym, &st.name, &h.event, is_async, &h.body, be, out);
                be.close_handler(ret, is_async, end.terminated(), out);
            }
        }
    }
}

/// TEST-ONLY (GATE-A) — one system's dual handler-emission (machine path vs hand oracle), for
/// `tests/emit_handlers.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct HandlersParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `EmitHandlers` machine path ([`super::emit_handlers::walk`]) emits for ALL of this
    /// system's private `(state, handler)` methods.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`emit_handlers_hand`]) emits for the same.
    pub hand_text: String,
    /// How many `(state, handler)` methods this system emits — so the test can prove the corpus
    /// actually exercised multi-handler / multi-state / multi-section shapes (a system that emits
    /// zero handlers is a vacuous pass).
    pub handler_count: usize,
}

/// TEST-ONLY (GATE-A). Emit **every** system's private `(state, handler)` methods through BOTH the
/// `EmitHandlers` machine ([`super::emit_handlers::walk`]) and the preserved hand oracle
/// ([`emit_handlers_hand`]) — over the SAME real parsed systems and the SAME backend — and return,
/// per system, the two emitted Strings. `tests/emit_handlers.rs` asserts, for every entry,
/// `machine_text == hand_text` byte-for-byte. The library owns the `.finish()` and the real emit
/// traversal.
#[doc(hidden)]
pub fn handlers_parity_report(
    src: &Source,
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<HandlersParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        let mut mo = super::Sink::default();
        super::emit_handlers::walk(src, syms, sym, &sys.sections, be, &mut mo);
        let mut ho = super::Sink::default();
        emit_handlers_hand(src, syms, sym, &sys.sections, be, &mut ho);
        report.push(HandlersParity {
            label: sym.name.clone(),
            machine_text: mo.finish(),
            hand_text: ho.finish(),
            handler_count: count_handlers(&sys.sections),
        });
    }
    report
}

/// TEST-ONLY (GATE-A) — the number of `(state, handler)` methods `sections` emits, the coverage
/// tally [`handlers_parity_report`] reports so a vacuous (zero-handler) system cannot pass as
/// covered. Same three-level structural walk the machine and the oracle share.
#[doc(hidden)]
fn count_handlers(sections: &[Section]) -> usize {
    let mut n = 0;
    for sec in sections {
        let Section::Machine(mach) = sec else { continue };
        for mm in &mach.members {
            let MachineMember::State(st) = mm else { continue };
            for member in &st.members {
                if let StateMember::Handler(_) = member {
                    n += 1;
                }
            }
        }
    }
    n
}

/// The preserved byte-for-byte **oracle** for the body BASE-column min-fold — the original inline
/// `.filter_map(...).min().unwrap_or(0)` `emit_body` computed before it was reified as the
/// [`super::base_column`] `@@system`. Kept as the differential check that machine is proven against
/// (GATE-A, `tests/base_column.rs`, via [`base_parity_report`]), and read by the preserved
/// [`emit_body_hand`] so a single fold anchors both conversions. Doc-hidden and **not on the
/// production path**. Do not edit it to add behavior: it exists only to reproduce the
/// pre-conversion value exactly, so any divergence is the machine's bug, not the oracle's.
#[doc(hidden)]
pub fn base_column_hand(stmts: &[Stmt]) -> u32 {
    stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::Native(n) => Some(n.logical_indent),
            Stmt::Transition(t) | Stmt::StackPush(t) => Some(t.col),
            Stmt::StackPop(x) | Stmt::StackPopBare(x) | Stmt::Forward(x) => Some(x.col),
            Stmt::Assign(a) => Some(a.col),
            Stmt::ReturnCall(r) => Some(r.col),
            Stmt::SelfCall(c) => Some(c.col),
            Stmt::Trivia(_) => None,
        })
        .min()
        .unwrap_or(0)
}

/// TEST-ONLY (GATE-A) — one body's dual BASE-column (machine min-fold vs hand oracle), for
/// `tests/base_column.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct BaseParity {
    /// A `system/state/event` (or `system/action`) label for a failing assertion message.
    pub label: String,
    /// The base column the `BaseColumn` machine path ([`super::base_column::compute`]) reports.
    pub machine_base: u32,
    /// The base column the preserved hand oracle ([`base_column_hand`]) reports.
    pub hand_base: u32,
    /// The kind discriminants (0..9) of the statements in this body — so the test can prove the
    /// corpus exercised every column-bearing (and the skipped Trivia) variant.
    pub kinds: Vec<i32>,
}

/// TEST-ONLY (GATE-A). Compute the BASE column of **every** handler and action body in `ast`
/// through BOTH the `BaseColumn` machine ([`super::base_column::compute`]) and the preserved hand
/// oracle ([`base_column_hand`]) — over the SAME real parsed bodies. `tests/base_column.rs`
/// asserts, for every entry, `machine_base == hand_base`. The BASE column is target-free (it comes
/// from the tree's source columns, not any backend), so this takes no `Backend`.
#[doc(hidden)]
pub fn base_parity_report(ast: &FileAst, syms: &SymbolTable) -> Vec<BaseParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        for sec in &sys.sections {
            match sec {
                Section::Machine(mach) => {
                    for mm in &mach.members {
                        let MachineMember::State(st) = mm else { continue };
                        for member in &st.members {
                            let StateMember::Handler(h) = member else { continue };
                            report.push(BaseParity {
                                label: format!("{}/{}/{}", sym.name, st.name, h.event),
                                machine_base: super::base_column::compute(&h.body.stmts),
                                hand_base: base_column_hand(&h.body.stmts),
                                kinds: h.body.stmts.iter().map(super::stmt_walk::kind_of).collect(),
                            });
                        }
                    }
                }
                Section::Actions(d) | Section::Operations(d) => {
                    for m in &d.members {
                        let Decl::WithBody(b) = m else { continue };
                        report.push(BaseParity {
                            label: format!("{}/action:{}", sym.name, b.name),
                            machine_base: super::base_column::compute(&b.body.stmts),
                            hand_base: base_column_hand(&b.body.stmts),
                            kinds: b.body.stmts.iter().map(super::stmt_walk::kind_of).collect(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    report
}

/// TEST-ONLY (GATE-A) — one body's dual emission (machine path vs hand oracle), for
/// `tests/stmt_walk.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct BodyParity {
    /// A `system/state/event` (or `system/action`) label for a failing assertion message.
    pub label: String,
    /// Text emitted by the `StmtWalk` machine path (production [`emit_body`]).
    pub machine_text: String,
    /// Whether the machine path reported a base-nesting terminal.
    pub machine_terminated: bool,
    /// Text emitted by the preserved hand oracle ([`emit_body_hand`]).
    pub hand_text: String,
    /// Whether the hand oracle reported a base-nesting terminal.
    pub hand_terminated: bool,
    /// The kind discriminants (0..9) of the statements in this body — so the test can prove the
    /// corpus exercised every Stmt variant, using the SAME classifier the machine dispatches on.
    pub kinds: Vec<i32>,
}

/// TEST-ONLY (GATE-A). Emit **every** handler and action body in `ast` through BOTH the
/// `StmtWalk` machine ([`emit_body`]) and the preserved hand oracle ([`emit_body_hand`]) — over
/// the SAME real parsed bodies and the SAME backend — and return, per body, the two emitted
/// Strings and their `terminated` bits. `tests/stmt_walk.rs` asserts, for every entry,
/// `machine_text == hand_text` byte-for-byte AND `machine_terminated == hand_terminated`. The
/// library owns the `.finish()` (a test crate cannot obtain a `String` from a `Sink`) and the real
/// emit traversal (so the bodies, spans, and refs are exactly production's).
#[doc(hidden)]
pub fn body_parity_report(
    src: &Source,
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<BodyParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        for sec in &sys.sections {
            match sec {
                Section::Machine(mach) => {
                    for mm in &mach.members {
                        let MachineMember::State(st) = mm else { continue };
                        for member in &st.members {
                            let StateMember::Handler(h) = member else { continue };
                            let is_async = sym.is_async
                                || sym.interface.iter().any(|m| m.name == h.event && m.is_async);
                            let label = format!("{}/{}/{}", sym.name, st.name, h.event);
                            let mut mo = super::Sink::default();
                            let me =
                                emit_body(src, syms, sym, &st.name, &h.event, is_async, &h.body, be, &mut mo);
                            let mut ho = super::Sink::default();
                            let he = emit_body_hand(
                                src, syms, sym, &st.name, &h.event, is_async, &h.body, be, &mut ho,
                            );
                            report.push(BodyParity {
                                label,
                                machine_text: mo.finish(),
                                machine_terminated: me.terminated(),
                                hand_text: ho.finish(),
                                hand_terminated: he.terminated(),
                                kinds: h.body.stmts.iter().map(super::stmt_walk::kind_of).collect(),
                            });
                        }
                    }
                }
                Section::Actions(d) | Section::Operations(d) => {
                    for m in &d.members {
                        let Decl::WithBody(b) = m else { continue };
                        let label = format!("{}/action:{}", sym.name, b.name);
                        let mut mo = super::Sink::default();
                        let me = emit_body(src, syms, sym, "", "", false, &b.body, be, &mut mo);
                        let mut ho = super::Sink::default();
                        let he = emit_body_hand(src, syms, sym, "", "", false, &b.body, be, &mut ho);
                        report.push(BodyParity {
                            label,
                            machine_text: mo.finish(),
                            machine_terminated: me.terminated(),
                            hand_text: ho.finish(),
                            hand_terminated: he.terminated(),
                            kinds: b.body.stmts.iter().map(super::stmt_walk::kind_of).collect(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    report
}

/// Lower `@@SystemName(args)` (spec §1103) to the target constructor call. **The call-site
/// arg routing lives here, once, for every target.**
///
/// The declared params fix the constructor's shape: order is state, then enter, then domain
/// (matching [`Backend::open_system`]'s signature). For each declared param this fills its
/// value from the matching call arg — by name in the named form, by group-position in the
/// positional form — falling back to the declared default. The backend only spells the
/// final call; it never sees the matching.
pub(super) fn lower_instantiation(syms: &SymbolTable, be: &dyn Backend, inst: &Instantiation) -> Atom {
    let Some(sys) = syms.systems.iter().find(|s| s.name == inst.name) else {
        // Unknown system: emit a best-effort call so the TARGET compiler reports it
        // (the closed-world validation layer, §1167, is deferred). `inst.args` is the
        // PRIMARY candidate — G when the angle hypotheses forked (owner ruling, design
        // record §11.3): when arity is unavailable, G is the default rendering, because G
        // keeps generics whole — the reading that produces plausible-looking emitted code
        // for the target compiler to judge.
        let args: Vec<String> = inst.args.iter().map(|a| a.value.clone()).collect();
        return be.system_ctor_call(&inst.name, &args);
    };
    // The angle fork (if any) is settled by the ONE shared adjudicator — the same call
    // validate makes, so the two consumers can never disagree. `NoneAdmissible` /
    // `BothAdmissible` are unreachable here on the error path (validate's E407 blocks
    // emission); post-error best-effort renders the primary (G) candidate.
    let (args, named) = match crate::validate::adjudicate(&sys.params, inst) {
        crate::validate::Adjudication::Alt => match &inst.angles {
            crate::tree::body::ArgAngles::Forked {
                alt_args,
                alt_named,
            } => (alt_args.as_slice(), *alt_named),
            _ => (inst.args.as_slice(), inst.named),
        },
        _ => (inst.args.as_slice(), inst.named),
    };
    let p = &sys.params;
    let mut ordered = Vec::new();
    ordered.extend(resolve_group(&p.state, ParamGroup::State, args, named));
    ordered.extend(resolve_group(&p.enter, ParamGroup::Enter, args, named));
    ordered.extend(resolve_group(&p.domain, ParamGroup::Domain, args, named));
    // A no-default param omitted at the call site leaves an empty slot — an arity error the
    // target compiler will report. Trailing empties (an all-defaulted tail) just shorten
    // the call.
    while ordered.last().map(|s: &String| s.is_empty()).unwrap_or(false) {
        ordered.pop();
    }
    be.system_ctor_call(&inst.name, &ordered)
}

/// Resolve the values for one declared param group against the adjudicated candidate's
/// args of that group (the candidate view: `inst.args`/`inst.named`, or the fork's
/// alternate when adjudication picked O).
fn resolve_group(
    decls: &[crate::tree::Param],
    group: ParamGroup,
    args: &[InstArg],
    named: bool,
) -> Vec<String> {
    let provided: Vec<&InstArg> = args.iter().filter(|a| a.group == group).collect();
    decls
        .iter()
        .enumerate()
        .map(|(idx, decl)| {
            let arg = if named {
                provided
                    .iter()
                    .find(|a| a.name.as_deref() == Some(decl.name.as_str()))
                    .copied()
            } else {
                provided.get(idx).copied()
            };
            match arg {
                Some(a) => a.value.clone(),
                None => decl.default.clone().unwrap_or_default(),
            }
        })
        .collect()
}

/// The system's header params as Frame `name: type` declaration text, in CONSTRUCTOR
/// order — state, then enter, then domain (spec §203). Feed to [`Backend::param_list`] for
/// the constructor signature; the call site ([`lower_instantiation`]) fills values in the
/// same order.
pub fn ctor_params_text(p: &crate::tree::SystemParams) -> String {
    p.state
        .iter()
        .chain(&p.enter)
        .chain(&p.domain)
        .map(|param| match &param.ty {
            Some(t) => format!("{}: {}", param.name, t),
            None => param.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Does `state` declare a lifecycle handler for `event` (`$>` / `<$`)?
pub fn has_lifecycle(sym: &SystemSym, state: &str, event: &str) -> bool {
    sym.states
        .iter()
        .find(|s| s.name == state)
        .map(|s| s.handlers.iter().any(|h| h.event == event))
        .unwrap_or(false)
}

/// `a: int, b: String` -> `a, b`. The CALL SITE — shared, because Frame names the
/// parameters and every target passes them positionally by that name.
///
/// (The DECLARATION is a different matter, and is a per-target spelling — see
/// [`Backend::param_list`].)
pub fn param_names(params: &str) -> String {
    param_scan::parse_decl(params.as_bytes())
        .into_iter()
        .map(|(_group, body)| parse_one_param(&body).name)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Split Frame's `name: type, name: type` into `(name, type)` pairs.
///
/// **Frame's syntax, so framec may split it** (RULE 1). The type on the right is the
/// user's text and is returned untouched — a backend reorders it, never reads it.
///
/// Routed through the SAME machines the system-header path uses (#249 B1): the top-level comma
/// split is the dogfooded `ParamScan` counter automaton (Dyck-1 over `()[]{}` + angle fork,
/// `"`-opaque), and each body's `name: type` split is `parse_one_param` (top-level `=`/`:` via
/// `TopLevelEq`). So a param whose type carries a top-level `,` (`Map<K, V>`, `fn(int, str)`) or a
/// nested `=` is no longer torn into a phantom param — the naive `.split(',')` is GONE. `ParamScan`
/// is target-free (`"`-only), so the emitter needs no `Target` (the residual char/lifetime opacity
/// gap is the same #219 carry the declaration-site family accepts).
pub fn params_split(params_text: &str) -> Vec<(String, Option<String>)> {
    param_scan::parse_decl(params_text.as_bytes())
        .into_iter()
        .map(|(_group, body)| {
            let p = parse_one_param(&body);
            (p.name, p.ty)
        })
        .collect()
}
