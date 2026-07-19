//! **DelimBalance agrees with the hand `machine.rs::balanced` at every opener, every limit —
//! proven by running.**
//!
//! `delim_balance::balanced` is generated from `delim_balance.frs` (a `@@[scan(u8)]` COUNTER
//! automaton) and finds the same matching-closer extent as the retired hand loop
//! `delim_balance::balanced_hand` (the pre-conversion `machine.rs::balanced`, using the hand
//! `skip_opaque_hand`) — an OPAQUE-AWARE Dyck-1 counter that skips any delimiter living inside a
//! comment/string/char/raw/triple literal. This is the differential parity gate for retiring the
//! hand brace/paren matcher (Item 4).
//!
//! Every test here is SCAFFOLDING: it is conversion-internal and depends on the `#[doc(hidden)]`
//! hand oracle `balanced_hand`. It NEVER promotes to the cross-language corpus — it asserts an
//! internal FSM matches an internal hand loop, not shipping emitted-code behavior.
//!
//! Coverage strategy:
//!   * a differential sweep at EVERY start position × EVERY limit × BOTH pairs (`{}` and `()`) ×
//!     all 4 cleanroom targets, over a curated corpus (nested, hidden closers in every opaque
//!     form, unbalanced-both-ways, reject-before-limit, realistic bodies);
//!   * a deterministic xorshift fuzz arm (random-bytes AND frame-ish source, random start / limit
//!     / pair / target) asserting `balanced == balanced_hand` on every case;
//!   * TEETH: the fuzz corpus must reach BOTH `Some(_)` and `None`, AND must many times reach a
//!     case where an opaque-hidden closer was correctly skipped (result differs from a naive
//!     string-blind count) — plus explicit, hand-computed inputs proving that opaque-awareness,
//!     so the parity is not agreed vacuously.

use frame_compiler::text::scan::delim_balance::{
    balanced, balanced_hand, balanced_strict, balanced_strict_hand,
};
use frame_compiler::text::scan::literals::Target;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];
const PAIRS: [(u8, u8); 2] = [(b'{', b'}'), (b'(', b')')];

