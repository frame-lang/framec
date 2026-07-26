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
use crate::tree::{Decl, FileAst, HandlerNode, Item, MachineMember, Section, StateMember};

/// **Which kind of method's body is being walked.**
///
/// A `machine:` handler and an `actions:`/`operations:` member are both "a body of statements",
/// but they are not the same *thing*, and one Frame construct — `@@:(expr)` — has to be spelled
/// differently in each (see [`Backend::return_call`]). The distinction is a fact **framec** put
/// on the tree: the body came out of a [`Section::Actions`]/[`Section::Operations`] decl or out
/// of a state's [`HandlerNode`]. So it travels as a TAG.
///
/// It deliberately does **not** travel as `state == "" && event == ""`, which is what the sentinel
/// arguments at the action call site would otherwise mean. That is the shipped compiler's
/// `MethodRole` mistake in miniature — a generated *name* used as the ad-hoc wire format of a
/// missing field, decoded independently by every consumer. Names are for humans; tags are for
/// compilers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BodyRole {
    /// A `machine:` state handler. It runs UNDER the kernel, with a live `FrameContext` on
    /// `self._context_stack`, and it returns its value through that context's slot.
    Handler,
    /// An `actions:` / `operations:` member. It is an ORDINARY METHOD: the user may call it
    /// directly, from outside any dispatch, so there is no context to park a return value on.
    Action,
}

/// The **statement-leaf context** — the per-body facts a spelling leaf needs but the census's
/// "one parameter" seam never carried to it.
///
/// Several `Backend` leaves (`return_call`, `terminate`, `close_handler`, `native_stmt`) were
/// handed only `rel`/`out` and so could not tell *which system / method / state* they were
/// spelling, nor whether the enclosing system is a positioned `@@[scan]` recognizer (whose 24
/// self-hosted `.gen.rs` ride a thin dispatch model) or an ordinary kernel-model system. Rust
/// needs all four: a value-return builds `<Sys>FrameReturn::<Method>(expr)` (needs `sym` + the
/// method = `event`); a body terminal is `return;` under the kernel but `return Default::default()`
/// under a scanner (needs `is_scan`); the kernel handler body indents 12, the scanner's 8.
///
/// It travels as ONE struct — not four positional params bolted onto every leaf — so a new
/// per-body fact is a new field, not a new argument on N signatures across four backends. It is a
/// bundle of borrows + a bool, cheap to build per leaf call. `python`/`java`/`c` read nothing from
/// it (their spellings do not vary on any of these facts), so its arrival leaves their bytes
/// exactly where they were.
#[derive(Clone, Copy)]
pub struct LeafCtx<'a> {
    /// The system being emitted — for `sym.name` (the `<Sys>` prefix) and `sym.states` (the
    /// parent chain a state-var read climbs).
    pub sym: &'a SystemSym,
    /// The interface method / handler event this body belongs to — the `<Method>` in a typed
    /// return slot. Empty for a non-handler body (an `actions:` member).
    pub event: &'a str,
    /// The state whose handler this is — the anchor a state-var read/write walks the compartment
    /// chain to find. Empty for a non-handler body.
    pub state: &'a str,
    /// Is the enclosing system a positioned `@@[scan]` recognizer? Thin dispatch model + 8-space
    /// body indent when true; kernel model + 12-space when false. `sym.scan.is_some()`.
    pub is_scan: bool,
}

