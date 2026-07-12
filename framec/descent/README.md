# The Descent — a standalone battery

Implements [RFC-0056.1](../../docs/rfcs/rfc-0056-1.md).

**Question:** for each machine framec needs, what is the **most restrictive language class that
still solves it** — and what does each class *cost*?

```bash
python3 framec/descent/run.py                    # all tasks -> report/latest.{json,md}
python3 framec/descent/run.py --task skip_string
```

Standalone by design. Not wired into `cargo test` or CI: timing wants a quiet machine, and a flaky
gate is how a guard becomes an `#[ignore]`. **It is a laboratory instrument before it is a ratchet.**
The correctness axis is portable and should move to the 17-backend matrix once the task corpus is
stable.

## The rules

**Correctness gates. Cost never does.** A candidate fails only if it disagrees with the task
specification, or if it *changes the language it accepts* (RFC-0056.1 D5 — bounding a nesting
scanner's depth is not a demotion, it is a different machine). Cost is **recorded**, never a veto:
production uses the most efficient correct candidate by default.

Why measure cost at all, then? Because **cost pressure is what produced the costumes.** Authors faced
a real choice between a correct machine and an affordable one, chose affordability, put the mode in a
native local — and that *is* the string-blindness bug family. The numbers exist so the trade is made
once, in the open, with evidence.

## Read `copy-growth` before `time-growth`

The first task proves the point. `@@system`'s wall-clock growth reads 2.2x → 2.8x → 3.6x per
doubling — **ambiguous**; you could argue it linear. Its *bytes copied* grows **exactly 4.00x per
doubling** — provably quadratic, no argument available.

**Wall time can hide super-linear work behind a fast `memcpy`. Work cannot hide.** Measure work.

## Structure

```
tasks/<task>/
  *.frs        one per candidate construct (@@fsm, @@system, …), compiled by framec
  driver.rs    the task SPECIFICATION + the sweep + the measurement
report/
  latest.md    human table
  latest.json  the ratchet's data
```

A driver owns the task spec (`incumbent()`), the corpus (deterministic, deliberately hostile —
escaped quotes, unterminated strings, quotes inside comments), and the sweep. Every candidate is
judged against the **spec**, never against another candidate.

## Current findings

### `skip_string` — a positioned probe (the shape of all 15 SyntaxSkippers)

| candidate | ns/el | time-growth | bytes copied | correct |
|---|---:|---:|---|:--:|
| incumbent (native byte loop) | 0.9 | 2.0x — linear | 0 | ✅ |
| **`@@fsm`** (borrowed + positioned) | 11.1 | 2.0x — **linear** | **0** | ✅ |
| `@@system` (owns its input) | 173 → 484 | 3.6x and climbing | **n² — 4.00x/doubling** | ✅ |

**Three things are now measured rather than argued:**

1. **`@@fsm` is a correct, linear, zero-copy replacement for a hand-rolled skipper loop.** It costs
   ~12x the native constant — recorded, not disqualifying. RFC-0042.1's `over()` / `scan_at()` /
   `impl <Name>Input for &[u8]` do exactly what they claim.
2. **#209 is quantified.** A `@@system` must own its input, so a positioned probe copies the buffer
   every call: **256 MB copied to scan a 16 KB buffer.** Not a slur on the construct — a missing
   language feature (RFC-0056 P9), now with a number on it.
3. **The `@@system` candidate is a *real* machine** — its mode lives in Frame states (`$Scan`,
   `$InString`), not in a native `in_string: u8` local. It is correct. It is simply unaffordable,
   **and that is precisely the trade that produced the thirty costumes.**

## Notes / bugs hit while building this

- A **negative literal is rejected as an `@@fsm` return default**: `: int = -1` →
  `E700: unexpected byte '-' in @@fsm header`. `= 0` works. Worked around in `skip_string/efsm.frs`;
  filed.