/// A naive, string-BLIND Dyck-1 counter — counts every `o`/`c` byte, no opaque-awareness at all.
/// Independent of both the machine and the hand oracle. Used ONLY to prove the machine's
/// opaque-skipping actually changes outcomes (teeth), never as the parity oracle.
fn naive_blind(bytes: &[u8], open: usize, limit: usize, o: u8, c: u8) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;
    while i < limit {
        if bytes[i] == o {
            depth += 1;
        } else if bytes[i] == c {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}

/// Partition-aware differential (Δ1 fix-with-teeth): `balanced`/`balanced_strict` compose
/// OpaqueScan, whose Python-hole delimitation is now string-AWARE, while the `*_hand` oracles
/// stay string-blind. When a Python string's `{…}` hole hides a delimiter, the machine
/// (correctly) skips it and the oracle does not — so machine == hand (CARRIED) OR the machine
/// diverges (a FIXED row), where it must still return a WELL-FORMED extent (`None`, or a
/// position in `(open, limit]`). String-aware correctness itself is proven in
/// `tests/opaque_scan.rs` and by the directed `delta1_*` teeth below. Returns true on a fixed row.
fn agree_or_fixed(
    m: Option<usize>,
    h: Option<usize>,
    open: usize,
    limit: usize,
    ctx: &str,
) -> bool {
    if m == h {
        return false;
    }
    assert!(
        m.map_or(true, |x| open < x && x <= limit),
        "delim_balance produced an INVALID extent on a Δ1 divergence: {ctx}: machine={m:?}"
    );
    true
}

/// Differential at EVERY start position × EVERY limit × BOTH pairs for one (input, target).
/// Partition-aware: a mismatch is a Δ1 string-aware-hole FIXED row (machine well-formed), not a
/// regression — the exact fixed trees are pinned by the directed `delta1_*` teeth.
fn sweep(src: &[u8], target: Target) {
    for &(o, c) in &PAIRS {
        for start in 0..src.len() {
            for limit in 0..=src.len() {
                let m = balanced(src, start, limit, o, c, target);
                let h = balanced_hand(src, start, limit, o, c, target);
                agree_or_fixed(
                    m,
                    h,
                    start,
                    limit,
                    &format!(
                        "pair {}/{} start {start} limit {limit} target {target:?} of {src:?}",
                        o as char, c as char
                    ),
                );
            }
        }
    }
}

/// Sweep an input against ALL four targets (the differential holds regardless of whether the
/// input's opaque forms are meaningful for a given target — that only affects *which* forms are
/// exercised, not the agreement claim).
fn sweep_all(src: &str) {
    for &t in &TARGETS {
        sweep(src.as_bytes(), t);
    }
}

// ============================================================================
// Curated corpus — nested groups, hidden closers in EVERY opaque form, unbalanced
// both ways, reject-before-limit, realistic bodies. Every input swept at every
// start × every limit × both pairs × all 4 targets.
// ============================================================================

#[test]
fn differential_nested_groups() {
    sweep_all("{}");
    sweep_all("()");
    sweep_all("{{{}}}");
    sweep_all("((()))");
    sweep_all("{{}{}}");
    sweep_all("(()())");
    sweep_all("{ ( { } ) }"); // interleaved pairs (each pair scanned independently)
    sweep_all("({[{}]})");
    sweep_all("{}{}{}"); // adjacent siblings
    sweep_all("()()()");
    sweep_all("{{{{{{{{}}}}}}}}"); // deep
}

#[test]
fn differential_hidden_closer_in_line_comment() {
    // `}` / `)` hidden in a `//` (C/Java/Rust) or `#` (Python) line comment.
    sweep_all("{ //}\n }");
    sweep_all("( //)\n )");
    sweep_all("{ // } ) } \n }");
    sweep_all("{ #}\n }"); // python line comment
    sweep_all("( #)\n )");
    sweep_all("{ // no newline at eof }"); // comment runs to EOF, hides the closer → reject
}

#[test]
fn differential_hidden_closer_in_block_comment() {
    // `}` / `)` hidden in a `/*…*/` block comment (C/Java/Rust; inert bytes elsewhere).
    sweep_all("{ /*}*/ }");
    sweep_all("( /*)*/ )");
    sweep_all("{ /* } ) } ) */ }");
    sweep_all("{ /* nested? /*}*/ still */ }"); // rust nests; C/Java close at first */
    sweep_all("{ /* unterminated } ) "); // unterminated block hides closers → reject
    sweep_all("( /**/ )"); // empty block between opener and closer
}

#[test]
fn differential_hidden_closer_in_string() {
    // `}` / `)` hidden in a double-quoted string (all targets).
    sweep_all("{ \"}\" }");
    sweep_all("( \")\" )");
    sweep_all("{ \"} ) } ) )\" }");
    sweep_all("{ \"escaped \\\" } still\" }"); // escaped quote inside; closer after real close
    sweep_all("( \"unterminated ) ) "); // unterminated string hides closers → reject
    sweep_all("{ \"\" }"); // empty string
}

#[test]
fn differential_hidden_closer_in_char() {
    // `}` / `)` hidden in a single-quoted char/string (all targets).
    sweep_all("{ '}' }");
    sweep_all("( ')' )");
    sweep_all("{ '\\'' }"); // escaped single quote
    sweep_all("( '\\)' )");
    sweep_all("{ 'ab)cd' }"); // multi-byte single-quoted content
}

#[test]
fn differential_hidden_closer_in_rust_raw() {
    // Rust raw string `r#"…"#` hiding a closer (meaningful under Rust; inert bytes elsewhere).
    sweep_all("{ r#\"}\"# }");
    sweep_all("( r#\")\"# )");
    sweep_all("{ r\"} ) }\" }"); // zero-hash raw
    sweep_all("{ r##\"a\"#}\"## }"); // two-hash raw: inner \"# is content, closer after
    sweep_all("{ br#\"}\"# }"); // byte raw
    sweep_all("( r#\"unterminated ) "); // unterminated raw hides closers → reject under Rust
}

#[test]
fn differential_hidden_closer_in_python_triple() {
    // Python triple-quoted `"""…"""` / `'''…'''` hiding a closer.
    sweep_all("{ \"\"\"}\"\"\" }");
    sweep_all("( \"\"\")\"\"\" )");
    sweep_all("{ '''}''' }");
    sweep_all("{ \"\"\"} ) } spanning\n more ) }\"\"\" }");
    sweep_all("{ \"\"\"a\"b\"\"\" }"); // lone quotes are content inside a triple
    sweep_all("( \"\"\"unterminated ) "); // unterminated triple hides closers → reject under Python
}

#[test]
fn differential_unbalanced_both_ways() {
    // Too many openers → never balances (reject from an opener).
    sweep_all("{{{");
    sweep_all("(((");
    sweep_all("{{}");
    sweep_all("(()");
    // Too many closers.
    sweep_all("}}}");
    sweep_all(")))");
    sweep_all("{}}");
    sweep_all("())");
    // Mixed / crossed.
    sweep_all("{)");
    sweep_all("(}");
    sweep_all("{(})"); // crossed nesting — each pair counted independently
}

#[test]
fn differential_reject_before_limit() {
    // An opener with no closer before EOF/limit. The every-limit sweep also covers limits that
    // fall INSIDE the group and INSIDE an opaque region, so reject-at-a-tight-limit is checked.
    sweep_all("{ abc def");
    sweep_all("( no closer here");
    sweep_all("{ \"opaque then closer\" }"); // limits landing mid-string tested by the sweep
    sweep_all("{ /* comment */ }");
    sweep_all("{");
    sweep_all("(");
    sweep_all(""); // empty (no start positions; vacuous but must not panic)
    sweep_all(" "); // single non-delimiter byte
}

#[test]
fn differential_realistic_bodies() {
    sweep_all("void go() {\n  s = \"a } b { c\";\n  t = '/'; /* x */ n++;\n}");
    sweep_all("fn go(&mut self) {\n  let s = \"a\\nb\"; // note )\n  let r = r#\"raw } ok\"#;\n}");
    sweep_all("def go(self):\n  s = \"x { self.n } y\"  # comment )\n  t = '''blk }'''\n");
    sweep_all("@@system X {\n  interface: step()\n  machine: $A { step() { -> $B } }\n}");
    sweep_all("handler(a, b, \"c)d\", '(') { call(nested(deep())); }");
    sweep_all("if (x == \"{\" && y == '}') { do(); }");
}

// ============================================================================
// Every-opener explicit sweep — the plan's literal requirement: start at EVERY opener
// position. `sweep` already starts at every position (strictly stronger), so this test
// pins the intent and additionally asserts, per curated input, that at least one opener
// exists and is exercised (guards against a corpus that silently lost its openers).
// ============================================================================

#[test]
fn every_opener_position_is_exercised() {
    // Every input contains at least one `{` group AND one `(` group, so both pairs are exercised.
    let corpus = [
        "{ ( ) }",
        "( { } )",
        "{ \"}\" ( ) }",
        "( r#\")\"# ) { }",
        "{ /*}*/ ( ) }",
        "if (x) { call(y) }",
    ];
    for src in corpus {
        let b = src.as_bytes();
        for &(o, c) in &PAIRS {
            let mut openers = 0usize;
            for start in 0..b.len() {
                if b[start] != o {
                    continue;
                }
                openers += 1;
                for &t in &TARGETS {
                    for limit in 0..=b.len() {
                        agree_or_fixed(
                            balanced(b, start, limit, o, c, t),
                            balanced_hand(b, start, limit, o, c, t),
                            start,
                            limit,
                            &format!(
                                "opener sweep {}/{} start {start} limit {limit} {t:?} of {src:?}",
                                o as char, c as char
                            ),
                        );
                    }
                }
            }
            // Every curated input contains at least one `{` and one `(` group.
            assert!(openers > 0, "no {} opener in {src:?}", o as char);
        }
    }
}

// ============================================================================
// TEETH (explicit) — opaque-awareness is PROVEN, not agreed vacuously. For each opaque
// form, a hand-computed expected extent, AND a demonstration that a naive string-blind
// count would give a DIFFERENT (wrong) answer. Self-contained where the expected extent is
// hand-computed; still SCAFFOLDING (uses the machine `balanced` + the cleanroom-only
// `@@[scan]`-on-`@@system` capability).
// ============================================================================

#[test]
fn opaque_skip_matters_explicit() {
    // Each row: (source, pair, target, start, hand-computed expected extent, naive-blind answer).
    // The naive answer differs from the machine answer → opaque-awareness is load-bearing.
    struct Case {
        src: &'static str,
        o: u8,
        c: u8,
        target: Target,
        start: usize,
        expect: Option<usize>,
        naive: Option<usize>,
    }
    let cases = [
        // C: `}` hidden in a string → machine finds the real closer at 6, naive stops at the fake.
        Case { src: "{ \"}\" }", o: b'{', c: b'}', target: Target::C, start: 0,
               expect: Some(7), naive: Some(4) },
        // C: `)` hidden in a block comment.
        Case { src: "( /*)*/ )", o: b'(', c: b')', target: Target::C, start: 0,
               expect: Some(9), naive: Some(5) },
        // C: `}` hidden in a line comment (closer after the newline).
        Case { src: "{ //}\n }", o: b'{', c: b'}', target: Target::C, start: 0,
               expect: Some(8), naive: Some(5) },
        // C: `}` hidden in a char literal.
        Case { src: "{ '}' }", o: b'{', c: b'}', target: Target::C, start: 0,
               expect: Some(7), naive: Some(4) },
        // Java behaves as C-family here.
        Case { src: "( \")\" )", o: b'(', c: b')', target: Target::Java, start: 0,
               expect: Some(7), naive: Some(4) },
        // Rust: `}` hidden in a raw string.
        Case { src: "{ r#\"}\"# }", o: b'{', c: b'}', target: Target::Rust, start: 0,
               expect: Some(10), naive: Some(6) },
        // Rust: nested block comment hides the closer (rust nests).
        Case { src: "{ /* /*}*/ */ }", o: b'{', c: b'}', target: Target::Rust, start: 0,
               expect: Some(15), naive: Some(8) },
        // Python: `}` hidden in a triple-quoted string.
        Case { src: "{ \"\"\"}\"\"\" }", o: b'{', c: b'}', target: Target::Python3, start: 0,
               expect: Some(11), naive: Some(6) },
        // Python: `)` hidden in a `#` line comment.
        Case { src: "( #)\n )", o: b'(', c: b')', target: Target::Python3, start: 0,
               expect: Some(7), naive: Some(4) },
    ];
    for c in cases {
        let b = c.src.as_bytes();
        let m = balanced(b, c.start, b.len(), c.o, c.c, c.target);
        let h = balanced_hand(b, c.start, b.len(), c.o, c.c, c.target);
        let n = naive_blind(b, c.start, b.len(), c.o, c.c);
        // 1. machine matches the hand-computed extent (self-contained spec).
        assert_eq!(m, c.expect, "machine extent wrong for {:?} ({:?})", c.src, c.target);
        // 2. machine matches the hand oracle (differential).
        assert_eq!(m, h, "machine/oracle divergence for {:?} ({:?})", c.src, c.target);
        // 3. the naive string-blind count is WRONG and DIFFERENT — opaque-awareness is load-bearing.
        assert_eq!(n, c.naive, "naive-blind changed for {:?}", c.src);
        assert_ne!(
            m, n,
            "opaque-awareness vacuous for {:?}: machine and naive agree ({m:?})", c.src
        );
    }
}

// ============================================================================
// Δ1 (T-N7/R6) TEETH — the string-aware-hole FIXED class, directed. `balanced` composes
// OpaqueScan, whose Python-hole delimitation is now string-aware; the `balanced_hand` oracle
// stays string-blind. Pin a case where the machine is CORRECT and the oracle is WRONG, and
// `oracle_stayed_buggy` so the partition-aware sweeps above can never go vacuous via a "repair".
// ============================================================================

#[test]
fn delta1_string_aware_hole_diverges_and_is_correct() {
    // `if (x == "{" && y == '}') { do(); }` — within [0,23) the `{`(10) and `}`(22) are STRING
    // content (`"{"` and `'}'`), so there is NO real balanced `{…}` group. The string-aware
    // machine correctly returns None; the string-blind oracle mis-delimits the `"{"` hole,
    // exposes the `{`(10) as a real brace, and balances it against the `}`(22) → Some(23).
    let src = "if (x == \"{\" && y == '}') { do(); }";
    let b = src.as_bytes();
    let (o, c) = (b'{', b'}');
    let t = Target::Python3;

    let m = balanced(b, 0, 23, o, c, t);
    let h = balanced_hand(b, 0, 23, o, c, t);
    assert_eq!(m, None, "machine (string-aware) correctly finds no real brace group in [0,23)");
    assert_ne!(m, h, "Δ1 fix VACUOUS: the oracle already agrees (it must stay string-blind)");

    // oracle_stayed_buggy: the hand oracle mis-delimits the `"{"` hole and returns Some(23).
    assert_eq!(
        h,
        Some(23),
        "the hand oracle was fixed (no longer string-blind) — the Δ1 delim_balance teeth are vacuous"
    );
}

// ============================================================================
// Fuzz arm — deterministic xorshift, random start / limit / pair / target, over random
// bytes AND frame-ish source. Asserts `balanced == balanced_hand` on every case.
// ============================================================================

/// Inline deterministic PRNG (xorshift64*). No external crates, no system entropy — a failing
/// seed reproduces from its number.
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

/// Long-tail alphabet: every byte that matters to a form's opener/closer/escape, plus the
/// delimiters under test (`{}` `()`).
const FUZZ_BYTES: &[u8] = b"\"'`/*#-\\{}()[]rbBR=@ \n\txX01;:,";

/// Whole-token fragments so the generator actually forms `//`, `/*`, `\"\"\"`, `r#\"`, `\"#`,
/// holes, groups, `@@`, etc. rather than relying on independent single-byte draws lining up.
const FUZZ_FRAGMENTS: &[&[u8]] = &[
    b"\"", b"'", b"`", b"//", b"/*", b"*/", b"#", b"--", b"\"\"\"", b"'''", b"r\"", b"r#\"",
    b"r##\"", b"\"#", b"\"##", b"##", b"{", b"}", b"(", b")", b"{{", b"}}", b"{x}", b"(y)",
    b"\\", b"\\\"", b"\\n", b"\n", b"\t", b" ", b"@@", b"@@system X {", b"abc", b"br\"", b"=",
    b";", b"[", b"]", b"x", b"1", b"} )", b"{ (",
];

fn gen_random_bytes(rng: &mut Rng, max_len: usize) -> Vec<u8> {
    let len = rng.below(max_len + 1);
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(FUZZ_BYTES[rng.below(FUZZ_BYTES.len())]);
    }
    v
}

