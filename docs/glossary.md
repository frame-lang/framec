---
title: Glossary
nav_order: 7
---

# Frame Glossary

Plain-English definitions of the terms used across the Frame documentation —
the [language reference](frame_language.md), the [runtime walkthrough](frame_runtime.md),
the [cookbook](frame_cookbook.md), the per-language guides, and the
[RFCs](rfcs/). When one of those documents uses a term defined here, it links
to the term's entry on first use rather than redefining it.

Each entry gives a one- or two-sentence definition and a cross-reference to the
section that specifies it normatively. The language reference is the source of
truth for syntax and semantics; the runtime walkthrough is the source of truth
for runtime behavior; an RFC is the source of truth for a feature the language
reference has not yet absorbed.

Word-named terms are listed alphabetically. Symbol-named terms (those starting
with `@`, `$`, or other punctuation) are collected in [Symbols](#symbols) at the
end.

---

### action

A private helper procedure declared in a system's `actions:` block. Actions can
read domain variables and the system / self / return accessors but cannot fire
[transitions](#transition) or push / pop the [state stack](#state-stack). See
[language reference § Actions Section](frame_language.md#actions-section).

### async system

A [system](#system) declared with the `@@[async]` system-header attribute.
framec emits an async system as a layered pair — a public [casing](#casing)
exposing the declared [interface](#interface) and [operations](#operation),
holding a private inner [machine](#machine) that owns the
[dispatch](#dispatch) path (the watchmaker's metaphor: a case houses the
movement). The casing is the natural home for the
[single-driver gate](#single-driver-gate) (`E703`) and for future
cross-cutting concerns (instrumentation, optional serialization). Interface
methods are generated in the host language's asynchronous form
(`async`/`await`, `CompletableFuture`, `Future`, …). On Erlang the
`gen_statem` actor model already provides single-driver semantics; the
layered shape is not emitted there. Async members in a system without
`@@[async]` are an `E710` validator error (with a one-release `W710`
grace period). See [RFC-0043](rfcs/rfc-0043.md) and
[language reference § Async](frame_language.md#async).

### backbone

The top-level coordinating [system](#system) of a parser written in Frame: its
[states](#state) correspond to the grammar's major nonterminals (a section, a
state, a handler), and it drives the parse forward, handing focused sub-problems
to [oracles](#oracle). Named for being the structural spine the specialists
attach to. framec's own parser is organized this way — one `SystemBackbone`
system whose flat self-looping states cover the entire token-level outer grammar.
See [RFC-0039](rfcs/rfc-0039.md#terminology).

### casing

The public outer artifact framec emits for an [async system](#async-system).
Bears the user-declared name (e.g. `Counter`); is what users instantiate,
what [domain](#domain) fields hold, and what other systems' handlers call.
Exposes the declared [interface](#interface) and [operations](#operation) as
the externally visible API; holds a private inner [machine](#machine) and
delegates to it through the [single-driver gate](#single-driver-gate).
Named for the watchmaker's metaphor: a watch case houses the movement,
presents the readable face, and routes inputs (crown turns, button
presses) to the movement. The casing is also where future cross-cutting
concerns (trace hooks, instrumentation, optional serialization) will be
mounted. Only emitted for systems carrying `@@[async]`; sync systems are
single artifacts. See [RFC-0043](rfcs/rfc-0043.md).

### compartment

The runtime object holding everything specific to one *occupancy* of a
[state](#state): the state's identity, its [state variables](#state-variable),
and the [state-args](#state-args) / [enter-args](#enter-args) /
[exit-args](#exit-args) passed when it was entered. A new compartment is created
on every [transition](#transition); the [state stack](#state-stack) is a stack
of compartments. See [language reference § Compartment](frame_language.md#compartment).

### composed system

*(Also: nested system.)* A [system](#system) that holds another `@@system`
instance as a [domain](#domain) field, e.g. `domain: inner = @@Inner(...)`. The
parent's [factory](#factory) constructs the child by calling the child's
factory; the parent's [restore](#restore) reconstructs the child by allocating
it with [no-initialization](#no-initialization) and recursively loading. See
[RFC-0015](rfcs/rfc-0015.md).

### construction

Creating a fully-initialized [system](#system) instance: allocate the object,
then run the [start state](#start-state)'s `$Start(...)` body and its
[`$>` enter handler](#-enter-handler). The user-facing entry point for
construction is the [factory](#factory). Contrast [no-initialization](#no-initialization).
See [RFC-0015](rfcs/rfc-0015.md).

### dispatch

The runtime act of routing an incoming [frame event](#frame-event) to the
[event handler](#event-handler) for the current [state](#state) — walking up the
[HSM](#hierarchical-state-machine-hsm) parent chain if the current state
[forwards](#forward). Performed by the per-system *kernel* function. See
[runtime walkthrough § Step 1](frame_runtime.md#step-1--a-system-that-accepts-a-call).

### domain

A [system](#system)'s persistent, instance-level data, declared in the `domain:`
block. Each entry is a **domain variable** (also **domain field**) with an
initializer expression. Domain variables live for the lifetime of the system
instance — unlike [state variables](#state-variable), they are not reset by
[transitions](#transition). See
[language reference § Domain Section](frame_language.md#domain-section).

### domain-param

A [system parameter](#system-parameter) with no sigil, supplying the initial
value of a [domain](#domain) variable of the same name. See
[language reference § Domain params](frame_language.md#domain-params).

### enter-args

The arguments passed to a [state](#state)'s [`$>` enter handler](#-enter-handler)
when that state is entered, carried on the new [compartment](#compartment).
Declared on the state via `$>(name: type)`; supplied at the [transition](#transition)
or instantiation site with the `$>(...)` sigil. See
[language reference § Enter Handler](frame_language.md#enter-handler).

### enter-param

A [system parameter](#system-parameter) tagged with `$>` that flows into the
[start state](#start-state)'s [enter-args](#enter-args). See
[language reference § Enter params](frame_language.md#enter-params).

### event handler

*(Also: handler.)* The block a [state](#state) runs in response to a named event
— an [interface](#interface) method call, an `$>` enter, an `<$` exit, or a
[forwarded](#forward) event. See
[language reference § Event Handlers](frame_language.md#event-handlers).

### exit-args

The arguments passed to a [state](#state)'s [`<$` exit handler](#-exit-handler)
when that state is left. See
[language reference § Exit Handler](frame_language.md#exit-handler).

### factory

The auto-generated [construction](#construction) entry point for a
[system](#system): it allocates the instance, routes the
[system parameters](#system-parameter) into the right channels (domain
initializers / [state-args](#state-args) / [enter-args](#enter-args)), runs the
[start state](#start-state)'s `$Start(...)` body and `$>` handler, and returns
the initialized instance. Named with `@@[create(<name>)]`; its per-backend call
shape is specified by [RFC-0017](rfcs/rfc-0017.md). See [RFC-0015](rfcs/rfc-0015.md).

### forward

*(Also: event forwarding.)* A [handler](#event-handler) that re-dispatches the
current event to its parent [state](#state), written `=> $^`. The basis of
[hierarchical state machines](#hierarchical-state-machine-hsm). See
[language reference § Forward to Parent](frame_language.md#forward-to-parent--).

### frame context

*(`FrameContext`.)* The per-call runtime object carrying the in-flight event,
the return-value slot, the `@@:params` view, and the dispatch-scoped `@@:data`
store. A stack of frame contexts (the *context stack*) exists so a
[self-call](#selfmethodargs-self-call) nests cleanly. See
[language reference § System Context](frame_language.md#system-context).

### frame event

*(`FrameEvent`.)* The runtime object representing one [dispatched](#dispatch)
event: its name and its positional parameters. See
[runtime walkthrough § Step 1](frame_runtime.md#step-1--a-system-that-accepts-a-call).

### hierarchical state machine (HSM)

A [state machine](#machine) in which a [state](#state) may declare a parent
state and [forward](#forward) (`=> $^`) unhandled events up the chain. Frame
supports a parent chain up to three levels deep. See
[language reference § Hierarchical State Machines](frame_language.md#hierarchical-state-machines).

### interface

A [system](#system)'s public API, declared in the `interface:` block: the named
methods callers may invoke. Each interface call is [dispatched](#dispatch) to
the current [state](#state). See
[language reference § Interface Section](frame_language.md#interface-section).

### load

The deserialization half of the [persist contract](#persist-contract): an
instance method that takes a serialized blob and overwrites the instance's
[domain](#domain) and [compartment](#compartment) state **without running any
[construction](#construction) code**. Named with `@@[load(<name>)]`. The clean
[restore](#restore) pattern allocates a target with
[no-initialization](#no-initialization) (`@@!Foo()`) and then calls load. See
[RFC-0015](rfcs/rfc-0015.md) and
[language reference § Persistence](frame_language.md#persistence).

### machine

A [system](#system)'s state machine, declared in the `machine:` block: its
[states](#state), their [handlers](#event-handler), and the
[transitions](#transition) between them. See
[language reference § Machine Section](frame_language.md#machine-section).

In an [async system](#async-system) the term also refers to the *generated*
inner artifact that owns the [dispatch](#dispatch) path —
[kernel](#dispatch), [compartment](#compartment), [context stack](#frame-context),
transition loop, lifecycle cascades — held privately by the
[casing](#casing) and unreachable from outside it. See
[RFC-0043](rfcs/rfc-0043.md).

### no-initialization

Allocating a [system](#system) instance *without* running any
[construction](#construction) code — no `$Start(...)` body, no `$>` handler.
The Frame source expression is the `@@!Foo()` sigil (always zero-argument). Used
with [`@@[load]`](#load) to [restore](#restore) a persisted instance: allocate
with no-initialization, then deserialize. What `@@!Foo()` becomes in each target
language is specified by [RFC-0017](rfcs/rfc-0017.md); the design is in
[RFC-0015](rfcs/rfc-0015.md).
*(Earlier drafts called this "blank allocation"; the current name is
"no-initialization".)*

### Oceans Model

Frame's principle that anything in a source file outside an `@@system` block —
imports, package declarations, free functions, helper classes, comments — is
emitted into the generated target file verbatim, in position. framec does not
parse or rewrite this pass-through text; it knows only that the text is not
Frame. The name evokes "islands of Frame in an ocean of native host code." See
[framepiler design § Segmenter](framepiler_design.md).

**Calibration.** The Oceans Model is a pragmatic decomposition — the
transpiler touches `@@system` blocks and passes through everything else
verbatim. It is architectural humility, not a theoretical breakthrough. Avoid
framing it as a paradigm shift in writing or in conversation. The open
question of whether it can be elevated to a calculus (with a preservation
theorem and pre-backend normalization passes) is captured in
[RFC-0026](rfcs/rfc-0026.md) as an exploration thread, not a commitment.

### operation

A non-[handler](#event-handler) method declared in a [system](#system)'s
`operations:` block. Unlike [interface](#interface) methods, operations are not
[dispatched](#dispatch) to a state — they run directly. Non-static operations
may read [domain](#domain) variables and `@@:return`. See
[language reference § Operations Section](frame_language.md#operations-section).

### oracle

A small, focused machine that a [backbone](#backbone) *consults* to answer one
bounded question or parse one sub-structure (e.g. "capture this balanced
expression"); it returns a result and the backbone transitions on it without
needing to know how it was reached. The name is borrowed from the
computer-science *oracle machine* — a sub-machine treated as a black box.
Concretely, an oracle is another Frame system (or scanner) the backbone calls,
as framec's parser does with the dogfooded attribute / expression scanners. See
[RFC-0039](rfcs/rfc-0039.md#terminology).

### parameterized system

A [system](#system) whose header declares [system parameters](#system-parameter)
— [state-args](#state-args), [enter-args](#enter-args), and/or domain args —
that the [factory](#factory) requires at [construction](#construction) time. See
[language reference § System Parameters](frame_language.md#system-parameters).

### persist contract

The set of rules and generated code that makes a [system](#system) serializable:
`@@[persist(<type>)]` names the host-language type of the serialized blob (see
the [`@@[persist(<type>)]`](#persisttype) entry), `@@[save(<name>)]` and
`@@[load(<name>)]` name the two operations, and framec generates both bodies.
[Load](#load) bypasses [construction](#construction). Domain fields tagged
[`@@[no_persist]`](#no_persist) are excluded from the blob. See
[language reference § Persistence](frame_language.md#persistence),
[RFC-0012](rfcs/rfc-0012.md), and [RFC-0015](rfcs/rfc-0015.md).

### push / pop

[State-stack](#state-stack) operations. `push$` (often `push$ -> $State`) saves
the current [compartment](#compartment) on the [state stack](#state-stack) and
transitions to a new one; `-> pop$` discards the current compartment and resumes
the one on top of the stack. The mechanism behind modal sub-flows. See
[language reference § Stack Push](frame_language.md#stack-push--push) and
[§ Stack Pop](frame_language.md#stack-pop--pop).

### restore

Reconstructing a [system](#system) instance from a serialized blob: allocate the
instance with [no-initialization](#no-initialization), then call [`load`](#load).
For a [composed system](#composed-system), the same pattern recurses into nested
`@@system` domain fields. See [RFC-0015](rfcs/rfc-0015.md).

### save

The serialization half of the [persist contract](#persist-contract): an instance
method that returns a blob containing the [system](#system)'s [domain](#domain)
fields, [compartment](#compartment), and [state stack](#state-stack) (and any
nested `@@system` domain fields). Named with `@@[save(<name>)]`. See
[language reference § Persistence](frame_language.md#persistence).

### single-driver contract

The rule that an [async system](#async-system) instance has at most one
external [interface](#interface) call in flight at any time. The caller —
the host code holding the instance — is responsible for serializing its
events. Frame provides no concurrency primitives, no locking, no event
queue. Internal flow (`@@:self`, [forward](#forward), lifecycle cascades,
async [await](#dispatch) suspension) is not subject to this rule because
it never crosses out of the inner [machine](#machine). Sync systems are
subject to the same single-driver discipline they have always been —
the host's own call-return order — and the gate is not emitted for them.
Enforced at runtime by the [single-driver gate](#single-driver-gate). See
[RFC-0043](rfcs/rfc-0043.md).

### single-driver gate

The runtime mechanism that detects violations of the
[single-driver contract](#single-driver-contract). Emitted on the
[casing](#casing) of every [async system](#async-system) as a
per-instance busy flag, set on entry to a public [interface](#interface)
method and cleared on return in a `try`/`finally` equivalent. Internal
dispatch lives in the inner [machine](#machine) and never touches the
flag. A violation raises `E703`. See [RFC-0043](rfcs/rfc-0043.md).

### start state

The first [state](#state) declared in a [system](#system)'s `machine:` block —
the state the system occupies immediately after [construction](#construction).
Its `$Start(name: type)` header receives the [state-args](#state-args); its `$>`
[enter handler](#-enter-handler) receives the [enter-args](#enter-args). See
[language reference § System Parameters](frame_language.md#system-parameters).

### state

A named node in a [system](#system)'s [machine](#machine). The system is always
"in" exactly one state; an [event handler](#event-handler) in that state runs
when a matching event is [dispatched](#dispatch). See
[language reference § State Declaration](frame_language.md#state-declaration).

### state-args

The arguments bound to a [state](#state)'s `$Start(...)`-style parameters when
that state is entered, carried on the new [compartment](#compartment). Declared
on the state; supplied at the [transition](#transition) or instantiation site
with the `$(...)` sigil. See
[language reference § State params](frame_language.md#state-params).

### state-param

A [system parameter](#system-parameter) tagged with `$(...)` that flows into the
[start state](#start-state)'s [state-args](#state-args). See
[language reference § State params](frame_language.md#state-params).

### state stack

The stack of [compartments](#compartment) maintained by `push$` / `pop$`. The
current state is the compartment on top; pushing saves it and starts a new one;
popping discards the current one and resumes the saved one. See
[language reference § State Stack = Compartment Stack](frame_language.md#state-stack--compartment-stack).

### state variable

A variable scoped to a single [state](#state) (declared inside the state), reset
to its initializer each time the state is entered. Distinct from a
[domain](#domain) variable, which persists for the instance's lifetime. Accessed
with `$.name`. See
[language reference § State Variables](frame_language.md#state-variables).

### system

A Frame state machine as a unit: the `@@system` declaration with its
`interface:`, `machine:`, `actions:`, `operations:`, and `domain:` blocks. The
compilation target. See
[language reference § System Declaration](frame_language.md#system-declaration).

### system context

The runtime façade through which a [handler](#event-handler) reads call-scoped
information: `@@:return`, `@@:params`, `@@:event`, `@@:data`, `@@:system.state`,
and `@@:self`. Backed by the [frame context](#frame-context) stack. See
[language reference § System Context](frame_language.md#system-context).

### system parameter

A parameter declared in a [system](#system)'s header. Frame distinguishes three
groups by sigil: `$(...)` → [state-param](#state-param), `$>(...)` →
[enter-param](#enter-param), bare → [domain-param](#domain-param). The
[factory](#factory) routes each group to the appropriate channel. See
[language reference § System Parameters](frame_language.md#system-parameters).

### transition

Moving the [system](#system) from one [state](#state) to another, written
`-> $Target` (optionally with `$(...)` / `$>(...)` argument groups). A
transition runs the old state's `<$` [exit handler](#-exit-handler), creates a
fresh [compartment](#compartment) for the target, and runs the target's `$>`
[enter handler](#-enter-handler). See
[language reference § Transition](frame_language.md#transition---state).

---

## Symbols

### `@@`

The *system context* token. On its own it introduces a context accessor
(`@@:return`, `@@:params.x`, `@@:event`, `@@:data.k`, `@@:system.state`,
`@@:self.method(...)`); as a prefix it tags compiler-recognized constructs
(`@@system`, `@@[...]` attributes, `@@SystemName(...)` instantiation,
`@@!Foo()`). See [language reference § System Context — `@@`](frame_language.md#system-context--).

### `@@system`

Declares a Frame [system](#system). See
[language reference § System Declaration](frame_language.md#system-declaration).

### `@@[...]` (attribute syntax)

An `@@[name(args)]` annotation attached to a declaration (file, system, or
domain field). See [language reference § `@@[target(...)]`](frame_language.md#target)
and [RFC-0013](rfcs/rfc-0013.md).

### `@@[target(...)]`

File-level attribute selecting the code-generation backend. See
[language reference § `@@[target(...)]`](frame_language.md#target).

### `@@[main]`

System-level attribute marking the module's primary [system](#system) (the one a
caller instantiates as the module's entry point). See [RFC-0014](rfcs/rfc-0014.md).

### `@@[persist(<type>)]`

System-level attribute marking a [system](#system) serializable. `<type>` is the
**host-language type of the serialized blob** — the return type of the generated
[save](#save) method and the parameter type of the generated [load](#load)
method. It is not a fixed set: you write your language's string-or-byte-buffer
type (`bytes` in Python, `String` in Rust/Java/Kotlin/Swift/Dart/PHP, `string`
in C#/Go/JS/TS, `char*` in C, `std::string` in C++, `PackedByteArray` or
`String` in GDScript, `binary` in Erlang). framec emits it verbatim into the
`save`/`load` signatures and does not interpret it; the per-backend
serialization library (serde, Jackson, `nlohmann/json`, pickle, cJSON, …) does
the actual encoding. So `<type>` is "what the blob looks like to your code", not
"what format is inside it". Companion attributes: `@@[save(<name>)]` /
`@@[load(<name>)]` name the methods (next entry); [`@@[no_persist]`](#no_persist)
excludes a domain field. See [persist contract](#persist-contract),
[language reference § `@@[persist]`](frame_language.md#persist),
[RFC-0012](rfcs/rfc-0012.md), and [RFC-0016](rfcs/rfc-0016.md) (the proposed
system-level inclusion list).

### `@@[save(<name>)]` / `@@[load(<name>)]`

System-level attributes naming the generated [save](#save) and [load](#load)
operations. See [RFC-0015](rfcs/rfc-0015.md).

### `@@[create(<name>)]`

System-level attribute naming the generated [factory](#factory). See
[RFC-0015](rfcs/rfc-0015.md).

### `@@[no_persist]`

Domain-field attribute excluding the field from the serialized blob; after
[restore](#restore) the field holds its `domain:` default (typically `null` for
a resource handle, which the user reattaches explicitly). Applies only to
`domain:` fields — not [state variables](#state-variable) or the rest of the
[compartment](#compartment) bookkeeping, which is always persisted. Specified in
[RFC-0016.1](rfcs/rfc-0016-1.md); see also [persist contract](#persist-contract).

### `@@import`

Module-level **analysis directive** naming a Frame source file —
`@@import "./other.frm"`. It tells framec where a referenced [system](#system)'s
source lives so framec can read it *while compiling the current file*: to check
this file's use of that system (argument arity/types, existence) and to resolve
cross-file facts code generation needs (notably a [composed](#composed-system)
child's [save](#save) / [load](#load) method names). It is **not** a
code-generation directive — framec emits no import line and no target code for an
imported system; native host imports remain [Oceans Model](#oceans-model)
pass-through. Removed by [RFC-0024](rfcs/rfc-0024.md) (which deleted the original
analysis-*and*-emission directive); re-introduced in analysis-only form by
[RFC-0040](rfcs/rfc-0040.md).

### `@@SystemName(args)`

Instantiation expression: construct a [system](#system) instance by calling its
[factory](#factory). See
[language reference § System Instantiation](frame_language.md#system-instantiation);
the per-backend call shape is in [RFC-0017](rfcs/rfc-0017.md).

### `@@!Foo()`

[No-initialization](#no-initialization) allocation expression: produce an
instance of `Foo` without running any [construction](#construction) code. Always
zero-argument. Used with [`@@[load]`](#load) for [restore](#restore). See
[RFC-0015](rfcs/rfc-0015.md) (design) and [RFC-0017](rfcs/rfc-0017.md) (what it
becomes in each target language).

### `$>` (enter handler)

A [state](#state)'s *enter handler*: the block run when the state is entered.
Receives [enter-args](#enter-args). Since RFC-0019, `$>` is an
ordinary leaf-dispatched event — only the *current* state's `$>` runs on
entry. Ancestor `$>` handlers run only if the leaf explicitly forwards via
[`=> $^`](#-) (placement in the handler body controls order). See
[language reference § Enter Handler](frame_language.md#enter-handler) and
[RFC-0019](rfcs/rfc-0019.md).

### `<$` (exit handler)

A [state](#state)'s *exit handler*: the block run when the state is left.
Receives [exit-args](#exit-args). Since RFC-0019, `<$` is an ordinary
leaf-dispatched event — only the *current* state's `<$` runs on exit.
Ancestor `<$` handlers run only if the leaf explicitly forwards via
[`=> $^`](#-) (placement in the handler body controls order). See
[language reference § Exit Handler](frame_language.md#exit-handler) and
[RFC-0019](rfcs/rfc-0019.md).

### `$Start(...)`

A [start state](#start-state)'s parameter header, receiving the
[state-args](#state-args) that flow from `$(...)`-tagged
[system parameters](#system-parameter).

### `$.name`

Accesses a [state variable](#state-variable). See
[language reference § State Variable Access](frame_language.md#state-variable-access--varname).

### `-> $State`

A [transition](#transition) to `$State`. See
[language reference § Transition](frame_language.md#transition---state).

### `=> $^`

[Forward](#forward) the current event to the parent [state](#state). See
[language reference § Forward to Parent](frame_language.md#forward-to-parent--).

### `push$` / `-> pop$`

[State-stack](#state-stack) [push / pop](#push--pop). See
[language reference § Stack Push](frame_language.md#stack-push--push).

### `@@:self.method(args)` (self-call)

Invoke one of this [system](#system)'s own [interface](#interface) methods
through the [dispatcher](#dispatch) (so it re-enters cleanly, with the automatic
transition guard). See
[language reference § Self Interface Call](frame_language.md#self-interface-call--selfmethodargs)
and [RFC-0006](rfcs/rfc-0006.md).

### `@@:return`

The current call's return-value slot: `@@:return = expr` sets it, `@@:return`
reads it. Shorthand `@@:(expr)` sets it and is the common form in
[handlers](#event-handler). See
[language reference § System Context — `@@`](frame_language.md#system-context--).

### `@@:system.state`

The name of the current [state](#state) at runtime. See
[language reference § Current State](frame_language.md#current-state--systemstate).

---

## How to use this glossary

- **Documents** (language reference, runtime walkthrough, cookbook, per-language
  guides, RFCs) use these terms without re-defining them, and link to the term's
  entry on first use.
- **New terms** get an entry here *before or with* the RFC or feature that
  introduces them — never after.
- When a term's canonical name changes, update the entry and leave a
  parenthetical note about the old name (see [no-initialization](#no-initialization)
  for an example), so a reader who encounters the old name in history can find
  the current one.
