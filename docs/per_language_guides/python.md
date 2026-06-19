# Per-Language Guide: Python

Python is the de facto baseline for most Frame documentation —
the cookbook examples, the runtime spec walkthroughs, and the
matrix's primary fixtures all use Python first. The Python target
maps cleanly to Frame's structural model: classes for systems,
methods for events, dynamic typing, `async def` / `await` for
async, `f"..."` for interpolation.

This guide documents the Python-specific patterns. Most are
unsurprising to anyone familiar with modern Python (3.7+).

For the canonical capability table, see
`framec-test-env/docs/runtime-capability-matrix.md`. Python is
fully spec-conformant on every row.

---

## Foundation: class with method dispatch

A Frame system targeting Python generates a single `.py` file
containing:

- A `class WithInterface:` block.
- An `__init__(self)` constructor that fires the start-state's
  `$>` cascade.
- One `def greet(self, name)` method per interface entry.
- Internal `_state_<S>(self, ...)` and
  `_s_<S>_hdl_<kind>_<event>(self, ...)` helpers.

```python
class WithInterface:
    def __init__(self):
        # start-state $> cascade fires here
        self.call_count = 0

    def greet(self, name):
        # ... handler body
        return result
```

Frame's `@@:self.field` lowers to `self.field` directly (Python's
instance reference is `self`). Method calls use `s.greet("World")`.

---

## Domain fields: dynamic attributes set in `__init__`

Domain fields lower to attributes set in `__init__`:

```frame
domain:
    call_count: int = 0
    name: str = "alice"
```

```python
def __init__(self):
    self.call_count = 0
    self.name = "alice"
    # ... runtime fields
```

Frame's `: type` annotation is documentation only — Python is
dynamically typed at runtime, though type hints (PEP 484) are
respected by static analyzers like mypy.

---

## Strings: `+` for concat, `f"..."` interpolation

Python's `+` operator concatenates strings. F-strings (Python 3.6+)
provide interpolation:

```frame
$Ready {
    greet(name: str): str {
        self.call_count += 1
        @@:(f"Hello, {name}!")
        return
    }
}
```

```python
def greet(self, name):
    self.call_count += 1
    return f"Hello, {name}!"
```

For older Python versions, `"Hello, {}!".format(name)` and
`"Hello, %s!" % name` also work. F-strings are preferred for
readability.

---

## Async: `async def` with `await`

Frame's `async` interface methods on Python lower to `async def`
methods returning coroutines:

```frame
async fetch(key: str): str {
    @@:return = await self.cache.get(key)
}
```

```python
async def fetch(self, key):
    __result = await self.cache.get(key)
    return __result
```

Python async is mature:

- `async def` for coroutine-returning functions.
- `await EXPR` at call sites.
- `asyncio.run(...)` to drive from sync code.
- The matrix harness uses `asyncio.run(main())` for async test
  drivers.

---

## Concurrency and re-entrancy

Frame does **not** provide thread-level concurrency control — the
context stack, compartment, and state-variable storage on a system
instance are **not** guarded by locks or atomics. The intended
deployment model is:

> **One system instance per session, driven by a single sequential
> driver.**

For an automation pipeline, a long-running daemon, or a background
worker, that typically means one `asyncio` task or one Python
thread owns each `@@Counter()` instance and serializes events
through it.

For an `@@[async]` system (RFC-0043), the generated **casing**
*enforces* this contract on a single event loop: each interface
method checks a `busy` flag on entry and raises
`RuntimeError("E703: …")` if a second external call arrives while
the first is still in flight, then clears the flag in a `finally`.
This is a cooperative single-driver gate, not a lock — it catches
the accidental re-entry described below rather than serializing
true OS-thread parallelism.

### What's safe

- **Sequential event dispatch on one instance.** Call `s.event_a()`,
  let it return, call `s.event_b()`. This is the normal usage.
- **`@@:self.method()` from inside a handler.** Frame's context
  stack handles re-entry on the same instance — the inner call
  runs to completion before control returns to the outer handler.
  Cookbook recipes covering self-call (e.g. transitions triggered
  from a handler body) rely on this.
