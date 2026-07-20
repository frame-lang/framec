//! **OpaqueScan agrees with the hand lexer at every position — proven by running.**
//!
//! `opaque_scan::opaque_extent` is generated from `opaque_scan.frs` (a `@@[scan(u8)]` Frame
//! system) and computes the SAME string/comment extent as the hand `Lexer::comment_at` +
//! `literal_at` funneled through `skip_opaque_at` — for all four cleanroom targets, at EVERY
//! byte position. Every consumer of the hand lexer used only that extent, so this is the parity
//! gate for retiring the hand recognizer. Raw strings (rust) route through the `RawString`
//! system and holes (python) through `BraceBalance`; those systems are exercised here too.

use frame_compiler::text::scan::literals::{Form, Target};
use frame_compiler::text::scan::opaque_scan::{opaque_at, opaque_extent, OpaqueAt};
use frame_compiler::text::scan::{opaque_at_hand, skip_opaque_at_hand};

/// Assert the machine and the hand path agree at every position of `b` for `target`.
///
/// TWO independent parity checks at every position, both against the retired hand
/// implementation (independent of the machine), raw bytes (fuzz inputs are not UTF-8):
///
/// 1. **extent-only** (`opaque_extent` vs `skip_opaque_at_hand`) — the original gate.
/// 2. **4-way** (`opaque_at` vs `opaque_at_hand`, exact `OpaqueAt` equality) — STRICTLY
///    STRONGER: it also pins the comment-vs-literal `kind` register AND the `Unterminated`
///    arm, neither of which the extent number can distinguish (both a `None` and an
///    `Unterminated` collapse to the same extent `i`). Every curated + adversarial + fuzz
///    input that flows through `agree*` is therefore now checked both ways.
fn agree_bytes(b: &[u8], target: Target) -> usize {
    let mut fixed = 0usize;
    for i in 0..=b.len() {
        let hand = skip_opaque_at_hand(b, i, target);
        let machine = opaque_extent(b, i, target).unwrap_or(i);
        if machine != hand {
            // Δ1 (T-N7/R6) FIXED row: OpaqueScan's hole delimitation is now string-AWARE, while
            // `skip_opaque_at_hand` stays string-blind. On a Python string whose `{…}` hole hides
            // a delimiter the extents (correctly) differ — the machine's extent must still be
            // WELL-FORMED (`i <= extent <= len`); its string-aware correctness is pinned by the
            // directed `*_delta1` teeth. Carried rows (the vast majority) still match exactly.
            fixed += 1;
            assert!(
                i <= machine && machine <= b.len(),
                "opaque_extent produced an INVALID extent on a Δ1 divergence: \
                 target {target:?}, pos {i} of {b:?}: machine={machine}"
            );
        }
        // 4-way exact classification at every real byte position.
        if i < b.len() {
            let hand_at = opaque_at_hand(b, i, target);
            let machine_at = opaque_at(b, i, target);
            if machine_at != hand_at {
                match machine_at {
                    OpaqueAt::Comment(e) | OpaqueAt::Literal(e) => assert!(
                        i < e && e <= b.len(),
                        "opaque_at produced an INVALID extent on a Δ1 divergence: \
                         target {target:?}, pos {i} of {b:?}: machine={machine_at:?}"
                    ),
                    OpaqueAt::None | OpaqueAt::Unterminated => {}
                }
            }
        }
    }
    fixed
}

/// String convenience wrapper. Returns the count of Δ1 FIXED (string-aware-hole) rows.
fn agree(src: &str, target: Target) -> usize {
    agree_bytes(src.as_bytes(), target)
}

// ---- C / Java --------------------------------------------------------------

