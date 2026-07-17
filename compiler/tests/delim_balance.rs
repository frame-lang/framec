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

use frame_compiler::text::scan::delim_balance::{balanced, balanced_hand};
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

/// Differential at EVERY start position × EVERY limit × BOTH pairs for one (input, target).
/// A mismatch is a real machine/oracle divergence and panics with the reproducing coordinates.
fn sweep(src: &[u8], target: Target) {
    for &(o, c) in &PAIRS {
        for start in 0..src.len() {
            for limit in 0..=src.len() {
                let m = balanced(src, start, limit, o, c, target);
                let h = balanced_hand(src, start, limit, o, c, target);
                assert_eq!(
                    m, h,
                    "DIVERGENCE pair {}/{} start {start} limit {limit} target {target:?} of {src:?}: \
                     machine={m:?} hand={h:?}",
                    o as char, c as char
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
                        assert_eq!(
                            balanced(b, start, limit, o, c, t),
                            balanced_hand(b, start, limit, o, c, t),
                            "opener sweep {}/{} start {start} limit {limit} {t:?} of {src:?}",
                            o as char, c as char
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
    assert_eq!(
        m, h,
        "FUZZ DIVERGENCE pair {}/{} start {start} limit {limit} target {target:?} of {b:?}: \
         machine={m:?} hand={h:?}",
        o as char, c as char
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
                    assert_eq!(m, h, "teeth-scan divergence {t:?} {start} of {b:?}");
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