- **Independent instances on independent driver tasks.** Two
  `Counter()` instances driven by two `asyncio` tasks do not
  contend.

### What's not safe (without external serialization)

- **External re-entry during `await`.** If interface method `A` is
  an `async` method and is currently suspended on
  `await self.cache.get(...)`, calling interface method `B` on the
  *same* instance from another `asyncio` task re-enters dispatch
  against a partially-progressed context stack. On an `@@[async]`
  system the casing **detects** this and raises
  `RuntimeError("E703: …")` — the corruption is turned into a loud,
  catchable error. On a non-`@@[async]` system there is no gate and
  the re-entry runs silently; serialize externally.
- **Multi-threaded access to one instance.** Frame's generated
  Python code has no `threading.Lock` around dispatch — the E703
  gate is a plain boolean, not an atomic, so it does not make an
  instance thread-safe. Two OS threads calling methods on the same
  instance still race on the compartment and state vars.

### Pattern: serialize external events

If your driver has multiple sources that can fire events into the
same instance (HTTP handlers, scheduler ticks, signal handlers),
put a per-instance `asyncio.Queue` (or `queue.Queue` for threaded
drivers) in front of the system and drain it from a single
consumer task:

```python
import asyncio

async def driver(system, inbox: asyncio.Queue):
    while True:
        event, args = await inbox.get()
        await getattr(system, event)(*args)
        inbox.task_done()

system = Counter()
inbox = asyncio.Queue()
asyncio.create_task(driver(system, inbox))

# Producers (handlers, schedulers, sockets) only enqueue.
await inbox.put(("bump", ()))
```

This keeps the "one driver per instance" invariant while letting
many producers fire events.

### Persistence under concurrency

`save_state()` requires the system to be **quiescent** — no event
in flight, `_context_stack` empty. Calling it from inside a
handler raises `RuntimeError("E700: system not quiescent")`. In a
queued-driver design, save between drains (after `inbox.get()`
returns and before the next event runs) or with the driver paused.

---

## Cross-system fields: direct instantiation

`var counter: Counter = @@Counter()` lowers to an instance
attribute:

```python
def __init__(self):
    self.counter = Counter()
    # start-state $> fires

def notify(self):
    self.counter.bump(1)
```

---

## Loop idioms — both work

Python has `while`, `for-in`, and various iterator protocols.
Frame's idiom 1 (`while cond { ... }`) compiles to a Python
`while cond:` block via passthrough.

---

## Multi-system per file: works as you'd expect

A `.fpy` source containing multiple `@@system` blocks compiles
to a single `.py` file with multiple class definitions.

---

## Comments and the Oceans Model

Frame's "Oceans Model" applies to Python the same way it applies
to every other backend. The comment leader is `#` (line); for
docstrings, use `"""..."""` (triple-quoted).

```frame
@@[target("python_3")]

# Module-prolog block — passes through as Python source.

@@system Counter {
    machine:
        # Section-level comments are preserved as native # blocks.
        $Counting {
            tick() { ... }
        }
}
```

---

## Idiomatic patterns and common gotchas

**`self.field` everywhere.** Python's explicit `self` argument
on methods means handler bodies always reference `self.x`
explicitly. Frame's codegen handles this.

**No `new` keyword.** `Counter()` is the constructor call.
Frame's `@@Counter()` lowers to `Counter()`.

**Indentation matters.** Python is whitespace-sensitive. Frame
generates correct indentation; if you write native Python in
handler bodies, watch for tab/space consistency.

**`None` is the absent-value marker.** Python's `None` is the
universal nil-value.

**`print(...)` for output.** Built-in, no import needed.

**Common imports: `import os, sys, asyncio`.** Use the prolog
for `import` declarations.

---

## Persist contract — `@@[save]` / `@@[load]`

A persisted system declares three system-level attributes:
`@@[persist(<blob_type>)]`, `@@[save(<save_method_name>)]`, and
`@@[load(<load_method_name>)]`. Framec generates the save/load pair
on the system class — save returns the blob, load is an instance
method that mutates self.

```frame
@@[persist(str)]
@@[save(pickle)]
@@[load(unpickle)]
@@system Counter {
    interface:  bump()
    machine:    $Active { bump() { self.n = self.n + 1 } }
    domain:     n: int = 0
}
```