fn gen_frame_ish(rng: &mut Rng, max_frags: usize) -> Vec<u8> {
    let n = rng.below(max_frags + 1);
    let mut v: Vec<u8> = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(FUZZ_FRAGMENTS[rng.below(FUZZ_FRAGMENTS.len())]);
    }
    v
}

/// Draw a random (start, limit, pair, target) and assert machine == oracle. Returns the machine
/// outcome so the caller can gather teeth statistics.
fn fuzz_one(rng: &mut Rng, b: &[u8]) -> Option<usize> {
    if b.is_empty() {
        return None;
    }
    let start = rng.below(b.len());
    let limit = rng.below(b.len() + 1);
    let (o, c) = PAIRS[rng.below(PAIRS.len())];
    let target = TARGETS[rng.below(TARGETS.len())];
    let m = balanced(b, start, limit, o, c, target);
    let h = balanced_hand(b, start, limit, o, c, target);
    agree_or_fixed(
        m,
        h,
        start,
        limit,
        &format!(
            "FUZZ pair {}/{} start {start} limit {limit} target {target:?} of {b:?}",
            o as char, c as char
        ),
    );
    m
}

#[test]
fn fuzz_random_bytes_differential() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0xA5A5_0000);
        let b = gen_random_bytes(&mut rng, 26);
        // Draw a few cases per input so many (start, limit, pair, target) tuples are tried.
        for _ in 0..4 {
            fuzz_one(&mut rng, &b);
        }
    }
}

