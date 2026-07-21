//! **DelimBalance obeys its standalone invariants — proven by running.**
//!
//! `delim_balance::balanced` is generated from `delim_balance.frs` (a `@@[scan(u8)]` COUNTER
//! automaton): an OPAQUE-AWARE Dyck-1 counter that finds the matching-closer extent of a
//! `{}` / `()` group, skipping any delimiter living inside a comment/string/char/raw/triple
//! literal. `balanced_strict` is the FAIL variant (an unterminated opaque region → `None`).
//!
//! Every test here is SCAFFOLDING (white-box on the internal `balanced`/`balanced_strict`); it
//! NEVER promotes. The standalone spec has three layers, no hand oracle:
//!
//!   * **Independent correctness on pure-Dyck inputs**: where there is NO opaque content, a
//!     matching closer is a pure bracket-count, so `balanced` must EQUAL the tiny independent
//!     string-blind counter `naive_blind` at every start × limit × pair × target. (`naive_blind`
//!     is a five-line reference, not the production logic.)
//!   * **Directed opaque-aware extents (hand-computed)**: for each opaque form, an input with a
//!     HAND-COMPUTED expected extent, plus a demonstration that `naive_blind` gives a DIFFERENT
//!     (wrong) answer — so opaque-awareness is proven load-bearing, not agreed vacuously.
//!   * **Well-formedness sweeps + teeth**: at every start × limit × pair × target over the
//!     curated corpus AND a deterministic fuzz corpus, `balanced` returns a WELL-FORMED extent
//!     (`None`, or a position in `(open, limit]`) and never panics; teeth counters (via
//!     `naive_blind`) prove the fuzz actually reaches Some/None AND opaque-skipping outcomes.
//!     The FAIL policy is proven to DIVERGE from TOLERATE by comparing the two machines directly.

use frame_compiler::text::scan::delim_balance::{balanced, balanced_strict};
use frame_compiler::text::scan::literals::Target;

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];
const PAIRS: [(u8, u8); 2] = [(b'{', b'}'), (b'(', b')')];

/// A naive, string-BLIND Dyck-1 counter — counts every `o`/`c` byte, no opaque-awareness at all.
/// Independent of the production logic (five lines). Two uses: the parity oracle on pure-Dyck
/// (no-opaque) inputs, and the teeth witness that opaque-skipping actually changes outcomes.
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

/// Well-formedness: an extent is `None` or a position strictly after `open` and at most `limit`.
fn assert_wf(m: Option<usize>, open: usize, limit: usize, ctx: &str) {
    assert!(
        m.map_or(true, |x| open < x && x <= limit),
        "delim_balance produced an INVALID extent: {ctx}: machine={m:?}"
    );
}