impl<'a> LeafCtx<'a> {
    /// Build the context for `sym`'s `(state, event)` body. `is_scan` is derived once here so a
    /// leaf never re-derives it.
    pub fn new(sym: &'a SystemSym, event: &'a str, state: &'a str) -> Self {
        LeafCtx { sym, event, state, is_scan: sym.scan.is_some() }
    }
}

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

    /// The FILE's preamble, aware of whether the file contains a positioned `@@[scan]` recognizer.
    ///
    /// The kernel model (an ordinary system) carries no file-level preamble at all — legacy 4.6.1
    /// emits none — while Rust's thin `@@[scan]` model (whose 24 self-hosted `.gen.rs` are
    /// byte-frozen) opens with `use std::collections::HashMap; use std::any::Any;`. So on Rust the
    /// preamble is present IFF the file scans. Every other target's preamble does not vary on this,
    /// so the default forwards to [`Self::file_header`] and their bytes are unchanged.
    fn file_header_ctx(&self, has_scan: bool, out: &mut Sink) {
        let _ = has_scan;
        self.file_header(out);
    }

    /// The class opening.
    fn open_system(&self, sym: &SystemSym, out: &mut Sink);
    /// Close the class.
    fn close_system(&self, sym: &SystemSym, out: &mut Sink);

    /// Spell ONE domain field's **constructor initializer** — `sym.domain[idx]`, seeded into the
    /// generated constructor body.
    ///
    /// Split out of [`Self::open_system`] so the `for f in &sym.domain` loop could become the
    /// [`super::domain_init_walk`] `@@system`: the WALK is framec's (one cursor, one bound, one
    /// halt), the SPELLING is the target's. Default: nothing — a target that seeds its domain some
    /// other way, or not at all, is byte-unchanged by the walk's existence.
    fn domain_init(&self, sym: &SystemSym, idx: usize, out: &mut Sink) {
        let _ = (sym, idx, out);
    }

    /// Spell ONE `_HSM_CHAIN`-style entry: the ROOT..LEAF state path for leaf state `leaf`.
    ///
    /// `chain` is root-first and inclusive of the leaf — a flat state is `[leaf]`, a nested
    /// `$Child => $Parent` is `[.., "Parent", "Child"]`. The ancestor climb is framec's
    /// ([`super::hsm_chain_walk`]); the table's spelling is the target's. Default: nothing.
    fn hsm_chain_entry(&self, leaf: &str, chain: &[String], out: &mut Sink) {
        let _ = (leaf, chain, out);
    }

    /// Spell ONE arm of the runtime state ROUTER — "if the live compartment is in `state`, hand the
    /// event to that state's dispatcher". `first` is `true` for the leading arm (the `if` rather
    /// than the `elif`/`else if`), a fact the WALK carries so the spelling never has to re-derive
    /// it from what it already wrote. `sym` comes along because whether the call it spells must be
    /// awaited is a property of the SYSTEM, not of the arm. Default: nothing.
    fn router_arm(&self, sym: &SystemSym, state: &str, first: bool, out: &mut Sink) {
        let _ = (sym, state, first, out);
    }

    /// Spell ONE state's **message dispatcher** — the private method the router hands an event to,
    /// which matches the event's message against the handlers this state declares and calls the
    /// matching `(state, handler)` method.
    ///
    /// `arms` are the state's handled event messages, in declaration order, resolved from the
    /// symbol table by the [`super::state_dispatch_walk`] `@@system`. As with [`Self::route`], the
    /// backend only spells the switch; it never walks the table. Default: nothing — a target whose
    /// router dispatches directly to `(state, event)` methods (Java, Rust, C) needs no such layer
    /// and is byte-unchanged.
    fn dispatch(&self, sym: &SystemSym, state: &str, arms: &[String], out: &mut Sink) {
        let _ = (sym, state, arms, out);
    }

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

    /// Where does `=> $^` go — the state's DECLARED parent, or the nearest ancestor that
    /// actually handles the event?
    ///
    /// # The behavior is UNIVERSAL. The default is `false` anyway, and this is why.
    ///
    /// **Classification (measured, not assumed): the DECLARED parent, in every target.** `=> $^`
    /// shifts the live compartment by exactly ONE level and hands the event to that state's
    /// *dispatcher*, so the callee is always exactly one level up. It does NOT climb to the nearest
    /// ancestor that handles the event. Measured against the 4.6.1 oracle on
    /// `$Child => $Mid => $Root` where `$Mid` declares NO handler for `ev` and `$Root` does:
    ///
    /// ```text
    /// python  self._state_Mid(__e, compartment.parent_compartment)
    /// java    _state_Mid(__e, compartment.parent_compartment);
    /// rust    self._state_Mid(__e);
    /// c       Fwd3_state_Mid(self, __e, compartment->parent_compartment);
    /// ```
    ///
    /// `$Mid`'s dispatcher has no arm for `ev`, so the event stops there — legacy never reaches
    /// `$Root`. Resolving the ancestor instead ([`crate::resolve::SymbolTable::resolve_forward`])
    /// reroutes a 3-deep HSM to a state legacy never calls.
    ///
    /// # Why it is nevertheless still opt-in
    ///
    /// The DECISION is universal; the SPELLING is not yet. Legacy calls the parent's DISPATCHER
    /// with the shifted compartment — which is exactly what [`super::python::Python::forward`]
    /// spells. ng's java, rust and c [`Self::forward`] spell the parent's HANDLER METHOD
    /// (`{owner}_{event}`), because those backends have no dispatcher layer yet
    /// ([`Self::dispatch`] is the no-op default there).
    ///
    /// So flipping the default on those three emits the RIGHT construct in the WRONG spelling, and
    /// the faithfulness ratchet counts that as a regression. Measured over the full positive corpus
    /// (rust 342 / java 327 / c 346 divergent fixtures), flipping this alone moves total differing
    /// lines by **+44 rust, +42 java, +61 c** — and, decisively, the legacy-only (`<`) half of that
    /// diff does not move AT ALL (121354/85976/241756 before and after). ng recovers zero legacy
    /// lines and adds unmatched ones.
    ///
    /// The mechanism, on `data_types/dict_ops` (`$A => $P`, `$P` handles nothing):
    ///
    /// ```text
    /// legacy java   _state_P(__e, compartment.parent_compartment);
    /// ng, resolve   (nothing — no ancestor handles `e`, so `forward_no_parent` fires)
    /// ng, declared  P_e();
    /// ```
    ///
    /// Note what the ratchet is hiding: on `resolve`, **the `=> $^` silently VANISHES**. That is
    /// strictly worse code than `P_e();`, and the line metric prefers it, because emitting nothing
    /// costs one unmatched legacy line while emitting the wrong spelling costs two.
    ///
    /// **VOID CONDITION — flip this default to `true` and delete the Python override the moment
    /// java/rust/c gain the dispatcher layer** (a `_state_<Name>` method) and their [`Self::forward`]
    /// spells `_state_<parent>(__e, compartment.parent_compartment)`. At that point the same flip
    /// removes lines from BOTH halves of the diff. Until then the universal decision is held here in
    /// one place, documented, rather than re-derived per backend.
    fn forward_to_declared_parent(&self) -> bool {
        false
    }

    /// Are a state's handlers emitted in HANDLER-KEY order rather than declaration order?
    ///
    /// The shipped compiler holds them in a `HashMap` and sorts by key for determinism, so its
    /// dispatch arms and private handler methods both come out alphabetically with the lifecycle
    /// pair (exit, then enter) first — see [`handler_sort_key`]. ng's walk is over the TREE, which
    /// is in declaration order, so the projection has to be applied.
    ///
    /// **Default `true` — this is a UNIVERSAL legacy behavior, not a per-language spelling.** The
    /// sort happens in legacy's arcanum, before any backend is consulted, so every target sees the
    /// same order. Measured against the 4.6.1 oracle on a state declaring `$>`, `zebra`, `alpha`,
    /// `mango`, `<$` — all four of python_3, java, rust and c emit, for BOTH
    /// the dispatch arms and the private handler methods:
    ///
    /// ```text
    /// _s_S_hdl_frame_exit  _s_S_hdl_frame_enter  _s_S_hdl_user_alpha
    /// _s_S_hdl_user_mango  _s_S_hdl_user_zebra
    /// ```
    ///
    /// i.e. exit-before-enter, then user events alphabetically — exactly [`handler_sort_key`], in
    /// every target. (The PUBLIC interface methods stay in declaration order in all four; only the
    /// state's handlers are keyed.) Gating this per backend would make four targets re-derive one
    /// legacy fact four times, which is how the shipped compiler's seventeen arms drifted.
    fn orders_handlers_by_key(&self) -> bool {
        true
    }

    /// Does the `\n` that terminates a system's closing `}` line belong to the SYSTEM?
    ///
    /// **Default `true` — the shipped compiler says yes for EVERY target.** A system's emission is
    /// already newline-terminated, so it consumes that byte rather than letting the water that
    /// follows re-emit it. ng's partition puts the byte in the following native item, which is what
    /// the totality gate pins, so the correction is applied here, at the boundary's consumer.
    ///
    /// This looked per-target because the four SYSTEM TAILS differ (java `}\n}\n`, c `{\n}\n\n`,
    /// rust `…_framec::*;\n`, python `pass\n`). Those are [`Self::close_system`] SPELLINGS and are
    /// invariant under the boundary. The boundary RULE itself was measured against the 4.6.1 oracle
    /// by varying ONLY the newline run after `}` in one otherwise-identical source:
    ///
    /// | source after `}` | python | java | rust | c |
    /// |---|---|---|---|---|
    /// | `}\nWATER`     | 0 blank | 0 blank | 0 blank | 0 blank |
    /// | `}\n\nWATER`   | 1 blank | 1 blank | 1 blank | 1 blank |
    /// | `}\n\n\nWATER` | 2 blank | 2 blank | 2 blank | 2 blank |
    /// | `}   \nWATER`  | `   \n` kept | `   \n` kept | `   \n` kept | `   \n` kept |
    ///
    /// One rule, four targets: the `}` line's OWN terminator is the system's; every further blank
    /// line is the water's; and when the water does not start with the newline (trailing spaces),
    /// nothing is consumed. (C's extra `\n` is constant across all four rows — it is part of C's
    /// own close spelling, not the boundary.)
    fn consumes_close_brace_newline(&self) -> bool {
        true
    }

    /// Does a `@@:(expr)` / `@@:return(expr)` END the body — i.e. may the walk stop and drop every
    /// statement after it?
    ///
    /// # The behavior is UNIVERSAL (`false` everywhere). The default is `true` anyway, and this is why.
    ///
    /// **Classification (measured, not assumed): legacy NEVER treats this construct as a body
    /// terminal, in any target or either role.** The prior belief — "java/rust/c spell it as a real
    /// `return`, so it is terminal; only Python differs" — is refuted on both halves.
    ///
    /// A HANDLER body `@@:(1); after_one(); after_two()`, against the 4.6.1 oracle:
    ///
    /// ```text
    /// python  self._context_stack[-1]._return = 1              after_one()  after_two()
    /// java    _context_stack.get(…)._return = 1;               after_one()  after_two();
    /// rust    let __return_val = …; ctx._return = Some(…);     after_one()  after_two();
    /// c       RetT_CTX(self)->_return = (void*)(intptr_t)(1);  after_one()  after_two();
    /// ```
    ///
    /// No target spells a `return` in a handler — all four park the value on the live
    /// `FrameContext` and keep running. And in an ACTION body, where all four DO spell a real
    /// `return 7;`, legacy STILL emits the following `after_one()`. Neither role, no target.
    ///
    /// Latching it terminal SILENTLY DELETES LIVE CODE. On `linux/01_process_lifecycle`, a handler
    /// reads `@@:("forked")` then `-> $Ready`; legacy emits the transition
    /// (`__prepareEnter("Ready", …); __transition(…); return;`) and ng, latched, **drops the
    /// transition entirely** — the state machine stops moving.
    /// `tests/legacy_bug_fixes.rs::ng_bug_statements_after_frame_return_still_run` runs the emitted
    /// program and proves the statements execute.
    ///
    /// # Why it is nevertheless still opt-in
    ///
    /// Same shape as [`Self::forward_to_declared_parent`]: the DECISION is universal, the SPELLING
    /// of what then gets emitted is not yet. Flipping this alone moves total differing lines by
    /// **+1525 rust, +1430 java, +1570 c**, while the legacy-only (`<`) half barely moves
    /// (−9 / −2 / −21). ng now emits the statements legacy emits, but spells them its own way
    /// (`ReadyComp __next = new ReadyComp(); …` vs legacy's `__prepareEnter("Ready", …)`), so each
    /// recovered statement costs a `>` without buying back a `<`.
    ///
    /// About a fifth of the added lines are not even that: they are a spurious fallback return
    /// (java `return null;`, rust `Default::default()`, c `return (t){0};`) emitted by
    /// [`Self::close_handler`] because `terminated` is now false. **That is one bit doing two
    /// jobs**, and legacy answers the two differently: *"may the walk stop?"* — NO; *"does the body
    /// need a fallback return?"* — also NO (legacy's handlers are `void` and its actions get no
    /// fallback after a `@@:(expr)`). Splitting [`BodyEnd`] into a `halt` fact and a
    /// `needs_fallback` fact is the structurally correct fix and removes that fifth outright.
    ///
    /// **VOID CONDITION — flip this default to `false` and delete the Python override once java,
    /// rust and c spell transitions/returns the legacy way** (the `__prepareEnter`/`__transition`
    /// kernel model, handlers `void`). At that point the flip removes lines from both halves.
    ///
    /// (A transition / push / pop IS still a body terminal — a different construct, and legacy's own
    /// `strip_java_unreachable` existed precisely because it *is* terminal.)
    fn return_call_terminates(&self) -> bool {
        true
    }

    /// Are `actions:` members emitted **before** `operations:` members, regardless of the order
    /// the two sections appear in the source?
    ///
    /// Frame's canonical block order is `operations:, interface:, machine:, actions:, domain:`
    /// (E113), so `operations:` comes FIRST in every well-formed source — and yet the shipped
    /// compiler emits actions first. It holds the two in separate arcanum collections and runs two
    /// passes; source order never reaches the emitter.
    ///
    /// **Default `true` — universal.** The two-pass emission is backend-agnostic in legacy (the
    /// split happens in the arcanum, before any target is chosen). Measured against the 4.6.1
    /// oracle on a system declaring `operations: op_one, op_two` FIRST and `actions: act_one,
    /// act_two` LAST — all four of python_3, java, rust and c emit
    /// `act_one, act_two, op_one, op_two`. Also verified on `demos/23_vending_machine`.
    ///
    /// ng's walk is over the TREE, which is in declaration order, so the two-phase admission
    /// ([`action_section_in_phase`]) has to be applied.
    fn orders_actions_before_operations(&self) -> bool {
        true
    }

    /// When a body has **no executable statement** (empty, or only comments — [`body_is_empty`]),
    /// is the body's text still emitted before the [`Self::noop`]?
    ///
    /// The shipped compiler says NO: its body model was a list of statement SEGMENTS, and a
    /// comment was never a segment, so a comment-only body reached the emitter as nothing at all
    /// and came out as a bare `pass`. ng's tree carries the comment (it is a `NativePart::Literal`
    /// with `is_comment`), so it would emit `# …` *and* the `pass`. Both are valid Python; only one
    /// is byte-identical to legacy.
    ///
    /// Default `true` — every target that has not yet been driven to byte-faithfulness keeps its
    /// current bytes. Python opts out.
    fn empty_body_keeps_text(&self) -> bool {
        true
    }

    /// `=> $^` in a state with **no parent at all**. Legacy spells this a comment-tagged bare
    /// `return`; ng's default is [`Self::noop`], which is what every not-yet-faithful backend
    /// keeps.
    fn forward_no_parent(&self, rel: u32, out: &mut Sink) {
        self.noop(rel, out);
    }

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
    fn close_handler(&self, ret: Option<&str>, is_async: bool, terminated: bool, ctx: &LeafCtx, out: &mut Sink);

    /// The indentation prefix for a statement `rel` columns deeper than the body's base.
    ///
    /// Java returns a constant — braces carry the nesting, so the layout is cosmetic.
    /// Python **must** reproduce it: a `@@:return` inside an `if x:` has to be indented
    /// under it, or the file is a SyntaxError.
    ///
    /// The driver computes `rel` from the source columns the scanner recorded. What to
    /// do with it is a SPELLING, and it lives here.
    fn pad(&self, rel: u32) -> String;

    /// The indentation prefix, aware of the **body model** (`is_scan`). A kernel-model handler
    /// body sits one `impl`-level deeper than a scanner's thin dispatch method, so its base
    /// column differs. Default forwards to [`Self::pad`] — every target whose base does not vary
    /// on the model (python/java/c, and Rust's own scanner path) is byte-unchanged by this
    /// method's existence. Rust overrides it for the kernel's 12-space base.
    fn pad_ctx(&self, rel: u32, is_scan: bool) -> String {
        let _ = is_scan;
        self.pad(rel)
    }

    /// Emit a native statement, already lowered and re-indented.
    fn native_stmt(&self, rel: u32, text: crate::NativeText, ctx: &LeafCtx, out: &mut Sink);

    /// The re-indent basis for a statement — a native statement (`x = @@:self.g()`) or a Frame
    /// assignment (`@@:self.x = @@:self.g()`) — that **bears a `@@:self.<method>()` self interface
    /// call**, whose reentrancy guard follows it.
    ///
    /// Most targets measure it like any other statement — relative to the body base (the default).
    /// Rust's KERNEL handler reproduces the shipped compiler's quirk: such a statement is **not**
    /// base-subtracted, so it lands at `target base + the full source column`, and its guard sits at
    /// the same indent. A scan system (no kernel, no self-call) keeps the base-relative basis, so
    /// its byte-frozen `.gen.rs` are unmoved. `col` is the statement's source column; `base` the
    /// body's shallowest.
    fn selfcall_stmt_rel(&self, col: u32, base: u32, is_scan: bool) -> u32 {
        let _ = is_scan;
        col.saturating_sub(base)
    }

    /// `-> $Target(args)` — build and install the next compartment, then return.
    fn transition(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink);
    /// `push$ -> $Target(args)`
    fn push(&self, rel: u32, sym: &SystemSym, target: &str, args: Option<&str>, out: &mut Sink);
    /// `-> pop$` — restore the caller's compartment. **No return** (the driver adds it).
    fn pop(&self, rel: u32, out: &mut Sink);

    /// `-> (enter_args) $Target(state_args)` — the ENTER-ARG-AWARE form of [`Self::transition`].
    ///
    /// Two arg lists reach a transition and they belong to different things: the STATE args
    /// parameterise the destination compartment, the ENTER args are the payload of the `$>` event
    /// the destination receives. A target that delivers that payload as a separate call on the
    /// enter handler ([`Self::lifecycle_call`]) needs only the first, and takes the default below —
    /// byte-unchanged. A target whose runtime builds the destination compartment through ONE
    /// factory (Python's `__prepareEnter(leaf, state_args, enter_args)`, where the kernel later
    /// synthesises the `$>` from the compartment) must receive both at once, and overrides this.
    ///
    /// Additive on purpose: the driver calls THIS, the default forwards to [`Self::transition`],
    /// and the enter args still flow to `lifecycle_call` exactly as before. No existing backend's
    /// bytes move.
    #[allow(clippy::too_many_arguments)]
    fn transition_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        let _ = enter_args;
        self.transition(rel, sym, target, args, out);
    }

    /// `push$ -> (enter_args) $Target(state_args)` — the enter-arg-aware form of [`Self::push`].
    /// Same contract as [`Self::transition_with_enter`], same default.
    #[allow(clippy::too_many_arguments)]
    fn push_with_enter(
        &self,
        rel: u32,
        sym: &SystemSym,
        target: &str,
        args: Option<&str>,
        enter_args: Option<&str>,
        out: &mut Sink,
    ) {
        let _ = enter_args;
        self.push(rel, sym, target, args, out);
    }
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
    fn terminate(&self, rel: u32, ctx: &LeafCtx, out: &mut Sink);

    /// **`@@:return(<expr>)`** — set the return value and exit. Terminal.
    ///
    /// `role` says WHICH method this body belongs to ([`BodyRole`]), because the construct does
    /// not mean the same thing in both. In a `machine:` handler the value is parked on the live
    /// `FrameContext` (dynamic targets) and read back by the public wrapper; in an
    /// `actions:`/`operations:` member there IS no live context — the user may call the method
    /// directly — so the only correct spelling is the target's own `return`. Targets that already
    /// spell `@@:(expr)` as a real `return` (Java, Rust, C) ignore the tag.
    ///
    /// `multiline` is a POSITION fact about the SOURCE expression: it spans more than
    /// one line. Indent-continuation targets (Python) must wrap the RHS in parens so
    /// the continuation lines are legal; brace/`;` targets ignore it. The decision is
    /// made where the source + span live (never by inspecting the opaque native text).
    fn return_call(&self, role: BodyRole, rel: u32, is_async: bool, multiline: bool, expr: crate::NativeText, ctx: &LeafCtx, out: &mut Sink);

    /// **`@@:self.method(<args>)`** — a reentrant interface call. framec authored it, so
    /// framec terminates it.
    fn self_call(&self, rel: u32, is_async: bool, method: &str, args: &str, out: &mut Sink);

    /// The **reentrancy guard** emitted immediately AFTER a `@@:self.<method>()` self interface
    /// call — bare (`@@:self.g()`) or in expression position (`x = @@:self.g()`).
    ///
    /// A self interface call re-enters dispatch and may itself transition the machine; if it did,
    /// the calling handler must bail before its remaining statements run in the wrong state. The
    /// shipped compiler emitted this per target at the call site (its `self_call_guard.rs`). `rel`
    /// is the call statement's own indentation (the same basis the statement was emitted at).
    ///
    /// Default **no-op**: a target with no `_transitioned` guard — or one not yet on the kernel
    /// model here (Java, C) — emits nothing and is byte-unmoved. Kernel targets that carry a
    /// `_context_stack` (Rust, Python) spell the read-and-bail.
    fn reentrancy_guard(&self, rel: u32, ctx: &LeafCtx, out: &mut Sink) {
        let _ = (rel, ctx, out);
    }

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