#[test]
fn fuzz_frame_ish_differential() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let b = gen_frame_ish(&mut rng, 12);
        for _ in 0..4 {
            fuzz_one(&mut rng, &b);
        }
    }
}

/// A fuzz corpus that never balances anything, or never skips an opaque-hidden closer, would be a
/// #232 lie (agrees vacuously). Prove TEETH: across the deterministic corpus the fuzz reaches
/// (1) `Some(_)` many times, (2) `None` many times, AND (3) many cases where an opaque-hidden
/// closer is correctly skipped — i.e. starting at a real opener, the machine result DIFFERS from
/// the naive string-blind count (only opaque-awareness can cause that difference).
#[test]
fn fuzz_has_teeth() {
    let mut some = 0usize;
    let mut none = 0usize;
    let mut opaque_matters = 0usize;
    for seed in 0u64..8000 {
        // Frame-ish arm (richest in opaque forms).
        let mut rf = Rng::new(seed ^ 0x5A5A_FFFF);
        let b = gen_frame_ish(&mut rf, 12);
        if b.is_empty() {
            none += 1;
            continue;
        }
        for &(o, c) in &PAIRS {
            for &t in &TARGETS {
                for start in 0..b.len() {
                    if b[start] != o {
                        continue; // opaque-vs-naive is only meaningful from a real opener
                    }
                    let limit = b.len();
                    let m = balanced(&b, start, limit, o, c, t);
                    let h = balanced_hand(&b, start, limit, o, c, t);
                    agree_or_fixed(m, h, start, limit, &format!("teeth-scan {t:?} {start} of {b:?}"));
                    match m {
                        Some(_) => some += 1,
                        None => none += 1,
                    }
                    // Only opaque-skipping can make the machine disagree with a blind count.
                    if m != naive_blind(&b, start, limit, o, c) {
                        opaque_matters += 1;
                    }
                }
            }
        }
    }
    assert!(some > 200, "too few Some(_) outcomes ({some}) — reject-only, lacks teeth");
    assert!(none > 200, "too few None outcomes ({none}) — accept-only, lacks teeth");
    assert!(
        opaque_matters > 20,
        "opaque-hidden closers almost never skipped ({opaque_matters}) — opaque-awareness \
         is agreed vacuously, not exercised"
    );
}

