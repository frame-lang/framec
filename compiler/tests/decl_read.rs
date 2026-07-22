//! **The decl-line reader, as a system, obeys its TOTALITY + span-anchor invariants — proven by
//! running.** SCAFFOLDING (white-box on the internal `read`/`member_decl_of`; conversion-internal
//! — never promoted; needs `@@[scan(u8)]`-on-`@@system`, a cleanroom-only capability today).
//!
//! `decl_read::read` + `decl_read::member_decl_of` are generated from / built around
//! `decl_read.frs`, a `@@[scan(u8)]` register TRANSDUCER (Item 3d, `_scratch/declwalk_design.md`)
//! — `$Indent → $Async → $Name → $Params → $Type → $Init → $Accept`, NO `$Reject` (the reader is
//! TOTAL; malformedness is registers: `empty_name` = ledger T7, `params_clamped` = T8). This
//! proves — by running — that `member_decl_of(bytes, &read(bytes, from, to, t), to, from)` is
//! TOTAL and span-anchored (never panics; `MemberDecl.span.start == from`) at EVERY `(from, to)`
//! window, for all four cleanroom targets, over realistic AND adversarial decl windows AND a
//! deterministic fuzz corpus (a debug-build panic-freedom witness with every `Span`/slice assert
//! live). The EXACT `MemberDecl` fields for each ledger row are pinned self-contained by the
//! directed T* tests below (T9's opaque-aware paren scan, T12's top-level `=` split, etc.).
//!
//! LEDGER ROSTER (design §6 — one test per reader-side Phase-1 row):
//!   T7  Empty-name malformed decl -> `t7_empty_name_register_fires_on_each_reaching_class`
//!       (all four reaching classes: stray punctuation, unterminated-literal fallthrough,
//!       `$.`-alone empty window, Allman-split `{` decl).
//!   T8  Unbalanced-params clamp   -> `t8_params_clamp_pinned` (register + exact clamp text;
//!       type/init never parsed).
//!   T9  Opaque-aware paren count  -> `t9_string_blind_params_pinned` (PHASE-B DIRECTED FIX:
//!       the `params_close` leaf routes through `delim_balance::balanced`, so a `)`/`(` in a
//!       string default no longer mis-closes/mis-deepens — the real close is found).
//!   T10 Async-modifier fork       -> `t10_async_modifier_fork` (modifier vs name, all shapes).
//!   T11 Empty-type-after-`:`      -> `t11_type_annotation_and_empty_type`.
//!   T12 Top-level `=` in type     -> `t12_eq_inside_type_now_protected` (FIXED #249 B9:
//!       `eq_or_end` routes through `TopLevelEq`, so a `=` inside `<…>`/`(…)`/`"…"` is not the
//!       type/init separator; residual char/lifetime opacity is the #219 carry).
//!   (T13/T14/T15 are walk-level rows — pinned in tests/decl_walk.rs; T14's read-side
//!   consequence — the empty-named `{` decl — is class 4 of the T7 test here.)

use frame_compiler::text::scan::decl_read::{member_decl_of, read};
use frame_compiler::text::scan::literals::Target;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// The standalone TOTALITY + span-anchor invariant: the `read`+`member_decl_of` chain runs (no
/// panic, in debug builds) and the produced `MemberDecl` is anchored at the caller's `from`
/// (`span.start == from`, `span.end == to`) for these exact `(from, to)` window arguments. No
/// oracle. Exact field values are pinned by the directed T* tests.
fn agree(bytes: &[u8], from: usize, to: usize, target: Target) {
    let machine = member_decl_of(bytes, &read(bytes, from, to, target), to, from);
    assert_eq!(
        machine.span.start, from,
        "decl span not anchored at from={from} (to={to}, {target:?}) on {:?}",
        String::from_utf8_lossy(bytes),
    );
    assert_eq!(
        machine.span.end, to,
        "decl span end != to={to} (from={from}, {target:?}) on {:?}",
        String::from_utf8_lossy(bytes),
    );
}