/// Well-formedness sweep at EVERY start × EVERY limit × BOTH pairs for one (input, target).
fn sweep(src: &[u8], target: Target) {
    for &(o, c) in &PAIRS {
        for start in 0..src.len() {
            for limit in 0..=src.len() {
                let m = balanced(src, start, limit, o, c, target);
                assert_wf(
                    m,
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

fn sweep_all(src: &str) {
    for &t in &TARGETS {
        sweep(src.as_bytes(), t);
    }
}

// ============================================================================
// INDEPENDENT correctness on pure-Dyck (no-opaque) inputs — `balanced == naive_blind`
// at every start × limit × pair × target. These inputs carry no comment/string, so the
// opaque skip is inert and a matching closer is exactly a bracket-count; the independent
// `naive_blind` is a valid oracle here.
// ============================================================================

/// Every byte of `src` must be one of `{}()[]` or whitespace (so `naive_blind`, which only knows
/// the pair under test, is a correct oracle: no opaque form can hide a delimiter).
fn pure_dyck_matches_naive(src: &str) {
    let b = src.as_bytes();
    for &t in &TARGETS {
        for &(o, c) in &PAIRS {
            for start in 0..b.len() {
                for limit in 0..=b.len() {
                    assert_eq!(
                        balanced(b, start, limit, o, c, t),
                        naive_blind(b, start, limit, o, c),
                        "pure-Dyck divergence: pair {}/{} start {start} limit {limit} {t:?} of {src:?}",
                        o as char,
                        c as char
                    );
                }
            }
        }
    }
}

#[test]
fn pure_dyck_agrees_with_the_independent_counter() {
    for src in [
        "{}", "()", "{{{}}}", "((()))", "{{}{}}", "(()())", "{ ( { } ) }", "({[{}]})", "{}{}{}",
        "()()()", "{{{{{{{{}}}}}}}}",
        // unbalanced both ways
        "{{{", "(((", "{{}", "(()", "}}}", ")))", "{}}", "())", "{)", "(}", "{(})",
        // reject before limit (no opaque)
        "{ abc def", "( no closer here", "{", "(", "", " ",
    ] {
        pure_dyck_matches_naive(src);
    }
}

// ============================================================================
// Well-formedness sweeps — nested groups, hidden closers in EVERY opaque form, unbalanced
// both ways, reject-before-limit, realistic bodies. Every input swept at every start × every
// limit × both pairs × all 4 targets; the extent is always well-formed and never panics.
// ============================================================================

#[test]
fn wf_nested_groups() {
    sweep_all("{}");
    sweep_all("()");
    sweep_all("{{{}}}");
    sweep_all("((()))");
    sweep_all("{{}{}}");
    sweep_all("(()())");
    sweep_all("{ ( { } ) }");
    sweep_all("({[{}]})");
    sweep_all("{}{}{}");
    sweep_all("()()()");
    sweep_all("{{{{{{{{}}}}}}}}");
}

#[test]
fn wf_hidden_closer_in_line_comment() {
    sweep_all("{ //}\n }");
    sweep_all("( //)\n )");
    sweep_all("{ // } ) } \n }");
    sweep_all("{ #}\n }");
    sweep_all("( #)\n )");
    sweep_all("{ // no newline at eof }");
}

#[test]
fn wf_hidden_closer_in_block_comment() {
    sweep_all("{ /*}*/ }");
    sweep_all("( /*)*/ )");
    sweep_all("{ /* } ) } ) */ }");
    sweep_all("{ /* nested? /*}*/ still */ }");
    sweep_all("{ /* unterminated } ) ");
    sweep_all("( /**/ )");
}

#[test]
fn wf_hidden_closer_in_string() {
    sweep_all("{ \"}\" }");
    sweep_all("( \")\" )");
    sweep_all("{ \"} ) } ) )\" }");
    sweep_all("{ \"escaped \\\" } still\" }");
    sweep_all("( \"unterminated ) ) ");
    sweep_all("{ \"\" }");
}

#[test]
fn wf_hidden_closer_in_char() {
    sweep_all("{ '}' }");
    sweep_all("( ')' )");
    sweep_all("{ '\\'' }");
    sweep_all("( '\\)' )");
    sweep_all("{ 'ab)cd' }");
}

#[test]
fn wf_hidden_closer_in_rust_raw() {
    sweep_all("{ r#\"}\"# }");
    sweep_all("( r#\")\"# )");
    sweep_all("{ r\"} ) }\" }");
    sweep_all("{ r##\"a\"#}\"## }");
    sweep_all("{ br#\"}\"# }");
    sweep_all("( r#\"unterminated ) ");
}

#[test]
fn wf_hidden_closer_in_python_triple() {
    sweep_all("{ \"\"\"}\"\"\" }");
    sweep_all("( \"\"\")\"\"\" )");
    sweep_all("{ '''}''' }");
    sweep_all("{ \"\"\"} ) } spanning\n more ) }\"\"\" }");
    sweep_all("{ \"\"\"a\"b\"\"\" }");
    sweep_all("( \"\"\"unterminated ) ");
}

#[test]
fn wf_reject_before_limit() {
    sweep_all("{ abc def");
    sweep_all("( no closer here");
    sweep_all("{ \"opaque then closer\" }");
    sweep_all("{ /* comment */ }");
    sweep_all("{");
    sweep_all("(");
    sweep_all("");
    sweep_all(" ");
}

#[test]
fn wf_realistic_bodies() {
    sweep_all("void go() {\n  s = \"a } b { c\";\n  t = '/'; /* x */ n++;\n}");
    sweep_all("fn go(&mut self) {\n  let s = \"a\\nb\"; // note )\n  let r = r#\"raw } ok\"#;\n}");
    sweep_all("def go(self):\n  s = \"x { self.n } y\"  # comment )\n  t = '''blk }'''\n");
    sweep_all("@@system X {\n  interface: step()\n  machine: $A { step() { -> $B } }\n}");
    sweep_all("handler(a, b, \"c)d\", '(') { call(nested(deep())); }");
    sweep_all("if (x == \"{\" && y == '}') { do(); }");
}

// ============================================================================
// Every-opener explicit sweep — start at EVERY opener position, well-formed everywhere, and
// (guard) assert at least one opener of each pair exists so a corpus cannot silently lose them.
// ============================================================================

#[test]
fn every_opener_position_is_exercised() {
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
                        assert_wf(
                            balanced(b, start, limit, o, c, t),
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
            assert!(openers > 0, "no {} opener in {src:?}", o as char);
        }
    }
}

// ============================================================================
// Directed opaque-awareness (hand-computed) — for each opaque form, a hand-computed expected
// extent AND a demonstration that a naive string-blind count gives a DIFFERENT (wrong) answer.
// Self-contained: the expected extent is hand-computed, no oracle.
// ============================================================================

#[test]
fn opaque_skip_matters_explicit() {
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
        let n = naive_blind(b, c.start, b.len(), c.o, c.c);
        // 1. machine matches the hand-computed extent (self-contained spec).
        assert_eq!(m, c.expect, "machine extent wrong for {:?} ({:?})", c.src, c.target);
        // 2. the naive string-blind count is WRONG and DIFFERENT — opaque-awareness is load-bearing.
        assert_eq!(n, c.naive, "naive-blind changed for {:?}", c.src);
        assert_ne!(
            m, n,
            "opaque-awareness vacuous for {:?}: machine and naive agree ({m:?})", c.src
        );
    }
}

/// A directed string-aware-hole case, self-contained: within `[0,23)` the `{`(10) and `}`(22)
/// are STRING content (`"{"` and `'}'`), so there is NO real balanced `{…}` group — the
/// string-aware machine correctly returns `None`, while a string-blind count would balance them.
#[test]
fn string_aware_hole_finds_no_real_group() {
    let src = "if (x == \"{\" && y == '}') { do(); }";
    let b = src.as_bytes();
    let (o, c) = (b'{', b'}');
    let t = Target::Python3;
    let m = balanced(b, 0, 23, o, c, t);
    assert_eq!(m, None, "machine (string-aware) correctly finds no real brace group in [0,23)");
    // A string-blind count WOULD balance the `"{"`(10) against `'}'`(22) → the teeth.
    assert_eq!(
        naive_blind(b, 0, 23, o, c),
        Some(23),
        "the naive string-blind counter mis-balances the in-string braces — opaque-awareness matters"
    );
}

// ============================================================================
// Fuzz arm — deterministic xorshift, random start / limit / pair / target, over random bytes AND
// frame-ish source. Well-formedness on every case; teeth prove the corpus reaches Some/None AND
// opaque-skipping outcomes.
// ============================================================================

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

const FUZZ_BYTES: &[u8] = b"\"'`/*#-\\{}()[]rbBR=@ \n\txX01;:,";

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

/// Draw a random (start, limit, pair, target) and assert `balanced` is well-formed. Returns the
/// machine outcome so the caller can gather teeth statistics.
fn fuzz_one(rng: &mut Rng, b: &[u8]) -> Option<usize> {
    if b.is_empty() {
        return None;
    }
    let start = rng.below(b.len());
    let limit = rng.below(b.len() + 1);
    let (o, c) = PAIRS[rng.below(PAIRS.len())];
    let target = TARGETS[rng.below(TARGETS.len())];
    let m = balanced(b, start, limit, o, c, target);
    assert_wf(
        m,
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
fn fuzz_random_bytes_wellformed() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0xA5A5_0000);
        let b = gen_random_bytes(&mut rng, 26);
        for _ in 0..4 {
            fuzz_one(&mut rng, &b);
        }
    }
}

#[test]
fn fuzz_frame_ish_wellformed() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let b = gen_frame_ish(&mut rng, 12);
        for _ in 0..4 {
            fuzz_one(&mut rng, &b);
        }
    }
}