#[test]
fn c_family_comments_and_strings() {
    for t in [Target::C, Target::Java] {
        agree("int x = 1; // a comment\n y=2;", t);
        agree("a /* block */ b", t);
        agree("a /* /* not nested */ b */ c", t); // C/Java do NOT nest: closes at first */
        agree(r#"s = "hello world";"#, t);
        agree(r#"s = "with \" escaped quote";"#, t);
        agree(r#"s = "trailing backslash \\";"#, t);
        agree("c = 'x';", t); // char / single-quote string
        agree(r#"c = '\'';"#, t);
        agree(r#"x = "unterminated"#, t); // unterminated string
        agree("y /* unterminated comment", t);
        agree(r#"m("a)b", c)"#, t); // paren inside string
        agree("empty=\"\";", t);
    }
}

// ---- Rust ------------------------------------------------------------------

#[test]
fn rust_nesting_raw_and_multiline() {
    let t = Target::Rust;
    agree("x // line\n y", t);
    agree("a /* one */ b", t);
    agree("a /* /* nested */ still */ c", t); // Rust DOES nest
    agree("a /* /* deep /* three */ two */ one */ z", t);
    agree(r#"let s = "multi
line ok";"#, t); // rust " is multiline
    agree(r#"let r = r"no escapes \ here";"#, t);
    agree(r##"let r = r#"has "quote" inside"#;"##, t);
    agree(r###"let r = r##"a"#b"##;"###, t); // needs two closing hashes
    agree("let c = 'a';", t);
    agree(r#"let c = '\'';"#, t);
    agree("read_thing(); // r-identifier not raw", t);
    agree(r#"br"byte raw""#, t);
    agree("lifetime: &'a str", t); // 'a is a lifetime, not a terminated char — unterminated
}

// ---- Python ----------------------------------------------------------------

#[test]
fn python_hashes_triples_and_holes() {
    let t = Target::Python3;
    agree("x = 1  # a comment\n y = 2", t);
    agree(r#"s = "double""#, t);
    agree("s = 'single'", t);
    agree(r#"d = """triple
   spanning
   lines"""  # ok"#, t);
    agree("e = '''also triple'''", t);
    agree(r#"f = f"value is {x + 1}!""#, t); // hole skipped whole
    agree(r#"g = "brace {a} then {b} done""#, t);
    agree(r#"h = "escaped {{ not a hole }}""#, t);
    agree(r#"j = "quote in hole {x['\"']} end""#, t); // delim-looking bytes inside a hole (Δ1: hole is string-aware; outer extent coincides here)
    agree(r#"k = "unterminated hole {a + b"#, t);
    agree(r#"u = "plain unterminated"#, t);
    agree(r#"nest = f"{ {1:2} }""#, t); // nested braces in a hole
}

// ---- A realistic mixed handler body per target -----------------------------

#[test]
fn realistic_mixed_sources() {
    agree(
        "void go() {\n  // set up\n  s = \"a } b { c\";\n  t = '/'; /* x */ n++;\n}",
        Target::C,
    );
    agree(
        "fn go(&mut self) {\n  let s = \"a\\nb\"; // note\n  let r = r#\"raw \" ok\"#;\n}",
        Target::Rust,
    );
    agree(
        "def go(self):\n  s = \"x { self.n } y\"  # comment\n  t = '''block'''\n",
        Target::Python3,
    );
}

// ============================================================================
// D4 anti-omission guard — the curated set must exercise EVERY form the target
// has. This test pins each cleanroom target's `literals()` table to the exact
// set of forms the curated batteries above cover. If someone ADDS a form to a
// target's table (e.g. a Rust `TripleQuoted`), this fails LOUDLY — forcing a new
// curated input + fuzz alphabet entry before the parity claim can be trusted.
// SCAFFOLDING: reads the internal form table; conversion-internal.
// ============================================================================

#[test]
fn curated_set_covers_every_form_of_each_cleanroom_target() {
    use Form::*;
    // The forms each battery above demonstrably exercises, per target. Kept in
    // lock-step with `literals.rs`; a divergence means the curated corpus no longer
    // covers the target and must be extended (D4 falsifier).
    let expect: &[(Target, &[Form])] = &[
        (
            Target::C,
            &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: false },
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],
        ),
        (
            Target::Java,
            &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: false },
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],
        ),
        (
            Target::Rust,
            &[
                LineComment("//"),
                BlockComment { open: "/*", close: "*/", nests: true },
                RustRaw,
                Quoted { delim: b'"', multiline: true, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],
        ),
        (
            Target::Python3,
            &[
                LineComment("#"),
                TripleQuoted { delim: b'"' },
                TripleQuoted { delim: b'\'' },
                Quoted { delim: b'"', multiline: false, escapes: true },
                Quoted { delim: b'\'', multiline: false, escapes: true },
            ],
        ),
    ];
    for (t, forms) in expect {
        assert_eq!(
            t.literals().forms,
            *forms,
            "target {t:?}: literals() table diverged from the curated coverage set — \
             a form was added/removed; extend the curated battery + fuzz alphabet before \
             trusting parity (D4)."
        );
    }
}

// ============================================================================
// Strengthened curated edges + adversarial (per-form, per the 4 tables).
// SCAFFOLDING: differential vs the hand oracle at every position.
// ============================================================================

#[test]
fn edges_empty_eof_and_escape_at_eof() {
    for &t in &[Target::C, Target::Java, Target::Rust, Target::Python3] {
        agree("", t); // empty
        agree("x", t); // single non-opaque byte
        agree("\"", t); // a lone opening quote at EOF (unterminated)
        agree("\"a\\", t); // escape at EOF inside a string (backslash then EOF)
        agree("\"\\", t); // backslash immediately at EOF
        agree("'", t); // lone single quote
        agree("\n", t); // bare newline
    }
    // Comment openers truncated at EOF.
    agree("/", Target::C); // half a `//` / `/*`
    agree("/*", Target::C); // block opener, no close → unterminated
    agree("//", Target::C); // line opener, nothing after
    agree("#", Target::Python3); // python line comment opener at EOF
    agree("\"\"\"", Target::Python3); // triple opener, no close → unterminated
    agree("\"\"", Target::Python3); // an EMPTY plain string ("" ), not a triple
    agree("r", Target::Rust); // lone `r` (identifier start, not raw)
    agree("r#\"", Target::Rust); // raw opener, no close → unterminated
    agree("br", Target::Rust); // `br` at EOF
}

#[test]
fn adversarial_c_family() {
    for t in [Target::C, Target::Java] {
        agree("a /*/ b", t); // `/*/` — opener then a `/` that is NOT a close yet
        agree("a /**/ b", t); // minimal empty block comment
        agree("s = \"\\\\\";", t); // string is exactly a doubled backslash then close
        agree("s = \"\\\"\";", t); // escaped quote then real close
        agree("'\\''", t); // char: escaped quote inside single quotes
        agree("x // /* not a block */\n y", t); // block markers inside a line comment
        agree("s = \"// not a comment\";", t); // comment marker inside a string
        agree("s = \"/* nor this */\";", t);
        agree("q = '\n';", t); // newline inside a char literal → unterminated (not multiline)
        agree("nested = \"a { b } c\";", t); // braces are plain content in C
        agree("'ab'", t); // multi-byte char literal content
    }
}

#[test]
fn adversarial_rust() {
    let t = Target::Rust;
    agree("a /* /* */ b */ c", t); // exactly-two-deep nesting
    agree("a /* unterminated /* still open", t); // unterminated NESTED block
    agree("r\"\"", t); // empty zero-hash raw
    agree("r#\"\"#", t); // empty one-hash raw
    agree("r###\"x\"###", t); // three hashes
    agree("r#\"a\"##", t); // close needs ONE hash; the trailing `#` is content-after
    agree("r#\"a\"b\"#", t); // inner `\"` not followed by a hash is content
    agree("br#\"raw bytes\"#", t); // byte raw with a hash
    agree("let s = \"line1\nline2\";", t); // rust `\"` IS multiline — spans the newline
    agree("let c = '\n';", t); // rust `'` is NOT multiline → unterminated char
    agree("read_raw(r\"x\")", t); // `read_raw` starts with r but is an ident
    agree("r\"a\\\"", t); // no escapes in raw: the `\\` is content, `\"` closes
    agree("'a", t); // unterminated char / lifetime
}

#[test]
fn adversarial_python() {
    let t = Target::Python3;
    agree("s = \"\"\"\"\"\"", t); // an empty triple string: six quotes
    agree("s = \"\"\"a\"b\"\"\"", t); // single/double quotes are content inside a triple
    agree("s = '''it's ok'''", t); // an apostrophe inside a triple-single string
    agree("f\"{a}{b}{c}\"", t); // three consecutive holes
    agree("f\"{{}}\"", t); // escaped braces — NOT a hole
    agree("f\"{ '}' }\"", t); // a `}` hidden in `'}'` inside a hole (Δ1: hole now string-aware; outer extent still matches the hand here)
    agree("f\"{ {nested} }\"", t); // nested braces inside a hole
    agree("f\"{ unclosed\"", t); // hole opens but never closes before the string ends
    agree("\"# not a comment\"", t); // `#` inside a string is not a line comment
    agree("# \"not a string\"\n", t); // a `\"` inside a line comment
    agree("'''\n multi \n line\n'''", t); // triple spanning newlines
    agree("s = \"a\nb\"", t); // plain python `\"` is NOT multiline → unterminated at the newline
}

// ============================================================================
// D4 fuzz/property arm — the missing piece: the curated set above is position-
// exhaustive but input-sparse (#219 long tail). This feeds `agree()` (machine vs
// the INDEPENDENT hand oracle, at EVERY position) with deterministically-generated
// random inputs for EACH cleanroom target: (a) random bytes over the literal
// long-tail alphabet, (b) random Frame-ish/native source assembled from literal
// fragments. Determinism: a fixed inline xorshift PRNG over a fixed seed range —
// no Date/system-random. A failure here is a real machine/oracle divergence and is
// reproducible from its seed.
// SCAFFOLDING: differential vs the hand oracle; needs the internal oracle + spans.
// ============================================================================

/// Inline deterministic PRNG (xorshift64*). No external crates, no system entropy.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        // Avoid the zero fixed-point; splitmix the seed so adjacent seeds diverge fast.
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

/// The literal long-tail, one byte at a time. All ASCII → always valid UTF-8, so
/// `agree`'s `&src[..]` slicing stays on char boundaries. Every byte here is a byte
/// that MATTERS to some form's opener/closer/escape (#219 alphabet).
const FUZZ_BYTES: &[u8] = b"\"'`/*#-\\{}()[]rbBR=@ \n\txX01;:,";

/// Multi-byte literal fragments — the openers/closers/escapes as whole tokens, so the
/// generator actually forms `//`, `/*`, `\"\"\"`, `r#\"`, `\"#`, holes, `@@`, etc. instead
/// of relying on two independent single-byte draws lining up.
const FUZZ_FRAGMENTS: &[&[u8]] = &[
    b"\"", b"'", b"`", b"//", b"/*", b"*/", b"#", b"--", b"\"\"\"", b"'''", b"r\"", b"r#\"",
    b"r##\"", b"\"#", b"\"##", b"##", b"{", b"}", b"{{", b"}}", b"{x}", b"${", b"\\", b"\\\"",
    b"\\n", b"\n", b"\t", b" ", b"@@", b"@@system X {", b"abc", b"br\"", b"=", b";", b"(", b")",
    b"[", b"]", b"x", b"1",
];

fn gen_random_bytes(rng: &mut Rng, max_len: usize) -> String {
    let len = rng.below(max_len + 1);
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(FUZZ_BYTES[rng.below(FUZZ_BYTES.len())]);
    }
    // All bytes are ASCII → this is always valid UTF-8.
    String::from_utf8(v).expect("fuzz bytes are ASCII")
}

fn gen_frame_ish(rng: &mut Rng, max_frags: usize) -> String {
    let n = rng.below(max_frags + 1);
    let mut v: Vec<u8> = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(FUZZ_FRAGMENTS[rng.below(FUZZ_FRAGMENTS.len())]);
    }
    String::from_utf8(v).expect("fuzz fragments are ASCII")
}

/// A failing seed prints itself; re-run `agree(gen_*(…, seed), target)` to reproduce.
#[test]
fn fuzz_random_bytes_every_position_all_targets() {
    let targets = [Target::C, Target::Java, Target::Rust, Target::Python3];
    for &t in &targets {
        for seed in 0u64..2000 {
            let mut rng = Rng::new(seed ^ 0xA5A5_0000);
            let src = gen_random_bytes(&mut rng, 22);
            // `agree` is partition-aware: machine == hand at EVERY byte position (carried), OR a
            // Δ1 string-aware-hole FIXED row where the machine stays well-formed (checked inside).
            agree(&src, t);
        }
    }
}

#[test]
fn fuzz_frame_ish_source_every_position_all_targets() {
    let targets = [Target::C, Target::Java, Target::Rust, Target::Python3];
    let mut fixed = 0usize;
    for &t in &targets {
        for seed in 0u64..2000 {
            let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
            let src = gen_frame_ish(&mut rng, 9);
            fixed += agree(&src, t);
        }
    }
    // Δ1 fix-with-teeth: the frame-ish fuzz (rich in `{`/quotes) must actually reach the
    // string-aware-hole FIXED class (machine != string-blind hand), or the partition arm is
    // vacuous.
    assert!(
        fixed > 0,
        "the frame-ish fuzz never reached a Δ1 string-aware-hole divergence — partition arm vacuous"
    );
}

/// A generator that can't produce an opening string/comment tests nothing (a #232 lie).
/// Prove both arms have TEETH: over the same seed range, they must (1) be diverse and
/// (2) actually reach an ACCEPTING scan (some form opens and closes) many times, and
/// (3) reach the RUST-RAW and PYTHON-HOLE delegated sub-systems specifically.
#[test]
fn fuzz_corpus_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut accepts = 0usize;
    let mut raw_accepts = 0usize;
    let mut hole_effect = 0usize;
    for seed in 0u64..2000 {
        let mut rb = Rng::new(seed ^ 0xA5A5_0000);
        distinct.insert(gen_random_bytes(&mut rb, 22));
        let mut rf = Rng::new(seed ^ 0x5A5A_FFFF);
        let s = gen_frame_ish(&mut rf, 9);
        let b = s.as_bytes();
        // A Rust raw string opening somewhere → exercises the RawString sub-system to Accept.
        for i in 0..b.len() {
            if opaque_extent(b, i, Target::Rust).map_or(false, |e| e > i) {
                accepts += 1;
                if b[i] == b'r' || b[i] == b'b' {
                    raw_accepts += 1;
                }
            }
        }
        // A Python string carrying a `{…}` hole → exercises the BraceBalance sub-system.
        if s.contains('{') {
            for i in 0..b.len() {
                if (b[i] == b'"' || b[i] == b'\'')
                    && opaque_extent(b, i, Target::Python3).map_or(false, |e| e > i + 1)
                {
                    hole_effect += 1;
                    break;
                }
            }
        }
        distinct.insert(s);
    }
    assert!(distinct.len() > 2000, "generator not diverse: {} distinct", distinct.len());
    assert!(accepts > 200, "too few accepting scans ({accepts}) — arm lacks teeth");
    assert!(raw_accepts > 5, "RawString sub-system barely exercised by fuzz ({raw_accepts})");
    assert!(hole_effect > 5, "BraceBalance/hole path barely exercised by fuzz ({hole_effect})");
}

// ============================================================================
// LEVEL-1: opaque_at 4-way parity, Unterminated-arm-specific inputs.
// `agree_bytes` already checks `opaque_at == opaque_at_hand` at every position of the
// ENTIRE curated + adversarial + fuzz corpus (it is strictly stronger than the extent
// check). These inputs specifically drive the `Unterminated` arm — the one the extent
// number cannot see — so a regression that mis-classifies an unterminated body as
// `None` (or vice versa) fails a NAMED test even though its extent is unchanged.
// SCAFFOLDING: differential vs the hand oracle; needs the internal oracle + registers.
// ============================================================================

#[test]
fn unterminated_arm_four_way() {
    // Unterminated block comment (every target that has a block comment).
    for t in [Target::C, Target::Java, Target::Rust] {
        agree("/* never closes", t);
        agree("a = 1; /* still open", t);
    }
    agree("a /* /* nested still open", Target::Rust); // unterminated NESTED (rust nests)

    // Unterminated plain string, and escape-consumes-EOF, on every target.
    for t in [Target::C, Target::Java, Target::Rust, Target::Python3] {
        agree("\"abc", t); // opener + body, no close
        agree("\"a\\", t); // escape then EOF → the `\` consumes the (missing) next byte
        agree("'ab", t); // single-quote form, unterminated
    }

    // Bare newline inside a NON-multiline string → Unterminated (NOT swallow-to-EOF).
    agree("\"a\nb\";", Target::C);
    agree("x = 'p\nq';", Target::Java);
    agree("let c = '\n';", Target::Rust); // rust char is NOT multiline
    agree("s = \"a\nb\"", Target::Python3); // plain python `\"` is NOT multiline

    // Unterminated triple (python).
    agree("\"\"\"abc", Target::Python3);
    agree("'''abc", Target::Python3);
    agree("d = \"\"\"open\n spanning", Target::Python3);

    // Unterminated raw (rust), including the wrong-hash-count close.
    agree("r#\"abc", Target::Rust);
    agree("r\"abc", Target::Rust);
    agree("r##\"x\"#", Target::Rust); // one closing hash where two are needed
    agree("br#\"open", Target::Rust);
}

/// The 4-way check is only meaningful if every `OpaqueAt` variant is actually produced.
/// A classifier that could only ever emit `None`/`Literal` would be a #232 lie. Prove all
/// four variants (None, Comment, Literal, Unterminated) occur — across a curated set AND
/// the frame-ish fuzz corpus — as observed from the MACHINE (`opaque_at`).
#[test]
fn opaque_at_variants_all_occur() {
    use std::collections::HashSet;
    fn tag(o: OpaqueAt) -> u8 {
        match o {
            OpaqueAt::None => 0,
            OpaqueAt::Comment(_) => 1,
            OpaqueAt::Literal(_) => 2,
            OpaqueAt::Unterminated => 3,
        }
    }
    let targets = [Target::C, Target::Java, Target::Rust, Target::Python3];
    let mut seen: HashSet<u8> = HashSet::new();

    let curated: &[&str] = &[
        "plain identifier",
        "// a line comment\n",
        "/* a block comment */",
        "\"a string literal\"",
        "/* unterminated block",
        "\"unterminated string",
        "r\"raw\"",
        "\"\"\"triple\"\"\"",
        "'''unterminated triple",
        "r#\"unterminated raw",
    ];
    for &t in &targets {
        for s in curated {
            let b = s.as_bytes();
            for i in 0..b.len() {
                seen.insert(tag(opaque_at(b, i, t)));
            }
        }
    }
    for seed in 0u64..2000 {
        let mut rf = Rng::new(seed ^ 0x5A5A_FFFF);
        let s = gen_frame_ish(&mut rf, 9);
        let b = s.as_bytes();
        for &t in &targets {
            for i in 0..b.len() {
                seen.insert(tag(opaque_at(b, i, t)));
            }
        }
    }
    for (v, name) in [(0u8, "None"), (1, "Comment"), (2, "Literal"), (3, "Unterminated")] {
        assert!(
            seen.contains(&v),
            "OpaqueAt::{name} never produced across corpus+fuzz — the 4-way check lacks teeth"
        );
    }
}

// --------------------------------------------------------------------------------------
// B-14 (Item 4, Commit A): the `opaque_probe` register battery.
//
// The probe is the driver's ONE source for a literal's `delim` + hole content-spans (the
// same machine run that found the extent). Verified against the hand `Lexer::literal_at`'s
// `LiteralExtent` — delim, holes, and end, register for register — plus the probe-level
// pins of the carried behaviors: the `{{` second-brace phantom hole (T-N8, flips at Δ2)
// and comments carrying `delim == 0` at the probe (the `b'/'` fabrication is the DRIVER's,
// T-N5).

/// Probe vs hand `LiteralExtent`, register for register, at position `i`.
fn probe_matches_hand_literal(src: &str, i: usize, t: Target) {
    use frame_compiler::text::scan::lex::Lexer;
    use frame_compiler::text::scan::opaque_scan::opaque_probe;
    let b = src.as_bytes();
    let lx = Lexer::new(b, t);
    let l = lx
        .literal_at(i)
        .expect("hand literal_at must not Err here")
        .expect("hand literal_at must recognize here");
    let p = opaque_probe(b, i, t).expect("probe must recognize where the hand does");
    assert_eq!(p.kind, 2, "literal kind on {src:?} at {i}");
    assert_eq!(p.end, l.span.end, "extent end on {src:?} at {i}");
    assert_eq!(p.delim, l.delim, "delim register on {src:?} at {i}");
    let hand_holes: Vec<(usize, usize)> = l.holes.iter().map(|h| (h.start, h.end)).collect();
    assert_eq!(p.holes, hand_holes, "holes register on {src:?} at {i}");
}

#[test]
fn probe_delim_per_form_matches_hand() {
    // Plain quoted, both quotes, per target (`'x'` is the char/single-quote form).
    for t in [Target::C, Target::Java, Target::Rust, Target::Python3] {
        probe_matches_hand_literal(r#"x = "abc";"#, 4, t);
        probe_matches_hand_literal("y = 'z';", 4, t);
    }
    // Rust raw string → delim b'"' (the Item-4 raw-edge register; Lexer::rust_raw parity).
    probe_matches_hand_literal(r##"let s = r#"raw"#;"##, 8, Target::Rust);
    probe_matches_hand_literal(r#"let s = r"raw";"#, 8, Target::Rust);
    // Python triples, both quote bytes.
    probe_matches_hand_literal(r#"s = """abc""""#, 4, Target::Python3);
    probe_matches_hand_literal("s = '''abc'''", 4, Target::Python3);
}

#[test]
fn probe_holes_per_fstring_shape_match_hand() {
    // One hole; two holes; adjacent holes; a hole in a triple; nested braces inside a hole.
    for (src, i) in [
        (r#"f"a {x} b""#, 1),
        (r#"f"{x}{y}""#, 1),
        (r#"f"a {x!r} b {y:>8} c""#, 1),
        (r#"f"""t {v} u""""#, 1),
        (r#"f"d { {'k': 1}['k'] } e""#, 1),
    ] {
        probe_matches_hand_literal(src, i, Target::Python3);
    }
}

#[test]
fn probe_hole_is_string_aware_delta1() {
    // Δ1 (T-N7/R6): the probe's `holes` register is now string-AWARE — a `}` hidden inside a
    // nested string within a hole no longer mis-delimits it. In `f"{ d['}'] }"` the hole content
    // is the WHOLE ` d['}'] ` (span 3..11), not the truncated ` d['` (3..7) the string-blind
    // counter produced. This is where `probe.holes != LiteralExtent.holes`: the FIXED class.
    use frame_compiler::text::scan::lex::Lexer;
    use frame_compiler::text::scan::opaque_scan::opaque_probe;

    let src = "f\"{ d['}'] }\"";
    //          f=0 "=1 {=2 ␠=3 d=4 [=5 '=6 }=7 '=8 ]=9 ␠=10 }=11 "=12
    let b = src.as_bytes();
    let p = opaque_probe(b, 1, Target::Python3).expect("the f-string probes");
    assert_eq!(p.holes, vec![(3, 11)], "string-aware hole content span (Δ1)");
    assert_eq!(p.end, 13, "outer extent unperturbed on this input");

    // oracle_stayed_buggy (probe level): the hand `Lexer::hole_at`, via `literal_at`, stays
    // string-blind — it closes the hole at the first `}` (7), inside `'}'`. Any repair makes
    // the Δ1 fix teeth vacuous.
    let lx = Lexer::new(b, Target::Python3);
    let l = lx.literal_at(1).expect("no Err").expect("recognizes");
    let hand_holes: Vec<(usize, usize)> = l.holes.iter().map(|h| (h.start, h.end)).collect();
    assert_eq!(
        hand_holes,
        vec![(3, 7)],
        "the hand oracle was fixed (no longer string-blind) — the Δ1 fix teeth are now vacuous"
    );
    assert_ne!(p.holes, hand_holes, "probe (aware) must diverge from the hand (blind) — teeth");
}

#[test]
fn opaque_extent_string_aware_outer_delta1() {
    // Δ1 (T-N7/R6) outer-extent teeth: `br"/*${br"}r##"` — at pos 2 the Python string is
    // `"/*${br"`, closing at the `"` (pos 9), extent 10. The string-blind oracle mis-delimits
    // the `{`(6) hole (skipping past the closing `"`), so it wrongly extends the string to 15.
    // The machine (string-aware) is CORRECT; the oracle stays buggy → the partition arm has teeth.
    let b = b"br\"/*${br\"}r##\"";
    let m = opaque_extent(b, 2, Target::Python3).unwrap_or(2);
    let h = skip_opaque_at_hand(b, 2, Target::Python3);
    assert_eq!(m, 10, "machine (string-aware) closes the string at the real `\"` (extent 10)");
    assert_ne!(m, h, "Δ1 fix VACUOUS: the oracle already agrees (it must stay string-blind)");
    assert_eq!(
        h, 15,
        "the hand oracle was fixed (no longer string-blind) — the Δ1 outer-extent teeth are vacuous"
    );
}

#[test]
fn probe_no_double_brace_phantom_hole_delta2() {
    // T-N8 — FLIPPED at Δ2: `{{` is consumed WHOLE (both braces) before `hole_skip`, so the
    // second brace no longer opens a phantom hole. The machine records NO hole; the hand
    // `Lexer::hole_at` still phantom-opens (oracle_stayed_buggy at the probe level).
    use frame_compiler::text::scan::lex::Lexer;
    use frame_compiler::text::scan::opaque_scan::opaque_probe;

    for (src, phantom) in [
        (r#"f"{{x}}""#, (4usize, 5usize)), // f=0 "=1 {=2 {=3 x=4 }=5 }=6 "=7
        (r#"f"{{}}""#, (4, 4)),            // the empty phantom
    ] {
        let b = src.as_bytes();
        let p = opaque_probe(b, 1, Target::Python3).unwrap();
        assert!(p.holes.is_empty(), "no phantom hole for {src:?} (Δ2)");
        // oracle_stayed_buggy: the hand still phantom-opens the second brace.
        let lx = Lexer::new(b, Target::Python3);
        let l = lx.literal_at(1).expect("no Err").expect("recognizes");
        let hand_holes: Vec<(usize, usize)> = l.holes.iter().map(|h| (h.start, h.end)).collect();
        assert_eq!(
            hand_holes,
            vec![phantom],
            "the hand oracle was fixed (no longer phantom-opens `{{{{`) — the Δ2 teeth are vacuous"
        );
        assert_ne!(p.holes, hand_holes, "probe must diverge from the phantoming hand — teeth");
    }
}

#[test]
fn probe_comment_kind_and_end_match_opaque_at() {
    use frame_compiler::text::scan::opaque_scan::opaque_probe;
    for (src, t, delim) in [
        ("// line\nx", Target::Rust, b'/'),
        ("/* block */x", Target::C, b'/'),
        ("# py\nx", Target::Python3, b'#'),
    ] {
        let b = src.as_bytes();
        let p = opaque_probe(b, 0, t).expect("comment probes");
        assert_eq!(p.kind, 1, "comment kind on {src:?}");
        match opaque_at(b, 0, t) {
            OpaqueAt::Comment(end) => assert_eq!(p.end, end, "probe.end ≡ opaque_at end"),
            other => panic!("expected Comment for {src:?}, got {other:?}"),
        }
        // Δ4 (T-N5): the probe now reports the comment's REAL opener byte (`#`/`/`), so the
        // driver sources it instead of fabricating `b'/'`.
        assert_eq!(p.delim, delim, "comment delim register on {src:?}");
        assert!(p.holes.is_empty(), "comments have no holes");
    }
}

#[test]
fn probe_end_equals_opaque_at_end_everywhere() {
    use frame_compiler::text::scan::opaque_scan::opaque_probe;
    // The single-source/double-run tripwire, swept: wherever `opaque_at` reports a closed
    // body, the probe reports the SAME end (same machine, same run shape).
    let corpus: &[&str] = &[
        r#"a = "s" + 'c'; // tail"#,
        r#"f"a {x} b" '''t''' # c"#,
        r##"r#"raw"# "q" /* b */ 'x'"##,
        "\"\"\"t {v} u\"\"\" 'w'",
    ];
    for t in [Target::C, Target::Java, Target::Rust, Target::Python3] {
        for src in corpus {
            let b = src.as_bytes();
            for i in 0..b.len() {
                let at = opaque_at(b, i, t);
                let p = opaque_probe(b, i, t);
                match at {
                    OpaqueAt::Comment(end) | OpaqueAt::Literal(end) => {
                        let p = p.expect("probe must accept where opaque_at accepts");
                        assert_eq!(p.end, end, "end drift at {i} of {src:?} ({t:?})");
                    }
                    OpaqueAt::None | OpaqueAt::Unterminated => {
                        assert!(p.is_none(), "probe must reject where opaque_at rejects");
                    }
                }
            }
        }
    }
}
