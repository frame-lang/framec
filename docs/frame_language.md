---
title: Language Reference
nav_order: 4
---

# Frame Language Reference

*Prompt Engineer: Mark Truluck <mark@frame-lang.org>*

Complete reference for the Frame language. For a tutorial introduction, see [Getting Started](frame_getting_started.md).

## Table of Contents

- [Source File Structure](#source-file-structure)
- [System Declaration](#system-declaration)
- [Interface Section](#interface-section)
- [Machine Section](#machine-section)
- [Actions Section](#actions-section)
- [Operations Section](#operations-section)
- [Domain Section](#domain-section)
- [Frame Statements](#frame-statements)
- [Hierarchical State Machines](#hierarchical-state-machines)
- [System Context](#system-context)
- [Self Reference](#self-reference)
- [System Runtime](#system-runtime)
- [Compartment](#compartment)
- [Persistence](#persistence)
- [Async](#async)
- [System Instantiation](#system-instantiation)
- [Versioning & Stability](#versioning--stability)
- [Token Summary](#token-summary)
- [Error Codes](#error-codes)
- [Complete Example](#complete-example)
- [Appendix: Frame Syntax Taxonomy](#appendix-frame-syntax-taxonomy)

---

## Source File Structure

```
<preamble>          // native code (optional)
@@[target("<lang>")]     // required, exactly once
<annotations>*           // zero or more (@@[persist], etc.)
@@system <Name> (<params>)? {
    <sections>
}
<postamble>         // native code (optional)
```

Everything outside `@@[target(...)]`, annotations, and `@@system` is native code and passes through unchanged.

### Types and Expressions

Frame has **no type system**. Wherever a type or expression appears in Frame syntax — interface params, state variables, domain fields, return types, initializers — Frame treats them as **opaque strings** and passes them through to the generated code verbatim. Write your target language's type names (`int`, `String`, `Vec<i32>`, `std::string`, etc.) and expressions. Frame does not parse, validate, or translate them.

### `@@[target(...)]`

```
@@[target("<language_id>")]
```

Required. Must appear before `@@system`. Specifies the target language.

| ID | Language | | ID | Language |
|----|----------|-|----|----------|
| `python_3` | Python 3 | | `go` | Go |
| `typescript` | TypeScript | | `php` | PHP |
| `javascript` | JavaScript | | `kotlin` | Kotlin |
| `rust` | Rust | | `swift` | Swift |
| `c` | C (C11) | | `ruby` | Ruby |
| `cpp_23` | C++ (≥ C++20 for async) | | `erlang` | Erlang |
| `java` | Java | | `lua` | Lua |
| `csharp` | C# | | `dart` | Dart |
| `graphviz` | GraphViz DOT | | `gdscript` | GDScript |

The `@@[target(...)]` attribute is the authoritative declaration of the file's target language. It can be overridden by a CLI flag (`-l <language>`). The bare `@@target` form is hard-cut (E804) per RFC-0013.

### `@@[persist]`

```frame
@@[persist(<blob_type>)]
@@[save(<save_method_name>)]
@@[load(<load_method_name>)]
```

Marks a system as serializable. A persisted system declares three
system-level attributes: `@@[persist(<blob_type>)]` (the blob type),
`@@[save(<name>)]` (the save method name), and `@@[load(<name>)]`
(the load method name). Framec generates the save/load pair on the
system class — save returns the blob, load is an instance method
that mutates self.

Bare `@@[persist]` (no save/load names) is rejected with **E814**.
The legacy operation-attribute form (`operations: @@[save] foo()`)
is rejected with **E819** at framec 4.1.0+; the codemod at
`scripts/migrate_rfc0015.py` rewrites old fixtures.

Optional companion on domain fields:

| Attribute        | Position     | Purpose                                       |
|------------------|--------------|-----------------------------------------------|
| `@@[no_persist]` | domain field | Excludes this field from the serialized blob |

See [Persistence](#persistence), [RFC-0015](rfcs/rfc-0015.md),
and [RFC-0016](rfcs/rfc-0016.md) (deferred selective-domain-persist
form).

---

## System Declaration

```
@@system <Name> ( <system_params> )? ( : <Base1>, <Base2>, ... )? {
    ( operations: <operations_block> )?
    ( interface: <interface_block> )?
    ( machine: <machine_block> )?
    ( actions: <actions_block> )?
    ( domain: <domain_block> )?
}
```

Sections are optional but **must appear in the order shown**: operations → interface → machine → actions → domain.

### Base Classes

A system can declare base classes or interfaces using `:` after the name (and optional parameters):

```
@@system Pong : RefCounted { ... }
@@system NetworkPlayer : Node, Serializable { ... }
@@[main]
@@system Robot($(x: int)) : Controller { ... }
```

Frame passes base class names through **verbatim** to the target language. It does not validate inheritance rules — the target compiler does. Each backend renders the base list per its language's convention:

| Target | `@@system Foo : A, B` |
|--------|----------------------|
| Python | `class Foo(A, B):` |
| TypeScript | `class Foo extends A implements B` |
| JavaScript | `export class Foo extends A implements B` |
| Java | `class Foo extends A implements B` |
| Kotlin | `class Foo : A(), B` |
| Swift | `class Foo: A, B` |
| C# | `class Foo : A, B` |
| C++ | `class Foo : public A, public B` |
| PHP | `class Foo extends A implements B` |
| Ruby | `class Foo < A` (single inheritance; extra bases ignored with a warning) |
| Dart | `class Foo extends A implements B` |
| GDScript | `extends A` (module scope; only one base) |
| Rust | *(not supported — structs have no inheritance; use traits via native code)* |
| Go | *(not supported — structs have no inheritance; use embedding via native code)* |
| C | *(not supported — no inheritance)* |
| Lua | *(not supported — use metatables via native code)* |
| Erlang | *(not supported — use behaviours via native code)* |

Systems without `:` generate standalone classes with no base (the default). For targets that don't support inheritance (Rust, Go, C, Lua, Erlang), declaring `:` on a system is currently ignored — a warning may be added in a future revision.

### Visibility

System classes are **public by default** — they emit `public class` (Java/C#/Swift), `export class` (TypeScript/JavaScript), or `pub struct` (Rust). Languages where classes are public by default (Python, Kotlin, Dart, PHP, Ruby, Lua) emit a bare class declaration.

To make a system non-public, use the `private` keyword:

```
@@system private Helper { ... }
```

| Target | `@@system Foo` (default) | `@@system private Foo` |
|--------|-------------------------|----------------------|
| Java | `public class Foo` | `class Foo` (package-private) |
| C# | `public class Foo` | `class Foo` (internal) |
| Swift | `public class Foo` | `class Foo` (internal) |
| TypeScript | `export class Foo` | `class Foo` (not exported) |
| JavaScript | `export class Foo` | `class Foo` (not exported) |
| Kotlin | `class Foo` (public) | `private class Foo` |
| Rust | `pub struct Foo` | `struct Foo` (crate-private) |

**Rules:**
- `@@system public Foo` is an **error** — systems are public by default; the keyword is redundant.
- `@@system private Foo` targeting Python, Ruby, Lua, C, GDScript, or Erlang is an **error** — these languages do not support class-level visibility modifiers.

**Other elements** follow fixed visibility and do not accept modifiers:
- **Interface methods** — always public (that is their purpose)
- **Operations** — always public
- **Actions and handlers** — always private (implementation details)

### System Parameters

Three parameter groups configure a system at construction time. Each is optional, but when present they must appear in this order: **state params** (`$()`), then **enter params** (`$>()`), then **domain params** (bare).

```
@@system Name ( $(state_params), $>(enter_params), domain_params )
```

| Group | Sigil | Target |
|-------|-------|--------|
| State arg | `$(name: type)` | Start state's `compartment.state_args` |
| Enter arg | `$>(name: type)` | Start state's `compartment.enter_args` |
| Domain arg | `name: type` (bare) | Constructor argument, used in domain field initializers |

Each param body has the same shape (`name: type` or `name: type = default`) regardless of group; only the sigil differs. framec validates that state and enter args have matching declarations on the start state's `$Start(name: type)` and `$>(name: type)` handlers.

#### Param syntax

Each individual parameter follows the same shape as an interface method parameter:

```
name
name : type
name : type = default
```

- Untyped (`name`): valid in dynamically-typed targets (Python, JavaScript, Ruby, Lua, GDScript, PHP, Erlang). Static-typed targets require an explicit type.
- Typed (`name : type`): the type string is passed through verbatim to the target language's constructor signature. Use the target's native type names (`int`, `str`, `bool`, `float`, etc.).
- Defaulted (`name : type = default`): the default expression is pasted verbatim into the constructor signature. Defaults must be valid in the target language at the parameter-default position. Integer and boolean literals are portable; string and collection defaults may not be.

#### State params

`$(name: type)` declares a parameter that lands in the start state's `compartment.state_args` map under the declared name. The start state must have a matching `$Start(name: type)` declaration so the dispatch function can bind the param to a local at the top of the state body:

```frame
@@system Robot($(x: int), name: str) {
    interface:
        describe(): str

    machine:
        $Start(x: int) {
            describe(): str { @@:(self.name + "@" + str(x)) }
        }

    domain:
        name = name
}

r = @@Robot($(7), "R2D2")       // x = 7 (state arg), name = "R2D2" (domain)
```

Note the call site: state args are tagged with `$(...)` so framec can route them into `compartment.state_args`. See [System Instantiation](#system-instantiation) for the full call site form.

State args are also written by transitions (`-> $Start(42)`). The codegen stores transition-passed args under the same declared param name, so the dispatch reads the param identically whether the state was entered via the system constructor or a transition.

#### Enter params

`$>(name: type)` declares a parameter that lands in the start state's `compartment.enter_args` map under the declared name. The start state must have a matching `$>(name: type)` enter handler that reads the param:

```frame
@@system Worker($>(batch_size: int)) {
    interface:
        run()

    machine:
        $Start {
            $>(batch_size: int) {
                self.size = batch_size
            }
            run() {
                // process self.size items
            }
        }

    domain:
        size = 0
}

w = @@Worker($>(50))            // start state's enter handler sees batch_size = 50
```

The call site tags enter args with `$>(...)`, the same shape as the declaration. Enter args are also written by transitions that use the `-> "args" $State` form. As with state args, the codegen stores both transition-passed and constructor-passed enter args under the declared param name.

#### Domain params

Bare identifiers in the header become **constructor arguments** that are in scope when the domain field initializers run. A domain field's right-hand side can reference any header param by name:

```frame
@@system Counter(initial: int = 0) {
    interface:
        get(): int

    machine:
        $Counting {
            get(): int { @@:(self.value) }
        }

    domain:
        value = initial         // initial is a constructor arg in scope
}

c = @@Counter(10)               // value is 10
```

The codegen prepends the language-appropriate self-reference (`self.`, `this.`, `@`) to the LHS of the domain field assignment, so `value = value` (param and field with the same name) is unambiguous: it transpiles to `self.value = value`.

---

## Interface Section

Declares the system's public API.

```
interface:
    <method_name> ( <params>? ) (: <return_type> (= <default_value>)? )?
```

**Examples:**

```frame
interface:
    start()
    stop()
    process(data: str, priority: int)
    getStatus(): str
    getDecision(): str = "yes"
```

**Rules:**
- Method names must be unique within the interface
- Parameters: `name: type` or untyped `name`
- Default return value is a native expression, used when no handler sets `@@:return`
- A return type with no default implies `None`/`null` as default

**Cross-target return behavior:** in **strongly-typed targets**
(TypeScript, Java, Kotlin, Swift, C#, Dart, C, C++, Go, Rust) the
declared `: type` annotation is required for the wrapper to expose a
return value — without it, the method is `void`. In **dynamic targets**
(Python, JavaScript, Ruby, Lua, PHP, GDScript, Erlang) the wrapper
always exposes the FrameContext's return slot, so `: type` is
documentation only. See [Frame Runtime — Step 8: Return value](frame_runtime.md#step-8--return-value).

---

## Machine Section

Contains state definitions.

### State Declaration

```
$<StateName> ( => $<ParentState> )? {
    <state_var_declarations>*
    <handlers>*
    ( => $^ )?
}
```

- State names must be unique within the system
- The **first state listed** is the start state
- `=> $ParentState` declares an HSM parent (see [HSM](#hierarchical-state-machines))

### State Variables

Must appear at the top of the state block, before any handlers.

```
$.<varName> (: <type>)? = <initializer_expr>
```

| Part | Required | Description |
|------|----------|-------------|
| `$.` | Yes | State variable prefix |
| `<varName>` | Yes | Identifier |
| `: <type>` | No | Type annotation |
| `= <initializer_expr>` | Yes | Native expression; evaluated on every state entry |

**Scope rules:**
- `$.x` always refers to the enclosing state's variable `x`
- No syntax exists to access another state's variables
- No duplicates within a state
- State variable names may shadow domain variables (no ambiguity due to `$.` prefix)

**Init values are emitted verbatim.** Frame has no type system and does not interpret or wrap initializer values — the text you write after `=` is passed through to the generated code unchanged, exactly like a domain-field initializer. So write a value that is valid in the **target language** for the declared type:

| Declared type | Write (examples) |
|---|---|
| Rust `String` | `String::from("")` (a bare `""` is a `&str` and will not compile) |
| Rust `f64` / `f32` | `0.0`, `1.0`, `3.14` |
| C++ `std::string` | `std::string("")` |
| Java / C# / Kotlin 32-bit `float` | `0.0f` / `0.0F` (a bare `0.0` is a `double`) |
| Java / C# / Kotlin `double` | `0.0` |
| Go `float64` | `0.0` (or `0` — untyped constant) |
| Python / JS / Ruby / Lua (dynamic) | `""`, `0`, `0.0`, `False`/`false` per the target's own literal spelling |

There is no single "portable" literal that is valid across every target — a `String` slot needs `String::from("")` on Rust but `""` on Python; a 32-bit `float` needs `0.0f` on Java but `0.0` on Rust. Write what the target compiler expects. (Earlier versions wrapped a small set of "portable" literals per target; that wrapping was removed — it contradicted the verbatim-passthrough contract.)

### Event Handlers

```
<event_name> ( <params>? ) (: <return_type> (= <default_value>)? )? {
    <body>
}
```

When a handler declares a return type with a default value (`= <expr>`), that expression initializes `@@:return` before the handler body executes.

The body is a mix of native code and Frame statements. Native code passes through unchanged.

### Enter Handler

```
$> ( <params>? ) {
    <body>
}
```

Called when the state is entered via a transition. Parameters come from the transition's enter args.

### Exit Handler

```
<$ ( <params>? ) {
    <body>
}
```

Called when the state is exited via a transition. Parameters come from the transition's exit args.

### Enter/Exit Parameter Mapping

Enter and exit args are passed **positionally**:

```frame
$Idle {
    start() {
        -> ("from_idle", 42) $Active
    }
}

$Active {
    $>(source: str, value: int) {
        print(f"Entered from {source} with {value}")
    }
}
```

### Argument-receiver contract

A transition that supplies args must have a receiver that can take
them. framec enforces this at transpile time:

| Site             | Receiver                       | Code  |
|------------------|--------------------------------|-------|
| `(args) -> $T`   | source state's `<$(...)`       | E419  |
| `-> (args) $T`   | target state's `$>(...)`       | E417  |
| `-> $T(args)`    | target state's state params    | E405  |

If the receiver is missing or its arity doesn't fit, transpilation
fails. EventParam-backed receivers (E417, E419) honor trailing
defaults — `<$(a, b = "x")` accepts 1 or 2 supplied args. State
params (E405) currently have no defaults, so the count must match
exactly.

The check applies only when the transition supplies args.
`-> $T` against a state with `<$(reason)` is allowed; `<$` simply
runs with `reason` unbound (a runtime concern, not a structural
error).

---

## Actions Section

Private helper methods on the system class.

```frame
actions:
    validate(data): bool {
        return data is not None
    }
```

**Can access:** domain variables, `@@:return`, `@@:params.x`, `@@:event`, `@@:data.key`, `@@:self.method()`, `@@:system.state.name`

**Cannot access (E401):** `-> $State`, `=> $^`, `push$`, `pop$`, `$.varName`

Actions have no state context. `return` in actions is the native language return.

---

## Operations Section

Public methods that bypass the state machine entirely.

```frame
operations:
    static version(): str {
        return "1.0.0"
    }

    get_debug_info(): str {
        return f"state={self.__compartment.state}"
    }
```

- **Static operations** have no `self`/`this` access
- **Non-static operations** can access domain variables and `@@:return`
- Same Frame statement restrictions as actions (E401)
- `return` is the native language return

---

## Domain Section

Instance variables declared in canonical Frame syntax: `name : type = init`.

```frame
domain:
    count : int = 0
    label : str = "default"
    items : list = [1, 2, 3]
```

- **Type** is an opaque string — write the target language's type name (`int`, `String`, `Vec<i32>`, etc.)
- **Init** is an opaque native expression — Frame passes it through verbatim
- Type is optional for dynamic targets (Python, JS, Ruby, Lua, Erlang, PHP): `count = 0`
- Init is optional for static targets that zero-initialize (C, C++, Go): `count : int`
- Multi-line init uses paren wrapper: `items : list = (\n    [1, 2, 3]\n)`

Domain variables persist across state transitions and are accessed via **`@@:self.field`** (RFC-0046) in handler, action, and operation bodies. framec lowers `@@:self.field` to the target's native receiver (`self.field`, `this.field`, `this->field`, `$this->field`, `s.field`, `Data#data.field`, …), so a single spelling is portable across all targets. A bare native `self.` is passthrough — valid only where the host language defines `self`.

### `const` Modifier

Prefix a domain field with `const` to mark it immutable after construction:

```frame
domain:
    const max_retries : int = 3
    const threshold   : int = threshold     // initialized from system param
    counter           : int = 0             // mutable
```

A `const` field may be assigned exactly once — either via its initializer or via a system param of the same name in the constructor. Assignment in any handler body is rejected (E615).

Per-target emission uses each language's idiomatic immutability keyword:

| Target | Emitted as |
|---|---|
| C++ | `const T name;` (member init list when init refs a system param) |
| Java | `final T name = init;` |
| C# | `readonly T name = init;` |
| Dart | `final T name = init;` |
| Kotlin | `val name: T = init` (promoted to primary constructor on param collision) |
| Swift | `let name: T = init` |
| TypeScript | `readonly name: T = init;` |
| Rust | (fields are immutable by default) |
| Python / JS / PHP / Ruby / Lua / Erlang / GDScript / C / Go | comment-only marker; immutability not enforced at the target level |

---

## Frame Statements

Frame recognizes exactly **7 constructs** within handler bodies. Everything else is native code.

### Transition — `-> $State`

```
( <exit_params> )? -> ( => )? ( <enter_params> )? <label>? $<TargetState> ( <state_params> )?
```

| Form | Meaning |
|------|---------|
| `-> $State` | Simple transition |
| `-> $State(args)` | With state args |
| `-> (args) $State` | With enter args |
| `(args) -> $State` | With exit args |
| `(exit) -> (enter) $State(state)` | Full form |
| `-> "label" $State` | With label (for diagrams) |
| `-> => $State` | With event forwarding |
| `-> pop$` | Transition to popped state |
| `-> (enter_args) pop$` | Pop with fresh enter args |
| `(exit_args) -> pop$` | Pop with exit args |
| `-> => pop$` | Pop with event forwarding |

**Event forwarding** (`-> =>`): The current event is stashed on the target compartment. After the enter handler fires, the forwarded event is dispatched to the target state. Works on both `$State` and `pop$` targets.

**Transition to popped state** (`-> pop$`): Pops a compartment from the state stack. Full lifecycle fires. State variables are **preserved** (not reinitialized).

**Decorated pop transitions**: Pop transitions accept the same decorations as normal transitions. `-> (result) pop$` replaces the popped compartment's enter_args with fresh values (the caller's `$>` handler receives `result` instead of the original snapshot). `(reason) -> pop$` writes exit_args on the current compartment before leaving. `-> => pop$` forwards the current event to the restored state instead of sending `$>`. All decorations can be combined: `(exit) -> (enter) => pop$`. State args on pop$ are not allowed (E607) — the popped compartment carries its own.

Every transition is implicitly followed by a `return` — code after a transition is unreachable.

### Forward to Parent — `=> $^`

```frame
=> $^
```

Forwards the current event to the parent state's dispatch function. The enclosing state must have a parent declared with `=> $ParentState`.

### Stack Push — `push$`

```frame
push$
```

Saves a **reference** to the current compartment (including all state variables) onto the state stack. The compartment itself is NOT copied — the stack entry and `__compartment` point to the same object.

`push$` is almost always followed by a transition (`push$ -> $State`). The transition creates a new compartment for the target state; the old one is preserved on the stack. `-> pop$` later restores the saved reference.

**Bare `push$`** (no transition): the stack entry and current compartment are the same object. Any modifications to state variables after push$ are visible through both. `pop$` restores the same modified object. For snapshot/undo semantics, use `push$ -> $SameState(args)` to create a new compartment on transition.

### Stack Pop — `pop$`

```frame
pop$
```

Pops and discards the top compartment. To transition to the popped state, use `-> pop$`.

### State Variable Access — `$.varName`

```frame
$.counter               // read
$.counter = <expr>      // write
```

`$.varName` works inside string interpolation expressions for languages that support them (Python f-strings, TypeScript template literals, Kotlin `${}`, Ruby `#{}`, Swift `\()`, C# `$"{}"`). The expansion uses the opposite quote from the string delimiter to avoid collisions — e.g., inside `f"text {$.count}"`, the generated code uses single quotes for the dict key: `state_vars['count']`.

### System Context — `@@`

```frame
@@:params.x         // interface parameter (by name)
@@:return = <expr>  // set return value
@@:return           // read return value
@@:event            // interface method name
@@:data.key         // call-scoped data (by key)
```

See [System Context](#system-context) for full semantics.

### Self & System Prefixes

```frame
@@:self.method(args) // call own interface method (reentrant)
@@:system.state.name      // current state name (read-only)
```

`@@:self` and `@@:system` are syntactic prefixes — neither is a first-class value. Bare `@@:self` (E603) and bare `@@:system` (E604) are errors.

See [Self Reference](#self-reference) and [System Runtime](#system-runtime) for full semantics.

**`return` is always native.** It exits the current function — it does NOT set `@@:return`. In event handlers, `return expr` silently loses the value (W415 warning). Use `@@:(expr)` or `@@:return = expr` to set return values.

| Syntax | Effect |
|--------|--------|
| `@@:(expr)` | Set return value only (concise) |
| `@@:return = expr` | Set return value only (explicit long form) |
| **`@@:return(expr)`** | **Set return value AND exit handler (one statement)** |
| `return` | Exit the handler (native — valid everywhere) |
| `return expr` | Native return — in handlers, value is lost (W415) |
| `return @@:(expr)` | Error E408 — cannot combine |

**`@@:return(expr)`** is the recommended form when you want to set the return value and immediately exit. It replaces the common two-statement pattern `@@:(expr)` + `return`. The expression inside the parens is evaluated, stored in the context return slot, and a native `return` is emitted — all in one Frame statement.

---

## Hierarchical State Machines

### Parent Declaration

```frame
$Child => $Parent {
    ...
}
```

### Explicit Forwarding

**V4 uses explicit-only forwarding.** Unhandled events are **ignored**, not forwarded.

**In-handler forward:**

```frame
$Child => $Parent {
    event_a() {
        log("Child processing")
        => $^
    }
}
```

**State-level default forward** (forwards ALL unhandled events):

```frame
$Child => $Parent {
    specific_event() { ... }
    => $^
}
```

**Key semantics:**
- `=> $^` is the **only** way to forward to parent
- `=> $^` can appear **anywhere** in a handler
- Without `=> $^`, unhandled events are **ignored**

### Lifecycle handlers in HSM (`$>` and `<$`)

Since [RFC-0019](rfcs/rfc-0019.md), `$>` (enter) and `<$` (exit) are
**ordinary leaf-dispatched events** — the kernel does **not** walk the parent
chain firing ancestor `$>`/`<$` handlers. Only the *current* state's lifecycle
handler runs on entry / exit. If you want an ancestor's lifecycle to run, the
leaf must explicitly forward via `=> $^` (placement controls order):

```frame
$Child => $Parent {
    $>() {
        => $^                      // run $Parent.$> first (parent-then-child)
        self.log.append("Child:enter")
    }
    <$() {
        self.log.append("Child:exit")
        => $^                      // run $Parent.<$ last (child-then-parent)
    }
}
$Parent {
    $>() { self.log.append("Parent:enter") }
    <$() { self.log.append("Parent:exit") }
}
```

A `$Child` with **no** `$>`/`<$` handler and **no** state-level `=> $^`
silently *overrides* its ancestor's lifecycle — the parent's `$>`/`<$` does
**not** run. Tag the child with a state-level `=> $^` if you want the parent's
lifecycle to fire when the child has nothing to add.

This is intentionally different from UML statecharts; the rationale is in
[RFC-0019 § Motivation](rfcs/rfc-0019.md).

---

## System Context

The `@@` prefix provides access to the current interface call's context.

### Architecture

Every interface call creates:

- **FrameEvent** — `{ _message: string, _parameters: dict }`
- **FrameContext** — `{ event: FrameEvent, _return: any, _data: dict }`

The context is pushed onto `_context_stack` on call and popped on return. Lifecycle events (`$>`, `<$`) use the existing context.

### Accessor Grammar

All `@@` accessors follow a uniform grammar:

- **`:`** (colon) — navigates Frame's namespace hierarchy
- **`.`** (dot) — accesses a field on the resolved object

Colon drills through Frame namespaces. Dot accesses a property on whatever you've arrived at. If the target is a value (not a container), no dot is needed.

### Context Accessors

`@@:` refers to the current execution context. It is transient — it exists for the duration of a dispatch chain and is then discarded. Multiple contexts stack on `_context_stack` during reentrant calls.

| Syntax | Meaning |
|--------|---------|
| `@@:params.x` | Interface parameter `x` |
| `@@:params` | Parameter bag (if needed as object) |
| `@@:return` | Get/set return value |
| `@@:(expr)` | Set return value (concise) |
| `@@:return(expr)` | Set return value and exit handler |
| `@@:event` | Interface method name |
| `@@:data.key` | Call-scoped data entry |

### Reentrancy

Each interface call pushes its own context. Nested calls are isolated — inner `@@:return` does not affect outer `@@:return`.

### Context Not Available

`@@` context accessors are not available in static operations or the initial `$>` during construction.

---

## Self Reference

`@@:self` is a syntactic prefix used to dispatch through the system's own interface. It is **not** a first-class value — bare `@@:self` is a transpile error (E603). The only valid form is `@@:self.method(args)`.

### Self Accessors

| Syntax | Meaning |
|--------|---------|
| `@@:self.method(args)` | Reentrant interface call |
| `@@:self` (bare) | **Error — E603.** Requires `.method(args)`. |

### Self Interface Call — `@@:self.method(args)`

A system can call its own interface methods using `@@:self.<method>(args)`. This dispatches through the full kernel pipeline — FrameEvent construction, context push, router, state dispatch, handler execution, context pop — exactly as an external call would.

#### Why `@@:self.method()` and not native `self.method()`?

In OO target languages a plain native `self.method()` / `this.method()` inside a handler body *reaches* the generated interface method (so the call's transition executes), but it is **not** equivalent — and is unsupported. `@@:self.method(args)` is the only correct form, for three reasons:

1. **Caller-side transition guard (correctness).** `@@:self.method()` emits a guard at the call site: if the callee transitions, the caller's remaining statements are short-circuited (`if _transitioned: return;` / the Erlang `case … of` wrapper). A native self-call gets **no** such guard, so the caller keeps running against a system that has already left the state — a silent bug.
2. **Static validation.** The validator checks `method` exists in the `interface:` block (or is an action) with the right arity (E601/E602). Native calls bypass this.
3. **Cross-backend portability.** In C and Erlang the handler scope has no `self`/`this` keyword; dispatch goes through a different mechanism. `@@:self.` abstracts that difference so the same Frame source transpiles everywhere.

```frame
$Active {
    calibrate() {
        baseline = @@:self.reading()    // reentrant self-call
        self.offset = baseline * -1
    }
    reading(): float {
        @@:(self.raw_sensor_value + self.offset)
    }
}
```

#### Semantics

- **Full dispatch.** The call goes through the kernel. The handler that executes depends on the current state at the time of the call.
- **Context isolation.** A new context is pushed onto `_context_stack`. Inside the called handler, `@@:event` is the called method's name, `@@:params` are the called method's parameters, and `@@:return` is the called method's return slot. The calling handler's context is untouched.
- **Return value.** The return value is available to the caller as a native expression, just like any function call.
- **State sensitivity.** If a transition occurred before the self-call, the call dispatches to a handler in the new state.

#### Restrictions

- Only interface methods can be called via `@@:self.method()`. Actions and operations are called directly using native syntax.
- `@@:self.method()` does not support calling constructors.

#### Self-Call Validation

| Code | Check | Severity |
|------|-------|----------|
| E601 | Method does not exist in `interface:` block | Error |
| E602 | Argument count does not match interface declaration | Error |
| W601 | Return value not captured for method with return type | Warning |

#### Codegen Expansion

The transpiler expands `@@:self.method(args)` into the target language's native self-call on the generated interface method:

| Target | Expansion |
|--------|-----------|
| Python | `self.method(args)` |
| TypeScript | `this.method(args)` |
| Rust | `self.method(args)` |
| C | `SystemName_method(self, args)` |
| C++ | `this->method(args)` |
| Go | `s.Method(args)` |
| Java | `this.method(args)` |

The generated interface method handles FrameEvent construction, context push/pop, kernel dispatch, and return value extraction. The self-call enters the same code path as an external call.

---

## System Runtime

`@@:system` provides read-only access to the system's runtime state from within handlers, actions, and non-static operations.

| Syntax | Meaning |
|--------|---------|
| `@@:system.state.name` | Current state name (read-only string) |

### Current State — `@@:system.state.name`

Returns the current state name as a string, without the `$` prefix. Read-only — assignment is a parse error.

```frame
$Processing {
    status(): str {
        @@:(@@:system.state.name)    // returns "Processing"
    }
}
```

`@@:system.state.name` reads from the compartment's `state` field. It reflects the current state at the time of access — if a transition has been deferred but not yet processed, `@@:system.state.name` still returns the pre-transition state.

> **Bare `@@:system.state` is reserved** ([RFC-0045](rfcs/rfc-0045.md)). The
> name accessor is `@@:system.state.name`; writing bare `@@:system.state` is a
> transpile error (**E608**). The `@@:system.state` path is held for a future
> direct reference to the current *compartment*, of which the name is one field.

**Available in:** event handlers, enter/exit handlers, actions, non-static operations.

**Not available in:** static operations (no system instance).

---

## Compartment

The **compartment** is Frame's central runtime data structure — a closure for states that preserves state identity and all scoped data.

| Field | Purpose |
|-------|---------|
| `state` | Current state identifier |
| `state_args` | Arguments via `$State(args)` |
| `state_vars` | State variables (`$.varName`) |
| `enter_args` | Arguments via `-> (args) $State` |
| `exit_args` | Arguments via `(args) -> $State` |
| `forward_event` | Stashed event for `-> =>` forwarding |

### State Stack = Compartment Stack

`push$` saves the **entire compartment** (including state variables). `-> pop$` restores it.

| Transition | State Variable Behavior |
|------------|------------------------|
| `-> $State` (normal) | **Reset** to initial values |
| `-> pop$` (history) | **Preserved** from saved compartment |

---

## Persistence

`@@[persist]` generates save/restore methods.

| Language | Save | Restore |
|----------|------|---------|
| Python | `save_state()` → `bytes` | `restore_state(data)` [static] |
| TypeScript | `saveState()` → `any` | `restoreState(data)` [static] |
| Rust | `save_state(&mut self)` → `String` | `restore_state(json)` [static] |
| C | `save_state(self)` → `char*` | `restore_state(json)` [static] |

**Persisted:** current state, state stack, state/enter/exit args, state vars, forward event, domain variables.

**Reinitialized on restore:** `_context_stack` (empty), `__next_compartment` (null).

**Restore does NOT invoke the enter handler** — the state is being restored, not entered.

### Field Filtering

By default every domain variable round-trips through save/restore. To exclude
one — a cache, a resource handle — tag it `@@[no_persist]` in the `domain:`
block; after restore it holds its declared default value:

```frame
domain:
    n: int = 0
    @@[no_persist]
    connection : Connection = null   // not in the blob; null after restore
```

`@@[no_persist]` is specified in [RFC-0016.1](rfcs/rfc-0016-1.md). A proposed
system-level *inclusion* list — `@@[persist_fields([...])]` — is tracked in
[RFC-0016](rfcs/rfc-0016.md) (deferred; not yet shipped).

---

## Async

Interface methods, actions, and operations can be declared `async`. A system
that declares **any** async member **must** carry the `@@[async]` attribute on a
line immediately preceding `@@system` (RFC-0043):

```frame
@@[async]
@@system HttpClient {
    interface:
        async connect(url: str)
        async receive(): Message

    machine:
        $Idle {
            connect(url: str) { ... }
        }

    actions:
        async fetch_data() {
            return await http.get("/data")
        }
}
```

`@@[async]` opts the system into the async **layered codegen architecture** (see
below). It takes no arguments. A system that declares no async members **may**
still carry `@@[async]` to opt into the single-driver gate without becoming
async.

If ANY interface method is `async`, the entire dispatch chain becomes async
(with a couple of per-language carve-outs noted below). Async systems use a
two-phase init: `s = @@System()` (sync construct), then `await s.init()` (async
— fires the `$>` enter event). Swift is the exception: `init` is a reserved
keyword, so the async entry point is named `initAsync()`.

### Layered architecture: casing + machine

Framec emits an async system `<Name>` as **two classes**:

- **`<Name>` — the casing.** The user-facing class, with the name you declared
  (`HttpClient`). Each interface method is a *gated wrapper*: it enforces the
  single-driver contract (below), then delegates to the machine and clears the
  gate on the way out — on both the happy and the error path. This is the only
  surface external callers touch; `@@<Other>()` composition and `@@import`
  resolution always reference this name.
- **`_<Name>Machine` — the machine.** A private class holding the actual
  dispatch core — `__kernel`, `__router`, the state methods, the transition
  loop, the lifecycle cascades. It is byte-for-byte the previous-release
  single-class emission, minus the public name. Self-calls and kernel-internal
  dispatch run against the machine directly and **never** touch the gate.

The machine is internal — private to the file / module / namespace per the
target's privacy unit. **User code must not name `_<Name>Machine` directly**;
its surface is unstable between releases.

### The single-driver gate (`E703`)

The casing permits **at most one external dispatch in flight at a time**. If an
interface method is entered while another is already running (re-entrant or
concurrent external entry), the casing raises **`E703`**:

```
E703: system busy: cannot enter '<method>' while '<in-flight method>' is in flight
```

`E703` reports a programming error — a violation of the single-driver contract,
not a recoverable runtime condition. Operations and persist save/load pass
through to the machine **without** the gate (they're explicitly non-dispatching).

Two validator errors guard the attribute itself:

- **`E720`** — a system declares an async member but lacks `@@[async]`. This is
  a **hard cut**: no warning grace period. Add the attribute, or run the
  migration codemod (below).
- **`E721`** — a *sync* system has a domain field whose type names an `@@[async]`
  system declared in the same file. A sync holder can't await the async
  system's wrappers without itself becoming async; add `@@[async]` to the
  holder. (Same-file only — cross-file composition via `@@import` is not yet
  resolved.)

### Per-target support

| Target | Supported | Mechanism | `E703` surface |
|---|---|---|---|
| Python | Yes | `async def` + `await` | `RuntimeError` |
| TypeScript | Yes | `async` + `await`, `Promise<T>` | `Error` |
| JavaScript | Yes | `async` + `await`, `Promise<T>` | `Error` |
| Rust | Yes | `async fn` + `.await`, boxed futures for recursion | `Err(FrameE703Error)` — recoverable via `?` (D5) |
| Dart | Yes | `Future<T> foo() async` + `await` | `StateError` |
| GDScript | Yes | bare `await` on dispatch calls (no keyword) | `push_error(...)` + typed-zero return (D3) |
| Kotlin | Yes | `suspend fun` — suspend→suspend calls are bare, no `await` keyword | `IllegalStateException` |
| Swift | Yes | `func foo() async throws -> T`; async entry is `initAsync()` (not `init()`) | `throws FrameE703Error` (D2) |
| C# | Yes | `async Task<T>` | `InvalidOperationException` |
| Java | Yes | `CompletableFuture<T>` on the casing only — the machine's internal dispatch (`__kernel`, `__router`, `_state_X`) stays synchronous. Bodies run sync and wrap the result via `CompletableFuture.completedFuture(...)`. | `CompletableFuture.failedFuture(RuntimeException)` |
| C++ | Yes (C++23) | `FrameTask<T>` coroutine promise emitted header-guarded at file scope. `suspend_never` initial + `suspend_always` final — bodies run sync until a real `co_await`; callers extract via `.get()`. | `std::runtime_error` |
| C | No | No native async/await. `async` members are a framec error (the test environment marks these with `@@skip`). | — |
| Go | No | No `async`/`await` keyword. Goroutines + channels model concurrency differently. | — |
| PHP | No | No native async. Fibers (PHP 8.1+) exist but framec has no PHP fiber backend. | — |
| Ruby | No | No native async. Fibers/Async gem exist but framec has no Ruby fiber backend. | — |
| Lua | No | No native async. Coroutines exist but framec has no Lua coroutine backend. | — |
| Erlang | No | gen_statem is a one-color functional async model — `async` isn't applicable. | — |

The `E703` surface is **recoverable** on every layered backend: callers can
catch it (`try`/`catch`), `?`-chain it (Rust), or `try?` it (Swift). It is never
a process-aborting `panic!`/`fatalError`/stripped-`assert`.

**Notes:**

- **Kotlin** is the one supported language that does *not* take an `await` keyword on internal dispatch calls — a `suspend fun` calling another `suspend fun` is bare syntax. This is handled by the framec backend.
- **Java** (no native async/await) uses `CompletableFuture<T>` for the casing only; the machine's dispatch chain stays sync so the call graph doesn't explode through `.thenCompose(...)`. Net cost: callers `.get()`.
- **C++** target must be `cpp_23` (the default `cpp`/`cpp_17` aliases also work, but the compiler needs ≥ C++20 for coroutines — see `framepiler_design.md` for the `FrameTask<T>` model).

### Migration

Existing pre-RFC-0043 sources that declare async members without `@@[async]` are
mechanically migrated — the codemod inserts a single `@@[async]` line above each
affected `@@system` header and changes nothing else:

```bash
framec project add-async-attr path/to/source-tree
```

```javascript
import { migrate_async_attr } from "@frame-lang/framec-wasm";
const migrated = migrate_async_attr(originalSource);
```

---

## System Instantiation

Use `@@SystemName()` in native code to instantiate a Frame system. framec expands this to the appropriate native constructor and validates that the system name exists and arguments match.

```frame
calc = @@Calculator()
```

### Passing system parameters

When the system header declares parameters (see [System Parameters](#system-parameters)), the call site supplies them in one of two forms. **Within a single call, all arguments must use the same form** — mixing positional and named is rejected.

#### Sigil-tagged positional form

State and enter args at the call site are tagged with the same sigils used in the declaration. Domain args remain bare. Order at the call site must match declaration order.

```frame
// Pure domain params — no sigils needed
@@system Counter(initial: int = 0) { ... }. 
c = @@Counter(10)

// Mixed: state param + domain
@@system Robot($(x: int), name: str) { ... }
r = @@Robot($(7), "R2D2")

// Pure enter param
@@system Worker($>(batch_size: int)) { ... }
w = @@Worker($>(50))

// All three groups in one header
@@[main]
@@system Service($(slot: int), $>(timeout: int), name: str) { ... }
s = @@Service($(0), $>(1000), "primary")
```

#### Named form

The named form omits ordering requirements and lets you supply args by declared name. Domain args use bare `name=value`; state and enter args wrap the assignment in their sigil.

```frame
@@system Robot($(x: int), name: str) { ... }
r = @@Robot($(x=7), name="R2D2")

@@[main]
@@system Service($(slot: int), $>(timeout: int), name: str) { ... }
s = @@Service($(slot=0), $>(timeout=1000), name="primary")
```

Named-form args may be supplied in any order. Defaults are filled in for any omitted params.

#### Defaults are substituted at the call site

Parameters with default values may be omitted from either form. framec substitutes the declared default expression at the tagged-instantiation expansion site, so the target language never sees it as a constructor-default — it's a literal arg in the generated call.

```frame
@@system Counter(initial: int = 0) { ... }
c1 = @@Counter()         // expands to Counter(0)  — Frame substitutes the default
c2 = @@Counter(42)       // expands to Counter(42)
```

This means default values can use any expression valid in the target language at *call* scope, not just at *parameter-default* scope. It's also why the call site for `@@Counter()` works in target languages that don't natively support default arguments (Java, C, Go, etc.).

#### Instantiation Validation

framec validates this when it expands the instantiation:

- The system name exists in this file.
- Sigils on the call site match the declared groups (`$(...)` for state args, `$>(...)` for enter args, bare for domain).
- All required (no-default) params are supplied.
- Named args reference declared param names (no typos).
- No duplicate named args.
- No mixing positional and named within a single call.
- State and enter args have matching declarations on the start state's `$Start(name: type)` and `$>(name: type)` handlers.

---

## Versioning & Stability

Frame has two version numbers that move on different schedules.

| Number | What it tracks | Example |
|---|---|---|
| **framec semver** | The transpiler release line. Bumps signal CLI/codegen changes. | `4.3.0` |
| **Grammar version** | The Frame language specification itself. Moves much more slowly. | `v0.30` |

The framec version on its own does not tell you the language version, and vice versa. A patch release of framec almost never changes the grammar; a grammar bump only happens when the language surface changes (new syntax, removed syntax, semantics change).

### Source compatibility (what `4.x` means for your `.fpy` files)

`framec` follows semver for **source-level** compatibility of `.fpy` / `.frs` / `.fts` / etc. files:

- **Major** (`4.x` → `5.x`) may require source changes — a grammar version bump usually rides with it. Migration notes ship in `docs/releases/<version>-migration.md`.
- **Minor** (`4.2` → `4.3`) is additive. Existing valid sources continue to transpile.
- **Patch** (`4.2.3` → `4.2.4`) is bug-fix only. No source changes required.

### Generated code stability (will codegen churn on upgrade?)

Frame does **not** offer a formal byte-stability contract across versions, but in practice patch and minor releases are de facto byte-stable for sources that don't use changed features. Each release's CHANGELOG entry calls out the specific cases where output differs from the previous release.

For example, the `4.2.4` entry states:

> Output for files without `@@import` is byte-identical to `4.2.3` except for the two fixes below.

**Practical advice:**

- Treat the CHANGELOG as the authoritative diff between releases. If your repo pins `framec` and commits generated code, a CHANGELOG read is enough to know whether `git diff` after `cargo install framec` is expected.
- Pin `framec` in CI for reproducible builds. Bump intentionally.
- The `--debug-output` and `--emit-debug` JSON sidecars (source maps, frame-maps, visitor-maps) are not currently positioned as a stable public schema — useful for tooling but expect churn between minor versions.

### Where to look

- `CHANGELOG.md` — per-release notes, including codegen-affecting changes.
- `docs/releases/` — long-form release notes and migration guides.

---

## Token Summary

### Module-Level

| Token | Meaning |
|-------|---------|
| `@@[target("<lang>")]` | Declare target language (attribute form; required, exactly once) |
| `@@[persist]` | Enable serialization (attribute form) |
| `@@system` | Declare state machine |

### State Machine

| Token | Meaning |
|-------|---------|
| `$<Name>` | State reference |
| `$>` | Enter handler |
| `<$` | Exit handler |
| `$^` | Parent state reference |
| `$.` | State variable prefix |

### Statements

| Token | Meaning |
|-------|---------|
| `->` | Transition |
| `-> "label"` | Labeled transition |
| `=>` | Forward |
| `-> =>` | Transition with forwarding |
| `-> pop$` | Transition to popped state |
| `push$` | Push to state stack |
| `pop$` | Pop from state stack |
| `return` | Native return (exits handler/action/operation) |

### Context

| Token | Meaning |
|-------|---------|
| `@@:params.x` | Interface parameter `x` |
| `@@:return` | Return value |
| `@@:event` | Event name |
| `@@:data.key` | Call-scoped data |

### Self & System

Both `@@:self` and `@@:system` are syntactic prefixes. Bare forms are errors (E603 / E604).

| Token | Meaning |
|-------|---------|
| `@@:self.method()` | Self interface call (reentrant) |
| `@@:system.state.name` | Current state name (read-only) |

---

## Error Codes

### Parse Errors (E0xx)

| Code | Name | Description |
|------|------|-------------|
| E001 | `parse-error` | Malformed Frame syntax |
| E002 | `unexpected-token` | Unexpected token in Frame construct |
| E003 | `unclosed-block` | Missing closing brace or delimiter |

### Structural Errors (E1xx)

| Code | Name | Description |
|------|------|-------------|
| E105 | `missing-target` | `@@[target(...)]` directive missing or invalid |
| E111 | `duplicate-system-param` | Duplicate parameter in system declaration |
| E113 | `section-order` | System sections out of order |
| E114 | `duplicate-section` | Section declared more than once |
| E116 | `duplicate-state` | State name declared more than once |
| E117 | `duplicate-handler` | Handler declared more than once in same state |

### Semantic Errors (E4xx)

| Code | Name | Description |
|------|------|-------------|
| E400 | `unreachable-code` | Code after terminal statement |
| E401 | `frame-in-action` | Forbidden Frame statement in action or operation |
| E402 | `unknown-state` | Transition targets undefined state |
| E403 | `invalid-forward` | `=> $^` in state without parent |
| E405 | `param-arity-mismatch` | Wrong number of parameters |
| E406 | `multi-system-erlang` | Multiple systems in single file (Erlang target) |
| E407 | `frame-in-closure` | Frame statement inside nested function scope |
| E410 | `duplicate-state-var` | State variable declared more than once |
| E413 | `hsm-cycle` | Circular parent chain |

### Self-Call Errors (E6xx)

| Code | Name | Description |
|------|------|-------------|
| E601 | `unknown-iface-method` | `@@:self.method()` targets method not in `interface:` |
| E602 | `self-call-arity` | Argument count does not match interface declaration |
| E603 | `bare-self-reference` | Bare `@@:self` — must be `@@:self.method(args)` |
| E604 | `bare-system-reference` | Bare `@@:system` — must be `@@:system.state.name` (or other member) |
| E608 | `reserved-system-state` | `@@:system.state` is reserved (RFC-0045) — use `@@:system.state.name` for the state name |

### Domain & Pop Errors (E6xx)

| Code | Name | Description |
|------|------|-------------|
| E605 | `static-field-no-type` | Static target requires explicit type on domain field |
| E607 | `state-args-on-pop` | State arguments on `pop$` — popped compartment carries its own |
| E613 | `field-shadows-param` | Domain field name shadows a system parameter |
| E614 | `duplicate-field` | Duplicate domain field name |
| E615 | `const-field-assign` | Assignment to `const` domain field in handler body |

### Warnings (W4xx, W6xx)

| Code | Name | Description |
|------|------|-------------|
| W414 | `unreachable-state` | State has no incoming transitions |
| W415 | `handler-return-value-lost` | `return expr` in event handler; value not set on context stack |
| W601 | `unused-self-call-return` | Return value not captured for method with return type |

---

## Complete Example

```frame
import logging

@@[target("python_3")]

@@[persist(str)]
@@[save(save_state)]
@@[load(restore_state)]
@@system OrderProcessor (max_retries: int) {

    operations:
        static version(): str {
            return "1.0.0"
        }

    interface:
        submit(order)
        cancel(reason)
        getStatus(): str = "unknown"

    machine:
        $Idle {
            submit(order) {
                logging.info("Received order")
                self.order_data = order
                -> $Validating
            }
        }

        $Validating {
            $.attempts: int = 0

            $>() {
                $.attempts = $.attempts + 1
                if self.validate(self.order_data):
                    -> $Processing
                else:
                    if $.attempts >= self.max_retries:
                        -> $Failed
            }

            getStatus(): str {
                @@:("validating")
            }
        }

        $Processing {
            $>() {
                logging.info("Processing order")
            }

            cancel(reason) {
                -> (reason) $Cancelled
            }

            getStatus(): str {
                @@:("processing")
            }
        }

        $Cancelled {
            $>(reason) {
                logging.info(f"Cancelled: {reason}")
            }
        }

        $Failed {
            $>() {
                logging.error("Order failed")
            }
        }

    actions:
        validate(data) {
            return data is not None
        }

    domain:
        max_retries: int = 3
        order_data = None
}

if __name__ == '__main__':
    proc = @@OrderProcessor(5)
    proc.submit({"item": "widget", "qty": 3})
```

---

## Appendix: Frame Syntax Taxonomy

Frame's surface syntax divides into a small, closed set of categories. This
appendix names each with standard compiler terminology and is the source of the
vocabulary used throughout this guide. Every classification here is verified
against *emitted code* by `framec/tests/syntax_taxonomy.rs` — it states what framec
does, not what the syntax looks like.

The central fact: **Frame has almost no expression grammar of its own.** There
are no operators, literals, precedence, or control-flow keywords (`if`/`while`/
`for` are not Frame tokens). The value-bearing parts of a handler line are
**native** code; Frame contributes only *references* (which splice a value into
that native expression) and *calls* (which produce one). So in
`self.total = $.x + n * 2`, the whole line is a native expression with a single
Frame reference (`$.x` → `self.x`) spliced in.

### Categories

1. **Sections** — the five block headers that partition a system:
   `interface:`, `machine:`, `actions:`, `operations:`, `domain:`.

2. **Declarations** — introduce a named entity: the system; a state
   (`$State(params) => $Parent`); a handler (`event(params): ret`, plus the
   `$>` enter and `<$` exit lifecycle handlers); interface / action / operation
   methods; a state variable (`$.x: T = init`); a domain field
   (`[const] x: T = init`).

3. **Statements** — executed for effect; yield no value:
   - *Control flow*: transition `->`, forward `=>` / `=> $^`, push `push$`,
     pop `-> pop$`.
   - *Mutations* (property **setters**): `$.x = e`, `@@:data.key = e`,
     `@@:return = e`, and `@@:(e)` (sugar for `@@:return = e`).
   - *Exit-return*: `@@:return(e)` — a setter **plus** an exit.

4. **Expressions** — yield a value. Frame has exactly two kinds:
   - *Property references* (**getters**): `$.x`, `@@:return`, `@@:data.key`,
     `@@:event`, `@@:params.x`, `@@:system.state.name`, `@@:self`.
   - *Call expressions*: `@@:self.method(args)` (re-entrant self-dispatch) and
     `@@Sys(args)` / `@@!Sys()` (system instantiation). Both are usable in value
     position (assignment RHS) and, standalone, as expression-statements.

5. **Attributes / Pragmas** — transpile-time metadata, never runtime:
   `@@[target(...)]`, `@@[persist]`, `@@[main]`, `@@[create/save/load/no_persist]`,
   and the bare directives `@@import`, `@@codegen`, `@@run-expect`, `@@skip-if`,
   `@@timeout`.

6. **Native code** — opaque target-language passthrough. The only place where
   whitespace belongs to the user rather than to Frame.

### Properties and accessors

The organizing concept beneath categories 3–4 is the **property**: a named,
Frame-managed place value. A property exposes up to two **accessors**:

- a **getter** (read) — a *Reference*; it is an **expression** (yields a value);
- a **setter** (write) — a *Mutation*; it is a **statement** (a store).

| Property | Getter (Reference) | Setter (Mutation) |
|----------|--------------------|-------------------|
| `$.x` | yes | yes |
| `@@:data.key` | yes | yes |
| `@@:return` | yes | yes (`= e`, `@@:(e)`; `@@:return(e)` also exits) |
| `@@:event` | yes | — (read-only) |
| `@@:params.x` | yes | — (read-only) |
| `@@:system.state.name` | yes | — (read-only) |
| `@@:self` | yes | — (read-only) |

"Two kinds of accessor" = getter and setter. A read-only property has only a
getter.

### The word "return" names two unrelated things

- **Native `return e`** — the host language's own keyword. *Passthrough*:
  emitted verbatim. It is **not** a Frame construct; the parser only recognizes
  it so it can reason about control flow (does this path return?).
- **Frame return** — the `@@:` family that writes the Frame-managed return slot
  on the event context: `@@:return = e` (setter), `@@:(e)` (sugar for it),
  `@@:return(e)` (setter + exit), and `@@:return` (getter). These are **not**
  passthrough — each lowers to a read/write of the runtime return slot.

### Whitespace sensitivity

- **Tier A — whitespace-invariant.** All structural Frame tokens (every
  statement, reference, call, and the `@@:` / `$.` families). Whitespace
  *between* Frame tokens — including line breaks, tabs, and `\r\n`/`\r` — is
  insignificant: any permutation must produce byte-identical output.
- **Tier B — whitespace-significant.** The `domain:` section (indentation marks
  the section's end) and section ordering.
- **Tier C — native passthrough.** Whitespace is the user's and is preserved
  verbatim.

### Authority

The construct list is derived from the lexer's `Token` set and the parser's
`Statement` variants; the category each construct belongs to is pinned by
`framec/tests/syntax_taxonomy.rs`, which asserts the lowered form (statement vs
reference vs mutation vs passthrough) against emitted code. When this guide uses
a term — *statement*, *expression*, *property*, *accessor*, *reference*,
*mutation* — it means it in the sense defined here.