/// Emit every item in the file. **This function has no `Target`.**
///
/// The whole-file walk — the `file_header` preamble, then per item either the "water" (top-level
/// native code, verbatim) or a system's phase spine — is reified as the outermost
/// [`super::emit_file`] `@@system` (`EmitFile`), whose `walk` is this function's body. With it
/// landed, the entire emit driver from the file down through each system's phases, handlers, and
/// statements runs through `@@system`s. The byte-for-byte oracle it replaced is preserved as
/// [`emit_file_hand`], gated in `tests/emit_file.rs` (GATE-A, via [`file_parity_report`]).
pub fn emit(src: &Source, ast: &FileAst, syms: &SymbolTable, be: &dyn Backend) -> String {
    super::emit_file::walk(src, ast, syms, be)
}

/// The shipped compiler's **handler ordering key** for one event message.
///
/// Legacy holds a state's handlers in a `HashMap<String, HandlerEntry>` and, for determinism,
/// sorts them by that KEY before emitting either the dispatch arms or the private handler methods
/// (`state_dispatch.rs:141` and `:348`, `sorted_handlers.sort_by_key(|(event, _)| *event)`). Two
/// consequences, both of which ng must reproduce and neither of which is source order:
///
/// * user events come out **alphabetically** (`zz aa mm bb` declared → `aa bb mm zz` emitted);
/// * the lifecycle pair comes out **exit first**, because the arcanum's exit KEY is `$<` even
///   though its wire MESSAGE is `<$` — and `$<` < `$>` bytewise. Sorting the wire messages
///   directly would put enter first and be wrong, which is why this mapping exists.
///
/// Everything sorts after the two `$`-prefixed lifecycle keys, since `$` is 0x24.
pub(super) fn handler_sort_key(event: &str) -> &str {
    match event {
        "<$" => "$<",
        e => e,
    }
}