/// A fuzz corpus that never balances anything, or never skips an opaque-hidden closer, would be a
/// #232 lie. Prove TEETH: across the deterministic corpus the fuzz reaches (1) `Some(_)` many
/// times, (2) `None` many times, AND (3) many cases where an opaque-hidden closer is correctly
/// skipped — i.e. from a real opener, `balanced` DIFFERS from the naive string-blind count (only
/// opaque-awareness can cause that difference).
#[test]
fn fuzz_has_teeth() {
    let mut some = 0usize;
    let mut none = 0usize;
    let mut opaque_matters = 0usize;
    for seed in 0u64..8000 {
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
                        continue;
                    }
                    let limit = b.len();
                    let m = balanced(&b, start, limit, o, c, t);
                    assert_wf(m, start, limit, &format!("teeth-scan {t:?} {start} of {b:?}"));
                    match m {
                        Some(_) => some += 1,
                        None => none += 1,
                    }
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
         is exercised vacuously"
    );
}

// ============================================================================
// STRICT ARM — `balanced_strict` (FAIL policy: unterminated opaque region → None). Well-formed
// over the same curated corpus PLUS inputs with a closer buried in an UNTERMINATED opaque region;
// the FAIL-vs-TOLERATE divergence is proven by comparing the two MACHINES directly (no oracle).
// ============================================================================