/// Every-position sweep: agree for EVERY `from` in `0..=len` and EVERY `to` in `from..=len`.
/// Exhaustive over the whole `(from, to)` rectangle, so windows starting mid-name, mid-params,
/// mid-string and ending mid-anything are covered by construction — not spot checks.
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for from in 0..=len {
            for to in from..=len {
                agree(b, from, to, target);
            }
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a DECL WINDOW — the bytes the walk hands
// the reader (`[start, eol)` for a line decl, `[start, open)` for a body-decl
// signature, `[start+2, eol)` for a `$.x` state var). Enumerated from
// `decl_read.frs` + the hand `decl_of` (machine.rs): indent, optional `async`
// modifier, name, optional `(params)`, optional `: type`, optional `= init`
// with the `@@Sys`/`@@!Sys` probe.
// ===========================================================================

/// Realistic, well-formed decl windows.
const REALISTIC: &[&str] = &[
    "go()",
    "  stop()",
    "n: int = 0",
    "cache: Cache = Cache",
    "fetch(key: String): String",
    "go(a: int, b: str): bool",
    "async fetch(key: String): String",
    "async(x)",
    "asyncFoo",
    "async",
    "$.count: int = 0",
    "sys: Inner = @@Inner(1)",
    "other = @@!Sys()",
    "greet(name: str): str ",
    "   x   ",
    "no_params: t=@@S()",
];

/// Adversarial decl windows — the long tail the reader must get right.
const ADVERSARIAL: &[&str] = &[
    // empty window
    "",
    // whitespace only
    "  \t  ",
    // stray punctuation (empty-name family, T7)
    ";",
    // an unterminated literal's quote byte fell through the walk (T4 -> T7)
    "\"never closed",
    // the Allman-split `{` decl (T14 -> T7)
    "{ x = 1 }",
    // unbalanced params (T8)
    "go(a: int",
    "go(a: int : T = 5",
    // `)` / `(` inside a string default (T9, Phase A: parity-pinned)
    "f(s: str = \")\")",
    "f(s: str = \"(\")",
    // params-only shapes
    "()",
    "(a)",
    // text directly after the params close
    "go()x",
    // `=` chains and empty-name inits
    "a=b=c",
    ": int = 0",
    "= 5",
    // the async modifier followed by nothing (modifier + empty name)
    "async  ",
    // `=` inside the type text (T12, now protected by TopLevelEq — #249 B9)
    "m: Map<k=v> = 0",
    // empty type after `:` (T11)
    "x:",
    "x: ",
    // `@@` probe edges (the hand's strict `k + 2 < to` guard, `!` handling, ws-gap)
    "x = @@",
    "y = @@!",
    "z = @@ Sys",
    "w = @@9x",
    "v =   @@Deep(1, 2)",
];

#[test]
fn realistic_windows_agree_every_position() {
    for src in REALISTIC {
        sweep_all_positions(src);
    }
}

#[test]
fn adversarial_windows_agree_every_position() {
    for src in ADVERSARIAL {
        sweep_all_positions(src);
    }
}

/// The state-var caller shape (machine.rs `state()`: `decl_of(bytes, start + 2, e, start)`) —
/// span_start ≠ from. Directed, since the rectangle sweep pins span_start = from.
#[test]
fn statevar_caller_shape_span_start_differs() {
    let src = b"$.count: int = 0";
    for t in TARGETS {
        let machine = member_decl_of(src, &read(src, 2, src.len(), t), src.len(), 0);
        assert_eq!(machine.name, "count");
        assert_eq!(machine.span.start, 0, "the span starts at the `$`, not the name");
        assert_eq!(machine.type_text.as_deref(), Some("int"));
        assert_eq!(machine.init_text.as_deref(), Some("0"));
    }
}

// ===========================================================================
// Teeth — every named register fires, non-vacuously, by RUNNING the system
// (not the oracle) over the corpus: `is_async`, `empty_name`, `params_clamped`,
// and `has_sys` for BOTH `@@Sys` and `@@!Sys`. A differential whose corpus
// never fires a register proves nothing about it (the #232 lie). SCAFFOLDING.
// ===========================================================================

#[test]
fn corpus_fires_every_register() {
    let mut is_async = 0usize;
    let mut empty_name = 0usize;
    let mut params_clamped = 0usize;
    let mut sys_plain = 0usize; // has_sys via `@@Sys`
    let mut sys_bang = 0usize; // has_sys via `@@!Sys`
    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for t in TARGETS {
            let s = read(b, 0, b.len(), t);
            if s.is_async {
                is_async += 1;
            }
            if s.empty_name {
                empty_name += 1;
            }
            if s.params_clamped {
                params_clamped += 1;
            }
            if s.has_sys {
                if b[s.sys_s.saturating_sub(1)] == b'!' {
                    sys_bang += 1;
                } else {
                    sys_plain += 1;
                }
            }
        }
    }
    assert!(is_async > 0, "the is_async register never fired");
    assert!(empty_name > 0, "the empty_name (T7) register never fired");
    assert!(params_clamped > 0, "the params_clamped (T8) register never fired");
    assert!(sys_plain > 0, "has_sys never fired for `@@Sys`");
    assert!(sys_bang > 0, "has_sys never fired for `@@!Sys`");
}

