//! **OpaqueScan's standalone invariants — proven by running.**
//!
//! `opaque_scan::opaque_extent` / `opaque_at` / `opaque_probe` are generated from
//! `opaque_scan.frs` (a `@@[scan(u8)]` Frame system) and compute string/comment extents +
//! classification + the driver's `delim`/hole registers, for all four cleanroom targets, at every
//! byte position. This battery proves the standalone invariants (no hand oracle): the curated
//! coverage of every FORM, that all four `OpaqueAt` variants actually occur, that the fuzz corpus
//! reaches accepting scans and the delegated RawString / BraceBalance sub-systems, that the probe
//! and `opaque_at` never drift, and the directed Δ1/Δ2/Δ4 register pins (string-aware holes, no
//! phantom `{{` hole, the real comment opener byte).
//!
//! The position-exhaustive differential against the retired hand recognizer lives in
//! `machine.rs::skip_opaque_tests` (deleted with its oracle in a later stage); this file is the
//! system's own standalone spec. SCAFFOLDING (white-box on the internal systems).

use frame_compiler::text::scan::literals::{Form, Target};
use frame_compiler::text::scan::opaque_scan::{opaque_at, opaque_extent, OpaqueAt};

// ============================================================================
// D4 anti-omission guard — the curated set must exercise EVERY form the target
// has. This test pins each cleanroom target's `literals()` table to the exact
// set of forms the batteries cover. If someone ADDS a form to a target's table
// (e.g. a Rust `TripleQuoted`), this fails LOUDLY — forcing a new curated input +
// fuzz alphabet entry before the coverage claim can be trusted.
// SCAFFOLDING: reads the internal form table; conversion-internal.
// ============================================================================

#[test]
fn curated_set_covers_every_form_of_each_cleanroom_target() {
    use Form::*;
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
             trusting coverage (D4)."
        );
    }
}

// ============================================================================
// Deterministic PRNG + fuzz alphabets (no Date/system-random). Reused by the
// teeth + variant-coverage tests.
// ============================================================================

/// Inline deterministic PRNG (xorshift64*). No external crates, no system entropy.
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

/// The literal long-tail, one byte at a time. All ASCII → always valid UTF-8. Every byte here is
/// a byte that MATTERS to some form's opener/closer/escape (#219 alphabet).
const FUZZ_BYTES: &[u8] = b"\"'`/*#-\\{}()[]rbBR=@ \n\txX01;:,";

/// Multi-byte literal fragments — the openers/closers/escapes as whole tokens, so the generator
/// actually forms `//`, `/*`, `\"\"\"`, `r#\"`, `\"#`, holes, `@@`, etc.
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

/// A generator that can't produce an opening string/comment tests nothing (a #232 lie).
/// Prove both arms have TEETH: over the seed range they must (1) be diverse and (2) actually
/// reach an ACCEPTING scan (some form opens and closes) many times, and (3) reach the RUST-RAW
/// and PYTHON-HOLE delegated sub-systems specifically.
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

/// The 4-way classification is only meaningful if every `OpaqueAt` variant is actually produced.
/// A classifier that could only ever emit `None`/`Literal` would be a #232 lie. Prove all four
/// variants (None, Comment, Literal, Unterminated) occur — across a curated set AND the frame-ish
/// fuzz corpus — as observed from the MACHINE (`opaque_at`).
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
            "OpaqueAt::{name} never produced across corpus+fuzz — the classification lacks teeth"
        );
    }
}

// ============================================================================
// The `opaque_probe` register battery (Item 4). The probe is the driver's ONE source for a
// literal's `delim` + hole content-spans. These pin the machine-visible register behaviors
// (Δ1 string-aware holes, Δ2 no phantom `{{` hole, Δ4 the real comment opener byte) directly,
// self-contained — no hand oracle.
// ============================================================================

/// Δ1 (T-N7/R6): the probe's `holes` register is string-AWARE — a `}` hidden inside a nested
/// string within a hole no longer mis-delimits it. In `f"{ d['}'] }"` the hole content is the
/// WHOLE ` d['}'] ` (span 3..11), not the truncated ` d['`, and the outer extent is unperturbed.
#[test]
fn probe_hole_is_string_aware_delta1() {
    use frame_compiler::text::scan::opaque_scan::opaque_probe;
    let src = "f\"{ d['}'] }\"";
    //          f=0 "=1 {=2 ␠=3 d=4 [=5 '=6 }=7 '=8 ]=9 ␠=10 }=11 "=12
    let b = src.as_bytes();
    let p = opaque_probe(b, 1, Target::Python3).expect("the f-string probes");
    assert_eq!(p.holes, vec![(3, 11)], "string-aware hole content span (Δ1)");
    assert_eq!(p.end, 13, "outer extent unperturbed on this input");
}

/// Δ1 outer-extent: `br"/*${br"}r##"` — at pos 2 the Python string is `"/*${br"`, closing at the
/// `"` (pos 9), extent 10. A string-blind counter would mis-delimit the `{`(6) hole and wrongly
/// extend to 15; the machine (string-aware) is CORRECT at 10.
#[test]
fn opaque_extent_string_aware_outer_delta1() {
    let b = b"br\"/*${br\"}r##\"";
    let m = opaque_extent(b, 2, Target::Python3).unwrap_or(2);
    assert_eq!(m, 10, "machine (string-aware) closes the string at the real `\"` (extent 10)");
}

/// Δ2 (T-N8): `{{` is consumed WHOLE (both braces) before `hole_skip`, so the second brace no
/// longer opens a phantom hole. Escaped braces are content, not interpolation — the machine
/// records NO hole.
#[test]
fn probe_no_double_brace_phantom_hole_delta2() {
    use frame_compiler::text::scan::opaque_scan::opaque_probe;
    for src in [r#"f"{{x}}""#, r#"f"{{}}""#] {
        let b = src.as_bytes();
        let p = opaque_probe(b, 1, Target::Python3).unwrap();
        assert!(p.holes.is_empty(), "no phantom hole for {src:?} (Δ2)");
    }
}

/// Δ4 (T-N5): the probe reports the comment's REAL opener byte (`#`/`/`), so the driver sources
/// it instead of fabricating `b'/'`. Already machine-only (probe vs `opaque_at`, same run).
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
        assert_eq!(p.delim, delim, "comment delim register on {src:?}");
        assert!(p.holes.is_empty(), "comments have no holes");
    }
}

/// The single-source/double-run tripwire, swept: wherever `opaque_at` reports a closed body, the
/// probe reports the SAME end (same machine, same run shape); wherever it rejects, the probe
/// rejects. Machine-only.
#[test]
fn probe_end_equals_opaque_at_end_everywhere() {
    use frame_compiler::text::scan::opaque_scan::opaque_probe;
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