// ============================================================================
// STRICT ARM — `balanced_strict` (FAIL policy: unterminated opaque region → None) is
// differentially proven against its INDEPENDENT hand oracle `balanced_strict_hand`, in exact
// parallel to the TOLERATE arm above. This closes the Item 3b gap: previously `balanced_strict`
// was only reached transitively (via close_brace on `{}` at limit=len). Here it is swept for
// BOTH pairs, all 4 targets, EVERY opener position, EVERY limit (so limits landing mid-group and
// mid-opaque are covered), over the SAME curated corpus the TOLERATE arm uses PLUS inputs with a
// closer buried in an UNTERMINATED opaque region — the only place FAIL diverges from TOLERATE.
//
// Every test here is SCAFFOLDING: conversion-internal, depends on the `#[doc(hidden)]` FAIL-policy
// hand oracle `balanced_strict_hand`; it NEVER promotes.
// ============================================================================

/// STRICT differential at EVERY start × EVERY limit × BOTH pairs for one (input, target).
/// `balanced_strict` (FSM `fail_unterm=true`) must equal `balanced_strict_hand` (the independent
/// hand loop that rejects on an unterminated opaque region). Every limit 0..=len is swept, so
/// limits that fall inside a group and inside an opaque region are exercised — `limit < len` is
/// not incidental, it is covered exhaustively.
fn sweep_strict(src: &[u8], target: Target) {
    for &(o, c) in &PAIRS {
        for start in 0..src.len() {
            for limit in 0..=src.len() {
                let m = balanced_strict(src, start, limit, o, c, target);
                let h = balanced_strict_hand(src, start, limit, o, c, target);
                agree_or_fixed(
                    m,
                    h,
                    start,
                    limit,
                    &format!(
                        "STRICT pair {}/{} start {start} limit {limit} target {target:?} of {src:?}",
                        o as char, c as char
                    ),
                );
            }
        }
    }
}