/// Does item `i` begin at the byte after a **system's** closing `}`?
///
/// A purely STRUCTURAL question over facts framec put in the tree (the kind of the previous
/// item) — no byte of the user's text is examined to answer it. It exists because legacy
/// assigns the `}` line's terminator to the system while ng's partition assigns it to the
/// water; see [`super::reindent::render_water`]. Shared by the [`super::emit_file`] machine
/// and the [`emit_file_hand`] oracle so both make the same call.
pub(super) fn prev_item_is_system(ast: &FileAst, i: usize) -> bool {
    i > 0 && matches!(ast.items.get(i - 1), Some(Item::System(_)))
}

/// Render ONE top-level native item — **the water** — into `out`.
///
/// Native code outside a system is the USER'S code and passes through VERBATIM (the Oceans model;
/// leaving it out meant every type the user defined alongside their system silently vanished). The
/// one exception is `@@SystemName(...)` islands (spec §1103), Frame's own syntax even out here, which
/// lower to the target constructor. There is no compartment at top level, so a plain ref/embed cannot
/// legally occur; either renders as its original text.
///
/// Shared by the [`super::emit_file`] machine's `emit_native_item` leaf AND the preserved
/// [`emit_file_hand`] oracle, so the two whole-file paths differ ONLY in how the item loop is
/// sequenced (the exact SPELLING of a water item is one function, gated once).
///
/// `after_system` carries the ONE boundary fact emission needs: this water begins at the byte
/// after a system's closing `}`, so its leading line terminator is that `}`'s, not the water's.
/// See [`super::reindent::render_water`].
pub(super) fn render_native_item(
    src: &Source,
    syms: &SymbolTable,
    be: &dyn Backend,
    n: &crate::tree::NativeItem,
    after_system: bool,
    out: &mut Sink,
) {
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
    out.native(super::reindent::render_water(
        src,
        &n.parts,
        n.span,
        &lower,
        after_system && be.consumes_close_brace_newline(),
    ));
}

/// The preserved byte-for-byte **oracle** for the driver's TOP-LEVEL ITEM WALK — the original
/// `file_header` + item loop [`emit`] ran before it was reified as the [`super::emit_file`]
/// `@@system` (`EmitFile`). Kept as the differential check that machine is proven against (GATE-A,
/// `tests/emit_file.rs`, via [`file_parity_report`]). It calls the SAME shared [`render_native_item`]
/// and the SAME landed [`super::emit_system::walk`] the machine's leaves call — the two paths differ
/// only in how the item loop is SEQUENCED (hand loop vs `$Item` cycle), which is exactly what the
/// gate isolates. Doc-hidden and **not on the production path**. Do not edit it to add behavior: it
/// exists only to reproduce the pre-conversion sequencing exactly, so any divergence is the machine's
/// bug, not the oracle's.
#[doc(hidden)]
fn emit_file_hand(src: &Source, ast: &FileAst, syms: &SymbolTable, be: &dyn Backend) -> String {
    let mut out = Sink::new();
    be.file_header_ctx(syms.systems.iter().any(|s| s.scan.is_some()), &mut out);
    for (i, item) in ast.items.iter().enumerate() {
        if let Item::Native(n) = item {
            render_native_item(src, syms, be, n, prev_item_is_system(ast, i), &mut out);
            continue;
        }
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        super::emit_system::walk(src, syms, sym, &sys.sections, be, &mut out);
    }
    out.finish()
}

/// TEST-ONLY (GATE-A) — the whole file's dual emission (machine top walk vs hand oracle), for
/// `tests/emit_file.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct FileParity {
    /// Text the `EmitFile` machine path ([`super::emit_file::walk`], = the production [`emit`]) emits
    /// for the WHOLE file.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`emit_file_hand`]) emits for the same.
    pub hand_text: String,
    /// How many items the file walked — so the test can prove the corpus exercised multi-item files
    /// (systems, water, and skippable items interleaved), not a single trivial system.
    pub item_count: usize,
    /// How many of those items were top-level native "water" — so the test can prove the water arm of
    /// the `$Item` fork was actually taken (a corpus of bare systems would leave it unproven).
    pub native_count: usize,
}

