# Language pack — Rust

Per-target **spellings** for the faithfulness grid. Pairs with a milestone core (`M<k>.md`) which
holds the language-neutral behavior. Oracle: `the local build (framec 4.6.0.x) -l rust`. Every spelling below is quoted
from emitted bytes (`/tmp/rust_scout/out_rust/*.rs`); **the builder must re-emit and `cmp` — this
pack is a guide, not a substitute for a freshly regenerated oracle.**

## Type-ignorance (the governing principle — resolves the "type mapping" question)

Frame is **type-ignorant**: legacy emits the user's type strings **verbatim** and does NO mapping.
Frame `int` emits as literal `int` (invalid Rust); the canonical framec Rust fixtures therefore
author **Rust-native types in the source** (`i32`, `i64`, `f64`, `bool`, `String`). So faithful ng
Rust = **pass types through unchanged**, and the **Rust DoD fixtures are authored with Rust types**.
ng must never invent an `int→i64` mapping — that would diverge from the pass-through oracle. The ONE
place types are hard-coded is the runtime `FrameValue` enum (below).

## M1 Foundation spellings (per layer)

**L0 Module wrapper.** Everything is inside `mod _<snake_system>_framec { use super::*; extern crate
alloc; use alloc::{vec, format}; ... }` with a trailing `pub use _<snake_system>_framec::*;`. A
fixed block of `#[allow(...)]` attributes precedes it (dead_code, non_camel_case_types,
non_snake_case, unused_variables/mut/imports, and 8 `clippy::*`). `no_std`: `alloc::rc::Rc`,
`alloc::collections::BTreeMap`, never `std`.

**L1 Event = typed enum.** `enum <Sys>FrameEvent { <Method> { <param>: <T>, ... }, FrameEnter {},
FrameExit {} }` (`#[derive(Clone)]`). One struct-variant per interface method, PascalCased; params
are named typed fields; paramless → `<Method> {  }`. A companion `impl` maps each variant back to its
message string (`.name()`: `Set { .. } => "set"`, `FrameEnter { .. } => "$>"`, `FrameExit { .. } =>
"<$"`).

**L2 Return slot = typed union.** `enum <Sys>FrameReturn { <Method>(<RetT>), ..., _Lifecycle(alloc::
rc::Rc<dyn core::any::Any>) }` — one tuple variant per **value-returning** interface method, typed to
its return type, plus the type-erased `_Lifecycle`. Void systems emit only `_Lifecycle`. **Return
types must implement `Default`** (used as the fallback). Write: `let __return_val =
<Sys>FrameReturn::<M>(<expr>); if let Some(ctx) = self._context_stack.last_mut() { ctx._return =
Some(__return_val); }`. String returns wrap: `<Sys>FrameReturn::<M>(String::from("x"))`.

**L3 Context.** `struct <Sys>FrameContext { event: Rc<<Sys>FrameEvent>, _return:
Option<<Sys>FrameReturn>, _data: BTreeMap<String, <Sys>FrameValue>, _transitioned: bool }` + `fn
new(event, default_return)`. **`FrameValue`** is the fixed hard-typed value enum: `enum
<Sys>FrameValue { Int(i64), Float(f64), Bool(bool), Str(String), List(Vec<Self>),
Dict(BTreeMap<String, Self>) }`.

**L4 Compartment + StateContext.** State is a **`String`** (`state: String`, router matches
`.as_str()`). State args + enter args are unified into a per-state typed context: `enum
<Sys>StateContext { <State>, <State>(<State>Context), __NoContext }` (with `impl Default` → the start
state) where each data-bearing state gets `struct <State>Context { <field>: <T>, ... }` + a `Default`
impl. `struct <Sys>Compartment { state: String, state_context: <Sys>StateContext, forward_event:
Option<<Sys>FrameEvent>, parent_compartment: Option<Box<<Sys>Compartment>> }` + `fn new(state: &str)`
building `state_context` via a `match state { "B" => ...::B(BContext::default()), ... }`. `derive(Clone)`
— no runtime `copy()`. **No separate state_args/state_vars/enter_args/exit_args lists.**

**L5 System struct + two-phase ctor.** `pub struct <Sys> { _state_stack: Vec<<Sys>Compartment>,
__compartment: <Sys>Compartment, __next_compartment: Option<<Sys>Compartment>, _context_stack:
Vec<<Sys>FrameContext>, pub <domain>: <T>, ... }`. `pub fn new() -> Self` (struct literal, domain
defaults inline, `__compartment: <Sys>Compartment::new("A")`, no `$>`). `pub fn __create() -> Self`
(new → `__prepareEnter` → push `FrameEnter` ctx → `__kernel` → pop). Note literal `__` identifiers.

