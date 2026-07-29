# reifier — framec, reifying itself

The doctrine: **framec is the Smalltalk of machines.** Smalltalk made everything an
object, no exceptions of convenience. framec makes everything a **machine**. The only
carve-out is the OS boundary — `main`, the CLI entry, the syscall/`Sink`-write
primitive, FFI — excepted not by choice but because we physically *cannot* machine the
operating system. The discriminator is not the advocate's "does naming pay?" nor even
the ghost-buster's "does it branch?" — it is **"can this be a machine?"** If yes, it
must be one.

Total reification of framec's own logic is mechanical and thousands-of-sites, so it is
a **compiler problem, not a hand-grind**: this tool reads a native Rust `fn` and emits
its `@@system` (`.frs`) per the calculus below. Optimization of the resulting
fine-grained systems is deferred to the framepiler.

## The reification calculus

| native construct | Frame form |
|---|---|
| `fn f(params) { body }` | `@@system F(params, out) { interface: step() machine: <body> domain: <params> }` |
| a run of straight-line statements (no decision) | a state chain `$S0 -> $S1 -> ...` (one state per statement; the statement is the state's native action) |
| `if c { a } else { b }` | a fork: guard state transitions `-> $Then` / `-> $Else`, both `-> $Join` |
| `match e { arms }` | a fork with one state per arm |
| `for x in it { body }` / `while c { body }` / `loop` | a cycle: a cursor register in `domain`, a self-transition `-> $Loop`, an exit edge |
| `.iter().map(f).collect()` / `.fold(..)` | a cycle accumulating into a `domain` register |
| `return` / `break` / `continue` / `?` | transitions to terminal / loop-exit / loop-head / error states |
| terminal op (`out.frame(..)`, a field read, `format!`) | a state's native action (the irreducible leaf — a single non-branching, non-iterating expression) |

## The one exception

A `fn` is left native **iff it is the OS boundary** — `main`, the CLI/arg entry, the
`Sink` write primitive, an FFI shim — i.e. iff it *cannot* be a machine because the
thing it talks to is not one. Everything else is a target. The tool flags an
OS-boundary fn and skips it; it never skips anything else.

## Golden test

`compiler/src/text/emit/rust_enter/rust_enter.frs` — the hand-built control-flow
reification of rust's `enter` (the 4-way arg-case fork as 4 explicit states). The
reifier must reproduce that structure from `enter`'s Rust, then go deeper (the
`vars_expr` / `args_default_expr` folds → their own cycle-systems).

## Status

Increment 1: parse a fn via `syn`; emit the `@@system` shell (domain from params) and a
first-pass body decomposition (straight-line → state chain; `if`/`match` → forks;
`for`/`while` → cycles). Not yet byte-faithful to the golden test; the calculus is
built up increment by increment against it.

Run: `cargo run -- <path/to/file.rs> <fn_name>`
