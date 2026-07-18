//! **The machine-section state walk, as a system, agrees with the hand walk — proven by
//! running.** SCAFFOLDING (differential vs the retired hand oracle + the internal `Source`/
//! `segment` entry and tree spans; conversion-internal — never promoted; needs `@@[scan(u8)]`-
//! on-`@@system`, a cleanroom-only capability today, plus the hand oracle it is racing).
//!
//! `machine_walk::state_starts` is generated from `machine_walk.frs`, a `@@[scan(u8)]` Frame
//! system that now DRIVES `machine::machine_section`. This proves — by running — that the
//! `$Name` state-start offsets it accumulates match the pre-conversion hand loop
//! (`state_starts_hand`, kept ONLY as the differential oracle) at EVERY (`from`, `limit`)
//! position, for all four cleanroom targets, over realistic machine bodies AND adversarial
//! ones (`$Name` inside a string/comment; `$.x`/`$>`/`<$` heads; nested `{}`; adjacent states;
//! unterminated body; empty span) AND a deterministic frame-ish fuzz corpus.
//!
//! The two implementations share the SAME leaves (`skip_opaque`, `state_extent`) exactly as the
//! segmenter's `hand_item_starts` shares `item_end_at`, so what the differential proves is the
//! WALK — the thing being converted. A MISMATCH here is a real machine/oracle divergence and is
//! reproducible from its printed inputs (or seed).

use frame_compiler::text::scan::literals::Target;
use frame_compiler::text::scan::machine_walk::{state_starts, state_starts_hand};

const TARGETS: [Target; 4] = [Target::C, Target::Java, Target::Rust, Target::Python3];

/// The differential: the system and the retired hand oracle must return the byte-identical
/// `Vec<usize>` of state-start offsets for these exact `(from, limit)` arguments.
fn agree(bytes: &[u8], from: usize, limit: usize, target: Target) {
    let machine = state_starts(bytes, from, limit, target);
    let hand = state_starts_hand(bytes, from, limit, target);
    assert_eq!(
        machine, hand,
        "MISMATCH target {target:?} from={from} limit={limit} on {:?}:\n  machine={machine:?}\n  hand  ={hand:?}",
        String::from_utf8_lossy(bytes),
    );
    // A partition sanity check that holds independently of the oracle: the recorded starts are
    // strictly increasing and every one is a real index in [from, limit).
    let mut prev: Option<usize> = None;
    for &s in &machine {
        assert!(s >= from && s < limit, "start {s} out of [{from},{limit}) on {:?}", String::from_utf8_lossy(bytes));
        if let Some(p) = prev {
            assert!(s > p, "starts not strictly increasing ({p} then {s}) on {:?}", String::from_utf8_lossy(bytes));
        }
        prev = Some(s);
        assert_eq!(bytes[s], b'$', "a recorded start must point at a `$` byte");
    }
}

/// Every-position sweep: agree for EVERY `from` in `0..=len` and EVERY `limit` in `from..=len`.
/// This is exhaustive over the whole `(from, limit)` rectangle, so the required mid-span `from`,
/// mid-state-body `limit`, and mid-opaque `limit` are all covered by construction — not spot
/// checks.
fn sweep_all_positions(src: &str) {
    let b = src.as_bytes();
    let len = b.len();
    for target in TARGETS {
        for from in 0..=len {
            for limit in from..=len {
                agree(b, from, limit, target);
            }
        }
    }
}

// ===========================================================================
// The curated corpus. Each string is a `machine:`-body-shaped span. Enumerated
// from `machine_walk.frs` (the forms the walk recognizes) and `machine.rs`'s
// `state_extent`/`is_state_start` (what counts as a `$Name` start): a start is
// `$` + [A-Za-z_]; a state's whole body (its `{ … }`, nesting balanced) is
// skipped; opaque regions are skipped whole.
// ===========================================================================

/// Realistic, well-formed bodies — the shapes a real `machine:` section contains.
const REALISTIC: &[&str] = &[
    // single state
    "$A { go() { } }",
    // multiple states, trivia between
    "$A { e() { } }\n$B { e() { } }\n$C { }",
    // params and `=> $Parent`
    "$A(x: int) { }\n$B(y) => $A { }\n$C => $B { }",
    // nested `{}` in a handler body — the inner braces must not close the state early
    "$A {\n    go() {\n        if x { y } else { z }\n    }\n}\n$B { }",
    // adjacent states, NO trivia between them
    "$A {}$B {}$C {}",
    // a realistic multi-state machine with handlers, transitions, nested blocks
    "$Begin {\n    |>| { start() -> $Run }\n}\n$Run {\n    tick() {\n        if done() { -> $End }\n    }\n}\n$End { }",
];

