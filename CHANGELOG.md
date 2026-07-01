# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [4.6.1] - 2026-06-20

### Fixed

- **PHP: a non-constant domain-field initializer is lowered into the
  constructor (#144).** PHP class-property defaults admit only *constant
  expressions*, so `public $inner = new Counter();` — or any call /
  `@@<System>()` instantiation — is rejected by the PHP parser ("New
  expressions are not supported in this context") while framec exits 0. The
  field-emission path stripped only `@@`-tagged instantiations, so a native
  `new X()` / `foo()` slipped through as an invalid property default. framec now
  applies one predicate — `php_init_needs_constructor` — symmetrically to the
  property-strip decision and the constructor-body emission: any non-constant
  initializer is declared without a default and assigned in `__construct`
  (before the compartment setup, so enter handlers see it), while constant
  scalars / strings / arrays stay inline. This matches how every other OO
  backend already lowers such initializers. Validated with `php -l` and a
  runtime round-trip on PHP 8.5.
- **Semicolon targets: Frame call statements are terminated by a forward,
  provably-closed rule (#116, #117).** A Frame call segment (`@@:self.method()`
  or `@@:self.field.method()`) needs a trailing `;` on semicolon targets when —
  and only when — it ends a statement. The original fix detected this by scanning
  *backward* over what precedes the call (parens, then condition keywords, …), an
  open-ended enumeration of expression contexts that could never be complete: it
  shipped #116 (a void field-call statement got no `;` → `CS1002`), then its
  patch shipped #117 (a value call in a paren-less `if @@:self.f.m() {` got a
  stray `;` → "missing condition" on Go/Rust/Swift). Both are now decided by a
  single *forward* characterization — a call ends a statement iff nothing
  continues the expression after it on its source line (the next token past
  horizontal whitespace and an optional line comment is a line break, `}`, or
  end-of-body). This is closed and target-independent, and additionally fixes
  latent cases the backward rule missed — an assignment ending in a call
  (`int x = @@:self.f.reading()`) and a call with a call-valued argument
  (`@@:self.f.poke(@@:self.f.reading())`) — while declining conditions and
  arguments. A user-written `;` is never doubled. Validated by an adversarial
  cross-product (every call position × all 12 semicolon backends, generated and
  compiled with the real toolchain) plus the 17-language matrix. Contract: framec
  terminates a line only when its own emitted call is that line's last token; a
  call written mid-expression with a native tail (`@@:self.f() + 1`) is the
  author's to terminate, per the Oceans model (native is passthrough).
- **Deterministic codegen: the auto-derived interface (no explicit `interface:`
  section) is emitted in source order.** It previously iterated a `HashSet`, so
  interface-method emission order was randomized per run — non-reproducible
  output, and flaky codegen diffs/snapshots. Now emitted in declaration order,
  matching the explicit-`interface:` path. (Residual of commit `72f3ea5`, which
  made state/handler iteration deterministic but missed this derive path.)
- **Dart: generated files now carry an `// ignore_for_file` header (#110).** The
  Dart backend emitted no suppression header, so the generated runtime
  scaffolding (`_parameters`, `_state_stack`, `__prepareExit`, snake_case members,
  …) tripped `dart analyze` warnings/lints in any normal project (a 3-system FSM
  raised ~900). framec now prepends the header — the Dart parallel to the Rust
  backend's `#![allow(...)]` — driving generated output to 0 analyzer issues.
- **Go: same-system action calls are no longer PascalCased (#115).** `@@:self.<m>()`
  was always capitalized for Go (`s.M()`), correct for exported interface methods
  but wrong for **actions**, which are private/lowercase (`func helper`) — so the
  call referenced an undefined exported method and the generated Go didn't compile.
  Capitalization is now gated on the method being an interface method, not an
  action. (Pre-existing in 4.6.0; surfaced alongside #112, which fixed the inverse
  cross-system interface-call case.)
- **Dart: the generated system's parameter-taking constructor is now public
  (#108).** Dart construction previously went through `static {Sys} _create(...)`,
  whose leading underscore makes it **library-private** — a system generated into
  its own file could not be instantiated from a separate host library (the public
  no-arg `{Sys}()` left `late` domain fields uninitialized → `LateInitializationError`).
  framec now emits a public **factory constructor** `factory {Sys}.create(...)`.
  This is the idiomatic Dart form and — unlike a `static create` method — coexists
  with a user interface method named `create`. Call sites (`@@Sys(...)` instantiation
  and the `@@[create(name)]` alias) updated to `{Sys}.create(...)`. Dart-only; every
  other backend's factory was already cross-module-callable.
- **Go: cross-system interface calls now use the exported (PascalCase) method
  name (#112).** Go exports interface methods by capitalizing the first letter
  (`tick` → `Tick`), but a cross-system call `@@:self.field.method()` emitted the
  raw lowercase name (`s.field.tick(...)`), referencing a method that doesn't
  exist — the generated Go didn't compile. Root cause: the Go per-handler context
  was built with an empty `domain_field_types` map, so the call was never
  recognized as a cross-system (embed) call. framec now threads the field→type map
  into the Go handler context and capitalizes the method at embed call sites,
  using the same shared `capitalize_first` as the definition rename so the two
  can't drift. Go-only.
- **Scanner: an apostrophe in a `#` comment on a `domain:` field no longer
  swallows the following field (#113).** A lone `'` (e.g. in "it's") inside a
  trailing comment opened a string in the single-line domain-RHS scanner
  (`ExprScannerFsm`) that ran past the newline, absorbing the next field
  declaration into the first field's initializer — emitted **verbatim** into the
  generated constructor instead of being transpiled (a silent miscompile;
  `framec` exited 0). The scanner now stops an unterminated string at the
  depth-0 newline in single-line mode. Pre-backend, so all targets benefit; a
  balanced `"…"` in a comment was never affected. The companion *own-line* form
  — a `#` comment with a prose apostrophe on its own line *before* a field
  (`# don't drop the fields below` followed by `x: int = 1`) — is handled by the
  `domain:` line scanner's comment-trivia path, which consumes the whole line to
  the newline and never opens a string; regression coverage now pins both the
  own-line and trailing forms (apostrophe and double-quote), the typed no-init
  trailing-comment case, and a genuine apostrophe-bearing string initializer.
- **`@@:return` — void form and action/operation-body form now lower on every
  backend (#141).** A void `@@:return()` emits a native early-return with no
  spurious `_return =` assignment, and `@@:return(expr)` is now lowered inside
  action and operation bodies (not just event handlers) via a balanced-paren
  scan. A false-positive Rust E606 on the void form was exempted. All 17 targets.
- **`@@:self.<operation>()` is accepted by the validator (E601).** Operations are
  callable via the portable `@@:self` form, not only interface methods; the
  validator no longer rejects the operation case.
- **Erlang: only `@@:self`-marked calls lower — bare native `self.X()` passes
  through unchanged.** Aligns Erlang with the Oceans call model: framec stops
  rewriting native call syntax and lowers only explicit `@@:self` references
  (marker-based). Native child/self calls are the author's, per target idiom.
- **Erlang: `@@[persist]` affinity, `@@[*persist]` broadcast, `@@:data`
  call-scoping, `@@:return(e)` short-circuit, `pop$` exit dispatch, and
  operation-body conditionals are runtime-correct (#127, #132).** A sweep that
  moved Erlang from "compiles clean" to "behaves correctly": per-system persist
  affinity with a `@@[*persist]` broadcast opt-in (#127); reserved-word state
  quoting, lowercased domain fields, enter state-arg binding (#132 A–D); and
  `@@:data` scoping, `@@:return` short-circuit, and `pop$`-driven exit dispatch
  (#132 E–G). Nested control-flow arms now return valid `gen_statem` tuples
  (#119, partial — the else-if-without-trailing-else case closes its outer case).
- **Erlang: non-terminal `if` threads `Data` through the `case` to trailing code
  (#125).** A no-else/else-if `if` that merely mutates and falls through no longer
  drops its trailing statements or wraps them in a spurious `__ReturnVal = case`;
  a `body_is_terminal` check gates the early-exit deferral so trailing code runs
  unconditionally after the `end`. Discriminated by the `@@:return` short-circuit
  sentinel vs the `@@:` fall-through setter — no `gen_statem` IR (#119) needed.
- **Erlang: machine-less systems run via a synthetic `init_state/3` (#121).** A
  system with no `machine:` section previously produced no start state; framec now
  synthesizes one so the system initializes.
- **Lua: `else if` chains lower to `elseif`, nested `if` inside `else {}` keeps
  its brace, table constructors survive, cross-system calls pass `self`, and the
  module exports its systems (#120, #122, #124, #134, #135).** A cluster of Lua
  block-lowering fixes: a direct `} else if c {` link now lowers to `elseif …
  then` instead of the invalid `end else if … then` (#124); a nested `if` inside
  an `else { … }` no longer leaks a brace (#135); `table.insert(t, {…})` literals
  inside handler bodies keep their `{}` (#122); bare action and cross-system calls
  emit the colon form so `self` is passed (#134), and the generated module now
  actually exports its systems (#120).
- **Persist: const domain fields are emitted mutable on typed backends so restore
  can reassign them (#138, #139).** A persisted `const` field must round-trip
  through `restore`, which reassigns it — so it can no longer carry an immutable
  modifier (`final`/`readonly`/`val`/`let`/`const`) on the 6 affected typed
  backends (#139); TypeScript persist output additionally now type-checks under
  `tsc --strict` (#138). Frame-level `const` stays enforced by the validator.
- **Transitions: a forward `-> => $S(args)` carries its argument decorations with
  no false `W414` (#128); Rust drops redundant parens on a bare context-read
  return (#129).** Forward (re-dispatch) transitions now propagate state/enter-arg
  decorations correctly and no longer trip a spurious unreachable-state warning
  (#128); a Rust bare `@@:(ctx.field)` return no longer wraps in redundant parens
  (#129).
- **Validator: a bare `@@:return` used as a no-op statement warns (W416) (#130).**
- **Kotlin: state-variable read casts are parenthesized so a following operator
  binds correctly.** `$.i` read as `(state_vars["i"] as Int)` rather than the
  unparenthesized form, which mis-parsed in `while ($.i < n)` and similar.
- **CLI: the output-filename picker is comment/string-aware (#142).** The scan for
  the public class name that names the output file (`javac` requires
  `<Name>.java`) delegated to a naive `find("public class ")`, which matched the
  token inside a native comment or string literal and misnamed the file. It now
  walks the source via the language's `SyntaxSkipper` (the dogfooded native-region
  primitive) and requires a word boundary.
- **Parity: empty / machine-less systems compile on Swift, Rust, and Kotlin.** A
  system with no states or an empty `machine:` now emits a valid, constructible
  class on these targets (Swift additionally seeds `__compartment`), matching the
  dynamic backends.
- **Robustness: the per-backend text rewriters are string/comment-safe.** A
  post-audit sweep hardened the backend body/call rewriters (including Erlang) so
  they never corrupt content inside string literals or comments.

## [4.6.0] - 2026-06-19

### Added

- **RFC-0042.1 — pluggable input source for Rust `@@fsm` recognizers.** A generated
  Rust recognizer is now generic over where it reads input: an owned buffer
  (`Vec<char>` / `Vec<String>` / `Vec<u8>`), a borrowed slice (`&[char]` / `&[u8]` /
  `&[String]`, zero-copy over the host's buffer), or a host callback
  (`<Name>Fn(closure, len)`) — all behind a small per-fsm `fsm_get` / `fsm_len`
  accessor. Two new drive forms join the unchanged `new()`: `over(src)` builds the
  recognizer **without** running it, and `scan_at(start)` re-seeds and re-runs from
  any offset. Build once, scan many: an `@@fsm` can now be used as a reusable,
  zero-copy scan-at-cursor lexer primitive instead of being constructed with its full
  input each call. Mode-C inner recognizers stay owned (materialized through the
  accessor). Existing `@@fsm` source compiles identically — this is additive to the
  generated API. Rust backend only; the other 16 backends keep the
  construction-driven form (the `fsm_get`/`fsm_len` + `over`/`scan_at` contract is
  uniform for the follow-on).

### Changed

- **The framec lexer now recognizes number, string, and identifier tokens through
  generated `@@fsm` recognizers** — dogfooding `@@fsm` on the compiler's own lexical
  leaves, driven zero-copy over the source bytes via RFC-0042.1. Tokenization is
  unchanged: identical output across all 17 targets (the differential matrix is
  unaffected). Internal refactor, no user-visible behavior change.

### Removed

- Dead internal scanner modules `pragma_scanner` and `prolog_scanner` (no production
  callers).

### Fixed

- **`@@fsm` Mode C:** an invalid `@`-reference at the start of a regex stage now
  raises a diagnostic (E731/E732) instead of emitting uncompilable code (#100).
- **`@@fsm` segmentation:** the block body closer is now regex-literal-aware, so a
  literal `{` / `}` or an unbalanced quote inside a regex literal no longer trips
  `E001 Unterminated` (#103).
- **Docs:** Python `@@[persist]` is documented as JSON, not pickle — `save_state()`
  emits field-by-field UTF-8 JSON and `restore_state()` reads it with `json.loads`,
  so a restored snapshot does not execute code from the blob. The pickle → JSON
  migration itself shipped in 4.2.0 (#101, #104).

## [4.5.0] - 2026-06-16

### Added

- **`@@fsm` — finite-state recognizer construct (RFC-0042).** A new top-level
  construct that compiles a regular-language recognizer to a Pike-VM-backed state
  machine, emitted on **all 17 target languages at full behavioral parity**. It
  implements the RE2 regular-expression dialect across the regular-language
  feature set: literal, character-class, and Unicode `\d`/`\w`/`\s` alphabets
  (plus `\p{...}` classes via `@@[allow(unicode_classes)]`); `|` ordered choice;
  greedy **and** lazy quantifiers; edge **and** interior anchors; character-,
  byte-, and Unicode-aware word boundaries `\b`/`\B`; inline flags `(?i)`/`(?m)`/`(?s)`
  and scoped `(?ims:…)`/`(?-i:…)`; captures with action blocks; `when`-conditional
  and stage-reference transition targets; a token alphabet; multi-match (`|`)
  states; embedding; and Mode-C sub-fsm call-out. Zero-width assertions are
  evaluated by a dedicated Pike-VM `Assert` opcode wired into every backend. The
  action-body statement grammar is specified in RFC-0050.
- **`@@:self` — portable, blessed self-reference (RFC-0046).** Write
  `@@:self.field`, `@@:self.action()`, and `@@:self.field.method()` (embed calls),
  including inside return expressions and across systems; framec lowers each to its
  target's native `self`/`this` idiom on all 17 backends. This replaces the
  per-language textual self-rewriters with segment-driven lowering.

### Changed

- **Type annotations now emit verbatim on every backend (#61).** Removed the
  residual per-backend type-alias tables (`int`→`i64`, `str`→`String`,
  `float`→`f64`, …) from the Rust pipeline (`rust_system.rs`/`runtime.rs` typed
  compartment + the return-boxing cast shim) and from `map_type`/`convert_type`
  on the other backends. Frame has no type system — write your target's own type
  names; they reach the generated code unchanged. Machinery types (generic
  stacks, untyped event params) are unaffected.
- **State-variable initializers now emit verbatim.** Init values carry raw text
  and are emitted exactly like domain-field initializers; the per-target
  "portable init" value wrapping (`""` → `String::from("")`, …) was removed. Write
  the native init value for the declared type.
- **Section comments now emit verbatim (#58).** framec no longer rewrites a `//`
  comment leader to the target's native one (`#`/`--`/`%`). Comments pass through
  unchanged like everything else — write your target's own comment syntax
  everywhere. This was the last source transform framec performed.
- **State variables now require an explicit initializer (#84, `E610`).** The
  synthesized-default value table was deleted — no magic. A state variable
  declared without an initializer is now an `E610` error; write the native init
  value for its declared type, exactly as for a domain field. *Breaking;
  pre-public-beta, mechanical to fix.*

### Fixed

- **Whole-number float state-var initializers no longer truncate to integers
  (#59).** `$.x: f64 = 0.0` previously parsed-and-reserialized through
  `f64::to_string()` and emitted `0` (uncompilable Rust; wrong on every typed
  target). State-var inits are now verbatim, so the literal is preserved.
- **A `#` in a structural section no longer throws `E002` (#58).** `#` was
  rejected as an unexpected byte in `interface:` while passing through verbatim
  in `domain:`/handlers. The structural scanner now passes `#` through like any
  non-Frame text, consistent across all sections (the Oceans Model).
- **Rust: param-referencing domain initializers no longer require `Default` (#67).**
  A domain field whose initializer references a ctor param (a parameterized embed
  `inner: Inner = @@Inner(p)`, or a non-`Default` handle like `Gd<Node>`) emitted
  `Default::default()` in the parameterless `new()` — uncompilable for non-`Default`
  types. For systems with such fields, framec now skips `new()` and builds
  `__create(<params>)` directly with the params in scope (`inner: Inner::__create(p)`).
- **C: `float`/`double` arguments on `pop$` now marshal correctly (#83, RFC-0048).**
  C is the only backend whose `void*` argument slots are not self-describing, so a
  floating-point value pushed for a `pop$` transition was reinterpreted, not
  converted. A C11 `_Generic` value-dispatch macro (`{sys}_ARG_PUSH`) now boxes
  each argument by its static type. RFC-0048 generalizes the contract for future
  low-level targets.
- **C: doubles are heap-boxed through `void*` slots — pointer-width safe (#81).**
  A `double` no longer aliases a pointer slot (truncated on wasm32/32-bit targets);
  it is boxed in an owned allocation, with the ISO-C `strdup` shim for strings.
- **C: category-driven `void*`-slot marshalling (#72) and pointer-typed embed
  calls (#73).** Floats unpack by value, structs box; pointer-typed embed calls
  lower to the free-function family.
- **Declared-type coercion at every type-erasure write site (#77, #78).** The
  coercion that re-types a value leaving an untyped compartment slot now also
  covers the `@@:return(expr)` early-return path, with runtime gates.
- **C++ and Swift core generated code is now exception-free / exception-safe
  (RFC-0049; #86, #87, #88, #89).** Per the new Exception Policy, core dispatch
  maintains its context-stack invariant with each language's idiomatic
  scope-cleanup rather than a mandatory catch-and-rethrow, so it compiles under a
  target's no-exceptions mode wherever one exists:
  - **C++ dispatch (#86):** a method-local RAII scope-guard replaces the
    `try { … } catch(...) { pop; throw }` wrapper, so output compiles **and links**
    under `-fno-exceptions` (required by Godot web GDExtensions).
  - **C++ persist (#87):** no-throw pointer probing `std::any_cast<T>(&v)` replaces
    `try { any_cast } catch`; the `E700` type-mismatch path is `#if`-fallback to
    `abort()` when exceptions are off.
  - **C++ async (#88):** an RAII gate-guard replaces the busy-gate try/catch; the
    `E703` single-driver violation uses the same `#if`-guarded throw/abort fallback.
  - **Swift (#89):** context-stack cleanup moved into `defer`, exception-safe (R4).
- **C++: persisted systems now `#include <nlohmann/json.hpp>` (#94).** Persist
  codegen emitted `nlohmann::json` without the include, so a persisted system did
  not compile standalone; the include is now emitted whenever a system persists.

### Docs

- Per-language guides, the language reference's type-contract and "init values"
  sections, and the portable-float guidance (#62) updated to the verbatim-native
  contract. See [4.5.0 migration](docs/releases/4.5.0-migration.md).
- New RFCs: **RFC-0042** (`@@fsm`; updated with §1 motivation, §6.10 RE2-compliance
  matrix, §12 status), **RFC-0046** (`@@:self`), **RFC-0047** (guard-syntax survey
  placeholder), **RFC-0048** (self-describing argument marshalling), **RFC-0049**
  (exception philosophy — errors vs. queries, cross-language fallback), and
  **RFC-0050** (Frame statement syntax). The language reference gains an
  **Exception Policy** section.

### CI

- **PR matrix-smoke now compile-gates the typed targets C++, C#, and Go (#60).**
  Text snapshots are blind to "looks plausible but doesn't compile/run" codegen
  bugs on typed targets (cf. #58/#59), and those don't always share a failure
  mode with the existing Rust/Java representatives. They were previously gated
  only by the nightly full matrix; now they run on every PR alongside
  Python/Rust/Java/Erlang.
- **The `fsm_typescript` type-check tests now run a real `tsc` in CI.** The harness
  skips cleanly when no genuine TypeScript is resolvable (instead of misreading an
  unrelated `tsc` npm package's non-zero exit as a compile error), and CI installs
  TypeScript so the generated `@@fsm` TypeScript is actually type-checked.
- **`dotnet` first-run is warmed once before the parallel test step**, removing a
  NuGet-migration named-mutex race that intermittently failed the `fsm_csharp`
  tests on cold runners.

## [4.4.0] - 2026-06-06

Ships RFC-0043: the `@@[async]` system-header attribute and a layered
casing/machine codegen architecture for every async-capable backend. Async
systems are now emitted as a public **casing** (user-declared name) that gates
external entry against concurrent dispatch, and a private **machine**
(`_<Name>Machine`) holding the existing async dispatch core. Hard cut from
the previous release: async members now **require** the `@@[async]` header.

### Added

- **`@@[async]` system-header attribute (RFC-0043).** Opts a system into the
  layered codegen architecture. Required for any system declaring an `async`
  interface method, action, or operation. Permitted without async members
  (a sync-dispatch system that still wants the single-driver gate).
- **Layered casing/machine emission across 11 async-capable backends.**
  Python, Rust, TypeScript, JavaScript, Java, C#, Kotlin, Swift, Dart,
  GDScript, C++. Each casing wrapper enforces a single-flight gate; the
  embedded machine carries the existing async dispatch core unchanged.
  Operations and persist save/load bypass the gate (they're
  non-dispatching). Operations honor the user's `async` declaration —
  a sync op produces a sync delegate; an `async` op produces a coroutine
  delegate that awaits the machine. Previously every method on an async
  system was coroutinized indiscriminately by `make_system_async`,
  including user-sync operations.
- **`E703` — concurrent external dispatch.** Runtime error raised when an
  external caller enters an async system while a dispatch is in flight.
  The gate is **single-driver** — at most one external dispatch in flight
  at a time. Internal self-calls (RFC-0006) bypass the gate by going
  directly to the machine. Per-backend surface (all recoverable):
  - Python: `RuntimeError("E703: …")`
  - Rust: `Err(FrameE703Error)` (D5 — replaces the original `panic!`)
  - TypeScript / JavaScript: `Error("E703: …")`
  - Java: `CompletableFuture.failedFuture(RuntimeException)`
  - C#: `InvalidOperationException("E703: …")`
  - Kotlin: `IllegalStateException("E703: …")`
  - Swift: `throws FrameE703Error` (D2 — replaces the original `fatalError`)
  - Dart: `StateError("E703: …")`
  - GDScript: `push_error(…)` + typed-zero return (D3 — replaces `assert`
    which Godot strips in `--remap` release builds)
  - C++: `std::runtime_error("E703: …")`
- **Single-driver concurrency contract (D1, documented).** The gate is a
  plain non-atomic boolean. Concurrent entry from multiple OS threads /
  dispatchers / executors is the caller's responsibility — see RFC-0043
  Drawbacks for per-language mitigation patterns (Mutex, single-thread
  executor, actor wrapper, etc.).
- **`E720` — async members require `@@[async]`** (validator, hard cut).
  An `async` interface method, action, or operation without the
  `@@[async]` system-header attribute is now an error. No warning grace
  period.
- **`E721` — sync system composes async system as domain field** (validator,
  same-file). A non-`@@[async]` system declaring a domain field whose type
  names an `@@[async]` system in the same compilation unit is now an error.
  Detection tokenizes the type text on non-identifier characters so direct
  (`f: Fetcher`), nullable (`f: Fetcher?`), and container-wrapped
  (`Vec<Fetcher>`, `Option<Fetcher>`, `List<Fetcher>`) cases all fire.
  Same-file scope only — cross-file resolution arrives with RFC-0040's
  follow-up.
- **`framec project add-async-attr <path>` codemod** + **`migrate_async_attr`
  WASM export.** Purely textual migration: inserts `@@[async]` above each
  `@@system` header whose body declares an async member. Recognizes
  `.frm`, `.frame`, and target-suffixed `.f<ext>` source files.

### Changed

- **C++ `Class` arm now exposes the current class name as `ctx.system_name`**
  for the scope of its emission. Previously the C++ emitter left
  `system_name` untouched, so the Constructor arm reused the outer scope's
  value across every class in a multi-class module — producing the wrong
  factory name when more than one class appears in a single emission. The
  fix mirrors the established Java / Kotlin / Swift pattern.
- **`<stdexcept>` added to the C++ runtime imports** for
  `std::runtime_error` used by the casing's E703 gate.
- **D2 — Swift gate is recoverable.** Casing interface methods are now
  `async throws -> T` and throw `FrameE703Error` on busy. Replaces the
  original `fatalError` so callers can `try?` / `catch`. Aligns Swift
  with every other layered backend's recoverable contract.
- **D3 — GDScript gate uses `push_error` + typed-zero.** On busy, the
  casing pushes an error and returns the typed-zero for the declared
  return type (`""` for String, `0` for int, `false` for bool, `0.0`
  for float, `null` for object). Survives Godot `--remap` release builds
  (which strip `assert`).
- **D4 — C++ matrix lane uses `-std=c++23`** for `@@[target("cpp_23")]`
  fixtures (local harness + Docker runners).
- **D5 — Rust gate is recoverable.** Casing interface methods now return
  `Result<T, FrameE703Error>`. The error type implements
  `std::error::Error` + `Display` + `Debug` and `?`-chains cleanly.
  Replaces the original `panic!`. `_GateGuard::drop` still runs on
  user-handler panic so the gate clears via RAII; `panic = "abort"`
  bypasses this (documented limitation).
- **Operation behavior — `op.is_async` is now respected.** Operations
  declared without `async` no longer get coroutinized when the system is
  `@@[async]`. A sync casing delegate calling a sync machine op returns
  a sync value (not an unawaited coroutine).
- **Casing persist save/restore signatures are typed correctly.** Fix #2:
  the casing's save/load delegates now carry the system's persist-blob
  type instead of `void` / `Object` placeholders. Affects 8 typed
  backends (Java, Kotlin, C#, Swift, C++, Dart, Rust, GDScript).
- **Java casing wraps sync interface methods in
  `CompletableFuture.completedFuture(...)`.** A non-`async` interface
  method on an `@@[async]` Java system used to emit a
  `CompletableFuture<T>` declaration with a plain `T` body — silently
  broken. The casing now splits on `is_async` and wraps the sync result
  appropriately.
- **TS/JS init() emits `[]` for empty params, not `null`** (Fix #1 /
  D-TS-1). Strict-TS rejects `null` where `any[]` is expected.
- **JS/TS finally clear order swap** (Fix #4 / D-JS-3): the casing now
  clears `in_flight = null` before `busy = false`. A microtask observer
  between the two writes sees the gate fully cleared instead of
  half-stale.
- **Dart E703 message uses nil-coalesce** (Fix #5 / D-DT-1): emits
  `${this.in_flight ?? "?"}` so a null in-flight slot doesn't render as
  the literal string `"null"`.
- **Java handler exceptions become `CompletableFuture.failedFuture`**
  (Fix #3 / D-JAVA-1). Previously a sync `RuntimeException` escaped the
  casing — diverging from every other layered backend's recoverable
  contract. Now the casing wraps and surfaces via the future's
  exceptional state. The asymmetric Java unwrap semantics
  (`.handle()`/`.exceptionally()` see the raw cause; `.get()` wraps in
  `ExecutionException`; `.join()` wraps in `CompletionException`) are
  pinned by the fixture suite.
- **`19_async_http_client` demos use a `has_log_entry` operation
  accessor** (Fix #6) instead of reaching into the casing's
  not-actually-public log field. Consistent with `@@system private`
  encapsulation across 17 demos.

### Fixed

- **Kernel context-stack must clean up on exception (RFC-0044, D-PY-1).**
  Every interface dispatch wrapper used to emit `push / __kernel / pop`
  without exception safety. If a handler raised mid-dispatch, the pop
  was skipped and the stack accumulated a stale entry per failed call.
  Now wrapped in language-idiomatic try/finally (or equivalent) on 12
  backends: Python (`try/finally`), TypeScript/JavaScript/Java/Kotlin/
  C#/C++/Dart (`try/catch + rethrow` or `try/finally`), Ruby
  (`begin/ensure`), Go (`defer`), Lua (`pcall` + re-raise). Exempt:
  C (no exceptions), GDScript (no try/catch; assert halts the script),
  Erlang (process model isolates state), Swift (machine dispatch
  signature isn't `throws`). See [RFC-0044](docs/rfcs/rfc-0044.md).

### Testing

The contract is pinned by a layered fixture suite in `framec-test-env`:

- **Cross-backend common core** — 6 patterns × 11 backends = **66
  fixtures** verifying the gate's structural invariants: exception clears
  gate (C1), cooperative concurrent E703 (C2), distinct-instance
  parallelism (C3), sync operations bypass gate (C4), persist roundtrip
  preserves gate (C5), parent–child composition (C6).
- **Per-language P1 unique** — **48 fixtures** exercising each backend's
  unique async risk surface (e.g. Kotlin's cancellation cluster, Java's
  CompletableFuture semantics, TypeScript's Promise foot-guns, JavaScript's
  unbound-`this`, Python's asyncio idioms, Swift's `TaskGroup`/`async let`/
  detached tasks, Dart's `Future.wait`/Zone propagation, GDScript's
  D3 typed-zero contract, C++'s nested `co_await` + `std::string` lifetime,
  Rust's `select!`/`timeout` cancellation, C#'s `WhenAll`/`WhenAny`/
  exception-filter).
- **RFC-0044 leak regression** — 12 backends pinning the
  `len(context_stack_after) == len(context_stack_before)` invariant.

Every fixture verified end-to-end against its real runtime.

### Migration

If your code has async systems without `@@[async]`, run the codemod once
before upgrading and the validator stays quiet:

```bash
framec project add-async-attr path/to/source-tree
```

The codemod is purely additive — it inserts the now-required attribute and
changes nothing else. Sources that already carry `@@[async]` (or that
declare no async members) generate byte-identical output to 4.3.x for those
systems.

If your code has a sync system holding an async system as a domain field,
E721 surfaces it at compile time. The two fixes are: (1) add `@@[async]`
to the holder, or (2) restructure so the async child is held by an async
parent.

If your Swift code calls casing methods directly (without `try`), update
to `try await sys.method()` and add a `do { ... } catch { ... }` block
to handle `FrameE703Error` — Swift's recoverable gate contract (D2).

If your Rust code calls casing methods, the return type is now
`Result<T, FrameE703Error>` — use `?`-chains or match. The error type
implements `std::error::Error` so `Box<dyn std::error::Error>` conversion
works.

If your GDScript code relies on E703 firing in release builds, no change
needed — D3 replaces the assert-based gate with `push_error` + typed-zero,
which survives `--remap`.

- **Graphviz: `push$ -> $X` now draws a forward edge.** The Graphviz backend
  emitted no edge into a state reached by `push$ -> $Target`, so the pushed-to
  state appeared unreachable in the diagram even though the FSM ran correctly.
  The diagram IR builder's `StackPush` arm dropped the transition target; it now
  emits a forward edge to the pushed-to state. For an HSM-inherited `push$` on a
  parent state the edge is cluster-anchored (`ltail="cluster_<Parent>"`), and a
  single-state push emits a plain forward edge. The push edge is distinguished
  by a `(push$)` label tag on a solid edge (dashed/dotted remain `->>`/`=>`).
  Graphviz-only; all other targets are byte-identical to 4.3.0.

### RFC-0045 — reserve `@@:system.state`, relocate name to `@@:system.state.name`

- **BREAKING — the current-state name accessor moves to `@@:system.state.name`
  (RFC-0045).** `@@:system.state` previously evaluated to the current state's
  name as a string; that spelling is now **reserved** for a future meaning (a
  direct reference to the current compartment), and the name accessor is
  `@@:system.state.name`. Bare `@@:system.state` is a hard error (**E608**) with
  a fix-it message. Generated output for `@@:system.state.name` is byte-identical
  on all 17 backends to what `@@:system.state` produced before. Migration is
  mechanical: `@@:system.state` → `@@:system.state.name`. (Pre-public-beta: no
  published program is affected.)
- **E608 — `@@:system.state` is reserved (RFC-0045).** Fires in handler and
  operation bodies for the bare form, pointing at `@@:system.state.name`. The
  E604 hint (bare `@@:system`) now suggests `@@:system.state.name`, and E421
  (no state access in static operations) is retargeted to the new spelling.

### RFC-0044 — context stack cleans up on a handler exception (D-PY-1)

- **The interface dispatch wrapper now pops the context stack even when a
  handler throws.** Previously the `push / __kernel / pop` sequence had no
  exception safety, so a handler that raised mid-dispatch leaked a stale
  context-stack entry per failed call — the leak RFC-0043's casing surfaced.
  Fixed across 12 backends with each language's idiom (try/finally, try/catch +
  rethrow, begin/ensure, `defer`, `pcall` + re-raise). Holds under RFC-0043's
  casing (the machine layer carries the guard; the casing delegates to it).
  C, GDScript, Swift, and Erlang are exempt — no catchable exception can
  propagate through their dispatch.

### Fixed — codegen & validator hardening (bug sweep)

- **Transition across a newline now parses (#43).** `->` followed by its
  `$State` target on the next line previously emitted the tokens as native
  text — no transition, and a spurious W414 ("state not reachable"). Both the
  unified scanner (codegen) and the lexer (AST / reachability) are now
  newline-aware. The trailing-`$` requirement is preserved, so native `-> T`
  (Rust / Erlang) is unaffected. All backends.
- **`E400` — inline control flow with transition arms is rejected (#13).**
  `if c then -> $A else -> $B end` on a single line silently miscompiled (the
  transition's implicit `return` cannot be scoped through an opaque native
  block — the Oceans Model). It is now a clear error; the multi-line
  `if/then/else` form (and brace-delimited inline blocks) work as before.
- **Ruby internal dispatch uses `__send__` (#14).** A user event named `send`
  shadowed `Object#send` and broke the router; internal dispatch now uses the
  override-proof `__send__` alias, so a user `send` event is harmless.
- **`E501` — Ruby `initialize` interface method is rejected (#15).** A
  `def initialize` event handler would silently replace the object
  constructor; rejected with a suggested rename (mirrors the GDScript
  reserved-method check). Other targets are unaffected.
- **`E609` — Rust `list`/`dict`/`set`/`tuple` pseudo-types are rejected (#37).**
  Rust passes type names through verbatim and has no mapping, so
  `domain: xs: list` emitted the invalid `pub xs: list`. Rust-scoped — these
  names remain supported on C and dynamic targets via the runtime's list/dict
  helpers.

## [4.3.0] - 2026-05-27

Re-introduces the `@@import` directive (removed in 4.2.0 by RFC-0024) in a
strictly narrower, **analysis-only** form per RFC-0040, and fixes the
composed-child persist-naming bug in both its same-file and cross-file forms.

### Added

- **`@@import "<path>"` as an analysis directive (RFC-0040).** framec reads the
  referenced Frame source *while compiling the current file* to resolve and
  check cross-file references — but emits **nothing** for it (no import line, no
  target code for the imported system). Native host imports remain the user's
  own Oceans Model pass-through. The directive changes what framec *knows*,
  never what it *writes*. Imported systems are **analysis-visible but
  emission-excluded** — present in the symbol table and the cross-system
  codegen registries, never generated into the importer's output.

### Fixed

- **Composed-child persist method names — same-file (#44).** When a parent
  composes a child via `domain: child = @@Child()` and the child renamed its
  persist ops with `@@[save(name)]` / `@@[load(name)]`, the parent's generated
  `save_state` / `restore_state` called the child by the hardcoded
  target-default name instead of the child's declared name — a `TypeError` at
  runtime, silent at compile time. The parent now resolves the child's declared
  names across all 14 backends. Also corrects a latent Go case where the
  new-contract nested-restore branch called a non-existent `LoadState`.
- **Composed-child persist method names — cross-file.** With `@@import`, the
  same resolution now works when the child lives in another file: the parent
  reads the imported source and calls the child's declared names instead of the
  target default.
- **Imported `@@[main]` no longer trips E806 (#45).** A file that `@@import`s
  another Frame source whose primary system carries `@@[main]` no longer fails
  the importer's single-`@@[main]` check — that rule is scoped to locally
  declared systems, as it should be.

### Notes

- Output for files without `@@import` is byte-identical to 4.2.x.
- Cross-file *argument/type* validation (surfacing imported-call mismatches) is
  a planned follow-up under RFC-0040; it does not yet fire.

## [4.2.4] - 2026-05-26

A maintenance release. Two user-visible codegen fixes; the bulk is internal —
framec's own parser and several scanners are now Frame state machines
(dogfooding), and the compile pipeline is FSM-driven. **Generated output is
byte-identical to 4.2.3 except for the two fixes below** — every internal
refactor was verified against the code it replaced (17-backend matrix +
full structural fuzz, both green).

### Fixed

- **`push$ -> $State` codegen (#42).** Inline push-with-transition emitted a
  call to a non-existent `_transition()` on Python, GDScript, JavaScript, and
  TypeScript, and the W414 reachability check didn't count `push$ -> $State`
  edges. It now lowers through the compartment model (`__prepareEnter` +
  `__transition`) like every other transition, and reachability counts the edge.
- **Multi-line `domain:` default literals (#41).** A dict/array default that
  spanned multiple physical lines was split at the first newline into stray
  field declarations; it is now captured whole (via the dogfooded
  `ExprScannerFsm`).

### Changed

Internal architecture — no change to generated code:

- **The parser is now a Frame state machine (RFC-0039).** A `SystemBackbone`
  Frame system owns and drives parsing, calling the recursive-descent
  `parse_*` methods as native oracles; the lexer was made lifetime-free so the
  backbone can hold it. framec's own front-end grammar is now expressed in Frame.
- **The compile pipeline is FSM-driven (RFC-0035 Round 8).** `compile_ast_based`
  was carved into phase functions sequenced by a `PipelineFsm` (one state per
  phase: segment → parse → module-gates → graphviz → validate+codegen →
  assemble); the previous observational supervisor was replaced by one that
  actually controls the flow.
- **Scanners converted to Frame FSMs (RFC-0035 Rounds 9–12):** the `domain:`
  line scanner, the assembler's `@@SystemName(args)` call-site lexer, the
  transition-string metadata parser, and the Erlang paren-balance lexical
  scanner. Several inline delimiter scans now reuse the existing dogfooded
  `ExprScannerFsm` / `AttributeScannerFsm` (#372/#373).

### Added

- **Frame Syntax Taxonomy** — a standard-compiler-terminology appendix in the
  language guide, anchored by behavioral tests, a compile-time exhaustiveness
  guard over the token + statement enums, and a horizontal-whitespace
  invariance generator.
- **RFC-0039** (parser as composed Frame state machines), Accepted; **RFC-0035**
  dogfooding roadmap Rounds 8–13; glossary "backbone" / "oracle" terms; a
  Release Notes section + style guide.

## [4.2.3] - 2026-05-24

Type-name passthrough is now total, and statically-typed targets enforce
typed parameters. Surface syntax is unchanged; the behavior change below
affects only sources that relied on the old portable-alias translation.

> (4.2.2 was tagged but never released — its commit tripped the CI
> formatting gate; 4.2.3 is the same content cut from a clean commit.)

### Changed

- **Type names pass through verbatim on every target (#37).** framec no
  longer translates a portable alias to a per-backend native type
  (`int`→`i64`, `str`→`String`, `float`→`f64`, …); it copies the type name
  you write straight into the generated code. This honors Frame's documented
  "no type system — type names pass through" contract
  (`docs/frame_language.md § Types and Expressions`); the per-backend alias
  tables were a deviation and have been removed. **Structural** transforms
  are unaffected (Rust borrow→owned widening `&str`→`String` / `&[T]`→`Vec<T>`,
  Kotlin `void`→`Unit`, Go empty return, C runtime containers
  `list`→`FrameVec*` / `dict`→`FrameDict*`).

  **Action required — statically-typed targets** (C, C++, Java, Go, Rust, C#,
  Kotlin, Swift): write your target's own type names instead of portable
  aliases — e.g. `x: i64` (Rust), `x: long` (Java), `x: std::string` (C++) —
  since framec now passes them straight through. **Dynamic targets** (Python,
  JS, TS, Ruby, Lua, PHP, Dart, GDScript) are unaffected.

### Added

- **E606 — statically-typed targets require typed parameters (#37).** On C,
  C++, Java, Go, Rust, C#, Kotlin, and Swift, an untyped event / handler /
  state / enter-exit lifecycle parameter is now a hard error with a fix-it
  message — under passthrough framec can no longer synthesize a parameter
  type. Complements the existing E605 (typed domain fields). Dynamic targets
  are unaffected.
- **Default-target warning + `FRAMEC_DEFAULT_TARGET` (#36).** Invoking framec
  with no `-l` flag and no `@@[target(...)]` pragma now warns once on stderr
  before falling back to `python_3`, and `FRAMEC_DEFAULT_TARGET` lets you pick
  the implicit target. Precedence: `-l` > `@@[target]` >
  `FRAMEC_DEFAULT_TARGET` > `python_3`.
- **wasm build / `@frame-lang/framec-wasm`.** The `framec` library now builds
  for `wasm32`; the wasm-bindgen entry point was extracted into a separate
  `framec-wasm` crate so the core `framec` crate stays a clean `rlib` + `bin`.
  README documents usage from Node.
- RFC-0036 (no-alloc dispatch) and RFC-0038 (deferred dispatch) drafts.

## [4.2.1] - 2026-05-22

Rust-target codegen fixes. No surface-syntax or wire-format changes; all
16 other backends are unaffected.

### Fixed

- **Typed lifecycle args (RFC-0025.1, #34/#35).** The Rust target carried
  `$>` / `<$` enter/exit args through a stringified `Vec<String>` +
  `parse::<T>()`, losing type fidelity (silent default on a parse miss)
  and hard-breaking on compound types (`Vec<i64>: FromStr`). Enter/exit
  args now ride the typed per-state `StateContext`, like state args — no
  stringify, no type erasure. Includes the start-state `<$` exit-handler
  binding (#35) and decorated `pop$` args (exit → source ctx, enter →
  a `match` over the restored compartment's variants).
- **`E0124` on a state named `$Empty` (#40).** The `StateContext` enum's
  synthesized no-context sentinel was hardcoded as `Empty`, colliding with
  a user state `$Empty` ("name defined multiple times"). The sentinel is
  now the reserved `__NoContext`.
- **`E0124` on a state var + same-named lifecycle/state param.** A state
  with `$.name` and `$>(name)` emitted a duplicate ctx field; the param
  now reuses the state var's field.

### Added

- **Validator E115.** State names beginning with the reserved `__` prefix
  are rejected, guaranteeing user state names never collide with
  framec-synthesized identifiers (across every backend).

## [4.2.0] - 2026-05-21

> **Migrating from 4.1.x?** Two hard-cut breaking changes and one
> wire-format break in this release. See
> [`docs/migration/4.1_to_4.2.md`](docs/migration/4.1_to_4.2.md) for
> the upgrade walk-through.

### Added — `no_std` support for the Rust target (#31, #33)

- The Rust backend no longer hardcodes `std::` paths in the runtime.
  It emits `core::any::Any`, `alloc::rc::Rc`, and
  `alloc::collections::BTreeMap`, plus an `extern crate alloc;` +
  `use alloc::{vec, format};` module preamble — so a generated system
  compiles unchanged in a `#![no_std]` + `alloc` crate (e.g. a
  bare-metal kernel), with the consumer providing only the heap
  *types* (`String`/`Vec`/`Box`). Hosted builds are unaffected (std
  re-exports `alloc` + the prelude macros).
- framec's Rust output now targets **edition 2018+** (every Cargo
  consumer is 2018/2021/2024; a crate-relative `use alloc::…` does not
  resolve under the legacy edition-2015 default of bare `rustc`).
- The map backing call-scoped `@@:data` and the `Dict` value type
  changed from `HashMap` to `BTreeMap`; iteration is now sorted by
  key (the map is only ever inserted/read, never order-iterated, so
  this is unobservable in practice).

### Fixed

- **C# / Java:** a host-language type-cast immediately before an
  inline `@@:self.method()` self-call no longer mangles the call
  (`x = (double) @@:self.m()` previously emitted
  `x = (double); this.m();`, an E824/CS1525 compile break) (#32).
- **Rust:** multiline `@@:(…)` return values no longer emit redundant
  double-parens (`Variant((expr))`), clearing
  `clippy::double_parens`; the codebase is `cargo clippy --all-targets
  -- -D warnings` clean.

### Removed — RFC-0032 `@@codegen { ... }` directive (breaking)

- **`@@codegen { ... }` is gone.** The directive's single config knob
  (`frame_event: on | off`) was redundant with the framepiler's
  auto-inference, declared at module scope while the underlying
  codegen decision is per-system, and a no-op in practice
  (`generate_frame_event_class` returned `Some(class)`
  unconditionally on every backend except Rust). Removing it shrinks
  the language surface to no behavioral cost. See
  [RFC-0032](docs/rfcs/rfc-0032.md).
- **E824 hard-cut.** A Frame source file containing `@@codegen` at
  module scope is now a parse error: `E824: @@codegen { ... } is no
  longer accepted (RFC-0032). Delete the directive — the framepiler
  auto-enables frame_event whenever a feature that requires it
  appears...`.
- **Migration.** Delete the `@@codegen { ... }` block from each
  source file. The generated code is byte-identical without it
  (the directive was a no-op). Zero fixtures in the test corpus
  used the directive (verified 2026-05-18); the only mentions
  were in framec documentation, all swept clean in the same
  release. User sources with the block: a `sed`/`perl` one-liner
  suffices — see [RFC-0032 § Migration](docs/rfcs/rfc-0032.md#migration).

### Removed — RFC-0024 `@@import` directive (breaking)

- **`@@import` is gone.** Cross-file dependencies are now expressed
  entirely in the host language's native import syntax — `from .x
  import Y` for Python, `use crate::...` for Rust, `import` for
  Java/JS/TS/Dart/Kotlin/Swift, `require_relative` for Ruby,
  `#include` for C/C++, `const X = preload(...)` for GDScript, and
  so on — written by the user as Oceans Model pass-through. framec
  emits no `@@import` lowering. Cross-file `@@SystemName()` lowers
  using only the literal name; the host language's import system
  resolves it. See [RFC-0024](docs/rfcs/rfc-0024.md), which
  supersedes RFC-0022 and RFC-0022.1.
- **E823 hard-cut.** A Frame source file containing `@@import` at
  module scope is now a parse error: `E823: @@import has been
  removed. Replace with the target language's native import syntax
  outside any @@system block. See RFC-0024.`
- **`--import-mode strict` CLI flag removed.** E821 (unreadable
  import) and E822 (no system in imported file) are gone with it.
- **Migration.** Convert each `@@import "./other.f<ext>"` to the
  target's native import line. See the per-target table in
  [RFC-0024 § Migration](docs/rfcs/rfc-0024.md#migration) and the
  worked walk-through in
  [`docs/migration/4.1_to_4.2.md`](docs/migration/4.1_to_4.2.md).
  Java/C#/Go users already on native imports (per RFC-0022.1) just
  delete the no-op `@@import` line.

### Added — Cross-file composition via Oceans Model (RFC-0022 → RFC-0024 trajectory)

- **RFC-0022 shipped briefly and was superseded by RFC-0024.** The
  intermediate work (per-symbol module imports across 7 backends,
  `--import-mode strict` validators, GDScript `@@import` header
  ordering, cross-file persist contract detection via importer
  peek) all landed and was then retired. The net user-visible
  change is: cross-file composition now requires no Frame-level
  directive at all — just native imports through Oceans Model
  pass-through. The fact that this happened in one release window
  is by design — Frame is pre-1.0; hard cuts replace soft
  deprecation. RFCs preserved as historical record.

### Added — RFC-0025 quality remediation (Rust target)

- **Track A — typed errors for kernel-level invariants.** A sweep
  through the framec compiler crate converted recoverable `unwrap`s
  and `panic`s in compiler paths to either typed `CompileError`
  returns (E-coded; new E900–E999 block reserved for "internal
  invariant surfaced as recoverable error") or `.expect("invariant:
  …")` calls whose message documents the invariant. Crashes that
  used to bottom out at "called `Option::unwrap()` on a `None`
  value" now surface as actionable error messages with E-codes the
  user can grep against in
  [`docs/error_codes.md`](docs/error_codes.md).
- **Track B — typed compartment payload.** The generated Rust
  target's dispatch infrastructure (`Compartment`, `FrameContext`,
  `StateContext`, `FrameValue`) was retyped: `Box<dyn Any>` storage
  for state/enter args + return values is wrapped in a typed
  `FrameValue { Int(i64), Float(f64), Bool(bool), Str(String),
  List(Vec<FrameValue>), Dict(HashMap<String, FrameValue>) }`
  enum. Generated bodies downcast through `FrameValue` accessors
  (`as_int()`, `as_str()`, etc.) instead of raw `Box<dyn Any>`
  `downcast_ref::<T>()`. The `downcast-rs` dependency was dropped
  from `Cargo.toml` for the Rust target's generated code. User
  code reading from generated Rust state machines now sees typed
  enum variants, not untyped `Any`. See
  [RFC-0025](docs/rfcs/rfc-0025.md).

### Added — RFC-0027 in-tree snapshot tests (insta)

- **204 in-tree snapshots across all 17 backends.** Cargo's
  `cargo test` now runs `insta`-based snapshot tests against
  12 representative fixtures × 17 backends, locking the generated
  code byte-for-byte against approved baselines. A codegen change
  that wasn't intended to alter output is now a `cargo test`
  failure pre-merge, not a 5-minute-later matrix surprise. Phase
  rollout: P1 (Python only) → P2 (16 other backends) → P3 (12
  fixtures including HSM, persist, push/pop, multi-system). See
  [RFC-0027](docs/rfcs/rfc-0027.md).
- **Workflow.** `cargo test` reports diff if the generated code
  drifts from the approved baseline. `cargo insta accept` accepts
  the new output as the new baseline after intentional changes.
  The `.snap` files are checked into git; `.snap.new` files are
  gitignored and represent unaccepted drift.

### Added — Property-based tests for the codegen invariants

- **proptest scaffolding wired in.** A new `proptest`-driven test
  module exercises 8 codegen invariants against randomly-generated
  Frame source: round-trip Frame-source → generated-code →
  framec-parse-of-generated-code (where applicable per backend),
  factory-call shape, persist field-order stability, and
  transition-emission well-formedness. Initial corpus is
  Python+Rust; Erlang excluded for now pending diff-harness work.
  See `framec/tests/proptest/` and the
  `_scratch/proptest_invariants.md` design doc.

### Added — Post-release process (RFC-0031, ci/)

- **Three CI workflows landed** to prevent the "RFC ships, fuzz
  rots silently for months" pattern that surfaced at RC time.
  - **`.github/workflows/fuzz-smoke.yml`** — pre-merge fuzz gate.
    4-backend × all-phase smoke (Python + Rust + Erlang restricted
    + one typed lang per family). Runs on every PR; blocks
    merge if fuzz fails.
  - **`.github/workflows/nightly.yml`** — Layer 4 drift detection.
    Full 17-backend matrix + full fuzz suite. Runs once nightly;
    issues filed on regression.
  - **`.github/workflows/quarterly-audit.yml`** — recurring
    roadmap-staleness reminder. Files an issue if a roadmap task
    has been Open for more than 90 days with no commit activity.
- See [RFC-0031](docs/rfcs/rfc-0031.md) for the full release
  process model (RC validation → CI gates → drift detection).

### Added — Forward-looking design RFCs

- **[RFC-0028](docs/rfcs/rfc-0028.md)** — in-process framec API.
  Forward-looking scoping document for an eventual library
  interface to framec (today: CLI-subprocess only). Captures the
  three caller classes (test harness, IDE LSP-style integration,
  hosted compile services), the threading model, and the
  Cargo-feature-gating proposal. No execution commitment.
- **[RFC-0029](docs/rfcs/rfc-0029.md)** — fuzz infrastructure
  status report + deferred-work catalog. Documents the current
  state of the fuzz harness, what works, what's missing, and the
  RFC-0012/RFC-0015/RFC-0024 contract-drift backlog the corpus
  needs to catch up to.
- **[RFC-0030](docs/rfcs/rfc-0030.md)** — fuzz infrastructure
  catch-up plan. The execution-committed companion to RFC-0029;
  multi-RFC corpus migration.

### Changed — RFC-0019 uniform `$>` / `<$` dispatch (breaking)

- **The HSM enter/exit cascade is gone.** Before RFC-0019 the kernel walked the
  state's parent chain on every `$>` (top-down) and `<$` (bottom-up), firing
  every layer's lifecycle handler. After RFC-0019, `$>` and `<$` are **ordinary
  leaf-dispatched events**: only the *current* state's `$>`/`<$` runs on
  entry/exit. An ancestor's lifecycle runs **only** if the leaf explicitly
  forwards via `=> $^` (placement in the handler body controls order). A leaf
  with no `$>`/`<$` and no forward silently *overrides* its ancestor's lifecycle.
  See [RFC-0019](docs/rfcs/rfc-0019.md).
- **Kernel surface deleted.** `__fire_enter_cascade` and `__fire_exit_cascade`
  are removed from every backend. `__process_transition_loop` now dispatches
  `<$` to the current leaf and `$>` to the new leaf — no chain walk. Erlang's
  gen_statem `enter` callback runs only the leaf's `$>` body; its
  `frame_exit_dispatch__` runs only `frame_exit__<leaf>`.
- **Construction-context push** (resolves RFC-0018 / F1). `_frame_init` /
  `__frame_init` now pushes a `FrameContext` for the start `$>` so the
  context-stack invariant (*every event handler runs in a context*) holds
  during construction. `@@:self.method()` inside a start `$>` no longer
  crashes on the post-call self-call guard.
- **`=> $^` inside `$>` / `<$` is now meaningful and supported on every
  backend.** In dynamic / typed backends it routes the lifecycle event to the
  parent's compartment dispatcher synchronously. In Erlang, `=> $^` in a `$>`
  body lowers to `frame_enter__<P>(Data)` and in a `<$` body to
  `frame_exit__<P>(Data)`, threaded through `Data`. Documented residual: a
  transition inside an ancestor's `$>` reached via `=> $^` on Erlang doesn't
  fire (`state_timeout` only works in the leaf's own `enter` clause).
- **Migration.** The cascade-asserting matrix HSM fixtures per backend
  (`40_hsm_parent_state_vars`, `42_hsm_three_levels`, `46_hsm_enter_parent_only`,
  `47_hsm_enter_both`, `48_hsm_exit_handlers`, `51_hsm_persist`) gained explicit
  `=> $^` forwards to keep their ancestor-lifecycle assertions green. User
  code with HSM cascades needs the same treatment: walk each substate, decide
  whether the parent's lifecycle should still run, and add `=> $^` if so.
  Matrix is 17/17 clean post-migration.

### Added — RFC-0016.1 `@@[no_persist]` honored end-to-end

- **`@@[no_persist]` now works on every backend.** The per-field opt-out
  attribute was parsed and validated (E801) since RFC-0012's persist-stress
  wave, but **no backend's codegen actually skipped the field** — it
  round-tripped just like every other domain field. As of this release, all
  17 backends honor it: the generated `save` body omits the tagged field; the
  `load` body leaves it at its `domain:` default (the value the constructor /
  `@@!Foo()` no-init allocation sets it to). New matrix fixture
  `100_no_persist_field.f*` covers every backend.
- **Python pickle → JSON migration** (deferred from RFC-0012). Python persist
  is now field-by-field UTF-8 JSON (the same wire shape the other dynamic
  backends already use), not whole-object pickle.
- **GDScript: native fidelity preserved (Godot binary Variant).** A brief
  JSON-for-all migration shipped on the morning of 2026-05-13 was reverted
  the same day after a user-reported fidelity bug: Godot's
  `JSON.parse_string` returns every JSON number as `float`, so a persisted
  `int`-typed domain field or list element came back as `float`, and
  `Array.has(typed_int)` after restore returned false even when the value
  was present. The fix is to keep GDScript on `var_to_bytes` /
  `bytes_to_var` — Godot's native binary Variant format, which round-trips
  every Variant type (int / float / string / array / dictionary /
  boolean / null) exactly. Wire-format **shape** still matches every
  other backend (a `PackedByteArray`); the **encoding** inside is Godot
  binary, not JSON. New matrix fixture `101_persist_int_fidelity.fgd`
  locks the regression. GDScript matrix 283/283 clean post-revert.
- **Lua: native fidelity preserved (serpent textual table-literal).** Lua
  has the same class of bug as GDScript: lua-cjson decodes every JSON
  number as `lua_Number` (Lua's float type), erasing the Lua 5.3+ integer
  subtype. Most user code is unaffected (Lua's `==` is numeric-equal
  across int and float) but `math.type()` queries and bitwise operations
  on persisted ints break. Lua persist now uses the **serpent** library
  ([github.com/pkulchenko/serpent](https://github.com/pkulchenko/serpent))
  — a single ~700-line pure-Lua file that dumps each value as a Lua
  table literal serpent.load can read back as the same type. Integers
  stay integers, floats stay floats, nested tables / strings / booleans
  / nil all round-trip exactly. As a side benefit, the previous
  type-aware `math.floor` int-coercion workaround in framec's Lua
  codegen (a type-ignorant boundary violation) was removed. Wire-format
  **shape** still matches every other backend (a Lua `string`); the
  **encoding** inside is a Lua table literal, not JSON. New fixture
  `101_persist_int_fidelity.flua` locks the regression. Lua matrix
  280/280 clean (1 pre-existing async skip).
- **Net wire-format inventory.** **14 backends share JSON**
  (Python, JS, TS, Ruby, PHP, Dart, Java, Kotlin, Swift, C#, Rust, Go,
  C, C++). Three documented native-fidelity exceptions:
  **Erlang** uses ETF (`term_to_binary`), **GDScript** uses Godot
  binary Variant (`var_to_bytes`), **Lua** uses serpent textual
  table-literal. All three exceptions are driven by the same pattern:
  the language has real types JSON can't represent (atoms, Variant
  int/float distinction, Lua int subtype), and forcing a lossy
  conversion silently breaks idiomatic code. A future opt-in
  `@@[persist_format(...)]` attribute (RFC pending) will give the
  14 JSON backends a typed-binary path (MessagePack / CBOR) for
  cross-language use cases that need int/float fidelity outside Frame.
- **Erlang: native fidelity preserved (Erlang External Term Format).** After
  weighing tagged-JSON marshalling against the cost of forcing Erlang
  programmers to deal with lossy round-trip for atoms, tagged tuples, and
  char-list strings, Erlang's persist wire format is `term_to_binary` /
  `binary_to_term({safe})` — the OTP-standard, zero-dep, fully-lossless
  serialization the rest of the Erlang ecosystem (mnesia, dets, ets,
  distributed Erlang) uses for the same job. Wire-format **shape** still
  matches the other 16 backends (a `binary()`); the **encoding** inside that
  binary is ETF, not JSON. Cross-language consumers who need to inspect the
  payload can use an ETF parser (one exists in every major language). The
  `@@[no_persist]` skip contract is preserved by omitting the field from
  the saved `Persisted` map, so the freshly-constructed `#data{}` on load
  picks up the record's compile-time default. Erlang matrix 275/275 clean
  (9 pre-existing framec-gap skips).
- **Wire-format breaks.**
  - **Python**: pickle blobs written by prior framec releases will not load
    (now JSON).
  - **GDScript**: the wire format is **back on `var_to_bytes`** (the
    pre-yesterday format). Pre-4.2 blobs continue to load. If anyone
    pulled framec between the morning-of-2026-05-13 JSON migration and
    the same-day revert, their JSON blobs from that window will not
    load — but the window was a few hours.
  - **Lua**: cjson JSON → serpent textual table-literal. Pre-4.2 cjson
    blobs will not load.
  - **Erlang**: persist now returns `binary()` instead of `map()`. The 3
    existing test drivers that introspected `Saved` directly
    (`23_persist_basic`, `24_persist_roundtrip`, `25_persist_stack`) were
    updated to call `binary_to_term/2` first. User code that calls
    `save_state` / `load_state` round-trip-only is unaffected.
  Persist work is still `[Unreleased]` — no released-format-compat promise
  yet.
- New spec: [RFC-0016.1](docs/rfcs/rfc-0016-1.md). Complementary inclusion-list
  form `@@[persist_fields([...])]` (RFC-0016) remains deferred.

### Fixed — Erlang `#` comments inside handler bodies

- A `#` (Frame comment) inside an Erlang handler body — `$>` / `<$` / a regular
  event handler — used to leak verbatim into the generated `.erl` as `# ...,`.
  Erlang uses `#` for record / map syntax, not comments, so this was a parse
  error (no fixture exercised it before today). The body processor now
  translates Frame `#` comments to Erlang `%` in its pre-pass, distinguishing
  comment-`#` from `Var#rec{...}` / `Var#rec.field` / `#{...}` / `Map#{...}`
  by neighbour chars. `erlang_smart_join` already drops `%` comment-only lines,
  so handler-body comments don't appear in the output — they just no longer
  break it.

### Changed — RFC-0017 init decoupling (breaking)

- Every system class is now emitted as three artifacts instead of one: a **bare constructor** (framework setup only — state stack, compartment placeholder, domain defaults, no user `$>`), a **`__frame_init(args)` method** (runs the user `$>` body, fires the enter cascade, drains the transition loop), and a **`__create(args)` factory** (bare ctor + `__frame_init` + return). Per-backend spellings: typed backends use `__create` / `__frame_init`; Python/Dart/JS/TS/Ruby/Lua/PHP/GDScript use `_create` / `_frame_init`; Go uses `CreateCounter` / `NewCounter`; C uses `Counter_create` / `Counter_new` / `Counter_frame_init`; Erlang uses `create/N` / `start_link/0` / `frame_init/(N+1)`.
- `@@Counter(7)` now lowers to `Counter.__create(7)` (or per-backend equivalent — see the [RFC-0017 mapping table](docs/rfcs/rfc-0017.md#generated-calls-per-backend)). `@@!Counter()` lowers to the bare constructor in every backend (Erlang: `element(2, counter:start_link())`). D4 invariant from RFC-0015 preserved: `$>` runs exactly once on the factory path, never on `@@!`. Verified end-to-end across all 17 backends — the differential matrix is 17/17 (~4,800 fixture×backend executions, 0 failed).
- **`const` domain fields seeded from a system param** (`const x: int = x`): the assignment moves to the constructor body / `__frame_init` (where the param is in scope). C++ keeps `const T` and seeds it via the member-initializer list — so on C++ the bare ctor takes the system params, threaded through `__create`. On Kotlin/Swift, where a `val`/`let` can't be assigned outside the constructor, the field is emitted as mutable at the target-language level (the Frame-level `const` is still enforced by the validator, E814+); Swift additionally seeds it with the type's zero value so the designated `init()` satisfies definite-initialization.
- C++ `__create` returns `T` **by value** (so driver call sites stay value-semantics: `auto c = @@Counter(7); c.method()`). System-typed *domain fields* are `shared_ptr<T>`; their initializer wraps the factory result — `std::make_shared<Counter>(Counter::__create(7))` — move-constructed from the returned temporary.
- **Removed:** `__skipInitialEnter` static flag (Java/Kotlin/Swift/Dart/GDScript/C++), `kotlin_type_default_expr` / `swift_type_default_expr` helper functions, all `__no_init` / `_no_init` / `Foo_alloc` / `'__no_init'/0` D7 synthesized helpers (8 backends), and 5 legacy `__skipInitialEnter` branches in `interface_gen.rs` restore-state codegen (dead since E814 hard-cut). (Swift's `emit_field` does have a small inline type-default `match` for stripped-initializer fields — that's the const-from-param handling above, a different mechanism than the removed helper.)
- **Erlang specifics:** `callback_mode/0` now returns `[state_functions, state_enter]`; `init([])` always sets `frame_skip_enter__ = true`; the `frame_init` `gen_statem:call` handler clears the flag and uses `{repeat_state, Data1, [{reply, From, ok}]}` (not `next_state`) so `state_enter` re-fires on the same state. The differential test harness now constructs systems via `Mod:create/N` (the factory), not the no-init `start_link/N`.
- **Migration.** Frame source is unchanged. Host code that called the generated constructor *with arguments* directly (`new Counter(7)`, `Counter(7)`, `Counter::new(7)`, `counter:start_link(7)`) must switch to the explicit factory (`Counter.__create(7)`, `Counter._create(7)`, `Counter::__create(7)`, `counter:create(7)`). Bare zero-arg constructor calls still compile but now produce a no-init instance — equivalent to `@@!Counter()`. `scripts/migrate_rfc0017_fixtures.py` does the mechanical rewrite for driver code (it ported the ~1,830-file test corpus).
- See [RFC-0017](docs/rfcs/rfc-0017.md) for the full mapping table, rationale, and rejected alternatives.

### Changed — type-ignorant codegen

- framec emits a domain field's *declared* type, spelled the target way, and lets the target's own tooling do the (de)serialization — no per-type `match` in the codegen. The **C `@@[persist]` domain-var path** moved to the symbol-mangled dispatcher: framec emits `<sys>_persist_pack_field_<mangled>((void*)&self->x)` / `<sys>_persist_unpack_field_<mangled>(json, (void*)&self->x)`; the runtime owns the cJSON typing (matching the state/enter-arg path). The now-dead `is_int_type` / `is_float_type` / `is_bool_type` / `is_string_type` predicates were removed.
- `@@:return` typed-read (`context_return_read_typed`) is type-ignorant on 16 backends — C++/Go/Swift/Kotlin/C# downcast to the spelled declared type uniformly for any `T`; Java keeps a primitive-vs-reference branch the JVM forces; C keeps a `void*`-ABI category branch (`double` bit-pun / string pointer / integer width). Rust handles `int`/`float`/`bool`/`str`; a user-declared-struct return still falls back to the raw `Option<&Box<dyn Any>>`.
- New: [`docs/contributing/type-ignorant-codegen.md`](docs/contributing/type-ignorant-codegen.md) — the architectural boundary (the three legitimate per-type branchings: type spelling, definite-init defaults, type-erased-storage downcasts; everything else is forbidden), linked from `adding-a-backend.md` and the architecture guide.

### Changed — documentation

- New [`docs/glossary.md`](docs/glossary.md) (every non-standard Frame term and symbol, deep-linkable, cross-referenced to the language/runtime docs and the defining RFC) and [`docs/rfcs/STYLE.md`](docs/rfcs/STYLE.md) (RFC style guide, grounded in the Rust RFC process / IETF RFC 2119+7322 / PEP 1+12). `rfc-0015.md` (Factory-Only System Construction) and `rfc-0016.md` (Selective Domain Persist — draft, deferred) rewritten against them: internal phase/wave/decision-code noise stripped, `Blank` → "no-initialization" throughout, validator tables verified against the implementation. Terminology aligned in `rfc-0017.md` and `frame_language.md`.

## [4.1.1] - 2026-05-09

### Changed

- **Repository moved to `frame-lang/framec`** (new GitHub org + rename). The previous canonical location, `frame-lang-old/framepiler`, was renamed to `frame-lang-old/framec` then transferred to `frame-lang/framec` in the new org. GitHub serves auto-redirects from both prior URLs.
- Cargo.toml `repository`, `homepage`, and `documentation` URLs updated to the new location. (Crate metadata for `4.1.0` retains the old `frame-lang/framepiler` URLs — fixed forward in this patch release.)
- README CI badge URL updated to the new repository.
- `.github/CODEOWNERS` owner handle migrated to `@cogiton`.
- Doc + contribution-guide URLs swept to the new repository (CONTRIBUTING, SECURITY, getting-started, adding-a-backend).

No code changes. Pure metadata + URL hygiene release.

## [4.1.0] - 2026-05-08

Headline of 4.1.0: **RFC-0015 — factory-only construction with system-level lifecycle attributes**, the new persist contract that supersedes RFC-0012's operation-attribute form. Hard-cut at this release; legacy form rejected by **E819**. Backed by three-attribute lifecycle (`@@[create]` / `@@[save]` / `@@[load]`), the `scripts/migrate_rfc0015.py` codemod (multi-system + visibility-aware), and end-to-end coverage on all 17 backends.

This release also closes the last gaps from the RFC-0014 `@@[main]` wave 1, the RFC-0013 annotation syntax, and the persist wave 8 closure (nested `@@SystemName` on every backend; E700 quiescent contract).

### Added — RFC-0015 factory-only construction (system-level lifecycle attributes)

- **Three-attribute lifecycle contract** at the system level: `@@[create(<name>)]`, `@@[save(<name>)]`, `@@[load(<name>)]`. Names default to `create_<system>` / `save_<system>` / `load_<system>` when unspecified. Generated factory and persist methods adopt the user-named identifiers across all 17 backends.
- **E815** — lifecycle attribute attached to a non-system attachment position (must be system-level, not operation-level).
- **E817** — invalid lifecycle attribute name (non-identifier).
- **E818** — duplicate `@@[save]` or `@@[load]` on the same system (only one of each per system).
- **E819 — hard-cut.** RFC-0012's op-attribute persist form (`save: () { @@[save] ... }`) is rejected at validation time with a one-line migration message pointing at the RFC-0015 system-level form.
- **`scripts/migrate_rfc0015.py` codemod** — multi-system aware (handles `@@system Inner(seed: int) { … }` headers with parameter lists and superclasses) and visibility-modifier aware (`@@system private`, `public`, `internal`). Migrates RFC-0012 op-attribute fixtures to the new contract; full corpus pass touched 4,734 fixtures.
- **D3 — C cross-system method call rewrite.** Post-pass in the C backend rewrites `self.field.method(args)` to `<Sys>_method(self->field[, args])` for domain fields whose `type_annotation` matches a defined system. Closes the long-standing C cross-system call gap (analogous to the existing Erlang post-pass).
- **D4 — leading-`_` C action symbols** preserved verbatim in `func_name` and given a matching `static` keyword in the forward declaration.
- **D5 — Erlang `@@:(<expr>)` in action bodies** now lowered via `expand_system_state_in_code`. Multi-line case/if-block trailing expressions bind to `__ActionRetVal__` and return `{Data, __ActionRetVal__}`. Leading-`_` user names quoted as `'_name'` atoms across action and operation call sites.
- **Rust default load param type** for the new persist contract is `String` (not `&str`), so `save_<sys>() -> String` and `load_<sys>(s: String)` round-trip cleanly through the common case.
- **Phase 5 fuzz coverage** — `gen_persist_multisys.py` (P1 simple_nested, P2 parameterized_inner — Issue #2 reproducer at scale, P3 chained 3-level) across 16 backends; `gen_async_persist.py` Python canary; negative-case fixtures for E815/E817/E818/E819.
- **Cookbook + per-language guides + spec** all migrated to the system-level form. **Recipe 111 added**: "Init Logic in `$>` — Where Setup Code Lives" (clarifies the canonical home for one-time setup vs. recurring transitions).

### Added — RFC-0016 selective domain persist (deferred design)

- `@@[persist_fields([...])]` form documented for use cases that need a subset of domain fields persisted. Explicitly deferred from 4.0/4.1; tracked as RFC-0016 for a future release.

### Added — W705 transition return-type default warning

- Validator warns on `-> $State` transitions in event handlers whose declared return type's default value might silently leak. Strict `return -> $State` form remains the supported pattern; the warning helps catch the loose form during migration.

### Fixed — multi-line `@@:()` return expressions on indent-sensitive targets

- Multi-line expressions inside `@@:(<expr>)`, `@@:return = <expr>`, and `@@:return(<expr>)` are now re-wrapped in `(...)` when the expanded RHS contains a newline. Without the wrap, GDScript and Python parsed the assignment up to the first newline and rejected the continuation as an `Indent` parse error. Curly-brace targets receive redundant-but-harmless parens. Matrix fixture `92_return_expr_multiline.{fpy,fgd}` covers the regression. Surfaced by `frame-arcade/ch05-pacman`.

### Fixed — additional codegen + runtime fixes

- **Erlang multi-line `@@:(value)`** no longer joins lines with stray commas in the emitted record-update.
- **Rust `rewrite_arg_if_non_copy_field`** byte-slice OOB panic — defensive bounds check on the arg-rewrite walk.
- **Two FRAMEC_BUGS hot-fixes** (Issues #1 and #2 from `frame-arcade/FRAMEC_BUGS.md`) closed end-to-end at the codegen layer, with verification trace in the bug report.

### Added — RFC-0014 `@@[main]` (wave 1)

- Multi-line expressions inside `@@:(<expr>)`, `@@:return = <expr>`, and `@@:return(<expr>)` are now re-wrapped in `(...)` when the expanded RHS contains a newline. Without the wrap, GDScript and Python parsed the assignment up to the first newline and rejected the continuation as an `Indent` parse error. Curly-brace targets receive redundant-but-harmless parens. Matrix fixture `92_return_expr_multiline.{fpy,fgd}` covers the regression. Surfaced by `frame-arcade/ch05-pacman`.

### Added — RFC-0014 `@@[main]` (wave 1)

- **`@@[main]` system attribute** to mark the file's primary system in multi-system `.fgd` files. The primary owns the script-module slot in targets that privilege one class per file (GDScript today; Java/C#/TypeScript planned in later waves). Non-main systems wrap as inner classes (sibling-resolvable from the main system's domain initializers and from each other).
- **`SystemAst.attributes: Vec<Attribute>`** — generic system-level attribute storage parallel to RFC-0013 wave 2 phase 2's per-item attributes. RFC-0014 ships `@@[main]` as the first user; `@@[persist]` stays special-cased in `persist_attr` for backwards compatibility (a follow-up will migrate it).
- **E805** — multi-system module declares zero `@@[main]`. Hard cut at parse time with a one-line migration message.
- **E806** — multi-system module declares multiple `@@[main]`. Only one system per file may occupy the primary slot.
- **GDScript multi-system fix.** Solves the long-standing "first system silently becomes the primary" bug that broke every multi-system `.fgd` whose driver instantiated the lexically-last system (the natural authoring order: primitives first, composer last). The main system's `extends Base` directive is hoisted to the top of the file so the developer-natural source order produces a valid script.
- **Reverted** the D22 `class_name` post-pass — it didn't actually solve cross-reference resolution (Godot's inner classes can't see their own script's `class_name`) and added Godot global-namespace pollution.
- **Test corpus migrated** — 204 multi-system fixtures (88 `.fgd` and 116 across other targets) updated to mark the lexically-last system `@@[main]`, matching every test driver's instantiation pattern. New fixture `tests/common/positive/primary/91_main_attr_cross_ref.fgd` exercises the cross-reference shape end-to-end in Godot.

### Added — persist contract (wave 8 closure)

- **E700 quiescent contract for `save_state`.** Mid-event saves now error with `E700: system not quiescent` instead of producing partial / undefined snapshots. Per-backend mechanism: throw on JVM/.NET/dynamic langs/C++, panic on Rust/Go, abort on C/Swift, push_error on GDScript, implicit (gen_statem deadlock) on Erlang. Hard cut, no soft warning. Documented in `frame_runtime.md`, `rfcs/rfc-0012.md`, and all 17 per-language guides.
- **Nested `@@SystemName` persist parity across all 17 backends.** Wave 8 closure: nested-system save/restore now works on every backend including the previously-blocked C and Erlang. C uses cJSON recursive embedding; Erlang uses gen_statem process trees with `child:save_state` recursing through Pids. JVM (Java + Kotlin) gained Option A nested-system support to match the existing 12-backend rollout.
- **Erlang multi-statement handler + cross-system call** — pre-existing limitation closed: handlers that combine self-mutation with cross-system calls (e.g., `self.n = self.n + 1; self.child.bump()`) now compile correctly. Cross-system rewrite extended to match `Data1#data.field`, `Data2#data.field`, etc. (the per-statement record-update suffix that emerges in chained handlers).
- **Lua `int` domain field type coercion on persist restore** — cjson decodes JSON numbers as Lua floats by default; declared `int` fields now coerce via `math.floor()` on restore so they round-trip with the integer subtype intact (Lua 5.3+).
- **70+ new persist tests** (tests 84–88 across 14 wired backends + multi-system Erlang variants in `tests/erlang/multi/`) covering: nested HSM × persist, 3-level nested HSM × persist, numeric typing in nested persist, multi-instance independence, E700 quiescent error path, plus the existing 5-deep nested chain extended to C and Erlang.
- **RFC-0012 expanded** with three new sections marked deferred pending customer feedback: cycles in the persist graph (Option A E702 recommended), Python pickle → JSON migration, adversarial input threat model + E701 corrupted-snapshot proposal.
- **Python pickle security warning** — `frame_runtime.md` and the Python per-language guide now warn that `pickle.loads` on attacker-controlled input is RCE. JSON migration tracked in RFC-0012, deferred pending customer feedback.

### Added — annotation syntax (RFC-0013)

- **`@@[name]` and `@@[name(args)]` attribute grammar.** New C#/Java/Kotlin-style annotation form across the language. Wave 1 migrated `@@persist` → `@@[persist]` (and `@@[persist(library)]` for the library form); wave 2 migrated `@@target python_3` → `@@[target("python_3")]`. Both bare forms hard-cut: bare `@@persist` errors with **E803**, bare `@@target` errors with **E804**. Test corpus and docs migrated repo-wide (~4,800 fixtures + ~30 doc samples).
- **Per-item `@@[target("lang")]`** attached to interface methods, handlers, and domain fields. Emits the item only when compiling for the named target — useful for mixed-target docs, polyglot demos, or scaffolding language-specific shim methods. Codegen filter pass (`filter_by_target_attribute`) prunes unmatched items just before emit.
- **Validator codes**: **E800** (unknown attribute name), **E801** (attribute attached at wrong attachment position — currently fires for `@@[persist]` outside system declarations), **E802** (invalid `target` argument: missing arg or unsupported language). Filter pass runs after validation so attribute-shape errors surface even on items the filter would prune.
- **Tests 89 + 90** added: per-item conditional emit on interface methods (test 89, Python + JS) and on domain fields (test 90, Python + JS). Domain-field attribute parsing supports both same-line (`@@[target("python_3")] field: int = 0`) and own-line forms.

### Added — async (carried from prior session)

- Async codegen for six new targets: Dart (`Future<T> foo() async`), GDScript (bare `await`), Kotlin (`suspend fun`), Swift (`func … async`; async entry renamed to `initAsync` since `init` is reserved), C# (`async Task<T>`), Java (`CompletableFuture<T>` on the public interface only — internal dispatch stays sync; callers `.get()`), and C++23 (`FrameTask<T>` coroutine promise emitted header-guarded at file scope). Total: 11 of 17 targets now produce real async code.
- C backend double-return marshalling — per-system `Sys_pack_double` / `Sys_unpack_double` `memcpy`-based helpers for handlers declared `float` / `double`. The `void*` return slot previously truncated fractional parts via `intptr_t`.
- C backend pointer-type parameter support — `fmt_unpack` and `fmt_bind_param` now pass types ending in `*` through as-is instead of defaulting to `int`.
- Erlang `@@:self.method(args)` full semantics — Data threading (via the existing classifier) plus transition-guard `case ...#data.frame_current_state of` wrappers around each dispatch site, so a state change inside the called handler short-circuits the rest of the caller's body.
- GDScript `@@Foo()` in domain initializers now emits `Foo.new()` (was `Foo()`, parsed as a function call on a null instance).
- Dart persist codegen updated to match the post–HashMap→Vec compartment shape (state_args / enter_args / exit_args as `List<dynamic>`, not `Map`); `_restore()` constructor initializes `late` fields; domain-field restore uses `.cast<X>()` for typed containers.
- Pop enter-args (`-> ($.items) pop$`) now routes each arg through `expand_expression` with the handler's context, so Frame sigils (`$.items`, `self.field`, `@@:params.name`) resolve to their language-specific accessors.
- C++ target pinned to C++23 (`cpp_23` alias added; `cpp` / `cpp_17` / `cpp_20` still resolve to the same backend but generated coroutine code needs `-std=c++20`+).
- Cookbook recipes #53 Byte Scanner and #54 Pushdown Parser — composed scanner + parser pipeline demonstrating Frame's `@@:self` for delimiter replay and `push$` / `pop$` as a call stack.

### Fixed

- Erlang RecordUpdate codegen strips trailing `,` / `.` from the update value — `self.count = self.count + 1,` (with Erlang statement separator attached) was emitting `Data#data{count = ... ,}` with a trailing comma inside the record-update braces, a parse error.
- Docker harness: `lua_batch.sh` now prefers `lua5.4` (Ubuntu's `lua` is 5.1 and rejects `::label::`/`goto`); `lua-cjson` installs for 5.4; `TestRunner.cs` moved `Console.SetOut` before `Task.Run` to close a race that leaked phantom TAP lines.
- Kotlin test image pulls `kotlinx-coroutines-core-jvm.jar`.

### Changed

- Integration matrix: **17 / 17 clean, 3,377 passed / 0 failed / 29 skipped** — all 29 skips are legitimate language-incompat with clear inline comments. Down from 71 skips at the start of the 2026-04-20 session (42 framec-gap tests burned down). Ten languages at zero skips.
- Unit tests: 244 → 370.
- Repo housekeeping: `CLAUDE.md` removed from version control (project-internal AI agent context, kept local-only via `.gitignore`).
- Author / project email migrated to `mark@frame-lang.org`.

## [4.0.0] - 2026-04-05

### Added

- Frame V4 transpiler with the Oceans Model — native code passes through unchanged, `@@system` blocks expand into full state machine implementations
- 9 core language backends: Python, TypeScript, JavaScript, C, C++, C#, Java, Rust, Go
- 8 experimental backends: Kotlin, Swift, PHP, Ruby, Lua, Erlang, Dart, GDScript
- GraphViz DOT output for state chart visualization
- Hierarchical state machine (HSM) support with explicit parent forwarding
- Async/await support for Python, TypeScript, and Rust
- State persistence with `@@persist` annotation
- System context (`@@`) for interface parameter access, return values, and call-scoped data
- State variables (`$.varName`) with per-state scope
- State stack operations (`push$` / `pop$`) for history transitions
- Multi-system file support
- Project-level compilation with `compile-project` command
- WASM compilation target for browser-based transpilation
- Comprehensive validation with 40+ error codes
