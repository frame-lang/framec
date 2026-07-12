# The Descent — battery report

_Generated 2026-07-12 10:07 · `Darwin arm64` · framec `framec 4.6.0.25`_

**Correctness gates. Cost is data, never a veto** (RFC-0056.1 D4) — production uses the most efficient correct candidate by default.

> Read `copy-growth` before `time-growth`. Wall time can hide super-linear *work* behind a fast `memcpy`; bytes-copied cannot.


## TASK: `balanced_expr`

| candidate | n | ns/el | time-growth | bytes copied | copy-growth | agrees |
|---|---:|---:|---:|---:|---:|:--:|
| `incumbent(native)` | 4000 | 9.2 | — | 0 | — | ✅ |
| `incumbent(native)` | 8000 | 8.6 | 1.87x | 0 | — | ✅ |
| `incumbent(native)` | 16000 | 8.2 | 1.91x | 0 | — | ✅ |
| `incumbent(native)` | 32000 | 8.1 | 1.97x | 0 | — | ✅ |
| `@@fsm(unbounded counter)` | 4000 | 232.2 | — | 0 | — | ✅ |
| `@@fsm(unbounded counter)` | 8000 | 232.3 | 2.00x | 0 | — | ✅ |
| `@@fsm(unbounded counter)` | 16000 | 224.9 | 1.93x | 0 | — | ✅ |
| `@@fsm(unbounded counter)` | 32000 | 224.5 | 2.00x | 0 | — | ✅ |
| `@@fsm(bounded int(0..3))` | 4000 | 204.6 | — | 0 | — | ❌ |
| `@@fsm(bounded int(0..3))` | 8000 | 215.7 | 2.11x | 0 | — | ❌ |
| `@@fsm(bounded int(0..3))` | 16000 | 216.8 | 2.01x | 0 | — | ❌ |
| `@@fsm(bounded int(0..3))` | 32000 | 214.1 | 1.98x | 0 | — | ❌ |


## TASK: `skip_string`

| candidate | n | ns/el | time-growth | bytes copied | copy-growth | agrees |
|---|---:|---:|---:|---:|---:|:--:|
| `incumbent(native)` | 2000 | 0.8 | — | 0 | — | ✅ |
| `incumbent(native)` | 4000 | 0.9 | 2.10x | 0 | — | ✅ |
| `incumbent(native)` | 8000 | 0.8 | 1.87x | 0 | — | ✅ |
| `incumbent(native)` | 16000 | 0.8 | 2.01x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 2000 | 11.0 | — | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 4000 | 11.3 | 2.05x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 8000 | 11.8 | 2.09x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 16000 | 11.7 | 1.99x | 0 | — | ✅ |
| `@@system(owns input)` | 2000 | 169.2 | — | 4,000,000 | — | ✅ |
| `@@system(owns input)` | 4000 | 198.5 | 2.35x | 16,000,000 | 4.00x | ✅ |
| `@@system(owns input)` | 8000 | 264.7 | 2.67x | 64,000,000 | 4.00x | ✅ |
| `@@system(owns input)` | 16000 | 418.8 | 3.16x | 256,000,000 | 4.00x | ✅ |