/// TEST-ONLY (GATE-A). Emit the WHOLE file through BOTH the `EmitFile` machine
/// ([`super::emit_file::walk`], the production path) and the preserved hand oracle
/// ([`emit_file_hand`]) — over the SAME parsed file and the SAME backend. `tests/emit_file.rs`
/// asserts `machine_text == hand_text` byte-for-byte. The library owns the emission and `.finish()`.
#[doc(hidden)]
pub fn file_parity_report(
    src: &Source,
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> FileParity {
    let native_count = ast
        .items
        .iter()
        .filter(|it| matches!(it, Item::Native(_)))
        .count();
    FileParity {
        machine_text: super::emit_file::walk(src, ast, syms, be),
        hand_text: emit_file_hand(src, ast, syms, be),
        item_count: ast.items.len(),
        native_count,
    }
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
    role: BodyRole,
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
        src, syms, sym, role, &body.stmts, state, event, is_async, base, be, seed,
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
    role: BodyRole,
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
    // `bool`. WHAT COUNTS as a terminal is the backend's ([`Backend::return_call_terminates`]):
    // a statement only ends the body if that target's spelling of it actually returns.
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

    // The per-body leaf context (`sym` / `event` / `state` / `is_scan`), constant across the walk.
    let ctx = LeafCtx::new(sym, event, state);

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
                // A native statement whose RHS embeds a `@@:self.<method>()` self interface call
                // (`x = @@:self.g()`) gets the reentrancy guard after it — the expression-position
                // twin of the bare `SelfCall`. Only in the KERNEL model. Kept in step with the
                // `stmt_walk::emit_native` leaf (GATE-A parity).
                let bears =
                    !ctx.is_scan && super::stmt_walk::bears_reentrant_self_call(&n.parts, sym);
                let r = if bears {
                    be.selfcall_stmt_rel(n.logical_indent, base, ctx.is_scan)
                } else {
                    rel(n.logical_indent)
                };
                let delta = be.pad_ctx(r, ctx.is_scan).len() as i32 - n.logical_indent as i32;
                let text = super::reindent::render_native(src, n, delta, &lower);
                be.native_stmt(r, text, &ctx, out);
                if bears {
                    be.reentrancy_guard(r, &ctx, out);
                }
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
                        let ea = super::reindent::render_args(src, t.exit_args.as_ref(), &lower);
                        be.lifecycle_call(r, sym, state, "<$", ea.as_deref(), out);
                    }
                    let sa = super::reindent::render_args(src, t.args_text.as_ref(), &lower);
                    let na = super::reindent::render_args(src, t.enter_args.as_ref(), &lower);
                    be.transition_with_enter(r, sym, target, sa.as_deref(), na.as_deref(), out);
                    if has_lifecycle(sym, target, "$>") {
                        be.lifecycle_call(r, sym, target, "$>", na.as_deref(), out);
                    }
                    be.terminate(r, &ctx, out);
                    terminated = t.depth == 0 && r == 0;
                }
            }
            Stmt::StackPush(t) => {
                if let Some(target) = &t.target {
                    let r = rel(t.col);
                    if has_lifecycle(sym, state, "<$") {
                        let ea = super::reindent::render_args(src, t.exit_args.as_ref(), &lower);
                        be.lifecycle_call(r, sym, state, "<$", ea.as_deref(), out);
                    }
                    let sa = super::reindent::render_args(src, t.args_text.as_ref(), &lower);
                    let na = super::reindent::render_args(src, t.enter_args.as_ref(), &lower);
                    be.push_with_enter(r, sym, target, sa.as_deref(), na.as_deref(), out);
                    if has_lifecycle(sym, target, "$>") {
                        be.lifecycle_call(r, sym, target, "$>", na.as_deref(), out);
                    }
                    be.terminate(r, &ctx, out);
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
                    let ea = super::reindent::render_args(src, st.exit_args.as_ref(), &lower);
                    be.lifecycle_call(r, sym, state, "<$", ea.as_deref(), out);
                }
                be.pop(r, out);
                // `-> (enter) pop$` — deliver the enter args to the RESTORED state's `$>`,
                // dispatched at runtime (the popped state is dynamic).
                if st.enter_args.is_some() {
                    let na = super::reindent::render_args(src, st.enter_args.as_ref(), &lower);
                    be.pop_enter(r, sym, na.as_deref(), out);
                }
                be.terminate(r, &ctx, out);
                terminated = st.depth == 0 && r == 0;
            }
            // A FRAME assignment. framec authored it, so framec terminates it — in the
            // backend's spelling, unconditionally, without ever looking at what it just
            // wrote.
            Stmt::Assign(a) => {
                // A Frame assignment whose RHS embeds a `@@:self.<method>()` self interface call
                // gets the reentrancy guard after it. Only in the KERNEL model. Kept in step with
                // the `stmt_walk::emit_assign` leaf (GATE-A parity).
                let bears =
                    !ctx.is_scan && super::stmt_walk::bears_reentrant_self_call(&a.rhs, sym);
                let r = if bears {
                    be.selfcall_stmt_rel(a.col, base, ctx.is_scan)
                } else {
                    rel(a.col)
                };
                let rhs = super::reindent::render_parts(src, &a.rhs, a.rhs_span, &lower);
                be.assign(sym, state, &a.lhs, rhs, r, out);
                if bears {
                    be.reentrancy_guard(r, &ctx, out);
                }
            }
            Stmt::ReturnCall(r) => {
                let e = super::reindent::render_parts(src, &r.expr, r.expr_span, &lower);
                let multiline = src.span_is_multiline(r.expr_span);
                be.return_call(role, rel(r.col), is_async, multiline, e, &ctx, out);
                // Terminal — but only if this target's spelling RETURNS, and only for the BODY
                // if it is at the base nesting.
                terminated =
                    be.return_call_terminates() && r.depth == 0 && rel(r.col) == 0;
            }
            Stmt::SelfCall(c) => {
                let a = super::reindent::render_args(src, Some(&c.args), &lower);
                be.self_call(rel(c.col), is_async, &c.method, a.as_deref().unwrap_or(""), out);
            }
            // `=> $^` — forward this event to the PARENT's handler. The driver knows
            // which state that is, because the symbol table knows the parent chain.
            Stmt::Forward(fwd) => {
                let target = if be.forward_to_declared_parent() {
                    sym.declared_parent(state)
                } else {
                    sym.resolve_forward(state, event)
                };
                if let Some(owner) = target {
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
                    // There is nowhere to forward TO — no parent (on a backend that forwards to
                    // the declared parent), or no ancestor handles the event (on one that
                    // resolves). Either way `=> $^` lowers to nothing, and the enclosing block
                    // must not be left empty: `pass` on python, nothing on brace targets.
                    be.forward_no_parent(rel(fwd.col), out);
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
            // Declaration order, unless the backend takes the shipped compiler's HANDLER-KEY
            // order ([`handler_sort_key`]). The machine path asks the same question through its
            // `member_slot` projection, so the two stay byte-identical under GATE-A.
            let mut handlers: Vec<&HandlerNode> = st
                .members
                .iter()
                .filter_map(|m| match m {
                    StateMember::Handler(h) => Some(h),
                    _ => None,
                })
                .collect();
            if be.orders_handlers_by_key() {
                handlers.sort_by(|a, b| {
                    handler_sort_key(&a.event).cmp(handler_sort_key(&b.event))
                });
            }
            for h in handlers {
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
                let empty = body_is_empty(&h.body);
                let end = if !empty || be.empty_body_keeps_text() {
                    emit_body(src, syms, sym, BodyRole::Handler, &st.name, &h.event, is_async, &h.body, be, out)
                } else {
                    BodyEnd::Fell
                };
                if empty {
                    be.noop(0, out);
                }
                be.close_handler(ret, is_async, end.terminated(), &LeafCtx::new(sym, &h.event, &st.name), out);
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

/// The preserved byte-for-byte **oracle** for the driver's INTERFACE/ROUTER pass — the original
/// `(method, arm)` nested loops [`emit`] ran before they were reified as the
/// [`super::emit_interface`] `@@system` (`EmitInterface`). Kept as the differential check that
/// machine is proven against (GATE-A, `tests/emit_interface.rs`, via [`interface_parity_report`]).
/// It calls the SAME `be.route` the machine's `route_method` leaf calls — the two paths differ only
/// in how the two-level walk is SEQUENCED (hand loops vs cycle states), which is exactly what the
/// gate isolates. Doc-hidden and **not on the production path**. Do not edit it to add behavior: it
/// exists only to reproduce the pre-conversion sequencing exactly, so any divergence is the
/// machine's bug, not the oracle's.
#[doc(hidden)]
fn emit_interface_hand(sym: &SystemSym, be: &dyn Backend, out: &mut Sink) {
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
            out,
        );
    }
}

/// TEST-ONLY (GATE-A) — one system's dual interface-emission (machine path vs hand oracle), for
/// `tests/emit_interface.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct InterfaceParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `EmitInterface` machine path ([`super::emit_interface::walk`]) emits for ALL of this
    /// system's public router methods.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`emit_interface_hand`]) emits for the same.
    pub hand_text: String,
    /// How many public router methods this system emits (one per interface event) — so the test can
    /// prove the corpus actually exercised multi-method / multi-state / HSM shapes (a system that
    /// emits zero routers is a vacuous pass).
    pub route_count: usize,
}