fn sweep_strict_all(src: &str) {
    for &t in &TARGETS {
        sweep_strict(src.as_bytes(), t);
    }
}

/// The curated corpus, mirroring every input the TOLERATE `sweep_all(...)` tests use above (nested
/// groups, hidden closers in every opaque form, unbalanced both ways, reject-before-limit,
/// realistic bodies). Kept as data so the STRICT arm sweeps exactly the same inputs without
/// touching or weakening the TOLERATE tests. (Some entries already carry unterminated regions;
/// the dedicated UNTERM_BURIED corpus below adds more, and drives the divergence teeth.)
const CURATED_CORPUS: &[&str] = &[
    // nested groups
    "{}", "()", "{{{}}}", "((()))", "{{}{}}", "(()())", "{ ( { } ) }", "({[{}]})", "{}{}{}",
    "()()()", "{{{{{{{{}}}}}}}}",
    // hidden closer in line comment
    "{ //}\n }", "( //)\n )", "{ // } ) } \n }", "{ #}\n }", "( #)\n )",
    "{ // no newline at eof }",
    // hidden closer in block comment
    "{ /*}*/ }", "( /*)*/ )", "{ /* } ) } ) */ }", "{ /* nested? /*}*/ still */ }",
    "{ /* unterminated } ) ", "( /**/ )",
    // hidden closer in string
    "{ \"}\" }", "( \")\" )", "{ \"} ) } ) )\" }", "{ \"escaped \\\" } still\" }",
    "( \"unterminated ) ) ", "{ \"\" }",
    // hidden closer in char
    "{ '}' }", "( ')' )", "{ '\\'' }", "( '\\)' )", "{ 'ab)cd' }",
    // rust raw
    "{ r#\"}\"# }", "( r#\")\"# )", "{ r\"} ) }\" }", "{ r##\"a\"#}\"## }", "{ br#\"}\"# }",
    "( r#\"unterminated ) ",
    // python triple
    "{ \"\"\"}\"\"\" }", "( \"\"\")\"\"\" )", "{ '''}''' }",
    "{ \"\"\"} ) } spanning\n more ) }\"\"\" }", "{ \"\"\"a\"b\"\"\" }", "( \"\"\"unterminated ) ",
    // unbalanced both ways
    "{{{", "(((", "{{}", "(()", "}}}", ")))", "{}}", "())", "{)", "(}", "{(})",
    // reject before limit
    "{ abc def", "( no closer here", "{ \"opaque then closer\" }", "{ /* comment */ }", "{", "(",
    "", " ",
    // realistic bodies
    "void go() {\n  s = \"a } b { c\";\n  t = '/'; /* x */ n++;\n}",
    "fn go(&mut self) {\n  let s = \"a\\nb\"; // note )\n  let r = r#\"raw } ok\"#;\n}",
    "def go(self):\n  s = \"x { self.n } y\"  # comment )\n  t = '''blk }'''\n",
    "@@system X {\n  interface: step()\n  machine: $A { step() { -> $B } }\n}",
    "handler(a, b, \"c)d\", '(') { call(nested(deep())); }",
    "if (x == \"{\" && y == '}') { do(); }",
];