/// Adversarial bodies — the long tail the walk must get right.
const ADVERSARIAL: &[&str] = &[
    // empty span
    "",
    // whitespace only
    "   \n\t ",
    // a `$Name` buried in a `//` line comment (C/Java/Rust) — NOT a state
    "$A { }\n// $Fake { should not count }\n$B { }",
    // a `$Name` buried in a `#` line comment (Python) — NOT a state
    "$A { }\n# $Fake { should not count }\n$B { }",
    // a `$Name` buried in a block comment (C/Java/Rust) — NOT a state
    "$A { } /* $Fake { } and $Also { } */ $B { }",
    // a `$Name` inside a string literal — NOT a state
    "$A { s = \"$Fake { } $Nope { }\" }\n$B { }",
    // `$.stateVar` — `$.` is not a state start
    "$A { x = $.count; y = $.total }\n$B { }",
    // `$>` / `<$` handler-head-ish sequences — `$` followed by `>` / preceded by `<` is not a start
    "$A { $> foo() <$ bar() $.v }\n$B { }",
    // a `$` at end-of-buffer, and `$` followed by a digit / punctuation (not an identifier start)
    "$A { }\n$1 $. $+ $",
    // nested braces AND an opaque region carrying braces inside a body
    "$A {\n    go() {\n        s = \"a } b { c\"; // } { comment\n    }\n}\n$B { }",
    // unterminated body — the last state never closes; `state_extent` runs to `limit`
    "$A { }\n$B { go() { unterminated",
    // an unterminated string carrying `$Name{` — must swallow to limit, no phantom state
    "$A { }\n$B { s = \"open $Fake {",
    // a bare `$` alone, and `$_underscore` (underscore IS an identifier start)
    "$_hidden { }\n$ { }\n$x { }",
    // only a `$Name` INSIDE opaque — zero real states
    "// $OnlyInComment { }\n/* $AlsoHidden { } */",
    // a `$Name` inside opaque, python flavored
    "# $OnlyInComment { }\n\"$InString { }\"",
];

#[test]
fn realistic_bodies_agree_every_position() {
    for src in REALISTIC {
        sweep_all_positions(src);
    }
}

#[test]
fn adversarial_bodies_agree_every_position() {
    for src in ADVERSARIAL {
        sweep_all_positions(src);
    }
}

// ===========================================================================
// Teeth — the corpus is non-trivial. A differential over inputs that all yield
// the empty vector proves nothing (the #232 lie). Assert, by RUNNING the system
// (not the oracle), that the corpus spans the outcome space: some input yields
// >=2 top-level starts; some input with a `$Name` only inside opaque yields 0.
// SCAFFOLDING.
// ===========================================================================

#[test]
fn corpus_has_teeth() {
    let mut max_starts = 0usize;

    for src in REALISTIC.iter().chain(ADVERSARIAL.iter()) {
        let b = src.as_bytes();
        for target in TARGETS {
            let starts = state_starts(b, 0, b.len(), target);
            max_starts = max_starts.max(starts.len());
        }
    }

    // A `$Name` that exists ONLY inside opaque must yield zero top-level states, on the
    // target whose comment/string syntax actually swallows it.
    let only_c_comment = "// $OnlyInComment { }\n/* $AlsoHidden { } */";
    assert_eq!(
        state_starts(only_c_comment.as_bytes(), 0, only_c_comment.len(), Target::C).len(),
        0,
        "a `$Name` only inside C comments must produce ZERO states"
    );
    let only_py = "# $OnlyInComment { }\n\"$InString { }\"";
    assert_eq!(
        state_starts(only_py.as_bytes(), 0, only_py.len(), Target::Python3).len(),
        0,
        "a `$Name` only inside a `#` comment / string (python) must produce ZERO states"
    );
    assert!(max_starts >= 2, "no corpus input yields >=2 state starts — the differential is toothless (max={max_starts})");
}

/// A focused self-contained spec: KNOWN state-start counts for hand-verified inputs, per the
/// target whose opaque syntax applies. This survives the oracle's eventual retirement — it
/// asserts the extent directly, not by comparison. (The offsets themselves are position-
/// dependent and left to the differential; the COUNT is the language-level fact.)
#[test]
fn known_state_counts_self_contained() {
    // three real top-level states, string/comment `$Name`s buried, `$.x` not a state
    let src = "$A { x = $.n }\n$B { s = \"$Fake {}\" }\n$C { } // $Nope {}";
    // For C/Java/Rust the `//` line comment hides `$Nope`; the string hides `$Fake`.
    for t in [Target::C, Target::Java, Target::Rust] {
        assert_eq!(
            state_starts(src.as_bytes(), 0, src.len(), t).len(),
            3,
            "expected exactly $A,$B,$C for {t:?}"
        );
    }
    // adjacent states, no trivia: four states.
    let adj = "$A {}$B {}$C {}$D {}";
    for t in TARGETS {
        assert_eq!(state_starts(adj.as_bytes(), 0, adj.len(), t).len(), 4, "four adjacent states for {t:?}");
    }
    // nested braces inside a single state body: exactly one top-level state.
    let nested = "$A { go() { if x { y { z } } } }";
    for t in TARGETS {
        assert_eq!(state_starts(nested.as_bytes(), 0, nested.len(), t).len(), 1, "nested braces => one state for {t:?}");
    }
    // `$.`, `$>`, `<$`, `$` + digit, bare `$` are NOT state starts: only $A and $B here.
    let noise = "$A { $.v $> <$ $1 $+ $ }\n$B { }";
    for t in TARGETS {
        assert_eq!(state_starts(noise.as_bytes(), 0, noise.len(), t).len(), 2, "only $A,$B are starts for {t:?}");
    }
}