/// TEST-ONLY (GATE-A). Emit **every** system's public router methods through BOTH the
/// `EmitInterface` machine ([`super::emit_interface::walk`]) and the preserved hand oracle
/// ([`emit_interface_hand`]) — over the SAME real parsed systems and the SAME backend — and return,
/// per system, the two emitted Strings. `tests/emit_interface.rs` asserts, for every entry,
/// `machine_text == hand_text` byte-for-byte. The library owns the `.finish()` and the real emit
/// traversal.
#[doc(hidden)]
pub fn interface_parity_report(
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<InterfaceParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        let mut mo = super::Sink::default();
        super::emit_interface::walk(sym, be, &mut mo);
        let mut ho = super::Sink::default();
        emit_interface_hand(sym, be, &mut ho);
        report.push(InterfaceParity {
            label: sym.name.clone(),
            machine_text: mo.finish(),
            hand_text: ho.finish(),
            route_count: sym.interface.len(),
        });
    }
    report
}

/// The preserved byte-for-byte **oracle** for the driver's ACTIONS/OPERATIONS pass — the original
/// `(section, member)` nested loops [`emit`] ran before they were reified as the
/// [`super::emit_actions`] `@@system` (`EmitActions`). Kept as the differential check that machine
/// is proven against (GATE-A, `tests/emit_actions.rs`, via [`actions_parity_report`]). It calls the
/// SAME production [`emit_body`] the machine's `emit_action` leaf calls — the two paths differ only
/// in how the two-level walk is SEQUENCED (hand loops vs cycle states), which is exactly what the
/// gate isolates. Doc-hidden and **not on the production path**. Do not edit it to add behavior: it
/// exists only to reproduce the pre-conversion sequencing exactly, so any divergence is the
/// machine's bug, not the oracle's.
#[doc(hidden)]
fn emit_actions_hand(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    // Two PASSES when the target orders actions before operations
    // ([`Backend::orders_actions_before_operations`]), one otherwise. `nphase == 1` admits both
    // kinds in source order — the pre-change behavior, and what every target that has not opted in
    // still gets.
    let nphase = if be.orders_actions_before_operations() { 2 } else { 1 };
    for phase in 0..nphase {
    for sec in sections {
        let Some(d) = action_section_in_phase(sec, phase, nphase) else {
            continue;
        };
        for m in &d.members {
            let Decl::WithBody(b) = m else { continue };
            be.open_action(&b.name, &b.params_text, b.return_text.as_deref(), out);
            let empty = body_is_empty(&b.body);
            if !empty || be.empty_body_keeps_text() {
                emit_body(src, syms, sym, BodyRole::Action, "", "", false, &b.body, be, out);
            }
            if empty {
                be.noop(0, out);
            }
            be.close_action(out);
        }
    }
    }
}

/// Which `Decl` group does `sec` contribute in pass `phase` of `nphase`?
///
/// `nphase == 1` — one pass, both kinds admitted in SOURCE order (the pre-change behavior; every
/// target that has not opted into [`Backend::orders_actions_before_operations`]).
/// `nphase == 2` — pass 0 admits only `actions:`, pass 1 only `operations:`, so actions come out
/// first whatever order the two sections were declared in.
///
/// Shared by the [`emit_actions_hand`] oracle and by the `EmitActions` machine's fork leaf, so the
/// two sequencings cannot drift on WHICH sections they admit (GATE-A isolates only HOW they are
/// walked).
pub(super) fn action_section_in_phase(
    sec: &Section,
    phase: usize,
    nphase: usize,
) -> Option<&crate::tree::DeclSection> {
    match (sec, nphase, phase) {
        (Section::Actions(d), 1, _) | (Section::Operations(d), 1, _) => Some(d),
        (Section::Actions(d), _, 0) => Some(d),
        (Section::Operations(d), _, 1) => Some(d),
        _ => None,
    }
}

/// TEST-ONLY (GATE-A) — one system's dual actions-emission (machine path vs hand oracle), for
/// `tests/emit_actions.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct ActionsParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `EmitActions` machine path ([`super::emit_actions::walk`]) emits for ALL of this
    /// system's `actions:`/`operations:` methods.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`emit_actions_hand`]) emits for the same.
    pub hand_text: String,
    /// How many `actions:`/`operations:` methods this system emits — so the test can prove the
    /// corpus actually exercised multi-member / actions-plus-operations shapes (a system that emits
    /// zero actions is a vacuous pass).
    pub action_count: usize,
}

/// TEST-ONLY (GATE-A). Emit **every** system's `actions:`/`operations:` methods through BOTH the
/// `EmitActions` machine ([`super::emit_actions::walk`]) and the preserved hand oracle
/// ([`emit_actions_hand`]) — over the SAME real parsed systems and the SAME backend — and return,
/// per system, the two emitted Strings. `tests/emit_actions.rs` asserts, for every entry,
/// `machine_text == hand_text` byte-for-byte. The library owns the `.finish()` and the real emit
/// traversal.
#[doc(hidden)]
pub fn actions_parity_report(
    src: &Source,
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<ActionsParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        let mut mo = super::Sink::default();
        super::emit_actions::walk(src, syms, sym, &sys.sections, be, &mut mo);
        let mut ho = super::Sink::default();
        emit_actions_hand(src, syms, sym, &sys.sections, be, &mut ho);
        report.push(ActionsParity {
            label: sym.name.clone(),
            machine_text: mo.finish(),
            hand_text: ho.finish(),
            action_count: count_actions(&sys.sections),
        });
    }
    report
}

/// TEST-ONLY (GATE-A) — the number of `actions:`/`operations:` methods `sections` emits, the
/// coverage tally [`actions_parity_report`] reports so a vacuous (zero-action) system cannot pass as
/// covered. Same two-level structural walk the machine and the oracle share.
#[doc(hidden)]
fn count_actions(sections: &[Section]) -> usize {
    let mut n = 0;
    for sec in sections {
        let (Section::Actions(d) | Section::Operations(d)) = sec else {
            continue;
        };
        for m in &d.members {
            if let Decl::WithBody(_) = m {
                n += 1;
            }
        }
    }
    n
}

/// The preserved byte-for-byte **oracle** for the driver's PER-SYSTEM PHASE RUN — the original
/// `open_system` → interface → dispatch → handlers → actions → persist-guard → `close_system` sequence [`emit`]
/// ran inline before it was reified as the [`super::emit_system`] `@@system` (`EmitSystem`). Kept as
/// the differential check that machine is proven against (GATE-A, `tests/emit_system.rs`, via
/// [`system_parity_report`]). It calls the SAME already-landed sub-system machines
/// ([`super::emit_interface::walk`], [`super::state_dispatch_walk::walk`],
/// [`super::emit_handlers::walk`], [`super::emit_actions::walk`]) and the SAME `be.persist` the
/// machine's phase leaves call — the two paths differ only in how the five phases are SEQUENCED
/// (inline calls vs spine states), which is exactly what the gate
/// isolates. Doc-hidden and **not on the production path**. Do not edit it to add behavior: it
/// exists only to reproduce the pre-conversion sequencing exactly, so any divergence is the
/// machine's bug, not the oracle's.
#[doc(hidden)]
fn emit_system_hand(
    src: &Source,
    syms: &SymbolTable,
    sym: &SystemSym,
    sections: &[Section],
    be: &dyn Backend,
    out: &mut Sink,
) {
    be.open_system(sym, out);
    super::emit_interface::walk(sym, be, out);
    super::state_dispatch_walk::walk(sym, be, out);
    super::emit_handlers::walk(src, syms, sym, sections, be, out);
    super::emit_actions::walk(src, syms, sym, sections, be, out);
    let manifest = super::persist::PersistManifest::derive(sym, syms);
    if manifest.enabled {
        be.persist(&manifest, out);
    }
    be.close_system(sym, out);
}

