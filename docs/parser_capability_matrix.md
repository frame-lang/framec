# Parser capability × demand matrix

A **mechanistic** way to identify parsing bugs and the machine that fixes them. It is the standing,
checkable form of one idea: every scanning task has a *minimal correct machine* fixed by its
computational class, and a bug is a machine deployed **below** that class. The live guard is
[`compiler/tests/capability_matrix.rs`](../compiler/tests/capability_matrix.rs); this document is
its rendered view.

## The method — two axes and a demand

For any task "find the extent/split of construct X in native code," the minimal correct machine is
fixed by:

| Axis | Levels (low → high) | Meaning |
|---|---|---|
| **opacity** | `None` → `DoubleQuote` → `TargetAware` | telling code from strings/comments: byte-blind → `"`-only (StringScan) → per-target (OpaqueScan) |
| **nesting** | `None` → `Dyck1` → `PerKind` → `KindChecked` | counting delimiters for depth: a flat `.split(',')`/eol scan counts nothing → one merged `()[]{}` counter → per-kind → kind-checked closers |

The correct machine is **OpaqueScan(target) ∘ Counter/PDA(nesting)** at the level the construct
demands, plus an adjudicator (`arity` / `g_viable` / a policy) iff the delimiter is genuinely
ambiguous (`<>`). A **cell where capability < demand on either axis is a class-deficiency bug** — a
lower-class machine standing in for a higher-class problem. The fix *is* the minimal machine at the
demanded class (usually: route through the machine that already does it right).

## The matrix (capability audited from code; demand from the construct)

`✗` = capability below demand (a bug). Splitters of native-typed lists and structural seekers over
native code both **demand** `TargetAware` opacity + `Dyck1` nesting.

| Scanner | Construct | Opacity (cap → demand) | Nesting (cap → demand) | Adjudicator | Verdict |
|---|---|---|---|---|---|
| `arg_scan` | instantiation args | TargetAware ✓ | Dyck1 ✓ | arity (E407) | **OK — the reference splitter** |
| `param_scan` | system-header params | DoubleQuote **✗** TargetAware | Dyck1 ✓ | g_viable fork | **CARRIED #219** (char/lifetime) |
| `top_level_eq` | first top-level `=` (default/init sep) | DoubleQuote ✓ | Dyck1 ✓ (+angle) | none | OK (shared primitive; `"`-only scope) |
| `parse_one_param` | `name:type=default` split | DoubleQuote **✗** TargetAware | Dyck1 ✓ | none | **CARRIED #219 B2** (routed through `top_level_eq`) |
| `params_split` | state/handler params | DoubleQuote **✗** TargetAware | Dyck1 ✓ | none | **CARRIED #219 B1** (routed through `param_scan`+`parse_one_param`) |
| `param_names` | state/handler param names | DoubleQuote **✗** TargetAware | Dyck1 ✓ | none | **CARRIED #219 B1** (routed through `param_scan`+`parse_one_param`) |
| `args_of` | transition args (no-split) | TargetAware ✓ | Dyck1 ✓ | policy (no-split) | OK (deliberate) |
| `read_name_params_brace` | header skip-to-`{` | TargetAware ✓ | n/a | policy | **OK** (#249 B5 fixed: opaque-aware seek) |
| `paren_balance` | header `()` balance | DoubleQuote **✗** TargetAware | Dyck1 ✓ | none | **CARRIED #219** |
| `decl_read` | decl `name:type=init` | DoubleQuote **✗** TargetAware | Dyck1 ✓ | none | **CARRIED #219 B9** (type/init `=` via `top_level_eq`) |
| `body_walk` | statement extents | TargetAware ✓ | Dyck1 ✓ | none | OK (B7 = within-class) |
| `opaque_scan` | per-target string/comment | TargetAware ✓ | Dyck1 ✓ | none | OK (form gaps below) |
| `string_scan` / `string_counter` | `"`-string | DoubleQuote ✓ | — | none | OK (scoped primitive) |
| `delim_balance` | kind-checked balance | TargetAware ✓ | Dyck1 ✓ | policy | OK |
| `native_parts_scan` | island dispatch | TargetAware ✓ | — | policy | OK |
| `decl_walk` / `machine_walk` / `state_walk` / `section_scan` / `state_head_scan` / `handler_head_scan` | section/member/head boundaries | TargetAware ✓ | Dyck1 ✓ | none/policy | OK |
| `segmenter` | top-level `@@`-item boundaries | TargetAware ✓ | — | none | OK (form gaps below) |
| `ref_scan` / `inst_scan` / `embed_scan` | Frame-syntax recognizers | (caller opacity) | — | — | OK (opacity handled by `native_parts`) |

## What the matrix derived — the class-deficiency bugs (now RESOLVED, #249)

These fell straight off the `✗` cells. All are now fixed by **unifying the split sites onto the
correct-class machine** — one new shared primitive `top_level_eq` (a `@@[scan(u8)]` counter
automaton: Dyck-1 + digraph-guarded angle counter, `"`-opaque) plus routing through the shipped
`param_scan` / `skip_opaque`. The class-deficiency (the actual javac-rejecting nesting-blindness) is
gone; the residual `DoubleQuote < TargetAware` opacity gap is folded into the accepted **#219**
char/lifetime carry (void condition: a scan-time `Vec<Param>` carry, or a target-aware
char-vs-lifetime leaf).

- **B1** — `params_split` / `param_names` were `None`/`None` (naive `.split(',')`; `$B(m: Map<K,V>)`
  → phantom params, javac-rejected). **FIXED → CARRIED #219:** routed through `param_scan::parse_decl`
  (Dyck-1 + angle fork, `"`-opaque, target-free) + `parse_one_param`. Also fixes the scan-time
  state-param split at `machine.rs:81` for free.
- **B2** — `parse_one_param` was `None`/`None` (`split_once('=')` truncated `<Item = u8>`).
  **FIXED → CARRIED #219:** the `=` separator is now `top_level_eq::find` (first top-level `=`).
- **B5** — `read_name_params_brace`'s skip-to-`{` was `None` opacity (a `{` in a header comment/string
  mis-bounded the system). **FIXED → OK:** the seek composes `machine::skip_opaque` (OpaqueScan),
  target threaded from `read_pragma`.