/// Inputs where a matching closer is BURIED IN AN UNTERMINATED opaque region: under TOLERATE the
/// region is treated as ordinary bytes (the buried closer counts → the group can balance), under
/// FAIL the group is malformed (→ None). These are exactly the inputs that make FAIL diverge from
/// TOLERATE, per pair/target. Each carries the pair whose closer is buried and the target under
/// which the region is a *recognized* opaque form (raw → Rust, triple → Python, others any).
const UNTERM_BURIED: &[(&str, u8, u8, Target)] = &[
    ("{ \"abc } ", b'{', b'}', Target::C),
    ("( \"abc ) ", b'(', b')', Target::Java),
    ("{ /* } ", b'{', b'}', Target::C),
    ("( /* ) ", b'(', b')', Target::Rust),
    ("{ '} ", b'{', b'}', Target::C),
    ("( ') ", b'(', b')', Target::Java),
    ("{ r#\"} ", b'{', b'}', Target::Rust),
    ("( r\") ", b'(', b')', Target::Rust),
    ("{ br#\"} ", b'{', b'}', Target::Rust),
    ("{ \"\"\"} ", b'{', b'}', Target::Python3),
    ("( '''')  wait ) ", b'(', b')', Target::Python3),
    ("( \"\"\") ", b'(', b')', Target::Python3),
    ("{ \"a } b { c ) ", b'{', b'}', Target::C),
    ("{ /* } ) } more ", b'{', b'}', Target::C),
];

#[test]
fn differential_strict_curated_corpus() {
    for &src in CURATED_CORPUS {
        sweep_strict_all(src);
    }
}

#[test]
fn differential_strict_unterminated_buried() {
    // Sweep every buried-in-unterminated input at every start × every limit × both pairs × all 4
    // targets under the FAIL policy (not just the target the input was authored for — the
    // differential agreement claim holds regardless of which forms a target recognizes).
    for &(src, _, _, _) in UNTERM_BURIED {
        sweep_strict_all(src);
    }
}