Load is an instance method (allocate, then populate):

```python
data = c1.pickle()
c2 = Counter()
c2.unpickle(data)
```

Bare `@@[persist]` (no save/load names) is rejected with **E814**.
The legacy operation-attribute form (`operations: @@[save] foo()`)
is rejected with **E819** at framec 4.1.0+; the codemod at
`scripts/migrate_rfc0015.py` rewrites old fixtures mechanically.
See [`frame_runtime.md`](../frame_runtime.md) "Naming the save/load
methods" and [RFC-0015](../rfcs/rfc-0015.md) for the design.

## Persist quiescent contract — E700

`save_state()` requires the system to be quiescent (no event in
flight, `_context_stack` empty). Calling it from inside a handler
raises `RuntimeError("E700: system not quiescent")`. The error is
catchable via `try/except`, but recovery isn't possible — the
handler's context frame is corrupted; discard the instance and
restore from a prior snapshot. See
[`docs/frame_runtime.md`](../frame_runtime.md) and
[`rfc-0012`](../rfcs/rfc-0012.md) for the full contract.

## Persist uses JSON

Frame's Python `save_state()` returns field-by-field UTF-8 JSON
bytes (`json.dumps(state_data).encode("utf-8")`) and
`restore_state()` reads them back with `json.loads`. There is no
`pickle.dumps`/`pickle.loads` in generated Python, so restoring a
snapshot does not execute arbitrary code from the blob.

The blob carries the saveable fields, not a whole-object graph:
`save_state()` returns `bytes` regardless of the declared
`@@[persist(<type>)]` argument, and the canonical JSON shape is
the same wire format the other dynamic backends already use. See
[`frame_runtime.md`](../frame_runtime.md) "The canonical format"
for the structured `StateBlob` layout.

Unlike whole-object pickle, JSON persist does not preserve shared
object identity or reference cycles in the domain. RFC-0012
discusses that trade-off; the pickle → JSON migration itself
shipped in 4.2.0 (see `CHANGELOG.md`).

---

## Testing a Frame system

Frame's canonical test pattern is **white-box assertion through
operations**, documented in detail as
[Cookbook Recipe 32 — Test Harness](../frame_cookbook.md#32-test-harness--white-box-testing-with-operations).
The shape:

1. Read state via `@@:system.state` inside an operation. It returns
   the current state name as a string (no `$` prefix) and is
   read-only — assignment is a parse error.
2. Expose any state-variable values you need to assert on through
   additional operations (operations don't dispatch events, so
   they're safe inspection points).
3. Drive the system through events in your `pytest` / `unittest`
   driver and assert against those operations.

```frame
@@system Counter {
    interface:
        tick()
    machine:
        $Idle {
            tick() { $.ticks = $.ticks + 1; -> $Running }
        }
        $Running {
            tick() { $.ticks = $.ticks + 1 }

            $.ticks: int = 0
        }
    operations:
        current_state(): str { @@:(@@:system.state) }
        tick_count(): int    { @@:($.ticks) }
}
```

```python
def test_counter_advances_to_running():
    c = Counter()
    assert c.current_state() == "Idle"
    c.tick()
    assert c.current_state() == "Running"
    assert c.tick_count() == 1
```

For deeper introspection (compartment, state stack), generated
Python exposes `obj._compartment.state`,
`obj._compartment.state_vars`, etc. These are not part of the
documented stable surface — operations are the supported route.

**Per-event tracing / mocking actions:** Frame does not currently
provide a built-in transition-trace callback or action-stub mode.
If you need to suppress side effects in unit tests today, write
thin actions that delegate to injectable callables and replace
them in the test fixture.

---

## Cross-references

- `docs/runtime-capability-matrix.md` — per-backend capability
  table; Python shows ✅ on every row.
- `tests/common/positive/primary/02_interface.fpy` — canonical
  interface-method shape with f-string interpolation.
- `framec/src/frame_c/compiler/codegen/backends/python.rs` —
  Python backend codegen.
- `memory/python_runner_fix_2026_04_26.md` — context on the
  Python test runner fix that resolved 282 silent no-op tests.
