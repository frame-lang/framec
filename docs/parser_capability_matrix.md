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
| `parse_one_param` | `name:type=default` split | None **✗** TargetAware | None **✗** Dyck1 | none | **OPEN B2 (#249)** |
| `params_split` | state/handler params | None **✗** TargetAware | None **✗** Dyck1 | none | **OPEN B1 (#249)** |
| `param_names` | state/handler param names | None **✗** TargetAware | None **✗** Dyck1 | none | **OPEN B1 (#249)** |
| `args_of` | transition args (no-split) | TargetAware ✓ | Dyck1 ✓ | policy (no-split) | OK (deliberate) |
| `read_name_params_brace` | header skip-to-`{` | None **✗** TargetAware | n/a | policy | **OPEN B5 (#249)** |
| `paren_balance` | header `()` balance | DoubleQuote **✗** TargetAware | Dyck1 ✓ | none | **CARRIED #219** |
| `decl_read` | decl `name:type=init` | None **✗?** TargetAware | Dyck1 ✓ | none | **REVIEW** (needs a repro) |
| `body_walk` | statement extents | TargetAware ✓ | Dyck1 ✓ | none | OK (B7 = within-class) |
| `opaque_scan` | per-target string/comment | TargetAware ✓ | Dyck1 ✓ | none | OK (form gaps below) |
| `string_scan` / `string_counter` | `"`-string | DoubleQuote ✓ | — | none | OK (scoped primitive) |
| `delim_balance` | kind-checked balance | TargetAware ✓ | Dyck1 ✓ | policy | OK |
| `native_parts_scan` | island dispatch | TargetAware ✓ | — | policy | OK |
| `decl_walk` / `machine_walk` / `state_walk` / `section_scan` / `state_head_scan` / `handler_head_scan` | section/member/head boundaries | TargetAware ✓ | Dyck1 ✓ | none/policy | OK |
| `segmenter` | top-level `@@`-item boundaries | TargetAware ✓ | — | none | OK (form gaps below) |
| `ref_scan` / `inst_scan` / `embed_scan` | Frame-syntax recognizers | (caller opacity) | — | — | OK (opacity handled by `native_parts`) |

## What the matrix derived — the class-deficiency bugs

Read straight off the `✗` cells, **without being told them**:

- **B1** — `params_split` / `param_names` are `None`/`None`: a naive `.split(',')` with no nesting
  guard at all (worse than the legacy `parse_type`, which at least counted `<>`). `$B(m: Map<K,V>)`
  → phantom params, javac-rejected. **Fix: route through the `Dyck1 ∘ TargetAware` machine** — the
  same `param_scan`/`arg_scan` the F5 work built (#249 "unify the split sites").
- **B2** — `parse_one_param` is `None`/`None`: `split_once('=')` then `split_once(':')`, so a `=`
  inside `<Item = u8>` truncates the type. Same fix class.
- **B5** — `read_name_params_brace`'s skip-to-`{` is `None` opacity: a `{` in a header comment/string
  mis-bounds the system. **Fix: compose OpaqueScan.**
- **#219** — `param_scan` + `paren_balance` are `DoubleQuote` where the construct demands
  `TargetAware`: a `)`/`,` in a `'…'` char default miscounts (carried, accepted).

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

## Open review item

`decl_read` reports `None` opacity while its construct (`name: type = init`, where type/init are
native) plausibly demands `TargetAware`. It is flagged `Review` pending a concrete repro (a string
or comment carrying a `:`/`=`/newline inside a decl type/init). Confirm → open a #249-class bug and
move it to `Open`; refute → move it to `Ok` with the reason.