// ===========================================================================
// Deterministic fuzz arm. Assemble frame-ish `machine:` bodies from `$Name{…}`
// fragments + comments/strings/noise, draw random `from`/`limit`, run the
// differential for all 4 targets. Determinism: inline xorshift64* over a fixed
// seed range — no Date/system-random. A divergence panics with the source and
// arguments and reproduces from its seed.
// SCAFFOLDING.
// ===========================================================================

/// Inline deterministic PRNG (xorshift64*). Mirrors the opaque_scan/delim_balance prior art.
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

/// Whole-token fragments so the generator actually forms real states, real openers/closers, and
/// real opaque regions carrying decoy `$Name`s — instead of relying on single bytes lining up.
const FRAGMENTS: &[&str] = &[
    "$A ", "$B ", "$Cc ", "$_s ", "$Name(x: int) ", "$D(y) => $A ", "=> $P ",
    "{", "}", "{}", "{ go() { } }", "{ if x { y } }",
    " ", "\n", "\t",
    "// $Decoy {}\n", "# $Decoy {}\n", "/* $Decoy {} */", "\"$InStr {}\"", "'$InStr {}'",
    "$.field", "$>", "<$", "$1", "$+", "$", "x", ";", "( )", "-> $Q",
];

fn gen_frame_ish(rng: &mut Rng, max_frags: usize) -> String {
    let n = rng.below(max_frags + 1);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
    }
    s
}

#[test]
fn fuzz_frame_ish_every_position_all_targets() {
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let src = gen_frame_ish(&mut rng, 12);
        let b = src.as_bytes();
        let len = b.len();
        // Random from/limit (clamped so from <= limit <= len), PLUS the full span, for each target.
        let from = if len == 0 { 0 } else { rng.below(len + 1) };
        let limit = if len == 0 { 0 } else { from + rng.below(len - from + 1) };
        for target in TARGETS {
            agree(b, from, limit, target); // random window
            agree(b, 0, len, target); // full span
        }
    }
}

/// The fuzz generator must reach BOTH empty and non-empty results — a generator that only ever
/// produces the empty vector (or only ever finds states) tests half the machine. Assert, by
/// running the system, that both outcomes occur many times over the seed range, and that the
/// corpus is diverse (not one string repeated).
#[test]
fn fuzz_has_teeth() {
    use std::collections::HashSet;
    let mut distinct = HashSet::new();
    let mut empty_results = 0usize;
    let mut nonempty_results = 0usize;
    let mut multi_state_results = 0usize;
    for seed in 0u64..3000 {
        let mut rng = Rng::new(seed ^ 0x5A5A_FFFF);
        let src = gen_frame_ish(&mut rng, 12);
        let b = src.as_bytes();
        distinct.insert(src.clone());
        for target in TARGETS {
            let starts = state_starts(b, 0, b.len(), target);
            if starts.is_empty() {
                empty_results += 1;
            } else {
                nonempty_results += 1;
            }
            if starts.len() >= 2 {
                multi_state_results += 1;
            }
        }
    }
    assert!(distinct.len() > 1500, "fuzz generator not diverse: {} distinct", distinct.len());
    assert!(empty_results > 0, "fuzz never produced an EMPTY result — the zero-state path is untested");
    assert!(nonempty_results > 100, "fuzz produced too few NON-empty results ({nonempty_results}) — lacks teeth");
    assert!(multi_state_results > 50, "fuzz produced too few multi-state results ({multi_state_results})");
}

// ===========================================================================
// I1 byte-partition through the REAL pipeline. Drive `segment()` on full `.frm`
// files whose `machine:` sections ARE the realistic bodies above, then assert the
// tree covers every byte. `check_coverage` is the top-level partition; `unparse`
// round-trip is the constructive form; `check_total` recurses INTO the machine
// section's `State`/`Trivia` nodes (built by the native driver over `state_starts`),
// so a broken state partition surfaces as a `Gap`/`Overlap` — never silently.
// An `UndecomposedBlob` (un-parsed handler body) is EXPECTED and tolerated; a
// Gap/Overlap is a real bug and fails the test.
// SCAFFOLDING (real pipeline + internal tree entry; conversion-internal).
// ===========================================================================