/// TEST-ONLY (GATE-A) — one system's dual whole-system emission (machine phase spine vs hand
/// oracle), for `tests/emit_system.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct SystemParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `EmitSystem` machine path ([`super::emit_system::walk`]) emits for the WHOLE system
    /// (open → interface → dispatch → handlers → actions → persist → close).
    pub machine_text: String,
    /// Text the preserved hand oracle ([`emit_system_hand`]) emits for the same.
    pub hand_text: String,
    /// Whether `@@[persist]` was in force for this system — so the test can prove the corpus
    /// exercised BOTH the persist-enabled `$Persist` arm and the guarded skip (a corpus that never
    /// enabled persist would leave the guarded arm unproven).
    pub persist_enabled: bool,
}

/// TEST-ONLY (GATE-A). Emit **every** system through BOTH the `EmitSystem` phase spine
/// ([`super::emit_system::walk`]) and the preserved hand oracle ([`emit_system_hand`]) — over the
/// SAME real parsed systems and the SAME backend — and return, per system, the two emitted Strings.
/// `tests/emit_system.rs` asserts, for every entry, `machine_text == hand_text` byte-for-byte. The
/// library owns the `.finish()` and the real emit traversal.
#[doc(hidden)]
pub fn system_parity_report(
    src: &Source,
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<SystemParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        let mut mo = super::Sink::default();
        super::emit_system::walk(src, syms, sym, &sys.sections, be, &mut mo);
        let mut ho = super::Sink::default();
        emit_system_hand(src, syms, sym, &sys.sections, be, &mut ho);
        report.push(SystemParity {
            label: sym.name.clone(),
            machine_text: mo.finish(),
            hand_text: ho.finish(),
            persist_enabled: super::persist::PersistManifest::derive(sym, syms).enabled,
        });
    }
    report
}

/// The preserved byte-for-byte **oracle** for the constructor's DOMAIN-INIT walk — the original
/// `for f in &sym.domain { … }` loop `open_system` ran inline before it was reified as the
/// [`super::domain_init_walk`] `@@system` (`DomainInitWalk`). Kept as the differential check that
/// machine is proven against (GATE-A, `tests/domain_init_walk.rs`, via
/// [`domain_init_parity_report`]). It calls the SAME [`Backend::domain_init`] spelling the machine's
/// `stamp_domain_init` leaf calls — the two paths differ only in how the field loop is SEQUENCED
/// (hand loop vs `$Field` cycle), which is exactly what the gate isolates. Doc-hidden and **not on
/// the production path**.
#[doc(hidden)]
fn domain_init_hand(sym: &SystemSym, be: &dyn Backend) -> String {
    let mut out = Sink::new();
    for idx in 0..sym.domain.len() {
        be.domain_init(sym, idx, &mut out);
    }
    out.finish()
}

/// TEST-ONLY (GATE-A) — one system's dual domain-init emission (machine walk vs hand oracle), for
/// `tests/domain_init_walk.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct DomainInitParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `DomainInitWalk` machine path ([`super::domain_init_walk::walk`], = the production
    /// path) emits for ALL of this system's constructor domain seeds.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`domain_init_hand`]) emits for the same.
    pub hand_text: String,
    /// How many domain fields this system declares — so the test can prove the corpus exercised
    /// multi-field systems (a system with zero domain fields is a vacuous pass).
    pub field_count: usize,
}

/// TEST-ONLY (GATE-A). Emit **every** system's constructor domain seeds through BOTH the
/// `DomainInitWalk` machine ([`super::domain_init_walk::walk`]) and the preserved hand oracle
/// ([`domain_init_hand`]) — over the SAME real parsed systems and the SAME backend.
/// `tests/domain_init_walk.rs` asserts, for every entry, `machine_text == hand_text` byte-for-byte.
#[doc(hidden)]
pub fn domain_init_parity_report(
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<DomainInitParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        report.push(DomainInitParity {
            label: sym.name.clone(),
            machine_text: super::domain_init_walk::walk(sym, be),
            hand_text: domain_init_hand(sym, be),
            field_count: sym.domain.len(),
        });
    }
    report
}

/// The preserved byte-for-byte **oracle** for the STATE-CHAIN table walk — the hand `for st in
/// &sym.states { … climb … }` loop that produced the generated runtime's root..leaf path table
/// before it was reified as the [`super::hsm_chain_walk`] `@@system` (`HsmChainWalk`). Kept as the
/// differential check that machine is proven against (GATE-A, `tests/emit_scaffold_walks.rs`, via
/// [`hsm_chain_parity_report`]). It calls the SAME leaves the machine's cycle states call
/// ([`super::hsm_chain_walk::push_state_name`], [`super::hsm_chain_walk::parent_index`],
/// [`super::hsm_chain_walk::stamp_chain`]) — the two paths differ only in how the outer cursor and
/// the inner climb are SEQUENCED, which is exactly what the gate isolates. Doc-hidden and **not on
/// the production path**.
#[doc(hidden)]
fn hsm_chain_hand(sym: &SystemSym, be: &dyn Backend) -> String {
    use super::hsm_chain_walk::{clear_chain, parent_index, push_state_name, stamp_chain};
    let n = sym.states.len();
    let mut chain: Vec<String> = Vec::new();
    let mut out = Sink::new();
    for si in 0..n {
        clear_chain(&mut chain);
        let mut ci = si;
        let mut depth = 0usize;
        loop {
            if depth > n {
                break;
            }
            push_state_name(sym, ci, &mut chain);
            depth += 1;
            let p = parent_index(sym, ci);
            if p < 0 {
                break;
            }
            ci = p as usize;
        }
        stamp_chain(sym, be, si, &mut chain, &mut out);
    }
    out.finish()
}

/// TEST-ONLY (GATE-A) — one system's dual state-chain table (machine walk vs hand oracle), for
/// `tests/emit_scaffold_walks.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct HsmChainParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `HsmChainWalk` machine path ([`super::hsm_chain_walk::walk`], = the production path)
    /// emits for the whole table.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`hsm_chain_hand`]) emits for the same.
    pub hand_text: String,
    /// How many states this system declares.
    pub state_count: usize,
    /// The DEEPEST ancestor chain the walk produced — so the test can prove the corpus exercised a
    /// genuine climb (`> 1`) and not only flat one-element paths.
    pub max_depth: usize,
}

/// TEST-ONLY (GATE-A). Build **every** system's state-chain table through BOTH the `HsmChainWalk`
/// machine ([`super::hsm_chain_walk::walk`]) and the preserved hand oracle ([`hsm_chain_hand`]) —
/// over the SAME real parsed systems and the SAME backend. `tests/emit_scaffold_walks.rs` asserts,
/// for every entry, `machine_text == hand_text` byte-for-byte.
#[doc(hidden)]
pub fn hsm_chain_parity_report(
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<HsmChainParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        // The corpus-coverage tally: the longest root..leaf path, computed from the same frozen
        // `parent` links both paths read.
        let max_depth = (0..sym.states.len())
            .map(|si| {
                let mut ci = si;
                let mut d = 0usize;
                while d <= sym.states.len() {
                    d += 1;
                    let p = super::hsm_chain_walk::parent_index(sym, ci);
                    if p < 0 {
                        break;
                    }
                    ci = p as usize;
                }
                d
            })
            .max()
            .unwrap_or(0);
        report.push(HsmChainParity {
            label: sym.name.clone(),
            machine_text: super::hsm_chain_walk::walk(sym, be),
            hand_text: hsm_chain_hand(sym, be),
            state_count: sym.states.len(),
            max_depth,
        });
    }
    report
}