// ===========================================================================
// One directed, self-contained test per reader-side ledger row (design §6).
// KNOWN values for hand-verified windows — these survive the oracle's eventual
// retirement (they assert facts directly, not by comparison). SCAFFOLDING
// (T9 pins the Phase-B opaque-aware paren fix; T12 pins the #249 B9 top-level `=` fix).
// ===========================================================================

/// T7: the `empty_name` register fires on EACH reaching class, and the output reproduces the
/// hand shape (`name: ""` — the `public Object ;` family, carried; the register makes the
/// gloss inspectable for the future diagnostics pass).
#[test]
fn t7_empty_name_register_fires_on_each_reaching_class() {
    // (1) stray punctuation, (2) unterminated-literal fallthrough (T4), (3) `$.` alone — the
    // state-var caller hands an EMPTY window (from == to), (4) the Allman-split `{` decl (T14).
    let classes: &[(&str, usize)] = &[(";", 0), ("\"never closed", 0), ("$.", 2), ("{ x = 1 }", 0)];
    for &(src, from) in classes {
        let b = src.as_bytes();
        for t in TARGETS {
            let shape = read(b, from, b.len(), t);
            assert!(
                shape.empty_name,
                "empty_name must fire on {src:?} (from={from}) for {t:?}"
            );
            let m = member_decl_of(b, &shape, b.len(), from);
            assert_eq!(m.name, "", "the hand output shape is reproduced for {src:?}");
        }
    }
    // Non-vacuity control: a named decl does NOT fire it.
    for t in TARGETS {
        assert!(!read(b"go()", 0, 4, t).empty_name);
    }
}

/// T8: an unbalanced `(` clamps — `params_text` = the rest of the window, the cursor lands at
/// `to`, so type/init are NEVER parsed even if their syntax follows; the `params_clamped`
/// register names it.
#[test]
fn t8_params_clamp_pinned() {
    let src = b"go(a: int : T = 5";
    for t in TARGETS {
        let shape = read(src, 0, src.len(), t);
        assert!(shape.params_clamped, "the T8 register must fire for {t:?}");
        let m = member_decl_of(src, &shape, src.len(), 0);
        assert_eq!(m.name, "go");
        assert_eq!(
            m.params_text.as_deref(),
            Some("a: int : T = 5"),
            "the clamp text is the rest of the window for {t:?}"
        );
        assert_eq!(m.type_text, None, "type is never parsed after the clamp");
        assert_eq!(m.init_text, None, "init is never parsed after the clamp");
    }
    // Control: balanced params parse the trailing type.
    for t in TARGETS {
        let m = member_decl_of(
            b"go(a: int): bool",
            &read(b"go(a: int): bool", 0, 16, t),
            16,
            0,
        );
        assert_eq!(m.params_text.as_deref(), Some("a: int"));
        assert_eq!(m.type_text.as_deref(), Some("bool"));
        assert!(!read(b"go(a: int): bool", 0, 16, t).params_clamped);
    }
}

/// T9 (PHASE-A PARITY PIN — the string-blind bare counter, `machine.rs::decl_of`'s params
/// scan): a `)` inside a string default MIS-CLOSES the group (the params text stops at the
/// quote); a `(` inside one MIS-DEEPENS it into the T8 clamp. Phase 1 reproduces both,
/// byte-for-byte. Phase B routes the shared `params_close` leaf through
/// `delim_balance::balanced` and replaces this pin with directed-fix tests per target.
#[test]
fn t9_string_blind_params_pinned() {
    // Phase-B fix: a `)` inside a string default is opaque, so the paren count reaches the REAL
    // close — params carry the whole `s: str = ")"`, no clamp. (Phase A mis-closed at the string.)
    let close = b"f(s: str = \")\")";
    for t in TARGETS {
        let shape = read(close, 0, close.len(), t);
        assert!(
            !shape.params_clamped,
            "Phase-B fix for {t:?}: the real close is found, no clamp"
        );
        let m = member_decl_of(close, &shape, close.len(), 0);
        assert_eq!(m.name, "f");
        assert_eq!(
            m.params_text.as_deref(),
            Some("s: str = \")\""),
            "Phase-B fix for {t:?}: params carry the full string default"
        );
        assert_eq!(m.type_text, None);
        assert_eq!(m.init_text, None);
    }
    // Phase-B fix: a `(` inside a string default no longer mis-deepens — the real close is found,
    // no clamp. (Phase A deepened on the in-string `(` and ran into the T8 clamp.)
    let deepen = b"f(s: str = \"(\")";
    for t in TARGETS {
        let shape = read(deepen, 0, deepen.len(), t);
        assert!(
            !shape.params_clamped,
            "Phase-B fix for {t:?}: the in-string `(` is opaque, no mis-deepen, no clamp"
        );
        let m = member_decl_of(deepen, &shape, deepen.len(), 0);
        assert_eq!(m.name, "f");
        assert_eq!(
            m.params_text.as_deref(),
            Some("s: str = \"(\""),
            "Phase-B fix for {t:?}: params carry the full string default"
        );
        assert_eq!(m.type_text, None);
        assert_eq!(m.init_text, None);
    }
}