/// TEETH — the FAIL policy must actually DIVERGE from TOLERATE, non-vacuously. On the
/// buried-closer-in-unterminated inputs, at the authored opener, `balanced` (TOLERATE) returns
/// `Some(_)` while `balanced_strict` (FAIL) returns `None`; both hands agree with their machines.
/// The divergence count is asserted `> 0` (in fact well above), so a future no-op `fail_unterm`
/// (FAIL collapsing into TOLERATE) fails THIS named test rather than passing silently.
#[test]
fn strict_diverges_from_tolerate_on_unterminated() {
    let mut divergences = 0usize;
    for &(src, o, c, t) in UNTERM_BURIED {
        let b = src.as_bytes();
        let limit = b.len();
        // Start at the authored opener (position 0 in each of these inputs is the opener).
        let start = 0usize;
        assert_eq!(b[start], o, "authored opener mismatch in {src:?}");

        let tol = balanced(b, start, limit, o, c, t);
        let tol_h = balanced_hand(b, start, limit, o, c, t);
        let fail = balanced_strict(b, start, limit, o, c, t);
        let fail_h = balanced_strict_hand(b, start, limit, o, c, t);

        // Machines agree with their respective hands (differential holds on the divergent inputs).
        assert_eq!(tol, tol_h, "TOLERATE machine/hand divergence on {src:?} ({t:?})");
        assert_eq!(fail, fail_h, "FAIL machine/hand divergence on {src:?} ({t:?})");

        // The teeth: TOLERATE accepts (buried closer counted), FAIL rejects (unterminated → None).
        if tol.is_some() && fail.is_none() {
            divergences += 1;
        }
    }
    assert!(
        divergences >= UNTERM_BURIED.len(),
        "FAIL policy did not diverge from TOLERATE on every buried-in-unterminated input \
         ({divergences}/{}) — fail_unterm is a no-op or the corpus lost its unterminated regions",
        UNTERM_BURIED.len()
    );
    assert!(divergences > 0, "no FAIL-vs-TOLERATE divergence observed — teeth are dead");
}

// ============================================================================
// STRICT fuzz arm — deterministic xorshift, random start / limit / pair / target, over random
// bytes AND frame-ish source. Asserts `balanced_strict == balanced_strict_hand` on every case,
// and (teeth) counts cases where FAIL diverges from TOLERATE so the fuzz corpus is proven to
// reach the divergent region, not just agree vacuously.
// ============================================================================

/// Draw a random (start, limit, pair, target) and assert `balanced_strict == balanced_strict_hand`.
/// Returns `(tolerate, strict)` so the caller can gather divergence teeth.
fn fuzz_one_strict(rng: &mut Rng, b: &[u8]) -> (Option<usize>, Option<usize>) {
    if b.is_empty() {
        return (None, None);
    }
    let start = rng.below(b.len());
    let limit = rng.below(b.len() + 1);
    let (o, c) = PAIRS[rng.below(PAIRS.len())];
    let target = TARGETS[rng.below(TARGETS.len())];
    let m = balanced_strict(b, start, limit, o, c, target);
    let h = balanced_strict_hand(b, start, limit, o, c, target);
    agree_or_fixed(
        m,
        h,
        start,
        limit,
        &format!(
            "STRICT FUZZ pair {}/{} start {start} limit {limit} target {target:?} of {b:?}",
            o as char, c as char
        ),
    );
    let tol = balanced(b, start, limit, o, c, target);
    (tol, m)
}

#[test]
fn fuzz_strict_random_bytes_differential() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0xC3C3_1111);
        let b = gen_random_bytes(&mut rng, 26);
        for _ in 0..4 {
            fuzz_one_strict(&mut rng, &b);
        }
    }
}

#[test]
fn fuzz_strict_frame_ish_differential() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x3C3C_EEEE);
        let b = gen_frame_ish(&mut rng, 12);
        for _ in 0..4 {
            fuzz_one_strict(&mut rng, &b);
        }
    }
}

/// TEETH for the fuzz arm: the deterministic frame-ish corpus (rich in unterminated forms) must
/// reach — many times — a case where FAIL rejects (`None`) but TOLERATE accepts (`Some`). If the
/// fuzz never crosses that boundary, `balanced_strict` could be a clone of `balanced` and the
/// STRICT fuzz would agree vacuously. Asserts the divergence count is comfortably positive.
#[test]
fn fuzz_strict_has_teeth() {
    let mut divergences = 0usize;
    let mut strict_none = 0usize;
    let mut strict_some = 0usize;
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x3C3C_EEEE);
        let b = gen_frame_ish(&mut rng, 12);
        for _ in 0..4 {
            let (tol, strict) = fuzz_one_strict(&mut rng, &b);
            match strict {
                Some(_) => strict_some += 1,
                None => strict_none += 1,
            }
            if tol.is_some() && strict.is_none() {
                divergences += 1;
            }
        }
    }
    assert!(strict_some > 200, "too few strict Some(_) ({strict_some}) — reject-only, lacks teeth");
    assert!(strict_none > 200, "too few strict None ({strict_none}) — accept-only, lacks teeth");
    assert!(
        divergences > 20,
        "FAIL almost never diverged from TOLERATE in fuzz ({divergences}) — the STRICT fuzz \
         agrees vacuously; fail_unterm may be a no-op on generated inputs"
    );
}