fn sweep_strict(src: &[u8], target: Target) {
    for &(o, c) in &PAIRS {
        for start in 0..src.len() {
            for limit in 0..=src.len() {
                let m = balanced_strict(src, start, limit, o, c, target);
                assert_wf(
                    m,
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

const CURATED_CORPUS: &[&str] = &[
    "{}", "()", "{{{}}}", "((()))", "{{}{}}", "(()())", "{ ( { } ) }", "({[{}]})", "{}{}{}",
    "()()()", "{{{{{{{{}}}}}}}}",
    "{ //}\n }", "( //)\n )", "{ // } ) } \n }", "{ #}\n }", "( #)\n )",
    "{ // no newline at eof }",
    "{ /*}*/ }", "( /*)*/ )", "{ /* } ) } ) */ }", "{ /* nested? /*}*/ still */ }",
    "{ /* unterminated } ) ", "( /**/ )",
    "{ \"}\" }", "( \")\" )", "{ \"} ) } ) )\" }", "{ \"escaped \\\" } still\" }",
    "( \"unterminated ) ) ", "{ \"\" }",
    "{ '}' }", "( ')' )", "{ '\\'' }", "( '\\)' )", "{ 'ab)cd' }",
    "{ r#\"}\"# }", "( r#\")\"# )", "{ r\"} ) }\" }", "{ r##\"a\"#}\"## }", "{ br#\"}\"# }",
    "( r#\"unterminated ) ",
    "{ \"\"\"}\"\"\" }", "( \"\"\")\"\"\" )", "{ '''}''' }",
    "{ \"\"\"} ) } spanning\n more ) }\"\"\" }", "{ \"\"\"a\"b\"\"\" }", "( \"\"\"unterminated ) ",
    "{{{", "(((", "{{}", "(()", "}}}", ")))", "{}}", "())", "{)", "(}", "{(})",
    "{ abc def", "( no closer here", "{ \"opaque then closer\" }", "{ /* comment */ }", "{", "(",
    "", " ",
    "void go() {\n  s = \"a } b { c\";\n  t = '/'; /* x */ n++;\n}",
    "fn go(&mut self) {\n  let s = \"a\\nb\"; // note )\n  let r = r#\"raw } ok\"#;\n}",
    "def go(self):\n  s = \"x { self.n } y\"  # comment )\n  t = '''blk }'''\n",
    "@@system X {\n  interface: step()\n  machine: $A { step() { -> $B } }\n}",
    "handler(a, b, \"c)d\", '(') { call(nested(deep())); }",
    "if (x == \"{\" && y == '}') { do(); }",
];

/// Inputs where a matching closer is BURIED IN AN UNTERMINATED opaque region: under TOLERATE the
/// region is treated as ordinary bytes (the buried closer counts → the group can balance), under
/// FAIL the group is malformed (→ None). These make FAIL diverge from TOLERATE. Each carries the
/// pair whose closer is buried and the target under which the region is a *recognized* opaque form.
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
fn wf_strict_curated_corpus() {
    for &src in CURATED_CORPUS {
        sweep_strict_all(src);
    }
}

#[test]
fn wf_strict_unterminated_buried() {
    for &(src, _, _, _) in UNTERM_BURIED {
        sweep_strict_all(src);
    }
}

/// TEETH — the FAIL policy must actually DIVERGE from TOLERATE, non-vacuously and self-contained:
/// on the buried-closer-in-unterminated inputs, at the authored opener, `balanced` (TOLERATE)
/// returns `Some(_)` while `balanced_strict` (FAIL) returns `None`. A future no-op `fail_unterm`
/// (FAIL collapsing into TOLERATE) fails THIS named test rather than passing silently.
#[test]
fn strict_diverges_from_tolerate_on_unterminated() {
    let mut divergences = 0usize;
    for &(src, o, c, t) in UNTERM_BURIED {
        let b = src.as_bytes();
        let limit = b.len();
        let start = 0usize;
        assert_eq!(b[start], o, "authored opener mismatch in {src:?}");

        let tol = balanced(b, start, limit, o, c, t);
        let fail = balanced_strict(b, start, limit, o, c, t);
        assert_wf(tol, start, limit, &format!("TOLERATE {src:?} ({t:?})"));
        assert_wf(fail, start, limit, &format!("FAIL {src:?} ({t:?})"));

        // The teeth: TOLERATE accepts (buried closer counted), FAIL rejects (unterminated → None).
        if tol.is_some() && fail.is_none() {
            divergences += 1;
        }
    }
    assert_eq!(
        divergences,
        UNTERM_BURIED.len(),
        "FAIL policy did not diverge from TOLERATE on every buried-in-unterminated input \
         ({divergences}/{}) — fail_unterm is a no-op or the corpus lost its unterminated regions",
        UNTERM_BURIED.len()
    );
}

/// Draw a random (start, limit, pair, target) and assert `balanced_strict` is well-formed. Returns
/// `(tolerate, strict)` so the caller can gather divergence teeth.
fn fuzz_one_strict(rng: &mut Rng, b: &[u8]) -> (Option<usize>, Option<usize>) {
    if b.is_empty() {
        return (None, None);
    }
    let start = rng.below(b.len());
    let limit = rng.below(b.len() + 1);
    let (o, c) = PAIRS[rng.below(PAIRS.len())];
    let target = TARGETS[rng.below(TARGETS.len())];
    let m = balanced_strict(b, start, limit, o, c, target);
    assert_wf(
        m,
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
fn fuzz_strict_random_bytes_wellformed() {
    for seed in 0u64..8000 {
        let mut rng = Rng::new(seed ^ 0xC3C3_1111);
        let b = gen_random_bytes(&mut rng, 26);
        for _ in 0..4 {
            fuzz_one_strict(&mut rng, &b);
        }
    }
}

#[test]
fn fuzz_strict_frame_ish_wellformed() {
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
/// STRICT fuzz would agree vacuously. Self-contained (machine-vs-machine).
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