/// The preserved byte-for-byte **oracle** for the STATE-ROUTER walk — the hand `for st in
/// &sym.states { … }` loop (with its `first` flag) that produced the generated runtime's dispatch
/// chain before it was reified as the [`super::router_walk`] `@@system` (`RouterWalk`). Kept as the
/// differential check that machine is proven against (GATE-A, `tests/emit_scaffold_walks.rs`, via
/// [`router_parity_report`]). It calls the SAME [`super::router_walk::stamp_router_arm`] leaf the
/// machine's `$Arm` cycle calls — the two paths differ only in how the arm loop is SEQUENCED, which
/// is exactly what the gate isolates. Doc-hidden and **not on the production path**.
#[doc(hidden)]
fn router_hand(sym: &SystemSym, be: &dyn Backend) -> String {
    let mut out = Sink::new();
    let mut first = true;
    for si in 0..sym.states.len() {
        super::router_walk::stamp_router_arm(sym, be, si, first, &mut out);
        first = false;
    }
    out.finish()
}

/// TEST-ONLY (GATE-A) — one system's dual router chain (machine walk vs hand oracle), for
/// `tests/emit_scaffold_walks.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct RouterParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `RouterWalk` machine path ([`super::router_walk::walk`], = the production path)
    /// emits for the whole arm chain.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`router_hand`]) emits for the same.
    pub hand_text: String,
    /// How many states (= arms) this system routes — so the test can prove the corpus exercised
    /// MULTIPLE arms, which is the only way the `first` latch is observable at all.
    pub state_count: usize,
}

/// TEST-ONLY (GATE-A). Build **every** system's router chain through BOTH the `RouterWalk` machine
/// ([`super::router_walk::walk`]) and the preserved hand oracle ([`router_hand`]) — over the SAME
/// real parsed systems and the SAME backend. `tests/emit_scaffold_walks.rs` asserts, for every
/// entry, `machine_text == hand_text` byte-for-byte.
#[doc(hidden)]
pub fn router_parity_report(
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<RouterParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        report.push(RouterParity {
            label: sym.name.clone(),
            machine_text: super::router_walk::walk(sym, be),
            hand_text: router_hand(sym, be),
            state_count: sym.states.len(),
        });
    }
    report
}

/// The preserved byte-for-byte **oracle** for the PER-STATE DISPATCH walk — the hand
/// `for st in &sym.states { for h in &st.handlers { … } }` loops that produced the generated
/// runtime's message dispatchers before they were reified as the [`super::state_dispatch_walk`]
/// `@@system` (`StateDispatchWalk`). Kept as the differential check that machine is proven against
/// (GATE-A, `tests/emit_scaffold_walks.rs`, via [`state_dispatch_parity_report`]). It calls the SAME
/// leaves the machine's cycle states call — the two paths differ only in how the two-level walk is
/// SEQUENCED, which is exactly what the gate isolates. Doc-hidden and **not on the production
/// path**.
#[doc(hidden)]
fn state_dispatch_hand(sym: &SystemSym, be: &dyn Backend, out: &mut Sink) {
    use super::state_dispatch_walk::{clear_arms, dispatch_state, handler_count, stamp_handler};
    let mut arms: Vec<String> = Vec::new();
    for si in 0..sym.states.len() {
        let nh = handler_count(sym, si);
        clear_arms(&mut arms);
        for hi in 0..nh {
            stamp_handler(sym, be, si, hi, &mut arms);
        }
        dispatch_state(sym, be, si, &arms, out);
    }
}

/// TEST-ONLY (GATE-A) — one system's dual state-dispatch emission (machine walk vs hand oracle),
/// for `tests/emit_scaffold_walks.rs`. Doc-hidden.
#[doc(hidden)]
#[derive(Debug)]
pub struct StateDispatchParity {
    /// The system name, for a failing assertion message.
    pub label: String,
    /// Text the `StateDispatchWalk` machine path ([`super::state_dispatch_walk::walk`], = the
    /// production `$Dispatch` phase) emits for ALL of this system's per-state dispatchers.
    pub machine_text: String,
    /// Text the preserved hand oracle ([`state_dispatch_hand`]) emits for the same.
    pub hand_text: String,
    /// How many `(state, handler)` arms this system stamps in total — so the test can prove the
    /// corpus exercised multi-handler states (a corpus of empty states is a vacuous pass).
    pub arm_count: usize,
    /// How many states declare NO handler — so the test can prove the empty-dispatcher arm (a bare
    /// `pass` on python) was actually taken.
    pub empty_states: usize,
}

/// TEST-ONLY (GATE-A). Emit **every** system's per-state dispatchers through BOTH the
/// `StateDispatchWalk` machine ([`super::state_dispatch_walk::walk`]) and the preserved hand oracle
/// ([`state_dispatch_hand`]) — over the SAME real parsed systems and the SAME backend.
/// `tests/emit_scaffold_walks.rs` asserts, for every entry, `machine_text == hand_text`
/// byte-for-byte. The library owns the `.finish()`.
#[doc(hidden)]
pub fn state_dispatch_parity_report(
    ast: &FileAst,
    syms: &SymbolTable,
    be: &dyn Backend,
) -> Vec<StateDispatchParity> {
    let mut report = Vec::new();
    for item in &ast.items {
        let Item::System(sys) = item else { continue };
        let Some(sym) = syms.systems.iter().find(|s| s.name == sys.name) else {
            continue;
        };
        let mut mo = super::Sink::default();
        super::state_dispatch_walk::walk(sym, be, &mut mo);
        let mut ho = super::Sink::default();
        state_dispatch_hand(sym, be, &mut ho);
        report.push(StateDispatchParity {
            label: sym.name.clone(),
            machine_text: mo.finish(),
            hand_text: ho.finish(),
            arm_count: sym.states.iter().map(|s| s.handlers.len()).sum(),
            empty_states: sym.states.iter().filter(|s| s.handlers.is_empty()).count(),
        });
    }
    report
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
                                emit_body(src, syms, sym, BodyRole::Handler, &st.name, &h.event, is_async, &h.body, be, &mut mo);
                            let mut ho = super::Sink::default();
                            let he = emit_body_hand(
                                src,
                                syms,
                                sym,
                                BodyRole::Handler,
                                &st.name,
                                &h.event,
                                is_async,
                                &h.body,
                                be,
                                &mut ho,
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
                        let me = emit_body(src, syms, sym, BodyRole::Action, "", "", false, &b.body, be, &mut mo);
                        let mut ho = super::Sink::default();
                        let he = emit_body_hand(src, syms, sym, BodyRole::Action, "", "", false, &b.body, be, &mut ho);
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

/// Did this body have **nothing to emit**? True when every statement is `Stmt::Trivia` (a comment
/// or blank run between statements, which lowers to no target text) — or when there are none.
///
/// An AST predicate, computed BEFORE emission and never by inspecting what was emitted. It exists
/// because an indent-delimited target cannot leave a method body empty: `def f(self):` with nothing
/// under it is a SyntaxError, not a no-op. The slot still needs a statement, which is exactly what
/// [`Backend::noop`] spells (`pass` on python, nothing on a brace target — so no brace target's
/// bytes move).
///
/// Shared by the `EmitHandlers` machine's `emit_handler` leaf and by the preserved
/// [`emit_handlers_hand`] oracle, so the two paths cannot drift on it (GATE-A would catch it if
/// they did).
pub(super) fn body_is_empty(body: &Body) -> bool {
    body.stmts.iter().all(|s| match s {
        Stmt::Trivia(_) => true,
        // A native statement made only of COMMENTS contributes no executable code. framec is not
        // reading the user's text to decide that — the SCANNER already distinguished a comment
        // from a string (it must, or a `;` gets spliced into one), and the tree now carries the
        // distinction as `LiteralNode::is_comment`. This is the difference between asking the node
        // a question framec answered and re-deriving it from bytes framec does not understand.
        Stmt::Native(n) => n.parts.iter().all(
            |p| matches!(p, crate::tree::body::NativePart::Literal(l) if l.is_comment),
        ),
        _ => false,
    })
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