**L6 Kernel.** `fn __hsm_chain(&mut self, leaf: &str) -> &'static [&'static str]` (a `match` returning
leaf→root slices, `"A" => &["A"]`). `fn __prepareEnter(&mut self, leaf: &str) -> <Sys>Compartment`
(builds the boxed chain; takes **only** `leaf` — args are written at the transition site into the
typed ctx). `fn __kernel(&mut self, __e: &<Sys>FrameEvent)` (route, then `while
self.__next_compartment.is_some()` drain: synthesize `FrameExit`, swap, then `match
self.__compartment.forward_event.take() { None => fresh FrameEnter; Some(fwd) if matches!(fwd, ...
FrameEnter {..}) => dispatch fwd; _ => fresh enter then fwd }`, mark contexts `_transitioned = true`).
`fn __router(&mut self, __e)` (`match self.__compartment.state.as_str() { "A" => self._state_A(__e),
_ => {} }` — does NOT pass the compartment). `fn __transition(&mut self, next) { self.__next_compartment
= Some(next); }`. **No `__prepareExit`.**

**L7 Interface wrapper.** `pub fn <method>(&mut self, <p>: <T>) -> <RetT>` — build `Rc::new(<Sys>Frame
Event::<Method> { <p>: <p> })`, `_context_stack.push(ctx)`, `self.__kernel(&__e)`; **void** → pop,
return `()`; **value** → `let __ctx = self._context_stack.pop().expect("invariant: ..."); match
__ctx._return { Some(<Sys>FrameReturn::<M>(v)) => v, Some(<Sys>FrameReturn::_Lifecycle(v)) =>
v.downcast_ref::<<RetT>>().cloned().unwrap_or_default(), _ => Default::default() }`. No `try/finally`.

**L8 Dispatcher + handler.** `fn _state_<S>(&mut self, __e: &<Sys>FrameEvent) { match __e {
<Sys>FrameEvent::<Method> { <field>, .. } => { self._s_<S>_hdl_user_<method>(__e, *<field>...); }, _
=> {} } }` — event fields destructured in the arm, passed **positionally**. Handlers:
`_s_<S>_hdl_user_<method>(&mut self, __e: &<Sys>FrameEvent, <field>: <T>)`; enter →
`_s_<S>_hdl_frame_enter`. Enter-arg reads walk `parent_compartment` to the owning state and `match
&__sc.state_context { <Sys>StateContext::<S>(ctx) => ctx.<field>.clone(), _ => Default::default() }`.

**L9 Statement lowering.** assign `@@:self.x = 1` → `self.x = 1;`. transition `-> $B` → `let mut
__compartment = self.__prepareEnter("B"); self.__transition(__compartment); return;`. transition w/
enter args `-> (99) $B` → same but with `{ if let <Sys>StateContext::B(ref mut ctx) =
__compartment.state_context { ctx.a = 99; } }` before `__transition`. return `@@:(5)` → the L2 typed
write (terminal on Rust: a real `return;` follows). self-call statement `@@:self.g()` → `self.g();`;
in expr `x = @@:self.g()` → `x = self.g();` + a reentrancy guard `if
self._context_stack.last().map_or(false, |ctx| ctx._transitioned) { return; }`.

## `return_call_terminates` = FALSE for Rust  (CORRECTED — the pilot's re-emit-and-cmp caught this)
The private handler `_s_<S>_hdl_user_<m>` is **void** and simply **parks** the value — the L2 typed
write with **no `return;` following**; the handler runs on. This matches Python and the UNIVERSAL
classification in `driver.rs:286-337` (`return_call_terminates` is universally false). An earlier
scout mis-read a transition's `return;` as a value-return `return;` and this pack wrongly said TRUE.
**Trusting the wrong value silently drops every statement after `@@:(e)`** — the exact M1.md landmine.
The `return;` you DO see in a handler comes from a **transition** lowering (L9), not from `@@:(e)`.

## Intentional divergences seeded from the scout (legacy-Rust bugs — ng emits the correct thing)
Each needs a runtime-validating fixture + a `intentional_divergences.txt` entry + an emit-site comment.
1. **`var x` leaks verbatim** (legacy: `var x = self.g();`, `var y: i32 = 7` even drops the `;`).
   No legacy rust fixture uses `var` — untested/unsupported. ng must emit correct `let`/`let mut`.
   *If* the M1 fixtures avoid `var` entirely, defer this to when a local-decl fixture needs it.
2. **`String`-wrapped literal under an unwrapped annotation** — internally inconsistent in legacy;
   only relevant if a fixture writes `string` rather than `String`. Author fixtures with `String`.

*Type pass-through is NOT a divergence — it is Frame type-ignorance; author Rust-native types.*

## Current ng Rust output (the gap to close)
`framec-ng --emit -l rust` today emits an unrelated thin model: `use std::collections::HashMap;`,
`enum <Sys>Vars {..}` + `enum <Sys>Args {..}` + `struct <Sys>Comp`, and `<Sys> { compartment, stack,
pub <domain> }`. **No FrameEvent enum, no FrameReturn enum, no FrameContext, no kernel/router split,
no module wrapper.** M1 replaces this stub with the legacy structure above.