fn wrap_system(machine_body: &str) -> String {
    // A complete, well-formed system whose `machine:` section is `machine_body`.
    format!(
        "@@system S {{\n    interface:\n        go()\n    machine:\n{}\n}}\n",
        machine_body
    )
}

/// Well-formed machine bodies suitable for a full parse (the adversarial UNTERMINATED ones are
/// excluded here — they intentionally do not close and are covered by the differential instead).
const WELL_FORMED_MACHINES: &[&str] = &[
    "        $A { go() { } }",
    "        $A { go() { } }\n        $B { go() { } }\n        $C { go() { } }",
    "        $A(x: int) { go() { } }\n        $B(y) => $A { go() { } }",
    "        $A {\n            go() {\n                if x { y } else { z }\n            }\n        }\n        $B { go() { } }",
    "        $Begin {\n            go() { -> $End }\n        }\n        $End { go() { } }",
    // opaque decoys in a handler body: a string and a comment carrying buried `$Fake`/`$Nope`.
    // (Kept brace-BALANCED so the file parses on every target, incl. Python where `//` is not a
    // comment; the imbalanced-inside-opaque case is covered by the differential above.)
    "        $A {\n            go() {\n                s = \"$Fake { } $Nope { }\"; // $Decoy { }\n            }\n        }\n        $B { go() { } }",
];

#[test]
fn real_pipeline_partition_covers_every_byte() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{check_total, Defect, Node};
    use frame_compiler::Source;

    let mut checked = 0usize;
    for target in [Target::C, Target::Rust, Target::Python3] {
        for body in WELL_FORMED_MACHINES {
            let text = wrap_system(body);
            let bytes = text.as_bytes().to_vec();
            let src = Source::new("machine_walk_partition.frm", bytes.clone())
                .expect("utf8 source");
            let ast = segment(&src, target).expect("segment should succeed");

            // I1 top-level: items partition [0, len).
            ast.check_coverage()
                .unwrap_or_else(|d| panic!("check_coverage failed for {target:?} on:\n{text}\n  => {d}"));

            // I1 constructive: byte-identical round-trip.
            let rebuilt = ast.unparse(&bytes);
            assert_eq!(
                rebuilt, bytes,
                "unparse != source for {target:?} on:\n{text}"
            );

            // I1 RECURSIVE: traverse into the machine section's State/Trivia nodes. A broken
            // state partition (a start off by a byte, an overlap, a dropped trivia gap) is a
            // Gap/Overlap here. An UndecomposedBlob is the still-unparsed handler body — expected.
            match check_total(&ast as &dyn Node) {
                Ok(()) => {}
                Err(Defect::UndecomposedBlob { .. }) => {}
                Err(d) => panic!(
                    "recursive partition BROKEN for {target:?} on:\n{text}\n  => {d}"
                ),
            }
            checked += 1;
        }
    }
    assert!(checked >= WELL_FORMED_MACHINES.len() * 3, "expected every body x target to be checked");
}

/// A milestone-validation test: the machine section actually DECOMPOSES into the right number of
/// `State` nodes end-to-end through `segment()`, and a `$Name` buried in a string in a handler
/// body does NOT spawn a phantom state. This is the observable outcome of the `MachineWalk`
/// system driving `machine_section` — a regression fails THIS named test.
#[test]
fn machine_decomposes_into_the_right_states() {
    use frame_compiler::scan::segment;
    use frame_compiler::tree::{Item, MachineMember, Section};
    use frame_compiler::Source;

    let text = wrap_system(
        "        $A {\n            go() {\n                s = \"$Fake { } $Nope { }\";\n            }\n        }\n        $B { go() { } }\n        $C { go() { } }",
    );
    let src = Source::new("machine_walk_states.frm", text.as_bytes().to_vec()).unwrap();
    let ast = segment(&src, Target::Rust).unwrap();

    let sys = ast
        .items
        .iter()
        .find_map(|i| match i {
            Item::System(s) => Some(s),
            _ => None,
        })
        .expect("a system");

    let machine = sys
        .sections
        .iter()
        .find_map(|sec| match sec {
            Section::Machine(m) => Some(m),
            _ => None,
        })
        .expect("a machine section");

    let state_names: Vec<&str> = machine
        .members
        .iter()
        .filter_map(|m| match m {
            MachineMember::State(s) => Some(s.name.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(
        state_names,
        vec!["A", "B", "C"],
        "exactly three states; the `$Fake`/`$Nope` inside the string are NOT states"
    );
}