- **B9** — `decl_read`'s `eq_or_end` type/init `=` find was byte-blind (the `Review` item, CONFIRMED
  by repro: `x: impl Iterator<Item = u8> = 0` truncated to `impl Iterator<Item` / `u8> = 0`).
  **FIXED → CARRIED #219:** the leaf routes through `top_level_eq::find` (`decl_read.frs` unchanged).
- **#219** — `param_scan` + `paren_balance` are `DoubleQuote` where the construct demands
  `TargetAware`: a `)`/`,` in a `'…'` char default miscounts (carried, accepted) — now the shared
  residual for the whole declaration-site family.

## What the matrix does NOT catch — and where those bugs live

The matrix is *only* the class check. Two categories are out of its scope by construction, and both
are covered in [`parser_bug_corpus.rs`](../compiler/tests/parser_bug_corpus.rs):

- **Within-class logic bugs** — the machine is the right class, one transition/guard is wrong:
  **B4** (empty `$()` emits a phantom param), **B6** (Python `"{"` false-reject), **B7** (`body_walk`
  loses a `{` to the eol extent), **B3** (multi-line init truncated at eol). These need the
  *differential oracle* (the target's real lexer) and the *axis-keyed adversarial generator*.
- **Per-target form completeness** — `TargetAware`, but a form is missing/wrong: **B8** (C `//` has
  no `\`-newline splice), Lua `--[[ ]]`, Ruby `=begin` column-0. These are `opaque_scan`'s per-target
  form table, not a class deficiency.

## How to use it

`cargo test --test capability_matrix` **ratchets**: the set of `✗` cells must equal the documented
set. Add a byte-blind splitter and it **fails** ("undocumented class-deficiency"); fix a bug without
updating the matrix and it **fails** ("stale documented deficiency"). It is the guard that catches
the *next* B1 the day it is written.

## Resolved review item (was: decl_read)

`decl_read` was flagged `Review` (`None` opacity, construct `name: type = init`). **CONFIRMED
deficient** by repro through `segment()`: a domain var `x: impl Iterator<Item = u8> = 0` truncated
the type to `impl Iterator<Item` and fabricated the init `u8> = 0` — the byte-blind `eq_or_end`
(`while != b'='`) found the `=` inside the angle pair. This is nesting-blind (worse than the
opacity-only concern the review named), the same class as B2. Opened as **B9** and fixed: the
`eq_or_end` leaf now routes through `top_level_eq::find` (Dyck-1 + digraph-guarded angle, `"`-opaque).
Row moved `Review → Carried(#219 B9)`; pinned by `t12_eq_inside_type_now_protected` (decl_read.rs)
and `fixed_b9_decl_read_eq_in_angle_type` (parser_bug_corpus.rs).
