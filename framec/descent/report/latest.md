# The Descent — battery report

_Generated 2026-07-12 10:01 · `Darwin arm64` · framec `framec 4.6.0.25`_

**Correctness gates. Cost is data, never a veto** (RFC-0056.1 D4) — production uses the most efficient correct candidate by default.

> Read `copy-growth` before `time-growth`. Wall time can hide super-linear *work* behind a fast `memcpy`; bytes-copied cannot.


## TASK: `skip_string`

| candidate | n | ns/el | time-growth | bytes copied | copy-growth | agrees |
|---|---:|---:|---:|---:|---:|:--:|
| `incumbent(native)` | 2000 | 0.9 | — | 0 | — | ✅ |
| `incumbent(native)` | 4000 | 0.9 | 2.00x | 0 | — | ✅ |
| `incumbent(native)` | 8000 | 0.9 | 2.04x | 0 | — | ✅ |
| `incumbent(native)` | 16000 | 0.8 | 1.85x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 2000 | 11.0 | — | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 4000 | 11.2 | 2.03x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 8000 | 11.7 | 2.09x | 0 | — | ✅ |
| `@@fsm(borrowed+positioned)` | 16000 | 11.1 | 1.90x | 0 | — | ✅ |
| `@@system(owns input)` | 2000 | 173.1 | — | 4,000,000 | — | ✅ |
| `@@system(owns input)` | 4000 | 191.8 | 2.22x | 16,000,000 | 4.00x | ✅ |
| `@@system(owns input)` | 8000 | 269.7 | 2.81x | 64,000,000 | 4.00x | ✅ |
| `@@system(owns input)` | 16000 | 483.7 | 3.59x | 256,000,000 | 4.00x | ✅ |