/// T10: the `$Async` fork — `async` + `' '/'\t'` is a MODIFIER (the recorded bug-fix, now a
/// named state); `async(` / `asyncFoo` / bare `async` at the window end are a NAME.
#[test]
fn t10_async_modifier_fork() {
    for t in TARGETS {
        let m = member_decl_of(
            b"async fetch(key: String): String",
            &read(b"async fetch(key: String): String", 0, 32, t),
            32,
            0,
        );
        assert!(m.is_async, "modifier for {t:?}");
        assert_eq!(m.name, "fetch", "the name is fetch, NOT async, for {t:?}");
        assert_eq!(m.params_text.as_deref(), Some("key: String"));
        assert_eq!(m.type_text.as_deref(), Some("String"));

        let m = member_decl_of(b"async(x)", &read(b"async(x)", 0, 8, t), 8, 0);
        assert!(!m.is_async, "`async(` is a NAME for {t:?}");
        assert_eq!(m.name, "async");
        assert_eq!(m.params_text.as_deref(), Some("x"));

        let m = member_decl_of(b"asyncFoo", &read(b"asyncFoo", 0, 8, t), 8, 0);
        assert!(!m.is_async);
        assert_eq!(m.name, "asyncFoo");

        let m = member_decl_of(b"async", &read(b"async", 0, 5, t), 5, 0);
        assert!(!m.is_async, "bare `async` at the window end is a NAME for {t:?}");
        assert_eq!(m.name, "async");

        // modifier followed by nothing: is_async AND empty_name both fire.
        let shape = read(b"async  ", 0, 7, t);
        assert!(shape.is_async && shape.empty_name, "modifier + empty name for {t:?}");
    }
}

/// T11: `: type` up to `=` or window end, trimmed; an empty type after `:` maps to `None`.
#[test]
fn t11_type_annotation_and_empty_type() {
    for t in TARGETS {
        let m = member_decl_of(b"x:", &read(b"x:", 0, 2, t), 2, 0);
        assert_eq!(m.type_text, None, "empty type after `:` is None for {t:?}");

        let m = member_decl_of(b"n: int = 0", &read(b"n: int = 0", 0, 10, t), 10, 0);
        assert_eq!(m.type_text.as_deref(), Some("int"));
        assert_eq!(m.init_text.as_deref(), Some("0"));
    }
}

/// T12 (FIXED, #249 B9): the type/init `=` find is no longer byte-blind — `eq_or_end` routes
/// through the `TopLevelEq` counter automaton, so a `=` INSIDE the type (here inside the angle
/// pair `<k=v>`) is NOT the separator. The whole generic is the type and the top-level `= 0` is
/// the init. (Inverted from the pre-fix pin, which asserted the truncation to `Map<k` / `v> = 0`.)
#[test]
fn t12_eq_inside_type_now_protected() {
    let src = b"m: Map<k=v> = 0";
    for t in TARGETS {
        let m = member_decl_of(src, &read(src, 0, src.len(), t), src.len(), 0);
        assert_eq!(m.name, "m");
        assert_eq!(
            m.type_text.as_deref(),
            Some("Map<k=v>"),
            "#249 (B9) fixed: the angle-interior `=` is not the separator for {t:?}"
        );
        assert_eq!(m.init_text.as_deref(), Some("0"), "the top-level `= 0` is the init for {t:?}");
        assert_eq!(m.init_system, None);
    }
}

