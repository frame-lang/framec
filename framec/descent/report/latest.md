# The Descent — battery report

_Generated 2026-07-12 10:47 · `Darwin arm64` · framec `framec 4.6.1`_

**Correctness gates. Cost is data, never a veto** (RFC-0056.1 D4) — production uses the most efficient correct candidate by default.

> Read `copy-growth` before `time-growth`. Wall time can hide super-linear *work* behind a fast `memcpy`; bytes-copied cannot.


## TASK: `skip_string`

| candidate | n | ns/el | time-growth | bytes copied | copy-growth | agrees |
|---|---:|---:|---:|---:|---:|:--:|
| `incumbent(native)` | 2000 | 1.9 | — | 0 | — | ✅ |
| `incumbent(native)` | 4000 | 1.6 | 1.72x | 0 | — | ✅ |
| `incumbent(native)` | 8000 | 0.9 | 1.09x | 0 | — | ✅ |
| `incumbent(native)` | 16000 | 0.8 | 1.96x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 2000 | 23.4 | — | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 4000 | 24.7 | 2.12x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 8000 | 13.5 | 1.09x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 16000 | 11.5 | 1.70x | 0 | — | ✅ |
| `@@system(BORROWED - P9)` | 2000 | 96.6 | — | 0 | — | ✅ |
| `@@system(BORROWED - P9)` | 4000 | 101.1 | 2.09x | 0 | — | ✅ |
| `@@system(BORROWED - P9)` | 8000 | 51.4 | 1.02x | 0 | — | ✅ |
| `@@system(BORROWED - P9)` | 16000 | 50.5 | 1.96x | 0 | — | ✅ |