/// The `@@Sys` / `@@!Sys` initializer probe — every hand edge: both forms, the ws gap after
/// `=`, the strict `k + 2 < to` room guard, `!` with no name, a gap after `@@` (no probe), and
/// a digit-led name (the hand takes it — alnum class).
#[test]
fn sys_probe_every_edge() {
    let cases: &[(&str, Option<&str>)] = &[
        ("sys: Inner = @@Inner(1)", Some("Inner")),
        ("other = @@!Sys()", Some("Sys")),
        ("v =   @@Deep(1, 2)", Some("Deep")),
        ("x = @@", None),
        ("y = @@!", None),
        ("z = @@ Sys", None),
        ("w = @@9x", Some("9x")),
        ("cache: Cache = Cache", None),
    ];
    for &(src, want) in cases {
        let b = src.as_bytes();
        for t in TARGETS {
            let m = member_decl_of(b, &read(b, 0, b.len(), t), b.len(), 0);
            assert_eq!(
                m.init_system.as_deref(),
                want,
                "sys probe on {src:?} for {t:?}"
            );
        }
    }
}

// ===========================================================================
// Deterministic fuzz arm. Assemble decl-ish windows from name / params / type /
// init / noise fragments, draw random `from`/`to`, run the full-struct
// differential for all 4 targets. Determinism: inline xorshift64* over a fixed
// seed range. A divergence panics with the source and arguments and reproduces
// from its seed. SCAFFOLDING.
// ===========================================================================

/// Inline deterministic PRNG (xorshift64*). Mirrors the sibling batteries' prior art.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5678);
        if s == 0 {
            s = 0xDEAD_BEEF;
        }
        Rng(s)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % (n as u64)) as usize
    }
}

/// Whole-token fragments so the generator forms real decl windows (names, param groups incl.
/// unbalanced and string-trapped, types, inits with `@@` probes) — instead of relying on single
/// bytes lining up.
const FRAGMENTS: &[&str] = &[
    "go", "fetch", "async", "async ", "x", "_a1", "9q", "$.",
    "(", ")", "(a: int)", "(a: int, b: str)", "(s = \")\")", "(s = \"(\")", "(",
    ":", ": int", ": Map<k=v>", ": ",
    "=", "= 0", "= @@Sys(1)", "= @@!S()", "= @@", "= @@ X", "=@@T()",
    " ", "\t", ";", "{", "}", "\"str", "\"s\"", "->",
];

fn gen_decl_ish(rng: &mut Rng, max_frags: usize) -> String {
    let n = rng.below(max_frags + 1);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    s
}

#[test]
fn fuzz_decl_ish_every_position_all_targets() {
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x7C7C_FFFF);
        let src = gen_decl_ish(&mut rng, 8);
        let b = src.as_bytes();
        let len = b.len();
        let from = if len == 0 { 0 } else { rng.below(len + 1) };
        let to = if len == 0 { 0 } else { from + rng.below(len - from + 1) };
        for target in TARGETS {
            agree(b, from, to, target); // random window (may open mid-name / mid-string)
            agree(b, 0, len, target); // full window
        }
    }
}

/// The fuzz generator must fire every register — `is_async`, `empty_name`, `params_clamped`,
/// `has_sys` — and produce named/params/typed/initialized shapes many times over the seed
/// range; a generator that never reached a register would leave its arm untested. Assert by
/// running the system; also require corpus diversity.
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut named = 0usize;
    let mut is_async = 0usize;
    let mut empty_name = 0usize;
    let mut params_clamped = 0usize;
    let mut has_params = 0usize;
    let mut has_type = 0usize;
    let mut has_init = 0usize;
    let mut has_sys = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x7C7C_FFFF);
        let src = gen_decl_ish(&mut rng, 8);
        let b = src.as_bytes();
        distinct.insert(src.clone());
        for t in TARGETS {
            let s = read(b, 0, b.len(), t);
            if s.empty_name {
                empty_name += 1;
            } else {
                named += 1;
            }
            if s.is_async {
                is_async += 1;
            }
            if s.params_clamped {
                params_clamped += 1;
            }
            if s.has_params {
                has_params += 1;
            }
            if s.has_type {
                has_type += 1;
            }
            if s.has_init {
                has_init += 1;
            }
            if s.has_sys {
                has_sys += 1;
            }
        }
    }
    assert!(distinct.len() > 1200, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(named > 100, "too few named decls ({named})");
    assert!(empty_name > 100, "too few empty-name windows ({empty_name}) — T7 barely exercised");
    assert!(is_async > 20, "too few async modifiers ({is_async})");
    assert!(params_clamped > 20, "too few clamped param groups ({params_clamped}) — T8 barely exercised");
    assert!(has_params > 100, "too few param groups ({has_params})");
    assert!(has_type > 100, "too few typed decls ({has_type})");
    assert!(has_init > 100, "too few initialized decls ({has_init})");
    assert!(has_sys > 20, "too few `@@Sys` probes ({has_sys})");
}
